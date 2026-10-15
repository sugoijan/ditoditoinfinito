//! The theme's screens with the plain look (they know nothing of
//! Infinito; `ddi-theme` runs them again in its own).

use std::rc::Rc;

use super::sample;
use super::{COMPACT, DESIGN, Env, SharedTheme};
use crate::dev_widgets::FoldFn;
use crate::look::Plain;
use crate::testing::Harness;
use crate::{PointerKind, TextMode};

fn env() -> Env {
    let fold: FoldFn =
        Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s));
    Env::new(Rc::new(Plain), fold)
}

fn select(size: [f32; 2], design: [f32; 2], reduced: bool) -> (Harness, SharedTheme) {
    let env = env();
    env.reduced_motion.set(reduced);
    let state = env.state.clone();
    let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Mirror);
    h.ui.set_design(design);
    h.push(sample::select_screen(&env, None));
    h.run(0.5);
    (h, state)
}

fn tab(h: &mut Harness) {
    h.key("Tab", "Tab");
    h.run(0.1);
}

#[test]
fn song_select_starts_on_the_song_and_its_preferred_chart() {
    let (h, s) = select([1280.0, 720.0], DESIGN, false);
    let s = s.borrow();
    assert_eq!(s.screen, "select");
    assert_eq!(s.song, "Some Things Must");
    assert_eq!(s.style, "Single");
    assert_eq!(s.chart.as_deref(), Some("Hard 13"));
    assert_eq!(s.ladder.len(), 5);
    assert!(!s.greyed);
    assert_eq!(
        s.backdrop.as_deref(),
        Some("songs/some-things-must/sevish-bg.png")
    );
    // Every style shown, lit with its count for this song.
    assert_eq!(
        s.style_counts,
        [
            ("Single".to_string(), 5),
            ("Double".to_string(), 1),
            ("Pump single".to_string(), 6),
            ("Pump half-double".to_string(), 0),
            ("Pump double".to_string(), 2),
        ]
    );
    assert!(s.hints.iter().any(|t| t == "ENTER Play"), "{:?}", s.hints);
    assert!(h.texts().iter().any(|t| t == "Some Things Must"));
}

#[test]
fn changing_style_keeps_the_difficulty_or_the_closest() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    // Double has only a Beginner: the closest to Hard.
    tab(&mut h);
    assert_eq!(s.borrow().style, "Double");
    assert_eq!(s.borrow().chart.as_deref(), Some("Beginner 2"));
    // The player's choice is still Hard: Pump single has one.
    tab(&mut h);
    assert_eq!(s.borrow().style, "Pump single");
    assert_eq!(s.borrow().chart.as_deref(), Some("Hard 16"));
    // Half-double has nothing here: Tab skips to the next lit style.
    tab(&mut h);
    assert_eq!(s.borrow().style, "Pump double");
    assert_eq!(s.borrow().chart.as_deref(), Some("Hard 17"));
    // Left picks Medium, which becomes the choice kept.
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.1);
    assert_eq!(s.borrow().chart.as_deref(), Some("Medium 11"));
    assert_eq!(s.borrow().preferred, "Medium");
    tab(&mut h);
    assert_eq!(s.borrow().style, "Single");
    assert_eq!(s.borrow().chart.as_deref(), Some("Medium 10"));
}

#[test]
fn songs_without_charts_in_the_style_are_greyed_not_hidden() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    let items = s.borrow().items;
    assert_eq!(
        s.borrow().greyed_rows,
        0,
        "every song nearby has Single charts"
    );
    // Pump double: three of the bundled songs have none.
    for _ in 0..3 {
        tab(&mut h);
    }
    assert_eq!(s.borrow().style, "Pump double");
    assert_eq!(s.borrow().items, items, "no song is hidden");
    assert!(s.borrow().greyed_rows > 0);
    // Onto Into My Dream (no pump double): still selectable, greyed,
    // a message for the ladder.
    h.key("ArrowUp", "ArrowUp");
    h.run(0.5);
    let st = s.borrow();
    assert_eq!(st.song, "Into My Dream");
    assert!(st.greyed);
    assert_eq!(st.chart, None);
    assert!(st.ladder.is_empty());
    assert!(
        !st.hints.iter().any(|t| t == "ENTER Play"),
        "{:?}",
        st.hints
    );
    drop(st);
    assert!(
        h.texts()
            .iter()
            .any(|t| t == "No Pump double charts for this song.")
    );
}

