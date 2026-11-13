//! The room's side of live play, sans-IO: what the server's room (and the
//! fake) runs beside the reducer. It reads the room's state, never writes
//! it: what it decides for the room goes back as the authority's commands
//! ([`LiveOut::commands`]: a stage's battle plan, its confirmed result,
//! its song's end), which the host applies through the reducer as its
//! own.
//!
//! [`LiveServer`] keeps what outlives a stage's play (each upcoming
//! stage's battle plan and who said they were ready to play it live:
//! [`LiveDurable`], for the host's storage) and the stage being played,
//! [`LiveStage`]: each competitor's stream, the attack ledger run at
//! deadlines, the settlement and the frames, built once a tick and kept
//! for a reconnect's resume.

use crate::attacks::{
    ATTACKS_V0, ATTACKS_V1, AttackRules, LEAD_V0, POWER_CLASSES_V0, PowerClasses, Report,
    RosterSeat, SETTLEMENT_V0, Settling, StageBattle, StageRoster,
};
use crate::battle::{Entrant, Run};
use crate::command::Command;
use crate::ids::{Millis, ParticipantId, StageId};
use crate::live::plan::{BattlePlan, PlanDigest};
use crate::live::stream::{
    Batch, CheckpointNews, Commitment, Frame, FrameEntry, LiveReport, Moment, MomentKind,
    StreamEnd,
};
use crate::live::wire;
use crate::live::{ClientLive, LiveRefusal, LiveRules, Pong, ServerLive, room_instant};
use crate::member::{Capability, Connection, can};
use crate::phase::Phase;
use crate::state::{RoomState, StagePlay};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// What goes out on the sender's socket alone.
#[derive(Clone, Debug, PartialEq)]
pub enum LiveReply {
    /// JSON text (`ServerLiveEnvelope`).
    Message(ServerLive),
    /// A frame's bytes, as kept (a resume's).
    Frame(Vec<u8>),
}

/// What the room's live side did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveOut {
    /// For the sender's socket alone, in order.
    pub reply: Vec<LiveReply>,
    /// Frames for every socket that said [`ClientLive::Hello`], in order:
    /// the same bytes for all.
    pub broadcast: Vec<Vec<u8>>,
    /// The authority's commands for the reducer, in order.
    pub commands: Vec<Command>,
    /// What the host should log (English): a CPU's reports that two
    /// devices computed differently (`crate::live::cpus`).
    pub notes: Vec<String>,
}

impl LiveOut {
    fn refuse(&mut self, refusal: LiveRefusal) {
        self.reply
            .push(LiveReply::Message(ServerLive::Refused { refusal }));
    }
}

/// A member's word that they will play a stage live.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prepared {
    pub horizon_us: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanDigest>,
}

/// What the host keeps in its storage between a pick and its stage (the
/// room may sleep in between): the plans and who prepared.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveDurable {
    #[serde(with = "crate::ids::pairs")]
    pub plans: BTreeMap<StageId, BattlePlan>,
    #[serde(with = "crate::ids::pairs")]
    pub prepared: BTreeMap<StageId, Vec<(ParticipantId, Prepared)>>,
}

/// One competitor of a stage, as its status stands (a snapshot's).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompetitorStatus {
    pub participant: ParticipantId,
    /// Plays live (said it was prepared, its plan agreed); otherwise a
    /// device that predates live play, ranked from its report alone.
    pub live: bool,
    pub seq: u32,
    pub through_us: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<StreamEnd>,
    pub life_ppm: u32,
    pub combo: u32,
    pub score: i64,
    pub power: u32,
}

/// A stage as it stands, for a socket that could not resume from the
/// frames kept: the frames from `first_frame` on follow it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveSnapshot {
    pub stage: StageId,
    pub song_zero_at: Millis,
    pub at: Millis,
    pub first_frame: u32,
    pub competitors: Vec<CompetitorStatus>,
    /// Every attack committed so far.
    pub commitments: Vec<Commitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settling: Option<Settling>,
}

fn attack_rules(id: &str) -> Option<AttackRules> {
    [ATTACKS_V0, ATTACKS_V1].into_iter().find(|r| r.id == id)
}

fn power_classes(id: &str) -> Option<PowerClasses> {
    (id == POWER_CLASSES_V0.id).then_some(POWER_CLASSES_V0)
}

/// The room's live side.
#[derive(Clone, Debug)]
pub struct LiveServer {
    rules: LiveRules,
    plans: BTreeMap<StageId, BattlePlan>,
    prepared: BTreeMap<StageId, BTreeMap<ParticipantId, Prepared>>,
    stage: Option<LiveStage>,
    /// The stage played last, as it ended (for the record and tests).
    last: Option<LiveStage>,
}

/// The stage of `room` being played live, if any: one committed with a
/// song time zero, from its countdown to its song's end.
pub fn live_stage(room: &RoomState) -> Option<(StageId, &StagePlay)> {
    let id = match room.phase {
        Phase::Countdown { stage, .. }
        | Phase::Stage { stage }
        | Phase::SongCompletion { stage } => stage,
        _ => return None,
    };
    let play = room.stage(id)?.play.as_ref()?;
    play.song_zero_at.is_some().then_some((id, play))
}

