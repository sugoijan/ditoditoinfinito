//! A CPU opponent beside a play (`docs/plans/multiplayer-prototype-progress.md`,
//! M3): its own [`Player`] on the chart's tap-only projection ([`project`]),
//! fed the analytical plan's seeded presses ([`Plan::script`]) and judged by
//! the normal judge, advanced to a judged song time as the play goes, as the
//! best play's ghost is. Deterministic per chart, level and seed: the same
//! three give the same presses and the same judgments at any frame rate.
//!
//! What it is not: a skill-tuned opponent. Its strength is the engine's
//! level relative to the chart (`docs/plans/cpu-player.md`); it plays a
//! tap-only copy (holds and rolls as taps, mines, lifts and fakes left
//! out) and responds to nothing (no mines, no attacks). Its life is its
//! own gauge under the ruleset it is given, judged, never drawn by hand.

use super::*;
use crate::player::Player;
use crate::rules::{Ruleset, ScoreView};

/// A CPU's play: the plan's presses on its own player.
pub struct Opponent {
    player: Player,
    presses: Vec<Press>,
    next: usize,
    pending: Vec<(f64, u8)>,
    judge_offset: f64,
}

impl Opponent {
    /// Chart `chart` of `song` (as played: shifted by a song offset like
    /// the human's) planned at `level` (an index into [`STRENGTHS`]) with
    /// `seed`, readied under `ruleset` on its tap-only projection. Refused,
    /// with the engine's reason, where the CPU cannot plan the chart (not
    /// one-player four lanes, no playable rows, over the limits).
    pub fn new(
        song: &Song,
        chart: usize,
        ruleset: Ruleset,
        level: usize,
        seed: u64,
    ) -> Result<Opponent, String> {
        let plan = Plan::automatic(song, chart)?;
        let projected = plan.check_song(song, chart)?;
        let presses = plan.script(level, seed)?;
        let judge_offset = ruleset.judge.judge_offset;
        let mut player = Player::new(
            &projected,
            chart,
            ruleset,
            crate::player::PlayOptions::default(),
        );
        player.set_autoplay(true);
        Ok(Opponent {
            player,
            presses,
            next: 0,
            pending: Vec::new(),
            judge_offset,
        })
    }

    /// Starts it with the play, from song second `song_offset` (presses
    /// before it are not played).
    pub fn start(&mut self, song_offset: f64) {
        self.player.start(0.0, song_offset);
        self.next = self.presses.partition_point(|p| p.press < song_offset);
    }

    /// Judged up to judged song second `t` (a play's `judge_time`): the
    /// presses and releases due by then fed at their own times, then the
    /// player updated, as a replay is. `t` only goes forward.
    pub fn advance(&mut self, t: f64) {
        let mut due = Vec::new();
        while let Some(p) = self
            .presses
            .get(self.next)
            .filter(|p| p.press - self.judge_offset <= t)
        {
            due.push((p.press, p.lane, true));
            self.pending.push((p.release, p.lane));
            self.next += 1;
        }
        let offset = self.judge_offset;
        self.pending.retain(|&(release, lane)| {
            if release - offset <= t {
                due.push((release, lane, false));
                false
            } else {
                true
            }
        });
        due.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (at, lane, pressed) in due {
            self.player.input_at(lane, pressed, at - self.judge_offset);
        }
        self.player.update_at(t);
    }

    /// Played to its end at once (nothing it does depends on anyone
    /// else): every press fed and every note judged.
    pub fn finish(&mut self) {
        let end = self
            .presses
            .last()
            .map_or(f64::NEG_INFINITY, |p| p.release - self.judge_offset)
            .max(self.player.end_time())
            + 1.0;
        self.advance(end);
        // A hold's outcome shown past the end, at most a few seconds.
        let mut t = end;
        while !self.player.finished() && t < end + 30.0 {
            t += 1.0;
            self.player.update_at(t);
        }
    }

    /// Its player (read only).
    pub fn player(&self) -> &Player {
        &self.player
    }

    /// The presses its plan made, in time order.
    pub fn presses(&self) -> &[Press] {
        &self.presses
    }

    /// Its gauge's life, `0..=1`.
    pub fn life(&self) -> f32 {
        self.player.ruleset().gauge.life()
    }

    /// When its gauge first emptied (judged song seconds).
    pub fn out_at(&self) -> Option<f64> {
        self.player.gauge_failed_at()
    }

    pub fn combo(&self) -> u32 {
        self.player.combo()
    }

    pub fn score(&self) -> ScoreView {
        self.player.ruleset().score.view()
    }

    pub fn finished(&self) -> bool {
        self.player.finished()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{FailPolicy, presets};

    fn song() -> Song {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
        let path = std::fs::read_dir(path)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "ssc"))
            .unwrap();
        ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc").unwrap()
    }

    fn rules() -> Ruleset {
        let mut r = presets::itg();
        r.fail = FailPolicy::Immediate;
        r
    }

    /// The same chart, level and seed play the same, whatever the frames.
    fn run(song: &Song, level: usize, seed: u64, step: f64) -> Opponent {
        let mut o = Opponent::new(song, 1, rules(), level, seed).unwrap();
        o.start(0.0);
        let mut t = -2.0;
        while !o.finished() && t < 400.0 {
            t += step;
            o.advance(t);
        }
        o
    }

    #[test]
    fn deterministic_per_chart_level_and_seed_at_any_frame_rate() {
        let song = song();
        let a = run(&song, 2, 7, 1.0 / 60.0);
        let b = run(&song, 2, 7, 1.0 / 144.0);
        assert!(a.finished() && b.finished());
        assert_eq!(a.presses(), b.presses());
        assert_eq!(a.player().tally(), b.player().tally());
        assert_eq!(a.score(), b.score());
        assert_eq!(a.life(), b.life());
        assert_eq!(a.out_at(), b.out_at());
        // Played to the end at once: the same again.
        let mut c = Opponent::new(&song, 1, rules(), 2, 7).unwrap();
        c.start(0.0);
        c.finish();
        assert_eq!(c.player().tally(), a.player().tally());
        assert_eq!(c.score(), a.score());
        // Another seed presses differently.
        let d = Opponent::new(&song, 1, rules(), 2, 8).unwrap();
        assert_ne!(d.presses(), a.presses());
        // Every press judged by the normal judge: nothing is skipped.
        let judged: u32 = a.player().tally().taps.iter().sum();
        assert!(judged > 32, "{:?}", a.player().tally());
    }

    #[test]
    fn a_weaker_level_scores_no_better_on_average() {
        let song = song();
        let mean = |level| {
            (1..=4)
                .map(|seed| {
                    let mut o = Opponent::new(&song, 1, rules(), level, seed).unwrap();
                    o.start(0.0);
                    o.finish();
                    o.score().percent.unwrap_or(0.0)
                })
                .sum::<f64>()
                / 4.0
        };
        assert!(mean(0) <= mean(4), "{} {}", mean(0), mean(4));
    }

    #[test]
    fn charts_the_cpu_cannot_plan_are_refused() {
        let song = song();
        // Out of range, and a chart that is not one-player four lanes.
        assert!(Opponent::new(&song, 99, rules(), 2, 1).is_err());
        let double = song
            .charts
            .iter()
            .position(|c| song.layout_of(c).is_some_and(|l| l.lanes.len() != 4));
        if let Some(i) = double {
            assert!(Opponent::new(&song, i, rules(), 2, 1).is_err());
        }
        assert!(
            Opponent::new(&song, 1, rules(), 9, 1).is_err(),
            "no level 9"
        );
    }
}
