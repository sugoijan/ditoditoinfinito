//! Which pointer holds what on a play, for both shells (round PU,
//! 2026-11-24; `docs/plans/touch-controls.md`, "One pointer handling"):
//! fingers, the mouse and a pen landing on a lane's touch control, the
//! quit button, a stage's attack policy label, the music's volume
//! ([`crate::play_volume`]) or a watched stage's fields and skip plate.
//!
//! The rules are the web's, which were the older and the stricter copy
//! (the maintainer, 2026-11-24: "The web input behavior makes most sense
//! and unifying them would be best"):
//!
//! - A press is tried on what the play drew ([`Places`]) in one order:
//!   the volume, the quit button, the policy label, the watched stage's
//!   skip plate and fields, then, for a finger or a pen alone, the lanes.
//!   Only a primary press (the mouse's main button, a finger, a pen's
//!   tip) reaches the volume, the buttons and the fields; a press that
//!   misses the volume while its slider is open closes it and goes on.
//! - Every pointer on a target is counted: the target is pressed when
//!   the first lands and released when the last lifts (the quit button
//!   and the policy label too, so a second finger on the quit button is
//!   no second tap). A pointer holds what it went down on until it lifts,
//!   whatever it slides over, but for a lane, which slides to another
//!   only where the shell's lane finder says (the touch controls' dead
//!   band, `Controls::lane_at`). A press on a watched stage's field is a
//!   press alone, nothing held.
//! - A pointer already holding something that goes down again (an id
//!   reused before its lift came) is taken and does nothing.
//! - Every pointer on the volume hands it its edges (a second finger's
//!   too, as on the web); a drag cut off ([`PlayPointers::release_all`])
//!   ends where it last was.
//! - [`PlayPointers::release_all`] lets go of everything at once: the
//!   shells call it when the page or the window loses the focus or is
//!   hidden, where no lift may come.
//!
//! What stays each shell's own: where its event is (the web's CSS pixels
//! of the canvas, desktop's logical pixels, the units the play's places
//! are in), the lane under a finger (the touch controls' plan, or the
//! web's legacy columns), the time stamp of the edges ([`Sink::at`]: the
//! web's event time, desktop's clock as the event is handled, exactly as
//! before), and what it does around a press (the web's pointer capture,
//! desktop's screens under a framed play).

use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_session::play::{
    TOUCH_CANCEL_CONTROL, TOUCH_POLICY_CONTROL, TOUCH_SKIP_CONTROL, TOUCH_WATCH_ALL,
    TOUCH_WATCH_CONTROL,
};

use crate::play_volume::{Edge as VolumeEdge, Phase as VolumePhase, VolumeAt};

/// What a pointer holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    /// A lane (its touch control, or the web's legacy column).
    Lane(usize),
    /// The quit button: the play's `cancel` control.
    Quit,
    /// A stage's attack policy label: the play's `policy` control.
    Policy,
    /// The music's volume (no control of the play's).
    Volume,
}

impl Hold {
    /// The play's control it presses, none for the volume.
    fn control(self) -> Option<String> {
        match self {
            Hold::Lane(l) => Some(format!("lane:{l}")),
            Hold::Quit => Some(TOUCH_CANCEL_CONTROL.to_string()),
            Hold::Policy => Some(TOUCH_POLICY_CONTROL.to_string()),
            Hold::Volume => None,
        }
    }
}

/// Where the play drew what takes a press this frame
/// (`ddi_shell::play_view::Drawn`), in the shell's units.
#[derive(Clone, Copy, Debug, Default)]
pub struct Places<'a> {
    pub volume: Option<VolumeAt>,
    /// The quit button: middle and radius.
    pub quit: Option<[f32; 3]>,
    /// The attack policy's label: left, top, right, bottom.
    pub policy: Option<[f32; 4]>,
    /// A watched stage's fields, each with the seat it follows (none:
    /// back to every field), and the skip's plate.
    pub watch: &'a [(Option<usize>, [f32; 4])],
    pub skip: Option<[f32; 4]>,
}