impl LiveServer {
    pub fn new(rules: LiveRules) -> Self {
        Self {
            rules,
            plans: BTreeMap::new(),
            prepared: BTreeMap::new(),
            stage: None,
            last: None,
        }
    }

    /// From what the host kept.
    pub fn restore(rules: LiveRules, durable: LiveDurable) -> Self {
        Self {
            rules,
            plans: durable.plans,
            prepared: durable
                .prepared
                .into_iter()
                .map(|(s, v)| (s, v.into_iter().collect()))
                .collect(),
            stage: None,
            last: None,
        }
    }

    /// What the host keeps (after every live message that changed it).
    pub fn durable(&self) -> LiveDurable {
        LiveDurable {
            plans: self.plans.clone(),
            prepared: self
                .prepared
                .iter()
                .map(|(s, m)| (*s, m.iter().map(|(p, x)| (*p, x.clone())).collect()))
                .collect(),
        }
    }

    pub fn rules(&self) -> LiveRules {
        self.rules
    }

    /// The stage being played, if any.
    pub fn stage(&self) -> Option<&LiveStage> {
        self.stage.as_ref()
    }

    /// The stage played before, as it ended.
    pub fn last_stage(&self) -> Option<&LiveStage> {
        self.last.as_ref()
    }

    /// The stage being played, else the one before.
    pub fn stage_or_last(&self) -> Option<&LiveStage> {
        self.stage.as_ref().or(self.last.as_ref())
    }

    pub fn plan(&self, stage: StageId) -> Option<&BattlePlan> {
        self.plans.get(&stage)
    }

    /// When the host should call [`LiveServer::tick`] next (its alarm).
    pub fn next_wake(&self) -> Option<Millis> {
        self.stage.as_ref().map(|s| s.next_tick)
    }

    /// A live text message from member `from` at room time `now`.
    pub fn receive(
        &mut self,
        room: &RoomState,
        from: ParticipantId,
        msg: ClientLive,
        now: Millis,
    ) -> LiveOut {
        let mut out = LiveOut::default();
        match msg {
            // The audience's (`crate::audience::AudienceHub::set_open`).
            ClientLive::Hello { .. } | ClientLive::Audience { .. } => {}
            ClientLive::Ping(p) => out
                .reply
                .push(LiveReply::Message(ServerLive::Pong(Pong::answer(p, now.0)))),
            ClientLive::Prepared {
                stage,
                horizon_us,
                plan,
            } => {
                let Some(s) = room.stage(stage) else {
                    out.refuse(LiveRefusal::UnknownStage { stage });
                    return out;
                };
                if s.play.is_some() {
                    // Too late to change a lead: the stage started.
                    return out;
                }
                if let Some(room_plan) = s.content.as_ref().and_then(|c| c.battle.clone())
                    && plan.as_ref() != Some(&room_plan)
                {
                    out.refuse(LiveRefusal::PlanMismatch {
                        stage,
                        room: room_plan,
                        yours: plan,
                    });
                    self.prepared.entry(stage).or_default().remove(&from);
                    return out;
                }
                self.prepared.entry(stage).or_default().insert(
                    from,
                    Prepared {
                        horizon_us: horizon_us.clamp(0, LEAD_V0.horizon_cap_us()),
                        plan,
                    },
                );
            }
            ClientLive::OfferPlan { stage, plan } => {
                self.offer(room, from, stage, *plan, &mut out);
            }
            ClientLive::Resume { stage, after } => match &self.stage {
                Some(s) if s.id == stage => out.reply.extend(s.resume(after, now)),
                _ => out.refuse(LiveRefusal::UnknownStage { stage }),
            },
            ClientLive::CpuRun {
                stage,
                cpu,
                run,
                detail,
            } => match self.stage.as_mut() {
                Some(s) if s.id == stage => s.cpu_run(room, from, cpu, run, detail, &mut out),
                _ => {
                    // The stage's streams are gone (the room moved on long
                    // since): its run is passed over.
                }
            },
        }
        out
    }

    fn offer(
        &mut self,
        room: &RoomState,
        from: ParticipantId,
        stage: StageId,
        plan: BattlePlan,
        out: &mut LiveOut,
    ) {
        let Some(s) = room.stage(stage).filter(|s| s.play.is_none()) else {
            out.refuse(LiveRefusal::UnknownStage { stage });
            return;
        };
        let Some(content) = &s.content else {
            out.refuse(LiveRefusal::UnknownStage { stage });
            return;
        };
        let may = room.chooses(from)
            || room
                .member(from)
                .filter(|m| !m.left && !m.is_cpu())
                .is_some_and(|m| {
                    can(
                        m.role,
                        m.admin,
                        Capability::ChooseContent,
                        room.phase.kind(),
                    )
                });
        if !may {
            out.refuse(LiveRefusal::NotPicker { stage });
            return;
        }
        if let Err(problem) = plan.check() {
            out.refuse(LiveRefusal::BadPlan { stage, problem });
            return;
        }
        if attack_rules(&plan.attack_rules).is_none() || power_classes(&plan.classes).is_none() {
            out.refuse(LiveRefusal::BadPlan {
                stage,
                problem: crate::live::PlanProblem::UnknownVersion,
            });
            return;
        }
        if let Some(have) = &content.battle {
            if *have != plan.digest {
                out.refuse(LiveRefusal::PlanSet {
                    stage,
                    digest: have.clone(),
                });
            }
            return;
        }
        let digest = plan.digest.clone();
        self.plans.insert(stage, plan);
        // Words already said against another plan no longer count.
        if let Some(p) = self.prepared.get_mut(&stage) {
            p.retain(|_, x| x.plan.as_ref() == Some(&digest));
        }
        out.commands.push(Command::SetBattlePlan { stage, digest });
    }

