//! `DDI_NOTESKINS=<dir>[:<dir>…] cargo test -p ddi-noteskin --test local_skins -- --nocapture`
//! loads every dance skin under each folder (a `NoteSkins` tree, or any
//! folder holding skins; third-party skins stay local) and resolves every
//! element the engine and the fallbacks' Lua ask for, for all six buttons.
//! Each folder is its own library: its `default` and `common` skins, when
//! present, are the fallbacks instead of the stand-ins (with
//! `DDI_NOTESKINS_STANDINS=1` they are left out, as when a skin is imported
//! on its own). Prints, per skin,
//! how many elements are the skin's images, the stand-ins' (built in), Lua,
//! models, other files, blank or missing.
//!
//! With `DDI_NOTESKINS_LUA=1`, `local_skins_lua` loads each skin with its
//! Lua instead (`ddi_noteskin::lua`), builds every element the engine asks
//! for, and prints what came from Lua, the time, fuel and memory, and
//! why a skin's Lua failed. Run the test binary under a memory watchdog:
//! third-party scripts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ddi_noteskin::build::{Built, Images, Textures, build};
use ddi_noteskin::load::{ENGINE_ELEMENTS, EXPLOSION_PARTS, RECEPTOR_PARTS};
use ddi_noteskin::{Button, Dir, Folder, Resolved, Skin, Source};

/// Every image read as 128 px square: enough to build the actors.
struct AnySize;

impl Images for AnySize {
    fn size(&self, _dir: Dir, _path: &str) -> Option<(u32, u32)> {
        Some((128, 128))
    }

    fn max_size(&self) -> u32 {
        8192
    }
}

/// Skin folders: those holding a `metrics.ini` or a `NoteSkin.lua`.
fn skins(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    let mut is_skin = false;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
            is_skin |=
                n.eq_ignore_ascii_case("metrics.ini") || n.eq_ignore_ascii_case("NoteSkin.lua");
        }
    }
    if is_skin {
        out.push(dir.to_path_buf());
    }
    subdirs.sort();
    for s in subdirs {
        // Skins inside skins (`variants/`) are not separate here.
        if !is_skin {
            skins(&s, out);
        }
    }
}

fn files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            files(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn folder(path: &Path) -> Folder {
    let mut list = Vec::new();
    files(path, path, &mut list);
    list.sort();
    let texts = list
        .iter()
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            l == "metrics.ini" || l.ends_with(".redir") || l.ends_with(".lua")
        })
        .filter_map(|p| {
            let bytes = std::fs::read(path.join(p)).ok()?;
            Some((p.clone(), String::from_utf8_lossy(&bytes).into_owned()))
        })
        .collect();
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // A skin's game is its parent folder in a `NoteSkins` tree; OutFox's
    // `global` skins are tried as dance skins, and a folder anywhere else
    // is taken to hold dance skins.
    let game = match parent.as_str() {
        "common" => "common",
        "dance" | "global" => "dance",
        g if GAMES.contains(&g) => g,
        _ => "dance",
    };
    Folder {
        game: game.into(),
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        files: list,
        texts,
    }
}

fn kind(r: &Resolved) -> &'static str {
    match r {
        Resolved::Blank | Resolved::NotASprite(_) => "blank",
        Resolved::Missing(_) => "missing",
        Resolved::File { source, .. } => match source {
            Source::Image(_) => "image",
            Source::BuiltInArt(_) | Source::Native(..) => "builtin",
            Source::Lua(_) => "lua",
            Source::Model(_) => "model",
            Source::Movie(_) | Source::Other(_) => "other",
        },
    }
}

/// StepMania's and OutFox's other game folders.
const GAMES: &[&str] = &[
    "pump",
    "kb7",
    "ez2",
    "para",
    "ds3ddx",
    "beat",
    "maniax",
    "techno",
    "popn",
    "lights",
    "kickbox",
    "taiko",
    "gh",
    "gddm",
    "gdgf",
    "be-mu",
    "po-mu",
    "smx",
    "stepstage",
    "kbx",
];

const KINDS: [&str; 7] = [
    "image", "builtin", "lua", "model", "other", "blank", "missing",
];

