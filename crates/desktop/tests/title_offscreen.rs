//! The real title shader: its geometry stays fixed while the spatial field
//! evolves, even a full palette cycle later. Reduced motion freezes both.
use ddi_actor::Kind;
use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::UiEvent;
use std::rc::Rc;

#[test]
fn title_flows_without_moving_letters_and_stops_for_reduced_motion() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", Rc::new(str::to_string));
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    for _ in 0..60 {
        ui.update(1.0 / 60.0);
    }
    let size = [1280, 720];
    let first = gpu.draw_offscreen(&mut ui, size, format);
    assert!(!ui.needs_redraw());
    ui.update(4.0);
    assert_eq!(first, gpu.draw_offscreen(&mut ui, size, format));
    let bounds = ui.bounds("TitleLayer3").expect("title lettering");
    env.reduced_motion.set(false);
    assert!(ui.needs_redraw());
    // Four seconds is a full palette turn: any change now must come from
    // the evolving field itself, not just from the palette phase.
    for _ in 0..240 {
        ui.update(1.0 / 60.0);
    }
    let second = gpu.draw_offscreen(&mut ui, size, format);
    assert_eq!(bounds, ui.bounds("TitleLayer3").unwrap());
    let mut colored = 0;
    let mut changed = 0;
    let mut white = 0;
    for y in bounds[1].ceil() as u32..bounds[3].floor() as u32 {
        for x in bounds[0].ceil() as u32..bounds[2].floor() as u32 {
            let i = ((y * size[0] + x) * 4) as usize;
            let a = &first[i..i + 3];
            let b = &second[i..i + 3];
            if a == [242, 238, 233] {
                white += 1;
                assert_eq!(a, b, "white letter moved at {x},{y}");
            }
            if *a.iter().max().unwrap() - *a.iter().min().unwrap() > 60 {
                colored += 1;
                if a.iter().zip(b).any(|(a, b)| a.abs_diff(*b) > 12) {
                    changed += 1;
                }
            }
        }
    }
    assert!(
        white > 1000 && colored > 300,
        "white {white}, colored {colored}"
    );
    assert!(
        changed > colored / 3,
        "field did not evolve: {changed}/{colored}"
    );
    env.reduced_motion.set(true);
    ui.update(8.0);
    assert_eq!(second, gpu.draw_offscreen(&mut ui, size, format));
    assert!(!ui.needs_redraw());
    if let Some(path) = std::env::var_os("DDI_OFFSCREEN_PPM") {
        let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
        ppm.extend(
            second
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]]),
        );
        std::fs::write(path, ppm).unwrap();
    }
    // Resizing preserves the animation clock and fits both the artwork and
    // the menu's touch targets in portrait and short landscape windows.
    let clock = |ui: &ddi_ui::Ui| match &ui.actor("TitleLayer0").unwrap().kind {
        Kind::Sprite(s) => s.shape.vivid.unwrap().time,
        _ => panic!("outline sprite"),
    };
    let time = clock(&ui);
    for size in [[390.0, 844.0], [844.0, 390.0], [320.0, 568.0]] {
        ui.handle(UiEvent::Resize { size, ratio: 1.0 });
        ui.update(0.0);
        assert_eq!(time, clock(&ui));
        let vp = ui.viewport();
        for name in [
            "TitleLayer3",
            "Singleplayer",
            "Multiplayer",
            "Options",
            "Credits",
            "Quit",
        ] {
            let [x0, y0, x1, y1] = ui.bounds(name).unwrap();
            assert!(
                x0 >= 0.0 && y0 >= 0.0 && x1 <= vp.screen.width && y1 <= vp.screen.height,
                "{name} outside {size:?}: {:?}",
                [x0, y0, x1, y1]
            );
        }
    }
}
