//! The multiplayer prototype's room, as the maintainer's match flow shows it
//! (`docs/plans/multiplayer-prototype-progress.md`, M7b): no room screen of
//! its own any more. The room's main view is song select (`select_room.rs`):
//! the host's own list, whose Start on a song picks the next stage's song
//! and starts (or goes on with) the match, and the others' song select
//! following the host's resting song with the match's picks and the players
//! beside it. Readiness gathers on the stage's start prompt (`prompt_room.rs`).
//! Rules, Players and the stage's standings are pop-ups (`room_dialogs.rs`).
//!
//! What lives here is what those screens share: the calls to the shell
//! ([`RoomCall`]), the room's per-frame pump ([`Pump`]: the client ticked,
//! the songs to prepare and share handed to the shell, the prompt opened
//! when a stage gathers), the status line and the rows they show, the room's
//! More menu, and the debug state ([`RoomView`]). Every screen binds to the
//! [`SharedRoom`] client and keeps no room logic: changes are commands the
//! room accepts or refuses.

use std::rc::Rc;

use ddi_actor::Actor;
use ddi_l10n::UiText;
use ddi_l10n::msg::{common, room as m};
use ddi_room::{
    Capability, Connection, ContentStatus, Finish, LinkState, MemberKind, Participation, Phase,
    Rejection, Role, RulesProblem, StageId, StageStanding,
};
use serde::Serialize;

use super::{Env, left, right};
use crate::look;
use crate::screen::Cx;
use crate::state::online::Redirect;
use crate::state::online::link::LinkNotice;
use crate::state::room::{
    LoadJob, LoadProblem, Refusal, RoomClient, RoomView as Where, SharedRoom, Sharing,
    SharingProblem, StageMark, StageOrder, Unsupported, UploadJob, cpu_levels,
};

/// What the room asks of its shell.
#[derive(Clone, Debug, PartialEq)]
pub enum RoomCall {
    /// Back: to the multiplayer home, the room kept for the session.
    Leave,
    /// Leave room: the practice forgotten, then back to the home.
    Discard,
    /// Prepare a stage's song on this device (read its chart, check the
    /// CPUs' plan) and answer `RoomClient::finish_load`; the spotlight's
    /// song (`state::room::SPOTLIGHT`) answers `finish_shared_load`.
    Prepare(LoadJob),
    /// A stage gathers readiness with this player in it (M7b): open its
    /// start prompt (`prompt_room.rs`: Ready, the players), which plays it
    /// once the room starts it, reports its outcome
    /// (`RoomClient::report_stage`, an online room's `report_run`) and
    /// comes back to the room.
    PlayStage(Box<StageOrder>),
    /// An online room: upload a stage's (or the spotlight's) song as the
    /// room's share with its grant, telling `RoomClient::upload_progress`
    /// and `upload_finished`.
    Upload(Box<UploadJob>),
    /// An online room's Sign in again: go where signing in needs (the
    /// server's page on the web, the system browser on desktop).
    Redirect(Redirect),
}

pub type RoomShell = Rc<dyn Fn(RoomCall, &mut Cx)>;

