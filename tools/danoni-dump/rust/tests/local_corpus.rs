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
    let registry = std::env::var_os("DDI_DANONI_EXPORT_REGISTRY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| exports.parent().unwrap().join("export-registry.json"));
    let mut dirs = fs::read_dir(exports.join("reports"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.join("manifest.json").is_file())
        .collect::<Vec<_>>();
    dirs.sort();
    if let Ok(ids) = std::env::var("DDI_DANONI_WORK_IDS") {
        let wanted = ids.split(',').collect::<std::collections::BTreeSet<_>>();
        assert!(!wanted.is_empty() && !wanted.contains(""));
        dirs.retain(|p| wanted.contains(p.file_name().unwrap().to_str().unwrap()));
        assert_eq!(dirs.len(), wanted.len(), "Every requested work must exist");
    }
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
    assert!(matches!(
        generation["schema"].as_str(),
        Some("ddi-danoni-rust-offline-generation/1" | "ddi-danoni-rust-packaged-generation/1")
    ));
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

#[test]
fn rust_decode_preserves_all_source_audio_timelines() {
    use ddi_danoni_export::audio::{self, DecodeOptions};
    let Some(root) = std::env::var_os("DDI_DANONI_AUDIO_GENERATION") else {
        eprintln!("SKIP: set DDI_DANONI_AUDIO_GENERATION for native audio gate");
        return;
    };
    let root = Path::new(&root);
    let generation = read(&root.join("manifest.json"));
    let mut checks = Vec::new();
    for work in generation["works"].as_array().unwrap() {
        let dir = root.join("works").join(work["id"].as_str().unwrap());
        let m = read(&dir.join("manifest.json"));
        for song in m["songs"].as_array().unwrap() {
            let source_hash = song["sourceAudioSha256"].as_str().unwrap();
            let key = m["inputs"]
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, b)| b["sha256"] == source_hash)
                .unwrap()
                .0;
            let original = fs::read(dir.join(m["inputArtifacts"][key].as_str().unwrap())).unwrap();
            let got = audio::decode(&original, &DecodeOptions::default(), &mut || Ok(())).unwrap();
            let reference =
                fs::read(dir.join(format!("references/{}.wav", song["id"].as_str().unwrap())))
                    .unwrap();
            let old = audio::decode(&reference, &DecodeOptions::default(), &mut || Ok(())).unwrap();
            assert_eq!(got.pcm.rate, old.pcm.rate);
            assert_eq!(got.pcm.channels, old.pcm.channels);
            assert_eq!(
                got.pcm.frames(),
                old.pcm.frames(),
                "{}/{}",
                work["id"],
                song["id"]
            );
            let equal = got.pcm == old.pcm;
            let match_ = audio::origin_match(
                &old.pcm,
                &got.pcm,
                0,
                Some((old.pcm.rate as usize * 12).min(old.pcm.frames())),
                &mut || Ok(()),
            );
            // Exact PCM is independently stronger evidence than correlation for
            // quiet/sparse audio. Codec differences must pass measured matching.
            if !equal {
                let m = match_.as_ref().unwrap();
                assert!(
                    m.lag_samples.abs() <= 1,
                    "{}/{}: {:?}",
                    work["id"],
                    song["id"],
                    m
                );
            }
            let max_error = got
                .pcm
                .samples
                .iter()
                .zip(&old.pcm.samples)
                .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs())
                .max()
                .unwrap();
            checks.push(json!({"work":work["id"],"song":song["id"],"sourceAudioSha256":source_hash,"decode":got.report,"pcmSamplesIdentical":equal,"maximumSampleValueDifference":max_error,"origin":match_.ok()}));
        }
    }
    assert_eq!(checks.len(), generation["songs"].as_u64().unwrap() as usize);
    println!(
        "{}",
        json!({"schema":"ddi-danoni-rust-audio-parity/1","songs":checks.len(),"checks":checks,"passed":true})
    );
}

