//! A plugin's messages held to version 1's limits, and its Hello to its
//! manifest. The app runs these on everything a plugin sends; a plugin
//! (the SDK) may run them on what it is about to send.

use std::collections::{BTreeMap, BTreeSet};

use crate::limits::*;
use crate::manifest::{Manifest, is_web_link, name_problem};
use crate::message::*;
use crate::text::{clean, text_problem};

/// Why `id` (an item's, a node's, a scope's, a filter's) is not one, or
/// `None`.
pub fn id_problem(what: &str, id: &str) -> Option<String> {
    if id.is_empty() || id.len() > MAX_ID_BYTES || id.chars().any(char::is_control) {
        return Some(format!("{what}: empty, over {MAX_ID_BYTES} bytes or with control characters"));
    }
    None
}

/// Why `path` is not a fetched file's path inside its item: relative,
/// `/`-separated, each part a plain name (no `.`, `..`, empty part, `\`,
/// `:` or control character), at most [`MAX_PATH_BYTES`] and
/// [`MAX_PATH_DEPTH`] parts.
pub fn path_problem(path: &str) -> Option<&'static str> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES {
        return Some("empty or too long");
    }
    let mut depth = 0;
    for part in path.split('/') {
        depth += 1;
        if part.is_empty() || part == "." || part == ".." {
            return Some("not a plain relative path");
        }
        if part
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | ':') || crate::text::is_bidi_control(c))
        {
            return Some("a part with a character no file name takes here");
        }
    }
    if depth > MAX_PATH_DEPTH {
        return Some("too deep");
    }
    None
}

fn opt_text(what: &str, text: &Option<String>, max: usize) -> Option<String> {
    let t = text.as_deref()?;
    text_problem(t, max).map(|p| format!("{what}: {p}"))
}

/// Why `item` is not an item's metadata of version 1, or `None`.
pub fn item_problem(item: &ItemMeta) -> Option<String> {
    if let Some(p) = id_problem("item id", &item.id) {
        return Some(p);
    }
    if let Some(p) = text_problem(&item.title, MAX_ITEM_TEXT_CHARS) {
        return Some(format!("title: {p}"));
    }
    for (what, text) in [
        ("subtitle", &item.subtitle),
        ("artist", &item.artist),
        ("title_translit", &item.title_translit),
        ("subtitle_translit", &item.subtitle_translit),
        ("artist_translit", &item.artist_translit),
        ("pack", &item.pack),
        ("license", &item.license),
        ("attribution", &item.attribution),
        ("updated", &item.updated),
    ] {
        if let Some(p) = opt_text(what, text, MAX_ITEM_TEXT_CHARS) {
            return Some(p);
        }
    }
    if item.charts.len() > MAX_CHARTS {
        return Some(format!("more than {MAX_CHARTS} charts"));
    }
    for chart in &item.charts {
        let style_ok = !chart.style.is_empty()
            && chart.style.len() <= 64
            && chart
                .style
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !style_ok {
            return Some("a chart's style is not a layout id".into());
        }
        if chart.meter.is_some_and(|m| m > 1000) {
            return Some("a chart's meter past 1000".into());
        }
        for (what, text) in [("chart name", &chart.name), ("chart author", &chart.author)] {
            if let Some(p) = opt_text(what, text, MAX_LABEL_CHARS) {
                return Some(p);
            }
        }
    }
    if item.length.is_some_and(|l| !l.is_finite() || !(0.0..=86_400.0).contains(&l)) {
        return Some("length: seconds, at most a day".into());
    }
    if item
        .bpm
        .is_some_and(|[a, b]| !a.is_finite() || !b.is_finite() || a <= 0.0 || a > b || b > 1e6)
    {
        return Some("bpm: the lowest and the highest, positive".into());
    }
    if item.page.as_deref().is_some_and(|p| !is_web_link(p)) {
        return Some("page: not an http or https link".into());
    }
    if let Some(p) = item.revision.as_deref().and_then(|r| id_problem("revision", r)) {
        return Some(p);
    }
    if item.song_key.as_deref().is_some_and(|k| !is_song_key(k)) {
        return Some("song_key: `s1:` and 16 hex digits".into());
    }
    None
}

/// The game's `song_key` shape: `s1:` and 16 lowercase hex digits.
pub fn is_song_key(key: &str) -> bool {
    key.strip_prefix("s1:").is_some_and(|h| {
        h.len() == 16 && h.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    })
}

/// Why `node` is not a node of version 1, or `None`.
pub fn node_problem(node: &Node) -> Option<String> {
    id_problem("node id", &node.id).or_else(|| {
        text_problem(&node.title, MAX_ITEM_TEXT_CHARS).map(|p| format!("node title: {p}"))
    })
}

