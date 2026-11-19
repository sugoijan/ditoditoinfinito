//! The multiplayer prototype's room, as the maintainer's match flow shows it
//! (`docs/plans/multiplayer-prototype-progress.md`, M7b): no room screen of
//! its own any more. The room's main view is song select (`select_room.rs`):
//! the picker's own list (one picker at a time, the room's owner at first:
//! `RoomClient::chooses`), whose Start on a song picks the next stage's song
//! and starts (or goes on with) the match, and the others' song select
//! following the picker's resting song with the match's picks and the
//! players beside it; an operator who follows can take the pick over
//! ([`TAKE_PICK`]). Readiness gathers on the stage's start prompt (`prompt_room.rs`).
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
    LastSongChoice,
    LoadJob, LoadProblem, Refusal, RoomClient, RoomView as Where, SharedRoom, Sharing,
    SharingProblem, StageMark, StageOrder, Unsupported, UploadJob, cpu_levels,
};

/// What the room asks of its shell.
#[derive(Clone, Debug, PartialEq)]
pub enum RoomCall {
    /// Back from a practice room between matches: to the multiplayer
    /// home, the room kept for the session.
    Leave,
    /// The room left (`leave_now`: More's Leave room, Back on an online
    /// room's song select, a practice with a match running): forgotten
    /// (an online one told first), then back to the home.
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
    /// How many watch with the room's audience and whether it is open
    /// (none where it has none or before it said).
    pub audience: Option<(u32, bool)>,
    /// The phase (`lobby`, `preparing`, `countdown`, `stage`, …).
    pub phase: String,
    /// The revision of the room the screen shows.
    pub revision: u64,
    /// The local player chooses the room's songs now (the picker): song
    /// select is theirs.
    pub host: bool,
    /// Who chooses now (a member's id; none: nobody), and whether the
    /// local player may take the pick over (the Pick songs action shows).
    pub picker: Option<u64>,
    pub can_take_pick: bool,
    pub stages: Vec<RoomStageView>,
    pub members: Vec<RoomMemberView>,
    /// The CPUs the shown rules seat, and the stages.
    pub cpus: u16,
    pub stage_count: u16,
    /// The shown rules' ruleset (`MatchRules::ruleset_id`, the engine's
    /// id: DDI), their difficulty (`easy`, `normal`, `hard`), Boos and
    /// Attacks, and whether Attacks can be set here (practice).
    pub ruleset: String,
    pub difficulty: String,
    pub boos: bool,
    pub attacks: bool,
    pub attacks_offered: bool,
    /// The local player's attack policy (`spread`, `retaliate`,
    /// `leader`).
    pub targeting: String,
    /// The local player chose Watch (`watch-v0`: `RoomClient::watching`),
    /// and whether they may change between Play and Watch now.
    pub watching: bool,
    pub can_choose_intent: bool,
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
    /// The draft's difficulty (`easy`, `normal`, `hard`), Boos and
    /// Attacks.
    pub draft_rules: Option<(String, bool, bool)>,
    /// The draft's Points choice (`strong`, `medium`, `mild`) where the
    /// room offers it.
    pub draft_points: Option<String>,
    /// The draft's Charts choice (`as_picked`, `adaptive`) where the
    /// room offers it.
    pub draft_charts: Option<String>,
    /// The draft's Attacks at once (`one`, `two`, `three`, `no_limit`),
    /// where its row shows (attacks offered and On, the authority honoring
    /// it).
    pub draft_at_once: Option<String>,
    /// The draft's Shocks per attack (`two`, `four`, `six`, `eight`),
    /// where its row shows (with Attacks at once).
    pub draft_shocks: Option<String>,
    pub rules_problem: Option<String>,
    /// While a match runs, the pop-up says so at its top (`rules_running`),
    /// marks the rows whose draft differs from the running match's
    /// (`stages`, `difficulty`, `boos`, `charts`, `cpus`, `attacks`, `points`), and offers
    /// Apply and restart to who may use it (`restart_offered`).
    pub rules_running: bool,
    pub rules_changed: Vec<String>,
    pub restart_offered: bool,
    /// During a match whose room takes changes from a later stage on:
    /// where the pop-up's edits go (`stage`, `next`), the stage a change
    /// lands on (its number), and the stage's rules as edited
    /// (difficulty, Boos, Attacks, Charts).
    pub rules_destination: Option<String>,
    pub rules_from: Option<u16>,
    pub stage_draft_rules: Option<(String, bool, bool, String)>,
    /// The stage's Points as edited where Points may change from a later
    /// stage (`strong`, `medium`, `mild`), and, where it differs from the
    /// stage before, that stage's (the sentence under the row says the
    /// totals shift).
    pub stage_draft_points: Option<String>,
    pub points_shift: Option<String>,
    /// The roster pop-ups: Add custom, Players, the stage's standings.
    pub add_open: bool,
    pub players_open: bool,
    pub standings_open: bool,
    /// What leaving the room now would cost (`RoomClient::leave_cost`:
    /// `host_pass`, `host_close`, `host_alone`, `playing`,
    /// `picking`, `co_host`; none: nothing, Back leaves at once, as
    /// always in CPU practice), the sentence the question would say, and
    /// whether the question is open.
    pub leave_cost: Option<String>,
    pub leave_text: Option<String>,
    pub leave_open: bool,
    /// The notice that a stage is due is open over a screen over the
    /// room ([`DueDialog`]): the stage's number, and whether it is the
    /// player's to watch only.
    pub due_open: Option<(u16, bool)>,
    /// The host's: Host options is open; the local player may change the
    /// room's hosts (an online room's owner); and what the room does when
    /// its host leaves (`pass_on`, `close`; none: a room that does not
    /// pass on, or practice).
    pub host_open: bool,
    pub manages_hosts: bool,
    pub host_leaves: Option<String>,
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
    /// Their attack policy (`spread`, `retaliate`, `leader`), whatever
    /// the match's rules.
    pub targeting: String,
    /// They choose the room's songs now.
    pub picker: bool,
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
            .tick(cx.wall_ms());
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
                // A leave question left open goes first: what it said
                // leaving costs was song select's, and under the prompt
                // it would be buried (ui-screens-7).
                cx.broadcast(crate::Message::new(STAGE_OPENS));
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
pub(crate) fn number_of(r: &RoomClient, id: StageId) -> usize {
    r.stages()
        .iter()
        .position(|st| st.id == id)
        .map_or(1, |i| i + 1)
}

/// Whole seconds left of the countdown or the readiness window (a window
/// waiting for its song to be shared has none yet; one waiting for its
/// players to load, `hosted-deadline-v2`, the loading grace's). Counted
/// on the room's clock where it is estimated (an online room that plays
/// live: its deadlines are the room's), else on this device's.
pub fn seconds_left(r: &RoomClient) -> Option<u64> {
    let s = r.state();
    let at = match &s.phase {
        Phase::Countdown { start_at, .. } => *start_at,
        Phase::Preparing {
            stage,
            window: Some(w),
        } if !s.content_pending(*stage) => w.loading_until.unwrap_or(w.deadline),
        _ => return None,
    };
    let now = r.room_now().map_or_else(|| r.now_ms(), |m| m.0);
    Some(at.0.saturating_sub(now).div_ceil(1000))
}

/// How many players the room's loading grace still waits for
/// (`hosted-deadline-v2`: connected, wanting to play the upcoming stage,
/// its song not loaded yet), while it waits; `None` once the Ready
/// countdown counts.
pub fn loading_waits(r: &RoomClient) -> Option<usize> {
    let s = r.state();
    let Phase::Preparing {
        stage,
        window: Some(w),
    } = &s.phase
    else {
        return None;
    };
    if !w.loading() || s.content_pending(*stage) {
        return None;
    }
    let st = s.stage(*stage)?;
    // Of who counts among the players ([`plays`]), the people connected.
    let plays = plays(s);
    Some(
        s.candidates()
            .into_iter()
            .filter(|id| {
                plays(*id)
                    && s.members.get(id).is_some_and(|m| {
                        !m.is_cpu() && m.connection == ddi_room::Connection::Connected
                    })
                    && matches!(
                    st.prep(*id).loading,
                    ddi_room::Loading::NotStarted | ddi_room::Loading::Loading { .. }
                )
            })
            .count(),
    )
}

/// A person's state at the right of their row, the same on every list
/// (the players panel, the Players pop-up, the Ready card, a phone's
/// strip). The room's order of states for a stage, in one place
/// (feedback round N1):
///
/// 1. nothing, while no song is picked for the next stage (the lobby,
///    between stages) or nothing is asked of them (watching, waiting for
///    the next match: their seat's word says so under the name);
/// 2. Waiting for song, once a song is picked and until their device says
///    it is loading it (the picker's upload may still be on its way);
/// 3. Loading, with how far where their device said;
/// 4. Not ready, once it is loaded: the Ready screen is theirs to press;
/// 5. Ready;
/// 6. during the stage, Playing, or that they sit it out and why
///    ([`sat_out_word`]).
///
/// A song that failed to load says so in 3's place; a connection's
/// trouble is said in the state's place whatever it is ([`conn_word`]).
fn person_state(
    s: &ddi_room::RoomState,
    part: Option<&Participation>,
    prep: Option<&ddi_room::Prep>,
) -> String {
    let gathering = matches!(
        s.phase,
        Phase::Preparing {
            window: Some(_),
            ..
        }
    );
    match part {
        Some(Participation::ReadyLoaded { .. } | Participation::Committed { .. }) => {
            m::state_ready().into()
        }
        Some(Participation::LoadFailed { .. }) => m::state_load_failed().into(),
        Some(Participation::Competing { .. }) => m::state_playing().into(),
        // The room's own reason: since room model revision 12 it says
        // still loading for one whose song had not loaded.
        Some(Participation::SittingOut { reason, .. }) => sat_out_word(*reason).into(),
        Some(Participation::NotReady { .. } | Participation::ReadyLoading { .. }) if gathering => {
            match prep.map(|p| &p.loading) {
                Some(ddi_room::Loading::Loading { permille }) if *permille > 0 => {
                    m::state_loading_percent(u32::from(*permille / 10)).into()
                }
                Some(ddi_room::Loading::Loading { .. }) => m::state_loading().into(),
                Some(ddi_room::Loading::Loaded) => m::state_not_ready().into(),
                _ => m::state_waiting().into(),
            }
        }
        _ => String::new(),
    }
}

/// That a person sits a stage out, with why, at the right of their row.
fn sat_out_word(reason: ddi_room::SitOutReason) -> UiText {
    use ddi_room::SitOutReason;
    match reason {
        SitOutReason::NotReady => m::state_sat_out_not_ready(),
        SitOutReason::StillLoading => m::state_sat_out_loading(),
        SitOutReason::LoadFailed => m::state_sat_out_failed(),
        SitOutReason::NotConnected => m::state_sat_out_away(),
        SitOutReason::Watching | SitOutReason::Left => m::state_sitting_out(),
    }
}

/// The local player sits stage `stage` out: the room's status line and
/// the stage's prompt say why (they were not told before feedback round
/// N1: "not ready in time" whatever the reason) and what comes next
/// (feedback round P2, [`SatOutThen`]).
pub fn sat_out_status(r: &RoomClient, stage: StageId, reason: ddi_room::SitOutReason) -> UiText {
    sat_out_line(number_of(r, stage), reason, SatOutThen::of(r, stage, reason))
}

/// [`sat_out_status`]'s sentences: why, then what comes next.
pub(crate) fn sat_out_line(number: usize, reason: ddi_room::SitOutReason, then: SatOutThen) -> UiText {
    use ddi_room::SitOutReason;
    let then = then.text();
    match reason {
        SitOutReason::NotReady => m::status_sat_out_not_ready(number, &then),
        SitOutReason::StillLoading => m::status_sat_out_loading(number, &then),
        SitOutReason::LoadFailed => m::status_sat_out_failed(number, &then),
        SitOutReason::NotConnected => m::status_sat_out_away(number, &then),
        SitOutReason::Watching | SitOutReason::Left => m::status_sitting_out(number, &then),
    }
}

/// What comes next for a player sitting a stage out (feedback round P2:
/// "the sentence says what comes next"): whether this device shows them
/// the stage ([`RoomClient::watches`]: practice, or a room that plays
/// live), and whether they play the next stage (there is one, the
/// match's rules bring everyone back each stage, and they had not chosen
/// Watch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SatOutThen {
    /// Back for the next stage.
    Next { watching: bool },
    /// The match's last stage: nothing after it.
    Last { watching: bool },
    /// A missed stage ends the match for them (the match's rule that
    /// everyone returns each stage is off).
    Out { watching: bool },
    /// They chose Watch: they play again once they choose Play.
    Chosen { watching: bool },
}

impl SatOutThen {
    pub(crate) fn of(r: &RoomClient, stage: StageId, reason: ddi_room::SitOutReason) -> SatOutThen {
        let watching = r.watches();
        let current = r.state().current.as_ref();
        let rules = current.map(|m| &m.rules);
        let max = r.state().policy.max_stages;
        if current.is_some_and(|m| m.is_last(stage, max)) {
            SatOutThen::Last { watching }
        } else if reason == ddi_room::SitOutReason::Watching {
            SatOutThen::Chosen { watching }
        } else if rules.is_some_and(|rules| !rules.return_each_stage) {
            SatOutThen::Out { watching }
        } else {
            SatOutThen::Next { watching }
        }
    }

    fn text(self) -> UiText {
        match self {
            SatOutThen::Next { watching: true } => m::sat_out_then_watch_next(),
            SatOutThen::Next { watching: false } => m::sat_out_then_next(),
            SatOutThen::Last { watching: true } => m::sat_out_then_watch_last(),
            SatOutThen::Last { watching: false } => m::sat_out_then_last(),
            SatOutThen::Out { watching: true } => m::sat_out_then_watch_out(),
            SatOutThen::Out { watching: false } => m::sat_out_then_out(),
            SatOutThen::Chosen { watching: true } => m::sat_out_then_watch_chosen(),
            SatOutThen::Chosen { watching: false } => m::sat_out_then_chosen(),
        }
    }
}

