//! The chart editor's field in the player's noteskin
//! (`docs/plans/editor.md`, step 3): the shell draws the notes as play
//! draws them, paused at the cursor ([`FieldPicture`], drawn by
//! `ddi_render::editor_field` into the texture [`SOURCE`]), and the
//! screen draws over it what must read exactly: the grid, the rows'
//! ticks, the cursor line, the tails, the selection, the markers.
//!
//! Both are placed by one [`Mapping`] from ticks to the screen's y: in
//! the edit view a beat is the same height everywhere (the zoom); in the
//! play view a note is where play draws it at that moment
//! ([`ScrollState`]: the player's speed, BPM changes, stops, scrolls and
//! speed segments, reverse), so what is seen is what is played.

use ddi_chart::{Chart, NoteKind, TICKS_PER_BEAT, Tick, TimingMap};
use ddi_engine::appearance::Appearance;
use ddi_engine::frame::{
    Frame, HoldKind, NoteSprite, ReceptorState, SkinEvent, SkinEventKind, SkinNote, SpriteKind,
};
use ddi_engine::rules::Judgement;
use ddi_engine::scroll::{ScrollOptions, ScrollState};

/// The texture source the shell draws the field into.
pub const SOURCE: &str = "editor-field";

/// How rows are spaced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldView {
    /// Evenly by beat at the editor's zoom (StepMania's editor).
    #[default]
    Edit,
    /// As play draws them at the cursor's moment.
    Play,
}

impl FieldView {
    pub fn name(self) -> &'static str {
        match self {
            FieldView::Edit => "edit",
            FieldView::Play => "play",
        }
    }
}

/// What a shell that draws the field in the player's skin tells the
/// editor ([`super::editor::EditorHost::skin_field`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkinField {
    /// The player's scroll (speed, reverse, Boost and the like), as a
    /// play of the chart takes it.
    pub scroll: ScrollOptions,
}

/// What the shell draws: the frame of a play paused at the cursor, in a
/// field of its own geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldPicture {
    /// Where it shows, in the surface's physical pixels `[x, y, w, h]`
    /// (whole pixels): the texture's size.
    pub rect: [f32; 4],
    /// The chart's layout id.
    pub layout: String,
    /// In the texture's pixels: a lane's width (the arrow's size), the
    /// first lane's left edge, the receptors' line.
    pub arrow: f32,
    pub left: f32,
    pub receptor_y: f32,
    /// Later notes above the line (the play view under Reverse).
    pub reverse: bool,
    /// The notes in view, their `y` in arrow heights from the line.
    pub frame: Frame,
    /// The music plays (listening): the skin's clock runs, and
    /// [`FieldPicture::events`] are its notes passing the line. Else the
    /// skin is held at rest.
    pub running: bool,
    pub events: Vec<SkinEvent>,
    /// Under the field: encoded RGBA.
    pub background: [f32; 4],
}

impl FieldPicture {
    /// The texture's y of a note `arrows` arrow heights from the line.
    pub fn y(&self, arrows: f32) -> f32 {
        let dir = if self.reverse { -1.0 } else { 1.0 };
        self.receptor_y + arrows * self.arrow * dir
    }

    /// The texture's x of a lane's middle, its layout column `column`.
    pub fn lane_x(&self, column: f32) -> f32 {
        self.left + (column + 0.5) * self.arrow
    }
}

/// The edit view's least height of a beat for the snap's rows to be
/// a third of a note (`lane_w`) apart: the art keeps its size, the zoom
/// goes up.
pub fn least_beat_height(step: i64, lane_w: f32) -> f32 {
    lane_w / 3.0 * TICKS_PER_BEAT as f32 / step.max(1) as f32
}

/// The play view's furthest rows from the cursor looked at (beats): what
/// a slow scroll shows further is not drawn.
const PLAY_REACH: i64 = 64;
/// The most rows the play view looks at.
const PLAY_ROWS: i64 = 4096;

/// Ticks to the screen's y (virtual pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    /// The cursor's line (the receptors').
    pub line_y: f32,
    /// The tick at the line.
    pub shown: f64,
    /// The edit view's beat height.
    pub beat_h: f32,
    pub lane_w: f32,
    /// The play view: the scroll at the moment shown, and +1 (later notes
    /// below) or −1 (above, Reverse).
    pub play: Option<(ScrollState, f32)>,
}

