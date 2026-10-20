//! The process's host clock: one epoch, an `Instant` taken at start, that
//! audio, input and the history all stamp against
//! (`docs/plans/desktop.md`, section 1: "Host time"). [`ddi_platform::HostTime`]
//! is seconds since it, as `performance.now()/1000` is on the web.

use std::sync::OnceLock;
use std::time::Instant;

use ddi_platform::HostTime;

static EPOCH: OnceLock<Instant> = OnceLock::new();

/// The epoch (taken on first use).
pub fn epoch() -> Instant {
    *EPOCH.get_or_init(Instant::now)
}

/// Nanoseconds since the epoch, now.
pub fn now_ns() -> u64 {
    nanos_since(Instant::now())
}

/// Nanoseconds from the epoch to `at` (0 before it).
pub fn nanos_since(at: Instant) -> u64 {
    at.saturating_duration_since(epoch()).as_nanos() as u64
}

/// The host time now.
pub fn now() -> HostTime {
    HostTime(now_ns() as f64 / 1e9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_time_counts_from_one_epoch() {
        let a = now();
        let at = Instant::now();
        let b = now();
        assert!(a.0 >= 0.0 && b.0 >= a.0);
        let mid = nanos_since(at) as f64 / 1e9;
        assert!(a.0 <= mid && mid <= b.0);
        // Before the epoch counts as 0.
        assert_eq!(nanos_since(epoch() - std::time::Duration::from_secs(1)), 0);
    }
}
