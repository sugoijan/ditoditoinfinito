//! A practice stage's attacks run on this device
//! (`docs/plans/multiplayer-prototype-progress.md`, M8a and M8b): every
//! competitor's judgments handed to `ddi_room`'s [`StageBattle`] as
//! reports, a checkpoint run once every competitor still in has been
//! judged past its time. So what it comes to never depends on the frame
//! rate (each judgment carries the instant it happened, life is read from
//! each play's record at the checkpoint's instant, a checkpoint waits for
//! the slowest judge), and the same log and seeds come to the same record
//! ([`AttackRunner::record`]).
//!
//! The chart's sections with room for shocks come from the engine's
//! placement (`ddi_engine::battle::schedule`, one for everyone), each
//! competitor's lead from its reading horizon ([`LEAD_V0`]). What the
//! checkpoints commit, the stage delivers (`crate::battle::Battle`).
//!
//! Behind a development switch ([`BattlePreset::GaugeAttacksV0`]).
//!
//! [`BattlePreset::GaugeAttacksV0`]: ddi_room::BattlePreset::GaugeAttacksV0

use std::collections::BTreeMap;

use ddi_chart::{Song, TICKS_PER_BEAT, Tick};
use ddi_engine::judge::{JudgeEvent, JudgeEventKind};
use ddi_engine::rules::JudgeTable;
use ddi_room::ParticipantId;
pub use ddi_room::attacks::LEAD_V0;
use ddi_room::attacks::{
    ATTACKS, Attack, AttackEvent, BlockReason, Cell, CheckpointRecord, Judged, POWER_CLASSES_V0,
    Report, Schedule, SeatStats, Section, Slot, StageBattle, StageRoster, Targeting,
};

/// The most checkpoints a stage has (a song of an hour at 240 BPM is
/// 3 600): a bound, never reached by a real chart.
const MAX_CHECKPOINTS: usize = 8_192;

/// Events kept for the debug state.
const EVENTS_KEPT: usize = 16;

/// One stage's ledger and what it waits on.
pub struct AttackRunner {
    ledger: StageBattle,
    /// Each competitor's judgments not yet reported.
    pending: BTreeMap<ParticipantId, Vec<Judged>>,
    /// When each went out (judged song microseconds).
    out: BTreeMap<ParticipantId, i64>,
    /// The last events, each with its checkpoint's time.
    events: Vec<(i64, AttackEvent)>,
}

pub(crate) fn micros(seconds: f64) -> i64 {
    (seconds * 1e6).round() as i64
}

/// The placement a stage's sections are drawn by: the engine's. The
/// attack-rate study (`battle/study.rs`, tests only) may ask for an older
/// one to compare with (`DDI_ATTACK_PLACEMENT=v1` or `v0`).
fn placement() -> ddi_engine::battle::Placement {
    #[cfg(test)]
    match std::env::var("DDI_ATTACK_PLACEMENT").as_deref() {
        Ok("v1") => return ddi_engine::battle::PLACEMENT_V1,
        Ok("v0") => return ddi_engine::battle::PLACEMENT_V0,
        _ => {}
    }
    ddi_engine::battle::PLACEMENT
}

/// The rules a stage's attacks are played under: the room's
/// ([`ATTACKS`], `attacks-v1`). The attack-rate study (tests only) may
/// ask for the older ones to compare with (`DDI_ATTACK_RULES=v0`).
pub fn rules() -> ddi_room::attacks::AttackRules {
    #[cfg(test)]
    if std::env::var("DDI_ATTACK_RULES").as_deref() == Ok("v0") {
        return ddi_room::attacks::ATTACKS_V0;
    }
    ATTACKS
}

/// The sections chart `chart` of `song` has room for shocks in under
/// `judge` (`ddi_engine::battle::schedule`, drawn from `seed`), as the
/// ledger has them: a slot every [`ATTACKS`]'s checkpoint beats from
/// the chart's start, each section's shocks timed, when it clears (its
/// last shock and its window) and when its recipient may first recover.
pub fn schedule(song: &Song, chart: usize, judge: &JudgeTable, seed: u64) -> Schedule {
    let Some(c) = song.charts.get(chart) else {
        return Schedule::default();
    };
    let timing = c.timing(song);
    let window = ddi_engine::battle::INJECTED_MINE_V0.window(judge);
    let placement = placement();
    let every = i64::from(placement.every_beats.max(1));
    let recover = f64::from(rules().recover_beats);
    let slots = ddi_engine::battle::schedule_with(song, chart, judge, seed, &placement)
        .into_iter()
        .enumerate()
        .map(|(j, section)| Slot {
            start_us: micros(timing.seconds_at(Tick(j as i64 * every * TICKS_PER_BEAT))),
            section: section.map(|s| {
                let cells: Vec<Cell> = s
                    .cells
                    .iter()
                    .map(|h| Cell {
                        tick: h.tick.0,
                        lane: h.lane,
                        at_us: micros(timing.seconds_at(h.tick)),
                    })
                    .collect();
                let last = cells.iter().map(|c| c.at_us).max().unwrap_or(0);
                let clear = last as f64 / 1e6 + window;
                // No recovery period (`attacks-v1`): over as it clears.
                let after = if recover > 0.0 {
                    let after = timing.beat_at(clear) + recover;
                    timing.seconds_at(Tick((after * TICKS_PER_BEAT as f64).ceil() as i64))
                } else {
                    clear
                };
                Section {
                    cells,
                    clear_us: micros(clear),
                    recover_after_us: micros(after.max(clear)),
                }
            }),
        })
        .collect();
    Schedule { slots }
}

