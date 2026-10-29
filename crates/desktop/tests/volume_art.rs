//! The quick volume's icon (`assets/ui/volume.svg`) in the bounded SVG
//! subset both shells draw: its four cells, muted then each level with
//! one more wave.
#[test]
fn the_volume_icon_has_a_cell_per_level() {
    let text = include_str!("../../../assets/ui/volume.svg");
    assert_eq!(ddi_noteskin::svg::check(text).unwrap(), (256, 64));
    let rgba = ddi_noteskin::raster::rasterize(text, 256, 64).unwrap();
    // What is drawn right of the speaker in each cell: the mute's cross,
    // then the waves.
    let right = |cell: usize| {
        (0..64)
            .flat_map(|y| (cell * 64 + 34..(cell + 1) * 64).map(move |x| (x, y)))
            .filter(|&(x, y)| rgba[(y * 256 + x) * 4 + 3] > 128)
            .count()
    };
    assert!(right(0) > 100, "the cross");
    assert!(right(1) > 50 && right(1) < right(2) && right(2) < right(3));
    // The same speaker in every cell.
    let speaker = |cell: usize| {
        (0..64)
            .flat_map(|y| (cell * 64..cell * 64 + 32).map(move |x| (x, y)))
            .map(|(x, y)| rgba[(y * 256 + x) * 4 + 3] as u32)
            .sum::<u32>()
    };
    assert!((1..4).all(|c| speaker(c) == speaker(0)));
}
