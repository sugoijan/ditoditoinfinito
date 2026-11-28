//! The window as it was left (round WN, `docs/plans/desktop.md`, "The
//! window as it was left"): its size, its place, whether it was maximized
//! or fullscreen, kept in a desktop-only value ([`KEY`], never the
//! settings the web shares) and given back when the window is made.
//!
//! Everything here is plain data, decided without a window: the shell
//! (`crate::shell`) turns what winit says of the window and the displays
//! into [`Observed`] and [`Display`]s, and [`place`]'s [`Placement`] into
//! the window's attributes. The saved state is checked against the
//! displays present at the start, so a window never comes back
//! unusable: off every display, larger than its display, too small, on a
//! display that is gone, or from a value that does not read.
//!
//! Sizes and places are kept in logical units (points on macOS), each
//! place from the top-left of the display the window was on, with that
//! display named and the scale it had: a display's resolution or scale
//! changed keeps the window's size as it looks (its text the same size),
//! and the place is found again on the display by its name wherever the
//! displays are arranged now. Physical pixels would make a window saved on
//! a Retina display twice as large on a plain one, and a place in the
//! desktop's coordinates means nothing once a display is unplugged.

use serde::{Deserialize, Serialize};

/// The desktop's own value in `storage.json` (`crate::store`).
pub const KEY: &str = "ddi.desktop.window.v1";

/// The window's inner size when nothing is saved, logical, as before
/// round WN; smaller on a display that cannot hold it ([`sensible`]).
pub const DEFAULT_SIZE: [f64; 2] = [1280.0, 720.0];

/// The smallest inner size given back, logical: anything smaller is
/// taken for a mistake (the screens' compact design is made for a phone
/// of 360 × 640, so 320 × 240 is still a window one can use).
pub const MIN_SIZE: [f64; 2] = [320.0, 240.0];

/// A sensible window fills at most this share of its display each way.
pub const FILL: f64 = 0.9;

/// How much of the window's top edge must lie on its display, logical,
/// so its title bar can be taken and moved.
pub const GRAB: [f64; 2] = [120.0, 40.0];

/// How far a window's outer frame may stand over its display's edges
/// and still count as on it: Windows counts its invisible resize borders
/// (about 7 pixels) in the frame, so a window against the left edge is at
/// −7.
pub const OVERHANG: f64 = 16.0;

/// What a size may be over its display's and still fit, logical: a
/// physical size over a fractional scale rounds.
pub const SLACK: f64 = 1.0;

/// How long the window is left as it is before what changed is written
/// (s): a drag or a resize changes it every frame.
pub const WRITE_AFTER: f64 = 1.0;

/// A display as the window library tells it: physical pixels, in the
/// system's coordinates ([`Coordinates`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub name: Option<String>,
    pub position: [i32; 2],
    pub size: [u32; 2],
    pub scale: f64,
}

impl Display {
    /// Its size in logical units.
    pub fn logical(&self) -> [f64; 2] {
        let s = self.scale_or_one();
        [f64::from(self.size[0]) / s, f64::from(self.size[1]) / s]
    }

    fn scale_or_one(&self) -> f64 {
        if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        }
    }

    /// What is saved of it.
    pub fn key(&self) -> DisplayKey {
        DisplayKey {
            name: self.name.clone(),
            position: self.position,
            size: self.size,
            scale: self.scale,
        }
    }
}

/// A display as saved: enough to find it again ([`find`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayKey {
    pub name: Option<String>,
    pub position: [i32; 2],
    pub size: [u32; 2],
    pub scale: f64,
}

/// The window's normal (neither maximized nor fullscreen) frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// The inner size, logical.
    pub size: [f64; 2],
    /// The outer frame's top-left from its display's top-left, logical;
    /// none where the system tells no window's place (Wayland).
    pub offset: Option<[f64; 2]>,
    /// The display the window was on, none where the system said none.
    pub display: Option<DisplayKey>,
    /// The window's scale when it was saved (the display's, kept for the
    /// record: sizes are logical, so a changed scale keeps them).
    pub scale: f64,
}

/// What is kept ([`KEY`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub frame: Frame,
    pub maximized: bool,
    pub fullscreen: bool,
    /// The display it was fullscreen on.
    #[serde(default)]
    pub fullscreen_on: Option<DisplayKey>,
}

