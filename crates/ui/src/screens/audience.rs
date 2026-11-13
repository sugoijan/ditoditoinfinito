//! A room watched with its audience, between stages
//! (`docs/plans/multiplayer-audience.md`, "What the game builds"): a page
//! made from the room's public view and remade as it changes: the room's
//! name and host, what the room is doing (what is being picked, the stage
//! that plays), the match's rules in words, the players and the
//! standings, the last stage's result and how many watch; and where the
//! seat stands (joining, reconnecting, refused, ended, with why and the
//! way back).
//!
//! The stage itself is watched in the play's spectator view: the screen
//! hands the shell each stage's song to prepare from the room's share
//! ([`AudienceCall::Prepare`]) and, once the room starts it, the stage to
//! watch ([`AudienceCall::Watch`]); the shell comes back here after it.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_l10n::UiText;
use ddi_l10n::msg::room as m;
use ddi_room::Phase;
use ddi_room::audience::AudienceEnd;
use ddi_room::audience::public::{PublicContent, PublicKind, PublicView};
use serde::Serialize;

use super::Env;
use super::page::{Action, Block, PAGE_CHANGED, PageCall, PageDoc, PageFeed, PageScreen, PageTitle};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::audience::{AudienceClient, Refused, SeatState, SharedAudience};
use crate::state::online::Redirect;
use crate::state::room::{LoadJob, StageOrder};

/// What the audience's page asks of its shell.
#[derive(Clone, Debug)]
pub enum AudienceCall {
    /// Read this stage's song from the room's share (answered by
    /// [`AudienceClient::finish_load`]).
    Prepare(LoadJob),
    /// Watch this stage now (the play's spectator view, fed by the
    /// audience's frames: `ddi_session::battle::live`).
    Watch(Box<StageOrder>),
    /// Where signing in again needs.
    Redirect(Redirect),
    /// Back: the seat is left.
    Leave,
}

pub type AudienceShell = Rc<dyn Fn(AudienceCall, &mut Cx)>;

/// The audience as the debug state shows it (`ThemeState::audience`,
/// the web's `__DDI_DEBUG.audience`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AudienceView {
    /// `joining`, `watching`, `reconnecting`, `refused`, `ended`,
    /// `sign_in_again`.
    pub seat: String,
    /// Why refused or ended (a code).
    pub why: Option<String>,
    pub room: String,
    pub name: Option<String>,
    pub phase: Option<String>,
    pub count: Option<u32>,
    pub frames: u32,
    pub sockets: u32,
    /// The music's delay behind the room's song time for the stage
    /// handed over last (ms).
    pub delay_ms: Option<u64>,
    pub players: usize,
    pub stages: usize,
}

/// The seat's state as a word, and why.
pub fn seat_words(s: &SeatState) -> (&'static str, Option<String>) {
    match s {
        SeatState::Joining => ("joining", None),
        SeatState::Watching => ("watching", None),
        SeatState::Reconnecting => ("reconnecting", None),
        SeatState::Refused(Refused::Audience(r)) => ("refused", Some(r.code().into())),
        SeatState::Refused(Refused::Other(c)) => ("refused", Some(c.clone())),
        SeatState::Ended(AudienceEnd::Closed) => ("ended", Some("audience_closed".into())),
        SeatState::Ended(AudienceEnd::RoomClosed) => ("ended", Some("room_closed".into())),
        SeatState::SignInAgain => ("sign_in_again", None),
    }
}

/// The debug state's view of `a`.
pub fn debug_view(a: &AudienceClient) -> AudienceView {
    let (seat, why) = seat_words(&a.state());
    let v = a.view();
    AudienceView {
        seat: seat.into(),
        why,
        room: a.room_id().into(),
        name: v.map(|v| v.name.clone()),
        phase: v.map(|v| super::room::snake(&v.phase.kind())),
        count: a.count(),
        frames: a.frames(),
        sockets: a.sockets(),
        delay_ms: a.current_stage().filter(|s| a.launched(*s)).map(|s| a.music_delay_ms(s)),
        players: v.map_or(0, |v| v.players.len()),
        stages: v.map_or(0, |v| v.stages.len()),
    }
}

/// Why the seat ended or was refused, a sentence.
pub fn seat_problem(s: &SeatState) -> Option<UiText> {
    Some(match s {
        SeatState::Refused(Refused::Audience(r)) => {
            super::online::invite_problem(&crate::state::online::api::Problem {
                status: r.status(),
                code: r.code().into(),
            })
        }
        SeatState::Refused(Refused::Other(code)) => {
            super::online::invite_problem(&crate::state::online::api::Problem {
                status: 0,
                code: code.clone(),
            })
        }
        SeatState::Ended(AudienceEnd::Closed) => m::audience_closed(),
        SeatState::Ended(AudienceEnd::RoomClosed) => m::invite_room_closed(),
        SeatState::SignInAgain => m::link_sign_in_again(),
        _ => return None,
    })
}

