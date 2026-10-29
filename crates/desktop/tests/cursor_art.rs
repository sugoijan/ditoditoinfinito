//! Cursor masks use the same bounded SVG path on both shells.
#[test]
fn both_cursor_variants_have_charge_frames_and_soft_interior_shadows() {
    let text = include_str!("../../../assets/ui/cursors.svg");
    assert_eq!(ddi_noteskin::svg::check(text).unwrap(), (768, 192));
    let rgba = ddi_noteskin::raster::rasterize(text, 768, 192).unwrap();
    let pixel = |x: usize, y: usize| &rgba[(y * 768 + x) * 4..][..4];
    for row in 0..2 {
        let count = |col: usize| {
            (row * 96..(row + 1) * 96)
                .flat_map(|y| (col * 96..(col + 1) * 96).map(move |x| (x, y)))
                .filter(|&(x, y)| pixel(x, y)[3] > 100)
                .count()
        };
        assert!(count(0) > 100, "band {row}");
        assert!(count(1) > 10 && count(1) < count(2) && count(2) < count(3));
        let mut soft_inside = 0;
        for y in row * 96..(row + 1) * 96 {
            for x in 0..96 {
                let a = pixel(4 * 96 + x, y)[3];
                if pixel(x, y)[3] == 0 && a > 0 && a < 100 {
                    soft_inside += 1;
                }
            }
        }
        assert!(soft_inside > 100, "blur reaches transparent edges {row}");
    }
    let burst = include_str!("../../../assets/ui/cursor-burst.svg");
    ddi_noteskin::svg::check(burst).unwrap();
    ddi_noteskin::raster::rasterize(burst, 96, 96).unwrap();
}
