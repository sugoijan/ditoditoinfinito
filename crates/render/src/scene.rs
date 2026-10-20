//! Turns an engine [`Frame`] into sprite instances and HUD text.
//!
//! Geometry is derived from the target size every frame: the note field is
//! centred, one lane is one arrow wide, and one "arrow height" of engine
//! distance is one arrow of pixels (x-mod 1 = one arrow per beat).

use ddi_chart::{Color, Glyph, Layout};
use ddi_engine::frame::{Frame, NoteSprite, SpriteKind};
use ddi_engine::rules::{JudgeNames, Judgement};

use crate::noteskin::{LayerPart, NoteLayer};
use crate::sprite::{Instance, Shape, Symbol, SymbolMode};
use crate::text::{Align, TextItem};
use crate::textured::TexQuad;

pub use crate::note_colors::{ColorScheme, Gradient, quantization_color};

#[derive(Clone, Debug, PartialEq)]
pub struct Skin {
    pub receptor: [f32; 4],
    pub hold_active: [f32; 4],
    pub hold_inactive: [f32; 4],
    pub hold_dropped: [f32; 4],
    pub mine: [f32; 4],
    pub background: wgpu::Color,
}

impl Default for Skin {
    fn default() -> Skin {
        Skin {
            receptor: [0.75, 0.75, 0.8, 0.9],
            hold_active: [0.55, 0.95, 0.65, 0.95],
            hold_inactive: [0.35, 0.65, 0.45, 0.85],
            hold_dropped: [0.4, 0.4, 0.4, 0.6],
            mine: [0.35, 0.3, 0.45, 1.0],
            background: wgpu::Color {
                r: ddi_actor::background::GROUND[0] as f64,
                g: ddi_actor::background::GROUND[1] as f64,
                b: ddi_actor::background::GROUND[2] as f64,
                a: 1.0,
            },
        }
    }
}

pub fn judgement_color(j: Judgement) -> [f32; 4] {
    match j {
        Judgement::W1 => [0.55, 0.95, 1.0, 1.0],
        Judgement::W2 => [1.0, 0.9, 0.35, 1.0],
        Judgement::W3 => [0.45, 0.95, 0.45, 1.0],
        Judgement::W4 => [0.75, 0.5, 1.0, 1.0],
        Judgement::W5 => [1.0, 0.55, 0.3, 1.0],
        Judgement::Miss => [1.0, 0.3, 0.3, 1.0],
    }
}

/// Pixel geometry of the note field for a given target size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldGeometry {
    pub arrow: f32,
    pub left: f32,
    pub receptor_y: f32,
    /// Sign applied to engine distance (arrow heights above the receptor,
    /// positive = not yet reached) to get a pixel offset in y-down space:
    /// +1 in normal scroll (receptors at the top, notes below them travel
    /// up), -1 in reverse (receptors at the bottom, notes above travel down).
    pub direction: f32,
    pub width: f32,
    pub height: f32,
}

impl FieldGeometry {
    /// Geometry for a field `span` lane widths wide (see [`field_span`]).
    pub fn new(width: f32, height: f32, span: f32, reverse: bool) -> FieldGeometry {
        let by_height = height / 10.0;
        let by_width = width / (span + 2.0);
        let arrow = by_height.min(by_width).clamp(24.0, 160.0);
        let field_w = arrow * span;
        let left = (width - field_w) / 2.0;
        let (receptor_y, direction) = if reverse {
            (height - arrow * 1.5, -1.0)
        } else {
            (arrow * 1.5, 1.0)
        };
        FieldGeometry {
            arrow,
            left,
            receptor_y,
            direction,
            width,
            height,
        }
    }

    pub fn lane_x(&self, column: f32) -> f32 {
        self.left + (column + 0.5) * self.arrow
    }

    /// Screen y for an engine distance in arrow heights.
    pub fn y(&self, arrows: f32) -> f32 {
        self.receptor_y + arrows * self.arrow * self.direction
    }

    /// [`FieldGeometry::direction`] of a lane with this scroll sign (a
    /// Dancing☆Onigiri lower-row lane scrolls the other way).
    pub fn lane_direction(&self, scroll_sign: i8) -> f32 {
        if scroll_sign < 0 {
            -self.direction
        } else {
            self.direction
        }
    }

    /// Receptor y of a lane with this scroll sign: at the top when its
    /// notes travel up, at the bottom when they travel down.
    pub fn lane_receptor_y(&self, scroll_sign: i8) -> f32 {
        // A lane scrolling the other way has its receptors at the other end.
        if self.lane_direction(scroll_sign) == self.direction {
            self.receptor_y
        } else {
            self.height - self.receptor_y
        }
    }

    /// Screen y of a lane's note at an engine distance in arrow heights.
    pub fn lane_y(&self, scroll_sign: i8, arrows: f32) -> f32 {
        self.lane_receptor_y(scroll_sign) + arrows * self.arrow * self.lane_direction(scroll_sign)
    }
}

#[derive(Default)]
pub struct SceneOutput {
    pub instances: Vec<Instance>,
    pub text: Vec<TextItem>,
    /// A noteskin's quads.
    pub quads: Vec<TexQuad>,
    /// The order the two kinds are drawn in.
    pub order: Vec<Segment>,
}

/// A run of the scene's drawing.
#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Sprites(std::ops::Range<u32>),
    Quads(std::ops::Range<u32>),
}

/// Cuts the scene into runs as drawing switches between the procedural
/// sprites and a noteskin's quads.
#[derive(Default)]
struct Order {
    segments: Vec<Segment>,
    sprites: u32,
    quads: u32,
}

impl Order {
    fn sprites(&mut self, end: usize) {
        let end = end as u32;
        if end > self.sprites {
            self.segments.push(Segment::Sprites(self.sprites..end));
            self.sprites = end;
        }
    }

    fn quads(&mut self, end: usize) {
        let end = end as u32;
        if end > self.quads {
            self.segments.push(Segment::Quads(self.quads..end));
            self.quads = end;
        }
    }
}

/// Which part of the built-in skin a note is.
fn layer_part(kind: &SpriteKind) -> LayerPart {
    match kind {
        SpriteKind::Tap | SpriteKind::Dummy => LayerPart::Tap,
        SpriteKind::Lift => LayerPart::Lift,
        SpriteKind::Fake => LayerPart::Fake,
        SpriteKind::Mine | SpriteKind::Shock => LayerPart::Mine,
        SpriteKind::HoldHead { .. } => LayerPart::Hold,
        SpriteKind::RollHead { .. } => LayerPart::Roll,
    }
}

/// Which part of the built-in skin a note's head is (a hold's head is
/// drawn apart from its body).
fn head_part(kind: &SpriteKind) -> LayerPart {
    match kind {
        SpriteKind::HoldHead { .. } => LayerPart::HoldHead,
        SpriteKind::RollHead { .. } => LayerPart::RollHead,
        other => layer_part(other),
    }
}

