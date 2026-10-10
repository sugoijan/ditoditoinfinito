// A background image or movie frame, cover-fit to the target, dimmed and
// faded. Images are RGBA textures (`fs_main`); movie frames are three
// planes, Y, U and V, converted here (`fs_yuv`), as
// `ddi_platform::video::yuv_to_rgb` does.

struct Params {
    // Texture coordinates: uv = uv_offset + screen01 * uv_scale.
    uv_scale: vec2<f32>,
    uv_offset: vec2<f32>,
    // Brightness multiplier, in the target's colour space.
    brightness: f32,
    // Opacity, for crossfades.
    alpha: f32,
    // Movie frames: the matrix's luma weights of red and blue, full range
    // (1) or limited (0), and whether the target wants linear light (1).
    kr: f32,
    kb: f32,
    full_range: f32,
    linear_out: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> params: Params;
// The image, or a movie frame's Y plane.
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var image_sampler: sampler;
@group(0) @binding(3) var plane_u: texture_2d<f32>;
@group(0) @binding(4) var plane_v: texture_2d<f32>;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[vi];
    var out: VsOut;
    // c is top-left based (y down); clip space is y up.
    out.position = vec4<f32>(c.x * 2.0 - 1.0, 1.0 - c.y * 2.0, 0.0, 1.0);
    out.uv = params.uv_offset + c * params.uv_scale;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(image, image_sampler, in.uv);
    return vec4<f32>(c.rgb * params.brightness, params.alpha);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_yuv(in: VsOut) -> @location(0) vec4<f32> {
    // Samples are the 8-bit codes / 255.
    let y = textureSample(image, image_sampler, in.uv).r * 255.0;
    let u = textureSample(plane_u, image_sampler, in.uv).r * 255.0 - 128.0;
    let v = textureSample(plane_v, image_sampler, in.uv).r * 255.0 - 128.0;
    var luma: f32;
    var cb: f32;
    var cr: f32;
    if (params.full_range > 0.5) {
        luma = y / 255.0;
        cb = u / 255.0;
        cr = v / 255.0;
    } else {
        luma = (y - 16.0) / 219.0;
        cb = u / 224.0;
        cr = v / 224.0;
    }
    let r = luma + 2.0 * (1.0 - params.kr) * cr;
    let b = luma + 2.0 * (1.0 - params.kb) * cb;
    let g = (luma - params.kr * r - params.kb * b) / (1.0 - params.kr - params.kb);
    var rgb = clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
    // Video carries gamma-encoded RGB; an sRGB target blends in linear
    // light, like image textures sampled from sRGB.
    if (params.linear_out > 0.5) {
        rgb = srgb_to_linear(rgb);
    }
    return vec4<f32>(rgb * params.brightness, params.alpha);
}