/// An attack's shocks as a play takes them.
pub fn hazards(attack: &Attack) -> Vec<ddi_engine::battle::Hazard> {
    attack
        .cells
        .iter()
        .map(|c| ddi_engine::battle::Hazard {
            tick: Tick(c.tick),
            lane: c.lane,
        })
        .collect()
}

impl AttackRunner {
    /// The ledger of chart `chart` of `song` (as played) under the match's
    /// `ruleset` (its judge `judge`), for `roster`: its checkpoints every
    /// [`ATTACKS`]'s beats from the fourth on, to four beats past the
    /// chart's last note; its sections drawn from the roster's seed.
    pub fn new(
        song: &Song,
        chart: usize,
        ruleset: &str,
        judge: &JudgeTable,
        roster: StageRoster,
    ) -> AttackRunner {
        let mut checkpoints = Vec::new();
        if let Some(c) = song.charts.get(chart) {
            let timing = c.timing(song);
            let last = c
                .notes
                .iter()
                .map(|n| n.kind.hold_end().unwrap_or(n.tick).0.max(n.tick.0))
                .max()
                .unwrap_or(0);
            let step = i64::from(rules().checkpoint_beats) * TICKS_PER_BEAT;
            let mut tick = step;
            while tick <= last + step && checkpoints.len() < MAX_CHECKPOINTS {
                checkpoints.push(micros(timing.seconds_at(Tick(tick))));
                tick += step;
            }
        }
        let participants: Vec<ParticipantId> = roster.seats.iter().map(|s| s.participant).collect();
        let schedule = schedule(song, chart, judge, roster.seed);
        AttackRunner {
            ledger: StageBattle::new(
                rules(),
                POWER_CLASSES_V0,
                ruleset,
                checkpoints,
                roster,
                schedule,
            ),
            pending: participants.iter().map(|&p| (p, Vec::new())).collect(),
            out: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    /// `participant`'s judgments, as its play made them: each note's
    /// timing (holds, mines and stray presses earn nothing).
    pub fn judged(&mut self, participant: ParticipantId, events: &[JudgeEvent]) {
        let Some(pending) = self.pending.get_mut(&participant) else {
            return;
        };
        for e in events {
            // A tap of the chart (a battle shock's events, never one,
            // carry no timing).
            if let JudgeEventKind::Tap(j, _) = e.kind {
                pending.push(Judged {
                    at_us: micros(e.song_time),
                    tier: j.index() as u8,
                });
            }
        }
    }

    /// `participant` went out at judged song second `at`.
    pub fn out_at(&mut self, participant: ParticipantId, at: f64) {
        self.out.entry(participant).or_insert(micros(at));
    }

    /// The next checkpoint's time, song seconds.
    pub fn next_checkpoint(&self) -> Option<f64> {
        self.ledger.next_checkpoint().map(|(_, at)| at as f64 / 1e6)
    }

    /// Runs every checkpoint whose time is at or before `until`: the
    /// least time any competitor still in has been judged to; `life`, a
    /// competitor's life just before a song second (millionths, from its
    /// play's record). What they changed, each with its checkpoint's time
    /// (song microseconds).
    pub fn run_due(
        &mut self,
        until: f64,
        life: &dyn Fn(ParticipantId, f64) -> u32,
    ) -> Vec<(i64, AttackEvent)> {
        let until = micros(until);
        let mut changed = Vec::new();
        while let Some((k, at)) = self.ledger.next_checkpoint()
            && at <= until
        {
            let seconds = at as f64 / 1e6;
            let reports: Vec<Report> = self
                .pending
                .iter_mut()
                .map(|(p, judged)| {
                    let split = judged.partition_point(|j| j.at_us < at);
                    Report {
                        participant: *p,
                        judged: judged.drain(..split).collect(),
                        life_ppm: life(*p, seconds),
                        out_at_us: self.out.get(p).copied(),
                    }
                })
                .collect();
            changed.extend(
                self.ledger
                    .checkpoint(k, &reports)
                    .into_iter()
                    .map(|e| (at, e)),
            );
        }
        self.events.extend(changed.iter().copied());
        let over = self.events.len().saturating_sub(EVENTS_KEPT);
        self.events.drain(..over);
        changed
    }

    /// The ledger itself (read only).
    pub fn ledger(&self) -> &StageBattle {
        &self.ledger
    }

    /// `participant`'s power as a share of the bar, `0..=1`, and whether
    /// it is full.
    pub fn bar(&self, participant: ParticipantId) -> (f32, bool) {
        (
            self.ledger.fill(participant),
            self.ledger.charged(participant),
        )
    }

    pub fn power(&self, participant: ParticipantId) -> u32 {
        self.ledger.power(participant)
    }

    /// Why its full bar waits, if it does, and its policy.
    pub fn waiting(&self, participant: ParticipantId) -> Option<(Targeting, BlockReason)> {
        self.ledger
            .waiting(participant)
            .map(|r| (self.ledger.targeting(participant), r))
    }

    pub fn stats(&self, participant: ParticipantId) -> SeatStats {
        self.ledger.stats(participant)
    }

    /// The checkpoints run so far, and how many there are.
    pub fn checkpoints(&self) -> (usize, usize) {
        (
            self.ledger.record().len(),
            self.ledger.checkpoints_us().len(),
        )
    }

    /// The ledger after each checkpoint run: equal for the same plays,
    /// whatever the frames.
    pub fn record(&self) -> &[CheckpointRecord] {
        self.ledger.record()
    }

    /// The last few things the checkpoints changed (the debug state).
    pub fn recent(&self) -> impl Iterator<Item = &AttackEvent> {
        self.events.iter().map(|(_, e)| e)
    }
}
