//! What a skin's images cost once loaded (`docs/plans/noteskin-svg.md`,
//! step 3): each image the skin asked for, with its file's hash, the size
//! the skin sees, the pixels decoded and what became of it, against the
//! skin's budget ([`MAX_PIXELS`]). Both shells fill it as they decode, so
//! an author (SkinCraft, `#/skin-preview`) sees which images are large,
//! which were left out, and which version of a file was drawn.

use serde::Serialize;

use crate::build::MAX_PIXELS;

/// A skin's images, in the order they were asked for.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ImageReport {
    pub images: Vec<ImageCost>,
}

/// One image of a skin.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImageCost {
    /// The folder's skin name and the file's path in it.
    pub skin: String,
    pub path: String,
    /// The file's [`hash`]; empty when it was not read.
    pub hash: String,
    pub svg: bool,
    /// The size the skin sees (an SVG's own, a bitmap's pixels).
    pub own: (u32, u32),
    /// The pixels kept decoded (and again on the GPU); `(0, 0)` when none.
    pub decoded: (u32, u32),
    pub outcome: Outcome,
}

/// What became of an image.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "why", rename_all = "snake_case")]
pub enum Outcome {
    /// Decoded (an SVG drawn by `ddi_noteskin::raster`).
    Kept,
    /// An SVG the browser drew, where the web's SVG worker could not.
    ByBrowser,
    /// Not decoded: the skin's images were already at [`MAX_PIXELS`].
    LeftOut,
    /// Not decoded: why.
    Failed(String),
    /// Decoded, the same pixels as the image at this path: kept once, as
    /// that one.
    Shared(String),
}

