//! The chart editor's model (`docs/plans/editor.md`, step 2): a song being
//! edited, the chart shown, the cursor, the snap, the selection, the
//! clipboard and the edits with their undo and redo. Sans IO: the screen
//! draws it and turns input into its calls, the shells load, save, play
//! sound and start playtests.
//!
//! Every edit keeps the song as a file keeps it (`ddi_chart::formats::
//! sm_write`): notes sorted, one per cell, nothing inside a hold in its
//! lane, timing tidy, text trimmed. So what is saved reads back as it was
//! edited, and each edit is one [`Change`] whose inverse undoes it.
//!
//! As StepMania's editor, timing changes keep the notes on their beats
//! (a BPM change moves the later notes in time); [`Retime::KeepTimes`]
//! instead keeps them where they are heard, moving each to the tick
//! nearest its old time (re-syncing a chart).

mod change;
#[cfg(test)]
mod tests;

pub use change::{ChartInfo, SongInfo};

use change::{Change, chart_tags};
use ddi_chart::formats::sm_write;
use ddi_chart::{
    BpmSegment, Chart, ComboSegment, Difficulty, DisplayBpm, FakeSegment, Label, Layout, Note,
    NoteKind, ScrollSegment, Song, SourceFormat, SourceInfo, SpeedSegment, SpeedUnit, StopSegment,
    TICKS_PER_BEAT, Tick, TickCount, TimeSignature, TimingMap, WarpSegment,
};
use ddi_engine::transform::{Turn, lane_map};

/// Ticks in a measure as note data lays them out (4 beats).
pub const MEASURE: i64 = 4 * TICKS_PER_BEAT;

/// The snaps the cursor moves by, in notes per measure (4th to 192nd,
/// StepMania's `NoteType`s).
pub const SNAPS: [i64; 9] = [4, 8, 12, 16, 24, 32, 48, 64, 192];

/// Seconds a playtest from the cursor starts before it: time to see the
/// first notes come.
pub const PLAYTEST_LEAD: f64 = 2.0;

/// What a lane press places.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoteTool {
    /// A tap; held while the cursor moves, a hold.
    #[default]
    Tap,
    /// A tap; held while the cursor moves, a roll.
    Roll,
    Mine,
    Lift,
    Fake,
}

impl NoteTool {
    pub const ALL: [NoteTool; 5] = [
        NoteTool::Tap,
        NoteTool::Roll,
        NoteTool::Mine,
        NoteTool::Lift,
        NoteTool::Fake,
    ];

    fn single(self) -> NoteKind {
        match self {
            NoteTool::Tap | NoteTool::Roll => NoteKind::Tap,
            NoteTool::Mine => NoteKind::Mine,
            NoteTool::Lift => NoteKind::Lift,
            NoteTool::Fake => NoteKind::Fake,
        }
    }

    fn long(self, end: Tick) -> Option<NoteKind> {
        match self {
            NoteTool::Tap => Some(NoteKind::HoldHead { end }),
            NoteTool::Roll => Some(NoteKind::RollHead { end }),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            NoteTool::Tap => "Tap / hold",
            NoteTool::Roll => "Roll",
            NoteTool::Mine => "Mine",
            NoteTool::Lift => "Lift",
            NoteTool::Fake => "Fake",
        }
    }
}

/// What a timing change does to the notes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Retime {
    /// They stay on their beats (StepMania's editor).
    #[default]
    KeepBeats,
    /// They stay where they are heard, on the nearest tick.
    KeepTimes,
}

/// A timing segment's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SegmentKind {
    Bpm,
    Stop,
    Delay,
    Warp,
    Speed,
    Scroll,
    Fake,
    TimeSignature,
    TickCount,
    Combo,
    Label,
}

impl SegmentKind {
    pub const ALL: [SegmentKind; 11] = [
        SegmentKind::Bpm,
        SegmentKind::Stop,
        SegmentKind::Delay,
        SegmentKind::Warp,
        SegmentKind::Speed,
        SegmentKind::Scroll,
        SegmentKind::Fake,
        SegmentKind::TimeSignature,
        SegmentKind::TickCount,
        SegmentKind::Combo,
        SegmentKind::Label,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SegmentKind::Bpm => "BPM",
            SegmentKind::Stop => "Stop",
            SegmentKind::Delay => "Delay",
            SegmentKind::Warp => "Warp",
            SegmentKind::Speed => "Speed",
            SegmentKind::Scroll => "Scroll",
            SegmentKind::Fake => "Fake",
            SegmentKind::TimeSignature => "Time signature",
            SegmentKind::TickCount => "Tick count",
            SegmentKind::Combo => "Combo",
            SegmentKind::Label => "Label",
        }
    }
}

