//! The window's UI on a system in Japanese and in English (the system's
//! languages faked by `DDI_SYSTEM_LANGUAGES`, `shell::system_languages`;
//! no `--lang`), with no settings stored and with settings stored before
//! the defaults of 2026-11-17 (`storage.json` in a scratch `DDI_HOME`):
//! the system's shipped language spoken, English judgment words, the
//! Infinito cursor; the old settings put back once and marked in the
//! file, a choice made after it kept.

use std::path::PathBuf;
use std::rc::Rc;

use ddi_l10n::judgments::Words;
use ddi_ui::state::settings::{CursorStyle, DEFAULTS_RESET, KEY};

fn home() -> PathBuf {
    let home = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch/tests/system-language")
        .join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    home
}

/// The settings as `storage.json` keeps them.
fn stored(home: &std::path::Path) -> serde_json::Value {
    let path = home.join("config/storage.json");
    let file: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let text = file[KEY].as_str().unwrap_or_else(|| panic!("{file}"));
    serde_json::from_str(text).unwrap()
}

/// The window's UI made as the shell makes it, on the system `languages`:
/// the language it speaks, the judgment words' and the cursor.
fn window(languages: &str) -> (&'static str, Words, CursorStyle) {
    // SAFETY: this file's one test sets them, before any thread reads them.
    unsafe { std::env::set_var("DDI_SYSTEM_LANGUAGES", languages) };
    ddi_desktop::store::install();
    let (_ui, env, _) = ddi_desktop::shell::theme_ui("start", Rc::new(str::to_string));
    ddi_platform::storage::flush().unwrap();
    (
        ddi_l10n::current().tag,
        ddi_l10n::judgments::words(),
        env.cursor.get().0,
    )
}

#[test]
fn a_japanese_system_gets_japanese_and_old_settings_are_put_back_once() {
    let home = home();
    // SAFETY: as above.
    unsafe { std::env::set_var("DDI_HOME", &home) };
    // A fresh profile on a Japanese system, then on an English one.
    assert_eq!(
        window("ja-JP, en-US"),
        ("ja", Words::English, CursorStyle::PointerTip)
    );
    assert_eq!(ddi_l10n::msg::start::singleplayer().as_str(), "シングルプレイ");
    assert_eq!(
        ddi_desktop::shell::system_languages(),
        ["ja-JP", "en-US"]
    );
    assert_eq!(
        window("en-US,ja-JP"),
        ("en", Words::English, CursorStyle::PointerTip)
    );
    // Settings stored before the defaults of 2026-11-17: Japanese chosen,
    // the words in the menus' language, the Infinito arrow; on an
    // English system they come out as a fresh profile's, marked in the
    // file, the cursor's size as it was.
    let config = home.join("config");
    std::fs::create_dir_all(&config).unwrap();
    let old = serde_json::json!({
        "language": "ja",
        "judgment_words": "Localized",
        "cursor_style": "InfinitoArrow",
        "cursor_size": 40.0,
        "rules_moved": true,
        "speed": 3.5,
    });
    std::fs::write(
        config.join("storage.json"),
        serde_json::json!({ KEY: old.to_string() }).to_string(),
    )
    .unwrap();
    assert_eq!(
        window("en-US"),
        ("en", Words::English, CursorStyle::PointerTip)
    );
    let s = stored(&home);
    assert_eq!(s[DEFAULTS_RESET], true);
    assert_eq!(s["language"], serde_json::Value::Null);
    assert_eq!(s["judgment_words"], "English");
    assert_eq!(s["cursor_style"], "PointerTip");
    assert_eq!((s["cursor_size"].as_f64(), s["speed"].as_f64()), (Some(40.0), Some(3.5)));
    // Chosen again after it: kept, on any system.
    let mut chosen = s.clone();
    chosen["language"] = "ja".into();
    chosen["judgment_words"] = "ja".into();
    chosen["cursor_style"] = "InfinitoArrow".into();
    std::fs::write(
        config.join("storage.json"),
        serde_json::json!({ KEY: chosen.to_string() }).to_string(),
    )
    .unwrap();
    let ja = ddi_l10n::find("ja").unwrap();
    assert_eq!(
        window("en-US"),
        ("ja", Words::In(ja), CursorStyle::InfinitoArrow)
    );
    assert_eq!(stored(&home), chosen);
    ddi_l10n::choose(Some("en"));
    let _ = std::fs::remove_dir_all(&home);
}