/// The room as the debug state shows it (`ThemeState::room`), for tests.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RoomView {
    /// What shows it: `select` (song select, the host's or following) or
    /// `prompt` (a stage's start prompt).
    pub view: String,
    /// The phase (`lobby`, `preparing`, `countdown`, `stage`, …).
    pub phase: String,
    /// The revision of the room the screen shows.
    pub revision: u64,
    /// The local player may pick (an operator): song select is theirs.
    pub host: bool,
    pub stages: Vec<RoomStageView>,
    pub members: Vec<RoomMemberView>,
    /// The CPUs the shown rules seat, and the stages.
    pub cpus: u16,
    pub stage_count: u16,
    /// The shown rules' ruleset (`MatchRules::ruleset`, the engine's id).
    pub ruleset: String,
    /// The running match's rules are locked (a match exists).
    pub locked: bool,
    pub ready: bool,
    /// Whole seconds left of the countdown or the readiness window.
    pub countdown: Option<u64>,
    /// The status line, and the refusal it says (its kind), if any.
    pub status: String,
    pub refusal: Option<String>,
    /// Finished matches kept in the room.
    pub history: usize,
    /// The screen's own controls: where a shell's debug view finds them.
    pub items: Vec<String>,
    /// The Rules pop-up is open (`first`: the one entering opens, Reset
    /// and OK); its draft (stages, CPUs) and what the room says of it.
    pub rules_open: bool,
    pub rules_first: bool,
    pub draft: Vec<u16>,
    /// The draft's ruleset (the engine's id).
    pub draft_ruleset: Option<String>,
    pub rules_problem: Option<String>,
    /// The roster pop-ups: Add custom, Players, the stage's standings.
    pub add_open: bool,
    pub players_open: bool,
    pub standings_open: bool,
    /// The local player's last stage result, as said.
    pub mark: Option<String>,
    /// The match's standings (in a match or its results), best first.
    pub standings: Vec<RoomStandingView>,
    /// The operator's resting song (its title) and, once this device has
    /// it, its song id (a member's: the room's share of it).
    pub spotlight: Option<String>,
    pub spotlight_song: Option<String>,
    /// The match's picks as the history shows them, one a stage.
    pub picks: Vec<String>,
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
    /// A bot (`bot:<name>`, M7d): its name is shown labeled Bot.
    pub bot: bool,
    /// A CPU's level, from 1, and personality.
    pub level: Option<u8>,
    pub personality: Option<String>,
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

/// A room screen's per-frame work: the client ticked on the UI's clock,
/// an online room's own (signing in again, the invite fetched for its
/// operators), the songs to prepare and upload handed to the shell
/// whatever is over the screen, and, where the screen is the current one,
/// the stage whose prompt this player belongs on opened once.
#[derive(Default)]
pub struct Pump {
    seen: Option<u64>,
}

impl Pump {
    /// Runs it; whether the room changed since the last run.
    pub fn run(&mut self, room: &SharedRoom, shell: &RoomShell, cx: &mut Cx) -> bool {
        room.borrow_mut()
            .tick((cx.clock() * 1000.0).max(0.0) as u64);
        online_update(room, shell, cx);
        let jobs = room.borrow_mut().take_load_jobs();
        for job in jobs {
            shell(RoomCall::Prepare(job), cx);
        }
        let uploads = room.borrow_mut().take_upload_jobs();
        for job in uploads {
            shell(RoomCall::Upload(Box::new(job)), cx);
        }
        let version = room.borrow().version();
        self.seen.replace(version) != Some(version)
    }

    /// The stage's prompt, where the room says this player belongs on it
    /// (a pick gathering readiness): handed to the shell once.
    pub fn open_prompt(room: &SharedRoom, shell: &RoomShell, cx: &mut Cx) -> bool {
        if room.borrow().view() == Where::Select {
            return false;
        }
        let order = room.borrow_mut().take_stage_order();
        match order {
            Some(order) => {
                shell(RoomCall::PlayStage(Box::new(order)), cx);
                true
            }
            None => false,
        }
    }
}

/// An online room's own: where signing in again sends the shell, the
/// invite asked for once the player operates the room.
fn online_update(room: &SharedRoom, shell: &RoomShell, cx: &mut Cx) {
    let (redirect, fetch) = {
        let r = room.borrow();
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
        room.borrow_mut().fetch_invite();
    }
    if let Some(r) = redirect {
        shell(RoomCall::Redirect(r), cx);
    }
}

/// The room's name for the header: CPU PRACTICE, or the online room's.
pub fn heading(r: &RoomClient) -> UiText {
    match (r.is_online(), r.name()) {
        (false, _) => m::practice_heading(),
        (true, Some(name)) => UiText::data(name.to_uppercase()),
        (true, None) => m::home_heading(),
    }
}

/// A stage's number in the shown stages, from 1.
fn number_of(r: &RoomClient, id: StageId) -> usize {
    r.stages()
        .iter()
        .position(|st| st.id == id)
        .map_or(1, |i| i + 1)
}

/// Whole seconds left of the countdown or the readiness window (a window
/// waiting for its song to be shared has none yet).
pub fn seconds_left(r: &RoomClient) -> Option<u64> {
    let s = r.state();
    let at = match &s.phase {
        Phase::Countdown { start_at, .. } => *start_at,
        Phase::Preparing {
            stage,
            window: Some(w),
        } if !s.content_pending(*stage) => w.deadline,
        _ => return None,
    };
    Some(at.0.saturating_sub(r.now_ms()).div_ceil(1000))
}