/// The second a play is at with `shown` at the receptors: a row's own
/// (before its delay, as listening starts there), or between rows the
/// beat's.
pub fn shown_seconds(timing: &TimingMap, shown: f64) -> f64 {
    if shown.fract() == 0.0 {
        timing.seconds_before_delay(Tick(shown as i64))
    } else {
        timing.seconds_at_beat(shown / TICKS_PER_BEAT as f64)
    }
}

impl Mapping {
    /// The edit view.
    pub fn edit(line_y: f32, shown: f64, beat_h: f32, lane_w: f32) -> Mapping {
        Mapping {
            line_y,
            shown,
            beat_h,
            lane_w,
            play: None,
        }
    }

    /// The play view in a field `[top, bottom]`: at `seconds` (the music
    /// heard, else [`shown_seconds`]), the line where play has it (from
    /// the bottom under Reverse, as far as `line_y` is from the top).
    #[allow(clippy::too_many_arguments)]
    pub fn play(
        scroll: ScrollOptions,
        timing: &TimingMap,
        seconds: f64,
        field: [f32; 2],
        line_y: f32,
        shown: f64,
        beat_h: f32,
        lane_w: f32,
    ) -> Mapping {
        let (line_y, dir) = if scroll.reverse {
            (field[0] + field[1] - line_y, -1.0)
        } else {
            (line_y, 1.0)
        };
        Mapping {
            line_y,
            shown,
            beat_h,
            lane_w,
            play: Some((ScrollState::at(scroll, timing, seconds), dir)),
        }
    }

    /// +1 when later notes are below the line.
    pub fn dir(&self) -> f32 {
        self.play.map_or(1.0, |p| p.1)
    }

    /// Arrow heights from the line to `tick`, positive for later notes
    /// (a play's note `y`).
    pub fn arrows(&self, timing: &TimingMap, tick: Tick) -> f32 {
        match &self.play {
            Some((s, _)) => s.y(timing, tick, timing.seconds_at(tick)),
            None => {
                ((tick.0 as f64 - self.shown) / TICKS_PER_BEAT as f64) as f32 * self.beat_h
                    / self.lane_w
            }
        }
    }

    /// The screen's y of `tick` (fractional in the edit view).
    pub fn y(&self, timing: &TimingMap, tick: f64) -> f32 {
        match &self.play {
            Some(_) => {
                self.line_y
                    + self.arrows(timing, Tick(tick.round() as i64)) * self.lane_w * self.dir()
            }
            None => {
                self.line_y + ((tick - self.shown) / TICKS_PER_BEAT as f64) as f32 * self.beat_h
            }
        }
    }

    /// The rows of the play view looked at, `step` apart: as far as
    /// [`PLAY_REACH`] beats either side, at most [`PLAY_ROWS`].
    fn play_rows(&self, step: i64) -> impl Iterator<Item = i64> {
        let step = step.max(1);
        let reach = (PLAY_REACH * TICKS_PER_BEAT).min(PLAY_ROWS / 2 * step);
        let mid = (self.shown.round() as i64).div_euclid(step) * step;
        let first = (mid - reach).max(0);
        (first..=mid + reach).step_by(step as usize)
    }

    /// The rows `step` apart (from 0) whose y is in `[top, bottom]`.
    pub fn rows(&self, timing: &TimingMap, top: f32, bottom: f32, step: i64) -> Vec<i64> {
        let step = step.max(1);
        match &self.play {
            None => {
                let (a, b) = self.edit_span(top, bottom);
                let first = a.div_euclid(step).max(0) * step;
                (first..=b)
                    .step_by(step as usize)
                    .filter(|&t| {
                        let y = self.y(timing, t as f64);
                        y >= top && y <= bottom
                    })
                    .collect()
            }
            Some(_) => self
                .play_rows(step)
                .filter(|&t| {
                    let y = self.y(timing, t as f64);
                    y >= top && y <= bottom
                })
                .collect(),
        }
    }

    /// The edit view's ticks from `top` to `bottom`, a tick either side.
    fn edit_span(&self, top: f32, bottom: f32) -> (i64, i64) {
        let at = |y: f32| {
            self.shown + f64::from((y - self.line_y) / self.beat_h) * TICKS_PER_BEAT as f64
        };
        (at(top).floor() as i64 - 1, at(bottom).ceil() as i64 + 1)
    }