#[test]
fn packs_open_and_close() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    let items = s.borrow().items;
    // Back on a song closes its pack, the header selected.
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(s.borrow().closed, ["Bundled songs"]);
    assert_eq!(s.borrow().items, items - 4);
    assert_eq!(s.borrow().header.as_deref(), Some("Bundled songs"));
    // Start on the header opens it again.
    h.key("Enter", "Enter");
    h.run(0.5);
    assert!(s.borrow().closed.is_empty());
    assert_eq!(s.borrow().items, items);
    // Down into another pack's header, Start closes it.
    for _ in 0..5 {
        h.key("ArrowDown", "ArrowDown");
    }
    h.run(0.5);
    assert_eq!(s.borrow().header.as_deref(), Some("Arrow Basics"));
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(s.borrow().closed, ["Arrow Basics"]);
}

#[test]
fn search_filters_the_wheel_and_never_shows_its_popup_while_composing() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    let all = s.borrow().items;
    h.key("Slash", "/");
    h.run(0.1);
    assert!(h.ui.text_field().is_some(), "typing");
    assert!(
        s.borrow()
            .hints
            .iter()
            .any(|t| t.contains("kana and romaji"))
    );
    // Composing: no popup, whatever the suggestions.
    h.sync("Swe", [3, 3], Some([0, 3]));
    h.run(0.1);
    assert!(!s.borrow().popup);
    // Committed: the popup and the wheel's matches.
    h.sync("Swe", [3, 3], None);
    h.run(0.1);
    assert!(s.borrow().popup, "{:?}", s.borrow().suggestions);
    assert!(s.borrow().suggestions.iter().any(|t| t == "Sweeteners"));
    assert_eq!(s.borrow().matches, 1);
    assert_eq!(s.borrow().items, 2, "the pack's header and the song");
    assert_eq!(s.borrow().song, "Sweeteners");
    // Composing again closes it.
    h.sync("Swee", [4, 4], Some([3, 4]));
    h.run(0.1);
    assert!(!s.borrow().popup);
    // Step artists are searched too; kana match romaji.
    h.sync("YOSEFU", [6, 6], None);
    h.run(0.1);
    assert_eq!(s.borrow().matches, 2);
    h.sync("hoshizora", [9, 9], None);
    h.run(0.1);
    assert!(s.borrow().matches >= 1);
    assert!(s.borrow().song.contains("ほしぞら"), "{}", s.borrow().song);
    // Escape closes the popup, Escape stops typing, Escape clears.
    h.key("Escape", "Escape");
    h.key("Escape", "Escape");
    h.run(0.1);
    assert!(h.ui.text_field().is_none());
    h.key("Escape", "Escape");
    h.run(0.3);
    assert_eq!(s.borrow().search, "");
    assert_eq!(s.borrow().items, all);
}

#[test]
fn the_pulse_runs_on_the_beat_and_not_with_reduced_motion() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    // 160 BPM: lit on the beat, settled between.
    let mut seen = Vec::new();
    for _ in 0..30 {
        h.frame(1.0 / 60.0);
        seen.push(s.borrow().pulse);
    }
    assert!(s.borrow().pulsing);
    let max = seen.iter().copied().fold(0.0, f32::max);
    let min = seen.iter().copied().fold(1.0, f32::min);
    assert!(max > 0.2 && min < 0.05, "{min} {max}");
    // It needs frames while it runs.
    assert_eq!(h.run_on_demand(1.0), 60);

    let (mut h, s) = select([1280.0, 720.0], DESIGN, true);
    h.run(1.0);
    assert!(!s.borrow().pulsing);
    assert_eq!(s.borrow().pulse, 0.0);
    assert!(s.borrow().reduced_motion);
    // Idle: no frames at all.
    assert_eq!(h.run_on_demand(1.0), 0);
    // The wheel jumps rather than glides (the backdrop still fades).
    h.key("ArrowDown", "ArrowDown");
    h.frame(1.0 / 60.0);
    assert_eq!(h.ui.bounds("Wheel").map(|_| ()), Some(()));
    assert!(h.run_on_demand(1.0) <= 30);
    assert_eq!(s.borrow().song, "Sweeteners");
}

