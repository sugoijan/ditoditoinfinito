//! Hash routes. The site lives under a sub-path on GitHub Pages, so routing
//! uses the fragment: `#/` (Start), `#/select` (Songs), `#/play?song=<id>&chart=<n>`, `#/options`,
//! `#/calibrate`, `#/credits`, `#/results`, `#/edit?song=<id>&chart=<n>`,
//! `#/video-check`, `#/skin-preview`, `#/ui-test`, `#/ui-dev`.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Home,
    Songs,
    Play {
        song: String,
        chart: usize,
        /// `gfx=gl` forces the WebGL2 backend.
        force_gl: bool,
        /// `auto=1` plays the chart automatically (testing / demo).
        auto: bool,
        /// `auto=pad` autoplays through a test gamepad the headless tests
        /// install (`window.__DDI_FAKE_PAD`); implies `auto`.
        auto_pad: bool,
        /// `bias=<ms>` makes autoplay press late by that much (testing).
        bias_ms: i64,
        /// `slow=1` makes each frame 40 ms longer while a background movie
        /// is drawn, to test the automatic video setting.
        slow: bool,
        /// `replay=<play id>`: a kept play watched again (its song and
        /// chart are the record's; `auto` and `bias` mean nothing then).
        replay: Option<String>,
        /// `view=results`: the kept play's results, not its replay.
        view_results: bool,
        /// `edit=1`: the chart editor's playtest (`crate::editor`): the
        /// song as edited, never kept, back to the editor.
        edit: bool,
    },
    /// The chart editor on a song (`components::edit_page`).
    Edit {
        song: String,
        chart: usize,
    },
    /// Songs shared by link (`crate::share`): song select on the share
    /// `id`, or the page saying the link is gone.
    Shared {
        id: String,
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
    /// The video decoder check; `src=a,b` checks those site files,
    /// `decoder=webcodecs` the browser's decoder and `decoder=both` both,
    /// comparing their frames.
    VideoCheck {
        sources: Vec<String>,
        decoder: String,
    },
    /// A noteskin's elements alone (`components::skin_preview`):
    /// `skin=<key>`, `size`, `bg`, `beat`, `bpm`, `notes`, `receptors`,
    /// `gfx=gl`.
    SkinPreview {
        params: Vec<(String, String)>,
    },
    /// The 2D pass's test scene (`components::ui_test`): `t` (seconds,
    /// held still), `scroll` (the list's rows, held still), `speed`,
    /// `delay` (ms before the late texture shows), `budget` (text pages
    /// kept), `all`.
    UiTest {
        params: Vec<(String, String)>,
    },
    /// The UI core's developer screen (`components::ui_dev`), drawn by
    /// the game on the whole window; `screen=widgets` opens the widget
    /// screen at once.
    UiDev {
        screen: String,
    },
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
        if let Some(id) = path.strip_prefix("s/")
            && ddi_library::share::valid_id(id)
        {
            return Route::Shared { id: id.to_string() };
        }
        match path {
            "" => Route::Home,
            "select" => Route::Songs,
            "play" => Route::Play {
                song: params.get("song").cloned().unwrap_or_default(),
                chart: params
                    .get("chart")
                    .and_then(|c| c.parse().ok())
                    .unwrap_or(0),
                force_gl: params.get("gfx").is_some_and(|g| g == "gl"),
                auto: params.get("auto").is_some_and(|a| a == "1" || a == "pad"),
                auto_pad: params.get("auto").is_some_and(|a| a == "pad"),
                bias_ms: parse_bias(&params),
                slow: params.get("slow").is_some_and(|v| v == "1"),
                replay: params.get("replay").filter(|r| !r.is_empty()).cloned(),
                view_results: params.get("view").is_some_and(|v| v == "results"),
                edit: params.get("edit").is_some_and(|v| v == "1"),
            },
            "edit" => Route::Edit {
                song: params.get("song").cloned().unwrap_or_default(),
                chart: params
                    .get("chart")
                    .and_then(|c| c.parse().ok())
                    .unwrap_or(0),
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
            "skin-preview" => Route::SkinPreview {
                params: params
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            },
            "ui-dev" => Route::UiDev {
                screen: params.get("screen").cloned().unwrap_or_default(),
            },
            "ui-test" => Route::UiTest {
                params: params
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            },
            "video-check" => Route::VideoCheck {
                sources: params
                    .get("src")
                    .map(|s| {
                        s.split(',')
                            .filter(|p| !p.is_empty())
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                decoder: params.get("decoder").cloned().unwrap_or_default(),
            },
            other => Route::NotFound(other.to_string()),
        }
    }

    pub(crate) fn to_hash(&self) -> String {
        match self {
            // The testing parameters a page was opened with go on to every
            // play and back ([`keep_testing`]).
            // `auto` as the autoplay switch has it ([`testing`]).
            Route::Home => match testing() {
                t if t.is_empty() => "#/".into(),
                t => format!("#/?{t}"),
            },
            Route::Songs => match testing() {
                t if t.is_empty() => "#/select".into(),
                t => format!("#/select?{t}"),
            },
            Route::Play {
                song,
                chart,
                force_gl,
                auto,
                auto_pad,
                bias_ms,
                slow,
                replay,
                view_results,
                edit,
            } => {
                let gl = if *force_gl { "&gfx=gl" } else { "" };
                let slow = if *slow { "&slow=1" } else { "" };
                let edit = if *edit { "&edit=1" } else { "" };
                if let Some(id) = replay {
                    let view = if *view_results { "&view=results" } else { "" };
                    return format!("#/play?replay={}{view}{gl}{slow}", percent_encode(id));
                }
                let auto = match (*auto, *auto_pad) {
                    (_, true) => "&auto=pad",
                    (true, false) => "&auto=1",
                    (false, false) => "",
                };
                format!(
                    "#/play?song={}&chart={chart}{gl}{auto}{}{slow}{edit}",
                    percent_encode(song),
                    bias_param(*bias_ms)
                )
            }
            Route::Edit { song, chart } => {
                format!("#/edit?song={}&chart={chart}", percent_encode(song))
            }
            Route::Shared { id } => format!("#/s/{id}"),
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
            Route::VideoCheck { sources, decoder } => {
                let mut query = Vec::new();
                if !sources.is_empty() {
                    let list: Vec<_> = sources.iter().map(|s| percent_encode(s)).collect();
                    query.push(format!("src={}", list.join(",")));
                }
                if !decoder.is_empty() {
                    query.push(format!("decoder={}", percent_encode(decoder)));
                }
                if query.is_empty() {
                    "#/video-check".into()
                } else {
                    format!("#/video-check?{}", query.join("&"))
                }
            }
            Route::SkinPreview { params } => {
                let query: Vec<String> = params
                    .iter()
                    .map(|(k, v)| format!("{k}={}", percent_encode(v)))
                    .collect();
                format!("#/skin-preview?{}", query.join("&"))
            }
            Route::UiTest { params } => {
                let query: Vec<String> = params
                    .iter()
                    .map(|(k, v)| format!("{k}={}", percent_encode(v)))
                    .collect();
                if query.is_empty() {
                    "#/ui-test".into()
                } else {
                    format!("#/ui-test?{}", query.join("&"))
                }
            }
            Route::UiDev { screen } if screen.is_empty() => "#/ui-dev".into(),
            Route::UiDev { screen } => format!("#/ui-dev?screen={}", percent_encode(screen)),
            Route::NotFound(p) => format!("#/{p}"),
        }
    }

    pub(crate) fn current() -> Route {
        let hash = web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default();
        keep_testing(&hash);
        Route::parse(&hash)
    }

    pub(crate) fn navigate(&self) {
        navigate(&self.to_hash());
    }

    /// Return to an existing caller, or replace a direct deep link.
    pub(crate) fn return_to(&self) {
        visit(&self.to_hash(), true);
    }
}

thread_local! {
    /// The testing parameters (`gfx`, `auto`, `bias`, `slow`) the page was
    /// opened with, as written.
    static TESTING: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The testing parameters a song select or play address carries, kept
/// for the session: song select passes them to the plays it starts, and
/// the way back keeps them.
///
/// Song select's own address says whether its plays autoplay: `auto`
/// there (or its absence) replaces what was kept, and the switch follows
/// it again (the switch rewrites that address when used,
/// [`set_autoplay`]).
pub(crate) fn keep_testing(hash: &str) {
    let (path, query) = hash
        .trim_start_matches('#')
        .split_once('?')
        .unwrap_or((hash.trim_start_matches('#'), ""));
    let path = path.trim_start_matches('/');
    if !matches!(path, "" | "select" | "play" | "calibrate") {
        return;
    }
    // The chart editor's playtest says nothing about the session's
    // plays (its `auto` is the editor's choice for that test only).
    if path == "play" && query.split('&').any(|kv| kv == "edit=1") {
        return;
    }
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| matches!(kv.split('=').next(), Some("gfx" | "auto" | "bias" | "slow")))
        .collect();
    if path == "select" || path.is_empty() && !kept.is_empty() {
        TESTING.with(|t| {
            let mut t = t.borrow_mut();
            let mut parts: Vec<String> = t
                .split('&')
                .filter(|kv| !kv.is_empty() && kv.split('=').next() != Some("auto"))
                .filter(|kv| {
                    // What the address gives again replaces the old.
                    let k = kv.split('=').next();
                    !kept.iter().any(|n| n.split('=').next() == k)
                })
                .map(String::from)
                .collect();
            parts.extend(kept.iter().map(|s| s.to_string()));
            *t = parts.join("&");
        });
        return;
    }
    if !kept.is_empty() {
        TESTING.with(|t| *t.borrow_mut() = kept.join("&"));
    }
}

/// The testing parameters kept ([`keep_testing`]), `&`-joined.
pub(crate) fn testing() -> String {
    TESTING.with(|t| t.borrow().clone())
}

/// Whether plays started from song select are autoplayed (`auto=` in
/// the address).
pub(crate) fn autoplay() -> bool {
    testing()
        .split('&')
        .any(|kv| kv.starts_with("auto=") && kv != "auto=0")
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

const NAVIGATION: &str = "ddiNavigation";

/// Clone rather than replace the host's overlay markers.
pub(crate) fn history_state() -> js_sys::Object {
    let state = web_sys::window()
        .and_then(|w| w.history().ok())
        .and_then(|h| h.state().ok());
    let object = js_sys::Object::new();
    if let Some(state) = state.filter(|s| s.is_object()) {
        js_sys::Object::assign(&object, &js_sys::Object::from(state));
    }
    object
}

fn hash() -> String {
    web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "#/".into())
}

fn trail() -> ddi_ui::navigation::RouteTrail {
    let current = hash();
    let stored = js_sys::Reflect::get(&history_state(), &NAVIGATION.into())
        .ok()
        .and_then(|v| v.as_string())
        .and_then(|s| serde_json::from_str::<ddi_ui::navigation::RouteTrail>(&s).ok());
    stored
        .filter(|t| t.routes.last() == Some(&current))
        .unwrap_or_else(|| ddi_ui::navigation::RouteTrail::at(current))
}

fn put_trail(state: &js_sys::Object, trail: &ddi_ui::navigation::RouteTrail) {
    if let Ok(text) = serde_json::to_string(trail) {
        let _ = js_sys::Reflect::set(state, &NAVIGATION.into(), &text.into());
    }
}

fn changed() {
    if let Some(w) = web_sys::window()
        && let Ok(event) = web_sys::Event::new("hashchange")
    {
        let _ = w.dispatch_event(&event);
    }
}

pub(crate) fn navigate(next: &str) {
    visit(next, false);
}

fn visit(next: &str, returning: bool) {
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return;
    };
    let current = trail();
    // Already there (an address differing only in what the route does
    // not read, as testing parameters): nothing to do. Matched below as
    // the trail's own last entry, it would be `history.go(0)`, a reload.
    let here = hash();
    if next == here || Route::parse(next) == Route::parse(&here) {
        return;
    }
    // An explicit return to an ancestor (including song/results returns)
    // consumes its intervening visits instead of adding a history loop.
    if let Some(i) = current
        .routes
        .iter()
        .rposition(|s| Route::parse(s) == Route::parse(next))
    {
        let _ = history.go_with_delta(i as i32 - current.routes.len() as i32 + 1);
        return;
    }
    if returning {
        let state = js_sys::Object::new();
        put_trail(&state, &ddi_ui::navigation::RouteTrail::at(next.into()));
        if history
            .replace_state_with_url(&state, "", Some(next))
            .is_ok()
        {
            changed();
        }
        return;
    }
    let state = history_state();
    put_trail(&state, &current);
    let _ = history.replace_state_with_url(&state, "", None);
    let next_state = js_sys::Object::new();
    put_trail(&next_state, &current.push(next.into()));
    if history
        .push_state_with_url(&next_state, "", Some(next))
        .is_ok()
    {
        changed();
    }
}

pub(crate) fn back() {
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return;
    };
    let current = trail();
    if current.parent().is_some() {
        let _ = history.back();
        return;
    }
    use ddi_ui::navigation::Destination;
    let destination = match Route::current() {
        Route::Home => Destination::Start,
        Route::Songs | Route::Shared { .. } => Destination::Songs,
        Route::Play { .. } | Route::Edit { .. } => Destination::Play,
        Route::Options => Destination::Options,
        Route::Credits => Destination::Credits,
        _ => Destination::Calibration,
    };
    let parent = match destination.fallback() {
        Destination::Songs => Route::Songs,
        _ => Route::Home,
    }
    .to_hash();
    let state = js_sys::Object::new();
    put_trail(&state, &ddi_ui::navigation::RouteTrail::at(parent.clone()));
    if history
        .replace_state_with_url(&state, "", Some(&parent))
        .is_ok()
    {
        changed();
    }
}

