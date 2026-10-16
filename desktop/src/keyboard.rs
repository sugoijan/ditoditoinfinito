//! The keyboard on desktop (`docs/plans/desktop.md`, section 2, D4):
//! winit's key events with a time. winit gives none, so a key is stamped
//! when its event reaches the shell, unless the OS said when it happened:
//! on macOS an `NSEvent` local monitor ([`monitor`]) reads each key's
//! `timestamp` (seconds since boot, `CLOCK_UPTIME_RAW`, the clock of
//! Rust's `Instant` there: step 7.0 checked it) before winit sees the
//! event, and the shell pairs winit's `KeyboardInput` with it by key code
//! and direction ([`Stamps`]).
//!
//! [`DesktopKeyboard`] is the play's input source, as the web's
//! `WebKeyboard`: physical codes, the OS's repeats dropped, a pressed set
//! against doubled reports, everything released when the window loses
//! focus.

use std::collections::{HashSet, VecDeque};

use ddi_platform::{DeviceId, HostTime, InputSource, RawInput};

/// A stamp this far (seconds) after its event's receipt is the
/// conversion's jitter (two clocks read back to back), taken as the
/// receipt; later ones are not taken.
const FUTURE: f64 = 0.002;
/// A stamp older than this (seconds) was never paired: dropped.
const STALE: f64 = 1.0;
/// The most stamps kept waiting (a monitor whose keys winit never
/// reports, as a key held through a menu that takes it).
const KEPT: usize = 64;

/// A key's down or up as the OS stamped it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamp {
    /// The platform's key code (macOS's virtual key code, as winit's
    /// `to_scancode` gives it).
    pub code: u32,
    pub down: bool,
    pub repeat: bool,
    pub time: HostTime,
}

/// The OS's stamps waiting for winit's events.
#[derive(Clone, Debug, Default)]
pub struct Stamps {
    queue: VecDeque<Stamp>,
    /// Keys paired with a stamp, and stamped on receipt.
    pub paired: u64,
    pub unpaired: u64,
}

impl Stamps {
    /// A stamp from the OS, in order.
    pub fn push(&mut self, s: Stamp) {
        self.queue.push_back(s);
        while self.queue.len() > KEPT {
            self.queue.pop_front();
        }
    }

    /// The time of winit's event for key `code` (`None`: it has no code)
    /// going `down` (or up), received `now`: the stamp of that key and
    /// direction, else `now`. Of the waiting stamps of that key and
    /// direction before the key's next stamp the other way, the newest
    /// is taken and the older ones dropped: a stamp whose event never
    /// came (a key an input method or a system panel took, a doubled
    /// monitor call) must not stamp a later press early. Stale stamps go;
    /// a stamp in the future is not taken.
    pub fn take(&mut self, code: Option<u32>, down: bool, repeat: bool, now: HostTime) -> HostTime {
        self.queue.retain(|s| now.0 - s.time.0 <= STALE);
        let mut run: Vec<usize> = Vec::new();
        if let Some(code) = code {
            for (i, s) in self.queue.iter().enumerate() {
                if !same_key(s.code, code) {
                    continue;
                }
                if s.down != down {
                    // The key's other direction: the run is over, once
                    // it began.
                    if !run.is_empty() {
                        break;
                    }
                    continue;
                }
                if s.repeat == repeat {
                    run.push(i);
                }
            }
        }
        let found = run.last().copied();
        let taken = found.and_then(|i| self.queue.get(i).copied());
        // The run's older stamps and the one taken go (in reverse, so the
        // indices hold).
        if taken.is_some_and(|s| s.time.0 <= now.0 + FUTURE) {
            for &i in run.iter().rev() {
                self.queue.remove(i);
            }
        }
        match taken {
            Some(s) if s.time.0 <= now.0 + FUTURE => {
                self.paired += 1;
                HostTime(s.time.0.min(now.0))
            }
            _ => {
                self.unpaired += 1;
                now
            }
        }
    }

