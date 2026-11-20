//! The music's volume during a play (`docs/plans/play-hud.md`, "The
//! music's volume", round AB, 2026-11-19): a pointer-only control beside
//! the play's quit button, for the mouse and touch screens alike and no
//! key or controller (the maintainer's choice: the play's keys are its
//! lanes'). It shows exactly when and where the quit button shows
//! (throughout on a touch screen, for two seconds after the mouse moved,
//! `ddi_session::play::quit_shown`), next to it away from the receptors'
//! row; a press on its button opens its slider, a press on the button
//! with the slider open mutes or brings the level back (the level a mute
//! kept, `ddi_ui::widgets::volume::kept_level`), as song select's quick
//! volume does for a tap; a press or a drag along the slider sets the
//! level, kept in the settings when it is let go. The slider closes by
//! itself with the buttons, after [`IDLE`] untouched, and at a press
//! elsewhere (which goes on to do what it does: a lane's press presses
//! the lane).
//!
//! The shells hit-test it as they hit-test the quit button, from where the
//! frame drew it ([`VolumeAt`], in `Drawn`), so the play's surface stays
//! the play's: nothing else of the screens hovers or takes a press. The
//! level is the one Music volume setting (`Settings::volume`); a change
//! reaches the play's sound through [`apply`] (the session's
//! `set_volume`), never its clock, judging or log.

use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_platform::AudioBackend;
use ddi_render::draw2d::{Affine, DrawList, Quad, Shape};
use ddi_session::play::{Inputs, PlaySession};
use ddi_ui::state::settings::{Settings, slider_to_volume, volume_to_slider};
use ddi_ui::widgets::volume::{VolumeChange, keep_level, kept_level};

/// Seconds the slider stays open untouched (a touch screen's buttons
/// never hide).
pub const IDLE: f64 = 3.0;

/// The gap between the quit button and this one, and around the slider,
/// logical pixels.
const GAP: f32 = 8.0;
/// The slider's track, its plate's width and its padding, logical pixels.
const TRACK: f32 = 120.0;
const PLATE_W: f32 = 44.0;
const PAD: f32 = 14.0;
/// Room for the level's percentage at the plate's far end.
const VALUE_ROOM: f32 = 22.0;
/// The percentage's size, logical pixels.
const VALUE_SIZE: f32 = 12.0;

/// Where the control is, target pixels: the button (middle and radius),
/// and the slider's plate (left, top, right, bottom) with its track (x,
/// the y of level 0 and the y of level 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub button: [f32; 3],
    pub plate: [f32; 4],
    pub track: [f32; 3],
}

/// Where the shells hit-test it, logical pixels: the button (middle and
/// radius), and the slider's plate while it is open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeAt {
    pub button: [f32; 3],
    pub plate: Option<[f32; 4]>,
}

impl VolumeAt {
    /// Whether `pos` (logical pixels) is on the control.
    pub fn holds(&self, [x, y]: [f32; 2]) -> bool {
        let [bx, by, r] = self.button;
        (x - bx).powi(2) + (y - by).powi(2) <= r * r
            || self
                .plate
                .is_some_and(|[l, t, r, b]| (l..=r).contains(&x) && (t..=b).contains(&y))
    }
}

/// The control beside the quit button `quit` (target pixels: middle and
/// radius) on a target `size` wide and tall, `ratio` target pixels a
/// logical one: the button the quit button's size, toward the screen's
/// middle from it (below a quit button in the top half, above one in the
/// bottom half: away from the receptors' row, which the quit button
/// already keeps clear of); the slider along the same way where it fits,
/// else beside the button, on the screen.
pub fn place(quit: [f32; 3], size: [f32; 2], ratio: f32) -> Place {
    place_way(quit, size, ratio, quit[1] <= size[1] / 2.0)
}

