// Instanced quads with procedural shapes. One draw call per batch; each
// instance carries its centre, size, rotation, color and a shape id that the
// fragment shader renders as a signed-distance field. No textures: the
// built-in notes are Infinito's shapes made light (`docs/PLAN.md` decision
// 80): its silhouettes exactly, its paint as a few waves instead of noise.

struct Globals {
    // Pixels -> clip space: x' = x * sx + tx, y' = y * sy + ty
    scale: vec2<f32>,
    translate: vec2<f32>,
    // The song's seconds.
    time: f32,
    // 1.0 on an sRGB target: instance colors arrive in linear light, so
    // the constants and factors below are converted to match.
    srgb: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct Instance {
    @location(0) center: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) rotation: f32,
    @location(3) shape: u32,
    @location(4) color: vec4<f32>,
    @location(5) params: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,          // -1..1 in quad space
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) shape: u32,
    @location(3) params: vec4<f32>,
    @location(4) aspect: vec2<f32>,      // size in pixels, for AA width
    // Offset from the centre in screen axes (unrotated), in half heights:
    // -1..1 across an unrotated quad, for screen-aligned paint.
    @location(5) screen: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    // Two triangles, counter-clockwise, covering -1..1.
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[vi];
    let cs = cos(inst.rotation);
    let sn = sin(inst.rotation);
    let local = vec2<f32>(c.x * inst.size.x * 0.5, c.y * inst.size.y * 0.5);
    let rotated = vec2<f32>(local.x * cs - local.y * sn, local.x * sn + local.y * cs);
    let px = inst.center + rotated;
    var out: VsOut;
    out.position = vec4<f32>(px * globals.scale + globals.translate, 0.0, 1.0);
    out.uv = c;
    out.color = inst.color;
    out.shape = inst.shape;
    out.params = inst.params;
    out.aspect = inst.size;
    out.screen = rotated / max(inst.size.y * 0.5, 1.0);
    return out;
}

// Shape ids (keep in sync with render::Shape and render::sprite).
const SHAPE_RECT: u32 = 0u;
const SHAPE_MINE: u32 = 3u;
const SHAPE_CIRCLE: u32 = 4u;
const SHAPE_HOLD_BODY: u32 = 5u;

// The notes and receptors: a glyph from GLYPH_BASE (its low byte), how its
// band is painted (bits 8–11) and its charge frame plus one (bits 12–14,
// 0 for none). A glyph's quad is GLYPH_QUAD lanes wide; its shapes are in
// Infinito's SVG units (`_spiral components.svg`, 64 a box, the arrow
// pointing up), GLYPH_UNITS of them in half a quad (render::GLYPH_QUAD).
const GLYPH_BASE: u32 = 32u;
const GLYPH_ARROW: u32 = 32u;
const GLYPH_CENTER: u32 = 33u;
const GLYPH_ONIGIRI: u32 = 34u;
const GLYPH_DOT: u32 = 35u;
const GLYPH_UNITS: f32 = 38.2;

// The band's paint.
// The palette's window at `params.x` cycles (Vivid, Flat).
const PAINT_PALETTE: u32 = 0u;
// `color` to `params.rgb` in waves shifted by `params.w` (Rainbow).
const PAINT_FLOW: u32 = 1u;
// `color`, shimmering (Quantized, one color, the chart's colors, a glow).
const PAINT_SOLID: u32 = 2u;
// White and mint (a lift).
const PAINT_LIFT: u32 = 3u;
// A long note's waves (its head): `params` as the body's.
const PAINT_LONG: u32 = 4u;
// Neutral (a receptor): `params.x` 1 on the beat's bright part, `params.y`
// a judgment's flash in `color`.
const PAINT_RECEPTOR: u32 = 5u;

const TAU: f32 = 6.2831853;

fn sd_box(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    return sd_box(p, b - vec2<f32>(r)) - r;
}

fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

// One sRGB-encoded component to linear light (as crates/render/src/color.rs).
fn srgb_to_linear(c: f32) -> f32 {
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

// A display-space constant (sRGB-encoded) in the space colors arrive in.
fn display(c: vec3<f32>) -> vec3<f32> {
    if (globals.srgb > 0.5) {
        return vec3<f32>(srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b));
    }
    return c;
}

