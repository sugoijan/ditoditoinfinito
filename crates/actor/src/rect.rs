//! Rectangles as `[left, top, right, bottom]`.

/// The smallest rectangle around both.
pub fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// What both cover; inside out where they do not meet.
pub fn intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

/// The smallest rectangle around `points`; inside out, at infinity, around
/// none.
pub fn around(points: impl IntoIterator<Item = [f32; 2]>) -> [f32; 4] {
    let mut r = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for p in points {
        r = [
            r[0].min(p[0]),
            r[1].min(p[1]),
            r[2].max(p[0]),
            r[3].max(p[1]),
        ];
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_combine() {
        let (a, b) = ([0.0, 1.0, 4.0, 5.0], [2.0, -1.0, 6.0, 3.0]);
        assert_eq!(union(a, b), [0.0, -1.0, 6.0, 5.0]);
        assert_eq!(intersect(a, b), [2.0, 1.0, 4.0, 3.0]);
        assert_eq!(around([[1.0, 4.0], [3.0, 2.0]]), [1.0, 2.0, 3.0, 4.0]);
    }
}