/// Why a seat ended during a watched stage, a sentence (said over the
/// play's place: `ddi_shell::play_view`).
pub fn seat_end_text(e: &crate::state::room::SeatEnd) -> UiText {
    use crate::state::room::SeatEnd;
    match e {
        SeatEnd::AudienceClosed => m::audience_closed(),
        SeatEnd::RoomClosed => m::invite_room_closed(),
        SeatEnd::LinkReplaced => m::audience_link_unknown(),
        SeatEnd::SignInAgain => m::link_sign_in_again(),
        SeatEnd::Refused(code) => super::online::invite_problem(&crate::state::online::api::Problem {
            status: 0,
            code: code.clone(),
        }),
    }
}

fn player_name(v: &PublicView, id: ddi_room::ParticipantId) -> String {
    v.players
        .iter()
        .find(|p| p.id == id)
        .map(|p| match (&p.name, &p.kind) {
            (Some(n), PublicKind::Bot) => super::room::shown_name(n, true),
            (Some(n), _) => UiText::data(n).into(),
            (None, PublicKind::Cpu { ordinal, .. }) => m::cpu_name(*ordinal).into(),
            (None, _) => String::new(),
        })
        .unwrap_or_default()
}

/// The stage's number, from 1.
fn number(v: &PublicView, stage: ddi_room::StageId) -> usize {
    v.stages.iter().position(|s| s.id == stage).map_or(1, |i| i + 1)
}

/// What the room is doing, a sentence.
fn doing(a: &AudienceClient, v: &PublicView) -> UiText {
    let song = |stage: ddi_room::StageId| {
        v.stages
            .iter()
            .find(|s| s.id == stage)
            .and_then(|s| s.card.as_ref())
            .map(|c| UiText::data(format!("{} — {}", c.title, c.artist)).to_string())
    };
    match &v.phase {
        _ if v.closed => m::invite_room_closed(),
        Phase::Lobby => m::audience_doing_lobby(),
        Phase::Preparing { stage, .. } | Phase::Intermission { next: stage } => {
            match (song(*stage), v.stages.iter().find(|s| s.id == *stage).map(|s| s.content)) {
                (Some(song), Some(PublicContent::Available | PublicContent::Pending)) => {
                    m::audience_doing_picked(number(v, *stage), song)
                }
                _ => m::audience_doing_picking(number(v, *stage)),
            }
        }
        Phase::Countdown { stage, .. } => m::status_countdown(number(v, *stage), 0),
        Phase::Stage { stage } | Phase::SongCompletion { stage } => {
            if !a.plays_live() {
                m::audience_doing_not_live(number(v, *stage))
            } else {
                m::status_online_stage(number(v, *stage))
            }
        }
        Phase::MatchResults => m::audience_doing_match_over(),
        Phase::Interrupted { .. } => m::audience_doing_stopped(),
    }
}

/// The page as `a` stands.
pub fn doc(a: &AudienceClient) -> PageDoc {
    let mut blocks = Vec::new();
    let state = a.state();
    match a.view() {
        // Still joining: said so (a refusal says why below instead).
        None if matches!(state, SeatState::Joining | SeatState::Reconnecting) => {
            blocks.push(Block::Text(m::audience_joining().into()))
        }
        None => {}
        Some(v) => {
            blocks.push(Block::Subheading(UiText::data(&v.name).into()));
            if let Some(host) = &v.host {
                blocks.push(Block::Text(m::invite_host(UiText::data(host)).into()));
            }
            if let Some(n) = a.count() {
                blocks.push(Block::Muted(m::audience_watching(n).into()));
            }
            blocks.push(Block::Text(doing(a, v).into()));
            if let Some(stage) = a.current_stage() {
                if a.plan_mismatch(stage) {
                    blocks.push(Block::Muted(m::audience_plan_mismatch().into()));
                }
                if a.load_problem(stage).is_some() {
                    blocks.push(Block::Muted(m::audience_song_unreadable().into()));
                }
            }
            blocks.push(Block::Muted(
                super::room::rules_words(&a.stage_rules(), Some(a.attacks())).into(),
            ));
            // The last stage confirmed: who won it.
            if let Some((i, s)) = v
                .stages
                .iter()
                .enumerate()
                .rev()
                .find(|(_, s)| s.play.as_ref().is_some_and(|p| p.result.is_some()))
                && let Some(r) = s.play.as_ref().and_then(|p| p.result.as_ref())
            {
                let first: Vec<String> = r
                    .placements
                    .iter()
                    .filter(|p| p.place == 1)
                    .map(|p| player_name(v, p.participant))
                    .collect();
                if !first.is_empty() {
                    blocks.push(Block::Text(
                        m::audience_stage_won(i + 1, first.join(", ")).into(),
                    ));
                }
            }
            blocks.push(Block::Gap);
            if !v.standings.is_empty() {
                blocks.push(Block::Heading(m::audience_standings().into()));
                for st in v.standings.iter().take(12) {
                    blocks.push(Block::Text(
                        m::audience_standing(
                            st.rank,
                            player_name(v, st.participant),
                            st.total.to_string(),
                        )
                        .into(),
                    ));
                }
                if v.standings.len() > 12 {
                    blocks.push(Block::Muted(m::panel_more_below(v.standings.len() - 12).into()));
                }
            } else if !v.players.is_empty() {
                blocks.push(Block::Heading(m::audience_players(v.players.len()).into()));
                let names: Vec<String> = v
                    .players
                    .iter()
                    .filter(|p| !p.left)
                    .take(24)
                    .map(|p| player_name(v, p.id))
                    .collect();
                blocks.push(Block::Text(UiText::data(names.join(", ")).into()));
            }
        }
    }
    match &state {
        SeatState::Joining if a.view().is_some() => {}
        SeatState::Joining | SeatState::Watching => {}
        SeatState::Reconnecting => blocks.push(Block::Note(m::link_reconnecting().into())),
        other => {
            if let Some(why) = seat_problem(other) {
                blocks.push(Block::Note(why.into()));
            }
        }
    }
    let mut actions = Vec::new();
    if state == SeatState::SignInAgain {
        actions.push(Action::new("signin", m::sign_in_again()).primary());
    }
    actions.push(Action::new("back", m::audience_leave()));
    blocks.push(Block::Gap);
    blocks.push(Block::Actions(actions));
    PageDoc {
        title: PageTitle::new(m::audience_title(), m::audience_heading()),
        blocks,
    }
}

