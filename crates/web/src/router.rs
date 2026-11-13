//! Hash routes. The site lives under a sub-path on GitHub Pages, so routing
//! uses the fragment: `#/` (Start), `#/select` (Songs), `#/play?song=<id>&chart=<n>`, `#/options`,
//! `#/calibrate`, `#/credits`, `#/results`, `#/edit?song=<id>&chart=<n>`,
//! `#/video-check`, `#/skin-preview`, `#/ui-test`, `#/ui-dev`; behind the
//! multiplayer prototype's switch (`#/?mp=1`), `#/mp`, `#/room` and
//! `#/room/pick?stage=<id>&n=<number>`.

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
                    None => Ok(cpu),
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
            }) => {
                let gl = if *force_gl { "&gfx=gl" } else { "" };
                let slow = if *slow { "&slow=1" } else { "" };
                let edit = match (scene, stage) {
                    (Some(line), _) => format!("&scene={}", percent_encode(line)),
                    (None, Some(id)) => format!("&stage={id}"),
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

    pub(crate) fn current() -> Route {
        let hash = web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default();
        keep_testing(&hash);
        share_switch(&hash);
        multiplayer_switch(&hash);
        lang_switch(&hash);
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

/// The names of the testing parameters kept for the session.
pub(crate) const TESTING_KEYS: [&str; 4] = ["gfx", "auto", "bias", "slow"];

thread_local! {
    /// The testing parameters ([`TESTING_KEYS`]) the page was opened
    /// with, as written.
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
    crate::perf::note_address(query);
    // The chart editor's playtest, the announcer studio's scenes and a
    // practice room's stages say nothing about the session's plays
    // (their `auto` is their own).
    if path == "play"
        && query
            .split('&')
            .any(|kv| kv == "edit=1" || kv.starts_with("scene=") || kv.starts_with("stage="))
    {
        return;
    }
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| TESTING_KEYS.iter().any(|k| *k == param_name(kv)))
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

/// `?lang=TAG` on any address: the UI's language for the session (`ja`,
/// the pseudo-languages `qps-ploc` and `qps-wide`), whatever the
/// settings say, until one is chosen in Options; `lang=` alone gives the
/// choice back to the settings. Pages built after it speak it.
fn lang_switch(hash: &str) {
    let Some((_, query)) = hash.trim_start_matches('#').split_once('?') else {
        return;
    };
    for tag in query.split('&').filter_map(|kv| kv.strip_prefix("lang=")) {
        if tag.is_empty() {
            ddi_l10n::unforce();
            crate::settings::Settings::load().choose_language();
        } else if !ddi_l10n::force(tag) {
            web_sys::console::warn_1(&format!("lang={tag}: no such language").into());
        }
    }
}

/// The hidden switch of sharing by link (`docs/plans/shared-songs.md`):
/// `#/?share=1` on the start route sets `Settings::sharing`, `share=0`
/// clears it; never a row of Options.
fn share_switch(hash: &str) {
    let Some((path, query)) = hash.trim_start_matches('#').split_once('?') else {
        return;
    };
    if !path.trim_start_matches('/').is_empty() {
        return;
    }
    let Some(on) = query.split('&').find_map(|kv| match kv.split_once('=') {
        Some(("share", "1")) => Some(true),
        Some(("share", "0")) => Some(false),
        _ => None,
    }) else {
        return;
    };
    let mut settings = crate::settings::Settings::load();
    if settings.sharing != on {
        settings.sharing = on;
        settings.save();
    }
}

/// The multiplayer prototype's hidden switch
/// (`docs/plans/multiplayer-prototype-progress.md`): `#/?mp=1` on the start
/// route (or on an invite's link, `#/mp/join/<invite>?mp=1`) sets
/// `Settings::multiplayer`, `mp=0` clears it; never a row of Options
/// (DEVELOPMENT ONLY: `mpcpus=N` beside `mp=1`, practice against N CPUs
/// beyond the cap, `crate::room::develop`, and `mpdevmines=BEAT`, a
/// section of battle shocks, M8a; a convenience, `mpattacks=0`: the next
/// practice room starts with its Attacks rule Off, which anyone sets in
/// Rules (`mpattacks=1`, On, is the default now); and a hidden
/// option, `mprules=1`, a room's Rules opening by themselves on entering,
/// `Settings::multiplayer_rules_first`; another, `mpwatchlink=1`, More's
/// Copy watch-only link, `Settings::multiplayer_watch_link`). Off,
/// the start menu's Multiplayer says "Coming soon" and the prototype's
/// routes lead to the start menu.
fn multiplayer_switch(hash: &str) {
    let Some((path, query)) = hash.trim_start_matches('#').split_once('?') else {
        return;
    };
    let path = path.trim_start_matches('/');
    // The start route, or an invite's link (which carries the switch so it
    // opens where it was never set).
    if !path.is_empty() && !path.starts_with("mp/join/") && !path.starts_with("mp/audience/") {
        return;
    }
    let Some(on) = query.split('&').find_map(|kv| match kv.split_once('=') {
        Some(("mp", "1")) => Some(true),
        Some(("mp", "0")) => Some(false),
        _ => None,
    }) else {
        return;
    };
    // DEVELOPMENT ONLY: `mpcpus=N` beside `mp=1` makes the next practice
    // room one against N CPUs, beyond the cap (M6b's miniature fields).
    if on
        && let Some(n) = query.split('&').find_map(|kv| match kv.split_once('=') {
            Some(("mpcpus", n)) => n.parse::<u16>().ok(),
            _ => None,
        })
    {
        crate::room::develop(n);
    }
    // `mpattacks=0`: a convenience for tests and screenshots, the next
    // practice room starts with its Attacks rule Off (the rule is in
    // Rules for anyone, On by default since 2026-11-12; `mpattacks=1`,
    // once the way to start with it On, is still taken). DEVELOPMENT ONLY
    // (M8a): `mpdevmines=BEAT` puts a section of battle shocks into each
    // stage's play from that beat (`ddi_ui::state::room::Development`).
    if on {
        let param = |key: &str| {
            query.split('&').find_map(|kv| match kv.split_once('=') {
                Some((k, v)) if k == key => Some(v.to_string()),
                _ => None,
            })
        };
        let attacks = param("mpattacks").map(|v| v == "1");
        let mines = param("mpdevmines").map(|v| v.parse::<u32>().ok());
        if attacks.is_some() || mines.is_some() {
            let mut d = ddi_ui::state::room::development();
            if let Some(a) = attacks {
                d.attacks = Some(a);
            }
            if let Some(m) = mines {
                d.mines_at_beat = m;
            }
            ddi_ui::state::room::set_development(d);
            crate::room::discard();
        }
    }
    // TESTING ONLY: `mpfakeseat=1` beside `mp=1`, the next audience seat
    // is one in a fake room's audience in the page (`crate::fake_seat`:
    // the headless browsers' look at a watched stage without a server),
    // `mpfakeend=S` closing that audience S seconds into its stage.
    if on
        && query
            .split('&')
            .any(|kv| kv == "mpfakeseat=1")
    {
        let end = query.split('&').find_map(|kv| match kv.split_once('=') {
            Some(("mpfakeend", s)) => s.parse::<f64>().ok(),
            _ => None,
        });
        crate::fake_seat::want(end);
    }
    // `mprules=1` beside `mp=1`: a room's Rules open by themselves on
    // entering, as M7b had it (`Settings::multiplayer_rules_first`, a
    // hidden option, off by default); `mprules=0` turns it off again.
    let rules_first = query.split('&').find_map(|kv| match kv.split_once('=') {
        Some(("mprules", "1")) => Some(true),
        Some(("mprules", "0")) => Some(false),
        _ => None,
    });
    // `mpwatchlink=1` beside `mp=1`: More offers Copy watch-only link
    // (`Settings::multiplayer_watch_link`, a hidden option, off by
    // default: the link is kept but hidden); `mpwatchlink=0` hides it.
    let watch_link = query.split('&').find_map(|kv| match kv.split_once('=') {
        Some(("mpwatchlink", "1")) => Some(true),
        Some(("mpwatchlink", "0")) => Some(false),
        _ => None,
    });
    let mut settings = crate::settings::Settings::load();
    let rules_first = rules_first.unwrap_or(settings.multiplayer_rules_first);
    let watch_link = watch_link.unwrap_or(settings.multiplayer_watch_link);
    if settings.multiplayer != on
        || settings.multiplayer_rules_first != rules_first
        || settings.multiplayer_watch_link != watch_link
    {
        settings.multiplayer = on;
        settings.multiplayer_rules_first = rules_first;
        settings.multiplayer_watch_link = watch_link;
        settings.save();
    }
}

/// A parameter of the current address's query, if there.
pub(crate) fn query_param(key: &str) -> Option<String> {
    let hash = web_sys::window()?.location().hash().ok()?;
    let (_, query) = hash.split_once('?')?;
    query.split('&').find_map(|kv| match kv.split_once('=') {
        Some((k, v)) if k == key => Some(percent_decode(v)),
        None if kv == key => Some(String::new()),
        _ => None,
    })
}

/// The testing parameters kept ([`keep_testing`]), `&`-joined.
pub(crate) fn testing() -> String {
    TESTING.with(|t| t.borrow().clone())
}

/// `hash` with the testing parameters kept among `keys` added, as they
/// were written.
pub(crate) fn with_testing(mut hash: String, keys: &[&str]) -> String {
    for kv in testing().split('&') {
        if keys.contains(&param_name(kv)) {
            hash.push('&');
            hash.push_str(kv);
        }
    }
    hash
}

fn param_name(kv: &str) -> &str {
    kv.split('=').next().unwrap_or(kv)
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

/// Pushes an entry of the address shown over its own, marked `key`, its
/// trail one longer (the way back from it counts it): a page's guard of
/// the browser's Back (a room's stage's play). Whether it was pushed.
pub(crate) fn push_guard(key: &str) -> bool {
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return false;
    };
    let state = history_state();
    put_trail(&state, &trail().push(hash()));
    let _ = js_sys::Reflect::set(&state, &key.into(), &true.into());
    history.push_state_with_url(&state, "", None).is_ok()
}

fn put_trail(state: &js_sys::Object, trail: &ddi_ui::navigation::RouteTrail) {
    if let Ok(text) = serde_json::to_string(trail) {
        let _ = js_sys::Reflect::set(state, &NAVIGATION.into(), &text.into());
    }
}

fn changed() {
    history_changed();
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
    let parent = fallback_parent(Route::current());
    let state = js_sys::Object::new();
    put_trail(&state, &ddi_ui::navigation::RouteTrail::at(parent.clone()));
    if history
        .replace_state_with_url(&state, "", Some(&parent))
        .is_ok()
    {
        changed();
    }
}

/// Where [`back`] goes from here: the trail's last route before this one,
/// else the route's own parent.
pub(crate) fn back_target() -> String {
    match trail().parent() {
        Some(parent) => parent.to_string(),
        None => fallback_parent(Route::current()),
    }
}

/// The parent of a route opened directly (no trail of ours before it).
fn fallback_parent(current: Route) -> String {
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
    .to_hash()
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
        history_changed();
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

/// What [`save_view`] last wrote, and into which history entry (the
/// [`history_changed`] count then).
type Saved = (u64, Option<String>, Option<(String, String, f32)>);

thread_local! {
    /// Bumped each time the history's entry may have changed: a visit, a
    /// return, a pop, a hash change, a dialog's marker.
    static GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static SAVED: std::cell::RefCell<Option<Saved>> = const { std::cell::RefCell::new(None) };
}

/// The history's entry may be another now: the next [`save_view`] writes.
pub(crate) fn history_changed() {
    GENERATION.with(|g| g.set(g.get() + 1));
}

/// Keeps the focus and Options' place in the history's entry, written
/// only when they changed or the entry did (called every frame).
pub(crate) fn save_view(focus: Option<&str>, theme: Option<&ddi_ui::screens::ThemeState>) {
    let options = theme
        .filter(|t| t.screen == "options")
        .map(|t| (t.section.clone(), t.option_id.clone(), t.scroll));
    let now: Saved = (
        GENERATION.with(|g| g.get()),
        focus.map(str::to_string),
        options,
    );
    if SAVED.with_borrow(|s| s.as_ref() == Some(&now)) {
        return;
    }
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        return;
    };
    let state = history_state();
    let mut changed = false;
    for (key, value) in [
        ("ddiFocus", now.1.clone()),
        (
            "ddiOptions",
            now.2.as_ref().and_then(|o| serde_json::to_string(o).ok()),
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
    SAVED.with_borrow_mut(|s| *s = Some(now));
}

// The app crate only builds for wasm32; run with
// `wasm-pack test --headless --chrome app` or a configured
// `wasm-bindgen-test-runner`. `just test` does not cover it.
#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn cpu_route_roundtrip_and_conflicts() {
        let route = Route::parse(
            "#/play?song=beatucada&chart=3&cpu=1&strength=easy&seed=18446744073709551615",
        );
        assert!(
            matches!(&route,Route::Play(PlayRoute{cpu:Some(c),..}) if c.level==1 && c.seed==u64::MAX)
        );
        assert_eq!(Route::parse(&route.to_hash()), route);
        // A level of the multiplayer CPUs' absolute scale.
        let route = Route::parse("#/play?song=beatucada&chart=3&cpu=1&level=4&seed=9");
        assert!(
            matches!(&route,Route::Play(PlayRoute{cpu:Some(c),..}) if c.absolute==Some(3) && c.seed==9)
        );
        assert_eq!(Route::parse(&route.to_hash()), route);
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

    #[wasm_bindgen_test]
    fn cpu_personality_routes_round_trip() {
        for input in ["jacks", "1.1,0.9,1,1.25,0.8,1,1"] {
            let route = Route::parse(&format!("#/play?song=beatucada&cpu=1&skills={input}"));
            assert!(
                matches!(&route,Route::Play(PlayRoute{cpu:Some(c),..}) if c.skills==ddi_engine::cpu::Skills::parse(input).unwrap())
            );
            assert_eq!(Route::parse(&route.to_hash()), route);
        }
    }

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
        assert_eq!(Route::parse(&r.to_hash()), r);
        let r = Route::parse("#/play?replay=000001760000000-0a1b2c3d&view=results");
        assert!(matches!(
            &r,
            Route::Play(PlayRoute { replay: Some(id), view_results: true, .. }) if id == "000001760000000-0a1b2c3d"
        ));
        assert_eq!(Route::parse(&r.to_hash()), r);
        assert_eq!(Route::parse(""), Route::Home);
        assert_eq!(Route::parse("#/"), Route::Home);
        assert_eq!(Route::parse("#/nope"), Route::NotFound("nope".into()));
        assert_eq!(Route::parse(&r.to_hash()), r);
    }
}
