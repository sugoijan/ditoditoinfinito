//! Local CPU practice as the room screen sees it
//! (`docs/plans/multiplayer-prototype-progress.md`, M2): a client of the
//! shared room model (`ddi_room`), holding the practice authority and a
//! mirror kept from its envelopes, as an online client will hold a remote
//! one. No room logic lives here: every change is a command the authority
//! accepts or refuses, and what the screen shows is read from the mirror.
//!
//! What the client adds is what only the device knows: which song and
//! chart of the local library a stage's opaque content names (its
//! [`Picked`] entry), whether the CPUs can play a chart at all, and the
//! clock the authority's deadlines run on.
//!
//! M3 (`docs/plans/multiplayer-prototype-progress.md`): a chosen stage's
//! preparation is real. The room reports it loading and hands the shell a
//! [`LoadJob`]; the shell reads the chart and checks the CPUs' plan
//! ([`check_chart`]) and answers ([`RoomClient::finish_load`]), which
//! reports it loaded or failed with a [`LoadProblem`]. When a stage's
//! countdown is over the room hands the shell its [`StageOrder`] once (what
//! to play, against whom); the shell plays it and reports the judged
//! outcome as the authority ([`RoomClient::report_stage`]).
//!
//! One picker at a time (2026-11-12): the room names who chooses its
//! songs now (`ddi_room::Picking`, its owner at first). Only that player's
//! client rests on songs for the room and picks ([`RoomClient::chooses`]);
//! an operator takes the pick over at once ([`RoomClient::take_pick`],
//! offered where [`RoomClient::may_take_pick`]), and everyone else follows.
//!
//! M5d, online rooms: the picker's pick is the room's commit
//! (`Pick`, pending); this client then asks the room for the
//! stage's upload grant and hands the shell an [`UploadJob`] (its song
//! uploaded as the room's share with the shell's own sharing code), whose
//! progress and end the shell reports ([`RoomClient::upload_progress`],
//! [`RoomClient::upload_finished`]); a failure is said with a retry. Once
//! the room says the stage is available with its share, every member
//! prepares it: the picker who chose it from its library, the others
//! from the share ([`LoadJob::share`], answered by
//! [`RoomClient::finish_shared_load`]), reported loaded only once the
//! chart read has the chosen digest. A stage whose countdown is over is
//! played by each member on its own device ([`StageOrder::others`]); its
//! run goes to the room as the member's own report
//! ([`RoomClient::report_run`]), and the room ranks the reports.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use std::collections::BTreeSet;

use crate::state::settings::TickSettings;
use ddi_library::share::ShareError;
use ddi_room::audience::public::{PublicRules, PublicView};
use ddi_room::attacks::Targeting;
use ddi_room::{
    BattlePreset, Capability, ChartKey, ClientId, Command, ContentCard, ContentId, ContentSource,
    ContentStatus, Finisher, Identity, LinkState, Loading, LocalAuthority, ManualClock, MatchId,
    MatchRules, MemberKind, Mirror, OpponentSpec, ParticipantId, Phase, Placement, Points,
    Rejection, ReportedRun, Revision, Role, RoomAuthority, RoomId, RoomPolicy, RoomState,
    RoomSong, RoomSongId, RulesProblem, Run, RunDetail, ServerMessage, SongKey, Stage, StageId,
    Transport,
};

use crate::state::online::SharedOnline;
use crate::state::online::link::{LinkNotice, OnlineLink};

use crate::screens::data::{ChartEntry, SongEntry};

mod choosing;
mod preload;
mod starts;
pub use choosing::Reveal;
pub(crate) use choosing::within_reveal;
pub use starts::{StartRule, start_line};
pub use preload::{Listing, PRELOAD_V1, PreloadJob, PreloadProblem, PreloadRules, Stop};

/// The engine's CPU levels (`ddi_engine::cpu::CPU_LEVELS`, ten, since
/// `cpu-levels-v1` abilities the same on every chart, with a person's shape
/// since `cpu-levels-v2`), shown as five
/// stars (`crate::stars`): what a level keeps up with, never a skill
/// claim (`docs/plans/cpu-player.md`).
pub fn cpu_levels() -> u8 {
    ddi_engine::cpu::LEVELS as u8
}

/// Practice's starting CPU level, as the engine's index: its default
/// strength (the solo CPU's `normal`, level 8 of ten). PROVISIONAL: a
/// convenient start, not a calibrated profile.
pub const DEFAULT_CPU_LEVEL: u8 = 7;

/// Practice's starting CPUs: three of the catalog (`cpu_roster`), drawn
/// from the room's seed.
pub const STARTING_CPUS: usize = 3;

/// How long the picker's song select rests on a song before the room
/// shows it to the others and an online room shares it (M7b). PROVISIONAL
/// (`spotlight-rest-v0`): long enough that scrolling through the list
/// sends nothing.
pub const SPOTLIGHT_REST_MS: u64 = 3_000;

/// How long an online room left while its socket was down is still
/// driven to tell the room (its Leave sent once the socket has the room
/// again: [`RoomClient::leaving`], `online::kept::Kept::drive_leaving`),
/// milliseconds; past it the link is shut and the room counts the member
/// away until its own patience ends, as before (review 2026-11-22,
/// room-client 4). A socket on a working network has the room again
/// within a second or two (the reconnect tried at once, a ticket and the
/// socket's first snapshot).
pub const LEAVE_WAIT_MS: u64 = 10_000;

/// The stage id the spotlight's upload and download go under (stages are
/// numbered from 1): the server's grant for "no stage" (M7b).
pub const SPOTLIGHT: StageId = StageId(0);

/// Round TB: how long after the server took a Twitch setting asked Host
/// options show it while the room has not said it yet.
const TWITCH_SAID_MS: u64 = 5_000;

/// Round TB: where a Twitch setting asked stands
/// ([`RoomClient::twitch_shown`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TwitchAsking {
    /// Sent, not answered yet.
    Sent,
    /// The access needs a trip through Twitch first.
    Trip,
    /// Answered (taken, or refused): shown until then at most.
    Taken(u64),
}

/// Round FX: where this session's player said no at Twitch to allowing
/// chat (a trip canceled), in the session's storage (the web's trip leaves
/// the page): the room's status line asks no more (More still offers it).
const CHAT_DECLINED: &str = "ddi.mp.chatdeclined";

/// The player said no at Twitch to allowing chat this session.
fn chat_declined() -> bool {
    ddi_platform::storage::get(ddi_platform::storage::Area::Session, CHAT_DECLINED).is_some()
}

/// [`chat_declined`] from now.
fn decline_chat() {
    let _ = ddi_platform::storage::set(ddi_platform::storage::Area::Session, CHAT_DECLINED, "1");
}

/// What the room's screens show now (M7b's flow): song select (the host
/// picking, the others following), or a stage's start prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomView {
    Select,
    Prompt(StageId),
}

/// What leaving a stage's prompt means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptExit {
    /// The stage is being played with this player in it: a forfeit.
    Forfeit,
    /// Before its start: back to the room, not ready (sitting it out if it
    /// starts meanwhile).
    Withdraw,
}

/// A local library song and chart a stage's content names.
#[derive(Clone, Debug, PartialEq)]
pub struct Picked {
    pub song: SongEntry,
    pub chart: ChartEntry,
}

/// Why a chart cannot be chosen for a practice stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// The CPU plays one-player charts with four lanes only
    /// (`ddi_engine::cpu::project`); `style` is the chart's style's id.
    Layout { style: String },
    /// An online room names a chart by its digest, which the library has
    /// not worked out for this one yet.
    NoDigest,
}

/// What leaving the room now would cost, where it costs something
/// ([`RoomClient::leave_cost`]): the cases the screens ask before leaving
/// in, each with what the room will do.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveCost {
    /// An online room's host: the room passes to `to` (their name as the
    /// screens show it), a match running or not.
    HostPass { to: String, match_running: bool },
    /// An online room's host with others in it: the room closes for
    /// everyone (its setting, or only bots to pass it to).
    HostClose { match_running: bool },
    /// An online room's host with nobody else in it: the room closes.
    HostAlone,
    /// In the running match (not the host): it goes on without them.
    Playing,
    /// Choosing the room's songs while a match runs (not the host, not
    /// playing): the host chooses instead.
    Picking,
    /// A co-host while a match runs (neither playing nor choosing).
    CoHost,
}

/// Why something asked of the room did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The room's authority refused the command.
    Room(Rejection),
    /// The chart is not one the CPUs can play.
    Chart(Unsupported),
    /// The stage's song could not be prepared on this device.
    Load(LoadProblem),
    /// The stage's song is still being prepared.
    Preparing,
}

/// Why a stage's song could not be prepared: what its loading reports as
/// failed (`Loading::Failed`'s code, [`LoadProblem::code`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadProblem {
    /// The song or its chart file could not be read.
    Unreadable,
    /// The chart is not in the song any more, or it changed since it was
    /// chosen (its digest differs).
    Changed,
    /// The CPUs cannot plan the chart (`ddi_engine::cpu`: no playable
    /// rows, over its limits).
    CpuCannotPlay,
    /// The room's share of the song could not be downloaded (online).
    Download,
    /// An online room that plays live: this device worked out another
    /// battle plan from the chart than the room has
    /// (`docs/plans/multiplayer-online-play.md`, "The battle plan"): it
    /// sits the stage out, as a chart digest's mismatch does.
    PlanMismatch,
}

impl LoadProblem {
    /// Every problem (a match of `code` below names each).
    pub const ALL: [LoadProblem; 5] = [
        Self::Unreadable,
        Self::Changed,
        Self::CpuCannotPlay,
        Self::Download,
        Self::PlanMismatch,
    ];

    /// Its code in the room's loading report.
    pub fn code(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::Changed => "changed",
            Self::CpuCannotPlay => "cpu-plan",
            Self::Download => "download",
            Self::PlanMismatch => "battle_plan_mismatch",
        }
    }

    pub fn from_code(code: &str) -> Option<LoadProblem> {
        Self::ALL.into_iter().find(|p| p.code() == code)
    }
}

/// A stage's song for the shell to prepare: read chart `chart` of library
/// song `song` and check it ([`prepare_chart`]: the digest chosen, and the
/// CPUs' plan where `cpus`), then answer [`RoomClient::finish_load`]. With
/// `share` (an online room's member who did not choose it), the song is
/// share `share`'s first (`song` is its shared id, `chart` unknown until
/// read: found by its digest), answered by
/// [`RoomClient::finish_shared_load`].
#[derive(Clone, Debug, PartialEq)]
pub struct LoadJob {
    pub stage: StageId,
    pub readiness_rev: u32,
    pub song: String,
    pub chart: usize,
    pub digest: Option<ddi_chart::ChartDigest>,
    pub share: Option<String>,
    /// CPUs play it: their plan is checked too (practice).
    pub cpus: bool,
    /// An online room that plays live: what its preparation works out
    /// besides ([`prepare_stage`]: the reading horizon, the battle plan).
    /// Boxed: the job travels in the screens' calls to the shell.
    pub live: Option<Box<LiveAsk>>,
    /// A member's: the room's store its share is read into and from
    /// ([`RoomClient::store`], stage 2: where its songs downloaded ahead
    /// are); none: the shell's own place for shares (an audience seat's).
    pub store: Option<String>,
}

impl LoadJob {
    /// `picked`'s song and chart from this device's library.
    pub fn library(
        stage: StageId,
        readiness_rev: u32,
        picked: &Picked,
        digest: Option<ddi_chart::ChartDigest>,
        cpus: bool,
        live: Option<LiveAsk>,
    ) -> LoadJob {
        LoadJob {
            stage,
            readiness_rev,
            song: picked.song.id.clone(),
            chart: picked.chart.index,
            digest,
            share: None,
            cpus,
            live: live.map(Box::new),
            store: None,
        }
    }

    /// The song of the room's share `share`, its chart the one `chart`
    /// names by its digest.
    pub fn from_share(
        stage: StageId,
        readiness_rev: u32,
        share: String,
        chart: &ddi_room::ChartKey,
        live: Option<LiveAsk>,
    ) -> LoadJob {
        LoadJob {
            stage,
            readiness_rev,
            song: ddi_library::share::song_id(&share, 0),
            chart: 0,
            digest: chart.as_str().parse().ok(),
            share: Some(share),
            cpus: false,
            live: live.map(Box::new),
            store: None,
        }
    }
}

/// What a stage's preparation works out for an online room that plays it
/// live (`docs/plans/multiplayer-online-play.md`, "The battle plan"): the
/// stage's seed (`ddi_room::live::plan::seed_for`), the match's rules,
/// whether it has attacks, and how this player plays (their options, read
/// from the settings as the job was made: the reading horizon).
#[derive(Clone, Debug, PartialEq)]
pub struct LiveAsk {
    pub seed: u64,
    pub rules: StageRules,
    pub attacks: bool,
    pub options: ddi_engine::player::PlayOptions,
    pub speed_source: ddi_engine::scroll::SpeedSource,
}

/// What a stage's preparation worked out for live play: how far ahead the
/// player reads (µs: the room's lead for them) and, with attacks, the
/// battle plan of the chart.
#[derive(Clone, Debug, PartialEq)]
pub struct LivePrep {
    pub horizon_us: i64,
    pub plan: Option<ddi_room::live::plan::BattlePlan>,
}

/// A stage's song prepared ([`prepare_stage`]): the chart's index, and
/// what live play needs of it where the job asked.
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    pub index: usize,
    pub live: Option<LivePrep>,
}

/// [`prepare_chart`], and where the job asks ([`LoadJob::live`]) what an
/// online room that plays live needs of the chart: the player's reading
/// horizon under their options, and the battle plan where the match has
/// attacks (`crate::state::battle_plan`).
pub fn prepare_stage(
    job: &LoadJob,
    song: &ddi_chart::Song,
    log: &mut dyn FnMut(&str),
) -> Result<Prepared, LoadProblem> {
    let index = prepare_chart(job, song, log)?;
    let live = job.live.as_ref().map(|ask| {
        let mut options = ask.options;
        options.transform = Default::default();
        options.clock.rate = 1.0;
        options.scroll.speed =
            ddi_engine::scroll::speed_for_chart(song, index, options.scroll.speed, ask.speed_source);
        LivePrep {
            horizon_us: crate::state::battle_plan::reading_horizon_us(
                song,
                index,
                &options,
                ask.attacks,
            ),
            plan: ask
                .attacks
                .then(|| crate::state::battle_plan::plan(song, index, &ask.rules, ask.seed)),
        }
    });
    Ok(Prepared { index, live })
}

/// A stage's song to upload as an online room's share (M5d): the library
/// song and chart chosen, with the room's grant
/// (`Authorization: Grant <grant>` on the share routes,
/// `ddi_share_protocol::UploadGrant`); the shell answers
/// [`RoomClient::upload_progress`] as it goes and
/// [`RoomClient::upload_finished`] at its end.
#[derive(Clone, Debug, PartialEq)]
pub struct UploadJob {
    pub stage: StageId,
    pub content: ContentId,
    pub song: SongEntry,
    pub chart: ChartEntry,
    pub grant: String,
    pub max_bytes: f64,
    /// The entry of the room's song list it is for (revision 17: a song
    /// is uploaded when it is added; `stage` is then [`SPOTLIGHT`]'s 0),
    /// answered by [`RoomClient::song_upload_progress`] and
    /// [`RoomClient::song_upload_finished`]; none for a stage's.
    pub entry: Option<RoomSongId>,
}

/// Where this device's sharing of a stage's song is (an online room's
/// operator who chose it).
#[derive(Clone, Debug, PartialEq)]
pub enum Sharing {
    /// Asking the room for the upload's grant.
    Asking,
    /// Uploading: bytes sent of all (all `0` until the files are read).
    Uploading {
        done: f64,
        total: f64,
    },
    /// Sent: the room makes it available (or says why not).
    Sent,
    Failed(SharingProblem),
}

/// Why sharing a stage's song failed.
#[derive(Clone, Debug, PartialEq)]
pub enum SharingProblem {
    /// The room refused the grant (its code: `not_an_operator`,
    /// `stage_started`, …) or could not be reached.
    Grant(String),
    /// The upload failed.
    Upload(ShareError),
}

/// What a shell's own threads tell the room client (desktop's uploads and
/// downloads), drained each tick through [`RoomClient::set_inbox`]: the
/// same answers a single-threaded shell gives by calling the client.
/// (A few a stage: the live preparation's battle plan is the large part,
/// kept inline so the shells build it as before.)
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum ShellNews {
    Progress {
        stage: StageId,
        done: f64,
        total: f64,
    },
    Uploaded {
        stage: StageId,
        result: Result<String, ShareError>,
    },
    /// The same of a song added to the room's list (revision 17).
    SongProgress {
        song: RoomSongId,
        done: f64,
        total: f64,
    },
    SongUploaded {
        song: RoomSongId,
        result: Result<String, ShareError>,
    },
    Prepared {
        job: Box<LoadJob>,
        result: Result<(), LoadProblem>,
    },
    PreparedShared {
        job: Box<LoadJob>,
        result: Result<Box<SharedFound>, LoadProblem>,
    },
    /// Stage 2's downloads ahead ([`PreloadJob`]): a listing read, a
    /// song's download moving on and ending.
    Listed {
        song: RoomSongId,
        result: Result<Box<Listing>, PreloadProblem>,
    },
    AheadProgress {
        song: RoomSongId,
        done: f64,
        total: f64,
    },
    Ahead {
        song: RoomSongId,
        result: Result<(), PreloadProblem>,
    },
}

/// A shell's news since the last call.
pub type Inbox = Box<dyn FnMut() -> Vec<ShellNews>>;

/// A stage's song read from the room's share (on a shell's thread, so
/// plain data): its entry, its chart's index, the pack its record names,
/// its banner as the shell's texture source and what live play needs of
/// the chart where the job asked ([`prepare_stage`]). The room's client
/// makes what it shows and plays of it ([`SharedFound::picked`]).
#[derive(Clone, Debug, PartialEq)]
pub struct SharedFound {
    pub entry: ddi_library::manifest::ManifestEntry,
    pub index: usize,
    pub pack: String,
    pub banner: Option<String>,
    /// Its background picture as the shell's texture source, where the
    /// song has one (the play's backdrop's picture: an audience seat's
    /// page shows the last played stage's behind its card).
    pub background: Option<String>,
    pub live: Option<LivePrep>,
}

impl SharedFound {
    /// The song and chart as the room shows and plays them ([`picked_of`]),
    /// with what live play needs; the chart no longer there: changed.
    pub fn picked(self) -> Result<(Picked, Option<LivePrep>), LoadProblem> {
        picked_of(&self.entry, self.index, &self.pack, self.banner)
            .map(|picked| (picked, self.live))
            .ok_or(LoadProblem::Changed)
    }
}

/// Chart `chart` of `song` (as read) checked for a stage: the one chosen
/// (`digest`) and, with `cpus`, one the CPUs can plan
/// ([`check_chart`]). A shared song's chart (`job.share`) is found by its
/// digest. The chart's index, or why not.
pub fn prepare_chart(
    job: &LoadJob,
    song: &ddi_chart::Song,
    log: &mut dyn FnMut(&str),
) -> Result<usize, LoadProblem> {
    if job.share.is_some() {
        let digest = job.digest.ok_or(LoadProblem::Changed)?;
        return (0..song.charts.len())
            .find(|i| song.chart_digest(*i) == digest)
            .ok_or(LoadProblem::Changed);
    }
    if job.cpus {
        check_chart(song, job.chart, job.digest, log)?;
    } else {
        chart_matches(song, job.chart, job.digest)?;
    }
    Ok(job.chart)
}

/// Whether chart `chart` of `song` is there and is the one chosen
/// (`digest`, where the library had one).
fn chart_matches(
    song: &ddi_chart::Song,
    chart: usize,
    digest: Option<ddi_chart::ChartDigest>,
) -> Result<(), LoadProblem> {
    if chart >= song.charts.len() || digest.is_some_and(|d| song.chart_digest(chart) != d) {
        return Err(LoadProblem::Changed);
    }
    Ok(())
}

/// A shared song's entry as the room shows and plays it: song `entry`
/// (its record's, its id the shared one), its chart `chart` (an index of
/// `entry.charts`), the pack its record names (empty: a bundled song's)
/// and its banner as the shell's texture source.
pub fn picked_of(
    entry: &ddi_library::manifest::ManifestEntry,
    chart: usize,
    pack: &str,
    banner: Option<String>,
) -> Option<Picked> {
    let mut song = crate::state::select_view::plain_song_entry(entry);
    song.pack = (!pack.is_empty()).then(|| pack.to_string());
    song.banner = banner;
    let chart = song.charts.iter().find(|c| c.index == chart)?.clone();
    Some(Picked { song, chart })
}

/// Whether chart `chart` of `song` (as read) is the one chosen (`digest`,
/// where the library had one) and the CPUs can play it as they will (the
/// engine's plan built as a stage's CPUs are, `ddi_engine::cpu::PlannedRung`:
/// the chart itself, `cpu-play-v1`, and a CPU's presses made from it). The
/// engine's own words go to `log` (logs stay English); the room hears the
/// problem.
pub fn check_chart(
    song: &ddi_chart::Song,
    chart: usize,
    digest: Option<ddi_chart::ChartDigest>,
    log: &mut dyn FnMut(&str),
) -> Result<(), LoadProblem> {
    chart_matches(song, chart, digest)?;
    let level = ddi_engine::cpu::CPU_LEVELS
        .strength(DEFAULT_CPU_LEVEL.into())
        .ok_or(LoadProblem::CpuCannotPlay)?;
    ddi_engine::cpu::PlannedRung::new(song, chart)
        .and_then(|home| {
            home.plan.script_play(
                level,
                0,
                ddi_engine::cpu::Skills::default(),
                0.0,
                home.play,
            )
        })
        .map(|_| ())
        .map_err(|e| {
            log(&e);
            LoadProblem::CpuCannotPlay
        })
}

/// A CPU seat in a stage: who, which CPU (from 1), its level (an index
/// into `ddi_engine::cpu::CPU_LEVELS`' ten) and seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpuSeat {
    pub participant: ParticipantId,
    pub ordinal: u16,
    pub level: u8,
    pub seed: u64,
    /// A roster CPU's name (data) and personality (`ddi_engine::cpu::Skills`).
    pub name: Option<String>,
    pub skills: Option<String>,
    /// Its attack policy: its roster entry's (`OpponentSpec::targeting`),
    /// Spread where it names none. Never shown as a skill.
    pub targeting: ddi_room::attacks::Targeting,
}

impl CpuSeat {
    /// CPU `participant` seated in the stage numbered `number` (from 1):
    /// its own seed for each stage (`cpu-stage-seed-v1`), as every
    /// device seeds it.
    #[allow(clippy::too_many_arguments)]
    pub fn for_stage(
        participant: ParticipantId,
        ordinal: u16,
        level: u8,
        base_seed: u64,
        number: usize,
        name: Option<String>,
        skills: Option<String>,
        targeting: Option<ddi_room::attacks::Targeting>,
    ) -> CpuSeat {
        CpuSeat {
            participant,
            ordinal,
            level,
            seed: ddi_room::OpponentSpec::stage_seed(
                base_seed,
                u16::try_from(number).unwrap_or(u16::MAX),
            ),
            name,
            skills,
            targeting: targeting.unwrap_or_default(),
        }
    }
}

/// A stage due for a player who is not on the room's screens
/// ([`RoomClient::stage_due`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageDue {
    pub stage: StageId,
    /// Its number in the match, from 1.
    pub number: u16,
    /// They are not in it: it is theirs to watch.
    pub watching: bool,
}

/// What a session needs to play a stage (`ddi_session::battle`): which,
/// under what battle, who the local player is and the CPUs committed to
/// it. The part of a [`StageOrder`] that is the play's.
#[derive(Clone, Debug, PartialEq)]
pub struct StageSetup {
    pub stage: StageId,
    /// The stage's number, from 1, and the match's stages.
    pub number: u16,
    pub of: u16,
    pub battle: BattlePreset,
    /// The match's rules the stage is judged by, everyone's (D0: DDI, its
    /// difficulty and Boos), played in place of the player's own: the
    /// play's, the CPUs' and the bots' (`ddi_session::battle::stage_ruleset`).
    pub rules: StageRules,
    pub me: ParticipantId,
    /// The local player's attack policy as the stage begins (their
    /// member's, `RoomClient::my_targeting`, chosen before it: the room's
    /// players panel, the stage's prompt; the shells give the play the
    /// one they have as it starts); changed in play by
    /// `ddi_session::battle::Battle::set_targeting`.
    pub targeting: ddi_room::attacks::Targeting,
    pub cpus: Vec<CpuSeat>,
    /// DEVELOPMENT ONLY: a section of battle shocks put into the local
    /// player's play from this beat on ([`Development::mines_at_beat`]:
    /// `--mp-dev-mines`, `mpdevmines=`).
    pub dev_mines_at_beat: Option<u32>,
    /// The chart the local player starts on where it is their own (room
    /// model revision 23: under Adaptive, `ddi_room::RoomState::start_of`,
    /// a chart of the stage's song by its digest); none: the stage's own
    /// chart. The others' come from the room (`LiveRoom::start_of`); a
    /// CPU's from its level (`ddi_engine::ladder::cpu_start`).
    pub start: Option<ddi_room::ChartKey>,
    /// Other people starting it, each on their own device (an online
    /// room's): the stage is decided by the room from everyone's reports,
    /// never on this device ([`RoomClient::report_run`]). Not shown during
    /// the play in this prototype.
    pub others: u16,
    /// The local player is not in the stage: they watch it
    /// ([`RoomClient::watches`]; `watch-v0`). `me` is then nobody's seat
    /// in it: the CPUs alone are its competitors, judged on the song
    /// clock as ever, and nothing of the session's own player is judged.
    pub watching: bool,
    /// An online room that plays the stage live
    /// (`docs/plans/multiplayer-online-play.md`): what this device worked
    /// out for it and who else plays it. Its song starts at the room's
    /// instant ([`RoomClient::song_zero_in`]), the other competitors'
    /// plays are streamed to this one and its attacks are the room's (a
    /// play given its room's client, `ddi_session::battle::Battle::with_live`).
    pub live: Option<LiveOrder>,
    /// The room's chat as the stage is ordered (C1, room model revision
    /// 18, `crate::state::twitch::RoomChat`): the host's ceiling and
    /// channel, from `RoomState::twitch`; none where the room shows no
    /// chat (an audience's seat never has one, C13). A device never shows
    /// more than it (`ddi_session::chat::room_ceiling`).
    pub chat: Option<crate::state::twitch::RoomChat>,
}

impl StageSetup {
    /// The stage's opening event: the match's beginning on its first stage
    /// (a one-stage match's too: one opening, never stacked), its last
    /// stage's (a counted match's last stage, or an open-ended match's
    /// stage marked its last song: `battle final stage` for both, round
    /// AF), else a stage's.
    pub fn opening(&self) -> ddi_library::announcer::Battle {
        use ddi_library::announcer::Battle as Event;
        if self.number <= 1 {
            Event::MatchBegin
        } else if self.is_last() {
            Event::FinalStage
        } else {
            Event::StageBegin
        }
    }

    /// The match's last stage: a counted match's, or an open-ended
    /// match's last song (its `of` is then its number; 0 before it is
    /// marked).
    pub fn is_last(&self) -> bool {
        self.of != 0 && self.number >= self.of
    }

    /// Decided by the room from everyone's reports, never on this device:
    /// other people play it on their own devices, or the room plays it
    /// live (its CPUs too, `ddi_room::live::cpus`).
    pub fn remote(&self) -> bool {
        self.others > 0 || self.live.is_some()
    }
}

/// A stage to play now, handed to the shell once its countdown is over
/// ([`RoomClient::take_stage_order`]): the play's [`StageSetup`] (read
/// through the order: `order.me`), the library song and chart, and what
/// its prompt says of the room.
#[derive(Clone, Debug, PartialEq)]
pub struct StageOrder {
    pub setup: StageSetup,
    pub song: SongEntry,
    pub chart: ChartEntry,
    /// Attacks can be played in the stage's room (CPU practice): its
    /// prompt then says where the match has none.
    pub attacks_offered: bool,
    /// The room offers Adaptive charts ([`RoomClient::adaptive_offered`]):
    /// its prompt says the charts rule only where it does.
    pub adaptive_offered: bool,
}

impl std::ops::Deref for StageOrder {
    type Target = StageSetup;

    fn deref(&self) -> &StageSetup {
        &self.setup
    }
}

impl std::ops::DerefMut for StageOrder {
    fn deref_mut(&mut self) -> &mut StageSetup {
        &mut self.setup
    }
}

/// A stage an online room plays live, as its order carries it.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveOrder {
    /// This device's reading horizon and battle plan (none: it did not
    /// say it plays live, a stage it watches or sits out).
    pub prep: Option<LivePrep>,
    /// The other people of the stage, in the room's order, with their
    /// names as the room has them (data).
    pub people: Vec<(ParticipantId, String)>,
    /// The stage's seed (`ddi_room::live::plan::seed_for`): what the
    /// stage's CPUs' battle plan is made from on a device that has none
    /// of its own (one watching it; `ddi_room::live::cpus`).
    pub seed: Option<u64>,
}

impl StageOrder {
    /// The stage's play may be given a battle's shocks (its match has
    /// attacks, or the development switch puts a section of them in): its
    /// noteskin is told so (`ddi_noteskin::lua::Env::shocks`), so that a
    /// chart's own mines and the shocks never look the same.
    pub fn may_have_shocks(&self) -> bool {
        self.battle.attacks() || self.dev_mines_at_beat.is_some()
    }

    /// The Attacks rule as the stage's summary says it: on or off where
    /// attacks are offered, none where they are not (nothing to say).
    pub fn attacks_rule(&self) -> Option<bool> {
        self.attacks_offered.then(|| self.battle.attacks())
    }

    /// The stage as its start prompt says it.
    pub fn line(&self) -> crate::screens::prompt::StageLine<'_> {
        crate::screens::prompt::StageLine {
            number: self.number,
            stages: self.of,
            cpus: self.cpus.len(),
            others: self.others,
            rules: &self.rules,
            attacks: self.attacks_rule(),
            adaptive_offered: self.adaptive_offered,
        }
    }

    /// A stage handed over to watch with a room's audience
    /// (`crate::state::audience`): its play is fed by the seat's client
    /// and reports nothing.
    pub fn for_audience(&self) -> bool {
        self.me == crate::state::audience::AUDIENCE_ME && self.watching
    }
}

/// The rules a match's stages are judged by, everyone's, the CPUs'
/// included (`docs/plans/ddi-ruleset.md`, "What it means for
/// multiplayer"): the ruleset by the engine's id (DDI,
/// `MatchRules::ruleset_id`) and its options that are match rules.
/// Practice (DDI's option) is never one: a stage is never played with it,
/// whatever the player's own settings say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageRules {
    pub ruleset: String,
    pub difficulty: ddi_room::MatchDifficulty,
    pub boos: bool,
    /// The points' bias for fields of nine and more (summaries name it
    /// where it is not Strong; the play never reads it).
    pub points: ddi_room::PointsBias,
    /// The charts it is played on: the picked one, or Adaptive, down and
    /// up the song's ladder (`ddi_engine::ladder`, `adaptive-v0`).
    pub charts: ddi_room::Charts,
    /// When life runs out, as the stage plays it
    /// (`ddi_room::MatchRules::life_out`: under As picked at zero life,
    /// under Adaptive on the easiest chart; room model revision 16): Play
    /// on (failed, the song played to its end, ranked after every survivor
    /// by score) or Out.
    pub easiest: ddi_room::AtEasiest,
    /// Attacks at once: the most attacks that may stand placed against one
    /// competitor (`attacks-v3`; read only where the stage has attacks).
    pub at_once: ddi_room::AttacksAtOnce,
    /// Shocks per attack: how many shocks an attack of the stage has (a
    /// new room's four; read only where the stage has attacks).
    pub shocks: ddi_room::ShocksPerAttack,
    /// Song choice and Repeats (revision 22): how its song is chosen;
    /// nothing a play is judged by.
    pub choice: ddi_room::SongChoice,
    pub repeats: ddi_room::Repeats,
    /// The stage is played under per-stage rules changed during the match
    /// (`ddi_room::Match::changes`): other than those the match began
    /// with (Points included: the Standings mark it).
    pub changed: bool,
    /// Of those, the rules a play is judged by (all but Points): its
    /// battle plan says so ([`StageRules::changed_tag`]). A change of
    /// Points alone leaves the plan as it was, so a device that predates
    /// it plays the stage alike.
    pub play_changed: bool,
}

