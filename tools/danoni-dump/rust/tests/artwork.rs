use ddi_danoni_export::{Error, Result, artwork::*, assets::*};
use serde_json::{Value, json};
use std::collections::BTreeMap;
struct Memory(BTreeMap<String, Vec<u8>>);
impl AssetSource for Memory {
    fn read(&self, _: &str, p: &str, m: usize) -> Result<Vec<u8>> {
        let b = &self.0[p];
        if b.len() > m {
            Err(Error {
                code: "limit",
                message: p.into(),
            })
        } else {
            Ok(b.clone())
        }
    }
}
fn binding(p: &str, b: &[u8]) -> Binding {
    Binding {
        namespace: "test".into(),
        path: p.into(),
        sha256: sha256(b),
    }
}
fn font() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf"
    ))
    .unwrap()
}
#[test]
fn title_cards_escape_markup_and_are_reproducible() {
    let font = font();
    let store = Memory(BTreeMap::from([("font".into(), font.clone())]));
    let r = TitleRequest {
        title: "Song <&> \"Title\"".into(),
        artist: "Artist & arranger".into(),
        source_manifest_sha256: "a".repeat(64),
        font: binding("font", &font),
        roles: vec![Role::Banner, Role::Cover, Role::Pack],
    };
    let a = title_cards(&r, &store).unwrap();
    let b = title_cards(&r, &store).unwrap();
    assert_eq!(a.manifest, b.manifest);
    assert_eq!(a.files.len(), 6);
    for f in a.files.iter().filter(|f| f.path.ends_with(".svg")) {
        let s = std::str::from_utf8(&f.bytes).unwrap();
        assert!(s.contains("&lt;&amp;&gt;"));
        assert!(!s.contains("<image"));
    }
    for (f, role) in a
        .files
        .iter()
        .filter(|f| f.path.ends_with(".png"))
        .zip(r.roles)
    {
        let i = image::load_from_memory(&f.bytes).unwrap();
        assert_eq!([i.width(), i.height()], role.dimensions());
    }
}
#[test]
fn title_errors_are_visible_instead_of_silent_truncation() {
    let r = TitleRenderer::new(&font()).unwrap();
    for s in ["", "bad\ntext", &"x".repeat(301), "日本語"] {
        assert!(r.card(s, "", Role::Banner).is_err(), "{s}");
    }
    assert!(
        r.card("Title", &"Very long credit ".repeat(15), Role::Banner)
            .is_err()
    );
    assert!(TitleRenderer::new(b"not a font").is_err());
}
fn fixture() -> (Memory, PanelRequest, Value) {
    let state = json!({"incomplete":false,"avm1Halted":false,"stageToViewport":[1,0,0,1,0,0],"displayTree":[{"id":0,"kind":"movie-clip","name":{"type":"string","utf16":[116,105,116,108,101],"truncated":false},"visible":true,"parent":null,"stageBoundsPixels":[2,2,7,6],"movieFrame":1}]});
    let snapshot = json!({"schema":"ddi-ruffle-probe/experimental-1","source":{"movieSha256":"a".repeat(64),"config":{"sourceUrl":"https://example.invalid/original.swf"}},"rendered":true,"avm1Halted":false,"ruffleRevision":"89a7049965f3090e706265670df309a5128233ac","audit":{"truncated":false,"runtimeErrors":0,"droppedEntries":0,"truncatedDetails":0,"entries":[]},"captures":[{"frame":10,"state":state}]});
    let s = serde_json::to_vec(&snapshot).unwrap();
    let run = serde_json::to_vec(&json!({"schema":"ddi-ruffle-run/experimental-1","workerSha256":"b".repeat(64),"sandboxSha256":"c".repeat(64),"configSha256":"d".repeat(64),"startedAt":"2026-10-13T00:00:00Z","exitCode":0,"termination":null,"inputsUnchanged":true}))
        .unwrap();
    let mut pixels = b"P6\n10 8\n255\n".to_vec();
    for y in 0..8 {
        for x in 0..10 {
            pixels.extend([x * 20, y * 30, 60]);
        }
    }
    let request = PanelRequest {
        snapshot: binding("snapshot", &s),
        run: binding("run", &run),
        image: binding("ppm", &pixels),
        frame: 10,
        clip: "title".into(),
        expected_clip_frame: Some(1),
    };
    (
        Memory(BTreeMap::from([
            ("snapshot".into(), s),
            ("run".into(), run),
            ("ppm".into(), pixels),
        ])),
        request,
        snapshot,
    )
}
#[test]
fn panel_crop_preserves_source_pixels_and_reports_transform() {
    let (store, r, _) = fixture();
    let art = title_panel(&r, &store).unwrap();
    assert_eq!(art.manifest["cropPixels"], json!([1, 1, 7, 6]));
    assert_eq!(art.manifest["titleReplaced"], false);
    let image = image::load_from_memory(&art.files[0].bytes)
        .unwrap()
        .to_rgb8();
    assert_eq!(image.get_pixel(0, 0).0, [20, 30, 60]);
    assert_eq!(image.get_pixel(6, 5).0, [140, 180, 60]);
    let banner = image::load_from_memory(&art.files[1].bytes).unwrap();
    assert_eq!([banner.width(), banner.height()], [640, 200]);
}
#[test]
fn unsafe_incomplete_or_ambiguous_panel_captures_reject() {
    for mutate in [0, 1, 2, 3, 4, 5, 6] {
        let (mut store, mut r, mut s) = fixture();
        match mutate {
            0 => s["captures"][0]["state"]["displayTree"][0]["visible"] = json!(false),
            1 => s["captures"][0]["state"]["incomplete"] = json!(true),
            2 => s["captures"][0]["state"]["displayTree"][0]["parent"] = json!(0),
            3 => s["captures"][0]["state"]["stageToViewport"] = json!([1e308, 0, 0, 1, 0, 0]),
            4 => s["audit"]["runtimeErrors"] = json!(1),
            5 => {
                let v = s["captures"][0].clone();
                s["captures"].as_array_mut().unwrap().push(v);
            }
            _ => r.expected_clip_frame = Some(2),
        };
        let b = serde_json::to_vec(&s).unwrap();
        r.snapshot.sha256 = sha256(&b);
        store.0.insert("snapshot".into(), b);
        assert!(title_panel(&r, &store).is_err(), "case {mutate}");
    }
    let (mut store, r, _) = fixture();
    store.0.get_mut("ppm").unwrap().push(0);
    assert!(title_panel(&r, &store).is_err());
}
#[test]
fn zip_is_deterministic_and_contains_only_requested_paths() {
    use std::{
        io::{Cursor, Read},
        sync::Arc,
    };
    let files = vec![
        Artifact {
            path: "Pack/b/file.txt".into(),
            bytes: Arc::new(b"second".to_vec()),
        },
        Artifact {
            path: "Pack/a.txt".into(),
            bytes: Arc::new(b"first".to_vec()),
        },
    ];
    let a = ddi_danoni_export::package::write_zip(Cursor::new(Vec::new()), &files)
        .unwrap()
        .into_inner();
    let b = ddi_danoni_export::package::write_zip(Cursor::new(Vec::new()), &files)
        .unwrap()
        .into_inner();
    assert_eq!(a, b);
    let mut z = zip::ZipArchive::new(Cursor::new(a)).unwrap();
    let mut e = z.by_index(0).unwrap();
    assert_eq!(e.name().unwrap(), "Pack/a.txt");
    let mut bytes = Vec::new();
    e.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"first");
    drop(e);
    let mut bad = files.clone();
    bad.push(files[0].clone());
    assert!(ddi_danoni_export::package::write_zip(Cursor::new(Vec::new()), &bad).is_err());
    bad[0].path = "../escape".into();
    assert!(ddi_danoni_export::package::write_zip(Cursor::new(Vec::new()), &bad).is_err());
}