#[test]
fn raw_mp3_matches_captured_native_origins() {
    use ddi_danoni_export::{
        assets::sha256,
        audio::{self, DecodeOptions, Pcm16},
    };
    let Some(path) = std::env::var_os("DDI_DANONI_AUDIO_ORIGIN_CASES") else {
        eprintln!("SKIP: set DDI_DANONI_AUDIO_ORIGIN_CASES");
        return;
    };
    let cases = read(Path::new(&path));
    let mut checks = Vec::new();
    let mut failures = Vec::new();
    let bytes = |r: &Value| {
        let b = fs::read(r["path"].as_str().unwrap()).unwrap();
        assert_eq!(sha256(&b), r["sha256"]);
        b
    };
    for row in cases["cases"].as_array().unwrap() {
        let raw = bytes(&row["mp3"]);
        let native_raw = bytes(&row["nativePcm"]);
        let snapshot: Value = serde_json::from_slice(&bytes(&row["snapshot"])).unwrap();
        let run: Value = serde_json::from_slice(&bytes(&row["run"])).unwrap();
        let old: Value = serde_json::from_slice(&bytes(&row["oldOrigin"])).unwrap();
        assert_eq!(run["exitCode"], 0);
        assert_eq!(run["inputsUnchanged"], true);
        assert_eq!(snapshot["audio"]["sampleRate"], 44100);
        assert_eq!(snapshot["audio"]["channels"], 2);
        let attempt = (|| -> ddi_danoni_export::Result<Value> {
            let decoded = audio::decode(&raw, &DecodeOptions::default(), &mut || Ok(()))?;
            let (comparison, resample) =
                audio::resample(&decoded.pcm, 44100, true, &mut || Ok(()))?;
            let native = Pcm16 {
                rate: 44100,
                channels: 2,
                samples: native_raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| i16::from_le_bytes([b[0], b[1]]))
                    .collect(),
            };
            let m = audio::origin_match(
                &native,
                &comparison,
                row["startFrame"].as_u64().unwrap() as usize,
                row["endFrame"].as_u64().map(|n| n as usize),
                &mut || Ok(()),
            )?;
            let shift =
                (m.lag_samples as f64 * decoded.pcm.rate as f64 / 44100.0).round_ties_even() as i64;
            let old_shift = old["appliedShiftSamples"].as_i64().unwrap();
            if (shift - old_shift).abs() > 1 {
                return Err(ddi_danoni_export::Error {
                    code: "origin-parity",
                    message: format!("New shift {shift} differs from old {old_shift}"),
                });
            }
            Ok(
                json!({"work":row["work"],"sourceMp3Sha256":row["mp3"]["sha256"],"nativePcmSha256":row["nativePcm"]["sha256"],"decode":decoded.report,"resample":resample,"origin":m,"newShiftSourceFrames":shift,"oldShiftSourceFrames":old_shift}),
            )
        })();
        match attempt {
            Ok(c) => checks.push(c),
            Err(e) => failures.push(json!({"work":row["work"],"error":e})),
        }
    }
    println!(
        "{}",
        json!({"schema":"ddi-danoni-rust-native-origin-parity/1","passed":failures.is_empty(),"checks":checks,"failures":failures,"excluded":cases["excluded"]})
    );
    assert!(
        failures.is_empty(),
        "{} raw origin failures",
        failures.len()
    );
    assert!(!checks.is_empty());
}