/// A fresh launch or reload starts the title menu without focus. History
/// still restores its focus when returning from another route in this run.
pub(crate) fn reset_start_focus() {
    if Route::parse(&hash()) != Route::Home {
        return;
    }
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return;
    };
    let state = history_state();
    if js_sys::Reflect::has(&state, &"ddiFocus".into()).unwrap_or(false) {
        let _ = js_sys::Reflect::delete_property(&state, &"ddiFocus".into());
        let _ = history.replace_state_with_url(&state, "", None);
    }
}

pub(crate) fn saved_focus() -> Option<String> {
    js_sys::Reflect::get(&history_state(), &"ddiFocus".into())
        .ok()?
        .as_string()
}

pub(crate) fn saved_options() -> Option<(String, String, f32)> {
    let text = js_sys::Reflect::get(&history_state(), &"ddiOptions".into())
        .ok()?
        .as_string()?;
    serde_json::from_str(&text).ok()
}

pub(crate) fn save_view(focus: Option<&str>, theme: Option<&ddi_ui::screens::ThemeState>) {
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return;
    };
    let state = history_state();
    let mut changed = false;
    for (key, value) in [
        ("ddiFocus", focus.map(str::to_string)),
        (
            "ddiOptions",
            theme.filter(|t| t.screen == "options").and_then(|t| {
                serde_json::to_string(&(t.section.clone(), t.option_id.clone(), t.scroll)).ok()
            }),
        ),
    ] {
        if let Some(value) = value
            && js_sys::Reflect::get(&state, &key.into())
                .ok()
                .and_then(|v| v.as_string())
                .as_deref()
                != Some(&value)
        {
            let _ = js_sys::Reflect::set(&state, &key.into(), &value.into());
            changed = true;
        }
    }
    if changed {
        let _ = history.replace_state_with_url(&state, "", None);
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
        let r = Route::parse("#/play?song=into%2Dmy%2Ddream&chart=2&gfx=gl&auto=1&slow=1");
        assert_eq!(
            r,
            Route::Play {
                song: "into-my-dream".into(),
                chart: 2,
                force_gl: true,
                auto: true,
                auto_pad: false,
                bias_ms: 0,
                slow: true,
                replay: None,
                view_results: false,
                edit: false,
            }
        );
        let r = Route::parse("#/edit?song=a%20b&chart=3");
        assert_eq!(
            r,
            Route::Edit {
                song: "a b".into(),
                chart: 3
            }
        );
        assert_eq!(Route::parse(&r.to_hash()), r);
        let r = Route::parse("#/play?replay=000001760000000-0a1b2c3d&view=results");
        assert!(matches!(
            &r,
            Route::Play { replay: Some(id), view_results: true, .. } if id == "000001760000000-0a1b2c3d"
        ));
        assert_eq!(Route::parse(&r.to_hash()), r);
        assert_eq!(Route::parse(""), Route::Home);
        assert_eq!(Route::parse("#/"), Route::Home);
        assert_eq!(Route::parse("#/nope"), Route::NotFound("nope".into()));
        assert_eq!(Route::parse(&r.to_hash()), r);
    }
}
