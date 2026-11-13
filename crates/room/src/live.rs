//! Live play in an online room (`docs/plans/multiplayer-online-play.md`):
//! the contract the game and the private server both compile, beside the
//! room's reducer and never inside it.
//!
//! - **One clock** ([`clock`]): pings on the room's socket and a sans-IO
//!   estimator of the room's clock from a device's own.
//! - **One start**: a stage's song time zero is an instant on the room's
//!   clock (`StagePlay::song_zero_at`, set by the reducer where the room's
//!   policy plays live), so every device's audio is scheduled on it.
//! - **The play stream** ([`stream`]): each competitor's judged moments
//!   and checkpoint reports in numbered batches a few times a second; the
//!   room gathers them and sends ONE combined frame a tick to every socket
//!   that follows the stage, encoded once ([`wire`]: binary).
//! - **Attacks by the room** ([`server`]): the existing ledger
//!   (`crate::attacks::StageBattle`) run on the reports, each checkpoint
//!   decided at a deadline on the room's clock so a late device never
//!   stalls the others; the chart's safe sections come with the pick
//!   ([`plan`]: the room cannot read charts), checked by digest.
//! - **Settlement by the room**: the stage confirmed once it is decided
//!   (`SETTLEMENT_V0`), from the streams' outs and the ledger.
//!
//! [`server::LiveServer`] is the room's side, sans-IO (the server's room
//! and [`crate::fake::FakeServer`] run it); [`client::LiveLink`] the
//! client's, inside [`crate::OnlineAuthority`]. Text frames are JSON in
//! [`ClientLiveEnvelope`] and [`ServerLiveEnvelope`]; a batch and a frame
//! travel as binary WebSocket messages ([`wire`]).
//!
//! PROVISIONAL: every number in [`LIVE_V1`] is a working value, named and
//! versioned.

pub mod client;
pub mod clock;
pub mod cpus;
pub mod delay;
pub mod plan;
pub mod server;
pub mod slew;
pub mod stream;
pub mod wire;

use crate::ids::{Millis, StageId};
use crate::protocol::PROTOCOL_VERSION;
use serde::{Deserialize, Serialize};

pub use client::{LiveLink, LiveNews};
pub use clock::{ClockEstimate, ClockEstimator, Ping, Pong};
pub use delay::{VIEW_DELAY_V0, ViewDelay, ViewDelayRules, ViewMeter};
pub use plan::{BattlePlan, PlanDigest, PlanProblem};
pub use server::{LiveOut, LiveReply, LiveServer, LiveStage};
pub use slew::{SLEW_V0, SlewRules, SongSlew};
pub use stream::{
    Batch, Commitment, Frame, FrameEntry, FrameReader, LiveReport, Moment, MomentKind, Outbox,
    StreamEnd,
};

/// The live protocol's version, said in [`ClientLive::Hello`].
pub const LIVE_VERSION: u32 = 1;

/// How live play runs: its numbers, named and versioned. PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveRules {
    pub id: &'static str,
    /// The room builds one combined frame this often during a stage.
    pub tick_ms: u64,
    /// A competitor sends a batch this often (and once more at its end).
    pub batch_ms: u64,
    /// The views draw a remote moment at most this long after it happened
    /// (song time): what hides batching, both trips and the room's tick.
    /// `live-v1`'s views drew every moment exactly this late; since
    /// `live-v2` a view waits as little as its own measurements allow
    /// ([`delay::ViewDelay`]), this its ceiling.
    pub presentation_delay_ms: u64,
    /// A view marks a field stale once its stream is this far behind the
    /// view's own song time.
    pub stale_ms: u64,
    /// Checkpoint k is decided this long after its instant on the room's
    /// clock, with whatever reports have come; a later report counts at
    /// the next one.
    pub report_deadline_ms: u64,
    /// What the network adds to a target's lead (`attacks::LEAD_V0`): the
    /// deadline, a tick and a trip down, so that a commitment reaches its
    /// target before its warning is due.
    pub network_lead_us: i64,
    /// A device that cannot start at song zero joins its play this late
    /// at most (song time, its missed notes judged as misses); later, it
    /// sits the stage out (a forfeit) and watches.
    pub late_join_ms: u64,
    /// Frames kept for a reconnect's resume (about 30 s).
    pub resync_window: u32,
    /// Frames sent after a live snapshot (about 4 s of recent moments).
    pub snapshot_frames: u32,
    /// A batch not acknowledged (in a frame) this long is sent again.
    pub resend_after_ms: u64,
    /// Pings while a stage is coming or playing, and otherwise.
    pub ping_ms: u64,
    pub idle_ping_ms: u64,
    /// Pings sent on a socket's opening, this far apart.
    pub ping_burst: u32,
    pub ping_burst_gap_ms: u64,
    /// Bounds on what one batch may carry (more is refused).
    pub max_batch_moments: usize,
    pub max_batch_reports: usize,
    pub max_report_judged: usize,
}