#[test]
fn results_and_options_come_and_go() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(s.borrow().screen, "results");
    assert_eq!(s.borrow().panels, "Overview");
    assert!(
        h.texts().iter().any(|t| t.contains("96.77")),
        "{:?}",
        h.texts()
    );
    h.key("ArrowRight", "ArrowRight");
    h.run(0.1);
    assert_eq!(s.borrow().panels, "Timing");
    h.tap(PointerKind::Mouse, "OffsetUp");
    h.run(0.1);
    assert_eq!(s.borrow().song_offset, 1);
    h.tap(PointerKind::Mouse, "SongList");
    h.run(0.5);
    assert_eq!(s.borrow().screen, "select");
    // Start goes back too (Song list has focus when results come in).
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(s.borrow().screen, "results");
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(s.borrow().screen, "select");
    // The footer's Options link.
    h.tap(PointerKind::Mouse, "Link.Options");
    h.run(0.5);
    assert_eq!(s.borrow().screen, "options");
    assert_eq!(s.borrow().option, "Speed");
    assert_eq!(s.borrow().value, "×2.5");
    h.key("ArrowRight", "ArrowRight");
    assert_eq!(s.borrow().value, "×2.75");
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(s.borrow().option, "Reverse");
    h.key("ArrowLeft", "ArrowLeft");
    assert_eq!(s.borrow().value, "Off");
    h.key("Tab", "Tab");
    assert_eq!(s.borrow().section, "Rules");
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(s.borrow().screen, "select");
}

#[test]
fn a_small_window_gets_the_compact_layout() {
    let (mut h, s) = select([844.0, 390.0], COMPACT, false);
    assert!(s.borrow().compact);
    assert!(s.borrow().hints.is_empty(), "no key hints on a phone");
    assert!(
        h.texts().iter().any(|t| t == "PLAY HARD 13"),
        "{:?}",
        h.texts()
    );
    assert!(h.ui.bounds("Chart4").is_some());
    // A chart button picks, the style button moves on.
    h.tap(PointerKind::Touch, "Chart2");
    h.run(0.2);
    assert_eq!(s.borrow().chart.as_deref(), Some("Medium 10"));
    h.tap(PointerKind::Touch, "StyleButton");
    h.run(0.2);
    assert_eq!(s.borrow().style, "Double");
    // Larger again: the full layout.
    h.ui.set_design(DESIGN);
    h.send(crate::UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    h.run(0.3);
    assert!(!s.borrow().compact);
    assert_eq!(s.borrow().style, "Double");
}

#[test]
fn the_compact_threshold() {
    let p = Plain;
    assert_eq!(super::design_for(&p, [844.0, 390.0]), COMPACT);
    assert_eq!(super::design_for(&p, [932.0, 430.0]), COMPACT);
    assert_eq!(super::design_for(&p, [1024.0, 600.0]), DESIGN);
}

// Cases from the step 5 review (its probes, kept).

use super::data::{ChartEntry, SelectData, SongEntry, StyleDef};
use super::select::{OpenFn, PlayFn, SelectScreen};
use ddi_chart::Difficulty;

/// Song select whose Start records what it would play.
fn recording(data: SelectData, size: [f32; 2], design: [f32; 2]) -> (Harness, SharedTheme) {
    let env = env();
    let state = env.state.clone();
    let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Mirror);
    h.ui.set_design(design);
    let play: PlayFn = Rc::new(|song, chart, env: &Env, _| {
        env.state.borrow_mut().status = format!(
            "PLAYED {} {:?} {}",
            song.title, chart.difficulty, chart.meter
        );
    });
    let open: OpenFn = Rc::new(|_, _| {});
    h.push(SelectScreen::new(
        env,
        data,
        Some("some-things-must"),
        play,
        open,
    ));
    h.run(0.5);
    (h, state)
}

