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
    /// The part shown beside the card's lines: `players` or `standings`
    /// (the tabs are there once a stage has settled).
    /// The song the room picked for the stage it is on, where the page
    /// shows it (its title), and whether its banner is drawn (`banner`)
    /// or the plain plate (`plate`).
    pub song: Option<String>,
    pub banner: Option<String>,
    pub tab: String,
    pub tabs: bool,
    /// The last settled stage's standings, as a member's results say
    /// theirs (`ThemeState::stage`), once a stage has settled.
    pub table: Option<super::results::StageView>,
    /// The group's draw revealed over the page now (round RL4b): whose
    /// choice it took and its song's title.
    pub reveal: Option<[String; 2]>,
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
        song: picked_song(a).map(|s| s.title),
        banner: picked_song(a).map(|s| if s.banner.is_some() { "banner" } else { "plate" }.into()),
        tab: "players".into(),
        tabs: false,
        table: None,
        reveal: a.reveal().and_then(|r| {
            let v = v?;
            let won = r.what.entries.get(usize::from(r.what.winner))?;
            Some([player_name(v, won.participant), won.card.title.clone()])
        }),
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

/// The public view's names for a standings' table, and nobody's own line:
/// the audience's.
pub(crate) struct PublicWho<'a>(pub &'a PublicView);

impl super::results::TableWho for PublicWho<'_> {
    fn label(&self, id: ddi_room::ParticipantId) -> (String, Option<u8>) {
        let level = self
            .0
            .players
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| match p.kind {
                PublicKind::Cpu { level, .. } => Some(super::room::shown_level(level)),
                _ => None,
            });
        (player_name(self.0, id), level)
    }

    fn me(&self) -> Option<ddi_room::ParticipantId> {
        None
    }

    fn badge(&self) -> bool {
        false
    }
}

/// The part beside the card's lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Part {
    #[default]
    Players,
    Standings,
}

const TAB_PLAYERS: &str = "Tab.Players";
const TAB_STANDINGS: &str = "Tab.Standings";
/// The tabs' row's height and the room under it.
const TABS_H: f32 = 32.0;
const TABS_GAP: f32 = 10.0;
/// The standings' width from which every column shows (`results_stage`'s
/// `columns`), which the card's lines make room for where they can.
const TABLE_FULL_W: f32 = 724.0;
/// The table's mark for the lines hidden, as a button.
const TABLE_DOWN: &str = "Audience.TableDown";
const TABLE_UP: &str = "Audience.TableUp";

