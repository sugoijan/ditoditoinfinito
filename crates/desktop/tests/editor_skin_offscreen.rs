//! The chart editor's field in the player's skin
//! (`ddi_render::editor_field`, `docs/plans/editor.md` step 3) drawn on
//! this machine's GPU: a note's art sits where the editor's overlay
//! marks its row (the picture's `y`), in the edit view and the play
//! view, with Dito, a raster skin (Delta), skins of their own geometry
//! (Peters-DDR) and models (CF_CHROME): placed by its own anchor, as
//! play places it, its art moving exactly as the row does; the edit
//! view is the play view where the tempo is one and the zoom the speed;
//! the editor on a bundled song shows the skin's picture, and the
//! compact field (F) gives it up and takes it back. Skipped where there
//! is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures.

mod common;
use std::rc::Rc;

use ddi_chart::{Chart, Note, NoteKind, Tick, TimingMap};
use ddi_desktop::gpu;
use ddi_engine::scroll::{ScrollOptions, SpeedMod};
use ddi_render::editor_field::{EditorField, FieldDraw};
use ddi_render::field_preview::SkinWant;
use ddi_ui::UiEvent;
use ddi_ui::event::KeyEvent;
use ddi_ui::screens::editor_field::{FieldPicture, Mapping, frame, shown_seconds};
use ddi_ui::state::editor::Editor;

/// Rows of 1536 bytes, as copies want them.
const SIZE: [u32; 2] = [384, 640];
const ARROW: f32 = 64.0;
const LEFT: f32 = (384.0 - 4.0 * 64.0) / 2.0;
const LINE: f32 = 150.0;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Gpu {
    fn new() -> Option<Gpu> {
        let instance = gpu::instance();
        let adapter = gpu::adapter(&instance, None)?;
        let (device, queue) = gpu::device(&adapter).ok()?;
        Some(Gpu { device, queue })
    }

    /// `texture`'s RGBA rows.
    fn read(&self, texture: &wgpu::Texture, encoder: wgpu::CommandEncoder) -> Vec<u8> {
        let mut encoder = encoder;
        let [w, h] = [texture.width(), texture.height()];
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(w * 4 * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
            },
            texture.size(),
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        slice.get_mapped_range().expect("mapped").to_vec()
    }

    /// The field drawn with skin `key` (`None`: the built-in arrows) on
    /// dance single.
    fn field(&self, key: Option<&str>, chart: &Chart, editor: &Editor) -> (EditorField, String) {
        let mut field = EditorField::new(
            &self.device,
            &self.queue,
            Some(wgpu::TextureFormat::Rgba16Float),
            &[ddi_render::HUD_FONT],
        );
        let want = SkinWant {
            key: key.map(str::to_string),
            layout: "dance-single".into(),
            reverse: false,
            params: Default::default(),
        };
        let layout = want.layout();
        let mut name = "built-in".to_string();
        if let Some(key) = key {
            let env = ddi_shell::editor_field::skin_env(&want, editor.song(), chart);
            let reading = ddi_desktop::play::skins::Reading::start(key, &layout, env, None);
            let read = loop {
                if let Some(r) = reading.poll() {
                    break r;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            let prepared = read
                .and_then(|r| ddi_desktop::play::skins::build(r, &layout))
                .unwrap_or_else(|| panic!("{key} is read"));
            name = prepared.play.name.clone();
            let images = prepared.images;
            let (device, queue) = (&self.device, &self.queue);
            field.set_noteskin(device, queue, Some(prepared.play), |renderer, play| {
                ddi_desktop::play::view::upload_noteskin(renderer, device, queue, play, &images);
            });
        }
        (field, name)
    }

    /// `picture` drawn: its pixels.
    fn draw(&self, field: &mut EditorField, picture: &FieldPicture) -> Vec<u8> {
        let layout = ddi_shell::editor_field::layout(picture);
        let d: FieldDraw = ddi_shell::editor_field::draw(picture, &layout, Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        field.draw(&self.device, &self.queue, &mut encoder, SIZE, &d);
        self.read(field.texture().expect("drawn"), encoder)
    }
}

fn keep(pixels: &[u8], size: [u32; 2], name: &str) {
    let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") else {
        return;
    };
    let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
    ppm.extend(pixels.chunks(4).flat_map(|p| [p[0], p[1], p[2]]));
    let name: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), ppm);
}