/// What the shell saw of the window (winit's units: physical pixels).
#[derive(Clone, Debug, PartialEq)]
pub struct Observed {
    pub inner: [u32; 2],
    /// The outer frame's top-left; none where the system tells none.
    pub outer: Option<[i32; 2]>,
    pub scale: f64,
    /// The display the window is on.
    pub display: Option<Display>,
    pub maximized: bool,
    /// Minimized, where the system says (Windows then reports a place of
    /// −32000 and a size of nothing): nothing is learned from it.
    pub minimized: bool,
    pub fullscreen: bool,
}

impl Saved {
    /// What `seen` says the window is now, over what was kept (`self`):
    /// the normal frame is learned only while the window is neither
    /// maximized nor fullscreen (maximized, the frame is the display's;
    /// left again, the window goes back to the frame before). `None`
    /// where nothing can be learned (minimized, no size).
    pub fn observe(kept: Option<&Saved>, seen: &Observed) -> Option<Saved> {
        if seen.minimized || seen.inner[0] == 0 || seen.inner[1] == 0 {
            return kept.cloned();
        }
        let scale = if seen.scale.is_finite() && seen.scale > 0.0 {
            seen.scale
        } else {
            1.0
        };
        let normal = !seen.maximized && !seen.fullscreen;
        let frame = match kept {
            Some(k) if !normal => k.frame.clone(),
            _ => {
                let size = [
                    f64::from(seen.inner[0]) / scale,
                    f64::from(seen.inner[1]) / scale,
                ];
                // From the display's top-left: on the window's display its
                // scale is the window's, in either system's coordinates.
                let offset = match (seen.outer, &seen.display) {
                    (Some(o), Some(d)) => Some([
                        f64::from(o[0] - d.position[0]) / d.scale_or_one(),
                        f64::from(o[1] - d.position[1]) / d.scale_or_one(),
                    ]),
                    _ => None,
                };
                Frame {
                    size,
                    offset,
                    display: seen.display.as_ref().map(Display::key),
                    scale,
                }
            }
        };
        Some(Saved {
            frame,
            maximized: seen.maximized && !seen.fullscreen,
            fullscreen: seen.fullscreen,
            fullscreen_on: if seen.fullscreen {
                seen.display.as_ref().map(Display::key)
            } else {
                None
            },
        })
    }

    /// What is kept of a window made by `p` on `displays` (before the
    /// window says anything of itself): the normal frame it was made
    /// with, so a window made fullscreen and left so keeps the frame it
    /// will go back to.
    pub fn made(p: &Placement, displays: &[Display]) -> Saved {
        let d = p.display.and_then(|i| displays.get(i));
        Saved {
            frame: Frame {
                size: p.size,
                offset: p.offset,
                display: d.map(Display::key),
                scale: d.map_or(1.0, |d| d.scale),
            },
            maximized: p.maximized,
            fullscreen: p.fullscreen.is_some(),
            fullscreen_on: p
                .fullscreen
                .flatten()
                .and_then(|i| displays.get(i))
                .map(Display::key),
        }
    }

    /// Whether every number in it is one a window could have; a value
    /// that does not pass is not given back at all.
    fn readable(&self) -> bool {
        let fine = |v: f64, limit: f64| v.is_finite() && v.abs() <= limit;
        let f = &self.frame;
        f.size.iter().all(|&v| fine(v, 1e6) && v > 0.0)
            && f.offset.is_none_or(|o| o.iter().all(|&v| fine(v, 1e6)))
            && fine(f.scale, 1e3)
    }
}

/// How the system's window places are counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coordinates {
    /// macOS: places in points, each display's pixels its points times
    /// its scale (winit's `MonitorHandle::position` is so); a window is
    /// placed in points (`LogicalPosition`).
    Points,
    /// Windows and X11: places in the desktop's pixels; a window is
    /// placed in them (`PhysicalPosition`).
    Pixels,
}

impl Coordinates {
    /// This system's.
    pub const fn here() -> Coordinates {
        if cfg!(target_os = "macos") {
            Coordinates::Points
        } else {
            Coordinates::Pixels
        }
    }
}

/// Where the window's outer top-left goes, in the units the system
/// takes ([`Coordinates`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum At {
    Logical([f64; 2]),
    Physical([i32; 2]),
}

/// What [`place`] decided, and why: said on stderr when what was saved
/// is not given back as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Nothing saved (a first start, a fresh data folder).
    Fresh,
    /// As it was left.
    Restored,
    /// What was saved does not read as a window (a value edited or
    /// broken).
    Unreadable,
    /// Its display is not there now.
    DisplayGone,
    /// Smaller than [`MIN_SIZE`].
    TooSmall,
    /// Larger than its display.
    TooLarge,
    /// Its title bar would not be on its display.
    OffDisplay,
}

