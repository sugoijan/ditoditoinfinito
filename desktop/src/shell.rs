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

use std::sync::Arc;
use std::time::Instant;

use ddi_platform::storage::{self, Area};
use std::rc::Rc;

use ddi_ui::dev::{self, DevScreen, Shared};
use ddi_ui::dev_widgets::{FoldFn, SharedWidgets, WidgetScreen};
use ddi_ui::event::{
    DeltaMode, KeyEvent, LINES_PER_NOTCH, Modifiers, PointerEvent, PointerKind, PointerPhase,
    TextEvent, UiEvent, WheelEvent, buttons, wheel_pixels,
};
use ddi_ui::request::Cursor;
use ddi_ui::{TextMode, Ui, UiRequest};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{CursorIcon, Window, WindowId};

use crate::gpu::{self, Gpu};

/// How long to wait before asking the surface for a frame again.
const RETRY: std::time::Duration = std::time::Duration::from_millis(250);

/// The mouse's pointer id; touches are numbered after it.
const MOUSE: u64 = 0;

struct Surface {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    gpu: Gpu,
}

pub struct Shell {
    ui: Ui,
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
    /// A redraw was asked for and has not come: not asked again (a window
    /// that cannot draw, a locked screen, must not spin the loop).
    pending: bool,
    /// The surface gave no frame: when to try again.
    retry_at: Option<Instant>,
    start: Instant,
    last: Option<Instant>,
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
}

impl Shell {
    pub fn new() -> Shell {
        Shell::with_screen(false)
    }

    /// The developer screen; with `widgets`, the widget screen over it.
    pub fn with_screen(widgets: bool) -> Shell {
        let dev = Shared::default();
        let fold: FoldFn = Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&crate::Nfkd, s));
        let shared = SharedWidgets::default();
        let mut ui = Ui::new(dev::DESIGN, TextMode::Own, false);
        ui.push(DevScreen::with_widgets(
            dev.clone(),
            shared.clone(),
            fold.clone(),
        ));
        if widgets {
            ui.push(WidgetScreen::new(shared, fold));
        }
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
            pending: false,
            retry_at: None,
            start: Instant::now(),
            last: None,
            frames: 0,
            asked: 0,
            occluded_skips: 0,
            surface_failures: 0,
            sounds: 0,
            files: 0,
            quit_after: None,
            error: None,
        }
    }

    fn now_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Dito Dito Infinito")
                    .with_inner_size(LogicalSize::new(1280.0, 720.0)),
            )
            .map_err(|e| e.to_string())?;
        let window = Arc::new(window);
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
        surface.configure(&device, &config);
        self.scale = window.scale_factor();
        self.surface = Some(Surface {
            window,
            surface,
            config,
            gpu: Gpu::new(device, queue, format),
        });
        self.resized();
        Ok(())
    }

    fn resized(&mut self) {
        self.occluded = false;
        let Some(s) = &mut self.surface else { return };
        let size = s.window.inner_size();
        s.config.width = size.width.max(1);
        s.config.height = size.height.max(1);
        s.surface.configure(&s.gpu.device, &s.config);
        let logical = size.to_logical::<f32>(self.scale);
        self.send(UiEvent::Resize {
            size: [logical.width.max(1.0), logical.height.max(1.0)],
            ratio: self.scale as f32,
        });
    }

    fn send(&mut self, event: UiEvent) {
        let out = self.ui.handle(event);
        self.run(out.requests);
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
                    // Sound comes with the rest of phase 9.
                    self.sounds += 1;
                    self.dev.borrow_mut().status = "beep (no sound on desktop yet)".into();
                }
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
                UiRequest::SetText { .. }
                | UiRequest::Navigate(_)
                | UiRequest::PlayPreview { .. }
                | UiRequest::StopPreview
                | UiRequest::Redraw => {}
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
        if self.occluded || self.pending || !self.ui.needs_redraw() {
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
        let dt = self
            .last
            .map_or(1.0 / 60.0, |l| (now - l).as_secs_f32().min(0.25));
        self.last = Some(now);
        self.ui.update(dt);
        let r = self.ui.take_requests();
        self.run(r);
        let Some(s) = &mut self.surface else { return };
        let frame = match s.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                s.surface.configure(&s.gpu.device, &s.config);
                f
            }
            // Hidden (minimized, a locked screen), or no frame in time:
            // tried again shortly (only winit's `Occluded` stops frames, as
            // its `Occluded(false)` will start them again).
            other => {
                self.surface_failures += 1;
                if self.surface_failures <= 3 {
                    eprintln!("no frame from the surface: {other:?}");
                }
                if !matches!(other, wgpu::CurrentSurfaceTexture::Occluded) {
                    s.surface.configure(&s.gpu.device, &s.config);
                }
                self.retry_at = Some(Instant::now() + RETRY);
                return;
            }
        };
        self.surface_failures = 0;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        s.gpu
            .draw(&mut self.ui, &view, [s.config.width, s.config.height]);
        s.window.pre_present_notify();
        s.gpu.queue.present(frame);
        self.frames += 1;
        let r = self.ui.take_requests();
        self.run(r);
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
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let to_logical = |p: winit::dpi::PhysicalPosition<f64>, scale: f64| {
            let l = p.to_logical::<f32>(scale);
            [l.x, l.y]
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => self.resized(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor;
                self.resized();
            }
            WindowEvent::Focused(f) => {
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
                // winit's positive y scrolls up; the DOM's down.
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => wheel_pixels(
                        [-x * LINES_PER_NOTCH, -y * LINES_PER_NOTCH],
                        DeltaMode::Line,
                        [0.0; 2],
                    ),
                    MouseScrollDelta::PixelDelta(p) => {
                        let l = p.to_logical::<f32>(self.scale);
                        [-l.x, -l.y]
                    }
                };
                self.send(UiEvent::Wheel(WheelEvent {
                    pos: self.cursor,
                    delta,
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
                self.send(UiEvent::Key(KeyEvent {
                    code: crate::keys::code(event.physical_key),
                    key: crate::keys::key(&event.logical_key),
                    down,
                    repeat: event.repeat,
                    modifiers: self.modifiers,
                    composing: self.preedit,
                    time: self.now_ms(),
                }));
                // A key's text is typed unless a composition is showing:
                // winit sends no `KeyboardInput` for keys an IME takes
                // (macOS's `interpretKeyEvents`, Windows' `WM_IME_*`, X11's
                // and Wayland's text input), and macOS inserts text that
                // never went through marked text (Latin with a Japanese
                // input source, a dead key's result) only as this `text`,
                // not as `Ime::Commit`. Control characters are dropped by
                // the field.
                if down
                    && self.typing
                    && !self.preedit
                    && !self.modifiers.command()
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
        if let Some(limit) = self.quit_after {
            if self.start.elapsed().as_secs_f64() >= limit {
                event_loop.exit();
                return;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + std::time::Duration::from_millis(100),
            ));
        } else if let Some(at) = self.retry_at {
            event_loop.set_control_flow(ControlFlow::WaitUntil(at));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
        if self.occluded || !self.ui.needs_redraw() {
            // Idle: the next frame starts its clock afresh.
            self.last = None;
        }
        self.ask();
    }
}
