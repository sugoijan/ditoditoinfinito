//! A multiplayer stage's results (feedback round B,
//! `docs/plans/multiplayer-prototype-progress.md`): where a play's four
//! graphs are, two tabs.
//!
//! - **Standings** (shown first): every starter of the stage on a line,
//!   in the stage's order once the room confirmed it: the place, the name
//!   (You on a quiet plate, a bot labeled, a CPU's stars), the points the
//!   stage awarded, the match total after it and the place in the match
//!   (a quiet mark where it moved), and how they performed: the ruleset's
//!   score or percent (as the player's settings show it), the grade, life
//!   at the end or when they went out, the longest combo and their
//!   judgments as one thin stacked bar in the ruleset's colors, the exact
//!   counts on a second line where the rows have room and for the row the
//!   pointer is over (the local player's otherwise) where they have not;
//!   with attacks, those launched and the shocks stepped on. Before the
//!   room confirms (an online stage whose starters are still playing),
//!   the lines are as seated and fill in as reports come. After the
//!   match's last stage its result is on top: the winner or winners, then
//!   everyone's final place and total.
//! - **Graphs**: the four graphs, as a play's results have them.
//!
//! Left and Right (a keyboard's), the shoulders, a tap or the focus walk
//! change the tab; the one chosen is remembered for the session (never
//! saved), the last stage's results opening on the standings whatever it
//! was. A phone shows fewer columns, dropped in this order as the width
//! runs out: the judgments, the combo, life, the place in the match, the
//! grade; its text is never made smaller.
//!
//! The room is read, never changed: what a line shows is
//! [`crate::state::room::StageTable`]. The screen keeps the room's
//! messages coming while it shows (an online room's socket), and once the
//! standings showed the confirmed stage the room's song select does not
//! pop them up again.
//!
//! A child module of `results` (it reads the screen's own fields).

use std::cell::Cell;

use ddi_l10n::msg::room as m;
use ddi_room::{Finish, ParticipantId, Run, StageId};
use serde::Serialize;

use super::*;
use crate::screens::play::Tier;
use crate::screens::room::{cpu_level, cpu_name, person_name, points_text};
use crate::state::history::list_grade;
use crate::state::room::{RoomClient, SharedRoom, StageRow, StageTable};
use crate::state::settings::Settings;

/// The tabs' row's height, and the room under it.
const TABS_H: f32 = 32.0;
const TABS_GAP: f32 = 12.0;
/// A line's height at most, at least, and from where it has a second
/// line (the judgments' counts).
const ROW_MAX: f32 = 46.0;
const ROW_MIN: f32 = 26.0;
const ROW_TWO: f32 = 42.0;
/// A phone's line.
const ROW_COMPACT: f32 = 30.0;
/// The column headings' height, and the pointed line's counts' under the
/// table.
const HEAD_H: f32 = 22.0;
const DETAIL_H: f32 = 22.0;

/// The two tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    #[default]
    Standings,
    Graphs,
}

thread_local! {
    /// The tab last chosen: for the session, never saved.
    static CHOSEN: Cell<Tab> = const { Cell::new(Tab::Standings) };
}

/// Forgets the tab chosen (tests: each starts as a session does).
#[cfg(test)]
pub fn forget_tab() {
    CHOSEN.set(Tab::Standings);
}

/// The results' stage: its room, the tab shown and what was drawn.
pub(super) struct StageMode {
    room: SharedRoom,
    stage: StageId,
    pub(super) tab: Tab,
    /// The room's version the standings were drawn for.
    seen: Option<u64>,
    /// The line the pointer is over.
    pointed: Option<ParticipantId>,
    /// The first line shown, where they do not all fit.
    first: usize,
    /// The UI's clock at the last update (the room is looked at ten times
    /// a second while nothing moves).
    time: f64,
    /// The local player watched the stage (`watch-v0`): no play of their
    /// own, so the stage's winner where their judgments would be and no
    /// Graphs tab (there are none of theirs to draw).
    pub(super) watched: bool,
    /// The game is quitting once the room has answered the player's
    /// leaving, or at this time on the UI's clock.
    quit_at: Option<f64>,
}

/// The stage's standings as the debug state shows them
/// (`ThemeState::stage`), for tests.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct StageView {
    pub tab: Tab,
    pub number: u16,
    pub of: u16,
    /// The room confirmed the stage: places and awards are final.
    pub confirmed: bool,
    /// The columns shown, left to right.
    pub columns: Vec<String>,
    pub rows: Vec<StageRowView>,
    /// The local player's battle summary's lines, where the stage had
    /// attacks.
    pub summary: Vec<String>,
    /// The match's winners and everyone's final place and total, on the
    /// last stage once the match is over.
    pub winners: Vec<String>,
    pub result: Vec<(String, u16, String)>,
    /// The local player watched the stage: its winner or winners as the
    /// results' left side names them (no play of their own).
    pub watched: bool,
    pub stage_winners: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct StageRowView {
    pub id: u64,
    pub name: String,
    pub me: bool,
    /// A CPU's level, from 1.
    pub level: Option<u8>,
    pub place: Option<u16>,
    pub award: Option<String>,
    pub total: Option<String>,
    pub rank: Option<u16>,
    /// The place in the match moved: `up`, `down` or none.
    pub moved: Option<String>,
    /// Their result has come (always, once confirmed).
    pub reported: bool,
    pub score: String,
    pub grade: String,
    pub life: String,
    pub combo: Option<u32>,
    pub tiers: Vec<u32>,
    /// Where the battle has attacks: those they launched, those sent
    /// against them and the shocks they stepped on.
    pub attacks: Option<(u32, u32, u32)>,
}

/// A column of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Col {
    Award,
    Total,
    Match,
    Score,
    Grade,
    Life,
    Combo,
    Judgments,
    /// With attacks: those launched, those received, the shocks stepped
    /// on.
    Sent,
    Taken,
    Shocks,
}

