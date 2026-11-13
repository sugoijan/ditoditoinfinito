//! The room's state: every fact the reducer keeps, and the views derived
//! from them (participation, eligibility, standings). Mutated only through
//! events (`reduce.rs`), so a client mirror that applies the same events
//! holds the same state.

use crate::awards::{Finish, Placement, StageResult};
use crate::battle::{ReportedRun, RunDetail};
use crate::ids::{
    ChartKey, ContentId, Identity, MatchId, Millis, ParticipantId, Revision, RoomId, StageId,
};
use crate::member::{Capability, Connection, Intent, Member, MemberKind, OpponentSpec, Role, can};
use crate::phase::{Interruption, Phase};
use crate::points::Points;
use crate::rules::{Access, ContentSource, MatchRules, RoomPolicy, RulesProblem};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomState {
    pub room: RoomId,
    pub revision: Revision,
    pub policy: RoomPolicy,
    pub source: ContentSource,
    pub access: Access,
    #[serde(with = "crate::ids::pairs")]
    pub members: BTreeMap<ParticipantId, Member>,
    /// The rules the next match will start with: the explicit next-match
    /// draft, never the running match's rules.
    pub next: NextMatchDraft,
    /// The next match's stages, in order; as many as `next.rules.stages`.
    /// In the lobby, readiness gathers on the first.
    pub playlist: Vec<Stage>,
    /// The running match, or the one just finished or interrupted until the
    /// room opens the next.
    pub current: Option<Match>,
    pub phase: Phase,
    /// Finished matches, oldest first, at most `policy.history`.
    pub history: Vec<MatchRecord>,
    pub counters: Counters,
    /// The song an operator's song select rests on, for the others to see
    /// and hear before it is picked (M7b), or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spotlight: Option<Spotlight>,
    /// One picker at a time ([`Picking`]): who chooses the room's songs
    /// now. `None` is a room from before the picker (made by, or kept
    /// on, an authority that predates it): every operator chooses there,
    /// as it was, and its owner is named as the picker
    /// ([`RoomState::picker`]). Additive to protocol 2: a client reads
    /// from its absence that the pick cannot be taken there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picking: Option<Picking>,
    /// What the room does when its host goes ([`Hosting`]). `None` is a
    /// room whose authority predates the host's succession, or one it has
    /// decided nothing for yet: this reducer reads it as Pass it on
    /// ([`RoomState::if_host_leaves`]) and writes the setting with the
    /// first change it makes there, so a client reads from its absence
    /// that the authority may not know the commands that came with it
    /// ([`crate::Command::SetHostLeaves`], [`crate::Command::MakeHost`])
    /// and never sends them. Additive to protocol 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosting: Option<Hosting>,
    /// The room ended ([`crate::Event::RoomClosed`]), and why: nothing is
    /// decided there any more (`Rejection::RoomClosed`), and its authority
    /// closes its sockets (4410). Additive to protocol 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<Closure>,
}

/// What a room does when its host goes (2026-11-12), the host's to choose
/// ([`crate::Command::SetHostLeaves`]): a room setting, not a match rule,
/// never locked by a match.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostLeaves {
    /// The room goes on under someone else ([`RoomState::heir`]): a
    /// co-host, the one who joined first among them, else the member who
    /// joined first; people who are connected before ones who are away,
    /// never a bot. With nobody to pass it to, a host who leaves closes
    /// the room; one whose connection is lost keeps it until someone
    /// connected is there to take it.
    #[default]
    PassOn,
    /// The room ends for everyone.
    Close,
}

/// The host's succession ([`RoomState::hosting`]). It applies when the
/// host leaves the room (at once), and when the host has been away past
/// the authority's patience (`Connection::Disconnected`: the server's
/// reconnect grace, the one a picker is given; Reconnecting keeps
/// everything). The new host has everything a host has; the one before is
/// a co-host from then on, also when they come back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hosting {
    #[serde(default)]
    pub if_host_leaves: HostLeaves,
}

/// Why a room ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Closure {
    /// Its host went and the room was not passed on: the setting says
    /// Close ([`HostLeaves::Close`]), or nobody was there to pass it to.
    HostLeft,
}

