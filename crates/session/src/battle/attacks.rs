//! A practice stage's attack ledger run on this device
//! (`docs/plans/multiplayer-prototype-progress.md`, M8a): every
//! competitor's judgments handed to `ddi_room`'s [`StageBattle`] as
//! reports, a checkpoint run once every competitor still in has been
//! judged past its time. So what it comes to never depends on the frame
//! rate (each judgment carries the instant it happened; a checkpoint
//! waits for the slowest judge), and the same log and seeds come to the
//! same ledger ([`AttackRunner::record`]).
//!
//! M8a shows the power only ([`BattlePreset::GaugeAttacksV0`], a
//! development switch): nothing is launched. Later rounds run targeting,
//! commitments and deliveries from the same checkpoints.
//!
//! [`BattlePreset::GaugeAttacksV0`]: ddi_room::BattlePreset::GaugeAttacksV0

use std::collections::BTreeMap;

use ddi_chart::{Song, TICKS_PER_BEAT, Tick};
use ddi_engine::judge::{JudgeEvent, JudgeEventKind};
use ddi_room::ParticipantId;
use ddi_room::attacks::{
    ATTACKS_V0, AttackEvent, CheckpointRecord, Judged, POWER_CLASSES_V0, Report, StageBattle,
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
    events: Vec<AttackEvent>,
}

fn micros(seconds: f64) -> i64 {
    (seconds * 1e6).round() as i64
}

impl AttackRunner {
    /// The ledger of chart `chart` of `song` (as played) under the match's
    /// `ruleset`, for `participants`: its checkpoints every
    /// [`ATTACKS_V0`]'s beats from the fourth on, to four beats past the
    /// chart's last note.
    pub fn new(
        song: &Song,
        chart: usize,
        ruleset: &str,
        participants: impl IntoIterator<Item = ParticipantId>,
    ) -> AttackRunner {
        let participants: Vec<ParticipantId> = participants.into_iter().collect();
        let mut checkpoints = Vec::new();
        if let Some(c) = song.charts.get(chart) {
            let timing = c.timing(song);
            let last = c
                .notes
                .iter()
                .map(|n| n.kind.hold_end().unwrap_or(n.tick).0.max(n.tick.0))
                .max()
                .unwrap_or(0);
            let step = i64::from(ATTACKS_V0.checkpoint_beats) * TICKS_PER_BEAT;
            let mut tick = step;
            while tick <= last + step && checkpoints.len() < MAX_CHECKPOINTS {
                checkpoints.push(micros(timing.seconds_at(Tick(tick))));
                tick += step;
            }
        }
        AttackRunner {
            ledger: StageBattle::new(
                ATTACKS_V0,
                POWER_CLASSES_V0,
                ruleset,
                checkpoints,
                participants.iter().copied(),
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

    /// Runs every checkpoint whose time is at or before `until`: the
    /// least time any competitor still in has been judged to. What they
    /// changed.
    pub fn run_due(&mut self, until: f64) -> Vec<AttackEvent> {
        let until = micros(until);
        let mut changed = Vec::new();
        while let Some((k, at)) = self.ledger.next_checkpoint()
            && at <= until
        {
            let reports: Vec<Report> = self
                .pending
                .iter_mut()
                .map(|(p, judged)| {
                    let split = judged.partition_point(|j| j.at_us < at);
                    Report {
                        participant: *p,
                        judged: judged.drain(..split).collect(),
                        out_at_us: self.out.get(p).copied(),
                    }
                })
                .collect();
            changed.extend(self.ledger.checkpoint(k, &reports));
        }
        self.events.extend(changed.iter().copied());
        let over = self.events.len().saturating_sub(EVENTS_KEPT);
        self.events.drain(..over);
        changed
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
    pub fn recent(&self) -> &[AttackEvent] {
        &self.events
    }
}
