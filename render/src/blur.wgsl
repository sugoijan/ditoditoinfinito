// The blurred extension around the background frame
// (docs/plans/video-backgrounds.md, step 5): the backdrop is drawn small,
// blurred in two passes (`fs_blur`, one direction each) and stretched over
// the screen, dimmed (`fs_composite`). Colors stay premultiplied by alpha
// throughout: the small target starts transparent.
// `fs_cached_blur` also filters static menu images once on upload.

struct Blur {
    // One texel along the blur's direction (unused by the composite).
    step: vec2<f32>,
    // The composite's brightness multiplier, applied to encoded values.
    brightness: f32,
    // 1: the target holds linear light (encode, multiply, decode).
    linear: f32,
    // The field filter's band (target pixels) and what it keeps.
    band: vec2<f32>,
    band_keep: f32,
    // Cached-image vertical pass: return straight alpha for the painter.
    unpremultiply: f32,
};

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3<f32>(0.0031308));
}

@group(0) @binding(0) var<uniform> blur: Blur;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // One triangle over the whole target; uv is top-left based (y down).
    let c = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var out: VsOut;
    out.position = vec4<f32>(c.x * 2.0 - 1.0, 1.0 - c.y * 2.0, 0.0, 1.0);
    out.uv = c;
    return out;
}

// A Gaussian of σ = 3 texels, 13 taps.
@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -6; i <= 6; i++) {
        let x = f32(i);
        let w = exp(-x * x / 18.0);
        sum += w * textureSampleLevel(source, source_sampler, in.uv + blur.step * x, 0.0);
        total += w;
    }
    return sum / total;
}

// Cached menu images: σ = 6 texels, truncated at 3σ. The horizontal pass
// premultiplies straight RGBA; the vertical one unpremultiplies
// for the painter. Filter encoded values, as the painter blends them.
@fragment
fn fs_cached_blur(in: VsOut) -> @location(0) vec4<f32> {
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -18; i <= 18; i++) {
        let x = f32(i);
        let w = exp(-x * x / 72.0);
        var c = textureSampleLevel(source, source_sampler, in.uv + blur.step * x, 0.0);
        if (blur.unpremultiply < 0.5) {
            c = vec4<f32>(c.rgb * c.a, c.a);
        }
        sum += w * c;
        total += w;
    }
    let c = sum / total;
    if (blur.unpremultiply > 0.5 && c.a > 0.0) {
        return vec4<f32>(c.rgb / c.a, c.a);
    }
    return c;
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(source, source_sampler, in.uv, 0.0);
    var b = blur.brightness;
    if (in.position.x >= blur.band.x && in.position.x < blur.band.y) {
        b = b * blur.band_keep;
    }
    if (blur.linear > 0.5 && c.a > 0.0) {
        // Premultiplied: the color itself is dimmed, as encoded.
        let rgb = srgb_to_linear(linear_to_srgb(c.rgb / c.a) * b);
        return vec4<f32>(rgb * c.a, c.a);
    }
    return vec4<f32>(c.rgb * b, c.a);
}
