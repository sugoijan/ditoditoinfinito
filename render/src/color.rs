//! Colour spaces.
//!
//! Colours in the scene and skin are written as they should appear on screen
//! (sRGB-encoded, like CSS colours). On an sRGB target the GPU blends in
//! linear light and encodes on write, so the renderer converts them to
//! linear first; on a plain target they pass through unchanged. Either way
//! a solid colour looks the same.

/// One sRGB-encoded component to linear light (IEC 61966-2-1).
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// `rgba` (sRGB-encoded, straight alpha) as the target expects it.
pub fn to_target(rgba: [f32; 4], srgb_target: bool) -> [f32; 4] {
    if !srgb_target {
        return rgba;
    }
    [
        srgb_to_linear(rgba[0]),
        srgb_to_linear(rgba[1]),
        srgb_to_linear(rgba[2]),
        rgba[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_matches_the_srgb_curve() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        // 50 % grey in sRGB is about 21.4 % linear light.
        assert!((srgb_to_linear(0.5) - 0.214).abs() < 1e-3);
        // The linear segment near black.
        assert!((srgb_to_linear(0.02) - 0.02 / 12.92).abs() < 1e-9);
        assert_eq!(to_target([0.5, 0.2, 0.9, 0.3], false), [0.5, 0.2, 0.9, 0.3]);
        let t = to_target([0.5, 0.2, 0.9, 0.3], true);
        assert_eq!(t[3], 0.3);
        assert!((t[0] - srgb_to_linear(0.5)).abs() < 1e-9);
    }
}
