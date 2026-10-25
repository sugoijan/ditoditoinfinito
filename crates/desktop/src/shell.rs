//! The window: winit's events as `UiEvent`s, the UI's requests answered,
//! frames drawn on demand (P13: `ControlFlow::Wait`, a redraw asked only
//! while `Ui::needs_redraw`; none while the window is occluded).
//!
//! Events map as the web's do: logical (CSS-like) positions, the scale
//! factor as the device pixel ratio, keys by their DOM names
//! ([`crate::keys`]), wheel notches as [`LINES_PER_NOTCH`] lines, touches
//! as touch pointers. Typing (P5) is ours (`TextMode::Own`): the IME is
//! allowed while a field is typed into, its caret area follows the caret
//! each time it moves, `Ime::Preedit` and `Ime::Commit` edit the field, and
//! a key's text is committed while no composition shows. Typing ends with
//! the window's focus or the screen's stop, never at `Ime::Disabled`.
//! Gestures are not required: every request is answered at once.
//!
//! A play's frames run continuously, paced by [`crate::pacing`]: one
//! after another, each waiting for the display in the surface's acquire
//! (the default), or with `--pacing late` each started as late before its
//! refresh as its cost allows, the loop sleeping in `WaitUntil` until
//! then; `--pacing-report` alternates the two and prints what each did.

use std::sync::Arc;
use std::time::Instant;

use ddi_platform::storage::{self, Area};
use std::rc::Rc;

use ddi_ui::dev::{self, DevScreen, Shared};
use ddi_ui::dev_widgets::{FoldFn, SharedWidgets, WidgetScreen};
use ddi_ui::event::{
    DeltaMode, KeyEvent, LINES_PER_NOTCH, Modifiers, PointerEvent, PointerKind, PointerPhase,
    TextEvent, UiEvent, WheelEvent, WheelSource, buttons, wheel_pixels,
};
use ddi_ui::message::Message;
use ddi_ui::request::Cursor;
use ddi_ui::screens::select::{SelectCall, SharedFeed, ShellFn};
use ddi_ui::screens::{self as theme_screens, Env as ThemeEnv, sample};
use ddi_ui::{TextMode, Ui, UiRequest};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{CursorIcon, Window, WindowId};

use crate::gpu::{self, Gpu};
use crate::pacing::{self, Next, Pacer};
use crate::preview::{Player, SharedPlayer};
use std::cell::RefCell;

/// How long to wait before asking a surface whose window is not visible
/// yet for a frame again.
const OCCLUDED_RETRY: std::time::Duration = std::time::Duration::from_millis(50);

/// How long to wait before asking the surface for a frame again.
const RETRY: std::time::Duration = std::time::Duration::from_millis(250);

/// The mouse's pointer id; touches are numbered after it.
const MOUSE: u64 = 0;

/// How long a play's window may be hidden and the play go on: macOS
/// reports the window covered while it moves to or from its fullscreen
/// Space (most of a second of animation), which must not stop a play; a
/// window minimized, or another Space shown, for longer does.
const HIDDEN_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// The window hidden during a play: since when, and whether the play
/// stops for it once the window shows again (no frame is drawn, so the
/// play is judged again only then).
#[derive(Default)]
struct Hidden {
    since: Option<Instant>,
}

impl Hidden {
    /// The window was hidden at `now` during a play (the first time
    /// counts until it shows again).
    fn hide(&mut self, now: Instant) {
        self.since.get_or_insert(now);
    }

    /// The window shows again at `now`: how long it was hidden during a
    /// play, and whether that stops the play (longer than
    /// [`HIDDEN_GRACE`]).
    fn show(&mut self, now: Instant) -> Option<(std::time::Duration, bool)> {
        let hidden = now.saturating_duration_since(self.since.take()?);
        Some((hidden, hidden > HIDDEN_GRACE))
    }
}

/// The game's id, as the packages name it (`tools/package/`): the Linux
/// desktop entry's and its icon's.
#[cfg(target_os = "linux")]
const APP_ID: &str = "dev.sugoijan.ditoditoinfinito";

/// The packages' icon (`tools/package/icon.svg`) at `side` pixels, for
/// the window's (macOS shows the app's own).
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn window_icon(side: u32) -> Option<winit::window::Icon> {
    let svg = include_str!("../../../tools/package/icon.svg");
    let rgba = ddi_noteskin::raster::rasterize(svg, side, side).ok()?;
    winit::window::Icon::from_rgba(rgba, side, side).ok()
}

struct Surface {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    gpu: Gpu,
    /// What the play draws through (D6): the sRGB view the surface was
    /// configured with, and the plain one.
    formats: crate::play::view::Formats,
    /// Present modes the surface has (play prefers Mailbox).
    modes: Vec<wgpu::PresentMode>,
    /// The display's refreshes as the OS tells them (macOS), for the
    /// pacing; running while a play does.
    link: Option<crate::display_link::DisplayLink>,
}

pub struct Shell {
    pub ui: Ui,
    pub dev: Shared,
    surface: Option<Surface>,
    scale: f64,
    cursor: [f32; 2],
    buttons: u32,
    modifiers: Modifiers,
    /// A preedit is shown; a field is typed into.
    preedit: bool,
    typing: bool,
    occluded: bool,
    /// The window hidden during a play ([`HIDDEN_GRACE`]).
    hidden: Hidden,
    /// A redraw was asked for and has not come: not asked again (a window
    /// that cannot draw, a locked screen, must not spin the loop).
    pending: bool,
    /// The surface gave no frame: when to try again.
    retry_at: Option<Instant>,
    start: Instant,
    last: Option<Instant>,
    /// Frames stopped (idle) since, [`crate::clock`]'s seconds: the UI's
    /// clock is told how long when a screen's deadline comes
    /// ([`Ui::idle_wake`]).
    idle_since: Option<f64>,
    pub frames: u64,
    /// Redraws asked for, and skipped while occluded.
    pub asked: u64,
    pub occluded_skips: u64,
    surface_failures: u32,
    pub sounds: u32,
    pub files: usize,
    /// Quit after this many seconds (a run under a watchdog).
    pub quit_after: Option<f64>,
    pub error: Option<String>,
    quitting: bool,
    quit_ready: bool,
    /// The theme's screens, when asked for (`--screen select`).
    pub theme: Option<(ThemeEnv, Rc<ddi_theme::Infinito>)>,
    /// Decoded images wait for a frame to show them.
    loaded: bool,
    /// Sound: song select's previews, the developer screen's tone
    /// (`crate::preview`).
    pub sound: SharedPlayer,
    /// Input (`crate::keyboard`, `crate::pads`): the keyboard's source,
    /// the OS's key stamps waiting for winit's events, the controllers,
    /// the bindings menus read, the options screen's state.
    pub input: crate::input::Input,
    /// The library on disk, when song select runs on it.
    pub songs: Option<Rc<crate::songs::Songs>>,
    /// A play: its prompt, the play itself, its results
    /// (`crate::play::host`).
    pub play: crate::play::host::Host,
    /// The mouse pointer is hidden (a key or a pad was pressed since it
    /// last moved).
    pointer_hidden: bool,
    /// The device notices' buttons pressed, for the watcher.
    notice_actions: Rc<RefCell<Vec<String>>>,
    /// Play mode is on ([`Shell::sync_play_mode`]).
    play_on: bool,
    /// The device was lost (its callback's flag).
    lost: Arc<std::sync::atomic::AtomicBool>,
    /// Devices lost and made again.
    pub devices_lost: u32,
    /// Lose the device this long after the start (`--lose-device-after`).
    pub lose_device_after: Option<f64>,
    /// When a play's frames start.
    pub pacer: Pacer,
    /// `--pacing` and `--in-flight`, over the defaults
    /// (`pacing::default_for`: late with two in flight on a display link).
    pub pacing_flag: Option<pacing::Mode>,
    pub in_flight_flag: Option<u32>,
    /// `--pacing-report`: what a play's frames did under each pacing.
    pub pacing_report: Option<pacing::Report>,
}

