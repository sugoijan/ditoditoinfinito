//! The editor's audio view (`docs/plans/editor.md`, "The audio view"):
//! the music's waveform and spectrogram in a column beside the lanes,
//! time down the field as the notes are, so a beat's onset and its row
//! read together: when the timing is right, a kick's onset is on its
//! beat's line. G shows or hides it, Shift G shows the waveform, the
//! spectrogram or both; where the field has no room beside the lanes it
//! is drawn behind them, faint.
//!
//! Each of the picture's pixel rows is the span of song seconds between
//! its edges as the field's own [`Mapping`] has them ([`row_spans`]):
//! in the edit view rows evenly by beat, a stop's or a delay's seconds
//! all in its row (they have no height: the hit that starts a stop
//! shows on its row, the rest of the stop folded into it; marked across
//! the column), a warp's beats empty (no seconds); in the play view as
//! play scrolls (a c-mod gives a stop its height). Between
//! the timing's changes the mapping from a second to a row is a straight
//! line (the play view's scroll action bends it afterwards, undone
//! exactly: `ScrollState::distance_at`), so each row's seconds are
//! exact at any zoom, down to the sample. The screen paints the picture
//! (sans IO, [`AudioPicture`]) only when the mapping or the analysis
//! changed, and the shell shows it as the texture [`SOURCE`].

use std::collections::HashMap;
use std::rc::Rc;

use ddi_actor::{Actor, Kind, Rgba};
use ddi_chart::{TICKS_PER_BEAT, Tick, TimingMap};

use super::editor_field::Mapping;
use super::{EditorScreen, quad, with_alpha};
use crate::look::call;
use crate::screen::Cx;
use crate::state::editor::audio::{Analysis, BANDS};

/// The texture source the shell shows the picture as.
pub const SOURCE: &str = "editor-audio";

/// What the view shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AudioMode {
    /// The spectrogram and, nearer the lanes, the waveform.
    #[default]
    Both,
    Wave,
    Spectrogram,
}

impl AudioMode {
    pub fn name(self) -> &'static str {
        match self {
            AudioMode::Both => "both",
            AudioMode::Wave => "waveform",
            AudioMode::Spectrogram => "spectrogram",
        }
    }

    fn next(self) -> AudioMode {
        match self {
            AudioMode::Both => AudioMode::Wave,
            AudioMode::Wave => AudioMode::Spectrogram,
            AudioMode::Spectrogram => AudioMode::Both,
        }
    }

    fn says(self) -> &'static str {
        match self {
            AudioMode::Both => "waveform and spectrogram",
            AudioMode::Wave => "waveform",
            AudioMode::Spectrogram => "spectrogram",
        }
    }
}

/// The picture the shell shows: encoded RGBA rows, straight alpha.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioPicture {
    /// Where, in the surface's physical pixels `[x, y, w, h]` (whole
    /// ones): the image's size.
    pub rect: [f32; 4],
    pub size: [u32; 2],
    pub rgba: Rc<Vec<u8>>,
    /// Painted this many times so far (a new one is a new number).
    pub serial: u64,
}

/// What the audio view shows, for the debug views and tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioDebug {
    /// The analysis is made, and being made.
    pub ready: bool,
    pub coming: bool,
    /// Shown, and what ([`AudioMode::name`]).
    pub shown: bool,
    pub mode: String,
    /// "column", "behind" (the lanes) or "" (not shown).
    pub layout: String,
    /// Where it is drawn, virtual pixels `[x, y, w, h]`.
    pub rect: [f32; 4],
    /// The rows (virtual y of their tops) where the waveform rises past
    /// half the song's loudest after a quiet row (under a tenth):
    /// onsets.
    pub onsets: Vec<f32>,
    /// Pictures painted, and the last one's cost in microseconds (0
    /// where the screen has no clock).
    pub painted: u64,
    pub micros: f64,
}

