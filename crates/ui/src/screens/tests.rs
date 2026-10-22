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

fn double_arrow(h: &mut Harness, code: &str) {
    h.key(code, code);
    h.run(0.08);
    h.key(code, code);
    h.run(0.2);
}

#[test]
fn every_arrow_moves_songs_on_a_single_tap_and_charts_on_a_double_tap() {
    for (key, song, chart) in [
        ("ArrowUp", "Into My Dream", "Medium 10"),
        ("ArrowLeft", "Into My Dream", "Medium 10"),
        ("ArrowDown", "Sweeteners", "Challenge 16"),
        ("ArrowRight", "Sweeteners", "Challenge 16"),
    ] {
        let (mut h, s) = select([1280.0, 720.0], DESIGN, true);
        h.key(key, key);
        h.run(0.1);
        assert_eq!(s.borrow().song, "Some Things Must", "wait for a second tap");
        h.run_on_demand(0.5);
        assert_eq!(s.borrow().song, song, "{key}");
        assert_eq!(
            s.borrow().preferred,
            "Hard",
            "single taps keep the difficulty"
        );

        let (mut h, s) = select([1280.0, 720.0], DESIGN, true);
        double_arrow(&mut h, key);
        assert_eq!(s.borrow().chart.as_deref(), Some(chart), "{key}");
        h.run_on_demand(0.6);
        assert_eq!(
            s.borrow().song,
            "Some Things Must",
            "a double tap never moves songs"
        );
        assert_eq!(s.borrow().chart.as_deref(), Some(chart));
    }
}

#[test]
fn held_arrows_scroll_songs_and_never_change_difficulty() {
    for key in ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"] {
        let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
        h.key_down(key, key);
        h.run(0.8);
        h.key_up(key, key);
        assert_eq!(s.borrow().preferred, "Hard", "{key}");
        assert_ne!(s.borrow().song, "Some Things Must", "{key}");
    }
}

#[test]
fn pads_double_tap_all_four_directions_without_moving_the_song() {
    use crate::MenuAction::{Down, Left, Right, Up};
    for (action, chart) in [
        (Up, "Medium 10"),
        (Left, "Medium 10"),
        (Down, "Challenge 16"),
        (Right, "Challenge 16"),
    ] {
        let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
        h.pad(action, true);
        h.pad(action, false);
        h.run(0.08);
        h.pad(action, true);
        h.pad(action, false);
        h.run(0.5);
        assert_eq!(s.borrow().song, "Some Things Must");
        assert_eq!(s.borrow().chart.as_deref(), Some(chart));
    }
}

#[test]
fn mixed_directions_are_separate_moves_and_other_inputs_cancel_a_pending_tap() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    h.key("ArrowDown", "ArrowDown");
    h.run(0.08);
    h.key("ArrowRight", "ArrowRight");
    let after = s.borrow().song.clone();
    assert_ne!(
        after, "Some Things Must",
        "different directions commit both moves"
    );
    h.key("Slash", "/");
    h.run(0.5);
    assert_eq!(
        s.borrow().song,
        after,
        "search cancels provisional navigation"
    );
    assert_eq!(s.borrow().preferred, "Hard");
}

#[test]
fn slow_taps_and_taps_from_different_devices_do_not_change_difficulty() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.4);
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.4);
    assert_eq!(s.borrow().song, "Beatucada");
    assert_eq!(s.borrow().preferred, "Hard");

    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.08);
    h.pad(crate::MenuAction::Left, true);
    h.pad(crate::MenuAction::Left, false);
    h.run(0.4);
    assert_eq!(s.borrow().song, "Beatucada");
    assert_eq!(s.borrow().preferred, "Hard");
}

#[test]
fn choosing_a_chart_by_pointer_cancels_a_pending_song_move() {
    for kind in [PointerKind::Mouse, PointerKind::Touch] {
        let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
        h.key("ArrowRight", "ArrowRight");
        h.tap(kind, "Ladder.Row2");
        h.run(0.6);
        assert_eq!(s.borrow().song, "Some Things Must");
        assert_eq!(s.borrow().chart.as_deref(), Some("Medium 10"));
    }
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
    // Double Left picks Medium, which becomes the choice kept.
    double_arrow(&mut h, "ArrowLeft");
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
    // Activate the pack header to close it (Back now leaves the screen).
    for _ in 0..3 {
        h.key("ArrowUp", "ArrowUp");
        h.run(0.5);
    }
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(s.borrow().closed.len(), 6);
    assert!(s.borrow().closed.contains(&"Bundled songs".into()));
    assert_eq!(s.borrow().items, items - 4);
    assert_eq!(s.borrow().header.as_deref(), Some("Bundled songs"));
    // Start on the header opens it again.
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(s.borrow().closed.len(), 5);
    assert!(!s.borrow().closed.contains(&"Bundled songs".into()));
    assert_eq!(s.borrow().items, items);
    // Down into another pack's header, Start opens it and closes the previous pack.
    for _ in 0..5 {
        h.key("ArrowDown", "ArrowDown");
        h.run(0.3);
    }
    h.run(0.5);
    assert_eq!(s.borrow().header.as_deref(), Some("Arrow Basics"));
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(s.borrow().closed.len(), 5);
    assert!(s.borrow().closed.contains(&"Bundled songs".into()));
    assert!(!s.borrow().closed.contains(&"Arrow Basics".into()));
}

#[test]
fn search_filters_the_wheel_and_never_shows_its_popup_while_composing() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
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
    // Escape closes the popup and stops typing; Clear removes the filter.
    h.key("Escape", "Escape");
    h.key("Escape", "Escape");
    h.run(0.1);
    assert!(h.ui.text_field().is_none());
    h.tap(PointerKind::Mouse, "ClearSearch");
    h.run(0.3);
    assert_eq!(s.borrow().search, "");
    let data = sample::select();
    let pack = &data
        .songs
        .iter()
        .find(|song| song.title == s.borrow().song)
        .unwrap()
        .pack;
    let songs = data.songs.iter().filter(|song| &song.pack == pack).count();
    assert_eq!(s.borrow().items, songs + 6);
    assert_eq!(s.borrow().closed.len(), 5);
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
    assert!(
        h.run_on_demand(1.0) <= 45,
        "tap window plus the backdrop fade"
    );
    assert_eq!(s.borrow().song, "Sweeteners");
}

#[test]
fn results_and_options_come_and_go() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    h.key("Enter", "Enter");
    h.run(0.7);
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
    // Start goes back to the songs (Song list has focus when results come
    // in), once they have settled; R retries; Backspace goes back.
    h.key("Enter", "Enter");
    h.run(0.7);
    assert_eq!(s.borrow().screen, "results");
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(s.borrow().screen, "select");
    h.key("Enter", "Enter");
    h.run(0.7);
    h.key("KeyR", "r");
    h.run(0.2);
    assert_eq!(s.borrow().status, "retry");
    assert_eq!(s.borrow().screen, "results");
    h.key("Backspace", "Backspace");
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
    h.key("ArrowRight", "ArrowRight");
    assert_eq!(s.borrow().value, "On");
    h.key("Tab", "Tab");
    assert_eq!(s.borrow().section, "Notes");
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
    // After quiet, double Left changes the chart of the song moved to.
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    h.key("ArrowDown", "ArrowDown");
    h.run(0.5);
    double_arrow(&mut h, "ArrowLeft");
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
fn escape_from_the_search_gives_the_focus_back() {
    let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
    assert_eq!(h.ui.focused(), Some("Wheel"));
    h.key("Slash", "/");
    h.sync("Swe", [3, 3], None);
    h.run(0.1);
    assert_eq!(h.ui.focused(), Some("Search"));
    assert!(s.borrow().popup);
    // The first Escape closes the popup, still typing in the field.
    h.key("Escape", "Escape");
    h.run(0.1);
    assert!(h.ui.text_field().is_some());
    assert_eq!(h.ui.focused(), Some("Search"));
    // The next stops typing: the field is left for the wheel.
    h.key("Escape", "Escape");
    h.run(0.1);
    assert!(h.ui.text_field().is_none());
    assert_eq!(h.ui.focused(), Some("Wheel"));
    assert_eq!(s.borrow().search, "Swe", "the filter stays");
    // From a header's switch, back to it; a press elsewhere leaves it too.
    h.tap(PointerKind::Mouse, "Autoplay");
    h.run(0.1);
    assert_eq!(h.ui.focused(), Some("Autoplay"));
    h.key("Slash", "/");
    h.run(0.1);
    assert_eq!(h.ui.focused(), Some("Search"));
    h.key("Escape", "Escape");
    h.run(0.1);
    assert_eq!(h.ui.focused(), Some("Autoplay"));
    h.key("Slash", "/");
    h.run(0.1);
    h.send(crate::UiEvent::Text(crate::event::TextEvent::Blurred));
    h.run(0.1);
    assert!(h.ui.text_field().is_none());
    assert_eq!(h.ui.focused(), Some("Autoplay"));
}