/// What of a stage's rules a play is judged by (all but Points): where a
/// stage's differ from its match's, its battle plan says so
/// ([`StageRules::play_changed`]). The one comparison a member's device
/// and an audience's make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlayKey {
    difficulty: ddi_room::MatchDifficulty,
    boos: bool,
    attacks: bool,
    charts: ddi_room::Charts,
    easiest: ddi_room::AtEasiest,
    at_once: ddi_room::AttacksAtOnce,
    shocks: ddi_room::ShocksPerAttack,
}

impl PlayKey {
    fn of(r: &MatchRules) -> PlayKey {
        PlayKey {
            difficulty: r.difficulty,
            boos: r.boos,
            attacks: r.attacks(),
            charts: r.charts,
            easiest: r.life_out(),
            at_once: r.at_once,
            shocks: r.shocks,
        }
    }

    fn of_public(r: &PublicRules) -> PlayKey {
        PlayKey {
            difficulty: r.difficulty,
            boos: r.boos,
            attacks: r.attacks,
            charts: r.charts,
            easiest: r.easiest,
            at_once: r.at_once,
            shocks: r.shocks,
        }
    }
}

/// The rules of stage `stage` in an audience's public view: its own where
/// they were changed (and so), else the view's.
pub fn public_rules_of(v: &PublicView, stage: Option<StageId>) -> (&PublicRules, bool) {
    let changed = stage
        .and_then(|id| v.stages.iter().find(|s| s.id == id))
        .and_then(|s| s.rules.as_ref());
    (changed.unwrap_or(&v.rules), changed.is_some())
}

impl StageRules {
    pub fn of(rules: &MatchRules) -> StageRules {
        StageRules {
            ruleset: rules.ruleset_id().into(),
            difficulty: rules.difficulty,
            boos: rules.boos,
            points: rules.points,
            charts: rules.charts,
            easiest: rules.life_out(),
            at_once: rules.at_once,
            shocks: rules.shocks,
            choice: rules.choice,
            repeats: rules.repeats,
            changed: false,
            play_changed: false,
        }
    }

    /// Stage `stage` of match `m`'s rules: the match's, or a change from
    /// it or an earlier stage on.
    pub fn of_stage(m: &ddi_room::Match, stage: StageId) -> StageRules {
        StageRules {
            changed: m.stage_rules_changed(stage),
            play_changed: m.change_for(stage).is_some()
                && PlayKey::of(&m.rules_for(stage)) != PlayKey::of(&m.rules),
            ..StageRules::of(&m.rules_for(stage))
        }
    }

    /// Stage `stage`'s rules as an audience's public view says them: the
    /// view's, or the stage's own where they were changed (no stage: the
    /// view's).
    pub fn of_public(v: &PublicView, stage: Option<StageId>) -> StageRules {
        let (r, changed) = public_rules_of(v, stage);
        StageRules {
            ruleset: r.ruleset.clone(),
            difficulty: r.difficulty,
            boos: r.boos,
            points: r.points,
            charts: r.charts,
            easiest: r.easiest,
            at_once: r.at_once,
            shocks: r.shocks,
            choice: r.choice,
            repeats: r.repeats,
            changed,
            play_changed: changed && PlayKey::of_public(r) != PlayKey::of_public(&v.rules),
        }
    }

    /// What a stage's battle plan names of rules changed during the
    /// match (`BattlePlan::stage_rules`): its difficulty, Boos and zero
    /// life, so a device that plays it under other rules (one that
    /// predates the change) disagrees on the plan's digest and sits it
    /// out. `None` for a stage under the match's own rules, whose plan is
    /// what it was. Play on when life runs out (revision 15 under
    /// Adaptive, 16 under both) adds its word, only where it applies, so a
    /// tag of before is unchanged.
    pub fn changed_tag(&self) -> Option<String> {
        self.play_changed.then(|| {
            let play_on = if self.plays_on_failed() { "/play-on" } else { "" };
            format!(
                "{:?}/{}/{}{play_on}",
                self.difficulty,
                if self.boos { "boos" } else { "no-boos" },
                self.charts.id()
            )
            .to_lowercase()
        })
    }

    /// Life out where it would end the play is played on as failed (When
    /// life runs out: Play on; revision 16 under both chart rules, under
    /// As picked at zero life, under Adaptive on the easiest chart).
    pub fn plays_on_failed(&self) -> bool {
        self.easiest == ddi_room::AtEasiest::PlayOn
    }

    /// The attack rules a stage under these rules is played with: the
    /// room's (`ddi_room::attacks::ATTACKS`) with its Attacks at once and
    /// its Shocks per attack.
    pub fn attack_rules(&self) -> ddi_room::attacks::AttackRules {
        ddi_room::attacks::ATTACKS
            .with_at_once(self.at_once.limit())
            .with_shocks(self.shocks.count())
    }

    /// Adaptive charts: down and up the song's ladder
    /// (`ddi_engine::ladder`).
    pub fn adaptive(&self) -> bool {
        self.charts == ddi_room::Charts::Adaptive
    }

    /// How the stage ranks those still in (`ddi_room::Charts::rank_by`).
    pub fn rank_by(&self) -> ddi_room::RankBy {
        self.charts.rank_by()
    }

    /// DDI's options for a stage under these rules: the match's
    /// difficulty and Boos, never Practice.
    pub fn ddi_options(&self) -> ddi_engine::rules::ddi::DdiOptions {
        use ddi_engine::rules::ddi::Difficulty;
        ddi_engine::rules::ddi::DdiOptions {
            difficulty: match self.difficulty {
                ddi_room::MatchDifficulty::Easy => Difficulty::Easy,
                ddi_room::MatchDifficulty::Normal => Difficulty::Normal,
                ddi_room::MatchDifficulty::Hard => Difficulty::Hard,
            },
            boos: self.boos,
            practice: false,
        }
    }

    /// The engine's ruleset a stage under these rules is played with:
    /// DDI with the match's options, an empty life out of the stage.
    /// Under When life runs out: Play on (revision 16, both chart rules)
    /// the fail plays on (`FailPolicy::ImmediateContinue`: failed, judged
    /// and scored to the song's end, the gauge taking nothing more): under
    /// Adaptive zero life with a rung below still steps down, so only the
    /// easiest chart's is a fail.
    pub fn build(&self) -> ddi_engine::Ruleset {
        let mut rules = ddi_engine::rules::presets::ddi().with_ddi(&self.ddi_options());
        rules.fail = if self.plays_on_failed() {
            ddi_engine::FailPolicy::ImmediateContinue
        } else {
            ddi_engine::FailPolicy::Immediate
        };
        rules
    }
}

impl Default for StageRules {
    fn default() -> StageRules {
        StageRules::of(&MatchRules::hosted())
    }
}

/// What More offers of an open-ended match's last song
/// ([`RoomClient::last_song_choice`], room model revision 15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LastSongChoice {
    /// The stage picked and not started: whether it is the last song.
    Picked { stage: StageId, last: bool },
    /// Before a pick: whether the next pick will be the last song.
    NextPick { last: bool },
}

/// How the match ended for the local player, once the room has its
/// results (a stopped match has none): the announcer's line for it on
/// the last stage's results (`ddi_session::announcer::battle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEnd {
    /// First alone.
    Won,
    /// First, shared.
    Drawn,
    NotWon,
}

/// The local player's confirmed result of a stage, as the room shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageMark {
    /// The stage's number, from 1.
    pub number: u16,
    pub placement: Placement,
    /// How many started it.
    pub field: u16,
}

/// One starter's line of a stage's standings ([`StageTable`]).
#[derive(Clone, Debug, PartialEq)]
pub struct StageRow {
    pub participant: ParticipantId,
    /// What the room confirmed: the place, how the stage ended for them
    /// and the points it awarded.
    pub placement: Option<Placement>,
    /// Their run as they reported it (an online stage's), confirmed or
    /// not yet.
    pub run: Option<Run>,
    /// How they performed, where it is known (a practice's competitors;
    /// an online starter whose report said so).
    pub detail: Option<RunDetail>,
    /// Once confirmed: their match total and place after this stage, and
    /// their place before it (none on the match's first stage).
    pub total: Option<Points>,
    pub rank: Option<u16>,
    pub rank_before: Option<u16>,
}

/// A stage's standings, as its results show them.
#[derive(Clone, Debug, PartialEq)]
pub struct StageTable {
    pub stage: StageId,
    /// The stage's number, from 1, and the match's stages.
    pub number: u16,
    pub of: u16,
    /// The match is open-ended (revision 15): its title says the stage's
    /// number alone ([`StageTable::of_shown`]).
    pub open_ended: bool,
    /// The room confirmed the outcome: the places, awards and order are
    /// final. Before, the rows are as seated and fill in as reports come.
    pub confirmed: bool,
    /// The room offers Adaptive charts ([`RoomClient::adaptive_offered`]):
    /// the rules' words name the charts rule only where it does.
    pub adaptive_offered: bool,
    /// The match's rules: the ruleset's tiers and score (the engine's
    /// id), its difficulty and Boos.
    pub rules: StageRules,
    /// The battle has attacks: the rows say those launched, those
    /// received and the shocks stepped on.
    pub attacks: bool,
    /// The local player's attacks of the stage, where it was played here
    /// with attacks ([`AttackSummary`]).
    pub summary: Option<AttackSummary>,
    /// Round TW7: why this device's Twitch chat could not show during the
    /// stage, where it could not (one quiet line under the standings).
    pub chat: Option<ddi_chat::live::Trouble>,
    pub rows: Vec<StageRow>,
    /// The match's result, where this was its last stage and the match is
    /// over: everyone in it with their place and total, best first.
    pub result: Option<Vec<(ParticipantId, u16, Points)>>,
}

impl StageTable {
    /// The match's count as the title says it: none (0) for an
    /// open-ended match (the messages' `[0]`).
    pub fn of_shown(&self) -> u16 {
        if self.open_ended { 0 } else { self.of }
    }
}

/// What a stage's standings are built from ([`TableSource::table`]): a
/// member's room state ([`RoomClient::stage_table`]) or the room's public
/// view (an audience seat's,
/// [`crate::state::audience::AudienceClient::stage_table`]). One builder
/// for both, so the audience's table is the members'.
pub(crate) struct TableSource<'a> {
    pub stage: StageId,
    /// The stage's place in its match, from 0, and the match's stages.
    pub index: usize,
    pub of: usize,
    /// The match is open-ended (room model revision 15): its count is no
    /// count, said as none ([`StageTable::open_ended`]).
    pub open_ended: bool,
    /// The match's roster.
    pub roster: Vec<ParticipantId>,
    /// The match's stages' results up to this stage, it included, in
    /// order (none: not confirmed, or not played).
    pub results: Vec<Option<&'a ddi_room::StageResult>>,
    /// The stage's starters, as seated.
    pub lineup: Vec<ParticipantId>,
    /// How a starter performed, where known.
    pub detail: &'a dyn Fn(ParticipantId) -> Option<RunDetail>,
    /// A starter's run as reported, where known.
    pub run: &'a dyn Fn(ParticipantId) -> Option<Run>,
    /// The match is over with every stage played.
    pub completed: bool,
    /// The match's standings, best first (asked for the match's result).
    pub standings: &'a dyn Fn() -> Vec<ddi_room::Standing>,
    pub rules: StageRules,
    pub attacks: bool,
    pub adaptive_offered: bool,
    pub summary: Option<AttackSummary>,
    pub chat: Option<ddi_chat::live::Trouble>,
}

impl TableSource<'_> {
    /// The stage's standings: every starter, what the room confirmed of
    /// them and how they performed, the match's totals after it, and the
    /// match's result where it was its last.
    pub fn table(self) -> StageTable {
        let index = self.index;
        // The match's totals before this stage and after it, in one pass
        // over the stages' placements.
        let mut before: BTreeMap<ParticipantId, Points> =
            self.roster.iter().map(|&id| (id, Points::ZERO)).collect();
        let add = |totals: &mut BTreeMap<ParticipantId, Points>,
                   result: Option<&ddi_room::StageResult>| {
            for p in result.into_iter().flat_map(|r| &r.placements) {
                if let Some(t) = totals.get_mut(&p.participant) {
                    *t = *t + p.points;
                }
            }
        };
        for result in self.results.iter().take(index) {
            add(&mut before, *result);
        }
        let own = self.results.get(index).copied().flatten();
        let mut after = before.clone();
        add(&mut after, own);
        // A rank: one more than the totals above it, from the totals
        // sorted once.
        let sorted = |totals: &BTreeMap<ParticipantId, Points>| {
            let mut v: Vec<Points> = totals.values().copied().collect();
            v.sort_unstable();
            v
        };
        let (before_sorted, after_sorted) = (sorted(&before), sorted(&after));
        let rank_in = |totals: &BTreeMap<ParticipantId, Points>,
                       sorted: &[Points],
                       id: ParticipantId| {
            let mine = totals.get(&id)?;
            let above = sorted.len() - sorted.partition_point(|t| t <= mine);
            Some(u16::try_from(above + 1).unwrap_or(u16::MAX))
        };
        let confirmed = own.is_some();
        let row = |id: ParticipantId| StageRow {
            participant: id,
            placement: own.and_then(|r| r.placement(id)).copied(),
            run: (self.run)(id),
            detail: (self.detail)(id),
            total: confirmed.then(|| after.get(&id).copied()).flatten(),
            rank: confirmed.then(|| rank_in(&after, &after_sorted, id)).flatten(),
            rank_before: (confirmed && index > 0)
                .then(|| rank_in(&before, &before_sorted, id))
                .flatten(),
        };
        // The stage's order once confirmed; before, as they were seated.
        let rows: Vec<StageRow> = match own {
            Some(result) => result
                .placements
                .iter()
                .map(|p| row(p.participant))
                .collect(),
            None => self.lineup.iter().map(|id| row(*id)).collect(),
        };
        let last = index + 1 == self.of;
        let result = (last && confirmed && self.completed).then(|| {
            (self.standings)()
                .into_iter()
                .map(|st| (st.participant, st.rank, st.total))
                .collect()
        });
        StageTable {
            stage: self.stage,
            number: u16::try_from(index + 1).unwrap_or(u16::MAX),
            of: u16::try_from(self.of).unwrap_or(u16::MAX),
            open_ended: self.open_ended,
            confirmed,
            rules: self.rules,
            attacks: self.attacks,
            adaptive_offered: self.adaptive_offered,
            summary: self.summary,
            chat: self.chat,
            rows,
            result,
        }
    }
}

/// The local player's attacks of a stage, for its results' short battle
/// summary (M8 round 3): how many attacks they sent each opponent and
/// were sent by each (in the order first sent), the shocks they stepped
/// on, and whether the stage was drawn by a final attack
/// (`SETTLEMENT_V1`). Kept by a practice's client beside the standings'
/// details ([`RoomClient::set_stage_summary`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttackSummary {
    pub sent: Vec<(ParticipantId, u32)>,
    pub taken: Vec<(ParticipantId, u32)>,
    pub shocks: u32,
    pub drawn_by_final_attack: bool,
}

/// How a play went, for a stage's standings (`ddi_room::RunDetail`):
/// `results` under `ruleset` (the match's), `life` its gauge's at the end,
/// `attacks` those it launched and those sent against it where the
/// battle has them.
pub fn run_detail(
    results: &ddi_engine::player::Results,
    ruleset: &ddi_engine::Ruleset,
    life: f32,
    attacks: Option<(u32, u32)>,
) -> RunDetail {
    let score = &results.score;
    let out = results.gauge_failed_at;
    RunDetail {
        // DDI's score as shown everywhere: the sum of the proportional
        // part and the absolute score (a CPU's its own: it plays the
        // whole chart, `cpu-play-v1`).
        score: match &score.ddi {
            Some(d) => Some(d.score),
            None => score.money.and_then(|m| i64::try_from(m).ok()),
        },
        // As shown: never below 0 % (round AC).
        percent: match &score.ddi {
            Some(d) => i32::try_from(ddi_engine::rules::ddi::shown_hundredths(d.percent)).ok(),
            None => score.percent.map(|p| (p * 10_000.0).round() as i32),
        },
        grade: results.grade.clone(),
        life_ppm: if out.is_some() {
            0
        } else {
            (f64::from(life.clamp(0.0, 1.0)) * ReportedRun::SCALE).round() as u32
        },
        out_at_ms: out.map(|at| (f64::from(at.max(0.0)) * 1_000.0).round() as u64),
        max_combo: results.max_combo,
        tiers: crate::screens::play::tier_counts(ruleset, &results.tally),
        attacks: attacks.map(|a| a.0),
        received: attacks.map(|a| a.1),
        shocks: attacks.map(|_| results.battle_mines.map_or(0, |b| b.hit)),
        chart: None,
        harder: false,
        start: None,
        start_harder: false,
    }
    .bounded()
}

/// Whether the CPUs can play `chart`: a one-player chart with four lanes,
/// which is what `ddi_engine::cpu` accepts. The final check (building the
/// CPU's plan from the loaded chart) is the stage's load, in the next
/// slice; a song-declared layout the library does not know is refused
/// here rather than guessed.
pub fn eligible(chart: &ChartEntry) -> Result<(), Unsupported> {
    if cpu_style(&chart.style) {
        Ok(())
    } else {
        Err(Unsupported::Layout {
            style: chart.style.clone(),
        })
    }
}

/// Whether the CPUs play charts of style `style`: one player, four lanes.
pub fn cpu_style(style: &str) -> bool {
    matches!(
        ddi_chart::Layout::builtin(style),
        Some(l) if l.players == 1 && l.lanes.len() == 4
    )
}

/// What the room shows of `song`'s `chart` before anyone loads it.
pub fn card_of(song: &SongEntry, chart: &ChartEntry) -> ContentCard {
    let cut = |t: &str| t.chars().take(ContentCard::MAX_TEXT).collect::<String>();
    ContentCard {
        title: cut(&song.title),
        artist: cut(&song.artist),
        difficulty: cut(chart.difficulty.name()),
        meter: chart.meter,
        style: cut(&chart.style),
    }
}

/// The opaque content id of a library chart: the song's library id and
/// the chart's digest (its index where the library has no digest yet).
pub fn content_id(song: &SongEntry, chart: &ChartEntry) -> ContentId {
    ContentId {
        song: SongKey::new(song.id.clone()),
        chart: ChartKey::new(match chart.digest {
            Some(d) => d.to_string(),
            None => format!("chart-{}", chart.index),
        }),
    }
}

/// What the client talks to: the practice room's authority on this
/// device, or an online room's link (M5c).
enum Link {
    Local(Box<LocalAuthority<ManualClock>>),
    Online(Box<OnlineLink>),
}

/// Round DE: an online room left (its client dropped) keeps no chat
/// connected for it.
impl Drop for RoomClient {
    fn drop(&mut self) {
        if self.is_online() {
            crate::state::twitch::keep_room_chat(None, self.now_ms());
        }
        self.stop_downloads();
    }
}

/// A room's client: the authority (the practice room's own, or an online
/// room's link), the client's mirror of it and what the device knows
/// about the content it names.
pub struct RoomClient {
    link: Link,
    /// The room's Twitch setting as last seen (revision 18), and whether
    /// this player asked it off themselves: one that ends otherwise is
    /// said on the status line (round TW3, [`RoomClient::tell_twitch_end`]).
    seen_twitch: Option<ddi_room::TwitchSetting>,
    twitch_off_asked: bool,
    /// Round TB: the room's Twitch setting this player asked and the room
    /// has not said yet ([`RoomClient::twitch_shown`]), and where its
    /// asking stands.
    twitch_pending: Option<(ddi_room::TwitchMirror, ddi_room::TwitchChat, TwitchAsking)>,
    /// Round TW7: the room's chat channel this player was told their
    /// Twitch access cannot show ([`RoomClient::tell_chat_access`]).
    chat_access_told: Option<String>,
    /// Round TW7: why this device's Twitch chat could not show during a
    /// stage, for its results' one line ([`RoomClient::set_chat_trouble`]).
    chat_troubles: BTreeMap<StageId, ddi_chat::live::Trouble>,
    /// An online room before its first snapshot: nobody in it yet.
    placeholder: Option<RoomState>,
    clock: ManualClock,
    client: ClientId,
    seq: u64,
    /// The last command sent, when the authority has not answered it yet
    /// (online: the answer comes over the socket later).
    unanswered: Option<u64>,
    /// Ready (or not) asked of the room and not answered yet: the command's
    /// `seq` and the readiness asked ([`Self::ready_as_asked`]).
    ready_asked: Option<(u64, bool)>,
    /// The link's state as the end of the last tick left it: a reconnect
    /// is the link Connected now and not then, however it came (a command
    /// sent between two ticks may be the first to read the new socket's
    /// room; review 2026-11-22, room-client 1).
    link_seen: LinkState,
    /// When this player left the online room ([`Self::leave`]): its link
    /// is driven until the room has read their Leave, [`LEAVE_WAIT_MS`]
    /// at most.
    left_at: Option<u64>,
    mirror: Mirror,
    me: ParticipantId,
    picks: BTreeMap<ContentId, Picked>,
    /// The last refusal, for the screen to say until the next change.
    refusal: Option<Refusal>,
    /// Bumped by every change the screen should redraw for.
    version: u64,
    /// Stages' songs reported loading whose preparation the shell has not
    /// taken yet, and those asked for (by stage and readiness revision).
    jobs: Vec<LoadJob>,
    asked: BTreeSet<(StageId, u32)>,
    /// How far each such song's load was last told the room, in tenths
    /// ([`RoomClient::load_progress`]).
    load_said: BTreeMap<(StageId, u32), u16>,
    /// Stages handed to the shell to play.
    launched: BTreeSet<StageId>,
    /// Stages whose prompt this player left (withdrew from or forfeited):
    /// never handed over again while that window or play lasts
    /// ([`Self::forget_left_prompts`]).
    left_prompts: BTreeSet<StageId>,
    /// Stages this player was told are due on a screen over the room
    /// ([`RoomClient::stage_due`]) and chose to stay for: never told
    /// again while that window or play lasts.
    stayed: BTreeSet<StageId>,
    /// The stage a notice over a screen over the room tells is due now,
    /// while that notice is open ([`RoomClient::telling_due`]).
    telling: Rc<std::cell::Cell<Option<StageId>>>,
    /// The stage whose prompt the last tick had open, if any: a notice is
    /// never carried across its opening or its end (feedback round H,
    /// item 2).
    notice_stage: Option<StageId>,
    /// An online room's operator: the stages' songs this device shares,
    /// by stage, with the content's `content_changed_at` they are for.
    sharing: BTreeMap<StageId, (Revision, Sharing)>,
    /// Uploads the shell has not taken yet.
    uploads: Vec<UploadJob>,
    /// The shell's threads' news ([`ShellNews`]).
    inbox: Option<Inbox>,
    /// The song and chart the picker's song select rests on, since when
    /// (M7b): told to the room once it rested [`SPOTLIGHT_REST_MS`].
    resting: Option<(ContentId, ContentCard, u64)>,
    /// Since when the picker's song select rests on no song (a search
    /// that finds nothing): the room's spotlight cleared after the same
    /// rest.
    resting_on_none: Option<u64>,
    /// This device's upload of the spotlight's song (an online picker's):
    /// one at a time, so that the answers (which name no song) are its.
    spotlight_sharing: Option<(ContentId, Sharing)>,
    /// The spotlight's song as read from the room's share (a member's), and
    /// the content asked for with its share (an answer for another
    /// dropped).
    spotlight_loaded: Option<(ContentId, Picked)>,
    spotlight_asked: Option<(ContentId, String)>,
    /// The Rules pop-up opened by itself on entering (M7b).
    rules_seen: bool,
    /// Practice's: the player's signed-in display name, where they are
    /// signed in (the shell's, [`RoomClient::set_own_name`]); an online
    /// room's is its member's own.
    own_name: Option<String>,
    /// The last results the player was shown: the match and its confirmed
    /// stages (the match's results counted as one more).
    results_seen: (Option<MatchId>, usize),
    /// How each competitor of the stages judged on this device performed
    /// (a practice's: the player and the CPUs, [`RoomClient::set_stage_details`]);
    /// an online stage's are the room's, with its reports.
    details: BTreeMap<StageId, BTreeMap<ParticipantId, RunDetail>>,
    /// The local player's attacks of each stage played here with attacks
    /// ([`RoomClient::set_stage_summary`]).
    summaries: BTreeMap<StageId, AttackSummary>,
    /// The question before leaving is open on a screen of this room
    /// ([`RoomClient::asking_leave`]).
    asking: Rc<std::cell::Cell<bool>>,
    /// The live side's news for the stage played live here, while one is
    /// ([`RoomClient::take_live_news`]).
    live_news: LiveNewsTap,
    /// Stages prepared for live play whose Loaded waits on the room's
    /// battle plan, with the readiness revision prepared for.
    preparing: BTreeMap<StageId, (u32, LivePrep)>,
    /// The battle plans this device offered: when (this device's clock,
    /// ms), and whether the room refused it for a reason that may pass
    /// (offered again `PLAN_OFFERS_V1.again_ms` later, room model
    /// revision 24).
    offered: BTreeMap<StageId, Offered>,
    /// What this device worked out for each stage it said it plays live.
    live_preps: BTreeMap<StageId, LivePrep>,
    /// A practice room whose roster the settings keep as it changes
    /// (`Settings::practice_roster`): the shells' ([`RoomClient::practice`]).
    remembers_roster: bool,
    /// The music's delay behind the room's song time of each live stage
    /// this member watches ([`WATCH_MUSIC_V0`]'s rule; fixed as its order
    /// is handed over).
    watch_music: crate::state::audience::MusicDelays,
    /// The link's end whose notice was shown last
    /// (`take_opened_elsewhere`): once each time it happens.
    elsewhere_told: std::cell::Cell<Option<LinkState>>,
    /// More's End the match now was chosen: its question is still to be
    /// asked over the screen (`take_opened_elsewhere`, round AA).
    asking_end: std::cell::Cell<bool>,
    /// The picker chose to make their next pick the open-ended match's
    /// last song (More, round AA): sent with the pick.
    pick_last: bool,
    /// The never-in end (`LinkState::SignInToEnter`, `NotAMember`) last
    /// asked about, its notice told from the next ask
    /// (`take_opened_elsewhere`).
    never_in_seen: std::cell::Cell<Option<LinkState>>,
    /// Round P1b: each stage's song's downloads tried again so far (by
    /// stage and readiness), the jobs waiting to be tried again (when),
    /// and the job whose retries were spent, kept for Retry.
    load_tries: BTreeMap<(StageId, u32), u32>,
    retrying: Vec<(u64, LoadJob)>,
    failed_jobs: BTreeMap<StageId, LoadJob>,
    /// Round AC: the last command's `seq` when "Use this device" joined
    /// again; refusals of those up to it are of what the stopped page had
    /// asked, and are not said.
    stale_before: Option<u64>,
    /// Revision 17: this player's library by each song's content
    /// ([`song_key`]), as the room's screen gave it
    /// ([`RoomClient::set_library`]): what a listed song is uploaded from
    /// by its adder, and loaded from by anyone who has it.
    library: BTreeMap<String, SongEntry>,
    /// This device's uploads of the songs it added to the room's list.
    song_sharing: BTreeMap<RoomSongId, Sharing>,
    /// Stage 2: the listed songs downloaded ahead (`preload`).
    ahead: preload::Ahead,
    /// The songs of a pack this player added at once (More's Add this
    /// pack), by content, while any of them is still being uploaded.
    pack_keys: Vec<SongKey>,
    /// Revision 22: the group's countdown as first seen here and the
    /// draw's reveal (`choosing`).
    choosing: choosing::Choosing,
    /// Revision 23: this player's own starting charts under Adaptive
    /// (`starts`).
    starts: starts::Starts,
}

/// What an entry of the room's song list is for this player (revision
/// 17): its upload, then whether this device has it.
#[derive(Clone, Debug, PartialEq)]
pub enum EntryState {
    /// Being uploaded (this device's: how far, `done` of `total` bytes;
    /// another's: none known).
    Uploading { done: f64, total: f64 },
    /// This device's upload failed (Retry offered), or the room says the
    /// upload was not the song added.
    Failed,
    /// On this device: in the library, or read from the room's share.
    Ready,
    /// Being loaded here for a stage.
    Loading,
    /// Being downloaded ahead (`preload`): `done` of `total` bytes (`0`
    /// while the whole is not known).
    Downloading { done: f64, total: f64 },
    /// Not on this device yet (downloaded ahead in its turn, or loaded
    /// when picked).
    NotLoaded,
}

/// A song's content as a room's song list names it (revision 17,
/// `ddi_share_protocol::song_key`): its original charts' digests; none
/// where one has no digest yet (the library works it out when it next
/// reads the chart) or it has no original chart.
pub fn song_key(song: &SongEntry) -> Option<String> {
    let digests: Vec<String> = song
        .charts
        .iter()
        .filter(|c| c.version.is_none())
        .map(|c| c.digest.map(|d| d.to_string()))
        .collect::<Option<_>>()?;
    (!digests.is_empty()).then(|| ddi_library::share::song_key(digests.iter().map(String::as_str)))
}

/// What the room's list is told of `song` when it is added: its card and
/// its original charts (with their digests: [`song_key`] first).
pub fn new_song(song: &SongEntry) -> Option<ddi_room::NewSong> {
    let key = song_key(song)?;
    let cut = |t: &str| t.chars().take(ContentCard::MAX_TEXT).collect::<String>();
    let bytes = |t: &str| {
        let mut end = 0;
        for (i, c) in t.char_indices() {
            if i + c.len_utf8() > ddi_room::songs::MAX_CHART_TEXT {
                break;
            }
            end = i + c.len_utf8();
        }
        t[..end].to_string()
    };
    let mut charts: Vec<&ChartEntry> = song.charts.iter().filter(|c| c.version.is_none()).collect();
    charts.sort_by_key(|c| c.index);
    charts.dedup_by_key(|c| c.digest);
    Some(ddi_room::NewSong {
        key: SongKey::new(key),
        card: ddi_room::SongCard {
            title: cut(&song.title),
            artist: cut(&song.artist),
            pack: cut(song.pack.as_deref().unwrap_or_default()),
        },
        charts: charts
            .into_iter()
            .take(ddi_room::songs::MAX_CHARTS)
            .filter_map(|c| {
                Some(ddi_room::ChartLine {
                    chart: ChartKey::new(c.digest?.to_string()),
                    style: bytes(&c.style),
                    difficulty: bytes(c.difficulty.name()),
                    meter: c.meter,
                })
            })
            .collect(),
    })
}

/// A battle plan this device offered ([`RoomClient`]'s `offered`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Offered {
    /// When, on this device's clock (ms).
    at: u64,
    /// The room refused it for a reason that may pass.
    refused: bool,
}

/// PROVISIONAL (`load-retry-v0`, round P1b): a member's failed download
/// of a stage's song from the room's share (the share's list, the song's
/// files, its music) is tried again by itself after these waits, the room
/// still told "loading" meanwhile; once they are spent the failure is
/// reported and the member offered Retry while the stage's window is
/// open. Only a download is worth trying again: a song or chart that
/// cannot be read, a chart that changed, a plan the CPUs cannot make or a
/// battle plan that differs would fail the same way again.
pub const LOAD_RETRY_V0_MS: [u64; 2] = [2_000, 5_000];

/// What [`RoomClient`] keeps as said of a download being tried again
/// (`Loading::Retrying`): above every tenth, so no progress is said over
/// it until its load is over.
const RETRYING_SAID: u16 = u16::MAX - 1;

/// How far behind the room's song time a member who watches a live stage
/// hears its music, so that it matches the others' fields, which are
/// drawn the view's delay behind (about half a second, `live-v2`;
/// the maintainer, 2026-11-13): 0.4 s with nothing measured, moved only at
/// a stage's start (the music is one take) by what the last stage watched
/// needed beyond it (its fields' own delay's 90th percentile past 100 ms,
/// rounded up to 50 ms), at most 0.8 s (which leaves the fields 0.2 s of
/// the live rules' one-second ceiling), down 0.1 s a stage after one that
/// needed less. The audience's rule ([`crate::state::audience::AudienceView`])
/// with a member's numbers. A player out of a stage who watches it on
/// keeps the music on time (it is not moved mid-song), and CPU practice
/// has no delay. PROVISIONAL (`watch-music-v0`).
pub const WATCH_MUSIC_V0: crate::state::audience::AudienceView = crate::state::audience::AudienceView {
    id: "watch-music-v0",
    music_delay_ms: 400,
    most_music_delay_ms: 800,
    rise_above_ms: 100,
    fall_ms: 100,
    lag_quantile: 0.9,
};

/// The room shared by the screens that show it (the room, its song
/// picker) and kept by the shell while the player goes elsewhere.
pub type SharedRoom = Rc<RefCell<RoomClient>>;