/// The view's state on the screen.
#[derive(Default)]
pub(super) struct AudioView {
    mode: AudioMode,
    /// Shown or hidden by W; `None`: as the room allows.
    wanted: Option<bool>,
    pub(super) analysis: Option<Rc<Analysis>>,
    pub(super) coming: bool,
    texture: u32,
    /// The timing's seconds where the mapping bends, the timing they
    /// are of and its number (what the pictures are keyed by: the
    /// timing compared each frame, copied only when it changes).
    knots: Option<(TimingMap, u64, Vec<f64>)>,
    /// What the picture was painted for, and the picture.
    painted_for: Option<Key>,
    picture: Option<AudioPicture>,
    /// The picture the shell was told of.
    told: Option<u64>,
    onsets: Vec<f32>,
    serial: u64,
    micros: f64,
    spans: Vec<(u32, f64, f64)>,
    /// The picture's pixels before the last, painted over again once
    /// the shell lets them go.
    spare: Option<Rc<Vec<u8>>>,
    /// While listening in the edit view: rows painted, by their place
    /// on the field's own grid of rows, and what they were painted for.
    scroll: Option<(ScrollKey, HashMap<i64, Row>)>,
}

/// What rows on the edit view's grid are painted for: everything but
/// where the grid is on the screen.
#[derive(Clone, PartialEq)]
struct ScrollKey {
    beat_h: f32,
    scale: f32,
    width: usize,
    timing: u64,
    mode: AudioMode,
    analysis: *const Analysis,
}

/// A row painted: its pixels and the waveform's loudest there.
struct Row {
    pixels: Box<[u8]>,
    level: Option<f32>,
}

/// What a picture is painted from.
#[derive(Clone, PartialEq)]
struct Key {
    mapping: Mapping,
    timing: u64,
    pixels: [f32; 4],
    top: f32,
    scale: f32,
    mode: AudioMode,
    behind: bool,
    analysis: *const Analysis,
}

/// The column's width beside the lanes, virtual pixels.
const COLUMN: [f32; 2] = [120.0, 184.0];
/// The narrowest a lane may get for the column to fit beside them.
const LANE_ROOM: f32 = 48.0;
/// The view's opacity behind the lanes.
const BEHIND: f32 = 0.4;

/// The bins of the picture's colors.
struct Colors {
    ramp: [[u8; 4]; 256],
    wave: [u8; 4],
    rms: [u8; 4],
}