    pub fn waiting(&self) -> usize {
        self.queue.len()
    }
}

/// Whether a stamp's key code `stamp` is the key winit reports as
/// `code`: the same code, or macOS's ISO section key (0x0a), which winit
/// reports as `Backquote` and gives the code of the ANSI grave key (0x32).
fn same_key(stamp: u32, code: u32) -> bool {
    stamp == code || (code == 0x32 && stamp == 0x0a)
}

/// The keyboard as a play's input source.
#[derive(Debug, Default)]
pub struct DesktopKeyboard {
    held: HashSet<String>,
    out: Vec<RawInput>,
    /// Edges are kept for [`InputSource::poll`] only while a play reads
    /// them.
    pub enabled: bool,
}

impl DesktopKeyboard {
    pub fn new() -> DesktopKeyboard {
        DesktopKeyboard::default()
    }

    /// A key event: `code` as the DOM names it, at `time`. Repeats and
    /// doubled reports make no edge.
    pub fn key(&mut self, code: &str, down: bool, repeat: bool, time: HostTime) {
        if repeat {
            return;
        }
        let changed = if down {
            self.held.insert(code.to_string())
        } else {
            self.held.remove(code)
        };
        if changed && self.enabled {
            self.out.push(RawInput {
                device: DeviceId::Keyboard,
                control: code.to_string(),
                pressed: down,
                host_time: time,
                slot: 0,
            });
        }
    }

    /// The window lost focus: what is held is released at `time` (its
    /// releases would go elsewhere).
    pub fn release_all(&mut self, time: HostTime) {
        let mut held: Vec<String> = self.held.drain().collect();
        held.sort();
        if self.enabled {
            self.out.extend(held.into_iter().map(|control| RawInput {
                device: DeviceId::Keyboard,
                control,
                pressed: false,
                host_time: time,
                slot: 0,
            }));
        }
    }

    pub fn held(&self) -> &HashSet<String> {
        &self.held
    }
}

impl InputSource for DesktopKeyboard {
    fn poll(&mut self, out: &mut Vec<RawInput>) {
        out.append(&mut self.out);
    }
}

/// macOS: the `NSEvent` local monitor. Installed once on the main thread
/// ([`monitor::install`]); each key down and up is queued with its time
/// before AppKit hands it on (to winit), and [`monitor::take`] moves them
/// into the shell's [`Stamps`].
#[cfg(target_os = "macos")]
pub mod monitor {
    use std::cell::RefCell;
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};

    use super::{Stamp, Stamps};

    thread_local! {
        static QUEUE: RefCell<Vec<Stamp>> = const { RefCell::new(Vec::new()) };
        static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    }

    unsafe extern "C" {
        /// `<time.h>`: nanoseconds of a clock (macOS 10.12 and later).
        fn clock_gettime_nsec_np(clock: u32) -> u64;
    }

    /// `CLOCK_UPTIME_RAW`: `mach_absolute_time`, which `NSEvent.timestamp`
    /// counts (as seconds since boot) and Rust's `Instant` reads.
    const CLOCK_UPTIME_RAW: u32 = 8;

    /// An `NSEvent` timestamp on the host clock (`crate::clock`): the
    /// uptime and the host time read back to back, both on one clock.
    pub fn host_time(timestamp: f64) -> ddi_platform::HostTime {
        // SAFETY: a plain libc call with a valid clock id.
        let uptime = unsafe { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) } as f64 / 1e9;
        let now = crate::clock::now();
        ddi_platform::HostTime(now.0 - (uptime - timestamp))
    }

    /// Installs the monitor (once; on the main thread, after the app
    /// started). Whether it is installed.
    pub fn install() -> bool {
        if MONITOR.with(|m| m.borrow().is_some()) {
            return true;
        }
        let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit hands a valid event for the monitor's call.
            let e = unsafe { event.as_ref() };
            let kind = e.r#type();
            if kind == NSEventType::KeyDown || kind == NSEventType::KeyUp {
                let stamp = Stamp {
                    code: u32::from(e.keyCode()),
                    down: kind == NSEventType::KeyDown,
                    repeat: kind == NSEventType::KeyDown && e.isARepeat(),
                    time: host_time(e.timestamp()),
                };
                QUEUE.with(|q| q.borrow_mut().push(stamp));
            }
            // Handed on unchanged: winit sees every key as before.
            event.as_ptr()
        });
        // SAFETY: called on the main thread with a block of the expected
        // signature that returns the event it was given.
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::KeyDown | NSEventMask::KeyUp,
                &block,
            )
        };
        let ok = monitor.is_some();
        MONITOR.with(|m| *m.borrow_mut() = monitor);
        ok
    }

    /// Moves the stamps the monitor queued into `stamps`.
    pub fn take(stamps: &mut Stamps) {
        QUEUE.with(|q| {
            for s in q.borrow_mut().drain(..) {
                stamps.push(s);
            }
        });
    }
}