#[test]
fn the_ladder_picks_and_lights_what_it_shows() {
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    // Pump single: six charts, all shown at 720 high, Hard 16 picked.
    tab(&mut h);
    tab(&mut h);
    h.run(0.3);
    assert_eq!(s.borrow().chart.as_deref(), Some("Hard 16"));
    for k in 0..6 {
        assert!(h.ui.bounds(&format!("Ladder.Row{k}")).is_some(), "row {k}");
    }
    // The first row is Beginner 1; the fifth (Hard) is the pick.
    h.tap(PointerKind::Mouse, "Ladder.Row0");
    h.run(0.2);
    assert_eq!(s.borrow().chart.as_deref(), Some("Beginner 1"));
    h.tap(PointerKind::Mouse, "Ladder.Row4");
    h.run(0.2);
    assert_eq!(s.borrow().chart.as_deref(), Some("Hard 16"));
    // A tap on the picked row plays it.
    h.tap(PointerKind::Mouse, "Ladder.Row4");
    h.run(0.2);
    assert_eq!(s.borrow().status, "PLAYED Some Things Must Hard 16");
}

#[test]
fn start_acts_on_the_song_moved_to_in_the_same_poll() {
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    h.key("ArrowUp", "ArrowUp");
    h.key("Enter", "Enter");
    assert_eq!(s.borrow().status, "PLAYED Into My Dream Hard 12");
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    h.pad(crate::MenuAction::Up, true);
    h.pad(crate::MenuAction::Start, true);
    assert_eq!(s.borrow().status, "PLAYED Into My Dream Hard 12");
    // Left too: the chart of the song moved to.
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.2);
    assert_eq!(s.borrow().song, "Sweeteners");
    assert_eq!(s.borrow().chart.as_deref(), Some("Medium 10"));
}

#[test]
fn enter_in_the_search_goes_to_the_song_selected() {
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    h.key("Slash", "/");
    h.sync("a", [1, 1], None);
    h.run(0.1);
    h.key("Escape", "Escape");
    for _ in 0..3 {
        h.key("ArrowDown", "ArrowDown");
        h.run(0.3);
    }
    let song = s.borrow().song.clone();
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(s.borrow().song, song);
    assert!(h.ui.text_field().is_none());
}

#[test]
fn the_pick_survives_a_new_layout() {
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    tab(&mut h);
    tab(&mut h);
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.1);
    assert_eq!(s.borrow().chart.as_deref(), Some("Medium 10"));
    h.send(crate::UiEvent::Resize {
        size: [1200.0, 700.0],
        ratio: 1.0,
    });
    h.run(0.3);
    assert_eq!(s.borrow().chart.as_deref(), Some("Medium 10"));
    h.ui.set_design(COMPACT);
    h.send(crate::UiEvent::Resize {
        size: [844.0, 390.0],
        ratio: 1.0,
    });
    h.run(0.3);
    assert!(s.borrow().compact);
    assert_eq!(s.borrow().chart.as_deref(), Some("Medium 10"));
}

#[test]
fn clearing_a_search_opens_the_shown_songs_pack() {
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    // Close Bundled songs on Some Things Must; the header selected.
    h.key("Escape", "Escape");
    h.run(0.5);
    h.key("Slash", "/");
    h.sync("Swe", [3, 3], None);
    h.run(0.3);
    assert_eq!(s.borrow().song, "Sweeteners");
    for _ in 0..3 {
        if h.ui.text_field().is_some() {
            h.key("Escape", "Escape");
        }
    }
    h.run(0.1);
    assert!(
        s.borrow().hints.iter().any(|t| t == "ESC Clear search"),
        "{:?}",
        s.borrow().hints
    );
    h.key("Escape", "Escape");
    h.run(0.5);
    let st = s.borrow();
    assert_eq!(st.search, "");
    assert_eq!(st.song, "Sweeteners");
    assert!(st.closed.is_empty(), "{:?}", st.closed);
    assert_eq!(st.header, None);
}

#[test]
fn a_header_during_a_search_keeps_its_state() {
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    h.key("Slash", "/");
    h.sync("Swe", [3, 3], None);
    h.run(0.2);
    h.key("Escape", "Escape");
    h.key("Escape", "Escape");
    h.key("ArrowUp", "ArrowUp");
    h.run(0.3);
    assert!(s.borrow().header.is_some());
    h.key("Enter", "Enter");
    h.run(0.3);
    assert!(s.borrow().closed.is_empty());
    assert_eq!(s.borrow().items, 2);
}