/// The room's status line (M7b: above the list on song select, the
/// panel's first line for the others, the prompt's under its song): what
/// the room waits for, and the last refusal or link change; with how many
/// watch with the room's audience after it, where the room has one and
/// someone watches (`docs/plans/multiplayer-audience.md`: between stages
/// only, the maintainer, 2026-11-13: never while a stage is counted down
/// or played, nor on a stage's prompt, [`status_line`], nor over a play,
/// whose status line is the play's own).
pub fn status(r: &RoomClient) -> UiText {
    let line = status_line(r);
    // An open-ended match's last song (round AA), said before the line.
    let line = match line_stage_id(r) {
        Some(stage) if r.is_last_song(stage) => m::status_last_song(line),
        _ => line,
    };
    let in_stage = matches!(
        r.state().phase,
        Phase::Countdown { .. } | Phase::Stage { .. } | Phase::SongCompletion { .. }
    );
    match r.audience() {
        Some((count, true))
            if count > 0 && !in_stage && r.link_state() == LinkState::Connected =>
        {
            m::status_with_audience(&line, count)
        }
        _ => line,
    }
}

/// How many watch with the room's audience, for the players panel and the
/// debug state (none where the room has no audience or it is closed).
pub fn audience_words(r: &RoomClient) -> Option<UiText> {
    match r.audience()? {
        (count, true) => Some(m::audience_watching(count)),
        (_, false) => Some(m::audience_closed_short()),
    }
}

/// The room's status line alone, without how many watch with its
/// audience: a stage's prompt's (a stage is beginning there).
pub fn status_line(r: &RoomClient) -> UiText {
    let line = phase_status(r);
    // A change of the rules from a later stage, said until that stage
    // begins (for everyone: the operator who made it and the others):
    // before the line where it is the stage the line is about, else after
    // it ("Pick stage 1's song. · From stage 2 on: Medium points").
    match pending_change(r) {
        Some((n, words))
            if r.refusal().is_none()
                && r.link_notice().is_none()
                && (!r.is_online() || r.link_state() == LinkState::Connected) =>
        {
            if line_stage(r).is_none_or(|at| at == usize::from(n)) {
                m::status_rules_change(n, words, line)
            } else {
                m::status_then_rules_change(line, n, words)
            }
        }
        _ => line,
    }
}

/// Nobody but this player (and CPUs) is in the room.
fn alone(r: &RoomClient) -> bool {
    let s = r.state();
    !s.members
        .values()
        .any(|m| m.id != r.me() && !m.left && !m.is_cpu())
}

/// The stage the room's status line is about (being chosen, gathered,
/// counted down or played).
fn line_stage_id(r: &RoomClient) -> Option<StageId> {
    match r.state().phase {
        Phase::Preparing { stage, .. }
        | Phase::Intermission { next: stage }
        | Phase::Countdown { stage, .. }
        | Phase::Stage { stage }
        | Phase::SongCompletion { stage } => Some(stage),
        _ => None,
    }
}

/// The number of the stage the room's status line is about (being
/// chosen, gathered or counted down), from 1.
fn line_stage(r: &RoomClient) -> Option<usize> {
    match r.state().phase {
        Phase::Preparing { stage, .. }
        | Phase::Intermission { next: stage }
        | Phase::Countdown { stage, .. } => Some(number_of(r, stage)),
        _ => None,
    }
}

fn phase_status(r: &RoomClient) -> UiText {
    if r.is_online() {
        match r.link_state() {
            LinkState::Joining => return m::link_joining(),
            LinkState::Reconnecting { .. } => return m::link_reconnecting(),
            LinkState::SignInAgain => return m::link_sign_in_again(),
            // Never signed in here, or never a member (feedback round
            // AA): said as it is, never "Your sign-in ended".
            LinkState::SignInToEnter => return m::link_sign_in_to_enter(),
            LinkState::NotAMember => return m::link_not_a_member(),
            // Closed by the server's operator: said as it is.
            LinkState::Closed
                if r.state().closed == Some(ddi_room::Closure::ByAdministrator) =>
            {
                return m::closed_by_operator();
            }
            LinkState::Closed => return m::link_closed(),
            LinkState::Left => return m::link_left(),
            // Stopped for the person's newer device (round P1b): More's
            // Use this device, and the notice, bring it back.
            LinkState::OpenedElsewhere => return m::link_opened_elsewhere(),
            // The room's authority is newer: a reload (More's Reload on
            // the web), or a newer desktop build.
            LinkState::UpdateRequired if r.on_web() => return m::update_reload(),
            LinkState::UpdateRequired => return m::update_required(),
            // The room's authority is older: asked again by itself.
            LinkState::ServerBehind { .. } => return m::server_updating(),
            LinkState::Connected => {}
        }
    }
    // A refused pick or rest of someone who does not choose: the line
    // below says who does.
    if let Some(why) = r.refusal()
        && !matches!(why, Refusal::Room(Rejection::NotPicker { .. }))
    {
        return refusal_text(why);
    }
    if let Some(n) = r.link_notice() {
        return match n {
            LinkNotice::InviteCopied => m::invite_copied(),
            LinkNotice::InviteReplaced => m::invite_replaced(),
            LinkNotice::InviteNotReplaced => m::invite_not_replaced(),
            LinkNotice::JoinedToWatch => m::joined_to_watch(),
            LinkNotice::WatchInviteCopied => m::watch_invite_copied(),
            LinkNotice::AudienceInviteCopied => m::audience_invite_copied(),
            LinkNotice::AudienceClosed => m::audience_closed_notice(),
            LinkNotice::AudienceOpened => m::audience_opened_notice(),
        };
    }
    let s = r.state();
    let host = r.chooses();
    let seconds = seconds_left(r).unwrap_or(0);
    // Who the others follow: the picker, named; nobody, said with how to
    // take the pick where this player may.
    let picker = follows(r);
    match &s.phase {
        Phase::Lobby => match picker {
            // An online room's host alone (feedback round AA): the way
            // to bring people in, which the page's address is not.
            _ if host && r.is_online() && alone(r) => m::status_alone_invite(),
            _ if host => m::status_pick_first(),
            Follows::Named(name) => m::status_picker_first(name),
            Follows::Nobody(text) => text,
        },
        Phase::Preparing {
            stage,
            window: None,
        }
        | Phase::Intermission { next: stage } => {
            let n = number_of(r, *stage);
            match picker {
                _ if host => m::status_pick_next(n),
                Follows::Named(name) => m::status_picker_next(name, n),
                Follows::Nobody(text) => text,
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
            } else if let Some(waits) = loading_waits(r).filter(|&w| w > 0) {
                // `hosted-deadline-v2`: the room waits for its players to
                // load before the Ready countdown counts.
                m::status_loading_wait(waits, seconds)
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
                // Not in it yet, and Ready still enters (the late join,
                // round P1b): said so, never "you're watching" while the
                // Ready card offers it (round Y3).
                Some(Participation::SittingOut { .. }) if r.may_enter_late(*stage) => {
                    m::status_enter_late(n)
                }
                Some(Participation::SittingOut { reason, .. }) => {
                    sat_out_status(r, *stage, reason)
                }
                // Joined while the match ran: told why they are not in it.
                Some(Participation::WaitingNextMatch) => m::status_waiting_next_match(n),
                Some(Participation::WaitingNextStage) => m::status_waiting_next_stage(n),
                Some(Participation::WaitingPlace) => m::status_waiting_place(),
                _ if r.is_online() => m::status_online_stage(n),
                _ => m::status_playing(n),
            }
        }
        Phase::MatchResults if host => m::status_pick_new_match(),
        // Stopped with other rules saved for the next match (the Rules
        // pop-up's Apply and restart, or Apply then Stop match).
        Phase::Interrupted { .. }
            if host && s.current.as_ref().is_some_and(|m| m.rules != s.next.rules) =>
        {
            m::status_stopped_new_rules()
        }
        Phase::Interrupted { .. } if host => m::status_stopped_pick(),
        Phase::MatchResults | Phase::Interrupted { .. } => match picker {
            Follows::Named(name) => m::status_picker_new_match(name),
            Follows::Nobody(text) => text,
        },
    }
}

/// Whom a player who does not choose the songs follows, for their status
/// line.
enum Follows {
    /// The picker, by name (data; labeled Bot for a bot).
    Named(String),
    /// Nobody chooses: the line saying so (and how to take the pick, for
    /// one who may).
    Nobody(UiText),
}

fn follows(r: &RoomClient) -> Follows {
    let s = r.state();
    match s.picker().and_then(|id| s.member(id)) {
        Some(member) => Follows::Named(person_name(member)),
        None if r.may_take_pick() => Follows::Nobody(m::status_nobody_picks_take()),
        None => Follows::Nobody(m::status_nobody_picks()),
    }
}

/// The name of the Pick songs button on a following operator's song
/// select, and its item in the room's More menu: takes the pick over
/// (`RoomClient::take_pick`).
pub const TAKE_PICK: &str = "Room.TakePick";
const MORE_TAKE_PICK: &str = "takepick";

/// One person's or CPU's row, as the players panel, its pop-up and the
/// prompt show it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerRow {
    /// The name shown: the local player's own signed-in name where one is
    /// known ([`RoomClient::own_name`], with [`PlayerRow::badge`]), else
    /// "You"; anyone else's as the room has it.
    pub name: String,
    /// The local player's row shows a "You" badge after their own name
    /// ([`name_and_badge`]).
    pub badge: bool,
    /// Under the name: a role, a CPU's personality (after its level's
    /// stars), the points.
    pub sub: String,
    /// A CPU's level, from 1 to ten: drawn as stars (`crate::stars`).
    pub level: Option<u8>,
    /// At the right: ready or not, a connection's trouble, the place.
    pub state: String,
    pub me: bool,
    /// Ready (or a CPU, which needs no readiness) for the upcoming stage.
    pub ready: bool,
    pub cpu: bool,
    /// Counted among the room's players ([`plays`]): a CPU, or a person
    /// who can play now (not watching, nor waiting for the next match;
    /// in the lineup once a stage's start is committed).
    pub plays: bool,
    /// The players panel's line ([`players_panel`]): the place in the
    /// match and the match points, once a stage of it is confirmed.
    pub place: Option<u16>,
    pub points: Option<String>,
    /// A person's role, after the name (Host, Co-host, Admin).
    pub role: String,
    /// What they are doing, in words (ready, loading, why not loaded,
    /// away, sitting out, watching), and how it reads.
    pub status: String,
    pub tone: Tone,
    /// Their attack policy's word, where the match has attacks (empty
    /// otherwise): shown quietly, a policy and never a skill.
    pub policy: String,
    /// A person's id in the room (none: a CPU).
    pub id: Option<u64>,
    /// The Players pop-up of the room's host: a tap on this row opens
    /// Host options (the row is drawn on a faint plate named
    /// [`person_row_name`]). Set by the pop-up, never by the rows' maker.
    pub manage: bool,
}

/// The local player's own name as the lists show it (round D, item 5):
/// their signed-in display name with a "You" badge where one is known
/// ([`RoomClient::own_name`]), else "You" alone. `(name, badge)`.
pub fn own_label(r: &RoomClient) -> (String, bool) {
    match r.own_name() {
        Some(own) => (UiText::data(&own).into(), true),
        None => (m::you().into(), false),
    }
}

/// A participant's name as the lists show it (You, a bot labeled, a
/// CPU's own) and a CPU's level, from `member` (none: an empty name).
pub(crate) fn participant_label(
    r: &RoomClient,
    member: Option<&ddi_room::Member>,
) -> (String, Option<u8>) {
    match member {
        Some(mb) if mb.id == r.me() => (own_label(r).0, None),
        Some(mb) => match &mb.kind {
            MemberKind::Cpu { ordinal, opponent } => {
                (cpu_name(opponent, *ordinal), Some(cpu_level(opponent)))
            }
            MemberKind::Human { .. } => (person_name(mb), None),
        },
        None => (String::new(), None),
    }
}

/// The "You" badge's width at a name of `size` pixels, with the gap
/// before it.
pub fn badge_width(size: f32) -> f32 {
    let t: String = m::you().into();
    let text = badge_text_size(size);
    BADGE_GAP + look::mono_width(&t, text) + 2.0 * BADGE_PAD
}

const BADGE_GAP: f32 = 8.0;
const BADGE_PAD: f32 = 6.0;

/// How far below a run's line box's middle its letters' optical middle
/// sits, as a share of its size: the text face (Iosevka Term) puts the
/// baseline 0.375 of the size below the box's middle (both shells center
/// the face's ascent of 0.965 and descent of 0.215 in a line of 1.25),
/// and halfway between its x-height's (0.52) and its capitals' (0.735)
/// middles reads right beside a lowercase name, a capitalized one and a
/// Japanese one alike (feedback round H, item 1). Ideographs and kana
/// sit on the box's middle already (their em box centered there).
pub const OPTICAL_MIDDLE: f32 = 0.375 - (0.52 + 0.735) / 4.0;

/// Where the "You" badge sits against its name, for a name of `size`
/// pixels: the plate's middle below the name's line box's middle, and its
/// word's line box's middle below the plate's (negative: above), so the
/// plate's middle is on the name's letters' optical middle and its
/// word's letters on the plate's.
pub fn badge_offsets(size: f32) -> (f32, f32) {
    let word: String = m::you().into();
    let text = badge_text_size(size);
    let cased = word.chars().any(|c| c.is_uppercase() || c.is_lowercase());
    let inner = if cased { -OPTICAL_MIDDLE * text } else { 0.0 };
    (OPTICAL_MIDDLE * size, inner)
}

