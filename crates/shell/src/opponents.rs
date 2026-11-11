//! A multiplayer stage's CPU opponents beside the field
//! (`docs/plans/multiplayer-prototype-progress.md`, M3, M6 and M6b;
//! `docs/plans/multiplayer-ui.md`, "Play and spectator layout" and
//! "Faithful miniature fields"), in the side space, never over the field.
//!
//! Where the sides have room, each opponent has a miniature field in a
//! stable slot ([`arrange`]): the tap-only copy of the chart it plays
//! scrolling to its receptors on this play's song clock, drawn by the
//! play's own noteskin on a band as the play's field's
//! (`ddi_render::scene::Miniature`, fed by its stream of judged moments,
//! `ddi_session::battle::feed`), its life gauge and combo by the play's
//! HUD placed on it as the play's are on the play's field (with room
//! above the gauge for the power bar to come), its judgments' hit
//! effects (the skin's where its components draw them, else a glow of the
//! judgment's color, [`draw_fields`]), and its name and level above it.
//! The slots are split between the two sides of the field, the first half
//! on the left, in the order the room seated them, in grids of the same
//! size, each side using its whole height clear of what is on it (the
//! HUD's score, its watermark, the quit button's place), and they never
//! move: one that is out stays in place, dimmed, saying so. A status line
//! under the right side's slots says "Stage won", and a battle's moment
//! that no announcer line said for a few seconds (fading, at once under
//! reduced motion). Where there are more opponents than slots at the
//! smallest miniature (a development-only practice far beyond the cap),
//! the opponents take turns in the slots ([`rotate`], [`ROTATION_V0`]),
//! but for those a stage keeps in view ([`priority_seats`]: none until
//! attacks exist).
//!
//! Where a side is too narrow for a miniature (a phone held upright),
//! or lyrics are shown over the play, the quiet column instead
//! ([`draw`]): one slot per opponent with its name and level, its life
//! (its own engine-judged gauge) and a quiet Out once it is gone; on a
//! narrow side only the numbered bars, on none at all nothing; as many
//! as the screen's height holds, then how many more.

use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_render::FieldGeometry;
use ddi_render::draw2d::{Affine, Blend, Command, DrawList, Mask, Quad, Shape, TextRun};
use ddi_session::battle::OpponentView;

/// The theme's text face (`crates/theme/src/infinito.toml`), with the
/// language's faces after it ([`ddi_l10n::font_stack`]).
const FACE: &str = "Iosevka Custom, Iosevka Term, monospace";

/// The column's width, logical pixels, at most, and the least a side
/// must have for names (else bars only), and for bars.
const WIDE: f32 = 172.0;
const NAMES: f32 = 96.0;
const BARS: f32 = 20.0;

const TEXT: [f32; 4] = [0.92, 0.92, 0.95, 1.0];
const MUTED: [f32; 4] = [0.80, 0.80, 0.86, 1.0];
const PANEL: [f32; 4] = [0.02, 0.01, 0.05, 0.55];
const TRACK: [f32; 4] = [1.0, 1.0, 1.0, 0.14];
/// Life: the HUD's mint, and its coral in danger.
const LIFE: [f32; 4] = [0.45, 0.86, 0.72, 0.95];
const LOW: [f32; 4] = [0.96, 0.49, 0.42, 0.95];
const WON: [f32; 4] = [0.98, 0.84, 0.45, 1.0];
/// Attack power: the HUD's electric violet, lighter when the bar is full.
const POWER: [f32; 4] = [0.55, 0.36, 1.0, 0.95];
const CHARGED: [f32; 4] = [0.80, 0.70, 1.0, 1.0];

/// What the column shows.
#[derive(Clone, Debug, Default)]
pub struct Column {
    pub opponents: Vec<OpponentView>,
    /// "Out" and "Stage won", worded.
    pub out: String,
    pub won: Option<String>,
    /// A battle's moment no announcer line said, worded, and how opaque,
    /// `0..=1`.
    pub cue: Option<(String, f32)>,
    /// "+n more": the opponents the column has no room for, worded
    /// (`None`: nothing said of them).
    pub more: Option<fn(usize) -> String>,
}

/// Where it went, for tests and the debug state: the column's rectangle
/// in the target's pixels (`[x, y, width, height]`) and whether it has
/// names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub rect: [f32; 4],
    pub names: bool,
    /// A moment's cue was drawn.
    pub cue: bool,
}

