//! What can be asked of a room (commands), what it reports happened
//! (events) and why it refuses (rejections). These are the wire contract's
//! payloads (`protocol.rs` wraps them); their serde form is the JSON the
//! private server will speak.

use crate::awards::{Finisher, NoAwardRule, StageResult};
use crate::battle::{ReportedRun, RunDetail};
use crate::ids::{ContentId, Identity, MatchId, Millis, ParticipantId, Revision, StageId};
use crate::member::{Capability, Connection, Intent, Member, Role};
use crate::phase::{Interruption, Phase, PhaseKind};
use crate::rules::{Access, Admission, CapExceeded, MatchRules, RulesProblem};
use crate::state::{
    ContentCard, ContentStatus, Loading, MatchEnding, SitOutReason, Spotlight, StageContent,
};
use serde::{Deserialize, Serialize};

/// Who issues a command. Clients act as their member; the authority itself
/// (the local practice driver, the server) reports joins, connections,
/// content status and confirmed stage outcomes, which no client may.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    Member(ParticipantId),
    Authority,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    // One's own participation.
    /// Play or watch. Play reserves a competitive seat (under the cap).
    SetIntent {
        intent: Intent,
    },
    /// Declare (or withdraw) readiness for the upcoming stage, as last seen
    /// at `readiness_rev`. Allowed while loading.
    SetReady {
        stage: StageId,
        readiness_rev: u32,
        ready: bool,
    },
    /// Report loading the stage's content.
    ReportLoading {
        stage: StageId,
        readiness_rev: u32,
        loading: Loading,
    },
    Leave,
    /// One's own run of a stage played on one's own device (an online
    /// room's, whose result comes from its starters' reports). SELF-REPORTED:
    /// the room trusts it.
    ReportOutcome {
        stage: StageId,
        run: ReportedRun,
        /// How they performed, for the stage's standings to show
        /// ([`RunDetail`]: optional and additive to protocol 2).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<RunDetail>,
    },

    // Operators (owner, co-hosts, admins). Edits carry the envelope's base.
    /// Choose (or clear) an unstarted stage's song and chart.
    ChooseContent {
        stage: StageId,
        content: Option<ContentId>,
    },
    /// The maintainer's match flow (M7b): the operator's song select's Start
    /// on a song. It becomes the song of the stage the room plays next and
    /// the match starts (from the lobby, or after a match's results: the
    /// next match) or goes on (between stages); the stage's readiness
    /// window opens at once, its deadline counting from when the song is
    /// available to everyone, and the stage starts as soon as everyone who
    /// could still join is ready and loaded. One change, refused whole. In
    /// the lobby and after a match it needs the envelope's base, as
    /// `StartMatch` does.
    Pick {
        content: ContentId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        card: Option<Box<ContentCard>>,
    },
    /// The song an operator's song select rests on (M7b), for the others
    /// to see and hear before it is picked; `None` clears it. Changes
    /// nothing else (no readiness, no stage).
    Spotlight {
        content: Option<ContentId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        card: Option<Box<ContentCard>>,
    },
    /// Save the next match's rules. Never touches a running match's rules.
    SaveNextRules {
        rules: MatchRules,
    },
    /// Lock the next match's rules and roster and start preparing stage 1.
    /// (Protocol 1 still takes it; a client starts a match with
    /// `StartStage` from the lobby, which cannot leave a match locked that
    /// its first stage refused.)
    StartMatch,
    /// From the intermission to the next stage's preparation.
    PrepareNextStage,
    /// Start the stage: at once when everyone eligible is ready and loaded,
    /// else open the readiness window. In the lobby, for the playlist's
    /// first stage, it starts the match too (its rules and roster locked),
    /// as one change: refused whole, nothing locked, when the stage cannot
    /// start (it needs the envelope's base, as `StartMatch` does).
    StartStage {
        stage: StageId,
        readiness_rev: u32,
    },
    /// Close the readiness window or cancel the committed countdown.
    CancelStart {
        stage: StageId,
    },
    StopMatch,
    /// Close the finished or interrupted match and open the next match's
    /// admission in the lobby.
    OpenNextMatch,

    // The owner alone.
    SetCoHost {
        participant: ParticipantId,
        on: bool,
    },
    SetAccess {
        admission: Admission,
        rotate_invite: bool,
    },

    // The authority alone.
    /// A person entered (or returned to) the room, as the authority
    /// established their identity.
    Join {
        identity: Identity,
        name: String,
        admin: bool,
        intent: Intent,
    },
    SetConnection {
        participant: ParticipantId,
        connection: Connection,
    },
    SetContentStatus {
        stage: StageId,
        status: ContentStatus,
        /// Where the members fetch it, once available (an online room's
        /// share).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        share: Option<String>,
    },
    /// A share of chart `chart` was made for the room (the spotlight's
    /// upload, M7b; shares are told apart by their chart's digest, as the
    /// server's reuse is): the spotlight on that chart takes it, and every
    /// unstarted stage on it waiting for its share becomes available with
    /// it.
    SetSpotlightShare {
        chart: crate::ids::ChartKey,
        share: String,
    },
    /// The stage's result is confirmed before the song ends; the winner may
    /// finish the song.
    ConfirmStage {
        stage: StageId,
        ranking: Vec<Vec<Finisher>>,
    },
    /// The song ended; `ranking` confirms the result unless it already was.
    EndSong {
        stage: StageId,
        ranking: Option<Vec<Vec<Finisher>>>,
    },
    Interrupt {
        reason: Interruption,
    },
}

