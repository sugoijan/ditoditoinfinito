//! Dancing☆Onigiri works in the library on disk (`docs/plans/desktop.md`,
//! 7.5b): found as the web's import finds them (`ddi_library::works`),
//! with the same ids and entries, read in place, played. Local works:
//! `DDI_WORKS=<dir>` with `--ignored` lists them (`DDI_WEB_WORKS=<file>`:
//! the web's import of the same folder, its records as JSON lines, to
//! compare with) and autoplays every chart, all top tier.

mod common;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::library::LibSong;
use ddi_desktop::play::{Ended, Feed, Play, Request, load};
use ddi_platform::HostTime;
use ddi_ui::state::settings::Settings;

/// A song's facts as the web's records keep them (`works-web.mjs`).
fn facts(s: &LibSong) -> serde_json::Value {
    let e = &s.entry;
    serde_json::json!({
        "id": e.id,
        "pack": s.pack,
        "title": e.title,
        "artist": e.artist,
        "bpm": e.bpm,
        "length": e.length,
        "layer_images": e.layer_images,
        "charts": e.charts.iter().map(|c| serde_json::json!({
            "index": c.index,
            "layout": c.layout,
            "difficulty": c.difficulty,
            "meter": c.meter,
            "name": c.name,
            "notes": c.notes,
            "digest": c.digest,
            "excessive": c.excessive,
        })).collect::<Vec<_>>(),
    })
}

/// Autoplays chart `chart` of library song `id` headless on a fake clock:
/// its tally.
fn autoplay(id: &str, chart: usize) -> Result<Vec<u32>, String> {
    let loaded = Rc::new(load::load(id, 48_000)?);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let mut p = Play::start(
        Request::song(id, chart, true),
        loaded,
        &Settings::default(),
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )?;
    let step = 1_000_000_000 / 60;
    loop {
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        match p.tick(HostTime(now as f64 / 1e9), None) {
            Some(Ended::Finished { results, .. }) => {
                let mut t = results.tally.taps.to_vec();
                t.push(results.tally.let_go);
                return Ok(t);
            }
            Some(_) => return Err("the play did not finish".into()),
            None => {}
        }
        if now > 1_000_000_000 * 1200 {
            return Err("never ends".into());
        }
    }
}

#[test]
#[ignore = "local works: DDI_WORKS=<dir>"]
fn local_works() {
    common::indexes();
    let Some(dir) = std::env::var_os("DDI_WORKS") else {
        eprintln!("DDI_WORKS not set: skipped");
        return;
    };
    let (songs, report) = ddi_desktop::library::scan_folders(&[PathBuf::from(dir)]);
    for (at, why) in &report.failed {
        eprintln!("not read: {at}: {why}");
    }
    let works: Vec<&LibSong> = songs.iter().filter(|s| s.work.is_some()).collect();
    eprintln!("{} songs of works", works.len());
    let mine: Vec<serde_json::Value> = works.iter().map(|s| facts(s)).collect();
    if let Some(web) = std::env::var_os("DDI_WEB_WORKS") {
        let text = std::fs::read_to_string(web).expect("the web's records");
        let mut theirs: Vec<serde_json::Value> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let mut mine = mine.clone();
        let key = |v: &serde_json::Value| v["id"].as_str().unwrap_or_default().to_string();
        theirs.sort_by_key(key);
        mine.sort_by_key(key);
        assert_eq!(
            mine.iter().map(key).collect::<Vec<_>>(),
            theirs.iter().map(key).collect::<Vec<_>>(),
            "the same songs"
        );
        for (m, t) in mine.iter().zip(&theirs) {
            assert_eq!(m, t, "{}", key(m));
        }
        eprintln!("the same {} songs as the web's import", mine.len());
    }
    // Every chart autoplayed, all top tier.
    let lib: Vec<LibSong> = songs.clone();
    ddi_desktop::library::register_for_tests(&lib);
    let mut played = 0;
    for s in &works {
        for c in &s.entry.charts {
            let t = autoplay(&s.entry.id, c.index).unwrap_or_else(|e| {
                panic!("{} ({}) chart {}: {e}", s.entry.title, s.entry.id, c.index)
            });
            let below: u32 = t[1..t.len() - 1].iter().sum();
            assert_eq!(below, 0, "{} chart {}: {t:?}", s.entry.title, c.index);
            assert_eq!(
                t[t.len() - 1],
                0,
                "{} chart {}: let go",
                s.entry.title,
                c.index
            );
            played += 1;
        }
    }
    eprintln!("{played} charts autoplayed, all top tier");
}

