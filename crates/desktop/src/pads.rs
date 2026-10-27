//! Controllers on desktop (`docs/plans/desktop.md`, section 2, D5): gilrs
//! on a thread of its own, its events turned into the snapshots the web's
//! Gamepad API gives, fed to the same [`PadTracker`] (axes, hats,
//! hysteresis, plausible timestamps), and the edges sent to the main
//! thread, which a waker wakes (the window's event loop, P13).
//!
//! - **Ids** are `<name> (Vendor: vvvv Product: pppp)`, desktop's own
//!   (decision 11: each platform keeps its own bindings).
//! - **Standard pads** (gilrs maps them, from SDL's mappings or the
//!   driver) get the W3C standard numbers ([`w3c_button`], [`w3c_axis`]),
//!   so `ControlBindings`' standard defaults and menu input apply
//!   unchanged; gilrs's d-pad axes (macOS reports the d-pad as a hat)
//!   become buttons 12–15.
//! - **Other pads** (dance pads, adapters) number their raw controls by
//!   their codes, in ascending order and the same each time
//!   ([`raw_button`], [`raw_axis`]); hats arrive as two axes (gilrs's way)
//!   and bind as `axis:N±`.
//! - **Times:** gilrs stamps each event (`SystemTime`: on Linux the
//!   kernel's evdev time, on macOS its IOKit callback's, on Windows its
//!   poll's), turned into host time through `SystemTime::now()` and the
//!   host clock read together; the tracker falls back to the receipt
//!   for a stamp that is not plausible.
//!
//! The pure part ([`PadHub`]) takes [`PadEvent`]s, so recorded streams
//! test it, and a fake source ([`Pads::fake`]) drives the same thread
//! without gilrs.

use std::collections::{BTreeMap, HashSet};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use ddi_engine::ConnectedPad;
use ddi_platform::gamepad::{PadSnapshot, PadTracker};
use ddi_platform::{DeviceId, HostTime, RawInput};

/// A controller event, as gilrs reports it, without gilrs.
#[derive(Clone, Debug, PartialEq)]
pub enum PadEvent {
    Connected {
        pad: usize,
        name: String,
        vendor: Option<u16>,
        product: Option<u16>,
        /// gilrs maps it (SDL's mappings or the driver's layout).
        standard: bool,
    },
    Disconnected {
        pad: usize,
    },
    /// A button: its W3C number on a standard pad, else its raw index.
    Button {
        pad: usize,
        index: u16,
        pressed: bool,
    },
    /// An axis: its W3C number on a standard pad, else its raw index;
    /// −1…1, positive right and down as the W3C's.
    Axis {
        pad: usize,
        index: u16,
        value: f64,
    },
}

/// What the thread tells the main thread.
#[derive(Clone, Debug, PartialEq)]
pub enum PadOut {
    Edge(RawInput),
    Connected(ConnectedPad),
    Disconnected(ConnectedPad),
}

/// The binding id of a pad: its name, with its USB ids when it has them.
pub fn pad_id(name: &str, vendor: Option<u16>, product: Option<u16>) -> String {
    match (vendor, product) {
        (Some(v), Some(p)) => format!("{name} (Vendor: {v:04x} Product: {p:04x})"),
        _ => name.to_string(),
    }
}

struct Pad {
    info: ConnectedPad,
    snapshot: PadSnapshot,
    tracker: PadTracker,
}

/// Every pad's state and tracker: events in, edges out.
#[derive(Default)]
pub struct PadHub {
    pads: BTreeMap<usize, Pad>,
}

impl PadHub {
    pub fn new() -> PadHub {
        PadHub::default()
    }

    /// `event`, stamped `at` by its source and received `now`.
    pub fn event(&mut self, event: PadEvent, at: HostTime, now: HostTime, out: &mut Vec<PadOut>) {
        self.events(vec![event], at, now, out);
    }

    /// `events` of one report (gilrs's d-pad axis is two buttons), all
    /// stamped `at`: read as one reading of each pad they change.
    pub fn events(
        &mut self,
        events: Vec<PadEvent>,
        at: HostTime,
        now: HostTime,
        out: &mut Vec<PadOut>,
    ) {
        let mut changed: Vec<usize> = Vec::new();
        for event in events {
            if let Some(pad) = self.apply(event, at, now, out)
                && !changed.contains(&pad)
            {
                changed.push(pad);
            }
        }
        for pad in changed {
            if let Some(p) = self.pads.get_mut(&pad) {
                p.snapshot.timestamp = at;
                Self::track(p, now, out);
            }
        }
    }

