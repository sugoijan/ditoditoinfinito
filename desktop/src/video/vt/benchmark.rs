//! Explicit offscreen experiment: actual VT callback and background renderer.
//! No window, sound, library imports or saved settings. Never used by play.
use super::*;
use crate::video::{MovieFile, media::Media};
use ddi_render::background::{Backdrop, BackgroundPipeline};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::super::metal::{Cache as Import, Surface};
fn wait(device: &wgpu::Device, submission: wgpu::SubmissionIndex) {
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(5)),
        })
        .expect("bounded GPU completion");
}
struct Scene {
    pass: BackgroundPipeline,
    id: usize,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
}
impl Scene {
    fn new(device: &wgpu::Device, size: [u32; 2], metal: bool, srgb: bool) -> Self {
        let format = if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let mut pass = if metal {
            BackgroundPipeline::new_nv12_benchmark(device, format, srgb)
        } else {
            BackgroundPipeline::new(device, format, srgb)
        };
        let id = pass.add_video(device, size[0], size[1]);
        if metal {
            let plane = |format, w, h| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
            };
            let y = plane(wgpu::TextureFormat::R8Unorm, size[0], size[1]);
            let uv = plane(wgpu::TextureFormat::Rg8Unorm, size[0] / 2, size[1] / 2);
            pass.set_nv12_benchmark(device, id, &y, &uv, false, false);
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ddi-bench-target"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Self {
            pass,
            id,
            texture,
            view,
            size,
        }
    }
    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        capture: bool,
        import: Option<&mut Import>,
    ) -> Pending {
        let p = self.pass.prepare(
            device,
            queue,
            self.size[0] as f32,
            self.size[1] as f32,
            1.0,
            Backdrop::image(self.id),
            None,
            None,
        );
        let mut encoder = device.create_command_encoder(&Default::default());
        if let Some(import) = import {
            import.protect(&encoder);
        }
        {
            let mut r = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ddi-bench-draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.pass.draw(&mut r, &p);
        }
        let readback = capture.then(|| Readback::new(device, &self.texture, &mut encoder));
        Pending {
            submission: queue.submit(Some(encoder.finish())),
            readback,
            id: 0,
        }
    }
}
struct Readback {
    buffer: wgpu::Buffer,
    pitch: u32,
    width: u32,
}
impl Readback {
    fn new(
        device: &wgpu::Device,
        texture: &wgpu::Texture,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Self {
        let pitch = (texture.width() * 4).next_multiple_of(256);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(pitch * texture.height()),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(texture.height()),
                },
            },
            texture.size(),
        );
        Self {
            buffer,
            pitch,
            width: texture.width(),
        }
    }
    fn hash(&self, device: &wgpu::Device) -> u32 {
        let slice = self.buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(5)),
            })
            .unwrap();
        let mapped = slice.get_mapped_range().unwrap();
        let bytes: Vec<_> = mapped
            .chunks(self.pitch as usize)
            .flat_map(|r| r[..self.width as usize * 4].iter().copied())
            .collect();
        crate::video::crc32(&bytes)
    }
}
struct Pending {
    submission: wgpu::SubmissionIndex,
    readback: Option<Readback>,
    id: usize,
}
fn retire(
    p: Pending,
    device: &wgpu::Device,
    import: &mut Import,
    stats: &mut Stats,
    hashes: &mut Vec<(usize, u32)>,
) {
    let start = Instant::now();
    wait(device, p.submission);
    stats.wait += start.elapsed().as_secs_f64();
    if let Some(r) = p.readback {
        hashes.push((p.id, r.hash(device)));
    }
    import.collect().unwrap();
}

