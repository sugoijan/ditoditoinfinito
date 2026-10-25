//! Additional, explicitly assisted charts. The source charts, their indexes
//! and their digests stay unchanged; generated versions are appended.

use serde::{Deserialize, Serialize};

use crate::{Chart, NoteKind, Song};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Derivation {
    /// A Dancing☆Onigiri chart without its onigiri lane.
    WithoutOnigiri { source: usize },
    /// A pump chart without its center panels, the rest of each pad turned
    /// an eighth clockwise onto a dance pad (`docs/plans/chart-simplification.md`).
    WithoutCenter { source: usize },
}

/// The kinds of generated version, in the order they are appended: every
/// onigiri-free version before every center-free one, so a song that
/// gained the latter keeps the indexes its former versions had.
const KINDS: [Kind; 2] = [Kind::WithoutOnigiri, Kind::WithoutCenter];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Kind {
    WithoutOnigiri,
    WithoutCenter,
}

impl Kind {
    fn mapping(self, layout: &str) -> Option<(&'static str, &'static [Option<u8>])> {
        match self {
            Kind::WithoutOnigiri => without_onigiri(layout),
            Kind::WithoutCenter => without_center(layout),
        }
    }

    fn derivation(self, source: usize) -> Derivation {
        match self {
            Kind::WithoutOnigiri => Derivation::WithoutOnigiri { source },
            Kind::WithoutCenter => Derivation::WithoutCenter { source },
        }
    }

    /// The version's name alone, and after its source's.
    fn names(self) -> (&'static str, &'static str) {
        match self {
            Kind::WithoutOnigiri => ("Without onigiri", "without onigiri"),
            Kind::WithoutCenter => ("Without center", "without center"),
        }
    }
}

impl Derivation {
    pub fn source(&self) -> usize {
        match self {
            Self::WithoutOnigiri { source } | Self::WithoutCenter { source } => *source,
        }
    }

    fn kind(&self) -> Kind {
        match self {
            Self::WithoutOnigiri { .. } => Kind::WithoutOnigiri,
            Self::WithoutCenter { .. } => Kind::WithoutCenter,
        }
    }

    /// Whether the version is played by its source's game, pump, though
    /// drawn and bound as dance: StepMania's pump game ticks holds, which
    /// the chart's author wrote them for (`docs/plans/pump.md`).
    pub fn pump_game(&self) -> bool {
        matches!(self, Self::WithoutCenter { .. })
    }

    /// What the version leaves out, as the song list marks it ("Without
    /// center").
    pub fn name(&self) -> &'static str {
        self.kind().names().0
    }
}