    /// Applies `event` to its pad's reading; the pad when a reading is
    /// to be tracked.
    fn apply(
        &mut self,
        event: PadEvent,
        at: HostTime,
        now: HostTime,
        out: &mut Vec<PadOut>,
    ) -> Option<usize> {
        match event {
            PadEvent::Connected {
                pad,
                name,
                vendor,
                product,
                standard,
            } => {
                // Connected again in its slot: what the old one held is
                // let go first.
                if let Some(mut old) = self.pads.remove(&pad) {
                    old.snapshot.buttons.fill(false);
                    old.snapshot.axes.fill(0.0);
                    old.snapshot.timestamp = at;
                    Self::track(&mut old, now, out);
                }
                let info = ConnectedPad {
                    vendor_product: vendor.zip(product).map(|(v, p)| [v, p]),
                    id: pad_id(&name, vendor, product),
                    index: pad as u32,
                    standard,
                };
                let tracker = PadTracker::for_slot(&info.id, info.index);
                let mut p = Pad {
                    snapshot: PadSnapshot {
                        index: info.index,
                        id: info.id.clone(),
                        standard,
                        timestamp: at,
                        buttons: Vec::new(),
                        axes: Vec::new(),
                    },
                    info: info.clone(),
                    tracker,
                };
                // The first reading: the resting state, no edges.
                Self::track(&mut p, now, out);
                self.pads.insert(pad, p);
                out.push(PadOut::Connected(info));
                None
            }
            PadEvent::Disconnected { pad } => {
                if let Some(mut p) = self.pads.remove(&pad) {
                    // What it held is let go.
                    p.snapshot.buttons.fill(false);
                    p.snapshot.axes.fill(0.0);
                    p.snapshot.timestamp = at;
                    Self::track(&mut p, now, out);
                    out.push(PadOut::Disconnected(p.info));
                }
                None
            }
            PadEvent::Button {
                pad,
                index,
                pressed,
            } => {
                let p = self.seen(pad, at, now, out);
                let i = usize::from(index);
                if p.snapshot.buttons.len() <= i {
                    p.snapshot.buttons.resize(i + 1, false);
                }
                p.snapshot.buttons[i] = pressed;
                Some(pad)
            }
            PadEvent::Axis { pad, index, value } => {
                let p = self.seen(pad, at, now, out);
                let i = usize::from(index);
                if p.snapshot.axes.len() <= i {
                    p.snapshot.axes.resize(i + 1, 0.0);
                }
                p.snapshot.axes[i] = value;
                Some(pad)
            }
        }
    }

    /// Pad `pad`, made on first sight when no `Connected` came for it (a
    /// source that only reports pads plugged in later): by the raw
    /// numbering, under a name of its slot.
    fn seen(&mut self, pad: usize, at: HostTime, now: HostTime, out: &mut Vec<PadOut>) -> &mut Pad {
        if !self.pads.contains_key(&pad) {
            self.apply(
                PadEvent::Connected {
                    pad,
                    name: format!("Controller {}", pad + 1),
                    vendor: None,
                    product: None,
                    standard: false,
                },
                at,
                now,
                out,
            );
        }
        self.pads.get_mut(&pad).expect("made above")
    }

    fn track(p: &mut Pad, now: HostTime, out: &mut Vec<PadOut>) {
        let mut edges = Vec::new();
        p.tracker.update(&p.snapshot, now, &mut edges);
        out.extend(edges.into_iter().map(PadOut::Edge));
    }

    pub fn connected(&self) -> Vec<ConnectedPad> {
        self.pads.values().map(|p| p.info.clone()).collect()
    }
}

/// The first number of a standard pad's buttons the standard does not
/// name.
pub const EXTRA: u16 = 17;

/// The W3C standard number of gilrs's button (`None`: none).
pub fn w3c_button(b: gilrs::Button) -> Option<u16> {
    use gilrs::Button::*;
    Some(match b {
        South => 0,
        East => 1,
        West => 2,
        North => 3,
        LeftTrigger => 4,
        RightTrigger => 5,
        LeftTrigger2 => 6,
        RightTrigger2 => 7,
        Select => 8,
        Start => 9,
        LeftThumb => 10,
        RightThumb => 11,
        DPadUp => 12,
        DPadDown => 13,
        DPadLeft => 14,
        DPadRight => 15,
        Mode => 16,
        _ => return None,
    })
}

/// The W3C standard number of gilrs's stick axis, and whether its sign
/// turns (gilrs's y is up, the W3C's down).
pub fn w3c_axis(a: gilrs::Axis) -> Option<(u16, bool)> {
    use gilrs::Axis::*;
    Some(match a {
        LeftStickX => (0, false),
        LeftStickY => (1, true),
        RightStickX => (2, false),
        RightStickY => (3, true),
        _ => return None,
    })
}