impl Shell {
    pub fn new() -> Shell {
        Shell::with_screen(false)
    }

    /// The developer screen; with `widgets`, the widget screen over it.
    pub fn with_screen(widgets: bool) -> Shell {
        Shell::named(if widgets { "widgets" } else { "" })
    }

    /// The screen `name`: the developer screen (`""`), the widget screen
    /// over it (`widgets`), or the theme's song select, results or
    /// options on sample data (`select`, `results`, `options`).
    pub fn named(name: &str) -> Shell {
        Shell::named_at(name, None)
    }

    /// [`Shell::named`], with the master volume `volume` instead of the
    /// settings' (`--volume`).
    pub fn named_at(name: &str, volume: Option<f32>) -> Shell {
        Shell::named_with(name, volume, None)
    }

    /// [`Shell::named_at`]; with `songs`, song select and options are on
    /// the library on disk (`crate::songs`), not on sample data.
    pub fn named_with(
        name: &str,
        volume: Option<f32>,
        songs: Option<Rc<crate::songs::Songs>>,
    ) -> Shell {
        let volume = volume.unwrap_or_else(|| ddi_ui::state::settings::Settings::load().volume);
        let sound: SharedPlayer = Rc::new(RefCell::new(match songs {
            // Previews of the library's songs too.
            Some(_) => Player::with(
                crate::audio::Audio::new(volume),
                crate::library::preview_song,
            ),
            None => Player::new(volume),
        }));
        let dev = Shared::default();
        let fold: FoldFn = Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&crate::Nfkd, s));
        let mut theme = None;
        let mut input = crate::input::Input::default();
        let mut play = crate::play::host::Host::new(
            songs.as_ref().map(|s| s.play.clone()).unwrap_or_default(),
            songs.as_ref().and_then(|s| s.plays.clone()),
        );
        // A song's chart read for its prompt: its stale plays first.
        if let Some(songs) = &songs {
            let songs = songs.clone();
            play.chart_loaded = Some(Rc::new(move |id, song| songs.chart_loaded(id, song)));
        }
        let notice_actions: Rc<RefCell<Vec<String>>> = Rc::default();
        let ui = if let Some(songs) = songs
            .as_ref()
            .filter(|_| matches!(name, "start" | "select" | "options"))
        {
            let (mut ui, env, look) = library_ui(name, fold, sound.clone(), songs);
            // Calibration's menu, opened again when a test leaves.
            {
                let (songs, sound) = (songs.clone(), sound.clone());
                play.menu = Some(Rc::new(move |env: &ThemeEnv, ui: &mut Ui| {
                    ui.push(crate::calibrate::menu(env, &songs, Some(sound.clone())));
                }));
            }
            // The devices' notices over every screen (`crate::devices`).
            let actions = notice_actions.clone();
            ui.push_chrome(theme_screens::notices::NoticeBar::new(
                env.clone(),
                play.watcher.feed.clone(),
                Rc::new(move |id: &str, _cx: &mut ddi_ui::Cx| {
                    actions.borrow_mut().push(id.to_string())
                }),
            ));
            theme = Some((env, look));
            ui
        } else if matches!(
            name,
            "start" | "select" | "results" | "options" | "credits" | "calibration"
        ) {
            let (mut ui, env, look) = theme_ui_with(
                if name == "options" { "select" } else { name },
                fold,
                Some(sound.clone()),
            );
            if name == "options" {
                let (screen, shared) = crate::options::screen(&env, Some(sound.clone()), None);
                ui.push(screen);
                input.options = Some(shared);
            }
            theme = Some((env, look));
            ui
        } else {
            let shared = SharedWidgets::default();
            let mut ui = Ui::new(dev::DESIGN, TextMode::Own, false);
            ui.push(DevScreen::with_widgets(
                dev.clone(),
                shared.clone(),
                fold.clone(),
            ));
            if name == "widgets" {
                ui.push(WidgetScreen::new(shared, fold));
            }
            ui
        };
        Shell {
            ui,
            dev,
            surface: None,
            scale: 1.0,
            cursor: [0.0; 2],
            buttons: 0,
            modifiers: Modifiers::default(),
            preedit: false,
            typing: false,
            occluded: false,
            hidden: Hidden::default(),
            pending: false,
            retry_at: None,
            start: Instant::now(),
            last: None,
            idle_since: None,
            frames: 0,
            asked: 0,
            occluded_skips: 0,
            surface_failures: 0,
            sounds: 0,
            files: 0,
            quit_after: None,
            error: None,
            quitting: false,
            quit_ready: false,
            theme,
            loaded: false,
            play,
            notice_actions,
            play_on: false,
            lost: Arc::default(),
            devices_lost: 0,
            lose_device_after: None,
            pacer: Pacer::default(),
            pacing_flag: None,
            in_flight_flag: None,
            pacing_report: None,
            sound,
            input,
            songs,
            pointer_hidden: false,
        }
    }

    /// The host clock (`crate::clock`) in milliseconds: keys and pads are
    /// stamped on it.
    fn now_ms(&self) -> f64 {
        crate::clock::now().0 * 1000.0
    }

    fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let attributes = Window::default_attributes()
            .with_title("Dito Dito Infinito")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        #[cfg(target_os = "windows")]
        let attributes = {
            use winit::platform::windows::WindowAttributesExtWindows;
            // The title bar's small icon and the taskbar's big one, which
            // Windows scales to the display's (16 to 32 and 32 to 64
            // pixels).
            attributes
                .with_window_icon(window_icon(32))
                .with_taskbar_icon(window_icon(64))
        };
        #[cfg(target_os = "linux")]
        let attributes = {
            use winit::platform::wayland::WindowAttributesExtWayland;
            // X11's icon (Wayland's desktop takes the entry's, found by
            // the app id), and the app id and X11 class the desktop entry
            // is matched by.
            attributes
                .with_window_icon(window_icon(64))
                .with_name(APP_ID, APP_ID)
        };
        let window = event_loop
            .create_window(attributes)
            .map_err(|e| e.to_string())?;
        let window = Arc::new(window);
        screen_height(&window);
        self.with_gpu(window)
    }

    /// The window's surface and the device it draws with: made at the
    /// start, and again after the device was lost.
    fn with_gpu(&mut self, window: Arc<Window>) -> Result<(), String> {
        // The old surface goes before a new one is made on the window.
        self.surface = None;
        let instance = gpu::instance();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let adapter = gpu::adapter(&instance, Some(&surface)).ok_or("no GPU adapter")?;
        let (device, queue) = gpu::device(&adapter)?;
        let caps = surface.get_capabilities(&adapter);
        let format = gpu::plain_format(&caps.formats).ok_or("no surface format")?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface configuration unsupported")?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        }
        // The play draws in linear light through an sRGB view of the
        // plain surface (D6); a surface that cannot have one draws it
        // through the plain view.
        // Vulkan drivers without mutable swapchain formats cannot: the
        // play then draws into the plain surface itself (its colors
        // encoded by its pipelines, skins blending as the scene does).
        let srgb = format.add_srgb_suffix();
        let srgb_view = srgb != format
            && adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS);
        if srgb_view {
            config.view_formats = vec![srgb];
        }
        surface.configure(&device, &config);
        self.watch_device(&device);
        self.scale = window.scale_factor();
        self.play.display = display_fingerprint(&window);
        self.play.refresh_hint = refresh_rate(&window);
        self.pacer.set_refresh_hint(self.play.refresh_hint);
        self.play.graphics = format!("{:?}", adapter.get_info().backend);
        let link = crate::display_link::DisplayLink::new(&window, self.play.refresh_hint);
        if let Some(l) = &link {
            l.run(self.play_on);
        }
        let (mode, in_flight) = pacing::default_for(link.is_some());
        self.pacer.set_mode(self.pacing_flag.unwrap_or(mode));
        self.pacer
            .set_in_flight(self.in_flight_flag.unwrap_or(in_flight));
        // `--pacing-report` with `--pacing` or `--in-flight`: that column
        // alone (a play as it runs by default, its fallback included).
        if let Some(r) = &mut self.pacing_report
            && (self.pacing_flag.is_some() || self.in_flight_flag.is_some())
        {
            r.columns = vec![pacing::Column::new(
                self.pacer.mode(),
                self.pacer.in_flight(),
            )];
        }
        self.surface = Some(Surface {
            link,
            window,
            surface,
            config,
            gpu: Gpu::new(device, queue, format),
            formats: crate::play::view::Formats::of(format, srgb_view),
            modes: caps.present_modes.clone(),
        });
        self.resized();
        Ok(())
    }

    /// A device error is logged, not a panic (wgpu's default); a lost
    /// device is found in the next turn of the loop
    /// ([`Shell::recover_device`]).
    fn watch_device(&self, device: &wgpu::Device) {
        device.on_uncaptured_error(Arc::new(|error: wgpu::Error| {
            use std::sync::atomic::{AtomicU32, Ordering};
            static LOGGED: AtomicU32 = AtomicU32::new(0);
            if LOGGED.fetch_add(1, Ordering::Relaxed) < 20 {
                eprintln!("wgpu: {error}");
            }
        }));
        let lost = self.lost.clone();
        let wake = self.play.wake.clone();
        device.set_device_lost_callback(move |reason, message| {
            if reason != wgpu::DeviceLostReason::Destroyed || !message.is_empty() {
                eprintln!("the GPU device was lost ({reason:?}): {message}");
            }
            lost.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(w) = &wake {
                w();
            }
        });
    }

    /// The device was lost: a new one, and everything made on the old
    /// one made again (the UI's textures reload from their sources; a
    /// play stops as interrupted, its prompt readies the song's drawing
    /// again; results stay), as the web recovers.
    fn recover_device(&mut self) {
        // The callback runs from a poll of the device.
        if let Some(s) = &self.surface {
            let _ = s.gpu.device.poll(wgpu::PollType::Poll);
        }
        if !self.lost.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let Some(window) = self.surface.as_ref().map(|s| s.window.clone()) else {
            return;
        };
        self.devices_lost += 1;
        self.play.device_lost();
        let field = self.surface.as_ref().map(|s| s.gpu.field.for_new_device());
        if let Err(e) = self.with_gpu(window) {
            eprintln!("no new GPU device: {e}");
            self.error = Some(e);
            return;
        }
        if let (Some(s), Some(field)) = (&mut self.surface, field) {
            s.gpu.field = field;
            s.gpu.editor = crate::editor_field::EditorFieldHost::for_new_device();
        }
        self.ui.broadcast(Message::new("DeviceRecovered"));
        self.pending = false;
        self.ask();
    }

    /// Loses the device on purpose (`--lose-device-after`: a test of the
    /// recovery).
    pub fn lose_device(&self) {
        if let Some(s) = &self.surface {
            s.gpu.device.destroy();
        }
    }

    fn resized(&mut self) {
        self.occluded = false;
        self.shown();
        let Some(s) = &mut self.surface else { return };
        screen_height(&s.window);
        let size = s.window.inner_size();
        s.config.width = size.width.max(1);
        s.config.height = size.height.max(1);
        s.surface.configure(&s.gpu.device, &s.config);
        // Another size, or fullscreen entered or left: late pacing tries
        // again where it had fallen back (`crate::pacing`).
        self.pacer.set_surface(
            (s.config.width, s.config.height),
            s.window.fullscreen().is_some(),
        );
        let logical = size.to_logical::<f32>(self.scale);
        // The theme's design for this size (compact for a small window).
        if let Some((_, look)) = &self.theme {
            let design = theme_screens::design_for(&**look, [logical.width, logical.height]);
            self.ui.set_design(design);
        }
        self.send(UiEvent::Resize {
            size: [logical.width.max(1.0), logical.height.max(1.0)],
            ratio: self.scale as f32,
        });
    }

    /// The window shows again (or was resized, which shows it): a play
    /// hidden longer than [`HIDDEN_GRACE`] stops as interrupted, its
    /// prompt offering Retry; one hidden for less goes on, the notes
    /// passed meanwhile judged at its next frame.
    fn shown(&mut self) {
        let Some((hidden, stops)) = self.hidden.show(Instant::now()) else {
            return;
        };
        if !self.play.playing() {
            return;
        }
        let secs = hidden.as_secs_f64();
        if stops {
            eprintln!("the window was hidden for {secs:.2} s: the play stops as interrupted");
            self.play.feed.borrow_mut().interrupted = true;
        } else {
            eprintln!("the window was hidden for {secs:.2} s: the play goes on");
        }
    }

    fn send(&mut self, event: UiEvent) {
        let out = self.ui.handle(event);
        self.run(out.requests);
    }

    fn request_quit(&mut self) {
        if !self.quitting {
            self.quitting = true;
            self.sound.borrow_mut().close();
            self.quit_ready = self.songs.as_ref().is_none_or(|s| s.shutdown());
        }
    }

    fn run(&mut self, requests: Vec<UiRequest>) {
        let mut later = Vec::new();
        for r in requests {
            let window = self.surface.as_ref().map(|s| s.window.clone());
            match r {
                UiRequest::StartTextInput { caret, .. } => {
                    self.typing = true;
                    if let Some(w) = &window {
                        w.set_ime_allowed(true);
                        ime_area(w, caret);
                    }
                }
                UiRequest::StopTextInput => {
                    self.typing = false;
                    self.preedit = false;
                    if let Some(w) = &window {
                        w.set_ime_allowed(false);
                    }
                }
                UiRequest::SetImeCursorArea(caret) => {
                    if let Some(w) = &window {
                        ime_area(w, caret);
                    }
                }
                UiRequest::PickFiles { folder } => {
                    let picked = crate::os::pick(folder);
                    self.files += picked.len();
                    self.dev.borrow_mut().status = format!("{} file(s) picked", picked.len());
                }
                UiRequest::OpenUrl(url) => {
                    if let Err(e) = crate::os::open_url(&url) {
                        self.dev.borrow_mut().status = e;
                    }
                }
                UiRequest::StartAudio => {
                    self.sounds += 1;
                    self.sound.borrow_mut().beep();
                    self.dev.borrow_mut().status = "beep".into();
                }
                UiRequest::PlayPreview { song } => self.sound.borrow_mut().select(Some(&song)),
                UiRequest::StopPreview => self.sound.borrow_mut().pause(),
                UiRequest::Back => {}
                UiRequest::Quit => self.request_quit(),
                UiRequest::CopyText(text) => {
                    let _ = crate::os::copy(&text);
                }
                UiRequest::Paste => {
                    if let Some(text) = crate::os::paste() {
                        later.push(UiEvent::Paste(text));
                    }
                }
                UiRequest::SetCursor(c) => {
                    if let Some(w) = &window {
                        w.set_cursor(match c {
                            Cursor::Default => CursorIcon::Default,
                            Cursor::Pointer => CursorIcon::Pointer,
                            Cursor::Text => CursorIcon::Text,
                            Cursor::Grab => CursorIcon::Grab,
                            Cursor::Grabbing => CursorIcon::Grabbing,
                        });
                    }
                }
                UiRequest::Persist { key, value } => {
                    let _ = storage::set(Area::Local, &key, &value);
                }
                UiRequest::SetText { .. } | UiRequest::Navigate(_) | UiRequest::Redraw => {}
                // Options' field preview: drawn where the screen says,
                // with the settings as they are now.
                UiRequest::FieldPreview(rect) => {
                    if let Some(s) = &mut self.surface {
                        s.gpu.field.set_rect(rect);
                        if rect.is_some() {
                            let settings = ddi_ui::state::settings::Settings::load();
                            s.gpu.field.set_settings(&settings);
                        }
                    }
                }
            }
        }
        for e in later {
            self.send(e);
        }
    }

    fn pointer(&mut self, id: u64, kind: PointerKind, phase: PointerPhase, pos: [f32; 2]) {
        self.send(UiEvent::Pointer(PointerEvent {
            id,
            kind,
            phase,
            pos,
            buttons: if kind == PointerKind::Mouse {
                self.buttons
            } else if matches!(phase, PointerPhase::Down | PointerPhase::Move) {
                buttons::PRIMARY
            } else {
                0
            },
            modifiers: self.modifiers,
            time: self.now_ms(),
        }));
    }

    /// Asks for a frame if the UI needs one and none is coming.
    fn ask(&mut self) {
        let field = self
            .surface
            .as_ref()
            .is_some_and(|s| s.gpu.field.shown() || s.gpu.movie.busy() || s.gpu.editor.busy())
            || self.play.playing()
            || self.play.prompt_changed();
        if self.occluded || self.pending || !(self.ui.needs_redraw() || self.loaded || field) {
            return;
        }
        // A paced play's frame waits for its start.
        if self.play.playing() && matches!(self.pace(), Next::At(_)) {
            return;
        }
        match self.retry_at {
            Some(at) if Instant::now() < at => return,
            Some(_) => self.retry_at = None,
            None => {}
        }
        if let Some(s) = &self.surface {
            self.pending = true;
            self.asked += 1;
            s.window.request_redraw();
        }
    }

    fn redraw(&mut self) {
        self.pending = false;
        if self.occluded {
            self.occluded_skips += 1;
            return;
        }
        let now = Instant::now();
        // A play's frame is paced (`crate::pacing`): its start, its
        // acquire and its present are timed.
        let paced = self.play.playing();
        if paced {
            self.feed_link();
            self.pacer.begin(crate::clock::now().0);
        }
        let dt = self
            .last
            .map_or(1.0 / 60.0, |l| (now - l).as_secs_f32().min(0.25));
        self.last = Some(now);
        self.idle_since = None;
        self.ui.update(dt);
        let r = self.ui.take_requests();
        self.run(r);
        let Some(s) = &mut self.surface else { return };
        let acquire = crate::clock::now().0;
        let frame = match s.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                s.surface.configure(&s.gpu.device, &s.config);
                f
            }
            // Hidden (minimized, a locked screen), or no frame in time:
            // tried again shortly (only winit's `Occluded` stops frames, as
            // its `Occluded(false)` will start them again).
            // wgpu's `Occluded` is the window not visible yet (a window
            // just opened, before AppKit shows it) or hidden: no failure,
            // looked at again soon; the pacing's report starts with the
            // first frame presented.
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.retry_at = Some(Instant::now() + OCCLUDED_RETRY);
                return;
            }
            other => {
                self.surface_failures += 1;
                if self.surface_failures <= 3 {
                    eprintln!("no frame from the surface: {other:?}");
                }
                s.surface.configure(&s.gpu.device, &s.config);
                self.retry_at = Some(Instant::now() + RETRY);
                return;
            }
        };
        self.surface_failures = 0;
        let fallbacks = self.pacer.fallbacks;
        let present_at = paced
            .then(|| self.pacer.acquired(acquire, crate::clock::now().0))
            .flatten();
        if self.pacer.fallbacks > fallbacks {
            eprintln!(
                "late pacing kept missing refreshes with its margin at its cap: \
                 blocking frames ({} in flight) until the window's size or \
                 fullscreen changes",
                self.pacer.in_flight()
            );
        }
        let mut sample = None;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let size = [s.config.width, s.config.height];
        // What the play draws through: the sRGB view, skins the plain one.
        let formats = s.formats;
        let scene_view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(formats.view),
            ..Default::default()
        });
        let plain = formats.plain.is_some().then_some(&view);
        let env = self.theme.as_ref().map(|(e, _)| e.clone());
        if self.play.playing()
            && let Some(env) = &env
        {
            // The play alone: its field, nothing of the UI under it.
            let dev = crate::play::host::Device {
                device: &s.gpu.device,
                queue: &s.gpu.queue,
                formats,
            };
            let mut encoder =
                s.gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("play"),
                    });
            self.play.scale = self.scale as f32;
            // Late pacing says which refresh the frame is for; else the
            // play predicts it.
            self.play.present_at = present_at.map(ddi_platform::HostTime);
            let now = crate::clock::now();
            sample = Some(now.0);
            self.play.draw_play(
                &dev,
                &mut encoder,
                &scene_view,
                plain,
                size,
                now,
                &mut self.ui,
                env,
            );
            s.gpu.queue.submit(Some(encoder.finish()));
        } else {
            if let Some(env) = &env {
                let feed = self.songs.as_ref().map(|songs| &songs.feed);
                s.gpu.movie.follow(&self.ui, env, feed);
            }
            s.gpu.draw(&mut self.ui, &view, size);
        }
        self.loaded = s.gpu.images_ready();
        s.window.pre_present_notify();
        let present = crate::clock::now().0;
        s.gpu.queue.present(frame);
        if paced && let Some(sample) = sample {
            let record = self.pacer.presented(sample, present, crate::clock::now().0);
            if let Some(r) = &mut self.pacing_report {
                r.push(record);
            }
        }
        self.frames += 1;
        // A play that ended in this frame: menus again.
        self.sync_play_mode();
        let r = self.ui.take_requests();
        self.run(r);
    }
}

