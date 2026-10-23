use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_ui::look::{self, Look, Spec};
use ddi_ui::screens::{self, COMPACT, DESIGN, Env, sample};
use ddi_ui::testing::Harness;
use ddi_ui::{PointerKind, TextMode};

use super::tunables::{parse, parse_color};
use super::*;

fn names(a: &Actor, out: &mut Vec<String>) {
    if !a.name.is_empty() {
        out.push(a.name.clone());
    }
    for c in a.children() {
        names(c, out);
    }
}

fn has_parts(a: &Actor, parts: &[&str]) {
    let mut n = Vec::new();
    names(a, &mut n);
    for p in parts {
        assert!(n.iter().any(|x| x == p), "no {p} in {n:?}");
    }
}

fn env(theme: &Rc<Infinito>) -> Env {
    let fold: ddi_ui::dev_widgets::FoldFn =
        Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s));
    Env::new(theme.clone(), fold)
}

fn harness(size: [f32; 2], design: [f32; 2]) -> Harness {
    let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Mirror);
    h.ui.set_design(design);
    h
}

#[test]
fn the_shipped_tunables_parse() {
    let t = Infinito::new();
    assert_eq!(t.color("Accent"), parse_color("#ff6ea0"));
    assert_eq!(t.metric("Design.Width"), Some(1280.0));
    assert_eq!(t.metric("Numbers.Weight"), Some(800.0));
    assert_eq!(
        &*t.font_for("Numbers").unwrap(),
        "Iosevka Custom, Iosevka Term, monospace"
    );
    assert!(t.font().unwrap().starts_with("Iosevka Term"));
    assert_eq!(
        parse_color("#0a090db8"),
        Some([10.0 / 255.0, 9.0 / 255.0, 13.0 / 255.0, 184.0 / 255.0])
    );
    assert_eq!(parse_color("#12345"), None);
    // Unknown keys and sections are refused.
    assert!(parse(TUNABLES).is_ok());
    assert!(parse(&format!("{TUNABLES}\n[extra]\na = 1\n")).is_err());
    assert!(parse(&TUNABLES.replace("numbers_weight", "weight")).is_err());
    assert!(parse(&TUNABLES.replace("\"#ff6ea0\"", "\"pink\"")).is_err());
    // The tables are the file's.
    let t2 = parse(TUNABLES).unwrap();
    assert_eq!(
        t2.colors.len(),
        t.keys().iter().filter(|k| k.starts_with("color:")).count()
    );
}

#[test]
fn the_plain_looks_components_are_restyled_with_their_parts() {
    let t = Infinito::new();
    let s = |w, h, text: &str| Spec::new([w, h], text);
    for (name, spec, parts) in [
        ("Button", s(120.0, 44.0, "Go"), &["Plate", "Label"][..]),
        (
            "Toggle",
            s(400.0, 42.0, "Reverse"),
            &["Plate", "Label", "Box", "Mark"],
        ),
        (
            "OptionRow",
            s(400.0, 42.0, "Turn"),
            &["Plate", "Label", "Choices"],
        ),
        (
            "Slider",
            s(400.0, 42.0, "Volume"),
            &["Plate", "Label", "Track", "Fill", "Knob", "Value"],
        ),
        (
            "List.Row",
            s(400.0, 46.0, ""),
            &["Plate", "Banner", "Title", "Subtitle"],
        ),
        ("List.Header", s(400.0, 30.0, ""), &["Plate", "Title"]),
        ("List.Bar", s(4.0, 400.0, ""), &["Track", "Thumb"]),
        ("TextField", s(560.0, 42.0, "Search"), &["Box", "Hint"]),
        (
            "Popup.Item",
            s(560.0, 36.0, "Sweeteners"),
            &["Plate", "Text", "Note"],
        ),
        (
            "Panel",
            s(384.0, 230.0, "Timing"),
            &["Plate", "Title", "Note"],
        ),
        (
            "SongList.Row",
            s(596.0, 64.0, ""),
            &[
                "Plate", "Banner", "Title", "Subtitle", "Right", "Cell0", "Cell4", "Meter0",
                "Meter4", "NoCharts", "Pulse",
            ],
        ),
        (
            "SongList.Header",
            s(596.0, 64.0, ""),
            &["Plate", "Chevron", "Cover", "Title", "Count"],
        ),
        (
            "SongInfo",
            s(560.0, 280.0, ""),
            &[
                "Banner",
                "BannerShort",
                "Jacket",
                "JacketBack",
                "CdTitle",
                "Title",
                "Subtitle",
                "Artist",
                "Bpm",
                "Length",
                "Pack",
            ],
        ),
        (
            "SongInfo",
            s(300.0, 140.0, ""),
            &[
                "Banner",
                "BannerShort",
                "Jacket",
                "CdTitle",
                "Title",
                "Line",
            ],
        ),
        (
            "StyleTab",
            s(0.0, 32.0, "Single").with_items(["5"]),
            &["Plate", "Label", "Count", "Extent"],
        ),
        (
            "ChartLadder.Row",
            s(560.0, 44.0, "Hard"),
            &["Plate", "Bar", "Label", "Notes", "Meter", "Pulse"],
        ),
        (
            "ChartLadder.Empty",
            s(560.0, 150.0, "No"),
            &["Title", "Hint"],
        ),
        (
            "ChartButton",
            s(56.0, 44.0, "13"),
            &["Plate", "Meter", "Pulse"],
        ),
        (
            "KeyHint",
            s(0.0, 22.0, "Song").with_items(["↑↓"]),
            &["Label", "Extent", "Key0"],
        ),
        ("Pill", s(60.0, 26.0, "×2.5"), &["Plate", "Label", "Extent"]),
        ("Link", s(60.0, 32.0, "Options"), &["Label", "Extent"]),
        (
            "Screen.Header",
            s(1280.0, 64.0, "SELECT MUSIC").with_items(["Dito"]),
            &["Plate", "Title", "Subtitle"],
        ),
        ("Screen.Footer", s(1280.0, 40.0, ""), &["Plate"]),
        (
            "Results.Judgement",
            s(400.0, 28.0, "Great"),
            &["Name", "Bar", "Count"],
        ),
        (
            "Results.Score",
            s(400.0, 70.0, ""),
            &["Grade", "Score", "Ex"],
        ),
        (
            "Options.Section",
            s(208.0, 44.0, "Rules"),
            &["Plate", "Title", "Mark"],
        ),
        (
            "Options.Row",
            s(600.0, 42.0, "Speed"),
            &["Plate", "Label", "Pulse"],
        ),
        (
            "Options.Description",
            s(976.0, 60.0, ""),
            &["Title", "Note"],
        ),
        (
            "Options.Preview",
            s(344.0, 500.0, "Dito"),
            &["Plate", "Title"],
        ),
        ("PlayButton", s(300.0, 48.0, "PLAY"), &["Plate", "Label"]),
        ("IconButton", s(44.0, 44.0, "‹"), &["Plate", "Label"]),
        ("StyleButton", s(300.0, 36.0, "Single"), &["Plate", "Label"]),
    ] {
        let a = t
            .component(name, &spec)
            .unwrap_or_else(|| panic!("no {name}"));
        has_parts(&a, parts);
    }
}

