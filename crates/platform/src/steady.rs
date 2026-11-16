//! The rooms' clock (round P1b, the maintainer's of 2026-11-16: "one
//! steady clock"): milliseconds on the wall clock's scale that never go
//! backwards. Anchored once to the wall clock, at its first reading, and
//! run on from there by a monotonic clock (the web's `performance.now()`
//! from the page's time origin; desktop's own, which counts on while the
//! computer sleeps), so that a device's wall clock set back (a network
//! time correction, a clock set by hand) or forward never stalls nor
//! skips a room's countdown, its reconnect waits or a live stage's song
//! time. Rooms, their screens' ticks (`ddi_ui::Ui::set_wall_ms`) and a
//! live play all read the one clock.
//!
//! What still reads the real wall clock: this clock's anchor (once), and
//! everything that is a date rather than a duration (a play's date in the
//! history, an import's, a share's or a session's expiry as the server
//! says it). The room's clock offset (`ddi_room::live::clock`) is
//! measured between this clock and the server's at every ping: a step of
//! the device's wall clock never reaches it, and a monotonic clock that
//! stood still while the computer slept (a browser's) is one step, which
//! the estimate drops its older samples for.

/// A steady clock's anchor and last reading.
#[derive(Clone, Debug)]
pub struct SteadyClock {
    /// The wall clock less the monotonic clock at the first reading.
    origin_ms: Option<f64>,
    last_ms: u64,
}

impl SteadyClock {
    /// A clock not read yet (anchored at its first reading).
    pub const fn new() -> Self {
        Self {
            origin_ms: None,
            last_ms: 0,
        }
    }

    /// The steady time (ms) for a reading of the wall clock (`wall_ms`,
    /// ms since the Unix epoch) and of the monotonic clock (`running_ms`,
    /// ms from any origin): the first reading's wall time, run on by the
    /// monotonic clock since; never less than the last.
    pub fn read(&mut self, wall_ms: f64, running_ms: f64) -> u64 {
        let origin = *self.origin_ms.get_or_insert(wall_ms - running_ms);
        let now = (origin + running_ms).max(0.0) as u64;
        self.last_ms = self.last_ms.max(now);
        self.last_ms
    }
}

impl Default for SteadyClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wall clock set back a minute, then forward an hour: the steady
    /// clock goes on with the monotonic one, never backwards, never
    /// skipping; and a monotonic clock that goes on through a sleep is
    /// followed.
    #[test]
    fn steps_of_the_wall_clock_are_not_followed() {
        let mut c = SteadyClock::default();
        let (mut wall, mut run) = (1_700_000_000_000.0, 5_000.0);
        assert_eq!(c.read(wall, run), 1_700_000_000_000);
        wall += 1_000.0;
        run += 1_000.0;
        assert_eq!(c.read(wall, run), 1_700_000_001_000);
        wall -= 60_000.0;
        run += 50.0;
        assert_eq!(c.read(wall, run), 1_700_000_001_050, "set back: on");
        wall += 3_600_000.0;
        run += 50.0;
        assert_eq!(c.read(wall, run), 1_700_000_001_100, "set forward: on");
        // Asleep a minute (a monotonic clock that counts it).
        wall += 60_000.0;
        run += 60_000.0;
        assert_eq!(c.read(wall, run), 1_700_000_061_100);
        // A monotonic reading behind the last (never, but): held.
        assert_eq!(c.read(wall, run - 10.0), 1_700_000_061_100);
    }
}
