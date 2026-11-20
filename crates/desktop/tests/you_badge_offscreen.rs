//! The player's own name with its "You" badge (feedback round H, item 1)
//! wherever a practice room lists players, drawn offscreen on this
//! machine's GPU in the game's theme at 1280×720 and 844×390, in English
//! and Japanese: the players panel beside the song, the Players pop-up,
//! the stage's prompt's players and the stage's Standings pop-up. The
//! badge's place against its name is pinned in `ddi-ui`'s tests
//! (`assert_badge_on_line`); here the pictures, and each badge drawn
//! inside the window. Without a GPU adapter (CI) only the
//! pictures (and a play's prompt, which readies its skin on one) are
//! skipped: the rest is checked all the same.
//! `DDI_OFFSCREEN_PPM=<dir>` keeps them (`badge-<place>-<w>x<h>-<lang>.ppm`).

mod common;
use std::cell::RefCell;
use std::rc::Rc;

use ddi_desktop::play::host::Host;
use ddi_room::{Finish, Finisher};
use ddi_ui::screens::data::Difficulty;
use ddi_ui::screens::prompt::{PromptScreen, PromptState};
use common::ui::{frames, key, top};

#[test]
fn the_you_badge_beside_the_name_everywhere() {
    common::indexes();
    let mut pictures = common::gpu::Pictures::new();
    for (size, lang) in [
        ([1280u32, 720u32], "en"),
        ([1280, 720], "ja"),
        ([844, 390], "en"),
        ([844, 390], "ja"),
    ] {
        assert!(ddi_l10n::set_language(lang));
        ddi_desktop::room::discard();
        let (mut ui, env) = common::ui::theme("start", common::ui::lowercase(), size, true);
        // (The window's UI chose the settings' language: this run's.)
        assert!(ddi_l10n::set_language(lang));
        let host = Host::new(Rc::default(), None);
        let room = common::practice::open(&mut ui, &env, &host);
        room.borrow_mut().set_own_name(Some("alice".into()));
        frames(&mut ui, 0.5);
        let mut shot = |ui: &mut ddi_ui::Ui, place: &str| {
            let badge = (0..80)
                .flat_map(|k| {
                    [
                        format!("Players{k}.Name.Badge"),
                        format!("Panel{k}.Name.Badge"),
                        format!("Room.Player{k}.Name.Badge"),
                        format!("Name{k}.Badge"),
                    ]
                })
                .find_map(|n| ui.bounds(&n));
            let name = format!("badge-{place}-{}x{}-{lang}.ppm", size[0], size[1]);
            pictures.shot(ui, size, &name);
            badge
        };
        let inside = |b: Option<[f32; 4]>, place: &str| {
            let b = b.unwrap_or_else(|| panic!("{place} {lang} {size:?}: no badge drawn"));
            assert!(b[0] >= 0.0 && b[1] >= 0.0 && b[2] > b[0], "{place} {lang} {size:?}: {b:?}");
        };
        // The players beside the song (a phone's are in the Players
        // pop-up alone: its strip has no badge).
        let panel = shot(&mut ui, "0-panel");
        if size[0] >= 1000 {
            inside(panel, "panel");
        }
        // The Players pop-up: the wide panel's head (L folds that panel),
        // a phone's L.
        if ui.bounds(ddi_ui::screens::select::PANEL_HEAD).is_some() {
            common::ui::tap(&mut ui, ddi_ui::screens::select::PANEL_HEAD, 0.5);
        } else {
            key(&mut ui, "KeyL", "l");
            frames(&mut ui, 0.5);
        }
        assert_eq!(top(&ui), "room-players");
        let players = shot(&mut ui, "1-players");
        inside(players, "players");
        key(&mut ui, "Escape", "Escape");
        frames(&mut ui, 0.5);
        // The stage's prompt, over the room.
        let entry = common::practice::song_entry("beatucada", "Beatucada", 1, Difficulty::Hard);
        room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
        ui.update(0.1);
        let request = host.requested.borrow_mut().take().expect("the stage asked for");
        let order = request.stage.clone().expect("a stage's");
        // (A window of its own: the room's song select stays as it is.)
        let (mut over, env2) = common::ui::theme("start", common::ui::lowercase(), size, true);
        assert!(ddi_l10n::set_language(lang));
        over.push(
            PromptScreen::new(env2, Rc::new(RefCell::new(PromptState::default())), Rc::new(|_, _| {}))
                .with_room(room.clone(), order.stage, Rc::new(|_, _| {})),
        );
        frames(&mut over, 0.6);
        let prompt = shot(&mut over, "2-prompt");
        inside(prompt, "prompt");
        drop(over);
        // The stage's Standings, popped up on song select.
        let _ = room.borrow_mut().set_ready(true);
        let later = room.borrow().now_ms() + 10_000;
        room.borrow_mut().tick(later);
        let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
        ids.insert(0, order.me);
        let ranking = ids
            .into_iter()
            .map(|participant| {
                vec![Finisher {
                    participant,
                    finish: Finish::Survived,
                }]
            })
            .collect();
        room.borrow_mut().report_stage(order.stage, ranking, false).unwrap();
        frames(&mut ui, 0.6);
        assert_eq!(top(&ui), "room-standings");
        let standings = shot(&mut ui, "3-standings");
        inside(standings, "standings");
        ddi_desktop::room::discard();
    }
}