/// A raw button code's index on a pad gilrs does not map: by the
/// platform's code, ascending, the same on every connection.
///
/// - macOS (IOKit, `page << 16 | usage`): the button page's usages from 1
///   are 0, 1, …; other pages after 64.
/// - Linux (evdev, `EV_KEY << 16 | code`): `BTN_MISC` to the end of
///   `BTN_GAMEPAD` (0x100–0x13f) are 0–63, `BTN_TRIGGER_HAPPY` 64 on,
///   other keys after 128.
/// - Windows (Windows.Gaming.Input): the raw button's own index.
pub fn raw_button(code: u32) -> u16 {
    let (high, low) = (code >> 16, code & 0xffff);
    if cfg!(target_os = "macos") {
        if high == 9 && low >= 1 {
            (low - 1).min(63) as u16
        } else {
            64 + (low & 0x3f) as u16
        }
    } else if cfg!(target_os = "linux") {
        match low {
            0x100..=0x13f => (low - 0x100) as u16,
            0x2c0..=0x2ff => 64 + (low - 0x2c0) as u16,
            _ => 128 + (low & 0x7f) as u16,
        }
    } else {
        low.min(255) as u16
    }
}

/// A raw axis code's index on a pad gilrs does not map, as
/// [`raw_button`]: macOS's generic desktop usages from X (0x30) are 0, 1,
/// …, so the hat (0x39, and gilrs's second axis for it) is 9 and 10, as
/// browsers number it; Linux's `ABS_*` codes as they are (`ABS_HAT0X` is
/// 16); Windows's raw axis index.
pub fn raw_axis(code: u32) -> u16 {
    let low = code & 0xffff;
    if cfg!(target_os = "macos") {
        low.saturating_sub(0x30).min(63) as u16
    } else {
        low.min(63) as u16
    }
}

/// The host time of a gilrs stamp: `SystemTime::now()` and the host
/// clock read together, so a step of the wall clock never reaches it.
fn host_time_of(at: SystemTime) -> (HostTime, HostTime) {
    let (wall, now) = (SystemTime::now(), crate::clock::now());
    let age = wall.duration_since(at).unwrap_or(Duration::ZERO);
    (HostTime(now.0 - age.as_secs_f64()), now)
}

/// Whether a pad gilrs reports through `source` reads by the standard
/// (W3C) numbering: one SDL's mappings know; one mapped by its driver
/// only when it has a gamepad's buttons (`gamepad_buttons`: Linux's
/// `BTN_GAMEPAD` range, Windows.Gaming.Input's `Gamepad` (XInput pads
/// among them), unknown elsewhere). gilrs 0.11 calls every
/// unmapped pad `Driver`; on macOS that is any HID joystick (a dance
/// pad), so there only SDL's mappings count.
pub fn standard_for(source: gilrs::MappingSource, gamepad_buttons: Option<bool>) -> bool {
    match source {
        gilrs::MappingSource::SdlMappings => true,
        gilrs::MappingSource::None => false,
        gilrs::MappingSource::Driver => {
            !cfg!(target_os = "macos") && gamepad_buttons.unwrap_or(true)
        }
    }
}

/// Whether a Linux `capabilities/key` bitmap (hex words, the highest
/// first, as sysfs writes them) has key `bit`.
pub fn key_caps_have(caps: &str, bit: usize) -> bool {
    let words: Vec<&str> = caps.split_whitespace().collect();
    let width = if words.iter().any(|w| w.len() > 8) {
        64
    } else {
        // 32-bit systems write 32-bit words; a short line gives no hint,
        // and 64 is what 64-bit kernels write.
        if cfg!(target_pointer_width = "32") {
            32
        } else {
            64
        }
    };
    let (word, shift) = (bit / width, bit % width);
    let Some(w) = words.len().checked_sub(word + 1).and_then(|i| words.get(i)) else {
        return false;
    };
    u64::from_str_radix(w, 16).is_ok_and(|v| (v >> shift) & 1 == 1)
}

/// Linux: whether the input device named `name` (with these USB ids) has
/// a gamepad's buttons (`BTN_GAMEPAD`, 0x130), from sysfs; `None` when it
/// is not found there.
#[cfg(target_os = "linux")]
fn has_gamepad_buttons(name: &str, vendor: Option<u16>, product: Option<u16>) -> Option<bool> {
    let read = |p: std::path::PathBuf| {
        std::fs::read_to_string(p)
            .ok()
            .map(|s| s.trim().to_string())
    };
    for e in std::fs::read_dir("/sys/class/input").ok()?.flatten() {
        let dev = e.path().join("device");
        if read(dev.join("name")).as_deref() != Some(name) {
            continue;
        }
        let id =
            |f: &str| read(dev.join("id").join(f)).and_then(|v| u16::from_str_radix(&v, 16).ok());
        if (vendor.is_some() && id("vendor") != vendor)
            || (product.is_some() && id("product") != product)
        {
            continue;
        }
        if let Some(caps) = read(dev.join("capabilities/key")) {
            return Some(key_caps_have(&caps, 0x130));
        }
    }
    None
}

