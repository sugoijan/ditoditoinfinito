//! Dosis artwork, shaped offline; the atlas contains three protected outline
//! masks and the white lettering. Only their paint changes at runtime.
use ddi_actor::{Actor, Kind, Sprite, State, Vivid};
use ddi_ui::look::{call, named};

mod geometry {
    include!("wordmark_geometry.rs");
}

pub fn actor([w, h]: [f32; 2]) -> Actor {
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
                    time: 0.0,
                    phase_offset: -(i as f32) * 0.25,
                    rect: geometry::FIELDS[i],
                });
            }
            let mut a = named(Kind::Sprite(sprite), &format!("TitleLayer{i}"));
            if i == 3 {
                call(
                    &mut a,
                    "diffuse",
                    &[242.0 / 255.0, 238.0 / 255.0, 233.0 / 255.0, 1.0],
                );
            }
            a
        })
        .collect();
    named(Kind::Frame(parts), "Title")
}
