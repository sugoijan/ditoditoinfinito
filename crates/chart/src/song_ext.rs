//! Convenience queries on [`Song`].

use std::borrow::Cow;

use crate::layout::Layout;
use crate::model::{Chart, Difficulty, Song, SourceFormat};

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

    /// The file a tag the model keeps as written (`source.unknown_tags`)
    /// names: its first parameter, the last such tag winning (a later tag
    /// overwrites an earlier one in StepMania's loaders); `None` when
    /// empty.
    fn file_tag(&self, name: &str) -> Option<&str> {
        self.source
            .unknown_tags
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.split(':').next())
            .filter(|v| !v.is_empty())
    }

    fn is_ssc(&self) -> bool {
        matches!(self.source.format, SourceFormat::Ssc { .. })
    }

    /// `#LYRICSPATH`: the song's `.lrc` lyrics, relative to its folder
    /// (`.sm` and `.ssc`; StepMania finds a lone `.lrc` without it,
    /// `Song::TidyUpData`, as the library does).
    pub fn lyrics_path(&self) -> Option<&str> {
        match self.source.format {
            SourceFormat::Sm | SourceFormat::Ssc { .. } => self.file_tag("LYRICSPATH"),
            _ => None,
        }
    }

    /// `.ssc` `#PREVIEW`: a sound file song select plays instead of the
    /// music's sample, from its start (`Song::GetPreviewMusicPath`,
    /// `GetPreviewStartSeconds`).
    pub fn preview_music(&self) -> Option<&str> {
        self.is_ssc().then(|| self.file_tag("PREVIEW")).flatten()
    }

    /// `.ssc` `#PREVIEWVID`: a movie a theme may show in song select
    /// (StepMania's own themes do not).
    pub fn preview_video(&self) -> Option<&str> {
        self.is_ssc().then(|| self.file_tag("PREVIEWVID")).flatten()
    }

    /// `.ssc` per-chart `#MUSIC`: the sound file chart `index` plays
    /// instead of the song's (`Steps::GetMusicPath`); a generated version
    /// of a chart plays its original's.
    pub fn chart_music(&self, index: usize) -> Option<&str> {
        if !self.is_ssc() {
            return None;
        }
        let index = self.effects_chart(index);
        self.file_tag(&format!("NOTEDATA[{index}].MUSIC"))
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

    /// The song with its notes moved `seconds` later ([`Song::shift_notes`]),
    /// borrowed as it is when that is 0.
    pub fn shifted(&self, seconds: f64) -> Cow<'_, Song> {
        if seconds == 0.0 {
            Cow::Borrowed(self)
        } else {
            let mut s = self.clone();
            s.shift_notes(seconds);
            Cow::Owned(s)
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
    fn extra_files_by_format() {
        let ssc = "#VERSION:0.83;#LYRICSPATH:a.lrc;#PREVIEW:p.ogg;#PREVIEWVID:p.mp4;\
                   #LYRICSPATH:b.lrc;#BPMS:0=120;\n\
                   #NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Easy;#METER:1;\
                   #MUSIC:e.ogg;#NOTES:\n1000\n;\n\
                   #NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:9;\
                   #NOTES:\n1000\n;\n";
        let song = crate::formats::sm::parse_simfile(ssc, "ssc").unwrap();
        // The last of a tag wins.
        assert_eq!(song.lyrics_path(), Some("b.lrc"));
        assert_eq!(song.preview_music(), Some("p.ogg"));
        assert_eq!(song.preview_video(), Some("p.mp4"));
        assert_eq!(song.chart_music(0), Some("e.ogg"));
        assert_eq!(song.chart_music(1), None);
        // `.sm` has lyrics, but no preview file.
        let sm = crate::formats::sm::parse_simfile(
            "#LYRICSPATH:l.lrc;#PREVIEW:p.ogg;#BPMS:0=120;",
            "sm",
        )
        .unwrap();
        assert_eq!(sm.lyrics_path(), Some("l.lrc"));
        assert_eq!(sm.preview_music(), None);
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
