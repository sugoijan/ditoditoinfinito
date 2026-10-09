//! Read-only export checks using the game's real parsers. No work JS runs here.
use ddi_chart::{
    NoteKind,
    formats::{
        danoni::{self, Dos, keys},
        sm,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read input")).expect("JSON input")
}
fn strv(v: &Value) -> &str {
    v.as_str().expect("string")
}
fn num(v: &Value) -> f64 {
    v.as_f64().expect("number")
}
fn notes(chart: &ddi_chart::Chart, song: &ddi_chart::Song) -> Vec<(u8, bool, f64, f64)> {
    let timing = chart.timing(song);
    let mut out: Vec<_> = chart
        .notes
        .iter()
        .map(|n| {
            let t = timing.seconds_at(n.tick);
            match n.kind {
                NoteKind::Tap => (n.lane, false, t, t),
                NoteKind::HoldHead { end } => (n.lane, true, t, timing.seconds_at(end)),
                _ => panic!("unexpected note kind"),
            }
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.total_cmp(&b.2)));
    out
}
fn normalized(source: &Path, prepared: &Path, baseline: Option<&Path>) -> Value {
    let original = read(source);
    let m = read(&prepared.join("manifest.json"));
    let old = baseline.map(read);
    let mut count = 0;
    let mut taps = 0;
    let mut holds = 0;
    let mut max_error: f64 = 0.0;
    for entry in m["songs"].as_array().unwrap() {
        let song =
            sm::parse_ssc(&fs::read_to_string(prepared.join(strv(&entry["simfile"]))).unwrap())
                .unwrap();
        assert_eq!(song.title, strv(&entry["title"]), "song title changed");
        assert!((song.preview_start - num(&entry["preview"]["startSeconds"])).abs() < 1e-6);
        assert!((song.preview_length - num(&entry["preview"]["lengthSeconds"])).abs() < 1e-6);
        assert!(song.preview_start >= 0.0 && song.preview_length > 0.0);
        assert!(
            song.preview_start + song.preview_length
                <= num(&entry["pcmFrames"]) / num(&entry["pcmSampleRate"]) + 1e-6
        );
        assert_eq!(song.charts.len(), entry["charts"].as_array().unwrap().len());
        let old_song = old.as_ref().map(|old| {
            let row = old["songs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == entry["id"])
                .unwrap();
            assert_eq!(
                row["musicSha256"], entry["musicSha256"],
                "accepted audio changed"
            );
            sm::parse_ssc(
                &fs::read_to_string(
                    baseline
                        .unwrap()
                        .parent()
                        .unwrap()
                        .join(strv(&row["simfile"])),
                )
                .unwrap(),
            )
            .unwrap()
        });
        for (index, c) in entry["charts"].as_array().unwrap().iter().enumerate() {
            let src = original["charts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == c["id"])
                .unwrap();
            let chart = &song.charts[index];
            let timing = chart.timing(&song);
            if let Some(label) = src["metadata"]["difficultyLabel"].as_str() {
                assert_eq!(song.charts[index].name, label, "difficulty label changed");
            }
            assert_eq!(chart.layout, format!("danoni-{}", strv(&src["mode"])));
            assert!(ddi_chart::PLAYED_DANONI_MODES.contains(&strv(&src["mode"])));
            assert_eq!(
                chart.meter,
                c["difficultyStars"].as_u64().unwrap_or(0) as u32
            );
            assert_eq!(
                format!("{:?}", chart.difficulty),
                strv(&c["difficultyClassification"])
            );
            let budget = c["musicalQuantization"]["grid"]["maximumErrorMs"]
                .as_f64()
                .unwrap_or(0.0)
                / 1000.0
                + 1e-10;
            let mut wanted = vec![];
            for (lane, l) in src["lanes"].as_array().unwrap().iter().enumerate() {
                for t in l["tapsSeconds"].as_array().unwrap() {
                    wanted.push((lane as u8, false, num(t), num(t)));
                    taps += 1;
                }
                for h in l["holdsSeconds"].as_array().unwrap() {
                    wanted.push((lane as u8, true, num(&h[0]), num(&h[1])));
                    holds += 1;
                }
            }
            wanted.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.total_cmp(&b.2)));
            let got = notes(chart, &song);
            assert_eq!(wanted.len(), got.len());
            for (a, b) in wanted.iter().zip(&got) {
                assert_eq!((a.0, a.1), (b.0, b.1));
                let error = (a.2 - b.2).abs().max((a.3 - b.3).abs());
                assert!(error <= budget);
                max_error = max_error.max(error);
            }
            if let Some(old_song) = &old_song {
                assert_eq!(old_song.charts[index].notes.len(), chart.notes.len());
                for (a, b) in notes(&old_song.charts[index], old_song).iter().zip(&got) {
                    assert_eq!((a.0, a.1), (b.0, b.1));
                    assert!((a.2 - b.2).abs() < 1e-10 && (a.3 - b.3).abs() < 1e-10);
                }
                for (a, b) in old_song.charts[index].notes.iter().zip(&chart.notes) {
                    assert_eq!(a.tick.quantization(), b.tick.quantization());
                }
            }
            let speed = src["speed"].as_array().cloned().unwrap_or_default();
            let native = c["nativeScroll"].is_object();
            let extra = usize::from(native);
            let base = c["nativeScroll"]["sscBaseRatio"].as_f64().unwrap_or(1.0);
            assert_eq!(timing.scrolls.len(), speed.len() + extra);
            if native {
                assert_eq!(timing.scrolls[0].tick, ddi_chart::Tick(0));
                assert!((timing.scrolls[0].ratio - base).abs() < 1e-12);
                // Use a small interval before the first author change.
                let first = &chart.notes[0];
                let sec = timing.seconds_at(first.tick);
                let dt = 0.001;
                for x in [1.0, 2.0, 3.0] {
                    let opt = ddi_engine::scroll::ScrollOptions {
                        speed: ddi_engine::scroll::SpeedMod::XMod(x),
                        ..Default::default()
                    };
                    let velocity = (ddi_engine::scroll::note_y(opt, timing, first.tick, sec - dt)
                        - ddi_engine::scroll::note_y(opt, timing, first.tick, sec))
                        as f64
                        / dt;
                    assert!(
                        (velocity - num(&c["nativeScroll"]["baseArrowHeightsPerSecond"]) * x).abs()
                            < 0.001
                    );
                }
            }
            for (e, s) in speed.iter().zip(timing.scrolls.iter().skip(extra)) {
                assert!((timing.seconds_at(s.tick) - num(&e["seconds"])).abs() <= budget);
                assert!((s.ratio - num(&e["multiplier"]) * base).abs() < 1e-12);
            }
            count += 1;
        }
    }
    json!({"format":"ssc","charts":count,"taps":taps,"holds":holds,"maximumErrorMs":max_error*1000.0,"baselineMatched":baseline.is_some(),"passed":true})
}
fn cw(path: &Path) -> Value {
    let m = read(path);
    let root = path.parent().unwrap();
    let mut import_data: Option<danoni::Import> = None;
    let mut first_dos: Option<Dos> = None;
    let mut taps = 0;
    let mut holds = 0;
    let mut count = 0;
    for c in m["charts"].as_array().unwrap() {
        let eff = read(&root.join(strv(&c["paths"]["effectiveDos"])));
        let mut dos = Dos::default();
        for (k, v) in eff["fields"].as_object().unwrap() {
            dos.set(k, strv(v).to_string());
        }
        let r = read(&root.join(strv(&c["paths"]["reference"])));
        let mode = keys::canonical_label(strv(&c["keyMode"]));
        assert!(ddi_chart::PLAYED_DANONI_MODES.contains(&mode));
        let (def, custom) = keys::key_def(&dos, mode).unwrap().unwrap();
        assert!(!custom, "custom/redefined keys are outside scope");
        let adj = num(&r["timing"]["intAdjustment"]) as i64;
        let pre = num(&r["timing"]["preblankFrame"]) as i64;
        assert_eq!(
            num(&r["timing"]["headerAdjustment"]).floor() as i64,
            adj - pre
        );
        let lanes = danoni::lane_frames(&dos, &def.chara, strv(&c["sourceSuffix"]));
        assert_eq!(lanes.len(), r["lanes"].as_array().unwrap().len());
        for (i, (o, l)) in lanes.iter().zip(r["lanes"].as_array().unwrap()).enumerate() {
            assert_eq!(def.chara[i], strv(&l["chara"]));
            let ints = |v: &Value| {
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|n| num(n) as i64)
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                o.taps.iter().map(|f| f + adj).collect::<Vec<_>>(),
                ints(&l["taps"])
            );
            assert_eq!(
                o.holds.iter().map(|f| f + adj).collect::<Vec<_>>(),
                ints(&l["holdEndpoints"])
            );
            taps += o.taps.len();
            holds += o.holds.len() / 2;
        }
        for (name, events) in [
            ("speed", danoni::speed_pairs(&dos, strv(&c["sourceSuffix"]))),
            ("boost", danoni::boost_pairs(&dos, strv(&c["sourceSuffix"]))),
        ] {
            let expected: Vec<_> = r[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| (num(&e["frame"]) as i64, num(&e["multiplier"])))
                .collect();
            assert_eq!(
                events
                    .into_iter()
                    .map(|(f, m)| (f + adj, m))
                    .collect::<Vec<_>>(),
                expected
            );
        }
        let import = danoni::import(&dos).unwrap();
        let id = c["chartId"].as_u64().unwrap() as usize;
        assert!(import.charts[id].is_some());
        if let Some(first) = &import_data {
            let (si, ci) = import.charts[id].unwrap();
            let (first_si, first_ci) = first.charts[id].unwrap();
            assert_eq!(
                import.music[si], first.music[first_si],
                "chart/music grouping changed between captures"
            );
            assert_eq!(
                notes(&import.songs[si].charts[ci], &import.songs[si]),
                notes(
                    &first.songs[first_si].charts[first_ci],
                    &first.songs[first_si]
                ),
                "the first effective dos must contain every selected chart"
            );
            for events in [danoni::speed_pairs, danoni::boost_pairs] {
                assert_eq!(
                    events(&dos, strv(&c["sourceSuffix"])),
                    events(first_dos.as_ref().unwrap(), strv(&c["sourceSuffix"]))
                );
            }
        } else {
            first_dos = Some(dos);
            import_data = Some(import);
        }
        count += 1;
    }
    let import = import_data.unwrap();
    let actual: Vec<_> = import
        .charts
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.map(|_| i))
        .collect();
    let expected: Vec<_> = m["charts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["chartId"].as_u64().unwrap() as usize)
        .collect();
    assert_eq!(
        actual, expected,
        "uncaptured playable charts would leak into import"
    );
    json!({"format":"native-cw-dump","charts":count,"songs":import.songs.len(),"taps":taps,"holds":holds,"passed":true})
}
fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("generation directory"));
    let m = read(&root.join("index.json"));
    assert_eq!(strv(&m["schema"]), "ddi-danoni-export-generation/1");
    for w in m["works"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["status"] == "built")
    {
        let result = if w["kind"] == "normalized" {
            normalized(
                Path::new(strv(&w["source"])),
                &root.join(strv(&w["prepared"])),
                w["baseline"].as_str().map(Path::new),
            )
        } else {
            cw(&root.join(strv(&w["dump"])))
        };
        println!("{}", json!({"work":w["id"],"validation":result}));
    }
}
