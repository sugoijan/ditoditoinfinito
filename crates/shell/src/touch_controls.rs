//! A play's touch controls drawn (`docs/plans/touch-controls.md`): the
//! buttons or the pad `ddi_ui::touch_controls` places, over the field
//! (which stays as it is), translucent, each lane's target showing its
//! receptor's glyph turned as the receptor is, lit while it is held. Drawn
//! by the play's 2D pass while touch is the input source; hit-tested by
//! the shells from the same plan (`Drawn::touch`).
//!
//! Three looks, a choice still open (testing only: the web's `touchlook=`,
//! desktop's `DDI_TOUCH_LOOK`), [`Look`]: a plain glass plate, the dance
//! mat (after the dancing hand's pad: dark tiles, each press lighting its
//! tile in a bright hue of its own that stays a moment after the release
//! and spills onto the tiles around it), and the glyphs alone.

use ddi_chart::layout::Glyph;
use ddi_render::draw2d::{Affine, Quad, Shape};
use ddi_render::texture_cache::{TextureId, TextureKey};
use ddi_render::{DrawList, Painter};
use ddi_ui::touch_controls::glyphs::{self as atlas, Weight};
use ddi_ui::touch_controls::{Controls, Kind, Target};

/// How the controls look (a choice for the maintainer).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    /// A quiet glass plate and the glyph; a press brightens both.
    Glass,
    /// The dancing hand's mat: dark tiles lit in a hue a press, with a
    /// sustain and a spill.
    #[default]
    Mat,
    /// The glyphs alone, a faint outline round each target; a press
    /// fills a plate behind the glyph.
    Glyphs,
}

impl Look {
    /// From its testing name (`glass`, `mat`, `glyphs`).
    pub fn from_name(name: &str) -> Option<Look> {
        match name {
            "glass" | "plate" => Some(Look::Glass),
            "mat" => Some(Look::Mat),
            "glyphs" | "outline" => Some(Look::Glyphs),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Look::Glass => "glass",
            Look::Mat => "mat",
            Look::Glyphs => "glyphs",
        }
    }
}

/// What a play's view draws of the touch controls (the shell's choice).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchSpec {
    pub choice: ddi_ui::touch_controls::Choice,
    /// The safe area's insets (logical pixels: top, right, bottom, left).
    pub insets: [f32; 4],
    /// Touch is the input source: they show.
    pub shown: bool,
    pub look: Look,
}

/// A frame's controls to draw.
pub(crate) struct TouchFrame {
    pub(crate) controls: Controls,
    pub(crate) glyphs: Vec<Glyph>,
    pub(crate) held: Vec<bool>,
    pub(crate) now: f64,
    pub(crate) reduced: bool,
    pub(crate) look: Look,
}

/// How long a lit mat tile stays lit after its release (the hand's
/// `CONTACT_SUSTAIN`), then how long it fades.
const SUSTAIN: f64 = 0.08;
const FADE: f64 = 0.2;

/// The mat's tile (the hand's mat color) and its edge.
const MAT: [f32; 4] = [0.055, 0.068, 0.11, 0.5];
const MAT_EDGE: [f32; 4] = [0.62, 0.7, 0.92, 0.28];

/// What the controls remember between frames: each lane's light.
#[derive(Default)]
pub struct TouchView {
    /// Each lane held at the last frame, since when (host seconds), and
    /// how many times it was pressed (each press its own hue).
    lanes: Vec<(bool, f64, u32)>,
}

impl TouchView {
    /// Each lane's light now, `0..=1`: 1 while held, then the look's
    /// sustain and fade (none under reduced motion).
    fn lights(&mut self, held: &[bool], now: f64, reduced: bool, look: Look) -> Vec<(f32, u32)> {
        self.lanes.resize(held.len(), (false, f64::NEG_INFINITY, 0));
        let mut out = Vec::with_capacity(held.len());
        for (lane, &down) in self.lanes.iter_mut().zip(held) {
            if lane.0 != down {
                lane.0 = down;
                lane.1 = now;
                if down {
                    lane.2 = lane.2.wrapping_add(1);
                }
            }
            let light = if down {
                1.0
            } else if reduced {
                0.0
            } else {
                let (sustain, fade) = match look {
                    Look::Mat => (SUSTAIN, FADE),
                    _ => (0.0, 0.12),
                };
                let t = now - lane.1 - sustain;
                if t <= 0.0 {
                    1.0
                } else {
                    (1.0 - t / fade).clamp(0.0, 1.0) as f32
                }
            };
            out.push((light, lane.2));
        }
        out
    }

