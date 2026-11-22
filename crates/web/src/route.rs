//! The hash routes' table, pure: what an address names ([`Route::parse`])
//! and each route's address ([`Route::to_hash_with`]), the parent of a
//! route opened directly ([`fallback_parent`]) and which routes' screens
//! have the shared bars ([`has_bars`]). No browser: `crate::router` reads
//! and writes the page's address and history around it, and the rest of
//! the web shell is wasm's alone. This module compiles natively too, so
//! that its tests run with `cargo test -p ddi-web` (`just test`, CI):
//! the smallest part of the web's room routes a native test can reach
//! (round M, the review's gap 8); the pages behind the routes are
//! checked in a browser (`.claude/skills/verify`).

use ddi_library::share::percent_encode;
use std::collections::BTreeMap;

/// What `#/play` plays, and how.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlayRoute {
    pub(crate) song: String,
    pub(crate) chart: usize,
    pub(crate) cpu: Option<ddi_engine::cpu::Selection>,
    /// `gfx=gl` forces the WebGL2 backend.
    pub(crate) force_gl: bool,
    /// `auto=1` plays the chart automatically (testing / demo).
    pub(crate) auto: bool,
    /// `auto=pad` autoplays through a test gamepad the headless tests
    /// install (`window.__DDI_FAKE_PAD`); implies `auto`.
    pub(crate) auto_pad: bool,
    /// `bias=<ms>` makes autoplay press late by that much (testing).
    pub(crate) bias_ms: i64,
    /// `slow=1` makes each frame 40 ms longer while a background movie
    /// is drawn, to test the automatic video setting.
    pub(crate) slow: bool,
    /// `replay=<play id>`: a kept play watched again (its song and
    /// chart are the record's; `auto` and `bias` mean nothing then).
    pub(crate) replay: Option<String>,
    /// `view=results`: the kept play's results, not its replay.
    pub(crate) view_results: bool,
    /// `edit=1`: the chart editor's playtest (`crate::editor`): the
    /// song as edited, never kept, back to the editor.
    pub(crate) edit: bool,
    /// `scene=<line folder>`: the announcer studio's scene in play of
    /// that line (`crate::studio_scene`): staged, never kept, back to
    /// the studio.
    pub(crate) scene: Option<String>,
    /// `stage=<stage id>`: a stage of the practice room (`crate::room`):
    /// played against its CPUs, never kept, its outcome reported to the
    /// room, back to the room.
    pub(crate) stage: Option<u64>,
    /// `room=<id>` beside `stage=`: the online room the stage is of
    /// (none: the practice room's), so that the address reloaded goes
    /// back to its room (review3 web-5).
    pub(crate) room: Option<String>,
}

impl PlayRoute {
    /// Chart `chart` of the song `song`.
    pub(crate) fn song(song: impl Into<String>, chart: usize) -> PlayRoute {
        PlayRoute {
            song: song.into(),
            chart,
            ..PlayRoute::default()
        }
    }

