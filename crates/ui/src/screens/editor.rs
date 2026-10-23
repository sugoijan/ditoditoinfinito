//! The chart editor (`docs/plans/editor.md`, steps 3 and 4): a song's
//! chart on a field that scrolls under a fixed cursor line, later beats
//! below, the beat grid at the snap, the timing's markers beside it; a
//! side panel says where the cursor is and what is there; the footer
//! the keys. The editing itself is [`Editor`]'s (`state::editor`); the
//! shell loads the song, saves it, plays its music and its playtests
//! ([`EditorHost`]).
//!
//! A shell that can ([`EditorHost::skin_field`]) draws the notes in the
//! player's noteskin as play draws them, paused at the cursor
//! ([`editor_field`]); the screen draws over them what must read
//! exactly: the grid, a tick at each note's row, the tails, the
//! selection, the markers and the cursor line. Rows are evenly spaced
//! by beat (the edit view, at the zoom; raised at dense snaps so the
//! rows stay a third of a note apart), or where play has them (the
//! play view, V: the player's speed, the timing's changes, Reverse).
//! Elsewhere (the harness), and as the compact field (F), notes are
//! their lanes' arrows on plates colored by their beat, as StepMania's
//! editor colors them, drawn with the menus' actors.
//!
//! Keys, after StepMania's editor: ↑↓ move by the snap (Shift selects),
//! ←→ change the snap, Page Up/Down a measure, Home/End the start and the
//! end; the number keys are the lanes (pressed: a note placed or taken
//! away; held while the cursor moves: a hold, or a roll with the roll
//! tool); N the next note tool; Space or Enter listen from the cursor;
//! P play from the cursor (Shift: autoplayed); T the timing at the
//! cursor; B its BPM; I the song, C the chart; Tab the menu; ⌘/Ctrl Z
//! undo (with Shift redo), C, X, V, A copy, cut, paste, select all, S
//! save; Delete deletes the selection, M mirrors it, [ ] or Alt ↑↓ move
//! it by the snap; − and = zoom; V the view, F the compact field;
//! Escape leaves (asking when unsaved).
//! K turns the metronome on or off, Shift K the assist clap; Y is tapped
//! along to the music, Shift Y sets the BPM tapped ([`sound`]).
//! A controller moves and changes the snap with its directions, listens
//! with Start and opens the menu with Select. A pointer places a note
//! with a tap on a lane and a hold with a drag along one, moves the
//! cursor with a tap left of the lanes and scrolls with the wheel.
//! With reduced motion the field jumps to the cursor instead of gliding.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_actor::{Actor, Kind, Rgba};
use ddi_chart::{DisplayBpm, Glyph, NoteKind, Quantization, TICKS_PER_BEAT, Tick};
use ddi_engine::transform::Turn;

use super::editor_dialogs::{ActionsDialog, PickFn, SubmitFn, ValueDialog};
use super::editor_field::{self, FieldPicture, FieldView, Mapping, SkinField};
use super::options_dialogs::ConfirmDialog;
use super::{Env, footer, frame, ground, header, left, right};
use crate::gesture::{Gesture, WheelKind};
use crate::hit;
use crate::look::{Spec, call, quad};
use crate::message::{Message, Param};
use crate::nav::MenuAction;
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::editor::overview::Overview;
use crate::state::editor::{
    ChartInfo, Editor, MEASURE, NoteTool, Retime, Segment, SegmentKind, SongInfo, autosave,
    trim_number,
};

/// Broadcast when a dialog changed the song: the screen draws it again.
pub const CHANGED: &str = "EditorChanged";
/// Broadcast by the shell when a save ended: `Revision` (the
/// [`Editor::revision`] saved, -1 for none) on success, `Error` (text)
/// on failure.
pub const SAVED: &str = "EditorSaved";

/// What the editor asks of its shell.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorCall {
    /// Play the music from song second `from` (inside the press: the web
    /// starts sound only there).
    Listen {
        from: f64,
    },
    Stop,
    /// Save the song where it came from (the shell broadcasts [`SAVED`]).
    Save {
        revision: Option<u64>,
    },
    /// Save a copy elsewhere: a download on the web, a folder picked on
    /// desktop; as `.sm` (only when nothing would be lost) or `.ssc`.
    Export {
        sm: bool,
    },
    /// Play the chart shown as it is now, from the cursor or the start,
    /// autoplayed or not; back to the editor after. Never kept in the
    /// play history.
    Playtest {
        from_cursor: bool,
        auto: bool,
    },
    /// The editor is left (stop the music, forget the autosave).
    Leave,
    /// Keep the song as edited (`state::editor::autosave`): every
    /// [`autosave::EVERY`] seconds while there are unsaved changes; a
    /// save that ends well, or leaving, forgets it.
    Autosave,
    /// What sounds along the music changed while it plays (the editor's
    /// [`Editor::sounds`], or what they come from: the song, the chart;
    /// `ddi_session::listen::Cues::of`). A listen asked for reads them
    /// too.
    Cues,
}

/// The shell's side of the editor.
pub trait EditorHost {
    fn call(&self, call: EditorCall, editor: &Editor, cx: &mut Cx);

    /// The song second heard while the music plays, `None` once it
    /// stopped.
    fn music_time(&self) -> Option<f64> {
        None
    }

    /// The music's overview, once made from the music decoded for
    /// listening (`state::editor::overview`).
    fn overview(&self) -> Option<Rc<Overview>> {
        None
    }

    /// The overview is being made: the screen asks again soon.
    fn overview_coming(&self) -> bool {
        false
    }

    /// The shell draws the field in the player's skin, as
    /// [`EditorHost::show_field`] says, with this scroll; `None`: the
    /// screen draws the notes on plates.
    fn skin_field(&self) -> Option<SkinField> {
        None
    }

    /// What the shell draws for the field from now on (into the texture
    /// [`editor_field::SOURCE`]); `None`: nothing (the plates drawn, the
    /// screen gone). Told only when it changes, with the song (whose
    /// chart a skin's Lua is told of).
    fn show_field(&self, _picture: Option<&FieldPicture>, _editor: &Editor) {}
}

/// What the shell can do with the song.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorPlace {
    /// Where saving puts the song ("Songs/Pack/Song/song.ssc"); `None`:
    /// it cannot be saved there (a bundled song).
    pub save: Option<String>,
    /// Saving replaces what the song was read from: what the question
    /// asked first says (what is replaced, what is kept); `None`: saved
    /// without asking.
    pub confirm: Option<String>,
    /// What exporting does ("Download", "Save a copy"); `None`: none.
    pub export: Option<String>,
    /// The music can be listened to.
    pub listen: bool,
    /// Playtests can be started.
    pub playtest: bool,
    /// What leaving without saving loses besides the changes (a new
    /// song never saved: the song); `None` once saved.
    pub discard: Option<String>,
}

/// What the screen shows, for the shells' debug views and tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorView {
    pub song: String,
    pub chart: usize,
    pub charts: usize,
    pub layout: String,
    pub cursor: i64,
    pub snap: i64,
    pub tool: String,
    pub notes: usize,
    pub selection: Option<(i64, i64)>,
    pub dirty: bool,
    pub listening: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The markers' texts at the cursor.
    pub segments: Vec<String>,
    /// The tick drawn at the cursor line (it glides to the cursor).
    pub shown: f64,
    /// "edit" or "play" ([`FieldView`]).
    pub view: String,
    /// The notes are drawn on plates (the compact field, or a shell
    /// that draws no skin).
    pub plates: bool,
    /// A beat's height at the cursor line, virtual pixels.
    pub beat_h: f32,
    /// The metronome and the assist clap are on ([`Editor::sounds`]).
    pub metronome: bool,
    pub clap: bool,
    /// The taps of tap-to-tempo, and the BPM they say.
    pub taps: usize,
    pub tapped: Option<f64>,
    /// The music's overview's bins (0 until it is made).
    pub overview: usize,
}

/// A note's plate color by its beat (StepMania's editor's: 4th red, 8th
/// blue, 12th purple, 16th yellow, …), unless the look names
/// `Editor.Quant.<n>`.
fn quant_color(env: &Env, q: Quantization) -> Rgba {
    let (name, default) = match q {
        Quantization::N4 => ("4", [0.92, 0.25, 0.3, 1.0]),
        Quantization::N8 => ("8", [0.25, 0.45, 0.95, 1.0]),
        Quantization::N12 => ("12", [0.68, 0.3, 0.9, 1.0]),
        Quantization::N16 => ("16", [0.95, 0.82, 0.2, 1.0]),
        Quantization::N24 => ("24", [0.95, 0.45, 0.75, 1.0]),
        Quantization::N32 => ("32", [0.95, 0.6, 0.2, 1.0]),
        Quantization::N48 => ("48", [0.3, 0.85, 0.9, 1.0]),
        Quantization::N64 => ("64", [0.35, 0.8, 0.4, 1.0]),
        Quantization::N192 => ("192", [0.6, 0.6, 0.65, 1.0]),
    };
    env.look
        .color(&format!("Editor.Quant.{name}"))
        .unwrap_or(default)
}

/// The arrow a lane's glyph points (`Glyph::Arrow`: 0 left, 90 up).
fn glyph_text(glyph: &Glyph) -> &'static str {
    match glyph {
        Glyph::Arrow { rotation_deg } => {
            let a = (rotation_deg.rem_euclid(360.0) / 45.0).round() as i32 % 8;
            ["←", "↖", "↑", "↗", "→", "↘", "↓", "↙"][a as usize]
        }
        Glyph::Center => "●",
        _ => "●",
    }
}

/// "16th", "32nd".
pub fn snap_name(n: i64) -> String {
    let suffix = match n % 10 {
        2 if n % 100 != 12 => "nd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Song seconds as `m:ss.mmm` (negative before the music).
fn clock(seconds: f64) -> String {
    let sign = if seconds < 0.0 { "-" } else { "" };
    let s = seconds.abs();
    let m = (s / 60.0).floor();
    format!("{sign}{}:{:06.3}", m as i64, s - m * 60.0)
}

/// Where things are, from the screen's size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Geometry {
    size: [f32; 2],
    zoom: f32,
    compact: bool,
    /// The field's area, `[x, y, w, h]`.
    field: [f32; 4],
    /// The panel's.
    panel: [f32; 4],
    /// The music's overview's strip ([`sound`]).
    overview: [f32; 4],
    /// The lanes' left edge, a lane's width.
    lanes_x: f32,
    lane_w: f32,
    /// Where the cursor line is, from the top.
    line_y: f32,
    beat_h: f32,
    /// The field in the surface's physical pixels (whole ones: the
    /// skin's texture), where the virtual screen's origin is in them,
    /// and physical pixels a virtual one.
    pixels: [f32; 4],
    origin: [f32; 2],
    scale: f32,
}

/// A drag along a lane (a hold being drawn).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    lane: Option<u8>,
    from: Tick,
    to: Tick,
    moved: bool,
}