    /// A binary message from member `from`.
    pub fn receive_binary(
        &mut self,
        room: &RoomState,
        from: ParticipantId,
        bytes: &[u8],
        now: Millis,
    ) -> LiveOut {
        let mut out = LiveOut::default();
        let stage = self.stage.as_ref().map_or(StageId(0), |s| s.id);
        match wire::decode(bytes) {
            Ok(wire::Binary::Batch(b)) => match self.stage.as_mut() {
                Some(s) if s.id == b.stage => {
                    let ends = b.end.is_some();
                    match s.accept(from, b, now) {
                        // A stream's end may decide the stage: at once, so
                        // the decision comes before the run the same
                        // device reports next.
                        Ok(()) if ends => s.decide(room, &mut out),
                        Ok(()) => {}
                        Err(r) => out.refuse(r),
                    }
                }
                _ => out.refuse(LiveRefusal::UnknownStage { stage: b.stage }),
            },
            Ok(wire::Binary::Cpus(n)) => match self.stage.as_mut() {
                Some(s) if s.id == n.stage => match s.accept_cpus(room, from, n, &mut out) {
                    Ok(true) => s.decide(room, &mut out),
                    Ok(false) => {}
                    Err(r) => out.refuse(r),
                },
                _ => out.refuse(LiveRefusal::UnknownStage { stage: n.stage }),
            },
            Ok(wire::Binary::Frame(_)) | Err(_) => out.refuse(LiveRefusal::BadBatch { stage }),
        }
        out
    }

    /// Time passes (the host's alarm, and after every change of the room):
    /// the stage being played follows the room's, and runs what is due.
    pub fn tick(&mut self, room: &RoomState, now: Millis) -> LiveOut {
        self.sync(room, now);
        let mut out = LiveOut::default();
        if let Some(s) = self.stage.as_mut() {
            s.tick(room, now, &mut out);
        }
        out
    }

    /// How long a stage's streams are still taken and sent after the room
    /// moved on (its last reports settled it before the last batches came).
    pub const LINGER_MS: u64 = 10_000;

    fn sync(&mut self, room: &RoomState, now: Millis) {
        let current = live_stage(room);
        match (current, &mut self.stage) {
            (Some((id, _)), Some(s)) if s.id == id => {}
            (Some((id, play)), old) => {
                if let Some(old) = old.take() {
                    self.last = Some(old);
                }
                self.stage = Some(LiveStage::new(
                    room,
                    id,
                    play,
                    self.rules,
                    self.plans.get(&id),
                    self.prepared.get(&id),
                ));
            }
            (None, Some(s)) => {
                // The room moved on: the last batches still go out, for a
                // while; a canceled start's stage goes at once.
                let canceled = room.stage(s.id).is_none_or(|st| st.play.is_none());
                let left = *s.left_at.get_or_insert(now);
                let done = s.comps.values().all(|c| c.end.is_some()) && s.pending.is_empty();
                if canceled || done || now.0 >= left.0 + Self::LINGER_MS {
                    self.last = self.stage.take();
                }
            }
            (None, None) => {}
        }
        let playing = self.stage.as_ref().map(|s| s.id);
        let keep = |id: &StageId| {
            Some(*id) == playing || room.stage(*id).is_some_and(|s| s.play.is_none())
        };
        self.plans.retain(|id, _| keep(id));
        self.prepared.retain(|id, _| keep(id));
    }
}

#[derive(Clone, Debug)]
struct Comp {
    /// A CPU of the match (`crate::live::cpus`): its reports come from
    /// the devices that compute it.
    cpu: bool,
    live: bool,
    seq: u32,
    through_us: i64,
    streamed: bool,
    end: Option<StreamEnd>,
    last: Option<Moment>,
    /// Reports not run yet, by checkpoint.
    reports: BTreeMap<u32, LiveReport>,
    /// The highest checkpoint it reported.
    reported: Option<u32>,
    /// Life as last reported (millionths).
    life_ppm: u32,
    /// The steps down its stream said (a match's "At zero life: Step
    /// down"): the highest rung.
    steps: u8,
}

impl Comp {
    fn out_at_us(&self) -> Option<i64> {
        match self.end {
            Some(StreamEnd::Out { at_us }) => Some(at_us),
            Some(e) if e.forfeit() => Some(self.through_us.max(0)),
            _ => None,
        }
    }

