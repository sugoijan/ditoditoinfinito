//! Images drawn here in place of StepMania's own `default` and `common`
//! skins' art (not under an explicit licence): what a noteskin's explosion
//! row or mine shows when it falls back to them. White, so the commands'
//! colours tint them as they tint StepMania's.

/// An image, sRGB-encoded RGBA8 with straight alpha.
pub struct Art {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The stand-in for StepMania's file `name`, sized as its frame grid
/// says (128 px a frame).
pub fn builtin(name: &str) -> Art {
    let lower = name.to_ascii_lowercase();
    let (cols, rows) = ddi_noteskin::hints::Hints::parse(&lower)
        .frames
        .unwrap_or((1, 1));
    let (cols, rows) = (cols.clamp(1, 8), rows.clamp(1, 8));
    let size = 128u32;
    let shape: fn(f32, f32, u32) -> f32 = if lower.contains("mine") && lower.contains("hitmine") {
        ring
    } else if lower.contains("mine") {
        mine
    } else if lower.contains("hold explosion") || lower.contains("roll explosion") {
        hold_glow
    } else if lower.contains("glow") || lower.contains("explosion") {
        glow
    } else if lower.contains("blank") {
        |_, _, _| 0.0
    } else {
        square
    };
    let (w, h) = (size * cols, size * rows);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let frame = (y / size) * cols + x / size;
            // -1..1 within the frame.
            let u = ((x % size) as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let v = ((y % size) as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let a = shape(u, v, frame).clamp(0.0, 1.0);
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[255, 255, 255, (a * 255.0).round() as u8]);
        }
    }
    Art {
        width: w,
        height: h,
        rgba,
    }
}

/// A soft disc fading out to the edge.
fn glow(u: f32, v: f32, _frame: u32) -> f32 {
    let r = (u * u + v * v).sqrt();
    (1.0 - r).max(0.0).powf(1.5)
}

/// A soft ring.
fn ring(u: f32, v: f32, _frame: u32) -> f32 {
    let r = (u * u + v * v).sqrt();
    (1.0 - ((r - 0.7) / 0.25).abs()).max(0.0)
}

/// A glow brightest in the middle; the second frame a little dimmer, so
/// the two alternate.
fn hold_glow(u: f32, v: f32, frame: u32) -> f32 {
    glow(u, v, 0) * if frame.is_multiple_of(2) { 1.0 } else { 0.7 }
}

/// A round mine with four spokes.
fn mine(u: f32, v: f32, _frame: u32) -> f32 {
    let r = (u * u + v * v).sqrt();
    let disc = smooth(0.62 - r, 0.04);
    let spoke = (smooth(0.12 - u.abs(), 0.03) + smooth(0.12 - v.abs(), 0.03)).min(1.0)
        * smooth(0.9 - r, 0.04);
    let hole = 1.0 - smooth(0.3 - r, 0.04) * 0.6;
    (disc * hole).max(spoke)
}

/// A filled square with a soft edge.
fn square(u: f32, v: f32, _frame: u32) -> f32 {
    smooth(0.9 - u.abs().max(v.abs()), 0.05)
}

fn smooth(x: f32, width: f32) -> f32 {
    (x / width + 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_has_the_frames_its_name_says() {
        let a = builtin("Fallback Hold Explosion 2x1.png");
        assert_eq!((a.width, a.height), (256, 128));
        assert_eq!(a.rgba.len(), 256 * 128 * 4);
        // Opaque in the middle, clear at the corner.
        let px = |a: &Art, x: u32, y: u32| a.rgba[((y * a.width + x) * 4 + 3) as usize];
        assert!(px(&a, 64, 64) > 200);
        assert_eq!(px(&a, 0, 0), 0);
        assert!(px(&a, 192, 64) < px(&a, 64, 64));
        let blank = builtin("_blank.png");
        assert!(blank.rgba.chunks(4).all(|p| p[3] == 0));
        let m = builtin("_mine (doubleres).png");
        assert!(px(&m, 64, 30) > 0);
    }
}
