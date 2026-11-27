//! `plugin.toml` (`docs/plans/source-plugins.md`, "Installing" and "The
//! manifest"): the plugin's folder's description of it, read by the app
//! before the plugin is ever started. The host parses the TOML into
//! [`Manifest`] and checks it with [`Manifest::problem`].
//!
//! ```toml
//! id = "example-songs"            # the folder's name too
//! name = { en = "Example Songs", ja = "…" }
//! publisher = "…"
//! version = "1.2.0"
//! homepage = "https://…"          # the source's page
//! protocol = [1]
//! executable = "example-songs"    # bin/<arch>-<os>/<executable>[.exe]
//! capabilities = ["search", "filters", "metadata", "fetch"]
//! inputs = []                     # or ["folder"], ["url"], ["link"], ["text"]
//! links = []                      # with "link": [{ host = "…", path = "/songs/" }]
//! reach = "keep"                 # or "rooms", or "play" (how far its songs may go)
//! attribution = { required = true, link_back = true }
//! ```

use serde::{Deserialize, Serialize};

use crate::limits::{MAX_LABEL_CHARS, MAX_LINKS, MAX_NAME_CHARS, MAX_URL_BYTES};
use crate::message::{Capability, InputKind, Reach};
use crate::text::{Text, clean};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Its id: lowercase ASCII letters, digits and `-`, at most 64, the
    /// name of its folder in `plugins/`.
    pub id: String,
    pub name: Text,
    pub publisher: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// The protocol versions it speaks.
    pub protocol: Vec<u32>,
    /// Its executable's file name in `bin/<arch>-<os>/` (`.exe` added on
    /// Windows).
    pub executable: String,
    pub capabilities: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<InputKind>,
    /// The links it claims (`inputs` has `link`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkClaim>,
    /// How far its songs may go: played here only (`play`), added to
    /// online rooms too (`rooms`), or imported as well (`keep`, where it
    /// says nothing: decided 2026-11-27). Each item may lower it.
    #[serde(default)]
    pub reach: Reach,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<Attribution>,
}

/// A link a plugin claims: its host, and the start of its path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkClaim {
    pub host: String,
    #[serde(default)]
    pub path: String,
}

/// What a source asks of those who use its songs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attribution {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub link_back: bool,
}