/// The view's mapping with `shown` at the line: the edit view at
/// `beat_h`, or the play view at `scroll`.
fn mapping(timing: &TimingMap, shown: f64, play: Option<ScrollOptions>, beat_h: f32) -> Mapping {
    match play {
        None => Mapping::edit(LINE, shown, beat_h, ARROW),
        Some(scroll) => Mapping::play(
            scroll,
            timing,
            shown_seconds(timing, shown),
            [0.0, SIZE[1] as f32],
            LINE,
            shown,
            beat_h,
            ARROW,
        ),
    }
}

/// The picture of `chart` with `m`, as the editor makes it.
fn picture(chart: &Chart, timing: &TimingMap, m: &Mapping) -> FieldPicture {
    FieldPicture {
        rect: [0.0, 0.0, SIZE[0] as f32, SIZE[1] as f32],
        layout: "dance-single".into(),
        arrow: ARROW,
        left: LEFT,
        receptor_y: m.line_y,
        reverse: m.dir() < 0.0,
        frame: frame(
            chart,
            timing,
            m,
            0.0,
            SIZE[1] as f32,
            shown_seconds(timing, m.shown),
            &[None; 4],
        ),
        running: false,
        events: Vec::new(),
        background: [0.05, 0.04, 0.1, 1.0],
    }
}

/// What differs between `a` and `b` in columns `x0..x1`: its middle
/// (weighted by how much each pixel changed), the middle of the rows it
/// spans, and its weight.
fn changed_middle(a: &[u8], b: &[u8], [x0, x1]: [u32; 2]) -> Option<(f32, f32, f32)> {
    let (mut sum, mut weight) = (0.0f64, 0.0f64);
    let (mut lo, mut hi) = (u32::MAX, 0);
    for y in 0..SIZE[1] {
        for x in x0..x1 {
            let i = ((y * SIZE[0] + x) * 4) as usize;
            let d: i32 = (0..3)
                .map(|k| (i32::from(a[i + k]) - i32::from(b[i + k])).abs())
                .sum();
            if d > 24 {
                sum += f64::from(y) * f64::from(d);
                weight += f64::from(d);
                (lo, hi) = (lo.min(y), hi.max(y));
            }
        }
    }
    (weight > 0.0).then(|| {
        (
            (sum / weight) as f32 + 0.5,
            (lo + hi + 1) as f32 / 2.0,
            weight as f32,
        )
    })
}

