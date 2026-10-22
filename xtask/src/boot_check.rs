//! `check-boot`: the built index.html draws its loader from its own first
//! bytes. The page compressed fits the first round trip (the "14 KB
//! rule": a new connection's first window, ten segments of 1460 bytes,
//! less what the response's headers and TLS take of it), and nothing the
//! first paint waits for comes from another fetch: no stylesheet links,
//! no classic scripts with a source, no `@import` or `url()` in its
//! styles. The loader's markup comes after its styles, and the page
//! follows the game's theme alone: nothing asks for the system's color
//! scheme. Trunk runs this after every build.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

/// A new connection's first window (RFC 6928: ten segments).
const WINDOW: usize = 10 * 1460;
/// Kept for the response's headers and TLS's framing.
const HEADERS: usize = 2048;
/// What the compressed page may take.
pub(crate) const BUDGET: usize = WINDOW - HEADERS;

const SCHEME: &str =
    "the page asks for the system's color scheme: it follows the game's theme alone";

pub(crate) fn check_boot(dest: Option<PathBuf>) -> Result<()> {
    let dest = crate::staging_dir(dest, "check-boot")?;
    let index = dest.join("index.html");
    let html =
        std::fs::read_to_string(&index).with_context(|| format!("reading {}", index.display()))?;
    let report = check(&html);
    if !report.problems.is_empty() {
        bail!(
            "{} does not draw its loader from its first bytes:\n  {}",
            index.display(),
            report.problems.join("\n  ")
        );
    }
    eprintln!(
        "check-boot: index.html is {} bytes, {} gzipped of {BUDGET}; nothing the first paint waits for is fetched",
        html.len(),
        report.gzipped
    );
    Ok(())
}

pub(crate) struct Report {
    pub(crate) gzipped: usize,
    pub(crate) problems: Vec<String>,
}

pub(crate) fn check(html: &str) -> Report {
    // Raw deflate at the level servers use, plus gzip's header and trailer.
    let gzipped = miniz_oxide::deflate::compress_to_vec(html.as_bytes(), 6).len() + 18;
    let mut problems = Vec::new();
    if gzipped > BUDGET {
        problems.push(format!(
            "{gzipped} bytes gzipped, over the {BUDGET} the first round trip carries"
        ));
    }
    let mut loader_styled = false;
    let mut loader = false;
    for el in elements(html) {
        let has = |name: &str| el.attrs.iter().any(|(n, _)| n == name);
        let attr = |name: &str| {
            el.attrs
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.to_ascii_lowercase())
        };
        match el.name.as_str() {
            "link"
                if attr("rel").is_some_and(|r| r.split_whitespace().any(|r| r == "stylesheet")) =>
            {
                problems.push(format!(
                    "a stylesheet link (`{}`): the first paint waits for it; inline it",
                    attr("href").unwrap_or_default()
                ));
            }
            "script" if has("src") => {
                let deferred =
                    has("async") || has("defer") || attr("type").is_some_and(|t| t == "module");
                if !deferred {
                    problems.push(format!(
                        "a classic script with a source (`{}`): parsing waits for it; make it a module, async or deferred",
                        attr("src").unwrap_or_default()
                    ));
                }
            }
            "style" => {
                let css = el.text.to_ascii_lowercase();
                if css.contains("@import") || css.contains("url(") {
                    problems.push(
                        "an inline style imports or points at a file (`@import`, `url()`): the first paint may wait for it"
                            .into(),
                    );
                }
                if css.contains("prefers-color-scheme") {
                    problems.push(SCHEME.into());
                }
                loader_styled |= el.text.contains(".boot-shell");
            }
            "meta" if attr("media").is_some_and(|m| m.contains("prefers-color-scheme")) => {
                problems.push(SCHEME.into());
            }
            _ if attr("id").is_some_and(|id| id == "boot-shell") => {
                loader = true;
                if !loader_styled {
                    problems
                        .push("the loader (`#boot-shell`) comes before its inline styles".into());
                }
            }
            _ => {}
        }
    }
    if !loader {
        problems.push("no loader (`#boot-shell`)".into());
    }
    Report { gzipped, problems }
}

struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    /// A script's or style's text.
    text: String,
}

/// The page's start tags in order, comments skipped, a script's or
/// style's text read as text (enough of HTML for our own page).
fn elements(html: &str) -> Vec<Element> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        let Some(end) = tag_end(rest) else {
            break;
        };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag.starts_with('/') || tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        let name_len = tag
            .find(|c: char| c.is_ascii_whitespace() || c == '/')
            .unwrap_or(tag.len());
        let name = tag[..name_len].to_ascii_lowercase();
        let attrs = attributes(&tag[name_len..]);
        let mut text = String::new();
        if name == "script" || name == "style" {
            let close = format!("</{name}");
            let lower = rest.to_ascii_lowercase();
            let stop = lower.find(&close).unwrap_or(rest.len());
            text = rest[..stop].to_string();
            rest = &rest[stop..];
        }
        out.push(Element { name, attrs, text });
    }
    out
}

