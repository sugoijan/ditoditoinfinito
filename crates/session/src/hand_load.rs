//! The dancing hand giving way when a play runs late (`hand-load-v0`,
//! `docs/plans/dancing-hand.md`, "Under load"): the hand is an autoplay
//! beside the play, never the play, so when frames miss their refresh
//! while it is on it does less, one way and a step at a time: lighter
//! (half its share of a frame to prepare in, its picture drawn every
//! other frame), then resting (no preparation, its picture standing
//! still), then gone for the rest of the play. Each step after its own
//! run of late seconds, as the automatic video setting's rule
//! (`ddi_platform::video::VideoGuard`) watches a movie: the same
//! measured seconds ([`crate::backgrounds::FpsMeter`]), the same idea of
//! a second that was late and of a stall that counts for nothing. The
//! field, the judging, the sound and the input are never touched by it,
//! and nothing of a play's record or replay.
//!
//! Every value is named here and PROVISIONAL.

use ddi_platform::video::Second;

/// The rule's name and version, for the debug state and the notes.
pub const HAND_LOAD: &str = "hand-load-v0";
/// The hand is watched this long (ms of host time) before a second is
/// judged: a play's first frames, its textures and skin warming up.
pub const WARMUP_MS: f64 = 2000.0;
/// A second whose frame rate is below this share of the refresh rate is
/// late: one frame in ten missed its refresh. (The maintainer's captures
/// under load: 16–17% of frames late with the hand on, 3.5–4% with it off;
/// quiet, 0.4%.)
pub const LATE_SHARE: f64 = 0.9;
/// Late seconds in a row that take each step.
pub const LATE_SECONDS: u32 = 3;
/// Seconds after a step that are not judged: what it sheds shows first.
pub const SETTLE_SECONDS: u32 = 1;
/// A second holding a frame this long (ms) had a one-off stall (a hidden
/// tab, a collection, a seek): it and the warm-up after it count for
/// nothing, as the video rule's `GUARD_HITCH_MS`.
pub const HITCH_MS: f64 = 250.0;
/// The share of a frame the hand prepares in once lighter, of its usual.
pub const LIGHTER_SHARE: f64 = 0.5;

/// How much of itself the hand spends, in the rule's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum HandStep {
    /// As always.
    #[default]
    Full,
    /// Half its share of a frame to prepare in ([`LIGHTER_SHARE`]), its
    /// picture drawn every other frame.
    Lighter,
    /// No preparation; its picture stands still where it was.
    Resting,
    /// Neither prepared nor drawn, for the rest of the play.
    Gone,
}

impl HandStep {
    pub fn name(self) -> &'static str {
        match self {
            HandStep::Full => "full",
            HandStep::Lighter => "lighter",
            HandStep::Resting => "resting",
            HandStep::Gone => "gone",
        }
    }

    fn next(self) -> HandStep {
        match self {
            HandStep::Full => HandStep::Lighter,
            HandStep::Lighter => HandStep::Resting,
            HandStep::Resting | HandStep::Gone => HandStep::Gone,
        }
    }
}

/// The rule over one play: where it stands and what it has seen.
#[derive(Clone, Debug, Default)]
pub struct HandLoad {
    step: HandStep,
    /// Watched since (ms), for the warm-up; `None` while not watched.
    since: Option<f64>,
    /// Late seconds in a row, and seconds still to settle after a step.
    run: u32,
    settle: u32,
    /// The refresh rate measured before the play started.
    refresh_at_start: f64,
    /// Each step taken, and when (ms of host time).
    taken: Vec<(HandStep, f64)>,
    /// The seconds judged, and of them the late ones.
    judged: u32,
    late: u32,
}

impl HandLoad {
    /// A play starts (again) on a display refreshing at `refresh` Hz:
    /// the hand as always.
    pub fn start(&mut self, refresh: f64) {
        *self = HandLoad {
            refresh_at_start: refresh,
            ..HandLoad::default()
        };
    }

    /// Where the rule stands.
    pub fn step(&self) -> HandStep {
        self.step
    }

    /// Each step taken and when (ms).
    pub fn taken(&self) -> &[(HandStep, f64)] {
        &self.taken
    }

    /// One frame at `now_ms`: `second` a measuring second that ended
    /// with it, `refresh` the rate measured now, `watching` whether the
    /// hand is on and playing beside a running play (not before the
    /// song, not once it ended, not where it is off, unsupported or
    /// stopped). The step to take now, if one is due.
    pub fn frame(
        &mut self,
        now_ms: f64,
        second: Option<Second>,
        refresh: f64,
        watching: bool,
    ) -> Option<HandStep> {
        if !watching || self.step == HandStep::Gone {
            self.since = None;
            self.run = 0;
            return None;
        }
        let refresh = self.refresh_at_start.max(refresh);
        let since = *self.since.get_or_insert(now_ms);
        let s = second?;
        if s.worst_ms > HITCH_MS {
            self.since = Some(now_ms);
            self.run = 0;
            return None;
        }
        if now_ms - since < WARMUP_MS {
            return None;
        }
        if self.settle > 0 {
            self.settle -= 1;
            return None;
        }
        let late = s.fps < LATE_SHARE * refresh;
        self.judged += 1;
        self.late += u32::from(late);
        self.run = if late { self.run + 1 } else { 0 };
        if self.run < LATE_SECONDS {
            return None;
        }
        self.step = self.step.next();
        self.run = 0;
        self.settle = SETTLE_SECONDS;
        self.taken.push((self.step, now_ms));
        Some(self.step)
    }

