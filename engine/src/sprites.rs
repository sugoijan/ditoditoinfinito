//! The back and mask layers of Dancing☆Onigiri works (`back_data`,
//! `mask_data`; `docs/plans/overlays.md`), as danoniplus shows them
//! (`drawBaseSpriteData`, `js/lib/dosConverter.js`): each depth of a layer
//! holds one image or text at a time, placed on the play's stage in
//! danoniplus's px; a cue replaces its depth's content, a clear empties
//! it, `ALL` every depth of the layer. A content plays the CSS animation
//! it names once from its cue: danoniplus's own animations
//! (`css/danoni_main.css`) are reproduced here; a work's own, defined in
//! its stylesheet, are not known and play as none.
//!
//! The importer turns the data into `danoni:back` and `danoni:mask` effect
//! events, timed in seconds, layer = chart (see
//! [`ddi_chart::formats::danoni`]).

use std::collections::HashSet;
use std::rc::Rc;

use ddi_chart::formats::danoni::{SPRITE_BACK, SPRITE_MASK, source::decode_entities};
use ddi_chart::{EffectTime, Song};

/// Which side of the field a layer is on.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum StageLayer {
    /// Under the field (`backSprite`).
    Back,
    /// Over the field (`maskSprite`).
    Mask,
}

/// What a depth shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SpriteSource {
    /// An image, by the name the work's data uses (its stored
    /// `layer_images` key).
    Image(Rc<str>),
    /// Text: danoniplus's span, `w` its font size and `h` its colour.
    Text {
        text: Rc<str>,
        /// Font size, px on the stage; 0 for the page's own.
        size: u32,
        /// A CSS colour, as written; empty (or unreadable) for the page's
        /// own.
        color: Rc<str>,
    },
}

/// How an animation applies outside its run (`animation-fill-mode`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Fill {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

/// One cue's content.
#[derive(Clone, Debug, PartialEq)]
struct Sprite {
    source: SpriteSource,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    opacity: f32,
    animation: Option<&'static Animation>,
    seconds: f64,
    fill: Fill,
}

/// A content as it is drawn at some instant: placed on the stage (its top
/// left at `x, y`, `w × h` px, 0 for its own size), then transformed about
/// its centre as CSS transforms it (`scale`, `rotate` degrees clockwise,
/// `translate` px), at `opacity` and `brightness`.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteDraw {
    pub layer: StageLayer,
    pub depth: u8,
    pub source: SpriteSource,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub opacity: f32,
    pub translate: [f32; 2],
    pub scale: [f32; 2],
    pub rotate: f32,
    pub brightness: f32,
}

/// One depth's cues: each one's time and content (`None`: empty).
type Depth = Vec<(f64, Option<Rc<Sprite>>)>;

/// Every depth of both layers over a play.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpriteTrack {
    /// danoniplus's stage, width and height in its px.
    stage: (f32, f32),
    /// Per layer, per depth, each cue's time and content.
    layers: Vec<(StageLayer, Vec<Depth>)>,
    /// Animations the work names that are not danoniplus's own.
    unknown_animations: Vec<String>,
    unknown_seen: HashSet<String>,
}

