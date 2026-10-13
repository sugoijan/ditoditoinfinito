#![cfg(feature = "native")]
//! Local-only exact audio/tag metadata parity; never executes work code.
use ddi_danoni_export::{assets::*, native::DirectorySource, swf::*};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
fn read(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
fn compare(a: &Value, b: &Value, path: &str) {
    if a.is_number() && b.is_number() {
        let (x, y) = (a.as_f64().unwrap(), b.as_f64().unwrap());
        assert!((x - y).abs() <= 1e-9, "{path}: {x} != {y}");
        return;
    }
    match (a, b) {
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path}"
            );
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn preserved_swf_audio_matches_every_byte_block_and_timing_field() {
    let Some(path) = std::env::var_os("DDI_DANONI_SWF_CASES") else {
        eprintln!("SKIP: set DDI_DANONI_SWF_CASES");
        return;
    };
    let table = read(Path::new(&path));
    assert_eq!(table["schema"], "ddi-danoni-swf-parity-cases/1");
    let root = PathBuf::from(table["root"].as_str().unwrap());
    let source = DirectorySource::new(BTreeMap::from([("corpus".into(), root.clone())])).unwrap();
    let output = std::env::var_os("DDI_DANONI_SWF_OUTPUT").map(PathBuf::from);
    if let Some(out) = &output {
        fs::create_dir(out).expect("fresh external SWF parity output");
    }
    let mut checks = vec![];
    for row in table["cases"].as_array().unwrap() {
        let binding: Binding = serde_json::from_value(row["movie"].clone()).unwrap();
        let original = source
            .read(&binding.namespace, &binding.path, MOVIE_LIMIT)
            .unwrap();
        assert_eq!(sha256(&original), binding.sha256);
        let m = Movie::read(&original, &mut || Ok(())).unwrap();
        let (bytes, report, receipt) = if !row["request"].is_null() {
            let request: DemuxRequest = serde_json::from_value(row["request"].clone()).unwrap();
            match prepare_demux(&request, &source, &mut || Ok(())) {
                Ok(result) => {
                    assert!(row["expectedError"].is_null());
                    let report = result.manifest["stream"].clone();
                    (result.bytes, report, Some(result.manifest))
                }
                Err(error) => {
                    assert_eq!(
                        error.code,
                        row["expectedError"]
                            .as_str()
                            .expect("unexpected job rejection"),
                        "{}: {error}",
                        row["id"]
                    );
                    assert_eq!(error.message, row["expectedMessage"].as_str().unwrap());
                    let observed: ObservedStart =
                        serde_json::from_value(row["observedStart"].clone()).unwrap();
                    let d = m.observed_stream(&observed, &mut || Ok(())).unwrap();
                    (d.bytes, d.report, None)
                }
            }
        } else {
            let d = m
                .demux(row["route"].as_str().unwrap(), None, &mut || Ok(()))
                .unwrap();
            (d.bytes, d.report, None)
        };
        let expected = source
            .read("corpus", row["mp3"]["path"].as_str().unwrap(), MEDIA_LIMIT)
            .unwrap();
        assert_eq!(sha256(&expected), row["mp3"]["sha256"]);
        assert_eq!(sha256(&bytes), sha256(&expected), "{}", row["id"]);
        assert!(bytes == expected);
        let oracle = source
            .read(
                "corpus",
                row["report"]["path"].as_str().unwrap(),
                JSON_LIMIT,
            )
            .unwrap();
        assert_eq!(sha256(&oracle), row["report"]["sha256"]);
        compare(
            &report,
            &serde_json::from_slice(&oracle).unwrap(),
            row["id"].as_str().unwrap(),
        );
        let inventory = m.inventory(&mut || Ok(())).unwrap();
        let item = json!({"id":row["id"],"movieSha256":binding.sha256,"mp3Sha256":sha256(&bytes),"bytes":bytes.len(),"blocks":report["blocks"].as_array().unwrap().len(),"rate":report["sampleRate"],"streamCount":inventory.len(),"runtimeBound":receipt.is_some(),"savedJobRejection":row["expectedError"]});
        if let Some(out) = &output {
            let dir = out.join(row["id"].as_str().unwrap());
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("stream.mp3"), &bytes).unwrap();
            fs::write(
                dir.join("manifest.json"),
                serde_json::to_vec_pretty(&receipt.unwrap_or_else(|| report.clone())).unwrap(),
            )
            .unwrap();
        }
        checks.push(item);
    }
    let receipt = json!({"schema":"ddi-danoni-rust-swf-parity/1","passed":true,"casesSha256":sha256(&fs::read(&path).unwrap()),"checks":checks});
    if let Some(out) = &output {
        fs::write(
            out.join("manifest.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        receipt["checks"].as_array().unwrap().len(),
        table["expectedCases"].as_u64().unwrap() as usize
    );
    eprintln!("SWF parity: {} exact streams", checks.len());
}
