//! StepMania's named background effects and transitions, as data: what a
//! change's effect (field 7), second file (field 8), transition (field 9)
//! and colors (fields 10 and 11) draw, and how the background that leaves
//! goes. StepMania 5.1 (`BackgroundEffects/*.lua`,
//! `BackgroundTransitions/*.xml`, `src/Background.cpp`,
//! `src/NotesLoaderSM.cpp`, `src/RageTypes.h` at `825467b`) runs every
//! effect as a theme script; none of them needs one here, as each places
//! one or two sprites in a fixed way:
//!
//! - `StretchNormal` (and `StretchNoLoop`, `StretchRewind`,
//!   `StretchPaused`, `StretchNormalAlignLeft`, which differ in how a
//!   movie plays, [`super::MovieEffect`], or not at all): the file
//!   covers the screen (`scale_or_crop_background`, the default
//!   `BackgroundFitMode_CoverPreserve`), tinted by `Color1`;
//!   `StretchNormalRed`, `…Green`, `…Blue` with that color instead.
//! - `Centered` and `UpperLeft`: the file at its own size (StepMania's
//!   screen being 480 px tall), centred on the screen or on its upper
//!   left corner (a sprite is centred on its position).
//! - `OverlayAdd`: covering, added (`BlendMode_Add`). `File1FlashOverlay`:
//!   the same, fading out over a second from each change.
//! - `File2Normal`: both files covering, the second over the first, both
//!   in `Color1`; `File2Flash`: the second in `Color2`, fading out over a
//!   second; `Visualization2File`: the second added and stretched to the
//!   screen; `Visualization2FileFlash`: added, covering, fading out.
//!   `SongBgWithMovieViz`: the song's background with the file added.
//! - `Checkerboard1File2x2`, `Checkerboard2x2`, `Checkerboard2File2x2`,
//!   `Kaleidoscope2x2`: four quarters, each the file cropped to cover it
//!   (`Sprite::CropTo`), turned half a turn in the kaleidoscope (both zooms
//!   negated).
//!
//! A name StepMania has no file for is `Centered` (`CreateBackground`:
//! "BackgroundEffect '%s' is missing"); names are matched without regard
//! to case, as StepMania finds the theme's files. A color is read as
//! `RageColor::FromString` reads it (`r,g,b[,a]` with `^` for `,`, or
//! `#rrggbb[aa]`), then rounded to 8 bits (`NormalizeColorString`);
//! anything else is white. An effect that needs a second file draws
//! nothing without one (its script fails on `LoadActor(nil)`), save
//! those that test for it; a second file found nowhere is StepMania's
//! blank graphic, which shows nothing.
//!
//! Transitions act on the background that leaves, drawn over the one that
//! comes in: `CrossFade` (and its faster kinds) fades it out; `Fade*` crop
//! it away from one side or towards its middle behind a 30 % soft edge;
//! `Slide*` move it off by a screen while it fades. Each takes a second
//! (the crossfades 1, 0.75 and 0.5).

use ddi_chart::formats::sm::sm_float;

use super::BgImage;

/// A change's effect, as far as it changes what is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Effect {
    /// `StretchNormal` and the effects that differ only for movies.
    #[default]
    Stretch,
    Centered,
    UpperLeft,
    OverlayAdd,
    File1FlashOverlay,
    File2Flash,
    File2Normal,
    Checkerboard1File,
    Checkerboard2,
    Checkerboard2File,
    Kaleidoscope,
    Visualization2File,
    Visualization2FileFlash,
    SongBgWithMovieViz,
}

