//! Additional, explicitly assisted charts. The source charts, their indexes
//! and their digests stay unchanged; generated versions are appended.

use serde::{Deserialize, Serialize};

use crate::{Chart, NoteKind, Song};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Derivation {
    WithoutOnigiri { source: usize },
}

impl Derivation {
    pub fn source(&self) -> usize {
        match self {
            Self::WithoutOnigiri { source } => *source,
        }
    }
}

/// The matching layout and `old_lane -> new_lane` mapping. 7 keys keeps
/// its six arrow positions on Solo (whose left diagonal points upward).
/// 9B's diagonals are reordered to StepMania's techno-single8 track order.
pub fn without_onigiri(layout: &str) -> Option<(&'static str, &'static [Option<u8>])> {
    Some(match layout {
        "danoni-5" => ("dance-single", &[Some(0), Some(1), Some(2), Some(3), None]),
        "danoni-7" => (
            "dance-solo",
            &[Some(0), Some(1), Some(2), None, Some(3), Some(4), Some(5)],
        ),
        "danoni-9A" => (
            "dance-double",
            &[
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                None,
                Some(4),
                Some(5),
                Some(6),
                Some(7),
            ],
        ),
        "danoni-9B" => (
            "techno-single8",
            &[
                Some(2),
                Some(1),
                Some(0),
                Some(3),
                None,
                Some(4),
                Some(5),
                Some(6),
                Some(7),
            ],
        ),
        _ => return None,
    })
}

fn conversion(chart: &Chart) -> Option<(&'static str, &'static [Option<u8>])> {
    if chart.derivation.is_some() {
        return None;
    }
    let (layout, mapping) = without_onigiri(&chart.layout)?;
    // An invalid lane must not be silently lost by the conversion.
    if chart
        .notes
        .iter()
        .any(|n| usize::from(n.lane) >= mapping.len())
    {
        return None;
    }
    // Share eligibility with the metadata freshness check without cloning notes.
    let has_judged = chart.notes.iter().any(|n| {
        mapping[usize::from(n.lane)].is_some()
            && matches!(
                n.kind,
                NoteKind::Tap
                    | NoteKind::HoldHead { .. }
                    | NoteKind::RollHead { .. }
                    | NoteKind::Lift
            )
    });
    has_judged.then_some((layout, mapping))
}

fn convert(chart: &Chart, source: usize) -> Option<Chart> {
    let (layout, mapping) = conversion(chart)?;
    let mut converted = chart.clone();
    converted.layout = layout.into();
    converted.notes = chart
        .notes
        .iter()
        .filter_map(|note| {
            let mut note = note.clone();
            note.lane = mapping[usize::from(note.lane)]?;
            Some(note)
        })
        .collect();
    converted.notes.sort_by_key(|n| (n.tick, n.lane));
    converted.derivation = Some(Derivation::WithoutOnigiri { source });
    converted.name = if chart.name.is_empty() {
        "Without onigiri".into()
    } else {
        format!("{} · without onigiri", chart.name)
    };
    Some(converted)
}

impl Song {
    /// Whether every eligible original already has its generated version.
    /// Empty or invalid conversions need no version. This only reads notes;
    /// it never allocates another chart or changes their order or identity.
    pub fn chart_variants_current(&self) -> bool {
        self.charts.iter().enumerate().all(|(source, chart)| {
            self.charts
                .iter()
                .any(|c| c.derivation.as_ref().is_some_and(|d| d.source() == source))
                || conversion(chart).is_none()
        })
    }

    /// Adds every available onigiri-free version once, independently of the
    /// display setting. This keeps chart URLs, stored plays and replays stable.
    pub fn add_no_onigiri_charts(&mut self) {
        let generated: Vec<usize> = self
            .charts
            .iter()
            .filter_map(|c| c.derivation.as_ref().map(Derivation::source))
            .collect();
        let converted: Vec<_> = self
            .charts
            .iter()
            .enumerate()
            .filter(|(i, _)| !generated.contains(i))
            .filter_map(|(i, chart)| convert(chart, i))
            .collect();
        self.charts.extend(converted);
    }