/// What the host's leaving would do now ([`RoomState::if_host_left`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostDeparture {
    /// The room goes on under this member.
    PassTo(ParticipantId),
    /// The room ends for everyone.
    Close,
}

/// Who chooses the room's songs now (2026-11-12): one picker at a time.
/// Only the picker rests on songs for the others to see
/// ([`crate::Command::Spotlight`]) and picks ([`crate::Command::Pick`],
/// [`crate::Command::ChooseContent`]); everyone else follows, co-hosts
/// and the owner included when they are not the picker. A room starts
/// with its owner; an operator takes the pick at once
/// ([`crate::Command::TakePick`]) and it stays with whoever has it, across
/// stages and matches, until someone takes it or they can hold it no
/// longer (they leave, lose the role, or are disconnected past the
/// authority's patience): then it is the owner's again, or nobody's
/// where the owner is the one gone.
///
/// The picker is "who chooses now", any participant in principle (a
/// later match rule may pass the pick around every player each stage);
/// only the taking is an operator's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Picking {
    /// Who chooses now; none: nobody, until an operator takes the pick.
    pub picker: Option<ParticipantId>,
    /// The room's shares of the songs a picker rested on when the pick
    /// changed hands (the spotlight goes with its picker), by chart,
    /// newest last, [`Picking::KEPT_SHARES`] at most: a rest on one of
    /// them again reuses it ([`RoomState::known_share`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shares: Vec<(ChartKey, String)>,
}

impl Picking {
    /// The shares kept across changes of hands.
    pub const KEPT_SHARES: usize = 8;

    pub(crate) fn keep_share(&mut self, chart: ChartKey, share: String) {
        self.shares.retain(|(c, _)| *c != chart);
        self.shares.push((chart, share));
        if self.shares.len() > Self::KEPT_SHARES {
            let drop = self.shares.len() - Self::KEPT_SHARES;
            self.shares.drain(..drop);
        }
    }
}

/// What the room shows of a song before anyone has loaded it: the words
/// the operator's library has for it. Display only, opaque to the room,
/// never checked against the song (the content id is what counts).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentCard {
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    /// The chart's difficulty as the chart names it (`Hard`), its meter and
    /// its style's id (`dance-single`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub difficulty: String,
    #[serde(default)]
    pub meter: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub style: String,
}

impl ContentCard {
    /// The longest text a card's field may carry (characters).
    pub const MAX_TEXT: usize = 200;

    /// Whether every text is within [`ContentCard::MAX_TEXT`].
    pub fn fits(&self) -> bool {
        [&self.title, &self.artist, &self.difficulty, &self.style]
            .iter()
            .all(|t| t.chars().count() <= Self::MAX_TEXT)
    }
}

/// The song (and chart) an operator's song select rests on (M7b): who,
/// what it is, and the room's share of it once uploaded (an online room's,
/// which a later pick of the same content reuses).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spotlight {
    pub content: ContentId,
    pub card: ContentCard,
    pub by: ParticipantId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
}

/// The next unused id of each kind. Ids are never reused.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counters {
    pub participant: u64,
    pub matches: u64,
    pub stage: u64,
}

/// The next match's rules as saved by an operator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextMatchDraft {
    pub rules: MatchRules,
    /// When it was last saved, or last retargeted by a match start: an edit
    /// made against an older view is stale.
    pub saved_at: Revision,
    /// Set when the boundary's revalidation found these rules unusable now
    /// (more reserved players than seats, say). Blocks the next start until
    /// an operator saves rules that pass.
    pub review: Option<RulesProblem>,
}

/// One match: its locked rules, its roster and its stages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    pub id: MatchId,
    pub rules: MatchRules,
    #[serde(with = "crate::ids::pairs")]
    pub roster: BTreeMap<ParticipantId, Seat>,
    pub stages: Vec<Stage>,
}

/// A competitor's place in a match.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    /// The first stage they could play: the match's first, or a later one
    /// for a late entrant (earlier stages count zero).
    pub first_stage: StageId,
    /// Persistent elimination ended their match.
    pub out_for_match: bool,
}