/// `live-v1` (PROVISIONAL): a frame every 250 ms, batches every 250 ms,
/// views 1 s behind, checkpoints decided 750 ms after their instant,
/// 1.25 s more lead online, a late join of 5 s at most, 30 s of frames
/// for a reconnect.
pub const LIVE_V1: LiveRules = LiveRules {
    id: "live-v1",
    tick_ms: 250,
    batch_ms: 250,
    presentation_delay_ms: 1_000,
    stale_ms: 2_000,
    report_deadline_ms: 750,
    network_lead_us: 1_250_000,
    late_join_ms: 5_000,
    resync_window: 120,
    snapshot_frames: 16,
    resend_after_ms: 1_000,
    ping_ms: 2_000,
    idle_ping_ms: 10_000,
    ping_burst: 5,
    ping_burst_gap_ms: 100,
    max_batch_moments: 512,
    max_batch_reports: 16,
    max_report_judged: 4_096,
};

/// `live-v2` (PROVISIONAL, 2026-11-13): `live-v1`'s contract with a frame
/// and a batch every 100 ms, so that a view can follow a remote field half
/// a second behind or less (`delay::VIEW_DELAY_V0`). Everything the room
/// decides is as `live-v1`'s: the checkpoint deadline, the network's lead
/// and the late join are unchanged (nothing measured called for changing
/// them). `presentation_delay_ms` is the views' ceiling now: a view waits
/// as little as its measurements allow (`delay::ViewDelay`), at most this.
/// Bought with two and a half times the messages each way
/// (`docs/plans/multiplayer-online-play.md`, "The views' delay").
pub const LIVE_V2: LiveRules = LiveRules {
    id: "live-v2",
    tick_ms: 100,
    batch_ms: 100,
    ..LIVE_V1
};

impl LiveRules {
    /// The rules an id names, if this build knows them.
    pub fn named(id: &str) -> Option<LiveRules> {
        [LIVE_V1, LIVE_V2].into_iter().find(|r| r.id == id)
    }

    /// A target's lead online: practice's ([`crate::attacks::LEAD_V0`],
    /// from its reading horizon) and what the network takes.
    pub fn lead_us(&self, horizon_us: i64) -> i64 {
        crate::attacks::LEAD_V0.of(horizon_us) + self.network_lead_us
    }
}

/// Song microseconds as an instant on the room's clock, for a stage whose
/// song time zero is `song_zero_at` (rounded up to the millisecond).
pub fn room_instant(song_zero_at: Millis, song_us: i64) -> Millis {
    let ms = (song_us.max(0) as u64).div_ceil(1_000);
    let early = (song_us.min(0).unsigned_abs()) / 1_000;
    Millis(song_zero_at.0.saturating_add(ms).saturating_sub(early))
}

/// The song time (µs) at room instant `now` of a stage whose song zero is
/// `song_zero_at`.
pub fn song_us_at(song_zero_at: Millis, now: Millis) -> i64 {
    (now.0 as i64 - song_zero_at.0 as i64) * 1_000
}

