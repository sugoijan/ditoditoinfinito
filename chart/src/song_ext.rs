//! Convenience queries on [`Song`].

use crate::layout::Layout;
use crate::model::{Chart, Difficulty, Song};

impl Song {
    /// First chart with the given layout id (e.g. `dance-single`) and difficulty.
    pub fn chart_by(&self, layout: &str, difficulty: Difficulty) -> Option<&Chart> {
        self.charts
            .iter()
            .find(|c| c.layout == layout && c.difficulty == difficulty)
    }

    /// Charts whose layout has a built-in [`Layout`] (the ones the game can play).
    pub fn playable_charts(&self) -> impl Iterator<Item = &Chart> {
        self.charts
            .iter()
            .filter(|c| Layout::builtin(&c.layout).is_some())
    }
}
