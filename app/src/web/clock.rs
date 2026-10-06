//! Host clock: `performance.now()` in seconds. Event timestamps
//! (`Event.timeStamp`) and rAF timestamps live on the same timeline.

use ddi_platform::{HostClock, HostTime};

#[derive(Clone, Copy, Default)]
pub(crate) struct PerformanceClock;

impl PerformanceClock {
    pub(crate) fn now_ms() -> f64 {
        web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }

    /// Converts a DOM `timeStamp` (ms on the performance timeline) to `HostTime`.
    pub(crate) fn from_dom_ms(ms: f64) -> HostTime {
        HostTime(ms / 1000.0)
    }
}

impl HostClock for PerformanceClock {
    fn now(&self) -> HostTime {
        HostTime(Self::now_ms() / 1000.0)
    }
}