impl Places<'_> {
    fn on_quit(&self, [x, y]: [f64; 2]) -> bool {
        self.quit.is_some_and(|[qx, qy, r]| {
            let dx = x - f64::from(qx);
            let dy = y - f64::from(qy);
            dx * dx + dy * dy <= f64::from(r * r)
        })
    }

    fn on_policy(&self, pos: [f64; 2]) -> bool {
        self.policy.is_some_and(|r| inside(r, pos))
    }

    /// The control a press on a watched stage sends: its skip first
    /// (it lies over a field), then a field.
    fn watch_control(&self, pos: [f64; 2]) -> Option<String> {
        if self.skip.is_some_and(|r| inside(r, pos)) {
            return Some(TOUCH_SKIP_CONTROL.to_string());
        }
        self.watch
            .iter()
            .find(|(_, r)| inside(*r, pos))
            .map(|(seat, _)| match seat {
                Some(seat) => format!("{TOUCH_WATCH_CONTROL}{seat}"),
                None => TOUCH_WATCH_ALL.to_string(),
            })
    }
}

fn inside([l, t, r, b]: [f32; 4], [x, y]: [f64; 2]) -> bool {
    (f64::from(l)..=f64::from(r)).contains(&x) && (f64::from(t)..=f64::from(b)).contains(&y)
}

fn narrow([x, y]: [f64; 2]) -> [f32; 2] {
    [x as f32, y as f32]
}

/// Where a pointer's edges go: the play's inputs (the touch device's
/// controls, stamped [`Sink::at`]) and the play view's volume
/// (`PlayView::volume_edges`, never the play's inputs or log).
pub struct Sink<'a> {
    pub inputs: &'a mut Vec<RawInput>,
    pub volume: &'a mut Vec<VolumeEdge>,
    /// The edges' time stamp, the shell's own.
    pub at: HostTime,
}

impl Sink<'_> {
    fn input(&mut self, control: String, pressed: bool) {
        self.inputs.push(RawInput {
            device: DeviceId::Touch,
            control,
            pressed,
            host_time: self.at,
            slot: 0,
        });
    }

    fn volume(&mut self, phase: VolumePhase, pos: [f32; 2]) {
        self.volume.push(VolumeEdge { phase, pos });
    }
}

/// A pointer going down, as the shell read its event.
#[derive(Clone, Copy, Debug)]
pub struct Press {
    /// Where (the shell's units, as [`Places`]).
    pub pos: [f64; 2],
    /// The primary press (the mouse's main button, a finger, a pen's
    /// tip): only it reaches the volume, the buttons and the fields.
    pub primary: bool,
    /// A finger or a pen, which may press a lane (the mouse never does).
    pub finger: bool,
}

/// What became of a pointer going down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Landed {
    /// The play took it (the web prevents its default; desktop gives it
    /// to no screen).
    pub taken: bool,
    /// It holds a target from now on, until it lifts (the web captures
    /// it).
    pub holds: bool,
    /// A finger reached the lanes, whether one was under it or not:
    /// touch is the input source (the touch controls show).
    pub touch: bool,
}

/// The pointers holding something on a play, one a play (made anew as it
/// starts). `I` is the shell's pointer id (the web's `pointerId`,
/// desktop's: the mouse 0, a finger its touch id plus one).
#[derive(Clone, Debug)]
pub struct PlayPointers<I = u64> {
    /// Each pointer down on a target, with what it holds and, for the
    /// volume, where it last was.
    holds: Vec<(I, Hold, [f32; 2])>,
}

impl<I> Default for PlayPointers<I> {
    fn default() -> Self {
        PlayPointers { holds: Vec::new() }
    }
}

impl<I: Copy + PartialEq> PlayPointers<I> {
    /// How many pointers hold `hold` now.
    pub fn held(&self, hold: Hold) -> usize {
        self.holds.iter().filter(|(_, h, _)| *h == hold).count()
    }

    /// What pointer `id` holds, if anything.
    pub fn holding(&self, id: I) -> Option<Hold> {
        self.find(id).map(|i| self.holds[i].1)
    }

    /// Whether no pointer holds anything.
    pub fn is_empty(&self) -> bool {
        self.holds.is_empty()
    }