#[test]
fn the_pick_survives_a_new_layout() {
    let (mut h, s) = recording(sample::select(), [1280.0, 720.0], DESIGN);
    tab(&mut h);
    tab(&mut h);
    double_arrow(&mut h, "ArrowLeft");
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
    // Close Bundled songs through its header.
    for _ in 0..3 {
        h.key("ArrowUp", "ArrowUp");
        h.run(0.5);
    }
    h.key("Enter", "Enter");
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
        s.borrow().hints.iter().any(|t| t == "ESC Back"),
        "{:?}",
        s.borrow().hints
    );
    h.tap(PointerKind::Mouse, "ClearSearch");
    h.run(0.5);
    let st = s.borrow();
    assert_eq!(st.search, "");
    assert_eq!(st.song, "Sweeteners");
    assert_eq!(st.closed.len(), 5);
    assert!(!st.closed.contains(&"Bundled songs".into()));
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
    assert_eq!(s.borrow().closed.len(), 5);
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
    h.tap(PointerKind::Touch, "ClearSearch");
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
fn a_small_library_is_one_list_without_packs() {
    let data = |n: usize| SelectData {
        styles: sample::styles(),
        songs: (0..n)
            .map(|i| SongEntry {
                pack: format!("Pack {}", i % 2),
                ..song(&format!("s{i}"), vec![])
            })
            .collect(),
        ..Default::default()
    };
    // Nine songs in two packs: the nine, no headers, none closed away.
    let (mut h, s, _, _) = shelled(data(9), None);
    h.run(0.3);
    assert_eq!(s.borrow().items, 9);
    assert_eq!(s.borrow().header, None);
    for _ in 0..9 {
        h.key("ArrowDown", "ArrowDown");
        h.run(0.1);
        assert_eq!(s.borrow().header, None);
    }
    // Ten: by pack, under their headers.
    let (mut h, s, _, _) = shelled(data(10), None);
    h.run(0.3);
    assert!(s.borrow().items < 10 + 2, "a pack closed");
    h.key("Home", "Home");
    for _ in 0..12 {
        if s.borrow().header.is_some() {
            break;
        }
        h.key("ArrowDown", "ArrowDown");
        h.run(0.1);
    }
    assert!(s.borrow().header.is_some());
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
        ..Default::default()
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

#[test]
fn results_ask_the_shell_and_keep_what_they_show() {
    use super::results::{ResultsActions, ResultsScreen};
    use std::cell::RefCell;
    let env = env();
    let state = env.state.clone();
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let l = |what: &'static str| {
        let log = log.clone();
        Rc::new(move |_: &Env, _: &mut crate::Cx| log.borrow_mut().push(what.into()))
            as super::results::ActionFn
    };
    let offsets = log.clone();
    let panels = log.clone();
    let actions = ResultsActions {
        retry: Some(l("retry")),
        song_list: Some(l("song list")),
        offset: Some(Rc::new(move |s| {
            offsets.borrow_mut().push(format!("offset {s}"))
        })),
        panels: Some(Rc::new(move |n| {
            panels.borrow_mut().push(format!("panels {n}"))
        })),
        replay: None,
    };
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(ResultsScreen::new(env, sample::results(), actions));
    // Keys held or mashed as the song ends do nothing at first; frames
    // run until then (on demand, too).
    h.run(0.3);
    assert!(h.ui.needs_redraw());
    h.key("Enter", "Enter");
    h.key("KeyR", "r");
    // A tap mashed at the end too (a phone's touch lanes covered the
    // screen).
    h.tap(PointerKind::Touch, "SongList");
    h.tap(PointerKind::Touch, "Retry");
    h.pad(crate::MenuAction::Start, true);
    h.pad(crate::MenuAction::Start, false);
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    assert!(h.run_on_demand(1.0) < 40);
    // A held key's repeat is let go too.
    let mut held = crate::event::KeyEvent::down("KeyR", "r");
    held.repeat = true;
    h.send(crate::UiEvent::Key(held));
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    // Song list has focus: Enter goes back; R and a pad's Select retry;
    // Escape and Backspace go back.
    h.key("Enter", "Enter");
    h.key("KeyR", "r");
    h.pad(crate::MenuAction::Select, true);
    h.pad(crate::MenuAction::Select, false);
    h.key("Escape", "Escape");
    h.key("Backspace", "Backspace");
    // Tab is not a retry (it changes the style on song select).
    h.key("Tab", "Tab");
    assert_eq!(
        *log.borrow(),
        ["song list", "retry", "retry", "song list", "song list"]
    );
    log.borrow_mut().clear();
    // The offset: a millisecond either way, this play's mean, reset.
    h.tap(PointerKind::Mouse, "OffsetUp");
    h.run(0.1);
    h.tap(PointerKind::Mouse, "OffsetUp");
    h.run(0.1);
    assert_eq!(state.borrow().song_offset, 2);
    h.tap(PointerKind::Mouse, "UseMean");
    h.run(0.1);
    assert_eq!(state.borrow().song_offset, -2);
    // Taken: not offered again; reset is.
    assert!(h.ui.bounds("UseMean").is_none());
    h.tap(PointerKind::Mouse, "ResetOffset");
    h.run(0.1);
    assert_eq!(state.borrow().song_offset, 0);
    assert!(h.ui.bounds("ResetOffset").is_none());
    // Not the offset set any more: offered again, as the old card did.
    assert!(h.ui.bounds("UseMean").is_some());
    assert_eq!(
        *log.borrow(),
        ["offset 0.001", "offset 0.002", "offset -0.002", "offset 0"]
    );
    log.borrow_mut().clear();
    // Panels, told to the shell; kept through a new layout.
    h.key("ArrowRight", "ArrowRight");
    h.key("ArrowRight", "ArrowRight");
    assert_eq!(*log.borrow(), ["panels Timing", "panels Song"]);
    h.send(crate::UiEvent::Resize {
        size: [1000.0, 700.0],
        ratio: 1.0,
    });
    h.run(0.2);
    assert_eq!(state.borrow().panels, "Song");
    assert!(h.ui.bounds("Panel.Life").is_some());
    assert!(h.ui.bounds("Panel.Timing").is_none());
}