#[test]
fn local_skins() {
    let Some(dirs) = std::env::var_os("DDI_NOTESKINS") else {
        return;
    };
    let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut skins_seen, mut loaded, mut core_images) = (0, 0, 0);
    for root in std::env::split_paths(&dirs) {
        let mut paths = Vec::new();
        skins(&root, &mut paths);
        if std::env::var_os("DDI_NOTESKINS_STANDINS").is_some() {
            paths.retain(|p| {
                let f = folder(p);
                f.game != "common" && !f.name.eq_ignore_ascii_case("default")
            });
        }
        let folders: Vec<Folder> = paths.iter().map(|p| folder(p)).collect();
        for (path, f) in paths.iter().zip(&folders) {
            if f.game != "dance" || f.name.eq_ignore_ascii_case("default") && folders.len() > 1 {
                // Fallbacks, not skins of their own here.
                continue;
            }
            skins_seen += 1;
            let skin = match Skin::load(&folders, &f.game, &f.name) {
                Ok(s) => s,
                Err(e) => {
                    println!("{}\tnot loaded: {e}", path.display());
                    continue;
                }
            };
            loaded += 1;
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            let mut core = true;
            for b in Button::ALL {
                for e in ENGINE_ELEMENTS
                    .iter()
                    .chain(EXPLOSION_PARTS)
                    .chain(RECEPTOR_PARTS)
                {
                    let r = skin.resolve(b, e);
                    let k = kind(&r);
                    *counts.entry(k).or_default() += 1;
                    *totals.entry(k).or_default() += 1;
                    if matches!(*e, "Tap Note" | "Receptor") && k != "image" {
                        core = false;
                    }
                }
            }
            core_images += usize::from(core);
            // The explosion row and the mine, with their commands.
            let mut textures = Textures::default();
            let mut lua_commands = 0;
            for e in ["Explosion", "Tap Mine"] {
                let built = build(&skin, Button::Down, e, &mut textures, &AnySize);
                lua_commands += built
                    .notes
                    .iter()
                    .filter(|n| n.contains("needs Lua"))
                    .count();
                if matches!(built.built, Built::Actor(_)) {
                    *totals.entry("actors").or_default() += 1;
                }
            }
            *totals.entry("lua commands").or_default() += lua_commands;
            let row: Vec<String> = KINDS
                .iter()
                .map(|k| format!("{k} {}", counts.get(k).copied().unwrap_or(0)))
                .collect();
            let chain: Vec<String> = skin
                .chain
                .iter()
                .map(|d| match skin.folder(*d) {
                    Some(f) => f.name.clone(),
                    None => format!("{d:?}"),
                })
                .collect();
            println!(
                "{}\t{}\tchain {}\t{}{}",
                path.display(),
                row.join(", "),
                chain.join(" > "),
                if core {
                    "taps and receptor are images"
                } else {
                    ""
                },
                if skin.warnings().is_empty() {
                    String::new()
                } else {
                    format!("\t{} warnings", skin.warnings().len())
                }
            );
        }
    }
    let row: Vec<String> = KINDS
        .iter()
        .map(|k| format!("{k} {}", totals.get(k).copied().unwrap_or(0)))
        .collect();
    println!(
        "{loaded} of {skins_seen} skins loaded; {core_images} with image taps and receptors; elements: {}; explosion rows and mines built as actors: {}, metrics commands needing Lua: {}",
        row.join(", "),
        totals.get("actors").copied().unwrap_or(0),
        totals.get("lua commands").copied().unwrap_or(0)
    );
}

