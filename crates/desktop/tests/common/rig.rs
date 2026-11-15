//! The window's play host offscreen: on a fake clock (no sound device),
//! its frames drawn into a [`Target`] beside the theme's UI.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_ui::Ui;
use ddi_ui::screens::Env;

use super::gpu::{Target, formats};

/// The fake clock's first reading, in nanoseconds.
pub const START_NS: u64 = 1_000_000_000;

/// `host` on a fake clock from [`START_NS`]: the clock, which the test
/// moves on.
pub fn fake_clock(host: &mut Host) -> Arc<AtomicU64> {
    let clock = Arc::new(AtomicU64::new(START_NS));
    let c = clock.clone();
    host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    clock
}

/// Polls `host` (with the GPU) until its prompt is ready, a twentieth of
/// a second of the UI each time; the song's reading on its thread given
/// real time between.
pub fn wait_prompt<'a>(host: &mut Host, ui: &mut Ui, env: &Env, dev: impl Fn() -> Device<'a>) {
    let mut waited = 0;
    loop {
        host.poll(ui, env, Some(dev()), 48_000);
        ui.update(0.05);
        if host.prompt_ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
}

/// One frame of `host`'s play drawn into `target` at `now_ns` on the fake
/// clock, submitted.
pub fn draw(
    host: &mut Host,
    dev: &Device<'_>,
    target: &Target,
    ui: &mut Ui,
    env: &Env,
    now_ns: u64,
) {
    let mut encoder = dev.device.create_command_encoder(&Default::default());
    host.draw_play(
        dev,
        &mut encoder,
        &target.srgb,
        Some(&target.plain),
        target.size,
        HostTime(now_ns as f64 / 1e9),
        ui,
        env,
    );
    dev.queue.submit(Some(encoder.finish()));
}

/// A practice room's stage readied and started, as a person does it from
/// its prompt: Ready (Enter), then the room's countdown passed.
pub fn ready_and_start<'a>(
    host: &mut Host,
    ui: &mut Ui,
    env: &Env,
    room: &ddi_ui::state::room::SharedRoom,
    dev: impl Fn() -> Device<'a>,
) {
    ui.update(0.5);
    super::ui::key(ui, "Enter", "Enter");
    ui.update(0.05);
    countdown(host, ui, env, room, dev);
}

/// The room's countdown passed (three seconds on its clock): the stage
/// starts, the prompt pressing Start.
pub fn countdown<'a>(
    host: &mut Host,
    ui: &mut Ui,
    env: &Env,
    room: &ddi_ui::state::room::SharedRoom,
    dev: impl Fn() -> Device<'a>,
) {
    {
        let mut r = room.borrow_mut();
        let later = r.now_ms() + 3_000;
        r.tick(later);
    }
    ui.update(0.05);
    host.poll(ui, env, Some(dev()), 48_000);
}

/// The window's parts a test drives: its GPU, the UI, the play host on a
/// fake clock and the picture it draws into.
pub struct Rig {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// The UI's own device, as the window keeps one.
    pub ui_gpu: Option<Gpu>,
    pub ui: Ui,
    pub env: Env,
    pub host: Host,
    pub clock: Arc<AtomicU64>,
    pub formats: Formats,
    pub target: Target,
}

impl Rig {
    /// `host` beside `ui` on a fake clock, drawing at `size` on a device
    /// of `adapter`'s.
    pub fn new(adapter: &wgpu::Adapter, size: [u32; 2], ui: Ui, env: Env, mut host: Host) -> Rig {
        let (device, queue) = gpu::device(adapter).unwrap();
        let clock = fake_clock(&mut host);
        let target = Target::new(&device, size);
        Rig {
            device,
            queue,
            ui_gpu: None,
            ui,
            env,
            host,
            clock,
            formats: formats(),
            target,
        }
    }

    /// With a device of its own for the UI, as the window has.
    pub fn with_ui_gpu(mut self, adapter: &wgpu::Adapter) -> Rig {
        let (device, queue) = gpu::device(adapter).unwrap();
        self.ui_gpu = Some(Gpu::new(device, queue, wgpu::TextureFormat::Rgba8Unorm));
        self
    }