impl Shell {
    /// The play's news: one asked for, its song read, the prompt's or
    /// the results' choice; the keyboard and present mode follow whether
    /// a play runs.
    fn poll_play(&mut self) {
        let Some((env, _)) = self.theme.clone() else {
            return;
        };
        let rate = self.sound.borrow().audio.sample_rate();
        let dev = self.surface.as_ref().map(|s| crate::play::host::Device {
            device: &s.gpu.device,
            queue: &s.gpu.queue,
            formats: s.formats,
        });
        self.play.poll(&mut self.ui, &env, dev, rate);
        self.sync_play_mode();
        if self.play.playing() {
            let mut feed = self.play.feed.borrow_mut();
            use ddi_platform::InputSource;
            self.input.keyboard.poll(&mut feed.edges);
            feed.pads = self
                .input
                .pads
                .as_ref()
                .map(crate::pads::Pads::connected)
                .unwrap_or_default();
        }
    }

    /// The devices looked at again (every second: the default output
    /// can change under the stream, the window move to another display),
    /// the notices' buttons done, the notices shown as they are now (none
    /// over a play, its prompt and results, or calibration's pages); a
    /// change during a play flags it. When to look again.
    fn watch_devices(&mut self) -> Option<Instant> {
        self.songs.as_ref()?;
        let actions: Vec<String> = self.notice_actions.borrow_mut().drain(..).collect();
        for id in actions {
            self.play.watcher.act(&id);
            if id == "calibrate"
                && let (Some((env, _)), Some(menu)) = (self.theme.clone(), self.play.menu.clone())
            {
                menu(&env, &mut self.ui);
            }
        }
        let due = self
            .play
            .watcher
            .checked
            .is_none_or(|c| c.elapsed() >= crate::devices::EVERY);
        if due {
            let audio = self
                .play
                .play_fingerprint()
                .or_else(|| self.sound.borrow().audio.fingerprint());
            let display = self
                .surface
                .as_ref()
                .and_then(|s| display_fingerprint(&s.window));
            if display.is_some() {
                self.play.display = display.clone();
            }
            if let Some(s) = &self.surface {
                self.play.refresh_hint = refresh_rate(&s.window);
                self.pacer.set_refresh_hint(self.play.refresh_hint);
            }
            if self.play.watcher.probe(audio, display) && self.play.playing() {
                self.play.feed.borrow_mut().device_changed = true;
            }
        }
        let calibrating = self
            .ui
            .layers()
            .iter()
            .any(|l| l.name.starts_with("calibration"));
        if self.play.watcher.publish(!self.play.idle() || calibrating) {
            self.ui
                .broadcast(Message::new(theme_screens::notices::NOTICES_CHANGED));
        }
        self.play.watcher.checked.map(|c| c + crate::devices::EVERY)
    }

