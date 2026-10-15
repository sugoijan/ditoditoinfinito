//! The virtual screen actors are placed on, as StepMania makes it
//! (`src/ScreenDimensions.cpp`, `src/ScreenDimensions.h`): a theme designs
//! for a size; a wider display keeps the design's height and widens the
//! screen, a narrower one keeps its width and heightens it. Positions are
//! in the screen's (virtual) pixels whatever the window's size and the
//! device's pixel ratio; [`Viewport`] maps them onto the surface.

use crate::actor::View;

/// `SCREEN_WIDTH` by `SCREEN_HEIGHT` for a theme's design size and a
/// display's aspect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VirtualScreen {
    pub width: f32,
    pub height: f32,
    /// The theme's design (`[Common] ScreenWidth`, `ScreenHeight`), whose
    /// aspect is its native one.
    pub design: [f32; 2],
}

impl VirtualScreen {
    /// `GetScreenWidth` and `GetScreenHeight` (`ScreenDimensions.cpp:34-55`):
    /// the dimension the aspect stretches scaled up and rounded up, the
    /// width then down to an even number (so that 16:9 at 480 lines is
    /// 854 wide, not 855).
    pub fn new(design: [f32; 2], aspect: f32) -> VirtualScreen {
        let native = design[0] / design[1];
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            native
        };
        let wide = if aspect > native {
            aspect / native
        } else {
            1.0
        };
        let tall = if aspect < native {
            native / aspect
        } else {
            1.0
        };
        let mut width = (design[0] * wide).ceil() as i64;
        width -= width % 2;
        VirtualScreen {
            width: width as f32,
            height: (design[1] * tall).ceil(),
            design,
        }
    }

    /// The screen for a surface of `size` (any unit: only its aspect
    /// counts).
    pub fn for_surface(design: [f32; 2], size: [f32; 2]) -> VirtualScreen {
        VirtualScreen::new(design, size[0] / size[1])
    }

    pub fn left(&self) -> f32 {
        0.0
    }

    pub fn right(&self) -> f32 {
        self.width
    }

    pub fn top(&self) -> f32 {
        0.0
    }

    pub fn bottom(&self) -> f32 {
        self.height
    }

    pub fn center_x(&self) -> f32 {
        self.width / 2.0
    }

    pub fn center_y(&self) -> f32 {
        self.height / 2.0
    }

    /// `ASPECT_SCALE_FACTOR`: how much wider than the design's aspect.
    pub fn aspect_scale_factor(&self) -> f32 {
        (self.width / self.height) / (self.design[0] / self.design[1])
    }

    /// `WideScale(AR4_3, AR16_9)` (`02 Utilities.lua`): `scale(SCREEN_WIDTH,
    /// 640, 854, a, b)`, a straight line through a 4:3 and a 16:9 screen
    /// 480 high, not clamped. Those widths are StepMania's whatever the
    /// design: themes 720 high get 16:9 values past `b`.
    pub fn wide_scale(&self, four_three: f32, sixteen_nine: f32) -> f32 {
        four_three + (self.width - 640.0) / (854.0 - 640.0) * (sixteen_nine - four_three)
    }
}

/// A virtual screen on a surface: the whole screen shown, as large as it
/// fits, centred (the rounding of [`VirtualScreen::new`] leaves at most a
/// pixel or so of the surface's aspect unused).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub screen: VirtualScreen,
    /// The surface's size in physical pixels.
    pub physical: [f32; 2],
    /// Physical pixels per CSS (logical) pixel: the device pixel ratio, or
    /// winit's scale factor. It changes at run time (a window moved to
    /// another monitor, the browser's zoom); a new viewport follows it.
    pub ratio: f32,
    /// Physical pixels per virtual pixel.
    pub scale: f32,
    /// Where the screen's origin is, in physical pixels.
    pub offset: [f32; 2],
}

impl Viewport {
    /// The screen of a theme designed at `design` on a surface `size` CSS
    /// pixels big at device pixel ratio `ratio`.
    pub fn new(design: [f32; 2], size: [f32; 2], ratio: f32) -> Viewport {
        let ratio = if ratio.is_finite() && ratio > 0.0 {
            ratio
        } else {
            1.0
        };
        let physical = [size[0] * ratio, size[1] * ratio];
        let screen = VirtualScreen::for_surface(design, size);
        let scale = (physical[0] / screen.width).min(physical[1] / screen.height);
        let offset = [
            (physical[0] - screen.width * scale) / 2.0,
            (physical[1] - screen.height * scale) / 2.0,
        ];
        Viewport {
            screen,
            physical,
            ratio,
            scale,
            offset,
        }
    }

    /// Virtual pixels to physical ones, as [`crate::Actor::draw_in`]
    /// takes a transform.
    pub fn transform(&self) -> [f32; 6] {
        [
            self.scale,
            0.0,
            0.0,
            self.scale,
            self.offset[0],
            self.offset[1],
        ]
    }

    /// What `fov` frames and shadows are measured on.
    pub fn view(&self) -> View {
        View {
            transform: self.transform(),
            size: [self.screen.width, self.screen.height],
        }
    }

    /// A point of the screen in physical pixels.
    pub fn to_physical(&self, p: [f32; 2]) -> [f32; 2] {
        [
            p[0] * self.scale + self.offset[0],
            p[1] * self.scale + self.offset[1],
        ]
    }

