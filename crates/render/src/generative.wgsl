// Generative backgrounds (`generative.rs`, `docs/plans/phase-8-gaps.md`,
// step 5), drawn into a plain texture of encoded values, as the design
// study (`tools/background-design/visuals.js`) drew them on its canvas:
// a look over the whole texture (`fs_look`: the ground, the field, the
// kaleidoscope, the song's picture; the bars over them), then strokes
// (`vs_stroke`: the field's lines, the rain's outlines), blended in
// encoded values.

struct Look {
    // Width, height, units (pixels of a 540-pixel screen), light (how
    // bright the look is drawn).
    size: vec4<f32>,
    // Look (0 bars, 1 kaleidoscope, 2 field, 3 rain), picture (1: there),
    // the picture's samples are linear light (1), the picture's brightness.
    flags: vec4<f32>,
    // Ground, low, middle, high, encoded.
    c0: vec4<f32>,
    c1: vec4<f32>,
    c2: vec4<f32>,
    c3: vec4<f32>,
    // The kaleidoscope's tint (rgb), and the beat's pulse (a).
    tint: vec4<f32>,
    // The picture covering the texture: uv scale (xy) and offset (zw).
    fit: vec4<f32>,
    // The kaleidoscope: the piece's turn (radians), its size, its middle.
    kaleido: vec4<f32>,
    // Wedges, the rosette's radius (of the height), the picture's aspect,
    // the brightness kept (1 − darken).
    kaleido2: vec4<f32>,
    // Desaturation, tint amount, 0, 0.
    kaleido3: vec4<f32>,
    // The field: scale, contrast, brightness, 0.
    field: vec4<f32>,
    // The bars: height (of the screen's), gap (of a bar), segments,
    // segment gap (of a pitch).
    bar_shape: vec4<f32>,
    // Opacity, count, 0, 0.
    bar_more: vec4<f32>,
    bars: array<vec4<f32>, 6>,
    peaks: array<vec4<f32>, 6>,
    bar_colors: array<vec4<f32>, 24>,
};

@group(0) @binding(0) var<uniform> u: Look;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32(vi & 1u), f32(vi >> 1u)) * 2.0;
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), s.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), s.x),
        s.y,
    );
}

fn fbm(start: vec2<f32>) -> f32 {
    var p = start;
    var v = 0.0;
    var a = 0.5;
    for (var i = 0; i < 5; i++) {
        v += a * noise(p);
        p = p * 2.03 + vec2<f32>(17.0, 9.0);
        a *= 0.5;
    }
    return v;
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3<f32>(0.0031308));
}

// GLSL's `mod`: the remainder toward minus infinity.
fn modulo(a: f32, n: f32) -> f32 {
    return a - n * floor(a / n);
}

fn modulo2(a: vec2<f32>, n: f32) -> vec2<f32> {
    return a - n * floor(a / n);
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// The song's picture at `uv` (its own 0..1), encoded.
fn picture_at(uv: vec2<f32>) -> vec3<f32> {
    let c = textureSampleLevel(picture, picture_sampler, uv, 0.0).rgb;
    return select(c, linear_to_srgb(c), u.flags.z > 0.5);
}

// The field: a domain-warped field in the palette, held still.
fn field(uv: vec2<f32>) -> vec3<f32> {
    let aspect = u.size.x / u.size.y;
    let p = (uv - 0.5) * vec2<f32>(aspect, 1.0) * u.field.x;
    let q = vec2<f32>(fbm(p), fbm(p + vec2<f32>(5.2, 1.3)));
    var f = fbm(p + 2.2 * q);
    f = clamp(0.5 + (f - 0.5) * (0.6 + u.field.y * 2.4), 0.0, 1.0);
    var c = select(
        mix(u.c1.rgb, u.c2.rgb, (f - 0.5) * 2.0),
        mix(u.c0.rgb, u.c1.rgb, f * 2.0),
        f < 0.5,
    );
    c = mix(c, u.c3.rgb, smoothstep(0.82, 1.0, f) * 0.25);
    return mix(u.c0.rgb, c, clamp(u.field.z * 1.6, 0.0, 1.0));
}

// Pieces of glass in the palette, for a song with no picture.
fn glass(p: vec2<f32>) -> vec3<f32> {
    let g = p * 14.0;
    let i = floor(g);
    let f = fract(g);
    var best = 9.0;
    var second = 9.0;
    var id = vec2<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let o = vec2<f32>(f32(x), f32(y));
            let d = length(f - o - vec2<f32>(hash(i + o), hash(i + o + 19.7)));
            if (d < best) {
                second = best;
                best = d;
                id = i + o;
            } else if (d < second) {
                second = d;
            }
        }
    }
    let h = hash(id + 3.1);
    var c = u.c3.rgb;
    if (h < 0.45) {
        c = mix(u.c1.rgb, u.c2.rgb, 0.5);
    } else if (h < 0.85) {
        c = u.c2.rgb;
    }
    c *= 0.7 + 0.6 * hash(id + 7.3);
    return c * mix(0.2, 1.0, smoothstep(0.0, 0.08, second - best));
}

