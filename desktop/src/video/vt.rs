//! VideoToolbox ownership and plane copies. All session calls run on the
//! movie's worker, behind its deadline. Callback state outlives invalidate;
//! callbacks copy into a bounded queue and never touch the game or renderer.

use objc2_core_foundation::{
    CFBoolean, CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType,
};
use objc2_core_media::*;
use objc2_core_video::*;
use objc2_video_toolbox::*;
use std::{
    collections::VecDeque,
    ffi::c_void,
    ptr::{self, NonNull},
    sync::Mutex,
};

const OUTPUTS: usize = 32;
const OUTPUT_BYTES: usize = 128 * 1024 * 1024;

use super::native::Output;
struct Queue {
    frames: VecDeque<Output>,
    bytes: usize,
    error: Option<String>,
}
struct Callback {
    queue: Mutex<Queue>,
    width: usize,
    height: usize,
    generation: u32,
    full_range: bool,
}

pub(super) struct Session {
    session: CFRetained<VTDecompressionSession>,
    format: CFRetained<CMFormatDescription>,
    callback: Box<Callback>,
}

fn status(code: i32, what: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("VideoToolbox {what}: {code}"))
    }
}

fn parameter_format(
    sets: &[Vec<u8>],
    hevc: bool,
    raw: &mut *const CMFormatDescription,
) -> Result<(), String> {
    let mut pointers: Vec<NonNull<u8>> = sets
        .iter()
        .map(|s| NonNull::new(s.as_ptr().cast_mut()).ok_or("empty parameter set"))
        .collect::<Result<_, _>>()?;
    let mut sizes: Vec<usize> = sets.iter().map(Vec::len).collect();
    if pointers.is_empty() || sizes.contains(&0) {
        return Err("empty parameter sets".into());
    }
    // SAFETY: the arrays and NAL bytes remain live for the call. CoreMedia
    // copies them; raw is an aligned pointer receiving one owned reference.
    unsafe {
        let pointers = NonNull::new(pointers.as_mut_ptr()).unwrap();
        let sizes = NonNull::new(sizes.as_mut_ptr()).unwrap();
        status(
            if hevc {
                CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                    None,
                    sets.len(),
                    pointers,
                    sizes,
                    4,
                    None,
                    NonNull::from(raw),
                )
            } else {
                CMVideoFormatDescriptionCreateFromH264ParameterSets(
                    None,
                    sets.len(),
                    pointers,
                    sizes,
                    4,
                    NonNull::from(raw),
                )
            },
            "format",
        )
    }
}

impl Session {
    pub fn new(
        codec: &super::codec::Format,
        generation: u32,
        _period: f64,
    ) -> Result<Self, String> {
        let (width, height, full_range) = (codec.width(), codec.height(), codec.full_range());
        let mut raw = ptr::null();
        match codec {
            super::codec::Format::H264(f) => parameter_format(&f.sets, false, &mut raw)?,
            super::codec::Format::Hevc(f) => parameter_format(&f.sets, true, &mut raw)?,
            super::codec::Format::Vp9(f) => {
                let data = CFData::from_bytes(&f.configuration());
                let key = CFString::from_str("vpcC");
                let atoms = CFDictionary::from_slices(&[&*key], &[&*data]);
                // SAFETY: exported CF key, retained property-list values and
                // an aligned out pointer. CoreMedia copies the atom payload.
                unsafe {
                    VTRegisterSupplementalVideoDecoderIfAvailable(u32::from_be_bytes(*b"vp09"));
                    let extensions = CFDictionary::from_slices(
                        &[kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms],
                        &[&*atoms],
                    );
                    status(
                        CMVideoFormatDescriptionCreate(
                            None,
                            u32::from_be_bytes(*b"vp09"),
                            width as i32,
                            height as i32,
                            Some(extensions.as_ref()),
                            NonNull::from(&mut raw),
                        ),
                        "format",
                    )?;
                }
            }
        }
        // SAFETY: a successful Create returns one owned reference.
        let format =
            unsafe { CFRetained::from_raw(NonNull::new(raw.cast_mut()).ok_or("null format")?) };
        let callback = Box::new(Callback {
            queue: Mutex::new(Queue {
                frames: VecDeque::with_capacity(OUTPUTS),
                bytes: 0,
                error: None,
            }),
            width: width as usize,
            height: height as usize,
            generation,
            full_range,
        });
        let rec = VTDecompressionOutputCallbackRecord {
            decompressionOutputCallback: Some(output),
            decompressionOutputRefCon: (&*callback as *const Callback).cast_mut().cast(),
        };
        let mut raw = ptr::null_mut();
        // SAFETY: Apple's exported keys have the indicated CF types; the
        // dictionaries retain their values. Session retains its format and
        // attributes; our callback Box stays put until after invalidation.
        unsafe {
            let spec = CFDictionary::from_slices(
                &[kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder],
                &[CFBoolean::new(true)],
            );
            let pf = CFNumber::new_i32(i32::from_be_bytes(if full_range {
                *b"420f"
            } else {
                *b"420v"
            }));
            let attrs = CFDictionary::from_slices(&[kCVPixelBufferPixelFormatTypeKey], &[&*pf]);
            status(
                VTDecompressionSession::create(
                    None,
                    &format,
                    Some(spec.as_ref()),
                    Some(attrs.as_ref()),
                    &rec,
                    NonNull::from(&mut raw),
                ),
                "session",
            )?;
        }
        // SAFETY: successful Create returns an owned session reference.
        let session = unsafe { CFRetained::from_raw(NonNull::new(raw).ok_or("null session")?) };
        let s = Self {
            session,
            format,
            callback,
        };
        let mut raw: *mut CFType = ptr::null_mut();
        // SAFETY: a decompression session is a VTSession; CopyProperty
        // writes an owned CF object to the correctly aligned out pointer.
        unsafe {
            status(
                VTSessionCopyProperty(
                    &s.session,
                    kVTDecompressionPropertyKey_UsingHardwareAcceleratedVideoDecoder,
                    None,
                    (&mut raw as *mut *mut CFType).cast(),
                ),
                "hardware property",
            )?;
            let value = CFRetained::from_raw(NonNull::new(raw).ok_or("no hardware property")?);
            if !value
                .downcast::<CFBoolean>()
                .map_err(|_| "invalid hardware property")?
                .as_bool()
            {
                return Err("VideoToolbox did not use hardware".into());
            }
        }
        Ok(s)
    }