impl Reason {
    /// What a person is told when the window does not come back as it
    /// was left (logs are English).
    pub fn line(self) -> Option<&'static str> {
        Some(match self {
            Reason::Fresh | Reason::Restored => return None,
            Reason::Unreadable => "the window's saved state does not read",
            Reason::DisplayGone => "the display the window was on is not there",
            Reason::TooSmall => "the window was saved too small to use",
            Reason::TooLarge => "the window was saved larger than its display",
            Reason::OffDisplay => "the window was saved off its display",
        })
    }
}

/// How the window is made.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// The inner size, logical: the normal frame's (a maximized or
    /// fullscreen window's when it is left again).
    pub size: [f64; 2],
    /// Where its outer top-left goes; none: where the system puts a new
    /// window (centered on the main display on macOS).
    pub at: Option<At>,
    /// The display of the normal frame (its index in the list given) and
    /// the frame's offset from its top-left, logical, as [`At`] was made
    /// from: what [`Saved::made`] keeps of it.
    pub display: Option<usize>,
    pub offset: Option<[f64; 2]>,
    pub maximized: bool,
    /// Fullscreen: on the display of this index in the list given, or,
    /// with none, the window's own (no displays were told).
    pub fullscreen: Option<Option<usize>>,
    /// Why the normal frame is what it is.
    pub reason: Reason,
    /// Why the fullscreen display is what it is (`DisplayGone`: on the
    /// primary display instead).
    pub fullscreen_reason: Option<Reason>,
}

/// The display `key` names among `displays`: the one with its name and
/// place, else its name and size, else its name alone (a resolution
/// changed, the displays arranged otherwise); one saved with no name is
/// found only as a display with none at its place and of its size.
pub fn find(key: &DisplayKey, displays: &[Display]) -> Option<usize> {
    let same_name = |d: &Display| key.name.is_some() && d.name == key.name;
    let rank = |d: &Display| {
        if same_name(d) {
            Some(match (d.position == key.position, d.size == key.size) {
                (true, true) => 0,
                (true, false) => 1,
                (false, true) => 2,
                (false, false) => 3,
            })
        } else if key.name.is_none()
            && d.name.is_none()
            && d.position == key.position
            && d.size == key.size
        {
            Some(0)
        } else {
            None
        }
    };
    displays
        .iter()
        .enumerate()
        .filter_map(|(i, d)| rank(d).map(|r| (r, i)))
        .min()
        .map(|(_, i)| i)
}

/// A new window's inner size on `display`: [`DEFAULT_SIZE`], smaller
/// (its shape kept) where that would fill more than [`FILL`] of it.
pub fn sensible(display: Option<&Display>) -> [f64; 2] {
    let Some(d) = display else {
        return DEFAULT_SIZE;
    };
    let room = d.logical();
    let fit = (room[0] * FILL / DEFAULT_SIZE[0])
        .min(room[1] * FILL / DEFAULT_SIZE[1])
        .min(1.0);
    [
        (DEFAULT_SIZE[0] * fit).round(),
        (DEFAULT_SIZE[1] * fit).round(),
    ]
}

/// The outer top-left that puts a window at `offset` (logical) from
/// `display`'s top-left.
fn at_on(display: &Display, offset: [f64; 2], coords: Coordinates) -> At {
    let s = display.scale_or_one();
    match coords {
        Coordinates::Points => At::Logical([
            f64::from(display.position[0]) / s + offset[0],
            f64::from(display.position[1]) / s + offset[1],
        ]),
        Coordinates::Pixels => At::Physical([
            display.position[0] + (offset[0] * s).round() as i32,
            display.position[1] + (offset[1] * s).round() as i32,
        ]),
    }
}

/// The offset that centers a window of `size` on `display` (logical).
fn centered(display: &Display, size: [f64; 2]) -> [f64; 2] {
    let room = display.logical();
    [
        ((room[0] - size[0]) / 2.0).max(0.0).round(),
        ((room[1] - size[1]) / 2.0).max(0.0).round(),
    ]
}

