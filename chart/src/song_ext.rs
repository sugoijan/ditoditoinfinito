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

    /// Moves every note `seconds` later relative to the music (negative:
    /// earlier), in the song timing and in every split chart timing. This is
    /// StepMania's per-song offset edit: `#OFFSET` decreases by `seconds`.
    pub fn shift_notes(&mut self, seconds: f64) {
        self.timing.offset_seconds -= seconds;
        for chart in &mut self.charts {
            if let Some(t) = &mut chart.timing {
                t.offset_seconds -= seconds;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shift_notes_moves_beat_zero_later() {
        let mut song =
            crate::formats::sm::parse_simfile("#OFFSET:0.1;\n#BPMS:0=120;\n", "sm").unwrap();
        let before = song.timing.seconds_at(crate::Tick(0));
        song.shift_notes(0.025);
        let after = song.timing.seconds_at(crate::Tick(0));
        assert!((after - before - 0.025).abs() < 1e-12);
    }
}
