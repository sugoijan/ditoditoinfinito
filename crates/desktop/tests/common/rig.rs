//! The window's play host offscreen: on a fake clock (no sound device),
//! its frames drawn into a [`Target`] beside the theme's UI.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu;
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

/// The UI's step while a prompt's song is read: none of its time. A
/// twentieth of a second a poll made the prompt's own clock (a room's
/// countdown to Ready, song select's to Start) depend on how long the
/// read took in real time: on a loaded machine a stage's prompt readied
/// by itself before the test pressed anything.
pub const WAIT_STEP: f32 = 0.0;

/// Polls `host` (with the GPU) until its prompt is ready, the song's
/// reading on its thread given real time between. The UI is stepped
/// without its clock moving ([`WAIT_STEP`]): how long the song takes to
/// read on this machine never runs the prompt's countdown, which a test
/// moves on by itself after the wait.
pub fn wait_prompt<'a>(host: &mut Host, ui: &mut Ui, env: &Env, dev: impl Fn() -> Device<'a>) {
    let mut waited = 0;
    loop {
        host.poll(ui, env, Some(dev()), 48_000);
        ui.update(WAIT_STEP);
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

/// What a play is drawn on in a test: this machine's GPU and a picture
/// to draw into, or nothing where there is none (CI's runners,
/// `DDI_NO_GPU=1`): the host is then headless (`Host::headless`: its
/// prompt readies and its play runs, judged and not drawn), so what a
/// test asserts of the room, the stage's outcome, the voice and the
/// session runs either way, and only what reads the view (the fields'
/// places, what was drawn) or the pixels sits behind [`Draws::drawn`].
pub struct Draws {
    gpu: Option<(wgpu::Device, wgpu::Queue, Target)>,
    pub formats: Formats,
    pub size: [u32; 2],
}

impl Draws {
    /// On this machine's GPU at `size`; without one, saying so once, and
    /// `host` made headless.
    pub fn here(host: &mut Host, size: [u32; 2]) -> Draws {
        let instance = gpu::instance();
        match gpu::adapter(&instance, None) {
            Some(adapter) => Draws::on(&adapter, size),
            None => {
                eprintln!("no GPU adapter: the play runs undrawn, its pictures are skipped");
                host.headless = true;
                Draws {
                    gpu: None,
                    formats: formats(),
                    size,
                }
            }
        }
    }

    /// On a device of `adapter`'s.
    pub fn on(adapter: &wgpu::Adapter, size: [u32; 2]) -> Draws {
        let (device, queue) = gpu::device(adapter).unwrap();
        let target = Target::new(&device, size);
        Draws {
            gpu: Some((device, queue, target)),
            formats: formats(),
            size,
        }
    }

    /// Whether the play is drawn here (else it runs headless).
    pub fn drawn(&self) -> bool {
        self.gpu.is_some()
    }

    /// The host's device, where there is one.
    pub fn dev(&self) -> Option<Device<'_>> {
        self.gpu.as_ref().map(|(device, queue, _)| Device {
            device,
            queue,
            formats: self.formats,
        })
    }

    pub fn poll(&self, host: &mut Host, ui: &mut Ui, env: &Env) {
        host.poll(ui, env, self.dev(), 48_000);
    }

    /// Polls `host` until its prompt is ready, the song's reading on its
    /// thread given real time between, the UI stepped without its clock
    /// moving ([`WAIT_STEP`]).
    pub fn wait_prompt(&self, host: &mut Host, ui: &mut Ui, env: &Env) {
        let mut waited = 0;
        loop {
            self.poll(host, ui, env);
            ui.update(WAIT_STEP);
            if host.prompt_ready() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited += 1;
            assert!(waited < 1000, "the song was never read: {:?}", host.prompt_waiting());
        }
    }

    /// [`ready_and_start`], drawn or not.
    pub fn ready_and_start(
        &self,
        host: &mut Host,
        ui: &mut Ui,
        env: &Env,
        room: &ddi_ui::state::room::SharedRoom,
    ) {
        ui.update(0.5);
        super::ui::key(ui, "Enter", "Enter");
        ui.update(0.05);
        self.countdown(host, ui, env, room);
    }

    /// [`countdown`], drawn or not.
    pub fn countdown(
        &self,
        host: &mut Host,
        ui: &mut Ui,
        env: &Env,
        room: &ddi_ui::state::room::SharedRoom,
    ) {
        {
            let mut r = room.borrow_mut();
            let later = r.now_ms() + 3_000;
            r.tick(later);
        }
        ui.update(0.05);
        self.poll(host, ui, env);
    }

    /// One frame of `host`'s play at `now_ns` on the fake clock: drawn
    /// into the picture and submitted, or judged alone.
    pub fn frame(&self, host: &mut Host, ui: &mut Ui, env: &Env, now_ns: u64) {
        match (&self.gpu, self.dev()) {
            (Some((_, _, target)), Some(dev)) => draw(host, &dev, target, ui, env, now_ns),
            _ => host.step_play(HostTime(now_ns as f64 / 1e9), ui, env),
        }
    }

    /// The GPU's work waited for (nothing without one).
    pub fn wait_gpu(&self) {
        if let Some((device, _, _)) = &self.gpu {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
    }

    /// The pixels drawn last (RGBA rows), where the play is drawn.
    pub fn picture(&self) -> Option<Vec<u8>> {
        self.gpu
            .as_ref()
            .map(|(device, queue, target)| target.read(device, queue))
    }

    /// The pixels drawn last kept as `name` under `DDI_OFFSCREEN_PPM`,
    /// where the play is drawn.
    pub fn shot(&self, name: &str) {
        if let Some(pixels) = self.picture() {
            super::shot(name, &pixels, self.size);
        }
    }
}

/// The window's parts a test drives: what its play is drawn on
/// ([`Draws`]), the UI, the play host on a fake clock and the UI's own
/// pictures.
pub struct Rig {
    pub draws: Draws,
    /// The UI's own pictures, as the window keeps a device for it
    /// (undrawn without a GPU).
    pub pictures: super::gpu::Pictures,
    pub ui: Ui,
    pub env: Env,
    pub host: Host,
    pub clock: Arc<AtomicU64>,
}

impl Rig {
    /// `host` beside `ui` on a fake clock, drawing at `size` on a device
    /// of `adapter`'s.
    pub fn new(adapter: &wgpu::Adapter, size: [u32; 2], ui: Ui, env: Env, mut host: Host) -> Rig {
        let clock = fake_clock(&mut host);
        Rig {
            draws: Draws::on(adapter, size),
            pictures: super::gpu::Pictures::new(),
            ui,
            env,
            host,
            clock,
        }
    }

    /// [`Rig::new`] on this machine's GPU, or headless where there is
    /// none ([`Draws::here`]).
    pub fn here(size: [u32; 2], ui: Ui, env: Env, mut host: Host) -> Rig {
        let clock = fake_clock(&mut host);
        Rig {
            draws: Draws::here(&mut host, size),
            pictures: super::gpu::Pictures::new(),
            ui,
            env,
            host,
            clock,
        }
    }

    /// (The UI's pictures are always there now.)
    pub fn with_ui_gpu(self, _adapter: &wgpu::Adapter) -> Rig {
        self
    }

    /// Whether the play is drawn (else it runs headless).
    pub fn drawn(&self) -> bool {
        self.draws.drawn()
    }

    pub fn poll(&mut self) {
        self.draws.poll(&mut self.host, &mut self.ui, &self.env);
    }

    /// [`wait_prompt`].
    pub fn wait_prompt(&mut self) {
        self.draws.wait_prompt(&mut self.host, &mut self.ui, &self.env);
    }

    /// [`ready_and_start`] in `room`.
    pub fn ready_and_start(&mut self, room: &ddi_ui::state::room::SharedRoom) {
        self.draws
            .ready_and_start(&mut self.host, &mut self.ui, &self.env, room);
    }

    /// [`countdown`] in `room`.
    pub fn countdown(&mut self, room: &ddi_ui::state::room::SharedRoom) {
        self.draws
            .countdown(&mut self.host, &mut self.ui, &self.env, room);
    }

    /// One frame of the play `step_ns` on: the host second it was drawn
    /// (or, headless, judged) at.
    pub fn frame(&mut self, step_ns: u64) -> f64 {
        let now = self.clock.fetch_add(step_ns, Ordering::SeqCst) + step_ns;
        self.draws
            .frame(&mut self.host, &mut self.ui, &self.env, now);
        now as f64 / 1e9
    }

    /// The pixels drawn last (empty where the play is not drawn).
    pub fn read(&self) -> Vec<u8> {
        self.draws.picture().unwrap_or_default()
    }

    /// The pixels drawn last kept as `name`, where the play is drawn.
    pub fn shot(&self, name: &str) {
        self.draws.shot(name);
    }

    /// The UI drawn alone on its own device: its pixels (none without a
    /// GPU, the UI laid out as a frame all the same).
    pub fn ui_picture(&mut self) -> Option<Vec<u8>> {
        let size = self.draws.size;
        self.pictures.draw(&mut self.ui, size)
    }

    /// The GPU's work waited for.
    pub fn wait_gpu(&self) {
        self.draws.wait_gpu();
    }

    /// The stage as it is watched now, as the session has it (drawn or
    /// not): the view's word as the drawn view says it (`grid`, `focus`,
    /// `out`, `left`) and the seat followed; none while the player plays.
    pub fn watching(&self) -> Option<(&'static str, Option<usize>)> {
        use ddi_session::play::Watching;
        let now = self.clock.load(Ordering::SeqCst) as f64 / 1e9;
        let w = self.host.running()?.session.watch(now)?;
        let view = match (w.why, w.focus) {
            (Watching::Spectator, None) => "grid",
            (Watching::Spectator, Some(_)) => "focus",
            (Watching::Out, _) => "out",
            (Watching::Left, _) => "left",
        };
        Some((view, w.focus))
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
/// (`ddi_desktop::clock::UiClock`): the host clock and the system's clocks
/// moving on together frame by frame, and apart across a sleep (the host
/// clock, an `Instant`, stands still while the computer sleeps; the wall
/// clock and the monotonic clock the rooms run on do not) or a step of
/// the wall clock (set back or forward: only the wall clock moves). The
/// rooms' time is the wall clock's at the first frame run on by the
/// monotonic clock (`ddi_platform::steady::SteadyClock`, as
/// `ddi_desktop::clock::now_ms` reads it, round P1b).
pub struct ShellClock {
    pub ui_clock: ddi_desktop::clock::UiClock,
    /// Host time, seconds.
    pub host: f64,
    /// The system's wall clock, ms since the epoch.
    pub wall_ms: u64,
    /// The system's monotonic clock that counts through sleep, ms.
    pub running_ms: f64,
    steady: ddi_platform::steady::SteadyClock,
}

impl ShellClock {
    /// From the system's wall clock now, host time one second in.
    pub fn now() -> ShellClock {
        ShellClock {
            ui_clock: ddi_desktop::clock::UiClock::default(),
            host: 1.0,
            wall_ms: ddi_desktop::clock::now_ms(),
            running_ms: 0.0,
            steady: ddi_platform::steady::SteadyClock::new(),
        }
    }

    /// The rooms' time now, as the window reads it.
    pub fn steady_ms(&mut self) -> u64 {
        self.steady.read(self.wall_ms as f64, self.running_ms)
    }

    /// One frame `dt` seconds after the last: the clocks on, set on the
    /// UI as the window sets them, then its update.
    pub fn frame(&mut self, ui: &mut Ui, dt: f64) {
        self.host += dt;
        let ms = (dt * 1000.0).round();
        self.wall_ms += ms as u64;
        self.running_ms += ms;
        let steady = self.steady_ms();
        self.ui_clock.frame(ui, self.host, steady);
        ui.update(dt as f32);
    }

    /// The computer asleep `seconds`: the wall clock and the monotonic
    /// one move on, the host clock does not.
    pub fn sleep(&mut self, seconds: u64) {
        self.wall_ms += seconds * 1000;
        self.running_ms += (seconds * 1000) as f64;
    }
}