impl SpriteTrack {
    /// The layers of `song.charts[chart]`; under Reverse a layer's
    /// `Rev` data when the work has one for it (danoniplus tries
    /// `backRev…` before `back…`).
    pub fn new(song: &Song, chart: usize, reverse: bool) -> SpriteTrack {
        let stage = song
            .charts
            .get(chart)
            .and_then(|c| c.danoni.as_ref())
            .and_then(|d| d.stage)
            .map_or((500.0, 500.0), |(w, h)| (w as f32, h as f32));
        // A play from `startFrame` shows no cue from before it: danoniplus
        // fires a cue only on its frame.
        let start = song
            .charts
            .get(chart)
            .and_then(|c| c.danoni.as_ref())
            .and_then(|d| d.start)
            .unwrap_or(f64::NEG_INFINITY);
        let mut track = SpriteTrack {
            stage,
            ..SpriteTrack::default()
        };
        for (layer, kind) in [
            (StageLayer::Back, SPRITE_BACK),
            (StageLayer::Mask, SPRITE_MASK),
        ] {
            let cues = |variant: &str| -> Vec<(f64, &[String])> {
                song.effects
                    .iter()
                    .filter(|e| e.kind == kind && usize::from(e.layer) == chart)
                    .filter(|e| e.fields.first().is_some_and(|v| v == variant))
                    .filter_map(|e| match e.at {
                        EffectTime::Seconds(at) if at.is_finite() => Some((at, &e.fields[1..])),
                        _ => None,
                    })
                    .collect()
            };
            // Reverse's data replaces the other when the work has it, even
            // with nothing in it (its marker is a cue of no fields).
            let rev = cues("Rev");
            let mut cues = if reverse && !rev.is_empty() {
                rev
            } else {
                cues("")
            };
            cues.retain(|(at, f)| !f.is_empty() && *at >= start - 1e-9);
            // Stable: cues of one instant keep their order.
            cues.sort_by(|a, b| a.0.total_cmp(&b.0));
            let depths = cues
                .iter()
                .filter_map(|(_, f)| f.first()?.parse::<u8>().ok())
                .max()
                .map_or(0, |d| usize::from(d) + 1);
            let mut timeline: Vec<Depth> = vec![Vec::new(); depths];
            for (at, f) in cues {
                let field = |k: usize| f.get(k).map_or("", String::as_str);
                let source = field(1);
                if field(0) == "ALL" {
                    if source.is_empty() {
                        for d in &mut timeline {
                            d.push((at, None));
                        }
                    }
                    continue;
                }
                let Ok(depth) = field(0).parse::<u8>() else {
                    continue;
                };
                let sprite = (!source.is_empty())
                    .then(|| track.sprite(source, &f[2..]))
                    .flatten()
                    .map(Rc::new);
                // A colour box or a transform keeps the depth as it is.
                if sprite.is_none() && !source.is_empty() {
                    continue;
                }
                timeline[usize::from(depth)].push((at, sprite));
            }
            track.layers.push((layer, timeline));
        }
        track
    }

