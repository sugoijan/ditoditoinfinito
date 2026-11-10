//! The multiplayer prototype's screens (`docs/plans/multiplayer-ui.md`,
//! "Unified room layout and roles"; `docs/plans/multiplayer-prototype-progress.md`,
//! M2): the multiplayer home (a page) and the room, one screen with the
//! players and their standing at the left, the selected stage's song and
//! chart in the middle, the match's stages at the right, Ready and Start
//! (separate actions) at the bottom right; Rules and More are pop-ups
//! (`room_dialogs.rs`).
//!
//! The screen binds to a [`SharedRoom`] (`crate::state::room`): it reads
//! the room from the client's mirror and asks for changes as commands; it
//! keeps no room logic. Choosing a stage's song is the shell's: it opens
//! song select in its pick mode ([`RoomCall::ChooseSong`]), whose chart
//! goes back through [`RoomClient::choose`](crate::state::room::RoomClient::choose).
//!
//! - **Keys and controllers**: the directions move between the stages,
//!   the song's button and the actions (the stage focused is the one
//!   shown); Start on a stage or Choose song opens song select for it;
//!   More (a controller's West) opens More; Back leaves for the
//!   multiplayer home, the room kept.
//! - **Pointers**: a tap on a stage, a button.

use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_l10n::UiText;
use ddi_l10n::msg::{common, room as m, select as sm};
use ddi_room::{
    Capability, Connection, Finish, LinkState, MemberKind, Participation, Phase, Rejection, Role,
    RulesProblem, Stage, StageId, StageStanding,
};
use serde::Serialize;

use super::{Backdrop, Env, ground, header, left, right};
use crate::look::{self, Spec, call};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::online::Redirect;
use crate::state::online::link::LinkNotice;
use crate::state::room::{
    LoadJob, LoadProblem, Refusal, RoomClient, SharedRoom, Sharing, SharingProblem, StageMark,
    StageOrder, Unsupported, UploadJob, cpu_levels,
};
use crate::widgets::{play as play_command, set_image, set_text};

/// What the room asks of its shell.
#[derive(Clone, Debug, PartialEq)]
pub enum RoomCall {
    /// Back: to the multiplayer home, the room kept for the session.
    Leave,
    /// Leave room: the practice forgotten, then back to the home.
    Discard,
    /// Open song select to choose stage `stage`'s song (the `number`th).
    ChooseSong { stage: StageId, number: u16 },
    /// Prepare a stage's song on this device (read its chart, check the
    /// CPUs' plan) and answer `RoomClient::finish_load`.
    Prepare(LoadJob),
    /// The stage's countdown is over: play it (its prompt, the play with
    /// the CPUs, its results), report its outcome
    /// (`RoomClient::report_stage`, an online room's `report_run`) and
    /// come back to the room.
    PlayStage(Box<StageOrder>),
    /// An online room: upload the stage's song as the room's share with
    /// its grant, telling `RoomClient::upload_progress` and
    /// `upload_finished`.
    Upload(Box<UploadJob>),
    /// An online room's Sign in again: go where signing in needs (the
    /// server's page on the web, the system browser on desktop).
    Redirect(Redirect),
}

pub type RoomShell = Rc<dyn Fn(RoomCall, &mut Cx)>;

