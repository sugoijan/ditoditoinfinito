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
            &["Plate", "Chevron", "Title", "Count"],
        ),
        (
            "SongInfo",
            s(560.0, 280.0, ""),
            &[
                "Banner", "Title", "Subtitle", "Artist", "Bpm", "Length", "Pack",
            ],
        ),
        (
            "SongInfo",
            s(300.0, 140.0, ""),
            &["Banner", "Title", "Line"],
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
    // The selected row (88) centred on the line; neighbours 6 from it.
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
    h.key("Escape", "Escape");
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
    // The compact layout.
    let mut c = harness([844.0, 390.0], COMPACT);
    c.push(sample::select_screen(&env, None));
    c.run(0.5);
    assert!(state.borrow().compact);
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
        "Dito Dito Infinito",
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
    // The meter in its difficulty's colour.
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
/// colour; and the footer's hints never run into its links.
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