    /// Idle: the loop wakes when a screen's deadline comes
    /// ([`Ui::idle_wake`], on [`crate::clock`]'s seconds).
    fn wake_ui(&mut self, event_loop: &ActiveEventLoop) {
        let now = crate::clock::now().0;
        let Some(at) = self
            .ui
            .idle_wake(&mut self.idle_since, now)
            .and_then(|t| std::time::Duration::try_from_secs_f64(t).ok())
            .and_then(|t| crate::clock::epoch().checked_add(t))
        else {
            return;
        };
        match event_loop.control_flow() {
            ControlFlow::WaitUntil(t) if t <= at => {}
            ControlFlow::Poll => {}
            _ => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
        }
    }

    /// The mouse pointer hidden on a key's or a pad's press, shown again
    /// when the mouse moves.
    fn hide_pointer(&mut self, hide: bool) {
        if self.pointer_hidden == hide {
            return;
        }
        self.pointer_hidden = hide;
        // On macOS AppKit hides it until the mouse moves: winit's invisible
        // cursor is made from a GIF that ImageIO on macOS 27 fails to read
        // (a bus error in `IIO_Reader_GIF::parse`).
        #[cfg(target_os = "macos")]
        if hide {
            objc2_app_kit::NSCursor::setHiddenUntilMouseMoves(true);
        }
        #[cfg(not(target_os = "macos"))]
        if let Some(s) = &self.surface {
            s.window.set_cursor_visible(!hide);
        }
    }

