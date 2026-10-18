#![cfg(feature = "native")]
use ddi_danoni_export::{Result, assets::*, native::DirectorySource, network::*};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf};
struct Count {
    source: DirectorySource,
    reads: RefCell<BTreeMap<String, usize>>,
}
impl AssetSource for Count {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        *self
            .reads
            .borrow_mut()
            .entry(format!("{ns}:{path}"))
            .or_default() += 1;
        self.source.read(ns, path, max)
    }
}
#[test]
fn local_cache_replay_matches_original_fetcher() {
    let Ok(path) = std::env::var("DDI_DANONI_CACHE_CASES") else {
        return;
    };
    let doc: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(doc["schema"], "ddi-danoni-cache-replay-cases/1");
    assert_eq!(doc["networkRequests"], 0);
    let mut total = 0;
    let mut hits = 0;
    let mut failures = 0;
    let mut redirects = 0;
    let mut read_files = 0;
    let mut policies = 0;
    let mut unavailable = 0;
    for owner in doc["caches"].as_array().unwrap() {
        let root = PathBuf::from(owner["root"].as_str().unwrap());
        let source = Count {
            source: DirectorySource::new(BTreeMap::from([("cache".into(), root)])).unwrap(),
            reads: RefCell::new(BTreeMap::new()),
        };
        let binding: Binding = serde_json::from_value(owner["index"].clone()).unwrap();
        let mut cache = CacheReplay::open(&source, binding).unwrap();
        for case in owner["cases"].as_array().unwrap() {
            total += 1;
            let request: ReplayRequest = serde_json::from_value(case["request"].clone()).unwrap();
            let result = cache.lookup(&request);
            let expected = &case["expected"];
            if expected["kind"] == "error" {
                assert_eq!(
                    result.err().unwrap().code,
                    expected["code"].as_str().unwrap(),
                    "{}",
                    request.url
                );
                continue;
            }
            let result = result.unwrap_or_else(|e| panic!("{}: {e}", request.url));
            assert_eq!(
                result.receipt()["kind"],
                expected["kind"],
                "{}",
                request.url
            );
            match result {
                ReplayLookup::Hit(response) => {
                    hits += 1;
                    assert_eq!(
                        response.status as u64,
                        expected["status"].as_u64().unwrap(),
                        "{}",
                        request.url
                    );
                    assert_eq!(
                        sha256(&response.body),
                        expected["sha256"].as_str().unwrap(),
                        "{}",
                        request.url
                    );
                    assert_eq!(
                        response.body.len() as u64,
                        expected["byteLength"].as_u64().unwrap(),
                        "{}",
                        request.url
                    );
                    assert_eq!(
                        json!(response.redirect_to),
                        expected["redirectTo"],
                        "{}",
                        request.url
                    );
                    redirects += usize::from(response.redirect_to.is_some());
                }
                ReplayLookup::RecordedFailure { record, .. } => {
                    failures += 1;
                    assert_eq!(&record, &expected["record"], "{}", request.url);
                }
                ReplayLookup::Miss { .. } => panic!("Unexpected miss: {}", request.url),
            }
        }
        for case in owner["robotsCases"].as_array().unwrap() {
            let decision = cache.robots(case["targetUrl"].as_str().unwrap());
            let expected = &case["expected"];
            if expected["kind"] == "unavailable" {
                assert!(decision.is_err(), "{}", case["targetUrl"]);
                unavailable += 1;
            } else {
                let decision = decision.unwrap_or_else(|e| panic!("{}: {e}", case["targetUrl"]));
                assert_eq!(
                    decision.allowed,
                    expected["allowed"].as_bool().unwrap(),
                    "{}",
                    case["targetUrl"]
                );
                assert_eq!(decision.status as u64, expected["status"].as_u64().unwrap());
                assert_eq!(decision.body.sha256, expected["sha256"].as_str().unwrap());
                assert_eq!(
                    json!(decision.crawl_delay_ms),
                    expected["crawlDelayMs"],
                    "{}",
                    case["targetUrl"]
                );
                policies += 1;
            }
        }
        assert!(
            source.reads.borrow().values().all(|n| *n == 1),
            "{}: a bound file was reread",
            owner["id"]
        );
        read_files += source.reads.borrow().len();
    }
    assert!(total > 0);
    println!(
        "{policies} historical robots decisions and {unavailable} unavailable policies match the original fetcher"
    );
    println!(
        "{total} original-fetcher cases: {hits} hits, {failures} failures, {redirects} redirect responses; {read_files} bound files each read once; no network"
    );
}