pub(super) fn player_name(v: &PublicView, id: ddi_room::ParticipantId) -> String {
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
    // The group's draw being revealed (round RL4b): its heading, never its
    // result before the reel stops.
    if let Some(r) = a.reveal()
        && !v.closed
    {
        return m::draw_heading(number(v, r.what.stage));
    }
    let song = |stage: ddi_room::StageId| {
        v.stages
            .iter()
            .find(|s| s.id == stage)
            .and_then(|s| s.card.as_ref())
            .map(|c| UiText::data(format!("{} — {}", c.title, c.artist)).to_string())
    };
    let picked = |stage: ddi_room::StageId| {
        v.stages
            .iter()
            .find(|s| s.id == stage)
            .is_some_and(|s| matches!(s.content, PublicContent::Available | PublicContent::Pending))
    };
    match &v.phase {
        _ if v.closed && a.closed_by_operator() => m::closed_by_operator(),
        _ if v.closed => m::invite_room_closed(),
        // The players choose the next song (revision 22): how many chose,
        // the draw's countdown.
        Phase::Lobby | Phase::Intermission { .. } | Phase::Preparing { window: None, .. }
            if v.rules.choice == ddi_room::SongChoice::Group
                && !matches!(v.phase, Phase::Preparing { stage, .. } if picked(stage)) =>
        {
            group_doing(a, v)
        }
        Phase::Lobby => m::audience_doing_lobby(),
        Phase::Preparing { stage, .. } | Phase::Intermission { next: stage } => {
            match (song(*stage), v.stages.iter().find(|s| s.id == *stage).map(|s| s.content)) {
                // Its song drawn from the players' choices: whose it was.
                (Some(_), Some(PublicContent::Available | PublicContent::Pending))
                    if let Some(line) = drawn_line(v, *stage) =>
                {
                    line
                }
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

/// Under Song choice: Group (revision 22), what the audience is told
/// while the players choose: the draw's countdown (its seconds on the
/// room's clock as the seat estimates it, how many chose, who started it),
/// else how many of the players chose.
fn group_doing(a: &AudienceClient, v: &PublicView) -> UiText {
    use crate::state::room::LiveRoom;
    let stage = match v.phase {
        Phase::Intermission { next } => Some(next),
        Phase::Preparing { stage, .. } => Some(stage),
        _ => v.stages.first().map(|s| s.id),
    };
    let n = stage.map_or(1, |st| number(v, st));
    let chose = v.choices.len();
    let of = v
        .players
        .iter()
        .filter(|p| !p.left && !p.out_for_match && !matches!(p.kind, PublicKind::Cpu { .. }))
        .count()
        .max(chose);
    match &v.draw {
        Some(d) => {
            let seconds = a
                .room_ms_at(a.now_ms())
                .map_or(0, |now| d.ends_at.0.saturating_sub(now.0).div_ceil(1000));
            m::status_draw(seconds, chose, of, UiText::data(player_name(v, d.by)))
        }
        None => m::status_group_watching(chose, of, n),
    }
}

/// Whose choice stage `stage`'s song was, where the players' draw picked
/// it (revision 22).
fn drawn_line(v: &PublicView, stage: ddi_room::StageId) -> Option<UiText> {
    let d = v.drawn.as_ref().filter(|d| d.stage == stage)?;
    let won = d.entries.get(usize::from(d.winner))?;
    let chart = UiText::data(format!("{} {}", won.card.difficulty, won.card.meter));
    let who = UiText::data(player_name(v, won.participant));
    // Under Adaptive the song alone (round OC).
    if v.rules.charts == ddi_room::Charts::Adaptive {
        return Some(m::draw_won_song(who, UiText::data(&won.card.title)));
    }
    Some(m::draw_won(who, UiText::data(&won.card.title), chart))
}

/// The song the room picked for the stage it is on, as the page shows
/// it: its title and artist from the stage's card, and its banner (a
/// texture source) once the seat read the room's share of it.
struct PickedSong {
    title: String,
    artist: String,
    banner: Option<String>,
}

/// The stage the room is on has a song picked ([`PickedSong`]): from the
/// pick until the stage is over. Nothing before the pick: what the
/// picker rests on is not the audience's to see.
fn picked_song(a: &AudienceClient) -> Option<PickedSong> {
    let v = a.view()?;
    // Not while the group's draw is revealed: the reel says it (round
    // RL4b).
    if v.closed || a.reveal().is_some() {
        return None;
    }
    let stage = a.current_stage()?;
    let st = v.stages.iter().find(|s| s.id == stage)?;
    if !matches!(st.content, PublicContent::Available | PublicContent::Pending) {
        return None;
    }
    let card = st.card.as_ref()?;
    Some(PickedSong {
        title: UiText::data(&card.title).into(),
        artist: UiText::data(&card.artist).into(),
        banner: a.banner(stage).map(str::to_string),
    })
}

/// The picked song's banner's height and shape (a banner's usual 418 by
/// 164).
const BANNER_H: f32 = 56.0;
const BANNER_ASPECT: f32 = 418.0 / 164.0;

/// One line of the audience's card: its text and how it reads.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Tone {
    /// The picked song's banner (or its plain plate) with its title and
    /// artist beside it: no text of its own.
    Song,
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
            if picked_song(a).is_some() {
                out.push((String::new(), Tone::Song));
            }
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
    rows_charted(v, &|_| None)
}

/// [`player_rows`] with each player's chart for the stage the room is on
/// under Charts: Adaptive, as the members' Players pop-up shows it (round
/// LO, `AudienceClient::start_shown`): its level number in its
/// difficulty's color beside the name; none for one out of the match, or
/// not in the lineup once the start is committed.
pub(crate) fn player_rows_of(a: &AudienceClient) -> Vec<super::room::PlayerRow> {
    let Some(v) = a.view() else {
        return Vec::new();
    };
    let stage = a.current_stage();
    let lineup = stage
        .and_then(|id| v.stages.iter().find(|s| s.id == id))
        .and_then(|s| s.play.as_ref())
        .map(|p| p.lineup.clone());
    rows_charted(v, &|p| {
        if lineup.as_ref().is_some_and(|l| !l.contains(&p)) {
            return None;
        }
        let c = a.start_shown(stage?, p)?;
        Some((c.meter, c.difficulty))
    })
}

fn rows_charted(
    v: &PublicView,
    chart_of: &dyn Fn(ddi_room::ParticipantId) -> Option<(u32, ddi_chart::Difficulty)>,
) -> Vec<super::room::PlayerRow> {
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
            // Under Song choice: Group (round RL4b): the song this player
            // chose, as everyone in the room sees it.
            let chose: String = v
                .choices
                .iter()
                .find(|c| c.participant == p.id)
                .map(|c| m::audience_chose(UiText::data(&c.card.title)).into())
                .unwrap_or_default();
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
                role: chose,
                status,
                tone: if p.out_for_match {
                    RowTone::Away
                } else {
                    RowTone::Plain
                },
                policy: String::new(),
                chart: if p.out_for_match { None } else { chart_of(p.id) },
                id: None,
                manage: false,
                kick: false,
                marks: Default::default(),
                bot: false,
                icon: if p.out_for_match {
                    super::room::StateIcon::Out
                } else {
                    super::room::StateIcon::Idle
                },
            }
        })
        .collect();
    super::room::panel_order(rows)
}