    /// Play mode follows the play: a play starts in a poll, and ends in a
    /// frame (`Host::draw_play`) or a poll.
    pub fn sync_play_mode(&mut self) {
        let now = self.play.playing();
        if now != self.play_on {
            self.play_on = now;
            self.play_mode(now);
        }
    }

    /// Whether play mode is on (the play's keys read, its presentation).
    pub fn play_mode_on(&self) -> bool {
        self.play_on
    }

    /// A play starts (`true`) or ends: its keys are read, previews stop,
    /// frames go out with the least latency the surface has (D6: Mailbox
    /// where there is one, else Fifo, one frame in flight).
    fn play_mode(&mut self, on: bool) {
        self.input.keyboard.enabled = on;
        // Keys from before are not the play's (nor the menus' after it).
        ddi_platform::InputSource::poll(&mut self.input.keyboard, &mut Vec::new());
        if on {
            self.sound.borrow_mut().pause();
            // The play is not the UI's: no pointer from the menus over it,
            // and the UI tells it again after.
            if let Some(s) = &self.surface {
                s.window.set_cursor(CursorIcon::Default);
            }
            self.ui.forget_cursor();
        }
        let link = self.surface.as_ref().and_then(|s| s.link.as_ref());
        if let Some(l) = link {
            l.run(on);
        }
        if on {
            self.pacer.restart();
            if let Some(r) = &mut self.pacing_report {
                r.hint = self.play.refresh_hint;
                r.link = link.map(|l| l.name());
            }
        } else if let Some(text) = self.pacing_text() {
            println!("{text}");
            if let Some(r) = &mut self.pacing_report {
                *r = r.fresh();
            }
        }
        self.present_mode(on);
    }

    /// The surface's present mode: in play, the least latency the
    /// surface has with blocking frames (D6: Mailbox where there is one,
    /// else Fifo), Fifo with late pacing (it paces against the
    /// refreshes), the pacer's frames in flight (`--in-flight`, two with a
    /// display link, one elsewhere); in menus AutoVsync, two.
    ///
    /// `--pacing-report` keeps Fifo for every column, so they differ only
    /// in pacing and frames in flight (Mailbox shows a frame up to a
    /// refresh sooner, which moves the notes about half a period against
    /// Fifo's); a switch of frames in flight configures the surface again.
    fn present_mode(&mut self, play: bool) {
        let fifo = self.pacer.mode() == pacing::Mode::Late || self.pacing_report.is_some();
        let in_flight = self.pacer.in_flight();
        let Some(s) = &mut self.surface else { return };
        let (mode, latency) = if !play {
            (wgpu::PresentMode::AutoVsync, 2)
        } else if !fifo && s.modes.contains(&wgpu::PresentMode::Mailbox) {
            (wgpu::PresentMode::Mailbox, in_flight)
        } else {
            (wgpu::PresentMode::Fifo, in_flight)
        };
        if (
            s.config.present_mode,
            s.config.desired_maximum_frame_latency,
        ) == (mode, latency)
        {
            return;
        }
        s.config.present_mode = mode;
        s.config.desired_maximum_frame_latency = latency;
        s.surface.configure(&s.gpu.device, &s.config);
    }

    /// When the play's next frame starts: `--pacing-report` sets the
    /// pacing this moment runs in first.
    fn pace(&mut self) -> Next {
        self.feed_link();
        let now = crate::clock::now().0;
        if let Some(r) = &mut self.pacing_report
            && r.choose(now, &mut self.pacer)
        {
            self.present_mode(true);
        }
        self.pacer.next(now)
    }

    /// The display link's last refresh, to the pacer.
    fn feed_link(&mut self) {
        if let Some(t) = self
            .surface
            .as_ref()
            .and_then(|s| s.link.as_ref())
            .and_then(|l| l.latest())
        {
            self.pacer.refresh(t.at, t.target);
        }
    }

    /// `--pacing-report`'s text for the play so far, if it drew frames.
    pub fn pacing_text(&self) -> Option<String> {
        let r = self.pacing_report.as_ref().filter(|r| !r.is_empty())?;
        let modes = self.surface.as_ref().map_or(String::new(), |s| {
            let mailbox = if s.modes.contains(&wgpu::PresentMode::Mailbox) {
                " (block alone uses Mailbox here)"
            } else {
                ""
            };
            format!(" · {} · every column on Fifo{mailbox}", self.play.graphics)
        });
        let every = r
            .alternate
            .filter(|_| r.columns.len() > 1)
            .map_or(String::new(), |e| format!(" · alternating every {e} s"));
        Some(r.text(&format!(
            "{}{modes}{every}",
            self.play.log.last().map_or("a play", |l| l.as_str())
        )))
    }
}

impl Default for Shell {
    fn default() -> Shell {
        Shell::new()
    }
}

