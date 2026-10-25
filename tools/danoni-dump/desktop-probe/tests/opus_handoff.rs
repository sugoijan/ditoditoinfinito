//! Original synthetic exports through the real desktop scanner/decoder/player.
use ddi_danoni_desktop_probe::audit;
use ddi_danoni_export::{
    Chart, PreparationOptions,
    audio::{Pcm16, encode_opus},
    compile_chart,
    model::{Lane, Metadata, Mode},
};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn ssc_opus_preserves_odd_frame_counts_and_plays_every_supported_layout() {
    let root = std::env::temp_dir().join(format!(
        "ddi-ssc-opus-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let frames = 48_000 * 4 + 17;
    for channels in [1, 2] {
        // Nested folders also exercise logical-to-OS path conversion.
        let folder = root.join("pack").join(channels.to_string()).join("song");
        fs::create_dir_all(&folder).unwrap();
        let pcm = Pcm16 {
            rate: 48_000,
            channels,
            samples: (0..frames)
                .flat_map(|i| {
                    let v = ((i as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin() * 4000.0)
                        as i16;
                    (0..channels).map(move |c| if c == 0 { v } else { -v / 2 })
                })
                .collect(),
        };
        fs::write(
            folder.join("music.opus"),
            encode_opus(&pcm, 0.5, &mut || Ok(())).unwrap().0,
        )
        .unwrap();
        let mut ssc = String::from(
            "#TITLE:Synthetic SSC Opus;\n#MUSIC:music.opus;\n#BPMS:0=75;\n#OFFSET:0;\n",
        );
        for mode in [Mode::Five, Mode::Seven, Mode::NineA, Mode::NineB] {
            let chart = Chart {
                id: mode.label().into(),
                mode,
                lanes: (0..mode.lanes())
                    .map(|lane| Lane {
                        taps_seconds: vec![0.5 + lane as f64 / 60.0, 1.5],
                        holds_seconds: vec![[2.0, 2.5]],
                        source: Metadata::new(),
                    })
                    .collect(),
                speed: vec![],
                metadata: Metadata::new(),
                source: Metadata::new(),
            };
            ssc.push_str(
                &compile_chart(&chart, &Metadata::new(), &PreparationOptions::default())
                    .unwrap()
                    .ssc,
            );
        }
        fs::write(folder.join("song.ssc"), ssc).unwrap();
    }
    let (songs, scan) = ddi_desktop::library::scan_folders(std::slice::from_ref(&root));
    assert!(
        scan.failed.is_empty() && scan.missing.is_empty() && scan.duplicates.is_empty(),
        "{scan:?}"
    );
    assert_eq!(songs.len(), 2, "{scan:?}");
    let report = audit(&songs).unwrap();
    let mut channels = Vec::new();
    for song in report["songs"].as_array().unwrap() {
        assert_eq!(song["audioFrames"], frames);
        assert_eq!(song["audioRate"], 48_000);
        channels.push(song["audioChannels"].as_u64().unwrap());
        let charts = song["charts"].as_array().unwrap();
        let mut layouts = charts
            .iter()
            .map(|c| c["layout"].as_str().unwrap())
            .collect::<Vec<_>>();
        layouts.sort_unstable();
        assert_eq!(
            layouts,
            [
                "dance-double",
                "dance-single",
                "dance-solo",
                "danoni-5",
                "danoni-7",
                "danoni-9A",
                "danoni-9B",
                // 5 keys converted.
                "pump-single",
                "techno-single8",
            ]
        );
        for chart in charts {
            let layout = chart["layout"].as_str().unwrap();
            let lanes = match layout {
                "danoni-5" | "pump-single" => 5,
                "danoni-7" => 7,
                "danoni-9A" | "danoni-9B" => 9,
                "dance-single" => 4,
                "dance-solo" => 6,
                "dance-double" | "techno-single8" => 8,
                other => panic!("Unexpected layout: {other}"),
            };
            let runs = chart["runs"].as_array().unwrap();
            assert_eq!(
                runs.iter()
                    .map(|r| r["fps"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                [60, 144]
            );
            for run in runs {
                assert_eq!(run["topTier"], lanes * 3);
                assert_eq!(run["held"], lanes);
                assert_eq!(run["replayMatches"], true);
                // The originals, and 5 keys on pump single, keep every note.
                assert_eq!(
                    run["assist"],
                    !(layout.starts_with("danoni-") || layout == "pump-single")
                );
            }
        }
    }
    channels.sort_unstable();
    assert_eq!(channels, [1, 2]);
    fs::remove_dir_all(root).unwrap();
}
