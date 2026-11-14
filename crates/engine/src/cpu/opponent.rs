//! A CPU opponent beside a play (`docs/plans/multiplayer-prototype-progress.md`,
//! M3): its own [`Player`] on the chart's tap-only projection ([`project`]),
//! fed the analytical plan's seeded presses ([`Plan::script_at`]) and judged by
//! the normal judge, advanced to a judged song time as the play goes, as the
//! best play's ghost is. Deterministic per chart, level and seed: the same
//! three give the same presses and the same judgments at any frame rate.
//!
//! What it is not: a skill-tuned opponent. It plays at one of ten levels
//! (`super::levels`, [`CPU_LEVELS`]): abilities in real units, the same on
//! every chart (`cpu-levels-v1`, `super::absolute`), or the earlier
//! strength relative to the chart with, for the weaker five, a chance of
//! leaving notes out where the chart is demanding (`cpu-levels-v0`;
//! `docs/plans/cpu-player.md`); it plays a
//! tap-only copy (holds and rolls as taps, mines, lifts and fakes left
//! out). A battle's shocks put into its play (M8b,
//! [`Opponent::receive_hazards`]) are judged by its own player as anyone's
//! are; its script of presses still to come responds to them by its level
//! and its skills' Evasion (`super::mines`, [`super::MINE_RESPONSE_V1`]). Its life is its
//! own gauge under the ruleset it is given, judged, never drawn by hand.

use super::mines::{MINE_RESPONSE_V1, MineResponse, Purpose, draw};
use super::*;
use crate::battle::{Hazard, HazardError};
use crate::player::Player;
use crate::rules::{Ruleset, ScoreView};

/// Of a shock's window, how far a CPU's step on it lands from its middle,
/// either way.
const HIT_SPREAD: f64 = 0.6;

/// A rung's presses and its notes `(seconds, lane)` in time order.
type RungPlan = (Vec<Press>, Vec<(f64, u8)>);

/// A rung of a stage's ladder as the CPUs play it (`crate::ladder`): its
/// plan, its tap-only copy and its notes, made once for every CPU of the
/// stage ([`Opponent::add_rungs`]).
pub struct PlannedRung {
    pub chart: usize,
    pub plan: Plan,
    pub projected: Song,
    /// Its copy's notes, `(seconds, lane)` in time order.
    pub notes: Vec<(f64, u8)>,
    /// A rung above the stage's chart (harder); below it otherwise.
    pub up: bool,
}

/// A stage's ladder on chart `chart` of `song` as everyone plays it
/// (`crate::ladder::Ladder`), each way cut at the first rung the CPUs
/// cannot plan, each rung planned: the easier rungs, the next first,
/// then the harder ones (numbered so from 1, as `Ladder::rungs`); the
/// same on every device.
pub fn planned_ladder(song: &Song, chart: usize) -> Vec<PlannedRung> {
    let below = crate::ladder::below(song, chart)
        .into_iter()
        .map_while(|r| PlannedRung::new(song, r).ok());
    let above = crate::ladder::above(song, chart)
        .into_iter()
        .map_while(|r| PlannedRung::new(song, r).ok())
        .map(|mut r| {
            r.up = true;
            r
        });
    below.chain(above).collect()
}

/// The ladder `planned_ladder` keeps, as charts.
pub fn planned_charts(stage: usize, rungs: &[PlannedRung]) -> crate::ladder::Ladder {
    crate::ladder::Ladder {
        stage,
        below: rungs.iter().filter(|r| !r.up).map(|r| r.chart).collect(),
        above: rungs.iter().filter(|r| r.up).map(|r| r.chart).collect(),
    }
}