/// Elsewhere there is no monitor: keys are stamped on receipt.
#[cfg(not(target_os = "macos"))]
pub mod monitor {
    use super::Stamps;

    pub fn install() -> bool {
        false
    }

    pub fn take(_stamps: &mut Stamps) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(code: u32, down: bool, t: f64) -> Stamp {
        Stamp {
            code,
            down,
            repeat: false,
            time: HostTime(t),
        }
    }

    /// Recorded on macOS: the monitor's stamps and winit's events of a
    /// quick roll over F, J (key codes 3 and 38) and a held F repeating,
    /// winit's events later by the time the main thread was busy.
    #[test]
    fn winit_keys_take_the_os_times_in_order() {
        let mut s = Stamps::default();
        // The monitor runs first, as AppKit dispatches.
        s.push(stamp(3, true, 10.000));
        s.push(stamp(38, true, 10.004));
        // winit's events come in a frame later.
        assert_eq!(
            s.take(Some(3), true, false, HostTime(10.016)),
            HostTime(10.000)
        );
        assert_eq!(
            s.take(Some(38), true, false, HostTime(10.016)),
            HostTime(10.004)
        );
        // A repeat pairs with a repeat only.
        s.push(Stamp {
            repeat: true,
            ..stamp(3, true, 10.500)
        });
        s.push(stamp(38, false, 10.510));
        assert_eq!(
            s.take(Some(38), false, false, HostTime(10.52)),
            HostTime(10.510)
        );
        assert_eq!(
            s.take(Some(3), true, true, HostTime(10.52)),
            HostTime(10.500)
        );
        s.push(stamp(3, false, 10.600));
        assert_eq!(
            s.take(Some(3), false, false, HostTime(10.61)),
            HostTime(10.600)
        );
        assert_eq!((s.paired, s.unpaired), (5, 0));
        assert_eq!(s.waiting(), 0);
    }

    #[test]
    fn stamps_whose_events_never_came_do_not_stamp_later_presses() {
        let mut s = Stamps::default();
        // Return taken by an input method: its down never reaches winit.
        s.push(stamp(36, true, 10.0));
        s.push(stamp(36, true, 10.6));
        assert_eq!(
            s.take(Some(36), true, false, HostTime(10.61)),
            HostTime(10.6)
        );
        assert_eq!(s.waiting(), 0);
        // A doubled monitor call.
        s.push(stamp(3, true, 1.0));
        s.push(stamp(3, true, 1.0));
        assert_eq!(s.take(Some(3), true, false, HostTime(1.01)), HostTime(1.0));
        s.push(stamp(3, false, 1.1));
        assert_eq!(s.take(Some(3), false, false, HostTime(1.11)), HostTime(1.1));
        s.push(stamp(3, true, 1.2));
        assert_eq!(s.take(Some(3), true, false, HostTime(1.21)), HostTime(1.2));
        // A quick down, up, down waiting together: each its own.
        s.push(stamp(4, true, 2.0));
        s.push(stamp(4, false, 2.03));
        s.push(stamp(4, true, 2.06));
        assert_eq!(s.take(Some(4), true, false, HostTime(2.07)), HostTime(2.0));
        assert_eq!(
            s.take(Some(4), false, false, HostTime(2.07)),
            HostTime(2.03)
        );
        assert_eq!(s.take(Some(4), true, false, HostTime(2.07)), HostTime(2.06));
        // The ISO section key: winit's Backquote code pairs with it.
        s.push(stamp(0x0a, true, 3.0));
        assert_eq!(
            s.take(Some(0x32), true, false, HostTime(3.01)),
            HostTime(3.0)
        );
    }