#[test]
fn results_offer_the_replay_and_a_replays_results_watch_again() {
    use super::data::ResultsKind;
    use super::results::{ResultsActions, ResultsScreen};
    use std::cell::RefCell;
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let l = |what: &'static str| {
        let log = log.clone();
        Rc::new(move |_: &Env, _: &mut crate::Cx| log.borrow_mut().push(what.into()))
            as super::results::ActionFn
    };
    let actions = || ResultsActions {
        retry: Some(l("retry")),
        song_list: Some(l("song list")),
        replay: Some(l("replay")),
        ..ResultsActions::default()
    };
    for (size, compact) in [
        ([1280.0, 720.0], false),
        ([390.0, 844.0], true),
        ([844.0, 390.0], true),
    ] {
        // A play: Watch replay by W or its button (on top beside the
        // offset editor; in the bar on an upright phone).
        let e = env();
        let state = e.state.clone();
        let mut h = Harness::new(super::design_for(&Plain, size), size, 1.0, TextMode::Mirror);
        h.push(ResultsScreen::new(e, sample::results(), actions()));
        h.run(1.0);
        assert_eq!(state.borrow().compact, compact, "{size:?}");
        assert!(h.ui.bounds("WatchReplay").is_some(), "{size:?}");
        assert!(h.ui.bounds("Retry").is_some());
        // Nothing overlaps the buttons.
        let r = h.ui.bounds("WatchReplay").unwrap();
        for other in [
            "Retry",
            "SongList",
            "OffsetDown",
            "OffsetUp",
            "UseMean",
            "ResetOffset",
            "Back",
            "PanelSet0",
            "PanelSet1",
            "PanelSet2",
        ] {
            if let Some(o) = h.ui.bounds(other) {
                // Corners: `[x0, y0, x1, y1]`.
                let apart = r[2] <= o[0] || o[2] <= r[0] || r[3] <= o[1] || o[3] <= r[1];
                assert!(apart, "{size:?}: WatchReplay {r:?} over {other} {o:?}");
            }
        }
        h.key("KeyW", "w");
        h.tap(PointerKind::Mouse, "WatchReplay");
        assert_eq!(*log.borrow(), ["replay", "replay"], "{size:?}");
        log.borrow_mut().clear();
    }
    // A replay's results: marked, Watch again, no editor, no W.
    let mut data = sample::results();
    super::play::mark_replay(&mut data, ResultsKind::Replay, "Played 2026-10-16 14:03");
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(ResultsScreen::new(env(), data, actions()));
    h.run(1.0);
    assert!(h.ui.bounds("WatchReplay").is_none());
    assert!(h.ui.bounds("OffsetUp").is_none());
    assert!(h.ui.bounds("Replay").is_some());
    h.key("KeyW", "w");
    h.key("KeyR", "r");
    h.tap(PointerKind::Mouse, "Retry");
    assert_eq!(*log.borrow(), ["retry", "retry"]);
    log.borrow_mut().clear();
    // A kept play's: its replay, no Retry; R does nothing, Select watches.
    let mut data = sample::results();
    super::play::mark_replay(&mut data, ResultsKind::Kept, "");
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(ResultsScreen::new(env(), data, actions()));
    h.run(1.0);
    assert!(h.ui.bounds("Retry").is_none());
    h.key("KeyR", "r");
    h.key("KeyW", "w");
    h.pad(crate::MenuAction::Select, true);
    h.pad(crate::MenuAction::Select, false);
    h.key("Escape", "Escape");
    assert_eq!(*log.borrow(), ["replay", "replay", "song list"]);
}

// Song select on a shell (step 6.2): what it asks, what it is sent.

use std::cell::RefCell;

use super::data::{
    Best, ImportReportView, ImportView, Notice, PackEntry, SelectData as Data, SongEntry as Song,
};
use super::select::{FEED, Feed, SelectCall, SharedFeed, ShellFn};

type Calls = Rc<RefCell<Vec<SelectCall>>>;

fn shelled(data: Data, start: Option<&str>) -> (Harness, SharedTheme, Calls, SharedFeed) {
    shelled_with(data, start, true)
}

/// `gestures`: whether the platform's field starts only from a key or a
/// tap (a browser), or from anything (desktop).
fn shelled_with(
    data: Data,
    start: Option<&str>,
    gestures: bool,
) -> (Harness, SharedTheme, Calls, SharedFeed) {
    let env = env();
    let state = env.state.clone();
    let mut h = Harness::with_gestures(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror, gestures);
    let play: PlayFn = Rc::new(|song, chart, env: &Env, _| {
        env.state.borrow_mut().status = format!("PLAYED {} {}", song.id, chart.index);
    });
    let open: OpenFn = Rc::new(|env: &Env, _| env.state.borrow_mut().status = "OPTIONS".into());
    let calls: Calls = Rc::default();
    let c = calls.clone();
    let shell: ShellFn = Rc::new(move |call, _| c.borrow_mut().push(call));
    let feed: SharedFeed = Rc::new(RefCell::new(Feed::default()));
    h.push(SelectScreen::new(env, data, start, play, open).with_shell(shell, feed.clone()));
    h.run(0.5);
    (h, state, calls, feed)
}

/// The sample, with an imported pack of two songs (one greyed in Single).
fn library() -> Data {
    let mut d = sample::select();
    let chart = |style: &str, index: usize| ChartEntry {
        style: style.into(),
        difficulty: Difficulty::Hard,
        meter: 9,
        notes: 300,
        credit: "x".into(),
        index,
        ..Default::default()
    };
    for (id, charts) in [
        ("u-one", vec![chart("dance-single", 0)]),
        ("u-two", vec![chart("dance-double", 3)]),
    ] {
        d.songs.push(Song {
            id: id.into(),
            title: id.into(),
            artist: "a".into(),
            pack: "My Pack".into(),
            bpm: 150.0,
            bpm_text: "150".into(),
            length: 90.0,
            charts,
            removable: true,
            ..Default::default()
        });
    }
    d.packs = vec![
        PackEntry {
            name: sample::BUNDLED_PACK.into(),
            ..Default::default()
        },
        PackEntry {
            name: "My Pack".into(),
            removable: true,
            rereadable: true,
            banner: None,
        },
    ];
    d
}

fn last_place(calls: &Calls) -> super::select::SelectPlace {
    calls
        .borrow()
        .iter()
        .rev()
        .find_map(|c| match c {
            SelectCall::Place(p) => Some(p.clone()),
            _ => None,
        })
        .expect("a place")
}

fn last_song(calls: &Calls) -> Option<(Option<String>, bool)> {
    calls.borrow().iter().rev().find_map(|c| match c {
        SelectCall::Selected { song, from_pad } => Some((song.clone(), *from_pad)),
        _ => None,
    })
}

#[test]
fn the_shell_hears_the_song_shown_and_the_place() {
    let (mut h, _s, calls, _) = shelled(library(), Some("some-things-must"));
    assert_eq!(
        last_song(&calls),
        Some((Some("some-things-must".into()), false))
    );
    let p = last_place(&calls);
    assert_eq!(p.song.as_deref(), Some("some-things-must"));
    assert_eq!(p.style, "dance-single");
    assert!(p.chart.is_some());
    // A single tap settles after the double-tap window, keeping its source.
    let n = calls.borrow().len();
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(
        calls.borrow().len(),
        n,
        "not selected while a double tap is possible"
    );
    h.run(0.3);
    assert_eq!(last_song(&calls), Some((Some("sweeteners".into()), false)));
    // A pad's move says so.
    h.pad(crate::MenuAction::Up, true);
    h.pad(crate::MenuAction::Up, false);
    h.run(0.3);
    assert_eq!(
        last_song(&calls),
        Some((Some("some-things-must".into()), true))
    );
    // The style and the packs closed are the place's.
    tab(&mut h);
    assert_eq!(last_place(&calls).style, "dance-double");
    let place = last_place(&calls);
    h.key("Escape", "Escape");
    h.run(0.3);
    assert_eq!(last_place(&calls), place);
    assert!(calls.borrow().contains(&SelectCall::Leave));
}

#[test]
fn the_place_comes_back() {
    let mut d = library();
    d.closed = vec![sample::BUNDLED_PACK.into()];
    d.style = Some("dance-double".into());
    let (_h, s, _, _) = shelled(d, Some("u-two"));
    let st = s.borrow();
    assert_eq!(st.song, "u-two");
    assert_eq!(st.style, "Double");
    // The start song's pack open; the bundled one kept closed.
    assert_eq!(st.closed.len(), 6);
    assert!(!st.closed.contains(&"My Pack".into()));
    drop(st);
    // A search kept.
    let mut d = library();
    d.search = "sweet".into();
    let (_h, s, _, _) = shelled(d, Some("sweeteners"));
    assert_eq!(s.borrow().search, "sweet");
    assert_eq!(s.borrow().matches, 1);
    assert_eq!(s.borrow().song, "Sweeteners");
    // The chart last started.
    let env = env();
    let state = env.state.clone();
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    let screen = SelectScreen::new(
        env,
        library(),
        Some("some-things-must"),
        Rc::new(|_, _, _: &Env, _| {}),
        Rc::new(|_, _| {}),
    );
    let medium = sample::select()
        .songs
        .iter()
        .find(|s| s.id == "some-things-must")
        .and_then(|s| {
            s.charts
                .iter()
                .find(|c| c.style == "dance-single" && c.difficulty == Difficulty::Medium)
                .map(|c| c.index)
        })
        .unwrap();
    h.push(screen.with_chart(medium));
    h.run(0.3);
    assert_eq!(state.borrow().chart.as_deref(), Some("Medium 10"));
}

