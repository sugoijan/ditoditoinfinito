//! The web game's own error reports (the stats proposal's step 4,
//! `errors-v1`): what a report says and the scrub its message goes through,
//! shared by the game, which sends it, and the server (ddi-server's
//! `errors.rs`), which takes it. Not part of the room's protocol: a report
//! goes by `navigator.sendBeacon` to `<api>/stats/error`, never over a
//! room's socket, and no peer computes anything from it.
//!
//! **Nothing identifying**: a report is a kind from a fixed list
//! ([`KINDS`]), the route's kind ([`ROUTES`]), the client's kind as the
//! usage report says it ([`crate::client_stats`]) and a message scrubbed by
//! [`scrub`] (paths, addresses, quoted text, ids, long numbers and any
//! non-ASCII letters taken out; [`MAX_MESSAGE`] characters at most). The
//! server scrubs again and refuses a report whose message its scrub would
//! change ([`Report::check`]), so a client that skipped the scrub sends
//! nothing that is kept.
//!
//! What can still pass: an English word of a song's or a person's name in
//! an unquoted message of the game's own (`no chart Hard for Lumen`). The
//! kind and the fingerprint are what the reports are for; the text is kept
//! only as one sample a fingerprint, for 30 days.

use serde::{Deserialize, Serialize};

use crate::client_stats as cs;

/// The report's schema (`s`).
pub const SCHEMA: u32 = 1;

/// A report's JSON at most (bytes); the server reads no more than twice it.
pub const MAX_BYTES: usize = 2_048;

/// A message's characters at most, after the scrub.
pub const MAX_MESSAGE: usize = 300;

/// What went wrong. `panic`: the game's code panicked (the crash page);
/// `unhandled_rejection`: a promise failed with nobody to catch it;
/// `wasm_trap`: the game's code trapped outside a panic (its message one of
/// [`TRAPS`]); `gpu_lost`: the graphics device was lost; `no_gpu`: no
/// graphics device could be had (the no-GPU page); `boot_*`: the page could
/// not start the game, as its loader classifies it (no message).
pub const KINDS: [&str; 13] = [
    "panic",
    "unhandled_rejection",
    "wasm_trap",
    "gpu_lost",
    "no_gpu",
    "boot_init_failed",
    "boot_no_wasm",
    "boot_no_wasm_simd",
    "boot_no_fetch",
    "boot_no_textdecoder",
    "boot_no_raf",
    "boot_no_pointer",
    "boot_no_audio",
];

/// A trap's message, as the loader words it.
pub const TRAPS: [&str; 4] = ["unreachable", "out_of_bounds", "call_indirect", "other"];

/// The route's kind (never the address or its parameters).
pub const ROUTES: [&str; 8] = [
    "start", "select", "play", "room", "watch", "options", "edit", "other",
];

/// One report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub s: u32,
    pub kind: String,
    pub route: String,
    /// The game's version (`client_stats::valid_build`), or `unknown`
    /// (the loader's own reports, made before the game knows it).
    #[serde(default = "unknown")]
    pub build: String,
    #[serde(default = "unknown")]
    pub system: String,
    #[serde(default = "unknown")]
    pub form: String,
    #[serde(default = "unknown")]
    pub gpu: String,
    /// The message, scrubbed ([`scrub`]); empty for none.
    #[serde(default)]
    pub msg: String,
}

fn unknown() -> String {
    "unknown".into()
}

impl Report {
    /// Whether every value is from its list, the message is what [`scrub`]
    /// makes of it (so a message that was not scrubbed, or scrubbed
    /// otherwise, is refused) and its kind may carry one.
    pub fn check(&self) -> bool {
        let one = |v: &str, l: &[&str]| l.contains(&v);
        self.s == SCHEMA
            && one(&self.kind, &KINDS)
            && one(&self.route, &ROUTES)
            && (self.build == "unknown" || cs::valid_build(&self.build))
            && one(&self.system, &cs::SYSTEMS)
            && one(&self.form, &cs::FORMS)
            && one(&self.gpu, &cs::GPUS)
            && self.msg.chars().count() <= MAX_MESSAGE
            && scrub(&self.msg) == self.msg
            && match self.kind.as_str() {
                "wasm_trap" => one(&self.msg, &TRAPS),
                k if k.starts_with("boot_") => self.msg.is_empty(),
                _ => true,
            }
    }

    /// Its fingerprint: [`fingerprint`] of its kind and message.
    pub fn fingerprint(&self) -> String {
        fingerprint(&self.kind, &self.msg)
    }