/// Where the IME's candidate window goes: the caret, logical pixels.
fn ime_area(window: &Window, [x, y, w, h]: [f32; 4]) {
    window.set_ime_cursor_area(LogicalPosition::new(x, y), LogicalSize::new(w.max(1.0), h));
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.surface.is_some() {
            return;
        }
        if let Err(e) = self.open(event_loop) {
            self.error = Some(e);
            event_loop.exit();
        }
        // After the app is up: keys' own times (macOS).
        self.input.monitored = crate::keyboard::monitor::install();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let to_logical = |p: winit::dpi::PhysicalPosition<f64>, scale: f64| {
            let l = p.to_logical::<f32>(scale);
            [l.x, l.y]
        };
        match event {
            WindowEvent::CloseRequested => {
                self.request_quit();
                if self.quit_ready || self.songs.as_ref().is_some_and(|s| s.drained()) {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(_) => self.resized(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor;
                self.resized();
            }
            WindowEvent::Focused(f) => {
                if !f {
                    self.input.keyboard.release_all(crate::clock::now());
                }
                // Typing ends with the window's focus, as the web's
                // textarea blurs with its page.
                if !f && self.typing {
                    self.typing = false;
                    self.preedit = false;
                    if let Some(s) = &self.surface {
                        s.window.set_ime_allowed(false);
                    }
                    self.send(UiEvent::Text(TextEvent::Blurred));
                }
                self.send(UiEvent::Focus(f));
            }
            WindowEvent::Occluded(o) => {
                // A play stops when its window stays hidden, as the web's
                // does with its page (its frames stop); a moment hidden
                // (a fullscreen Space's transition) does not stop it.
                if o && self.play.playing() {
                    self.hidden.hide(Instant::now());
                } else if !o {
                    self.shown();
                }
                self.occluded = o;
                self.pending = false;
                self.last = None;
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.modifiers = Modifiers {
                    shift: s.shift_key(),
                    ctrl: s.control_key(),
                    alt: s.alt_key(),
                    meta: s.super_key(),
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.hide_pointer(false);
                self.cursor = to_logical(position, self.scale);
                self.pointer(MOUSE, PointerKind::Mouse, PointerPhase::Move, self.cursor);
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer(MOUSE, PointerKind::Mouse, PointerPhase::Leave, self.cursor)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let bit = match button {
                    MouseButton::Left => buttons::PRIMARY,
                    MouseButton::Right => buttons::SECONDARY,
                    MouseButton::Middle => buttons::MIDDLE,
                    _ => 0,
                };
                let phase = match state {
                    ElementState::Pressed => {
                        self.buttons |= bit;
                        PointerPhase::Down
                    }
                    ElementState::Released => {
                        self.buttons &= !bit;
                        PointerPhase::Up
                    }
                };
                // Only the primary button presses (other buttons change
                // what is held, as the DOM's `pointermove` would say).
                if bit == buttons::PRIMARY {
                    self.pointer(MOUSE, PointerKind::Mouse, phase, self.cursor);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // winit's positive y scrolls up; the DOM's down. Lines are
                // a wheel's notches (or fractions of one: Windows'
                // precision touchpads, X11's smooth scrolling); pixels
                // come from precise devices only (macOS's trackpads and
                // Magic Mouse, Wayland's touchpads), whose stream the UI
                // follows one for one (`ddi_ui::gesture::WheelStream`).
                // Its phases are not needed: a stream ends when its
                // events stop, momentum included.
                let (delta, source) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        wheel_pixels(
                            [-x * LINES_PER_NOTCH, -y * LINES_PER_NOTCH],
                            DeltaMode::Line,
                            [0.0; 2],
                        ),
                        WheelSource::Lines,
                    ),
                    MouseScrollDelta::PixelDelta(p) => {
                        let l = p.to_logical::<f32>(self.scale);
                        ([-l.x, -l.y], WheelSource::Precise)
                    }
                };
                self.send(UiEvent::Wheel(WheelEvent {
                    pos: self.cursor,
                    delta,
                    source,
                    modifiers: self.modifiers,
                    time: self.now_ms(),
                }));
            }
            WindowEvent::Touch(t) => {
                let phase = match t.phase {
                    TouchPhase::Started => PointerPhase::Down,
                    TouchPhase::Moved => PointerPhase::Move,
                    TouchPhase::Ended => PointerPhase::Up,
                    TouchPhase::Cancelled => PointerPhase::Cancel,
                };
                let pos = to_logical(t.location, self.scale);
                self.pointer(t.id + 1, PointerKind::Touch, phase, pos);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                let code = crate::keys::code(event.physical_key);
                // The OS's own time where it said (macOS), else now.
                let time = self.input.key_time(event.physical_key, down, event.repeat);
                self.input.keyboard.key(&code, down, event.repeat, time);
                if down {
                    self.hide_pointer(true);
                }
                // A play's keys are its own (`crate::play`): not the
                // menus' under it.
                if self.play.playing() {
                    return;
                }
                // Whether a field was typed into before this key: a key
                // that opens one (`/` for search) does not type itself.
                let was_typing = self.typing;
                self.send(UiEvent::Key(KeyEvent {
                    code,
                    key: crate::keys::key(&event.logical_key),
                    down,
                    repeat: event.repeat,
                    modifiers: self.modifiers,
                    composing: self.preedit,
                    time: time.0 * 1000.0,
                }));
                // A key's text is typed unless a composition is showing:
                // winit sends no `KeyboardInput` for keys an IME takes
                // (macOS's `interpretKeyEvents`, Windows' `WM_IME_*`, X11's
                // and Wayland's text input), and macOS inserts text that
                // never went through marked text (Latin with a Japanese
                // input source, a dead key's result) only as this `text`,
                // not as `Ime::Commit`. Control characters are dropped by
                // the field.
                if types_text(down, was_typing, self.typing, self.preedit, self.modifiers)
                    && let Some(text) = event.text
                {
                    self.send(UiEvent::Text(TextEvent::Commit(text.to_string())));
                }
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Enabled => {}
                // Not the end of typing: Windows sends it at every
                // `WM_IME_ENDCOMPOSITION` (each committed word), macOS when
                // the input source changes (英数). Only what it showed goes.
                Ime::Disabled => {
                    if std::mem::take(&mut self.preedit) {
                        self.send(UiEvent::Text(TextEvent::Preedit {
                            text: String::new(),
                            cursor: None,
                        }));
                    }
                }
                Ime::Preedit(text, cursor) => {
                    self.preedit = !text.is_empty();
                    self.send(UiEvent::Text(TextEvent::Preedit { text, cursor }));
                }
                Ime::Commit(text) => {
                    self.preedit = false;
                    self.send(UiEvent::Text(TextEvent::Commit(text)));
                }
            },
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
        self.ask();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.quitting && (self.quit_ready || self.songs.as_ref().is_some_and(|s| s.drained())) {
            event_loop.exit();
            return;
        }
        if self
            .lose_device_after
            .is_some_and(|t| self.start.elapsed().as_secs_f64() >= t)
        {
            self.lose_device_after = None;
            self.lose_device();
        }
        self.recover_device();
        // The library's news (its scan woke the loop), and the options
        // screen open over it.
        if let Some(songs) = &self.songs {
            // Stale plays worked out again (song select's bests follow
            // in the poll), never during a play.
            songs.rework(self.play.playing(), !self.play.idle());
            songs.poll(&mut self.ui);
            // `--edit`: the editor once the library has the song.
            if let Some((env, _)) = &self.theme
                && let Some((id, chart)) = songs.edit_ready()
            {
                match crate::editor::screen(env, songs, &id, chart) {
                    Ok(screen) => self.ui.push(screen),
                    Err(e) => eprintln!("--edit {id}: {e}"),
                }
            }
            let invalidated = crate::library::take_invalidated_previews();
            if !invalidated.is_empty() {
                self.sound.borrow_mut().invalidate(&invalidated);
            }
            if songs.drained() {
                event_loop.exit();
                return;
            }
            // A library song's preview made ready on its thread.
            if crate::library::take_prepared() {
                self.sound.borrow_mut().refind();
            }
            if let Some(o) = songs.options.borrow().clone() {
                if o.take_pending() {
                    self.ui
                        .broadcast(Message::new(theme_screens::options::OPTIONS_CHANGED));
                }
                self.input.options = Some(o);
            }
        }
        // The controllers' news (their thread woke the loop): a play's
        // own while it runs, else the menus'.
        let playing = self.play.playing();
        for edge in self.input.poll(&mut self.ui) {
            if edge.pressed {
                self.hide_pointer(true);
            }
            if playing {
                self.play.feed.borrow_mut().edges.push(edge);
            } else if let Some(e) = self.input.route(&self.ui, edge) {
                self.send(e);
            }
        }
        self.poll_play();
        let watch_at = self.watch_devices();
        if let Some(settings) = self.input.take_edited()
            && let Some(s) = &mut self.surface
        {
            s.gpu.field.set_settings(&settings);
        }
        // Back on song select after a play (its results and prompt gone):
        // the selected song's preview plays again.
        if !self.quitting
            && self.sound.borrow().paused()
            && !self.play.playing()
            && self
                .ui
                .layers()
                .last()
                .is_some_and(|l| l.name == "select" && l.phase != "out")
        {
            if let Some((env, _)) = &self.theme {
                let id = env.state.borrow().song_id.clone();
                if !id.is_empty() {
                    self.sound.borrow_mut().select(Some(&id));
                }
            }
            self.sound.borrow_mut().resume();
        }
        // Sound moves on (a preview settled, decoded, looped); a preview
        // that started draws a frame, so the pulse starts.
        let (wake, started) = {
            let mut p = self.sound.borrow_mut();
            (p.tick(), p.take_started())
        };
        if started {
            self.ui.broadcast(Message::new("PreviewStarted"));
        }
        if let Some(limit) = self.quit_after {
            if self.start.elapsed().as_secs_f64() >= limit {
                self.request_quit();
                if self.quit_ready || self.songs.as_ref().is_some_and(|s| s.drained()) {
                    event_loop.exit();
                } else {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + std::time::Duration::from_millis(50),
                    ));
                }
                return;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + std::time::Duration::from_millis(100),
            ));
        } else if self.songs.as_ref().is_some_and(|s| s.importing()) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + std::time::Duration::from_millis(50),
            ));
        } else if let Some(at) = self.retry_at {
            event_loop.set_control_flow(ControlFlow::WaitUntil(at));
        } else if self.surface.as_ref().is_some_and(|s| s.gpu.loading()) {
            // Images are decoding on other threads: looked for shortly
            // (only while they are).
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + std::time::Duration::from_millis(8),
            ));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
        // The prompt waiting for the play's sound: looked at again soon.
        let sound_at = self
            .play
            .wants_poll()
            .then(|| Instant::now() + std::time::Duration::from_millis(15));
        let rework_at = self.songs.as_ref().and_then(|s| s.rework_due());
        if let Some(at) = wake
            .into_iter()
            .chain(watch_at)
            .chain(sound_at)
            .chain(rework_at)
            .min()
        {
            match event_loop.control_flow() {
                ControlFlow::WaitUntil(t) if t <= at => {}
                ControlFlow::Poll => {}
                _ => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
            }
        }
        if let Some(s) = &mut self.surface {
            self.loaded |= s.gpu.images_ready();
        }
        let field = self
            .surface
            .as_ref()
            .is_some_and(|s| s.gpu.field.shown() || s.gpu.movie.busy() || s.gpu.editor.busy())
            || self.play.playing();
        if self.play.playing() && !self.occluded {
            match self.pace() {
                // Blocking frames come one after another (each waits for
                // the display in `get_current_texture`).
                Next::Now if self.quit_after.is_none() => {
                    event_loop.set_control_flow(ControlFlow::Poll);
                }
                Next::Now => {}
                // Late pacing sleeps until the frame's start.
                Next::At(t) => {
                    let at = crate::clock::epoch() + std::time::Duration::from_secs_f64(t);
                    match event_loop.control_flow() {
                        ControlFlow::WaitUntil(w) if w <= at => {}
                        _ => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
                    }
                }
            }
        }
        if self.occluded || !(self.ui.needs_redraw() || self.loaded || field) {
            // Idle: the next frame starts its clock afresh.
            self.last = None;
            self.wake_ui(event_loop);
        }
        self.ask();
    }
}

