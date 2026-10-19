#![cfg(feature = "native")]
use ddi_danoni_export::{Error, Result, acquisition::*, assets::*};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};
const PAGE: &str = "https://example.org/list/work.html?chart=2";
#[test]
fn addresses_keep_queries_and_explicit_archive_identity() {
    let a = SourceAddress::parse(
        "https://web.archive.org/web/20050607080910id_/http://example.org/a/play.html?id=3#title",
    )
    .unwrap();
    assert_eq!(a.logical_url, "http://example.org/a/play.html?id=3");
    assert_eq!(a.capture.as_deref(), Some("20050607080910"));
    let movie = a.resolve("../song.swf?name=num1&chart=2").unwrap();
    assert_eq!(
        movie.logical_url,
        "http://example.org/song.swf?name=num1&chart=2"
    );
    assert_eq!(
        movie.retrieval_url,
        "https://web.archive.org/web/20050607080910id_/http://example.org/song.swf?name=num1&chart=2"
    );
    let mixed = a
        .resolve("/web/20060102030405/http://example.org/other.html")
        .unwrap();
    assert_eq!(mixed.capture.as_deref(), Some("20060102030405"));
    assert_eq!(mixed.logical_url, "http://example.org/other.html");
    assert_eq!(
        a.resolve("https://web.archive.org/web/20070102030405id_/http://example.org/third")
            .unwrap()
            .capture
            .as_deref(),
        Some("20070102030405")
    );
    assert_eq!(
        SourceAddress::parse(PAGE)
            .unwrap()
            .resolve("?chart=7#part")
            .unwrap()
            .logical_url,
        "https://example.org/list/work.html?chart=7"
    );
}
#[test]
fn addresses_reject_credentials_controls_and_implicit_archives() {
    assert!(SourceAddress::parse(&format!("https://example.org/{}", "曲".repeat(1300))).is_err());
    for url in [
        "file:///tmp/x",
        "https://u:p@example.org/",
        "https://example.org/\n",
        "https://web.archive.org/web/*/http://example.org/",
        "https://web.archive.org/web/2005/http://example.org/",
        "https://web.archive.org/web/20050607080910if_/http://example.org/",
        "https://web.archive.org/web/20050607080910id_/https://web.archive.org/web/20050607080910/http://example.org/",
    ] {
        assert!(SourceAddress::parse(url).is_err(), "{url}");
    }
}
#[test]
fn parses_passive_html_noscript_flashvars_frames_and_document_base() {
    let html = br##"<a href='before.html'>before</a><base href='../songs/'>
    <object><param name='movie' value='shared.swf?name=one'>
      <param name='FlashVars' value='dos=one.txt&amp;title=A+B&amp;q=%E6%9B%B2'></object>
    <noscript><embed src='other.SWF?v=2' flashvars='dos=two.txt'></noscript>
    <frame src='../index.html'><iframe src='?song=3'></iframe>
    <a href='#top'>top</a><a href='mailto:x@example.org'>mail</a><a href='direct.swf'>direct</a>
    <script src='../engine.js'></script><script>document.write("<embed src='fake.swf'>")</script>
    <style>body:before {content:"<a href='fake.html'>"}</style>
    <textarea><embed src='fake2.swf'></textarea>"##;
    let p = inspect_page(PAGE, html, None).unwrap();
    assert_eq!(p.base.logical_url, "https://example.org/songs/");
    assert_eq!(p.links.len(), 3);
    assert_eq!(
        p.links[0].address.logical_url,
        "https://example.org/songs/before.html"
    );
    assert_eq!(p.movies.len(), 3);
    let m = p.movies.iter().find(|m| m.kind == "object").unwrap();
    assert_eq!(
        m.address.logical_url,
        "https://example.org/songs/shared.swf?name=one"
    );
    assert_eq!(m.flash_vars["title"], "A B");
    assert_eq!(m.flash_vars["q"], "曲");
    assert_eq!(p.scripts[0].logical_url, "https://example.org/engine.js");
    assert!(p.has_inline_script);
    assert_eq!(p.diagnostics.len(), 1);
    assert_eq!(p.body_sha256, sha256(html));
}
#[test]
fn scopes_object_parameters_and_rejects_ambiguous_flashvars() {
    let p=inspect_page(PAGE,br#"<param name='movie' value='orphan.swf'><object data='outer.swf'><param name='flashvars' value='id=one'><object><param name='movie' value='inner.swf'><param name='flashvars' value='id=two'></object></object><embed src='bad.swf' flashvars='id=a&amp;id=b'><object><param name='movie' value='a.swf'><param name='movie' value='b.swf'></object>"#,None).unwrap();
    assert_eq!(p.movies.len(), 2);
    assert_eq!(p.movies[0].flash_vars["id"], "two");
    assert_eq!(p.movies[1].flash_vars["id"], "one");
    assert_eq!(p.diagnostics.len(), 2);
}
#[test]
fn decodes_legacy_meta_http_charset_and_bom_without_silent_replacement() {
    let html = "<meta http-equiv='Content-Type' content='text/html; charset=Shift_JIS'><embed src='song.swf' flashvars='title=曲'>";
    let (bytes, _, bad) = encoding_rs::SHIFT_JIS.encode(html);
    assert!(!bad);
    let p = inspect_page(PAGE, &bytes, None).unwrap();
    assert_eq!(p.encoding, "Shift_JIS");
    assert_eq!(p.movies[0].flash_vars["title"], "曲");
    let (raw, _, _) = encoding_rs::SHIFT_JIS.encode("<embed src='song.swf' flashvars='title=曲'>");
    assert!(inspect_page(PAGE, &raw, None).is_err());
    assert!(inspect_page(PAGE, &raw, Some("cp932")).is_ok());
    assert!(
        inspect_page(
            PAGE,
            b"\xef\xbb\xbf<embed src='song.swf'>",
            Some("Shift_JIS")
        )
        .is_ok()
    );
    assert!(inspect_page(PAGE, b"abc", Some("imaginary-encoding")).is_err());
}
#[test]
fn bounded_pages_and_element_lists_reject_without_partial_results() {
    assert_eq!(
        inspect_page(PAGE, &vec![b' '; PAGE_LIMIT + 1], None)
            .unwrap_err()
            .code,
        "discovery-limit"
    );
    assert!(inspect_page(PAGE, "<a href='x'>x</a>".repeat(2049).as_bytes(), None).is_err());
    assert!(inspect_page(PAGE, "<object>".repeat(65).as_bytes(), None).is_err());
    let p = inspect_page(
        PAGE,
        format!("<embed src='song.swf' flashvars='{}'>", "a".repeat(17000)).as_bytes(),
        None,
    )
    .unwrap();
    assert!(p.movies.is_empty());
    assert_eq!(p.diagnostics.len(), 1);
}
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
    reads: RefCell<Vec<String>>,
}
impl AssetSource for Memory {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        assert_eq!(ns, "source");
        relative_path(path)?;
        self.reads.borrow_mut().push(path.into());
        let b = self.files.get(path).ok_or_else(|| Error {
            code: "missing",
            message: path.into(),
        })?;
        assert!(b.len() <= max);
        Ok(b.clone())
    }
}
fn cache(record: Option<Value>) -> (Memory, CachedPageRequest) {
    let html = b"<embed src='song.swf?name=3' flashvars='dos=chart.txt'>".to_vec();
    let record=record.unwrap_or_else(||json!({"requestedUrl":PAGE,"finalUrl":"https://example.org/redirect/work.html?chart=2","status":200,"completeBody":true,"retrievedAt":"2026-10-10T00:00:00Z","mimeType":"text/html","charset":null,"sha256":sha256(&html),"byteLength":html.len(),"localPath":"inputs/page.html","robots":[{"status":404}],"requestContext":{"kind":"browser","headers":{"referer":"https://example.org/list/"}}}));
    let manifest = serde_json::to_vec(
        &json!({"schema":"ddi-danoni-single-source/experimental-1","resources":[record]}),
    )
    .unwrap();
    let request = CachedPageRequest {
        manifest: Binding {
            namespace: "source".into(),
            path: "manifest.json".into(),
            sha256: sha256(&manifest),
        },
        url: PAGE.into(),
        encoding_override: None,
    };
    (
        Memory {
            files: BTreeMap::from([
                ("manifest.json".into(), manifest),
                ("inputs/page.html".into(), html),
            ]),
            reads: RefCell::new(vec![]),
        },
        request,
    )
}
#[test]
fn cached_pages_verify_bytes_preserve_record_and_use_final_base() {
    let (source, request) = cache(None);
    let got = inspect_cached_page(&source, &request).unwrap();
    assert_eq!(
        got.discovery.movies[0].address.logical_url,
        "https://example.org/redirect/song.swf?name=3"
    );
    assert_eq!(got.resource["requestContext"]["kind"], "browser");
    assert_eq!(got.resource["robots"][0]["status"], 404);
    assert_eq!(
        *source.reads.borrow(),
        vec!["manifest.json", "inputs/page.html"]
    );
}
#[test]
fn cached_pages_reject_tampering_partial_errors_and_unsafe_paths() {
    for field in [
        "status",
        "completeBody",
        "sha256",
        "byteLength",
        "localPath",
        "mimeType",
        "retrievedAt",
        "requestedUrl",
    ] {
        let (source, _) = cache(None);
        let mut doc: Value = serde_json::from_slice(&source.files["manifest.json"]).unwrap();
        doc["resources"][0][field] = match field {
            "status" => json!(206),
            "completeBody" => json!(false),
            "sha256" => json!("a".repeat(64)),
            "byteLength" => json!(999),
            "localPath" => json!("../escape"),
            "mimeType" => json!("application/javascript"),
            "retrievedAt" => json!(""),
            _ => json!("https://example.org/other"),
        };
        let (source, request) = cache(Some(doc["resources"][0].clone()));
        assert!(inspect_cached_page(&source, &request).is_err(), "{field}");
    }
}

