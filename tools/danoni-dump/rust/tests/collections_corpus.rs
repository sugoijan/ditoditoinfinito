//! Optional local-only, independently enumerated frozen publisher-list oracle.
#![cfg(feature = "native")]
use ddi_danoni_export::collection;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
#[test]
fn frozen_indexes_match_independent_html_oracle() {
    let Some(root) = std::env::var_os("DDI_DANONI_COLLECTION_REVIEW") else {
        eprintln!(
            "Set DDI_DANONI_COLLECTION_REVIEW to the external rust-collections review directory; no third-party fixtures are bundled"
        );
        return;
    };
    let root = PathBuf::from(root);
    let oracle: Value =
        serde_json::from_slice(&fs::read(root.join("oracle.json")).unwrap()).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let d = collection::load_discovery(&root.join(format!("{name}-discovery"))).unwrap();
        assert!(d.inventory().complete);
        assert_eq!(d.inventory().body_sha256, case["sha256"].as_str().unwrap());
        let actual = Value::Array(
            d.inventory()
                .entries
                .iter()
                .map(|e| json!({"url":e.address.as_ref().map(|a|&a.retrieval_url),"label":e.label}))
                .collect(),
        );
        assert_eq!(actual, case["expected"], "independent frozen index {name}");
        assert_eq!(d.transfers().requests, 0);
        eprintln!(
            "{name}: {} destinations, zero HTTP requests",
            d.inventory().entries.len()
        );
    }
}