    /// Draws `controls` (logical pixels), each lane's glyph of `glyphs`
    /// from the glyphs' atlas `texture` ([`glyph_texture`]), for the lanes
    /// held now (`held`), `ratio` target pixels a logical one, at host
    /// second `now`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        list: &mut DrawList,
        controls: &Controls,
        glyphs: &[Glyph],
        texture: TextureId,
        held: &[bool],
        now: f64,
        ratio: f32,
        reduced: bool,
        look: Look,
    ) {
        let lights = self.lights(held, now, reduced, look);
        let light = |t: &Target| t.lane.and_then(|l| lights.get(l)).copied().unwrap_or((0.0, 0));
        // The mat's spill first, under every tile.
        if look == Look::Mat {
            for t in &controls.targets {
                let (k, strike) = light(t);
                if k > 0.0 {
                    let hue = hue(t, strike);
                    spill(list, scaled(t.rect, ratio), hue, k);
                }
            }
        }
        for t in &controls.targets {
            let rect = scaled(t.rect, ratio);
            let (k, strike) = light(t);
            let side = (rect[2] - rect[0]).min(rect[3] - rect[1]);
            let radius = (side * 0.16).min(18.0 * ratio);
            let glyph = t.lane.and_then(|l| glyphs.get(l));
            match look {
                Look::Glass => {
                    plate(list, rect, radius, [1.0, 1.0, 1.0, 0.07 + 0.2 * k]);
                    outline(list, rect, radius, 1.5 * ratio, [1.0, 1.0, 1.0, 0.22 + 0.5 * k]);
                }
                Look::Mat => {
                    let mut tile = MAT;
                    if t.lane.is_none() {
                        tile[3] *= 0.7;
                    }
                    plate(list, rect, radius, tile);
                    if k > 0.0 {
                        let [r, g, b] = hue(t, strike);
                        plate(list, rect, radius, [r, g, b, 0.4 * k]);
                    }
                    outline(list, rect, radius, 1.5 * ratio, MAT_EDGE);
                }
                Look::Glyphs => {
                    if k > 0.0 {
                        plate(list, rect, radius, [0.16, 0.9, 0.68, 0.28 * k]);
                    }
                    outline(list, rect, radius, 1.0 * ratio, [1.0, 1.0, 1.0, 0.14]);
                }
            }
            if let Some(glyph) = glyph {
                let alpha = match look {
                    Look::Glass => 0.5 + 0.5 * k,
                    Look::Mat => 0.62 + 0.38 * k,
                    Look::Glyphs => 0.45 + 0.55 * k,
                };
                draw_glyph(
                    list,
                    glyph,
                    texture,
                    Weight::Regular,
                    [(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0],
                    side * atlas::CELL_SHARE,
                    [1.0, 1.0, 1.0, alpha],
                );
            }
        }
    }
}

/// A target's light's hue: as the hand's pad lights a tile, a hue of its
/// own a press (its tile and the press counted).
fn hue(t: &Target, strike: u32) -> [f32; 3] {
    let tile = match t.kind {
        Kind::Tile { pad, cell } => usize::from(pad) * 9 + usize::from(cell) - 1,
        Kind::Button { group } => usize::from(group) * 9 + t.lane.unwrap_or(0),
    };
    ddi_character::appearance::tile_color(1, tile, strike)
}

fn scaled(r: [f32; 4], ratio: f32) -> [f32; 4] {
    r.map(|v| v * ratio)
}

fn plate(list: &mut DrawList, rect: [f32; 4], radius: f32, color: [f32; 4]) {
    let mut q = Quad::plain(rect, color);
    q.shape = Shape {
        radius: [radius; 2],
        ..Shape::NONE
    };
    list.quad(q);
}

