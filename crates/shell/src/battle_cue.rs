//! A battle's attacks as the play shows them beside the field
//! (`docs/plans/multiplayer-prototype-progress.md`, M8b;
//! `docs/plans/multiplayer-ui.md`, "Incoming lightning"): no attack button,
//! no target to choose, nothing said by sound alone.
//!
//! - The incoming warning ([`Cue`]): an attack against the local player,
//!   committed. A shock glyph drawn here (whatever the skin) beside the
//!   field's outer receptor edge, at the receptors' row (the bottom under
//!   Reverse), with one dot a shock under it; it scales in over 0.2 s
//!   (at once under reduced motion), dims to 60% once the first shock is
//!   in sight, and goes once the last has passed: there is no recovery
//!   since `attacks-v1`, so no shield. Where more attacks are placed
//!   behind it (a chain: under `attacks-v2` any number), smaller glyphs
//!   stand in a column at its right, one an attack up to [`BEHIND_GLYPHS`],
//!   past that one glyph and the count ("×5"), never dimmed (they are still
//!   to come); once the first has passed, the warning is the next one's.
//! - The waiting line: a muted line under the life gauge's right end
//!   while the player's full bar waits ("Waiting for target").
//! - The policy label ([`draw_policy`], M8 round 3, D8): the player's
//!   attack policy, one small muted word under the life gauge's left end,
//!   as steady as the waiting line (nothing flashes, nothing moves). A
//!   tap or a click on it or on the bars above it ([`policy_area`])
//!   cycles the policy, as Shift, Tab and a pad's View do
//!   (`ddi_session::play`); after it a small quiet cap names the key (or
//!   the pad's control) that does it, where it fits (none on a touch
//!   screen, where the label is tapped).
//! - The chart line ([`draw_chart`], feedback round J): in a stage under
//!   "Charts: Adaptive", the chart played now as song select names it
//!   ("HARD 9"), one small muted text on the policy label's line, aligned
//!   to the life gauge's right end. It changes at a step's boundary, when
//!   the new chart's notes begin, bright for [`CHART_EMPHASIS`] seconds
//!   with a chevron pointing the step's way, then muted again (under
//!   reduced motion it simply changes). Where the waiting line is due too
//!   it goes one line lower ([`draw_waiting_row`]) and the chart keeps its
//!   place; where that line would reach the receptors' row (a short field,
//!   a phone held sideways: [`waiting_below_fits`]) the waiting line takes
//!   the chart's place while it is due, but for a step's bright moment.

use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_render::FieldGeometry;
use ddi_render::draw2d::{Affine, Blend, DrawList, Quad, Shape};
use ddi_session::battle::Incoming;

use crate::opponents::FACE;
/// The glyph: the power bar's electric violet, lighter.
const BOLT: [f32; 4] = [0.84, 0.76, 1.0, 1.0];
const GLOW: [f32; 4] = [0.55, 0.36, 1.0, 0.45];
const MUTED: [f32; 4] = [0.80, 0.80, 0.86, 0.72];
/// How long the warning takes to scale in, seconds.
const SCALE_IN: f64 = 0.2;
/// Its opacity once the first shock is in sight.
const DIMMED: f32 = 0.6;
/// The most attacks behind the nearest drawn one glyph each; past it, one
/// glyph and the count.
pub const BEHIND_GLYPHS: u16 = 3;

/// The warning as drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    /// One dot a shock.
    pub shocks: u8,
    /// Attacks placed behind this one (a chain).
    pub behind: u16,
    /// The first shock is in sight.
    pub dim: bool,
    /// `0..=1`, as it scales in.
    pub scale: f32,
}

/// The warning for `incoming` at judged song second `t`; `in_sight`, the
/// first of its shocks is drawn on the field now.
pub fn cue(incoming: Option<Incoming>, t: f64, in_sight: bool, reduced: bool) -> Option<Cue> {
    let i = incoming?;
    let scale = if reduced {
        1.0
    } else {
        ((t - i.committed) / SCALE_IN).clamp(0.0, 1.0) as f32
    };
    Some(Cue {
        shocks: i.shocks,
        behind: i.behind,
        dim: in_sight || t >= i.first,
        scale,
    })
}

/// Where the warning goes, `[left, top, right, bottom]` in target pixels:
/// beside the field's right edge (`span` lanes wide), on the receptors'
/// row. The miniatures keep clear of it.
pub fn place(geo: &FieldGeometry, span: f32) -> [f32; 4] {
    let a = geo.arrow;
    let left = geo.left + a * span + a * 0.15;
    let y = geo.receptor_y;
    [left, y - a * 0.5, left + a * 0.8, y + a * 0.55]
}