/// Whether a frame of `size` at `offset` (both logical) on a display of
/// `room` (logical) can be used: its title bar on the display.
fn on_display(size: [f64; 2], offset: [f64; 2], room: [f64; 2]) -> bool {
    let top = offset[1] >= -OVERHANG && offset[1] <= room[1] - GRAB[1];
    let across = (offset[0] + size[0]).min(room[0]) - offset[0].max(0.0);
    top && across >= GRAB[0].min(size[0])
}

/// How the window is made from what was saved, checked against the
/// displays present now (`primary` the main one, where the system says;
/// else the first): every way a saved window could come back unusable
/// gives a new window of [`sensible`] size centered on the primary
/// display instead, and a saved fullscreen on a display that is gone is
/// fullscreen on the primary one.
pub fn place(
    saved: Option<&Saved>,
    displays: &[Display],
    primary: Option<usize>,
    coords: Coordinates,
) -> Placement {
    place_kept(saved.map(Ok), displays, primary, coords)
}

/// What the store holds under [`KEY`] (its text), read: nothing, a saved
/// window, or a value that does not read as one (`Err`, its error).
pub fn read(text: Option<&str>) -> Option<Result<Saved, String>> {
    text.map(|t| serde_json::from_str(t).map_err(|e| e.to_string()))
}

/// [`place`] from what [`read`] read: a value that does not read as a
/// window gives a new one ([`Reason::Unreadable`]), as numbers no window
/// has do.
pub fn place_read(
    read: Option<&Result<Saved, String>>,
    displays: &[Display],
    primary: Option<usize>,
    coords: Coordinates,
) -> Placement {
    let kept = read.map(|r| r.as_ref().map_err(|_| ()));
    place_kept(kept, displays, primary, coords)
}

fn place_kept(
    saved: Option<Result<&Saved, ()>>,
    displays: &[Display],
    primary: Option<usize>,
    coords: Coordinates,
) -> Placement {
    let primary = primary
        .filter(|&i| i < displays.len())
        .or((!displays.is_empty()).then_some(0));
    let main = primary.map(|i| &displays[i]);
    let frame = |size: [f64; 2], on: Option<usize>, offset: Option<[f64; 2]>, reason| Placement {
        size,
        at: on.zip(offset).map(|(i, o)| at_on(&displays[i], o, coords)),
        display: on,
        offset: on.and(offset),
        maximized: false,
        fullscreen: None,
        reason,
        fullscreen_reason: None,
    };
    let fresh = |reason: Reason| {
        let size = sensible(main);
        // A first start where the system puts a new window, as before; a
        // reset explicitly centered on the main display.
        let offset = match reason {
            Reason::Fresh => None,
            _ => main.map(|d| centered(d, size)),
        };
        frame(size, primary, offset, reason)
    };
    let saved = match saved {
        None => return fresh(Reason::Fresh),
        Some(Ok(saved)) if saved.readable() => saved,
        Some(_) => return fresh(Reason::Unreadable),
    };
    let f = &saved.frame;
    // The normal frame: on its display where it fits there.
    let display = match &f.display {
        Some(key) => find(key, displays),
        // No display was told when it was saved (none known then): the
        // main one.
        None => primary,
    };
    let mut out = match display {
        None if !displays.is_empty() => fresh(Reason::DisplayGone),
        None => {
            // No displays told now: the size alone, as saved, if usable
            // (nothing to check it against but the least).
            if f.size[0] >= MIN_SIZE[0] && f.size[1] >= MIN_SIZE[1] {
                frame(f.size, None, None, Reason::Restored)
            } else {
                fresh(Reason::TooSmall)
            }
        }
        Some(i) => {
            let d = &displays[i];
            let room = d.logical();
            if f.size[0] < MIN_SIZE[0] || f.size[1] < MIN_SIZE[1] {
                fresh(Reason::TooSmall)
            } else if f.size[0] > room[0] + SLACK || f.size[1] > room[1] + SLACK {
                fresh(Reason::TooLarge)
            } else {
                match f.offset {
                    Some(o) if !on_display(f.size, o, room) => fresh(Reason::OffDisplay),
                    o => frame(f.size, Some(i), o, Reason::Restored),
                }
            }
        }
    };
    out.maximized = saved.maximized && !saved.fullscreen;
    if saved.fullscreen {
        let on = saved.fullscreen_on.as_ref().and_then(|k| find(k, displays));
        out.fullscreen = Some(on.or(primary));
        out.fullscreen_reason = Some(match (&saved.fullscreen_on, on) {
            (Some(_), None) if !displays.is_empty() => Reason::DisplayGone,
            _ => Reason::Restored,
        });
    }
    out
}

