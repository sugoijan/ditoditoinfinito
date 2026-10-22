//! A song folder made by a test, played autoplayed on a fake clock with
//! no sound device and drawn offscreen through the play's view, its
//! pixels read back at chosen song times (`play_layers`, `play_scripts`).

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::gpu;
use ddi_desktop::library::files::{DirReader, walk};
use ddi_desktop::play::load::{Files, Loaded};
use ddi_desktop::play::scripts::ScriptLoad;
use ddi_desktop::play::view::{Formats, Rgba, View};
use ddi_desktop::play::{Feed, Play, Request};
use ddi_platform::HostTime;
use ddi_platform::decode_all::Range;
use ddi_render::RenderOptions;
use ddi_ui::state::settings::Settings;

/// The target's size.
pub const W: u32 = 640;
pub const H: u32 = 360;

/// The song under `root` (its only one), scanned as the library scans
/// it and loaded as a play loads it, its movies with it.
pub fn load(root: &Path) -> Rc<Loaded> {
    let paths: Vec<_> = walk(root).listed.into_iter().map(|l| l.path).collect();
    let reader = DirReader {
        root: root.to_path_buf(),
    };
    let scan =
        ddi_library::scan::scan_root(&reader, &paths, "Mine", &[], &ddi_library::legacy::none);
    let s = &scan.songs[0];
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
    let movies = ddi_desktop::play::load::movies(&files, &s.entry);
    Rc::new(Loaded {
        entry: s.entry.clone(),
        song: parsed,
        files,
        music,
        movies,
        layers: Vec::new(),
    })
}

/// A play of a loaded song, drawn offscreen at [`W`] × [`H`].
pub struct OffscreenPlay {
    pub view: View,
    pub play: Play,
    device: wgpu::Device,
    queue: wgpu::Queue,
    clock: Arc<AtomicU64>,
    texture: wgpu::Texture,
    render: RenderOptions,
}

impl OffscreenPlay {
    /// `loaded` autoplayed with `settings`: its view given the song and
    /// its pictures (read here, as the host's threads read them), the
    /// play started on a fake clock.
    pub fn new(adapter: &wgpu::Adapter, loaded: Rc<Loaded>, settings: &Settings) -> OffscreenPlay {
        let (device, queue) = gpu::device(adapter).unwrap();
        let mut view = View::new(
            &device,
            &queue,
            Formats::of(wgpu::TextureFormat::Rgba8Unorm, true),
        );
        view.set_song(&loaded, settings);
        // Its scripts, readied to their end as the host readies them.
        if let Some(scripts) = ScriptLoad::start(&loaded, settings, None) {
            view.set_scripts(scripts.wait());
        }
        for (what, source, side) in View::images_to_read(&loaded, settings) {
            let (size, data) = ddi_desktop::images::load(&source, side).expect("an image");
            view.add_background(what, Rgba { size, data });
        }
        view.start();
        let clock = Arc::new(AtomicU64::new(1_000_000_000));
        let c = clock.clone();
        let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
        let play = Play::start(
            Request::song(&loaded.entry.id, 0, true),
            loaded,
            settings,
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
        OffscreenPlay {
            view,
            play,
            device,
            queue,
            clock,
            texture,
            render: ddi_shell::render_options(settings),
        }
    }

    /// A frame each 1/60 s for up to 30 s, `each` given the view and the
    /// play before each is drawn; at the first frame at or after each of
    /// `times` (song seconds), `read` given the time drawn and the
    /// frame's pixels (RGBA rows, [`pixel`]).
    pub fn run(
        &mut self,
        times: &[f64],
        mut each: impl FnMut(&mut View, &mut Play),
        mut read: impl FnMut(f64, &[u8]),
    ) {
        let (device, queue, texture) = (&self.device, &self.queue, &self.texture);
        let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });
        let plain = texture.create_view(&Default::default());
        let mut next = 0;
        for _ in 0..60 * 30 {
            if next == times.len() {
                break;
            }
            let now =
                self.clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            let now = HostTime(now as f64 / 1e9);
            self.play.tick(now, None);
            let t = self
                .play
                .session
                .player
                .frame(self.play.session.predicted_present(now))
                .song_time;
            let mut encoder = device.create_command_encoder(&Default::default());
            each(&mut self.view, &mut self.play);
            self.view.draw(
                device,
                queue,
                &mut encoder,
                &srgb,
                Some(&plain),
                [W, H],
                &self.play,
                self.render,
                now,
            );
            if t < times[next] {
                queue.submit(Some(encoder.finish()));
                continue;
            }
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
            read(t, &slice.get_mapped_range().expect("mapped"));
        }
    }
}

/// The color at `(x, y)` of a frame's `pixels` ([`OffscreenPlay::run`]).
pub fn pixel(pixels: &[u8], (x, y): (u32, u32)) -> [u8; 3] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}