#[test]
fn local_cached_page_corpus() {
    let Ok(path) = std::env::var("DDI_DANONI_PAGE_CASES") else {
        return;
    };
    let doc: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(doc["schema"], "ddi-danoni-passive-discovery-cases/1");
    let mut movies = 0;
    for case in doc["cases"].as_array().unwrap() {
        let roots = serde_json::from_value(case["job"]["roots"].clone()).unwrap();
        let source = ddi_danoni_export::native::DirectorySource::new(roots).unwrap();
        let request: CachedPageRequest =
            serde_json::from_value(case["job"]["request"].clone()).unwrap();
        let result = inspect_cached_page(&source, &request);
        if let Some(code) = case["expectedError"].as_str() {
            assert_eq!(result.err().unwrap().code, code, "{}", case["id"]);
        } else {
            let page = result.unwrap_or_else(|e| panic!("{}: {e}", case["id"]));
            assert!(
                page.discovery
                    .movies
                    .iter()
                    .any(|m| m.address.logical_url == case["expectedMovie"].as_str().unwrap()),
                "{}",
                case["id"]
            );
            movies += 1;
        }
    }
    println!(
        "{} source-bound cached page cases, {movies} matched work SWFs; no network",
        doc["cases"].as_array().unwrap().len()
    );
}