#[test]
fn the_toggle_is_an_empty_box_while_off() {
    let t = Infinito::new();
    let mut a = look::component(&t, "Toggle", &Spec::new([400.0, 42.0], "Reverse"));
    let bx = ddi_ui::widgets::part(&mut a, "Box").unwrap();
    // An outline, nothing filling it.
    let Kind::Sprite(s) = &bx.kind else { panic!() };
    assert_eq!(s.shape.outline, 2.0);
    let mark = ddi_ui::widgets::part(&mut a, "Mark").unwrap();
    ddi_ui::widgets::play(mark, "Off");
    mark.finish_tweening();
    assert_eq!(mark.current().diffuse[0][3], 0.0);
    // The plain look's too.
    let mut p = look::component(&look::Plain, "Toggle", &Spec::new([400.0, 42.0], "Reverse"));
    assert_eq!(
        ddi_ui::widgets::part(&mut p, "Box")
            .unwrap()
            .children()
            .len(),
        4
    );
}

#[test]
fn the_wheel_makes_room_for_the_selected_row() {
    let t = Infinito::new();
    let w = |d| t.wheel("Wheel", d).unwrap();
    // The selected row (88) centered on the line; neighbours 6 from it.
    assert_eq!(w(0.0).y, -11.0);
    assert_eq!(w(1.0).y, 17.0);
    assert_eq!(w(-1.0).y, -11.0);
    assert_eq!(w(2.0).x, 20.0);
    assert!((w(3.0).alpha - 0.67).abs() < 1e-5);
    assert_eq!(w(10.0).alpha, 0.3);
    // Continuous through a glide.
    let mut last = w(-2.0).y;
    for i in -200..=200 {
        let y = w(i as f32 / 100.0).y;
        assert!(y >= last && y - last < 0.5, "{y} after {last}");
        last = y;
    }
}

/// Runs every screen with the theme: what the shipped file holds is
/// what is read, and everything read is in it.
#[test]
fn every_tunable_is_used_and_every_one_asked_for_is_there() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    let _ = screens::design_for(&*theme, [844.0, 390.0]);
    let mut title = harness([1280.0, 720.0], DESIGN);
    title.push(sample::start_screen(&env, true));
    title.run(0.3);
    // Song select, then results (both panel sets), then options.
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(sample::select_screen(&env, None));
    h.run(0.5);
    // A greyed song, a header, the search's popup.
    for _ in 0..3 {
        h.key("Tab", "Tab");
    }
    h.key("ArrowUp", "ArrowUp");
    h.run(0.3);
    assert!(state.borrow().greyed);
    for _ in 0..4 {
        h.key("ArrowUp", "ArrowUp");
        h.run(0.3);
    }
    h.run(0.3);
    assert!(state.borrow().header.is_some());
    h.key("Slash", "/");
    h.sync("Swe", [3, 3], None);
    h.run(0.3);
    assert!(state.borrow().popup);
    for _ in 0..3 {
        if h.ui.text_field().is_some() {
            h.key("Escape", "Escape");
        }
    }
    h.tap(PointerKind::Mouse, "ClearSearch");
    h.run(0.3);
    assert_eq!(state.borrow().search, "");
    // The next lit style (Sweeteners has no pump charts), then play.
    h.key("Tab", "Tab");
    h.run(0.3);
    assert!(!state.borrow().greyed);
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(state.borrow().screen, "results");
    h.key("ArrowRight", "ArrowRight");
    h.run(0.2);
    h.key("Escape", "Escape");
    h.run(0.5);
    h.tap(PointerKind::Mouse, "Link.Options");
    h.run(0.5);
    assert_eq!(state.borrow().screen, "options");
    // The start prompt.
    let mut p = harness([1280.0, 720.0], DESIGN);
    p.push(sample::prompt_screen(
        &env,
        screens::prompt::PromptStep::Ready,
    ));
    p.run(0.3);
    assert_eq!(state.borrow().screen, "prompt");
    // The compact layout.
    let mut c = harness([844.0, 390.0], COMPACT);
    c.push(sample::select_screen(&env, None));
    c.run(0.5);
    assert!(state.borrow().compact);
    let mut c = harness([390.0, 844.0], COMPACT);
    c.push(sample::select_screen(&env, None));
    c.run(0.5);
    assert!(state.borrow().compact);
    // Options on a phone, sideways and upright.
    for size in [[844.0, 390.0], [390.0, 844.0]] {
        let mut o = harness(size, COMPACT);
        o.push(sample::options_screen(&env, None));
        o.run(0.3);
        assert!(state.borrow().compact);
    }
    // The plain components the screens do not use.
    for name in [
        "Toggle",
        "Slider",
        "OptionRow",
        "OptionRow.Choice",
        "List.Row",
        "List.Header",
        "List.Bar",
    ] {
        theme.component(name, &Spec::new([400.0, 42.0], "x"));
    }
    let _ = theme.color("Background");
    // An ITG FA+ play's white Fantastics (the sample plays ITG).
    let _ = theme.color("Judgement.W1White");
    // The chart editor's audio view (the editor is not run here).
    for i in 1..=5 {
        let _ = theme.color(&format!("Editor.Audio.Ramp{i}"));
    }
    let _ = theme.color("Editor.Audio.Wave");
    let _ = theme.color("Editor.Audio.Rms");
    // Which design a window gets: short or narrow, the compact one.
    assert_eq!(screens::design_for(&*theme, [390.0, 844.0]), COMPACT);
    assert_eq!(screens::design_for(&*theme, [844.0, 390.0]), COMPACT);
    assert_eq!(screens::design_for(&*theme, [1280.0, 720.0]), DESIGN);
    let used = theme.used();
    let keys = theme.keys();
    let unused: Vec<&String> = keys.difference(&used).collect();
    assert!(unused.is_empty(), "never read: {unused:?}");
    let missing: Vec<&String> = used.difference(&keys).collect();
    assert!(
        missing.is_empty(),
        "asked for, not in the file: {missing:?}"
    );
}

#[test]
fn the_screens_in_the_theme() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(sample::select_screen(&env, None));
    h.run(0.5);
    let texts = h.texts();
    for t in [
        "SELECT MUSIC",
        "Esc / Backspace: start menu",
        "Some Things Must",
        "(OutFox Edit)",
        "160",
        "2:26",
        "HARD",
        "595 notes · steps by -YOSEFU-",
        "13",
    ] {
        assert!(texts.iter().any(|x| x == t), "no {t:?} in {texts:?}");
    }
    // The meter in its difficulty's color.
    let hard = parse_color("#e8584a").unwrap();
    let meter = h.ui.actor("Ladder.Row3").unwrap();
    let mut m = meter.clone();
    let mm = ddi_ui::widgets::part(&mut m, "Meter").unwrap();
    assert_eq!(mm.current().diffuse[0], hard);
    // Greyed rows say so.
    for _ in 0..3 {
        h.key("Tab", "Tab");
    }
    h.run(0.3);
    assert!(h.texts().iter().any(|t| t == "no Pump double charts"));
    // The pulse lights the selected row's ring.
    let row = state.borrow().song.clone();
    assert!(!row.is_empty());
    h.key("Enter", "Enter");
    h.run(0.5);
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "96.77%"), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("mean −")), "{texts:?}");
    assert!(texts.iter().any(|t| t == "EX 1680 / 1785"), "{texts:?}");
}

#[test]
fn the_pulse_lights_the_selected_row() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(sample::select_screen(&env, None));
    let mut lit = 0.0f32;
    for _ in 0..40 {
        h.frame(1.0 / 60.0);
        // The selected row's pulse outline.
        if let Some(Kind::Frame(rows)) = h.ui.actor("Wheel.Items").map(|a| a.kind.clone()) {
            for r in rows {
                let mut r = r;
                if let Some(p) = ddi_ui::widgets::part(&mut r, "Pulse") {
                    lit = lit.max(p.current().diffuse[0][3]);
                }
            }
        }
    }
    assert!(lit > 0.2, "{lit}");
}

