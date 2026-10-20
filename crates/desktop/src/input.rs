//! The window's input beyond winit's events (`docs/plans/desktop.md`,
//! 7.4): keys' times ([`crate::keyboard`]), the controllers
//! ([`crate::pads`]) driving every screen's menus (`ControlBindings`'
//! menu input, as on the web) or, while a screen binds controls, handed
//! to it as they are; and the options screen's view of them.

use std::rc::Rc;
use std::time::Duration;

use ddi_engine::ControlBindings;
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_ui::Ui;
use ddi_ui::event::{MenuEvent, UiEvent};
use ddi_ui::state::settings::Settings;
use winit::keyboard::PhysicalKey;
use winit::platform::scancode::PhysicalKeyExtScancode;

use crate::keyboard::{DesktopKeyboard, Stamps};
use crate::pads::{FakePad, PadEvent, PadOut, Pads};

#[derive(Default)]
pub struct Input {
    pub keyboard: DesktopKeyboard,
    pub stamps: Stamps,
    /// The OS's key times are read (macOS's monitor is installed).
    pub monitored: bool,
    pub pads: Option<Pads>,
    /// The bindings menus read (the stored settings', or the options' as
    /// edited).
    controls: Option<ControlBindings>,
    pub options: Option<Rc<crate::options::Shared>>,
    /// The fake controller's feed, while a script plays it.
    fake: Option<FakePad>,
    /// Pad edges that went to menus, and to a screen binding.
    pub menu_edges: u64,
    pub raw_edges: u64,
    /// The pads (by slot) read by the standard numbering.
    standard: Vec<u32>,
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

    fn controls(&mut self) -> &ControlBindings {
        if let Some(o) = &self.options {
            // The options' bindings as edited (a bind just done).
            self.controls = Some(o.model.borrow().settings.controls.clone());
        }
        self.controls
            .get_or_insert_with(|| Settings::load().controls)
    }

    /// The controllers' news: their edges, in order, for
    /// [`Input::route`] one at a time; the options told who is connected
    /// and what they hold.
    pub fn poll(&mut self, ui: &mut Ui) -> Vec<RawInput> {
        let Some(pads) = &mut self.pads else {
            return Vec::new();
        };
        let news = pads.take();
        if news.is_empty() {
            return Vec::new();
        }
        let connected = pads.connected();
        self.standard = connected
            .iter()
            .filter(|p| p.standard)
            .map(|p| p.index)
            .collect();
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

    /// A pad's edge as the UI's event, as the UI is now (each edge after
    /// the one before was handled: a press that opens the bind flow
    /// sends the next ones raw): raw while a screen binds, else a menu
    /// action.
    pub fn route(&mut self, ui: &Ui, e: RawInput) -> Option<UiEvent> {
        let DeviceId::Gamepad(id) = &e.device else {
            return None;
        };
        if ui.wants_raw() {
            self.raw_edges += 1;
            return Some(UiEvent::Raw(e));
        }
        let id = id.clone();
        let std = self.standard.contains(&e.slot);
        let action = self.controls().menu_input(&id, std, &e.control)?;
        self.menu_edges += 1;
        Some(UiEvent::Menu(MenuEvent {
            action: action.into(),
            down: e.pressed,
            source: e.slot,
            // Desktop has no rule about gestures.
            gesture: true,
            time: e.host_time.0 * 1000.0,
        }))
    }

    /// The settings the options changed since the last call.
    pub fn take_edited(&mut self) -> Option<Settings> {
        let s = self.options.as_ref()?.take_edited()?;
        self.controls = Some(s.controls.clone());
        Some(s)
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