    /// A content from a cue's fields after its depth and source
    /// (`class, x, y, w, h, opacity, animation, seconds, fill`); `None` for
    /// a colour box (`[c]`) or a transform (`[t]`), not drawn yet.
    fn sprite(&mut self, source: &str, f: &[String]) -> Option<Sprite> {
        if source.starts_with("[c]") || source.starts_with("[t]") {
            return None;
        }
        let field = |k: usize| f.get(k).map_or("", String::as_str);
        let number = |s: &str| -> f32 {
            let s = s
                .trim()
                .trim_start_matches('{')
                .trim_end_matches('}')
                .trim();
            s.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .unwrap_or(0.0)
        };
        let image = ddi_chart::formats::danoni::is_sprite_image(source);
        let w = number(field(3));
        let source = if image {
            SpriteSource::Image(Rc::from(source))
        } else {
            SpriteSource::Text {
                text: Rc::from(plain_text(source).as_str()),
                size: w.max(0.0) as u32,
                color: Rc::from(field(4).trim()),
            }
        };
        let name = field(6).trim();
        let animation = ANIMATIONS.iter().find(|a| a.name == name);
        if animation.is_none()
            && name != "none"
            && !name.is_empty()
            && self.unknown_seen.insert(name.to_string())
        {
            self.unknown_animations.push(name.to_string());
        }
        Some(Sprite {
            x: number(field(1)),
            y: number(field(2)),
            w: if image { w.max(0.0) } else { 0.0 },
            h: if image {
                number(field(4)).max(0.0)
            } else {
                0.0
            },
            opacity: field(5)
                .parse::<f32>()
                .ok()
                .filter(|o| o.is_finite())
                .unwrap_or(1.0),
            animation,
            seconds: field(7)
                .parse::<f64>()
                .ok()
                .filter(|s| s.is_finite())
                .unwrap_or(0.0),
            // danoniplus writes the fill mode only into a colour box's
            // style (`colorObjInfo`); images and text keep CSS's `none`.
            fill: Fill::None,
            source,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.layers
            .iter()
            .all(|(_, depths)| depths.iter().all(Vec::is_empty))
    }

    /// danoniplus's stage, width and height in its px.
    pub fn stage(&self) -> (f32, f32) {
        self.stage
    }

    /// Animations the work names that are not danoniplus's own (played as
    /// none).
    pub fn unknown_animations(&self) -> &[String] {
        &self.unknown_animations
    }

    /// Every content the play may show, once each.
    pub fn sources(&self) -> Vec<SpriteSource> {
        let mut seen: HashSet<&SpriteSource> = HashSet::new();
        let mut out: Vec<SpriteSource> = Vec::new();
        for (_, depths) in &self.layers {
            for (_, sprite) in depths.iter().flatten() {
                if let Some(s) = sprite
                    && seen.insert(&s.source)
                {
                    out.push(s.source.clone());
                }
            }
        }
        out
    }

    /// What is shown at song second `t`: the back layer's depths, then the
    /// mask layer's, each from depth 0 up (danoniplus stacks them in that
    /// order).
    pub fn at(&self, t: f64) -> Vec<SpriteDraw> {
        let mut out = Vec::new();
        for (layer, depths) in &self.layers {
            for (depth, cues) in depths.iter().enumerate() {
                let i = cues.partition_point(|(at, _)| *at <= t);
                let Some((at, Some(sprite))) = i.checked_sub(1).map(|i| &cues[i]) else {
                    continue;
                };
                out.push(draw(*layer, depth as u8, sprite, t - at));
            }
        }
        out
    }
}

/// What a song's back and mask layers ask that is not reproduced, for an
/// import's report: animations from a work's own stylesheet (played as
/// none), colour boxes and layer transforms (not drawn), positions a
/// script computes (placed at 0).
pub fn import_notes(song: &Song) -> Vec<String> {
    let mut animations: Vec<String> = Vec::new();
    for chart in 0..song.charts.len() {
        for reverse in [false, true] {
            for a in SpriteTrack::new(song, chart, reverse).unknown_animations() {
                if !animations.contains(a) {
                    animations.push(a.clone());
                }
            }
        }
    }
    let fields = || {
        song.effects
            .iter()
            .filter(|e| e.kind == SPRITE_BACK || e.kind == SPRITE_MASK)
            .map(|e| e.fields.as_slice())
    };
    let mut notes = Vec::new();
    if !animations.is_empty() {
        animations.truncate(12);
        notes.push(format!(
            "back and mask animations from the work's own stylesheet play as none: {}",
            animations.join(", ")
        ));
    }
    if fields().any(|f| {
        f.get(2)
            .is_some_and(|s| s.starts_with("[c]") || s.starts_with("[t]"))
    }) {
        notes.push("back and mask colour boxes and layer transforms are not drawn".into());
    }
    let scripted = |s: &str| {
        let t = s
            .trim()
            .trim_start_matches('{')
            .trim_end_matches('}')
            .trim();
        s.contains('{') && t.parse::<f32>().is_err()
    };
    if fields()
        .any(|f| f.get(4).is_some_and(|x| scripted(x)) || f.get(5).is_some_and(|y| scripted(y)))
    {
        notes.push("back and mask positions computed by a script are placed at 0".into());
    }
    notes
}

/// A content `elapsed` seconds after its cue.
fn draw(layer: StageLayer, depth: u8, s: &Sprite, elapsed: f64) -> SpriteDraw {
    let mut d = SpriteDraw {
        layer,
        depth,
        source: s.source.clone(),
        x: s.x,
        y: s.y,
        w: s.w,
        h: s.h,
        opacity: s.opacity,
        translate: [0.0, 0.0],
        scale: [1.0, 1.0],
        rotate: 0.0,
        brightness: 1.0,
    };
    let Some(anim) = s.animation else {
        return d;
    };
    // CSS: the run is `[0, seconds)`; after it, the last keyframe holds
    // under `forwards` and `both`, nothing applies otherwise. A run of no
    // length is over at once.
    let progress = if s.seconds > 0.0 && elapsed < s.seconds {
        (elapsed / s.seconds).max(0.0) as f32
    } else if matches!(s.fill, Fill::Forwards | Fill::Both) {
        1.0
    } else {
        return d;
    };
    let k = anim.at(progress);
    if let Some(o) = k.opacity {
        d.opacity = o;
    }
    d.translate = k.translate;
    d.scale = k.scale;
    d.rotate = k.rotate_z;
    // Without perspective, a turn about X or Y squeezes the other axis.
    d.scale[0] *= k.rotate_y.to_radians().cos();
    d.scale[1] *= k.rotate_x.to_radians().cos();
    d.brightness = k.brightness;
    d
}

/// The values a keyframe sets; what it leaves out is the element's own.
#[derive(Copy, Clone, Debug, PartialEq)]
struct Keyframe {
    offset: f32,
    opacity: Option<f32>,
    translate: [f32; 2],
    scale: [f32; 2],
    rotate_x: f32,
    rotate_y: f32,
    rotate_z: f32,
    brightness: f32,
}

const K: Keyframe = Keyframe {
    offset: 0.0,
    opacity: None,
    translate: [0.0, 0.0],
    scale: [1.0, 1.0],
    rotate_x: 0.0,
    rotate_y: 0.0,
    rotate_z: 0.0,
    brightness: 1.0,
};

/// One of danoniplus's animations.
#[derive(Debug, PartialEq)]
struct Animation {
    name: &'static str,
    keyframes: &'static [Keyframe],
}

impl Animation {
    /// The values at `progress` (0 to 1) through the run: between two
    /// keyframes, eased as CSS's default timing function (`ease`) eases
    /// each segment.
    fn at(&self, progress: f32) -> Keyframe {
        let ks = self.keyframes;
        let i = ks
            .iter()
            .rposition(|k| k.offset <= progress)
            .unwrap_or(0)
            .min(ks.len().saturating_sub(2));
        let (a, b) = (&ks[i], &ks[(i + 1).min(ks.len() - 1)]);
        let span = b.offset - a.offset;
        let t = if span > 0.0 {
            ease(((progress - a.offset) / span).clamp(0.0, 1.0))
        } else {
            1.0
        };
        let lerp = |x: f32, y: f32| x + (y - x) * t;
        Keyframe {
            offset: progress,
            opacity: match (a.opacity, b.opacity) {
                (Some(x), Some(y)) => Some(lerp(x, y)),
                (x, y) => x.or(y),
            },
            translate: [
                lerp(a.translate[0], b.translate[0]),
                lerp(a.translate[1], b.translate[1]),
            ],
            scale: [lerp(a.scale[0], b.scale[0]), lerp(a.scale[1], b.scale[1])],
            rotate_x: lerp(a.rotate_x, b.rotate_x),
            rotate_y: lerp(a.rotate_y, b.rotate_y),
            rotate_z: lerp(a.rotate_z, b.rotate_z),
            brightness: lerp(a.brightness, b.brightness),
        }
    }
}

/// CSS's `ease`, `cubic-bezier(0.25, 0.1, 0.25, 1)`: the curve's `y` where
/// its `x` is `x`.
fn ease(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let (x1, y1, x2, y2) = (0.25f32, 0.1f32, 0.25f32, 1.0f32);
    let bezier = |t: f32, p1: f32, p2: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
    };
    // Bisection on x(t), which rises from 0 to 1.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if bezier(mid, x1, x2) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bezier((lo + hi) / 2.0, y1, y2)
}

/// Text as the page shows it: tags dropped (`<br>` a line break), then
/// character references decoded.
fn plain_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('<') {
        out.push_str(&rest[..p]);
        let Some(end) = rest[p..].find('>') else {
            out.push_str(&rest[p..]);
            rest = "";
            break;
        };
        let tag = rest[p + 1..p + end].trim().to_ascii_lowercase();
        if tag == "br" || tag == "br/" || tag == "br /" {
            out.push('\n');
        }
        rest = &rest[p + end + 1..];
    }
    out.push_str(rest);
    // References after tags, as the page reads them.
    decode_entities(&out)
}

