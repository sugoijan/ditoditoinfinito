use super::*;
use crate::video::{Backend, MovieFile, VideoHost};
use ddi_platform::video::{
    VideoBackend, VideoCodec, VideoContainer, VideoDecoder, VideoEvent, VideoFormat,
};
fn readers() -> Vec<usize> {
    CACHES.with(|c| {
        c.borrow()
            .iter()
            .flat_map(|c| c.owners.values().map(|o| o.readers.load(Ordering::Acquire)))
            .collect()
    })
}
#[test]
#[ignore = "strict opt-in Metal play ownership, budget and device-loss test"]
fn retained_play_frames_obey_gpu_leases_and_fallback() {
    assert!(
        requested(),
        "set DDI_VIDEO_PLANES=metal before the test process starts"
    );
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch/video-gpu/leases");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("clip.mp4");
    assert!(
        std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=128x96:rate=30",
                "-t",
                "1",
                "-c:v",
                "libx264",
                "-bf",
                "0",
                "-pix_fmt",
                "yuv420p"
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let mut host = VideoHost::with_backend(crate::video::module_path(), Backend::VideoToolbox);
    let mut video = host.open(
        MovieFile::from(path),
        "clip.mp4",
        &VideoFormat {
            container: VideoContainer::Unknown,
            codec: VideoCodec::Unknown,
        },
    );
    video.want(1);
    let start = std::time::Instant::now();
    let frame = loop {
        let mut events = Vec::new();
        video.poll(&mut events);
        if let Some(f) = events.into_iter().find_map(|e| match e {
            VideoEvent::Frame(f) => Some(f),
            VideoEvent::Error(e) => panic!("{e}"),
            _ => None,
        }) {
            break f;
        }
        assert!(start.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(frame.external.as_ref().unwrap().0.is::<Surface>());
    // Plain I420 diagnostics still see the original decoded pixels.
    let copied = crate::video::copy_to_cpu(frame.clone()).unwrap();
    assert!(copied.split().is_some());
    let instance = crate::gpu::instance();
    let adapter = crate::gpu::adapter(&instance, None).expect("GPU required");
    let (device, queue) = crate::gpu::device(&adapter).unwrap();
    let mut renderer = ddi_render::Renderer::new(
        &device,
        &queue,
        wgpu::TextureFormat::Rgba8Unorm,
        ddi_render::SkinBlending::Scene,
        &[],
    );
    for _ in 0..LIMIT {
        let id = renderer.add_video(&device, 128, 96);
        assert!(show(&mut renderer, &device, &queue, id, &frame));
    }
    assert_eq!(stats().imported, LIMIT as u64);
    assert_eq!(stats().retained, LIMIT);
    let id = renderer.add_video(&device, 128, 96);
    assert!(show(&mut renderer, &device, &queue, id, &frame));
    assert_eq!(
        stats().copied,
        1,
        "budget pressure uses existing copied planes"
    );
    assert!(!stats().disabled);
    // A paused/cached picture remains protected each time it is encoded.
    let abandoned = device.create_command_encoder(&Default::default());
    protect(&device, &abandoned);
    assert!(readers().iter().all(|n| *n == 1));
    drop(abandoned);
    assert!(
        readers().iter().all(|n| *n == 0),
        "abandoned encoder releases unsubmitted leases"
    );
    let encoder = device.create_command_encoder(&Default::default());
    protect(&device, &encoder);
    assert!(readers().iter().all(|n| *n == 1));
    queue.submit(Some(encoder.finish()));
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(readers().iter().all(|n| *n == 0));
    assert_eq!(
        stats().retained,
        LIMIT,
        "cached textures still own their CV views"
    );
    drop(renderer);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(
        stats().retained,
        0,
        "normal retirement returns the whole global budget"
    );
    // Lose a private logical device without waiting. Unconfirmed owners stay
    // bounded and all subsequent rendering uses copies on a fresh device.
    let mut renderer = ddi_render::Renderer::new(
        &device,
        &queue,
        wgpu::TextureFormat::Rgba8Unorm,
        ddi_render::SkinBlending::Scene,
        &[],
    );
    let id = renderer.add_video(&device, 128, 96);
    assert!(show(&mut renderer, &device, &queue, id, &frame));
    let encoder = device.create_command_encoder(&Default::default());
    protect(&device, &encoder);
    queue.submit(Some(encoder.finish()));
    device.destroy();
    drop(renderer);
    let after = stats();
    assert!(after.retained <= LIMIT);
    let (new, queue) = crate::gpu::device(&adapter).unwrap();
    let mut renderer = ddi_render::Renderer::new(
        &new,
        &queue,
        wgpu::TextureFormat::Rgba8Unorm,
        ddi_render::SkinBlending::Scene,
        &[],
    );
    let id = renderer.add_video(&new, 128, 96);
    assert!(show(&mut renderer, &new, &queue, id, &frame));
    if after.disabled {
        assert!(
            stats().copied >= 2,
            "unconfirmed loss forces copied recovery"
        );
    }
    assert!(stats().retained <= LIMIT);
    assert!(stats().bytes <= BYTES);
    // Deterministic missing-completion fault: unlike logical device loss on
    // this driver, this must exercise quarantine and disable sharing globally.
    CACHES.with(|c| {
        for cache in c.borrow_mut().iter_mut().filter(|c| c.device == new) {
            cache.force_unconfirmed = true;
        }
    });
    let before = stats();
    assert!(before.disabled);
    assert!(before.retained <= LIMIT);
    let id = renderer.add_video(&new, 128, 96);
    assert!(show(&mut renderer, &new, &queue, id, &frame));
    assert!(
        stats().copied > before.copied,
        "unconfirmed completion forces copied recovery"
    );
    eprintln!("Metal leases/budget/loss: {:?}", stats());
}
