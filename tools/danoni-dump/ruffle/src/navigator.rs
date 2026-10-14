use async_channel::{Receiver, Sender};
use ruffle_core::{
    backend::{log::LogBackend, navigator::*},
    indexmap::IndexMap,
    loader::Error,
    socket::{ConnectionState, SocketAction, SocketHandle},
};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use url::{ParseError, Url};

#[derive(Clone)]
pub struct CachedResource {
    url: String,
    body: Vec<u8>,
    encoding: Option<&'static encoding_rs::Encoding>,
}

pub fn read_resources(
    config: &Value,
    root: &Path,
) -> Result<HashMap<String, CachedResource>, Box<dyn std::error::Error>> {
    let mut resources = HashMap::new();
    let mut total = 0;
    let root = root.canonicalize()?;
    if let Some(entries) = config["resources"].as_array() {
        if entries.len() > 64 {
            return Err("too many cached resources".into());
        }
        for entry in entries {
            let url = Url::parse(entry["url"].as_str().ok_or("resource URL missing")?)?;
            if !matches!(url.scheme(), "https" | "http")
                || !url.has_host()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
            {
                return Err("invalid cached resource URL".into());
            }
            let path = root
                .join(entry["path"].as_str().ok_or("resource path missing")?)
                .canonicalize()?;
            if !path.starts_with(&root) {
                return Err("resource path escapes input directory".into());
            }
            let body = super::read_bounded(&path, 16 * 1024 * 1024)?;
            total += body.len();
            if total > 64 * 1024 * 1024 {
                return Err("cached resource byte limit".into());
            }
            let digest = super::hash(&body);
            if entry["sha256"].as_str() != Some(&digest) {
                return Err("cached resource hash mismatch".into());
            }
            let encoding = match entry["httpCharset"].as_str() {
                None => None,
                Some(label) => Some(
                    encoding_rs::Encoding::for_label(label.as_bytes())
                        .ok_or("unknown HTTP charset")?,
                ),
            };
            if resources
                .insert(
                    url.to_string(),
                    CachedResource {
                        url: url.to_string(),
                        body,
                        encoding,
                    },
                )
                .is_some()
            {
                return Err("duplicate resource URL".into());
            }
        }
    }
    Ok(resources)
}

impl SuccessResponse for CachedResource {
    fn url(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.url)
    }
    fn set_url(&mut self, url: String) {
        self.url = url;
    }
    fn body(self: Box<Self>) -> OwnedFuture<Vec<u8>, Error> {
        async_return(Ok(self.body))
    }
    fn text_encoding(&self) -> Option<&'static encoding_rs::Encoding> {
        self.encoding
    }
    fn status(&self) -> u16 {
        200
    }
    fn redirected(&self) -> bool {
        false
    }
    fn next_chunk(&mut self) -> OwnedFuture<Option<Vec<u8>>, Error> {
        async_return(Ok(if self.body.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.body))
        }))
    }
    fn expected_length(&self) -> Result<Option<u64>, Error> {
        Ok(Some(self.body.len() as u64))
    }
}

#[derive(Clone, Default)]
pub struct Audit(Arc<Mutex<AuditData>>);
#[derive(Default)]
struct AuditData {
    entries: Vec<Value>,
    dropped: usize,
    truncated_details: usize,
    runtime_errors: usize,
    iteration: u64,
}
impl Audit {
    pub fn set_iteration(&self, iteration: u64) {
        self.0.lock().unwrap().iteration = iteration;
    }
    fn record(&self, kind: &str, detail: &str) {
        self.record_request(kind, detail, None);
    }
    fn record_request(&self, kind: &str, detail: &str, mut request: Option<Value>) {
        let mut data = self.0.lock().unwrap();
        if kind == "runtime-error" {
            data.runtime_errors += 1;
        }
        if data.entries.len() == 256 {
            data.dropped += 1;
            return;
        }
        let mut truncated = detail.chars().count() > 1024;
        if let Some(request) = &mut request {
            for name in ["originalUrl", "resolvedUrl"] {
                if let Some(url) = request[name].as_str() {
                    truncated |= url.chars().count() > 1024;
                    request[name] = json!(url.chars().take(1024).collect::<String>());
                }
            }
        }
        data.truncated_details += usize::from(truncated);
        let iteration = data.iteration;
        let mut entry = json!({"iteration":iteration,"kind":kind,"detail":detail.chars().take(1024).collect::<String>(),"truncated":truncated});
        if let Some(request) = request {
            entry["request"] = request;
        }
        data.entries.push(entry);
    }
    pub fn report(&self) -> Value {
        let data = self.0.lock().unwrap();
        json!({"entries":data.entries,"droppedEntries":data.dropped,"truncatedDetails":data.truncated_details,
            "runtimeErrors":data.runtime_errors,"truncated":data.dropped > 0 || data.truncated_details > 0})
    }
}
impl LogBackend for Audit {
    fn avm_trace(&self, message: &str) {
        self.record("trace", message);
    }
    fn avm_warning(&self, message: &str) {
        self.record("warning", message);
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Audit {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if *event.metadata().level() > tracing::Level::WARN {
            return;
        }
        struct Fields(String);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                use std::fmt::Write;
                if self.0.len() < 2048 {
                    let _ = write!(self.0, "{}={value:?} ", field.name());
                }
            }
        }
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        self.record(
            if *event.metadata().level() == tracing::Level::ERROR {
                "runtime-error"
            } else {
                "runtime-warning"
            },
            &fields.0,
        );
    }
}