#[test]
fn a_notes_art_sits_on_its_row() {
    common::indexes();
    let Some(gpu) = Gpu::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    if ddi_desktop::library::noteskins::bundled().is_empty() {
        eprintln!("no bundled noteskins' index: skipped");
        return;
    }
    let editor = Editor::new_song(None, "Field");
    let mut timing = TimingMap::constant(120.0, 0.0);
    // A tempo change and a stop before the note: the play view bends,
    // the edit view does not.
    timing.bpms.push(ddi_chart::BpmSegment {
        tick: Tick::from_beats(2),
        bpm: 180.0,
    });
    timing.stops.push(ddi_chart::StopSegment {
        tick: Tick::from_beats(3),
        seconds: 0.25,
    });
    let base = editor.chart().clone();
    let empty = Chart {
        notes: Vec::new(),
        ..base.clone()
    };
    // A 4th in the second lane, beat 4.
    let lane = 1u8;
    let with_note = |tick: i64| Chart {
        notes: vec![Note::new(Tick(tick), lane, NoteKind::Tap)],
        ..base.clone()
    };
    let columns = [
        (LEFT + ARROW * f32::from(lane) - ARROW / 2.0) as u32,
        (LEFT + ARROW * (f32::from(lane) + 1.0) + ARROW / 2.0) as u32,
    ];
    let skins: [Option<&str>; 5] = [
        None,
        Some(ddi_noteskin::bundled::DITO),
        Some("bundled:dance/delta"),
        Some(ddi_noteskin::bundled::PETERS_DDR),
        Some("bundled:dance/cf_chrome"),
    ];
    let views: [(&str, Option<ScrollOptions>); 3] = [
        ("edit", None),
        (
            "play",
            Some(ScrollOptions {
                speed: SpeedMod::XMod(1.5),
                ..Default::default()
            }),
        ),
        (
            "cmod",
            Some(ScrollOptions {
                speed: SpeedMod::CMod(200.0),
                ..Default::default()
            }),
        ),
    ];
    for key in skins {
        let (mut field, name) = gpu.field(key, &base, &editor);
        for (view, scroll) in views {
            // The note at two places on the field: the cursor a beat
            // apart.
            let mut middles = Vec::new();
            for shown in [0.0, 48.0] {
                let m = mapping(&timing, shown, scroll, 72.0);
                let note = with_note(192);
                let p = picture(&note, &timing, &m);
                let row = p.y(m.arrows(&timing, Tick(192)));
                // The overlay's tick for that note is at its row.
                assert!(
                    (m.y(&timing, 192.0) - row).abs() < 0.01,
                    "{name} {view}: the overlay's row {} is the picture's {row}",
                    m.y(&timing, 192.0)
                );
                let with = gpu.draw(&mut field, &p);
                let without = gpu.draw(&mut field, &picture(&empty, &timing, &m));
                keep(&with, SIZE, &format!("editor-skin-{name}-{view}-{shown}"));
                let (y, span, weight) = changed_middle(&with, &without, columns)
                    .unwrap_or_else(|| panic!("{name} {view}: the note is drawn"));
                assert!(weight > 2000.0, "{name} {view}: a note's worth drawn");
                eprintln!(
                    "{name} {view} at {shown}: row {row:.1}, art {y:.1}, its rows' middle {span:.1}"
                );
                // The art's rows centered on the row (an arrow's mass is
                // not: its head is wider).
                assert!(
                    (span - row).abs() < 2.0,
                    "{name} {view}: the art's rows' middle {span} on the row {row}"
                );
                middles.push((row, y));
            }
            // Placed by its own anchor: the art moves as the row does.
            let [(r0, y0), (r1, y1)] = [middles[0], middles[1]];
            assert!(r0 != r1);
            assert!(
                ((y1 - y0) - (r1 - r0)).abs() < 1.5,
                "{name} {view}: the row moved {} and the art {}",
                r1 - r0,
                y1 - y0
            );
        }
    }
}

#[test]
fn the_edit_view_is_the_play_view_at_one_tempo() {
    common::indexes();
    let Some(gpu) = Gpu::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let editor = Editor::new_song(None, "Field");
    let timing = TimingMap::constant(150.0, 0.0);
    let chart = Chart {
        notes: [
            (0, 0),
            (48, 1),
            (72, 2),
            (96, 3),
            (120, 0),
            (168, 1),
            (204, 2),
        ]
        .iter()
        .map(|&(t, l)| Note::new(Tick(t), l, NoteKind::Tap))
        .chain([Note::new(
            Tick(216),
            3,
            NoteKind::HoldHead { end: Tick(312) },
        )])
        .collect(),
        ..editor.chart().clone()
    };
    let key = (!ddi_desktop::library::noteskins::bundled().is_empty())
        .then_some(ddi_noteskin::bundled::DITO);
    let (mut field, _) = gpu.field(key, &chart, &editor);
    let x = 1.75;
    for shown in [0.0, 60.0] {
        let edit = gpu.draw(
            &mut field,
            &picture(&chart, &timing, &mapping(&timing, shown, None, ARROW * x)),
        );
        let scroll = ScrollOptions {
            speed: SpeedMod::XMod(f64::from(x)),
            ..Default::default()
        };
        let play = gpu.draw(
            &mut field,
            &picture(&chart, &timing, &mapping(&timing, shown, Some(scroll), 0.0)),
        );
        let (at, worst) = edit
            .iter()
            .zip(&play)
            .map(|(a, b)| a.abs_diff(*b))
            .enumerate()
            .max_by_key(|(_, d)| *d)
            .unwrap_or((0, 0));
        keep(&play, SIZE, &format!("editor-views-play-{shown}"));
        let px = at as u32 / 4;
        eprintln!("worst at ({}, {})", px % SIZE[0], px / SIZE[0]);
        assert!(worst <= 2, "at {shown}: the views differ by {worst}");
        keep(&edit, SIZE, &format!("editor-views-{shown}"));
    }
}

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

