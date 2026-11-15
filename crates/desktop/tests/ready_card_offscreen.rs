//! A room's stage's Ready card (`ready-card-v1`, the feedback round I),
//! as the desktop window draws it, on this machine's GPU offscreen with
//! no sound device: a practice room of 2, 8, 24 and 73 players at
//! 1280×720, 1920×1080, 844×390 and 390×844, in English and Japanese.
//! The card's parts stay inside the window and the player's own line is
//! drawn.
//!
//! Skipped where there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>`
//! keeps pictures (PPM); `DDI_READY_CARD=all` draws every size, count and
//! language (else 73 at 1280×720 and 24 on a phone, in English).

mod common;
use std::rc::Rc;

use ddi_desktop::play::host::Host;
use ddi_room::CpuMode;
use ddi_ui::screens::data::Difficulty;
use common::ui::top;

fn card(size: [u32; 2], players: u16, lang: &str) {
    common::indexes();
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    assert!(ddi_l10n::force(lang));
    let (mut ui, env) = common::ui::theme("start", common::ui::lowercase(), size, true);
    // The design the window would choose (compact on a phone's sizes).
    let css = [size[0] as f32, size[1] as f32];
    ui.set_design(ddi_ui::screens::design_for(&*env.look, css));
    common::ui::resize(&mut ui, size);
    let host = Host::new(Rc::default(), None);
    // A fresh practice room each time (the run keeps one).
    ddi_desktop::room::discard();
    let mut rig = common::rig::Rig::new(&adapter, size, ui, env, host).with_ui_gpu(&adapter);
    let room = common::practice::open(&mut rig.ui, &rig.env, &rig.host);
    let entry = common::practice::song_entry("beatucada", "Beatucada", 4, Difficulty::Medium);
    {
        let mut r = room.borrow_mut();
        let mut rules = ddi_room::MatchRules {
            stages: 3,
            ..r.rules().clone()
        };
        rules.seats = rules.seats.max(players);
        rules.cpu.mode = CpuMode::Fixed { count: players - 1 };
        rules.cpu.opponent.level = 7;
        rules.cpu.opponent.seed = 7;
        r.save_rules(rules).unwrap();
        r.pick(&entry, &entry.charts[0]).unwrap();
    }
    rig.ui.update(0.1);
    rig.wait_prompt();
    for _ in 0..30 {
        rig.ui.update(1.0 / 30.0);
    }
    assert_eq!(top(&rig.ui), "play-prompt");
    // `DDI_READY_CARD_BEFORE=1`: the picture alone (the card before
    // `ready-card-v1`, for comparing).
    if std::env::var_os("DDI_READY_CARD_BEFORE").is_some() {
        let pixels = rig.ui_picture();
        common::shot(
            &format!("ready-{players}-{}x{}-{lang}.ppm", size[0], size[1]),
            &pixels,
            size,
        );
        ddi_l10n::unforce();
        return;
    }
    let vp = rig.ui.viewport().screen;
    let [w, h] = [vp.width, vp.height];
    let inside = |name: &str| {
        let b = rig.ui.bounds(name).unwrap_or_else(|| panic!("{name} drawn"));
        assert!(
            b[0] >= -0.5 && b[1] >= -0.5 && b[2] <= w + 0.5 && b[3] <= h + 0.5,
            "{name} {b:?} in {size:?}"
        );
        b
    };
    let card = inside("Card");
    inside("Start");
    // Most of the screen at a desktop's sizes.
    if size[0] >= 1280 && size[1] >= 720 {
        assert!(card[2] - card[0] >= w * 0.85, "{card:?}");
    }
    // The player's own line among those drawn.
    let mine = (0..usize::from(players))
        .map(|k| format!("Room.Player{k}.Me"))
        .find(|n| rig.ui.actor(n).is_some())
        .expect("the player's own line in view");
    inside(&mine);
    let shown = (0..usize::from(players))
        .filter(|k| rig.ui.actor(&format!("Room.Player{k}.Name")).is_some())
        .count();
    eprintln!("{lang} {}x{} {players}: {shown} players in view", size[0], size[1]);
    if size == [1280, 720] {
        assert!(shown >= usize::from(players).min(24), "{shown}");
    }
    let pixels = rig.ui_picture();
    common::shot(
        &format!("ready-{players}-{}x{}-{lang}.ppm", size[0], size[1]),
        &pixels,
        size,
    );
    ddi_l10n::unforce();
}

#[test]
fn a_ready_card_of_73_at_1280x720() {
    card([1280, 720], 73, "en");
}

#[test]
fn a_ready_card_of_24_on_a_phone_held_upright() {
    card([390, 844], 24, "en");
}

#[test]
#[ignore = "every size, count and language, for the look's review"]
fn ready_cards_pictures() {
    for lang in ["en", "ja"] {
        for size in [[1280, 720], [1920, 1080], [844, 390], [390, 844]] {
            for players in [2, 8, 24, 73] {
                card(size, players, lang);
            }
        }
    }
}