/// The badge word's size beside a name of `size` pixels.
fn badge_text_size(size: f32) -> f32 {
    (size - 3.0).max(9.0)
}

/// A name (the actor `id`) at `[x, mid]` (its left edge, its middle) in
/// `room` pixels, and, where `badge`, the quiet "You" chip after it
/// (`<id>.Badge`: a
/// faint rounded plate of the text's color, its word muted, never a color
/// the tiers or the roles use). The rule at narrow widths: the name is
/// never cut while the badge shows; where the two do not fit, the badge
/// goes first, then the name is cut. The actors, and the width the name
/// and the badge take.
#[allow(clippy::too_many_arguments)]
pub fn name_and_badge(
    env: &Env,
    id: &str,
    name: &str,
    badge: bool,
    (size, weight, color): (f32, u16, &str),
    [x, mid]: [f32; 2],
    room: f32,
) -> (Vec<Actor>, f32) {
    let full = look::mono_width(name, size);
    let chip = badge_width(size);
    let with_badge = badge && full + chip <= room;
    let name_w = full.min(room.max(24.0));
    let mut n = env.text(id, name, size, weight, color);
    look::ellipsized(&mut n, name_w);
    let mut out = vec![left(n, x, mid)];
    if !with_badge {
        return (out, name_w);
    }
    let word: String = m::you().into();
    let text = badge_text_size(size);
    let w = chip - BADGE_GAP;
    let h = text + 6.0;
    let cx = x + full + BADGE_GAP + w / 2.0;
    let (plate_dy, word_dy) = badge_offsets(size);
    let cy = mid + plate_dy;
    let mut plate = look::quad_at(
        &format!("{id}.Badge"),
        [w, h],
        look::with_alpha(env.color("Text"), 0.10),
        [cx, cy],
    );
    look::call(&mut plate, "cornerradius", &[h / 2.0]);
    out.push(plate);
    let mut t = env.text(&format!("{id}.BadgeText"), &word, text, 600, "Muted");
    look::call(&mut t, "xy", &[cx, cy + word_dy]);
    out.push(t);
    (out, full + chip)
}

/// The name of a person's row in the host's Players pop-up.
pub fn person_row_name(id: u64) -> String {
    format!("Person.{id}")
}

/// How a player's status reads in the players panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    /// Nothing to act on.
    #[default]
    Plain,
    /// Ready for the stage.
    Good,
    /// Not there now (reconnecting, away): the row is dimmed.
    Away,
    /// Their song did not load.
    Failed,
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
        Some(Some("evasive")) => m::cpu_style_evasive(),
        Some(Some("reckless")) => m::cpu_style_reckless(),
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

/// A CPU's line under its name, after its level's stars: its
/// personality.
pub fn cpu_sub(spec: &ddi_room::OpponentSpec) -> String {
    personality_text(spec.personality.as_deref()).into()
}

/// A CPU's level as shown, from 1 (`crate::stars`), from its seat's (the
/// engine's index).
pub fn cpu_level(spec: &ddi_room::OpponentSpec) -> u8 {
    shown_level(spec.level)
}

/// A CPU's level as shown, from 1, from the engine's index (a seat's, or
/// the audience's public view's).
pub fn shown_level(index: u8) -> u8 {
    index.min(cpu_levels() - 1) + 1
}

/// The stars' height beside a 12 px line, and the room after them.
pub const STARS_HEIGHT: f32 = 12.0;
pub const STARS_GAP: f32 = 7.0;

/// A CPU's level and what goes with it on a line from `x` rightwards (or,
/// `to_end`, ending at `x`), on `y`, at most `room` wide: its stars
/// (`<id>.Stars`) and `text` (`<id>.Sub`), muted, the stars nearer `x`.
pub fn level_line(
    env: &Env,
    out: &mut Vec<Actor>,
    id: &str,
    (level, text): (u8, &str),
    [x, y]: [f32; 2],
    room: f32,
    to_end: bool,
) {
    let w = crate::stars::width(STARS_HEIGHT);
    let stars = format!("{id}.Stars");
    out.push(if to_end {
        crate::stars::stars_right(env, &stars, level, STARS_HEIGHT, x, y)
    } else {
        crate::stars::stars_left(env, &stars, level, STARS_HEIGHT, x, y)
    });
    if !text.is_empty() && room > w + STARS_GAP + 24.0 {
        let mut sub = env.text(&format!("{id}.Sub"), text, 12.0, 400, "Muted");
        look::ellipsized(&mut sub, room - w - STARS_GAP);
        out.push(if to_end {
            right(sub, x - w - STARS_GAP, y)
        } else {
            left(sub, x + w + STARS_GAP, y)
        });
    }
}

/// An attack policy's word (Spread, Retaliate, Leader).
pub fn policy_word(t: ddi_room::attacks::Targeting) -> UiText {
    use ddi_room::attacks::Targeting;
    match t {
        Targeting::Spread => m::attack_policy_spread(),
        Targeting::Retaliate => m::attack_policy_retaliate(),
        Targeting::Leader => m::attack_policy_leader(),
    }
}

/// What a policy does, in a line (where there is room for it: a stage's
/// prompt; never in play).
pub fn policy_about(t: ddi_room::attacks::Targeting) -> UiText {
    use ddi_room::attacks::Targeting;
    match t {
        Targeting::Spread => m::attack_policy_spread_about(),
        Targeting::Retaliate => m::attack_policy_retaliate_about(),
        Targeting::Leader => m::attack_policy_leader_about(),
    }
}

/// Everyone in the room: the people present, then the CPUs (the match's,
/// or before it those its rules will seat).
pub fn player_rows(r: &RoomClient) -> Vec<PlayerRow> {
    let s = r.state();
    let standings = s.standings().unwrap_or_default();
    let in_match = s.current.is_some() && s.phase != Phase::Lobby;
    // Each one's standing by participant, the first where one is listed
    // twice (as a search from the top finds it).
    let mut by_id = std::collections::BTreeMap::new();
    for st in &standings {
        by_id.entry(st.participant).or_insert(st);
    }
    let points = |id| {
        by_id
            .get(&id)
            .filter(|_| in_match)
            .map(|st| m::rank_points(st.rank, points_text(st.total)).into())
    };
    // The panel's place and points: once a stage of the match is confirmed.
    let placed = in_match
        && standings.iter().any(|st| {
            st.stages
                .iter()
                .any(|sg| matches!(sg, StageStanding::Placed(_)))
        });
    let standing = |id| {
        by_id
            .get(&id)
            .filter(|_| placed)
            .map(|st| (st.rank, String::from(points_text(st.total))))
    };
    let upcoming = s.upcoming_stage().and_then(|id| s.stage(id));
    let plays = plays(s);
    let attacks = r.attacks_on();
    let policy = |t: ddi_room::attacks::Targeting| -> String {
        if attacks {
            policy_word(t).into()
        } else {
            String::new()
        }
    };
    let mut rows = Vec::new();
    for member in s.members.values().filter(|mb| !mb.left) {
        match &member.kind {
            MemberKind::Human { .. } => {
                let me = member.id == r.me();
                let (name, badge) = if me {
                    own_label(r)
                } else {
                    (person_name(member), false)
                };
                let prep = upcoming.map(|st| st.prep(member.id));
                let part = s.participation(member.id);
                // Ready for the upcoming stage, or in the lineup of the one
                // counting down (the prompt's count and rows agree).
                let ready = prep.as_ref().is_some_and(|p| p.ready)
                    || matches!(part, Some(Participation::Committed { .. }));
                let state = person_state(s, part.as_ref(), prep.as_ref());
                let (status, tone) = person_status(member, part.as_ref(), upcoming, &state);
                let (sub, state) = person_row(s, member, part.as_ref(), points(member.id), state);
                let standing = standing(member.id);
                rows.push(PlayerRow {
                    name,
                    badge,
                    sub,
                    level: None,
                    state,
                    me,
                    ready,
                    cpu: false,
                    plays: plays(member.id),
                    place: standing.as_ref().map(|(rank, _)| *rank),
                    points: standing.map(|(_, points)| points),
                    role: person_role(s, member).unwrap_or_default(),
                    status,
                    tone,
                    policy: policy(member.targeting()),
                    id: Some(member.id.0),
                    manage: false,
                });
            }
            MemberKind::Cpu { ordinal, opponent } => {
                let standing = standing(member.id);
                rows.push(PlayerRow {
                    name: cpu_name(opponent, *ordinal),
                    badge: false,
                    sub: cpu_sub(opponent),
                    level: Some(cpu_level(opponent)),
                    state: points(member.id).unwrap_or_default(),
                    me: false,
                    ready: true,
                    cpu: true,
                    plays: true,
                    place: standing.as_ref().map(|(rank, _)| *rank),
                    points: standing.map(|(_, points)| points),
                    role: String::new(),
                    status: String::new(),
                    tone: Tone::Plain,
                    policy: policy(member.targeting()),
                    id: None,
                    manage: false,
                });
            }
        }
    }
    if !in_match {
        // Before the match: the CPUs its rules will seat.
        let rules = &s.next.rules;
        let roster = crate::state::cpu_roster::roster_of(rules, r.humans());
        for (n, spec) in roster.iter().enumerate() {
            rows.push(PlayerRow {
                name: cpu_name(spec, u16::try_from(n + 1).unwrap_or(u16::MAX)),
                badge: false,
                sub: cpu_sub(spec),
                level: Some(cpu_level(spec)),
                state: m::cpu_at_start().into(),
                me: false,
                ready: true,
                cpu: true,
                plays: true,
                place: None,
                points: None,
                role: String::new(),
                status: m::cpu_at_start().into(),
                tone: Tone::Plain,
                policy: policy(spec.targeting.unwrap_or_default()),
                id: None,
                manage: false,
            });
        }
    }
    if rows.len() > ROOM_OF_NINE {
        // A large room: the local player first, then the room's operators
        // (and anyone with a role), then the people, then the CPUs, each
        // in the room's order, so the one looking and those who run the
        // room are found at once among 73.
        rows.sort_by_key(|p| match (p.me, p.role.is_empty(), p.cpu) {
            (true, ..) => 0,
            (false, false, _) => 1,
            (false, true, false) => 2,
            (false, true, true) => 3,
        });
    }
    rows
}

/// Who counts among the room's players, the one definition the Ready
/// card, the players panel, its phone strip and the Players pop-up share:
/// in the lineup of the stage whose start is committed; else, during a
/// match, one of its competitors who wants to play; else (between
/// matches) a person who wants to play. Never a watcher, nor one waiting
/// for the next match.
pub fn plays(s: &ddi_room::RoomState) -> impl Fn(ddi_room::ParticipantId) -> bool + '_ {
    let stage = match s.phase {
        Phase::Preparing { stage, .. }
        | Phase::Countdown { stage, .. }
        | Phase::Stage { stage }
        | Phase::SongCompletion { stage } => s.stage(stage),
        _ => None,
    };
    let lineup = stage.and_then(|st| st.play.as_ref()).map(|p| p.lineup.clone());
    let candidates = s.current.is_some().then(|| s.candidates());
    move |id| {
        let wants = s.member(id).is_some_and(|m| m.wants_to_play());
        match (&lineup, &candidates) {
            (Some(lineup), _) => lineup.contains(&id),
            (None, Some(candidates)) => wants && candidates.contains(&id),
            (None, None) => wants,
        }
    }
}

/// How many of `rows` count among the room's players ([`plays`]).
pub fn players_counted(rows: &[PlayerRow]) -> usize {
    rows.iter().filter(|p| p.plays).count()
}

/// The most rows a room of nine (the cap until 2026-11-13) lists: up to
/// it, its lists keep the order they always had; past it (a large room),
/// the local player first ([`player_rows`], [`panel_order`]).
pub const ROOM_OF_NINE: usize = 9;

/// The rows of a list `len` long that a window of `shown` from `first`
/// draws, keeping `me` (the local player's row) in it: where it would be
/// scrolled out, it takes the window's line on its side (the first
/// above, the last below), so the player never loses their own place
/// among many and every other line can still be scrolled to.
pub fn window_keeping(len: usize, first: usize, shown: usize, me: Option<usize>) -> Vec<usize> {
    let first = first.min(len.saturating_sub(shown));
    let end = (first + shown).min(len);
    match me.filter(|&m| m < len && shown >= 2) {
        // Above the window: first, the window's top line making room.
        Some(me) if me < first => std::iter::once(me).chain(first + 1..end).collect(),
        // Below it: last, the window's bottom line making room.
        Some(me) if me >= end => (first..end - 1).chain(std::iter::once(me)).collect(),
        _ => (first..end).collect(),
    }
}

#[cfg(test)]
#[test]
fn the_window_keeps_the_players_line_and_reaches_every_other() {
    // In the window: as it is.
    assert_eq!(window_keeping(20, 0, 5, Some(2)), vec![0, 1, 2, 3, 4]);
    // Below it: the last line.
    assert_eq!(window_keeping(20, 0, 5, Some(12)), vec![0, 1, 2, 3, 12]);
    // Above it: the first, and the end still reached.
    assert_eq!(window_keeping(20, 15, 5, Some(0)), vec![0, 16, 17, 18, 19]);
    // None, or everything fits.
    assert_eq!(window_keeping(4, 0, 5, Some(3)), vec![0, 1, 2, 3]);
    assert_eq!(window_keeping(20, 3, 5, None), vec![3, 4, 5, 6, 7]);
}