/// The audience's page over its client.
pub struct AudienceScreen {
    env: Env,
    page: PageScreen,
    audience: SharedAudience,
    feed: PageFeed,
    shell: AudienceShell,
    seen: u64,
    time: f64,
}

impl AudienceScreen {
    pub fn new(env: Env, audience: SharedAudience, shell: AudienceShell) -> AudienceScreen {
        let feed: PageFeed = Rc::new(RefCell::new(None));
        let first = doc(&audience.borrow());
        let page_shell: super::page::PageShell = {
            let audience = audience.clone();
            let shell = shell.clone();
            Rc::new(move |call, cx| {
                match call {
                    PageCall::Leave => {
                        audience.borrow_mut().leave();
                        shell(AudienceCall::Leave, cx);
                        return None;
                    }
                    PageCall::Button(id) if id == "back" => {
                        audience.borrow_mut().leave();
                        shell(AudienceCall::Leave, cx);
                        return None;
                    }
                    PageCall::Button(id) if id == "signin" => {
                        audience.borrow_mut().sign_in_again();
                    }
                    _ => return None,
                }
                Some(doc(&audience.borrow()))
            })
        };
        let page =
            PageScreen::new(env.clone(), "audience", first).with_shell(page_shell, feed.clone());
        let seen = audience.borrow().version();
        AudienceScreen {
            env,
            page,
            audience,
            feed,
            shell,
            seen,
            time: 0.0,
        }
    }
}

impl Screen for AudienceScreen {
    fn shared_bars(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        self.page.name()
    }

    fn build(&mut self, cx: &mut Cx) -> ddi_actor::Actor {
        self.page.build(cx)
    }

    fn input(&mut self, input: &Input, root: &mut ddi_actor::Actor, cx: &mut Cx) -> Handled {
        self.page.input(input, root, cx)
    }

    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        self.page.presentation()
    }

    fn active_element(&self, focused: Option<&str>) -> Option<String> {
        self.page.active_element(focused)
    }

    fn cursor(&self, target: &crate::hit::Target) -> Option<crate::request::Cursor> {
        self.page.cursor(target)
    }

    fn animating(&self) -> bool {
        self.page.animating()
    }

    fn wake_at(&self) -> Option<f64> {
        // The seat's socket and the room's view come while nothing moves.
        Some(self.time + 0.1)
    }

    fn focusables(&self) -> Vec<crate::nav::Focusable> {
        self.page.focusables()
    }

    fn message(&mut self, msg: &crate::Message, root: &mut ddi_actor::Actor, cx: &mut Cx) {
        self.page.message(msg, root, cx);
    }

    fn update(&mut self, dt: f32, root: &mut ddi_actor::Actor, cx: &mut Cx) {
        self.time = cx.time();
        self.page.update(dt, root, cx);
        let now = (cx.clock() * 1000.0).max(0.0) as u64;
        let (jobs, order, redirect) = {
            let mut a = self.audience.borrow_mut();
            a.tick(now);
            let redirect = a.online().borrow_mut().take_redirect();
            (a.take_load_jobs(), a.take_stage_order(), redirect)
        };
        for job in jobs {
            (self.shell)(AudienceCall::Prepare(job), cx);
        }
        if let Some(r) = redirect {
            (self.shell)(AudienceCall::Redirect(r), cx);
        }
        let version = self.audience.borrow().version();
        self.env_publish();
        if version != self.seen {
            self.seen = version;
            let d = doc(&self.audience.borrow());
            *self.feed.borrow_mut() = Some(d);
            self.page.message(&crate::Message::new(PAGE_CHANGED), root, cx);
        }
        if let Some(order) = order {
            (self.shell)(AudienceCall::Watch(Box::new(order)), cx);
        }
    }
}

impl AudienceScreen {
    fn env_publish(&self) {
        let view = debug_view(&self.audience.borrow());
        self.env.state.borrow_mut().audience = Some(view);
    }
}