/// The chart editor's screen.
pub struct EditorScreen {
    env: Env,
    editor: Rc<RefCell<Editor>>,
    host: Option<Rc<dyn EditorHost>>,
    place: EditorPlace,
    geo: Geometry,
    /// The tick drawn at the cursor line.
    shown: f64,
    /// What the field was last drawn for.
    drawn: Option<(f64, u64)>,
    /// Bumped by every change: the field and the panel are drawn again.
    version: u64,
    held: Vec<u8>,
    shift: bool,
    retime: Rc<Cell<Retime>>,
    listening: bool,
    drag: Option<Drag>,
    /// Zoom of the beat's height, a power of two.
    zoom_steps: i32,
    /// Trackpad scroll not yet a step.
    scroll: f32,
    status: String,
    /// The revision last kept by the autosave, and when (the UI's
    /// seconds).
    autosaved: (Option<u64>, f64),
    /// The metronome's, the taps' and the overview's ([`sound`]).
    sound: sound::SoundTools,
    /// The field's view, and whether the notes are on plates (the
    /// compact field) where the shell draws the skin.
    view: FieldView,
    plates: bool,
    /// The shell draws the skin, with this scroll.
    skin: Option<SkinField>,
    /// The skin's texture, the picture the shell was given last.
    texture: u32,
    pictured: Option<FieldPicture>,
    /// While listening: the second heard, the tick the music's notes
    /// were passed to, and the skin's events (numbered by `seq`).
    heard: Option<f64>,
    passed_to: Option<f64>,
    events: Vec<ddi_engine::frame::SkinEvent>,
    seq: u64,
    /// The editor was left: the shell draws no field any more.
    left: bool,
}

impl EditorScreen {
    pub fn new(env: Env, editor: Editor, place: EditorPlace) -> EditorScreen {
        EditorScreen::shared(env, Rc::new(RefCell::new(editor)), place)
    }

    /// On an editor the shell keeps (the web's, across a playtest's
    /// page).
    pub fn shared(env: Env, editor: Rc<RefCell<Editor>>, place: EditorPlace) -> EditorScreen {
        EditorScreen {
            env,
            editor,
            host: None,
            place,
            geo: Geometry::default(),
            shown: 0.0,
            drawn: None,
            version: 0,
            held: Vec::new(),
            shift: false,
            retime: Rc::new(Cell::new(Retime::KeepBeats)),
            listening: false,
            drag: None,
            zoom_steps: 0,
            scroll: 0.0,
            status: String::new(),
            autosaved: (None, 0.0),
            sound: sound::SoundTools::default(),
            view: FieldView::Edit,
            plates: false,
            skin: None,
            texture: 0,
            pictured: None,
            heard: None,
            passed_to: None,
            events: Vec::new(),
            seq: 0,
            left: false,
        }
    }

    /// With a line in the panel at first ("Recovered …").
    pub fn with_status(mut self, status: &str) -> EditorScreen {
        self.status = status.into();
        self
    }

    pub fn with_host(mut self, host: Rc<dyn EditorHost>) -> EditorScreen {
        self.skin = host.skin_field();
        self.host = Some(host);
        self
    }

    /// The notes are drawn by the shell in the player's skin.
    fn skin_drawn(&self) -> bool {
        self.skin.is_some() && !self.plates
    }

    /// The song being edited, shared with the shell.
    pub fn editor(&self) -> Rc<RefCell<Editor>> {
        self.editor.clone()
    }

    fn call(&self, call: EditorCall, cx: &mut Cx) {
        if let Some(h) = &self.host {
            h.call(call, &self.editor.borrow(), cx);
        }
    }

    fn changed(&mut self) {
        self.version += 1;
    }

    fn say(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.changed();
    }

    // ---------------------------------------------------------------------
    // Layout and drawing.
    // ---------------------------------------------------------------------