// The kaleidoscope: a rosette of two mirrors, mirrored outward in rings,
// over a piece of the picture (or of glass) that turns and wanders.
fn kaleidoscope(uv: vec2<f32>) -> vec3<f32> {
    let aspect = u.size.x / u.size.y;
    let p = (uv - 0.5) * vec2<f32>(aspect, 1.0) / u.kaleido2.y;
    let wedge = 6.2831853 / u.kaleido2.x;
    var a = modulo(atan2(p.y, p.x), wedge);
    a = min(a, wedge - a);
    let r = 1.0 - abs(modulo(length(p), 2.0) - 1.0);
    let local = vec2<f32>(cos(a), sin(a)) * r
        - 0.6 * vec2<f32>(cos(wedge * 0.25), sin(wedge * 0.25));
    let cs = cos(u.kaleido.x);
    let sn = sin(u.kaleido.x);
    let turned = vec2<f32>(cs * local.x - sn * local.y, sn * local.x + cs * local.y);
    let s = u.kaleido.zw + turned * u.kaleido.y;
    var c: vec3<f32>;
    if (u.flags.y > 0.5) {
        // Mirrored past its edges.
        let m = abs(modulo2(vec2<f32>(s.x / u.kaleido2.z, s.y), 2.0) - 1.0);
        c = picture_at(vec2<f32>(1.0) - m);
    } else {
        c = glass(s);
    }
    let l = luma(c);
    c = mix(c, vec3<f32>(l), u.kaleido3.x);
    c = mix(c, u.tint.rgb * (0.3 + l), u.kaleido3.y * 0.6);
    c *= u.kaleido2.w * (1.0 + u.tint.a);
    return mix(u.c0.rgb, c, 0.97);
}

// How much of a pixel at distance `d` inside an edge is covered.
fn cover(d: f32) -> f32 {
    return clamp(d + 0.5, 0.0, 1.0);
}

// The bars over `under`, at pixel `p`.
fn bars(p: vec2<f32>, under: vec3<f32>) -> vec3<f32> {
    let w = u.size.x;
    let h = u.size.y;
    let n = u.bar_more.y;
    let bw = w / n;
    let i = clamp(floor(p.x / bw), 0.0, n - 1.0);
    let k = u32(i);
    let v = u.bars[k / 4u][k % 4u];
    let peak = u.peaks[k / 4u][k % 4u];
    let across = cover(bw * (1.0 - u.bar_shape.y) / 2.0 - abs(p.x - (i + 0.5) * bw));
    let cells = u.bar_shape.z;
    let pitch = h * u.bar_shape.x / cells;
    let tall = pitch * (1.0 - u.bar_shape.w);
    let g = (pitch - tall) / 2.0;
    let up = h - p.y;
    let j = floor(up / pitch);
    let within = up - j * pitch;
    let along = cover(min(within - g, g + tall - within));
    let lit = min(cells, floor(v * cells + 0.35));
    let top = min(cells, floor(peak * cells + 0.35)) - 1.0;
    let on = j >= 0.0 && (j < lit || (j == top && top >= lit));
    let a = select(0.0, u.bar_more.x * across * along, on);
    return mix(under, u.bar_colors[k].rgb, a);
}

@fragment
fn fs_look(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let p = position.xy;
    let uv = p / u.size.xy;
    let look = u32(u.flags.x + 0.5);
    let light = u.size.w;
    var c: vec3<f32>;
    if (look == 1u) {
        c = kaleidoscope(uv) * light;
    } else if (u.flags.y > 0.5) {
        c = picture_at(u.fit.zw + uv * u.fit.xy) * u.flags.w;
    } else if (look == 2u) {
        c = field(uv) * light;
    } else {
        c = u.c0.rgb * light;
    }
    if (look == 0u) {
        c = bars(p, c);
    }
    return vec4<f32>(c, 1.0);
}