/// The `Connected` event of gilrs's pad `id`, its numbering decided.
fn connected_event(gilrs: &gilrs::Gilrs, id: gilrs::GamepadId) -> PadEvent {
    let gamepad = gilrs.gamepad(id);
    let (vendor, product) = (gamepad.vendor_id(), gamepad.product_id());
    #[cfg(target_os = "linux")]
    let buttons = has_gamepad_buttons(gamepad.os_name(), vendor, product);
    // Windows.Gaming.Input: a `Gamepad` (XInput's and WGI's own) has
    // gilrs-core's nil uuid; any other raw game controller (a dance pad)
    // a uuid made from its USB ids.
    #[cfg(windows)]
    let buttons = Some(gamepad.uuid() == [0; 16]);
    #[cfg(not(any(target_os = "linux", windows)))]
    let buttons = None;
    PadEvent::Connected {
        pad: usize::from(id),
        name: gamepad.name().to_string(),
        vendor,
        product,
        standard: standard_for(gamepad.mapping_source(), buttons),
    }
}

/// The pad's [`PadEvent`]s of a gilrs event; `standard`: its numbering
/// as its `Connected` said (`connected_event`).
fn from_gilrs(e: &gilrs::Event, standard: bool, out: &mut Vec<PadEvent>) {
    use gilrs::EventType::*;
    let pad = usize::from(e.id);
    match e.event {
        Connected => {}
        Disconnected => out.push(PadEvent::Disconnected { pad }),
        ButtonPressed(b, code) | ButtonReleased(b, code) => {
            let pressed = matches!(e.event, ButtonPressed(..));
            out.extend(button_event(pad, standard, b, code.into_u32(), pressed));
        }
        AxisChanged(a, value, code) => {
            axis_events(pad, standard, a, code.into_u32(), f64::from(value), out)
        }
        _ => {}
    }
}

/// A gilrs button's press or release (`code`: its raw code, packed):
/// by its W3C number on a standard pad (a button the standard has no
/// number for after them, from 17, as browsers number a pad's extra
/// buttons: by its raw index, so the same each time), by its raw index on
/// another.
pub fn button_event(
    pad: usize,
    standard: bool,
    button: gilrs::Button,
    code: u32,
    pressed: bool,
) -> Option<PadEvent> {
    let index = if standard {
        w3c_button(button).unwrap_or(EXTRA + raw_button(code))
    } else {
        raw_button(code)
    };
    Some(PadEvent::Button {
        pad,
        index,
        pressed,
    })
}

/// A gilrs axis's change: on a standard pad the sticks by their W3C
/// numbers (y turned down) and the d-pad's axes (a hat on macOS) as its
/// buttons; on another, the raw axis.
pub fn axis_events(
    pad: usize,
    standard: bool,
    axis: gilrs::Axis,
    code: u32,
    value: f64,
    out: &mut Vec<PadEvent>,
) {
    if !standard {
        out.push(PadEvent::Axis {
            pad,
            index: raw_axis(code),
            value,
        });
        return;
    }
    let dpad = match axis {
        gilrs::Axis::DPadX => Some([(14, value <= -0.5), (15, value >= 0.5)]),
        gilrs::Axis::DPadY => Some([(12, value >= 0.5), (13, value <= -0.5)]),
        _ => None,
    };
    if let Some(buttons) = dpad {
        for (index, pressed) in buttons {
            out.push(PadEvent::Button {
                pad,
                index,
                pressed,
            });
        }
    } else if let Some((index, turn)) = w3c_axis(axis) {
        out.push(PadEvent::Axis {
            pad,
            index,
            value: if turn { -value } else { value },
        });
    }
}

/// Where the pads' events come from.
enum Source {
    Gilrs(Box<gilrs::Gilrs>),
    Fake(mpsc::Receiver<PadEvent>),
}

/// The controllers, read on their own thread.
pub struct Pads {
    edges: mpsc::Receiver<PadOut>,
    connected: Vec<ConnectedPad>,
    /// Controls each pad (`(id, slot)`) holds now.
    held: BTreeMap<(String, u32), HashSet<String>>,
    /// Whether controllers can be read at all here.
    pub supported: bool,
    /// Edges received, for the debug summary.
    pub edges_seen: u64,
}

/// Feeds a fake pad's events to [`Pads::fake`]'s thread (tests, the
/// `--fake-pad` run).
#[derive(Clone)]
pub struct FakePad(mpsc::Sender<PadEvent>);

impl FakePad {
    pub fn send(&self, e: PadEvent) {
        let _ = self.0.send(e);
    }
}

