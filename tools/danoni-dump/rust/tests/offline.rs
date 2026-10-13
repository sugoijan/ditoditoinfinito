use ddi_danoni_export::{Error, Result, assets::*, model::*, preview, reviews, work::*};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};

#[derive(Default)]
struct Memory {
    files: BTreeMap<(String, String), Vec<u8>>,
    reads: RefCell<BTreeMap<(String, String), usize>>,
}
impl Memory {
    fn put(&mut self, ns: &str, path: &str, body: Vec<u8>) -> Binding {
        let hash = sha256(&body);
        self.files.insert((ns.into(), path.into()), body);
        Binding {
            namespace: ns.into(),
            path: path.into(),
            sha256: hash,
        }
    }
    fn put_json(&mut self, ns: &str, path: &str, value: Value) -> Binding {
        self.put(ns, path, serde_json::to_vec(&value).unwrap())
    }
}
impl AssetSource for Memory {
    fn read(&self, ns: &str, path: &str, limit: usize) -> Result<Vec<u8>> {
        let key = (ns.to_owned(), path.to_owned());
        *self.reads.borrow_mut().entry(key.clone()).or_default() += 1;
        let value = self.files.get(&key).ok_or_else(|| Error {
            code: "missing-test-asset",
            message: path.into(),
        })?;
        if value.len() > limit {
            return Err(Error {
                code: "asset-limit",
                message: path.into(),
            });
        }
        Ok(value.clone())
    }
}
fn wav(seconds: u32, stereo: bool, quiet_until: u32) -> Vec<u8> {
    let rate = 8000u32;
    let channels = if stereo { 2u16 } else { 1 };
    let mut data = Vec::new();
    for frame in 0..rate * seconds {
        let value = if frame < rate * quiet_until {
            100i16
        } else {
            1600
        };
        data.extend(value.to_le_bytes());
        if stereo {
            data.extend((-value).to_le_bytes());
        }
    }
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((36u32 + data.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(channels.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * u32::from(channels) * 2).to_le_bytes());
    out.extend((channels * 2).to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    out
}
fn chart(id: &str, label: &str, stars: Option<u16>) -> Value {
    json!({"id":id,"mode":"5","difficultyStars":stars,"metadata":{"difficultyLabel":label},"lanes":(0..5).map(|i|json!({"tapsSeconds":if i==0{vec![0.5,4.0,4.5,5.0,5.5]}else{vec![]},"holdsSeconds":[]})).collect::<Vec<_>>()})
}
fn fixture() -> (Memory, WorkRequest) {
    let mut store = Memory::default();
    let pcm = store.put("work", "music.wav", wav(10, true, 4));
    store.put_json("work","reference.json",json!({"metadata":{"artist":"Reference artist","title":"Reference title"},"nativeFrames":[30,240]}));
    let manifest=store.put_json("work","manifest.json",json!({"schema":"ddi-normalized-work/experimental-1","metadata":{"title":"Default title","artist":"Default artist"},"music":{"path":pcm.path,"sha256":pcm.sha256},"evidence":{"reference":"reference.json"},"charts":[chart("a","Normal",Some(4)),chart("b","Original",Some(4))]}));
    let proof=store.put_json("media","receipt.json",json!({"schema":"ddi-playable-pack/experimental-1","sourceManifestSha256":manifest.sha256,"codec":"wav","songs":[{"sourceAudioSha256":pcm.sha256,"musicSha256":pcm.sha256,"pcmSampleRate":8000,"pcmChannels":2,"pcmFrames":80000}]}));
    let media = SuppliedMedia {
        source_audio_sha256: pcm.sha256.clone(),
        audio: pcm.clone(),
        pcm,
        codec: Codec::Wav,
        provenance: proof,
    };
    (
        store,
        WorkRequest {
            manifest,
            reviews: ReviewBindings::default(),
            media: vec![media],
            options: WorkOptions::default(),
        },
    )
}
fn prepare(store: &Memory, request: &WorkRequest) -> Result<PreparedWork> {
    prepare_work(request, store, &mut |_| Ok(()))
}

#[test]
fn complete_song_merges_metadata_groups_audio_and_preserves_duplicate_labels() {
    let (store, request) = fixture();
    let result = prepare(&store, &request).unwrap();
    let song = &result.manifest["songs"][0];
    assert_eq!(result.manifest["songs"].as_array().unwrap().len(), 1);
    assert_eq!(song["title"], "Reference title");
    assert_eq!(song["artist"], "Reference artist");
    assert_eq!(song["preview"]["anchorSeconds"], 4.0);
    assert_eq!(song["preview"]["startSeconds"], 3.92);
    assert!(
        song["charts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["preservedDuplicateLabel"] == true
                && c["difficultyClassification"] == "Medium")
    );
    let ssc = result
        .files
        .iter()
        .find(|a| a.path == "songs/a/song.ssc")
        .unwrap();
    let parsed =
        ddi_chart::formats::sm::parse_ssc(std::str::from_utf8(&ssc.bytes).unwrap()).unwrap();
    assert_eq!(parsed.charts.len(), 2);
    assert_eq!(parsed.charts[0].name, "Normal");
    assert_eq!(parsed.charts[1].name, "Original");
    assert!(
        store.reads.borrow().values().all(|n| *n == 1),
        "each input should be read once per work"
    );
    for (key, binding) in result.manifest["inputs"].as_object().unwrap() {
        let path = result.manifest["inputArtifacts"][key].as_str().unwrap();
        let file = result.files.iter().find(|a| a.path == path).unwrap();
        assert_eq!(sha256(&file.bytes), binding["sha256"]);
    }
}
#[test]
fn modified_assets_and_cross_work_media_reject() {
    let (mut store, request) = fixture();
    store
        .files
        .get_mut(&("work".into(), "music.wav".into()))
        .unwrap()[44] ^= 1;
    assert_eq!(
        prepare(&store, &request).err().unwrap().code,
        "changed-asset"
    );
    let (mut store, mut request) = fixture();
    request.media[0].provenance = store.put_json(
        "media",
        "wrong.json",
        json!({"schema":"ddi-playable-pack/experimental-1","sourceManifestSha256":"0".repeat(64)}),
    );
    assert!(prepare(&store, &request).is_err());
    request.media.clear();
    assert!(prepare(&store, &request).is_err());
}
#[test]
fn grid_review_must_cover_and_bind_exact_source() {
    let (mut store, mut request) = fixture();
    let grid = json!({"bpm":120,"beatZeroSeconds":0,"subdivisionsPerBeat":2});
    for (hash, rows) in [
        ("0".repeat(64), json!({"a":grid,"b":grid})),
        (request.manifest.sha256.clone(), json!({"a":grid})),
    ] {
        request.reviews.grid=Some(store.put_json("grid","grid.json",json!({"schema":"ddi-reviewed-musical-grid/experimental-1","sourceManifestSha256":hash,"charts":rows})));
        assert!(prepare(&store, &request).is_err());
    }
}
#[test]
fn cancellation_does_not_publish_or_mutate_source() {
    let (store, request) = fixture();
    let before = store.files.clone();
    let error = prepare_work(&request, &store, &mut |event| {
        if matches!(event.stage, Stage::PrepareAudio) {
            Err(Error {
                code: "cancelled",
                message: "test".into(),
            })
        } else {
            Ok(())
        }
    })
    .err()
    .unwrap();
    assert_eq!(error.code, "cancelled");
    assert_eq!(store.files, before);
}
#[test]
fn preview_handles_sparse_chords_and_stereo_without_channel_cancellation() {
    let envelope = preview::pcm_envelope(&wav(10, true, 4), &mut || Ok(())).unwrap();
    let mut chart: Chart = serde_json::from_value(chart("a", "Normal", None)).unwrap();
    for lane in &mut chart.lanes {
        lane.taps_seconds = vec![4.0];
    }
    let selected = preview::choose_preview(
        &envelope,
        &[&chart],
        &preview::PreviewSettings::default(),
        &mut || Ok(()),
    )
    .unwrap();
    assert_eq!(selected["status"], "audio-note-fallback");
    assert_eq!(selected["selectedAudioSupport"]["supported"], true);
    assert_eq!(selected["sparseChartIds"], json!(["a"]));
    let mut invalid = envelope;
    invalid.energy.clear();
    assert!(preview::choose_preview(&invalid, &[], &Default::default(), &mut || Ok(())).is_err());
}
#[test]
fn malformed_pcm_and_invalid_settings_reject() {
    for at in [0, 4, 12, 20, 22, 32, 34, 40] {
        let mut body = wav(2, false, 0);
        body[at] ^= 0x40;
        assert!(
            preview::pcm_envelope(&body, &mut || Ok(())).is_err(),
            "corruption at {at}"
        );
    }
    let envelope = preview::pcm_envelope(&wav(2, false, 0), &mut || Ok(())).unwrap();
    let settings = preview::PreviewSettings {
        quiet_ratio: f64::NAN,
        ..Default::default()
    };
    assert!(preview::choose_preview(&envelope, &[], &settings, &mut || Ok(())).is_err());
}
#[test]
fn passive_html_text_and_numeric_notation() {
    let html=b"<title>ignore 99</title><p>Level <span>6</span>&nbsp;stars</p><script>const x = '<p>88</p>';</script><style>p{}</style><textarea>77</textarea>";
    assert_eq!(
        reviews::visible_text(html, "utf-8").unwrap(),
        "Level 6 stars"
    );
    assert_eq!(
        reviews::visible_text(b"\xef\xbb\xbf<p>Level 6</p>", "utf-8-sig").unwrap(),
        "Level 6"
    );
    assert_eq!(
        reviews::rating("star-number", "★×６").unwrap()["maximum"],
        6
    );
    assert_eq!(
        reviews::rating("filled-stars", "★★★☆☆").unwrap()["maximum"],
        3
    );
    assert_eq!(
        reviews::rating("star-range", "★★→★★★★").unwrap()["minimum"],
        2
    );
    assert!(reviews::rating("integer", "Hard 6").is_err());
    assert!(reviews::rating("star-range", "★★★★→★★").is_err());
}
#[test]
fn rating_requires_unique_visible_evidence_and_rejects_uncertain_selection() {
    let (mut store, mut request) = fixture();
    // Runtime rating is known (4); only matching source evidence can supplement it.
    let html = store.put("ratings", "page.html", b"<p>5 Normal 4</p>".to_vec());
    let row = |token: &str| json!({"chartIdentity":{"mode":"5","label":"Normal"},"selectedRating":0,"selectionReason":"Publisher rating","ratings":[{"notation":"integer","token":token,"excerpt":"5 Normal 4","encoding":"utf-8","scale":"Publisher level","authority":"Original fixture","evidence":{"path":html.path,"sha256":html.sha256,"url":"https://example.org/song","retrievedAt":"2026-10-13"}}]});
    request.reviews.ratings=Some(store.put_json("ratings","review.json",json!({"schema":"ddi-reviewed-source-ratings/1","sourceManifestSha256":request.manifest.sha256,"charts":{"a":row("4")}})));
    let result = prepare(&store, &request).unwrap();
    assert_eq!(
        result.manifest["songs"][0]["charts"][0]["sourceRating"]["meter"],
        4
    );
    store.files.insert(
        ("ratings".into(), "page.html".into()),
        b"<p>5 Normal 4</p><p>5 Normal 4</p>".to_vec(),
    );
    let changed = sha256(&store.files[&("ratings".into(), "page.html".into())]);
    let mut review: Value =
        serde_json::from_slice(&store.files[&("ratings".into(), "review.json".into())]).unwrap();
    review["charts"]["a"]["ratings"][0]["evidence"]["sha256"] = json!(changed);
    request.reviews.ratings = Some(store.put_json("ratings", "review.json", review));
    assert!(prepare(&store, &request).is_err());
}
#[test]
fn contained_names_and_display_bpm_are_checked() {
    for path in [
        "../secret",
        "/absolute",
        "x/../secret",
        "x\\secret",
        "C:secret",
        "x//y",
        "",
    ] {
        assert!(relative_path(path).is_err());
    }
    assert_eq!(
        display_bpm("110:220", "explicit review input").unwrap()["kind"],
        "range"
    );
    assert_eq!(
        display_bpm("*", "explicit review input").unwrap()["kind"],
        "unknown"
    );
    for raw in ["0", "nan", "inf", "220:110", "120:130:140"] {
        assert!(display_bpm(raw, "explicit review input").is_err());
    }
}

#[cfg(feature = "native")]
mod native_tests {
    use super::*;
    use ddi_danoni_export::native;
    use std::{fs, path::PathBuf};
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn temp() -> Temp {
        // Wall-clock resolution alone can collide between parallel tests.
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ddi-danoni-rust-{}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&p).unwrap();
        Temp(p.canonicalize().unwrap())
    }
    #[test]
    fn fresh_output_has_atomic_readiness_and_cancelled_output_is_removed() {
        let (store, request) = fixture();
        let prepared = prepare(&store, &request).unwrap();
        let t = temp();
        let output = t.0.join("prepared");
        native::write_prepared(&prepared, &output, &mut |_| Ok(())).unwrap();
        assert!(output.join("manifest.json").is_file());
        assert!(!output.join(".manifest.tmp").exists());
        let mut changed = prepare(&store, &request).unwrap();
        changed.files[0].bytes = std::sync::Arc::new(b"changed".to_vec());
        assert!(native::write_prepared(&changed, &t.0.join("tampered"), &mut |_| Ok(())).is_err());
        assert!(!t.0.join("tampered").exists());
        let before = fs::read(output.join("manifest.json")).unwrap();
        assert!(native::write_prepared(&prepared, &output, &mut |_| Ok(())).is_err());
        assert_eq!(fs::read(output.join("manifest.json")).unwrap(), before);
        let cancelled = t.0.join("cancelled");
        assert!(
            native::write_prepared(&prepared, &cancelled, &mut |_| Err(Error {
                code: "cancelled",
                message: "test".into()
            }))
            .is_err()
        );
        assert!(!cancelled.exists());
    }
    #[test]
    fn directory_source_rejects_escape_symlinks_and_oversized_files() {
        let t = temp();
        fs::write(t.0.join("file"), b"original").unwrap();
        let source =
            native::DirectorySource::new(BTreeMap::from([("test".into(), t.0.clone())])).unwrap();
        assert_eq!(source.read("test", "file", 8).unwrap(), b"original");
        assert!(source.read("test", "file", 7).is_err());
        assert!(source.read("test", "../secret", 100).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(t.0.join("file"), t.0.join("link")).unwrap();
            assert!(source.read("test", "link", 100).is_err());
        }
        fs::create_dir(t.0.join(".git")).unwrap();
        assert!(
            native::DirectorySource::new(BTreeMap::from([("test".into(), t.0.clone())])).is_err()
        );
    }
    fn packaged(t: &Temp, name: &str) -> PathBuf {
        use ddi_danoni_export::package::{Collection, PackageJob, package_generation};
        let (store, request) = fixture();
        let prepared = prepare(&store, &request).unwrap();
        let source = t.0.join(format!("source-{name}"));
        fs::create_dir(&source).unwrap();
        fs::create_dir(source.join("works")).unwrap();
        native::write_prepared(&prepared, &source.join("works/test"), &mut |_| Ok(())).unwrap();
        fs::write(source.join("manifest.json"),serde_json::to_vec(&json!({"schema":"ddi-danoni-rust-offline-generation/1","works":[{"id":"test","collection":"singles","manifest":"works/test/manifest.json","sourceManifestSha256":request.manifest.sha256}],"songs":1,"charts":2,"testGeneration":name})).unwrap()).unwrap();
        let font = fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf"
        ))
        .unwrap();
        fs::write(t.0.join("font.ttf"), &font).unwrap();
        let job = PackageJob {
            schema: "ddi-danoni-package-job/1".into(),
            generation: source,
            roots: BTreeMap::from([("font".into(), t.0.clone())]),
            font: Binding {
                namespace: "font".into(),
                path: "font.ttf".into(),
                sha256: sha256(&font),
            },
            collections: vec![Collection {
                id: "singles".into(),
                title: "DanOni Singles".into(),
                artist: "Source authors".into(),
            }],
        };
        let output = t.0.join(name);
        package_generation(&job, &output, &mut |_| Ok(())).unwrap();
        output
    }
    #[test]
    fn collection_inventory_parser_and_zip_are_checked() {
        let t = temp();
        let p = packaged(&t, "pack");
        let doc = ddi_danoni_export::package::verify_generation(&p, &mut |_| Ok(())).unwrap();
        assert_eq!(doc["songs"], 1);
        assert_eq!(doc["charts"], 2);
        fs::write(p.join("extra.txt"), b"extra").unwrap();
        assert!(ddi_danoni_export::package::verify_generation(&p, &mut |_| Ok(())).is_err());
        fs::remove_file(p.join("extra.txt")).unwrap();
        fs::write(
            p.join("singles/files/DanOni Singles/test--a/song.ssc"),
            b"#TITLE:changed;",
        )
        .unwrap();
        assert!(ddi_danoni_export::package::verify_generation(&p, &mut |_| Ok(())).is_err());
    }
    #[test]
    fn publication_archives_and_recovery_is_idempotent() {
        let t = temp();
        let old = packaged(&t, "published");
        let old_bytes = fs::read(old.join("manifest.json")).unwrap();
        let stage = packaged(&t, "staged");
        let history = t.0.join("history");
        fs::create_dir(&history).unwrap();
        let receipt =
            ddi_danoni_export::transaction::promote(&stage, &old, &history, &mut |_| Ok(()))
                .unwrap();
        assert_eq!(receipt["status"], "committed");
        assert!(!stage.exists());
        let archive = PathBuf::from(receipt["archive"].as_str().unwrap());
        assert_eq!(fs::read(archive.join("manifest.json")).unwrap(), old_bytes);
        let journal = PathBuf::from(receipt["journal"].as_str().unwrap());
        assert_eq!(
            receipt,
            ddi_danoni_export::transaction::recover(&journal, &mut |_| Ok(())).unwrap()
        );
    }
    #[test]
    fn cancellation_after_archival_restores_old_tree() {
        let t = temp();
        let old = packaged(&t, "published");
        let bytes = fs::read(old.join("manifest.json")).unwrap();
        let stage = packaged(&t, "staged");
        let history = t.0.join("history");
        fs::create_dir(&history).unwrap();
        let r = ddi_danoni_export::transaction::promote(&stage, &old, &history, &mut |p| {
            if matches!(p.stage, Stage::Assemble) && p.completed == 1 && p.total == 2 {
                Err(Error {
                    code: "cancelled",
                    message: "test".into(),
                })
            } else {
                Ok(())
            }
        });
        assert_eq!(r.unwrap_err().code, "cancelled");
        assert_eq!(fs::read(old.join("manifest.json")).unwrap(), bytes);
        assert!(stage.exists());
        let journal = fs::read_dir(&history)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|s| s == "json"))
            .unwrap();
        assert_eq!(
            ddi_danoni_export::transaction::recover(&journal, &mut |_| Ok(())).unwrap()["status"],
            "rolled-back"
        );
    }
    #[test]
    fn recovery_uses_directory_identity_when_phase_is_stale() {
        let t = temp();
        let old = packaged(&t, "published");
        let stage = packaged(&t, "staged");
        let history = t.0.join("history");
        fs::create_dir(&history).unwrap();
        let archive = history.join("archive-test");
        let hash = |p: &PathBuf| sha256(&fs::read(p.join("manifest.json")).unwrap());
        let j = json!({"schema":"ddi-danoni-publication/1","phase":"prepared","stage":stage,"destination":old,"archive":archive,"newManifestSha256":hash(&stage),"oldManifestSha256":hash(&old)});
        let path = history.join("transaction-test.json");
        fs::write(&path, serde_json::to_vec(&j).unwrap()).unwrap();
        fs::rename(&old, &archive).unwrap();
        assert_eq!(
            ddi_danoni_export::transaction::recover(&path, &mut |_| Ok(())).unwrap()["status"],
            "rolled-back"
        );
        assert!(old.exists());
        assert!(!archive.exists());
        fs::write(&path, serde_json::to_vec(&j).unwrap()).unwrap();
        fs::rename(&old, &archive).unwrap();
        fs::rename(&stage, &old).unwrap();
        assert_eq!(
            ddi_danoni_export::transaction::recover(&path, &mut |_| Ok(())).unwrap()["status"],
            "committed"
        );
    }
    #[test]
    fn first_publication_and_partial_journal_recovery() {
        let t = temp();
        let stage = packaged(&t, "staged");
        let dest = t.0.join("published");
        let history = t.0.join("history");
        fs::create_dir(&history).unwrap();
        let r = ddi_danoni_export::transaction::promote(&stage, &dest, &history, &mut |_| Ok(()))
            .unwrap();
        assert_eq!(r["status"], "committed");
        assert!(r["archive"].is_null());
        let path = PathBuf::from(r["journal"].as_str().unwrap());
        fs::write(path.with_extension("tmp"), b"{ interrupted").unwrap();
        assert_eq!(
            ddi_danoni_export::transaction::recover(&path, &mut |_| Ok(())).unwrap()["status"],
            "committed"
        );
        assert!(!path.with_extension("tmp").exists());
        assert!(
            fs::read_dir(history)
                .unwrap()
                .map(|e| e.unwrap().path())
                .any(|p| p.to_string_lossy().contains(".interrupted-"))
        );
    }
    #[test]
    fn concurrent_publishers_cannot_replace_the_same_destination() {
        let t = temp();
        let stage = packaged(&t, "staged");
        let dest = t.0.join("published");
        let history = t.0.join("history");
        fs::create_dir(&history).unwrap();
        let lock = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(t.0.join(".published.publication.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        assert!(
            ddi_danoni_export::transaction::promote(&stage, &dest, &history, &mut |_| Ok(()))
                .is_err()
        );
        assert!(stage.exists());
        assert!(!dest.exists());
    }

    #[test]
    fn unknown_tree_and_overlapping_locations_are_never_removed() {
        let t = temp();
        let stage = packaged(&t, "staged");
        let dest = t.0.join("unknown");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("keep"), b"keep").unwrap();
        let history = t.0.join("history");
        fs::create_dir(&history).unwrap();
        assert!(
            ddi_danoni_export::transaction::promote(&stage, &dest, &history, &mut |_| Ok(()))
                .is_err()
        );
        assert_eq!(fs::read(dest.join("keep")).unwrap(), b"keep");
        assert!(
            ddi_danoni_export::transaction::promote(
                &stage,
                &stage.join("inner"),
                &history,
                &mut |_| Ok(())
            )
            .is_err()
        );
        assert!(stage.exists());
    }
}

#[test]
fn neutral_media_receipt_can_be_produced_without_legacy_tools() {
    let (mut store, mut request) = fixture();
    let media = &request.media[0];
    let value = json!({"schema":"ddi-danoni-prepared-media/1","sourceManifestSha256":request.manifest.sha256,"sourceAudioSha256":media.source_audio_sha256,"playbackSha256":media.audio.sha256,"pcmSha256":media.pcm.sha256,"sampleRate":8000,"channels":2,"sampleFrames":80000,"codec":"wav","producer":{"name":"original synthetic fixture"}});
    request.media[0].provenance = store.put_json("media", "neutral.json", value.clone());
    assert!(prepare(&store, &request).is_ok());
    let mut changed = value;
    changed["pcmSha256"] = json!("0".repeat(64));
    request.media[0].provenance = store.put_json("media", "neutral.json", changed);
    assert!(prepare(&store, &request).is_err());
}