    pub fn decode(
        &self,
        bytes: &[u8],
        id: usize,
        _presentation: Option<f64>,
    ) -> Result<(), String> {
        let mut raw = ptr::null_mut();
        // SAFETY: CoreMedia allocates its own data block. Bytes are copied
        // into it before the sample is submitted; VT never borrows Rust data.
        unsafe {
            status(
                CMBlockBuffer::create_with_memory_block(
                    None,
                    ptr::null_mut(),
                    bytes.len(),
                    None,
                    ptr::null(),
                    0,
                    bytes.len(),
                    1,
                    NonNull::from(&mut raw),
                ),
                "block buffer",
            )?;
            let block = CFRetained::from_raw(NonNull::new(raw).ok_or("null block")?);
            status(
                CMBlockBuffer::replace_data_bytes(
                    NonNull::new(bytes.as_ptr().cast_mut().cast()).ok_or("empty packet")?,
                    &block,
                    0,
                    bytes.len(),
                ),
                "packet copy",
            )?;
            let timing = CMSampleTimingInfo {
                duration: kCMTimeInvalid,
                presentationTimeStamp: CMTime::new(id as i64, 1000),
                decodeTimeStamp: kCMTimeInvalid,
            };
            let size = bytes.len();
            let mut raw = ptr::null_mut();
            status(
                CMSampleBuffer::create_ready(
                    None,
                    Some(&block),
                    Some(&self.format),
                    1,
                    1,
                    &timing,
                    1,
                    &size,
                    NonNull::from(&mut raw),
                ),
                "sample",
            )?;
            let sample = CFRetained::from_raw(NonNull::new(raw).ok_or("null sample")?);
            let mut flags = VTDecodeInfoFlags(0);
            status(
                self.session.decode_frame(
                    &sample,
                    VTDecodeFrameFlags(0),
                    ptr::with_exposed_provenance_mut(id + 1),
                    &mut flags,
                ),
                "decode",
            )?;
            if flags.contains(VTDecodeInfoFlags::FrameDropped) {
                return Err("VideoToolbox dropped a frame".into());
            }
        }
        Ok(())
    }

    pub fn finish(&self) -> Result<(), String> {
        // SAFETY: live session, only used by its owning worker.
        unsafe {
            status(self.session.finish_delayed_frames(), "drain")?;
            status(self.session.wait_for_asynchronous_frames(), "wait")
        }
    }

    pub fn take(&self) -> Result<Vec<Output>, String> {
        let mut q = self
            .callback
            .queue
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(e) = q.error.take() {
            return Err(e);
        }
        q.bytes = 0;
        Ok(q.frames.drain(..).collect())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: the callback Box remains alive through both operations.
        // A stuck OS call holds only this worker, which the controller can
        // abandon; it never frees state still reachable by an OS callback.
        unsafe {
            self.session.wait_for_asynchronous_frames();
            self.session.invalidate();
        }
    }
}