/// What a stage played live on this device reads from where it comes
/// from (`ddi_session::battle::live`): a member's room client, or an
/// audience seat's (`crate::state::audience::AudienceClient`,
/// `docs/plans/multiplayer-audience.md`). The room's clock, the stage's
/// song zero, the frames, the stream sent (a member's alone) and the
/// room's decision.
pub trait LiveRoom {
    /// The live rules the room plays by (`LIVE_V1` where it says none).
    fn live_rules(&self) -> ddi_room::live::LiveRules;
    /// The views' delay as a stage's play starts: ceiling, start and
    /// rules. A member's from the live rules and the clock's uncertainty;
    /// an audience's what is left of its ceiling once its music is moved
    /// behind the room's song time (`AudienceClient::music_delay_ms`).
    fn view_delay(&self, rules: &ddi_room::live::LiveRules) -> ddi_room::live::ViewDelay;
    /// The room's time at the shell's wall clock `wall_ms`, as estimated.
    fn room_ms_at(&self, wall_ms: u64) -> Option<ddi_room::Millis>;
    /// How sure the estimate is (µs).
    fn clock_uncertainty_us(&self) -> i64;
    /// Stage `stage`'s song time zero as this device hears and draws it,
    /// on the room's clock.
    fn song_zero_at(&self, stage: StageId) -> Option<ddi_room::Millis>;
    /// The shell's wall clock as of the last tick (ms).
    fn now_ms(&self) -> u64;
    /// Seconds from now until [`LiveRoom::song_zero_at`] (negative once
    /// passed), on the clock of the last tick.
    fn song_zero_in(&self, stage: StageId) -> Option<f64> {
        let zero = self.song_zero_at(stage)?;
        let now = self.room_ms_at(self.now_ms())?;
        Some((zero.0 as f64 - now.0 as f64) / 1_000.0)
    }
    /// Time passes (the shell's wall ms): its socket read and written.
    fn tick(&mut self, now_ms: u64);
    /// The frames, snapshots and refusals since the last call (asking
    /// keeps them coming).
    fn take_live_news(&mut self) -> Vec<ddi_room::live::LiveNews>;
    /// The stage played live here is over: its news is no longer kept.
    fn stop_live_news(&mut self);
    /// What is kept of `stage`'s news is all of it since its start or its
    /// last snapshot ([`LiveNewsTap::whole`]): a play of it that starts
    /// now learns the stage as it stands from it. Else it asks the room
    /// again ([`LiveRoom::read_live_again`]).
    fn live_news_whole(&self, _stage: StageId) -> bool {
        true
    }
    /// Stage `stage` read again from the room: its reader started over,
    /// asking for the stage as it stands (review finding session-1).
    fn read_live_again(&mut self, _stage: StageId) {}
    /// The local play's stream of `stage` (none: nothing is sent, an
    /// audience's).
    fn live_stream(&mut self, stage: StageId) -> Option<&mut ddi_room::live::Outbox>;
    /// The room lost `stage`'s live part.
    fn forget_live(&mut self, stage: StageId);
    /// The room's decision of `stage` once confirmed: its ranking (best
    /// first, groups tied) and whether it came before the song's end.
    fn decision(&self, stage: StageId) -> Option<(Vec<Vec<Finisher>>, bool)>;
    /// What this device worked out for `stage`'s live play.
    fn live_prep(&self, stage: StageId) -> Option<LivePrep>;
    /// Who plays `stage`, once its start is committed (its lineup): a
    /// stage's order, taken as its prompt opened, names every candidate.
    /// None where it is not known (the order's people then stand).
    fn lineup(&self, _stage: StageId) -> Option<Vec<ParticipantId>> {
        None
    }
    /// The chart `p` starts `stage` on where it is their own (room model
    /// revision 23: under Adaptive, `ddi_room::RoomState::start_of`; the
    /// stage's own chart where none): what their field is drawn on from
    /// its first note.
    fn start_of(&self, _stage: StageId, _p: ParticipantId) -> Option<ddi_room::ChartKey> {
        None
    }
    /// Why this device's link to the stage ended for good, where it did
    /// (an audience's seat: the audience closed, the room ended, its link
    /// replaced; a member's, round BM: removed by a host, kept out by the
    /// host's Twitch lists, the room closed): what it shows of the stage
    /// is stale from then on. A member's link that drops reconnects and
    /// never ends so.
    fn seat_end(&self) -> Option<SeatEnd> {
        None
    }
    /// How far behind the stage's song its fields are drawn this frame
    /// beyond what the music already is (ms; an audience's: its next
    /// stage's music delay follows it). A member keeps nothing.
    fn note_view_delay(&mut self, _stage: StageId, _delay_ms: f64) {}
    /// [`LiveRoom::view_delay`] for stage `stage`: a member who watches it
    /// with its music delayed has the fields' delay beyond the music's.
    fn view_delay_for(
        &self,
        _stage: StageId,
        rules: &ddi_room::live::LiveRules,
    ) -> ddi_room::live::ViewDelay {
        self.view_delay(rules)
    }
    /// Round SD: the room's chat as a stage of it shows it now
    /// ([`RoomClient::stage_chat`]: whose channel, under which ceiling;
    /// none where the room shows none), so that a stage being played
    /// follows the host's changes at once. Outer `None` where nothing is
    /// followed (CPU practice, an audience's seat): the stage keeps what
    /// it started with.
    fn room_chat(&self) -> Option<Option<crate::state::twitch::RoomChat>> {
        None
    }
    /// How far ahead of this device's song time (ms) the CPUs it computes
    /// for `stage` are judged: a member watching with its music delayed
    /// judges them on the room's song time all the same, so that what it
    /// reports for them is on time. None for anyone else.
    fn cpu_lead_ms(&self, _stage: StageId) -> u64 {
        0
    }
    /// Stage `stage`'s CPU `cpu` as this device computed it
    /// (`ddi_room::live::cpus`): judged to song µs `through_us`, the
    /// reports it made since and its stream's end, kept and sent where
    /// this device is one that sends (`sends_for_cpus`). An audience's
    /// seat sends nothing.
    fn cpu_news(
        &mut self,
        _stage: StageId,
        _cpu: ParticipantId,
        _through_us: i64,
        _reports: Vec<ddi_room::live::LiveReport>,
        _end: Option<ddi_room::live::StreamEnd>,
    ) {
    }
    /// Its run, once its stream ended.
    fn cpu_run(
        &mut self,
        _stage: StageId,
        _cpu: ParticipantId,
        _run: ddi_room::ReportedRun,
        _detail: Option<RunDetail>,
    ) {
    }
}

/// Why a seat watching with a room's audience ended during a watched
/// stage ([`LiveRoom::seat_end`]): said over the play's place, with the
/// way back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeatEnd {
    /// The host closed the audience (4403).
    AudienceClosed,
    /// The room ended (4410).
    RoomClosed,
    /// The room ended because the server's operator closed it (4410, its
    /// last view saying so: `PublicView::closure`).
    ClosedByOperator,
    /// Its link is no longer the room's (the invite replaced): a new
    /// ticket was refused.
    LinkReplaced,
    /// A member removed by a host (4403, round BM).
    Removed,
    /// A member the host's Twitch lists keep out (4406, round BM).
    NotAdmitted,
    /// Another refusal of a new ticket (its code: data).
    Refused(String),
}

/// A stage's confirmed ranking from its placements (best first, groups
/// tied by place).
/// A stage's decision once its placements are confirmed: its ranking
/// ([`ranking_of`]) and whether it came before the song's end (the room
/// in `phase`).
pub fn decision_of(
    phase: &Phase,
    stage: StageId,
    placements: &[Placement],
) -> (Vec<Vec<Finisher>>, bool) {
    let early = matches!(phase, Phase::SongCompletion { stage: x } if *x == stage);
    (ranking_of(placements), early)
}

/// What a live stage played here reads of the room's live side: all of
/// it while one is read ([`LiveNewsTap::take`]); while none is, the
/// latest stage's news since its last snapshot, so that a play that
/// starts part-way (an audience's Watch, a member handed a running stage)
/// learns the stage as it stands (review finding session-1) and nothing
/// piles up beyond one stage. Where a play read part of a stage and
/// stopped, what is kept of it is not the whole: the next play of it asks
/// the room again (`LiveRoom::read_live_again`).
#[derive(Debug, Default)]
pub struct LiveNewsTap {
    news: Vec<ddi_room::live::LiveNews>,
    reading: bool,
    /// The latest stage its news were of.
    stage: Option<StageId>,
    /// A stage a play read and stopped reading: what it read is gone, so
    /// what is kept of it is not the whole ([`LiveNewsTap::whole`]).
    partial: Option<StageId>,
}

impl LiveNewsTap {
    /// News come: kept for the stage read, or for one to come.
    pub fn extend(&mut self, news: impl IntoIterator<Item = ddi_room::live::LiveNews>) {
        use ddi_room::live::LiveNews;
        let stage_of = |n: &LiveNews| match n {
            LiveNews::Frame(f) => Some(f.stage),
            LiveNews::Snapshot(s) => Some(s.stage),
            LiveNews::Refused(_) => None,
        };
        for n in news {
            if let Some(stage) = stage_of(&n) {
                self.stage = Some(self.stage.map_or(stage, |s| s.max(stage)));
            }
            if !self.reading {
                match &n {
                    // The stage as it stands: what came of it before is
                    // in it.
                    LiveNews::Snapshot(s) => {
                        let stage = s.stage;
                        self.news.retain(|x| stage_of(x).is_some_and(|y| y > stage));
                        if self.partial == Some(stage) {
                            self.partial = None;
                        }
                    }
                    LiveNews::Frame(f) => {
                        let stage = f.stage;
                        self.news.retain(|x| stage_of(x).is_none_or(|y| y >= stage));
                    }
                    LiveNews::Refused(_) => {}
                }
            }
            self.news.push(n);
        }
    }

    /// What it keeps of `stage` is all of it since its start or its last
    /// snapshot: no play read part of it and stopped since.
    pub fn whole(&self, stage: StageId) -> bool {
        self.partial != Some(stage)
    }

    /// Stage `stage` is read again from the room (its link's reader
    /// started over): nothing kept of it any more.
    pub fn read_again(&mut self, stage: StageId) {
        self.news.clear();
        self.stage = Some(stage);
        self.partial = None;
    }

    /// The news since the last call; asking keeps them coming.
    pub fn take(&mut self) -> Vec<ddi_room::live::LiveNews> {
        self.reading = true;
        std::mem::take(&mut self.news)
    }

    /// The stage read is over: what it left is dropped.
    pub fn stop(&mut self) {
        if self.reading {
            self.partial = self.stage;
        }
        self.reading = false;
        self.news.clear();
    }
}

pub fn ranking_of(placements: &[Placement]) -> Vec<Vec<Finisher>> {
    let mut groups: Vec<Vec<Finisher>> = Vec::new();
    let mut last: Option<u16> = None;
    for p in placements {
        let f = Finisher {
            participant: p.participant,
            finish: p.finish,
        };
        if last == Some(p.place) {
            if let Some(g) = groups.last_mut() {
                g.push(f);
            }
        } else {
            groups.push(vec![f]);
        }
        last = Some(p.place);
    }
    groups
}

impl LiveRoom for RoomClient {
    fn room_chat(&self) -> Option<Option<crate::state::twitch::RoomChat>> {
        self.is_online().then(|| self.stage_chat())
    }

    /// Round BM: a member's link ends for good when a host removed them,
    /// the host's Twitch lists keep them out, or the room closed: a play
    /// of theirs still running is left as a quit is, and a stage they
    /// watch stops, saying why.
    fn seat_end(&self) -> Option<SeatEnd> {
        match self.link_state() {
            LinkState::Kicked => Some(SeatEnd::Removed),
            LinkState::NotAdmitted => Some(SeatEnd::NotAdmitted),
            LinkState::Closed if self.state().closed == Some(ddi_room::Closure::ByAdministrator) => {
                Some(SeatEnd::ClosedByOperator)
            }
            LinkState::Closed => Some(SeatEnd::RoomClosed),
            _ => None,
        }
    }

    fn lineup(&self, stage: StageId) -> Option<Vec<ParticipantId>> {
        let play = self.state().stage(stage)?.play.as_ref()?;
        Some(play.lineup.iter().copied().collect())
    }

    fn start_of(&self, stage: StageId, p: ParticipantId) -> Option<ddi_room::ChartKey> {
        self.state().start_of(stage, p).cloned()
    }

    fn live_rules(&self) -> ddi_room::live::LiveRules {
        self.state()
            .policy
            .live
            .as_ref()
            .and_then(|l| l.rules())
            .unwrap_or(ddi_room::live::LIVE_V1)
    }

    fn view_delay(&self, rules: &ddi_room::live::LiveRules) -> ddi_room::live::ViewDelay {
        use ddi_room::live::{VIEW_DELAY_V0, ViewDelay};
        let uncertainty_ms = self
            .live_link()
            .and_then(|l| l.clock())
            .map_or(100.0, |c| c.uncertainty_us as f64 / 1_000.0);
        let ceiling = rules.presentation_delay_ms as f64;
        ViewDelay::new(
            VIEW_DELAY_V0,
            ceiling,
            VIEW_DELAY_V0.initial_ms(rules, uncertainty_ms, ceiling),
        )
    }

    fn room_ms_at(&self, wall_ms: u64) -> Option<ddi_room::Millis> {
        self.live_link().and_then(|l| l.room_ms(wall_ms))
    }

    fn clock_uncertainty_us(&self) -> i64 {
        self.live_link()
            .and_then(|l| l.clock())
            .map_or(0, |c| c.uncertainty_us)
    }

    /// The room's song zero, a member who watches the stage hearing its
    /// music [`RoomClient::watch_music_ms`] later.
    fn song_zero_at(&self, stage: StageId) -> Option<ddi_room::Millis> {
        let zero = RoomClient::song_zero_at(self, stage)?;
        Some(ddi_room::Millis(zero.0 + self.watch_music_ms(stage)))
    }

    fn now_ms(&self) -> u64 {
        RoomClient::now_ms(self)
    }

    /// Watching with the music delayed: what is left of the ceiling once
    /// the music is moved behind, from nothing, as the frames measure (as
    /// the audience's).
    fn view_delay_for(
        &self,
        stage: StageId,
        rules: &ddi_room::live::LiveRules,
    ) -> ddi_room::live::ViewDelay {
        let music = self.watch_music_ms(stage);
        if music == 0 {
            return self.view_delay(rules);
        }
        let left = (rules.presentation_delay_ms.saturating_sub(music)) as f64;
        crate::state::audience::behind_music("watch-view-delay-v0", left)
    }

    fn cpu_lead_ms(&self, stage: StageId) -> u64 {
        self.watch_music_ms(stage)
    }

    /// The fields' own delay beyond the music this frame, where the music
    /// is delayed: kept for the next watched stage's.
    fn note_view_delay(&mut self, stage: StageId, delay_ms: f64) {
        if self.watch_music_ms(stage) == 0 {
            return;
        }
        self.watch_music.note(stage, delay_ms);
    }

    fn tick(&mut self, now_ms: u64) {
        RoomClient::tick(self, now_ms);
    }

    fn take_live_news(&mut self) -> Vec<ddi_room::live::LiveNews> {
        RoomClient::take_live_news(self)
    }

    fn stop_live_news(&mut self) {
        RoomClient::stop_live_news(self);
    }

    fn live_news_whole(&self, stage: StageId) -> bool {
        self.live_news.whole(stage)
    }

    fn read_live_again(&mut self, stage: StageId) {
        self.live_news.read_again(stage);
        if let Link::Online(l) = &mut self.link {
            l.authority.live_mut().read_again(stage);
        }
    }

    fn live_stream(&mut self, stage: StageId) -> Option<&mut ddi_room::live::Outbox> {
        RoomClient::live_stream(self, stage)
    }

    fn forget_live(&mut self, stage: StageId) {
        RoomClient::forget_live(self, stage);
    }

    fn decision(&self, stage: StageId) -> Option<(Vec<Vec<Finisher>>, bool)> {
        let s = self.state();
        let result = s.stage(stage)?.play.as_ref()?.result.as_ref()?;
        Some(decision_of(&s.phase, stage, &result.placements))
    }

    fn live_prep(&self, stage: StageId) -> Option<LivePrep> {
        RoomClient::live_prep(self, stage).cloned()
    }

    fn cpu_news(
        &mut self,
        stage: StageId,
        cpu: ParticipantId,
        through_us: i64,
        reports: Vec<ddi_room::live::LiveReport>,
        end: Option<ddi_room::live::StreamEnd>,
    ) {
        if !self.plays_live() {
            return;
        }
        let sends = ddi_room::live::cpus::sends_for_cpus(self.state(), stage, self.me);
        if let Link::Online(l) = &mut self.link {
            let live = l.authority.live_mut();
            live.cpu_news(stage, cpu, through_us, reports, end);
            live.set_cpu_sends(stage, sends);
        }
    }

    fn cpu_run(
        &mut self,
        stage: StageId,
        cpu: ParticipantId,
        run: ddi_room::ReportedRun,
        detail: Option<RunDetail>,
    ) {
        if !self.plays_live() {
            return;
        }
        if let Link::Online(l) = &mut self.link {
            l.authority.live_mut().cpu_run(stage, cpu, run, detail);
        }
    }
}

/// What the shells' switches change in a practice room
/// (`docs/plans/multiplayer-prototype-progress.md`, M8a), never reached
/// from a menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Development {
    /// A CONVENIENCE, not a gate: how the next practice room's Attacks
    /// rule starts, where a switch says (the web's `mpattacks=0`, Off;
    /// `mpattacks=1` and desktop's `--mp-attacks`, On, which it is by
    /// default since 2026-11-12, so those are kept only for old commands
    /// and scripts); none: the rules' default. Attacks are a match rule
    /// anyone sets in Rules.
    pub attacks: Option<bool>,
    /// DEVELOPMENT ONLY.
    /// A section of battle shocks put into the player's own play of each
    /// stage from this beat on (desktop's `--mp-dev-mines BEAT`, the
    /// web's `mpdevmines=BEAT`; `ddi_engine::battle`).
    pub mines_at_beat: Option<u32>,
    /// DEVELOPMENT ONLY: the CPUs of the next practice room beyond the
    /// cap (0: an ordinary one; desktop's `--mp-dev-cpus N`, the web's
    /// `mpcpus=N`; [`practice_room`], `RoomClient::development`;
    /// `docs/plans/multiplayer-prototype-progress.md`, M6b).
    pub cpus: u16,
}

thread_local! {
    static DEVELOPMENT: std::cell::Cell<Development> = const {
        std::cell::Cell::new(Development {
            attacks: None,
            mines_at_beat: None,
            cpus: 0,
        })
    };
}

/// Sets the practice rooms' switches (the next practice room on; a
/// stage's development shocks from its next order).
pub fn set_development(d: Development) {
    DEVELOPMENT.set(d);
}

/// The development switches set ([`set_development`]).
pub fn development() -> Development {
    DEVELOPMENT.get()
}

/// A new practice room as the shells make it, at `now_ms` (ms since the
/// epoch, its seed too): [`RoomClient::shared`]'s, or where the
/// development switch asks for CPUs beyond the cap
/// ([`Development::cpus`]) [`RoomClient::development`]'s.
pub fn practice_room(now_ms: u64) -> SharedRoom {
    match development().cpus {
        0 => RoomClient::shared(now_ms, now_ms),
        cpus => Rc::new(RefCell::new(RoomClient::development(now_ms, now_ms, cpus))),
    }
}

/// Practice's rules for a common CPU profile `opponent`: `MatchRules::practice`
/// (its Attacks rule On), the rule as the shells' convenience switch has
/// it where one was given.
pub fn practice_rules(opponent: OpponentSpec) -> MatchRules {
    let mut rules = MatchRules::practice(opponent);
    if let Some(on) = development().attacks {
        rules.set_attacks(on);
    }
    rules
}

/// The common CPU profile of a practice room seeded by `seed`.
fn practice_opponent(seed: u64) -> OpponentSpec {
    OpponentSpec {
        level: DEFAULT_CPU_LEVEL,
        seed,
        personality: None,
        name: None,
        targeting: None,
    }
}

impl RoomClient {
    /// A new practice room in its lobby: the local player, three CPUs at
    /// the default level, four empty stages (`MatchRules::practice`). The
    /// CPUs' seeds come from `seed` (the shells pass the time it was
    /// made, tests a fixed number).
    pub fn new(now_ms: u64, seed: u64) -> RoomClient {
        Self::practice_under(now_ms, Self::starting_rules(seed), RoomPolicy::default())
    }

    /// DEVELOPMENT ONLY: a practice room of one stage against `cpus` CPUs,
    /// beyond the cap (`RoomPolicy::development`, its points
    /// `AwardScheme::DevelopmentV0`), for trying the miniature fields' layout
    /// and cost with far more opponents
    /// (`docs/plans/multiplayer-prototype-progress.md`, M6b). Reached only by
    /// the shells' development switches (the web's `mpcpus=`, desktop's
    /// `--mp-dev-cpus`), never from a menu.
    pub fn development(now_ms: u64, seed: u64, cpus: u16) -> RoomClient {
        let policy = RoomPolicy::development();
        let cpus = cpus.clamp(1, policy.max_competitors - 1);
        let mut rules = practice_rules(practice_opponent(seed));
        rules.stages = 1;
        rules.seats = cpus + 1;
        rules.cpu.mode = ddi_room::CpuMode::Fixed { count: cpus };
        rules.awards = ddi_room::AwardScheme::DevelopmentV0;
        Self::practice_under(now_ms, rules, policy)
    }

    /// A client over `link`, its bookkeeping empty.
    fn with_link(
        link: Link,
        placeholder: Option<RoomState>,
        clock: ManualClock,
        client: ClientId,
        me: ParticipantId,
    ) -> RoomClient {
        RoomClient {
            link,
            seen_twitch: None,
            chat_access_told: None,
            chat_troubles: BTreeMap::new(),
            twitch_off_asked: false,
            twitch_pending: None,
            placeholder,
            clock,
            client,
            seq: 0,
            unanswered: None,
            ready_asked: None,
            link_seen: LinkState::Joining,
            left_at: None,
            mirror: Mirror::new(),
            me,
            picks: BTreeMap::new(),
            refusal: None,
            version: 0,
            jobs: Vec::new(),
            asked: BTreeSet::new(),
            load_said: BTreeMap::new(),
            launched: BTreeSet::new(),
            left_prompts: BTreeSet::new(),
            stayed: BTreeSet::new(),
            telling: Rc::default(),
            notice_stage: None,
            sharing: BTreeMap::new(),
            uploads: Vec::new(),
            inbox: None,
            resting: None,
            resting_on_none: None,
            spotlight_sharing: None,
            spotlight_loaded: None,
            spotlight_asked: None,
            rules_seen: false,
            own_name: None,
            results_seen: (None, 0),
            details: BTreeMap::new(),
            summaries: BTreeMap::new(),
            asking: Rc::default(),
            elsewhere_told: std::cell::Cell::new(None),
            asking_end: std::cell::Cell::new(false),
            pick_last: false,
            never_in_seen: std::cell::Cell::new(None),
            load_tries: BTreeMap::new(),
            retrying: Vec::new(),
            failed_jobs: BTreeMap::new(),
            stale_before: None,
            library: BTreeMap::new(),
            song_sharing: BTreeMap::new(),
            ahead: Default::default(),
            pack_keys: Vec::new(),
            choosing: Default::default(),
            starts: Default::default(),
            live_news: LiveNewsTap::default(),
            preparing: BTreeMap::new(),
            offered: BTreeMap::new(),
            live_preps: BTreeMap::new(),
            remembers_roster: false,
            watch_music: crate::state::audience::MusicDelays::new(WATCH_MUSIC_V0),
        }
    }

    /// Practice's rules with its starting roster of CPUs, seeded by `seed`.
    fn starting_rules(seed: u64) -> MatchRules {
        let rules = practice_rules(practice_opponent(seed));
        let roster = super::cpu_roster::starting(STARTING_CPUS, seed);
        super::cpu_roster::with_roster(rules, roster, 1)
    }

    fn practice_under(now_ms: u64, rules: MatchRules, policy: RoomPolicy) -> RoomClient {
        let clock = ManualClock::new(now_ms);
        // The player's name is the room's data; the screen calls the local
        // player "You" in the language shown.
        let (authority, client) =
            LocalAuthority::practice_under("Player".into(), rules, clock.clone(), policy);
        let me = authority.participant(&client).unwrap_or(ParticipantId(1));
        let mut room =
            RoomClient::with_link(Link::Local(Box::new(authority)), None, clock, client, me);
        room.pump();
        room
    }

    /// A new practice room as the shells make it: [`RoomClient::new`]'s,
    /// its CPUs the roster the last practice room was left with where the
    /// settings keep one this build reads (`Settings::practice_roster`;
    /// the maintainer, 2026-11-13), and its roster kept there again as it
    /// changes. Never the stage count, the other rules or the player's
    /// own attack policy.
    pub fn practice(now_ms: u64, seed: u64) -> RoomClient {
        let mut room = RoomClient::new(now_ms, seed);
        let most = usize::from(room.max_cpus());
        let kept = crate::state::settings::Settings::cached()
            .practice_roster
            .and_then(|v| super::cpu_roster::from_kept(&v, seed, most));
        if let Some(roster) = kept {
            let rules = super::cpu_roster::with_roster(room.state().next.rules.clone(), roster, 1);
            if room.check_rules(&rules).is_ok() {
                let _ = room.save_rules(rules);
            }
        }
        room.remembers_roster = true;
        room
    }

    pub fn shared(now_ms: u64, seed: u64) -> SharedRoom {
        Rc::new(RefCell::new(RoomClient::practice(now_ms, seed)))
    }

    /// Whether this room keeps its roster in the settings as it changes
    /// ([`RoomClient::practice`]).
    pub fn remembers_roster(&self) -> bool {
        self.remembers_roster
    }

    /// The next match's roster kept in the settings where this room
    /// remembers it ([`RoomClient::practice`]), saved only when it changed.
    fn remember_roster(&self) {
        if !self.remembers_roster {
            return;
        }
        let rules = &self.state().next.rules;
        let roster = match rules.cpu.mode {
            ddi_room::CpuMode::Roster | ddi_room::CpuMode::Off => rules.cpu.roster.clone(),
            _ => return,
        };
        let kept = super::cpu_roster::kept(&roster);
        let mut settings = crate::state::settings::Settings::load();
        if settings.practice_roster.as_ref() != Some(&kept) {
            settings.practice_roster = Some(kept);
            settings.save();
        }
    }

    /// Online room `room_id` for the person signed in on `online`, its
    /// socket `transport`, this client's id `client` (unique to this
    /// device's session: the server keeps its last sequence number). It
    /// joins at once: a ticket, then the socket's snapshot.
    pub fn online(
        online: SharedOnline,
        room_id: &str,
        transport: Box<dyn Transport>,
        client: ClientId,
        now_ms: u64,
    ) -> RoomClient {
        let clock = ManualClock::new(now_ms);
        let link = OnlineLink::new(online, room_id, transport, clock.clone());
        let mut placeholder = RoomState::new(
            RoomId::new(room_id),
            Identity::new(""),
            String::new(),
            RoomPolicy::default(),
            MatchRules::hosted(),
            ContentSource::Shared,
        );
        placeholder.members.clear();
        let mut room = RoomClient::with_link(
            Link::Online(Box::new(link)),
            Some(placeholder),
            clock,
            client,
            ParticipantId(0),
        );
        room.tick(now_ms);
        room
    }

    /// The room as this client knows it.
    pub fn state(&self) -> &RoomState {
        if let Some(s) = self.mirror.state() {
            return s;
        }
        match &self.link {
            Link::Local(a) => a.state(),
            Link::Online(l) => l
                .authority
                .room()
                .or(self.placeholder.as_ref())
                .expect("an online room has its placeholder"),
        }
    }

    fn authority_mut(&mut self) -> &mut dyn RoomAuthority {
        match &mut self.link {
            Link::Local(a) => a.as_mut(),
            Link::Online(l) => &mut l.authority,
        }
    }

    /// An online room's link (none for practice).
    pub fn online_link(&self) -> Option<&OnlineLink> {
        match &self.link {
            Link::Online(l) => Some(l),
            Link::Local(_) => None,
        }
    }

    fn online_link_mut(&mut self) -> Option<&mut OnlineLink> {
        match &mut self.link {
            Link::Online(l) => Some(l),
            Link::Local(_) => None,
        }
    }

    /// Whether this is an online room.
    pub fn is_online(&self) -> bool {
        matches!(self.link, Link::Online(_))
    }

    /// An online room on the web (a page that reloads to update), as
    /// against desktop's.
    pub fn on_web(&self) -> bool {
        self.online_link().is_some_and(|l| {
            l.online().borrow().config.platform == crate::state::online::Platform::Web
        })
    }

    /// Whether this online room plays its stages live
    /// (`docs/plans/multiplayer-online-play.md`): its policy says so
    /// (`RoomPolicy::live`, a server that predates it says nothing) and
    /// the shell's socket carries binary messages. Practice: never (its
    /// stages are played on this device).
    pub fn plays_live(&self) -> bool {
        let Some(l) = self.online_link() else {
            return false;
        };
        l.authority.transport().binary()
            && self
                .state()
                .policy
                .live
                .as_ref()
                .and_then(|p| p.rules())
                .is_some()
    }

    /// The live link of an online room's socket (`ddi_room::live`): the
    /// room's clock, the frames read.
    pub fn live_link(&self) -> Option<&ddi_room::live::LiveLink> {
        self.online_link().map(|l| l.authority.live())
    }

    /// The room's time now on its own clock, as estimated from this
    /// device's (an online room's that plays live, once its clock is
    /// known).
    pub fn room_now(&self) -> Option<ddi_room::Millis> {
        self.online_link().and_then(|l| l.authority.room_now())
    }

    /// The online link's state (practice: always connected).
    pub fn link_state(&self) -> LinkState {
        self.online_link()
            .map_or(LinkState::Connected, OnlineLink::state)
    }

    /// Whether this online room is gone for the local player for good,
    /// their screens showing nothing they can do in it (round BM): removed
    /// by a host (`LinkState::Kicked`), kept out by the host's Twitch lists
    /// (`NotAdmitted`), the room closed (`Closed`), no member any more by
    /// another way than this device's own Leave (`Left`), or a room's page
    /// opened by someone never in it (`NotAMember`, `SignInToEnter`). Its
    /// screens say so in a notice whose OK goes to the multiplayer home
    /// (`screens::room::tell_opened_elsewhere`), no stage's prompt is
    /// theirs ([`RoomClient::view`]), and a play of theirs still running
    /// is left as a quit is ([`LiveRoom::seat_end`]). Not the ends a
    /// person mends from here (signed out, the room opened on another
    /// device, a newer game or server).
    pub fn room_gone(&self) -> bool {
        match self.link_state() {
            LinkState::Kicked
            | LinkState::NotAdmitted
            | LinkState::Closed
            | LinkState::NotAMember
            | LinkState::SignInToEnter => true,
            LinkState::Left => self.left_at.is_none(),
            _ => false,
        }
    }

    /// The room's name (online, once read).
    pub fn name(&self) -> Option<&str> {
        self.online_link().and_then(OnlineLink::name)
    }

    /// Whether the local player may `capability` now, as the room decides
    /// it (`ddi_room::can`, from their role and the admin flag): what the
    /// screen offers. Practice: the player owns the room.
    pub fn may(&self, capability: Capability) -> bool {
        let s = self.state();
        s.member(self.me)
            .filter(|m| !m.left)
            .is_some_and(|m| ddi_room::can(m.role, m.admin, capability, s.phase.kind()))
    }

    /// Whether the local player chooses the room's songs now (one picker
    /// at a time, `ddi_room::Picking`): their own list is the room's song
    /// select, the song it rests on is told the room, their Start picks.
    /// Everyone else follows the picker's song. In a room whose authority
    /// predates the picker, every operator, as it was; in practice, the
    /// player.
    pub fn chooses(&self) -> bool {
        self.state().chooses(self.me)
    }

    /// Who chooses the room's songs now; none: nobody, until an operator
    /// takes the pick.
    pub fn picker(&self) -> Option<ParticipantId> {
        self.state().picker()
    }

    /// Whether the local player may take the pick over now: an operator
    /// who does not have it.
    pub fn may_take_pick(&self) -> bool {
        self.link_state() == LinkState::Connected && self.state().may_take_pick(self.me)
    }

    /// Take the pick over (`Command::TakePick`): at once, the previous
    /// picker follows. Nothing is sent where it cannot be taken.
    pub fn take_pick(&mut self) -> Result<(), Refusal> {
        if !self.may_take_pick() {
            return Ok(());
        }
        self.send(Command::TakePick)
    }

    /// What the room does when its host goes (meaningful online:
    /// practice is never passed on).
    pub fn if_host_leaves(&self) -> ddi_room::HostLeaves {
        self.state().if_host_leaves()
    }