impl Effect {
    /// The effect StepMania's file `name` (field 7) is, and the color it
    /// puts in place of `Color1` (the `StretchNormal` colors). An empty name
    /// is the default for the file (`StretchNormal` for images and movies).
    fn named(name: &str) -> (Effect, Option<[f32; 4]>) {
        let effect = match name.trim().to_ascii_lowercase().as_str() {
            ""
            | "stretchnormal"
            | "stretchnoloop"
            | "stretchrewind"
            | "stretchpaused"
            | "stretchnormalalignleft" => Effect::Stretch,
            "stretchnormalred" => return (Effect::Stretch, Some([1.0, 0.0, 0.0, 1.0])),
            "stretchnormalgreen" => return (Effect::Stretch, Some([0.0, 1.0, 0.0, 1.0])),
            "stretchnormalblue" => return (Effect::Stretch, Some([0.0, 0.0, 1.0, 1.0])),
            "upperleft" => Effect::UpperLeft,
            "overlayadd" => Effect::OverlayAdd,
            "file1flashoverlay" => Effect::File1FlashOverlay,
            "file2flash" => Effect::File2Flash,
            "file2normal" => Effect::File2Normal,
            "checkerboard1file2x2" => Effect::Checkerboard1File,
            "checkerboard2x2" => Effect::Checkerboard2,
            "checkerboard2file2x2" => Effect::Checkerboard2File,
            "kaleidoscope2x2" => Effect::Kaleidoscope,
            "visualization2file" => Effect::Visualization2File,
            "visualization2fileflash" => Effect::Visualization2FileFlash,
            "songbgwithmovieviz" => Effect::SongBgWithMovieViz,
            // `Centered`, and every name without a file.
            _ => Effect::Centered,
        };
        (effect, None)
    }

    /// Whether its `On` command starts a tween, so a change to the same
    /// background plays it again from the start.
    pub fn flashes(self) -> bool {
        matches!(
            self,
            Effect::File1FlashOverlay | Effect::File2Flash | Effect::Visualization2FileFlash
        )
    }
}

/// How a change shows: its effect, its colors and its second file.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub effect: Effect,
    /// `Color1` and `Color2`, straight RGBA in encoded values.
    pub color1: [f32; 4],
    pub color2: [f32; 4],
    /// The second file: `None` when not given, [`BgImage::Clear`] when
    /// found nowhere (StepMania's blank graphic).
    pub file2: Option<BgImage>,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            effect: Effect::Stretch,
            color1: WHITE,
            color2: WHITE,
            file2: None,
        }
    }
}

const WHITE: [f32; 4] = [1.0; 4];

impl Look {
    /// The look of a change's fields; `file2` resolves field 8.
    pub(super) fn of(fields: &[String], file2: impl FnOnce(&str) -> BgImage) -> Look {
        let (effect, fixed) = Effect::named(fields.get(6).map_or("", |s| s.as_str()));
        let color1 = fixed.unwrap_or_else(|| color(fields.get(9)));
        let file2 = fields
            .get(7)
            .map(|f| f.trim())
            .filter(|f| !f.is_empty())
            .map(file2);
        Look {
            effect,
            color1,
            color2: color(fields.get(10)),
            file2,
        }
    }
}

/// A color field as StepMania keeps it: `^` read as `,`
/// (`LoadFromBGChangesVector`), parsed by `RageColor::FromString` and
/// rounded to 8 bits by `NormalizeColorString`; white when absent or
/// unreadable.
fn color(field: Option<&String>) -> [f32; 4] {
    let Some(field) = field.filter(|f| !f.is_empty()) else {
        return WHITE;
    };
    let text = field.replace('^', ",");
    let Some(c) = parse_color(&text) else {
        return WHITE;
    };
    c.map(|v| (v * 255.0).round_ties_even().clamp(0.0, 255.0) / 255.0)
}

/// `RageColor::FromString`: `sscanf("%f,%f,%f,%f")` with three or four
/// values, else `sscanf("#%2x%2x%2x%2x")` with three or four.
fn parse_color(s: &str) -> Option<[f32; 4]> {
    let mut values = Vec::with_capacity(4);
    let mut rest = s;
    while values.len() < 4 {
        if !values.is_empty() {
            match rest.strip_prefix(',') {
                Some(r) => rest = r,
                None => break,
            }
        }
        let Some((v, r)) = scan_float(rest) else {
            break;
        };
        values.push(v);
        rest = r;
    }
    match values[..] {
        [r, g, b] => return Some([r, g, b, 1.0]),
        [r, g, b, a] => return Some([r, g, b, a]),
        _ => {}
    }
    let mut rest = s.strip_prefix('#')?;
    let mut hex = Vec::with_capacity(4);
    while hex.len() < 4 {
        // `%2x` skips white space, then takes up to two hex digits.
        let r = rest.trim_start();
        let n = r.bytes().take(2).take_while(u8::is_ascii_hexdigit).count();
        if n == 0 {
            break;
        }
        hex.push(u8::from_str_radix(&r[..n], 16).ok()?);
        rest = &r[n..];
    }
    let f = |v: u8| f32::from(v) / 255.0;
    match hex[..] {
        [r, g, b] => Some([f(r), f(g), f(b), 1.0]),
        [r, g, b, a] => Some([f(r), f(g), f(b), f(a)]),
        _ => None,
    }
}