/// A client's live message (JSON text: `{"v":2,"live":{"kind":…}}`). Sent
/// only to a room whose policy plays live (`RoomPolicy::live`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClientLive {
    /// First on every socket: this client plays live (binary frames
    /// understood). The room sends frames only to sockets that said it.
    Hello { version: u32 },
    /// A clock exchange (`clock`): `t` the client's own clock, ms.
    Ping(Ping),
    /// Ready for live play of `stage` (sent with its Loaded): its reading
    /// horizon (how long a note shows before it is judged, µs; the room
    /// adds the warning, the delay and the network to make its lead) and
    /// the digest of the battle plan it computed itself, where the stage
    /// has one. Sent again changes the horizon.
    Prepared {
        stage: StageId,
        horizon_us: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan: Option<PlanDigest>,
    },
    /// The battle plan of `stage` (the chart's checkpoints and safe
    /// sections), from the device of whoever picked it, once the pick is
    /// accepted. The room keeps it and says its digest
    /// (`Event::BattlePlanSet`).
    OfferPlan {
        stage: StageId,
        plan: Box<BattlePlan>,
    },
    /// The frames of `stage` after frame `after` (none: from a live
    /// snapshot).
    Resume {
        stage: StageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<u32>,
    },
    /// The owner opens or closes the room's audience (`crate::audience`;
    /// sent only where the room's policy has one). A room's live side
    /// passes it over; its audience side takes it from the owner alone.
    Audience { open: bool },
    /// A CPU's run in `stage` as this device computed it (`cpus`,
    /// `online-cpus-v0`): what it ranks by and how it performed, sent
    /// once its stream ended. The room takes the first device's (the
    /// authority's `Command::ReportFor`); sent only to a room whose
    /// policy seats CPUs online (`RoomPolicy::cpus`).
    CpuRun {
        stage: StageId,
        cpu: crate::ids::ParticipantId,
        run: crate::battle::ReportedRun,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<crate::battle::RunDetail>,
    },
}

/// The room's live message (JSON text: `{"v":2,"live":{"kind":…}}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerLive {
    Pong(Pong),
    /// Something live refused, and why (to the sender alone).
    Refused {
        refusal: LiveRefusal,
    },
    /// The stage as it stands, for a socket that could not resume from
    /// the frames kept; the frames after it follow.
    Snapshot(Box<server::LiveSnapshot>),
    /// How many watch in the room's audience (`crate::audience`), and
    /// whether it is open: when it changed, once a second at most. A
    /// client that predates it passes it over.
    Audience {
        count: u32,
        open: bool,
    },
}

