//! The start menu's outline rain as particles over its 960 × 540 layout:
//! where each falls at a second, how deep it is, the shape and direction
//! it keeps until it wraps outside the view, and how quiet it is round
//! the middle. The menu draws them as sprites (`ddi_theme`'s `rain.rs`),
//! the generated rain background as strokes (`ddi_render`'s
//! `generative.rs`), and the web page's loader copies them in JavaScript
//! (`site/boot-art.js`). Evaluated in f64 so wasm and native agree.

/// The layout's size the particles fall over.
pub const WIDTH: f64 = 960.0;
pub const HEIGHT: f64 = 540.0;
/// Rows of depth, a line weight each.
pub const ROWS: usize = 8;

/// The study's deterministic seed.
pub fn seed(i: f64, salt: f64) -> f64 {
    ((i * 127.1 + salt * 311.7).sin() * 43758.5453).rem_euclid(1.0)
}

/// One particle at a second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// Its center in the layout.
    pub x: f64,
    pub y: f64,
    /// How near it is, 0 to 1: nearer ones fall faster, larger and
    /// brighter.
    pub depth: f64,
    /// Which fall this is of the particle, new each time it wraps, for
    /// the choices kept until then.
    pub identity: f64,
    /// Drawn as an onigiri rather than an arrow.
    pub onigiri: bool,
    /// The arrow's direction, in quarter turns.
    pub direction: u8,
    /// Its row of depth, below [`ROWS`].
    pub row: usize,
    /// Its fade in at the top and out at the bottom, 0 to 1.
    pub fade: f64,
    /// How much quieter it is round the middle, 0 to 1.
    pub quiet: f64,
}

impl Particle {
    /// How opaque it is drawn, before a look's own strength.
    pub fn alpha(&self) -> f64 {
        (0.13 + self.depth * 0.24) * self.fade * (1.0 - self.quiet)
    }
}

/// The `i`th particle at `time` seconds, falling at 0.8 to 2 times
/// `speed` layout units a second by its depth.
pub fn particle(i: usize, time: f64, speed: f64) -> Particle {
    let i = i as f64;
    let depth = seed(i, 2.0);
    let rate = speed * (0.8 + depth * 1.2);
    let travel = seed(i, 1.0) * 680.0 + (time + 8.0) * rate;
    let identity = i + (travel / 680.0).floor() * 4099.0;
    let y = -70.0 + travel.rem_euclid(680.0);
    let x = -50.0 + (seed(i, 3.0) * 1060.0 + (time + 8.0) * rate * 0.17).rem_euclid(1060.0);
    let fade = ((y + 70.0) / 80.0).min((610.0 - y) / 80.0).clamp(0.0, 1.0);
    let distance = ((x - 480.0).hypot(y - 290.0) - 60.0).max(0.0) / 370.0;
    let quiet = if distance < 0.6 {
        0.58 + (0.23 - 0.58) * distance / 0.6
    } else {
        0.23 * (1.0 - distance).max(0.0) / 0.4
    };
    Particle {
        x,
        y,
        depth,
        identity,
        onigiri: seed(identity, 6.0) < 0.01,
        direction: (seed(identity, 21.0) * 4.0).floor() as u8,
        row: (depth * (ROWS - 1) as f64).round() as usize,
        fade,
        quiet,
    }
}