#[test]
fn bests_show_on_the_ladder_and_the_wheel() {
    let (mut h, s, _, _) = shelled(library(), Some("some-things-must"));
    assert_eq!(s.borrow().best.as_deref(), Some("AA 96.12% FullCombo"));
    double_arrow(&mut h, "ArrowLeft");
    h.run(0.2);
    assert_eq!(
        s.borrow().best.as_deref(),
        Some("★★ 99.20% PerfectFullCombo")
    );
    double_arrow(&mut h, "ArrowLeft");
    h.run(0.2);
    assert_eq!(s.borrow().best, None);
    // Pending: marked.
    let mut d = library();
    for c in &mut d.songs[2].charts {
        c.best = Some(Best {
            grade: "B".into(),
            score: "80.00%".into(),
            lamp: "Clear".into(),
            lamp_text: "CLEAR".into(),
            pending: true,
            plays: 1,
            play: String::new(),
        });
    }
    let id = d.songs[2].id.clone();
    let (_h, s, _, _) = shelled(d, Some(&id));
    assert!(s.borrow().best.as_deref().unwrap().ends_with("pending"));
}

#[test]
fn new_data_keeps_the_song_shown() {
    let (mut h, s, _, feed) = shelled(library(), Some("sweeteners"));
    let mut d = library();
    d.songs.retain(|s| s.id != "into-my-dream");
    for c in &mut d
        .songs
        .iter_mut()
        .find(|s| s.id == "sweeteners")
        .unwrap()
        .charts
    {
        c.best = Some(Best {
            grade: "S".into(),
            score: "100.00%".into(),
            lamp: "MarvelousFullCombo".into(),
            lamp_text: "MFC".into(),
            ..Default::default()
        });
    }
    feed.borrow_mut().data = Some(d);
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.3);
    assert_eq!(s.borrow().song, "Sweeteners");
    assert!(s.borrow().best.as_deref().unwrap().starts_with("S 100.00%"));
    // The song shown gone: the first one.
    let mut d = library();
    d.songs.retain(|s| s.id != "sweeteners");
    feed.borrow_mut().data = Some(d);
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.3);
    assert_ne!(s.borrow().song, "Sweeteners");
    assert!(!s.borrow().song.is_empty());
}

#[test]
fn search_says_what_it_found() {
    let (mut h, s, _, _) = shelled(library(), Some("some-things-must"));
    h.key("Slash", "/");
    h.sync("dream", [5, 5], None);
    h.run(0.2);
    h.key("Escape", "Escape");
    h.key("Escape", "Escape");
    h.run(0.3);
    assert_eq!(s.borrow().matches, 1);
    assert!(
        s.borrow().hints.iter().any(|t| t == "1 song matches."),
        "{:?}",
        s.borrow().hints
    );
}

#[test]
fn links_and_notices_ask_the_shell() {
    let mut d = library();
    d.notices = vec![
        Notice {
            text: "This site stores about 12 MB in this browser.".into(),
            link: Some(("keep them".into(), "persist".into())),
            error: false,
        },
        Notice {
            text: "Controller “X” has no bindings.".into(),
            link: Some(("bind it in Options".into(), "options".into())),
            error: false,
        },
    ];
    let (mut h, s, calls, _) = shelled(d, Some("some-things-must"));
    assert_eq!(s.borrow().notices.len(), 2);
    assert!(h.texts().iter().any(|t| t.starts_with("This site stores")));
    h.tap(PointerKind::Mouse, "Notice.Link0");
    assert!(
        calls
            .borrow()
            .contains(&SelectCall::Notice("persist".into()))
    );
    h.tap(PointerKind::Mouse, "Notice.Link1");
    assert_eq!(s.borrow().status, "OPTIONS");
    h.tap(PointerKind::Mouse, "Link.Calibrate");
    assert!(
        calls
            .borrow()
            .contains(&SelectCall::Open("calibrate".into()))
    );
    // The plays are P's, the credits and the source the start menu's.
    for gone in ["Link.Plays", "Link.Credits", "Link.Source"] {
        assert!(h.ui.bounds(gone).is_none(), "{gone}");
    }
    // The mods open the options.
    s.borrow_mut().status.clear();
    h.tap(PointerKind::Mouse, "Mod0");
    assert_eq!(s.borrow().status, "OPTIONS");
    // Start plays the chart by its index.
    h.key("Enter", "Enter");
    assert!(s.borrow().status.starts_with("PLAYED some-things-must "));
}

fn top(h: &Harness) -> String {
    h.ui.layers()
        .last()
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

#[test]
fn the_import_dialog_asks_the_shell_and_shows_the_report() {
    let mut d = library();
    d.import = ImportView {
        folders: true,
        videos: false,
        ..Default::default()
    };
    let (mut h, _s, calls, feed) = shelled(d, Some("some-things-must"));
    h.tap(PointerKind::Mouse, "Link.Add");
    h.run(0.3);
    assert_eq!(top(&h), "import");
    h.tap(PointerKind::Mouse, "PickFolder");
    h.tap(PointerKind::Mouse, "PickFiles");
    h.tap(PointerKind::Mouse, "Option.videos");
    h.tap(PointerKind::Mouse, "Option.copy");
    {
        let c = calls.borrow();
        assert!(c.contains(&SelectCall::PickFolder));
        assert!(c.contains(&SelectCall::PickFiles));
        assert!(c.contains(&SelectCall::ImportOption("videos".into(), true)));
        assert!(c.contains(&SelectCall::ImportOption("copy".into(), true)));
    }
    // Running: the pickers wait.
    feed.borrow_mut().import = Some(ImportView {
        running: Some("reading 12 files…".into()),
        ..Default::default()
    });
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.3);
    assert!(h.texts().iter().any(|t| t == "reading 12 files…"));
    let n = calls.borrow().len();
    h.tap(PointerKind::Mouse, "PickFiles");
    assert_eq!(calls.borrow().len(), n, "not while importing");
    // The report, in a list.
    feed.borrow_mut().import = Some(ImportView {
        report: Some(ImportReportView {
            summary: "Imported 2 songs; 1 warning.".into(),
            error: false,
            sections: vec![(
                "Imported with problems".into(),
                vec!["Pack/Song: no music".into()],
            )],
        }),
        ..Default::default()
    });
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.3);
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "Imported 2 songs; 1 warning."));
    assert!(texts.iter().any(|t| t == "Imported with problems"));
    assert!(texts.iter().any(|t| t == "Pack/Song: no music"));
    h.tap(PointerKind::Mouse, "Close");
    h.run(0.3);
    assert_eq!(top(&h), "select");
    // Back on the wheel: Enter plays, it does not open the dialog again.
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(top(&h), "select");
    assert!(
        _s.borrow().status.starts_with("PLAYED"),
        "{}",
        _s.borrow().status
    );
}

#[test]
fn removing_asks_first() {
    let (mut h, s, calls, _) = shelled(library(), Some("u-one"));
    assert_eq!(s.borrow().song, "u-one");
    // A song imported: Manage, then Remove song, confirmed.
    assert!(h.ui.bounds("Link.Manage").is_some());
    h.tap(PointerKind::Mouse, "Link.Manage");
    h.run(0.3);
    assert_eq!(top(&h), "manage");
    h.tap(PointerKind::Mouse, "RemoveSong");
    h.run(0.2);
    assert!(h.texts().iter().any(|t| t == "Remove “u-one”?"));
    h.tap(PointerKind::Mouse, "Cancel");
    h.run(0.2);
    assert_eq!(top(&h), "manage", "Keep goes back a step");
    h.tap(PointerKind::Mouse, "RemoveSong");
    h.tap(PointerKind::Mouse, "Confirm");
    h.run(0.3);
    assert!(
        calls
            .borrow()
            .contains(&SelectCall::RemoveSong("u-one".into()))
    );
    assert_eq!(top(&h), "select");
    // The pack, from its header with Delete: read again, or removed.
    h.key("ArrowUp", "ArrowUp");
    h.run(0.3);
    assert_eq!(s.borrow().header.as_deref(), Some("My Pack"));
    h.key("Delete", "Delete");
    h.run(0.3);
    assert_eq!(top(&h), "manage");
    h.tap(PointerKind::Mouse, "Reread");
    h.run(0.3);
    assert!(
        calls
            .borrow()
            .contains(&SelectCall::Reread("My Pack".into()))
    );
    h.key("Delete", "Delete");
    h.run(0.3);
    h.tap(PointerKind::Mouse, "RemovePack");
    h.tap(PointerKind::Mouse, "Confirm");
    h.run(0.3);
    assert!(
        calls
            .borrow()
            .contains(&SelectCall::RemovePack("My Pack".into()))
    );
    // Bundled songs: nothing to manage.
    let (h, _, _, _) = shelled(library(), Some("some-things-must"));
    assert!(h.ui.bounds("Link.Manage").is_none());
}

