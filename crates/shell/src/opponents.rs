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
//! size kept clear of the HUD's score, watermark and the quit button's
//! place, and they never move: one that is out stays in place, dimmed,
//! saying so. A heading above the left side says the CPUs play a
//! tap-only copy; a battle's moment that no announcer line said takes its
//! place for a few seconds (fading, at once under reduced motion), and
//! "Stage won" shows above the right side. Where there are more opponents
//! than slots at the smallest miniature (a development-only practice far
//! beyond the cap), the first seats have one and a line says how many
//! more.
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

/// What the column shows.
#[derive(Clone, Debug, Default)]
pub struct Column {
    pub opponents: Vec<OpponentView>,
    /// "CPUs · taps-only copy", "Out" and "Stage won", worded.
    pub heading: String,
    pub out: String,
    pub won: Option<String>,
    /// A battle's moment shown in the heading's place, worded, and how
    /// opaque, `0..=1`.
    pub cue: Option<(String, f32)>,
    /// "+n more": the opponents there is no room for, worded (`None`:
    /// nothing said of them).
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
        // The cue alone while it shows (fading in and out), never over
        // the heading.
        match column.cue.as_ref().filter(|(_, a)| *a > 0.0) {
            None => text(
                list,
                &column.heading,
                &style(12.0, 500),
                [px(x0), px(top)],
                false,
                MUTED,
            ),
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
/// The heading line above the slots.
const HEADING: f32 = 22.0;
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
    /// The seat it shows (an index into the room's order).
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
}

/// The opponents' miniatures' places ([`arrange`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Fields {
    /// One a seat shown, in the order the room seated them.
    pub slots: Vec<Slot>,
    /// The heading's line above the left side's slots, and above the
    /// right side's (where "Stage won" shows), `[x, y, width, height]`:
    /// the side's room, from its edge to its slots' far end; a heading
    /// starts over its first slot ([`Fields::heading_start`]) and moves
    /// back into that room where it is longer.
    pub heading: [f32; 4],
    pub heading_start: f32,
    pub right_heading: Option<[f32; 4]>,
    /// The seats with no room, the last ones: said in a line under the
    /// right side's slots ("+n more").
    pub more: usize,
    /// Where that line goes, `[x, y]`.
    pub more_at: [f32; 2],
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
/// keep clear of whether it shows or not so that they never move: each
/// moves the slots' band below or above it). The first half (rounded up)
/// on the left, the rest on the right, each side a grid of the same size,
/// its columns from the field out.
///
/// Where all of them do not fit at [`MINI_LANE_MIN`] (far more opponents
/// than the cap, a development-only practice; or a small window), as many
/// as fit at it, the first seats, and a line saying how many more
/// (PROVISIONAL: the design leaves the density of large parties open);
/// `None` where not even one fits (a phone held upright): the column.
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
    // One band for both sides, below what is above it, above what is
    // below.
    let (mut top, mut bottom) = (EDGE, h - EDGE);
    for k in keep_out {
        let k = k.map(|v| v / ratio);
        if !wings.iter().any(|[a, b]| k[0] < *b && k[2] > *a) {
            continue;
        }
        if (k[1] + k[3]) / 2.0 < h / 2.0 {
            top = top.max(k[3] + CLEAR);
        } else {
            bottom = bottom.min(k[1] - CLEAR);
        }
    }
    let room = bottom - top - HEADING;
    let slot_w = |lane: f32| (4.0 + BAND_MORE) * lane;
    let slot_h = |lane: f32| LABEL + MINI_ROWS * lane;
    // How many slots a side holds at a lane: columns and rows.
    let holds = |width: f32, lane: f32| -> (usize, usize) {
        let cols = ((width + SLOT_GAP) / (slot_w(lane) + SLOT_GAP))
            .floor()
            .max(0.0) as usize;
        let rows = ((room + SLOT_GAP) / (slot_h(lane) + SLOT_GAP))
            .floor()
            .max(0.0) as usize;
        (cols, rows)
    };
    let widths = wings.map(|[a, b]| (b - a).max(0.0));
    // The largest lane each side's share fits at, over its grids.
    let best = |width: f32, k: usize| -> f32 {
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
        counts
            .iter()
            .zip(&widths)
            .filter(|(k, _)| **k > 0)
            .map(|(k, w)| best(*w, *k))
            .fold(cap, f32::min)
            .floor()
    };
    let mut shown = n;
    let mut lane = lane_for(split(n));
    if lane < MINI_LANE_MIN {
        // As many as fit at the least lane, each side full.
        lane = MINI_LANE_MIN;
        let fit: usize = widths
            .iter()
            .map(|w| {
                let (c, r) = holds(*w, lane);
                c * r
            })
            .sum();
        shown = fit.min(n);
        // A line for the rest: one slot's place fewer.
        if shown < n {
            shown = shown.saturating_sub(1);
        }
        if shown == 0 {
            return None;
        }
        lane = lane_for(split(shown)).max(MINI_LANE_MIN).min(cap);
    }
    let counts = split(shown);
    let grids: Vec<Option<(usize, usize)>> = counts
        .iter()
        .zip(&widths)
        .map(|(&k, &width)| {
            if k == 0 {
                return None;
            }
            // As few columns as the rows allow: a side's grid as square
            // as it can be.
            let (cols, rows) = holds(width, lane);
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
    let rows = grids.iter().flatten().map(|g| g.1).max().unwrap_or(1) as f32;
    let block = HEADING + rows * sh + (rows - 1.0) * SLOT_GAP;
    let y0 = top + ((bottom - top - block) / 2.0).max(0.0);
    let px = |v: f32| v * ratio;
    let mut slots = Vec::with_capacity(shown);
    let mut headings = [None, None];
    let mut more_at = [0.0; 2];
    let mut seat = 0;
    for (side, grid) in grids.iter().enumerate() {
        let Some((cols, used_rows)) = *grid else {
            continue;
        };
        let k = counts[side];
        let used = cols as f32 * sw + (cols as f32 - 1.0) * SLOT_GAP;
        let x0 = if side == 0 {
            wings[0][1] - used
        } else {
            wings[1][0]
        };
        headings[side] = Some(if side == 0 {
            let from = wings[0][0];
            (
                [px(from), px(y0), px(x0 + used - from), px(HEADING)],
                px(x0),
            )
        } else {
            ([px(x0), px(y0), px(wings[1][1] - x0), px(HEADING)], px(x0))
        });
        more_at = [
            px(x0),
            px(y0 + HEADING + used_rows as f32 * (sh + SLOT_GAP)),
        ];
        for i in 0..k {
            let (row, col) = ((i / cols) as f32, (i % cols) as f32);
            let x = x0 + col * (sw + SLOT_GAP);
            let y = y0 + HEADING + row * (sh + SLOT_GAP);
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
            });
            seat += 1;
        }
    }
    let (heading, right_heading) = match headings {
        [Some(l), r] => (l, r.map(|r| r.0)),
        [None, r] => (r?, None),
    };
    Some(Fields {
        slots,
        heading: heading.0,
        heading_start: heading.1,
        right_heading,
        more: n - shown,
        more_at,
        lane,
        range: MINI_ROWS - MINI_RECEPTOR,
    })
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
/// (`flashes`, in `0..`[`FLASH`] seconds), its Out; the heading, the
/// battle's moment or "Stage won" above the slots, and how many more
/// there are than slots. Whether a moment's cue was drawn.
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
    let [hx, hy, hw, _] = fields.heading;
    // Over the first slot, moved back into the side's room if longer.
    let mut at = |t: &str, st: &TextStyle| {
        let t = fitted(measure, t, st, hw);
        let w = measure.measure(&t, st).advance;
        (fields.heading_start.min(hx + hw - w).max(hx), t)
    };
    let cued = column.cue.as_ref().is_some_and(|(_, a)| *a > 0.0);
    match column.cue.as_ref().filter(|(_, a)| *a > 0.0) {
        None => {
            let st = style(13.0, 500);
            let (x, t) = at(&column.heading, &st);
            list.push(run(t, &st, [x, hy], MUTED));
        }
        Some((cue, alpha)) => {
            let st = style(14.0, 600);
            let (x, t) = at(cue, &st);
            let mut c = TEXT;
            c[3] *= alpha.clamp(0.0, 1.0);
            list.push(run(t, &st, [x, hy - ratio], c));
        }
    }
    if let Some(won) = &column.won {
        let st = style(14.0, 700);
        let [x, y, w, _] = fields.right_heading.unwrap_or(fields.heading);
        // Without a right side, above the heading's line.
        let (x, y) = if fields.right_heading.is_some() {
            (x, y)
        } else {
            (fields.heading_start, y - HEADING * ratio)
        };
        let t = fitted(measure, won, &st, w);
        list.push(run(t, &st, [x, y], WON));
    }
    if fields.more > 0
        && let Some(words) = column.more
    {
        let st = style(13.0, 500);
        list.push(run(words(fields.more), &st, fields.more_at, MUTED));
    }
    for (i, slot) in fields.slots.iter().enumerate() {
        let Some(o) = column.opponents.get(slot.seat) else {
            continue;
        };
        let [x, y, w, _] = slot.rect;
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
        let name = fitted(
            measure,
            &o.name,
            &name_style,
            (w - right_w - 6.0 * ratio).max(0.0),
        );
        list.push(run(name, &name_style, [x, y], fade(TEXT)));
        list.push(run(
            right,
            &right_style,
            [x + w - right_w, y + 1.5 * ratio],
            fade(MUTED),
        ));
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
    }
    cued
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
                })
                .collect(),
            heading: "CPUs · taps-only copy".into(),
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
    fn a_moment_takes_the_heading_s_place_while_it_shows() {
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
        assert!(!t.iter().any(|(s, _)| s.starts_with("CPUs")));
        // Fading: the cue alone, partly.
        c.cue = Some(("Final stage".into(), 0.5));
        let mut list = DrawList::new();
        draw(&mut list, &c, &g, 4.0, 1.0, &[], &mut FixedMeasure).unwrap();
        let t = texts(&list);
        assert!(t.iter().any(|(s, a)| s == "Final stage" && *a < 0.9));
        assert!(!t.iter().any(|(s, _)| s.starts_with("CPUs")));
        // Gone: the heading back.
        c.cue = Some(("Final stage".into(), 0.0));
        let mut list = DrawList::new();
        assert!(
            !draw(&mut list, &c, &g, 4.0, 1.0, &[], &mut FixedMeasure)
                .unwrap()
                .cue
        );
        assert!(texts(&list).iter().any(|(s, _)| s.starts_with("CPUs")));
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
        assert!(texts.iter().any(|t| t.starts_with("CPUs")));
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
            assert_eq!(f.more, 0);
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
        // One alone: on the left, no right side.
        let one = arrange(1, &g, 4.0, false, 1.0, &k).unwrap();
        assert!(one.slots[0].left && one.right_heading.is_none());
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
        assert!(t.iter().any(|(s, _)| s.starts_with("CPUs")));
        // A moment in the heading's place.
        let mut c = column(3);
        c.cue = Some(("Final stage".into(), 1.0));
        c.won = Some("Stage won".into());
        let mut list = DrawList::new();
        assert!(draw_fields(&mut list, &c, &f, &[], 1.0, &mut FixedMeasure));
        let t = texts(&list);
        assert!(t.iter().any(|(s, _)| s == "Final stage"));
        assert!(t.iter().any(|(s, _)| s == "Stage won"));
        assert!(!t.iter().any(|(s, _)| s.starts_with("CPUs")));
    }

    #[test]
    fn far_more_opponents_than_room_show_the_first_seats_and_a_count() {
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0)] {
            let g = geo(w, h);
            let k = keep(&g, 1.0);
            let mut last = 0;
            for n in [16, 32, 64, 128] {
                let f = arrange(n, &g, 4.0, false, 1.0, &k).unwrap();
                assert_eq!(f.slots.len() + f.more, n, "{w}x{h} {n}");
                assert!(f.lane >= MINI_LANE_MIN);
                // The first seats, in order.
                assert!(f.slots.iter().enumerate().all(|(i, s)| s.seat == i));
                // Never over the field, the HUD or each other.
                let [mid, band] = ddi_render::scene::filter_band(&g, 4.0);
                for (i, s) in f.slots.iter().enumerate() {
                    let r = ltrb(s.rect);
                    assert!(r[0] >= 0.0 && r[2] <= w && r[1] >= 0.0 && r[3] <= h);
                    assert!(r[2] <= mid - band / 2.0 || r[0] >= mid + band / 2.0);
                    assert!(k.iter().all(|k| !overlaps(r, *k)));
                    assert!(f.slots[i + 1..].iter().all(|t| !overlaps(r, ltrb(t.rect))));
                }
                // The count's line on the screen, under the slots.
                if f.more > 0 {
                    assert!(f.more_at[1] < h);
                    assert!(f.slots.len() >= last, "{n}: as many as fit");
                }
                last = f.slots.len();
            }
        }
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