impl PlannedRung {
    /// Chart `chart` of `song` planned; refused where the CPU cannot
    /// plan it.
    pub fn new(song: &Song, chart: usize) -> Result<PlannedRung, String> {
        let plan = Plan::automatic(song, chart)?;
        let projected = plan.check_song(song, chart)?;
        let rung = Player::new(
            &projected,
            chart,
            crate::rules::presets::ddi(),
            crate::player::PlayOptions::default(),
        );
        let mut notes: Vec<(f64, u8)> = rung
            .judge()
            .notes()
            .iter()
            .filter(|n| n.judgeable && n.seconds.is_finite())
            .map(|n| (n.seconds, n.note.lane))
            .collect();
        notes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Ok(PlannedRung {
            chart,
            plan,
            projected,
            notes,
            up: false,
        })
    }
}

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
    /// How well it avoids shocks, `0..=1`, and what it does about them
    /// (its level's response).
    avoidance: f64,
    response: MineResponse,
    /// Its skills' Evasion, which bends that response shock by shock
    /// ([`MINE_RESPONSE_V1`]), and its chart's notes, `(seconds, lane)`
    /// in time order, which say where a shock is hard to get around.
    evasion: f64,
    notes: Vec<(f64, u8)>,
    /// Its strength and skills, which its plans for the other charts are
    /// made at ([`Opponent::add_rungs`]).
    level: Strength,
    skills: Skills,
    /// The presses its plan made for each rung of its ladder (numbered as
    /// `Ladder::rungs`: the easier ones, then the harder ones), and each
    /// one's notes as `notes` has them; and the stage's chart's, to step
    /// back up to.
    rungs: Vec<RungPlan>,
    home: Option<RungPlan>,
    /// The battle's shocks received so far (its response to them is made
    /// again on a new rung's presses).
    received: Vec<Hazard>,
}

impl Opponent {
    /// Chart `chart` of `song` (as played: shifted by a song offset like
    /// the human's) planned at `level` (an index into [`CPU_LEVELS`]' ten,
    /// 0 the weakest) with `seed`, readied under `ruleset` on its tap-only projection. Refused,
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

    /// [`Opponent::new`] with a personality: `skills` bend its abilities
    /// per pattern (`super::personality`, `super::absolute`). At one of
    /// the ten levels opponents play at ([`CPU_LEVELS`], `cpu-levels-v1`:
    /// abilities the same on every chart).
    pub fn with_skills(
        song: &Song,
        chart: usize,
        ruleset: Ruleset,
        level: usize,
        seed: u64,
        skills: Skills,
    ) -> Result<Opponent, String> {
        Self::on_scale(song, chart, ruleset, CPU_LEVELS, level, seed, skills)
    }

    /// [`Opponent::with_skills`] on the levels of `scale` (the earlier
    /// `cpu-levels-v0`, strength relative to the chart, for comparison
    /// and its fixtures).
    pub fn on_scale(
        song: &Song,
        chart: usize,
        ruleset: Ruleset,
        scale: LevelScale,
        level: usize,
        seed: u64,
        skills: Skills,
    ) -> Result<Opponent, String> {
        let at = scale.strength(level).ok_or("CPU level must be 0..9")?;
        Self::at(
            song,
            chart,
            ruleset,
            (at, MINE_RESPONSE_V0.avoidance(level)),
            seed,
            skills,
        )
    }

    /// [`Opponent::with_skills`] at a strength of its own and an avoidance
    /// (`0..=1`): the ten levels' ([`CPU_LEVELS_V1`], [`CPU_LEVELS_V0`],
    /// [`MINE_RESPONSE_V0`]), or a study's candidates.
    pub fn at(
        song: &Song,
        chart: usize,
        ruleset: Ruleset,
        (level, avoidance): (impl Into<Strength>, f64),
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
        (level, avoidance): (impl Into<Strength>, f64),
        seed: u64,
        skills: Skills,
    ) -> Result<Opponent, String> {
        let level: Strength = level.into();
        let projected = plan.check_song(song, chart)?;
        let presses = plan.script_strength(level, seed, skills)?;
        let judge_offset = ruleset.judge.judge_offset;
        let mut player = Player::new(
            &projected,
            chart,
            ruleset,
            crate::player::PlayOptions::default(),
        );
        player.set_autoplay(true);
        let mut notes: Vec<(f64, u8)> = player
            .judge()
            .notes()
            .iter()
            .filter(|n| n.judgeable && n.seconds.is_finite())
            .map(|n| (n.seconds, n.note.lane))
            .collect();
        notes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Ok(Opponent {
            player,
            presses,
            next: 0,
            pending: Vec::new(),
            judge_offset,
            judged: Vec::new(),
            seed,
            avoidance,
            response: MINE_RESPONSE_V1.table.response(avoidance),
            evasion: skills.evasion(),
            notes,
            level,
            skills,
            rungs: Vec::new(),
            home: None,
            received: Vec::new(),
        })
    }