/// A bar from `p` to `q`, `thick` across, its ends round.
pub(crate) fn segment(p: [f32; 2], q: [f32; 2], thick: f32, color: [f32; 4]) -> Quad {
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let mut quad = Quad::plain([-len / 2.0, -thick / 2.0, len / 2.0, thick / 2.0], color);
    quad.transform = Affine::rotate(dy.atan2(dx))
        .then(Affine::translate((p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0));
    quad.shape = Shape {
        radius: [thick / 2.0; 2],
        ..Shape::NONE
    };
    quad
}

/// Draws `cue` beside the field `geo` (`span` lanes wide).
pub fn draw(list: &mut DrawList, cue: &Cue, geo: &FieldGeometry, span: f32) {
    if cue.scale <= 0.0 {
        return;
    }
    let [l, t, r, b] = place(geo, span);
    let (w, h) = (r - l, b - t);
    let alpha = if cue.dim { DIMMED } else { 1.0 };
    let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * alpha];
    // Scaled about the glyph's middle.
    let (cx, cy) = (l + w / 2.0, t + h * 0.32);
    let k = cue.scale;
    let at = |x: f32, y: f32| [cx + (l + x * w - cx) * k, cy + (t + y * h - cy) * k];
    // A soft glow behind the bolt.
    let g = w * 0.42 * k;
    let mut glow = Quad::plain([cx - g, cy - g, cx + g, cy + g], fade(GLOW));
    glow.shape = Shape {
        radius: [g; 2],
        soft: [g * 0.6; 2],
        ..Shape::NONE
    };
    glow.blend = Blend::Add;
    list.quad(glow);
    // The bolt: down-left, across, down-left again.
    let thick = w * 0.13 * k;
    for (p, q) in [
        (at(0.66, 0.0), at(0.34, 0.33)),
        (at(0.30, 0.33), at(0.70, 0.33)),
        (at(0.66, 0.33), at(0.34, 0.66)),
    ] {
        list.quad(segment(p, q, thick, fade(BOLT)));
    }
    // Those placed behind it: small bolts in a column at its right, one
    // an attack up to three, else one and the count; never dimmed (they
    // are still to come). Inside the warning's place, above the dots.
    let glyphs = if cue.behind > BEHIND_GLYPHS { 1 } else { cue.behind };
    let s = 0.34;
    let small_bolt = |list: &mut DrawList, oy: f32| {
        let ox = 0.70;
        let thick = w * 0.13 * k * s;
        for (p, q) in [
            ((0.66, 0.0), (0.34, 0.33)),
            ((0.30, 0.33), (0.70, 0.33)),
            ((0.66, 0.33), (0.34, 0.66)),
        ] {
            let small = |(x, y): (f32, f32)| at(ox + x * s, oy + y * s);
            list.quad(segment(small(p), small(q), thick, BOLT));
        }
    };
    for i in 0..glyphs {
        small_bolt(list, -0.04 + f32::from(i) * 0.24);
    }
    if cue.behind > BEHIND_GLYPHS {
        let mut style = TextStyle::new(ddi_l10n::font_stack(FACE), h * 0.24 * k);
        style.weight = 700;
        let [x, y] = at(0.76, 0.22);
        list.text(format!("×{}", cue.behind), &style, x, y, BOLT);
    }
    // One dot a shock, in rows of four under it.
    let n = usize::from(cue.shocks).min(16);
    let d = w * 0.13 * k;
    let gap = w * 0.22 * k;
    for i in 0..n {
        let (row, col) = (i / 4, i % 4);
        let x = cx + (col as f32 - 1.5) * gap;
        let y = t + h * 0.80 + row as f32 * gap * 0.9;
        let y = cy + (y - cy) * k;
        let mut dot = Quad::plain(
            [x - d / 2.0, y - d / 2.0, x + d / 2.0, y + d / 2.0],
            fade(BOLT),
        );
        dot.shape = Shape {
            radius: [d / 2.0; 2],
            ..Shape::NONE
        };
        list.quad(dot);
    }
}

/// Draws `text`, the waiting line, under the power bar's right end over
/// the field `geo` (`span` lanes wide), right-aligned to it, under the
/// life gauge; `ratio` a logical pixel.
pub fn draw_waiting(
    list: &mut DrawList,
    text: &str,
    geo: &FieldGeometry,
    span: f32,
    ratio: f32,
    measure: &mut dyn TextMeasure,
) {
    draw_waiting_row(list, text, geo, span, ratio, 0, measure);
}

/// [`draw_waiting`] on line `row` under the life gauge (0 the policy
/// label's; 1 the next, where the chart line takes the first's right
/// end).
pub fn draw_waiting_row(
    list: &mut DrawList,
    text: &str,
    geo: &FieldGeometry,
    span: f32,
    ratio: f32,
    row: u8,
    measure: &mut dyn TextMeasure,
) {
    let (power, life) = ddi_render::scene::power_and_life(geo, span);
    let mut style = TextStyle::new(ddi_l10n::font_stack(FACE), 12.0 * ratio);
    style.weight = 500;
    let width = measure.measure(text, &style).advance;
    let right = power[0] + power[2];
    let x = (right - width).max(4.0 * ratio);
    let y = line_y(&life, ratio, row);
    list.text(text, &style, x, y, MUTED);
}

/// Whether a second line under the life gauge of the field `geo` (`span`
/// lanes wide) clears the receptors' row (at the top: not under Reverse)
/// and the judgment; `ratio` a logical pixel.
pub fn waiting_below_fits(geo: &FieldGeometry, span: f32, ratio: f32) -> bool {
    let (_, life) = ddi_render::scene::power_and_life(geo, span);
    let bottom = line_y(&life, ratio, 1) + POLICY_SIZE * 1.25 * ratio;
    let judgment = ddi_render::scene::judgement_area(geo, span);
    let receptors = geo.direction < 0.0 || bottom <= geo.receptor_y - geo.arrow * 0.5;
    receptors && bottom <= judgment[1]
}

/// The top of line `row` under the life gauge `life` (0 the policy
/// label's).
fn line_y(life: &[f32; 4], ratio: f32, row: u8) -> f32 {
    life[1] + life[3] + 3.0 * ratio + f32::from(row) * POLICY_SIZE * LINE * ratio
}

/// A line's height under the gauge, in its text's size.
const LINE: f32 = 1.3;

/// Where a tap or a click cycles the attack policy, `[left, top, right,
/// bottom]` in target pixels: the label's place under the life gauge's
/// left end and the left of the two bars above it, over the field `geo`
/// (`span` lanes wide); `ratio` a logical pixel. Inside the field's own
/// width: clear of the quit button, the warning and the miniatures,
/// which all stand beside the field.
pub fn policy_area(geo: &FieldGeometry, span: f32, ratio: f32) -> [f32; 4] {
    let (power, life) = ddi_render::scene::power_and_life(geo, span);
    let pad = 4.0 * ratio;
    [
        (power[0] - pad).max(0.0),
        (power[1] - pad).max(0.0),
        power[0] + power[2] * 0.45,
        life[1] + life[3] + 3.0 * ratio + POLICY_SIZE * ratio * 1.3 + pad,
    ]
}