/// Typing a search on 5,000 songs (`cargo test --release -p ddi-ui
/// typing_on_a_big_library -- --ignored --nocapture`).
#[test]
#[ignore]
fn typing_on_a_big_library() {
    let mut d = library();
    let base = d.songs.clone();
    let words = [
        "sakura", "night", "dream", "flower", "star", "rain", "fire", "ocean",
    ];
    for i in 0..5000 {
        let mut s = base[i % base.len()].clone();
        s.id = format!("big-{i}");
        s.title = format!("{} {} {i}", words[i % 8], words[(i / 8) % 8]);
        s.pack = format!("Pack {}", i / 100);
        d.songs.push(s);
    }
    let (mut h, s, _, _) = shelled(d, Some("some-things-must"));
    h.key("Slash", "/");
    h.run(0.1);
    let mut text = String::new();
    for c in "sakura nig".chars() {
        text.push(c);
        let t = std::time::Instant::now();
        h.sync(&text, [text.len(), text.len()], None);
        let input = t.elapsed();
        h.frame(1.0 / 60.0);
        println!(
            "{text:12} input {:6.2} ms, +frame {:6.2} ms, {} matches",
            input.as_secs_f64() * 1000.0,
            t.elapsed().as_secs_f64() * 1000.0,
            s.borrow().matches
        );
    }
}

#[test]
fn transliterations_and_credits_are_searched() {
    let mut d = library();
    d.songs.push(Song {
        id: "u-kakumei".into(),
        title: "革命".into(),
        title_translit: "Kakumei".into(),
        artist: "木星".into(),
        artist_translit: "Mokusei".into(),
        credit: "Ponpokolin Steps".into(),
        pack: "My Pack".into(),
        bpm: 150.0,
        bpm_text: "150".into(),
        length: 90.0,
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Hard,
            meter: 9,
            ..Default::default()
        }],
        removable: true,
        ..Default::default()
    });
    for q in ["kakumei", "mokusei", "ponpokolin"] {
        let (mut h, s, _, _) = shelled(d.clone(), Some("some-things-must"));
        h.key("Slash", "/");
        h.sync(q, [q.len(), q.len()], None);
        h.run(0.2);
        assert_eq!(s.borrow().matches, 1, "{q}");
        assert_eq!(s.borrow().song, "革命", "{q}");
    }
}

#[test]
fn a_removed_song_gives_way_to_its_neighbour() {
    let mut d = library();
    for (i, id) in ["u-three", "u-four"].into_iter().enumerate() {
        let mut s = d.songs.iter().find(|s| s.id == "u-one").unwrap().clone();
        s.id = id.into();
        s.title = id.into();
        s.pack = if i == 0 {
            "My Pack".into()
        } else {
            "Other".into()
        };
        d.songs.push(s);
    }
    d.packs.push(PackEntry {
        name: "Other".into(),
        removable: true,
        rereadable: false,
        banner: None,
    });
    let (mut h, s, _, feed) = shelled(d.clone(), Some("u-two"));
    assert_eq!(s.borrow().song, "u-two");
    // u-two removed: u-three takes its place in its pack.
    let mut d2 = d.clone();
    d2.songs.retain(|s| s.id != "u-two");
    feed.borrow_mut().data = Some(d2.clone());
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.3);
    assert_eq!(s.borrow().song, "u-three");
    // The whole pack removed: the next pack's first song.
    let mut d3 = d2.clone();
    d3.songs.retain(|s| s.pack != "My Pack");
    d3.packs.retain(|p| p.name != "My Pack");
    feed.borrow_mut().data = Some(d3);
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.3);
    assert_eq!(s.borrow().song, "u-four");
}

#[test]
fn unreadable_songs_are_removed_only_when_confirmed() {
    let mut d = library();
    d.notices = vec![Notice {
        text: "3 stored songs could not be read.".into(),
        link: Some(("remove them".into(), "remove-unreadable".into())),
        error: true,
    }];
    let (mut h, _s, calls, _) = shelled(d, Some("some-things-must"));
    h.tap(PointerKind::Mouse, "Notice.Link0");
    h.run(0.3);
    assert_eq!(top(&h), "manage");
    assert!(
        !calls
            .borrow()
            .iter()
            .any(|c| matches!(c, SelectCall::Notice(_)))
    );
    assert!(
        h.texts()
            .iter()
            .any(|t| t == "Remove 3 stored songs that could not be read?")
    );
    h.tap(PointerKind::Mouse, "Confirm");
    h.run(0.3);
    assert!(
        calls
            .borrow()
            .contains(&SelectCall::Notice("remove-unreadable".into()))
    );
    // The line in full, in a dialog.
    h.tap(PointerKind::Mouse, "Notice");
    h.run(0.3);
    assert_eq!(top(&h), "notices");
    h.tap(PointerKind::Mouse, "Link0");
    h.run(0.3);
    assert_eq!(top(&h), "manage", "its link, as on the line");
}

#[test]
fn nothing_is_removed_while_an_import_runs() {
    let mut d = library();
    d.import.running = Some("reading 12 files…".into());
    let (mut h, s, calls, _) = shelled(d, Some("u-one"));
    h.key("Delete", "Delete");
    h.run(0.3);
    assert_eq!(top(&h), "manage");
    assert!(h.ui.bounds("RemoveSong").is_none());
    assert!(h.ui.bounds("RemovePack").is_none());
    assert!(h.ui.bounds("Reread").is_none());
    h.tap(PointerKind::Mouse, "Cancel");
    h.run(0.3);
    assert_eq!(top(&h), "select");
    assert!(!calls.borrow().iter().any(|c| matches!(
        c,
        SelectCall::RemoveSong(_) | SelectCall::RemovePack(_) | SelectCall::Reread(_)
    )));
    // ⌘ Backspace (Ctrl elsewhere) manages too; Backspace alone is Back.
    h.key_with(
        "Backspace",
        "Backspace",
        crate::Modifiers {
            meta: true,
            ..Default::default()
        },
    );
    h.run(0.3);
    assert_eq!(top(&h), "manage");
    h.key("Escape", "Escape");
    h.run(0.3);
    assert_eq!(top(&h), "select");
    h.key_with(
        "Backspace",
        "Backspace",
        crate::Modifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    h.run(0.3);
    assert_eq!(top(&h), "manage");
    h.key("Escape", "Escape");
    h.run(0.3);
    // Backspace asks the shell to leave, preserving the selected song.
    assert!(s.borrow().header.is_none());
    h.key("Backspace", "Backspace");
    h.run(0.3);
    assert_eq!(top(&h), "select");
    assert!(s.borrow().header.is_none());
}

#[test]
fn files_dropped_open_the_import_dialog() {
    let (mut h, _s, _, feed) = shelled(library(), Some("some-things-must"));
    feed.borrow_mut().open_import = true;
    feed.borrow_mut().import = Some(ImportView {
        running: Some("reading 3 files…".into()),
        ..Default::default()
    });
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.3);
    assert_eq!(top(&h), "import");
    assert!(h.texts().iter().any(|t| t == "reading 3 files…"));
    // Again while it is open: still one.
    feed.borrow_mut().open_import = true;
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.3);
    assert_eq!(
        h.ui.layers().iter().filter(|l| l.name == "import").count(),
        1
    );
}

