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
}
impl Audit {
    fn record(&self, kind: &str, detail: &str) {
        let mut data = self.0.lock().unwrap();
        if kind == "runtime-error" {
            data.runtime_errors += 1;
        }
        if data.entries.len() == 256 {
            data.dropped += 1;
            return;
        }
        let truncated = detail.chars().count() > 1024;
        data.truncated_details += usize::from(truncated);
        data.entries.push(json!({"kind":kind,"detail":detail.chars().take(1024).collect::<String>(),"truncated":truncated}));
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
        if request.method() == NavigationMethod::Get
            && request.body().is_none()
            && let Ok(url) = self.resolve_url(request.url())
            && let Some(resource) = self.resources.get(url.as_str())
        {
            self.audit.record("cache-hit", url.as_str());
            return async_return(Ok(Box::new(resource.clone()) as Box<dyn SuccessResponse>));
        }
        self.audit.record("fetch-denied", request.url());
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
