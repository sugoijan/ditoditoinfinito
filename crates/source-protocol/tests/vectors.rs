//! `vectors/v1.json`: version 1's frames as bytes, with the message each
//! decodes to or why it is refused, for implementations in other
//! languages. Made here and checked against the file; `DDI_WRITE_VECTORS=1`
//! writes the file again after a deliberate change.

use std::collections::BTreeMap;

use ddi_source_protocol::frame::{self, read_frame};
use ddi_source_protocol::*;
use serde_json::{Value, json};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn raw(json: &str, attachment: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&((4 + json.len() + attachment.len()) as u32).to_be_bytes());
    out.extend_from_slice(&(json.len() as u32).to_be_bytes());
    out.extend_from_slice(json.as_bytes());
    out.extend_from_slice(attachment);
    out
}

fn app(name: &str, message: AppMessage) -> Value {
    let bytes = frame::encode(&message, &[]).unwrap();
    json!({"name": name, "from": "app", "frame": hex(&bytes), "message": serde_json::to_value(&message).unwrap()})
}

fn plugin(name: &str, message: PluginMessage, attachment: &[u8]) -> Value {
    let bytes = frame::encode(&message, attachment).unwrap();
    let mut v = json!({"name": name, "from": "plugin", "frame": hex(&bytes), "message": serde_json::to_value(&message).unwrap()});
    if !attachment.is_empty() {
        v["attachment"] = hex(attachment).into();
    }
    v
}

fn refused(name: &str, from: &str, bytes: &[u8], error: &str) -> Value {
    json!({"name": name, "from": from, "frame": hex(bytes), "error": error})
}

