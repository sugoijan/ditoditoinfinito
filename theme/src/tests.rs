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
    assert!(texts.iter().any(|t| t == "96.77"), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("mean −")), "{texts:?}");
    assert!(texts.iter().any(|t| t == "EX 94.10%"), "{texts:?}");
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
