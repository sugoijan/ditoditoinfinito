//! Tap-to-tempo (`docs/plans/editor.md`, step 5): a key tapped along to
//! the music, its BPM estimated from the last taps, and while the music
//! plays the second a tapped beat was heard at (the offset to apply).
//!
//! The fit is robust: each tap gets a beat number from the gaps (each
//! gap rounded to whole beats of the median gap, so a beat skipped
//! counts two and an extra tap between two beats none), then a line of
//! least squares through the taps' times against their numbers, the
//! farthest tap more than a fifth of a beat off it left out and the
//! line fitted again. A pause of [`RESET_AFTER`] starts again.

/// A pause this long (seconds) starts a new run of taps.
pub const RESET_AFTER: f64 = 2.0;
/// The taps kept: the last.
pub const KEEP: usize = 24;
/// Taps needed for an estimate.
pub const MIN_TAPS: usize = 4;
/// A tap farther than this share of a beat off the line is left out.
const OUTLIER: f64 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Tap {
    at: f64,
    heard: Option<f64>,
}

/// The taps of a run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TapTempo {
    taps: Vec<Tap>,
}

/// What the taps say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub bpm: f64,
    /// The taps the fit kept.
    pub taps: usize,
    /// A song second a tapped beat was heard at, when the taps were
    /// made while the music played: on the line, in the run's middle.
    pub heard: Option<f64>,
}

impl TapTempo {
    /// A tap at `at` seconds (a clock steady across the run: the key's
    /// time stamp), at song second `heard` while the music plays (read
    /// as the key is handled). After a pause of [`RESET_AFTER`], or a
    /// clock gone back, the run starts again.
    pub fn tap(&mut self, at: f64, heard: Option<f64>) {
        if self
            .taps
            .last()
            .is_some_and(|l| at - l.at > RESET_AFTER || at < l.at)
        {
            self.taps.clear();
        }
        self.taps.push(Tap { at, heard });
        if self.taps.len() > KEEP {
            self.taps.remove(0);
        }
    }

    pub fn clear(&mut self) {
        self.taps.clear();
    }

    /// The taps in the run.
    pub fn len(&self) -> usize {
        self.taps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.taps.is_empty()
    }

    /// The run's tempo, once [`MIN_TAPS`] taps agree on one (30 to 600
    /// BPM).
    pub fn estimate(&self) -> Option<Estimate> {
        let taps = &self.taps;
        if taps.len() < MIN_TAPS {
            return None;
        }
        let gaps: Vec<f64> = taps.windows(2).map(|w| w[1].at - w[0].at).collect();
        let gap = median(gaps.clone())?;
        if gap <= 0.0 {
            return None;
        }
        // Beat numbers from the gaps.
        let mut k = 0.0;
        let mut points = vec![(0.0, taps[0].at, taps[0].heard)];
        for (g, t) in gaps.iter().zip(&taps[1..]) {
            k += (g / gap).round();
            points.push((k, t.at, t.heard));
        }
        let (a, p) = loop {
            let (a, p) = line(&points)?;
            let worst = points
                .iter()
                .enumerate()
                .map(|(i, &(k, t, _))| (i, (t - a - p * k).abs()))
                .max_by(|x, y| x.1.total_cmp(&y.1))?;
            if worst.1 > OUTLIER * p && points.len() > MIN_TAPS {
                points.remove(worst.0);
            } else {
                break (a, p);
            }
        };
        let (first, last) = (points[0].0, points[points.len() - 1].0);
        if points.len() < MIN_TAPS || last - first < (MIN_TAPS - 1) as f64 {
            return None;
        }
        let bpm = 60.0 / p;
        if !(30.0..=600.0).contains(&bpm) {
            return None;
        }
        // The song second at each tap less its clock's: the same along
        // the run but for the time a key took to be handled.
        let lag = median(
            points
                .iter()
                .filter_map(|&(_, t, h)| h.map(|h| h - t))
                .collect(),
        );
        let middle = ((first + last) / 2.0).round();
        Some(Estimate {
            bpm,
            taps: points.len(),
            heard: lag.map(|lag| a + p * middle + lag),
        })
    }
}