impl Col {
    fn name(self) -> &'static str {
        match self {
            Col::Award => "award",
            Col::Total => "total",
            Col::Match => "match",
            Col::Score => "score",
            Col::Grade => "grade",
            Col::Life => "life",
            Col::Combo => "combo",
            Col::Judgments => "judgments",
            Col::Sent => "sent",
            Col::Taken => "taken",
            Col::Shocks => "shocks",
        }
    }

    fn heading(self) -> UiText {
        match self {
            Col::Award => m::table_award(),
            Col::Total => m::table_total(),
            Col::Match => m::table_match(),
            Col::Score => m::table_score(),
            Col::Grade => m::table_grade(),
            Col::Life => m::table_life(),
            Col::Combo => m::table_combo(),
            Col::Judgments => m::table_judgments(),
            Col::Sent => m::table_sent(),
            Col::Taken => m::table_taken(),
            Col::Shocks => m::table_shocks(),
        }
    }

    /// Its width; with the three attack columns beside them, life and
    /// the judgments' strip a little narrower, for the names' room.
    fn width_with(self, attacks: bool) -> f32 {
        match self {
            Col::Life if attacks => 70.0,
            Col::Judgments if attacks => 72.0,
            _ => self.width(),
        }
    }

    fn width(self) -> f32 {
        match self {
            Col::Award => 54.0,
            Col::Total => 50.0,
            Col::Match => 52.0,
            Col::Score => 82.0,
            Col::Grade => 52.0,
            Col::Life => 80.0,
            Col::Combo => 52.0,
            Col::Judgments => 96.0,
            Col::Sent => 34.0,
            Col::Taken => 40.0,
            Col::Shocks => 46.0,
        }
    }
}

/// The columns a table `w` wide shows after the place and the name: all
/// of them where there is room, else without (in this order) the
/// judgments, the combo, life, the place in the match and the grade.
/// With `attacks`, three small ones more at the end (attacks launched,
/// attacks received, shocks stepped on), which take the combo's place
/// (the widest table has not the room for both beside the names) and go
/// first where the table is narrow.
fn columns(w: f32, attacks: bool) -> Vec<Col> {
    let all = [
        Col::Award,
        Col::Total,
        Col::Match,
        Col::Score,
        Col::Grade,
        Col::Life,
        Col::Combo,
        Col::Judgments,
        Col::Sent,
        Col::Taken,
        Col::Shocks,
    ];
    let dropped = [
        (Col::Sent, 700.0),
        (Col::Taken, 700.0),
        (Col::Shocks, 700.0),
        (Col::Judgments, 700.0),
        (Col::Combo, 640.0),
        (Col::Life, 560.0),
        (Col::Match, 480.0),
        (Col::Grade, 400.0),
    ];
    all.into_iter()
        .filter(|c| !dropped.iter().any(|(d, least)| d == c && w < *least))
        .filter(|c| match c {
            Col::Sent | Col::Taken | Col::Shocks => attacks,
            Col::Combo => !(attacks && w >= 700.0),
            _ => true,
        })
        .collect()
}

/// The local player's battle summary of the stage, in lines (where it
/// had attacks and was played here): whom their attacks went to and how
/// many, who attacked them, the shocks they stepped on, and a final
/// attack's draw.
pub(super) fn summary_lines(r: &RoomClient, table: &StageTable) -> Vec<String> {
    let Some(s) = table.summary.as_ref().filter(|_| table.attacks) else {
        return Vec::new();
    };
    let list = |of: &[(ParticipantId, u32)]| -> String {
        let names: Vec<String> = of
            .iter()
            .map(|(id, n)| {
                let name = name_of(r, *id).0;
                if *n > 1 {
                    format!("{name} ×{n}")
                } else {
                    name
                }
            })
            .collect();
        UiText::data(names.join(", ")).into()
    };
    let mut lines: Vec<String> = vec![
        if s.sent.is_empty() {
            m::summary_sent_none().into()
        } else {
            m::summary_sent(list(&s.sent)).into()
        },
        if s.taken.is_empty() {
            m::summary_taken_none().into()
        } else {
            m::summary_taken(list(&s.taken)).into()
        },
        m::summary_shocks(s.shocks).into(),
    ];
    if s.drawn_by_final_attack {
        lines.push(m::results_drawn_final_attack().into());
    }
    lines
}

/// A summary line's height.
const SUMMARY_LINE: f32 = 18.0;

/// How [`summary_lines`] flow into a table `w` wide: one after another
/// on a line while they fit, a dot between.
fn summary_flow(lines: &[String], w: f32) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in lines {
        match out.last_mut() {
            Some(last) if look::mono_width(last, 12.0) + look::mono_width(l, 12.0) + 24.0 <= w => {
                last.push_str("  ·  ");
                last.push_str(l);
            }
            _ => out.push(l.clone()),
        }
    }
    out
}

/// A participant's name as the lists show it (You, a bot labeled, a
/// CPU's own) and a CPU's level.
fn name_of(r: &RoomClient, id: ParticipantId) -> (String, Option<u8>) {
    match r.member_of(id) {
        Some(_) if id == r.me() => (m::you().into(), None),
        Some(mb) => match &mb.kind {
            ddi_room::MemberKind::Cpu { ordinal, opponent } => {
                (cpu_name(opponent, *ordinal), Some(cpu_level(opponent)))
            }
            ddi_room::MemberKind::Human { .. } => (person_name(mb), None),
        },
        None => (String::new(), None),
    }
}

/// A percent in hundredths as shown ("99.50%").
fn percent_text(hundredths: i32) -> String {
    let sign = if hundredths < 0 { "-" } else { "" };
    let p = hundredths.unsigned_abs();
    format!("{sign}{}.{:02}%", p / 100, p % 100)
}

/// A line's main number: the ruleset's score, or its percent where it
/// counts none or the player's settings show a DDI score as its percent.
fn score_of(row: &StageRow, percent: bool) -> String {
    let text = match &row.detail {
        Some(d) => match (d.score, d.percent) {
            (Some(score), Some(p)) if percent => {
                let _ = score;
                percent_text(p)
            }
            (Some(score), _) => score.to_string(),
            (None, Some(p)) => percent_text(p),
            (None, None) => String::new(),
        },
        None => match row.run {
            Some(Run::Live { accuracy, .. }) => format!("{:.2}%", accuracy * 100.0),
            _ => String::new(),
        },
    };
    UiText::data(text).into()
}