/// A UI running the theme's screens on sample data, `first` on top: song
/// select under it, at the theme's design. Desktop has no system setting
/// for reduced motion: motion is reduced only when the settings say so.
pub fn theme_ui(first: &str, fold: FoldFn) -> (Ui, ThemeEnv, Rc<ddi_theme::Infinito>) {
    theme_ui_with(first, fold, None)
}

/// [`theme_ui`]; with `sound`, song select's previews play and its pulse
/// follows them (the made-up songs, which have no music, pulse by their
/// BPM).
pub fn theme_ui_with(
    first: &str,
    fold: FoldFn,
    sound: Option<SharedPlayer>,
) -> (Ui, ThemeEnv, Rc<ddi_theme::Infinito>) {
    let (mut ui, env, look) = theme_base(fold, sound.as_ref().map(|p| preview_beat(p.clone())));
    let changed = sample::motion_change(&env, || false);
    let select = match &sound {
        Some(sound) => sample_select(&env, sound.clone()),
        None => sample::select_screen(&env, Some(changed.clone())),
    };
    if let Some(sound) = sound.clone() {
        ui.push(title_screen(&env, sound, None));
    } else {
        ui.push(sample::start_screen(&env, true));
    }
    if first != "start" {
        ui.push(select);
    }
    match first {
        "results" => ui.push(sample::results_screen(&env)),
        "options" => ui.push(sample::options_screen(&env, Some(changed))),
        "credits" => ui.push(credits_screen(&env)),
        "calibration" => ui.push(sample::calibration_screen(&env)),
        _ => {}
    }
    (ui, env, look)
}

/// The theme's song select on the library on disk (`first`: `select`,
/// or `options` over it), its previews and pulse from `sound`.
pub fn library_ui(
    first: &str,
    fold: FoldFn,
    sound: SharedPlayer,
    songs: &Rc<crate::songs::Songs>,
) -> (Ui, ThemeEnv, Rc<ddi_theme::Infinito>) {
    // The pulse follows a preview as heard, else the song's BPM.
    let beat: theme_screens::BeatFn = {
        let player = sound.clone();
        let by_bpm = theme_screens::bpm_beat();
        Rc::new(move |id, t, bpm| player.borrow().beat(id).or_else(|| by_bpm(id, t, bpm)))
    };
    let (mut ui, env, look) = theme_base(fold, Some(beat));
    ui.push(title_screen(&env, sound.clone(), Some(songs.clone())));
    if first != "start" && first != "options" {
        ui.push(crate::songs::screen(&env, songs, Some(sound.clone())));
    }
    if first == "options" {
        let (screen, shared) = crate::options::screen(&env, Some(sound), Some(songs.clone()));
        *songs.options.borrow_mut() = Some(shared);
        ui.push(screen);
    }
    (ui, env, look)
}

/// The theme's look, its environment (`beat` its pulse, when given;
/// motion reduced as the settings say) and a UI for its screens.
fn theme_base(
    fold: FoldFn,
    beat: Option<theme_screens::BeatFn>,
) -> (Ui, ThemeEnv, Rc<ddi_theme::Infinito>) {
    let look = Rc::new(ddi_theme::Infinito::new());
    let mut env = ThemeEnv::new(look.clone(), fold);
    if let Some(beat) = beat {
        env.beat = beat;
    }
    let settings = ddi_ui::state::settings::Settings::load();
    env.follow_motion(settings.reduced_motion, false);
    let mut ui = Ui::new(theme_screens::DESIGN, TextMode::Own, false);
    ui.set_focus_ring(false);
    (ui, env, look)
}