fn outline(list: &mut DrawList, rect: [f32; 4], radius: f32, thick: f32, color: [f32; 4]) {
    let mut q = Quad::plain(rect, color);
    q.shape = Shape {
        radius: [radius; 2],
        outline: [thick; 2],
        ..Shape::NONE
    };
    list.quad(q);
}

/// The light's spill round a lit tile, onto its neighbors (soft).
fn spill(list: &mut DrawList, rect: [f32; 4], [r, g, b]: [f32; 3], k: f32) {
    let side = (rect[2] - rect[0]).min(rect[3] - rect[1]);
    let by = side * 0.35;
    let grown = [rect[0] - by, rect[1] - by, rect[2] + by, rect[3] + by];
    let mut q = Quad::plain(grown, [r, g, b, 0.2 * k]);
    q.shape = Shape {
        radius: [by * 1.6; 2],
        soft: [by; 2],
        ..Shape::NONE
    };
    list.quad(q);
}

/// A lane's glyph, its cell of the glyphs' atlas (`texture`,
/// `ddi_ui::touch_controls::glyphs`) drawn `side` across at `center`,
/// turned as its receptor: one quad, one outline, so a translucent glyph
/// has no darker joints where strokes would cross.
pub fn draw_glyph(
    list: &mut DrawList,
    glyph: &Glyph,
    texture: TextureId,
    weight: Weight,
    center: [f32; 2],
    side: f32,
    color: [f32; 4],
) {
    let (uv, turn) = atlas::cell(glyph, weight);
    let h = side / 2.0;
    let mut q = Quad::plain([-h, -h, h, h], color);
    q.texture = Some(texture);
    q.uv = uv;
    q.transform = Affine::rotate(turn.to_radians()).then(Affine::translate(center[0], center[1]));
    list.quad(q);
}

/// The glyphs' atlas in `painter` (put in, made once a process, where it
/// is not loaded): its texture, for [`TouchView::draw`].
pub fn glyph_texture(painter: &mut Painter, device: &wgpu::Device, queue: &wgpu::Queue) -> TextureId {
    let id = painter.texture(atlas_key());
    if !painter.texture_ready(id) {
        painter.insert_rgba(device, queue, id, atlas::SIZE, atlas::atlas());
    }
    id
}

fn atlas_key() -> TextureKey {
    TextureKey {
        source: atlas::SOURCE.into(),
        max_side: 0,
        blurred: false,
    }
}