#[test]
fn versioned_resource_manifests_accept_body_paths_and_reject_ambiguity() {
    let (mut source, mut request) = cache(None);
    let mut doc: Value = serde_json::from_slice(&source.files["manifest.json"]).unwrap();
    doc["schema"] = json!("ddi-danoni-resources/1");
    let row = doc["resources"][0].as_object_mut().unwrap();
    let path = row.remove("localPath").unwrap();
    row.insert("bodyPath".into(), path);
    for kind in ["resources", "duplicate", "unknown"] {
        let mut d = doc.clone();
        if kind == "duplicate" {
            let r = d["resources"][0].clone();
            d["resources"].as_array_mut().unwrap().push(r);
        }
        if kind == "unknown" {
            d["schema"] = json!("ddi-danoni-resources/999");
        }
        let bytes = serde_json::to_vec(&d).unwrap();
        request.manifest.sha256 = sha256(&bytes);
        source.files.insert("manifest.json".into(), bytes);
        assert_eq!(
            inspect_cached_page(&source, &request).is_ok(),
            kind == "resources"
        );
    }
}

#[test]
fn archived_body_records_never_invent_http_response_metadata() {
    let (mut source, mut request) = cache(None);
    let replay =
        "https://web.archive.org/web/20050607080910id_/https://example.org/list/work.html?chart=2";
    request.url = replay.into();
    let body = &source.files["inputs/page.html"];
    let doc = json!({"schema":"ddi-danoni-single-source/experimental-1","archive":{"retrievedOn":"2026-10-10"},"resources":[{"role":"page","logicalUrl":PAGE,"replayUrl":replay,"snapshot":"20050607080910","sha256":sha256(body),"localPath":"inputs/page.html"}]});
    for kind in ["valid", "snapshot", "logical", "date"] {
        let mut d = doc.clone();
        match kind {
            "snapshot" => d["resources"][0]["snapshot"] = json!("20060101000000"),
            "logical" => d["resources"][0]["logicalUrl"] = json!("https://example.org/wrong"),
            "date" => d["archive"]["retrievedOn"] = Value::Null,
            _ => {}
        }
        let bytes = serde_json::to_vec(&d).unwrap();
        request.manifest.sha256 = sha256(&bytes);
        source.files.insert("manifest.json".into(), bytes);
        let got = inspect_cached_page(&source, &request);
        if kind == "valid" {
            let page = got.unwrap();
            assert_eq!(page.response_evidence, "archive-body-only");
            assert!(page.resource.get("status").is_none());
            assert_eq!(
                page.discovery.movies[0].address.capture.as_deref(),
                Some("20050607080910")
            );
        } else {
            assert!(got.is_err(), "{kind}");
        }
    }
}

