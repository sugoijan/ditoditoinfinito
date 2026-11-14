//! The multiplayer prototype's practice room drawn offscreen on this
//! machine's GPU, as the desktop window draws it (`--screen room`):
//! straight on the room's song select, no Rules by themselves (the hidden
//! option aside): the header with Rules, Players and More, the song's
//! side with the players panel under it, the list and the status line,
//! each inside the window. Skipped where there is no GPU adapter
//! (CI). `DDI_OFFSCREEN_PPM=<file>` keeps the picture.

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::UiEvent;
use ddi_ui::dev_widgets::FoldFn;

#[test]
fn the_practice_room_draws_its_parts_inside_the_window() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: FoldFn = Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_desktop::Nfkd, s));
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("room", fold);
    env.reduced_motion.set(true);
    let size = [1280, 720];
    ui.handle(UiEvent::Resize {
        size: [size[0] as f32, size[1] as f32],
        ratio: 1.0,
    });
    for _ in 0..60 {
        ui.update(1.0 / 60.0);
    }
    // Song select at once: the Rules stay behind their button.
    assert_eq!(ui.layers().last().unwrap().name, "select");
    let pixels = gpu.draw_offscreen(&mut ui, size, format);
    if let Some(path) = std::env::var_os("DDI_OFFSCREEN_PPM") {
        let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
        ppm.extend(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]]),
        );
        std::fs::write(path, ppm).expect("ppm");
    }
    assert_eq!(env.state.borrow().screen, "select");
    let view = env.state.borrow().room.clone().expect("the room's view");
    assert_eq!(view.phase, "lobby");
    assert!(view.host);
    assert_eq!(view.status, "Pick a song to start the match.");
    // The header is singleplayer's: Rules a footer link, no Players
    // button (follow-up round F).
    assert!(ui.bounds("Room.Players").is_none());
    assert!(ui.bounds("Room.Rules").is_some_and(|b| b[1] > 600.0));
    for name in [
        "Back",
        "Room.Rules",
        "More",
        "Info",
        "Room.Status",
        "RoomPlayers",
    ] {
        let b = ui.bounds(name).unwrap_or_else(|| panic!("{name} drawn"));
        assert!(
            b[0] >= 0.0 && b[1] >= 0.0 && b[2] <= 1280.0 && b[3] <= 720.0,
            "{name} {b:?}"
        );
    }
    // The song's card left of the status line, over the list.
    let info = ui.bounds("Info").unwrap();
    let status = ui.bounds("Room.Status").unwrap();
    assert!(info[2] <= status[0], "{info:?} {status:?}");
    // The players under the song, left of the list: the player and the
    // three CPUs the default rules seat.
    let players = ui.bounds("RoomPlayers").unwrap();
    assert!(
        players[1] >= info[3] && players[2] <= status[0],
        "{players:?}"
    );
    assert_eq!(view.members.len(), 4);
    // Something was drawn: the banner and plates are not the ground alone.
    let distinct: std::collections::HashSet<[u8; 4]> =
        pixels.as_chunks::<4>().0.iter().copied().collect();
    assert!(distinct.len() > 100, "{} colors", distinct.len());
}
