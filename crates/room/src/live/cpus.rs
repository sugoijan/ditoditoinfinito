//! CPUs in an online room that plays live (`online-cpus-v0`,
//! `docs/plans/multiplayer-online-play.md`, "CPUs"): a match's CPUs seated
//! as in practice (`crate::rules::CpuMode::Roster`), played by no
//! connection. Every device that follows the stage computes each CPU's
//! play the same way (the engine's CPU from the chart, the stage's rules,
//! its level, seed and personality, and the shocks the room committed
//! against it, each put in at a song time every device knows,
//! [`OnlineCpuRules::apply_at_us`]); it draws the CPU's field from its own
//! computation and tells the room what the ledger and the ranking need:
//! each CPU's checkpoint reports and its stream's end ([`CpuNews`], one
//! binary message for every CPU it has news of) and its run
//! (`crate::live::ClientLive::CpuRuns`). The room takes the first report
//! of each CPU and checkpoint, passes over an equal later one and counts a
//! different one as a mismatch ([`crate::live::LiveStage::cpu_mismatches`]),
//! keeping the first: the room's trust stays "self-reported".
//!
//! Who sends ([`sends_for_cpus`], `cpu-reporters-v0`): every device of a
//! person in the stage's lineup that is connected; where none is, the
//! three connected watchers with the lowest ids. Nothing is coordinated,
//! a device leaving changes nothing for the others, and the room drops
//! the duplicates; one message a checkpoint a device (incoming messages
//! are what the platform bills).
//!
//! PROVISIONAL: every number here is a working value, named and versioned.

use crate::attacks::{Judged, Targeting};
use crate::ids::{ParticipantId, StageId};
use crate::live::stream::{LiveReport, StreamEnd};
use crate::member::Connection;
use crate::state::RoomState;
use serde::{Deserialize, Serialize};

/// How an online room's CPUs are played, named and versioned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnlineCpuRules {
    pub id: &'static str,
    /// The reading horizon a CPU's lead is reckoned from (µs): the room's
    /// value, the same for every CPU and every device (practice reckons
    /// it from the local player's own, `local-player-horizon-v0`).
    pub horizon_us: i64,
    /// A shock committed at checkpoint k goes into a CPU's play this long
    /// after k's song time (rounded up to the step): every device puts it
    /// in at the same song time whenever its frame came. Within the
    /// network's lead (`LiveRules::network_lead_us`), so the shocks still
    /// come a reading horizon and the warning after it.
    pub apply_after_us: i64,
    /// A CPU's play is judged in steps of this song time (µs), its reports
    /// made after each: what keeps its reports the same whatever a
    /// device's frame rate.
    pub step_us: i64,
    /// A CPU's play starts at this song time (µs) on every device.
    pub start_us: i64,
    /// A device sends its CPUs' news at most this often (ms).
    pub send_ms: u64,
    /// Watchers who send where no person of the lineup is connected.
    pub watcher_reporters: usize,
    /// Bounds on one message (more is refused).
    pub max_entries: usize,
    pub max_reports: usize,
}

/// `online-cpus-v0` (PROVISIONAL, 2026-11-13).
pub const ONLINE_CPUS_V0: OnlineCpuRules = OnlineCpuRules {
    id: "online-cpus-v0",
    horizon_us: 2_000_000,
    apply_after_us: 1_250_000,
    step_us: 50_000,
    start_us: -2_000_000,
    send_ms: 250,
    watcher_reporters: 3,
    max_entries: 128,
    max_reports: 4_096,
};

impl OnlineCpuRules {
    /// The rules an id names, if this build knows them.
    pub fn named(id: &str) -> Option<OnlineCpuRules> {
        (id == ONLINE_CPUS_V0.id).then_some(ONLINE_CPUS_V0)
    }

    /// The song time (µs) a commitment of the checkpoint at song µs
    /// `checkpoint_us` goes into its CPU's play: `apply_after_us` later,
    /// rounded up to a step.
    pub fn apply_at_us(&self, checkpoint_us: i64) -> i64 {
        let at = checkpoint_us.saturating_add(self.apply_after_us);
        let step = self.step_us.max(1);
        at.div_euclid(step) * step + if at.rem_euclid(step) == 0 { 0 } else { step }
    }
}

/// One CPU's news from a device: its checkpoint reports made since it
/// last sent (or all of them again, after a reconnect) and its stream's
/// end once it ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuEntry {
    pub cpu: ParticipantId,
    /// The song time (µs) the device has judged the CPU's play to.
    pub through_us: i64,
    pub reports: Vec<LiveReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<StreamEnd>,
}