/// The room's status line (M7b: above the list on song select, the
/// panel's first line for the others, the prompt's under its song): what
/// the room waits for, and the last refusal or link change.
pub fn status(r: &RoomClient) -> UiText {
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
    let host = r.may(Capability::ChooseContent);
    let seconds = seconds_left(r).unwrap_or(0);
    match &s.phase {
        Phase::Lobby => {
            if host {
                m::status_pick_first()
            } else {
                m::status_host_picks_first()
            }
        }
        Phase::Preparing {
            stage,
            window: None,
        }
        | Phase::Intermission { next: stage } => {
            let n = number_of(r, *stage);
            if host {
                m::status_pick_next(n)
            } else {
                m::status_host_chooses(n)
            }
        }
        Phase::Preparing {
            stage,
            window: Some(_),
        } => {
            let n = number_of(r, *stage);
            if let Some(text) = sharing_status(r, *stage, n) {
                text
            } else if s.content_pending(*stage) {
                m::status_sharing(n)
            } else if let Some(p) = r.load_problem(*stage) {
                load_problem_text(p)
            } else if r.loading(*stage) {
                m::status_preparing(n)
            } else {
                m::status_gathering(n, seconds)
            }
        }
        Phase::Countdown { stage, .. } => m::status_countdown(number_of(r, *stage), seconds),
        Phase::Stage { stage } | Phase::SongCompletion { stage } => {
            let n = number_of(r, *stage);
            match s.participation(r.me()) {
                Some(Participation::SittingOut { .. }) => m::status_sitting_out(n),
                _ if r.is_online() => m::status_online_stage(n),
                _ => m::status_playing(n),
            }
        }
        Phase::MatchResults if host => m::status_pick_new_match(),
        Phase::Interrupted { .. } if host => m::status_stopped_pick(),
        Phase::MatchResults | Phase::Interrupted { .. } => m::status_host_picks_new_match(),
    }
}

/// One person's or CPU's row, as the players panel, its pop-up and the
/// prompt show it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerRow {
    pub name: String,
    /// Under the name: a role, a CPU's level and personality, the points.
    pub sub: String,
    /// At the right: ready or not, a connection's trouble, the place.
    pub state: String,
    pub me: bool,
    /// Ready (or a CPU, which needs no readiness) for the upcoming stage.
    pub ready: bool,
    pub cpu: bool,
}

/// A CPU's personality's name (`Streams`), or Custom for multipliers of
/// its own.
pub fn personality_text(skills: Option<&str>) -> UiText {
    match skills.map(crate::state::cpu_roster::preset) {
        None | Some(Some("balanced")) => m::cpu_style_balanced(),
        Some(Some("stream")) => m::cpu_style_stream(),
        Some(Some("jacks")) => m::cpu_style_jacks(),
        Some(Some("chords")) => m::cpu_style_chords(),
        Some(Some("stamina")) => m::cpu_style_stamina(),
        Some(Some("technical")) => m::cpu_style_technical(),
        Some(_) => m::cpu_style_custom(),
    }
}

/// A CPU's name: a roster's (data), else "CPU n".
pub fn cpu_name(spec: &ddi_room::OpponentSpec, ordinal: u16) -> String {
    match &spec.name {
        Some(n) => UiText::data(n).into(),
        None => m::cpu_name(ordinal).into(),
    }
}

/// A CPU's line under its name: its level of the engine's, and its
/// personality.
pub fn cpu_sub(spec: &ddi_room::OpponentSpec) -> String {
    format!(
        "{} · {}",
        m::cpu_level(u16::from(spec.level) + 1, cpu_levels()),
        personality_text(spec.personality.as_deref())
    )
}

