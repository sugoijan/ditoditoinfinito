//! Passive, bounded source discovery. This module never fetches or executes code.
use crate::{Error, Result, assets::*};
use html5ever::{
    tendril::StrTendril,
    tokenizer::{
        BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap};
use url::Url;

pub const PAGE_LIMIT: usize = 2 * 1024 * 1024;
const ITEM_LIMIT: usize = 2048;
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-discovery", message)
}
fn limited() -> Error {
    Error::new(
        "discovery-limit",
        "Page, URL, element or FlashVars limit exceeded",
    )
}
pub(crate) fn http(raw: &str) -> Result<Url> {
    if raw.is_empty() || raw.len() > 4096 || raw.chars().any(char::is_control) {
        return Err(invalid("Expected a bounded HTTP(S) URL"));
    }
    let mut url = Url::parse(raw).map_err(|e| invalid(e.to_string()))?;
    if !["http", "https"].contains(&url.scheme())
        || !url.has_host()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid(
            "Only anonymous HTTP(S) addresses are discovery candidates",
        ));
    }
    url.set_fragment(None);
    if url.as_str().len() > 4096 {
        return Err(limited());
    }
    Ok(url)
}
/// Original runtime address and requested retrieval address are distinct identities.
/// An archive snapshot does not authorize fetching it or any linked resource.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAddress {
    pub logical_url: String,
    pub retrieval_url: String,
    pub capture: Option<String>,
    pub replay_modifier: Option<String>,
}
impl SourceAddress {
    pub fn parse(raw: &str) -> Result<Self> {
        let retrieval = http(raw)?;
        if retrieval.host_str() == Some("web.archive.org") {
            let rest = retrieval.path().strip_prefix("/web/").ok_or_else(|| {
                invalid("Expected an explicit Wayback snapshot, not search/calendar")
            })?;
            let (stamp, original) = rest
                .split_once('/')
                .ok_or_else(|| invalid("Missing Wayback original URL"))?;
            let (capture, modifier) = if let Some(c) = stamp.strip_suffix("id_") {
                (c, "id_")
            } else {
                (stamp, "")
            };
            if capture.len() != 14 || !capture.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid(
                    "Need a 14-digit explicit Wayback snapshot with optional id_",
                ));
            }
            let original = match retrieval.query() {
                Some(q) => format!("{original}?{q}"),
                None => original.to_owned(),
            };
            let logical = http(&original)?;
            if logical.host_str() == Some("web.archive.org") {
                return Err(invalid("Nested archive addresses are unsupported"));
            }
            return Ok(Self {
                logical_url: logical.into(),
                retrieval_url: retrieval.to_string(),
                capture: Some(capture.into()),
                replay_modifier: Some(modifier.into()),
            });
        }
        Ok(Self {
            logical_url: retrieval.to_string(),
            retrieval_url: retrieval.into(),
            capture: None,
            replay_modifier: None,
        })
    }
    /// Relative links inherit an explicit snapshot; rewritten archive links retain
    /// their own snapshot, including a different capture time. Never falls back live.
    pub fn resolve(&self, raw: &str) -> Result<Self> {
        if raw.len() > 4096 || raw.chars().any(char::is_control) {
            return Err(invalid("Invalid relative address"));
        }
        let raw = raw.trim();
        let resolved = http(&self.logical_url)?
            .join(raw)
            .map_err(|e| invalid(e.to_string()))?;
        let logical = http(resolved.as_str())?;
        if logical.host_str() == Some("web.archive.org") {
            return Self::parse(logical.as_str());
        }
        // Wayback often rewrites links as /web/TIMESTAMP/http://original/… .
        if self.capture.is_some() && raw.starts_with("/web/") {
            return Self::parse(&format!("https://web.archive.org{raw}"));
        }
        if let Some(capture) = &self.capture {
            return Self::parse(&format!(
                "https://web.archive.org/web/{capture}{}/{}",
                self.replay_modifier.as_deref().unwrap_or(""),
                logical
            ));
        }
        Self::parse(logical.as_str())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub kind: String,
    pub address: SourceAddress,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Movie {
    pub kind: String,
    pub address: SourceAddress,
    pub flash_vars: BTreeMap<String, String>,
    /// Declared HTML pixel viewport only; CSS/layout is not evaluated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<[u32; 2]>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDiscovery {
    pub schema: &'static str,
    pub source: SourceAddress,
    pub body_sha256: String,
    pub encoding: String,
    /// First completed, unique HTML title, bounded to 512 characters.
    pub title: Option<String>,
    /// Document title, refined by a unique matching heading before the player.
    pub pack_title: Option<String>,
    pub base: SourceAddress,
    pub links: Vec<Link>,
    pub movies: Vec<Movie>,
    pub scripts: Vec<SourceAddress>,
    pub has_inline_script: bool,
    /// Invalid/unusable candidates are retained as diagnostics, never fetch authority.
    pub diagnostics: Vec<String>,
}
#[derive(Default)]
struct Object {
    data: Option<String>,
    movie: Option<String>,
    vars: String,
    conflict: bool,
    viewport: Option<[u32; 2]>,
}
type EmbeddedMovie = (String, String, String, bool, Option<[u32; 2]>);
#[derive(Default)]
struct TitleBlock {
    tag: String,
    text: String,
    invalid: bool,
}
#[derive(Default)]
struct Tokens {
    base: Option<String>,
    charset: Option<String>,
    title: String,
    in_title: bool,
    title_count: usize,
    title_complete: bool,
    title_block: Option<TitleBlock>,
    title_blocks: Vec<String>,
    title_blocks_full: bool,
    before_player: bool,
    links: Vec<(String, String)>,
    movies: Vec<EmbeddedMovie>,
    scripts: Vec<String>,
    objects: Vec<Object>,
    count: usize,
    inline_script: bool,
    overflow: bool,
}
#[derive(Default)]
struct Sink(RefCell<Tokens>);
impl TokenSink for Sink {
    type Handle = ();
    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let mut t = self.0.borrow_mut();
        if let Token::CharacterTokens(chars) = &token {
            if t.in_title && t.title.len() <= 4096 {
                t.title.push_str(chars);
            }
            if let Some(block) = &mut t.title_block {
                if block.text.len() + chars.len() <= 4096 {
                    block.text.push_str(chars);
                } else {
                    block.invalid = true;
                }
            }
        }
        let Token::TagToken(tag) = token else {
            return TokenSinkResult::Continue;
        };
        t.count += 1;
        if t.count > 32768 {
            t.overflow = true;
            return TokenSinkResult::Continue;
        }
        let name = tag.name.as_ref();
        if tag.kind == TagKind::EndTag {
            if t.title_block.as_ref().is_some_and(|b| b.tag == name)
                && let Some(block) = t.title_block.take()
                && !block.invalid
            {
                if t.title_blocks.len() < 64 {
                    t.title_blocks.push(block.text);
                } else {
                    t.title_blocks_full = true;
                }
            }
            if name == "title" {
                t.in_title = false;
                t.title_complete = true;
            }
            if name == "object"
                && let Some(o) = t.objects.pop()
                && let Some(url) = o.data.or(o.movie)
            {
                t.movies
                    .push(("object".into(), url, o.vars, o.conflict, o.viewport));
            }
            return TokenSinkResult::Continue;
        }
        let attr = |n: &str| {
            tag.attrs
                .iter()
                .find(|a| &*a.name.local == n)
                .map(|a| a.value.to_string())
        };
        if matches!(name, "object" | "embed") {
            t.before_player = false;
            t.title_block = None;
        }
        if t.before_player && matches!(name, "p" | "h1" | "h2") {
            t.title_block = Some(TitleBlock {
                tag: name.into(),
                ..Default::default()
            });
        }
        if let Some(block) = &mut t.title_block
            && (matches!(name, "script" | "style" | "template" | "textarea" | "a")
                || attr("hidden").is_some()
                || attr("style").is_some())
        {
            block.invalid = true;
        }
        match name {
            "base" if t.base.is_none() => t.base = attr("href"),
            "meta" if t.charset.is_none() => {
                t.charset = attr("charset");
                if t.charset.is_none()
                    && attr("http-equiv").is_some_and(|s| s.eq_ignore_ascii_case("content-type"))
                {
                    t.charset = attr("content").and_then(|c| {
                        c.split(';').find_map(|p| {
                            p.trim()
                                .split_once('=')
                                .filter(|(k, _)| k.trim().eq_ignore_ascii_case("charset"))
                                .map(|(_, v)| v.trim().trim_matches(['\'', '"']).to_owned())
                        })
                    });
                }
            }
            "a" | "area" => {
                if let Some(url) = attr("href") {
                    t.links.push(("anchor".into(), url));
                }
            }
            "frame" | "iframe" => {
                if let Some(url) = attr("src") {
                    t.links.push((name.into(), url));
                }
            }
            "object" => {
                if t.objects.len() >= 64 {
                    t.overflow = true;
                } else {
                    t.objects.push(Object {
                        data: attr("data"),
                        viewport: viewport(attr("width"), attr("height")),
                        ..Default::default()
                    });
                }
            }
            "param" => {
                if let (Some(o), Some(n), Some(v)) =
                    (t.objects.last_mut(), attr("name"), attr("value"))
                {
                    match n.to_ascii_lowercase().as_str() {
                        "movie" => {
                            if o.movie.as_ref().is_some_and(|s| s != &v) {
                                o.conflict = true;
                            }
                            o.movie = Some(v);
                        }
                        "flashvars" => {
                            if !o.vars.is_empty() && o.vars != v {
                                o.conflict = true;
                            }
                            o.vars = v;
                        }
                        _ => {}
                    }
                }
            }
            "embed" => {
                if let Some(url) = attr("src") {
                    t.movies.push((
                        "embed".into(),
                        url,
                        attr("flashvars").unwrap_or_default(),
                        false,
                        viewport(attr("width"), attr("height")),
                    ));
                }
            }
            "plaintext" => return TokenSinkResult::Plaintext,
            "script" => {
                if let Some(src) = attr("src") {
                    t.scripts.push(src);
                } else {
                    t.inline_script = true;
                }
                return TokenSinkResult::RawData(RawKind::ScriptData);
            }
            "style" | "xmp" | "noembed" | "noframes" => {
                return TokenSinkResult::RawData(RawKind::Rawtext);
            }
            "title" => {
                t.title_count += 1;
                t.in_title = true;
                return TokenSinkResult::RawData(RawKind::Rcdata);
            }
            "textarea" => return TokenSinkResult::RawData(RawKind::Rcdata),
            _ => {}
        }
        if t.links.len() + t.movies.len() + t.scripts.len() > ITEM_LIMIT {
            t.overflow = true;
        }
        TokenSinkResult::Continue
    }
}
fn viewport(width: Option<String>, height: Option<String>) -> Option<[u32; 2]> {
    let number = |s: String| {
        s.trim()
            .parse::<u32>()
            .ok()
            .filter(|n| (1..=4096).contains(n))
    };
    Some([number(width?)?, number(height?)?])
}
fn tokenize(text: &str) -> Tokens {
    let queue = BufferQueue::default();
    queue.push_back(StrTendril::from_slice(text));
    let tokenizer = Tokenizer::new(
        Sink(RefCell::new(Tokens {
            before_player: true,
            ..Default::default()
        })),
        Default::default(),
    );
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    let mut t = tokenizer.sink.0.into_inner();
    for o in t.objects.drain(..) {
        if let Some(url) = o.data.or(o.movie) {
            t.movies
                .push(("object".into(), url, o.vars, o.conflict, o.viewport));
        }
    }
    if t.links.len() + t.movies.len() + t.scripts.len() > ITEM_LIMIT {
        t.overflow = true;
    }
    t
}
pub(crate) fn decode(body: &[u8], hint: Option<&str>) -> Result<(String, String)> {
    if body.len() > PAGE_LIMIT {
        return Err(limited());
    }
    let (encoding, skip) = if let Some((e, n)) = encoding_rs::Encoding::for_bom(body) {
        (e, n)
    } else {
        let sniff = String::from_utf8_lossy(&body[..body.len().min(1024)]);
        let meta = tokenize(&sniff).charset;
        let label = hint.or(meta.as_deref()).unwrap_or("utf-8");
        let label = if label.eq_ignore_ascii_case("cp932") {
            "shift_jis"
        } else {
            label
        };
        (
            encoding_rs::Encoding::for_label(label.as_bytes())
                .ok_or_else(|| invalid("Unsupported page encoding"))?,
            0,
        )
    };
    let (text, errors) = encoding.decode_without_bom_handling(&body[skip..]);
    if errors {
        return Err(invalid(
            "Invalid page bytes; supply an evidenced encoding override",
        ));
    }
    Ok((text.into_owned(), encoding.name().into()))
}
fn flash_vars(raw: &str) -> Result<BTreeMap<String, String>> {
    if raw.len() > 16384 {
        return Err(limited());
    }
    let mut vars = BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if key.is_empty()
            || key.len() > 1024
            || value.len() > 4096
            || key.chars().chain(value.chars()).any(char::is_control)
            || key.contains('\u{fffd}')
            || value.contains('\u{fffd}')
        {
            return Err(invalid("Invalid FlashVars field"));
        }
        if vars.get(key.as_ref()).is_some_and(|v| v != value.as_ref()) {
            return Err(invalid("Conflicting FlashVars keys"));
        }
        vars.insert(key.into_owned(), value.into_owned());
        if vars.len() > 64 {
            return Err(limited());
        }
    }
    Ok(vars)
}
fn swf(url: &str) -> bool {
    Url::parse(url).is_ok_and(|u| u.path().to_ascii_lowercase().ends_with(".swf"))
}
/// Parse static anchors/frames/object/embed/noscript and FlashVars. No scripts,
/// routers, meta refresh, CSS or event handlers run. Candidates are not a crawl.
pub fn inspect_page(
    raw_url: &str,
    body: &[u8],
    encoding_hint: Option<&str>,
) -> Result<PageDiscovery> {
    let source = SourceAddress::parse(raw_url)?;
    let (text, encoding) = decode(body, encoding_hint)?;
    let t = tokenize(&text);
    if t.overflow {
        return Err(limited());
    }
    let mut diagnostics = vec![];
    let base = if let Some(b) = t.base {
        match source.resolve(&b) {
            Ok(b) => b,
            Err(e) => {
                diagnostics.push(format!("base: {}", e.message));
                source.clone()
            }
        }
    } else {
        source.clone()
    };
    let title = t.title.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = (t.title_count == 1
        && t.title_complete
        && !title.is_empty()
        && title.chars().count() <= 512)
        .then_some(title);
    let pack_title = title.as_ref().map(|title| {
        let mut headings = t
            .title_blocks
            .iter()
            .map(|s| {
                s.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim_matches(|c: char| c.is_whitespace() || matches!(c, '~' | '～'))
                    .to_owned()
            })
            .filter(|s| {
                s.starts_with(title)
                    && s.len() > title.len()
                    && s.chars().count() <= 512
                    && !s.chars().any(char::is_control)
            })
            .collect::<std::collections::BTreeSet<_>>();
        if headings.len() == 1 && !t.title_blocks_full {
            headings.pop_first().unwrap()
        } else {
            title.clone()
        }
    });
    let mut result = PageDiscovery {
        schema: "ddi-danoni-page-discovery/1",
        source,
        body_sha256: sha256(body),
        encoding,
        title,
        pack_title,
        base,
        links: vec![],
        movies: vec![],
        scripts: vec![],
        has_inline_script: t.inline_script,
        diagnostics,
    };
    for (kind, raw) in t.links {
        if raw.trim().is_empty() || raw.trim().starts_with('#') {
            continue;
        }
        match result.base.resolve(&raw) {
            Ok(address) if swf(&address.logical_url) => {
                let movie = Movie {
                    kind,
                    address,
                    flash_vars: BTreeMap::new(),
                    viewport: None,
                };
                if !result.movies.contains(&movie) {
                    result.movies.push(movie);
                }
            }
            Ok(address) => {
                let link = Link { kind, address };
                if !result.links.contains(&link) {
                    result.links.push(link);
                }
            }
            Err(e) => result.diagnostics.push(format!("{kind}: {}", e.message)),
        }
    }
    for (kind, raw, vars, conflict, viewport) in t.movies {
        let candidate = (|| {
            if conflict {
                return Err(invalid("Conflicting object parameters"));
            }
            let address = result.base.resolve(&raw)?;
            if !swf(&address.logical_url) {
                return Err(invalid("Embed is not an explicit .swf address"));
            }
            Ok(Movie {
                kind: kind.clone(),
                address,
                flash_vars: flash_vars(&vars)?,
                viewport,
            })
        })();
        match candidate {
            Ok(m) => {
                if !result.movies.contains(&m) {
                    result.movies.push(m);
                }
            }
            Err(e) => result.diagnostics.push(format!("{kind}: {}", e.message)),
        }
    }
    for raw in t.scripts {
        match result.base.resolve(&raw) {
            Ok(a) => {
                if !result.scripts.contains(&a) {
                    result.scripts.push(a);
                }
            }
            Err(e) => result.diagnostics.push(format!("script: {}", e.message)),
        }
    }
    Ok(result)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CachedPageRequest {
    pub manifest: Binding,
    pub url: String,
    pub encoding_override: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedPage {
    pub schema: &'static str,
    pub manifest: Binding,
    pub body: Binding,
    /// `archive-body-only` has no inferred HTTP status, completeness or policy grant.
    pub response_evidence: &'static str,
    pub encoding_override: Option<String>,
    /// Exact original resource record, including policy, redirects and request context.
    pub resource: Value,
    pub discovery: PageDiscovery,
}
/// Read a v1 single-source/resource manifest as historical evidence. This does
/// not grant a current network policy decision or reuse a cached browser variant
/// for an anonymous request. The full original resource record is retained.
/// Older archive body records are explicitly weaker evidence, never HTTP receipts.
pub fn inspect_cached_page<S: AssetSource>(
    source: &S,
    request: &CachedPageRequest,
) -> Result<CachedPage> {
    let mut inputs = Inputs::new(source);
    let bytes = inputs.checked(&request.manifest, JSON_LIMIT)?;
    let manifest: Value = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    if ![
        Some("ddi-danoni-single-source/experimental-1"),
        Some("ddi-danoni-resources/1"),
    ]
    .contains(&manifest["schema"].as_str())
    {
        return Err(invalid("Unsupported source/resource manifest version"));
    }
    let requested = SourceAddress::parse(&request.url)?;
    let rows = manifest["resources"]
        .as_array()
        .ok_or_else(|| invalid("Missing resource records"))?;
    if rows.len() > 4096 {
        return Err(limited());
    }
    let matching: Vec<_> = rows
        .iter()
        .filter(|r| {
            (r["requestedUrl"]
                .as_str()
                .or_else(|| r["replayUrl"].as_str()))
                == Some(requested.retrieval_url.as_str())
        })
        .collect();
    if matching.len() != 1 {
        return Err(invalid("Need one exact cached resource identity"));
    }
    let record = matching[0];
    let archive_body = record.get("requestedUrl").is_none()
        && record.get("status").is_none()
        && record["replayUrl"].as_str() == Some(requested.retrieval_url.as_str())
        && manifest["schema"] == "ddi-danoni-single-source/experimental-1";
    let final_url = if archive_body {
        let logical = record["logicalUrl"]
            .as_str()
            .ok_or_else(|| invalid("Missing archived logical URL"))?;
        if requested.capture.is_none()
            || record["snapshot"].as_str() != requested.capture.as_deref()
            || http(logical)?.as_str() != requested.logical_url
            || record["role"] != "page"
            || manifest["archive"]["retrievedOn"]
                .as_str()
                .is_none_or(str::is_empty)
        {
            return Err(invalid("Inconsistent archived page body evidence"));
        }
        requested.retrieval_url.as_str()
    } else {
        if record["status"] != 200
            || record["completeBody"] != true
            || record["retrievedAt"]
                .as_str()
                .is_none_or(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
            || record["mimeType"] != "text/html"
        {
            return Err(invalid("Need a complete, dated HTTP 200 HTML response"));
        }
        record["finalUrl"]
            .as_str()
            .ok_or_else(|| invalid("Missing final retrieval URL"))?
    };
    SourceAddress::parse(final_url)?;
    let path = record["localPath"]
        .as_str()
        .or_else(|| record["bodyPath"].as_str())
        .ok_or_else(|| invalid("Missing body binding"))?;
    let hash = record["sha256"]
        .as_str()
        .ok_or_else(|| invalid("Missing body hash"))?;
    let body = request.manifest.sibling(&RelativeBinding {
        path: path.into(),
        sha256: hash.into(),
    })?;
    let bytes = inputs.checked(&body, PAGE_LIMIT)?;
    if (!archive_body || record.get("byteLength").is_some())
        && record["byteLength"].as_u64() != Some(bytes.len() as u64)
    {
        return Err(invalid("Cached body length differs"));
    }
    let hint = request
        .encoding_override
        .as_deref()
        .or_else(|| record["charset"].as_str());
    let discovery = inspect_page(final_url, &bytes, hint)?;
    Ok(CachedPage {
        schema: "ddi-danoni-cached-page/1",
        manifest: request.manifest.clone(),
        body,
        response_evidence: if archive_body {
            "archive-body-only"
        } else {
            "http-response"
        },
        resource: record.clone(),
        encoding_override: request.encoding_override.clone(),
        discovery,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedPageJob {
    schema: String,
    roots: BTreeMap<String, std::path::PathBuf>,
    request: CachedPageRequest,
}
pub fn inspect_cached_page_job(path: &std::path::Path) -> Result<CachedPage> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(crate::native::io)?
        .take(JSON_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(crate::native::io)?;
    if bytes.len() > JSON_LIMIT {
        return Err(limited());
    }
    let job: CachedPageJob = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    if job.schema != "ddi-danoni-cached-page-job/1" {
        return Err(invalid("Unsupported cached-page job version"));
    }
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let roots = job
        .roots
        .into_iter()
        .map(|(ns, p)| (ns, if p.is_absolute() { p } else { parent.join(p) }))
        .collect();
    let source = crate::native::DirectorySource::new(roots)?;
    inspect_cached_page(&source, &job.request)
}
