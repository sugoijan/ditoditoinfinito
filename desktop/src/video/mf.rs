//! Experimental Windows Media Foundation decoder. COM, MFT calls, D3D11
//! copies and teardown stay on the bounded native worker. No MF callbacks
//! borrow Rust state: asynchronous transforms are polled through GetEvent.
//! Auto never enters this module until Windows fidelity evidence exists.
use super::{
    codec::Format,
    mf_core::{self, Flow, Ids, Signal},
    native::Output,
};
use std::{
    collections::VecDeque,
    mem::ManuallyDrop,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{E_NOTIMPL, HMODULE},
        Graphics::{
            Direct3D::{
                D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1,
                D3D_FEATURE_LEVEL_11_0,
            },
            Direct3D10::ID3D10Multithread,
            Direct3D11::*,
            Dxgi::{Common::DXGI_FORMAT_NV12, IDXGIAdapter},
        },
        Media::MediaFoundation::*,
        System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
    },
    core::{GUID, Interface},
};
mod api;
mod surface;
const WAIT: Duration = Duration::from_secs(2);
const QUEUE: usize = 32;
const BYTES: usize = 128 * 1024 * 1024;
fn w<T>(result: windows::core::Result<T>, what: &str) -> Result<T, String> {
    result.map_err(|e| format!("Media Foundation {what}: {e}"))
}
struct Runtime {
    api: api::Api,
}
impl Runtime {
    fn new() -> Result<Self, String> {
        let api = api::Api::load()?;
        // SAFETY: a fresh decoder worker owns the COM apartment and the
        // matching MF/COM shutdowns; no interface is sent to another thread.
        unsafe {
            w(
                CoInitializeEx(None, COINIT_MULTITHREADED).ok(),
                "COM startup",
            )?;
            if let Err(e) = api.start() {
                CoUninitialize();
                return Err(e);
            }
        }
        Ok(Self { api })
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // SAFETY: all MFT/D3D fields are declared before Runtime, hence released
        // before these balanced thread-owned shutdowns.
        unsafe {
            self.api.stop();
            CoUninitialize();
        }
    }
}
struct Active {
    activation: IMFActivate,
    transform: IMFTransform,
}
impl Drop for Active {
    fn drop(&mut self) {
        // SAFETY: activation and transform belong to this worker. Shutdown also
        // runs here on unsuccessful negotiation and on abandoned-worker teardown.
        unsafe {
            let _ = self.activation.ShutdownObject();
        }
    }
}
struct Activations {
    ptr: *mut Option<IMFActivate>,
    len: usize,
}
impl Drop for Activations {
    fn drop(&mut self) {
        // SAFETY: MFTEnumEx's successful result is a CoTaskMem array of exactly
        // len owned interface pointers. Release each before freeing the array.
        unsafe {
            if !self.ptr.is_null() {
                for a in std::slice::from_raw_parts_mut(self.ptr, self.len) {
                    drop(a.take());
                }
                CoTaskMemFree(Some(self.ptr.cast()));
            }
        }
    }
}
pub(super) struct Session {
    active: Active,
    events: Option<IMFMediaEventGenerator>,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    _manager: IMFDXGIDeviceManager,
    staging: Option<(u32, u32, ID3D11Texture2D)>,
    input: u32,
    output: u32,
    width: u32,
    height: u32,
    full_range: bool,
    bt709: bool,
    generation: u32,
    duration: i64,
    sets: Vec<Vec<u8>>,
    first: bool,
    ids: Ids,
    flow: Flow,
    ready: VecDeque<Output>,
    ready_bytes: usize,
    _runtime: Rc<Runtime>,
}
impl Session {
    pub fn new(format: &Format, generation: u32, period: f64) -> Result<Self, String> {
        if !format.width().is_multiple_of(2) || !format.height().is_multiple_of(2) {
            return Err("experimental MF requires even picture dimensions".into());
        }
        let duration = (period * 10_000_000.0).round();
        if !duration.is_finite() || duration < 1.0 || duration >= i64::MAX as f64 {
            return Err("MF invalid frame duration".into());
        }
        let runtime = Rc::new(Runtime::new()?);
        // SAFETY: out pointers are aligned Options; all resources remain on
        // this worker and hardware is requested (no WARP fallback).
        let (device, context, manager) = unsafe {
            let (mut device, mut context) = (None, None);
            w(
                D3D11CreateDevice(
                    None::<&IDXGIAdapter>,
                    D3D_DRIVER_TYPE_HARDWARE,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&[
                        D3D_FEATURE_LEVEL_11_0,
                        D3D_FEATURE_LEVEL_10_1,
                        D3D_FEATURE_LEVEL_10_0,
                    ]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                ),
                "D3D11 device",
            )?;
            let device = device.ok_or("MF null D3D11 device")?;
            let context = context.ok_or("MF null D3D11 context")?;
            let multithread: ID3D10Multithread = w(device.cast(), "D3D multithread interface")?;
            let _ = multithread.SetMultithreadProtected(true);
            let (manager, token) = runtime.api.manager()?;
            w(manager.ResetDevice(&device, token), "DXGI reset")?;
            (device, context, manager)
        };
        let subtype = match format {
            Format::H264(_) => MFVideoFormat_H264,
            Format::Hevc(_) => MFVideoFormat_HEVC,
            Format::Vp9(_) => MFVideoFormat_VP90,
        };
        let input = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: subtype,
        };
        let output = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_NV12,
        };
        let list = runtime.api.enumerate(&input, &output)?;
        if list.len == 0 || list.ptr.is_null() {
            return Err("MF no matching NV12 decoder".into());
        }
        if list.len > 256 {
            return Err("MF activation list exceeds bound".into());
        }
        let mut last = "no D3D11-aware decoder".to_string();
        // SAFETY: count and array ownership come from successful MFTEnumEx.
        let entries = unsafe { std::slice::from_raw_parts(list.ptr, list.len) };
        for activation in entries.iter().take(16).flatten() {
            match Self::configure(activation, format, &manager, subtype, &runtime.api) {
                Ok((active, events, input, output)) => {
                    let sets = match format {
                        Format::H264(f) => f.sets.clone(),
                        Format::Hevc(f) => f.sets.clone(),
                        Format::Vp9(_) => Vec::new(),
                    };
                    let mut session = Self {
                        active,
                        events,
                        device: device.clone(),
                        context: context.clone(),
                        _manager: manager.clone(),
                        staging: None,
                        input,
                        output,
                        width: format.width(),
                        height: format.height(),
                        full_range: format.full_range(),
                        bt709: format.matrix() == ddi_platform::video::YuvMatrix::Bt709,
                        generation,
                        duration: duration as i64,
                        sets,
                        first: true,
                        ids: Ids::default(),
                        flow: Flow::default(),
                        ready: VecDeque::new(),
                        ready_bytes: 0,
                        _runtime: runtime.clone(),
                    };
                    if let Err(e) = session.choose_output() {
                        last = e;
                        continue;
                    }
                    // SAFETY: negotiated stream ids and thread-owned transform.
                    unsafe {
                        w(
                            session
                                .active
                                .transform
                                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0),
                            "begin streaming",
                        )?;
                        w(
                            session
                                .active
                                .transform
                                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0),
                            "start stream",
                        )?;
                    }
                    return Ok(session);
                }
                Err(e) => last = e,
            }
        }
        Err(format!("MF decoder negotiation refused: {last}"))
    }
    fn configure(
        a: &IMFActivate,
        f: &Format,
        manager: &IMFDXGIDeviceManager,
        subtype: GUID,
        api: &api::Api,
    ) -> Result<(Active, Option<IMFMediaEventGenerator>, u32, u32), String> {
        // SAFETY: activation, attributes, media types and transform are all
        // held by this worker. Types copy their blobs; no Rust data is borrowed.
        unsafe {
            let transform: IMFTransform = w(a.ActivateObject(), "activation")?;
            let active = Active {
                activation: a.clone(),
                transform,
            };
            let t = &active.transform;
            let attrs = w(t.GetAttributes(), "attributes")?;
            if attrs.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0) != 1 {
                return Err("MF decoder is not D3D11-aware".into());
            }
            let asynchronous = attrs.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) == 1;
            let events = if asynchronous {
                w(
                    attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1),
                    "async unlock",
                )?;
                Some(w(t.cast(), "event generator")?)
            } else {
                None
            };
            let (mut ins, mut outs) = (0, 0);
            w(t.GetStreamCount(&mut ins, &mut outs), "stream count")?;
            if ins != 1 || outs != 1 {
                return Err("MF requires one input and output stream".into());
            }
            let (mut input, mut output) = ([0], [0]);
            if let Err(e) = t.GetStreamIDs(&mut input, &mut output)
                && e.code() != E_NOTIMPL
            {
                return Err(format!("MF stream ids: {e}"));
            }
            w(
                t.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize),
                "set D3D manager",
            )?;
            let ty = api.media_type()?;
            w(
                ty.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video),
                "input major type",
            )?;
            w(ty.SetGUID(&MF_MT_SUBTYPE, &subtype), "input codec")?;
            w(
                ty.SetUINT64(
                    &MF_MT_FRAME_SIZE,
                    (u64::from(f.width()) << 32) | u64::from(f.height()),
                ),
                "input size",
            )?;
            w(
                ty.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32),
                "progressive input",
            )?;
            let profile = match f {
                Format::H264(f) => u32::from(f.stream.sps.profile_idc),
                Format::Hevc(_) => 1,
                Format::Vp9(_) => 0,
            };
            w(ty.SetUINT32(&MF_MT_MPEG2_PROFILE, profile), "input profile")?;
            w(
                ty.SetUINT32(
                    &MF_MT_VIDEO_NOMINAL_RANGE,
                    if f.full_range() {
                        MFNominalRange_0_255.0 as u32
                    } else {
                        MFNominalRange_16_235.0 as u32
                    },
                ),
                "input range",
            )?;
            w(
                ty.SetUINT32(
                    &MF_MT_YUV_MATRIX,
                    if f.matrix() == ddi_platform::video::YuvMatrix::Bt709 {
                        MFVideoTransferMatrix_BT709.0 as u32
                    } else {
                        MFVideoTransferMatrix_BT601.0 as u32
                    },
                ),
                "input matrix",
            )?;
            let sets = match f {
                Format::H264(f) => &f.sets,
                Format::Hevc(f) => &f.sets,
                Format::Vp9(_) => &Vec::new(),
            };
            if !sets.is_empty() {
                let header = mf_core::annex_b(&[], sets, true)?;
                w(
                    ty.SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &header),
                    "sequence header",
                )?;
            }
            w(t.SetInputType(input[0], &ty, 0), "input negotiation")?;
            Ok((active, events, input[0], output[0]))
        }
    }
    fn choose_output(&mut self) -> Result<(), String> {
        // SAFETY: all returned media types and blobs are owned COM objects.
        // Output remains NV12 at the checked geometry; no colour conversion.
        unsafe {
            let t = &self.active.transform;
            let mut last = "no NV12 output".to_string();
            for n in 0..32 {
                let ty = match t.GetOutputAvailableType(self.output, n) {
                    Ok(t) => t,
                    Err(e) if e.code() == MF_E_NO_MORE_TYPES => break,
                    Err(e) => return Err(format!("MF output types: {e}")),
                };
                if ty.GetGUID(&MF_MT_SUBTYPE).ok() != Some(MFVideoFormat_NV12) {
                    continue;
                }
                if let Err(e) = self.geometry(&ty) {
                    last = e;
                    continue;
                }
                w(
                    ty.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, self.range()),
                    "output range",
                )?;
                w(
                    ty.SetUINT32(&MF_MT_YUV_MATRIX, self.matrix()),
                    "output matrix",
                )?;
                w(
                    ty.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32),
                    "output interlace",
                )?;
                if let Err(e) = t.SetOutputType(self.output, &ty, 0) {
                    last = format!("MF output type: {e}");
                    continue;
                }
                let actual = w(t.GetOutputCurrentType(self.output), "current output type")?;
                self.geometry(&actual)?;
                if actual.GetGUID(&MF_MT_SUBTYPE).ok() != Some(MFVideoFormat_NV12)
                    || actual
                        .GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE)
                        .ok()
                        .is_some_and(|n| n != self.range())
                    || actual
                        .GetUINT32(&MF_MT_YUV_MATRIX)
                        .ok()
                        .is_some_and(|n| n != self.matrix())
                {
                    return Err("MF negotiated format or colours changed".into());
                }
                let info = w(t.GetOutputStreamInfo(self.output), "output stream info")?;
                if info.dwFlags
                    & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32
                        | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32)
                    == 0
                {
                    return Err("MF requires decoder-owned DXGI samples".into());
                }
                return Ok(());
            }
            Err(last)
        }
    }
    fn geometry(&self, ty: &IMFMediaType) -> Result<(), String> {
        // SAFETY: typed attribute getters check the blob length before the
        // unaligned copy of the plain MFVideoArea structure.
        unsafe {
            let size = w(ty.GetUINT64(&MF_MT_FRAME_SIZE), "output geometry")?;
            let (width, height) = ((size >> 32) as u32, size as u32);
            if width < self.width || height < self.height || width > 4096 || height > 4096 {
                return Err("MF output geometry changed".into());
            }
            if ty
                .GetUINT32(&MF_MT_INTERLACE_MODE)
                .ok()
                .is_some_and(|n| n != MFVideoInterlace_Progressive.0 as u32)
            {
                return Err("MF output is not progressive".into());
            }
            let mut aperture = false;
            for key in [MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_GEOMETRIC_APERTURE] {
                if let Ok(n) = ty.GetBlobSize(&key) {
                    if n as usize != std::mem::size_of::<MFVideoArea>() {
                        return Err("MF invalid aperture blob".into());
                    }
                    let mut bytes = vec![0; n as usize];
                    w(ty.GetBlob(&key, &mut bytes, None), "aperture")?;
                    let area = std::ptr::read_unaligned(bytes.as_ptr().cast::<MFVideoArea>());
                    if area.OffsetX.value != 0
                        || area.OffsetX.fract != 0
                        || area.OffsetY.value != 0
                        || area.OffsetY.fract != 0
                        || area.Area.cx != self.width as i32
                        || area.Area.cy != self.height as i32
                    {
                        return Err("MF unsupported crop or aperture".into());
                    }
                    aperture = true;
                }
            }
            if !aperture && (width != self.width || height != self.height) {
                return Err("MF padded media type has no checked aperture".into());
            }
            if ty.GetUINT32(&MF_MT_PAN_SCAN_ENABLED).unwrap_or(0) != 0 {
                return Err("MF pan-scan unsupported".into());
            }
        }
        Ok(())
    }
    fn range(&self) -> u32 {
        if self.full_range {
            MFNominalRange_0_255.0 as u32
        } else {
            MFNominalRange_16_235.0 as u32
        }
    }
    fn matrix(&self) -> u32 {
        if self.bt709 {
            MFVideoTransferMatrix_BT709.0 as u32
        } else {
            MFVideoTransferMatrix_BT601.0 as u32
        }
    }
    pub fn decode(
        &mut self,
        bytes: &[u8],
        id: usize,
        presentation: Option<f64>,
    ) -> Result<(), String> {
        if self.flow.draining {
            return Err("MF input after drain".into());
        }
        let stamp = self.ids.insert(presentation, id)?;
        let data = if self.sets.is_empty() {
            bytes.to_vec()
        } else {
            mf_core::annex_b(bytes, &self.sets, self.first)?
        };
        let sample = self.sample(&data, stamp)?;
        if self.events.is_some() {
            let start = Instant::now();
            while self.flow.inputs == 0 {
                self.events_once(true, start)?;
            }
            self.flow.input()?;
        }
        // SAFETY: the sample owns its compressed bytes, and remains live for
        // ProcessInput. The MFT retains it if asynchronous work needs it.
        unsafe {
            let mut r = self.active.transform.ProcessInput(self.input, &sample, 0);
            if self.events.is_none() && r.as_ref().is_err_and(|e| e.code() == MF_E_NOTACCEPTING) {
                self.sync_outputs()?;
                r = self.active.transform.ProcessInput(self.input, &sample, 0);
            }
            w(r, "ProcessInput")?;
        }
        self.first = false;
        self.available()
    }
    fn sample(&self, data: &[u8], stamp: i64) -> Result<IMFSample, String> {
        if data.is_empty() || data.len() > 8 * 1024 * 1024 {
            return Err("MF input sample bound".into());
        }
        // SAFETY: MF owns the allocation; Lock's returned capacity is checked
        // before copying. The guard unlocks it on every error path.
        unsafe {
            let buffer = self._runtime.api.buffer(data.len() as u32)?;
            let (mut ptr, mut capacity) = (std::ptr::null_mut(), 0);
            w(
                buffer.Lock(&mut ptr, Some(&mut capacity), None),
                "input lock",
            )?;
            struct Unlock<'a>(&'a IMFMediaBuffer);
            impl Drop for Unlock<'_> {
                fn drop(&mut self) {
                    unsafe {
                        let _ = self.0.Unlock();
                    }
                }
            }
            let guard = Unlock(&buffer);
            if ptr.is_null() || (capacity as usize) < data.len() {
                return Err("MF invalid input buffer".into());
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            drop(guard);
            w(buffer.SetCurrentLength(data.len() as u32), "input length")?;
            let sample = self._runtime.api.sample()?;
            w(sample.AddBuffer(&buffer), "input sample buffer")?;
            w(sample.SetSampleTime(stamp), "input PTS")?;
            w(sample.SetSampleDuration(self.duration), "input duration")?;
            if self.first {
                w(
                    sample.SetUINT32(&MFSampleExtension_Discontinuity, 1),
                    "discontinuity",
                )?;
                w(
                    sample.SetUINT32(&MFSampleExtension_CleanPoint, 1),
                    "clean point",
                )?;
            }
            Ok(sample)
        }
    }
    fn available(&mut self) -> Result<(), String> {
        if self.events.is_some() {
            let start = Instant::now();
            for _ in 0..128 {
                if !self.events_once(false, start)? {
                    return Ok(());
                }
            }
            Err("MF event flood".into())
        } else {
            self.sync_outputs()
        }
    }
    fn events_once(&mut self, wait: bool, start: Instant) -> Result<bool, String> {
        loop {
            // SAFETY: nonblocking event polling on the owning thread; no
            // callback can outlive this Session or refer to Rust pointers.
            let event = unsafe {
                self.events
                    .as_ref()
                    .expect("async")
                    .GetEvent(MF_EVENT_FLAG_NO_WAIT)
            };
            match event {
                Ok(event) => {
                    unsafe {
                        w(w(event.GetStatus(), "event status")?.ok(), "event failure")?;
                    }
                    let kind = unsafe { w(event.GetType(), "event type")? };
                    if kind == METransformNeedInput.0 as u32 {
                        self.flow.event(Signal::Input)?;
                    } else if kind == METransformHaveOutput.0 as u32 {
                        self.flow.event(Signal::Output)?;
                    } else if kind == METransformDrainComplete.0 as u32 {
                        self.flow.event(Signal::Complete)?;
                    } else {
                        return Err(format!("MF unsupported transform event {kind}"));
                    }
                    while self.flow.outputs > 0 {
                        self.flow.outputs -= 1;
                        if !self.pull()? {
                            return Err("MF async output without a sample".into());
                        }
                    }
                    return Ok(true);
                }
                Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => {
                    if !wait {
                        return Ok(false);
                    }
                    if start.elapsed() > WAIT {
                        return Err("MF event wait exceeded two seconds".into());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(format!("MF event polling: {e}")),
            }
        }
    }
    fn sync_outputs(&mut self) -> Result<(), String> {
        for _ in 0..QUEUE {
            if !self.pull()? {
                return Ok(());
            }
        }
        Err("MF output burst exceeds bound".into())
    }
    fn pull(&mut self) -> Result<bool, String> {
        for _ in 0..4 {
            let mut out = OwnedOutput(MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: self.output,
                ..Default::default()
            });
            let mut status = 0;
            // SAFETY: one initialized output record, owned sample/event
            // pointers released by OwnedOutput even on error or renegotiation.
            let r = unsafe {
                self.active.transform.ProcessOutput(
                    0,
                    std::slice::from_mut(&mut out.0),
                    &mut status,
                )
            };
            match r {
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(false),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    self.choose_output()?;
                    continue;
                }
                Err(e) => return Err(format!("MF ProcessOutput: {e}")),
                Ok(()) => {}
            }
            if status != 0
                || out.0.dwStatus & !(MFT_OUTPUT_DATA_BUFFER_INCOMPLETE.0 as u32) != 0
                || out.0.pEvents.is_some()
            {
                return Err("MF unsupported output status/events".into());
            }
            let sample = out.0.pSample.as_ref().ok_or("MF output has no sample")?;
            let stamp = unsafe { w(sample.GetSampleTime(), "output PTS")? };
            let planes = self.readback(sample)?;
            let id = self.ids.output(stamp)?;
            if self.ready.len() >= QUEUE || self.ready_bytes.saturating_add(planes.len()) > BYTES {
                return Err("MF output queue exceeds bound".into());
            }
            self.ready_bytes += planes.len();
            self.ready.push_back(Output {
                id,
                generation: self.generation,
                pixels: super::native::Pixels::Planes(planes),
            });
            return Ok(true);
        }
        Err("MF repeated stream changes".into())
    }
    pub fn finish(&mut self) -> Result<(), String> {
        self.flow.draining = true;
        // SAFETY: one end/drain sequence, same worker and stream ids.
        unsafe {
            w(
                self.active
                    .transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, self.input as usize),
                "end stream",
            )?;
            w(
                self.active
                    .transform
                    .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0),
                "drain",
            )?;
        }
        if self.events.is_some() {
            let start = Instant::now();
            for _ in 0..128 {
                if self.flow.complete {
                    break;
                }
                self.events_once(true, start)?;
            }
            if !self.flow.complete {
                return Err("MF drain event bound".into());
            }
        } else {
            self.sync_outputs()?;
        }
        if !self.ids.empty() {
            return Err("MF drain omitted an input picture".into());
        }
        Ok(())
    }
    pub fn take(&mut self) -> Result<Vec<Output>, String> {
        self.available()?;
        self.ready_bytes = 0;
        Ok(self.ready.drain(..).collect())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: teardown remains in the worker even if the controller abandons
        // it. Active then shuts down its activation before Runtime is dropped.
        unsafe {
            let _ = self
                .active
                .transform
                .ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            let _ = self
                .active
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
        }
    }
}
struct OwnedOutput(MFT_OUTPUT_DATA_BUFFER);
impl Drop for OwnedOutput {
    fn drop(&mut self) {
        // SAFETY: ProcessOutput returns owned COM references in these two ABI
        // fields. ManuallyDrop suppresses the bindings' implicit drop, not ours.
        unsafe {
            ManuallyDrop::drop(&mut self.0.pSample);
            ManuallyDrop::drop(&mut self.0.pEvents);
        }
    }
}
