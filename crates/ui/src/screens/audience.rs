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

use std::rc::Rc;

use ddi_l10n::UiText;
use ddi_l10n::msg::room as m;
use ddi_room::Phase;
use ddi_room::audience::AudienceEnd;
use ddi_room::audience::public::{PublicContent, PublicKind, PublicView};
use serde::Serialize;

use ddi_actor::Actor;
use ddi_engine::MenuInput as HintAction;

use super::dialog::wrap_lines;
use super::{Env, footer, frame, left};
use crate::look::Spec;
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input};
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
    /// Watch pressed on the page (inside the press: the web makes and
    /// starts the stage's audio context there, as the invite page's
    /// Watch does); the stage's [`AudienceCall::Watch`] follows at once.
    WatchPressed,
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
    /// `joining`, `watching`, `reconnecting`, `refused`, `ended`.
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
    /// The next stage's (`audience-view-v1`: moved by what the last one
    /// needed) and the rules' id.
    pub next_delay_ms: u64,
    pub view_rules: String,
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
        next_delay_ms: a.next_music_delay_ms(),
        view_rules: a.view_rules().id.into(),
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
        SeatEnd::ClosedByOperator => m::closed_by_operator(),
        SeatEnd::LinkReplaced => m::audience_link_unknown(),
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

/// Whole seconds left of a stage's countdown, on the room's clock as
/// the seat estimates it (none before it does).
fn countdown_seconds(a: &AudienceClient) -> Option<u64> {
    use crate::state::room::LiveRoom;
    let Phase::Countdown { start_at, .. } = a.view()?.phase else {
        return None;
    };
    let now = a.room_ms_at(a.now_ms())?;
    Some(start_at.0.saturating_sub(now.0).div_ceil(1000))
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
        _ if v.closed && a.closed_by_operator() => m::closed_by_operator(),
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
        Phase::Countdown { stage, .. } => match countdown_seconds(a) {
            Some(seconds) => m::status_countdown(number(v, *stage), seconds),
            None => m::status_countdown_soon(number(v, *stage)),
        },
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

/// One line of the audience's card: its text and how it reads.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Tone {
    Title,
    Text,
    Accent,
    Muted,
    Note,
}

/// The card's lines as `a` stands: the room's name and host, what the
/// room is doing (the song picked for the next stage, a stage's number
/// and the match's end), who won the last stage, the rules in a few
/// words, how many watch; and where the seat stands (joining,
/// reconnecting, refused, ended, with why).
fn lines(a: &AudienceClient) -> Vec<(String, Tone)> {
    let mut out = Vec::new();
    let state = a.state();
    match a.view() {
        // Still joining: said so (a refusal says why below instead).
        None if matches!(state, SeatState::Joining | SeatState::Reconnecting) => {
            out.push((m::audience_joining().into(), Tone::Text))
        }
        None => {}
        Some(v) => {
            out.push((UiText::data(&v.name).into(), Tone::Title));
            if let Some(host) = &v.host {
                out.push((m::invite_host(UiText::data(host)).into(), Tone::Muted));
            }
            out.push((doing(a, v).into(), Tone::Accent));
            if let Some(stage) = a.current_stage() {
                if a.plan_mismatch(stage) {
                    out.push((m::audience_plan_mismatch().into(), Tone::Muted));
                }
                if a.load_problem(stage).is_some() {
                    out.push((m::audience_song_unreadable().into(), Tone::Muted));
                }
            }
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
                    out.push((m::audience_stage_won(i + 1, first.join(", ")).into(), Tone::Text));
                }
            }
            out.push((
                super::room::rules_words(&a.stage_rules(), Some(a.attacks())).into(),
                Tone::Muted,
            ));
            if let Some(n) = a.count() {
                out.push((m::audience_watching(n).into(), Tone::Muted));
            }
        }
    }
    match &state {
        SeatState::Joining | SeatState::Watching => {}
        SeatState::Reconnecting => out.push((m::link_reconnecting().into(), Tone::Note)),
        // (What the room is doing says why it ended where its operator
        // closed it: not said twice.)
        SeatState::Ended(AudienceEnd::RoomClosed) if a.closed_by_operator() => {}
        other => {
            if let Some(why) = seat_problem(other) {
                out.push((why.into(), Tone::Note));
            }
        }
    }
    out
}

