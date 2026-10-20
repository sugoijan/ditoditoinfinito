//! Generate the README's gameplay still or GIF: `just screenshot` / `just animation`.
//! Runs the actual play and renderer offscreen on a fixed clock, without
//! opening a window, a sound device, or the player's settings/history.

use std::cell::RefCell;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_chart::Difficulty;
use ddi_desktop::audio::Audio;
use ddi_desktop::gpu;
use ddi_desktop::play::view::{Formats, Rgba, View};
use ddi_desktop::play::{Feed, Play, Request, host, load};
use ddi_platform::HostTime;
use ddi_ui::state::settings::Settings;
use serde::Deserialize;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    song: String,
    style: String,
    difficulty: Difficulty,
    seconds: f64,
    fps: u32,
    size: [u32; 2],
    settings: Settings,
    #[serde(default)]
    animation: Option<Animation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Animation {
    seconds: f64,
    fps: u32,
}

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() > 2 {
        return Err("usage: screenshot [scenario.json] [output.png|output.gif]".into());
    }
    let scenario = root.join(args.first().map_or_else(
        || PathBuf::from("docs/screenshots/readme.json"),
        PathBuf::from,
    ));
    let scenario: Scenario = serde_json::from_str(&std::fs::read_to_string(scenario)?)?;
    let extension = if scenario.animation.is_some() {
        "gif"
    } else {
        "png"
    };
    let output = root.join(args.get(1).map_or_else(
        || PathBuf::from(format!("docs/screenshots/readme.{extension}")),
        PathBuf::from,
    ));
    if output.extension().is_none_or(|ext| ext != extension) {
        return Err(format!("this scenario needs a .{extension} output path").into());
    }
    if !scenario.seconds.is_finite()
        || !(0.0..=600.0).contains(&scenario.seconds)
        || !(1..=240).contains(&scenario.fps)
        || scenario.size.iter().any(|v| !(64..=4096).contains(v))
    {
        return Err(
            "scenario needs seconds in 0..=600, fps in 1..=240, and dimensions in 64..=4096".into(),
        );
    }
    let (count, stride, duration) = if let Some(animation) = &scenario.animation {
        if !animation.seconds.is_finite()
            || !(0.0..=10.0).contains(&animation.seconds)
            || animation.seconds == 0.0
            || animation.fps == 0
            || !scenario.fps.is_multiple_of(animation.fps)
            || !100_u32.is_multiple_of(animation.fps)
        {
            return Err("animation needs seconds in (0, 10] and fps dividing both the simulation fps and 100 (GIF centiseconds)".into());
        }
        (
            (animation.seconds * f64::from(animation.fps)).ceil() as u64,
            u64::from(scenario.fps / animation.fps),
            animation.seconds,
        )
    } else {
        (1, 1, 0.0)
    };
    if !ddi_desktop::library::bundled()
        .iter()
        .any(|s| s.id == scenario.song)
    {
        return Err(
            "scenario song must be bundled; run `just screenshot` to generate the indexes".into(),
        );
    }
    let loaded = Rc::new(load::load(&scenario.song, 48_000)?);
    if loaded.entry.background.is_none() || scenario.settings.bg_brightness <= 0.0 {
        return Err("the screenshot scenario must show a song background".into());
    }
    let chart = loaded
        .song
        .charts
        .iter()
        .position(|c| c.layout == scenario.style && c.difficulty == scenario.difficulty)
        .ok_or("scenario chart is not in the song")?;
    let settings = Settings {
        style: scenario.style.clone(),
        volume: 0.0,
        ..scenario.settings
    };
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));

    let instance = gpu::instance();
    let adapter =
        gpu::adapter(&instance, None).ok_or("a GPU adapter is needed for the screenshot")?;
    let (device, queue) = gpu::device(&adapter)?;
    let plain_format = wgpu::TextureFormat::Rgba8Unorm;
    let mut view = View::new(&device, &queue, Formats::of(plain_format, true));
    view.set_song(&loaded, &settings);
    view.set_noteskin(&device, &queue, host::noteskin(&loaded, chart, &settings));
    for (what, source, side) in View::images_to_read(&loaded, &settings) {
        let (size, data) = ddi_desktop::images::load(&source, side)
            .ok_or_else(|| format!("cannot load scenario image {source}"))?;
        view.add_background(what, Rgba { size, data });
    }
    view.start();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("readme screenshot"),
        size: wgpu::Extent3d {
            width: scenario.size[0],
            height: scenario.size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: plain_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    let plain = texture.create_view(&Default::default());
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    // Integer nanoseconds derived from the frame number, with no sleeps,
    // elapsed wall time, or accumulated floating-point steps.
    const START: u64 = 1_000_000_000;
    let clock = Arc::new(AtomicU64::new(START));
    let audio_clock = clock.clone();
    let audio = Audio::headless(
        0.0,
        Some(Arc::new(move || audio_clock.load(Ordering::SeqCst))),
    );
    let mut play = Play::start(
        Request::song(&scenario.song, chart, true),
        loaded,
        &settings,
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        f64::from(scenario.fps),
    )?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut capture = Capture::new(&output, scenario.size, scenario.animation.as_ref())?;
    let render = ddi_shell::render_options(&settings);
    let last = ((scenario.seconds + duration + 5.0) * f64::from(scenario.fps)).ceil() as u64;
    let mut captured = 0;
    let mut first = None;
    for frame in 1..=last {
        let nanos = START + frame * 1_000_000_000 / u64::from(scenario.fps);
        clock.store(nanos, Ordering::SeqCst);
        let now = HostTime(nanos as f64 / 1e9);
        if play.tick(now, None).is_some() {
            return Err("the play ended before the capture time".into());
        }
        let at = play.session.player.clock().heard_now(now);
        let mut encoder = device.create_command_encoder(&Default::default());
        view.draw(
            &device,
            &queue,
            &mut encoder,
            &srgb,
            Some(&plain),
            scenario.size,
            &play,
            render,
            now,
        );
        queue.submit(Some(encoder.finish()));
        if at >= scenario.seconds {
            let first = *first.get_or_insert(frame);
            if !(frame - first).is_multiple_of(stride) {
                continue;
            }
            if let Some(skin) = view.noteskin()
                && let Some(why) = skin.lua_failure()
            {
                return Err(format!("noteskin stopped: {why}").into());
            }
            let pixels = read(&device, &queue, &texture, scenario.size)?;
            capture.write(&pixels, scenario.size)?;
            captured += 1;
            if captured == count {
                capture.finish()?;
                println!(
                    "{}: {} / {} {:?}, {captured} captured frame(s), ending at song second {at:.6}, simulation frame {frame}, combo {}",
                    output.display(),
                    scenario.song,
                    scenario.style,
                    scenario.difficulty,
                    play.session.player.combo()
                );
                return Ok(());
            }
        }
    }
    Err("the audio clock did not reach the capture time".into())
}

