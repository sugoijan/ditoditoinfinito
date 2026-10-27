//! Options' field preview draws the noteskin play would draw (the
//! settings' `noteskin`, the pump one for pump layouts), changing once
//! the pick is read (on a thread of its own: the frames go on meanwhile),
//! a pick passed over before its read came never shown.
//! Skipped where there is no GPU adapter (CI).

mod common;
use ddi_desktop::field_preview::Field;
use ddi_desktop::gpu;
use ddi_render::Painter;
use ddi_ui::state::settings::{ParamValue, Settings};

#[test]
fn a_skin_picked_shows_once_read() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    if ddi_desktop::library::noteskins::bundled().is_empty() {
        eprintln!(
            "no bundled noteskins' index (`cargo run -p xtask -- gen-noteskins`, which `just \
             test` runs): skipped"
        );
        return;
    }
    let (device, queue) = gpu::device(&adapter).unwrap();
    let mut painter = Painter::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut field = Field::default();
    field.set_rect(Some([0.0, 0.0, 256.0, 480.0]));
    let mut frame = |field: &mut Field| {
        let mut encoder = device.create_command_encoder(&Default::default());
        field.draw(&device, &queue, &mut encoder, &mut painter);
        queue.submit(Some(encoder.finish()));
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
    };
    // Infinito's declared options match between preview and play.
    let infinito = |name: &str, value: ParamValue| {
        let mut s = Settings::default();
        s.set_infinito_param(name, value);
        s
    };
    let peters = |colors: &str| {
        let key = ddi_noteskin::bundled::PETERS_DDR;
        let mut s = Settings {
            noteskin: Some(key.into()),
            ..Settings::default()
        };
        s.set_skin_param(key, "Colors", Some(ParamValue::Text(colors.into())), None);
        s.set_skin_param(key, "Explosions", Some(ParamValue::Bool(false)), None);
        s
    };
    let picks: [(Settings, Option<&str>); 13] = [
        // SVG Infinito by default.
        (Settings::default(), Some("Infinito")),
        (
            infinito("Colors", ParamValue::Text("Rainbow".into())),
            Some("Infinito"),
        ),
        (
            infinito("Colors", ParamValue::Text("Flat".into())),
            Some("Infinito"),
        ),
        (
            infinito("ShockArrows", ParamValue::Bool(true)),
            Some("Infinito"),
        ),
        (
            infinito("Onigiri", ParamValue::Bool(false)),
            Some("Infinito"),
        ),
        (
            infinito("ReceptorFill", ParamValue::Bool(false)),
            Some("Infinito"),
        ),
        (peters("Vivid"), Some("Peters-DDR-Vivid")),
        (peters("Quantized"), Some("Peters-DDR")),
        // The bundled raster center, diagonal holds/rolls and layered mines.
        (
            Settings {
                style: "danoni-9B".into(),
                ..peters("Vivid")
            },
            Some("Peters-DDR-Vivid"),
        ),
        (
            Settings {
                style: "danoni-9B".into(),
                ..peters("Quantized")
            },
            Some("Peters-DDR"),
        ),
        (
            Settings {
                noteskin: Some("bundled:dance/delta".into()),
                ..Settings::default()
            },
            Some("Delta"),
        ),
        (
            Settings {
                noteskin: Some("builtin".into()),
                ..Settings::default()
            },
            None,
        ),
        // A pump layout: the pump skin's (here the built-in arrows),
        // not the dance one's.
        (
            Settings {
                style: "pump-single".into(),
                noteskin: Some("bundled:dance/delta".into()),
                pump_noteskin: Some("builtin".into()),
                ..Settings::default()
            },
            None,
        ),
    ];
    for (i, (settings, want)) in picks.into_iter().enumerate() {
        field.set_settings(&settings);
        let mut frames = 0;
        loop {
            frame(&mut field);
            if i == 0 && field.reading() {
                // Nothing before the first skin, not the built-in arrows.
                assert_eq!(field.frames(), 0);
            }
            frames += 1;
            if field.skin_name().as_deref() == want && !field.reading() {
                break;
            }
            assert!(
                frames < 500,
                "{:?}: {:?}",
                settings.noteskin,
                field.skin_name()
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    // The preview's skin with the values a play's has: the same skin,
    // its Lua reading the same.
    let mut settings = infinito("HitVariation", ParamValue::Bool(false));
    settings.set_infinito_param("Explosions", ParamValue::Bool(false));
    field.set_settings(&settings);
    let mut frames = 0;
    while field.skin_name().as_deref() != Some("Infinito") || field.reading() {
        frame(&mut field);
        frames += 1;
        assert!(frames < 500, "{:?}", field.skin_name());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let loaded = ddi_desktop::play::load::load("beatucada", 48_000).unwrap();
    let (play, _, strike) = ddi_desktop::play::host::noteskin(&loaded, 0, &settings).unwrap();
    assert_eq!(play.name, "Infinito");
    assert_eq!(play.parameters(), field.skin_parameters());
    assert_eq!(
        play.parameters().get("Explosions"),
        Some(&ParamValue::Bool(false))
    );
    // A Lua stop counts against Infinito with any options.
    assert_eq!(strike, ddi_ui::state::settings::INFINITO);
    // Delta picked and, before it is read, Infinito: Infinito shows, never Delta.
    field.set_settings(&Settings {
        noteskin: Some("bundled:dance/delta".into()),
        ..Settings::default()
    });
    frame(&mut field);
    field.set_settings(&Settings::default());
    let mut frames = 0;
    loop {
        frame(&mut field);
        assert_ne!(field.skin_name().as_deref(), Some("Delta"));
        if field.skin_name().as_deref() == Some("Infinito") && !field.reading() {
            break;
        }
        frames += 1;
        assert!(frames < 500);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(field.frames() >= 4);
}

#[test]
fn the_prompt_readies_its_own_skin_and_releases_the_picture() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let mut painter = Painter::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut field = Field::default();
    let mut picture = ddi_ui::screens::prompt_controls::Picture {
        controls: std::rc::Rc::new(ddi_ui::screens::prompt_controls::Controls::new(
            &ddi_chart::Layout::builtin("danoni-7").unwrap(),
            &Settings::default(),
        )),
        size: [448.0, 64.0],
        ready: std::rc::Rc::default(),
        time: 0.0,
        active: None,
    };
    let mut draw = |field: &mut Field| {
        let mut encoder = device.create_command_encoder(&Default::default());
        field.draw(&device, &queue, &mut encoder, &mut painter);
        queue.submit(Some(encoder.finish()));
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
    };
    field.set_receptors(Some(&picture));
    let started = std::time::Instant::now();
    while !picture.ready.get() {
        assert!(
            started.elapsed().as_secs() < 10,
            "preview skin never became ready"
        );
        draw(&mut field);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(field.skin_name().as_deref(), Some("Infinito"));
    assert!(field.holds_targets());
    for lane in 0..7 {
        picture.time = 0.25 + lane as f32 * 0.14;
        picture.active = Some(lane);
        field.set_receptors(Some(&picture));
        draw(&mut field);
        assert!(picture.ready.get());
    }
    field.set_receptors(None);
    draw(&mut field);
    assert!(!field.holds_targets());
    assert!(!field.animating());
}

/// The options screen as the window runs it: a pick on its Arrows row
/// reaches the preview through the edited settings
/// (`options::Shared::take_edited`, as `shell.rs` polls them each
/// loop), which then shows the skin picked.
#[test]
fn the_options_screen_switches_the_preview_skin() {
    use ddi_desktop::gpu::Gpu;
    use ddi_ui::event::{KeyEvent, UiEvent};
    use ddi_ui::{Ui, UiRequest};
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        return;
    };
    ddi_platform::storage::install(std::rc::Rc::new(
        ddi_platform::storage::MemoryStore::default(),
    ));
    let (device, queue) = gpu::device(&adapter).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: ddi_ui::dev_widgets::FoldFn = std::rc::Rc::new(|s: &str| s.to_string());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
    let (screen, shared) = ddi_desktop::options::screen(&env, None, None);
    ui.push(screen);
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    let size = [1280, 720];
    let apply = |gpu: &mut Gpu, requests: Vec<UiRequest>| {
        for r in requests {
            if let UiRequest::FieldPreview(rect) = r {
                gpu.field.set_rect(rect);
                if rect.is_some() {
                    gpu.field.set_settings(&Settings::load());
                }
            }
        }
    };
    let tick = |ui: &mut Ui, gpu: &mut Gpu| {
        ui.update(1.0 / 60.0);
        apply(gpu, ui.take_requests());
        if let Some(s) = shared.take_edited() {
            gpu.field.set_settings(&s);
        }
        gpu.draw_offscreen(ui, size, format);
    };
    let key = |ui: &mut Ui, gpu: &mut Gpu, code: &str| {
        let k = code
            .strip_prefix("Key")
            .map_or(code, |k| k.to_lowercase().leak());
        apply(
            gpu,
            ui.handle(UiEvent::Key(KeyEvent::down(code, k))).requests,
        );
        apply(gpu, ui.handle(UiEvent::Key(KeyEvent::up(code, k))).requests);
        tick(ui, gpu);
    };
    let mut frames = 0;
    while gpu.field.skin_name().as_deref() != Some("Infinito") || gpu.field.reading() {
        tick(&mut ui, &mut gpu);
        frames += 1;
        assert!(frames < 500, "{:?}", gpu.field.skin_name());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(gpu.field.shown());
    // The Arrows section's noteskin row.
    for _ in 0..12 {
        if env
            .state
            .borrow()
            .option_rows
            .iter()
            .any(|r| r == "noteskin")
        {
            break;
        }
        key(&mut ui, &mut gpu, "Tab");
    }
    for _ in 0..40 {
        if env.state.borrow().option_id == "noteskin" {
            break;
        }
        key(&mut ui, &mut gpu, "ArrowDown");
    }
    eprintln!("sections: {:?}", env.state.borrow().section);
    eprintln!("rows: {:?}", env.state.borrow().option_rows);
    assert_eq!(env.state.borrow().option_id, "noteskin");
    let before = shared.model.borrow().settings.noteskin.clone();
    key(&mut ui, &mut gpu, "ArrowRight");
    let after = shared.model.borrow().settings.noteskin.clone();
    assert_ne!(before, after, "the row picked another skin");
    let mut frames = 0;
    while gpu.field.skin_name().as_deref() == Some("Infinito") || gpu.field.reading() {
        tick(&mut ui, &mut gpu);
        frames += 1;
        assert!(frames < 500, "{after:?}: {:?}", gpu.field.skin_name());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let name = gpu.field.skin_name();
    assert_eq!(
        gpu.field.shown_layout().map(|l| l.id.as_str()),
        Some("dance-single")
    );
    // P picks the preview's next layout, which is no setting: the
    // preview follows it (the edited settings carry it), the stored
    // style does not.
    key(&mut ui, &mut gpu, "KeyP");
    let style = shared.preview_settings().style;
    assert_ne!(style, "dance-single");
    assert_eq!(Settings::load().style, "dance-single");
    let mut frames = 0;
    while gpu.field.shown_layout().map(|l| l.id.as_str()) != Some(style.as_str())
        || gpu.field.reading()
    {
        tick(&mut ui, &mut gpu);
        frames += 1;
        assert!(
            frames < 500,
            "{style}: {:?}",
            gpu.field.shown_layout().map(|l| &l.id)
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(gpu.field.skin_name(), name);
    eprintln!("picked {after:?}: {name:?} on {style}");
}

/// Song select's chart readies the prompt's diagram's skin ahead
/// (`Field::warm`): nothing is drawn meanwhile, and the diagram is ready
/// on the prompt's first frame.
#[test]
fn a_warmed_skin_is_ready_on_the_prompts_first_frame() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let mut painter = Painter::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut field = Field::default();
    let controls = std::rc::Rc::new(ddi_ui::screens::prompt_controls::Controls::new(
        &ddi_chart::Layout::builtin("pump-single").unwrap(),
        &Settings::default(),
    ));
    let mut draw = |field: &mut Field| {
        let mut encoder = device.create_command_encoder(&Default::default());
        field.draw(&device, &queue, &mut encoder, &mut painter);
        queue.submit(Some(encoder.finish()));
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
    };
    field.warm(&controls);
    assert!(!field.ready());
    let started = std::time::Instant::now();
    while !field.ready() {
        // Frames go on (the shell's loop polls the read with them).
        assert!(field.animating(), "frames go on while the skin is read");
        assert!(started.elapsed().as_secs() < 10, "the skin never came");
        draw(&mut field);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(field.frames(), 0);
    assert!(!field.holds_targets() && !field.animating());
    assert_eq!(
        field.shown_layout().map(|l| l.id.as_str()),
        Some("pump-single")
    );
    let picture = ddi_ui::screens::prompt_controls::Picture {
        controls,
        size: [320.0, 64.0],
        ready: std::rc::Rc::default(),
        time: 0.0,
        active: None,
    };
    field.set_receptors(Some(&picture));
    draw(&mut field);
    assert!(picture.ready.get(), "ready on the first frame");
    assert_eq!(field.frames(), 1);
    assert!(!field.reading());
}