#[test]
fn pack_title_uses_a_unique_matching_heading_before_the_player() {
    let html = "<meta charset=shift_jis><title>第３回紅白FLASH合戦</title><p>戻る</p><p align=center>～　 第３回紅白FLASH合戦<font>提出作品</font>　～</p><p>DANCING☆ONIGIRI</p><object data=work.swf></object><p>第３回紅白FLASH合戦 後書き</p>";
    let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(html);
    let page = inspect_page(PAGE, &bytes, None).unwrap();
    assert_eq!(page.title.as_deref(), Some("第３回紅白FLASH合戦"));
    assert_eq!(
        page.pack_title.as_deref(),
        Some("第３回紅白FLASH合戦提出作品")
    );
    for heading in [
        "<p>Unrelated text</p>",
        "<p>Work A</p><p>Work B</p>",
        "<p>Work unfinished",
        "<p hidden>Work hidden</p>",
        "<p>Work<script> hidden</script></p>",
    ] {
        let html = format!("<title>Work</title>{heading}<embed src=work.swf>");
        assert_eq!(
            inspect_page(PAGE, html.as_bytes(), None)
                .unwrap()
                .pack_title
                .as_deref(),
            Some("Work")
        );
    }
    for html in [
        "<title>Work",
        "<title>A</title><title>B</title>",
        "<p>Some heading</p>",
    ] {
        assert!(
            inspect_page(PAGE, html.as_bytes(), None)
                .unwrap()
                .pack_title
                .is_none()
        );
    }
}
