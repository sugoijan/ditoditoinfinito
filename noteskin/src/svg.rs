//! SVG images in skins: our extension (StepMania reads bitmaps only). A
//! skin may use an `.svg` wherever it uses an image; the shell rasterises
//! it at the size the lanes need, so vector art stays sharp at any size.
//!
//! Only a static subset is accepted, the one the browser and `resvg` draw
//! alike: shapes, paths, groups, `use`, gradients, masks and clip paths;
//! no text, embedded images, filters, styles, scripts, animation, entities
//! or references outside the file. The root `<svg>` gives its size in
//! `width` and `height` (pixels, as an image's): that is the size the skin
//! sees, as a bitmap's would be.
//!
//! The file is parsed as XML, strictly, in document order (an XML
//! declaration, comments, elements, attributes in quotes, text, the five
//! predefined entities and nothing else), so nothing hides from the check,
//! and what drawing it costs is bounded: every element counts once for
//! each time it is drawn, through `use`, `mask` and `clip-path` too.

use std::collections::HashMap;

/// The largest SVG text read, in bytes.
pub const MAX_SVG: usize = 1 << 20;

/// The most elements drawn, counting each `use`, mask and clip path at
/// every place it is drawn (a few hundred for the bundled art).
pub const MAX_DRAWN: u64 = 50_000;

/// How deep elements may nest.
const MAX_DEPTH: usize = 64;

/// The elements an SVG may hold.
const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "use",
    "symbol",
    "linearGradient",
    "radialGradient",
    "stop",
    "mask",
    "clipPath",
    "title",
    "desc",
];

/// Attributes refused whatever their value: scripts' and styles'.
const REFUSED: &[&str] = &["style", "class", "filter"];

struct Element {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<Element>,
}

/// Checks `text` against the subset and returns its size, `(width,
/// height)` in pixels.
pub fn check(text: &str) -> Result<(u32, u32), String> {
    if text.len() > MAX_SVG {
        return Err(format!("larger than {MAX_SVG} bytes"));
    }
    let mut p = Parser {
        text: text.strip_prefix('\u{feff}').unwrap_or(text),
        at: 0,
    };
    p.skip_space();
    if p.rest().starts_with("<?xml") {
        let end = p.rest().find("?>").ok_or("an unclosed XML declaration")?;
        p.at += end + 2;
    }
    p.misc()?;
    let root = p.element(0)?;
    p.misc()?;
    if !p.rest().is_empty() {
        return Err("content after the root element".into());
    }
    if root.name != "svg" {
        return Err("the root is not <svg>".into());
    }
    let mut ids = HashMap::new();
    collect_ids(&root, &mut ids)?;
    let mut memo = HashMap::new();
    let drawn = cost(&root, &ids, &mut memo, &mut Vec::new())?;
    if drawn > MAX_DRAWN {
        return Err(format!("draws more than {MAX_DRAWN} elements"));
    }
    let size = |name: &str| {
        let v = root
            .attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())?;
        let v = v.strip_suffix("px").unwrap_or(v);
        // Digits, maybe a fraction: what every browser reads alike.
        if v.is_empty() || !v.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return None;
        }
        let n: f64 = v.parse().ok()?;
        (1.0..=8192.0).contains(&n).then(|| n.round() as u32)
    };
    match (size("width"), size("height")) {
        (Some(w), Some(h)) => Ok((w, h)),
        _ => Err("the <svg> needs a width and a height in pixels".into()),
    }
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn skip_space(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start_matches([' ', '\t', '\r', '\n']).len();
    }

    /// Whitespace and comments.
    fn misc(&mut self) -> Result<(), String> {
        loop {
            self.skip_space();
            if self.rest().starts_with("<!--") {
                self.comment()?;
            } else {
                return Ok(());
            }
        }
    }

    fn comment(&mut self) -> Result<(), String> {
        let body = &self.rest()[4..];
        let end = body.find("--").ok_or("an unclosed comment")?;
        // XML allows no `--` in a comment.
        if !body[end..].starts_with("-->") {
            return Err("a comment holds --".into());
        }
        self.at += 4 + end + 3;
        Ok(())
    }

    fn name(&mut self) -> Result<String, String> {
        let rest = self.rest();
        let len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.')))
            .unwrap_or(rest.len());
        if len == 0 || !rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            return Err("a malformed name".into());
        }
        self.at += len;
        Ok(rest[..len].to_string())
    }

    fn element(&mut self, depth: usize) -> Result<Element, String> {
        if depth > MAX_DEPTH {
            return Err("elements nest too deeply".into());
        }
        if !self.rest().starts_with('<') {
            return Err("an element was expected".into());
        }
        self.at += 1;
        let name = self.name()?;
        if !ELEMENTS.contains(&name.as_str()) {
            return Err(format!("<{name}> is not allowed"));
        }
        let mut attributes: Vec<(String, String)> = Vec::new();
        loop {
            let before = self.at;
            self.skip_space();
            if self.rest().starts_with("/>") {
                self.at += 2;
                return Ok(Element {
                    name,
                    attributes,
                    children: Vec::new(),
                });
            }
            if self.rest().starts_with('>') {
                self.at += 1;
                break;
            }
            if self.at == before {
                return Err(format!("<{name}>: a malformed attribute"));
            }
            let attribute = self.name()?;
            self.skip_space();
            if !self.rest().starts_with('=') {
                return Err(format!("<{name}>: {attribute} has no value"));
            }
            self.at += 1;
            self.skip_space();
            let quote = self
                .rest()
                .chars()
                .next()
                .filter(|c| matches!(c, '"' | '\''))
                .ok_or_else(|| format!("<{name}>: {attribute} is not quoted"))?;
            self.at += 1;
            let end = self
                .rest()
                .find(quote)
                .ok_or_else(|| format!("<{name}>: {attribute} is not closed"))?;
            let raw = &self.rest()[..end];
            self.at += end + 1;
            if raw.contains('<') {
                return Err(format!("<{name}>: {attribute} holds <"));
            }
            let value = unescape(raw)?;
            check_attribute(&name, &attribute, &value)?;
            if attributes.iter().any(|(n, _)| *n == attribute) {
                return Err(format!("<{name}>: {attribute} twice"));
            }
            attributes.push((attribute, value));
        }
        let mut children = Vec::new();
        loop {
            let rest = self.rest();
            if rest.starts_with("</") {
                self.at += 2;
                let closing = self.name()?;
                self.skip_space();
                if closing != name || !self.rest().starts_with('>') {
                    return Err(format!("<{name}> is closed by </{closing}>"));
                }
                self.at += 1;
                return Ok(Element {
                    name,
                    attributes,
                    children,
                });
            } else if rest.starts_with("<!--") {
                self.comment()?;
            } else if rest.starts_with("<!") || rest.starts_with("<?") {
                return Err("only elements, comments and text may be in an <svg>".into());
            } else if rest.starts_with('<') {
                children.push(self.element(depth + 1)?);
            } else if rest.is_empty() {
                return Err(format!("<{name}> is not closed"));
            } else {
                let end = rest.find('<').unwrap_or(rest.len());
                if rest[..end].contains("]]>") {
                    return Err("]]> in text".into());
                }
                unescape(&rest[..end])?;
                self.at += end;
            }
        }
    }
}

