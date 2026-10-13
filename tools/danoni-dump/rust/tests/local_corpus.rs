//! Read-only local differential gate. No third-party content is a fixture.
use ddi_chart::{Chart as GameChart, NoteKind, Song, formats::sm};
use ddi_danoni_export::{compile_chart, model::*};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= 1e-9, "{what}: {a} != {b}");
}
fn compare_charts(a: &GameChart, sa: &Song, b: &GameChart, sb: &Song) {
    assert_eq!(a.layout, b.layout);
    assert_eq!(a.name, b.name);
    assert_eq!(a.difficulty, b.difficulty);
    assert_eq!(a.meter, b.meter);
    assert_eq!(a.notes.len(), b.notes.len());
    let ta = a.timing(sa);
    let tb = b.timing(sb);
    close(ta.offset_seconds, tb.offset_seconds, "offset");
    for (a, b) in a.notes.iter().zip(&b.notes) {
        assert_eq!(a.lane, b.lane);
        assert_eq!(a.tick, b.tick);
        assert_eq!(a.tick.quantization(), b.tick.quantization());
        assert_eq!(a.kind, b.kind);
        close(ta.seconds_at(a.tick), tb.seconds_at(b.tick), "note time");
        if let (NoteKind::HoldHead { end: a }, NoteKind::HoldHead { end: b }) = (a.kind, b.kind) {
            close(ta.seconds_at(a), tb.seconds_at(b), "hold tail");
        }
    }
    assert_eq!(ta.bpms.len(), tb.bpms.len());
    for (a, b) in ta.bpms.iter().zip(&tb.bpms) {
        assert_eq!(a.tick, b.tick);
        close(a.bpm, b.bpm, "BPM");
    }
    assert_eq!(ta.scrolls.len(), tb.scrolls.len());
    for (a, b) in ta.scrolls.iter().zip(&tb.scrolls) {
        assert_eq!(a.tick, b.tick);
        close(a.ratio, b.ratio, "scroll ratio");
    }
    let start = ta.seconds_at(ddi_chart::Tick::ZERO);
    let dt = ta
        .scrolls
        .iter()
        .find(|e| e.tick > ddi_chart::Tick::ZERO)
        .map_or(0.01, |e| ((ta.seconds_at(e.tick) - start) / 4.0).min(0.01));
    assert!(dt > 0.0);
    for x in [1.0, 2.0, 3.0] {
        let opt = ddi_engine::scroll::ScrollOptions {
            speed: ddi_engine::scroll::SpeedMod::XMod(x),
            ..Default::default()
        };
        let time = start + 2.0 * dt;
        let a = ddi_engine::scroll::note_y(opt, ta, a.notes[0].tick, time);
        let b = ddi_engine::scroll::note_y(opt, tb, b.notes[0].tick, time);
        assert!((a - b).abs() <= 0.001, "x{x} scroll position changed");
    }
}