fn bytes(c: Rgba) -> [u8; 4] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl EditorScreen {
    /// The view is shown: asked for (or the room allows it), and the
    /// music's analysis is made or being made.
    pub(super) fn audio_on(&self, compact: bool) -> bool {
        let a = &self.audio;
        (a.analysis.is_some() || a.coming) && a.wanted.unwrap_or(!compact)
    }

    /// The width the column takes from the field's left (0: none, or
    /// behind the lanes: the second).
    pub(super) fn audio_room(&self, field: [f32; 4], compact: bool, room: f32) -> (f32, bool) {
        if !self.audio_on(compact) {
            return (0.0, false);
        }
        let lanes = f32::from(self.editor.borrow().lanes().max(1));
        let w = (field[2] * 0.2).clamp(COLUMN[0], COLUMN[1]);
        if !compact && room - w >= lanes * LANE_ROOM {
            (w, false)
        } else {
            (0.0, true)
        }
    }

    /// G: shown or hidden; Shift G: the next of what it shows (shown).
    pub(super) fn toggle_audio(&mut self, next: bool) {
        if self.audio.analysis.is_none() && !self.audio.coming {
            self.say("No music to show");
            return;
        }
        let on = self.audio_on(self.geo.compact);
        if next {
            self.audio.mode = self.audio.mode.next();
            self.audio.wanted = Some(true);
        } else {
            self.audio.wanted = Some(!on);
        }
        let s = if self.audio.wanted == Some(false) {
            "Audio view hidden".to_string()
        } else {
            format!("Audio view: {}", self.audio.mode.says())
        };
        self.say(s);
    }

    /// The panel's line for the view (`None` where there is no music).
    pub(super) fn audio_line(&self) -> Option<String> {
        if self.audio.analysis.is_none() && !self.audio.coming {
            return None;
        }
        let what = if !self.audio_on(self.geo.compact) {
            "hidden"
        } else if self.audio.analysis.is_none() {
            "reading the music…"
        } else {
            self.audio.mode.says()
        };
        let key = if self.geo.compact { "" } else { " (G)" };
        Some(format!("Audio: {what}{key}"))
    }

    /// The menu's items for the view.
    pub(super) fn audio_items(&self, items: &mut Vec<(&'static str, String)>) {
        if self.audio.analysis.is_none() && !self.audio.coming {
            return;
        }
        let on = self.audio_on(self.geo.compact);
        items.push((
            "audio",
            if on {
                "Hide the audio view (G)".into()
            } else {
                "Show the audio view (G)".into()
            },
        ));
        items.push((
            "audio-mode",
            format!(
                "Audio view: {} (Shift G)",
                (if on {
                    self.audio.mode.next()
                } else {
                    self.audio.mode
                })
                .says()
            ),
        ));
    }

    /// The menu's action `id`, if it is the view's.
    pub(super) fn audio_action(&mut self, id: &str) -> bool {
        match id {
            "audio" => self.toggle_audio(false),
            "audio-mode" => self.toggle_audio(true),
            _ => return false,
        }
        true
    }

    /// The music's analysis from the shell (each update).
    pub(super) fn audio_update(&mut self) {
        let (analysis, coming) = match &self.host {
            Some(h) => (h.audio(), h.audio_coming()),
            None => (None, false),
        };
        let same = match (&analysis, &self.audio.analysis) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        let coming = analysis.is_none() && coming;
        if !same || coming != self.audio.coming {
            self.audio.analysis = analysis;
            self.audio.coming = coming;
            self.changed();
        }
    }

    /// The picture's texture, and the picture painted again when what
    /// it shows moved (before the field is drawn), while the view is
    /// shown.
    pub(super) fn audio_attach(&mut self, cx: &mut Cx) {
        if let Some(pixels) = self.audio_pixels() {
            self.audio.texture = cx.texture(SOURCE, pixels[2].max(pixels[3]));
            self.paint_audio(pixels);
        }
    }

    /// The view's place in physical pixels (whole ones), while shown.
    fn audio_pixels(&self) -> Option<[f32; 4]> {
        let g = &self.geo;
        let [x, y, w, h] = g.audio;
        if w <= 0.0 || h <= 0.0 || self.audio.analysis.is_none() {
            return None;
        }
        let px = |v: f32, o: f32| (o + v * g.scale).round();
        let (x0, y0) = (px(x, g.origin[0]), px(y, g.origin[1]));
        let (x1, y1) = (px(x + w, g.origin[0]), px(y + h, g.origin[1]));
        (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
    }

    /// The shell told of a new picture (or of none).
    pub(super) fn send_audio(&mut self) {
        let picture = match (self.audio_pixels(), self.left) {
            (Some(_), false) => self.audio.picture.clone(),
            _ => None,
        };
        let serial = picture.as_ref().map(|p| p.serial);
        if serial != self.audio.told {
            if let Some(h) = &self.host {
                h.show_audio(picture.as_ref());
            }
            self.audio.told = serial;
        }
    }

    fn paint_audio(&mut self, pixels: [f32; 4]) {
        let Some(analysis) = self.audio.analysis.clone() else {
            return;
        };
        let g = self.geo;
        let e = self.editor.borrow();
        let timing = e.timing();
        let mapping = self.mapping(timing);
        if self.audio.knots.as_ref().is_none_or(|(t, ..)| t != timing) {
            let n = self.audio.knots.as_ref().map_or(0, |(_, n, _)| n + 1);
            self.audio.knots = Some((timing.clone(), n, knots(timing)));
        }
        let timing_n = self.audio.knots.as_ref().map_or(0, |(_, n, _)| *n);
        let key = Key {
            mapping,
            timing: timing_n,
            pixels,
            top: (pixels[1] - g.origin[1]) / g.scale,
            scale: g.scale,
            mode: self.audio.mode,
            behind: g.audio_behind,
            analysis: Rc::as_ptr(&analysis),
        };
        if self.audio.painted_for.as_ref() == Some(&key) {
            return;
        }
        let clock = || self.host.as_ref().and_then(|h| h.clock());
        let started = clock();
        let knots = self.audio.knots.take();
        let cuts = knots.as_ref().map_or(&[][..], |(.., k)| &k[..]);
        let w = pixels[2] as usize;
        let row_h = 1.0 / g.scale;
        let colors = self.audio_colors(&analysis);
        let painter = Painter::new(&analysis, &colors, self.audio.mode, w);
        let mut rgba = self
            .audio
            .spare
            .take()
            .and_then(|r| Rc::try_unwrap(r).ok())
            .unwrap_or_default();
        let (rect, levels) = if self.listening && mapping.play.is_none() {
            // Listening in the edit view: the rows move as one, so
            // those painted before are kept by their place on the
            // field's grid of rows and only new ones painted; the
            // picture on the whole pixel nearest (half a pixel at most
            // from the field's own place).
            let scroll = ScrollKey {
                beat_h: mapping.beat_h,
                scale: g.scale,
                width: w,
                timing: timing_n,
                mode: self.audio.mode,
                analysis: Rc::as_ptr(&analysis),
            };
            let mut rows = match self.audio.scroll.take() {
                Some((k, rows)) if k == scroll => rows,
                _ => HashMap::new(),
            };
            // Row 0's top, physical pixels: tick 0's.
            let base = g.origin[1]
                + (mapping.line_y
                    - (mapping.shown / TICKS_PER_BEAT as f64) as f32 * mapping.beat_h)
                    * g.scale;
            let n0 = (pixels[1] - base).floor() as i64;
            let n1 = (pixels[1] + pixels[3] - base).ceil() as i64;
            let mut n = n0;
            while n < n1 {
                if rows.contains_key(&n) {
                    n += 1;
                    continue;
                }
                let mut m = n;
                while m < n1 && !rows.contains_key(&m) {
                    m += 1;
                }
                let top = (base + n as f32 - g.origin[1]) / g.scale;
                row_spans(
                    &mapping,
                    timing,
                    cuts,
                    [top, top + (m - n) as f32 * row_h],
                    top,
                    row_h,
                    (m - n) as usize,
                    &mut self.audio.spans,
                );
                let mut painted: Vec<Row> = (n..m)
                    .map(|_| Row {
                        pixels: vec![0; w * 4].into_boxed_slice(),
                        level: None,
                    })
                    .collect();
                for (r, spans) in by_row(&self.audio.spans) {
                    if let Some(row) = painted.get_mut(r) {
                        row.level = painter.row(spans, &mut row.pixels);
                    }
                }
                for (i, row) in painted.into_iter().enumerate() {
                    rows.insert(n + i as i64, row);
                }
                n = m;
            }
            let h = (n1 - n0) as usize;
            rows.retain(|k, _| *k >= n0 - h as i64 && *k < n1 + h as i64);
            rgba.clear();
            rgba.reserve(w * h * 4);
            let mut levels = Vec::with_capacity(h);
            for k in n0..n1 {
                let row = &rows[&k];
                rgba.extend_from_slice(&row.pixels);
                levels.push(row.level);
            }
            self.audio.scroll = Some((scroll, rows));
            let y = (base + n0 as f32).round();
            ([pixels[0], y, pixels[2], h as f32], levels)
        } else {
            self.audio.scroll = None;
            let h = pixels[3] as usize;
            let [_, fy, _, fh] = g.field;
            row_spans(
                &mapping,
                timing,
                cuts,
                [fy, fy + fh],
                key.top,
                row_h,
                h,
                &mut self.audio.spans,
            );
            rgba.clear();
            rgba.resize(w * h * 4, 0);
            let mut levels = vec![None; h];
            for (r, spans) in by_row(&self.audio.spans) {
                if r < h {
                    levels[r] = painter.row(spans, &mut rgba[r * w * 4..(r + 1) * w * 4]);
                }
            }
            (pixels, levels)
        };
        drop(e);
        self.audio.knots = knots;
        let top = (rect[1] - g.origin[1]) / g.scale;
        self.audio.onsets = onsets(&levels, mapping.dir() < 0.0)
            .into_iter()
            .take(64)
            .map(|r| top + r as f32 * row_h)
            .collect();
        self.audio.serial += 1;
        let last = self.audio.picture.replace(AudioPicture {
            rect,
            size: [w as u32, levels.len() as u32],
            rgba: Rc::new(rgba),
            serial: self.audio.serial,
        });
        self.audio.spare = last.map(|p| p.rgba);
        self.audio.painted_for = Some(key);
        if let (Some(t0), Some(t1)) = (started, clock()) {
            self.audio.micros = (t1 - t0) * 1e6;
        }
        // The debug view follows (the field moves without the rest).
        let debug = self.audio_debug();
        if let Some(v) = &mut self.env.state.borrow_mut().editor {
            v.audio = debug;
        }
    }

    /// The picture's colors from the look: the spectrogram's ramp, from
    /// the ground up through the theme's warm colors to near white,
    /// and the waveform's.
    fn audio_colors(&self, analysis: &Analysis) -> Colors {
        let look = &self.env.look;
        let pick = |name: &str, default: Rgba| look.color(name).unwrap_or(default);
        let stops = [
            [0.04, 0.035, 0.05, 0.0],
            pick("Editor.Audio.Ramp1", [0.2, 0.09, 0.38, 1.0]),
            pick("Editor.Audio.Ramp2", [0.66, 0.16, 0.5, 1.0]),
            pick("Editor.Audio.Ramp3", [1.0, 0.43, 0.63, 1.0]),
            pick("Editor.Audio.Ramp4", [1.0, 0.82, 0.4, 1.0]),
            pick("Editor.Audio.Ramp5", [1.0, 0.96, 0.86, 1.0]),
        ];
        let levels = analysis.spectrogram.levels();
        let ramp = std::array::from_fn(|v| {
            let l = levels[v] * (stops.len() - 1) as f32;
            let i = (l.floor() as usize).min(stops.len() - 2);
            let t = l - i as f32;
            let (a, b) = (stops[i], stops[i + 1]);
            bytes(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t))
        });
        Colors {
            ramp,
            wave: bytes(pick("Editor.Audio.Wave", [0.45, 0.68, 1.0, 0.85])),
            rms: bytes(pick("Editor.Audio.Rms", [0.82, 0.91, 1.0, 1.0])),
        }
    }

    /// The view on the field: its picture (under the grid's lines, which
    /// the field draws across it too).
    pub(super) fn audio_layer(&self, c: &mut Vec<Actor>) {
        let g = &self.geo;
        let Some(p) = &self.audio.picture else {
            return;
        };
        if self.audio_pixels().is_none() {
            return;
        }
        let [px, py, pw, ph] = p.rect;
        let (x, y) = ((px - g.origin[0]) / g.scale, (py - g.origin[1]) / g.scale);
        let (w, h) = (pw / g.scale, ph / g.scale);
        if !g.audio_behind {
            let mut plate = quad(
                "AudioColumn",
                [g.audio[2], g.audio[3]],
                [0.0, 0.0, 0.0, 0.3],
            );
            call(
                &mut plate,
                "xy",
                &[g.audio[0] + g.audio[2] / 2.0, g.audio[1] + g.audio[3] / 2.0],
            );
            c.push(plate);
        }
        let mut picture = Actor::new(Kind::Sprite(ddi_actor::Sprite::quad([w, h])));
        picture.name = "AudioPicture".into();
        if let Kind::Sprite(s) = &mut picture.kind {
            s.texture = self.audio.texture;
        }
        call(&mut picture, "xy", &[x + w / 2.0, y + h / 2.0]);
        if g.audio_behind {
            call(&mut picture, "diffusealpha", &[BEHIND]);
        }
        c.push(picture);
    }

    /// A line across the column at `y` (the grid's, the cursor's).
    pub(super) fn audio_line_at(&self, c: &mut Vec<Actor>, y: f32, h: f32, color: Rgba) {
        let g = &self.geo;
        if g.audio_behind || g.audio[2] <= 0.0 || self.audio.picture.is_none() {
            return;
        }
        let mut line = quad("", [g.audio[2], h], color);
        call(&mut line, "xy", &[g.audio[0] + g.audio[2] / 2.0, y]);
        c.push(line);
    }

    /// Across the column, where the music is folded or not shown: a
    /// stop's or a delay's row (its seconds have no height there), a
    /// warp's beats.
    pub(super) fn audio_marks(&self, m: &Mapping, timing: &TimingMap, c: &mut Vec<Actor>) {
        let g = &self.geo;
        if g.audio[2] <= 0.0 || self.audio.picture.is_none() {
            return;
        }
        let [x, fy, w, fh] = [g.audio[0], g.field[1], g.audio[2], g.field[3]];
        let y_of = |t: Tick| m.y(timing, t.0 as f64);
        let danger = self.env.color("Danger");
        // A c-mod's play view gives a stop its height: shown there.
        let cmod = m.play.is_some()
            && matches!(
                self.skin.map(|s| s.scroll.speed),
                Some(ddi_engine::scroll::SpeedMod::CMod(_))
            );
        for (tick, seconds) in timing
            .stops
            .iter()
            .chain(&timing.delays)
            .map(|s| (s.tick, s.seconds))
        {
            let y = y_of(tick);
            if seconds > 0.0 && !cmod && y >= fy - 2.0 && y <= fy + fh + 2.0 {
                let mut mark = quad("AudioStop", [w, 3.0], with_alpha(danger, 0.85));
                call(&mut mark, "xy", &[x + w / 2.0, y]);
                c.push(mark);
            }
        }
        for warp in &timing.warps {
            let (a, b) = (y_of(warp.tick), y_of(warp.tick + warp.length));
            let (a, b) = (a.min(b).max(fy), a.max(b).min(fy + fh));
            if b > a {
                let mut band = quad("", [w, b - a], with_alpha(danger, 0.14));
                call(&mut band, "xy", &[x + w / 2.0, (a + b) / 2.0]);
                c.push(band);
            }
        }
    }

    /// The view for the debug state.
    pub(super) fn audio_debug(&self) -> AudioDebug {
        let g = &self.geo;
        let shown = self.audio_pixels().is_some();
        AudioDebug {
            ready: self.audio.analysis.is_some(),
            coming: self.audio.coming,
            shown,
            mode: self.audio.mode.name().into(),
            layout: match (shown, g.audio_behind) {
                (false, _) => String::new(),
                (true, false) => "column".into(),
                (true, true) => "behind".into(),
            },
            rect: if shown { g.audio } else { [0.0; 4] },
            onsets: if shown {
                self.audio.onsets.clone()
            } else {
                Vec::new()
            },
            painted: self.audio.serial,
            micros: self.audio.micros,
        }
    }
}

/// The song seconds where `timing`'s mapping from seconds to beats
/// bends or jumps (each change's row, before and after a delay or a
/// stop on it; a warp's start and end), in order.
pub fn knots(timing: &TimingMap) -> Vec<f64> {
    let mut ticks: Vec<Tick> = timing.bpms.iter().map(|s| s.tick).collect();
    ticks.extend(timing.stops.iter().map(|s| s.tick));
    ticks.extend(timing.delays.iter().map(|s| s.tick));
    ticks.extend(timing.scrolls.iter().map(|s| s.tick));
    for w in &timing.warps {
        ticks.extend([w.tick, w.tick + w.length]);
    }
    ticks.sort();
    ticks.dedup();
    let mut out = Vec::with_capacity(ticks.len() * 3);
    for t in ticks {
        let at = timing.seconds_at(t);
        let stop: f64 = timing
            .stops
            .iter()
            .filter(|s| s.tick == t)
            .map(|s| s.seconds)
            .sum();
        out.extend([timing.seconds_before_delay(t), at, at + stop]);
    }
    out.retain(|s| s.is_finite());
    out.sort_by(f64::total_cmp);
    out.dedup();
    out
}

/// The screen's y of song second `s` as the mapping places a note heard
/// then, through `pre` (the value that moves in a straight line with
/// the seconds between knots) and `post` (that value to the screen).
#[derive(Clone, Copy)]
enum Line<'a> {
    Edit(&'a Mapping),
    Play(&'a Mapping, ddi_engine::scroll::ScrollState, f32),
}

impl Line<'_> {
    fn of(m: &Mapping) -> Line<'_> {
        match m.play {
            Some((s, dir)) => Line::Play(m, s, dir),
            None => Line::Edit(m),
        }
    }

    /// Beats in the edit view; the distance before the scroll action in
    /// the play view.
    fn pre(&self, timing: &TimingMap, s: f64) -> f64 {
        match self {
            Line::Edit(_) => timing.beat_at(s),
            Line::Play(_, state, _) => state.distance(timing.displayed_beat(timing.beat_at(s)), s),
        }
    }

    fn post(&self, u: f64) -> f64 {
        match self {
            Line::Edit(m) => {
                f64::from(m.line_y) + (u - m.shown / TICKS_PER_BEAT as f64) * f64::from(m.beat_h)
            }
            Line::Play(m, state, dir) => {
                f64::from(m.line_y)
                    + f64::from(state.arrows(u)) * f64::from(m.lane_w) * f64::from(*dir)
            }
        }
    }

    /// [`Line::post`] undone.
    fn unpost(&self, y: f64) -> f64 {
        match self {
            Line::Edit(m) => {
                (y - f64::from(m.line_y)) / f64::from(m.beat_h) + m.shown / TICKS_PER_BEAT as f64
            }
            Line::Play(m, state, dir) => state
                .distance_at((y - f64::from(m.line_y)) / (f64::from(m.lane_w) * f64::from(*dir))),
        }
    }
}

