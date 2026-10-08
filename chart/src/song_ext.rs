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

    /// The layout a chart is played on: one the song defines itself, else
    /// a built-in one. `None` for layouts the game does not know.
    pub fn layout_of(&self, chart: &Chart) -> Option<Layout> {
        self.layouts
            .iter()
            .find(|l| l.id == chart.layout)
            .cloned()
            .or_else(|| Layout::builtin(&chart.layout))
    }

    /// Charts whose layout is known (the ones the game can play).
    pub fn playable_charts(&self) -> impl Iterator<Item = &Chart> {
        self.charts.iter().filter(|c| self.layout_of(c).is_some())
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
    fn layouts_resolve_from_the_song_first() {
        let mut song = crate::formats::sm::parse_simfile(
            "#BPMS:0=120;\n#NOTES:dance-solo::Easy:1::0000000;\n#NOTES:custom-3::Easy:1::;\n",
            "sm",
        )
        .unwrap();
        assert_eq!(song.charts.len(), 2);
        assert_eq!(
            song.layout_of(&song.charts[0]).map(|l| l.id),
            Some("dance-solo".into())
        );
        assert!(song.layout_of(&song.charts[1]).is_none());
        let mut custom = crate::Layout::generic(3);
        custom.id = "custom-3".into();
        song.layouts.push(custom);
        assert_eq!(song.playable_charts().count(), 2);
    }

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