/// Per-frame presentation switches that are not part of the skin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderOptions {
    /// Receptors at the bottom, notes travel down.
    pub reverse: bool,
    /// Draw receptors and HUD but no notes (audio-only calibration; the
    /// app also holds the frame's beat at 0 then).
    pub hide_notes: bool,
    /// Show the signed timing error of the last hit under the judgement.
    pub show_deltas: bool,
    /// Progress of a cancel gesture in 0..=1 (0 = none); draws the "hold to
    /// quit" hint.
    pub cancel_progress: f32,
    /// The quit hint shown with it; empty for the keyboard's.
    pub cancel_hint: &'static str,
    /// Perceived brightness of the background images, `0..=1` (0 = off).
    pub background: f32,
    /// Which background images to show this frame.
    pub backdrop: crate::background::Backdrop,
    /// The shape (width over height) backgrounds are fitted into, centred,
    /// above the persistent blurred base; `None`: the whole screen.
    pub frame_aspect: Option<f32>,
    /// Visual entrance only; None is settled, fully sharp play.
    pub background_reveal: Option<ddi_actor::background::Reveal>,
    /// How notes are colored (an explicit chart color still wins).
    pub note_colors: ColorScheme,
    /// Draw notes in the colors a chart gives them (Dancing☆Onigiri
    /// works); otherwise every note follows `note_colors`.
    pub chart_colors: bool,
    /// Darkening behind the lanes, `0..=1` of perceived brightness removed
    /// (0 = none).
    pub field_filter: f32,
    /// Draw only the field (receptors, holds, notes, explosions: a
    /// noteskin's, or the built-in arrows where it has none), as placed by
    /// this geometry, over the clear color (a skin's preview).
    pub preview: Option<Preview>,
    /// The ghost: how far the score is ahead of (or behind) the chart's
    /// best play at the same point, drawn under the score.
    pub ghost: Option<ScoreDelta>,
}

/// A score's lead over another ([`RenderOptions::ghost`]), as the
/// ruleset scores: percentage points (0.42 is "+0.42%"), or points
/// (money, EX).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScoreDelta {
    Percent(f32),
    Points(i64),
}

impl ScoreDelta {
    /// "+0.42%", "-1230", "+0.00%" (ASCII signs: the HUD's font is a
    /// subset).
    pub fn text(self) -> String {
        let (v, s) = match self {
            ScoreDelta::Percent(p) => (p as f64, format!("{:.2}%", p.abs())),
            ScoreDelta::Points(n) => (n as f64, n.unsigned_abs().to_string()),
        };
        let zero = s
            .trim_end_matches('%')
            .chars()
            .all(|c| c == '0' || c == '.');
        if v < 0.0 && !zero {
            format!("-{s}")
        } else {
            format!("+{s}")
        }
    }

    fn behind(self) -> bool {
        match self {
            ScoreDelta::Percent(p) => p < -0.005,
            ScoreDelta::Points(n) => n < 0,
        }
    }
}

/// [`RenderOptions::preview`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preview {
    pub geometry: FieldGeometry,
    /// The receptors too, not only the notes.
    pub receptors: bool,
}

/// A note's color under Hidden/Sudden: hidden notes show only their white
/// glow, visible ones are tinted towards white by it (StepMania draws the
/// glow as a white overlay of that opacity).
fn glowing(color: [f32; 4], note: &NoteSprite) -> [f32; 4] {
    let g = note.glow.clamp(0.0, 1.0);
    if note.alpha <= 0.0 {
        return [1.0, 1.0, 1.0, color[3] * g];
    }
    let mut c = color;
    for k in c.iter_mut().take(3) {
        *k += (1.0 - *k) * g;
    }
    c[3] *= note.alpha;
    c
}

/// O.K. and N.G.: as StepMania 5.1's `_fallback` theme animates its
/// `HoldJudgment` (`HoldJudgmentHeldCommand`, `HoldJudgmentLetGoCommand`,
/// in its 480-line screen where an arrow is 64): kept, it pops from 1.25
/// to its size in 0.3 s; dropped, it sinks 20 units over 0.8 s; both stay
/// 0.5 s, then go in 0.1 s (StepMania squashes it to half its height and
/// twice its width meanwhile; text here fades). The time it shows is the
/// engine's ([`ddi_engine::frame::HELD_SECONDS`]).
pub struct HoldLook {
    pub size: f32,
    /// Down the screen, in arrow sizes.
    pub dy: f32,
    pub alpha: f32,
}

impl HoldLook {
    pub fn at(held: bool, age: f32) -> HoldLook {
        let (settle, end) = if held { (0.3, 0.9) } else { (0.8, 1.4) };
        let alpha = (1.0 - (age - (end - 0.1)).max(0.0) / 0.1).clamp(0.0, 1.0);
        let t = (age / settle).clamp(0.0, 1.0);
        if held {
            HoldLook {
                size: 1.25 - 0.25 * t,
                dy: 0.0,
                alpha,
            }
        } else {
            HoldLook {
                size: 1.0,
                dy: (-10.0 + 20.0 * t) / 64.0,
                alpha,
            }
        }
    }
}

/// O.K.'s color (a pale gold, `#ffd666`) and N.G.'s (a pale red,
/// `#ff6e6e`).
pub const HELD_COLOR: [f32; 4] = [1.0, 0.84, 0.4, 1.0];
pub const LET_GO_COLOR: [f32; 4] = [1.0, 0.43, 0.43, 1.0];

/// The holds' and shocks' outcomes, under the ruleset's names: over each
/// lane, 54 units (of an arrow's 64) past the receptor towards the
/// field's middle (`HoldJudgmentYStandard` −90 against the receptors'
/// −144, and the same mirrored under Reverse), centred there and no wider
/// than the lane; or Dancing☆Onigiri's freeze judgement, still, two
/// arrows right of the tap judgement and 1.4 below it (`mainWindow.js`
/// places it 100 px right and 70 px below in its 50 px arrows), as large.
fn hold_judgements(
    frame: &Frame,
    layout: &Layout,
    geo: &FieldGeometry,
    names: &JudgeNames,
    judgement: (f32, f32, f32),
    text: &mut Vec<TextItem>,
) {
    use ddi_engine::frame::HoldPlace;
    let a = geo.arrow;
    for f in &frame.holds.flashes {
        let label = if f.held { &names.held } else { &names.let_go };
        if label.is_empty() {
            continue;
        }
        let mut color = if f.held { HELD_COLOR } else { LET_GO_COLOR };
        let (x, y, size) = match frame.holds.place {
            HoldPlace::None => continue,
            HoldPlace::Lanes => {
                let Some(lane) = layout.lanes.get(usize::from(f.lane)) else {
                    continue;
                };
                let look = HoldLook::at(f.held, f.age);
                if look.alpha <= 0.0 {
                    continue;
                }
                color[3] *= look.alpha;
                let ry = geo.lane_receptor_y(lane.scroll_sign);
                let inward = if ry < geo.height / 2.0 { 1.0 } else { -1.0 };
                let y = ry + inward * a * 54.0 / 64.0 + look.dy * a;
                // About the width of a lane (StepMania's graphic is 64 by
                // 32), never wider, so neighbours never run together.
                let rest = (a * 0.4).max(11.0).min(fit_width(label, a * 0.9));
                (geo.lane_x(lane.column), y, popped(rest, rest * look.size))
            }
            HoldPlace::Centre => {
                let (jx, jy, jsize) = judgement;
                (jx + a * 2.0, jy + a * 1.4, jsize)
            }
        };
        // `y` is the line's middle; a run is placed by its top, its line
        // 1.2 times its size.
        text.push(TextItem {
            text: label.clone(),
            x,
            y: y - size * 0.6,
            size,
            color,
            align: Align::Center,
            bold: true,
        });
    }
}

