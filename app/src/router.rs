//! Hash routes. The site lives under a sub-path on GitHub Pages, so routing
//! uses the fragment: `#/`, `#/play?song=<id>&chart=<n>`, `#/options`,
//! `#/calibrate`, `#/credits`, `#/results`.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Home,
    Play {
        song: String,
        chart: usize,
        /// `gfx=gl` forces the WebGL2 backend.
        force_gl: bool,
        /// `auto=1` plays the chart automatically (testing / demo).
        auto: bool,
        /// `bias=<ms>` makes autoplay press late by that much (testing).
        bias_ms: i64,
    },
    Options,
    Calibrate,
    /// A calibration run using the real gameplay path; `mode` is
    /// `combined`, `audio` or `visual`.
    CalibrateRun {
        mode: String,
        auto: bool,
        bias_ms: i64,
    },
    Credits,
    NotFound(String),
}

impl Route {
    pub(crate) fn parse(hash: &str) -> Route {
        let hash = hash.trim_start_matches('#');
        let (path, query) = match hash.split_once('?') {
            Some((p, q)) => (p, q),
            None => (hash, ""),
        };
        let path = path.trim_matches('/');
        let params: BTreeMap<&str, String> = query
            .split('&')
            .filter(|kv| !kv.is_empty())
            .map(|kv| match kv.split_once('=') {
                Some((k, v)) => (k, percent_decode(v)),
                None => (kv, String::new()),
            })
            .collect();
        match path {
            "" => Route::Home,
            "play" => Route::Play {
                song: params.get("song").cloned().unwrap_or_default(),
                chart: params
                    .get("chart")
                    .and_then(|c| c.parse().ok())
                    .unwrap_or(0),
                force_gl: params.get("gfx").is_some_and(|g| g == "gl"),
                auto: params.get("auto").is_some_and(|a| a == "1"),
                bias_ms: parse_bias(&params),
            },
            "options" => Route::Options,
            "calibrate" => Route::Calibrate,
            "calibrate/run" => Route::CalibrateRun {
                mode: params
                    .get("mode")
                    .cloned()
                    .unwrap_or_else(|| "combined".into()),
                auto: params.get("auto").is_some_and(|a| a == "1"),
                bias_ms: parse_bias(&params),
            },
            "credits" => Route::Credits,
            other => Route::NotFound(other.to_string()),
        }
    }

    pub(crate) fn to_hash(&self) -> String {
        match self {
            Route::Home => "#/".into(),
            Route::Play {
                song,
                chart,
                force_gl,
                auto,
                bias_ms,
            } => {
                let gl = if *force_gl { "&gfx=gl" } else { "" };
                let auto = if *auto { "&auto=1" } else { "" };
                format!(
                    "#/play?song={}&chart={chart}{gl}{auto}{}",
                    percent_encode(song),
                    bias_param(*bias_ms)
                )
            }
            Route::Options => "#/options".into(),
            Route::Calibrate => "#/calibrate".into(),
            Route::CalibrateRun {
                mode,
                auto,
                bias_ms,
            } => {
                let auto = if *auto { "&auto=1" } else { "" };
                format!("#/calibrate/run?mode={mode}{auto}{}", bias_param(*bias_ms))
            }
            Route::Credits => "#/credits".into(),
            Route::NotFound(p) => format!("#/{p}"),
        }
    }

    pub(crate) fn current() -> Route {
        let hash = web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default();
        Route::parse(&hash)
    }

    pub(crate) fn navigate(&self) {
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_hash(&self.to_hash());
        }
    }
}

fn parse_bias(params: &BTreeMap<&str, String>) -> i64 {
    params.get("bias").and_then(|b| b.parse().ok()).unwrap_or(0)
}

fn bias_param(bias_ms: i64) -> String {
    if bias_ms == 0 {
        String::new()
    } else {
        format!("&bias={bias_ms}")
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Decodes `%XX` escapes; malformed escapes are kept verbatim. Works on bytes
/// so a `%` followed by a multi-byte character cannot hit a non-char-boundary
/// `str` slice.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(&[hi, lo]) = bytes.get(i + 1..i + 3)
            && let (Some(hi), Some(lo)) = (hex_value(hi), hex_value(lo))
        {
            out.push(hi << 4 | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// The app crate only builds for wasm32; run with
// `wasm-pack test --headless --chrome app` or a configured
// `wasm-bindgen-test-runner`. `just test` does not cover it.
#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn decode_round_trips_encode() {
        for s in [
            "",
            "plain",
            "a b/c?d&e=f",
            "ünïcödé 日本語",
            "100%",
            "%zz%4",
        ] {
            assert_eq!(percent_decode(&percent_encode(s)), s, "{s}");
        }
    }

    #[wasm_bindgen_test]
    fn decode_edge_cases() {
        assert_eq!(percent_decode("%41%42"), "AB");
        assert_eq!(percent_decode("%4"), "%4");
        assert_eq!(percent_decode("%"), "%");
        assert_eq!(percent_decode("%g1"), "%g1");
        assert_eq!(percent_decode("a%"), "a%");
        // A `%` right before a multi-byte char must not panic.
        assert_eq!(percent_decode("%aé"), "%aé");
        assert_eq!(percent_decode("%é"), "%é");
        // Invalid UTF-8 from valid escapes is replaced, not rejected.
        assert_eq!(percent_decode("%ff"), "\u{fffd}");
    }

    #[wasm_bindgen_test]
    fn parse_play_route() {
        let r = Route::parse("#/play?song=into%2Dmy%2Ddream&chart=2&gfx=gl&auto=1");
        assert_eq!(
            r,
            Route::Play {
                song: "into-my-dream".into(),
                chart: 2,
                force_gl: true,
                auto: true,
                bias_ms: 0,
            }
        );
        assert_eq!(Route::parse(""), Route::Home);
        assert_eq!(Route::parse("#/"), Route::Home);
        assert_eq!(Route::parse("#/nope"), Route::NotFound("nope".into()));
        assert_eq!(Route::parse(&r.to_hash()), r);
    }
}
