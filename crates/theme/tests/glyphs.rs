//! Every character the screens' own strings draw is in the theme's face
//! (the Iosevka Term subset in `assets/fonts/`), so none falls to
//! another font by accident: a sign missing there is added to the
//! subset (`assets/fonts/PROVENANCE.toml` has the command) or listed in
//! [`FALLBACK`] with the reason. Song data (titles, artists) may be in
//! any script; it comes from the songs, not from these files (nor from
//! the sample data's, whose made-up packs are Japanese on purpose).

use std::collections::BTreeMap;

/// Drawn by the system's fonts on purpose.
const FALLBACK: &[(char, &str)] = &[
    // Iosevka's stars fill a monospace cell: at the text's 13–15 px they
    // read as "=" ("Dancing☆Onigiri"). The system's are full size. The
    // grades' stars are the numbers face's (`Iosevka Custom`), large.
    ('☆', "Iosevka's is too small at text sizes"),
    ('★', "Iosevka's is too small at text sizes"),
];

/// The files whose string literals are drawn: the screens, their
/// dialogs and models, the theme.
const SOURCES: &[(&str, &str)] = &[
    (
        "crates/ui/src/screens/select.rs",
        include_str!("../../ui/src/screens/select.rs"),
    ),
    (
        "crates/ui/src/screens/select_dialogs.rs",
        include_str!("../../ui/src/screens/select_dialogs.rs"),
    ),
    (
        "crates/ui/src/screens/options.rs",
        include_str!("../../ui/src/screens/options.rs"),
    ),
    (
        "crates/ui/src/screens/options_model.rs",
        include_str!("../../ui/src/screens/options_model.rs"),
    ),
    (
        "crates/ui/src/screens/options_dialogs.rs",
        include_str!("../../ui/src/screens/options_dialogs.rs"),
    ),
    (
        "crates/ui/src/screens/options_shell.rs",
        include_str!("../../ui/src/screens/options_shell.rs"),
    ),
    (
        "crates/ui/src/screens/results.rs",
        include_str!("../../ui/src/screens/results.rs"),
    ),
    (
        "crates/ui/src/screens/play.rs",
        include_str!("../../ui/src/screens/play.rs"),
    ),
    (
        "crates/ui/src/screens/page.rs",
        include_str!("../../ui/src/screens/page.rs"),
    ),
    (
        "crates/ui/src/screens/credits.rs",
        include_str!("../../ui/src/screens/credits.rs"),
    ),
    (
        "crates/ui/src/screens/calibration.rs",
        include_str!("../../ui/src/screens/calibration.rs"),
    ),
    (
        "crates/ui/src/screens/folders_offer.rs",
        include_str!("../../ui/src/screens/folders_offer.rs"),
    ),
    (
        "crates/ui/src/screens/data.rs",
        include_str!("../../ui/src/screens/data.rs"),
    ),
    (
        "crates/ui/src/screens/editor.rs",
        include_str!("../../ui/src/screens/editor.rs"),
    ),
    (
        "crates/ui/src/screens/editor_dialogs.rs",
        include_str!("../../ui/src/screens/editor_dialogs.rs"),
    ),
    (
        "crates/ui/src/state/editor/mod.rs",
        include_str!("../../ui/src/state/editor/mod.rs"),
    ),
    (
        "crates/ui/src/screens/mod.rs",
        include_str!("../../ui/src/screens/mod.rs"),
    ),
    (
        "crates/ui/src/state/settings.rs",
        include_str!("../../ui/src/state/settings.rs"),
    ),
    (
        "crates/ui/src/state/bind.rs",
        include_str!("../../ui/src/state/bind.rs"),
    ),
    ("crates/theme/src/lib.rs", include_str!("../src/lib.rs")),
    (
        "crates/theme/src/keycaps.rs",
        include_str!("../src/keycaps.rs"),
    ),
    (
        "crates/ui/src/keycaps.rs",
        include_str!("../../ui/src/keycaps.rs"),
    ),
    // The shells' and the shared session's lines the screens show (the
    // prompt's notes, the calibration's card, notices, imports' reports).
    (
        "crates/ui/src/screens/prompt.rs",
        include_str!("../../ui/src/screens/prompt.rs"),
    ),
    (
        "crates/session/src/calibration.rs",
        include_str!("../../session/src/calibration.rs"),
    ),
    (
        "crates/session/src/play.rs",
        include_str!("../../session/src/play.rs"),
    ),
    (
        "crates/session/src/stage.rs",
        include_str!("../../session/src/stage.rs"),
    ),
    (
        "crates/session/src/record.rs",
        include_str!("../../session/src/record.rs"),
    ),
    (
        "crates/session/src/backgrounds.rs",
        include_str!("../../session/src/backgrounds.rs"),
    ),
    (
        "crates/web/src/components/select_page.rs",
        include_str!("../../web/src/components/select_page.rs"),
    ),
    (
        "crates/web/src/components/options_page.rs",
        include_str!("../../web/src/components/options_page.rs"),
    ),
    (
        "crates/web/src/components/page_routes.rs",
        include_str!("../../web/src/components/page_routes.rs"),
    ),
    (
        "crates/web/src/components/game_canvas.rs",
        include_str!("../../web/src/components/game_canvas.rs"),
    ),
    (
        "crates/web/src/select_data.rs",
        include_str!("../../web/src/select_data.rs"),
    ),
    (
        "crates/web/src/songs.rs",
        include_str!("../../web/src/songs.rs"),
    ),
    (
        "crates/web/src/calibration.rs",
        include_str!("../../web/src/calibration.rs"),
    ),
    (
        "crates/web/src/play.rs",
        include_str!("../../web/src/play.rs"),
    ),
    (
        "crates/web/src/history.rs",
        include_str!("../../web/src/history.rs"),
    ),
    (
        "crates/web/src/movies.rs",
        include_str!("../../web/src/movies.rs"),
    ),
    (
        "crates/desktop/src/songs.rs",
        include_str!("../../desktop/src/songs.rs"),
    ),
    (
        "crates/desktop/src/options.rs",
        include_str!("../../desktop/src/options.rs"),
    ),
];