/// Why `filters` are not a version 1 declaration, or `None`.
pub fn filters_problem(filters: &[Filter]) -> Option<String> {
    if filters.len() > MAX_FILTERS {
        return Some(format!("more than {MAX_FILTERS} filters"));
    }
    let mut ids = BTreeSet::new();
    for f in filters {
        if let Some(p) = id_problem("filter id", &f.id) {
            return Some(p);
        }
        if !ids.insert(&f.id) {
            return Some(format!("filter {:?} declared twice", f.id));
        }
        if let Some(p) = f.label.problem(MAX_LABEL_CHARS) {
            return Some(format!("filter {:?}'s label: {p}", f.id));
        }
        match &f.kind {
            FilterKind::Text { max_chars } => {
                if max_chars.is_some_and(|m| m == 0 || m as usize > MAX_FILTER_TEXT_CHARS) {
                    return Some(format!("filter {:?}: max_chars out of range", f.id));
                }
            }
            FilterKind::Choice { choices, .. } => {
                if choices.is_empty() || choices.len() > MAX_CHOICES {
                    return Some(format!("filter {:?}: 1 to {MAX_CHOICES} choices", f.id));
                }
                let mut seen = BTreeSet::new();
                for c in choices {
                    if let Some(p) = id_problem("choice id", &c.id) {
                        return Some(p);
                    }
                    if !seen.insert(&c.id) {
                        return Some(format!("filter {:?}: a choice twice", f.id));
                    }
                    if let Some(p) = c.label.problem(MAX_LABEL_CHARS) {
                        return Some(format!("filter {:?}'s choice: {p}", f.id));
                    }
                }
            }
            FilterKind::Range { min, max, step, .. } => {
                let finite = min.is_finite() && max.is_finite() && step.is_finite();
                if !finite || min >= max || *step <= 0.0 || (max - min) / step > 1e6 {
                    return Some(format!("filter {:?}: not a range", f.id));
                }
            }
            FilterKind::Toggle {} => {}
        }
    }
    None
}

/// Why `values` are not values of `filters` (the app's own, before a
/// search is sent; a plugin may check what it is given), or `None`.
pub fn filter_values_problem(
    filters: &[Filter],
    values: &BTreeMap<String, FilterValue>,
) -> Option<String> {
    for (id, value) in values {
        let Some(filter) = filters.iter().find(|f| &f.id == id) else {
            return Some(format!("no filter {id:?}"));
        };
        let ok = match (&filter.kind, value) {
            (FilterKind::Text { max_chars }, FilterValue::Text { value }) => {
                let max = max_chars.map_or(MAX_FILTER_TEXT_CHARS, |m| m as usize);
                value.chars().count() <= max.min(MAX_FILTER_TEXT_CHARS)
            }
            (FilterKind::Choice { multiple, choices }, FilterValue::Choice { values }) => {
                (*multiple || values.len() <= 1)
                    && values.len() <= choices.len()
                    && values.iter().all(|v| choices.iter().any(|c| &c.id == v))
            }
            (FilterKind::Range { min, max, .. }, FilterValue::Range { min: lo, max: hi }) => {
                let inside = |v: &Option<f64>| v.is_none_or(|v| v.is_finite() && (*min..=*max).contains(&v));
                inside(lo) && inside(hi) && !matches!((lo, hi), (Some(a), Some(b)) if a > b)
            }
            (FilterKind::Toggle {}, FilterValue::Toggle { .. }) => true,
            _ => false,
        };
        if !ok {
            return Some(format!("filter {id:?}: a value it does not take"));
        }
    }
    None
}