/// Draws `column` beside the field `geo` (`span` lanes wide) at `ratio`
/// target pixels to a logical one, clear of `keep_out` (`[left, top,
/// right, bottom]`); where it went, if anywhere.
pub fn draw(
    list: &mut DrawList,
    column: &Column,
    geo: &FieldGeometry,
    span: f32,
    ratio: f32,
    keep_out: &[[f32; 4]],
    measure: &mut dyn TextMeasure,
) -> Option<Placed> {
    let n = column.opponents.len();
    if n == 0 {
        return None;
    }
    let field = [geo.left, geo.left + geo.arrow * span];
    let left_room = field[0];
    let right_room = geo.width - field[1];
    let on_right = right_room >= left_room;
    let room = left_room.max(right_room) / ratio;
    if room < BARS {
        return None;
    }
    let names = room >= NAMES;
    let gap = 12.0_f32.min(room * 0.1);
    let w = (room - 2.0 * gap).min(WIDE);
    let row = if names { 36.0 } else { 14.0 };
    let head = if names { 22.0 } else { 0.0 } + if column.won.is_some() { 20.0 } else { 0.0 };
    let pad = if names { 10.0 } else { 4.0 };
    // As many rows as the screen's height holds; a line says how many more.
    let fits = ((geo.height / ratio - 2.0 * EDGE - head - 2.0 * pad) / row)
        .floor()
        .max(1.0) as usize;
    let (shown, more) = if n > fits {
        (
            fits.saturating_sub(1).max(1),
            n - fits.saturating_sub(1).max(1),
        )
    } else {
        (n, 0)
    };
    let h = head + row * (shown + usize::from(more > 0)) as f32 + 2.0 * pad;
    let x = if on_right {
        field[1] / ratio + gap
    } else {
        field[0] / ratio - gap - w
    };
    // In the middle of the screen's height, moved below what it would
    // cover of the HUD.
    let mut y = (geo.height / ratio - h) / 2.0;
    for k in keep_out {
        let k = k.map(|v| v / ratio);
        if x < k[2] && x + w > k[0] && y < k[3] && y + h > k[1] {
            y = k[3] + 8.0;
        }
    }
    let px = |v: f32| v * ratio;
    let rect = [px(x), px(y), px(w), px(h)];
    let mut panel = Quad::plain(
        [rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]],
        PANEL,
    );
    // In the rectangle's units: the target's pixels.
    panel.shape = Shape {
        radius: [px(8.0); 2],
        ..Shape::NONE
    };
    list.push(Command::Quad(panel));
    let face = ddi_l10n::font_stack(FACE);
    let style = |size: f32, weight: u16| {
        let mut s = TextStyle::new(face.as_str(), px(size));
        s.weight = weight;
        s
    };
    let mut text =
        |list: &mut DrawList, s: &str, style: &TextStyle, at: [f32; 2], right: bool, color| {
            let mut s = s.to_string();
            let room = px(w - 2.0 * pad);
            // Cut to the room, an ellipsis at the end.
            while !s.is_empty() && measure.measure(&s, style).advance > room {
                s.pop();
                while s.ends_with(' ') {
                    s.pop();
                }
                if s.is_empty() {
                    break;
                }
                s.pop();
                s.push('…');
            }
            let advance = measure.measure(&s, style).advance;
            let x = if right { at[0] - advance } else { at[0] };
            list.push(Command::Text(TextRun {
                curve: None,
                text: s.into(),
                style: style.clone(),
                transform: Affine::translate(x, at[1]),
                color,
                blend: Blend::Normal,
                mask: Mask::None,
                perspective: None,
                blur: [0.0; 2],
            }));
        };
    let (x0, x1) = (x + pad, x + w - pad);
    let mut top = y + pad;
    if let Some(won) = &column.won {
        text(list, won, &style(12.0, 700), [px(x0), px(top)], false, WON);
        top += 20.0;
    }
    let mut cued = false;
    if names {
        // A moment's line at the top while it shows (fading in and out).
        match column.cue.as_ref().filter(|(_, a)| *a > 0.0) {
            None => {}
            Some((cue, alpha)) => {
                let mut color = TEXT;
                color[3] *= alpha.clamp(0.0, 1.0);
                text(
                    list,
                    cue,
                    &style(12.5, 600),
                    [px(x0), px(top - 1.0)],
                    false,
                    color,
                );
                cued = true;
            }
        }
        top += 22.0;
    }
    for (i, o) in column.opponents.iter().enumerate().take(shown) {
        let ry = top + row * i as f32;
        let dim = if o.out { 0.5 } else { 1.0 };
        let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * dim];
        let bar_y = if names { ry + 20.0 } else { ry + 4.0 };
        let mut bar_x = x0;
        if names {
            text(
                list,
                &o.name,
                &style(14.0, 600),
                [px(x0), px(ry)],
                false,
                fade(TEXT),
            );
            let right = if o.out { &column.out } else { &o.level };
            text(
                list,
                right,
                &style(12.0, 400),
                [px(x1), px(ry + 1.0)],
                true,
                fade(MUTED),
            );
        } else {
            // Bars only: the CPU's number before its bar.
            let number = (i + 1).to_string();
            text(
                list,
                &number,
                &style(10.0, 600),
                [px(x0), px(ry - 3.0)],
                false,
                fade(MUTED),
            );
            bar_x = x0 + 9.0;
        }
        let bw = (x1 - bar_x).max(1.0);
        list.rect(
            [px(bar_x), px(bar_y), px(bar_x + bw), px(bar_y + 4.0)],
            TRACK,
        );
        let life = o.life.clamp(0.0, 1.0);
        if life > 0.0 && !o.out {
            let color = if life < 0.25 { LOW } else { LIFE };
            list.rect(
                [px(bar_x), px(bar_y), px(bar_x + bw * life), px(bar_y + 4.0)],
                color,
            );
        }
        // Its attack power: a thin line over its life, from the left.
        if let Some((power, charged)) = o.power.filter(|_| !o.out) {
            let power = power.clamp(0.0, 1.0);
            list.rect(
                [px(bar_x), px(bar_y - 3.0), px(bar_x + bw), px(bar_y - 1.0)],
                TRACK,
            );
            if power > 0.0 {
                list.rect(
                    [
                        px(bar_x),
                        px(bar_y - 3.0),
                        px(bar_x + bw * power),
                        px(bar_y - 1.0),
                    ],
                    if charged { CHARGED } else { POWER },
                );
            }
        }
    }
    if more > 0
        && let Some(words) = column.more
    {
        let ry = top + row * shown as f32;
        let size = if names { 12.0 } else { 10.0 };
        text(
            list,
            &words(more),
            &style(size, 500),
            [px(x0), px(ry)],
            false,
            MUTED,
        );
    }
    Some(Placed {
        rect,
        names,
        cue: cued,
    })
}

/// A miniature's lane, logical pixels: at least this (else fewer of them,
/// then the column), at most this share of the play's own lane, and at
/// most this.
const MINI_LANE_MIN: f32 = 16.0;
const MINI_LANE_SHARE: f32 = 0.45;
const MINI_LANE_MAX: f32 = 44.0;
/// A miniature's field, in its lanes' heights, and its receptors' place
/// from the end they are at: as the play's own field's (`FieldGeometry`:
/// a lane and a half), so that its life gauge, its combo and the power
/// bar to come sit on it as the play's do on the play's field
/// (`ddi_render::hud`).
const MINI_ROWS: f32 = 7.0;
const MINI_RECEPTOR: f32 = 1.5;
/// Above each field, logical pixels: its name line.
const NAME_LINE: f32 = 18.0;
const LABEL: f32 = NAME_LINE + 4.0;
/// The status line under one side's slots (the right's, else the left's):
/// a battle's moment no line said, and "Stage won".
const STATUS: f32 = 22.0;
/// Between slots, from the play's field's band, from the screen's edges and
/// from what the slots keep clear of.
const SLOT_GAP: f32 = 10.0;
const FIELD_GAP: f32 = 20.0;
const EDGE: f32 = 10.0;
const CLEAR: f32 = 6.0;
/// A field's band is wider than its lanes by this many lanes, as the
/// play's (`ddi_render::scene::filter_band`).
const BAND_MORE: f32 = 0.3;

