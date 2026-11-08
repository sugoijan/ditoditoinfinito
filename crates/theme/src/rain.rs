//! Slow outline rain behind the title menu. Atlas frames keep glyphs upright
//! in their quads, so all of them can sample one screen-aligned Vivid field.
use ddi_actor::{Actor, Kind, Sprite, State, Vivid, rain};
use ddi_ui::look::{call, named};

use super::Infinito;

const COUNT: usize = 50;

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

struct Particle {
    pos: [f32; 2],
    side: f32,
    alpha: f32,
    frame: [f32; 4],
}

/// The shared particle ([`ddi_actor::rain`]) as the menu's sprite.
fn particle(i: usize, time: f64, size: [f32; 2], speed: f64, strength: f32) -> Particle {
    let p = rain::particle(i, time, speed);
    let depth = p.depth as f32;
    let column = if p.onigiri {
        4.0
    } else {
        f32::from(p.direction)
    };
    let row = p.row as f32;
    Particle {
        pos: [
            p.x as f32 / rain::WIDTH as f32 * size[0],
            p.y as f32 / rain::HEIGHT as f32 * size[1],
        ],
        // Keep glyph proportions on portrait and ultrawide viewports.
        side: 72.0 * (0.22 + depth * 0.36) * 64.0 / 56.0
            * (size[0] / 960.0).min(size[1] / 540.0).max(0.65),
        // In f32, as the menu has always weighed it.
        alpha: (0.13 + depth * 0.24) * p.fade as f32 * (1.0 - p.quiet as f32) * strength / 22.0,
        frame: [
            column / 5.0,
            row / rain::ROWS as f32,
            (column + 1.0) / 5.0,
            (row + 1.0) / rain::ROWS as f32,
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
            evolution_speed: 1.0,
            screen_space: false,
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
