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
//! (`ChooseContent`, pending); this client then asks the room for the
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

use ddi_library::share::ShareError;
use ddi_room::attacks::Targeting;
use ddi_room::{
    BattlePreset, Capability, ChartKey, ClientId, Command, ContentCard, ContentId, ContentSource,
    ContentStatus, Finisher, Identity, LinkState, Loading, LocalAuthority, ManualClock, MatchId,
    MatchRules, MemberKind, Mirror, OpponentSpec, ParticipantId, Phase, Placement, Points,
    Rejection, ReportedRun, Revision, Role, RoomAuthority, RoomId, RoomPolicy, RoomState,
    RulesProblem, Run, RunDetail, ServerMessage, SongKey, Stage, StageId, Transport,
};

use crate::state::online::SharedOnline;
use crate::state::online::link::{LinkNotice, OnlineLink};

use crate::screens::data::{ChartEntry, SongEntry};

/// The engine's CPU levels (`ddi_engine::cpu::CPU_LEVELS_V0`, ten),
/// shown as five stars (`crate::stars`): strength relative to the chart,
/// never a skill claim (`docs/plans/cpu-player.md`).
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

/// The stage id the spotlight's upload and download go under (stages are
/// numbered from 1): the server's grant for "no stage" (M7b).
pub const SPOTLIGHT: StageId = StageId(0);

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
    /// The host of a room whose authority does not pass rooms on: it
    /// stays open without a host.
    HostStays,
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
        [
            Self::Unreadable,
            Self::Changed,
            Self::CpuCannotPlay,
            Self::Download,
            Self::PlanMismatch,
        ]
        .into_iter()
        .find(|p| p.code() == code)
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
    pub live: Option<LiveAsk>,
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
    Prepared {
        job: Box<LoadJob>,
        result: Result<(), LoadProblem>,
    },
    PreparedShared {
        job: Box<LoadJob>,
        result: Result<Box<Picked>, LoadProblem>,
        /// What live play needs of the chart, where the job asked
        /// ([`prepare_stage`]).
        live: Option<LivePrep>,
    },
}

/// A shell's news since the last call.
pub type Inbox = Box<dyn FnMut() -> Vec<ShellNews>>;

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
    } else if job.chart >= song.charts.len()
        || job
            .digest
            .is_some_and(|d| song.chart_digest(job.chart) != d)
    {
        return Err(LoadProblem::Changed);
    }
    Ok(job.chart)
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
/// where the library had one) and the CPUs can plan it (the engine's
/// automatic plan built and checked: `ddi_engine::cpu::Plan`). The
/// engine's own words go to `log` (logs stay English); the room hears the
/// problem.
pub fn check_chart(
    song: &ddi_chart::Song,
    chart: usize,
    digest: Option<ddi_chart::ChartDigest>,
    log: &mut dyn FnMut(&str),
) -> Result<(), LoadProblem> {
    if chart >= song.charts.len() || digest.is_some_and(|d| song.chart_digest(chart) != d) {
        return Err(LoadProblem::Changed);
    }
    let plan = ddi_engine::cpu::Plan::automatic(song, chart).map_err(|e| {
        log(&e);
        LoadProblem::CpuCannotPlay
    })?;
    let level = ddi_engine::cpu::CPU_LEVELS_V0
        .level(DEFAULT_CPU_LEVEL.into())
        .ok_or(LoadProblem::CpuCannotPlay)?;
    plan.check_song(song, chart)
        .and_then(|_| plan.script_at(level, 0, ddi_engine::cpu::Skills::default()))
        .map(|_| ())
        .map_err(|e| {
            log(&e);
            LoadProblem::CpuCannotPlay
        })
}

/// A CPU seat in a stage: who, which CPU (from 1), its level (an index
/// into `ddi_engine::cpu::CPU_LEVELS_V0`'s ten) and seed.
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

/// A stage to play now, handed to the shell once its countdown is over
/// ([`RoomClient::take_stage_order`]): the library song and chart, the
/// stage's place in the match, the battle, the local player and the CPUs
/// committed to it.
#[derive(Clone, Debug, PartialEq)]
pub struct StageOrder {
    pub stage: StageId,
    /// The stage's number, from 1, and the match's stages.
    pub number: u16,
    pub of: u16,
    pub song: SongEntry,
    pub chart: ChartEntry,
    pub battle: BattlePreset,
    /// Attacks can be played in the stage's room (CPU practice): its
    /// prompt then says where the match has none.
    pub attacks_offered: bool,
    /// The match's rules the stage is judged by, everyone's (D0), played
    /// in place of the player's own.
    pub rules: StageRules,
    pub me: ParticipantId,
    /// The local player's attack policy as the order was made (their
    /// member's, `RoomClient::my_targeting`): the shells give the play the
    /// one they have as it starts (it may change on the stage's prompt).
    pub targeting: ddi_room::attacks::Targeting,
    pub cpus: Vec<CpuSeat>,
    /// DEVELOPMENT ONLY: a section of battle shocks put into the local
    /// player's play from this beat on ([`Development::mines_at_beat`]).
    pub dev_mines_at_beat: Option<u32>,
    /// Other people starting it, each on their own device (an online
    /// room's): the stage is decided by the room from everyone's reports,
    /// never on this device ([`RoomClient::report_run`]). Not shown during
    /// the play in this prototype.
    pub others: u16,
    /// The local player is not in the stage: they watch it
    /// ([`RoomClient::watches`]: CPU practice, the CPUs' plays judged on
    /// this device). `me` is then nobody's seat in it.
    pub watching: bool,
    /// An online room that plays the stage live
    /// (`docs/plans/multiplayer-online-play.md`): what this device worked
    /// out for it and who else plays it. Its song starts at the room's
    /// instant ([`RoomClient::song_zero_in`]), its plays are streamed and
    /// its attacks are the room's.
    pub live: Option<LiveOrder>,
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
}

