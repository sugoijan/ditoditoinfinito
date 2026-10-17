//! Opt-in macOS planes. Only a completed, immutable pixel buffer crosses the
//! decoder boundary. CVMetalTexture/cache ownership stays on the drawing
//! thread; command-buffer completion leases and HAL retirement tokens protect
//! it. Any unconfirmed teardown quarantines a bounded set and disables sharing.
use super::vt;
use objc2_core_foundation::CFRetained;
use objc2_core_video::*;
use objc2_metal::{MTLPixelFormat, MTLTextureType};
use std::{
    cell::RefCell,
    collections::HashMap,
    ptr::{self, NonNull},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

const LIMIT: usize = 4;
const BYTES: usize = 128 * 1024 * 1024;
struct Budget {
    frames: usize,
    bytes: usize,
    disabled: bool,
}
static BUDGET: Mutex<Budget> = Mutex::new(Budget {
    frames: 0,
    bytes: 0,
    disabled: false,
});
static IMPORTED: AtomicU64 = AtomicU64::new(0);
static COPIED: AtomicU64 = AtomicU64::new(0);
static WARNED: std::sync::Once = std::sync::Once::new();
fn disabled() -> bool {
    BUDGET.lock().unwrap_or_else(|p| p.into_inner()).disabled
}
fn disable() {
    BUDGET.lock().unwrap_or_else(|p| p.into_inner()).disabled = true;
}
pub(super) fn requested() -> bool {
    std::env::var_os("DDI_VIDEO_PLANES").is_some_and(|v| v == "metal") && !disabled()
}
#[derive(Clone, Copy, Debug)]
pub struct MetalStats {
    pub imported: u64,
    pub copied: u64,
    pub retained: usize,
    pub bytes: usize,
    pub disabled: bool,
}
pub fn stats() -> MetalStats {
    CACHES.with(|c| {
        for cache in c.borrow_mut().iter_mut() {
            let _ = cache.collect();
        }
    });
    let b = BUDGET.lock().unwrap_or_else(|p| p.into_inner());
    MetalStats {
        imported: IMPORTED.load(Ordering::Relaxed),
        copied: COPIED.load(Ordering::Relaxed),
        retained: b.frames,
        bytes: b.bytes,
        disabled: b.disabled,
    }
}
struct Permit(usize);
impl Permit {
    fn new(bytes: usize) -> Result<Self, String> {
        let mut b = BUDGET.lock().unwrap_or_else(|p| p.into_inner());
        if b.disabled || b.frames >= LIMIT || b.bytes.saturating_add(bytes) > BYTES {
            return Err("Metal surface budget or sharing disabled".into());
        }
        b.frames += 1;
        b.bytes += bytes;
        Ok(Self(bytes))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut b = BUDGET.lock().unwrap_or_else(|p| p.into_inner());
        b.frames -= 1;
        b.bytes -= self.0;
    }
}

/// This type cannot expose mutation of the completed decoder image.
pub(super) struct Surface {
    image: CFRetained<CVPixelBuffer>,
    width: usize,
    height: usize,
    full: bool,
    bytes: usize,
}
// SAFETY: construction is restricted to completed VT outputs. The decoder
// never changes a published image; this shell never mutates pixels/attachments.
// Retain/release and transfer of this immutable image are supported by CV.
// This asserts ownership transfer only, not concurrent access (no Sync impl).
unsafe impl Send for Surface {}
impl Surface {
    /// Caller must own a completed, immutable decoder image, with no subsequent
    /// pixel or attachment mutation through any alias. Retention prevents reuse.
    pub(super) unsafe fn completed(
        image: CFRetained<CVPixelBuffer>,
        width: usize,
        height: usize,
        full: bool,
    ) -> Result<Self, String> {
        vt::checked_layout(&image, width, height, full)?;
        let mut bytes = 0usize;
        for p in 0..2 {
            let n = CVPixelBufferGetBytesPerRowOfPlane(&image, p)
                .checked_mul(CVPixelBufferGetHeightOfPlane(&image, p))
                .ok_or("CV allocation overflow")?;
            bytes = bytes.checked_add(n).ok_or("CV allocation overflow")?;
        }
        bytes = bytes.max(CVPixelBufferGetDataSize(&image));
        if bytes == 0 || bytes > BYTES {
            return Err("CV retained allocation bound".into());
        }
        Ok(Self {
            image,
            width,
            height,
            full,
            bytes,
        })
    }
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }
    pub(super) fn copy(&self) -> Result<Vec<u8>, String> {
        vt::copy(&self.image, self.width, self.height, self.full)
    }
}
struct Owner {
    _image: CFRetained<CVPixelBuffer>,
    _views: [CFRetained<CVMetalTexture>; 2],
    readers: Arc<AtomicUsize>,
    retired: bool,
    _permit: Permit,
}
struct Reading(Vec<Arc<AtomicUsize>>);
impl Drop for Reading {
    fn drop(&mut self) {
        for r in &self.0 {
            r.fetch_sub(1, Ordering::AcqRel);
        }
    }
}
pub(super) struct Cache {
    device: wgpu::Device,
    cache: CFRetained<CVMetalTextureCache>,
    owners: HashMap<usize, Owner>,
    next: usize,
    retire: mpsc::Sender<usize>,
    retired: mpsc::Receiver<usize>,
    #[cfg(test)]
    force_unconfirmed: bool,
}
fn cv(code: i32, what: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("Core Video {what}: {code}"))
    }
}
// The generated binding omits this fixed-size-array SDK function.
unsafe extern "C-unwind" {
    fn CVMetalTextureGetCleanTexCoords(
        image: &CVMetalTexture,
        lower_left: *mut f32,
        lower_right: *mut f32,
        upper_right: *mut f32,
        upper_left: *mut f32,
    );
}
fn full_aperture(view: &CVMetalTexture) -> bool {
    let (mut ll, mut lr, mut ur, mut ul) = ([0f32; 2], [0f32; 2], [0f32; 2], [0f32; 2]);
    // SAFETY: live retained view and four disjoint, writable two-float arrays.
    unsafe {
        CVMetalTextureGetCleanTexCoords(
            view,
            ll.as_mut_ptr(),
            lr.as_mut_ptr(),
            ur.as_mut_ptr(),
            ul.as_mut_ptr(),
        );
    }
    [ll, lr, ur, ul]
        .into_iter()
        .flatten()
        .zip([0., 1., 1., 1., 1., 0., 0., 0.])
        .all(|(a, b)| a.is_finite() && (a - b).abs() < 1e-6)
}

