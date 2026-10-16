//! A Dancing☆Onigiri work's back and mask layers in a play
//! (`docs/plans/overlays.md`): the play's timeline (the shell keeps the
//! decoded contents and their textures, and draws what this says shows).

use ddi_engine::sprites::{SpriteDraw, SpriteTrack};

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
