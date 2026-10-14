use super::*;
use crate::assets::{AssetSource, Binding};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};
type Observed = Arc<Mutex<Vec<(String, BTreeMap<String, String>, std::time::Instant)>>>;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ddi-http-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn cache(&self) -> PathBuf {
        self.0.join("cache")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[derive(Clone)]
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
    pub(crate) trickle: bool,
    pub(crate) drop: bool,
}
impl Reply {
    pub(crate) fn body(body: &[u8]) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())],
            body: body.to_vec(),
            trickle: false,
            drop: false,
        }
    }
    pub(crate) fn redirect(to: &str) -> Self {
        let mut r = Self::body(b"redirect");
        r.status = 302;
        r.headers.push(("location".into(), to.into()));
        r
    }
}
pub(crate) struct Server {
    pub(crate) origin: String,
    pub(crate) requests: Observed,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    pub(crate) fn new(handler: impl Fn(&str) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(vec![]));
        let observed = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let handler = Arc::new(handler);
        let worker = thread::spawn(move || {
            let mut clients = vec![];
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let h = handler.clone();
                        let r = observed.clone();
                        clients.push(thread::spawn(move || serve(stream, h.as_ref(), r)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
            for c in clients {
                let _ = c.join();
            }
        });
        Self {
            origin,
            requests,
            stop,
            thread: Some(worker),
        }
    }
    pub(crate) fn paths(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.0.clone())
            .collect()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn serve(mut stream: TcpStream, handler: &dyn Fn(&str) -> Reply, requests: Observed) {
    // Darwin may inherit the listener's nonblocking flag on accepted sockets.
    // The fixture uses blocking BufRead; a transient WouldBlock is not EOF.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut read = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if read.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    if parts.next() != Some("GET") {
        return;
    }
    let Some(path) = parts.next().map(str::to_owned) else {
        return;
    };
    let mut headers = BTreeMap::new();
    loop {
        line.clear();
        if read.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        if line == "\r\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.to_ascii_lowercase(), v.trim().to_owned());
        }
    }
    requests
        .lock()
        .unwrap()
        .push((path.clone(), headers, std::time::Instant::now()));
    let reply = handler(&path);
    if reply.drop {
        return;
    }
    let mut head = format!(
        "HTTP/1.1 {} Test\r\nConnection: close\r\nContent-Length: {}\r\n",
        reply.status,
        reply.body.len()
    );
    for (k, v) in reply.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    if reply.trickle {
        for byte in reply.body {
            if stream.write_all(&[byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    } else {
        let _ = stream.write_all(&reply.body);
    }
}
fn request(url: &str) -> ReplayRequest {
    ReplayRequest {
        url: url.into(),
        context: RequestContext::default(),
        maximum_bytes: 1024 * 1024,
    }
}
fn test_fetcher(path: &Path, mut options: FetchOptions, origins: &[&str]) -> Fetcher {
    options.transport.dns_server = Some("100.100.100.100".parse().unwrap());
    let mut f = Fetcher::open(path, options).unwrap();
    let mut t = HttpTransport::new(f.options.transport.clone()).unwrap();
    t.test_origins = origins.iter().map(|s| s.to_string()).collect();
    f.transport = Some(t);
    f
}
fn allow(_: &str) -> Result<String> {
    Ok("Original public HTTP fixture".into())
}
fn index(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path.join("index.json")).unwrap()).unwrap()
}
#[tokio::test]
async fn live_acquisition_restarts_offline_and_keeps_stable_provenance() {
    let server = Server::new(|p| match p {
        "/robots.txt" => Reply::body(b"User-agent: *\nDisallow: /private\n"),
        "/start" => Reply::redirect("/song"),
        _ => Reply::body(b"chart data"),
    });
    let root = Scratch::new();
    let mut f = test_fetcher(&root.cache(), FetchOptions::default(), &[&server.origin]);
    let cancel = NetworkCancel::default();
    assert_eq!(
        Fetcher::open(&root.cache(), FetchOptions::default())
            .err()
            .unwrap()
            .code,
        "cache-locked"
    );
    let result = f
        .fetch(
            &request(&format!("{}/start", server.origin)),
            &cancel,
            &mut allow,
        )
        .await
        .unwrap();
    assert_eq!(result.responses.len(), 2);
    assert_eq!(*result.response().body, b"chart data");
    assert_eq!(f.counts.requests, 3);
    let binding = result.responses[0].index.clone();
    f.fetch(
        &request(&format!("{}/other", server.origin)),
        &cancel,
        &mut allow,
    )
    .await
    .unwrap();
    assert_eq!(
        sha256(&fs::read(root.cache().join(&binding.path)).unwrap()),
        binding.sha256
    );
    {
        let actual = server.requests.lock().unwrap();
        for r in actual.iter() {
            assert_eq!(r.1["user-agent"], USER_AGENT);
            assert_eq!(r.1["accept-encoding"], ACCEPT_ENCODING);
            assert!(!r.1.contains_key("cookie") && !r.1.contains_key("authorization"));
        }
        for pair in actual.windows(2) {
            assert!(pair[1].2.duration_since(pair[0].2) >= Duration::from_millis(990));
        }
    }
    let before = server.paths();
    drop(f);
    let mut offline = Fetcher::open(
        &root.cache(),
        FetchOptions {
            offline: true,
            ..Default::default()
        },
    )
    .unwrap();
    let cached = offline
        .fetch(
            &request(&format!("{}/start", server.origin)),
            &cancel,
            &mut |_| panic!("Offline lookup consulted live site policy"),
        )
        .await
        .unwrap();
    assert_eq!(*cached.response().body, b"chart data");
    assert_eq!(offline.counts.requests, 0);
    assert_eq!(before, server.paths());
}
#[tokio::test]
async fn denied_robots_and_cross_origin_terms_never_fetch_targets() {
    let target = Server::new(|_| Reply::body(b"User-agent: *\nDisallow: /private\n"));
    let to = format!("{}/private/chart", target.origin);
    let source = Server::new(move |p| {
        if p == "/robots.txt" {
            Reply::body(b"User-agent: *\nAllow: /\n")
        } else {
            Reply::redirect(&to)
        }
    });
    let root = Scratch::new();
    let mut f = test_fetcher(
        &root.cache(),
        FetchOptions::default(),
        &[&source.origin, &target.origin],
    );
    let cancel = NetworkCancel::default();
    let q = request(&format!("{}/start", source.origin));
    assert_eq!(
        f.fetch(&q, &cancel, &mut allow).await.err().unwrap().code,
        "robots-denied"
    );
    assert_eq!(target.paths(), vec!["/robots.txt"]);
    let count = f.counts.requests;
    assert_eq!(
        f.fetch(&q, &cancel, &mut allow).await.err().unwrap().code,
        "cached-request-failure"
    );
    assert_eq!(f.counts.requests, count);
    drop(f);
    let root2 = Scratch::new();
    let mut f = test_fetcher(
        &root2.cache(),
        FetchOptions::default(),
        &[&source.origin, &target.origin],
    );
    let allowed = source.origin.clone();
    let before = target.paths();
    assert_eq!(
        f.fetch(&q, &cancel, &mut |u| if u.starts_with(&allowed) {
            allow(u)
        } else {
            Err(Error::new("site-terms-denied", "Fixture origin refused"))
        })
        .await
        .err()
        .unwrap()
        .code,
        "site-terms-denied"
    );
    assert_eq!(before, target.paths());
}
#[tokio::test]
async fn cancellation_before_dispatch_leaves_no_attempt_and_restart_keeps_pacing() {
    let server = Server::new(|_| Reply::body(b"User-agent: *\nAllow: /\n"));
    let root = Scratch::new();
    let mut f = test_fetcher(&root.cache(), FetchOptions::default(), &[&server.origin]);
    let q = request(&format!("{}/chart", server.origin));
    let cancel = NetworkCancel::default();
    let trigger = cancel.clone();
    let mut site = allow;
    let (result, ()) = tokio::join!(f.fetch(&q, &cancel, &mut site), async {
        while server.paths().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
        trigger.cancel();
    });
    assert_eq!(result.err().unwrap().code, "request-cancelled");
    assert!(
        index(&root.cache())["failures"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert_eq!(server.paths(), vec!["/robots.txt"]);
    drop(f);
    let mut f = test_fetcher(&root.cache(), FetchOptions::default(), &[&server.origin]);
    f.fetch(&q, &NetworkCancel::default(), &mut allow)
        .await
        .unwrap();
    let r = server.requests.lock().unwrap();
    assert!(r[1].2.duration_since(r[0].2) >= Duration::from_millis(990));
}
#[tokio::test]
async fn incomplete_attempts_and_started_cancellation_are_not_retried() {
    let server = Server::new(|p| {
        let mut r = Reply::body(if p == "/robots.txt" {
            b"User-agent: *\nAllow: /\n"
        } else {
            b"a long slowly sent chart"
        });
        if p != "/robots.txt" {
            r.trickle = true;
        }
        r
    });
    let root = Scratch::new();
    let q = request(&format!("{}/slow", server.origin));
    let mut f = test_fetcher(&root.cache(), FetchOptions::default(), &[&server.origin]);
    let cancel = NetworkCancel::default();
    let trigger = cancel.clone();
    let mut site = allow;
    let (result, ()) = tokio::join!(f.fetch(&q, &cancel, &mut site), async {
        while !server.paths().contains(&"/slow".to_string()) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        trigger.cancel();
    });
    assert_eq!(result.err().unwrap().code, "request-cancelled");
    assert_eq!(
        index(&root.cache())["failures"][&q.url]["code"],
        "request-cancelled"
    );
    drop(f);
    let mut f = test_fetcher(&root.cache(), FetchOptions::default(), &[&server.origin]);
    assert_eq!(
        f.fetch(&q, &NetworkCancel::default(), &mut allow)
            .await
            .err()
            .unwrap()
            .code,
        "cached-request-failure"
    );
    assert_eq!(server.paths().iter().filter(|p| *p == "/slow").count(), 1);
    drop(f);
    let root2 = Scratch::new();
    let mut store = CacheStore::open(&root2.cache()).unwrap();
    store.begin(&q.url).unwrap();
    drop(store);
    let mut f = test_fetcher(&root2.cache(), FetchOptions::default(), &[&server.origin]);
    assert_eq!(
        f.fetch(&q, &NetworkCancel::default(), &mut allow)
            .await
            .err()
            .unwrap()
            .code,
        "cached-request-failure"
    );
    assert!(
        index(&root2.cache())["failures"][&q.url]
            .get("recoveredAt")
            .is_some()
    );
}
#[tokio::test]
async fn fresh_unsupported_vary_is_used_once_and_corruption_never_refetches() {
    let server = Server::new(|p| {
        let mut r = Reply::body(if p == "/robots.txt" {
            b"User-agent: *\nAllow: /\n"
        } else {
            b"data"
        });
        if p != "/robots.txt" {
            r.headers.push(("vary".into(), "*".into()));
        }
        r
    });
    let root = Scratch::new();
    let mut f = test_fetcher(&root.cache(), FetchOptions::default(), &[&server.origin]);
    let q = request(&format!("{}/varied", server.origin));
    let cancel = NetworkCancel::default();
    let result = f.fetch(&q, &cancel, &mut allow).await.unwrap();
    assert_eq!(*result.response().body, b"data");
    assert_eq!(
        f.fetch(&q, &cancel, &mut allow).await.err().unwrap().code,
        "cache-vary-unsupported"
    );
    assert_eq!(server.paths().len(), 2);
    drop(f);
    let doc = index(&root.cache());
    let hash = doc["entries"][&q.url]["sha256"].as_str().unwrap();
    fs::write(root.cache().join(format!("blobs/sha256/{hash}")), b"edit").unwrap();
    let mut doc = doc;
    doc["entries"][&q.url]["headers"]
        .as_object_mut()
        .unwrap()
        .remove("vary");
    fs::write(
        root.cache().join("index.json"),
        serde_json::to_vec(&doc).unwrap(),
    )
    .unwrap();
    let mut f = test_fetcher(&root.cache(), FetchOptions::default(), &[&server.origin]);
    assert_eq!(
        f.fetch(&q, &cancel, &mut allow).await.err().unwrap().code,
        "changed-asset"
    );
    assert_eq!(server.paths().len(), 2);
}
#[tokio::test]
async fn stale_robots_and_exhausted_budgets_stop_before_resource_dispatch() {
    let server = Server::new(|_| Reply::body(b"User-agent: *\nAllow: /\n"));
    let root = Scratch::new();
    let mut f = test_fetcher(
        &root.cache(),
        FetchOptions {
            maximum_requests: 1,
            ..Default::default()
        },
        &[&server.origin],
    );
    let q = request(&format!("{}/chart", server.origin));
    let cancel = NetworkCancel::default();
    assert_eq!(
        f.fetch(&q, &cancel, &mut allow).await.err().unwrap().code,
        "network-limit"
    );
    assert_eq!(server.paths(), vec!["/robots.txt"]);
    drop(f);
    let mut doc = index(&root.cache());
    let policy = format!("{}/robots.txt", server.origin);
    doc["entries"][&policy]["retrievedAt"] = json!("2005-01-01T00:00:00Z");
    fs::write(
        root.cache().join("index.json"),
        serde_json::to_vec(&doc).unwrap(),
    )
    .unwrap();
    let mut f = test_fetcher(&root.cache(), FetchOptions::default(), &[&server.origin]);
    assert_eq!(
        f.fetch(&q, &cancel, &mut allow).await.err().unwrap().code,
        "robots-stale"
    );
    assert_eq!(server.paths(), vec!["/robots.txt"]);
}
#[tokio::test]
async fn production_configuration_cannot_fetch_loopback_or_enable_browser_headers() {
    let server = Server::new(|_| Reply::body(b"data"));
    let root = Scratch::new();
    let mut options = FetchOptions::default();
    options.transport.dns_server = Some("100.100.100.100".parse().unwrap());
    let mut f = Fetcher::open(&root.cache(), options).unwrap();
    let q = request(&format!("{}/chart", server.origin));
    assert_eq!(
        f.fetch(&q, &NetworkCancel::default(), &mut allow)
            .await
            .err()
            .unwrap()
            .code,
        "private-network"
    );
    assert!(server.paths().is_empty());
    drop(f);
    let root2 = Scratch::new();
    let mut f = test_fetcher(&root2.cache(), FetchOptions::default(), &[&server.origin]);
    let mut q = q;
    q.context.kind = ContextKind::Browser;
    assert_eq!(
        f.fetch(&q, &NetworkCancel::default(), &mut allow)
            .await
            .err()
            .unwrap()
            .code,
        "request-context-denied"
    );
    assert!(server.paths().is_empty());
}
#[test]
fn cache_detects_uncooperative_writers_symlinks_and_preserves_shared_bytes() {
    let root = Scratch::new();
    let mut store = CacheStore::open(&root.cache()).unwrap();
    fs::write(root.cache().join("index.json"), b"{}").unwrap();
    assert_eq!(
        store.begin("https://example.org/song").err().unwrap().code,
        "cache-integrity"
    );
    assert_eq!(
        store
            .lookup(&request("https://example.org/song"), false)
            .err()
            .unwrap()
            .code,
        "cache-write-failed"
    );
    let root2 = Scratch::new();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&root.0, root2.cache()).unwrap();
        assert!(CacheStore::open(&root2.cache()).is_err());
    }
    let root3 = Scratch::new();
    let mut s = CacheStore::open(&root3.cache()).unwrap();
    let url = "https://example.org/song";
    let id = s.begin(url).unwrap();
    let body = Arc::new(b"payload".to_vec());
    let hash = sha256(&body);
    s.success(url,&id,json!({"requestedUrl":url,"url":url,"finalUrl":url,"status":200,"headers":{},"sha256":hash,"bodyPath":format!("blobs/sha256/{hash}"),"byteLength":body.len(),"completeBody":true,"retrievedAt":stamp().unwrap(),"redirects":[]}),body).unwrap();
    let a = match s.lookup(&request(url), false).unwrap() {
        ReplayLookup::Hit(r) => r,
        _ => panic!(),
    };
    let b = match s.lookup(&request(url), false).unwrap() {
        ReplayLookup::Hit(r) => r,
        _ => panic!(),
    };
    assert!(Arc::ptr_eq(&a.body, &b.body));
    let source =
        crate::native::DirectorySource::new(BTreeMap::from([("cache".into(), root3.cache())]))
            .unwrap();
    assert_eq!(
        sha256(
            &source
                .read("cache", &a.index.path, crate::assets::JSON_LIMIT)
                .unwrap()
        ),
        a.index.sha256
    );
    let _: Binding = a.body_binding;
}

#[test]
fn writer_admits_historical_optional_collections_without_losing_unknown_fields() {
    let root = Scratch::new();
    fs::create_dir(root.cache()).unwrap();
    fs::write(
        root.cache().join("index.json"),
        br#"{"schema":"ddi-danoni-cache/1","entries":{},"review":"retained"}"#,
    )
    .unwrap();
    let mut cache = CacheStore::open(&root.cache()).unwrap();
    cache.begin("https://example.org/attempt").unwrap();
    let doc = index(&root.cache());
    assert_eq!(doc["review"], "retained");
    assert!(doc["aliases"].as_array().unwrap().is_empty());
    assert_eq!(
        doc["failures"]["https://example.org/attempt"]["code"],
        "request-incomplete"
    );
}