/// A person's role as their rows say it: Host (an online room's owner),
/// Co-host, Admin; then, quietly, that they choose the room's songs now
/// (an online room with one picker at a time; practice has one person
/// and says nothing).
fn person_role(s: &ddi_room::RoomState, member: &ddi_room::Member) -> Option<String> {
    let role: Option<String> = match (member.role, member.admin) {
        (Role::Owner, _) if s.source == ddi_room::ContentSource::Shared => {
            Some(m::role_host().into())
        }
        (Role::CoHost, _) => Some(m::role_co_host().into()),
        (Role::Member, true) => Some(m::role_admin().into()),
        _ => None,
    };
    let picking: Option<String> = picks_shown(s, member).then(|| m::role_picking().into());
    let words: Vec<String> = [role, picking].into_iter().flatten().collect();
    (!words.is_empty()).then(|| words.join(" · "))
}

/// Whether `member`'s rows mark them as the one choosing the room's
/// songs: the picker of an online room.
fn picks_shown(s: &ddi_room::RoomState, member: &ddi_room::Member) -> bool {
    s.source == ddi_room::ContentSource::Shared && s.picker() == Some(member.id)
}

/// A person's status on the players panel's line: their connection's
/// trouble first, then their seat (watching, next match), why their song
/// did not load, else `state` (ready, loading, not ready, playing, sitting
/// out).
fn person_status(
    member: &ddi_room::Member,
    part: Option<&Participation>,
    upcoming: Option<&ddi_room::Stage>,
    state: &str,
) -> (String, Tone) {
    if let Some(word) = conn_word(member.connection) {
        let tone = match member.connection {
            Connection::Joining => Tone::Plain,
            _ => Tone::Away,
        };
        return (word, tone);
    }
    if let Some(word) = seat_word(part) {
        return (word, Tone::Plain);
    }
    match part {
        Some(Participation::LoadFailed { .. }) => {
            let why = upcoming.and_then(|st| match &st.prep(member.id).loading {
                ddi_room::Loading::Failed { code } => LoadProblem::from_code(code),
                _ => None,
            });
            let text = match why {
                Some(LoadProblem::Download) => m::state_failed_download(),
                Some(LoadProblem::Unreadable) => m::state_failed_unreadable(),
                Some(LoadProblem::Changed) => m::state_failed_changed(),
                Some(LoadProblem::PlanMismatch) => m::state_failed_plan(),
                _ => m::state_load_failed(),
            };
            (text.into(), Tone::Failed)
        }
        Some(Participation::ReadyLoaded { .. } | Participation::Committed { .. }) => {
            (state.to_string(), Tone::Good)
        }
        // (Nothing to be ready for until a pick gathers readiness, the
        // lobby, between stages: `person_state` says nothing there.)
        _ => (state.to_string(), Tone::Plain),
    }
}

/// A person's row: under the name their role (Host, Co-host, Admin;
/// Watching, Next match) and their points in a match; at the right their
/// connection when not connected, else `state`.
fn person_row(
    s: &ddi_room::RoomState,
    member: &ddi_room::Member,
    part: Option<&Participation>,
    points: Option<String>,
    state: String,
) -> (String, String) {
    let role = person_role(s, member);
    // Joined during the match (round AA): the stage they played from.
    let joined = s.current.as_ref().and_then(|m| {
        let seat = m.roster.get(&member.id)?;
        let first = m.stages.first()?.id;
        (seat.first_stage != first)
            .then(|| m.stage_index(seat.first_stage))
            .flatten()
            .map(|i| String::from(m::joined_at(i + 1)))
    });
    let sub = [role, seat_word(part), joined, points]
        .into_iter()
        .flatten()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    (sub, conn_word(member.connection).unwrap_or(state))
}

/// What a person's connection says where they are not connected.
pub(crate) fn conn_word(c: Connection) -> Option<String> {
    Some(
        match c {
            Connection::Joining => m::conn_joining(),
            Connection::Reconnecting => m::conn_reconnecting(),
            Connection::Disconnected => m::conn_away(),
            Connection::Connected => return None,
        }
        .into(),
    )
}

/// What a person's seat says where they are not in the match's play:
/// waiting for the next match, or watching.
fn seat_word(part: Option<&Participation>) -> Option<String> {
    match part {
        Some(Participation::WaitingNextMatch) => Some(m::next_match().into()),
        Some(Participation::WaitingNextStage) => Some(m::next_song().into()),
        Some(Participation::WaitingPlace) => Some(m::place_full().into()),
        Some(Participation::Watching) => Some(m::watching().into()),
        _ => None,
    }
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
            // Its MSD where shown, once this device has it (rated first).
            if let Some(p) = picked {
                crate::state::ratings::want_first(
                    &p.song.id,
                    p.chart.index,
                    p.chart.digest,
                    &p.chart.style,
                    p.chart.version.is_none(),
                );
            }
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
    // An online room's links first, the invite link at the top
    // (feedback round AA: a host shared the page's own address, which
    // lets nobody in).
    if r.is_online() {
        if r.invite_link().is_some() {
            items.push(("copy".to_string(), m::copy_invite().into()));
        }
        // The watch-only link is kept but hidden (the maintainer,
        // 2026-11-13): its Copy only under the hidden option
        // (`Settings::multiplayer_watch_link`).
        if r.watch_invite_link().is_some()
            && crate::state::settings::Settings::cached().multiplayer_watch_link
        {
            items.push((MORE_COPY_WATCH.to_string(), m::copy_watch_invite().into()));
        }
        if r.audience_invite_link().is_some() {
            items.push((MORE_COPY_AUDIENCE.to_string(), m::copy_audience_invite().into()));
        }
    }
    // Rules: a phone's here, a wide screen's a footer link. Players
    // everywhere (a wide screen's button went: the panel beside the song,
    // L, and here).
    if compact {
        items.push(("rules".to_string(), m::rules().into()));
    }
    items.push(("players".to_string(), m::players().into()));
    // An operator who follows takes the pick over (a controller's way to
    // it on every layout; a phone has no room for its button).
    if r.may_take_pick() {
        items.push((MORE_TAKE_PICK.to_string(), m::take_pick().into()));
    }
    // Play or Watch, the player's own, outside a stage they are in.
    if r.may_choose_intent() {
        items.push((MORE_INTENT.to_string(), intent_label(r).into()));
    }
    let running = r.state().phase.kind().match_running();
    // An open-ended match's last song and its end (round AA).
    if let Some((id, label)) = last_song_item(r) {
        items.push((id.to_string(), label.into()));
    }
    if r.may_end_match() {
        items.push((MORE_END_MATCH.to_string(), m::end_match().into()));
    }
    if running && r.may(Capability::StopMatch) {
        items.push(("stop".to_string(), m::stop_match().into()));
    }
    if r.is_online() {
        if r.link_state() == LinkState::SignInAgain {
            items.push(("signin".to_string(), m::sign_in_again().into()));
        }
        if r.link_state() == LinkState::SignInToEnter {
            items.push(("signin".to_string(), m::sign_in().into()));
        }
        if r.link_state() == LinkState::UpdateRequired && r.on_web() {
            items.push((MORE_RELOAD.to_string(), m::update_reload_button().into()));
        }
        if r.link_state() == LinkState::OpenedElsewhere {
            items.push((MORE_USE_HERE.to_string(), m::use_this_device().into()));
        }
        if r.may_replace_invite() {
            items.push((MORE_REPLACE_INVITE.to_string(), m::replace_invite().into()));
        }
        if r.state()
            .upcoming_stage()
            .is_some_and(|up| r.may_retry_sharing(up))
        {
            items.push(("retry".to_string(), m::retry_sharing().into()));
        }
        if retry_shown(r) {
            items.push((RETRY_LOAD.to_string(), m::retry_download().into()));
        }
    }
    // Never in the room (feedback round AA): its way out is the home.
    let never_in = matches!(
        r.link_state(),
        LinkState::SignInToEnter | LinkState::NotAMember
    );
    let leave = if never_in {
        m::multiplayer_home()
    } else {
        m::leave()
    };
    items.push(("leave".to_string(), leave.into()));
    items
}

/// More's End the match now (an open-ended match's, between stages,
/// round AA), asked once more ([`ask_end_match`]).
pub const MORE_END_MATCH: &str = "endmatch";

/// More's last-song items (round AA): the stage picked and not started
/// made the open-ended match's last song or not; before a pick, the
/// next pick made it.
pub const MORE_MAKE_LAST: &str = "makelast";
pub const MORE_NOT_LAST: &str = "notlast";
pub const MORE_PICK_LAST: &str = "picklast";

/// More's last-song item for `r` now, if any: the picked stage made the
/// last song or not (an operator's, until its start); before a pick, the
/// next pick made it or not (the picker's).
fn last_song_item(r: &RoomClient) -> Option<(&'static str, UiText)> {
    match r.last_song_choice()? {
        LastSongChoice::Picked { last: false, .. } => Some((MORE_MAKE_LAST, m::make_last())),
        LastSongChoice::Picked { last: true, .. } => Some((MORE_NOT_LAST, m::not_last())),
        LastSongChoice::NextPick { last: false } => Some((MORE_PICK_LAST, m::pick_last_next())),
        LastSongChoice::NextPick { last: true } => Some((MORE_NOT_LAST, m::not_last())),
    }
}

/// The question before an open-ended match ends now ([`MORE_END_MATCH`]).
pub fn ask_end_match(env: &Env, room: &SharedRoom, cx: &mut Cx) {
    let played = room.borrow().stages_played();
    let room = room.clone();
    cx.overlay(
        super::dialog::ConfirmDialog::new(
            env.clone(),
            &m::end_match(),
            &m::end_match_body(played),
            &m::end_match(),
            Rc::new(move |_| room.borrow_mut().end_match()),
        )
        .named(END_MATCH_QUESTION),
        true,
    );
}

/// The layer of [`ask_end_match`]'s question.
pub const END_MATCH_QUESTION: &str = "room-end-match";

/// More's Copy watch-only link (an operator's, from a server that makes
/// one).
const MORE_COPY_WATCH: &str = "copywatch";

/// More's Reload (the web, where the room's authority is newer than the
/// page: `ddi_room::UPDATE_REQUIRED`).
pub const MORE_RELOAD: &str = "reload";

/// Retry the stage's song's download (round P1b): More's item and the
/// button beside the status line of the room's song select, once the
/// download's own retries are spent, while the stage's window is open.
pub const RETRY_LOAD: &str = "Room.RetryLoad";

/// Whether [`RETRY_LOAD`] is offered: the upcoming stage's download
/// failed after its own retries, its window open.
pub fn retry_shown(r: &RoomClient) -> bool {
    r.state()
        .upcoming_stage()
        .is_some_and(|up| r.may_retry_load(up))
}

/// [`RETRY_LOAD`] pressed.
pub fn retry_load(room: &SharedRoom) {
    let up = room.borrow().state().upcoming_stage();
    if let Some(up) = up {
        room.borrow_mut().retry_load(up);
    }
}

/// More's Use this device (round P1b): the room was opened on another of
/// the person's devices and this one stopped; it joins again.
pub const MORE_USE_HERE: &str = "usehere";

/// The notice that the room was opened on another of the person's devices
/// (round P1b: the server keeps four sockets a person, and closed this
/// device's for a newer one; it no longer connects again by itself), with
/// Use this device, which joins again (and so stops another device the
/// same way). The layer's name, for tests.
pub const OPENED_ELSEWHERE: &str = "room-opened-elsewhere";

/// The notice on a room's page opened by someone never in the room
/// (feedback round AA): not signed in ([`LinkState::SignInToEnter`]), or
/// signed in and no member ([`LinkState::NotAMember`]).
pub const NEVER_IN: &str = "room-never-in";

/// The notice a link's end asks the person to act on, once each time
/// (`RoomClient::take_opened_elsewhere`): the room opened on another
/// device (round P1b), or a room's page opened by someone never in it
/// (feedback round AA): sign in, or go to the multiplayer home.
pub fn tell_opened_elsewhere(env: &Env, room: &SharedRoom, cx: &mut Cx) {
    // More's End the match now (round AA), asked over the screen.
    if room.borrow().take_end_question() {
        ask_end_match(env, room, cx);
        return;
    }
    let state = room.borrow().link_state();
    let home: super::dialog::ConfirmFn = Rc::new(|cx| {
        cx.broadcast(
            crate::Message::new(LEAVE_CONFIRMED)
                .with("Action", crate::message::Param::Str(LeaveBy::More.id().into())),
        )
    });
    match state {
        LinkState::SignInToEnter => {
            let room = room.clone();
            cx.overlay(
                super::dialog::ConfirmDialog::new(
                    env.clone(),
                    &m::link_sign_in_to_enter(),
                    &m::sign_in_to_enter_body(),
                    &m::sign_in(),
                    Rc::new(move |_| room.borrow_mut().sign_in_again()),
                )
                .with_answer(&m::multiplayer_home(), home)
                .with_no(&m::opened_elsewhere_not_now())
                .named(NEVER_IN),
                true,
            );
            return;
        }
        LinkState::NotAMember => {
            cx.overlay(
                super::dialog::ConfirmDialog::new(
                    env.clone(),
                    &m::link_not_a_member(),
                    &m::not_a_member_body(),
                    &m::multiplayer_home(),
                    home,
                )
                .with_no(&common::close())
                .named(NEVER_IN),
                true,
            );
            return;
        }
        _ => {}
    }
    let room = room.clone();
    cx.overlay(
        super::dialog::ConfirmDialog::new(
            env.clone(),
            &m::link_opened_elsewhere(),
            &m::opened_elsewhere_body(),
            &m::use_this_device(),
            std::rc::Rc::new(move |_| room.borrow_mut().use_this_device()),
        )
        .with_no(&m::opened_elsewhere_not_now())
        .named(OPENED_ELSEWHERE),
        true,
    );
}

/// More's Replace invite link (an operator's, round B1), asked once.
pub const MORE_REPLACE_INVITE: &str = "rotate";

