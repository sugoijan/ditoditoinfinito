//! The joined pipeline is checked through the game's parser and real scroll code.
use ddi_danoni_export::{Error, Result, assets::*, audio::Pcm16, preparation::*, work::Codec};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Default)]
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
    reads: RefCell<BTreeMap<String, usize>>,
}
impl Memory {
    fn put(&mut self, path: &str, bytes: Vec<u8>) -> Binding {
        let binding = Binding {
            namespace: "test".into(),
            path: path.into(),
            sha256: sha256(&bytes),
        };
        self.files.insert(path.into(), bytes);
        binding
    }
    fn json(&mut self, path: &str, value: Value) -> Binding {
        self.put(path, serde_json::to_vec(&value).unwrap())
    }
}
impl AssetSource for Memory {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        assert_eq!(ns, "test");
        *self.reads.borrow_mut().entry(path.into()).or_default() += 1;
        let bytes = self.files.get(path).ok_or_else(|| Error {
            code: "missing",
            message: path.into(),
        })?;
        assert!(bytes.len() <= max);
        Ok(bytes.clone())
    }
}
fn fixture(sparse: bool) -> (Memory, Request) {
    let mut memory = Memory::default();
    let rate = 8000usize;
    let samples = (0..24 * rate)
        .map(|n| {
            let phase = n % (rate / 2);
            if phase < 160 {
                (12000.0 * (phase as f64 * 0.38).sin() * (1.0 - phase as f64 / 160.0)) as i16
            } else {
                0
            }
        })
        .collect();
    let pcm = Pcm16 {
        rate: rate as u32,
        channels: 1,
        samples,
    };
    let audio = memory.put("music.wav", pcm.wav().unwrap());
    let charts=(0..2).map(|c|json!({"id":format!("chart-{c}"),"mode":"5","difficultyStars":4,
        "metadata":{"difficultyLabel":if c==0{"Original"}else{"Hard"}},
        "lanes":(0..5).map(|l| json!({"tapsSeconds":if l==0{if sparse{vec![2.0,4.0]}else{(4..42).map(|i|i as f64*0.5).collect::<Vec<_>>()}}else{vec![]},
            "holdsSeconds":if l==1{vec![[4.5,5.5]]}else{vec![]}})).collect::<Vec<_>>(),
        "speed":[{"seconds":8.0,"multiplier":1.5}]})).collect::<Vec<_>>();
    let manifest = memory.json(
        "manifest.json",
        json!({"schema":"ddi-synthetic-export/experimental-1",
        "metadata":{"title":"Synthetic click track","artist":"Test oscillator"},
        "music":{"path":audio.path,"sha256":audio.sha256},"charts":charts}),
    );
    let motion=memory.json("motion.json",json!({"schema":"ddi-reviewed-native-scroll/experimental-1",
        "sourceManifestSha256":manifest.sha256,"charts":{"chart-0":{"baseArrowHeightsPerSecond":4.0},"chart-1":{"baseArrowHeightsPerSecond":4.0}}}));
    (
        memory,
        Request {
            manifest,
            motion,
            font: None,
            options: Options {
                codec: Codec::Wav,
                ..Default::default()
            },
        },
    )
}
fn verify(result: &ddi_danoni_export::work::PreparedWork) -> usize {
    let mut endpoints = 0;
    for song in result.manifest["songs"].as_array().unwrap() {
        let file = result
            .files
            .iter()
            .find(|f| f.path == song["simfile"])
            .unwrap();
        let parsed =
            ddi_chart::formats::sm::parse_ssc(std::str::from_utf8(&file.bytes).unwrap()).unwrap();
        assert_eq!(parsed.charts.len(), 2);
        for (i, c) in parsed.charts.iter().enumerate() {
            assert_eq!(c.meter, 4);
            let row = &song["charts"][i];
            let timing = c.timing(&parsed);
            let padding = row["paddingTicks"].as_i64().unwrap();
            for e in row["endpoints"].as_array().unwrap() {
                let t = timing.seconds_at(ddi_chart::Tick(e["tick"].as_i64().unwrap() + padding));
                assert!((t - e["sourceSeconds"].as_f64().unwrap()).abs() <= 0.0090001);
                endpoints += 1;
            }
            for x in [1.0, 2.0, 3.0] {
                let opts = ddi_engine::scroll::ScrollOptions {
                    speed: ddi_engine::scroll::SpeedMod::XMod(x),
                    ..Default::default()
                };
                let time = 6.0;
                let target = timing.displayed_beat(timing.beat_at(time));
                let y = |t| {
                    ddi_engine::scroll::ScrollState::at(opts, timing, t).y_displayed(target, time)
                        as f64
                };
                let velocity = (y(time) - y(time + 0.01)) / 0.01;
                assert!(
                    (velocity - 4.0 * x).abs() < 0.001,
                    "{velocity} != {}",
                    4.0 * x
                );
            }
        }
    }
    endpoints
}
#[test]
fn fresh_fit_prepares_one_shared_song_with_exact_inventory_and_source_motion() {
    let (memory, request) = fixture(false);
    let result = prepare(&request, &memory, &mut |_| Ok(())).unwrap();
    assert_eq!(result.manifest["songs"].as_array().unwrap().len(), 1);
    assert_eq!(
        result.manifest["preparation"]["musicalQuality"],
        "provisional"
    );
    let report: Value = serde_json::from_slice(
        &result
            .files
            .iter()
            .find(|f| f.path == "provenance/preparation.json")
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(report["tempo"]["status"], "estimated");
    assert_eq!(report["tempo"]["sourceNotesModified"], false);
    assert!(verify(&result) > 70);
    assert!(
        memory.reads.borrow().values().all(|n| *n == 1),
        "{:?}",
        memory.reads
    );
    let inventory = result.manifest["artifacts"].as_array().unwrap();
    assert_eq!(inventory.len(), result.files.len());
    for f in &result.files {
        let r = inventory.iter().find(|r| r["path"] == f.path).unwrap();
        assert_eq!(r["sha256"], sha256(&f.bytes));
    }
}
#[test]
fn insufficient_heads_preserve_source_clock_and_mark_display_bpm_unknown() {
    let (memory, request) = fixture(true);
    let result = prepare(&request, &memory, &mut |_| Ok(())).unwrap();
    assert_eq!(
        result.manifest["preparation"]["tempoStatus"],
        "exact-source-fallback"
    );
    assert_eq!(result.manifest["songs"][0]["displayBpm"]["sscValue"], "*");
    assert_eq!(verify(&result), 8);
}
#[test]
fn motion_mismatch_is_rejected_before_audio_processing_and_cancellation_reaches_each_stage() {
    let (mut memory, mut request) = fixture(false);
    request.motion=memory.json("motion.json",json!({"schema":"ddi-reviewed-native-scroll/experimental-1","sourceManifestSha256":"0".repeat(64),"charts":{}}));
    let mut stages = Vec::new();
    assert!(
        prepare(&request, &memory, &mut |p| {
            stages.push(p.stage);
            Ok(())
        })
        .is_err()
    );
    assert!(!stages.contains(&Stage::Audio));
    for stop in [Stage::Audio, Stage::Tempo, Stage::Songs, Stage::Complete] {
        let (memory, request) = fixture(false);
        let e = prepare(&request, &memory, &mut |p| {
            if p.stage == stop {
                Err(Error {
                    code: "test-cancel",
                    message: "cancel".into(),
                })
            } else {
                Ok(())
            }
        })
        .err()
        .unwrap();
        assert_eq!(e.code, "test-cancel");
    }
}

#[cfg(feature = "opus")]
#[test]
fn default_opus_changes_playback_without_changing_analysis_or_charts() {
    let (memory, mut request) = fixture(false);
    let wav = prepare(&request, &memory, &mut |_| Ok(())).unwrap();
    request.options = Options::default();
    let opus = prepare(&request, &memory, &mut |_| Ok(())).unwrap();
    assert_eq!(verify(&opus), verify(&wav));
    let a = &wav.manifest["songs"][0];
    let b = &opus.manifest["songs"][0];
    for field in [
        "charts",
        "preview",
        "timingBpm",
        "timingOffsetSeconds",
        "pcmSha256",
        "pcmFrames",
        "pcmSampleRate",
    ] {
        assert_eq!(a[field], b[field], "{field}");
    }
    let music = opus
        .files
        .iter()
        .find(|f| Some(f.path.as_str()) == b["music"].as_str())
        .unwrap();
    assert!(music.path.ends_with("music.opus"));
    let decoded =
        ddi_danoni_export::audio::decode(&music.bytes, &Default::default(), &mut || Ok(()))
            .unwrap();
    assert_eq!(decoded.pcm.rate, 48000);
    assert_eq!(decoded.pcm.frames(), 24 * 48000);
    assert_eq!(b["pcmSampleRate"], 8000);
}