    /// Its report at checkpoint `k`: every report it sent up to `k` (late
    /// ones from before, the ledger counting their judgments now), or
    /// none sent (late, or ended): nothing judged, its life as last known.
    fn report_for(&mut self, k: u32, participant: ParticipantId) -> Report {
        let keys: Vec<u32> = self.reports.range(..=k).map(|(k, _)| *k).collect();
        let mut merged: Option<LiveReport> = None;
        for key in keys {
            let Some(r) = self.reports.remove(&key) else {
                continue;
            };
            merged = Some(match merged {
                None => r,
                Some(mut m) => {
                    m.judged.extend(r.judged);
                    m.life_ppm = r.life_ppm;
                    m.out_at_us = match (m.out_at_us, r.out_at_us) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (a, b) => a.or(b),
                    };
                    m.targeting = r.targeting.or(m.targeting);
                    m.k = r.k;
                    m
                }
            });
        }
        let ended = self.out_at_us();
        let mut report = match merged {
            Some(r) => r.into_report(participant),
            None => Report {
                participant,
                judged: Vec::new(),
                life_ppm: self.life_ppm,
                out_at_us: None,
                targeting: None,
            },
        };
        report.out_at_us = match (report.out_at_us, ended) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.life_ppm = report.life_ppm;
        report
    }
}

/// One stage played live.
#[derive(Clone, Debug)]
pub struct LiveStage {
    id: StageId,
    rules: LiveRules,
    song_zero_at: Millis,
    comps: BTreeMap<ParticipantId, Comp>,
    ledger: Option<StageBattle>,
    plan: Option<PlanDigest>,
    /// Every competitor plays live: the room may decide the stage early.
    early: bool,
    decided: bool,
    song_end_sent: bool,
    settling: Option<Settling>,
    settling_sent: Option<Settling>,
    commitments: Vec<Commitment>,
    // The next frame's makings.
    pending: BTreeMap<ParticipantId, FrameEntry>,
    pending_commitments: Vec<Commitment>,
    pending_checkpoints: Vec<CheckpointNews>,
    next_n: u32,
    kept: VecDeque<(u32, Vec<u8>)>,
    next_tick: Millis,
    /// When the room moved on from it.
    left_at: Option<Millis>,
    /// The CPUs' reports taken, by CPU and checkpoint: the first's digest
    /// (`crate::live::cpus::report_digest`), what a later one is told
    /// equal or not by; their ends and runs as first taken.
    cpu_reports: BTreeMap<(ParticipantId, u32), u64>,
    cpu_ends: BTreeMap<ParticipantId, StreamEnd>,
    cpu_runs: BTreeMap<ParticipantId, crate::battle::ReportedRun>,
    /// Reports, ends and runs of a CPU a device sent that differ from
    /// the first taken (counted; the first kept).
    cpu_mismatches: u32,
    /// CPU messages taken, and the reports in them (duplicates included).
    cpu_messages: u32,
    cpu_reports_seen: u32,
}

impl LiveStage {
    fn new(
        room: &RoomState,
        id: StageId,
        play: &StagePlay,
        rules: LiveRules,
        plan: Option<&BattlePlan>,
        prepared: Option<&BTreeMap<ParticipantId, Prepared>>,
    ) -> LiveStage {
        let m = room.current.as_ref();
        let attacks = m.is_some_and(|m| m.rules_for(id).attacks());
        let room_plan = room
            .stage(id)
            .and_then(|s| s.content.as_ref())
            .and_then(|c| c.battle.clone());
        let plan = plan.filter(|p| attacks && room_plan.as_ref() == Some(&p.digest));
        let prepared_ok = |p: &ParticipantId| {
            prepared.and_then(|m| m.get(p)).filter(|x| match plan {
                Some(plan) => x.plan.as_ref() == Some(&plan.digest),
                None => true,
            })
        };
        // A CPU plays live where the room seats CPUs online: the devices
        // that compute it report for it (`crate::live::cpus`).
        let is_cpu = |p: &ParticipantId| room.member(*p).is_some_and(|m| m.is_cpu());
        let cpus_live = room.policy.cpus;
        let comps: BTreeMap<ParticipantId, Comp> = play
            .lineup
            .iter()
            .map(|p| {
                (
                    *p,
                    Comp {
                        cpu: is_cpu(p),
                        live: if is_cpu(p) {
                            cpus_live
                        } else {
                            prepared_ok(p).is_some()
                        },
                        seq: 0,
                        through_us: i64::MIN,
                        streamed: false,
                        end: None,
                        last: None,
                        reports: BTreeMap::new(),
                        reported: None,
                        life_ppm: 1_000_000,
                        steps: 0,
                    },
                )
            })
            .collect();
        let ledger = plan.and_then(|plan| {
            let seats = play
                .lineup
                .iter()
                .filter_map(|p| {
                    // A CPU's lead from the room's horizon for CPUs, the
                    // same on every device.
                    let horizon_us = if is_cpu(p) {
                        cpus_live.then_some(crate::live::cpus::ONLINE_CPUS_V0.horizon_us)?
                    } else {
                        prepared_ok(p)?.horizon_us
                    };
                    Some(RosterSeat {
                        participant: *p,
                        targeting: room.member(*p).map(|m| m.targeting()).unwrap_or_default(),
                        lead_us: rules.lead_us(horizon_us),
                    })
                })
                .collect();
            Some(StageBattle::new(
                attack_rules(&plan.attack_rules)?,
                power_classes(&plan.classes)?,
                m.map_or(crate::rules::MatchRules::DEFAULT_RULESET, |m| {
                    m.rules.ruleset_id()
                }),
                plan.checkpoints_us.clone(),
                StageRoster {
                    seats,
                    seed: plan.seed,
                },
                plan.schedule.clone(),
            ))
        });
        LiveStage {
            id,
            rules,
            song_zero_at: play.song_zero_at.unwrap_or(play.start_at),
            early: comps.values().all(|c| c.live),
            comps,
            ledger,
            plan: plan.map(|p| p.digest.clone()),
            decided: false,
            song_end_sent: false,
            settling: None,
            settling_sent: None,
            commitments: Vec::new(),
            pending: BTreeMap::new(),
            pending_commitments: Vec::new(),
            pending_checkpoints: Vec::new(),
            next_n: 1,
            kept: VecDeque::new(),
            next_tick: Millis(0),
            left_at: None,
            cpu_reports: BTreeMap::new(),
            cpu_ends: BTreeMap::new(),
            cpu_runs: BTreeMap::new(),
            cpu_mismatches: 0,
            cpu_messages: 0,
            cpu_reports_seen: 0,
        }
    }