/// The room as the debug state shows it (`ThemeState::room`), for tests.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RoomView {
    /// The phase (`lobby`, `preparing`, `countdown`, `stage`, …).
    pub phase: String,
    /// The revision of the room the screen shows.
    pub revision: u64,
    /// The selected stage's index in the list.
    pub selected: usize,
    pub stages: Vec<RoomStageView>,
    pub members: Vec<RoomMemberView>,
    /// The rules the shown stages play under: CPU count and level (from
    /// 1), stages.
    pub cpus: u16,
    pub level: u8,
    pub stage_count: u16,
    /// The running match's rules are locked (a match exists).
    pub locked: bool,
    pub ready: bool,
    /// Whole seconds left of the countdown.
    pub countdown: Option<u64>,
    /// The status line, and the refusal it says (its kind), if any.
    pub status: String,
    pub refusal: Option<String>,
    /// The Start button's label.
    pub start: String,
    /// Finished matches kept in the room.
    pub history: usize,
    /// The room's own controls (`Stage.<i>`, `Choose`, `Ready`, `Start`,
    /// `Rules`, `More`, `Back`): where a shell's debug view finds them,
    /// whatever dialog said its own since.
    pub items: Vec<String>,
    /// The Rules pop-up is open; its draft (stages, CPUs, level from 1)
    /// and what the room says of it.
    pub rules_open: bool,
    pub draft: Vec<u16>,
    pub rules_problem: Option<String>,
    /// The local player's last stage result, as said under the song.
    pub mark: Option<String>,
    /// The match's standings (in a match or its results), best first.
    pub standings: Vec<RoomStandingView>,
    /// An online room (M5c): its id and name, the link (`joining`,
    /// `connected`, `reconnecting`, `sign_in_again`, `closed`, `left`),
    /// the local player's role (`owner`, `co_host`, `member`) and admin
    /// flag, and the invite link once read (operators).
    pub online: Option<RoomOnlineView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RoomOnlineView {
    pub room_id: String,
    pub name: Option<String>,
    pub link: String,
    pub role: String,
    pub admin: bool,
    pub invite: Option<String>,
    /// Whether More offers Copy invite link and Replace invite link.
    pub can_invite: bool,
    pub can_rotate: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RoomStandingView {
    pub id: u64,
    pub rank: u16,
    pub total: String,
    /// Each stage's points, `None` where not placed.
    pub stages: Vec<Option<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RoomStageView {
    pub id: u64,
    /// The library's song id and chart index of its content.
    pub song: Option<String>,
    pub chart: Option<usize>,
    pub started: bool,
    /// The content's status (`pending`, `available`, `unavailable`) and an
    /// online room's share of it (M5d).
    pub content: Option<String>,
    pub share: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RoomMemberView {
    pub id: u64,
    pub name: String,
    pub cpu: bool,
    /// A CPU's level, from 1.
    pub level: Option<u8>,
    pub ready: bool,
    pub points: String,
    /// A CPU that takes its seat when the match starts.
    pub at_start: bool,
    /// Online: the role (`owner`, `co_host`, `member`), admin flag,
    /// connection and participation as the room keeps them.
    pub role: String,
    pub admin: bool,
    pub connection: String,
    pub participation: String,
}

/// One player's row as the column shows it.
struct Row {
    name: String,
    sub: String,
    state: String,
    me: bool,
}

/// Where the parts go.
#[derive(Clone, Copy, Default)]
struct Layout {
    size: [f32; 2],
    zoom: f32,
    compact: bool,
    margin: f32,
    top: f32,
    /// The players' column, the song's and the stages' (`[x, w]`).
    players: [f32; 2],
    song: [f32; 2],
    stages: [f32; 2],
    /// The stages' rows' height and gap, and the bottom of their room.
    row: f32,
    gap: f32,
    bottom: f32,
    /// The action row's middle.
    actions_y: f32,
}

impl Layout {
    fn new(env: &Env, cx: &Cx) -> Layout {
        let vp = cx.viewport().screen;
        let (size, zoom, compact) = super::compact_fit(env, vp);
        let [w, h] = size;
        let footer = env.metric("Footer.Height", 40.0);
        if compact {
            let margin = 16.0;
            let col = ((w - 3.0 * margin) / 2.0).max(120.0);
            Layout {
                size,
                zoom,
                compact,
                margin,
                top: 76.0,
                players: [margin, w - 2.0 * margin],
                song: [margin, col],
                stages: [2.0 * margin + col, col],
                row: 40.0,
                gap: 4.0,
                bottom: h - footer - 52.0,
                actions_y: h - footer - 26.0,
            }
        } else {
            let margin = env.metric("Margin", 32.0);
            let players = 240.0;
            let song = 480.0_f32.min(w * 0.4);
            let stages_x = margin + players + 24.0 + song + 24.0;
            Layout {
                size,
                zoom,
                compact,
                margin,
                top: env.metric("Header.Height", 64.0) + 24.0,
                players: [margin, players],
                song: [margin + players + 24.0, song],
                stages: [stages_x, (w - margin - stages_x).max(160.0)],
                row: 64.0,
                gap: 8.0,
                bottom: h - footer - 72.0,
                actions_y: h - footer - 34.0,
            }
        }
    }
}

pub struct RoomScreen {
    env: Env,
    room: SharedRoom,
    shell: RoomShell,
    /// The stage shown (an index into the list), followed by focus.
    selected: usize,
    /// The room's version and the countdown's second last drawn.
    seen: Option<(u64, Option<u64>)>,
    layout: Layout,
    backdrop: Backdrop,
    live: super::Live,
    /// The focus to take at the first build (a stage coming back from
    /// song select).
    initial: Option<String>,
    /// A dialog or another screen was over it: it says what it shows
    /// again once uncovered.
    covered: bool,
    /// The room's phase when last looked at.
    phase: Option<ddi_room::PhaseKind>,
    /// The UI's clock at the last update ([`Screen::wake_at`]).
    time: f64,
}

impl RoomScreen {
    pub fn new(env: Env, room: SharedRoom, shell: RoomShell) -> RoomScreen {
        let selected = {
            let r = room.borrow();
            let upcoming = r.state().upcoming_stage();
            r.stages()
                .iter()
                .position(|s| Some(s.id) == upcoming)
                .unwrap_or(0)
        };
        RoomScreen {
            env,
            room,
            shell,
            selected,
            seen: None,
            layout: Layout::default(),
            backdrop: Backdrop::new(),
            live: Default::default(),
            initial: None,
            covered: false,
            phase: None,
            time: 0.0,
        }
    }

    /// Opens with stage `stage` selected and focused (back from choosing
    /// its song).
    pub fn with_stage(mut self, stage: StageId) -> RoomScreen {
        if let Some(i) = self
            .room
            .borrow()
            .stages()
            .iter()
            .position(|s| s.id == stage)
        {
            self.selected = i;
            self.initial = Some(stage_name(i));
        }
        self
    }

    fn countdown(&self) -> Option<u64> {
        let r = self.room.borrow();
        match r.state().phase {
            Phase::Countdown { start_at, .. } => {
                Some(start_at.0.saturating_sub(r.now_ms()).div_ceil(1000))
            }
            Phase::Preparing {
                window: Some(w), ..
            } => Some(w.deadline.0.saturating_sub(r.now_ms()).div_ceil(1000)),
            _ => None,
        }
    }

    fn selected_stage(&self) -> Option<Stage> {
        self.room.borrow().stages().get(self.selected).cloned()
    }

    /// Whether the selected stage's song can be chosen now: unstarted,
    /// by someone who may choose (the room's own rule).
    fn choosable(&self) -> bool {
        self.selected_stage().is_some_and(|s| s.play.is_none())
            && self.room.borrow().may(Capability::ChooseContent)
    }

    fn ready_shown(&self) -> bool {
        let r = self.room.borrow();
        let s = r.state();
        let gathering = matches!(
            s.phase,
            Phase::Lobby | Phase::Preparing { window: None, .. } | Phase::Intermission { .. }
        );
        if !gathering {
            return false;
        }
        if !r.is_online() {
            return true;
        }
        // Online, only a player gathering readiness has Ready: one who
        // watches or waits for the next match has none (never a grayed
        // Ready), nor does anyone before the room is reached.
        r.link_state() == LinkState::Connected
            && matches!(
                s.participation(r.me()),
                Some(
                    Participation::NotReady { .. }
                        | Participation::ReadyLoading { .. }
                        | Participation::ReadyLoaded { .. }
                        | Participation::LoadFailed { .. }
                )
            )
    }

    /// Whether Start (or what it becomes: Cancel start, Stop match, Next
    /// match) is offered: to whom the room lets do it.
    fn start_shown(&self) -> bool {
        let r = self.room.borrow();
        if r.is_online() && r.link_state() != LinkState::Connected {
            return false;
        }
        match r.state().phase {
            Phase::Stage { .. } | Phase::SongCompletion { .. } => r.may(Capability::StopMatch),
            _ => r.may(Capability::Advance),
        }
    }

    /// The header's name: the practice's, or the online room's own.
    fn heading(&self) -> UiText {
        let r = self.room.borrow();
        match (r.is_online(), r.name()) {
            (false, _) => m::practice_heading(),
            (true, Some(name)) => UiText::data(name.to_uppercase()),
            (true, None) => m::home_heading(),
        }
    }

    fn start_label(&self) -> UiText {
        let r = self.room.borrow();
        let s = r.state();
        let number = |id: StageId| {
            r.stages()
                .iter()
                .position(|st| st.id == id)
                .map_or(1, |i| i + 1)
        };
        match &s.phase {
            Phase::Lobby => m::start_match(),
            Phase::Preparing {
                window: None,
                stage,
            } => m::start_stage(number(*stage)),
            Phase::Intermission { next } => m::start_stage(number(*next)),
            Phase::Preparing { .. } | Phase::Countdown { .. } => m::cancel_start(),
            Phase::Stage { .. } | Phase::SongCompletion { .. } => m::stop_match(),
            Phase::MatchResults | Phase::Interrupted { .. } => m::new_match(),
        }
    }

    fn status(&self) -> UiText {
        let r = self.room.borrow();
        if r.is_online() {
            match r.link_state() {
                LinkState::Joining => return m::link_joining(),
                LinkState::Reconnecting { .. } => return m::link_reconnecting(),
                LinkState::SignInAgain => return m::link_sign_in_again(),
                LinkState::Closed => return m::link_closed(),
                LinkState::Left => return m::link_left(),
                LinkState::Connected => {}
            }
        }
        if let Some(why) = r.refusal() {
            return refusal_text(why);
        }
        if let Some(n) = r.link_notice() {
            return match n {
                LinkNotice::InviteCopied => m::invite_copied(),
                LinkNotice::InviteReplaced => m::invite_replaced(),
                LinkNotice::InviteNotReplaced => m::invite_not_replaced(),
            };
        }
        let s = r.state();
        let number = |id: StageId| {
            r.stages()
                .iter()
                .position(|st| st.id == id)
                .map_or(1, |i| i + 1)
        };
        let seconds = self.countdown().unwrap_or(0);
        match &s.phase {
            Phase::Lobby | Phase::Preparing { window: None, .. } | Phase::Intermission { .. } => {
                let Some(up) = s.upcoming_stage() else {
                    return m::status_over();
                };
                let has_song = s.stage(up).is_some_and(|st| st.content.is_some());
                let pending = s
                    .stage(up)
                    .and_then(|st| st.content.as_ref())
                    .is_some_and(|c| c.status == ddi_room::ContentStatus::Pending);
                if !has_song && !r.may(Capability::ChooseContent) {
                    m::status_host_chooses(number(up))
                } else if !has_song {
                    m::status_choose(number(up))
                } else if let Some(text) = sharing_status(&r, up, number(up)) {
                    text
                } else if r.is_online() && pending {
                    m::status_sharing(number(up))
                } else if let Some(p) = r.load_problem(up) {
                    load_problem_text(p)
                } else if r.loading(up) {
                    m::status_preparing(number(up))
                } else if r.ready() {
                    m::status_ready()
                } else if s.phase == Phase::Lobby {
                    m::status_not_ready()
                } else {
                    m::status_next(number(up))
                }
            }
            Phase::Preparing { .. } => m::status_window(seconds),
            Phase::Countdown { stage, .. } => m::status_countdown(number(*stage), seconds),
            Phase::Stage { stage } | Phase::SongCompletion { stage } if r.is_online() => {
                m::status_online_stage(number(*stage))
            }
            Phase::Stage { stage } | Phase::SongCompletion { stage } => {
                m::status_playing(number(*stage))
            }
            Phase::MatchResults => m::status_over(),
            Phase::Interrupted { .. } => m::status_stopped(),
        }
    }

    fn rows(&self) -> Vec<Row> {
        let r = self.room.borrow();
        let s = r.state();
        let levels = cpu_levels();
        let standings = s.standings().unwrap_or_default();
        // Places once a stage gave points; before, the points alone.
        let placed = r.last_mark().is_some();
        let points = |id| {
            standings
                .iter()
                .find(|st| st.participant == id)
                .map(|st| {
                    if placed {
                        m::rank_points(st.rank, st.total.to_string()).into()
                    } else {
                        m::points(st.total.to_string()).into()
                    }
                })
                .unwrap_or_default()
        };
        let in_match = s.current.is_some() && s.phase != Phase::Lobby;
        let mut rows = Vec::new();
        for member in s.members.values().filter(|mb| !mb.left) {
            match &member.kind {
                MemberKind::Human { .. } => {
                    let me = member.id == r.me();
                    let name: String = if me {
                        m::you().into()
                    } else {
                        UiText::data(member_name(member)).into()
                    };
                    let state = match s.participation(member.id) {
                        Some(
                            Participation::ReadyLoaded { .. }
                            | Participation::ReadyLoading { .. }
                            | Participation::Committed { .. },
                        ) => m::state_ready().into(),
                        Some(Participation::NotReady { .. } | Participation::LoadFailed { .. }) => {
                            m::state_not_ready().into()
                        }
                        Some(Participation::Competing { .. }) => m::state_playing().into(),
                        _ => String::new(),
                    };
                    let (sub, state) = if r.is_online() {
                        online_row(s, member, in_match.then(|| points(member.id)), state)
                    } else {
                        (
                            if in_match {
                                points(member.id)
                            } else {
                                String::new()
                            },
                            state,
                        )
                    };
                    rows.push(Row {
                        name,
                        sub,
                        state,
                        me,
                    });
                }
                MemberKind::Cpu { ordinal, opponent } => rows.push(Row {
                    name: m::cpu_name(*ordinal).into(),
                    sub: m::cpu_level(u16::from(opponent.level) + 1, levels).into(),
                    state: points(member.id),
                    me: false,
                }),
            }
        }
        if !in_match {
            // Before the match: the CPUs its rules will seat.
            let rules = &s.next.rules;
            for n in 1..=r.cpus_of(rules) {
                rows.push(Row {
                    name: m::cpu_name(n).into(),
                    sub: m::cpu_level(u16::from(rules.cpu.opponent.level) + 1, levels).into(),
                    state: m::cpu_at_start().into(),
                    me: false,
                });
            }
        }
        rows
    }

    /// The window of stage rows shown: its first and how many.
    fn window(&self, count: usize) -> (usize, usize) {
        let l = &self.layout;
        let room = (l.bottom - l.top).max(l.row);
        let fit = ((room + l.gap) / (l.row + l.gap)).floor().max(1.0) as usize;
        if count <= fit {
            return (0, count);
        }
        let first = self.selected.saturating_sub(fit / 2).min(count - fit);
        (first, fit)
    }

    fn tree(&mut self, cx: &mut Cx) -> Actor {
        self.layout = Layout::new(&self.env, cx);
        let l = self.layout;
        let env = self.env.clone();
        let [w, h] = l.size;
        let stages: Vec<Stage> = self.room.borrow().stages().to_vec();
        self.selected = self.selected.min(stages.len().saturating_sub(1));
        let mut children = vec![ground(&env, l.size), self.backdrop.build(&env, l.size)];

        // The header: Back, the room's name; Rules and More at the right.
        let mut top = Vec::new();
        if l.compact {
            top.push(super::back_button(
                &env,
                [44.0, 44.0],
                [l.margin + 22.0, 28.0],
            ));
            let mut title = env.text("Title", &self.heading(), 16.0, 700, "Text");
            look::ellipsized(&mut title, (w - 2.0 * l.margin - 260.0).max(60.0));
            top.push(left(title, l.margin + 56.0, 28.0));
        } else {
            top.push(header(&env, cx, w, &self.heading()));
        }
        let more_x = w - l.margin - 32.0;
        top.push(super::more_button(&env, [more_x, 32.0]));
        let rules = m::rules();
        let rw = (look::mono_width(&rules, 15.0) + 24.0).max(72.0);
        top.push(env.component(
            "Button",
            "Button",
            &Spec::new([rw, 40.0], rules.as_str()),
            "Rules",
            [more_x - 32.0 - 12.0 - rw / 2.0, 32.0],
        ));
        children.push(super::frame("Top", top));

        // The players.
        let rows = self.rows();
        let mut players = Vec::new();
        let [px, pw] = l.players;
        let cap = self.room.borrow().state().policy.max_competitors;
        if l.compact {
            let (cpus, level) = {
                let r = self.room.borrow();
                let rules = r.rules();
                (r.cpus_of(rules), u16::from(rules.cpu.opponent.level) + 1)
            };
            let words = if self.room.borrow().is_online() {
                m::online_note()
            } else {
                m::players_line(cpus, level, cpu_levels())
            };
            let mut line = env.text("Players.Line", &words, 13.0, 400, "Secondary");
            look::ellipsized(&mut line, pw - 80.0);
            players.push(left(line, px, l.top - 14.0));
            players.push(right(
                env.text(
                    "Players.Count",
                    &m::players_count(rows.len(), cap),
                    13.0,
                    400,
                    "Muted",
                ),
                px + pw,
                l.top - 14.0,
            ));
        } else {
            players.push(left(
                env.text("Players.Heading", &m::players(), 13.0, 700, "Muted"),
                px,
                l.top + 8.0,
            ));
            players.push(right(
                env.text(
                    "Players.Count",
                    &m::players_count(rows.len(), cap),
                    13.0,
                    400,
                    "Muted",
                ),
                px + pw,
                l.top + 8.0,
            ));
            let mut y = l.top + 44.0;
            for (i, row) in rows.iter().enumerate() {
                let color = if row.me { "Text" } else { "Secondary" };
                let mut name = env.text(&format!("Player{i}.Name"), &row.name, 16.0, 600, color);
                look::ellipsized(&mut name, pw - 90.0);
                players.push(left(name, px, y));
                if !row.sub.is_empty() {
                    let mut sub = env.text(&format!("Player{i}.Sub"), &row.sub, 12.0, 400, "Muted");
                    look::ellipsized(&mut sub, pw - 90.0);
                    players.push(left(sub, px, y + 18.0));
                }
                players.push(right(
                    env.text(&format!("Player{i}.State"), &row.state, 13.0, 600, "Muted"),
                    px + pw,
                    y,
                ));
                y += 48.0;
                if y > l.bottom {
                    break;
                }
            }
            let note_text = if self.room.borrow().is_online() {
                m::online_note()
            } else {
                m::practice_note()
            };
            let mut note = env.text("Players.Note", &note_text, 12.0, 400, "Muted");
            look::ellipsized(&mut note, pw);
            players.push(left(note, px, y + 4.0));
        }
        children.push(super::frame("Players", players));

        // The selected stage's song and chart; at the match's end, its
        // results in their place.
        let over = matches!(
            self.room.borrow().state().phase,
            Phase::MatchResults | Phase::Interrupted { .. }
        );
        if over {
            children.push(self.results_part());
        } else {
            children.push(self.song_part(cx, stages.get(self.selected)));
        }

        // The stages.
        let [sx, sw] = l.stages;
        let (first, n) = self.window(stages.len());
        let mut list = Vec::new();
        for (k, stage) in stages.iter().enumerate().skip(first).take(n) {
            let y = l.top + (k - first) as f32 * (l.row + l.gap);
            let mut row = env.component(
                "List.Row",
                "Button",
                &Spec::new([sw, l.row], ""),
                &stage_name(k),
                [sx, y],
            );
            let picked = stage
                .content
                .as_ref()
                .and_then(|c| self.room.borrow().picked(&c.id).cloned());
            let title: String = match (&picked, &stage.content) {
                (Some(p), _) => UiText::data(&p.song.title).into(),
                // Online: another person's song, not known here.
                (None, Some(_)) => m::host_song().into(),
                (None, None) => m::stage_empty().into(),
            };
            let sub: String = match &picked {
                Some(p) => m::stage_chart(
                    k + 1,
                    UiText::data(format!("{} {}", p.chart.label(), p.chart.meter_text())),
                )
                .into(),
                None => m::stage(k + 1).into(),
            };
            set_text(&mut row, "Title", &title);
            set_text(&mut row, "Subtitle", &sub);
            let banner = picked
                .as_ref()
                .and_then(|p| p.song.banner.as_deref())
                .map(|b| cx.texture(b, 128.0));
            set_image(&mut row, "Banner", banner);
            play_command(
                &mut row,
                if k == self.selected {
                    "Select"
                } else {
                    "Deselect"
                },
            );
            row.finish_tweening();
            list.push(row);
        }
        children.push(super::frame("Stages", list));

        // The action row: the status at the left, Ready and Start at the
        // right (Choose song too, on a phone).
        let mut actions = Vec::new();
        let bw = if l.compact { 132.0 } else { 172.0 };
        let bh = if l.compact { 40.0 } else { 44.0 };
        let mut x = w - l.margin - bw / 2.0;
        let start = self.start_shown();
        if start {
            actions.push(env.component(
                "Button",
                "Button",
                &Spec::new([bw, bh], self.start_label().as_str()),
                "Start",
                [x, l.actions_y],
            ));
        }
        if self.ready_shown() {
            if start {
                x -= bw + 12.0;
            }
            let label = if self.room.borrow().ready() {
                m::unready()
            } else {
                m::ready()
            };
            actions.push(env.component(
                "Button",
                "Button",
                &Spec::new([bw, bh], label.as_str()),
                "Ready",
                [x, l.actions_y],
            ));
        }
        if l.compact && self.choosable() {
            x -= bw + 12.0;
            actions.push(self.choose_button([bw, bh], [x, l.actions_y]));
        }
        if !start && !self.ready_shown() && !(l.compact && self.choosable()) {
            // No buttons: the status has the row.
            x = w - l.margin + bw / 2.0 + 16.0;
        }
        // The status left of the buttons; on a phone with no room there,
        // above them, the screen's width.
        let beside = x - bw / 2.0 - 16.0 - l.margin;
        let (room, middle) = if l.compact && beside < 160.0 {
            (w - 2.0 * l.margin, l.actions_y - bh / 2.0 - 26.0)
        } else {
            (beside.max(80.0), l.actions_y)
        };
        let lines = super::dialog::wrap_lines(&self.status(), room, 13.0);
        for (i, line) in lines.iter().take(2).enumerate() {
            let mut t = env.text(&format!("Status{i}"), line, 13.0, 400, "Secondary");
            look::ellipsized(&mut t, room);
            let dy = (i as f32 - (lines.len().min(2) as f32 - 1.0) / 2.0) * 17.0;
            actions.push(left(t, l.margin, middle + dy));
        }
        children.push(super::frame("Actions", actions));

        // The footer's hints.
        children.push(self.footer(cx));

        let mut root = Actor::new(Kind::Frame(children));
        call(&mut root, "zoom", &[l.zoom]);
        super::transitions(
            &mut root,
            &env,
            &[
                ("Top", super::Edge::Top),
                ("Players", super::Edge::Left),
                ("Song", super::Edge::Left),
                ("Stages", super::Edge::Right),
                ("Actions", super::Edge::Bottom),
                ("Footer", super::Edge::Bottom),
                ("Backdrop", super::Edge::None),
            ],
        );
        let _ = h;
        root
    }

    /// An online room's own: where signing in again sends the shell, the
    /// invite asked for once the player operates the room.
    fn online_update(&mut self, cx: &mut Cx) {
        let (redirect, fetch) = {
            let r = self.room.borrow();
            let Some(link) = r.online_link() else {
                return;
            };
            let fetch = r.link_state() == LinkState::Connected
                && r.may(Capability::ChooseContent)
                && r.invite_link().is_none();
            let mut o = link.online().borrow_mut();
            // Back in this room after signing in again: it is shown.
            let _ = o.take_entered();
            (o.take_redirect(), fetch)
        };
        if fetch {
            self.room.borrow_mut().fetch_invite();
        }
        if let Some(r) = redirect {
            (self.shell)(RoomCall::Redirect(r), cx);
        }
    }

    fn choose_button(&self, size: [f32; 2], at: [f32; 2]) -> Actor {
        let has_song = self.selected_stage().is_some_and(|s| s.content.is_some());
        let label = if has_song {
            m::change_song()
        } else {
            m::choose_song()
        };
        self.env.component(
            "Button",
            "Button",
            &Spec::new(size, label.as_str()),
            "Choose",
            at,
        )
    }

    /// The song part: the song's card (`SongInfo`, as song select's), its
    /// chart's row, and Choose song.
    fn song_part(&self, cx: &mut Cx, stage: Option<&Stage>) -> Actor {
        let l = self.layout;
        let env = &self.env;
        let [x, w] = l.song;
        let picked = stage
            .and_then(|s| s.content.as_ref())
            .and_then(|c| self.room.borrow().picked(&c.id).cloned());
        let info_h = if l.compact { 170.0 } else { 300.0 };
        let mut info = env.component(
            "SongInfo",
            "Panel",
            &Spec::new([w, info_h], ""),
            "Info",
            [0.0, 0.0],
        );
        let mut children = Vec::new();
        match &picked {
            Some(p) => {
                let song = &p.song;
                set_text(&mut info, "Title", &UiText::data(&song.title));
                set_text(&mut info, "Artist", &UiText::data(&song.artist));
                set_text(&mut info, "Bpm", &UiText::data(&song.bpm_text));
                set_text(&mut info, "Length", &UiText::data(song.length_text()));
                let pack = crate::state::select_view::pack_label(song.pack.as_deref());
                set_text(&mut info, "Pack", &pack);
                set_text(
                    &mut info,
                    "Line",
                    &sm::info_line(&song.artist, &song.bpm_text, song.length_text()),
                );
                let side = if l.compact { w } else { 560.0 };
                let banner = song.banner.as_deref().map(|b| cx.texture(b, side));
                set_image(&mut info, "Banner", banner);
            }
            None => {
                if stage.is_some_and(|st| st.content.is_some()) {
                    set_text(&mut info, "Title", &m::host_song());
                }
                set_image(&mut info, "Banner", None);
                // No song: the card's labels with nothing beside them say
                // nothing.
                for part in ["BpmLabel", "AfterBpm"] {
                    if let Some(a) = crate::widgets::part(&mut info, part) {
                        a.visible = false;
                    }
                }
            }
        }
        children.push(info);
        let mut y = info_h + 10.0;
        let chart_h = if l.compact { 36.0 } else { 52.0 };
        match (&picked, stage) {
            (Some(p), _) => {
                let c = &p.chart;
                let mut r = env.component(
                    "ChartLadder.Row",
                    "List.Row",
                    &Spec::new([w, chart_h], c.difficulty.name()),
                    "Chart",
                    [0.0, y],
                );
                set_text(&mut r, "Label", &UiText::data(c.label().to_uppercase()));
                set_text(&mut r, "Title", &UiText::data(c.label()));
                let notes: String = sm::chart_notes(c.notes).into();
                set_text(&mut r, "Notes", &notes);
                set_text(&mut r, "Subtitle", &notes);
                set_text(&mut r, "Meter", &c.meter_text());
                for p in ["Grade", "Score", "LampText"] {
                    set_text(&mut r, p, "");
                }
                play_command(&mut r, "Lamp.None");
                play_command(&mut r, c.difficulty.name());
                play_command(&mut r, "Select");
                r.finish_tweening();
                children.push(r);
            }
            (None, Some(st)) if st.content.is_none() => {
                let words = if self.room.borrow().may(Capability::ChooseContent) {
                    m::no_song()
                } else {
                    m::no_song_yet()
                };
                let t = env.text("NoSong", &words, 15.0, 400, "Muted");
                children.push(left(t, 0.0, y + chart_h / 2.0));
            }
            _ => {}
        }
        y += chart_h + 12.0;
        if !l.compact {
            if self.choosable() {
                children.push(self.choose_button([200.0, 40.0], [100.0, y + 20.0]));
            } else if stage.is_some_and(|st| st.play.is_some()) {
                let mut t = env.text("Locked", &m::stage_locked(), 13.0, 400, "Muted");
                look::ellipsized(&mut t, w);
                children.push(left(t, 0.0, y + 20.0));
            }
            y += 52.0;
        }
        // Between stages: how the last one went for the player.
        if let Some(mark) = self.room.borrow().last_mark() {
            let size = if l.compact { 11.0 } else { 13.0 };
            let lines = super::dialog::wrap_lines(&mark_text(&mark), w, size);
            for (i, line) in lines.iter().take(3).enumerate() {
                let mut t = env.text(&format!("Mark{i}"), line, size, 600, "Secondary");
                look::ellipsized(&mut t, w);
                children.push(left(t, 0.0, y + size / 2.0 + i as f32 * (size + 4.0)));
            }
        }
        let mut f = super::frame("Song", children);
        call(&mut f, "xy", &[x, l.top]);
        f
    }

    /// The match's results where the song was: its title, the winners,
    /// and each player's place, total and stages' points.
    fn results_part(&self) -> Actor {
        let l = self.layout;
        let env = &self.env;
        let [x, w] = l.song;
        let r = self.room.borrow();
        let s = r.state();
        let mut children = Vec::new();
        let completed = s.phase == Phase::MatchResults;
        let title = if completed {
            m::results_title()
        } else {
            m::results_stopped()
        };
        let mut t = env.text("Results.Title", &title, 18.0, 700, "Text");
        look::ellipsized(&mut t, w);
        children.push(left(t, 0.0, 12.0));
        let standings = s.standings().unwrap_or_default();
        let name = |id| match s.member(id) {
            Some(mb) if mb.id == r.me() => String::from(m::you()),
            Some(mb) => match &mb.kind {
                MemberKind::Cpu { ordinal, .. } => m::cpu_name(*ordinal).into(),
                MemberKind::Human { .. } => UiText::data(member_name(mb)).into(),
            },
            None => String::new(),
        };
        let mut y = 40.0;
        if completed {
            let winners: Vec<String> = standings
                .iter()
                .filter(|st| st.rank == 1)
                .map(|st| name(st.participant))
                .collect();
            let line = m::results_winners(winners.len(), UiText::data(winners.join(", ")));
            let mut t = env.text("Results.Winners", &line, 14.0, 600, "Gold");
            look::ellipsized(&mut t, w);
            children.push(left(t, 0.0, y));
            y += 30.0;
        }
        // The table: place, name, then each stage and the total at the
        // right (the stages only where they fit).
        let stages = standings.first().map_or(0, |st| st.stages.len());
        let cell = 40.0;
        let total_w = 64.0;
        let show_stages = !l.compact && w - 36.0 - 120.0 - total_w >= cell * stages as f32;
        let header = |key: &str, text: &str, at: f32, children: &mut Vec<Actor>| {
            children.push(right(env.text(key, text, 11.0, 700, "Muted"), at, y));
        };
        header("Results.Total", &m::results_total(), w, &mut children);
        if show_stages {
            for i in 0..stages {
                let at = w - total_w - cell * (stages - 1 - i) as f32;
                header(
                    &format!("Results.S{i}"),
                    &m::results_stage(i + 1),
                    at,
                    &mut children,
                );
            }
        }
        y += 22.0;
        let row = if l.compact { 24.0 } else { 30.0 };
        for (k, st) in standings.iter().enumerate() {
            if y > l.bottom - l.top {
                break;
            }
            let me = st.participant == r.me();
            let color = if me { "Text" } else { "Secondary" };
            children.push(left(
                env.numbers(
                    &format!("Results.Rank{k}"),
                    &st.rank.to_string(),
                    14.0,
                    "Muted",
                ),
                0.0,
                y,
            ));
            let mut n = env.text(
                &format!("Results.Name{k}"),
                &name(st.participant),
                14.0,
                600,
                color,
            );
            let room = w
                - 36.0
                - total_w
                - if show_stages {
                    cell * stages as f32
                } else {
                    0.0
                };
            look::ellipsized(&mut n, room.max(40.0));
            children.push(left(n, 36.0, y));
            children.push(right(
                env.numbers(
                    &format!("Results.Total{k}"),
                    &st.total.to_string(),
                    14.0,
                    color,
                ),
                w,
                y,
            ));
            if show_stages {
                for (i, cell_of) in st.stages.iter().enumerate() {
                    let text: String = match cell_of {
                        StageStanding::Placed(p) => p.points.to_string(),
                        _ => m::results_none().into(),
                    };
                    let at = w - total_w - cell * (stages - 1 - i) as f32;
                    children.push(right(
                        env.numbers(&format!("Results.Cell{k}.{i}"), &text, 13.0, "Muted"),
                        at,
                        y,
                    ));
                }
            }
            y += row;
        }
        drop(r);
        let mut f = super::frame("Song", children);
        call(&mut f, "xy", &[x, l.top]);
        f
    }

    fn footer(&self, cx: &mut Cx) -> Actor {
        use ddi_engine::MenuInput as A;
        let (mv, select, back, more) = (
            m::hint_move(),
            m::hint_select(),
            m::hint_back(),
            common::more(),
        );
        let hints: Vec<(&[&str], &str)> = vec![
            (&["↑", "↓", "←", "→"], mv.as_str()),
            (&["ENTER"], select.as_str()),
            (&["ESC"], back.as_str()),
        ];
        let controller: Vec<(&[A], &str)> = vec![
            (&[A::Up, A::Down, A::Left, A::Right], mv.as_str()),
            (&[A::Confirm, A::Start], select.as_str()),
            (&[A::More], more.as_str()),
            (&[A::Back], back.as_str()),
        ];
        let l = self.layout;
        super::footer(&self.env, l.size, &hints, &controller, "", &[], cx)
    }

    /// Says what the screen shows (`ThemeState::room`).
    fn publish(&self) {
        let r = self.room.borrow();
        let s = r.state();
        let rules = r.rules();
        let stages = r
            .stages()
            .iter()
            .map(|st| {
                let p = st.content.as_ref().and_then(|c| r.picked(&c.id));
                RoomStageView {
                    id: st.id.0,
                    song: p.map(|p| p.song.id.clone()),
                    chart: p.map(|p| p.chart.index),
                    started: st.play.is_some(),
                    content: st.content.as_ref().map(|c| {
                        match c.status {
                            ddi_room::ContentStatus::Pending => "pending",
                            ddi_room::ContentStatus::Available => "available",
                            ddi_room::ContentStatus::Unavailable { .. } => "unavailable",
                        }
                        .to_string()
                    }),
                    share: st.content.as_ref().and_then(|c| c.share.clone()),
                }
            })
            .collect();
        let standings = s.standings().unwrap_or_default();
        let mut members: Vec<RoomMemberView> = s
            .members
            .values()
            .filter(|mb| !mb.left)
            .map(|mb| RoomMemberView {
                id: mb.id.0,
                name: member_name(mb),
                role: snake(&mb.role),
                admin: mb.admin,
                connection: snake(&mb.connection),
                participation: s
                    .participation(mb.id)
                    .map(|p| snake(&p))
                    .unwrap_or_default(),
                cpu: mb.is_cpu(),
                level: s.opponent(mb.id).map(|o| o.level + 1),
                ready: s
                    .upcoming_stage()
                    .and_then(|id| s.stage(id))
                    .is_some_and(|st| st.prep(mb.id).ready),
                points: standings
                    .iter()
                    .find(|st| st.participant == mb.id)
                    .map(|st| st.total.to_string())
                    .unwrap_or_default(),
                at_start: false,
            })
            .collect();
        if s.phase == Phase::Lobby || s.current.is_none() {
            for n in 1..=r.cpus_of(&s.next.rules) {
                members.push(RoomMemberView {
                    name: format!("CPU {n}"),
                    cpu: true,
                    level: Some(s.next.rules.cpu.opponent.level + 1),
                    at_start: true,
                    ..RoomMemberView::default()
                });
            }
        }
        let view = RoomView {
            phase: format!("{:?}", s.phase.kind()).to_lowercase(),
            revision: s.revision.0,
            selected: self.selected,
            stages,
            members,
            cpus: r.cpus_of(rules),
            level: rules.cpu.opponent.level + 1,
            stage_count: rules.stages,
            locked: r.locked_rules().is_some(),
            ready: r.ready(),
            countdown: self.countdown(),
            status: String::new(),
            refusal: r.refusal().map(|f| format!("{f:?}")),
            start: String::new(),
            history: s.history.len(),
            online: r.online_link().map(|l| RoomOnlineView {
                room_id: l.room_id().into(),
                name: l.name().map(str::to_string),
                link: snake(&l.state()),
                role: r
                    .my_role()
                    .map(|(role, _)| snake(&role))
                    .unwrap_or_default(),
                admin: r.my_role().is_some_and(|(_, a)| a),
                invite: r.invite_link(),
                can_invite: r.invite_link().is_some(),
                can_rotate: r.invite_link().is_some() && r.may(Capability::ChangeAccess),
            }),
            ..RoomView::default()
        };
        drop(r);
        let r = self.room.borrow();
        let standings = r
            .state()
            .standings()
            .unwrap_or_default()
            .into_iter()
            .map(|st| RoomStandingView {
                id: st.participant.0,
                rank: st.rank,
                total: st.total.to_string(),
                stages: st
                    .stages
                    .iter()
                    .map(|s| match s {
                        StageStanding::Placed(p) => Some(p.points.to_string()),
                        _ => None,
                    })
                    .collect(),
            })
            .collect();
        let mark = r.last_mark().map(|m| mark_text(&m));
        drop(r);
        let view = RoomView {
            status: self.status().into(),
            start: self.start_label().into(),
            mark,
            standings,
            ..view
        };
        let items: Vec<String> = self.focusables().into_iter().map(|f| f.name).collect();
        let mut st = self.env.state.borrow_mut();
        st.screen = "room".into();
        st.compact = self.layout.compact;
        st.page_items = items.clone();
        let rules_open = st.room.as_ref().is_some_and(|r| r.rules_open);
        st.room = Some(RoomView {
            items,
            rules_open,
            ..view
        });
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx) {
        let left = root.tween_time_left();
        *root = self.tree(cx);
        super::relaid(root, left, cx);
        self.finish_build(root, cx);
    }

    /// What a fresh tree needs: the backdrop's images, the focus kept on
    /// something there, the debug state.
    fn finish_build(&mut self, root: &mut Actor, cx: &mut Cx) {
        let source = self.selected_stage().and_then(|st| {
            let c = st.content?;
            self.room.borrow().picked(&c.id)?.song.background.clone()
        });
        self.backdrop.restore(root, cx, &self.env);
        self.backdrop.set(source.as_deref(), root, cx, &self.env);
        let names: Vec<String> = self.focusables().into_iter().map(|f| f.name).collect();
        let focused = cx.focused().map(str::to_string);
        match focused {
            Some(f) if names.contains(&f) => super::restore_focus(root, Some(&f)),
            Some(_) => cx.focus(stage_name(self.selected)),
            None => {}
        }
        self.seen = Some((self.room.borrow().version(), self.countdown()));
        self.publish();
    }

    fn select(&mut self, i: usize, root: &mut Actor, cx: &mut Cx) {
        if i == self.selected {
            return;
        }
        self.selected = i;
        self.rebuild(root, cx);
    }

    fn choose(&mut self, cx: &mut Cx) {
        let Some(stage) = self.selected_stage() else {
            return;
        };
        if !self.choosable() {
            self.room.borrow_mut().clear_refusal();
            return;
        }
        let number = u16::try_from(self.selected + 1).unwrap_or(u16::MAX);
        (self.shell)(
            RoomCall::ChooseSong {
                stage: stage.id,
                number,
            },
            cx,
        );
    }

    fn start(&mut self, cx: &mut Cx) {
        let phase = self.room.borrow().state().phase.clone();
        let mut r = self.room.borrow_mut();
        let _ = match phase {
            Phase::Lobby | Phase::Preparing { window: None, .. } | Phase::Intermission { .. } => {
                r.start()
            }
            Phase::Preparing { .. } | Phase::Countdown { .. } => r.cancel_start(),
            Phase::Stage { .. } | Phase::SongCompletion { .. } => r.stop_match(),
            Phase::MatchResults | Phase::Interrupted { .. } => r.open_next_match(),
        };
        drop(r);
        // The stage started shows: the upcoming one, or the first of the
        // next match.
        let up = self.room.borrow().state().upcoming_stage();
        if let Some(i) =
            up.and_then(|id| self.room.borrow().stages().iter().position(|s| s.id == id))
        {
            self.selected = i;
        }
        let _ = cx;
    }

    fn open_more(&mut self, cx: &mut Cx) {
        let r = self.room.borrow();
        let running = r.state().phase.kind().match_running();
        let mut items = Vec::new();
        if running && r.may(Capability::StopMatch) {
            items.push(("stop".to_string(), m::stop_match().into()));
        }
        if r.is_online() {
            if r.link_state() == LinkState::SignInAgain {
                items.push(("signin".to_string(), m::sign_in_again().into()));
            }
            if r.invite_link().is_some() {
                items.push(("copy".to_string(), m::copy_invite().into()));
            }
            if r.invite_link().is_some() && r.may(Capability::ChangeAccess) {
                items.push(("rotate".to_string(), m::replace_invite().into()));
            }
            if r.state()
                .upcoming_stage()
                .is_some_and(|up| r.may_retry_sharing(up))
            {
                items.push(("retry".to_string(), m::retry_sharing().into()));
            }
        }
        drop(r);
        items.push(("leave".to_string(), m::leave().into()));
        super::action_menu(&self.env, &common::more(), items, "Room.More", cx);
    }

    fn open_rules(&mut self, cx: &mut Cx) {
        cx.overlay(
            super::room_dialogs::RulesDialog::new(self.env.clone(), self.room.clone()),
            true,
        );
    }
}

/// A stage row's actor name.
fn stage_name(i: usize) -> String {
    format!("Stage.{i}")
}

fn member_name(m: &ddi_room::Member) -> String {
    match &m.kind {
        MemberKind::Human { name, .. } => name.clone(),
        MemberKind::Cpu { ordinal, .. } => format!("CPU {ordinal}"),
    }
}

/// The local player's result of a stage, as a line (under the song in
/// the room, on the stage's results).
pub fn mark_text(mark: &StageMark) -> String {
    let p = &mark.placement;
    let points = p.points.to_string();
    let line: String = match p.finish {
        Finish::Survived => m::mark_survived(mark.number, p.place, mark.field, points),
        Finish::Eliminated => m::mark_out(mark.number, p.place, mark.field, points),
        Finish::Forfeit => m::mark_forfeit(mark.number, p.place, mark.field),
    }
    .into();
    if mark.experimental && p.finish != Finish::Forfeit {
        format!("{line} · {}", m::mark_experimental())
    } else {
        line
    }
}

/// A value's name as serde writes it (`co_host`, `reconnecting`), for the
/// debug state.
fn snake<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(serde_json::Value::Object(o)) => o.keys().next().cloned().unwrap_or_default(),
        _ => String::new(),
    }
}

/// An online room's person, as their row says it: under the name their
/// role (Host, Co-host, Admin; Watching, Next match), else their points
/// in a match; at the right their connection when not connected, else
/// their readiness (`ready`).
fn online_row(
    s: &ddi_room::RoomState,
    member: &ddi_room::Member,
    points: Option<String>,
    ready: String,
) -> (String, String) {
    let role: Option<String> = match (member.role, member.admin) {
        (Role::Owner, _) => Some(m::role_host().into()),
        (Role::CoHost, _) => Some(m::role_co_host().into()),
        (Role::Member, true) => Some(m::role_admin().into()),
        (Role::Member, false) => None,
    };
    let seat: Option<String> = match s.participation(member.id) {
        Some(Participation::WaitingNextMatch) => Some(m::next_match().into()),
        Some(Participation::Watching) => Some(m::watching().into()),
        _ => None,
    };
    let sub = [role, seat, points]
        .into_iter()
        .flatten()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let state = match member.connection {
        Connection::Joining => m::conn_joining().into(),
        Connection::Reconnecting => m::conn_reconnecting().into(),
        Connection::Disconnected => m::conn_away().into(),
        Connection::Connected => ready,
    };
    (sub, state)
}

/// Why a stage's song could not be prepared, worded.
pub fn load_problem_text(p: LoadProblem) -> UiText {
    match p {
        LoadProblem::Unreadable => m::load_unreadable(),
        LoadProblem::Changed => m::load_changed(),
        LoadProblem::CpuCannotPlay => m::load_cpu(),
        LoadProblem::Download => m::load_download(),
    }
}

/// What the status line says of stage `up`'s song being shared with an
/// online room (M5d), if anything: this device's upload as it goes or
/// why it failed, or the room's refusal of what was shared.
fn sharing_status(r: &RoomClient, up: StageId, number: usize) -> Option<UiText> {
    if !r.is_online() {
        return None;
    }
    let content = r.state().stage(up)?.content.as_ref()?;
    if let ddi_room::ContentStatus::Unavailable { code } = &content.status {
        return Some(if code == "digest_mismatch" {
            m::status_share_mismatch(number)
        } else {
            m::status_share_unavailable(number)
        });
    }
    if content.status != ddi_room::ContentStatus::Pending {
        return None;
    }
    Some(match r.sharing(up)? {
        Sharing::Uploading { done, total } if *total > 0.0 => {
            let percent = ((done / total) * 100.0).floor().clamp(0.0, 100.0) as u32;
            m::status_uploading(number, percent)
        }
        Sharing::Asking | Sharing::Uploading { .. } | Sharing::Sent => m::status_sharing(number),
        Sharing::Failed(SharingProblem::Upload(e)) => {
            m::status_share_failed(number, super::failures::share_text(e))
        }
        Sharing::Failed(SharingProblem::Grant(code)) => {
            m::status_share_failed(number, UiText::data(code))
        }
    })
}

/// What a refusal says, on the status line.
pub fn refusal_text(why: &Refusal) -> UiText {
    match why {
        Refusal::Load(p) => load_problem_text(*p),
        Refusal::Preparing => m::refused_preparing(),
        Refusal::Chart(Unsupported::NoDigest) => m::chart_no_digest(),
        Refusal::Chart(Unsupported::Layout { style }) => {
            let name = ddi_chart::Layout::builtin(style).map_or(style.clone(), |l| l.name);
            m::chart_unsupported(super::data::style_name(style, &name))
        }
        Refusal::Room(r) => match r {
            Rejection::NoContent | Rejection::ContentPending => m::refused_no_song(),
            Rejection::NotEnoughReady { .. } => m::refused_not_ready(),
            Rejection::StageStarted | Rejection::StartCommitted => m::refused_started(),
            Rejection::WrongPhase { .. } | Rejection::NotUpcoming | Rejection::WindowOpen => {
                m::refused_phase()
            }
            Rejection::InvalidRules(p) | Rejection::RulesNeedReview(p) => rules_problem(p),
            other => {
                let code = format!("{other:?}");
                let code = code.split([' ', '{', '(']).next().unwrap_or("").to_string();
                m::refused_other(UiText::data(code))
            }
        },
    }
}

/// Why rules cannot be used, worded.
pub fn rules_problem(p: &RulesProblem) -> UiText {
    match p {
        RulesProblem::StageCount { max, .. } => m::rules_stage_count(*max),
        RulesProblem::OverCap(c) => m::rules_over_cap(c.cap),
        RulesProblem::TooFewSeats { min, .. } => m::rules_too_few(*min),
        _ => m::rules_invalid(),
    }
}

impl Screen for RoomScreen {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "room"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.room
            .borrow_mut()
            .tick((cx.clock() * 1000.0).max(0.0) as u64);
        let mut root = self.tree(cx);
        if let Some(f) = self.initial.take() {
            cx.focus(f);
        } else if cx.focused().is_none() {
            cx.focus(stage_name(self.selected));
        }
        self.finish_build(&mut root, cx);
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        let n = self.room.borrow().stages().len();
        let (first, shown) = self.window(n);
        let mut v: Vec<Focusable> = (first..first + shown)
            .map(|i| Focusable::new(stage_name(i)))
            .collect();
        if self.choosable() {
            v.push(Focusable::new("Choose"));
        }
        if self.ready_shown() {
            v.push(Focusable::new("Ready"));
        }
        if self.start_shown() {
            v.push(Focusable::new("Start"));
        }
        v.push(Focusable::new("Rules"));
        v.push(Focusable::new("More"));
        v.push(Focusable::new("Back"));
        v
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        self.time = cx.time();
        self.live.tick(root, "Wordmark", &self.env, cx);
        self.online_update(cx);
        // The stages' songs to prepare, whatever is over the room.
        let jobs = self.room.borrow_mut().take_load_jobs();
        for job in jobs {
            (self.shell)(RoomCall::Prepare(job), cx);
        }
        let uploads = self.room.borrow_mut().take_upload_jobs();
        for job in uploads {
            (self.shell)(RoomCall::Upload(Box::new(job)), cx);
        }
        if !cx.is_current_screen() {
            self.covered = true;
            return;
        }
        if std::mem::take(&mut self.covered) {
            self.publish();
        }
        self.room
            .borrow_mut()
            .tick((cx.clock() * 1000.0).max(0.0) as u64);
        // The countdown is over: the stage is played (once).
        let order = self.room.borrow_mut().take_stage_order();
        if let Some(order) = order {
            (self.shell)(RoomCall::PlayStage(Box::new(order)), cx);
        }
        // Back from a stage: the next one shown (the one played is done).
        let phase = self.room.borrow().state().phase.kind();
        if self.phase.replace(phase) != Some(phase)
            && matches!(phase, ddi_room::PhaseKind::Intermission)
        {
            let up = self.room.borrow().state().upcoming_stage();
            let i = up.and_then(|id| self.room.borrow().stages().iter().position(|s| s.id == id));
            if let Some(i) = i {
                self.selected = i;
                cx.focus(stage_name(i));
            }
        }
        let now = (self.room.borrow().version(), self.countdown());
        let size = Layout::new(&self.env, cx);
        if self.seen != Some(now) || size.size != self.layout.size || size.zoom != self.layout.zoom
        {
            self.rebuild(root, cx);
        } else if self.env.state.borrow().screen != "room" {
            self.publish();
        }
    }

    fn animating(&self) -> bool {
        // The countdown and the readiness window run on the room's clock.
        self.countdown().is_some() || self.live.animating(&self.env)
    }

    fn wake_at(&self) -> Option<f64> {
        // An online room's messages come while nothing moves: looked for
        // ten times a second.
        self.room.borrow().is_online().then_some(self.time + 0.1)
    }

    fn message(&mut self, msg: &crate::Message, root: &mut Actor, cx: &mut Cx) {
        if msg.name == crate::controller_hints::CHANGED {
            self.rebuild(root, cx);
        }
        if msg.name == super::room_dialogs::RULES_SAVED {
            self.rebuild(root, cx);
        }
        match super::menu_pick(msg, "Room.More") {
            Some("stop") => {
                let _ = self.room.borrow_mut().stop_match();
                self.rebuild(root, cx);
            }
            Some("copy") => {
                let link = self.room.borrow().invite_link();
                if let Some(link) = link {
                    cx.request(crate::UiRequest::CopyText(link));
                    self.room
                        .borrow_mut()
                        .set_link_notice(Some(LinkNotice::InviteCopied));
                }
                self.rebuild(root, cx);
            }
            Some("rotate") => {
                self.room.borrow_mut().rotate_invite();
                self.rebuild(root, cx);
            }
            Some("signin") => self.room.borrow_mut().sign_in_again(),
            Some("retry") => {
                let up = self.room.borrow().state().upcoming_stage();
                if let Some(up) = up {
                    self.room.borrow_mut().retry_sharing(up);
                }
                self.rebuild(root, cx);
            }
            Some("leave") => {
                self.room.borrow_mut().leave();
                (self.shell)(RoomCall::Discard, cx)
            }
            _ => {}
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Focus { to: Some(to), .. } => {
                if let Some(i) = to.strip_prefix("Stage.").and_then(|i| i.parse().ok()) {
                    self.select(i, root, cx);
                }
                Handled::No
            }
            Input::Menu {
                action: MenuAction::Back,
                ..
            } => {
                (self.shell)(RoomCall::Leave, cx);
                Handled::Yes
            }
            Input::Menu {
                action: MenuAction::More,
                ..
            } => {
                self.open_more(cx);
                Handled::Yes
            }
            Input::Activate { name } => {
                if let Some(i) = name.strip_prefix("Stage.").and_then(|i| i.parse().ok()) {
                    if i != self.selected {
                        self.select(i, root, cx);
                    } else {
                        self.choose(cx);
                    }
                    return Handled::Yes;
                }
                match name.as_str() {
                    "Back" => (self.shell)(RoomCall::Leave, cx),
                    "More" => self.open_more(cx),
                    "Rules" => self.open_rules(cx),
                    "Choose" => self.choose(cx),
                    "Ready" => {
                        let ready = self.room.borrow().ready();
                        let _ = self.room.borrow_mut().set_ready(!ready);
                        self.rebuild(root, cx);
                    }
                    "Start" if self.start_shown() => {
                        self.start(cx);
                        self.rebuild(root, cx);
                    }
                    _ => {}
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}