/// The policy label's size, logical pixels (the waiting line's).
const POLICY_SIZE: f32 = 12.0;

/// Draws `text`, the player's attack policy, under the life gauge's left
/// end over the field `geo` (`span` lanes wide), left-aligned to it;
/// `ratio` a logical pixel. With `key` (what changes it: a key's name,
/// "shift", or a pad control's), a small quiet cap after it, where it
/// fits before the bars' middle (the waiting line's half is the right).
/// Where what it drew ends, at the right (the chart line keeps clear of
/// it).
#[allow(clippy::too_many_arguments)]
pub fn draw_policy(
    list: &mut DrawList,
    text: &str,
    key: Option<&str>,
    geo: &FieldGeometry,
    span: f32,
    ratio: f32,
    measure: &mut dyn TextMeasure,
) -> f32 {
    let (power, life) = ddi_render::scene::power_and_life(geo, span);
    let mut style = TextStyle::new(ddi_l10n::font_stack(FACE), POLICY_SIZE * ratio);
    style.weight = 500;
    let y = life[1] + life[3] + 3.0 * ratio;
    let width = measure.measure(text, &style).advance;
    list.text(text, &style, life[0], y, MUTED);
    let Some(key) = key else {
        return life[0] + width;
    };
    let limit = power[0] + power[2] * 0.45;
    policy_cap(list, key, life[0] + width, y, ratio, limit, measure).unwrap_or(life[0] + width)
}

/// The cap's legend size, logical pixels.
const CAP_SIZE: f32 = 9.0;

/// The policy key's cap after the label ending at `x` (its top at `y`):
/// an outline with the key's name in it, quieter than the label; nothing
/// where it would reach past `limit`. Where it ends, if drawn.
fn policy_cap(
    list: &mut DrawList,
    key: &str,
    x: f32,
    y: f32,
    ratio: f32,
    limit: f32,
    measure: &mut dyn TextMeasure,
) -> Option<f32> {
    let mut style = TextStyle::new(ddi_l10n::font_stack(FACE), CAP_SIZE * ratio);
    style.weight = 500;
    let legend = measure.measure(key, &style).advance;
    let (pad, h) = (4.0 * ratio, (POLICY_SIZE + 2.0) * ratio);
    let left = x + 6.0 * ratio;
    let w = legend + 2.0 * pad;
    if left + w > limit {
        return None;
    }
    let top = y + 0.5 * ratio;
    let color = [MUTED[0], MUTED[1], MUTED[2], MUTED[3] * 0.7];
    let outline = 1.0 * ratio;
    let mut q = Quad::plain([left, top, left + w, top + h], color);
    q.shape = Shape {
        radius: [3.0 * ratio; 2],
        outline: [outline; 2],
        ..Shape::NONE
    };
    list.quad(q);
    let ty = top + (h - CAP_SIZE * ratio * 1.25) / 2.0;
    list.text(key, &style, left + pad, ty, color);
    Some(left + w)
}

/// Who a watcher follows, for the name plate on their field (the
/// feedback round K; `followed-plate-v0`): the competitor whose field is
/// the large one, in the play's field's place.
#[derive(Clone, Debug, PartialEq)]
pub struct Followed {
    pub name: std::rc::Rc<str>,
    /// A CPU's level, drawn as its stars (a person has none).
    pub level: Option<u8>,
    /// Their place among everyone in the stage now, worded ("2 of 4"),
    /// as the stage ranks.
    pub place: Option<String>,
    /// Out of the stage: the word their field is marked with (Out, or
    /// that they left), in the place's place.
    pub out: Option<String>,
}

/// What a name plate drew, target pixels: its plate (`[left, top, right,
/// bottom]`), the name as cut, and whether the stars and the words after
/// the plate (the place, or Out) had room.
#[derive(Clone, Debug, PartialEq)]
pub struct PlateDrawn {
    pub plate: [f32; 4],
    pub name: String,
    pub stars: bool,
    pub words: bool,
    /// Where all of it ends, at the right (the chart line keeps clear).
    pub end: f32,
}