/// Everyone in the room: the people present, then the CPUs (the match's,
/// or before it those its rules will seat).
pub fn player_rows(r: &RoomClient) -> Vec<PlayerRow> {
    let s = r.state();
    let standings = s.standings().unwrap_or_default();
    let in_match = s.current.is_some() && s.phase != Phase::Lobby;
    let points = |id| {
        standings
            .iter()
            .find(|st| st.participant == id)
            .filter(|_| in_match)
            .map(|st| m::rank_points(st.rank, points_text(st.total)).into())
    };
    let upcoming = s.upcoming_stage().and_then(|id| s.stage(id));
    let mut rows = Vec::new();
    for member in s.members.values().filter(|mb| !mb.left) {
        match &member.kind {
            MemberKind::Human { .. } => {
                let me = member.id == r.me();
                let name: String = if me {
                    m::you().into()
                } else {
                    person_name(member)
                };
                let prep = upcoming.map(|st| st.prep(member.id));
                let part = s.participation(member.id);
                // Ready for the upcoming stage, or in the lineup of the one
                // counting down (the prompt's count and rows agree).
                let ready = prep.as_ref().is_some_and(|p| p.ready)
                    || matches!(part, Some(Participation::Committed { .. }));
                let state: String = match part {
                    Some(Participation::ReadyLoaded { .. } | Participation::Committed { .. }) => {
                        m::state_ready().into()
                    }
                    Some(Participation::ReadyLoading { .. }) => match prep.map(|p| p.loading) {
                        Some(ddi_room::Loading::Loading { permille }) if permille > 0 => {
                            m::state_loading_percent(u32::from(permille / 10)).into()
                        }
                        _ => m::state_loading().into(),
                    },
                    Some(Participation::NotReady { .. }) if s.current.is_some() => {
                        m::state_not_ready().into()
                    }
                    Some(Participation::LoadFailed { .. }) => m::state_load_failed().into(),
                    Some(Participation::Competing { .. }) => m::state_playing().into(),
                    Some(Participation::SittingOut { .. }) => m::state_sitting_out().into(),
                    _ => String::new(),
                };
                let (sub, state) = person_row(s, member, points(member.id), state);
                rows.push(PlayerRow {
                    name,
                    sub,
                    state,
                    me,
                    ready,
                    cpu: false,
                });
            }
            MemberKind::Cpu { ordinal, opponent } => rows.push(PlayerRow {
                name: cpu_name(opponent, *ordinal),
                sub: cpu_sub(opponent),
                state: points(member.id).unwrap_or_default(),
                me: false,
                ready: true,
                cpu: true,
            }),
        }
    }
    if !in_match {
        // Before the match: the CPUs its rules will seat.
        let rules = &s.next.rules;
        let roster = crate::state::cpu_roster::roster_of(rules, r.humans());
        for (n, spec) in roster.iter().enumerate() {
            rows.push(PlayerRow {
                name: cpu_name(spec, u16::try_from(n + 1).unwrap_or(u16::MAX)),
                sub: cpu_sub(spec),
                state: m::cpu_at_start().into(),
                me: false,
                ready: true,
                cpu: true,
            });
        }
    }
    rows
}

/// A person's row: under the name their role (Host, Co-host, Admin;
/// Watching, Next match) and their points in a match; at the right their
/// connection when not connected, else `state`.
fn person_row(
    s: &ddi_room::RoomState,
    member: &ddi_room::Member,
    points: Option<String>,
    state: String,
) -> (String, String) {
    let role: Option<String> = match (member.role, member.admin) {
        (Role::Owner, _) if s.source == ddi_room::ContentSource::Shared => {
            Some(m::role_host().into())
        }
        (Role::CoHost, _) => Some(m::role_co_host().into()),
        (Role::Member, true) => Some(m::role_admin().into()),
        _ => None,
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
        Connection::Connected => state,
    };
    (sub, state)
}

/// One stage of the match as its picks' history shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct PickRow {
    /// "Stage 2".
    pub stage: String,
    /// The song's title (none: not picked yet).
    pub title: Option<String>,
    /// "Hard 9", and the local player's result once confirmed.
    pub sub: String,
    /// The stage being gathered or played now.
    pub now: bool,
}

