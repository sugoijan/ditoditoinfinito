//! The quick volume control: a button whose icon shows the music's level
//! and a vertical slider that drops under it.
//!
//! The slider shows while the mouse is over the button or the slider,
//! while the button has the focus (keys, pads), or after a tap on the
//! button (touch, which has no hover: it stays until a press elsewhere).
//! A press on the button while the slider shows mutes, or brings back the
//! level from before the mute; with a mouse the slider is already up, so
//! a click always mutes. Up and Down step the level while the button has
//! the focus, a wheel over it does, and a press or a drag along the
//! slider's track sets it.
//!
//! The level is the options' slider position (`volume_to_slider`), in
//! whole percent as Options shows it; the screen gets the gain.

use std::cell::Cell;

use ddi_actor::{Actor, Kind};

use crate::gesture::Gesture;
use crate::look::{self, Look, Spec, call};
use crate::nav::MenuAction;
use crate::screen::{Cx, Handled, Input};
use crate::state::settings::{DEFAULT_VOLUME, slider_to_volume, volume_to_slider};
use crate::widgets::{part, play, set_text};

/// How long the slider stays once the mouse has left it, seconds: room to
/// cross the gap between the button and the slider on a diagonal.
const LINGER: f64 = 0.3;

/// A step of Up, Down or a wheel's notch.
const STEP: f32 = 0.05;

/// The level changed: the gain, and whether it is kept (`false` while a
/// drag goes on: the preview follows, the settings wait for the release).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeChange {
    pub gain: f32,
    pub commit: bool,
}

pub struct Volume {
    pub name: String,
    pub centre: [f32; 2],
    /// The button's size; the slider's is the look's.
    pub size: [f32; 2],
    /// The slider position, `0..=1`, in hundredths.
    position: f32,
    /// The position a mute keeps for its unmute.
    unmuted: f32,
    open: bool,
    hovered: bool,
    focused: bool,
    /// Opened by a tap: stays until a press elsewhere.
    pinned: bool,
    dragging: bool,
    /// When the mouse left (closing after [`LINGER`]).
    left_at: Option<f64>,
    /// The track's bottom and length, in the slider's own units (its
    /// `Track` part as the look placed it).
    track: Cell<(f32, f32)>,
}

impl Volume {
    pub fn new(name: &str, gain: f32) -> Volume {
        Volume {
            name: name.into(),
            centre: [0.0; 2],
            size: [40.0, 40.0],
            position: quantized(volume_to_slider(gain)),
            unmuted: 0.0,
            open: false,
            hovered: false,
            focused: false,
            pinned: false,
            dragging: false,
            left_at: None,
            track: Cell::new((60.0, 120.0)),
        }
    }

    /// The slider's part.
    pub fn popup_name(&self) -> String {
        format!("{}.Popup", self.name)
    }

    fn button_name(&self) -> String {
        format!("{}.Button", self.name)
    }

    /// The slider position, `0..=1`.
    pub fn position(&self) -> f32 {
        self.position
    }