/// One stage: what it plays, who prepared for it and how it went.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage {
    pub id: StageId,
    pub content: Option<StageContent>,
    /// When the content was last chosen; an edit against an older view is
    /// stale.
    pub content_changed_at: Revision,
    /// Bumped whenever this stage's readiness is invalidated (its content
    /// replaced, the lobby's rules changed). Ready and Start carry it.
    pub readiness_rev: u32,
    /// Readiness and loading for this stage, per person. Absent means not
    /// ready and nothing loaded.
    #[serde(with = "crate::ids::pairs")]
    pub prep: BTreeMap<ParticipantId, Prep>,
    /// Set when its start was committed.
    pub play: Option<StagePlay>,
}

impl Stage {
    pub fn new(id: StageId) -> Self {
        Self {
            id,
            content: None,
            content_changed_at: Revision(0),
            readiness_rev: 0,
            prep: BTreeMap::new(),
            play: None,
        }
    }

    pub fn prep(&self, id: ParticipantId) -> Prep {
        self.prep.get(&id).cloned().unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageContent {
    pub id: ContentId,
    pub status: ContentStatus,
    /// Where the members fetch it (an online room's: the share its upload
    /// made, `ddi_share_protocol`), once available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    /// What to show of it before it is loaded (M7b's picks history).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<ContentCard>,
    /// The digest of the battle plan the room keeps for it (live play
    /// with attacks, `crate::live::plan`), which every device compares
    /// its own with before it says it is ready to play live. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battle: Option<crate::live::PlanDigest>,
}

/// Whether the room can use a stage's content, as its authority knows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentStatus {
    /// Being shared; nobody can load it yet.
    Pending,
    Available,
    /// Sharing failed or the content is gone; `code` says why.
    Unavailable {
        code: String,
    },
}

/// One person's preparation for one stage. Ready is intent and may be set
/// while loading; only ready and loaded is eligible.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prep {
    pub ready: bool,
    pub loading: Loading,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Loading {
    #[default]
    NotStarted,
    Loading {
        permille: u16,
    },
    Loaded,
    Failed {
        code: String,
    },
}

/// A committed stage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagePlay {
    pub lineup: BTreeSet<ParticipantId>,
    /// Match competitors who did not start, and why. They earn zero.
    #[serde(with = "crate::ids::pairs")]
    pub sat_out: BTreeMap<ParticipantId, SitOutReason>,
    pub start_at: Millis,
    pub result: Option<StageResult>,
    pub song_ended: bool,
    /// The match stopped before this stage's result was confirmed: no award.
    pub interrupted: bool,
    /// The starters' own reports of their runs (an online stage's, M5d).
    #[serde(default, with = "crate::ids::pairs")]
    pub reports: BTreeMap<ParticipantId, ReportedRun>,
    /// When the first report of the song played to its end came.
    #[serde(default)]
    pub song_end_at: Option<Millis>,
    /// How the starters who said so performed ([`RunDetail`], with their
    /// reports): shown on the stage's standings, never ranked by.
    #[serde(
        default,
        with = "crate::ids::pairs",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub details: BTreeMap<ParticipantId, RunDetail>,
    /// Where the room plays live: the instant of the song's time zero on
    /// the room's clock (`Event::StartCommitted`). Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song_zero_at: Option<Millis>,
}

impl StagePlay {
    /// Its lineup has no CPU: its result comes from its starters' reports.
    pub fn self_reported(&self, members: &BTreeMap<ParticipantId, Member>) -> bool {
        !self
            .lineup
            .iter()
            .any(|id| members.get(id).is_some_and(Member::is_cpu))
    }
}

/// Why a match competitor did not start a stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SitOutReason {
    NotReady,
    StillLoading,
    LoadFailed,
    NotConnected,
    Watching,
    Left,
}

impl SitOutReason {
    /// Whether the person could still become eligible before a deadline,
    /// which is what a readiness window waits for.
    pub fn may_still_join(self) -> bool {
        matches!(
            self,
            Self::NotReady | Self::StillLoading | Self::LoadFailed | Self::NotConnected
        )
    }
}

