use super::control::NetworkCancel;
use super::*;
use crate::{Result, assets::MEDIA_LIMIT};
use hickory_resolver::{
    TokioResolver,
    config::{LookupIpStrategy, NameServerConfig, ResolverConfig},
    net::runtime::TokioRuntimeProvider,
};
use reqwest::{
    Client,
    dns::{Name, Resolve, Resolving},
};
use serde_json::{Value, json};
use std::{
    io::{Cursor, Read},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, BufReader, ReadBuf},
    time::Instant,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootBundle {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TransportOptions {
    pub dns_server: Option<IpAddr>,
    pub tls_root_bundle: Option<RootBundle>,
    pub request_timeout_ms: u64,
}
impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            dns_server: None,
            tls_root_bundle: None,
            request_timeout_ms: 30000,
        }
    }
}
pub(crate) struct HttpTransport {
    dns: TokioResolver,
    roots: Option<Vec<reqwest::Certificate>>,
    trust: Value,
    pub options: TransportOptions,
    #[cfg(test)]
    pub test_origins: std::collections::BTreeSet<String>,
}
pub(crate) struct Prepared {
    client: Client,
    url: String,
    addresses: Vec<SocketAddr>,
}
pub(crate) struct HttpResponse {
    pub body: Arc<Vec<u8>>,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub network: Value,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferCounts {
    pub wire_bytes: u64,
    pub decoded_bytes: u64,
    pub requests: usize,
}
struct NoDns;
impl Resolve for NoDns {
    fn resolve(&self, _name: Name) -> Resolving {
        Box::pin(async { Err(std::io::Error::other("Second DNS resolution denied").into()) })
    }
}
fn network(e: impl std::error::Error) -> Error {
    let mut message = e.to_string();
    let mut source = e.source();
    while let Some(cause) = source {
        if message.len() >= 2048 {
            break;
        }
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    Error::new("http-error", message.chars().take(4096).collect::<String>())
}
impl HttpTransport {
    pub fn new(options: TransportOptions) -> Result<Self> {
        if !(1..=60000).contains(&options.request_timeout_ms) {
            return Err(Error::new(
                "network-limit",
                "Request deadline must be 1..60000 ms",
            ));
        }
        let (roots, trust) = if let Some(bundle) = &options.tls_root_bundle {
            if !bundle.path.is_absolute() || !crate::assets::valid_hash(&bundle.sha256) {
                return Err(Error::new(
                    "tls-root-bundle",
                    "Need an absolute host CA bundle path and SHA-256",
                ));
            }
            let meta = std::fs::symlink_metadata(&bundle.path).map_err(crate::native::io)?;
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err(Error::new(
                    "tls-root-bundle",
                    "CA bundle must be a regular file",
                ));
            }
            let mut bytes = vec![];
            std::fs::File::open(&bundle.path)
                .map_err(crate::native::io)?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(crate::native::io)?;
            if bytes.len() > 1024 * 1024 || sha256(&bytes) != bundle.sha256 {
                return Err(Error::new("tls-root-bundle", "CA bundle size/hash differs"));
            }
            let roots = reqwest::Certificate::from_pem_bundle(&bytes)
                .map_err(|e| Error::new("tls-root-bundle", e.to_string()))?;
            if roots.is_empty() || roots.len() > 1024 {
                return Err(Error::new(
                    "tls-root-bundle",
                    "Need 1..1024 PEM CA certificates",
                ));
            }
            (
                Some(roots),
                json!({"kind":"pem-bundle","sha256":bundle.sha256}),
            )
        } else {
            (None, json!({"kind":"platform"}))
        };
        let mut builder = if let Some(ip) = options.dns_server {
            TokioResolver::builder_with_config(
                ResolverConfig::from_name_servers(vec![NameServerConfig::udp_and_tcp(ip)]),
                TokioRuntimeProvider::default(),
            )
        } else {
            TokioResolver::builder_tokio().map_err(network)?
        };
        builder.options_mut().ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
        builder.options_mut().attempts = 1;
        builder.options_mut().timeout = Duration::from_millis(options.request_timeout_ms.min(5000));
        Ok(Self {
            dns: builder.build().map_err(network)?,
            roots,
            trust,
            options,
            #[cfg(test)]
            test_origins: Default::default(),
        })
    }
    pub async fn prepare(
        &self,
        raw: &str,
        cancel: &NetworkCancel,
        deadline: Instant,
    ) -> Result<Prepared> {
        let url = address(raw)?;
        let host = url.host_str().unwrap().trim_matches(['[', ']']);
        let ips = if let Ok(ip) = host.parse::<IpAddr>() {
            vec![ip]
        } else {
            cancel
                .until(deadline, async {
                    self.dns
                        .lookup_ip(format!("{host}."))
                        .await
                        .map(|r| r.iter().collect::<Vec<_>>())
                        .map_err(|e| Error::new("dns-error", e.to_string()))
                })
                .await?
        };
        let addresses = match pin_public_addresses(url.as_str(), &ips) {
            Ok(a) => a,
            #[cfg(test)]
            Err(_)
                if self
                    .test_origins
                    .contains(&url.origin().ascii_serialization()) =>
            {
                ips.iter()
                    .map(|ip| SocketAddr::new(*ip, url.port_or_known_default().unwrap()))
                    .collect()
            }
            Err(e) => return Err(e),
        };
        cancel.check(deadline)?;
        let mut client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .http1_only()
            .http1_max_headers(128)
            .pool_max_idle_per_host(0)
            .no_gzip()
            .no_deflate()
            .no_brotli()
            .no_zstd()
            .tls_sslkeylogfile(false)
            .timeout(Duration::from_millis(self.options.request_timeout_ms))
            .dns_resolver(NoDns)
            .resolve_to_addrs(host, &addresses);
        if let Some(roots) = &self.roots {
            client = client.tls_certs_only(roots.clone());
        }
        let client = client.build().map_err(network)?;
        Ok(Prepared {
            client,
            url: url.into(),
            addresses,
        })
    }
    pub async fn send(
        &self,
        p: Prepared,
        request: &ReplayRequest,
        maximum_wire: usize,
        cancel: &NetworkCancel,
        deadline: Instant,
        counts: &mut TransferCounts,
    ) -> Result<HttpResponse> {
        request.context.validate()?;
        cancel.until(deadline, async {
            counts.requests += 1;
            let mut builder = p.client.get(&p.url)
                .header("user-agent", USER_AGENT)
                .header("accept-encoding", ACCEPT_ENCODING);
            for (name, value) in &request.context.headers {
                builder = builder.header(name, value);
            }
            let mut response = builder.send().await.map_err(network)?;
            if response.url().as_str() != p.url
                || response.remote_addr().is_none_or(|a| !p.addresses.contains(&a))
            {
                return Err(Error::new("dns-identity", "Response peer or URL differs from pinned request"));
            }
            let status = response.status().as_u16();
            if status == 206 || response.headers().contains_key("content-range") {
                return Err(Error::new("partial-response", "Unrequested partial HTTP response"));
            }
            let mut headers = BTreeMap::new();
            let mut header_bytes = 0;
            for (name, value) in response.headers() {
                header_bytes += name.as_str().len() + value.as_bytes().len();
                if header_bytes > 65536 || value.as_bytes().len() > 16384 {
                    return Err(Error::new("network-limit", "Response headers exceed their bound"));
                }
                let value = value.to_str().map_err(network)?;
                if kept(name.as_str()) {
                    headers.entry(name.to_string())
                        .and_modify(|v: &mut String| { v.push_str(", "); v.push_str(value); })
                        .or_insert_with(|| value.into());
                }
            }
            if response.headers().get_all("content-encoding").iter().count() > 1 {
                return Err(Error::new("unsupported-encoding", "Multiple Content-Encoding fields"));
            }
            let encoding = response.headers().get("content-encoding")
                .map(|v| v.to_str()).transpose().map_err(network)?
                .unwrap_or("identity").trim().to_ascii_lowercase();
            if !["identity", "", "gzip", "deflate", "br"].contains(&encoding.as_str()) {
                return Err(Error::new("unsupported-encoding", format!("Unsupported Content-Encoding: {encoding}")));
            }
            if response.content_length().is_some_and(|n| n > maximum_wire as u64) {
                return Err(Error::new("network-byte-limit", "Encoded Content-Length exceeds request budget"));
            }
            let mut encoded = vec![];
            while let Some(chunk) = response.chunk().await.map_err(network)? {
                counts.wire_bytes += chunk.len() as u64;
                if encoded.len().checked_add(chunk.len()).is_none_or(|n| n > maximum_wire) {
                    return Err(Error::new("network-byte-limit", "Encoded response exceeds its bound"));
                }
                encoded.extend_from_slice(&chunk);
                cancel.check(deadline)?;
                tokio::task::yield_now().await;
            }
            let wire_length = encoded.len();
            let bytes = decode(encoded, &encoding, request.maximum_bytes, cancel, deadline, counts).await?;
            cancel.check(deadline)?;
            Ok(HttpResponse {
                body: Arc::new(bytes), status, headers,
                network: json!({
                    "dnsServer": self.options.dns_server, "tlsTrust": self.trust,
                    "validatedAddresses": p.addresses,
                    "protocol": "HTTP/1.1", "contentEncoding": encoding, "wireBytes": wire_length
                }),
            })
        }).await
    }
}
fn kept(name: &str) -> bool {
    [
        "content-type",
        "cache-control",
        "etag",
        "last-modified",
        "location",
        "access-control-allow-origin",
        "access-control-allow-methods",
        "access-control-allow-headers",
        "access-control-expose-headers",
        "cross-origin-resource-policy",
        "cross-origin-opener-policy",
        "cross-origin-embedder-policy",
        "content-security-policy",
        "content-security-policy-report-only",
        "content-range",
        "accept-ranges",
        "vary",
        "date",
        "expires",
        "x-content-type-options",
        "content-language",
        "referrer-policy",
    ]
    .contains(&name)
}
async fn decoded(
    reader: &mut (impl AsyncRead + Unpin),
    maximum: usize,
    cancel: &NetworkCancel,
    deadline: Instant,
    counts: &mut TransferCounts,
) -> Result<Vec<u8>> {
    let mut result = vec![];
    let mut chunk = [0u8; 8192];
    loop {
        cancel.check(deadline)?;
        let n = reader
            .read(&mut chunk)
            .await
            .map_err(|e| Error::new("decode-error", e.to_string()))?;
        if n == 0 {
            break;
        }
        counts.decoded_bytes += n as u64;
        if result.len().checked_add(n).is_none_or(|n| n > maximum) {
            return Err(Error::new(
                "network-byte-limit",
                "Decoded response exceeds its bound",
            ));
        }
        result.extend_from_slice(&chunk[..n]);
        tokio::task::yield_now().await;
    }
    Ok(result)
}
// Codec readers can consume many empty members without producing output.
// Yield on every input refill as well as every output chunk so cancellation and
// the outer deadline remain observable even for that case.
struct CodecInput {
    cursor: Cursor<Vec<u8>>,
    yielded: bool,
}
impl AsyncRead for CodecInput {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if !self.yielded {
            self.yielded = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        self.yielded = false;
        Pin::new(&mut self.cursor).poll_read(cx, buf)
    }
}
fn consumed(reader: BufReader<CodecInput>) -> Result<()> {
    if reader.get_ref().cursor.position() as usize != reader.get_ref().cursor.get_ref().len()
        || !reader.buffer().is_empty()
    {
        return Err(Error::new("decode-error", "Trailing encoded bytes"));
    }
    Ok(())
}
async fn decode(
    encoded: Vec<u8>,
    encoding: &str,
    maximum: usize,
    cancel: &NetworkCancel,
    deadline: Instant,
    counts: &mut TransferCounts,
) -> Result<Vec<u8>> {
    use async_compression::tokio::bufread::{BrotliDecoder, GzipDecoder, ZlibDecoder};
    if maximum == 0 || maximum > MEDIA_LIMIT {
        return Err(Error::new("network-limit", "Invalid decoded body limit"));
    }
    if encoding.is_empty() || encoding == "identity" {
        counts.decoded_bytes += encoded.len() as u64;
        if encoded.len() > maximum {
            return Err(Error::new(
                "network-byte-limit",
                "Decoded identity response exceeds its bound",
            ));
        }
        return Ok(encoded);
    }
    let input = BufReader::new(CodecInput {
        cursor: Cursor::new(encoded),
        yielded: false,
    });
    macro_rules! read {
        ($decoder:expr) => {{
            let mut d = $decoder;
            let out = decoded(&mut d, maximum, cancel, deadline, counts).await?;
            consumed(d.into_inner())?;
            out
        }};
    }
    Ok(match encoding {
        "gzip" => {
            let mut d = GzipDecoder::new(input);
            d.multiple_members(true);
            read!(d)
        }
        "deflate" => read!(ZlibDecoder::new(input)),
        "br" => read!(BrotliDecoder::new(input)),
        _ => return Err(Error::new("unsupported-encoding", "Unknown codec")),
    })
}

#[cfg(test)]
#[path = "tests_transport.rs"]
mod tests;