/// A work with back and mask layers, written to `dir` as a page with its
/// music and images (`LayerPack/danoni/layers.html`): a gradient and two
/// shapes behind the field, a text, a pink band with a white disc over
/// it, notes from 13 s.
fn layer_pack(dir: &std::path::Path) -> PathBuf {
    let pack = dir.join("LayerPack");
    for d in ["danoni", "music", "img"] {
        std::fs::create_dir_all(pack.join(d)).unwrap();
    }
    // 30 s of a quiet tone.
    let rate = 44_100u32;
    let n = rate * 30;
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + n * 2).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((n * 2).to_le_bytes());
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let v = (3000.0 * (t * 440.0 * std::f32::consts::TAU).sin()) as i16;
        wav.extend(v.to_le_bytes());
    }
    std::fs::write(pack.join("music/layers.wav"), wav).unwrap();
    let back = image::RgbaImage::from_fn(640, 480, |x, y| {
        let (dx, dy) = (x as f32 - 320.0, y as f32 - 240.0);
        if dx * dx + dy * dy < 100.0 * 100.0 {
            image::Rgba([240, 200, 40, 255])
        } else if (40..140).contains(&x) && (40..140).contains(&y) {
            image::Rgba([30, 220, 120, 255])
        } else {
            image::Rgba([(40 + y / 4) as u8, 60, (160 - y / 4) as u8, 255])
        }
    });
    back.save(pack.join("img/back.png")).unwrap();
    let mask = image::RgbaImage::from_fn(500, 120, |x, y| {
        let (dx, dy) = (x as f32 - 250.0, y as f32 - 60.0);
        if dx * dx + dy * dy < 50.0 * 50.0 {
            image::Rgba([255, 255, 255, 230])
        } else {
            image::Rgba([255, 40, 90, 140])
        }
    });
    mask.save(pack.join("img/mask.png")).unwrap();
    let left: Vec<String> = (1000..1900).step_by(60).map(|f| f.to_string()).collect();
    let down: Vec<String> = (1030..1900).step_by(120).map(|f| f.to_string()).collect();
    let html = format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Layer test</title></head><body>\n\
         <input type=\"hidden\" name=\"dos\" id=\"dos\" value='\n\
         |musicTitle=Layer Test,DitoDitoInfinito test,,,120|\n\
         |difData=5,Easy,3.5|\n|musicUrl=layers.wav|\n|blankFrame=200|\n\
         |left_data={}|\n|down_data={}|\n\
         |back_data=200,0,../img/back.png,,0,0,500,375,1\n200,1,Layer test,,30,400,28,#ffcc00|\n\
         |mask_data=200,0,../img/mask.png,,0,380,500,120,0.8|\n\
         |word_data=200,0,歌詞のテスト|\n'>\n</body></html>\n",
        left.join(","),
        down.join(",")
    );
    std::fs::write(pack.join("danoni/layers.html"), html).unwrap();
    pack
}