/// A finished match, kept in the room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRecord {
    pub played: Match,
    /// The match's CPU seats, which left the room with it.
    #[serde(with = "crate::ids::pairs")]
    pub cpus: BTreeMap<ParticipantId, Member>,
    pub ending: MatchEnding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchEnding {
    Completed,
    /// Partial results: confirmed stages only, no overall winner.
    Interrupted {
        reason: Interruption,
    },
}

impl MatchRecord {
    /// The overall winner or tied winners; none for an interrupted match.
    pub fn winners(&self) -> Vec<ParticipantId> {
        match self.ending {
            MatchEnding::Completed => winners(&self.played.standings()),
            MatchEnding::Interrupted { .. } => Vec::new(),
        }
    }
}

fn winners(standings: &[Standing]) -> Vec<ParticipantId> {
    standings
        .iter()
        .filter(|s| s.rank == 1)
        .map(|s| s.participant)
        .collect()
}

/// A competitor's standing in a match.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standing {
    pub participant: ParticipantId,
    /// Equal totals share a rank.
    pub rank: u16,
    pub total: Points,
    /// One entry per stage of the match, in order.
    pub stages: Vec<StageStanding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStanding {
    Placed(Placement),
    SatOut(SitOutReason),
    /// Not played: before their entry, or not reached.
    NotPlayed,
    /// Started without a confirmed result: running, or interrupted (no award).
    Unconfirmed,
}

impl Match {
    pub fn stage(&self, id: StageId) -> Option<&Stage> {
        self.stages.iter().find(|s| s.id == id)
    }

    pub fn stage_index(&self, id: StageId) -> Option<usize> {
        self.stages.iter().position(|s| s.id == id)
    }

    /// Stage awards summed per competitor (equal stage weights; a missed or
    /// unconfirmed stage is zero, never an estimate).
    pub fn standings(&self) -> Vec<Standing> {
        let mut out: Vec<Standing> = self
            .roster
            .keys()
            .map(|&id| {
                let stages: Vec<StageStanding> = self
                    .stages
                    .iter()
                    .map(|stage| match &stage.play {
                        None => StageStanding::NotPlayed,
                        Some(play) => {
                            if let Some(p) = play.result.as_ref().and_then(|r| r.placement(id)) {
                                StageStanding::Placed(*p)
                            } else if let Some(reason) = play.sat_out.get(&id) {
                                StageStanding::SatOut(*reason)
                            } else if play.lineup.contains(&id) {
                                StageStanding::Unconfirmed
                            } else {
                                StageStanding::NotPlayed
                            }
                        }
                    })
                    .collect();
                let total = stages
                    .iter()
                    .map(|s| match s {
                        StageStanding::Placed(p) => p.points,
                        _ => Points::ZERO,
                    })
                    .sum();
                Standing {
                    participant: id,
                    rank: 0,
                    total,
                    stages,
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.total
                .cmp(&a.total)
                .then(a.participant.cmp(&b.participant))
        });
        let totals: Vec<Points> = out.iter().map(|s| s.total).collect();
        for s in &mut out {
            let above = totals.iter().filter(|t| **t > s.total).count();
            s.rank = u16::try_from(above + 1).unwrap_or(u16::MAX);
        }
        out
    }
}

/// What a member is doing now, derived from the room's separate facts
/// (`docs/plans/multiplayer-ui.md`, State model). Viewing results is not
/// among them: it changes nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Participation {
    Left,
    /// Chose to watch (an admin observer, say).
    Watching,
    /// Wants to play; arrived during the match and waits for the next.
    WaitingNextMatch,
    /// Persistent elimination ended their match.
    OutForMatch,
    /// Admitted for the upcoming stage, not ready.
    NotReady {
        stage: StageId,
    },
    /// Ready requested; loading the stage's content.
    ReadyLoading {
        stage: StageId,
    },
    /// Ready and loaded: eligible.
    ReadyLoaded {
        stage: StageId,
    },
    /// The required content failed to load.
    LoadFailed {
        stage: StageId,
    },
    /// In the committed lineup, counting down.
    Committed {
        stage: StageId,
    },
    Competing {
        stage: StageId,
    },
    /// Confirmed winner, finishing the song (adds nothing).
    WinnerFinishing {
        stage: StageId,
    },
    /// Confirmed place in a stage whose winner may still be playing.
    Placed {
        stage: StageId,
        place: u16,
        finish: Finish,
    },
    SittingOut {
        stage: StageId,
        reason: SitOutReason,
    },
    /// Nothing to prepare: the match is over or interrupted.
    Idle,
}

