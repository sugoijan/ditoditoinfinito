//! A CPU of an online room's stage, played on this device as on every
//! other (`ddi_room::live::cpus`, `online-cpus-v0`): what keeps its play
//! the same everywhere whatever a device's frame rate or its network.
//!
//! - Its play is judged in fixed steps of song time
//!   ([`OnlineCpuRules::step_us`]), its checkpoint reports made after each
//!   (a `Reporter`'s reports depend on when it is asked; the steps fix it).
//! - The shocks the room committed against it go in at a song time every
//!   device knows ([`OnlineCpuRules::apply_at_us`]: the checkpoint's time
//!   and the network's lead, on a step), never when a frame came. Its play
//!   waits there until this device has heard that checkpoint's news (its
//!   record comes in the same frame as its commitments): a frame that
//!   came late holds the CPU, never changes it.
//! - It starts at one song time everywhere ([`OnlineCpuRules::start_us`])
//!   and goes out when its gauge first empties, playing on after it (its
//!   fail `ImmediateContinue`), so that a decided winner's play never
//!   depends on when the decision reached a device.
//!
//! What it reports is a function of the chart, the stage's rules, its
//! level, seed and personality, and the commitments against it: the same
//! on every device, native or in a browser (`tests::the_cpus_play_alike`,
//! with its recorded fixture).

use ddi_engine::battle::Hazard;
use ddi_engine::cpu::Opponent;
use ddi_engine::judge::JudgeEvent;
use ddi_engine::JudgeEventKind;
use ddi_room::attacks::{Judged, Reporter};
use ddi_room::live::cpus::OnlineCpuRules;
use ddi_room::live::stream::{LiveReport, StreamEnd};
use ddi_room::ParticipantId;

use super::attacks::micros;

/// CPU `seat` on chart `chart` of `song` (as played) under `ruleset` (the
/// match's, built from the stage's rules: nothing of a device's own),
/// with its ladder where zero life steps down: what practice plays and
/// what every device of an online room plays alike (not started yet).
/// Refused, with the engine's reason, where the CPUs cannot plan the
/// chart.
pub fn opponent(
    song: &ddi_chart::Song,
    chart: usize,
    ruleset: &ddi_engine::rules::Ruleset,
    seat: &ddi_ui::state::room::CpuSeat,
    ladder: &[ddi_engine::cpu::PlannedRung],
) -> Result<Opponent, String> {
    let mut rules = ruleset.fresh();
    rules.fail = ddi_engine::FailPolicy::Immediate;
    let level = usize::from(seat.level).min(ddi_engine::cpu::LEVELS - 1);
    // A roster CPU's personality (M7b); an unreadable one plays balanced.
    let skills = seat
        .skills
        .as_deref()
        .and_then(|v| ddi_engine::cpu::Skills::parse(v).ok())
        .unwrap_or_default();
    let mut opponent = Opponent::with_skills(song, chart, rules, level, seat.seed, skills)?;
    opponent.add_rungs(ladder)?;
    Ok(opponent)
}

/// A commitment against the CPU not put in yet.
#[derive(Clone, Debug)]
struct Pending {
    at_us: i64,
    attack: u32,
    cells: Vec<Hazard>,
}

/// One CPU's online play's driver, beside its `Opponent`.
#[derive(Clone, Debug)]
pub struct Drive {
    rules: OnlineCpuRules,
    participant: ParticipantId,
    /// Where the stage has attacks: its checkpoints' song times and the
    /// reporter that turns its judgments into the ledger's reports.
    checkpoints_us: Vec<i64>,
    reporter: Option<Reporter>,
    /// The song time its play is judged to (a step).
    at_us: i64,
    /// The checkpoints whose news this device has heard (all before too).
    known: usize,
    pending: Vec<Pending>,
    applied: Vec<u32>,
    /// Commitments that came after the song time they were due at (a
    /// stalled frame held past the gate): put in at once, so this
    /// device's copy may differ (counted, said in the log).
    pub late: u32,
    reports: Vec<LiveReport>,
    end: Option<StreamEnd>,
    end_taken: bool,
    plays_on: bool,
}

impl Drive {
    /// CPU `participant` played under `rules`, its stage's checkpoints
    /// `checkpoints_us` where it has attacks (none: no ledger, no
    /// reports). `opponent` is started at the rules' start.
    pub fn new(
        rules: OnlineCpuRules,
        participant: ParticipantId,
        checkpoints_us: Option<Vec<i64>>,
        opponent: &mut Opponent,
    ) -> Drive {
        opponent.set_fail(ddi_engine::FailPolicy::ImmediateContinue);
        opponent.start(rules.start_us as f64 / 1e6);
        let step = rules.step_us.max(1);
        Drive {
            rules,
            participant,
            reporter: checkpoints_us
                .as_ref()
                .map(|c| Reporter::new(participant, c.clone())),
            checkpoints_us: checkpoints_us.unwrap_or_default(),
            at_us: rules.start_us.div_euclid(step) * step,
            known: 0,
            pending: Vec::new(),
            applied: Vec::new(),
            late: 0,
            reports: Vec::new(),
            end: None,
            end_taken: false,
            plays_on: false,
        }
    }

    /// Checkpoint `k`'s song time, where the stage has attacks.
    pub fn checkpoint_us(&self, k: usize) -> Option<i64> {
        self.checkpoints_us.get(k).copied()
    }

