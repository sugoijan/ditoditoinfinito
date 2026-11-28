//! What the system itself says of the window, where winit's own notion
//! can be out of step with it (round WN2, `window_state::restore`).
//!
//! On macOS winit 0.30 records a fullscreen request before AppKit takes
//! it (`set_fullscreen` replaces its state, then calls
//! `toggleFullScreen:`): a request AppKit refuses without starting a
//! transition leaves `Window::fullscreen` saying fullscreen on a window
//! that is not, and asking winit again for the same is then nothing
//! (`fullscreen == old_fullscreen` returns). So the window's own style
//! mask says whether it is fullscreen, `isOnActiveSpace` whether its
//! Space shows, and a request made again where winit is out of step is
//! AppKit's `toggleFullScreen:` itself, which winit follows from its
//! delegate (`windowWillEnterFullScreen:` keeps its state, or sets it to
//! the window's display where it had none). Elsewhere winit's own state
//! is the window's (Windows and X11 set it as they are asked).

use winit::window::Window;

/// Whether the window is fullscreen, as the system says.
pub fn is_fullscreen(window: &Window) -> bool {
    #[cfg(target_os = "macos")]
    if let Some(w) = imp::ns_window(window) {
        return w
            .styleMask()
            .contains(objc2_app_kit::NSWindowStyleMask::FullScreen);
    }
    window.fullscreen().is_some()
}

/// Whether the window's Space is the one shown (macOS; true elsewhere).
pub fn on_active_space(window: &Window) -> bool {
    #[cfg(target_os = "macos")]
    if let Some(w) = imp::ns_window(window) {
        return w.isOnActiveSpace();
    }
    let _ = window;
    true
}

/// How a request for fullscreen was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Through winit, on the display given.
    Winit,
    /// AppKit's `toggleFullScreen:` itself, winit's state saying
    /// fullscreen already (macOS).
    Toggle,
}

/// Asks the window, not fullscreen as the system says, for fullscreen on
/// `on` (its own display where none): through winit where winit agrees
/// it is not, else (macOS, winit out of step) by AppKit's own toggle.
pub fn enter_fullscreen(window: &Window, on: Option<winit::monitor::MonitorHandle>) -> Asked {
    #[cfg(target_os = "macos")]
    if window.fullscreen().is_some()
        && let Some(w) = imp::ns_window(window)
    {
        w.toggleFullScreen(None);
        return Asked::Toggle;
    }
    window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(on)));
    Asked::Winit
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSView, NSWindow};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    /// The window's `NSWindow`, through its view.
    pub fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return None;
        };
        // SAFETY: winit's pointer to the window's view, which lives as
        // long as the window; read on the main thread (the loop's).
        let view: &NSView = unsafe { h.ns_view.cast().as_ref() };
        view.window()
    }
}
