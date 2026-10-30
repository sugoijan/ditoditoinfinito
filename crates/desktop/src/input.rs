//! The window's input beyond winit's events (`docs/plans/desktop.md`,
//! 7.4): keys' times ([`crate::keyboard`]), the controllers
//! ([`crate::pads`]), whose edges and connected pads go to the UI core
//! as they are (it resolves them to menu actions, or hands them on to a
//! screen binding controls, `docs/plans/controller-input-core.md`); and
//! the options screen's view of them.

use std::rc::Rc;
use std::time::Duration;

use ddi_engine::ConnectedPad;
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_ui::Ui;
use ddi_ui::event::UiEvent;
use ddi_ui::state::settings::Settings;
use winit::keyboard::PhysicalKey;
use winit::platform::scancode::PhysicalKeyExtScancode;

use crate::keyboard::{DesktopKeyboard, Stamps};
use crate::pads::{FakePad, PadEvent, PadOut, Pads};

thread_local! {
    /// The controllers connected as last read (the editor's recording
    /// reads its lanes with them, as a play).
    static CONNECTED: std::cell::RefCell<Vec<ConnectedPad>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The controllers connected as last read.
pub fn connected_pads() -> Vec<ConnectedPad> {
    CONNECTED.with(|c| c.borrow().clone())
}

#[derive(Default)]
pub struct Input {
    pub keyboard: DesktopKeyboard,
    pub stamps: Stamps,
    /// The OS's key times are read (macOS's monitor is installed).
    pub monitored: bool,
    pub pads: Option<Pads>,
    pub options: Option<Rc<crate::options::Shared>>,
    /// The fake controller's feed, while a script plays it.
    fake: Option<FakePad>,
    /// Pad edges that went to menus, and to a screen binding.
    pub menu_edges: u64,
    pub raw_edges: u64,
}

impl Input {
    /// The time of winit's key event: the OS's stamp of it, else now.
    pub fn key_time(&mut self, key: PhysicalKey, down: bool, repeat: bool) -> HostTime {
        crate::keyboard::monitor::take(&mut self.stamps);
        // A key winit cannot name keeps the OS's own code (JIS Eisu, Kana
        // and Ro on macOS).
        let code = match key {
            PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::MacOS(c)) => {
                Some(u32::from(c))
            }
            key => key.to_scancode(),
        };
        self.stamps.take(code, down, repeat, crate::clock::now())
    }

    /// Starts the controllers' thread: gilrs, or with `fake` a scripted
    /// pad ([`script`]); `wake` wakes the event loop.
    pub fn start_pads(&mut self, wake: Box<dyn Fn() + Send>, fake: Option<&str>) {
        match fake {
            Some(text) => {
                let (pads, feed) = Pads::fake(wake);
                let events = script(text);
                let f = feed.clone();
                let _ = std::thread::Builder::new()
                    .name("ddi-fake-pad".into())
                    .spawn(move || {
                        let start = std::time::Instant::now();
                        for (at, e) in events {
                            let wait = Duration::from_secs_f64(at).saturating_sub(start.elapsed());
                            std::thread::sleep(wait);
                            f.send(e);
                        }
                    });
                self.pads = Some(pads);
                self.fake = Some(feed);
            }
            None => self.pads = Some(Pads::start(wake)),
        }
    }

    /// The controllers' news: the UI told who is connected (before their
    /// edges, so a pad's first press finds it), the options too with what
    /// they hold; their edges, in order, for [`Input::route`].
    pub fn poll(&mut self, ui: &mut Ui) -> Vec<RawInput> {
        let Some(pads) = &mut self.pads else {
            return Vec::new();
        };
        let news = pads.take();
        if news.is_empty() {
            return Vec::new();
        }
        let connected = pads.connected();
        ui.set_pads(connected.clone());
        CONNECTED.with(|c| c.borrow_mut().clone_from(&connected));
        if let Some(o) = &self.options {
            let mut held = o.held.borrow_mut();
            held.clear();
            for p in &connected {
                held.insert(p.id.clone(), pads.held(&p.id));
            }
            drop(held);
            o.set_pads(connected, ui);
        }
        news.into_iter()
            .filter_map(|n| match n {
                PadOut::Edge(e) if matches!(e.device, DeviceId::Gamepad(_)) => Some(e),
                _ => None,
            })
            .collect()
    }

    /// A pad's edge as the UI's event, counted by where the UI sends it
    /// now (each edge after the one before was handled: a press that
    /// opens the bind flow sends the next ones raw).
    pub fn route(&mut self, ui: &Ui, e: RawInput) -> UiEvent {
        if ui.wants_raw() {
            self.raw_edges += 1;
        } else {
            self.menu_edges += 1;
        }
        UiEvent::Pad(e)
    }

    /// The settings the options changed since the last call.
    pub fn take_edited(&mut self) -> Option<Settings> {
        self.options.as_ref()?.take_edited()
    }

    /// One line for the end of a timed run.
    pub fn summary(&self) -> String {
        format!(
            "input: keys {} by the OS's time, {} on receipt (monitor {}) · pads {} \
             connected, {} edges ({} to menus, {} to binding){}",
            self.stamps.paired,
            self.stamps.unpaired,
            if self.monitored { "on" } else { "off" },
            self.pads.as_ref().map_or(0, |p| p.connected().len()),
            self.pads.as_ref().map_or(0, |p| p.edges_seen),
            self.menu_edges,
            self.raw_edges,
            if self.fake.is_some() {
                " · fake pad"
            } else {
                ""
            },
        )
    }
}

/// A fake standard pad's script (`--fake-pad`): `seconds:button±` steps
/// apart by commas, the W3C numbers (`1.0:13+,1.1:13-` presses the
/// d-pad's down at a second for a tenth); the pad connects at once as
/// "Fake Pad".
pub fn script(text: &str) -> Vec<(f64, PadEvent)> {
    let mut out = vec![(
        0.0,
        PadEvent::Connected {
            pad: 0,
            name: "Fake Pad".into(),
            vendor: None,
            product: None,
            standard: true,
        },
    )];
    for step in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let Some((at, control)) = step.split_once(':') else {
            continue;
        };
        let (index, pressed) = match control.strip_suffix('+') {
            Some(i) => (i, true),
            None => match control.strip_suffix('-') {
                Some(i) => (i, false),
                None => continue,
            },
        };
        if let (Ok(at), Ok(index)) = (at.parse::<f64>(), index.parse::<u16>()) {
            out.push((
                at,
                PadEvent::Button {
                    pad: 0,
                    index,
                    pressed,
                },
            ));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_presses_and_releases_in_time() {
        let s = script("1.1:13-, 1.0:13+,junk,2:0+");
        assert!(matches!(s[0].1, PadEvent::Connected { standard: true, .. }));
        let steps: Vec<(f64, u16, bool)> = s[1..]
            .iter()
            .map(|(t, e)| match e {
                PadEvent::Button { index, pressed, .. } => (*t, *index, *pressed),
                _ => panic!(),
            })
            .collect();
        assert_eq!(
            steps,
            vec![(1.0, 13, true), (1.1, 13, false), (2.0, 0, true)]
        );
    }
}
