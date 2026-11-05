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