    fn geometry(&self, cx: &Cx) -> Geometry {
        let vp = cx.viewport().screen;
        let (size, zoom) = super::narrow_fit(&self.env, vp);
        let compact = super::is_compact(&*self.env.look, vp) || size[0] < 700.0;
        let env = &self.env;
        let head = env.metric("Header.Height", 64.0);
        let foot = env.metric("Footer.Height", 40.0);
        let margin = if compact { 8.0 } else { 16.0 };
        let upright = size[1] > size[0];
        let lanes = self.editor.borrow().lanes().max(1) as f32;
        // The panel at the right; under the field when held upright.
        let (field, panel) = if upright {
            let ph = (size[1] * 0.3).min(230.0);
            let fh = size[1] - head - foot - ph - 2.0 * margin;
            (
                [margin, head + margin, size[0] - 2.0 * margin, fh],
                [
                    margin,
                    head + margin + fh + margin,
                    size[0] - 2.0 * margin,
                    ph - margin,
                ],
            )
        } else {
            let pw = if compact { 250.0_f32 } else { 360.0 }.min(size[0] * 0.42);
            let fw = size[0] - pw - 3.0 * margin;
            let h = size[1] - head - foot - 2.0 * margin;
            (
                [margin, head + margin, fw, h],
                [margin * 2.0 + fw, head + margin, pw, h],
            )
        };
        let (field, overview) = sound::carve(field, compact);
        let ruler = if compact { 44.0 } else { 64.0 };
        let markers = if compact { 90.0 } else { 150.0 };
        // The lanes in the middle of the room the ruler and the markers
        // leave.
        let room = field[2] - ruler - markers;
        let widest = if compact { 48.0 } else { 72.0 };
        let lane_w = (room / lanes).clamp(18.0, widest);
        let lanes_x = field[0] + ruler + ((room - lane_w * lanes) / 2.0).max(0.0);
        let base = if compact { 56.0 } else { 88.0 };
        let mut beat_h = base * 2f32.powi(self.zoom_steps);
        if self.skin_drawn() {
            let step = self.editor.borrow().step();
            beat_h = beat_h.max(editor_field::least_beat_height(step, lane_w));
        }
        // The skin's texture on whole pixels.
        let scale = cx.viewport().scale * zoom;
        let origin = cx.viewport().to_physical([0.0, 0.0]);
        let px = |v: f32, o: f32| (o + v * scale).round();
        let (x0, y0) = (px(field[0], origin[0]), px(field[1], origin[1]));
        let (x1, y1) = (
            px(field[0] + field[2], origin[0]),
            px(field[1] + field[3], origin[1]),
        );
        Geometry {
            size,
            zoom,
            compact,
            field,
            panel,
            overview,
            lanes_x,
            lane_w,
            line_y: field[1] + field[3] * 0.28,
            beat_h,
            pixels: [x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)],
            origin,
            scale,
        }
    }

    /// The tick at `y` on the field (the play view: the row drawn
    /// nearest, to a 16th).
    pub(super) fn tick_at_y(&self, y: f32) -> f64 {
        let e = self.editor.borrow();
        let m = self.mapping(e.timing());
        match m.play {
            Some(_) => m.row_at(e.timing(), y, TICKS_PER_BEAT / 4).0 as f64,
            None => m.shown + f64::from((y - m.line_y) / m.beat_h) * TICKS_PER_BEAT as f64,
        }
    }

    /// Where ticks are on the field, `shown` at the cursor line: in the
    /// view's spacing.
    fn mapping(&self, timing: &ddi_chart::TimingMap) -> Mapping {
        let g = &self.geo;
        match (self.view, self.skin.map(|s| s.scroll)) {
            (FieldView::Play, scroll) => {
                let seconds = self
                    .heard
                    .unwrap_or_else(|| editor_field::shown_seconds(timing, self.shown));
                Mapping::play(
                    scroll.unwrap_or_default(),
                    timing,
                    seconds,
                    [g.field[1], g.field[1] + g.field[3]],
                    g.line_y,
                    self.shown,
                    g.beat_h,
                    g.lane_w,
                )
            }
            (FieldView::Edit, _) => Mapping::edit(g.line_y, self.shown, g.beat_h, g.lane_w),
        }
    }

    fn tree(&mut self, cx: &mut Cx) -> Actor {
        self.geo = self.geometry(cx);
        self.attach_texture(cx);
        let g = self.geo;
        let title = {
            let e = self.editor.borrow();
            let t = &e.song().title;
            if t.is_empty() {
                "Edit".to_string()
            } else {
                format!("Edit · {t}")
            }
        };
        let mut children = vec![
            ground(&self.env, g.size),
            header(&self.env, g.size[0], &title),
        ];
        children.push(self.field());
        children.push(self.overview_strip());
        children.push(self.overview_marks());
        children.push(self.panel());
        let (f, texts) = self.footer();
        children.push(f);
        let mut root = Actor::new(Kind::Frame(children));
        call(&mut root, "zoom", &[g.zoom]);
        self.env.state.borrow_mut().hints = texts;
        self.drawn = Some((self.shown, self.version));
        self.send_picture();
        root
    }

    /// The skin's texture, as long as the field is drawn.
    fn attach_texture(&mut self, cx: &mut Cx) {
        if self.skin_drawn() {
            let [_, _, w, h] = self.geo.pixels;
            self.texture = cx.texture(editor_field::SOURCE, w.max(h));
        }
    }

    /// Tells the shell what to draw for the field when it changed
    /// (nothing once the editor is left).
    fn send_picture(&mut self) {
        let picture = (self.skin_drawn() && !self.left).then(|| self.picture());
        if picture != self.pictured {
            if let Some(h) = &self.host {
                h.show_field(picture.as_ref(), &self.editor.borrow());
            }
            self.pictured = picture;
        }
    }

    /// What the shell draws: the notes in view as a play paused at the
    /// moment shown (running while the music plays), in the texture's
    /// pixels.
    fn picture(&self) -> FieldPicture {
        let g = self.geo;
        let e = self.editor.borrow();
        let timing = e.timing();
        let m = self.mapping(timing);
        let [_, fy, _, fh] = g.field;
        let lanes = e.lanes() as usize;
        let seconds = self
            .heard
            .unwrap_or_else(|| editor_field::shown_seconds(timing, self.shown));
        let holding = if self.listening {
            editor_field::holding(e.chart(), lanes, self.shown)
        } else {
            vec![None; lanes]
        };
        let frame = editor_field::frame(e.chart(), timing, &m, fy, fy + fh, seconds, &holding);
        let [px, py, _, _] = g.pixels;
        let ground = self.env.color("Ground");
        let plate = self.env.color("Plate");
        let a = plate[3] * 0.55;
        let mix = |k: usize| ground[k] * (1.0 - a) + plate[k] * a;
        FieldPicture {
            rect: g.pixels,
            layout: e.layout().id.clone(),
            arrow: g.lane_w * g.scale,
            left: g.origin[0] + g.lanes_x * g.scale - px,
            receptor_y: g.origin[1] + m.line_y * g.scale - py,
            reverse: m.dir() < 0.0,
            frame,
            running: self.listening,
            events: if self.listening {
                self.events.clone()
            } else {
                Vec::new()
            },
            background: [mix(0), mix(1), mix(2), 1.0],
        }
    }

    fn footer(&self) -> (Actor, Vec<String>) {
        let lanes = self.editor.borrow().lanes();
        let lane_keys = format!("1–{}", lanes.min(10) % 10);
        let mut hints: Vec<(Vec<&str>, &str)> = vec![
            (vec!["↑↓"], "Move"),
            (vec!["←→"], "Snap"),
            (vec![lane_keys.as_str()], "Notes"),
        ];
        if !self.geo.compact {
            if self.place.listen {
                hints.push((vec!["SPACE"], "Listen"));
            }
            if self.place.playtest {
                hints.push((vec!["P"], "Play"));
            }
        }
        hints.push((vec!["TAB"], "Menu"));
        hints.push((vec!["ESC"], "Back"));
        let hints: Vec<(&[&str], &str)> = hints.iter().map(|(k, l)| (k.as_slice(), *l)).collect();
        footer(&self.env, self.geo.size, &hints, "", &[])
    }

    /// The field: its plate, the beat grid, the selection, the notes
    /// (the skin's picture under the rest, or plates), the markers and
    /// the cursor line.
    fn field(&self) -> Actor {
        let g = self.geo;
        let env = &self.env;
        let e = self.editor.borrow();
        let timing = e.timing();
        let m = self.mapping(timing);
        let y_of = |t: f64| m.y(timing, t);
        let skin = self.skin_drawn();
        let layout = e.layout();
        let lanes = layout.lane_count();
        let [fx, fy, fw, fh] = g.field;
        let mut c = Vec::new();
        let mut plate = quad("Field", [fw, fh], with_alpha(env.color("Plate"), 0.55));
        call(&mut plate, "xy", &[fx + fw / 2.0, fy + fh / 2.0]);
        c.push(plate);
        if skin {
            // The shell's picture, on the whole pixels it is drawn at.
            let [px, py, pw, ph] = g.pixels;
            let (x, y) = ((px - g.origin[0]) / g.scale, (py - g.origin[1]) / g.scale);
            let (w, h) = (pw / g.scale, ph / g.scale);
            let mut picture = Actor::new(Kind::Sprite(ddi_actor::Sprite::quad([w, h])));
            if let Kind::Sprite(s) = &mut picture.kind {
                s.texture = self.texture;
            }
            call(&mut picture, "xy", &[x + w / 2.0, y + h / 2.0]);
            c.push(picture);
        }
        let lanes_w = g.lane_w * lanes as f32;
        for l in 0..lanes {
            if l % 2 == 1 && !skin {
                let mut band = quad("", [g.lane_w, fh], [1.0, 1.0, 1.0, 0.025]);
                call(
                    &mut band,
                    "xy",
                    &[g.lanes_x + (l as f32 + 0.5) * g.lane_w, fy + fh / 2.0],
                );
                c.push(band);
            }
        }
        let (first, last) = m.span(timing, fy, fy + fh);
        // Warps and fakes (no note in them is judged): shaded.
        if skin {
            let regions = timing
                .warps
                .iter()
                .map(|w| (w.tick, w.length, [0.95, 0.35, 0.35, 0.14]))
                .chain(
                    timing
                        .fakes
                        .iter()
                        .map(|f| (f.tick, f.length, [0.6, 0.6, 0.7, 0.16])),
                );
            for (tick, length, color) in regions {
                let (y0, y1) = (y_of(tick.0 as f64), y_of((tick.0 + length.0) as f64));
                let (y0, y1) = (y0.min(y1).max(fy), y0.max(y1).min(fy + fh));
                if y1 > y0 {
                    let mut band = quad("", [lanes_w, y1 - y0], color);
                    call(
                        &mut band,
                        "xy",
                        &[g.lanes_x + lanes_w / 2.0, (y0 + y1) / 2.0],
                    );
                    c.push(band);
                }
            }
        }
        // The selection.
        if let Some((a, b)) = e.selection() {
            let (y0, y1) = (y_of(a.0 as f64), y_of(b.0 as f64));
            let (y0, y1) = (y0.min(y1).max(fy), y0.max(y1).min(fy + fh));
            if y1 >= y0 {
                let mut band = quad(
                    "Selection",
                    [lanes_w + 8.0, (y1 - y0).max(3.0)],
                    with_alpha(env.color("Accent"), 0.22),
                );
                call(
                    &mut band,
                    "xy",
                    &[g.lanes_x + lanes_w / 2.0, (y0 + y1) / 2.0],
                );
                c.push(band);
            }
        }
        // The grid: measures, beats, and the snap's lines when they are
        // far enough apart.
        let step = e.step();
        let beat_h = m.beat_height(timing);
        let snap_px = step as f32 / TICKS_PER_BEAT as f32 * beat_h;
        let grid_step = if snap_px >= 6.0 { step } else { TICKS_PER_BEAT };
        for t in m.rows(timing, fy - 2.0, fy + fh + 2.0, grid_step) {
            let y = y_of(t as f64);
            let (alpha, h) = if t % MEASURE == 0 {
                (0.55, 2.0)
            } else if t % TICKS_PER_BEAT == 0 {
                (0.25, 1.0)
            } else {
                (0.09, 1.0)
            };
            let mut line = quad("", [lanes_w, h], [1.0, 1.0, 1.0, alpha]);
            call(&mut line, "xy", &[g.lanes_x + lanes_w / 2.0, y]);
            c.push(line);
            if t % MEASURE == 0 {
                let m = env.numbers("", &(t / MEASURE).to_string(), 13.0, "Muted");
                c.push(right(m, g.lanes_x - 10.0, y));
            } else if t % TICKS_PER_BEAT == 0 && beat_h >= 40.0 {
                let b = env.text("", &trim_number(Tick(t).beat()), 10.0, 400, "Muted");
                let mut b = right(b, g.lanes_x - 10.0, y);
                call(&mut b, "diffusealpha", &[0.6]);
                c.push(b);
            }
        }
        let visible = e
            .chart()
            .notes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.end_tick().0 >= first && n.tick.0 <= last);
        if skin {
            // Over the skin's notes: a tick at each one's row in its
            // lane's gutter, a line at each tail, what the art cannot
            // say (fake, lift) as a badge.
            for (i, n) in visible {
                let x0 = g.lanes_x + f32::from(n.lane) * g.lane_w;
                let y = y_of(n.tick.0 as f64);
                let color = match n.kind {
                    NoteKind::Mine => [0.95, 0.4, 0.4, 1.0],
                    _ => quant_color(env, n.tick.quantization()),
                };
                let tick_w = (g.lane_w * 0.16).max(4.0);
                let mut tick = quad(&format!("Row.{i}"), [tick_w, 2.0], color);
                call(&mut tick, "xy", &[x0 + tick_w / 2.0, y]);
                c.push(tick);
                if let Some(end) = n.kind.hold_end() {
                    let roll = matches!(n.kind, NoteKind::RollHead { .. });
                    let color = if roll {
                        [0.95, 0.75, 0.25, 0.95]
                    } else {
                        [0.45, 0.85, 0.5, 0.95]
                    };
                    let mut tail = quad("", [g.lane_w * 0.7, 2.0], color);
                    call(&mut tail, "xy", &[x0 + g.lane_w / 2.0, y_of(end.0 as f64)]);
                    c.push(tail);
                }
                let badge = match n.kind {
                    NoteKind::Fake => Some("F"),
                    NoteKind::Lift => Some("L"),
                    _ => None,
                };
                if let Some(b) = badge {
                    let size = (g.lane_w * 0.28).clamp(9.0, 14.0);
                    let mut back = quad("", [size + 4.0, size + 2.0], [0.0, 0.0, 0.0, 0.7]);
                    let at = [x0 + g.lane_w - size / 2.0 - 3.0, y - g.lane_w * 0.3];
                    call(&mut back, "xy", &at);
                    call(&mut back, "cornerradius", &[3.0]);
                    c.push(back);
                    let mut t = env.text("", b, size, 700, "Text");
                    call(&mut t, "xy", &at);
                    c.push(t);
                }
            }
        } else {
            // Holds' bodies first, then the heads over them.
            let size = (g.lane_w * 0.82).min(44.0);
            let head_h = (size * 0.62).max(12.0);
            let mut heads = Vec::new();
            for (_, n) in visible {
                let x = g.lanes_x + (f32::from(n.lane) + 0.5) * g.lane_w;
                let y = y_of(n.tick.0 as f64);
                if let Some(end) = n.kind.hold_end() {
                    let yt = y_of(end.0 as f64);
                    let y1 = yt.max(y).min(fy + fh + 4.0);
                    let y0 = yt.min(y).max(fy - 4.0);
                    let roll = matches!(n.kind, NoteKind::RollHead { .. });
                    let color = if roll {
                        [0.95, 0.75, 0.25, 0.55]
                    } else {
                        [0.45, 0.85, 0.5, 0.55]
                    };
                    let mut body = quad("", [size * 0.55, (y1 - y0).max(1.0)], color);
                    call(&mut body, "xy", &[x, (y0 + y1) / 2.0]);
                    c.push(body);
                    let mut tail = quad("", [size * 0.75, 4.0], with_alpha(color, 0.9));
                    call(&mut tail, "xy", &[x, yt]);
                    c.push(tail);
                }
                heads.push((x, y, n));
            }
            for (x, y, n) in heads {
                let lane = layout.lanes.get(n.lane as usize);
                let (plate_color, text) = match n.kind {
                    NoteKind::Mine => ([0.25, 0.25, 0.28, 1.0], "✕"),
                    _ => (
                        quant_color(env, n.tick.quantization()),
                        lane.map_or("●", |l| glyph_text(&l.glyph)),
                    ),
                };
                let alpha = if matches!(n.kind, NoteKind::Fake) {
                    0.45
                } else {
                    1.0
                };
                let mut p = quad("", [size, head_h], with_alpha(plate_color, alpha));
                call(&mut p, "xy", &[x, y]);
                call(&mut p, "cornerradius", &[head_h * 0.3]);
                c.push(p);
                let mut glyph = env.text("", text, (head_h * 0.9).min(24.0), 700, "Text");
                call(&mut glyph, "xy", &[x, y]);
                if matches!(n.kind, NoteKind::Mine) {
                    call(&mut glyph, "diffuse", &[1.0, 0.35, 0.35, 1.0]);
                }
                c.push(glyph);
                if matches!(n.kind, NoteKind::Lift) {
                    let mut bar = quad("", [size, 3.0], [1.0, 1.0, 1.0, 0.9]);
                    call(&mut bar, "xy", &[x, y - head_h / 2.0 - 2.0]);
                    c.push(bar);
                }
            }
            // Receptors: the lanes' arrows, faint, at the cursor line.
            for (i, l) in layout.lanes.iter().enumerate() {
                let mut r = env.text("", glyph_text(&l.glyph), 14.0, 700, "Muted");
                call(
                    &mut r,
                    "xy",
                    &[
                        g.lanes_x + (i as f32 + 0.5) * g.lane_w,
                        m.line_y - head_h * m.dir(),
                    ],
                );
                call(&mut r, "diffusealpha", &[0.35]);
                c.push(r);
            }
        }
        // The timing's markers at the right of the lanes.
        let mx = g.lanes_x + lanes_w + 10.0;
        let mut on_row: Vec<(i64, usize)> = Vec::new();
        for (tick, seg) in e.segments_in(Tick(first), Tick(last)) {
            let y = y_of(tick.0 as f64);
            if y < fy - 20.0 || y > fy + fh + 20.0 {
                continue;
            }
            let k = match on_row.iter_mut().find(|(t, _)| *t == tick.0) {
                Some((_, k)) => {
                    *k += 1;
                    *k
                }
                None => {
                    on_row.push((tick.0, 0));
                    0
                }
            };
            let color = match seg.kind() {
                SegmentKind::Bpm => "Accent",
                _ => "Muted",
            };
            let mut m = env.text("", &seg.describe(), 12.0, 600, color);
            super::select_dialogs::wrap(&mut m, fx + fw - mx - 4.0);
            c.push(left(m, mx, y + k as f32 * 14.0));
        }
        // The cursor line, and the row it is on.
        let mut line = quad(
            "Cursor",
            [lanes_w + 16.0, 3.0],
            with_alpha(env.color("Accent"), 0.95),
        );
        call(
            &mut line,
            "xy",
            &[g.lanes_x + lanes_w / 2.0, y_of(e.cursor().0 as f64)],
        );
        c.push(line);
        let mut f = frame("FieldLayer", c);
        f.clip = Some([fx, fy, fx + fw, fy + fh]);
        f
    }

    /// The panel: where the cursor is and what is there, and buttons.
    fn panel(&self) -> Actor {
        let g = self.geo;
        let env = &self.env;
        let e = self.editor.borrow();
        let [px, py, pw, ph] = g.panel;
        let mut c = Vec::new();
        let mut plate = env.component(
            "Editor.Panel",
            "Panel",
            &Spec::new([pw, ph], ""),
            "PanelPlate",
            [px, py],
        );
        plate.name = "PanelPlate".into();
        c.push(plate);
        let song = e.song();
        let chart = e.chart();
        let layout = e.layout();
        let small = g.compact;
        let size = if small { 12.0 } else { 14.0 };
        let line_h = if small { 16.0 } else { 20.0 };
        let mut y = py + if small { 16.0 } else { 24.0 };
        let x = px + 16.0;
        let width = pw - 32.0;
        let mut push = |c: &mut Vec<Actor>, name: &str, s: &str, weight: u16, color: &str| {
            let mut t = env.text(name, s, size, weight, color);
            super::select_dialogs::wrap(&mut t, width);
            c.push(left(t, x, y));
            y += line_h;
        };
        let artist = if song.artist.is_empty() {
            "—".to_string()
        } else {
            song.artist.clone()
        };
        push(&mut c, "Panel.Artist", &artist, 400, "Muted");
        let chart_line = format!(
            "{} · {} {} ({}/{})",
            layout.name,
            chart.difficulty.name(),
            chart.meter,
            e.chart_index() + 1,
            song.charts.len()
        );
        push(&mut c, "Panel.Chart", &chart_line, 700, "Text");
        let cursor = e.cursor();
        let measure = cursor.0.div_euclid(MEASURE);
        push(
            &mut c,
            "Panel.Beat",
            &format!(
                "Beat {} · measure {} · {}",
                trim_number(cursor.beat()),
                measure,
                clock(e.cursor_seconds())
            ),
            400,
            "Text",
        );
        push(
            &mut c,
            "Panel.Snap",
            &format!("Snap {} · {}", snap_name(e.snap()), e.tool().name()),
            400,
            "Text",
        );
        push(&mut c, "Button.View", &self.view_line(), 400, "Accent");
        let bpm = e.timing().bpm_at_beat(cursor.beat());
        let own = if e.has_own_timing() {
            " (chart's own timing)"
        } else {
            ""
        };
        push(
            &mut c,
            "Panel.Bpm",
            &format!("{} BPM{own}", trim_number(bpm)),
            400,
            "Text",
        );
        for (name, text, color) in self.sound_lines() {
            push(&mut c, name, &text, 400, color);
        }
        let at = e.segments_at(cursor);
        if !at.is_empty() {
            let s: Vec<String> = at.iter().map(Segment::describe).collect();
            push(
                &mut c,
                "Panel.Here",
                &format!("Here: {}", s.join(", ")),
                400,
                "Accent",
            );
        }
        push(
            &mut c,
            "Panel.Notes",
            &format!("{} notes", e.note_count()),
            400,
            "Muted",
        );
        if let Some((a, b)) = e.selection() {
            push(
                &mut c,
                "Panel.Selection",
                &format!(
                    "Selected beats {}–{}",
                    trim_number(a.beat()),
                    trim_number(b.beat())
                ),
                400,
                "Accent",
            );
        }
        let saved = if e.is_dirty() {
            "Unsaved changes"
        } else {
            "Saved"
        };
        push(
            &mut c,
            "Panel.Saved",
            saved,
            600,
            if e.is_dirty() { "Accent" } else { "Muted" },
        );
        if !self.status.is_empty() {
            push(&mut c, "Panel.Status", &self.status, 400, "Muted");
        }
        // Buttons along the panel's bottom.
        let mut buttons = vec![("Button.Menu", "Menu")];
        if self.place.save.is_some() {
            buttons.push(("Button.Save", "Save"));
        }
        if self.place.playtest {
            buttons.push(("Button.Play", "Play"));
        }
        let n = buttons.len() as f32;
        let bh = if small { 34.0 } else { 40.0 };
        let bw = (pw - 32.0 - (n - 1.0) * 8.0) / n;
        let by = py + ph - 12.0 - bh / 2.0;
        for (i, (name, label)) in buttons.iter().enumerate() {
            let mut b = crate::look::component_or(
                &*env.look,
                "Button",
                "Button",
                &Spec::new([bw, bh], *label),
            );
            b.name = (*name).into();
            call(
                &mut b,
                "xy",
                &[px + 16.0 + bw / 2.0 + i as f32 * (bw + 8.0), by],
            );
            c.push(b);
        }
        frame("Panel", c)
    }

    /// The panel's line saying how the field is drawn (a tap changes
    /// the view).
    fn view_line(&self) -> String {
        let mut s = match self.view {
            FieldView::Edit => "Edit view".to_string(),
            FieldView::Play => {
                let scroll = self.skin.map(|s| s.scroll).unwrap_or_default();
                let speed = match scroll.speed {
                    ddi_engine::scroll::SpeedMod::XMod(x) => format!("{}x", trim_number(x)),
                    ddi_engine::scroll::SpeedMod::CMod(b) => format!("C{}", trim_number(b)),
                };
                let reverse = if scroll.reverse { ", reverse" } else { "" };
                format!("Play view · {speed}{reverse}")
            }
        };
        if self.plates && self.skin.is_some() {
            s.push_str(" · compact");
        }
        if !self.geo.compact {
            s.push_str(" (V)");
        }
        s
    }

    /// Draws the field and the panel again when something changed; the
    /// field alone when only it moved (a glide, the music heard).
    fn refresh(&mut self, root: &mut Actor, cx: &mut Cx) {
        if let Some((shown, version)) = self.drawn
            && version == self.version
        {
            if shown == self.shown {
                return;
            }
            if self.geometry(cx) == self.geo {
                self.attach_texture(cx);
                if let Some(old) = hit::find_mut(root, "FieldLayer") {
                    *old = self.field();
                }
                if let Some(old) = hit::find_mut(root, "OverviewMarks") {
                    *old = self.overview_marks();
                }
                self.drawn = Some((self.shown, self.version));
                self.send_picture();
                if let Some(view) = &mut self.env.state.borrow_mut().editor {
                    view.shown = self.shown;
                }
                return;
            }
        }
        let geo = self.geometry(cx);
        if geo != self.geo {
            *root = self.tree(cx);
            root.finish_tweening();
        } else {
            self.attach_texture(cx);
            let field = self.field();
            let panel = self.panel();
            let (footer, texts) = self.footer();
            let (strip, marks) = (self.overview_strip(), self.overview_marks());
            for (name, a) in [
                ("FieldLayer", field),
                ("Overview", strip),
                ("OverviewMarks", marks),
                ("Panel", panel),
                ("Footer", footer),
            ] {
                if let Some(old) = hit::find_mut(root, name) {
                    *old = a;
                }
            }
            self.env.state.borrow_mut().hints = texts;
            self.drawn = Some((self.shown, self.version));
            self.send_picture();
        }
        self.publish();
    }

    /// The editor leaves: the debug view no longer shows it, the shell
    /// draws no field.
    fn gone(&mut self) {
        self.env.state.borrow_mut().editor = None;
        self.left = true;
        if self.pictured.take().is_some()
            && let Some(h) = &self.host
        {
            h.show_field(None, &self.editor.borrow());
        }
    }

    fn publish(&self) {
        let e = self.editor.borrow();
        let mut st = self.env.state.borrow_mut();
        st.screen = "editor".into();
        st.compact = self.geo.compact;
        st.song = e.song().title.clone();
        st.status = self.status.clone();
        st.editor = Some(EditorView {
            song: e.song().title.clone(),
            chart: e.chart_index(),
            charts: e.song().charts.len(),
            layout: e.chart().layout.clone(),
            cursor: e.cursor().0,
            snap: e.snap(),
            tool: e.tool().name().into(),
            notes: e.note_count(),
            selection: e.selection().map(|(a, b)| (a.0, b.0)),
            dirty: e.is_dirty(),
            listening: self.listening,
            can_undo: e.can_undo(),
            can_redo: e.can_redo(),
            segments: e
                .segments_at(e.cursor())
                .iter()
                .map(Segment::describe)
                .collect(),
            shown: self.shown,
            view: self.view.name().into(),
            plates: !self.skin_drawn(),
            beat_h: self.mapping(e.timing()).beat_height(e.timing()),
            metronome: e.sounds().metronome,
            clap: e.sounds().clap,
            taps: self.sound.taps.len(),
            tapped: self.sound.taps.estimate().map(|t| t.bpm),
            overview: self.sound.overview.as_ref().map_or(0, |o| o.levels.len()),
        });
    }

    // ---------------------------------------------------------------------
    // Actions.
    // ---------------------------------------------------------------------

    /// Runs `f` on the editor; the song is drawn again.
    fn edit(&mut self, f: impl FnOnce(&mut Editor)) {
        f(&mut self.editor.borrow_mut());
        self.changed();
    }

    fn listen(&mut self, cx: &mut Cx) {
        if !self.place.listen {
            return;
        }
        if self.listening {
            self.stop_listening(cx);
        } else {
            let from = self.editor.borrow().cursor_seconds();
            self.listening = true;
            // The cursor's own row passes too.
            self.passed_to = Some(self.editor.borrow().cursor().0 as f64 - 0.5);
            self.listen_started();
            self.call(EditorCall::Listen { from }, cx);
            self.say("Listening");
        }
    }

    /// Stops the music; the cursor goes to the snapped row last heard.
    fn stop_listening(&mut self, cx: &mut Cx) {
        if !self.listening {
            return;
        }
        self.listening = false;
        (self.heard, self.passed_to) = (None, None);
        self.events.clear();
        self.call(EditorCall::Stop, cx);
        let tick = Tick(self.shown.round() as i64);
        self.edit(|e| {
            let t = e.snap_tick(tick);
            e.set_cursor(t, false);
        });
        self.say("");
    }

    /// While listening in the skin: its events of the notes the music
    /// passed since the last frame (the last few kept).
    fn pass_notes(&mut self, seconds: f64) {
        if !self.skin_drawn() {
            return;
        }
        let from = self.passed_to.unwrap_or(self.shown);
        let mut passed = editor_field::passed(
            self.editor.borrow().chart(),
            from,
            self.shown,
            seconds,
            &mut self.seq,
        );
        self.events.append(&mut passed);
        let keep = 64;
        if self.events.len() > keep {
            self.events.drain(..self.events.len() - keep);
        }
        self.passed_to = Some(self.shown.max(from));
    }

    fn playtest(&mut self, from_cursor: bool, auto: bool, cx: &mut Cx) {
        if !self.place.playtest {
            return;
        }
        self.stop_listening(cx);
        self.call(EditorCall::Playtest { from_cursor, auto }, cx);
    }

    fn save(&mut self, cx: &mut Cx) {
        if self.place.save.is_none() {
            self.say("This song cannot be saved here: export a copy instead");
            return;
        }
        let editor = self.editor.clone();
        let host = self.host.clone();
        let save = Rc::new(move |cx: &mut Cx| {
            let e = editor.borrow();
            if let Some(h) = &host {
                h.call(
                    EditorCall::Save {
                        revision: e.revision(),
                    },
                    &e,
                    cx,
                );
            }
        });
        match &self.place.confirm {
            Some(body) => cx.overlay(
                ConfirmDialog::new(
                    self.env.clone(),
                    "Save over the song?",
                    &format!(
                        "{body} Charts whose notes changed are new charts for the play history."
                    ),
                    "Save",
                    save,
                ),
                true,
            ),
            None => save(cx),
        }
    }

    fn back(&mut self, cx: &mut Cx) {
        self.stop_listening(cx);
        if !self.editor.borrow().is_dirty() {
            self.call(EditorCall::Leave, cx);
            self.gone();
            cx.close(true);
            return;
        }
        let host = self.host.clone();
        let editor = self.editor.clone();
        let leave = Rc::new(move |cx: &mut Cx| {
            if let Some(h) = &host {
                h.call(EditorCall::Leave, &editor.borrow(), cx);
            }
            cx.broadcast(Message::new(LEAVE));
        });
        cx.overlay(
            ConfirmDialog::new(
                self.env.clone(),
                "Leave without saving?",
                self.place
                    .discard
                    .as_deref()
                    .unwrap_or("The changes since the last save are lost."),
                "Leave",
                leave,
            ),
            true,
        );
    }

    fn open_menu(&mut self, cx: &mut Cx) {
        let e = self.editor.borrow();
        let mut items: Vec<(&str, String)> = Vec::new();
        if self.place.listen {
            items.push((
                "listen",
                if self.listening { "Stop" } else { "Listen" }.into(),
            ));
        }
        if self.place.playtest {
            items.push(("play", "Play from cursor".into()));
            items.push(("play-start", "Play from start".into()));
            items.push(("auto", "Autoplay from cursor".into()));
        }
        drop(e);
        self.sound_items(&mut items);
        let e = self.editor.borrow();
        items.push(("undo", "Undo".into()));
        items.push(("redo", "Redo".into()));
        items.push(("tool", format!("Tool: {}", e.tool().name())));
        items.push(("timing", "Timing here…".into()));
        items.push(("song", "Song…".into()));
        items.push(("chart", "Chart…".into()));
        if e.selection().is_some() {
            items.push(("copy", "Copy".into()));
            items.push(("cut", "Cut".into()));
            items.push(("delete", "Delete selection".into()));
            items.push(("mirror", "Mirror selection".into()));
        } else {
            items.push(("select-all", "Select all".into()));
        }
        if e.clip().is_some() {
            items.push(("paste", "Paste".into()));
        }
        if self.place.save.is_some() {
            items.push(("save", "Save".into()));
        }
        if let Some(label) = &self.place.export {
            items.push(("export", format!("{label} (.ssc)")));
            if ddi_chart::formats::sm_write::sm_losses(e.song()).is_empty() {
                items.push(("export-sm", format!("{label} (.sm)")));
            }
        }
        items.push((
            "view",
            match self.view {
                FieldView::Edit => "Play view",
                FieldView::Play => "Edit view",
            }
            .into(),
        ));
        if self.skin.is_some() {
            items.push((
                "plates",
                if self.plates {
                    "Field in the skin"
                } else {
                    "Compact field"
                }
                .into(),
            ));
        }
        items.push(("leave", "Leave the editor".into()));
        drop(e);
        let items = items
            .into_iter()
            .map(|(id, l)| (id.to_string(), l))
            .collect();
        cx.overlay(
            ActionsDialog::new(self.env.clone(), "Editor", items, pick(ACTION)),
            true,
        );
    }

    /// What the menu's action `id` does.
    fn action(&mut self, id: &str, cx: &mut Cx) {
        if self.sound_action(id) {
            return;
        }
        match id {
            "listen" => self.listen(cx),
            "play" => self.playtest(true, false, cx),
            "play-start" => self.playtest(false, false, cx),
            "auto" => self.playtest(true, true, cx),
            "undo" => self.edit(Editor::undo),
            "redo" => self.edit(Editor::redo),
            "tool" => self.next_tool(1),
            "timing" => self.open_timing(cx),
            "song" => self.open_song(cx),
            "chart" => self.open_chart(cx),
            "copy" => self.edit(Editor::copy),
            "cut" => self.edit(Editor::cut),
            "paste" => self.edit(Editor::paste),
            "delete" => self.edit(Editor::delete_selection),
            "mirror" => self.edit(|e| e.turn_selection(Turn::Mirror)),
            "select-all" => self.edit(Editor::select_all),
            "save" => self.save(cx),
            "export" => self.call(EditorCall::Export { sm: false }, cx),
            "export-sm" => self.call(EditorCall::Export { sm: true }, cx),
            "leave" => self.back(cx),
            "view" => self.toggle_view(),
            "plates" => self.toggle_plates(),
            _ => {}
        }
    }

    /// The edit view (rows by beat) or the play view (as play has them).
    fn toggle_view(&mut self) {
        self.view = match self.view {
            FieldView::Edit => FieldView::Play,
            FieldView::Play => FieldView::Edit,
        };
        self.say(match self.view {
            FieldView::Edit => "Edit view: rows evenly by beat",
            FieldView::Play => "Play view: as play draws it",
        });
    }

    /// The notes on plates (the compact field) or in the skin.
    fn toggle_plates(&mut self) {
        if self.skin.is_none() {
            return;
        }
        self.plates = !self.plates;
        self.say(if self.plates {
            "Compact field"
        } else {
            "Field in the skin"
        });
    }

    fn next_tool(&mut self, by: i32) {
        let tools = NoteTool::ALL;
        let at = tools
            .iter()
            .position(|t| *t == self.editor.borrow().tool())
            .unwrap_or(0) as i32;
        let next = tools[(at + by).rem_euclid(tools.len() as i32) as usize];
        self.edit(|e| e.set_tool(next));
        self.say(format!("Tool: {}", next.name()));
    }

    /// A value dialog whose text `apply` takes: the song is drawn again
    /// once it is.
    fn ask(
        &self,
        cx: &mut Cx,
        title: &str,
        note: &str,
        value: &str,
        apply: impl Fn(&mut Editor, &str) -> Result<(), String> + 'static,
    ) {
        cx.overlay(self.value_dialog(title, note, value, apply), true);
    }

    /// [`EditorScreen::ask`]'s dialog.
    fn value_dialog(
        &self,
        title: &str,
        note: &str,
        value: &str,
        apply: impl Fn(&mut Editor, &str) -> Result<(), String> + 'static,
    ) -> ValueDialog {
        let editor = self.editor.clone();
        let submit: SubmitFn = Rc::new(move |text: &str, cx: &mut Cx| {
            apply(&mut editor.borrow_mut(), text.trim())?;
            cx.broadcast(Message::new(CHANGED));
            Ok(())
        });
        ValueDialog::new(self.env.clone(), title, note, value, submit)
    }

    /// The timing at the cursor: each kind of segment, the offset, and
    /// how timing changes treat the notes.
    fn open_timing(&mut self, cx: &mut Cx) {
        let e = self.editor.borrow();
        let tick = e.cursor();
        let at = e.segments_at(tick);
        let mut items = Vec::new();
        for kind in SegmentKind::ALL {
            let here = at.iter().find(|s| s.kind() == kind);
            let value = match (kind, here) {
                (_, Some(s)) => segment_text(s),
                (SegmentKind::Bpm, None) => format!(
                    "{} from before",
                    trim_number(e.timing().bpm_at_beat(tick.beat()))
                ),
                _ => "—".into(),
            };
            items.push((
                format!("seg:{}", kind_id(kind)),
                format!("{}: {value}", kind.name()),
            ));
        }
        items.push((
            "offset".into(),
            format!("Offset: {} s", trim_number(e.timing().offset_seconds)),
        ));
        items.push((
            "retime".into(),
            match self.retime.get() {
                Retime::KeepBeats => "Changes keep notes on their beats".into(),
                Retime::KeepTimes => "Changes keep notes' times".into(),
            },
        ));
        items.push((
            "own".into(),
            if e.has_own_timing() {
                "Chart's own timing: on".into()
            } else {
                "Chart's own timing: off".into()
            },
        ));
        let note = format!("At beat {}.", trim_number(tick.beat()));
        drop(e);
        cx.overlay(
            ActionsDialog::new(self.env.clone(), "Timing", items, pick(TIMING)).with_note(&note),
            true,
        );
    }

    fn timing_action(&mut self, id: &str, cx: &mut Cx) {
        let retime = self.retime.clone();
        match id {
            "offset" => {
                let value = trim_number(self.editor.borrow().timing().offset_seconds);
                self.ask(
                    cx,
                    "Offset",
                    "Seconds; the music's start is at minus the offset (StepMania's sign).",
                    &value,
                    move |e, t| {
                        let v = parse_number(t)?;
                        e.set_offset(v, retime.get());
                        Ok(())
                    },
                );
            }
            "retime" => {
                self.retime.set(match self.retime.get() {
                    Retime::KeepBeats => Retime::KeepTimes,
                    Retime::KeepTimes => Retime::KeepBeats,
                });
                self.open_timing(cx);
            }
            "own" => {
                self.edit(|e| {
                    let own = e.has_own_timing();
                    e.set_own_timing(!own);
                });
                self.open_timing(cx);
            }
            other => {
                let Some(kind) = other
                    .strip_prefix("seg:")
                    .and_then(|k| SegmentKind::ALL.into_iter().find(|s| kind_id(*s) == k))
                else {
                    return;
                };
                self.ask_segment(kind, cx);
            }
        }
    }

    fn ask_segment(&mut self, kind: SegmentKind, cx: &mut Cx) {
        let (tick, current) = {
            let e = self.editor.borrow();
            let tick = e.cursor();
            let here = e.segments_at(tick).into_iter().find(|s| s.kind() == kind);
            (tick, here)
        };
        let value = current.as_ref().map(segment_text).unwrap_or_default();
        let note = match kind {
            SegmentKind::Bpm => "Beats a minute from here. Empty takes the change away.",
            SegmentKind::Stop | SegmentKind::Delay => {
                "Seconds (a stop pauses after the notes on its row, a delay before). Empty or 0 \
                 takes it away."
            }
            SegmentKind::Warp | SegmentKind::Fake => "Its length in beats. Empty takes it away.",
            SegmentKind::Speed => {
                "The scroll speed's ratio, then how long it takes to get there and in what: \
                 “1.5 2 beats”, “0.5 0.25 s”."
            }
            SegmentKind::Scroll => "How fast the notes scroll from here, 1 normal.",
            SegmentKind::TimeSignature => "As “3/4”.",
            SegmentKind::TickCount => "Hold ticks a beat, 0 to 48.",
            SegmentKind::Combo => "Combo a hit adds, and a miss: “2/1”, or one number for both.",
            SegmentKind::Label => "A name for this place.",
        };
        let retime = self.retime.clone();
        self.ask(
            cx,
            &format!("{} at beat {}", kind.name(), trim_number(tick.beat())),
            note,
            &value,
            move |e, t| {
                if t.is_empty() {
                    e.remove_segment(kind, tick, retime.get());
                    return Ok(());
                }
                let segment = parse_segment(kind, t)?;
                if e.set_segment(tick, segment, retime.get()) {
                    Ok(())
                } else {
                    Err("That value cannot be kept in a file.".into())
                }
            },
        );
    }

    /// The song's fields.
    fn open_song(&mut self, cx: &mut Cx) {
        let mut info = self.editor.borrow().song_info();
        let items = SONG_FIELDS
            .iter()
            .map(|(id, label, field)| (id.to_string(), field.line(label, &mut info)))
            .collect();
        cx.overlay(
            ActionsDialog::new(self.env.clone(), "Song", items, pick(SONG)),
            true,
        );
    }

    fn song_action(&mut self, id: &str, cx: &mut Cx) {
        let Some(&(_, label, field)) = SONG_FIELDS.iter().find(|(f, ..)| *f == id) else {
            return;
        };
        let value = field.value(&mut self.editor.borrow().song_info());
        self.ask(
            cx,
            &field.title(label),
            field.note(),
            &value,
            move |e, t| {
                let mut info = e.song_info();
                field.set(&mut info, t)?;
                e.set_song_info(info);
                Ok(())
            },
        );
    }

    /// The chart's fields, and the song's other charts.
    fn open_chart(&mut self, cx: &mut Cx) {
        let e = self.editor.borrow();
        let info = e.chart_info();
        let mut items = vec![
            (
                "difficulty".to_string(),
                format!("Difficulty: {}", info.difficulty.name()),
            ),
            ("meter".into(), format!("Meter: {}", info.meter)),
            ("name".into(), format!("Name: {}", info.name)),
            (
                "description".into(),
                format!("Description: {}", info.description),
            ),
            ("credit".into(), format!("Step artist: {}", info.credit)),
        ];
        let n = e.song().charts.len();
        if n > 1 {
            items.push(("prev".into(), "Previous chart".into()));
            items.push(("next".into(), "Next chart".into()));
        }
        items.push(("add".into(), "Add a chart…".into()));
        if n > 1 {
            items.push(("remove".into(), "Remove this chart…".into()));
        }
        let title = format!("Chart {} of {n}", e.chart_index() + 1);
        drop(e);
        cx.overlay(
            ActionsDialog::new(self.env.clone(), &title, items, pick(CHART)),
            true,
        );
    }

    fn chart_action(&mut self, id: &str, cx: &mut Cx) {
        let info = self.editor.borrow().chart_info();
        let count = self.editor.borrow().song().charts.len();
        let index = self.editor.borrow().chart_index();
        match id {
            "difficulty" => {
                let all = ddi_chart::Difficulty::ALL;
                let at = all.iter().position(|d| *d == info.difficulty).unwrap_or(0);
                let next = all[(at + 1) % all.len()];
                self.edit(|e| {
                    e.set_chart_info(ChartInfo {
                        difficulty: next,
                        ..info
                    })
                });
                self.open_chart(cx);
            }
            "prev" => {
                self.edit(|e| e.select_chart((index + count - 1) % count));
            }
            "next" => {
                self.edit(|e| e.select_chart((index + 1) % count));
            }
            "add" => {
                let items = ddi_chart::BUILTIN_LAYOUTS
                    .iter()
                    .filter_map(|id| ddi_chart::Layout::builtin(id))
                    .map(|l| {
                        (
                            format!("add:{}", l.id),
                            format!("{} ({} lanes)", l.name, l.lane_count()),
                        )
                    })
                    .collect();
                cx.overlay(
                    ActionsDialog::new(self.env.clone(), "Add a chart", items, pick(CHART))
                        .with_note("An empty Edit chart on the song's timing."),
                    true,
                );
            }
            "remove" => {
                let editor = self.editor.clone();
                cx.overlay(
                    ConfirmDialog::new(
                        self.env.clone(),
                        "Remove this chart?",
                        "Its notes go with it (Undo brings it back until you leave).",
                        "Remove",
                        Rc::new(move |cx| {
                            editor.borrow_mut().remove_chart();
                            cx.broadcast(Message::new(CHANGED));
                        }),
                    ),
                    true,
                );
            }
            other => {
                if let Some(layout) = other.strip_prefix("add:") {
                    let layout = layout.to_string();
                    self.edit(|e| e.add_chart(&layout, ddi_chart::Difficulty::Edit));
                    self.say("Chart added");
                    return;
                }
                let (title, value, numeric) = match other {
                    "meter" => ("Meter", info.meter.to_string(), true),
                    "name" => ("Name", info.name.clone(), false),
                    "description" => ("Description", info.description.clone(), false),
                    "credit" => ("Step artist", info.credit.clone(), false),
                    _ => return,
                };
                let field = other.to_string();
                let dialog = self.value_dialog(title, "", &value, move |e, t| {
                    let mut info = e.chart_info();
                    match field.as_str() {
                        "meter" => {
                            info.meter = t
                                .parse::<u32>()
                                .map_err(|_| "A whole number, 0 or more.".to_string())?
                        }
                        "name" => info.name = t.into(),
                        "description" => info.description = t.into(),
                        "credit" => info.credit = t.into(),
                        _ => {}
                    }
                    e.set_chart_info(info);
                    Ok(())
                });
                cx.overlay(if numeric { dialog.numeric() } else { dialog }, true);
            }
        }
    }

    // ---------------------------------------------------------------------
    // Input.
    // ---------------------------------------------------------------------

    fn lane_key(code: &str) -> Option<u8> {
        let d = code.strip_prefix("Digit")?.parse::<u8>().ok()?;
        Some(if d == 0 { 9 } else { d - 1 })
    }

    fn key(&mut self, k: &crate::KeyEvent, cx: &mut Cx) -> Handled {
        if k.code.starts_with("Shift") {
            self.shift = k.down;
            return Handled::No;
        }
        if let Some(lane) = Self::lane_key(&k.code)
            && !k.modifiers.any_but_shift()
        {
            if !k.down {
                if let Some(at) = self.held.iter().position(|l| *l == lane) {
                    self.held.remove(at);
                    self.editor.borrow_mut().release(lane);
                }
                return Handled::Yes;
            }
            if k.repeat || lane >= self.editor.borrow().lanes() {
                return Handled::Yes;
            }
            self.held.push(lane);
            self.edit(|e| e.press(lane));
            return Handled::Yes;
        }
        if !k.down || k.composing {
            return Handled::No;
        }
        let command = k.modifiers.command();
        let shift = k.modifiers.shift || self.shift;
        let key = k.key.to_ascii_lowercase();
        match (command, k.code.as_str()) {
            (true, "KeyZ") if shift => self.edit(Editor::redo),
            (true, "KeyZ") => self.edit(Editor::undo),
            (true, "KeyY") => self.edit(Editor::redo),
            (true, "KeyC") => {
                self.edit(Editor::copy);
                self.say("Copied");
            }
            (true, "KeyX") => self.edit(Editor::cut),
            (true, "KeyV") => self.edit(Editor::paste),
            (true, "KeyA") => self.edit(Editor::select_all),
            (true, "KeyS") => self.save(cx),
            (true, _) => return Handled::No,
            (false, "PageUp") => self.edit(|e| e.move_measures(-1, shift)),
            (false, "PageDown") => self.edit(|e| e.move_measures(1, shift)),
            (false, "Home") => self.edit(|e| e.set_cursor(Tick::ZERO, shift)),
            (false, "End") => self.edit(|e| {
                let end = e.end();
                e.set_cursor(end, shift)
            }),
            (false, "Delete") => self.edit(Editor::delete_selection),
            (false, "ArrowUp") if k.modifiers.alt => self.edit(|e| e.shift_selection(-1)),
            (false, "ArrowDown") if k.modifiers.alt => self.edit(|e| e.shift_selection(1)),
            (false, "BracketLeft") => self.edit(|e| e.shift_selection(-1)),
            (false, "BracketRight") => self.edit(|e| e.shift_selection(1)),
            (false, "Minus") => {
                self.zoom_steps = (self.zoom_steps - 1).max(-3);
                self.changed();
            }
            (false, "Equal") => {
                self.zoom_steps = (self.zoom_steps + 1).min(3);
                self.changed();
            }
            _ if k.modifiers.any_but_shift() => return Handled::No,
            _ => match key.as_str() {
                "m" => self.edit(|e| e.turn_selection(Turn::Mirror)),
                "n" => self.next_tool(if shift { -1 } else { 1 }),
                "p" => self.playtest(true, shift, cx),
                "t" => self.open_timing(cx),
                "b" => self.ask_segment(SegmentKind::Bpm, cx),
                "i" => self.open_song(cx),
                "c" => self.open_chart(cx),
                "v" => self.toggle_view(),
                "f" => self.toggle_plates(),
                "k" => self.toggle_sound(shift),
                "y" if shift => self.apply_tap(false),
                "y" if !k.repeat => self.tap(k.time, cx),
                "y" => {}
                _ => return Handled::No,
            },
        }
        Handled::Yes
    }

    /// Where a point on the field is: its lane (none left or right of
    /// them) and its tick on the snap's grid.
    fn field_point(&self, pos: [f32; 2]) -> (Option<u8>, Tick) {
        let g = &self.geo;
        let p = [pos[0] / g.zoom, pos[1] / g.zoom];
        let lanes = self.editor.borrow().lanes();
        let l = ((p[0] - g.lanes_x) / g.lane_w).floor();
        let lane = (l >= 0.0 && l < f32::from(lanes)).then_some(l as u8);
        let e = self.editor.borrow();
        let timing = e.timing();
        let m = self.mapping(timing);
        let tick = match m.play {
            // The play view: the row drawn nearest.
            Some(_) => m.row_at(timing, p[1], e.step()),
            None => Tick(self.tick_at_y(p[1]).round() as i64),
        };
        (lane, e.snap_tick(tick))
    }

    fn pointer(
        &mut self,
        target: &Option<hit::Target>,
        gesture: &Gesture,
        pos: [f32; 2],
        cx: &mut Cx,
    ) -> Handled {
        let Some(t) = target else {
            return Handled::No;
        };
        if let Some(name) = t.chain.iter().find_map(|n| n.strip_prefix("Button.")) {
            if *gesture == Gesture::Tap {
                match name {
                    "Menu" => self.open_menu(cx),
                    "View" => self.toggle_view(),
                    "Save" => self.save(cx),
                    "Play" => self.playtest(true, false, cx),
                    _ => {}
                }
            }
            return Handled::Yes;
        }
        if t.within("Overview") || t.within("OverviewMarks") {
            self.overview_pointer(gesture, pos, cx);
            return Handled::Yes;
        }
        if *gesture == Gesture::Tap && t.within("Panel.Sounds") {
            self.toggle_sound(false);
            return Handled::Yes;
        }
        if *gesture == Gesture::Tap && t.within("Panel.Tap") {
            self.apply_tap(false);
            return Handled::Yes;
        }
        if !t.within("FieldLayer") {
            return Handled::No;
        }
        let (lane, tick) = self.field_point(pos);
        match gesture {
            Gesture::Press => {
                self.drag = Some(Drag {
                    lane,
                    from: tick,
                    to: tick,
                    moved: false,
                });
            }
            Gesture::Tap => {
                self.drag = None;
                match lane {
                    Some(lane) => {
                        self.edit(|e| {
                            e.toggle_at(lane, tick);
                            e.set_cursor(tick, false);
                        });
                    }
                    None => self.edit(|e| e.set_cursor(tick, false)),
                }
            }
            Gesture::DragStart | Gesture::Drag { .. } => {
                if let Some(d) = &mut self.drag {
                    d.to = tick;
                    d.moved = true;
                    // Left of the lanes, a drag selects rows.
                    if d.lane.is_none() {
                        let (a, b) = (d.from, d.to);
                        self.edit(|e| e.select(a, b));
                    } else {
                        self.changed();
                    }
                }
            }
            Gesture::DragEnd { .. } | Gesture::Flick { .. } | Gesture::Release => {
                if let Some(d) = self.drag.take()
                    && d.moved
                    && let Some(lane) = d.lane
                {
                    self.edit(|e| {
                        e.place_long(lane, d.from, tick);
                        e.set_cursor(d.from.min(tick), false);
                    });
                }
            }
            Gesture::Cancel => self.drag = None,
            Gesture::LongPress => {}
        }
        Handled::Yes
    }
}