impl StageRules {
    pub fn of(rules: &MatchRules) -> StageRules {
        StageRules {
            ruleset: rules.ruleset_id().into(),
            difficulty: rules.difficulty,
            boos: rules.boos,
        }
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
    pub fn build(&self) -> ddi_engine::Ruleset {
        let mut rules = ddi_engine::rules::presets::ddi().with_ddi(&self.ddi_options());
        rules.fail = ddi_engine::FailPolicy::Immediate;
        rules
    }
}

impl Default for StageRules {
    fn default() -> StageRules {
        StageRules::of(&MatchRules::hosted())
    }
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
    /// Its points came from an experimental rule.
    pub experimental: bool,
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
    /// The room confirmed the outcome: the places, awards and order are
    /// final. Before, the rows are as seated and fill in as reports come.
    pub confirmed: bool,
    /// The match's rules: the ruleset's tiers and score (the engine's
    /// id), its difficulty and Boos.
    pub rules: StageRules,
    /// The battle has attacks: the rows say those launched, those
    /// received and the shocks stepped on.
    pub attacks: bool,
    /// The local player's attacks of the stage, where it was played here
    /// with attacks ([`AttackSummary`]).
    pub summary: Option<AttackSummary>,
    /// The points came from an experimental rule.
    pub experimental: bool,
    pub rows: Vec<StageRow>,
    /// The match's result, where this was its last stage and the match is
    /// over: everyone in it with their place and total, best first.
    pub result: Option<Vec<(ParticipantId, u16, Points)>>,
}

/// The local player's attacks of a stage, for its results' short battle
/// summary (M8 round 3): how many attacks they sent each opponent and
/// were sent by each (in the order first sent), the shocks they stepped
/// on, and whether the stage was drawn by a final attack
/// (`SETTLEMENT_V0`). Kept by a practice's client beside the standings'
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
        // part and the absolute score (a CPU's put on the chart's scale
        // before it comes here, `ddi_session::battle::cpu_score`).
        score: match &score.ddi {
            Some(d) => Some(d.score),
            None => score.money.and_then(|m| i64::try_from(m).ok()),
        },
        percent: match &score.ddi {
            Some(d) => i32::try_from(d.percent).ok(),
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
    }
    .bounded()
}

/// Whether the CPUs can play `chart`: a one-player chart with four lanes,
/// which is what `ddi_engine::cpu` accepts. The final check (building the
/// CPU's plan from the loaded chart) is the stage's load, in the next
/// slice; a song-declared layout the library does not know is refused
/// here rather than guessed.
pub fn eligible(chart: &ChartEntry) -> Result<(), Unsupported> {
    match ddi_chart::Layout::builtin(&chart.style) {
        Some(l) if l.players == 1 && l.lanes.len() == 4 => Ok(()),
        _ => Err(Unsupported::Layout {
            style: chart.style.clone(),
        }),
    }
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

/// A room's client: the authority (the practice room's own, or an online
/// room's link), the client's mirror of it and what the device knows
/// about the content it names.
pub struct RoomClient {
    link: Link,
    /// An online room before its first snapshot: nobody in it yet.
    placeholder: Option<RoomState>,
    clock: ManualClock,
    client: ClientId,
    seq: u64,
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
    /// Stages handed to the shell to play.
    launched: BTreeSet<StageId>,
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
    /// This device's upload of the spotlight's song (an online picker's).
    spotlight_sharing: Option<(ContentId, Sharing)>,
    /// The spotlight's song as read from the room's share (a member's), and
    /// the content asked for.
    spotlight_loaded: Option<(ContentId, Picked)>,
    spotlight_asked: Option<ContentId>,
    /// The Rules pop-up opened by itself on entering (M7b).
    rules_seen: bool,
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
    live_news: Vec<ddi_room::live::LiveNews>,
    live_reader: bool,
    /// Stages prepared for live play whose Loaded waits on the room's
    /// battle plan, with the readiness revision prepared for.
    preparing: BTreeMap<StageId, (u32, LivePrep)>,
    /// The battle plans this device offered (once a stage).
    offered: BTreeSet<StageId>,
    /// What this device worked out for each stage it said it plays live.
    live_preps: BTreeMap<StageId, LivePrep>,
}

/// The room shared by the screens that show it (the room, its song
/// picker) and kept by the shell while the player goes elsewhere.
pub type SharedRoom = Rc<RefCell<RoomClient>>;

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
}

