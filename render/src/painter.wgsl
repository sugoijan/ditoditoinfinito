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
    // goes, (m2, m3) y; and where the origin goes, in pixels. Seen in
    // perspective: the top left and top right corners, in pixels.
    @location(0) axes: vec4<f32>,
    @location(1) origin: vec2<f32>,
    // Bit 0: the texture holds coverage in red (a text page); bit 1: no
    // texture (the colours alone); bit 2: StepMania's glow (the colours
    // with the texture's alpha); bit 3: seen in perspective.
    @location(2) mode: u32,
    // left, top, right, bottom in the instance's space. In perspective:
    // the bottom right and bottom left corners, in pixels.
    @location(3) rect: vec4<f32>,
    // The texture's rectangle, in fractions of it.
    @location(4) uv: vec4<f32>,
    // Top left, top right, bottom right, bottom left.
    @location(5) c0: vec4<f32>,
    @location(6) c1: vec4<f32>,
    @location(7) c2: vec4<f32>,
    @location(8) c3: vec4<f32>,
    // The corners' w (top left, top right, bottom right, bottom left).
    @location(9) w: vec4<f32>,
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
    var out: VsOut;
    if ((inst.mode & 8u) != 0u) {
        // Each corner where it is seen, divided by its w again on the way:
        // texturing in perspective.
        var which = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
        var points = array<vec2<f32>, 4>(inst.axes.xy, inst.axes.zw, inst.rect.xy, inst.rect.zw);
        let k = which[vi];
        let w = inst.w[k];
        out.position = vec4<f32>((points[k] * globals.scale + globals.translate) * w, 0.0, w);
    } else {
        let local = mix(inst.rect.xy, inst.rect.zw, t);
        let px = inst.origin + inst.axes.xy * local.x + inst.axes.zw * local.y;
        out.position = vec4<f32>(px * globals.scale + globals.translate, 0.0, 1.0);
    }
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
    var out = t * in.color;
    if ((in.mode & 4u) != 0u) {
        out = vec4<f32>(in.color.rgb, t.a * in.color.a);
    }
    // StepMania's alpha test, on for every draw: what is no more than
    // 1/256 opaque is not drawn (nor masks), `RageDisplay_OGL.cpp:2731`.
    if (out.a <= 0.00390625) {
        discard;
    }
    return out;
}