    /// Reports, ends and runs of a CPU that a device sent differently
    /// from the first taken (`crate::live::cpus`: the first is kept).
    pub fn cpu_mismatches(&self) -> u32 {
        self.cpu_mismatches
    }

    /// CPU messages taken so far, and the reports they carried (equal
    /// ones from several devices each counted).
    pub fn cpu_traffic(&self) -> (u32, u32) {
        (self.cpu_messages, self.cpu_reports_seen)
    }

    /// A device's news of the stage's CPUs (`crate::live::cpus`): each
    /// CPU's reports, the first of each checkpoint taken (counted at its
    /// checkpoint, or at the next one where it came after its deadline,
    /// as a person's late report), a later one compared; its end, the
    /// first taken. From any person of the room (a player or a watcher),
    /// never for anyone but a CPU of the stage. Whether a CPU's stream
    /// ended with it (the stage may be decided).
    pub fn accept_cpus(
        &mut self,
        room: &RoomState,
        from: ParticipantId,
        n: crate::live::cpus::CpuNews,
        out: &mut LiveOut,
    ) -> Result<bool, LiveRefusal> {
        let stage = self.id;
        let rules = crate::live::cpus::ONLINE_CPUS_V0;
        if !room
            .member(from)
            .is_some_and(|m| !m.left && !m.is_cpu())
        {
            return Err(LiveRefusal::NotInLineup { stage });
        }
        if n.entries.len() > rules.max_entries
            || n.entries.iter().map(|e| e.reports.len()).sum::<usize>() > rules.max_reports
            || n.entries.iter().any(|e| {
                e.reports
                    .iter()
                    .any(|r| r.judged.len() > self.rules.max_report_judged)
            })
        {
            return Err(LiveRefusal::BadBatch { stage });
        }
        self.cpu_messages += 1;
        let checkpoints = self.ledger.as_ref().map_or(0, |l| l.checkpoints_us().len());
        let mut ended = false;
        for e in n.entries {
            let Some(c) = self.comps.get_mut(&e.cpu).filter(|c| c.cpu && c.live) else {
                continue;
            };
            c.streamed = true;
            c.through_us = c.through_us.max(e.through_us);
            for r in e.reports {
                self.cpu_reports_seen += 1;
                let digest = crate::live::cpus::report_digest(&r);
                match self.cpu_reports.get(&(e.cpu, r.k)) {
                    Some(d) if *d == digest => {}
                    Some(_) => {
                        self.cpu_mismatches += 1;
                        out.notes.push(format!(
                            "live: stage {}: CPU {} checkpoint {}: participant {}'s report differs from the first taken (kept)",
                            stage.0, e.cpu.0, r.k, from.0
                        ));
                    }
                    None => {
                        self.cpu_reports.insert((e.cpu, r.k), digest);
                        if (r.k as usize) < checkpoints {
                            c.reported = Some(c.reported.map_or(r.k, |k| k.max(r.k)));
                            c.reports.insert(r.k, r);
                        }
                    }
                }
            }
            if let Some(end) = e.end {
                match self.cpu_ends.get(&e.cpu) {
                    Some(first) if *first == end => {}
                    Some(_) => {
                        self.cpu_mismatches += 1;
                        out.notes.push(format!(
                            "live: stage {}: CPU {}: participant {}'s end {end:?} differs from the first taken (kept)",
                            stage.0, e.cpu.0, from.0
                        ));
                    }
                    None if c.end.is_none() => {
                        self.cpu_ends.insert(e.cpu, end);
                        c.end = Some(end);
                        ended = true;
                        let entry = self.pending.entry(e.cpu).or_insert_with(|| FrameEntry {
                            participant: e.cpu,
                            seq: 0,
                            through_us: 0,
                            moments: Vec::new(),
                            end: None,
                        });
                        entry.through_us = c.through_us.max(0);
                        entry.end = Some(end);
                    }
                    // Ended by the room already (nobody left to compute
                    // it, then someone came back): the room's end stands.
                    None => {}
                }
            }
        }
        Ok(ended)
    }