/// The match's stages (the next match's before one starts): what was
/// picked for each, the stage now, and the player's result of those
/// played.
pub fn pick_rows(r: &RoomClient) -> Vec<PickRow> {
    let s = r.state();
    let now = match &s.phase {
        Phase::Preparing { stage, .. }
        | Phase::Countdown { stage, .. }
        | Phase::Stage { stage }
        | Phase::SongCompletion { stage } => Some(*stage),
        _ => None,
    };
    r.stages()
        .iter()
        .enumerate()
        .map(|(i, st)| {
            let picked = st.content.as_ref().and_then(|c| r.picked(&c.id));
            let card = st.content.as_ref().and_then(|c| c.card.as_ref());
            let title = picked
                .map(|p| p.song.title.clone())
                .or_else(|| card.map(|c| c.title.clone()))
                .or_else(|| st.content.as_ref().map(|_| m::host_song().into()));
            // The chart's own words (its difficulty, or an edit's name).
            let chart: Option<String> = picked
                .map(|p| {
                    UiText::data(format!("{} {}", p.chart.label(), p.chart.meter_text())).into()
                })
                .or_else(|| {
                    card.filter(|c| !c.difficulty.is_empty())
                        .map(|c| UiText::data(format!("{} {}", c.difficulty, c.meter)).into())
                });
            let mark = r.mark_of(st.id).map(|mk| mark_short(&mk));
            let sub = [chart, mark]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            PickRow {
                stage: m::stage(i + 1).into(),
                title: title.map(|t| UiText::data(t).into()),
                sub,
                now: Some(st.id) == now,
            }
        })
        .collect()
}

/// The player's result of a stage, short: "1st · +4 pts".
fn mark_short(mark: &StageMark) -> String {
    let p = &mark.placement;
    match p.finish {
        Finish::Forfeit => m::mark_short_forfeit(p.place, mark.field).into(),
        _ => m::mark_short(p.place, mark.field, points_text(p.points)).into(),
    }
}

/// The room's More menu's items: its rules and players where the header
/// has no room for their buttons (`compact`), stopping the match, an
/// online room's invite, sign-in and sharing, and leaving.
pub fn more_items(r: &RoomClient, compact: bool) -> Vec<(String, String)> {
    let mut items = Vec::new();
    if compact {
        items.push(("rules".to_string(), m::rules().into()));
        items.push(("players".to_string(), m::players().into()));
    }
    let running = r.state().phase.kind().match_running();
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
    items.push(("leave".to_string(), m::leave().into()));
    items
}

/// The name of the room's More menu's pick message.
pub const MORE: &str = "Room.More";

/// Opens the room's More menu.
pub fn open_more(env: &Env, room: &SharedRoom, compact: bool, cx: &mut Cx) {
    let items = more_items(&room.borrow(), compact);
    super::action_menu(env, &common::more(), items, MORE, cx);
}

/// What a pick of More leaves to the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoreAction {
    Rules,
    Players,
    /// Done here (or nothing): the screen redraws.
    Done,
}

/// Does what More's `id` asks.
pub fn more_picked(id: &str, room: &SharedRoom, shell: &RoomShell, cx: &mut Cx) -> MoreAction {
    match id {
        "rules" => return MoreAction::Rules,
        "players" => return MoreAction::Players,
        "stop" => {
            let _ = room.borrow_mut().stop_match();
        }
        "copy" => {
            let link = room.borrow().invite_link();
            if let Some(link) = link {
                cx.request(crate::UiRequest::CopyText(link));
                room.borrow_mut()
                    .set_link_notice(Some(LinkNotice::InviteCopied));
            }
        }
        "rotate" => room.borrow_mut().rotate_invite(),
        "signin" => room.borrow_mut().sign_in_again(),
        "retry" => {
            let up = room.borrow().state().upcoming_stage();
            if let Some(up) = up {
                room.borrow_mut().retry_sharing(up);
            }
        }
        "leave" => {
            room.borrow_mut().leave();
            room.borrow_mut().set_rules_seen(false);
            shell(RoomCall::Discard, cx);
        }
        _ => {}
    }
    MoreAction::Done
}

/// The rows the players panel scrolls through: the picks' history, then
/// the players, each under its heading.
#[derive(Clone, Debug, PartialEq)]
pub enum PanelRow {
    Heading(String, String),
    Pick(PickRow),
    Player(PlayerRow),
}