/// What the debug view was published for: the client's version, the
/// counts that move without it, the part shown and its first line.
type Published = (u64, u32, Option<u32>, u32, u64, Part, usize);

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
///
/// Since round D3 (`audience-page-v2`): the song picked for the stage
/// the room is on has its banner beside its title (a plain plate until
/// the seat read the room's share of it, [`picked_song`]); once a stage
/// settled, the players' part has two tabs, Players and Standings, the
/// second the last settled stage's standings as the members' results
/// draw them (`results::stage_standings` over
/// [`AudienceClient::stage_table`]), brought up as each stage settles;
/// and a seat that watched no stage has the last played stage's picture
/// behind the card where its share is still read
/// (`AudienceClient::sync_picture`).
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
    /// The part shown, and the settled stage it was last chosen for (a
    /// stage that settles brings its standings up, once).
    part: Part,
    settled: Option<ddi_room::StageId>,
    /// The tabs are there (a stage has settled).
    tabs: bool,
    /// The standings' first line, the lines shown and those there are
    /// when last laid out, its width, its lines' starters and the one
    /// the pointer is over.
    table_first: usize,
    table_shown: (usize, usize, f32),
    table_lines: Vec<ddi_room::ParticipantId>,
    pointed: Option<ddi_room::ParticipantId>,
    /// The client's version the card was built for, and the screen it
    /// was laid out for (laid out again when the window changes shape:
    /// a phone turned, or the page's first size coming after its first
    /// frame, as after a watched stage on the web).
    built: Option<u64>,
    laid: Option<ddi_actor::screen::VirtualScreen>,
    /// The countdown's seconds as built: the card is built again as they
    /// change.
    seconds: Option<u64>,
    /// What the debug view was last published for.
    published: Option<Published>,
    /// Watch was offered when last laid out (it is focused as it comes).
    offered: bool,
    /// What is focused, as the screen was last told, and whether the
    /// footer says Enter watches the stage.
    focused: Option<String>,
    hinted_watch: bool,
    time: f64,
    /// The rain's clock (the screens' wall clock; still under reduced
    /// motion) and whether it moves.
    rain_time: f64,
    rain_animating: bool,
    /// The group's last draw revealed over the page (round RL4b).
    revealed: Option<(ddi_room::StageId, ddi_room::Millis)>,
}