/// A timing segment's values, at a tick given apart.
#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Bpm(f64),
    /// Seconds.
    Stop(f64),
    /// Seconds.
    Delay(f64),
    /// Length.
    Warp(Tick),
    Speed {
        ratio: f64,
        delay: f64,
        unit: SpeedUnit,
    },
    Scroll(f64),
    /// Length.
    Fake(Tick),
    TimeSignature(u32, u32),
    TickCount(u32),
    Combo(u32, u32),
    Label(String),
}

impl Segment {
    pub fn kind(&self) -> SegmentKind {
        match self {
            Segment::Bpm(_) => SegmentKind::Bpm,
            Segment::Stop(_) => SegmentKind::Stop,
            Segment::Delay(_) => SegmentKind::Delay,
            Segment::Warp(_) => SegmentKind::Warp,
            Segment::Speed { .. } => SegmentKind::Speed,
            Segment::Scroll(_) => SegmentKind::Scroll,
            Segment::Fake(_) => SegmentKind::Fake,
            Segment::TimeSignature(..) => SegmentKind::TimeSignature,
            Segment::TickCount(_) => SegmentKind::TickCount,
            Segment::Combo(..) => SegmentKind::Combo,
            Segment::Label(_) => SegmentKind::Label,
        }
    }

    /// Its values as a file writes them, for the marker's text.
    pub fn describe(&self) -> String {
        let n = sm_write::number;
        match self {
            Segment::Bpm(v) => format!("{} BPM", trim_number(*v)),
            Segment::Stop(v) => format!("stop {} s", trim_number(*v)),
            Segment::Delay(v) => format!("delay {} s", trim_number(*v)),
            Segment::Warp(l) => format!("warp {} beats", trim_number(l.beat())),
            Segment::Speed { ratio, delay, unit } => format!(
                "speed {}× over {} {}",
                trim_number(*ratio),
                n(*delay),
                match unit {
                    SpeedUnit::Beats => "beats",
                    SpeedUnit::Seconds => "s",
                }
            ),
            Segment::Scroll(v) => format!("scroll {}×", trim_number(*v)),
            Segment::Fake(l) => format!("fake {} beats", trim_number(l.beat())),
            Segment::TimeSignature(a, b) => format!("{a}/{b}"),
            Segment::TickCount(t) => format!("{t} ticks"),
            Segment::Combo(a, b) => format!("combo {a}/{b}"),
            Segment::Label(s) => format!("“{s}”"),
        }
    }
}

/// A number with at most three decimals, for showing.
pub fn trim_number(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Notes cut or copied: ticks from the selection's start, and its length.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub notes: Vec<Note>,
    pub length: Tick,
}

/// A lane held down: what its press did, so moving the cursor makes the
/// note placed a hold.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    lane: u8,
    start: Tick,
    /// The serial of the change the press made, when it placed a note.
    placed: Option<u64>,
}

/// A song being edited.
#[derive(Clone, Debug)]
pub struct Editor {
    song: Song,
    chart: usize,
    cursor: Tick,
    snap: usize,
    anchor: Option<Tick>,
    tool: NoteTool,
    clip: Option<Clip>,
    undo: Vec<(u64, Change)>,
    redo: Vec<(u64, Change)>,
    serial: u64,
    /// The serial on top of the undo stack when last saved; `Some(0)`
    /// (no change has it) for a song never saved.
    saved: Option<u64>,
    held: Option<Held>,
}

impl Editor {
    /// Opens `song` at chart `chart`. Generated charts are left out (the
    /// library makes them again); a song without charts gets one.
    pub fn open(mut song: Song, chart: usize) -> Editor {
        song.charts.retain(|c| c.derivation.is_none());
        let mut saved = None;
        if song.charts.is_empty() {
            song.charts
                .push(empty_chart("dance-single", Difficulty::Medium));
            saved = Some(0);
        }
        let chart = chart.min(song.charts.len() - 1);
        Editor {
            song,
            chart,
            cursor: Tick::ZERO,
            snap: 3,
            anchor: None,
            tool: NoteTool::Tap,
            clip: None,
            undo: Vec::new(),
            redo: Vec::new(),
            serial: 0,
            saved,
            held: None,
        }
    }