/// A line's life column: life at the end, when it ran out, or that they
/// left.
fn life_of(row: &StageRow) -> String {
    let left =
        row.placement.is_some_and(|p| p.finish == Finish::Forfeit) || row.run == Some(Run::Forfeit);
    let out = row
        .detail
        .as_ref()
        .and_then(|d| d.out_at_ms)
        .map(|ms| ms as f32 / 1_000.0)
        .or(match row.run {
            Some(Run::Out { at }) => Some(at as f32),
            _ => None,
        });
    let life = row
        .detail
        .as_ref()
        .map(|d| f64::from(d.life_ppm) / 1e6)
        .or(match row.run {
            Some(Run::Live { life, .. }) => Some(life),
            _ => None,
        });
    if let Some(at) = out {
        m::table_out(UiText::data(super::super::data::clock(at))).into()
    } else if left {
        m::table_left().into()
    } else if let Some(life) = life {
        UiText::data(format!("{:.0}%", life * 100.0)).into()
    } else {
        String::new()
    }
}

/// What a line shows, in words and numbers.
fn row_view(r: &RoomClient, table: &StageTable, row: &StageRow, percent: bool) -> StageRowView {
    let (name, level) = name_of(r, row.participant);
    let reported = table.confirmed || row.run.is_some() || row.detail.is_some();
    let moved = match (row.rank, row.rank_before) {
        (Some(now), Some(before)) if now < before => Some("up".to_string()),
        (Some(now), Some(before)) if now > before => Some("down".to_string()),
        _ => None,
    };
    let d = row.detail.as_ref();
    StageRowView {
        id: row.participant.0,
        name,
        me: row.participant == r.me(),
        level,
        place: row.placement.map(|p| p.place),
        award: row
            .placement
            .map(|p| m::table_award_value(points_text(p.points)).into()),
        total: row.total.map(|t| points_text(t).into()),
        rank: row.rank,
        moved,
        reported,
        score: if reported {
            score_of(row, percent)
        } else {
            String::new()
        },
        grade: d
            .map(|d| UiText::data(list_grade(&d.grade)).into())
            .unwrap_or_default(),
        life: if reported {
            life_of(row)
        } else {
            String::new()
        },
        combo: d.map(|d| d.max_combo),
        tiers: d.map(|d| d.tiers.clone()).unwrap_or_default(),
        attacks: d.filter(|_| table.attacks).and_then(|d| {
            Some((
                d.attacks?,
                d.received.unwrap_or(0),
                d.shocks.unwrap_or(0),
            ))
        }),
    }
}

/// The stage's standings as the debug state has them.
fn view_of(r: &RoomClient, table: &StageTable, tab: Tab, cols: &[Col]) -> StageView {
    let percent = shows_percent(table);
    let name = |id| name_of(r, id).0;
    let result = table.result.as_deref().unwrap_or_default();
    StageView {
        tab,
        number: table.number,
        of: table.of,
        confirmed: table.confirmed,
        columns: cols.iter().map(|c| c.name().to_string()).collect(),
        rows: table
            .rows
            .iter()
            .map(|row| row_view(r, table, row, percent))
            .collect(),
        summary: summary_lines(r, table),
        winners: result
            .iter()
            .filter(|(_, rank, _)| *rank == 1)
            .map(|(id, _, _)| name(*id))
            .collect(),
        result: result
            .iter()
            .map(|(id, rank, total)| (name(*id), *rank, points_text(*total).into()))
            .collect(),
        watched: false,
        stage_winners: stage_winners(r, table),
    }
}

/// The stage's winner or winners by name (first place, once the room
/// confirmed it), each with a CPU's level.
fn stage_winners_of(r: &RoomClient, table: &StageTable) -> Vec<(String, Option<u8>)> {
    table
        .rows
        .iter()
        .filter(|row| row.placement.as_ref().is_some_and(|p| p.place == 1))
        .map(|row| name_of(r, row.participant))
        .collect()
}

