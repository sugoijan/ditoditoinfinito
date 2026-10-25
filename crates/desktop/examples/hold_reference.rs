//! Compare Infinito long notes through the actual loader and renderer.
//! No window/audio. `cargo run -p ddi-desktop --no-default-features --example
//! hold_reference -- OUTPUT.png` (scratch DDI_HOME, GPU required).
#[path = "common/reference.rs"]
mod reference;

use ddi_chart::Quantization;
use ddi_engine::frame::{NoteSprite, SpriteKind};
use ddi_noteskin::lua::Env;
use ddi_render::noteskin::NoteLayer;
use reference::{env, flag};

fn main() -> reference::Result<()> {
    let output = std::env::args().nth(1).ok_or("OUTPUT.png required")?;
    let mut gpu = reference::Gpu::new()?;
    let layout = reference::ten_lanes();
    let mut ready = reference::infinito(&layout, Env::default())?;
    gpu.upload(&mut ready);
    eprintln!("{}", ready.report.summary());
    for warning in &ready.play.notes {
        eprintln!("{warning}");
    }
    let size = [1280, 768];
    let mut quads = Vec::new();
    let mut frame = reference::frame()?;
    let tail_y = env("DDI_HOLD_LENGTH", 173.0_f32) / 64.0;
    let (roll, active) = (flag("DDI_REFERENCE_ROLL"), flag("DDI_REFERENCE_ACTIVE"));
    frame.notes = (0..10)
        .map(|lane| NoteSprite {
            note_index: lane,
            lane: lane as u8,
            y: 0.0,
            kind: if roll {
                SpriteKind::RollHead {
                    tail_y,
                    active,
                    dropped: false,
                }
            } else {
                SpriteKind::HoldHead {
                    tail_y,
                    active,
                    dropped: false,
                }
            },
            quantization: Quantization::N4,
            beat_frac: 0.0,
            color: None,
            alpha: 1.0,
            glow: 0.0,
            beat: 0.0,
            hold_life: 1.0,
        })
        .collect();
    ready.play.update(&frame, std::iter::empty(), false);
    let geo = reference::geometry(size);
    ready.play.holds(&frame, &layout, &geo, &mut quads);
    eprintln!("{} hold quads", quads.len());
    let params = ready.play.materials().to_vec();
    gpu.materials(&ready, &mut quads, &params, &frame);
    gpu.capture(&quads, size, reference::GROUND, Some(true), &output)?;
    eprintln!("captured Infinito hold masks and shared material");
    Ok(())
}