#[test]
fn named_charts_go_by_their_names() {
    let mut d = library();
    let song = d.songs.iter_mut().find(|s| s.id == "u-one").unwrap();
    song.charts = vec![
        ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Edit,
            meter: 0,
            name: "Hyper Edit".into(),
            index: 4,
            ..Default::default()
        },
        ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Hard,
            meter: 9,
            index: 1,
            ..Default::default()
        },
        ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Edit,
            meter: 12,
            name: "Stamina Edit".into(),
            index: 5,
            ..Default::default()
        },
    ];
    let env = env();
    let state = env.state.clone();
    let mut h = Harness::new(DESIGN, COMPACT, 1.0, TextMode::Mirror);
    h.ui.set_design(COMPACT);
    h.push(SelectScreen::new(
        env,
        d.clone(),
        Some("u-one"),
        Rc::new(|_, _, _: &Env, _| {}),
        Rc::new(|_, _| {}),
    ));
    h.run(0.5);
    double_arrow(&mut h, "ArrowRight");
    h.run(0.3);
    assert_eq!(state.borrow().chart.as_deref(), Some("Edit 0"));
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "PLAY HYPER EDIT"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Hyper Edit"), "{texts:?}");
    // A named chart with a meter: its button shows the meter.
    assert!(texts.iter().any(|t| t == "12"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Stamina Edit"), "{texts:?}");
    // On the full layout's ladder (the theme counts it on the wheel's
    // row: `ddi-theme`'s tests).
    let (h, _, _, _) = shelled(d, Some("u-one"));
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "Hyper Edit"), "{texts:?}");
}