/// An id of a plugin's: lowercase ASCII letters, digits and `-`, a letter
/// or digit first, at most 64.
pub fn is_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A link a source names: http or https, at most [`MAX_URL_BYTES`], no
/// spaces or control characters.
pub fn is_web_link(url: &str) -> bool {
    url.len() <= MAX_URL_BYTES
        && (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() > "https://".len()
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// A name or a version as a manifest or a Hello writes it.
pub fn name_problem(what: &str, text: &str) -> Option<String> {
    if text.chars().count() > MAX_NAME_CHARS || clean(text).is_empty() || clean(text) != text {
        return Some(format!("{what}: empty, too long or with control characters"));
    }
    None
}

impl Manifest {
    /// Why it is not a manifest the app takes, or `None`.
    pub fn problem(&self) -> Option<String> {
        if !is_plugin_id(&self.id) {
            return Some(format!("id {:?}: lowercase letters, digits and -, at most 64", self.id));
        }
        if let Some(p) = self.name.problem(MAX_LABEL_CHARS) {
            return Some(format!("name: {p}"));
        }
        if let Some(p) = name_problem("publisher", &self.publisher) {
            return Some(p);
        }
        if let Some(p) = name_problem("version", &self.version) {
            return Some(p);
        }
        if self.homepage.as_deref().is_some_and(|h| !is_web_link(h)) {
            return Some("homepage: not an http or https link".into());
        }
        if self.protocol.is_empty() || self.protocol.len() > 16 || self.protocol.contains(&0) {
            return Some("protocol: the versions it speaks, at least one".into());
        }
        let exe = &self.executable;
        if exe.is_empty()
            || exe.len() > 128
            || exe.starts_with('.')
            || !exe
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Some("executable: a plain file name".into());
        }
        if self.capabilities.is_empty() || has_duplicates(&self.capabilities) {
            return Some("capabilities: at least one, each once".into());
        }
        if has_duplicates(&self.inputs) {
            return Some("inputs: each once".into());
        }
        let has = |c| self.capabilities.contains(&c);
        if has(Capability::Input) == self.inputs.is_empty() {
            return Some("inputs: listed exactly when the input capability is".into());
        }
        if has(Capability::Filters) && !has(Capability::Search) {
            return Some("capabilities: filters without search".into());
        }
        if self.inputs.contains(&InputKind::Link) == self.links.is_empty()
            || self.links.len() > MAX_LINKS
        {
            return Some("links: claimed exactly when the link input is, at most 16".into());
        }
        for link in &self.links {
            let host_ok = !link.host.is_empty()
                && link.host.len() <= 253
                && link
                    .host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'));
            let path_ok = link.path.len() <= 256
                && (link.path.is_empty() || link.path.starts_with('/'))
                && !link.path.chars().any(|c| c.is_whitespace() || c.is_control());
            if !host_ok || !path_ok {
                return Some(format!("links: {:?} is not a host and a path", link.host));
            }
        }
        None
    }

    /// It offers `capability` (and the app speaks it).
    pub fn has(&self, capability: Capability) -> bool {
        !capability.reserved() && self.capabilities.contains(&capability)
    }
}

fn has_duplicates<T: Ord + Copy>(list: &[T]) -> bool {
    let mut sorted = list.to_vec();
    sorted.sort_unstable();
    sorted.windows(2).any(|w| w[0] == w[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        serde_json::from_value(serde_json::json!({
            "id": "example-songs",
            "name": {"en": "Example Songs"},
            "publisher": "Someone",
            "version": "1.2.0",
            "protocol": [1],
            "executable": "example-songs",
            "capabilities": ["search", "filters", "metadata", "fetch"],
        }))
        .unwrap()
    }

    #[test]
    fn a_plain_manifest_is_taken_and_its_songs_go_everywhere_by_default() {
        let m = manifest();
        assert_eq!(m.problem(), None);
        assert_eq!(m.reach, Reach::Keep);
        assert!(m.has(Capability::Fetch));
        assert!(!m.has(Capability::Browse));
    }

    #[test]
    fn the_level_is_read_by_its_names_and_the_old_room_rule_is_not() {
        let with = |key: &str, value: &str| {
            let mut v = serde_json::to_value(manifest()).unwrap();
            v[key] = value.into();
            serde_json::from_value::<Manifest>(v)
        };
        assert_eq!(with("reach", "play").unwrap().reach, Reach::Play);
        assert_eq!(with("reach", "rooms").unwrap().reach, Reach::Rooms);
        assert_eq!(with("reach", "keep").unwrap().reach, Reach::Keep);
        assert!(with("reach", "upload").is_err());
        let old = with("rooms", "none").unwrap_err().to_string();
        assert!(old.contains("unknown field `rooms`"), "{old}");
    }

    #[test]
    fn an_item_lowers_its_sources_level_and_never_raises_it() {
        use Reach::*;
        assert!(Play < Rooms && Rooms < Keep);
        for (source, item, is) in [
            (Keep, None, Keep),
            (Keep, Some(Rooms), Rooms),
            (Keep, Some(Play), Play),
            (Rooms, Some(Keep), Rooms),
            (Rooms, Some(Play), Play),
            (Play, Some(Keep), Play),
            (Play, Some(Rooms), Play),
            (Play, None, Play),
        ] {
            assert_eq!(source.narrowed(item), is, "{source:?} by {item:?}");
        }
        assert!(!Play.rooms() && Rooms.rooms() && Keep.rooms());
        assert!(!Play.keeps() && !Rooms.keeps() && Keep.keeps());
    }

    #[test]
    fn manifests_that_contradict_themselves_are_refused() {
        let bad: [fn(&mut Manifest); 9] = [
            |m| m.id = "Example".into(),
            |m| m.id = "../x".into(),
            |m| m.executable = "../../bin/sh".into(),
            |m| m.executable = "a/b".into(),
            |m| m.capabilities.push(Capability::Search),
            |m| m.inputs = vec![InputKind::Url],
            |m| m.capabilities.retain(|c| *c != Capability::Search),
            |m| m.homepage = Some("javascript:alert(1)".into()),
            |m| m.publisher = "\u{202E}x".into(),
        ];
        for (i, f) in bad.iter().enumerate() {
            let mut m = manifest();
            f(&mut m);
            assert!(m.problem().is_some(), "case {i}");
        }
        let mut m = manifest();
        m.capabilities.push(Capability::Input);
        m.inputs = vec![InputKind::Link];
        assert!(m.problem().is_some());
        m.links = vec![LinkClaim {
            host: "songs.example".into(),
            path: "/song/".into(),
        }];
        assert_eq!(m.problem(), None);
    }
}
