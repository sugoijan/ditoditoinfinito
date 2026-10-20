//! The display's refreshes as the OS tells them, for the pacing
//! (`crate::pacing`; `docs/plans/desktop.md`, "Frame pacing").
//!
//! On macOS 14 and later, the window's view's `CADisplayLink`
//! (`NSView.displayLink(target:selector:)`), on the main run loop: each
//! refresh's `timestamp` (the one just shown) and `targetTimestamp` (the
//! next, which Apple says to draw for), with a preferred frame rate range
//! pinned at the display's top rate, so that ProMotion does not hold the
//! window back. On older macOS a `CVDisplayLink` on its own thread (the
//! active displays'). Elsewhere none: the pacing infers the refreshes from
//! the acquires' returns. Running only while a play does.

use std::sync::{Arc, Mutex};

use winit::window::Window;

/// One refresh: the one shown at `at`, the next at `target` (host
/// seconds, `crate::clock`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tick {
    pub at: f64,
    pub target: f64,
}

/// The last refresh, written by the link's callback, read by the loop.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Option<Tick>>>);

impl Shared {
    // Written by macOS's links only.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn set(&self, tick: Tick) {
        if let Ok(mut t) = self.0.lock() {
            *t = Some(tick);
        }
    }

    fn get(&self) -> Option<Tick> {
        self.0.lock().ok().and_then(|t| *t)
    }
}

/// A display link for the window's display.
pub struct DisplayLink {
    shared: Shared,
    inner: imp::Inner,
}

impl DisplayLink {
    /// One for `window`'s display, paused, asking for `hz` (the OS's
    /// rate) where it can ask; none where the OS has none.
    pub fn new(window: &Window, hz: Option<f64>) -> Option<DisplayLink> {
        let shared = Shared::default();
        let inner = imp::Inner::new(window, hz, shared.clone())?;
        Some(DisplayLink { shared, inner })
    }

