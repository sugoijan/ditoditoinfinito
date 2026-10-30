//! Recording notes on the editor's screen (`docs/plans/editor.md`, "Note
//! record"; the take's rules are `state::editor::record`'s): R plays the
//! music from a measure before the cursor (over the selection only when
//! there is one) with a count-in, and the keys and controllers a play
//! reads for the chart's layout ([`RecordInput`]) are its lanes, each
//! press at the second it was heard ([`EditorHost::heard_at`], its time
//! stamp as a play judges it); Shift R adds to the notes there instead of
//! replacing them. While it records the screen takes keys and controls
//! as they are ([`crate::Screen::raw_input`]): bound ones are lanes, even
//! the editor's own (the arrows); R, Escape, Backspace, and Space or
//! Enter when not bound, a controller's Back or Select, or a tap stop it;
//! nothing else does anything. The notes recorded show tinted at their
//! rows until it stops; then the take is one edit, the cursor on the row
//! it stopped on, and the panel says how far the presses were from the
//! grid.

use ddi_actor::Actor;
use ddi_chart::{Layout, NoteKind, TimingMap};
use ddi_engine::frame::{SkinEvent, SkinEventKind};
use ddi_engine::input::{Bindings, LaneInput};
use ddi_engine::{ControlBindings, MenuInput};
use ddi_platform::{DeviceId, HostTime, RawInput};

use super::editor_field::Mapping;
use super::{EditorCall, EditorScreen, trim_number, with_alpha};
use crate::gesture::Gesture;
use crate::look::{call, quad};
use crate::screen::{Cx, Handled, Input};
use crate::state::editor::record::{Mode, Recorded, Take};

/// The recording's tint.
const TINT: [f32; 4] = [0.95, 0.3, 0.38, 1.0];

/// What recording reads input with (the shell's: what a play of the
/// chart would read).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordInput {
    /// The lanes: a play's bindings for the chart's layout, keys and
    /// controllers (`ddi_session::play::play_bindings`).
    pub lanes: Bindings,
    /// The menus' bindings, and the controllers with the standard
    /// mapping: a controller's Back or Select stops.
    pub controls: ControlBindings,
    pub standard: Vec<String>,
}

impl RecordInput {
    /// The default keys of `layout`, where a shell gives none.
    pub fn defaults(layout: &Layout) -> RecordInput {
        let controls = ControlBindings::default();
        RecordInput {
            lanes: controls.resolve(layout, &[]),
            controls,
            standard: Vec::new(),
        }
    }
}

/// The recording, for the shells' debug views and tests: the take being
/// recorded, else the last one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordView {
    pub active: bool,
    /// "replace" or "add".
    pub mode: String,
    /// The first row recorded, ticks.
    pub start: i64,
    pub notes: usize,
    pub presses: usize,
    /// The presses' mean and spread from their rows, ms.
    pub mean_ms: Option<f64>,
    pub spread_ms: Option<f64>,
    /// Its notes: their ticks, lanes, and holds' and rolls' ends.
    pub rows: Vec<(i64, u8, Option<i64>)>,
}

/// The rows of `notes`, as [`RecordView::rows`].
fn rows(notes: &[ddi_chart::Note]) -> Vec<(i64, u8, Option<i64>)> {
    notes
        .iter()
        .map(|n| (n.tick.0, n.lane, n.kind.hold_end().map(|t| t.0)))
        .collect()
}

/// A take being recorded.
struct Recording {
    take: Take,
    input: RecordInput,
    lanes: LaneInput,
    /// Controllers whose bindings were asked for (a play adopts a pad
    /// first seen the same way).
    seen: Vec<String>,
    /// The second a stop was pressed at.
    stop: Option<f64>,
}

/// The recording state on the screen.
#[derive(Default)]
pub(super) struct Recorder {
    taking: Option<Recording>,
    last: Option<Recorded>,
}

impl EditorScreen {
    pub(super) fn recording(&self) -> bool {
        self.record.taking.is_some()
    }

    /// R: records from the cursor or over the selection; `add`: adding to
    /// the notes there.
    pub(super) fn record(&mut self, add: bool, cx: &mut Cx) {
        if !self.place.listen || self.recording() {
            return;
        }
        self.stop_listening(cx);
        let mode = if add { Mode::Add } else { Mode::Replace };
        let (take, layout) = {
            let e = self.editor.borrow();
            (Take::new(&e, mode), e.layout().clone())
        };
        let input = self.record_input(&layout);
        let (from, count_in, start) = (take.preroll(), take.start_seconds(), take.start());
        self.record.taking = Some(Recording {
            take,
            lanes: LaneInput::new(input.lanes.clone()),
            input,
            seen: Vec::new(),
            stop: None,
        });
        self.listening = true;
        self.passed_to = None;
        self.listen_started();
        self.call(EditorCall::Record { from, count_in }, cx);
        let what = match mode {
            Mode::Replace => "Recording",
            Mode::Add => "Recording, adding",
        };
        self.say(format!(
            "{what} from beat {}: R or Escape stops",
            trim_number(start.beat())
        ));
    }