thread_local! {
    static DEVELOPMENT: std::cell::Cell<Development> =
        const { std::cell::Cell::new(Development { attacks: None, mines_at_beat: None }) };
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

impl RoomClient {
    /// A new practice room in its lobby: the local player, three CPUs at
    /// the default level, four empty stages (`MatchRules::practice`). The
    /// CPUs' seeds come from `seed` (the shells pass the time it was
    /// made, tests a fixed number).
    pub fn new(now_ms: u64, seed: u64) -> RoomClient {
        let rules = practice_rules(OpponentSpec {
            level: DEFAULT_CPU_LEVEL,
            seed,
            personality: None,
            name: None,
            targeting: None,
        });
        let roster = super::cpu_roster::starting(STARTING_CPUS, seed);
        let rules = super::cpu_roster::with_roster(rules, roster, 1);
        Self::practice_under(now_ms, rules, RoomPolicy::default())
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
        let mut rules = practice_rules(OpponentSpec {
            level: DEFAULT_CPU_LEVEL,
            seed,
            personality: None,
            name: None,
            targeting: None,
        });
        rules.stages = 1;
        rules.seats = cpus + 1;
        rules.cpu.mode = ddi_room::CpuMode::Fixed { count: cpus };
        rules.awards = ddi_room::AwardScheme::DevelopmentV0;
        Self::practice_under(now_ms, rules, policy)
    }

    fn practice_under(now_ms: u64, rules: MatchRules, policy: RoomPolicy) -> RoomClient {
        let clock = ManualClock::new(now_ms);
        // The player's name is the room's data; the screen calls the local
        // player "You" in the language shown.
        let (authority, client) =
            LocalAuthority::practice_under("Player".into(), rules, clock.clone(), policy);
        let me = authority.participant(&client).unwrap_or(ParticipantId(1));
        let mut room = RoomClient {
            link: Link::Local(Box::new(authority)),
            placeholder: None,
            clock,
            client,
            seq: 0,
            mirror: Mirror::new(),
            me,
            picks: BTreeMap::new(),
            refusal: None,
            version: 0,
            jobs: Vec::new(),
            asked: BTreeSet::new(),
            launched: BTreeSet::new(),
            sharing: BTreeMap::new(),
            uploads: Vec::new(),
            inbox: None,
            resting: None,
            spotlight_sharing: None,
            spotlight_loaded: None,
            spotlight_asked: None,
            rules_seen: false,
            results_seen: (None, 0),
            details: BTreeMap::new(),
            summaries: BTreeMap::new(),
            asking: Rc::default(),
            live_news: Vec::new(),
            live_reader: false,
            preparing: BTreeMap::new(),
            offered: BTreeSet::new(),
            live_preps: BTreeMap::new(),
        };
        room.pump();
        room
    }

    pub fn shared(now_ms: u64, seed: u64) -> SharedRoom {
        Rc::new(RefCell::new(RoomClient::new(now_ms, seed)))
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
        let mut room = RoomClient {
            link: Link::Online(Box::new(link)),
            placeholder: Some(placeholder),
            clock,
            client,
            seq: 0,
            mirror: Mirror::new(),
            me: ParticipantId(0),
            picks: BTreeMap::new(),
            refusal: None,
            version: 0,
            jobs: Vec::new(),
            asked: BTreeSet::new(),
            launched: BTreeSet::new(),
            sharing: BTreeMap::new(),
            uploads: Vec::new(),
            inbox: None,
            resting: None,
            spotlight_sharing: None,
            spotlight_loaded: None,
            spotlight_asked: None,
            rules_seen: false,
            results_seen: (None, 0),
            details: BTreeMap::new(),
            summaries: BTreeMap::new(),
            asking: Rc::default(),
            live_news: Vec::new(),
            live_reader: false,
            preparing: BTreeMap::new(),
            offered: BTreeSet::new(),
            live_preps: BTreeMap::new(),
        };
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
    /// who does not have it, in a room that has one picker at a time
    /// (never one whose authority predates it, which could not read the
    /// command), while connected.
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

    /// Whether this online room's authority passes a room on when its
    /// host goes (`ddi_room::Hosting`: the room says its setting). Not
    /// where it predates that: its commands are never sent there, and a
    /// host's leaving leaves the room without one. Practice: no.
    pub fn has_succession(&self) -> bool {
        self.is_online() && self.state().has_succession()
    }

    /// What the room does when its host goes (meaningful where
    /// [`RoomClient::has_succession`]).
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
        if !self.has_succession() || !self.manages_hosts() || self.if_host_leaves() == policy {
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

    /// Whether the owner may hand the room to `participant` now: a room
    /// that passes on, and someone there to run it (present, connected,
    /// a person).
    pub fn may_make_host(&self, participant: ParticipantId) -> bool {
        self.has_succession()
            && self.manages_hosts()
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

    /// The local player's role and admin flag.
    pub fn my_role(&self) -> Option<(Role, bool)> {
        self.state().member(self.me).map(|m| (m.role, m.admin))
    }

    /// The invite link to share (an online room's operators, once read).
    pub fn invite_link(&self) -> Option<String> {
        self.online_link().and_then(OnlineLink::invite_link)
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

    /// The owner replaces the online room's invite.
    pub fn rotate_invite(&mut self) {
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
            if !s.has_succession() {
                return Some(LeaveCost::HostStays);
            }
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
        } else if s.one_picker() && s.picker() == Some(self.me) {
            Some(LeaveCost::Picking)
        } else if me.role == Role::CoHost {
            Some(LeaveCost::CoHost)
        } else {
            None
        }
    }

    /// Leave an online room: the room told (the member's row kept with
    /// its results, marked left), the socket closed when it answers.
    pub fn leave(&mut self) {
        if self.is_online() && self.link_state() == LinkState::Connected {
            let _ = self.send(Command::Leave);
        } else if let Some(l) = self.online_link_mut() {
            l.authority.shut();
        }
    }

    /// The local player.
    pub fn me(&self) -> ParticipantId {
        self.me
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

    /// Time passes (milliseconds on the shell's clock, never backwards):
    /// the authority's deadlines and countdowns due by now happen.
    pub fn tick(&mut self, now_ms: u64) {
        if now_ms > self.now_ms() {
            self.clock.set(now_ms);
        }
        let before = self.link_state();
        if let Link::Online(l) = &mut self.link
            && l.tick(now_ms)
        {
            self.version += 1;
        }
        self.take_grants();
        self.read_inbox();
        self.pump();
        self.read_live();
        self.tell_spotlight();
        if self.link_state() != before {
            self.version += 1;
        }
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
        if self.resting.as_ref().map(|(id, _, _)| id) != next.as_ref().map(|(id, _)| id) {
            self.resting = next.map(|(id, card)| (id, card, now));
        }
    }

    /// Tells the room what the picker's song select has rested on.
    fn tell_spotlight(&mut self) {
        if !self.is_online() || self.link_state() != LinkState::Connected || !self.chooses() {
            return;
        }
        let Some((id, card, since)) = self.resting.clone() else {
            return;
        };
        let chart_has_digest = id.chart.as_str().parse::<ddi_chart::ChartDigest>().is_ok();
        if self.now_ms() < since + SPOTLIGHT_REST_MS || !chart_has_digest {
            return;
        }
        let shown = self.state().spotlight.as_ref();
        if shown.is_some_and(|s| s.content == id && s.by == self.me) {
            return;
        }
        // Once per rest: a refusal is not asked again until it moves.
        self.resting = None;
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
        if !self.picks.contains_key(&st.content.as_ref()?.id) {
            return None;
        }
        // Its battle plan is not the room's here: it sits the stage out
        // (the room's song select says why), nothing to be ready for.
        if st.play.is_none() && self.load_problem(stage) == Some(LoadProblem::PlanMismatch) {
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
            Some(play) => play.lineup.contains(&me),
            None => {
                s.candidates().contains(&me)
                    && s.member(me)
                        .is_some_and(|m| m.intent == ddi_room::Intent::Play)
            }
        }
    }

    /// Whether a stage the local player is not in is watched on this
    /// device (`watch-v0`): CPU practice, where the CPUs' plays are judged
    /// here and feed the spectator view; an online room that plays live,
    /// whose frames feed it (`docs/plans/multiplayer-online-play.md`,
    /// "Watching"). An online room that does not play live streams
    /// nothing: a watcher there stays on the room's song select.
    pub fn watches(&self) -> bool {
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
                .is_some_and(|p| p.lineup.contains(&self.me) || self.watches())
    }

    /// What leaving stage `stage`'s prompt means now; withdrawing clears
    /// this player's Ready (and the picker's pick: the window closed, back
    /// to choosing; another operator following the picker only withdraws).
    pub fn leave_prompt(&mut self, stage: StageId) -> PromptExit {
        self.launched.remove(&stage);
        if self.stage_started(stage) {
            return PromptExit::Forfeit;
        }
        if self.state().upcoming_stage() == Some(stage) {
            if self.ready() {
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
        // A watcher leaving a start already committed (the CPUs wait for
        // nobody): the start canceled, so that no stage runs unwatched.
        if !self.is_online()
            && self.watches()
            && !self.in_stage(stage)
            && self.may(Capability::Advance)
            && matches!(self.state().phase, Phase::Countdown { stage: p, .. } if p == stage)
        {
            let _ = self.cancel_start();
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
    /// from 1) or the match's (`None`), once confirmed.
    pub fn unseen_results(&self) -> Option<Option<u16>> {
        let s = self.state();
        let m = s.current.as_ref().filter(|_| s.phase != Phase::Lobby)?;
        let confirmed = m
            .stages
            .iter()
            .filter(|st| st.play.as_ref().is_some_and(|p| p.song_ended))
            .count();
        let over = matches!(s.phase, Phase::MatchResults | Phase::Interrupted { .. });
        let seen = match self.results_seen {
            (Some(id), n) if id == m.id => n,
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
        let s = self.state();
        let Some(m) = s.current.as_ref() else {
            return;
        };
        let confirmed = m
            .stages
            .iter()
            .filter(|st| st.play.as_ref().is_some_and(|p| p.song_ended))
            .count();
        let over = matches!(s.phase, Phase::MatchResults | Phase::Interrupted { .. });
        self.results_seen = (Some(m.id), confirmed + usize::from(over));
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
            // No CPUs in a hosted room; its content must be named by the
            // chart's digest, which every participant can check.
            if chart.digest.is_none() {
                return Err(Unsupported::NoDigest);
            }
            Ok(())
        } else {
            eligible(chart)
        }
    }

    /// Whether the room can play charts of style `style` at all: a
    /// practice's CPUs play one-player charts with four lanes
    /// ([`eligible`]); an online room has none and plays any.
    pub fn style_playable(&self, style: &str) -> bool {
        self.is_online()
            || matches!(
                ddi_chart::Layout::builtin(style),
                Some(l) if l.players == 1 && l.lanes.len() == 4
            )
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
        self.send(Command::Pick {
            content: id,
            card: Some(Box::new(card_of(song, chart))),
        })
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
        let Some(stage) = self.state().upcoming_stage() else {
            return self.refuse(Refusal::Room(Rejection::NotUpcoming));
        };
        let readiness_rev = self.readiness_rev(stage);
        self.send(Command::SetReady {
            stage,
            readiness_rev,
            ready,
        })
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
    /// rules say (`crate::screens::room::attacks_offered`).
    pub fn attacks_offered(&self) -> bool {
        !self.is_online() || self.plays_live()
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
        self.attacks_played(self.locked_rules().unwrap_or_else(|| self.rules()))
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
            matches!(
                s.prep(self.me).loading,
                Loading::Loading { .. } | Loading::NotStarted
            ) && s
                .content
                .as_ref()
                .is_some_and(|c| c.status == ContentStatus::Available)
        })
    }

    /// The songs to prepare, once each (the shell's to answer).
    pub fn take_load_jobs(&mut self) -> Vec<LoadJob> {
        std::mem::take(&mut self.jobs)
    }

    /// The shell prepared `job`: the stage's song reported loaded, or
    /// failed with why. An answer for a song replaced since is dropped
    /// (the new one has its own job).
    pub fn finish_load(&mut self, job: &LoadJob, result: Result<(), LoadProblem>) {
        if self.readiness_rev(job.stage) != job.readiness_rev {
            return;
        }
        self.preparing.remove(&job.stage);
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

    /// The shell prepared a shared song's `job` (an online member's): the
    /// song and chart as read from the room's share, or why not. The stage
    /// is reported loaded once its chart is the one chosen.
    pub fn finish_shared_load(&mut self, job: &LoadJob, result: Result<Picked, LoadProblem>) {
        if job.stage == SPOTLIGHT {
            // The spotlight's song for the others to see and hear; a
            // failure leaves its card alone.
            if let (Ok(picked), Some(asked)) = (result, self.spotlight_asked.clone()) {
                self.spotlight_loaded = Some((asked, picked));
                self.version += 1;
            }
            return;
        }
        if self.readiness_rev(job.stage) != job.readiness_rev {
            return;
        }
        let content = self
            .state()
            .stage(job.stage)
            .and_then(|s| s.content.as_ref())
            .map(|c| c.id.clone());
        let result = match (result, content) {
            (Ok(picked), Some(content)) => {
                self.picks.insert(content, picked);
                Ok(())
            }
            (Ok(_), None) => return,
            (Err(p), _) => Err(p),
        };
        self.finish_load(job, result);
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
        if self.readiness_rev(job.stage) != job.readiness_rev {
            return;
        }
        let content = self
            .state()
            .stage(job.stage)
            .and_then(|s| s.content.as_ref())
            .map(|c| c.id.clone());
        match (result, content) {
            (Ok(picked), Some(content)) => {
                self.picks.insert(content, picked);
                self.finish_load_live(job, Ok(prep));
            }
            (Ok(_), None) => {}
            (Err(p), _) => self.finish_load(job, Err(p)),
        }
    }

    /// What a stage's preparation should work out for live play here
    /// (none: the room does not play live).
    fn live_ask(&self, stage: StageId, content: &ContentId) -> Option<LiveAsk> {
        if !self.plays_live() {
            return None;
        }
        let rules = self.locked_rules().unwrap_or_else(|| self.rules()).clone();
        let settings = crate::state::settings::Settings::load();
        Some(LiveAsk {
            seed: ddi_room::live::plan::seed_for(&self.state().room, stage, &content.chart),
            rules: StageRules::of(&rules),
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
        let pending: Vec<(StageId, u32, LivePrep)> = self
            .preparing
            .iter()
            .map(|(s, (rev, prep))| (*s, *rev, prep.clone()))
            .collect();
        for (stage, rev, prep) in pending {
            // Taken meanwhile (a command's answer ran this again).
            if !self.preparing.contains_key(&stage) {
                continue;
            }
            let Some(st) = self.state().stage(stage) else {
                self.preparing.remove(&stage);
                continue;
            };
            if st.readiness_rev != rev || st.play.is_some() {
                self.preparing.remove(&stage);
                continue;
            }
            let room_plan = st.content.as_ref().and_then(|c| c.battle.clone());
            let digest = match (&prep.plan, room_plan) {
                (None, _) => None,
                (Some(plan), None) => {
                    // The picker's device offers it, once; the others
                    // wait for the room to say which it took.
                    if self.chooses() && self.offered.insert(stage)
                        && let Some(link) = self.online_link_mut()
                    {
                        link.authority
                            .live_mut()
                            .send(ddi_room::live::ClientLive::OfferPlan {
                                stage,
                                plan: Box::new(plan.clone()),
                            });
                    }
                    continue;
                }
                (Some(plan), Some(room)) if room == plan.digest => Some(room),
                (Some(_), Some(_)) => {
                    self.preparing.remove(&stage);
                    self.finish_mismatch(stage, rev);
                    continue;
                }
            };
            self.preparing.remove(&stage);
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
                ShellNews::Prepared { job, result } => self.finish_load(&job, result),
                ShellNews::PreparedShared { job, result, live } => {
                    self.finish_shared_load_live(&job, result.map(|p| (*p, live)))
                }
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
            if let Some((_, s)) = &mut self.spotlight_sharing {
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
            && (matches!(self.sharing(stage), Some(Sharing::Failed(_))) || refused)
    }

    /// Grants the room answered: an upload handed to the shell, the room's
    /// earlier share taken (nothing to upload), or the refusal kept.
    fn take_grants(&mut self) {
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
            let next = match (n.answer, content) {
                (Ok(answer), Some(content)) => match (answer.grant, answer.share) {
                    (Some(grant), _) => match self.picks.get(&content) {
                        Some(p) => {
                            self.uploads.push(UploadJob {
                                stage: n.stage,
                                content,
                                song: p.song.clone(),
                                chart: p.chart.clone(),
                                grant,
                                max_bytes: answer.max_bytes,
                            });
                            Sharing::Uploading {
                                done: 0.0,
                                total: 0.0,
                            }
                        }
                        None => continue,
                    },
                    (None, Some(_)) => Sharing::Sent,
                    (None, None) => Sharing::Failed(SharingProblem::Grant("no_grant".into())),
                },
                (Ok(_), None) => continue,
                (Err(p), _) => Sharing::Failed(SharingProblem::Grant(p.code)),
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
        let next = match answer {
            Ok(answer) => match (answer.grant, self.picks.get(&content)) {
                (Some(grant), Some(p)) => {
                    self.uploads.push(UploadJob {
                        stage: SPOTLIGHT,
                        content: content.clone(),
                        song: p.song.clone(),
                        chart: p.chart.clone(),
                        grant,
                        max_bytes: answer.max_bytes,
                    });
                    Sharing::Uploading {
                        done: 0.0,
                        total: 0.0,
                    }
                }
                (None, _) if answer.share.is_some() => Sharing::Sent,
                _ => Sharing::Failed(SharingProblem::Grant("no_grant".into())),
            },
            Err(p) => Sharing::Failed(SharingProblem::Grant(p.code)),
        };
        self.spotlight_sharing = Some((content, next));
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

    /// Stage `stage`'s standings as its results show them
    /// ([`StageTable`]): every starter, what the room confirmed of them
    /// and how they performed, the match's totals after it, and the
    /// match's result where it was its last.
    pub fn stage_table(&self, stage: StageId) -> Option<StageTable> {
        let (m, index, completed) = self.match_of(stage)?;
        let st = &m.stages[index];
        let play = st.play.as_ref()?;
        let local = self.details.get(&stage);
        // The match's totals over its first `n` stages.
        let totals = |n: usize| -> BTreeMap<ParticipantId, Points> {
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
        let rank_in = |totals: &BTreeMap<ParticipantId, Points>, id: ParticipantId| {
            let mine = totals.get(&id)?;
            let above = totals.values().filter(|t| *t > mine).count();
            Some(u16::try_from(above + 1).unwrap_or(u16::MAX))
        };
        let (before, after) = (totals(index), totals(index + 1));
        let confirmed = play.result.is_some();
        let row = |id: ParticipantId| {
            let detail = local
                .and_then(|d| d.get(&id))
                .or_else(|| play.details.get(&id))
                .cloned();
            let placement = play.result.as_ref().and_then(|r| r.placement(id)).copied();
            StageRow {
                participant: id,
                placement,
                run: play.reports.get(&id).map(|r| r.run()),
                detail,
                total: confirmed.then(|| after.get(&id).copied()).flatten(),
                rank: confirmed.then(|| rank_in(&after, id)).flatten(),
                rank_before: (confirmed && index > 0)
                    .then(|| rank_in(&before, id))
                    .flatten(),
            }
        };
        // The stage's order once confirmed; before, as they were seated.
        let rows: Vec<StageRow> = match &play.result {
            Some(result) => result
                .placements
                .iter()
                .map(|p| row(p.participant))
                .collect(),
            None => play.lineup.iter().map(|id| row(*id)).collect(),
        };
        let last = index + 1 == m.stages.len();
        let result = (last && confirmed && completed).then(|| {
            m.standings()
                .into_iter()
                .map(|st| (st.participant, st.rank, st.total))
                .collect()
        });
        Some(StageTable {
            stage,
            number: u16::try_from(index + 1).unwrap_or(u16::MAX),
            of: u16::try_from(m.stages.len()).unwrap_or(u16::MAX),
            confirmed,
            rules: StageRules::of(&m.rules),
            attacks: self.attacks_played(&m.rules),
            summary: self.summaries.get(&stage).cloned(),
            experimental: play.result.as_ref().is_some_and(|r| r.rule.experimental()),
            rows,
            result,
        })
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
                MemberKind::Cpu { ordinal, opponent } => Some(CpuSeat {
                    participant: *id,
                    ordinal: *ordinal,
                    level: opponent.level,
                    seed: opponent.seed,
                    name: opponent.name.clone(),
                    skills: opponent.personality.clone(),
                    targeting: opponent.targeting.unwrap_or_default(),
                }),
                MemberKind::Human { .. } => None,
            })
            .collect();
        let order = StageOrder {
            stage,
            number: u16::try_from(index + 1).unwrap_or(u16::MAX),
            of: u16::try_from(m.stages.len()).unwrap_or(u16::MAX),
            song: picked.song,
            chart: picked.chart,
            // What is really played: attacks only where they are
            // offered (never a rule that silently does nothing).
            battle: if self.attacks_played(&m.rules) {
                BattlePreset::GaugeAttacksV0
            } else {
                BattlePreset::GaugeSurvivalV0
            },
            attacks_offered: self.attacks_offered(),
            rules: StageRules::of(&m.rules),
            dev_mines_at_beat: development().mines_at_beat,
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
        };
        self.launched.insert(stage);
        Some(order)
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
            // Online stages are not played in this slice.
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
        m.stages.iter().enumerate().rev().find_map(|(i, st)| {
            let play = st.play.as_ref()?;
            let result = play.result.as_ref()?;
            Some(StageMark {
                number: u16::try_from(i + 1).unwrap_or(u16::MAX),
                placement: *result.placement(self.me)?,
                field: u16::try_from(play.lineup.len()).unwrap_or(u16::MAX),
                experimental: result.rule.experimental(),
            })
        })
    }

    /// The local player's result of stage `stage`, once confirmed.
    pub fn mark_of(&self, stage: StageId) -> Option<StageMark> {
        let s = self.state();
        let m = s.current.as_ref()?;
        let i = m.stage_index(stage)?;
        let play = m.stages[i].play.as_ref()?;
        let result = play.result.as_ref()?;
        Some(StageMark {
            number: u16::try_from(i + 1).unwrap_or(u16::MAX),
            placement: *result.placement(self.me)?,
            field: u16::try_from(play.lineup.len()).unwrap_or(u16::MAX),
            experimental: result.rule.experimental(),
        })
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

    /// From a finished or stopped match back to the lobby, the next match's
    /// rules applied.
    pub fn open_next_match(&mut self) -> Result<(), Refusal> {
        self.send(Command::OpenNextMatch)
    }

    /// Save the next match's rules (in the lobby, the rules the match will
    /// start with; during a match, a draft that leaves it alone).
    pub fn save_rules(&mut self, rules: MatchRules) -> Result<(), Refusal> {
        self.send(Command::SaveNextRules { rules })
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
        self.seq += 1;
        let seq = self.seq;
        let base = self.mirror.state().map(|s| s.revision);
        let client = self.client.clone();
        self.authority_mut()
            .submit(ddi_room::ClientEnvelope::new(client, seq, base, cmd));
        let answer = self.pump_for(seq);
        match answer {
            Some(Some(rejection)) => self.refuse(Refusal::Room(rejection)),
            _ => {
                self.refusal = None;
                self.version += 1;
                self.sync_loading();
                Ok(())
            }
        }
    }

    fn pump(&mut self) {
        self.pump_for(0);
        self.sync_loading();
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
                ServerMessage::Reject {
                    client,
                    seq: s,
                    rejection,
                } if *client == self.client && *s == seq => {
                    answer = Some(Some(rejection.clone()));
                }
                ServerMessage::Reject {
                    client, rejection, ..
                } if *client == self.client => {
                    // An earlier command's refusal, come later (online).
                    self.refusal = Some(Refusal::Room(rejection.clone()));
                    self.version += 1;
                }
                ServerMessage::Events { .. } | ServerMessage::Snapshot { .. } => {
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

    /// What the room's live side said since the last tick: kept for the
    /// stage being played, which reads it (`take_live_news`); nothing is
    /// kept while no stage is played live here, so that it never piles up.
    fn read_live(&mut self) {
        let Link::Online(l) = &mut self.link else {
            return;
        };
        let news = l.authority.live_mut().take_news();
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
        if self.live_reader {
            self.live_news.extend(news);
        }
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
        self.live_reader = true;
        std::mem::take(&mut self.live_news)
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
        self.live_reader = false;
        self.live_news.clear();
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
            self.seq += 1;
            let seq = self.seq;
            let base = self.mirror.state().map(|s| s.revision);
            let client = self.client.clone();
            self.authority_mut().submit(ddi_room::ClientEnvelope::new(
                client,
                seq,
                base,
                Command::ReportLoading {
                    stage,
                    readiness_rev,
                    loading: Loading::Loading { permille: 0 },
                },
            ));
            if !matches!(self.pump_for(seq), Some(None)) {
                return;
            }
            if let Some(p) = self.picks.get(&content)
                && self.asked.insert((stage, readiness_rev))
            {
                self.jobs.push(LoadJob {
                    stage,
                    readiness_rev,
                    song: p.song.id.clone(),
                    chart: p.chart.index,
                    digest: p.chart.digest,
                    share: None,
                    cpus: true,
                    live: None,
                });
            }
        }
    }

    /// The spotlight's song (M7b): the picker who rests on it shares it
    /// ahead under the room's grant (once per song), so that its pick is
    /// available at once; the others read the room's share of it, to show
    /// its banner and play its preview before anything is picked.
    fn sync_spotlight(&mut self) {
        let Some(spot) = self.state().spotlight.clone() else {
            // The spotlight went with its picker (the pick changed hands):
            // a song rested on again is asked for again, once, and the
            // room answers with the share it kept or made meanwhile.
            if matches!(
                self.spotlight_sharing,
                Some((_, Sharing::Sent | Sharing::Failed(_)))
            ) {
                self.spotlight_sharing = None;
            }
            return;
        };
        if spot.by == self.me {
            let asked = self
                .spotlight_sharing
                .as_ref()
                .is_some_and(|(c, _)| *c == spot.content);
            if spot.share.is_none() && !asked && self.picks.contains_key(&spot.content) {
                self.spotlight_sharing = Some((spot.content.clone(), Sharing::Asking));
                if let Some(l) = self.online_link_mut() {
                    l.request_grant(SPOTLIGHT, Revision(0));
                }
            }
            return;
        }
        let Some(share) = spot.share else {
            return;
        };
        if self.picks.contains_key(&spot.content)
            || self.spotlight_asked.as_ref() == Some(&spot.content)
        {
            return;
        }
        self.spotlight_asked = Some(spot.content.clone());
        self.jobs.push(LoadJob {
            stage: SPOTLIGHT,
            readiness_rev: 0,
            song: ddi_library::share::song_id(&share, 0),
            chart: 0,
            digest: spot.content.chart.as_str().parse().ok(),
            share: Some(share),
            cpus: false,
            live: None,
        });
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
        self.sync_spotlight();
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
                (ContentStatus::Available, Some(share))
                    if st.prep(me).loading == Loading::NotStarted =>
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
            let digest = content.chart.as_str().parse().ok();
            let live = self.live_ask(stage, &content);
            let job = match self.picks.get(&content) {
                Some(p) => LoadJob {
                    stage,
                    readiness_rev,
                    song: p.song.id.clone(),
                    chart: p.chart.index,
                    digest,
                    share: None,
                    cpus: false,
                    live,
                },
                None => LoadJob {
                    stage,
                    readiness_rev,
                    song: ddi_library::share::song_id(&share, 0),
                    chart: 0,
                    digest,
                    share: Some(share),
                    cpus: false,
                    live,
                },
            };
            self.jobs.push(job);
        }
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
        assert_eq!(room.max_cpus(), 8, "nine competitors, the player included");
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

    #[test]
    fn rules_follow_the_cap_and_the_roster() {
        let mut room = RoomClient::new(0, 1);
        assert_eq!((room.min_cpus(), room.max_cpus()), (1, 8));
        let roster = crate::state::cpu_roster::at_level(&[], 4, 8, 3);
        let mut rules = crate::state::cpu_roster::with_roster(room.rules().clone(), roster, 1);
        rules.stages = 2;
        room.save_rules(rules.clone()).unwrap();
        assert_eq!(cpus(&room), 8);
        assert_eq!(room.stages().len(), 2);
        let mut over = rules.clone();
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
        assert!(mark.experimental);
        assert!(!room.ready(), "the next stage asks for Ready again");
        assert_eq!(room.match_end(), None, "the match goes on");
        let second = play(&mut room, false, false);
        assert_ne!(first.stage, second.stage);
        assert_eq!(room.state().phase, Phase::MatchResults);
        assert_eq!(room.unseen_results(), Some(None), "the match's results");
        room.results_shown();
        assert_eq!(room.mark_of(second.stage).unwrap().placement.place, 4);
        let standings = room.state().standings().unwrap();
        let mine = standings
            .iter()
            .find(|s| s.participant == room.me())
            .unwrap();
        assert_eq!(mine.total, ddi_room::Points::whole(5));
        assert_eq!(room.match_end(), Some(MatchEnd::NotWon));
        assert_eq!(room.next_pick_number(), 1, "the next match's first");
        play(&mut room, true, true);
        assert_eq!(room.state().history.len(), 1);
    }
}