impl RoomState {
    /// A new room in its lobby, its owner the first member.
    pub fn new(
        room: RoomId,
        owner: Identity,
        owner_name: String,
        policy: RoomPolicy,
        rules: MatchRules,
        source: ContentSource,
    ) -> Self {
        let owner_id = ParticipantId(1);
        let playlist: Vec<Stage> = (1..=u64::from(rules.stages))
            .map(|i| Stage::new(StageId(i)))
            .collect();
        let mut members = BTreeMap::new();
        members.insert(
            owner_id,
            Member {
                id: owner_id,
                kind: MemberKind::Human {
                    identity: owner,
                    name: owner_name,
                },
                role: Role::Owner,
                admin: false,
                intent: Intent::Play,
                connection: Connection::Connected,
                left: false,
                targeting: Default::default(),
            },
        );
        Self {
            room,
            revision: Revision(0),
            counters: Counters {
                participant: 2,
                matches: 1,
                stage: playlist.len() as u64 + 1,
            },
            policy,
            source,
            access: Access::default(),
            members,
            next: NextMatchDraft {
                rules,
                saved_at: Revision(0),
                review: None,
            },
            playlist,
            current: None,
            phase: Phase::Lobby,
            history: Vec::new(),
            spotlight: None,
            picking: Some(Picking {
                picker: Some(owner_id),
                shares: Vec::new(),
            }),
            hosting: Some(Hosting::default()),
            closed: None,
        }
    }

    /// What the room does when its host goes; a room that does not say
    /// (from before the setting) passes it on.
    pub fn if_host_leaves(&self) -> HostLeaves {
        self.hosting
            .as_ref()
            .map(|h| h.if_host_leaves)
            .unwrap_or_default()
    }

    /// Whether the room's authority runs the host's succession, as far as
    /// this state says ([`RoomState::hosting`]): what a client checks
    /// before it sends [`crate::Command::SetHostLeaves`] or
    /// [`crate::Command::MakeHost`], or says what a host's leaving does.
    pub fn has_succession(&self) -> bool {
        self.hosting.is_some()
    }

    /// Who the room passes to when host `from` goes
    /// ([`HostLeaves::PassOn`]): among the people present (never a bot: a
    /// bot follows a room, it does not run one), those connected first,
    /// then those whose connection is coming or coming back, then those
    /// away, and only with `away_too`; among equals a co-host before a
    /// member, then whoever joined first. None: nobody to pass it to.
    pub fn heir(&self, from: ParticipantId, away_too: bool) -> Option<ParticipantId> {
        let rank = |c: Connection| match c {
            Connection::Connected => 0,
            Connection::Joining | Connection::Reconnecting => 1,
            Connection::Disconnected => 2,
        };
        self.members
            .values()
            .filter(|m| m.id != from && !m.left && !m.is_cpu() && !m.is_bot())
            .filter(|m| away_too || m.connection == Connection::Connected)
            .min_by_key(|m| (rank(m.connection), m.role != Role::CoHost, m.id))
            .map(|m| m.id)
    }

    /// What the host's leaving the room now would do, by the room's
    /// setting: for a screen to say before they leave.
    pub fn if_host_left(&self) -> HostDeparture {
        let heir = self.owner().and_then(|o| self.heir(o.id, true));
        match (self.if_host_leaves(), heir) {
            (HostLeaves::PassOn, Some(heir)) => HostDeparture::PassTo(heir),
            _ => HostDeparture::Close,
        }
    }