    /// The stage was decided with it a winner: its gauge emptying after
    /// it is no end of its stream (it plays the song out).
    pub fn plays_on(&mut self) {
        self.plays_on = true;
    }

    pub fn participant(&self) -> ParticipantId {
        self.participant
    }

    /// The song time (µs) its play is judged to.
    pub fn at_us(&self) -> i64 {
        self.at_us
    }

    /// The news of the stage's checkpoints before `n` has come (their
    /// records, and with them every commitment they made).
    pub fn known(&mut self, n: usize) {
        self.known = self.known.max(n);
    }

    /// The checkpoints whose news has come.
    pub fn known_count(&self) -> usize {
        self.known
    }

    /// An attack committed against it at checkpoint `k`: its shocks put in
    /// at [`OnlineCpuRules::apply_at_us`] of `k`'s time. Whether it came in
    /// time (else it goes in at once, counted late).
    pub fn commit(&mut self, k: usize, attack: u32, cells: Vec<Hazard>) -> bool {
        if self.applied.contains(&attack) || self.pending.iter().any(|p| p.attack == attack) {
            return true;
        }
        let Some(cp) = self.checkpoints_us.get(k).copied() else {
            return false;
        };
        let at_us = self.rules.apply_at_us(cp);
        let in_time = at_us > self.at_us;
        if !in_time {
            self.late += 1;
        }
        let i = self
            .pending
            .partition_point(|p| (p.at_us, p.attack) <= (at_us, attack));
        self.pending.insert(i, Pending { at_us, attack, cells });
        in_time
    }

    /// How far its play may be judged now: up to `limit_us`, and short of
    /// the song time the next checkpoint not heard of yet would put
    /// shocks in at (where the stage has attacks).
    pub fn gate_us(&self, limit_us: i64) -> i64 {
        match self.checkpoints_us.get(self.known) {
            Some(&cp) if self.reporter.is_some() => limit_us.min(self.rules.apply_at_us(cp) - 1),
            _ => limit_us,
        }
    }

    /// Its play judged on, a step at a time, as far as `limit_us` and its
    /// gate allow: each step's shocks put in, its judgments to its
    /// reporter, its reports made. Its judgments, for its field.
    pub fn advance(&mut self, opponent: &mut Opponent, limit_us: i64) -> Vec<JudgeEvent> {
        // Nothing comes after its chart's end and the stage's last
        // checkpoint (a second on, to be sure of both).
        let last = micros(opponent.player().end_time())
            .max(self.checkpoints_us.last().copied().unwrap_or(i64::MIN))
            .saturating_add(1_000_000);
        let target = self.gate_us(limit_us).min(last);
        let step = self.rules.step_us.max(1);
        let mut events = Vec::new();
        // Late ones first (they missed their step).
        self.apply_due(opponent, self.at_us);
        while self.at_us + step <= target {
            let g = self.at_us + step;
            opponent.advance(g as f64 / 1e6);
            self.at_us = g;
            self.apply_due(opponent, g);
            let judged = opponent.take_judged();
            self.report(opponent, &judged, g);
            events.extend(judged);
        }
        events
    }

    fn apply_due(&mut self, opponent: &mut Opponent, g: i64) {
        while self.pending.first().is_some_and(|p| p.at_us <= g) {
            let p = self.pending.remove(0);
            let _ = opponent.receive_hazards(&p.cells);
            self.applied.push(p.attack);
        }
    }

    fn report(&mut self, opponent: &Opponent, judged: &[JudgeEvent], g: i64) {
        let out = opponent.out_at();
        if let Some(r) = self.reporter.as_mut() {
            for e in judged {
                if let JudgeEventKind::Tap(j, _) = e.kind {
                    r.judged(Judged {
                        at_us: micros(e.song_time),
                        tier: j.index() as u8,
                    });
                }
            }
            if let Some(at) = out.filter(|_| !self.plays_on) {
                r.out_at(micros(at));
            }
            let player = opponent.player();
            let life_at = |us: i64| {
                (f64::from(player.life_at(us as f64 / 1e6).clamp(0.0, 1.0)) * 1e6).round() as u32
            };
            for (k, rep) in r.due(g, &life_at) {
                self.reports.push(LiveReport::from_report(k, rep));
            }
        }
        if self.end.is_none() {
            if let Some(at) = out.filter(|_| !self.plays_on) {
                self.end = Some(StreamEnd::Out { at_us: micros(at) });
            } else if opponent.finished() {
                self.end = Some(StreamEnd::Finished);
            }
        }
    }

    /// The reports made since the last call.
    pub fn take_reports(&mut self) -> Vec<LiveReport> {
        std::mem::take(&mut self.reports)
    }

    /// Its stream's end, once, when it ended (out, or its chart played).
    pub fn take_end(&mut self) -> Option<StreamEnd> {
        if self.end_taken {
            return None;
        }
        let end = self.end?;
        self.end_taken = true;
        Some(end)
    }

    /// Its stream's end, if it ended.
    pub fn end(&self) -> Option<StreamEnd> {
        self.end
    }

    /// The commitments put into its play, in order.
    pub fn applied(&self) -> &[u32] {
        &self.applied
    }
}

#[cfg(test)]
mod tests;
