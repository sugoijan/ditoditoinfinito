//! A Dancing☆Onigiri work's back and mask layers in a play
//! (`docs/plans/overlays.md`): the play's timeline (the shell keeps the
//! decoded contents and their textures, and draws what this says shows).

use std::collections::HashSet;

use ddi_chart::{Layout, Song};
use ddi_engine::lyrics::LyricTrack;
use ddi_engine::sprites::{SpriteDraw, SpriteSource, SpriteTrack};
use ddi_ui::state::settings::Settings;

/// Longest side of a back or mask image kept (they show at the stage's
/// size, about the screen's height at most).
pub const MAX_LAYER_SIDE: u32 = 2048;

/// Texts of a work's layers drawn, at most.
pub const MAX_LAYER_TEXTS: usize = 64;

/// How much larger than its stage size layer text is drawn, to stay
/// sharp on large screens.
pub const TEXT_SCALE: f32 = 3.0;

/// A play's layers and lyrics: chart `chart` of `song` (as played: its
/// song offset moves them too) on `layout`, as `settings` show them
/// (the layers with the backgrounds; the lyrics over the field, on one
/// row of lanes or two). Lyrics with nothing to show are none.
pub fn play_layers(
    song: &Song,
    chart: usize,
    layout: &Layout,
    settings: &Settings,
) -> (Option<SpriteTrack>, Option<LyricTrack>) {
    let track =
        (settings.bg_brightness > 0.0).then(|| SpriteTrack::new(song, chart, settings.reverse));
    let lyrics = settings
        .lyrics
        .then(|| {
            let single_row = layout.lanes.iter().all(|l| l.row == 0);
            LyricTrack::new(song, chart, settings.reverse, single_row)
        })
        .filter(|t| !t.is_empty());
    (track, lyrics)
}

/// What chart `chart` of `song` shows on its layers, under both scroll
/// directions: the images, and the texts within [`MAX_LAYER_TEXTS`].
pub fn layer_sources(song: &Song, chart: usize) -> Vec<SpriteSource> {
    let mut sources = SpriteTrack::new(song, chart, false).sources();
    let mut seen: HashSet<SpriteSource> = sources.iter().cloned().collect();
    for s in SpriteTrack::new(song, chart, true).sources() {
        if seen.insert(s.clone()) {
            sources.push(s);
        }
    }
    let mut texts = 0;
    sources.retain(|s| match s {
        SpriteSource::Image(_) => true,
        SpriteSource::Text { .. } => {
            texts += 1;
            texts <= MAX_LAYER_TEXTS
        }
    });
    sources
}

/// The layers of the play on, if it shows any.
#[derive(Default)]
pub struct StageLayers {
    track: Option<SpriteTrack>,
}

impl StageLayers {
    /// The layers of the play about to start (`None`, or none in it: none
    /// shown).
    pub fn set_track(&mut self, track: Option<SpriteTrack>) {
        self.track = track.filter(|t| !t.is_empty());
    }

    /// Whether a play's layers are shown.
    pub fn shown(&self) -> bool {
        self.track.is_some()
    }

    /// The track, while one shows.
    pub fn track(&self) -> Option<&SpriteTrack> {
        self.track.as_ref()
    }

    /// What shows at song second `t`, and the stage's size (danoniplus's
    /// px), while a track shows.
    pub fn at(&self, t: f64) -> Option<(Vec<SpriteDraw>, (f32, f32))> {
        self.track
            .as_ref()
            .map(|track| (track.at(t), track.stage()))
    }
}