/// The song seconds each of `rows` rows covers, the first's top at `top`
/// and each `row_h` high (virtual pixels), in the field `[top, bottom]`
/// as `m` maps it: `(row, from, to)` for each span (a row can have more
/// than one: either side of a stop), by row, then by second. Seconds
/// with no height (a stop's, a delay's) are all in the row they are at;
/// a warp's rows (no seconds) have none.
#[allow(clippy::too_many_arguments)]
pub fn row_spans(
    m: &Mapping,
    timing: &TimingMap,
    knots: &[f64],
    field: [f32; 2],
    top: f32,
    row_h: f32,
    rows: usize,
    out: &mut Vec<(u32, f64, f64)>,
) {
    out.clear();
    if rows == 0 || row_h <= 0.0 {
        return;
    }
    let line = Line::of(m);
    // The seconds in view, a beat beyond either end.
    let (first, last) = m.span(timing, field[0].min(top), field[1]);
    let s_lo = timing.seconds_before_delay(Tick(first - TICKS_PER_BEAT));
    let s_hi = {
        let t = Tick(last + TICKS_PER_BEAT);
        let stop: f64 = timing
            .stops
            .iter()
            .filter(|s| s.tick == t)
            .map(|s| s.seconds)
            .sum();
        timing.seconds_at(t) + stop
    };
    if s_hi <= s_lo {
        return;
    }
    let mut cuts: Vec<f64> = vec![s_lo];
    let from = knots.partition_point(|k| *k <= s_lo);
    cuts.extend(knots[from..].iter().take_while(|k| **k < s_hi));
    if let Line::Play(_, state, _) = line {
        // The scroll action bends at the line.
        let now = state.song_time();
        if now > s_lo && now < s_hi {
            cuts.push(now);
        }
    }
    cuts.push(s_hi);
    cuts.sort_by(f64::total_cmp);
    let (top, row_h) = (f64::from(top), f64::from(row_h));
    for piece in cuts.windows(2) {
        let (a, b) = (piece[0], piece[1]);
        if b - a <= 1e-9 {
            continue;
        }
        // A straight line through two seconds inside.
        let (p1, p2) = (a + (b - a) * 0.25, a + (b - a) * 0.75);
        let (u1, u2) = (line.pre(timing, p1), line.pre(timing, p2));
        let k = (u2 - u1) / (p2 - p1);
        let (ua, ub) = (u1 - k * (p1 - a), u2 + k * (b - p2));
        let (ya, yb) = (line.post(ua), line.post(ub));
        if (yb - ya).abs() < 1e-6 || k == 0.0 {
            // No height (a stop, a delay): its seconds in its row alone,
            // where the hit that starts a stop or ends a delay is.
            let r = ((ya - top) / row_h).floor();
            if r >= 0.0 && r < rows as f64 {
                out.push((r as u32, a, b));
            }
            continue;
        }
        let (ylo, yhi) = (ya.min(yb), ya.max(yb));
        let r0 = ((ylo - top) / row_h).floor().max(0.0);
        let r1 = ((yhi - top) / row_h).ceil().min(rows as f64);
        let mut r = r0;
        while r < r1 {
            let (y0, y1) = (top + r * row_h, top + (r + 1.0) * row_h);
            let s0 = a + (line.unpost(y0) - ua) / k;
            let s1 = a + (line.unpost(y1) - ua) / k;
            let (s0, s1) = (s0.min(s1).max(a), s0.max(s1).min(b));
            if s1 > s0 {
                out.push((r as u32, s0, s1));
            }
            r += 1.0;
        }
    }
    out.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)));
}