    /// A CPU's run as device `from` computed it: the first taken (the
    /// authority's `Command::ReportFor`), a later one compared.
    fn cpu_run(
        &mut self,
        room: &RoomState,
        from: ParticipantId,
        cpu: ParticipantId,
        run: crate::battle::ReportedRun,
        detail: Option<crate::battle::RunDetail>,
        out: &mut LiveOut,
    ) {
        if !room.member(from).is_some_and(|m| !m.left && !m.is_cpu())
            || !self.comps.get(&cpu).is_some_and(|c| c.cpu && c.live)
        {
            out.refuse(LiveRefusal::NotInLineup { stage: self.id });
            return;
        }
        match self.cpu_runs.get(&cpu) {
            Some(first) if *first == run => {}
            Some(_) => {
                self.cpu_mismatches += 1;
                out.notes.push(format!(
                    "live: stage {}: CPU {}: participant {}'s run differs from the first taken (kept)",
                    self.id.0, cpu.0, from.0
                ));
            }
            None => {
                self.cpu_runs.insert(cpu, run);
                out.commands.push(Command::ReportFor {
                    stage: self.id,
                    participant: cpu,
                    run,
                    detail,
                });
            }
        }
    }

    pub fn id(&self) -> StageId {
        self.id
    }

    pub fn song_zero_at(&self) -> Millis {
        self.song_zero_at
    }

    /// The attack ledger, where the stage has attacks.
    pub fn ledger(&self) -> Option<&StageBattle> {
        self.ledger.as_ref()
    }

    /// Whether the room may decide it before the song's end (every
    /// competitor plays live).
    pub fn decides_early(&self) -> bool {
        self.early
    }

    /// Frames sent so far.
    pub fn frames(&self) -> u32 {
        self.next_n - 1
    }

    pub fn end(&self, p: ParticipantId) -> Option<StreamEnd> {
        self.comps.get(&p).and_then(|c| c.end)
    }

    /// The steps down `p`'s stream said (the rung it plays).
    pub fn steps(&self, p: ParticipantId) -> u8 {
        self.comps.get(&p).map_or(0, |c| c.steps)
    }

    /// Whether `p` plays live (prepared, its plan agreed).
    pub fn plays_live(&self, p: ParticipantId) -> bool {
        self.comps.get(&p).is_some_and(|c| c.live)
    }

    /// A batch from `from`: taken in order (an old one passed over, a
    /// later one left for the sender to send again after the one missing).
    pub fn accept(
        &mut self,
        from: ParticipantId,
        b: Batch,
        _now: Millis,
    ) -> Result<(), LiveRefusal> {
        let stage = self.id;
        let rules = self.rules;
        let checkpoints = self.ledger.as_ref().map_or(0, |l| l.checkpoints_us().len());
        let Some(c) = self.comps.get_mut(&from) else {
            return Err(LiveRefusal::NotInLineup { stage });
        };
        if b.seq <= c.seq {
            return Ok(());
        }
        if c.end.is_some() {
            return Err(LiveRefusal::StreamEnded { stage });
        }
        if b.seq != c.seq + 1 {
            return Ok(());
        }
        if b.moments.len() > rules.max_batch_moments
            || b.reports.len() > rules.max_batch_reports
            || b.reports
                .iter()
                .any(|r| r.judged.len() > rules.max_report_judged)
        {
            return Err(LiveRefusal::BadBatch { stage });
        }
        c.seq = b.seq;
        c.streamed = true;
        c.through_us = c.through_us.max(b.through_us);
        if let Some(m) = b.moments.last() {
            c.last = Some(*m);
        }
        for m in &b.moments {
            if let MomentKind::StepDown { rung } = m.kind {
                c.steps = c.steps.max(rung);
            }
        }
        for r in b.reports {
            if (r.k as usize) < checkpoints {
                c.reported = Some(c.reported.map_or(r.k, |k| k.max(r.k)));
                c.reports.insert(r.k, r);
            }
        }
        if let Some(end) = b.end {
            c.end = Some(end);
        }
        let e = self.pending.entry(from).or_insert_with(|| FrameEntry {
            participant: from,
            seq: 0,
            through_us: 0,
            moments: Vec::new(),
            end: None,
        });
        e.seq = c.seq;
        e.through_us = c.through_us;
        e.moments.extend(b.moments);
        if b.end.is_some() {
            e.end = b.end;
        }
        Ok(())
    }

    fn end_by_room(&mut self, p: ParticipantId, end: StreamEnd) {
        let Some(c) = self.comps.get_mut(&p) else {
            return;
        };
        if c.end.is_some() {
            return;
        }
        c.end = Some(end);
        let e = self.pending.entry(p).or_insert_with(|| FrameEntry {
            participant: p,
            seq: 0,
            through_us: 0,
            moments: Vec::new(),
            end: None,
        });
        e.seq = c.seq;
        e.through_us = c.through_us.max(0);
        e.end = Some(end);
    }

    fn tick(&mut self, room: &RoomState, now: Millis, out: &mut LiveOut) {
        if now < self.next_tick {
            return;
        }
        let step = self.rules.tick_ms.max(1);
        self.next_tick = if self.next_tick.0 == 0 {
            Millis(now.0 + step)
        } else {
            let mut t = self.next_tick.0;
            while t <= now.0 {
                t += step;
            }
            Millis(t)
        };
        self.room_ends(room, now);
        self.run_checkpoints(now);
        self.decide(room, out);
        self.song_end(room, out);
        if let Some(bytes) = self.build_frame(now) {
            out.broadcast.push(bytes);
        }
    }

