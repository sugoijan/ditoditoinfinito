//! What can be asked of a room (commands), what it reports happened
//! (events) and why it refuses (rejections). These are the wire contract's
//! payloads (`protocol.rs` wraps them); their serde form is the JSON the
//! private server will speak.

use crate::attacks::Targeting;
use crate::awards::{Finisher, NoAwardRule, StageResult};
use crate::battle::{ReportedRun, RunDetail};
use crate::ids::{
    ChartKey, ContentId, Identity, MatchId, Millis, ParticipantId, Revision, RoomSongId, StageId,
};
use crate::live::PlanDigest;
use crate::member::{Capability, Connection, Intent, Member, Role};
use crate::phase::{Interruption, Phase, PhaseKind};
use crate::rules::{Access, Admission, CapExceeded, MatchRules, PlayRules, RulesProblem};
use crate::songs::{ChartLine, ListRules, NewSong, Removal, RoomSong};
use crate::state::{
    Closure, ContentCard, ContentStatus, HostLeaves, Loading, MatchEnding, SitOutReason, Spotlight,
    StageContent,
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
    /// One's own attack policy (M8 round 3, D8: how they pick whom to
    /// attack where a match has attacks), what the next stage's battle
    /// starts them with. Taken outside a stage's play (before a match,
    /// between stages, while a stage gathers readiness and through its
    /// countdown); during one the play itself carries a change
    /// (`attacks::Report::targeting`), and the room is told what it was
    /// left at afterwards. Clears nobody's readiness: it is nobody else's
    /// rule, and it may change mid-play anyway. Additive to protocol 2.
    SetTargeting {
        targeting: Targeting,
    },
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

    // The picker (`crate::state::Picking`: who chooses the room's songs
    // now). Edits carry the envelope's base.
    /// The maintainer's match flow (M7b): the operator's song select's Start
    /// on a song. It becomes the song of the stage the room plays next and
    /// the match starts (from the lobby, or after a match's results: the
    /// next match) or goes on (between stages); the stage's readiness
    /// window opens at once, its deadline counting from when the song is
    /// available to everyone, and the stage starts as soon as everyone who
    /// could still join is ready and loaded. One change, refused whole. In
    /// the lobby and after a match it needs the envelope's base (the next
    /// match's rules as the picker saw them).
    Pick {
        content: ContentId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        card: Option<Box<ContentCard>>,
    },
    /// The song the picker's song select rests on (M7b), for the others
    /// to see and hear before it is picked; `None` clears it. Changes
    /// nothing else (no readiness, no stage).
    Spotlight {
        content: Option<ContentId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        card: Option<Box<ContentCard>>,
    },

    /// The picker's pick of a song of the room's list (room model
    /// revision 17, `crate::songs`): chart `chart` of entry `song`, which
    /// must be there to load (`Available`). As [`Command::Pick`] from
    /// there on, the stage's song available at once with the entry's
    /// share (nothing to upload after the pick), its card the entry's.
    PickListed {
        song: RoomSongId,
        chart: ChartKey,
    },

    // The room's song list (revision 17): who may add, `RoomState::may_add`.
    /// Add a song to the list (its adder's device describes it; the
    /// server checks its key and charts against what is uploaded). The
    /// same song again is refused ([`Rejection::AlreadyListed`]); a full
    /// list makes room by its rules ([`crate::songs::ListRules`]).
    AddSong {
        song: Box<NewSong>,
    },
    /// Remove an entry: its adder's, or a host's. Never one a stage not
    /// yet over names ([`Rejection::SongInUse`]). Its upload is deleted.
    RemoveSong {
        song: RoomSongId,
    },
    /// The list's size and what an addition to a full list does: the
    /// owner's room setting (as [`Command::SetHostLeaves`]). A smaller
    /// size removes nothing by itself.
    SetSongList {
        rules: ListRules,
    },

    // Operators (owner, co-hosts, admins). Edits carry the envelope's base.
    /// Take the pick (2026-11-12): the sender chooses the room's songs
    /// from now on, at once, with no request and no confirmation; whoever
    /// had it follows. Taken in any phase: during a stage's readiness,
    /// countdown or play it is the next choice that changes hands (the
    /// stage's song stays).
    TakePick,
    /// Save the next match's rules. Never touches a running match's rules.
    SaveNextRules {
        rules: MatchRules,
    },
    /// Close the readiness window or cancel the committed countdown.
    CancelStart {
        stage: StageId,
    },
    StopMatch,
    /// Save the next match's rules and stop the running match, as one
    /// change (the Rules pop-up's "Apply and restart", 2026-11-13): the
    /// same events as `SaveNextRules` followed by `StopMatch`, so nobody's
    /// command can come between them; the next pick starts a new match
    /// under `rules`. Needs both capabilities (`EditNextRules`,
    /// `StopMatch`: an operator, while a match runs) and the envelope's
    /// base, as `SaveNextRules` does; refused whole otherwise.
    RestartMatch {
        rules: MatchRules,
    },
    /// Change the running match's per-stage rules ([`PlayRules`]: Timing
    /// difficulty, Boos, Attacks, At zero life) from stage `from` on
    /// (2026-11-13, the Rules pop-up's "From stage N"). `from` is the
    /// stage the sender was told the change lands on
    /// (`RoomState::rules_land_on`); where the room has moved on since (a
    /// pick or a start came first) it is refused with
    /// [`Rejection::RulesLandLater`] naming where it would land now,
    /// never moved silently. Needs `EditNextRules` (the operator who may
    /// edit rules), a running match, and the envelope's base against the
    /// match's last change (`Match::changes_saved_at`). The stage count,
    /// seats, CPUs and ruleset never change mid-match (the award curve's
    /// bias does, [`PlayRules::points`]).
    ChangeStageRules {
        from: StageId,
        rules: PlayRules,
    },
    /// Close the finished or interrupted match and open the next match's
    /// admission in the lobby.
    OpenNextMatch,
    /// Make stage `stage` of an open-ended match its last song, or not
    /// (`last`) (room model revision 15, `MatchRules::open_ended`): the
    /// one who chooses the songs at their pick ("Pick as the last song",
    /// the pick then this, as two commands), or an operator
    /// (`Capability::Advance`) for a stage picked and not started. Taken
    /// until the stage's start is committed ([`Rejection::StageStarted`]
    /// after); refused in a counted match ([`Rejection::NotOpenEnded`]).
    SetLastStage {
        stage: StageId,
        last: bool,
    },
    /// End a running open-ended match now, between stages, with the
    /// stages played (room model revision 15): an operator's
    /// (`Capability::Advance`). Its stage not begun is dropped and the
    /// match's results come as when its last song settles: totals, the
    /// winner. Refused during a stage's countdown or play
    /// ([`Rejection::StageStarted`]), before any stage settled
    /// ([`Rejection::NothingPlayed`]) and in a counted match
    /// ([`Rejection::NotOpenEnded`]).
    EndMatch,

    // The owner alone.
    SetCoHost {
        participant: ParticipantId,
        on: bool,
    },
    /// The admission is the owner's; replacing the invite with the
    /// admission as it is (`rotate_invite`) an operator's
    /// (`Capability::ReplaceInvite`, round B1).
    SetAccess {
        admission: Admission,
        rotate_invite: bool,
    },
    /// What the room does when its host goes (2026-11-12,
    /// [`HostLeaves`]): a room setting, taken in every phase.
    SetHostLeaves {
        policy: HostLeaves,
    },
    /// Hand the room over to `participant` (a person present and
    /// connected, never a bot): they are the owner at once, with
    /// everything an owner has, and the sender a co-host, who cannot take
    /// it back by themselves. The pick stays with whoever has it.
    MakeHost {
        participant: ParticipantId,
    },
    /// Whether `participant` is held to watching (`Member::watch_only`).
    /// The authority sets it for one who came by the room's watch-only
    /// link (right after their `Join`: a newcomer, or one who had left
    /// and comes back by it, who then watches whatever they wanted
    /// before, round D1); an operator (the owner, a co-host) may only
    /// lift it (`on` false: "Let play", under `Capability::LetPlay`; the
    /// owner's alone before round B1). Lifting it changes nothing else:
    /// they choose Play themselves. Additive to
    /// protocol 2: a member is held to it only by an authority that has
    /// it, and it is offered only for such a member, so it is never sent
    /// to one that could not read it.
    SetWatchOnly {
        participant: ParticipantId,
        on: bool,
    },

    // The authority alone.
    /// A person entered (or returned to) the room, as the authority
    /// established their identity. A newcomer asking to play where every
    /// player's place is taken joins to watch
    /// ([`RoomState::arrival_intent`](crate::RoomState::arrival_intent));
    /// only the room's member limit refuses one (`Rejection::RoomFull`).
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
    /// What the server made of entry `song`'s upload (revision 17): there
    /// to load with share `share`, its charts as the uploaded record lists
    /// them (`charts`, replacing the adder's description), or not
    /// (`Unavailable`: the record's charts were not the ones added).
    SetSongContent {
        song: RoomSongId,
        status: ContentStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        share: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        charts: Option<Vec<ChartLine>>,
    },
    /// The battle plan the room keeps for `stage` (live play,
    /// `crate::live::plan`: offered by its picker's device, checked by the
    /// authority), named by its digest for every device to compare its own
    /// with. Additive to protocol 2: only an authority that plays live
    /// issues it, and a client that predates it never sees the field.
    SetBattlePlan {
        stage: StageId,
        digest: PlanDigest,
    },
    /// A CPU's run of a stage an online room plays live, as a device that
    /// computed it reported it (`crate::live::cpus`, `online-cpus-v0`): the
    /// authority's, for the first device's report (its live side takes it
    /// from `crate::live::ClientLive::CpuRuns`). Its event is the
    /// `Event::OutcomeReported` a person's own report makes. Additive to
    /// protocol 2: only an authority that seats CPUs online issues it.
    ReportFor {
        stage: StageId,
        participant: ParticipantId,
        run: ReportedRun,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<RunDetail>,
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
    /// End the room for everyone (round D1: the server's operator's
    /// "Close this room", `reason` [`Closure::ByAdministrator`]): its
    /// event is [`Event::RoomClosed`], after which nothing is decided
    /// there and its authority closes every socket (4410), as when a host
    /// leaves a room that is not passed on. Taken in every phase; a stage
    /// under way ends with the room, unranked.
    CloseRoom {
        reason: Closure,
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
    /// A member's administrator flag brought to what the authority's
    /// join says of them (room model revision 12): an administrator who
    /// comes back to a room, or whose session gained or lost the flag
    /// since their row was made. Never a bot's.
    AdminChanged {
        participant: ParticipantId,
        admin: bool,
    },
    /// [`Command::SetWatchOnly`].
    WatchOnlyChanged {
        participant: ParticipantId,
        on: bool,
    },
    /// A member's own attack policy ([`Command::SetTargeting`]).
    TargetingChanged {
        participant: ParticipantId,
        targeting: Targeting,
    },
    AccessChanged {
        access: Access,
    },
    /// What the room does when its host goes ([`Command::SetHostLeaves`];
    /// also the first change this reducer makes in a room from before the
    /// setting, which says so from then on).
    HostLeavesChanged {
        policy: HostLeaves,
    },
    /// The room ended: nothing more is decided there, and its authority
    /// closes its sockets (4410). A change of host is two
    /// [`Event::RoleChanged`]: the one before a co-host, then the new
    /// owner.
    RoomClosed {
        reason: Closure,
    },
    /// The song the picker's song select rests on, or none (M7b).
    SpotlightChanged {
        spotlight: Option<Spotlight>,
    },
    /// A song joined the room's list (revision 17).
    SongAdded {
        song: Box<RoomSong>,
    },
    /// An entry left the list, and why; `share` its upload's, which the
    /// server deletes.
    SongRemoved {
        song: RoomSongId,
        why: Removal,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        share: Option<String>,
    },
    /// [`Command::SetSongContent`].
    SongContentSet {
        song: RoomSongId,
        status: ContentStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        share: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        charts: Option<Vec<ChartLine>>,
    },
    /// [`Command::SetSongList`].
    SongListChanged {
        rules: ListRules,
    },
    /// The pick changed hands ([`Command::TakePick`], or its holder could
    /// hold it no longer); none: nobody chooses until an operator takes
    /// it. The spotlight goes with the picker it was: cleared, the room's
    /// share of it kept known by chart.
    PickerChanged {
        picker: Option<ParticipantId>,
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
    /// The battle plan the room keeps for `stage`
    /// ([`Command::SetBattlePlan`]): its digest, on the stage's content.
    BattlePlanSet {
        stage: StageId,
        digest: PlanDigest,
    },
    /// Clears every stage of the playlist's readiness (the lobby's rules
    /// changed).
    PlaylistReadinessCleared,
    ReadyChanged {
        stage: StageId,
        participant: ParticipantId,
        ready: bool,
    },
    /// Round P1b (the maintainer's of 2026-11-16): a member counted out of
    /// `stage`'s lineup when its start committed (not ready in time, or
    /// reconnecting) readied within the late join of its start
    /// (`crate::live::LiveRules::late_join_ms` after its song's time zero;
    /// the countdown before it counts as before the start): a starter now,
    /// ready, out of the stage's sit-outs. Only where the room plays live.
    EnteredLate {
        stage: StageId,
        participant: ParticipantId,
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
    /// The running match's per-stage rules from stage `from` on
    /// (`Command::ChangeStageRules`): kept on the match
    /// (`Match::changes`), replacing any change from `from` or later.
    StageRulesChanged {
        from: StageId,
        rules: PlayRules,
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
    /// An open-ended match's next stage, made as the one before settled
    /// (room model revision 15).
    StageAdded {
        stage: StageId,
    },
    /// An open-ended match's stage made its last song, or not
    /// ([`Command::SetLastStage`]).
    LastStageChanged {
        stage: StageId,
        last: bool,
    },
    /// An open-ended match's stages not begun dropped: it ends between
    /// stages ([`Command::EndMatch`]), or its last song settled with a
    /// stage made ahead after it (a start canceled, then the stage made
    /// its last). Its results follow (`PhaseChanged` to `MatchResults`).
    StagesDropped,
    PhaseChanged {
        phase: Phase,
    },
    StartCommitted {
        stage: StageId,
        lineup: Vec<ParticipantId>,
        sat_out: Vec<(ParticipantId, SitOutReason)>,
        start_at: Millis,
        /// Where the room plays live (`RoomPolicy::live`): the instant on
        /// its clock of the song's time zero, for every device to schedule
        /// its audio on. Additive: none from an authority that predates it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        song_zero_at: Option<Millis>,
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
    /// A starter took Ready back during the countdown, where the room's
    /// policy lets them (`ReadinessPolicy::unready_cancels_countdown`:
    /// CPU practice's, round P1b).
    ReadyTakenBack,
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
    /// An operator who is not the one choosing the room's songs now
    /// (`picker`: who is; none: nobody, the pick is there to take).
    NotPicker {
        picker: Option<ParticipantId>,
    },
    /// The room ended (`RoomState::closed`).
    RoomClosed,
    /// A change of a running match's per-stage rules made for stage
    /// `asked`, which has begun since (or never existed): it would land on
    /// `from` now (none: no stage left). Additive.
    RulesLandLater {
        asked: StageId,
        from: Option<StageId>,
    },
    /// Came by the watch-only link: Play waits for the owner's Let play
    /// (`Member::watch_only`). Additive: only such a member is refused it.
    WatchOnly,
    /// Round P1b: a Ready after a stage's start committed came too late
    /// (past the late join of its start), or from someone who could not
    /// enter it late (not counted out of it, the stage decided).
    LateEntryClosed,
    /// Round P1b: a Ready to enter a stage late before its song was
    /// reported loaded.
    NotLoaded,
    /// A last song or an early end asked of a counted match (room model
    /// revision 15: only an open-ended match has them).
    NotOpenEnded,
    /// An early end before any stage of the match settled (revision 15).
    NothingPlayed,
    /// Revision 17: the room has no song list (CPU practice picks from
    /// the player's own library).
    NoSongList,
    /// No such entry in the room's list.
    NotListed,
    /// The same song is in the list already, as entry `song`.
    AlreadyListed {
        song: RoomSongId,
    },
    /// The list holds as many songs as it may (and makes no room: it
    /// refuses new songs, or every song in it is in use).
    ListFull,
    /// The person added as many songs as one may (`limit`).
    TooManySongs {
        limit: u16,
    },
    /// A stage not yet over plays it.
    SongInUse,
    /// Not there to load yet (its upload is not done) or not at all.
    SongNotReady,
    /// The chart is not one of the song's.
    ChartNotInSong,
    /// A song described past the list's bounds (`NewSong::fits`), or list
    /// rules out of theirs.
    InvalidSong,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingProblem {
    Missing(ParticipantId),
    NotInLineup(ParticipantId),
    Duplicate(ParticipantId),
    EmptyGroup,
}