fn clip(name: &str, size: &str, codec: &str, extra: &[&str]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/scratch/video-gpu");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut c = std::process::Command::new("ffmpeg");
    c.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc2=size={size}:rate=60"),
        "-t",
        "2",
        "-c:v",
        codec,
        "-pix_fmt",
        "yuv420p",
        "-g",
        "60",
    ]);
    match codec {
        "libx264" => {
            c.args(["-bf", "3", "-x264-params", "open-gop=0"]);
        }
        "libx265" => {
            c.args([
                "-x265-params",
                "pools=1:frame-threads=1:log-level=error:open-gop=0",
            ]);
        }
        _ => {
            c.args([
                "-cpu-used",
                "4",
                "-lag-in-frames",
                "0",
                "-auto-alt-ref",
                "0",
            ]);
        }
    }
    assert!(c.args(extra).arg(&path).status().unwrap().success());
    path
}
fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = crate::gpu::adapter(&instance, None).expect("Metal adapter required");
    static LOG: std::sync::Once = std::sync::Once::new();
    LOG.call_once(|| eprintln!("adapter: {:?}", adapter.get_info()));
    crate::gpu::device(&adapter).unwrap()
}
fn cpu() -> f64 {
    let mut r = std::mem::MaybeUninit::<libc::rusage>::uninit();
    assert_eq!(
        unsafe { libc::getrusage(libc::RUSAGE_SELF, r.as_mut_ptr()) },
        0
    );
    let r = unsafe { r.assume_init() };
    r.ru_utime.tv_sec as f64
        + r.ru_utime.tv_usec as f64 * 1e-6
        + r.ru_stime.tv_sec as f64
        + r.ru_stime.tv_usec as f64 * 1e-6
}
#[derive(Default)]
struct Stats {
    frames: usize,
    upload: f64,
    draw: f64,
    wait: f64,
    wall: f64,
    cpu: f64,
}
fn run(
    path: &Path,
    metal: bool,
    validate: bool,
    srgb: bool,
    seek: Option<f64>,
    depth: usize,
) -> (Vec<(usize, u32)>, Stats) {
    let (device, queue) = device();
    let mut media = Media::open(
        &MovieFile::from(path.to_path_buf()),
        path.file_name().unwrap().to_str().unwrap(),
        crate::video::module_path(),
    )
    .unwrap();
    if let Some(t) = seek {
        media.seek(t).unwrap();
    }
    let f = &media.format;
    let session = Session::new_output(f, 0, metal).unwrap();
    let mut order = f.order();
    let mut scene = Scene::new(&device, [f.width(), f.height()], metal, srgb);
    let mut import = Import::new(&device).unwrap();
    let mut stats = Stats::default();
    let mut hashes = Vec::new();
    let mut i = 0;
    let mut eof = false;
    assert!((1..=3).contains(&depth));
    // Warm the real render pipeline before timing, including GPU completion.
    let warm = scene.draw(&device, &queue, false, None);
    wait(&device, warm.submission);
    let mut pending = VecDeque::new();
    let begin = Instant::now();
    let cpu_start = cpu();
    loop {
        if let Some(packet) = media.next().unwrap() {
            let (bytes, _) = order
                .submit(&media.format, &packet.bytes, i, media.packet_path())
                .unwrap();
            session.decode(&bytes, i, packet.presentation).unwrap();
            i += 1;
        } else if !eof {
            session.finish().unwrap();
            eof = true;
        }
        let mut q = session.callback.queue.lock().unwrap();
        if let Some(e) = q.error.take() {
            panic!("{e}");
        }
        let surfaces: Vec<_> = q.surfaces.drain(..).collect();
        let copied: Vec<_> = q.frames.drain(..).collect();
        q.bytes = 0;
        drop(q);
        for (id, image, planes) in surfaces
            .into_iter()
            .map(|(id, pb)| (id, Some(pb), None))
            .chain(copied.into_iter().map(|f| {
                let super::super::native::Pixels::Planes(p) = f.pixels else {
                    panic!("benchmark expected copied planes")
                };
                (f.id, None, Some(p))
            }))
        {
            objc2::rc::autoreleasepool(|_| {
                if pending.len() == depth {
                    retire(
                        pending.pop_front().unwrap(),
                        &device,
                        &mut import,
                        &mut stats,
                        &mut hashes,
                    );
                }
                let t = Instant::now();
                let textures = image.map(|pb| {
                    // SAFETY: this is a completed, retained VT output. The
                    // experiment never modifies published pixels/attachments.
                    let surface = unsafe {
                        Surface::completed(
                            pb,
                            media.format.width() as usize,
                            media.format.height() as usize,
                            media.format.full_range(),
                        )
                    }
                    .unwrap();
                    import.frame(&surface).unwrap()
                });
                if let Some(p) = &textures {
                    scene.pass.set_nv12_benchmark(
                        &device,
                        scene.id,
                        &p[0],
                        &p[1],
                        media.format.matrix() == ddi_platform::video::YuvMatrix::Bt709,
                        media.format.full_range(),
                    );
                } else {
                    assert!(scene.pass.set_video_frame(
                        &device,
                        &queue,
                        scene.id,
                        media.format.width(),
                        media.format.height(),
                        &planes.unwrap(),
                        media.format.matrix() == ddi_platform::video::YuvMatrix::Bt709,
                        media.format.full_range()
                    ));
                }
                stats.upload += t.elapsed().as_secs_f64();
                let t = Instant::now();
                let mut frame = scene.draw(&device, &queue, validate, metal.then_some(&mut import));
                frame.id = id;
                stats.draw += t.elapsed().as_secs_f64();
                pending.push_back(frame);
                stats.frames += 1;
                drop(textures);
                import.collect().unwrap();
                assert!(import.retained() <= 4, "surface ownership bound");
            });
        }
        if eof {
            break;
        }
    }
    // Invalidate the decoder while its last submitted surfaces may still be
    // sampled. Their independent owners must survive through GPU retirement.
    drop(session);
    while let Some(frame) = pending.pop_front() {
        retire(frame, &device, &mut import, &mut stats, &mut hashes);
    }
    stats.wall = begin.elapsed().as_secs_f64();
    stats.cpu = cpu() - cpu_start;
    assert!(stats.frames > 0);
    drop(scene);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(5)),
        })
        .unwrap();
    import.collect().unwrap();
    assert_eq!(
        import.retained(),
        0,
        "all imported owners released after completion"
    );
    device.destroy();
    hashes.sort_unstable();
    (hashes, stats)
}
#[test]
#[ignore = "explicit offscreen Metal fidelity and decoder lifetime experiment"]
fn rendered_metal_planes_match_copies() {
    let mut total = 0;
    for (codec, ext, extra) in [
        ("libx264", "mp4", vec![]),
        (
            "libx264",
            "mkv",
            vec!["-color_range", "pc", "-colorspace", "bt709"],
        ),
        ("libx265", "mp4", vec!["-colorspace", "bt709"]),
        ("libvpx-vp9", "webm", vec![]),
    ] {
        let path = clip(&format!("fidelity-{codec}.{ext}"), "640x360", codec, &extra);
        for srgb in [false, true] {
            for seek in [None, Some(0.75), Some(1.25)] {
                for depth in [1, 3] {
                    let a = run(&path, false, true, srgb, seek, depth);
                    let b = run(&path, true, true, srgb, seek, depth);
                    assert_eq!(
                        a.0, b.0,
                        "rendered identities/CRCs: {codec} {ext} srgb={srgb} seek={seek:?} depth={depth}"
                    );
                    total += a.0.len();
                }
            }
        }
    }
    eprintln!(
        "matched {total} rendered frame comparisons; both targets, seeks, 1/3 in flight, decoder invalidation and fresh devices"
    );
}
#[test]
#[ignore = "explicit offscreen full decoder-to-GPU benchmark"]
fn measure_video_gpu() {
    eprintln!(
        "GPU_BENCH,size,metal,depth,frames,cpu_ms,wall_ms,transfer_ms,draw_submit_ms,wait_ms"
    );
    for (size, name) in [
        ("640x360", "360"),
        ("1280x720", "720"),
        ("1920x1080", "1080"),
    ] {
        let path = clip(&format!("h264-{name}.mp4"), size, "libx264", &["-t", "10"]);
        for depth in [1, 3] {
            for metal in [false, true, true, false] {
                let (_, s) = run(&path, metal, false, false, None, depth);
                let n = s.frames as f64;
                eprintln!(
                    "GPU_BENCH,{name},{metal},{depth},{},{:.6},{:.6},{:.6},{:.6},{:.6}",
                    s.frames,
                    s.cpu * 1000. / n,
                    s.wall * 1000. / n,
                    s.upload * 1000. / n,
                    s.draw * 1000. / n,
                    s.wait * 1000. / n
                );
            }
        }
    }
}