    pub fn gain(&self) -> f32 {
        slider_to_volume(self.position)
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The icon's level: 0 muted, then 1 to 3.
    pub fn level(&self) -> usize {
        match self.position {
            p if p <= 0.0 => 0,
            p if p < 0.34 => 1,
            p if p < 0.67 => 2,
            _ => 3,
        }
    }

    /// A frame still wanted (the slider waits to close).
    pub fn animating(&self) -> bool {
        self.left_at.is_some()
    }

    /// The level changed elsewhere (Options): shown again, a mute's
    /// memory forgotten where it no longer applies.
    pub fn set_gain(&mut self, gain: f32, root: &mut Actor) {
        let p = quantized(volume_to_slider(gain));
        if p != self.position {
            self.position = p;
            self.show(root);
        }
    }

    pub fn build(&self, look: &dyn Look, cx: &mut Cx) -> Actor {
        let spec = Spec::new(self.size, "");
        let mut button = look::component_or(look, "Volume", "Button", &spec);
        button.name = self.button_name();
        // The look's icon: an atlas of the four levels across.
        if let Some(source) = look.image("Volume.Icon")
            && let Some(icon) = part(&mut button, "Icon")
            && let Kind::Sprite(s) = &mut icon.kind
        {
            s.texture = cx.texture(source, s.size[0] * 4.0);
        }
        let popup_size = [self.size[0], 176.0];
        let mut popup = look::component(look, "Volume.Popup", &Spec::new(popup_size, ""));
        popup.name = self.popup_name();
        if let Some(t) = part(&mut popup, "Track") {
            let len = t.size()[1] * t.current().scale[1];
            self.track.set((t.current().pos[1] + len / 2.0, len));
        }
        // Hanging from the button's bottom edge, so the pointer crosses
        // no gap between them.
        call(
            &mut popup,
            "y",
            &[self.size[1] / 2.0 + popup_size[1] / 2.0 - 2.0],
        );
        popup.visible = self.open;
        let mut a = Actor::new(Kind::Frame(vec![button, popup]));
        a.name = self.name.clone();
        call(&mut a, "xy", &[self.centre[0], self.centre[1]]);
        self.show_in(&mut a);
        a
    }

    fn show(&self, root: &mut Actor) {
        if let Some(a) = crate::hit::find_mut(root, &self.name) {
            self.show_in(a);
        }
    }

    fn show_in(&self, a: &mut Actor) {
        let level = self.level();
        if let Some(icon) = part(a, "Icon")
            && let Kind::Sprite(s) = &mut icon.kind
        {
            let l = level as f32;
            s.custom = Some([l / 4.0, 0.0, (l + 1.0) / 4.0, 1.0]);
        }
        // A look without the icon: the percentage on the button.
        set_text(a, "Label", &format!("{:.0}", self.position * 100.0));
        let (bottom, len) = self.track.get();
        let p = self.position;
        if let Some(f) = part(a, "Fill") {
            call(f, "zoomy", &[p]);
        }
        if let Some(k) = part(a, "Knob") {
            call(k, "y", &[bottom - len * p]);
        }
        set_text(a, "Value", &format!("{:.0}%", p * 100.0));
    }

    fn set_open(&mut self, open: bool, root: &mut Actor, reduced: bool) {
        if open == self.open {
            return;
        }
        self.open = open;
        if !open {
            self.pinned = false;
            self.dragging = false;
        }
        let name = self.popup_name();
        if let Some(p) = crate::hit::find_mut(root, &name) {
            p.visible = open;
            if open {
                look::run(
                    p,
                    if reduced {
                        "stoptweening;diffusealpha,1"
                    } else {
                        "stoptweening;diffusealpha,0;linear,0.1;diffusealpha,1"
                    },
                );
            }
        }
    }

    fn settle(&mut self, root: &mut Actor, reduced: bool) {
        let keep = self.hovered || self.focused || self.pinned || self.dragging;
        if keep {
            self.left_at = None;
            self.set_open(true, root, reduced);
        }
    }

    /// Closes the slider once the mouse has been away long enough.
    pub fn update(&mut self, now: f64, root: &mut Actor, reduced: bool) {
        if let Some(t) = self.left_at
            && now - t >= LINGER
        {
            self.left_at = None;
            if !(self.hovered || self.focused || self.pinned || self.dragging) {
                self.set_open(false, root, reduced);
            }
        }
    }

    fn set(&mut self, p: f32, commit: bool, root: &mut Actor) -> Option<VolumeChange> {
        let p = quantized(p);
        if p == self.position && !commit {
            return None;
        }
        self.position = p;
        self.show(root);
        Some(VolumeChange {
            gain: self.gain(),
            commit,
        })
    }

    /// Mutes, or brings back the level the mute kept (the default level
    /// when there is none: muted since the game started).
    pub fn toggle_mute(&mut self, root: &mut Actor) -> Option<VolumeChange> {
        if self.position > 0.0 {
            self.unmuted = self.position;
            self.set(0.0, true, root)
        } else {
            let back = if self.unmuted > 0.0 {
                self.unmuted
            } else {
                volume_to_slider(DEFAULT_VOLUME)
            };
            self.set(back, true, root)
        }
    }

    /// The control's part of an input: whether it took it, and the level
    /// when it changed.
    pub fn input(
        &mut self,
        input: &Input,
        root: &mut Actor,
        cx: &mut Cx,
        reduced: bool,
    ) -> (Handled, Option<VolumeChange>) {
        let within = |t: &Option<crate::Target>| t.as_ref().is_some_and(|t| t.within(&self.name));
        match input {
            Input::Hover { target } => {
                let over = within(target);
                if over != self.hovered {
                    self.hovered = over;
                    if over {
                        self.settle(root, reduced);
                    } else {
                        self.left_at = Some(cx.time());
                    }
                }
                (Handled::No, None)
            }
            Input::Focus { from, to } => {
                let me = |n: &Option<String>| n.as_deref() == Some(self.name.as_str());
                if me(to) {
                    self.focused = true;
                    self.settle(root, reduced);
                } else if me(from) {
                    self.focused = false;
                    if !(self.hovered || self.pinned || self.dragging) {
                        self.set_open(false, root, reduced);
                    }
                }
                (Handled::No, None)
            }
            Input::Menu { action, .. } if self.focused => match action {
                MenuAction::Up => (Handled::Yes, self.set(self.position + STEP, true, root)),
                MenuAction::Down => (Handled::Yes, self.set(self.position - STEP, true, root)),
                MenuAction::Start => (Handled::Yes, self.toggle_mute(root)),
                _ => (Handled::No, None),
            },
            Input::Wheel { target, delta, .. } if within(target) => {
                let change = if delta[1] == 0.0 {
                    None
                } else {
                    let by = if delta[1] < 0.0 { STEP } else { -STEP };
                    self.set(self.position + by, true, root)
                };
                (Handled::Yes, change)
            }
            Input::Pointer {
                target,
                gesture,
                pos,
                ..
            } => {
                let Some(t) = target.as_ref().filter(|t| t.within(&self.name)) else {
                    // A press elsewhere closes what a tap opened.
                    if *gesture == Gesture::Press && self.pinned {
                        self.pinned = false;
                        if !(self.hovered || self.focused) {
                            self.set_open(false, root, reduced);
                        }
                    }
                    return (Handled::No, None);
                };
                if t.within(&self.popup_name()) {
                    return (Handled::Yes, self.track_input(*gesture, *pos, root));
                }
                let button = self.button_name();
                if let Some(b) = crate::hit::find_mut(root, &button) {
                    match gesture {
                        Gesture::Press => play(b, "Press"),
                        Gesture::Tap | Gesture::Cancel | Gesture::Release | Gesture::DragStart => {
                            play(b, "Release")
                        }
                        _ => {}
                    }
                }
                if *gesture != Gesture::Tap {
                    return (Handled::Yes, None);
                }
                // A tap: up if it was not (touch), else a mute.
                if self.open {
                    (Handled::Yes, self.toggle_mute(root))
                } else {
                    self.pinned = true;
                    self.settle(root, reduced);
                    (Handled::Yes, None)
                }
            }
            _ => (Handled::No, None),
        }
    }

    /// A press or a drag along the slider.
    fn track_input(
        &mut self,
        gesture: Gesture,
        pos: [f32; 2],
        root: &mut Actor,
    ) -> Option<VolumeChange> {
        let (bottom, len) = self.track.get();
        let name = self.popup_name();
        let y =
            crate::hit::local_point(root, &crate::hit::IDENTITY, &name, pos).map_or(0.0, |p| p[1]);
        let p = (bottom - y) / len.max(1.0);
        match gesture {
            Gesture::Press if y >= bottom - len - 16.0 && y <= bottom + 16.0 => {
                self.dragging = true;
                self.set(p, false, root)
            }
            Gesture::DragStart | Gesture::Drag { .. } if self.dragging => self.set(p, false, root),
            Gesture::Tap | Gesture::DragEnd { .. } | Gesture::Flick { .. } | Gesture::Release
                if self.dragging =>
            {
                self.dragging = false;
                self.set(p, true, root)
            }
            Gesture::Cancel => {
                self.dragging = false;
                None
            }
            _ => None,
        }
    }
}

/// In hundredths, as Options' rows step it.
fn quantized(p: f32) -> f32 {
    (p.clamp(0.0, 1.0) * 100.0).round() / 100.0
}
