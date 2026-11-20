//! The room's state: every fact the reducer keeps, and the views derived
//! from them (participation, eligibility, standings). Mutated only through
//! events (`reduce.rs`), so a client mirror that applies the same events
//! holds the same state.

use crate::awards::{Finish, Placement, StageResult};
use crate::battle::{Entrant, ReportedRun, Run, RunDetail};
use crate::ids::{
    ChartKey, ContentId, Identity, MatchId, Millis, ParticipantId, Revision, RoomId, RoomSongId,
    StageId,
};
use crate::member::{Capability, Connection, Intent, Member, MemberKind, OpponentSpec, Role, can};
use crate::phase::{Interruption, Phase};
use crate::points::Points;
use crate::rules::{Access, ContentSource, MatchRules, PlayRules, RoomPolicy, RulesProblem};
use crate::songs::RoomSongs;
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
    /// now. A room stored without it (from before the picker) reads as
    /// one nobody's pick is held in, which its owner is given with the
    /// next change while there ([`RoomState::apply`]'s settling).
    #[serde(default)]
    pub picking: Picking,
    /// What the room does when its host goes ([`Hosting`]). A room stored
    /// without it (from before the host's succession) reads as Pass it on.
    #[serde(default)]
    pub hosting: Hosting,
    /// The room ended ([`crate::Event::RoomClosed`]), and why: nothing is
    /// decided there any more (`Rejection::RoomClosed`), and its authority
    /// closes its sockets (4410). Additive to protocol 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<Closure>,
    /// The room's song list (room model revision 17, [`crate::songs`]):
    /// an online room's stages are picked from it. Behind a shared
    /// pointer, so the copy a command is decided on costs nothing where
    /// the command does not change the list. A room stored before it
    /// reads with none, and one that never had one is written without it.
    #[serde(default, skip_serializing_if = "songs_untouched")]
    pub songs: std::sync::Arc<RoomSongs>,
    /// The host's Twitch in the room (room model revision 18,
    /// [`crate::twitch`]): whose bans (and blocks) keep people out, and
    /// how much of their chat the room may show; none: off. Set only by
    /// the authority ([`crate::Command::SetTwitch`]), which holds the
    /// person's Twitch access; ends when that person leaves the room. A
    /// room stored before reads with none, and one that never had it is
    /// written without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub twitch: Option<crate::twitch::TwitchSetting>,
}

fn songs_untouched(songs: &std::sync::Arc<RoomSongs>) -> bool {
    songs.is_empty()
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
    /// The server's operator closed it ([`crate::Command::CloseRoom`],
    /// round D1): the screens say "The room was closed." Additive to
    /// revision 11: an authority before it never says it.
    ByAdministrator,
}

/// Who the room passes to when its host goes ([`RoomState::succession`];
/// `succession-v1`, the maintainer, 2026-11-13).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Succession {
    /// To this member, now.
    To(ParticipantId),
    /// To this co-host, who is away but inside the reconnect grace
    /// (`Reconnecting`, which the authority ends with `Disconnected` when
    /// the grace runs out): nobody is made anything while the room waits;
    /// it passes to them once back, or on to the next in line once they
    /// are `Disconnected`. (Never `Joining`: no grace times that.)
    Wait(ParticipantId),
    /// Nobody to pass it to.
    Nobody,
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
/// ([`crate::Command::Spotlight`]) and picks ([`crate::Command::Pick`]);
/// everyone else follows, co-hosts
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
    /// Its Shocks per attack Four where stored before the rule
    /// ([`crate::rules::draft_rules`]).
    #[serde(deserialize_with = "crate::rules::draft_rules")]
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
    /// The rules the match began under, locked. A stage is played under
    /// [`Match::rules_for`]: these, or a change from it on.
    pub rules: MatchRules,
    #[serde(with = "crate::ids::pairs")]
    pub roster: BTreeMap<ParticipantId, Seat>,
    pub stages: Vec<Stage>,
    /// The per-stage rules changed during the match
    /// (`crate::Command::ChangeStageRules`, 2026-11-13), in the order of
    /// the stages they land on, each from its stage to the next change or
    /// the match's end. ADDITIVE to protocol 2: left out of the JSON while
    /// empty (every match whose rules nobody changed mid-match), read as
    /// empty where missing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<RulesChange>,
}