    /// A new song for a music file: 120 BPM from its start, one empty
    /// dance-single chart. Not saved yet.
    pub fn new_song(music: Option<String>, title: &str) -> Editor {
        let song = Song {
            title: title.trim().to_string(),
            subtitle: String::new(),
            artist: String::new(),
            title_translit: String::new(),
            subtitle_translit: String::new(),
            artist_translit: String::new(),
            genre: String::new(),
            credit: String::new(),
            music,
            preview_start: 0.0,
            preview_length: 12.0,
            banner: None,
            background: None,
            jacket: None,
            cd_title: None,
            timing: TimingMap::constant(120.0, 0.0),
            display_bpm: DisplayBpm::Actual,
            charts: Vec::new(),
            effects: Vec::new(),
            keysounds: Vec::new(),
            layouts: Vec::new(),
            source: SourceInfo {
                format: SourceFormat::Ssc {
                    version: sm_write::SSC_VERSION,
                },
                unknown_tags: Vec::new(),
            },
        };
        Editor::open(song, 0)
    }

    pub fn song(&self) -> &Song {
        &self.song
    }

    pub fn chart_index(&self) -> usize {
        self.chart
    }

    pub fn chart(&self) -> &Chart {
        &self.song.charts[self.chart]
    }

    /// The chart's layout: its steps type's, else as many lanes as a file
    /// writes for it.
    pub fn layout(&self) -> Layout {
        self.song
            .layout_of(self.chart())
            .unwrap_or_else(|| Layout::generic(sm_write::lane_count(self.chart())))
    }

    pub fn lanes(&self) -> u8 {
        self.layout().lane_count() as u8
    }

    /// The timing the chart is edited on: its own, else the song's.
    pub fn timing(&self) -> &TimingMap {
        self.chart().timing(&self.song)
    }

    pub fn has_own_timing(&self) -> bool {
        self.chart().timing.is_some()
    }

    pub fn cursor(&self) -> Tick {
        self.cursor
    }

    pub fn tool(&self) -> NoteTool {
        self.tool
    }

    pub fn set_tool(&mut self, tool: NoteTool) {
        self.tool = tool;
    }

    pub fn clip(&self) -> Option<&Clip> {
        self.clip.as_ref()
    }

    /// Notes per measure of the snap.
    pub fn snap(&self) -> i64 {
        SNAPS[self.snap]
    }

    /// The snap's step in ticks.
    pub fn step(&self) -> i64 {
        MEASURE / self.snap()
    }

    pub fn set_snap(&mut self, notes_per_measure: i64) {
        if let Some(i) = SNAPS.iter().position(|&s| s == notes_per_measure) {
            self.snap = i;
        }
    }

    /// A finer (`by > 0`) or coarser snap.
    pub fn change_snap(&mut self, by: i32) {
        self.snap = (self.snap as i32 + by).clamp(0, SNAPS.len() as i32 - 1) as usize;
    }

    pub fn is_dirty(&self) -> bool {
        self.undo.last().map(|(s, _)| *s) != self.saved
    }

    /// The song as it is now has been saved.
    pub fn mark_saved(&mut self) {
        self.saved = self.revision();
    }

    /// Which edit the song is at (`None`: none made): what a save that
    /// ends later marks as saved ([`Editor::mark_saved_at`]).
    pub fn revision(&self) -> Option<u64> {
        self.undo.last().map(|(s, _)| *s)
    }

    /// The song as it was at `revision` has been saved.
    pub fn mark_saved_at(&mut self, revision: Option<u64>) {
        self.saved = revision;
    }

