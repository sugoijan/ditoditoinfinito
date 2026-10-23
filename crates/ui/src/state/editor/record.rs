//! Recording notes (`docs/plans/editor.md`, "Note record"): the music
//! plays from a measure before the start row, and each press of a lane,
//! at the song second it was heard (its time stamp turned into it as a
//! play judges a press), is put on the snap's nearest row; a press held
//! longer than [`HOLD_SECONDS`] is a hold (a roll with the roll tool)
//! from its row to the row it was let go on. StepMania 5.1's record
//! mode, read for behavior only (`src/ScreenEdit.cpp`: `InputRecord`
//! places a tap at `Quantize(GetBeat(), snap)`, rows outside the range
//! ignored; `Update` makes a hold of a press held more than
//! `record_hold_seconds`, 0.3 s by default, its ends quantized the same
//! way; a tap on a row inside a hold takes the hold's place; only taps,
//! holds and rolls are recorded, whatever note type is chosen; rolls
//! while its roll key is held, here with the roll tool).
//!
//! A [`Take`] is what one recording heard; [`Editor::record`] puts it in
//! the chart as one change (one undo). [`Mode::Replace`] takes away the
//! notes whose heads are in the span recorded first; [`Mode::Add`]
//! keeps them (StepMania's record keeps them too: it records over a copy
//! of the chart, `TransitionEditState`), a note recorded on one kept.

use ddi_chart::{Note, NoteKind, TICKS_PER_BEAT, Tick, TimingMap};

use super::{Editor, NoteTool};

/// A press held longer than this, seconds, records a hold (StepMania's
/// `record_hold_default`).
pub const HOLD_SECONDS: f64 = 0.3;

/// What recording does to the notes already in the span it records.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// They are taken away first.
    #[default]
    Replace,
    /// They stay; a note recorded on one is left out.
    Add,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Replace => "replace",
            Mode::Add => "add",
        }
    }
}

/// A lane held down: since when, and the row its note is on.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    second: f64,
    head: Tick,
}

/// What one recording heard: the notes on their rows so far.
#[derive(Clone, Debug)]
pub struct Take {
    mode: Mode,
    /// The first row recorded, and the last (a selection's; `None`: until
    /// it stops).
    start: Tick,
    last: Option<Tick>,
    /// The snap's step, ticks.
    step: i64,
    roll: bool,
    lanes: u8,
    timing: TimingMap,
    notes: Vec<Note>,
    held: Vec<Option<Held>>,
    /// Each press recorded: its second less its row's, seconds.
    offsets: Vec<f64>,
}

/// What a take put in the chart.
#[derive(Clone, Debug, PartialEq)]
pub struct Recorded {
    pub mode: Mode,
    /// The notes put in, and how many.
    pub added: Vec<Note>,
    pub notes: usize,
    pub presses: usize,
    /// The presses' mean and standard deviation from their rows,
    /// milliseconds (late positive).
    pub mean_ms: Option<f64>,
    pub spread_ms: Option<f64>,
    /// The rows recorded, the end not included.
    pub span: (Tick, Tick),
}

impl Recorded {
    /// The line the editor says it with: "12 notes recorded · 12 ms
    /// late on average, ±9 ms".
    pub fn describe(&self) -> String {
        let notes = match self.notes {
            0 => "Nothing recorded".to_string(),
            1 => "1 note recorded".to_string(),
            n => format!("{n} notes recorded"),
        };
        match (self.mean_ms, self.spread_ms) {
            (Some(m), Some(s)) => {
                let side = if m.round() == 0.0 {
                    "on the grid".to_string()
                } else if m > 0.0 {
                    format!("{:.0} ms late", m)
                } else {
                    format!("{:.0} ms early", -m)
                };
                format!("{notes} · {side} on average, ±{s:.0} ms")
            }
            _ => notes,
        }
    }
}

