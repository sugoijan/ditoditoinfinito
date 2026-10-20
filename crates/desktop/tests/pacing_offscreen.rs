//! Late pacing on this machine's GPU, offscreen, against a simulated
//! display (`ddi_desktop::pacing::sim`, its refreshes told by a
//! simulated display link) on a fake clock: a play drawn
//! for the refresh the pacer gives (`View::present_at`) draws the same
//! picture as the play's own prediction when the two agree, and moves its
//! notes when they do not; then five seconds of the play paced late,
//! each frame's cost its real time to tick and draw, every frame drawn
//! for the refresh it is shown at (but those the pacer reports shown at
//! another: built slower or faster than usual on a busy machine),
//! nothing judged below the top tier.
//! Skipped where there is no GPU adapter (CI).

mod common;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use ddi_desktop::audio::Audio;
use ddi_desktop::gpu;
use ddi_desktop::pacing::sim::{Display, Surface};
use ddi_desktop::pacing::{Mode, Next, Pacer, Report};
use ddi_desktop::play::view::{Formats, Rgba, View};
use ddi_desktop::play::{Feed, Play, Request, host, load};
use ddi_platform::HostTime;
use ddi_ui::state::settings::Settings;

const SIZE: [u32; 2] = [640, 360];

fn read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    mut encoder: wgpu::CommandEncoder,
) -> Vec<u8> {
    let [w, h] = SIZE;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
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
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    slice.get_mapped_range().expect("mapped").to_vec()
}

/// Pixels that differ by more than 8 levels in a channel.
fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
        .count()
}

#[test]
fn a_play_paced_late_draws_for_the_refresh_it_is_shown_at() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let settings = Settings::default();
    let loaded = Rc::new(load::load("beatucada", 48_000).unwrap());
    let chart = 0;
    let formats = Formats::of(wgpu::TextureFormat::Rgba8Unorm, true);
    let mut view = View::new(&device, &queue, formats);
    view.set_song(&loaded, &settings);
    view.set_noteskin(&device, &queue, host::noteskin(&loaded, chart, &settings));
    for (what, source, side) in View::images_to_read(&loaded, &settings) {
        let (size, data) = ddi_desktop::images::load(&source, side).expect("the song's picture");
        view.add_background(what, Rgba { size, data });
    }
    view.start();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("play"),
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
        Request::song("beatucada", chart, true),
        loaded,
        &settings,
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )
    .unwrap();
    let render = ddi_shell::render_options(&settings);
    let draw = |view: &mut View, play: &mut Play, now: HostTime, keep: bool| {
        play.tick(now, None);
        let mut encoder = device.create_command_encoder(&Default::default());
        view.draw(
            &device,
            &queue,
            &mut encoder,
            &srgb,
            Some(&plain),
            SIZE,
            play,
            render,
            now,
        );
        if keep {
            Some(read(&device, &queue, &texture, encoder))
        } else {
            queue.submit(Some(encoder.finish()));
            None
        }
    };

    // Steady frames 12 s into the song (notes on the field).
    let step = 1_000_000_000 / 60;
    let now = loop {
        let now = HostTime((clock.fetch_add(step, Ordering::SeqCst) + step) as f64 / 1e9);
        draw(&mut view, &mut play, now, false);
        if play.session.player.clock().heard_now(now) >= 12.0 {
            break now;
        }
    };
    // The same moment three ways: the play's own prediction, the pacer's
    // when it agrees, a refresh later.
    let own = draw(&mut view, &mut play, now, true).unwrap();
    let predicted = play.session.predicted_present(now);
    view.present_at = Some(predicted);
    let agreed = draw(&mut view, &mut play, now, true).unwrap();
    assert!(view.present_at.is_none(), "taken by the frame");
    view.present_at = Some(HostTime(predicted.0 + 1.0 / 60.0));
    let later = draw(&mut view, &mut play, now, true).unwrap();
    assert_eq!(differing(&own, &agreed), 0, "drawn for the same refresh");
    let moved = differing(&own, &later);
    assert!(
        moved > 200,
        "a refresh later the notes move: {moved} pixels"
    );

    // Five seconds paced late on a simulated 60 Hz display, each frame
    // costing its real time.
    let start = now.0;
    let mut surface = Surface::new(Display::Fixed {
        period: 1.0 / 60.0,
        phase: start + 0.004,
    });
    let mut pacer = Pacer::new(Mode::Late);
    pacer.set_refresh_hint(Some(60.0));
    let mut report = Report::default();
    let mut t = start + 0.001;
    let mut drawn_for = Vec::new();
    let mut overrun = Vec::new();
    let mut late = 0;
    // The refreshes from a display link, as on macOS.
    let link = |pacer: &mut Pacer, surface: &Surface, t: f64| {
        if let Some((at, target)) = surface.link_tick(t) {
            pacer.refresh(at, target);
        }
    };
    let mut linked = 0;
    for i in 0..300 {
        link(&mut pacer, &surface, t);
        if let Next::At(w) = pacer.next(t) {
            t = t.max(w);
        }
        link(&mut pacer, &surface, t);
        pacer.begin(t);
        let a = t;
        t = surface.acquire(t);
        let present_at = pacer.acquired(a, t);
        late += usize::from(present_at.is_some());
        view.present_at = present_at.map(HostTime);
        clock.store((t * 1e9) as u64, Ordering::SeqCst);
        let sample = t;
        let began = Instant::now();
        draw(&mut view, &mut play, HostTime(t), false);
        t += began.elapsed().as_secs_f64();
        surface.present(t, i);
        let p = t;
        t += 0.000_1;
        let record = pacer.presented(sample, p, t);
        linked += usize::from(record.source == ddi_desktop::pacing::Source::Link);
        overrun.push(record.overrun || record.early);
        report.push(record);
        drawn_for.push(present_at);
        assert!(view.present_at.is_none());
    }
    surface.finish();
    eprintln!("{}", report.text("offscreen, simulated 60 Hz"));
    assert!(late > 200, "{late} of 300 frames paced late");
    assert_eq!(linked, 300, "every frame on the display link's refreshes");
    // Each late frame was shown at the refresh it was drawn for, but for
    // those the pacer knows were not (a busy machine).
    let mut off = 0;
    for (i, at) in drawn_for.iter().enumerate() {
        if let (Some(at), Some(glass)) = (at, surface.glass.get(i).copied().flatten()) {
            off += usize::from((glass - at).abs() > 0.000_5 && !overrun[i]);
        }
    }
    assert_eq!(
        off, 0,
        "frames shown at another refresh, unknown to the pacer"
    );
    assert_eq!(pacer.latch_misses, 0);
    let t = play.session.player.tally();
    assert!(
        t.taps[0] > 0 && t.taps[1..].iter().sum::<u32>() == 0,
        "{:?}",
        t.taps
    );
}