/// The players' count over the list.
fn players_count(v: &PublicView) -> String {
    m::audience_players(player_rows(v).len()).into()
}

/// The competitors as the public view has them, as the players panel's
/// rows (`room::player_line`): each one's name (a CPU's level as stars,
/// a bot labeled), their place and match points once a stage of the
/// match is confirmed, and Out where the match is over for them; in the
/// room's order (by place once placed). Only what the public view
/// carries: no readiness, no connection, no policy.
pub fn player_rows(v: &PublicView) -> Vec<super::room::PlayerRow> {
    use super::room::{PlayerRow, Tone as RowTone};
    let placed = v.standings.iter().any(|st| {
        st.stages
            .iter()
            .any(|sg| matches!(sg, ddi_room::StageStanding::Placed(_)))
    });
    let rows: Vec<PlayerRow> = v
        .players
        .iter()
        .filter(|p| !p.left || p.in_match)
        .map(|p| {
            let standing = v
                .standings
                .iter()
                .find(|st| st.participant == p.id)
                .filter(|_| placed);
            let level = match p.kind {
                // The view carries the engine's index, as a seat does.
                PublicKind::Cpu { level, .. } => Some(super::room::shown_level(level)),
                _ => None,
            };
            let status: String = if p.out_for_match {
                m::play_out().into()
            } else {
                String::new()
            };
            PlayerRow {
                name: player_name(v, p.id),
                badge: false,
                sub: String::new(),
                level,
                state: status.clone(),
                me: false,
                ready: level.is_some(),
                cpu: level.is_some(),
                plays: !p.out_for_match,
                place: standing.map(|st| st.rank),
                points: standing.map(|st| super::room::points_text(st.total).into()),
                role: String::new(),
                status,
                tone: if p.out_for_match {
                    RowTone::Away
                } else {
                    RowTone::Plain
                },
                policy: String::new(),
                id: None,
                manage: false,
            }
        })
        .collect();
    super::room::panel_order(rows)
}

/// The card's Watch: the stage being played, as it stands; and its
/// Stop watching.
pub const WATCH: &str = "watch";
const WATCH_BUTTON: &str = "Item.watch";
const LEAVE_BUTTON: &str = "Item.back";

/// The audience's page (`audience-page-v1`, the feedback round I): no
/// song select and no page of text, but a card built as a room's stage's
/// Ready card is (`ready-card-v1`: the same translucent plate, most of
/// the screen, the players' part in columns, `room::players_part`) over
/// the start menu's note rain before the first stage this seat watched,
/// and after it over that stage's picture (`AudienceClient::backdrop`),
/// darkened as the play's backdrop is; its buttons Watch (where a stage
/// plays that the seat is not watching) and Stop watching.
pub struct AudienceScreen {
    env: Env,
    audience: SharedAudience,
    shell: AudienceShell,
    screen: [f32; 2],
    zoom: f32,
    /// The buttons laid out, in order.
    buttons: Vec<&'static str>,
    /// The players' part's first row, rows, rows shown and a column's
    /// rows when last laid out.
    first: usize,
    shown: (usize, usize, usize),
    /// The client's version the card was built for.
    built: Option<u64>,
    /// The countdown's seconds as built: the card is built again as they
    /// change.
    seconds: Option<u64>,
    /// What the debug view was last published for.
    published: Option<(u64, u32, Option<u32>, u32, u64)>,
    /// Watch was offered when last laid out (it is focused as it comes).
    offered: bool,
    time: f64,
    /// The rain's clock (the screens' wall clock; still under reduced
    /// motion) and whether it moves.
    rain_time: f64,
    rain_animating: bool,
}