/// The least squares line `t = a + p·k` through `(k, t)`.
fn line(points: &[(f64, f64, Option<f64>)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    let (mk, mt) = points
        .iter()
        .fold((0.0, 0.0), |(k, t), p| (k + p.0 / n, t + p.1 / n));
    let (mut skk, mut skt) = (0.0, 0.0);
    for &(k, t, _) in points {
        skk += (k - mk) * (k - mk);
        skt += (k - mk) * (t - mt);
    }
    (skk > 0.0).then(|| {
        let p = skt / skk;
        (mt - p * mk, p)
    })
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic jitter in `-1..1`.
    fn jitter(i: usize) -> f64 {
        let x = ((i as u64).wrapping_mul(2_654_435_761) % 1000) as f64 / 500.0;
        x - 1.0
    }

    /// Taps on beats `beats` of `bpm` from `t0`, `ms` of jitter.
    fn taps(t: &mut TapTempo, bpm: f64, t0: f64, beats: &[i32], ms: f64, lag: Option<f64>) {
        let p = 60.0 / bpm;
        for (i, &b) in beats.iter().enumerate() {
            let at = t0 + f64::from(b) * p + jitter(i) * ms / 1000.0;
            t.tap(at, lag.map(|l| at + l));
        }
    }

    #[test]
    fn steady_taps_give_the_tempo() {
        let mut t = TapTempo::default();
        taps(&mut t, 128.0, 10.0, &[0, 1, 2], 0.0, None);
        assert_eq!(t.estimate(), None, "too few");
        t.tap(10.0 + 3.0 * 60.0 / 128.0, None);
        let e = t.estimate().unwrap();
        assert!((e.bpm - 128.0).abs() < 1e-6, "{e:?}");
        assert_eq!((e.taps, e.heard), (4, None));
    }

    #[test]
    fn jitter_averages_out() {
        let mut t = TapTempo::default();
        let beats: Vec<i32> = (0..16).collect();
        taps(&mut t, 174.0, 3.0, &beats, 15.0, None);
        let e = t.estimate().unwrap();
        assert!((e.bpm - 174.0).abs() < 0.5, "{e:?}");
        assert_eq!(e.taps, 16);
    }

    #[test]
    fn a_stray_tap_a_skipped_beat_and_a_double_tap_are_left_out() {
        let mut t = TapTempo::default();
        let p = 0.5;
        // Beats 0..12 at 120 BPM, beat 5 skipped, beat 8 a third of a
        // beat late, an extra tap between 10 and 11.
        for b in 0..12 {
            match b {
                5 => {}
                8 => t.tap(f64::from(b) * p + p / 3.0, None),
                10 => {
                    t.tap(f64::from(b) * p, None);
                    t.tap(f64::from(b) * p + 0.4 * p, None);
                }
                _ => t.tap(f64::from(b) * p, None),
            }
        }
        let e = t.estimate().unwrap();
        assert!((e.bpm - 120.0).abs() < 1e-6, "{e:?}");
        assert_eq!(e.taps, 10, "the late tap and the extra one left out");
    }

    #[test]
    fn a_pause_starts_again_and_only_the_last_taps_count() {
        let mut t = TapTempo::default();
        let beats: Vec<i32> = (0..8).collect();
        taps(&mut t, 90.0, 0.0, &beats, 0.0, None);
        assert!((t.estimate().unwrap().bpm - 90.0).abs() < 1e-6);
        // 2.5 s later, another tempo.
        let last = 7.0 * 60.0 / 90.0;
        t.tap(last + 2.5, None);
        assert_eq!(t.len(), 1);
        assert_eq!(t.estimate(), None);
        let beats: Vec<i32> = (1..40).collect();
        taps(&mut t, 150.0, last + 2.5, &beats, 0.0, None);
        assert_eq!(t.len(), KEEP);
        assert!((t.estimate().unwrap().bpm - 150.0).abs() < 1e-6);
        // A clock gone back starts again too.
        t.tap(0.0, None);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn taps_while_listening_say_where_a_beat_was_heard() {
        let mut t = TapTempo::default();
        // Beats at song seconds 0.3 + n·0.5; the key's clock 100 s
        // behind the song's.
        let beats: Vec<i32> = (0..9).collect();
        taps(&mut t, 120.0, -99.7, &beats, 4.0, Some(100.0));
        let e = t.estimate().unwrap();
        let heard = e.heard.unwrap();
        let phase = (heard - 0.3) / 0.5;
        assert!((phase - phase.round()).abs() < 0.02, "{heard}");
    }

    #[test]
    fn nonsense_gives_nothing() {
        let mut t = TapTempo::default();
        for _ in 0..6 {
            t.tap(1.0, None);
        }
        assert_eq!(t.estimate(), None, "all at once");
        let mut t = TapTempo::default();
        for i in 0..6 {
            t.tap(f64::from(i) * 0.01, None);
        }
        assert_eq!(t.estimate(), None, "6000 BPM");
    }

    #[test]
    fn a_tempo_tapped_while_listening_sets_the_bpm_and_the_offset() {
        use ddi_chart::Tick;

        use crate::state::editor::{Editor, Retime};

        let mut e = Editor::new_song(None, "Taps");
        e.toggle_at(0, Tick::from_beats(4));
        // The music's beats at 0.237 + n·60/128 s, tapped with 3 ms of
        // jitter, the key's clock 50 s ahead of the song's.
        let mut t = TapTempo::default();
        let beats: Vec<i32> = (0..12).collect();
        taps(
            &mut t,
            128.0,
            50.237 + 20.0 * 60.0 / 128.0,
            &beats,
            3.0,
            Some(-50.0),
        );
        let est = t.estimate().unwrap();
        assert!((est.bpm - 128.0).abs() < 0.05, "{est:?}");
        let bpm = (est.bpm * 100.0).round() / 100.0;
        assert!(e.set_tapped_tempo(bpm, est.heard, Retime::KeepBeats));
        let timing = e.timing();
        assert_eq!(timing.bpms.len(), 1);
        assert_eq!(timing.bpms[0].bpm, bpm);
        // Beat 0 where the music's is, give or take a few milliseconds.
        let first = timing.seconds_at(Tick::ZERO);
        assert!((first - 0.237).abs() < 0.006, "{first}");
        let ms = timing.offset_seconds * 1000.0;
        assert!((ms - ms.round()).abs() < 1e-6, "to the millisecond: {ms}");
        // One edit: undone at once.
        e.undo();
        assert_eq!(e.timing().bpms[0].bpm, 120.0);
        assert_eq!(e.timing().offset_seconds, 0.0);
        // At a later cursor without the offset: a BPM change there.
        e.set_cursor(Tick::from_beats(8), false);
        assert!(e.set_tapped_tempo(bpm, None, Retime::KeepBeats));
        assert_eq!(e.timing().bpms.len(), 2);
        assert_eq!(e.timing().bpms[1].tick, Tick::from_beats(8));
        assert_eq!(e.timing().offset_seconds, 0.0);
        assert!(!e.set_tapped_tempo(0.0, None, Retime::KeepBeats), "refused");
    }
}
