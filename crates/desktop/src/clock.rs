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

/// Milliseconds since the Unix epoch.
pub fn epoch_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// Whole milliseconds since the Unix epoch (0 before it): the rooms'
/// wall clock.
pub fn now_ms() -> u64 {
    epoch_ms().max(0.0) as u64
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

/// The clocks the window gives its UI each frame: the live art's
/// (`ddi_ui::Ui::set_clock`), seconds since the epoch as the web's, so a
/// start does not begin the art over from the same state, taken once and
/// run on by the host clock; and the wall clock (`ddi_ui::Ui::set_wall_ms`),
/// which the rooms are made, ticked and read on (review3 desktop-1: the
/// host clock stands still while the computer sleeps, so the art's clock
/// falls behind the wall clock by the sleep, and a room ticked on it froze
/// for as long).
#[derive(Debug, Default)]
pub struct UiClock {
    /// The wall clock less the host clock, at the first frame.
    origin: Option<f64>,
}

impl UiClock {
    /// One frame's clocks at host time `host` (seconds) and wall clock
    /// `wall_ms` (ms since the epoch, [`now_ms`]).
    pub fn frame(&mut self, ui: &mut ddi_ui::Ui, host: f64, wall_ms: u64) {
        let origin = *self.origin.get_or_insert(wall_ms as f64 / 1000.0 - host);
        ui.set_clock(origin + host);
        ui.set_wall_ms(wall_ms);
    }
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