    /// A point in CSS pixels (a pointer's) on the screen.
    pub fn to_virtual(&self, css: [f32; 2]) -> [f32; 2] {
        [
            (css[0] * self.ratio - self.offset[0]) / self.scale,
            (css[1] * self.ratio - self.offset[1]) / self.scale,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `_fallback`'s design: 1 by 480, so any aspect widens it.
    const FALLBACK: [f32; 2] = [1.0, 480.0];

    #[test]
    fn stepmanias_screen_sizes() {
        // 16:9 at 480 lines: 853.33 rounded up, then down to even.
        let s = VirtualScreen::new(FALLBACK, 16.0 / 9.0);
        assert_eq!((s.width, s.height), (854.0, 480.0));
        assert_eq!((s.center_x(), s.center_y()), (427.0, 240.0));
        let s = VirtualScreen::new(FALLBACK, 4.0 / 3.0);
        assert_eq!((s.width, s.height), (640.0, 480.0));
        let s = VirtualScreen::new(FALLBACK, 16.0 / 10.0);
        assert_eq!(s.width, 768.0);
        // A 4:3 design on a 16:9 display keeps its height.
        let s = VirtualScreen::new([640.0, 480.0], 16.0 / 9.0);
        assert_eq!((s.width, s.height), (854.0, 480.0));
        assert!((s.aspect_scale_factor() - 854.0 / 480.0 / (4.0 / 3.0)).abs() < 1e-6);
        // Narrower than the design: the width is kept, the height grows.
        let s = VirtualScreen::new([854.0, 480.0], 4.0 / 3.0);
        assert_eq!((s.width, s.height), (854.0, 641.0));
        // Portrait.
        let s = VirtualScreen::new([1280.0, 720.0], 9.0 / 16.0);
        assert_eq!((s.width, s.height), (1280.0, 2276.0));
        // No aspect: the design's.
        let s = VirtualScreen::new([1280.0, 720.0], f32::NAN);
        assert_eq!((s.width, s.height), (1280.0, 720.0));
    }

    #[test]
    fn wide_scale_is_a_straight_line() {
        let four_three = VirtualScreen::new(FALLBACK, 4.0 / 3.0);
        let wide = VirtualScreen::new(FALLBACK, 16.0 / 9.0);
        assert_eq!(four_three.wide_scale(10.0, 20.0), 10.0);
        assert_eq!(wide.wide_scale(10.0, 20.0), 20.0);
        // 16:10 is in between; 21:9 past it.
        let mid = VirtualScreen::new(FALLBACK, 1.6).wide_scale(10.0, 20.0);
        assert!(mid > 10.0 && mid < 20.0);
        assert!(VirtualScreen::new(FALLBACK, 2.37).wide_scale(10.0, 20.0) > 20.0);
    }

    #[test]
    fn viewports_map_virtual_to_physical() {
        // A 1366×768 CSS window at 2×, a theme 480 high whatever the
        // width: 854×480 virtual (1366/768 is a little under 16:9), about
        // 3.2 physical pixels per virtual one.
        let v = Viewport::new(FALLBACK, [1366.0, 768.0], 2.0);
        assert_eq!(v.physical, [2732.0, 1536.0]);
        assert_eq!((v.screen.width, v.screen.height), (854.0, 480.0));
        assert_eq!(v.scale, 2732.0 / 854.0);
        // Centred vertically in what the rounding leaves.
        assert_eq!(v.offset[0], 0.0);
        assert!(v.offset[1] > 0.0 && v.offset[1] < 2.0);
        let c = v.to_physical([427.0, 240.0]);
        assert!((c[0] - 1366.0).abs() < 1e-3 && (c[1] - 768.0).abs() < 1e-3);
        // A pointer at the window's centre is the screen's centre.
        let p = v.to_virtual([683.0, 384.0]);
        assert!((p[0] - 427.0).abs() < 1e-3 && (p[1] - 240.0).abs() < 1e-3);
        // The same window at 1×: the same screen, half the pixels.
        let one = Viewport::new(FALLBACK, [1366.0, 768.0], 1.0);
        assert_eq!(one.screen, v.screen);
        assert!((one.scale * 2.0 - v.scale).abs() < 1e-6);
        let view = v.view();
        assert_eq!(view.size, [854.0, 480.0]);
        assert_eq!(view.transform, v.transform());
        // A design wider than the window's aspect grows taller instead:
        // 854×480 is a little wider than 16:9.
        let tall = Viewport::new([854.0, 480.0], [1280.0, 720.0], 1.0);
        assert_eq!((tall.screen.width, tall.screen.height), (854.0, 481.0));
        // An exact 16:9 design on a 16:9 window: whole pixels.
        let hd = Viewport::new([1280.0, 720.0], [1280.0, 720.0], 1.5);
        assert_eq!(
            (hd.screen.width, hd.screen.height, hd.scale),
            (1280.0, 720.0, 1.5)
        );
        assert_eq!(hd.offset, [0.0, 0.0]);
        // A ratio that is no number counts as 1.
        assert_eq!(
            Viewport::new([854.0, 480.0], [854.0, 480.0], 0.0).scale,
            1.0
        );
    }
}