impl Cache {
    pub(super) fn new(device: &wgpu::Device) -> Result<Self, String> {
        if disabled() {
            return Err("Metal sharing disabled".into());
        }
        // SAFETY: the cache uses this exact wgpu Metal device; no default-device
        // assumption. The HAL guard remains live through the creation call.
        let hal =
            unsafe { device.as_hal::<wgpu::hal::api::Metal>() }.ok_or("not a Metal device")?;
        let mut raw = ptr::null_mut();
        unsafe {
            cv(
                CVMetalTextureCache::create(
                    None,
                    None,
                    hal.raw_device(),
                    None,
                    NonNull::from(&mut raw),
                ),
                "texture cache",
            )?;
        }
        let cache = unsafe { CFRetained::from_raw(NonNull::new(raw).ok_or("null CV cache")?) };
        let (retire, retired) = mpsc::channel();
        Ok(Self {
            device: device.clone(),
            cache,
            owners: HashMap::new(),
            next: 0,
            retire,
            retired,
            #[cfg(test)]
            force_unconfirmed: false,
        })
    }
    pub(super) fn collect(&mut self) -> Result<(), String> {
        #[cfg(test)]
        if self.force_unconfirmed && !self.owners.is_empty() {
            disable();
            return Err("injected unconfirmed GPU completion".into());
        }
        // A lost device is not evidence of physical GPU completion. Keep its
        // references and disable further sharing rather than guess at reuse.
        if self.device.poll(wgpu::PollType::Poll).is_err() {
            if !self.owners.is_empty() {
                disable();
            }
            return Err("Metal device lost before confirmed retirement".into());
        }
        for id in self.retired.try_iter() {
            if let Some(o) = self.owners.get_mut(&id) {
                o.retired = true;
            }
        }
        self.owners
            .retain(|_, o| !o.retired || o.readers.load(Ordering::Acquire) != 0);
        // Evict unused cached views periodically, as Core Video requires;
        // live drawing-thread owners retain both views independently.
        self.cache.flush(0);
        Ok(())
    }
    pub(super) fn frame(&mut self, surface: &Surface) -> Result<[wgpu::Texture; 2], String> {
        self.collect()?;
        let permit = Permit::new(surface.bytes)?;
        let mut views = Vec::with_capacity(2);
        let mut raws = Vec::with_capacity(2);
        for p in 0..2 {
            let (f, w, h) = if p == 0 {
                (MTLPixelFormat::R8Unorm, surface.width, surface.height)
            } else {
                (
                    MTLPixelFormat::RG8Unorm,
                    surface.width / 2,
                    surface.height / 2,
                )
            };
            if CVPixelBufferGetWidthOfPlane(&surface.image, p) != w
                || CVPixelBufferGetHeightOfPlane(&surface.image, p) != h
            {
                return Err("unsupported CV plane aperture".into());
            }
            let mut raw = ptr::null_mut();
            // SAFETY: immutable retained NV12, checked extent, same-device cache.
            unsafe {
                cv(
                    CVMetalTextureCache::create_texture_from_image(
                        None,
                        &self.cache,
                        &surface.image,
                        None,
                        f,
                        w,
                        h,
                        p,
                        NonNull::from(&mut raw),
                    ),
                    "plane view",
                )?;
            }
            let view = unsafe { CFRetained::from_raw(NonNull::new(raw).ok_or("null CV view")?) };
            // Core Video calls an upper-left origin "flipped"; that is
            // precisely the renderer/Metal texture convention used here.
            if !CVMetalTextureIsFlipped(&view) || !full_aperture(&view) {
                return Err("unsupported CV origin or clean aperture".into());
            }
            raws.push(CVMetalTextureGetTexture(&view).ok_or("null Metal texture")?);
            views.push(view);
        }
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or("Metal identity exhausted")?;
        self.owners.insert(
            id,
            Owner {
                _image: surface.image.clone(),
                _views: views.try_into().unwrap(),
                readers: Arc::new(AtomicUsize::new(0)),
                retired: false,
                _permit: permit,
            },
        );
        let left = Arc::new(AtomicUsize::new(2));
        Ok(std::array::from_fn(|p| {
            let (format, w, h) = if p == 0 {
                (wgpu::TextureFormat::R8Unorm, surface.width, surface.height)
            } else {
                (
                    wgpu::TextureFormat::Rg8Unorm,
                    surface.width / 2,
                    surface.height / 2,
                )
            };
            let desc = wgpu::TextureDescriptor {
                label: Some("ddi-video-metal-plane"),
                size: wgpu::Extent3d {
                    width: w as u32,
                    height: h as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            };
            let left = left.clone();
            let retire = self.retire.clone();
            // SAFETY: initialized same-device CV texture, matching format/type/
            // extent. The owner retains image AND CV views until both texture
            // retirement and every encoded GPU read lease have completed.
            unsafe {
                let raw = wgpu::hal::metal::Device::texture_from_raw(
                    raws[p].clone(),
                    format,
                    MTLTextureType::Type2D,
                    1,
                    1,
                    desc.size.into(),
                    Some(Box::new(move || {
                        if left.fetch_sub(1, Ordering::AcqRel) == 1 {
                            let _ = retire.send(id);
                        }
                    })),
                );
                self.device
                    .create_texture_from_hal::<wgpu::hal::api::Metal>(
                        raw,
                        &desc,
                        wgpu::TextureUses::RESOURCE,
                    )
            }
        }))
    }
    #[cfg(all(test, feature = "video-bench"))]
    pub(super) fn retained(&self) -> usize {
        self.owners.len()
    }
    pub(super) fn protect(&mut self, encoder: &wgpu::CommandEncoder) {
        let _ = self.collect();
        let readers = self
            .owners
            .values()
            .map(|o| {
                o.readers.fetch_add(1, Ordering::AcqRel);
                o.readers.clone()
            })
            .collect();
        let reading = Reading(readers);
        // Deferred on THIS command buffer, not prior work on the queue. If
        // abandoned before submission, dropping the closure releases its lease.
        encoder.on_submitted_work_done(move || drop(reading));
    }
}
impl Drop for Cache {
    fn drop(&mut self) {
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_millis(10)),
        });
        let _ = self.collect();
        if !self.owners.is_empty() {
            disable();
            // Bound is global: at most four images / 128 MiB, then all future
            // frames use copies. Even a hung/lost GPU cannot recycle live CV
            // storage. Also protects a renderer moved beyond this thread's life.
            std::mem::forget(std::mem::take(&mut self.owners));
        }
    }
}
thread_local! {static CACHES:RefCell<Vec<Cache>>=const{RefCell::new(Vec::new())};}
pub(super) fn protect(device: &wgpu::Device, encoder: &wgpu::CommandEncoder) {
    CACHES.with(|c| {
        for cache in c.borrow_mut().iter_mut().filter(|c| c.device == *device) {
            cache.protect(encoder);
        }
    });
}
pub(super) fn show(
    renderer: &mut ddi_render::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: usize,
    frame: &ddi_platform::video::VideoFrame,
) -> bool {
    let Some(surface) = frame
        .external
        .as_ref()
        .and_then(|e| e.0.downcast_ref::<Surface>())
    else {
        return false;
    };
    let result = CACHES.with(|c| {
        let mut caches = c.borrow_mut();
        caches.retain_mut(|c| {
            let _ = c.collect();
            c.device == *device || !c.owners.is_empty()
        });
        if disabled() {
            return Err("Metal sharing disabled".to_string());
        }
        let index = if let Some(i) = caches.iter().position(|c| c.device == *device) {
            i
        } else {
            caches.push(Cache::new(device)?);
            caches.len() - 1
        };
        caches[index].frame(surface)
    });
    match result {
        Ok(p) => {
            renderer.set_video_nv12(
                device,
                id,
                &p[0],
                &p[1],
                frame.matrix == ddi_platform::video::YuvMatrix::Bt709,
                frame.full_range,
            );
            IMPORTED.fetch_add(1, Ordering::Relaxed);
            true
        }
        Err(e) => {
            WARNED.call_once(|| eprintln!("[video] Metal planes use copies: {e}"));
            COPIED.fetch_add(1, Ordering::Relaxed);
            match surface.copy() {
                Ok(p) => renderer.set_video_frame(
                    device,
                    queue,
                    id,
                    frame.width,
                    frame.height,
                    &p,
                    frame.matrix == ddi_platform::video::YuvMatrix::Bt709,
                    frame.full_range,
                ),
                Err(_) => false,
            }
        }
    }
}

#[cfg(test)]
mod tests;
