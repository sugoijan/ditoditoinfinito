//! Scripted backgrounds and foregrounds on desktop
//! (`docs/plans/scripted-backgrounds.md`): a song folder made here with a
//! `#BGCHANGES` and an `#FGCHANGES` naming folders with a `default.lua`,
//! played autoplayed on a fake clock with no sound device, drawn
//! offscreen through the play's view, pixels left of the field read back
//! at chosen song times. Skipped without a GPU adapter.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::gpu;
use ddi_desktop::library::files::{DirReader, walk};
use ddi_desktop::play::load::{Files, Loaded};
use ddi_desktop::play::view::{Formats, Rgba, View};
use ddi_desktop::play::{Feed, Play, Request};
use ddi_platform::HostTime;
use ddi_platform::decode_all::Range;
use ddi_ui::state::settings::{BackgroundFrame, Settings};

const W: u32 = 640;
const H: u32 = 360;

/// 120 BPM: beat `b` at `b / 2` s. The song background (blue) until 2 s,
/// then the `BG` script: a green quad over the whole screen and the
/// folder's red star, four times its size, left of the field; from 6 s,
/// the `FG` script: a white square at the screen's left edge for a
/// second (`sleep`), then nothing. `Stop` runs away when it comes on at
/// 10 s: every script stops, the song background shows.
fn simfile(stop: bool) -> String {
    let stop = if stop { ",20=Stop=1=0=0=0" } else { "" };
    format!(
        "#TITLE:Scripts;#ARTIST:Test;#MUSIC:music.wav;#OFFSET:0;#BPMS:0=120;\
         #BACKGROUND:bg.png;\
         #BGCHANGES:4=BG=1=0=0=0{stop},99999=-nosongbg-;#FGCHANGES:12=FG=1=0=0=0;\
         #NOTES:dance-single:x:Easy:1:0,0,0,0,0:\
         1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
         1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
         1000\n0100\n0010\n0001\n;"
    )
}

const BG: &str = "return Def.ActorFrame {\n\
    Def.Quad { OnCommand = cmd(FullScreen; diffuse, color(\"0,0.8,0,1\")) },\n\
    LoadActor(\"star.png\") .. { OnCommand = cmd(xy, 100, 240; zoom, 4) },\n\
    Def.BitmapText { Font = \"Common Normal\", Text = \"TEXT\",\n\
      OnCommand = cmd(xy, 120, 400; zoom, 2; diffuse, color(\"1,1,0,1\")) },\n\
}\n";

const FG: &str = "return Def.Quad {\n\
    OnCommand = cmd(zoomto, 80, 80; xy, 40, 240; sleep, 1),\n\
}\n";

const STOP: &str = "return Def.ActorFrame {\n\
    OnCommand = function(self) self:SetUpdateFunction(function() while true do end end) end,\n\
}\n";

/// The song folder under `dir`.
fn song(dir: &Path, stop: bool) -> Option<()> {
    let song = dir.join("Pack/Scripts");
    for (folder, script) in [("BG", BG), ("FG", FG), ("Stop", STOP)] {
        std::fs::create_dir_all(song.join(folder)).ok()?;
        std::fs::write(song.join(folder).join("default.lua"), script).ok()?;
    }
    std::fs::write(song.join("song.sm"), simfile(stop)).ok()?;
    for (name, rgba) in [
        ("bg.png", [30, 30, 200, 255]),
        ("BG/star.png", [200, 30, 30, 255]),
    ] {
        image::RgbaImage::from_pixel(16, 16, image::Rgba(rgba))
            .save(song.join(name))
            .ok()?;
    }
    // 16 s of 16-bit stereo silence.
    let rate = 44_100u32;
    let data = vec![0u8; (rate * 4 * 16) as usize];
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    std::fs::write(song.join("music.wav"), wav).ok()?;
    Some(())
}

/// The pixels read: left of the field's middle (20, 180) and on the star
/// (75, 180).
const POINTS: [(u32, u32); 2] = [(20, 180), (75, 180)];