enum Capture {
    Png(PathBuf),
    Gif(Child),
}

impl Capture {
    fn new(output: &Path, [w, h]: [u32; 2], animation: Option<&Animation>) -> Result<Self> {
        let Some(animation) = animation else {
            return Ok(Self::Png(output.to_path_buf()));
        };
        // One palette for the whole clip and fixed ordered dithering keep the
        // beach stable. FFmpeg stores only changed rectangles between frames.
        let child = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "rawvideo", "-pixel_format", "rgba", "-video_size"])
            .arg(format!("{w}x{h}"))
            .arg("-framerate")
            .arg(animation.fps.to_string())
            .args(["-i", "pipe:0", "-filter_complex"])
            .arg("split[frames][colors];[colors]palettegen=stats_mode=diff[palette];[frames][palette]paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle")
            .args(["-loop", "0", "-f", "gif"])
            .arg(output)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("GIF generation requires ffmpeg on PATH: {e}"))?;
        Ok(Self::Gif(child))
    }

    fn write(&mut self, pixels: &[u8], [w, h]: [u32; 2]) -> Result<()> {
        match self {
            Self::Png(output) => image::save_buffer_with_format(
                output,
                pixels,
                w,
                h,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )?,
            Self::Gif(child) => child
                .stdin
                .as_mut()
                .ok_or("GIF input closed")?
                .write_all(pixels)?,
        }
        Ok(())
    }

    fn finish(self) -> Result<()> {
        if let Self::Gif(mut child) = self {
            drop(child.stdin.take());
            if !child.wait()?.success() {
                return Err("ffmpeg could not encode the GIF".into());
            }
        }
        Ok(())
    }
}

fn read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    [w, h]: [u32; 2],
) -> Result<Vec<u8>> {
    let row = w * 4;
    let padded =
        row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot readback"),
        size: u64::from(padded * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        texture.size(),
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely())?;
    let data = slice.get_mapped_range()?;
    Ok(data
        .chunks_exact(padded as usize)
        .flat_map(|row| row[..w as usize * 4].iter().copied())
        .collect())
}
