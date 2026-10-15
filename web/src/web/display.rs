//! Display identity from what a page can observe: the screen the window is
//! on (logical size, device pixel ratio) and the measured refresh rate.
//! `screen.width/height` describe the screen, not the window, so they only
//! change when the window moves to another display.

use ddi_platform::DeviceFingerprint;
use gloo::events::EventListener;
use web_sys::MediaQueryList;

/// Common panel refresh rates; a measured rate snaps to the nearest.
const RATES: [f64; 10] = [
    30.0, 48.0, 50.0, 60.0, 72.0, 75.0, 90.0, 120.0, 144.0, 165.0,
];

pub(crate) fn snap_refresh_rate(measured_hz: f64) -> f64 {
    if !measured_hz.is_finite() || measured_hz <= 0.0 {
        return 60.0;
    }
    if measured_hz > 200.0 {
        return 240.0;
    }
    RATES
        .iter()
        .copied()
        .min_by(|a, b| {
            (a - measured_hz)
                .abs()
                .partial_cmp(&(b - measured_hz).abs())
                .unwrap()
        })
        .unwrap_or(60.0)
}

pub(crate) struct ScreenInfo {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) dpr: f64,
}

pub(crate) fn screen_info() -> ScreenInfo {
    let window = web_sys::window();
    let (width, height) = window
        .as_ref()
        .and_then(|w| w.screen().ok())
        .map(|s| {
            (
                s.width().unwrap_or(0).max(0) as u32,
                s.height().unwrap_or(0).max(0) as u32,
            )
        })
        .unwrap_or((0, 0));
    let dpr = window.map(|w| w.device_pixel_ratio()).unwrap_or(1.0);
    ScreenInfo { width, height, dpr }
}

/// Fingerprint for the current screen. The id is the screen's logical size
/// and scale only: the refresh rate is informative but not an identity,
/// since variable-rate panels (ProMotion) change it with the content, so it
/// goes into the label.
pub(crate) fn fingerprint(measured_hz: f64) -> DeviceFingerprint {
    let s = screen_info();
    let hz = snap_refresh_rate(measured_hz);
    let dpr = (s.dpr * 100.0).round() / 100.0;
    DeviceFingerprint::new(
        display_id(s.width, s.height, dpr),
        format!("{}×{} ({dpr}×), ~{hz} Hz", s.width, s.height),
    )
}

pub(crate) fn display_id(width: u32, height: u32, dpr: f64) -> String {
    format!("display:{width}x{height}@{dpr}")
}

/// Fires `on_change` when the device pixel ratio changes, which happens when
/// the window moves to a display with a different scale. Returns the guard
/// (and the query it watches) to keep alive.
pub(crate) fn dpr_change_listener(
    on_change: impl Fn() + 'static,
) -> Option<(MediaQueryList, EventListener)> {
    let window = web_sys::window()?;
    let dpr = window.device_pixel_ratio();
    let query = window
        .match_media(&format!("(resolution: {dpr}dppx)"))
        .ok()??;
    let listener = EventListener::new(&query, "change", move |_| on_change());
    Some((query, listener))
}