// A display-space brightness factor in that space. Scaling an encoded
// color by f is close to scaling it in linear light by f^2.2 (the curve is
// close to a power law); exact for no curve, so this is approximate.
fn factor(f: f32) -> f32 {
    if (globals.srgb > 0.5) {
        return pow(f, 2.2);
    }
    return f;
}

// `src` (straight alpha) over `dst` (premultiplied).
fn over(dst: vec4<f32>, rgb: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(rgb * a + dst.rgb * (1.0 - a), a + dst.a * (1.0 - a));
}

// ---- Infinito's shapes, in its SVG units about the box's centre ----

// A 90° chevron's arm from `apex` down to `end` (x folded), `w` half its
// width: a mitered tip, a round end.
fn sd_chevron(p: vec2<f32>, apex: f32, end: vec2<f32>, w: f32) -> f32 {
    let q = vec2<f32>(abs(p.x), p.y);
    let a = vec2<f32>(0.0, apex);
    let u = normalize(end - a);
    let t = dot(q - a, u);
    if (t > length(end - a)) {
        return length(q - end) - w;
    }
    return abs(dot(q - a, vec2<f32>(u.y, -u.x))) - w;
}

// A vertical slot `w` wide each side, pointed at `apex` (45°), rounded
// at `bottom`.
fn sd_slot(p: vec2<f32>, w: f32, apex: f32, bottom: f32) -> f32 {
    if (p.y > bottom) {
        return length(p - vec2<f32>(0.0, bottom)) - w;
    }
    return max(abs(p.x) - w, (abs(p.x) + apex - p.y) * 0.70710678);
}

// What a glyph is made of: its outline, its grooves (the band is the
// outline less the grooves), the charge frame's regions in them and the
// receptor's faint fill (every charge region).
struct Glyph {
    outline: f32,
    grooves: f32,
    charge: f32,
    fill: f32,
};

fn arrow_glyph(p: vec2<f32>, frame: u32) -> Glyph {
    var g: Glyph;
    let apex = -15.626;
    let end = vec2<f32>(15.626, 0.0);
    g.outline = min(
        sd_chevron(p, apex, end, 8.75),
        sd_segment(p, vec2<f32>(0.0, apex), vec2<f32>(0.0, 15.626)) - 8.75,
    );
    g.grooves = min(sd_chevron(p, apex, end, 3.75), sd_slot(p, 3.75, -3.251, 15.626));
    let line = sd_chevron(p, apex, end, 1.914);
    let slot = sd_slot(p, 1.914, -0.655, 15.626);
    g.fill = min(line, slot);
    // Full on the hit, empty a beat before, then the slot's foot, the slot.
    switch frame {
        case 1u: { g.charge = g.fill; }
        case 3u: { g.charge = sd_slot(p, 1.914, 10.659, 15.626); }
        case 4u: { g.charge = slot; }
        default: { g.charge = 1e3; }
    }
    return g;
}

// A ring between Chebyshev (square) or Euclidean (round) radii.
fn ring(r: f32, inner: f32, outer: f32) -> f32 {
    return max(r - outer, inner - r);
}

// Pump's centre (and anything square): a rounded square band; or, `round`,
// a ring.
fn square_glyph(p: vec2<f32>, frame: u32, round: bool) -> Glyph {
    var g: Glyph;
    var r = max(abs(p.x), abs(p.y));
    if (round) {
        r = length(p);
        g.outline = r - 19.799;
        g.grooves = r - 14.799;
    } else {
        g.outline = sd_round_box(p, vec2<f32>(19.799), 8.75);
        g.grooves = sd_round_box(p, vec2<f32>(14.799), 3.75);
    }
    let outer = ring(r, 9.135, 12.963);
    g.fill = outer;
    switch frame {
        case 1u: { g.charge = outer; }
        case 3u: { g.charge = ring(r, 1.479, 5.307); }
        case 4u: { g.charge = ring(r, 5.307, 9.135); }
        default: { g.charge = 1e3; }
    }
    return g;
}

