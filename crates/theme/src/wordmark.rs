//! Dosis artwork, shaped offline; the atlas contains three protected outline
//! masks and the white lettering. Only their paint changes at runtime.
use ddi_actor::{Actor, Kind, Rgba, Sprite, State, Vivid};
use ddi_ui::look::{call, named};

mod geometry {
    include!("wordmark_geometry.rs");
}

/// The artwork's width over its height.
pub const ASPECT: f32 = geometry::ASPECT;

/// The lettering in `text`, the theme's text color.
pub fn actor([w, h]: [f32; 2], text: Rgba) -> Actor {
    let width = w.min(h * geometry::ASPECT);
    let size = [width, width / geometry::ASPECT];
    let parts = (0..4)
        .map(|i| {
            let mut sprite = Sprite::new(
                0,
                vec![State {
                    rect: [0.0, i as f32 / 4.0, 1.0, (i + 1) as f32 / 4.0],
                    delay: 1.0,
                }],
                size,
            );
            sprite.shape.hide_unloaded = true;
            if i < 3 {
                sprite.shape.vivid = Some(Vivid {
                    evolution_speed: 1.0,
                    screen_space: false,
                    time: 0.0,
                    phase_offset: -(i as f32) * 0.25,
                    rect: geometry::FIELDS[i],
                });
            }
            let mut a = named(Kind::Sprite(sprite), &format!("TitleLayer{i}"));
            if i == 3 {
                call(&mut a, "diffuse", &text);
            }
            a
        })
        .collect();
    named(Kind::Frame(parts), "Title")
}