/// A message the dialogs send back: an action's id picked.
/// How a song's field is shown and set.
#[derive(Clone, Copy)]
enum SongField {
    Text(fn(&mut SongInfo) -> &mut String),
    /// A file in the song's folder, none when empty.
    File(fn(&mut SongInfo) -> &mut Option<String>),
    /// Seconds, not negative.
    Seconds(fn(&mut SongInfo) -> &mut f64),
    DisplayBpm,
}

/// The song's fields as the Song menu lists them: id, label, kind.
const SONG_FIELDS: [(&str, &str, SongField); 13] = [
    ("title", "Title", SongField::Text(|i| &mut i.title)),
    ("subtitle", "Subtitle", SongField::Text(|i| &mut i.subtitle)),
    ("artist", "Artist", SongField::Text(|i| &mut i.artist)),
    (
        "title_translit",
        "Title (romanized)",
        SongField::Text(|i| &mut i.title_translit),
    ),
    (
        "artist_translit",
        "Artist (romanized)",
        SongField::Text(|i| &mut i.artist_translit),
    ),
    ("genre", "Genre", SongField::Text(|i| &mut i.genre)),
    ("credit", "Credit", SongField::Text(|i| &mut i.credit)),
    ("music", "Music", SongField::File(|i| &mut i.music)),
    ("banner", "Banner", SongField::File(|i| &mut i.banner)),
    (
        "background",
        "Background",
        SongField::File(|i| &mut i.background),
    ),
    (
        "preview_start",
        "Preview from",
        SongField::Seconds(|i| &mut i.preview_start),
    ),
    (
        "preview_length",
        "Preview length",
        SongField::Seconds(|i| &mut i.preview_length),
    ),
    ("display_bpm", "Display BPM", SongField::DisplayBpm),
];