#[test]
fn local_skins_lua() {
    use ddi_noteskin::lua::{Env, LoadBudget, LuaSkin};
    use std::time::Instant;
    let (Some(dirs), Some(_)) = (
        std::env::var_os("DDI_NOTESKINS"),
        std::env::var_os("DDI_NOTESKINS_LUA"),
    ) else {
        return;
    };
    let only = std::env::var("DDI_NOTESKINS_ONLY").ok();
    let (mut seen, mut ok, mut failed) = (0, 0, 0);
    let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
    let mut worst = (0.0f64, String::new());
    let mut failures: BTreeMap<String, usize> = BTreeMap::new();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    let mut play_worst: Vec<(u64, f64, String)> = Vec::new();
    for root in std::env::split_paths(&dirs) {
        let mut paths = Vec::new();
        skins(&root, &mut paths);
        let folders: Vec<Folder> = paths.iter().map(|p| folder(p)).collect();
        for (path, f) in paths.iter().zip(&folders) {
            if f.game != "dance" || f.name.eq_ignore_ascii_case("default") && folders.len() > 1 {
                continue;
            }
            if only
                .as_deref()
                .is_some_and(|o| !f.name.eq_ignore_ascii_case(o))
            {
                continue;
            }
            seen += 1;
            let Ok(skin) = Skin::load(&folders, &f.game, &f.name) else {
                continue;
            };
            let start = Instant::now();
            let clock = Box::new(move || start.elapsed().as_secs_f64());
            let result = LuaSkin::new(
                skin.into_owned(),
                Env {
                    trace_missing: std::env::var_os("DDI_NOTESKINS_WARNINGS").is_some(),
                    ..Env::default()
                },
                ddi_lua::Settings::default(),
                LoadBudget::default(),
                clock,
            );
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            let mut error = None;
            let mut lua = match result {
                Ok(l) => Some(l),
                Err(e) => {
                    error = Some(format!("chain: {e}"));
                    None
                }
            };
            let mut textures = Textures::default();
            // The trees of dance-single's lanes, for the play.
            let mut trees: Vec<(usize, &str, Box<ddi_noteskin::actor::Actor>)> = Vec::new();
            if let Some(l) = &mut lua {
                'all: for b in Button::ALL {
                    for e in ENGINE_ELEMENTS {
                        match l.element(b, e, &mut textures, &AnySize) {
                            Ok(el) => {
                                let lane = Button::ALL[..4].iter().position(|x| *x == b);
                                if let (Some(lane), Built::Actor(a)) = (lane, &el.built) {
                                    trees.push((lane, e, Box::new(a.clone())));
                                }
                                if matches!(el.built, Built::BuiltIn) {
                                    // Why, without paths and line numbers.
                                    let n = el.notes.first().cloned().unwrap_or_default();
                                    let key: String = n
                                        .rsplit(": ")
                                        .next()
                                        .unwrap_or(&n)
                                        .chars()
                                        .filter(|c| !c.is_ascii_digit())
                                        .take(60)
                                        .collect();
                                    if std::env::var_os("DDI_NOTESKINS_NOTES").is_some()
                                        && !key.contains("D model")
                                        && !key.contains("blank filename")
                                    {
                                        println!("\t\t{} {e}: {n}", b.name());
                                    }
                                    *reasons.entry(key).or_default() += 1;
                                }
                                let k = match el.built {
                                    Built::Actor(_) => "actor",
                                    Built::BuiltIn => "builtin",
                                    Built::Blank => "blank",
                                };
                                *counts.entry(k).or_default() += 1;
                                *totals.entry(k).or_default() += 1;
                            }
                            Err(err) => {
                                error = Some(format!("{} {e}: {err}", b.name()));
                                break 'all;
                            }
                        }
                    }
                }
            }
            let secs = start.elapsed().as_secs_f64();
            if secs > worst.0 {
                worst = (secs, f.name.clone());
            }
            if let (Some(l), None, Some(_)) =
                (&mut lua, &error, std::env::var_os("DDI_NOTESKINS_LUA_PLAY"))
            {
                for (_, _, a) in trees.iter_mut() {
                    l.reclaim(a);
                }
                for hz in [60.0, 144.0] {
                    let r = play(l, &mut trees, hz, 20.0);
                    println!(
                        "\tplay {hz} Hz: fuel per frame mean {:.0}, p99 {}, max {}; time per frame mean {:.3} ms, max {:.3} ms{}",
                        r.mean,
                        r.p99,
                        r.max,
                        r.mean_ms,
                        r.max_ms,
                        r.error
                            .map(|e| format!("; STOPPED: {e}"))
                            .unwrap_or_default()
                    );
                    if hz == 60.0 {
                        play_worst.push((r.max, r.max_ms, f.name.clone()));
                    }
                }
                let new = l.warnings_since(0);
                if std::env::var_os("DDI_NOTESKINS_WARNINGS").is_some() {
                    for w in new {
                        println!("\t\tplay warning: {w}");
                    }
                }
            }
            let (fuel, memory, warnings) = lua.as_ref().map_or((0, 0, 0), |l| {
                (l.fuel_used(), l.memory(), l.warnings().len())
            });
            match &error {
                Some(e) => {
                    failed += 1;
                    // Group by the message without the file and line.
                    let key: String = e.split(": ").last().unwrap_or(e).chars().take(80).collect();
                    *failures.entry(key).or_default() += 1;
                }
                None => ok += 1,
            }
            println!(
                "{}\t{}\t{:.1} ms\tfuel {}\t{:.1} MiB\tactor {} builtin {} blank {}\twarnings {}{}",
                path.display(),
                if error.is_some() { "FAILED" } else { "ok" },
                secs * 1e3,
                fuel,
                memory as f64 / f64::from(1 << 20),
                counts.get("actor").copied().unwrap_or(0),
                counts.get("builtin").copied().unwrap_or(0),
                counts.get("blank").copied().unwrap_or(0),
                warnings,
                error.map(|e| format!("\t{e}")).unwrap_or_default()
            );
            if let (Some(_), Some(l)) = (std::env::var_os("DDI_NOTESKINS_WARNINGS"), &lua) {
                for w in l.warnings() {
                    println!("\t\twarning: {w}");
                }
            }
        }
    }
    println!(
        "{ok} of {seen} skins' Lua loaded, {failed} failed; elements: actor {}, builtin {}, blank {}; slowest {:.1} ms ({})",
        totals.get("actor").copied().unwrap_or(0),
        totals.get("builtin").copied().unwrap_or(0),
        totals.get("blank").copied().unwrap_or(0),
        worst.0 * 1e3,
        worst.1
    );
    for (f, n) in &failures {
        println!("  {n}\t{f}");
    }
    if !play_worst.is_empty() {
        play_worst.sort_by_key(|w| std::cmp::Reverse(w.0));
        println!("most fuel in one frame at 60 Hz:");
        for (fuel, ms, name) in play_worst.iter().take(15) {
            println!("  {fuel}\t{ms:.3} ms\t{name}");
        }
        play_worst.sort_by(|a, b| b.1.total_cmp(&a.1));
        println!("longest frame at 60 Hz:");
        for (fuel, ms, name) in play_worst.iter().take(5) {
            println!("  {ms:.3} ms\t{fuel}\t{name}");
        }
    }
    let mut reasons: Vec<_> = reasons.into_iter().collect();
    reasons.sort_by_key(|r| std::cmp::Reverse(r.1));
    println!("built in, by reason:");
    for (r, n) in reasons.iter().take(40) {
        println!("  {n}\t{r}");
    }
}