/// Desktop's first start: the shell's offer of other games' song folders
/// opens over song select through the feed; "Add them" answers yes, and
/// the dialog goes.
#[test]
fn the_shells_folders_offer_opens_over_song_select() {
    use super::folders_offer::{AnswerFn, FoldersOfferDialog};
    let (mut h, _state, _calls, feed) = shelled(library(), None);
    let answers: Rc<RefCell<Vec<bool>>> = Rc::default();
    let a = answers.clone();
    feed.borrow_mut().overlay = Some(Box::new(move |env: &Env, cx: &mut crate::Cx| {
        let on: AnswerFn = Rc::new(move |add, _| a.borrow_mut().push(add));
        cx.overlay(
            FoldersOfferDialog::new(
                env.clone(),
                vec!["/Users/me/Library/Application Support/StepMania 5".into()],
                on,
            ),
            true,
        );
    }));
    h.ui.broadcast(crate::message::Message::new(FEED));
    h.run(0.5);
    let texts = h.texts();
    assert!(texts.iter().any(|t| t.contains("StepMania 5")), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Add it"), "{texts:?}");
    h.tap(PointerKind::Mouse, "Add");
    h.run(0.5);
    assert_eq!(*answers.borrow(), vec![true]);
    assert!(!h.texts().iter().any(|t| t == "Add it"));
}

#[test]
fn the_autoplay_switch_is_shown_and_told() {
    let (mut h, state, calls, _) = shelled(library(), None);
    assert!(!state.borrow().autoplay);
    assert!(
        h.texts().iter().any(|t| t == "Autoplay off"),
        "{:?}",
        h.texts()
    );
    // A tap turns it on: the shell is told, the switch says so.
    h.tap(PointerKind::Mouse, "Autoplay");
    h.run(0.1);
    assert!(state.borrow().autoplay);
    assert!(calls.borrow().contains(&SelectCall::Autoplay(true)));
    assert!(
        h.texts().iter().any(|t| t == "AUTOPLAY ON"),
        "{:?}",
        h.texts()
    );
    // The tap left the focus on it: Start turns it off.
    assert_eq!(h.ui.focused(), Some("Autoplay"));
    h.key("Enter", "Enter");
    assert!(!state.borrow().autoplay);
    assert_eq!(calls.borrow().last(), Some(&SelectCall::Autoplay(false)));
}

#[test]
fn the_autoplay_switch_on_a_phone() {
    let mut d = library();
    d.autoplay = true;
    let env = env();
    let state = env.state.clone();
    let mut h = Harness::new(DESIGN, COMPACT, 1.0, TextMode::Mirror);
    h.ui.set_design(COMPACT);
    h.push(SelectScreen::new(
        env,
        d,
        Some("some-things-must"),
        Rc::new(|_, _, _: &Env, _| {}),
        Rc::new(|_, _| {}),
    ));
    h.run(0.5);
    assert!(state.borrow().autoplay);
    assert!(
        h.texts().iter().any(|t| t.starts_with("AUTOPLAY ")),
        "{:?}",
        h.texts()
    );
    assert!(h.ui.bounds("AutoIcon").is_some());
    h.tap(PointerKind::Touch, "AutoIcon");
    h.run(0.3);
    assert!(!state.borrow().autoplay);
    assert!(
        h.texts().iter().any(|t| t.starts_with("PLAY ")),
        "{:?}",
        h.texts()
    );
}

#[test]
fn the_import_dialog_moves_below_the_notices_arriving_after_it() {
    use super::notices::{AppNotice, AppNotices, NoticeBar, NoticesFeed};
    let (mut h, _s, _calls, _feed) = shelled(library(), None);
    h.tap(PointerKind::Mouse, "Link.Add");
    h.run(0.5);
    assert_eq!(top(&h), "import");
    let before = h.ui.bounds("Panel").unwrap();
    let feed: NoticesFeed = Rc::new(RefCell::new(AppNotices {
        banner: Some(AppNotice {
            text: "New audio path: 48 kHz. No calibration for it yet.".into(),
            actions: vec![("later".into(), "later".into(), false)],
        }),
        toast: None,
    }));
    h.ui.push_chrome(NoticeBar::new(env(), feed, Rc::new(|_, _| {})));
    h.run(0.3);
    let plate = h.ui.bounds("AppNotice.Plate").unwrap();
    let after = h.ui.bounds("Panel").unwrap();
    assert!(after[1] >= plate[3] - 0.5, "{after:?} under {plate:?}");
    assert_ne!(before, after);
    assert_eq!(top(&h), "import");
}

#[test]
fn the_devices_line_lets_taps_through_to_the_footer() {
    use super::notices::{AppNotices, NoticeBar, NoticesFeed};
    let (mut h, _s, calls, _feed) = shelled(library(), None);
    let feed: NoticesFeed = Rc::new(RefCell::new(AppNotices {
        banner: None,
        toast: Some(format!(
            "devices: {} (+0 ms)",
            "a very long device name ".repeat(8)
        )),
    }));
    h.ui.push_chrome(NoticeBar::new(env(), feed, Rc::new(|_, _| {})));
    h.run(0.3);
    let toast = h.ui.bounds("AppToast.Frame").unwrap();
    let l = h.ui.bounds("Link.Calibrate").unwrap();
    // A point on both: the line's plate over the link's top.
    let link = [
        (l[0] + l[2]) / 2.0,
        (l[1].max(toast[1]) + l[3].min(toast[3])) / 2.0,
    ];
    assert!(
        l[1] < toast[3] && link[0] > toast[0] && link[0] < toast[2],
        "{l:?} under {toast:?}"
    );
    h.tap_at(PointerKind::Mouse, link);
    assert!(
        calls
            .borrow()
            .contains(&SelectCall::Open("calibrate".into())),
        "{:?}",
        calls.borrow()
    );
}

#[test]
fn song_selects_plays_list_watches_opens_and_removes() {
    use super::data::PlayRow;
    use super::select::PlaysFn;
    let mut d = library();
    for c in &mut d
        .songs
        .iter_mut()
        .find(|s| s.id == "sweeteners")
        .unwrap()
        .charts
    {
        c.best = Some(Best {
            grade: "A".into(),
            score: "90.00%".into(),
            lamp: "Clear".into(),
            lamp_text: "CLEAR".into(),
            plays: 2,
            play: "p2".into(),
            ..Default::default()
        });
    }
    let row = |id: &str, best: bool| PlayRow {
        id: id.into(),
        when: format!("2026-10-1{} 10:00", &id[1..]),
        grade: "A".into(),
        score: "90.00%".into(),
        lamp: "Clear".into(),
        lamp_text: "CLEAR".into(),
        ruleset: "ITG".into(),
        mods: "×2".into(),
        best,
        ..Default::default()
    };
    let asked: Rc<RefCell<Vec<String>>> = Rc::default();
    let a = asked.clone();
    let plays: PlaysFn = Rc::new(move |song, chart| {
        a.borrow_mut().push(format!("{} {}", song.id, chart.index));
        vec![row("p3", false), row("p2", true), row("p1", false)]
    });
    let e = env();
    let state = e.state.clone();
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    let play: PlayFn = Rc::new(|_, _, _: &Env, _| {});
    let open: OpenFn = Rc::new(|_: &Env, _| {});
    let calls: Calls = Rc::default();
    let c = calls.clone();
    let shell: ShellFn = Rc::new(move |call, _| c.borrow_mut().push(call));
    let feed: SharedFeed = Rc::new(RefCell::new(Feed::default()));
    h.push(
        SelectScreen::new(e, d, Some("sweeteners"), play, open)
            .with_shell(shell, feed)
            .with_plays(plays),
    );
    h.run(0.5);
    assert!(state.borrow().hints.iter().any(|t| t.contains("Plays")));
    let watched = |calls: &Calls| {
        calls
            .borrow()
            .iter()
            .filter_map(|c| match c {
                SelectCall::WatchPlay(id) => Some(format!("watch {id}")),
                SelectCall::ShowPlay(id) => Some(format!("show {id}")),
                SelectCall::RemovePlay(id) => Some(format!("remove {id}")),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // P: the chart's plays, the best chosen, Start watches it.
    h.key("KeyP", "p");
    h.run(0.3);
    assert_eq!(state.borrow().plays.len(), 3);
    assert_eq!(state.borrow().plays_selected.as_deref(), Some("p2"));
    assert!(state.borrow().plays[0].starts_with("p3|2026-10-13 10:00|A 90.00% CLEAR|×2"));
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(watched(&calls), ["watch p2"]);
    assert!(state.borrow().plays.is_empty(), "closed");
    // A row tapped is chosen; Results opens it.
    h.key("KeyP", "p");
    h.run(0.3);
    h.tap(PointerKind::Mouse, "Row0");
    h.run(0.2);
    assert_eq!(state.borrow().plays_selected.as_deref(), Some("p3"));
    h.tap(PointerKind::Mouse, "Open");
    h.run(0.3);
    assert_eq!(watched(&calls), ["watch p2", "show p3"]);
    // Remove takes two presses; the list goes on without it.
    h.key("KeyP", "p");
    h.run(0.3);
    h.tap(PointerKind::Mouse, "Row2");
    h.run(0.2);
    h.tap(PointerKind::Mouse, "Remove");
    h.run(0.2);
    assert_eq!(watched(&calls).len(), 2);
    // Armed, then focus moved away by keys: disarmed.
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.2);
    h.tap(PointerKind::Mouse, "Remove");
    h.run(0.2);
    assert_eq!(
        watched(&calls).len(),
        2,
        "one press after the move only arms it"
    );
    h.tap(PointerKind::Mouse, "Remove");
    h.run(0.2);
    assert_eq!(watched(&calls), ["watch p2", "show p3", "remove p1"]);
    assert_eq!(state.borrow().plays.len(), 2);
    h.key("Escape", "Escape");
    h.run(0.3);
    assert!(state.borrow().plays.is_empty());
    // W: the best's replay at once.
    h.key("KeyW", "w");
    assert_eq!(watched(&calls).len(), 4);
    assert_eq!(watched(&calls).last().map(String::as_str), Some("watch p2"));
    // P opens the list again.
    h.key("KeyP", "p");
    h.run(0.3);
    assert_eq!(state.borrow().plays.len(), 3);
    assert!(asked.borrow().iter().all(|a| a.starts_with("sweeteners ")));
}

#[test]
fn a_long_press_on_a_phones_chart_opens_its_plays() {
    use super::data::PlayRow;
    use super::select::PlaysFn;
    let plays: PlaysFn = Rc::new(|_, chart| {
        vec![PlayRow {
            id: format!("p{}", chart.index),
            when: "2026-10-16 10:00".into(),
            ..Default::default()
        }]
    });
    let e = env();
    let state = e.state.clone();
    let mut h = Harness::new(COMPACT, [844.0, 390.0], 1.0, TextMode::Mirror);
    let play: PlayFn = Rc::new(|_, _, _: &Env, _| {});
    let open: OpenFn = Rc::new(|_: &Env, _| {});
    let shell: ShellFn = Rc::new(|_, _| {});
    let feed: SharedFeed = Rc::new(RefCell::new(Feed::default()));
    h.push(
        SelectScreen::new(e, library(), Some("sweeteners"), play, open)
            .with_shell(shell, feed)
            .with_plays(plays),
    );
    h.run(0.5);
    assert!(state.borrow().compact);
    let at = h.centre("Chart1");
    h.pointer(1, PointerKind::Touch, crate::PointerPhase::Down, at);
    h.run(1.0);
    h.pointer(1, PointerKind::Touch, crate::PointerPhase::Up, at);
    h.run(0.3);
    assert_eq!(state.borrow().plays.len(), 1, "{:?}", state.borrow().status);
    // The chart pressed is the one chosen, and its plays the list's.
    let chart = state.borrow().chart.clone().unwrap();
    assert!(state.borrow().plays[0].starts_with('p'), "{chart}");
}

#[test]
fn saved_selection_waits_for_the_initial_library_scan() {
    let mut full = library();
    full.resume = Some(("u-two".into(), Some(3)));
    full.style = Some("dance-double".into());
    full.closed = full
        .songs
        .iter()
        .map(|s| s.pack.clone())
        .filter(|p| p != "My Pack")
        .collect();
    let mut initial = full.clone();
    initial.songs.retain(|s| !s.removable);
    initial.loading = true;
    let (mut h, state, calls, feed) = shelled(initial, None);
    assert!(
        !calls
            .borrow()
            .iter()
            .any(|c| matches!(c, SelectCall::Place(_))),
        "temporary initial selection must not overwrite the saved one"
    );
    feed.borrow_mut().data = Some(full);
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.5);
    assert_eq!(state.borrow().song, "u-two");
    assert_eq!(last_place(&calls).song.as_deref(), Some("u-two"));
    assert_eq!(last_place(&calls).chart, Some(3));
    assert_eq!(last_place(&calls).open.as_deref(), Some("My Pack"));
}

#[test]
fn saved_selection_does_not_reopen_a_collapsed_pack() {
    let mut data = library();
    data.closed = data.songs.iter().map(|s| s.pack.clone()).collect();
    data.resume = Some(("u-two".into(), Some(3)));
    let (_h, state, calls, _) = shelled(data, None);
    assert_eq!(
        state.borrow().items,
        7,
        "all seven packs have only their header"
    );
    assert!(last_place(&calls).open.is_none());
}

#[test]
fn missing_saved_song_settles_when_the_scan_finishes() {
    let mut data = library();
    data.resume = Some(("gone".into(), None));
    data.loading = true;
    let (mut h, _, calls, feed) = shelled(data.clone(), None);
    assert!(
        !calls
            .borrow()
            .iter()
            .any(|c| matches!(c, SelectCall::Place(_)))
    );
    data.loading = false;
    feed.borrow_mut().data = Some(data);
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.5);
    assert!(
        calls
            .borrow()
            .iter()
            .any(|c| matches!(c, SelectCall::Place(_)))
    );
}

#[test]
fn saved_open_pack_waits_for_its_header_even_without_a_song() {
    let mut full = library();
    full.resume_pack = Some("My Pack".into());
    full.closed = full
        .songs
        .iter()
        .map(|s| s.pack.clone())
        .filter(|p| p != "My Pack")
        .collect();
    let mut initial = full.clone();
    initial.songs.retain(|s| !s.removable);
    initial.loading = true;
    let (mut h, _, calls, feed) = shelled(initial, None);
    assert!(
        !calls
            .borrow()
            .iter()
            .any(|c| matches!(c, SelectCall::Place(_)))
    );
    feed.borrow_mut().data = Some(full);
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.5);
    assert_eq!(last_place(&calls).open.as_deref(), Some("My Pack"));
}