// Dancing☆Onigiri's onigiri: a rice ball's outline, its nori notched in
// from the bottom, as a band.
fn onigiri_glyph(p_in: vec2<f32>, frame: u32) -> Glyph {
    var g: Glyph;
    let p = p_in + vec2<f32>(0.0, 7.0);
    // The corners' centres (an equilateral triangle), rounded by 8.17.
    let a = vec2<f32>(0.0, -7.152);
    let b = vec2<f32>(12.976, 15.323);
    let c = vec2<f32>(-12.976, 15.323);
    let q = vec2<f32>(abs(p.x), p.y);
    let inside = (q.y - a.y) * (b.x - a.x) - q.x * (b.y - a.y) >= 0.0 && q.y <= b.y;
    var tri = min(sd_segment(q, a, b), sd_segment(q, b, c));
    if (inside) {
        tri = -tri;
    }
    let ball = tri - 8.17;
    // Open below the ball (the band runs up round it).
    let nori = sd_round_box(p - vec2<f32>(0.0, 24.0), vec2<f32>(9.87, 15.93), 1.377);
    let shape = max(ball, -nori);
    g.outline = abs(shape) - 2.5;
    // Nothing is cut out of a band this thin; the inside is the groove.
    g.grooves = shape + 2.5;
    let rice = shape + 4.5;
    g.fill = rice;
    switch frame {
        case 1u: { g.charge = rice; }
        case 3u: { g.charge = sd_box(p - vec2<f32>(0.0, 21.686), vec2<f32>(5.533, 3.093)); }
        case 4u: { g.charge = sd_box(p - vec2<f32>(0.0, 18.593), vec2<f32>(5.533, 6.186)); }
        default: { g.charge = 1e3; }
    }
    // The outline as drawn, filled inside.
    g.outline = min(g.outline, shape);
    return g;
}

// ---- Paint ----

// Infinito's palette (`_spiral components.svg#palette`), a closed loop.
fn ramp(t: f32) -> vec3<f32> {
    let palette = array<vec3<f32>, 8>(
        vec3<f32>(49.0, 213.0, 255.0) / 255.0,
        vec3<f32>(107.0, 91.0, 255.0) / 255.0,
        vec3<f32>(195.0, 61.0, 255.0) / 255.0,
        vec3<f32>(255.0, 78.0, 185.0) / 255.0,
        vec3<f32>(255.0, 136.0, 61.0) / 255.0,
        vec3<f32>(227.0, 236.0, 62.0) / 255.0,
        vec3<f32>(72.0, 239.0, 155.0) / 255.0,
        vec3<f32>(49.0, 213.0, 255.0) / 255.0
    );
    let at = fract(t) * 7.0;
    let stop = min(u32(floor(at)), 6u);
    return mix(palette[stop], palette[stop + 1u], fract(at));
}

// Infinito's flowing field (its material's domain-warped noise) as a few
// waves: 0..1 at `p` (SVG units) at the song's `t` seconds.
fn field(p_in: vec2<f32>, t_in: f32) -> f32 {
    let p = p_in * 0.0193;
    let t = t_in * 0.3;
    let drift = vec2<f32>(
        sin(p.x * 1.6 + p.y * 0.9 + t),
        sin(p.y * 1.2 - p.x * 0.7 - t * 0.8 + 2.0),
    ) * 0.34;
    let w = p + drift;
    let folds = 0.5 + 0.5 * sin(w.x * 2.0 + w.y * 1.3 + t * 0.6) * cos(w.y * 1.7 - w.x * 0.6 - t * 0.4);
    return fract((dot(w, vec2<f32>(0.18, 0.12)) + 1.25 * folds) * 2.0 - 0.269);
}

// The palette's window at `phase` across the field: as Infinito's
// material, 0.43 of the loop with its seam blended.
fn palette(q: f32, phase: f32) -> vec3<f32> {
    let a = ramp(phase + 0.43 * q);
    let b = ramp(phase + 0.43 * (q - 1.0));
    return display(mix(a, b, smoothstep(0.0, 1.0, (q - 0.5) / 0.5)));
}

