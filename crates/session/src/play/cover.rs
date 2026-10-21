//! The lane covers' controls during a play (`docs/plans/lane-covers.md`):
//! DDR moves its LANE COVER with the cabinet's Up and Down buttons and
//! hides or shows it with Start ([`ddi_engine::cover`]). Here Page Up and
//! Page Down move it; Start (a controller's bound Start, Enter on a
//! keyboard) held with a press of an up or down arrow lane moves it too,
//! and Start tapped alone hides it or shows it again. "Up" is up the
//! screen, whichever end the receptors are at. A control held moves it on
//! after a moment, as a menu's key repeats.
//!
//! The lane presses made while Start is held still reach their lanes and
//! are judged like any other: only drawing changes, never judging. A
//! replay draws its log's covers and ignores these controls.

use ddi_chart::{Glyph, Layout};
use ddi_engine::cover::CoverMode;
use ddi_engine::input::Bindings;
use ddi_engine::player::Player;
use ddi_platform::{DeviceId, HostTime, RawInput};

/// Keys that move the cover up and down the screen.
pub const COVER_UP_KEY: &str = "PageUp";
pub const COVER_DOWN_KEY: &str = "PageDown";
/// Keys that act as Start on a keyboard.
pub const START_KEYS: [&str; 2] = ["Enter", "NumpadEnter"];

/// Seconds a moving control is held before it moves the cover on, and
/// then between steps (25% of the lane a second).
const REPEAT_DELAY: f64 = 0.35;
const REPEAT_EVERY: f64 = 0.04;

/// What a play's edges do to its lane covers.
#[derive(Debug, Default)]
pub struct CoverControl {
    /// The receptors are at the bottom: up the screen is away from them.
    reverse: bool,
    /// The mode a tap of Start hid, shown again by the next.
    hidden_mode: Option<CoverMode>,
    /// Start held: its device, slot and control, and whether the cover
    /// moved meanwhile (then its release hides nothing).
    start: Option<(DeviceId, u32, String, bool)>,
    /// The control moving the cover: its device, slot and control, the
    /// way it moves (+1: up the screen) and when it steps next.
    held: Option<(DeviceId, u32, String, i32, f64)>,
}

impl CoverControl {
    pub fn new(reverse: bool) -> CoverControl {
        CoverControl {
            reverse,
            ..CoverControl::default()
        }
    }

    /// Takes edge `raw`: whether it was the covers' alone (a key bound to
    /// no lane), so that it goes no further. `starts` are the pads' Start
    /// controls, `(Gamepad.id, control)`.
    pub fn feed(
        &mut self,
        raw: &RawInput,
        bindings: &Bindings,
        layout: &Layout,
        starts: &[(String, String)],
        player: &mut Player,
    ) -> bool {
        let lane = bindings.lane_of_slot(&raw.device, raw.slot, &raw.control);
        let same = |d: &DeviceId, slot: u32, c: &str| {
            *d == raw.device && slot == raw.slot && c == raw.control
        };
        // A release ends what its press began.
        if !raw.pressed {
            if self
                .held
                .as_ref()
                .is_some_and(|(d, s, c, ..)| same(d, *s, c))
            {
                self.held = None;
            }
            if let Some((d, s, c, moved)) = &self.start
                && same(d, *s, c)
            {
                if !moved {
                    self.toggle(player, raw.host_time);
                }
                self.start = None;
                return lane.is_none();
            }
            return lane.is_none() && self.is_cover_key(raw);
        }
        let is_start = match &raw.device {
            DeviceId::Keyboard => lane.is_none() && START_KEYS.contains(&raw.control.as_str()),
            DeviceId::Gamepad(id) => starts.iter().any(|(p, c)| p == id && *c == raw.control),
            _ => false,
        };
        if is_start {
            self.start = Some((raw.device.clone(), raw.slot, raw.control.clone(), false));
            return lane.is_none();
        }
        let up = match &raw.device {
            DeviceId::Keyboard if lane.is_none() && raw.control == COVER_UP_KEY => Some(1),
            DeviceId::Keyboard if lane.is_none() && raw.control == COVER_DOWN_KEY => Some(-1),
            // An up or down arrow's lane while Start is held on the same
            // device.
            _ => self
                .start
                .as_ref()
                .filter(|(d, ..)| *d == raw.device)
                .and(lane)
                .and_then(|l| layout.lanes.get(usize::from(l)))
                .and_then(|l| screen_way(&l.glyph)),
        };
        let Some(up) = up else {
            return false;
        };
        if let Some(start) = &mut self.start {
            start.3 = true;
        }
        let t = raw.host_time.0;
        self.held = Some((
            raw.device.clone(),
            raw.slot,
            raw.control.clone(),
            up,
            t + REPEAT_DELAY,
        ));
        self.step(up, player, raw.host_time);
        lane.is_none()
    }

    /// Steps a held control has made by host time `now`.
    pub fn tick(&mut self, now: HostTime, player: &mut Player) {
        let Some((.., up, next)) = &mut self.held else {
            return;
        };
        let up = *up;
        let mut steps = 0;
        while *next <= now.0 && steps < 100 {
            *next += REPEAT_EVERY;
            steps += 1;
        }
        if steps > 0 {
            self.step(up * steps, player, now);
        }
    }

    /// Lets go of everything held (the play ended or lost its inputs).
    pub fn release(&mut self) {
        self.start = None;
        self.held = None;
    }

    fn is_cover_key(&self, raw: &RawInput) -> bool {
        raw.device == DeviceId::Keyboard
            && (raw.control == COVER_UP_KEY
                || raw.control == COVER_DOWN_KEY
                || START_KEYS.contains(&raw.control.as_str()))
    }

    /// Moves the cover `up` percent up the screen.
    fn step(&mut self, up: i32, player: &mut Player, at: HostTime) {
        let towards_receptors = if self.reverse { -up } else { up };
        let cover = player.cover().moved(towards_receptors);
        player.set_cover(cover, at);
    }

    /// Hides the cover, or shows the one hidden again.
    fn toggle(&mut self, player: &mut Player, at: HostTime) {
        let mut cover = player.cover();
        if cover.is_on() {
            self.hidden_mode = Some(cover.mode);
            cover.mode = CoverMode::Off;
        } else if let Some(mode) = self.hidden_mode.take() {
            cover.mode = mode;
        } else {
            return;
        }
        player.set_cover(cover, at);
    }
}

/// +1 for a lane whose arrow points up the screen, −1 down, else none.
fn screen_way(glyph: &Glyph) -> Option<i32> {
    match glyph {
        Glyph::Arrow { rotation_deg } => match (rotation_deg.rem_euclid(360.0)).round() as i32 {
            90 => Some(1),
            270 => Some(-1),
            _ => None,
        },
        _ => None,
    }
}
