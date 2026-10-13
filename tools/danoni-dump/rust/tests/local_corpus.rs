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