/// When what changed is written: [`WRITE_AFTER`] once the window is left
/// as it is, and whatever is still to be written at the end
/// ([`Writes::last`]). Times are the caller's seconds.
#[derive(Debug, Default)]
pub struct Writes {
    /// The newest state seen.
    current: Option<Saved>,
    /// The state last written (or read at the start).
    written: Option<Saved>,
    /// When the newest state is to be written.
    due: Option<f64>,
    /// Fullscreen is still wanted on this display (none: the window's
    /// own) though the window is not: a restore under way or given up
    /// ([`restore`]). What is kept says fullscreen until it is
    /// [`Writes::released`], so a restore the system refused is tried
    /// again at the next start rather than forgotten at the next quit.
    wanted: Option<Option<DisplayKey>>,
    /// The window as last seen, learned again when `wanted` goes.
    seen: Option<Observed>,
}

impl Writes {
    /// Starts from what the window was made as (`made`), with what was
    /// read (`read`, which need not be written again): where they differ
    /// (what was read was not given back as it was) the window's state
    /// replaces it once it changes, or at the end.
    pub fn new(read: Option<Saved>, made: Saved) -> Writes {
        Writes {
            current: Some(made),
            written: read,
            due: None,
            wanted: None,
            seen: None,
        }
    }

    /// Fullscreen on `on` is wanted until [`Writes::released`], whatever
    /// the window says meanwhile.
    pub fn want_fullscreen(&mut self, on: Option<DisplayKey>) {
        self.wanted = Some(on);
    }

    /// Whether fullscreen is still kept as wanted.
    pub fn wants_fullscreen(&self) -> bool {
        self.wanted.is_some()
    }

    /// Fullscreen is no longer wanted for the window (it took, or the
    /// player chose otherwise): what the window last said is kept.
    pub fn released(&mut self, now: f64) {
        if self.wanted.take().is_some()
            && let Some(seen) = self.seen.clone()
        {
            self.observe(&seen, now);
        }
    }

    /// The newest state seen.
    pub fn current(&self) -> Option<&Saved> {
        self.current.as_ref()
    }

    /// The window was seen at `now` as `seen`: what it learns of it.
    pub fn observe(&mut self, seen: &Observed, now: f64) {
        self.seen = Some(seen.clone());
        let mut next = Saved::observe(self.current.as_ref(), seen);
        if let (Some(on), Some(next)) = (&self.wanted, &mut next) {
            next.fullscreen = true;
            next.maximized = false;
            next.fullscreen_on = on.clone();
        }
        if next == self.current {
            return;
        }
        self.current = next;
        self.due = (self.current != self.written).then_some(now + WRITE_AFTER);
    }

    /// When the loop should come back to write, if it should.
    pub fn wake(&self) -> Option<f64> {
        self.due
    }

    /// What to write at `now`, once it has been left as it is long
    /// enough.
    pub fn due(&mut self, now: f64) -> Option<Saved> {
        if self.due.is_some_and(|d| now >= d) {
            return self.last();
        }
        None
    }

    /// What is still to be written, whatever the time (the window
    /// closes).
    pub fn last(&mut self) -> Option<Saved> {
        self.due = None;
        if self.current == self.written {
            return None;
        }
        self.written = self.current.clone();
        self.current.clone()
    }
}

/// Whether a run remembers its window: every run that opens one but a
/// timed one (`--seconds`, the runs scripts and watchdogs make), which
/// neither reads nor writes it. (No flag sets a window's size or
/// fullscreen; runs without a window, offscreen and the tests, never
/// come here.)
pub fn remembers(a: crate::args::Args) -> bool {
    !a.has("--seconds")
}

/// The key that enters and leaves fullscreen where the system has a
/// usual one: ⌃⌘F on macOS (Apple's, the Window menu's Enter Full
/// Screen), F11 on Windows and Linux (as browsers and file managers
/// there), with no other modifier. `key` is the logical key's text (a
/// layout's F), `code` the physical key's.
pub fn is_fullscreen_key(code: &str, key: &str, m: ddi_ui::event::Modifiers, mac: bool) -> bool {
    if mac {
        m.ctrl && m.meta && !m.alt && !m.shift && key.eq_ignore_ascii_case("f")
    } else {
        code == "F11" && !m.ctrl && !m.meta && !m.alt && !m.shift
    }
}

pub mod restore;

#[cfg(test)]
mod tests;