fn vectors() -> Value {
    let mut filters = BTreeMap::new();
    filters.insert(
        "level".to_string(),
        FilterValue::Range {
            min: Some(5.0),
            max: Some(9.0),
        },
    );
    let list = vec![
        app(
            "app hello",
            AppMessage::Hello {
                protocols: vec![1],
                features: vec![],
                app: AppInfo {
                    version: "0.1.0".into(),
                    language: "en".into(),
                },
                dirs: Dirs {
                    cache: "/cache/plugins/example".into(),
                    data: "/data/plugin-data/example".into(),
                },
            },
        ),
        app(
            "open a link",
            AppMessage::Open {
                op: 1,
                input: Input {
                    kind: InputKind::Url,
                    value: "https://songs.example/pack/7".into(),
                },
            },
        ),
        app(
            "search with a filter",
            AppMessage::Search {
                op: 2,
                scope: None,
                query: "night".into(),
                filters,
                cursor: None,
                page_size: 50,
            },
        ),
        app(
            "describe",
            AppMessage::Describe {
                op: 3,
                item: "song-17".into(),
            },
        ),
        app(
            "fetch into memory",
            AppMessage::Fetch {
                op: 4,
                item: "song-17".into(),
                deliver: Deliver::Memory { budget: 67_108_864 },
            },
        ),
        app(
            "fetch into a cache folder",
            AppMessage::Fetch {
                op: 5,
                item: "pack-2".into(),
                deliver: Deliver::CacheDir {
                    path: "/cache/sources/op-5".into(),
                },
            },
        ),
        app("cancel", AppMessage::Cancel { op: 4 }),
        app("shutdown", AppMessage::Shutdown {}),
        plugin(
            "plugin hello",
            PluginMessage::Hello {
                protocol: 1,
                features: vec![],
                plugin: PluginInfo {
                    id: "example".into(),
                    version: "1.0.0".into(),
                },
                capabilities: vec![
                    Capability::Search,
                    Capability::Filters,
                    Capability::Metadata,
                    Capability::Fetch,
                ],
                inputs: vec![],
                filters: vec![
                    Filter {
                        id: "level".into(),
                        label: Text::en("Level").with("ja", "レベル"),
                        kind: FilterKind::Range {
                            min: 1.0,
                            max: 20.0,
                            step: 1.0,
                            unit: Unit::Meter,
                        },
                    },
                    Filter {
                        id: "style".into(),
                        label: Text::en("Style"),
                        kind: FilterKind::Choice {
                            multiple: true,
                            choices: vec![Choice {
                                id: "single".into(),
                                label: Text::en("Single"),
                            }],
                        },
                    },
                ],
            },
            &[],
        ),
        plugin(
            "results",
            PluginMessage::Results {
                op: 2,
                items: vec![ItemMeta {
                    id: "song-17".into(),
                    kind: Some(ItemKind::Song),
                    title: "Night Train".into(),
                    artist: Some("Someone".into()),
                    charts: vec![ChartMeta {
                        style: "dance-single".into(),
                        difficulty: Difficulty::Hard,
                        meter: Some(9),
                        name: None,
                        author: None,
                    }],
                    size: Some(4_200_000),
                    license: Some("CC BY 4.0".into()),
                    ..Default::default()
                }],
            },
            &[],
        ),
        plugin(
            "results: an item that may only be played here",
            PluginMessage::Results {
                op: 2,
                items: vec![ItemMeta {
                    id: "song-18".into(),
                    title: "Day Train".into(),
                    reach: Some(Reach::Play),
                    ..Default::default()
                }],
            },
            &[],
        ),
        plugin(
            "done with a cursor",
            PluginMessage::Done {
                op: 2,
                cursor: Some("page-2".into()),
                total: Some(130),
            },
            &[],
        ),
        plugin(
            "progress",
            PluginMessage::Progress {
                op: 4,
                step: Step::Downloading,
                done: Some(1),
                total: Some(3),
                bytes: Some(1_048_576),
            },
            &[],
        ),
        plugin(
            "a file's chunk",
            PluginMessage::File {
                op: 4,
                path: "Night Train/song.ssc".into(),
                size: 5,
                offset: 0,
                last: true,
            },
            b"#TITL",
        ),
        plugin(
            "fetched files",
            PluginMessage::Fetched {
                op: 4,
                delivery: Delivery::Files {
                    files: 2,
                    bytes: 4_200_000,
                },
            },
            &[],
        ),
        plugin(
            "failed",
            PluginMessage::Failed {
                op: 3,
                code: FailCode::RateLimited,
                detail: Some("429 from the site".into()),
            },
            &[],
        ),
        plugin("bye", PluginMessage::Bye {}, &[]),
        refused("an empty stream ends cleanly", "plugin", &[], "end"),
        refused(
            "a cut length",
            "plugin",
            &raw(r#"{"type":"bye"}"#, &[])[..2],
            "truncated",
        ),
        refused(
            "a cut body",
            "plugin",
            &raw(r#"{"type":"bye"}"#, &[])[..10],
            "truncated",
        ),
        refused(
            "a length past 4 MiB",
            "plugin",
            &(4 * 1024 * 1024 + 1u32).to_be_bytes(),
            "oversized",
        ),
        refused(
            "a JSON length past the frame",
            "plugin",
            &[0, 0, 0, 6, 0, 0, 0, 9, b'{', b'}'],
            "malformed",
        ),
        refused(
            "an unknown field",
            "plugin",
            &raw(r#"{"type":"bye","extra":true}"#, &[]),
            "malformed",
        ),
        refused(
            "an unknown message",
            "plugin",
            &raw(r#"{"type":"teleport","op":1}"#, &[]),
            "malformed",
        ),
        refused(
            "an item's room rule, replaced by its reach",
            "plugin",
            &raw(
                r#"{"type":"results","op":2,"items":[{"id":"song-18","title":"Day Train","rooms":"none"}]}"#,
                &[],
            ),
            "malformed",
        ),
        refused(
            "a reach that is not one of the three",
            "plugin",
            &raw(
                r#"{"type":"results","op":2,"items":[{"id":"song-18","title":"Day Train","reach":"share"}]}"#,
                &[],
            ),
            "malformed",
        ),
        refused(
            "words where a value belongs",
            "plugin",
            &raw(r#"{"type":"progress","op":1,"step":"Downloading…"}"#, &[]),
            "malformed",
        ),
        refused(
            "an attachment on a message that takes none",
            "plugin",
            &raw(r#"{"type":"bye"}"#, b"x"),
            "malformed",
        ),
        refused(
            "an attachment on an app message",
            "app",
            &raw(r#"{"type":"shutdown"}"#, b"x"),
            "malformed",
        ),
    ];
    json!({
        "protocol": 1,
        "framing": "u32 big-endian length of the rest (4 + json + attachment, at most 4194304); u32 big-endian JSON length (at least 2); the JSON; the attachment (at most 1048576 bytes, a `file` message's only). A clean end only between frames.",
        "vectors": list,
    })
}

/// What the frame's bytes decode to: the message as JSON (and its
/// attachment), or the refusal's kind.
fn decode(from: &str, bytes: &[u8]) -> Result<(Value, Vec<u8>), String> {
    let mut r = bytes;
    let frame = match read_frame(&mut r) {
        Ok(Some(f)) => f,
        Ok(None) => return Err("end".into()),
        Err(e) => return Err(e.kind().into()),
    };
    if from == "app" {
        frame::decode_app(frame)
            .map(|m| (serde_json::to_value(m).unwrap(), Vec::new()))
            .map_err(|e| e.kind().into())
    } else {
        frame::decode_plugin(frame)
            .map(|(m, a)| (serde_json::to_value(m).unwrap(), a))
            .map_err(|e| e.kind().into())
    }
}

#[test]
fn the_vector_file_matches_and_every_vector_decodes_as_it_says() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("vectors/v1.json");
    let made = serde_json::to_string_pretty(&vectors()).unwrap() + "\n";
    if std::env::var_os("DDI_WRITE_VECTORS").is_some() {
        std::fs::write(&path, &made).unwrap();
    }
    let file = std::fs::read_to_string(&path).unwrap();
    assert_eq!(file, made, "vectors/v1.json is stale: DDI_WRITE_VECTORS=1");
    let parsed: Value = serde_json::from_str(&file).unwrap();
    for v in parsed["vectors"].as_array().unwrap() {
        let name = v["name"].as_str().unwrap();
        let bytes = unhex(v["frame"].as_str().unwrap());
        let got = decode(v["from"].as_str().unwrap(), &bytes);
        match v.get("error") {
            Some(error) => assert_eq!(got.unwrap_err(), error.as_str().unwrap(), "{name}"),
            None => {
                let (message, attachment) = got.unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(message, v["message"], "{name}");
                let want = v.get("attachment").map_or(Vec::new(), |a| unhex(a.as_str().unwrap()));
                assert_eq!(attachment, want, "{name}");
            }
        }
    }
}