/// A hash of an image's RGBA rows at `size`, quick over many megabytes
/// (a word at a time): what the shells find images of the same pixels by
/// (`NoteskinPlay::share_textures`), confirming them where they can.
pub fn pixels_hash(size: [u32; 2], rgba: &[u8]) -> u64 {
    let mut h = 0x9e37_79b9_7f4a_7c15 ^ (u64::from(size[0]) << 32 | u64::from(size[1]));
    let (words, rest) = rgba.as_chunks::<8>();
    for w in words {
        h = (h ^ u64::from_le_bytes(*w)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h ^= h >> 31;
    }
    for &b in rest {
        h = (h ^ u64::from(b)).wrapping_mul(0x94d0_49bb_1331_11eb);
    }
    h ^ (h >> 29) ^ rgba.len() as u64
}

/// FNV-1a (64 bits) of a file's bytes, in 16 hex digits: what a report
/// and the bundled index name a file's content by.
pub fn hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

impl ImageCost {
    /// Skin `skin`'s image at `path`, not read yet: left out until it is
    /// (an SVG by its extension).
    pub fn left_out(skin: &str, path: &str) -> Self {
        ImageCost {
            skin: skin.to_string(),
            path: path.to_string(),
            hash: String::new(),
            svg: path.to_ascii_lowercase().ends_with(".svg"),
            own: (0, 0),
            decoded: (0, 0),
            outcome: Outcome::LeftOut,
        }
    }

    /// Decoded pixels over the size the skin sees, along the longer side.
    pub fn scale(&self) -> f64 {
        let own = self.own.0.max(self.own.1);
        if own == 0 {
            return 0.0;
        }
        f64::from(self.decoded.0.max(self.decoded.1)) / f64::from(own)
    }

    /// The pixels it keeps (none when it shares another's).
    pub fn pixels(&self) -> u64 {
        if matches!(self.outcome, Outcome::Shared(_)) {
            return 0;
        }
        u64::from(self.decoded.0) * u64::from(self.decoded.1)
    }
}

impl ImageReport {
    /// The pixels the skin keeps decoded.
    pub fn pixels(&self) -> u64 {
        self.images.iter().map(ImageCost::pixels).sum()
    }

    /// Says that skin `skin`'s image at `path` is the same pixels as the
    /// one at `with` (of the same skin or another), kept once.
    pub fn share(&mut self, skin: &str, path: &str, with: &str) {
        if let Some(i) = self.images.iter_mut().find(|i| {
            i.skin == skin
                && i.path == path
                && matches!(i.outcome, Outcome::Kept | Outcome::ByBrowser)
        }) {
            i.outcome = Outcome::Shared(with.to_string());
        }
    }

    /// Gives the image at `at` the hash, sizes and outcome of skin
    /// `skin`'s image at `first` (the last listed): an image not decoded,
    /// its document the same as that one's. Whether there was one.
    pub fn copy_outcome(&mut self, at: usize, skin: &str, first: &str) -> bool {
        let Some(found) = self
            .images
            .iter()
            .rev()
            .find(|c| c.skin == skin && c.path == first)
            .cloned()
        else {
            return false;
        };
        let Some(cost) = self.images.get_mut(at) else {
            return false;
        };
        cost.hash = found.hash;
        cost.own = found.own;
        cost.decoded = found.decoded;
        cost.outcome = found.outcome;
        true
    }

    /// The pixels a skin may keep.
    pub fn budget(&self) -> u64 {
        MAX_PIXELS
    }

    /// The skin's images' content in one [`hash`]: their paths and
    /// hashes, in sorted order (the shells list them as they decode), so
    /// two loads drew the same files exactly when it is the same.
    pub fn hash(&self) -> String {
        let mut entries: Vec<[&String; 3]> = self
            .images
            .iter()
            .map(|i| [&i.skin, &i.path, &i.hash])
            .collect();
        entries.sort();
        let mut all = Vec::new();
        for entry in entries {
            for part in entry {
                all.extend_from_slice(part.as_bytes());
                all.push(0);
            }
        }
        hash(&all)
    }

    /// One line: the images, their pixels against the budget, and what
    /// was not decoded.
    pub fn summary(&self) -> String {
        let svgs = self.images.iter().filter(|i| i.svg).count();
        let mb = |px: u64| px as f64 / f64::from(1 << 20);
        let mut line = format!(
            "{} images ({svgs} SVG), {:.1} of {:.0} Mpx ({:.0}%), images {}",
            self.images.len(),
            mb(self.pixels()),
            mb(self.budget()),
            100.0 * self.pixels() as f64 / self.budget() as f64,
            self.hash(),
        );
        let left = self
            .images
            .iter()
            .filter(|i| i.outcome == Outcome::LeftOut)
            .count();
        let failed = self
            .images
            .iter()
            .filter(|i| matches!(i.outcome, Outcome::Failed(_)))
            .count();
        let browser = self
            .images
            .iter()
            .filter(|i| i.outcome == Outcome::ByBrowser)
            .count();
        let shared = self
            .images
            .iter()
            .filter(|i| matches!(i.outcome, Outcome::Shared(_)))
            .count();
        for (n, what) in [
            (left, "left out (budget)"),
            (failed, "failed"),
            (browser, "drawn by the browser"),
            (shared, "the same pixels as another, kept once"),
        ] {
            if n > 0 {
                line.push_str(&format!("; {n} {what}"));
            }
        }
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_scale_and_hash() {
        let image = |path: &str, own, decoded, outcome| ImageCost {
            skin: "S".into(),
            path: path.into(),
            hash: hash(path.as_bytes()),
            svg: true,
            own,
            decoded,
            outcome,
        };
        let mut r = ImageReport {
            images: vec![
                image("a.svg", (64, 32), (256, 128), Outcome::Kept),
                image("b.svg", (64, 64), (0, 0), Outcome::LeftOut),
            ],
        };
        assert_eq!(r.pixels(), 256 * 128);
        assert_eq!(r.images[0].scale(), 4.0);
        assert!(r.summary().contains("2 images (2 SVG)"), "{}", r.summary());
        assert!(
            r.summary().ends_with("; 1 left out (budget)"),
            "{}",
            r.summary()
        );
        let before = r.hash();
        // Whatever order the shell listed them in.
        r.images.reverse();
        assert_eq!(r.hash(), before);
        r.images[1].hash = hash(b"changed");
        assert_ne!(r.hash(), before);
        assert_eq!(hash(b""), "cbf29ce484222325");
        // Pixels' hashes tell sizes and contents apart.
        let px = [0u8, 0, 0, 0, 255, 255, 255, 255];
        assert_eq!(pixels_hash([2, 1], &px), pixels_hash([2, 1], &px));
        assert_ne!(pixels_hash([2, 1], &px), pixels_hash([1, 2], &px));
        assert_ne!(pixels_hash([2, 1], &px), pixels_hash([2, 1], &[0; 8]));
        // The same pixels as another: kept once.
        r.images
            .push(image("c.svg", (64, 32), (256, 128), Outcome::Kept));
        r.share("S", "c.svg", "a.svg");
        assert_eq!(r.images[2].outcome, Outcome::Shared("a.svg".into()));
        assert_eq!(r.pixels(), 256 * 128);
        assert!(
            r.summary()
                .ends_with("1 the same pixels as another, kept once")
        );
    }
}