/// The name's size on the plate, logical pixels, and the share of the
/// life gauge's width the plate and its words may take (the chart line
/// has the rest).
const PLATE_NAME: f32 = 13.0;
const PLATE_SHARE: f32 = 0.75;
const PLATE: [f32; 4] = [0.04, 0.03, 0.08, 0.80];
const PLATE_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Draws the name plate of the competitor a watcher follows on their
/// field `geo` (`span` lanes wide): under the life gauge's left end, on
/// the line the chart is named at the right of (where a player's own
/// field says their attack policy, which a followed field has none of):
/// the name bright on a dark plate with a CPU's stars after it, then,
/// quietly, their place of everyone, or Out once they are out of the
/// stage (the name dimmed then, as their field is). The name is cut
/// between graphemes only once the place and then the stars have gone;
/// the place also gives way to the chart line, which needs `reserve`
/// pixels at the gauge's right end to be whole ([`chart_room`]).
/// `ratio`: a logical pixel.
pub fn draw_followed(
    list: &mut DrawList,
    who: &Followed,
    reserve: f32,
    geo: &FieldGeometry,
    span: f32,
    ratio: f32,
    measure: &mut dyn TextMeasure,
) -> PlateDrawn {
    let (_, life) = ddi_render::scene::power_and_life(geo, span);
    let face: std::sync::Arc<str> = ddi_l10n::font_stack(FACE).into();
    let mut name_style = TextStyle::new(face.clone(), PLATE_NAME * ratio);
    name_style.weight = 700;
    let mut quiet = TextStyle::new(face, POLICY_SIZE * ratio);
    quiet.weight = 500;
    let (pad, gap) = (5.0 * ratio, 6.0 * ratio);
    let room = life[2] * PLATE_SHARE;
    let star_tall = 10.0 * ratio;
    let stars_w = who.level.map(|_| ddi_ui::stars::width(star_tall) + gap);
    let words = who.out.as_deref().or(who.place.as_deref());
    let words_w = words.map(|w| measure.measure(w, &quiet).advance + gap);
    let full = measure.measure(&who.name, &name_style).advance;
    // What goes first where it is narrow: the words, then the stars, then
    // the name's end.
    let (mut stars, mut said) = (stars_w, words_w);
    let fits = |stars: Option<f32>, said: Option<f32>| {
        let room = if said.is_some() { room.min(life[2] - reserve) } else { room };
        full + 2.0 * pad + stars.unwrap_or(0.0) + said.unwrap_or(0.0) <= room
    };
    if !fits(stars, said) {
        said = None;
    }
    if !fits(stars, said) {
        stars = None;
    }
    let (name, name_w) = fit(measure, &who.name, &name_style, (room - 2.0 * pad).max(0.0));
    let dim = if who.out.is_some() { 0.6 } else { 1.0 };
    let y = line_y(&life, ratio, 0);
    let tall = PLATE_NAME * 1.3 * ratio;
    let x = life[0];
    let right = x + pad + name_w + stars.unwrap_or(0.0) + pad;
    let mut plate = Quad::plain([x, y - ratio, right, y + tall + ratio], PLATE);
    plate.shape = Shape {
        radius: [3.0 * ratio; 2],
        ..Shape::NONE
    };
    let rect = plate.rect;
    list.quad(plate);
    let mut color = PLATE_TEXT;
    color[3] *= dim;
    list.text(name.clone(), &name_style, x + pad, y, color);
    if let (Some(level), Some(_)) = (who.level, stars) {
        crate::opponents::draw_stars(
            list,
            level,
            [x + pad + name_w + gap, y + tall / 2.0],
            star_tall,
            dim,
        );
    }
    let mut end = right;
    if let (Some(w), Some(width)) = (words, said) {
        // On the name's baseline's line: the quiet text is a pixel smaller.
        let dy = (PLATE_NAME - POLICY_SIZE) * ratio * 0.6;
        list.text(w, &quiet, right + gap, y + dy, MUTED);
        end = right + width;
    }
    PlateDrawn {
        plate: rect,
        name,
        stars: stars.is_some(),
        words: said.is_some(),
        end,
    }
}

/// How long the chart line stays bright after a step's boundary, seconds.
pub const CHART_EMPHASIS: f64 = 1.5;
/// The chart line bright, as a step changes it.
const BRIGHT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// How bright the chart line is `age` song seconds after its step's
/// boundary, `0..=1` (0: muted, at rest): fading out over
/// [`CHART_EMPHASIS`]; none under reduced motion (the text simply
/// changes).
pub fn chart_emphasis(age: f64, reduced: bool) -> f32 {
    if reduced || !(0.0..CHART_EMPHASIS).contains(&age) {
        return 0.0;
    }
    (1.0 - age / CHART_EMPHASIS).clamp(0.0, 1.0) as f32
}

/// The room the chart line `text` needs at the life gauge's right end to
/// be drawn whole ([`draw_chart`]: its words, its chevron's place and the
/// gap before them), target pixels.
pub fn chart_room(text: &str, ratio: f32, measure: &mut dyn TextMeasure) -> f32 {
    let mut style = TextStyle::new(ddi_l10n::font_stack(FACE), POLICY_SIZE * ratio);
    style.weight = 500;
    measure.measure(text, &style).advance + (10.0 + POLICY_SIZE * 0.7 + 4.0) * ratio
}

/// Draws `text`, the chart played now, on the policy label's line over
/// the field `geo` (`span` lanes wide), right-aligned to the life gauge's
/// right end; `ratio` a logical pixel. Its room starts past `clear` (where
/// the policy label and its cap end, or the gauge's left end): a name too
/// long for it is cut at its end between graphemes, an ellipsis after it,
/// its meter kept whole ("CHALL… 12"; the meter alone where nothing of the
/// name fits).
/// `step`: the step that brought it, up or down, and how bright it is
/// still ([`chart_emphasis`]): a chevron before the text pointing its way
/// and the text brighter, both fading to rest.
#[allow(clippy::too_many_arguments)]
pub fn draw_chart(
    list: &mut DrawList,
    text: &str,
    step: Option<(bool, f32)>,
    clear: f32,
    geo: &FieldGeometry,
    span: f32,
    ratio: f32,
    measure: &mut dyn TextMeasure,
) {
    let (_, life) = ddi_render::scene::power_and_life(geo, span);
    let mut style = TextStyle::new(ddi_l10n::font_stack(FACE), POLICY_SIZE * ratio);
    style.weight = 500;
    let right = life[0] + life[2];
    let gap = 10.0 * ratio;
    let chevron = POLICY_SIZE * 0.7 * ratio;
    let room = right - clear.max(life[0]) - gap - chevron - 4.0 * ratio;
    // The meter kept whole where the name is cut ("CHALL… 12").
    let (text, width) = match text.rsplit_once(' ') {
        Some((name, meter))
            if !meter.is_empty() && meter.bytes().all(|b| b.is_ascii_digit()) =>
        {
            let tail = format!(" {meter}");
            let tail_w = measure.measure(&tail, &style).advance;
            let (name, w) = fit(measure, name, &style, room - tail_w);
            if name.is_empty() {
                fit(measure, meter, &style, room)
            } else {
                (format!("{name}{tail}"), w + tail_w)
            }
        }
        _ => fit(measure, text, &style, room),
    };
    if text.is_empty() {
        return;
    }
    let y = line_y(&life, ratio, 0);
    let x = right - width;
    let k = step.map_or(0.0, |(_, k)| k.clamp(0.0, 1.0));
    let mix = |a: f32, b: f32| a + (b - a) * k;
    let color = [
        mix(MUTED[0], BRIGHT[0]),
        mix(MUTED[1], BRIGHT[1]),
        mix(MUTED[2], BRIGHT[2]),
        mix(MUTED[3], BRIGHT[3]),
    ];
    list.text(text, &style, x, y, color);
    // The chevron, before the text, pointing the step's way.
    if let Some((up, _)) = step.filter(|_| k > 0.0) {
        let cx = x - 4.0 * ratio - chevron / 2.0;
        let cy = y + POLICY_SIZE * 1.25 * ratio / 2.0;
        let (hw, hh) = (chevron / 2.0, chevron / 4.0);
        let (tip, ends) = if up { (cy - hh, cy + hh) } else { (cy + hh, cy - hh) };
        let thick = 1.6 * ratio;
        let c = [BRIGHT[0], BRIGHT[1], BRIGHT[2], k];
        list.quad(segment([cx - hw, ends], [cx, tip], thick, c));
        list.quad(segment([cx, tip], [cx + hw, ends], thick, c));
    }
}