/// The question before the invite is replaced: it cannot be undone, and
/// the watch-only and audience links change with it.
pub fn ask_replace_invite(env: &Env, room: &SharedRoom, cx: &mut Cx) {
    let room = room.clone();
    cx.overlay(
        super::dialog::ConfirmDialog::new(
            env.clone(),
            &m::replace_invite_title(),
            &m::replace_invite_body(),
            &m::replace_invite(),
            std::rc::Rc::new(move |_| room.borrow_mut().rotate_invite()),
        ),
        true,
    );
}

/// More's Copy audience link (an operator's, where the room has an
/// audience).
const MORE_COPY_AUDIENCE: &str = "copyaudience";

/// More's item that changes between Play and Watch.
const MORE_INTENT: &str = "intent";

/// What the control that changes the local player between Play and Watch
/// says: what a press does (Watch only, while they play; Play, while
/// they watch).
pub fn intent_label(r: &RoomClient) -> UiText {
    if r.watching() {
        m::intent_play()
    } else {
        m::intent_watch()
    }
}

/// The local player changes between Play and Watch (More's item, the
/// Players pop-up's button): the room is told; a refusal shows on the
/// status line.
pub fn toggle_intent(room: &SharedRoom) {
    let next = if room.borrow().watching() {
        ddi_room::Intent::Play
    } else {
        ddi_room::Intent::Watch
    };
    let _ = room.borrow_mut().set_intent(next);
}

/// The name of the room's More menu's pick message.
pub const MORE: &str = "Room.More";

/// Opens the room's More menu.
pub fn open_more(env: &Env, room: &SharedRoom, compact: bool, cx: &mut Cx) {
    open_more_with(env, room, compact, Vec::new(), cx);
}

/// [`open_more`] with the screen's own `first` (the room's song select's
/// heart for the picker's song), picked as this menu's ([`MORE`]).
pub fn open_more_with(
    env: &Env,
    room: &SharedRoom,
    compact: bool,
    first: Vec<(String, String)>,
    cx: &mut Cx,
) {
    // The invite read again: another operator may have replaced it.
    room.borrow_mut().refresh_invite();
    let mut items = first;
    items.extend(more_items(&room.borrow(), compact));
    super::action_menu(env, &common::more(), items, MORE, cx);
}

/// What a pick of More leaves to the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoreAction {
    Rules,
    Players,
    /// Replace invite link: asked once first ([`ask_replace_invite`]).
    ReplaceInvite,
    /// Leave room: asked first where it costs something ([`leave`]).
    Leave,
    /// Done here (or nothing): the screen redraws.
    Done,
}

/// How the player set about leaving the room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaveBy {
    /// Back on the room's song select (a key, the header's, a
    /// controller's, the browser's).
    Back,
    /// More's Leave room.
    More,
    /// The window's close button or the app's Quit
    /// ([`crate::ui::CLOSE_REQUESTED`]): the game quits once the room is
    /// told.
    Quit,
}

impl LeaveBy {
    fn id(self) -> &'static str {
        match self {
            LeaveBy::Back => "back",
            LeaveBy::More => "more",
            LeaveBy::Quit => "quit",
        }
    }
}

/// Broadcast by the question when the player chose Leave; its `Action`
/// says how they set about it ([`left_by`]).
pub const LEAVE_CONFIRMED: &str = "Room.LeaveConfirmed";

/// Broadcast as the room hands the player a stage's prompt: the leave
/// question closes ([`ask_leave`]).
pub const STAGE_OPENS: &str = "Room.StageOpens";

/// What the due notice's button does: goes to the stage's Ready screen
/// (a shell closes what is over the room: the web navigates to the
/// room's song select, desktop's room screen asks the screens over it to
/// leave, [`Cx::surface`]), where the prompt opens by itself.
pub type DueGo = Rc<dyn Fn(&mut Cx)>;

/// Broadcast by the notice's button where the room's screen is under
/// the screens it was opened over (desktop): that screen asks them to
/// leave ([`Cx::surface`]).
pub const DUE_GO: &str = "Room.DueGo";

/// The notice's layer among the layers.
pub const DUE_NOTICE: &str = "room-stage-due";

/// The room's stage is due while the player is on a screen opened over
/// the room (Options, Calibrate, the announcer studio; the maintainer,
/// 2026-11-15: "Tell them") or still on the last stage's results ("Tell
/// them the same way"): a notice over that screen saying so, with
/// the room's status line (its countdown) and a button that goes to the
/// stage's Ready screen. Nothing is taken out of the player's hands:
/// Stay here keeps the screen (told once a stage, `RoomClient::stay`),
/// and the notice goes by itself only when the stage is no longer theirs
/// to go to. It never opens over a control being bound
/// ([`crate::Screen::holds_notices`]): it waits until the binding is
/// saved or canceled.
pub struct DueDialog {
    env: Env,
    room: SharedRoom,
    go: DueGo,
    due: crate::state::room::StageDue,
    telling: Rc<std::cell::Cell<Option<StageId>>>,
    lines: Vec<String>,
}

impl DueDialog {
    /// The notice, where a stage is due for the player (`RoomClient::
    /// stage_due`) and no notice tells it already.
    pub fn due(env: &Env, room: &SharedRoom, go: DueGo) -> Option<DueDialog> {
        let (due, telling) = {
            let r = room.try_borrow().ok()?;
            (r.stage_due()?, r.telling_due())
        };
        if telling.get() == Some(due.stage) {
            return None;
        }
        telling.set(Some(due.stage));
        publish_dialog(env, |v| v.due_open = Some((due.number, due.watching)));
        Some(DueDialog {
            env: env.clone(),
            room: room.clone(),
            go,
            due,
            telling,
            lines: Vec::new(),
        })
    }

    /// Opens it over the screens from a room screen's own call
    /// (desktop's room under Options); whether it did.
    pub fn tell(env: &Env, room: &SharedRoom, go: DueGo, cx: &mut Cx) -> bool {
        match DueDialog::due(env, room, go) {
            Some(d) => {
                cx.overlay_current(d);
                true
            }
            None => false,
        }
    }

    /// What it says now: the sentence, then the room's status line.
    fn words(&self, width: f32) -> Vec<String> {
        let Ok(r) = self.room.try_borrow() else {
            return self.lines.clone();
        };
        let first = if self.due.watching {
            m::due_watch()
        } else {
            m::due_play()
        };
        let mut lines = super::dialog::wrap_lines(&first, width, 14.0);
        lines.extend(super::dialog::wrap_lines(&status_line(&r), width, 14.0));
        lines
    }

    fn panel(&self, screen: [f32; 2]) -> f32 {
        520.0_f32.min(screen[0] - 32.0)
    }
}