    /// The kept play `id`: its replay, or its results.
    pub(crate) fn replay(id: impl Into<String>, view_results: bool) -> PlayRoute {
        PlayRoute {
            replay: Some(id.into()),
            view_results,
            ..PlayRoute::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Home,
    Songs,
    Play(PlayRoute),
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
    /// The announcer studio (`docs/plans/announcer-studio.md`).
    Studio,
    /// The multiplayer prototype (`docs/plans/multiplayer-prototype-progress.md`),
    /// behind its hidden switch (`#/?mp=1`): its home (`#/mp`) and the
    /// practice room (`#/room`: its song select, M7b).
    Multiplayer,
    Room,
    /// An invite's page (`#/mp/join/<invite>`, M5c): the room it is for,
    /// joining it; the link carries `mp=1`, the switch.
    MpJoin {
        invite: String,
    },
    /// An online room (`#/mp/room/<id>`), joined already.
    MpRoom {
        id: String,
    },
    /// A room's audience link's page (`#/mp/audience/<link>`,
    /// `docs/plans/multiplayer-audience.md`): the invite's page offering
    /// to watch with the audience; the link carries `mp=1`.
    MpAudience {
        link: String,
    },
    /// A room watched with its audience (`#/mp/watch/<link>`): the
    /// audience's page between stages, its stages watched in the play's
    /// spectator view.
    MpWatch {
        link: String,
    },
    /// A stage of that room watched (`#/mp/watch/<link>/stage`): the
    /// play's spectator view fed by the audience's seat
    /// (`crate::online::watch_stage`), back to the audience's page after
    /// it; opened with no stage handed over (a reload), the audience's
    /// page, which hands the stage over again as it stands.
    MpWatchStage {
        link: String,
    },
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
        let (path, query) = split_hash(hash);
        let path = path.trim_matches('/');
        let params: BTreeMap<&str, String> = params(query.unwrap_or_default())
            .map(|(k, v)| (k, v.map(percent_decode).unwrap_or_default()))
            .collect();
        if let Some(id) = path.strip_prefix("s/")
            && ddi_library::share::valid_id(id)
        {
            return Route::Shared { id: id.to_string() };
        }
        if let Some(invite) = path.strip_prefix("mp/join/") {
            return Route::MpJoin {
                invite: invite.to_string(),
            };
        }
        if let Some(id) = path.strip_prefix("mp/room/") {
            return Route::MpRoom { id: id.to_string() };
        }
        if let Some(link) = path.strip_prefix("mp/audience/") {
            return Route::MpAudience {
                link: link.to_string(),
            };
        }
        if let Some(link) = path
            .strip_prefix("mp/watch/")
            .and_then(|l| l.strip_suffix("/stage"))
        {
            return Route::MpWatchStage {
                link: link.to_string(),
            };
        }
        if let Some(link) = path.strip_prefix("mp/watch/") {
            return Route::MpWatch {
                link: link.to_string(),
            };
        }
        let cpu = if path == "play" {
            if let Some(profile) = params.get("cpu") {
                if ["auto", "bias", "replay", "view", "edit", "scene", "stage"]
                    .iter()
                    .any(|key| params.contains_key(key))
                {
                    return Route::NotFound(
                        "CPU cannot be combined with autoplay, bias, replay, editor or scene parameters".into(),
                    );
                }
                match ddi_engine::cpu::Selection::parse(
                    profile,
                    params.get("strength").map_or("normal", String::as_str),
                    params.get("seed").map_or("1", String::as_str),
                )
                .and_then(|cpu| {
                    cpu.with_skills(params.get("skills").map_or("balanced", String::as_str))
                })
                .and_then(|cpu| match params.get("level") {
                    // A level of the multiplayer CPUs' absolute scale
                    // instead of a strength (`docs/plans/cpu-player.md`).
                    Some(_) if params.contains_key("strength") => {
                        Err("CPU strength and level cannot be combined".into())
                    }
                    Some(level) => cpu.with_level(level),
                    // The absolute scale by default (2026-11-14): a named
                    // strength is levels 6 to 10 of it, none level 5.
                    None => Ok(cpu.on_absolute_scale(params.contains_key("strength"))),
                }) {
                    Ok(cpu) => Some(cpu),
                    Err(error) => return Route::NotFound(error),
                }
            } else {
                if params.contains_key("strength")
                    || params.contains_key("seed")
                    || params.contains_key("skills")
                    || params.contains_key("level")
                {
                    return Route::NotFound(
                        "CPU strength, level, seed and skills require cpu=1 or a profile path"
                            .into(),
                    );
                }
                None
            }
        } else {
            None
        };
        match path {
            "" => Route::Home,
            "select" => Route::Songs,
            "play" => Route::Play(PlayRoute {
                cpu,
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
                scene: params.get("scene").filter(|s| !s.is_empty()).cloned(),
                stage: params.get("stage").and_then(|s| s.parse().ok()),
                room: params.get("room").filter(|r| !r.is_empty()).cloned(),
            }),
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
            "announcer" => Route::Studio,
            "mp" => Route::Multiplayer,
            // `room/pick` was M2's song select choosing a stage's song;
            // the room's song select is the room now (M7b).
            "room" | "room/pick" => Route::Room,
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

    /// The address of the route, `testing` the testing parameters kept
    /// for the session (`&`-joined, as written: the start menu's and song
    /// select's addresses carry them).
    pub(crate) fn to_hash_with(&self, testing: &str) -> String {
        match self {
            // The testing parameters a page was opened with go on to every
            // play and back ([`keep_testing`]).
            // `auto` as the autoplay switch has it ([`testing`]).
            Route::Home => match testing {
                "" => "#/".into(),
                t => format!("#/?{t}"),
            },
            Route::Songs => match testing {
                "" => "#/select".into(),
                t => format!("#/select?{t}"),
            },
            Route::Play(PlayRoute {
                song,
                chart,
                force_gl,
                auto,
                auto_pad,
                bias_ms,
                slow,
                replay,
                view_results,
                cpu,
                edit,
                scene,
                stage,
                room,
            }) => {
                let gl = if *force_gl { "&gfx=gl" } else { "" };
                let slow = if *slow { "&slow=1" } else { "" };
                let edit = match (scene, stage) {
                    (Some(line), _) => format!("&scene={}", percent_encode(line)),
                    (None, Some(id)) => match room {
                        Some(room) => format!("&stage={id}&room={}", percent_encode(room)),
                        None => format!("&stage={id}"),
                    },
                    (None, None) if *edit => "&edit=1".into(),
                    (None, None) => String::new(),
                };
                if let Some(id) = replay {
                    let view = if *view_results { "&view=results" } else { "" };
                    return format!("#/play?replay={}{view}{gl}{slow}", percent_encode(id));
                }
                if let Some(cpu) = cpu {
                    let skills = if cpu.skills == ddi_engine::cpu::Skills::default() {
                        String::new()
                    } else {
                        format!("&skills={}", percent_encode(&cpu.skills.to_string()))
                    };
                    let strength = match cpu.absolute {
                        Some(level) => format!("level={}", level + 1),
                        None => format!(
                            "strength={}",
                            ddi_engine::cpu::LABELS[cpu.level].to_ascii_lowercase()
                        ),
                    };
                    return format!(
                        "#/play?song={}&chart={chart}&cpu={}&{strength}&seed={}{skills}{gl}{slow}",
                        percent_encode(song),
                        percent_encode(cpu.path.as_deref().unwrap_or("1")),
                        cpu.seed
                    );
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
            Route::Studio => "#/announcer".into(),
            Route::Multiplayer => "#/mp".into(),
            Route::Room => "#/room".into(),
            Route::MpJoin { invite } => format!("#/mp/join/{}", percent_encode(invite)),
            Route::MpRoom { id } => format!("#/mp/room/{}", percent_encode(id)),
            Route::MpAudience { link } => format!("#/mp/audience/{}", percent_encode(link)),
            Route::MpWatch { link } => format!("#/mp/watch/{}", percent_encode(link)),
            Route::MpWatchStage { link } => {
                format!("#/mp/watch/{}/stage", percent_encode(link))
            }
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
}

/// An address's path and its query (`#/play?song=x`: `/play` and
/// `song=x`), `None` where it has no `?`.
pub(crate) fn split_hash(hash: &str) -> (&str, Option<&str>) {
    let hash = hash.trim_start_matches('#');
    match hash.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (hash, None),
    }
}

/// A query's parameters in order: each name and its value as written
/// (`None`: the name alone).
pub(crate) fn params(query: &str) -> impl Iterator<Item = (&str, Option<&str>)> {
    query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| match kv.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (kv, None),
        })
}

pub(crate) fn param_name(kv: &str) -> &str {
    kv.split('=').next().unwrap_or(kv)
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

/// Decodes `%XX` escapes; malformed escapes are kept verbatim. Works on bytes
/// so a `%` followed by a multi-byte character cannot hit a non-char-boundary
/// `str` slice.
pub(crate) fn percent_decode(s: &str) -> String {
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

/// The parent of a route opened directly (no trail of ours before it).
pub(crate) fn fallback_parent(current: Route) -> Route {
    use ddi_ui::navigation::Destination;
    // The multiplayer prototype's routes opened directly: each its parent.
    let multiplayer = match &current {
        Route::Room
        | Route::MpJoin { .. }
        | Route::MpRoom { .. }
        | Route::MpAudience { .. }
        | Route::MpWatch { .. } => Some(Route::Multiplayer),
        // A watched stage's: its audience's page.
        Route::MpWatchStage { link } => Some(Route::MpWatch { link: link.clone() }),
        _ => None,
    };
    // A shared song's play opened directly: its share is its list.
    let shared = match &current {
        Route::Play(PlayRoute { song, .. }) => {
            ddi_library::share::parse_song_id(song).map(|(share, _)| Route::Shared {
                id: share.to_string(),
            })
        }
        _ => None,
    };
    let destination = match current {
        Route::Home => Destination::Start,
        Route::Songs | Route::Shared { .. } => Destination::Songs,
        Route::Play(_) | Route::Edit { .. } => Destination::Play,
        // Options opens the studio: a direct launch goes back to Start
        // as from Options.
        Route::Options | Route::Studio => Destination::Options,
        Route::Credits => Destination::Credits,
        _ => Destination::Calibration,
    };
    match (multiplayer, destination.fallback(), shared) {
        (Some(parent), _, _) => parent,
        (None, Destination::Songs, Some(share)) => share,
        (None, Destination::Songs, None) => Route::Songs,
        _ => Route::Home,
    }
}

/// Whether the route's screen shows the shared header and footer, the
/// bars (`ddi_ui::Screen::shared_bars`): from one such route to another
/// they stay put (`ddi_ui::Ui::set_bars_route`). The play route's
/// screens do not (its results do, but come in from the play); the
/// editor fades as a whole.
pub(crate) fn has_bars(route: &Route) -> bool {
    matches!(
        route,
        Route::Songs
            | Route::Shared { .. }
            | Route::Options
            | Route::Calibrate
            | Route::Credits
            | Route::Studio
            | Route::Multiplayer
            | Route::Room
            | Route::MpJoin { .. }
            | Route::MpRoom { .. }
            | Route::MpAudience { .. }
            | Route::MpWatch { .. }
    )
}

/// The hosts a testing parameter's address may name: this machine.
const LOOPBACK: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

/// Whether `rest` (an address's path and what follows) is only of the
/// plain characters a test's address takes: no escapes, no spaces or
/// controls (which a browser's parser drops or reads), no backslashes.
fn plain_path(rest: &str) -> bool {
    rest.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/._~-?=&,:".contains(&b))
}

/// `scheme://host[:port]` at the start of `url` where its host is this
/// machine's (exactly, never a name that merely begins so): what follows
/// it, which must be nothing or a path.
fn on_loopback<'a>(url: &'a str, schemes: &[&str]) -> Option<&'a str> {
    let rest = schemes
        .iter()
        .find_map(|s| url.strip_prefix(s).and_then(|r| r.strip_prefix("://")))?;
    let host = LOOPBACK.iter().find(|h| rest.starts_with(**h))?;
    let mut rest = &rest[host.len()..];
    if let Some(port) = rest.strip_prefix(':') {
        let digits = port.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=5).contains(&digits) {
            return None;
        }
        rest = &port[digits..];
    }
    (rest.is_empty() || rest.starts_with('/')).then_some(rest)
}

/// TESTING ONLY: whether `mpfakechaturl`'s socket may be opened: a
/// WebSocket on this machine (`ws://127.0.0.1:<port>/…`, a local fake
/// EventSub socket), never another's, which a link could otherwise point
/// a player's live chat at (review 2026-11-22, chat-touch 3).
pub(crate) fn local_socket(url: &str) -> bool {
    on_loopback(url, &["ws", "wss"]).is_some_and(plain_path)
}

/// TESTING ONLY: whether a testing parameter's base address
/// (`mpfakechatimg`, `#/video-check`'s `src`) may be fetched from: a path
/// beside the page (relative, or under the page's own `origin`) or one on
/// this machine, never another's (review 2026-11-22, chat-touch 3).
pub(crate) fn local_base(base: &str, origin: &str) -> bool {
    if let Some(rest) = on_loopback(base, &["http", "https"]) {
        return plain_path(rest);
    }
    if let Some(rest) = base.strip_prefix(origin).filter(|_| !origin.is_empty()) {
        return rest.starts_with('/') && plain_path(rest);
    }
    // Relative: no scheme (no colon before the path), never `//host`.
    !base.starts_with("//") && !base.contains(':') && plain_path(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review 2026-11-22, chat-touch 3: the testing chat's socket and
    /// images, and the video check's files, are this machine's or the
    /// page's own alone; every way of naming another host is refused
    /// (a name that begins as a loopback one, credentials, escapes,
    /// backslashes, a scheme-relative address, controls a browser drops).
    #[test]
    fn testing_addresses_are_this_machines_or_the_pages() {
        for ok in [
            "ws://127.0.0.1:9123/eventsub",
            "ws://localhost:8164/scenario?x=1",
            "wss://[::1]:443/ws",
            "ws://127.0.0.1",
        ] {
            assert!(local_socket(ok), "{ok}");
        }
        for bad in [
            "wss://evil.example/ws",
            "wss://eventsub.wss.twitch.tv/ws",
            "ws://127.0.0.1.evil.example/",
            "ws://localhost.evil.example:80/",
            "ws://localhost@evil.example/",
            "ws://user@127.0.0.1/",
            "ws://127.0.0.1%2eevil.example/",
            "ws://localhost\\@evil.example/",
            "ws://local\thost.evil.example/",
            "ws://127.0.0.1:99999999/",
            "ws://127.0.0.1:/",
            "http://127.0.0.1:9/",
            "//127.0.0.1/",
            "",
        ] {
            assert!(!local_socket(bad), "{bad}");
        }
        let origin = "https://sugoijan.dev";
        for ok in [
            "chat-test/",
            "./global/",
            "/ditoditoinfinito/chat-test/",
            "http://localhost:8098/chat-test/",
            "http://127.0.0.1:8098/global/",
            "https://sugoijan.dev/ditoditoinfinito/chat/",
            "movies/a.mp4",
        ] {
            assert!(local_base(ok, origin), "{ok}");
        }
        for bad in [
            "https://evil.example/",
            "//evil.example/",
            "\\\\evil.example/",
            "/\\evil.example/",
            "https://sugoijan.dev.evil.example/",
            "https://sugoijan.dev@evil.example/",
            "http:evil.example/",
            "javascript:alert(1)",
            "data:image/png;base64,AAAA",
            "http://localhost.evil.example/",
            "chat%2f..%2f/",
            "chat test/",
        ] {
            assert!(!local_base(bad, origin), "{bad}");
        }
        assert!(!local_base("https://x.example/", ""), "no origin known: never an absolute address");
    }

    #[test]
    fn an_address_splits_into_its_path_and_parameters() {
        assert_eq!(
            split_hash("#/play?song=a%20b&auto"),
            ("/play", Some("song=a%20b&auto"))
        );
        assert_eq!(split_hash("#/select"), ("/select", None));
        assert_eq!(split_hash("/mp?"), ("/mp", Some("")));
        let got: Vec<_> = params("a=1&&b&c=x=y&a=2").collect();
        assert_eq!(
            got,
            [
                ("a", Some("1")),
                ("b", None),
                ("c", Some("x=y")),
                ("a", Some("2"))
            ]
        );
        assert_eq!(params("").count(), 0);
    }

    #[test]
    fn cpu_route_roundtrip_and_conflicts() {
        let route = Route::parse(
            "#/play?song=beatucada&chart=3&cpu=1&strength=easy&seed=18446744073709551615",
        );
        assert!(
            matches!(&route,Route::Play(PlayRoute{cpu:Some(c),..}) if c.level==1 && c.seed==u64::MAX)
        );
        // A named strength plays its level of the absolute scale (since
        // 2026-11-14: Easy is level 7) and its address says that level:
        // the same play again, the address the same from then on. (The
        // strength's own index is not carried: nothing plays by it on
        // the absolute scale. This test never ran before round M, the
        // crate being wasm's alone, and still expected the route back
        // field for field.)
        let address = route.to_hash_with("");
        assert_eq!(address, "#/play?song=beatucada&chart=3&cpu=1&level=7&seed=18446744073709551615");
        let again = Route::parse(&address);
        assert!(
            matches!(&again,Route::Play(PlayRoute{cpu:Some(c),..}) if c.absolute==Some(6) && c.seed==u64::MAX)
        );
        assert_eq!(again.to_hash_with(""), address);
        // A level of the multiplayer CPUs' absolute scale.
        let route = Route::parse("#/play?song=beatucada&chart=3&cpu=1&level=4&seed=9");
        assert!(
            matches!(&route,Route::Play(PlayRoute{cpu:Some(c),..}) if c.absolute==Some(3) && c.seed==9)
        );
        assert_eq!(Route::parse(&route.to_hash_with("")), route);
        for suffix in [
            "cpu=1&auto=1",
            "cpu=1&edit=1",
            "cpu=1&scene=gameplay",
            "cpu=1&bias=0",
            "cpu=1&replay=x",
            "cpu=1&seed=-1",
            "cpu=1&strength=bad",
            "strength=easy",
            "skills=jacks",
            "cpu=1&skills=1,2",
            "cpu=1&skills=0.1,1,1,1,1,1,1",
            "cpu=cpu/a.json&skills=jacks",
            "cpu=../x.json",
            "cpu=1&level=0",
            "cpu=1&level=11",
            "cpu=1&level=3&strength=easy",
            "cpu=cpu/a.json&level=3",
            "level=3",
        ] {
            assert!(matches!(
                Route::parse(&format!("#/play?song=beatucada&{suffix}")),
                Route::NotFound(_)
            ));
        }
    }

    #[test]
    fn cpu_personality_routes_round_trip() {
        for input in ["jacks", "1.1,0.9,1,1.25,0.8,1,1"] {
            let route = Route::parse(&format!("#/play?song=beatucada&cpu=1&skills={input}"));
            assert!(
                matches!(&route,Route::Play(PlayRoute{cpu:Some(c),..}) if c.skills==ddi_engine::cpu::Skills::parse(input).unwrap())
            );
            assert_eq!(Route::parse(&route.to_hash_with("")), route);
        }
    }

    #[test]
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

    #[test]
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

    #[test]
    fn parse_play_route() {
        let r = Route::parse("#/play?song=into%2Dmy%2Ddream&chart=2&gfx=gl&auto=1&slow=1");
        assert_eq!(
            r,
            Route::Play(PlayRoute {
                cpu: None,
                force_gl: true,
                auto: true,
                slow: true,
                ..PlayRoute::song("into-my-dream", 2)
            })
        );
        let r = Route::parse("#/edit?song=a%20b&chart=3");
        assert_eq!(
            r,
            Route::Edit {
                song: "a b".into(),
                chart: 3
            }
        );
        assert_eq!(Route::parse(&r.to_hash_with("")), r);
        let r = Route::parse("#/play?replay=000001760000000-0a1b2c3d&view=results");
        assert!(matches!(
            &r,
            Route::Play(PlayRoute { replay: Some(id), view_results: true, .. }) if id == "000001760000000-0a1b2c3d"
        ));
        assert_eq!(Route::parse(&r.to_hash_with("")), r);
        assert_eq!(Route::parse(""), Route::Home);
        assert_eq!(Route::parse("#/"), Route::Home);
        assert_eq!(Route::parse("#/nope"), Route::NotFound("nope".into()));
        assert_eq!(Route::parse(&r.to_hash_with("")), r);
    }

    // Round M (the review's gap 8): the multiplayer routes, which no
    // test ran (the pages behind them are checked in a browser).

    /// Every multiplayer address names its route and each route's
    /// address names it again; a stage's play carries its stage and,
    /// for an online room's, its room.
    #[test]
    fn the_multiplayer_addresses_name_their_routes_and_back() {
        let link = "Room000001.audienceAUDIENCEaudien01";
        let cases = [
            ("#/mp", Route::Multiplayer),
            ("#/room", Route::Room),
            ("#/mp/join/Room000001.INVITEinvite0001", Route::MpJoin { invite: "Room000001.INVITEinvite0001".into() }),
            ("#/mp/room/Room000001", Route::MpRoom { id: "Room000001".into() }),
            (&format!("#/mp/audience/{link}"), Route::MpAudience { link: link.into() }),
            (&format!("#/mp/watch/{link}"), Route::MpWatch { link: link.into() }),
            (&format!("#/mp/watch/{link}/stage"), Route::MpWatchStage { link: link.into() }),
        ];
        for (hash, route) in &cases {
            assert_eq!(&Route::parse(hash), route, "{hash}");
            assert_eq!(&route.to_hash_with(""), hash, "{route:?}");
            // The testing parameters and the old switch in the address
            // (links made before 2026-11-14 carry `mp=1`) change nothing.
            assert_eq!(&Route::parse(&format!("{hash}?mp=1&mpcpus=16")), route, "{hash}");
            // Only the start menu's and song select's addresses carry
            // the session's testing parameters on.
            assert_eq!(&route.to_hash_with("auto=1"), hash, "{route:?}");
        }
        // `#/room/pick` was M2's song select choosing a stage's song.
        assert_eq!(Route::parse("#/room/pick"), Route::Room);
        // A practice room's stage, and an online room's.
        let practice = Route::parse("#/play?song=beatucada&chart=4&stage=3");
        assert!(matches!(
            &practice,
            Route::Play(PlayRoute { stage: Some(3), room: None, chart: 4, .. })
        ));
        assert_eq!(practice.to_hash_with(""), "#/play?song=beatucada&chart=4&stage=3");
        let online = Route::parse("#/play?song=shared%3Aabc12345%3A0&chart=1&stage=2&room=Room000001");
        assert!(matches!(
            &online,
            Route::Play(PlayRoute { stage: Some(2), room: Some(r), song, .. })
                if r == "Room000001" && song == "shared:abc12345:0"
        ));
        assert_eq!(Route::parse(&online.to_hash_with("")), online);
        // A room without a stage means nothing; a stage that is no
        // number is no stage (the page then shows the room).
        assert!(matches!(
            Route::parse("#/play?song=a&stage=x&room=Room000001"),
            Route::Play(PlayRoute { stage: None, .. })
        ));
        // A stage's play is never a CPU demo's.
        assert!(matches!(
            Route::parse("#/play?song=a&cpu=1&stage=1"),
            Route::NotFound(_)
        ));
        // The start menu's and song select's addresses keep the testing
        // parameters.
        assert_eq!(Route::Home.to_hash_with("auto=1&gfx=gl"), "#/?auto=1&gfx=gl");
        assert_eq!(Route::Songs.to_hash_with("auto=1"), "#/select?auto=1");
        assert_eq!(Route::Songs.to_hash_with(""), "#/select");
    }

    /// Back from a multiplayer route opened directly (a link, a reload:
    /// no trail of ours before it) goes to its parent: the home for the
    /// rooms and the links' pages, the audience's page for a watched
    /// stage, the start menu for the home; a play's (a stage's too, with
    /// no room to say otherwise here) to song select.
    #[test]
    fn a_multiplayer_route_opened_directly_goes_back_to_its_parent() {
        let link = String::from("Room000001.audienceAUDIENCEaudien01");
        for route in [
            Route::Room,
            Route::MpJoin { invite: "x".into() },
            Route::MpRoom { id: "Room000001".into() },
            Route::MpAudience { link: link.clone() },
            Route::MpWatch { link: link.clone() },
        ] {
            assert_eq!(fallback_parent(route.clone()), Route::Multiplayer, "{route:?}");
        }
        assert_eq!(
            fallback_parent(Route::MpWatchStage { link: link.clone() }),
            Route::MpWatch { link }
        );
        assert_eq!(fallback_parent(Route::Multiplayer), Route::Home);
        assert_eq!(fallback_parent(Route::Songs), Route::Home);
        assert_eq!(fallback_parent(Route::Play(PlayRoute::song("beatucada", 1))), Route::Songs);
        // A shared song's play: its share is its list.
        assert_eq!(
            fallback_parent(Route::Play(PlayRoute::song("shared:abc12345:0", 0))),
            Route::Shared { id: "abc12345".into() }
        );
    }

    /// The shared bars (the header and footer that stay put from one
    /// screen to the next): the multiplayer home, the rooms, the links'
    /// pages and the audience's page have them; a play, a watched stage
    /// and the start menu do not.
    #[test]
    fn the_rooms_and_their_pages_have_the_bars_and_a_play_has_none() {
        let link = String::from("l");
        for route in [
            Route::Multiplayer,
            Route::Room,
            Route::MpJoin { invite: "x".into() },
            Route::MpRoom { id: "r".into() },
            Route::MpAudience { link: link.clone() },
            Route::MpWatch { link: link.clone() },
            Route::Songs,
            Route::Options,
        ] {
            assert!(has_bars(&route), "{route:?}");
        }
        for route in [
            Route::Home,
            Route::MpWatchStage { link },
            Route::Play(PlayRoute::song("a", 0)),
            Route::Edit { song: "a".into(), chart: 0 },
        ] {
            assert!(!has_bars(&route), "{route:?}");
        }
    }
}
