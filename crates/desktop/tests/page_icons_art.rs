//! The pages' icons (`assets/ui/icons.svg`) in the bounded SVG subset
//! both shells draw: a cell each for play, stop and the eye.
#[test]
fn the_page_icons_have_a_cell_each() {
    let text = include_str!("../../../assets/ui/icons.svg");
    assert_eq!(ddi_noteskin::svg::check(text).unwrap(), (192, 64));
    let rgba = ddi_noteskin::raster::rasterize(text, 192, 64).unwrap();
    let alpha = |x: usize, y: usize| rgba[(y * 192 + x) * 4 + 3];
    let inked = |cell: usize| {
        (0..64)
            .flat_map(|y| (cell * 64..(cell + 1) * 64).map(move |x| (x, y)))
            .filter(|&(x, y)| alpha(x, y) > 128)
            .count()
    };
    // Each cell drawn, and kept inside it.
    for cell in 0..3 {
        assert!(inked(cell) > 400, "cell {cell}: {}", inked(cell));
        for y in 0..64 {
            assert!(alpha(cell * 64, y) < 16 && alpha(cell * 64 + 63, y) < 16);
        }
    }
    // The play triangle points right: more of it left of its middle.
    let half = |from: usize| {
        (0..64)
            .flat_map(|y| (from..from + 18).map(move |x| (x, y)))
            .filter(|&(x, y)| alpha(x, y) > 128)
            .count()
    };
    assert!(half(18) > half(36));
    // The eye: its pupil filled, a ring of nothing around it, its lids.
    assert!(alpha(128 + 32, 32) > 200);
    assert!(alpha(128 + 32, 32 - 13) < 64);
    assert!(alpha(128 + 32, 13) > 128);
}