/// Where an opponent's miniature goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    /// The seat it shows (an index into the room's order): its place's
    /// own until [`rotate`] says otherwise.
    pub seat: usize,
    /// The whole slot, its name line to its field's end, `[x, y, width,
    /// height]` in the target's pixels.
    pub rect: [f32; 4],
    /// Its field's geometry in the target's pixels.
    pub field: FieldGeometry,
    /// Its field's band, `[left, top, right, bottom]`.
    pub panel: [f32; 4],
    /// On the field's left.
    pub left: bool,
    /// How dark it is drawn over, `0..=1`: a slot whose seat is changing
    /// ([`rotate`]).
    pub dim: f32,
}

/// The opponents' miniatures' places ([`arrange`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Fields {
    /// The slots, the left side's first, each side's from the field out
    /// row by row.
    pub slots: Vec<Slot>,
    /// The status line under the right side's slots (the left's where
    /// there is no right side), `[x, y, width, height]`: a battle's moment
    /// no announcer line said, and "Stage won".
    pub status: [f32; 4],
    /// Fewer slots than opponents: whose miniature shows turns
    /// ([`rotate`]).
    pub rotating: bool,
    /// The miniatures' lane, logical pixels.
    pub lane: f32,
    /// Arrow heights shown past the receptors.
    pub range: f32,
}

/// Places `n` miniatures in the side space of the play's field `geo`
/// (`span` lanes wide, its notes scrolling the other way under
/// `reverse`) at `ratio` target pixels a logical one, clear of
/// `keep_out` (`[left, top, right, bottom]`, the target's pixels: the
/// HUD's score and watermark, the quit button's place, which the slots
/// keep clear of whether it shows or not so that they never move). Each
/// side uses its whole height but for what it keeps clear of on that side
/// (the watermark and the quit button at the left, the score at the
/// right) and, under one side, the status line. The first half (rounded
/// up) on the left, the rest on the right, each side a grid of slots of
/// one size, its columns from the field out.
///
/// Where all of them do not fit at [`MINI_LANE_MIN`] (more opponents than
/// the cap, a development-only practice; or a small window), as many
/// slots as fit at it, and the opponents take turns in them
/// ([`rotate`]); `None` where not even one fits (a phone held upright):
/// the column.
pub fn arrange(
    n: usize,
    geo: &FieldGeometry,
    span: f32,
    reverse: bool,
    ratio: f32,
    keep_out: &[[f32; 4]],
) -> Option<Fields> {
    if n == 0 || ratio <= 0.0 {
        return None;
    }
    let (w, h) = (geo.width / ratio, geo.height / ratio);
    // The field's band (its filter's, a little wider than the lanes).
    let [mid, band] = ddi_render::scene::filter_band(geo, span);
    let field = [
        geo.left.min(mid - band / 2.0) / ratio,
        (geo.left + geo.arrow * span).max(mid + band / 2.0) / ratio,
    ];
    let wings = [
        [EDGE, field[0] - FIELD_GAP],
        [field[1] + FIELD_GAP, w - EDGE],
    ];
    // The status line goes under the right side where there is one.
    let status_side = usize::from(n > 1);
    // Each side's band: below what is above it on that side, above what
    // is below, and the status line under one.
    let bands = wings.map(|[a, b]| {
        let (mut top, mut bottom) = (EDGE, h - EDGE);
        for k in keep_out {
            let k = k.map(|v| v / ratio);
            if !(k[0] < b && k[2] > a) {
                continue;
            }
            if (k[1] + k[3]) / 2.0 < h / 2.0 {
                top = top.max(k[3] + CLEAR);
            } else {
                bottom = bottom.min(k[1] - CLEAR);
            }
        }
        [top, bottom]
    });
    let rooms: [f32; 2] = std::array::from_fn(|side| {
        let [top, bottom] = bands[side];
        let status = if side == status_side { STATUS } else { 0.0 };
        (bottom - top - status).max(0.0)
    });
    let slot_w = |lane: f32| (4.0 + BAND_MORE) * lane;
    let slot_h = |lane: f32| LABEL + MINI_ROWS * lane;
    // How many slots a side holds at a lane: columns and rows.
    let holds = |width: f32, room: f32, lane: f32| -> (usize, usize) {
        let cols = ((width + SLOT_GAP) / (slot_w(lane) + SLOT_GAP))
            .floor()
            .max(0.0) as usize;
        let rows = ((room + SLOT_GAP) / (slot_h(lane) + SLOT_GAP))
            .floor()
            .max(0.0) as usize;
        (cols, rows)
    };
    let widths = wings.map(|[a, b]| (b - a).max(0.0));
    // The largest lane a side's share fits at, over its grids.
    let best = |width: f32, room: f32, k: usize| -> f32 {
        (1..=k)
            .map(|cols| {
                let rows = k.div_ceil(cols) as f32;
                let cw = (width - (cols as f32 - 1.0) * SLOT_GAP) / cols as f32;
                let ch = (room - (rows - 1.0) * SLOT_GAP) / rows;
                (cw / (4.0 + BAND_MORE)).min((ch - LABEL) / MINI_ROWS)
            })
            .fold(0.0, f32::max)
    };
    let cap = (geo.arrow / ratio * MINI_LANE_SHARE).min(MINI_LANE_MAX);
    let split = |n: usize| [n.div_ceil(2), n / 2];
    let lane_for = |counts: [usize; 2]| {
        (0..2)
            .filter(|&side| counts[side] > 0)
            .map(|side| best(widths[side], rooms[side], counts[side]))
            .fold(cap, f32::min)
            .floor()
    };
    let mut shown = n;
    let mut lane = lane_for(split(n));
    if lane < MINI_LANE_MIN {
        // As many as fit at the least lane, each side full; the rest take
        // turns.
        lane = MINI_LANE_MIN;
        let fit: usize = (0..2)
            .map(|side| {
                let (c, r) = holds(widths[side], rooms[side], lane);
                c * r
            })
            .sum();
        shown = fit.min(n);
        if shown == 0 {
            return None;
        }
        lane = lane_for(split(shown)).max(MINI_LANE_MIN).min(cap);
    }
    let counts = split(shown);
    let grids: Vec<Option<(usize, usize)>> = (0..2)
        .map(|side| {
            let k = counts[side];
            if k == 0 {
                return None;
            }
            // As few columns as the rows allow: a side's grid as square
            // as it can be.
            let (cols, rows) = holds(widths[side], rooms[side], lane);
            if cols == 0 || rows == 0 {
                return None;
            }
            let cols = k.div_ceil(rows).clamp(1, cols);
            let need = k.div_ceil(cols);
            (need <= rows).then_some((cols, need))
        })
        .collect();
    if grids
        .iter()
        .zip(&counts)
        .any(|(g, &k)| k > 0 && g.is_none())
    {
        return None;
    }
    let (sw, sh) = (slot_w(lane), slot_h(lane));
    let px = |v: f32| v * ratio;
    // Both sides' blocks on one middle where their rooms allow (the
    // middle of the room both sides share), else moved into their own.
    let shared = [
        bands[0][0].max(bands[1][0]),
        (0..2)
            .map(|side| bands[side][0] + rooms[side])
            .fold(f32::MAX, f32::min),
    ];
    let middle = (shared[0] + shared[1]) / 2.0;
    let mut slots = Vec::with_capacity(shown);
    let mut status = None;
    let mut seat = 0;
    for (side, grid) in grids.iter().enumerate() {
        let Some((cols, rows)) = *grid else {
            continue;
        };
        let k = counts[side];
        let used = cols as f32 * sw + (cols as f32 - 1.0) * SLOT_GAP;
        let block = rows as f32 * sh + (rows as f32 - 1.0) * SLOT_GAP;
        let [top, _] = bands[side];
        let y0 = (middle - block / 2.0)
            .min(top + rooms[side] - block)
            .max(top);
        let x0 = if side == 0 {
            wings[0][1] - used
        } else {
            wings[1][0]
        };
        if side == status_side || status.is_none() {
            // From the slots' left to the side's far edge.
            let to = if side == 0 { x0 + used } else { wings[1][1] };
            status = Some([
                px(x0),
                px(y0 + block + SLOT_GAP / 2.0),
                px(to - x0),
                px(STATUS),
            ]);
        }
        for i in 0..k {
            let (row, col) = ((i / cols) as f32, (i % cols) as f32);
            let x = x0 + col * (sw + SLOT_GAP);
            let y = y0 + row * (sh + SLOT_GAP);
            let (ft, fb) = (y + LABEL, y + sh);
            // Its lanes inside its band.
            let left = x + BAND_MORE / 2.0 * lane;
            let receptor = if reverse {
                fb - MINI_RECEPTOR * lane
            } else {
                ft + MINI_RECEPTOR * lane
            };
            slots.push(Slot {
                seat,
                rect: [px(x), px(y), px(sw), px(sh)],
                // Its camera on its own middle.
                field: FieldGeometry {
                    arrow: px(lane),
                    left: px(left),
                    receptor_y: px(receptor),
                    direction: if reverse { -1.0 } else { 1.0 },
                    width: px(2.0 * (left + 2.0 * lane)),
                    height: px(ft + fb),
                },
                panel: [px(x), px(ft), px(x + sw), px(fb)],
                left: side == 0,
                dim: 0.0,
            });
            seat += 1;
        }
    }
    Some(Fields {
        slots,
        status: status?,
        rotating: shown < n,
        lane,
        range: MINI_ROWS - MINI_RECEPTOR,
    })
}