/// Why the room refused a live message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum LiveRefusal {
    /// Not a stage this room is preparing or playing.
    UnknownStage {
        stage: StageId,
    },
    /// The plan's digest is not its contents', or it breaks a bound.
    BadPlan {
        stage: StageId,
        problem: PlanProblem,
    },
    /// A plan only comes from the stage's picker (or an operator).
    NotPicker {
        stage: StageId,
    },
    /// The stage has a different plan already.
    PlanSet {
        stage: StageId,
        digest: PlanDigest,
    },
    /// The device computed a different plan from the same chart: a
    /// content mismatch, as a chart digest's (it sits the stage out).
    PlanMismatch {
        stage: StageId,
        room: PlanDigest,
        yours: Option<PlanDigest>,
    },
    /// A batch the room could not read, or out of bounds; or from someone
    /// not in the stage's lineup.
    BadBatch {
        stage: StageId,
    },
    NotInLineup {
        stage: StageId,
    },
    /// Its stream had ended already.
    StreamEnded {
        stage: StageId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientLiveEnvelope {
    pub v: u32,
    pub live: ClientLive,
}

impl ClientLiveEnvelope {
    pub fn new(live: ClientLive) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            live,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerLiveEnvelope {
    pub v: u32,
    pub live: ServerLive,
}

impl ServerLiveEnvelope {
    pub fn new(live: ServerLive) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            live,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_peers_read_and_are_read() {
        // A start from an authority that predates live play: no song zero.
        let old = r#"{"start_committed":{"stage":1,"lineup":[1,2],"sat_out":[],"start_at":5000}}"#;
        let e: crate::Event = serde_json::from_str(old).unwrap();
        assert!(matches!(
            e,
            crate::Event::StartCommitted {
                song_zero_at: None,
                ..
            }
        ));
        // And without one, nothing new is written.
        assert_eq!(serde_json::to_string(&e).unwrap(), old);
        // A room that does not play live says nothing of it; one that does
        // says so in its policy.
        let policy = crate::RoomPolicy::default();
        assert!(!serde_json::to_string(&policy).unwrap().contains("live"));
        let live = serde_json::to_string(&policy.clone().with_live()).unwrap();
        assert!(live.contains(
            r#""live":{"id":"live-v1","prelude_ms":1000,"timing":"live-v2"}"#
        ));
        let back: crate::RoomPolicy = serde_json::from_str(&live).unwrap();
        assert_eq!(back.live.unwrap().rules(), Some(LIVE_V2));
        // A room made before `live-v2` (no timing) runs `live-v1`'s numbers.
        let old: crate::LivePolicy =
            serde_json::from_str(r#"{"id":"live-v1","prelude_ms":1000}"#).unwrap();
        assert_eq!(old.rules(), Some(LIVE_V1));
        // A timing this build does not know: the contract's own numbers,
        // as a build that predates `live-v2` reads a `live-v2` room.
        let newer: crate::LivePolicy = serde_json::from_str(
            r#"{"id":"live-v1","prelude_ms":1000,"timing":"live-v9"}"#,
        )
        .unwrap();
        assert_eq!(newer.rules(), Some(LIVE_V1));
        // A contract this build does not know: not live.
        let other: crate::LivePolicy =
            serde_json::from_str(r#"{"id":"live-v9","prelude_ms":1000}"#).unwrap();
        assert_eq!(other.rules(), None);
    }

    #[test]
    fn song_time_and_room_instants_agree() {
        let zero = Millis(10_000);
        assert_eq!(room_instant(zero, 0), Millis(10_000));
        assert_eq!(room_instant(zero, 1_500_000), Millis(11_500));
        assert_eq!(room_instant(zero, 1_500_001), Millis(11_501));
        assert_eq!(room_instant(zero, -800_000), Millis(9_200));
        assert_eq!(song_us_at(zero, Millis(12_000)), 2_000_000);
        assert_eq!(song_us_at(zero, Millis(9_000)), -1_000_000);
    }

    #[test]
    fn the_live_messages_read_back_and_never_look_like_commands() {
        let hello = ClientLiveEnvelope::new(ClientLive::Hello { version: 1 });
        let text = serde_json::to_string(&hello).unwrap();
        assert_eq!(text, r#"{"v":2,"live":{"kind":"hello","version":1}}"#);
        // A live frame is a client frame of its own: not a command, not a
        // resync (an authority that predates it refuses to read it, which
        // is why a client sends it only where the policy says live).
        let frame: crate::ClientFrame = serde_json::from_str(&text).unwrap();
        assert!(matches!(frame, crate::ClientFrame::Live(_)));
        let ping = ClientLiveEnvelope::new(ClientLive::Ping(Ping { n: 3, t: 1234 }));
        let back: ClientLiveEnvelope =
            serde_json::from_str(&serde_json::to_string(&ping).unwrap()).unwrap();
        assert_eq!(back, ping);
        // The room's: never read as an envelope of the room's revisions.
        let pong = ServerLiveEnvelope::new(ServerLive::Pong(Pong {
            n: 3,
            t: 1234,
            s: 99,
        }));
        let text = serde_json::to_string(&pong).unwrap();
        assert!(serde_json::from_str::<crate::ServerEnvelope>(&text).is_err());
        assert_eq!(
            serde_json::from_str::<ServerLiveEnvelope>(&text).unwrap(),
            pong
        );
    }
}