#[cfg(all(feature = "native", any(feature = "opus", feature = "vorbis")))]
#[test]
fn native_codec_receipts_prepare_the_complete_corpus() {
    use ddi_danoni_export::{
        assets::*,
        audio::{self, DecodeOptions, MediaRequest},
        native,
        work::*,
    };
    use std::collections::BTreeMap;
    let Some(root) = std::env::var_os("DDI_DANONI_AUDIO_GENERATION") else {
        eprintln!("SKIP: set DDI_DANONI_AUDIO_GENERATION");
        return;
    };
    let Some(output) = std::env::var_os("DDI_DANONI_AUDIO_OUTPUT") else {
        eprintln!("SKIP: set fresh DDI_DANONI_AUDIO_OUTPUT for codec preparation");
        return;
    };
    let (root, output) = (Path::new(&root), Path::new(&output));
    fs::create_dir(output).expect("fresh codec output required");
    let generation = read(&root.join("manifest.json"));
    let codec: Codec = serde_json::from_value(json!(
        std::env::var("DDI_DANONI_AUDIO_CODEC").unwrap_or_else(|_| if cfg!(feature = "opus") {
            "opus"
        } else {
            "ogg"
        }
        .into())
    ))
    .unwrap();
    let extension = match codec {
        Codec::Wav => "wav",
        Codec::Ogg => "ogg",
        Codec::Opus => "opus",
    };
    let mut checks = Vec::new();
    for row in generation["works"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let dir = root.join("works").join(id);
        let w = read(&dir.join("manifest.json"));
        let mut request: WorkRequest = serde_json::from_value(w["request"].clone()).unwrap();
        let mut roots = BTreeMap::new();
        // Resolve all saved namespaces through their self-contained input copies.
        struct Bundle<'a> {
            dir: &'a Path,
            manifest: &'a Value,
        }
        impl AssetSource for Bundle<'_> {
            fn read(
                &self,
                ns: &str,
                path: &str,
                maximum: usize,
            ) -> ddi_danoni_export::Result<Vec<u8>> {
                let key = format!("{ns}:{path}");
                let p = self.manifest["inputArtifacts"][&key].as_str().unwrap();
                let b = fs::read(self.dir.join(p)).unwrap();
                assert!(b.len() <= maximum);
                Ok(b)
            }
        }
        let original = Bundle {
            dir: &dir,
            manifest: &w,
        };
        for (i, media) in request.media.iter_mut().enumerate() {
            let key = w["inputs"]
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, b)| b["sha256"] == media.source_audio_sha256)
                .unwrap()
                .0;
            let audio_path = w["inputArtifacts"][key].as_str().unwrap();
            let source =
                native::DirectorySource::new(BTreeMap::from([("source".into(), dir.clone())]))
                    .unwrap();
            let r = MediaRequest {
                manifest: Binding {
                    namespace: "source".into(),
                    path: "provenance/source-manifest.json".into(),
                    sha256: w["sourceManifestSha256"].as_str().unwrap().into(),
                },
                audio: Binding {
                    namespace: "source".into(),
                    path: audio_path.into(),
                    sha256: media.source_audio_sha256.clone(),
                },
                codec,
                decode: DecodeOptions::default(),
                quality: 0.5,
            };
            let prepared = audio::prepare_media(&r, &source, &mut || Ok(())).unwrap();
            let path = output.join(format!("{id}-{i}"));
            native::write_media(&prepared, &path, &mut || Ok(())).unwrap();
            let ns = format!("new-media-{i}");
            roots.insert(ns.clone(), path.clone());
            let bind = |name: &str| Binding {
                namespace: ns.clone(),
                path: name.into(),
                sha256: sha256(&fs::read(path.join(name)).unwrap()),
            };
            media.audio = bind(&format!("music.{extension}"));
            media.pcm = bind("pcm.wav");
            media.provenance = bind("manifest.json");
            media.codec = codec;
            checks.push(json!({"work":id,"media":i,"receipt":format!("{id}-{i}/manifest.json"),"sourceAudioSha256":media.source_audio_sha256,"validation":prepared.manifest["validation"]}));
        }
        struct Combined<'a> {
            original: Bundle<'a>,
            media: native::DirectorySource,
        }
        impl AssetSource for Combined<'_> {
            fn read(&self, ns: &str, path: &str, max: usize) -> ddi_danoni_export::Result<Vec<u8>> {
                if ns.starts_with("new-media-") {
                    self.media.read(ns, path, max)
                } else {
                    self.original.read(ns, path, max)
                }
            }
        }
        let source = Combined {
            original,
            media: native::DirectorySource::new(roots).unwrap(),
        };
        let prepared = prepare_work(&request, &source, &mut |_| Ok(())).unwrap();
        for (a, b) in prepared.manifest["songs"]
            .as_array()
            .unwrap()
            .iter()
            .zip(w["songs"].as_array().unwrap())
        {
            assert_eq!(a["preview"]["startSeconds"], b["preview"]["startSeconds"]);
            assert_eq!(a["charts"], b["charts"]);
            assert_eq!(a["artwork"], b["artwork"]);
        }
        let works = output.join("works");
        fs::create_dir_all(&works).unwrap();
        native::write_prepared(&prepared, &works.join(id), &mut |_| Ok(())).unwrap();
        eprintln!("codec-work: {id}");
    }
    assert_eq!(checks.len(), generation["songs"].as_u64().unwrap() as usize);
    let report = json!({"schema":"ddi-danoni-rust-codec-corpus/1","songs":checks.len(),"checks":checks,"chartDiagnosticsMatched":true,"previewStartsMatched":true,"passed":true});
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{}", report);
}

