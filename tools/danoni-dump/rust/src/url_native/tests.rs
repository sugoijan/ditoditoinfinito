use super::*;
use crate::network::{TestReply, TestServer, test_fetcher};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ddi-url-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
fn site(_: &str) -> Result<String> {
    Ok("Original HTTP test fixture".into())
}
fn request(url: &str) -> UrlRequest {
    serde_json::from_value(json!({"url":url,"fetch":{"offline":true}})).unwrap()
}
#[test]
fn static_selection_keeps_flashvars_and_requires_unique_context() {
    let page=acquisition::inspect_page("https://example.org/work/page.html",br#"<object data="song.swf"><param name="flashvars" value="name=num301"></object><embed src="song.swf" flashvars="name=num301">"#,None).unwrap();
    let m = choose(&page, None).unwrap();
    assert_eq!(m.address.logical_url, "https://example.org/work/song.swf");
    assert_eq!(m.flash_vars["name"], "num301");
    let page = acquisition::inspect_page(
        "https://example.org/work/page.html",
        br#"<embed src="a.swf"><embed src="b.swf">"#,
        None,
    )
    .unwrap();
    assert_eq!(
        choose(&page, None).unwrap_err().code,
        "url-native-ambiguous"
    );
    assert_eq!(
        choose(&page, Some("https://example.org/work/b.swf"))
            .unwrap()
            .address
            .logical_url,
        "https://example.org/work/b.swf"
    );
    let page = acquisition::inspect_page(
        "https://example.org/work/page.html",
        br#"<embed src="a.swf" flashvars="name=a"><embed src="a.swf" flashvars="name=b">"#,
        None,
    )
    .unwrap();
    assert_eq!(
        choose(&page, Some("https://example.org/work/a.swf"))
            .unwrap_err()
            .code,
        "url-native-ambiguous"
    );
    let page = acquisition::inspect_page(
        "https://example.org/work/page.html",
        br#"<a href="next.html">Next</a><script>loadSomething()</script>"#,
        None,
    )
    .unwrap();
    assert_eq!(choose(&page, None).unwrap_err().code, "url-native-no-movie");
}
#[test]
fn acquisition_keeps_complete_evidence_and_shared_bytes_after_cache_changes() {
    let server = TestServer::new(|p| {
        if p == "/robots.txt" {
            TestReply::body(b"User-agent: *\nAllow: /\n")
        } else {
            TestReply::body(b"chart bytes")
        }
    });
    let scratch = Scratch::new();
    let root = scratch.0.join("evidence");
    fs::create_dir(&root).unwrap();
    let rt = runtime();
    let mut f = {
        let _enter = rt.enter();
        test_fetcher(
            &scratch.0.join("cache"),
            FetchOptions::default(),
            &[&server.origin],
        )
    };
    let cancel = NetworkCancel::default();
    let target = SourceAddress::parse(&format!("{}/dos.txt", server.origin)).unwrap();
    let response = acquire(&rt, &mut f, &target, 1000, &cancel, &mut site, &mut || {
        Ok(())
    })
    .unwrap();
    let evidence = Evidence::default();
    let receipt = evidence.archive(&root, &f, &response).unwrap();
    let body = evidence.body(&response.response().body_binding);
    assert_eq!(
        *evidence.read_shared("url", &body.path, 1000).unwrap(),
        b"chart bytes"
    );
    let provenance = evidence.read("url", &receipt.path, JSON_LIMIT).unwrap();
    assert_eq!(sha256(&provenance), receipt.sha256);
    let value: Value = json(&provenance).unwrap();
    let policy = &value["responses"][0]["selectedResponse"]["robots"][0]["decision"];
    for binding in [&policy["index"], &policy["body"]] {
        let b: Binding = serde_json::from_value(binding.clone()).unwrap();
        assert_eq!(
            sha256(&fs::read(root.join("cache").join(b.path)).unwrap()),
            b.sha256
        );
    }
    let next = SourceAddress::parse(&format!("{}/later", server.origin)).unwrap();
    acquire(&rt, &mut f, &next, 1000, &cancel, &mut site, &mut || Ok(())).unwrap();
    assert_eq!(
        evidence.read("url", &receipt.path, JSON_LIMIT).unwrap(),
        provenance
    );
    assert_eq!(server.paths(), vec!["/robots.txt", "/dos.txt", "/later"]);
}
#[test]
fn checkpoint_cancellation_drains_started_http_and_never_retries() {
    let server = TestServer::new(|p| {
        let mut r = TestReply::body(if p == "/robots.txt" {
            b"User-agent: *\nAllow: /\n"
        } else {
            b"slow data bytes"
        });
        r.trickle = p != "/robots.txt";
        r
    });
    let scratch = Scratch::new();
    let rt = runtime();
    let mut f = {
        let _enter = rt.enter();
        test_fetcher(
            &scratch.0.join("cache"),
            FetchOptions::default(),
            &[&server.origin],
        )
    };
    let target = SourceAddress::parse(&format!("{}/slow", server.origin)).unwrap();
    let cancel = NetworkCancel::default();
    let e = acquire(&rt, &mut f, &target, 1000, &cancel, &mut site, &mut || {
        if server.paths().iter().any(|p| p == "/slow") {
            Err(Error::new("host-cancelled", "fixture checkpoint"))
        } else {
            Ok(())
        }
    })
    .err()
    .unwrap();
    assert_eq!(e.code, "host-cancelled");
    let index: Value = json(&fs::read(scratch.0.join("cache/index.json")).unwrap()).unwrap();
    assert_eq!(
        index["failures"][&target.retrieval_url]["code"],
        "request-cancelled"
    );
    assert_eq!(server.paths(), vec!["/robots.txt", "/slow"]);
}
fn record(url: &str, body: &[u8], status: u16, headers: Value) -> Value {
    json!({"requestedUrl":url,"url":url,"finalUrl":url,"status":status,"headers":headers,"sha256":sha256(body),"bodyPath":format!("blobs/sha256/{}",sha256(body)),"byteLength":body.len(),"completeBody":true,"retrievedAt":"2026-10-14T12:00:00Z","redirects":[],"mimeType":"text/html","charset":"UTF-8"})
}
fn cached(root: &Path, entries: Vec<(&str, Vec<u8>, u16, Value)>) {
    fs::create_dir(root).unwrap();
    fs::create_dir_all(root.join("blobs/sha256")).unwrap();
    let mut records = BTreeMap::new();
    for (u, b, s, h) in entries {
        fs::write(root.join(format!("blobs/sha256/{}", sha256(&b))), &b).unwrap();
        records.insert(u.to_owned(), record(u, &b, s, h));
    }
    fs::write(
        root.join("index.json"),
        serde_json::to_vec(
            &json!({"schema":"ddi-danoni-cache/1","entries":records,"aliases":[],"failures":{}}),
        )
        .unwrap(),
    )
    .unwrap();
}
#[test]
fn archived_cached_redirect_is_checked_before_a_live_target_or_callback() {
    let scratch = Scratch::new();
    let url = "https://web.archive.org/web/20200101000000id_/http://example.org/movie.swf";
    cached(
        &scratch.0.join("cache"),
        vec![(
            url,
            b"redirect".to_vec(),
            302,
            json!({"location":"https://example.org/live.swf"}),
        )],
    );
    let mut f = Fetcher::open(
        &scratch.0.join("cache"),
        FetchOptions {
            offline: true,
            ..Default::default()
        },
    )
    .unwrap();
    let e = acquire(
        &runtime(),
        &mut f,
        &SourceAddress::parse(url).unwrap(),
        1000,
        &NetworkCancel::default(),
        &mut |_| panic!("Unexpected site callback"),
        &mut || Ok(()),
    )
    .err()
    .unwrap();
    assert_eq!(e.code, "archive-live-fallback");
    assert_eq!(f.counts().requests, 0);
}
#[test]
fn failed_static_discovery_preserves_evidence_without_a_ready_marker_or_worker_run() {
    let scratch = Scratch::new();
    let url = "https://example.org/page.html";
    cached(
        &scratch.0.join("cache"),
        vec![(
            url,
            b"<a href='other.html'>next</a>".to_vec(),
            200,
            json!({}),
        )],
    );
    let executable = scratch.0.join("never-run");
    fs::write(&executable, b"not an executable").unwrap();
    let worker = WorkerSpec {
        executable: executable.clone(),
        sha256: sha256(b"not an executable"),
        build_identity: executable,
        build_identity_sha256: sha256(b"not an executable"),
    };
    let out = execute(
        &request(url),
        &scratch.0.join("cache"),
        &worker,
        &scratch.0.join("out"),
        &NetworkCancel::default(),
        &mut |_| panic!("Offline policy"),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert!(!out.ready);
    assert_eq!(out.failure.unwrap().code, "url-native-no-movie");
    assert!(!out.output.join("manifest.json").exists());
    assert!(out.output.join("failure.json").exists());
    assert!(!out.output.join("native").exists());
    assert!(out.output.join("acquisition/http").exists());
    assert!(
        execute(
            &request(url),
            &scratch.0.join("cache"),
            &worker,
            &out.output,
            &NetworkCancel::default(),
            &mut site,
            &mut |_| Ok(())
        )
        .is_err()
    );
}
#[test]
fn pre_cancelled_and_invalid_requests_do_not_create_output_or_cache() {
    let scratch = Scratch::new();
    let cancel = NetworkCancel::default();
    cancel.cancel();
    let worker = WorkerSpec {
        executable: PathBuf::new(),
        sha256: String::new(),
        build_identity: PathBuf::new(),
        build_identity_sha256: String::new(),
    };
    for cancelled in [false, true] {
        let mut r = request("https://example.org/song.swf");
        if !cancelled {
            r.dependencies.attempts = 0;
        }
        assert!(
            execute(
                &r,
                &scratch.0.join("cache"),
                &worker,
                &scratch.0.join("out"),
                &cancel,
                &mut site,
                &mut |_| Ok(())
            )
            .is_err()
        );
        assert!(!scratch.0.join("out").exists());
        assert!(!scratch.0.join("cache").exists());
    }
}

#[test]
fn viewport_hint_selects_only_a_unique_largest_fully_known_context() {
    let conflicting = acquisition::inspect_page("https://example.org/page.html",br#"<embed src="game.swf" width="400" height="350" flashvars="chart=a"><embed src="game.swf" width="125" height="350" flashvars="chart=b">"#,None).unwrap();
    assert_eq!(
        choose(&conflicting, None).unwrap_err().code,
        "url-native-ambiguous"
    );

    for (other, expected) in [
        ("width='125' height='350'", true),
        ("width='400' height='350'", false),
        ("width='600' height='350'", false),
        ("width='100%' height='350'", false),
        ("width='0' height='350'", false),
        ("width='99999' height='350'", false),
    ] {
        let html = format!(
            "<object data='game.swf' width='400' height='350'></object><embed src='side.swf' {other}>"
        );
        let page =
            acquisition::inspect_page("https://example.org/page.html", html.as_bytes(), None)
                .unwrap();
        let result = choose(&page, None);
        assert_eq!(result.is_ok(), expected, "{other}");
        if expected {
            let movie = result.unwrap();
            assert_eq!(movie.viewport, Some([400, 350]));
            assert_eq!(movie.address.logical_url, "https://example.org/game.swf");
        }
    }
}
#[tokio::test]
async fn entered_runtime_returns_a_host_error_without_nested_runtime_panic() {
    let scratch = Scratch::new();
    let worker = WorkerSpec {
        executable: PathBuf::new(),
        sha256: String::new(),
        build_identity: PathBuf::new(),
        build_identity_sha256: String::new(),
    };
    let e = execute(
        &request("https://example.org/song.swf"),
        &scratch.0.join("cache"),
        &worker,
        &scratch.0.join("out"),
        &NetworkCancel::default(),
        &mut site,
        &mut |_| Ok(()),
    )
    .err()
    .unwrap();
    assert_eq!(e.code, "invalid-url-native");
    assert!(!scratch.0.join("cache").exists());
}

#[test]
fn passive_title_is_unique_complete_decoded_and_bounded() {
    let page = acquisition::inspect_page(
        "https://example.org/song.html",
        "<title>  譜面 &amp; Music\n Test </title><script>document.title='other'</script>"
            .as_bytes(),
        None,
    )
    .unwrap();
    assert_eq!(page.title.as_deref(), Some("譜面 & Music Test"));
    for html in [
        "<title>one</title><title>two</title>".to_owned(),
        "<title>unfinished".into(),
        format!("<title>{}</title>", "a".repeat(513)),
    ] {
        assert!(
            acquisition::inspect_page("https://example.org/", html.as_bytes(), None)
                .unwrap()
                .title
                .is_none()
        );
    }
}

#[test]
fn host_colour_reviews_are_bounded_and_reject_before_worker_or_network_access() {
    let local = Scratch::new();
    let review = ReviewedColor {
        work: "reviewed".into(),
        source_swf_sha256: "a".repeat(64),
        url: "https://example.org/iro.txt".into(),
        response_sha256: "b".repeat(64),
    };
    let worker = WorkerSpec {
        executable: local.0.join("never-read"),
        sha256: String::new(),
        build_identity: local.0.join("never-read-identity"),
        build_identity_sha256: String::new(),
    };
    for variant in 0..5 {
        let mut req = UrlRequest::new("https://example.org/work.html");
        req.optional_color_reviews = vec![review.clone()];
        match variant {
            0 => req.optional_color_reviews = vec![review.clone(); 17],
            1 => req.optional_color_reviews.push(review.clone()),
            2 => req.optional_color_reviews[0].response_sha256 = "unbound".into(),
            3 => req.optional_color_reviews[0].source_swf_sha256 = "unbound".into(),
            _ => req.optional_color_reviews[0].url = "https://example.org/required-data.txt".into(),
        }
        let output = local.0.join(format!("output-{variant}"));
        let err = execute(
            &req,
            &local.0.join("cache"),
            &worker,
            &output,
            &NetworkCancel::default(),
            &mut |_| panic!("No site access"),
            &mut |_| panic!("No job progress"),
        )
        .unwrap_err();
        assert_eq!(err.code, "invalid-url-native");
        assert!(!output.exists());
    }
}