/// The work's layers drawn in play as the web draws them: the back image
/// and text behind the field, the mask's band over it, read where the
/// library found them; its lyrics over the field (the 2D pass, the
/// system's fonts). Skipped where there is no GPU adapter (CI).
/// `DDI_OFFSCREEN_PPM=<dir>` keeps the picture (`works-layers.ppm`), to
/// set beside Chrome's of the same pack at 2 s.
#[test]
fn a_works_layers_show_in_play() {
    common::indexes();
    use ddi_desktop::play::view::{Formats, View};
    let instance = ddi_desktop::gpu::instance();
    let Some(adapter) = ddi_desktop::gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = ddi_desktop::gpu::device(&adapter).unwrap();
    let dir = std::env::temp_dir().join(format!("ddi-works-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let pack = layer_pack(&dir);
    let (songs, report) = ddi_desktop::library::scan_folders(std::slice::from_ref(&pack));
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(songs.len(), 1);
    let s = &songs[0];
    // The web's id for the pack picked as a folder (as Chrome's import
    // gave it).
    assert_eq!(s.entry.id, "u-5de60f272f35fdf2");
    assert_eq!(
        s.entry.id,
        ddi_library::pack::song_id("LayerPack", "danoni/layers.html#0")
    );
    assert_eq!(s.entry.layer_images, ["../img/back.png", "../img/mask.png"]);
    ddi_desktop::library::register_for_tests(&songs);
    let loaded = Rc::new(load::load(&s.entry.id, 48_000).unwrap());
    assert_eq!(loaded.layers.len(), 2);
    let settings = Settings::default();
    const SIZE: [u32; 2] = [1280, 720];
    let mut view = View::new(
        &device,
        &queue,
        Formats::of(wgpu::TextureFormat::Rgba8Unorm, true),
    );
    view.set_song(&loaded, &settings);
    view.set_noteskin(
        &device,
        &queue,
        ddi_desktop::play::host::noteskin(&loaded, 0, &settings),
    );
    let contents = ddi_desktop::play::stage::contents(&loaded.song, 0, &loaded.layers);
    assert_eq!(contents.len(), 3, "two images and a text");
    for read in ddi_desktop::play::stage::read(contents, None) {
        view.add_stage_content(read);
    }
    view.set_stage_track(Some(ddi_engine::sprites::SpriteTrack::new(
        &loaded.song,
        0,
        false,
    )));
    view.set_lyrics(Some(ddi_engine::lyrics::LyricTrack::new(
        &loaded.song,
        0,
        false,
        true,
    )));
    view.wait_for_fonts();
    view.start();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let mut play = Play::start(
        Request::song(&s.entry.id, 0, true),
        loaded,
        &settings,
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )
    .unwrap();
    let render = ddi_shell::render_options(&settings);
    let step = 1_000_000_000 / 60;
    let pixels = loop {
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        let now = HostTime(now as f64 / 1e9);
        play.tick(now, None);
        let t = play.session.player.clock().heard_now(now);
        let mut encoder = device.create_command_encoder(&Default::default());
        view.draw(
            &device,
            &queue,
            &mut encoder,
            &srgb,
            Some(&plain),
            SIZE,
            &play,
            render,
            now,
        );
        if t >= 2.0 {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(SIZE[0] * 4 * SIZE[1]),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(SIZE[0] * 4),
                        rows_per_image: Some(SIZE[1]),
                    },
                },
                texture.size(),
            );
            queue.submit(Some(encoder.finish()));
            let slice = buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            break slice.get_mapped_range().expect("mapped").to_vec();
        }
        queue.submit(Some(encoder.finish()));
    };
    assert_eq!(view.stage_drawn(), (2, 1), "back: image and text; mask");
    // The lyrics over the field, in the system's Japanese face.
    assert_eq!(view.lyrics_shown(), ["歌詞のテスト"]);
    if let Some(out) = std::env::var_os("DDI_OFFSCREEN_PPM") {
        let mut ppm = format!("P6\n{} {}\n255\n", SIZE[0], SIZE[1]).into_bytes();
        for p in pixels.chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(std::path::Path::new(&out).join("works-layers.ppm"), ppm).unwrap();
    }
    let at = |x: u32, y: u32| {
        let i = ((y * SIZE[0] + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    // The back's green square (left of the field), the mask's white disc
    // and pink band (at the bottom), the back's gradient at the stage's
    // right edge.
    let green = at(380, 100);
    assert!(green[1] > 180 && green[0] < 80, "back's square {green:?}");
    let disc = at(640, 640);
    assert!(disc.iter().all(|&c| c > 150), "mask's disc {disc:?}");
    let band = at(900, 700);
    assert!(band[0] > 90 && band[1] < 60, "mask's band {band:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
