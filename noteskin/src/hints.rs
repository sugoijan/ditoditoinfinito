//! What a texture's file name says about it (StepMania 5.1,
//! `src/RageTexture.cpp`, `src/RageBitmapTexture.cpp`): the frame grid
//! (`Down Tap Note 4x2.png`), the displayed size (`(res 64x64)`) and
//! `(doubleres)`.

use crate::ini::string_to_int;

/// The hints in a file name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hints {
    /// `" WxH"` before a `.`, a space or the end, as written (checked
    /// against the image in [`Hints::sheet`]).
    pub frames: Option<(u32, u32)>,
    /// `(… res WxH …)`: the size the image is shown at.
    pub res: Option<(u32, u32)>,
    /// `doubleres` anywhere: shown at half its pixel size.
    pub doubleres: bool,
    /// `stretch` anywhere: the image is resized to its padded texture.
    pub stretch: bool,
}

impl Hints {
    /// Both patterns are case-insensitive (`Regex::Compile` uses
    /// `PCRE_CASELESS`), as are the word searches (the name is lowercased
    /// first). StepMania reads them from the whole path
    /// (`/NoteSkins/dance/<skin>/<file>`), so pass that.
    pub fn parse(name: &str) -> Hints {
        let lower = name.to_ascii_lowercase();
        Hints {
            frames: frame_grid(&lower),
            res: resolution(&lower),
            doubleres: lower.contains("doubleres"),
            stretch: lower.contains("stretch"),
        }
    }

    /// The sheet an image of `width × height` pixels makes, at most
    /// `max_size` on a side (the GPU's limit), with high-resolution textures
    /// on (the image keeps its pixels; `(doubleres)` only halves the size it
    /// is shown at).
    pub fn sheet(&self, width: u32, height: u32, max_size: u32) -> Sheet {
        // `GetFrameDimensionsFromFileName`: a hint larger than the image in
        // pixels is ignored (it runs before the size hints apply).
        let frames = match self.frames {
            Some((w, h)) if w <= width && h <= height => (w, h),
            _ => (1, 1),
        };
        let mut image = (width.min(max_size), height.min(max_size));
        let mut texture = (power_of_two(image.0), power_of_two(image.1));
        // Under 8 on a side the texture is raised to 8 and the image
        // stretched to it (`RageBitmapTexture::Create`, `:168-182`).
        let mut stretch = self.stretch;
        if texture.0 < 8 || texture.1 < 8 {
            stretch = true;
            texture = (texture.0.max(8), texture.1.max(8));
        }
        if stretch {
            image = texture;
        }
        let (mut sw, mut sh) = self.res.unwrap_or((width, height));
        if self.doubleres {
            sw /= 2;
            sh /= 2;
        }
        Sheet {
            frames,
            image,
            texture,
            source: (sw, sh),
        }
    }
}

/// A texture as StepMania lays it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sheet {
    /// Frames across and down, numbered row by row.
    pub frames: (u32, u32),
    /// The image's size in pixels (`m_iImageWidth`/`Height`).
    pub image: (u32, u32),
    /// The image padded to powers of two (`m_iTextureWidth`/`Height`):
    /// texture-coordinate metrics are fractions of this.
    pub texture: (u32, u32),
    /// The size it is shown at (`m_iSourceWidth`/`Height` after the hints).
    pub source: (u32, u32),
}

impl Sheet {
    pub fn frame_count(&self) -> u64 {
        u64::from(self.frames.0) * u64::from(self.frames.1)
    }

    /// The size one frame is shown at, in StepMania pixels (a lane is 64).
    pub fn frame_size(&self) -> (f32, f32) {
        (
            self.source.0 as f32 / self.frames.0 as f32,
            self.source.1 as f32 / self.frames.1 as f32,
        )
    }

    /// Frame `i`'s rectangle `[left, top, right, bottom]` as a fraction of
    /// the image (not the padded texture: a renderer that does not pad uses
    /// these directly). `RageTexture::CreateFrameRects`.
    pub fn frame_rect(&self, i: u64) -> [f32; 4] {
        let (cols, rows) = self.frames;
        let i = i % self.frame_count().max(1);
        let (x, y) = ((i % u64::from(cols)) as f32, (i / u64::from(cols)) as f32);
        [
            x / cols as f32,
            y / rows as f32,
            (x + 1.0) / cols as f32,
            (y + 1.0) / rows as f32,
        ]
    }