/// Text with only the five predefined entities, which it decodes.
fn unescape(raw: &str) -> Result<String, String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let (entity, c) = [
            ("&amp;", '&'),
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
            ("&apos;", '\''),
        ]
        .into_iter()
        .find(|(e, _)| rest.starts_with(e))
        .ok_or("an entity other than &amp; &lt; &gt; &quot; &apos;")?;
        out.push(c);
        rest = &rest[entity.len()..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Attributes: no event handlers, styles or filters; references only
/// inside the file.
fn check_attribute(element: &str, name: &str, value: &str) -> Result<(), String> {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("on") || REFUSED.contains(&lower.as_str()) {
        return Err(format!("<{element}>: the attribute {name} is not allowed"));
    }
    if (lower == "href" || lower.ends_with(":href")) && !value.starts_with('#') {
        return Err(format!(
            "<{element}>: {name}=\"{value}\" points outside the file"
        ));
    }
    let value = value.to_ascii_lowercase();
    let mut rest = value.as_str();
    while let Some(i) = rest.find("url(") {
        rest = rest[i + 4..].trim_start_matches(['\'', '"', ' ']);
        if !rest.starts_with('#') {
            return Err(format!("<{element}>: a url() outside the file"));
        }
    }
    Ok(())
}

fn collect_ids<'a>(e: &'a Element, ids: &mut HashMap<&'a str, &'a Element>) -> Result<(), String> {
    if let Some((_, id)) = e.attributes.iter().find(|(n, _)| n == "id")
        && ids.insert(id.as_str(), e).is_some()
    {
        return Err(format!("the id {id} twice"));
    }
    e.children.iter().try_for_each(|c| collect_ids(c, ids))
}

/// The ids an element draws besides its children: a `use`'s target and
/// the masks and clip paths it is drawn through.
fn references(e: &Element) -> impl Iterator<Item = &str> {
    e.attributes.iter().filter_map(move |(n, v)| {
        if e.name == "use" && (n == "href" || n.ends_with(":href")) {
            v.strip_prefix('#')
        } else if n == "mask" || n == "clip-path" {
            v.trim()
                .strip_prefix("url(")?
                .trim_start_matches(['\'', '"', ' '])
                .strip_prefix('#')
                .map(|s| s.trim_end_matches([')', '\'', '"', ' ']))
        } else {
            None
        }
    })
}