    pub fn text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// A report of `kind` on `route` (a route's kind, [`route_kind`]) by a
/// client of `client`'s kind (none: the loader's, before the game knows
/// it), its message `raw` scrubbed ([`scrub`]); none for a kind not in
/// [`KINDS`]. A kind that carries no message ([`Report::check`]) gets
/// none; a trap's is put to [`TRAPS`]' word.
pub fn report(kind: &str, route: &str, client: Option<cs::ClientKind>, raw: &str) -> Option<Report> {
    let kind = KINDS.iter().find(|k| **k == kind)?;
    let route = ROUTES.iter().find(|r| **r == route).unwrap_or(&"other");
    let msg = match *kind {
        "wasm_trap" => trap_word(raw).to_string(),
        k if k.starts_with("boot_") => String::new(),
        _ => scrub(raw),
    };
    let c = client.map(cs::ClientKind::checked);
    Some(Report {
        s: SCHEMA,
        kind: kind.to_string(),
        route: route.to_string(),
        build: c.map_or_else(unknown, |c| c.build.to_string()),
        system: c.map_or_else(unknown, |c| c.system.to_string()),
        form: c.map_or_else(unknown, |c| c.form.to_string()),
        gpu: c.map_or_else(unknown, |c| c.gpu.to_string()),
        msg,
    })
}

/// A trap's message as one of [`TRAPS`].
pub fn trap_word(raw: &str) -> &'static str {
    let l = raw.to_ascii_lowercase();
    if l.contains("unreachable") {
        "unreachable"
    } else if l.contains("out of bounds") {
        "out_of_bounds"
    } else if l.contains("call_indirect") || l.contains("indirect call") {
        "call_indirect"
    } else {
        "other"
    }
}

/// Whether a device sends reports at all: the player's row on (Options →
/// Data → Send error reports) and no Global Privacy Control signal from
/// the browser.
pub fn may_send(row_on: bool, privacy_signal: bool) -> bool {
    row_on && !privacy_signal
}

/// Reports a page sends a session at most (kept across the loader's crash
/// reloads), and one a fingerprint.
pub const PER_SESSION: usize = 5;

/// What a page sent this session (`sessionStorage`'s `ddi.errors.v1`,
/// which the loader's own reports share: `{"seen":[…]}`, a fingerprint or
/// the loader's `kind:word` each).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sent {
    #[serde(default)]
    pub seen: Vec<String>,
}

/// Its key in `sessionStorage`.
pub const SENT_KEY: &str = "ddi.errors.v1";

impl Sent {
    pub fn read(text: Option<&str>) -> Sent {
        text.and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or_default()
    }

    pub fn text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Whether a report of `key` may go now (then counted): never the
    /// same twice, at most [`PER_SESSION`] in all.
    pub fn allow(&mut self, key: &str) -> bool {
        if self.seen.len() >= PER_SESSION || self.seen.iter().any(|s| s == key) {
            return false;
        }
        self.seen.push(key.to_string());
        true
    }
}

/// A report's text read: `None` where it is longer than [`MAX_BYTES`],
/// unreadable, has a field it should not or fails [`Report::check`].
pub fn read(text: &str) -> Option<Report> {
    if text.len() > MAX_BYTES {
        return None;
    }
    serde_json::from_str::<Report>(text)
        .ok()
        .filter(Report::check)
}