impl AudienceScreen {
    pub fn new(env: Env, audience: SharedAudience, shell: AudienceShell) -> AudienceScreen {
        AudienceScreen {
            env,
            audience,
            shell,
            screen: super::DESIGN,
            zoom: 1.0,
            buttons: Vec::new(),
            first: 0,
            shown: (0, 0, 0),
            built: None,
            seconds: None,
            published: None,
            offered: false,
            time: 0.0,
            rain_time: 0.0,
            rain_animating: false,
        }
    }

    /// What lies behind the card: the last stage watched's picture,
    /// darkened as the play's backdrop is (its blurred base, cover
    /// fitted), else the start menu's rain.
    fn backdrop(&mut self, cx: &mut Cx) -> Actor {
        let size = self.screen;
        let [w, h] = size;
        if let Some((source, _)) = self.audience.borrow().backdrop().cloned() {
            use ddi_actor::background::BASE_SIDE;
            let texture = cx.blurred_texture(&source, BASE_SIDE);
            let mut sprite = crate::look::named(
                ddi_actor::Kind::Sprite(ddi_actor::Sprite::quad([w, h])),
                "Audience.Backdrop",
            );
            if let ddi_actor::Kind::Sprite(sp) = &mut sprite.kind {
                sprite_texture(sp, texture);
            }
            let alpha = self
                .env
                .metric("Backdrop.Alpha", ddi_actor::background::BASE_ALPHA);
            crate::look::call(&mut sprite, "xy", &[w / 2.0, h / 2.0]);
            crate::look::call(&mut sprite, "diffuse", &[1.0, 1.0, 1.0, alpha]);
            self.rain_animating = false;
            return sprite;
        }
        match self
            .env
            .look
            .component("TitleMenu.Background", &Spec::new(size, ""))
        {
            Some(mut background) => {
                background.name = "MenuBackground".into();
                if let Some(source) = self.env.look.image("TitleMenu.Background") {
                    let texture = cx.texture(source, 1024.0);
                    super::sprites_mut(&mut background, &mut |s| s.texture = texture);
                }
                self.rain_animating = self.env.look.animate_component(
                    "TitleMenu.Background",
                    &mut background,
                    size,
                    self.rain_time,
                );
                background
            }
            None => frame("MenuBackground", Vec::new()),
        }
    }