/// [`place`] below the quit button (`down`) or above it, whatever half
/// it is in: above it while touch controls show under it
/// (`docs/plans/touch-controls.md`).
pub fn place_way(quit: [f32; 3], [w, h]: [f32; 2], ratio: f32, down: bool) -> Place {
    let [qx, qy, r] = quit;
    let gap = GAP * ratio;
    let way = if down { 1.0 } else { -1.0 };
    let by = qy + way * (2.0 * r + gap);
    let button = [qx, by, r];
    let pw = PLATE_W * ratio;
    let ph = (TRACK + 2.0 * PAD + VALUE_ROOM) * ratio;
    let margin = 8.0 * ratio;
    // Along the way, from the button's edge.
    let near = by + way * (r + gap / 2.0);
    let far = near + way * ph;
    let (top, left) = if far.min(near) >= margin && far.max(near) <= h - margin {
        (near.min(far), qx - pw / 2.0)
    } else {
        // Beside it, on the screen.
        let top = (by - ph / 2.0).clamp(margin, (h - margin - ph).max(margin));
        let left = (qx + r + gap).min(w - margin - pw).max(margin);
        (top, left)
    };
    let plate = [left, top, left + pw, top + ph];
    // The level's percentage at the top, the track under it.
    let x = left + pw / 2.0;
    let low = top + ph - PAD * ratio;
    let high = low - TRACK * ratio;
    Place {
        button,
        plate,
        track: [x, low, high],
    }
}

/// Whether a play has the control at all (while the quit button shows):
/// never where its volume does not change (calibration, whose click track
/// is left as it is: `adjustable` false), and on a touch screen (`touch`,
/// where the quit button never hides) not in a room's stage, played or
/// watched (`stage`): there it would stay over the opponents' miniatures,
/// and the stage's Ready card has the volume (the maintainer, round AD,
/// 2026-11-19: "Hide it in stages on touch"). With a mouse it shows
/// everywhere, as the quit button does, for a moment after it moved.
pub fn offered(adjustable: bool, touch: bool, stage: bool) -> bool {
    adjustable && !(touch && stage)
}

/// What a pointer does on the play (logical pixels).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    Cancel,
}

/// A pointer's edge for the control, as a shell saw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub phase: Phase,
    /// Logical pixels.
    pub pos: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    Button,
    Track,
}

/// The control's state, one a play view ([`crate::play_view::PlayView`]).
#[derive(Debug)]
pub struct PlayVolume {
    /// The slider position, `0..=1`, in hundredths (Options' slider).
    position: f32,
    open: bool,
    held: Option<Held>,
    /// When it was last touched (the play's host seconds).
    touched: f64,
    /// Where the last frame drew it, target pixels, and the target
    /// pixels a logical one then.
    place: Option<Place>,
    ratio: f32,
}

impl Default for PlayVolume {
    fn default() -> PlayVolume {
        PlayVolume {
            position: 0.0,
            open: false,
            held: None,
            touched: 0.0,
            place: None,
            ratio: 1.0,
        }
    }
}