/// The largest size `label` fits `width` at: the HUD's face is monospaced,
/// half an em a character, wide ones (CJK) a whole em.
fn fit_width(label: &str, width: f32) -> f32 {
    let ems: f32 = label
        .chars()
        .map(|c| if (c as u32) < 0x1100 { 0.5 } else { 1.0 })
        .sum();
    if ems > 0.0 {
        width / ems
    } else {
        f32::INFINITY
    }
}

/// The touch screens' quit button (hold or double-tap it to quit, as
/// Escape): its middle and radius in the target's pixels, `ratio` of them
/// to a CSS pixel; 44 CSS pixels across, kept 8 in from the edges and the
/// safe area (`inset`: top, right, bottom, left, CSS pixels). In the left
/// margin beside the field when the field leaves room for it there, else
/// at the left edge; at the end away from the receptors (the bottom in
/// normal scroll, the top under Reverse; halfway down when the layout
/// has receptors at both ends), so a finger at the receptors never meets
/// it.
pub fn quit_button(geo: &FieldGeometry, layout: &Layout, ratio: f32, inset: [f32; 4]) -> [f32; 3] {
    let (r, m) = (22.0 * ratio, 8.0 * ratio);
    let [top, _, bottom, left] = inset.map(|v| v.max(0.0) * ratio);
    let mid = geo.height / 2.0;
    let rows = |above: bool| {
        layout
            .lanes
            .iter()
            .any(|l| (geo.lane_receptor_y(l.scroll_sign) < mid) == above)
    };
    let (at_top, at_bottom) = (rows(true), rows(false));
    let y = match (at_top, at_bottom) {
        (true, false) => geo.height - bottom.max(m) - r,
        (false, true) | (false, false) => top.max(m) + r,
        (true, true) => mid,
    };
    // Beside the field when it leaves room, else at the edge.
    let x = left.max(m) + r;
    let x = if geo.left - m >= x + r {
        (geo.left / 2.0).max(x)
    } else {
        x
    };
    [x, y, r]
}

/// Where the HUD draws over the top of the screen, `[left, top, right,
/// bottom]` in the target's px: the life gauge and the score (as wide as
/// its longest, "1000000"): what the lyrics keep clear of.
pub fn hud_areas(geo: &FieldGeometry) -> Vec<[f32; 4]> {
    let a = geo.arrow;
    let bar_w = (geo.width * 0.3).min(a * 6.0);
    let bar_h = (a * 0.22).max(8.0);
    let bar_y = bar_h * 1.5;
    let cx = geo.width / 2.0;
    let size = (a * 0.5).max(14.0);
    let right = geo.width - a * 0.5;
    vec![
        [
            cx - bar_w / 2.0,
            bar_y - bar_h / 2.0,
            cx + bar_w / 2.0,
            bar_y + bar_h / 2.0,
        ],
        [
            right - 7.0 * 0.6 * size,
            a * 0.35,
            right,
            a * 0.35 + size * 1.25,
        ],
    ]
}

/// The receptor rows and the lanes' approach near them, `[left, top,
/// right, bottom]` in the target's px, across the field: each row's
/// receptors (half an arrow either side of their middle, a little more)
/// and an arrow's height of the approach on the side the notes come from:
/// what lyrics never cover.
pub fn receptor_areas(geo: &FieldGeometry, layout: &Layout) -> Vec<[f32; 4]> {
    let a = geo.arrow;
    let span = field_span(layout);
    let (x0, x1) = (geo.left - a * 0.2, geo.left + a * span + a * 0.2);
    let mut rows: Vec<f32> = Vec::new();
    for l in &layout.lanes {
        let y = geo.lane_receptor_y(l.scroll_sign);
        if !rows.iter().any(|r| (r - y).abs() < 0.5) {
            rows.push(y);
        }
    }
    rows.into_iter()
        .map(|y| {
            if y < geo.height / 2.0 {
                [x0, y - a * 0.6, x1, y + a]
            } else {
                [x0, y - a, x1, y + a * 0.6]
            }
        })
        .collect()
}

/// Draws the quit button ([`quit_button`]) over everything else: a dark
/// disc with a light rim and a ✕.
pub fn add_quit_button(scene: &mut SceneOutput, [x, y, r]: [f32; 3]) {
    let start = scene.instances.len() as u32;
    scene.instances.push(Instance::new(
        [x, y],
        [r * 2.0, r * 2.0],
        Shape::Circle,
        [1.0, 1.0, 1.0, 0.3],
    ));
    let rim = (r / 22.0).max(1.0);
    scene.instances.push(Instance::new(
        [x, y],
        [(r - rim) * 2.0, (r - rim) * 2.0],
        Shape::Circle,
        [0.0, 0.0, 0.0, 0.6],
    ));
    for angle in [std::f32::consts::FRAC_PI_4, -std::f32::consts::FRAC_PI_4] {
        scene.instances.push(
            Instance::new(
                [x, y],
                [r * 0.85, r * 0.13],
                Shape::Rect,
                [1.0, 1.0, 1.0, 0.95],
            )
            .params([0.5, 0.0, 0.0, 0.0])
            .rotated(angle),
        );
    }
    let end = scene.instances.len() as u32;
    match scene.order.last_mut() {
        Some(Segment::Sprites(range)) if range.end == start => range.end = end,
        _ => scene.order.push(Segment::Sprites(start..end)),
    }
}

/// The field filter's band for a field `span` lanes wide: its middle and
/// its width, pixels across the target (a little wider than the lanes).
/// The start prompt draws the same band (`ddi_ui::screens::prompt`).
pub fn filter_band(geo: &FieldGeometry, span: f32) -> [f32; 2] {
    let a = geo.arrow;
    [geo.left + a * span / 2.0, a * (span + 0.3)]
}