#[test]
fn the_compact_field_hides_when_typing_stops() {
    let (mut h, _) = recording(sample::select(), [844.0, 390.0], COMPACT);
    assert!(!h.actor("Search").visible);
    h.tap(PointerKind::Touch, "SearchIcon");
    h.run(0.2);
    assert!(h.actor("Search").visible);
    h.key("Escape", "Escape");
    h.run(0.2);
    assert!(h.ui.text_field().is_none());
    assert!(!h.actor("Search").visible);
    // Typed, then cleared.
    h.tap(PointerKind::Touch, "SearchIcon");
    h.sync("Swe", [3, 3], None);
    h.run(0.2);
    h.key("Escape", "Escape");
    h.key("Escape", "Escape");
    h.run(0.1);
    assert!(h.actor("Search").visible, "it holds text");
    h.key("Escape", "Escape");
    h.run(0.1);
    assert!(!h.actor("Search").visible);
}

fn song(id: &str, charts: Vec<ChartEntry>) -> SongEntry {
    SongEntry {
        id: id.into(),
        title: id.into(),
        artist: "x".into(),
        pack: "P".into(),
        bpm: 120.0,
        bpm_text: "120".into(),
        length: 100.0,
        charts,
        ..Default::default()
    }
}

#[test]
fn empty_and_tiny_libraries_take_every_key() {
    let style = || StyleDef {
        id: "s".into(),
        name: "S".into(),
    };
    let chart = ChartEntry {
        style: "s".into(),
        difficulty: Difficulty::Easy,
        meter: 3,
        notes: 100,
        credit: "c".into(),
    };
    for data in [
        SelectData::default(),
        SelectData {
            styles: vec![style()],
            ..Default::default()
        },
        SelectData {
            songs: vec![song("a", vec![])],
            ..Default::default()
        },
        SelectData {
            styles: vec![style()],
            songs: vec![song("a", vec![chart.clone()])],
            ..Default::default()
        },
    ] {
        for (size, design) in [([1280.0, 720.0], DESIGN), ([844.0, 390.0], COMPACT)] {
            let (mut h, _) = recording(data.clone(), size, design);
            for (c, k) in [
                ("Enter", "Enter"),
                ("Tab", "Tab"),
                ("ArrowLeft", "ArrowLeft"),
                ("ArrowRight", "ArrowRight"),
                ("ArrowUp", "ArrowUp"),
                ("ArrowDown", "ArrowDown"),
                ("Escape", "Escape"),
                ("Escape", "Escape"),
            ] {
                h.key(c, k);
                h.run(0.1);
            }
            h.key("Slash", "/");
            h.sync("zz", [2, 2], None);
            h.key("ArrowDown", "ArrowDown");
            h.key("Enter", "Enter");
            h.key("Escape", "Escape");
            h.run(0.1);
            for n in [
                "Play",
                "Chart0",
                "StyleButton",
                "Ladder.Row0",
                "Tab0",
                "SearchIcon",
                "Back",
            ] {
                if h.ui.bounds(n).is_some() {
                    h.tap(PointerKind::Touch, n);
                    h.run(0.1);
                }
            }
        }
    }
}

#[test]
fn typing_goes_on_through_a_new_layout() {
    let env = env();
    let state = env.state.clone();
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(sample::select_screen(&env, None));
    h.run(0.3);
    h.key("Slash", "/");
    h.sync("Swe", [3, 3], None);
    h.run(0.2);
    env.reduced_motion.set(true);
    h.run(0.2);
    h.sync("Swee", [4, 4], None);
    h.run(0.2);
    assert_eq!(state.borrow().search, "Swee");
    assert_eq!(state.borrow().matches, 1);
    h.ui.set_design(COMPACT);
    h.send(crate::UiEvent::Resize {
        size: [844.0, 390.0],
        ratio: 1.0,
    });
    h.run(0.3);
    h.sync("Sweet", [5, 5], None);
    h.run(0.2);
    assert_eq!(state.borrow().search, "Sweet");
    assert!(h.actor("Search").visible);
    assert_eq!(state.borrow().song, "Sweeteners");
}
