// Textured quads for noteskins: each instance maps the unit square
// (-0.5..0.5) to the screen with a 2D affine transform and samples a
// rectangle of its texture; or it is a triangle of a model, its corners
// given on the screen with their perspective w. StepMania draws an actor's
// diffuse pass, then its glow pass (the texture's alpha in the glow
// colour) on top (`Sprite::DrawTexture`, `TextureMode_Glow`).

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
    // A triangle: its first two corners.
    @location(0) axes: vec4<f32>,
    // A triangle: its third corner.
    @location(1) origin: vec2<f32>,
    // left, top, right, bottom of the texture, in texture fractions. A
    // triangle: its first two corners'.
    @location(2) uv: vec4<f32>,
    @location(3) color: vec4<f32>,
    // Bit 0: glow (colour, texture alpha) rather than diffuse (texture
    // times colour); bit 1: the texture repeats (hold bodies, models);
    // bit 2: a triangle; bit 3: no texture (the colour alone).
    @location(4) mode: u32,
    // A triangle: its third corner's texture coordinates, and each
    // corner's w.
    @location(5) uv2: vec2<f32>,
    @location(6) w: vec3<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) mode: u32,
    @location(3) @interpolate(flat) rect: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, 0.5), vec2<f32>(-0.5, 0.5),
    );
    var out: VsOut;
    if ((inst.mode & 4u) != 0u) {
        // The second half of the six vertices collapses onto the first
        // corner: nothing drawn.
        let k = select(vi, 0u, vi > 2u);
        var points = array<vec2<f32>, 3>(inst.axes.xy, inst.axes.zw, inst.origin);
        var uvs = array<vec2<f32>, 3>(inst.uv.xy, inst.uv.zw, inst.uv2);
        let w = inst.w[k];
        // Divided by w again on the way: perspective-correct texturing.
        out.position = vec4<f32>((points[k] * globals.scale + globals.translate) * w, 0.0, w);
        out.uv = uvs[k];
    } else {
        let c = corners[vi];
        let px = inst.origin + inst.axes.xy * c.x + inst.axes.zw * c.y;
        out.position = vec4<f32>(px * globals.scale + globals.translate, 0.0, 1.0);
        let t = c + vec2<f32>(0.5, 0.5);
        out.uv = vec2<f32>(mix(inst.uv.x, inst.uv.z, t.x), mix(inst.uv.y, inst.uv.w, t.y));
    }
    out.color = inst.color;
    out.mode = inst.mode;
    out.rect = inst.uv;
    return out;
}

// The quad's texel, its frame sampled within itself: drawn larger than its
// pixels, linear filtering at a frame's edge would blend in the next
// frame's edge (stray pixels that change with the frame). The edge half
// texel repeats instead, as the texture's own edges do.
fn texel_of(in: VsOut) -> vec4<f32> {
    let half_texel = vec2<f32>(0.5) / vec2<f32>(textureDimensions(image));
    let lo = min(in.rect.xy, in.rect.zw);
    let hi = max(in.rect.xy, in.rect.zw);
    let middle = (lo + hi) * 0.5;
    let inside = clamp(in.uv, min(lo + half_texel, middle), max(hi - half_texel, middle));
    let uv = select(inside, in.uv, (in.mode & 2u) != 0u);
    let texel = textureSample(image, image_sampler, uv);
    return select(texel, vec4<f32>(1.0), (in.mode & 8u) != 0u);
}

fn shade(texel: vec4<f32>, in: VsOut) -> vec4<f32> {
    if ((in.mode & 1u) != 0u) {
        return vec4<f32>(in.color.rgb, texel.a * in.color.a);
    }
    return texel * in.color;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return shade(texel_of(in), in);
}
