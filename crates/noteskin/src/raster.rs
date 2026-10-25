//! A skin's SVG images drawn into pixels, by the same code on both
//! shells: desktop calls it on its loading threads, the web in its SVG
//! worker (`crates/svg-worker`), so an image looks the same on each and
//! the subset no longer has to be what browsers draw alike. Behind the
//! `raster` feature: the app's own wasm goes without it.

/// The most pixel work an SVG's layers may take ([`layer_work`]): sixteen
/// passes over the largest image a skin keeps (4096 pixels square).
pub const MAX_LAYER_WORK: f64 = 16.0 * 4096.0 * 4096.0;

/// What drawing `tree` at `w`×`h` pixels costs in layers, in pixels
/// visited: resvg draws every isolated group (one with an opacity, a mask,
/// a clip path or a filter) into a layer of its own, its box at that size
/// (at most five times the image each way), and passes over it once, once
/// more for its mask and its clip path, and once for each filter
/// primitive (three for a blur, a morphology's whole window for each
/// pixel, its radius scaled as resvg scales it: by the group's whole
/// transform as well as the image's size). Shapes are bounded by [`crate::svg`]'s count of drawn elements;
/// this bounds the layers, which a small file can make large.
pub fn layer_work(tree: &resvg::usvg::Tree, w: u32, h: u32) -> f64 {
    use resvg::usvg::{Group, Node, filter::Kind};
    let size = tree.size();
    let (sx, sy) = (
        f64::from(w) / f64::from(size.width()),
        f64::from(h) / f64::from(size.height()),
    );
    let (w, h) = (f64::from(w), f64::from(h));
    fn group(g: &Group, sx: f64, sy: f64, w: f64, h: f64) -> f64 {
        let mut work = 0.0;
        if g.should_isolate() {
            let b = g.abs_layer_bounding_box();
            let (left, top) = (
                (f64::from(b.left()) * sx).max(-2.0 * w),
                (f64::from(b.top()) * sy).max(-2.0 * h),
            );
            let (right, bottom) = (
                (f64::from(b.right()) * sx).min(3.0 * w),
                (f64::from(b.bottom()) * sy).min(3.0 * h),
            );
            let area = (right - left).max(0.0) * (bottom - top).max(0.0);
            let mut passes = 1.0
                + f64::from(u8::from(g.mask().is_some()))
                + f64::from(u8::from(g.clip_path().is_some()));
            let (gx, gy) = g.abs_transform().get_scale();
            let (rx, ry) = (sx * f64::from(gx), sy * f64::from(gy));
            for f in g.filters() {
                for p in f.primitives() {
                    passes += match p.kind() {
                        Kind::GaussianBlur(_) | Kind::DropShadow(_) => 3.0,
                        Kind::Morphology(m) => {
                            (2.0 * f64::from(m.radius_x().get()) * rx + 1.0)
                                * (2.0 * f64::from(m.radius_y().get()) * ry + 1.0)
                        }
                        _ => 1.0,
                    };
                }
            }
            work += area * passes;
        }
        for child in g.children() {
            child.subroots(|root| work += group(root, sx, sy, w, h));
            if let Node::Group(child) = child {
                work += group(child, sx, sy, w, h);
            }
        }
        work
    }
    group(tree.root(), sx, sy, w, h)
}

/// SVG `text` (checked) drawn at `w`×`h` pixels, stretched to them: RGBA
/// rows, not premultiplied. Refused when its layers would cost more than
/// [`MAX_LAYER_WORK`].
pub fn rasterize(text: &str, w: u32, h: u32) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_str(text, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let work = layer_work(&tree, w, h);
    if work > MAX_LAYER_WORK {
        return Err(format!(
            "its layers and filters would take {:.0} passes over the largest image, at most {:.0}",
            work / (4096.0 * 4096.0),
            MAX_LAYER_WORK / (4096.0 * 4096.0)
        ));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("an empty SVG")?;
    let tree_size = tree.size();
    let transform = resvg::tiny_skia::Transform::from_scale(
        w as f32 / tree_size.width(),
        h as f32 / tree_size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // tiny-skia's pixels are premultiplied; images are not.
    let mut data = pixmap.take();
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3];
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
            }
        }
    }
    Ok(data)
}