    /// Ends the room sees: a connection lost past the grace (or the room
    /// left), a competitor that never streamed by the late join's end.
    fn room_ends(&mut self, room: &RoomState, now: Millis) {
        let no_show = room_instant(self.song_zero_at, self.rules.late_join_ms as i64 * 1_000)
            .after(self.rules.report_deadline_ms);
        let computes = crate::live::cpus::anyone_computes(room);
        let ids: Vec<ParticipantId> = self.comps.keys().copied().collect();
        for p in ids {
            let c = &self.comps[&p];
            if c.end.is_some() {
                continue;
            }
            // A CPU's stream is the devices' that compute it: it ends when
            // nobody is left who could (a forfeit, as a dropped
            // connection's); it is never a no-show (its first report may
            // come at a checkpoint later than a person's first batch).
            if c.cpu {
                if !computes {
                    self.end_by_room(p, StreamEnd::Dropped);
                }
                continue;
            }
            let gone = room
                .member(p)
                .is_none_or(|m| m.left || m.connection == Connection::Disconnected);
            if gone {
                self.end_by_room(p, StreamEnd::Dropped);
            } else if c.live && !c.streamed && now >= no_show {
                self.end_by_room(p, StreamEnd::NoShow);
            }
        }
    }

    /// Every checkpoint whose reports are all in, or whose deadline passed.
    fn run_checkpoints(&mut self, now: Millis) {
        let Some(ledger) = self.ledger.as_mut() else {
            return;
        };
        while let Some((k, at)) = ledger.next_checkpoint() {
            let k32 = k as u32;
            let deadline = room_instant(self.song_zero_at, at).after(self.rules.report_deadline_ms);
            let seated: Vec<ParticipantId> = self
                .comps
                .iter()
                .filter(|(_, c)| c.live)
                .map(|(p, _)| *p)
                .collect();
            let all_in = seated.iter().all(|p| {
                let c = &self.comps[p];
                c.end.is_some() || c.reported.is_some_and(|r| r >= k32)
            });
            if !all_in && now < deadline {
                break;
            }
            let reports: Vec<Report> = seated
                .iter()
                .map(|p| self.comps.get_mut(p).expect("seated").report_for(k32, *p))
                .collect();
            let events = ledger.checkpoint(k, &reports);
            if let Some(record) = ledger.record().last() {
                self.pending_checkpoints.push(CheckpointNews {
                    k: k32,
                    record: record.clone(),
                });
            }
            for e in events {
                if let crate::attacks::AttackEvent::Committed { attack, .. } = e
                    && let Some(a) = ledger.attack(attack)
                {
                    let c = Commitment {
                        attack,
                        checkpoint: a.checkpoint as u32,
                        slot: a.slot as u32,
                        sender: a.sender,
                        recipient: a.recipient,
                    };
                    self.commitments.push(c);
                    self.pending_commitments.push(c);
                }
            }
        }
    }

    fn entrants(&self) -> Vec<Entrant> {
        self.comps
            .iter()
            .map(|(p, c)| Entrant {
                participant: *p,
                run: match c.end {
                    Some(StreamEnd::Out { at_us }) => Run::Out {
                        at: at_us as f64 / 1e6,
                    },
                    Some(e) if e.forfeit() => Run::Forfeit,
                    _ => Run::Live {
                        life: f64::from(c.last.map_or(c.life_ppm, |m| m.life_ppm)) / 1e6,
                        accuracy: 0.0,
                        absolute: None,
                        score: c.last.map(|m| m.score),
                        steps: c.steps,
                    },
                },
            })
            .collect()
    }

    /// The stage decided as practice decides it (`SETTLEMENT_V0`): at most
    /// one still in, no attack against them still to clear; confirmed.
    fn decide(&mut self, room: &RoomState, out: &mut LiveOut) {
        if self.decided || !self.early {
            return;
        }
        match room.phase {
            Phase::Stage { stage } if stage == self.id => {}
            Phase::SongCompletion { stage } if stage == self.id => {
                self.decided = true;
                return;
            }
            _ => return,
        }
        let entrants = self.entrants();
        if !crate::battle::decided(&entrants) {
            return;
        }
        let outstanding: Vec<ParticipantId> = self.ledger.as_ref().map_or_else(Vec::new, |l| {
            l.outstanding()
                .filter(|(p, clear)| {
                    self.comps
                        .get(p)
                        .is_none_or(|c| c.end.is_none() && c.through_us <= *clear)
                })
                .map(|(p, _)| p)
                .collect()
        });
        if !SETTLEMENT_V0.decided(&entrants, &outstanding) {
            if self.settling.is_none() {
                let survivor = entrants
                    .iter()
                    .find(|e| matches!(e.run, Run::Live { .. }))
                    .map(|e| e.participant);
                let last_out = entrants
                    .iter()
                    .filter_map(|e| match e.run {
                        Run::Out { at } => Some((at * 1e6).round() as i64),
                        _ => None,
                    })
                    .max();
                if let (Some(survivor), Some(last_out_us)) = (survivor, last_out) {
                    self.settling = Some(Settling {
                        survivor,
                        last_out_us,
                    });
                }
            }
            return;
        }
        let checkpoints = self.ledger.as_ref().map_or(&[][..], |l| l.checkpoints_us());
        let (ranking, _drawn) = SETTLEMENT_V0.rank(&entrants, checkpoints, self.settling);
        out.commands.push(Command::ConfirmStage {
            stage: self.id,
            ranking,
        });
        self.decided = true;
    }

