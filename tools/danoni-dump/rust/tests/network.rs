#![cfg(feature = "native")]
use ddi_danoni_export::{Error, Result, assets::*, network::*};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, net::IpAddr};
const ROOT: &str = "https://example.org";
#[derive(Default)]
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
    reads: RefCell<BTreeMap<String, usize>>,
}
impl AssetSource for Memory {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        assert_eq!(ns, "cache");
        relative_path(path)?;
        *self.reads.borrow_mut().entry(path.into()).or_default() += 1;
        let b = self.files.get(path).ok_or_else(|| Error {
            code: "missing-test-asset",
            message: path.into(),
        })?;
        if b.len() > max {
            return Err(Error {
                code: "test-asset-limit",
                message: path.into(),
            });
        }
        Ok(b.clone())
    }
}
impl Memory {
    fn record(&mut self, url: &str, body: &[u8], status: u16, headers: Value) -> Value {
        let hash = sha256(body);
        let path = format!("blobs/sha256/{hash}");
        self.files.insert(path.clone(), body.to_vec());
        json!({"requestedUrl":url,"url":url,"finalUrl":url,"status":status,"headers":headers,
            "sha256":hash,"bodyPath":path,"byteLength":body.len(),"completeBody":status!=206,
            "retrievedAt":"2026-10-10T00:00:00.000Z","redirects":[],"robots":[],"requestContext":RequestContext::default(),
            "requestContextFingerprint":RequestContext::default().fingerprint().unwrap()})
    }
    fn index(&mut self, entries: Value, aliases: Value, failures: Value) -> Binding {
        let bytes=serde_json::to_vec(&json!({"schema":"ddi-danoni-cache/1","entries":entries,"aliases":aliases,"failures":failures})).unwrap();
        let binding = Binding {
            namespace: "cache".into(),
            path: "index.json".into(),
            sha256: sha256(&bytes),
        };
        self.files.insert(binding.path.clone(), bytes);
        binding
    }
}
fn request(url: &str) -> ReplayRequest {
    ReplayRequest {
        url: url.into(),
        context: RequestContext::default(),
        maximum_bytes: MEDIA_LIMIT,
    }
}
fn browser(headers: Value) -> RequestContext {
    RequestContext {
        kind: ContextKind::Browser,
        headers: serde_json::from_value(headers).unwrap(),
    }
}
fn hit(lookup: ReplayLookup) -> ReplayResponse {
    match lookup {
        ReplayLookup::Hit(r) => *r,
        _ => panic!("Expected hit"),
    }
}
fn code(lookup: Result<ReplayLookup>) -> &'static str {
    lookup.err().unwrap().code
}
#[test]
fn compatible_fingerprints_and_bounded_actual_request_headers() {
    assert_eq!(
        RequestContext::default().fingerprint().unwrap(),
        "be1ab9a068d2371605191b9e8df0af648aafecf4760c70050295c0a79179c34c"
    );
    for headers in [
        json!({"cookie":"session=x"}),
        json!({"authorization":"secret"}),
        json!({"Accept":"*/*"}),
        json!({"referer":"https://user:pass@example.org/"}),
        json!({"origin":"https://example.org/path"}),
        json!({"accept":"x\r\ny"}),
    ] {
        assert!(browser(headers).validate().is_err());
    }
    assert!(
        browser(
            json!({"origin":"null","referer":"https://example.org/page?key=5","accept":"audio/*"})
        )
        .validate()
        .is_ok()
    );
    let bad = RequestContext {
        kind: ContextKind::Anonymous,
        headers: BTreeMap::new(),
    };
    assert!(bad.validate().is_err());
}
#[test]
fn no_vary_reuses_bytes_without_rewriting_captured_context() {
    let mut m = Memory::default();
    let r = m.record(&format!("{ROOT}/song"), b"payload", 200, json!({}));
    let b = m.index(json!({format!("{ROOT}/song"):r}), json!([]), json!({}));
    let mut cache = CacheReplay::open(&m, b).unwrap();
    let mut q = request(&format!("{ROOT}/song"));
    let first = hit(cache.lookup(&q).unwrap());
    q.context = browser(json!({"accept":"audio/*","origin":ROOT}));
    let second = hit(cache.lookup(&q).unwrap());
    assert_eq!(*second.body, b"payload");
    assert_eq!(second.resource["requestContext"]["kind"], "anonymous");
    assert_eq!(second.request.context.kind, ContextKind::Browser);
    assert!(std::sync::Arc::ptr_eq(&first.body, &second.body));
    assert_eq!(m.reads.borrow().values().sum::<usize>(), 2);
}
#[test]
fn matching_vary_replays_but_mismatch_star_and_unknown_never_become_misses() {
    for vary in [
        "Origin, Accept",
        "*",
        "X-Unknown",
        "Accept-Encoding, User-Agent, Cookie, Authorization, Range, Host",
    ] {
        let url = format!("{ROOT}/varied");
        let mut m = Memory::default();
        let mut r = m.record(&url, b"varied", 200, json!({"vary":vary}));
        let context = browser(json!({"origin":ROOT,"accept":"audio/*"}));
        r["requestContext"] = json!(context);
        r["requestContextFingerprint"] = json!(context.fingerprint().unwrap());
        let b = m.index(json!({url.clone():r}), json!([]), json!({}));
        let mut cache = CacheReplay::open(&m, b).unwrap();
        let mut q = request(&url);
        q.context = context;
        if ["*", "X-Unknown"].contains(&vary) {
            assert_eq!(code(cache.lookup(&q)), "cache-vary-unsupported");
            assert_eq!(m.reads.borrow().values().sum::<usize>(), 1);
        } else {
            assert_eq!(hit(cache.lookup(&q).unwrap()).status, 200);
            if vary.starts_with("Origin") {
                q.context = RequestContext::default();
                assert_eq!(code(cache.lookup(&q)), "cache-vary-mismatch");
            }
        }
    }
}
#[test]
fn aliases_are_explicit_same_origin_queries_are_never_guessed() {
    let mut m = Memory::default();
    let url = format!("{ROOT}/dos.txt");
    let alias = format!("{url}?1234567890123");
    let r = m.record(&url, b"dos", 200, json!({}));
    let b = m.index(json!({url.clone():r}), json!([[alias, url]]), json!({}));
    let mut cache = CacheReplay::open(&m, b).unwrap();
    let a = hit(cache.lookup(&request(&alias)).unwrap());
    let c = hit(cache.lookup(&request(&url)).unwrap());
    assert!(std::sync::Arc::ptr_eq(&a.body, &c.body));
    assert_eq!(a.request.url, alias);
    assert_eq!(a.canonical_url, url);
    assert!(matches!(
        cache
            .lookup(&request(&format!("{url}?1234567890124")))
            .unwrap(),
        ReplayLookup::Miss { .. }
    ));
    assert_eq!(m.reads.borrow().values().sum::<usize>(), 2);
    for aliases in [
        json!([[url, "https://other.example/song"]]),
        json!([[url, alias], [alias, url]]),
        json!([[alias, url], [alias, url]]),
    ] {
        let b = m.index(json!({}), aliases, json!({}));
        assert!(CacheReplay::open(&m, b).is_err());
    }
}
#[test]
fn refusals_are_bound_evidence_and_are_not_offline_misses() {
    let mut m = Memory::default();
    let url = format!("{ROOT}/blocked");
    let alias = format!("{url}?1");
    let failure =
        json!({"code":"robots-denied","message":"Original refusal","failedAt":"2026-10-10"});
    let b = m.index(
        json!({}),
        json!([[alias, url]]),
        json!({url.clone():failure}),
    );
    let mut cache = CacheReplay::open(&m, b).unwrap();
    let replay = cache.lookup(&request(&alias)).unwrap();
    assert_eq!(replay.receipt()["kind"], "recorded-failure");
    assert_eq!(replay.receipt()["record"], failure);
    assert_eq!(m.reads.borrow().values().sum::<usize>(), 1);
}
#[test]
fn each_redirect_hop_retains_its_body_and_target_vary_is_rechecked() {
    let root = format!("{ROOT}/start");
    let target = format!("{ROOT}/end");
    let mut m = Memory::default();
    let mut hop = m.record(&root, b"redirect", 302, json!({"location":"/end"}));
    hop["location"] = json!(target);
    hop.as_object_mut().unwrap().remove("completeBody");
    let mut end = m.record(&target, b"final", 200, json!({"vary":"Origin"}));
    let browser = browser(json!({"origin":ROOT}));
    end["requestContext"] = json!(browser);
    end["requestContextFingerprint"] = json!(browser.fingerprint().unwrap());
    let mut collapsed = end.clone();
    collapsed["requestedUrl"] = json!(root);
    collapsed["url"] = json!(root);
    collapsed["redirects"] = json!([hop]);
    let b = m.index(
        json!({root.clone():collapsed,target.clone():end}),
        json!([]),
        json!({}),
    );
    let mut cache = CacheReplay::open(&m, b).unwrap();
    let r = hit(cache.lookup(&request(&root)).unwrap());
    assert_eq!(r.status, 302);
    assert_eq!(*r.body, b"redirect");
    assert_eq!(r.redirect_to, Some(target.clone()));
    assert_eq!(code(cache.lookup(&request(&target))), "cache-vary-mismatch");
    let mut q = request(&target);
    q.context = browser;
    assert_eq!(*hit(cache.lookup(&q).unwrap()).body, b"final");
}
#[test]
fn cache_tampering_incomplete_responses_and_body_bounds_are_closed_failures() {
    let url = format!("{ROOT}/body");
    for kind in [
        "hash",
        "length",
        "path",
        "partial",
        "fingerprint",
        "header-case",
        "header-empty",
        "limit",
        "date",
    ] {
        let mut m = Memory::default();
        let mut r = m.record(&url, b"payload", 200, json!({}));
        let hash = r["sha256"].as_str().unwrap().to_owned();
        match kind {
            "hash" => {
                m.files
                    .insert(format!("blobs/sha256/{hash}"), b"edited!".to_vec());
            }
            "length" => r["byteLength"] = json!(99),
            "path" => r["bodyPath"] = json!("../outside"),
            "partial" => r["status"] = json!(206),
            "fingerprint" => r["requestContextFingerprint"] = json!("f".repeat(64)),
            "header-case" => r["headers"] = json!({"Vary":"*"}),
            "header-empty" => r["headers"] = json!({"":"bad"}),
            "date" => r["retrievedAt"] = Value::Null,
            _ => {}
        }
        let b = m.index(json!({url.clone():r}), json!([]), json!({}));
        let mut cache = CacheReplay::open(&m, b).unwrap();
        let mut q = request(&url);
        if kind == "limit" {
            q.maximum_bytes = 2;
        }
        assert!(cache.lookup(&q).is_err(), "{kind}");
    }
}
fn robots(body: &[u8], status: u16) -> ReplayResponse {
    let mut m = Memory::default();
    let url = format!("{ROOT}/robots.txt");
    let r = m.record(&url, body, status, json!({}));
    let b = m.index(json!({url.clone():r}), json!([]), json!({}));
    let mut cache = CacheReplay::open(&m, b).unwrap();
    hit(cache.lookup(&request(&url)).unwrap())
}
#[test]
fn robots_specific_groups_percent_paths_query_rules_and_longest_allow() {
    let r=robots(b"\xef\xbb\xbfUser-agent: *\nDisallow: /\nCrawl-delay: 5\n\nUser-agent: DdiDanoniDump\nDisallow: /secret/\nAllow: /secret/open?keys=5$\nCrawl-delay: 1.25\n",200);
    let denied = evaluate_robots(&r, &format!("{ROOT}/secret/chart")).unwrap();
    assert!(!denied.allowed);
    assert_eq!(denied.crawl_delay_ms, Some(1250));
    assert!(
        evaluate_robots(&r, &format!("{ROOT}/secret/open?keys=5"))
            .unwrap()
            .allowed
    );
    assert!(
        !evaluate_robots(&r, &format!("{ROOT}/secret/open?keys=7"))
            .unwrap()
            .allowed
    );
    assert!(
        evaluate_robots(&r, &format!("{ROOT}/public"))
            .unwrap()
            .allowed
    );
    let r = robots("User-agent: *\nDisallow: /曲/\n".as_bytes(), 200);
    assert!(
        !evaluate_robots(&r, &format!("{ROOT}/%E6%9B%B2/song"))
            .unwrap()
            .allowed
    );
}
#[test]
fn robots_missing_is_allowed_but_failures_bad_encoding_and_wrong_origin_stop() {
    assert!(
        evaluate_robots(&robots(b"not found", 404), &format!("{ROOT}/song"))
            .unwrap()
            .allowed
    );
    for r in [
        robots(b"server error", 503),
        robots(b"\xff", 200),
        robots(b"User-agent: *\0\n", 200),
        robots(&vec![b' '; ROBOTS_LIMIT + 1], 200),
    ] {
        assert!(evaluate_robots(&r, &format!("{ROOT}/song")).is_err());
    }
    let mut r = robots(b"User-agent: *\nAllow: /\n", 200);
    assert!(evaluate_robots(&r, "https://other.example/song").is_err());
    r.body = std::sync::Arc::new(b"User-agent: *\nDisallow: /\n".to_vec());
    assert!(evaluate_robots(&r, &format!("{ROOT}/song")).is_err());
}
#[test]
fn robots_crawl_delays_are_conservative_and_never_clamped_down() {
    for (body, delay) in [
        (
            "User-agent: *\nCrawl-delay: 2\nUser-agent: DdiDanoniDump\nDisallow:\n",
            None,
        ),
        (
            "User-agent: *\nCrawl-delay: NaN\nUser-agent: DdiDanoniDump\nCrawl-delay: 0\n",
            Some(0),
        ),
        (
            "User-agent: DdiDanoniDump\nCrawl-delay: 2\nUser-agent: DdiDanoniDump\nCrawl-delay: 5\n",
            Some(5000),
        ),
    ] {
        assert_eq!(
            evaluate_robots(&robots(body.as_bytes(), 200), ROOT)
                .unwrap()
                .crawl_delay_ms,
            delay
        );
    }
    for delay in ["NaN", "inf", "-1", "3601"] {
        let body = format!("User-agent: *\nCrawl-delay: {delay}\n");
        assert!(evaluate_robots(&robots(body.as_bytes(), 200), ROOT).is_err());
    }
}
#[test]
fn pacing_waits_are_cancellable_and_started_slots_are_per_origin() {
    let mut p = Pacer::default();
    assert_eq!(p.ready_at(ROOT, 0, None).unwrap(), 0);
    // Repeated reservations/cancellation do not count as starts.
    assert_eq!(p.ready_at(ROOT, 10, None).unwrap(), 10);
    p.mark_started(ROOT, 10, None).unwrap();
    assert_eq!(p.ready_at(ROOT, 20, None).unwrap(), 1010);
    assert_eq!(p.ready_at(ROOT, 20, Some(3000)).unwrap(), 3010);
    assert!(p.mark_started(ROOT, 30, None).is_err());
    p.mark_started(ROOT, 1010, None).unwrap();
    assert_eq!(p.ready_at("http://example.org/", 1010, None).unwrap(), 1010);
    assert!(p.ready_at(ROOT, 1000, None).is_err());
    assert!(p.ready_at(ROOT, 1010, Some(3_600_001)).is_err());
    assert!(Pacer::new(0).is_err());
    let mut p = Pacer::default();
    p.mark_started(ROOT, u64::MAX, None).unwrap();
    assert!(p.ready_at(ROOT, u64::MAX, None).is_err());
}
#[test]
fn dns_validation_checks_every_answer_and_pins_the_exact_url_port() {
    let ips: Vec<IpAddr> = vec![
        "8.8.8.8".parse().unwrap(),
        "2001:4860:4860::8888".parse().unwrap(),
        "8.8.8.8".parse().unwrap(),
    ];
    let pinned = pin_public_addresses("https://example.org:8443/song", &ips).unwrap();
    assert_eq!(pinned.len(), 2);
    assert!(pinned.iter().all(|a| a.port() == 8443));
    for bad in [
        "127.0.0.1",
        "10.0.0.1",
        "192.168.1.1",
        "172.16.0.1",
        "169.254.169.254",
        "100.64.0.1",
        "192.0.2.1",
        "198.18.0.1",
        "203.0.113.1",
        "224.0.0.1",
        "::1",
        "fc00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
        "2001:db8::1",
        "2002:7f00:1::1",
        "3fff::1",
    ] {
        assert!(
            pin_public_addresses(ROOT, &[ips[0], bad.parse().unwrap()]).is_err(),
            "{bad}"
        );
    }
    assert!(pin_public_addresses(ROOT, &[]).is_err());
    assert!(pin_public_addresses("https://1.1.1.1/", &[ips[0]]).is_err());
    assert!(pin_public_addresses("http://127.1/", &["127.0.0.1".parse().unwrap()]).is_err());
}