    /// Converts a StepMania texture-coordinate offset (a fraction of the
    /// padded texture) to a fraction of the image.
    pub fn from_padded(&self, offset: [f32; 2]) -> [f32; 2] {
        [
            offset[0] * self.texture.0 as f32 / self.image.0.max(1) as f32,
            offset[1] * self.texture.1 as f32 / self.image.1.max(1) as f32,
        ]
    }
}

/// `power_of_two` (`src/RageUtil.cpp`): the smallest power of two not below
/// `n` (1 for 0).
pub fn power_of_two(n: u32) -> u32 {
    n.max(1).checked_next_power_of_two().unwrap_or(1 << 31)
}

/// `" ([0-9]+)x([0-9]+)([\. ]|$)"`, the leftmost match; nonsense values
/// (0, or too large for `stoi`) mean no grid.
fn frame_grid(name: &str) -> Option<(u32, u32)> {
    let b = name.as_bytes();
    for start in 0..b.len() {
        if b[start] != b' ' {
            continue;
        }
        let w_end = digits_end(b, start + 1);
        if w_end == start + 1 || b.get(w_end) != Some(&b'x') {
            continue;
        }
        let h_end = digits_end(b, w_end + 1);
        if h_end == w_end + 1 {
            continue;
        }
        if !matches!(b.get(h_end), None | Some(b'.') | Some(b' ')) {
            // Fewer height digits cannot help: a digit follows.
            continue;
        }
        let (w, h) = (
            string_to_int(&name[start + 1..w_end]),
            string_to_int(&name[w_end + 1..h_end]),
        );
        return (w > 0 && h > 0).then_some((w as u32, h as u32));
    }
    None
}

/// `"\([^\)]*res ([0-9]+)x([0-9]+).*\)"`: within a parenthesis (before its
/// first `)`), the last `res WxH` that a `)` follows somewhere later. Of the
/// `(`s before one `)`, the first sees everything the later ones see, so
/// only it is tried: one pass over the name.
fn resolution(name: &str) -> Option<(u32, u32)> {
    let b = name.as_bytes();
    let last_close = b.iter().rposition(|&c| c == b')')?;
    let mut open = 0;
    while open < b.len() {
        let start = open + b[open..].iter().position(|&c| c == b'(')?;
        let close = b[start + 1..]
            .iter()
            .position(|&c| c == b')')
            .map_or(b.len(), |p| start + 1 + p);
        // `[^\)]*` is greedy: the rightmost `res ` before the `)` that
        // matches wins.
        let mut found = None;
        let mut at = start + 1;
        while at < close {
            if !b[at..].starts_with(b"res ") {
                at += 1;
                continue;
            }
            let w_start = at + 4;
            let w_end = digits_end(b, w_start);
            if w_end > w_start && b.get(w_end) == Some(&b'x') {
                let h_end = digits_end(b, w_end + 1);
                if h_end > w_end + 1 && last_close >= h_end {
                    found = Some((
                        string_to_int(&name[w_start..w_end]),
                        string_to_int(&name[w_end + 1..h_end]),
                    ));
                }
            }
            at += 1;
        }
        if let Some((w, h)) = found {
            // `GetResolutionFromFileName`: nonsense values are ignored.
            return (w > 0 && h > 0).then_some((w as u32, h as u32));
        }
        // The next `(` after this one's `)`.
        open = close + 1;
    }
    None
}