#[test]
#[ignore = "explicit offscreen logical device-loss and fallback experiment"]
fn metal_device_loss_uses_fresh_resources() {
    let path = clip("loss.mp4", "640x360", "libx264", &["-bf", "0"]);
    let mut media = Media::open(
        &path.clone().into(),
        "loss.mp4",
        crate::video::module_path(),
    )
    .unwrap();
    let session = Session::new_output(&media.format, 0, true).unwrap();
    let mut order = media.format.order();
    let image = (0..16)
        .find_map(|id| {
            let p = media.next().unwrap().unwrap();
            let (bytes, _) = order
                .submit(&media.format, &p.bytes, id, media.packet_path())
                .unwrap();
            session.decode(&bytes, id, p.presentation).unwrap();
            let mut q = session.callback.queue.lock().unwrap();
            assert!(q.error.is_none());
            q.surfaces.pop_front().map(|(_, image)| image)
        })
        .expect("decoded image");
    drop(session);
    let (old, queue) = device();
    let mut import = Import::new(&old).unwrap();
    let mut scene = Scene::new(&old, [640, 360], true, false);
    for _ in 0..3 {
        let surface = unsafe { Surface::completed(image.clone(), 640, 360, false) }.unwrap();
        let textures = import.frame(&surface).unwrap();
        scene
            .pass
            .set_nv12_benchmark(&old, scene.id, &textures[0], &textures[1], false, false);
        let _ = scene.draw(&old, &queue, false, Some(&mut import));
    }
    // Logical loss on this private device only; no system-wide GPU reset.
    // There was deliberately no wait before loss or decoder invalidation.
    old.destroy();
    drop(scene);
    let _ = old.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(5)),
    });
    let _ = import.collect();
    assert!(import.retained() <= 3);
    eprintln!(
        "logical loss: {} owners awaiting confirmed retirement",
        import.retained()
    );
    drop(import);
    // A new device/cache and the existing copied-plane fallback must render
    // the same image. No old wgpu texture or cache enters the new device.
    let (new, queue) = device();
    let mut copied = Scene::new(&new, [640, 360], false, false);
    let planes = copy(&image, 640, 360, false).unwrap();
    assert!(
        copied
            .pass
            .set_video_frame(&new, &queue, copied.id, 640, 360, &planes, false, false)
    );
    let expected = copied.draw(&new, &queue, true, None);
    wait(&new, expected.submission);
    let expected = expected.readback.unwrap().hash(&new);
    if super::super::metal::stats().disabled {
        eprintln!("bounded quarantine: copied rendering on a fresh device passed");
        return;
    }
    let mut import = Import::new(&new).unwrap();
    let surface = unsafe { Surface::completed(image, 640, 360, false) }.unwrap();
    let textures = import.frame(&surface).unwrap();
    let mut shared = Scene::new(&new, [640, 360], true, false);
    shared
        .pass
        .set_nv12_benchmark(&new, shared.id, &textures[0], &textures[1], false, false);
    let got = shared.draw(&new, &queue, true, Some(&mut import));
    wait(&new, got.submission);
    assert_eq!(got.readback.unwrap().hash(&new), expected);
    drop(textures);
    drop(shared);
    new.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(5)),
    })
    .unwrap();
    import.collect().unwrap();
    assert_eq!(import.retained(), 0);
}
