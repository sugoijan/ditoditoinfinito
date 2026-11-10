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
//! M5d, online rooms: the operator's pick is the room's commit
//! (`ChooseContent`, pending); this client then asks the room for the
//! stage's upload grant and hands the shell an [`UploadJob`] (its song
//! uploaded as the room's share with the shell's own sharing code), whose
//! progress and end the shell reports ([`RoomClient::upload_progress`],
//! [`RoomClient::upload_finished`]); a failure is said with a retry. Once
//! the room says the stage is available with its share, every member
//! prepares it: the operator who chose it from its library, the others
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
use ddi_room::{
    BattlePreset, Capability, ChartKey, ClientId, Command, ContentId, ContentSource, ContentStatus,
    CpuMode, Finisher, Identity, LinkState, Loading, LocalAuthority, ManualClock, MatchRules,
    MemberKind, Mirror, OpponentSpec, ParticipantId, Phase, Placement, Rejection, ReportedRun,
    Revision, Role, RoomAuthority, RoomId, RoomPolicy, RoomState, RulesProblem, Run, ServerMessage,
    SongKey, Stage, StageId, Transport,
};

use crate::state::online::SharedOnline;
use crate::state::online::link::{LinkNotice, OnlineLink};

use crate::screens::data::{ChartEntry, SongEntry};

/// The engine's CPU levels (`ddi_engine::cpu::STRENGTHS`), shown as
/// "Level 1" to "Level 5": strength relative to the chart, never a skill
/// claim (`docs/plans/cpu-player.md`).
pub fn cpu_levels() -> u8 {
    ddi_engine::cpu::STRENGTHS.len() as u8
}

/// Practice's starting CPU level: the engine's default strength (its
/// `normal`, the third of five). PROVISIONAL: a convenient start, not a
/// calibrated profile.
pub const DEFAULT_CPU_LEVEL: u8 = 2;

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
}

