//! A room's players panel on song select's wide layout (round SLB, the
//! maintainer's decisions of 2026-11-19 and 20; `docs/plans/song-navigation.md`,
//! "In a room"): the room in three parts, the song's side where solo has
//! it, the list, and the players at the screen's right as a panel of the
//! frame.
//!
//! - **The panel**: a 1 px line from the header's bar to the footer's at
//!   its left edge, a quieter ground from the line to the screen's edge;
//!   the list's scrollbar sits against the line as it sits against the
//!   screen's edge alone. Its head says Players and the count (a tap opens
//!   the Players pop-up); a chevron handle on the line folds it.
//! - **Folded**: a [`RAIL`] wide rail with the count and one state icon a
//!   player, the list taking the freed width. L, the handle and the room's
//!   More fold and unfold it; the fold is this device's (`Settings::
//!   players_folded`), open by default, never changed by itself; a 0.2 s
//!   slide (none under reduced motion).
//! - **A row**: a monotone state icon ([`StateIcon`]), then the person's
//!   marks where they have any (a golden house for the host, a silver one
//!   for a co-host, a wrench for an administrator, a note for the one
//!   choosing the room's songs: the marks displace the name, no slot is
//!   kept), then the name; at the right the place in the match once a
//!   stage counts, a CPU's level as five pips, a bot's head.
//! - **The words**: a small pop-over beside the row on hover (a mouse) or
//!   a tap (touch; a second tap opens the Players pop-up), and the Players
//!   pop-up (a click on a row, the head, L's More).
//!
//! Icons and marks are vector shapes in the look's colors (never glyphs).
//! A child module of `select` (it reads the screen's own fields).

use ddi_actor::Actor;

use super::SelectScreen;
use crate::look::{self, call, quad};
use crate::screens::room::{self as rm, HostMark, PlayerRow, StateIcon};
use crate::screens::{Env, frame, left, right};
use ddi_l10n::msg::room as m;

/// The players' column's width at 1280 × 720 (the decision's 210).
pub(super) const PANEL_W: f32 = 210.0;
/// The folded panel's rail's width.
pub(super) const RAIL: f32 = 40.0;
/// From the panel's line to its column.
pub(super) const LINE_GAP: f32 = 16.0;
/// From the list's end to the panel's line (its scrollbar between).
pub(super) const LIST_GAP: f32 = 22.0;
/// A row's height, and a rail's icon's.
const ROW: f32 = 24.0;
const RAIL_ROW: f32 = 22.0;
/// The fold's slide, seconds.
pub(super) const SLIDE: f32 = 0.2;

/// The handle on the line (folds and unfolds).
pub(super) const HANDLE: &str = "Panel.Handle";
/// The panel's head (the Players pop-up).
pub(super) const HEAD: &str = "Players.Head";
/// The folded rail (a tap unfolds it).
pub(super) const RAIL_NAME: &str = "Panel.Rail";
/// The pop-over with a row's words.
pub(super) const POPOVER: &str = "Players.Popover";

/// A row's hit area's name.
pub(super) fn row_name(k: usize) -> String {
    format!("Players.Row{k}")
}

/// The row a target's name is, if it is one: its hit area, or its frame
/// (`Players<k>`, which its name and marks are in).
pub(super) fn row_of(name: &str) -> Option<usize> {
    let k = name.strip_prefix("Players.Row").or_else(|| name.strip_prefix("Players"))?;
    k.parse().ok()
}

/// Where the panel's line is for a screen `width` wide with side margins
/// `margin`: `PANEL_W` and its gap in from the margin, or the rail's
/// width from the edge folded.
pub(super) fn line_x(width: f32, margin: f32, folded: bool) -> f32 {
    if folded {
        width - RAIL
    } else {
        width - margin - PANEL_W - LINE_GAP
    }
}

/// A shape `size` large centered on `at` (rounded `r`), turned `deg`, in
/// the look's color `color`.
fn shape(env: &Env, color: &str, size: [f32; 2], at: [f32; 2], r: f32, deg: f32) -> Actor {
    let mut q = quad("", size, env.color(color));
    call(&mut q, "cornerradius", &[r]);
    if deg != 0.0 {
        call(&mut q, "rotationz", &[deg]);
    }
    call(&mut q, "xy", &at);
    q
}