    /// Which link it is (`CADisplayLink`, `CVDisplayLink`).
    pub fn name(&self) -> &'static str {
        self.inner.name()
    }

    /// Runs (in play) or pauses.
    pub fn run(&self, on: bool) {
        self.inner.run(on);
    }

    /// The last refresh it told of.
    pub fn latest(&self) -> Option<Tick> {
        self.shared.get()
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
    use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send, sel};
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSRunLoop, NSRunLoopCommonModes};
    use objc2_quartz_core::{CACurrentMediaTime, CADisplayLink, CAFrameRateRange};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    use super::{Shared, Tick};

    /// A media time (`CACurrentMediaTime`'s clock: `mach_absolute_time`,
    /// as `Instant` reads it) on the host clock, read back to back.
    pub fn host(t: f64) -> f64 {
        let media = CACurrentMediaTime();
        crate::clock::now().0 - (media - t)
    }

    pub struct Ivars {
        shared: Shared,
    }

    define_class!(
        // SAFETY: `NSObject` has no subclassing requirements, and
        // `Target` implements no `Drop`.
        #[unsafe(super(NSObject))]
        #[name = "DDIDisplayLinkTarget"]
        #[ivars = Ivars]
        pub struct Target;

        impl Target {
            /// The link's callback, each refresh, on the main run loop.
            #[unsafe(method(refresh:))]
            fn refresh(&self, link: &CADisplayLink) {
                self.ivars().shared.set(Tick {
                    at: host(link.timestamp()),
                    target: host(link.targetTimestamp()),
                });
            }
        }
    );

    impl Target {
        fn new(shared: Shared) -> Retained<Target> {
            let this = Target::alloc().set_ivars(Ivars { shared });
            // SAFETY: `NSObject`'s `init` on a freshly allocated object.
            unsafe { msg_send![super(this), init] }
        }
    }

    pub enum Inner {
        Ca {
            link: Retained<CADisplayLink>,
            _target: Retained<Target>,
        },
        Cv(cv::Link),
    }

    impl Inner {
        pub fn new(window: &Window, hz: Option<f64>, shared: Shared) -> Option<Inner> {
            let handle = window.window_handle().ok()?;
            let RawWindowHandle::AppKit(h) = handle.as_raw() else {
                return None;
            };
            // SAFETY: winit's pointer to the window's view, which lives as
            // long as the window; read on the main thread.
            let view: &NSView = unsafe { h.ns_view.cast().as_ref() };
            if !view.respondsToSelector(sel!(displayLinkWithTarget:selector:)) {
                return cv::Link::new(shared).map(Inner::Cv);
            }
            let target = Target::new(shared);
            let object: &AnyObject = &target;
            // SAFETY: `target` answers `refresh:`, taking the link.
            let link = unsafe { view.displayLinkWithTarget_selector(object, sel!(refresh:)) };
            if let Some(hz) = hz.filter(|h| *h > 0.0) {
                let hz = hz.round() as f32;
                link.setPreferredFrameRateRange(CAFrameRateRange {
                    minimum: hz,
                    maximum: hz,
                    preferred: hz,
                });
            }
            link.setPaused(true);
            // SAFETY: the main run loop, from the main thread (winit's).
            unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
            Some(Inner::Ca {
                link,
                _target: target,
            })
        }

        pub fn name(&self) -> &'static str {
            match self {
                Inner::Ca { .. } => "CADisplayLink",
                Inner::Cv(_) => "CVDisplayLink",
            }
        }

        pub fn run(&self, on: bool) {
            match self {
                Inner::Ca { link, .. } => link.setPaused(!on),
                Inner::Cv(l) => l.run(on),
            }
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            if let Inner::Ca { link, .. } = self {
                link.invalidate();
            }
        }
    }

    /// `CVDisplayLink` (CoreVideo, before macOS 14), called on its own
    /// thread.
    pub mod cv {
        use std::cell::Cell;
        use std::ffi::c_void;

        use super::super::{Shared, Tick};

        #[repr(C)]
        struct SmpteTime {
            subframes: i16,
            subframe_divisor: i16,
            counter: u32,
            kind: u32,
            flags: u32,
            hours: i16,
            minutes: i16,
            seconds: i16,
            frames: i16,
        }

        /// `CVTimeStamp`.
        #[repr(C)]
        struct TimeStamp {
            version: u32,
            video_time_scale: i32,
            video_time: i64,
            host_time: u64,
            rate_scalar: f64,
            video_refresh_period: i64,
            smpte_time: SmpteTime,
            flags: u64,
            reserved: u64,
        }

        type LinkRef = *mut c_void;
        type Callback = unsafe extern "C" fn(
            LinkRef,
            *const TimeStamp,
            *const TimeStamp,
            u64,
            *mut u64,
            *mut c_void,
        ) -> i32;

        #[repr(C)]
        struct Timebase {
            numer: u32,
            denom: u32,
        }

        #[link(name = "CoreVideo", kind = "framework")]
        unsafe extern "C" {
            fn CVDisplayLinkCreateWithActiveCGDisplays(out: *mut LinkRef) -> i32;
            fn CVDisplayLinkSetOutputCallback(link: LinkRef, cb: Callback, ctx: *mut c_void)
            -> i32;
            fn CVDisplayLinkStart(link: LinkRef) -> i32;
            fn CVDisplayLinkStop(link: LinkRef) -> i32;
            fn CVDisplayLinkRelease(link: LinkRef);
        }

        unsafe extern "C" {
            fn mach_timebase_info(info: *mut Timebase) -> i32;
        }

        /// Mach ticks in seconds (`CACurrentMediaTime`'s clock).
        fn seconds(ticks: u64) -> f64 {
            let mut tb = Timebase { numer: 1, denom: 1 };
            // SAFETY: writes the struct it is given.
            unsafe { mach_timebase_info(&mut tb) };
            ticks as f64 * f64::from(tb.numer) / f64::from(tb.denom.max(1)) / 1e9
        }

        /// The callback: the output time is the refresh the frame being
        /// made is shown at; the one before it a period sooner.
        unsafe extern "C" fn output(
            _link: LinkRef,
            _now: *const TimeStamp,
            out: *const TimeStamp,
            _flags: u64,
            _out_flags: *mut u64,
            ctx: *mut c_void,
        ) -> i32 {
            // SAFETY: CoreVideo passes valid stamps, and `ctx` is the
            // `Shared` the link owns until after it stops.
            let (out, shared) = unsafe { (&*out, &*(ctx as *const Shared)) };
            if out.video_time_scale <= 0 || out.video_refresh_period <= 0 {
                return 0;
            }
            let period = out.video_refresh_period as f64 / f64::from(out.video_time_scale);
            let target = super::host(seconds(out.host_time));
            shared.set(Tick {
                at: target - period,
                target,
            });
            0
        }

        pub struct Link {
            link: LinkRef,
            ctx: *mut Shared,
            running: Cell<bool>,
        }

        impl Link {
            pub fn new(shared: Shared) -> Option<Link> {
                let mut link: LinkRef = std::ptr::null_mut();
                // SAFETY: CoreVideo writes the new link.
                if unsafe { CVDisplayLinkCreateWithActiveCGDisplays(&mut link) } != 0
                    || link.is_null()
                {
                    return None;
                }
                let ctx = Box::into_raw(Box::new(shared));
                // SAFETY: a valid link; `ctx` outlives it (freed in `drop`
                // after the link stops).
                unsafe { CVDisplayLinkSetOutputCallback(link, output, ctx.cast()) };
                Some(Link {
                    link,
                    ctx,
                    running: Cell::new(false),
                })
            }

            pub fn run(&self, on: bool) {
                if on == self.running.get() {
                    return;
                }
                self.running.set(on);
                // SAFETY: a valid link.
                unsafe {
                    if on {
                        CVDisplayLinkStart(self.link);
                    } else {
                        CVDisplayLinkStop(self.link);
                    }
                }
            }
        }

        impl Drop for Link {
            fn drop(&mut self) {
                // SAFETY: the link is stopped before it and its context go.
                unsafe {
                    CVDisplayLinkStop(self.link);
                    CVDisplayLinkRelease(self.link);
                    drop(Box::from_raw(self.ctx));
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        unsafe extern "C" {
            fn clock_gettime_nsec_np(clock: u32) -> u64;
        }

        #[test]
        fn a_cvdisplaylink_tells_refreshes_on_the_host_clock() {
            // No window: the active displays' link (skipped when none
            // ticks, a machine with no display awake).
            let shared = super::Shared::default();
            let Some(link) = super::cv::Link::new(shared.clone()) else {
                eprintln!("no CVDisplayLink: skipped");
                return;
            };
            link.run(true);
            let start = crate::clock::now().0;
            let mut seen = None;
            while crate::clock::now().0 - start < 1.0 {
                std::thread::sleep(std::time::Duration::from_millis(20));
                if let Some(t) = shared.get() {
                    seen = Some(t);
                    if crate::clock::now().0 - start > 0.2 {
                        break;
                    }
                }
            }
            link.run(false);
            let Some(t) = seen else {
                eprintln!("no refresh in a second: skipped");
                return;
            };
            let now = crate::clock::now().0;
            let period = t.target - t.at;
            eprintln!(
                "CVDisplayLink: period {:.3} ms, target {:.1} ms from now",
                period * 1000.0,
                (t.target - now) * 1000.0
            );
            assert!((0.002..0.05).contains(&period), "{period}");
            // The next refresh is near now, on the host clock.
            assert!((t.target - now).abs() < 0.1, "{t:?} {now}");
        }

        #[test]
        fn media_time_is_the_host_clock() {
            // `CACurrentMediaTime` counts `CLOCK_UPTIME_RAW`, the clock
            // `Instant` and the keyboard's stamps use.
            // SAFETY: a plain libc call with a valid clock id.
            let uptime = unsafe { clock_gettime_nsec_np(8) } as f64 / 1e9;
            let media = objc2_quartz_core::CACurrentMediaTime();
            assert!((uptime - media).abs() < 0.001, "{uptime} {media}");
            let now = crate::clock::now().0;
            assert!((super::host(media) - now).abs() < 0.001);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use winit::window::Window;

    use super::Shared;

    pub struct Inner;

    impl Inner {
        pub fn new(_window: &Window, _hz: Option<f64>, _shared: Shared) -> Option<Inner> {
            None
        }

        pub fn name(&self) -> &'static str {
            "none"
        }

        pub fn run(&self, _on: bool) {}
    }
}
