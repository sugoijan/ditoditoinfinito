//! The start prompt of a song chosen in song select counts down to Start
//! (`docs/plans/start-prompt.md`): at zero the play begins by itself; a key
//! other than Enter stops it and Start waits; a request from elsewhere
//! (`--play`, a replay, a playtest) does not count down. On this machine's
//! GPU offscreen and a fake clock with no sound device; skipped where
//! there is no GPU adapter (CI).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_ui::event::{KeyEvent, UiEvent};

const SIZE: [u32; 2] = [960, 540];

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

#[test]
fn song_selects_prompt_counts_down_to_start_and_a_key_stops_it() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("select", fold);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    ui.update(0.5);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let formats = Formats::of(wgpu::TextureFormat::Rgba8Unorm, true);
    let dev = || Device {
        device: &device,
        queue: &queue,
        formats,
    };
    let state = |env: &ddi_ui::screens::Env| {
        env.state
            .borrow()
            .prompt
            .as_ref()
            .map(|p| p.countdown_state.clone())
            .unwrap_or_default()
    };
    // A UI's frame of `dt` and a poll, `n` times.
    let run = |host: &mut Host, ui: &mut ddi_ui::Ui, n: usize, dt: f32| {
        for _ in 0..n {
            ui.update(dt);
            host.poll(ui, &env, Some(dev()), 48_000);
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    for (chosen, cancel) in [(true, false), (true, true), (false, false)] {
        let mut host = Host::new(Rc::default(), None);
        let c = clock.clone();
        host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
        let mut request = Request::song("beatucada", 2, true);
        request.countdown = chosen;
        *host.requested.borrow_mut() = Some(request);
        run(&mut host, &mut ui, 2, 0.05);
        assert!(host.prompting());
        let expect = if chosen { "running" } else { "off" };
        assert_eq!(state(&env), expect);
        if cancel {
            key(&mut ui, "KeyA", "a");
            assert_eq!(state(&env), "cancelled");
        }
        // Past the 3 s the settings give (and what the play loads).
        run(&mut host, &mut ui, 80, 0.05);
        let mut polls = 0;
        while chosen && !cancel && !host.playing() {
            run(&mut host, &mut ui, 1, 0.05);
            polls += 1;
            assert!(polls < 2000, "never played: {:?}", host.prompt_waiting());
        }
        assert_eq!(host.playing(), chosen && !cancel, "{chosen} {cancel}");
        if !host.playing() {
            // Start waits for Start.
            key(&mut ui, "Escape", "Escape");
            run(&mut host, &mut ui, 2, 0.05);
            assert!(host.idle());
        }
    }
}