impl AudienceScreen {
    /// The seat's run: ticked on the shell's wall clock, its stages'
    /// songs and a sign-in's redirect handed to the shell.
    fn run_seat(&mut self, cx: &mut Cx) {
        let now = cx.wall_ms();
        let (jobs, redirect) = {
            let mut a = self.audience.borrow_mut();
            a.tick(now);
            let redirect = a.online().borrow_mut().take_redirect();
            (a.take_load_jobs(), redirect)
        };
        for job in jobs {
            (self.shell)(AudienceCall::Prepare(job), cx);
        }
        if let Some(r) = redirect {
            (self.shell)(AudienceCall::Redirect(r), cx);
        }
    }

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
            part: Part::Players,
            settled: None,
            tabs: false,
            table_first: 0,
            table_shown: (0, 0, 0.0),
            table_lines: Vec::new(),
            pointed: None,
            built: None,
            laid: None,
            seconds: None,
            published: None,
            offered: false,
            focused: None,
            hinted_watch: false,
            time: 0.0,
            rain_time: 0.0,
            rain_animating: false,
            revealed: None,
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
        self.laid = Some(vp);
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
        // The last settled stage's standings, as the members' results
        // show them: a second part beside the players', brought up once
        // as a stage settles.
        let table = a.settled_stage().and_then(|s| a.stage_table(s));
        let settled = table.as_ref().map(|t| t.stage);
        if settled != self.settled {
            self.settled = settled;
            self.table_first = 0;
            self.part = if settled.is_some() { Part::Standings } else { Part::Players };
        }
        self.tabs = table.is_some();
        let w = match (split, &table) {
            // The lines give the table the room for all its columns
            // where the card has it.
            (true, Some(_)) => (full_w - 24.0 - TABLE_FULL_W)
                .clamp(340.0, classic)
                .min(full_w - 294.0),
            (true, None) => classic.min(full_w - 294.0),
            (false, _) => full_w,
        };
        let inner = w - 48.0;
        let mut c: Vec<Actor> = Vec::new();
        let mut y = 24.0;
        for (k, (text, tone)) in lines(&a).into_iter().enumerate() {
            if tone == Tone::Song {
                let Some(song) = picked_song(&a) else {
                    continue;
                };
                // The banner once the share's listing was read; the
                // plain plate meanwhile and for a song without one.
                let bw = (BANNER_H * BANNER_ASPECT).min(inner * 0.45);
                let bh = bw / BANNER_ASPECT;
                y += 6.0;
                let mid = y + bh / 2.0;
                let mut plate = crate::look::quad_at(
                    "Audience.BannerPlate",
                    [bw, bh],
                    crate::look::with_alpha(env.color("Text"), 0.07),
                    [24.0 + bw / 2.0, mid],
                );
                crate::look::call(&mut plate, "cornerradius", &[6.0]);
                c.push(plate);
                if let Some(source) = &song.banner {
                    let mut sprite = crate::look::named(
                        ddi_actor::Kind::Sprite(ddi_actor::Sprite::quad([bw, bh])),
                        "Audience.Banner",
                    );
                    if let ddi_actor::Kind::Sprite(sp) = &mut sprite.kind {
                        sprite_texture(sp, cx.texture(source, bw));
                    }
                    crate::look::call(&mut sprite, "xy", &[24.0 + bw / 2.0, mid]);
                    crate::look::call(&mut sprite, "cornerradius", &[6.0]);
                    c.push(sprite);
                }
                let tx = 24.0 + bw + 12.0;
                let room = (inner - bw - 12.0).max(40.0);
                let mut title = env.text("Audience.Song", &song.title, 15.0, 600, "Text");
                crate::look::ellipsized(&mut title, room);
                let mut artist = env.text("Audience.Artist", &song.artist, 13.0, 400, "Muted");
                crate::look::ellipsized(&mut artist, room);
                if song.artist.is_empty() {
                    c.push(left(title, tx, mid));
                } else {
                    c.push(left(title, tx, mid - 10.0));
                    c.push(left(artist, tx, mid + 10.0));
                }
                y += bh + 8.0;
                continue;
            }
            let (size, weight, color) = match tone {
                Tone::Song => continue,
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
        let rows = player_rows_of(&a);
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
        let part_shown = self.part;
        let (table_first, pointed) = (self.table_first, self.pointed);
        let mut table_laid = None;
        let mut place_part = |c: &mut Vec<Actor>, [ax, ay, aw, ah]: [f32; 4]| -> f32 {
            let mut top = ay;
            if let Some(t) = &table {
                // The tabs, and which stage the standings are of.
                let mut tx = ax;
                let tabs = [
                    (Part::Players, TAB_PLAYERS, String::from(m::players())),
                    (Part::Standings, TAB_STANDINGS, m::results_tab_standings().into()),
                ];
                for (part, name, label) in tabs {
                    let bw = (crate::look::mono_width(&label, 13.0) + 32.0).max(104.0);
                    let mut b = crate::look::component_or(
                        &*env.look,
                        "StyleTab",
                        "Button",
                        &Spec::new([bw, TABS_H], label.as_str()).with_items([String::new()]),
                    );
                    b.name = name.into();
                    let bw = super::extent(&mut b, [bw, TABS_H])[0];
                    crate::look::call(&mut b, "xy", &[tx + bw / 2.0, top + TABS_H / 2.0]);
                    crate::widgets::play(
                        &mut b,
                        if part == part_shown { "Selected" } else { "Available" },
                    );
                    b.finish_tweening();
                    c.push(b);
                    tx += bw + 8.0;
                }
                if part_shown == Part::Standings {
                    let title = m::table_title(usize::from(t.number), usize::from(t.of_shown()));
                    let mut tt = crate::look::tracked(
                        env.text("Audience.TableTitle", &title, 11.0, 600, "Faint"),
                        120,
                    );
                    crate::look::ellipsized(&mut tt, (ax + aw - tx).max(0.0));
                    if ax + aw - tx >= 60.0 {
                        c.push(super::right(tt, ax + aw, top + TABS_H / 2.0));
                    }
                }
                top += TABS_H + TABS_GAP;
            }
            if let (Part::Standings, Some(t), Some(v)) = (part_shown, &table, a.view()) {
                if t.rules.changed {
                    // Played under rules changed during the match: said,
                    // with those rules, as the members' table says it.
                    let note = m::table_rules_changed(super::room::rules_words_in(
                        &t.rules,
                        Some(t.attacks),
                        t.adaptive_offered,
                    ));
                    let mut n = env.text("Audience.TableNote", &note, 12.0, 400, "Gold");
                    crate::look::ellipsized(&mut n, aw);
                    c.push(left(n, ax, top + 8.0));
                    top += 20.0;
                }
                let drawn = super::results::stage_standings(
                    &env,
                    &PublicWho(v),
                    t,
                    [ax, top, aw],
                    Some((ah - (top - ay)).max(0.0)),
                    pointed,
                    table_first,
                    c,
                );
                if let Some((my, mh, below)) = drawn.more {
                    // The mark takes a tap: a page on (or back).
                    let mut mark = crate::look::quad_at(
                        if below { TABLE_DOWN } else { TABLE_UP },
                        [aw, mh],
                        crate::look::with_alpha(env.color("Text"), 0.012),
                        [ax + aw / 2.0, my],
                    );
                    crate::look::call(&mut mark, "cornerradius", &[6.0]);
                    c.push(mark);
                }
                table_laid = Some((drawn.first, drawn.shown, aw));
                return top - ay + drawn.height;
            }
            let part = super::room::players_part(
                &env,
                &rows,
                &names,
                (&words.0, &words.1),
                self.first,
                [ax, top, aw, (ah - (top - ay)).max(0.0)],
            );
            self.first = part.first;
            self.shown = (rows.len(), part.shown, part.per);
            c.extend(part.actors);
            top - ay + part.height
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
        if let Some((first, shown, tw)) = table_laid {
            self.table_first = first;
            self.table_shown = (shown, table.as_ref().map_or(0, |t| t.rows.len()), tw);
        }
        self.table_lines = table
            .as_ref()
            .map(|t| t.rows.iter().map(|row| row.participant).collect())
            .unwrap_or_default();
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
            // Each key's own action, once: Enter the focused button's
            // (Watch, while it is offered and focused, as it is first),
            // Esc Stop watching; never Stop watching twice (Enter on its
            // button is the button's own label).
            let (watch, leave) = (m::audience_watch_stage(), m::audience_leave());
            let offered = self.buttons.first() == Some(&WATCH_BUTTON);
            let on_watch = offered && self.focused.as_deref().is_none_or(|f| f == WATCH_BUTTON);
            let mut keys: Vec<(&[&str], &str)> = Vec::new();
            let mut pads: Vec<(&[HintAction], &str)> = Vec::new();
            if on_watch {
                keys.push((&["ENTER"], watch.as_str()));
                pads.push((&[HintAction::Confirm, HintAction::Start], watch.as_str()));
            }
            keys.push((&["ESC"], leave.as_str()));
            pads.push((&[HintAction::Back], leave.as_str()));
            self.hinted_watch = on_watch;
            all.push(footer(&env, self.screen, &keys, &pads, "", &[], cx));
        }
        let mut root = ddi_actor::Actor::new(ddi_actor::Kind::Frame(all));
        super::transitions(&mut root, &env, &[("Audience", super::Edge::None)]);
        if self.zoom != 1.0 {
            crate::look::call(&mut root, "zoom", &[self.zoom]);
        }
        root
    }

    /// What the page has to press, for the debug state (its buttons
    /// first, then the tabs and the table's mark where they are).
    fn items(&self) -> Vec<String> {
        let mut items: Vec<String> = self.buttons.iter().map(|b| b.to_string()).collect();
        if self.tabs {
            items.extend([TAB_PLAYERS, TAB_STANDINGS, TABLE_DOWN, TABLE_UP].map(String::from));
        }
        items
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx) {
        let left = crate::hit::find_mut(root, "Audience").map_or(0.0, |c| c.tween_time_left());
        *root = self.tree(cx);
        super::relaid(root, left, cx);
        let st = &mut self.env.state.borrow_mut();
        st.page_items = self.items();
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
            self.part,
            self.table_first,
        );
        if self.published == Some(key) {
            return;
        }
        self.published = Some(key);
        let mut view = debug_view(&a);
        view.tabs = self.tabs;
        view.tab = match self.part {
            Part::Players => "players",
            Part::Standings => "standings",
        }
        .into();
        view.table = a.view().and_then(|v| {
            let table = a.stage_table(a.settled_stage()?)?;
            let w = match self.table_shown.2 {
                w if w > 0.0 => w,
                _ => TABLE_FULL_W,
            };
            Some(super::results::stage_view_of(
                &PublicWho(v),
                &table,
                super::results::Tab::Standings,
                w,
            ))
        });
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

    /// The part shown scrolled by `by` lines (`pages`: by so many of
    /// its pages), where its lines do not all fit.
    /// Whether it moved.
    fn scroll(&mut self, by: i32, pages: i32, root: &mut Actor, cx: &mut Cx) -> bool {
        let moved = if self.part == Part::Standings && self.tabs {
            let (shown, n, _) = self.table_shown;
            let by = by + pages * shown.max(1) as i32;
            let to = super::scrolled(self.table_first, by, n, shown);
            std::mem::replace(&mut self.table_first, to) != to
        } else {
            let (n, shown, per) = self.shown;
            let by = by + pages * per.max(1) as i32;
            let to = super::scrolled(self.first, by, n, shown);
            std::mem::replace(&mut self.first, to) != to
        };
        self.rebuild(root, cx);
        self.env_publish();
        moved
    }

    /// The part beside the lines changes (a tab, the shoulders).
    fn set_part(&mut self, part: Part, root: &mut Actor, cx: &mut Cx) {
        if !self.tabs || self.part == part {
            return;
        }
        self.part = part;
        self.rebuild(root, cx);
        self.env_publish();
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
        self.env.state.borrow_mut().page_items = self.items();
        self.env_publish();
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v: Vec<Focusable> = self.buttons.iter().map(|b| Focusable::new(*b)).collect();
        // The tabs over them: Up from a button, Left and Right between
        // the two, Down back to the buttons.
        let tab = match self.part {
            Part::Players => TAB_PLAYERS,
            Part::Standings => TAB_STANDINGS,
        };
        if self.tabs {
            for b in &mut v {
                b.up = Some(tab.into());
            }
        }
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
        if self.tabs {
            let down = self.buttons.first().map(|b| b.to_string());
            v.push(Focusable {
                right: Some(TAB_STANDINGS.into()),
                down: down.clone(),
                ..Focusable::new(TAB_PLAYERS)
            });
            v.push(Focusable {
                left: Some(TAB_PLAYERS.into()),
                down,
                ..Focusable::new(TAB_STANDINGS)
            });
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

    /// A step while the page is hidden (a covered or minimized window):
    /// the seat run as a frame runs it (its socket read, a lost one's
    /// ticket asked for, a stage's song prepared), so that it keeps its
    /// seat and has the stage ready when the page is seen again, as a
    /// member's room is run on a hidden page (round Y3). A stage the room
    /// starts meanwhile is handed over once the page is seen (watching
    /// starts from where the stage is; its music needs the page).
    fn unseen(&mut self, cx: &mut Cx) {
        self.run_seat(cx);
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        self.time = cx.time();
        // The seat's socket and the room's view come while nothing moves.
        self.run_seat(cx);
        let order = self.audience.borrow_mut().take_stage_order();
        let changed = {
            let a = self.audience.borrow();
            self.built != Some(a.version())
                || self.seconds != countdown_seconds(&a)
                || self.laid != Some(cx.viewport().screen)
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
        // The group's draw (round RL4b): the members' reel over the page.
        super::room_reveal::open_audience_reveal(&self.env, &self.audience, &mut self.revealed, cx);
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
            Input::Activate { name } if name == TAB_PLAYERS || name == TAB_STANDINGS => {
                let part = if name == TAB_STANDINGS { Part::Standings } else { Part::Players };
                self.set_part(part, root, cx);
                Handled::Yes
            }
            // The shoulders change the part, as a stage's results' tabs.
            Input::Menu {
                action: action @ (MenuAction::Previous | MenuAction::Next),
                ..
            } if self.tabs => {
                let part = if *action == MenuAction::Next { Part::Standings } else { Part::Players };
                self.set_part(part, root, cx);
                Handled::Yes
            }
            Input::Activate { name }
                if ["Audience.Down", "Audience.Up", TABLE_DOWN, TABLE_UP].contains(&name.as_str()) =>
            {
                let down = name == "Audience.Down" || name == TABLE_DOWN;
                self.scroll(0, if down { 1 } else { -1 }, root, cx);
                Handled::Yes
            }
            // Up and Down scroll the part shown a line (a controller's
            // D-pad and stick, as the keys: the page has no other way to
            // scroll with one), as a pop-up's rows scroll for someone with
            // no row to move to: from a tab focused where the page has
            // tabs (Down past the last line going on to the buttons, as
            // it does where everything fits), from its buttons where it
            // has none (nothing is above or below them).
            Input::Menu {
                action: action @ (MenuAction::Up | MenuAction::Down),
                ..
            } if !self.tabs
                || self
                    .focused
                    .as_deref()
                    .is_some_and(|f| f == TAB_PLAYERS || f == TAB_STANDINGS) =>
            {
                let by = if *action == MenuAction::Down { 1 } else { -1 };
                if self.scroll(by, 0, root, cx) {
                    Handled::Yes
                } else {
                    Handled::No
                }
            }
            Input::Wheel { delta, .. } if delta[1] != 0.0 => {
                self.scroll(super::wheel_step(*delta), 0, root, cx);
                Handled::Yes
            }
            Input::Key(k) if k.down && matches!(k.code.as_str(), "PageDown" | "PageUp") => {
                self.scroll(0, if k.code == "PageDown" { 1 } else { -1 }, root, cx);
                Handled::Yes
            }
            // The footer's Enter is the focused button's.
            Input::Focus { to, .. } => {
                self.focused = to.clone();
                let on_watch = self.buttons.first() == Some(&WATCH_BUTTON)
                    && to.as_deref().is_none_or(|f| f == WATCH_BUTTON);
                if on_watch != self.hinted_watch {
                    self.rebuild(root, cx);
                }
                Handled::No
            }
            // The line the pointer is over: its exact counts under the
            // table where the lines have no second line for them.
            Input::Hover { target } if self.tabs && self.part == Part::Standings => {
                let row = target.as_ref().and_then(|t| {
                    t.chain
                        .iter()
                        .find_map(|n| n.strip_prefix("Stand")?.parse::<usize>().ok())
                });
                let pointed = row.and_then(|k| self.table_lines.get(k).copied());
                if self.pointed != pointed {
                    self.pointed = pointed;
                    self.rebuild(root, cx);
                }
                Handled::No
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

    /// Round RL4 (revision 22): a stage whose song the players' draw
    /// picked says whose choice it was, as the members' reveal does.
    #[test]
    fn the_audience_is_told_whose_choice_was_drawn() {
        use ddi_room::audience::public::{PublicChoice, PublicDrawn};
        let mut v = view();
        let stage = v.stages[0].id;
        assert!(drawn_line(&v, stage).is_none());
        let card = |title: &str| ddi_room::ContentCard {
            title: title.into(),
            difficulty: "Hard".into(),
            meter: 9,
            ..Default::default()
        };
        v.drawn = Some(PublicDrawn {
            stage,
            entries: vec![
                PublicChoice { participant: ParticipantId(1), card: card("Beatucada") },
                PublicChoice { participant: ParticipantId(5), card: card("Sweeteners") },
            ],
            winner: 1,
            at: ddi_room::Millis(0),
        });
        assert_eq!(
            String::from(drawn_line(&v, stage).unwrap()),
            "dan's choice: Sweeteners · Hard 9"
        );
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