    fn record_input(&self, layout: &Layout) -> RecordInput {
        self.host
            .as_ref()
            .and_then(|h| h.record_input(layout))
            .unwrap_or_else(|| RecordInput::defaults(layout))
    }

    /// The song second heard at host time `host` (the music's now when a
    /// press has no time stamp).
    fn heard(&self, host: HostTime) -> Option<f64> {
        let h = self.host.as_ref()?;
        if host.0 > 0.0 {
            h.heard_at(host.0)
        } else {
            h.music_time()
        }
    }

    /// Input while recording: lanes, the stops, nothing else.
    pub(super) fn record_input_event(&mut self, input: &Input, cx: &mut Cx) -> Handled {
        match input {
            Input::Raw(raw) => self.record_raw(raw, cx),
            Input::Pointer { gesture, .. } => {
                if *gesture == Gesture::Tap {
                    self.stop_recording(None, cx);
                }
                Handled::Yes
            }
            Input::CancelDirections => {
                // The keys are let go (the window lost the focus).
                let now = self.host.as_ref().and_then(|h| h.music_time());
                if let (Some(rec), Some(now)) = (self.record.taking.as_mut(), now) {
                    rec.take.release_all(now);
                    rec.lanes = LaneInput::new(rec.input.lanes.clone());
                }
                Handled::No
            }
            Input::Key(_) => Handled::No,
            _ => Handled::Yes,
        }
    }

    fn record_raw(&mut self, raw: &RawInput, cx: &mut Cx) -> Handled {
        // A controller first seen: its bindings asked for again.
        if let DeviceId::Gamepad(id) = &raw.device
            && raw.pressed
            && self
                .record
                .taking
                .as_ref()
                .is_some_and(|r| !r.seen.contains(id))
        {
            let layout = self.editor.borrow().layout().clone();
            let input = self.record_input(&layout);
            if let Some(rec) = self.record.taking.as_mut() {
                rec.seen.push(id.clone());
                rec.lanes.bindings = input.lanes.clone();
                rec.input = input;
            }
        }
        let heard = self.heard(raw.host_time);
        let Some(rec) = self.record.taking.as_mut() else {
            return Handled::No;
        };
        if let Some(ev) = rec.lanes.feed(raw) {
            let Some(second) = heard else {
                return Handled::Yes;
            };
            let kind = if ev.pressed {
                rec.take.press(ev.lane, second);
                SkinEventKind::Press
            } else {
                rec.take.release(ev.lane, second);
                SkinEventKind::Lift
            };
            self.receptor(ev.lane, kind, second);
            self.changed();
            return Handled::Yes;
        }
        if raw.pressed && stops(&rec.input, raw) {
            self.stop_recording(heard, cx);
        }
        Handled::Yes
    }

    /// The skin's receptor pressed or let go.
    fn receptor(&mut self, lane: u8, kind: SkinEventKind, second: f64) {
        if !self.skin_drawn() {
            return;
        }
        self.events.push(SkinEvent {
            seq: self.seq,
            lane,
            kind,
            song_time: second,
            autoplay: false,
            danoni_judgments: false,
            note: None,
        });
        self.seq += 1;
        let keep = 64;
        if self.events.len() > keep {
            self.events.drain(..self.events.len() - keep);
        }
    }

    /// Stops at song second `at` (else where the music is heard).
    fn stop_recording(&mut self, at: Option<f64>, cx: &mut Cx) {
        if let Some(rec) = self.record.taking.as_mut() {
            rec.stop = at;
        }
        self.stop_listening(cx);
    }

    /// Each frame while recording, the music heard at `second`: the
    /// held lanes grow; a selection's take ends after its last row.
    pub(super) fn record_update(&mut self, second: f64, cx: &mut Cx) {
        let Some(rec) = self.record.taking.as_mut() else {
            return;
        };
        rec.take.update(second);
        if rec.take.ended(second) {
            self.stop_recording(Some(second), cx);
        }
    }

    /// The music stopped (`heard`: the second last heard): the take, if
    /// one was recording, is put in the chart. Whether there was one.
    pub(super) fn end_record(&mut self, heard: Option<f64>) -> bool {
        let Some(rec) = self.record.taking.take() else {
            return false;
        };
        let stop = rec
            .stop
            .or(heard)
            .unwrap_or_else(|| rec.take.start_seconds());
        let recorded = self.editor.borrow_mut().record(rec.take, stop);
        self.say(recorded.describe());
        self.record.last = Some(recorded);
        true
    }

