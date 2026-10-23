//! The editor's sound tools (`docs/plans/editor.md`, step 5): the
//! metronome and the assist clap while listening (K, Shift K; the shell
//! schedules them, `ddi_session::listen::Cues`), tap-to-tempo (Y tapped
//! along, Shift Y applies the BPM at the cursor; the menu also applies
//! the offset the taps heard), and the music's overview: a strip at the
//! field's right edge, the song's start at its top, its loudness, the
//! rows in view and the cursor; a tap or a drag on it moves the cursor
//! there.

use std::rc::Rc;

use ddi_actor::Actor;
use ddi_chart::{TICKS_PER_BEAT, Tick};

use super::{EditorCall, EditorScreen, frame, trim_number, with_alpha};
use crate::gesture::Gesture;
use crate::look::{call, quad};
use crate::screen::Cx;
use crate::state::editor::Sounds;
use crate::state::editor::overview::Overview;
use crate::state::editor::tap::{Estimate, TapTempo};

/// How often the screen asks for the overview while the shell makes it,
/// seconds.
const POLL: f64 = 0.25;

/// The sound tools' state on the screen.
#[derive(Default)]
pub(super) struct SoundTools {
    pub(super) taps: TapTempo,
    pub(super) overview: Option<Rc<Overview>>,
    /// What the cues were last given for while listening: the revision,
    /// the chart, the sounds.
    cued: Option<(Option<u64>, usize, Sounds)>,
    /// The UI's seconds at the last update, and whether the shell was
    /// making the overview then.
    now: f64,
    awaiting: bool,
}

/// The field less the overview's strip at its right edge, and the strip.
pub(super) fn carve(field: [f32; 4], compact: bool) -> ([f32; 4], [f32; 4]) {
    let (w, gap) = if compact { (12.0, 4.0) } else { (16.0, 8.0) };
    let [x, y, fw, h] = field;
    let fw = (fw - w - gap).max(0.0);
    ([x, y, fw, h], [x + fw + gap, y, w, h])
}

/// A tempo as the panel and the menu say it.
fn bpm_text(bpm: f64) -> String {
    format!("{bpm:.2}")
}

/// A tapped BPM as it is applied: to the hundredth.
fn rounded(bpm: f64) -> f64 {
    (bpm * 100.0).round() / 100.0
}

impl EditorScreen {
    fn cue_key(&self) -> (Option<u64>, usize, Sounds) {
        let e = self.editor.borrow();
        (e.revision(), e.chart_index(), e.sounds())
    }

    /// Listening starts: the cues are the shell's to read from the
    /// editor, and the taps start again (their seconds heard change).
    pub(super) fn listen_started(&mut self) {
        self.sound.cued = Some(self.cue_key());
        self.sound.taps.clear();
    }

    /// The metronome (or with `clap`, the assist clap) on or off.
    pub(super) fn toggle_sound(&mut self, clap: bool) {
        let mut s = self.editor.borrow().sounds();
        let (name, on) = if clap {
            s.clap = !s.clap;
            ("Assist clap", s.clap)
        } else {
            s.metronome = !s.metronome;
            ("Metronome", s.metronome)
        };
        self.edit(|e| e.set_sounds(s));
        self.say(format!("{name} {}", if on { "on" } else { "off" }));
    }

    /// A tap of the tempo key at `time_ms` (the key's time stamp, else
    /// the UI's), at the second heard while listening.
    pub(super) fn tap(&mut self, time_ms: f64, cx: &mut Cx) {
        let at = if time_ms > 0.0 {
            time_ms / 1000.0
        } else {
            cx.time()
        };
        let heard = if self.listening {
            self.host.as_ref().and_then(|h| h.music_time())
        } else {
            None
        };
        self.sound.taps.tap(at, heard);
        self.changed();
    }

    fn estimate(&self) -> Option<Estimate> {
        self.sound.taps.estimate()
    }