/// Why `message` (with an attachment of `attachment` bytes) breaks
/// version 1's limits, or `None`. What it means in its operation (which
/// operation, in what order) is the host's to check.
pub fn plugin_message_problem(message: &PluginMessage) -> Option<String> {
    if message.op() == Some(0) {
        return Some("operation 0".into());
    }
    match message {
        PluginMessage::Hello {
            features,
            plugin,
            filters,
            ..
        } => {
            if features.len() > MAX_FEATURES
                || features
                    .iter()
                    .any(|f| f.is_empty() || f.len() > MAX_FEATURE_BYTES)
            {
                return Some("features: too many or too long".into());
            }
            name_problem("plugin version", &plugin.version)
                .or_else(|| filters_problem(filters))
        }
        PluginMessage::Opened { scope, title, .. } => id_problem("scope", scope).or_else(|| {
            title
                .as_ref()
                .and_then(|t| t.problem(MAX_LABEL_CHARS))
                .map(|p| format!("scope title: {p}"))
        }),
        PluginMessage::Results { items, .. } => {
            if items.is_empty() || items.len() > MAX_PAGE as usize {
                return Some("results: 1 to a page of items".into());
            }
            items.iter().find_map(item_problem)
        }
        PluginMessage::Nodes { nodes, .. } => {
            if nodes.is_empty() || nodes.len() > MAX_PAGE as usize {
                return Some("nodes: 1 to a page".into());
            }
            nodes.iter().find_map(node_problem)
        }
        PluginMessage::Done { cursor, .. } => cursor.as_deref().and_then(|c| {
            (c.is_empty() || c.len() > MAX_CURSOR_BYTES || c.chars().any(char::is_control))
                .then(|| "cursor: empty, too long or with control characters".into())
        }),
        PluginMessage::Item { item, .. } => item_problem(item),
        PluginMessage::Progress {
            done, total, bytes, ..
        } => {
            let _ = bytes;
            match (done, total) {
                (Some(d), Some(t)) if d > t => Some("progress: done past total".into()),
                _ => None,
            }
        }
        PluginMessage::File {
            path, size, offset, ..
        } => path_problem(path)
            .map(|p| format!("file path: {p}"))
            .or_else(|| (offset > size).then(|| "file: offset past its size".into())),
        PluginMessage::Fetched { delivery, .. } => match delivery {
            Delivery::Archive { path, .. } => path_problem(path).map(|p| format!("archive path: {p}")),
            Delivery::InPlace { path } => (!path.is_absolute())
                .then(|| "in_place: not an absolute path".into()),
            Delivery::Files { .. } | Delivery::Cached { .. } => None,
        },
        PluginMessage::Failed { detail, .. } => detail
            .as_deref()
            .is_some_and(|d| d.len() > MAX_DETAIL_BYTES)
            .then(|| format!("detail: over {MAX_DETAIL_BYTES} bytes")),
        PluginMessage::Bye {} => None,
    }
}

/// What a plugin's Hello means for the app.
#[derive(Clone, Debug, PartialEq)]
pub struct Accepted {
    pub protocol: u32,
    /// What the app will offer: the Hello's, less the reserved.
    pub capabilities: BTreeSet<Capability>,
    pub inputs: BTreeSet<InputKind>,
    pub filters: Vec<Filter>,
}

/// Why a Hello is not taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelloProblem {
    /// No version in common: the plugin speaks newer ones than the app
    /// (`plugin_newer`; "this plugin needs a newer version of the game")
    /// or only older ones.
    Incompatible { plugin_newer: bool },
    /// It is not what its manifest says, or breaks the limits.
    Invalid(String),
}

/// A plugin's Hello against its manifest and the versions the app speaks.
pub fn accept_hello(
    message: &PluginMessage,
    manifest: &Manifest,
    app_versions: &[u32],
) -> Result<Accepted, HelloProblem> {
    let PluginMessage::Hello {
        protocol,
        plugin,
        capabilities,
        inputs,
        filters,
        ..
    } = message
    else {
        return Err(HelloProblem::Invalid("not a Hello".into()));
    };
    if !app_versions.contains(protocol) {
        let newest = app_versions.iter().copied().max().unwrap_or(0);
        return Err(HelloProblem::Incompatible {
            plugin_newer: *protocol > newest,
        });
    }
    let invalid = |s: String| HelloProblem::Invalid(s);
    if let Some(p) = plugin_message_problem(message) {
        return Err(invalid(p));
    }
    if !manifest.protocol.contains(protocol) {
        return Err(invalid(format!("version {protocol} is not in its manifest")));
    }
    if plugin.id != manifest.id || plugin.version != manifest.version {
        return Err(invalid("its id or version is not its manifest's".into()));
    }
    if capabilities.len() > 16 || inputs.len() > 8 {
        return Err(invalid("too many capabilities or inputs".into()));
    }
    if let Some(c) = capabilities.iter().find(|c| !manifest.capabilities.contains(c)) {
        return Err(invalid(format!("{c:?} is not in its manifest")));
    }
    if let Some(i) = inputs.iter().find(|i| !manifest.inputs.contains(i)) {
        return Err(invalid(format!("input {i:?} is not in its manifest")));
    }
    let capabilities: BTreeSet<Capability> = capabilities
        .iter()
        .copied()
        .filter(|c| !c.reserved())
        .collect();
    if capabilities.contains(&Capability::Input) == inputs.is_empty() {
        return Err(invalid("inputs listed exactly when the input capability is".into()));
    }
    if !filters.is_empty() && !capabilities.contains(&Capability::Filters) {
        return Err(invalid("filters without the filters capability".into()));
    }
    Ok(Accepted {
        protocol: *protocol,
        capabilities,
        inputs: inputs.iter().copied().collect(),
        filters: filters.clone(),
    })
}