impl SongField {
    /// Its value as its dialog shows it.
    fn value(self, info: &mut SongInfo) -> String {
        match self {
            SongField::Text(f) => f(info).clone(),
            SongField::File(f) => f(info).clone().unwrap_or_default(),
            SongField::Seconds(f) => trim_number(*f(info)),
            SongField::DisplayBpm => display_bpm_text(info.display_bpm),
        }
    }

    /// Its line in the menu.
    fn line(self, label: &str, info: &mut SongInfo) -> String {
        match self {
            SongField::File(f) => format!("{label}: {}", f(info).as_deref().unwrap_or("—")),
            SongField::Seconds(_) => format!("{label}: {} s", self.value(info)),
            _ => format!("{label}: {}", self.value(info)),
        }
    }

    /// Its dialog's title.
    fn title(self, label: &str) -> String {
        match self {
            SongField::File(_) => format!("{label} file"),
            SongField::Seconds(_) => format!("{label} (seconds)"),
            _ => label.to_string(),
        }
    }

    fn note(self) -> &'static str {
        match self {
            SongField::File(_) => "A file in the song's folder, by its name.",
            SongField::DisplayBpm => {
                "One number, a range “100-200”, “*” for random, or empty for the chart's own."
            }
            _ => "",
        }
    }

    /// Sets it from its dialog's text.
    fn set(self, info: &mut SongInfo, t: &str) -> Result<(), String> {
        match self {
            SongField::Text(f) => *f(info) = t.into(),
            SongField::File(f) => *f(info) = (!t.is_empty()).then(|| t.to_string()),
            SongField::Seconds(f) => *f(info) = parse_number(t)?.max(0.0),
            SongField::DisplayBpm => info.display_bpm = parse_display_bpm(t)?,
        }
        Ok(())
    }
}