/// `%f`: white space, then the longest decimal number (sign, digits, a
/// point, an exponent); the rest of the text after it.
fn scan_float(s: &str) -> Option<(f32, &str)> {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let digits = |i: usize| b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    let int = digits(i);
    i += int;
    let mut frac = 0;
    if b.get(i) == Some(&b'.') {
        frac = digits(i + 1);
        if int + frac > 0 {
            i += 1 + frac;
        }
    }
    if int + frac == 0 {
        return None;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(b.get(i + 1), Some(b'+' | b'-')));
        let exp = digits(i + 1 + sign);
        if exp > 0 {
            i += 1 + sign + exp;
        }
    }
    Some((sm_float(&t[..i]) as f32, &t[i..]))
}

/// How the background that leaves goes ([`transition`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transition {
    /// Fades out (`CrossFade`, and the 1 s fade of field 4).
    #[default]
    Fade,
    /// Cropped away behind a soft edge (`Fade*`).
    Wipe(Wipe),
    /// Moved off by a screen while it fades (`Slide*`).
    Slide(Slide),
}

/// The `Fade*` transitions: the side the leaving background is cropped
/// from (`FadeDown` crops its top: the edge moves down).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wipe {
    Down,
    Up,
    Left,
    Right,
    /// `FadeCenterHorizontal`: top and bottom towards the middle.
    CenterHorizontal,
    /// `FadeCenterVertical`: left and right towards the middle.
    CenterVertical,
}

/// The `Slide*` transitions: where the leaving background moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slide {
    Down,
    Up,
    Left,
    Right,
}

/// A transition by its name (field 9): its seconds and kind; `None` for a
/// name StepMania has no file for (it reports it and cuts).
pub(super) fn transition(name: &str) -> Option<(f64, Transition)> {
    use Transition::*;
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "crossfade" => (1.0, Fade),
        "crossfade_faster" => (0.75, Fade),
        "crossfade_fastest" => (0.5, Fade),
        "fadedown" => (1.0, Wipe(self::Wipe::Down)),
        "fadeup" => (1.0, Wipe(self::Wipe::Up)),
        "fadeleft" => (1.0, Wipe(self::Wipe::Left)),
        "faderight" => (1.0, Wipe(self::Wipe::Right)),
        "fadecenterhorizontal" => (1.0, Wipe(self::Wipe::CenterHorizontal)),
        "fadecentervertical" => (1.0, Wipe(self::Wipe::CenterVertical)),
        "slidedown" => (1.0, Slide(self::Slide::Down)),
        "slideup" => (1.0, Slide(self::Slide::Up)),
        "slideleft" => (1.0, Slide(self::Slide::Left)),
        "slideright" => (1.0, Slide(self::Slide::Right)),
        _ => return None,
    })
}

/// Where a background quad goes, in the frame's own coordinates (`0..1`
/// across and down, StepMania's screen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// Scaled, keeping its shape, to cover the box `size` centred at
    /// `center`, whole: it may reach past the box (the frame cuts it).
    Cover { center: [f32; 2], size: [f32; 2] },
    /// Scaled to cover the box, its texture cropped to it
    /// (`Sprite::CropTo`); mirrored on both axes when `flip`.
    CropTo {
        center: [f32; 2],
        size: [f32; 2],
        flip: bool,
    },
    /// Stretched to the box (`scaletoclipped`).
    Fill { center: [f32; 2], size: [f32; 2] },
    /// At its own size in StepMania's pixels (the frame being 480 tall),
    /// centred at `center`.
    Native { center: [f32; 2] },
}

/// The whole frame.
const FULL: ([f32; 2], [f32; 2]) = ([0.5, 0.5], [1.0, 1.0]);

