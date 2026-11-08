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

use crate::editor::analyze_buffer;
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
/// heard), [`analyze_buffer`] [`BUDGET_MS`] each animation frame; `None`
/// once `stale` says the page went.
pub(crate) async fn analyze(
    buffer: web_sys::AudioBuffer,
    stale: impl Fn() -> bool,
) -> Option<Series> {
    let music = analyze_buffer::<Analyzer>(&buffer, 1 << 14, 32, BUDGET_MS, stale).await?;
    Some(Series::new(&music))
}