    /// Chart-scoped lyrics and layers belong to the original chart.
    pub fn effects_chart(&self, index: usize) -> usize {
        self.charts
            .get(index)
            .and_then(|c| c.derivation.as_ref())
            .map_or(index, Derivation::source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::danoni::{self, Dos, DosFlags};
    use crate::{Glyph, NoteKind};

    fn make_song(mode: &str, notes: &str) -> Song {
        let text = format!(
            "|musicTitle=Test,Artist|musicUrl=test.ogg|blankFrame=0|difData={mode},Hard,3.5|{notes}"
        );
        danoni::import(&Dos::parse(&text, DosFlags::default()))
            .unwrap()
            .songs
            .remove(0)
    }

    #[test]
    fn every_supported_mode_gets_its_own_version_without_changing_the_original() {
        for (mode, layout, lanes) in [
            ("5", "dance-single", 4),
            ("7", "dance-solo", 6),
            ("9A", "dance-double", 8),
            ("9B", "techno-single8", 8),
        ] {
            let mut song = make_song(
                mode,
                "|left_data=60|right_data=120|space_data=90|frzSpace_data=150,210|frzLeft_data=240,300|",
            );
            let original = song.charts[0].clone();
            let digest = song.chart_digest(0);
            assert!(!song.chart_variants_current());
            song.add_no_onigiri_charts();
            assert!(song.chart_variants_current());
            assert_eq!(song.charts.len(), 2, "{mode}");
            assert_eq!(song.charts[0], original);
            assert_eq!(song.chart_digest(0), digest);
            let converted = &song.charts[1];
            assert_eq!(converted.layout, layout);
            assert_eq!(converted.notes.len(), 3);
            assert!(converted.notes.iter().all(|n| usize::from(n.lane) < lanes));
            assert!(
                converted
                    .notes
                    .iter()
                    .any(|n| matches!(n.kind, NoteKind::HoldHead { .. }))
            );
            let layout = song.layout_of(converted).unwrap();
            assert_eq!(layout.lanes.len(), lanes);
            assert!(
                layout
                    .lanes
                    .iter()
                    .all(|lane| !matches!(lane.glyph, Glyph::Onigiri))
            );
            assert_ne!(song.chart_digest(1), digest);
            assert_eq!(song.effects_chart(1), 0);
            let first = song.clone();
            song.add_no_onigiri_charts();
            assert_eq!(song, first, "generation is idempotent");
        }
    }

    #[test]
    fn no_empty_or_unrelated_versions_are_generated() {
        let mut song = make_song("5", "|space_data=90|");
        assert!(song.chart_variants_current());
        song.add_no_onigiri_charts();
        assert_eq!(song.charts.len(), 1);
        let mut song = make_song("5", "|left_data=60|space_data=90|");
        song.charts[0].notes[0].lane = 9;
        assert!(
            song.chart_variants_current(),
            "invalid lanes cannot be converted"
        );
        song.add_no_onigiri_charts();
        assert_eq!(song.charts.len(), 1);
        song.charts[0].layout = "pump-single".into();
        assert!(song.chart_variants_current());
        song.add_no_onigiri_charts();
        assert_eq!(song.charts.len(), 1);
    }

    #[test]
    fn eight_direction_notes_keep_their_original_directions() {
        let mut song = make_song("9B", "|left_data=60|up_data=90|sright_data=120|");
        song.add_no_onigiri_charts();
        let source = song.layout_of(&song.charts[0]).unwrap();
        let target = song.layout_of(&song.charts[1]).unwrap();
        for (old, new) in song.charts[0].notes.iter().zip(&song.charts[1].notes) {
            assert_eq!(
                source.lanes[old.lane as usize].glyph,
                target.lanes[new.lane as usize].glyph
            );
            assert_eq!(old.tick, new.tick);
            assert_eq!(old.color, new.color);
            assert_eq!(old.speed_mul, new.speed_mul);
        }
    }
}