/// How the opponents take turns in too few slots ([`rotate`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    /// Seconds each turn lasts.
    pub every: f32,
    /// Seconds a change takes, the slots darkening to it and back (none
    /// under reduced motion).
    pub fade: f32,
}

/// PROVISIONAL (`docs/plans/multiplayer-prototype-progress.md`, M7c):
/// a calm turn of eight seconds, the change darkening for 0.4 s.
pub const ROTATION_V0: Rotation = Rotation {
    every: 8.0,
    fade: 0.4,
};

/// Which seats the slots of `fields` show at song second `time`, of `n`
/// opponents, by `policy`: every seat in its own slot where all fit (they
/// never move); else the seats in `pinned` (the opponents the play must
/// keep in view: later, who attacks the player and whom the player
/// attacks) in the first slots for as long as they are pinned, and the
/// others taking turns in the rest, a turn every `policy.every` seconds:
/// each of those slots goes round its own seats (the `k`-th free slot
/// showing the `k`-th, then the `k + free`-th, and so on), so an opponent
/// always shows in the same place and a slot with one seat never changes.
/// A slot whose seat changes darkens through the change (`Slot::dim`),
/// at once under `reduced` motion.
pub fn rotate(
    fields: &mut Fields,
    n: usize,
    pinned: &[usize],
    time: f64,
    policy: Rotation,
    reduced: bool,
) {
    let slots = fields.slots.len();
    if !fields.rotating || slots == 0 {
        for (i, s) in fields.slots.iter_mut().enumerate() {
            s.seat = i;
            s.dim = 0.0;
        }
        return;
    }
    let mut kept: Vec<usize> = Vec::new();
    for &p in pinned {
        if p < n && !kept.contains(&p) && kept.len() < slots {
            kept.push(p);
        }
    }
    let others: Vec<usize> = (0..n).filter(|s| !kept.contains(s)).collect();
    let free = slots - kept.len();
    let every = f64::from(policy.every.max(0.1));
    let time = time.max(0.0);
    let turn = (time / every).floor() as usize;
    // Darkening around each change (none before the first).
    let since = time - turn as f64 * every;
    let near = if turn == 0 {
        every - since
    } else {
        since.min(every - since)
    };
    let half = f64::from(policy.fade) / 2.0;
    let dim = if reduced || half <= 0.0 {
        0.0
    } else {
        (1.0 - near / half).clamp(0.0, 1.0) as f32
    };
    for (i, s) in fields.slots.iter_mut().enumerate() {
        if i < kept.len() {
            s.seat = kept[i];
            s.dim = 0.0;
            continue;
        }
        // Its own seats: every `free`-th from its place.
        let k = i - kept.len();
        let own = others.len().saturating_sub(k).div_ceil(free.max(1)).max(1);
        s.seat = others
            .get(k + free * (turn % own))
            .or(others.get(k))
            .copied()
            .unwrap_or(0);
        s.dim = if own > 1 { dim } else { 0.0 };
    }
}

