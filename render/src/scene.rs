//! Turns an engine [`Frame`] into sprite instances and HUD text.
//!
//! Geometry is derived from the target size every frame: the note field is
//! centred, one lane is one arrow wide, and one "arrow height" of engine
//! distance is one arrow of pixels (x-mod 1 = one arrow per beat).

use ddi_chart::{Color, Glyph, Layout, Quantization};
use ddi_engine::frame::{Frame, NoteSprite, SpriteKind};
use ddi_engine::rules::{JudgeNames, Judgement};

use crate::sprite::{Instance, Shape};
use crate::text::{Align, TextItem};

/// Colour choice for notes.
#[derive(Clone, Debug, PartialEq)]
pub enum ColorScheme {
    /// One colour for everything.
    Static([f32; 4]),
    /// Colour by subdivision (DDR "NOTE", ITG default).
    Quantized,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Skin {
    pub scheme: ColorScheme,
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
            scheme: ColorScheme::Quantized,
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

pub fn quantization_color(q: Quantization) -> [f32; 4] {
    match q {
        Quantization::N4 => [0.95, 0.25, 0.3, 1.0],
        Quantization::N8 => [0.3, 0.5, 1.0, 1.0],
        Quantization::N12 => [0.7, 0.35, 0.95, 1.0],
        Quantization::N16 => [0.95, 0.85, 0.25, 1.0],
        Quantization::N24 => [0.95, 0.45, 0.75, 1.0],
        Quantization::N32 => [0.95, 0.6, 0.25, 1.0],
        Quantization::N48 => [0.3, 0.85, 0.9, 1.0],
        Quantization::N64 => [0.35, 0.9, 0.45, 1.0],
        Quantization::N192 => [0.75, 0.75, 0.75, 1.0],
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
    pub fn new(width: f32, height: f32, lanes: usize, reverse: bool) -> FieldGeometry {
        let by_height = height / 10.0;
        let by_width = width / (lanes as f32 + 2.0);
        let arrow = by_height.min(by_width).clamp(24.0, 160.0);
        let field_w = arrow * lanes as f32;
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

fn glyph_shape(glyph: &Glyph, outline: bool) -> Shape {
    match glyph {
        Glyph::Arrow { .. } => {
            if outline {
                Shape::ArrowOutline
            } else {
                Shape::Arrow
            }
        }
        Glyph::Onigiri => Shape::Onigiri,
        _ => Shape::Circle,
    }
}

/// Build the sprite and text lists for one frame.
pub fn build(
    frame: &Frame,
    layout: &Layout,
    names: &JudgeNames,
    skin: &Skin,
    geo: &FieldGeometry,
    opts: RenderOptions,
) -> SceneOutput {
    let mut instances = Vec::with_capacity(frame.notes.len() * 2 + 32);
    let mut text = Vec::with_capacity(8);
    let a = geo.arrow;

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
        instances.push(
            Instance::new(
                [geo.lane_x(lane.column), geo.receptor_y],
                [a * 0.92 * scale, a * 0.92 * scale],
                glyph_shape(&lane.glyph, true),
                color,
            )
            .rotated(glyph_rotation(&lane.glyph)),
        );
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
            let y0 = geo.y(note.y);
            let y1 = geo.y(tail_y);
            let color = if dropped {
                skin.hold_dropped
            } else if active {
                skin.hold_active
            } else {
                skin.hold_inactive
            };
            let is_roll = matches!(note.kind, SpriteKind::RollHead { .. });
            let w = if is_roll { a * 0.38 } else { a * 0.5 };
            let center_y = (y0 + y1) / 2.0;
            let h = (y1 - y0).abs().max(1.0);
            instances.push(
                Instance::new(
                    [geo.lane_x(lane.column), center_y],
                    [w, h],
                    Shape::HoldBody,
                    [color[0], color[1], color[2], color[3] * note.alpha],
                )
                .params([0.6, 0.0, 0.0, 0.0]),
            );
        }
    }

    // Notes.
    for note in notes {
        push_note(&mut instances, note, layout, skin, geo);
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
    let field_center_x = geo.left + a * layout.lanes.len() as f32 / 2.0;
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
        let y = geo.height - a * 0.9;
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
) {
    let Some(lane) = layout.lanes.get(note.lane as usize) else {
        return;
    };
    let a = geo.arrow;
    let x = geo.lane_x(lane.column);
    let y = geo.y(note.y);
    let mut color = match (&skin.scheme, note.color) {
        (_, Some(c)) => rgba(c),
        (ColorScheme::Static(c), None) => *c,
        (ColorScheme::Quantized, None) => quantization_color(note.quantization),
    };
    color[3] *= note.alpha;
    match note.kind {
        SpriteKind::Tap | SpriteKind::Lift | SpriteKind::Dummy => {
            out.push(
                Instance::new(
                    [x, y],
                    [a * 0.92, a * 0.92],
                    glyph_shape(&lane.glyph, false),
                    color,
                )
                .rotated(glyph_rotation(&lane.glyph)),
            );
            if matches!(note.kind, SpriteKind::Lift) {
                out.push(Instance::new(
                    [x, y],
                    [a * 0.3, a * 0.3],
                    Shape::Circle,
                    [1.0, 1.0, 1.0, 0.8 * note.alpha],
                ));
            }
        }
        SpriteKind::HoldHead { dropped, .. } | SpriteKind::RollHead { dropped, .. } => {
            let c = if dropped { skin.hold_dropped } else { color };
            out.push(
                Instance::new(
                    [x, y],
                    [a * 0.92, a * 0.92],
                    glyph_shape(&lane.glyph, false),
                    c,
                )
                .rotated(glyph_rotation(&lane.glyph)),
            );
        }
        SpriteKind::Fake => {
            color[3] *= 0.6;
            out.push(
                Instance::new(
                    [x, y],
                    [a * 0.92, a * 0.92],
                    glyph_shape(&lane.glyph, false),
                    color,
                )
                .rotated(glyph_rotation(&lane.glyph)),
            );
        }
        SpriteKind::Mine | SpriteKind::Shock => {
            let mut m = skin.mine;
            m[3] *= note.alpha;
            out.push(Instance::new([x, y], [a * 0.8, a * 0.8], Shape::Mine, m));
        }
    }
}
