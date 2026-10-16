#![cfg(feature = "native")]
//! Explicit local study: normal fitted exports through the game's real parser.
//! Musical aliases/colour changes are reported, never mistaken for chart parity.
use ddi_danoni_export::{assets::sha256, audio, model::Chart};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn close(a: f64, b: f64, eps: f64) {
    assert!((a - b).abs() <= eps, "{a} != {b} (budget {eps})");
}
#[test]
fn fitted_exports_preserve_endpoints_scroll_media_and_report_musical_differences() {
    let Some(cases) = std::env::var_os("DDI_DANONI_FRESH_EXPORT_CASES") else {
        eprintln!("SKIP: set DDI_DANONI_FRESH_EXPORT_CASES; no external works counted");
        return;
    };
    let table = read(Path::new(&cases));
    assert_eq!(table["schema"], "ddi-danoni-fresh-export-cases/1");
    let mut records = Vec::new();
    let (mut charts, mut songs, mut endpoints, mut velocities, mut maximum) =
        (0usize, 0usize, 0usize, 0usize, 0f64);
    for case in table["cases"].as_array().unwrap() {
        let root = Path::new(case["output"].as_str().unwrap());
        let manifest = read(&root.join("manifest.json"));
        for f in manifest["artifacts"].as_array().unwrap() {
            let bytes = fs::read(root.join(f["path"].as_str().unwrap())).unwrap();
            assert_eq!(sha256(&bytes), f["sha256"]);
            assert_eq!(bytes.len() as u64, f["bytes"]);
        }
        let baseline_path = Path::new(case["baselineManifest"]["path"].as_str().unwrap());
        let baseline_bytes = fs::read(baseline_path).unwrap();
        assert_eq!(sha256(&baseline_bytes), case["baselineManifest"]["sha256"]);
        let baseline: Value = serde_json::from_slice(&baseline_bytes).unwrap();
        for binding in case["baselineFiles"].as_array().unwrap() {
            let bytes = fs::read(
                baseline_path
                    .parent()
                    .unwrap()
                    .join(binding["path"].as_str().unwrap()),
            )
            .unwrap();
            assert_eq!(sha256(&bytes), binding["sha256"]);
        }
        let preparation = read(&root.join("provenance/preparation.json"));
        assert_eq!(preparation["tempo"]["status"], "estimated");
        assert_eq!(preparation["tempo"]["sourceNotesModified"], false);
        let source: Vec<Chart> = manifest["sourceManifest"]["charts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| serde_json::from_value(v.clone()).unwrap())
            .collect();
        for song in manifest["songs"].as_array().unwrap() {
            songs += 1;
            let parsed = ddi_chart::formats::sm::parse_ssc(
                &fs::read_to_string(root.join(song["simfile"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            close(
                parsed.preview_start,
                song["preview"]["startSeconds"].as_f64().unwrap(),
                1e-8,
            );
            close(parsed.preview_length, 12.0, 1e-8);
            assert!(parsed.banner.is_some());
            let banner = image::load_from_memory(
                &fs::read(root.join(song["banner"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            assert_eq!([banner.width(), banner.height()], [640, 200]);
            let decoded = audio::decode(
                &fs::read(root.join(song["music"].as_str().unwrap())).unwrap(),
                &audio::DecodeOptions::default(),
                &mut || Ok(()),
            )
            .unwrap();
            assert_eq!(decoded.pcm.rate as u64, song["pcmSampleRate"]);
            assert_eq!(decoded.pcm.channels as u64, song["pcmChannels"]);
            assert_eq!(decoded.pcm.frames() as u64, song["pcmFrames"]);
            assert_eq!(decoded.report["skippedPackets"], 0);
            assert_eq!(
                parsed.charts.len(),
                song["charts"].as_array().unwrap().len()
            );
            for (game, c) in parsed.charts.iter().zip(song["charts"].as_array().unwrap()) {
                charts += 1;
                let id = c["id"].as_str().unwrap();
                let original = source.iter().find(|v| v.id == id).unwrap();
                assert!(original.mode.is_playable());
                let timing = game.timing(&parsed);
                let mut heads = BTreeMap::new();
                let mut tails = BTreeMap::new();
                for n in &game.notes {
                    heads.insert((n.lane as usize, n.tick.0), n);
                    if let ddi_chart::NoteKind::HoldHead { end } = n.kind {
                        tails.insert((n.lane as usize, end.0), n);
                    }
                }
                assert_eq!(
                    game.notes.len(),
                    original
                        .lanes
                        .iter()
                        .map(|l| l.taps_seconds.len() + l.holds_seconds.len())
                        .sum::<usize>()
                );
                assert_eq!(
                    tails.len(),
                    original
                        .lanes
                        .iter()
                        .map(|l| l.holds_seconds.len())
                        .sum::<usize>()
                );
                let padding = c["paddingTicks"].as_i64().unwrap();
                let mut colours = BTreeMap::new();
                for endpoint in c["endpoints"].as_array().unwrap() {
                    let lane = endpoint["lane"].as_u64().unwrap() as usize;
                    let index = endpoint["index"].as_u64().unwrap() as usize;
                    let tick = endpoint["tick"].as_i64().unwrap() + padding;
                    let kind = endpoint["kind"].as_str().unwrap();
                    let source_seconds = match kind {
                        "tap" => original.lanes[lane].taps_seconds[index],
                        "hold-head" => original.lanes[lane].holds_seconds[index][0],
                        "hold-tail" => original.lanes[lane].holds_seconds[index][1],
                        _ => panic!("Unknown endpoint {kind}"),
                    };
                    close(
                        endpoint["sourceSeconds"].as_f64().unwrap(),
                        source_seconds,
                        1e-10,
                    );
                    if kind == "hold-tail" {
                        assert!(tails.contains_key(&(lane, tick)));
                    } else {
                        assert!(heads.contains_key(&(lane, tick)));
                        colours.insert((lane, kind, index), ddi_chart::Tick(tick).quantization());
                    }
                    let actual = timing.seconds_at(ddi_chart::Tick(tick));
                    close(actual, endpoint["exportSeconds"].as_f64().unwrap(), 1e-8);
                    let error = (actual - source_seconds).abs() * 1000.0;
                    assert!(error <= 9.000001, "{id}: {error} ms");
                    maximum = maximum.max(error);
                    endpoints += 1;
                }
                let stars = original
                    .source
                    .get("difficultyStars")
                    .and_then(Value::as_u64);
                assert_eq!(c["difficultyStars"].as_u64(), stars);
                assert_eq!(game.meter as u64, stars.unwrap_or(0));
                let expected_tier = match stars {
                    None => "Edit",
                    Some(0..=1) => "Beginner",
                    Some(2..=3) => "Easy",
                    Some(4..=5) => "Medium",
                    Some(6..=7) => "Hard",
                    _ => "Challenge",
                };
                assert_eq!(c["difficultyClassification"], expected_tier);
                let base = c["nativeMotion"]["baseArrowHeightsPerSecond"]
                    .as_f64()
                    .unwrap();
                assert!(base > 0.0);
                let mut boundaries = vec![
                    0.0,
                    decoded.pcm.frames() as f64 / f64::from(decoded.pcm.rate),
                ];
                boundaries.extend(timing.scrolls.iter().map(|s| timing.seconds_at(s.tick)));
                boundaries.sort_by(f64::total_cmp);
                boundaries.dedup();
                for window in boundaries.windows(2).filter(|w| w[1] - w[0] > 0.1) {
                    let time = (window[0] + window[1]) * 0.5;
                    let authored = original
                        .speed
                        .iter()
                        .rfind(|s| s.seconds <= time)
                        .map_or(1.0, |s| s.multiplier);
                    for x in [1.0, 2.0, 3.0] {
                        let opts = ddi_engine::scroll::ScrollOptions {
                            speed: ddi_engine::scroll::SpeedMod::XMod(x),
                            ..Default::default()
                        };
                        let target = timing.displayed_beat(timing.beat_at(time));
                        let y = |t| {
                            ddi_engine::scroll::ScrollState::at(opts, timing, t)
                                .y_displayed(target, time) as f64
                        };
                        close(
                            (y(time) - y(time + 0.01)) / 0.01,
                            base * authored * x,
                            0.002,
                        );
                        velocities += 1;
                    }
                }
                let old_id = case["chartMapping"][id].as_str().unwrap();
                let (old_song, old_c) = baseline["songs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find_map(|s| {
                        s["charts"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|v| v["id"] == old_id)
                            .map(|c| (s, c))
                    })
                    .unwrap();
                close(
                    base,
                    old_c["nativeScroll"]["baseArrowHeightsPerSecond"]
                        .as_f64()
                        .unwrap(),
                    1e-9,
                );
                close(
                    song["preview"]["startSeconds"].as_f64().unwrap(),
                    old_song["preview"]["startSeconds"].as_f64().unwrap(),
                    1e-8,
                );
                assert_eq!(song["pcmFrames"], old_song["pcmFrames"]);
                assert_eq!(song["pcmSampleRate"], old_song["pcmSampleRate"]);
                assert_eq!(song["preview"]["selectedAudioSupport"]["supported"], true);
                let old_parsed = ddi_chart::formats::sm::parse_ssc(
                    &fs::read_to_string(
                        baseline_path
                            .parent()
                            .unwrap()
                            .join(old_song["simfile"].as_str().unwrap()),
                    )
                    .unwrap(),
                )
                .unwrap();
                let old_index = old_song["charts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|c| c["id"] == old_id)
                    .unwrap();
                let old_game = &old_parsed.charts[old_index];
                assert_eq!(game.layout, old_game.layout);
                assert_eq!(game.notes.len(), old_game.notes.len());
                let old_q = &old_c["musicalQuantization"];
                let old_padding = old_q["grid"]["paddingTicks"].as_i64().unwrap_or(0);
                // Compare colours by source endpoint identity, independently of row order.
                let mut changed = 0;
                let mut compared = 0;
                let mut old_counts = BTreeMap::<String, usize>::new();
                let mut new_counts = BTreeMap::<String, usize>::new();
                for e in old_q["endpoints"].as_array().unwrap() {
                    let kind = e["kind"].as_str().unwrap();
                    if kind == "hold-tail" {
                        continue;
                    }
                    let key = (
                        e["lane"].as_u64().unwrap() as usize,
                        kind,
                        e["index"].as_u64().unwrap() as usize,
                    );
                    let old_colour =
                        ddi_chart::Tick(e["tick"].as_i64().unwrap() + old_padding).quantization();
                    let new_colour = colours[&key];
                    compared += 1;
                    changed += usize::from(old_colour != new_colour);
                    *old_counts.entry(format!("{old_colour:?}")).or_default() += 1;
                    *new_counts.entry(format!("{new_colour:?}")).or_default() += 1;
                }
                let old_grid = &old_q["grid"];
                let new_grid = &c["musicalGrid"];
                records.push(json!({"work":case["id"],"chart":id,"mode":original.mode,"timingBpm":song["timingBpm"],"displayBpm":format!("{:?}",parsed.display_bpm),"previousDisplayBpm":format!("{:?}",old_parsed.display_bpm),"fittedBpm":new_grid["bpm"],"previousBpm":old_grid["bpm"],"phase":new_grid["beatZeroSeconds"],"previousPhase":old_grid["beatZeroSeconds"],"subdivisionsPerBeat":new_grid["subdivisionsPerBeat"],"previousSubdivisionsPerBeat":old_grid["subdivisionsPerBeat"],"phaseEvidence":new_grid["phaseEvidence"],"phaseStatus":new_grid["phaseStatus"],"maximumNoteErrorMs":c["maximumNoteErrorMs"],"noteHeadsCompared":compared,"noteColourChanges":changed,"previousColours":old_counts,"newColours":new_counts,"previewStart":song["preview"]["startSeconds"],"previousPreviewStart":old_song["preview"]["startSeconds"],"previewSupport":song["preview"]["selectedAudioSupport"],"stars":c["difficultyStars"],"tier":c["difficultyClassification"],"previousStars":old_c["difficultyStars"],"previousTier":old_c["difficultyClassification"],"baseScroll":base,"musicalApproval":false}));
            }
        }
    }
    assert_eq!(charts, table["expectedCharts"].as_u64().unwrap() as usize);
    let report = json!({"schema":"ddi-danoni-fresh-export-validation/1","works":table["cases"].as_array().unwrap().len(),"songs":songs,"charts":charts,"endpoints":endpoints,"maximumNoteErrorMs":maximum,"velocityChecks":velocities,"records":records,"functionalContractsPassed":true,"musicalParityClaimed":false});
    if let Some(path) = std::env::var_os("DDI_DANONI_FRESH_EXPORT_RECEIPT") {
        fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!("{report}");
}