// A long note's waves (Infinito's `_spiral wave.wgsl` without its noise):
// `pos` in lane units (a lane 64), `roll` purple rather than green,
// `state` 0 waiting, 1 held, 2 dropped.
fn long_paint(pos: vec2<f32>, roll: bool, state: f32) -> vec3<f32> {
    var dir = vec2<f32>(0.866, 0.5);
    var period = 121.0;
    var dark = vec3<f32>(0.169, 0.671, 0.294);
    var mid = vec3<f32>(0.333, 0.824, 0.196);
    var light = vec3<f32>(0.808, 0.906, 0.455);
    if (roll) {
        dir = vec2<f32>(0.191, 0.982);
        period = 171.0;
        dark = vec3<f32>(0.231, 0.094, 0.933);
        mid = vec3<f32>(0.443, 0.196, 1.0);
        light = vec3<f32>(0.749, 0.208, 0.984);
    }
    let t = TAU * globals.time * 0.08;
    let across = pos.x / 209.0;
    let along = pos.y / 206.0;
    let primary = sin(TAU * (across + along) + t);
    let secondary = sin(TAU * (across * 0.73 - along * 0.46) - t * 0.61);
    let warp = 31.0 * mix(primary, secondary, 0.22);
    let q = fract((dot(dir, pos) + warp) / period + 0.7 + fract(globals.time));
    var c: vec3<f32>;
    if (q <= 0.3) {
        c = mix(dark, mid, q / 0.3);
    } else if (q <= 0.5) {
        c = mix(mid, light, (q - 0.3) / 0.2);
    } else if (q <= 0.7) {
        c = mix(light, mid, (q - 0.5) / 0.2);
    } else {
        c = mix(mid, dark, (q - 0.7) / 0.3);
    }
    if (state < 0.5) {
        c = c * 0.92;
    } else if (state > 1.5) {
        // Dropped: grey and dim.
        c = vec3<f32>(dot(c, vec3<f32>(0.3, 0.59, 0.11))) * 0.55;
    }
    return display(c);
}

// The band's color at `p` (SVG units, the glyph's own axes).
fn band_paint(in: VsOut, paint: u32, p: vec2<f32>) -> vec3<f32> {
    let screen = in.screen * GLYPH_UNITS;
    switch paint {
        case PAINT_PALETTE: {
            return palette(field(screen, globals.time), in.params.x);
        }
        case PAINT_FLOW: {
            let q = field(screen, globals.time);
            return mix(in.color.rgb, in.params.rgb, 0.5 - 0.5 * cos(TAU * (q - in.params.w)));
        }
        case PAINT_LIFT: {
            let q = field(screen, globals.time);
            return display(mix(vec3<f32>(1.0), vec3<f32>(0.78, 0.98, 0.84), 0.5 - 0.5 * cos(TAU * q)));
        }
        case PAINT_LONG: {
            return long_paint(lane_pos(in), in.params.x > 0.5, in.params.y);
        }
        case PAINT_RECEPTOR: {
            // Light on the beat's first fifth, dim after, lighter down the
            // arrow (its gradients run from its tip to its tail).
            let down = clamp((p.y + 28.0) / 56.0, 0.0, 1.0);
            let bright = mix(vec3<f32>(0.945, 0.953, 0.973), vec3<f32>(0.741, 0.776, 0.835), down);
            let dim = mix(vec3<f32>(0.612, 0.639, 0.69), vec3<f32>(0.376, 0.416, 0.482), down);
            let c = display(mix(dim, bright, in.params.x));
            return mix(c, mix(in.color.rgb, vec3<f32>(1.0), 0.45), in.params.y);
        }
        default: {
            let q = field(screen, globals.time);
            return in.color.rgb * factor(mix(0.88, 1.06, 0.5 + 0.5 * cos(TAU * q)));
        }
    }
}

// Where in its lane a fragment is, in lane units (a lane 64): across from
// the lane's middle, along the screen (`params.w`: the lane's pixels).
fn lane_pos(in: VsOut) -> vec2<f32> {
    let per = 64.0 / max(in.params.w, 1.0);
    return vec2<f32>(in.screen.x * in.aspect.y * 0.5, in.position.y) * per;
}

