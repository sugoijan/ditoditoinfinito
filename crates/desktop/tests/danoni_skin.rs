//! The derived masks must retain the exact alpha of the pinned originals.
use std::path::Path;

#[test]
fn danoni_masks_keep_upstream_geometry_and_alpha() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (original, adapted) in [
        ("arrow.svg", "_arrow shape.svg"),
        ("onigiri.svg", "_onigiri shape.svg"),
        ("arrowShadow.svg", "_arrow shadow.svg"),
        ("aaShadow.svg", "_onigiri shadow.svg"),
    ] {
        let original = std::fs::read(
            root.join("tools/noteskin-danoni/upstream/img")
                .join(original),
        )
        .unwrap();
        let adapted =
            std::fs::read(root.join("assets/noteskins/dance/DanOni").join(adapted)).unwrap();
        let (image, logical) = ddi_desktop::play::skins::decode_svg(&adapted).unwrap();
        assert_eq!(logical, (64, 64));
        assert_eq!(image.size, [256, 256]);
        // Unlike the runtime decoder, usvg accepts upstream's CSS class rules.
        let tree =
            resvg::usvg::Tree::from_data(&original, &resvg::usvg::Options::default()).unwrap();
        let mut reference = resvg::tiny_skia::Pixmap::new(256, 256).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                256.0 / tree.size().width(),
                256.0 / tree.size().height(),
            ),
            &mut reference.as_mut(),
        );
        for (pixel, expected) in image
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .zip(reference.data().as_chunks::<4>().0)
        {
            assert_eq!(pixel[3], expected[3], "changed mask alpha");
            if pixel[3] != 0 {
                assert_eq!(&pixel[..3], &[255; 3], "mask must be tintable white");
            }
        }
    }
    for (state, rgb, alpha) in [
        ("inactive", [102_u8, 0, 255], 191_u8),
        ("active", [153, 153, 51], 191),
        ("dropped", [153, 153, 153], 255),
    ] {
        let bytes =
            std::fs::read(root.join(format!("assets/noteskins/dance/DanOni/_bar {state}.svg")))
                .unwrap();
        let (image, _) = ddi_desktop::play::skins::decode_svg(&bytes).unwrap();
        let center = &image.data[(128 * 256 + 128) * 4..][..4];
        assert!(center[..3].iter().zip(rgb).all(|(a, b)| a.abs_diff(b) <= 1));
        assert!(center[3].abs_diff(alpha) <= 1);
        assert_eq!(image.data[3], 0, "strip margins stay transparent");
    }
}
