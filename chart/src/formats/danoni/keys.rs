//! Key modes a work defines in its header, and key names.
//!
//! A work may define its own key mode (`|keyCtrl7wi=…|chara7wi=…|`) or
//! redefine a built-in one; danoniplus reads these in `keysConvert`
//! (`js/lib/dosConverter.js` 2343–2755). Only the first pattern of each
//! attribute is read (the one danoniplus plays by default), with its
//! shorthands: `1...5` and `3...+4` ranges, `x@:n` repeats, `a!b@:n` groups,
//! `b`-prefixed lower-row positions, and references to a built-in mode's
//! first pattern (`7_0`, `7_0_0`, `a>4A`-style prefixed lane names). A
//! reference to another pattern of a built-in mode is reported as
//! unsupported rather than guessed.

use crate::layout::danoni::{self as layouts, KeyDef};

use super::dos::{Dos, parse_float, parse_int, split_lf2};
use super::keycodes::KEY_CODES;

/// danoniplus `g_escapeStr.keyCtrlName`: short names for codes.
const KEY_NAME_ESCAPES: [(&str, &str); 16] = [
    ("ShiftLeft", "Shift"),
    ("ControlLeft", "Control"),
    ("AltLeft", "Alt"),
    ("Digit", "D"),
    ("Numpad", "N"),
    ("Semicolon", ";"),
    ("Multiply", "*"),
    ("Add", "+"),
    ("Subtract", "-"),
    ("Decimal", "."),
    ("Divide", "Div"),
    ("Quote", "Ja-Colon"),
    ("BracketLeft", "Ja-@"),
    ("BracketRight", "Ja-["),
    ("Backslash", "Ja-]"),
    ("Equal", "Ja-^"),
];

/// The `KeyboardEvent.code` of a danoniplus key name (`getKeyCtrlVal`): a
/// code (`KeyD`), a short name (`D`, `Left`, `D1`, `;`), or a legacy key
/// code number. `None` for anything else.
pub fn key_code(name: &str) -> Option<String> {
    // Each code's short name, computed once.
    static ESCAPED: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let escaped = ESCAPED.get_or_init(|| {
        KEY_CODES
            .iter()
            .map(|(_, code)| {
                KEY_NAME_ESCAPES
                    .iter()
                    .fold(code.to_string(), |s, (from, to)| s.replace(from, to))
            })
            .collect()
    });
    for ((_, code), short) in KEY_CODES.iter().zip(escaped) {
        if *code == name
            || code.strip_prefix("Key") == Some(name)
            || code.strip_prefix("Arrow") == Some(name)
            || short == name
        {
            return Some(code.to_string());
        }
    }
    let n = parse_int(name)?;
    KEY_CODES
        .iter()
        .find(|(k, _)| i64::from(*k) == n)
        .map(|(_, c)| c.to_string())
}

/// Most lanes a key mode may have (lanes are numbered with a `u8`; the
/// largest built-in mode has 23, the largest known work 43).
pub const MAX_LANES: usize = 255;

/// Most entries a shorthand list may expand to: well above any lane count,
/// low enough that `x@:999999999` or `1...1e9` cannot exhaust memory.
const MAX_EXPANDED: usize = 1024;

/// Longest entry of a shorthand list read; longer ones are dropped (no
/// lane name, key list or number comes close), so a repeated entry stays
/// small too.
const MAX_ENTRY_BYTES: usize = 256;

/// Most alternative keys read for one lane (`S/D/F/…`).
const MAX_KEYS_PER_LANE: usize = 8;

/// danoniplus `keyTransPattern`: older key labels.
pub fn canonical_label(label: &str) -> &str {
    match label {
        "9" | "DP" | "9A-1" | "9A-2" => "9A",
        "9B-1" | "9B-2" => "9B",
        "himsiyauz" => "9h",
        "TP" => "13",
        "15" => "15A",
        "15R" => "15B",
        other => other,
    }
}

