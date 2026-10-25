// What a material is given, and helpers it may call: the prelude every
// skin's material is compiled with (`docs/plans/noteskin-scene.md`).
// Nothing here loops, so a material's cost per pixel stays bounded.

struct Material {
    // Material coordinates, in the skin's logical pixels, in the space and
    // from the origin the material declares.
    pos: vec2<f32>,
    // The part's own coordinates, 0 to 1 across its image.
    uv: vec2<f32>,
    // The part's alpha here.
    mask: f32,
    // The pixel's place on the target, 0 to 1 from its upper left.
    screen: vec2<f32>,
    // Set for each drawn part by its bindings.
    phase: f32,
    value: vec4<f32>,
    // Stable for a note, different between notes.
    seed: f32,
    // The song's beat and seconds.
    beat: f32,
    seconds: f32,
    // The material's parameters.
    params: array<vec4<f32>, 8>,
};

const PI: f32 = 3.14159265358979;
const TAU: f32 = 6.28318530717959;

// GLSL's `mod`: `x` less `y` times the floor of their quotient (WGSL's `%`
// keeps the sign of `x`).
fn fmod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

// sRGB-encoded values to linear light, and back.
fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(high, low, c <= vec3<f32>(0.0031308));
}

// Hue (in turns), saturation and value to sRGB-encoded values.
fn hsv(h: f32, s: f32, v: f32) -> vec3<f32> {
    let k = vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0);
    let p = abs(fract(vec3<f32>(h) + k) * 6.0 - vec3<f32>(3.0));
    return v * mix(vec3<f32>(1.0), clamp(p - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0)), s);
}

// Mixes two sRGB-encoded colors in Oklab, which keeps the lightness of a
// gradient even.
fn oklab_mix(a: vec3<f32>, b: vec3<f32>, t: f32) -> vec3<f32> {
    let to_lms = mat3x3<f32>(
        vec3<f32>(0.4122214708, 0.2119034982, 0.0883024619),
        vec3<f32>(0.5363325363, 0.6806995451, 0.2817188376),
        vec3<f32>(0.0514459929, 0.1073969566, 0.6299787005),
    );
    let from_lms = mat3x3<f32>(
        vec3<f32>(4.0767416621, -1.2684380046, -0.0041960863),
        vec3<f32>(-3.3077115913, 2.6097574011, -0.7034186147),
        vec3<f32>(0.2309699292, -0.3413193965, 1.7076147010),
    );
    let la = pow(max(to_lms * to_linear(a), vec3<f32>(0.0)), vec3<f32>(1.0 / 3.0));
    let lb = pow(max(to_lms * to_linear(b), vec3<f32>(0.0)), vec3<f32>(1.0 / 3.0));
    let l = mix(la, lb, t);
    return to_srgb(clamp(from_lms * (l * l * l), vec3<f32>(0.0), vec3<f32>(1.0)));
}

// A hash of a point, 0 to 1.
fn hash12(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(0.1031, 0.1030));
    q += dot(q, q.yx + vec2<f32>(33.33));
    return fract((q.x + q.y) * q.x);
}

// Smooth value noise, 0 to 1, varying over about one unit.
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (vec2<f32>(3.0) - 2.0 * f);
    let a = hash12(i);
    let b = hash12(i + vec2<f32>(1.0, 0.0));
    let c = hash12(i + vec2<f32>(0.0, 1.0));
    let d = hash12(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Four and eight octaves of noise, each twice as fine and half as strong,
// turned between octaves; 0 to 1.
fn ddi_octave(p: vec2<f32>) -> vec2<f32> {
    return mat2x2<f32>(vec2<f32>(1.6, 1.2), vec2<f32>(-1.2, 1.6)) * p;
}

fn fbm4(p: vec2<f32>) -> f32 {
    var q = p;
    var v = 0.5 * noise(q);
    q = ddi_octave(q);
    v += 0.25 * noise(q);
    q = ddi_octave(q);
    v += 0.125 * noise(q);
    q = ddi_octave(q);
    v += 0.0625 * noise(q);
    return v / 0.9375;
}

fn fbm8(p: vec2<f32>) -> f32 {
    let q = ddi_octave(ddi_octave(ddi_octave(ddi_octave(p))));
    // The last four octaves are the first four sixteen times weaker.
    return (fbm4(p) * 0.9375 + fbm4(q) * 0.05859375) / 0.99609375;
}