#[test]
fn only_the_selected_row_pulses() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(sample::select_screen(&env, None));
    h.run(0.5);
    // Moves and a style change (which fills the ladder again).
    h.key("ArrowUp", "ArrowUp");
    h.run(0.3);
    h.key("Tab", "Tab");
    h.run(0.3);
    h.key("ArrowDown", "ArrowDown");
    for _ in 0..40 {
        h.frame(1.0 / 60.0);
        let Some(Kind::Frame(rows)) = h.ui.actor("Wheel.Items").map(|a| a.kind.clone()) else {
            panic!("no wheel");
        };
        let lit = rows
            .into_iter()
            .filter(|r| r.visible)
            .filter(|r| {
                let mut r = r.clone();
                ddi_ui::widgets::part(&mut r, "Pulse")
                    .is_some_and(|p| p.current().diffuse[0][3] > 0.0)
            })
            .count();
        assert!(lit <= 1, "{lit} rows lit");
    }
}

#[test]
fn every_rulesets_grade_fits_before_the_score() {
    use ddi_ui::screens::results::{ResultsActions, ResultsScreen};
    let theme = Rc::new(Infinito::new());
    // The longest grades of every ruleset, and short ones.
    for grade in [
        "AAAA",
        "AAA",
        "AA+",
        "SS",
        "SA",
        "PF",
        "AP",
        "★★★★",
        "S+",
        "F",
        "E",
    ] {
        for score in ["100.00%", "1000000", "—"] {
            for (size, design) in [([1280.0, 720.0], DESIGN), ([844.0, 390.0], COMPACT)] {
                let mut data = sample::results();
                data.grade = grade.into();
                data.score = score.into();
                let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Mirror);
                h.ui.set_design(design);
                h.push(ResultsScreen::new(
                    env(&theme),
                    data,
                    ResultsActions::default(),
                ));
                h.run(0.3);
                let g = h.ui.bounds("Grade").expect("grade");
                let s = h.ui.bounds("Score").expect("score");
                let col = h.ui.bounds("ScoreColumn").expect("column");
                // Measured at half an em a character here; the bold face is
                // about 0.58 em: room for that.
                let grade_right = g[0] + (g[2] - g[0]) * 1.16;
                assert!(grade_right <= s[0], "{grade} {score} {size:?}: {g:?} {s:?}");
                assert!(
                    s[2] <= col[2] + 1.0,
                    "{grade} {score} {size:?}: {s:?} {col:?}"
                );
            }
        }
    }
}

/// The selected row's ring (and its pulse) lie within the wheel, which
/// clips at its edges, as the row glides onto the line and at rest, in
/// both layouts (the maintainer saw it cut on its left, 2026-10-15).
#[test]
fn the_selected_rows_ring_lies_within_the_wheel() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    for (size, design) in [([1280.0, 720.0], DESIGN), ([844.0, 390.0], COMPACT)] {
        let mut h = harness(size, design);
        h.push(sample::select_screen(&env, None));
        h.run(0.5);
        let mut checked = 0;
        for key in ["ArrowDown", "ArrowDown", "ArrowUp", "ArrowUp", "ArrowUp"] {
            h.key(key, key);
            // Every frame of the glide, and at rest.
            for f in 0..30 {
                h.frame(1.0 / 60.0);
                let st = state.borrow().clone();
                let [x, y, w, hh] = st.wheel;
                let Some(row) = st.selected_row else { continue };
                for part in ["Ring", "Pulse"] {
                    let Some([l, t, r, b]) = h.ui.bounds_of(&row, part) else {
                        continue;
                    };
                    assert!(
                        l >= x - 0.01 && r <= x + w + 0.01,
                        "{part} of {row} at {l}..{r}, wheel {x}..{}",
                        x + w
                    );
                    // Vertically, at rest: on the wheel's line, or
                    // scrolled into view.
                    if f == 29 {
                        assert!(t >= y - 0.01 && b <= y + hh + 0.01, "{part} {t}..{b}");
                    }
                    checked += 1;
                }
            }
        }
        assert!(checked > 100, "{checked}");
    }
}

/// The best on the ladder: its grade, score and lamp in the lamp's
/// color; and the footer's hints never run into its links.
#[test]
fn bests_and_the_footer_in_the_theme() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(sample::select_screen(&env, None));
    h.run(0.5);
    let texts = h.texts();
    for t in ["AA", "96.12%", "FC"] {
        assert!(texts.iter().any(|x| x == t), "no {t:?} in {texts:?}");
    }
    let fc = parse_color("#4ea3f0").unwrap();
    let mut row = h.ui.actor("Ladder.Row3").unwrap().clone();
    let lamp = ddi_ui::widgets::part(&mut row, "Lamp").unwrap();
    assert_eq!(lamp.current().diffuse[0], fc);
    // Hints left, links right: apart.
    let last_hint = (0..8)
        .filter_map(|i| h.ui.bounds(&format!("Hint{i}")))
        .map(|b| b[2])
        .fold(0.0f32, f32::max);
    let first_link = [
        "Link.Manage",
        "Link.Add",
        "Link.Options",
        "Link.Calibrate",
        "Link.Credits",
        "Link.Source",
    ]
    .iter()
    .filter_map(|n| h.ui.bounds(n))
    .map(|b| b[0])
    .fold(f32::MAX, f32::min);
    assert!(last_hint + 8.0 < first_link, "{last_hint} {first_link}");
}

/// The import and manage dialogs, drawn in the theme without problems.
#[test]
fn the_dialogs_in_the_theme() {
    use ddi_ui::screens::data::{ImportReportView, ImportView, PackEntry};
    use ddi_ui::screens::select_dialogs::{ImportDialog, Manage, ManageDialog};
    use std::cell::RefCell;
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    for (size, design) in [([1280.0, 720.0], DESIGN), ([844.0, 390.0], COMPACT)] {
        let mut h = harness(size, design);
        h.push(sample::select_screen(&env, None));
        h.run(0.3);
        let view = Rc::new(RefCell::new(ImportView {
            folders: true,
            report: Some(ImportReportView {
                summary: "Imported 3 songs.".into(),
                error: false,
                sections: vec![("Not imported".into(), vec!["A/B: no chart".into()])],
            }),
            ..Default::default()
        }));
        h.ui.push(ImportDialog::new(env.clone(), view, None));
        h.run(0.3);
        let texts = h.texts();
        for t in [
            "ADD SONGS",
            "Choose folder",
            "Imported 3 songs.",
            "A/B: no chart",
        ] {
            assert!(texts.iter().any(|x| x == t), "no {t:?} in {texts:?}");
        }
        for n in ["PickFolder", "PickFiles", "Option.copy", "Close"] {
            let [l, t, r, b] = h.ui.bounds(n).unwrap_or_else(|| panic!("{n}"));
            assert!(
                l >= 0.0 && t >= 0.0 && r <= design[0] && b <= design[1],
                "{n}"
            );
        }
        h.ui.push(ManageDialog::new(
            env.clone(),
            Manage::Pack {
                pack: PackEntry {
                    name: "My Pack".into(),
                    removable: true,
                    rereadable: true,
                    banner: None,
                },
                songs: 3,
            },
            None,
            false,
        ));
        h.run(0.3);
        assert!(h.texts().iter().any(|t| t == "My Pack"));
        assert!(h.ui.problems().is_empty(), "{:?}", h.ui.problems());
    }
}

