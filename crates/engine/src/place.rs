//! Where the field sits across the screen (drawing only; judging never
//! sees it). `ddi_render::scene::FieldGeometry::placed` turns it into
//! pixels.

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FieldPlace {
    /// In the middle of the screen.
    #[default]
    Center,
    /// Centered on the left half, as StepMania places player 1's field.
    Left,
    /// Centered on the right half, player 2's.
    Right,
    /// Halfway between center and the left-half placement.
    NearLeft,
    /// Halfway between center and the right-half placement.
    NearRight,
}

impl FieldPlace {
    /// Where the field's middle is wanted, as a share of the screen's
    /// width (an edge's margin may hold it back).
    pub fn center(self) -> f32 {
        match self {
            FieldPlace::Center => 0.5,
            FieldPlace::Left => 0.25,
            FieldPlace::NearLeft => 0.375,
            FieldPlace::NearRight => 0.625,
            FieldPlace::Right => 0.75,
        }
    }
}