// Do not use NullNavigatorBackend: it can read local files.
pub struct CachedNavigator {
    base: Url,
    spawner: NullSpawner,
    audit: Audit,
    resources: HashMap<String, CachedResource>,
}
impl CachedNavigator {
    pub fn new(
        base: Url,
        executor: &NullExecutor,
        audit: Audit,
        resources: HashMap<String, CachedResource>,
    ) -> Self {
        Self {
            base,
            spawner: executor.spawner(),
            audit,
            resources,
        }
    }
}
impl NavigatorBackend for CachedNavigator {
    fn navigate_to_url(
        &self,
        url: &str,
        _target: &str,
        _vars: Option<(NavigationMethod, IndexMap<String, String>)>,
    ) {
        self.audit.record("navigation-denied", url);
    }
    fn fetch(&self, request: Request) -> OwnedFuture<Box<dyn SuccessResponse>, ErrorResponse> {
        let resolved = self.resolve_url(request.url()).ok();
        let reason = if request.method() != NavigationMethod::Get {
            "method-denied"
        } else if request.body().is_some() {
            "body-denied"
        } else if !request.headers().is_empty() {
            "headers-denied"
        } else if let Some(url) = &resolved {
            if !matches!(url.scheme(), "http" | "https")
                || !url.has_host()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
            {
                "unsupported-url"
            } else if let Some(resource) = self.resources.get(url.as_str()) {
                self.audit.record("cache-hit", url.as_str());
                return async_return(Ok(Box::new(resource.clone()) as Box<dyn SuccessResponse>));
            } else {
                "cache-miss"
            }
        } else {
            "invalid-url"
        };
        self.audit.record_request("fetch-denied", request.url(), Some(json!({
            "schema":"ddi-native-request/1", "method":request.method().to_string(),
            "hasBody":request.body().is_some(), "hasHeaders":!request.headers().is_empty(),
            "originalUrl":request.url(), "resolvedUrl":resolved.as_ref().map(Url::as_str), "reason":reason
        })));
        async_return(Err(create_fetch_error(
            request.url(),
            "no exact GET match in immutable resource cache",
        )))
    }
    fn resolve_url(&self, url: &str) -> Result<Url, ParseError> {
        self.base.join(url)
    }
    fn spawn_future(&mut self, future: OwnedFuture<(), Error>) {
        self.spawner.spawn_local(future);
    }
    fn pre_process_url(&self, url: Url) -> Url {
        url
    }
    fn connect_socket(
        &mut self,
        host: String,
        _port: u16,
        _timeout: Duration,
        handle: SocketHandle,
        _receiver: Receiver<Vec<u8>>,
        sender: Sender<SocketAction>,
    ) {
        self.audit.record("socket-denied", &host);
        let _ = sender.try_send(SocketAction::Connect(handle, ConnectionState::Failed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_distinguish_exact_get_misses_from_other_denials() {
        let executor = NullExecutor::new();
        let audit = Audit::default();
        audit.set_iteration(7);
        let navigator = CachedNavigator::new(
            Url::parse("https://example.test/game/source.swf?name=a").unwrap(),
            &executor,
            audit.clone(),
            HashMap::new(),
        );
        let requests = [
            Request::get("dos.txt?name=a".into()),
            Request::post("dos.txt".into(), None),
            Request::request(
                NavigationMethod::Get,
                "dos.txt".into(),
                Some((vec![], "text/plain".into())),
            ),
            Request::get("file:///tmp/secret".into()),
            Request::get("https://user:password@example.test/x".into()),
            Request::get("http://[".into()),
        ];
        for request in requests {
            drop(navigator.fetch(request));
        }
        let report = audit.report();
        let rows = report["entries"].as_array().unwrap();
        assert_eq!(rows[0]["iteration"], 7);
        assert_eq!(rows[0]["detail"], "dos.txt?name=a");
        assert_eq!(
            rows[0]["request"]["resolvedUrl"],
            "https://example.test/game/dos.txt?name=a"
        );
        for (row, reason) in rows.iter().zip([
            "cache-miss",
            "method-denied",
            "body-denied",
            "unsupported-url",
            "unsupported-url",
            "invalid-url",
        ]) {
            assert_eq!(row["request"]["reason"], reason);
        }
        assert_eq!(report["truncated"], false);
    }
    #[test]
    fn structured_urls_are_bounded_and_hits_remain_cached() {
        let executor = NullExecutor::new();
        let audit = Audit::default();
        let url = "https://example.test/a";
        let navigator = CachedNavigator::new(
            Url::parse(url).unwrap(),
            &executor,
            audit.clone(),
            HashMap::from([(
                url.into(),
                CachedResource {
                    url: url.into(),
                    body: vec![1],
                    encoding: None,
                },
            )]),
        );
        drop(navigator.fetch(Request::get(url.into())));
        drop(navigator.fetch(Request::get(format!("{}{}", url, "x".repeat(2000)))));
        let report = audit.report();
        assert_eq!(report["entries"][0]["kind"], "cache-hit");
        assert_eq!(report["truncated"], true);
        assert_eq!(report["truncatedDetails"], 1);
        assert_eq!(
            report["entries"][1]["request"]["originalUrl"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            1024
        );
    }
}