/// A flick across the wheel leaves no gap on its line: the selected
/// row's room closes while rows rush past and opens as it settles.
#[test]
fn a_flick_leaves_no_gap_on_the_wheels_line() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(sample::select_screen(&env, None));
    h.run(0.5);
    let gap = theme.metric("Select.Wheel.Gap").unwrap();
    let [x, y, w, hh] = state.borrow().wheel;
    let line = y + hh / 2.0;
    let mut worst: f32 = 0.0;
    let mut frames = 0;
    for dir in [-1.0f32, 1.0] {
        let from = [x + w / 2.0, line + dir * 200.0];
        let to = [x + w / 2.0, line - dir * 200.0];
        h.drag(PointerKind::Touch, from, to, 0.08, 4);
        for _ in 0..90 {
            h.frame(1.0 / 60.0);
            frames += 1;
            let Some(Kind::Frame(rows)) = h.ui.actor("Wheel.Items").map(|a| a.kind.clone()) else {
                panic!("no wheel");
            };
            let mut plates: Vec<[f32; 4]> = rows
                .iter()
                .filter(|r| r.visible)
                .filter_map(|r| h.ui.bounds_of(&r.name, "Plate"))
                .filter(|b| b[3] > line - 150.0 && b[1] < line + 150.0)
                .collect();
            plates.sort_by(|a, b| a[1].total_cmp(&b[1]));
            for p in plates.windows(2) {
                let g = p[1][1] - p[0][3];
                if g > gap + 6.0 {
                    eprintln!(
                        "frame {frames}: gap {g} between {:?} and {:?}; selected {:?}",
                        p[0],
                        p[1],
                        state.borrow().selected_row
                    );
                }
                worst = worst.max(g);
            }
        }
    }
    assert!(frames > 100);
    assert!(
        worst <= gap + 6.0,
        "a gap of {worst} on the line (rows {gap} apart)"
    );
}

/// A song's jacket shows beside its banner, its CD title in the banner's
/// corner; a song without them shows its banner across.
#[test]
fn jackets_and_cd_titles_show_with_the_banner() {
    use ddi_ui::screens::select::SelectScreen;
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let mut d = sample::select();
    let song = d
        .songs
        .iter_mut()
        .find(|s| s.id == "some-things-must")
        .unwrap();
    song.jacket = Some("songs/some-things-must/jacket.png".into());
    song.cd_title = Some("songs/some-things-must/cd.png".into());
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(SelectScreen::new(
        env.clone(),
        d.clone(),
        Some("some-things-must"),
        Rc::new(|_, _, _: &Env, _| {}),
        Rc::new(|_, _| {}),
    ));
    h.run(0.6);
    let source = |h: &Harness, name: &str| -> Option<String> {
        let a = h.actor(name);
        let Kind::Sprite(s) = &a.kind else {
            panic!("{name}")
        };
        a.visible
            .then(|| h.ui.texture_source(s.texture))
            .flatten()
            .map(|t| t.source.to_string())
    };
    assert_eq!(
        source(&h, "Jacket").as_deref(),
        Some("songs/some-things-must/jacket.png")
    );
    assert_eq!(
        source(&h, "CdTitle").as_deref(),
        Some("songs/some-things-must/cd.png")
    );
    assert!(source(&h, "BannerShort").is_some());
    let jacket = h.ui.bounds("Jacket").unwrap();
    let short = h.ui.bounds("BannerShort").unwrap();
    // Square, at the right of the narrower banner, as tall.
    assert!(((jacket[2] - jacket[0]) - (jacket[3] - jacket[1])).abs() < 0.5);
    assert!(short[2] <= jacket[0]);
    assert!((short[3] - jacket[3]).abs() < 0.5);
    // Without: the banner across, nothing else.
    let mut h = harness([1280.0, 720.0], DESIGN);
    let mut plain = d;
    for s in &mut plain.songs {
        s.jacket = None;
        s.cd_title = None;
    }
    h.push(SelectScreen::new(
        env.clone(),
        plain,
        Some("some-things-must"),
        Rc::new(|_, _, _: &Env, _| {}),
        Rc::new(|_, _| {}),
    ));
    h.run(0.6);
    assert_eq!(source(&h, "Jacket"), None);
    assert_eq!(source(&h, "BannerShort"), None);
    assert_eq!(source(&h, "CdTitle"), None);
}

/// The shell's preview movie shows in the banner's place, cover-fitted,
/// while it names the song shown (the jacket's narrower banner with a
/// jacket); the banner again when it stops or the selection moves on.
#[test]
fn the_preview_movie_takes_the_banners_place_while_the_shell_draws_it() {
    use ddi_actor::Fit;
    use ddi_ui::screens::select::{
        FEED, Feed, PREVIEW_MOVIE, SelectCall, SelectScreen, SharedFeed,
    };
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let data = sample::select();
    let feed: SharedFeed = Rc::new(std::cell::RefCell::new(Feed::default()));
    let calls = Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut h = harness([1280.0, 720.0], DESIGN);
    let told = calls.clone();
    h.push(
        SelectScreen::new(
            env.clone(),
            data.clone(),
            Some("some-things-must"),
            Rc::new(|_, _, _: &Env, _| {}),
            Rc::new(|_, _| {}),
        )
        .with_shell(
            Rc::new(move |c: SelectCall, _: &mut ddi_ui::Cx| told.borrow_mut().push(c)),
            feed.clone(),
        ),
    );
    h.run(0.6);
    // The left side's (the wheel's rows have banners too).
    let shown = |h: &Harness, name: &str| -> Option<(String, Fit)> {
        let mut info = h.actor("Info").clone();
        let a = ddi_ui::widgets::part(&mut info, name).expect(name);
        let Kind::Sprite(s) = &a.kind else {
            panic!("{name}")
        };
        a.visible
            .then(|| h.ui.texture_source(s.texture))
            .flatten()
            .map(|t| (t.source.to_string(), s.fit))
    };
    let first = env.state.borrow().song_id.clone();
    let banner = shown(&h, "Banner").expect("the song's banner");
    assert_ne!(banner.0, PREVIEW_MOVIE);
    assert_eq!(banner.1, Fit::Stretch);
    let tell = |h: &mut Harness, song: Option<&str>| {
        feed.borrow_mut().preview_movie = Some(song.map(str::to_string));
        h.ui.broadcast(ddi_ui::Message::new(FEED));
        h.run(0.1);
    };
    // The movie of another song changes nothing here.
    tell(&mut h, Some("another"));
    assert_eq!(shown(&h, "Banner"), Some(banner.clone()));
    tell(&mut h, Some(&first));
    assert_eq!(
        shown(&h, "Banner"),
        Some((PREVIEW_MOVIE.to_string(), Fit::Cover))
    );
    // Stopped: the banner again, stretched as before.
    tell(&mut h, None);
    assert_eq!(shown(&h, "Banner"), Some(banner.clone()));
    // Shown, then the selection moves on: the next song's banner (the
    // shell is told, and stops the movie).
    tell(&mut h, Some(&first));
    calls.borrow_mut().clear();
    h.key("ArrowDown", "ArrowDown");
    h.run(0.6);
    let now = env.state.borrow().song_id.clone();
    assert_ne!(now, first);
    assert!(
        calls
            .borrow()
            .iter()
            .any(|c| matches!(c, SelectCall::Selected { song: Some(s), .. } if *s == now)),
        "{:?}",
        calls.borrow()
    );
    assert_ne!(
        shown(&h, "Banner").map(|b| b.0).as_deref(),
        Some(PREVIEW_MOVIE)
    );
    // With a jacket: the narrower banner shows it.
    let mut jacketed = data;
    for s in &mut jacketed.songs {
        s.jacket = Some(format!("songs/{}/jacket.png", s.id));
    }
    feed.borrow_mut().data = Some(jacketed);
    h.ui.broadcast(ddi_ui::Message::new(FEED));
    h.run(0.1);
    tell(&mut h, Some(&now));
    assert_eq!(
        shown(&h, "BannerShort").map(|b| b.0).as_deref(),
        Some(PREVIEW_MOVIE)
    );
}

