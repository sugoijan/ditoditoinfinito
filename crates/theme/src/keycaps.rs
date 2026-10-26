//! Key-specific artwork and custom printing on the same cap bodies.
use super::*;
use ddi_actor::text::TextCurve;
use ddi_ui::keycaps::{Arrow, KeyShape, KeyboardKey, Keycap};

/// Charcoal finishes and neutral gray satin alternatives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeycapMaterial {
    Satin,
    SemiGloss,
    LightSatin,
    GraySatin,
    DeepGraySatin,
}

impl KeycapMaterial {
    pub(super) fn image(self) -> &'static str {
        match self {
            Self::Satin => "ui/keycaps-a1.svg",
            Self::SemiGloss => "ui/keycaps-a2.svg",
            Self::LightSatin => "ui/keycaps-light.svg",
            Self::GraySatin => "ui/keycaps-gray.svg",
            Self::DeepGraySatin => "ui/keycaps-deep-gray.svg",
        }
    }
}

impl Infinito {
    fn keycap_ink_color(&self) -> &'static str {
        match self.keycap_material.get() {
            KeycapMaterial::LightSatin
            | KeycapMaterial::GraySatin
            | KeycapMaterial::DeepGraySatin => "Keycap.LightInk",
            _ => "Text",
        }
    }

    fn keycap_ink(&self) -> Rgba {
        self.c(self.keycap_ink_color())
    }

    /// Choose a finish before building/rebuilding the screen. Deep gray
    /// satin is the default; earlier sources remain available for review.
    pub fn set_keycap_material(&self, material: KeycapMaterial) {
        self.keycap_material.set(material);
    }
}

const FONT_SIZE: f32 = 10.0;
const ARROW_ADVANCE: f32 = 10.0;
const CAP_HEIGHT: f32 = 26.0;
const CAP_SCALE: f32 = CAP_HEIGHT / 32.0;

fn legend_width(legend: &str) -> f32 {
    legend
        .chars()
        .map(|c| {
            if Arrow::from_symbol(c).is_some() {
                ARROW_ADVANCE
            } else {
                mono_width(&c.to_string(), FONT_SIZE)
            }
        })
        .sum()
}

/// One master arrow, rotated without stretching. Eight units from tail to
/// tip and six across the head in every direction, independent of the font.
fn arrow_paths(direction: Arrow) -> [Vec<[f32; 2]>; 2] {
    let rotate = |[x, y]: [f32; 2]| match direction {
        Arrow::Right => [x, y],
        Arrow::Down => [-y, x],
        Arrow::Left => [-x, -y],
        Arrow::Up => [y, -x],
    };
    [
        vec![[-4.0, 0.0], [4.0, 0.0]],
        vec![[1.0, -3.0], [4.0, 0.0], [1.0, 3.0]],
    ]
    .map(|path| path.into_iter().map(rotate).collect())
}