fn glyph_color(in: VsOut) -> vec4<f32> {
    let base = in.shape & 0xffu;
    let paint = (in.shape >> 8u) & 0xfu;
    let frame = (in.shape >> 12u) & 0x7u;
    // The arrow points up in its units, left in the quad's.
    var p = in.uv * GLYPH_UNITS;
    if (base == GLYPH_ARROW) {
        p = vec2<f32>(in.uv.y, in.uv.x) * GLYPH_UNITS;
    }
    // Anti-aliasing width in SVG units (~1.2 px).
    let aa = 1.2 * GLYPH_UNITS / max(min(in.aspect.x, in.aspect.y) * 0.5, 1.0);
    var g: Glyph;
    switch base {
        case GLYPH_ARROW: { g = arrow_glyph(p, frame); }
        case GLYPH_ONIGIRI: { g = onigiri_glyph(p, frame); }
        case GLYPH_DOT: { g = square_glyph(p, frame, true); }
        default: { g = square_glyph(p, frame, false); }
    }
    let receptor = paint == PAINT_RECEPTOR;
    var out = vec4<f32>(0.0);
    // A soft shadow outside (half as dark round a receptor, and within
    // its lane: a press changes nothing beside it), or a judgment's glow.
    let falloff = clamp(1.0 - g.outline / select(8.0, 4.5, receptor), 0.0, 1.0);
    let shadow = falloff * falloff * select(0.6, 0.3, receptor);
    out = over(out, vec3<f32>(0.0), shadow);
    if (receptor && in.params.y > 0.0) {
        let glow = mix(in.color.rgb, vec3<f32>(1.0), 0.5);
        out = over(out, glow, falloff * falloff * falloff * in.params.y * 0.85);
    }
    let inside = 1.0 - smoothstep(-aa, aa, g.outline);
    let in_groove = 1.0 - smoothstep(-aa, aa, g.grooves);
    // The grooves: slate with a dark rim.
    let rim = 1.0 - smoothstep(0.6 - aa, 0.6 + aa, -g.grooves);
    out = over(out, display(vec3<f32>(0.212, 0.204, 0.282)), inside * in_groove * 0.85);
    out = over(out, display(vec3<f32>(0.078, 0.07, 0.118)), inside * in_groove * rim * 0.9);
    // The receptor's faint fill where notes charge, or the note's charge.
    if (receptor) {
        let fill = 1.0 - smoothstep(-aa, aa, g.fill);
        out = over(out, display(vec3<f32>(0.561, 0.612, 0.702)), fill * 0.24);
    } else {
        let charge = 1.0 - smoothstep(-aa, aa, g.charge);
        out = over(out, vec3<f32>(1.0), charge);
    }
    // The band.
    out = over(out, band_paint(in, paint, p), inside * (1.0 - in_groove));
    let a = out.a * in.color.a;
    if (a <= 0.002) {
        discard;
    }
    return vec4<f32>(out.rgb / max(out.a, 1e-4), a);
}

// A long note's body: as wide as the arrow, painted with the long note's
// waves, chevron ribs pointing `params.z` along it (1 to its tail, -1 to
// its head, 0 flat), its tail end (the quad's +y) pointed when bit 8 says
// it ends there. `params`: roll, state, ribs, the lane's pixels.
fn hold_body(in: VsOut) -> vec4<f32> {
    let lane = max(in.params.w, 1.0);
    let half_h = in.aspect.y * 0.5;
    // Pixels from the middle, +y to the tail.
    let px = vec2<f32>(in.uv.x * in.aspect.x * 0.5, in.uv.y * half_h);
    // As wide as the arrow, its head end rounded as its wings are (the
    // head drawn over it).
    let half_w = lane * 0.415;
    let r = lane * 0.149;
    let aa = 1.2;
    var d = abs(px.x) - half_w;
    let top = -half_h + r;
    if (px.y < top) {
        d = length(vec2<f32>(max(abs(px.x) - (half_w - r), 0.0), top - px.y)) - r;
    }
    if ((in.shape & 0x100u) != 0u) {
        // The tail: an arrow's tip, 45°.
        d = max(d, px.y - (half_h - half_w) + abs(px.x) - half_w);
    }
    // Pixels from the tail end, along.
    let roll = in.params.x > 0.5;
    let pitch = lane * select(0.195, 0.39, roll);
    let ribs = in.params.z;
    let along = px.y * select(1.0, -1.0, ribs < 0.0) + abs(px.x) * abs(ribs);
    let k = fract(along / pitch);
    let rib_w = select(0.012, 0.02, roll) * lane;
    let rib = 1.0 - smoothstep(rib_w - aa, rib_w + aa, abs(k - 0.5) * pitch);
    let inner = abs(px.x) < half_w - lane * 0.07;
    let inside = 1.0 - smoothstep(-aa, aa, d);
    let falloff = clamp(1.0 - d / (lane * 0.16), 0.0, 1.0);
    var out = over(vec4<f32>(0.0), vec3<f32>(0.0), falloff * falloff * 0.6);
    var c = long_paint(lane_pos(in), roll, in.params.y);
    if (inner) {
        c = mix(c, display(vec3<f32>(0.05, 0.04, 0.09)), rib * 0.85);
    }
    out = over(out, c, inside);
    let a = out.a * in.color.a;
    if (a <= 0.002) {
        discard;
    }
    return vec4<f32>(out.rgb / max(out.a, 1e-4), a);
}