#[test]
fn accepted_exports_match_rust_preparation() {
    let Some(root) = std::env::var_os("DDI_DANONI_EXPORTS") else {
        eprintln!("SKIP: set DDI_DANONI_EXPORTS to compare accepted local exports");
        return;
    };
    let root = Path::new(&root);
    let mut dirs = fs::read_dir(root.join("reports"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("manifest.json").is_file())
        .collect::<Vec<_>>();
    dirs.sort();
    assert!(!dirs.is_empty(), "No accepted report manifests");
    let (mut songs, mut charts, mut taps, mut holds, mut endpoints) = (0, 0, 0, 0, 0);
    let mut maximum_error_ms: f64 = 0.0;
    for dir in &dirs {
        let manifest = read(&dir.join("manifest.json"));
        assert_eq!(manifest["schema"], "ddi-playable-pack/experimental-1");
        for song in manifest["songs"].as_array().unwrap() {
            songs += 1;
            let baseline = sm::parse_ssc(
                &fs::read_to_string(dir.join(song["simfile"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            let sid = song["id"].as_str().unwrap();
            for (index, entry) in song["charts"].as_array().unwrap().iter().enumerate() {
                let reference = read(&dir.join(format!("references/{sid}-{index}.json")));
                let chart: Chart =
                    serde_json::from_value(reference["sourceChart"].clone()).unwrap();
                let metadata: Metadata =
                    serde_json::from_value(reference["sourceMetadata"].clone()).unwrap();
                let options:PreparationOptions=serde_json::from_value(json!({
                    "grid":entry["musicalQuantization"]["grid"],
                    "nativeMotion":entry["nativeScroll"]["sourceMotion"],
                    "scrollFactor":entry["nativeScroll"]["correctionFactor"].as_f64().unwrap_or(1.0),
                    "tierPolicy":manifest["tierPolicy"]["name"],
                    "tierBounds":manifest["tierPolicy"]["upperBounds"],
                    "rating":{"kind":"reviewed","stars":entry["difficultyStars"]},
                    "preserveLabel":entry["preservedDuplicateLabel"].as_bool().unwrap_or(false)
                })).unwrap();
                let before = serde_json::to_value(&chart).unwrap();
                let prepared = compile_chart(&chart, &metadata, &options)
                    .unwrap_or_else(|e| panic!("{} {sid} {}: {e}", dir.display(), chart.id));
                assert_eq!(serde_json::to_value(&chart).unwrap(), before);
                let compiled =
                    sm::parse_ssc(&format!("#TITLE:Local differential;\n{}", prepared.ssc))
                        .unwrap();
                compare_charts(
                    &compiled.charts[0],
                    &compiled,
                    &baseline.charts[index],
                    &baseline,
                );
                assert_eq!(
                    serde_json::to_value(&prepared.unrepresented_effects).unwrap(),
                    entry["unrepresentedEffects"]
                );
                if let Some(wanted) = entry["musicalQuantization"]["endpoints"].as_array() {
                    assert_eq!(prepared.endpoints.len(), wanted.len());
                    for (got, wanted) in prepared.endpoints.iter().zip(wanted) {
                        let got = serde_json::to_value(got).unwrap();
                        for key in ["lane", "kind", "index", "tick", "method"] {
                            assert_eq!(got[key], wanted[key], "{} {key}", chart.id);
                        }
                        for key in ["sourceSeconds", "exportSeconds", "shiftMs"] {
                            close(
                                got[key].as_f64().unwrap(),
                                wanted[key].as_f64().unwrap(),
                                key,
                            );
                        }
                    }
                }
                charts += 1;
                endpoints += prepared.endpoints.len();
                maximum_error_ms = maximum_error_ms.max(prepared.maximum_note_error_ms);
                for lane in &chart.lanes {
                    taps += lane.taps_seconds.len();
                    holds += lane.holds_seconds.len();
                }
            }
        }
    }
    assert!(charts > 0);
    println!(
        "{}",
        json!({"schema":"ddi-danoni-rust-chart-parity/1","works":dirs.len(),"songs":songs,"charts":charts,
        "taps":taps,"holds":holds,"noteEndpoints":endpoints,"maximumNoteErrorMs":maximum_error_ms,
        "unchangedSources":true,"gameParserCompared":true,"userMultipliers":[1,2,3],"passed":true})
    );
}

#[cfg(feature = "native")]
fn json_close(got: &Value, wanted: &Value, path: &str) {
    match (got, wanted) {
        (Value::Number(a), Value::Number(b)) => {
            close(a.as_f64().unwrap(), b.as_f64().unwrap(), path)
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                json_close(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (key, value) in b {
                json_close(&a[key], value, &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(got, wanted, "{path}"),
    }
}

#[cfg(feature = "native")]
#[test]
fn complete_songs_match_accepted_exports() {
    use ddi_danoni_export::{assets::sha256, native, work};
    let Some(root) = std::env::var_os("DDI_DANONI_EXPORTS") else {
        eprintln!("SKIP: set DDI_DANONI_EXPORTS to compare complete songs");
        return;
    };
    let exports = Path::new(&root);
    let registry = exports.parent().unwrap().join("export-registry.json");
    let mut dirs = fs::read_dir(exports.join("reports"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.join("manifest.json").is_file())
        .collect::<Vec<_>>();
    dirs.sort();
    let (mut songs, mut charts, mut banners, mut rated, mut heads) = (0, 0, 0, 0, 0);
    for dir in dirs {
        let id = dir.file_name().unwrap().to_str().unwrap();
        let job = native::accepted_job(&registry, exports, id).unwrap();
        let source = native::DirectorySource::new(job.roots.clone()).unwrap();
        let prepared = work::prepare_work(&job.work, &source, &mut |_| Ok(()))
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let previous = read(&dir.join("manifest.json"));
        assert_eq!(
            prepared.manifest["sourceManifestSha256"],
            previous["sourceManifestSha256"]
        );
        let new_songs = prepared.manifest["songs"].as_array().unwrap();
        let old_songs = previous["songs"].as_array().unwrap();
        assert_eq!(new_songs.len(), old_songs.len());
        for (new, old) in new_songs.iter().zip(old_songs) {
            for key in [
                "id",
                "title",
                "musicSha256",
                "sourceAudioSha256",
                "pcmSampleRate",
                "pcmChannels",
                "pcmFrames",
                "artwork",
            ] {
                assert_eq!(new[key], old[key], "{id} {key}");
            }
            json_close(&new["preview"], &old["preview"], &format!("{id} preview"));
            json_close(
                &new["displayBpm"],
                &old["displayBpm"],
                &format!("{id} display BPM"),
            );
            let file = |key: &str| {
                prepared
                    .files
                    .iter()
                    .find(|a| a.path == new[key].as_str().unwrap())
                    .unwrap()
            };
            let text = std::str::from_utf8(&file("simfile").bytes).unwrap();
            let got = sm::parse_ssc(text).unwrap();
            let wanted = sm::parse_ssc(
                &fs::read_to_string(dir.join(old["simfile"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            assert_eq!(got.title, wanted.title);
            assert_eq!(got.artist, wanted.artist);
            assert_eq!(got.music, wanted.music);
            assert_eq!(got.banner, wanted.banner);
            assert_eq!(got.display_bpm, wanted.display_bpm);
            assert!((got.preview_start - wanted.preview_start).abs() < 1e-6);
            assert!((got.preview_length - wanted.preview_length).abs() < 1e-6);
            assert_eq!(
                file("music").bytes.as_slice(),
                fs::read(dir.join(old["music"].as_str().unwrap())).unwrap()
            );
            assert_eq!(sha256(&file("music").bytes), new["musicSha256"]);
            if !new["banner"].is_null() {
                banners += 1;
                assert_eq!(
                    file("banner").bytes.as_slice(),
                    fs::read(dir.join(old["banner"].as_str().unwrap())).unwrap()
                );
            }
            for (index, (new, old)) in new["charts"]
                .as_array()
                .unwrap()
                .iter()
                .zip(old["charts"].as_array().unwrap())
                .enumerate()
            {
                compare_charts(&got.charts[index], &got, &wanted.charts[index], &wanted);
                for key in [
                    "id",
                    "difficultyStars",
                    "difficultyClassification",
                    "sourceDifficultyLabel",
                    "sourceDifficultyClassification",
                    "preservedDuplicateLabel",
                    "unrepresentedEffects",
                    "sourceRating",
                ] {
                    assert_eq!(new[key], old[key], "{id} {key}");
                }
                charts += 1;
                heads += got.charts[index].notes.len();
                rated += usize::from(!new["difficultyStars"].is_null());
            }
            songs += 1;
        }
        eprintln!("complete-work: {id}");
    }
    assert!(charts > 0);
    println!(
        "{}",
        json!({"schema":"ddi-danoni-rust-song-parity/1","songs":songs,"charts":charts,"banners":banners,"ratedCharts":rated,"judgedHeads":heads,"previewsCompared":true,"mediaByteIdentical":true,"sourceRatingsReparsed":true,"gameParserCompared":true,"passed":true})
    );
}

#[cfg(feature = "native")]
#[test]
fn written_generation_matches_the_accepted_songs() {
    use ddi_danoni_export::assets::sha256;
    let Some(stage) = std::env::var_os("DDI_DANONI_RUST_GENERATION") else {
        eprintln!("SKIP: set DDI_DANONI_RUST_GENERATION for the written generation gate");
        return;
    };
    let exports = std::env::var_os("DDI_DANONI_EXPORTS")
        .expect("DDI_DANONI_EXPORTS is required with a written generation");
    let (stage, exports) = (Path::new(&stage), Path::new(&exports));
    let generation = read(&stage.join("manifest.json"));
    assert_eq!(generation["schema"], "ddi-danoni-rust-offline-generation/1");
    assert_eq!(
        generation["acceptedGenerationSha256"],
        sha256(&fs::read(exports.join("index.json")).unwrap())
    );
    let (mut songs, mut charts, mut artifacts) = (0, 0, 0);
    for row in generation["works"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let path = stage.join(row["manifest"].as_str().unwrap());
        let root = path.parent().unwrap();
        let got = read(&path);
        for file in got["artifacts"].as_array().unwrap() {
            let body = fs::read(root.join(file["path"].as_str().unwrap())).unwrap();
            assert_eq!(body.len(), file["bytes"].as_u64().unwrap() as usize);
            assert_eq!(sha256(&body), file["sha256"]);
            artifacts += 1;
        }
        let old_root = exports.join("reports").join(id);
        let old = read(&old_root.join("manifest.json"));
        assert_eq!(
            got["songs"].as_array().unwrap().len(),
            old["songs"].as_array().unwrap().len()
        );
        for (got, old) in got["songs"]
            .as_array()
            .unwrap()
            .iter()
            .zip(old["songs"].as_array().unwrap())
        {
            let a = sm::parse_ssc(
                &fs::read_to_string(root.join(got["simfile"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            let b = sm::parse_ssc(
                &fs::read_to_string(old_root.join(old["simfile"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            assert_eq!(a.title, b.title);
            assert_eq!(a.artist, b.artist);
            assert_eq!(a.display_bpm, b.display_bpm);
            assert_eq!(a.banner, b.banner);
            assert_eq!(a.music, b.music);
            assert!((a.preview_start - b.preview_start).abs() < 1e-6);
            assert!((a.preview_length - b.preview_length).abs() < 1e-6);
            assert_eq!(a.charts.len(), b.charts.len());
            for (a_, b_) in a.charts.iter().zip(&b.charts) {
                compare_charts(a_, &a, b_, &b);
                charts += 1;
            }
            songs += 1;
        }
    }
    assert_eq!(songs, generation["songs"].as_u64().unwrap() as usize);
    assert_eq!(charts, generation["charts"].as_u64().unwrap() as usize);
    println!(
        "{}",
        json!({"schema":"ddi-danoni-rust-written-generation-parity/1","songs":songs,"charts":charts,"artifacts":artifacts,"allArtifactHashesVerified":true,"gameParserCompared":true,"passed":true})
    );
}