impl crate::screen::Screen for DueDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        DUE_NOTICE
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let w = self.panel(screen);
        self.lines = self.words(w - 48.0);
        let h = 160.0 + self.lines.len() as f32 * 18.0;
        let title = m::due_title(self.due.number);
        let (mut c, [x, y]) = super::dialog::shell(&self.env, screen, [w, h], &title);
        super::dialog::dialog_body(&self.env, &mut c, &self.lines, [x, y], w, |i| {
            if i + 1 == self.lines.len() { "Secondary" } else { "Muted" }
        });
        let go: String = if self.due.watching {
            m::due_go_watch().into()
        } else {
            m::due_go().into()
        };
        let buttons = [
            ("Stay", m::due_stay().into(), "Button"),
            ("Go", go, "Button"),
        ];
        c.extend(super::dialog::button_rows(&self.env, &buttons, 2, [x, y, w, h], 180.0));
        self.env.state.borrow_mut().page_items = vec!["Go".into(), "Stay".into()];
        super::dialog::fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<crate::nav::Focusable> {
        // Go first: what the notice is for.
        vec![crate::nav::Focusable::new("Go"), crate::nav::Focusable::new("Stay")]
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        // No longer theirs to go to (the stage over, its prompt opened
        // elsewhere, the room left): the notice goes; the screen stays.
        let due = self.room.try_borrow().ok().map(|r| r.stage_due());
        match due {
            Some(Some(due)) if due.stage == self.due.stage => {
                let vp = cx.viewport().screen;
                let (screen, _) = super::narrow_fit(&self.env, vp);
                let now = self.words(self.panel(screen) - 48.0);
                if due != self.due || now != self.lines {
                    self.due = due;
                    publish_dialog(&self.env, |v| v.due_open = Some((due.number, due.watching)));
                    let fresh = self.build(cx);
                    super::refill(root, fresh);
                }
            }
            Some(_) => cx.close(true),
            None => {}
        }
    }

    fn input(
        &mut self,
        input: &crate::screen::Input,
        _root: &mut Actor,
        cx: &mut Cx,
    ) -> crate::screen::Handled {
        use crate::screen::{Handled, Input};
        match input {
            Input::Activate { name } if name == "Go" => {
                cx.close(false);
                (self.go)(cx);
                Handled::Yes
            }
            Input::Activate { name } if name == "Stay" => {
                if let Ok(mut r) = self.room.try_borrow_mut() {
                    r.stay(self.due.stage);
                }
                cx.close(true);
                Handled::Yes
            }
            // Escape, Back, a press beside it: Stay.
            Input::Menu {
                action: crate::nav::MenuAction::Back,
                ..
            }
            | Input::Outside => {
                if let Ok(mut r) = self.room.try_borrow_mut() {
                    r.stay(self.due.stage);
                }
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

impl Drop for DueDialog {
    fn drop(&mut self) {
        if self.telling.get() == Some(self.due.stage) {
            self.telling.set(None);
        }
        publish_dialog(&self.env, |v| v.due_open = None);
    }
}

/// What leaving costs, in a sentence or two: what the room will do.
pub fn leave_text(cost: &crate::state::room::LeaveCost) -> UiText {
    use crate::state::room::LeaveCost;
    match cost {
        LeaveCost::HostPass {
            to,
            match_running: false,
        } => m::leave_host_pass(to.as_str()),
        LeaveCost::HostPass {
            to,
            match_running: true,
        } => m::leave_host_pass_match(to.as_str()),
        LeaveCost::HostClose {
            match_running: false,
        } => m::leave_host_close(),
        LeaveCost::HostClose {
            match_running: true,
        } => m::leave_host_close_match(),
        LeaveCost::HostAlone => m::leave_host_alone(),
        LeaveCost::Playing => m::leave_playing(),
        LeaveCost::Picking => m::leave_picking(),
        LeaveCost::CoHost => m::leave_co_host(),
    }
}

/// The question's words: what leaving costs, after the sentence saying
/// the stage was left where it was (each on lines of its own).
pub fn leave_question(cost: &crate::state::room::LeaveCost, left_stage: bool) -> UiText {
    let cost = leave_text(cost);
    if left_stage {
        UiText::data(format!("{}\n{}", m::leave_forfeited(), cost))
    } else {
        cost
    }
}

/// The player sets about leaving the room: where it costs something
/// (`RoomClient::leave_cost`) they are asked first ("Leave the room?",
/// what it does, Stay chosen first; Escape and Back there are Stay),
/// and [`LEAVE_CONFIRMED`] is broadcast if they say Leave; whether they
/// were asked. Where it costs nothing the screen leaves at once
/// ([`leave_now`]). Asked once: the question open already is not asked
/// again.
pub fn ask_leave(env: &Env, room: &SharedRoom, by: LeaveBy, cx: &mut Cx) -> bool {
    ask_leave_after(env, room, by, false, cx)
}

/// [`ask_leave`], asked over the results of a stage the player has just
/// left by setting about leaving the room during its play (`left_stage`:
/// the window's close, the browser's Back: the stage is forfeited first,
/// the maintainer, 2026-11-12): the question then says so before what
/// leaving the room does. Stay keeps the room; the forfeit is not
/// undone.
pub fn ask_leave_after(
    env: &Env,
    room: &SharedRoom,
    by: LeaveBy,
    left_stage: bool,
    cx: &mut Cx,
) -> bool {
    let Some(cost) = room.borrow().leave_cost() else {
        return false;
    };
    let asking = room.borrow().asking_leave();
    if asking.replace(true) {
        return true;
    }
    let id = by.id();
    let on_yes: super::dialog::ConfirmFn = Rc::new(move |cx| {
        cx.broadcast(
            crate::Message::new(LEAVE_CONFIRMED)
                .with("Action", crate::message::Param::Str(id.into())),
        )
    });
    publish_dialog(env, |v| v.leave_open = true);
    cx.overlay(
        LeaveDialog(
            super::dialog::ConfirmDialog::new(
                env.clone(),
                &m::leave_title(),
                &leave_question(&cost, left_stage),
                &m::leave_confirm(),
                on_yes,
            )
            .with_no(&m::leave_stay())
            .named(crate::ui::LEAVE_QUESTION),
            env.clone(),
            asking,
        ),
        true,
    );
    true
}

/// The question: the room's "asking" flag is its own while it is open
/// (cleared when it goes, however it goes), and the debug state says so.
struct LeaveDialog(super::dialog::ConfirmDialog, Env, Rc<std::cell::Cell<bool>>);

impl crate::screen::Screen for LeaveDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        self.0.presentation()
    }
    fn name(&self) -> &str {
        self.0.name()
    }
    fn build(&mut self, cx: &mut Cx) -> Actor {
        // Where the shells' debug views find its buttons.
        self.1.state.borrow_mut().page_items = vec!["Cancel".into(), "Confirm".into()];
        self.0.build(cx)
    }
    fn focusables(&self) -> Vec<crate::nav::Focusable> {
        self.0.focusables()
    }
    fn input(
        &mut self,
        input: &crate::screen::Input,
        root: &mut Actor,
        cx: &mut Cx,
    ) -> crate::screen::Handled {
        self.0.input(input, root, cx)
    }
    fn message(&mut self, message: &crate::Message, _root: &mut Actor, cx: &mut Cx) {
        if message.name == STAGE_OPENS {
            cx.close(true);
        }
    }
}

impl Drop for LeaveDialog {
    fn drop(&mut self) {
        self.2.set(false);
        publish_dialog(&self.1, |v| v.leave_open = false);
    }
}

/// How the player set about the leaving a [`LEAVE_CONFIRMED`] message
/// confirms.
pub fn left_by(msg: &crate::Message) -> Option<LeaveBy> {
    match super::menu_pick(msg, LEAVE_CONFIRMED)? {
        "back" => Some(LeaveBy::Back),
        "more" => Some(LeaveBy::More),
        "quit" => Some(LeaveBy::Quit),
        _ => None,
    }
}

/// Leaves the room, without asking (it was asked, or costs nothing): an
/// online room is told (`Command::Leave`) and forgotten; then the
/// multiplayer home. CPU practice is never asked about: between matches
/// it is kept for the session on Back, as it was, and forgotten by
/// More's Leave room; with a match running, any way out forgets the
/// practice and its unfinished match silently (the maintainer,
/// 2026-11-12), so that coming back starts a fresh room.
pub fn leave_now(room: &SharedRoom, shell: &RoomShell, by: LeaveBy, cx: &mut Cx) {
    let keep = by == LeaveBy::Back && {
        let r = room.borrow();
        !r.is_online() && !r.state().phase.kind().match_running()
    };
    room.borrow_mut().set_rules_seen(false);
    if keep {
        shell(RoomCall::Leave, cx);
    } else {
        room.borrow_mut().leave();
        shell(RoomCall::Discard, cx);
    }
}

/// How long a quitting game waits for the room's answer to its leaving
/// before it goes, seconds.
pub const QUIT_WAIT: f64 = 0.6;

/// Leaving the room to quit the game: the room told, and when the game
/// quits on the UI's clock (`now`): at once, or for an online room once
/// it has answered, [`QUIT_WAIT`] at most ([`quit_due`]).
pub fn quit_leaving(room: &SharedRoom, now: f64) -> f64 {
    let told = {
        let r = room.borrow();
        r.is_online() && r.link_state() == ddi_room::LinkState::Connected
    };
    room.borrow_mut().leave();
    now + if told { QUIT_WAIT } else { 0.0 }
}

/// Whether a game quitting at `at` ([`quit_leaving`]) quits now: its time
/// came, or the room answered.
pub fn quit_due(room: &SharedRoom, at: Option<f64>, now: f64) -> bool {
    at.is_some_and(|at| now >= at || room.borrow().link_state().ended())
}

/// Does what More's `id` asks.
pub fn more_picked(id: &str, room: &SharedRoom, cx: &mut Cx) -> MoreAction {
    match id {
        "rules" => return MoreAction::Rules,
        "players" => return MoreAction::Players,
        "stop" => {
            let _ = room.borrow_mut().stop_match();
        }
        MORE_TAKE_PICK => {
            let _ = room.borrow_mut().take_pick();
        }
        MORE_INTENT => toggle_intent(room),
        "copy" => {
            let link = room.borrow().invite_link();
            if let Some(link) = link {
                cx.request(crate::UiRequest::CopyText(link));
                room.borrow_mut()
                    .set_link_notice(Some(LinkNotice::InviteCopied));
            }
        }
        MORE_COPY_WATCH => {
            let link = room.borrow().watch_invite_link();
            if let Some(link) = link {
                cx.request(crate::UiRequest::CopyText(link));
                room.borrow_mut()
                    .set_link_notice(Some(LinkNotice::WatchInviteCopied));
            }
        }
        MORE_COPY_AUDIENCE => {
            let link = room.borrow().audience_invite_link();
            if let Some(link) = link {
                cx.request(crate::UiRequest::CopyText(link));
                room.borrow_mut()
                    .set_link_notice(Some(LinkNotice::AudienceInviteCopied));
            }
        }
        // Asked once: it cannot be undone.
        MORE_REPLACE_INVITE => return MoreAction::ReplaceInvite,
        "signin" => room.borrow_mut().sign_in_again(),
        MORE_MAKE_LAST | MORE_NOT_LAST | MORE_PICK_LAST => {
            room.borrow_mut().toggle_last_song();
        }
        // Asked once more over the room's song select: its question comes
        // by the room's own notice (`tell_opened_elsewhere`).
        MORE_END_MATCH => room.borrow().ask_end_match(),
        MORE_USE_HERE => room.borrow_mut().use_this_device(),
        RETRY_LOAD => retry_load(room),
        MORE_RELOAD => cx.request(crate::UiRequest::Reload),
        "retry" => {
            let up = room.borrow().state().upcoming_stage();
            if let Some(up) = up {
                room.borrow_mut().retry_sharing(up);
            }
        }
        "leave" => return MoreAction::Leave,
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

/// The panel's rows for the room now, its `players` ([`player_rows`]):
/// the picks, then the players.
pub fn panel_rows(r: &RoomClient, players: Vec<PlayerRow>) -> Vec<PanelRow> {
    let mut rows = pick_panel_rows(r);
    rows.extend(players_section(r, players));
    rows
}

/// [`panel_rows`] with the players before the picks: the Players
/// pop-up's order (its rows are what the host acts on).
pub fn players_first_rows(r: &RoomClient) -> Vec<PanelRow> {
    let mut rows = players_section(r, player_rows(r));
    rows.extend(pick_panel_rows(r));
    rows
}

/// The players under their heading (how many of the room's cap).
fn players_section(r: &RoomClient, players: Vec<PlayerRow>) -> Vec<PanelRow> {
    let cap = r.state().policy.max_competitors;
    let mut rows = vec![PanelRow::Heading(
        m::players().into(),
        m::players_count(players_counted(&players), cap).into(),
    )];
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
    // A mark's room for each that shows: above where the rows begin past
    // the first, below where they end before the last (`first` short of
    // `last_first`, or everything not fitting from the top).
    let marks = usize::from(first > 0 || last_first > 0)
        + usize::from(first > 0 && first < last_first);
    let room = height - 24.0 * marks as f32;
    let n = panel_fit(rows, first, room);
    let mut ry = y + if first > 0 { 24.0 } else { 0.0 };
    if first > 0 {
        children.push(more_mark(
            env,
            "Panel.Up",
            &m::panel_more_above(first),
            [w, 22.0],
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
                // The host's pop-up: a row to tap, on a faint plate.
                if let (true, Some(id)) = (p.manage, p.id) {
                    let mut plate = look::quad_at(
                        &person_row_name(id),
                        [w + 12.0, h - 4.0],
                        look::with_alpha(env.color("Text"), 0.05),
                        [x + w / 2.0, ry + h / 2.0 - 2.0],
                    );
                    look::call(&mut plate, "cornerradius", &[6.0]);
                    children.push(plate);
                }
                let color = if p.me { "Text" } else { "Secondary" };
                let state_w = look::mono_width(&p.state, 13.0).min(w * 0.45);
                let (parts, _) = name_and_badge(
                    env,
                    &format!("Panel{k}.Name"),
                    &p.name,
                    p.badge,
                    (15.0, 600, color),
                    [x, ry + 13.0],
                    (w - state_w - 12.0).max(40.0),
                );
                children.extend(parts);
                if let Some(level) = p.level {
                    // A CPU: its level's stars, then its personality.
                    level_line(
                        env,
                        &mut children,
                        &format!("Panel{k}"),
                        (level, &p.sub),
                        [x, ry + 31.0],
                        w,
                        false,
                    );
                } else if !p.sub.is_empty() {
                    let mut sub = env.text(&format!("Panel{k}.Sub"), &p.sub, 12.0, 400, "Muted");
                    look::ellipsized(&mut sub, (w - state_w - 12.0).max(40.0));
                    children.push(left(sub, x, ry + 31.0));
                }
                let color = if p.ready && !p.cpu { "Accent" } else { "Muted" };
                let mut state = env.text(&format!("Panel{k}.State"), &p.state, 13.0, 600, color);
                look::ellipsized(&mut state, w * 0.45);
                children.push(right(state, x + w, ry + 13.0));
                // Their attack policy, quietly, under the state.
                if !p.policy.is_empty() {
                    let mut policy =
                        env.text(&format!("Panel{k}.Policy"), &p.policy, 12.0, 400, "Faint");
                    look::ellipsized(&mut policy, w * 0.3);
                    children.push(right(policy, x + w, ry + 31.0));
                }
            }
        }
        ry += h;
    }
    let below = rows.len() - first - n;
    if below > 0 {
        children.push(more_mark(
            env,
            "Panel.Down",
            &m::panel_more_below(below),
            [w, 22.0],
            [x + w / 2.0, y + height - 11.0],
        ));
    }
    (super::frame("Panel", children), first)
}

/// The match's picks alone, under their heading: what the list's place
/// holds for a follower whose players are beside the song
/// ([`players_panel`]).
pub fn pick_panel_rows(r: &RoomClient) -> Vec<PanelRow> {
    let picks = pick_rows(r);
    let picked = picks.iter().filter(|p| p.title.is_some()).count();
    let mut rows = vec![PanelRow::Heading(
        m::picks_heading().into(),
        // An open-ended match's picks have no count (revision 15).
        m::picks_count(
            picked,
            if r.state().current.as_ref().is_some_and(|m| m.rules.open_ended) {
                0
            } else {
                picks.len()
            },
        )
        .into(),
    )];
    rows.extend(picks.into_iter().map(PanelRow::Pick));
    rows
}

/// The players panel's heading's height, and a row's at most and at least.
const PLAYERS_HEAD: f32 = 28.0;
const PLAYERS_ROW: f32 = 40.0;
const PLAYERS_ROW_MIN: f32 = 30.0;

/// What opens the full list of players from the panel: its line of the
/// rows that do not fit, and a phone's strip.
pub const PLAYERS_MORE: &str = "Players.More";
pub const PLAYERS_STRIP: &str = "Players.Strip";

/// The players in the panel's order: by their place in the match once it
/// has places (those without one after), otherwise as the room lists them;
/// in a large room the local player's line first.
pub fn panel_order(mut rows: Vec<PlayerRow>) -> Vec<PlayerRow> {
    if rows.len() > ROOM_OF_NINE {
        // A large room: the local player's line first, then by place.
        rows.sort_by_key(|p| (!p.me, p.place.unwrap_or(u16::MAX)));
    } else {
        rows.sort_by_key(|p| p.place.unwrap_or(u16::MAX));
    }
    rows
}

/// A status' color by its tone.
fn tone_color(tone: Tone) -> &'static str {
    match tone {
        Tone::Good => "Accent",
        Tone::Plain => "Muted",
        Tone::Away => "Faint",
        Tone::Failed => "Danger",
    }
}

/// The players panel (`RoomPlayers`), always shown on the room's song
/// select beside the song, the host's and the followers' alike: everyone
/// in the room on a line each (`Players<k>.*`): the place in the match
/// once a stage is confirmed, the name (You; a role; a CPU's stars and
/// personality), what they are doing in words, and the match points. In
/// `[x, y, w, h]`; rows that do not fit are counted on a last line
/// ([`PLAYERS_MORE`]) that opens the full list.
pub fn players_panel(env: &Env, r: &RoomClient, [x, y, w, h]: [f32; 4]) -> Actor {
    let rows = panel_order(player_rows(r));
    let cap = r.state().policy.max_competitors;
    let mut c = Vec::new();
    let title = m::players();
    let head = look::tracked(env.text("Players.Heading", &title, 11.0, 700, "Faint"), 120);
    let head_w = look::mono_width(&title, 11.0) + title.chars().count() as f32 * 11.0 * 0.12;
    c.push(left(head, x, y + 12.0));
    c.push(left(
        env.text(
            "Players.Count",
            &m::players_count(players_counted(&rows), cap),
            12.0,
            400,
            "Muted",
        ),
        x + head_w + 12.0,
        y + 12.0,
    ));
    let placed = rows.iter().any(|p| p.points.is_some());
    if placed {
        c.push(right(
            look::tracked(
                env.text("Players.Points", &m::panel_points(), 11.0, 700, "Faint"),
                120,
            ),
            x + w - 8.0,
            y + 12.0,
        ));
    }
    let avail = (h - PLAYERS_HEAD).max(PLAYERS_ROW_MIN);
    let row = (avail / rows.len().max(1) as f32).clamp(PLAYERS_ROW_MIN, PLAYERS_ROW);
    let fit = ((avail / row).floor() as usize).max(1);
    let shown = if rows.len() > fit {
        fit.saturating_sub(1)
    } else {
        rows.len()
    };
    let mut ry = y + PLAYERS_HEAD;
    for (k, p) in rows.iter().enumerate().take(shown) {
        player_line(env, &mut c, &format!("Players{k}"), p, [x, ry, w, row], placed);
        ry += row;
    }
    if shown < rows.len() {
        c.push(more_mark(
            env,
            PLAYERS_MORE,
            &m::panel_more_below(rows.len() - shown),
            [w, 24.0],
            [x + w / 2.0, ry + row / 2.0],
        ));
    }
    super::frame("RoomPlayers", c)
}

/// PROVISIONAL (`ready-card-v1`, the feedback round I): a players'
/// part's rows (the players panel's line, [`player_line`], at its
/// least), its columns' least width and the gap between them, its
/// heading's and its mark's heights.
pub const PART_ROW: f32 = 28.0;
pub const PART_COL_MIN: f32 = 260.0;
pub const PART_COL_GAP: f32 = 16.0;
const PART_HEAD: f32 = 22.0;
const PART_MARK: f32 = 22.0;

/// The names a players' part gives its actors: its heading, its count,
/// each row's (`{row}{k}`, `k` in the rows' order), its marks.
pub struct PartNames<'a> {
    pub heading: &'a str,
    pub count: &'a str,
    pub row: &'a str,
    pub down: &'a str,
    pub up: &'a str,
}

/// A players' part as laid out ([`players_part`]).
pub struct PlayersPart {
    pub actors: Vec<Actor>,
    pub height: f32,
    /// The first row of the window (clamped), the rows shown and a
    /// column's rows: what scrolling moves by.
    pub first: usize,
    pub shown: usize,
    pub per: usize,
}

/// `rows` in `[x, y, w, h]` (`h` at least four rows): a heading and a
/// count (`words`), then each the players panel's line ([`player_line`]),
/// in as many columns as they need and `w` holds (one for a few, several
/// for many, each at least [`PART_COL_MIN`] wide), down each column then
/// the next; past what the part holds a window of them from `first`, the
/// local player's line always in view ([`window_keeping`]), a mark under
/// it for the rows hidden. The stage's Ready card and the audience's page
/// (`ready-card-v1`).
pub fn players_part(
    env: &Env,
    rows: &[PlayerRow],
    names: &PartNames<'_>,
    (heading, count): (&str, &str),
    first: usize,
    [x, y, w, h]: [f32; 4],
) -> PlayersPart {
    let n = rows.len();
    let body = h.max(PART_HEAD + 4.0 * PART_ROW) - PART_HEAD;
    let per_all = ((body / PART_ROW).floor() as usize).max(1);
    let cols_most = (((w + PART_COL_GAP) / (PART_COL_MIN + PART_COL_GAP)).floor() as usize).max(1);
    let cols = n.div_ceil(per_all).clamp(1, cols_most);
    let marks = n > cols * per_all;
    let per = if marks {
        (((body - PART_MARK) / PART_ROW).floor() as usize).max(1)
    } else {
        per_all
    };
    let shown = if marks { cols * per } else { n };
    let first = first.min(n.saturating_sub(shown));
    let me_at = rows.iter().position(|p| p.me);
    let window = window_keeping(n, first, shown, me_at);
    let col_w = (w - PART_COL_GAP * (cols - 1) as f32) / cols as f32;
    let placed = rows.iter().any(|p| p.points.is_some());
    let mut out = Vec::new();
    out.push(left(
        look::tracked(env.text(names.heading, heading, 11.0, 600, "Faint"), 120),
        x,
        y + 8.0,
    ));
    if !count.is_empty() {
        out.push(right(env.text(names.count, count, 11.0, 400, "Muted"), x + w, y + 8.0));
    }
    let top = y + PART_HEAD;
    for (j, &k) in window.iter().enumerate() {
        let (col, at) = (j / per.max(1), j % per.max(1));
        let cx0 = x + col as f32 * (col_w + PART_COL_GAP);
        player_line(
            env,
            &mut out,
            &format!("{}{k}", names.row),
            &rows[k],
            [cx0, top + at as f32 * PART_ROW, col_w, PART_ROW],
            placed,
        );
    }
    let used = window.len().min(per);
    let mut ry = top + used as f32 * PART_ROW;
    if marks {
        let below = n - first - shown;
        let text: String = if below > 0 {
            m::panel_more_below(below).into()
        } else {
            m::panel_more_above(first).into()
        };
        let name = if below > 0 { names.down } else { names.up };
        out.push(more_mark(env, name, &text, [w, 20.0], [x + w / 2.0, ry + PART_MARK / 2.0]));
        ry += PART_MARK;
    }
    PlayersPart {
        actors: out,
        height: ry - y,
        first,
        shown,
        per,
    }
}

/// One player's line as the room's players panel draws it
/// ([`players_panel`]; the stage's prompt and the audience's page draw
/// theirs with it): in `[x, y, w, row]` (`row` tall), the local player's
/// plate under it, the place where the match is `placed`, the name and
/// its badge, a CPU's stars or a person's role, the policy's quiet word,
/// the status and the match points; its parts named from `id`.
pub fn player_line(
    env: &Env,
    c: &mut Vec<Actor>,
    id: &str,
    p: &PlayerRow,
    [x, ry, w, row]: [f32; 4],
    placed: bool,
) {
    let (place_w, points_w) = if placed { (30.0, 64.0) } else { (0.0, 0.0) };
    let mid = ry + row / 2.0;
    if p.me {
        let mut plate = look::quad_at(
            &format!("{id}.Me"),
            [w, row - 4.0],
            look::with_alpha(env.color("Text"), 0.06),
            [x + w / 2.0, mid],
        );
        look::call(&mut plate, "cornerradius", &[6.0]);
        c.push(plate);
    }
    if let Some(place) = p.place {
        c.push(left(
            env.numbers(&format!("{id}.Place"), &place.to_string(), 14.0, "Muted"),
            x + 8.0,
            mid,
        ));
    }
    let name_x = x + 8.0 + place_w;
    let end = x + w - 8.0 - points_w;
    let status_w = if p.status.is_empty() {
        0.0
    } else {
        look::mono_width(&p.status, 13.0).min(170.0) + 12.0
    };
    // Their attack policy, a quiet word before the status.
    let policy_w = if p.policy.is_empty() {
        0.0
    } else {
        look::mono_width(&p.policy, 12.0).min(90.0) + 12.0
    };
    if !p.policy.is_empty() {
        let mut policy = env.text(&format!("{id}.Policy"), &p.policy, 12.0, 400, "Faint");
        look::ellipsized(&mut policy, 90.0);
        c.push(right(policy, end - status_w, mid + 1.0));
    }
    let room = (end - status_w - policy_w - name_x).max(60.0);
    let color = match (p.me, p.tone) {
        (_, Tone::Away) => "Faint",
        (true, _) => "Text",
        _ => "Secondary",
    };
    // What follows the name: a CPU's stars, a person's role.
    let stars_w = crate::stars::width(STARS_HEIGHT);
    let after = match p.level {
        Some(_) => stars_w + 10.0,
        None if p.role.is_empty() => 0.0,
        None => look::mono_width(&p.role, 12.0) + 10.0,
    };
    let (parts, name_w) = name_and_badge(
        env,
        &format!("{id}.Name"),
        &p.name,
        p.badge,
        (15.0, 600, color),
        [name_x, mid],
        (room - after).max(40.0),
    );
    c.extend(parts);
    let ax = name_x + name_w + 10.0;
    if let Some(level) = p.level {
        level_line(
            env,
            c,
            id,
            (level, &p.sub),
            [ax, mid + 1.0],
            (room - name_w - 10.0).max(stars_w),
            false,
        );
    } else if !p.role.is_empty() {
        let mut role = env.text(&format!("{id}.Role"), &p.role, 12.0, 400, "Muted");
        look::ellipsized(&mut role, (room - name_w - 10.0).max(24.0));
        c.push(left(role, ax, mid + 1.0));
    }
    if !p.status.is_empty() {
        let mut status = env.text(
            &format!("{id}.State"),
            &p.status,
            13.0,
            600,
            tone_color(p.tone),
        );
        look::ellipsized(&mut status, 170.0);
        c.push(right(status, end, mid));
    }
    if let Some(points) = &p.points {
        c.push(right(
            env.numbers(&format!("{id}.Points"), points, 15.0, color),
            x + w - 8.0,
            mid,
        ));
    }
}

/// A name's two letters for a phone's strip: the first of its first two
/// words, else its first two.
fn initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split_whitespace()
        .filter(|w| w.chars().next().is_some_and(char::is_alphanumeric))
        .collect();
    let s: String = match words.as_slice() {
        [] => name.chars().take(2).collect(),
        [one] => one.chars().take(2).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
    };
    s.to_uppercase()
}