impl Pads {
    /// gilrs on its thread; `wake` is called after each batch of news
    /// (the event loop's proxy). Without gilrs here, no pads.
    pub fn start(wake: Box<dyn Fn() + Send>) -> Pads {
        let gilrs = gilrs::GilrsBuilder::new()
            // `PadTracker` does hysteresis: no deadzone or jitter filter.
            .with_default_filters(false)
            .build();
        match gilrs {
            Ok(g) => Pads::spawn(Source::Gilrs(Box::new(g)), wake, true),
            Err(gilrs::Error::NotImplemented(g)) => {
                Pads::spawn(Source::Gilrs(Box::new(g)), wake, false)
            }
            Err(e) => {
                eprintln!("controllers: {e}");
                let (_tx, rx) = mpsc::channel();
                Pads::spawn(Source::Fake(rx), wake, false)
            }
        }
    }

    /// A fake source on the same thread and hub (no gilrs).
    pub fn fake(wake: Box<dyn Fn() + Send>) -> (Pads, FakePad) {
        let (tx, rx) = mpsc::channel();
        (Pads::spawn(Source::Fake(rx), wake, true), FakePad(tx))
    }

    fn spawn(source: Source, wake: Box<dyn Fn() + Send>, supported: bool) -> Pads {
        let (tx, edges) = mpsc::channel();
        let run = move || {
            let mut hub = PadHub::new();
            let mut events = Vec::new();
            let mut out = Vec::new();
            match source {
                Source::Gilrs(mut gilrs) => {
                    // Each pad's numbering, from its `Connected`.
                    let mut known: BTreeMap<usize, bool> = BTreeMap::new();
                    // Pads there before gilrs started: no `Connected`
                    // comes for them (Linux, Windows).
                    let now = crate::clock::now();
                    let present: Vec<gilrs::GamepadId> =
                        gilrs.gamepads().map(|(id, _)| id).collect();
                    for id in present {
                        let ev = connected_event(&gilrs, id);
                        if let PadEvent::Connected { pad, standard, .. } = &ev {
                            known.insert(*pad, *standard);
                        }
                        hub.event(ev, now, now, &mut out);
                    }
                    if !send(&tx, &mut out, &wake) {
                        return;
                    }
                    loop {
                        let Some(e) = gilrs.next_event_blocking(Some(Duration::from_millis(250)))
                        else {
                            continue;
                        };
                        let (at, now) = host_time_of(e.time);
                        let pad = usize::from(e.id);
                        let first = !known.contains_key(&pad);
                        if matches!(e.event, gilrs::EventType::Connected)
                            || (first && !matches!(e.event, gilrs::EventType::Disconnected))
                        {
                            let ev = connected_event(&gilrs, e.id);
                            if let PadEvent::Connected { standard, .. } = &ev {
                                known.insert(pad, *standard);
                            }
                            hub.event(ev, at, now, &mut out);
                        }
                        if matches!(e.event, gilrs::EventType::Disconnected) {
                            known.remove(&pad);
                        }
                        let standard = known.get(&pad).copied().unwrap_or(false);
                        from_gilrs(&e, standard, &mut events);
                        hub.events(std::mem::take(&mut events), at, now, &mut out);
                        if !send(&tx, &mut out, &wake) {
                            return;
                        }
                    }
                }
                Source::Fake(rx) => {
                    while let Ok(ev) = rx.recv() {
                        let now = crate::clock::now();
                        hub.event(ev, now, now, &mut out);
                        if !send(&tx, &mut out, &wake) {
                            return;
                        }
                    }
                }
            }
        };
        let _ = std::thread::Builder::new()
            .name("ddi-pads".into())
            .spawn(run);
        Pads {
            edges,
            connected: Vec::new(),
            held: BTreeMap::new(),
            supported,
            edges_seen: 0,
        }
    }

    /// The edges and changes since the last call, in order.
    pub fn take(&mut self) -> Vec<PadOut> {
        let news: Vec<PadOut> = self.edges.try_iter().collect();
        for n in &news {
            match n {
                PadOut::Edge(e) => {
                    self.edges_seen += 1;
                    if let DeviceId::Gamepad(id) = &e.device {
                        let held = self.held.entry((id.clone(), e.slot)).or_default();
                        if e.pressed {
                            held.insert(e.control.clone());
                        } else {
                            held.remove(&e.control);
                        }
                    }
                }
                PadOut::Connected(p) => {
                    self.connected.retain(|c| c.index != p.index);
                    self.connected.push(p.clone());
                }
                PadOut::Disconnected(p) => {
                    self.connected.retain(|c| c.index != p.index);
                    self.held.remove(&(p.id.clone(), p.index));
                }
            }
        }
        news
    }

    /// Pads connected now, in their slots' order.
    pub fn connected(&self) -> Vec<ConnectedPad> {
        let mut v = self.connected.clone();
        v.sort_by_key(|p| p.index);
        v
    }

    /// Whether the pad in `slot` is standard.
    pub fn standard(&self, slot: u32) -> bool {
        self.connected.iter().any(|p| p.index == slot && p.standard)
    }