/// The size a popping run is drawn at: `size` with its overshoot past
/// `rest` in half pixels (a few sizes in all, each shaped and rasterised
/// once and cached, where every frame of a pop asked for a new one; the
/// resting size exactly). The text pipeline samples its glyphs pixel for
/// pixel, so a pop drawn by scaling one raster would show its pixels.
pub fn popped(rest: f32, size: f32) -> f32 {
    rest + ((size - rest) * 2.0).round() / 2.0
}

/// Alpha of a black overlay that removes `amount` of perceived brightness.
/// On an sRGB target blending happens in linear light, so the remaining
/// brightness is converted; on a plain target values are already
/// perceptual.
pub fn filter_alpha(amount: f32, srgb_target: bool) -> f32 {
    let keep = 1.0 - amount.clamp(0.0, 1.0);
    if srgb_target {
        1.0 - keep.powf(2.2)
    } else {
        1.0 - keep
    }
}

fn rgba(c: Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

fn glyph_rotation(glyph: &Glyph) -> f32 {
    match glyph {
        // Shader arrow points left at rotation 0. Layout rotation uses the
        // CSS convention (positive = clockwise on screen), and the vertex
        // shader's rotation matrix in y-down pixel space is also clockwise
        // for positive angles, so the value passes straight through.
        Glyph::Arrow { rotation_deg } => rotation_deg.to_radians(),
        _ => 0.0,
    }
}

/// Width of a layout's field in lane widths: from the leftmost lane's left
/// edge to the rightmost lane's right edge (rows may overlap).
pub fn field_span(layout: &Layout) -> f32 {
    if layout.lanes.is_empty() {
        return 0.0;
    }
    layout.lanes.iter().map(|l| l.column).fold(0.0f32, f32::max) + 1.0
}

/// The shape id of a lane's glyph drawn in `mode`: arrows, or the symbols
/// standing for the other lanes (decision 27): a square for a pump pad's
/// centre, a star for the onigiri, a square for giko, a circle for iyo and
/// anything else. A triangle would read as an up arrow.
fn glyph_shape(glyph: &Glyph, mode: SymbolMode) -> u32 {
    let symbol = match glyph {
        Glyph::Arrow { .. } => {
            return match mode {
                SymbolMode::Fill => Shape::Arrow,
                SymbolMode::Outline => Shape::ArrowOutline,
                SymbolMode::Flow => Shape::ArrowGradient,
                SymbolMode::Ramp => Shape::ArrowRamp,
            } as u32;
        }
        Glyph::Onigiri => Symbol::Star,
        // Pump's centre panel is square too; giko is no longer played.
        Glyph::Center | Glyph::Giko => Symbol::Square,
        Glyph::Iyo | Glyph::Custom(_) => Symbol::Dot,
    };
    symbol.shape(mode)
}

/// An instance drawing a lane's glyph.
fn glyph_instance(
    center: [f32; 2],
    size: f32,
    glyph: &Glyph,
    mode: SymbolMode,
    color: [f32; 4],
) -> Instance {
    Instance {
        shape: glyph_shape(glyph, mode),
        ..Instance::new(center, [size, size], Shape::Rect, color)
    }
    .rotated(glyph_rotation(glyph))
}

/// Build the sprite and text lists for one frame. `srgb_target`: the
/// target's format is sRGB (blending in linear light).
#[allow(clippy::too_many_arguments)]
pub fn build(
    frame: &Frame,
    layout: &Layout,
    names: &JudgeNames,
    skin: &Skin,
    geo: &FieldGeometry,
    opts: RenderOptions,
    srgb_target: bool,
    layer: Option<&mut dyn NoteLayer>,
    stage: (&[TexQuad], &[TexQuad]),
) -> SceneOutput {
    let mut out = SceneOutput::default();
    build_into(
        &mut out,
        frame,
        layout,
        names,
        skin,
        geo,
        opts,
        srgb_target,
        layer,
        stage,
    );
    out
}

/// [`build`] into `out`, its lists cleared first and their room reused.
#[allow(clippy::too_many_arguments)]
pub fn build_into(
    out: &mut SceneOutput,
    frame: &Frame,
    layout: &Layout,
    names: &JudgeNames,
    skin: &Skin,
    geo: &FieldGeometry,
    opts: RenderOptions,
    srgb_target: bool,
    mut layer: Option<&mut dyn NoteLayer>,
    stage: (&[TexQuad], &[TexQuad]),
) {
    // A preview without receptors.
    let no_receptors = opts.preview.is_some_and(|p| !p.receptors);
    fn reuse<T>(v: &mut Vec<T>) -> Vec<T> {
        let mut v = std::mem::take(v);
        v.clear();
        v
    }
    let mut instances = reuse(&mut out.instances);
    instances.reserve(frame.notes.len() * 2 + 32);
    let mut text = reuse(&mut out.text);
    text.reserve(8);
    let mut quads = reuse(&mut out.quads);
    let mut order = Order {
        segments: reuse(&mut out.order),
        ..Order::default()
    };
    // A work's back layers, over the background and under the field
    // (`docs/plans/overlays.md`).
    quads.extend_from_slice(stage.0);
    order.quads(quads.len());
    let drawn = |layer: &Option<&mut dyn NoteLayer>, lane: usize, part: LayerPart| {
        layer.as_ref().is_some_and(|l| l.draws(lane, part))
    };
    let a = geo.arrow;

    let span = field_span(layout);
    // Field filter: a dark band behind the lanes, over the background.
    if opts.field_filter > 0.0 && !layout.lanes.is_empty() {
        let [mid, w] = filter_band(geo, span);
        instances.push(
            Instance::new(
                [mid, geo.height / 2.0],
                [w, geo.height + 2.0],
                Shape::Rect,
                [0.0, 0.0, 0.0, filter_alpha(opts.field_filter, srgb_target)],
            )
            .params([0.0, 0.0, 0.0, 0.0]),
        );
    }

    // Receptors: the noteskin's, then the built-in ones.
    order.sprites(instances.len());
    if let Some(l) = layer.as_mut() {
        // The call starts the layer's frame, drawn or not.
        l.receptors(frame, layout, geo, &mut quads);
        if no_receptors {
            quads.clear();
        }
    }
    order.quads(quads.len());
    for (i, lane) in layout.lanes.iter().enumerate() {
        if drawn(&layer, i, LayerPart::Receptor) || no_receptors {
            continue;
        }
        let state = frame.receptors.get(i);
        let pressed = state.is_some_and(|s| s.pressed);
        let scale = if pressed { 0.88 } else { 1.0 };
        let mut color = skin.receptor;
        if let Some((j, age)) = state.and_then(|s| s.flash) {
            let t = (1.0 - age / 0.25).clamp(0.0, 1.0);
            let jc = judgement_color(j);
            for k in 0..3 {
                color[k] = color[k] + (jc[k] - color[k]) * t;
            }
        }
        instances.push(glyph_instance(
            [
                geo.lane_x(lane.column),
                geo.lane_receptor_y(lane.scroll_sign),
            ],
            a * 0.92 * scale,
            &lane.glyph,
            SymbolMode::Outline,
            color,
        ));
    }

    // Hold bodies first so heads draw over them.
    let notes: &[NoteSprite] = if opts.hide_notes { &[] } else { &frame.notes };
    for note in notes {
        if let SpriteKind::HoldHead {
            tail_y,
            active,
            dropped,
        }
        | SpriteKind::RollHead {
            tail_y,
            active,
            dropped,
        } = note.kind
        {
            let Some(lane) = layout.lanes.get(note.lane as usize) else {
                continue;
            };
            if drawn(&layer, usize::from(note.lane), layer_part(&note.kind)) {
                continue;
            }
            let color = if dropped {
                skin.hold_dropped
            } else if active {
                skin.hold_active
            } else {
                skin.hold_inactive
            };
            let is_roll = matches!(note.kind, SpriteKind::RollHead { .. });
            let w = if is_roll { a * 0.38 } else { a * 0.5 };
            // Only the parts Hidden/Sudden leave visible.
            for (lo, hi) in frame.appearance.visible_spans(note.y, tail_y) {
                let y0 = geo.lane_y(lane.scroll_sign, lo);
                let y1 = geo.lane_y(lane.scroll_sign, hi);
                let center_y = (y0 + y1) / 2.0;
                let h = (y1 - y0).abs().max(1.0);
                instances.push(
                    Instance::new(
                        [geo.lane_x(lane.column), center_y],
                        [w, h],
                        Shape::HoldBody,
                        color,
                    )
                    .params([0.6, 0.0, 0.0, 0.0]),
                );
            }
        }
    }

    // The noteskin's holds over the built-in bodies.
    order.sprites(instances.len());
    if !opts.hide_notes
        && let Some(l) = layer.as_mut()
    {
        l.holds(frame, layout, geo, &mut quads);
    }
    order.quads(quads.len());

    // Notes.
    for note in notes {
        if drawn(&layer, usize::from(note.lane), head_part(&note.kind)) {
            continue;
        }
        push_note(&mut instances, note, layout, skin, geo, &opts, frame.beat);
    }
    order.sprites(instances.len());
    if let Some(l) = layer.as_mut() {
        if !opts.hide_notes {
            l.notes(frame, layout, geo, &mut quads);
        }
        l.explosions(frame, layout, geo, &mut quads);
    }
    // Its mask layers, over the field and under the HUD.
    quads.extend_from_slice(stage.1);
    order.quads(quads.len());
    if opts.preview.is_some() {
        order.sprites(instances.len());
        *out = SceneOutput {
            instances,
            text,
            quads,
            order: order.segments,
        };
        return;
    }

    // HUD: life bar.
    let bar_w = (geo.width * 0.3).min(a * 6.0);
    let bar_h = (a * 0.22).max(8.0);
    let bar_x = geo.width / 2.0;
    let bar_y = bar_h * 1.5;
    instances.push(
        Instance::new(
            [bar_x, bar_y],
            [bar_w, bar_h],
            Shape::Rect,
            [0.1, 0.1, 0.14, 0.85],
        )
        .params([0.5, 0.0, 0.0, 0.0]),
    );
    let fill_w = bar_w * frame.life.clamp(0.0, 1.0);
    if fill_w > 0.5 {
        let fill_color = if frame.danger {
            [0.95, 0.25, 0.3, 1.0]
        } else {
            [0.5, 0.9, 0.55, 1.0]
        };
        instances.push(
            Instance::new(
                [bar_x - bar_w / 2.0 + fill_w / 2.0, bar_y],
                [fill_w, bar_h * 0.7],
                Shape::Rect,
                fill_color,
            )
            .params([0.5, 0.0, 0.0, 0.0]),
        );
    }

    // HUD: score, combo, judgement.
    let field_center_x = geo.left + a * span / 2.0;
    let score_text = match (frame.score.money, frame.score.percent) {
        (Some(money), _) => format!("{money}"),
        (None, Some(p)) => format!("{:.2}%", p * 100.0),
        _ => String::new(),
    };
    if !score_text.is_empty() {
        text.push(TextItem {
            text: score_text,
            x: geo.width - a * 0.5,
            y: a * 0.35,
            size: (a * 0.5).max(14.0),
            color: [1.0, 1.0, 1.0, 0.9],
            align: Align::Right,
            bold: true,
        });
    }
    // The ghost: the lead over the best play, under the score.
    if let Some(d) = opts.ghost {
        text.push(TextItem {
            text: d.text(),
            x: geo.width - a * 0.5,
            y: a * 0.35 + (a * 0.5).max(14.0) * 0.95,
            size: (a * 0.3).max(11.0),
            color: if d.behind() {
                [1.0, 0.45, 0.45, 0.9]
            } else {
                [0.55, 0.95, 0.6, 0.9]
            },
            align: Align::Right,
            bold: true,
        });
    }
    let mid_y = geo.height / 2.0;
    if frame.combo >= 4 {
        text.push(TextItem {
            text: format!("{}", frame.combo),
            x: field_center_x,
            y: mid_y - a * 0.1,
            size: (a * 0.9).max(20.0),
            color: [1.0, 1.0, 1.0, 0.95],
            align: Align::Center,
            bold: true,
        });
    }
    // The tap judgement's middle and resting size.
    let judgement_size = (a * 0.55).max(16.0);
    let judgement_at = (
        field_center_x,
        mid_y - a * 1.1 + judgement_size * 0.6,
        judgement_size,
    );
    if let Some(flash) = &frame.judgement {
        let fade = (1.0 - (flash.age - 0.5).max(0.0) / 0.3).clamp(0.0, 1.0);
        if fade > 0.0 {
            let mut color = judgement_color(flash.judgement);
            color[3] *= fade;
            let pop = 1.0 + 0.15 * (1.0 - (flash.age / 0.12).min(1.0));
            let label = names.name(flash.judgement);
            if !label.is_empty() {
                text.push(TextItem {
                    text: label.to_string(),
                    x: field_center_x,
                    y: mid_y - a * 1.1,
                    size: popped(judgement_size, (a * 0.55 * pop).max(16.0)),
                    color,
                    align: Align::Center,
                    bold: true,
                });
            }
            if opts.show_deltas && flash.judgement != Judgement::Miss {
                let ms = flash.delta_seconds * 1000.0;
                let (label, mut c) = if ms.abs() < 0.5 {
                    ("0 ms".to_string(), [0.8, 0.9, 0.8, 1.0])
                } else if ms < 0.0 {
                    (format!("{ms:+.0} ms early"), [0.55, 0.75, 1.0, 1.0])
                } else {
                    (format!("{ms:+.0} ms late"), [1.0, 0.65, 0.45, 1.0])
                };
                c[3] *= fade;
                text.push(TextItem {
                    text: label,
                    x: field_center_x,
                    y: mid_y - a * 0.55,
                    size: (a * 0.32).max(12.0),
                    color: c,
                    align: Align::Center,
                    bold: false,
                });
            }
        }
    }

    hold_judgements(frame, layout, geo, names, judgement_at, &mut text);

    if opts.cancel_progress > 0.0 {
        let p = opts.cancel_progress.clamp(0.0, 1.0);
        let w = (geo.width * 0.22).min(a * 5.0);
        let h = (a * 0.12).max(6.0);
        let x = geo.width / 2.0;
        // At the bottom, unless receptors are there (Reverse, the lower row
        // of a two-row layout): then under the combo.
        let bottom_receptors = layout
            .lanes
            .iter()
            .any(|l| geo.lane_receptor_y(l.scroll_sign) > geo.height / 2.0);
        let y = if bottom_receptors {
            mid_y + a * 2.4
        } else {
            geo.height - a * 0.9
        };
        text.push(TextItem {
            text: if opts.cancel_hint.is_empty() {
                "hold or double-tap Esc to quit"
            } else {
                opts.cancel_hint
            }
            .into(),
            x,
            y: y - a * 0.5,
            size: (a * 0.3).max(12.0),
            color: [1.0, 1.0, 1.0, 0.8],
            align: Align::Center,
            bold: false,
        });
        instances.push(
            Instance::new([x, y], [w, h], Shape::Rect, [0.1, 0.1, 0.14, 0.8])
                .params([0.5, 0.0, 0.0, 0.0]),
        );
        if p > 0.01 {
            instances.push(
                Instance::new(
                    [x - w / 2.0 + w * p / 2.0, y],
                    [w * p, h * 0.7],
                    Shape::Rect,
                    [1.0, 0.45, 0.5, 1.0],
                )
                .params([0.5, 0.0, 0.0, 0.0]),
            );
        }
    }

    if frame.failed {
        text.push(TextItem {
            text: "FAILED".into(),
            x: field_center_x,
            y: mid_y + a * 0.8,
            size: (a * 0.8).max(24.0),
            color: [1.0, 0.3, 0.35, 1.0],
            align: Align::Center,
            bold: true,
        });
    }

    order.sprites(instances.len());
    *out = SceneOutput {
        instances,
        text,
        quads,
        order: order.segments,
    };
}

fn push_note(
    out: &mut Vec<Instance>,
    note: &NoteSprite,
    layout: &Layout,
    skin: &Skin,
    geo: &FieldGeometry,
    opts: &RenderOptions,
    song_beat: f64,
) {
    let scheme = opts.note_colors;
    let Some(lane) = layout.lanes.get(note.lane as usize) else {
        return;
    };
    let a = geo.arrow;
    let x = geo.lane_x(lane.column);
    let y = geo.lane_y(lane.scroll_sign, note.y);
    if note.alpha <= 0.0 && note.glow <= 0.0 {
        return;
    }
    let explicit = note.color.filter(|_| opts.chart_colors);
    let mut color = match explicit {
        Some(c) => rgba(c),
        None => scheme.note_color(note.quantization, note.beat_frac, song_beat),
    };
    // A flowing gradient on notes of a scheme that has one (along an
    // arrow, radial on a symbol), unless the chart colors the note or it
    // is glowing (the glow is plain white).
    let gradient = (explicit.is_none() && note.glow <= 0.0)
        .then(|| scheme.note_gradient(note.beat_frac, song_beat))
        .flatten();
    let arrow = |color: [f32; 4], gradient: Option<Gradient>| {
        let (mode, params) = match gradient {
            Some(Gradient::Flow {
                to: [r, g, b],
                phase,
            }) => (SymbolMode::Flow, [r, g, b, phase]),
            Some(Gradient::Ramp { tip: [r, g, b] }) => (SymbolMode::Ramp, [r, g, b, 0.0]),
            None => (SymbolMode::Fill, [0.0; 4]),
        };
        glyph_instance([x, y], a * 0.92, &lane.glyph, mode, color).params(params)
    };
    color = glowing(color, note);
    match note.kind {
        SpriteKind::Tap | SpriteKind::Lift | SpriteKind::Dummy => {
            out.push(arrow(color, gradient));
            if matches!(note.kind, SpriteKind::Lift) {
                out.push(Instance::new(
                    [x, y],
                    [a * 0.3, a * 0.3],
                    Shape::Circle,
                    [1.0, 1.0, 1.0, 0.8 * color[3]],
                ));
            }
        }
        SpriteKind::HoldHead { dropped, .. } | SpriteKind::RollHead { dropped, .. } => {
            if dropped {
                out.push(arrow(glowing(skin.hold_dropped, note), None));
            } else {
                out.push(arrow(color, gradient));
            }
        }
        SpriteKind::Fake => {
            color[3] *= 0.6;
            out.push(glyph_instance(
                [x, y],
                a * 0.92,
                &lane.glyph,
                SymbolMode::Fill,
                color,
            ));
        }
        SpriteKind::Mine | SpriteKind::Shock => {
            let m = glowing(skin.mine, note);
            out.push(Instance::new([x, y], [a * 0.8, a * 0.8], Shape::Mine, m));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_row_lanes_have_their_receptors_at_the_bottom() {
        let geo = FieldGeometry::new(1000.0, 1000.0, 7.0, false);
        assert_eq!(geo.arrow, 100.0);
        assert_eq!(geo.lane_receptor_y(1), 150.0);
        assert_eq!(geo.lane_receptor_y(-1), 850.0);
        // Two arrows away: below the top receptor, above the bottom one.
        assert_eq!(geo.lane_y(1, 2.0), 350.0);
        assert_eq!(geo.lane_y(-1, 2.0), 650.0);
        // Reverse swaps both.
        let rev = FieldGeometry::new(1000.0, 1000.0, 7.0, true);
        assert_eq!(rev.lane_receptor_y(1), 850.0);
        assert_eq!(rev.lane_receptor_y(-1), 150.0);
        // An 11-key field (kept as data) spans its widest row.
        let eleven = ddi_chart::layout::danoni::builtin("11")
            .unwrap()
            .layout("danoni-11".into());
        assert_eq!(field_span(&eleven), 7.0);
    }
    use ddi_chart::Quantization;
    use ddi_engine::Appearance;
    use ddi_engine::frame::ReceptorState;
    use ddi_engine::rules::presets;

    fn note(y: f32, kind: SpriteKind, appearance: Appearance) -> NoteSprite {
        let (alpha, glow) = appearance.visibility(y);
        NoteSprite {
            note_index: 0,
            lane: 0,
            y,
            kind,
            quantization: Quantization::N4,
            beat_frac: 0.0,
            color: None,
            alpha,
            glow,
            beat: 0.0,
            hold_life: 1.0,
        }
    }

    fn frame(notes: Vec<NoteSprite>, appearance: Appearance) -> Frame {
        Frame {
            song_time: 0.0,
            beat: 0.0,
            lanes: 4,
            receptors: vec![
                ReceptorState {
                    pressed: false,
                    flash: None,
                    holding: None,
                };
                4
            ],
            notes,
            judgement: None,
            holds: Default::default(),
            combo: 0,
            score: Default::default(),
            life: 0.5,
            danger: false,
            failed: false,
            finished: false,
            visible_range: 12.0,
            appearance,
        }
    }

    fn shapes(f: &Frame, opts: RenderOptions) -> Vec<Instance> {
        let layout = Layout::dance_single();
        let geo = FieldGeometry::new(1000.0, 1000.0, 4.0, false);
        build(
            f,
            &layout,
            &presets::itg().names,
            &Skin::default(),
            &geo,
            opts,
            false,
            None,
            (&[], &[]),
        )
        .instances
    }

    #[test]
    fn a_pop_is_drawn_at_a_few_sizes_and_rests_exactly() {
        let rest = (50.0f32 * 0.55).max(16.0);
        let mut sizes = Vec::new();
        // A pop at 144 frames a second.
        for f in 0..=20 {
            let age = f as f32 / 144.0;
            let pop = 1.0 + 0.15 * (1.0 - (age / 0.12).min(1.0));
            let exact = rest * pop;
            let size = popped(rest, exact);
            assert!((size - exact).abs() <= 0.25, "{size} {exact}");
            if !sizes.contains(&size.to_bits()) {
                sizes.push(size.to_bits());
            }
        }
        assert!(sizes.len() <= 10, "{}", sizes.len());
        assert_eq!(popped(rest, rest), rest);
        assert_eq!(popped(16.0, 16.0), 16.0);
    }

    /// O.K. and N.G. over every lane, then the frame's text.
    fn hold_text(place: ddi_engine::frame::HoldPlace, names: &JudgeNames) -> Vec<TextItem> {
        use ddi_engine::frame::{HoldFlash, HoldJudgements};
        let mut f = frame(Vec::new(), Appearance::Visible);
        f.holds = HoldJudgements {
            place,
            flashes: (0..4)
                .map(|lane| HoldFlash {
                    lane,
                    held: lane % 2 == 0,
                    age: 0.85,
                })
                .collect(),
        };
        let geo = FieldGeometry::new(1000.0, 1000.0, 4.0, false);
        build(
            &f,
            &Layout::dance_single(),
            names,
            &Skin::default(),
            &geo,
            RenderOptions::default(),
            false,
            None,
            (&[], &[]),
        )
        .text
    }

    #[test]
    fn hold_outcomes_are_centred_past_the_receptors_and_fit_their_lanes() {
        use ddi_engine::frame::HoldPlace;
        let geo = FieldGeometry::new(1000.0, 1000.0, 4.0, false);
        let a = geo.arrow;
        let mut names = presets::itg().names;
        // A long name must shrink to its lane, not run into the next.
        names.let_go = "Dropped hold".into();
        let text = hold_text(HoldPlace::Lanes, &names);
        assert_eq!(text.len(), 4);
        let middle = geo.lane_receptor_y(1) + a * 54.0 / 64.0;
        for t in &text {
            // N.G. has sunk its 20 units of 64 by now (from 10 above).
            let sunk = if t.text == names.held {
                0.0
            } else {
                a * 10.0 / 64.0
            };
            assert!((t.y + t.size * 0.6 - middle - sunk).abs() < 0.01, "{t:?}");
            assert!(t.size / fit_width(&t.text, 1.0) <= a * 0.9 + 0.01, "{t:?}");
        }
    }

    #[test]
    fn the_freeze_judgement_sits_beside_the_tap_judgement() {
        use ddi_engine::frame::HoldPlace;
        let names = presets::danoni().names;
        let geo = FieldGeometry::new(1000.0, 1000.0, 4.0, false);
        let a = geo.arrow;
        let text = hold_text(HoldPlace::Centre, &names);
        assert_eq!(text.len(), 4);
        let size = (a * 0.55).max(16.0);
        let tap_middle = geo.height / 2.0 - a * 1.1 + size * 0.6;
        for t in &text {
            assert_eq!(t.size, size);
            assert!((t.y + t.size * 0.6 - (tap_middle + a * 1.4)).abs() < 0.01);
            assert_eq!(t.color[3], 1.0);
        }
    }

    #[test]
    fn the_quit_button_sits_in_the_corner_over_everything() {
        let single = Layout::dance_single();
        // A phone upright at 2×: the field from 130 to 650 of 780 px, the
        // receptors at the top; the button at the bottom, 8 in, in the
        // middle of the margin left of the field.
        let geo = FieldGeometry::new(780.0, 1688.0, 4.0, false);
        let [x, y, r] = quit_button(&geo, &single, 2.0, [0.0; 4]);
        assert_eq!(r, 44.0);
        assert_eq!(y, 1688.0 - 16.0 - 44.0);
        assert_eq!(x, 65.0);
        assert!(x + r < geo.left);
        // Under Reverse at the top, past a notch's inset.
        let rev = FieldGeometry::new(780.0, 1688.0, 4.0, true);
        let [_, y, _] = quit_button(&rev, &single, 2.0, [47.0, 0.0, 0.0, 0.0]);
        assert_eq!(y, 94.0 + 44.0);
        // Sideways, the field leaves room: halfway into the margin.
        let wide = FieldGeometry::new(844.0, 390.0, 4.0, false);
        let [x, ..] = quit_button(&wide, &single, 1.0, [0.0; 4]);
        assert_eq!(x, wide.left / 2.0);
        // Clear of every receptor and lane: wholly left of the field, or
        // away from the receptors.
        for g in [geo, wide] {
            let [x, y, r] = quit_button(&g, &single, 1.0, [0.0; 4]);
            let receptor = g.lane_receptor_y(1);
            assert!(x + r <= g.left || (y - receptor).abs() > g.arrow * 3.0);
        }
        let f = frame(Vec::new(), Appearance::Visible);
        let geo = FieldGeometry::new(1000.0, 1000.0, 4.0, false);
        let mut out = build(
            &f,
            &Layout::dance_single(),
            &presets::itg().names,
            &Skin::default(),
            &geo,
            RenderOptions::default(),
            false,
            None,
            (&[], &[]),
        );
        let before = out.instances.len() as u32;
        add_quit_button(&mut out, [30.0, 30.0, 22.0]);
        let end = out.instances.len() as u32;
        assert_eq!(end, before + 4);
        // Drawn last.
        let Some(Segment::Sprites(r)) = out.order.last() else {
            panic!("{:?}", out.order)
        };
        assert_eq!(r.end, end);
    }

    #[test]
    fn a_preview_draws_the_field_and_no_hud() {
        let a = Appearance::Visible;
        let f = frame(vec![note(1.0, SpriteKind::Tap, a)], a);
        let layout = Layout::dance_single();
        let geo = FieldGeometry::new(1000.0, 1000.0, 4.0, false);
        let out = |preview: Option<Preview>| {
            let opts = RenderOptions {
                preview,
                ..RenderOptions::default()
            };
            build(
                &f,
                &layout,
                &presets::itg().names,
                &Skin::default(),
                &geo,
                opts,
                false,
                None,
                (&[], &[]),
            )
        };
        let play = out(None);
        let with = out(Some(Preview {
            geometry: geo,
            receptors: true,
        }));
        let without = out(Some(Preview {
            geometry: geo,
            receptors: false,
        }));
        // The play adds the HUD (here the life bar).
        assert!(with.text.is_empty() && without.text.is_empty());
        assert_eq!(play.instances.len(), with.instances.len() + 2);
        // Four receptors, then the note alone.
        assert_eq!(with.instances.len(), 5);
        assert_eq!(without.instances.len(), 1);
    }

    fn count(instances: &[Instance], shape: Shape) -> usize {
        instances.iter().filter(|i| i.shape == shape as u32).count()
    }

    #[test]
    fn hidden_notes_are_not_drawn_and_hold_bodies_are_clipped() {
        let a = Appearance::Hidden;
        let hold = SpriteKind::HoldHead {
            tail_y: 6.0,
            active: false,
            dropped: false,
        };
        let f = frame(vec![note(1.0, SpriteKind::Tap, a), note(0.5, hold, a)], a);
        let out = shapes(&f, RenderOptions::default());
        // Receptors only: the tap and the hold head are hidden.
        assert_eq!(count(&out, Shape::Arrow), 0);
        // The body is drawn only above the hidden line (140 px / 48 = 2.92).
        let bodies: Vec<&Instance> = out
            .iter()
            .filter(|i| i.shape == Shape::HoldBody as u32)
            .collect();
        assert_eq!(bodies.len(), 1);
        let geo = FieldGeometry::new(1000.0, 1000.0, 4.0, false);
        let top = bodies[0].center[1] - bodies[0].size[1] / 2.0;
        assert!((top - geo.y(140.0 / 48.0)).abs() < 0.5, "{top}");

        let f = frame(
            vec![note(1.0, SpriteKind::Tap, Appearance::Visible)],
            Appearance::Visible,
        );
        assert_eq!(
            count(&shapes(&f, RenderOptions::default()), Shape::Arrow),
            1
        );
    }

    #[test]
    fn a_note_at_the_switch_glows_white() {
        let a = Appearance::Hidden;
        let mut n = note(140.0 / 48.0 - 0.01, SpriteKind::Tap, a);
        assert_eq!(n.alpha, 0.0);
        assert!(n.glow > 1.0);
        let out = shapes(&frame(vec![n], a), RenderOptions::default());
        let arrow = out.iter().find(|i| i.shape == Shape::Arrow as u32).unwrap();
        assert_eq!(arrow.color, [1.0, 1.0, 1.0, 1.0]);
        n.alpha = 1.0;
        n.glow = 0.5;
        let out = shapes(&frame(vec![n], a), RenderOptions::default());
        let arrow = out.iter().find(|i| i.shape == Shape::Arrow as u32).unwrap();
        let red = quantization_color(Quantization::N4);
        assert!((arrow.color[1] - (red[1] + (1.0 - red[1]) * 0.5)).abs() < 1e-6);
    }

    #[test]
    fn the_color_scheme_reaches_the_arrows() {
        let f = frame(
            vec![note(1.0, SpriteKind::Tap, Appearance::Visible)],
            Appearance::Visible,
        );
        let single = [0.1, 0.2, 0.3, 1.0];
        let out = shapes(
            &f,
            RenderOptions {
                note_colors: ColorScheme::Single(single),
                ..Default::default()
            },
        );
        let arrow = out.iter().find(|i| i.shape == Shape::Arrow as u32).unwrap();
        assert_eq!(arrow.color, single);
    }

    #[test]
    fn rainbow_arrows_carry_a_flowing_gradient() {
        let f = frame(
            vec![note(1.0, SpriteKind::Tap, Appearance::Visible)],
            Appearance::Visible,
        );
        let out = shapes(
            &f,
            RenderOptions {
                note_colors: ColorScheme::Rainbow,
                ..Default::default()
            },
        );
        let arrow = out
            .iter()
            .find(|i| i.shape == Shape::ArrowGradient as u32)
            .expect("gradient arrow");
        // A quarter note: orange running to yellow, at the song's beat phase.
        let Some(Gradient::Flow { to, phase }) = ColorScheme::Rainbow.note_gradient(0.0, f.beat)
        else {
            panic!("rainbow flows");
        };
        assert_eq!(arrow.params, [to[0], to[1], to[2], phase]);
    }

    #[test]
    fn the_ghost_shows_under_the_score() {
        assert_eq!(ScoreDelta::Percent(0.4199).text(), "+0.42%");
        assert_eq!(ScoreDelta::Percent(-1.5).text(), "-1.50%");
        assert_eq!(ScoreDelta::Percent(-0.001).text(), "+0.00%");
        assert_eq!(ScoreDelta::Points(-1230).text(), "-1230");
        assert_eq!(ScoreDelta::Points(0).text(), "+0");
        let mut f = frame(Vec::new(), Appearance::Visible);
        f.score.percent = Some(0.5);
        let layout = Layout::dance_single();
        let geo = FieldGeometry::new(1000.0, 1000.0, 4.0, false);
        let texts = |ghost| {
            build(
                &f,
                &layout,
                &presets::itg().names,
                &Skin::default(),
                &geo,
                RenderOptions {
                    ghost,
                    ..RenderOptions::default()
                },
                false,
                None,
                (&[], &[]),
            )
            .text
            .into_iter()
            .map(|t| t.text)
            .collect::<Vec<_>>()
        };
        assert!(
            !texts(None)
                .iter()
                .any(|t| t.ends_with('%') && t != "50.00%")
        );
        assert!(texts(Some(ScoreDelta::Percent(-2.0))).contains(&"-2.00%".to_string()));
    }

    #[test]
    fn field_filter_darkens_behind_the_lanes() {
        let f = frame(Vec::new(), Appearance::Visible);
        let off = shapes(&f, RenderOptions::default());
        let on = shapes(
            &f,
            RenderOptions {
                field_filter: 0.4,
                ..Default::default()
            },
        );
        assert_eq!(on.len(), off.len() + 1);
        let band = on[0];
        assert_eq!(band.shape, Shape::Rect as u32);
        assert!((band.color[3] - 0.4).abs() < 1e-6);
        assert!((filter_alpha(0.4, true) - (1.0 - 0.6f32.powf(2.2))).abs() < 1e-6);
        for srgb in [false, true] {
            assert!(filter_alpha(0.0, srgb) == 0.0 && filter_alpha(1.0, srgb) == 1.0);
        }
    }
}