/// Many styles: the tabs never run into the wheel; named charts by name,
/// counted on the wheel's row; packs say how many songs match.
#[test]
fn many_styles_named_charts_and_counts_in_the_theme() {
    use ddi_ui::screens::data::{ChartEntry, StyleDef};
    use ddi_ui::screens::select::SelectScreen;
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    let mut d = sample::select();
    let first_new = d.styles.len();
    for i in 0..6 {
        d.styles.push(StyleDef {
            id: format!("danoni-{i}"),
            name: format!("{} keys", 5 + i),
        });
    }
    let song = d
        .songs
        .iter_mut()
        .find(|s| s.id == "some-things-must")
        .unwrap();
    song.charts.push(ChartEntry {
        style: "dance-single".into(),
        difficulty: ddi_ui::screens::data::DIFFICULTIES[5],
        meter: 0,
        name: "Hyper Edit".into(),
        index: 40,
        ..Default::default()
    });
    for (i, st) in d.styles.clone().iter().skip(first_new).enumerate() {
        song.charts.push(ChartEntry {
            style: st.id.clone(),
            difficulty: ddi_ui::screens::data::DIFFICULTIES[3],
            meter: 5,
            index: 50 + i,
            ..Default::default()
        });
    }
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(SelectScreen::new(
        env.clone(),
        d,
        Some("some-things-must"),
        Rc::new(|_, _, _: &Env, _| {}),
        Rc::new(|_, _| {}),
    ));
    h.run(0.5);
    let wheel_left = state.borrow().wheel[0];
    for _ in 0..12 {
        for i in 0..12 {
            if let Some(b) = h.ui.bounds(&format!("Tab{i}")) {
                assert!(
                    b[2] < wheel_left - 8.0,
                    "Tab{i} reaches {} (wheel at {wheel_left})",
                    b[2]
                );
            }
        }
        h.key("Tab", "Tab");
        h.run(0.2);
    }
    for _ in 0..12 {
        if state.borrow().style == "Single" {
            break;
        }
        h.key("Tab", "Tab");
        h.run(0.2);
    }
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "HYPER EDIT"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "+1"), "{texts:?}");
    // A search: the packs' counts.
    h.key("Slash", "/");
    h.sync("some", [4, 4], None);
    h.run(0.3);
    let texts = h.texts();
    assert!(
        texts.iter().any(|t| t.ends_with(" of 4 songs")),
        "{texts:?}"
    );
}

/// The import dialog's toggles take a tap anywhere on their row; its
/// report's rows are text alone.
#[test]
fn import_toggles_and_report_rows_in_the_theme() {
    use ddi_ui::screens::data::{ImportReportView, ImportView};
    use ddi_ui::screens::select::SelectCall;
    use ddi_ui::screens::select_dialogs::ImportDialog;
    use std::cell::RefCell;
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(sample::select_screen(&env, None));
    h.run(0.3);
    let view = Rc::new(RefCell::new(ImportView {
        folders: true,
        report: Some(ImportReportView {
            summary: "Imported 3 songs.".into(),
            error: false,
            sections: vec![(
                "Imported with problems".into(),
                vec![format!("Pack/Song: {}", "a long reason ".repeat(12))],
            )],
        }),
        ..Default::default()
    }));
    let calls: Rc<RefCell<Vec<SelectCall>>> = Rc::default();
    let c = calls.clone();
    h.ui.push(ImportDialog::new(
        env.clone(),
        view,
        Some(Rc::new(move |call, _| c.borrow_mut().push(call))),
    ));
    h.run(0.3);
    // The middle of the row.
    let [l, t, r, b] = h.ui.bounds("Option.videos").unwrap();
    h.tap_at(PointerKind::Mouse, [(l + r) / 2.0, (t + b) / 2.0]);
    assert!(
        calls
            .borrow()
            .contains(&SelectCall::ImportOption("videos".into(), true)),
        "{:?}",
        calls.borrow()
    );
    // Keys go up to the buttons, down the options, through the report to
    // Close, and back up.
    for _ in 0..12 {
        h.key("ArrowUp", "ArrowUp");
        h.run(0.1);
    }
    assert_eq!(h.ui.focused(), Some("PickFolder"));
    h.key("ArrowRight", "ArrowRight");
    h.run(0.1);
    assert_eq!(h.ui.focused(), Some("PickFiles"));
    let mut seen = Vec::new();
    for _ in 0..12 {
        h.key("ArrowDown", "ArrowDown");
        h.run(0.1);
        seen.push(h.ui.focused().unwrap_or("").to_string());
    }
    assert_eq!(
        &seen[..4],
        ["Option.copy", "Option.videos", "Option.details", "Report"],
        "{seen:?}"
    );
    assert_eq!(seen.last().map(String::as_str), Some("Close"), "{seen:?}");
    for _ in 0..12 {
        h.key("ArrowUp", "ArrowUp");
        h.run(0.1);
    }
    assert!(
        h.ui.focused().is_some_and(|f| f.starts_with("Pick")),
        "{:?}",
        h.ui.focused()
    );
    // The long line wrapped over rows, none with a banner box.
    let rows: Vec<String> = h
        .texts()
        .into_iter()
        .filter(|t| t.contains("reason"))
        .collect();
    assert!(rows.len() >= 2, "{rows:?}");
    if let Some(Kind::Frame(items)) = h.ui.actor("Report.Items").map(|a| a.kind.clone()) {
        for r in items {
            let mut n = Vec::new();
            names(&r, &mut n);
            assert!(
                !n.iter().any(|x| x == "BannerBack" || x == "Banner"),
                "{n:?}"
            );
        }
    } else {
        panic!("no report");
    }
}

/// Options at every size: the rows, their values and buttons inside the
/// rows' room and the screen, every row reachable, the preview and the
/// description on the screen.
#[test]
fn options_fit_every_size() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], COMPACT),
        ([390.0, 844.0], COMPACT),
    ] {
        let mut h = harness(size, design);
        h.push(sample::options_screen(&env, None));
        h.run(0.5);
        let vp = h.ui.viewport().screen;
        let inside = |b: [f32; 4], outer: [f32; 4], what: &str| {
            assert!(
                b[0] >= outer[0] - 1.0
                    && b[1] >= outer[1] - 1.0
                    && b[2] <= outer[2] + 1.0
                    && b[3] <= outer[3] + 1.0,
                "{size:?} {what}: {b:?} not in {outer:?}"
            );
        };
        let screen = [0.0, 0.0, vp.width, vp.height];
        for name in [
            "Preview",
            "PreviewLayout",
            "Description",
            "RowsClip",
            "Sections",
        ] {
            let b =
                h.ui.bounds(name)
                    .unwrap_or_else(|| panic!("{size:?} {name}"));
            inside(b, screen, name);
        }
        let sections = state.borrow().option_rows.len();
        let selector = h.ui.bounds("PreviewLayout").unwrap();
        let preview = h.ui.bounds("Preview").unwrap();
        assert!(
            selector[3] < preview[1],
            "{size:?}: {selector:?} {preview:?}"
        );
        assert!(sections > 0);
        for _ in 0..10 {
            let rows = state.borrow().option_rows.clone();
            let clip = h.ui.bounds("RowsClip").unwrap();
            for (i, id) in rows.iter().enumerate() {
                if i > 0 {
                    h.key("ArrowDown", "ArrowDown");
                }
                h.run(0.02);
                assert_eq!(&state.borrow().option_id, id);
                let row = h.ui.bounds(&format!("Option{i}")).unwrap();
                inside(
                    row,
                    [clip[0] - 6.0, clip[1] - 6.0, clip[2] + 6.0, clip[3] + 6.0],
                    id,
                );
                // What it does, whole, inside its panel.
                let panel = h.ui.bounds("Description").unwrap();
                for k in 0..12 {
                    let Some(a) = h.ui.actor(&format!("Description.Line{k}")) else {
                        break;
                    };
                    if let Kind::Text(t) = &a.kind {
                        assert!(!t.text().ends_with('…'), "{size:?} {id}: {:?}", t.text());
                    }
                    let b = h.ui.bounds(&format!("Description.Line{k}")).unwrap();
                    inside(b, panel, &format!("{id} description line {k}"));
                }
                if let Some(v) = h.ui.bounds(&format!("Option{i}.Values")) {
                    inside(
                        [v[0], row[1], v[2], row[3]],
                        [row[0] - 6.0, row[1], row[2] + 6.0, row[3]],
                        &format!("{id} values"),
                    );
                }
            }
            h.key("Tab", "Tab");
            h.run(0.05);
        }
        // The compact tab strip: the chosen tab shows.
        let s = state.borrow().section.clone();
        let names: Vec<String> = sample::options(0)
            .sections
            .iter()
            .map(|x| x.name.clone())
            .collect();
        let i = names.iter().position(|n| *n == s).unwrap();
        let tab = h.ui.bounds(&format!("Section{i}")).unwrap();
        inside(tab, screen, "the chosen section");
        assert!(state.borrow().preview.is_some(), "{size:?}");
    }
}

