//! Shared geometry and progress for the song list → prompt → play backdrop.
//! Visual time only: this never changes a song's clock or its judgements.

pub const BASE_SIDE: f32 = 256.0;
pub const CONTENT_SIDE: f32 = 1024.0;
pub const BASE_ALPHA: f32 = 0.2;
pub const GROUND: [f32; 3] = [5.0 / 255.0, 2.0 / 255.0, 13.0 / 255.0];
pub const PROMPT_SECONDS: f32 = 0.45;
pub const PLAY_SECONDS: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reveal {
    /// The framed content emerging from its fullscreen copy (0 → 1).
    pub position: f32,
    /// The last unblur, from the prompt's soft content to sharp play (0 → 1).
    pub clarity: f32,
}

impl Default for Reveal {
    fn default() -> Self {
        Self {
            position: 1.0,
            clarity: 1.0,
        }
    }
}

pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Reveal {
    /// Opacities for sharp, strong blur, soft blur, in that drawing order.
    /// Their composite covers exactly `position` of the base.
    pub fn alphas(self) -> [f32; 3] {
        let p = self.position.clamp(0.0, 1.0);
        let c = self.clarity.clamp(0.0, 1.0);
        let soft = p * p * (1.0 - c);
        let strong_weight = p * (1.0 - p) * (1.0 - c);
        let divide = |a: f32, b: f32| {
            if b > 0.000001 {
                (a / b).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        [
            divide(p * c, 1.0 - soft - strong_weight),
            divide(strong_weight, 1.0 - soft),
            soft,
        ]
    }

    pub fn playing(from: f32, elapsed: f32, reduced: bool) -> Self {
        Self::continuing(
            Self {
                position: from,
                clarity: 0.0,
            },
            elapsed,
            reduced,
        )
    }

    /// The entrance into the play from `from`, `elapsed` seconds on: what
    /// is left of the unblur takes its share of [`PLAY_SECONDS`], the
    /// framed copy reaching its place with it. A start prompt's loading
    /// screen runs it (`docs/plans/start-prompt.md`); the play's first
    /// frame continues from where that left it, nothing at all after a
    /// wait.
    pub fn continuing(from: Reveal, elapsed: f32, reduced: bool) -> Self {
        let p = from.position.clamp(0.0, 1.0);
        let c = from.clarity.clamp(0.0, 1.0);
        let left = 1.0 - c;
        let t = if reduced || left <= 0.0 {
            1.0
        } else {
            ease(elapsed / (PLAY_SECONDS * left))
        };
        Self {
            position: p + (1.0 - p) * t,
            clarity: c + left * t,
        }
    }

    pub fn rect(self, width: f32, height: f32, aspect: Option<f32>) -> [f32; 4] {
        let to = frame_rect(width, height, aspect);
        let from = [0.0, 0.0, width, height];
        std::array::from_fn(|i| from[i] + (to[i] - from[i]) * self.position.clamp(0.0, 1.0))
    }
}

pub fn frame_rect(w: f32, h: f32, aspect: Option<f32>) -> [f32; 4] {
    match aspect.filter(|a| a.is_finite() && *a > 0.0) {
        Some(a) if w / h > a => [(w - h * a) / 2.0, 0.0, h * a, h],
        Some(a) => [0.0, (h - w / a) / 2.0, w, w / a],
        None => [0.0, 0.0, w, h],
    }
}

/// The part of an `iw × ih` image that covers a `w × h` area, per axis,
/// as fractions of the image (the overflow cropped evenly); all of it when
/// either is empty.
pub fn cover_scale(w: f32, h: f32, iw: f32, ih: f32) -> [f32; 2] {
    if w <= 0.0 || h <= 0.0 || iw <= 0.0 || ih <= 0.0 {
        return [1.0, 1.0];
    }
    if iw / ih > w / h {
        [(w / h) / (iw / ih), 1.0]
    } else {
        [1.0, (iw / ih) / (w / h)]
    }
}

/// Centered crop, as `[left, top, right, bottom]`, preserving image proportions.
pub fn cover_uv(w: f32, h: f32, iw: f32, ih: f32) -> [f32; 4] {
    let [x, y] = cover_scale(w, h, iw, ih);
    [
        (1.0 - x) / 2.0,
        (1.0 - y) / 2.0,
        (1.0 + x) / 2.0,
        (1.0 + y) / 2.0,
    ]
}

/// A shell's transition, starting at its first drawn play frame.
#[derive(Clone, Copy, Debug)]
pub struct Entrance {
    from: Reveal,
    started: Option<f64>,
    reduced: bool,
}
impl Entrance {
    pub fn new(from: f32, reduced: bool) -> Self {
        Self::resume(
            Reveal {
                position: from,
                clarity: 0.0,
            },
            reduced,
        )
    }
    /// Continues from where the prompt's loading screen left the backdrop
    /// ([`Reveal::continuing`]).
    pub fn resume(from: Reveal, reduced: bool) -> Self {
        Self {
            from,
            started: None,
            reduced,
        }
    }
    pub fn sample(&mut self, now: f64) -> Reveal {
        let start = *self.started.get_or_insert(now);
        Reveal::continuing(self.from, (now - start).max(0.0) as f32, self.reduced)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_early_start_continues_the_prompt_instead_of_jumping() {
        assert_eq!(
            Reveal::playing(0.3, 0.0, false),
            Reveal {
                position: 0.3,
                clarity: 0.0
            }
        );
        assert_eq!(Reveal::playing(0.3, PLAY_SECONDS, false), Reveal::default());
        assert_eq!(Reveal::playing(0.3, 0.0, true), Reveal::default());
        let half = Reveal::playing(0.3, PLAY_SECONDS / 2.0, false);
        assert!((half.position - 0.65).abs() < 1e-6);
        assert_eq!(half.clarity, 0.5);
        // Loading can finish much later; the entrance starts at its first draw.
        let mut entrance = Entrance::new(0.3, false);
        assert_eq!(entrance.sample(100.0), Reveal::playing(0.3, 0.0, false));
        let later = entrance.sample(100.0 + f64::from(PLAY_SECONDS));
        assert_eq!(later, Reveal::default());
        // After the prompt's loading screen: the rest only, or nothing.
        let half = Reveal {
            position: 0.65,
            clarity: 0.5,
        };
        assert_eq!(Reveal::continuing(half, 0.0, false), half);
        assert_eq!(
            Reveal::continuing(half, PLAY_SECONDS / 2.0, false),
            Reveal::default()
        );
        let mut settled = Entrance::resume(Reveal::default(), false);
        assert_eq!(settled.sample(5.0), Reveal::default());
    }
    #[test]
    fn the_copy_contracts_into_the_fixed_frame() {
        assert_eq!(
            Reveal {
                position: 0.0,
                clarity: 0.0
            }
            .rect(1600.0, 900.0, Some(4.0 / 3.0)),
            [0.0, 0.0, 1600.0, 900.0]
        );
        assert_eq!(
            Reveal::default().rect(1600.0, 900.0, Some(4.0 / 3.0)),
            [200.0, 0.0, 1200.0, 900.0]
        );
        assert_eq!(
            cover_uv(1600.0, 900.0, 640.0, 480.0),
            [0.0, 0.125, 1.0, 0.875]
        );
    }
}