/// A phone's players: one line (`Players.Strip`, a tap opens the full
/// list) of everyone's initials (You whole), each colored by what they
/// are doing as the panel's statuses are, then how many they are; those
/// that do not fit are counted. `players`: [`player_rows`].
pub fn players_strip(
    env: &Env,
    r: &RoomClient,
    players: Vec<PlayerRow>,
    [x, y, w, h]: [f32; 4],
) -> Actor {
    let rows = panel_order(players);
    let cap = r.state().policy.max_competitors;
    let mid = y + h / 2.0;
    let count: String = m::players_count(players_counted(&rows), cap).into();
    let count_w = look::mono_width(&count, 12.0) + 22.0;
    let mut c = Vec::new();
    // The hit area: drawn, faintly (`crate::hit`).
    let mut plate = look::quad_at(
        "",
        [w, h],
        look::with_alpha(env.color("Text"), 0.05),
        [x + w / 2.0, mid],
    );
    look::call(&mut plate, "cornerradius", &[6.0]);
    c.push(plate);
    let end = x + w - count_w - 4.0;
    let mut cx = x + 8.0;
    let mut hidden = 0;
    for (k, p) in rows.iter().enumerate() {
        let text: String = if p.me {
            p.name.clone()
        } else {
            UiText::data(initials(&p.name)).into()
        };
        let tw = look::mono_width(&text, 12.0);
        // Room for this one and, unless it is the last, the count of
        // the rest.
        let rest = if k + 1 < rows.len() { 34.0 } else { 0.0 };
        if cx + tw + rest > end {
            hidden = rows.len() - k;
            break;
        }
        let color = match p.tone {
            Tone::Good => "Accent",
            Tone::Away => "Faint",
            Tone::Failed => "Danger",
            Tone::Plain if p.me => "Text",
            Tone::Plain => "Secondary",
        };
        c.push(left(env.text("", &text, 12.0, 700, color), cx, mid));
        cx += tw + 10.0;
    }
    if hidden > 0 {
        c.push(left(
            env.text("", &m::strip_more(hidden), 12.0, 600, "Muted"),
            cx,
            mid,
        ));
    }
    c.push(right(
        env.text("", &count, 12.0, 400, "Muted"),
        x + w - 22.0,
        mid,
    ));
    c.push(right(
        env.text("", "›", 14.0, 700, "Muted"),
        x + w - 8.0,
        mid,
    ));
    super::frame(PLAYERS_STRIP, c)
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
            name: member_name(mb),
            role: snake(&mb.role),
            admin: mb.admin,
            connection: snake(&mb.connection),
            participation: s
                .participation(mb.id)
                .map(|p| snake(&p))
                .unwrap_or_default(),
            cpu: mb.is_cpu(),
            bot: mb.is_bot(),
            level: s.opponent(mb.id).map(cpu_level),
            personality: s.opponent(mb.id).and_then(|o| o.personality.clone()),
            ready: upcoming.is_some_and(|st| st.prep(mb.id).ready),
            points: standings
                .iter()
                .find(|st| st.participant == mb.id)
                .map(|st| st.total.to_string())
                .unwrap_or_default(),
            at_start: false,
            targeting: snake(&mb.targeting()),
            picker: s.picker() == Some(mb.id),
        })
        .collect();
    let roster = crate::state::cpu_roster::roster_of(&s.next.rules, r.humans());
    if s.phase == Phase::Lobby || s.current.is_none() {
        for (n, spec) in roster.iter().enumerate() {
            members.push(RoomMemberView {
                name: spec.name.clone().unwrap_or_else(|| {
                    m::cpu_name(u16::try_from(n + 1).unwrap_or(u16::MAX)).into()
                }),
                cpu: true,
                level: Some(cpu_level(spec)),
                personality: spec.personality.clone(),
                at_start: true,
                targeting: snake(&spec.targeting.unwrap_or_default()),
                ..RoomMemberView::default()
            });
        }
    }
    let spot = s.spotlight.as_ref();
    let leave_cost = r.leave_cost();
    let my_role = r.my_role();
    let invite = r.invite_link();
    let view = RoomView {
        view: view.into(),
        phase: format!("{:?}", s.phase.kind()).to_lowercase(),
        revision: s.revision.0,
        host: r.chooses(),
        picker: s.picker().map(|id| id.0),
        can_take_pick: r.may_take_pick(),
        stages,
        members,
        cpus: u16::try_from(crate::state::cpu_roster::roster_of(rules, r.humans()).len())
            .unwrap_or(u16::MAX),
        stage_count: if rules.open_ended { 0 } else { rules.stages },
        ruleset: rules.ruleset_id().into(),
        difficulty: difficulty_id(rules.difficulty).into(),
        boos: rules.boos,
        attacks: r.attacks_played(rules),
        attacks_offered: r.attacks_offered(),
        targeting: snake(&r.my_targeting()),
        watching: r.watching(),
        can_choose_intent: r.may_choose_intent(),
        locked: r.locked_rules().is_some(),
        ready: r.ready(),
        countdown: seconds_left(&r),
        audience: r.audience(),
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
        leave_cost: leave_cost.as_ref().map(snake),
        leave_text: leave_cost.as_ref().map(|c| leave_text(c).into()),
        manages_hosts: r.manages_hosts(),
        host_leaves: r.is_online().then(|| snake(&r.if_host_leaves())),
        online: r.online_link().map(|l| RoomOnlineView {
            room_id: l.room_id().into(),
            name: l.name().map(str::to_string),
            link: snake(&l.state()),
            role: my_role.map(|(role, _)| snake(&role)).unwrap_or_default(),
            admin: my_role.is_some_and(|(_, a)| a),
            can_invite: invite.is_some(),
            invite: invite.clone(),
            can_rotate: r.may_replace_invite(),
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
        draft_rules: kept.draft_rules,
        draft_points: kept.draft_points,
        draft_charts: kept.draft_charts,
        draft_at_once: kept.draft_at_once,
        draft_shocks: kept.draft_shocks,
        rules_problem: kept.rules_problem,
        rules_running: kept.rules_running,
        rules_changed: kept.rules_changed,
        restart_offered: kept.restart_offered,
        rules_destination: kept.rules_destination,
        rules_from: kept.rules_from,
        stage_draft_rules: kept.stage_draft_rules,
        stage_draft_points: kept.stage_draft_points,
        points_shift: kept.points_shift,
        add_open: kept.add_open,
        players_open: kept.players_open,
        standings_open: kept.standings_open,
        host_open: kept.host_open,
        leave_open: kept.leave_open,
        due_open: kept.due_open,
        ..view
    });
}