/// How many elements drawing `e` draws, references expanded.
fn cost<'a>(
    e: &'a Element,
    ids: &HashMap<&'a str, &'a Element>,
    memo: &mut HashMap<*const Element, u64>,
    path: &mut Vec<*const Element>,
) -> Result<u64, String> {
    let key = e as *const Element;
    if let Some(c) = memo.get(&key) {
        return Ok(*c);
    }
    if path.contains(&key) {
        return Err("a reference loops".into());
    }
    path.push(key);
    let mut total: u64 = 1;
    for c in &e.children {
        total = total.saturating_add(cost(c, ids, memo, path)?);
    }
    for id in references(e) {
        if let Some(target) = ids.get(id) {
            total = total.saturating_add(cost(target, ids, memo, path)?);
        }
    }
    path.pop();
    let total = total.min(MAX_DRAWN + 1);
    memo.insert(key, total);
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_the_subset() {
        let ok = r##"<?xml version="1.0"?>
<!-- a comment with <script> in it -->
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="64px" height='128' viewBox="0 0 64 128">
  <title>A &amp; B</title>
  <defs><linearGradient id="g"><stop offset="0" stop-color="#fff"/></linearGradient>
  <path id="p" d="M0 0L1 1"/><mask id="m"><use xlink:href="#p" stroke="#fff"/></mask></defs>
  <path d="M0 0L1 1" stroke="url(#g)"/><use xlink:href="#p"/><rect width="1" height="1" mask="url(#m)"/>
</svg>"##;
        assert_eq!(check(ok), Ok((64, 128)));
        for bad in [
            r#"<svg width="1" height="1"><script>x</script></svg>"#,
            r#"<svg width="1" height="1"><image href="a.png"/></svg>"#,
            r#"<svg width="1" height="1"><text>a</text></svg>"#,
            r#"<svg width="1" height="1" onload="x()"></svg>"#,
            r#"<svg width="1" height="1" onload ="x()"></svg>"#,
            "<svg width=\"1\" height=\"1\"><rect style\n=\"fill:red\"/></svg>",
            r#"<svg width="1" height="1"><rect filter="blur(9px)"/></svg>"#,
            r#"<svg width="1" height="1"><use href="other.svg#a"/></svg>"#,
            r#"<svg width="1" height="1"><use xlink:href ="http://x/a.svg#a"/></svg>"#,
            r#"<svg width="1" height="1"><rect fill="url(http://x/#a)"/></svg>"#,
            r#"<svg width="1" height="1"><rect fill="&#117;rl(http://x/#a)"/></svg>"#,
            r#"<!DOCTYPE svg [<!ENTITY a "b">]><svg width="1" height="1"/>"#,
            r#"<svg width="1" height="1"><?a <!-- ?><script/><?b -->?></svg>"#,
            r#"<svg width="1" height="1"><![CDATA[x]]></svg>"#,
            r#"<svg viewBox="0 0 1 1"/>"#,
            r#"<svg width="100%" height="1"/>"#,
            r#"<svg width="64 px" height="1"/>"#,
            r#"<g><svg width="1" height="1"/></g>"#,
            r#"<svg width="1" height="1"><!-- open"#,
            r#"<svg width="1" height="1"><!-- a -- b --></svg>"#,
            r#"<svg width="1" height="1"><g></svg>"#,
            r#"<svg width="1" height="1"><rect width=1/></svg>"#,
            r#"<svg width="1" height="1"></svg><svg width="1" height="1"/>"#,
            r##"<svg width="1" height="1"><g id="a"><use href="#a"/></g></svg>"##,
        ] {
            assert!(check(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn what_drawing_costs_is_bounded() {
        // Ten uses of the level below at each of six levels: a million.
        let mut s = String::from(r#"<svg width="1" height="1"><defs><path id="l0" d="M0 0"/>"#);
        for level in 1..=6 {
            s.push_str(&format!("<g id=\"l{level}\">"));
            for _ in 0..10 {
                s.push_str(&format!("<use href=\"#l{}\"/>", level - 1));
            }
            s.push_str("</g>");
        }
        s.push_str("</defs><use href=\"#l6\"/></svg>");
        assert!(check(&s).unwrap_err().contains("draws more"));
        // Masks drawn through masks multiply too.
        let mut s = String::from(r#"<svg width="1" height="1"><defs><mask id="m0"><rect/></mask>"#);
        for level in 1..=6 {
            s.push_str(&format!("<mask id=\"m{level}\">"));
            for _ in 0..10 {
                s.push_str(&format!("<rect mask=\"url(#m{})\"/>", level - 1));
            }
            s.push_str("</mask>");
        }
        s.push_str("</defs><rect mask=\"url(#m6)\"/></svg>");
        assert!(check(&s).unwrap_err().contains("draws more"));
    }
}
