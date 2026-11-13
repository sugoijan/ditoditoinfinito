//! The room as its audience is shown it: a public view derived from the
//! room's state, slim and changing rarely, with nothing a member alone
//! should know (`docs/plans/multiplayer-audience.md`, "What an audience
//! device needs").
//!
//! In it: the room's name, its host's name, the phase, the match's rules
//! as plain values (the device words them), the players of the match (a
//! display name, a person, a bot or a CPU with its level, still in the
//! match or not), each stage's song (its card, its chart's digest, the
//! room's share once available, the battle plan's digest), each stage's
//! start, result and the starters' reported details, and the match's
//! standings.
//!
//! Never in it: identities (`twitch:…`, `bot:…`), the admin flag, roles,
//! connections, readiness and loading, intents of those who only watch
//! (members watching are not listed), the watch-only mark, targeting,
//! the invite's generation and admission, the picker, the next match's
//! draft, the spotlight, reports' raw runs, sit-out reasons, song keys of
//! the operators' libraries. A test walks a room with all of them set
//! and checks the view's JSON for each.

use crate::awards::PointsBias;
use crate::awards::StageResult;
use crate::battle::RunDetail;
use crate::ids::{ChartKey, Millis, ParticipantId, Revision, RoomId, StageId};
use crate::live::PlanDigest;
use crate::member::{Intent, MemberKind};
use crate::phase::Phase;
use crate::rules::{MatchDifficulty, MatchRules};
use crate::state::{ContentCard, ContentStatus, RoomState, Stage, Standing};
use serde::{Deserialize, Serialize};

/// What the audience is shown of the room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicView {
    pub room: RoomId,
    /// The room's revision it was derived at (only ever compared: a
    /// newer view replaces an older one).
    pub revision: Revision,
    /// The room's name, as its host gave it (the authority keeps it
    /// beside the state).
    pub name: String,
    /// The host's display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub phase: Phase,
    /// The room ended: nothing more will come.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
    /// The live timing's id the room plays by (`live-v2`), where it plays
    /// live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<String>,
    /// The running (or last) match's rules, else the next match's.
    pub rules: PublicRules,
    /// The match's players (or, outside a match, the members who want to
    /// play), by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub players: Vec<PublicPlayer>,
    /// The match's stages (or the next match's playlist), in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<PublicStage>,
    /// The match's standings, best first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub standings: Vec<Standing>,
}

/// The match's rules as plain values: the device words them as the
/// room's screens do (`room::rules_words`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRules {
    pub stages: u16,
    pub seats: u16,
    pub ruleset: String,
    #[serde(default, skip_serializing_if = "MatchDifficulty::is_normal")]
    pub difficulty: MatchDifficulty,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub boos: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub attacks: bool,
    #[serde(default, skip_serializing_if = "PointsBias::is_strong")]
    pub points: PointsBias,
    /// The charts it is played on (additive, left out while As picked;
    /// "At zero life", `at_zero`, before it read as it).
    #[serde(
        default,
        alias = "at_zero",
        skip_serializing_if = "crate::rules::Charts::is_as_picked"
    )]
    pub charts: crate::rules::Charts,
    /// Attacks at once (additive, left out at three).
    #[serde(default, skip_serializing_if = "crate::rules::AttacksAtOnce::is_default")]
    pub at_once: crate::rules::AttacksAtOnce,
}

impl PublicRules {
    fn of(r: &MatchRules) -> PublicRules {
        PublicRules {
            stages: r.stages,
            seats: r.seats,
            ruleset: r.ruleset_id().into(),
            difficulty: r.difficulty,
            boos: r.boos,
            attacks: r.attacks(),
            points: r.points,
            charts: r.charts,
            at_once: r.at_once,
        }
    }
}

/// What kind of player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicKind {
    Person,
    /// A simulated player (the server's bots), labeled Bot.
    Bot,
    /// A CPU of the match: its level (1 to 10, shown as stars). Its
    /// seed, personality and attack policy: what an audience device
    /// computes its play from where the room plays its CPUs online
    /// (`crate::live::cpus`; ADDITIVE, left out at their defaults).
    Cpu {
        level: u8,
        ordinal: u16,
        #[serde(default, skip_serializing_if = "is_zero")]
        seed: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        personality: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        targeting: Option<crate::attacks::Targeting>,
    },
}

/// One player as the audience sees them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicPlayer {
    pub id: ParticipantId,
    /// What to show (a CPU without a name: none, the device says "CPU n").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub kind: PublicKind,
    /// Of the match's roster (not: wants to play the next one).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_match: bool,
    /// Persistent elimination ended their match.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub out_for_match: bool,
    /// Left the room (kept: their results stay).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub left: bool,
}

/// Where a stage's song stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicContent {
    None,
    Pending,
    Available,
    Unavailable,
}