/// Whether some generated version may come from a chart of `layout`.
pub fn has_versions(layout: &str) -> bool {
    KINDS.iter().any(|k| k.mapping(layout).is_some())
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

/// The dance layout and `old_lane -> new_lane` mapping of a pump layout
/// without its centers: a pad's four corners turned an eighth clockwise,
/// down-left to Left, up-left to Up, up-right to Right, down-right to
/// Down (pump's DL, UL, C, UR, DR onto dance's L, D, U, R). Double turns
/// each pad on its own. Half-double's four corner panels, the left pad's
/// UR and DR and the right pad's DL and UL, stand around the gap between
/// the pads as one pad's corners stand around its center: they are turned
/// the same way by where they stand (upper left to Up, upper right to
/// Right, lower left to Left, lower right to Down) onto Single.
pub fn without_center(layout: &str) -> Option<(&'static str, &'static [Option<u8>])> {
    const L: Option<u8> = Some(0);
    const D: Option<u8> = Some(1);
    const U: Option<u8> = Some(2);
    const R: Option<u8> = Some(3);
    Some(match layout {
        "pump-single" => ("dance-single", &[L, U, None, R, D]),
        "pump-halfdouble" => ("dance-single", &[None, U, L, D, R, None]),
        "pump-double" => (
            "dance-double",
            &[L, U, None, R, D, Some(4), Some(6), None, Some(7), Some(5)],
        ),
        _ => return None,
    })
}

fn conversion(chart: &Chart, kind: Kind) -> Option<(&'static str, &'static [Option<u8>])> {
    if chart.derivation.is_some() {
        return None;
    }
    let (layout, mapping) = kind.mapping(&chart.layout)?;
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

fn convert(chart: &Chart, source: usize, kind: Kind) -> Option<Chart> {
    let (layout, mapping) = conversion(chart, kind)?;
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
    converted.derivation = Some(kind.derivation(source));
    let (alone, after) = kind.names();
    converted.name = if chart.name.is_empty() {
        alone.into()
    } else {
        format!("{} · {after}", chart.name)
    };
    Some(converted)
}

impl Chart {
    /// Whether the chart is played by StepMania's pump game's rules (its
    /// ticked holds under "by the chart's game"): a pump layout's chart,
    /// or a version of one without its centers.
    pub fn pump_game(&self) -> bool {
        self.layout.starts_with("pump-")
            || self.derivation.as_ref().is_some_and(Derivation::pump_game)
    }
}

impl Song {
    fn has_version(&self, source: usize, kind: Kind) -> bool {
        self.charts.iter().any(|c| {
            c.derivation
                .as_ref()
                .is_some_and(|d| d.source() == source && d.kind() == kind)
        })
    }

    /// Whether every eligible original already has its generated versions.
    /// Empty or invalid conversions need no version. This only reads notes;
    /// it never allocates another chart or changes their order or identity.
    pub fn chart_variants_current(&self) -> bool {
        self.charts.iter().enumerate().all(|(source, chart)| {
            KINDS
                .iter()
                .all(|&kind| self.has_version(source, kind) || conversion(chart, kind).is_none())
        })
    }

    /// Adds every available generated version once (without onigiri,
    /// without center), independently of the display settings. This keeps
    /// chart URLs, stored plays and replays stable.
    pub fn add_chart_versions(&mut self) {
        for kind in KINDS {
            let converted: Vec<_> = self
                .charts
                .iter()
                .enumerate()
                .filter(|&(i, _)| !self.has_version(i, kind))
                .filter_map(|(i, chart)| convert(chart, i, kind))
                .collect();
            self.charts.extend(converted);
        }
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
            song.add_chart_versions();
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
            song.add_chart_versions();
            assert_eq!(song, first, "generation is idempotent");
        }
    }

    #[test]
    fn no_empty_or_unrelated_versions_are_generated() {
        let mut song = make_song("5", "|space_data=90|");
        assert!(song.chart_variants_current());
        song.add_chart_versions();
        assert_eq!(song.charts.len(), 1);
        let mut song = make_song("5", "|left_data=60|space_data=90|");
        song.charts[0].notes[0].lane = 9;
        assert!(
            song.chart_variants_current(),
            "invalid lanes cannot be converted"
        );
        song.add_chart_versions();
        assert_eq!(song.charts.len(), 1);
        song.charts[0].layout = "pump-single".into();
        assert!(song.chart_variants_current());
        song.add_chart_versions();
        assert_eq!(song.charts.len(), 1);
    }

    #[test]
    fn eight_direction_notes_keep_their_original_directions() {
        let mut song = make_song("9B", "|left_data=60|up_data=90|sright_data=120|");
        song.add_chart_versions();
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

    /// A pump song: a chart of `layout` with `rows` (one measure of four
    /// rows each, as `.sm` writes them), and a dance chart before it.
    fn pump_song(layout: &str, rows: &[&str]) -> Song {
        let notes = rows
            .chunks(4)
            .map(|m| m.join("\n"))
            .collect::<Vec<_>>()
            .join("\n,\n");
        let text = format!(
            "#TITLE:Pump;#OFFSET:0;#BPMS:0=120;#TICKCOUNTS:0=2;\n\
             #NOTES:\n dance-single:\n:\n Easy:\n 2:\n 0,0,0,0,0:\n1000\n0000\n0000\n0000\n;\n\
             #NOTES:\n {layout}:\n :\n Hard:\n 9:\n 0,0,0,0,0:\n{notes}\n;\n"
        );
        crate::formats::sm::parse_simfile(&text, "sm").unwrap()
    }

    fn lanes(chart: &Chart) -> Vec<(i64, u8, &NoteKind)> {
        chart
            .notes
            .iter()
            .map(|n| (n.tick.0, n.lane, &n.kind))
            .collect()
    }

    #[test]
    fn pump_single_turns_its_corners_onto_single_and_drops_the_center() {
        // DL, UL, C, UR, DR one by one, then a hold, a fake and a mine on
        // the center, a lift and a hold on UR.
        let mut song = pump_song(
            "pump-single",
            &[
                "10000", "01000", "00100", "00010", //
                "00001", "00200", "00000", "00300", //
                "000L0", "00F00", "00M20", "00000", //
                "00030", "00000", "00000", "00000",
            ],
        );
        let original = song.clone();
        let digests: Vec<_> = (0..2).map(|i| song.chart_digest(i)).collect();
        assert!(!song.chart_variants_current());
        song.add_chart_versions();
        assert!(song.chart_variants_current());
        assert_eq!(song.charts.len(), 3);
        assert_eq!(song.charts[..2], original.charts[..]);
        for (i, d) in digests.iter().enumerate() {
            assert_eq!(&song.chart_digest(i), d);
        }
        let converted = &song.charts[2];
        assert_eq!(
            converted.derivation,
            Some(Derivation::WithoutCenter { source: 1 })
        );
        assert_eq!(converted.layout, "dance-single");
        assert_eq!(converted.name, "Without center");
        assert_eq!(converted.meter, 9);
        assert_eq!(converted.credit, original.charts[1].credit);
        assert_eq!(converted.timing, original.charts[1].timing);
        // Every note off the center keeps its row, kind and hold's end.
        let beat = 48;
        assert_eq!(
            lanes(converted),
            vec![
                (0, 0, &NoteKind::Tap),
                (beat, 2, &NoteKind::Tap),
                (3 * beat, 3, &NoteKind::Tap),
                (4 * beat, 1, &NoteKind::Tap),
                (8 * beat, 3, &NoteKind::Lift),
                (
                    10 * beat,
                    3,
                    &NoteKind::HoldHead {
                        end: crate::Tick(12 * beat)
                    }
                ),
            ]
        );
        let target = song.layout_of(converted).unwrap();
        let names: Vec<_> = converted
            .notes
            .iter()
            .map(|n| target.lanes[usize::from(n.lane)].name.as_str())
            .collect();
        assert_eq!(names, ["left", "up", "right", "down", "right", "right"]);
        assert_ne!(song.chart_digest(2), digests[1]);
        assert_eq!(song.effects_chart(2), 1);
        assert!(converted.pump_game() && song.charts[1].pump_game());
        assert!(!song.charts[0].pump_game());
        let first = song.clone();
        song.add_chart_versions();
        assert_eq!(song, first, "generation is idempotent");
    }

    #[test]
    fn pump_double_turns_each_pad_on_its_own() {
        let rows: Vec<String> = (0..10)
            .map(|i| (0..10).map(|j| if i == j { '1' } else { '0' }).collect())
            .collect();
        let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        rows.extend(["0000000000"; 2]);
        let mut song = pump_song("pump-double", &rows);
        song.add_chart_versions();
        let converted = &song.charts[2];
        assert_eq!(converted.layout, "dance-double");
        let target = song.layout_of(converted).unwrap();
        let got: Vec<_> = converted
            .notes
            .iter()
            .map(|n| {
                let lane = &target.lanes[usize::from(n.lane)];
                (n.tick.0 / 48, lane.pad, lane.name.as_str())
            })
            .collect();
        assert_eq!(
            got,
            [
                (0, 0, "left"),
                (1, 0, "up"),
                (3, 0, "right"),
                (4, 0, "down"),
                (5, 1, "left"),
                (6, 1, "up"),
                (8, 1, "right"),
                (9, 1, "down"),
            ]
        );
    }

    #[test]
    fn half_double_turns_its_four_corners_as_one_pad() {
        // Pad 1's C, UR, DR, pad 2's DL, UL, C.
        let mut song = pump_song(
            "pump-halfdouble",
            &[
                "100000", "010000", "001000", "000100", "000010", "000001", "000000", "000000",
            ],
        );
        song.add_chart_versions();
        let converted = &song.charts[2];
        assert_eq!(converted.layout, "dance-single");
        let target = song.layout_of(converted).unwrap();
        let got: Vec<_> = converted
            .notes
            .iter()
            .map(|n| {
                (
                    n.tick.0 / 48,
                    target.lanes[usize::from(n.lane)].name.as_str(),
                )
            })
            .collect();
        // Upper left of the four (pad 1's UR) is Up, lower left (pad 1's
        // DR) Left, lower right (pad 2's DL) Down, upper right (pad 2's
        // UL) Right.
        assert_eq!(got, [(1, "up"), (2, "left"), (3, "down"), (4, "right")]);
    }

    #[test]
    fn a_center_only_chart_has_no_version() {
        // Center notes, and mines elsewhere, leave nothing to judge.
        let mut song = pump_song("pump-single", &["00100", "M0000", "00200", "00300"]);
        assert!(song.chart_variants_current());
        song.add_chart_versions();
        assert_eq!(song.charts.len(), 2);
    }

    #[test]
    fn onigiri_versions_keep_their_indexes_before_center_ones() {
        // A song with a pump chart before a Dancing☆Onigiri one: the
        // onigiri-free version, generated first before center-free ones
        // existed, keeps its index.
        let mut song = pump_song("pump-single", &["10000", "00100", "00010", "00000"]);
        let mut danoni =
            make_song("5", "|left_data=60|space_data=90|right_data=120|").charts[0].clone();
        danoni.timing = None;
        song.charts.push(danoni);
        let mut earlier = song.clone();
        earlier.charts.extend(
            earlier
                .charts
                .iter()
                .enumerate()
                .filter_map(|(i, c)| convert(c, i, Kind::WithoutOnigiri))
                .collect::<Vec<_>>(),
        );
        assert_eq!(earlier.charts.len(), 4);
        assert!(!earlier.chart_variants_current());
        let mut fresh = song.clone();
        fresh.add_chart_versions();
        earlier.add_chart_versions();
        assert_eq!(earlier, fresh);
        assert_eq!(
            fresh.charts[3].derivation,
            Some(Derivation::WithoutOnigiri { source: 2 })
        );
        assert_eq!(
            fresh.charts[4].derivation,
            Some(Derivation::WithoutCenter { source: 1 })
        );
        assert_eq!(fresh.charts.len(), 5);
    }
}