/// A CPU's run as a device computed it, once its stream ended
/// (`crate::live::ClientLive::CpuRuns`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuRun {
    pub cpu: ParticipantId,
    pub run: crate::battle::ReportedRun,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<crate::battle::RunDetail>,
}

/// A device's news of a stage's CPUs (binary, `crate::live::wire::CPUS_V1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuNews {
    pub stage: StageId,
    pub entries: Vec<CpuEntry>,
}

impl CpuNews {
    pub fn is_empty(&self) -> bool {
        self.entries
            .iter()
            .all(|e| e.reports.is_empty() && e.end.is_none())
    }
}

/// A report's digest (FNV-1a 64 over its fields): what the room keeps of
/// the first report of a CPU's checkpoint, to tell a later one equal or
/// not.
pub fn report_digest(r: &LiveReport) -> u64 {
    let mut h = Fnv::new();
    h.u64(u64::from(r.k));
    h.u64(r.judged.len() as u64);
    for Judged { at_us, tier } in &r.judged {
        h.u64(*at_us as u64);
        h.u64(u64::from(*tier));
    }
    h.u64(u64::from(r.life_ppm));
    h.u64(r.out_at_us.map_or(u64::MAX, |x| x as u64));
    h.u64(match r.targeting {
        None => 0,
        Some(Targeting::Spread) => 1,
        Some(Targeting::Retaliate) => 2,
        Some(Targeting::Leader) => 3,
    });
    h.0
}

/// FNV-1a 64.
pub struct Fnv(pub u64);

impl Fnv {
    pub fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    pub fn bytes(&mut self, b: &[u8]) {
        for x in b {
            self.0 ^= u64::from(*x);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    pub fn u64(&mut self, x: u64) {
        self.bytes(&x.to_le_bytes());
    }
}

impl Default for Fnv {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether member `me`'s device sends the news of stage `stage`'s CPUs
/// (`cpu-reporters-v0`): it is a person of the stage's lineup and
/// connected; or no such person is, and it is one of the
/// [`OnlineCpuRules::watcher_reporters`] connected people watching with
/// the lowest ids. Read from the room's state as every device mirrors it:
/// no coordination, and the room drops what two devices both sent.
pub fn sends_for_cpus(room: &RoomState, stage: StageId, me: ParticipantId) -> bool {
    let Some(play) = room.stage(stage).and_then(|s| s.play.as_ref()) else {
        return false;
    };
    let person_on = |id: &ParticipantId| {
        room.member(*id).is_some_and(|m| {
            !m.left && !m.is_cpu() && m.connection == Connection::Connected
        })
    };
    if play.lineup.contains(&me) {
        return person_on(&me);
    }
    if play.lineup.iter().any(person_on) {
        return false;
    }
    let mut watchers: Vec<ParticipantId> = room
        .members
        .values()
        .filter(|m| {
            !m.left
                && !m.is_cpu()
                && m.connection == Connection::Connected
                && !play.lineup.contains(&m.id)
        })
        .map(|m| m.id)
        .collect();
    watchers.sort();
    watchers
        .iter()
        .take(ONLINE_CPUS_V0.watcher_reporters)
        .any(|w| *w == me)
}

/// Whether anyone who could compute stage `stage`'s CPUs may still be
/// there: a person of the room not left and not past the reconnect grace.
/// Where nobody is, the room ends the CPUs' streams (a forfeit, as a
/// dropped connection's).
pub fn anyone_computes(room: &RoomState) -> bool {
    room.members
        .values()
        .any(|m| !m.left && !m.is_cpu() && m.connection != Connection::Disconnected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shock_goes_in_on_the_step_after_the_network_s_lead() {
        let r = ONLINE_CPUS_V0;
        assert_eq!(r.apply_at_us(0), 1_250_000);
        assert_eq!(r.apply_at_us(1_000_001), 2_300_000);
        assert_eq!(r.apply_at_us(-1_300_000), -50_000);
        assert_eq!(OnlineCpuRules::named("online-cpus-v0"), Some(r));
        assert_eq!(OnlineCpuRules::named("online-cpus-v9"), None);
    }

    #[test]
    fn a_report_s_digest_tells_every_field() {
        let base = LiveReport {
            k: 3,
            judged: vec![Judged { at_us: 10, tier: 1 }],
            life_ppm: 500_000,
            out_at_us: None,
            targeting: None,
        };
        let d = report_digest(&base);
        assert_eq!(d, report_digest(&base.clone()));
        let mut other = base.clone();
        other.judged[0].tier = 2;
        assert_ne!(d, report_digest(&other));
        let mut other = base.clone();
        other.out_at_us = Some(0);
        assert_ne!(d, report_digest(&other));
        let mut other = base;
        other.life_ppm += 1;
        assert_ne!(d, report_digest(&other));
    }
}