/// A ring `d` across, `w` thick.
fn ring(env: &Env, color: &str, d: f32, w: f32, at: [f32; 2]) -> Actor {
    let mut q = shape(env, color, [d, d], at, d / 2.0, 0.0);
    call(&mut q, "outline", &[w]);
    q
}

/// A state's icon, about 14 px, centered on the origin, in one ink: Text
/// where a state asks for attention (not ready, a failed load), Faint
/// where nothing goes on (before a pick, away, left), Muted otherwise;
/// shapes alone tell them apart (round 4's monotone set).
pub fn state_icon(env: &Env, icon: StateIcon) -> Actor {
    let ink = match icon {
        StateIcon::NotReady | StateIcon::LoadFailed => "Text",
        StateIcon::Idle | StateIcon::Away | StateIcon::Left => "Faint",
        _ => "Muted",
    };
    let c: Vec<Actor> = match icon {
        // A short dash: nothing yet.
        StateIcon::Idle => vec![shape(env, ink, [7.0, 2.0], [0.0, 0.0], 1.0, 0.0)],
        // Three dots: waiting for the song.
        StateIcon::WaitingSong => [-4.5, 0.0, 4.5]
            .iter()
            .map(|x| shape(env, ink, [3.0, 3.0], [*x, 0.0], 1.5, 0.0))
            .collect(),
        // An arrow down to a line, the line filled to the progress.
        StateIcon::Loading(p) => {
            let w = (12.0 * p.clamp(0.0, 1.0)).max(2.4);
            vec![
                shape(env, ink, [2.2, 7.0], [0.0, -3.0], 1.1, 0.0),
                shape(env, ink, [2.2, 5.5], [-1.7, 0.6], 1.1, -45.0),
                shape(env, ink, [2.2, 5.5], [1.7, 0.6], 1.1, 45.0),
                {
                    let mut t = shape(env, ink, [12.0, 2.6], [0.0, 5.6], 1.3, 0.0);
                    call(&mut t, "diffusealpha", &[0.35]);
                    t
                },
                shape(env, ink, [w, 2.6], [-6.0 + w / 2.0, 5.6], 1.3, 0.0),
            ]
        }
        // A filled disc with a dark "!".
        StateIcon::LoadFailed => vec![
            shape(env, ink, [13.0, 13.0], [0.0, 0.0], 6.5, 0.0),
            shape(env, "Ground", [2.0, 5.5], [0.0, -1.4], 1.0, 0.0),
            shape(env, "Ground", [2.2, 2.2], [0.0, 3.4], 1.1, 0.0),
        ],
        // An open ring: still to press Ready.
        StateIcon::NotReady => vec![ring(env, ink, 12.0, 1.8, [0.0, 0.0])],
        // A filled disc with a dark check.
        StateIcon::Ready => vec![
            shape(env, ink, [13.0, 13.0], [0.0, 0.0], 6.5, 0.0),
            shape(env, "Ground", [2.2, 4.6], [-2.3, 1.0], 1.1, -45.0),
            shape(env, "Ground", [2.2, 8.0], [1.4, -0.4], 1.1, 40.0),
        ],
        // Three rising bars.
        StateIcon::Playing => [-4.0, 0.0, 4.0]
            .iter()
            .zip([6.0, 9.0, 12.0])
            .map(|(x, h)| shape(env, ink, [2.6, h], [*x, 6.0 - h / 2.0], 1.0, 0.0))
            .collect(),
        StateIcon::Out => vec![
            shape(env, ink, [2.4, 12.0], [0.0, 0.0], 1.2, 45.0),
            shape(env, ink, [2.4, 12.0], [0.0, 0.0], 1.2, -45.0),
        ],
        // A ring struck through: not in this stage.
        StateIcon::SatOut => vec![
            ring(env, ink, 12.0, 1.6, [0.0, 0.0]),
            shape(env, ink, [1.8, 12.0], [0.0, 0.0], 0.9, 45.0),
        ],
        // A bar and an arrow leaving it.
        StateIcon::Left => vec![
            shape(env, ink, [2.0, 11.0], [-4.5, 0.0], 1.0, 0.0),
            shape(env, ink, [8.0, 2.0], [0.5, 0.0], 1.0, 0.0),
            shape(env, ink, [2.0, 5.0], [3.2, -1.6], 1.0, -45.0),
            shape(env, ink, [2.0, 5.0], [3.2, 1.6], 1.0, 45.0),
        ],
        // An eye.
        StateIcon::Watching => vec![
            {
                let mut e = shape(env, ink, [13.0, 8.0], [0.0, 0.0], 4.0, 0.0);
                call(&mut e, "outline", &[1.5]);
                e
            },
            shape(env, ink, [4.0, 4.0], [0.0, 0.0], 2.0, 0.0),
        ],
        // A pause.
        StateIcon::Away => vec![
            shape(env, ink, [2.6, 10.0], [-2.4, 0.0], 1.0, 0.0),
            shape(env, ink, [2.6, 10.0], [2.4, 0.0], 1.0, 0.0),
        ],
    };
    let mut a = frame("Icon", c);
    a.decoration = true;
    a
}