// A mine: Infinito's eight-pointed band turning slowly round a red core,
// its outline lit blue.
fn mine(in: VsOut) -> vec4<f32> {
    let aa = 1.2 * 32.0 / max(min(in.aspect.x, in.aspect.y) * 0.5, 1.0);
    let spin = globals.time * 0.6;
    let cs = cos(spin);
    let sn = sin(spin);
    let p0 = in.uv * 32.0;
    let p = vec2<f32>(p0.x * cs - p0.y * sn, p0.x * sn + p0.y * cs);
    let q = vec2<f32>(p.x - p.y, p.x + p.y) * 0.70710678;
    // Two squares, one turned 45°: an eight-pointed star.
    let star = min(sd_box(p, vec2<f32>(16.0)), sd_box(q, vec2<f32>(16.0)));
    let core = min(sd_box(p, vec2<f32>(9.0)), sd_box(q, vec2<f32>(9.0)));
    let inside = 1.0 - smoothstep(-aa, aa, star);
    let in_core = 1.0 - smoothstep(-aa, aa, core);
    let falloff = clamp(1.0 - star / 8.0, 0.0, 1.0);
    var out = over(vec4<f32>(0.0), vec3<f32>(0.0), falloff * falloff * 0.6);
    // The band: silver to blue across, its edge lit.
    let shade = clamp(0.5 + (p.x + p.y) / 64.0, 0.0, 1.0);
    var band = display(mix(vec3<f32>(0.47, 0.6, 0.85), vec3<f32>(0.85, 0.9, 0.98), shade));
    let edge = 1.0 - smoothstep(0.0, 2.0, -star);
    let pulse = 0.6 + 0.4 * sin(globals.time * 9.0 + atan2(p.y, p.x) * 3.0);
    band = mix(band, display(vec3<f32>(0.25, 0.65, 1.0)), edge * pulse);
    out = over(out, band, inside * (1.0 - in_core));
    // The core: red, lighter in the middle, a dark rim.
    let r = length(p0) / 12.0;
    var red = display(mix(vec3<f32>(0.85, 0.2, 0.32), vec3<f32>(0.4, 0.05, 0.15), clamp(r, 0.0, 1.0)));
    let rim = 1.0 - smoothstep(0.0, 1.5, -core);
    red = mix(red, display(vec3<f32>(0.06, 0.03, 0.1)), rim);
    out = over(out, red, in_core);
    let a = out.a * in.color.a;
    if (a <= 0.002) {
        discard;
    }
    return vec4<f32>(out.rgb / max(out.a, 1e-4), a);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    if ((in.shape & 0xffu) >= GLYPH_BASE) {
        return glyph_color(in);
    }
    switch in.shape & 0xffu {
        case SHAPE_HOLD_BODY: {
            return hold_body(in);
        }
        case SHAPE_MINE: {
            return mine(in);
        }
        default: {}
    }
    let p = in.uv;
    // Anti-aliasing width in local units (~1.2 px).
    let aa = 1.2 / max(min(in.aspect.x, in.aspect.y) * 0.5, 1.0);
    var d: f32;
    switch in.shape {
        case SHAPE_RECT: {
            d = sd_round_box(p, vec2<f32>(1.0), in.params.x);
        }
        case SHAPE_CIRCLE: {
            d = length(p) - 1.0;
        }
        default: {
            d = sd_box(p, vec2<f32>(1.0));
        }
    }
    let alpha = 1.0 - smoothstep(-aa, aa, d);
    if (alpha <= 0.002) {
        discard;
    }
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
