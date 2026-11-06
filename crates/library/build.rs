//! Turns `src/announcer/examples.toml` into a Rust table
//! (`$OUT_DIR/examples.rs`): the studio's examples, read here so the
//! game carries no TOML parser. What each key means is checked by the
//! library's tests (`announcer::examples`).

use std::collections::BTreeMap;
use std::fmt::Write;

const SOURCE: &str = "src/announcer/examples.toml";

fn main() {
    println!("cargo::rerun-if-changed={SOURCE}");
    let text = std::fs::read_to_string(SOURCE).expect(SOURCE);
    // Set → line folder → examples.
    let sets: BTreeMap<String, BTreeMap<String, Vec<String>>> =
        toml::from_str(&text).unwrap_or_else(|e| panic!("{SOURCE}: {e}"));
    let mut out = String::from(
        "/// The examples of `examples.toml`: (set, line folder, examples).\n\
         pub(crate) const MORE: &[(&str, &str, &[&str])] = &[\n",
    );
    for (set, lines) in &sets {
        for (folder, examples) in lines {
            let _ = writeln!(out, "    ({set:?}, {folder:?}, &{examples:?}),");
        }
    }
    out.push_str("];\n");
    let path =
        std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("examples.rs");
    std::fs::write(path, out).expect("examples.rs");
}
