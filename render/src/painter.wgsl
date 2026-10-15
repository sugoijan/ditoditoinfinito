// The 2D pass (`painter.rs`): each instance is a rectangle of its own
// space placed on the screen by a 2D affine transform, sampling a
// rectangle of an image, of a text page's coverage, or nothing, tinted
// with one colour a corner. Colours and texels are encoded values with
// straight alpha, drawn to a plain (non-sRGB) target: blending happens
// in encoded values, as StepMania and the browser blend
// (`docs/plans/ui-core.md`, P11).

struct Globals {
    // Pixels -> clip space: x' = x * sx + tx, y' = y * sy + ty
    scale: vec2<f32>,
    translate: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct Instance {
    // The linear part's columns, pixels per unit: (m0, m1) is where x
    // goes, (m2, m3) y; and where the origin goes, in pixels.
    @location(0) axes: vec4<f32>,
    @location(1) origin: vec2<f32>,
    // Bit 0: the texture holds coverage in red (a text page); bit 1: no
    // texture (the colours alone).
    @location(2) mode: u32,
    // left, top, right, bottom in the instance's space.
    @location(3) rect: vec4<f32>,
    // The texture's rectangle, in fractions of it.
    @location(4) uv: vec4<f32>,
    // Top left, top right, bottom right, bottom left.
    @location(5) c0: vec4<f32>,
    @location(6) c1: vec4<f32>,
    @location(7) c2: vec4<f32>,
    @location(8) c3: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) mode: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    // The corner's colour, in the order of `corners`.
    var colors = array<vec4<f32>, 6>(inst.c0, inst.c1, inst.c2, inst.c0, inst.c2, inst.c3);
    let t = corners[vi];
    let local = mix(inst.rect.xy, inst.rect.zw, t);
    let px = inst.origin + inst.axes.xy * local.x + inst.axes.zw * local.y;
    var out: VsOut;
    out.position = vec4<f32>(px * globals.scale + globals.translate, 0.0, 1.0);
    out.uv = mix(inst.uv.xy, inst.uv.zw, t);
    out.color = colors[vi];
    out.mode = inst.mode;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    var t = texel;
    if ((in.mode & 1u) != 0u) {
        // White text: its coverage is its alpha.
        t = vec4<f32>(1.0, 1.0, 1.0, texel.r);
    }
    if ((in.mode & 2u) != 0u) {
        t = vec4<f32>(1.0);
    }
    return t * in.color;
}