    /// The room's owner (the host).
    pub fn owner(&self) -> Option<&Member> {
        self.members.values().find(|m| m.role == Role::Owner)
    }

    /// Whether the room has one picker at a time ([`Picking`]); a room
    /// from before the picker has not, and every operator chooses there.
    pub fn one_picker(&self) -> bool {
        self.picking.is_some()
    }

    /// Who chooses the room's songs now; none: nobody, until an operator
    /// takes the pick. A room from before the picker names its owner.
    pub fn picker(&self) -> Option<ParticipantId> {
        match &self.picking {
            Some(p) => p.picker,
            None => self.owner().map(|m| m.id),
        }
    }

    /// Whether `id` chooses the room's songs now (rests on them for the
    /// others, picks them): the picker; in a room from before the picker,
    /// every operator.
    pub fn chooses(&self, id: ParticipantId) -> bool {
        let Some(m) = self.members.get(&id).filter(|m| !m.left && !m.is_cpu()) else {
            return false;
        };
        match &self.picking {
            Some(p) => p.picker == Some(id),
            None => can(
                m.role,
                m.admin,
                Capability::ChooseContent,
                self.phase.kind(),
            ),
        }
    }

    /// Whether `id` may take the pick now ([`crate::Command::TakePick`]):
    /// an operator who does not have it, in a room with one picker.
    pub fn may_take_pick(&self, id: ParticipantId) -> bool {
        self.one_picker()
            && self.picker() != Some(id)
            && self
                .members
                .get(&id)
                .filter(|m| !m.left && !m.is_cpu())
                .is_some_and(|m| can(m.role, m.admin, Capability::TakePick, self.phase.kind()))
    }

    pub fn member(&self, id: ParticipantId) -> Option<&Member> {
        self.members.get(&id)
    }

    pub fn member_by_identity(&self, identity: &Identity) -> Option<&Member> {
        self.members
            .values()
            .find(|m| m.identity() == Some(identity))
    }

    /// A stage of the running match or of the next match's playlist.
    pub fn stage(&self, id: StageId) -> Option<&Stage> {
        self.current
            .as_ref()
            .and_then(|m| m.stage(id))
            .or_else(|| self.playlist.iter().find(|s| s.id == id))
    }

    pub(crate) fn stage_mut(&mut self, id: StageId) -> Option<&mut Stage> {
        if let Some(m) = &mut self.current
            && let Some(s) = m.stages.iter_mut().find(|s| s.id == id)
        {
            return Some(s);
        }
        self.playlist.iter_mut().find(|s| s.id == id)
    }

    /// The stage gathering readiness now, if any.
    pub fn upcoming_stage(&self) -> Option<StageId> {
        match &self.phase {
            Phase::Lobby => self.playlist.first().map(|s| s.id),
            Phase::Preparing { stage, .. } => Some(*stage),
            Phase::Intermission { next } => Some(*next),
            _ => None,
        }
    }

    /// The room's next time-driven step on the authority's clock, if any:
    /// the readiness window's deadline or the countdown's start. An
    /// authority runs [`RoomState::advance`] then (a server's alarm).
    pub fn next_deadline(&self) -> Option<Millis> {
        match &self.phase {
            // A window waits while its song is still being shared.
            Phase::Preparing {
                stage,
                window: Some(w),
            } => (!self.content_pending(*stage)).then_some(w.deadline),
            Phase::Countdown { start_at, .. } => Some(*start_at),
            Phase::Stage { stage } | Phase::SongCompletion { stage } => {
                self.report_deadline(*stage)
            }
            _ => None,
        }
    }

    /// Whether `stage`'s chosen content is still being shared.
    pub fn content_pending(&self, stage: StageId) -> bool {
        self.stage(stage)
            .and_then(|s| s.content.as_ref())
            .is_some_and(|c| c.status == ContentStatus::Pending)
    }