/// An SVG drawn by [`decode`].
pub struct Drawn {
    /// RGBA rows, not premultiplied.
    pub pixels: Vec<u8>,
    /// Their size.
    pub size: (u32, u32),
    /// The size the skin sees: the SVG's own, as a bitmap's.
    pub own: (u32, u32),
}

/// An SVG file checked and drawn at `scale` times its size
/// ([`crate::build::svg_scale`]; at most its `data-raster-scale`,
/// [`crate::svg::Checked::raster_size`]).
pub fn decode(bytes: &[u8], scale: f64) -> Result<Drawn, String> {
    if bytes.len() > crate::svg::MAX_SVG {
        return Err("the SVG is too large".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "the SVG is not text")?;
    let checked = crate::svg::check_svg(text)?;
    let (w, h) = checked.raster_size(scale);
    Ok(Drawn {
        pixels: rasterize(text, w, h)?,
        size: (w, h),
        own: checked.size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawn_at_four_times_unpremultiplied() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="4"><rect width="8" height="4" fill="#ff8000" fill-opacity="0.5"/></svg>"##;
        let Drawn {
            pixels: px,
            size: at,
            own,
        } = decode(svg.as_bytes(), crate::build::SVG_SCALE).unwrap();
        assert_eq!((at, own), ((32, 16), (8, 4)));
        assert_eq!(px.len(), 32 * 16 * 4);
        let mid = &px[(8 * 32 + 16) * 4..][..4];
        assert!(
            mid[0] == 255 && mid[1].abs_diff(128) <= 1 && mid[2] == 0,
            "{mid:?}"
        );
        assert!(mid[3].abs_diff(128) <= 1, "{mid:?}");
        assert_eq!(crate::svg::raster_size((4096, 100), 4.0), (4096, 100));
        assert_eq!(crate::svg::raster_size((2000, 10), 4.0), (4096, 20));
    }

    #[test]
    fn a_shadow_is_drawn_and_layers_are_bounded() {
        // A square's blurred shadow reaches past it.
        let shadow = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><filter id="s"><feGaussianBlur stdDeviation="1.5"/></filter><rect x="4" y="4" width="8" height="8" filter="url(#s)"/></svg>"##;
        let Drawn { pixels, size, .. } = decode(shadow.as_bytes(), 4.0).unwrap();
        let alpha = |x: u32, y: u32| pixels[((y * size.0 + x) * 4 + 3) as usize];
        assert!(alpha(32, 32) >= 250, "{}", alpha(32, 32));
        assert!((1..255).contains(&alpha(14, 32)), "{}", alpha(14, 32));
        assert_eq!(alpha(0, 0), 0);
        // One small file asking for a huge layer, many times over.
        let mut big = String::from(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024"><defs><filter id="m"><feMorphology radius="8"/></filter></defs>"##,
        );
        for _ in 0..4 {
            big.push_str(
                r##"<rect x="-2000" y="-2000" width="5000" height="5000" filter="url(#m)"/>"##,
            );
        }
        big.push_str("</svg>");
        assert!(crate::svg::check(&big).is_ok());
        let err = decode(big.as_bytes(), 4.0).err().unwrap();
        assert!(err.contains("passes over the largest image"), "{err}");
        // A small morphology radius made large by a group's scale: resvg
        // scales the radius by it, so the window is counted at that size.
        let scaled = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><defs><filter id="m"><feMorphology radius="8"/></filter></defs><g transform="scale(64)"><rect width="0.5" height="0.5" filter="url(#m)"/></g></svg>"##;
        assert!(crate::svg::check(scaled).is_ok());
        let tree = resvg::usvg::Tree::from_str(scaled, &resvg::usvg::Options::default()).unwrap();
        let work = layer_work(&tree, 256, 256);
        assert!(work > MAX_LAYER_WORK, "{work}");
    }
}