    /// Notes of the chart shown.
    pub fn note_count(&self) -> usize {
        self.chart().notes.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ---------------------------------------------------------------------
    // Cursor and selection.
    // ---------------------------------------------------------------------

    /// The song second the cursor is at (before a delay on its row, as it
    /// is reached).
    pub fn cursor_seconds(&self) -> f64 {
        self.timing().seconds_before_delay(self.cursor)
    }

    /// The last tick anything is on: the last note's end, at least a
    /// measure.
    pub fn end(&self) -> Tick {
        let notes = self.chart().notes.iter().map(Note::end_tick).max();
        notes.unwrap_or(Tick::ZERO).max(Tick(MEASURE))
    }

    /// Moves the cursor to `tick` (not before the start); `extend` keeps
    /// or starts a selection from where it was, else it ends one.
    pub fn set_cursor(&mut self, tick: Tick, extend: bool) {
        let tick = tick.max(Tick::ZERO);
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = tick;
        self.extend_held();
    }

    /// Moves by `steps` snaps, to the snap's grid.
    pub fn move_snaps(&mut self, steps: i64, extend: bool) {
        let step = self.step();
        let on_grid = self.cursor.0.div_euclid(step) * step;
        // Back from between two lines, the first step is to the line
        // before.
        let target = if steps < 0 && on_grid != self.cursor.0 {
            on_grid + (steps + 1) * step
        } else {
            on_grid + steps * step
        };
        self.set_cursor(Tick(target), extend);
    }

    /// Moves by `measures` measures, to the measure grid.
    pub fn move_measures(&mut self, measures: i64, extend: bool) {
        let on_grid = self.cursor.0.div_euclid(MEASURE) * MEASURE;
        let target = if measures < 0 && on_grid != self.cursor.0 {
            on_grid + (measures + 1) * MEASURE
        } else {
            on_grid + measures * MEASURE
        };
        self.set_cursor(Tick(target), extend);
    }

    /// The tick nearest `seconds` on the snap's grid (where a scrub or a
    /// tap on the field lands).
    pub fn tick_at_seconds(&self, seconds: f64) -> Tick {
        let beat = self.timing().beat_at(seconds);
        self.snap_tick(Tick::from_beat_f64(beat))
    }

    /// `tick` on the snap's grid, the nearest.
    pub fn snap_tick(&self, tick: Tick) -> Tick {
        let step = self.step();
        Tick(((tick.0 as f64 / step as f64).round() as i64 * step).max(0))
    }

    /// The selected rows, both ends included.
    pub fn selection(&self) -> Option<(Tick, Tick)> {
        let a = self.anchor?;
        Some((a.min(self.cursor), a.max(self.cursor)))
    }

    /// Selects from `from` to `to`, the cursor at `to`.
    pub fn select(&mut self, from: Tick, to: Tick) {
        self.anchor = Some(from.max(Tick::ZERO));
        self.cursor = to.max(Tick::ZERO);
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    /// Selects every note of the chart.
    pub fn select_all(&mut self) {
        let last = self.chart().notes.iter().map(|n| n.tick).max();
        self.select(Tick::ZERO, last.unwrap_or(Tick::ZERO));
    }

    /// Shows chart `index` (cursor and selection kept).
    pub fn select_chart(&mut self, index: usize) {
        if index < self.song.charts.len() {
            self.chart = index;
            self.held = None;
        }
    }

    // ---------------------------------------------------------------------
    // Notes.
    // ---------------------------------------------------------------------

    /// Notes of the chart in `lane` whose span (head to tail) meets
    /// `from..=to`.
    fn covering(&self, lane: u8, from: Tick, to: Tick) -> Vec<Note> {
        self.chart()
            .notes
            .iter()
            .filter(|n| n.lane == lane && n.tick <= to && n.end_tick() >= from)
            .cloned()
            .collect()
    }

    /// The note at a cell, or whose hold covers it.
    pub fn note_at(&self, lane: u8, tick: Tick) -> Option<&Note> {
        self.chart()
            .notes
            .iter()
            .find(|n| n.lane == lane && n.tick <= tick && n.end_tick() >= tick)
    }

    /// Notes whose heads are in `from..=to`.
    fn heads_in(&self, from: Tick, to: Tick) -> Vec<Note> {
        self.chart()
            .notes
            .iter()
            .filter(|n| n.tick >= from && n.tick <= to)
            .cloned()
            .collect()
    }

    /// A lane pressed at the cursor: takes away the note there (a hold's
    /// whole span counts), else places the tool's. Held while the cursor
    /// moves, a placed tap grows into a hold or a roll
    /// ([`Editor::release`] ends it).
    pub fn press(&mut self, lane: u8) {
        let placed = self.toggle_at(lane, self.cursor);
        self.held = Some(Held {
            lane,
            start: self.cursor,
            placed: placed.then_some(self.serial),
        });
    }

    pub fn release(&mut self, lane: u8) {
        if self.held.is_some_and(|h| h.lane == lane) {
            self.held = None;
        }
    }

    /// Takes away the note covering a cell, else places the tool's there.
    /// Returns whether it placed one.
    pub fn toggle_at(&mut self, lane: u8, tick: Tick) -> bool {
        if lane >= self.lanes() || tick < Tick::ZERO {
            return false;
        }
        let cover = self.covering(lane, tick, tick);
        if !cover.is_empty() {
            self.commit_notes(cover, Vec::new());
            return false;
        }
        let note = Note::new(tick, lane, self.tool.single());
        self.commit_notes(Vec::new(), vec![note]);
        true
    }

    /// Places the tool's note from `a` to `b` in `lane`: a hold or a roll
    /// when they differ and the tool has one, replacing what is there.
    pub fn place_long(&mut self, lane: u8, a: Tick, b: Tick) {
        if lane >= self.lanes() {
            return;
        }
        let (from, to) = (a.min(b).max(Tick::ZERO), a.max(b).max(Tick::ZERO));
        let kind = match self.tool.long(to) {
            Some(kind) if to > from => kind,
            _ => self.tool.single(),
        };
        let note = Note::new(from, lane, kind);
        let removed = self.covering(lane, from, note.end_tick());
        self.commit_notes(removed, vec![note]);
    }

    /// The cursor moved while a lane is held: the note it placed now runs
    /// from the press to the cursor.
    fn extend_held(&mut self) {
        let Some(held) = self.held else {
            return;
        };
        let Some(serial) = held.placed else {
            return;
        };
        if self.undo.last().map(|(s, _)| *s) != Some(serial) || self.tool.long(held.start).is_none()
        {
            self.held = None;
            return;
        }
        let (_, change) = self.undo.pop().expect("checked above");
        change.inverse().apply(&mut self.song);
        self.place_long(held.lane, held.start, self.cursor);
        self.held = Some(Held {
            placed: Some(self.serial),
            ..held
        });
    }

    /// Deletes the selected notes (those whose heads are in it).
    pub fn delete_selection(&mut self) {
        if let Some((from, to)) = self.selection() {
            let removed = self.heads_in(from, to);
            self.commit_notes(removed, Vec::new());
        }
    }

    /// Copies the selected notes.
    pub fn copy(&mut self) {
        if let Some((from, to)) = self.selection() {
            let notes = self
                .heads_in(from, to)
                .into_iter()
                .map(|n| shifted(&n, Tick::ZERO - from))
                .collect();
            self.clip = Some(Clip {
                notes,
                length: to - from,
            });
        }
    }

    pub fn cut(&mut self) {
        self.copy();
        self.delete_selection();
    }

    /// Pastes the clipboard at the cursor, replacing the notes in the rows
    /// it covers.
    pub fn paste(&mut self) {
        let Some(clip) = self.clip.clone() else {
            return;
        };
        let at = self.cursor;
        let lanes = self.lanes();
        let added: Vec<Note> = clip
            .notes
            .iter()
            .filter(|n| n.lane < lanes)
            .map(|n| shifted(n, at))
            .collect();
        let mut removed = self.heads_in(at, at + clip.length);
        for n in &added {
            for c in self.covering(n.lane, n.tick, n.end_tick()) {
                if !removed.contains(&c) {
                    removed.push(c);
                }
            }
        }
        self.commit_notes(removed, added);
    }

    /// Moves the selected notes (and the selection) by `steps` snaps,
    /// replacing what they land on; not before the start.
    pub fn shift_selection(&mut self, steps: i64) {
        let Some((from, to)) = self.selection() else {
            return;
        };
        let by = Tick(steps * self.step());
        let moved = self.heads_in(from, to);
        if (from + by) < Tick::ZERO || moved.is_empty() {
            return;
        }
        let added: Vec<Note> = moved.iter().map(|n| shifted(n, by)).collect();
        self.replace(moved, added);
        self.anchor = self.anchor.map(|a| a + by);
        self.cursor = self.cursor + by;
    }

    /// Turns the selected notes (mirror, left or right, as the options'
    /// turns map lanes, `ddi_engine::transform`).
    pub fn turn_selection(&mut self, turn: Turn) {
        let Some((from, to)) = self.selection() else {
            return;
        };
        let map = lane_map(&self.layout(), turn, 0);
        // `map[new] = old`: where each old lane goes.
        let mut to_lane = vec![0u8; map.len()];
        for (new, &old) in map.iter().enumerate() {
            to_lane[old as usize] = new as u8;
        }
        let moved = self.heads_in(from, to);
        let added = moved
            .iter()
            .map(|n| Note {
                lane: to_lane.get(n.lane as usize).copied().unwrap_or(n.lane),
                ..n.clone()
            })
            .collect();
        self.replace(moved, added);
    }

    /// Takes `moved` out and puts `added` in, with whatever `added` lands
    /// on.
    fn replace(&mut self, moved: Vec<Note>, added: Vec<Note>) {
        let mut removed = moved;
        for n in &added {
            for c in self.covering(n.lane, n.tick, n.end_tick()) {
                if !removed.contains(&c) {
                    removed.push(c);
                }
            }
        }
        self.commit_notes(removed, added);
    }

    fn commit_notes(&mut self, removed: Vec<Note>, added: Vec<Note>) {
        if removed == added {
            return;
        }
        self.commit(Change::Notes {
            chart: self.chart,
            removed,
            added,
        });
    }

    // ---------------------------------------------------------------------
    // Timing.
    // ---------------------------------------------------------------------

    /// The segments on `tick` of the timing the chart is edited on.
    pub fn segments_at(&self, tick: Tick) -> Vec<Segment> {
        segments_at(self.timing(), tick)
    }

    /// Every segment's tick and values, in order (the markers).
    pub fn segments(&self) -> Vec<(Tick, Segment)> {
        let t = self.timing();
        let mut ticks: Vec<Tick> = all_ticks(t);
        ticks.sort();
        ticks.dedup();
        ticks
            .into_iter()
            .flat_map(|tick| segments_at(t, tick).into_iter().map(move |s| (tick, s)))
            .collect()
    }

    /// Sets a segment at `tick`, replacing one of its kind there. Values
    /// a file cannot hold are refused (a BPM not above 0, a time
    /// signature with a 0); a stop, delay, warp or fake of 0 takes it
    /// away. Returns whether it was taken.
    pub fn set_segment(&mut self, tick: Tick, segment: Segment, retime: Retime) -> bool {
        let Some(segment) = tidy_segment(segment).filter(|_| tick >= Tick::ZERO) else {
            return false;
        };
        self.edit_timing(retime, |t| {
            remove_segment(t, segment.kind(), tick);
            put_segment(t, tick, segment);
        });
        true
    }

    /// Takes away the segment of `kind` at `tick` (not the first BPM).
    pub fn remove_segment(&mut self, kind: SegmentKind, tick: Tick, retime: Retime) {
        if kind == SegmentKind::Bpm && tick == Tick::ZERO {
            return;
        }
        self.edit_timing(retime, |t| remove_segment(t, kind, tick));
    }

    /// Sets the offset (`time(beat 0) = -offset`).
    pub fn set_offset(&mut self, seconds: f64, retime: Retime) {
        if seconds.is_finite() {
            self.edit_timing(retime, |t| t.offset_seconds = seconds);
        }
    }

    /// Gives the chart its own timing (a copy of the song's), or puts it
    /// back on the song's (its own is dropped; notes keep their beats).
    pub fn set_own_timing(&mut self, own: bool) {
        if own == self.has_own_timing() {
            return;
        }
        let before = self.chart().timing.clone().map(Box::new);
        let after = own.then(|| Box::new(self.song.timing.clone()));
        self.commit(Change::Timing {
            chart: Some(self.chart),
            before,
            after,
        });
    }

    fn edit_timing(&mut self, retime: Retime, edit: impl FnOnce(&mut TimingMap)) {
        let before = self.timing().clone();
        let mut after = before.clone();
        edit(&mut after);
        tidy_timing(&mut after);
        if after == before {
            return;
        }
        let own = self.has_own_timing();
        let timing = Change::Timing {
            chart: own.then_some(self.chart),
            before: Some(Box::new(before.clone())),
            after: Some(Box::new(after.clone())),
        };
        let change = match retime {
            Retime::KeepBeats => timing,
            Retime::KeepTimes => {
                // Every chart on this timing keeps its notes' times.
                let mut changes = vec![timing];
                for (i, chart) in self.song.charts.iter().enumerate() {
                    let on_it = if own {
                        i == self.chart
                    } else {
                        chart.timing.is_none()
                    };
                    if !on_it {
                        continue;
                    }
                    let moved = retimed(&chart.notes, &before, &after);
                    if moved != chart.notes {
                        changes.push(Change::Notes {
                            chart: i,
                            removed: chart.notes.clone(),
                            added: moved,
                        });
                    }
                }
                Change::Batch(changes)
            }
        };
        self.commit(change);
    }

    // ---------------------------------------------------------------------
    // Song and charts.
    // ---------------------------------------------------------------------

    pub fn song_info(&self) -> SongInfo {
        SongInfo::of(&self.song)
    }

    pub fn set_song_info(&mut self, info: SongInfo) {
        let before = SongInfo::of(&self.song);
        let after = info.tidy();
        if after != before {
            self.commit(Change::Song {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
    }

    pub fn chart_info(&self) -> ChartInfo {
        ChartInfo::of(self.chart())
    }

    pub fn set_chart_info(&mut self, info: ChartInfo) {
        let before = ChartInfo::of(self.chart());
        let after = info.tidy();
        if after != before {
            self.commit(Change::Chart {
                chart: self.chart,
                before: Box::new(before),
                after: Box::new(after),
            });
        }
    }

    /// Adds an empty chart and shows it.
    pub fn add_chart(&mut self, layout: &str, difficulty: Difficulty) {
        let index = self.song.charts.len();
        self.commit(Change::InsertChart {
            index,
            chart: Box::new(empty_chart(layout, difficulty)),
            tags: Vec::new(),
        });
        self.chart = index;
    }

    /// Takes away the chart shown (not the last one).
    pub fn remove_chart(&mut self) {
        if self.song.charts.len() < 2 {
            return;
        }
        let index = self.chart;
        self.commit(Change::RemoveChart {
            index,
            chart: Box::new(self.chart().clone()),
            tags: chart_tags(&self.song, index),
        });
    }

    // ---------------------------------------------------------------------
    // Undo.
    // ---------------------------------------------------------------------

    fn commit(&mut self, change: Change) {
        change.apply(&mut self.song);
        self.serial += 1;
        self.undo.push((self.serial, change));
        self.redo.clear();
        self.fix_chart();
    }

    pub fn undo(&mut self) {
        self.held = None;
        if let Some((serial, change)) = self.undo.pop() {
            change.inverse().apply(&mut self.song);
            self.redo.push((serial, change));
            self.fix_chart();
        }
    }

    pub fn redo(&mut self) {
        self.held = None;
        if let Some((serial, change)) = self.redo.pop() {
            change.apply(&mut self.song);
            self.undo.push((serial, change));
            self.fix_chart();
        }
    }

    fn fix_chart(&mut self) {
        self.chart = self.chart.min(self.song.charts.len() - 1);
    }

    // ---------------------------------------------------------------------
    // Playing.
    // ---------------------------------------------------------------------

    /// What a playtest plays: the song as edited, the chart shown, and
    /// the second to start at ([`PLAYTEST_LEAD`] before the cursor's, or
    /// the start).
    pub fn playtest(&self, from_cursor: bool) -> (Song, usize, f64) {
        let start = if from_cursor {
            (self.cursor_seconds() - PLAYTEST_LEAD).max(0.0)
        } else {
            0.0
        };
        (self.song.clone(), self.chart, start)
    }
}

fn empty_chart(layout: &str, difficulty: Difficulty) -> Chart {
    Chart {
        derivation: None,
        layout: layout.to_string(),
        difficulty,
        meter: 1,
        name: String::new(),
        description: String::new(),
        credit: String::new(),
        notes: Vec::new(),
        timing: None,
        display_bpm: None,
        danoni: None,
    }
}

fn shifted(n: &Note, by: Tick) -> Note {
    let kind = match n.kind {
        NoteKind::HoldHead { end } => NoteKind::HoldHead { end: end + by },
        NoteKind::RollHead { end } => NoteKind::RollHead { end: end + by },
        k => k,
    };
    Note {
        tick: n.tick + by,
        kind,
        ..n.clone()
    }
}

/// `notes` moved from `before`'s timing to the ticks nearest their times
/// on `after`'s; of notes landing on one cell or inside a hold, the first
/// stays.
fn retimed(notes: &[Note], before: &TimingMap, after: &TimingMap) -> Vec<Note> {
    let at = |tick: Tick| {
        Tick::from_beat_f64(after.beat_at(before.seconds_before_delay(tick))).max(Tick::ZERO)
    };
    let mut moved: Vec<Note> = notes
        .iter()
        .map(|n| {
            let tick = at(n.tick);
            let kind = match n.kind {
                NoteKind::HoldHead { end } => NoteKind::HoldHead {
                    end: at(end).max(tick + Tick(1)),
                },
                NoteKind::RollHead { end } => NoteKind::RollHead {
                    end: at(end).max(tick + Tick(1)),
                },
                k => k,
            };
            Note {
                tick,
                kind,
                ..n.clone()
            }
        })
        .collect();
    moved.sort_by_key(|n| (n.tick, n.lane));
    let mut out: Vec<Note> = Vec::with_capacity(moved.len());
    for n in moved {
        let clear = out
            .iter()
            .rev()
            .filter(|m| m.lane == n.lane)
            .all(|m| m.end_tick() < n.tick);
        if clear {
            out.push(n);
        }
    }
    out
}

fn all_ticks(t: &TimingMap) -> Vec<Tick> {
    let mut v = Vec::new();
    v.extend(t.bpms.iter().map(|s| s.tick));
    v.extend(t.stops.iter().map(|s| s.tick));
    v.extend(t.delays.iter().map(|s| s.tick));
    v.extend(t.warps.iter().map(|s| s.tick));
    v.extend(t.speeds.iter().map(|s| s.tick));
    v.extend(t.scrolls.iter().map(|s| s.tick));
    v.extend(t.fakes.iter().map(|s| s.tick));
    v.extend(t.time_signatures.iter().map(|s| s.tick));
    v.extend(t.tick_counts.iter().map(|s| s.tick));
    v.extend(t.combos.iter().map(|s| s.tick));
    v.extend(t.labels.iter().map(|s| s.tick));
    v
}

/// The segments of a map on `tick`, in [`SegmentKind::ALL`]'s order.
pub fn segments_at(t: &TimingMap, tick: Tick) -> Vec<Segment> {
    let mut out = Vec::new();
    out.extend(
        t.bpms
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::Bpm(s.bpm)),
    );
    out.extend(
        t.stops
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::Stop(s.seconds)),
    );
    out.extend(
        t.delays
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::Delay(s.seconds)),
    );
    out.extend(
        t.warps
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::Warp(s.length)),
    );
    out.extend(
        t.speeds
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::Speed {
                ratio: s.ratio,
                delay: s.delay,
                unit: s.unit,
            }),
    );
    out.extend(
        t.scrolls
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::Scroll(s.ratio)),
    );
    out.extend(
        t.fakes
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::Fake(s.length)),
    );
    out.extend(
        t.time_signatures
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::TimeSignature(s.numerator, s.denominator)),
    );
    out.extend(
        t.tick_counts
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::TickCount(s.ticks_per_beat)),
    );
    out.extend(
        t.combos
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::Combo(s.combo, s.miss_combo)),
    );
    out.extend(
        t.labels
            .iter()
            .filter(|s| s.tick == tick)
            .map(|s| Segment::Label(s.text.clone())),
    );
    out
}

