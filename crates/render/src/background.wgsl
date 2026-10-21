// A background image or movie frame placed as a StepMania background
// effect places it (a rectangle of the viewport, with its texture
// coordinates), tinted, dimmed, cropped and faded (and, an image, blended by
// its own alpha). Colors come out premultiplied, so one blend state both
// blends and adds (alpha 0). Images are RGBA textures (`fs_main`); movie frames are three
// planes, Y, U and V, converted here (`fs_yuv`), as
// `ddi_platform::video::yuv_to_rgb` does.

struct Params {
    // Where the quad goes: x, y, width, height in the viewport's 0..1.
    rect: vec4<f32>,
    // Texture coordinates: uv = uv_offset + quad01 * uv_scale.
    uv_scale: vec2<f32>,
    uv_offset: vec2<f32>,
    // The sprite's own 0..1 from texture coordinates: (uv - xy) / zw.
    local: vec4<f32>,
    // Cropped parts of the sprite and the soft edges inside them: left,
    // top, right, bottom.
    crop: vec4<f32>,
    fade: vec4<f32>,
    // Multiplies the encoded colors (rgb).
    tint: vec4<f32>,
    // Brightness multiplier, applied to the encoded values (on a linear
    // target too: encoded, multiplied, decoded).
    brightness: f32,
    // Opacity, for crossfades.
    alpha: f32,
    // Movie frames: the matrix's luma weights of red and blue, full range
    // (1) or limited (0), and whether the target wants linear light (1).
    kr: f32,
    kb: f32,
    full_range: f32,
    linear_out: f32,
    // The field filter's band: left and right edges (target pixels) and
    // the brightness kept inside it, applied as the brightness is.
    band: vec2<f32>,
    band_keep: f32,
    source_linear: f32,
    // 1: added to what is under it.
    add: f32,
    _pad: f32,
};

// How much of the sprite shows at texture coordinates `uv`: nothing in its
// crops, fading in over their soft edges (`Sprite::DrawPrimitives`).
fn crop_alpha(uv: vec2<f32>) -> f32 {
    let l = (uv - params.local.xy) / params.local.zw;
    let c = params.crop;
    if (l.x < c.x || l.x > 1.0 - c.z || l.y < c.y || l.y > 1.0 - c.w) {
        return 0.0;
    }
    let f = params.fade;
    var a = 1.0;
    if (f.x > 0.0) { a = min(a, (l.x - c.x) / f.x); }
    if (f.y > 0.0) { a = min(a, (l.y - c.y) / f.y); }
    if (f.z > 0.0) { a = min(a, (1.0 - c.z - l.x) / f.z); }
    if (f.w > 0.0) { a = min(a, (1.0 - c.w - l.y) / f.w); }
    return clamp(a, 0.0, 1.0);
}

// `rgb` (in the target's light) at opacity `a`, premultiplied; added
// leaves the alpha under it as it is.
fn out_color(rgb: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(rgb * a, a * (1.0 - params.add));
}

// The brightness at a pixel of the target: dimmed further in the band.
fn dim_at(x: f32) -> f32 {
    if (x >= params.band.x && x < params.band.y) {
        return params.brightness * params.band_keep;
    }
    return params.brightness;
}

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
    let p = params.rect.xy + c * params.rect.zw;
    var out: VsOut;
    // p is top-left based (y down); clip space is y up.
    out.position = vec4<f32>(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
    out.uv = params.uv_offset + c * params.uv_scale;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(image, image_sampler, in.uv);
    let b = dim_at(in.position.x);
    // The image's own transparency too: StepMania draws backgrounds as
    // sprites, blended, so the first layer shows through the second's.
    let a = c.a * params.alpha * crop_alpha(in.uv);
    var rgb = c.rgb;
    if (params.source_linear > 0.5) {
        rgb = linear_to_srgb(rgb);
    }
    rgb *= params.tint.rgb * b;
    if (params.linear_out > 0.5) {
        rgb = srgb_to_linear(rgb);
    }
    return out_color(rgb, a);
}

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

@fragment
fn fs_yuv(in: VsOut) -> @location(0) vec4<f32> {
    // Samples are the 8-bit codes / 255.
    let y = textureSample(image, image_sampler, in.uv).r * 255.0;
    let u = textureSample(plane_u, image_sampler, in.uv).r * 255.0 - 128.0;
    let v = textureSample(plane_v, image_sampler, in.uv).r * 255.0 - 128.0;
    return convert_yuv(in, y, u, v);
}

@fragment
fn fs_nv12(in: VsOut) -> @location(0) vec4<f32> {
    let y = textureSample(image, image_sampler, in.uv).r * 255.0;
    let uv = textureSample(plane_u, image_sampler, in.uv).rg * 255.0 - vec2<f32>(128.0);
    return convert_yuv(in, y, uv.r, uv.g);
}

fn convert_yuv(in: VsOut, y: f32, u: f32, v: f32) -> vec4<f32> {
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
    var rgb = clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0)) * params.tint.rgb
        * dim_at(in.position.x);
    // Video carries gamma-encoded RGB (dimmed as encoded); an sRGB target
    // blends in linear light, like image textures sampled from sRGB.
    if (params.linear_out > 0.5) {
        rgb = srgb_to_linear(rgb);
    }
    return out_color(rgb, params.alpha * crop_alpha(in.uv));
}