    /// The ticks drawn between `top` and `bottom`: in the play view the
    /// first and last beats seen, a beat either side.
    pub fn span(&self, timing: &TimingMap, top: f32, bottom: f32) -> (i64, i64) {
        match &self.play {
            None => self.edit_span(top, bottom),
            Some(_) => {
                let rows = self.rows(timing, top, bottom, TICKS_PER_BEAT);
                match (rows.first(), rows.last()) {
                    (Some(a), Some(b)) => (a - TICKS_PER_BEAT, b + TICKS_PER_BEAT),
                    _ => {
                        let s = self.shown.round() as i64;
                        (s - TICKS_PER_BEAT, s + TICKS_PER_BEAT)
                    }
                }
            }
        }
    }

    /// The row `step` apart nearest `y` (the edit view: the tick there,
    /// rounded to the row; the play view: the row drawn nearest, the one
    /// nearer the cursor on a tie).
    pub fn row_at(&self, timing: &TimingMap, y: f32, step: i64) -> Tick {
        let step = step.max(1);
        match &self.play {
            None => {
                let t =
                    self.shown + f64::from((y - self.line_y) / self.beat_h) * TICKS_PER_BEAT as f64;
                Tick(((t / step as f64).round() as i64 * step).max(0))
            }
            Some(_) => {
                let shown = self.shown;
                let best = self.play_rows(step).min_by(|&a, &b| {
                    let da = (self.y(timing, a as f64) - y).abs();
                    let db = (self.y(timing, b as f64) - y).abs();
                    da.total_cmp(&db).then_with(|| {
                        (a as f64 - shown)
                            .abs()
                            .total_cmp(&(b as f64 - shown).abs())
                    })
                });
                Tick(best.unwrap_or(0))
            }
        }
    }

    /// A beat's height at the line (the play view's at the cursor).
    pub fn beat_height(&self, timing: &TimingMap) -> f32 {
        match &self.play {
            None => self.beat_h,
            Some(_) => {
                let s = self.shown.round() as i64;
                (self.y(timing, (s + TICKS_PER_BEAT) as f64) - self.y(timing, s as f64)).abs()
            }
        }
    }
}

/// A play's frame of `chart`'s notes whose span shows between `top` and
/// `bottom` (a lane's width beyond either), at song second `seconds` and
/// beat `beat`; `holding`, the lanes with a hold under the line while
/// the music plays.
pub fn frame(
    chart: &Chart,
    timing: &TimingMap,
    m: &Mapping,
    top: f32,
    bottom: f32,
    seconds: f64,
    holding: &[Option<HoldKind>],
) -> Frame {
    let lanes = holding.len();
    let px = m.lane_w * m.dir();
    let (lo, hi) = (top - m.lane_w, bottom + m.lane_w);
    let (first, last) = m.span(timing, lo, hi);
    let mut notes = Vec::new();
    for (index, n) in chart.notes.iter().enumerate() {
        // The edit view's rows in order: what is not near is not drawn.
        if m.play.is_none() && (n.end_tick().0 < first || n.tick.0 > last) {
            continue;
        }
        let y = m.arrows(timing, n.tick);
        let kind = match n.kind {
            NoteKind::Tap => SpriteKind::Tap,
            NoteKind::Lift => SpriteKind::Lift,
            NoteKind::Fake => SpriteKind::Fake,
            NoteKind::Mine => SpriteKind::Mine,
            NoteKind::Shock => SpriteKind::Shock,
            NoteKind::Dummy => SpriteKind::Dummy,
            NoteKind::AutoKeysound => continue,
            NoteKind::HoldHead { end } => SpriteKind::HoldHead {
                tail_y: m.arrows(timing, end),
                active: false,
                dropped: false,
            },
            NoteKind::RollHead { end } => SpriteKind::RollHead {
                tail_y: m.arrows(timing, end),
                active: false,
                dropped: false,
            },
        };
        let tail = match kind {
            SpriteKind::HoldHead { tail_y, .. } | SpriteKind::RollHead { tail_y, .. } => tail_y,
            _ => y,
        };
        let (a, b) = (m.line_y + y * px, m.line_y + tail * px);
        if a.max(b) < lo || a.min(b) > hi {
            continue;
        }
        let beat = n.tick.beat();
        notes.push(NoteSprite {
            note_index: index,
            lane: n.lane,
            y,
            kind,
            quantization: n.tick.quantization(),
            beat_frac: (beat - beat.floor()) as f32,
            color: n.color,
            alpha: 1.0,
            glow: 0.0,
            beat: beat as f32,
            hold_life: 1.0,
        });
    }
    Frame {
        song_time: seconds,
        beat: timing.beat_at(seconds),
        roll_window_seconds: 0.35,
        lanes: lanes as u8,
        receptors: holding
            .iter()
            .map(|h| ReceptorState {
                pressed: false,
                flash: None,
                holding: *h,
            })
            .collect(),
        notes,
        judgement: None,
        holds: Default::default(),
        combo: 0,
        score: Default::default(),
        life: 1.0,
        danger: false,
        failed: false,
        finished: false,
        visible_range: ((bottom - top) / m.lane_w).abs() + 2.0,
        appearance: Appearance::Visible,
        cover: Default::default(),
    }
}

