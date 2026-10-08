//! Where a work's dos text lives: a page and, maybe, a script file.
//!
//! danoniplus (`loadChartFile`, `js/lib/dosConverter.js` 541–583) reads the
//! `value` of the element with `id="dos"` (an `<input type="hidden">` or a
//! `<textarea>`), then, when the page names one with
//! `<input id="externalDos" value="...">`, loads that file as a script whose
//! `externalDosInit()` sets `g_externalDos`, and merges it over the inline
//! fields. The external file may be in another charset
//! (`<input id="externalDosCharset">`). Nothing is executed here: the script
//! is only read for a literal assignment to `g_externalDos`.

use super::dos::DosFlags;

/// What a page says about its dos.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageDos {
    /// The inline dos, entities decoded.
    pub inline: Option<String>,
    /// The external dos file, as the page names it.
    pub external: Option<String>,
    /// Its charset, when the page sets one.
    pub external_charset: Option<String>,
    /// `externalDosDivide`: one file per chart (`name2.txt`, …).
    pub divided: bool,
    pub flags: DosFlags,
}

/// The dos references of an HTML page, or `None` when it has none (not a
/// Dancing☆Onigiri page).
pub fn scan_html(html: &str) -> Option<PageDos> {
    let mut page = PageDos::default();
    for tag in tags(html) {
        // `getElementById`: the id only, the first element with it.
        let Some(id) = attr(tag.attrs, "id") else {
            continue;
        };
        let value = match tag.name.as_str() {
            "input" => attr(tag.attrs, "value").unwrap_or_default(),
            "textarea" => decode_entities(tag.content.unwrap_or_default()),
            _ => continue,
        };
        match id.as_str() {
            "dos" if page.inline.is_none() => page.inline = Some(value),
            "externalDos" if page.external.is_none() && !value.is_empty() => {
                page.external = Some(value)
            }
            "externalDosCharset" if !value.is_empty() => page.external_charset = Some(value),
            "externalDosDivide" => page.divided = value.eq_ignore_ascii_case("true"),
            "enableAmpersandSplit" => {
                page.flags.ampersand_split = !value.eq_ignore_ascii_case("false")
            }
            "enableDecodeURI" => page.flags.decode_uri = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    (page.inline.is_some() || page.external.is_some()).then_some(page)
}

/// The value a dos script assigns to `g_externalDos`, or `None` when it
/// builds the value from anything but string literals (variables,
/// functions): such a work needs its script run, which an importer never
/// does. Every `g_externalDos = …` and `g_externalDos += …` counts, in
/// order; each may join literals with `+`.
pub fn external_dos_text(js: &str) -> Option<String> {
    assigned_literals(js, "g_externalDos")
}

/// The base64 payload of an encoded music script (`g_musicdata = "…"`),
/// which danoniplus decodes into the audio file (`dataLoader.js` 118–133).
pub fn encoded_music(js: &str) -> Option<String> {
    assigned_literals(js, "g_musicdata")
}

/// What the assignments to `name` in a script add up to, when every one is
/// made of string literals joined with `+`.
fn assigned_literals(js: &str, name: &str) -> Option<String> {
    let mut value: Option<String> = None;
    let mut from = 0;
    while let Some(off) = js[from..].find(name) {
        let at = from + off + name.len();
        from = at;
        // A longer identifier (`g_externalDosX`) is not this one.
        if js[at..].starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = js[at..].trim_start();
        let (append, rest) = if let Some(r) = rest.strip_prefix("+=") {
            (true, r)
        } else if let Some(r) = rest.strip_prefix('=').filter(|r| !r.starts_with('=')) {
            (false, r)
        } else {
            // A declaration or a read (`let g_externalDos;`, `typeof …`).
            continue;
        };
        let (text, after) = literal_sum(rest.trim_start())?;
        let after_trim = after.trim_start();
        if !(after_trim.is_empty()
            || after_trim.starts_with(';')
            || after_trim.starts_with('}')
            || after_trim.starts_with(',')
            || after.len() != after_trim.len()
                && after[..after.len() - after_trim.len()].contains('\n'))
        {
            return None;
        }
        value = Some(match (append, value) {
            (true, Some(v)) => v + &text,
            (_, _) => text,
        });
        from = js.len() - after.len();
    }
    value
}

/// String literals joined with `+` at the start of `s`.
fn literal_sum(s: &str) -> Option<(String, &str)> {
    let (mut text, mut rest) = js_string_literal(s)?;
    while let Some(next) = rest.trim_start().strip_prefix('+') {
        let (t, r) = js_string_literal(next.trim_start())?;
        text.push_str(&t);
        rest = r;
    }
    Some((text, rest))
}

/// A JavaScript string literal at the start of `s` (`"…"`, `'…'` or a
/// template without substitutions), unescaped, and the text after it.
fn js_string_literal(s: &str) -> Option<(String, &str)> {
    let quote = s.chars().next()?;
    if !matches!(quote, '"' | '\'' | '`') {
        return None;
    }
    let mut out = String::new();
    let mut chars = s[1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            c if c == quote => return Some((out, &s[1 + i + 1..])),
            '$' if quote == '`' && s[1 + i..].starts_with("${") => return None,
            '\\' => {
                let (_, e) = chars.next()?;
                match e {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '0' => out.push('\0'),
                    'u' => {
                        let hex: String =
                            (0..4).filter_map(|_| chars.next().map(|x| x.1)).collect();
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    'x' => {
                        let hex: String =
                            (0..2).filter_map(|_| chars.next().map(|x| x.1)).collect();
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    '\n' => {}
                    '\r' => {}
                    other => out.push(other),
                }
            }
            c => out.push(c),
        }
    }
    None
}

struct Tag<'a> {
    name: String,
    attrs: &'a str,
    /// For `<textarea>`: the raw text up to `</textarea>`.
    content: Option<&'a str>,
}

/// `<input …>` and `<textarea …>…</textarea>` elements, in order. A small
/// scanner, not an HTML parser: enough for the pages danoniplus works use,
/// which put the dos in a quoted attribute (possibly spanning many lines).
fn tags(html: &str) -> Vec<Tag<'_>> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    while let Some(off) = lower[i..].find('<') {
        let start = i + off + 1;
        if !lower[start..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!') {
            i = start;
            continue;
        }
        let name_end = lower[start..]
            .find(|c: char| !c.is_ascii_alphanumeric())
            .map_or(lower.len(), |e| start + e);
        let name = &lower[start..name_end];
        if name == "!--" || lower[start..].starts_with("!--") {
            i = lower[start..]
                .find("-->")
                .map_or(lower.len(), |e| start + e + 3);
            continue;
        }
        // Find the end of the tag, skipping quoted attribute values.
        let mut j = name_end;
        let bytes = html.as_bytes();
        let mut quote = None;
        while j < bytes.len() {
            let c = bytes[j];
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == b'"' || c == b'\'' => quote = Some(c),
                None if c == b'>' => break,
                None => {}
            }
            j += 1;
        }
        let attrs = &html[name_end..j.min(html.len())];
        let mut content = None;
        let mut next = j + 1;
        if name == "textarea" {
            let close = lower[next.min(lower.len())..].find("</textarea");
            let end = close.map_or(lower.len(), |c| next + c);
            content = Some(&html[next.min(end)..end]);
            next = end;
        }
        if name == "input" || name == "textarea" {
            out.push(Tag {
                name: name.to_string(),
                attrs,
                content,
            });
        }
        i = next.min(html.len());
    }
    out
}