    fn tree(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let compact;
        (self.screen, self.zoom, compact) = super::compact_fit(&self.env, vp);
        let env = self.env.clone();
        let [sw, sh] = self.screen;
        let a = self.audience.borrow();
        self.built = Some(a.version());
        self.seconds = countdown_seconds(&a);
        // A compact screen (a phone) has no footer of hints, as a page.
        let touch = compact;
        let footer_h = if touch { 0.0 } else { 40.0 };
        // The Ready card's sizes (`super::prompt`'s `ready-card-v1`).
        let (full_w, full_h) = if sw > 600.0 && sh >= 480.0 {
            let m = (0.03 * sw).clamp(24.0, 64.0);
            (sw - 2.0 * m, sh - footer_h - 2.0 * m.min(32.0))
        } else {
            (sw - 16.0, sh - footer_h - 16.0)
        };
        let classic = if sh < 480.0 { 560.0_f32.min(full_w) } else { 560.0 };
        let split = full_w - 294.0 >= 480.0;
        let w = if split { classic.min(full_w - 294.0) } else { full_w };
        let inner = w - 48.0;
        let mut c: Vec<Actor> = Vec::new();
        let mut y = 24.0;
        for (k, (text, tone)) in lines(&a).into_iter().enumerate() {
            let (size, weight, color) = match tone {
                Tone::Title => (26.0, 700, "Text"),
                Tone::Text => (15.0, 600, "Text"),
                Tone::Accent => (15.0, 600, "Accent"),
                Tone::Muted => (13.0, 400, "Muted"),
                Tone::Note => (14.0, 600, "Gold"),
            };
            if k > 0 && matches!(tone, Tone::Accent | Tone::Note) {
                y += 6.0;
            }
            for (i, l) in wrap_lines(&text, inner, size).into_iter().enumerate() {
                let mut t = env.text(&format!("Line{k}.{i}"), &l, size, weight, color);
                crate::look::ellipsized(&mut t, inner);
                y += size * 0.65;
                c.push(left(t, 24.0, y));
                y += size * 0.65;
            }
            y += 2.0;
        }
        y += 16.0;
        // The buttons: Watch (where offered) first, focused as it comes.
        let mut buttons: Vec<(&'static str, String, &str)> = Vec::new();
        if a.watchable().is_some() {
            buttons.push((WATCH_BUTTON, m::audience_watch_stage().into(), "PlayButton"));
        }
        buttons.push((LEAVE_BUTTON, m::audience_leave().into(), "Button"));
        let bh = if self.zoom > 1.0 { 48.0 } else { 40.0 };
        let rows = a.view().map(player_rows).unwrap_or_default();
        let words = (
            String::from(m::players()),
            a.view().map(players_count).unwrap_or_default(),
        );
        let names = super::room::PartNames {
            heading: "Audience.Players",
            count: "Audience.Count",
            row: "Audience.Player",
            down: "Audience.Down",
            up: "Audience.Up",
        };
        let mut part_at = None;
        if !split && !rows.is_empty() {
            // Under the lines, as tall as the card leaves above the
            // buttons.
            let room = (full_h - y - bh - 24.0 - 18.0).max(0.0);
            part_at = Some([24.0, y, inner, room]);
        }
        let mut place_part = |c: &mut Vec<Actor>, area: [f32; 4]| -> f32 {
            let part = super::room::players_part(
                &env,
                &rows,
                &names,
                (&words.0, &words.1),
                self.first,
                area,
            );
            self.first = part.first;
            self.shown = (rows.len(), part.shown, part.per);
            c.extend(part.actors);
            part.height
        };
        if let Some(area) = part_at {
            y += place_part(&mut c, area) + 18.0;
        }
        let widths: Vec<f32> = buttons
            .iter()
            .map(|(_, l, kind)| {
                let size = if *kind == "PlayButton" { 18.0 } else { 15.0 };
                (crate::look::mono_width(l, size) + 36.0).max(104.0)
            })
            .collect();
        let mut x = w - 24.0;
        for ((name, label, kind), bw) in buttons.iter().zip(&widths) {
            c.push(env.component(
                kind,
                "Button",
                &Spec::new([*bw, bh], label.as_str()),
                name,
                [x - bw / 2.0, y + bh / 2.0],
            ));
            x -= bw + 10.0;
        }
        y += bh;
        let h = (y + 24.0).max(full_h);
        let card_w = if split { full_w } else { w };
        if split && !rows.is_empty() {
            place_part(&mut c, [w, 24.0, full_w - w - 24.0, h - 48.0]);
        }
        drop(a);
        self.buttons = buttons.iter().map(|(n, ..)| *n).collect();
        let mut children = vec![env.component(
            "Dialog.Glass",
            "Panel",
            &Spec::new([card_w, h], ""),
            "Card",
            [0.0, 0.0],
        )];
        children.extend(c);
        let mut card = frame("Audience", children);
        let room = sh - 16.0 - footer_h;
        let zoom = (room / h).min(1.0);
        crate::look::call(&mut card, "zoom", &[zoom]);
        let top = ((sh - footer_h) - h * zoom) / 2.0;
        crate::look::call(&mut card, "xy", &[(sw - card_w * zoom) / 2.0, top.max(8.0)]);
        let mut all = vec![super::ground(&env, self.screen), self.backdrop(cx), card];
        if !touch {
            let (watch, leave) = (m::audience_watch_stage(), m::audience_leave());
            let first = if self.buttons.first() == Some(&WATCH_BUTTON) {
                watch.as_str()
            } else {
                leave.as_str()
            };
            all.push(footer(
                &env,
                self.screen,
                &[(&["ENTER"], first), (&["ESC"], leave.as_str())],
                &[
                    (&[HintAction::Confirm, HintAction::Start], first),
                    (&[HintAction::Back], leave.as_str()),
                ],
                "",
                &[],
                cx,
            ));
        }
        let mut root = ddi_actor::Actor::new(ddi_actor::Kind::Frame(all));
        super::transitions(&mut root, &env, &[("Audience", super::Edge::None)]);
        if self.zoom != 1.0 {
            crate::look::call(&mut root, "zoom", &[self.zoom]);
        }
        root
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx) {
        let left = crate::hit::find_mut(root, "Audience").map_or(0.0, |c| c.tween_time_left());
        *root = self.tree(cx);
        super::relaid(root, left, cx);
        let st = &mut self.env.state.borrow_mut();
        st.page_items = self.buttons.iter().map(|b| b.to_string()).collect();
    }

    /// Says what the seat shows (`ThemeState::audience`), when anything
    /// it says changed: the client's version, or the counts that move
    /// without it.
    fn env_publish(&mut self) {
        let a = self.audience.borrow();
        let key = (
            a.version(),
            a.frames(),
            a.count(),
            a.sockets(),
            a.next_music_delay_ms(),
        );
        if self.published == Some(key) {
            return;
        }
        self.published = Some(key);
        let view = debug_view(&a);
        drop(a);
        self.env.state.borrow_mut().audience = Some(view);
    }

    fn leave(&mut self, cx: &mut Cx) {
        self.audience.borrow_mut().leave();
        (self.shell)(AudienceCall::Leave, cx);
    }

    /// Watch pressed: in the press, the sound's gesture, then the stage
    /// handed over as it stands.
    fn watch(&mut self, cx: &mut Cx) {
        (self.shell)(AudienceCall::WatchPressed, cx);
        let order = self.audience.borrow_mut().watch_now();
        if let Some(order) = order {
            (self.shell)(AudienceCall::Watch(Box::new(order)), cx);
        }
    }

    fn scroll(&mut self, by: i32, root: &mut Actor, cx: &mut Cx) {
        let (n, shown, _) = self.shown;
        self.first = super::scrolled(self.first, by, n, shown);
        self.rebuild(root, cx);
    }
}

/// `texture` on a sprite drawn cover fitted.
fn sprite_texture(sp: &mut ddi_actor::Sprite, texture: u32) {
    sp.texture = texture;
    sp.fit = ddi_actor::Fit::Cover;
}

impl crate::screen::Screen for AudienceScreen {
    fn name(&self) -> &str {
        "audience"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.time = cx.time();
        self.rain_time = if self.env.reduced() { 0.0 } else { cx.clock() };
        let root = self.tree(cx);
        self.env.state.borrow_mut().page_items =
            self.buttons.iter().map(|b| b.to_string()).collect();
        self.env_publish();
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v: Vec<Focusable> = self.buttons.iter().map(|b| Focusable::new(*b)).collect();
        // Side by side: Left and Right between them, Watch the rightmost.
        let n = v.len();
        for i in 0..n {
            if i + 1 < n {
                v[i].left = Some(self.buttons[i + 1].into());
            }
            if i > 0 {
                v[i].right = Some(self.buttons[i - 1].into());
            }
        }
        v
    }