/// A plugin's text as the app shows it: cleaned ([`clean`]).
pub fn shown(text: &str) -> String {
    clean(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Text;

    #[test]
    fn paths_stay_inside_their_item() {
        for ok in ["song.ssc", "Pack/Song/music.ogg", "a b/c (1).png", "曲/曲.ssc"] {
            assert_eq!(path_problem(ok), None, "{ok}");
        }
        for bad in [
            "", "/etc/passwd", "../x", "a/../../x", "a//b", "./a", "a\\b", "C:x", "a\0b",
            "a/\u{202E}b",
        ] {
            assert!(path_problem(bad).is_some(), "{bad:?}");
        }
        assert!(path_problem("a/".repeat(17).trim_end_matches('/')).is_some());
        assert!(path_problem(&"x".repeat(MAX_PATH_BYTES + 1)).is_some());
    }

    #[test]
    fn items_past_the_limits_are_refused() {
        let ok = ItemMeta {
            id: "song-1".into(),
            title: "Song".into(),
            song_key: Some("s1:0123456789abcdef".into()),
            page: Some("https://songs.example/1".into()),
            bpm: Some([120.0, 180.0]),
            ..Default::default()
        };
        assert_eq!(item_problem(&ok), None);
        let bad: [fn(&mut ItemMeta); 7] = [
            |i| i.id = "x".repeat(MAX_ID_BYTES + 1),
            |i| i.title = "x".repeat(MAX_ITEM_TEXT_CHARS + 1),
            |i| i.title = "\u{0007}".into(),
            |i| i.page = Some("file:///etc".into()),
            |i| i.song_key = Some("s1:XYZ".into()),
            |i| i.bpm = Some([200.0, 100.0]),
            |i| i.length = Some(f64::INFINITY),
        ];
        for (n, f) in bad.iter().enumerate() {
            let mut item = ok.clone();
            f(&mut item);
            assert!(item_problem(&item).is_some(), "case {n}");
        }
        let page = PluginMessage::Results {
            op: 1,
            items: vec![ok; MAX_PAGE as usize + 1],
        };
        assert!(plugin_message_problem(&page).is_some());
    }

    #[test]
    fn filter_values_follow_their_declaration() {
        let filters = vec![
            Filter {
                id: "level".into(),
                label: Text::en("Level"),
                kind: FilterKind::Range {
                    min: 1.0,
                    max: 20.0,
                    step: 1.0,
                    unit: Unit::Meter,
                },
            },
            Filter {
                id: "style".into(),
                label: Text::en("Style"),
                kind: FilterKind::Choice {
                    multiple: false,
                    choices: vec![
                        Choice { id: "single".into(), label: Text::en("Single") },
                        Choice { id: "double".into(), label: Text::en("Double") },
                    ],
                },
            },
        ];
        assert_eq!(filters_problem(&filters), None);
        let values = |v: Vec<(&str, FilterValue)>| {
            v.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<BTreeMap<_, _>>()
        };
        assert_eq!(
            filter_values_problem(
                &filters,
                &values(vec![("level", FilterValue::Range { min: Some(3.0), max: None })])
            ),
            None
        );
        for bad in [
            values(vec![("level", FilterValue::Range { min: Some(30.0), max: None })]),
            values(vec![("style", FilterValue::Choice { values: vec!["single".into(), "double".into()] })]),
            values(vec![("style", FilterValue::Toggle { value: true })]),
            values(vec![("nope", FilterValue::Toggle { value: true })]),
        ] {
            assert!(filter_values_problem(&filters, &bad).is_some());
        }
    }

    #[test]
    fn a_hello_must_be_what_its_manifest_says() {
        let manifest: Manifest = serde_json::from_value(serde_json::json!({
            "id": "p", "name": {"en": "P"}, "publisher": "Q", "version": "1",
            "protocol": [1], "executable": "p", "capabilities": ["search", "fetch", "upload"],
        }))
        .unwrap();
        let hello = |protocol, capabilities: Vec<Capability>, id: &str| PluginMessage::Hello {
            protocol,
            features: vec![],
            plugin: PluginInfo { id: id.into(), version: "1".into() },
            capabilities,
            inputs: vec![],
            filters: vec![],
        };
        let ok = accept_hello(
            &hello(1, vec![Capability::Search, Capability::Upload], "p"),
            &manifest,
            &[1],
        )
        .unwrap();
        assert_eq!(ok.capabilities, BTreeSet::from([Capability::Search]));
        assert_eq!(
            accept_hello(&hello(2, vec![], "p"), &manifest, &[1]),
            Err(HelloProblem::Incompatible { plugin_newer: true })
        );
        assert_eq!(
            accept_hello(&hello(1, vec![], "p"), &manifest, &[2, 3]),
            Err(HelloProblem::Incompatible { plugin_newer: false })
        );
        assert!(matches!(
            accept_hello(&hello(1, vec![Capability::Browse], "p"), &manifest, &[1]),
            Err(HelloProblem::Invalid(_))
        ));
        assert!(matches!(
            accept_hello(&hello(1, vec![Capability::Search], "q"), &manifest, &[1]),
            Err(HelloProblem::Invalid(_))
        ));
    }
}