/// The panel's rows for the room now.
pub fn panel_rows(r: &RoomClient) -> Vec<PanelRow> {
    let picks = pick_rows(r);
    let players = player_rows(r);
    let mut rows = Vec::new();
    let picked = picks.iter().filter(|p| p.title.is_some()).count();
    rows.push(PanelRow::Heading(
        m::picks_heading().into(),
        m::picks_count(picked, picks.len()).into(),
    ));
    rows.extend(picks.into_iter().map(PanelRow::Pick));
    let cap = r.state().policy.max_competitors;
    rows.push(PanelRow::Heading(
        m::players().into(),
        m::players_count(players.len(), cap).into(),
    ));
    rows.extend(players.into_iter().map(PanelRow::Player));
    rows
}

/// A row's height in the panel.
pub fn panel_row_height(row: &PanelRow) -> f32 {
    match row {
        PanelRow::Heading(..) => 30.0,
        _ => 44.0,
    }
}

/// How many of `rows` from `first` fit in `height`.
pub fn panel_fit(rows: &[PanelRow], first: usize, height: f32) -> usize {
    let mut used = 0.0;
    let mut n = 0;
    for row in rows.iter().skip(first) {
        used += panel_row_height(row);
        if used > height {
            break;
        }
        n += 1;
    }
    n.max(1).min(rows.len().saturating_sub(first))
}

/// The panel's scrolled window: its rows from `first`, `w` wide, at most
/// `height` tall, at `[x, y]`, with Up and Down marks (`Panel.Up`,
/// `Panel.Down`: taps scroll) where rows are hidden above or below. Its
/// first row at most the last that still fills it.
pub fn panel(
    env: &Env,
    rows: &[PanelRow],
    first: usize,
    [x, y, w, height]: [f32; 4],
) -> (Actor, usize) {
    let mut children = Vec::new();
    // The last first row that still shows the end.
    let mut last_first = rows.len();
    let mut used = 0.0;
    while last_first > 0 {
        let h = panel_row_height(&rows[last_first - 1]);
        if used + h > height - 24.0 {
            break;
        }
        used += h;
        last_first -= 1;
    }
    let first = first.min(last_first);
    let room = if first > 0 || last_first > 0 {
        height - 24.0
    } else {
        height
    };
    let n = panel_fit(rows, first, room);
    let mut ry = y + if first > 0 { 24.0 } else { 0.0 };
    if first > 0 {
        children.push(env.component(
            "Link",
            "Button",
            &look::Spec::new([w, 22.0], &*m::panel_more_above(first)),
            "Panel.Up",
            [x + w / 2.0, y + 11.0],
        ));
    }
    for (k, row) in rows.iter().enumerate().skip(first).take(n) {
        let h = panel_row_height(row);
        match row {
            PanelRow::Heading(title, count) => {
                let head = look::tracked(
                    env.text(&format!("Panel{k}.Heading"), title, 11.0, 700, "Faint"),
                    120,
                );
                children.push(left(head, x, ry + h / 2.0 + 2.0));
                children.push(right(
                    env.text(&format!("Panel{k}.Count"), count, 12.0, 400, "Muted"),
                    x + w,
                    ry + h / 2.0 + 2.0,
                ));
            }
            PanelRow::Pick(p) => {
                let mut stage = env.text(&format!("Panel{k}.Stage"), &p.stage, 12.0, 600, "Muted");
                look::ellipsized(&mut stage, 72.0);
                children.push(left(stage, x, ry + 13.0));
                let (title, color) = match &p.title {
                    Some(t) => (t.clone(), if p.now { "Accent" } else { "Text" }),
                    None => (m::stage_empty().into(), "Faint"),
                };
                let mut t = env.text(&format!("Panel{k}.Title"), &title, 15.0, 700, color);
                look::ellipsized(&mut t, w - 80.0);
                children.push(left(t, x + 80.0, ry + 13.0));
                if !p.sub.is_empty() {
                    let mut sub = env.text(&format!("Panel{k}.Sub"), &p.sub, 12.0, 400, "Muted");
                    look::ellipsized(&mut sub, w - 80.0);
                    children.push(left(sub, x + 80.0, ry + 31.0));
                }
            }
            PanelRow::Player(p) => {
                let color = if p.me { "Text" } else { "Secondary" };
                let state_w = look::mono_width(&p.state, 13.0).min(w * 0.45);
                let mut name = env.text(&format!("Panel{k}.Name"), &p.name, 15.0, 600, color);
                look::ellipsized(&mut name, (w - state_w - 12.0).max(40.0));
                children.push(left(name, x, ry + 13.0));
                if !p.sub.is_empty() {
                    let mut sub = env.text(&format!("Panel{k}.Sub"), &p.sub, 12.0, 400, "Muted");
                    look::ellipsized(&mut sub, (w - state_w - 12.0).max(40.0));
                    children.push(left(sub, x, ry + 31.0));
                }
                let color = if p.ready && !p.cpu { "Accent" } else { "Muted" };
                let mut state = env.text(&format!("Panel{k}.State"), &p.state, 13.0, 600, color);
                look::ellipsized(&mut state, w * 0.45);
                children.push(right(state, x + w, ry + 13.0));
            }
        }
        ry += h;
    }
    let below = rows.len() - first - n;
    if below > 0 {
        children.push(env.component(
            "Link",
            "Button",
            &look::Spec::new([w, 22.0], &*m::panel_more_below(below)),
            "Panel.Down",
            [x + w / 2.0, y + height - 11.0],
        ));
    }
    (super::frame("Panel", children), first)
}