/// `s` cut to `room` pixels, an ellipsis at its end, and its width: the
/// longest start that fits with "…", cut between graphemes, the spaces
/// before the ellipsis left out; nothing where not even "…" fits.
fn fit(measure: &mut dyn TextMeasure, s: &str, style: &TextStyle, room: f32) -> (String, f32) {
    let w = measure.measure(s, style).advance;
    if w <= room {
        return (s.to_string(), w);
    }
    let ends: Vec<usize> = unicode_segmentation::UnicodeSegmentation::grapheme_indices(s, true)
        .map(|(i, _)| i)
        .skip(1)
        .collect();
    let mut best = ("…".to_string(), measure.measure("…", style).advance);
    let (mut lo, mut hi) = (0usize, ends.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        let cut = format!("{}…", s[..ends[mid]].trim_end());
        let w = measure.measure(&cut, style).advance;
        if w <= room {
            best = (cut, w);
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if best.1 > room {
        return (String::new(), 0.0);
    }
    best
}

/// An attack policy's word, as the label and the rooms say it.
pub fn policy_word(t: ddi_session::battle::Targeting) -> String {
    use ddi_l10n::msg::room as m;
    use ddi_session::battle::Targeting;
    match t {
        Targeting::Spread => m::attack_policy_spread(),
        Targeting::Retaliate => m::attack_policy_retaliate(),
        Targeting::Leader => m::attack_policy_leader(),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_render::draw2d::Command;

    /// The policy label: under the life gauge's left end, inside the
    /// field's width at every size the miniatures' table uses, so the
    /// quit button, the warning and the miniatures never meet it; its
    /// tap area covers it and the left of the bars, and never reaches
    /// the waiting line's half.
    #[test]
    fn the_policy_label_is_under_the_gauge_inside_the_field() {
        for (w, h, ratio) in [
            (1280.0, 720.0, 1.0),
            (1920.0, 1080.0, 1.0),
            (844.0, 390.0, 1.0),
            (390.0, 844.0, 1.0),
            (2560.0, 1440.0, 2.0),
        ] {
            for reverse in [false, true] {
                let geo = FieldGeometry::new(w, h, 4.0, reverse);
                let (power, life) = ddi_render::scene::power_and_life(&geo, 4.0);
                let [l, t, r, b] = policy_area(&geo, 4.0, ratio);
                assert!(l >= 0.0 && t >= 0.0, "{w}x{h}");
                assert!(t <= power[1] && b > life[1] + life[3], "{w}x{h}");
                // Within the field's band, left of the bars' middle.
                assert!(l >= geo.left - 4.0 * ratio - 0.5, "{w}x{h}: {l} {}", geo.left);
                assert!(r < power[0] + power[2] / 2.0, "{w}x{h}");
                // Clear of the warning's place and the quit button's.
                let cue = place(&geo, 4.0);
                assert!(r < cue[0] || b < cue[1] || t > cue[3], "{w}x{h}");
                let layout = ddi_chart::Layout::dance_single();
                let [qx, qy, qr] = ddi_render::scene::quit_button(&geo, &layout, ratio, [0.0; 4]);
                let clear = qx + qr <= l || qx - qr >= r || qy + qr <= t || qy - qr >= b;
                assert!(clear, "{w}x{h} reverse {reverse}: quit {qx} {qy} {qr}, label {l} {t} {r} {b}");
                let mut list = DrawList::new();
                let mut m = ddi_actor::text::FixedMeasure;
                draw_policy(&mut list, "Spread", None, &geo, 4.0, ratio, &mut m);
                let Command::Text(run) = &list.commands[0] else {
                    panic!("a text");
                };
                assert_eq!(run.text.as_ref(), "Spread");
                assert_eq!(run.color, MUTED);
                assert_eq!(list.commands.len(), 1, "no key: the word alone");
                // With the key: its cap after the word, inside the tap
                // area and left of the bars' middle, where it fits.
                let mut list = DrawList::new();
                draw_policy(&mut list, "Spread", Some("shift"), &geo, 4.0, ratio, &mut m);
                if list.commands.len() > 1 {
                    let Command::Quad(q) = &list.commands[1] else {
                        panic!("a cap");
                    };
                    let word = m
                        .measure("Spread", &{
                            let mut st = TextStyle::new("x", POLICY_SIZE * ratio);
                            st.weight = 500;
                            st
                        })
                        .advance;
                    assert!(q.rect[0] > life[0] + word, "{w}x{h}: after the word");
                    assert!(q.rect[2] <= power[0] + power[2] * 0.45 + 0.01, "{w}x{h}");
                    assert!(q.rect[2] <= r + 0.01, "{w}x{h}: inside the tap area");
                    let Command::Text(t) = &list.commands[2] else {
                        panic!("its legend");
                    };
                    assert_eq!(t.text.as_ref(), "shift");
                } else {
                    assert!(w < 1000.0, "{w}x{h}: the cap fits on a wide screen");
                }
            }
        }
        assert_eq!(policy_word(ddi_session::battle::Targeting::Leader), "Leader");
    }

    /// A measure as the theme's face draws: half an em a Latin
    /// character, a whole one a Japanese one.
    struct Wide;
    impl TextMeasure for Wide {
        fn measure(&mut self, text: &str, style: &TextStyle) -> ddi_actor::text::RunMetrics {
            let size = style.size();
            let advance: f32 = text
                .chars()
                .map(|c| if c as u32 >= 0x3000 { size } else { size * 0.5 })
                .sum();
            ddi_actor::text::RunMetrics {
                advance,
                ink: [0.0, -size, advance, 0.0],
                baseline: style.line_height() * 0.8,
            }
        }
    }

    fn texts(list: &DrawList) -> Vec<(String, f32, f32, [f32; 4])> {
        list.commands
            .iter()
            .filter_map(|c| match c {
                Command::Text(t) => {
                    let [x, y] = t.transform.apply([0.0, 0.0]);
                    Some((t.text.to_string(), x, y, t.color))
                }
                _ => None,
            })
            .collect()
    }

    /// The followed competitor's name plate (the feedback round K): under
    /// the life gauge's left end, inside the field, clear of the
    /// receptors' row where the chart line is, at every size and both
    /// ways; the name whole while the stars and the place show, the place
    /// going first where it is narrow, then the stars, then the name's
    /// end, cut between graphemes; the chart line starts after it; Out in
    /// the place's place, the name dimmed.
    #[test]
    fn the_followed_plate_is_under_the_gauge_and_keeps_the_name() {
        let who = |name: &str, level: Option<u8>, out: Option<&str>| Followed {
            name: name.into(),
            level,
            place: Some("2 of 73".into()),
            out: out.map(String::from),
        };
        for (w, h, ratio) in [
            (1280.0, 720.0, 1.0),
            (1920.0, 1080.0, 1.0),
            (844.0, 390.0, 1.0),
            (390.0, 844.0, 1.0),
            (390.0 * 3.0, 844.0 * 3.0, 3.0),
            (2560.0, 1440.0, 2.0),
        ] {
            for reverse in [false, true] {
                let geo = FieldGeometry::new(w, h, 4.0, reverse);
                let (_, life) = ddi_render::scene::power_and_life(&geo, 4.0);
                for name in ["Ana", "Quill Sprocket", "しずかなアコーディオン弾き", "A very long name of a player indeed"] {
                    for level in [None, Some(7)] {
                        let mut list = DrawList::new();
                        let d = draw_followed(&mut list, &who(name, level, None), 0.0, &geo, 4.0, ratio, &mut Wide);
                        let at = format!("{w}x{h}@{ratio} reverse {reverse} {name:?} {level:?}: {d:?}");
                        let [l, t, r, b] = d.plate;
                        assert!(l >= life[0] - 0.01 && t >= life[1] + life[3], "{at}");
                        assert!(d.end <= life[0] + life[2] * PLATE_SHARE + 0.01, "{at}");
                        assert!(r <= d.end + 0.01, "{at}");
                        // As tall as the chart line's line, about.
                        assert!(b - t <= (PLATE_NAME * 1.3 + 2.0) * ratio + 0.01, "{at}");
                        // The name whole while anything else shows.
                        if d.stars || d.words {
                            assert_eq!(d.name, name, "{at}");
                        }
                        assert!(!d.stars || level.is_some(), "{at}");
                        // The place goes before the stars.
                        if level.is_some() && d.words {
                            assert!(d.stars, "{at}");
                        }
                        if d.name != name {
                            assert!(d.name.ends_with('…') && name.starts_with(d.name.trim_end_matches('…').trim_end()), "{at}");
                        }
                        let all = texts(&list);
                        assert_eq!(all[0].0, d.name, "{at}");
                        assert_eq!(all[0].3, PLATE_TEXT, "{at}");
                        assert_eq!(all.len(), 1 + usize::from(d.words), "{at}");
                        // The chart line after it, on its line.
                        let before = list.commands.len();
                        draw_chart(&mut list, "CHALLENGE 12", None, d.end, &geo, 4.0, ratio, &mut Wide);
                        if let Some((_, x, y, _)) = texts(&list).get(all.len()) {
                            assert!(*x >= d.end, "{at}: the chart at {x}");
                            assert!((*y - all[0].2).abs() < 0.01, "{at}");
                        }
                        assert!(list.commands.len() <= before + 1, "{at}");
                    }
                }
                // The place gives way to a chart line that needs its
                // room; the stars and the name do not.
                let mut list = DrawList::new();
                let need = chart_room("CHALLENGE 12", ratio, &mut Wide);
                let free = draw_followed(&mut list, &who("Ana Li", Some(3), None), 0.0, &geo, 4.0, ratio, &mut Wide);
                let tight = draw_followed(&mut list, &who("Ana Li", Some(3), None), need, &geo, 4.0, ratio, &mut Wide);
                assert!(tight.stars && tight.name == "Ana Li", "{w}x{h}: {tight:?}");
                if free.words && free.end > life[0] + life[2] - need {
                    assert!(!tight.words, "{w}x{h}: {tight:?}");
                }
                assert!(!tight.words || tight.end <= life[0] + life[2] - need + 0.01, "{w}x{h}: {tight:?}");
                // A short name on a desktop's field: all of it.
                if w >= 1280.0 {
                    let mut list = DrawList::new();
                    let d = draw_followed(&mut list, &who("Ana", Some(3), None), 0.0, &geo, 4.0, ratio, &mut Wide);
                    assert!(d.stars && d.words, "{w}x{h}: {d:?}");
                    assert_eq!(texts(&list)[1].0, "2 of 73");
                    // Out: said in the place's place, the name dimmed.
                    let mut list = DrawList::new();
                    let d = draw_followed(&mut list, &who("Ana", Some(3), Some("Out")), 0.0, &geo, 4.0, ratio, &mut Wide);
                    let all = texts(&list);
                    assert!(d.words && all[1].0 == "Out", "{w}x{h}: {all:?}");
                    assert!(all[0].3[3] < 1.0, "{w}x{h}: dimmed");
                }
            }
        }
    }

    /// The chart line: on the policy label's line, its end at the life
    /// gauge's right end, clear of the label and its cap, at every size
    /// the miniatures' table uses, a phone's upright among them, with the
    /// English and the Japanese policy words; the longest difficulty
    /// with a two-digit meter whole. The waiting line, due too, goes one
    /// line lower, still above the receptors' row and the judgment.
    #[test]
    fn the_chart_line_sits_at_the_gauge_s_right_end_beside_the_policy() {
        let chart = "CHALLENGE 12";
        for (w, h, ratio) in [
            (1280.0, 720.0, 1.0),
            (1920.0, 1080.0, 1.0),
            (844.0, 390.0, 1.0),
            (390.0, 844.0, 1.0),
            (390.0 * 3.0, 844.0 * 3.0, 3.0),
            (844.0 * 3.0, 390.0 * 3.0, 3.0),
            (2560.0, 1440.0, 2.0),
        ] {
            for reverse in [false, true] {
                let geo = FieldGeometry::new(w, h, 4.0, reverse);
                let (_, life) = ddi_render::scene::power_and_life(&geo, 4.0);
                for (word, key) in [
                    ("Retaliate", Some("Tab")),
                    ("Retaliate", None),
                    ("仕返し", Some("Tab")),
                    ("仕返し", None),
                ] {
                    let mut list = DrawList::new();
                    let clear = draw_policy(&mut list, word, key, &geo, 4.0, ratio, &mut Wide);
                    let before = list.commands.len();
                    draw_chart(&mut list, chart, None, clear, &geo, 4.0, ratio, &mut Wide);
                    let below = waiting_below_fits(&geo, 4.0, ratio);
                    draw_waiting_row(&mut list, "Waiting for target", &geo, 4.0, ratio, 1, &mut Wide);
                    let all = texts(&list);
                    let policy = &all[0];
                    let (text, x, y, color) = &all[all.len() - 2];
                    assert_eq!(list.commands.len(), before + 2, "{w}x{h}: the chart and the waiting line");
                    // Under Reverse and on a desktop's screen there is
                    // room for both; a phone's receptors at the top may be
                    // too near (the waiting line then takes the chart's
                    // place).
                    if reverse || w > h {
                        assert!(below || h / ratio < 500.0, "{w}x{h}@{ratio} reverse {reverse}");
                    }
                    // Whole on a desktop's screen and on a phone's (no
                    // key cap on a touch screen, where the label is
                    // tapped); a phone's sideways with a key's cap cuts the
                    // name and keeps the meter.
                    if w / ratio >= 1280.0 || key.is_none() {
                        assert_eq!(text, chart, "{w}x{h} {word}: whole");
                    } else {
                        assert!(text == chart || text.ends_with("… 12"), "{w}x{h} {word}: {text}");
                    }
                    assert_eq!(*color, MUTED, "at rest, muted");
                    assert_eq!(*y, policy.2, "{w}x{h}: on the policy's line");
                    let mut style = TextStyle::new("x", POLICY_SIZE * ratio);
                    style.weight = 500;
                    let width = Wide.measure(text, &style).advance;
                    assert!((x + width - (life[0] + life[2])).abs() < 0.01, "{w}x{h}: at the gauge's right end");
                    assert!(*x >= clear + 10.0 * ratio, "{w}x{h} {word}: clear of the policy ({x} {clear})");
                    // The waiting line under it, above the receptors' row
                    // (at the top) and the judgment.
                    let wait = &all[all.len() - 1];
                    let bottom = wait.2 + POLICY_SIZE * 1.25 * ratio;
                    assert!(wait.2 >= y + POLICY_SIZE * 1.25 * ratio, "{w}x{h}: under the chart");
                    if below {
                        if !reverse {
                            assert!(bottom <= geo.receptor_y - geo.arrow * 0.5, "{w}x{h}: above the receptors ({bottom})");
                        }
                        let judgment = ddi_render::scene::judgement_area(&geo, 4.0);
                        assert!(bottom <= judgment[1], "{w}x{h}: above the judgment");
                    }
                }
            }
        }
    }

    /// A step's emphasis: bright with a chevron its way at the boundary,
    /// fading to the muted rest over [`CHART_EMPHASIS`]; under reduced
    /// motion none (the text simply changes). A name too long for its
    /// room is cut at its end, an ellipsis after it.
    #[test]
    fn a_step_shows_bright_its_way_then_rests_and_long_names_are_cut() {
        assert_eq!(chart_emphasis(0.0, false), 1.0);
        assert!((chart_emphasis(CHART_EMPHASIS / 2.0, false) - 0.5).abs() < 1e-6);
        assert_eq!(chart_emphasis(CHART_EMPHASIS, false), 0.0);
        assert_eq!(chart_emphasis(-0.1, false), 0.0, "before its boundary");
        assert_eq!(chart_emphasis(0.0, true), 0.0, "reduced motion");
        let geo = FieldGeometry::new(1280.0, 720.0, 4.0, false);
        let chevron = |up: bool, k: f32| {
            let mut list = DrawList::new();
            draw_chart(&mut list, "HARD 9", Some((up, k)), 0.0, &geo, 4.0, 1.0, &mut Wide);
            list
        };
        let up = chevron(true, 1.0);
        assert_eq!(up.commands.len(), 3, "the text and the chevron's two strokes");
        assert_eq!(texts(&up)[0].3, BRIGHT);
        // The chevron's tip above its ends going up, below them going down.
        let tip_y = |l: &DrawList| {
            let Command::Quad(q) = &l.commands[1] else { panic!("a stroke") };
            let [_, y] = q.transform.apply([q.rect[2], 0.0]);
            let [_, y0] = q.transform.apply([q.rect[0], 0.0]);
            y - y0
        };
        assert!(tip_y(&up) < 0.0);
        assert!(tip_y(&chevron(false, 1.0)) > 0.0);
        let rest = chevron(true, 0.0);
        assert_eq!(rest.commands.len(), 1, "at rest: the text alone");
        assert_eq!(texts(&rest)[0].3, MUTED);
        // Too long: cut at its end between graphemes.
        let mut list = DrawList::new();
        let long = "A VERY LONG EDIT NAME THAT GOES ON AND ON AND ON AND ON AND ON AND ON AND ON AND ON 13";
        draw_chart(&mut list, long, None, 0.0, &geo, 4.0, 1.0, &mut Wide);
        let (text, x, ..) = &texts(&list)[0];
        let head = text.strip_suffix("… 13").expect("cut, its meter kept");
        assert!(long.starts_with(head), "{text}");
        let (_, life) = ddi_render::scene::power_and_life(&geo, 4.0);
        assert!(*x >= life[0] + 10.0 * 1.0, "{x}");
    }

    /// The cue's line as drawn, pinned: the policy and its cap, the chart
    /// at rest, just after a step up and a step down, and with the waiting
    /// line under it, at a desktop's size and a phone's upright.
    #[test]
    fn the_cue_s_line_is_pinned() {
        let fnv = |text: &str| {
            text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
            })
        };
        let mut got = Vec::new();
        for (name, w, h, ratio) in [("desktop", 1280.0, 720.0, 1.0), ("phone", 1170.0, 2532.0, 3.0)] {
            let geo = FieldGeometry::new(w, h, 4.0, false);
            let mut list = DrawList::new();
            let m = &mut ddi_actor::text::FixedMeasure;
            let clear = draw_policy(&mut list, "Spread", Some("Tab"), &geo, 4.0, ratio, m);
            draw_chart(&mut list, "HARD 9", None, clear, &geo, 4.0, ratio, m);
            draw_chart(&mut list, "CHALLENGE 12", Some((true, 0.8)), clear, &geo, 4.0, ratio, m);
            draw_chart(&mut list, "MEDIUM 6", Some((false, 0.3)), clear, &geo, 4.0, ratio, m);
            draw_waiting_row(&mut list, "Waiting for target", &geo, 4.0, ratio, 1, m);
            got.push((name, fnv(&ddi_ui::testing::rounded(&format!("{:?}", list.commands)))));
        }
        let want: [(&str, u64); 2] = [("desktop", 0xe2493277aa93490f), ("phone", 0x1a52adc5c42a7c57)];
        assert_eq!(got, want, "{got:#x?}");
    }

    fn incoming() -> Incoming {
        Incoming {
            shocks: 8,
            committed: 10.0,
            first: 14.0,
            clear: 18.0,
            behind: 0,
        }
    }

    #[test]
    fn it_scales_in_dims_in_sight_and_marks_those_placed_behind() {
        assert_eq!(cue(None, 10.0, false, false), None);
        let c = cue(Some(incoming()), 10.1, false, false).unwrap();
        assert!((c.scale - 0.5).abs() < 1e-3 && !c.dim && c.behind == 0);
        assert_eq!(cue(Some(incoming()), 10.0, false, true).unwrap().scale, 1.0);
        assert!(cue(Some(incoming()), 12.0, true, false).unwrap().dim);
        assert!(cue(Some(incoming()), 14.5, false, false).unwrap().dim);
        // Two more placed behind it (a chain): said by the cue.
        let chained = Incoming {
            behind: 2,
            ..incoming()
        };
        assert_eq!(cue(Some(chained), 12.0, false, false).unwrap().behind, 2);
    }

    #[test]
    fn it_sits_beside_the_receptors_and_follows_reverse() {
        let geo = FieldGeometry::new(1280.0, 720.0, 4.0, false);
        let [l, t, _, b] = place(&geo, 4.0);
        assert!(l > geo.left + geo.arrow * 4.0);
        assert!(t < geo.receptor_y && b > geo.receptor_y);
        let rev = FieldGeometry::new(1280.0, 720.0, 4.0, true);
        let [_, t2, _, _] = place(&rev, 4.0);
        assert!(t2 > 720.0 / 2.0, "at the bottom under Reverse");
        let mut list = DrawList::new();
        let c = cue(Some(incoming()), 11.0, false, false).unwrap();
        draw(&mut list, &c, &geo, 4.0);
        // A glow, three strokes and eight dots.
        assert_eq!(list.commands.len(), 1 + 3 + 8);
        // More placed behind it: a smaller bolt each, up to three, all
        // inside the warning's place (the miniatures keep clear of it).
        let [pl, pt, pr, pb] = place(&geo, 4.0);
        for n in 1..=3u16 {
            list.clear();
            draw(&mut list, &Cue { behind: n, scale: 1.0, ..c }, &geo, 4.0);
            assert_eq!(list.commands.len(), 1 + 3 + 3 * usize::from(n) + 8, "{n}");
            for cmd in &list.commands[4..4 + 3 * usize::from(n)] {
                let Command::Quad(q) = cmd else { panic!("a stroke") };
                let [x, y] = q.transform.apply([0.0, 0.0]);
                assert!(x > pl && x < pr && y > pt - 1.0 && y < pb, "{n}: {x} {y}");
            }
        }
        // Past three: one bolt and the count.
        list.clear();
        draw(&mut list, &Cue { behind: 5, scale: 1.0, ..c }, &geo, 4.0);
        assert_eq!(list.commands.len(), 1 + 3 + 3 + 1 + 8);
        assert!(list.commands.iter().any(
            |cmd| matches!(cmd, Command::Text(t) if t.text.as_ref() == "×5")
        ));
    }
}
