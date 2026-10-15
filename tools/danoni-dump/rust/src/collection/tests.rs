use super::*;
use crate::network::{NetworkCancel, RequestContext};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
const URL: &str = "https://autumnleaves.yukishigure.com/danoni.html";
fn row(no: &str, cell: &str) -> String {
    format!(
        "<tr><td>No.{no}</td><td>{cell}</td><td><a href='https://composer.example/'>Artist</a></td><td>★★★</td><td>Comment</td><td>Date</td></tr>"
    )
}
fn html() -> String {
    format!(
        "<meta charset=utf-8><title>List</title><a href='https://ads.example/'>Ad</a><table>{}{}{}{}</table>",
        row("1", "<a href='one/main.html?chart=1'>One<br>Title</a>"),
        row("2", "<a href='one/main.html?chart=2'>Other chart</a>"),
        row("3", "<a href='one/main.html?chart=1'>Duplicate</a>"),
        row("4", "Removed")
    )
}
#[test]
fn reviewed_rows_keep_semantic_queries_deduplicate_and_do_not_follow_composers() {
    let i = inspect(URL, html().as_bytes(), None, 512).unwrap();
    assert_eq!(i.entries.len(), 3);
    assert!(i.complete);
    assert_eq!(i.entries[0].label, "One Title");
    assert_eq!(i.entries[0].occurrences.len(), 2);
    assert!(
        i.entries[0]
            .address
            .as_ref()
            .unwrap()
            .logical_url
            .ends_with("?chart=1")
    );
    assert!(
        i.entries[1]
            .address
            .as_ref()
            .unwrap()
            .logical_url
            .ends_with("?chart=2")
    );
    assert_eq!(i.entries[2].kind, EntryKind::Unavailable);
    let empty = format!("<table>{}</table>", row("9", ""));
    assert_eq!(
        inspect(URL, empty.as_bytes(), None, 512).unwrap().entries[0].kind,
        EntryKind::Unavailable
    );
    let bounded = inspect(URL, html().as_bytes(), None, 1).unwrap();
    assert!(!bounded.complete);
    assert_eq!(bounded.entries[0].occurrences.len(), 2);
    assert_eq!(bounded.omitted, 2);
    assert!(
        inspect(
            "https://example.org/list.html",
            html().as_bytes(),
            None,
            512
        )
        .is_err()
    );
}
#[test]
fn festival_music_blocks_and_archives_preserve_capture_identity() {
    let body=b"<div id='music'>Music Name</div><div id='music'><a href='works/a/main.html'>A</a></div><div id='other2'><a href='https://artist.example/'>Artist</a></div><div id='music'><a href='https://web.archive.org/web/20190315000000id_/http://other.example/game.html'>B</a></div>";
    let i=inspect("https://web.archive.org/web/20190314000000id_/http://cw7.sakura.ne.jp/event/7fes06/works.html",body,None,512).unwrap();
    assert_eq!(i.entries.len(), 2);
    assert_eq!(
        i.entries[0].address.as_ref().unwrap().capture.as_deref(),
        Some("20190314000000")
    );
    assert_eq!(
        i.entries[1].address.as_ref().unwrap().capture.as_deref(),
        Some("20190315000000")
    );
    let bad=b"<div id=music><a href='https://web.archive.org/web/20190315000000oe_/http://other.example/game.html'>B</a></div>";
    assert_eq!(
        inspect(
            "https://cw7.sakura.ne.jp/event/7fes06/works.html",
            bad,
            None,
            512
        )
        .unwrap()
        .entries[0]
            .kind,
        EntryKind::UnsupportedAddress
    );
}
#[test]
fn cw_catalogue_requires_its_own_detail_cell_and_keeps_score_queries() {
    let row = |id: &str, href: &str| {
        format!(
            "<tr><td><a href='/dtl/ds0301/{id}'>5/{id}</a></td><td><a href='{href}'>Song</a></td><td><a href='https://artist.example/'>Artist</a></td><td>Hard</td><td>3</td><td>150</td><td>2:00</td><td>Date</td><td>HTML5</td></tr>"
        )
    };
    let body = format!(
        "<table>{}{}{}</table>",
        row("1", "https://songs.example/main/?id=A&amp;scoreId=0"),
        row("2", "https://songs.example/main/?id=A&amp;scoreId=1"),
        "<tr><td><a href='/dtl/ds9999/1'>other</a></td><td><a href='https://ignore.example/'>Wrong author</a></td><td></td><td></td><td></td><td></td><td></td><td></td><td></td></tr>"
    );
    let i = inspect(
        "https://cw7.sakura.ne.jp/lst/ds0301/?keys=5",
        body.as_bytes(),
        None,
        512,
    )
    .unwrap();
    assert_eq!(i.entries.len(), 2);
    assert!(
        i.entries[0]
            .address
            .as_ref()
            .unwrap()
            .logical_url
            .ends_with("id=A&scoreId=0")
    );
    assert!(
        inspect(
            "https://cw7.sakura.ne.jp/lst/ds0301/?keys=9t",
            body.as_bytes(),
            None,
            512
        )
        .is_err()
    );
}
#[test]
fn iso_navigation_and_removed_rows_stay_explicit() {
    let b = "<table><tr><td>３</td><td><a href='onigiri3.html'>A</a></td><td>Artist</td><td>Stars</td><td>Date</td><td>Comment</td></tr><tr><td>4</td><td>Removed</td><td></td><td></td><td></td><td></td></tr></table>";
    let i = inspect(
        "http://eucaly.net/~iso50/flash.html",
        b.as_bytes(),
        None,
        512,
    )
    .unwrap();
    assert_eq!(i.entries[0].kind, EntryKind::NavigationRequired);
    assert_eq!(i.entries[1].kind, EntryKind::Unavailable);
}
#[test]
fn fake_raw_markup_structure_drift_and_limits_fail_closed() {
    let fake = format!(
        "<script>var x={:?}</script><textarea>{}</textarea><style>{}</style>",
        html(),
        html(),
        html()
    );
    assert!(inspect(URL, fake.as_bytes(), None, 512).is_err());
    assert!(
        inspect(
            URL,
            b"<table><tr><td>No.1</td><td><a href='a.html'>A",
            None,
            512
        )
        .is_err()
    );
    let deep = format!(
        "{}{}{}",
        "<table>".repeat(65),
        html(),
        "</table>".repeat(65)
    );
    assert!(inspect(URL, deep.as_bytes(), None, 512).is_err());
}
struct Local(PathBuf);
impl Local {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "ddi-index-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn cache(&self, url: &str, body: &[u8], status: u16) -> PathBuf {
        let cache = self.0.join("cache");
        fs::create_dir(&cache).unwrap();
        let hash = sha256(body);
        let path = format!("blobs/sha256/{hash}");
        fs::create_dir_all(cache.join("blobs/sha256")).unwrap();
        fs::write(cache.join(&path), body).unwrap();
        let context = RequestContext::default();
        let record = json!({"requestedUrl":url,"url":url,"finalUrl":url,"status":status,"headers":{"content-type":"text/html; charset=utf-8"},"mimeType":"text/html","charset":"utf-8","sha256":hash,"bodyPath":path,"byteLength":body.len(),"completeBody":true,"retrievedAt":"2026-10-10T00:00:00Z","redirects":[],"robots":[],"requestContext":context,"requestContextFingerprint":context.fingerprint().unwrap()});
        fs::write(cache.join("index.json"),serde_json::to_vec(&json!({"schema":"ddi-danoni-cache/1","entries":{url:record},"aliases":[],"failures":{}})).unwrap()).unwrap();
        cache
    }
}
impl Drop for Local {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn offline_discovery_replays_without_cache_and_detects_bound_tampering() {
    let l = Local::new();
    let cache = l.cache(URL, html().as_bytes(), 200);
    let mut q = IndexRequest::new(URL);
    q.fetch.offline = true;
    let d = discover(
        &q,
        &cache,
        &l.0.join("index"),
        &NetworkCancel::default(),
        &mut |_| panic!("No live access"),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(d.inventory().entries.len(), 3);
    assert_eq!(d.transfers().requests, 0);
    fs::remove_dir_all(cache).unwrap();
    let replay = load_discovery(d.directory()).unwrap();
    assert_eq!(
        serde_json::to_value(replay.inventory()).unwrap(),
        serde_json::to_value(d.inventory()).unwrap()
    );
    let p = d.directory().join("inventory.json");
    let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    v["entries"][0]["label"] = json!("forged");
    fs::write(&p, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(load_discovery(d.directory()).is_err());
}
#[test]
fn cancelled_discovery_does_not_publish_even_when_inventory_exists() {
    let l = Local::new();
    let cache = l.cache(URL, html().as_bytes(), 200);
    let mut q = IndexRequest::new(URL);
    q.fetch.offline = true;
    let cancel = NetworkCancel::default();
    let out = l.0.join("index");
    let r = discover(
        &q,
        &cache,
        &out,
        &cancel,
        &mut |_| panic!("No live access"),
        &mut |p| {
            if matches!(p, DiscoveryProgress::Ready) {
                cancel.cancel();
            }
            Ok(())
        },
    );
    assert_eq!(r.err().unwrap().code, "request-cancelled");
    assert!(!out.join("manifest.json").exists());
    assert!(out.join("failure.json").exists());
    assert!(out.join("inventory.json").exists());
}