/// A menu's pick broadcast as `message`, its `Id` the item's.
fn pick(message: &'static str) -> PickFn {
    Rc::new(move |id, cx| cx.broadcast(Message::new(message).with("Id", Param::Str(id.into()))))
}

const ACTION: &str = "EditorAction";
const TIMING: &str = "EditorTiming";
const SONG: &str = "EditorSong";
const CHART: &str = "EditorChart";
/// Leave confirmed.
const LEAVE: &str = "EditorLeave";

fn with_alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}

fn kind_id(k: SegmentKind) -> &'static str {
    match k {
        SegmentKind::Bpm => "bpm",
        SegmentKind::Stop => "stop",
        SegmentKind::Delay => "delay",
        SegmentKind::Warp => "warp",
        SegmentKind::Speed => "speed",
        SegmentKind::Scroll => "scroll",
        SegmentKind::Fake => "fake",
        SegmentKind::TimeSignature => "timesig",
        SegmentKind::TickCount => "tickcount",
        SegmentKind::Combo => "combo",
        SegmentKind::Label => "label",
    }
}

fn parse_number(t: &str) -> Result<f64, String> {
    t.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("“{t}” is not a number."))
}

/// A segment's values as typed ([`parse_segment`] reads them back).
fn segment_text(s: &Segment) -> String {
    match s {
        Segment::Bpm(v) | Segment::Stop(v) | Segment::Delay(v) | Segment::Scroll(v) => {
            trim_number(*v)
        }
        Segment::Warp(l) | Segment::Fake(l) => trim_number(l.beat()),
        Segment::Speed { ratio, delay, unit } => format!(
            "{} {} {}",
            trim_number(*ratio),
            trim_number(*delay),
            match unit {
                ddi_chart::SpeedUnit::Beats => "beats",
                ddi_chart::SpeedUnit::Seconds => "s",
            }
        ),
        Segment::TimeSignature(a, b) => format!("{a}/{b}"),
        Segment::TickCount(t) => t.to_string(),
        Segment::Combo(a, b) => format!("{a}/{b}"),
        Segment::Label(s) => s.clone(),
    }
}