    /// The tapped tempo as the BPM at the cursor; with `offset`, the
    /// offset its taps heard too.
    pub(super) fn apply_tap(&mut self, offset: bool) {
        let Some(est) = self.estimate() else {
            self.say("Tap Y along to the beat first");
            return;
        };
        let bpm = rounded(est.bpm);
        let heard = est.heard.filter(|_| offset);
        let retime = self.retime.get();
        let mut taken = false;
        self.edit(|e| taken = e.set_tapped_tempo(bpm, heard, retime));
        if !taken {
            self.say("That tempo cannot be kept in a file");
            return;
        }
        let beat = trim_number(self.editor.borrow().cursor().beat());
        self.say(if heard.is_some() {
            let o = trim_number(self.editor.borrow().timing().offset_seconds);
            format!("{} BPM at beat {beat}, offset {o} s", trim_number(bpm))
        } else {
            format!("{} BPM at beat {beat}", trim_number(bpm))
        });
    }

    /// The panel's lines: the sounds, and the taps when there are any
    /// (name, text, color). On a small screen one at most: the taps, else
    /// the sounds when one is on (the menu has them).
    pub(super) fn sound_lines(&self) -> Vec<(&'static str, String, &'static str)> {
        let mut lines = Vec::new();
        let s = self.editor.borrow().sounds();
        if self.place.listen && (!self.geo.compact || s.metronome || s.clap) {
            let on = |b: bool| if b { "on" } else { "off" };
            lines.push((
                "Panel.Sounds",
                format!("Metronome {} · clap {}", on(s.metronome), on(s.clap)),
                if s.metronome || s.clap {
                    "Accent"
                } else {
                    "Muted"
                },
            ));
        }
        let taps = self.sound.taps.len();
        if taps > 0 {
            let text = match self.estimate() {
                Some(est) => {
                    let mut t = format!("Tapped {} BPM ({taps} taps)", bpm_text(est.bpm));
                    if let Some(timing) = est.heard.and_then(|h| {
                        self.editor
                            .borrow()
                            .tapped_timing(rounded(est.bpm), Some(h))
                    }) {
                        t.push_str(&format!(
                            " · offset {} s",
                            trim_number(timing.offset_seconds)
                        ));
                    }
                    t
                }
                None => format!("Tapping… ({taps})"),
            };
            lines.push(("Panel.Tap", text, "Accent"));
        }
        if self.geo.compact && lines.len() > 1 {
            // One line where there is little room: the taps'.
            lines.remove(0);
        }
        lines
    }