    /// A share the room already has of `content`'s chart (shares are told
    /// apart by their chart's digest, as the server's reuse is): the
    /// spotlight's, one kept from a spotlight that went with its picker
    /// ([`Picking::shares`]), or an available stage's (this match's, the
    /// next one's, or a recorded match's), which a pick of the same chart
    /// reuses.
    pub fn known_share(&self, content: &ContentId) -> Option<String> {
        if let Some(s) = &self.spotlight
            && s.content.chart == content.chart
            && s.share.is_some()
        {
            return s.share.clone();
        }
        if let Some((_, share)) = self
            .picking
            .iter()
            .flat_map(|p| p.shares.iter().rev())
            .find(|(chart, _)| *chart == content.chart)
        {
            return Some(share.clone());
        }
        self.current
            .iter()
            .flat_map(|m| m.stages.iter())
            .chain(self.playlist.iter())
            .chain(self.history.iter().flat_map(|r| r.played.stages.iter()))
            .filter_map(|s| s.content.as_ref())
            .find(|c| c.id.chart == content.chart && c.status == ContentStatus::Available)
            .and_then(|c| c.share.clone())
    }

    /// When a self-reported stage (`StagePlay::self_reported`) settles
    /// without the reports still missing (`OutcomePolicy`).
    pub fn report_deadline(&self, stage: StageId) -> Option<Millis> {
        let play = self.stage(stage)?.play.as_ref()?;
        // A live room's result confirmed before the song's end still waits
        // for the reports (the standings' detail) until the song ends.
        let waits = play.result.is_none()
            || (play.song_zero_at.is_some()
                && !play.song_ended
                && matches!(self.phase, Phase::SongCompletion { stage: s } if s == stage));
        if !play.self_reported(&self.members) || !waits {
            return None;
        }
        let p = &self.policy.outcomes;
        let cap = play.start_at.after(p.stage_cap_ms);
        Some(match play.song_end_at {
            Some(at) => at.after(p.after_song_end_ms).min(cap),
            None => cap,
        })
    }

    /// The running match (not one that already ended).
    pub fn running_match(&self) -> Option<&Match> {
        self.current
            .as_ref()
            .filter(|_| self.phase.kind().match_running())
    }

    /// People present who want a competitive seat: the human seats the next
    /// match must hold.
    pub fn reserved_humans(&self) -> usize {
        self.members
            .values()
            .filter(|m| !m.left && !m.is_cpu() && m.intent == Intent::Play)
            .count()
    }

    /// Every player's place is taken: one more person who wants to play
    /// would pass the competitor cap (`RoomPolicy::check_competitors`).
    /// A newcomer then joins to watch ([`RoomState::arrival_intent`]), and
    /// a watcher's Play is refused (`Rejection::Cap`) until a place frees.
    pub fn players_full(&self) -> bool {
        self.policy
            .check_competitors(self.reserved_humans() + 1)
            .is_err()
    }

    /// What a newcomer who asks for `wanted` joins as: the one decision
    /// of a join's intent, which the reducer's `Command::Join` makes and
    /// an authority's preview says ahead (`players_full`). Wanting to play
    /// where every player's place is taken joins to watch, never refused
    /// for it (`join-watch-v0`, PROVISIONAL: nobody is moved to Play when
    /// a place frees up later; the watcher chooses Play themselves). A
    /// room's member limit (`RoomPolicy::max_members`) still refuses.
    pub fn arrival_intent(&self, wanted: Intent) -> Intent {
        if wanted == Intent::Play && self.players_full() {
            Intent::Watch
        } else {
            wanted
        }
    }

    /// Why `id` would not start `stage` now, or `None` if eligible (a CPU,
    /// or a connected person who is ready, loaded and wants to play).
    pub fn sit_out_reason(&self, stage: &Stage, id: ParticipantId) -> Option<SitOutReason> {
        let Some(m) = self.members.get(&id) else {
            return Some(SitOutReason::Left);
        };
        if m.left {
            return Some(SitOutReason::Left);
        }
        if m.is_cpu() {
            return None;
        }
        if m.intent == Intent::Watch {
            return Some(SitOutReason::Watching);
        }
        if m.connection != Connection::Connected {
            return Some(SitOutReason::NotConnected);
        }
        let prep = stage.prep(id);
        match prep.loading {
            Loading::Failed { .. } => Some(SitOutReason::LoadFailed),
            _ if !prep.ready => Some(SitOutReason::NotReady),
            Loading::Loaded => None,
            _ => Some(SitOutReason::StillLoading),
        }
    }