    /// Whether the local player may change the room's hosts now: an
    /// online room's owner, connected (co-hosts made and removed, and
    /// where the room passes on, the room handed over and its setting).
    pub fn manages_hosts(&self) -> bool {
        self.is_online()
            && self.link_state() == LinkState::Connected
            && self.may(Capability::ManageCoHosts)
    }

    /// The people whose role the owner may change: everyone else present,
    /// by joining order; never a bot (a bot follows a room) nor a CPU.
    /// A stage of this room waits for its player's page to be seen before
    /// it counts down and starts (round AC, the maintainer: "Wait for
    /// you"): CPU practice with nobody else in it. An online room, and a
    /// practice room with another person in it were there such a thing,
    /// starts and plays on unseen (round P1b).
    pub fn waits_unseen(&self) -> bool {
        !self.is_online() && self.other_people().is_empty()
    }

    pub fn other_people(&self) -> Vec<&ddi_room::Member> {
        self.state()
            .members
            .values()
            .filter(|m| m.id != self.me && !m.left && !m.is_cpu() && !m.is_bot())
            .collect()
    }

    /// The owner sets what the room does when its host goes. Nothing is
    /// sent where the room does not pass on or the player may not.
    pub fn set_host_leaves(&mut self, policy: ddi_room::HostLeaves) -> Result<(), Refusal> {
        if !self.manages_hosts() || self.if_host_leaves() == policy {
            return Ok(());
        }
        self.send(Command::SetHostLeaves { policy })
    }

    /// The owner makes `participant` a co-host, or a member again.
    pub fn set_co_host(&mut self, participant: ParticipantId, on: bool) -> Result<(), Refusal> {
        if !self.manages_hosts() {
            return Ok(());
        }
        self.send(Command::SetCoHost { participant, on })
    }

    /// Whether the owner may hand the room to `participant` now: someone
    /// there to run it (present, connected, a person).
    pub fn may_make_host(&self, participant: ParticipantId) -> bool {
        self.manages_hosts()
            && self.may(Capability::MakeHost)
            && self.state().member(participant).is_some_and(|m| {
                m.id != self.me
                    && !m.left
                    && !m.is_cpu()
                    && !m.is_bot()
                    && m.connection == ddi_room::Connection::Connected
            })
    }

    /// The owner hands the room to `participant` and becomes a co-host.
    /// Nothing is sent where it may not be.
    pub fn make_host(&mut self, participant: ParticipantId) -> Result<(), Refusal> {
        if !self.may_make_host(participant) {
            return Ok(());
        }
        self.send(Command::MakeHost { participant })
    }

    /// Whether the local player moderates the room's people (room model
    /// revision 18, the Players pop-up's Moderate): an online room's host
    /// or co-host (or an administrator who operates), connected, with
    /// someone they may kick. CPU practice has nobody to kick.
    pub fn may_moderate(&self) -> bool {
        self.is_online()
            && self.link_state() == LinkState::Connected
            && self.may(Capability::Kick)
            && self
                .state()
                .members
                .keys()
                .any(|&id| self.state().can_kick(self.me, id))
    }

    /// Whether the local player may kick `participant` now
    /// (`ddi_room::RoomState::may_kick`: the host anyone but themselves, a
    /// co-host ordinary members only).
    pub fn may_kick(&self, participant: ParticipantId) -> bool {
        self.is_online()
            && self.link_state() == LinkState::Connected
            && self.state().can_kick(self.me, participant)
    }

    /// Kick `participant` out of the room (B9): at once; they may come
    /// back by the invite. Nothing is sent where it may not be.
    pub fn kick(&mut self, participant: ParticipantId) -> Result<(), Refusal> {
        if !self.may_kick(participant) {
            return Ok(());
        }
        self.send(Command::Remove { participant })
    }

    /// The room's Twitch setting (revision 18): whose lists keep people
    /// out and how much of their chat the room may show; none: off.
    pub fn twitch(&self) -> Option<&ddi_room::TwitchSetting> {
        self.state().twitch.as_ref()
    }

    /// Whether the local player may change the room's Twitch setting
    /// (B1, B3): an online room's owner (turn on, change, replace with
    /// their own), or the one who set it (turn it, or a part, off).
    pub fn may_set_twitch(&self) -> bool {
        self.is_online()
            && self.link_state() == LinkState::Connected
            && self.my_role().is_some_and(|(_, _)| {
                self.may(Capability::Twitch) || self.twitch().is_some_and(|t| t.by == self.me)
            })
            && self
                .state()
                .member(self.me)
                .is_some_and(|m| !m.is_bot())
    }

    /// Ask the server for the room's Twitch setting `mirror` and `chat`
    /// (with the local player's own lists and channel). Where their Twitch
    /// access lacks a scope, [`RoomClient::take_twitch_trip`] says which.
    pub fn set_twitch(&mut self, mirror: ddi_room::TwitchMirror, chat: ddi_room::TwitchChat) {
        if !self.may_set_twitch() {
            return;
        }
        self.twitch_off_asked =
            mirror == ddi_room::TwitchMirror::Off && chat == ddi_room::TwitchChat::Off;
        if let Some(l) = self.online_link_mut() {
            l.set_twitch(mirror, chat);
            self.twitch_pending = Some((mirror, chat, TwitchAsking::Sent));
            self.version += 1;
        }
    }

    /// The room's Twitch setting as Host options show it and build the
    /// next change on (round TB): the one this player asked until the room
    /// says it (a trip through Twitch on the way, the answer in flight),
    /// else the room's. Each change asks for the whole setting, so a
    /// second row changed within the room's round trip once read the first
    /// row from the room and undid it.
    pub fn twitch_shown(&self) -> (ddi_room::TwitchMirror, ddi_room::TwitchChat) {
        match self.twitch_pending {
            Some((mirror, chat, _)) => (mirror, chat),
            None => self.twitch().map_or(
                (ddi_room::TwitchMirror::Off, ddi_room::TwitchChat::Off),
                |t| (t.mirror, t.chat),
            ),
        }
    }

    /// Round TB: the setting asked is forgotten once the room says it,
    /// once the server refused it, a moment after the server took it (the
    /// room says it with its next change; another change may have come
    /// first), or once its trip through Twitch ended without the access.
    fn settle_twitch_pending(&mut self) {
        let answer = self.online_link_mut().and_then(OnlineLink::take_twitch_answer);
        let Some((mirror, chat, asking)) = self.twitch_pending else {
            return;
        };
        let now = self.now_ms();
        let said = self
            .twitch()
            .map_or((ddi_room::TwitchMirror::Off, ddi_room::TwitchChat::Off), |t| (t.mirror, t.chat));
        let asking = match answer {
            // A trip through Twitch first: kept until it ends.
            Some(409) => TwitchAsking::Trip,
            Some(s) if (200..300).contains(&s) => TwitchAsking::Taken(now + TWITCH_SAID_MS),
            Some(_) => TwitchAsking::Taken(now),
            None => asking,
        };
        // The trip ended without the access (canceled, failed): nothing
        // was set (one still to start keeps it).
        let trip_ended = asking == TwitchAsking::Trip
            && self.online_link().is_some_and(|l| {
                !l.twitch_trip_waiting()
                    && matches!(
                        l.online().borrow().sign_in_state(),
                        crate::state::online::SignIn::Canceled { .. }
                            | crate::state::online::SignIn::Failed { .. }
                    )
            });
        let given_up = matches!(asking, TwitchAsking::Taken(until) if now >= until);
        self.twitch_pending = if said == (mirror, chat) || trip_ended || given_up {
            None
        } else {
            Some((mirror, chat, asking))
        };
        if self.twitch_pending.is_none() {
            self.version += 1;
        }
    }

    /// The trip through Twitch the setting asked needs first, once: the
    /// local player's Twitch access is connected for those features and
    /// they come back to this room (the setting then asked again).
    pub fn take_twitch_trip(&mut self) -> Option<String> {
        let (trip, asked) = self.online_link_mut()?.take_twitch_trip()?;
        let link = self.online_link()?;
        let room = link.room_id().to_string();
        let wanted = asked.map(|(mirror, chat)| crate::state::online::TwitchWanted {
            room: room.clone(),
            mirror,
            chat,
        });
        link.online().borrow_mut().connect_twitch_for(
            crate::state::online::Dest::Room(room),
            &trip,
            wanted,
        );
        Some(trip)
    }

    /// Round TW1b: back from a trip through Twitch for this room's
    /// setting, the setting asked made by itself (no second press), or,
    /// the trip canceled, the status line says nothing changed.
    fn finish_twitch_trip(&mut self) {
        if self.link_state() != LinkState::Connected {
            return;
        }
        let Some(link) = self.online_link() else {
            return;
        };
        let (online, room) = (link.online().clone(), link.room_id().to_string());
        // Round TW3: a trip for this device's chat, back here.
        let chat = online.borrow_mut().take_chat_outcome();
        if let Some(on) = chat {
            self.set_link_notice(Some(if on {
                LinkNotice::TwitchChatOn
            } else if self.chat_allow_offered() {
                // Round FX: canceled at Twitch where the room shows chat:
                // nothing changed, the line and its offer stay, and the
                // room asks no more this session.
                decline_chat();
                LinkNotice::TwitchChatNoAccess
            } else {
                LinkNotice::TwitchNotConnected
            }));
        }
        let Some((wanted, canceled)) = online.borrow_mut().take_twitch_wanted(&room) else {
            return;
        };
        if canceled {
            self.twitch_pending = None;
            self.version += 1;
            self.set_link_notice(Some(LinkNotice::TwitchNotConnected));
        } else {
            self.set_twitch(wanted.mirror, wanted.chat);
        }
    }

    /// The room's Twitch setting ended without this player asking (B3,
    /// B8, A3; round TW3): said on the status line, to everyone where the
    /// one who set it left the room, to them where their Twitch access
    /// ended (refused at Twitch, disconnected elsewhere).
    fn tell_twitch_end(&mut self) {
        if !self.is_online() || self.link_state() != LinkState::Connected {
            return;
        }
        // (Unchanged: nothing to say, nothing copied.)
        if self.seen_twitch.as_ref() == self.state().twitch.as_ref() {
            return;
        }
        let now = self.state().twitch.clone();
        let Some(was) = std::mem::replace(&mut self.seen_twitch, now.clone()) else {
            return;
        };
        if now.is_some() {
            return;
        }
        if std::mem::take(&mut self.twitch_off_asked) {
            return;
        }
        let gone = self.state().member(was.by).is_none_or(|m| m.left);
        if gone {
            self.set_link_notice(Some(LinkNotice::TwitchSetterLeft { who: was.by }));
        } else if was.by == self.me {
            self.set_link_notice(Some(LinkNotice::TwitchEnded));
        }
    }

    /// Round TW7: where the room shows its host's chat and this device's
    /// "Twitch chat in rooms" is on, but the person's Twitch access (as
    /// `/me` last said it, read for the room anyway) does not allow
    /// reading chat, the status line says what to do, once for each
    /// channel the room shows.
    fn tell_chat_access(&mut self, settings: &TickSettings) {
        if !self.is_online() || self.link_state() != LinkState::Connected {
            return;
        }
        let Some(chat) = crate::state::twitch::RoomChat::of(self.twitch()) else {
            self.chat_access_told = None;
            return;
        };
        if self.chat_access_told.as_deref() == Some(&*chat.channel_id) {
            return;
        }
        let lacks = {
            let Some(link) = self.online_link() else { return };
            let o = link.online().borrow();
            match o.twitch() {
                Some(t) => !t.has("chat"),
                // Not read yet: nothing to say.
                None => return,
            }
        };
        if settings.get().twitch_chat == ddi_chat::Level::Off {
            return;
        }
        self.chat_access_told = Some(chat.channel_id);
        // Never asked again in the session once the player said no at
        // Twitch (round FX); More's Allow Twitch chat still offers it.
        if lacks && !chat_declined() {
            self.set_link_notice(Some(LinkNotice::TwitchChatNoAccess));
        }
    }

    /// Round TB: the Twitch permissions this device uses in this room,
    /// written down for its next sign-in to ask for again
    /// (`crate::state::twitch::mark_used`): what the room's Twitch setting
    /// needs where this player set it (they host with it on), and the
    /// chat where the room shows its host's chat and this device shows it
    /// (its "Twitch chat in rooms" on, its access allowing chat).
    fn mark_twitch_used(&self, settings: &TickSettings) {
        if !self.is_online() || self.link_state() != LinkState::Connected {
            return;
        }
        let Some(t) = self.twitch() else {
            return;
        };
        let mut used = Vec::new();
        if t.by == self.me {
            used.extend(crate::state::online::api::twitch_needs(t.mirror, t.chat));
        }
        let shows_chat = t.chat != ddi_room::TwitchChat::Off
            && !used.contains(&"chat")
            && self.online_link().is_some_and(|l| {
                l.online()
                    .borrow()
                    .twitch()
                    .is_some_and(|a| a.has("chat"))
            })
            && settings.get().twitch_chat != ddi_chat::Level::Off;
        if shows_chat {
            used.push("chat");
        }
        crate::state::twitch::mark_used_in(&used, settings);
    }

    /// An online room's chat as its stages show it (revision 18): the
    /// room's ceiling and its channel, the room's id naming its subscribe
    /// route (round TW6), the development script's named channel in its
    /// place on this device alone (round TW7); none where the room shows
    /// none. A stage's setup takes it as its prompt opens, and the stage
    /// follows it while it plays (round SD, [`LiveRoom::room_chat`]).
    pub fn stage_chat(&self) -> Option<crate::state::twitch::RoomChat> {
        crate::state::twitch::RoomChat::of(self.twitch()).map(|c| {
            c.in_room(self.online_link().map(|l| l.room_id().to_string()))
                .on_this_device()
        })
    }

    /// Round DE: the room's chat as this device keeps it connected
    /// between the stages (`ddi_session::chat`'s keeper, told at each
    /// tick): an online room this member is in (connected, or getting
    /// back after a drop), that shows its host's chat, this device's
    /// "Twitch chat in rooms" on and its Twitch access (as `/me` last
    /// said it) allowing chat; the room's channel as this device shows it
    /// (`RoomChat::on_this_device`) at the device's level under the
    /// room's ceiling. None in CPU practice, for a bot, or before the
    /// access is known: a stage then opens its own connection as before
    /// (round TW6), and gives it to the room at its end where the room
    /// wants it by then.
    pub fn chat_kept(&self) -> Option<crate::state::twitch::KeptChat> {
        self.chat_kept_in(&TickSettings::new())
    }

    /// [`RoomClient::chat_kept`] with the settings as the tick read them.
    fn chat_kept_in(&self, settings: &TickSettings) -> Option<crate::state::twitch::KeptChat> {
        if !self.is_online()
            || self.left_at.is_some()
            || !matches!(self.link_state(), LinkState::Connected | LinkState::Reconnecting { .. })
            || self.state().member(self.me).is_none_or(|m| m.is_bot())
        {
            return None;
        }
        // The room's and the access's first (the settings are read from
        // storage).
        let chat = crate::state::twitch::RoomChat::of(self.twitch())?;
        let link = self.online_link()?;
        let allowed = link
            .online()
            .borrow()
            .twitch()
            .is_some_and(|a| a.has("chat"));
        if !allowed {
            return None;
        }
        let settings = settings.get();
        if settings.twitch_chat == ddi_chat::Level::Off {
            return None;
        }
        let chat = chat
            .in_room(Some(link.room_id().to_string()))
            .on_this_device();
        let level = settings.twitch_chat.under(chat.ceiling);
        (level != ddi_chat::Level::Off).then_some(crate::state::twitch::KeptChat {
            chat,
            level,
            emotes_from: settings.chat_emotes_from,
        })
    }

    /// Round RT: the user's clears the room's chat connection heard since
    /// (`twitch::take_cleared`: Twitch's chat cleared someone's messages
    /// in the room's channel, which a ban or a timeout does), each asked
    /// of the room to check again (`OnlineLink::recheck`) where the
    /// room's Twitch mirror is on, it was heard in the channel of the one
    /// who set it, and the person is in the room (not left, not the one
    /// who set it, never a bot). Nothing in CPU practice (no online room)
    /// or once this player left; the server decides by asking Twitch.
    fn recheck_cleared(&mut self) {
        let Some(room) = self.online_link().map(|l| l.room_id().to_string()) else {
            return;
        };
        let cleared = crate::state::twitch::take_cleared(&room);
        if cleared.is_empty() || self.left_at.is_some() {
            return;
        }
        let Some(t) = self.twitch().filter(|t| t.mirror.on()) else {
            return;
        };
        let in_room = |user_id: &str| {
            self.state().members.values().any(|m| {
                !m.left
                    && m.id != t.by
                    && m.identity()
                        .and_then(|i| crate::state::twitch::user_id_of(i.as_str()))
                        .is_some_and(|u| u == user_id)
            })
        };
        let asked: Vec<String> = cleared
            .into_iter()
            .filter(|c| c.channel_id == t.user_id && in_room(&c.user_id))
            .map(|c| c.user_id)
            .collect();
        if let Some(l) = self.online_link_mut() {
            for user_id in asked {
                l.recheck(&user_id);
            }
        }
    }

    /// Round FX: the room shows its host's chat, this device's "Twitch
    /// chat in rooms" is on, and the person's Twitch access (as `/me`
    /// last said it) does not allow reading chat: the room offers the
    /// trip through Twitch that allows it ([`RoomClient::allow_chat`]: the
    /// status line's button while it says so, and More's item). Never in
    /// CPU practice (no online room), never for a bot.
    pub fn chat_allow_offered(&self) -> bool {
        if !self.is_online() || self.link_state() != LinkState::Connected {
            return false;
        }
        if crate::state::twitch::RoomChat::of(self.twitch()).is_none()
            || crate::state::settings::Settings::cached().twitch_chat == ddi_chat::Level::Off
            || self.state().member(self.me).is_none_or(|m| m.is_bot())
        {
            return false;
        }
        self.online_link().is_some_and(|link| {
            link.online()
                .borrow()
                .twitch()
                .is_some_and(|t| !t.has("chat"))
        })
    }

    /// Round FX: the trip through Twitch for this device's chat started
    /// from the room (as Options' "Twitch chat in rooms" starts it), back
    /// to this room, the setting's level kept: the web leaves the page on
    /// the room's next update (its redirect), desktop's dialog shows the
    /// code to type in the browser (the caller opens it). False where the
    /// room offers none.
    pub fn allow_chat(&mut self) -> bool {
        if !self.chat_allow_offered() {
            return false;
        }
        let Some(link) = self.online_link() else {
            return false;
        };
        let level = crate::state::settings::Settings::cached().twitch_chat;
        let dest = crate::state::online::Dest::Room(link.room_id().to_string());
        link.online().borrow_mut().connect_twitch_chat(dest, level);
        true
    }

    /// Round TW7: why this device's Twitch chat could not show during
    /// `stage` (the play's live chat gave up), for one quiet line on its
    /// results; none: nothing to say.
    pub fn set_chat_trouble(&mut self, stage: StageId, trouble: Option<ddi_chat::live::Trouble>) {
        if let Some(t) = trouble {
            self.chat_troubles.insert(stage, t);
            self.version += 1;
        }
    }

    /// The local player's role and admin flag.
    pub fn my_role(&self) -> Option<(Role, bool)> {
        self.state().member(self.me).map(|m| (m.role, m.admin))
    }

    /// The invite link to share (an online room's operators, once read).
    pub fn invite_link(&self) -> Option<String> {
        self.online_link().and_then(OnlineLink::invite_link)
    }

    /// The watch-only link to share (an online room's operators, from a
    /// server that makes one, once read).
    pub fn watch_invite_link(&self) -> Option<String> {
        self.online_link().and_then(OnlineLink::watch_invite_link)
    }

    /// The audience link to share (an online room's operators, from a
    /// server whose room has an audience, once read).
    pub fn audience_invite_link(&self) -> Option<String> {
        self.has_audience()
            .then(|| self.online_link().and_then(OnlineLink::audience_invite_link))
            .flatten()
    }

    /// The room has an audience (`docs/plans/multiplayer-audience.md`):
    /// its policy says so in a contract this build reads, and it plays
    /// live (the count and the closing are live messages). A room on a
    /// server without one shows no count, link or control.
    pub fn has_audience(&self) -> bool {
        self.plays_live()
            && self
                .state()
                .policy
                .audience
                .as_ref()
                .and_then(|a| a.rules())
                .is_some()
    }

    /// How many watch with the room's audience and whether it is open, as
    /// the room last said (`ServerLive::Audience`); none before it said
    /// or where it has none.
    pub fn audience(&self) -> Option<(u32, bool)> {
        self.has_audience()
            .then(|| self.live_link().and_then(|l| l.audience()))
            .flatten()
    }

    /// Whether the local player may close and open the room's audience
    /// (an operator's: `Capability::CloseAudience`, round B1).
    pub fn may_close_audience(&self) -> bool {
        self.has_audience() && self.may(Capability::CloseAudience)
    }

    /// Whether the local player has Host options: the owner always
    /// ([`RoomClient::manages_hosts`]); a co-host (or an administrator)
    /// while there is something an operator may do there (round B1: the
    /// audience's row, Let play for someone held to watching), never the
    /// owner's own rows (co-hosts, the host, what the room does if its
    /// host leaves).
    pub fn may_host_options(&self) -> bool {
        self.manages_hosts()
            || (self.is_online()
                && self.link_state() == LinkState::Connected
                && self.may(Capability::LetPlay)
                && (self.may_close_audience()
                    || self
                        .other_people()
                        .iter()
                        .any(|m| m.watch_only && m.role != Role::Owner)))
    }

    /// An operator closes (every seat sent away, joins refused) or opens
    /// the room's audience. Nothing is sent where it may not be.
    pub fn set_audience_open(&mut self, open: bool) {
        if !self.may_close_audience() {
            return;
        }
        if let Some(l) = self.online_link_mut() {
            l.authority
                .live_mut()
                .send(ddi_room::live::ClientLive::Audience { open });
            l.set_notice(Some(if open {
                LinkNotice::AudienceOpened
            } else {
                LinkNotice::AudienceClosed
            }));
        }
        self.version += 1;
    }

    /// The local player came by the watch-only link and is still held to
    /// watching (an owner never is).
    pub fn watch_only(&self) -> bool {
        self.state()
            .member(self.me)
            .is_some_and(|m| m.watch_only && m.role != Role::Owner)
    }

    /// An operator (the owner, a co-host) lets `participant`, held to
    /// watching, play (`Command::SetWatchOnly`): they choose Play
    /// themselves after.
    pub fn let_play(&mut self, participant: ParticipantId) -> Result<(), Refusal> {
        let held = self
            .state()
            .member(participant)
            .is_some_and(|m| m.watch_only);
        if !held || !self.may_host_options() {
            return Ok(());
        }
        self.send(Command::SetWatchOnly {
            participant,
            on: false,
        })
    }

    /// What the online link last did, for the status line.
    pub fn link_notice(&self) -> Option<LinkNotice> {
        self.online_link().and_then(OnlineLink::notice)
    }

    pub fn set_link_notice(&mut self, notice: Option<LinkNotice>) {
        if let Some(l) = self.online_link_mut() {
            l.set_notice(notice);
            self.version += 1;
        }
    }

    /// Ask for the online room's invite (its operators).
    pub fn fetch_invite(&mut self) {
        if let Some(l) = self.online_link_mut() {
            l.fetch_invite();
        }
    }

    /// Read the online room's invite again, where it was read (More
    /// opened): another operator may have replaced it since.
    pub fn refresh_invite(&mut self) {
        if let Some(l) = self.online_link_mut() {
            l.refresh_invite();
        }
    }

    /// Whether the local player may replace the online room's invite (an
    /// operator's, as copying it is: `Capability::ReplaceInvite`, round
    /// B1), once the invite is read.
    pub fn may_replace_invite(&self) -> bool {
        self.invite_link().is_some() && self.may(Capability::ReplaceInvite)
    }

    /// An operator replaces the online room's invite (the watch-only and
    /// audience links with it). Nothing is sent where it may not be.
    pub fn rotate_invite(&mut self) {
        if !self.may_replace_invite() {
            return;
        }
        if let Some(l) = self.online_link_mut() {
            l.rotate_invite();
        }
    }

    /// Sign in again, back to this online room after.
    pub fn sign_in_again(&mut self) {
        if let Some(l) = self.online_link_mut() {
            l.sign_in_again();
        }
    }

    /// Whether a screen of this room has the question before leaving
    /// open (`screens::room::ask_leave` sets it, the question clears it
    /// when it goes): asked once, however many ways out are pressed.
    /// "Use this device" (round P1b): this device stopped because the
    /// room was opened on another of the person's devices
    /// (`LinkState::OpenedElsewhere`); it joins again.
    pub fn use_this_device(&mut self) {
        if self.online_link_mut().is_some_and(|l| l.use_this_device()) {
            self.elsewhere_told.set(None);
            // What the stopped page had asked and is sent again on the
            // new socket is answered for a room gone on: its refusals are
            // not said (round AC: "Not possible at this point of the
            // match." was), the status line saying they are back instead.
            self.stale_before = Some(self.seq);
            self.refusal = None;
            self.version += 1;
        }
    }

    /// Whether a notice of the link's end that the person acts on is
    /// still to be shown (once each time it happens): the room opened on
    /// another device (round P1b), or a room's page opened by someone
    /// not signed in or no member of it (feedback round AA,
    /// [`LinkState::SignInToEnter`], [`LinkState::NotAMember`]). Taken
    /// by the screen that shows it (`screens::room::tell_opened_elsewhere`
    /// says which).
    pub fn take_opened_elsewhere(&self) -> bool {
        // A question the room asks over the screen (End the match now).
        if self.asking_end.get() {
            return true;
        }
        let state = self.link_state();
        let never_in = matches!(state, LinkState::SignInToEnter | LinkState::NotAMember);
        // Removed from the room (revision 18), or the room gone for them
        // another way (round BM: closed, or no member any more other than
        // by this device's own Leave): told once, as a stop is.
        let removed = matches!(state, LinkState::Kicked | LinkState::NotAdmitted)
            || (matches!(state, LinkState::Closed | LinkState::Left) && self.room_gone());
        if state != LinkState::OpenedElsewhere && !never_in && !removed {
            if !state.ended() {
                // The link went on (signed in, joining again): its next
                // end is told anew.
                self.elsewhere_told.set(None);
                self.never_in_seen.set(None);
            }
            return false;
        }
        // A link that never reached the room is told from the next ask
        // on: the screen that asks says the line first (the room's song
        // select's joining line is set in the frame it ended, after this
        // ask).
        if never_in && self.never_in_seen.replace(Some(state)) != Some(state) {
            return false;
        }
        self.elsewhere_told.replace(Some(state)) != Some(state)
    }

    pub fn asking_leave(&self) -> Rc<std::cell::Cell<bool>> {
        self.asking.clone()
    }

    /// What leaving the room now would cost, if anything: the cases a
    /// screen asks before leaving in (2026-11-12). An online room's host,
    /// always (what the room will do: pass on, close, or stay without a
    /// host on an authority that does not pass on); someone in the
    /// running match, choosing its songs or co-hosting it. None for
    /// everyone else (a member between matches, a watcher), for a room
    /// not known yet, and once the link has ended (the room closed, the
    /// sign-in ended, left): nothing is left to lose there. Never in CPU
    /// practice (the maintainer, 2026-11-12: "even silently dropping the
    /// match is fine … a prompt would be annoying"): leaving it with a
    /// match running forgets the practice and the match
    /// (`crate::screens::room::leave_now`).
    pub fn leave_cost(&self) -> Option<LeaveCost> {
        let s = self.state();
        let running = s.phase.kind().match_running();
        if !self.is_online() {
            return None;
        }
        if self.link_state().ended() {
            return None;
        }
        let me = s.member(self.me).filter(|m| !m.left)?;
        if me.role == Role::Owner {
            let others = s
                .members
                .values()
                .any(|m| m.id != self.me && !m.left && !m.is_cpu());
            return Some(match s.if_host_left() {
                ddi_room::HostDeparture::PassTo(id) => LeaveCost::HostPass {
                    to: s
                        .member(id)
                        .map(crate::screens::room::person_name)
                        .unwrap_or_default(),
                    match_running: running,
                },
                ddi_room::HostDeparture::Close if others => LeaveCost::HostClose {
                    match_running: running,
                },
                ddi_room::HostDeparture::Close => LeaveCost::HostAlone,
            });
        }
        if !running {
            return None;
        }
        let playing = me.intent == ddi_room::Intent::Play
            && s.running_match()
                .and_then(|m| m.roster.get(&self.me))
                .is_some_and(|seat| !seat.out_for_match);
        if playing {
            Some(LeaveCost::Playing)
        } else if s.picker() == Some(self.me) {
            Some(LeaveCost::Picking)
        } else if me.role == Role::CoHost {
            Some(LeaveCost::CoHost)
        } else {
            None
        }
    }

    /// Leave an online room: the room told (the member's row kept with
    /// its results, marked left), the socket closed when it answers. A
    /// socket down meanwhile (Reconnecting) is tried again at once and the
    /// Leave sent once it has the room (kept unanswered until then), so
    /// that the room is told rather than left to count the member away
    /// (review 2026-11-22, room-client 4): the shells keep driving the
    /// left client's link ([`Self::drive_leave`]) until then,
    /// [`LEAVE_WAIT_MS`] at most. The room's chat closes at once.
    pub fn leave(&mut self) {
        if !self.is_online() {
            return;
        }
        let now = self.now_ms();
        crate::state::twitch::keep_room_chat(None, now);
        self.stop_downloads();
        if self.left_at.is_some() {
            return;
        }
        if self.link_state().ended() {
            if let Some(l) = self.online_link_mut() {
                l.authority.shut();
            }
            return;
        }
        self.left_at = Some(now);
        if let Some(l) = self.online_link_mut() {
            l.hurry();
        }
        let _ = self.send(Command::Leave);
    }

    /// Whether this player left the online room and the room has not
    /// read it yet (nor was it given up: [`LEAVE_WAIT_MS`]): its link is
    /// still to be driven ([`Self::drive_leave`]).
    pub fn leaving(&self) -> bool {
        self.left_at.is_some() && !self.link_state().ended()
    }

    /// A left room's link driven alone (no screen shows the room any
    /// more): its answers and tickets read, its socket opened again and
    /// the Leave sent once it has the room; nothing else of the room is
    /// done. Whether it is still [`Self::leaving`].
    pub fn drive_leave(&mut self, now_ms: u64) -> bool {
        if now_ms > self.now_ms() {
            self.clock.set(now_ms);
        }
        if let Link::Online(l) = &mut self.link {
            l.tick(now_ms);
            let _ = ddi_room::RoomAuthority::poll(&mut l.authority);
        }
        self.leave_due()
    }

    /// A Leave the room has not read within [`LEAVE_WAIT_MS`] given up:
    /// the link shut. Whether it is still [`Self::leaving`].
    fn leave_due(&mut self) -> bool {
        let Some(at) = self.left_at else {
            return false;
        };
        if self.link_state().ended() {
            return false;
        }
        if self.now_ms() >= at.saturating_add(LEAVE_WAIT_MS) {
            if let Some(l) = self.online_link_mut() {
                l.authority.shut();
            }
            return false;
        }
        true
    }

    /// The local player.
    pub fn me(&self) -> ParticipantId {
        self.me
    }

    /// The local player's own name where one is known (round D, item 5):
    /// an online room's member's (their signed-in display name), or
    /// practice's as the shell set it while signed in. The lists show it
    /// with a "You" badge; where none is known, "You" alone.
    pub fn own_name(&self) -> Option<String> {
        if self.is_online() {
            return match &self.state().member(self.me)?.kind {
                MemberKind::Human { name, .. } if !name.is_empty() => Some(name.clone()),
                _ => None,
            };
        }
        self.own_name.clone().filter(|n| !n.is_empty())
    }

