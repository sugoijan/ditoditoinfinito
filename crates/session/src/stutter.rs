//! Bursts of late frames, what a player sees as stutter: one missed frame
//! goes unnoticed, several close together do not.

/// How much longer than the refresh a frame's interval must be to be late.
/// Under 2, so a missed refresh counts; over the browsers' jitter.
const LATE: f64 = 1.5;
/// Late frames this close (seconds) belong to one burst.
const JOINED: f64 = 0.25;
/// The fewest late frames that make a burst.
const FEWEST: usize = 3;

/// A run of late frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Burst {
    /// The index in the frames of its first late frame.
    pub first: usize,
    /// The late frames in it.
    pub late: usize,
    /// The longest interval in it, seconds.
    pub worst: f64,
    /// Seconds from its first late frame's start to its last's end.
    pub length: f64,
}

/// How many of the frames starting at the times `at` (seconds, ascending)
/// were late, on a display refreshing every `period` seconds, and the
/// bursts among them.
pub fn bursts(at: &[f64], period: f64) -> (usize, Vec<Burst>) {
    let threshold = period * LATE;
    let late: Vec<usize> = (1..at.len())
        .filter(|&i| at[i] - at[i - 1] > threshold)
        .collect();
    let mut found = Vec::new();
    let mut run = 0;
    while run < late.len() {
        let mut end = run;
        while end + 1 < late.len() && at[late[end + 1] - 1] - at[late[end]] <= JOINED {
            end += 1;
        }
        if end + 1 - run >= FEWEST {
            let worst = late[run..=end]
                .iter()
                .map(|&i| at[i] - at[i - 1])
                .fold(0.0, f64::max);
            found.push(Burst {
                first: late[run],
                late: end + 1 - run,
                worst,
                length: at[late[end]] - at[late[run] - 1],
            });
        }
        run = end + 1;
    }
    (late.len(), found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIOD: f64 = 1.0 / 120.0;

    /// Frames at each interval given, the first at zero.
    fn frames(intervals: impl IntoIterator<Item = f64>) -> Vec<f64> {
        let mut t = 0.0;
        std::iter::once(0.0)
            .chain(intervals.into_iter().map(|d| {
                t += d;
                t
            }))
            .collect()
    }

    fn even(n: usize) -> impl Iterator<Item = f64> {
        std::iter::repeat_n(PERIOD, n)
    }

    #[test]
    fn steady_frames_and_lone_misses_are_no_burst() {
        assert_eq!(bursts(&frames(even(600)), PERIOD), (0, vec![]));
        let lone: Vec<f64> = even(100)
            .chain([PERIOD * 2.0])
            .chain(even(100))
            .chain([PERIOD * 2.0])
            .chain(even(100))
            .collect();
        let (late, found) = bursts(&frames(lone), PERIOD);
        assert_eq!(late, 2);
        assert!(found.is_empty());
    }

    #[test]
    fn late_frames_close_together_are_one_burst() {
        let intervals: Vec<f64> = even(10)
            .chain([PERIOD * 2.0, PERIOD, PERIOD * 3.0, PERIOD, PERIOD * 2.0])
            .chain(even(10))
            .collect();
        let (late, found) = bursts(&frames(intervals), PERIOD);
        assert_eq!(late, 3);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].first, 11);
        assert_eq!(found[0].late, 3);
        assert!((found[0].worst - PERIOD * 3.0).abs() < 1e-9);
        assert!((found[0].length - PERIOD * 9.0).abs() < 1e-9);
    }

    #[test]
    fn bursts_far_apart_are_told_apart() {
        let burst = [PERIOD * 2.0; 3];
        let intervals: Vec<f64> = burst.into_iter().chain(even(240)).chain(burst).collect();
        let (late, found) = bursts(&frames(intervals), PERIOD);
        assert_eq!(late, 6);
        assert_eq!(found.iter().map(|b| b.first).collect::<Vec<_>>(), [1, 244]);
    }

    #[test]
    fn no_frames_and_one_frame_have_no_intervals() {
        assert_eq!(bursts(&[], PERIOD), (0, vec![]));
        assert_eq!(bursts(&[1.0], PERIOD), (0, vec![]));
    }
}
