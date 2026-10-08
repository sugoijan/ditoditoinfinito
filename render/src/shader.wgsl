// Instanced quads with procedural shapes. One draw call per batch; each
// instance carries its centre, size, rotation, colour and a shape id that the
// fragment shader renders as a signed-distance field (arrow, mine, bar,
// rectangle). No textures needed for the MVP skin.

struct Globals {
    // Pixels -> clip space: x' = x * sx + tx, y' = y * sy + ty
    scale: vec2<f32>,
    translate: vec2<f32>,
    time: f32,
    // 1.0 on an sRGB target: instance colours arrive in linear light, so
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
    // -1..1 across an unrotated quad, for screen-vertical gradients.
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

// Shape ids (keep in sync with render::Shape).
const SHAPE_RECT: u32 = 0u;
const SHAPE_ARROW: u32 = 1u;
const SHAPE_ARROW_OUTLINE: u32 = 2u;
const SHAPE_MINE: u32 = 3u;
const SHAPE_CIRCLE: u32 = 4u;
const SHAPE_HOLD_BODY: u32 = 5u;
const SHAPE_ONIGIRI: u32 = 6u;
// An arrow whose colour runs from `color` to `params.rgb` and back down the
// screen, shifted by `params.w` cycles (a flowing gradient).
const SHAPE_ARROW_GRADIENT: u32 = 7u;
// An arrow whose colour runs from `color` at its tail to `params.rgb` at its
// tip, along the arrow (it points to -x in quad space).
const SHAPE_ARROW_RAMP: u32 = 8u;

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

// DDR-style arrow pointing LEFT in local space (-1..1): a chevron head plus a
// thick shaft. Rotation is applied by the vertex shader.
fn sd_arrow(p: vec2<f32>) -> f32 {
    let w = 0.22;                              // half stroke width
    let tip = vec2<f32>(-0.78, 0.0);
    let d_head_a = sd_segment(p, tip, vec2<f32>(-0.05, 0.73)) - w;
    let d_head_b = sd_segment(p, tip, vec2<f32>(-0.05, -0.73)) - w;
    let d_shaft = sd_segment(p, vec2<f32>(-0.62, 0.0), vec2<f32>(0.72, 0.0)) - w;
    return min(min(d_head_a, d_head_b), d_shaft);
}

fn sd_onigiri(p: vec2<f32>) -> f32 {
    // Rounded equilateral triangle (iq), apex up. The SDF is written for
    // y-up; quad space is y-down, so flip y to keep the apex at the top.
    let k = sqrt(3.0);
    var q = vec2<f32>(abs(p.x) - 0.75, -p.y + 0.75 / k);
    if (q.x + k * q.y > 0.0) {
        q = vec2<f32>(q.x - k * q.y, -k * q.x - q.y) / 2.0;
    }
    q.x = q.x - clamp(q.x, -1.5, 0.0);
    return -length(q) * sign(q.y) - 0.18;
}

// One sRGB-encoded component to linear light (as render/src/color.rs).
fn srgb_to_linear(c: f32) -> f32 {
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

// A display-space constant (sRGB-encoded) in the space colours arrive in.
fn display(c: vec3<f32>) -> vec3<f32> {
    if (globals.srgb > 0.5) {
        return vec3<f32>(srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b));
    }
    return c;
}

// A display-space brightness factor in that space. Scaling an encoded
// colour by f is close to scaling it in linear light by f^2.2 (the curve is
// close to a power law); exact for no curve, so this is approximate.
fn factor(f: f32) -> f32 {
    if (globals.srgb > 0.5) {
        return pow(f, 2.2);
    }
    return f;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let p = in.uv;
    // Anti-aliasing width in local units (~1.2 px).
    let aa = 1.2 / max(min(in.aspect.x, in.aspect.y) * 0.5, 1.0);
    var d: f32 = 1.0;
    var color = in.color;
    switch in.shape {
        case SHAPE_RECT: {
            d = sd_round_box(p, vec2<f32>(1.0), in.params.x);
        }
        case SHAPE_ARROW: {
            d = sd_arrow(p);
            // Darker outline ring.
            let edge = smoothstep(-0.16 - aa, -0.16, d);
            color = vec4<f32>(mix(color.rgb, color.rgb * factor(0.25), edge), color.a);
        }
        case SHAPE_ARROW_GRADIENT: {
            d = sd_arrow(p);
            let wave = 0.5 + 0.5 * cos(6.2831853 * (in.screen.y * 0.5 + in.params.w));
            let body = mix(color.rgb, in.params.rgb, wave);
            let edge = smoothstep(-0.16 - aa, -0.16, d);
            color = vec4<f32>(mix(body, body * factor(0.25), edge), color.a);
        }
        case SHAPE_ARROW_RAMP: {
            d = sd_arrow(p);
            let body = mix(in.params.rgb, color.rgb, clamp((p.x + 1.0) * 0.5, 0.0, 1.0));
            let edge = smoothstep(-0.16 - aa, -0.16, d);
            color = vec4<f32>(mix(body, body * factor(0.25), edge), color.a);
        }
        case SHAPE_ARROW_OUTLINE: {
            let inner = sd_arrow(p);
            d = abs(inner + 0.09) - 0.09;      // ring of width ~0.18
        }
        case SHAPE_MINE: {
            let r = length(p);
            d = r - 0.62;
            // spikes: 8 bumps
            let ang = atan2(p.y, p.x);
            let spike = 0.12 * max(0.0, cos(ang * 8.0 + globals.time * 6.0));
            d = d - spike;
            let core = smoothstep(0.30, 0.34, r);
            color = vec4<f32>(mix(display(vec3<f32>(1.0, 0.9, 0.8)), color.rgb, core), color.a);
        }
        case SHAPE_CIRCLE: {
            d = length(p) - 1.0;
        }
        case SHAPE_HOLD_BODY: {
            // Body: full-height bar, rounded ends; params.x = corner radius (local units)
            d = sd_round_box(p, vec2<f32>(1.0), in.params.x);
        }
        case SHAPE_ONIGIRI: {
            d = sd_onigiri(p);
            let edge = smoothstep(-0.16 - aa, -0.16, d);
            color = vec4<f32>(mix(color.rgb, color.rgb * factor(0.3), edge), color.a);
        }
        default: {
            d = sd_box(p, vec2<f32>(1.0));
        }
    }
    let alpha = 1.0 - smoothstep(-aa, aa, d);
    if (alpha <= 0.002) {
        discard;
    }
    return vec4<f32>(color.rgb, color.a * alpha);
}