impl Take {
    /// A take on `editor`'s chart from its cursor, or over its selection
    /// only, at its snap and with its tool.
    pub fn new(editor: &Editor, mode: Mode) -> Take {
        let (start, last) = match editor.selection() {
            Some((a, b)) => (a, Some(b)),
            None => (editor.cursor(), None),
        };
        let lanes = editor.lanes();
        Take {
            mode,
            start,
            last,
            step: editor.step(),
            roll: editor.tool() == NoteTool::Roll,
            lanes,
            timing: editor.timing().clone(),
            notes: Vec::new(),
            held: vec![None; usize::from(lanes)],
            offsets: Vec::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn start(&self) -> Tick {
        self.start
    }

    /// The last row recorded, a selection's.
    pub fn last(&self) -> Option<Tick> {
        self.last
    }

    /// The notes recorded so far (holds held as long as they are now).
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// The presses recorded so far.
    pub fn presses(&self) -> usize {
        self.offsets.len()
    }

    /// The song second the music starts at: a measure (by the time
    /// signature at the start row) before it, not before the song's
    /// start (the music's, or beat 0 before it).
    pub fn preroll(&self) -> f64 {
        let (n, d) = self
            .timing
            .time_signatures
            .iter()
            .rev()
            .find(|s| s.tick <= self.start)
            .map_or((4, 4), |s| (s.numerator, s.denominator));
        let measure = (i64::from(n) * 4 * TICKS_PER_BEAT) / i64::from(d.max(1));
        let song = self.timing.seconds_at(Tick::ZERO).min(0.0);
        self.timing
            .seconds_at(self.start - Tick(measure.max(1)))
            .max(song)
            .min(self.start_seconds())
    }

    /// The start row's second: the count-in clicks before it.
    pub fn start_seconds(&self) -> f64 {
        self.timing.seconds_at(self.start)
    }

    /// The row a press heard at `second` is recorded on: the beat the
    /// timing has then (a stop's or a delay's while in it, past a warp
    /// after it), to the snap's nearest row, the later of two as near
    /// (StepMania's `Quantize`); a row in a warp, never heard, becomes
    /// the first after it.
    pub fn row(&self, second: f64) -> Tick {
        let t = self.timing.beat_at(second) * TICKS_PER_BEAT as f64;
        let step = self.step as f64;
        let mut row = Tick(((t + step / 2.0) / step).floor() as i64 * self.step);
        for _ in 0..100_000 {
            if !self.timing.in_warp(row) {
                break;
            }
            row = row + Tick(self.step);
        }
        row
    }

    fn within(&self, row: Tick) -> bool {
        row >= self.start && self.last.is_none_or(|l| row <= l)
    }

    /// Lane `lane` pressed, heard at `second`: its row, or `None` when
    /// that is outside what is recorded.
    pub fn press(&mut self, lane: u8, second: f64) -> Option<Tick> {
        if lane >= self.lanes {
            return None;
        }
        // Another control on a lane held: the first press ends here.
        self.release(lane, second);
        let row = self.row(second);
        if !self.within(row) {
            return None;
        }
        self.offsets.push(second - self.timing.seconds_at(row));
        self.put(lane, row);
        self.held[usize::from(lane)] = Some(Held { second, head: row });
        Some(row)
    }

    /// Lane `lane` let go at `second`.
    pub fn release(&mut self, lane: u8, second: f64) {
        if let Some(h) = self.held.get_mut(usize::from(lane)).and_then(Option::take) {
            self.grow(lane, h, second);
        }
    }

    /// Every lane let go at `second`.
    pub fn release_all(&mut self, second: f64) {
        for lane in 0..self.lanes {
            self.release(lane, second);
        }
    }

    /// The music heard at `second`: the lanes held grow into holds.
    pub fn update(&mut self, second: f64) {
        for lane in 0..self.lanes {
            if let Some(h) = self.held[usize::from(lane)] {
                self.grow(lane, h, second);
            }
        }
    }

    /// Whether a selection's take is over at `second`: no press from then
    /// on would land in it.
    pub fn ended(&self, second: f64) -> bool {
        self.last.is_some_and(|l| {
            let t = self.timing.beat_at(second) * TICKS_PER_BEAT as f64;
            t >= (l.0 as f64) + self.step as f64 / 2.0
        })
    }

    /// A tap on `row` of `lane`, in place of a note recorded before on
    /// it or a hold over it (StepMania's `InputRecord`: no tap inside a
    /// hold).
    fn put(&mut self, lane: u8, row: Tick) {
        self.notes
            .retain(|n| !(n.lane == lane && n.tick <= row && n.end_tick() >= row));
        self.notes.push(Note::new(row, lane, NoteKind::Tap));
    }

    /// The note of a press held from `h` to `second`: a hold to the row
    /// then once held longer than [`HOLD_SECONDS`], else a tap.
    fn grow(&mut self, lane: u8, h: Held, second: f64) {
        let held = second - h.second > HOLD_SECONDS;
        let mut end = self.row(second).max(h.head + Tick(self.step));
        if let Some(last) = self.last {
            end = end.min(last);
        }
        let roll = self.roll;
        if let Some(n) = self
            .notes
            .iter_mut()
            .find(|n| n.lane == lane && n.tick == h.head)
        {
            if held {
                set_end(n, end, roll);
            } else {
                n.kind = NoteKind::Tap;
            }
        }
    }

    /// The presses' mean and standard deviation from their rows, ms.
    pub fn stats(&self) -> (Option<f64>, Option<f64>) {
        let n = self.offsets.len() as f64;
        if self.offsets.is_empty() {
            return (None, None);
        }
        let mean = self.offsets.iter().sum::<f64>() / n;
        let var = self.offsets.iter().map(|o| (o - mean).powi(2)).sum::<f64>() / n;
        (Some(mean * 1000.0), Some(var.sqrt() * 1000.0))
    }

    /// The take stopped at `second`: the lanes held let go then; its
    /// notes in order, the rows it recorded (the end not included) and
    /// the row it stopped on.
    fn finish(mut self, second: f64) -> (Vec<Note>, (Tick, Tick), Tick) {
        self.release_all(second);
        let stopped = self.row(second).max(self.start);
        let end = match self.last {
            Some(l) => l + Tick(1),
            None => {
                let notes = self.notes.iter().map(|n| n.end_tick() + Tick(1)).max();
                stopped.max(notes.unwrap_or(stopped))
            }
        };
        let cursor = match self.last {
            Some(l) => stopped.min(l),
            None => stopped,
        };
        let mut notes = std::mem::take(&mut self.notes);
        notes.sort_by_key(|n| (n.tick, n.lane));
        (notes, (self.start, end), cursor)
    }
}

/// `n` ends at `end`: a hold or a roll, or a tap when that is not after
/// its head.
fn set_end(n: &mut Note, end: Tick, roll: bool) {
    n.kind = match (end > n.tick, roll) {
        (true, false) => NoteKind::HoldHead { end },
        (true, true) => NoteKind::RollHead { end },
        (false, _) => NoteKind::Tap,
    };
}

impl Editor {
    /// Puts `take`, stopped at song second `stop`, in the chart as one
    /// change; the cursor on the row it stopped on.
    pub fn record(&mut self, take: Take, stop: f64) -> Recorded {
        let mode = take.mode;
        let (mean_ms, spread_ms) = take.stats();
        let presses = take.presses();
        let (notes, (from, to), cursor) = take.finish(stop);
        let added: Vec<Note> = match mode {
            Mode::Replace => notes,
            Mode::Add => notes.into_iter().filter_map(|n| self.fit(n)).collect(),
        };
        let count = added.len();
        match mode {
            Mode::Replace if to > from => {
                let moved = self.heads_in(from, to - Tick(1));
                self.replace(moved, added.clone());
            }
            _ => self.replace(Vec::new(), added.clone()),
        }
        self.set_cursor(cursor, false);
        Recorded {
            mode,
            added,
            notes: count,
            presses,
            mean_ms,
            spread_ms,
            span: (from, to.max(from)),
        }
    }

    /// Note `n` added to the chart as it is: left out on a cell taken; a
    /// hold or a roll over a later note's head ends a snap before it.
    fn fit(&self, mut n: Note) -> Option<Note> {
        if self.note_at(n.lane, n.tick).is_some() {
            return None;
        }
        if let Some(end) = n.kind.hold_end() {
            let next = self
                .chart()
                .notes
                .iter()
                .filter(|o| o.lane == n.lane && o.tick > n.tick && o.tick <= end)
                .map(|o| o.tick)
                .min();
            if let Some(next) = next {
                let roll = matches!(n.kind, NoteKind::RollHead { .. });
                set_end(&mut n, next - Tick(self.step()), roll);
            }
        }
        Some(n)
    }
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