/// A list's mark of the lines it does not show ("N more above",
/// "N more below"): a link of `size` centered on `at`, named `name`,
/// pressed to scroll that way.
pub(crate) fn more_mark(
    env: &Env,
    name: &str,
    text: &str,
    size: [f32; 2],
    at: [f32; 2],
) -> Actor {
    env.component("Link", "Button", &look::Spec::new(size, text), name, at)
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
    match &m.kind {
        MemberKind::Human { name, .. } => shown_name(name, m.is_bot()),
        MemberKind::Cpu { ordinal, opponent } => cpu_name(opponent, *ordinal),
    }
}

/// `name` (data) as shown: labeled Bot where `bot`.
pub(crate) fn shown_name(name: &str, bot: bool) -> String {
    if bot {
        m::bot_name(UiText::data(name)).into()
    } else {
        UiText::data(name).into()
    }
}

/// A member's name as the debug view says it: theirs, a CPU's from its
/// roster entry, else "CPU n" in the language shown.
pub(crate) fn member_name(member: &ddi_room::Member) -> String {
    match &member.kind {
        MemberKind::Human { name, .. } => name.clone(),
        MemberKind::Cpu { ordinal, opponent } => opponent
            .name
            .clone()
            .unwrap_or_else(|| m::cpu_name(*ordinal).into()),
    }
}

/// The local player's result of a stage, as a line (the standings pop-up,
/// the stage's results).
pub fn mark_text(mark: &StageMark) -> String {
    let p = &mark.placement;
    let points = points_text(p.points);
    match p.finish {
        Finish::Survived => m::mark_survived(mark.number, p.place, mark.field, points),
        Finish::Eliminated => m::mark_out(mark.number, p.place, mark.field, points),
        Finish::Failed => m::mark_failed(mark.number, p.place, mark.field, points),
        Finish::Forfeit => m::mark_forfeit(mark.number, p.place, mark.field),
    }
    .into()
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
        LoadProblem::PlanMismatch => m::load_plan(),
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
            Rejection::NoPlayingHumans => m::refused_all_watching(),
            Rejection::Cap(_) => m::refused_players_full(),
            Rejection::WatchOnly => m::refused_watch_only(),
            Rejection::StageStarted | Rejection::StartCommitted => m::refused_started(),
            Rejection::WrongPhase { .. } | Rejection::NotUpcoming | Rejection::WindowOpen => {
                m::refused_phase()
            }
            Rejection::Unauthorized { .. } => m::refused_host_only(),
            Rejection::NotPicker { .. } => m::refused_not_picker(),
            Rejection::InvalidRules(p) | Rejection::RulesNeedReview(p) => rules_problem(p),
            Rejection::RulesLandLater { from: Some(_), .. } => m::refused_rules_later(),
            Rejection::RulesLandLater { from: None, .. } => m::refused_rules_no_stage(),
            Rejection::Stale { .. } => m::refused_rules_changed(),
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

/// A match difficulty as the debug state and the messages' selectors name
/// it.
pub fn difficulty_id(d: ddi_room::MatchDifficulty) -> &'static str {
    match d {
        ddi_room::MatchDifficulty::Easy => "easy",
        ddi_room::MatchDifficulty::Normal => "normal",
        ddi_room::MatchDifficulty::Hard => "hard",
    }
}

/// A match difficulty's name, as Options names DDI's.
pub fn difficulty_name(d: ddi_room::MatchDifficulty) -> UiText {
    use ddi_l10n::msg::options as o;
    match d {
        ddi_room::MatchDifficulty::Easy => o::ddi_easy(),
        ddi_room::MatchDifficulty::Normal => o::ddi_normal(),
        ddi_room::MatchDifficulty::Hard => o::ddi_hard(),
    }
}

/// The match's rules in a few words, where they are summarized (the Rules
/// pop-up's locked line, a stage's prompt): the ruleset's name, then what
/// differs from the usual: its difficulty where it is not Normal, Boos
/// where they are on, "No attacks" where the match has none though
/// the room offers them (`attacks`: the rule where attacks are offered,
/// which is On by default; none where they are not, and nothing is
/// said), and "Charts as picked" where the charts do not adapt (Adaptive
/// is the default since 2026-11-14; [`rules_words_in`] for a room that
/// does not offer it, where nothing is said).
pub fn rules_words(rules: &crate::state::room::StageRules, attacks: Option<bool>) -> UiText {
    rules_words_in(rules, attacks, true)
}

/// The people an operator manages in the Players pop-up and Host
/// options: everyone else for the owner; for a co-host, only those held
/// to watching (whom they may let play).
pub(crate) fn manageable(r: &RoomClient) -> Vec<&ddi_room::Member> {
    let owner = r.manages_hosts();
    r.other_people()
        .into_iter()
        .filter(|mb| owner || (mb.watch_only && mb.role != ddi_room::Role::Owner))
        .collect()
}

/// Match rules `rules` in a few words as room `r` plays them: attacks and
/// Adaptive charts said only where it offers them ([`rules_words_in`]).
pub fn match_rules_words(r: &RoomClient, rules: &ddi_room::MatchRules) -> UiText {
    rules_words_of(
        &crate::state::room::StageRules::of(rules),
        r.attacks_offered().then(|| rules.attacks()),
        r.adaptive_offered(),
        Some(rules),
    )
}

/// [`rules_words`] in a room that offers Adaptive charts or not
/// (`RoomClient::adaptive_offered`): where it does not, As picked is no
/// choice and is not said.
pub fn rules_words_in(
    rules: &crate::state::room::StageRules,
    attacks: Option<bool>,
    adaptive_offered: bool,
) -> UiText {
    rules_words_of(rules, attacks, adaptive_offered, None)
}

/// [`rules_words_in`], with the match's own rules where they are known
/// (`whole`): its Stages Open and Joining during a match said where they
/// are not the usual (round AC), as When life runs out is from the
/// stage's.
fn rules_words_of(
    rules: &crate::state::room::StageRules,
    attacks: Option<bool>,
    adaptive_offered: bool,
    whole: Option<&ddi_room::MatchRules>,
) -> UiText {
    let on = |b: bool| if b { "on" } else { "off" };
    let charts = if adaptive_offered {
        rules.charts
    } else {
        ddi_room::Charts::Adaptive
    };
    m::rules_words(
        ruleset_name(&rules.ruleset),
        difficulty_id(rules.difficulty),
        on(rules.boos),
        on(attacks.unwrap_or(true)),
        rules.points.id(),
        charts.id(),
        // Only where the stage has attacks.
        if attacks.unwrap_or(true) { at_once_id(rules.at_once) } else { "three" },
        if attacks.unwrap_or(true) { shocks_id(rules.shocks) } else { "four" },
        rules.easiest.id(),
        if whole.is_some_and(|w| w.open_ended) { "open" } else { "counted" },
        on(whole.is_none_or(|w| w.mid_match_entry)),
    )
}

/// Shocks per attack by its id in the messages.
pub fn shocks_id(s: ddi_room::ShocksPerAttack) -> &'static str {
    match s {
        ddi_room::ShocksPerAttack::Two => "two",
        ddi_room::ShocksPerAttack::Four => "four",
        ddi_room::ShocksPerAttack::Six => "six",
        ddi_room::ShocksPerAttack::Eight => "eight",
    }
}

/// Attacks at once by its id in the messages.
pub fn at_once_id(a: ddi_room::AttacksAtOnce) -> &'static str {
    match a {
        ddi_room::AttacksAtOnce::One => "one",
        ddi_room::AttacksAtOnce::Two => "two",
        ddi_room::AttacksAtOnce::Three => "three",
        ddi_room::AttacksAtOnce::NoLimit => "no_limit",
    }
}

/// Attacks at once's value as its row shows it: the number, or No limit.
pub fn at_once_name(a: ddi_room::AttacksAtOnce) -> UiText {
    match a.limit() {
        Some(n) => m::rules_at_once_count(n as usize),
        None => m::rules_at_once_no_limit(),
    }
}

/// The rows a comparison of two rules covers beyond those every one has
/// (Timing difficulty, Boos): each where the room offers it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RuleGates {
    pub stages: bool,
    pub charts: bool,
    /// The CPUs, compared as seated beside this many people.
    pub cpus: Option<usize>,
    pub attacks: bool,
    /// Attacks at once, compared where the rules after have attacks On.
    pub at_once: bool,
    /// Shocks per attack, compared as Attacks at once is.
    pub shocks: bool,
    pub points: bool,
}

/// The rows whose rules differ from `before` to `after`, among those
/// `gates` covers, in the Rules pop-up's order (`stages`, `difficulty`,
/// `boos`, `charts`, `cpus`, `attacks`, `at_once`, `shocks`, `points`).
pub(crate) fn rules_diff(
    before: &ddi_room::MatchRules,
    after: &ddi_room::MatchRules,
    gates: RuleGates,
) -> Vec<&'static str> {
    let roster = |rules, humans| crate::state::cpu_roster::roster_of(rules, humans);
    [
        (
            "stages",
            gates.stages
                && (before.open_ended != after.open_ended
                    || (!after.open_ended && before.stages != after.stages)),
        ),
        ("difficulty", before.difficulty != after.difficulty),
        ("boos", before.boos != after.boos),
        (
            "mid_match",
            gates.stages && before.mid_match_entry != after.mid_match_entry,
        ),
        ("charts", gates.charts && before.charts != after.charts),
        // When life runs out, as each plays it (revision 16: under both
        // chart rules; a rule set stored before reads as it was).
        ("easiest", before.life_out() != after.life_out()),
        (
            "cpus",
            gates
                .cpus
                .is_some_and(|humans| roster(before, humans) != roster(after, humans)),
        ),
        ("attacks", gates.attacks && before.attacks() != after.attacks()),
        (
            "at_once",
            gates.at_once && after.attacks() && before.at_once != after.at_once,
        ),
        (
            "shocks",
            gates.shocks && after.attacks() && before.shocks != after.shocks,
        ),
        ("points", gates.points && before.points != after.points),
    ]
    .into_iter()
    .filter_map(|(id, differs)| differs.then_some(id))
    .collect()
}

/// What a change of the running match's per-stage rules changes, from
/// the rules `before` it (the stage before the change's) to `after`, in
/// the parts' words joined ("Hard timing · Boos · No attacks"); `None`
/// where nothing a player plays by (or Points) differs.
pub fn change_words(
    before: &ddi_room::MatchRules,
    after: &ddi_room::MatchRules,
    attacks_offered: bool,
) -> Option<UiText> {
    let gates = RuleGates {
        charts: true,
        attacks: attacks_offered,
        at_once: attacks_offered,
        shocks: attacks_offered,
        points: true,
        ..RuleGates::default()
    };
    let diff = rules_diff(before, after, gates);
    let on = |b: bool| if b { "on" } else { "off" };
    ["difficulty", "boos", "attacks", "charts", "at_once", "shocks", "points"]
        .into_iter()
        .filter(|id| diff.contains(id))
        .map(|id| match id {
            "difficulty" => m::rules_change_timing(difficulty_id(after.difficulty)),
            "boos" => m::rules_change_boos(on(after.boos)),
            "attacks" => m::rules_change_attacks(on(after.attacks())),
            "charts" => m::rules_change_charts(after.charts.id()),
            "at_once" => m::rules_change_at_once(at_once_id(after.at_once)),
            "shocks" => m::rules_change_shocks(shocks_id(after.shocks)),
            _ => m::rules_change_points(after.points.id()),
        })
        .reduce(m::rules_change_join)
}

/// The running match's latest change of its per-stage rules whose stage
/// has not begun yet (nobody readied for it), with that stage's number
/// and what it changes: what the status line says until then.
pub fn pending_change(r: &RoomClient) -> Option<(u16, UiText)> {
    let s = r.state();
    if !s.phase.kind().match_running() {
        return None;
    }
    let m = s.current.as_ref()?;
    let change = m.changes.last()?;
    let index = m.stage_index(change.from)?;
    let st = &m.stages[index];
    let gathering = matches!(
        s.phase,
        Phase::Preparing { stage, window: Some(_) } if stage == change.from
    );
    if st.play.is_some() || gathering {
        return None;
    }
    let before = match index.checked_sub(1) {
        Some(i) => m.rules_for(m.stages[i].id),
        None => m.rules.clone(),
    };
    let words = change_words(&before, &m.rules_for(change.from), r.attacks_offered())?;
    Some((u16::try_from(index + 1).unwrap_or(u16::MAX), words))
}

/// The Charts choice's name (`MatchRules::charts`).
pub fn charts_name(z: ddi_room::Charts) -> UiText {
    match z {
        ddi_room::Charts::AsPicked => m::rules_charts_as_picked(),
        ddi_room::Charts::Adaptive => m::rules_charts_adaptive(),
    }
}

/// What the At zero life choice does, under its row.
pub fn charts_about(z: ddi_room::Charts) -> UiText {
    match z {
        ddi_room::Charts::AsPicked => m::rules_charts_picked_about(),
        ddi_room::Charts::Adaptive => m::rules_charts_adaptive_about(),
    }
}

/// The Points choice's name (`MatchRules::points`).
pub fn points_name(b: ddi_room::PointsBias) -> UiText {
    match b {
        ddi_room::PointsBias::Strong => m::rules_points_strong(),
        ddi_room::PointsBias::Medium => m::rules_points_medium(),
        ddi_room::PointsBias::Mild => m::rules_points_mild(),
    }
}

/// What the Points choice favors, a sentence.
pub fn points_about(b: ddi_room::PointsBias) -> UiText {
    match b {
        ddi_room::PointsBias::Strong => m::rules_points_strong_about(),
        ddi_room::PointsBias::Medium => m::rules_points_medium_about(),
        ddi_room::PointsBias::Mild => m::rules_points_mild_about(),
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