/// `spans` (by row, [`row_spans`]) a row at a time: the row and its
/// spans.
fn by_row(spans: &[(u32, f64, f64)]) -> impl Iterator<Item = (usize, &[(u32, f64, f64)])> {
    spans
        .chunk_by(|a, b| a.0 == b.0)
        .map(|c| (c[0].0 as usize, c))
}

/// The rows where the waveform's loudest (`levels`, a row each) rises
/// past half the song's after a quiet one (under a tenth), in the
/// music's order (up the picture when `reverse`).
fn onsets(levels: &[Option<f32>], reverse: bool) -> Vec<u32> {
    let mut out = Vec::new();
    let mut armed = true;
    let mut visit = |row: usize| match levels[row] {
        Some(l) if l >= 0.5 && armed => {
            out.push(row as u32);
            armed = false;
        }
        Some(l) if l < 0.1 => armed = true,
        _ => {}
    };
    if reverse {
        (0..levels.len()).rev().for_each(&mut visit);
    } else {
        (0..levels.len()).for_each(&mut visit);
    }
    out.sort_unstable();
    out
}

/// Paints rows of the picture: the spectrogram low to high left to
/// right, the waveform round its middle.
struct Painter<'a> {
    analysis: &'a Analysis,
    colors: &'a Colors,
    spec_w: usize,
    wave_x: usize,
    wave_w: usize,
    width: usize,
    /// Each of the spectrogram's columns: its band.
    columns: Vec<usize>,
}