impl LoadProblem {
    /// Its code in the room's loading report.
    pub fn code(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::Changed => "changed",
            Self::CpuCannotPlay => "cpu-plan",
            Self::Download => "download",
        }
    }

    pub fn from_code(code: &str) -> Option<LoadProblem> {
        [
            Self::Unreadable,
            Self::Changed,
            Self::CpuCannotPlay,
            Self::Download,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadJob {
    pub stage: StageId,
    pub readiness_rev: u32,
    pub song: String,
    pub chart: usize,
    pub digest: Option<ddi_chart::ChartDigest>,
    pub share: Option<String>,
    /// CPUs play it: their plan is checked too (practice).
    pub cpus: bool,
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
    plan.check_song(song, chart)
        .and_then(|_| plan.script(DEFAULT_CPU_LEVEL.into(), 0))
        .map(|_| ())
        .map_err(|e| {
            log(&e);
            LoadProblem::CpuCannotPlay
        })
}

/// A CPU seat in a stage: who, which CPU (from 1), its level (an index
/// into `ddi_engine::cpu::STRENGTHS`) and seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpuSeat {
    pub participant: ParticipantId,
    pub ordinal: u16,
    pub level: u8,
    pub seed: u64,
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
    pub me: ParticipantId,
    pub cpus: Vec<CpuSeat>,
    /// Other people starting it, each on their own device (an online
    /// room's): the stage is decided by the room from everyone's reports,
    /// never on this device ([`RoomClient::report_run`]). Not shown during
    /// the play in this prototype.
    pub others: u16,
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
}

/// The room shared by the screens that show it (the room, its song
/// picker) and kept by the shell while the player goes elsewhere.
pub type SharedRoom = Rc<RefCell<RoomClient>>;

impl RoomClient {
    /// A new practice room in its lobby: the local player, three CPUs at
    /// the default level, four empty stages (`MatchRules::practice`). The
    /// CPUs' seeds come from `seed` (the shells pass the time it was
    /// made, tests a fixed number).
    pub fn new(now_ms: u64, seed: u64) -> RoomClient {
        let rules = MatchRules::practice(OpponentSpec {
            level: DEFAULT_CPU_LEVEL,
            seed,
            personality: None,
        });
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
        let mut rules = MatchRules::practice(OpponentSpec {
            level: DEFAULT_CPU_LEVEL,
            seed,
            personality: None,
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
        if self.link_state() != before {
            self.version += 1;
        }
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

    /// Choose `song`'s `chart` for `stage`: refused when the CPUs cannot
    /// play it, or when the room refuses (a started stage, a stale view).
    pub fn choose(
        &mut self,
        stage: StageId,
        song: &SongEntry,
        chart: &ChartEntry,
    ) -> Result<(), Refusal> {
        if self.is_online() {
            // No CPUs in a hosted room; its content must be named by the
            // chart's digest, which every participant can check.
            if chart.digest.is_none() {
                return self.refuse(Refusal::Chart(Unsupported::NoDigest));
            }
        } else if let Err(why) = eligible(chart) {
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
        self.send(Command::ChooseContent {
            stage,
            content: Some(id),
        })
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

    /// Whether the local player is ready for the upcoming stage.
    pub fn ready(&self) -> bool {
        let s = self.state();
        s.upcoming_stage()
            .and_then(|id| s.stage(id))
            .is_some_and(|stage| stage.prep(self.me).ready)
    }

    /// Start: in the lobby the match (its rules and roster locked) with its
    /// first stage, one command the room refuses whole; between stages
    /// the next one.
    pub fn start(&mut self) -> Result<(), Refusal> {
        let Some(stage) = self.state().upcoming_stage() else {
            return self.refuse(Refusal::Room(Rejection::WrongPhase {
                phase: self.state().phase.kind(),
            }));
        };
        let readiness_rev = self.readiness_rev(stage);
        // This device's own preparation: failed, or not done yet (a
        // window would wait its whole length for it).
        if let Some(p) = self.load_problem(stage) {
            return self.refuse(Refusal::Load(p));
        }
        if self.loading(stage) {
            return self.refuse(Refusal::Preparing);
        }
        // Practice words its own refusal before the press (the room would
        // say the same); online, the others' readiness is the room's.
        if self.state().phase == Phase::Lobby
            && !self.is_online()
            && let Some(why) = self.start_blocker(stage)
        {
            return self.refuse(Refusal::Room(why));
        }
        self.send(Command::StartStage {
            stage,
            readiness_rev,
        })
    }

    /// Why Start would be refused now, as the room would say it: no song,
    /// or no ready player. The room decides; this only words it before
    /// the press.
    pub fn start_blocker(&self, stage: StageId) -> Option<Rejection> {
        let s = self.state();
        let st = s.stage(stage)?;
        match &st.content {
            None => return Some(Rejection::NoContent),
            Some(c) if c.status == ContentStatus::Pending => {
                return Some(Rejection::ContentPending);
            }
            _ => {}
        }
        (!st.prep(self.me).ready).then_some(Rejection::NotEnoughReady {
            ready_humans: 0,
            competitors: 0,
        })
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
                ShellNews::PreparedShared { job, result } => {
                    self.finish_shared_load(&job, result.map(|p| *p))
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
        if let Some((_, s @ Sharing::Uploading { .. })) = self.sharing.get_mut(&stage) {
            *s = Sharing::Uploading { done, total };
            self.version += 1;
        }
    }

    /// The upload of stage `stage`'s song ended: the share made (the room
    /// hears it from the server, not from here), or why not. An expired
    /// grant is renewed and the upload made again.
    pub fn upload_finished(&mut self, stage: StageId, result: Result<String, ShareError>) {
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
        self.may(Capability::ChooseContent)
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

    /// Report one's own run of stage `stage` (an online room's: it ranks
    /// everyone's reports).
    pub fn report_run(&mut self, stage: StageId, run: Run) -> Result<(), Refusal> {
        self.send(Command::ReportOutcome {
            stage,
            run: ReportedRun::of(run),
        })
    }

    /// The stage whose countdown just ended, to play now: handed over
    /// once (a stage is never played twice).
    pub fn take_stage_order(&mut self) -> Option<StageOrder> {
        let s = self.state();
        let Phase::Stage { stage } = s.phase else {
            return None;
        };
        if self.launched.contains(&stage) {
            return None;
        }
        let m = s.current.as_ref()?;
        let index = m.stage_index(stage)?;
        let st = &m.stages[index];
        let play = st.play.as_ref()?;
        if !play.lineup.contains(&self.me) {
            return None;
        }
        let picked = self.picked(&st.content.as_ref()?.id)?.clone();
        let cpus = play
            .lineup
            .iter()
            .filter_map(|id| match &s.member(*id)?.kind {
                MemberKind::Cpu { ordinal, opponent } => Some(CpuSeat {
                    participant: *id,
                    ordinal: *ordinal,
                    level: opponent.level,
                    seed: opponent.seed,
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
            battle: BattlePreset::parse(&m.rules.battle).unwrap_or(BattlePreset::GaugeSurvivalV0),
            me: self.me,
            cpus,
            others: u16::try_from(
                play.lineup
                    .iter()
                    .filter(|id| **id != self.me)
                    .filter(|id| s.member(**id).is_some_and(|m| !m.is_cpu()))
                    .count(),
            )
            .unwrap_or(u16::MAX),
        };
        self.launched.insert(stage);
        Some(order)
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

    /// `rules` with `cpus` fixed CPU opponents, the seats following.
    pub fn with_cpus(&self, mut rules: MatchRules, cpus: u16) -> MatchRules {
        let humans = u16::try_from(self.state().reserved_humans().max(1)).unwrap_or(u16::MAX);
        rules.cpu.mode = CpuMode::Fixed { count: cpus };
        rules.seats = humans.saturating_add(cpus);
        rules
    }

    /// How many CPUs `rules` seat with the people who play now.
    pub fn cpus_of(&self, rules: &MatchRules) -> u16 {
        let humans = self.state().reserved_humans().max(1);
        u16::try_from(rules.cpu_seats(humans)).unwrap_or(u16::MAX)
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
        if let Link::Online(l) = &self.link
            && let Some(m) = self.state().member_by_identity(l.identity())
        {
            self.me = m.id;
        }
        answer
    }

    /// Practice's content is the device's own library: every unstarted
    /// stage it has a song for whose preparation has not begun (again
    /// after a change cleared it) is reported loading and handed to the
    /// shell to prepare ([`Self::take_load_jobs`]).
    fn sync_loading(&mut self) {
        if self.is_online() {
            return self.sync_online();
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
                });
            }
        }
    }

    /// An online room's content (M5d). An operator's own pick waiting for
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
        let me = self.me;
        let operator = self.may(Capability::ChooseContent);
        let mut ask = Vec::new();
        let mut prepare = Vec::new();
        for st in self.stages().iter().filter(|st| st.play.is_none()) {
            let Some(content) = &st.content else {
                continue;
            };
            match (&content.status, &content.share) {
                (ContentStatus::Pending, _)
                    if operator
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
            let job = match self.picks.get(&content) {
                Some(p) => LoadJob {
                    stage,
                    readiness_rev,
                    song: p.song.id.clone(),
                    chart: p.chart.index,
                    digest,
                    share: None,
                    cpus: false,
                },
                None => LoadJob {
                    stage,
                    readiness_rev,
                    song: ddi_library::share::song_id(&share, 0),
                    chart: 0,
                    digest,
                    share: Some(share),
                    cpus: false,
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

    #[test]
    fn practice_seats_eight_opponents_and_development_far_more() {
        let room = RoomClient::new(0, 7);
        assert_eq!(room.max_cpus(), 8, "nine competitors, the player included");
        let dev = RoomClient::development(0, 7, 64);
        assert_eq!(dev.cpus_of(dev.rules()), 64);
        assert!(dev.check_rules(dev.rules()).is_ok());
        assert_eq!(dev.rules().awards, ddi_room::AwardScheme::DevelopmentV0);
        assert_eq!(dev.max_cpus(), 128);
        // Never beyond the development cap.
        let most = RoomClient::development(0, 7, 1000);
        assert_eq!(most.cpus_of(most.rules()), 128);
    }

    #[test]
    fn choosing_reports_the_local_content_loaded_and_refuses_what_cpus_cannot_play() {
        let mut room = RoomClient::new(1_000, 7);
        let first = room.stages()[0].id;
        let s = song("a");
        assert_eq!(
            room.choose(first, &s, &s.charts[1]),
            Err(Refusal::Chart(Unsupported::Layout {
                style: "dance-double".into()
            }))
        );
        assert!(room.stages()[0].content.is_none());
        room.choose(first, &s, &s.charts[0]).unwrap();
        let st = &room.stages()[0];
        assert_eq!(
            st.content.as_ref().unwrap().id,
            content_id(&s, &s.charts[0])
        );
        // Loading until the shell has prepared it: a job, handed once.
        assert_eq!(st.prep(room.me()).loading, Loading::Loading { permille: 0 });
        assert!(room.loading(first));
        let jobs = room.take_load_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!((jobs[0].song.as_str(), jobs[0].chart), ("a", 0));
        assert!(room.take_load_jobs().is_empty());
        room.finish_load(&jobs[0], Ok(()));
        let st = &room.stages()[0];
        assert_eq!(st.prep(room.me()).loading, Loading::Loaded);
        assert!(!st.prep(room.me()).ready, "never ready by itself");
        assert_eq!(
            room.picked(&content_id(&s, &s.charts[0])).unwrap().song.id,
            "a"
        );
        assert!(room.refusal().is_none());
    }

    /// Every song asked for prepared, as a shell does.
    pub(crate) fn prepare(room: &mut RoomClient) {
        for job in room.take_load_jobs() {
            room.finish_load(&job, Ok(()));
        }
    }

    #[test]
    fn start_needs_a_song_and_ready_then_counts_down_into_the_stage() {
        let mut room = RoomClient::new(1_000, 7);
        let first = room.stages()[0].id;
        assert_eq!(room.start(), Err(Refusal::Room(Rejection::NoContent)));
        let s = song("a");
        room.choose(first, &s, &s.charts[0]).unwrap();
        room.set_ready(true).unwrap();
        // Ready while loading is allowed; Start waits for the song.
        assert_eq!(room.start(), Err(Refusal::Preparing));
        room.set_ready(false).unwrap();
        prepare(&mut room);
        assert!(matches!(
            room.start(),
            Err(Refusal::Room(Rejection::NotEnoughReady { .. }))
        ));
        assert_eq!(room.state().phase, Phase::Lobby, "no match locked");
        room.set_ready(true).unwrap();
        assert!(room.ready());
        room.start().unwrap();
        assert!(matches!(room.state().phase, Phase::Countdown { .. }));
        let m = room.state().current.as_ref().unwrap();
        assert_eq!(m.roster.len(), 4, "the player and three CPUs");
        room.tick(3_999);
        assert!(matches!(room.state().phase, Phase::Countdown { .. }));
        room.tick(4_000);
        assert_eq!(room.state().phase, Phase::Stage { stage: first });
        // Handed to the shell once: the song, the CPUs committed.
        let order = room.take_stage_order().unwrap();
        assert_eq!((order.stage, order.number, order.of), (first, 1, 4));
        assert_eq!(order.song.id, "a");
        assert_eq!(order.cpus.len(), 3);
        assert_eq!(order.battle, BattlePreset::GaugeSurvivalV0);
        assert!(room.take_stage_order().is_none());
        room.stop_match().unwrap();
        assert!(matches!(room.state().phase, Phase::Interrupted { .. }));
        room.open_next_match().unwrap();
        assert_eq!(room.state().phase, Phase::Lobby);
        assert_eq!(room.state().history.len(), 1);
        assert!(!room.ready(), "a new match has fresh readiness");
    }

    #[test]
    fn rules_follow_the_cap_and_clear_readiness_in_the_lobby() {
        let mut room = RoomClient::new(0, 1);
        assert_eq!((room.min_cpus(), room.max_cpus()), (1, 8));
        let s = song("a");
        let first = room.stages()[0].id;
        room.choose(first, &s, &s.charts[0]).unwrap();
        prepare(&mut room);
        room.set_ready(true).unwrap();
        let mut rules = room.with_cpus(room.rules().clone(), 8);
        rules.cpu.opponent.level = 4;
        rules.stages = 2;
        room.save_rules(rules.clone()).unwrap();
        assert_eq!(room.cpus_of(room.rules()), 8);
        assert_eq!(room.stages().len(), 2);
        assert!(!room.ready(), "a rules change clears readiness");
        // The song is still there; its preparation, cleared with the
        // readiness, is asked for again.
        assert!(room.stages()[0].content.is_some());
        assert!(room.loading(first));
        prepare(&mut room);
        assert_eq!(room.stages()[0].prep(room.me()).loading, Loading::Loaded);
        let over = room.with_cpus(rules, 9);
        assert!(room.check_rules(&over).is_err());
        assert!(matches!(
            room.save_rules(over),
            Err(Refusal::Room(Rejection::InvalidRules(
                RulesProblem::OverCap(_)
            )))
        ));
    }

    #[test]
    fn a_failed_preparation_is_reported_and_a_replaced_song_asks_again() {
        let mut room = RoomClient::new(0, 1);
        let first = room.stages()[0].id;
        let s = song("a");
        room.choose(first, &s, &s.charts[0]).unwrap();
        let old = room.take_load_jobs();
        room.finish_load(&old[0], Err(LoadProblem::CpuCannotPlay));
        assert_eq!(room.load_problem(first), Some(LoadProblem::CpuCannotPlay));
        room.set_ready(true).unwrap();
        assert_eq!(room.start(), Err(Refusal::Load(LoadProblem::CpuCannotPlay)));
        // Another song: a new job; the old answer counts for nothing.
        let t = song("b");
        room.choose(first, &t, &t.charts[0]).unwrap();
        assert_eq!(room.load_problem(first), None);
        let jobs = room.take_load_jobs();
        assert_eq!(jobs[0].song, "b");
        room.finish_load(&old[0], Ok(()));
        assert!(room.loading(first));
        room.finish_load(&jobs[0], Ok(()));
        assert!(!room.loading(first));
    }

    /// A whole practice match with outcomes reported as a stage runner
    /// does: intermission with the points, the next stage, the match's
    /// results, then the lobby with fresh readiness.
    #[test]
    fn reported_stages_run_a_match_to_its_results_and_back_to_the_lobby() {
        let mut room = RoomClient::new(0, 1);
        let rules = MatchRules {
            stages: 2,
            ..room.rules().clone()
        };
        room.save_rules(rules).unwrap();
        let s = song("a");
        for st in room.stages().iter().map(|s| s.id).collect::<Vec<_>>() {
            room.choose(st, &s, &s.charts[0]).unwrap();
        }
        prepare(&mut room);
        let mut now = 0;
        let mut play = |room: &mut RoomClient, early: bool, me_first: bool| {
            room.set_ready(true).unwrap();
            room.start().unwrap();
            now += 3_000;
            room.tick(now);
            let order = room.take_stage_order().unwrap();
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
        let mark = room.last_mark().unwrap();
        assert_eq!((mark.number, mark.placement.place, mark.field), (1, 1, 4));
        assert!(mark.experimental);
        assert!(!room.ready(), "the next stage asks for Ready again");
        assert_eq!(room.match_end(), None, "the match goes on");
        let second = play(&mut room, false, false);
        assert_ne!(first.stage, second.stage);
        assert_eq!(room.state().phase, Phase::MatchResults);
        assert_eq!(room.mark_of(second.stage).unwrap().placement.place, 4);
        let standings = room.state().standings().unwrap();
        let mine = standings
            .iter()
            .find(|s| s.participant == room.me())
            .unwrap();
        assert_eq!(mine.total, ddi_room::Points::whole(5));
        // CPU 1: second, then first (3 + 4 points).
        assert_eq!(room.match_end(), Some(MatchEnd::NotWon));
        room.open_next_match().unwrap();
        assert_eq!(room.match_end(), None);
        assert_eq!(room.state().phase, Phase::Lobby);
        assert!(!room.ready());
        assert!(room.last_mark().is_none());
    }
}