/// A change of a running match's per-stage rules ([`Match::changes`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesChange {
    /// The first stage played under them.
    pub from: StageId,
    pub rules: crate::rules::PlayRules,
    /// The revision that saved it: a change made against an older view of
    /// the changes is stale.
    pub saved_at: Revision,
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
    /// An open-ended match's last song (room model revision 15,
    /// `MatchRules::open_ended`): the match ends once it settles.
    /// ADDITIVE: left out of the JSON while false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last: bool,
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
            last: false,
        }
    }

    pub fn prep(&self, id: ParticipantId) -> Prep {
        self.prep_ref(id).clone()
    }

    /// The same, borrowed (no copy of a failure's code).
    pub fn prep_ref(&self, id: ParticipantId) -> &Prep {
        static NONE: Prep = Prep {
            ready: false,
            loading: Loading::NotStarted,
        };
        self.prep.get(&id).unwrap_or(&NONE)
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
    /// The entry of the room's song list it plays (revision 17,
    /// [`crate::Command::PickListed`]); none for a pick made otherwise.
    /// Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song: Option<RoomSongId>,
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
    /// The song's download failed and is being tried again by itself
    /// (room model revision 16, round AC: the client's `load-retry-v0`):
    /// loading as far as the room is concerned, and while it lasts the
    /// readiness window does not end early on everyone else being ready,
    /// up to the loading grace's end (`ReadinessWindow::hold_until`);
    /// the others' status lines say whom the room waits for.
    Retrying,
}

impl Loading {
    /// Still loading as the room waits for it: not started, loading or
    /// trying its download again.
    pub fn in_progress(&self) -> bool {
        matches!(self, Loading::NotStarted | Loading::Loading { .. } | Loading::Retrying)
    }
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
    /// The starters as the battle ranks them: each its own report's run,
    /// a forfeit where none came.
    pub fn entrants(&self) -> Vec<Entrant> {
        self.lineup
            .iter()
            .map(|id| Entrant {
                participant: *id,
                run: self.reports.get(id).map_or(Run::Forfeit, |r| r.run()),
            })
            .collect()
    }

    /// Its lineup has no CPU: its result comes from its starters' reports.
    pub fn self_reported(&self, members: &BTreeMap<ParticipantId, Member>) -> bool {
        !self
            .lineup
            .iter()
            .any(|id| members.get(id).is_some_and(Member::is_cpu))
    }