/// A mark before a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A house: golden for the host, silver for a co-host (not a crown:
    /// "host" was read as the "Leader" targeting choice).
    House(HostMark),
    /// An administrator (the server's own).
    Wrench,
    /// The one choosing the room's songs.
    Note,
}

/// The marks a row has, in their order.
pub fn marks_of(p: &PlayerRow) -> Vec<Mark> {
    let mut v: Vec<Mark> = p.marks.host.map(Mark::House).into_iter().collect();
    if p.marks.admin {
        v.push(Mark::Wrench);
    }
    if p.marks.picking {
        v.push(Mark::Note);
    }
    v
}

/// A mark's part's name after its row's (`Players<k>.Host`): what it
/// marks.
pub fn mark_name(mark: Mark) -> &'static str {
    match mark {
        Mark::House(HostMark::Host) => "Host",
        Mark::House(HostMark::CoHost) => "CoHost",
        Mark::Wrench => "Admin",
        Mark::Note => "Picking",
    }
}

/// A mark's width.
const MARK_W: f32 = 12.0;
/// A mark's room (its width and the gap after it).
const MARK_STEP: f32 = 15.0;

/// A mark, about 12 px, centered on the origin: the houses in gold and
/// silver (the stars' colors), the rest in the row's one ink.
pub fn mark(env: &Env, mark: Mark) -> Actor {
    let c: Vec<Actor> = match mark {
        Mark::House(kind) => {
            let ink = match kind {
                HostMark::Host => "Star.Gold",
                HostMark::CoHost => "Star.Silver",
            };
            vec![
                // The roof (a square turned on its corner), the walls
                // under it, a dark door.
                shape(env, ink, [7.4, 7.4], [0.0, -1.6], 0.8, 45.0),
                shape(env, ink, [9.0, 6.4], [0.0, 2.6], 0.8, 0.0),
                shape(env, "Ground", [2.4, 3.6], [0.0, 4.0], 0.6, 0.0),
            ]
        }
        Mark::Wrench => vec![
            // A handle turned 45°, an open jaw at its top end.
            shape(env, "Secondary", [2.4, 9.0], [-1.4, 1.4], 1.2, 45.0),
            ring(env, "Secondary", 6.4, 2.0, [2.6, -2.6]),
            shape(env, "Ground", [2.6, 3.4], [3.9, -3.9], 0.4, 45.0),
        ],
        Mark::Note => vec![
            shape(env, "Secondary", [5.0, 4.4], [-1.6, 3.4], 2.2, -20.0),
            shape(env, "Secondary", [1.6, 9.0], [0.6, -1.2], 0.6, 0.0),
            shape(env, "Secondary", [4.6, 1.8], [2.4, -5.0], 0.8, 30.0),
        ],
    };
    let mut a = frame("Mark", c);
    a.decoration = true;
    a
}