unsafe extern "C-unwind" fn output(
    context: *mut c_void,
    frame: *mut c_void,
    code: i32,
    flags: VTDecodeInfoFlags,
    image: *mut CVImageBuffer,
    _pts: CMTime,
    _duration: CMTime,
) {
    // SAFETY: supplied by our session; lives until invalidate completes.
    let state = unsafe { &*context.cast::<Callback>() };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        status(code, "output")?;
        if flags.contains(VTDecodeInfoFlags::FrameDropped) {
            return Err("VideoToolbox dropped a frame".into());
        }
        let image = NonNull::new(image).ok_or("VideoToolbox returned no pixel buffer")?;
        // SAFETY: the callback owns a live image reference for this call;
        // retain while reading, release before returning. No native handles
        // escape the callback or cross to the renderer.
        let image = unsafe { CFRetained::retain(image) };
        let mut q = state.queue.lock().unwrap_or_else(|p| p.into_inner());
        let size =
            state.width * state.height + 2 * state.width.div_ceil(2) * state.height.div_ceil(2);
        if q.frames.len() >= OUTPUTS || q.bytes + size > OUTPUT_BYTES {
            return Err("VideoToolbox output queue exceeded its bound".into());
        }
        let planes = copy(&image, state.width, state.height, state.full_range)?;
        let id = (frame as usize)
            .checked_sub(1)
            .ok_or("invalid frame identity")?;
        q.frames.push_back(Output {
            id,
            generation: state.generation,
            planes,
        });
        q.bytes += size;
        Ok::<_, String>(())
    }))
    .unwrap_or_else(|_| Err("panic while copying a VideoToolbox frame".into()));
    if let Err(e) = result {
        state
            .queue
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .error
            .get_or_insert(e);
    }
}

type RowWriter<'a> = dyn FnMut(&[u8], usize, &mut [u8]) + 'a;

fn copy(
    pb: &CVPixelBuffer,
    width: usize,
    height: usize,
    full_range: bool,
) -> Result<Vec<u8>, String> {
    // Refuse scaling/cropping and unexpected plane layouts. Matching dimensions
    // are required by this first capability gate, rather than guessing a rect.
    let pf = CVPixelBufferGetPixelFormatType(pb);
    if pf != u32::from_be_bytes(if full_range { *b"420f" } else { *b"420v" })
        || CVPixelBufferGetWidth(pb) != width
        || CVPixelBufferGetHeight(pb) != height
        || CVPixelBufferGetPlaneCount(pb) != 2
    {
        return Err("unsupported VideoToolbox pixel layout".into());
    }
    // SAFETY: the buffer is retained. Base addresses are used only while
    // locked and after checking each plane's width, height and row stride.
    unsafe {
        status(
            CVPixelBufferLockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly),
            "lock pixels",
        )?;
    }
    struct Unlock<'a>(&'a CVPixelBuffer);
    impl Drop for Unlock<'_> {
        fn drop(&mut self) {
            unsafe {
                CVPixelBufferUnlockBaseAddress(self.0, CVPixelBufferLockFlags::ReadOnly);
            }
        }
    }
    let _unlock = Unlock(pb);
    let cw = width.div_ceil(2);
    let ch = height.div_ceil(2);
    let mut out = vec![0; width * height + 2 * cw * ch];
    let mut rows = |plane, w, h, write: &mut RowWriter<'_>| -> Result<(), String> {
        let stride = CVPixelBufferGetBytesPerRowOfPlane(pb, plane);
        let base = CVPixelBufferGetBaseAddressOfPlane(pb, plane).cast::<u8>();
        let plane_width = CVPixelBufferGetWidthOfPlane(pb, plane) * if plane == 1 { 2 } else { 1 };
        if base.is_null()
            || stride < w
            || plane_width < w
            || CVPixelBufferGetHeightOfPlane(pb, plane) < h
        {
            return Err("short VideoToolbox plane".into());
        }
        for y in 0..h {
            // SAFETY: checked dimensions/stride and locked buffer above.
            let row = unsafe { std::slice::from_raw_parts(base.add(y * stride), w) };
            write(row, y, &mut out);
        }
        Ok(())
    };
    rows(0, width, height, &mut |row, y, dst| {
        dst[y * width..(y + 1) * width].copy_from_slice(row)
    })?;
    rows(1, cw * 2, ch, &mut |row, y, dst| {
        for x in 0..cw {
            dst[width * height + y * cw + x] = row[2 * x];
            dst[width * height + cw * ch + y * cw + x] = row[2 * x + 1];
        }
    })?;
    Ok(out)
}