    /// Its result comes from reports: self-reported, or a stage an online
    /// room plays live: its people report their own runs, the authority
    /// its CPUs' for the devices that computed them (`crate::live::cpus`).
    pub fn reported(&self, members: &BTreeMap<ParticipantId, Member>) -> bool {
        self.self_reported(members) || self.song_zero_at.is_some()
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

    /// The rules stage `stage` is (or was, or will be) played under: the
    /// match's, with the last change from it or an earlier stage on in
    /// place of its per-stage rules ([`Match::changes`]).
    pub fn rules_for(&self, stage: StageId) -> MatchRules {
        match self.change_for(stage) {
            Some(c) => self.rules.with_play(&c.rules),
            None => self.rules.clone(),
        }
    }

    /// `rules` as a stage of this match plays them ([`PlayRules`]), the
    /// points' bias named only where it is not the match's own: the one
    /// form a change is sent, compared and kept in.
    pub fn play_rules_of(&self, rules: &MatchRules) -> PlayRules {
        PlayRules {
            points: (rules.points != self.rules.points).then_some(rules.points),
            ..rules.play_rules()
        }
    }

    /// The per-stage rules stage `stage` is played under, in
    /// [`Match::play_rules_of`]'s form.
    pub fn play_rules_for(&self, stage: StageId) -> PlayRules {
        self.play_rules_of(&self.rules_for(stage))
    }

    /// The change stage `stage` is played under, if any.
    pub fn change_for(&self, stage: StageId) -> Option<&RulesChange> {
        let at = self.stage_index(stage)?;
        self.changes
            .iter()
            .rev()
            .find(|c| self.stage_index(c.from).is_some_and(|i| i <= at))
    }

    /// Whether stage `stage` is played under other per-stage rules than
    /// the match began with.
    pub fn stage_rules_changed(&self, stage: StageId) -> bool {
        // Compared as played: a match stored before revision 16 and a
        // change saved since differ in how they are written alone.
        self.change_for(stage)
            .is_some_and(|c| c.rules.clone().normalized() != self.rules.play_rules().normalized())
    }

    /// When the changes were last saved (none: never).
    pub fn changes_saved_at(&self) -> Revision {
        self.changes.last().map_or(Revision(0), |c| c.saved_at)
    }

    /// Whether stage `index` has begun: its song picked (its readiness
    /// window open, `window`: a pick opens it with the song), its start
    /// committed or played. A change of rules never lands on such a
    /// stage: its prompt told them its rules. A song merely selected (the
    /// picker's resting song, shared and prepared ahead by the others) or
    /// a pick backed out of (its window closed, what was readied or
    /// loaded for it left as it was) has not begun it (follow-up round F,
    /// 2026-11-14: "There's a difference between the song being selected
    /// and it being picked").
    fn begun(&self, index: usize, window: bool) -> bool {
        let Some(s) = self.stages.get(index) else {
            return true;
        };
        window || s.play.is_some()
    }

    /// Whether stage `stage` is the match's last: a counted match's final
    /// stage; an open-ended match's (`MatchRules::open_ended`) picked as
    /// its last song ([`Stage::last`]), or the one at the room's
    /// `max_stages`, the last by itself.
    pub fn is_last(&self, stage: StageId, max_stages: u16) -> bool {
        let Some(i) = self.stage_index(stage) else {
            return true;
        };
        if self.rules.open_ended {
            self.stages[i].last || i + 1 >= usize::from(max_stages)
        } else {
            i + 1 >= self.stages.len()
        }
    }

    /// The match's stage count, `None` for an open-ended match (its
    /// stages so far are `stages.len()`).
    pub fn stage_count(&self) -> Option<u16> {
        (!self.rules.open_ended).then(|| u16::try_from(self.stages.len()).unwrap_or(u16::MAX))
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
        // Sorted by total, highest first: equal totals share the rank of
        // the first of them (one more than the number above).
        let mut last: Option<(Points, u16)> = None;
        for (i, s) in out.iter_mut().enumerate() {
            s.rank = match last {
                Some((total, rank)) if total == s.total => rank,
                _ => u16::try_from(i + 1).unwrap_or(u16::MAX),
            };
            last = Some((s.total, s.rank));
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
    /// Wants to play; arrived during the match and waits for the next
    /// (the match's rules admit nobody during it).
    WaitingNextMatch,
    /// Wants to play; arrived during a match that seats newcomers
    /// (`MatchRules::mid_match_entry`, room model revision 15) and is
    /// seated for its next stage not begun, at the next pick (a stage is
    /// being played now).
    WaitingNextStage,
    /// Wants to play; arrived during a match that seats newcomers, every
    /// place of which is taken (the competitor cap reached, its CPUs
    /// counted: a newcomer never takes a CPU's place): seated once one
    /// frees before a stage, else from the next match.
    WaitingPlace,
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
    /// How stage `stage` of the running match ranks those still in (its
    /// Charts rule, [`crate::rules::Charts::standing`]; as ever where no
    /// match runs).
    pub fn rank_by_for(&self, stage: StageId) -> crate::rules::RankBy {
        self.current
            .as_ref()
            .map(|m| m.rules_for(stage).charts.rank_by())
            .unwrap_or_default()
    }

    /// The stage a change of the running match's per-stage rules saved now
    /// would land on (`crate::Command::ChangeStageRules`): the next stage
    /// not yet picked, else the one after it; never a stage whose
    /// readiness window, countdown or play has begun (its prompt told
    /// them its rules). `None` outside a running match or when no such
    /// stage is left.
    pub fn rules_land_on(&self) -> Option<StageId> {
        let m = self.current.as_ref()?;
        let (stage, window) = match self.phase {
            Phase::Preparing { stage, window } => (stage, window.is_some()),
            Phase::Intermission { next } => (next, false),
            Phase::Countdown { stage, .. }
            | Phase::Stage { stage }
            | Phase::SongCompletion { stage } => (stage, true),
            _ => return None,
        };
        let mut i = m.stage_index(stage)?;
        let mut window = window;
        while m.begun(i, window) {
            i += 1;
            window = false;
            if i >= m.stages.len() {
                return None;
            }
        }
        m.stages.get(i).map(|s| s.id)
    }

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
        let playlist: Vec<Stage> = (1..=u64::from(rules.planned_stages()))
            .map(|i| Stage::new(StageId(i)))
            .collect();
        let mut members = BTreeMap::new();
        members.insert(
            owner_id,
            Member::person(owner_id, owner, owner_name, Role::Owner, false, Intent::Play),
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
            picking: Picking {
                picker: Some(owner_id),
                shares: Vec::new(),
            },
            hosting: Hosting::default(),
            closed: None,
            songs: Default::default(),
            twitch: None,
        }
    }

    /// This new room with its owner an administrator or not (revision
    /// 12): what the authority's session says of who made it, as a join
    /// says it of everyone after (never a bot).
    pub fn with_owner_admin(mut self, admin: bool) -> Self {
        if let Some(m) = self.members.get_mut(&ParticipantId(1)) {
            m.admin = admin && !m.is_bot();
        }
        self
    }

    /// What the room does when its host goes.
    pub fn if_host_leaves(&self) -> HostLeaves {
        self.hosting.if_host_leaves
    }

    /// Who the room passes to when host `from` goes
    /// ([`HostLeaves::PassOn`]; `succession-v1`, the maintainer,
    /// 2026-11-13: "a member can inherit the host role if there isn't a
    /// co-host to inherit it"). Among the people present who may run it
    /// (never a bot, which follows a room, nor an administrator who only
    /// observes, [`RoomState::observes`]):
    ///
    /// 1. a connected co-host, the one who joined first;
    /// 2. else a co-host inside the reconnect grace (`Reconnecting`): the
    ///    room waits for them ([`Succession::Wait`]);
    /// 3. else a connected member, the one who joined first;
    /// 4. else, only with `away_too` (the host left the room, so someone
    ///    must hold it), whoever is coming back, then whoever is away; a
    ///    co-host before a member among equals, then join order.
    pub fn succession(&self, from: ParticipantId, away_too: bool) -> Succession {
        let pool = || {
            self.members.values().filter(move |m| {
                m.id != from && m.present_person() && !m.is_bot() && !self.observes(m)
            })
        };
        let co_host = |m: &&Member| m.role == Role::CoHost;
        let connected = |m: &&Member| m.connection == Connection::Connected;
        let first = |it: &mut dyn Iterator<Item = &Member>| it.map(|m| m.id).min();
        if let Some(id) = first(&mut pool().filter(co_host).filter(connected)) {
            return Succession::To(id);
        }
        let in_grace = |m: &&Member| m.connection == Connection::Reconnecting;
        if let Some(id) = first(&mut pool().filter(co_host).filter(in_grace)) {
            return Succession::Wait(id);
        }
        if let Some(id) = first(&mut pool().filter(connected)) {
            return Succession::To(id);
        }
        if !away_too {
            return Succession::Nobody;
        }
        let rank = |c: Connection| match c {
            Connection::Connected => 0,
            Connection::Joining | Connection::Reconnecting => 1,
            Connection::Disconnected => 2,
        };
        pool()
            .min_by_key(|m| (rank(m.connection), m.role != Role::CoHost, m.id))
            .map_or(Succession::Nobody, |m| Succession::To(m.id))
    }

    /// Who the room goes to when host `from` goes, now or once back
    /// ([`RoomState::succession`]'s `To` or `Wait`). None: nobody.
    pub fn heir(&self, from: ParticipantId, away_too: bool) -> Option<ParticipantId> {
        match self.succession(from, away_too) {
            Succession::To(id) | Succession::Wait(id) => Some(id),
            Succession::Nobody => None,
        }
    }

    /// An administrator who only observes the room: the admin flag, no
    /// role of the room's (the host never made them a co-host) and
    /// watching. What the model knows of how they came: an administrator
    /// who is not a member enters watching from a ticket alone (the
    /// server's `RoomCore::connect`), while one who chose Play competes
    /// and is a player like anyone. An administrator who joined by the
    /// invite to watch looks the same and is skipped too. Never passed
    /// the room.
    pub fn observes(&self, m: &Member) -> bool {
        m.admin && m.role == Role::Member && m.intent == Intent::Watch
    }

    /// Whether `by` may remove `target` from the room now (B9, a host's
    /// Kick, `Capability::Kick`): the host anyone but themselves; a
    /// co-host (or an administrator who plays) ordinary members only,
    /// never the host, another co-host or an administrator; nobody an
    /// administrator who observes, nor a CPU (CPUs leave by the roster).
    /// Taken in every phase: during a stage the kicked one plays, it is a
    /// forfeit, as leaving is.
    pub fn may_kick(&self, by: ParticipantId, target: ParticipantId) -> Result<(), crate::Rejection> {
        use crate::Rejection;
        let unauthorized = Rejection::Unauthorized {
            capability: Capability::Kick,
        };
        let kicker = self.members.get(&by).ok_or(Rejection::NotAMember)?;
        if kicker.left {
            return Err(Rejection::NotPresent);
        }
        if kicker.is_cpu() || !can(kicker.role, kicker.admin, Capability::Kick, self.phase.kind()) {
            return Err(unauthorized);
        }
        let m = self
            .members
            .get(&target)
            .ok_or(Rejection::UnknownParticipant)?;
        if m.is_cpu() {
            return Err(Rejection::NotAHuman);
        }
        if m.left {
            return Err(Rejection::NotPresent);
        }
        if target == by || self.observes(m) {
            return Err(unauthorized);
        }
        if kicker.role == Role::Owner || (m.role == Role::Member && !m.admin) {
            Ok(())
        } else {
            Err(unauthorized)
        }
    }

    /// Whether `by` may remove `target` ([`RoomState::may_kick`]).
    pub fn can_kick(&self, by: ParticipantId, target: ParticipantId) -> bool {
        self.may_kick(by, target).is_ok()
    }

    /// Whether member `member` may make the room's Twitch setting `to`
    /// (none, or one with nothing on: off), as B3 has it: the owner turns
    /// it on, changes it or replaces it with their own (their own lists:
    /// `to.by` is them); the one who set it may turn it, or a part of it,
    /// off, host or not; nobody else changes it. Never a bot nor a CPU
    /// (no Twitch of theirs).
    pub fn may_set_twitch(
        &self,
        member: ParticipantId,
        to: Option<&crate::twitch::TwitchSetting>,
    ) -> Result<(), crate::Rejection> {
        use crate::Rejection;
        let unauthorized = Rejection::Unauthorized {
            capability: Capability::Twitch,
        };
        let m = self.members.get(&member).ok_or(Rejection::NotAMember)?;
        if m.left {
            return Err(Rejection::NotPresent);
        }
        if m.is_cpu() || m.is_bot() {
            return Err(Rejection::NotAHuman);
        }
        let owner = can(m.role, m.admin, Capability::Twitch, self.phase.kind());
        let set_it = self.twitch.as_ref().is_some_and(|t| t.by == member);
        match to.filter(|t| t.on()) {
            None => {
                if self.twitch.is_none() || owner || set_it {
                    Ok(())
                } else {
                    Err(unauthorized)
                }
            }
            Some(t) => {
                if !t.fits() {
                    return Err(Rejection::InvalidTwitch);
                }
                if t.by != member {
                    return Err(unauthorized);
                }
                let lowered = self.twitch.as_ref().is_some_and(|now| now.lowered_to(t));
                if owner || (set_it && lowered) {
                    Ok(())
                } else {
                    Err(unauthorized)
                }
            }
        }
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

    /// Who chooses the room's songs now; none: nobody, until an operator
    /// takes the pick.
    pub fn picker(&self) -> Option<ParticipantId> {
        self.picking.picker
    }

    /// Whether a member's ticket (a socket opened again: a reconnect, a
    /// reload) is given to a client of room model revision `client`
    /// ([`crate::check_model`]): refused where it is too old, except while
    /// a stage is under way (its countdown, its play, its song's end), so
    /// a deploy of the authority never throws anyone out of a stage; the
    /// next reconnect between stages is refused. A join is always checked.
    pub fn admits_model(&self, client: Option<u32>) -> Result<(), crate::UpdateRequired> {
        self.admits_model_against(client, crate::ROOM_MODEL_REVISION)
    }

    /// [`RoomState::admits_model`] for an authority of revision
    /// `authority`.
    pub fn admits_model_against(
        &self,
        client: Option<u32>,
        authority: u32,
    ) -> Result<(), crate::UpdateRequired> {
        use crate::phase::PhaseKind;
        match self.phase.kind() {
            PhaseKind::Countdown | PhaseKind::Stage | PhaseKind::SongCompletion => Ok(()),
            _ => crate::check_model_against(client, authority),
        }
    }

    /// Whether `id` chooses the room's songs now (rests on them for the
    /// others, picks them): the picker.
    pub fn chooses(&self, id: ParticipantId) -> bool {
        self.picking.picker == Some(id)
            && self.members.get(&id).is_some_and(|m| m.present_person())
    }

    /// Whether `id` may take the pick now ([`crate::Command::TakePick`]):
    /// an operator who does not have it.
    pub fn may_take_pick(&self, id: ParticipantId) -> bool {
        self.picker() != Some(id)
            && self
                .members
                .get(&id)
                .filter(|m| m.present_person())
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
            // A window waits while its song is still being shared; while
            // it waits for its players to load (`hosted-deadline-v2`), the
            // loading grace's end.
            Phase::Preparing {
                stage,
                window: Some(w),
            } => (!self.content_pending(*stage)).then_some(match w.loading_until {
                Some(l) => l,
                // A retrying download holds the early end until the
                // grace's end (revision 16): the room looks again then.
                None => match w.hold_until {
                    Some(h) if self.retrying(*stage) => h.min(w.deadline),
                    _ => w.deadline,
                },
            }),
            Phase::Countdown { start_at, .. } => Some(*start_at),
            Phase::Stage { stage } | Phase::SongCompletion { stage } => {
                self.report_deadline(*stage)
            }
            _ => None,
        }
    }

    /// Someone connected who wants to play `stage` is trying its song's
    /// download again ([`Loading::Retrying`], revision 16).
    pub fn retrying(&self, stage: StageId) -> bool {
        let Some(s) = self.stage(stage) else {
            return false;
        };
        self.candidates().iter().any(|&id| {
            self.members
                .get(&id)
                .is_some_and(|m| m.wants_to_play() && m.connection.holds_place())
                && s.prep_ref(id).loading == Loading::Retrying
        })
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
        // A listed song's upload (revision 17), by its content.
        if let Some(e) = self.songs.by_key(&content.song)
            && e.available()
            && e.chart(&content.chart).is_some()
            && e.share.is_some()
        {
            return e.share.clone();
        }
        if let Some(s) = &self.spotlight
            && s.content.chart == content.chart
            && s.share.is_some()
        {
            return s.share.clone();
        }
        if let Some((_, share)) = self
            .picking
            .shares
            .iter()
            .rev()
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
        if !play.reported(&self.members) || !waits {
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
    /// The people present (not left), the count a join is limited on.
    pub fn present_people(&self) -> usize {
        self.members.values().filter(|m| m.present_person()).count()
    }

    pub fn reserved_humans(&self) -> usize {
        self.members
            .values()
            .filter(|m| m.wants_to_play())
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
    /// or a person who is ready, loaded and wants to play, connected or
    /// reconnecting for a moment, `Connection::holds_place`).
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
        if !m.connection.holds_place() {
            return Some(SitOutReason::NotConnected);
        }
        // The loading first (revision 12): one whose song was still
        // loading could not sensibly have pressed Ready.
        let prep = stage.prep_ref(id);
        match prep.loading {
            Loading::Failed { .. } => Some(SitOutReason::LoadFailed),
            Loading::NotStarted | Loading::Loading { .. } | Loading::Retrying => {
                Some(SitOutReason::StillLoading)
            }
            Loading::Loaded if !prep.ready => Some(SitOutReason::NotReady),
            Loading::Loaded => None,
        }
    }

    /// The running match seats newcomers for its next stage not begun
    /// (`MatchRules::mid_match_entry`, room model revision 15): what an
    /// invite's preview says (`next_stage` rather than `next_match`).
    pub fn admits_next_stage(&self) -> bool {
        self.running_match()
            .is_some_and(|m| m.rules.mid_match_entry && !self.match_ends_now())
    }

    /// The running match has no stage left to seat anyone in: the stage
    /// being prepared or played is its last (counted, or an open match's
    /// last song, `Match::is_last`).
    fn match_ends_now(&self) -> bool {
        let Some(m) = self.running_match() else {
            return true;
        };
        let at = match self.phase {
            Phase::Preparing { stage, .. } => {
                // Not begun: newcomers are seated in it.
                if m.stage(stage).is_some_and(|s| s.play.is_none()) {
                    return false;
                }
                stage
            }
            Phase::Intermission { .. } => return false,
            Phase::Countdown { stage, .. }
            | Phase::Stage { stage }
            | Phase::SongCompletion { stage } => stage,
            _ => return true,
        };
        m.is_last(at, self.policy.max_stages)
    }

    /// The places left in the running match for newcomers (room model
    /// revision 15): the room's competitor cap
    /// (`RoomPolicy::max_competitors`) less those seated and still in it
    /// (its CPUs counted, never anyone gone or out for the match); never
    /// the seats its rules began with.
    pub fn late_places(&self) -> usize {
        let Some(m) = &self.current else {
            return 0;
        };
        let seated = m
            .roster
            .iter()
            .filter(|(id, seat)| {
                !seat.out_for_match && self.members.get(id).is_some_and(|p| !p.left)
            })
            .count();
        usize::from(self.policy.max_competitors).saturating_sub(seated)
    }

    /// Who would be seated now in the running match's stage `stage` not
    /// begun, in arrival order (by id): people who want to play and have
    /// no seat in it, as many as there are places
    /// ([`RoomState::late_places`]).
    pub(crate) fn late_entrants(&self) -> Vec<ParticipantId> {
        let Some(m) = &self.current else {
            return Vec::new();
        };
        if !m.rules.mid_match_entry {
            return Vec::new();
        }
        self.members
            .values()
            .filter(|p| p.wants_to_play() && !m.roster.contains_key(&p.id))
            .map(|p| p.id)
            .take(self.late_places())
            .collect()
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
            let prep = s.prep_ref(id);
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
                Intent::Play if !self.admits_next_stage() => Participation::WaitingNextMatch,
                Intent::Play if self.late_places() == 0 => Participation::WaitingPlace,
                Intent::Play => Participation::WaitingNextStage,
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

    /// Whether `id` may add songs to the room's list (revision 17): a
    /// person present in an online room whose role lets them
    /// (`Capability::AddSongs`: the hosts in this stage), never one held
    /// to watching.
    pub fn may_add(&self, id: ParticipantId) -> bool {
        self.source == ContentSource::Shared
            && self.members.get(&id).is_some_and(|m| {
                m.present_person()
                    && !m.watch_only
                    && can(m.role, m.admin, Capability::AddSongs, self.phase.kind())
            })
    }

    /// The most songs `id` may have added to the list, if any: none for a
    /// host, a co-host or an administrator (who has a co-host's powers),
    /// held only by the list's size and what a full list does; anyone
    /// else (once a setting lets everyone add, stage 3) half the list's
    /// size, [`crate::songs::ListRules::per_person`] (round DB, the
    /// maintainer, 2026-11-20: "Hosts and co-hosts are exempt").
    pub fn song_quota(&self, id: ParticipantId) -> Option<usize> {
        let host = self
            .members
            .get(&id)
            .is_some_and(|m| m.admin || matches!(m.role, crate::member::Role::Owner | crate::member::Role::CoHost));
        (!host).then(|| self.songs.rules.per_person())
    }

    /// Whether `id` may remove entry `song` from the list: who added it,
    /// or a host.
    pub fn may_remove(&self, id: ParticipantId, song: &crate::songs::RoomSong) -> bool {
        self.members.get(&id).is_some_and(|m| {
            m.present_person()
                && (song.by == id || can(m.role, m.admin, Capability::AddSongs, self.phase.kind()))
        })
    }

    /// Whether entry `song` is named by a stage not yet over: picked (its
    /// window open or not), counting down or being played. Such a song is
    /// never removed, by anyone or to make room.
    pub fn song_in_use(&self, song: &crate::songs::RoomSong) -> bool {
        self.current
            .iter()
            .flat_map(|m| m.stages.iter())
            .chain(self.playlist.iter())
            .filter(|s| s.play.as_ref().is_none_or(|p| !p.song_ended && !p.interrupted))
            .filter_map(|s| s.content.as_ref())
            .any(|c| c.song == Some(song.id) || (c.song.is_none() && c.id.song == song.key))
    }

    /// The CPU profile behind a member, if it is a CPU seat.
    pub fn opponent(&self, id: ParticipantId) -> Option<&OpponentSpec> {
        match &self.members.get(&id)?.kind {
            MemberKind::Cpu { opponent, .. } => Some(opponent),
            MemberKind::Human { .. } => None,
        }
    }
}