struct PlayCost {
    mean: f64,
    p99: u64,
    max: u64,
    mean_ms: f64,
    max_ms: f64,
    error: Option<String>,
}

/// `seconds` of a play at `hz` with what the engine sends a skin
/// (`render/src/noteskin.rs`): receptors' `ReverseOff` and `On`, then a
/// note every 0.2 s on the next lane and a jump of four every 2 s
/// (`Press`, its judgement, the
/// explosion's `ColumnJudgment` message and commands, `Lift`), a hold
/// every 4 s (`HoldingOn`/`Off`), a mine every 5 s, and every tree's
/// update each frame.
fn play(
    lua: &mut ddi_noteskin::lua::LuaSkin,
    trees: &mut [(usize, &str, Box<ddi_noteskin::actor::Actor>)],
    hz: f64,
    seconds: f64,
) -> PlayCost {
    use ddi_noteskin::actor::Clocks;
    use ddi_noteskin::lua::{FrameBudget, Param, Position};
    use std::time::Instant;
    let frames = (seconds * hz) as usize;
    let dt = 1.0 / hz;
    let mut fuel = Vec::with_capacity(frames);
    let (mut total_ms, mut max_ms) = (0.0f64, 0.0f64);
    let mut error = None;
    let scores = ["W1", "W2", "W3", "W1", "W4", "W2", "Miss"];
    let mut note = 0usize;
    'frames: for i in 0..frames {
        let t = i as f64 * dt;
        let start = Instant::now();
        lua.begin_frame(
            Position {
                seconds: t,
                beat: t * 2.5,
            },
            FrameBudget::default(),
        );
        for (_, _, a) in trees.iter_mut() {
            lua.lend(a);
        }
        type Call = (usize, &'static str, String, Vec<(&'static str, Param)>);
        let mut calls: Vec<Call> = Vec::new();
        fn on(calls: &mut Vec<Call>, lane: usize, element: &'static str, name: &str) {
            calls.push((lane, element, name.to_string(), Vec::new()))
        }
        if i == 0 {
            for lane in 0..4 {
                on(&mut calls, lane, "Receptor", "ReverseOff");
                on(&mut calls, lane, "Receptor", "On");
            }
        }
        let tick = |s: f64| (t / s).floor() != ((t - dt) / s).floor() || i == 0;
        // A note every 0.2 s, a jump of all four every 2 s.
        let lanes: Vec<usize> = if tick(2.0) {
            (0..4).collect()
        } else if tick(0.2) {
            vec![note % 4]
        } else {
            Vec::new()
        };
        for lane in lanes {
            let score = scores[note % scores.len()];
            note += 1;
            on(&mut calls, lane, "Receptor", "Press");
            on(&mut calls, lane, "Receptor", score);
            calls.push((
                lane,
                "Explosion",
                "ColumnJudgmentMessage".into(),
                vec![
                    ("TapNoteScore", Param::Str(format!("TapNoteScore_{score}"))),
                    ("Column", Param::Int(lane as i64)),
                ],
            ));
            on(&mut calls, lane, "Explosion", "Judgment");
            on(&mut calls, lane, "Explosion", "Dim");
            on(&mut calls, lane, "Explosion", score);
        }
        if tick(0.2) && i > 0 {
            on(&mut calls, (note + 3) % 4, "Receptor", "Lift");
        }
        if tick(4.0) {
            on(
                &mut calls,
                0,
                "Explosion",
                if ((t / 4.0) as u64).is_multiple_of(2) {
                    "HoldingOn"
                } else {
                    "HoldingOff"
                },
            );
        }
        if tick(5.0) {
            on(&mut calls, 2, "Explosion", "HitMine");
        }
        for (lane, element, name, params) in &calls {
            for (l, e, a) in trees.iter() {
                if l == lane
                    && e == element
                    && let Err(f) = lua.play(a.id, name, params)
                {
                    error = Some(f.to_string());
                }
            }
        }
        let clocks = Clocks {
            timer: t as f32,
            music_seconds: t as f32,
            beat: (t * 2.5) as f32,
            music_seconds_no_offset: t as f32,
            beat_no_offset: (t * 2.5) as f32,
        };
        for (_, _, a) in trees.iter() {
            if error.is_some() {
                break;
            }
            if let Err(f) = lua.update(a.id, dt as f32, &clocks) {
                error = Some(f.to_string());
            }
        }
        for (_, _, a) in trees.iter_mut() {
            lua.reclaim(a);
        }
        let ms = start.elapsed().as_secs_f64() * 1e3;
        total_ms += ms;
        max_ms = max_ms.max(ms);
        fuel.push(lua.frame_fuel());
        if error.is_some() {
            break 'frames;
        }
    }
    let n = fuel.len().max(1);
    let mean = fuel.iter().sum::<u64>() as f64 / n as f64;
    let max = fuel.iter().copied().max().unwrap_or(0);
    let mut sorted = fuel.clone();
    sorted.sort_unstable();
    let p99 = sorted.get(n * 99 / 100).copied().unwrap_or(max);
    PlayCost {
        mean,
        p99,
        max,
        mean_ms: total_ms / n as f64,
        max_ms,
        error,
    }
}
