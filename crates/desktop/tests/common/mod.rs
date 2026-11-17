//! What the desktop's tests share: the bundled songs' and noteskins'
//! indexes (`target/songs/index.json`, `target/noteskins/index.json`),
//! which Trunk's builds and `just test` write, written here when a fresh
//! checkout has none; the GPU or a skip and the picture drawn into
//! ([`gpu`]); the theme's UI and a person's keys and taps ([`ui`]); the
//! play host offscreen on a fake clock ([`rig`]); the practice room
//! ([`practice`]); an online room over the fake room ([`online`]); a song
//! played offscreen ([`offscreen_play`]); and the pictures kept
//! ([`shot`]).
#![allow(dead_code)]

pub mod gpu;
pub mod offscreen_play;
pub mod online;
pub mod practice;
pub mod rig;
pub mod ui;

use std::path::Path;
use std::sync::Once;

/// The system's languages English (`DDI_SYSTEM_LANGUAGES=en-US`,
/// `ddi_desktop::shell::system_languages`) for every test of a binary
/// that has these helpers, where the run names none: the UI speaks
/// English whatever the machine's languages (a developer's or a CI
/// image's in Japanese would see Japanese in assertions and pictures). A
/// test that means a system in another language sets its own before it
/// makes a UI (`tests/system_language.rs`, which has no helpers). Set
/// before `main`, so before any thread of the run reads the environment.
#[used]
#[cfg_attr(
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    unsafe(link_section = ".init_array")
)]
#[cfg_attr(target_vendor = "apple", unsafe(link_section = "__DATA,__mod_init_func"))]
#[cfg_attr(windows, unsafe(link_section = ".CRT$XCU"))]
static ENGLISH_SYSTEM: extern "C" fn() = english_system;

extern "C" fn english_system() {
    if std::env::var_os("DDI_SYSTEM_LANGUAGES").is_none() {
        // SAFETY: run before `main`, while the process has one thread.
        unsafe { std::env::set_var("DDI_SYSTEM_LANGUAGES", "en-US") };
    }
}

/// Writes the indexes the bundled songs and noteskins are listed from,
/// when they are missing (`cargo run -p xtask -- gen-songs`,
/// `gen-noteskins`), once a test binary.
pub fn indexes() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (index, task) in [
            ("target/songs/index.json", "gen-songs"),
            ("target/noteskins/index.json", "gen-noteskins"),
            ("target/announcers/index.json", "gen-announcers"),
        ] {
            if root.join(index).is_file() {
                continue;
            }
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let ok = std::process::Command::new(cargo)
                .current_dir(&root)
                .args(["run", "-q", "-p", "xtask", "--", task])
                .status()
                .is_ok_and(|s| s.success());
            assert!(ok, "`cargo run -p xtask -- {task}` failed");
        }
    });
}

/// Where the tests keep their pictures: the folder `DDI_OFFSCREEN_PPM`
/// names, else one the older names give (each test's own once).
const PICTURES: [&str; 9] = [
    "DDI_OFFSCREEN_PPM",
    "DDI_ROOM_SHOTS",
    "DDI_AV2_SHOTS",
    "DDI_FIELDS_SHOTS",
    "DDI_POLICY_SHOTS",
    "DDI_MENU_PPM",
    "DDI_PROMPT_PPM",
    "DDI_HUD_SNAPSHOTS",
    "DDI_CONTROLLER_PROOFS",
];

/// A picture (RGBA rows of `size`) kept as `name` in the pictures'
/// folder ([`PICTURES`]) where one is named: a PNG where `name` ends in
/// `.png`, else a PPM (P6, the alpha left out).
pub fn shot(name: &str, rgba: &[u8], size: [u32; 2]) {
    let Some(dir) = PICTURES.iter().find_map(std::env::var_os) else {
        return;
    };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the pictures' folder");
    let path = dir.join(name);
    if name.ends_with(".png") {
        image::save_buffer(path, rgba, size[0], size[1], image::ColorType::Rgba8).expect("png");
        return;
    }
    let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
    ppm.extend(
        rgba.as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]]),
    );
    std::fs::write(path, ppm).expect("ppm");
}