/// While the music plays: the lanes with a hold or a roll under the
/// line at `tick`.
pub fn holding(chart: &Chart, lanes: usize, tick: f64) -> Vec<Option<HoldKind>> {
    let mut out = vec![None; lanes];
    for n in &chart.notes {
        if let Some(end) = n.kind.hold_end()
            && (n.tick.0 as f64) <= tick
            && tick < end.0 as f64
            && let Some(h) = out.get_mut(usize::from(n.lane))
        {
            *h = Some(match n.kind {
                NoteKind::RollHead { .. } => HoldKind::Roll,
                _ => HoldKind::Hold,
            });
        }
    }
    out
}

/// The skin's events of the notes the music passed from tick `from`
/// (not included) to `to`, as an autoplay hits them: a press and a
/// perfect step and tap for each tap, lift and hold's head, a held for
/// each hold's tail. Numbered on from `seq`.
pub fn passed(chart: &Chart, from: f64, to: f64, seconds: f64, seq: &mut u64) -> Vec<SkinEvent> {
    let mut out = Vec::new();
    if to <= from {
        return out;
    }
    let crossed = |t: Tick| (t.0 as f64) > from && (t.0 as f64) <= to;
    let mut push = |out: &mut Vec<SkinEvent>, lane: u8, kind, index: usize| {
        out.push(SkinEvent {
            seq: *seq,
            lane,
            kind,
            song_time: seconds,
            autoplay: true,
            danoni_judgments: false,
            note: Some(SkinNote { index, y: 0.0 }),
        });
        *seq += 1;
    };
    for (i, n) in chart.notes.iter().enumerate() {
        let hit = matches!(
            n.kind,
            NoteKind::Tap | NoteKind::Lift | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
        );
        if hit && crossed(n.tick) {
            let judgement = Judgement::W1;
            push(&mut out, n.lane, SkinEventKind::Press, i);
            push(&mut out, n.lane, SkinEventKind::Step(Some(judgement)), i);
            for kind in [
                SkinEventKind::Tap {
                    judgement,
                    bright: false,
                },
                SkinEventKind::NoteTap {
                    judgement,
                    bright: false,
                },
            ] {
                push(&mut out, n.lane, kind, i);
            }
            if n.kind.hold_end().is_none() {
                push(&mut out, n.lane, SkinEventKind::Lift, i);
            }
        }
        if let Some(end) = n.kind.hold_end()
            && crossed(end)
        {
            push(&mut out, n.lane, SkinEventKind::Held { bright: false }, i);
            push(&mut out, n.lane, SkinEventKind::Lift, i);
        }
    }
    out
}