/// A bot's head, about 12 px.
fn bot_head(env: &Env) -> Actor {
    let mut a = frame(
        "Bot",
        vec![
            {
                let mut b = shape(env, "Muted", [11.0, 9.0], [0.0, 1.0], 2.5, 0.0);
                call(&mut b, "outline", &[1.4]);
                b
            },
            shape(env, "Muted", [2.2, 2.2], [-2.2, 1.0], 1.1, 0.0),
            shape(env, "Muted", [2.2, 2.2], [2.2, 1.0], 1.1, 0.0),
            shape(env, "Muted", [1.4, 3.0], [0.0, -4.2], 0.7, 0.0),
        ],
    );
    a.decoration = true;
    a
}

/// A CPU's level, 1 to 10, as five pips (silver to 5, gold over silver
/// past it, as the stars), 19 px wide.
fn level_pips(env: &Env, level: u8) -> Actor {
    let pips = (0..5)
        .map(|k| {
            let k1 = k as u8 + 1;
            let col = if level > 5 && k1 <= level - 5 {
                "Star.Gold"
            } else if level > 5 || k1 <= level {
                "Star.Silver"
            } else {
                "Star.Empty"
            };
            shape(env, col, [3.2, 3.2], [-8.0 + k as f32 * 4.0, 0.0], 1.6, 0.0)
        })
        .collect();
    let mut a = frame("Level", pips);
    a.decoration = true;
    a
}

/// An invisible hit area `size` large centered on `at` (a tap's target).
fn hit(name: &str, size: [f32; 2], at: [f32; 2]) -> Actor {
    look::quad_at(name, size, [0.0, 0.0, 0.0, 0.004], at)
}

/// What the panel shows of the room: its rows in the panel's order and
/// the count.
pub(super) struct Shown {
    pub rows: Vec<PlayerRow>,
    pub cap: u16,
}

impl SelectScreen {
    /// The players panel (`RoomPlayers`) of a room's wide layout, its
    /// column at `[x, y, w, h]` (the line `LINE_GAP` left of it), open or
    /// folded to its rail as the room's fold says.
    pub(super) fn players_panel(&self, [x, y, w, h]: [f32; 4]) -> Actor {
        let env = &self.env;
        let Some(room) = &self.room else {
            return frame("RoomPlayers", Vec::new());
        };
        let shown = {
            let r = room.room.borrow();
            Shown {
                rows: rm::panel_order(rm::player_rows(&r)),
                cap: r.state().policy.max_competitors,
            }
        };
        let folded = room.folded;
        let [sw, sh] = self.layout.screen;
        let line = x - LINE_GAP;
        let (top, bottom) = (self.env.metric("Header.Height", 64.0), sh - self.env.metric("Footer.Height", 40.0));
        let mut c = Vec::new();
        // The ground: from the line past the screen's edge (so a slide
        // never shows the list's ground under it).
        let gw = sw - line + PANEL_W + 64.0;
        let mut ground = quad("Panel.Ground", [gw, bottom - top], env.color("Plate"));
        call(&mut ground, "diffusealpha", &[0.72]);
        call(&mut ground, "xy", &[line + gw / 2.0, (top + bottom) / 2.0]);
        // (Drawn only: its slack past the edge counts in no bounds.)
        ground.decoration = true;
        c.push(ground);
        let mut rule = shape(env, "Line2", [1.0, bottom - top], [line, (top + bottom) / 2.0], 0.0, 0.0);
        rule.name = "Panel.Line".into();
        c.push(rule);
        let counted = rm::players_counted(&shown.rows);
        if folded {
            // The rail: a tap on it unfolds; the count at its head, then
            // one state icon a player.
            let mut rail = hit(RAIL_NAME, [sw - line, bottom - top], [(line + sw) / 2.0, (top + bottom) / 2.0]);
            rail.decoration = false;
            c.push(rail);
            let cx = line + RAIL / 2.0;
            let hy = top + 28.0;
            c.push({
                let mut t = env.numbers("Players.Count", &ddi_l10n::UiText::data(counted.to_string()), 12.0, "Muted");
                call(&mut t, "xy", &[cx, hy + 34.0]);
                t
            });
            let first = hy + 58.0;
            let fit = ((bottom - first - 8.0) / RAIL_ROW).floor().max(0.0) as usize;
            for (k, p) in shown.rows.iter().enumerate().take(fit) {
                let cy = first + k as f32 * RAIL_ROW;
                if p.me {
                    c.push(shape(env, "Accent", [3.0, 16.0], [line + 4.0, cy], 1.5, 0.0));
                }
                let mut ic = state_icon(env, p.icon);
                call(&mut ic, "xy", &[cx + 2.0, cy]);
                c.push(ic);
            }
        } else {
            self.panel_rows(&mut c, &shown, [x, y, w, h]);
        }
        c.extend(self.handle(line, top, folded));
        // The pop-over last: over the list.
        if !folded
            && let Some(k) = room.popover
            && let Some(p) = shown.rows.get(k)
        {
            c.push(self.popover(p, line, row_y(y, k)));
        }
        frame("RoomPlayers", c)
    }