    /// For the debug state.
    pub fn debug(&self) -> serde_json::Value {
        serde_json::json!({
            "rule": HAND_LOAD,
            "step": self.step.name(),
            "watching": self.since.is_some(),
            "late_run": self.run,
            "judged_seconds": self.judged,
            "late_seconds": self.late,
            "taken": self.taken.iter().map(|(s, t)| serde_json::json!({"step": s.name(), "at_ms": t})).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames at `hz` for `seconds` from `t` (ms), each second's rate
    /// `fps`: the steps the rule took, and when.
    fn run(
        rule: &mut HandLoad,
        t: &mut f64,
        seconds: u32,
        fps: f64,
        watching: bool,
    ) -> Vec<(HandStep, f64)> {
        let mut steps = Vec::new();
        for _ in 0..seconds {
            let n = fps.round() as u32;
            for i in 0..n {
                *t += 1000.0 / fps;
                let second = (i + 1 == n).then_some(Second {
                    fps,
                    worst_ms: 2000.0 / fps,
                });
                if let Some(s) = rule.frame(*t, second, 120.0, watching) {
                    steps.push((s, *t));
                }
            }
        }
        steps
    }

    #[test]
    fn a_quiet_play_takes_no_step() {
        let mut rule = HandLoad::default();
        rule.start(120.0);
        let mut t = 0.0;
        // A whole song at 120, and at 110 (8% of refreshes missed).
        assert!(run(&mut rule, &mut t, 90, 120.0, true).is_empty());
        assert!(run(&mut rule, &mut t, 90, 110.0, true).is_empty());
        assert_eq!(rule.step(), HandStep::Full);
        assert_eq!(rule.taken(), []);
    }

    #[test]
    fn steps_go_one_way_each_after_its_own_late_seconds() {
        let mut rule = HandLoad::default();
        rule.start(120.0);
        let mut t = 0.0;
        // The warm-up (2 s), then three late seconds: lighter.
        let steps = run(&mut rule, &mut t, 4, 100.0, true);
        assert_eq!(steps, []);
        let steps = run(&mut rule, &mut t, 1, 100.0, true);
        assert_eq!(steps.iter().map(|s| s.0).collect::<Vec<_>>(), [HandStep::Lighter]);
        // A second to settle, then three more: resting; then gone.
        assert_eq!(run(&mut rule, &mut t, 3, 100.0, true), []);
        assert_eq!(run(&mut rule, &mut t, 1, 100.0, true)[0].0, HandStep::Resting);
        assert_eq!(run(&mut rule, &mut t, 4, 100.0, true)[0].0, HandStep::Gone);
        // Never back, however quick the frames come after; nothing more.
        assert_eq!(run(&mut rule, &mut t, 30, 120.0, true), []);
        assert_eq!(run(&mut rule, &mut t, 30, 60.0, true), []);
        assert_eq!(rule.step(), HandStep::Gone);
        assert_eq!(rule.taken().len(), 3);
    }

    #[test]
    fn a_single_late_second_or_a_stall_takes_no_step() {
        let mut rule = HandLoad::default();
        rule.start(120.0);
        let mut t = 0.0;
        run(&mut rule, &mut t, 3, 120.0, true);
        // Late seconds broken by good ones never make a run of three.
        for _ in 0..20 {
            assert_eq!(run(&mut rule, &mut t, 2, 90.0, true), []);
            assert_eq!(run(&mut rule, &mut t, 1, 120.0, true), []);
        }
        // A stall (a hidden tab): the warm-up again, then the run counts.
        t += 400.0;
        let stalled = Second {
            fps: 60.0,
            worst_ms: 400.0,
        };
        assert_eq!(rule.frame(t, Some(stalled), 120.0, true), None);
        assert_eq!(run(&mut rule, &mut t, 2, 90.0, true), []);
        assert_eq!(rule.step(), HandStep::Full);
    }

    #[test]
    fn only_while_the_hand_plays_and_fresh_at_a_new_play() {
        let mut rule = HandLoad::default();
        rule.start(120.0);
        let mut t = 0.0;
        // Late, but the hand is off (or before the song): nothing.
        assert_eq!(run(&mut rule, &mut t, 20, 60.0, false), []);
        // Watched again: its warm-up first.
        assert_eq!(run(&mut rule, &mut t, 4, 60.0, true), []);
        assert_eq!(run(&mut rule, &mut t, 1, 60.0, true)[0].0, HandStep::Lighter);
        // A new play starts as always.
        rule.start(120.0);
        assert_eq!(rule.step(), HandStep::Full);
        assert_eq!(rule.taken(), []);
        // The refresh measured before the play holds when the frames
        // themselves fall to 60 (the meter's estimate following them).
        let mut late = 0;
        for i in 0..600 {
            t += 1000.0 / 60.0;
            let second = (i % 60 == 59).then_some(Second {
                fps: 60.0,
                worst_ms: 34.0,
            });
            late += usize::from(rule.frame(t, second, 60.0, true).is_some());
        }
        assert_eq!(late, 2, "{:?}", rule.taken());
    }
}