    /// The pointers holding something (the web lets go of their capture
    /// when the play ends).
    pub fn ids(&self) -> impl Iterator<Item = I> + '_ {
        self.holds.iter().map(|(id, _, _)| *id)
    }

    fn find(&self, id: I) -> Option<usize> {
        self.holds.iter().position(|(p, _, _)| *p == id)
    }

    /// Pointer `id` going down. `lane` is the lane under it, asked only
    /// for a finger that lands on nothing else.
    pub fn down(
        &mut self,
        id: I,
        press: Press,
        places: &Places,
        lane: impl FnOnce() -> Option<usize>,
        sink: &mut Sink,
    ) -> Landed {
        let pos = press.pos;
        // The music's volume first: its press is its own; one elsewhere
        // closes its slider and goes on.
        if press.primary
            && let Some(at) = places.volume
        {
            if at.holds(narrow(pos)) {
                if self.find(id).is_some() {
                    return Landed {
                        taken: true,
                        ..Landed::default()
                    };
                }
                self.holds.push((id, Hold::Volume, narrow(pos)));
                sink.volume(VolumePhase::Down, narrow(pos));
                return Landed {
                    taken: true,
                    holds: true,
                    touch: false,
                };
            }
            if at.plate.is_some() {
                sink.volume(VolumePhase::Down, narrow(pos));
            }
        }
        let on_quit = places.on_quit(pos);
        let on_policy = !on_quit && places.on_policy(pos);
        // A watched stage: a press on a field follows it, one on the
        // skip's plate skips (a press, with nothing to hold).
        if !on_quit
            && !on_policy
            && press.primary
            && let Some(control) = places.watch_control(pos)
        {
            sink.input(control, true);
            return Landed {
                taken: true,
                ..Landed::default()
            };
        }
        let mut landed = Landed::default();
        let hold = if on_quit || on_policy {
            if !press.primary {
                return landed;
            }
            if on_quit { Hold::Quit } else { Hold::Policy }
        } else if !press.finger {
            // The mouse presses no lane.
            return landed;
        } else {
            landed.touch = true;
            match lane() {
                Some(lane) => Hold::Lane(lane),
                None => return landed,
            }
        };
        landed.taken = true;
        if self.find(id).is_some() {
            return landed;
        }
        self.holds.push((id, hold, narrow(pos)));
        self.press(hold, sink);
        landed.holds = true;
        landed
    }

    /// Pointer `id` moved to `pos`: the volume's drag goes on; a lane's
    /// finger slides to the lane `lane` gives for the lane it holds (none
    /// keeps it). Whether the pointer holds something of the play's.
    pub fn moved(
        &mut self,
        id: I,
        pos: [f64; 2],
        lane: impl FnOnce(usize) -> Option<usize>,
        sink: &mut Sink,
    ) -> bool {
        let Some(i) = self.find(id) else {
            return false;
        };
        match self.holds[i].1 {
            Hold::Volume => {
                self.holds[i].2 = narrow(pos);
                sink.volume(VolumePhase::Move, narrow(pos));
            }
            Hold::Lane(old) => {
                if let Some(new) = lane(old)
                    && new != old
                {
                    self.holds[i].1 = Hold::Lane(new);
                    self.release(Hold::Lane(old), sink);
                    self.press(Hold::Lane(new), sink);
                }
            }
            Hold::Quit | Hold::Policy => {}
        }
        true
    }

    /// Pointer `id` lifted at `pos` (`cancel`: its system took it away,
    /// or the shell lost it): what it held is let go once no other
    /// pointer holds it. Whether it held something of the play's.
    pub fn up(&mut self, id: I, pos: [f64; 2], cancel: bool, sink: &mut Sink) -> bool {
        let Some(i) = self.find(id) else {
            return false;
        };
        let (_, hold, _) = self.holds.swap_remove(i);
        if hold == Hold::Volume {
            let phase = if cancel {
                VolumePhase::Cancel
            } else {
                VolumePhase::Up
            };
            sink.volume(phase, narrow(pos));
        } else {
            self.release(hold, sink);
        }
        true
    }

    /// Every pointer let go at once (the focus lost, the page or window
    /// hidden: no lift will come): each target released once, a drag on
    /// the volume ended where it last was (it keeps the level it
    /// reached).
    pub fn release_all(&mut self, sink: &mut Sink) {
        let holds = std::mem::take(&mut self.holds);
        for (i, (_, hold, last)) in holds.iter().enumerate() {
            if *hold == Hold::Volume {
                sink.volume(VolumePhase::Cancel, *last);
            } else if !holds[i + 1..].iter().any(|(_, h, _)| h == hold)
                && let Some(control) = hold.control()
            {
                // The last of its pointers.
                sink.input(control, false);
            }
        }
    }

    /// One more pointer on `hold` (already counted): the first presses it.
    fn press(&mut self, hold: Hold, sink: &mut Sink) {
        if self.held(hold) == 1
            && let Some(control) = hold.control()
        {
            sink.input(control, true);
        }
    }

    /// One pointer fewer on `hold` (already uncounted): the last releases
    /// it.
    fn release(&mut self, hold: Hold, sink: &mut Sink) {
        if self.held(hold) == 0
            && let Some(control) = hold.control()
        {
            sink.input(control, false);
        }
    }
}

#[cfg(test)]
mod tests;
