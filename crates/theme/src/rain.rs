//! Slow outline rain behind the title menu. Atlas frames keep glyphs upright
//! in their quads, so all of them can sample one screen-aligned Vivid field.
use ddi_actor::{Actor, Kind, Sprite, State, Vivid};
use ddi_ui::look::{call, named};

use super::Infinito;

const COUNT: usize = 50;
const WEIGHTS: usize = 8;

pub(super) fn actor() -> Actor {
    let particles = (0..COUNT)
        .map(|_| {
            let mut s = Sprite::new(
                0,
                vec![State {
                    rect: [0.0, 0.0, 0.2, 0.125],
                    delay: 1.0,
                }],
                [64.0; 2],
            );
            s.shape.hide_unloaded = true;
            s.shape.saturation = 0.35;
            named(Kind::Sprite(s), "")
        })
        .collect();
    named(Kind::Frame(particles), "MenuBackground")
}

// The study's deterministic seed, evaluated in f64 so wasm and native agree.
fn seed(i: f64, salt: f64) -> f64 {
    ((i * 127.1 + salt * 311.7).sin() * 43758.5453).rem_euclid(1.0)
}

struct Particle {
    pos: [f32; 2],
    side: f32,
    alpha: f32,
    frame: [f32; 4],
}

fn particle(i: usize, time: f64, size: [f32; 2], speed: f64, strength: f32) -> Particle {
    let i = i as f64;
    let depth = seed(i, 2.0);
    let rate = speed * (0.8 + depth * 1.2);
    let travel = seed(i, 1.0) * 680.0 + (time + 8.0) * rate;
    let identity = i + (travel / 680.0).floor() * 4099.0;
    let y = -70.0 + travel.rem_euclid(680.0);
    let x = -50.0 + (seed(i, 3.0) * 1060.0 + (time + 8.0) * rate * 0.17).rem_euclid(1060.0);
    let fade = ((y + 70.0) / 80.0).min((610.0 - y) / 80.0).clamp(0.0, 1.0);
    // Change shape/direction only when the particle wraps outside the view.
    let column = if seed(identity, 6.0) < 0.01 {
        4.0
    } else {
        (seed(identity, 21.0) * 4.0).floor() as f32
    };
    let row = (depth * (WEIGHTS - 1) as f64).round() as f32;
    let distance = ((x - 480.0).hypot(y - 290.0) - 60.0).max(0.0) / 370.0;
    let quiet = if distance < 0.6 {
        0.58 + (0.23 - 0.58) * distance / 0.6
    } else {
        0.23 * (1.0 - distance).max(0.0) / 0.4
    };
    Particle {
        pos: [x as f32 / 960.0 * size[0], y as f32 / 540.0 * size[1]],
        // Keep glyph proportions on portrait and ultrawide viewports.
        side: 72.0 * (0.22 + depth as f32 * 0.36) * 64.0 / 56.0
            * (size[0] / 960.0).min(size[1] / 540.0).max(0.65),
        alpha: (0.13 + depth as f32 * 0.24) * fade as f32 * (1.0 - quiet as f32) * strength / 22.0,
        frame: [
            column / 5.0,
            row / 8.0,
            (column + 1.0) / 5.0,
            (row + 1.0) / 8.0,
        ],
    }
}

pub(super) fn update(root: &mut Actor, size: [f32; 2], seconds: f64, theme: &Infinito) {
    let [w, h] = size;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let speed = theme.m("TitleMenu.RainSpeed").max(0.0) as f64;
    let strength = theme.m("TitleMenu.RainStrength").clamp(0.0, 40.0);
    for (i, a) in root.children_mut().iter_mut().enumerate() {
        let p = particle(i, seconds, size, speed, strength);
        let Kind::Sprite(s) = &mut a.kind else {
            continue;
        };
        s.custom = Some(p.frame);
        s.size = [p.side; 2];
        let [x, y] = p.pos;
        // Same stationary snapshot as the approved icon texture: 96 note
        // units across the viewport, phase .363, seconds 9.04. Only sampling
        // coordinates move; neither the field nor its palette animates.
        s.shape.vivid = Some(Vivid {
            time: 5.04,
            phase_offset: 0.623,
            rect: [
                (x - p.side / 2.0) / w * 96.0 - 48.0,
                (y - p.side / 2.0) / h * 96.0 - 48.0,
                (x + p.side / 2.0) / w * 96.0 - 48.0,
                (y + p.side / 2.0) / h * 96.0 - 48.0,
            ],
        });
        call(a, "xy", &p.pos);
        call(a, "diffusealpha", &[p.alpha]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_and_rare_onigiri_change_only_outside_the_view() {
        let mut directions = [false; 4];
        let mut onigiri = 0;
        for i in 0..COUNT {
            let p = particle(i, 0.0, [960.0, 540.0], 8.0, 22.0);
            let column = (p.frame[0] * 5.0).round() as usize;
            if column == 4 {
                onigiri += 1;
            } else {
                directions[column] = true;
            }
            let next = particle(i, 0.1, [960.0, 540.0], 8.0, 22.0);
            if (p.pos[1] - next.pos[1]).abs() < 100.0 {
                assert_eq!(p.frame, next.frame);
            }
        }
        assert!(directions.into_iter().all(|d| d));
        assert_eq!(onigiri, 1);
    }

    #[test]
    fn field_is_shared_and_frozen_while_outlines_move() {
        let theme = Infinito::default();
        let mut a = actor();
        update(&mut a, [960.0, 540.0], 0.0, &theme);
        let first = a.children()[0].clone();
        update(&mut a, [960.0, 540.0], 1.0, &theme);
        let Kind::Sprite(before) = first.kind else {
            panic!()
        };
        let Kind::Sprite(after) = &a.children()[0].kind else {
            panic!()
        };
        assert_ne!(
            before.shape.vivid.unwrap().rect,
            after.shape.vivid.unwrap().rect
        );
        for a in a.children() {
            let Kind::Sprite(s) = &a.kind else { panic!() };
            let v = s.shape.vivid.unwrap();
            assert_eq!((v.time, v.phase_offset), (5.04, 0.623));
            assert_eq!(s.shape.saturation, 0.35);
        }
    }
}