/// A segment of `kind` from its typed values.
fn parse_segment(kind: SegmentKind, t: &str) -> Result<Segment, String> {
    let whole = |s: &str| {
        s.trim()
            .parse::<u32>()
            .map_err(|_| format!("“{s}” is not a whole number."))
    };
    let pair = |t: &str| -> Result<(u32, u32), String> {
        match t.split_once('/') {
            Some((a, b)) => Ok((whole(a)?, whole(b)?)),
            None => {
                let a = whole(t)?;
                Ok((a, a))
            }
        }
    };
    Ok(match kind {
        SegmentKind::Bpm => Segment::Bpm(parse_number(t)?),
        SegmentKind::Stop => Segment::Stop(parse_number(t)?),
        SegmentKind::Delay => Segment::Delay(parse_number(t)?),
        SegmentKind::Warp => Segment::Warp(Tick::from_beat_f64(parse_number(t)?)),
        SegmentKind::Fake => Segment::Fake(Tick::from_beat_f64(parse_number(t)?)),
        SegmentKind::Scroll => Segment::Scroll(parse_number(t)?),
        SegmentKind::Speed => {
            let parts: Vec<&str> = t.split_whitespace().collect();
            let ratio = parse_number(parts.first().copied().unwrap_or(""))?;
            let delay = match parts.get(1) {
                Some(d) => parse_number(d)?,
                None => 0.0,
            };
            let unit = match parts.get(2).map(|u| u.to_ascii_lowercase()) {
                Some(u) if u.starts_with('s') => ddi_chart::SpeedUnit::Seconds,
                _ => ddi_chart::SpeedUnit::Beats,
            };
            Segment::Speed { ratio, delay, unit }
        }
        SegmentKind::TimeSignature => {
            let (a, b) = t.split_once('/').ok_or_else(|| "As “3/4”.".to_string())?;
            Segment::TimeSignature(whole(a)?, whole(b)?)
        }
        SegmentKind::TickCount => Segment::TickCount(whole(t)?),
        SegmentKind::Combo => {
            let (a, b) = pair(t)?;
            Segment::Combo(a, b)
        }
        SegmentKind::Label => Segment::Label(t.into()),
    })
}