/// danoniplus's animations (`css/danoni_main.css` at `500f794`), but the
/// `ex-*` play effects, which move the field rather than a layer; `blur`
/// draws without its blur.
static ANIMATIONS: [Animation; 16] = [
    Animation {
        name: "leftToRight",
        keyframes: &[
            Keyframe {
                opacity: Some(0.0),
                translate: [-50.0, 0.0],
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(1.0),
                ..K
            },
        ],
    },
    Animation {
        name: "upToDown",
        keyframes: &[
            Keyframe {
                opacity: Some(0.0),
                translate: [0.0, -50.0],
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(1.0),
                ..K
            },
        ],
    },
    Animation {
        name: "leftToRightFade",
        keyframes: &[
            Keyframe {
                opacity: Some(0.0),
                translate: [-30.0, 0.0],
                ..K
            },
            Keyframe {
                offset: 0.3,
                opacity: Some(1.0),
                ..K
            },
            Keyframe {
                offset: 0.6,
                opacity: Some(1.0),
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(0.0),
                translate: [30.0, 0.0],
                ..K
            },
        ],
    },
    Animation {
        name: "upToDownFade",
        keyframes: &[
            Keyframe {
                opacity: Some(0.0),
                translate: [0.0, -30.0],
                ..K
            },
            Keyframe {
                offset: 0.3,
                opacity: Some(1.0),
                ..K
            },
            Keyframe {
                offset: 0.8,
                opacity: Some(1.0),
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(0.0),
                translate: [0.0, 10.0],
                ..K
            },
        ],
    },
    Animation {
        name: "slowlyAppearing",
        keyframes: &[
            Keyframe {
                opacity: Some(0.5),
                ..K
            },
            Keyframe {
                offset: 0.8,
                opacity: Some(0.5),
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(1.0),
                ..K
            },
        ],
    },
    Animation {
        name: "fromBig",
        keyframes: &[
            Keyframe {
                opacity: Some(0.0),
                scale: [1.5, 1.5],
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(1.0),
                ..K
            },
        ],
    },
    Animation {
        name: "smallToNormalY",
        keyframes: &[
            Keyframe {
                opacity: Some(0.0),
                scale: [1.0, 0.0],
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(1.0),
                ..K
            },
        ],
    },
    Animation {
        name: "spinX",
        keyframes: &[
            K,
            Keyframe {
                offset: 1.0,
                rotate_x: 360.0,
                ..K
            },
        ],
    },
    Animation {
        name: "spinY",
        keyframes: &[
            K,
            Keyframe {
                offset: 1.0,
                rotate_y: 360.0,
                ..K
            },
        ],
    },
    Animation {
        name: "spinZ",
        keyframes: &[
            K,
            Keyframe {
                offset: 1.0,
                rotate_z: 360.0,
                ..K
            },
        ],
    },
    Animation {
        name: "blur",
        keyframes: &[K, Keyframe { offset: 1.0, ..K }],
    },
    Animation {
        name: "brightToDark",
        keyframes: &[
            Keyframe {
                brightness: 0.0,
                ..K
            },
            Keyframe {
                offset: 0.3,
                brightness: 2.0,
                ..K
            },
            Keyframe {
                offset: 0.7,
                brightness: 0.0,
                ..K
            },
            Keyframe { offset: 1.0, ..K },
        ],
    },
    Animation {
        name: "fadeIn0",
        keyframes: &[
            Keyframe {
                opacity: Some(0.0),
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(1.0),
                ..K
            },
        ],
    },
    Animation {
        name: "fadeIn1",
        keyframes: &[
            Keyframe {
                opacity: Some(0.0),
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(1.0),
                ..K
            },
        ],
    },
    Animation {
        name: "fadeOut0",
        keyframes: &[
            Keyframe {
                opacity: Some(1.0),
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(0.0),
                ..K
            },
        ],
    },
    Animation {
        name: "fadeOut1",
        keyframes: &[
            Keyframe {
                opacity: Some(1.0),
                ..K
            },
            Keyframe {
                offset: 1.0,
                opacity: Some(0.0),
                ..K
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::formats::danoni::{Dos, DosFlags, import};

    fn song(data: &str) -> Song {
        let text = format!("|difData=5,N|left_data=300|{data}|");
        import(&Dos::parse(&text, DosFlags::default()))
            .unwrap()
            .songs
            .remove(0)
    }

    /// The song second of `kind`'s `n`th cue.
    fn cue_at(song: &Song, kind: &str, n: usize) -> f64 {
        match song
            .effects
            .iter()
            .filter(|e| e.kind == kind)
            .nth(n)
            .unwrap()
            .at
        {
            EffectTime::Seconds(s) => s,
            EffectTime::Beat(_) => unreachable!(),
        }
    }

    #[test]
    fn depths_hold_their_last_cue_until_cleared() {
        let s = song(
            "back_data=200,0,a.png,,10,20,30,40,0.5\n300,1,b.png\n400,0\n500,1,c.png\n600,ALL\n\
             |mask_data=250,0,Hello<br>world,,5,6,24,#f00",
        );
        let t = SpriteTrack::new(&s, 0, false);
        let at = |n| cue_at(&s, SPRITE_BACK, n);
        let shown = |t: &SpriteTrack, sec: f64| -> Vec<(StageLayer, u8, SpriteSource)> {
            t.at(sec)
                .into_iter()
                .map(|d| (d.layer, d.depth, d.source))
                .collect()
        };
        let img = |n: &str| SpriteSource::Image(Rc::from(n));
        assert!(shown(&t, at(0) - 0.01).is_empty());
        let first = t.at(at(0) + 0.01);
        assert_eq!(
            (
                first[0].x,
                first[0].y,
                first[0].w,
                first[0].h,
                first[0].opacity
            ),
            (10.0, 20.0, 30.0, 40.0, 0.5)
        );
        // Back's depths first, then the mask's text.
        let mid = shown(&t, at(1) + 0.01);
        assert_eq!(mid.len(), 3);
        assert_eq!(mid[0], (StageLayer::Back, 0, img("a.png")));
        assert_eq!(mid[1], (StageLayer::Back, 1, img("b.png")));
        assert_eq!(
            mid[2],
            (
                StageLayer::Mask,
                0,
                SpriteSource::Text {
                    text: Rc::from("Hello\nworld"),
                    size: 24,
                    color: Rc::from("#f00"),
                }
            )
        );
        // Depth 0 cleared, depth 1 replaced, then everything cleared.
        let later = shown(&t, at(3) + 0.01);
        assert_eq!(later[0], (StageLayer::Back, 1, img("c.png")));
        assert_eq!(shown(&t, at(4) + 0.01).len(), 1, "the mask stays");
        assert_eq!(t.sources().len(), 4);
        assert_eq!(t.stage(), (500.0, 500.0));
    }

    #[test]
    fn an_empty_rev_header_shows_nothing_under_reverse() {
        let s = song("back_data=200,0,a.png|backRev_data=-");
        assert!(SpriteTrack::new(&s, 0, true).is_empty());
        assert!(!SpriteTrack::new(&s, 0, false).is_empty());
    }

    #[test]
    fn a_play_from_start_frame_shows_no_earlier_cue() {
        let s = song("startFrame=600|back_data=0,0,a.png\n700,1,b.png");
        let t = SpriteTrack::new(&s, 0, false);
        let shown: Vec<u8> = t.at(1e6).into_iter().map(|d| d.depth).collect();
        assert_eq!(shown, [1]);
    }

    #[test]
    fn text_tags_go_before_references() {
        let s = song("back_data=200,0,a *lt*b*gt* c<br>d*comma*e");
        let t = SpriteTrack::new(&s, 0, false);
        let SpriteSource::Text { text, .. } = &t.at(1e6)[0].source else {
            panic!()
        };
        assert_eq!(&**text, "a <b> c\nd\u{201a}e");
    }

    #[test]
    fn reverse_takes_the_rev_data_of_a_layer() {
        let s = song("back_data=200,0,a.png|backRev_data=200,0,r.png|mask_data=200,0,m.png");
        let shown = |reverse| -> Vec<SpriteSource> {
            SpriteTrack::new(&s, 0, reverse)
                .at(100.0)
                .into_iter()
                .map(|d| d.source)
                .collect()
        };
        assert_eq!(
            shown(true),
            [
                SpriteSource::Image(Rc::from("r.png")),
                SpriteSource::Image(Rc::from("m.png"))
            ]
        );
        assert_eq!(shown(false)[0], SpriteSource::Image(Rc::from("a.png")));
    }

    #[test]
    fn danoniplus_animations_play_once_as_css_plays_them() {
        // fadeIn0 over 60 frames (1 s); fill none, then forwards.
        let s = song(
            "back_data=200,0,a.png,,0,0,0,0,0.7,fadeIn0,60\n200,1,b.png,,0,0,0,0,1,fadeOut0,60,forwards\n\
             200,2,c.png,,0,0,0,0,1,spinY,60\n200,3,d.png,,0,0,0,0,1,leftToRightFade,60\n\
             200,4,e.png,,0,0,0,0,1,illustIn,60",
        );
        let t = SpriteTrack::new(&s, 0, false);
        let start = cue_at(&s, SPRITE_BACK, 0);
        let at = |sec: f64| t.at(start + sec);
        // Halfway: CSS's `ease` is at 0.8024 where x is 0.5.
        let half = at(0.5);
        assert!(
            (half[0].opacity - 0.8024).abs() < 1e-3,
            "{}",
            half[0].opacity
        );
        // Over: the element's own opacity again; a fill mode does not
        // hold an image's last keyframe (danoniplus writes it only into a
        // colour box's style).
        let over = at(1.5);
        assert_eq!(over[0].opacity, 0.7);
        assert_eq!(over[1].opacity, 1.0);
        assert!(half[1].opacity < 0.5);
        // spinY halfway: turned 360° × ease(0.5), squeezed by its cosine.
        let turned = (360.0 * ease(0.5)).to_radians().cos();
        assert!(
            (half[2].scale[0] - turned).abs() < 1e-5,
            "{}",
            half[2].scale[0]
        );
        // leftToRightFade starts 30 px left, transparent.
        let begin = at(0.0);
        assert_eq!((begin[3].translate[0], begin[3].opacity), (-30.0, 0.0));
        // A work's own animation: none, and noted.
        assert_eq!(half[4].opacity, 1.0);
        assert_eq!(t.unknown_animations(), ["illustIn"]);
    }

    #[test]
    fn what_is_not_reproduced_is_noted() {
        let s = song(
            "back_data=200,0,a.png,,0,0,0,0,1,illustIn,60\n300,1,[c]0/#fff,,0,0,10,10\n\
             400,2,b.png,,{g_sWidth/2},0|mask_data=200,0,c.png,,{12},5",
        );
        let notes = import_notes(&s);
        assert_eq!(notes.len(), 3, "{notes:?}");
        assert!(notes[0].ends_with("illustIn"));
        // `{12}` is a number; `{g_sWidth/2}` a script.
        let t = SpriteTrack::new(&s, 0, false);
        let mask = t
            .at(100.0)
            .into_iter()
            .find(|d| d.layer == StageLayer::Mask)
            .unwrap();
        assert_eq!(mask.x, 12.0);
    }

    #[test]
    fn ease_is_css_ease() {
        assert_eq!(ease(0.0), 0.0);
        assert!((ease(1.0) - 1.0).abs() < 1e-6);
        assert!((ease(0.25) - 0.4094).abs() < 1e-3);
    }
}