/// A segment as a file holds it, `None` when refused.
fn tidy_segment(segment: Segment) -> Option<Segment> {
    let ok = |v: f64| v.is_finite();
    Some(match segment {
        Segment::Bpm(v) if !ok(v) || v <= 0.0 || v > 9_999_999.0 => return None,
        Segment::Stop(v) | Segment::Delay(v) if !ok(v) => return None,
        Segment::Speed { ratio, delay, .. } if !ok(ratio) || !ok(delay) || delay < 0.0 => {
            return None;
        }
        Segment::Scroll(v) if !ok(v) => return None,
        Segment::TimeSignature(a, b) if a == 0 || b == 0 => return None,
        Segment::TickCount(t) => Segment::TickCount(t.min(TICKS_PER_BEAT as u32)),
        Segment::Label(s) => Segment::Label(s.replace([',', '='], " ").trim().to_string()),
        other => other,
    })
}

fn put_segment(t: &mut TimingMap, tick: Tick, segment: Segment) {
    match segment {
        Segment::Bpm(bpm) => t.bpms.push(BpmSegment { tick, bpm }),
        Segment::Stop(seconds) => t.stops.push(StopSegment { tick, seconds }),
        Segment::Delay(seconds) => t.delays.push(StopSegment { tick, seconds }),
        Segment::Warp(length) => t.warps.push(WarpSegment { tick, length }),
        Segment::Speed { ratio, delay, unit } => t.speeds.push(SpeedSegment {
            tick,
            ratio,
            delay,
            unit,
        }),
        Segment::Scroll(ratio) => t.scrolls.push(ScrollSegment { tick, ratio }),
        Segment::Fake(length) => t.fakes.push(FakeSegment { tick, length }),
        Segment::TimeSignature(numerator, denominator) => t.time_signatures.push(TimeSignature {
            tick,
            numerator,
            denominator,
        }),
        Segment::TickCount(ticks_per_beat) => t.tick_counts.push(TickCount {
            tick,
            ticks_per_beat,
        }),
        Segment::Combo(combo, miss_combo) => t.combos.push(ComboSegment {
            tick,
            combo,
            miss_combo,
        }),
        Segment::Label(text) => t.labels.push(Label { tick, text }),
    }
}

