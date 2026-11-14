//! A play after the GPU device was lost (`Host::device_lost`, what the
//! window does with a new device) and after an interruption: the prompt
//! waits for the song's drawing to be made again (its pictures read
//! again: the background shows), and Retry brings the prompt's Start
//! back rather than playing at once. On this machine's GPU offscreen, a
//! fake clock, no sound device. Skipped where there is no GPU adapter.

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_ui::screens::prompt::PromptStep;
use common::ui::key_step as key;

const SIZE: [u32; 2] = [960, 540];

#[test]
fn a_lost_device_and_an_interruption_ready_the_play_again() {
    common::indexes();
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let (mut ui, env) = common::ui::theme("select", common::ui::lowercase(), SIZE, false);
    ui.update(0.5);
    let mut host = Host::new(Rc::default(), None);
    let clock = common::rig::fake_clock(&mut host);
    let formats = common::gpu::formats();
    let dev = || Device {
        device: &device,
        queue: &queue,
        formats,
    };
    let target = common::gpu::Target::new(&device, SIZE);
    let until_ready = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        let mut waited = 0;
        while !host.prompt_ready() {
            host.poll(ui, &env, Some(dev()), 48_000);
            ui.update(0.05);
            std::thread::sleep(std::time::Duration::from_millis(10));
            waited += 1;
            assert!(waited < 1000, "never ready: {:?}", host.prompt_step());
        }
        // The pictures read again too.
        let mut waited = 0;
        while host.backgrounds() == 0 {
            host.poll(ui, &env, Some(dev()), 48_000);
            std::thread::sleep(std::time::Duration::from_millis(10));
            waited += 1;
            assert!(waited < 500, "no background");
        }
        ui.update(0.5);
    };
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    until_ready(&mut host, &mut ui);
    // Lost at the prompt: it loads again behind Start.
    host.device_lost();
    assert_eq!(host.prompt_step(), Some(PromptStep::Ready));
    assert!(!host.prompt_ready());
    assert_eq!(host.backgrounds(), 0);
    until_ready(&mut host, &mut ui);
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing());
    let frame = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        common::rig::draw(host, &dev(), &target, ui, &env, now);
    };
    for _ in 0..30 {
        frame(&mut host, &mut ui);
    }
    // Lost in play: stopped as interrupted; Retry readies it again and
    // offers Start, it does not play at once.
    host.device_lost();
    frame(&mut host, &mut ui);
    assert_eq!(host.prompt_step(), Some(PromptStep::Interrupted));
    ui.update(0.5);
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(!host.playing());
    assert_eq!(host.prompt_step(), Some(PromptStep::Ready));
    until_ready(&mut host, &mut ui);
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing());
    frame(&mut host, &mut ui);
    assert!(host.playing());
}