/// The beat as the preview is heard; by the BPM for songs without music.
fn preview_beat(player: SharedPlayer) -> theme_screens::BeatFn {
    let with_music: std::collections::HashSet<String> =
        sample::bundled().into_iter().map(|s| s.id).collect();
    let by_bpm = theme_screens::bpm_beat();
    Rc::new(move |id, t, bpm| {
        if with_music.contains(id) {
            player.borrow().beat(id)
        } else {
            by_bpm(id, t, bpm)
        }
    })
}

/// Skins' SVGs drawn for the window's display (moved to another, the next
/// skin loaded is drawn for that one).
fn screen_height(window: &Window) {
    if let Some(m) = window.current_monitor() {
        crate::play::skins::set_screen_height(m.size().height);
    }
}

/// The window's display as the offsets' profile names it (D6, AGENTS.md):
/// `display:<monitor name>:<w>x<h>@<scale>`, the refresh rate left out
/// (variable-rate panels change it).
fn display_fingerprint(window: &Window) -> Option<ddi_platform::DeviceFingerprint> {
    let m = window.current_monitor()?;
    let name = m.name().unwrap_or_else(|| "display".into());
    let size = m.size();
    let scale = m.scale_factor();
    Some(ddi_platform::DeviceFingerprint::new(
        format!("display:{name}:{}x{}@{scale}", size.width, size.height),
        format!("{name} · {}×{} · ×{scale}", size.width, size.height),
    ))
}

/// The window's display's refresh rate as the OS says it (Hz).
fn refresh_rate(window: &Window) -> Option<f64> {
    let mhz = window.current_monitor()?.refresh_rate_millihertz()?;
    (mhz > 0).then(|| f64::from(mhz) / 1000.0)
}

/// Whether a key's text is typed into the field: on a press, while a
/// field was already typed into before the key (the key that opens one,
/// `/` for search, does not type itself) and still is, with no
/// composition showing and no command held.
fn types_text(down: bool, was_typing: bool, typing: bool, preedit: bool, m: Modifiers) -> bool {
    down && was_typing && typing && !preedit && !m.command()
}

/// The credits page: the generated credits when they are there (`xtask
/// gen-credits`: the desktop's, with its own Rust libraries), found
/// beside the assets as the bundled songs' index is (not from the
/// working folder), else the sample.
pub(crate) fn credits_screen(env: &ThemeEnv) -> theme_screens::page::PageScreen {
    let assets = crate::images::assets();
    let credits = [
        assets.join("../target/credits/credits-desktop.json"),
        assets.join("credits/credits-desktop.json"),
    ]
    .iter()
    .find_map(|p| std::fs::read_to_string(p).ok())
    .and_then(|t| theme_screens::credits::parse(&t).ok())
    .unwrap_or_else(theme_screens::credits::sample);
    sample::credits_screen(
        env,
        &theme_screens::credits::Credits {
            platform: theme_screens::data::Platform::Desktop,
            ..credits
        },
    )
}

/// Sample screens still obey the real preview's ownership rules.
fn sample_select(env: &ThemeEnv, sound: SharedPlayer) -> theme_screens::select::SelectScreen {
    let play_sound = sound.clone();
    let options_sound = sound.clone();
    let calls: ShellFn = Rc::new(move |call, cx| match call {
        SelectCall::Selected { song, .. } => sound.borrow_mut().select(song.as_deref()),
        SelectCall::Leave => {
            sound.borrow_mut().pause();
            cx.close(true);
        }
        _ => {}
    });
    theme_screens::select::SelectScreen::new(
        env.clone(),
        sample::select(),
        Some("some-things-must"),
        Rc::new(move |_, _, env, cx| {
            play_sound.borrow_mut().pause();
            cx.push(sample::results_screen(env));
        }),
        Rc::new(move |env, cx| {
            options_sound.borrow_mut().pause();
            cx.push(sample::options_screen(
                env,
                Some(sample::motion_change(env, || false)),
            ));
        }),
    )
    .with_shell(calls, SharedFeed::default())
}

fn title_screen(
    env: &ThemeEnv,
    sound: SharedPlayer,
    songs: Option<Rc<crate::songs::Songs>>,
) -> theme_screens::start::StartScreen {
    let e = env.clone();
    theme_screens::start::StartScreen::new(
        env.clone(),
        true,
        Rc::new(move |choice, cx| {
            use theme_screens::start::Choice;
            sound.borrow_mut().pause();
            match choice {
                Choice::Singleplayer => {
                    if let Some(songs) = &songs {
                        cx.push(crate::songs::screen(&e, songs, Some(sound.clone())));
                    } else {
                        cx.push(sample_select(&e, sound.clone()));
                    }
                }
                Choice::Options if songs.is_none() => {
                    cx.push(sample::options_screen(
                        &e,
                        Some(sample::motion_change(&e, || false)),
                    ));
                }
                Choice::Options => {
                    let (screen, shared) =
                        crate::options::screen(&e, Some(sound.clone()), songs.clone());
                    if let Some(songs) = &songs {
                        *songs.options.borrow_mut() = Some(shared);
                        songs.tell_options();
                    }
                    cx.push(screen);
                }
                Choice::Credits => cx.push(credits_screen(&e)),
                Choice::Quit => cx.request(UiRequest::Quit),
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window's icon (`window_icon`) draws: the margin transparent,
    /// the tile dark, the arrow pink.
    #[test]
    fn the_window_icon_draws() {
        let svg = include_str!("../../../tools/package/icon.svg");
        let rgba = ddi_noteskin::raster::rasterize(svg, 64, 64).unwrap();
        let at = |x: usize, y: usize| &rgba[(y * 64 + x) * 4..][..4];
        assert_eq!(at(1, 1)[3], 0);
        assert_eq!(at(8, 32), [0x16, 0x13, 0x18, 0xff]);
        assert_eq!(at(32, 45), [0xff, 0x6e, 0xa0, 0xff]);
    }

    /// A play's window hidden for a moment (macOS's fullscreen Space
    /// entered or left) goes on; hidden longer, it stops. Shown again
    /// without having been hidden during a play, nothing.
    #[test]
    fn a_window_hidden_for_a_moment_keeps_the_play() {
        let t = Instant::now();
        let ms = std::time::Duration::from_millis;
        let mut h = Hidden::default();
        assert_eq!(h.show(t), None);
        h.hide(t);
        // Hidden again before it showed: from the first time.
        h.hide(t + ms(300));
        assert_eq!(h.show(t + ms(700)), Some((ms(700), false)));
        assert_eq!(h.show(t + ms(800)), None);
        h.hide(t);
        assert_eq!(h.show(t + HIDDEN_GRACE), Some((HIDDEN_GRACE, false)));
        h.hide(t);
        assert_eq!(h.show(t + ms(5000)), Some((ms(5000), true)));
    }

    #[test]
    fn quit_without_importer_is_ready_and_idempotent() {
        crate::audio::set_headless(true);
        let mut shell = Shell::named("start");
        shell.run(vec![UiRequest::Quit, UiRequest::Quit]);
        assert!(shell.quitting);
        assert!(shell.quit_ready);
        assert!(shell.sound.borrow().status().1.is_none());
    }

    #[test]
    fn the_key_that_opens_a_field_does_not_type_itself() {
        let none = Modifiers::default();
        // `/` opened search: not typed.
        assert!(!types_text(true, false, true, false, none));
        // The next keys are.
        assert!(types_text(true, true, true, false, none));
        // A key that closed the field, a release, a composition, a command.
        assert!(!types_text(true, true, false, false, none));
        assert!(!types_text(false, true, true, false, none));
        assert!(!types_text(true, true, true, true, none));
        let cmd = Modifiers {
            meta: cfg!(target_os = "macos"),
            ctrl: !cfg!(target_os = "macos"),
            ..Modifiers::default()
        };
        assert!(!types_text(true, true, true, false, cmd));
    }
}