fn display_bpm_text(b: DisplayBpm) -> String {
    match b {
        DisplayBpm::Actual => String::new(),
        DisplayBpm::Single(v) => trim_number(v),
        DisplayBpm::Range(a, c) => format!("{}-{}", trim_number(a), trim_number(c)),
        DisplayBpm::Random => "*".into(),
    }
}

fn parse_display_bpm(t: &str) -> Result<DisplayBpm, String> {
    let t = t.trim();
    if t.is_empty() {
        return Ok(DisplayBpm::Actual);
    }
    if t == "*" {
        return Ok(DisplayBpm::Random);
    }
    // A range "100-200" (the second number not negative).
    if let Some((a, b)) = t.split_once('-').filter(|(a, _)| !a.is_empty()) {
        return Ok(DisplayBpm::Range(parse_number(a)?, parse_number(b)?));
    }
    Ok(DisplayBpm::Single(parse_number(t)?))
}

impl Screen for EditorScreen {
    fn name(&self) -> &str {
        "editor"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.shown = self.editor.borrow().cursor().0 as f64;
        let mut root = self.tree(cx);
        super::entrance(&mut root, &self.env);
        self.publish();
        root
    }

    fn key_releases(&self) -> bool {
        true
    }

    fn animating(&self) -> bool {
        // Frames come while the field glides or follows the music.
        self.listening || (self.shown - self.editor.borrow().cursor().0 as f64).abs() > 0.01
    }

    fn wake_at(&self) -> Option<f64> {
        // The autosave, while changes are not kept.
        let e = self.editor.borrow();
        let autosave = (e.is_dirty() && e.revision() != self.autosaved.0 && self.host.is_some())
            .then_some(self.autosaved.1 + autosave::EVERY);
        // And the overview, while the shell makes it.
        match (autosave, self.sound_wake()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        // The autosave, while there are changes not kept yet.
        let (dirty, revision) = {
            let e = self.editor.borrow();
            (e.is_dirty(), e.revision())
        };
        if dirty && revision != self.autosaved.0 && cx.time() - self.autosaved.1 >= autosave::EVERY
        {
            self.autosaved = (revision, cx.time());
            self.call(EditorCall::Autosave, cx);
        }
        self.sound_update(cx);
        if self.listening {
            match self.host.as_ref().and_then(|h| h.music_time()) {
                Some(t) => {
                    let beat = self.editor.borrow().timing().beat_at(t);
                    self.shown = beat * TICKS_PER_BEAT as f64;
                    self.heard = Some(t);
                    self.pass_notes(t);
                }
                None => self.stop_listening(cx),
            }
        } else {
            let target = self.editor.borrow().cursor().0 as f64;
            if self.env.reduced() {
                self.shown = target;
            } else {
                // Glides there in about a tenth of a second.
                let k = 1.0 - (-f64::from(dt) * 30.0).exp();
                self.shown += (target - self.shown) * k;
                if (target - self.shown).abs() < 0.05 {
                    self.shown = target;
                }
            }
        }
        self.refresh(root, cx);
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        let id = match m.get("Id") {
            Some(Param::Str(s)) => s.clone(),
            _ => String::new(),
        };
        match m.name.as_str() {
            CHANGED => self.changed(),
            ACTION => self.action(&id, cx),
            TIMING => self.timing_action(&id, cx),
            SONG => self.song_action(&id, cx),
            CHART => self.chart_action(&id, cx),
            LEAVE => {
                self.gone();
                cx.close(true);
                return;
            }
            SAVED => match (m.get("Revision"), m.get("Error")) {
                (_, Some(Param::Str(e))) => self.say(format!("Not saved: {e}")),
                (Some(Param::Int(r)), _) => {
                    let r = (*r >= 0).then_some(*r as u64);
                    self.editor.borrow_mut().mark_saved_at(r);
                    self.place.discard = None;
                    self.say("Saved");
                }
                _ => {}
            },
            _ => {}
        }
        self.refresh(root, cx);
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let handled = match input {
            Input::Menu { action, .. } => {
                let shift = self.shift;
                match action {
                    MenuAction::Up => self.edit(|e| e.move_snaps(-1, shift)),
                    MenuAction::Down => self.edit(|e| e.move_snaps(1, shift)),
                    MenuAction::Left => {
                        self.edit(|e| e.change_snap(-1));
                        let s = snap_name(self.editor.borrow().snap());
                        self.say(format!("Snap {s}"));
                    }
                    MenuAction::Right => {
                        self.edit(|e| e.change_snap(1));
                        let s = snap_name(self.editor.borrow().snap());
                        self.say(format!("Snap {s}"));
                    }
                    MenuAction::Start => self.listen(cx),
                    MenuAction::Select => self.open_menu(cx),
                    MenuAction::Back => self.back(cx),
                }
                Handled::Yes
            }
            Input::Key(k) => self.key(k, cx),
            Input::Pointer {
                target,
                gesture,
                pos,
                ..
            } => self.pointer(target, gesture, *pos, cx),
            Input::Wheel {
                target,
                delta,
                kind,
                ..
            } if target.as_ref().is_some_and(|t| t.within("FieldLayer")) => {
                let steps = match kind {
                    WheelKind::Notch => delta[1].signum() as i64,
                    WheelKind::Precise => {
                        self.scroll += delta[1];
                        let px = {
                            let e = self.editor.borrow();
                            let beat_h = self.mapping(e.timing()).beat_height(e.timing());
                            e.step() as f32 / TICKS_PER_BEAT as f32 * beat_h
                        };
                        let n = (self.scroll / px.max(4.0)).trunc();
                        self.scroll -= n * px.max(4.0);
                        n as i64
                    }
                };
                if steps != 0 {
                    let shift = self.shift;
                    self.edit(|e| e.move_snaps(steps, shift));
                }
                Handled::Yes
            }
            Input::CancelDirections => {
                for lane in std::mem::take(&mut self.held) {
                    self.editor.borrow_mut().release(lane);
                }
                self.shift = false;
                Handled::No
            }
            _ => Handled::No,
        };
        self.refresh(root, cx);
        handled
    }
}

#[path = "editor_sound.rs"]
mod sound;

#[cfg(test)]
#[path = "editor_sound_tests.rs"]
mod sound_tests;

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;
