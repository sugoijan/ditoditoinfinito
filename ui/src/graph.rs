//! Graph primitives for results (`docs/plans/ui-core.md`, step 4 and
//! 6.1): lines (thin quads turned along each segment), bars, points, a
//! histogram, a line over time. Each returns a frame of actors in the
//! space of a rectangle `[x, y, w, h]` (y down), colored from the look
//! (`Graph.Line`, `Graph.Bar`, `Graph.Point`, `Graph.Axis`) or as given.

use ddi_actor::{Actor, Kind, Rgba};

use crate::look::{self, Look, call, named, quad};

/// A segment from `a` to `b`, `width` thick.
pub fn segment(a: [f32; 2], b: [f32; 2], width: f32, color: Rgba) -> Actor {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy).max(0.001);
    let mut q = quad("", [len, width], color);
    call(&mut q, "xy", &[(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
    call(&mut q, "rotationz", &[dy.atan2(dx).to_degrees()]);
    q
}

/// A polyline through `points`.
pub fn polyline(name: &str, points: &[[f32; 2]], width: f32, color: Rgba) -> Actor {
    let segments = points
        .windows(2)
        .map(|w| segment(w[0], w[1], width, color))
        .collect();
    named(Kind::Frame(segments), name)
}

/// Bars for `values` (each `0..=max`) filling `rect`, from its bottom.
pub fn bars(name: &str, values: &[f32], max: f32, rect: [f32; 4], gap: f32, color: Rgba) -> Actor {
    let [x, y, w, h] = rect;
    let n = values.len().max(1) as f32;
    let slot = w / n;
    let bw = (slot - gap).max(1.0);
    let max = if max > 0.0 { max } else { 1.0 };
    let quads = values
        .iter()
        .enumerate()
        .filter(|(_, v)| **v > 0.0)
        .map(|(i, v)| {
            let bh = (v / max).min(1.0) * h;
            let mut q = quad("", [bw, bh], color);
            call(
                &mut q,
                "xy",
                &[x + slot * (i as f32 + 0.5), y + h - bh / 2.0],
            );
            q
        })
        .collect();
    named(Kind::Frame(quads), name)
}

/// A dot at each point.
pub fn points(name: &str, points: &[[f32; 2]], size: f32, color: Rgba) -> Actor {
    let quads = points
        .iter()
        .map(|p| {
            let mut q = quad("", [size, size], color);
            call(&mut q, "xy", &[p[0], p[1]]);
            q
        })
        .collect();
    named(Kind::Frame(quads), name)
}

/// Counts of `samples` in `bins` equal bins over `range` (out of range
/// left out).
pub fn histogram(samples: &[f32], range: [f32; 2], bins: usize) -> Vec<u32> {
    let mut out = vec![0; bins];
    let span = range[1] - range[0];
    if bins == 0 || span <= 0.0 {
        return out;
    }
    for &s in samples {
        if !(s >= range[0] && s <= range[1]) {
            continue;
        }
        let i = (((s - range[0]) / span) * bins as f32).floor() as usize;
        out[i.min(bins - 1)] += 1;
    }
    out
}

/// A histogram drawn as bars in `rect`, with a centre line (the zero of
/// a timing histogram) when `range` spans it.
pub fn histogram_actor(
    name: &str,
    look: &dyn Look,
    samples: &[f32],
    range: [f32; 2],
    bins: usize,
    rect: [f32; 4],
) -> Actor {
    let counts = histogram(samples, range, bins);
    let max = counts.iter().copied().max().unwrap_or(0) as f32;
    let values: Vec<f32> = counts.iter().map(|&c| c as f32).collect();
    let mut children = vec![bars(
        &format!("{name}.Bars"),
        &values,
        max,
        rect,
        1.0,
        look::color(look, "Graph.Bar"),
    )];
    if range[0] < 0.0 && range[1] > 0.0 {
        let [x, y, w, h] = rect;
        let zx = x + w * (-range[0] / (range[1] - range[0]));
        children.insert(
            0,
            segment([zx, y], [zx, y + h], 1.0, look::color(look, "Graph.Axis")),
        );
    }
    named(Kind::Frame(children), name)
}

/// A line over time: `(t, v)` samples mapped from `t_range` × `v_range`
/// onto `rect` (v up), with a baseline.
pub fn line_graph(
    name: &str,
    look: &dyn Look,
    series: &[(f32, f32)],
    t_range: [f32; 2],
    v_range: [f32; 2],
    rect: [f32; 4],
) -> Actor {
    let [x, y, w, h] = rect;
    let ts = (t_range[1] - t_range[0]).max(f32::EPSILON);
    let vs = (v_range[1] - v_range[0]).max(f32::EPSILON);
    let pts: Vec<[f32; 2]> = series
        .iter()
        .map(|&(t, v)| {
            [
                x + ((t - t_range[0]) / ts).clamp(0.0, 1.0) * w,
                y + h - ((v - v_range[0]) / vs).clamp(0.0, 1.0) * h,
            ]
        })
        .collect();
    let pts = decimate(&pts, w);
    let axis = segment(
        [x, y + h],
        [x + w, y + h],
        1.0,
        look::color(look, "Graph.Axis"),
    );
    let line = polyline(
        &format!("{name}.Line"),
        &pts,
        2.0,
        look::color(look, "Graph.Line"),
    );
    named(Kind::Frame(vec![axis, line]), name)
}

/// At most two points a pixel of width: per pixel column the lowest and
/// the highest, in their order (a long song's life in a small panel
/// keeps its dips and draws a few hundred segments, not thousands).
fn decimate(pts: &[[f32; 2]], width: f32) -> Vec<[f32; 2]> {
    let cap = (width.max(1.0) * 2.0) as usize;
    if pts.len() <= cap {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(cap + 2);
    let mut i = 0;
    while i < pts.len() {
        let col = pts[i][0].floor();
        let mut j = i;
        let (mut lo, mut hi) = (i, i);
        while j < pts.len() && pts[j][0].floor() == col {
            if pts[j][1] < pts[lo][1] {
                lo = j;
            }
            if pts[j][1] > pts[hi][1] {
                hi = j;
            }
            j += 1;
        }
        out.push(pts[lo.min(hi)]);
        if lo != hi {
            out.push(pts[lo.max(hi)]);
        }
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::Plain;

    #[test]
    fn long_series_are_decimated_to_the_width() {
        // 10,000 samples into 100 pixels: at most 200 points, the dip kept.
        let series: Vec<(f32, f32)> = (0..10_000)
            .map(|i| (i as f32, if i == 5_000 { 0.0 } else { 1.0 }))
            .collect();
        let g = line_graph(
            "g",
            &Plain,
            &series,
            [0.0, 10_000.0],
            [0.0, 1.0],
            [0.0, 0.0, 100.0, 50.0],
        );
        let segs = g.children()[1].children().len();
        assert!(segs < 200, "{segs}");
        // Out-of-range times are kept at the edges.
        let g = line_graph(
            "g",
            &Plain,
            &[(-5.0, 0.5), (50.0, 0.5)],
            [0.0, 10.0],
            [0.0, 1.0],
            [0.0, 0.0, 100.0, 50.0],
        );
        let seg = &g.children()[1].children()[0];
        assert!((seg.size()[0] - 100.0).abs() < 1e-3);
        assert!(seg.current().pos[1] > 0.0);
        let lowest = |g: &Actor| {
            g.children()[1]
                .children()
                .iter()
                .map(|s| s.current().pos[1])
                .fold(0.0f32, f32::max)
        };
        let g = line_graph(
            "g",
            &Plain,
            &series,
            [0.0, 10_000.0],
            [0.0, 1.0],
            [0.0, 0.0, 100.0, 50.0],
        );
        assert!(lowest(&g) > 20.0, "the dip is drawn");
    }

    #[test]
    fn histograms_count_into_bins() {
        let h = histogram(
            &[-10.0, -0.1, 0.0, 0.1, 9.9, 10.0, 11.0, f32::NAN],
            [-10.0, 10.0],
            4,
        );
        assert_eq!(h, vec![1, 1, 2, 2]);
        assert_eq!(histogram(&[1.0], [0.0, 0.0], 3), vec![0, 0, 0]);
    }

    #[test]
    fn segments_lie_along_their_ends() {
        let s = segment([0.0, 0.0], [30.0, 40.0], 2.0, [1.0; 4]);
        assert_eq!(s.size(), [50.0, 2.0]);
        let st = s.current();
        assert_eq!([st.pos[0], st.pos[1]], [15.0, 20.0]);
        assert!((st.rotation[2] - 53.130_1).abs() < 1e-3);
        let g = line_graph(
            "g",
            &Plain,
            &[(0.0, 0.0), (1.0, 1.0), (2.0, 0.5)],
            [0.0, 2.0],
            [0.0, 1.0],
            [0.0, 0.0, 100.0, 50.0],
        );
        // The axis and a line of two segments.
        assert_eq!(g.children().len(), 2);
        assert_eq!(g.children()[1].children().len(), 2);
        let b = bars(
            "b",
            &[1.0, 0.0, 2.0],
            2.0,
            [0.0, 0.0, 30.0, 10.0],
            0.0,
            [1.0; 4],
        );
        assert_eq!(b.children().len(), 2);
        assert_eq!(b.children()[1].size(), [10.0, 10.0]);
    }
}