/// The credits, calibration's menu and the app's notices at every size:
/// every link and button inside the column and the screen, the banner's
/// buttons on the screen.
#[test]
fn pages_and_notices_fit_every_size() {
    use ddi_ui::screens::notices::{AppNotice, AppNotices, NOTICES_CHANGED, NoticeBar};
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], COMPACT),
        ([390.0, 844.0], COMPACT),
    ] {
        for page in ["credits", "calibration"] {
            let mut h = harness(size, design);
            match page {
                "credits" => h.push(sample::credits_screen(&env, &screens::credits::sample())),
                _ => h.push(sample::calibration_screen(&env)),
            }
            h.run(0.4);
            let vp = h.ui.viewport().screen;
            let screen = [0.0, 0.0, vp.width, vp.height];
            let column = h.ui.bounds("Column").unwrap();
            let inside = |b: [f32; 4], o: [f32; 4], what: &str| {
                assert!(
                    b[0] >= o[0] - 1.0 && b[2] <= o[2] + 1.0,
                    "{size:?} {page} {what}: {b:?} not in {o:?}"
                );
            };
            inside(column, screen, "column");
            let n = state.borrow().page_items.len();
            assert!(n > 0);
            for i in 0..n {
                // Each one scrolled into view in turn.
                h.key("ArrowDown", "ArrowDown");
                h.run(0.02);
                if let Some(b) = h.ui.bounds(&format!("Item{i}")) {
                    inside(b, column, &format!("Item{i}"));
                }
            }
            assert!(state.borrow().page_height > 200.0);
        }
        // The banner, with the longest wording and every action.
        let mut h = harness(size, design);
        h.push(sample::select_screen(&env, None));
        let feed: ddi_ui::screens::notices::NoticesFeed = Rc::default();
        h.ui.push_chrome(NoticeBar::new(
            env.clone(),
            feed.clone(),
            Rc::new(|_, _| {}),
        ));
        *feed.borrow_mut() = AppNotices {
            banner: Some(AppNotice {
                text: "New audio path: 44 kHz · ~21 ms output latency (wired/built-in?). New \
                       display: 1280×720 (1×), ~60 Hz. No calibration for it yet."
                    .into(),
                actions: vec![
                    ("calibrate".into(), "calibrate".into(), true),
                    ("defaults".into(), "use defaults".into(), false),
                    (
                        "copy-audio:a".into(),
                        "use +12 ms from Headphones".into(),
                        false,
                    ),
                    ("later".into(), "later".into(), false),
                ],
            }),
            toast: Some("devices: Headphones (+12 ms) · 1280×720 (+0 ms)".into()),
        };
        h.ui.broadcast(ddi_ui::Message::new(NOTICES_CHANGED));
        h.run(0.4);
        let vp = h.ui.viewport().screen;
        for a in ["calibrate", "defaults", "copy-audio:a", "later"] {
            let b =
                h.ui.bounds(&format!("AppNotice.{a}"))
                    .unwrap_or_else(|| panic!("{size:?} {a}"));
            assert!(
                b[0] >= -1.0 && b[2] <= vp.width + 1.0 && b[3] <= vp.height * 0.5,
                "{size:?} {a}: {b:?}"
            );
        }
        assert!(h.ui.bounds("AppToast").is_some());
    }
}

#[test]
fn page_links_take_taps_upright() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let mut h = harness([390.0, 844.0], COMPACT);
    h.push(sample::credits_screen(&env, &screens::credits::sample()));
    h.run(0.4);
    let r = h.tap(PointerKind::Touch, "Item0");
    assert!(
        r.iter().any(|q| matches!(q, ddi_ui::UiRequest::OpenUrl(_))),
        "{r:?} {:?}",
        h.ui.bounds("Item0")
    );
}

#[test]
fn options_sections_take_taps() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(sample::options_screen(&env, None));
    h.run(0.5);
    h.tap(PointerKind::Mouse, "Section3");
    h.run(0.1);
    assert_eq!(
        state.borrow().section,
        "Timing",
        "{:?}",
        h.ui.bounds("Section3")
    );
}

/// With the device banner up, every screen and dialog lies below it, at
/// every size; "+" takes its tap at 844×390.
#[test]
fn screens_lie_below_the_notices() {
    use ddi_ui::screens::notices::{AppNotice, AppNotices, NOTICES_CHANGED, NoticeBar};
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    let banner = || AppNotices {
        banner: Some(AppNotice {
            text: "New audio path: 44 kHz · ~21 ms output latency (wired/built-in?). New \
                   display: 1280×720 (1×), ~60 Hz. No calibration for it yet."
                .into(),
            actions: vec![
                ("calibrate".into(), "calibrate".into(), true),
                ("defaults".into(), "use defaults".into(), false),
                ("later".into(), "later".into(), false),
            ],
        }),
        toast: None,
    };
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], COMPACT),
        ([390.0, 844.0], COMPACT),
    ] {
        for screen in ["select", "options", "credits", "results", "import"] {
            let mut h = harness(size, design);
            match screen {
                "options" => h.push(sample::options_screen(&env, None)),
                "credits" => h.push(sample::credits_screen(&env, &screens::credits::sample())),
                "results" => {
                    h.push(sample::select_screen(&env, None));
                    h.push(sample::results_screen(&env));
                }
                _ => h.push(sample::select_screen(&env, None)),
            }
            let feed: ddi_ui::screens::notices::NoticesFeed =
                Rc::new(std::cell::RefCell::new(banner()));
            h.ui.push_chrome(NoticeBar::new(
                env.clone(),
                feed.clone(),
                Rc::new(|_, _| {}),
            ));
            h.ui.broadcast(ddi_ui::Message::new(NOTICES_CHANGED));
            h.run(0.6);
            if screen == "import" {
                let add = if state.borrow().compact {
                    "AddIcon"
                } else {
                    "Link.Add"
                };
                h.tap(PointerKind::Touch, add);
                h.run(0.4);
                assert_eq!(
                    h.ui.layers().last().map(|l| l.name.clone()).as_deref(),
                    Some("import"),
                    "{size:?}: {add} opens the import dialog"
                );
            }
            let plate = h.ui.bounds("AppNotice.Plate").unwrap();
            let names: &[&str] = match screen {
                "select" if state.borrow().compact => &["Back", "AddIcon", "SearchIcon", "Wheel"],
                "select" => &["Search", "Wheel", "Info"],
                "options" => &["Sections", "RowsClip", "Preview"],
                "credits" => &["Column"],
                "results" => &["SongList"],
                _ => &["PickFolder", "Close"],
            };
            for n in names {
                let b =
                    h.ui.bounds(n)
                        .unwrap_or_else(|| panic!("{size:?} {screen}: {n}"));
                assert!(
                    b[1] >= plate[3] - 0.5,
                    "{size:?} {screen}: {n} at {b:?} under the banner {plate:?}"
                );
            }
        }
    }
}

