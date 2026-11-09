// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_dance_mat::{Geometry, MatLayout, Tile, cell_position};
#[test]
fn identities_and_single_coordinates_round_trip_without_rounding_changes() {
    for id in 1..=18 {
        let t = Tile::try_from(id).unwrap();
        assert_eq!(Some(t), Tile::new(t.pad(), t.cell()));
        assert_eq!(serde_json::from_str::<Tile>(&id.to_string()).unwrap(), t);
        if id <= 9 {
            for pitch in [24., 15., 0., -0., 17.25] {
                let expected = match id {
                    1 => [-pitch, -pitch],
                    2 => [0., -pitch],
                    3 => [pitch, -pitch],
                    4 => [-pitch, 0.],
                    5 => [0., 0.],
                    6 => [pitch, 0.],
                    7 => [-pitch, pitch],
                    8 => [0., pitch],
                    _ => [pitch, pitch],
                };
                assert_eq!(
                    cell_position(id, pitch).unwrap().map(f64::to_bits),
                    expected.map(f64::to_bits)
                );
            }
            assert_eq!(Geometry::single().position(t), cell_position(id, 24.));
        } else {
            assert!(Geometry::single().position(t).is_none());
        }
    }
    for bad in [0, 19, 255] {
        assert!(serde_json::from_str::<Tile>(&bad.to_string()).is_err());
    }
    assert_eq!(Tile::new(1, 5).unwrap().to_string(), "R5");
}
#[test]
fn double_geometry_has_six_columns_and_a_separate_configurable_seam() {
    let g = Geometry::new(2, 24., 0.).unwrap();
    assert_eq!(g.bounds_mm(), [[-72., -36.], [72., 36.]]);
    assert_eq!(g.position(Tile::new(0, 6).unwrap()), Some([-12., 0.]));
    assert_eq!(g.position(Tile::new(1, 4).unwrap()), Some([12., 0.]));
    assert_eq!(g.tile_half_mm(), 11.5);
    let g = Geometry::new(2, 24., 4.).unwrap();
    assert_eq!(g.position(Tile::new(0, 6).unwrap()), Some([-14., 0.]));
    assert_eq!(g.position(Tile::new(1, 4).unwrap()), Some([14., 0.]));
    assert_eq!(g.bounds_mm(), [[-74., -36.], [74., 36.]]);
    let wire = serde_json::to_string(&g).unwrap();
    assert_eq!(serde_json::from_str::<Geometry>(&wire).unwrap(), g);
    for bad in [
        r#"{"version":2,"pads":2,"pitch_mm":24,"seam_gap_mm":0}"#,
        r#"{"version":1,"pads":3,"pitch_mm":24,"seam_gap_mm":0}"#,
        r#"{"version":1,"pads":1,"pitch_mm":24,"seam_gap_mm":1}"#,
    ] {
        assert!(serde_json::from_str::<Geometry>(bad).is_err());
    }
    assert!(Geometry::new(2, f64::NAN, 0.).is_err());
    assert!(Geometry::new(2, 24., -1.).is_err());
    assert!(Geometry::new(2, f64::MAX, 0.).is_err());
}
#[test]
fn lane_maps_and_physical_regions_preserve_logical_policy() {
    for (id, panels, masks) in [
        (
            "dance-double",
            vec![4, 2, 8, 6, 13, 11, 17, 15],
            vec![0x00f, 0x01e, 0x078, 0x0f0],
        ),
        ("pump-halfdouble", vec![5, 9, 3, 10, 16, 14], vec![0x03f]),
        (
            "pump-double",
            vec![1, 7, 5, 9, 3, 10, 16, 14, 18, 12],
            vec![0x01f, 0x0fc, 0x3e0],
        ),
        (
            "danoni-9A",
            vec![4, 2, 8, 6, 5, 13, 11, 17, 15],
            vec![0x01f, 0x03e, 0x0f8, 0x1f0],
        ),
    ] {
        let l = MatLayout::from_id(id).unwrap();
        assert_eq!(l.pads(), 2);
        assert!(l.single_panels().is_none());
        assert_eq!(
            l.fixed_panels(),
            (id != "danoni-9A").then_some(panels.as_slice())
        );
        assert_eq!(l.no_stretch_regions().unwrap(), masks);
        for (i, panel) in panels.iter().enumerate() {
            assert_eq!(l.targets(i)[0].id(), *panel);
        }
        for a in 0..l.lane_count() {
            for b in 0..l.lane_count() {
                let logical = masks
                    .iter()
                    .any(|mask| (1u16 << a | 1u16 << b) & !mask == 0);
                let physical = l
                    .targets(a)
                    .iter()
                    .any(|&ta| l.targets(b).iter().any(|&tb| l.pair_allowed(ta, tb)));
                assert_eq!(logical, physical, "{id} lanes {a}/{b}");
            }
        }
        for t in Geometry::new(2, 24., 0.).unwrap().tiles() {
            assert_eq!(
                l.unscored_tiles().any(|x| x == t),
                l.lane_for_tile(t).is_none()
            );
        }
    }
    let oni = MatLayout::from_id("danoni-9A").unwrap();
    assert_eq!(
        oni.targets(4).iter().map(|t| t.id()).collect::<Vec<_>>(),
        [5, 14]
    );
    assert!(!oni.pair_allowed(Tile::try_from(5).unwrap(), Tile::try_from(14).unwrap()));
    assert!(!oni.pair_allowed(Tile::try_from(14).unwrap(), Tile::try_from(4).unwrap()));
    assert!(oni.pair_allowed(Tile::try_from(5).unwrap(), Tile::try_from(4).unwrap()));
    assert!(oni.targets(9).is_empty());
}