/// One stage as the audience sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicStage {
    pub id: StageId,
    pub content: PublicContent,
    /// The chart's digest (what finds the chart in the share).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<ChartKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<ContentCard>,
    /// The room's share of the song, once available: read by anyone with
    /// its id, as every share is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    /// The battle plan's digest, which an audience device checks its own
    /// plan against before drawing shocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battle: Option<PlanDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play: Option<PublicPlay>,
    /// The rules it is played under where they are not those the match
    /// began with (changed during the match from it or an earlier stage
    /// on, `Match::changes`). Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<PublicRules>,
}

/// A committed stage as the audience sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicPlay {
    pub lineup: Vec<ParticipantId>,
    pub start_at: Millis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song_zero_at: Option<Millis>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub song_ended: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<StageResult>,
    /// How the starters who said so performed (the Standings' lines).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<(ParticipantId, RunDetail)>,
}

fn public_stage(m: Option<&crate::state::Match>, s: &Stage) -> PublicStage {
    let content = s.content.as_ref();
    let status = match content.map(|c| &c.status) {
        None => PublicContent::None,
        Some(ContentStatus::Pending) => PublicContent::Pending,
        Some(ContentStatus::Available) => PublicContent::Available,
        Some(ContentStatus::Unavailable { .. }) => PublicContent::Unavailable,
    };
    PublicStage {
        id: s.id,
        content: status,
        chart: content.map(|c| c.id.chart.clone()),
        card: content.and_then(|c| c.card.clone()),
        share: content
            .filter(|c| c.status == ContentStatus::Available)
            .and_then(|c| c.share.clone()),
        battle: content.and_then(|c| c.battle.clone()),
        rules: m
            .filter(|m| m.stage_rules_changed(s.id))
            .map(|m| PublicRules::of(&m.rules_for(s.id))),
        play: s.play.as_ref().map(|p| PublicPlay {
            lineup: p.lineup.iter().copied().collect(),
            start_at: p.start_at,
            song_zero_at: p.song_zero_at,
            song_ended: p.song_ended,
            interrupted: p.interrupted,
            result: p.result.clone(),
            details: p.details.iter().map(|(k, d)| (*k, d.clone())).collect(),
        }),
    }
}

impl PublicView {
    /// The view of `room` named `name` (deterministic: the same state
    /// gives the same view, byte for byte as JSON).
    pub fn of(room: &RoomState, name: &str) -> PublicView {
        let current = room.current.as_ref();
        // The match's own rules; a stage played under others says them
        // on its own (`PublicStage::rules`).
        let rules = current.map_or(&room.next.rules, |m| &m.rules);
        let mut ids: Vec<ParticipantId> = current
            .map(|m| m.roster.keys().copied().collect())
            .unwrap_or_default();
        ids.extend(
            room.members
                .values()
                .filter(|m| !m.left && !m.is_cpu() && m.intent == Intent::Play)
                .map(|m| m.id),
        );
        ids.sort();
        ids.dedup();
        let players = ids
            .into_iter()
            .filter_map(|id| {
                let m = room.member(id)?;
                let seat = current.and_then(|c| c.roster.get(&id));
                let (name, kind) = match &m.kind {
                    MemberKind::Human { name, identity } => (
                        Some(name.clone()),
                        if identity.is_bot() {
                            PublicKind::Bot
                        } else {
                            PublicKind::Person
                        },
                    ),
                    MemberKind::Cpu { ordinal, opponent } => (
                        opponent.name.clone(),
                        PublicKind::Cpu {
                            level: opponent.level,
                            ordinal: *ordinal,
                            seed: opponent.seed,
                            personality: opponent.personality.clone(),
                            targeting: opponent.targeting,
                        },
                    ),
                };
                Some(PublicPlayer {
                    id,
                    name,
                    kind,
                    in_match: seat.is_some(),
                    out_for_match: seat.is_some_and(|s| s.out_for_match),
                    left: m.left,
                })
            })
            .collect();
        let stages = current
            .map_or(room.playlist.as_slice(), |m| m.stages.as_slice())
            .iter()
            .map(|s| public_stage(current, s))
            .collect();
        PublicView {
            room: room.room.clone(),
            revision: room.revision,
            name: name.into(),
            host: room.owner().and_then(|m| match &m.kind {
                MemberKind::Human { name, .. } => Some(name.clone()),
                MemberKind::Cpu { .. } => None,
            }),
            phase: room.phase.clone(),
            closed: room.closed.is_some(),
            live: room
                .policy
                .live
                .as_ref()
                .and_then(|l| l.rules())
                .map(|r| r.id.to_string()),
            rules: PublicRules::of(rules),
            players,
            stages,
            standings: room.standings().unwrap_or_default(),
        }
    }

    /// The same view but for its revision: what the audience would see is
    /// unchanged (a member's connection, readiness or intent to watch
    /// moved the room on, nothing shown).
    pub fn same_as(&self, other: &PublicView) -> bool {
        PublicView {
            revision: other.revision,
            ..self.clone()
        } == *other
    }
}

fn is_zero(x: &u64) -> bool {
    *x == 0
}