impl Place {
    /// Covering the frame (`scale_or_crop_background`).
    pub const COVER: Place = Place::Cover {
        center: FULL.0,
        size: FULL.1,
    };

    fn moved(self, by: [f32; 2]) -> Place {
        let add = |c: [f32; 2]| [c[0] + by[0], c[1] + by[1]];
        match self {
            Place::Cover { center, size } => Place::Cover {
                center: add(center),
                size,
            },
            Place::CropTo { center, size, flip } => Place::CropTo {
                center: add(center),
                size,
                flip,
            },
            Place::Fill { center, size } => Place::Fill {
                center: add(center),
                size,
            },
            Place::Native { center } => Place::Native {
                center: add(center),
            },
        }
    }
}

/// One quad of a background layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BgDraw {
    /// The image's id, as the shell loaded it.
    pub id: usize,
    pub place: Place,
    /// Multiplies the image's encoded colors and its opacity (`diffuse`).
    pub color: [f32; 4],
    /// Added to what is under it (`BlendMode_Add`), else blended over it.
    pub add: bool,
    /// The part of the quad cropped from each side (left, top, right,
    /// bottom; fractions of the sprite, which may be negative), and the
    /// soft edge inside each crop over which it fades in (`Sprite`'s
    /// `crop*` and `fade*`).
    pub crop: [f32; 4],
    pub fade: [f32; 4],
}

impl BgDraw {
    /// An image covering the frame, as it is.
    pub fn cover(id: usize) -> BgDraw {
        BgDraw {
            id,
            place: Place::COVER,
            color: WHITE,
            add: false,
            crop: [0.0; 4],
            fade: [0.0; 4],
        }
    }
}

/// Quads a layer draws at most in one frame: an effect's four quarters,
/// for the background that comes in and the one that leaves.
pub const LAYER_DRAWS: usize = 8;

/// One layer's quads, back to front; the rest `None`.
pub type LayerDraws = [Option<BgDraw>; LAYER_DRAWS];

/// Adds `draw` after the quads in `out` (dropped past [`LAYER_DRAWS`]).
pub(super) fn push(out: &mut LayerDraws, draw: BgDraw) {
    if let Some(slot) = out.iter_mut().find(|d| d.is_none()) {
        *slot = Some(draw);
    }
}

/// The images one background draws: its file's id, its second file's
/// (`None`: not given; `Some(None)`: given but showing nothing), and the
/// song background's (for `SongBgWithMovieViz`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Ids {
    pub file1: usize,
    pub file2: Option<Option<usize>>,
    pub song: Option<usize>,
}