    /// The controls every pad with binding id `id` holds now.
    pub fn held(&self, id: &str) -> HashSet<String> {
        self.held
            .iter()
            .filter(|((i, _), _)| i == id)
            .flat_map(|(_, h)| h.iter().cloned())
            .collect()
    }
}

/// Sends what the hub put out; whether the main thread still listens.
fn send(tx: &mpsc::Sender<PadOut>, out: &mut Vec<PadOut>, wake: &dyn Fn()) -> bool {
    if out.is_empty() {
        return true;
    }
    for o in out.drain(..) {
        if tx.send(o).is_err() {
            return false;
        }
    }
    wake();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(out: &[PadOut]) -> Vec<(String, bool, f64, u32)> {
        out.iter()
            .filter_map(|o| match o {
                PadOut::Edge(e) => Some((e.control.clone(), e.pressed, e.host_time.0, e.slot)),
                _ => None,
            })
            .collect()
    }

    /// A recorded stream of a standard pad (gilrs's events as `PadEvent`s:
    /// an Xbox controller over USB on macOS) through the hub: its id, the
    /// W3C numbers, the d-pad's hat as buttons, a stick with hysteresis,
    /// stamps kept when plausible.
    #[test]
    fn a_standard_pad_plays_by_the_w3c_numbers() {
        let mut hub = PadHub::new();
        let mut out = Vec::new();
        let ev = |hub: &mut PadHub, out: &mut Vec<PadOut>, e, at: f64, now: f64| {
            hub.event(e, HostTime(at), HostTime(now), out)
        };
        ev(
            &mut hub,
            &mut out,
            PadEvent::Connected {
                pad: 0,
                name: "Xbox Wireless Controller".into(),
                vendor: Some(0x045e),
                product: Some(0x0b13),
                standard: true,
            },
            1.0,
            1.0,
        );
        assert_eq!(
            out[0],
            PadOut::Connected(ConnectedPad {
                vendor_product: Some([0x045e, 0x0b13]),
                id: "Xbox Wireless Controller (Vendor: 045e Product: 0b13)".into(),
                index: 0,
                standard: true,
            })
        );
        out.clear();
        // A (South), the d-pad down (a hat on macOS: buttons), Start.
        let stream = [
            (
                PadEvent::Button {
                    pad: 0,
                    index: 0,
                    pressed: true,
                },
                2.000,
                2.002,
            ),
            (
                PadEvent::Button {
                    pad: 0,
                    index: 0,
                    pressed: false,
                },
                2.080,
                2.081,
            ),
            (
                PadEvent::Button {
                    pad: 0,
                    index: 12,
                    pressed: false,
                },
                2.500,
                2.501,
            ),
            (
                PadEvent::Button {
                    pad: 0,
                    index: 13,
                    pressed: true,
                },
                2.500,
                2.501,
            ),
            (
                PadEvent::Button {
                    pad: 0,
                    index: 13,
                    pressed: false,
                },
                2.600,
                2.602,
            ),
            // The left stick down past the press point, back above the
            // release point: one press, one release.
            (
                PadEvent::Axis {
                    pad: 0,
                    index: 1,
                    value: 0.3,
                },
                3.000,
                3.001,
            ),
            (
                PadEvent::Axis {
                    pad: 0,
                    index: 1,
                    value: 0.7,
                },
                3.010,
                3.011,
            ),
            (
                PadEvent::Axis {
                    pad: 0,
                    index: 1,
                    value: 0.5,
                },
                3.020,
                3.021,
            ),
            (
                PadEvent::Axis {
                    pad: 0,
                    index: 1,
                    value: 0.2,
                },
                3.030,
                3.031,
            ),
            (
                PadEvent::Button {
                    pad: 0,
                    index: 9,
                    pressed: true,
                },
                4.000,
                4.001,
            ),
        ];
        // Events of one report (the same stamp: the d-pad's hat moving
        // from up to down) are one reading.
        let mut batch: Vec<PadEvent> = Vec::new();
        let mut stamp = (0.0, 0.0);
        for (e, at, now) in stream {
            if (at, now) != stamp && !batch.is_empty() {
                hub.events(
                    std::mem::take(&mut batch),
                    HostTime(stamp.0),
                    HostTime(stamp.1),
                    &mut out,
                );
            }
            stamp = (at, now);
            batch.push(e);
        }
        hub.events(batch, HostTime(stamp.0), HostTime(stamp.1), &mut out);
        assert_eq!(
            edges(&out),
            vec![
                ("button:0".into(), true, 2.000, 0),
                ("button:0".into(), false, 2.080, 0),
                ("button:13".into(), true, 2.500, 0),
                ("button:13".into(), false, 2.600, 0),
                ("axis:1+".into(), true, 3.010, 0),
                ("axis:1+".into(), false, 3.030, 0),
                ("button:9".into(), true, 4.000, 0),
            ]
        );
        // Unplugged holding Start: released.
        out.clear();
        ev(
            &mut hub,
            &mut out,
            PadEvent::Disconnected { pad: 0 },
            5.0,
            5.0,
        );
        assert_eq!(edges(&out), vec![("button:9".into(), false, 5.0, 0)]);
        assert!(matches!(out.last(), Some(PadOut::Disconnected(_))));
        assert!(hub.connected().is_empty());
    }

    /// A recorded dance pad without a mapping (a USB adapter: buttons by
    /// raw code, its arrows on a hat that gilrs gives as two axes), and a
    /// stamp that is not plausible (from the future) falling back to the
    /// receipt.
    #[test]
    fn an_unmapped_pad_numbers_its_raw_controls() {
        let mut hub = PadHub::new();
        let mut out = Vec::new();
        hub.event(
            PadEvent::Connected {
                pad: 3,
                name: "USB Gamepad".into(),
                vendor: None,
                product: None,
                standard: false,
            },
            HostTime(0.0),
            HostTime(0.0),
            &mut out,
        );
        assert!(
            matches!(&out[0], PadOut::Connected(p) if p.id == "USB Gamepad" && !p.standard && p.index == 3)
        );
        out.clear();
        let hat_x = raw_axis(if cfg!(target_os = "macos") {
            0x1_0039
        } else {
            0x3_0010
        });
        let button = raw_button(if cfg!(target_os = "macos") {
            0x9_0003
        } else {
            0x1_0122
        });
        let stream = [
            (
                PadEvent::Axis {
                    pad: 3,
                    index: hat_x,
                    value: -1.0,
                },
                1.0,
                1.001,
            ),
            (
                PadEvent::Axis {
                    pad: 3,
                    index: hat_x,
                    value: 0.0,
                },
                1.1,
                1.101,
            ),
            (
                PadEvent::Button {
                    pad: 3,
                    index: button,
                    pressed: true,
                },
                9.0,
                1.2,
            ),
        ];
        for (e, at, now) in stream {
            hub.event(e, HostTime(at), HostTime(now), &mut out);
        }
        let got = edges(&out);
        assert_eq!(got[0], (format!("axis:{hat_x}-"), true, 1.0, 3));
        assert_eq!(got[1], (format!("axis:{hat_x}-"), false, 1.1, 3));
        // Stamped in the future: the receipt.
        assert_eq!(got[2], (format!("button:{button}"), true, 1.2, 3));
    }

    /// gilrs's own events of a standard pad on macOS, as recorded (the
    /// d-pad a hat: two axes; the stick up; A; an unknown button), into
    /// the hub: the W3C controls.
    #[test]
    fn a_dance_pad_keeps_every_button_and_pads_seen_late_are_made() {
        use gilrs::{Button, MappingSource};
        // gilrs 0.11 calls an unmapped HID joystick `Driver`: only SDL's
        // mappings (or, on Linux, the gamepad buttons) make it standard.
        assert!(standard_for(MappingSource::SdlMappings, None));
        assert!(!standard_for(MappingSource::None, None));
        assert!(!standard_for(MappingSource::Driver, Some(false)));
        assert_eq!(
            standard_for(MappingSource::Driver, Some(true)),
            !cfg!(target_os = "macos")
        );
        // A dance pad's buttons (Linux: BTN_TRIGGER and on; macOS: the
        // button page's usages), as gilrs reports them: every one an
        // index of its own.
        let codes: Vec<u32> = if cfg!(target_os = "linux") {
            (0x120..0x12c).map(|c| 0x1_0000 | c).collect()
        } else {
            (1..13).map(|u| 0x9_0000 | u).collect()
        };
        let mut indices: Vec<u16> = codes
            .iter()
            .filter_map(
                |&c| match button_event(3, false, Button::Unknown, c, true) {
                    Some(PadEvent::Button { index, .. }) => Some(index),
                    _ => None,
                },
            )
            .collect();
        assert_eq!(indices.len(), 12);
        indices.dedup();
        assert_eq!(indices.len(), 12);
        // A pad no `Connected` came for (there before gilrs started):
        // made on its first event, its press kept.
        let mut hub = PadHub::new();
        let mut out = Vec::new();
        hub.event(
            PadEvent::Button {
                pad: 2,
                index: 4,
                pressed: true,
            },
            HostTime(1.0),
            HostTime(1.0),
            &mut out,
        );
        assert!(matches!(&out[0], PadOut::Connected(p) if p.index == 2 && !p.standard));
        assert_eq!(edges(&out).len(), 1);
        // Connected again in its slot while held: the old press is let go.
        out.clear();
        hub.event(
            PadEvent::Connected {
                pad: 2,
                name: "P".into(),
                vendor: None,
                product: None,
                standard: false,
            },
            HostTime(1.1),
            HostTime(1.1),
            &mut out,
        );
        let e = edges(&out);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(!e[0].1);
        // Linux's capability bitmaps.
        let gamepad = "7cdb000000000000 0 0 0 0";
        assert!(key_caps_have(gamepad, 0x130));
        let joystick = "ffff00000000 0 0 0 0";
        assert!(key_caps_have(joystick, 0x120));
        assert!(!key_caps_have(joystick, 0x130));
    }

    #[test]
    fn gilrs_events_become_w3c_controls() {
        use gilrs::{Axis, Button};
        let mut hub = PadHub::new();
        let mut out = Vec::new();
        hub.event(
            PadEvent::Connected {
                pad: 0,
                name: "Pad".into(),
                vendor: None,
                product: None,
                standard: true,
            },
            HostTime(0.0),
            HostTime(0.0),
            &mut out,
        );
        out.clear();
        let mut t = 1.0;
        let mut report = |evs: Vec<PadEvent>, hub: &mut PadHub, out: &mut Vec<PadOut>| {
            t += 0.05;
            hub.events(evs, HostTime(t), HostTime(t + 0.001), out);
        };
        let mut evs = Vec::new();
        axis_events(0, true, Axis::DPadY, 0x1_0039, -1.0, &mut evs);
        report(std::mem::take(&mut evs), &mut hub, &mut out);
        axis_events(0, true, Axis::DPadY, 0x1_0039, 0.0, &mut evs);
        report(std::mem::take(&mut evs), &mut hub, &mut out);
        axis_events(0, true, Axis::LeftStickY, 0x1_0031, 0.9, &mut evs);
        report(std::mem::take(&mut evs), &mut hub, &mut out);
        axis_events(0, true, Axis::LeftStickY, 0x1_0031, 0.0, &mut evs);
        report(std::mem::take(&mut evs), &mut hub, &mut out);
        let a = button_event(0, true, Button::South, 0x9_0001, true);
        report(a.into_iter().collect(), &mut hub, &mut out);
        // A button the standard does not name: after the standard's.
        assert_eq!(
            button_event(0, true, Button::Unknown, 0x9_0020, true),
            Some(PadEvent::Button {
                pad: 0,
                index: EXTRA + raw_button(0x9_0020),
                pressed: true
            })
        );
        let got: Vec<(String, bool)> = edges(&out).into_iter().map(|(c, p, _, _)| (c, p)).collect();
        assert_eq!(
            got,
            vec![
                ("button:13".into(), true),
                ("button:13".into(), false),
                // gilrs's stick up is the W3C's axis 1 −.
                ("axis:1-".into(), true),
                ("axis:1-".into(), false),
                ("button:0".into(), true),
            ]
        );
        // An unmapped pad's controls by their raw codes.
        assert_eq!(
            button_event(0, false, Button::Unknown, 0x9_0003, true),
            Some(PadEvent::Button {
                pad: 0,
                index: raw_button(0x9_0003),
                pressed: true
            })
        );
    }

    #[test]
    fn raw_codes_number_the_same_way_each_time() {
        if cfg!(target_os = "macos") {
            // The button page's usages 1, 2, … are 0, 1, …
            assert_eq!(raw_button(0x9_0001), 0);
            assert_eq!(raw_button(0x9_0010), 15);
            // X, Y, the hat and gilrs's second hat axis.
            assert_eq!(raw_axis(0x1_0030), 0);
            assert_eq!(raw_axis(0x1_0031), 1);
            assert_eq!(raw_axis(0x1_0039), 9);
            assert_eq!(raw_axis(0x1_003a), 10);
        } else if cfg!(target_os = "linux") {
            assert_eq!(raw_button(0x1_0120), 0x20);
            assert_eq!(raw_button(0x1_0130), 0x30);
            assert_eq!(raw_button(0x1_02c0), 64);
            assert_eq!(raw_axis(0x3_0010), 16);
        }
        assert_eq!(
            pad_id("Pad", Some(1), Some(0xbeef)),
            "Pad (Vendor: 0001 Product: beef)"
        );
    }

    #[test]
    fn the_fake_source_runs_the_same_thread() {
        let (mut pads, fake) = Pads::fake(Box::new(|| {}));
        fake.send(PadEvent::Connected {
            pad: 1,
            name: "Fake".into(),
            vendor: None,
            product: None,
            standard: true,
        });
        fake.send(PadEvent::Button {
            pad: 1,
            index: 13,
            pressed: true,
        });
        let mut news = Vec::new();
        for _ in 0..200 {
            news.extend(pads.take());
            if news.len() >= 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            matches!(&news[1], PadOut::Edge(e) if e.control == "button:13" && e.pressed && e.slot == 1)
        );
        assert!(pads.standard(1));
        assert_eq!(pads.held("Fake"), HashSet::from(["button:13".to_string()]));
    }
}