#[cfg(test)]
#[path = "editor_skin_tests.rs"]
mod skin_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::{BpmSegment, Note, StopSegment};
    use ddi_engine::scroll::SpeedMod;

    fn chart(notes: Vec<Note>) -> Chart {
        let mut c = crate::state::editor::Editor::new_song(None, "T")
            .chart()
            .clone();
        c.notes = notes;
        c
    }

    #[test]
    fn the_edit_view_spaces_beats_evenly() {
        let t = TimingMap::constant(120.0, 0.0);
        let m = Mapping::edit(100.0, 96.0, 80.0, 40.0);
        assert_eq!(m.y(&t, 96.0), 100.0);
        assert_eq!(m.y(&t, 144.0), 180.0);
        assert_eq!(m.arrows(&t, Tick(144)), 2.0);
        assert_eq!(m.row_at(&t, 181.0, 12), Tick(144));
        assert_eq!(m.row_at(&t, 195.0, 12), Tick(156));
        assert_eq!(m.rows(&t, 100.0, 180.0, 48), vec![96, 144]);
    }

    #[test]
    fn the_play_view_is_where_play_draws() {
        let mut t = TimingMap::constant(120.0, 0.0);
        t.bpms.push(BpmSegment {
            tick: Tick::from_beats(4),
            bpm: 240.0,
        });
        t.stops.push(StopSegment {
            tick: Tick::from_beats(6),
            seconds: 0.5,
        });
        let scroll = ScrollOptions {
            speed: SpeedMod::CMod(120.0),
            ..Default::default()
        };
        let shown = 96.0;
        let at = shown_seconds(&t, shown);
        let m = Mapping::play(scroll, &t, at, [0.0, 600.0], 100.0, shown, 80.0, 40.0);
        for tick in [96, 144, 192, 240, 288, 336] {
            let want = ddi_engine::scroll::note_y(scroll, &t, Tick(tick), at);
            assert_eq!(m.arrows(&t, Tick(tick)), want, "{tick}");
            assert!((m.y(&t, tick as f64) - (100.0 + want * 40.0)).abs() < 1e-3);
        }
        // Reverse: the line as far from the bottom, later notes above.
        let r = ScrollOptions {
            reverse: true,
            ..scroll
        };
        let m = Mapping::play(r, &t, at, [0.0, 600.0], 100.0, shown, 80.0, 40.0);
        assert_eq!(m.line_y, 500.0);
        assert!(m.y(&t, 144.0) < 500.0);
        // A row is found where it is drawn.
        let y = m.y(&t, 240.0);
        assert_eq!(m.row_at(&t, y + 1.0, 12), Tick(240));
    }

    #[test]
    fn the_play_view_equals_the_edit_view_at_one_tempo() {
        let t = TimingMap::constant(150.0, 0.1);
        let lane = 48.0;
        let scroll = ScrollOptions {
            speed: SpeedMod::XMod(2.0),
            ..Default::default()
        };
        let shown = 200.0;
        let e = Mapping::edit(120.0, shown, lane * 2.0, lane);
        let p = Mapping::play(
            scroll,
            &t,
            shown_seconds(&t, shown),
            [0.0, 500.0],
            120.0,
            shown,
            lane * 2.0,
            lane,
        );
        for tick in (0..800).step_by(6) {
            let (a, b) = (e.y(&t, tick as f64), p.y(&t, tick as f64));
            assert!((a - b).abs() < 1e-3, "{tick}: {a} {b}");
        }
    }

    #[test]
    fn frames_hold_what_shows() {
        let c = chart(vec![
            Note::new(Tick(0), 0, NoteKind::Tap),
            Note::new(Tick(48), 1, NoteKind::HoldHead { end: Tick(96) }),
            Note::new(Tick(4800), 2, NoteKind::Tap),
        ]);
        let t = TimingMap::constant(120.0, 0.0);
        let m = Mapping::edit(100.0, 0.0, 80.0, 40.0);
        let f = frame(&c, &t, &m, 0.0, 600.0, 0.0, &[None; 4]);
        assert_eq!(f.notes.len(), 2);
        assert_eq!(f.notes[0].y, 0.0);
        assert_eq!(f.notes[1].y, 2.0);
        assert!(matches!(
            f.notes[1].kind,
            SpriteKind::HoldHead { tail_y, .. } if tail_y == 4.0
        ));
        assert_eq!(f.receptors.len(), 4);
    }

    #[test]
    fn notes_passed_explode() {
        let c = chart(vec![
            Note::new(Tick(48), 0, NoteKind::Tap),
            Note::new(Tick(48), 1, NoteKind::HoldHead { end: Tick(96) }),
            Note::new(Tick(60), 2, NoteKind::Mine),
        ]);
        let mut seq = 5;
        let e = passed(&c, 40.0, 70.0, 1.0, &mut seq);
        assert_eq!(e.first().map(|e| e.seq), Some(5));
        assert!(e.iter().all(|e| e.lane != 2), "mines are not hit");
        assert!(
            e.iter()
                .any(|e| e.lane == 0 && e.kind == SkinEventKind::Lift)
        );
        let tail = passed(&c, 70.0, 96.0, 1.5, &mut seq);
        assert!(
            tail.iter()
                .any(|e| matches!(e.kind, SkinEventKind::Held { .. }))
        );
        assert_eq!(holding(&c, 4, 60.0)[1], Some(HoldKind::Hold));
        assert_eq!(holding(&c, 4, 96.0)[1], None);
        assert!(passed(&c, 96.0, 96.0, 2.0, &mut seq).is_empty());
    }

    #[test]
    fn dense_snaps_raise_the_zoom() {
        // A 16th's rows a third of a 48-pixel note apart: 64 a beat.
        assert_eq!(least_beat_height(12, 48.0), 64.0);
        assert_eq!(least_beat_height(1, 48.0), 768.0);
    }
}