    /// The song's end: every stream ended and every device that ended its
    /// own reported its run; those whose connection the room ended (or
    /// who never came) do not hold it up.
    fn song_end(&mut self, room: &RoomState, out: &mut LiveOut) {
        if self.song_end_sent || self.comps.values().any(|c| c.end.is_none()) {
            return;
        }
        let Some(play) = room.stage(self.id).and_then(|s| s.play.as_ref()) else {
            return;
        };
        if play.song_ended {
            return;
        }
        let missing: Vec<ParticipantId> = play
            .lineup
            .iter()
            .filter(|p| !play.reports.contains_key(p))
            .copied()
            .collect();
        if missing.is_empty()
            || missing.iter().any(|p| {
                !matches!(
                    self.comps.get(p).and_then(|c| c.end),
                    Some(StreamEnd::Dropped | StreamEnd::NoShow)
                )
            })
        {
            return;
        }
        let ranking = match room.phase {
            Phase::SongCompletion { stage } if stage == self.id => None,
            // Decided in this same tick: its `ConfirmStage` goes first and
            // puts the room in SongCompletion, where the song's end takes
            // no ranking (one would be refused, the song left waiting).
            Phase::Stage { stage } if stage == self.id && self.decided => None,
            Phase::Stage { stage } if stage == self.id => {
                let entrants: Vec<Entrant> = play
                    .lineup
                    .iter()
                    .map(|p| Entrant {
                        participant: *p,
                        run: play.reports.get(p).map_or(Run::Forfeit, |r| r.run()),
                    })
                    .collect();
                Some(crate::battle::rank(&entrants))
            }
            _ => return,
        };
        out.commands.push(Command::EndSong {
            stage: self.id,
            ranking,
        });
        self.song_end_sent = true;
    }

    fn build_frame(&mut self, now: Millis) -> Option<Vec<u8>> {
        if self.pending.is_empty()
            && self.pending_commitments.is_empty()
            && self.pending_checkpoints.is_empty()
            && self.settling == self.settling_sent
        {
            return None;
        }
        let frame = Frame {
            stage: self.id,
            n: self.next_n,
            at: now,
            entries: std::mem::take(&mut self.pending).into_values().collect(),
            commitments: std::mem::take(&mut self.pending_commitments),
            checkpoints: std::mem::take(&mut self.pending_checkpoints),
            settling: if self.settling != self.settling_sent {
                self.settling
            } else {
                None
            },
        };
        self.settling_sent = self.settling;
        let bytes = wire::encode_frame(&frame);
        self.kept.push_back((self.next_n, bytes.clone()));
        while self.kept.len() > self.rules.resync_window as usize {
            self.kept.pop_front();
        }
        self.next_n += 1;
        Some(bytes)
    }

    /// The frames after `after` where they are all kept; otherwise a
    /// snapshot and the last few frames.
    pub fn resume(&self, after: Option<u32>, now: Millis) -> Vec<LiveReply> {
        let last = self.next_n - 1;
        if let Some(a) = after
            && a <= last
            && self.kept.front().map_or(a == last, |(n, _)| *n <= a + 1)
        {
            return self
                .kept
                .iter()
                .filter(|(n, _)| *n > a)
                .map(|(_, b)| LiveReply::Frame(b.clone()))
                .collect();
        }
        let first = self
            .kept
            .front()
            .map_or(self.next_n, |(n, _)| *n)
            .max(self.next_n.saturating_sub(self.rules.snapshot_frames));
        let mut out = vec![LiveReply::Message(ServerLive::Snapshot(Box::new(
            self.snapshot(first, now),
        )))];
        out.extend(
            self.kept
                .iter()
                .filter(|(n, _)| *n >= first)
                .map(|(_, b)| LiveReply::Frame(b.clone())),
        );
        out
    }

    pub fn snapshot(&self, first_frame: u32, now: Millis) -> LiveSnapshot {
        LiveSnapshot {
            stage: self.id,
            song_zero_at: self.song_zero_at,
            at: now,
            first_frame,
            competitors: self
                .comps
                .iter()
                .map(|(p, c)| CompetitorStatus {
                    participant: *p,
                    live: c.live,
                    seq: c.seq,
                    through_us: if c.streamed { c.through_us } else { 0 },
                    end: c.end,
                    life_ppm: c.last.map_or(c.life_ppm, |m| m.life_ppm),
                    combo: c.last.map_or(0, |m| m.combo),
                    score: c.last.map_or(0, |m| m.score),
                    power: self.ledger.as_ref().map_or(0, |l| l.power(*p)),
                })
                .collect(),
            commitments: self.commitments.clone(),
            plan: self.plan.clone(),
            settling: self.settling,
        }
    }
}