/// Stroke a path on the cap surface, with the curve's x origin at the
/// legend's pen. Symbols and text share the same surface projection.
fn strokes(
    paths: &[&[[f32; 2]]],
    origin: [f32; 2],
    curve: Option<TextCurve>,
    color: Rgba,
    thickness: f32,
) -> Vec<Actor> {
    let mut actors = Vec::new();
    for path in paths {
        for ends in path.windows(2) {
            for j in 0..4 {
                let point = |t: f32| {
                    let x = ends[0][0] + (ends[1][0] - ends[0][0]) * t;
                    [
                        origin[0] + x,
                        origin[1]
                            + ends[0][1]
                            + (ends[1][1] - ends[0][1]) * t
                            + curve.map_or(0.0, |c| c.offset(x)),
                    ]
                };
                let a = point(j as f32 / 4.0);
                let b = point((j + 1) as f32 / 4.0);
                let dx = b[0] - a[0];
                let dy = b[1] - a[1];
                let mut stroke = quad("", [dx.hypot(dy) + 0.1, thickness], color);
                call(&mut stroke, "rotationz", &[dy.atan2(dx).to_degrees()]);
                actors.push(at(stroke, (a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0));
            }
        }
    }
    actors
}

impl Infinito {
    /// Composite captions also use the master arrows, so "↑↓" and
    /// "2×↑↓←→" do not bring the old unequal font glyphs back into footers.
    fn cap_legend(&self, name: &str, legend: &str, curve: Option<TextCurve>) -> Actor {
        if !legend.chars().any(|c| Arrow::from_symbol(c).is_some()) {
            let mut label = self.text(name, legend, FONT_SIZE, 400, self.keycap_ink_color());
            if let Kind::Text(text) = &mut label.kind {
                let mut style = text.style().clone();
                style.italic = true;
                text.set_style(style);
                text.curve = curve;
            }
            return at_left(label, 0.0, 0.0);
        }
        let mut children = Vec::new();
        let mut x = 0.0;
        for ch in legend.chars() {
            if let Some(arrow) = Arrow::from_symbol(ch) {
                let paths = arrow_paths(arrow).map(|p| {
                    p.into_iter()
                        .map(|[px, py]| [px + x + ARROW_ADVANCE / 2.0, py])
                        .collect::<Vec<_>>()
                });
                children.extend(strokes(
                    &[&paths[0], &paths[1]],
                    [0.0, 1.0],
                    curve,
                    self.keycap_ink(),
                    0.8,
                ));
                x += ARROW_ADVANCE;
            } else {
                let shifted = curve.map(|c| TextCurve {
                    center: c.center - x,
                    ..c
                });
                let label = self.cap_legend("", &ch.to_string(), shifted);
                children.push(at(label, x, curve.map_or(0.0, |c| c.offset(x))));
                x += mono_width(&ch.to_string(), FONT_SIZE);
            }
        }
        frame(name, children)
    }

    pub(super) fn key_hint(&self, spec: &Spec) -> Actor {
        let legacy;
        let caps = if spec.keycaps.is_empty() {
            legacy = spec
                .items
                .iter()
                .map(|k| Keycap::from_hint(k))
                .collect::<Vec<_>>();
            &legacy
        } else {
            &spec.keycaps
        };
        let mut children = Vec::new();
        let mut x = 0.0;
        if !spec.key_prefix.is_empty() {
            children.push(at_left(
                self.text("Prefix", &spec.key_prefix, 13.0, 400, "Muted"),
                x,
                0.0,
            ));
            x += mono_width(&spec.key_prefix, 13.0) + 4.0;
        }
        for (i, cap) in caps.iter().enumerate() {
            let legend = cap.label();
            let key = match cap {
                Keycap::Key(k) => Some(*k),
                Keycap::Custom { .. } => None,
            };
            let space = cap.shape() == KeyShape::Space;
            let mark = matches!(key, Some(KeyboardKey::Enter | KeyboardKey::Tab));
            let arrow = match key {
                Some(KeyboardKey::Arrow(a)) => Some(a),
                _ => None,
            };
            let minimum: f32 = if space {
                64.0
            } else {
                match key {
                    Some(KeyboardKey::Enter) => 64.0,
                    Some(KeyboardKey::Tab) => 48.0,
                    _ => 32.0,
                }
            };
            let width = (legend_width(&legend)
                + (15.0 + if mark { 12.0 } else { 0.0 }) * CAP_SCALE)
                .max(minimum * CAP_SCALE);
            let inset = if space {
                (width - legend_width(&legend)) / 2.0
            } else {
                7.5 * CAP_SCALE
            };
            let model = ((width / CAP_HEIGHT - 1.0) / 0.375).round().clamp(0.0, 3.0);
            let (row, source_width) = if space {
                (4.0, 512.0)
            } else {
                (model, 256.0 + model * 96.0)
            };
            let mut body = Sprite::new(
                0,
                vec![ddi_actor::State {
                    rect: [0.0, row / 5.0, source_width / 768.0, (row + 1.0) / 5.0],
                    delay: 1.0,
                }],
                [width, CAP_HEIGHT],
            );
            body.shape.hide_unloaded = true;
            children.push(round(
                plate(
                    "",
                    [x + width * 0.22, -11.0 * CAP_SCALE],
                    [width * 0.56, 22.0 * CAP_SCALE],
                    self.c(match self.keycap_material.get() {
                        KeycapMaterial::LightSatin => "Keycap.LightFace",
                        KeycapMaterial::GraySatin => "Keycap.GrayFace",
                        KeycapMaterial::DeepGraySatin => "Keycap.DeepGrayFace",
                        _ => "Keycap.Face",
                    }),
                ),
                1.5 * CAP_SCALE,
            ));
            children.push(at(
                named(Kind::Sprite(body), &format!("Cap{i}")),
                x + width / 2.0,
                0.0,
            ));
            // A 1 mm dish, seen from 38 mm forward / 85 mm above, projects
            // to 0.568 px at the 32 px study size. Its horizontal extent
            // follows the selected mesh; its depth scales with cap height.
            let face_half = (6.8 + model * 4.75) * (32.0 / 23.0) * width / (32.0 + model * 12.0);
            let curve = (!space).then_some(TextCurve {
                center: width / 2.0 - inset,
                quadratic: -0.568 * CAP_SCALE / face_half.powi(2),
            });
            if let Some(arrow) = arrow {
                // Cherry-style mixed placement: horizontal arrows centered,
                // vertical arrows slightly left; all share the same height.
                let anchor = match arrow {
                    Arrow::Up | Arrow::Down => [12.0 * CAP_SCALE, -3.5 * CAP_SCALE],
                    Arrow::Left | Arrow::Right => [width / 2.0, -3.5 * CAP_SCALE],
                };
                let paths = arrow_paths(arrow);
                let curve = Some(TextCurve {
                    center: width / 2.0 - anchor[0],
                    quadratic: -0.568 * CAP_SCALE / face_half.powi(2),
                });
                children.push(frame(
                    &format!("Key{i}"),
                    strokes(
                        &[&paths[0], &paths[1]],
                        [x + anchor[0], anchor[1]],
                        curve,
                        self.keycap_ink(),
                        0.8,
                    ),
                ));
            } else {
                children.push(at(
                    self.cap_legend(&format!("Key{i}"), &legend, curve),
                    x + inset,
                    if space {
                        -2.0 * CAP_SCALE
                    } else {
                        -5.5 * CAP_SCALE
                    },
                ));
            }
            if mark {
                let paths: &[&[[f32; 2]]] = if key == Some(KeyboardKey::Enter) {
                    &[
                        &[[7.0, -3.0], [7.0, 1.0], [0.0, 1.0]],
                        &[[2.5, -1.5], [0.0, 1.0], [2.5, 3.5]],
                    ]
                } else {
                    &[
                        &[[0.0, 0.0], [6.0, 0.0]],
                        &[[3.5, -2.5], [6.0, 0.0], [3.5, 2.5]],
                        &[[8.0, -3.5], [8.0, 3.5]],
                    ]
                };
                let sx = legend_width(&legend) + 3.0 * CAP_SCALE;
                let paths: Vec<Vec<_>> = paths
                    .iter()
                    .map(|p| {
                        p.iter()
                            .map(|[px, py]| [px * CAP_SCALE + sx, py * CAP_SCALE])
                            .collect()
                    })
                    .collect();
                children.push(frame(
                    &format!("Mark{i}"),
                    strokes(
                        &paths.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                        [x + inset, -4.5 * CAP_SCALE],
                        curve,
                        self.keycap_ink(),
                        0.8 * CAP_SCALE,
                    ),
                ));
            }
            x += width;
        }
        if !spec.text.is_empty() {
            x += 6.0;
        }
        children.push(at_left(
            self.text("Label", &spec.text, 13.0, 400, "Muted"),
            x,
            0.0,
        ));
        let width = x + mono_width(&spec.text, 13.0);
        let mut inner = frame("Inner", children);
        call(&mut inner, "x", &[-width / 2.0]);
        frame(
            "",
            vec![inner, extent([width, spec.size[1].max(CAP_HEIGHT)])],
        )
    }
}