    fn first_focus(&self) -> Option<String> {
        self.buttons.first().map(|b| b.to_string())
    }

    fn animating(&self) -> bool {
        self.rain_animating && !self.env.reduced()
    }

    fn wake_at(&self) -> Option<f64> {
        // Answers come while nothing moves: looked for ten times a second.
        Some(self.time + 0.1)
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        self.time = cx.time();
        // The seat's socket and the room's view come while nothing moves.
        let now = cx.wall_ms();
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
        let changed = {
            let a = self.audience.borrow();
            self.built != Some(a.version()) || self.seconds != countdown_seconds(&a)
        };
        if changed {
            self.rebuild(root, cx);
        }
        // Watch focused first as it comes (Enter or Start presses it).
        let offered = self.buttons.first() == Some(&WATCH_BUTTON);
        if offered && !self.offered {
            cx.focus(WATCH_BUTTON);
            self.offered = true;
        } else if !offered {
            self.offered = false;
        }
        if !self.env.reduced() {
            self.rain_time = cx.clock();
        }
        if let Some(background) = crate::hit::find_mut(root, "MenuBackground") {
            self.rain_animating = self.env.look.animate_component(
                "TitleMenu.Background",
                background,
                self.screen,
                self.rain_time,
            );
        }
        self.env_publish();
        if let Some(order) = order {
            (self.shell)(AudienceCall::Watch(Box::new(order)), cx);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Menu {
                action: MenuAction::Back,
                ..
            } => {
                self.leave(cx);
                Handled::Yes
            }
            Input::Activate { name } if name == WATCH_BUTTON => {
                self.watch(cx);
                Handled::Yes
            }
            Input::Activate { name } if name == LEAVE_BUTTON => {
                self.leave(cx);
                Handled::Yes
            }
            Input::Activate { name } if name == "Audience.Down" || name == "Audience.Up" => {
                let page = self.shown.2.max(1) as i32;
                self.scroll(if name == "Audience.Down" { page } else { -page }, root, cx);
                Handled::Yes
            }
            Input::Wheel { delta, .. } if delta[1] != 0.0 => {
                self.scroll(super::wheel_step(*delta), root, cx);
                Handled::Yes
            }
            Input::Key(k) if k.down && matches!(k.code.as_str(), "PageDown" | "PageUp") => {
                let page = self.shown.2.max(1) as i32;
                self.scroll(if k.code == "PageDown" { page } else { -page }, root, cx);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_room::audience::public::PublicPlayer;
    use ddi_room::{Identity, MatchRules, ParticipantId, RoomId, RoomPolicy, RoomState};

    /// A view of a room of one person, CPUs at the engine's levels 0, 4
    /// and 9, and a person who left before the match.
    fn view() -> PublicView {
        let state = RoomState::new(
            RoomId::new("Room000001"),
            Identity::new("dev:alice"),
            "alice".into(),
            RoomPolicy::default(),
            MatchRules::hosted(),
            ddi_room::ContentSource::Shared,
        );
        let mut v = PublicView::of(&state, "Room");
        let cpu = |id: u64, level: u8| PublicPlayer {
            id: ParticipantId(id),
            name: None,
            kind: PublicKind::Cpu {
                level,
                ordinal: id as u16,
                seed: 0,
                personality: None,
                targeting: Default::default(),
            },
            in_match: false,
            out_for_match: false,
            left: false,
        };
        v.players.extend([cpu(2, 0), cpu(3, 4), cpu(4, 9)]);
        v.players.push(PublicPlayer {
            id: ParticipantId(5),
            name: Some("dan".into()),
            kind: PublicKind::Person,
            in_match: false,
            out_for_match: false,
            left: true,
        });
        v
    }

    /// ui-screens-3: a CPU's stars are its shown level, from 1, as the
    /// members' panels draw them (the view carries the engine's index).
    #[test]
    fn the_audience_sees_a_cpu_at_the_level_the_members_see() {
        let levels: Vec<Option<u8>> = player_rows(&view()).iter().map(|r| r.level).collect();
        assert_eq!(levels, [None, Some(1), Some(5), Some(10)]);
    }

    /// ui-screens-11: the count is of the players listed, not of those
    /// who left.
    #[test]
    fn the_audience_counts_the_players_it_lists() {
        let v = view();
        assert_eq!(player_rows(&v).len(), 4);
        assert_eq!(players_count(&v), String::from(m::audience_players(4)));
    }
}