    /// Its ladder (`crate::ladder`, a match's "At zero life: Step
    /// down"): the rungs, the next easier first, each planned at its own
    /// level relative to that chart (the rung's plan, [`PlannedRung`])
    /// with its seed, its tap-only copy a rung of its player's. Before the
    /// start. Refused, with the engine's reason, where a rung's plan
    /// cannot be scripted.
    pub fn add_rungs(&mut self, rungs: &[PlannedRung]) -> Result<(), String> {
        if self.home.is_none() && !rungs.is_empty() {
            self.home = Some((self.presses.clone(), self.notes.clone()));
        }
        for r in rungs {
            let presses = r.plan.script_strength(self.level, self.seed, self.skills)?;
            if r.up {
                self.player.add_rung_up(&r.projected, r.chart)?;
            } else {
                self.player.add_rung(&r.projected, r.chart)?;
            }
            self.rungs.push((presses, r.notes.clone()));
        }
        Ok(())
    }

    /// The presses and notes its plan made for rung `rung` (0 the
    /// stage's chart).
    fn rung_plan(&self, rung: u8) -> Option<&RungPlan> {
        match rung {
            0 => self.home.as_ref(),
            r => self.rungs.get(usize::from(r) - 1),
        }
    }

    /// Whether it has a ladder ([`Opponent::add_rungs`]).
    fn has_ladder(&self) -> bool {
        !self.rungs.is_empty()
    }

    /// Its player stepped to a rung it was not playing yet: the presses
    /// still to come are the rung's plan's from just before its boundary
    /// (after a step up, those of the old chart's still to come before it
    /// kept as they were), its response to the shocks made again on the
    /// new rung's presses.
    fn follow_step(&mut self, played: usize) {
        let steps = self.player.steps();
        if steps.len() <= played {
            return;
        }
        let Some(step) = steps.last().copied() else {
            return;
        };
        let Some((presses, notes)) = self.rung_plan(step.rung).cloned() else {
            return;
        };
        let early = self.player.ruleset().judge.max_early();
        let from = step.boundary - early - self.judge_offset.max(0.0) - 0.001;
        let keep = if step.up {
            // The old chart is played to the boundary.
            self.next + self.presses[self.next..].partition_point(|p| p.press < from)
        } else {
            self.next
        };
        let mut script = self.presses[..keep].to_vec();
        let last = script.last().map_or(f64::NEG_INFINITY, |p| p.release);
        script.extend(presses.into_iter().filter(|p| p.press >= from && p.press > last));
        if validate_script(&script).is_ok() {
            self.presses = script;
        } else {
            self.presses.truncate(keep);
        }
        self.notes = notes;
        let cells = std::mem::take(&mut self.received);
        let hits_from = if step.up { from } else { f64::NEG_INFINITY };
        self.respond_in(&cells, keep, hits_from);
        self.received = cells;
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
        if self.has_ladder() {
            self.received.extend_from_slice(cells);
        }
        self.respond(cells);
        Ok(())
    }

    /// Its response to `cells` on the presses still to come
    /// ([`Opponent::receive_hazards`]).
    fn respond(&mut self, cells: &[Hazard]) {
        self.respond_in(cells, self.next, f64::NEG_INFINITY);
    }