/// Says what a room screen shows (`ThemeState::room`): `view` (`select`,
/// `prompt`), its controls `items`; what the pop-ups say of themselves is
/// kept.
pub fn publish(env: &Env, room: &SharedRoom, view: &str, items: Vec<String>) {
    let r = room.borrow();
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
                        ContentStatus::Pending => "pending",
                        ContentStatus::Available => "available",
                        ContentStatus::Unavailable { .. } => "unavailable",
                    }
                    .to_string()
                }),
                share: st.content.as_ref().and_then(|c| c.share.clone()),
            }
        })
        .collect();
    let standings = s.standings().unwrap_or_default();
    let upcoming = s.upcoming_stage().and_then(|id| s.stage(id));
    let mut members: Vec<RoomMemberView> = s
        .members
        .values()
        .filter(|mb| !mb.left)
        .map(|mb| RoomMemberView {
            id: mb.id.0,
            name: match &mb.kind {
                MemberKind::Cpu { ordinal, opponent } => opponent
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("CPU {ordinal}")),
                MemberKind::Human { name, .. } => name.clone(),
            },
            role: snake(&mb.role),
            admin: mb.admin,
            connection: snake(&mb.connection),
            participation: s
                .participation(mb.id)
                .map(|p| snake(&p))
                .unwrap_or_default(),
            cpu: mb.is_cpu(),
            bot: mb.is_bot(),
            level: s.opponent(mb.id).map(|o| o.level + 1),
            personality: s.opponent(mb.id).and_then(|o| o.personality.clone()),
            ready: upcoming.is_some_and(|st| st.prep(mb.id).ready),
            points: standings
                .iter()
                .find(|st| st.participant == mb.id)
                .map(|st| st.total.to_string())
                .unwrap_or_default(),
            at_start: false,
        })
        .collect();
    let roster = crate::state::cpu_roster::roster_of(&s.next.rules, r.humans());
    if s.phase == Phase::Lobby || s.current.is_none() {
        for (n, spec) in roster.iter().enumerate() {
            members.push(RoomMemberView {
                name: spec
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("CPU {}", n + 1)),
                cpu: true,
                level: Some(spec.level + 1),
                personality: spec.personality.clone(),
                at_start: true,
                ..RoomMemberView::default()
            });
        }
    }
    let spot = s.spotlight.as_ref();
    let view = RoomView {
        view: view.into(),
        phase: format!("{:?}", s.phase.kind()).to_lowercase(),
        revision: s.revision.0,
        host: r.may(Capability::ChooseContent),
        stages,
        members,
        cpus: u16::try_from(crate::state::cpu_roster::roster_of(rules, r.humans()).len())
            .unwrap_or(u16::MAX),
        stage_count: rules.stages,
        ruleset: rules.ruleset.clone(),
        locked: r.locked_rules().is_some(),
        ready: r.ready(),
        countdown: seconds_left(&r),
        status: status(&r).into(),
        refusal: r.refusal().map(|f| format!("{f:?}")),
        history: s.history.len(),
        items,
        mark: r.last_mark().map(|mk| mark_text(&mk)),
        standings: standings
            .iter()
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
            .collect(),
        spotlight: spot.map(|sp| sp.card.title.clone()),
        spotlight_song: r
            .spotlight()
            .and_then(|(_, p)| p)
            .map(|p| p.song.id.clone()),
        picks: pick_rows(&r)
            .into_iter()
            .map(|p| match p.title {
                Some(t) => format!("{}: {t} {}", p.stage, p.sub),
                None => p.stage,
            })
            .collect(),
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
    let mut st = env.state.borrow_mut();
    let kept = st.room.take().unwrap_or_default();
    st.room = Some(RoomView {
        rules_open: kept.rules_open,
        rules_first: kept.rules_first,
        draft: kept.draft,
        draft_ruleset: kept.draft_ruleset,
        rules_problem: kept.rules_problem,
        add_open: kept.add_open,
        players_open: kept.players_open,
        standings_open: kept.standings_open,
        ..view
    });
}

