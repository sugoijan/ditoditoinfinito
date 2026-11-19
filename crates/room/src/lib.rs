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
//! `rules::ReadinessPolicy` (`hosted-deadline-v2`),
//! `battle::BattlePreset::GaugeSurvivalV0`; attack power's ledger,
//! `attacks::StageBattle` under `attacks::ATTACKS` (`attacks-v3`) and
//! `attacks::POWER_CLASSES_V0`, settled by `attacks::SETTLEMENT`.
//!
//! Commands and events travel in a versioned envelope (`protocol`); the
//! shells talk to a `RoomAuthority`: the deterministic `LocalAuthority`
//! (practice: no network, no sign-in) over the pure reducer
//! (`RoomState::apply`, `advance`, `commit`) the private server runs as
//! well, or the `OnlineAuthority` (`online`), that server's room reached
//! over a socket each shell supplies (`Transport`). `fake::FakeServer` is
//! the server's room in process for tests, with faults in the way.
//!
//! The audience (`audience`, `docs/plans/multiplayer-audience.md`): people
//! who watch a room beyond its members, counted and not listed, served by
//! relays from frames the room coalesces from its members' (`audience-v0`).
//!
//! Live play in an online room (`live`, `docs/plans/multiplayer-online-play.md`):
//! a shared clock, a stage started at one instant, each competitor's play
//! streamed and gathered into one frame a tick, attacks and settlement
//! decided by the room; the room's side sans-IO (`live::LiveServer`, which
//! the fake runs too) and the client's inside `OnlineAuthority`.

/// A JSON envelope of the protocol's version and one message under
/// `$field` (`{"v":2,"<field>":…}`): its `new`, its `text` and its `read`.
/// Extra derives after the message's type (`Eq`).
macro_rules! envelope {
    ($name:ident, $field:ident, $msg:ty $(, $derive:ident)*) => {
        #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize $(, $derive)*)]
        pub struct $name {
            pub v: u32,
            pub $field: $msg,
        }

        impl $name {
            pub fn new($field: $msg) -> Self {
                Self {
                    v: $crate::protocol::PROTOCOL_VERSION,
                    $field,
                }
            }

            pub fn text(msg: $msg) -> String {
                serde_json::to_string(&Self::new(msg)).unwrap_or_default()
            }

            pub fn read(text: &str) -> Option<$msg> {
                serde_json::from_str::<Self>(text).ok().map(|e| e.$field)
            }
        }
    };
}

pub mod attacks;
pub mod audience;
pub mod authority;
pub mod awards;
pub mod battle;
pub mod client_stats;
pub mod command;
pub mod error_report;
pub mod fake;
pub mod ids;
pub mod live;
pub mod member;
pub mod online;
pub mod phase;
pub mod points;
pub mod protocol;
mod reduce;
pub mod rules;
pub mod songs;
pub mod state;

pub use authority::{Clock, LocalAuthority, ManualClock, RoomAuthority};
pub use awards::{
    AwardRule, AwardScheme, Finish, Finisher, Placement, PointsBias, StageResult, TopBiased,
};
pub use battle::{BattlePreset, Entrant, ReportedRun, Run, RunDetail};
pub use command::{Actor, CancelReason, Command, Event, RankingProblem, Rejection};
pub use ids::{
    ChartKey, ClientId, ContentId, Identity, MatchId, Millis, ParticipantId, Revision, RoomId,
    RoomSongId, SongKey, StageId,
};
pub use songs::{ChartLine, ListRules, NewSong, Removal, RoomSong, RoomSongs, SongCard, WhenFull};
pub use member::{Capability, Connection, Intent, Member, MemberKind, OpponentSpec, Role, can};
pub use online::{
    LinkState, OnlineAuthority, ReconnectPolicy, TicketProblem, Transport, TransportEvent,
};
pub use phase::{Interruption, Phase, PhaseKind, ReadinessWindow};
pub use points::Points;
pub use protocol::{
    ClientEnvelope, ClientFrame, Mirror, MirrorError, OLDEST_PROTOCOL_VERSION, PROTOCOL_VERSION,
    ROOM_MODEL_REVISION, Resync, SERVER_BEHIND, ServerEnvelope, ServerMessage, UPDATE_REQUIRED,
    UpdateRequired, Want, check_authority, check_model, check_model_against,
};
pub use rules::{
    Access, Admission, AtEasiest, AttacksAtOnce, Charts, ShocksPerAttack, CapExceeded, ContentSource, CpuMode, CpuRules, LivePolicy,
    MatchDifficulty, MatchRules, OutcomePolicy, PlayRules, RankBy, ReadinessPolicy, RoomPolicy,
    RulesProblem,
};
pub use state::{
    Closure, ContentCard, ContentStatus, HostDeparture, HostLeaves, Hosting, Loading, Match,
    MatchEnding, MatchRecord, NextMatchDraft, Participation, Picking, Prep, RoomState, RulesChange,
    Seat, SitOutReason, Spotlight, Stage, StageContent, StagePlay, StageStanding, Standing,
    Succession,
};