/// Plays the song under `root`, a frame each 1/60 s, and reads
/// [`POINTS`] at the first frame at or after each of `times` (song
/// seconds); with the results' note on the scripts, if any.
#[allow(clippy::type_complexity)]
fn play(
    adapter: &wgpu::Adapter,
    root: &Path,
    times: &[f64],
) -> (Vec<(f64, [[u8; 3]; 2], usize)>, Option<String>) {
    let paths: Vec<_> = walk(root).listed.into_iter().map(|l| l.path).collect();
    let reader = DirReader {
        root: root.to_path_buf(),
    };
    let scan =
        ddi_library::scan::scan_root(&reader, &paths, "Mine", &[], &ddi_library::legacy::none);
    let s = &scan.songs[0];
    assert_eq!(
        s.entry.bg_scripts.len(),
        if s.entry
            .bg_scripts
            .iter()
            .any(|f| f.as_str().starts_with("Stop"))
        {
            4
        } else {
            3
        },
        "{:?}",
        s.entry.bg_scripts
    );
    let files = Files::Dir {
        root: root.to_path_buf(),
        dir: s.dir.clone(),
    };
    let text = ddi_library::pack::decode_text(&files.read(&s.entry.chart).unwrap());
    let parsed = ddi_chart::formats::sm::parse_simfile(&text, "sm").unwrap();
    let music = ddi_desktop::audio::decoder::sound(
        files.read(&s.entry.music).unwrap(),
        Some("wav"),
        Range::default(),
        48_000,
    )
    .unwrap();
    let loaded = Rc::new(Loaded {
        entry: s.entry.clone(),
        song: parsed,
        files,
        music,
        movies: Vec::new(),
        layers: Vec::new(),
    });
    let (device, queue) = gpu::device(adapter).unwrap();
    let settings = Settings {
        noteskin: Some("builtin".into()),
        bg_brightness: 1.0,
        field_filter: 0.0,
        background_frame: BackgroundFrame::SixteenNine,
        ..Settings::default()
    };
    let mut view = View::new(
        &device,
        &queue,
        Formats::of(wgpu::TextureFormat::Rgba8Unorm, true),
    );
    view.set_song(&loaded, &settings);
    assert!(view.scripts_running());
    for (what, source, side) in View::images_to_read(&loaded, &settings) {
        let (size, data) = ddi_desktop::images::load(&source, side).expect("an image");
        view.add_background(what, Rgba { size, data });
    }
    view.start();
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
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: W,
            height: H,
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
    let render = ddi_shell::render_options(&settings);
    let mut out = Vec::new();
    let mut next = 0;
    for _ in 0..60 * 30 {
        if next == times.len() {
            break;
        }
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        let now = HostTime(now as f64 / 1e9);
        play.tick(now, None);
        let t = play
            .session
            .player
            .frame(play.session.predicted_present(now))
            .song_time;
        let mut encoder = device.create_command_encoder(&Default::default());
        view.draw(
            &device,
            &queue,
            &mut encoder,
            &srgb,
            Some(&plain),
            [W, H],
            &play,
            render,
            now,
        );
        if t >= times[next] {
            next += 1;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(W * 4 * H),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(W * 4),
                        rows_per_image: Some(H),
                    },
                },
                texture.size(),
            );
            queue.submit(Some(encoder.finish()));
            let slice = buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let pixels = slice.get_mapped_range().expect("mapped").to_vec();
            let at = |(x, y): (u32, u32)| {
                let i = ((y * W + x) * 4) as usize;
                [pixels[i], pixels[i + 1], pixels[i + 2]]
            };
            // The script's yellow text, left of the field's bottom half.
            let yellow = (270..330)
                .flat_map(|y| (30..160).map(move |x| (x, y)))
                .filter(|&p| {
                    let [r, g, b] = at(p);
                    r > 150 && g > 150 && b < 90
                })
                .count();
            out.push((t, POINTS.map(at), yellow));
        } else {
            queue.submit(Some(encoder.finish()));
        }
    }
    (out, view.scripts_stopped().map(str::to_string))
}

fn near(got: [u8; 3], want: [u8; 3]) -> bool {
    got.iter()
        .zip(want)
        .all(|(&g, w)| (i32::from(g) - i32::from(w)).abs() <= 3)
}

const BLUE: [u8; 3] = [30, 30, 200];
const GREEN: [u8; 3] = [0, 204, 0];
const RED: [u8; 3] = [200, 30, 30];
const WHITE: [u8; 3] = [255, 255, 255];

#[test]
fn scripts_draw_behind_the_field_and_over_it() {
    let dir = std::env::temp_dir().join(format!("ddi-play-scripts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let plain = dir.join("plain");
    song(&plain, false).expect("the song folder");
    let (samples, stopped) = play(&adapter, &plain, &[1.0, 3.0, 6.5, 7.5]);
    assert_eq!(stopped, None);
    let want = [
        [BLUE, BLUE],
        // The script's screen: green, the star on it.
        [GREEN, RED],
        // The foreground's white square over the field's left.
        [WHITE, RED],
        // Its second is over.
        [GREEN, RED],
    ];
    assert_eq!(samples.len(), want.len(), "{samples:?}");
    for ((t, got, _), want) in samples.iter().zip(want) {
        for (g, w) in got.iter().zip(want) {
            assert!(near(*g, w), "at {t:.3} s: {got:?}, want {want:?}");
        }
    }
    // The script's text (`BitmapText`), yellow, with the script only.
    assert_eq!(samples[0].2, 0, "{samples:?}");
    assert!(samples[1].2 > 40, "{samples:?}");
    // A script that runs away stops them all: the song background, and a
    // note on the results.
    let stop = dir.join("stop");
    song(&stop, true).expect("the song folder");
    let (samples, stopped) = play(&adapter, &stop, &[9.5, 10.5]);
    assert!(near(samples[0].1[0], GREEN), "{samples:?}");
    assert!(near(samples[1].1[0], BLUE), "{samples:?}");
    assert!(stopped.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
