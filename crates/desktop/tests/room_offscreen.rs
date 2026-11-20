//! The multiplayer prototype's practice room drawn offscreen on this
//! machine's GPU, as the desktop window draws it (`--screen room`):
//! straight on the room's song select, no Rules by themselves (the hidden
//! option aside): the header with More, the song's side, the list and
//! its status line, the players panel at the right (Rules in the footer),
//! each inside the window. Without a GPU adapter (CI)
//! only the pictures are skipped: the rest is checked all the same. `DDI_OFFSCREEN_PPM=<dir>` keeps the picture (`room.ppm`).

mod common;



#[test]
fn the_practice_room_draws_its_parts_inside_the_window() {
    let mut pictures = common::gpu::Pictures::new();
    let size = [1280, 720];
    let (mut ui, env) = common::ui::theme("room", common::ui::nfkd(), [1280, 720], true);
    for _ in 0..60 {
        ui.update(1.0 / 60.0);
    }
    // Song select at once: the Rules stay behind their button.
    assert_eq!(ui.layers().last().unwrap().name, "select");
    let pixels = pictures.shot(&mut ui, size, "room.ppm");
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
    // The players at the right, past the list and its status line (round
    // SLB): the player and the three CPUs the default rules seat.
    let players = ui.bounds("RoomPlayers").unwrap();
    assert!(players[0] >= status[2] && players[2] <= 1280.0, "{players:?}");
    assert_eq!(view.members.len(), 4);
    // Something was drawn: the banner and plates are not the ground alone.
    if let Some(pixels) = pixels {
        let distinct: std::collections::HashSet<[u8; 4]> =
            pixels.as_chunks::<4>().0.iter().copied().collect();
        assert!(distinct.len() > 100, "{} colors", distinct.len());
    }
}
