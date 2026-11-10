//! The multiplayer room, match and stage model (`docs/plans/multiplayer.md`,
//! `docs/plans/multiplayer-ui.md`, `docs/plans/multiplayer-prototype-handoff.md`).
//!
//! One model for the local CPU-practice authority and the online room
//! authority: pure, platform-free and dependency-light (serde and JSON), so
//! the private server runs the same transitions. Content is opaque (a song key
//! and a chart digest); the room never reads charts or judges notes.
//!
//! Separate facts, each with stable ids: room membership and roles
//! (`member`), match identity and its locked rules beside an explicit
//! next-match draft (`rules`), ordered stages and their content, each
//! participant's participation per match and stage, loading and readiness,
//! and connection. Provisional choices sit behind named, versioned types:
//! `awards::AwardRule::ExperimentalLinearV0`,
//! `rules::ReadinessPolicy::HOSTED_DEADLINE_V0`,
//! `battle::BattlePreset::GaugeSurvivalV0`.
//!
//! Commands and events travel in a versioned envelope (`protocol`); the
//! shells talk to a `RoomAuthority`: the deterministic `LocalAuthority`
//! (practice: no network, no sign-in) over the pure reducer
//! (`RoomState::apply`, `advance`, `commit`) the private server runs as
//! well, or the `OnlineAuthority` (`online`), that server's room reached
//! over a socket each shell supplies (`Transport`). `fake::FakeServer` is
//! the server's room in process for tests, with faults in the way.

pub mod authority;
pub mod awards;
pub mod battle;
pub mod command;
pub mod fake;
pub mod ids;
pub mod member;
pub mod online;
pub mod phase;
pub mod points;
pub mod protocol;
mod reduce;
pub mod rules;
pub mod state;

pub use authority::{Clock, LocalAuthority, ManualClock, RoomAuthority};
pub use awards::{AwardRule, AwardScheme, Finish, Finisher, Placement, StageResult};
pub use battle::{BattlePreset, Entrant, ReportedRun, Run};
pub use command::{Actor, CancelReason, Command, Event, RankingProblem, Rejection};
pub use ids::{
    ChartKey, ClientId, ContentId, Identity, MatchId, Millis, ParticipantId, Revision, RoomId,
    SongKey, StageId,
};
pub use member::{Capability, Connection, Intent, Member, MemberKind, OpponentSpec, Role, can};
pub use online::{
    LinkState, OnlineAuthority, ReconnectPolicy, TicketProblem, Transport, TransportEvent,
};
pub use phase::{Interruption, Phase, PhaseKind, ReadinessWindow};
pub use points::Points;
pub use protocol::{
    ClientEnvelope, ClientFrame, Mirror, MirrorError, OLDEST_PROTOCOL_VERSION, PROTOCOL_VERSION,
    Resync, ServerEnvelope, ServerMessage, Want,
};
pub use rules::{
    Access, Admission, CapExceeded, ContentSource, CpuMode, CpuRules, MatchRules, OutcomePolicy,
    ReadinessPolicy, RoomPolicy, RulesProblem,
};
pub use state::{
    ContentStatus, Loading, Match, MatchEnding, MatchRecord, NextMatchDraft, Participation, Prep,
    RoomState, Seat, SitOutReason, Stage, StageContent, StagePlay, StageStanding, Standing,
};
