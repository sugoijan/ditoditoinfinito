//! A CPU opponent beside a play (`docs/plans/multiplayer-prototype-progress.md`,
//! M3): its own [`Player`] on the chart's tap-only projection ([`project`]),
//! fed the analytical plan's seeded presses ([`Plan::script_at`]) and judged by
//! the normal judge, advanced to a judged song time as the play goes, as the
//! best play's ghost is. Deterministic per chart, level and seed: the same
//! three give the same presses and the same judgments at any frame rate.
//!
//! What it is not: a skill-tuned opponent. It plays at one of ten levels
//! (`super::levels`, [`CPU_LEVELS_V0`]): a strength relative to the chart
//! and, for the weaker five, a chance of leaving notes out where the
//! chart is demanding (`docs/plans/cpu-player.md`); it plays a
//! tap-only copy (holds and rolls as taps, mines, lifts and fakes left
//! out). A battle's shocks put into its play (M8b,
//! [`Opponent::receive_hazards`]) are judged by its own player as anyone's
//! are; its script of presses still to come responds to them by its level
//! alone (`super::mines`, [`super::MINE_RESPONSE_V0`]). Its life is its
//! own gauge under the ruleset it is given, judged, never drawn by hand.

use super::mines::{MINE_RESPONSE_V0, MineResponse, Purpose, draw};
use super::*;
use crate::battle::{Hazard, HazardError};
use crate::player::Player;
use crate::rules::{Ruleset, ScoreView};

/// Of a shock's window, how far a CPU's step on it lands from its middle,
/// either way.
const HIT_SPREAD: f64 = 0.6;

/// A CPU's play: the plan's presses on its own player.
pub struct Opponent {
    player: Player,
    presses: Vec<Press>,
    next: usize,
    pending: Vec<(f64, u8)>,
    judge_offset: f64,
    /// What it was judged since [`Opponent::take_judged`] last took it.
    judged: Vec<crate::judge::JudgeEvent>,
    /// Its seed (its responses' draws are keyed by it too).
    seed: u64,
    /// How well it avoids shocks, `0..=1`, and what it does about them.
    avoidance: f64,
    response: MineResponse,
}

impl Opponent {
    /// Chart `chart` of `song` (as played: shifted by a song offset like
    /// the human's) planned at `level` (an index into [`CPU_LEVELS_V0`]'s
    /// ten, 0 the weakest) with `seed`, readied under `ruleset` on its tap-only projection. Refused,
    /// with the engine's reason, where the CPU cannot plan the chart (not
    /// one-player four lanes, no playable rows, over the limits).
    pub fn new(
        song: &Song,
        chart: usize,
        ruleset: Ruleset,
        level: usize,
        seed: u64,
    ) -> Result<Opponent, String> {
        Self::with_skills(song, chart, ruleset, level, seed, Skills::default())
    }

    /// [`Opponent::new`] with a personality: `skills` bend its strength
    /// per pattern (`super::personality`).
    pub fn with_skills(
        song: &Song,
        chart: usize,
        ruleset: Ruleset,
        level: usize,
        seed: u64,
        skills: Skills,
    ) -> Result<Opponent, String> {
        let at = CPU_LEVELS_V0.level(level).ok_or("CPU level must be 0..9")?;
        Self::at(
            song,
            chart,
            ruleset,
            (at, MINE_RESPONSE_V0.avoidance(level)),
            seed,
            skills,
        )
    }

    /// [`Opponent::with_skills`] at a level of its own and an avoidance
    /// (`0..=1`): the ten levels' ([`CPU_LEVELS_V0`],
    /// [`MINE_RESPONSE_V0`]), or a study's candidates.
    pub fn at(
        song: &Song,
        chart: usize,
        ruleset: Ruleset,
        (level, avoidance): (Level, f64),
        seed: u64,
        skills: Skills,
    ) -> Result<Opponent, String> {
        Self::planned(
            &Plan::automatic(song, chart)?,
            song,
            chart,
            ruleset,
            (level, avoidance),
            seed,
            skills,
        )
    }