#[test]
fn saved_chart_is_restored_before_the_first_frame_and_survives_resize() {
    let mut data = library();
    data.resume = Some(("some-things-must".into(), Some(0)));
    let (mut h, _, calls, _) = shelled(data, None);
    assert_eq!(last_place(&calls).song.as_deref(), Some("some-things-must"));
    assert_eq!(last_place(&calls).chart, Some(0));
    h.send(crate::UiEvent::Resize {
        size: [844.0, 390.0],
        ratio: 1.0,
    });
    h.run(0.3);
    assert_eq!(last_place(&calls).chart, Some(0));
}

#[test]
fn refreshing_pack_art_keeps_the_selected_header_and_only_one_pack_open() {
    let data = library();
    let (mut h, state, _, feed) = shelled(data.clone(), Some("u-one"));
    h.key("ArrowUp", "ArrowUp");
    h.run(0.3);
    assert_eq!(state.borrow().header.as_deref(), Some("My Pack"));
    feed.borrow_mut().data = Some(data);
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.5);
    assert_eq!(state.borrow().header.as_deref(), Some("My Pack"));
    assert_eq!(state.borrow().closed.len(), 6);
}

#[test]
fn quick_opposite_pairs_do_not_publish_temporary_song_changes() {
    for pad in [false, true] {
        let (mut h, s, calls, _) = shelled(library(), Some("some-things-must"));
        let song = s.borrow().song.clone();
        let preferred = s.borrow().preferred.clone();
        let before = calls
            .borrow()
            .iter()
            .filter(|c| matches!(c, SelectCall::Selected { .. }))
            .count();
        for key in ["ArrowUp", "ArrowUp", "ArrowDown", "ArrowDown"] {
            if pad {
                let a = if key == "ArrowUp" {
                    crate::MenuAction::Up
                } else {
                    crate::MenuAction::Down
                };
                h.pad(a, true);
                h.pad(a, false);
            } else {
                h.key(key, key);
            }
            h.run(0.06);
            assert_eq!(s.borrow().song, song);
        }
        h.run(0.5);
        assert_eq!(s.borrow().song, song);
        assert_eq!(s.borrow().preferred, preferred);
        assert_eq!(
            calls
                .borrow()
                .iter()
                .filter(|c| matches!(c, SelectCall::Selected { .. }))
                .count(),
            before
        );
    }
}

#[test]
fn blur_and_overlays_stop_a_held_song_navigation() {
    for blur in [false, true] {
        let (mut h, s) = select([1280.0, 720.0], DESIGN, false);
        h.key_down("ArrowDown", "ArrowDown");
        h.run(0.45);
        if blur {
            h.send(crate::UiEvent::Focus(false));
        } else {
            h.key("Slash", "/");
        }
        let song = s.borrow().song.clone();
        h.run(1.0);
        assert_eq!(s.borrow().song, song);
    }
}

fn pad_press(h: &mut Harness, action: crate::MenuAction) {
    h.pad(action, true);
    h.pad(action, false);
    h.run(0.6);
}

#[test]
fn a_pad_reaches_the_search_field_and_leaves_it_where_the_list_was() {
    use crate::MenuAction::{Back, Down, Left, Right, Start, Up};
    // In a browser: a controller cannot start typing.
    let (mut h, s, calls, _) = shelled(library(), Some("some-things-must"));
    for _ in 0..3 {
        pad_press(&mut h, Up);
    }
    assert_eq!(s.borrow().header.as_deref(), Some("Bundled songs"));
    assert_eq!(h.ui.focused(), Some("Wheel"));
    // A fresh Up on the first row: the field.
    pad_press(&mut h, Up);
    assert_eq!(h.ui.focused(), Some("Search"));
    // Down goes back to the list as it was (no move).
    pad_press(&mut h, Down);
    assert_eq!(h.ui.focused(), Some("Wheel"));
    assert_eq!(s.borrow().header.as_deref(), Some("Bundled songs"));
    // Back does too, and stays on song select.
    pad_press(&mut h, Up);
    assert_eq!(h.ui.focused(), Some("Search"));
    pad_press(&mut h, Back);
    assert_eq!(h.ui.focused(), Some("Wheel"));
    assert_eq!(s.borrow().header.as_deref(), Some("Bundled songs"));
    assert!(!calls.borrow().contains(&SelectCall::Leave));
    // Start on the field asks for a key or a tap; the pack stays open.
    pad_press(&mut h, Up);
    let items = s.borrow().items;
    pad_press(&mut h, Start);
    assert!(h.ui.text_field().is_none());
    assert_eq!(h.ui.focused(), Some("Search"));
    assert_eq!(s.borrow().items, items, "the header was not toggled");
    assert!(
        s.borrow()
            .hints
            .iter()
            .any(|t| t.contains("Press / or tap the field to type.")),
        "{:?}",
        s.borrow().hints
    );
    // Along the header and back; Back from the list then leaves.
    pad_press(&mut h, Right);
    assert_eq!(h.ui.focused(), Some("Mod0"));
    pad_press(&mut h, Left);
    assert_eq!(h.ui.focused(), Some("Search"));
    pad_press(&mut h, Down);
    pad_press(&mut h, Back);
    assert!(calls.borrow().contains(&SelectCall::Leave));
}

#[test]
fn a_pad_types_searches_and_clears_where_typing_needs_no_gesture() {
    use crate::MenuAction::{Back, Down, Right, Start, Up};
    // Desktop: Start on the field types (on a keyboard).
    let (mut h, s, _, _) = shelled_with(library(), Some("some-things-must"), false);
    for _ in 0..4 {
        pad_press(&mut h, Up);
    }
    assert_eq!(h.ui.focused(), Some("Search"));
    pad_press(&mut h, Start);
    assert!(h.ui.text_field().is_some());
    h.sync("Swe", [3, 3], None);
    h.run(0.2);
    assert!(s.borrow().popup);
    // Back closes the popup, then stops typing; the filter stays.
    pad_press(&mut h, Back);
    assert!(!s.borrow().popup);
    assert!(h.ui.text_field().is_some());
    pad_press(&mut h, Back);
    assert!(h.ui.text_field().is_none());
    assert_eq!(h.ui.focused(), Some("Wheel"));
    assert_eq!(s.borrow().search, "Swe");
    assert_eq!(s.borrow().song, "Sweeteners");
    // Start submits as Enter does: to the songs found.
    for _ in 0..3 {
        pad_press(&mut h, Up);
    }
    assert_eq!(h.ui.focused(), Some("Search"));
    pad_press(&mut h, Start);
    assert!(h.ui.text_field().is_some());
    pad_press(&mut h, Start);
    assert!(h.ui.text_field().is_none());
    assert_eq!(h.ui.focused(), Some("Wheel"));
    assert_eq!(s.borrow().song, "Sweeteners");
    // Down while typing leaves for the list where it was.
    let song = s.borrow().song.clone();
    for _ in 0..3 {
        pad_press(&mut h, Up);
    }
    assert_eq!(h.ui.focused(), Some("Search"));
    let header_now = s.borrow().header.clone();
    pad_press(&mut h, Start);
    pad_press(&mut h, Down);
    assert!(h.ui.text_field().is_none());
    assert_eq!(h.ui.focused(), Some("Wheel"));
    assert_eq!(s.borrow().header, header_now);
    // The field's Clear is next along the header while there is text.
    pad_press(&mut h, Up);
    assert_eq!(h.ui.focused(), Some("Search"));
    pad_press(&mut h, Right);
    assert_eq!(h.ui.focused(), Some("ClearSearch"));
    pad_press(&mut h, Start);
    assert_eq!(s.borrow().search, "");
    assert_eq!(h.ui.focused(), Some("Wheel"));
    assert_eq!(s.borrow().song, song);
}

#[test]
fn a_held_up_stops_on_the_first_row_and_a_fresh_one_goes_on_to_the_field() {
    use crate::MenuAction::Up;
    let (mut h, s, _, _) = shelled(library(), Some("some-things-must"));
    h.pad(Up, true);
    h.run(2.0);
    assert_eq!(h.ui.focused(), Some("Wheel"));
    assert_eq!(s.borrow().header.as_deref(), Some("Bundled songs"));
    h.pad(Up, false);
    h.run(0.6);
    assert_eq!(h.ui.focused(), Some("Wheel"));
    pad_press(&mut h, Up);
    assert_eq!(h.ui.focused(), Some("Search"));
}