fn quantized(p: f32) -> f32 {
    (p.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

impl PlayVolume {
    /// The level shown, in percent of the slider.
    pub fn percent(&self) -> u32 {
        (self.position * 100.0).round() as u32
    }

    /// Whether the slider is open.
    pub fn open(&self) -> bool {
        self.open
    }

    /// Where it is now, for the shells' presses (none while it does not
    /// show).
    pub fn at(&self) -> Option<VolumeAt> {
        let p = self.place?;
        let k = self.ratio.max(f32::EPSILON);
        let [x, y, r] = p.button;
        Some(VolumeAt {
            button: [x / k, y / k, r / k],
            plate: self.open.then(|| p.plate.map(|v| v / k)),
        })
    }

    /// The frame's state: where it is drawn (none: hidden with the quit
    /// button, or a play whose volume does not change), the session's
    /// volume shown while nobody holds it.
    pub fn frame(&mut self, place: Option<Place>, ratio: f32, volume: f32, now: f64) {
        if self.held.is_none() {
            self.position = quantized(volume_to_slider(volume));
        }
        self.ratio = ratio;
        self.place = place;
        if place.is_none() && self.held.is_none() {
            self.open = false;
        }
        if self.open && self.held.is_none() && now - self.touched > IDLE {
            self.open = false;
        }
    }

    fn set(&mut self, p: f32, commit: bool) -> Option<VolumeChange> {
        let p = quantized(p);
        if p == self.position && !commit {
            return None;
        }
        self.position = p;
        Some(VolumeChange {
            gain: slider_to_volume(p),
            commit,
        })
    }

    /// The slider's level at `y` (target pixels).
    fn level_at(&self, y: f32) -> Option<f32> {
        let [_, low, high] = self.place?.track;
        Some((low - y) / (low - high).max(1.0))
    }

    /// A pointer's edge: whether the control took it (else it goes on to
    /// the play: the quit button, a lane, a field), and the level when it
    /// changed. A press elsewhere closes the slider and is not taken.
    pub fn pointer(&mut self, edge: Edge, now: f64) -> (bool, Option<VolumeChange>) {
        let Some(place) = self.place else {
            // Hidden: a drag goes on to its end.
            return match (self.held.take(), edge.phase) {
                (Some(Held::Track), Phase::Up | Phase::Cancel) => {
                    (true, self.set(self.position, true))
                }
                _ => (false, None),
            };
        };
        let k = self.ratio;
        let [x, y] = edge.pos.map(|v| v * k);
        let [bx, by, r] = place.button;
        let on_button = (x - bx).powi(2) + (y - by).powi(2) <= r * r;
        let [l, t, rr, b] = place.plate;
        let on_plate = self.open && (l..=rr).contains(&x) && (t..=b).contains(&y);
        match edge.phase {
            Phase::Down if on_button => {
                self.held = Some(Held::Button);
                self.touched = now;
                (true, None)
            }
            Phase::Down if on_plate => {
                self.held = Some(Held::Track);
                self.touched = now;
                let p = self.level_at(y).unwrap_or(self.position);
                (true, self.set(p, false))
            }
            Phase::Down => {
                self.open = false;
                (false, None)
            }
            Phase::Move => match self.held {
                Some(Held::Track) => {
                    self.touched = now;
                    let p = self.level_at(y).unwrap_or(self.position);
                    (true, self.set(p, false))
                }
                Some(Held::Button) => (true, None),
                None => (false, None),
            },
            Phase::Up | Phase::Cancel => match self.held.take() {
                Some(Held::Track) => {
                    self.touched = now;
                    let p = if edge.phase == Phase::Up {
                        self.level_at(y).unwrap_or(self.position)
                    } else {
                        self.position
                    };
                    (true, self.set(p, true))
                }
                Some(Held::Button) => {
                    self.touched = now;
                    if edge.phase == Phase::Cancel || !on_button {
                        return (true, None);
                    }
                    if !self.open {
                        self.open = true;
                        return (true, None);
                    }
                    let change = if self.position > 0.0 {
                        keep_level(self.position);
                        self.set(0.0, true)
                    } else {
                        self.set(kept_level(), true)
                    };
                    (true, change)
                }
                None => (false, None),
            },
        }
    }

    /// Draws it where [`PlayVolume::frame`] placed it: the button as the
    /// quit button is drawn (a dark disc with a light rim), the music's
    /// icon in it (crossed out when muted, else one to three waves, as
    /// song select's `ui/volume.svg` draws them, here in shapes), and the
    /// open slider, its track filled to the level, its knob and its
    /// percentage.
    pub fn draw(&self, list: &mut DrawList, measure: &mut dyn TextMeasure) {
        let Some(place) = self.place else {
            return;
        };
        let [plate_c, line_c, accent_c, value_c] = ddi_theme::volume_colors();
        let [x, y, r] = place.button;
        let disc = |list: &mut DrawList, radius: f32, color: [f32; 4]| {
            let mut q = Quad::plain([x - radius, y - radius, x + radius, y + radius], color);
            q.shape = Shape {
                radius: [radius; 2],
                ..Shape::NONE
            };
            list.quad(q);
        };
        let rim = (r / 22.0).max(1.0);
        disc(list, r, [1.0, 1.0, 1.0, 0.3]);
        disc(list, r - rim, [0.0, 0.0, 0.0, 0.6]);
        icon(list, [x, y], r * 0.85, self.level());
        if !self.open {
            return;
        }
        let k = self.ratio;
        let [l, t, rr, b] = place.plate;
        let mut plate = Quad::plain([l, t, rr, b], plate_c);
        plate.shape = Shape {
            radius: [8.0 * k; 2],
            ..Shape::NONE
        };
        list.quad(plate);
        let mut edge = Quad::plain([l, t, rr, b], line_c);
        edge.shape = Shape {
            radius: [8.0 * k; 2],
            outline: [1.0 * k; 2],
            ..Shape::NONE
        };
        list.quad(edge);
        let [tx, low, high] = place.track;
        let half = 2.0 * k;
        let rounded = |q: &mut Quad| {
            q.shape = Shape {
                radius: [half; 2],
                ..Shape::NONE
            };
        };
        let mut track = Quad::plain([tx - half, high, tx + half, low], line_c);
        rounded(&mut track);
        list.quad(track);
        let level = low - (low - high) * self.position;
        if self.position > 0.0 {
            let mut fill = Quad::plain([tx - half, level, tx + half, low], accent_c);
            rounded(&mut fill);
            list.quad(fill);
        }
        let kw = (rr - l) * 0.4 / 2.0;
        let mut knob = Quad::plain(
            [tx - kw, level - 3.0 * k, tx + kw, level + 3.0 * k],
            value_c,
        );
        knob.shape = Shape {
            radius: [3.0 * k; 2],
            ..Shape::NONE
        };
        list.quad(knob);
        let mut style =
            TextStyle::new(ddi_l10n::font_stack(crate::opponents::FACE), VALUE_SIZE * k);
        style.weight = 600;
        let text = format!("{}%", self.percent());
        let width = measure.measure(&text, &style).advance;
        list.text(text, &style, tx - width / 2.0, t + 6.0 * k, value_c);
    }

    /// The icon's level: 0 muted, then 1 to 3.
    fn level(&self) -> usize {
        match self.position {
            p if p <= 0.0 => 0,
            p if p < 0.34 => 1,
            p if p < 0.67 => 2,
            _ => 3,
        }
    }
}

/// The speaker of `ui/volume.svg` (its 64-unit cell) in white shapes, its
/// cell's middle at `at`, `half` its half side: the body, the cone (the
/// body and two slanted quads, the trapezoid's halves), and a cross for
/// a mute or `level` waves, each a ring's arc of ±45° (the ring cut by a
/// clip at its chord).
fn icon(list: &mut DrawList, [cx, cy]: [f32; 2], half: f32, level: usize) {
    let u = half / 32.0;
    let p = |x: f32, y: f32| [cx + (x - 32.0) * u, cy + (y - 32.0) * u];
    let white = [1.0, 1.0, 1.0, 1.0];
    // The body and the cone's middle: 11..30 across, 25..39 down.
    let [l, t] = p(11.0, 25.0);
    let [r, b] = p(30.0, 39.0);
    list.quad(Quad::plain([l, t, r, b], white));
    // The cone's halves: unit squares sent along its slanted sides.
    for (origin, a, bv) in [
        (p(19.0, 25.0), [11.0, -10.0], [0.0, 10.0]),
        (p(19.0, 39.0), [11.0, 10.0], [0.0, -10.0]),
    ] {
        let mut q = Quad::plain([0.0, 0.0, 1.0, 1.0], white);
        q.transform = Affine([
            a[0] * u,
            a[1] * u,
            bv[0] * u,
            bv[1] * u,
            origin[0],
            origin[1],
        ]);
        list.quad(q);
    }
    let stroke = 4.5 * u;
    if level == 0 {
        // A cross at (47, 32).
        let [mx, my] = p(47.0, 32.0);
        for angle in [std::f32::consts::FRAC_PI_4, -std::f32::consts::FRAC_PI_4] {
            let len = 14.0 * 1.414 * u / 2.0 + stroke / 2.0;
            let mut q = Quad::plain([-len, -stroke / 2.0, len, stroke / 2.0], white);
            q.transform = Affine::rotate(angle).then(Affine::translate(mx, my));
            q.shape = Shape {
                radius: [stroke / 2.0; 2],
                ..Shape::NONE
            };
            list.quad(q);
        }
        return;
    }
    let [ax, ay] = p(32.0, 32.0);
    for radius in [9.0, 17.0, 25.0].into_iter().take(level) {
        let outer = (radius + 2.25) * u;
        let chord = ax + radius * u * std::f32::consts::FRAC_1_SQRT_2;
        list.push_clip([chord, ay - outer, ax + outer + 1.0, ay + outer]);
        let mut ring = Quad::plain([ax - outer, ay - outer, ax + outer, ay + outer], white);
        ring.shape = Shape {
            radius: [outer; 2],
            outline: [stroke; 2],
            ..Shape::NONE
        };
        list.quad(ring);
        list.pop_clip();
    }
}

/// A change of the control reaches the play: its sound at once
/// (`PlaySession::set_volume`, the announcer at its share), and the
/// settings once it is let go, so that song select, the next prompt and a
/// restart have it.
pub fn apply<A: AudioBackend, I: Inputs>(session: &mut PlaySession<A, I>, change: VolumeChange) {
    let mut settings = Settings::cached();
    session.set_volume(change.gain, settings.announcer_volume);
    if change.commit && settings.volume != change.gain {
        settings.volume = change.gain;
        settings.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: [f32; 2] = [1280.0, 720.0];

    fn shown(v: &mut PlayVolume, quit: [f32; 3], now: f64) {
        v.frame(
            Some(place(quit, SIZE, 1.0)),
            1.0,
            slider_to_volume(v.position),
            now,
        );
    }

    fn edge(phase: Phase, pos: [f32; 2]) -> Edge {
        Edge { phase, pos }
    }

    /// Beside the quit button, away from the receptors' row, the slider on
    /// the screen; under a ratio the same in logical pixels.
    #[test]
    fn it_sits_beside_the_quit_button_and_its_slider_stays_on_the_screen() {
        for quit in [
            [250.0, 30.0, 22.0],
            [250.0, 690.0, 22.0],
            [30.0, 360.0, 22.0],
        ] {
            let p = place(quit, SIZE, 1.0);
            let [x, y, r] = p.button;
            assert_eq!((x, r), (quit[0], quit[2]));
            assert!((y - quit[1]).abs() >= 2.0 * r, "{quit:?} {p:?}");
            if quit[1] > 360.0 {
                assert!(y < quit[1]);
            } else {
                assert!(y > quit[1]);
            }
            let [l, t, rr, b] = p.plate;
            assert!(
                l >= 0.0 && t >= 0.0 && rr <= SIZE[0] && b <= SIZE[1],
                "{p:?}"
            );
            // Clear of the quit button.
            let [qx, qy, qr] = quit;
            let nearest = [qx.clamp(l, rr), qy.clamp(t, b)];
            assert!(
                (nearest[0] - qx).hypot(nearest[1] - qy) > qr,
                "{quit:?} {p:?}"
            );
        }
        // A short screen: beside the button.
        let p = place([30.0, 195.0, 22.0], [844.0, 390.0], 1.0);
        assert!(p.plate[0] > 30.0 + 22.0 && p.plate[3] <= 390.0, "{p:?}");
    }

    /// A press on the button opens the slider; the next mutes, the next
    /// brings the level back; a drag sets it, kept once let go; a press
    /// elsewhere closes it and goes on; it closes with the buttons and
    /// when left alone.
    #[test]
    fn presses_open_mute_drag_and_close_it() {
        let quit = [250.0, 30.0, 22.0];
        let mut v = PlayVolume::default();
        v.frame(None, 1.0, slider_to_volume(0.5), 0.0);
        shown(&mut v, quit, 0.0);
        let at = v.at().unwrap();
        let b = [at.button[0], at.button[1]];
        assert!(at.plate.is_none());
        assert_eq!(v.pointer(edge(Phase::Down, b), 0.1), (true, None));
        assert_eq!(v.pointer(edge(Phase::Up, b), 0.2), (true, None));
        assert!(v.open() && v.at().unwrap().plate.is_some());
        // Mute, then back to the level it was.
        v.pointer(edge(Phase::Down, b), 0.3);
        let (_, c) = v.pointer(edge(Phase::Up, b), 0.4);
        assert_eq!(
            c,
            Some(VolumeChange {
                gain: 0.0,
                commit: true
            })
        );
        v.pointer(edge(Phase::Down, b), 0.5);
        let (_, c) = v.pointer(edge(Phase::Up, b), 0.6);
        assert_eq!(c.unwrap().gain, slider_to_volume(0.5));
        // Along the track: the top is the loudest, kept when let go.
        let place = v.place.unwrap();
        let [tx, low, high] = place.track;
        let (taken, c) = v.pointer(edge(Phase::Down, [tx, low - (low - high) * 0.25]), 1.0);
        assert!(taken);
        assert!(!c.unwrap().commit);
        let (_, c) = v.pointer(edge(Phase::Move, [tx + 30.0, high - 40.0]), 1.1);
        assert_eq!(
            c,
            Some(VolumeChange {
                gain: 1.0,
                commit: false
            })
        );
        let (_, c) = v.pointer(edge(Phase::Up, [tx + 30.0, high - 40.0]), 1.2);
        assert_eq!(
            c,
            Some(VolumeChange {
                gain: 1.0,
                commit: true
            })
        );
        assert_eq!(v.percent(), 100);
        // A press elsewhere: not taken, the slider shut.
        assert_eq!(
            v.pointer(edge(Phase::Down, [700.0, 400.0]), 1.3),
            (false, None)
        );
        assert!(!v.open());
        // Opened again, then left alone past IDLE; then with the buttons.
        v.pointer(edge(Phase::Down, b), 2.0);
        v.pointer(edge(Phase::Up, b), 2.0);
        shown(&mut v, quit, 2.0 + IDLE - 0.1);
        assert!(v.open());
        shown(&mut v, quit, 2.0 + IDLE + 0.1);
        assert!(!v.open());
        v.pointer(edge(Phase::Down, b), 6.0);
        v.pointer(edge(Phase::Up, b), 6.0);
        v.frame(None, 1.0, 1.0, 6.1);
        assert!(!v.open() && v.at().is_none());
        // Hidden, nothing is taken.
        assert_eq!(v.pointer(edge(Phase::Down, b), 6.2), (false, None));
    }

    /// On a touch screen a room's stage has none (its Ready card has the
    /// volume); singleplayer plays, replays and playtests do; with a mouse
    /// every play does; calibration never.
    #[test]
    fn a_touch_screen_s_stage_has_none() {
        assert!(offered(true, true, false));
        assert!(!offered(true, true, true));
        assert!(offered(true, false, true));
        assert!(offered(true, false, false));
        assert!(!offered(false, false, false));
        assert!(!offered(false, true, false));
    }

    /// The session's volume is shown while nobody holds the slider.
    #[test]
    fn it_shows_the_sessions_volume() {
        let mut v = PlayVolume::default();
        v.frame(Some(place([250.0, 30.0, 22.0], SIZE, 1.0)), 1.0, 0.0, 0.0);
        assert_eq!(v.percent(), 0);
        v.frame(Some(place([250.0, 30.0, 22.0], SIZE, 1.0)), 1.0, 1.0, 0.0);
        assert_eq!(v.percent(), 100);
    }
}