    pub fn dev(&self) -> Device<'_> {
        Device {
            device: &self.device,
            queue: &self.queue,
            formats: self.formats,
        }
    }

    pub fn poll(&mut self) {
        let dev = Device {
            device: &self.device,
            queue: &self.queue,
            formats: self.formats,
        };
        self.host.poll(&mut self.ui, &self.env, Some(dev), 48_000);
    }

    /// [`wait_prompt`].
    pub fn wait_prompt(&mut self) {
        let (device, queue, formats) = (&self.device, &self.queue, self.formats);
        wait_prompt(&mut self.host, &mut self.ui, &self.env, || Device {
            device,
            queue,
            formats,
        });
    }

    /// [`ready_and_start`] in `room`.
    pub fn ready_and_start(&mut self, room: &ddi_ui::state::room::SharedRoom) {
        let (device, queue, formats) = (&self.device, &self.queue, self.formats);
        ready_and_start(&mut self.host, &mut self.ui, &self.env, room, || Device {
            device,
            queue,
            formats,
        });
    }

    /// [`countdown`] in `room`.
    pub fn countdown(&mut self, room: &ddi_ui::state::room::SharedRoom) {
        let (device, queue, formats) = (&self.device, &self.queue, self.formats);
        countdown(&mut self.host, &mut self.ui, &self.env, room, || Device {
            device,
            queue,
            formats,
        });
    }

    /// One frame of the play `step_ns` on: the host second it was drawn
    /// at.
    pub fn frame(&mut self, step_ns: u64) -> f64 {
        let now = self.clock.fetch_add(step_ns, Ordering::SeqCst) + step_ns;
        let dev = Device {
            device: &self.device,
            queue: &self.queue,
            formats: self.formats,
        };
        draw(
            &mut self.host,
            &dev,
            &self.target,
            &mut self.ui,
            &self.env,
            now,
        );
        now as f64 / 1e9
    }

    /// The pixels drawn last.
    pub fn read(&self) -> Vec<u8> {
        self.target.read(&self.device, &self.queue)
    }

    /// The UI drawn alone on its own device (an empty picture without
    /// one): its pixels.
    pub fn ui_picture(&mut self) -> Vec<u8> {
        let size = self.target.size;
        match self.ui_gpu.as_mut() {
            Some(gpu) => gpu.draw_offscreen(&mut self.ui, size, wgpu::TextureFormat::Rgba8Unorm),
            None => Vec::new(),
        }
    }

    /// The GPU's work waited for.
    pub fn wait_gpu(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// A control's edge from `device`, a tenth of a second before the
    /// clock's reading.
    pub fn edge(&mut self, device: DeviceId, control: &str, pressed: bool) {
        let at = self.clock.load(Ordering::SeqCst) as f64 / 1e9 - 0.1;
        self.host.feed.borrow_mut().edges.push(RawInput {
            device,
            control: control.into(),
            pressed,
            host_time: HostTime(at),
            slot: 0,
        });
    }

    /// A key pressed and released in the play.
    pub fn press(&mut self, code: &str) {
        self.edge(DeviceId::Keyboard, code, true);
        self.edge(DeviceId::Keyboard, code, false);
    }
}

/// The window's clocks as `Shell::redraw` sets them on the UI
/// (`ddi_desktop::clock::UiClock`): the host clock and the wall clock
/// moving on together frame by frame, and apart across a sleep (the host
/// clock, an `Instant`, stands still while the computer sleeps; the wall
/// clock does not).
pub struct ShellClock {
    pub ui_clock: ddi_desktop::clock::UiClock,
    /// Host time, seconds.
    pub host: f64,
    /// Wall clock, ms since the epoch.
    pub wall_ms: u64,
}

impl ShellClock {
    /// From the system's wall clock now, host time one second in.
    pub fn now() -> ShellClock {
        ShellClock {
            ui_clock: ddi_desktop::clock::UiClock::default(),
            host: 1.0,
            wall_ms: ddi_desktop::clock::now_ms(),
        }
    }

    /// One frame `dt` seconds after the last: both clocks on, set on the
    /// UI as the window sets them, then its update.
    pub fn frame(&mut self, ui: &mut Ui, dt: f64) {
        self.host += dt;
        self.wall_ms += (dt * 1000.0).round() as u64;
        self.ui_clock.frame(ui, self.host, self.wall_ms);
        ui.update(dt as f32);
    }

    /// The computer asleep `seconds`: the wall clock moves on, the host
    /// clock does not.
    pub fn sleep(&mut self, seconds: u64) {
        self.wall_ms += seconds * 1000;
    }
}
