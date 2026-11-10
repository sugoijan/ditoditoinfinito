//! The multiplayer prototype's practice room drawn offscreen on this
//! machine's GPU, as the desktop window draws it (`--screen room`): the
//! header with Rules and More, the players, the stage's song and chart,
//! the stages and the action row, each inside the window. Skipped where
//! there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<file>` keeps the
//! picture.

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
    // A song on the first stage, as song select's pick mode chooses it.
    let song = ddi_ui::screens::sample::bundled()
        .into_iter()
        .find(|s| s.charts.iter().any(|c| c.style == "dance-single"))
        .expect("a sample song with a Single chart");
    let chart = song
        .charts
        .iter()
        .find(|c| c.style == "dance-single")
        .unwrap()
        .clone();
    let room = ddi_desktop::room::current();
    let first = room.borrow().stages()[0].id;
    room.borrow_mut().choose(first, &song, &chart).unwrap();
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
    assert_eq!(env.state.borrow().screen, "room");
    let view = env.state.borrow().room.clone().expect("the room's view");
    assert_eq!(view.phase, "lobby");
    assert_eq!(view.stages[0].song.as_deref(), Some(song.id.as_str()));
    for name in [
        "Back", "Rules", "More", "Info", "Chart", "Choose", "Stage.0", "Stage.3", "Ready", "Start",
    ] {
        let b = ui.bounds(name).unwrap_or_else(|| panic!("{name} drawn"));
        assert!(
            b[0] >= 0.0 && b[1] >= 0.0 && b[2] <= 1280.0 && b[3] <= 720.0,
            "{name} {b:?}"
        );
    }
    // The song's card left of the stages, the action row under both.
    let info = ui.bounds("Info").unwrap();
    let stage = ui.bounds("Stage.0").unwrap();
    let start = ui.bounds("Start").unwrap();
    assert!(info[2] <= stage[0], "{info:?} {stage:?}");
    assert!(start[1] >= info[3], "{start:?} {info:?}");
    // Something was drawn: the banner and plates are not the ground alone.
    let distinct: std::collections::HashSet<[u8; 4]> =
        pixels.as_chunks::<4>().0.iter().copied().collect();
    assert!(distinct.len() > 100, "{} colors", distinct.len());
}