/// What happened. Applying a revision's events to the previous revision's
/// state gives its state (`RoomState::commit`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    MemberJoined {
        member: Member,
    },
    /// The same record back: same id, results and seat.
    MemberRestored {
        participant: ParticipantId,
    },
    MemberLeft {
        participant: ParticipantId,
    },
    ConnectionChanged {
        participant: ParticipantId,
        connection: Connection,
    },
    IntentChanged {
        participant: ParticipantId,
        intent: Intent,
    },
    RoleChanged {
        participant: ParticipantId,
        role: Role,
    },
    AccessChanged {
        access: Access,
    },
    /// The song an operator's song select rests on, or none (M7b).
    SpotlightChanged {
        spotlight: Option<Spotlight>,
    },
    /// Replaces the stage's content and clears its readiness.
    ContentChosen {
        stage: StageId,
        content: Option<StageContent>,
    },
    ContentStatusChanged {
        stage: StageId,
        status: ContentStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        share: Option<String>,
    },
    /// Clears every stage of the playlist's readiness (the lobby's rules
    /// changed).
    PlaylistReadinessCleared,
    ReadyChanged {
        stage: StageId,
        participant: ParticipantId,
        ready: bool,
    },
    LoadingChanged {
        stage: StageId,
        participant: ParticipantId,
        loading: Loading,
    },
    /// A starter's own report of their run, at authority time `at`.
    OutcomeReported {
        stage: StageId,
        participant: ParticipantId,
        run: ReportedRun,
        at: Millis,
        /// What they reported of how they performed, within the room's
        /// bounds ([`RunDetail::bounded`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<RunDetail>,
    },
    /// The next match's rules; the playlist grows by `added` or keeps its
    /// first stages.
    NextRulesSaved {
        rules: MatchRules,
        added: Vec<StageId>,
    },
    /// The boundary's revalidation of the saved rules.
    NextRulesReviewed {
        review: Option<RulesProblem>,
    },
    MatchStarted {
        id: MatchId,
        rules: MatchRules,
        humans: Vec<ParticipantId>,
        cpus: Vec<Member>,
        /// The following match's playlist (empty stages).
        playlist: Vec<StageId>,
    },
    LateEntrantsAdmitted {
        stage: StageId,
        participants: Vec<ParticipantId>,
    },
    PhaseChanged {
        phase: Phase,
    },
    StartCommitted {
        stage: StageId,
        lineup: Vec<ParticipantId>,
        sat_out: Vec<(ParticipantId, SitOutReason)>,
        start_at: Millis,
    },
    StartCanceled {
        stage: StageId,
        reason: CancelReason,
    },
    StageConfirmed {
        stage: StageId,
        result: StageResult,
    },
    SongEnded {
        stage: StageId,
    },
    OutForMatch {
        participants: Vec<ParticipantId>,
    },
    MatchInterrupted {
        reason: Interruption,
    },
    /// The current match goes to the room's history.
    MatchClosed {
        ending: MatchEnding,
    },
}

/// Why a readiness window or a committed start was canceled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    ByOperator,
    /// The stage's song or chart was replaced.
    ContentReplaced,
    ContentUnavailable,
    /// At the deadline, too few were ready and loaded.
    BelowMinimum,
    /// A committed starter left or lost their connection before the start.
    RosterChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    UnsupportedVersion {
        got: u32,
        oldest: u32,
        newest: u32,
    },
    UnknownClient,
    /// A sequence number below the client's last.
    OutOfOrder {
        last: u64,
    },
    NotAMember,
    /// The member left the room.
    NotPresent,
    Unauthorized {
        capability: Capability,
    },
    AuthorityOnly,
    UnknownParticipant,
    UnknownStage,
    WrongPhase {
        phase: PhaseKind,
    },
    /// A conflict-sensitive edit came without the revision it was based on.
    MissingBase,
    /// What it edits changed after the client's view: refused, never a
    /// silent overwrite.
    Stale {
        base: Revision,
        changed_at: Revision,
    },
    StaleReadiness {
        current: u32,
    },
    /// The stage is not the one gathering readiness.
    NotUpcoming,
    StageStarted,
    /// The start is committed; cancel it first.
    StartCommitted,
    /// A readiness window is already running.
    WindowOpen,
    NothingToCancel,
    /// Watching, or a CPU: no readiness.
    NotPlaying,
    /// Waiting for the next match.
    NotInMatch,
    OutForMatch,
    NoContent,
    ContentPending,
    ContentUnavailable,
    NotEnoughReady {
        ready_humans: u16,
        competitors: u16,
    },
    InvalidRules(RulesProblem),
    /// The saved next-match rules no longer pass; an operator must review.
    RulesNeedReview(RulesProblem),
    NoPlayingHumans,
    Cap(CapExceeded),
    AdmissionClosed,
    RoomFull,
    NotAHuman,
    OwnerRole,
    InvalidRanking(RankingProblem),
    NoAwardRule(NoAwardRule),
    /// The result was confirmed already; the rest of the song adds nothing.
    ResultAlreadyConfirmed,
    ResultMissing,
    /// Not one of the stage's starters.
    NotInLineup,
    /// A different run was reported already.
    AlreadyReported,
    /// A content card's text is too long (`ContentCard::MAX_TEXT`).
    InvalidCard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingProblem {
    Missing(ParticipantId),
    NotInLineup(ParticipantId),
    Duplicate(ParticipantId),
    EmptyGroup,
}