    /// The match's competitors who may start its next stage: its roster
    /// less those persistent elimination took out.
    pub fn candidates(&self) -> Vec<ParticipantId> {
        self.current
            .iter()
            .flat_map(|m| m.roster.iter())
            .filter(|(_, seat)| !seat.out_for_match)
            .map(|(id, _)| *id)
            .collect()
    }

    /// The running match's standings, or `None` outside a match.
    pub fn standings(&self) -> Option<Vec<Standing>> {
        self.current.as_ref().map(Match::standings)
    }

    /// The overall winners, once the match's results are in.
    pub fn match_winners(&self) -> Vec<ParticipantId> {
        match (&self.phase, &self.current) {
            (Phase::MatchResults, Some(m)) => winners(&m.standings()),
            _ => Vec::new(),
        }
    }

    pub fn participation(&self, id: ParticipantId) -> Option<Participation> {
        let m = self.members.get(&id)?;
        if m.left {
            return Some(Participation::Left);
        }
        let prepared = |stage: StageId| {
            let Some(s) = self.stage(stage) else {
                return Participation::Idle;
            };
            let prep = s.prep(id);
            match (prep.ready, &prep.loading) {
                (_, Loading::Failed { .. }) => Participation::LoadFailed { stage },
                (false, _) => Participation::NotReady { stage },
                (true, Loading::Loaded) => Participation::ReadyLoaded { stage },
                (true, _) => Participation::ReadyLoading { stage },
            }
        };
        let Some(current) = self.running_match() else {
            return Some(match &self.phase {
                Phase::Lobby if m.intent == Intent::Watch => Participation::Watching,
                Phase::Lobby => self.upcoming_stage().map_or(Participation::Idle, prepared),
                _ => Participation::Idle,
            });
        };
        let Some(seat) = current.roster.get(&id) else {
            return Some(match m.intent {
                Intent::Play => Participation::WaitingNextMatch,
                Intent::Watch => Participation::Watching,
            });
        };
        if seat.out_for_match {
            return Some(Participation::OutForMatch);
        }
        Some(match &self.phase {
            Phase::Preparing { stage, .. } | Phase::Intermission { next: stage } => {
                if m.intent == Intent::Watch {
                    Participation::Watching
                } else {
                    prepared(*stage)
                }
            }
            Phase::Countdown { stage, .. }
            | Phase::Stage { stage }
            | Phase::SongCompletion { stage } => {
                let play = current.stage(*stage).and_then(|s| s.play.as_ref());
                match play {
                    Some(play) if play.lineup.contains(&id) => {
                        match (
                            &self.phase,
                            play.result.as_ref().and_then(|r| r.placement(id)),
                        ) {
                            (Phase::Countdown { .. }, _) => {
                                Participation::Committed { stage: *stage }
                            }
                            (_, Some(p)) if p.place == 1 && !play.song_ended => {
                                Participation::WinnerFinishing { stage: *stage }
                            }
                            (_, Some(p)) => Participation::Placed {
                                stage: *stage,
                                place: p.place,
                                finish: p.finish,
                            },
                            _ => Participation::Competing { stage: *stage },
                        }
                    }
                    Some(play) => match play.sat_out.get(&id) {
                        Some(reason) => Participation::SittingOut {
                            stage: *stage,
                            reason: *reason,
                        },
                        None => Participation::Watching,
                    },
                    None => Participation::Idle,
                }
            }
            _ => Participation::Idle,
        })
    }

    /// The CPU profile behind a member, if it is a CPU seat.
    pub fn opponent(&self, id: ParticipantId) -> Option<&OpponentSpec> {
        match &self.members.get(&id)?.kind {
            MemberKind::Cpu { opponent, .. } => Some(opponent),
            MemberKind::Human { .. } => None,
        }
    }
}