/// A report's fingerprint: 16 hex digits of a 64-bit FNV-1a hash of its
/// kind and scrubbed message (no key: it identifies the error, not who
/// had it).
pub fn fingerprint(kind: &str, msg: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in kind.bytes().chain(*b"\n").chain(msg.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The kind of a route as the web's address names it (`#/play?…` →
/// `play`), never more of it.
pub fn route_kind(hash: &str) -> &'static str {
    let path = hash.trim_start_matches('#');
    let path = path.split(['?', '&']).next().unwrap_or("");
    let first = path.trim_start_matches('/').split('/').next().unwrap_or("");
    match first {
        "" => "start",
        "select" => "select",
        "play" => "play",
        "options" => "options",
        "edit" => "edit",
        "room" => "room",
        "mp" if path.contains("/watch/") || path.contains("/audience/") => "watch",
        "mp" => "room",
        _ => "other",
    }
}

// ---- the scrub ----

/// The placeholders the scrub puts in what it takes out.
pub const PLACEHOLDERS: [&str; 8] = [
    "<text>", "<url>", "<path>", "<email>", "<ip>", "<n>", "<hex>", "<id>",
];

fn is_open_quote(c: char) -> Option<char> {
    match c {
        '"' => Some('"'),
        '`' => Some('`'),
        '“' => Some('”'),
        '‘' => Some('’'),
        '「' => Some('」'),
        '『' => Some('』'),
        '«' => Some('»'),
        _ => None,
    }
}

/// Quoted text out: `"…"`, `` `…` ``, typographic and Japanese quotes, and
/// `'…'` where the opening `'` does not follow a letter or digit (an
/// apostrophe inside a word stays). An unclosed double quote takes the
/// rest of the message.
fn unquote(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // An apostrophe follows a letter or a digit of ASCII, or a
        // placeholder (what the scrub left of a word).
        let prev_word = i > 0 && (chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '>');
        let close = match c {
            '\'' if !prev_word => Some('\''),
            '\'' => None,
            _ => is_open_quote(c),
        };
        if let Some(close) = close {
            // A close escaped by a backslash (`Debug`'s `\"`) is not one.
            let found = (i + 1..chars.len()).find(|&j| chars[j] == close && chars[j - 1] != '\\');
            match found.map(|j| j - i - 1) {
                Some(n) => {
                    out.push_str("<text>");
                    i += n + 2;
                    continue;
                }
                None if c == '"' || c == '“' || c == '「' || c == '『' => {
                    out.push_str("<text>");
                    break;
                }
                None => {}
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| (1..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

fn is_ipv6(s: &str) -> bool {
    s.matches(':').count() >= 2
        && s.split(':').all(|p| p.len() <= 4 && (p.is_empty() || is_hex(p)))
        && s.bytes().any(|b| b.is_ascii_hexdigit())
}

/// A Rust source frame, from the crate's folder: the game's own
/// (`crates/ui/src/x.rs:10:5` → `ui/src/x.rs:10:5`), a dependency's from
/// Cargo's folders (`…/registry/src/<index>/wgpu-26.0.1/src/x.rs` →
/// `wgpu-26.0.1/src/x.rs`) or the standard library's
/// (`/rustc/<hash>/library/core/src/x.rs` → `core/src/x.rs`), where every
/// name is a plain lowercase one; else none (any other path, a song's
/// folder or a home's, is no frame).
fn frame(token: &str) -> Option<String> {
    let t = token.replace('\\', "/");
    let ours = t.starts_with("crates/");
    let cargo = t.contains("/registry/src/") || t.contains("/git/checkouts/");
    let std = t.starts_with("/rustc/") && t.contains("/library/");
    // A frame as this scrub writes it (`ui/src/x.rs:1:1`), read again.
    let kept = t.split('/').nth(1) == Some("src") && !t.starts_with('/');
    if !(ours || cargo || std || kept) {
        return None;
    }
    let i = t.find(".rs")?;
    let (path, pos) = (&t[..i + 3], &t[i + 3..]);
    if !(pos.is_empty()
        || (pos.starts_with(':')
            && pos[1..].split(':').count() <= 2
            && pos[1..]
                .split(':')
                .all(|n| !n.is_empty() && n.len() <= 6 && n.bytes().all(|b| b.is_ascii_digit()))))
    {
        return None;
    }
    let parts: Vec<&str> = path.split('/').collect();
    let src = parts.iter().rposition(|p| *p == "src")?;
    if src == 0 {
        return None;
    }
    let plain = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    };
    let krate = parts[src - 1];
    let crate_ok = plain(krate) && krate.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        || (krate.contains('-')
            && krate
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-.".contains(&b)));
    let inner = &parts[src + 1..];
    let file = inner.last()?;
    let inner_ok = inner[..inner.len() - 1].iter().all(|p| plain(p))
        && file
            .strip_suffix(".rs")
            .is_some_and(|f| plain(f) && !f.contains('-'));
    (crate_ok && inner_ok).then(|| format!("{}{pos}", parts[src - 1..].join("/")))
}

/// What a token of letters and digits becomes: itself, or a placeholder
/// where it looks like an id or a number.
fn word(w: &str) -> &str {
    let digits = w.bytes().filter(u8::is_ascii_digit).count();
    let letters = w.bytes().filter(u8::is_ascii_alphabetic).count();
    let mut longest_digits = 0;
    let mut run = 0;
    for b in w.bytes() {
        run = if b.is_ascii_digit() { run + 1 } else { 0 };
        longest_digits = longest_digits.max(run);
    }
    if longest_digits >= 6 && letters == 0 {
        return "<n>";
    }
    if w.len() >= 8 && is_hex(w) {
        return "<hex>";
    }
    let changes = w
        .as_bytes()
        .windows(2)
        .filter(|p| {
            p[0].is_ascii_alphabetic()
                && p[1].is_ascii_alphabetic()
                && p[0].is_ascii_uppercase() != p[1].is_ascii_uppercase()
        })
        .count();
    // A lowercase letter alone between capitals (`zsuvRbEvax`): no word
    // of the game's is written so.
    let b = w.as_bytes();
    let lone = b.windows(3).any(|t| {
        t[0].is_ascii_uppercase() && t[1].is_ascii_lowercase() && t[2].is_ascii_uppercase()
    });
    if w.len() >= 8 && ((digits > 0 && letters > 0) || lone || changes >= 8) {
        return "<id>";
    }
    if longest_digits >= 6 {
        return "<id>";
    }
    w
}

/// A message with everything that could name someone or something of
/// theirs taken out (see the module's doc and `step4` in the progress
/// note), then cut to [`MAX_MESSAGE`] characters. Idempotent:
/// `scrub(scrub(x)) == scrub(x)`.
pub fn scrub(message: &str) -> String {
    // Control characters out, whitespace runs one space.
    let flat: String = message
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    let flat = flat.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    // Quotes out, then any other character beyond ASCII but a letter
    // (punctuation, symbols) a space, so no two words meet where it was.
    let unquoted: String = unquote(&flat)
        .chars()
        .map(|c| if !c.is_ascii() && !c.is_alphanumeric() { ' ' } else { c })
        .collect();
    let mut out: Vec<String> = Vec::new();
    for token in unquoted.split(' ').filter(|t| !t.is_empty()) {
        out.push(scrub_token(token));
    }
    let mut text = out.join(" ");
    if text.chars().count() > MAX_MESSAGE {
        let cut_in_word = text.chars().nth(MAX_MESSAGE) != Some(' ');
        text = text.chars().take(MAX_MESSAGE).collect();
        // A token cut in two is dropped rather than left half (half a
        // frame or a placeholder would read otherwise the next time).
        if cut_in_word && let Some(i) = text.rfind(' ') {
            text.truncate(i);
        }
    }
    text.trim().to_string()
}

fn scrub_token(token: &str) -> String {
    if token.is_empty() || PLACEHOLDERS.contains(&token) {
        return token.to_string();
    }
    // Punctuation around a token kept around its placeholder.
    let lead: String = token
        .chars()
        .take_while(|c| "([{,;".contains(*c))
        .collect();
    let rest = &token[lead.len()..];
    let trail_len = rest
        .chars()
        .rev()
        .take_while(|c| ")]},;.!?".contains(*c))
        .map(char::len_utf8)
        .sum::<usize>();
    let (body, trail) = rest.split_at(rest.len() - trail_len);
    // `body:` after a word (`panicked at …:` or `id:`) keeps its colon.
    let (body, colon) = match body.strip_suffix(':') {
        Some(b) if !b.is_empty() => (b, ":"),
        _ => (body, ""),
    };
    let lower = body.to_ascii_lowercase();
    let replaced: String = if body.is_empty() {
        String::new()
    } else if lower.contains("://")
        || lower.starts_with("www.")
        || ["blob:", "data:", "file:", "javascript:", "mailto:"]
            .iter()
            .any(|p| lower.starts_with(p))
    {
        "<url>".into()
    } else if body.contains('@') {
        "<email>".into()
    } else if is_ipv4(body) || is_ipv6(body) {
        "<ip>".into()
    } else if body.contains('/') || body.contains('\\') {
        match frame(body) {
            Some(f) => f,
            None if body.contains('?') || body.contains('#') => "<url>".into(),
            None => "<path>".into(),
        }
    } else if body.contains('?') && body.contains('=') {
        "<url>".into()
    } else {
        scrub_words(body)
    };
    format!("{lead}{replaced}{colon}{trail}")
}

/// A token without slashes: each run of letters and digits put through
/// [`word`], runs of non-ASCII letters out, the rest as it is.
fn scrub_words(body: &str) -> String {
    let mut out = String::new();
    let mut ascii = String::new();
    let mut foreign = false;
    let flush = |out: &mut String, ascii: &mut String| {
        if !ascii.is_empty() {
            out.push_str(word(ascii));
            ascii.clear();
        }
    };
    for c in body.chars() {
        if c.is_ascii_alphanumeric() || c == '_' && !ascii.is_empty() {
            if foreign {
                out.push_str("<text>");
                foreign = false;
            }
            if c == '_' {
                flush(&mut out, &mut ascii);
                out.push('_');
            } else {
                ascii.push(c);
            }
        } else if !c.is_ascii() && (c.is_alphanumeric() || c.is_alphabetic()) {
            flush(&mut out, &mut ascii);
            foreign = true;
        } else {
            flush(&mut out, &mut ascii);
            if foreign {
                out.push_str("<text>");
                foreign = false;
            }
            if c.is_ascii() {
                out.push(c);
            }
        }
    }
    flush(&mut out, &mut ascii);
    if foreign {
        out.push_str("<text>");
    }
    out
}

#[cfg(test)]
mod tests;
