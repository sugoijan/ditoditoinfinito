//! Turns an engine [`Frame`] into sprite instances and HUD text.
//!
//! Geometry is derived from the target size every frame: the note field is
//! centred, one lane is one arrow wide, and one "arrow height" of engine
//! distance is one arrow of pixels (x-mod 1 = one arrow per beat).

use ddi_chart::{Color, Glyph, Layout};
use ddi_engine::frame::{Frame, NoteSprite, SpriteKind};
use ddi_engine::rules::{JudgeNames, Judgement};

use crate::sprite::{Instance, Shape, Symbol, SymbolMode};
use crate::text::{Align, TextItem};

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
                r: 0.02,
                g: 0.01,
                b: 0.05,
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
#[derive(Clone, Copy, Debug)]
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
        if self.lane_direction(scroll_sign) > 0.0 {
            self.arrow * 1.5
        } else {
            self.height - self.arrow * 1.5
        }
    }

    /// Screen y of a lane's note at an engine distance in arrow heights.
    pub fn lane_y(&self, scroll_sign: i8, arrows: f32) -> f32 {
        self.lane_receptor_y(scroll_sign) + arrows * self.arrow * self.lane_direction(scroll_sign)
    }
}

pub struct SceneOutput {
    pub instances: Vec<Instance>,
    pub text: Vec<TextItem>,
}

/// Per-frame presentation switches that are not part of the skin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderOptions {
    /// Receptors at the bottom, notes travel down.
    pub reverse: bool,
    /// Draw receptors and HUD but no notes (audio-only calibration).
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
    /// How notes are coloured (an explicit chart colour still wins).
    pub note_colors: ColorScheme,
    /// Darkening behind the lanes, `0..=1` of perceived brightness removed
    /// (0 = none).
    pub field_filter: f32,
}

/// A note's colour under Hidden/Sudden: hidden notes show only their white
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
/// standing for Dancing☆Onigiri's other lanes (decision 27): a star for the
/// onigiri, a square for giko, a circle for iyo and anything else. A
/// triangle would read as an up arrow.
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
        Glyph::Giko => Symbol::Square,
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
pub fn build(
    frame: &Frame,
    layout: &Layout,
    names: &JudgeNames,
    skin: &Skin,
    geo: &FieldGeometry,
    opts: RenderOptions,
    srgb_target: bool,
) -> SceneOutput {
    let mut instances = Vec::with_capacity(frame.notes.len() * 2 + 32);
    let mut text = Vec::with_capacity(8);
    let a = geo.arrow;

    let span = field_span(layout);
    // Field filter: a dark band behind the lanes, over the background.
    if opts.field_filter > 0.0 && !layout.lanes.is_empty() {
        let lanes = span;
        let w = a * (lanes + 0.3);
        instances.push(
            Instance::new(
                [geo.left + a * lanes / 2.0, geo.height / 2.0],
                [w, geo.height + 2.0],
                Shape::Rect,
                [0.0, 0.0, 0.0, filter_alpha(opts.field_filter, srgb_target)],
            )
            .params([0.0, 0.0, 0.0, 0.0]),
        );
    }

    // Receptors.
    for (i, lane) in layout.lanes.iter().enumerate() {
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

    // Notes.
    for note in notes {
        push_note(
            &mut instances,
            note,
            layout,
            skin,
            geo,
            opts.note_colors,
            frame.beat,
        );
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
                    size: (a * 0.55 * pop).max(16.0),
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

    SceneOutput { instances, text }
}

fn push_note(
    out: &mut Vec<Instance>,
    note: &NoteSprite,
    layout: &Layout,
    skin: &Skin,
    geo: &FieldGeometry,
    scheme: ColorScheme,
    song_beat: f64,
) {
    let Some(lane) = layout.lanes.get(note.lane as usize) else {
        return;
    };
    let a = geo.arrow;
    let x = geo.lane_x(lane.column);
    let y = geo.lane_y(lane.scroll_sign, note.y);
    if note.alpha <= 0.0 && note.glow <= 0.0 {
        return;
    }
    let mut color = match note.color {
        Some(c) => rgba(c),
        None => scheme.note_color(note.quantization, note.beat_frac, song_beat),
    };
    // A flowing gradient on notes of a scheme that has one (along an
    // arrow, radial on a symbol), unless the chart colours the note or it
    // is glowing (the glow is plain white).
    let gradient = (note.color.is_none() && note.glow <= 0.0)
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
            lane: 0,
            y,
            kind,
            quantization: Quantization::N4,
            beat_frac: 0.0,
            color: None,
            alpha,
            glow,
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
                    flash: None
                };
                4
            ],
            notes,
            judgement: None,
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
        )
        .instances
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
    fn the_colour_scheme_reaches_the_arrows() {
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