/// The seats a stage keeps in view whatever the turn ([`rotate`]): none
/// yet. Once attacks exist, the opponent attacking the player and the
/// one the player attacks.
pub fn priority_seats(_battle: &ddi_session::battle::Battle) -> Vec<usize> {
    Vec::new()
}

/// Cuts `s` to `room` pixels, an ellipsis at its end.
fn fitted(measure: &mut dyn TextMeasure, s: &str, style: &TextStyle, room: f32) -> String {
    let mut s = s.to_string();
    while !s.is_empty() && measure.measure(&s, style).advance > room {
        s.pop();
        while s.ends_with(' ') {
            s.pop();
        }
        if s.is_empty() {
            break;
        }
        s.pop();
        s.push('…');
    }
    s
}

fn run(text: String, style: &TextStyle, at: [f32; 2], color: [f32; 4]) -> Command {
    Command::Text(TextRun {
        curve: None,
        text: text.into(),
        style: style.clone(),
        transform: Affine::translate(at[0], at[1]),
        color,
        blend: Blend::Normal,
        mask: Mask::None,
        perspective: None,
        blur: [0.0; 2],
    })
}

/// What [`draw_fields`] adds to [`Column`]: each miniature's judgments
/// to flash on its receptors, where the play's noteskin does not draw
/// their hit effects (empty where it does), a slot's lanes'
/// `(judgment, seconds since)`.
pub type Flashes = Vec<Vec<Option<(ddi_engine::rules::Judgement, f32)>>>;

/// Draws what the miniatures in `fields` show around and over their
/// fields (drawn by the renderer before this, their life gauges and
/// combos by the play's HUD): each one's name and level above its field,
/// the flashes of its judgments where the skin does not draw them
/// (`flashes`, in `0..`[`FLASH`] seconds), its Out, a slot darkening as
/// its seat changes; in the status line, "Stage won" and a battle's moment
/// no line said. Whether a moment's cue was drawn.
pub fn draw_fields(
    list: &mut DrawList,
    column: &Column,
    fields: &Fields,
    flashes: &[Vec<Option<(ddi_engine::rules::Judgement, f32)>>],
    ratio: f32,
    measure: &mut dyn TextMeasure,
) -> bool {
    let face = ddi_l10n::font_stack(FACE);
    let style = |size: f32, weight: u16| {
        let mut s = TextStyle::new(face.as_str(), size * ratio);
        s.weight = weight;
        s
    };
    // The status line: "Stage won", then the moment, at its left.
    let [sx, sy, sw, _] = fields.status;
    let mut x = sx;
    if let Some(won) = &column.won {
        let st = style(14.0, 700);
        let t = fitted(measure, won, &st, sw);
        x += measure.measure(&t, &st).advance + 12.0 * ratio;
        list.push(run(t, &st, [sx, sy], WON));
    }
    let cue = column.cue.as_ref().filter(|(_, a)| *a > 0.0);
    if let Some((cue, alpha)) = cue {
        let st = style(14.0, 600);
        let t = fitted(measure, cue, &st, (sx + sw - x).max(0.0));
        let mut c = TEXT;
        c[3] *= alpha.clamp(0.0, 1.0);
        list.push(run(t, &st, [x, sy], c));
    }
    for (i, slot) in fields.slots.iter().enumerate() {
        let Some(o) = column.opponents.get(slot.seat) else {
            continue;
        };
        let [x, y, w, h] = slot.rect;
        let dim = if o.out { 0.55 } else { 1.0 };
        let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * dim];
        // Name, then its level (or Out) at the right; smaller where the
        // slot is narrow.
        let size = if w / ratio < 100.0 { 12.0 } else { 14.0 };
        let right_style = style(size - 1.5, 400);
        let right = if o.out { &column.out } else { &o.level };
        let right = fitted(measure, right, &right_style, w * 0.45);
        let right_w = measure.measure(&right, &right_style).advance;
        let name_style = style(size, 600);
        // The name whole before the level: where both do not fit, the
        // level goes (Out shows on the field too).
        let whole = measure.measure(&o.name, &name_style).advance;
        let level = whole + right_w + 6.0 * ratio <= w;
        let room = if level { w - right_w - 6.0 * ratio } else { w };
        let name = fitted(measure, &o.name, &name_style, room.max(0.0));
        list.push(run(name, &name_style, [x, y], fade(TEXT)));
        if level {
            list.push(run(
                right,
                &right_style,
                [x + w - right_w, y + 1.5 * ratio],
                fade(MUTED),
            ));
        }
        // The judgments' flashes where the skin draws no hit effect.
        let geo = &slot.field;
        for (lane, flash) in flashes.get(i).into_iter().flatten().enumerate() {
            let Some((j, age)) = *flash else {
                continue;
            };
            let k = (1.0 - age / FLASH).clamp(0.0, 1.0);
            if k <= 0.0 {
                continue;
            }
            // A soft glow round the receptor, in the judgment's color.
            let mut c = ddi_render::scene::judgement_color(j);
            c[3] = 0.55 * k;
            let (cx, cy) = (geo.lane_x(lane as f32), geo.receptor_y);
            let r = geo.arrow * 0.55;
            let mut q = Quad::plain([cx - r, cy - r, cx + r, cy + r], c);
            // In the rectangle's units: the target's pixels.
            q.shape = Shape {
                radius: [r; 2],
                soft: [r * 0.5; 2],
                ..Shape::NONE
            };
            q.blend = Blend::Add;
            list.push(Command::Quad(q));
        }
        // Out: the field dimmed, in place, saying so.
        if o.out {
            let [l, t, r, b] = slot.panel;
            list.rect([l, t, r, b], [0.0, 0.0, 0.0, 0.45]);
            let st = style(13.0, 600);
            let word = fitted(measure, &column.out, &st, r - l);
            let ww = measure.measure(&word, &st).advance;
            list.push(run(
                word,
                &st,
                [(l + r - ww) / 2.0, (t + b) / 2.0 - 8.0 * ratio],
                MUTED,
            ));
        }
        // Its seat changing: the whole slot darkens through the change.
        if slot.dim > 0.0 {
            list.rect([x, y, x + w, y + h], [0.0, 0.0, 0.0, 0.9 * slot.dim]);
        }
    }
    cue.is_some()
}

