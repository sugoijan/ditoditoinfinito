use super::*;

fn rows(masks: &[u8], dt: f64, n: usize) -> Vec<Row> {
    (0..n)
        .map(|i| Row {
            seconds: i as f64 * dt,
            mask: masks[i % masks.len()],
        })
        .collect()
}
fn mean(rows: &[Row], axis: usize) -> f64 {
    demands(rows).iter().map(|d| d[axis]).sum::<f64>() / rows.len() as f64
}

#[test]
fn vectors_are_bounded_and_round_trip() {
    for (name, _) in PRESETS {
        let s = Skills::parse(name).unwrap();
        assert_eq!(Skills::parse(&s.to_string()).unwrap(), s);
        assert_eq!(Skills::parse(&s.vector()).unwrap(), s);
    }
    for input in [
        "1,1,1",
        "1,1,1,1,1,1,1,1,1",
        "1,1,1,1,1,1,1,0.49",
        "NaN,1,1,1,1,1,1",
        "inf,1,1,1,1,1,1",
        "0.499,1,1,1,1,1,1",
        "2.001,1,1,1,1,1,1",
        "1.0001,1,1,1,1,1,1",
    ] {
        assert!(Skills::parse(input).is_err(), "{input}");
    }
    let s = Skills::parse("0.5,2,1.001,1,1,1,1").unwrap();
    assert_eq!(s.to_string(), "0.5,2,1.001,1,1,1,1");
    // Seven values are Evasion 1, written back as seven; eight take it.
    assert_eq!(s.evasion(), 1.0);
    assert_eq!(Skills::parse("0.5,2,1.001,1,1,1,1,1").unwrap(), s);
    let e = Skills::parse("0.5,2,1.001,1,1,1,1,1.6").unwrap();
    assert_eq!(
        (e.evasion(), e.to_string()),
        (1.6, "0.5,2,1.001,1,1,1,1,1.6".into())
    );
    assert_eq!(Skills::parse(&e.vector()).unwrap(), e);
    assert_eq!(e.log_values(), s.log_values(), "Evasion bends no pattern");
    assert_eq!(s.with_evasion(1.6).unwrap(), e);
    // The presets: where the character suggests it, another Evasion.
    for name in PRESET_NAMES {
        let p = Skills::parse(name).unwrap();
        let want = match name {
            "technical" => 1.25,
            "chords" => 0.8,
            "evasive" => 1.6,
            "reckless" => 0.6,
            _ => 1.0,
        };
        assert_eq!(p.evasion(), want, "{name}");
    }
}

#[test]
fn matched_patterns_separate_all_seven_demands() {
    let singles = rows(&[1, 4, 2, 8], 0.12, 128);
    let slow = rows(&[1, 4, 2, 8], 0.5, 128);
    let jump = rows(&[3, 12], 0.12, 128);
    let hand = rows(&[7, 14], 0.12, 128);
    let jack = rows(&[1], 0.12, 128);
    let chordjack = rows(&[3], 0.12, 128);
    let mut irregular = singles.clone();
    for (i, r) in irregular.iter_mut().enumerate() {
        r.seconds = (i / 2) as f64 * 0.24 + if i % 2 == 1 { 0.08 } else { 0. };
    }
    let pairs = [
        (&singles, &slow),
        (&jump, &singles),
        (&hand, &singles),
        (&singles, &singles[..12].to_vec()),
        (&jack, &singles),
        (&chordjack, &jump),
        (&irregular, &singles),
    ];
    for (axis, (hard, easy)) in pairs.iter().enumerate() {
        assert!(
            mean(hard, axis) > mean(easy, axis) + 0.03,
            "{}: {} vs {}",
            SKILL_NAMES[axis],
            mean(hard, axis),
            mean(easy, axis)
        );
        for d in demands(hard) {
            assert!(d.iter().all(|x| x.is_finite() && *x >= 0.));
            assert!(d.iter().sum::<f64>() <= 1. + 1e-12);
        }
    }
    let mut recovered = singles;
    recovered.push(Row {
        seconds: 100.,
        mask: 1,
    });
    assert_eq!(*demands(&recovered).last().unwrap(), [0.; 7]);
    let simultaneous = rows(&[1, 3, 7, 15], 0., 32);
    assert!(
        demands(&simultaneous)
            .iter()
            .flatten()
            .all(|x| x.is_finite())
    );
}
