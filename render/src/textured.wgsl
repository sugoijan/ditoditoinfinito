// Textured quads for noteskins: each instance maps the unit square
// (-0.5..0.5) to the screen with a 2D affine transform and samples a
// rectangle of its texture. StepMania draws an actor's diffuse pass, then
// its glow pass (the texture's alpha in the glow colour) on top
// (`Sprite::DrawTexture`, `TextureMode_Glow`).

struct Globals {
    // Pixels -> clip space: x' = x * sx + tx, y' = y * sy + ty
    scale: vec2<f32>,
    translate: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct Instance {
    // Columns of the linear part: (m0, m1) is where x goes, (m2, m3) y.
    @location(0) axes: vec4<f32>,
    @location(1) origin: vec2<f32>,
    // left, top, right, bottom of the texture, in texture fractions.
    @location(2) uv: vec4<f32>,
    @location(3) color: vec4<f32>,
    // 0: diffuse (texture times colour), 1: glow (colour, texture alpha).
    @location(4) mode: u32,
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
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, 0.5), vec2<f32>(-0.5, 0.5),
    );
    let c = corners[vi];
    let px = inst.origin + inst.axes.xy * c.x + inst.axes.zw * c.y;
    var out: VsOut;
    out.position = vec4<f32>(px * globals.scale + globals.translate, 0.0, 1.0);
    let t = c + vec2<f32>(0.5, 0.5);
    out.uv = vec2<f32>(mix(inst.uv.x, inst.uv.z, t.x), mix(inst.uv.y, inst.uv.w, t.y));
    out.color = inst.color;
    out.mode = inst.mode;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    if (in.mode == 1u) {
        return vec4<f32>(in.color.rgb, texel.a * in.color.a);
    }
    return texel * in.color;
}