    /// The handle on the line, near the head: a small plate with a
    /// chevron pointing the way it folds.
    fn handle(&self, line: f32, top: f32, folded: bool) -> Vec<Actor> {
        let env = &self.env;
        let hy = top + 28.0;
        let mut plate = shape(env, "Plate2", [18.0, 36.0], [line, hy], 6.0, 0.0);
        plate.name = HANDLE.into();
        let mut edge = shape(env, "Line2", [18.0, 36.0], [line, hy], 6.0, 0.0);
        call(&mut edge, "outline", &[1.0]);
        edge.decoration = true;
        // Open: ">" (it folds to the right); folded: "<".
        let dir = if folded { -1.0 } else { 1.0 };
        let mut a = shape(env, "Secondary", [1.8, 7.0], [line + dir * 0.8, hy - 2.2], 0.9, -dir * 40.0);
        let mut b = shape(env, "Secondary", [1.8, 7.0], [line + dir * 0.8, hy + 2.2], 0.9, dir * 40.0);
        a.decoration = true;
        b.decoration = true;
        vec![plate, edge, a, b]
    }

    /// The open panel's head and rows.
    fn panel_rows(&self, c: &mut Vec<Actor>, shown: &Shown, [x, y, w, h]: [f32; 4]) {
        let env = &self.env;
        let title: String = m::players().into();
        c.push(hit(HEAD, [w, 22.0], [x + w / 2.0, y + 12.0]));
        c.push(left(
            look::tracked(env.text("Players.Heading", &title, 11.0, 700, "Faint"), 120),
            x,
            y + 12.0,
        ));
        c.push(right(
            env.text(
                "Players.Count",
                &m::players_count(rm::players_counted(&shown.rows), shown.cap),
                11.0,
                400,
                "Muted",
            ),
            x + w,
            y + 12.0,
        ));
        let fit = ((h - 30.0) / ROW).floor().max(0.0) as usize;
        let rows = &shown.rows;
        let n = if rows.len() > fit { fit.saturating_sub(1) } else { rows.len() };
        for (k, p) in rows.iter().enumerate().take(n) {
            let cy = row_y(y, k);
            let mut parts = vec![hit(&row_name(k), [w + 8.0, ROW], [x + w / 2.0, cy])];
            if p.me {
                let mut bar = shape(env, "Accent", [3.0, ROW - 6.0], [x + 1.5, cy], 1.5, 0.0);
                bar.name = format!("Players{k}.Me");
                parts.push(bar);
            }
            let mut ic = state_icon(env, p.icon);
            ic.name = format!("Players{k}.Icon");
            call(&mut ic, "xy", &[x + 15.0, cy]);
            parts.push(ic);
            // At the right: the place once a stage counts, a CPU's
            // level, a bot's head.
            let mut right_w = 6.0;
            if let Some(place) = p.place {
                let t = place.to_string();
                parts.push(right(
                    env.numbers(&format!("Players{k}.Place"), &ddi_l10n::UiText::data(&t), 11.0, "Muted"),
                    x + w,
                    cy,
                ));
                right_w += look::mono_width(&t, 11.0) + 6.0;
            }
            if let Some(level) = p.level {
                let mut a = level_pips(env, level);
                call(&mut a, "xy", &[x + w - right_w - 19.0 / 2.0, cy]);
                parts.push(a);
                right_w += 19.0 + 6.0;
            } else if p.bot {
                let mut a = bot_head(env);
                call(&mut a, "xy", &[x + w - right_w - MARK_W / 2.0, cy]);
                parts.push(a);
                right_w += MARK_W + 6.0;
            }
            // The marks, then the name: the marks displace it.
            let mut nx = x + 29.0;
            for mk in marks_of(p) {
                let mut a = mark(env, mk);
                a.name = format!("Players{k}.{}", mark_name(mk));
                call(&mut a, "xy", &[nx + MARK_W / 2.0, cy]);
                parts.push(a);
                nx += MARK_STEP;
            }
            let color = if p.me {
                "Text"
            } else if p.tone == rm::Tone::Away {
                "Faint"
            } else {
                "Secondary"
            };
            // The name, and the "You" badge after the local player's
            // where it fits (the badge goes before the name is cut).
            let (name, _) = rm::name_and_badge(
                env,
                &format!("Players{k}.Name"),
                &p.name,
                p.badge,
                (13.0, if p.me { 700 } else { 400 }, color),
                [nx, cy],
                (w - (nx - x) - right_w).max(20.0),
            );
            parts.extend(name);
            c.push(frame(&format!("Players{k}"), parts));
        }
        if n < rows.len() {
            // The rest counted on a last line: the Players pop-up.
            let more = rows.len() - n;
            let cy = row_y(y, n);
            let text = m::panel_more_below(more);
            c.push(hit(rm::PLAYERS_MORE, [w, ROW], [x + w / 2.0, cy]));
            c.push(left(env.text("Players.MoreText", &text, 13.0, 600, "Muted"), x + 29.0, cy));
        }
    }