    /// [`Opponent::at`] on a plan already made for the chart (a study
    /// playing one chart many times).
    pub fn planned(
        plan: &Plan,
        song: &Song,
        chart: usize,
        ruleset: Ruleset,
        (level, avoidance): (Level, f64),
        seed: u64,
        skills: Skills,
    ) -> Result<Opponent, String> {
        let projected = plan.check_song(song, chart)?;
        let presses = plan.script_at(level, seed, skills)?;
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
            judged: Vec::new(),
            seed,
            avoidance,
            response: MINE_RESPONSE_V0.response(avoidance),
        })
    }

    /// How well it avoids shocks, `0..=1` (its level's,
    /// [`MineResponseTable::avoidance`](super::MineResponseTable::avoidance)).
    pub fn avoidance(&self) -> f64 {
        self.avoidance
    }

    pub fn mine_response(&self) -> MineResponse {
        self.response
    }

    /// TESTS AND STUDIES ONLY: another response than its level's
    /// ([`MineResponse::OFF`], the control).
    pub fn set_mine_response(&mut self, response: MineResponse) {
        self.response = response;
    }

    /// A battle's shocks put into its play (`Player::add_hazards`: judged
    /// as anyone's, refused as anyone's), and its presses still to come
    /// near them changed by its response: a shock it steps on is an extra
    /// press in the shock's window; its own presses within
    /// [`MineResponse::near`] of a shock move by up to
    /// [`MineResponse::jitter`] or are left out ([`MineResponse::slip`]).
    /// Every draw is keyed by its seed, the shock and the press, never by
    /// when this is called; presses already fed, or due before its judged
    /// time, are never changed, and a script that would not be valid is
    /// kept as it was.
    pub fn receive_hazards(&mut self, cells: &[Hazard]) -> Result<(), HazardError> {
        self.player.add_hazards(cells)?;
        if self.response.is_off() || cells.is_empty() {
            return Ok(());
        }
        let r = self.response;
        let window = self
            .player
            .hazards()
            .map_or(crate::battle::INJECTED_MINE_V0.fallback, |h| h.window());
        let mines: Vec<(Hazard, f64)> = cells
            .iter()
            .map(|c| (*c, self.player.timing().seconds_at(c.tick)))
            .collect();
        // Only what is still to come and clear of now.
        let now = self
            .player
            .judge()
            .last_time()
            .unwrap_or(f64::NEG_INFINITY)
            .max(
                self.presses[..self.next]
                    .last()
                    .map_or(f64::NEG_INFINITY, |p| p.press),
            )
            + self.judge_offset.max(0.0);
        let floor = now + r.jitter + 0.001;
        let mut changed: Vec<Press> = Vec::with_capacity(self.presses.len() - self.next + 8);
        for p in &self.presses[self.next..] {
            let near = mines
                .iter()
                .filter(|(_, at)| (p.press - at).abs() <= r.near)
                .min_by(|a, b| (p.press - a.1).abs().total_cmp(&(p.press - b.1).abs()));
            let (Some((cell, _)), true) = (near, p.press > floor) else {
                changed.push(*p);
                continue;
            };
            let key = ((p.press * 1e6).round() as i64 as u64) << 3 | u64::from(p.lane);
            if draw(self.seed, cell.tick.0, cell.lane, Purpose::Slip, key) < r.slip {
                continue;
            }
            let u = draw(self.seed, cell.tick.0, cell.lane, Purpose::Jitter, key);
            let press = (p.press + (2.0 * u - 1.0) * r.jitter).max(floor);
            changed.push(Press {
                press,
                release: press + 0.012,
                lane: p.lane,
            });
        }
        for (cell, at) in &mines {
            if draw(self.seed, cell.tick.0, cell.lane, Purpose::Hit, 0) >= r.hit {
                continue;
            }
            let u = draw(self.seed, cell.tick.0, cell.lane, Purpose::HitPlace, 0);
            let press = at + (2.0 * u - 1.0) * HIT_SPREAD * window;
            if press > floor {
                changed.push(Press {
                    press,
                    release: press + 0.012,
                    lane: cell.lane,
                });
            }
        }
        changed.sort_by(|a, b| a.press.total_cmp(&b.press).then(a.lane.cmp(&b.lane)));
        // Each lane's presses apart again (one moved onto another is left
        // out), after those already fed.
        let mut released = [f64::NEG_INFINITY; 4];
        for p in &self.presses[..self.next] {
            if let Some(r) = released.get_mut(usize::from(p.lane)) {
                *r = p.release;
            }
        }
        changed.retain(|p| {
            let Some(r) = released.get_mut(usize::from(p.lane)) else {
                return false;
            };
            if p.press < *r + 0.0001 {
                return false;
            }
            *r = p.release;
            true
        });
        let mut script = self.presses[..self.next].to_vec();
        script.extend(changed);
        if validate_script(&script).is_ok() {
            self.presses = script;
        }
        Ok(())
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
            let events = self.player.input_at(lane, pressed, at - self.judge_offset);
            self.judged.extend(events);
        }
        let events = self.player.update_at(t);
        self.judged.extend(events);
    }

    /// Its judgments since the last call, in the order they were made:
    /// what a view of its play is fed (`ddi_session::battle::feed`).
    pub fn take_judged(&mut self) -> Vec<crate::judge::JudgeEvent> {
        std::mem::take(&mut self.judged)
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
        // Played out at once: nothing to show of it.
        self.judged.clear();
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
        let a = run(&song, 7, 7, 1.0 / 60.0);
        let b = run(&song, 7, 7, 1.0 / 144.0);
        assert!(a.finished() && b.finished());
        assert_eq!(a.presses(), b.presses());
        assert_eq!(a.player().tally(), b.player().tally());
        assert_eq!(a.score(), b.score());
        assert_eq!(a.life(), b.life());
        assert_eq!(a.out_at(), b.out_at());
        // Played to the end at once: the same again.
        let mut c = Opponent::new(&song, 1, rules(), 7, 7).unwrap();
        c.start(0.0);
        c.finish();
        assert_eq!(c.player().tally(), a.player().tally());
        assert_eq!(c.score(), a.score());
        // Another seed presses differently.
        let d = Opponent::new(&song, 1, rules(), 7, 8).unwrap();
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
        assert!(mean(5) <= mean(9), "{} {}", mean(5), mean(9));
        assert!(mean(0) <= mean(5), "{} {}", mean(0), mean(5));
    }

    #[test]
    fn every_one_of_the_ten_levels_plays_the_same_at_any_frame_rate() {
        let song = song();
        for level in 0..LEVELS {
            let a = run(&song, level, 11, 1.0 / 60.0);
            let b = run(&song, level, 11, 1.0 / 144.0);
            assert_eq!(a.presses(), b.presses(), "{level}");
            assert_eq!(a.player().tally(), b.player().tally(), "{level}");
            assert_eq!(a.life(), b.life(), "{level}");
            assert_eq!(a.out_at(), b.out_at(), "{level}");
            validate_script(a.presses()).unwrap();
        }
    }

    /// Sections of shocks beatucada's chart has under `rules`, none
    /// overlapping (each eight beats, one every four).
    fn sections(song: &Song) -> Vec<crate::battle::Section> {
        let mut out: Vec<crate::battle::Section> = Vec::new();
        for s in crate::battle::schedule(song, 1, &rules().judge, 11)
            .into_iter()
            .flatten()
        {
            if out.last().is_none_or(|l| s.start_beat >= l.start_beat + 8) {
                out.push(s);
            }
        }
        out
    }

    /// An opponent at `level` with `seed` given the first `n` sections
    /// before it starts, stepped `step` seconds at a time to its end.
    fn shocked(song: &Song, level: usize, seed: u64, n: usize, step: f64) -> Opponent {
        let mut o = Opponent::new(song, 1, rules(), level, seed).unwrap();
        o.start(0.0);
        let mut t = -2.0;
        for s in sections(song).iter().take(n) {
            o.receive_hazards(&s.cells).unwrap();
        }
        while !o.finished() && t < 400.0 {
            t += step;
            o.advance(t);
        }
        o
    }

    #[test]
    fn shocks_are_judged_and_its_response_is_the_same_at_any_frame_rate() {
        let song = song();
        assert!(sections(&song).len() >= 4);
        let a = shocked(&song, 5, 3, 4, 1.0 / 60.0);
        let b = shocked(&song, 5, 3, 4, 1.0 / 144.0);
        assert_eq!(a.presses(), b.presses());
        assert_eq!(a.player().tally(), b.player().tally());
        assert_eq!(a.life(), b.life());
        let counts = a.player().hazards().unwrap().counts();
        assert_eq!(counts, b.player().hazards().unwrap().counts());
        // Every shock judged once (unless it went out first: judging stops).
        assert!(
            counts.0 + counts.1 == 32 || a.out_at().is_some(),
            "{counts:?}"
        );
        assert!(counts.0 + counts.1 > 8, "{counts:?}");
        validate_script(a.presses()).unwrap();
        // The control: nothing changes but the shocks being there.
        let mut off = Opponent::new(&song, 1, rules(), 5, 3).unwrap();
        off.set_mine_response(MineResponse::OFF);
        off.start(0.0);
        let plain = Opponent::new(&song, 1, rules(), 5, 3).unwrap();
        for s in sections(&song).iter().take(4) {
            off.receive_hazards(&s.cells).unwrap();
        }
        assert_eq!(off.presses(), plain.presses());
        off.finish();
        assert_eq!(off.player().hazards().unwrap().counts().0, 0, "passed");
    }

    #[test]
    fn a_poor_avoider_steps_on_more_shocks_than_a_good_one() {
        let song = song();
        let hits = |level: usize| -> u32 {
            (1..=24u64)
                .map(|seed| {
                    let mut o = Opponent::new(&song, 1, rules(), level, seed).unwrap();
                    assert_eq!(
                        o.mine_response(),
                        MINE_RESPONSE_V0.response(MINE_RESPONSE_V0.avoidance(level))
                    );
                    o.start(0.0);
                    for s in sections(&song).iter().take(4) {
                        o.receive_hazards(&s.cells).unwrap();
                    }
                    validate_script(o.presses()).unwrap();
                    o.finish();
                    o.player().hazards().unwrap().counts().0
                })
                .sum()
        };
        let (poor, good) = (hits(5), hits(9));
        assert!(poor > good, "{poor} {good}");
        // About 14% and 2% of 768.
        assert!((60..170).contains(&poor), "{poor}");
        assert!(good < 40, "{good}");
        assert_eq!(
            Opponent::new(&song, 1, rules(), 9, 1).unwrap().avoidance(),
            1.0
        );
        // The weakest steps on more still (30%; fewer judged where it
        // goes out first).
        let worst = hits(0);
        assert!(worst > poor, "{worst} {poor}");
    }

    /// Under DDI, the rooms' ruleset (`docs/plans/ddi-ruleset.md`, round
    /// 3), a shock a CPU steps on is a mine and locks its lane for a
    /// second, through the same judge as anyone's: it is in its mines'
    /// count and its points, its presses in the lane are ignored and what
    /// passes there is missed; the same at any frame rate. A CPU presses
    /// nothing stray (its extra press on a shock steps on it; its other
    /// presses are its notes'), so Boos cost it nothing.
    #[test]
    fn under_ddi_a_shock_locks_a_cpus_lane_and_what_passes_there_is_missed() {
        use crate::judge::NoteState;
        use crate::rules::ddi::{self, DdiOptions};
        // Its Hard chart: steps close enough for some to pass in a lock.
        const HARD: usize = 5;
        let song = song();
        let ddi = |boos: bool| {
            let mut r = ddi::ruleset(&DdiOptions {
                boos,
                ..Default::default()
            });
            // Never out: every shock is judged.
            r.fail = FailPolicy::Off;
            r
        };
        let sections: Vec<crate::battle::Section> = {
            let mut out: Vec<crate::battle::Section> = Vec::new();
            for s in crate::battle::schedule(&song, HARD, &ddi(false).judge, 11)
                .into_iter()
                .flatten()
            {
                if out.last().is_none_or(|l| s.start_beat >= l.start_beat + 8) {
                    out.push(s);
                }
            }
            out
        };
        assert!(sections.len() >= 4);
        let play = |seed: u64, step: f64, boos: bool| {
            // Level 1: the poorest avoider.
            let mut o = Opponent::new(&song, HARD, ddi(boos), 0, seed).unwrap();
            o.start(0.0);
            for s in sections.iter().take(4) {
                o.receive_hazards(&s.cells).unwrap();
            }
            let mut t = -2.0;
            while !o.finished() && t < 400.0 {
                t += step;
                o.advance(t);
            }
            o
        };
        let (mut stepped, mut passed_locked) = (0, 0);
        for seed in 1..=12u64 {
            let a = play(seed, 1.0 / 60.0, false);
            let b = play(seed, 1.0 / 144.0, false);
            assert_eq!(a.player().tally(), b.player().tally(), "{seed}");
            assert_eq!(a.score(), b.score(), "{seed}");
            assert_eq!(a.life(), b.life(), "{seed}");
            let p = a.player();
            let (hit, avoided) = p.hazards().unwrap().counts();
            assert_eq!(hit + avoided, 32, "every shock judged");
            // A mine, in its count (its copy has none of its own).
            assert_eq!(p.tally().mine_hit, hit, "{seed}");
            stepped += hit;
            let table = &p.ruleset().judge;
            let (window, reach) = (table.mine_window_seconds(), table.miss_after());
            let lock = table.lock.unwrap();
            for m in p.hazards().unwrap().mines() {
                if m.state != crate::battle::HazardState::Hit {
                    continue;
                }
                // Locked from the hit (within the shock's window) for the
                // lock's second.
                let until = p.judge().locked_until(m.lane).unwrap();
                assert!(until >= m.seconds - window + lock - 1e-9, "{seed}");
                // Every note of the lane whose windows are wholly inside
                // the lock, whenever in its window the hit came: missed.
                for n in p.judge().notes().iter().filter(|n| {
                    n.note.lane == m.lane
                        && n.seconds - reach > m.seconds + window
                        && n.seconds + reach < m.seconds - window + lock
                }) {
                    passed_locked += 1;
                    assert_eq!(n.state, NoteState::Missed, "{seed}: {n:?}");
                }
            }
            // With Boos on, the same play: none counted against it.
            let c = play(seed, 1.0 / 60.0, true);
            assert_eq!(c.player().tally().boo, 0, "{seed}");
            assert_eq!(c.player().tally().mine_hit, hit, "{seed}");
        }
        assert!(stepped > 40, "the poorest avoider steps on many: {stepped}");
        assert!(passed_locked > 0, "notes passed in a locked lane");
    }

    #[test]
    fn shocks_too_late_are_refused_and_change_nothing() {
        let song = song();
        let s = sections(&song);
        let mut o = Opponent::new(&song, 1, rules(), 5, 3).unwrap();
        o.start(0.0);
        let first = o.player().timing().seconds_at(s[0].cells[0].tick);
        o.advance(first + 1.0);
        let before = o.presses().to_vec();
        assert_eq!(
            o.receive_hazards(&s[0].cells),
            Err(crate::battle::HazardError::Late)
        );
        assert_eq!(o.presses(), &before[..]);
    }

    #[test]
    fn charts_the_cpu_cannot_plan_are_refused() {
        let song = song();
        // Out of range, and a chart that is not one-player four lanes.
        assert!(Opponent::new(&song, 99, rules(), 7, 1).is_err());
        let double = song
            .charts
            .iter()
            .position(|c| song.layout_of(c).is_some_and(|l| l.lanes.len() != 4));
        if let Some(i) = double {
            assert!(Opponent::new(&song, i, rules(), 7, 1).is_err());
        }
        assert!(
            Opponent::new(&song, 1, rules(), 10, 1).is_err(),
            "no level 11"
        );
    }
}