/// An attribute's value, entities decoded.
fn attr(attrs: &str, name: &str) -> Option<String> {
    let b = attrs.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let ks = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'>' {
            i += 1;
        }
        let key = &attrs[ks..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let value = if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let vs = i + 1;
                let ve = attrs[vs..].find(q as char).map_or(attrs.len(), |e| vs + e);
                i = (ve + 1).min(b.len());
                &attrs[vs..ve]
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                &attrs[vs..i]
            }
        } else {
            ""
        };
        if key.eq_ignore_ascii_case(name) {
            return Some(decode_entities(value));
        }
        if ks == i {
            i += 1;
        }
    }
    None
}

/// HTML character references: the named ones these pages use, and numeric.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        // A reference is short: look for its `;` within 12 characters
        // (characters, not bytes: the text around it may be Japanese).
        let semi = rest
            .char_indices()
            .take(12)
            .find(|&(_, c)| c == ';')
            .map(|(i, _)| i);
        let decoded = semi.and_then(|e| {
            let ent = &rest[1..e];
            let c = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => ent.strip_prefix('#').and_then(|n| {
                    let code = match n.strip_prefix(['x', 'X']) {
                        Some(h) => u32::from_str_radix(h, 16).ok(),
                        None => n.parse().ok(),
                    };
                    code.and_then(char::from_u32)
                }),
            }?;
            Some((c, e + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_dos_in_a_hidden_input() {
        let html = r#"<html><body>
<input type="hidden" name="dos" id="dos" value='
|musicTitle=Song &amp; Dance,Artist|
|difData=5,Normal,3.5|
|left_data=200,300|'>
<input type="hidden" id="enableAmpersandSplit" value="false">
</body></html>"#;
        let p = scan_html(html).unwrap();
        let inline = p.inline.unwrap();
        assert!(inline.contains("|musicTitle=Song & Dance,Artist|"));
        assert!(inline.contains("|left_data=200,300|"));
        assert!(!p.flags.ampersand_split);
        assert_eq!(p.external, None);
        assert!(scan_html("<input id=\"other\" value=\"x\">").is_none());
    }

    #[test]
    fn external_dos_and_textarea() {
        let html = r#"<input type="hidden" name="externalDos" id="externalDos" value="danoniA.txt">
<input type="hidden" name="externalDosCharset" id="externalDosCharset" value="Shift_JIS">
<textarea id="dos">|a=1&lt;2|</textarea>"#;
        let p = scan_html(html).unwrap();
        assert_eq!(p.external.as_deref(), Some("danoniA.txt"));
        // Entities next to multibyte text, `name` alone is not an id, and a
        // stray `<` in text does not swallow what follows.
        let jp = "<p>1 < 2 and don't</p><input name=\"dos\" value=\"|a=1|\">\
                  <input id=\"dos\" value=\"|t=ゆめ&amp;うつつ&ダンス|\">";
        assert_eq!(
            scan_html(jp).unwrap().inline.as_deref(),
            Some("|t=ゆめ&うつつ&ダンス|")
        );
        assert_eq!(p.external_charset.as_deref(), Some("Shift_JIS"));
        assert_eq!(p.inline.as_deref(), Some("|a=1<2|"));
    }

    #[test]
    fn external_scripts_are_read_not_run() {
        let js = "function externalDosInit() {\n\tg_externalDos = `\n|a=1|\n|b=\\`x\\`|\n`;\n}";
        assert_eq!(external_dos_text(js).as_deref(), Some("\n|a=1|\n|b=`x`|\n"));
        let built = "function externalDosInit() { g_externalDos = head + `|a=1|`; }";
        assert_eq!(external_dos_text(built), None);
        let pieces = "// g_externalDos is set below\nlet g_externalDos;\n\
                      g_externalDos = '|a=1|' + \"|b=2|\";\ng_externalDos += `|c=3|`;";
        assert_eq!(
            external_dos_text(pieces).as_deref(),
            Some("|a=1||b=2||c=3|")
        );
        assert_eq!(
            encoded_music("g_musicdata = \"SUQz\" +\n \"BA==\";").as_deref(),
            Some("SUQzBA==")
        );
        let subst = "g_externalDos = `|a=${x}|`;";
        assert_eq!(external_dos_text(subst), None);
        assert_eq!(
            encoded_music("function musicInit(){ g_musicdata = \"SUQz\"; }").as_deref(),
            Some("SUQz")
        );
    }
}