    /// A row's words in a small plate left of the panel's line, at the
    /// row's height: the name, then the role, seat and points, then what
    /// they are doing, then the attack policy.
    fn popover(&self, p: &PlayerRow, line: f32, cy: f32) -> Actor {
        let env = &self.env;
        let mut lines: Vec<(String, f32, u16, &str)> = vec![(p.name.clone(), 14.0, 700, "Text")];
        if !p.sub.is_empty() {
            lines.push((p.sub.clone(), 12.0, 400, "Secondary"));
        }
        let doing = if p.status.is_empty() { &p.state } else { &p.status };
        if !doing.is_empty() && !p.sub.contains(doing.as_str()) {
            lines.push((doing.clone(), 12.0, 600, "Muted"));
        }
        if !p.policy.is_empty() {
            lines.push((p.policy.clone(), 12.0, 400, "Muted"));
        }
        let pad = 10.0;
        let most = 280.0;
        let w = lines
            .iter()
            .map(|(t, size, ..)| look::mono_width(t, *size))
            .fold(0.0f32, f32::max)
            .min(most)
            + 2.0 * pad;
        let lh = 18.0;
        let h = lines.len() as f32 * lh + 2.0 * pad - 4.0;
        let x1 = line - 10.0;
        let x0 = x1 - w;
        let [_, sh] = self.layout.screen;
        let y0 = (cy - lh / 2.0 - pad).clamp(72.0, sh - 48.0 - h);
        let mut parts = Vec::new();
        let mut plate = look::quad_at("Plate", [w, h], env.color("Plate2"), [x0 + w / 2.0, y0 + h / 2.0]);
        call(&mut plate, "cornerradius", &[8.0]);
        parts.push(plate);
        let mut edge = look::quad_at("Edge", [w, h], env.color("Line2"), [x0 + w / 2.0, y0 + h / 2.0]);
        call(&mut edge, "cornerradius", &[8.0]);
        call(&mut edge, "outline", &[1.0]);
        parts.push(edge);
        for (i, (t, size, weight, color)) in lines.iter().enumerate() {
            let mut a = env.text(&format!("Popover.Line{i}"), t, *size, *weight, color);
            look::ellipsized(&mut a, w - 2.0 * pad);
            parts.push(left(a, x0 + pad, y0 + pad - 2.0 + lh / 2.0 + i as f32 * lh));
        }
        let mut f = frame(POPOVER, parts);
        f.decoration = true;
        f
    }
}

/// A row's middle, from the column's top.
fn row_y(y: f32, k: usize) -> f32 {
    y + 30.0 + k as f32 * ROW + ROW / 2.0
}