/// A painter's texture `requests` with the glyphs' atlas answered (put
/// in `painter`, the prompt's diagram draws it): the rest, for the
/// shell to load. Both shells' menus' painters go through it.
pub fn serve(
    painter: &mut Painter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    requests: Vec<(TextureId, TextureKey)>,
) -> Vec<(TextureId, TextureKey)> {
    requests
        .into_iter()
        .filter(|(id, key)| {
            if &*key.source != atlas::SOURCE || key.blurred {
                return true;
            }
            painter.insert_rgba(device, queue, *id, atlas::SIZE, atlas::atlas());
            false
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::layout::Layout;
    use ddi_render::draw2d::Command;
    use ddi_ui::touch_controls::{Choice, Screen, Style, plan};

    #[test]
    fn a_press_lights_its_lane_and_fades_after_the_release() {
        let mut v = TouchView::default();
        let held = [false, true, false, false];
        assert_eq!(v.lights(&held, 0.0, false, Look::Mat)[1].0, 1.0);
        let up = [false; 4];
        // Released at 1.0: lit through the sustain, then fading, then out.
        assert_eq!(v.lights(&up, 1.0, false, Look::Mat)[1].0, 1.0);
        assert_eq!(v.lights(&up, 1.0 + SUSTAIN, false, Look::Mat)[1].0, 1.0);
        let k = v.lights(&up, 1.0 + SUSTAIN + FADE / 2.0, false, Look::Mat)[1].0;
        assert!((k - 0.5).abs() < 1e-3, "{k}");
        assert_eq!(v.lights(&up, 2.0, false, Look::Mat)[1].0, 0.0);
        // Each press a hue of its own.
        let first = v.lights(&held, 3.0, false, Look::Mat)[1].1;
        v.lights(&up, 3.1, false, Look::Mat);
        assert_eq!(v.lights(&held, 3.2, false, Look::Mat)[1].1, first + 1);
    }

    #[test]
    fn reduced_motion_lights_only_while_held() {
        let mut v = TouchView::default();
        v.lights(&[true, false, false, false], 0.0, true, Look::Mat);
        assert_eq!(v.lights(&[false; 4], 0.01, true, Look::Mat)[0].0, 0.0);
    }

    #[test]
    fn every_look_draws_each_target_with_its_glyph_as_one_quad() {
        const ATLAS: TextureId = TextureId(7);
        for id in ["dance-single", "danoni-5", "pump-single", "dance-solo", "danoni-9A"] {
            let l = Layout::builtin(id).unwrap();
            let glyphs: Vec<Glyph> = l.lanes.iter().map(|l| l.glyph.clone()).collect();
            for style in [Style::Buttons, Style::Pad] {
                let choice = Choice { style, ..Choice::default() };
                let c = plan(&l, Screen::new([390.0, 844.0], [0.0; 4]), choice).unwrap();
                for look in [Look::Glass, Look::Mat, Look::Glyphs] {
                    let mut list = DrawList::new();
                    let mut v = TouchView::default();
                    let held: Vec<bool> = (0..glyphs.len()).map(|i| i == 0).collect();
                    v.draw(&mut list, &c, &glyphs, ATLAS, &held, 0.0, 3.0, false, look);
                    // Each lane's target: exactly one quad of the atlas,
                    // centered on it, nothing else of its glyph.
                    let quads: Vec<&Quad> = list
                        .commands
                        .iter()
                        .filter_map(|c| match c {
                            Command::Quad(q) if q.texture == Some(ATLAS) => Some(q),
                            _ => None,
                        })
                        .collect();
                    let lanes: Vec<&Target> = c.targets.iter().filter(|t| t.lane.is_some()).collect();
                    assert_eq!(quads.len(), lanes.len(), "{id} {style:?} {look:?}");
                    for (q, t) in quads.iter().zip(&lanes) {
                        let [x, y] = t.center();
                        let at = q.transform.apply([0.0, 0.0]);
                        assert!((at[0] - x * 3.0).abs() < 0.01 && (at[1] - y * 3.0).abs() < 0.01);
                        let (uv, _) = atlas::cell(&glyphs[t.lane.unwrap()], Weight::Regular);
                        assert_eq!(q.uv, uv);
                    }
                    // Nothing but the atlas is a glyph now: every other
                    // quad is a plate, an outline or the mat's spill.
                    assert!(list.commands.iter().all(|c| match c {
                        Command::Quad(q) => q.texture.is_none() || q.texture == Some(ATLAS),
                        _ => true,
                    }));
                    assert_eq!(Look::from_name(look.name()), Some(look));
                }
            }
        }
    }

    #[test]
    fn a_glyph_is_one_quad_turned_as_its_receptor() {
        for glyph in [
            Glyph::Arrow { rotation_deg: 0.0 },
            Glyph::Arrow { rotation_deg: 45.0 },
            Glyph::Center,
            Glyph::Onigiri,
            Glyph::Giko,
        ] {
            let mut list = DrawList::new();
            draw_glyph(&mut list, &glyph, TextureId(1), Weight::Regular, [50.0, 60.0], 20.0, [1.0; 4]);
            assert_eq!(list.commands.len(), 1, "{glyph:?}");
            let Command::Quad(q) = &list.commands[0] else { panic!() };
            assert_eq!(q.rect, [-10.0, -10.0, 10.0, 10.0]);
            // The cell's top (the arrow's tip) goes where its receptor points.
            let tip = q.transform.apply([0.0, -10.0]);
            if let Glyph::Arrow { rotation_deg } = glyph {
                let a = rotation_deg.to_radians();
                let want = [50.0 - 10.0 * a.cos(), 60.0 - 10.0 * a.sin()];
                assert!((tip[0] - want[0]).abs() < 1e-3 && (tip[1] - want[1]).abs() < 1e-3, "{glyph:?} {tip:?}");
            } else {
                assert!((tip[0] - 50.0).abs() < 1e-4 && (tip[1] - 50.0).abs() < 1e-4);
            }
        }
    }
}