    /// The menu's items for the sounds and the taps.
    pub(super) fn sound_items(&self, items: &mut Vec<(&'static str, String)>) {
        if self.place.listen {
            let s = self.editor.borrow().sounds();
            let on = |b: bool| if b { "on" } else { "off" };
            items.push(("metronome", format!("Metronome: {} (K)", on(s.metronome))));
            items.push(("clap", format!("Assist clap: {} (Shift K)", on(s.clap))));
        }
        if let Some(est) = self.estimate() {
            let bpm = bpm_text(rounded(est.bpm));
            items.push(("tap-bpm", format!("Apply tapped BPM: {bpm} (Shift Y)")));
            if est.heard.is_some() {
                items.push(("tap-offset", format!("Apply tapped BPM ({bpm}) and offset")));
            }
        }
    }

    /// The menu's action `id`, if it is one of the sound tools'.
    pub(super) fn sound_action(&mut self, id: &str) -> bool {
        match id {
            "metronome" => self.toggle_sound(false),
            "clap" => self.toggle_sound(true),
            "tap-bpm" => self.apply_tap(false),
            "tap-offset" => self.apply_tap(true),
            _ => return false,
        }
        true
    }

    /// Each update: the overview once the shell has it, and the cues
    /// told again when what they come from changed while listening.
    pub(super) fn sound_update(&mut self, cx: &mut Cx) {
        self.sound.now = cx.time();
        let overview = self.host.as_ref().and_then(|h| h.overview());
        let same = match (&overview, &self.sound.overview) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        self.sound.awaiting =
            overview.is_none() && self.host.as_ref().is_some_and(|h| h.overview_coming());
        if !same {
            self.sound.overview = overview;
            self.changed();
        }
        if self.listening {
            let key = self.cue_key();
            if self.sound.cued != Some(key) {
                self.sound.cued = Some(key);
                self.call(EditorCall::Cues, cx);
            }
        }
    }

    /// When to look for the overview again while the shell makes it.
    pub(super) fn sound_wake(&self) -> Option<f64> {
        self.sound.awaiting.then_some(self.sound.now + POLL)
    }

    /// The song seconds the strip spans: from the music's start (or beat
    /// 0 before it) to its end (or the last note after it).
    fn overview_span(&self) -> (f64, f64) {
        let e = self.editor.borrow();
        let t = e.timing();
        let start = t.seconds_at(Tick::ZERO).min(0.0);
        let music = self.sound.overview.as_ref().map_or(0.0, |o| o.end());
        let end = music.max(t.seconds_at(e.end())).max(start + 1.0);
        (start, end)
    }

    /// The strip: its plate and the music's loudness.
    pub(super) fn overview_strip(&self) -> Actor {
        let [x, y, w, h] = self.geo.overview;
        let env = &self.env;
        let mut c = Vec::new();
        let mut plate = quad(
            "OverviewPlate",
            [w, h],
            with_alpha(env.color("Plate"), 0.55),
        );
        call(&mut plate, "xy", &[x + w / 2.0, y + h / 2.0]);
        c.push(plate);
        if let Some(o) = &self.sound.overview {
            let (s0, s1) = self.overview_span();
            let n = (h / 3.0).floor().clamp(16.0, 400.0) as usize;
            let bar_h = h / n as f32;
            let color = with_alpha(env.color("Muted"), 0.8);
            for i in 0..n {
                let at = |i: usize| s0 + (s1 - s0) * i as f64 / n as f64;
                let level = o.level(at(i), at(i + 1));
                if level < 0.02 {
                    continue;
                }
                let mut bar = quad("", [(w - 4.0) * level, bar_h + 0.5], color);
                call(&mut bar, "xy", &[x + w / 2.0, y + bar_h * (i as f32 + 0.5)]);
                c.push(bar);
            }
        }
        frame("Overview", c)
    }

    /// Over the strip: the rows the field shows, and the cursor line
    /// (drawn again as the field moves).
    pub(super) fn overview_marks(&self) -> Actor {
        let [x, y, w, h] = self.geo.overview;
        let [_, fy, _, fh] = self.geo.field;
        let env = &self.env;
        let (s0, s1) = self.overview_span();
        let y_of = |s: f64| y + (((s - s0) / (s1 - s0)) as f32 * h).clamp(0.0, h);
        let (top, bottom, shown) = {
            let e = self.editor.borrow();
            let t = e.timing();
            let at = |tick: f64| t.seconds_at_beat(tick / TICKS_PER_BEAT as f64);
            (
                at(self.tick_at_y(fy)),
                at(self.tick_at_y(fy + fh)),
                at(self.shown),
            )
        };
        let (a, b) = (y_of(top.min(bottom)), y_of(top.max(bottom)));
        let mut c = Vec::new();
        let mut band = quad(
            "",
            [w, (b - a).max(2.0)],
            with_alpha(env.color("Accent"), 0.22),
        );
        call(&mut band, "xy", &[x + w / 2.0, (a + b) / 2.0]);
        c.push(band);
        let mut line = quad("", [w + 4.0, 2.0], with_alpha(env.color("Accent"), 0.95));
        call(&mut line, "xy", &[x + w / 2.0, y_of(shown)]);
        c.push(line);
        frame("OverviewMarks", c)
    }

    /// A pointer on the strip: the cursor to the second under it (the
    /// listening stopped).
    pub(super) fn overview_pointer(&mut self, gesture: &Gesture, pos: [f32; 2], cx: &mut Cx) {
        if !matches!(
            gesture,
            Gesture::Press | Gesture::Tap | Gesture::DragStart | Gesture::Drag { .. }
        ) {
            return;
        }
        let [_, y, _, h] = self.geo.overview;
        let p = pos[1] / self.geo.zoom;
        let (s0, s1) = self.overview_span();
        let s = s0 + f64::from(((p - y) / h.max(1.0)).clamp(0.0, 1.0)) * (s1 - s0);
        self.stop_listening(cx);
        let tick = self.editor.borrow().tick_at_seconds(s);
        self.edit(|e| e.set_cursor(tick, false));
    }
}