#[test]
fn native_codec_outputs_match_independent_ffmpeg_timing() {
    use ddi_danoni_export::audio::{self, DecodeOptions, Pcm16};
    let Some(root) = std::env::var_os("DDI_DANONI_AUDIO_MEDIA") else {
        eprintln!("SKIP: set DDI_DANONI_AUDIO_MEDIA to verify written codec outputs");
        return;
    };
    let Some(ffmpeg) = std::env::var_os("DDI_DANONI_FFMPEG") else {
        eprintln!("SKIP: set DDI_DANONI_FFMPEG for independent development-only decoder");
        return;
    };
    let root = Path::new(&root);
    let list = read(&root.join("manifest.json"));
    let mut checks = Vec::new();
    for row in list["checks"].as_array().unwrap() {
        let receipt_path = root.join(row["receipt"].as_str().unwrap());
        let receipt = read(&receipt_path);
        let dir = receipt_path.parent().unwrap();
        let pcm = audio::decode(
            &fs::read(dir.join("pcm.wav")).unwrap(),
            &DecodeOptions::default(),
            &mut || Ok(()),
        )
        .unwrap()
        .pcm;
        let extension = receipt["codec"].as_str().unwrap();
        let playback = audio::decode(
            &fs::read(dir.join(format!("music.{extension}"))).unwrap(),
            &DecodeOptions::default(),
            &mut || Ok(()),
        )
        .unwrap()
        .pcm;
        let (pcm, _) = audio::resample(&pcm, playback.rate, false, &mut || Ok(())).unwrap();
        let output = std::process::Command::new(&ffmpeg)
            .args(["-nostdin", "-v", "error", "-threads", "1", "-i"])
            .arg(dir.join(format!("music.{extension}")))
            .args([
                "-map_metadata",
                "-1",
                "-c:a",
                "pcm_s16le",
                "-f",
                "s16le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), pcm.samples.len() * 2);
        let independent = Pcm16 {
            rate: pcm.rate,
            channels: pcm.channels,
            samples: output
                .stdout
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b))
                .collect(),
        };
        let m = audio::origin_match(
            &pcm,
            &independent,
            0,
            Some((pcm.rate as usize * 12).min(pcm.frames())),
            &mut || Ok(()),
        )
        .unwrap();
        assert!(m.lag_samples.abs() <= 1);
        assert_eq!(playback.frames(), independent.frames());
        checks.push(json!({"work":row["work"],"musicSha256":receipt["playbackSha256"],"sampleFrames":independent.frames(),"rate":pcm.rate,"channels":pcm.channels,"origin":m}));
    }
    assert_eq!(checks.len(), list["songs"].as_u64().unwrap() as usize);
    println!(
        "{}",
        json!({"schema":"ddi-danoni-rust-codec-independent-timing/1","songs":checks.len(),"checks":checks,"passed":true,"scope":"Development-only FFmpeg decoding; not a runtime dependency"})
    );
}