    /// The menu's items for recording.
    pub(super) fn record_items(&self, items: &mut Vec<(&'static str, String)>) {
        if !self.place.listen || self.recording() {
            return;
        }
        let over = if self.editor.borrow().selection().is_some() {
            "the selection"
        } else {
            "from cursor"
        };
        items.push(("record", format!("Record {over} (R)")));
        items.push(("record-add", format!("Record {over}, adding (Shift R)")));
    }

    /// The menu's action `id`, if it is recording's.
    pub(super) fn record_action(&mut self, id: &str, cx: &mut Cx) -> bool {
        match id {
            "record" => self.record(false, cx),
            "record-add" => self.record(true, cx),
            _ => return false,
        }
        true
    }

    /// The take being recorded, else the last.
    pub(super) fn record_view(&self) -> Option<RecordView> {
        if let Some(rec) = &self.record.taking {
            let (mean_ms, spread_ms) = rec.take.stats();
            return Some(RecordView {
                active: true,
                mode: rec.take.mode().name().into(),
                start: rec.take.start().0,
                notes: rec.take.notes().len(),
                presses: rec.take.presses(),
                mean_ms,
                spread_ms,
                rows: rows(rec.take.notes()),
            });
        }
        self.record.last.as_ref().map(|r| RecordView {
            active: false,
            mode: r.mode.name().into(),
            start: r.span.0.0,
            notes: r.notes,
            presses: r.presses,
            mean_ms: r.mean_ms,
            spread_ms: r.spread_ms,
            rows: rows(&r.added),
        })
    }

    /// Over the field while recording: the rows recorded (from the start,
    /// to the selection's end), and the notes recorded, tinted.
    pub(super) fn record_layer(&self, m: &Mapping, timing: &TimingMap) -> Vec<Actor> {
        let Some(rec) = &self.record.taking else {
            return Vec::new();
        };
        let g = self.geo;
        let lanes = self.editor.borrow().lanes();
        let lanes_w = g.lane_w * f32::from(lanes);
        let [_, fy, _, fh] = g.field;
        let y_of = |t: i64| m.y(timing, t as f64);
        let mut c = Vec::new();
        // The span: a line at its start, its last row's when it has one.
        let take = &rec.take;
        for t in std::iter::once(take.start()).chain(take.last()) {
            let mut line = quad("Record.Edge", [lanes_w + 16.0, 2.0], with_alpha(TINT, 0.8));
            call(&mut line, "xy", &[g.lanes_x + lanes_w / 2.0, y_of(t.0)]);
            c.push(line);
        }
        let size = (g.lane_w * 0.82).min(44.0);
        let head_h = (size * 0.62).max(12.0);
        for (i, n) in take.notes().iter().enumerate() {
            let x = g.lanes_x + (f32::from(n.lane) + 0.5) * g.lane_w;
            let y = y_of(n.tick.0);
            if let Some(end) = n.kind.hold_end() {
                let yt = y_of(end.0);
                let (y0, y1) = (yt.min(y).max(fy - 4.0), yt.max(y).min(fy + fh + 4.0));
                let alpha = if matches!(n.kind, NoteKind::RollHead { .. }) {
                    0.4
                } else {
                    0.55
                };
                let mut body = quad(
                    "",
                    [size * 0.55, (y1 - y0).max(1.0)],
                    with_alpha(TINT, alpha),
                );
                call(&mut body, "xy", &[x, (y0 + y1) / 2.0]);
                c.push(body);
            }
            let mut p = quad(
                &format!("Record.{i}"),
                [size, head_h],
                with_alpha(TINT, 0.85),
            );
            call(&mut p, "xy", &[x, y]);
            call(&mut p, "cornerradius", &[head_h * 0.3]);
            c.push(p);
        }
        c
    }
}

/// Whether `raw`, bound to no lane, stops recording: R, Escape,
/// Backspace, Space or Enter; a controller's Back or Select.
fn stops(input: &RecordInput, raw: &RawInput) -> bool {
    match &raw.device {
        DeviceId::Keyboard => matches!(
            raw.control.as_str(),
            "KeyR" | "Escape" | "Backspace" | "Space" | "Enter" | "NumpadEnter"
        ),
        DeviceId::Gamepad(id) => {
            let standard = input.standard.contains(id);
            matches!(
                input.controls.menu_input(id, standard, &raw.control),
                Some(MenuInput::Back | MenuInput::Select)
            )
        }
        _ => false,
    }
}