fn stage_winners(r: &RoomClient, table: &StageTable) -> Vec<String> {
    stage_winners_of(r, table)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// Whether the player's settings show the match's ruleset's score as its
/// percent (DDI's; the others show what they count).
fn shows_percent(table: &StageTable) -> bool {
    table.rules.ruleset == "ddi" && Settings::cached().ddi_percent()
}

/// The tiers the match's ruleset lists, for the judgments' colors.
fn tiers_of(table: &StageTable) -> Vec<Tier> {
    let rules = ddi_engine::rules::presets::by_id(&table.rules.ruleset)
        .unwrap_or_else(ddi_engine::rules::presets::itg);
    crate::screens::play::tiers(&rules)
}

/// A line's judgments as one thin stacked bar `w` wide ending at `x`,
/// each tier its share in the ruleset's color.
fn strip(env: &Env, name: &str, tiers: &[Tier], counts: &[u32], [x, y]: [f32; 2], w: f32) -> Actor {
    let total: u32 = counts.iter().sum();
    let mut c = vec![quad_at(
        "",
        [w, 6.0],
        with_alpha(env.color("Text"), 0.08),
        [x - w / 2.0, y],
    )];
    let mut at = x - w;
    if total > 0 {
        for (tier, &n) in tiers.iter().zip(counts) {
            let part = w * n as f32 / total as f32;
            if part > 0.0 {
                c.push(quad_at(
                    "",
                    [part, 6.0],
                    env.color(tier.color),
                    [at + part / 2.0, y],
                ));
                at += part;
            }
        }
    }
    frame(name, c)
}

/// A line's exact counts, each in its tier's color, ending at `x`.
fn counts(
    env: &Env,
    name: &str,
    tiers: &[Tier],
    row: &StageRowView,
    [x, y]: [f32; 2],
    room: f32,
) -> Actor {
    let mut c = Vec::new();
    let mut at = x;
    for (tier, n) in tiers.iter().zip(&row.tiers).rev() {
        let text = n.to_string();
        let w = look::mono_width(&text, 12.0);
        if x - at + w > room {
            break;
        }
        c.push(right(env.numbers("", &text, 12.0, tier.color), at, y));
        at -= w + 9.0;
    }
    frame(name, c)
}

/// What the table drew: its height, the lines shown and those there are.
pub(super) struct Drawn {
    pub height: f32,
    pub shown: usize,
    pub first: usize,
}

/// The standings' table from `[x, y]`, `w` wide: the match's result where
/// there is one, the headings, then a line a starter. `height`: the room
/// it has (lines sized to it, those that do not fit counted and scrolled
/// from `first`); `None` on a phone, whose lines are all shown.
#[allow(clippy::too_many_arguments)]
pub(super) fn standings(
    env: &Env,
    r: &RoomClient,
    table: &StageTable,
    [x, y, w]: [f32; 3],
    height: Option<f32>,
    pointed: Option<ParticipantId>,
    first: usize,
    out: &mut Vec<Actor>,
) -> Drawn {
    let percent = shows_percent(table);
    let tiers = tiers_of(table);
    let cols = columns(w, table.attacks);
    // The player's battle summary under the lines: its room first.
    let summary = summary_flow(&summary_lines(r, table), w);
    let summary_h = if summary.is_empty() {
        0.0
    } else {
        summary.len() as f32 * SUMMARY_LINE + 6.0
    };
    let height = height.map(|h| (h - summary_h).max(ROW_MIN));
    let rows: Vec<StageRowView> = table
        .rows
        .iter()
        .map(|row| row_view(r, table, row, percent))
        .collect();
    let mut ry = y;
    // The match's result on top: the winners, then everyone's final
    // place and total, three across.
    if let Some(result) = &table.result {
        let head = look::tracked(
            env.text("Final.Heading", &m::final_heading(), 11.0, 700, "Faint"),
            120,
        );
        out.push(left(head, x, ry + 8.0));
        let winners: Vec<String> = result
            .iter()
            .filter(|(_, rank, _)| *rank == 1)
            .map(|(id, _, _)| name_of(r, *id).0)
            .collect();
        let line = m::results_winners(winners.len(), UiText::data(winners.join(", ")));
        let mut t = env.text("Final.Winners", &line, 17.0, 700, "Gold");
        look::ellipsized(&mut t, w);
        out.push(left(t, x, ry + 28.0));
        ry += 42.0;
        let across = if w >= 600.0 { 3 } else { 2 };
        let cell = w / across as f32;
        // A large field's: the first few (a line of them) and the
        // player's own place, rather than everyone (the lines below have
        // them all).
        let listed: Vec<usize> = if result.len() > crate::screens::room::ROOM_OF_NINE {
            let mut v: Vec<usize> = (0..across.min(result.len())).collect();
            if let Some(mine) = result.iter().position(|(id, ..)| *id == r.me())
                && !v.contains(&mine)
            {
                v.push(mine);
            }
            v
        } else {
            (0..result.len()).collect()
        };
        for (k, (id, rank, total)) in listed.iter().map(|&k| (k, &result[k])) {
            let at = listed.iter().position(|&l| l == k).unwrap_or(k);
            let cx = x + (at % across) as f32 * cell;
            let cy = ry + (at / across) as f32 * 19.0 + 10.0;
            let me = *id == r.me();
            let color = if me { "Text" } else { "Secondary" };
            out.push(left(
                env.numbers(&format!("Final{k}.Rank"), &rank.to_string(), 13.0, "Muted"),
                cx,
                cy,
            ));
            let mut n = env.text(
                &format!("Final{k}.Name"),
                &name_of(r, *id).0,
                13.0,
                600,
                color,
            );
            look::ellipsized(&mut n, cell - 24.0 - 56.0);
            out.push(left(n, cx + 24.0, cy));
            out.push(right(
                env.numbers(
                    &format!("Final{k}.Total"),
                    &points_text(*total),
                    13.0,
                    color,
                ),
                cx + cell - 16.0,
                cy,
            ));
        }
        ry += listed.len().div_ceil(across) as f32 * 19.0 + 10.0;
    }
    // Where each column ends, right to left.
    let mut ends = Vec::new();
    let mut end = x + w;
    for col in cols.iter().rev() {
        ends.push((*col, end));
        end -= col.width_with(table.attacks) + if *col == Col::Score { 12.0 } else { 0.0 };
    }
    let end_of = |col: Col| ends.iter().find(|(c, _)| *c == col).map(|(_, e)| *e);
    let name_x = x + 28.0;
    let name_room = (end - 8.0 - name_x).max(60.0);
    // The headings.
    let hy = ry + HEAD_H / 2.0;
    let mut head = env.text("Head.Player", &m::table_player(), 11.0, 700, "Faint");
    look::ellipsized(&mut head, name_room);
    out.push(left(head, name_x, hy));
    for (col, end) in &ends {
        let mut t = env.text(
            &format!("Head.{}", col.name()),
            &col.heading(),
            11.0,
            700,
            "Faint",
        );
        look::ellipsized(&mut t, col.width_with(table.attacks) - 4.0);
        out.push(right(t, *end, hy));
    }
    ry += HEAD_H;
    // The lines: sized to the room; a second line (the counts) where
    // they are tall enough, else the pointed line's counts under them.
    let n = rows.len().max(1) as f32;
    let (row_h, detail, fit) = match height {
        None => (ROW_COMPACT, false, rows.len()),
        Some(h) => {
            let room = (h - (ry - y)).max(ROW_MIN);
            let tall = (room / n).min(ROW_MAX);
            if tall >= ROW_TWO || !cols.contains(&Col::Judgments) {
                let row_h = tall.max(ROW_MIN);
                (row_h, false, ((room / row_h).floor() as usize).max(1))
            } else if (room - DETAIL_H) / n >= ROW_MIN {
                let room = room - DETAIL_H;
                let row_h = (room / n).clamp(ROW_MIN, ROW_MAX);
                (row_h, true, ((room / row_h).floor() as usize).max(1))
            } else {
                // No room for the pointed line's counts: the lines first.
                let row_h = tall.max(ROW_MIN);
                (row_h, false, ((room / row_h).floor() as usize).max(1))
            }
        }
    };
    let two = row_h >= ROW_TWO && cols.contains(&Col::Judgments);
    let more = rows.len() > fit;
    let shown = if more {
        fit.saturating_sub(1).max(1)
    } else {
        rows.len()
    };
    let first = first.min(rows.len().saturating_sub(shown));
    let top = ry;
    // The local player's line stays in view however far it is scrolled
    // (a large field's: the window's last line).
    let me_at = rows.iter().position(|row| row.me);
    let window = crate::screens::room::window_keeping(rows.len(), first, shown, me_at);
    for (k, row) in window.into_iter().map(|k| (k, &rows[k])) {
        let id = format!("Stand{k}");
        let mid = if two {
            ry + row_h / 2.0 - 8.0
        } else {
            ry + row_h / 2.0
        };
        let mut c = Vec::new();
        // The whole line takes the pointer (`crate::hit`: drawn, faintly).
        let plate_alpha = if row.me { 0.07 } else { 0.012 };
        let mut plate = quad_at(
            if row.me { "Me" } else { "" },
            [w + 16.0, row_h - 2.0],
            with_alpha(env.color("Text"), plate_alpha),
            [x + w / 2.0, ry + row_h / 2.0],
        );
        call(&mut plate, "cornerradius", &[6.0]);
        c.push(plate);
        let color = if row.me { "Text" } else { "Secondary" };
        if let Some(place) = row.place {
            c.push(left(
                env.numbers("Place", &place.to_string(), 14.0, "Muted"),
                x,
                mid,
            ));
        }
        let stars_h = 11.0;
        let stars_w = crate::stars::width(stars_h);
        let after = if row.level.is_some() {
            stars_w + 8.0
        } else {
            0.0
        };
        let name_w = look::mono_width(&row.name, 14.0).min((name_room - after).max(40.0));
        let mut name = env.text("Name", &row.name, 14.0, 600, color);
        look::ellipsized(&mut name, name_w);
        c.push(left(name, name_x, mid));
        if let Some(level) = row.level
            && name_room >= name_w + after
        {
            c.push(crate::stars::stars_left(
                env,
                "Stars",
                level,
                stars_h,
                name_x + name_w + 8.0,
                mid + 1.0,
            ));
        }
        let number = |c: &mut Vec<Actor>, name: &str, text: &str, col: Col, color: &str| {
            if let Some(end) = end_of(col)
                && !text.is_empty()
            {
                // Never past its column (a longer language's "+12").
                let mut a = env.numbers(name, text, 14.0, color);
                look::ellipsized(&mut a, col.width_with(table.attacks));
                c.push(right(a, end, mid));
            }
        };
        if let Some(award) = &row.award {
            number(&mut c, "Award", award, Col::Award, "Accent");
        }
        if let Some(total) = &row.total {
            number(&mut c, "Total", total, Col::Total, color);
        }
        if let (Some(rank), Some(end)) = (row.rank, end_of(Col::Match)) {
            // A quiet mark where the place moved.
            let (mark, mark_color) = match row.moved.as_deref() {
                Some("up") => ("▲", "Accent"),
                Some("down") => ("▼", "Faint"),
                _ => ("", "Faint"),
            };
            if !mark.is_empty() {
                c.push(right(
                    env.text("Move", mark, 9.0, 400, mark_color),
                    end,
                    mid,
                ));
            }
            c.push(right(
                env.numbers("Rank", &rank.to_string(), 14.0, "Muted"),
                end - 14.0,
                mid,
            ));
        }
        if !row.reported {
            // Their result has not come: quiet, across the performance.
            let from = end_of(Col::Score).map_or(x + w, |e| e - Col::Score.width());
            let mut t = env.text("Waiting", &m::table_playing(), 13.0, 400, "Faint");
            look::ellipsized(&mut t, (x + w - from).max(60.0));
            c.push(left(t, from + 8.0, mid));
        } else {
            number(&mut c, "Score", &row.score, Col::Score, color);
            if let Some(end) = end_of(Col::Grade)
                && !row.grade.is_empty()
            {
                // A grade of stars (the system's faces: about an em
                // each) smaller, so four fit.
                let size = if row.grade.contains('★') {
                    11.0
                } else {
                    14.0
                };
                let mut g = env.text("Grade", &row.grade, size, 700, "Gold");
                look::ellipsized(&mut g, Col::Grade.width() - 4.0);
                c.push(right(g, end, mid));
            }
            if let Some(end) = end_of(Col::Life)
                && !row.life.is_empty()
            {
                let mut l = env.text("Life", &row.life, 13.0, 400, "Muted");
                look::ellipsized(&mut l, Col::Life.width_with(table.attacks) - 4.0);
                c.push(right(l, end, mid));
            }
            if let Some(combo) = row.combo {
                number(&mut c, "Combo", &combo.to_string(), Col::Combo, "Muted");
            }
            if let Some((sent, taken, shocks)) = row.attacks {
                number(&mut c, "Sent", &sent.to_string(), Col::Sent, "Muted");
                number(&mut c, "Taken", &taken.to_string(), Col::Taken, "Muted");
                number(&mut c, "Shocks", &shocks.to_string(), Col::Shocks, "Muted");
            }
            if let Some(end) = end_of(Col::Judgments)
                && !row.tiers.is_empty()
            {
                c.push(strip(
                    env,
                    "Strip",
                    &tiers,
                    &row.tiers,
                    [end, mid],
                    Col::Judgments.width_with(table.attacks) - 12.0,
                ));
                if two {
                    c.push(counts(
                        env,
                        "Counts",
                        &tiers,
                        row,
                        [end, mid + 18.0],
                        x + w - name_x,
                    ));
                }
            }
        }
        out.push(frame(&id, c));
        ry += row_h;
    }
    if more {
        let below = rows.len() - first - shown;
        let text = if below > 0 {
            m::panel_more_below(below)
        } else {
            m::panel_more_above(first)
        };
        out.push(left(
            env.text("Stand.More", &text, 12.0, 400, "Faint"),
            name_x,
            ry + row_h / 2.0,
        ));
        ry += row_h;
    }
    if detail {
        // The exact counts of the line pointed at (the local player's
        // otherwise, else the first with any).
        let chosen = pointed
            .and_then(|id| rows.iter().find(|row| row.id == id.0))
            .or_else(|| rows.iter().find(|row| row.me && !row.tiers.is_empty()))
            .or_else(|| rows.iter().find(|row| !row.tiers.is_empty()));
        let dy =
            (top + shown as f32 * row_h + if more { row_h } else { 0.0 }) + DETAIL_H / 2.0 + 2.0;
        if let Some(row) = chosen.filter(|row| !row.tiers.is_empty()) {
            let mut c = Vec::new();
            let mut name = env.text("", &row.name, 12.0, 600, "Muted");
            look::ellipsized(&mut name, w * 0.3);
            c.push(left(name, name_x, dy));
            c.push(counts(
                env,
                "Detail.Counts",
                &tiers,
                row,
                [x + w, dy],
                w * 0.66,
            ));
            out.push(frame("Stand.Detail", c));
        }
        ry += DETAIL_H;
    }
    // The player's own attacks, summed up.
    if !summary.is_empty() {
        ry += 6.0;
        for (k, line) in summary.iter().enumerate() {
            let mut t = env.text(&format!("Stand.Battle{k}"), line, 12.0, 400, "Muted");
            look::ellipsized(&mut t, w - 28.0);
            out.push(left(t, name_x, ry + SUMMARY_LINE / 2.0));
            ry += SUMMARY_LINE;
        }
    }
    Drawn {
        height: ry - y,
        shown,
        first,
    }
}

impl ResultsScreen {
    /// A stage of `room`'s results (feedback round B): the panels' place
    /// holds the Standings and Graphs tabs.
    pub fn with_room(mut self, room: SharedRoom, stage: StageId) -> ResultsScreen {
        // The last stage's results open on the standings: the match's
        // result is there.
        let last = room
            .borrow()
            .stage_table(stage)
            .is_some_and(|t| t.number >= t.of);
        let tab = if last { Tab::Standings } else { CHOSEN.get() };
        self.stage = Some(StageMode {
            room,
            stage,
            tab,
            seen: None,
            pointed: None,
            first: 0,
            time: 0.0,
            watched: false,
            quit_at: None,
        });
        self
    }

    /// Whether closing the game from a stage's results asks first
    /// ([`crate::Screen::guards_leaving`]): leaving the room costs
    /// something (Back here only goes back to the room).
    pub(super) fn stage_guards(&self) -> bool {
        self.stage
            .as_ref()
            .is_some_and(|sm| sm.quit_at.is_none() && sm.room.borrow().leave_cost().is_some())
    }

    /// The window asked to close on a stage's results: asked ("Leave the
    /// room?", saying first that the stage was left where the player
    /// left its play: a close request during the play forfeits the stage
    /// and comes here), and the game quit once the room is told if the
    /// player says Leave.
    pub(super) fn stage_message(&mut self, msg: &crate::Message, cx: &mut Cx) {
        use crate::screens::room;
        let Some(sm) = &mut self.stage else {
            return;
        };
        if !cx.is_current_screen() {
            return;
        }
        // (A watcher who left the stage left no play of their own.)
        let left_stage = self.data.quit && !sm.watched;
        if msg.name == crate::ui::CLOSE_REQUESTED
            && !room::ask_leave_after(&self.env, &sm.room, room::LeaveBy::Quit, left_stage, cx)
        {
            cx.request(crate::UiRequest::Quit);
        }
        if room::left_by(msg) == Some(room::LeaveBy::Quit) {
            let told = {
                let r = sm.room.borrow();
                r.is_online() && r.link_state() == ddi_room::LinkState::Connected
            };
            sm.room.borrow_mut().leave();
            sm.quit_at = Some(cx.time() + if told { room::QUIT_WAIT } else { 0.0 });
        }
    }

    /// The local player watched this stage (`watch-v0`: they were not in
    /// it): its results have no play of their own. The left side names
    /// the stage's song and its winner where the grade and the judgments
    /// would be, and there is no Graphs tab.
    pub fn watched(mut self) -> ResultsScreen {
        if let Some(sm) = &mut self.stage {
            sm.watched = true;
            sm.tab = Tab::Standings;
        }
        self
    }

    /// Whether these are a watched stage's results.
    pub(super) fn stage_watched(&self) -> bool {
        self.stage.as_ref().is_some_and(|sm| sm.watched)
    }

    /// The left side of a watched stage's results, from `y` down, `w`
    /// wide: the stage's winner or winners (once the room confirmed it)
    /// under a small heading, a CPU's stars after its name, then that
    /// the viewer watched; where it ends.
    pub(super) fn watched_column(&self, y: f32, w: f32, children: &mut Vec<Actor>) -> f32 {
        let Some(sm) = &self.stage else {
            return y;
        };
        let env = &self.env;
        let r = sm.room.borrow();
        let winners = r
            .stage_table(sm.stage)
            .map(|t| stage_winners_of(&r, &t))
            .unwrap_or_default();
        let mut ty = y + 8.0;
        if !winners.is_empty() {
            let heading = if winners.len() > 1 {
                m::watched_winners()
            } else {
                m::watched_winner()
            };
            let head = look::tracked(env.text("Watched.Heading", &heading, 12.0, 600, "Faint"), 120);
            children.push(left(head, 0.0, ty));
            ty += 26.0;
            for (i, (name, level)) in winners.iter().enumerate() {
                let stars = level.map(|_| crate::stars::width(14.0) + 10.0).unwrap_or(0.0);
                let mut t = env.text(
                    &format!("Watched.Winner{i}"),
                    &UiText::data(name.clone()),
                    24.0,
                    700,
                    "Gold",
                );
                look::ellipsized(&mut t, (w - stars).max(40.0));
                children.push(left(t, 0.0, ty));
                if let Some(level) = level {
                    children.push(crate::stars::stars_right(
                        env,
                        &format!("Watched.Winner{i}.Stars"),
                        *level,
                        14.0,
                        w,
                        ty,
                    ));
                }
                ty += 32.0;
            }
            ty += 4.0;
        }
        let note = env.text("Watched.Note", &m::watched_note(), 13.0, 400, "Muted");
        children.push(left(note, 0.0, ty));
        ty + 20.0
    }

    /// The tabs' row from `[x, y]`: Standings, Graphs.
    fn stage_tabs(&self, [x, y]: [f32; 2], children: &mut Vec<Actor>) {
        let Some(sm) = &self.stage else {
            return;
        };
        let env = &self.env;
        let mut tx = x;
        // A watched stage has no graphs of the viewer's own.
        let tabs = [
            (Tab::Standings, "Tab.Standings", m::results_tab_standings()),
            (Tab::Graphs, "Tab.Graphs", m::results_tab_graphs()),
        ];
        for (tab, name, label) in tabs
            .into_iter()
            .filter(|(tab, ..)| *tab == Tab::Standings || !sm.watched)
        {
            let w = (look::mono_width(&label, 13.0) + 32.0).max(104.0);
            let mut b = look::component_or(
                &*env.look,
                "StyleTab",
                "Button",
                &Spec::new([w, TABS_H], label.as_str()).with_items([String::new()]),
            );
            b.name = name.into();
            let w = super::super::extent(&mut b, [w, TABS_H])[0];
            call(&mut b, "xy", &[tx + w / 2.0, y + TABS_H / 2.0]);
            crate::widgets::play(
                &mut b,
                if sm.tab == tab {
                    "Selected"
                } else {
                    "Available"
                },
            );
            b.finish_tweening();
            children.push(b);
            tx += w + 8.0;
        }
    }

    /// The standings in a panel at `rect` (`height` `None`: a phone's, as
    /// tall as its lines), and the panel's height.
    fn standings_panel(&self, [x, y, w, h]: [f32; 4], fitted: bool) -> (Actor, f32) {
        let Some(sm) = &self.stage else {
            return (frame("Panel.Standings", Vec::new()), 0.0);
        };
        let r = sm.room.borrow();
        let Some(table) = r.stage_table(sm.stage) else {
            return (frame("Panel.Standings", Vec::new()), 0.0);
        };
        let (top, pad) = (44.0, 14.0);
        let mut content = Vec::new();
        let drawn = standings(
            &self.env,
            &r,
            &table,
            [pad, top, w - 2.0 * pad],
            fitted.then_some(h - top - 10.0),
            sm.pointed,
            sm.first,
            &mut content,
        );
        let note: String = if !table.confirmed {
            m::table_waiting().into()
        } else if table.experimental {
            m::mark_experimental().into()
        } else {
            String::new()
        };
        let h = if fitted { h } else { top + drawn.height + 12.0 };
        let title = m::table_title(usize::from(table.number), usize::from(table.of));
        let panel = self.panel("Standings", &title, &note, [x, y, w, h], content);
        self.stage_shown
            .set((drawn.first, drawn.shown, table.rows.len(), w - 2.0 * pad));
        (panel, h)
    }

    /// The panels' place at `rect` for a stage: the tabs, then the
    /// standings or the graphs.
    pub(super) fn stage_panels(&self, [x, y, w, h]: [f32; 4]) -> Actor {
        let mut children = Vec::new();
        self.stage_tabs([x, y], &mut children);
        let rect = [x, y + TABS_H + TABS_GAP, w, h - TABS_H - TABS_GAP];
        match self.stage.as_ref().map(|sm| sm.tab) {
            Some(Tab::Graphs) => children.extend(self.graph_panels(rect)),
            _ => children.push(self.standings_panel(rect, true).0),
        }
        frame("Panels", children)
    }

    /// A phone's: the tabs, then the standings or the graphs one under
    /// another, from `[x, y]`, `w` wide; where they end.
    pub(super) fn stage_compact(
        &self,
        [x, y, w]: [f32; 3],
        ph: f32,
        gap: f32,
    ) -> (Vec<Actor>, f32) {
        let mut children = Vec::new();
        self.stage_tabs([x, y], &mut children);
        let mut py = y + TABS_H + TABS_GAP;
        match self.stage.as_ref().map(|sm| sm.tab) {
            Some(Tab::Graphs) => {
                for &kind in self.data.panels() {
                    children.push(self.panel_of(kind, [x, py, w, ph]));
                    py += ph + gap;
                }
            }
            _ => {
                let (panel, h) = self.standings_panel([x, py, w, 0.0], false);
                children.push(panel);
                py += h + gap;
            }
        }
        (children, py)
    }

    /// The tabs' place in the focus walk.
    pub(super) fn stage_focusables(&self) -> Vec<Focusable> {
        match &self.stage {
            Some(sm) if sm.watched => vec![Focusable::new("Tab.Standings")],
            Some(_) => vec![
                Focusable::new("Tab.Standings"),
                Focusable::new("Tab.Graphs"),
            ],
            None => Vec::new(),
        }
    }

    /// The tab shown changes: the panels built again, the choice kept
    /// for the session.
    fn set_tab(&mut self, tab: Tab, root: &mut Actor, cx: &mut Cx) {
        let Some(sm) = &mut self.stage else {
            return;
        };
        if sm.tab == tab || (sm.watched && tab == Tab::Graphs) {
            return;
        }
        sm.tab = tab;
        sm.first = 0;
        CHOSEN.set(tab);
        self.stage_redraw(root, cx);
    }

    /// The stage's part of the screen drawn again (the room changed, the
    /// tab, the line pointed at).
    fn stage_redraw(&mut self, root: &mut Actor, cx: &mut Cx) {
        if self.compact {
            let (m, top) = {
                let (top, _, m) = self.compact_bars();
                (m, top)
            };
            let view_w = self.screen[0] - 2.0 * m;
            let (mut content, content_h) = self.compact_content(view_w);
            self.content_h = content_h;
            self.scroll = self.scroll.min(self.max_scroll());
            call(&mut content, "y", &[-self.scroll]);
            if let Some(old) = hit::find_mut(root, "ResultsContent") {
                super::super::refill(old, content);
                call(old, "y", &[-self.scroll]);
            }
            let _ = top;
        } else {
            let (_, panels, _) = self.places();
            let fresh = self.stage_panels(panels);
            if let Some(old) = hit::find_mut(root, "Panels") {
                super::super::refill(old, fresh);
            }
        }
        // The focus stays on a tab's button, by its name.
        let _ = cx;
        self.stage_publish();
    }

    /// The stage's per-frame part: the room's messages taken (an online
    /// room's reports come while the results show), the standings drawn
    /// again when it changed, and the room told its results were shown
    /// once the standings showed them confirmed.
    pub(super) fn stage_update(&mut self, root: &mut Actor, cx: &mut Cx) {
        let Some(sm) = &mut self.stage else {
            return;
        };
        sm.time = cx.time();
        let room = sm.room.clone();
        room.borrow_mut()
            .tick((cx.clock() * 1000.0).max(0.0) as u64);
        if let Some(at) = sm.quit_at
            && (cx.time() >= at || room.borrow().link_state().ended())
        {
            sm.quit_at = None;
            cx.request(crate::UiRequest::Quit);
            return;
        }
        let version = room.borrow().version();
        let changed = sm.seen.replace(version) != Some(version);
        let (tab, stage) = (sm.tab, sm.stage);
        if tab == Tab::Standings
            && room
                .borrow()
                .stage_table(stage)
                .is_some_and(|t| t.confirmed)
            && room.borrow().unseen_results().is_some()
        {
            // Seen here: song select does not pop the standings up again.
            room.borrow_mut().results_shown();
        }
        if changed {
            self.stage_redraw(root, cx);
        }
    }

    /// When to look at the room again while nothing moves.
    pub(super) fn stage_wake_at(&self) -> Option<f64> {
        self.stage.as_ref().map(|sm| sm.time + 0.1)
    }

    /// Says what the stage's standings show (`ThemeState::stage`).
    pub(super) fn stage_publish(&self) {
        let view = self.stage.as_ref().and_then(|sm| {
            let r = sm.room.borrow();
            let table = r.stage_table(sm.stage)?;
            // The columns as the table was last drawn (the standings'
            // width; the wide layout's before it is).
            let w = match self.stage_shown.get().3 {
                w if w > 0.0 => w,
                _ => self.places().1[2] - 28.0,
            };
            let mut view = view_of(&r, &table, sm.tab, &columns(w, table.attacks));
            view.watched = sm.watched;
            Some(view)
        });
        self.env.state.borrow_mut().stage = view;
    }

    /// The footer's hints for the tabs: a keyboard's and a controller's.
    pub(super) fn stage_hint(&self) -> Option<UiText> {
        self.stage
            .as_ref()
            .filter(|sm| !sm.watched)
            .map(|_| m::results_hint_tabs())
    }

    /// The stage's input: Left and Right (a keyboard's) and the shoulders
    /// change the tab; a tab's button; the pointer over a line; Up, Down
    /// and the wheel where the lines do not all fit.
    pub(super) fn stage_input(
        &mut self,
        input: &Input,
        root: &mut Actor,
        cx: &mut Cx,
    ) -> Option<Handled> {
        let sm = self.stage.as_ref()?;
        let tab = sm.tab;
        match input {
            Input::Menu { action, .. } => {
                let to = match action {
                    MenuAction::Previous => Tab::Standings,
                    MenuAction::Next => Tab::Graphs,
                    MenuAction::Left if cx.menu_from_keyboard() => Tab::Standings,
                    MenuAction::Right if cx.menu_from_keyboard() => Tab::Graphs,
                    MenuAction::Up | MenuAction::Down
                        if cx.menu_from_keyboard() && !self.compact && tab == Tab::Standings =>
                    {
                        let by = if *action == MenuAction::Down { 1 } else { -1 };
                        return self.stage_scroll(by, root, cx).then_some(Handled::Yes);
                    }
                    _ => return None,
                };
                self.set_tab(to, root, cx);
                Some(Handled::Yes)
            }
            Input::Activate { name } if name == "Tab.Standings" || name == "Tab.Graphs" => {
                let to = if name == "Tab.Graphs" {
                    Tab::Graphs
                } else {
                    Tab::Standings
                };
                self.set_tab(to, root, cx);
                // The focus back on the bar's button: Enter after a tap on
                // a tab goes back to the room, not to the tab again.
                cx.focus("SongList");
                Some(Handled::Yes)
            }
            Input::Wheel {
                target: Some(t),
                delta,
                ..
            } if !self.compact && tab == Tab::Standings && t.within("Panel.Standings") => {
                let by = if delta[1] > 0.0 { 1 } else { -1 };
                self.stage_scroll(by, root, cx);
                Some(Handled::Yes)
            }
            Input::Hover { target } if tab == Tab::Standings && !self.compact => {
                let row = target.as_ref().and_then(|t| {
                    t.chain
                        .iter()
                        .find_map(|n| n.strip_prefix("Stand")?.parse::<usize>().ok())
                });
                let pointed = row.and_then(|k| {
                    let sm = self.stage.as_ref()?;
                    let table = sm.room.borrow().stage_table(sm.stage)?;
                    table.rows.get(k).map(|row| row.participant)
                });
                if let Some(sm) = &mut self.stage
                    && sm.pointed != pointed
                {
                    sm.pointed = pointed;
                    self.stage_redraw(root, cx);
                }
                None
            }
            _ => None,
        }
    }

    /// The lines scrolled by `by`, where they do not all fit; whether
    /// they moved.
    fn stage_scroll(&mut self, by: i32, root: &mut Actor, cx: &mut Cx) -> bool {
        let (first, shown, len, _) = self.stage_shown.get();
        let last = len.saturating_sub(shown);
        let to = (first as i64 + i64::from(by)).clamp(0, last as i64) as usize;
        let Some(sm) = &mut self.stage else {
            return false;
        };
        if to == sm.first && to == first {
            return false;
        }
        sm.first = to;
        self.stage_redraw(root, cx);
        true
    }
}
