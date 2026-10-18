#![cfg(all(feature = "native", unix))]
//! Opt-in durable import acceptance, using a fresh external store only.
use ddi_danoni_export::{
    assets::MEDIA_LIMIT, import_host as host, managed_store::Store, prepared_import,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
#[test]
fn local_ready_outputs_survive_durable_install_and_reopen() {
    let Some(root) = std::env::var_os("DDI_DANONI_MANAGED_STORE") else {
        return;
    };
    let root = PathBuf::from(root);
    let cases = std::env::var_os("DDI_DANONI_IMPORT_CASES")
        .expect("Managed-store corpus needs import cases");
    let cases: Value = serde_json::from_slice(&fs::read(cases).unwrap()).unwrap();
    // create refuses an existing path. This test never adopts the user's library.
    let store = Store::create(&root).unwrap();
    drop(store);
    let mut runs = Vec::new();
    for case in cases["cases"].as_array().unwrap() {
        let ready = PathBuf::from(case["ready"].as_str().unwrap());
        let admitted = prepared_import::load_ready(&ready, &mut |_| Ok(()));
        if case["expected"] == "not-ready" {
            assert!(admitted.is_err());
            runs.push(json!({"ready":ready,"rejected":true}));
            continue;
        }
        let import = admitted.unwrap();
        let mut store = Store::open(&root).unwrap();
        let plan = host::plan(
            &import,
            &store.catalog().unwrap().snapshot(),
            host::DuplicatePolicy::Reject,
        )
        .unwrap();
        let outcome = host::install(&import, &plan, &mut store, &mut |_| Ok(())).unwrap();
        if case["expected"] == "unchanged" {
            assert!(matches!(outcome, host::Outcome::Unchanged { .. }));
        } else {
            assert!(matches!(outcome, host::Outcome::Installed { .. }));
        }
        runs.push(json!({"ready":ready,"plan":plan.summary(),"outcome":outcome}));
        drop(store);
        let store = Store::open(&root).unwrap();
        let catalog = store.verify().unwrap();
        for song in &catalog.songs {
            for file in &song.files {
                store.read_asset(song, &file.path, MEDIA_LIMIT).unwrap();
            }
        }
    }
    let mut store = Store::open(&root).unwrap();
    let catalog = store.verify().unwrap();
    assert_eq!(catalog.songs.len() as u64, cases["songs"].as_u64().unwrap());
    assert_eq!(
        catalog
            .songs
            .iter()
            .map(|s| s.entry.charts.len())
            .sum::<usize>() as u64,
        cases["charts"].as_u64().unwrap()
    );
    let recovery = store.recover().unwrap();
    let report = json!({"schema":"ddi-danoni-managed-store-corpus/1","passed":true,"store":root,"catalog":catalog,"recovery":recovery,"runs":runs,"scope":"Fresh local-only durable store; no accepted exports or app library writes"});
    if let Some(out) = std::env::var_os("DDI_DANONI_MANAGED_REPORT") {
        fs::write(out, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!("{report}");
}