    /// Practice's: the player's signed-in display name (none signed out),
    /// set by the shell whenever it hands the room over.
    pub fn set_own_name(&mut self, name: Option<String>) {
        if self.own_name != name {
            self.own_name = name;
            self.version += 1;
        }
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn now_ms(&self) -> u64 {
        ddi_room::Clock::now(&self.clock).0
    }

    /// What the last refused request was refused for.
    pub fn refusal(&self) -> Option<&Refusal> {
        self.refusal.as_ref()
    }

    pub fn clear_refusal(&mut self) {
        if self.refusal.take().is_some() {
            self.version += 1;
        }
    }

    /// The library song and chart behind `content`.
    pub fn picked(&self, content: &ContentId) -> Option<&Picked> {
        self.picks.get(content)
    }

    /// An online room's reconnect that waits out its backoff tries now
    /// (`OnlineLink::hurry`: the shell's page shown again, or its socket
    /// closed where its timers cannot be trusted). Whether one waited.
    pub fn hurry(&mut self) -> bool {
        let hurried = self.online_link_mut().is_some_and(OnlineLink::hurry);
        if hurried {
            self.version += 1;
        }
        hurried
    }

    /// Time passes (milliseconds on the shell's clock, never backwards):
    /// the authority's deadlines and countdowns due by now happen.
    pub fn tick(&mut self, now_ms: u64) {
        if now_ms > self.now_ms() {
            self.clock.set(now_ms);
        }
        self.finish_twitch_trip();
        let before = self.link_state();
        if let Link::Online(l) = &mut self.link
            && l.tick(now_ms)
        {
            self.version += 1;
        }
        if self.left_at.is_some() {
            self.leave_due();
        }
        self.settle_twitch_pending();
        self.take_grants();
        self.read_inbox();
        self.retry_due();
        self.pump();
        // Connected since the last tick, whoever read it first (this
        // tick's poll, or a command's sent since).
        if self.link_seen != LinkState::Connected && self.link_state() == LinkState::Connected {
            self.connected_again();
        }
        self.tell_twitch_end();
        // The settings, read from storage at most once for the rest of
        // the tick (nothing it does changes what it reads of them).
        let settings = TickSettings::new();
        self.tell_chat_access(&settings);
        self.mark_twitch_used(&settings);
        // Round DE: an online room's chat kept connected between its
        // stages, while this device shows it (none in practice).
        if self.is_online() {
            crate::state::twitch::keep_room_chat(self.chat_kept_in(&settings).as_ref(), now_ms);
        }
        self.recheck_cleared();
        self.read_live();
        self.tell_spotlight();
        self.sync_preload(&settings);
        self.sync_choosing();
        self.sync_start();
        // A stage's prompt opens with no notice from before it (nor a
        // refusal of something done before it), and the room comes back
        // from a stage with none from it; joined to watch is said its
        // time from the connection, whatever opens meanwhile.
        let staged = self.prompt_stage();
        if staged != self.notice_stage {
            self.notice_stage = staged;
            if self
                .link_notice()
                .is_some_and(|n| n != LinkNotice::JoinedToWatch)
            {
                self.set_link_notice(None);
            }
            if self.refusal.take().is_some() {
                self.version += 1;
            }
        }
        if self.link_state() != before {
            self.version += 1;
        }
        self.link_seen = self.link_state();
    }

    /// The picker's song select rests on `song`'s `chart` (none: no
    /// song). Once it has rested [`SPOTLIGHT_REST_MS`], an online room is
    /// told (the others see and hear it, and it is shared ahead); moving
    /// on before then sends nothing. Practice has nobody to show it to.
    pub fn hover(&mut self, shown: Option<(&SongEntry, &ChartEntry)>) {
        let now = self.now_ms();
        let next = shown.map(|(song, chart)| {
            let id = content_id(song, chart);
            self.picks.entry(id.clone()).or_insert_with(|| Picked {
                song: song.clone(),
                chart: chart.clone(),
            });
            (id, card_of(song, chart))
        });
        self.rest_on(next, now);
    }

    /// The picker's song select rests on `next` (none: no song) from
    /// `now`: [`RoomClient::hover`]'s and [`RoomClient::hover_listed`]'s
    /// rest, its clock kept while it stays on the same content.
    fn rest_on(&mut self, next: Option<(ContentId, ContentCard)>, now: u64) {
        if next.is_none() {
            self.resting_on_none.get_or_insert(now);
        } else {
            self.resting_on_none = None;
        }
        if self.resting.as_ref().map(|(id, _, _)| id) != next.as_ref().map(|(id, _)| id) {
            self.resting = next.map(|(id, card)| (id, card, now));
        }
    }

    /// Tells the room what the picker's song select has rested on.
    fn tell_spotlight(&mut self) {
        if !self.is_online() || self.link_state() != LinkState::Connected || !self.chooses() {
            return;
        }
        // Resting on no song: the picker's spotlight cleared, once a rest.
        if let Some(since) = self.resting_on_none {
            if self.now_ms() >= since + SPOTLIGHT_REST_MS {
                self.resting_on_none = None;
            }
            if self.resting_on_none.is_none()
                && self.state().spotlight.as_ref().is_some_and(|s| s.by == self.me)
            {
                let _ = self.send(Command::Spotlight {
                    content: None,
                    card: None,
                });
            }
            return;
        }
        let Some((id, _, since)) = &self.resting else {
            return;
        };
        if self.now_ms() < since + SPOTLIGHT_REST_MS
            || id.chart.as_str().parse::<ddi_chart::ChartDigest>().is_err()
        {
            return;
        }
        let shown = self.state().spotlight.as_ref();
        if shown.is_some_and(|s| s.content == *id && s.by == self.me) {
            return;
        }
        // Once per rest: a refusal is not asked again until it moves.
        let Some((id, card, _)) = self.resting.take() else {
            return;
        };
        let _ = self.send(Command::Spotlight {
            content: Some(id),
            card: Some(Box::new(card)),
        });
    }

    /// The song the room's picker rests on, as this device can show it:
    /// its card, and the song and chart once read (its own library's, or
    /// the room's share of it).
    pub fn spotlight(&self) -> Option<(&ContentCard, Option<&Picked>)> {
        let s = self.state().spotlight.as_ref()?;
        let picked = self.picks.get(&s.content).or_else(|| {
            self.spotlight_loaded
                .as_ref()
                .filter(|(id, _)| *id == s.content)
                .map(|(_, p)| p)
        });
        Some((&s.card, picked))
    }

    /// What the room's screens should show now (M7b): the start prompt of
    /// the stage this player is in or may still join, else song select.
    pub fn view(&self) -> RoomView {
        // Gone for this player (round BM): no prompt of theirs; the room's
        // song select says so.
        if self.room_gone() {
            return RoomView::Select;
        }
        match self.prompt_stage() {
            Some(stage) => RoomView::Prompt(stage),
            None => RoomView::Select,
        }
    }

    /// The stage whose start prompt this player belongs on: gathering
    /// readiness after a pick (and they a candidate who has the song), or
    /// counting down or played with them in its lineup; and, where the
    /// stages are watched on this device ([`RoomClient::watches`]), one
    /// they are not in.
    fn prompt_stage(&self) -> Option<StageId> {
        let s = self.state();
        let stage = match s.phase {
            Phase::Preparing {
                stage,
                window: Some(_),
            }
            | Phase::Countdown { stage, .. }
            | Phase::Stage { stage } => stage,
            _ => return None,
        };
        let st = s.stage(stage)?;
        if !self.picks.contains_key(&st.content.as_ref()?.id) || self.left_prompts.contains(&stage) {
            return None;
        }
        // Its battle plan is not the room's here: it sits the stage out
        // (the room's song select says why), nothing to be ready for.
        if st.play.is_none() && self.load_problem(stage) == Some(LoadProblem::PlanMismatch) {
            return None;
        }
        // Its song just drawn by the group: the reveal first (revision 22).
        if st.play.is_none() && self.revealing(stage) {
            return None;
        }
        (self.in_stage(stage) || self.watches()).then_some(stage)
    }

    /// Whether the local player is in `stage` as a competitor: a candidate
    /// who wants to play while it gathers readiness, in its lineup once
    /// its start is committed.
    pub fn in_stage(&self, stage: StageId) -> bool {
        let s = self.state();
        let me = self.me;
        match s.stage(stage).and_then(|st| st.play.as_ref()) {
            // (Or not yet, but may still enter it late, round P1b.)
            Some(play) => play.lineup.contains(&me) || self.may_enter_late(stage),
            None => {
                s.candidates().contains(&me)
                    && s.member(me)
                        .is_some_and(|m| m.intent == ddi_room::Intent::Play)
            }
        }
    }

    /// Whether the local player, counted out of `stage`'s lineup when its
    /// start committed, may still enter it late (round P1b, the
    /// maintainer's of 2026-11-16: within the live rules' late join of its
    /// song's time zero; `ddi_room`'s `RoomState::late_entry_open`), their
    /// song loaded: their Ready screen stays theirs meanwhile, its Ready
    /// entering them. The room decides; this is what the screens offer.
    pub fn may_enter_late(&self, stage: StageId) -> bool {
        let s = self.state();
        let Some(st) = s.stage(stage) else {
            return false;
        };
        let Some(play) = st.play.as_ref() else {
            return false;
        };
        if play.lineup.contains(&self.me) || st.prep(self.me).loading != Loading::Loaded {
            return false;
        }
        let now = self.room_now().unwrap_or(ddi_room::Millis(self.now_ms()));
        s.late_entry_open(self.me, stage, now).is_ok()
    }

    /// Whether a stage the local player is not in is watched on this
    /// device (`watch-v0`): CPU practice, where the CPUs' plays are judged
    /// here and feed the spectator view; an online room that plays live,
    /// whose frames feed it (`docs/plans/multiplayer-online-play.md`,
    /// "Watching"). An online room that does not play live streams
    /// nothing: a watcher there stays on the room's song select.
    pub fn watches(&self) -> bool {
        self.judged_here_or_live()
    }

    /// Practice, which judges every play on this device, or an online
    /// room that plays live, whose room has the ledger and the streams:
    /// where watching, attacks and Adaptive charts are offered.
    fn judged_here_or_live(&self) -> bool {
        !self.is_online() || self.plays_live()
    }

    /// The local player's own choice, Play or Watch
    /// (`ddi_room::Intent`): Watch takes no seat, is ranked nowhere and
    /// needs no Ready; it holds across stages and matches until changed.
    pub fn my_intent(&self) -> ddi_room::Intent {
        self.member_of(self.me)
            .map_or(ddi_room::Intent::Play, |m| m.intent)
    }

    /// The local player chose Watch.
    pub fn watching(&self) -> bool {
        self.my_intent() == ddi_room::Intent::Watch
    }

    /// Whether the local player may change between Play and Watch now:
    /// outside a stage they are committed to (its countdown, its play).
    /// A change during a match is the room's rule for it: one of the
    /// match sits its next stages out (or comes back for them), one who
    /// was watching when it began waits for the next match.
    pub fn may_choose_intent(&self) -> bool {
        // Held to watching (the watch-only link): no Play to offer.
        if self.watch_only() && self.watching() {
            return false;
        }
        let s = self.state();
        match s.phase {
            Phase::Countdown { stage, .. }
            | Phase::Stage { stage }
            | Phase::SongCompletion { stage } => !self.in_stage(stage),
            _ => true,
        }
    }

    /// Play or Watch, the local player's own (`Command::SetIntent`).
    /// Nothing is sent where it is theirs already.
    pub fn set_intent(&mut self, intent: ddi_room::Intent) -> Result<(), Refusal> {
        if self.my_intent() == intent {
            return Ok(());
        }
        self.send(Command::SetIntent { intent })
    }

    /// Whether the room's stage `stage` has started with this player in it:
    /// its prompt starts the play now.
    pub fn stage_started(&self, stage: StageId) -> bool {
        let s = self.state();
        matches!(s.phase, Phase::Stage { stage: p } | Phase::SongCompletion { stage: p } if p == stage)
            && s.stage(stage)
                .and_then(|st| st.play.as_ref())
                .is_some_and(|p| {
                    p.lineup.contains(&self.me)
                        // One who may still enter it late keeps the Ready
                        // screen for those seconds (round P1b).
                        || (self.watches() && !self.may_enter_late(stage))
                })
    }

    /// What leaving stage `stage`'s prompt means now; withdrawing clears
    /// this player's Ready (and the picker's pick: the window closed, back
    /// to choosing; another operator following the picker only withdraws).
    pub fn leave_prompt(&mut self, stage: StageId) -> PromptExit {
        self.launched.remove(&stage);
        // Left, it stays left: never handed back while this window (or
        // this play) lasts; a new window opens it again.
        self.left_prompts.insert(stage);
        if self.stage_started(stage) {
            return PromptExit::Forfeit;
        }
        // Practice's start already committed (the countdown), left by a
        // watcher (the CPUs wait for nobody: no stage runs unwatched) or
        // by a player in it (their Ready is taken already: no stage
        // starts with them that nobody plays; review3 web-4): the start
        // canceled, back to choosing.
        if !self.is_online()
            && self.may(Capability::Advance)
            && matches!(self.state().phase, Phase::Countdown { stage: p, .. } if p == stage)
        {
            let _ = self.cancel_start();
        }
        if self.state().upcoming_stage() == Some(stage) {
            // Ready as asked: one pressed a moment ago, the room's answer
            // still to come, is taken back too (review 2026-11-22,
            // room-client 3).
            if self.ready_as_asked() {
                let _ = self.set_ready(false);
            }
            if self.chooses()
                && self.may(Capability::Advance)
                && matches!(
                    self.state().phase,
                    Phase::Preparing {
                        window: Some(_),
                        ..
                    }
                )
            {
                let _ = self.cancel_start();
            }
        }
        PromptExit::Withdraw
    }

    /// Whether the Rules pop-up has opened by itself since the player came
    /// into the room (M7b: once, until they leave and come back).
    pub fn rules_seen(&self) -> bool {
        self.rules_seen
    }

    pub fn set_rules_seen(&mut self, seen: bool) {
        self.rules_seen = seen;
    }

    /// Results the player has not been shown yet: a stage's (its number,
    /// from 1) or the match's (`None`), once confirmed. A match stopped
    /// (or restarted) before any of its stages was finished has none: its
    /// table would be zeros.
    pub fn unseen_results(&self) -> Option<Option<u16>> {
        if self.state().phase == Phase::Lobby {
            return None;
        }
        let (id, confirmed, over) = self.results_count()?;
        if over && confirmed == 0 {
            return None;
        }
        let seen = match self.results_seen {
            (Some(seen), n) if seen == id => n,
            _ => 0,
        };
        let total = confirmed + usize::from(over);
        if total <= seen {
            return None;
        }
        Some(if over {
            None
        } else {
            Some(u16::try_from(confirmed).unwrap_or(u16::MAX))
        })
    }

    /// The player was shown the results there are.
    pub fn results_shown(&mut self) {
        if let Some((id, confirmed, over)) = self.results_count() {
            self.results_seen = (Some(id), confirmed + usize::from(over));
        }
    }

    /// The running (or last) match's results there are: its id, its
    /// stages whose song ended, and whether it is over.
    fn results_count(&self) -> Option<(ddi_room::MatchId, usize, bool)> {
        let s = self.state();
        let m = s.current.as_ref()?;
        let confirmed = m
            .stages
            .iter()
            .filter(|st| st.play.as_ref().is_some_and(|p| p.song_ended))
            .count();
        let over = matches!(s.phase, Phase::MatchResults | Phase::Interrupted { .. });
        Some((m.id, confirmed, over))
    }

    /// The people who want to play now (at least one: the player).
    pub fn humans(&self) -> usize {
        self.state().reserved_humans().max(1)
    }

    /// The stages shown: the running (or just finished) match's, else the
    /// next match's playlist.
    pub fn stages(&self) -> &[Stage] {
        let s = self.state();
        match (&s.phase, &s.current) {
            (Phase::Lobby, _) | (_, None) => &s.playlist,
            (_, Some(m)) => &m.stages,
        }
    }

    /// The rules the shown stages play under: the running match's, locked,
    /// else the next match's.
    pub fn rules(&self) -> &MatchRules {
        let s = self.state();
        match (&s.phase, &s.current) {
            (Phase::Lobby, _) | (_, None) => &s.next.rules,
            (_, Some(m)) => &m.rules,
        }
    }

    /// The running (or just finished) match's locked rules.
    pub fn locked_rules(&self) -> Option<&MatchRules> {
        let s = self.state();
        s.current
            .as_ref()
            .filter(|_| s.phase != Phase::Lobby)
            .map(|m| &m.rules)
    }

    /// Whether `chart` may be picked here: the CPUs can play it (practice),
    /// or every member can check it by its digest (online).
    pub fn pickable(&self, chart: &ChartEntry) -> Result<(), Unsupported> {
        if self.is_online() {
            // A hosted room's content must be named by the chart's digest,
            // which every participant can check; where its match has CPUs
            // (`ddi_room::live::cpus`), they must be able to play it.
            if chart.digest.is_none() {
                return Err(Unsupported::NoDigest);
            }
            if self.has_cpus() {
                return eligible(chart);
            }
            Ok(())
        } else {
            eligible(chart)
        }
    }

    /// The match the room plays (its running one, else the next) seats
    /// CPUs.
    pub fn has_cpus(&self) -> bool {
        let rules = self.rules();
        rules.cpu_seats(self.state().reserved_humans()) > 0
    }

    /// The room can seat CPUs: practice, or an online room that plays
    /// live (`RoomPolicy::seats_cpus`: its devices compute them alike and
    /// report for them).
    pub fn cpus_offered(&self) -> bool {
        let s = self.state();
        s.policy.seats_cpus(s.source)
    }

    /// Whether the room can play charts of style `style` at all: CPUs
    /// play one-player charts with four lanes ([`eligible`]); an online
    /// room without them plays any.
    pub fn style_playable(&self, style: &str) -> bool {
        (self.is_online() && !self.has_cpus()) || cpu_style(style)
    }

    /// The picker's pick (M7b): `song`'s `chart` becomes the song of the
    /// stage the room plays next, and the match starts (or goes on); the
    /// stage's readiness gathers on its start prompt. Refused when the
    /// CPUs cannot play it, or when the room refuses.
    pub fn pick(&mut self, song: &SongEntry, chart: &ChartEntry) -> Result<(), Refusal> {
        if let Err(why) = self.pickable(chart) {
            return self.refuse(Refusal::Chart(why));
        }
        let id = content_id(song, chart);
        self.picks.insert(
            id.clone(),
            Picked {
                song: song.clone(),
                chart: chart.clone(),
            },
        );
        // The stage the pick is for, before the room moves on.
        let stage = self.stage_picked_next();
        self.send(Command::Pick {
            content: id,
            card: Some(Box::new(card_of(song, chart))),
        })?;
        // Made the open-ended match's last song (More, round AA): told
        // with the pick, as its own command.
        if std::mem::take(&mut self.pick_last)
            && let Some(stage) = stage
        {
            self.send(Command::SetLastStage { stage, last: true })?;
        }
        Ok(())
    }

    // ---- the room's song list (revision 17) ----

    /// Whether the room has a song list (an online room; CPU practice
    /// picks from the player's own library).
    pub fn has_song_list(&self) -> bool {
        self.is_online()
    }

    /// The room's songs, oldest first.
    pub fn room_songs(&self) -> &[RoomSong] {
        &self.state().songs.entries
    }

    /// The list's rules (its size, what happens when it is full).
    pub fn song_list_rules(&self) -> ddi_room::ListRules {
        self.state().songs.rules
    }

    /// This player's library, as the room's screen has it: what a listed
    /// song this device has is uploaded and loaded from (keyed by each
    /// song's content, [`song_key`]).
    pub fn set_library(&mut self, songs: &[SongEntry]) {
        self.library = songs
            .iter()
            .filter_map(|s| Some((song_key(s)?, s.clone())))
            .collect();
    }

    /// The library song that is listed entry `entry`, where this device
    /// has it.
    pub fn library_song(&self, entry: &RoomSong) -> Option<&SongEntry> {
        self.library.get(entry.key.as_str())
    }

    /// The entry of the room's list that is library song `song`, if listed.
    pub fn listed(&self, song: &SongEntry) -> Option<&RoomSong> {
        let key = song_key(song)?;
        self.state().songs.by_key(&SongKey::new(key))
    }

    /// Whether this player may add songs to the room's list now.
    pub fn may_add(&self) -> bool {
        self.has_song_list() && self.state().may_add(self.me)
    }

    /// Who may add songs to the room's list (revision 21): the owner's
    /// setting, Hosts until they open it to everyone.
    pub fn who_adds(&self) -> ddi_room::WhoAdds {
        self.state().songs.who_adds
    }

    /// The owner's Who can add songs (Host options).
    pub fn set_who_adds(&mut self, who: ddi_room::WhoAdds) -> Result<(), Refusal> {
        self.send(Command::SetWhoAdds { who })
    }

    /// What would stop this player adding a song now, where they may add
    /// at all (revision 21, `RoomState::add_refusal`): their half of the
    /// list reached, or a list full that refuses or has nothing it may
    /// remove. None where an addition would be taken (or they may not add:
    /// [`Self::may_add`] says that).
    pub fn add_blocked(&self) -> Option<Rejection> {
        if !self.may_add() {
            return None;
        }
        self.state().add_refusal(self.me, None)
    }

    /// How many songs this player added that are listed, and how many one
    /// who is not a host may (half the list's size); none for a host, a
    /// co-host or an administrator, who may fill the whole list, and none
    /// where this player may not add.
    pub fn my_song_count(&self) -> Option<(usize, usize)> {
        if !self.may_add() {
            return None;
        }
        let s = self.state();
        s.song_quota(self.me).map(|q| (s.songs.added_by(self.me), q))
    }

    /// Whether this player may remove entry `song`.
    pub fn may_remove(&self, song: RoomSongId) -> bool {
        self.state()
            .songs
            .get(song)
            .is_some_and(|e| self.state().may_remove(self.me, e) && !self.state().song_in_use(e))
    }

    /// Add library song `song` to the room's list: refused where this
    /// player may not, where a chart's digest is not known yet, or as the
    /// room refuses (the same song listed, the list full, their share of
    /// it). Its upload follows once the room lists it.
    pub fn add_song(&mut self, song: &SongEntry) -> Result<(), Refusal> {
        if !self.may_add() {
            return self.refuse(Refusal::Room(Rejection::Unauthorized {
                capability: Capability::AddSongs,
            }));
        }
        let Some(new) = new_song(song) else {
            return self.refuse(Refusal::Chart(Unsupported::NoDigest));
        };
        // The room's own decision asked first (the same song, this
        // player's half, a full list), so the refusal is said at the
        // press rather than when the room answers.
        if let Some(why) = self.state().add_refusal(self.me, Some(&new)) {
            return self.refuse(Refusal::Room(why));
        }
        self.library.insert(new.key.as_str().to_string(), song.clone());
        self.send(Command::AddSong {
            song: Box::new(new),
        })
    }

    /// The songs of `songs` (a pack of this player's library) that could
    /// be added to the room's list now: not listed yet, their charts'
    /// digests known, as many as this player may still add (half the
    /// list's size) and, where the list refuses new songs when full, as it
    /// has room for.
    pub fn pack_addable<'a>(&self, songs: &'a [SongEntry]) -> Vec<&'a SongEntry> {
        if !self.may_add() {
            return Vec::new();
        }
        let s = self.state();
        let rules = s.songs.rules;
        let mut room_left = s
            .song_quota(self.me)
            .map_or(usize::MAX, |q| q.saturating_sub(s.songs.added_by(self.me)))
            .min(usize::from(rules.size));
        if rules.when_full == ddi_room::WhenFull::Refuse {
            room_left = room_left.min(usize::from(rules.size).saturating_sub(s.songs.len()));
        }
        let mut seen = BTreeSet::new();
        songs
            .iter()
            .filter(|song| {
                new_song(song).is_some_and(|n| {
                    s.songs.by_key(&n.key).is_none() && seen.insert(n.key.as_str().to_string())
                })
            })
            .take(room_left)
            .collect()
    }

    /// More's Add this pack (on My songs): each of `songs` that may be
    /// added ([`Self::pack_addable`]) added to the room's list, uploaded
    /// one at a time as any added song; how many. Its progress is
    /// [`Self::pack_progress`].
    pub fn add_pack(&mut self, songs: &[SongEntry]) -> Result<usize, Refusal> {
        let adding: Vec<SongEntry> = self.pack_addable(songs).into_iter().cloned().collect();
        let mut added = 0;
        for song in &adding {
            if let Some(n) = new_song(song) {
                self.add_song(song)?;
                self.pack_keys.push(n.key);
                added += 1;
            }
        }
        Ok(added)
    }

    /// A pack being added: its songs uploaded so far of those still in the
    /// room's list, while any is not (none after).
    pub fn pack_progress(&self) -> Option<(usize, usize)> {
        let entries: Vec<&RoomSong> = self
            .pack_keys
            .iter()
            .filter_map(|k| self.state().songs.by_key(k))
            .collect();
        let done = entries.iter().filter(|e| e.available()).count();
        (!entries.is_empty() && done < entries.len()).then_some((done, entries.len()))
    }

    /// Remove entry `song` from the room's list.
    pub fn remove_song(&mut self, song: RoomSongId) -> Result<(), Refusal> {
        self.send(Command::RemoveSong { song })
    }

    /// The list's size and what happens when it is full (the owner's).
    pub fn set_song_list(&mut self, rules: ddi_room::ListRules) -> Result<(), Refusal> {
        self.send(Command::SetSongList { rules })
    }

    /// What entry `song` is for this player now.
    pub fn entry_state(&self, song: &RoomSong) -> EntryState {
        match (&song.status, self.song_sharing.get(&song.id)) {
            (ContentStatus::Pending, Some(Sharing::Uploading { done, total })) => {
                return EntryState::Uploading {
                    done: *done,
                    total: *total,
                };
            }
            (ContentStatus::Pending, Some(Sharing::Failed(_))) => return EntryState::Failed,
            (ContentStatus::Pending, _) => {
                return EntryState::Uploading {
                    done: 0.0,
                    total: 0.0,
                };
            }
            (ContentStatus::Unavailable { .. }, _) => return EntryState::Failed,
            (ContentStatus::Available, _) => {}
        }
        if self.library_song(song).is_some()
            || self.ahead_here(song)
            || self.picks.keys().any(|c| c.song == song.key)
        {
            return EntryState::Ready;
        }
        let loading = self.stages().iter().any(|st| {
            st.play.is_none()
                && st.content.as_ref().is_some_and(|c| c.id.song == song.key)
                && st.prep(self.me).loading.in_progress()
                && st.prep(self.me).loading != Loading::NotStarted
        });
        if loading {
            EntryState::Loading
        } else if let Some((done, total)) = self.ahead_progress(song) {
            EntryState::Downloading { done, total }
        } else {
            EntryState::NotLoaded
        }
    }

    /// Whether this device's upload of entry `song` failed and may be
    /// tried again.
    pub fn may_retry_song(&self, song: RoomSongId) -> bool {
        matches!(self.song_sharing.get(&song), Some(Sharing::Failed(_)))
            && self
                .state()
                .songs
                .get(song)
                .is_some_and(|e| e.status == ContentStatus::Pending)
    }

    /// Upload entry `song` again after a failure: a new grant.
    pub fn retry_song_upload(&mut self, song: RoomSongId) {
        if self.may_retry_song(song) {
            self.song_sharing.remove(&song);
            self.version += 1;
            self.sync_song_uploads();
        }
    }

    /// The picker's pick of chart `chart` of listed song `song` (the Room
    /// songs tab's Enter): the song of the stage the room plays next, there
    /// at once (it is uploaded); refused where the CPUs cannot play it, or
    /// as the room refuses.
    pub fn pick_listed(&mut self, song: RoomSongId, chart: &ChartKey) -> Result<(), Refusal> {
        let Some(entry) = self.state().songs.get(song).cloned() else {
            return self.refuse(Refusal::Room(Rejection::NotListed));
        };
        let Some(line) = entry.chart(chart) else {
            return self.refuse(Refusal::Room(Rejection::ChartNotInSong));
        };
        if self.has_cpus() && !cpu_style(&line.style) {
            return self.refuse(Refusal::Chart(Unsupported::Layout {
                style: line.style.clone(),
            }));
        }
        let content = ContentId {
            song: entry.key.clone(),
            chart: chart.clone(),
        };
        self.map_listed_content(&content);
        let stage = self.stage_picked_next();
        self.send(Command::PickListed {
            song,
            chart: chart.clone(),
        })?;
        if std::mem::take(&mut self.pick_last)
            && let Some(stage) = stage
        {
            self.send(Command::SetLastStage { stage, last: true })?;
        }
        Ok(())
    }

    /// The picker's song select rests on chart `chart` of listed song
    /// `song` (none: on no song): once it has rested
    /// [`SPOTLIGHT_REST_MS`], the room marks it for the others (its
    /// entry's row; nothing is uploaded or downloaded for it).
    pub fn hover_listed(&mut self, shown: Option<(RoomSongId, &ChartKey)>) {
        let now = self.now_ms();
        let next = shown.and_then(|(song, chart)| {
            let e = self.state().songs.get(song)?;
            let line = e.chart(chart)?;
            let id = ContentId {
                song: e.key.clone(),
                chart: chart.clone(),
            };
            Some((id, e.card_for(line)))
        });
        self.rest_on(next, now);
    }

    /// The entry the room's picker rests on (the spotlight's mark), if one.
    pub fn spotlight_entry(&self) -> Option<RoomSongId> {
        let spot = self.state().spotlight.as_ref()?;
        self.state().songs.by_key(&spot.content.song).map(|e| e.id)
    }

    /// The song and chart of the library this device loads a listed
    /// content from, where it has the song: made known as a pick of its
    /// own (so the stage is prepared from the library, nothing read from
    /// the share).
    fn map_listed_content(&mut self, content: &ContentId) {
        if self.picks.contains_key(content) {
            return;
        }
        let Some(song) = self.library.get(content.song.as_str()) else {
            return;
        };
        let Some(chart) = song
            .charts
            .iter()
            .find(|c| c.digest.is_some_and(|d| d.to_string() == content.chart.as_str()))
        else {
            return;
        };
        let picked = Picked {
            song: song.clone(),
            chart: chart.clone(),
        };
        self.picks.insert(content.clone(), picked);
    }

    /// Every stage's listed content this device has in its library, made
    /// known as its own ([`Self::map_listed_content`]).
    fn map_listed(&mut self) {
        if self.library.is_empty() {
            return;
        }
        let contents: Vec<ContentId> = self
            .stages()
            .iter()
            .filter_map(|st| st.content.as_ref())
            .filter(|c| c.song.is_some())
            .map(|c| c.id.clone())
            .collect();
        for c in contents {
            self.map_listed_content(&c);
        }
    }

    /// The songs this player added whose upload is not done: one asked
    /// for at a time (its grant, then the shell's upload), from the
    /// library; what is no longer waiting for an upload forgotten.
    fn sync_song_uploads(&mut self) {
        let me = self.me;
        let waiting: BTreeSet<RoomSongId> = self
            .state()
            .songs
            .entries
            .iter()
            .filter(|e| e.by == me && e.status == ContentStatus::Pending)
            .map(|e| e.id)
            .collect();
        let before = self.song_sharing.len();
        self.song_sharing.retain(|id, _| waiting.contains(id));
        if self.song_sharing.len() != before {
            self.version += 1;
        }
        if self.link_state() != LinkState::Connected
            || self
                .song_sharing
                .values()
                .any(|s| matches!(s, Sharing::Asking | Sharing::Uploading { .. }))
        {
            return;
        }
        let next = self.state().songs.entries.iter().find(|e| {
            waiting.contains(&e.id)
                && !self.song_sharing.contains_key(&e.id)
                && self.library.contains_key(e.key.as_str())
        });
        let Some(song) = next.map(|e| e.id) else {
            return;
        };
        self.song_sharing.insert(song, Sharing::Asking);
        if let Some(l) = self.online_link_mut() {
            l.request_song_grant(song);
        }
        self.version += 1;
    }

    /// The room's answers to the songs' grants: each upload handed to the
    /// shell, or the refusal kept (Retry offered).
    fn take_song_grants(&mut self) {
        let news = match self.online_link_mut() {
            Some(l) => l.take_song_grants(),
            None => return,
        };
        for n in news {
            if !matches!(self.song_sharing.get(&n.song), Some(Sharing::Asking)) {
                continue;
            }
            let entry = self.state().songs.get(n.song).cloned();
            let library = entry.as_ref().and_then(|e| self.library_song(e).cloned());
            let next = match (n.answer, entry, library) {
                (Ok(answer), Some(entry), Some(song)) => match answer.grant {
                    Some(grant) => {
                        let chart = song.charts.first().cloned().unwrap_or_default();
                        let content = ContentId {
                            song: entry.key.clone(),
                            chart: entry
                                .charts
                                .first()
                                .map(|c| c.chart.clone())
                                .unwrap_or_else(|| ChartKey::new("")),
                        };
                        self.uploads.push(UploadJob {
                            stage: SPOTLIGHT,
                            content,
                            song,
                            chart,
                            grant,
                            max_bytes: answer.max_bytes,
                            entry: Some(n.song),
                        });
                        Sharing::Uploading {
                            done: 0.0,
                            total: 0.0,
                        }
                    }
                    None => Sharing::Failed(SharingProblem::Grant("no_grant".into())),
                },
                (Err(p), _, _) => Sharing::Failed(SharingProblem::Grant(p.code)),
                _ => Sharing::Failed(SharingProblem::Grant("no_song".into())),
            };
            self.song_sharing.insert(n.song, next);
            self.version += 1;
        }
    }

    /// The upload of listed song `song` moved on: `done` bytes of `total`.
    pub fn song_upload_progress(&mut self, song: RoomSongId, done: f64, total: f64) {
        if let Some(s @ Sharing::Uploading { .. }) = self.song_sharing.get_mut(&song) {
            *s = Sharing::Uploading { done, total };
            self.version += 1;
        }
    }

    /// The upload of listed song `song` ended: the share made (the room
    /// hears it from the server and lists it as there), or why not. An
    /// expired grant is asked for again and the upload made again.
    pub fn song_upload_finished(&mut self, song: RoomSongId, result: Result<String, ShareError>) {
        if !matches!(self.song_sharing.get(&song), Some(Sharing::Uploading { .. })) {
            return;
        }
        let next = match result {
            Ok(_) => Sharing::Sent,
            Err(e) if ddi_library::share::grant_expired(&e) => {
                if let Some(l) = self.online_link_mut() {
                    l.request_song_grant(song);
                }
                Sharing::Asking
            }
            Err(e) => Sharing::Failed(SharingProblem::Upload(e)),
        };
        self.song_sharing.insert(song, next);
        self.version += 1;
    }

    /// The stage a pick now is for: the intermission's next, a stage
    /// prepared without its window, or the next match's first.
    fn stage_picked_next(&self) -> Option<StageId> {
        let s = self.state();
        match s.phase {
            Phase::Intermission { next } => Some(next),
            Phase::Preparing { stage, .. } => Some(stage),
            Phase::Lobby | Phase::MatchResults | Phase::Interrupted { .. } => {
                s.playlist.first().map(|p| p.id)
            }
            _ => None,
        }
    }

    /// Whether `stage` is an open-ended match's last song (picked so, or
    /// the room's last by its bound; round AA): its status line, Ready
    /// card and prompt say so.
    pub fn is_last_song(&self, stage: StageId) -> bool {
        let s = self.state();
        s.current
            .as_ref()
            .is_some_and(|m| m.rules.open_ended && m.is_last(stage, s.policy.max_stages))
    }

    /// What More offers of an open-ended match's last song now (round
    /// AA): for the stage picked and not started, made the last song or
    /// not (an operator's, until its start); before a pick, the next pick
    /// made it (the picker's).
    pub fn last_song_choice(&self) -> Option<LastSongChoice> {
        let s = self.state();
        if let Some(m) = s.running_match().filter(|m| m.rules.open_ended)
            && let Phase::Preparing {
                stage,
                window: Some(_),
            } = s.phase
            && let Some(st) = m.stage(stage).filter(|st| st.play.is_none())
        {
            return self
                .may(Capability::Advance)
                .then_some(LastSongChoice::Picked { stage, last: st.last });
        }
        let open = match s.running_match() {
            Some(m) => m.rules.open_ended,
            None => s.next.rules.open_ended,
        };
        let before_pick = matches!(
            s.phase,
            Phase::Lobby
                | Phase::Intermission { .. }
                | Phase::Preparing { window: None, .. }
                | Phase::MatchResults
                | Phase::Interrupted { .. }
        );
        (open && before_pick && self.chooses()).then_some(LastSongChoice::NextPick {
            last: self.pick_last,
        })
    }

    /// What the stage `stage`'s Ready card shows of an open-ended match's
    /// last song (round AC): whether it is marked, and whether this
    /// player may change it now ([`RoomClient::last_song_choice`]: the
    /// same capability as More's item, until the stage's start commits).
    /// `None` where the card has no such control: a counted match, a
    /// stage that is the last by the room's bound alone, a player who may
    /// not mark it.
    pub fn last_song_mark(&self, stage: StageId) -> Option<(bool, bool)> {
        let s = self.state();
        let m = s.running_match().filter(|m| m.rules.open_ended)?;
        // Kept through the countdown (unchangeable then), so that the
        // card's rows stay where they were.
        let st = m.stage(stage)?;
        if !self.may(Capability::Advance) {
            return None;
        }
        // The room's bound makes it the last by itself: nothing to mark.
        if !st.last && m.is_last(stage, s.policy.max_stages) {
            return None;
        }
        let changeable = matches!(
            self.last_song_choice(),
            Some(LastSongChoice::Picked { stage: p, .. }) if p == stage
        );
        Some((st.last, changeable))
    }

    /// More's last-song item pressed ([`RoomClient::last_song_choice`]).
    pub fn toggle_last_song(&mut self) {
        match self.last_song_choice() {
            Some(LastSongChoice::Picked { stage, last }) => {
                let _ = self.send(Command::SetLastStage { stage, last: !last });
            }
            Some(LastSongChoice::NextPick { last }) => {
                self.pick_last = !last;
                self.version += 1;
            }
            None => {}
        }
    }

    /// End the match now may be asked (round AA): a running open-ended
    /// match between stages, a stage of it settled, by an operator.
    pub fn may_end_match(&self) -> bool {
        let s = self.state();
        let Some(m) = s.running_match().filter(|m| m.rules.open_ended) else {
            return false;
        };
        let between = match s.phase {
            Phase::Intermission { .. } => true,
            Phase::Preparing { stage, .. } => m.stage(stage).is_some_and(|st| st.play.is_none()),
            _ => false,
        };
        between && self.stages_played() > 0 && self.may(Capability::Advance)
    }

    /// The running match's stages settled so far.
    pub fn stages_played(&self) -> usize {
        self.state().current.as_ref().map_or(0, |m| {
            m.stages
                .iter()
                .filter(|st| st.play.as_ref().is_some_and(|p| p.result.is_some()))
                .count()
        })
    }

    /// More's End the match now: its question asked over the screen.
    pub fn ask_end_match(&self) {
        if self.may_end_match() {
            self.asking_end.set(true);
        }
    }

    /// The End the match now question is to be asked now (once).
    pub fn take_end_question(&self) -> bool {
        self.asking_end.replace(false)
    }

    /// End the open-ended match now (its question answered).
    pub fn end_match(&mut self) {
        let _ = self.send(Command::EndMatch);
    }

    /// The stage the next pick is for: its number in the match (from 1),
    /// a new match's first after a match's results.
    pub fn next_pick_number(&self) -> u16 {
        let s = self.state();
        let n = match (&s.phase, &s.current) {
            (Phase::Lobby | Phase::MatchResults | Phase::Interrupted { .. }, _) | (_, None) => 0,
            (_, Some(m)) => s
                .upcoming_stage()
                .and_then(|id| m.stage_index(id))
                .unwrap_or(0),
        };
        u16::try_from(n + 1).unwrap_or(u16::MAX)
    }

    /// Declare (or withdraw) readiness for the upcoming stage. Never
    /// automatic: only the player's own press asks.
    pub fn set_ready(&mut self, ready: bool) -> Result<(), Refusal> {
        // During the countdown to the start, Ready taken back is the
        // room's to decide (CPU practice cancels the start, round P1b; an
        // online room refuses it): asked of the counting-down stage.
        let counting = match self.state().phase {
            Phase::Countdown { stage, .. } if !ready => Some(stage),
            // A late entry (round P1b): Ready after the start committed.
            Phase::Countdown { stage, .. } | Phase::Stage { stage }
                if ready && self.may_enter_late(stage) =>
            {
                Some(stage)
            }
            _ => None,
        };
        let Some(stage) = counting.or_else(|| self.state().upcoming_stage()) else {
            return self.refuse(Refusal::Room(Rejection::NotUpcoming));
        };
        let readiness_rev = self.readiness_rev(stage);
        let sent = self.send(Command::SetReady {
            stage,
            readiness_rev,
            ready,
        });
        // Online, the room answers later: until it does, what was asked
        // is what a press after this one acts on (`ready_as_asked`).
        self.ready_asked = match (&sent, self.unanswered) {
            (Ok(()), Some(seq)) => Some((seq, ready)),
            _ => None,
        };
        sent
    }

    /// Whether the local player is ready as they last asked the room: what
    /// was asked while the room has not answered it yet (an online room's
    /// round trip), else [`Self::ready`]. A stage's prompt reads it, so that
    /// a press right after Ready is the first of taking it back, and one
    /// right after taking it back readies again, however late the room's
    /// answer (found 2026-11-16, round P1b; fixed 2026-11-18).
    pub fn ready_as_asked(&self) -> bool {
        match self.ready_asked {
            Some((_, ready)) => ready,
            None => self.ready(),
        }
    }

    /// How `id` picks whom to attack (M8 round 3): a person's own choice,
    /// a CPU's as its roster entry names it; Spread for anyone unknown.
    pub fn targeting_of(&self, id: ParticipantId) -> Targeting {
        self.member_of(id)
            .map_or_else(Targeting::default, ddi_room::Member::targeting)
    }

    /// The local player's attack policy.
    pub fn my_targeting(&self) -> Targeting {
        self.targeting_of(self.me)
    }

    /// The local player's attack policy, chosen outside a stage's play
    /// (the players panel, a stage's prompt): the next stage's battle
    /// starts them with it. Nothing is sent where it is theirs already.
    pub fn set_targeting(&mut self, targeting: Targeting) -> Result<(), Refusal> {
        if self.my_targeting() == targeting {
            return Ok(());
        }
        self.send(Command::SetTargeting { targeting })
    }

    /// The next policy in the cycle (Spread, Retaliate, Leader): what a
    /// press on the prompt's line does.
    pub fn cycle_targeting(&mut self) -> Result<(), Refusal> {
        let all = Targeting::ALL;
        let i = all
            .iter()
            .position(|t| *t == self.my_targeting())
            .unwrap_or(0);
        self.set_targeting(all[(i + 1) % all.len()])
    }

    /// The policy a stage's play left the local player with (changed in
    /// play or not, `ddi_session::battle::StageOutcome::targeting`): kept
    /// as theirs for the next stage, once the room is past the stage. A
    /// refusal is not shown (the stage's outcome was reported already).
    pub fn played_targeting(&mut self, targeting: Targeting) {
        if self.my_targeting() != targeting {
            let kept = self.refusal.clone();
            let _ = self.send(Command::SetTargeting { targeting });
            self.refusal = kept;
        }
    }

    /// Whether attacks can be played in this room: CPU practice, where
    /// its ledger runs on this device and the shocks go into the plays
    /// judged here; an online room that plays live, where the room runs
    /// the ledger on everyone's streamed reports
    /// (`docs/plans/multiplayer-online-play.md`). An online room that does
    /// not (a server from before live play) has neither, whatever its
    /// rules say.
    pub fn attacks_offered(&self) -> bool {
        self.judged_here_or_live()
    }

    /// Whether a match under `rules` is really played with attacks here:
    /// its Attacks rule, where attacks are offered. What the stage's
    /// battle, its standings and every summary follow, never a rule that
    /// does nothing.
    pub fn attacks_played(&self, rules: &MatchRules) -> bool {
        self.attacks_offered() && rules.attacks()
    }

    /// Whether the match being played (else the next one's rules) is
    /// played with attacks: where the screens show the policies.
    pub fn attacks_on(&self) -> bool {
        match self.rules_now() {
            Some(r) => self.attacks_played(&r),
            None => self.attacks_played(self.rules()),
        }
    }

    /// The rules of the stage the running match is on (gathering,
    /// counting down, playing, or next after an intermission), which a
    /// change during the match may have made other than the match's.
    pub fn rules_now(&self) -> Option<MatchRules> {
        let s = self.state();
        let stage = match s.phase {
            Phase::Preparing { stage, .. }
            | Phase::Countdown { stage, .. }
            | Phase::Stage { stage }
            | Phase::SongCompletion { stage } => stage,
            Phase::Intermission { next } => next,
            _ => return None,
        };
        s.current.as_ref().map(|m| m.rules_for(stage))
    }

    /// Whether the local player is ready for the upcoming stage (or in
    /// the lineup of the stage counting down).
    pub fn ready(&self) -> bool {
        let s = self.state();
        if let Phase::Countdown { stage, .. } = s.phase {
            return s
                .stage(stage)
                .and_then(|st| st.play.as_ref())
                .is_some_and(|p| p.lineup.contains(&self.me));
        }
        s.upcoming_stage()
            .and_then(|id| s.stage(id))
            .is_some_and(|stage| stage.prep(self.me).ready)
    }

    /// Whether the local player, ready, may take Ready back now: before
    /// the start commits, and during its countdown where the room lets
    /// that cancel it (CPU practice, `unready_cancels_countdown`, round
    /// P1b). An online room's committed start is past taking back: its
    /// prompt says the stage is starting instead.
    pub fn may_take_ready_back(&self) -> bool {
        if !self.ready_as_asked() {
            return false;
        }
        let s = self.state();
        !matches!(s.phase, Phase::Countdown { .. }) || s.policy.readiness.unready_cancels_countdown
    }

    /// Why the local player's preparation of `stage` failed, if it did.
    pub fn load_problem(&self, stage: StageId) -> Option<LoadProblem> {
        match &self.state().stage(stage)?.prep(self.me).loading {
            Loading::Failed { code } => {
                Some(LoadProblem::from_code(code).unwrap_or(LoadProblem::Unreadable))
            }
            _ => None,
        }
    }

    /// The local player's preparation of `stage` is under way.
    pub fn loading(&self, stage: StageId) -> bool {
        self.state().stage(stage).is_some_and(|s| {
            s.prep(self.me).loading.in_progress() && s
                .content
                .as_ref()
                .is_some_and(|c| c.status == ContentStatus::Available)
        })
    }

    /// The songs to prepare, once each (the shell's to answer).
    pub fn take_load_jobs(&mut self) -> Vec<LoadJob> {
        std::mem::take(&mut self.jobs)
    }

    /// How far the shell is with `job`'s song (`done` of `total`, in
    /// whatever it counts: a download's bytes): the room is told in steps
    /// of a tenth, so that everyone's lists say "Loading 40%". Nothing for
    /// the song the picker rests on, a song replaced since, a stage that
    /// started, or a whole not known.
    pub fn load_progress(&mut self, job: &LoadJob, done: f64, total: f64) {
        if job.stage == SPOTLIGHT
            || total <= 0.0
            || self.readiness_rev(job.stage) != job.readiness_rev
            || self.state().stage(job.stage).is_none_or(|st| st.play.is_some())
        {
            return;
        }
        // (Never 1000: Loaded says that, once the song is prepared.)
        let tenth = ((done / total).clamp(0.0, 0.99) * 10.0) as u16;
        let key = (job.stage, job.readiness_rev);
        if tenth == 0 || self.load_said.get(&key).is_some_and(|said| *said >= tenth) {
            return;
        }
        self.load_said.insert(key, tenth);
        let _ = self.send(Command::ReportLoading {
            stage: job.stage,
            readiness_rev: job.readiness_rev,
            loading: Loading::Loading {
                permille: tenth * 100,
            },
        });
    }

    /// The shell prepared `job`: the stage's song reported loaded, or
    /// failed with why. An answer for a song replaced since is dropped
    /// (the new one has its own job).
    pub fn finish_load(&mut self, job: &LoadJob, result: Result<(), LoadProblem>) {
        if self.readiness_rev(job.stage) != job.readiness_rev {
            return;
        }
        // A download worth trying again (round P1b): later, the room
        // still told "loading".
        if result == Err(LoadProblem::Download) && self.retry_later(job) {
            return;
        }
        self.preparing.remove(&job.stage);
        // (Its load is over: nothing more is said of how far it is.)
        self.load_said
            .insert((job.stage, job.readiness_rev), u16::MAX);
        // A stage that started meanwhile takes no report (the room would
        // refuse it, and the status line say so): its song is here to
        // watch it by ([`Self::sync_watched`]); unless this member may
        // still load it to enter it late (round P1b).
        let now = self.room_now().unwrap_or(ddi_room::Millis(self.now_ms()));
        if self.state().stage(job.stage).is_some_and(|st| st.play.is_some())
            && self.state().late_entry_open(self.me, job.stage, now).is_err()
        {
            self.version += 1;
            return;
        }
        let loading = match result {
            Ok(()) => Loading::Loaded,
            Err(p) => Loading::Failed {
                code: p.code().into(),
            },
        };
        let _ = self.send(Command::ReportLoading {
            stage: job.stage,
            readiness_rev: job.readiness_rev,
            loading,
        });
    }

    /// A failed download of `job`'s song: tried again after
    /// [`LOAD_RETRY_V0_MS`]'s next wait, where some are left (whether it
    /// will be); spent, the job is kept for Retry
    /// ([`Self::retry_load`]).
    fn retry_later(&mut self, job: &LoadJob) -> bool {
        if job.share.is_none()
            || job.stage == SPOTLIGHT
            || self.state().stage(job.stage).is_none_or(|st| st.play.is_some())
        {
            return false;
        }
        let tries = self.load_tries.entry((job.stage, job.readiness_rev)).or_default();
        match LOAD_RETRY_V0_MS.get(*tries as usize) {
            Some(wait) => {
                *tries += 1;
                let due = self.now_ms() + wait;
                self.preparing.remove(&job.stage);
                self.retrying.push((due, job.clone()));
                self.version += 1;
                let _ = self.say_retrying(job);
                true
            }
            None => {
                self.failed_jobs.insert(job.stage, job.clone());
                false
            }
        }
    }

    /// The room told `job`'s download is being tried again (revision 16,
    /// `Loading::Retrying`: the readiness window's early end waits for it
    /// up to the loading grace's end, the others told whom the room waits
    /// for); its progress is not said again until it is over.
    fn say_retrying(&mut self, job: &LoadJob) -> Result<(), Refusal> {
        let key = (job.stage, job.readiness_rev);
        if self.load_said.get(&key) == Some(&RETRYING_SAID) {
            return Ok(());
        }
        self.load_said.insert(key, RETRYING_SAID);
        self.send(Command::ReportLoading {
            stage: job.stage,
            readiness_rev: job.readiness_rev,
            loading: Loading::Retrying,
        })
    }

    /// The downloads due to be tried again by now, handed to the shell
    /// again (a stage started or a song replaced since: dropped).
    fn retry_due(&mut self) {
        if self.retrying.is_empty() {
            return;
        }
        let now = self.now_ms();
        let (due, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut self.retrying)
            .into_iter()
            .partition(|(at, _)| *at <= now);
        self.retrying = wait;
        for (_, job) in due {
            let current = self.readiness_rev(job.stage) == job.readiness_rev
                && self.state().stage(job.stage).is_some_and(|st| st.play.is_none());
            if current {
                self.jobs.push(job);
                self.version += 1;
            }
        }
    }

    /// Whether the local player may try `stage`'s download again by hand
    /// (round P1b): its retries spent and the failure reported, while the
    /// stage's window is open.
    pub fn may_retry_load(&self, stage: StageId) -> bool {
        self.load_problem(stage) == Some(LoadProblem::Download)
            && self.failed_jobs.contains_key(&stage)
            && matches!(
                self.state().phase,
                Phase::Preparing { stage: p, window: Some(_) } if p == stage
            )
            && self.state().stage(stage).is_some_and(|st| st.play.is_none())
    }

    /// Retry (round P1b): `stage`'s song downloaded again, the room told
    /// "loading" again, its retries counted afresh.
    pub fn retry_load(&mut self, stage: StageId) {
        if !self.may_retry_load(stage) {
            return;
        }
        let Some(job) = self.failed_jobs.remove(&stage) else {
            return;
        };
        self.load_tries.remove(&(stage, job.readiness_rev));
        self.load_said.remove(&(stage, job.readiness_rev));
        // Tried again: retrying, as the automatic tries are (revision 16).
        if self.say_retrying(&job).is_ok() {
            self.jobs.push(job);
        }
        self.version += 1;
    }

    /// The shell prepared a shared song's `job` (an online member's): the
    /// song and chart as read from the room's share, or why not. The stage
    /// is reported loaded once its chart is the one chosen.
    pub fn finish_shared_load(&mut self, job: &LoadJob, result: Result<Picked, LoadProblem>) {
        if job.stage == SPOTLIGHT {
            // The spotlight's song for the others to see and hear; a
            // failure leaves its card alone.
            // An answer for a song asked for before is dropped.
            let asked = self.spotlight_asked.clone().filter(|(content, share)| {
                job.share.as_ref() == Some(share)
                    && job.digest == content.chart.as_str().parse().ok()
            });
            if let (Ok(picked), Some((asked, _))) = (result, asked) {
                self.spotlight_loaded = Some((asked, picked));
                self.version += 1;
            }
            return;
        }
        if let Some(result) = self.take_shared_pick(job, result) {
            self.finish_load(job, result);
        }
    }

    /// A shared song's `job` prepared: the song and chart kept as the
    /// stage's content's pick; none where the answer counts for nothing
    /// (the song replaced since, or the stage without content).
    fn take_shared_pick(
        &mut self,
        job: &LoadJob,
        result: Result<Picked, LoadProblem>,
    ) -> Option<Result<(), LoadProblem>> {
        if self.readiness_rev(job.stage) != job.readiness_rev {
            return None;
        }
        Some(match result {
            Ok(picked) => {
                let content = self.stage_content(job.stage)?;
                self.picks.insert(content, picked);
                Ok(())
            }
            Err(p) => Err(p),
        })
    }

    /// The content stage `stage` plays, where it has some.
    fn stage_content(&self, stage: StageId) -> Option<ContentId> {
        self.state()
            .stage(stage)
            .and_then(|s| s.content.as_ref())
            .map(|c| c.id.clone())
    }

    /// The shell prepared `job` for an online room that plays live
    /// ([`prepare_stage`]): with what live play needs of the chart, its
    /// Loaded waits until the room's battle plan is known and agrees
    /// (`docs/plans/multiplayer-online-play.md`, "The battle plan"): the
    /// picker's device offers its own; every device then says it is ready
    /// to play live (its reading horizon and the plan's digest) and
    /// reports Loaded, or reports the mismatch and sits the stage out.
    /// Without what it needs (`None`), Loaded at once, as a device that
    /// does not play live.
    pub fn finish_load_live(&mut self, job: &LoadJob, result: Result<Option<LivePrep>, LoadProblem>) {
        match result {
            Ok(Some(prep)) if job.live.is_some() => {
                if self.readiness_rev(job.stage) != job.readiness_rev {
                    return;
                }
                self.preparing
                    .insert(job.stage, (job.readiness_rev, prep));
                self.sync_live();
            }
            Ok(_) => self.finish_load(job, Ok(())),
            Err(p) => self.finish_load(job, Err(p)),
        }
    }

    /// [`Self::finish_shared_load`] with what live play needs
    /// ([`Self::finish_load_live`]).
    pub fn finish_shared_load_live(
        &mut self,
        job: &LoadJob,
        result: Result<(Picked, Option<LivePrep>), LoadProblem>,
    ) {
        let (result, prep) = match result {
            Ok((picked, prep)) => (Ok(picked), prep),
            Err(p) => (Err(p), None),
        };
        if job.stage == SPOTLIGHT || prep.is_none() || job.live.is_none() {
            return self.finish_shared_load(job, result);
        }
        match self.take_shared_pick(job, result) {
            Some(Ok(())) => self.finish_load_live(job, Ok(prep)),
            Some(Err(p)) => self.finish_load(job, Err(p)),
            None => {}
        }
    }

    /// [`Self::finish_shared_load_live`] from the song as the shell read
    /// it from the share ([`SharedFound`]).
    pub fn finish_shared_found(&mut self, job: &LoadJob, result: Result<SharedFound, LoadProblem>) {
        // A listed song read for a stage is here for the room's list too
        // (stage 2): its files are in the room's store.
        if let (Ok(found), Some(share)) = (&result, &job.share)
            && job.stage != SPOTLIGHT
            && job.store.is_some()
            && self.readiness_rev(job.stage) == job.readiness_rev
        {
            self.ahead_read(
                share,
                Listing {
                    entry: found.entry.clone(),
                    pack: found.pack.clone(),
                    banner: found.banner.clone(),
                    bytes: 0,
                },
            );
        }
        self.finish_shared_load_live(job, result.and_then(SharedFound::picked));
    }

    /// What a stage's preparation should work out for live play here
    /// (none: the room does not play live).
    fn live_ask(&self, stage: StageId, content: &ContentId) -> Option<LiveAsk> {
        if !self.plays_live() {
            return None;
        }
        // The stage's own rules: the match's, or a change from it or an
        // earlier stage on (a pick always comes after the change it is
        // played under: a change never lands on a stage once picked).
        let s = self.state();
        let (rules, stage_rules) = match s.current.as_ref().filter(|_| s.phase != Phase::Lobby) {
            Some(m) if m.stage_index(stage).is_some() => {
                (m.rules_for(stage), StageRules::of_stage(m, stage))
            }
            _ => {
                let r = self.rules().clone();
                let sr = StageRules::of(&r);
                (r, sr)
            }
        };
        let settings = crate::state::settings::Settings::load();
        Some(LiveAsk {
            seed: ddi_room::live::plan::seed_for(&self.state().room, stage, &content.chart),
            rules: stage_rules,
            attacks: self.attacks_played(&rules),
            options: settings.play_options(None),
            speed_source: settings.speed_source,
        })
    }

    /// Stages prepared for live play whose Loaded waits on the room's
    /// battle plan: the picker's device offers its own; once the room has
    /// one, the device says it is ready to play live and reports Loaded,
    /// or reports the plan's mismatch.
    fn sync_live(&mut self) {
        if self.preparing.is_empty() || self.link_state() != LinkState::Connected {
            return;
        }
        // Each prep read in place, taken only once it is done with.
        let pending: Vec<StageId> = self.preparing.keys().copied().collect();
        for stage in pending {
            // Taken meanwhile (a command's answer ran this again).
            let Some((rev, prep)) = self.preparing.get(&stage) else {
                continue;
            };
            let (rev, ours) = (*rev, prep.plan.as_ref().map(|p| p.digest.clone()));
            let Some(st) = self.state().stage(stage) else {
                self.preparing.remove(&stage);
                continue;
            };
            let late = st.play.is_some() && {
                let now = self.room_now().unwrap_or(ddi_room::Millis(self.now_ms()));
                self.state().late_entry_open(self.me, stage, now).is_err()
            };
            if st.readiness_rev != rev || late {
                self.preparing.remove(&stage);
                continue;
            }
            let room_plan = st.content.as_ref().and_then(|c| c.battle.clone());
            let digest = match (ours, room_plan) {
                (None, _) => None,
                (Some(_), None) => {
                    // The picker's (or an operator's) device offers it at
                    // once; any other once the room's offers are open to
                    // everyone (revision 24, `plan-offers-v1`: a picker
                    // away or whose load failed held everyone's Loaded
                    // up); again after a refusal that may pass. The room
                    // takes the first; everyone compares theirs with it.
                    let now = self.now_ms();
                    let room_now = self.room_now().unwrap_or(ddi_room::Millis(now));
                    let may = self.chooses()
                        || self.may(Capability::ChooseContent)
                        || self.state().plan_offers_open(stage, room_now);
                    let due = match self.offered.get(&stage) {
                        None => true,
                        Some(o) => {
                            o.refused
                                && now.saturating_sub(o.at)
                                    >= ddi_room::live::plan::PLAN_OFFERS_V1.again_ms
                        }
                    };
                    if may && due {
                        self.offered.insert(stage, Offered { at: now, refused: false });
                        let plan = self.preparing.get(&stage).and_then(|(_, p)| p.plan.clone());
                        if let (Some(plan), Some(link)) = (plan, self.online_link_mut()) {
                            link.authority
                                .live_mut()
                                .send(ddi_room::live::ClientLive::OfferPlan {
                                    stage,
                                    plan: Box::new(plan),
                                });
                        }
                    }
                    continue;
                }
                (Some(ours), Some(room)) if room == ours => Some(room),
                (Some(_), Some(_)) => {
                    self.preparing.remove(&stage);
                    self.finish_mismatch(stage, rev);
                    continue;
                }
            };
            let Some((_, prep)) = self.preparing.remove(&stage) else {
                continue;
            };
            if let Some(link) = self.online_link_mut() {
                link.authority
                    .live_mut()
                    .send(ddi_room::live::ClientLive::Prepared {
                        stage,
                        horizon_us: prep.horizon_us,
                        plan: digest,
                    });
            }
            self.live_preps.insert(stage, prep);
            let _ = self.send(Command::ReportLoading {
                stage,
                readiness_rev: rev,
                loading: Loading::Loaded,
            });
        }
    }

    /// A socket opened (again): what this device told the room's live
    /// side once and it may not have read with the socket before (a
    /// battle plan offered, a stage prepared) is told again; the room
    /// takes either again as the same.
    fn connected_again(&mut self) {
        if !self.is_online() {
            return;
        }
        self.offered.clear();
        self.preload_again();
        // Away past the room's patience, the room forgot this member's
        // own start too (`drop_prep`): told again by the next
        // `sync_start` (review 2026-11-22, room-client 2).
        self.starts.sent = None;
        // Away past the room's patience, the room forgot what this member
        // had loaded of its stages not yet started (`ddi_room`'s
        // `drop_prep`): each such stage is prepared and reported again by
        // the next `sync_online` (feedback round N1: it never was, the
        // room showed the member Loading for good and they sat the stage
        // out).
        let me = self.me;
        let forgotten: Vec<(StageId, u32)> = self
            .stages()
            .iter()
            .filter(|st| st.play.is_none() && st.prep(me).loading == Loading::NotStarted)
            .map(|st| (st.id, st.readiness_rev))
            .collect();
        for key in forgotten {
            // (One whose Loaded only waits for the room's battle plan
            // reports it by itself, `sync_live`.)
            if !self.preparing.contains_key(&key.0) {
                self.asked.remove(&key);
                self.load_said.remove(&key);
            }
        }
        let again: Vec<(StageId, i64, Option<ddi_room::live::plan::PlanDigest>)> = self
            .live_preps
            .iter()
            .filter(|(stage, _)| self.state().stage(**stage).is_some_and(|st| st.play.is_none()))
            .map(|(stage, prep)| {
                (*stage, prep.horizon_us, prep.plan.as_ref().map(|p| p.digest.clone()))
            })
            .collect();
        if let Some(link) = self.online_link_mut() {
            for (stage, horizon_us, plan) in again {
                link.authority
                    .live_mut()
                    .send(ddi_room::live::ClientLive::Prepared {
                        stage,
                        horizon_us,
                        plan,
                    });
            }
        }
        self.sync_live();
    }

    /// This device's battle plan of `stage` is not the room's: its
    /// preparation reported failed (it sits the stage out).
    fn finish_mismatch(&mut self, stage: StageId, readiness_rev: u32) {
        self.live_preps.remove(&stage);
        let _ = self.send(Command::ReportLoading {
            stage,
            readiness_rev,
            loading: Loading::Failed {
                code: LoadProblem::PlanMismatch.code().into(),
            },
        });
    }

    /// What this device worked out for stage `stage`'s live play (its
    /// reading horizon, its battle plan), once it said it is ready to play
    /// it live.
    pub fn live_prep(&self, stage: StageId) -> Option<&LivePrep> {
        self.live_preps.get(&stage)
    }

    /// Seconds from now (this device's clock) until stage `stage`'s song
    /// time zero, where the room plays it live and its start is committed
    /// (`StagePlay::song_zero_at`), on the room's clock as estimated here;
    /// negative once it has passed.
    pub fn song_zero_in(&self, stage: StageId) -> Option<f64> {
        let zero = self.song_zero_at(stage)?;
        let now = self.room_now()?;
        Some((zero.0 as f64 - now.0 as f64) / 1_000.0)
    }

    /// Stage `stage`'s song time zero on the room's clock, where the room
    /// plays it live and its start is committed.
    pub fn song_zero_at(&self, stage: StageId) -> Option<ddi_room::Millis> {
        self.state().stage(stage)?.play.as_ref()?.song_zero_at
    }

    /// Where a shell's own threads' answers come from, read each tick.
    pub fn set_inbox(&mut self, inbox: Inbox) {
        self.inbox = Some(inbox);
    }

    fn read_inbox(&mut self) {
        let Some(mut inbox) = self.inbox.take() else {
            return;
        };
        let news = inbox();
        self.inbox = Some(inbox);
        for n in news {
            match n {
                ShellNews::Progress { stage, done, total } => {
                    self.upload_progress(stage, done, total)
                }
                ShellNews::Uploaded { stage, result } => self.upload_finished(stage, result),
                ShellNews::SongProgress { song, done, total } => {
                    self.song_upload_progress(song, done, total)
                }
                ShellNews::SongUploaded { song, result } => self.song_upload_finished(song, result),
                ShellNews::Prepared { job, result } => self.finish_load(&job, result),
                ShellNews::PreparedShared { job, result } => {
                    self.finish_shared_found(&job, result.map(|f| *f))
                }
                ShellNews::Listed { song, result } => self.preload_listed(song, result.map(|l| *l)),
                ShellNews::AheadProgress { song, done, total } => {
                    self.preload_progress(song, done, total)
                }
                ShellNews::Ahead { song, result } => self.preload_done(song, result),
            }
        }
    }

    /// The uploads to start, once each (the shell's to answer).
    pub fn take_upload_jobs(&mut self) -> Vec<UploadJob> {
        std::mem::take(&mut self.uploads)
    }

    /// How this device's sharing of stage `stage`'s song is going, while
    /// it is the room's pending content.
    pub fn sharing(&self, stage: StageId) -> Option<&Sharing> {
        let st = self.state().stage(stage)?;
        let (chosen, sharing) = self.sharing.get(&stage)?;
        (st.play.is_none() && *chosen == st.content_changed_at).then_some(sharing)
    }

    /// The upload of stage `stage`'s song moved on: `done` bytes of
    /// `total`.
    pub fn upload_progress(&mut self, stage: StageId, done: f64, total: f64) {
        if stage == SPOTLIGHT {
            if let Some((_, s @ Sharing::Uploading { .. })) = &mut self.spotlight_sharing {
                *s = Sharing::Uploading { done, total };
                self.version += 1;
            }
            return;
        }
        if let Some((_, s @ Sharing::Uploading { .. })) = self.sharing.get_mut(&stage) {
            *s = Sharing::Uploading { done, total };
            self.version += 1;
        }
    }

    /// The upload of stage `stage`'s song ended: the share made (the room
    /// hears it from the server, not from here), or why not. An expired
    /// grant is renewed and the upload made again.
    pub fn upload_finished(&mut self, stage: StageId, result: Result<String, ShareError>) {
        if stage == SPOTLIGHT {
            // The room hears it from the server; a failure is not retried
            // (the pick shares the song itself).
            if let Some((_, s @ Sharing::Uploading { .. })) = &mut self.spotlight_sharing {
                *s = match result {
                    Ok(_) => Sharing::Sent,
                    Err(e) => Sharing::Failed(SharingProblem::Upload(e)),
                };
            }
            return;
        }
        let Some((chosen, state)) = self.sharing.get(&stage).cloned() else {
            return;
        };
        if !matches!(state, Sharing::Uploading { .. }) {
            return;
        }
        let next = match result {
            Ok(_) => Sharing::Sent,
            Err(e) if ddi_library::share::grant_expired(&e) => {
                if let Some(l) = self.online_link_mut() {
                    l.request_grant(stage, chosen);
                }
                Sharing::Asking
            }
            Err(e) => Sharing::Failed(SharingProblem::Upload(e)),
        };
        self.sharing.insert(stage, (chosen, next));
        self.version += 1;
    }

    /// Share stage `stage`'s song again after a failure (or the room's
    /// refusal of what was shared): a new grant, a new upload.
    pub fn retry_sharing(&mut self, stage: StageId) {
        // What the room refused (Unavailable) is never asked for by
        // itself: Retry asks for its grant here.
        let refused = self
            .state()
            .stage(stage)
            .filter(|st| {
                st.play.is_none()
                    && st
                        .content
                        .as_ref()
                        .is_some_and(|c| matches!(c.status, ContentStatus::Unavailable { .. }))
            })
            .map(|st| st.content_changed_at);
        if let Some(chosen) = refused
            && self.may_retry_sharing(stage)
            && self.link_state() == LinkState::Connected
        {
            self.sharing.insert(stage, (chosen, Sharing::Asking));
            if let Some(l) = self.online_link_mut() {
                l.request_grant(stage, chosen);
            }
            self.version += 1;
            return;
        }
        if self.sharing.remove(&stage).is_some() {
            self.version += 1;
            self.sync_loading();
        }
    }

    /// Whether stage `stage`'s sharing failed here, or the room refused what
    /// was shared, and this device may share it again.
    pub fn may_retry_sharing(&self, stage: StageId) -> bool {
        let Some(st) = self.state().stage(stage) else {
            return false;
        };
        let refused = st
            .content
            .as_ref()
            .is_some_and(|c| matches!(c.status, ContentStatus::Unavailable { .. }));
        self.chooses()
            && st
                .content
                .as_ref()
                .is_some_and(|c| self.picks.contains_key(&c.id))
            && match self.sharing(stage) {
                Some(Sharing::Failed(_)) => true,
                // A retry under way is not offered again.
                Some(Sharing::Asking | Sharing::Uploading { .. }) => false,
                _ => refused,
            }
    }

    /// Grants the room answered: an upload handed to the shell, the room's
    /// earlier share taken (nothing to upload), or the refusal kept.
    fn take_grants(&mut self) {
        self.take_song_grants();
        let news = match self.online_link_mut() {
            Some(l) => l.take_grants(),
            None => return,
        };
        for n in news {
            if n.stage == SPOTLIGHT {
                self.spotlight_granted(n.answer);
                continue;
            }
            let Some((chosen, Sharing::Asking)) = self.sharing.get(&n.stage).cloned() else {
                continue;
            };
            if chosen != n.chosen {
                continue;
            }
            let content = self
                .state()
                .stage(n.stage)
                .filter(|st| st.content_changed_at == chosen)
                .and_then(|st| st.content.as_ref())
                .map(|c| c.id.clone());
            let next = match content {
                Some(content) => self.sharing_after_grant(n.stage, content, n.answer),
                None => n
                    .answer
                    .err()
                    .map(|p| Sharing::Failed(SharingProblem::Grant(p.code))),
            };
            let Some(next) = next else {
                continue;
            };
            self.sharing.insert(n.stage, (chosen, next));
            self.version += 1;
        }
    }

    /// The room's answer to the spotlight's grant: its upload handed to the
    /// shell, or the room's earlier share taken.
    fn spotlight_granted(
        &mut self,
        answer: Result<ddi_library::share::GrantAnswer, crate::state::online::api::Problem>,
    ) {
        let Some((content, Sharing::Asking)) = self.spotlight_sharing.clone() else {
            return;
        };
        let next = self
            .sharing_after_grant(SPOTLIGHT, content.clone(), answer)
            .unwrap_or_else(|| Sharing::Failed(SharingProblem::Grant("no_grant".into())));
        self.spotlight_sharing = Some((content, next));
    }

    /// The sharing of `content` (stage `stage`'s, or the spotlight's)
    /// after the room's grant `answer`: its upload handed to the shell
    /// where there is a grant and a pick, the room's earlier share taken,
    /// or the refusal kept; none for a grant with no pick to upload.
    fn sharing_after_grant(
        &mut self,
        stage: StageId,
        content: ContentId,
        answer: Result<ddi_library::share::GrantAnswer, crate::state::online::api::Problem>,
    ) -> Option<Sharing> {
        Some(match answer {
            Ok(answer) => match answer.grant {
                Some(grant) => {
                    let p = self.picks.get(&content)?;
                    self.uploads.push(UploadJob {
                        stage,
                        content,
                        song: p.song.clone(),
                        chart: p.chart.clone(),
                        grant,
                        max_bytes: answer.max_bytes,
                        entry: None,
                    });
                    Sharing::Uploading {
                        done: 0.0,
                        total: 0.0,
                    }
                }
                None if answer.share.is_some() => Sharing::Sent,
                None => Sharing::Failed(SharingProblem::Grant("no_grant".into())),
            },
            Err(p) => Sharing::Failed(SharingProblem::Grant(p.code)),
        })
    }

    /// Report one's own run of stage `stage` (an online room's: it ranks
    /// everyone's reports).
    /// `detail`: how they performed, for the stage's standings
    /// ([`run_detail`]; additive, a room that predates it passes none on).
    pub fn report_run(
        &mut self,
        stage: StageId,
        run: Run,
        detail: Option<RunDetail>,
    ) -> Result<(), Refusal> {
        self.send(Command::ReportOutcome {
            stage,
            run: ReportedRun::of(run),
            detail,
        })
    }

    /// How the competitors of `stage`, judged on this device, performed
    /// (a practice's stage: the player and the CPUs), for its standings.
    /// Kept by this client: a practice's authority ranks the stage and
    /// takes no reports.
    pub fn set_stage_details(&mut self, stage: StageId, details: Vec<(ParticipantId, RunDetail)>) {
        self.details
            .entry(stage)
            .or_default()
            .extend(details.into_iter().map(|(id, d)| (id, d.bounded())));
        self.version += 1;
    }

    /// The local player's attacks of `stage`, played on this device with
    /// attacks, for its results' summary (none: nothing kept).
    pub fn set_stage_summary(&mut self, stage: StageId, summary: Option<AttackSummary>) {
        if let Some(s) = summary {
            self.summaries.insert(stage, s);
            self.version += 1;
        }
    }

    /// The match `stage` is of (the running one, else a finished one the
    /// room keeps), the stage's place in it, and whether that match is
    /// over with every stage played.
    fn match_of(&self, stage: StageId) -> Option<(&ddi_room::state::Match, usize, bool)> {
        let s = self.state();
        if let Some(m) = &s.current
            && let Some(i) = m.stage_index(stage)
        {
            return Some((m, i, s.phase == Phase::MatchResults));
        }
        s.history.iter().rev().find_map(|record| {
            let i = record.played.stage_index(stage)?;
            let completed = matches!(record.ending, ddi_room::state::MatchEnding::Completed);
            Some((&record.played, i, completed))
        })
    }

    /// A member by id: someone in the room, else a CPU that left it with
    /// its match.
    pub fn member_of(&self, id: ParticipantId) -> Option<&ddi_room::Member> {
        let s = self.state();
        s.member(id).or_else(|| {
            s.history
                .iter()
                .rev()
                .find_map(|record| record.cpus.get(&id))
        })
    }

    /// Whether the room confirmed stage `stage`'s result (its table's
    /// `confirmed`, without building the table).
    pub fn stage_confirmed(&self, stage: StageId) -> bool {
        self.match_of(stage).is_some_and(|(m, index, _)| {
            m.stages[index]
                .play
                .as_ref()
                .is_some_and(|p| p.result.is_some())
        })
    }

    /// Stage `stage`'s standings as its results show them
    /// ([`StageTable`]): every starter, what the room confirmed of them
    /// and how they performed, the match's totals after it, and the
    /// match's result where it was its last.
    pub fn stage_table(&self, stage: StageId) -> Option<StageTable> {
        let (m, index, completed) = self.match_of(stage)?;
        let st = &m.stages[index];
        let play = st.play.as_ref()?;
        let local = self.details.get(&stage);
        Some(
            TableSource {
                stage,
                index,
                of: m.stages.len(),
                open_ended: m.rules.open_ended,
                roster: m.roster.keys().copied().collect(),
                results: m.stages[..=index]
                    .iter()
                    .map(|sg| sg.play.as_ref().and_then(|p| p.result.as_ref()))
                    .collect(),
                lineup: play.lineup.iter().copied().collect(),
                detail: &|id| {
                    local
                        .and_then(|d| d.get(&id))
                        .or_else(|| play.details.get(&id))
                        .cloned()
                },
                run: &|id| play.reports.get(&id).map(|r| r.run()),
                completed,
                standings: &|| m.standings(),
                rules: StageRules::of_stage(m, stage),
                attacks: self.attacks_played(&m.rules_for(stage)),
                adaptive_offered: self.adaptive_offered(),
                summary: self.summaries.get(&stage).cloned(),
                chat: self.chat_troubles.get(&stage).copied(),
            }
            .table(),
        )
    }

    /// The stage whose prompt the room would hand this player now
    /// ([`RoomClient::take_stage_order`], which this does not take),
    /// for a player who is on a screen opened over the room (Options,
    /// Calibrate, the announcer studio): they are told it is due, once a
    /// stage (never again after [`RoomClient::stay`]).
    pub fn stage_due(&self) -> Option<StageDue> {
        let stage = self.prompt_stage()?;
        if self.launched.contains(&stage) || self.stayed.contains(&stage) {
            return None;
        }
        let s = self.state();
        let m = s.current.as_ref()?;
        let index = m.stage_index(stage)?;
        self.picked(&m.stages[index].content.as_ref()?.id)?;
        Some(StageDue {
            stage,
            number: u16::try_from(index + 1).unwrap_or(u16::MAX),
            watching: !self.in_stage(stage),
        })
    }

    /// The stage (its number, from 1) that gathers readiness and waits
    /// for this player's Ready: they are in it, have not pressed Ready and
    /// have not left its prompt. For a shell whose page cannot show it (a
    /// browser tab in the background says so in its title).
    pub fn ready_wanted(&self) -> Option<u16> {
        let Phase::Preparing {
            stage,
            window: Some(_),
        } = self.state().phase
        else {
            return None;
        };
        if !self.in_stage(stage) || self.ready() || self.left_prompts.contains(&stage) {
            return None;
        }
        let index = self.state().current.as_ref()?.stage_index(stage)?;
        Some(u16::try_from(index + 1).unwrap_or(u16::MAX))
    }

    /// Told `stage` is due, the player stays where they are: not told
    /// again while its window or its play lasts (its prompt still opens
    /// by itself once they are back on the room's song select).
    pub fn stay(&mut self, stage: StageId) {
        self.stayed.insert(stage);
    }

    /// The stage a notice tells is due now, while that notice is open on
    /// a screen over the room (`screens::room::DueDialog` sets it and
    /// clears it when it goes): told once, whoever looks.
    pub fn telling_due(&self) -> Rc<std::cell::Cell<Option<StageId>>> {
        self.telling.clone()
    }

    /// The stage whose countdown just ended, to play now: handed over
    /// once (a stage is never played twice).
    pub fn take_stage_order(&mut self) -> Option<StageOrder> {
        let stage = self.prompt_stage()?;
        if self.launched.contains(&stage) {
            return None;
        }
        let s = self.state();
        let m = s.current.as_ref()?;
        let index = m.stage_index(stage)?;
        let st = &m.stages[index];
        let picked = self.picked(&st.content.as_ref()?.id)?.clone();
        // Who plays it: its lineup once committed, else every candidate
        // (CPUs never sit out; people's readiness is still open).
        let field: Vec<ParticipantId> = match &st.play {
            Some(play) => play.lineup.iter().copied().collect(),
            None => s.candidates(),
        };
        let cpus = field
            .iter()
            .filter_map(|id| match &s.member(*id)?.kind {
                MemberKind::Cpu { ordinal, opponent } => Some(CpuSeat::for_stage(
                    *id,
                    *ordinal,
                    opponent.level,
                    opponent.seed,
                    index + 1,
                    opponent.name.clone(),
                    opponent.personality.clone(),
                    opponent.targeting,
                )),
                MemberKind::Human { .. } => None,
            })
            .collect();
        let order = StageOrder {
            setup: StageSetup {
                stage,
                number: u16::try_from(index + 1).unwrap_or(u16::MAX),
                of: self.stage_of(stage),
                // What is really played: attacks only where they are
                // offered (never a rule that silently does nothing).
                battle: BattlePreset::with_attacks(self.attacks_played(&m.rules_for(stage))),
                rules: StageRules::of_stage(m, stage),
                dev_mines_at_beat: development().mines_at_beat,
                // Its own chart under Adaptive (revision 23), locked once
                // the start committed.
                start: s.start_of(stage, self.me).cloned(),
                me: self.me,
                targeting: self.my_targeting(),
                cpus,
                others: u16::try_from(
                    field
                        .iter()
                        .filter(|id| **id != self.me)
                        .filter(|id| s.member(**id).is_some_and(|m| !m.is_cpu()))
                        .count(),
                )
                .unwrap_or(u16::MAX),
                watching: !self.in_stage(stage),
                live: self.plays_live().then(|| LiveOrder {
                    seed: st.content.as_ref().map(|c| {
                        ddi_room::live::plan::seed_for(&s.room, stage, &c.id.chart)
                    }),
                    prep: self.live_preps.get(&stage).cloned(),
                    people: field
                        .iter()
                        .filter(|id| **id != self.me)
                        .filter_map(|id| match &s.member(*id)?.kind {
                            MemberKind::Human { name, .. } => Some((*id, name.clone())),
                            MemberKind::Cpu { .. } => None,
                        })
                        .collect(),
                }),
                // The room's chat (revision 18); CPU practice has no
                // host's Twitch: only a test's scripted chat or the live
                // chat's development script give its stages one.
                chat: if self.is_online() {
                    self.stage_chat()
                } else {
                    crate::state::twitch::RoomChat::practice()
                },
            },
            song: picked.song,
            chart: picked.chart,
            attacks_offered: self.attacks_offered(),
            adaptive_offered: self.adaptive_offered(),
        };
        self.launched.insert(stage);
        // A live stage this member watches: its music delayed to match the
        // fields, from what the last one watched needed (fixed for it).
        if order.watching && order.live.is_some() {
            self.watch_music.fix(stage);
        }
        Some(order)
    }

    /// The `of` of stage `stage`'s [`StageSetup`]: the match's stage
    /// count; in an open-ended match (revision 15) the stage's own number
    /// where it is the last song, else 0 (no set count: "Stage N"). Read
    /// again as its play starts (round AC: the Ready card may have made it
    /// the last song since its order was taken; the mark is fixed once
    /// the start commits).
    pub fn stage_of(&self, stage: StageId) -> u16 {
        let s = self.state();
        let Some(m) = s.current.as_ref() else {
            return 0;
        };
        let Some(index) = m.stage_index(stage) else {
            return 0;
        };
        if m.rules.open_ended {
            if m.is_last(stage, s.policy.max_stages) {
                u16::try_from(index + 1).unwrap_or(u16::MAX)
            } else {
                0
            }
        } else {
            u16::try_from(m.stages.len()).unwrap_or(u16::MAX)
        }
    }

    /// The music's delay behind the room's song time (ms) for `stage`, a
    /// live stage this member watches ([`WATCH_MUSIC_V0`]); 0 for a stage
    /// they play (and after going out of it: it is never moved mid-song),
    /// and in practice.
    pub fn watch_music_ms(&self, stage: StageId) -> u64 {
        self.watch_music.of(stage).unwrap_or(0)
    }

    /// A stage played on this device, reported to its room: an online
    /// room hears the local player's own run `mine` with how they
    /// performed (theirs of `details`), and ranks everyone's; a practice's
    /// stage is reported as its authority ([`RoomClient::report_stage`]),
    /// its competitors' `details` kept for its standings.
    pub fn report_played(
        &mut self,
        stage: StageId,
        mine: Run,
        ranking: Vec<Vec<Finisher>>,
        confirmed_early: bool,
        details: Vec<(ParticipantId, RunDetail)>,
    ) -> Result<(), Refusal> {
        if self.is_online() {
            let me = self.me;
            let detail = details
                .into_iter()
                .find(|(id, _)| *id == me)
                .map(|(_, d)| d);
            self.report_run(stage, mine, detail)
        } else {
            self.set_stage_details(stage, details);
            self.report_stage(stage, ranking, confirmed_early)
        }
    }

    /// A stage's judged outcome, reported as its authority: its ranking
    /// confirmed (before the song's end when it was decided early, the
    /// winner finishing the song), then the song's end. The room moves to
    /// the intermission or the match's results.
    pub fn report_stage(
        &mut self,
        stage: StageId,
        ranking: Vec<Vec<Finisher>>,
        confirmed_early: bool,
    ) -> Result<(), Refusal> {
        let commands = if confirmed_early {
            vec![
                Command::ConfirmStage { stage, ranking },
                Command::EndSong {
                    stage,
                    ranking: None,
                },
            ]
        } else {
            vec![Command::EndSong {
                stage,
                ranking: Some(ranking),
            }]
        };
        let Link::Local(authority) = &mut self.link else {
            // An online room's stage is reported by its members
            // (`report_run`).
            return self.refuse(Refusal::Room(Rejection::AuthorityOnly));
        };
        for command in commands {
            if let Err(why) = authority.authority(command) {
                self.pump();
                return self.refuse(Refusal::Room(why));
            }
        }
        self.refusal = None;
        self.version += 1;
        self.pump();
        Ok(())
    }

    /// The stage being played cannot be (its song could not be read when
    /// it came to play it): the match stops, its finished stages kept.
    pub fn stage_lost(&mut self) {
        let Link::Local(authority) = &mut self.link else {
            return;
        };
        let _ = authority.authority(Command::Interrupt {
            reason: ddi_room::Interruption::ContentLost,
        });
        self.version += 1;
        self.pump();
    }

    /// How the match ended for the local player, once its results are in.
    pub fn match_end(&self) -> Option<MatchEnd> {
        let s = self.state();
        if s.phase != Phase::MatchResults {
            return None;
        }
        // Not of the match (they watched it): it ended for nobody here.
        if !s.current.as_ref().is_some_and(|m| m.roster.contains_key(&self.me)) {
            return None;
        }
        let winners = s.match_winners();
        Some(match (winners.contains(&self.me), winners.len()) {
            (true, 1) => MatchEnd::Won,
            (true, _) => MatchEnd::Drawn,
            (false, _) => MatchEnd::NotWon,
        })
    }

    /// The local player's result of the match's last confirmed stage.
    pub fn last_mark(&self) -> Option<StageMark> {
        let s = self.state();
        let m = s.current.as_ref().filter(|_| s.phase != Phase::Lobby)?;
        (0..m.stages.len()).rev().find_map(|i| self.mark_at(m, i))
    }

    /// The local player's result of `m`'s stage at `i`, once confirmed.
    fn mark_at(&self, m: &ddi_room::state::Match, i: usize) -> Option<StageMark> {
        let play = m.stages[i].play.as_ref()?;
        let result = play.result.as_ref()?;
        Some(StageMark {
            number: u16::try_from(i + 1).unwrap_or(u16::MAX),
            placement: *result.placement(self.me)?,
            field: u16::try_from(play.lineup.len()).unwrap_or(u16::MAX),
        })
    }

    /// The local player's result of stage `stage`, once confirmed.
    pub fn mark_of(&self, stage: StageId) -> Option<StageMark> {
        let s = self.state();
        let m = s.current.as_ref()?;
        self.mark_at(m, m.stage_index(stage)?)
    }

    /// Cancel the readiness window or the countdown.
    pub fn cancel_start(&mut self) -> Result<(), Refusal> {
        let stage = match self.state().phase {
            Phase::Preparing { stage, .. } | Phase::Countdown { stage, .. } => stage,
            _ => return self.refuse(Refusal::Room(Rejection::NothingToCancel)),
        };
        self.send(Command::CancelStart { stage })
    }

    /// Stop the running match: it becomes interrupted, confirmed stages
    /// kept.
    pub fn stop_match(&mut self) -> Result<(), Refusal> {
        self.send(Command::StopMatch)
    }

    /// The match's Charts rule can be set here: CPU practice, where every
    /// play is judged on this device, and an online room that plays live
    /// (its contract carries the ladder and the steps).
    pub fn adaptive_offered(&self) -> bool {
        self.judged_here_or_live()
    }

    /// Whether the song the picker rests on (or picked last) has another
    /// chart in its chart's style to step to, easier or harder, as the
    /// song list has it (a chart of a timing of its own is no rung
    /// either, which only the stage's load knows: `ddi_engine::ladder`);
    /// `None` where no song is known.
    pub fn picked_ladder(&self) -> Option<bool> {
        let picked = self
            .resting
            .as_ref()
            .and_then(|(id, _, _)| self.picks.get(id))
            .or_else(|| self.spotlight().and_then(|(_, p)| p))?;
        let stage = &picked.chart;
        Some(picked.song.charts.iter().any(|c| {
            c.index != stage.index
                && c.style == stage.style
                && c.version.is_none()
                && (ddi_engine::ladder::is_easier(
                    (stage.difficulty, stage.meter),
                    (c.difficulty, c.meter),
                ) || ddi_engine::ladder::is_harder(
                    (stage.difficulty, stage.meter),
                    (c.difficulty, c.meter),
                ))
        }))
    }

    /// Whether a match under `rules` really plays adaptive charts here:
    /// its rule, where it is offered.
    pub fn adaptive_charts(&self, rules: &MatchRules) -> bool {
        self.adaptive_offered() && rules.charts == ddi_room::Charts::Adaptive
    }

    /// Whether the local player may save `rules` for the next match and
    /// stop the running match as one change (the Rules pop-up's "Apply
    /// and restart"): an operator, while a match runs (Stop match's
    /// phases: a stage's readiness, countdown and play, its song's end,
    /// the intermission).
    pub fn may_restart(&self) -> bool {
        self.may(Capability::EditNextRules)
            && self.may(Capability::StopMatch)
    }

    /// Whether the running match's per-stage rules may be changed from a
    /// later stage on here (the Rules pop-up's "From stage N", Points
    /// too, the maintainer, 2026-11-13): the player may edit rules, and a
    /// stage is left for it to land on.
    pub fn may_change_stage_rules(&self) -> bool {
        self.may(Capability::EditNextRules) && self.rules_land_on().is_some()
    }

    /// The rules of the stage before the one a change saved now would
    /// land on (the match's own where it lands on the first): what a
    /// change of Points is told against.
    pub fn rules_before_landing(&self) -> Option<MatchRules> {
        let (stage, _) = self.rules_land_on()?;
        let s = self.state();
        let m = s.current.as_ref()?;
        let at = m.stage_index(stage)?;
        Some(match at.checked_sub(1) {
            Some(i) => m.rules_for(m.stages[i].id),
            None => m.rules.clone(),
        })
    }

    /// Where a change of the running match's per-stage rules saved now
    /// would land: the stage and its number, from 1
    /// (`RoomState::rules_land_on`).
    pub fn rules_land_on(&self) -> Option<(StageId, u16)> {
        let s = self.state();
        let stage = s.rules_land_on()?;
        let index = s.current.as_ref()?.stage_index(stage)?;
        Some((stage, u16::try_from(index + 1).unwrap_or(u16::MAX)))
    }

    /// The rules stage `stage` of the running (or last) match is played
    /// under ([`ddi_room::Match::rules_for`]).
    pub fn rules_for_stage(&self, stage: StageId) -> Option<MatchRules> {
        let s = self.state();
        s.current.as_ref().map(|m| m.rules_for(stage))
    }

    /// The rules a change saved now would change: those of the stage it
    /// would land on (the match's, or an earlier change's).
    pub fn rules_ahead(&self) -> Option<MatchRules> {
        let (stage, _) = self.rules_land_on()?;
        self.rules_for_stage(stage)
    }

    /// Change the running match's per-stage rules (`rules`' Timing
    /// difficulty, Boos, Attacks and Charts; the rest is the
    /// match's) from stage `from` on, the stage the player was told
    /// ([`RoomClient::rules_land_on`]). Refused with
    /// `Rejection::RulesLandLater` where the room has moved on since.
    pub fn change_stage_rules(&mut self, from: StageId, rules: &MatchRules) -> Result<(), Refusal> {
        self.change_stage_rules_at(None, from, rules)
    }

    /// The room's revision now: what an edit begun now is based on
    /// (the Rules pop-up keeps it from when it opened).
    pub fn revision(&self) -> Revision {
        self.state().revision
    }

    /// Whether the rules (the next match's, or the running match's
    /// changes from a later stage) were saved since revision `base`: an
    /// edit begun then would put back what it was begun on.
    pub fn rules_changed_since(&self, base: Revision) -> bool {
        let s = self.state();
        s.next.saved_at > base || s.current.as_ref().is_some_and(|m| m.changes_saved_at() > base)
    }

    /// [`Self::change_stage_rules`] for an edit begun at revision `base`
    /// (none: now): the room refuses it (`Rejection::Stale`) where they
    /// were changed since.
    pub fn change_stage_rules_at(
        &mut self,
        base: Option<Revision>,
        from: StageId,
        rules: &MatchRules,
    ) -> Result<(), Refusal> {
        // The bias named only where it is not the match's own: a match
        // whose Points nobody changes sends what it did before.
        let play = match self.state().current.as_ref() {
            Some(m) => {
                // Named wherever it differs from the match's or from the
                // stage's as it stands, so that a change back to the
                // match's own is said (an authority reads a change naming
                // none as keeping the stage's).
                let ahead = m.rules_for(from).points;
                ddi_room::PlayRules {
                    points: (rules.points != m.rules.points || ahead != m.rules.points)
                        .then_some(rules.points),
                    ..rules.play_rules()
                }
            }
            _ => rules.play_rules(),
        };
        self.send_at(base, Command::ChangeStageRules { from, rules: play })
    }

    /// Save `rules` for the next match and stop the running match, as one
    /// change: the next pick starts a new match under them.
    pub fn restart_match(&mut self, rules: MatchRules) -> Result<(), Refusal> {
        self.restart_match_at(None, rules)
    }

    /// [`Self::restart_match`] for an edit begun at revision `base`.
    pub fn restart_match_at(&mut self, base: Option<Revision>, rules: MatchRules) -> Result<(), Refusal> {
        self.send_at(base, Command::RestartMatch { rules })?;
        self.remember_roster();
        Ok(())
    }

    /// From a finished or stopped match back to the lobby, the next match's
    /// rules applied.
    pub fn open_next_match(&mut self) -> Result<(), Refusal> {
        self.send(Command::OpenNextMatch)
    }

    /// Save the next match's rules (in the lobby, the rules the match will
    /// start with; during a match, a draft that leaves it alone).
    pub fn save_rules(&mut self, rules: MatchRules) -> Result<(), Refusal> {
        self.save_rules_at(None, rules)
    }

    /// [`Self::save_rules`] for an edit begun at revision `base` (none:
    /// now): the room refuses it (`Rejection::Stale`) where the rules
    /// were saved since, never putting back what the edit began on.
    pub fn save_rules_at(&mut self, base: Option<Revision>, rules: MatchRules) -> Result<(), Refusal> {
        self.send_at(base, Command::SaveNextRules { rules })?;
        self.remember_roster();
        Ok(())
    }

    /// Whether `rules` would be accepted for the next match now: the room's
    /// own validation, before the press.
    pub fn check_rules(&self, rules: &MatchRules) -> Result<(), RulesProblem> {
        let s = self.state();
        rules.validate(&s.policy, s.reserved_humans().max(1))
    }

    /// The most CPU opponents the room's cap leaves beside the people who
    /// play.
    pub fn max_cpus(&self) -> u16 {
        let s = self.state();
        let humans = u16::try_from(s.reserved_humans().max(1)).unwrap_or(u16::MAX);
        s.policy.max_competitors.saturating_sub(humans)
    }

    /// The fewest: enough to make a field the room may start.
    pub fn min_cpus(&self) -> u16 {
        let s = self.state();
        let humans = u16::try_from(s.reserved_humans().max(1)).unwrap_or(u16::MAX);
        s.policy.readiness.min_competitors.saturating_sub(humans)
    }

    fn readiness_rev(&self, stage: StageId) -> u32 {
        self.state().stage(stage).map_or(0, |s| s.readiness_rev)
    }

    fn refuse(&mut self, why: Refusal) -> Result<(), Refusal> {
        self.refusal = Some(why.clone());
        self.version += 1;
        Err(why)
    }

    /// One command through the envelope; its answer read from the
    /// messages that follow (practice: at once; online: later, a refusal
    /// then said on the status line).
    fn send(&mut self, cmd: Command) -> Result<(), Refusal> {
        self.send_at(None, cmd)
    }

    /// [`Self::send`] based on revision `base` (none: the mirror's now).
    fn send_at(&mut self, base: Option<Revision>, cmd: Command) -> Result<(), Refusal> {
        let answer = self.submit(base, cmd);
        self.unanswered = answer.is_none().then_some(self.seq);
        match answer {
            Some(Some(rejection)) => self.refuse(Refusal::Room(rejection)),
            _ => {
                self.refusal = None;
                self.version += 1;
                self.sync_loading();
                self.sync_start();
                Ok(())
            }
        }
    }

    /// One command in its envelope, and what the authority sent after it
    /// applied: the command's answer if among it ([`Self::pump_for`]).
    fn submit(&mut self, base: Option<Revision>, cmd: Command) -> Option<Option<Rejection>> {
        self.seq += 1;
        let seq = self.seq;
        let base = base.or_else(|| self.mirror.state().map(|s| s.revision));
        let client = self.client.clone();
        self.authority_mut()
            .submit(ddi_room::ClientEnvelope::new(client, seq, base, cmd));
        self.pump_for(seq)
    }

    fn pump(&mut self) {
        self.pump_for(0);
        // The readiness asked shows in the room already.
        if self.ready_asked.is_some_and(|(_, r)| r == self.ready()) {
            self.ready_asked = None;
        }
        self.sync_loading();
        self.sync_start();
    }

    /// Applies what the authority sent; the answer to `seq` if among it
    /// (`Some(None)`: accepted).
    fn pump_for(&mut self, seq: u64) -> Option<Option<Rejection>> {
        let mut answer = None;
        let envelopes = self.authority_mut().poll();
        for envelope in envelopes {
            match &envelope.msg {
                ServerMessage::Ack { client, seq: s } if *client == self.client && *s == seq => {
                    answer = Some(None);
                }
                // Ready asked earlier, answered now: the room's own
                // readiness is the player's again.
                ServerMessage::Ack { client, seq: s }
                    if *client == self.client && self.ready_asked.is_some_and(|(r, _)| r == *s) =>
                {
                    self.ready_asked = None;
                    self.version += 1;
                }
                ServerMessage::Reject {
                    client,
                    seq: s,
                    rejection,
                } if *client == self.client && *s == seq => {
                    answer = Some(Some(rejection.clone()));
                }
                ServerMessage::Reject {
                    client,
                    rejection,
                    seq: s,
                } if *client == self.client => {
                    // An earlier command's refusal, come later (online).
                    if self.ready_asked.is_some_and(|(r, _)| r == *s) {
                        self.ready_asked = None;
                    }
                    // Not one asked before "Use this device" (round AC).
                    if !self.stale_before.is_some_and(|q| *s <= q) {
                        self.refusal = Some(Refusal::Room(rejection.clone()));
                    }
                    self.version += 1;
                }
                ServerMessage::Events { events } => {
                    // A kick is told to everyone (revision 18): the
                    // status line says who removed whom, but to the one
                    // removed, whose own words come with the link's end.
                    let me = self.me;
                    let kicked = events.iter().rev().find_map(|e| match e {
                        ddi_room::Event::MemberRemoved { participant, by } if *participant != me => {
                            Some((*participant, *by))
                        }
                        _ => None,
                    });
                    if let (Some((who, by)), Link::Online(l)) = (kicked, &mut self.link) {
                        l.set_notice(Some(LinkNotice::Kicked { who, by }));
                    }
                    // This player's song choice taken away by the room, not
                    // by their own doing (revision 22, round RL4b): told
                    // once, in the room's words.
                    let gone = events.iter().rev().find_map(|e| match e {
                        ddi_room::Event::ChoiceChanged {
                            participant,
                            choice: None,
                            why: Some(why),
                        } if *participant == me => Some(*why),
                        _ => None,
                    });
                    if let (Some(why), Link::Online(l)) = (gone, &mut self.link) {
                        l.set_notice(Some(LinkNotice::ChoiceGone { why }));
                    }
                    self.version += 1;
                }
                ServerMessage::Snapshot { .. } => {
                    self.version += 1;
                }
                _ => {}
            }
            if self.mirror.receive(&envelope).is_err()
                && let Some(room) = self.authority_mut().room().cloned()
            {
                // A missed revision: the authority's room (the local one,
                // or the online one's own mirror, which asks the server
                // again itself) is the snapshot to repair from.
                let room = Box::new(room);
                let _ = self.mirror.receive(&ddi_room::ServerEnvelope::new(
                    room.revision,
                    ServerMessage::Snapshot { room },
                ));
            }
            self.forget_left_prompts();
        }
        let found = match &self.link {
            Link::Online(l) => self.state().member_by_identity(l.identity()).map(|m| m.id),
            Link::Local(_) => None,
        };
        if let Some(id) = found {
            self.me = id;
            // Live play reads its stream's acknowledgements from this
            // participant's frame entries.
            if let Link::Online(l) = &mut self.link {
                l.authority.live_mut().set_me(id);
            }
        }
        answer
    }

    /// A prompt left is forgotten once the window or the play it was left
    /// in is over (a window closed, the stage played): the next window of
    /// the same stage (the pick made again) opens it again.
    fn forget_left_prompts(&mut self) {
        if self.left_prompts.is_empty() && self.stayed.is_empty() {
            return;
        }
        let open = match self.state().phase {
            Phase::Preparing {
                stage,
                window: Some(_),
            }
            | Phase::Countdown { stage, .. }
            | Phase::Stage { stage } => Some(stage),
            _ => None,
        };
        self.left_prompts.retain(|s| Some(*s) == open);
        self.stayed.retain(|s| Some(*s) == open);
    }

    /// What the room's live side said since the last tick: kept for the
    /// stage being played, which reads it (`take_live_news`); nothing is
    /// kept while no stage is played live here, so that it never piles up.
    fn read_live(&mut self) {
        let Link::Online(l) = &mut self.link else {
            return;
        };
        let news = l.authority.live_mut().take_news();
        // An offer refused for a reason that may pass is offered again
        // (revision 24): too soon, the stage not known yet, not ours yet.
        for n in &news {
            if let ddi_room::live::LiveNews::Refused(
                ddi_room::live::LiveRefusal::OfferTooSoon { stage }
                | ddi_room::live::LiveRefusal::NotPicker { stage }
                | ddi_room::live::LiveRefusal::UnknownStage { stage },
            ) = n
                && let Some(o) = self.offered.get_mut(stage)
            {
                o.refused = true;
            }
        }
        // A plan this device computed differently: it sits the stage out.
        let mismatched: Vec<StageId> = news
            .iter()
            .filter_map(|n| match n {
                ddi_room::live::LiveNews::Refused(ddi_room::live::LiveRefusal::PlanMismatch {
                    stage,
                    ..
                }) => Some(*stage),
                _ => None,
            })
            .collect();
        self.live_news.extend(news);
        for stage in mismatched {
            let rev = self.readiness_rev(stage);
            if self.state().stage(stage).is_some_and(|s| s.play.is_none()) {
                self.finish_mismatch(stage, rev);
            }
        }
    }

    /// The room's live news (frames, snapshots, refusals) since the last
    /// call, for the stage played live on this device; asking for it keeps
    /// it coming ([`Self::stop_live_news`] stops it).
    pub fn take_live_news(&mut self) -> Vec<ddi_room::live::LiveNews> {
        self.live_news.take()
    }

    /// The local play's stream of `stage`, where this room plays live
    /// (`ddi_room::live::Outbox`: sent as the room client is ticked).
    pub fn live_stream(&mut self, stage: StageId) -> Option<&mut ddi_room::live::Outbox> {
        if !self.plays_live() {
            return None;
        }
        match &mut self.link {
            Link::Online(l) => Some(l.authority.live_mut().stream(stage)),
            Link::Local(_) => None,
        }
    }

    /// The room no longer knows `stage` live (it restarted): its stream
    /// and frames dropped (`ddi_room::live::LiveLink::forget`).
    pub fn forget_live(&mut self, stage: StageId) {
        if let Link::Online(l) = &mut self.link {
            l.authority.live_mut().forget(stage);
        }
    }

    /// The stage played live here is over: its news is no longer kept.
    pub fn stop_live_news(&mut self) {
        self.live_news.stop();
    }

    /// Practice's content is the device's own library: every unstarted
    /// stage it has a song for whose preparation has not begun (again
    /// after a change cleared it) is reported loading and handed to the
    /// shell to prepare ([`Self::take_load_jobs`]).
    fn sync_loading(&mut self) {
        if self.is_online() {
            self.sync_online();
            return self.sync_live();
        }
        loop {
            let me = self.me;
            let next = self.stages().iter().find_map(|st| {
                let content = st
                    .content
                    .as_ref()
                    .filter(|c| c.status == ContentStatus::Available)?;
                (st.play.is_none() && st.prep(me).loading == Loading::NotStarted)
                    .then(|| (st.id, st.readiness_rev, content.id.clone()))
            });
            let Some((stage, readiness_rev, content)) = next else {
                return;
            };
            let answer = self.submit(None, Command::ReportLoading {
                stage,
                readiness_rev,
                loading: Loading::Loading { permille: 0 },
            });
            if !matches!(answer, Some(None)) {
                return;
            }
            if let Some(p) = self.picks.get(&content)
                && self.asked.insert((stage, readiness_rev))
            {
                let job = LoadJob::library(stage, readiness_rev, p, p.chart.digest, true, None);
                self.jobs.push(job);
            }
        }
    }

    /// An online room's content (M5d). The picker's own pick waiting for
    /// its share (pending, or refused by the room) is shared from here
    /// once per choice: the room's grant asked for, then the upload handed
    /// to the shell ([`Self::take_upload_jobs`]). Content the room made
    /// available is prepared by everyone, as practice's is: reported
    /// loading and handed to the shell, from the library where this device
    /// chose it, else from the room's share.
    fn sync_online(&mut self) {
        if self.link_state() != LinkState::Connected {
            return;
        }
        // Revision 17: the room's list replaces the spotlight's upload
        // ahead (the picker's resting song is a mark on its entry); the
        // songs this device added are uploaded from here.
        self.sync_song_uploads();
        self.map_listed();
        let me = self.me;
        let picker = self.chooses();
        let mut ask = Vec::new();
        let mut prepare = Vec::new();
        for st in self.stages().iter().filter(|st| st.play.is_none()) {
            let Some(content) = &st.content else {
                continue;
            };
            match (&content.status, &content.share) {
                (ContentStatus::Pending, _)
                    if picker
                        && self.picks.contains_key(&content.id)
                        && self
                            .sharing
                            .get(&st.id)
                            .is_none_or(|(chosen, _)| *chosen != st.content_changed_at) =>
                {
                    ask.push((st.id, st.content_changed_at));
                }
                (ContentStatus::Available, share)
                    if st.prep(me).loading == Loading::NotStarted
                        && (share.is_some() || content.song.is_some()) =>
                {
                    prepare.push((st.id, st.readiness_rev, content.id.clone(), share.clone()));
                }
                _ => {}
            }
        }
        for (stage, chosen) in ask {
            self.sharing.insert(stage, (chosen, Sharing::Asking));
            if let Some(l) = self.online_link_mut() {
                l.request_grant(stage, chosen);
            }
            self.version += 1;
        }
        for (stage, readiness_rev, content, share) in prepare {
            if !self.asked.insert((stage, readiness_rev)) {
                continue;
            }
            if self
                .send(Command::ReportLoading {
                    stage,
                    readiness_rev,
                    loading: Loading::Loading { permille: 0 },
                })
                .is_err()
            {
                continue;
            }
            let live = self.live_ask(stage, &content);
            let job = match (self.picks.get(&content), share) {
                (Some(p), _) => {
                    let digest = content.chart.as_str().parse().ok();
                    LoadJob::library(stage, readiness_rev, p, digest, false, live)
                }
                (None, Some(share)) => LoadJob {
                    store: Some(self.store()),
                    ..LoadJob::from_share(stage, readiness_rev, share, &content.chart, live)
                },
                // An entry a room stored before revision 19 listed as a
                // bundled song (nothing uploaded) that this device does
                // not have: it cannot be read here.
                (None, None) => {
                    let _ = self.send(Command::ReportLoading {
                        stage,
                        readiness_rev,
                        loading: Loading::Failed {
                            code: LoadProblem::Unreadable.code().into(),
                        },
                    });
                    continue;
                }
            };
            self.jobs.push(job);
        }
        self.sync_watched();
    }

    /// The stage under way, where this member is not in it and may watch
    /// it ([`Self::watches`]) but has not its song (they were away through
    /// its pick, or their page was loaded after it started): read from the
    /// room's share, so that the stage is handed to them to watch
    /// (feedback round N1: they stayed on the room's song select with
    /// nothing to see). Nothing is reported to the room: the stage started
    /// without them.
    fn sync_watched(&mut self) {
        let Phase::Stage { stage } = self.state().phase else {
            return;
        };
        if !self.watches() || self.in_stage(stage) {
            return;
        }
        let Some(st) = self.state().stage(stage) else {
            return;
        };
        let Some((content, share)) = st
            .content
            .as_ref()
            .and_then(|c| Some((c.id.clone(), c.share.clone()?)))
        else {
            return;
        };
        let readiness_rev = st.readiness_rev;
        if self.picks.contains_key(&content) || !self.asked.insert((stage, readiness_rev)) {
            return;
        }
        let job = LoadJob {
            store: Some(self.store()),
            ..LoadJob::from_share(stage, readiness_rev, share, &content.chart, None)
        };
        self.jobs.push(job);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::data::Difficulty;
    use ddi_room::Finish;

    pub(crate) fn song(id: &str) -> SongEntry {
        SongEntry {
            id: id.into(),
            title: format!("Song {id}"),
            artist: "Artist".into(),
            charts: vec![chart("dance-single", 0), chart("dance-double", 1)],
            ..SongEntry::default()
        }
    }

    pub(crate) fn chart(style: &str, index: usize) -> ChartEntry {
        ChartEntry {
            style: style.into(),
            difficulty: Difficulty::Hard,
            meter: 9,
            index,
            digest: Some(ddi_chart::ChartDigest(0x1234 + index as u128)),
            ..ChartEntry::default()
        }
    }

    fn cpus(room: &RoomClient) -> usize {
        crate::state::cpu_roster::roster_of(room.rules(), room.humans()).len()
    }

    #[test]
    fn practice_seats_a_named_roster_and_development_far_more() {
        let room = RoomClient::new(0, 7);
        assert_eq!(room.max_cpus(), 72, "73 competitors, the player included");
        let roster = &room.rules().cpu.roster;
        assert_eq!(roster.len(), STARTING_CPUS);
        assert!(
            roster
                .iter()
                .all(|c| c.name.is_some() && c.personality.is_some())
        );
        let dev = RoomClient::development(0, 7, 64);
        assert_eq!(cpus(&dev), 64);
        assert!(dev.check_rules(dev.rules()).is_ok());
        assert_eq!(dev.rules().awards, ddi_room::AwardScheme::DevelopmentV0);
        assert_eq!(dev.max_cpus(), 128);
        // Never beyond the development cap.
        let most = RoomClient::development(0, 7, 1000);
        assert_eq!(cpus(&most), 128);
        // The shells' maker: an ordinary practice room, or the development
        // switch's beyond the cap (the thread's switches put back after).
        let before = development();
        set_development(Development { cpus: 0, ..before });
        assert_eq!(practice_room(5).borrow().max_cpus(), 72);
        set_development(Development { cpus: 64, ..before });
        let dev = practice_room(5);
        assert_eq!(cpus(&dev.borrow()), 64);
        assert_eq!(
            dev.borrow().rules().awards,
            ddi_room::AwardScheme::DevelopmentV0
        );
        set_development(before);
    }

    /// Every song asked for prepared, as a shell does.
    pub(crate) fn prepare(room: &mut RoomClient) {
        for job in room.take_load_jobs() {
            room.finish_load(&job, Ok(()));
        }
    }

    #[test]
    fn a_pick_starts_the_match_and_opens_the_prompt_until_ready_starts_the_stage() {
        let mut room = RoomClient::new(1_000, 7);
        let s = song("a");
        assert_eq!(room.view(), RoomView::Select);
        assert_eq!(room.next_pick_number(), 1);
        // The CPUs play four-lane one-player charts only.
        assert_eq!(
            room.pick(&s, &s.charts[1]),
            Err(Refusal::Chart(Unsupported::Layout {
                style: "dance-double".into()
            }))
        );
        assert_eq!(room.state().phase, Phase::Lobby, "nothing started");
        room.pick(&s, &s.charts[0]).unwrap();
        let first = room.stages()[0].id;
        assert!(matches!(
            room.state().phase,
            Phase::Preparing {
                window: Some(_),
                ..
            }
        ));
        assert_eq!(room.view(), RoomView::Prompt(first));
        assert_eq!(
            room.stages()[0]
                .content
                .as_ref()
                .unwrap()
                .card
                .as_ref()
                .unwrap()
                .title,
            "Song a"
        );
        // The prompt opens once, before anyone is ready; the CPUs with it.
        let order = room.take_stage_order().unwrap();
        assert_eq!((order.stage, order.number, order.of), (first, 1, 4));
        assert_eq!(order.cpus.len(), 3);
        assert!(order.cpus.iter().all(|c| c.name.is_some()));
        // Practice plays with attacks by default; the order says what
        // is played.
        assert_eq!(order.battle, BattlePreset::GaugeAttacksV0);
        assert_eq!(order.attacks_rule(), Some(true));
        assert!(room.take_stage_order().is_none());
        // Loading until the shell prepared it; Ready while loading holds.
        assert!(room.loading(first));
        room.set_ready(true).unwrap();
        assert!(!room.stage_started(first));
        prepare(&mut room);
        // Ready and loaded, alone with CPUs: the countdown, then the stage.
        assert!(matches!(room.state().phase, Phase::Countdown { .. }));
        room.tick(3_999);
        assert!(!room.stage_started(first));
        room.tick(4_000);
        assert!(room.stage_started(first));
        assert_eq!(room.leave_prompt(first), PromptExit::Forfeit);
        room.stop_match().unwrap();
        assert!(matches!(room.state().phase, Phase::Interrupted { .. }));
        assert_eq!(room.view(), RoomView::Select);
        // Stopped before any stage was finished: no standings to pop up
        // (they would be zeros).
        assert_eq!(room.unseen_results(), None);
        // A pick after a stopped match opens the next match.
        room.pick(&s, &s.charts[0]).unwrap();
        assert_eq!(room.state().history.len(), 1);
        assert!(!room.ready(), "a new match has fresh readiness");
    }

    #[test]
    fn leaving_the_prompt_before_the_start_withdraws_and_reopens_the_pick() {
        let mut room = RoomClient::new(0, 1);
        let s = song("a");
        room.pick(&s, &s.charts[0]).unwrap();
        let first = room.stages()[0].id;
        assert!(room.take_stage_order().is_some());
        // The operator's Back: not ready, the window closed, song select.
        assert_eq!(room.leave_prompt(first), PromptExit::Withdraw);
        assert!(matches!(
            room.state().phase,
            Phase::Preparing { window: None, .. }
        ));
        assert_eq!(room.view(), RoomView::Select);
        assert_eq!(room.next_pick_number(), 1);
        // Picking it again opens its prompt again.
        room.pick(&s, &s.charts[0]).unwrap();
        assert_eq!(room.view(), RoomView::Prompt(first));
        assert!(room.take_stage_order().is_some());
    }

    /// The prompt left during the practice's countdown (Ready pressed,
    /// alone with CPUs; review3 web-4's prompt case, by the game's Back
    /// or the browser's): the start canceled and the player withdrawn, as
    /// before it, never a stage started with them that nobody plays.
    #[test]
    fn leaving_the_prompt_in_the_countdown_cancels_the_start() {
        let mut room = RoomClient::new(0, 1);
        let s = song("a");
        room.pick(&s, &s.charts[0]).unwrap();
        let first = room.stages()[0].id;
        assert!(room.take_stage_order().is_some());
        prepare(&mut room);
        room.set_ready(true).unwrap();
        assert!(matches!(room.state().phase, Phase::Countdown { .. }));
        room.tick(1_000);
        assert_eq!(room.leave_prompt(first), PromptExit::Withdraw);
        room.tick(10_000);
        assert!(!room.stage_started(first), "{:?}", room.state().phase);
        assert!(!room.ready());
        assert_eq!(room.view(), RoomView::Select);
        // Picking it again opens its prompt again.
        room.pick(&s, &s.charts[0]).unwrap();
        assert_eq!(room.view(), RoomView::Prompt(first));
        assert!(room.take_stage_order().is_some());
    }

    #[test]
    fn rules_follow_the_cap_and_the_roster() {
        let mut room = RoomClient::new(0, 1);
        assert_eq!((room.min_cpus(), room.max_cpus()), (1, 72));
        // At the cap: 72 CPUs, the catalog's and then numbered ones.
        let full = crate::state::cpu_roster::at_level(&[], 4, 72, 3);
        assert_eq!(full.len(), 72);
        let at_cap = crate::state::cpu_roster::with_roster(room.rules().clone(), full, 1);
        assert!(room.check_rules(&at_cap).is_ok());
        let roster = crate::state::cpu_roster::at_level(&[], 4, 8, 3);
        let mut rules = crate::state::cpu_roster::with_roster(room.rules().clone(), roster, 1);
        rules.stages = 2;
        room.save_rules(rules.clone()).unwrap();
        assert_eq!(cpus(&room), 8);
        assert_eq!(room.stages().len(), 2);
        let mut over = at_cap.clone();
        over.cpu.roster.push(over.cpu.roster[0].clone());
        let over = crate::state::cpu_roster::with_roster(over.clone(), over.cpu.roster, 1);
        assert!(room.check_rules(&over).is_err());
        assert!(matches!(
            room.save_rules(over),
            Err(Refusal::Room(Rejection::InvalidRules(
                RulesProblem::OverCap(_)
            )))
        ));
        // The match seats the roster's CPUs, each its own profile.
        let s = song("a");
        room.pick(&s, &s.charts[0]).unwrap();
        let order = room.take_stage_order().unwrap();
        assert_eq!(order.cpus.len(), 8);
        assert!(order.cpus.iter().all(|c| c.level == 4));
    }

    #[test]
    fn a_failed_preparation_is_reported_and_withdrawing_lets_the_host_pick_again() {
        let mut room = RoomClient::new(0, 1);
        let s = song("a");
        room.pick(&s, &s.charts[0]).unwrap();
        let first = room.stages()[0].id;
        let old = room.take_load_jobs();
        room.finish_load(&old[0], Err(LoadProblem::CpuCannotPlay));
        assert_eq!(room.load_problem(first), Some(LoadProblem::CpuCannotPlay));
        assert_eq!(room.leave_prompt(first), PromptExit::Withdraw);
        // Another song: a new job; the old answer counts for nothing.
        let t = song("b");
        room.pick(&t, &t.charts[0]).unwrap();
        assert_eq!(room.load_problem(first), None);
        let jobs = room.take_load_jobs();
        assert_eq!(jobs[0].song, "b");
        room.finish_load(&old[0], Ok(()));
        assert!(room.loading(first));
        room.finish_load(&jobs[0], Ok(()));
        assert!(!room.loading(first));
    }

    /// A whole practice match with outcomes reported as a stage runner
    /// does: the standings once, the next pick, the match's results, and
    /// a pick opening the next match.
    /// A watched match (`watch-v0`): its stages are handed over as
    /// watched, with nobody else and no seat of the player's; reported,
    /// it reaches its results, where it ended for nobody on this device
    /// (no "match not won" for someone who only watched) and the player
    /// has no mark; watching holds into the next match.
    #[test]
    fn a_watched_match_is_played_by_the_cpus_and_ends_for_nobody_here() {
        let mut room = RoomClient::new(0, 1);
        let rules = MatchRules {
            stages: 1,
            ..room.rules().clone()
        };
        room.save_rules(rules).unwrap();
        assert!(room.may_choose_intent() && !room.watching());
        room.set_intent(ddi_room::Intent::Watch).unwrap();
        assert!(room.watching());
        let s = song("a");
        room.pick(&s, &s.charts[0]).unwrap();
        let order = room.take_stage_order().unwrap();
        assert!(order.watching && order.others == 0);
        assert!(!room.in_stage(order.stage));
        // Not theirs to change during a stage they are in; a watcher may.
        assert!(room.may_choose_intent());
        room.tick(3_000);
        assert!(room.stage_started(order.stage));
        assert_eq!(room.leave_prompt(order.stage), PromptExit::Forfeit);
        let ranking: Vec<Vec<Finisher>> = order
            .cpus
            .iter()
            .map(|c| {
                vec![Finisher {
                    participant: c.participant,
                    finish: Finish::Survived,
                }]
            })
            .collect();
        room.report_played(
            order.stage,
            ddi_room::Run::Forfeit,
            ranking,
            false,
            Vec::new(),
        )
        .unwrap();
        assert_eq!(room.state().phase, Phase::MatchResults);
        assert_eq!(room.match_end(), None);
        assert!(room.mark_of(order.stage).is_none() && room.last_mark().is_none());
        let table = room.stage_table(order.stage).unwrap();
        assert_eq!(table.rows.len(), order.cpus.len());
        // The next match: still watching.
        room.pick(&s, &s.charts[0]).unwrap();
        assert!(room.watching());
        assert!(room.take_stage_order().unwrap().watching);
    }

    #[test]
    fn reported_stages_run_a_match_to_its_results_and_the_next_one() {
        let mut room = RoomClient::new(0, 1);
        let rules = MatchRules {
            stages: 2,
            ..room.rules().clone()
        };
        room.save_rules(rules).unwrap();
        assert!(!room.rules_seen());
        let s = song("a");
        let mut now = 0;
        let mut play = |room: &mut RoomClient, early: bool, me_first: bool| {
            room.pick(&s, &s.charts[0]).unwrap();
            let order = room.take_stage_order().unwrap();
            prepare(room);
            room.set_ready(true).unwrap();
            now += 3_000;
            room.tick(now);
            assert!(room.stage_started(order.stage));
            let mut ids: Vec<ParticipantId> = order.cpus.iter().map(|c| c.participant).collect();
            if me_first {
                ids.insert(0, order.me);
            } else {
                ids.push(order.me);
            }
            let ranking = ids
                .into_iter()
                .map(|p| {
                    vec![Finisher {
                        participant: p,
                        finish: Finish::Survived,
                    }]
                })
                .collect();
            room.report_stage(order.stage, ranking, early).unwrap();
            order
        };
        let first = play(&mut room, true, true);
        assert!(matches!(room.state().phase, Phase::Intermission { .. }));
        assert_eq!(room.unseen_results(), Some(Some(1)));
        room.results_shown();
        assert_eq!(room.unseen_results(), None);
        assert_eq!(room.next_pick_number(), 2);
        let mark = room.last_mark().unwrap();
        assert_eq!((mark.number, mark.placement.place, mark.field), (1, 1, 4));
        assert_eq!(mark.placement.points, ddi_room::Points::whole(12));
        assert!(!room.ready(), "the next stage asks for Ready again");
        assert_eq!(room.match_end(), None, "the match goes on");
        let second = play(&mut room, false, false);
        assert_ne!(first.stage, second.stage);
        // The same chart in both stages, each CPU seeded per stage
        // (`cpu-stage-seed-v1`): stage 1 with its own seed, stage 2
        // another, so the two are played two ways.
        for (a, b) in first.cpus.iter().zip(&second.cpus) {
            assert_eq!(a.participant, b.participant);
            let MemberKind::Cpu { opponent, .. } = &room.state().member(a.participant).unwrap().kind
            else {
                panic!("a CPU");
            };
            assert_eq!(a.seed, opponent.seed);
            assert_eq!(b.seed, ddi_room::OpponentSpec::stage_seed(opponent.seed, 2));
            assert_ne!(a.seed, b.seed);
        }
        assert_eq!(room.state().phase, Phase::MatchResults);
        assert_eq!(room.unseen_results(), Some(None), "the match's results");
        room.results_shown();
        assert_eq!(room.mark_of(second.stage).unwrap().placement.place, 4);
        let standings = room.state().standings().unwrap();
        let mine = standings
            .iter()
            .find(|s| s.participant == room.me())
            .unwrap();
        // First (12), then last (1) of four.
        assert_eq!(mine.total, ddi_room::Points::whole(13));
        assert_eq!(room.match_end(), Some(MatchEnd::NotWon));
        assert_eq!(room.next_pick_number(), 1, "the next match's first");
        play(&mut room, true, true);
        assert_eq!(room.state().history.len(), 1);
    }

    /// Each stage's table of a three-stage match with ties: its totals and
    /// ranks as the straightforward sums over the stages' placements say.
    #[test]
    fn stage_tables_total_and_rank_as_the_placements_sum() {
        let mut room = RoomClient::new(0, 1);
        let rules = MatchRules {
            stages: 3,
            ..room.rules().clone()
        };
        room.save_rules(rules).unwrap();
        let s = song("a");
        let mut now = 0;
        let mut stages = Vec::new();
        for round in 0..3usize {
            room.pick(&s, &s.charts[0]).unwrap();
            let order = room.take_stage_order().unwrap();
            prepare(&mut room);
            room.set_ready(true).unwrap();
            now += 3_000;
            room.tick(now);
            let mut ids: Vec<ParticipantId> = order.cpus.iter().map(|c| c.participant).collect();
            ids.insert(round % (ids.len() + 1), order.me);
            ids.rotate_left(round);
            // The first two tied in the second stage.
            let mut ranking: Vec<Vec<Finisher>> = Vec::new();
            for (k, p) in ids.into_iter().enumerate() {
                let f = Finisher {
                    participant: p,
                    finish: Finish::Survived,
                };
                match ranking.last_mut() {
                    Some(group) if round == 1 && k == 1 => group.push(f),
                    _ => ranking.push(vec![f]),
                }
            }
            room.report_stage(order.stage, ranking, true).unwrap();
            room.results_shown();
            stages.push(order.stage);
        }
        let m = room.state().current.clone().unwrap();
        let totals = |n: usize| -> BTreeMap<ParticipantId, ddi_room::Points> {
            m.roster
                .keys()
                .map(|&id| {
                    let total = m.stages[..n]
                        .iter()
                        .filter_map(|sg| sg.play.as_ref()?.result.as_ref()?.placement(id))
                        .map(|p| p.points)
                        .sum();
                    (id, total)
                })
                .collect()
        };
        let rank_in = |t: &BTreeMap<ParticipantId, ddi_room::Points>, id| {
            let mine = t.get(&id)?;
            Some(u16::try_from(t.values().filter(|x| *x > mine).count() + 1).unwrap())
        };
        let mut ranks_seen = Vec::new();
        for (index, stage) in stages.iter().enumerate() {
            let table = room.stage_table(*stage).unwrap();
            assert!(room.stage_confirmed(*stage));
            let (before, after) = (totals(index), totals(index + 1));
            for row in &table.rows {
                let id = row.participant;
                assert_eq!(row.total, after.get(&id).copied());
                assert_eq!(row.rank, rank_in(&after, id));
                let expected_before = (index > 0).then(|| rank_in(&before, id)).flatten();
                assert_eq!(row.rank_before, expected_before);
                ranks_seen.push(row.rank.unwrap());
            }
        }
        // Ties show: some rank is shared.
        assert!(ranks_seen.len() > 3);
    }

    /// A change of a later stage's rules that names no Points (as an older
    /// peer's, which cannot) keeps the Points the stage has as it stands,
    /// an earlier change's, never the match's by default; this client
    /// names them wherever they differ from the match's or the stage's,
    /// so a change back to the match's own is said (2026-11-13).
    #[test]
    fn a_change_naming_no_points_keeps_the_stage_s() {
        use ddi_room::PointsBias;
        let mut room = RoomClient::new(0, 1);
        let mut rules = room.rules().clone();
        rules.stages = 4;
        room.save_rules(rules).unwrap();
        let e = song("one");
        room.pick(&e, &e.charts[0]).unwrap();
        let (from, _) = room.rules_land_on().expect("a later stage");
        let mut medium = room.rules_ahead().unwrap();
        medium.points = PointsBias::Medium;
        room.change_stage_rules(from, &medium).unwrap();
        let points = |room: &RoomClient| room.rules_ahead().unwrap().points;
        assert_eq!(points(&room), PointsBias::Medium);
        // A change of the timing alone, naming no Points.
        let mut hard = room.rules_ahead().unwrap().play_rules();
        hard.difficulty = ddi_room::MatchDifficulty::Hard;
        assert_eq!(hard.points, None);
        room.send(Command::ChangeStageRules { from, rules: hard })
            .unwrap();
        let ahead = room.rules_ahead().unwrap();
        assert_eq!(ahead.difficulty, ddi_room::MatchDifficulty::Hard);
        assert_eq!(ahead.points, PointsBias::Medium, "the stage's kept");
        // Back to the match's own, through the client: said, and taken.
        let mut strong = room.rules_ahead().unwrap();
        strong.points = PointsBias::Strong;
        room.change_stage_rules(from, &strong).unwrap();
        assert_eq!(points(&room), PointsBias::Strong);
        let m = room.state().current.clone().unwrap();
        assert_eq!(m.changes.last().unwrap().rules.points, None, "kept in the one form");
    }

    /// The CPUs play the chart itself (`cpu-play-v1`): a chart they can
    /// plan as a tap-only copy but not whole (here: more notes than the
    /// full plan takes, mines past its limit) is refused when it is
    /// checked, never later when the stage makes its CPUs.
    #[test]
    fn a_chart_is_checked_as_the_cpus_play_it() {
        let text = "#TITLE:t;#BPMS:0.000=120.000;#OFFSET:0;\n#NOTES:\n     dance-single:\n     :\n     Hard:\n     9:\n     0,0,0,0,0:\n1000\n0100\n0010\n0001\n,\n1000\n0100\n0010\n0001\n;\n";
        let mut song = ddi_chart::formats::sm::parse_simfile(text, "sm").unwrap();
        let mut log = |_: &str| {};
        assert_eq!(check_chart(&song, 0, None, &mut log), Ok(()), "a chart they play whole");
        let limit = ddi_engine::cpu::MAX_AUTOMATIC_ROWS * 4;
        let first = song.charts[0].notes.last().unwrap().tick.0 + 48;
        song.charts[0].notes.extend((0..limit as i64).map(|k| ddi_chart::Note {
            tick: ddi_chart::Tick(first + k),
            lane: (k % 4) as u8,
            kind: ddi_chart::NoteKind::Mine,
            keysound: None,
            color: None,
            speed_mul: None,
        }));
        assert!(ddi_engine::cpu::Plan::automatic(&song, 0).is_ok(), "its tap-only copy planned");
        assert!(ddi_engine::cpu::PlannedRung::new(&song, 0).is_err(), "not whole");
        assert_eq!(
            check_chart(&song, 0, None, &mut log),
            Err(LoadProblem::CpuCannotPlay)
        );
    }
}