/// Where a tag's `>` is, outside its quoted values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices().skip(1) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

fn attributes(s: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut chars = s.trim_end_matches('/').chars().peekable();
    loop {
        while chars
            .next_if(|c| c.is_ascii_whitespace() || *c == '/')
            .is_some()
        {}
        let mut name = String::new();
        while let Some(c) = chars.next_if(|c| !c.is_ascii_whitespace() && *c != '=') {
            name.push(c);
        }
        if name.is_empty() {
            return attrs;
        }
        while chars.next_if(|c| c.is_ascii_whitespace()).is_some() {}
        let mut value = String::new();
        if chars.next_if_eq(&'=').is_some() {
            while chars.next_if(|c| c.is_ascii_whitespace()).is_some() {}
            match chars.next_if(|c| *c == '"' || *c == '\'') {
                Some(q) => {
                    for c in chars.by_ref() {
                        if c == q {
                            break;
                        }
                        value.push(c);
                    }
                }
                None => {
                    while let Some(c) = chars.next_if(|c| !c.is_ascii_whitespace()) {
                        value.push(c);
                    }
                }
            }
        }
        attrs.push((name.to_ascii_lowercase(), value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLE: &str = "<style>.boot-shell { position: fixed }</style>";
    const LOADER: &str = r#"<div id="boot-shell" class="boot-shell"></div>"#;

    fn page(head: &str, body: &str) -> String {
        format!("<!doctype html><html><head>{head}</head><body>{body}</body></html>")
    }

    fn problems(html: &str) -> Vec<String> {
        check(html).problems
    }

    #[test]
    fn an_inline_loader_passes() {
        let html = page(
            &format!(
                r#"{STYLE}<script id="boot-inline">if (a > b) {{ x("<link rel=stylesheet>") }}</script>
                <link rel="modulepreload" href="/app.js" crossorigin="anonymous">
                <link rel="preload" href="/app_bg.wasm" as="fetch">
                <link rel="icon" href="favicon.svg">
                <script type="module">import init from '/app.js'; init();</script>
                <script src="/later.js" defer></script>
                <!-- <link rel="stylesheet" href="old.css"> -->"#
            ),
            LOADER,
        );
        assert_eq!(problems(&html), Vec::<String>::new());
    }

    #[test]
    fn a_stylesheet_link_blocks_the_first_paint() {
        let html = page(
            &format!(r#"{STYLE}<link rel="stylesheet" href="/s.css"/>"#),
            LOADER,
        );
        let p = problems(&html);
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("/s.css"), "{p:?}");
    }

    #[test]
    fn a_classic_script_with_a_source_blocks_parsing() {
        let html = page(&format!(r#"{STYLE}<script src="/x.js"></script>"#), LOADER);
        let p = problems(&html);
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("/x.js"), "{p:?}");
    }

    #[test]
    fn styles_fetching_files_are_reported() {
        for css in [
            "@import 'more.css';",
            ".a { background: url(bg.png) }",
            "@font-face { src: URL('f.ttf') }",
        ] {
            let html = page(&format!("{STYLE}<style>{css}</style>"), LOADER);
            assert_eq!(problems(&html).len(), 1, "{css}");
        }
    }

    #[test]
    fn the_page_follows_the_theme_not_the_system() {
        let style =
            "<style>@media (prefers-color-scheme: light) { :root { --ground: #fff } }</style>";
        let meta =
            r##"<meta name="theme-color" content="#fff" media="(prefers-color-scheme: light)">"##;
        for extra in [style, meta] {
            let html = page(&format!("{STYLE}{extra}"), LOADER);
            assert_eq!(problems(&html).len(), 1, "{extra}");
        }
    }

    #[test]
    fn the_loader_needs_its_styles_first() {
        assert_eq!(problems(&page("", LOADER)).len(), 1);
        assert_eq!(problems(&page(STYLE, "")).len(), 1);
        assert_eq!(problems(&page("", &format!("{LOADER}{STYLE}"))).len(), 1);
    }

    #[test]
    fn a_page_past_the_first_window_is_reported() {
        // Not compressible: every byte its own.
        let mut seed = 1u64;
        let noise: String = (0..BUDGET * 2)
            .map(|_| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                char::from(b'!' + (seed >> 58) as u8)
            })
            .collect();
        let html = page(STYLE, &format!("{LOADER}<p>{noise}</p>"));
        let r = check(&html);
        assert!(r.gzipped > BUDGET);
        assert_eq!(r.problems.len(), 1, "{:?}", r.problems);
    }
}