    /// [`Opponent::respond`] on the presses from `keep` on alone (those
    /// before it as they are), its steps on the shocks no earlier than
    /// `hits_from` (after a step up: the new rung's part alone).
    fn respond_in(&mut self, cells: &[Hazard], keep: usize, hits_from: f64) {
        if self.response.is_off() || cells.is_empty() {
            return;
        }
        let r = self.response;
        let model = MINE_RESPONSE_V1;
        let window = self
            .player
            .hazards()
            .map_or(crate::battle::INJECTED_MINE_V0.fallback, |h| h.window());
        // Each shock with its own response: the level's bent by its
        // Evasion where the shock is hard to get around.
        let mines: Vec<(Hazard, f64, MineResponse)> = cells
            .iter()
            .map(|c| {
                let at = self.player.timing().seconds_at(c.tick);
                let context = model.context(&self.notes, at, c.lane);
                (*c, at, model.response(r, self.evasion, context))
            })
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
        let jitter = mines.iter().map(|m| m.2.jitter).fold(r.jitter, f64::max);
        let floor = now + jitter + 0.001;
        let keep = keep.clamp(self.next, self.presses.len());
        let mut changed: Vec<Press> = Vec::with_capacity(self.presses.len() - keep + 8);
        for p in &self.presses[keep..] {
            let near = mines
                .iter()
                .filter(|(_, at, _)| (p.press - at).abs() <= r.near)
                .min_by(|a, b| (p.press - a.1).abs().total_cmp(&(p.press - b.1).abs()));
            let (Some((cell, _, r)), true) = (near, p.press > floor) else {
                changed.push(*p);
                continue;
            };
            let key = ((p.press * 1e6).round() as i64 as u64) << 3 | u64::from(p.lane);
            if draw(self.seed, cell.tick.0, cell.lane, Purpose::Slip, key) < r.slip {
                continue;
            }
            let u = draw(self.seed, cell.tick.0, cell.lane, Purpose::Jitter, key);
            let press = (p.press + (2.0 * u - 1.0) * r.jitter).max(floor).max(hits_from);
            changed.push(Press {
                press,
                release: press + 0.012,
                lane: p.lane,
            });
        }
        for (cell, at, r) in &mines {
            if draw(self.seed, cell.tick.0, cell.lane, Purpose::Hit, 0) >= r.hit {
                continue;
            }
            let u = draw(self.seed, cell.tick.0, cell.lane, Purpose::HitPlace, 0);
            let press = at + (2.0 * u - 1.0) * HIT_SPREAD * window;
            if press > floor && press >= hits_from {
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
        for p in &self.presses[..keep] {
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
        let mut script = self.presses[..keep].to_vec();
        script.extend(changed);
        if validate_script(&script).is_ok() {
            self.presses = script;
        }
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
        if self.has_ladder() {
            return self.advance_on_ladder(t);
        }
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

    /// [`Opponent::advance`] with a ladder: each edge's time judged to
    /// before it is fed, so that a step down is found where its life ran
    /// out whatever the frames, and the old chart's presses after it are
    /// never fed (the new rung's are, [`Opponent::follow_step`]).
    fn advance_on_ladder(&mut self, t: f64) {
        loop {
            let offset = self.judge_offset;
            let press = self
                .presses
                .get(self.next)
                .map(|p| p.press - offset)
                .filter(|&at| at <= t);
            let release = self
                .pending
                .iter()
                .enumerate()
                .map(|(i, &(r, _))| (i, r - offset))
                .filter(|&(_, at)| at <= t)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            // The earliest edge due: a release before a press at once.
            let (at, release) = match (press, release) {
                (Some(p), Some((i, r))) if r <= p => (r, Some(i)),
                (Some(p), _) => (p, None),
                (None, Some((i, r))) => (r, Some(i)),
                (None, None) => break,
            };
            let played = self.player.steps().len();
            let events = self.player.update_at(at + crate::player::JUDGE_LAG);
            self.judged.extend(events);
            if self.player.steps().len() > played {
                self.follow_step(played);
                // A press of the old chart's after the step is never fed
                // (a release is: its key is down).
                if release.is_none() {
                    continue;
                }
            }
            let (lane, pressed) = match release {
                Some(i) => (self.pending.remove(i).1, false),
                None => {
                    let p = self.presses[self.next];
                    self.pending.push((p.release, p.lane));
                    self.next += 1;
                    (p.lane, true)
                }
            };
            let played = self.player.steps().len();
            let events = self.player.input_at(lane, pressed, at);
            self.judged.extend(events);
            self.follow_step(played);
        }
        let played = self.player.steps().len();
        let events = self.player.update_at(t);
        self.judged.extend(events);
        self.follow_step(played);
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

    /// The ladder rule its play steps by (`ladder::ADAPTIVE` unless set;
    /// the studies compare `adaptive-v0`).
    pub fn set_ladder_rule(&mut self, rule: crate::ladder::AdaptiveRule) {
        self.player.set_ladder_rule(rule);
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

    /// Its fail policy from now on (`Player::set_fail`): a battle's
    /// decided winner plays the song out whatever its gauge does
    /// (`FailPolicy::Off`). Its presses are its plan's, unchanged.
    pub fn set_fail(&mut self, fail: crate::rules::FailPolicy) {
        self.player.set_fail(fail);
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

    /// A CPU with a ladder (a match's "At zero life: Step down") steps
    /// down where its life runs out and plays the next rung's plan, the
    /// same at any frame rate; without one it plays as ever.
    #[test]
    fn a_cpu_steps_down_its_ladder_the_same_at_any_frame_rate() {
        use crate::rules::ddi::{self, DdiOptions};
        const HARD: usize = 5;
        let song = song();
        let rules = || {
            let mut r = ddi::ruleset(&DdiOptions::default());
            r.fail = FailPolicy::Immediate;
            r
        };
        let rungs = crate::ladder::below(&song, HARD);
        assert!(!rungs.is_empty());
        let play = |seed: u64, step: f64, ladder: bool| {
            let mut o = Opponent::new(&song, HARD, rules(), 0, seed).unwrap();
            if ladder {
                let planned: Vec<PlannedRung> =
                    rungs.iter().map(|&r| PlannedRung::new(&song, r).unwrap()).collect();
                o.add_rungs(&planned).unwrap();
            }
            o.start(0.0);
            for s in sections(&song).iter().take(4) {
                let _ = o.receive_hazards(&s.cells);
            }
            let mut t = -2.0;
            while !o.finished() && t < 400.0 {
                t += step;
                o.advance(t);
            }
            o
        };
        let mut stepped = 0;
        for seed in 1..=6u64 {
            let a = play(seed, 1.0 / 60.0, true);
            let b = play(seed, 1.0 / 144.0, true);
            let (ra, rb) = (a.player().results(), b.player().results());
            assert_eq!(ra.steps, rb.steps, "{seed}");
            assert_eq!(ra.tally, rb.tally, "{seed}");
            assert_eq!(ra.life, rb.life, "{seed}");
            assert_eq!(a.presses(), b.presses(), "{seed}");
            validate_script(a.presses()).unwrap();
            stepped += ra.steps.len();
            // Without its ladder: out where it was, as before.
            let plain = play(seed, 1.0 / 60.0, false);
            let alone = run_ddi(&song, HARD, seed);
            assert_eq!(plain.player().results().tally, alone.player().results().tally);
            if let Some(first) = ra.steps.first() {
                assert!(plain.out_at().is_some_and(|o| (o - first.at).abs() < 1e-9));
            }
        }
        assert!(stepped > 0, "the weakest level steps down on Hard");
    }

    /// A strong CPU with a full ladder (a match's "Charts: Adaptive")
    /// climbs above the stage's chart, the same at any frame rate, with
    /// shocks put in on the way.
    #[test]
    fn a_strong_cpu_climbs_its_ladder_the_same_at_any_frame_rate() {
        use crate::rules::ddi::{self, DdiOptions};
        let song = song();
        let medium = song
            .charts
            .iter()
            .position(|c| c.layout == "dance-single" && c.difficulty == ddi_chart::Difficulty::Medium)
            .unwrap();
        let planned = planned_ladder(&song, medium);
        assert!(planned.iter().any(|r| r.up));
        let ladder = planned_charts(medium, &planned);
        let play = |seed: u64, step: f64| {
            let mut r = ddi::ruleset(&DdiOptions::default());
            r.fail = FailPolicy::Immediate;
            let mut o = Opponent::new(&song, medium, r, 9, seed).unwrap();
            o.add_rungs(&planned).unwrap();
            o.start(0.0);
            for s in sections(&song).iter().take(4) {
                let _ = o.receive_hazards(&s.cells);
            }
            let mut t = -2.0;
            while !o.finished() && t < 400.0 {
                t += step;
                o.advance(t);
            }
            o
        };
        let mut up = 0;
        for seed in 1..=4u64 {
            let a = play(seed, 1.0 / 60.0);
            let b = play(seed, 1.0 / 144.0);
            let (ra, rb) = (a.player().results(), b.player().results());
            assert_eq!(ra.steps, rb.steps, "{seed}");
            assert_eq!(ra.tally, rb.tally, "{seed}");
            assert_eq!(ra.life, rb.life, "{seed}");
            assert_eq!(a.presses(), b.presses(), "{seed}");
            validate_script(a.presses()).unwrap();
            for s in ra.steps.iter().filter(|s| s.up) {
                assert_eq!(ladder.chart(s.rung), Some(s.chart));
                assert!(ladder.level(s.rung) > ladder.level(s.from));
                up += 1;
            }
        }
        assert!(up > 0, "the strongest level climbs from Medium");
    }

    fn run_ddi(song: &Song, chart: usize, seed: u64) -> Opponent {
        use crate::rules::ddi::{self, DdiOptions};
        let mut r = ddi::ruleset(&DdiOptions::default());
        r.fail = FailPolicy::Immediate;
        let mut o = Opponent::new(song, chart, r, 0, seed).unwrap();
        o.start(0.0);
        for s in sections(song).iter().take(4) {
            let _ = o.receive_hazards(&s.cells);
        }
        let mut t = -2.0;
        while !o.finished() && t < 400.0 {
            t += 1.0 / 30.0;
            o.advance(t);
        }
        o
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

    /// What every level does about four sections of shocks, by seed: a
    /// hash of its presses then, its shocks stepped on and its tally.
    fn shock_fixture(song: &Song, skills: Skills) -> Vec<(usize, u64, u64, u32, u32)> {
        let mut out = Vec::new();
        for level in 0..super::LEVELS {
            for seed in 1..=4u64 {
                let mut o = Opponent::on_scale(
                    song,
                    1,
                    rules(),
                    LevelScale::RelativeV0,
                    level,
                    seed,
                    skills,
                )
                .unwrap();
                o.start(0.0);
                for s in sections(song).iter().take(4) {
                    o.receive_hazards(&s.cells).unwrap();
                }
                let mut h = 0xcbf2_9ce4_8422_2325_u64;
                for p in o.presses() {
                    for v in [p.press.to_bits(), p.release.to_bits(), u64::from(p.lane)] {
                        h = (h ^ v).wrapping_mul(0x0100_0000_01b3);
                    }
                }
                o.finish();
                let hit = o.player().hazards().unwrap().counts().0;
                let missed = o.player().tally().taps[5];
                out.push((level, seed, h, hit, missed));
            }
        }
        out
    }

    /// Evasion 1 leaves a level's response exactly as it was: with no
    /// eighth value (every seed and roster CPU before round 4) a CPU's
    /// presses after four sections of shocks, the shocks it steps on and
    /// its misses are as `mine-response-v0` made them (the fixture taken
    /// from the build before; balanced, and a roster CPU's own seven).
    #[test]
    fn evasion_one_responds_exactly_as_mine_response_v0_did() {
        let song = song();
        for (skills, want) in [
            ("balanced", BALANCED_V0),
            ("1.1,1,1,0.9,1.2,1,0.8", CUSTOM_V0),
        ] {
            let got = shock_fixture(&song, Skills::parse(skills).unwrap());
            assert_eq!(got, want, "{skills}");
        }
        // And an eighth value of 1 is the same CPU.
        assert_eq!(
            shock_fixture(&song, Skills::parse("1,1,1,1,1,1,1,1").unwrap()),
            BALANCED_V0
        );
        // A dodger steps on fewer shocks than the level, a stomper on more.
        let stepped = |e: f64| -> u32 {
            shock_fixture(&song, Skills::default().with_evasion(e).unwrap())
                .iter()
                .map(|f| f.3)
                .sum()
        };
        let (dodger, level, stomper) = (stepped(2.0), stepped(1.0), stepped(0.5));
        assert!(
            dodger < level && level < stomper,
            "{dodger} {level} {stomper}"
        );
    }

    /// `(level, seed, presses' hash, shocks stepped on, misses)`.
    const BALANCED_V0: [(usize, u64, u64, u32, u32); 40] = [
        (0, 1, 12644587548427991872, 5, 0),
        (0, 2, 5390699820817223490, 5, 0),
        (0, 3, 12683791391249642834, 5, 0),
        (0, 4, 10156452717982803138, 5, 0),
        (1, 1, 12456761093607094426, 5, 0),
        (1, 2, 4449103971517048307, 5, 0),
        (1, 3, 4476676210479347219, 5, 0),
        (1, 4, 11286949876874043516, 5, 0),
        (2, 1, 16926452996149813455, 5, 0),
        (2, 2, 2230730451439837311, 5, 0),
        (2, 3, 2623470500724881099, 5, 0),
        (2, 4, 6623830531180444546, 5, 0),
        (3, 1, 7780207060611785259, 5, 0),
        (3, 2, 13564433352698024081, 5, 0),
        (3, 3, 5593264625379709119, 5, 0),
        (3, 4, 10776335962794265028, 5, 0),
        (4, 1, 18396473872998388888, 5, 0),
        (4, 2, 6923997244725870273, 4, 0),
        (4, 3, 17878045646674304349, 5, 0),
        (4, 4, 10072092854588491910, 4, 1),
        (5, 1, 10467682203878033719, 5, 0),
        (5, 2, 15336477412646474335, 4, 0),
        (5, 3, 9824261850096163867, 5, 0),
        (5, 4, 16776980410603012573, 3, 0),
        (6, 1, 12365859758034194081, 5, 0),
        (6, 2, 908753023979430523, 4, 0),
        (6, 3, 7969925767273837093, 5, 0),
        (6, 4, 7302122824047443977, 3, 0),
        (7, 1, 15294976252602108971, 5, 0),
        (7, 2, 2235882859966796232, 2, 0),
        (7, 3, 5350815333291544202, 1, 1),
        (7, 4, 573166749483343131, 2, 0),
        (8, 1, 12237707240959185173, 5, 0),
        (8, 2, 12783921805380916832, 2, 0),
        (8, 3, 4610328142332941682, 1, 1),
        (8, 4, 14827477005743867491, 2, 0),
        (9, 1, 18352016939461868639, 4, 0),
        (9, 2, 568873959764713046, 2, 0),
        (9, 3, 1155254195008070793, 0, 1),
        (9, 4, 2392023948904027071, 0, 0),
    ];
    const CUSTOM_V0: [(usize, u64, u64, u32, u32); 40] = [
        (0, 1, 12644587548427991872, 5, 0),
        (0, 2, 5390699820817223490, 5, 0),
        (0, 3, 12683791391249642834, 5, 0),
        (0, 4, 10156452717982803138, 5, 0),
        (1, 1, 12456761093607094426, 5, 0),
        (1, 2, 4449103971517048307, 5, 0),
        (1, 3, 4476676210479347219, 5, 0),
        (1, 4, 11286949876874043516, 5, 0),
        (2, 1, 16926452996149813455, 5, 0),
        (2, 2, 2230730451439837311, 5, 0),
        (2, 3, 2623470500724881099, 5, 0),
        (2, 4, 6623830531180444546, 5, 0),
        (3, 1, 7780207060611785259, 5, 0),
        (3, 2, 13564433352698024081, 5, 0),
        (3, 3, 5593264625379709119, 5, 0),
        (3, 4, 10776335962794265028, 5, 0),
        (4, 1, 18396473872998388888, 5, 0),
        (4, 2, 6923997244725870273, 4, 0),
        (4, 3, 17878045646674304349, 5, 0),
        (4, 4, 10072092854588491910, 4, 1),
        (5, 1, 10467682203878033719, 5, 0),
        (5, 2, 15336477412646474335, 4, 0),
        (5, 3, 9824261850096163867, 5, 0),
        (5, 4, 16776980410603012573, 3, 0),
        (6, 1, 12365859758034194081, 5, 0),
        (6, 2, 908753023979430523, 4, 0),
        (6, 3, 7969925767273837093, 5, 0),
        (6, 4, 7302122824047443977, 3, 0),
        (7, 1, 15294976252602108971, 5, 0),
        (7, 2, 2235882859966796232, 2, 0),
        (7, 3, 5350815333291544202, 1, 1),
        (7, 4, 573166749483343131, 2, 0),
        (8, 1, 12237707240959185173, 5, 0),
        (8, 2, 12783921805380916832, 2, 0),
        (8, 3, 4610328142332941682, 1, 1),
        (8, 4, 14827477005743867491, 2, 0),
        (9, 1, 18352016939461868639, 4, 0),
        (9, 2, 568873959764713046, 2, 0),
        (9, 3, 1155254195008070793, 0, 1),
        (9, 4, 2392023948904027071, 0, 0),
    ];
}