impl<'a> Painter<'a> {
    fn new(
        analysis: &'a Analysis,
        colors: &'a Colors,
        mode: AudioMode,
        width: usize,
    ) -> Painter<'a> {
        let (spec_w, wave_x) = match mode {
            AudioMode::Both => {
                let s = width * 11 / 20;
                (s, s + 2)
            }
            AudioMode::Wave => (0, 0),
            AudioMode::Spectrogram => (width, width),
        };
        Painter {
            analysis,
            colors,
            spec_w,
            wave_x,
            wave_w: width.saturating_sub(wave_x),
            width,
            columns: (0..spec_w)
                .map(|x| ((x * BANDS + BANDS / 2) / spec_w.max(1)).min(BANDS - 1))
                .collect(),
        }
    }

    /// One row of `spans` into `line` (its pixels, cleared); the
    /// waveform's loudest there.
    fn row(&self, spans: &[(u32, f64, f64)], line: &mut [u8]) -> Option<f32> {
        let mut peak: Option<crate::state::editor::audio::Peak> = None;
        let mut any = false;
        let mut bands = [0u8; BANDS];
        let mut merged = [0u8; BANDS];
        for &(_, a, b) in spans {
            if self.wave_w > 0
                && let Some(p) = self.analysis.wave(a, b)
            {
                peak = Some(peak.map_or(p, |q| q.merge(p)));
            }
            if self.spec_w > 0 && self.analysis.spectrum(a, b, &mut bands) {
                any = true;
                for (m, v) in merged.iter_mut().zip(&bands) {
                    *m = (*m).max(*v);
                }
            }
        }
        if any {
            let mut row_colors = [[0u8; 4]; BANDS];
            for (c, v) in row_colors.iter_mut().zip(&merged) {
                *c = self.colors.ramp[usize::from(*v)];
            }
            let spec = &mut line[..self.spec_w * 4];
            for (px, &b) in spec.as_chunks_mut::<4>().0.iter_mut().zip(&self.columns) {
                *px = row_colors[b];
            }
        }
        if let (Some(p), true) = (peak, self.wave_w > 2) {
            let w = self.width;
            let half = (self.wave_w as f32 - 1.0) / 2.0;
            let mid = self.wave_x as f32 + half;
            let x_of = |v: f32| (mid + v.clamp(-1.0, 1.0) * half).round() as usize;
            let mut fill = |x0: usize, x1: usize, c: &[u8; 4]| {
                for px in line[x0 * 4..(x1.min(w - 1) + 1) * 4].as_chunks_mut::<4>().0 {
                    *px = *c;
                }
            };
            fill(x_of(p.min), x_of(p.max).max(x_of(p.min)), &self.colors.wave);
            fill(x_of(-p.rms), x_of(p.rms), &self.colors.rms);
        }
        peak.map(|p| p.max.max(-p.min))
    }
}

#[cfg(test)]
#[path = "editor_audio_tests.rs"]
mod tests;
