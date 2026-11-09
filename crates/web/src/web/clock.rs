//! Host clock: `performance.now()` in seconds. Event timestamps
//! (`Event.timeStamp`) and rAF timestamps live on the same timeline.

use ddi_platform::HostTime;

#[derive(Clone, Copy, Default)]
pub(crate) struct PerformanceClock;

impl PerformanceClock {
    pub(crate) fn now_ms() -> f64 {
        web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }

    /// A reader of this clock in seconds that finds the page's `Performance`
    /// once, for a loop that reads the clock between small steps.
    pub(crate) fn seconds() -> impl Fn() -> f64 {
        let performance = web_sys::window().and_then(|w| w.performance());
        move || performance.as_ref().map_or(0.0, |p| p.now()) / 1000.0
    }

    /// Converts a DOM `timeStamp` (ms on the performance timeline) to `HostTime`.
    pub(crate) fn from_dom_ms(ms: f64) -> HostTime {
        HostTime(ms / 1000.0)
    }
}