const FACES: &[(&str, &[u8])] = &[
    (
        "Regular",
        include_bytes!("../../../assets/fonts/IosevkaTerm-Regular-latin.ttf"),
    ),
    (
        "SemiBold",
        include_bytes!("../../../assets/fonts/IosevkaTerm-SemiBold-latin.ttf"),
    ),
    (
        "Bold",
        include_bytes!("../../../assets/fonts/IosevkaTerm-Bold-latin.ttf"),
    ),
];

/// The string literals of `src` outside comments and its tests, with
/// their line numbers (escapes kept as written: only `\"` matters).
fn literals(src: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in src.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("#[cfg(test)]") {
            break;
        }
        if t.starts_with("//") {
            continue;
        }
        let mut chars = line.chars().peekable();
        let mut inside: Option<String> = None;
        let mut prev = ' ';
        while let Some(c) = chars.next() {
            match &mut inside {
                None => {
                    // A comment after code; a char literal's quote.
                    if c == '/' && chars.peek() == Some(&'/') {
                        break;
                    }
                    if c == '"' && prev != '\'' {
                        inside = Some(String::new());
                    }
                }
                Some(s) => {
                    if c == '\\' {
                        if let Some(e) = chars.next() {
                            if e != '"' && e != '\\' && e != 'n' {
                                s.push('\\');
                            }
                            if e != 'n' {
                                s.push(e);
                            }
                        }
                    } else if c == '"' {
                        out.push((n + 1, std::mem::take(s)));
                        inside = None;
                    } else {
                        s.push(c);
                    }
                }
            }
            prev = c;
        }
        // A literal going on to the next line (`\` at the end): what is
        // on this one.
        if let Some(s) = inside {
            out.push((n + 1, s));
        }
    }
    out
}

#[test]
fn the_screens_strings_are_in_the_theme_face() {
    let mut missing: BTreeMap<char, Vec<String>> = BTreeMap::new();
    for (face, bytes) in FACES {
        let font = ttf_parser::Face::parse(bytes, 0).expect("the subset parses");
        for (file, src) in SOURCES {
            for (line, s) in literals(src) {
                for c in s.chars() {
                    if c.is_control() || FALLBACK.iter().any(|(f, _)| *f == c) {
                        continue;
                    }
                    if font.glyph_index(c).is_none() {
                        missing
                            .entry(c)
                            .or_default()
                            .push(format!("{face}: {file}:{line}"));
                    }
                }
            }
        }
    }
    let report: Vec<String> = missing
        .iter()
        .map(|(c, at)| format!("{c:?} U+{:04X} at {}", *c as u32, at[0]))
        .collect();
    assert!(
        report.is_empty(),
        "not in the theme's face:\n{}",
        report.join("\n")
    );
}

#[test]
fn the_fallback_signs_are_not_in_the_subset() {
    // Listed as drawn by the system's fonts: the subset must not catch
    // them first.
    for (face, bytes) in FACES {
        let font = ttf_parser::Face::parse(bytes, 0).expect("the subset parses");
        for (c, why) in FALLBACK {
            assert!(font.glyph_index(*c).is_none(), "{face} has {c} ({why})");
        }
    }
}

#[test]
fn literals_are_found() {
    let src = "let a = \"x☆y\"; // \"not\"\n// \"no\"\nf('\"', \"q\\\"r\");\n";
    let l = literals(src);
    assert_eq!(l, vec![(1, "x☆y".to_string()), (3, "q\"r".to_string())]);
}