/// The results at every size: on a phone one column (two sideways)
/// scrolling between the bar and the buttons, nothing past the screen's
/// sides, the end reachable by keys.
#[test]
fn results_fit_every_size() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], COMPACT),
        ([390.0, 844.0], COMPACT),
    ] {
        let mut h = harness(size, design);
        h.push(sample::results_screen(&env));
        h.run(1.0);
        let vp = h.ui.viewport().screen;
        let inside = |b: [f32; 4], o: [f32; 4], what: &str| {
            assert!(
                b[0] >= o[0] - 1.0
                    && b[2] <= o[2] + 1.0
                    && b[1] >= o[1] - 1.0
                    && b[3] <= o[3] + 1.0,
                "{size:?} {what}: {b:?} not in {o:?}"
            );
        };
        let screen = [0.0, 0.0, vp.width, vp.height];
        for n in ["SongList", "Retry", "OffsetDown", "OffsetUp", "PanelSet0"] {
            let b = h.ui.bounds(n).unwrap_or_else(|| panic!("{size:?} {n}"));
            inside(b, screen, n);
        }
        let compact = state.borrow().compact;
        assert_eq!(compact, design == COMPACT, "{size:?}");
        if compact {
            let clip = h.ui.bounds("ResultsClip").unwrap();
            let sides = [clip[0], f32::MIN, clip[2], f32::MAX];
            for n in ["ScoreColumn", "Judgement0", "Panel.Timing"] {
                if let Some(b) = h.ui.bounds(n) {
                    inside(b, sides, n);
                }
            }
            // The buttons stay below the column.
            assert!(h.ui.bounds("SongList").unwrap()[1] >= clip[3] - 1.0);
            // Down to the end: the last thing shows.
            for _ in 0..30 {
                h.key("ArrowDown", "ArrowDown");
                h.run(0.02);
            }
            assert!(state.borrow().scroll > 0.0, "{size:?}");
            let content = h.ui.bounds("ResultsContent").unwrap();
            assert!(
                content[3] <= clip[3] + 20.0,
                "{size:?} {content:?} {clip:?}"
            );
        }
    }
}

#[test]
fn the_prompt_fits_every_size_at_every_step() {
    use screens::prompt::PromptStep;
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let steps = [
        (PromptStep::Ready, vec!["Start", "Back"]),
        (PromptStep::Loading("preparing audio…".into()), vec!["Back"]),
        (
            PromptStep::Access {
                folder: "A pack with a rather long folder name (2026)".into(),
                refused: Some("Reading the folder was not allowed.".into()),
            },
            vec!["Allow", "Back"],
        ),
        (
            PromptStep::NewDevice {
                lines: vec![
                    "Offsets are kept per audio path and per display. These have not been calibrated yet:".into(),
                    "New audio path: 48 kHz · ~21 ms output latency (wired/built-in?)".into(),
                    "New display: 1280×720 (1×), ~60 Hz".into(),
                ],
                actions: vec![
                    (
                        "copy-audio:a+copy-display:c".into(),
                        "Use +30 ms from Headphones and +8 ms from 2560×1440 (2×)".into(),
                    ),
                    ("calibrate".into(), "Calibrate now".into()),
                    ("defaults".into(), "Play with defaults".into()),
                ],
            },
            vec![
                "Device.defaults",
                "Device.copy-audio:a+copy-display:c",
                "Device.calibrate",
            ],
        ),
        (PromptStep::Interrupted, vec!["Retry", "Back"]),
        (
            PromptStep::Card {
                heading: "+12.4 ms ≈ 0.74 frames at 60 Hz".into(),
                lines: vec![
                    ("34 hits (8 warm-up/adaptation ignored, 2 extremes trimmed) · 0 missed · residual σ 4.1 ms · 95% CI ±1.6 ms".into(), "Muted".into()),
                    ("Converged after 2 adjustments: the remaining error (+0.4 ms ± 1.6) is within noise. Saving adds +12.4 ms to the visual offset, so these hits count as on time.".into(), "Text".into()),
                    ("offsets in effect: audio +0 ms · visual +0 ms".into(), "Muted".into()),
                ],
                actions: vec![
                    ("save".into(), "Save to the visual offset".into()),
                    ("again".into(), "Again".into()),
                    ("next".into(), "Next: sound only".into()),
                    ("done".into(), "Done".into()),
                ],
            },
            vec!["Action.save", "Action.again", "Action.next", "Action.done"],
        ),
        (
            PromptStep::Error("song failed to load: no music file".into()),
            vec!["Back"],
        ),
    ];
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], COMPACT),
        ([390.0, 844.0], COMPACT),
        ([360.0, 740.0], COMPACT),
    ] {
        for (step, buttons) in &steps {
            let mut h = harness(size, design);
            h.push(sample::prompt_screen(&env, step.clone()));
            h.run(0.3);
            let vp = h.ui.viewport().screen;
            let card = h.ui.bounds("Card").unwrap();
            assert!(
                card[0] >= -1.0
                    && card[1] >= -1.0
                    && card[2] <= vp.width + 1.0
                    && card[3] <= vp.height + 1.0,
                "{size:?} {step:?}: {card:?} in {vp:?}"
            );
            for b in buttons {
                let r =
                    h.ui.bounds(b)
                        .unwrap_or_else(|| panic!("{size:?} {step:?}: no {b}"));
                assert!(
                    r[0] >= card[0] - 1.0
                        && r[2] <= card[2] + 1.0
                        && r[1] >= card[1] - 1.0
                        && r[3] <= card[3] + 1.0,
                    "{size:?} {step:?} {b}: {r:?} in {card:?}"
                );
            }
            // Text stays inside the card, and the picture is drawn.
            for n in ["SongTitle", "Status0"] {
                let r =
                    h.ui.bounds(n)
                        .unwrap_or_else(|| panic!("{size:?} {step:?}: no {n}"));
                assert!(
                    r[0] >= card[0] - 1.0 && r[2] <= card[2] + 1.0,
                    "{size:?} {step:?} {n}: {r:?} in {card:?}"
                );
            }
            assert!(h.ui.actor("Picture").is_some());
            // Readable on a phone: CSS pixels of the status line and the
            // buttons.
            let css = |r: [f32; 4]| (r[3] - r[1]) * size[0] / vp.width;
            let status = css(h.ui.bounds("Status0").unwrap());
            assert!(status >= 12.0, "{size:?} {step:?}: status {status} CSS px");
            for b in buttons {
                let px = css(h.ui.bounds(b).unwrap());
                assert!(px >= 30.0, "{size:?} {step:?} {b}: {px} CSS px");
            }
        }
    }
}

