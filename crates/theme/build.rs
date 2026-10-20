//! Turns `src/infinito.toml` into Rust tables (`$OUT_DIR/tunables.rs`):
//! the file is checked here, and the game carries no TOML parser.

#[path = "src/tunables.rs"]
mod tunables;

use std::fmt::Write;

fn main() {
    println!("cargo::rerun-if-changed=src/infinito.toml");
    println!("cargo::rerun-if-changed=src/tunables.rs");
    let source = std::fs::read_to_string("src/infinito.toml").expect("src/infinito.toml");
    let t = tunables::parse(&source).unwrap_or_else(|e| panic!("src/infinito.toml: {e}"));
    let mut out = String::new();
    let _ = writeln!(out, "pub(crate) const TEXT_FONT: &str = {:?};", t.text);
    let _ = writeln!(
        out,
        "pub(crate) const NUMBERS_FONT: &str = {:?};",
        t.numbers
    );
    let _ = writeln!(
        out,
        "pub(crate) const NUMBERS_WEIGHT: u16 = {};",
        t.numbers_weight
    );
    let _ = writeln!(out, "pub(crate) const COLORS: &[(&str, [f32; 4])] = &[");
    for (k, c) in &t.colors {
        let _ = writeln!(
            out,
            "    ({k:?}, [{:?}, {:?}, {:?}, {:?}]),",
            c[0], c[1], c[2], c[3]
        );
    }
    let _ = writeln!(out, "];");
    let _ = writeln!(out, "pub(crate) const METRICS: &[(&str, f32)] = &[");
    for (k, v) in &t.metrics {
        let _ = writeln!(out, "    ({k:?}, {v:?}),");
    }
    let _ = writeln!(out, "];");
    let path =
        std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("tunables.rs");
    std::fs::write(path, out).expect("tunables.rs");
}