fn digits_end(b: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_grids() {
        let g = |n: &str| Hints::parse(n).frames;
        assert_eq!(g("Down Tap Note 4x2.png"), Some((4, 2)));
        assert_eq!(g("_down tap note 12x8.png"), Some((12, 8)));
        assert_eq!(g("_arrow 1x8 (doubleres).png"), Some((1, 8)));
        assert_eq!(g("Receptor 4X1.png"), Some((4, 1)));
        assert_eq!(g("Tap Note 4x2"), Some((4, 2)));
        // Needs a space before and a dot, a space or the end after.
        assert_eq!(g("Note4x2.png"), None);
        assert_eq!(g("Note 4x2b.png"), None);
        assert_eq!(g("Note 0x2.png"), None);
        assert_eq!(g("Note 99999999999x2.png"), None);
        // The leftmost match.
        assert_eq!(g("a 2x3 b 4x5.png"), Some((2, 3)));
        assert_eq!(g("a 2x3b 4x5.png"), Some((4, 5)));
    }

    #[test]
    fn resolutions_and_doubleres() {
        let h = Hints::parse("Down Hold LiftTail Active (res 64x64).png");
        assert_eq!(h.res, Some((64, 64)));
        assert!(!h.doubleres);
        assert_eq!(
            Hints::parse("Foo (dither, res 512x128).png").res,
            Some((512, 128))
        );
        assert_eq!(Hints::parse("Foo (RES 32x16).png").res, Some((32, 16)));
        assert_eq!(Hints::parse("Foo res 32x16.png").res, None);
        assert_eq!(Hints::parse("Foo (res 32x16.png").res, None);
        assert_eq!(Hints::parse("Foo (res 0x16).png").res, None);
        // The `)` may come after other parentheses.
        assert_eq!(Hints::parse("Foo (res 8x8 (x)).png").res, Some((8, 8)));
        let d = Hints::parse("_mine (DoubleRes).png");
        assert!(d.doubleres);
        assert_eq!(d.res, None);
    }

    #[test]
    fn sheets() {
        // `_down tap note 8x8 (doubleres).png` at 1024×1024: 128 px frames
        // shown at 64.
        let s = Hints::parse("_down tap note 8x8 (doubleres).png").sheet(1024, 1024, 8192);
        assert_eq!(s.frames, (8, 8));
        assert_eq!(s.source, (512, 512));
        assert_eq!(s.frame_size(), (64.0, 64.0));
        assert_eq!(s.frame_rect(9), [0.125, 0.125, 0.25, 0.25]);
        assert_eq!(s.frame_rect(64), s.frame_rect(0));
        // A grid larger than the image is ignored.
        let s = Hints::parse("x 16x16.png").sheet(8, 8, 8192);
        assert_eq!(s.frames, (1, 1));
        // `(res)` then `(doubleres)`.
        let s = Hints::parse("x (res 128x64) (doubleres).png").sheet(300, 100, 8192);
        assert_eq!(s.source, (64, 32));
        assert_eq!(s.texture, (512, 128));
        assert_eq!(
            s.from_padded([0.125, 0.5]),
            [0.125 * 512.0 / 300.0, 0.5 * 128.0 / 100.0]
        );
        // Capped to the GPU's limit.
        let s = Hints::parse("x.png").sheet(9000, 10, 8192);
        assert_eq!(s.image, (8192, 10));
        // A texture under 8 px a side is raised to 8 and the image stretched
        // to it; 5 px already pads to 8.
        let s = Hints::parse("x.png").sheet(3, 3, 8192);
        assert_eq!((s.image, s.texture), ((8, 8), (8, 8)));
        assert_eq!(s.from_padded([0.5, 0.5]), [0.5, 0.5]);
        let s = Hints::parse("x.png").sheet(5, 5, 8192);
        assert_eq!((s.image, s.texture), ((5, 5), (8, 8)));
        let s = Hints::parse("body (stretch).png").sheet(100, 60, 8192);
        assert_eq!((s.image, s.texture), ((128, 64), (128, 64)));
        // No overflow with huge grids.
        let s = Hints::parse("x 65536x65536.png").sheet(65536, 65536, 8192);
        assert_eq!(s.frame_count(), 1 << 32);
        assert_eq!(s.frame_rect(1 << 33), s.frame_rect(0));
        assert_eq!(power_of_two(0), 1);
        assert_eq!(power_of_two(64), 64);
        assert_eq!(power_of_two(65), 128);
        assert_eq!(power_of_two(u32::MAX), 1 << 31);
    }
}
