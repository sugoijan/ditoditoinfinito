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