// Strokes: the field's lines and the rain's outlines, a strip each.
struct Strokes {
    // Width, height, then the lines' count and waves across.
    size: vec4<f32>,
    // The lines' phase (radians run), amplitude (pixels), 0, 0.
    lines: vec4<f32>,
    // The bands the lines follow.
    bands: array<vec4<f32>, 8>,
    // The arrow's and the onigiri's outlines, 120 points each: x, y and
    // the normal, in the 64-unit square.
    shapes: array<vec4<f32>, 240>,
};

// Its own binding: the look's entry points never see it.
@group(0) @binding(3) var<uniform> strokes: Strokes;

struct Stroke {
    // Where (pixels), pixels a unit (an outline's 64th), turn (radians).
    @location(0) place: vec4<f32>,
    @location(1) color: vec4<f32>,
    // Width (pixels), the waves' amplitude (units), phase and count.
    @location(2) line: vec4<f32>,
    // Kind (0 the arrow, 1 the onigiri, 2 a line), the line's index.
    @location(3) kind: vec4<f32>,
};

struct StrokeOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    // Pixels across the strip from its middle, and half its width.
    @location(1) across: vec2<f32>,
};

const POINTS: u32 = 120u;
const LINE_STEPS: f32 = 160.0;

fn band(b: f32) -> f32 {
    let k = u32(min(b, 31.0));
    return strokes.bands[k / 4u][k % 4u];
}

// Line `l` of `n`'s offset at `x` (0..1 across), pixels.
fn line_offset(x: f32, l: f32) -> f32 {
    let n = strokes.size.z;
    let per = 32.0 / n;
    var d = 0.0;
    for (var k = 0; k < 4; k++) {
        let kf = f32(k);
        let b = band(floor(l * per + kf * per / 4.0));
        d += b * sin(6.2831853 * (x * strokes.size.w * (kf + 1.0))
            + strokes.lines.x * (kf + 1.0) + l * 1.7 + kf) / (kf + 1.0);
    }
    return d * 0.7 * strokes.lines.y * sin(3.14159265 * x);
}

@vertex
fn vs_stroke(@builtin(vertex_index) vi: u32, s: Stroke) -> StrokeOut {
    let j = vi / 2u;
    let side = f32(vi % 2u) * 2.0 - 1.0;
    var p: vec2<f32>;
    var normal: vec2<f32>;
    if (s.kind.x > 1.5) {
        let x = f32(j) / LINE_STEPS;
        let e = 0.5 / LINE_STEPS;
        let w = strokes.size.x;
        let here = vec2<f32>(x * w, s.place.y + line_offset(x, s.kind.y));
        let ahead = vec2<f32>((x + e) * w, s.place.y + line_offset(x + e, s.kind.y));
        let behind = vec2<f32>((x - e) * w, s.place.y + line_offset(x - e, s.kind.y));
        let t = normalize(ahead - behind);
        p = here;
        normal = vec2<f32>(-t.y, t.x);
    } else {
        let k = j % POINTS;
        let q = strokes.shapes[u32(s.kind.x + 0.5) * POINTS + k];
        let d = s.line.y * sin(6.2831853 * s.line.w * f32(k) / f32(POINTS) + s.line.z);
        var local = q.xy + q.zw * d - vec2<f32>(32.0);
        if (s.kind.x > 0.5) {
            local.y -= 7.0;
        }
        let cs = cos(s.place.w);
        let sn = sin(s.place.w);
        let turn = mat2x2<f32>(vec2<f32>(cs, sn), vec2<f32>(-sn, cs));
        p = s.place.xy + turn * local * s.place.z;
        normal = turn * q.zw;
    }
    let half = s.line.x / 2.0;
    let reach = half + 1.0;
    let edge = p + normal * side * reach;
    var out: StrokeOut;
    out.position = vec4<f32>(
        edge.x / strokes.size.x * 2.0 - 1.0,
        1.0 - edge.y / strokes.size.y * 2.0,
        0.0,
        1.0,
    );
    out.color = s.color;
    out.across = vec2<f32>(side * reach, half);
    return out;
}

@fragment
fn fs_stroke(in: StrokeOut) -> @location(0) vec4<f32> {
    let a = in.color.a * clamp(in.across.y + 0.5 - abs(in.across.x), 0.0, 1.0);
    return vec4<f32>(in.color.rgb, a);
}