/// How long [`draw_fields`] flashes a judgment, seconds.
pub const FLASH: f32 = ddi_session::battle::feed::FLASH_SECONDS;

/// Whether stages show the miniatures (on by default): a development
/// switch for comparing their cost with the column's
/// (`DDI_MINIATURES=0` in the environment; on the web `localStorage`'s
/// `ddi.dev.miniatures` set to `0`, read by the play's loop).
static MINIATURES: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_miniatures(on: bool) {
    MINIATURES.store(if on { 1 } else { 2 }, std::sync::atomic::Ordering::Relaxed);
}

pub fn miniatures() -> bool {
    match MINIATURES.load(std::sync::atomic::Ordering::Relaxed) {
        0 => {
            let on = std::env::var("DDI_MINIATURES").map_or(true, |v| v != "0");
            set_miniatures(on);
            on
        }
        v => v == 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_actor::text::FixedMeasure;
    use ddi_room::ParticipantId;

    fn column(n: usize) -> Column {
        Column {
            opponents: (0..n)
                .map(|i| OpponentView {
                    participant: ParticipantId(i as u64 + 2),
                    name: format!("CPU {}", i + 1),
                    level: "Lv 3".into(),
                    life: 0.5,
                    out: i == 1,
                    score: 0.9,
                    combo: 10,
                    power: None,
                })
                .collect(),
            out: "Out".into(),
            won: None,
            cue: None,
            more: None,
        }
    }

    fn texts(list: &DrawList) -> Vec<(String, f32)> {
        list.commands
            .iter()
            .filter_map(|c| match c {
                Command::Text(t) => Some((t.text.to_string(), t.color[3])),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_moment_shows_at_the_column_s_top_while_it_shows() {
        let g = geo(1280.0, 720.0);
        let plain = draw(
            &mut DrawList::new(),
            &column(3),
            &g,
            4.0,
            1.0,
            &[],
            &mut FixedMeasure,
        )
        .unwrap();
        assert!(!plain.cue);
        // Shown whole: the heading gone, the column where it was.
        let mut c = column(3);
        c.cue = Some(("Final stage".into(), 1.0));
        let mut list = DrawList::new();
        let placed = draw(&mut list, &c, &g, 4.0, 1.0, &[], &mut FixedMeasure).unwrap();
        assert!(placed.cue);
        assert_eq!(placed.rect, plain.rect);
        let t = texts(&list);
        assert!(t.iter().any(|(s, a)| s == "Final stage" && *a > 0.9));
        // Fading: partly.
        c.cue = Some(("Final stage".into(), 0.5));
        let mut list = DrawList::new();
        draw(&mut list, &c, &g, 4.0, 1.0, &[], &mut FixedMeasure).unwrap();
        let t = texts(&list);
        assert!(t.iter().any(|(s, a)| s == "Final stage" && *a < 0.9));
        // Gone: nothing there, and no heading ever.
        c.cue = Some(("Final stage".into(), 0.0));
        let mut list = DrawList::new();
        assert!(
            !draw(&mut list, &c, &g, 4.0, 1.0, &[], &mut FixedMeasure)
                .unwrap()
                .cue
        );
        assert!(!texts(&list).iter().any(|(s, _)| s == "Final stage"));
    }

    fn geo(w: f32, h: f32) -> FieldGeometry {
        FieldGeometry::placed(w, h, 4.0, false, Default::default())
    }

    #[test]
    fn beside_the_field_never_over_it() {
        let g = geo(1280.0, 720.0);
        let mut list = DrawList::new();
        let placed = draw(&mut list, &column(3), &g, 4.0, 1.0, &[], &mut FixedMeasure).unwrap();
        assert!(placed.names);
        let [x, _, w, _] = placed.rect;
        let field = [g.left, g.left + g.arrow * 4.0];
        assert!(x >= field[1] || x + w <= field[0], "{placed:?} {field:?}");
        // The Out one says so instead of its level; the others their level.
        let texts: Vec<String> = list
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::Text(t) => Some(t.text.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(texts.iter().filter(|t| *t == "Out").count(), 1);
        assert_eq!(texts.iter().filter(|t| *t == "Lv 3").count(), 2);
    }

    #[test]
    fn a_phone_shows_bars_or_nothing() {
        // Upright: little room either side of the field.
        let g = geo(390.0, 844.0);
        let mut list = DrawList::new();
        let placed = draw(&mut list, &column(7), &g, 4.0, 1.0, &[], &mut FixedMeasure);
        if let Some(p) = placed {
            assert!(!p.names);
            let field = [g.left, g.left + g.arrow * 4.0];
            assert!(p.rect[0] >= field[1] || p.rect[0] + p.rect[2] <= field[0]);
        }
        assert!(
            draw(
                &mut DrawList::new(),
                &column(0),
                &g,
                4.0,
                1.0,
                &[],
                &mut FixedMeasure
            )
            .is_none()
        );
    }

    #[test]
    fn it_moves_below_the_hud_it_would_cover() {
        let g = geo(1280.0, 720.0);
        let first = draw(
            &mut DrawList::new(),
            &column(7),
            &g,
            4.0,
            1.0,
            &[],
            &mut FixedMeasure,
        )
        .unwrap();
        let [x, y, w, _] = first.rect;
        let keep = [x, y - 10.0, x + w, y + 30.0];
        let moved = draw(
            &mut DrawList::new(),
            &column(7),
            &g,
            4.0,
            1.0,
            &[keep],
            &mut FixedMeasure,
        )
        .unwrap();
        assert!(moved.rect[1] >= keep[3]);
    }

    fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
    }

    fn ltrb([x, y, w, h]: [f32; 4]) -> [f32; 4] {
        [x, y, x + w, y + h]
    }

    /// The play's own keep-outs: its HUD and the quit button's place.
    fn keep(g: &FieldGeometry, ratio: f32) -> Vec<[f32; 4]> {
        let mut k = ddi_render::scene::hud_areas(g, 4.0);
        let [x, y, r] =
            ddi_render::scene::quit_button(g, &ddi_chart::Layout::dance_single(), ratio, [0.0; 4]);
        k.push([x - r, y - r, x + r, y + r]);
        k
    }

    #[test]
    fn eight_miniatures_in_stable_slots_beside_the_field() {
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0), (844.0, 390.0)] {
            let g = geo(w, h);
            let k = keep(&g, 1.0);
            let f = arrange(8, &g, 4.0, false, 1.0, &k).unwrap_or_else(|| panic!("{w}x{h}"));
            assert_eq!(f.slots.len(), 8);
            assert!(!f.rotating);
            // The first four on the left, the rest on the right, in order.
            assert!(f.slots[..4].iter().all(|s| s.left));
            assert!(f.slots[4..].iter().all(|s| !s.left));
            assert!(f.slots.iter().enumerate().all(|(i, s)| s.seat == i));
            // The field's band, a little wider than its lanes.
            let [mid, band] = ddi_render::scene::filter_band(&g, 4.0);
            let field = [mid - band / 2.0, 0.0, mid + band / 2.0, h];
            for (i, s) in f.slots.iter().enumerate() {
                let r = ltrb(s.rect);
                // On the screen, never over the field or the HUD.
                assert!(
                    r[0] >= 0.0 && r[2] <= w && r[1] >= 0.0 && r[3] <= h,
                    "{r:?}"
                );
                assert!(!overlaps(r, field), "{w}x{h} slot {i} over the field");
                for k in &k {
                    assert!(!overlaps(r, *k), "{w}x{h} slot {i} {r:?} over {k:?}");
                }
                // Never over another, all the same size.
                for t in &f.slots[i + 1..] {
                    assert!(!overlaps(r, ltrb(t.rect)));
                    assert_eq!([s.rect[2], s.rect[3]], [t.rect[2], t.rect[3]]);
                }
                // Its field inside its band, its receptors a lane and a
                // half from its top, as the play's are from the screen's.
                let a = s.field.arrow;
                assert!(s.field.lane_x(0.0) - a / 2.0 > s.panel[0]);
                assert!(s.field.lane_x(3.0) + a / 2.0 < s.panel[2]);
                assert!((s.field.receptor_y - s.panel[1] - 1.5 * a).abs() < 0.01);
                assert!(s.panel[1] >= r[1] && s.panel[3] <= r[3] + 0.01);
            }
            // Smaller than the play's own lanes.
            assert!(f.slots[0].field.arrow < g.arrow, "{w}x{h}");
            assert!(f.lane >= MINI_LANE_MIN);
        }
    }

    #[test]
    fn fewer_miniatures_are_larger_up_to_their_bound() {
        let g = geo(1280.0, 720.0);
        let k = keep(&g, 1.0);
        let eight = arrange(8, &g, 4.0, false, 1.0, &k).unwrap();
        let two = arrange(2, &g, 4.0, false, 1.0, &k).unwrap();
        assert_eq!(two.slots.len(), 2);
        assert!(two.slots[0].left && !two.slots[1].left);
        assert!(two.lane >= eight.lane);
        assert!(two.lane <= g.arrow * MINI_LANE_SHARE);
        // One alone: on the left, its status line under it.
        let one = arrange(1, &g, 4.0, false, 1.0, &k).unwrap();
        assert!(one.slots[0].left);
        assert!(one.status[0] < g.left);
        // Two or more: the status line under the right side.
        assert!(two.status[0] > g.left);
    }

    #[test]
    fn reverse_puts_the_miniatures_receptors_at_the_bottom() {
        let g = FieldGeometry::placed(1280.0, 720.0, 4.0, true, Default::default());
        let k = keep(&g, 1.0);
        let f = arrange(3, &g, 4.0, true, 1.0, &k).unwrap();
        for s in &f.slots {
            assert!(s.field.direction < 0.0);
            assert!(s.field.receptor_y > (s.panel[1] + s.panel[3]) / 2.0);
        }
    }

    #[test]
    fn a_phone_held_upright_falls_back_to_the_column() {
        // In CSS pixels at 3 target pixels each.
        let ratio = 3.0;
        let g = geo(390.0 * ratio, 844.0 * ratio);
        assert!(arrange(7, &g, 4.0, false, ratio, &keep(&g, ratio)).is_none());
        assert!(arrange(2, &g, 4.0, false, ratio, &keep(&g, ratio)).is_none());
        // Turned: room for them.
        let g = geo(844.0 * ratio, 390.0 * ratio);
        let f = arrange(7, &g, 4.0, false, ratio, &keep(&g, ratio)).unwrap();
        assert_eq!(f.slots.len(), 7);
        assert!(arrange(0, &g, 4.0, false, ratio, &[]).is_none());
    }

    #[test]
    fn out_stays_in_place_saying_so() {
        let g = geo(1280.0, 720.0);
        let f = arrange(3, &g, 4.0, false, 1.0, &keep(&g, 1.0)).unwrap();
        let c = column(3);
        let mut list = DrawList::new();
        let flashes = vec![
            vec![None; 4],
            vec![
                Some((ddi_engine::rules::Judgement::W1, 0.1)),
                None,
                None,
                None,
            ],
            vec![None; 4],
        ];
        assert!(!draw_fields(
            &mut list,
            &c,
            &f,
            &flashes,
            1.0,
            &mut FixedMeasure
        ));
        let t = texts(&list);
        // The second is out: its slot says so, twice (its line, its field).
        assert_eq!(t.iter().filter(|(s, _)| s == "Out").count(), 2);
        assert_eq!(t.iter().filter(|(s, _)| s == "Lv 3").count(), 2);
        // A moment and the stage won in the status line.
        let mut c = column(3);
        c.cue = Some(("Final stage".into(), 1.0));
        c.won = Some("Stage won".into());
        let mut list = DrawList::new();
        assert!(draw_fields(&mut list, &c, &f, &[], 1.0, &mut FixedMeasure));
        let t = texts(&list);
        assert!(t.iter().any(|(s, _)| s == "Final stage"));
        assert!(t.iter().any(|(s, _)| s == "Stage won"));
    }

    #[test]
    fn far_more_opponents_than_room_take_turns_in_the_slots() {
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0)] {
            let g = geo(w, h);
            let k = keep(&g, 1.0);
            let mut last = 0;
            for n in [16, 32, 64, 128] {
                let mut f = arrange(n, &g, 4.0, false, 1.0, &k).unwrap();
                assert!(f.slots.len() <= n, "{w}x{h} {n}");
                assert_eq!(f.rotating, f.slots.len() < n);
                assert!(f.lane >= MINI_LANE_MIN);
                // Never over the field, the HUD or each other; the status
                // line on the screen.
                let [mid, band] = ddi_render::scene::filter_band(&g, 4.0);
                for (i, s) in f.slots.iter().enumerate() {
                    let r = ltrb(s.rect);
                    assert!(r[0] >= 0.0 && r[2] <= w && r[1] >= 0.0 && r[3] <= h);
                    assert!(r[2] <= mid - band / 2.0 || r[0] >= mid + band / 2.0);
                    assert!(k.iter().all(|k| !overlaps(r, *k)));
                    assert!(f.slots[i + 1..].iter().all(|t| !overlaps(r, ltrb(t.rect))));
                }
                assert!(f.status[1] + f.status[3] <= h);
                if f.rotating {
                    assert!(f.slots.len() >= last, "{n}: as many as fit");
                    // Everyone has a turn, every slot always filled, no
                    // seat twice at once.
                    let mut seen = std::collections::BTreeSet::new();
                    let turns = n.div_ceil(f.slots.len());
                    let mut place = std::collections::BTreeMap::new();
                    for turn in 0..turns {
                        let t = f64::from(ROTATION_V0.every) * (turn as f64 + 0.5);
                        rotate(&mut f, n, &[], t, ROTATION_V0, false);
                        let seats: std::collections::BTreeSet<_> =
                            f.slots.iter().map(|s| s.seat).collect();
                        assert_eq!(seats.len(), f.slots.len());
                        seen.extend(seats);
                        // An opponent always in the same slot.
                        for (i, s) in f.slots.iter().enumerate() {
                            assert_eq!(*place.entry(s.seat).or_insert(i), i, "{n}");
                        }
                    }
                    assert_eq!(seen.len(), n, "{w}x{h} {n}");
                }
                last = f.slots.len();
            }
        }
    }

    #[test]
    fn the_sides_use_their_own_height() {
        // The score keeps the right side lower; the left, with only the
        // watermark and the quit button below, starts at the top.
        let g = geo(1920.0, 1080.0);
        let f = arrange(128, &g, 4.0, false, 1.0, &keep(&g, 1.0)).unwrap();
        let top = |left: bool| {
            f.slots
                .iter()
                .filter(|s| s.left == left)
                .map(|s| s.rect[1])
                .fold(f32::MAX, f32::min)
        };
        assert!(top(true) < top(false), "{} {}", top(true), top(false));
        // Above where the score ends.
        let score = ddi_render::scene::hud_areas(&g, 4.0)[1];
        assert!(top(true) < score[3], "{} {score:?}", top(true));
    }

    #[test]
    fn pinned_seats_stay_while_the_rest_turn() {
        let g = geo(1280.0, 720.0);
        let mut f = arrange(64, &g, 4.0, false, 1.0, &keep(&g, 1.0)).unwrap();
        assert!(f.rotating);
        let every = f64::from(ROTATION_V0.every);
        // A stub's priority: seats 40 and 7 (later: the attacker and the
        // attacked), always in the first slots.
        let pinned = [40, 7];
        let mut windows = Vec::new();
        for turn in 0..4 {
            rotate(
                &mut f,
                64,
                &pinned,
                every * (turn as f64 + 0.5),
                ROTATION_V0,
                false,
            );
            assert_eq!(f.slots[0].seat, 40);
            assert_eq!(f.slots[1].seat, 7);
            assert!(f.slots[2..].iter().all(|s| s.seat != 40 && s.seat != 7));
            windows.push(f.slots[2].seat);
        }
        assert!(windows.windows(2).all(|w| w[0] != w[1]), "{windows:?}");
        // A slot with one seat of its own never changes or darkens.
        let mut f = arrange(128, &g, 4.0, false, 1.0, &keep(&g, 1.0)).unwrap();
        let free = f.slots.len();
        let last = free - 1;
        if 128 - last <= free {
            rotate(&mut f, 128, &[], every, ROTATION_V0, false);
            let a = f.slots[last].seat;
            assert_eq!(f.slots[last].dim, 0.0);
            rotate(&mut f, 128, &[], every * 2.5, ROTATION_V0, false);
            assert_eq!(f.slots[last].seat, a);
        }
        // A change darkens the turning slots, never the pinned ones, and
        // not at all under reduced motion; between changes, nothing.
        rotate(&mut f, 64, &pinned, every, ROTATION_V0, false);
        assert_eq!(f.slots[0].dim, 0.0);
        assert!(f.slots[2].dim > 0.9);
        rotate(&mut f, 64, &pinned, every, ROTATION_V0, true);
        assert!(f.slots.iter().all(|s| s.dim == 0.0));
        rotate(&mut f, 64, &pinned, every * 1.5, ROTATION_V0, false);
        assert!(f.slots.iter().all(|s| s.dim == 0.0));
        // The first turn starts without a change.
        rotate(&mut f, 64, &pinned, 0.0, ROTATION_V0, false);
        assert!(f.slots.iter().all(|s| s.dim == 0.0));
        // All fitting: seat order, pins ignored, never moving.
        let mut eight = arrange(8, &g, 4.0, false, 1.0, &keep(&g, 1.0)).unwrap();
        rotate(&mut eight, 8, &pinned, every * 3.0, ROTATION_V0, false);
        assert!(eight.slots.iter().enumerate().all(|(i, s)| s.seat == i));
    }

    #[test]
    fn the_column_holds_what_the_height_holds_then_a_count() {
        let g = geo(390.0, 844.0);
        let mut c = column(128);
        c.more = Some(|n| format!("+{n} more"));
        let mut list = DrawList::new();
        let placed = draw(&mut list, &c, &g, 4.0, 1.0, &[], &mut FixedMeasure).unwrap();
        assert!(placed.rect[1] >= 0.0 && placed.rect[1] + placed.rect[3] <= 844.0);
        assert!(
            texts(&list)
                .iter()
                .any(|(s, _)| s.starts_with('+') && s.ends_with("more"))
        );
    }
}