#[test]
fn song_select_fits_upright_phones() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    for size in [[390.0, 844.0], [360.0, 740.0]] {
        let mut h = harness(size, COMPACT);
        h.push(sample::select_screen(&env, None));
        h.run(0.6);
        assert!(state.borrow().compact, "{size:?}");
        let vp = h.ui.viewport().screen;
        let b = |n: &str| h.ui.bounds(n).unwrap_or_else(|| panic!("{size:?} {n}"));
        let inside = |r: [f32; 4], what: &str| {
            assert!(
                r[0] >= -1.0 && r[1] >= -1.0 && r[2] <= vp.width + 1.0 && r[3] <= vp.height + 1.0,
                "{size:?} {what}: {r:?} not in {vp:?}"
            );
        };
        let apart = |a: [f32; 4], b: [f32; 4], what: &str| {
            let overlap =
                a[0] < b[2] - 1.0 && b[0] < a[2] - 1.0 && a[1] < b[3] - 1.0 && b[1] < a[3] - 1.0;
            assert!(!overlap, "{size:?} {what}: {a:?} and {b:?} overlap");
        };
        let bar = [
            "Back",
            "PackTitle",
            "AutoIcon",
            "AddIcon",
            "SearchIcon",
            "OptionsIcon",
        ];
        for n in bar {
            inside(b(n), n);
        }
        for w in bar.windows(2) {
            apart(b(w[0]), b(w[1]), &format!("{} {}", w[0], w[1]));
        }
        let (info, style, wheel, play) = (b("Info"), b("StyleButton"), b("Wheel"), b("Play"));
        for (r, n) in [
            (info, "Info"),
            (style, "StyleButton"),
            (wheel, "Wheel"),
            (play, "Play"),
        ] {
            inside(r, n);
        }
        let charts = b("Charts");
        // Top to bottom: the song, its style, its charts, the wheel, Play.
        assert!(
            info[3] <= style[1] + 1.0,
            "{size:?} info {info:?} style {style:?}"
        );
        assert!(
            style[3] <= charts[1] + 1.0,
            "{size:?} style {style:?} charts {charts:?}"
        );
        assert!(
            charts[3] <= wheel[1] + 1.0,
            "{size:?} charts {charts:?} wheel {wheel:?}"
        );
        assert!(
            wheel[3] <= play[1] + 1.0,
            "{size:?} wheel {wheel:?} play {play:?}"
        );
        // Play within the thumb's reach, the wheel using the height.
        assert!(play[1] > vp.height * 0.85, "{size:?} play {play:?}");
        assert!(
            wheel[3] - wheel[1] > vp.height * 0.4,
            "{size:?} wheel {wheel:?}"
        );
        // Text a phone can read: the title 14 CSS pixels at least.
        let css = |r: [f32; 4]| (r[3] - r[1]) * size[0] / vp.width;
        assert!(css(b("Play")) >= 44.0, "{size:?} play {} CSS px", css(play));
        assert!(
            css(b("Chart0")) >= 36.0,
            "{size:?} chart {} CSS px",
            css(b("Chart0"))
        );
    }
}

#[test]
fn song_select_upright_by_touch_keys_and_pad() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let state = env.state.clone();
    let mut h = harness([390.0, 844.0], COMPACT);
    h.push(sample::select_screen(&env, None));
    h.run(0.6);
    // Keys: single Down moves the wheel, double Left changes the chart.
    let song = state.borrow().song.clone();
    h.key("ArrowDown", "ArrowDown");
    h.run(0.5);
    assert_ne!(state.borrow().song, song);
    let chart = state.borrow().chart.clone();
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.08);
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.2);
    assert_ne!(state.borrow().chart, chart);
    // A pad's Up goes back up the wheel.
    h.pad(ddi_ui::MenuAction::Up, true);
    h.pad(ddi_ui::MenuAction::Up, false);
    h.run(0.3);
    assert_eq!(state.borrow().song, song);
    // A tap on a chart picks it; a tap on a row selects it.
    h.tap(ddi_ui::PointerKind::Touch, "Chart0");
    h.run(0.2);
    let picked = state.borrow().chart.clone();
    h.tap(ddi_ui::PointerKind::Touch, "Chart1");
    h.run(0.2);
    assert_ne!(state.borrow().chart, picked);
    // A flick up the wheel moves it as far as the finger and on.
    let wheel = h.ui.bounds("Wheel").unwrap();
    let x = (wheel[0] + wheel[2]) / 2.0;
    let before = h.texts();
    h.drag(
        ddi_ui::PointerKind::Touch,
        [x, wheel[3] - 20.0],
        [x, wheel[1] + 20.0],
        0.12,
        6,
    );
    h.run(1.5);
    assert_ne!(h.texts(), before, "a flick scrolls the wheel");
    // A tap on a row selects it (a song's, or a pack's header).
    let before = (
        state.borrow().song_id.clone(),
        state.borrow().header.clone(),
    );
    let rows: Vec<String> = (0..40)
        .map(|k| format!("Wheel#{k}"))
        .filter(|n| {
            h.ui.visible_bounds(n)
                .is_some_and(|r| r[1] > wheel[1] + 40.0)
        })
        .collect();
    assert!(rows.len() > 6, "{rows:?}");
    h.run(3.0);
    let mut changed = false;
    for row in [&rows[2], &rows[5]] {
        h.tap(ddi_ui::PointerKind::Touch, row);
        h.run(0.4);
        let after = (
            state.borrow().song_id.clone(),
            state.borrow().header.clone(),
        );
        changed |= after != before;
    }
    assert!(
        changed,
        "a tap on a row selects it: {before:?}, {}",
        state.borrow().status
    );
}

#[test]
fn pack_cover_headers_recycle_without_leaking_art_or_title_insets() {
    use ddi_ui::screens::data::PackEntry;
    use ddi_ui::screens::select::{FEED, Feed, SelectScreen, SharedFeed};
    for (size, design) in [([1280.0, 720.0], DESIGN), ([390.0, 844.0], COMPACT)] {
        let theme = Rc::new(Infinito::new());
        let env = env(&theme);
        let mut data = sample::select();
        data.packs.push(PackEntry {
            name: sample::BUNDLED_PACK.into(),
            banner: Some("pack-cover.png".into()),
            ..Default::default()
        });
        let feed: SharedFeed = Rc::new(std::cell::RefCell::new(Feed::default()));
        let mut h = harness(size, design);
        h.push(
            SelectScreen::new(
                env,
                data.clone(),
                None,
                Rc::new(|_, _, _, _| {}),
                Rc::new(|_, _| {}),
            )
            .with_shell(Rc::new(|_, _| {}), feed.clone()),
        );
        h.run(0.5);
        let cover =
            h.ui.actor("Wheel.Items")
                .unwrap()
                .children()
                .iter()
                .find_map(|row| {
                    let mut row = row.clone();
                    ddi_ui::widgets::part(&mut row, "Cover")
                        .filter(|a| a.visible)
                        .cloned()
                })
                .expect("a pack cover");
        let Kind::Sprite(sprite) = cover.kind else {
            panic!("not a sprite")
        };
        assert_eq!(
            h.ui.texture_source(sprite.texture).unwrap().source.as_ref(),
            "pack-cover.png"
        );
        data.packs[0].banner = None;
        feed.borrow_mut().data = Some(data);
        h.ui.broadcast(ddi_ui::Message::new(FEED));
        h.run(0.5);
        for row in h.ui.actor("Wheel.Items").unwrap().children() {
            let mut row = row.clone();
            if let Some(cover) = ddi_ui::widgets::part(&mut row, "Cover") {
                assert!(!cover.visible);
                assert_eq!(
                    ddi_ui::widgets::part(&mut row, "Title")
                        .unwrap()
                        .current()
                        .pos[0],
                    38.0
                );
            }
        }
        assert!(h.ui.problems().is_empty(), "{:?}", h.ui.problems());
    }
}
