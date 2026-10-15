//! Reading `infinito.toml`: at build time (`build.rs` turns it into
//! tables, so the game carries no TOML parser) and in the tests.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    fonts: Fonts,
    colors: BTreeMap<String, String>,
    metrics: BTreeMap<String, f32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fonts {
    text: String,
    numbers: String,
    numbers_weight: u16,
}

/// The tunables, checked.
#[derive(Debug, PartialEq)]
pub struct Tunables {
    pub text: String,
    pub numbers: String,
    pub numbers_weight: u16,
    pub colors: Vec<(String, [f32; 4])>,
    pub metrics: Vec<(String, f32)>,
}

/// `#rrggbb` or `#rrggbbaa` as encoded RGBA.
pub fn parse_color(s: &str) -> Option<[f32; 4]> {
    let hex = s.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let a = if hex.len() == 8 { byte(6)? } else { 255 };
    Some([
        f32::from(byte(0)?) / 255.0,
        f32::from(byte(2)?) / 255.0,
        f32::from(byte(4)?) / 255.0,
        f32::from(a) / 255.0,
    ])
}

/// Parses a tunables file: every section and key known, every colour a
/// colour, every metric finite.
pub fn parse(source: &str) -> Result<Tunables, String> {
    let f: File = toml::from_str(source).map_err(|e| e.to_string())?;
    let mut colors = Vec::new();
    for (k, v) in &f.colors {
        colors.push((
            k.clone(),
            parse_color(v).ok_or_else(|| format!("colour {k}: {v:?}"))?,
        ));
    }
    for (k, v) in &f.metrics {
        if !v.is_finite() {
            return Err(format!("metric {k}: {v}"));
        }
    }
    Ok(Tunables {
        text: f.fonts.text,
        numbers: f.fonts.numbers,
        numbers_weight: f.fonts.numbers_weight,
        colors,
        metrics: f.metrics.into_iter().collect(),
    })
}
