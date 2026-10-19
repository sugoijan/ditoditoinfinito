//! The SDK's verified publication handed to the ordinary desktop playback path.
//! Hosts never reconstruct managed-store transaction paths or rescan its tree.

use super::LibSong;
use ddi_danoni_export::managed_store::PlaybackSnapshot;

/// Retains SDK ids so settings/history survive replacement. Catalog entries stay
/// original-only; enrich a clone from the verified SSC for ordinary playback.
pub fn songs(snapshot: PlaybackSnapshot) -> Result<Vec<LibSong>, String> {
    snapshot
        .songs
        .into_iter()
        .map(|song| {
            let chart = song
                .entry
                .chart
                .to_path(&song.directory)
                .ok_or_else(|| "Invalid managed chart path".to_string())?;
            let text = std::fs::read_to_string(chart)
                .map_err(|e| format!("Cannot read managed chart: {e}"))?;
            let parsed = ddi_library::scan::parse_simfile(&text, "ssc")
                .map_err(|e| format!("Cannot parse managed chart: {e}"))?;
            if song.entry.charts.iter().any(|chart| {
                parsed
                    .charts
                    .get(chart.index)
                    .is_none_or(|c| c.derivation.is_some())
            }) {
                return Err("Managed SSC is missing an admitted source chart".into());
            }
            let entry = ddi_library::manifest::with_chart_variants(&song.entry, &parsed);
            if !entry.charts.starts_with(&song.entry.charts) {
                return Err("Chart variants changed admitted source metadata".into());
            }
            Ok(LibSong {
                root: song.directory,
                zip: false,
                dir: Default::default(),
                pack: "DanOni imports".into(),
                entry,
                warnings: Vec::new(),
                notes: Vec::new(),
                work: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_danoni_export::managed_store::PlaybackSong;

    #[test]
    fn adapter_keeps_admitted_identity_and_adds_separate_playable_variants() {
        let directory = std::env::temp_dir().join(format!(
            "ddi-managed-adapter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let text = "#VERSION:0.83;#TITLE:Export;#BPMS:0=120;#NOTEDATA:;\
            #STEPSTYPE:danoni-5;#CHARTNAME:Hard;#DIFFICULTY:Hard;#METER:8;\
            #BPMS:0=120;#NOTES:10000\n00100\n00001\n00000;";
        std::fs::write(directory.join("song.ssc"), text).unwrap();
        let original = ddi_chart::formats::sm::parse_ssc(text).unwrap();
        let mut entry = ddi_library::manifest::summarize(Default::default(), &original);
        entry.id = "u-sdk-stable-id".into();
        entry.chart = "song.ssc".into();
        entry.music = "music.opus".into();
        let admitted = serde_json::to_vec(&entry).unwrap();
        let rows = songs(PlaybackSnapshot {
            revision: "test-revision".into(),
            songs: vec![PlaybackSong {
                entry: entry.clone(),
                directory: directory.clone(),
            }],
        })
        .unwrap();
        let song = &rows[0];
        assert_eq!(song.entry.id, entry.id);
        assert_eq!(song.root, directory);
        assert!(song.dir.as_str().is_empty());
        assert!(!song.zip);
        assert!(song.work.is_none());
        assert_eq!(&song.entry.charts[..entry.charts.len()], entry.charts);
        assert_eq!(song.entry.charts.len(), 2);
        assert_eq!(song.entry.charts[1].layout, "dance-single");
        assert!(song.entry.charts[1].derivation.is_some());
        assert_ne!(song.entry.charts[0].digest, song.entry.charts[1].digest);
        assert!(!song.entry.needs_chart_variants());
        assert_eq!(serde_json::to_vec(&entry).unwrap(), admitted);
        assert_eq!(
            std::fs::read_to_string(directory.join("song.ssc")).unwrap(),
            text
        );
        std::fs::write(directory.join("song.ssc"), "not a simfile").unwrap();
        assert!(
            songs(PlaybackSnapshot {
                revision: "bad".into(),
                songs: vec![PlaybackSong {
                    entry,
                    directory: directory.clone()
                }]
            })
            .is_err()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