fn remove_segment(t: &mut TimingMap, kind: SegmentKind, tick: Tick) {
    match kind {
        SegmentKind::Bpm => t.bpms.retain(|s| s.tick != tick),
        SegmentKind::Stop => t.stops.retain(|s| s.tick != tick),
        SegmentKind::Delay => t.delays.retain(|s| s.tick != tick),
        SegmentKind::Warp => t.warps.retain(|s| s.tick != tick),
        SegmentKind::Speed => t.speeds.retain(|s| s.tick != tick),
        SegmentKind::Scroll => t.scrolls.retain(|s| s.tick != tick),
        SegmentKind::Fake => t.fakes.retain(|s| s.tick != tick),
        SegmentKind::TimeSignature => t.time_signatures.retain(|s| s.tick != tick),
        SegmentKind::TickCount => t.tick_counts.retain(|s| s.tick != tick),
        SegmentKind::Combo => t.combos.retain(|s| s.tick != tick),
        SegmentKind::Label => t.labels.retain(|s| s.tick != tick),
    }
}

/// A map as the reader makes it: tidy, and with a 4/4 from the start
/// when its first time signature is later.
fn tidy_timing(t: &mut TimingMap) {
    t.tidy();
    if t.time_signatures
        .first()
        .is_some_and(|s| s.tick > Tick::ZERO)
    {
        t.time_signatures.insert(
            0,
            TimeSignature {
                tick: Tick::ZERO,
                numerator: 4,
                denominator: 4,
            },
        );
    }
}