/// The quads `look` draws, `age` seconds after its change, into `out`.
pub(super) fn effect_draws(look: &Look, ids: Ids, age: f64, out: &mut LayerDraws) {
    let flash = (1.0 - age.max(0.0)).clamp(0.0, 1.0) as f32;
    let tint = |color: [f32; 4], alpha: f32| [color[0], color[1], color[2], color[3] * alpha];
    let quad = |id: usize, place: Place, color: [f32; 4], add: bool| BgDraw {
        id,
        place,
        color,
        add,
        crop: [0.0; 4],
        fade: [0.0; 4],
    };
    let (c1, c2) = (look.color1, look.color2);
    let file1 = ids.file1;
    // The second file: `Err(())` when not given (its script fails where it
    // loads it), `Ok(None)` when it shows nothing.
    let file2 = ids.file2.ok_or(());
    let mut add = |d: BgDraw| push(out, d);
    match look.effect {
        Effect::Stretch => add(quad(file1, Place::COVER, c1, false)),
        Effect::Centered => add(quad(file1, Place::Native { center: FULL.0 }, c1, false)),
        Effect::UpperLeft => add(quad(file1, Place::Native { center: [0.0; 2] }, c1, false)),
        Effect::OverlayAdd => add(quad(file1, Place::COVER, c1, true)),
        Effect::File1FlashOverlay => add(quad(file1, Place::COVER, tint(c1, flash), true)),
        Effect::File2Flash | Effect::File2Normal | Effect::Visualization2FileFlash => {
            add(quad(file1, Place::COVER, c1, false));
            if let Ok(Some(id)) = file2 {
                add(match look.effect {
                    Effect::File2Flash => quad(id, Place::COVER, tint(c2, flash), false),
                    Effect::File2Normal => quad(id, Place::COVER, c1, false),
                    _ => quad(id, Place::COVER, tint(c2, flash), true),
                });
            }
        }
        Effect::Visualization2File => {
            let Ok(file2) = file2 else { return };
            add(quad(file1, Place::COVER, c1, false));
            if let Some(id) = file2 {
                let (center, size) = FULL;
                add(quad(id, Place::Fill { center, size }, c2, true));
            }
        }
        Effect::SongBgWithMovieViz => {
            if let Some(song) = ids.song {
                add(quad(song, Place::COVER, c1, false));
            }
            add(quad(file1, Place::COVER, c2, true));
        }
        Effect::Checkerboard1File
        | Effect::Checkerboard2
        | Effect::Checkerboard2File
        | Effect::Kaleidoscope => {
            // Top left, top right, bottom left, bottom right: each
            // quarter's file and color.
            let one = [(Some(file1), c1); 4];
            let quarters = match look.effect {
                Effect::Checkerboard2 => {
                    let Ok(f2) = file2 else { return };
                    [(Some(file1), c1), (f2, c2), (f2, c1), (Some(file1), c2)]
                }
                Effect::Checkerboard2File => {
                    let Ok(f2) = file2 else { return };
                    [(Some(file1), c1), (f2, c2), (f2, c2), (Some(file1), c1)]
                }
                _ => one,
            };
            let flip = look.effect == Effect::Kaleidoscope;
            for (i, (id, color)) in quarters.into_iter().enumerate() {
                let Some(id) = id else { continue };
                let center = [[0.25, 0.75][i % 2], [0.25, 0.75][i / 2]];
                let place = Place::CropTo {
                    center,
                    size: [0.5, 0.5],
                    flip,
                };
                add(quad(id, place, color, false));
            }
        }
    }
}

