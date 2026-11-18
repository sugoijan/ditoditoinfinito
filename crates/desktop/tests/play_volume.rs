//! The music's volume on the start prompt and during a play (round AB,
//! 2026-11-19), through the window's own mapping (`Shell::play_pointer`)
//! on a fake clock with no sound device, the window at a scale factor of
//! 2: muted as song select left it, the prompt's card says so beside its
//! control in the corner; during the play the control shows with the
//! quit button after the mouse moved, a press opens its slider, a press
//! along it sets the level (the play's sound and the settings follow),
//! the press is never the play's (its log and judging the same), and it
//! goes with the quit button. Pictures under `DDI_OFFSCREEN_PPM`.
//!
//! Without a GPU (CI) the play runs undrawn (`common::rig::Draws`): the
//! control's places are then given by the test, so the mapping is
//! checked all the same.

mod common;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::play::Request;
use ddi_desktop::shell::Shell;
use ddi_ui::event::{PointerPhase, UiEvent};
use ddi_ui::state::settings::Settings;

const LOGICAL: [f32; 2] = [1280.0, 720.0];
const SCALE: f32 = 2.0;
const PHYSICAL: [u32; 2] = [2560, 1440];
const STEP: u64 = 1_000_000_000 / 30;

#[test]
fn the_music_volume_on_the_prompt_and_beside_the_quit_button() {
    common::indexes();
    ddi_desktop::audio::set_headless(true);
    let home = std::env::temp_dir().join(format!("ddi-play-volume-{}", std::process::id()));
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", &home) };
    let mut shell = Shell::named_at("select", Some(0.0));
    let env = shell.theme.clone().expect("the theme").0;
    shell.ui.handle(UiEvent::Resize {
        size: LOGICAL,
        ratio: SCALE,
    });
    shell.ui.update(0.5);
    // Muted on song select (the quick volume's mute is the setting at 0);
    // no hand and no announcer, so the play is the play alone.
    Settings {
        volume: 0.0,
        dancing_hand: false,
        announcer: None,
        ..Settings::load()
    }
    .save();
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    shell.play.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    shell.play.scale = SCALE;
    let draws = common::rig::Draws::here(&mut shell.play, PHYSICAL);
    *shell.play.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    draws.wait_prompt(&mut shell.play, &mut shell.ui, &env);
    shell.ui.update(0.5);
    // The prompt: the control in the card's corner, muted, saying so.
    let view = env.state.borrow().prompt.clone().expect("the prompt");
    assert_eq!((view.volume, view.music_muted), (Some(0), true));
    let card = shell.ui.bounds("Card").expect("the card");
    let button = shell.ui.bounds("Volume.Button").expect("the volume");
    assert!(button[2] <= card[2] && button[2] > card[2] - 40.0, "{button:?} {card:?}");
    assert!(shell.ui.actor("MusicMuted").is_some());
    let mut pictures = common::gpu::Pictures::new();
    pictures.shot(&mut shell.ui, PHYSICAL, "ab-desktop-prompt-muted.png");
    // Unmuted there by a click (the mouse over it first, as a person's):
    // kept, and the play starts with it.
    let v = shell.ui.viewport();
    let ph = v.to_physical([(button[0] + button[2]) / 2.0, (button[1] + button[3]) / 2.0]);
    let over = [ph[0] / v.ratio, ph[1] / v.ratio];
    shell.pointer(0, ddi_ui::PointerKind::Mouse, PointerPhase::Move, over);
    shell.ui.update(0.05);
    common::ui::tap(&mut shell.ui, "Volume.Button", 0.1);
    let unmuted = Settings::load().volume;
    assert!(unmuted > 0.0, "unmuted on the prompt");
    assert!(!env.state.borrow().prompt.clone().unwrap().music_muted);
    pictures.shot(&mut shell.ui, PHYSICAL, "ab-desktop-prompt-unmuted.png");
    common::ui::key(&mut shell.ui, "Enter", "Enter");
    shell.ui.update(0.05);
    draws.poll(&mut shell.play, &mut shell.ui, &env);
    assert!(shell.play.playing(), "{:?}", shell.ui.layers());
    let frame = |shell: &mut Shell| {
        let now = clock.fetch_add(STEP, Ordering::SeqCst) + STEP;
        shell.play.scale = SCALE;
        draws.frame(&mut shell.play, &mut shell.ui, &env, now);
    };
    for _ in 0..30 {
        frame(&mut shell);
    }
    let volume = |shell: &Shell| shell.play.running().expect("playing").session.volume();
    assert_eq!(volume(&shell), unmuted, "the play plays at the prompt's level");
    // Nothing shows before the mouse moves.
    assert!(shell.play.feed.borrow().volume_at.is_none());
    // The mouse moved: the quit button and the volume beside it.
    shell.play.feed.borrow_mut().pointer_at = Some(std::time::Instant::now());
    frame(&mut shell);
    if !draws.drawn() {
        shell.play.feed.borrow_mut().volume_at = Some(ddi_shell::play_volume::VolumeAt {
            button: [250.0, 82.0, 22.0],
            plate: None,
        });
    }
    let at = shell.play.feed.borrow().volume_at.expect("shown with the quit button");
    if draws.drawn() {
        let quit = shell.play.feed.borrow().quit_at.expect("the quit button");
        assert_eq!(at.button[0], quit[0], "beside the quit button: {at:?} {quit:?}");
        assert!((at.button[1] - quit[1]).abs() >= 2.0 * quit[2], "{at:?} {quit:?}");
        assert!(at.button[1] < LOGICAL[1] && at.button[0] < LOGICAL[0] / 2.0, "logical: {at:?}");
    }
    // A press on it: the slider opens; the play heard nothing.
    let b = [at.button[0], at.button[1]];
    assert!(shell.play_pointer(PointerPhase::Down, b));
    assert!(shell.play_pointer(PointerPhase::Up, b));
    frame(&mut shell);
    draws.wait_gpu();
    if !draws.drawn() {
        shell.play.feed.borrow_mut().volume_at = Some(ddi_shell::play_volume::VolumeAt {
            button: [250.0, 82.0, 22.0],
            plate: Some([228.0, 112.0, 272.0, 292.0]),
        });
    }
    let at = shell.play.feed.borrow().volume_at.expect("still shown");
    let plate = at.plate.expect("the slider open");
    // On the screen, along from the button (away from the quit button).
    assert!(plate[1] >= 0.0 && plate[3] <= LOGICAL[1], "{plate:?}");
    assert!(
        plate[3] <= at.button[1] - at.button[2] || plate[1] >= at.button[1] + at.button[2],
        "{plate:?} {at:?}"
    );
    draws.shot("ab-desktop-play-slider.png");
    // A press near the slider's top and a drag to it: the loudest, kept
    // once let go; a press elsewhere is not the volume's.
    let top = [(plate[0] + plate[2]) / 2.0, plate[1] + 30.0];
    assert!(shell.play_pointer(PointerPhase::Down, top));
    assert!(shell.play_pointer(PointerPhase::Move, [top[0], plate[1] - 50.0]));
    frame(&mut shell);
    assert!(shell.play_pointer(PointerPhase::Up, [top[0], plate[1] - 50.0]));
    frame(&mut shell);
    if draws.drawn() {
        assert_eq!(volume(&shell), 1.0);
        assert_eq!(Settings::load().volume, 1.0, "kept");
    }
    assert!(!shell.play_pointer(PointerPhase::Down, [900.0, 600.0]), "elsewhere");
    assert!(!shell.play_pointer(PointerPhase::Up, [900.0, 600.0]));
    frame(&mut shell);
    // The play's log heard none of it: no press of the pointer's.
    let p = shell.play.running().unwrap();
    let log = format!("{:?}", p.session.player.log());
    assert!(!log.contains("Touch"), "{}", &log[..log.len().min(400)]);
    if draws.drawn() {
        assert!(shell.play.feed.borrow().volume_at.is_some_and(|a| a.plate.is_none()));
        // Two seconds later, gone with the quit button.
        shell.play.feed.borrow_mut().pointer_at = std::time::Instant::now().checked_sub(
            std::time::Duration::from_secs_f64(ddi_session::play::QUIT_LINGER * 2.0),
        );
        frame(&mut shell);
        assert!(shell.play.feed.borrow().volume_at.is_none());
        assert!(shell.play.feed.borrow().quit_at.is_none());
    }
    let _ = std::fs::remove_dir_all(&home);
}