/// The key mode a chart is played in: the header's definition when it has
/// one (`Ok(Some)`, header = true), else the built-in mode, else `Ok(None)`.
pub fn key_def(dos: &Dos, mode: &str) -> Result<Option<(KeyDef, bool)>, String> {
    let builtin = layouts::builtin(mode);
    let appended = dos
        .get(&format!("append{mode}"))
        .is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let Some(ctrl) = dos.val(&format!("keyCtrl{mode}")) else {
        return Ok(builtin.map(|b| (b, false)));
    };
    if appended && builtin.is_some() {
        // Extra patterns after the built-in ones: pattern 0 stays built in.
        return Ok(builtin.map(|b| (b, false)));
    }
    let first = |s: &str| split_lf2(s).into_iter().next().unwrap_or_default();
    let keys: Vec<Vec<String>> = expand(&first(ctrl))
        .into_iter()
        .map(|t| {
            if let Some(k) = partial_keys(&t) {
                return Ok(k);
            }
            match pattern_ref(&t)? {
                Some(def) => Ok(def.keys),
                None => Ok(vec![
                    t.split('/')
                        .take(MAX_KEYS_PER_LANE)
                        .filter_map(key_code)
                        .collect::<Vec<String>>(),
                ]),
            }
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .flatten()
        .collect();
    let n = keys.len();
    if n == 0 {
        return Err(format!("key mode {mode} defines no lanes"));
    }
    if n > MAX_LANES {
        return Err(format!(
            "key mode {mode} defines {n} lanes; at most {MAX_LANES} are supported"
        ));
    }
    let base = builtin.filter(|b| b.chara.len() == n);
    let from_builtin = base.is_some();
    let mut def = base.unwrap_or_else(|| KeyDef {
        name: mode.to_string(),
        chara: (1..=n).map(|i| format!("{i}a")).collect(),
        color: vec![0; n],
        shuffle: vec![0; n],
        shape: vec![Ok(0.0); n],
        pos: (0..n).map(|i| i as f32).collect(),
        div: n as f32,
        div_max: None,
        keys: Vec::new(),
    });
    def.name = mode.to_string();
    def.keys = keys;

    if let Some(v) = dos.val(&format!("chara{mode}")) {
        let mut chara = Vec::new();
        for t in expand(&first(v)) {
            if let Some((prefix, ptn)) = t.split_once('>').filter(|(p, _)| !p.is_empty()) {
                // A prefix before a pattern's names, or before a plain name
                // (`a>left` is `aleft`), as `expandKeyPtn`.
                let names = match partial_chara(ptn) {
                    Some(n) => n,
                    None => match pattern_ref(ptn)? {
                        Some(d) => d.chara,
                        None => vec![ptn.to_string()],
                    },
                };
                chara.extend(names.iter().map(|c| format!("{prefix}{c}")));
            } else if let Some(names) = partial_chara(&t) {
                chara.extend(names);
            } else if let Some(d) = pattern_ref(&t)? {
                chara.extend(d.chara);
            } else {
                chara.push(t);
            }
        }
        def.chara = pad(chara, n, |i| format!("{}a", i + 1));
    }
    let group = |s: &str| first(s).split('/').next().unwrap_or_default().to_string();
    if let Some(v) = dos.val(&format!("color{mode}")) {
        def.color = numbers(&group(v), n, |d| d.color)?;
    }
    if let Some(v) = dos.val(&format!("shuffle{mode}")) {
        def.shuffle = numbers(&group(v), n, |d| d.shuffle)?;
    }
    if let Some(v) = dos.val(&format!("stepRtn{mode}")) {
        let g = group(v);
        let mut shape = Vec::new();
        if let Some(d) = pattern_ref(&g)? {
            shape = d.shape;
        } else {
            for t in expand(&g) {
                if let Some(p) = partial_shapes(&t) {
                    shape.extend(p);
                    continue;
                }
                match pattern_ref(&t)? {
                    Some(d) => shape.extend(d.shape),
                    // `parseInt`, as keysConvert does for custom modes.
                    None => shape.push(match parse_int(&t) {
                        Some(r) => Ok(r as f32),
                        None => Err(t),
                    }),
                }
            }
        }
        def.shape = pad(shape, n, |_| Ok(0.0));
    }
    if let Some(v) = dos.val(&format!("div{mode}")) {
        let seg = v.split('$').next().unwrap_or_default();
        let mut parts = seg.split(',');
        let d0 = parts.next().unwrap_or_default();
        if let Some(d) = pattern_ref(d0)? {
            def.div = d.div;
            def.div_max = d.div_max;
        } else if let Some(d) = parse_int(d0) {
            def.div = d as f32;
            def.div_max = parts.next().and_then(|m| pos_value(m, def.div));
        }
    }
    let explicit_div = dos.val(&format!("div{mode}")).is_some();
    if let Some(v) = dos.val(&format!("pos{mode}")) {
        let mut pos = Vec::new();
        for t in expand(&first(v)) {
            match pattern_ref(&t)? {
                Some(d) => pos.extend(d.pos),
                None => pos.extend(pos_value(&t, def.div)),
            }
        }
        def.pos = pad(pos, n, |i| i as f32);
        // `setKeyDfVal` only fills in a missing `div`: a redefined built-in
        // mode keeps its own.
        if !explicit_div && !from_builtin {
            def.div = def.pos.iter().copied().fold(0.0, f32::max) + 1.0;
            def.div_max = None;
        }
    }
    def.color = pad(def.color, n, |_| 0);
    def.shuffle = pad(def.shuffle, n, |_| 0);
    def.shape = pad(def.shape, n, |_| Ok(0.0));
    def.pos = pad(def.pos, n, |i| i as f32);
    Ok(Some((def, true)))
}

/// Partial lane lists danoniplus defines for custom modes
/// (`ptcharaX`, `danoni_constants.js` 3435–3438): `4A`, and `4A_s` for the
/// same names with a letter in front (`sleft`, …).
fn partial_chara(p: &str) -> Option<Vec<String>> {
    let (base, prefix) = match p.split_once('_') {
        Some((b, l)) if l.len() == 1 && l.bytes().all(|c| c.is_ascii_lowercase()) => (b, l),
        Some(_) => return None,
        None => (p, ""),
    };
    let names: &[&str] = match base {
        "4A" => &["left", "down", "up", "right"],
        "3S" => &["left", "leftdia", "down"],
        "3J" => &["up", "rightdia", "right"],
        "7" if !prefix.is_empty() || p == "7" => &[
            "left", "leftdia", "down", "space", "up", "rightdia", "right",
        ],
        _ => return None,
    };
    Some(names.iter().map(|n| format!("{prefix}{n}")).collect())
}

/// Partial key lists (`keyCtrl4A` … `keyCtrl3Z`, 3730–3739).
fn partial_keys(p: &str) -> Option<Vec<Vec<String>>> {
    let names: &[&[&str]] = match p {
        "4A" => &[&["Left"], &["Down"], &["Up"], &["Right"]],
        "4S" => &[&["S"], &["D"], &["E", "R"], &["F"]],
        "4J" => &[&["J"], &["K"], &["I", "O"], &["L"]],
        "4W" => &[&["W"], &["E"], &["D3", "D4"], &["R"]],
        "4U" => &[&["U"], &["I"], &["D8", "D9"], &["O"]],
        "3S" => &[&["S"], &["D"], &["F"]],
        "3J" => &[&["J"], &["K"], &["L"]],
        "3W" => &[&["W"], &["E"], &["R"]],
        "3Z" => &[&["Z"], &["X"], &["C"]],
        _ => return None,
    };
    Some(
        names
            .iter()
            .map(|ks| ks.iter().filter_map(|k| key_code(k)).collect())
            .collect(),
    )
}

/// Partial shapes (`stepRtn4A` … `stepRtn3Z`, 3604–3608).
fn partial_shapes(p: &str) -> Option<Vec<Result<f32, String>>> {
    let aa = |s: &str| Err(s.to_string());
    Some(match p {
        "4A" => vec![Ok(0.0), Ok(-90.0), Ok(90.0), Ok(180.0)],
        "3S" => vec![Ok(0.0), Ok(-45.0), Ok(-90.0)],
        "3J" => vec![Ok(90.0), Ok(135.0), Ok(180.0)],
        "3Z" => vec![aa("giko"), aa("onigiri"), aa("iyo")],
        _ => return None,
    })
}

/// A `posX` value: a number, or `b<n>` meaning `n` places into the lower
/// row (`div + n`).
fn pos_value(s: &str, div: f32) -> Option<f32> {
    match s.strip_prefix('b') {
        Some(r) => parse_float(r).map(|v| v as f32 + div),
        None => parse_float(s).map(|v| v as f32),
    }
}

/// Numbers of a colour or shuffle group list, with pattern references.
fn numbers(s: &str, n: usize, field: impl Fn(KeyDef) -> Vec<u8>) -> Result<Vec<u8>, String> {
    if let Some(d) = pattern_ref(s)? {
        return Ok(pad(field(d), n, |_| 0));
    }
    let mut out = Vec::new();
    for t in expand(s) {
        match pattern_ref(&t)? {
            Some(d) => out.extend(field(d)),
            None => out.push(parse_int(&t).unwrap_or(0).clamp(0, 255) as u8),
        }
    }
    Ok(pad(out, n, |_| 0))
}

/// A reference to the first pattern of a built-in mode (`7_0`, `7_0_0`,
/// `12_(0)`); `Ok(None)` when `s` is not a reference at all.
fn pattern_ref(s: &str) -> Result<Option<KeyDef>, String> {
    let s = s.replace("(", "").replace(")", "");
    let mut parts = s.split('_');
    let (Some(mode), Some(ptn)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };
    if mode.is_empty() || ptn.is_empty() || !ptn.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let group = parts.next();
    if group.is_some_and(|g| g.is_empty() || !g.bytes().all(|b| b.is_ascii_digit())) {
        return Ok(None);
    }
    if parts.next().is_some() {
        return Ok(None);
    }
    let mode = canonical_label(mode);
    match layouts::builtin(mode) {
        Some(d) if ptn == "0" && group.is_none_or(|g| g == "0") => Ok(Some(d)),
        Some(_) => Err(format!(
            "refers to key pattern {s}, which this importer does not know"
        )),
        None => Ok(None),
    }
}

/// danoniplus `toOriginalArray(…, toSameValStr)`: a comma list with its
/// shorthands written out.
fn expand(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for t in s.split(',').filter(|t| t.len() <= MAX_ENTRY_BYTES) {
        for v in same_val(t).split(',') {
            if out.len() >= MAX_EXPANDED {
                return out;
            }
            // `padArray` skips empty entries, moving the rest up.
            if !v.is_empty() {
                out.push(v.to_string());
            }
        }
    }
    out
}

/// `toSameValStr`: `x@:n` repeats `x` (a `!`-separated group) `n` times.
fn same_val(s: &str) -> String {
    let parts: Vec<&str> = s.split("@:").collect();
    let group = range(parts[0]).split('!').collect::<Vec<_>>().join(",");
    match (parts.len(), parts.get(1).and_then(|n| parse_int(n))) {
        (2, Some(n)) if n <= 0 => String::new(),
        (2, Some(n)) => {
            // Never more copies than the expanded list can hold.
            let per = group.split(',').count().max(1);
            let n = (n as usize).min(MAX_EXPANDED.div_ceil(per));
            vec![group; n].join(",")
        }
        _ => group,
    }
}

/// `toFloatStr`: `1...5` → `1,2,3,4,5`, `3...+2` → `3,4,5`, keeping a
/// leading `b`.
fn range(s: &str) -> String {
    let parts: Vec<&str> = s.split("...").collect();
    if parts.len() != 2 {
        return s.to_string();
    }
    let (mark, start) = match parts[0].strip_prefix('b') {
        Some(r) => ("b", r),
        None => ("", parts[0]),
    };
    let (Some(a), Some(b)) = (parse_float(start), parse_float(parts[1])) else {
        return s.to_string();
    };
    let end = if parts[1].starts_with('+') { a + b } else { b };
    let mut out = Vec::new();
    let mut k = a;
    while k <= end && out.len() < MAX_EXPANDED {
        out.push(format!("{mark}{k}"));
        k += 1.0;
    }
    out.join(",")
}

fn pad<T>(mut v: Vec<T>, n: usize, fill: impl Fn(usize) -> T) -> Vec<T> {
    v.truncate(n);
    while v.len() < n {
        v.push(fill(v.len()));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::danoni::dos::DosFlags;

    #[test]
    fn key_names_like_get_key_ctrl_val() {
        assert_eq!(key_code("D").as_deref(), Some("KeyD"));
        assert_eq!(key_code("Left").as_deref(), Some("ArrowLeft"));
        assert_eq!(key_code("D1").as_deref(), Some("Digit1"));
        assert_eq!(key_code(";").as_deref(), Some("Semicolon"));
        assert_eq!(key_code("Ja-@").as_deref(), Some("BracketLeft"));
        assert_eq!(key_code("Shift").as_deref(), Some("ShiftLeft"));
        assert_eq!(key_code("Space").as_deref(), Some("Space"));
        assert_eq!(key_code("37").as_deref(), Some("ArrowLeft"));
        assert_eq!(key_code("KeyQ").as_deref(), Some("KeyQ"));
        assert_eq!(key_code("nonsense"), None);
    }

    #[test]
    fn shorthands() {
        assert_eq!(expand("1...3,5"), ["1", "2", "3", "5"]);
        assert_eq!(expand("3...+2"), ["3", "4", "5"]);
        assert_eq!(expand("0@:3"), ["0", "0", "0"]);
        assert_eq!(
            expand("onigiri!giko@:2"),
            ["onigiri", "giko", "onigiri", "giko"]
        );
        assert_eq!(expand("b0...2"), ["b0", "b1", "b2"]);
        // Huge shorthands stay bounded.
        assert_eq!(expand("0@:999999999").len(), MAX_EXPANDED);
        assert!(expand(&"S/".repeat(100_000)).is_empty());
        // Empty entries are skipped and `x@:0` gives nothing (`padArray`).
        assert_eq!(expand("1,,2,x@:0,3"), ["1", "2", "3"]);
        assert_eq!(expand("a!b!c@:999999999,1...1e12").len(), MAX_EXPANDED);
        assert_eq!(expand("1...1e12").len(), MAX_EXPANDED);
        let dos = Dos::parse("|keyCtrl9z=S@:999999999|", DosFlags::default());
        assert!(key_def(&dos, "9z").unwrap_err().contains("at most 255"));
    }

    #[test]
    fn header_key_modes() {
        let dos = Dos::parse(
            "|keyCtrl6=S,D,F,J,K,L|chara6=a>3S,3J|stepRtn6=0,-45,-90,90,135,180|\
             color6=0,1,0,0,1,0|shuffle6=0@:6|",
            DosFlags::default(),
        );
        let (def, header) = key_def(&dos, "6").unwrap().unwrap();
        assert!(header);
        assert_eq!(def.keys[0], ["KeyS"]);
        // `a>3S` prefixes the partial list `3S`; `3J` is one too.
        assert_eq!(
            def.chara,
            ["aleft", "aleftdia", "adown", "up", "rightdia", "right"]
        );
        assert_eq!(def.shape[1], Ok(-45.0));
        assert_eq!(def.color, [0, 1, 0, 0, 1, 0]);
        assert_eq!(def.pos, [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(def.div, 6.0);

        // References to built-in patterns and two rows.
        let dos = Dos::parse(
            "|keyCtrl9x=4W,7_0|chara9x=4A_t,7_0|div9x=4|\
             pos9x=0...3,b0...b6|stepRtn9x=4A,7_0_0|color9x=3@:4,7_0_0|",
            DosFlags::default(),
        );
        let (def, _) = key_def(&dos, "9x").unwrap().unwrap();
        assert_eq!(def.keys.len(), 11);
        assert_eq!(def.keys[4], ["KeyS"]);
        assert_eq!(def.keys[2], ["Digit3", "Digit4"]);
        assert_eq!(def.chara[0], "tleft");
        assert_eq!(def.shape[1], Ok(-90.0));
        assert_eq!(def.chara[4], "left");
        assert_eq!(def.pos[4], 4.0);
        assert_eq!(def.pos[10], 10.0);
        assert_eq!(def.color[4..], [0, 1, 0, 2, 0, 1, 0]);
        let layout = def.layout("x".into());
        assert_eq!(layout.lanes.iter().filter(|l| l.row == 1).count(), 7);

        // A plain name after `>` takes the prefix too; a redefined built-in
        // mode with positions keeps its `div`.
        let dos = Dos::parse(
            "|keyCtrl11=4A,3S,Space,3J|chara11=a>left,a>down,4A_s,7_0|pos11=2...12|",
            DosFlags::default(),
        );
        let (def, _) = key_def(&dos, "11").unwrap().unwrap();
        assert_eq!(def.chara[..2], ["aleft", "adown"]);
        assert_eq!(def.div, 6.0);

        // A built-in mode without a header definition.
        let (def, header) = key_def(&Dos::default(), "7").unwrap().unwrap();
        assert!(!header);
        assert_eq!(def.chara[3], "space");
        // An unknown mode with no definition.
        assert!(key_def(&Dos::default(), "99z").unwrap().is_none());
        // Another pattern of a built-in mode is unsupported.
        let dos = Dos::parse("|keyCtrl5x=12_1|", DosFlags::default());
        assert!(key_def(&dos, "5x").is_err());
    }
}