#[test]
fn cached_robots_redirects_preserve_full_chain_and_do_not_skip_missing_targets() {
    let start = format!("{ROOT}/robots.txt");
    let end = "https://policy.example/robots.txt";
    let mut m = Memory::default();
    let first = m.record(&start, b"redirect", 302, json!({"location":end}));
    let final_response = m.record(end, b"User-agent: *\nDisallow: /private/\n", 200, json!({}));
    let b = m.index(
        json!({start.clone():first.clone(),end:final_response}),
        json!([]),
        json!({}),
    );
    let mut cache = CacheReplay::open(&m, b).unwrap();
    let decision = cache.robots(&format!("{ROOT}/private/song")).unwrap();
    assert!(!decision.allowed);
    assert_eq!(decision.robots_url, start);
    assert_eq!(decision.retrieval_url, end);
    assert_eq!(decision.responses.len(), 2);
    let chain = vec![
        hit(cache.lookup(&request(&start)).unwrap()),
        hit(cache.lookup(&request(end)).unwrap()),
    ];
    assert!(evaluate_robots_chain(&chain, "https://policy.example/song").is_err());
    assert!(evaluate_robots_chain(&chain[1..], ROOT).is_err());
    let b = m.index(json!({start.clone():first}), json!([]), json!({}));
    let mut cache = CacheReplay::open(&m, b).unwrap();
    assert!(cache.robots(ROOT).is_err());
    let a = m.record(&start, b"a", 302, json!({"location":end}));
    let z = m.record(end, b"z", 302, json!({"location":start}));
    let b = m.index(json!({start:a,end:z}), json!([]), json!({}));
    let mut cache = CacheReplay::open(&m, b).unwrap();
    assert!(cache.robots(ROOT).is_err());
}
#[test]
fn cache_versions_key_conflicts_and_ambiguous_refusals_reject_at_open() {
    let url = format!("{ROOT}/song");
    let mut m = Memory::default();
    let r = m.record(&url, b"song", 200, json!({}));
    let b = m.index(
        json!({url.clone():r.clone()}),
        json!([]),
        json!({url.clone():{"code":"robots-denied","message":"refused"}}),
    );
    assert!(CacheReplay::open(&m, b).is_err());
    let b = m.index(json!({format!("{ROOT}/other"):r}), json!([]), json!({}));
    assert!(CacheReplay::open(&m, b).is_err());
    let mut b = m.index(json!({}), json!([]), json!({}));
    let mut doc: Value = serde_json::from_slice(&m.files["index.json"]).unwrap();
    doc["schema"] = json!("ddi-danoni-cache/2");
    let bytes = serde_json::to_vec(&doc).unwrap();
    b.sha256 = sha256(&bytes);
    m.files.insert("index.json".into(), bytes);
    assert!(CacheReplay::open(&m, b).is_err());
}
