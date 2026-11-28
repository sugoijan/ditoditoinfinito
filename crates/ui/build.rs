//! Turns `src/state/experimental.toml` into a Rust table
//! (`$OUT_DIR/experimental.rs`): the experimental settings screen's
//! entries, read here so the game carries no TOML parser. Each key's
//! binding to its setting is in `crate::state::experimental`, whose tests
//! hold the two to each other.

use std::fmt::Write;

use serde::Deserialize;

const SOURCE: &str = "src/state/experimental.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    entry: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    key: String,
    group: String,
    name: String,
    line: String,
    kind: String,
    default: bool,
    platforms: Vec<String>,
    desktop: String,
    web: String,
    play: bool,
}

fn main() {
    println!("cargo::rerun-if-changed={SOURCE}");
    let text = std::fs::read_to_string(SOURCE).expect(SOURCE);
    let file: File = toml::from_str(&text).unwrap_or_else(|e| panic!("{SOURCE}: {e}"));
    let mut out = String::from(
        "/// The entries of `experimental.toml`, in its order.\n\
         pub const ENTRIES: &[Entry] = &[\n",
    );
    let mut keys = std::collections::HashSet::new();
    for e in &file.entry {
        assert!(keys.insert(e.key.clone()), "{SOURCE}: {} twice", e.key);
        assert_eq!(e.kind, "switch", "{SOURCE}: {}: only switches for now", e.key);
        for p in &e.platforms {
            assert!(
                p == "web" || p == "desktop",
                "{SOURCE}: {}: platform {p}",
                e.key
            );
        }
        let _ = writeln!(
            out,
            "    Entry {{ key: {:?}, group: {:?}, name: {:?}, line: {:?}, default: {}, \
             web: {}, desktop: {}, desktop_flag: {:?}, web_address: {:?}, play: {} }},",
            e.key,
            e.group,
            e.name,
            e.line,
            e.default,
            e.platforms.iter().any(|p| p == "web"),
            e.platforms.iter().any(|p| p == "desktop"),
            e.desktop,
            e.web,
            e.play,
        );
    }
    out.push_str("];\n");
    let path =
        std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("experimental.rs");
    std::fs::write(path, out).expect("experimental.rs");
}