    #[test]
    fn keys_without_a_stamp_are_stamped_on_receipt() {
        let mut s = Stamps::default();
        // No monitor (or a key it never saw): the receipt.
        assert_eq!(s.take(Some(3), true, false, HostTime(5.0)), HostTime(5.0));
        assert_eq!(s.take(None, true, false, HostTime(5.1)), HostTime(5.1));
        // A stamp of the other direction, of another key, from the future
        // or stale is not taken.
        s.push(stamp(3, false, 5.2));
        s.push(stamp(4, true, 5.2));
        assert_eq!(s.take(Some(3), true, false, HostTime(5.3)), HostTime(5.3));
        s.push(stamp(5, true, 6.0));
        assert_eq!(s.take(Some(5), true, false, HostTime(5.9)), HostTime(5.9));
        s.push(stamp(6, true, 1.0));
        assert_eq!(s.take(Some(6), true, false, HostTime(5.9)), HostTime(5.9));
        assert_eq!(s.paired, 0);
        // The stale ones went; the queue stays bounded.
        for i in 0..200 {
            s.push(stamp(i, true, 6.0));
        }
        assert!(s.waiting() <= KEPT);
    }

    #[test]
    fn the_keyboard_drops_repeats_and_doubles_and_releases_on_blur() {
        let mut k = DesktopKeyboard::new();
        k.enabled = true;
        k.key("KeyF", true, false, HostTime(1.0));
        k.key("KeyF", true, true, HostTime(1.5));
        k.key("KeyF", true, false, HostTime(1.6));
        k.key("KeyJ", true, false, HostTime(1.7));
        k.key("KeyF", false, false, HostTime(2.0));
        k.release_all(HostTime(3.0));
        let mut out = Vec::new();
        k.poll(&mut out);
        let edges: Vec<(&str, bool, f64)> = out
            .iter()
            .map(|e| (e.control.as_str(), e.pressed, e.host_time.0))
            .collect();
        assert_eq!(
            edges,
            vec![
                ("KeyF", true, 1.0),
                ("KeyJ", true, 1.7),
                ("KeyF", false, 2.0),
                ("KeyJ", false, 3.0),
            ]
        );
        assert!(k.held().is_empty());
        // Not read: nothing kept.
        k.enabled = false;
        k.key("KeyF", true, false, HostTime(4.0));
        k.poll(&mut out);
        assert_eq!(out.len(), 4);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn os_times_convert_onto_the_host_clock() {
        // An `NSEvent` stamped "now" (the uptime) is the host time now.
        unsafe extern "C" {
            fn clock_gettime_nsec_np(clock: u32) -> u64;
        }
        let uptime = unsafe { clock_gettime_nsec_np(8) } as f64 / 1e9;
        let before = crate::clock::now();
        let t = monitor::host_time(uptime);
        let after = crate::clock::now();
        assert!(
            t.0 <= after.0 + 1e-6 && t.0 >= before.0 - 0.001,
            "{t:?} {before:?} {after:?}"
        );
        // A second earlier is a second earlier.
        let earlier = monitor::host_time(uptime - 1.0);
        assert!((t.0 - earlier.0 - 1.0).abs() < 0.001);
    }
}
