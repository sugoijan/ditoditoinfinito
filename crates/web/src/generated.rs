//! A play's generated background's music, analyzed
//! (`ddi_library::music`, `docs/plans/phase-8-gaps.md`, step 5): the
//! decoded buffer's samples read a block at a time and analyzed a few
//! milliseconds each animation frame, as the chart editor's are
//! (`crate::editor`), from the prompt on. The game loop draws the look
//! on the beat alone until it is in. And the start prompt's still of it,
//! drawn for the 2D pass when it asks ([`serve_stills`]).

use std::cell::RefCell;

use ddi_library::generative::Series;
use ddi_library::music::Analyzer;
use ddi_render::{Painter, TextureId, TextureKey};
use ddi_shell::generative::{Still, Stills};

use crate::editor::{next_frame, now_ms};
use crate::web::gfx::Gfx;

thread_local! {
    /// The start prompt's generated still, and the device's generation
    /// it is drawn on.
    static STILLS: RefCell<(u64, Stills)> = RefCell::new((0, Stills::default()));
}

/// The still the prompt shows under `source` from now on (`None`: none).
pub(crate) fn set_still(source: Option<String>, still: Option<Still>) {
    STILLS.with(|s| s.borrow_mut().1.set(source, still));
}

/// The 2D pass's requests for the still, drawn ([`Stills::serve`]): the
/// rest are the platform's to read.
pub(crate) fn serve_stills(
    painter: &mut Painter,
    gfx: &Gfx,
    requests: Vec<(TextureId, TextureKey)>,
) -> Vec<(TextureId, TextureKey)> {
    STILLS.with(|s| {
        let mut s = s.borrow_mut();
        if s.0 != gfx.generation {
            s.0 = gfx.generation;
            s.1.device_changed();
        }
        s.1.serve(painter, &gfx.device, &gfx.queue, requests)
    })
}

/// Milliseconds a frame the analysis takes at most.
const BUDGET_MS: f64 = 3.0;

/// The signals of `buffer`'s music (its lead: where its first sample is
/// heard); `None` once `stale` says the page went.
pub(crate) async fn analyze(
    buffer: web_sys::AudioBuffer,
    stale: impl Fn() -> bool,
) -> Option<Series> {
    let frames = buffer.length() as usize;
    let mut a = Analyzer::new(
        f64::from(buffer.sample_rate()),
        frames,
        crate::web::audio::lead(&buffer),
    );
    let channels = (buffer.number_of_channels() as usize).max(1);
    let mut blocks = vec![vec![0.0f32; 1 << 14]; channels];
    loop {
        let until = now_ms() + BUDGET_MS;
        while now_ms() < until {
            let at = a.fed();
            if at < frames {
                let n = blocks[0].len().min(frames - at);
                for (c, block) in blocks.iter_mut().enumerate() {
                    if buffer
                        .copy_from_channel_with_start_in_channel(
                            &mut block[..n],
                            c as i32,
                            at as u32,
                        )
                        .is_err()
                    {
                        block[..n].fill(0.0);
                    }
                }
                let planar: Vec<&[f32]> = blocks.iter().map(|b| &b[..n]).collect();
                a.add_planar(&planar);
            }
            if a.step(32) {
                return Some(Series::new(&a.finish()));
            }
        }
        next_frame().await;
        if stale() {
            return None;
        }
    }
}
