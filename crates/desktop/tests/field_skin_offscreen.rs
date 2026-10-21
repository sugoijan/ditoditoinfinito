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
    // Dito's parameters, as a play takes them: its colors' skin, or
    // drawn lighter by the built-in arrows.
    let dito = |name: &str, value: ParamValue| {
        let mut s = Settings::default();
        s.set_dito_param(name, value);
        s
    };
    let picks: [(Settings, Option<&str>); 7] = [
        // Dito by default, in the note colors' variant.
        (Settings::default(), Some("Dito Vivid")),
        (
            dito("Colors", ParamValue::Text("Rainbow".into())),
            Some("Dito Rainbow"),
        ),
        (
            dito("Colors", ParamValue::Text("Quantized".into())),
            Some("Dito"),
        ),
        (dito("Lighter", ParamValue::Bool(true)), None),
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
    for (settings, want) in picks {
        field.set_settings(&settings);
        let mut frames = 0;
        loop {
            frame(&mut field);
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
    let mut settings = dito("Colors", ParamValue::Text("Flat".into()));
    settings.set_dito_param("Explosions", ParamValue::Bool(false));
    field.set_settings(&settings);
    let mut frames = 0;
    while field.skin_name().as_deref() != Some("Dito Flat") || field.reading() {
        frame(&mut field);
        frames += 1;
        assert!(frames < 500, "{:?}", field.skin_name());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let loaded = ddi_desktop::play::load::load("beatucada", 48_000).unwrap();
    let (play, _, strike) = ddi_desktop::play::host::noteskin(&loaded, 0, &settings).unwrap();
    assert_eq!(play.name, "Dito Flat");
    assert_eq!(play.parameters(), field.skin_parameters());
    assert_eq!(
        play.parameters().get("Explosions"),
        Some(&ParamValue::Bool(false))
    );
    // A Lua stop counts against Dito, whichever colors.
    assert_eq!(strike, ddi_ui::state::settings::DITO);
    // Delta picked and, before it is read, Dito: Dito shows, never Delta.
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
        if field.skin_name().as_deref() == Some("Dito Vivid") && !field.reading() {
            break;
        }
        frames += 1;
        assert!(frames < 500);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(field.frames() >= 4);
}