/// `draws`, the background that leaves, `progress` (`0..=1`) through
/// `transition`.
pub(super) fn leaving(draws: &mut LayerDraws, transition: Transition, progress: f32) {
    let p = progress.clamp(0.0, 1.0);
    for d in draws.iter_mut().flatten() {
        match transition {
            Transition::Fade => d.color[3] *= 1.0 - p,
            Transition::Slide(s) => {
                let by = match s {
                    Slide::Down => [0.0, p],
                    Slide::Up => [0.0, -p],
                    Slide::Left => [-p, 0.0],
                    Slide::Right => [p, 0.0],
                };
                d.place = d.place.moved(by);
                d.color[3] *= 1.0 - p;
            }
            Transition::Wipe(w) => {
                // `crop*,-0.3;fade*,0.3;linear,1.0;crop*,1.3` (to 0.5 on
                // both sides for the centred ones).
                let edge = -0.3 + 1.6 * p;
                let middle = -0.3 + 0.8 * p;
                // Left, top, right, bottom.
                let (crop, sides) = match w {
                    Wipe::Down => (edge, [false, true, false, false]),
                    Wipe::Up => (edge, [false, false, false, true]),
                    Wipe::Left => (edge, [false, false, true, false]),
                    Wipe::Right => (edge, [true, false, false, false]),
                    Wipe::CenterHorizontal => (middle, [false, true, false, true]),
                    Wipe::CenterVertical => (middle, [true, false, true, false]),
                };
                for (side, on) in sides.into_iter().enumerate() {
                    if on {
                        d.crop[side] = crop;
                        d.fade[side] = 0.3;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(text: &str) -> Vec<String> {
        text.split('=').map(str::to_string).collect()
    }

    #[test]
    fn colors_are_read_as_stepmania_reads_them() {
        let c = |s: &str| color(Some(&s.to_string()));
        assert_eq!(c("1^0^0^0.5"), [1.0, 0.0, 0.0, 128.0 / 255.0]);
        assert_eq!(c("0.5,0.25,1"), [128.0 / 255.0, 64.0 / 255.0, 1.0, 1.0]);
        assert_eq!(c("#000000"), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(c("#FF800040"), [1.0, 128.0 / 255.0, 0.0, 64.0 / 255.0]);
        // Over 1 is held at 1; two values, words or nothing are white.
        assert_eq!(c("2,0,0"), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(c("1,0"), WHITE);
        assert_eq!(c("red"), WHITE);
        assert_eq!(color(None), WHITE);
    }

    #[test]
    fn effects_are_named_without_regard_to_case() {
        let look = |s: &str| Look::of(&fields(s), |_| BgImage::Clear);
        assert_eq!(look("1=a.png=1=0=0=1").effect, Effect::Stretch);
        assert_eq!(
            look("1=a.png=1=0=0=1=overlayADD").effect,
            Effect::OverlayAdd
        );
        let red = look("1=a.png=1=0=0=1=StretchNormalRed=====0,1,0");
        assert_eq!(
            (red.effect, red.color1),
            (Effect::Stretch, [1.0, 0.0, 0.0, 1.0])
        );
        // A name without a file is `Centered`.
        assert_eq!(look("1=a.png=1=0=0=1=Wobble").effect, Effect::Centered);
        let two = look("1=a.png=1=0=0=1=File2Flash=b.png==#FF0000=0^0^1");
        assert_eq!(two.file2, Some(BgImage::Clear));
        assert_eq!(two.color1, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(two.color2, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn checkerboards_place_four_quarters() {
        let look = Look {
            effect: Effect::Checkerboard2,
            color2: [0.0, 0.0, 1.0, 1.0],
            ..Look::default()
        };
        let mut out = LayerDraws::default();
        let ids = Ids {
            file1: 1,
            file2: Some(Some(2)),
            song: None,
        };
        effect_draws(&look, ids, 0.0, &mut out);
        let got: Vec<(usize, [f32; 2], f32)> = out
            .iter()
            .flatten()
            .map(|d| match d.place {
                Place::CropTo { center, .. } => (d.id, center, d.color[2]),
                _ => panic!("{d:?}"),
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (1, [0.25, 0.25], 1.0),
                (2, [0.75, 0.25], 1.0),
                (2, [0.25, 0.75], 1.0),
                (1, [0.75, 0.75], 1.0),
            ]
        );
        // Colors: file 1 then file 2 in Color1 on the left, Color2 on the
        // right.
        assert_eq!(out[1].unwrap().color[0], 0.0);
        assert_eq!(out[2].unwrap().color[0], 1.0);
        // Without a second file the script fails: nothing.
        let mut out = LayerDraws::default();
        effect_draws(&look, Ids { file2: None, ..ids }, 0.0, &mut out);
        assert!(out.iter().all(Option::is_none));
    }

    #[test]
    fn flashes_fade_out_over_a_second() {
        let look = Look {
            effect: Effect::File1FlashOverlay,
            ..Look::default()
        };
        let mut out = LayerDraws::default();
        effect_draws(&look, Ids::default(), 0.25, &mut out);
        let d = out[0].unwrap();
        assert!(d.add);
        assert_eq!(d.color[3], 0.75);
    }

    #[test]
    fn transitions_crop_slide_and_fade_what_leaves() {
        assert_eq!(
            transition("FadeDown"),
            Some((1.0, Transition::Wipe(Wipe::Down)))
        );
        assert_eq!(
            transition("crossfade_faster"),
            Some((0.75, Transition::Fade))
        );
        assert_eq!(transition("Nope"), None);
        let one = || {
            let mut d = LayerDraws::default();
            d[0] = Some(BgDraw::cover(0));
            d
        };
        let mut d = one();
        leaving(&mut d, Transition::Wipe(Wipe::CenterHorizontal), 0.5);
        let d = d[0].unwrap();
        assert!((d.crop[1] - 0.1).abs() < 1e-6 && (d.crop[3] - 0.1).abs() < 1e-6);
        assert_eq!((d.fade[1], d.crop[0]), (0.3, 0.0));
        let mut d = one();
        leaving(&mut d, Transition::Slide(Slide::Left), 0.25);
        let d = d[0].unwrap();
        assert_eq!(
            d.place,
            Place::Cover {
                center: [0.25, 0.5],
                size: [1.0, 1.0]
            }
        );
        assert_eq!(d.color[3], 0.75);
    }
}