#[test]
fn the_editor_shows_the_skin_and_its_compact_field() {
    common::indexes();
    let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch/editor-skin-offscreen/home");
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", &home) };
    ddi_desktop::audio::set_headless(true);
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = gpu::Gpu::new(device, queue, format);
    let songs = ddi_desktop::songs::Songs::new(Vec::new());
    songs.skip_offer();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let sound = Rc::new(std::cell::RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::preview::PreviewSong::bundled,
    )));
    let (mut ui, env, _) = ddi_desktop::shell::library_ui("select", fold, sound, &songs);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    for _ in 0..30 {
        ui.update(1.0 / 60.0);
    }
    let screen = ddi_desktop::editor::screen(&env, &songs, "beatucada", 1).expect("editor");
    ui.push(screen);
    for _ in 0..30 {
        ui.update(1.0 / 60.0);
    }
    let view = || {
        env.state
            .borrow()
            .editor
            .clone()
            .expect("the editor's view")
    };
    assert!(!view().plates, "the skin draws the field");
    // The skin is read on its thread: frames until it is drawn.
    let started = std::time::Instant::now();
    let mut pixels = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    while gpu.editor.busy() || gpu.editor.skin_name().is_none() {
        assert!(
            started.elapsed().as_secs() < 60,
            "the skin never came: {:?}",
            gpu.editor.skin_name()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        ui.update(1.0 / 60.0);
        pixels = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    }
    keep(&pixels, [1280, 720], "editor-skin");
    let frames = gpu.editor.frames();
    assert!(frames > 0, "the field is drawn");
    // Nothing changed: no frame of the field's.
    ui.update(1.0 / 60.0);
    gpu.draw_offscreen(&mut ui, [1280, 720], format);
    assert_eq!(gpu.editor.frames(), frames, "drawn again only for a change");
    // A step down: drawn again.
    key(&mut ui, "ArrowDown", "ArrowDown");
    ui.update(1.0 / 60.0);
    gpu.draw_offscreen(&mut ui, [1280, 720], format);
    assert!(gpu.editor.frames() > frames);
    // The play view, then the compact field: the skin's texture goes.
    key(&mut ui, "KeyV", "v");
    ui.update(1.0 / 60.0);
    assert_eq!(view().view, "play");
    let play = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    keep(&play, [1280, 720], "editor-skin-play");
    key(&mut ui, "KeyF", "f");
    ui.update(1.0 / 60.0);
    assert!(view().plates);
    let compact = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    keep(&compact, [1280, 720], "editor-compact");
    assert!(gpu.editor.texture().is_none(), "the skin's texture let go");
    key(&mut ui, "KeyF", "f");
    key(&mut ui, "KeyV", "v");
    ui.update(1.0 / 60.0);
    assert!(!view().plates && view().view == "edit");
    let again = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    let differs = |a: &[u8], b: &[u8]| {
        a.iter()
            .zip(b)
            .filter(|(x, y)| x.abs_diff(**y) > 16)
            .count()
    };
    assert!(
        differs(&again, &compact) > 1000,
        "the plates are not the skin"
    );
}