/// Changes what the pop-ups say of themselves in the debug state.
pub fn publish_dialog(env: &Env, f: impl FnOnce(&mut RoomView)) {
    if let Ok(mut st) = env.state.try_borrow_mut() {
        f(st.room.get_or_insert_with(RoomView::default));
    }
}

/// A person's name as the screens show it: theirs (data), labeled Bot for
/// a bot (M7d).
pub(crate) fn person_name(m: &ddi_room::Member) -> String {
    shown_name(&member_name(m), m.is_bot())
}

/// `name` (data) as shown: labeled Bot where `bot`.
pub(crate) fn shown_name(name: &str, bot: bool) -> String {
    if bot {
        m::bot_name(UiText::data(name)).into()
    } else {
        UiText::data(name).into()
    }
}

pub(crate) fn member_name(m: &ddi_room::Member) -> String {
    match &m.kind {
        MemberKind::Human { name, .. } => name.clone(),
        MemberKind::Cpu { ordinal, opponent } => opponent
            .name
            .clone()
            .unwrap_or_else(|| format!("CPU {ordinal}")),
    }
}

/// The local player's result of a stage, as a line (the standings pop-up,
/// the stage's results).
pub fn mark_text(mark: &StageMark) -> String {
    let p = &mark.placement;
    let points = points_text(p.points);
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
pub(crate) fn snake<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(serde_json::Value::Object(o)) => o.keys().next().cloned().unwrap_or_default(),
        _ => String::new(),
    }
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
    if let ContentStatus::Unavailable { code } = &content.status {
        return Some(if code == "digest_mismatch" {
            m::status_share_mismatch(number)
        } else {
            m::status_share_unavailable(number)
        });
    }
    if content.status != ContentStatus::Pending {
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
            Rejection::NotEnoughReady { .. } => m::refused_too_few(),
            Rejection::StageStarted | Rejection::StartCommitted => m::refused_started(),
            Rejection::WrongPhase { .. } | Rejection::NotUpcoming | Rejection::WindowOpen => {
                m::refused_phase()
            }
            Rejection::Unauthorized { .. } => m::refused_host_only(),
            Rejection::InvalidRules(p) | Rejection::RulesNeedReview(p) => rules_problem(p),
            other => {
                let code = format!("{other:?}");
                let code = code.split([' ', '{', '(']).next().unwrap_or("").to_string();
                m::refused_other(UiText::data(code))
            }
        },
    }
}

/// Match points as the language writes a number ("1.5", "9.33"; "1,5"
/// where its decimal mark is a comma): the room's exact fraction rounded
/// to hundredths, for showing only (`ddi_room::Points` keeps and compares
/// the fraction).
pub fn points_text(p: ddi_room::Points) -> UiText {
    m::points_value((p.as_f64() * 100.0).round() / 100.0)
}

/// A ruleset's name by the engine's preset id, as Options names it: as
/// its game names it (data), "(approximate)" where this game follows it
/// only approximately.
pub fn ruleset_name(id: &str) -> UiText {
    let r = ddi_engine::rules::presets::by_id(id).unwrap_or_else(ddi_engine::rules::presets::itg);
    if r.approximate {
        ddi_l10n::msg::options::ruleset_approximate(&r.name)
    } else {
        UiText::data(&r.name)
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
