// A material's fragment stage (`docs/plans/noteskin-scene.md`): appended
// to `textured.wgsl`, then the prelude, then the skin's `paint`. What it
// paints is sRGB-encoded with straight alpha; the part's alpha masks it,
// the quad's color tints it, and StepMania's alpha test still applies, so
// masks drawn with a material keep working.

struct DdiMaterials {
    // The song's beat and seconds, then 1 when colors are made in linear
    // light (`NoteskinPlay::set_srgb`), else 0, then 1 when the images are
    // sampled in linear light (an sRGB texture's view), else 0.
    clock: vec4<f32>,
    // The target's size in pixels.
    size: vec4<f32>,
    // Each material's parameters, eight a slot, by the slot in the
    // quad's mode (one array: GLSL ES 3.0 has no arrays of arrays).
    params: array<vec4<f32>, 512>,
};

@group(2) @binding(0) var<uniform> ddi_materials: DdiMaterials;
@group(3) @binding(0) var ddi_image0: texture_2d<f32>;
@group(3) @binding(1) var ddi_image1: texture_2d<f32>;
@group(3) @binding(2) var ddi_images: sampler;

// The material's image `i` (0 or 1) at `uv`, repeating, sRGB-encoded with
// straight alpha. Sampled at its first level, so a material may sample
// anywhere, branches included.
fn image_at(i: u32, uv: vec2<f32>) -> vec4<f32> {
    let a = textureSampleLevel(ddi_image0, ddi_images, uv, 0.0);
    let b = textureSampleLevel(ddi_image1, ddi_images, uv, 0.0);
    let c = select(a, b, i == 1u);
    return select(c, vec4<f32>(to_srgb(c.rgb), c.a), ddi_materials.clock.w > 0.5);
}

// A gradient's ramp (`Images = {"file.svg#id"}`) at `t`, repeating: its
// color `t` of the way across.
fn ramp(i: u32, t: f32) -> vec3<f32> {
    return image_at(i, vec2<f32>(fract(t), 0.5)).rgb;
}

@fragment
fn fs_material(in: VsOut) -> @location(0) vec4<f32> {
    let texel = texel_of(in);
    var m: Material;
    m.pos = in.material.xy;
    let span = in.rect.zw - in.rect.xy;
    m.uv = (in.uv - in.rect.xy) / select(span, vec2<f32>(1.0), span == vec2<f32>(0.0));
    m.mask = texel.a;
    m.screen = in.position.xy / ddi_materials.size.xy;
    m.phase = in.material.z;
    m.seed = in.material.w;
    m.value = in.value;
    m.beat = ddi_materials.clock.x;
    m.seconds = ddi_materials.clock.y;
    let at = min((in.mode >> 8u) & 0xffu, 63u) * 8u;
    let p = &ddi_materials.params;
    m.params = array<vec4<f32>, 8>(
        (*p)[at], (*p)[at + 1u], (*p)[at + 2u], (*p)[at + 3u],
        (*p)[at + 4u], (*p)[at + 5u], (*p)[at + 6u], (*p)[at + 7u],
    );
    let painted = clamp(paint(m), vec4<f32>(0.0), vec4<f32>(1.0));
    let rgb = select(painted.rgb, to_linear(painted.rgb), ddi_materials.clock.z > 0.5);
    let color = select(in.color, in.graded, (in.mode & 64u) != 0u);
    let out = vec4<f32>(rgb * color.rgb, painted.a * texel.a * color.a);
    if (out.a <= 0.00390625) {
        discard;
    }
    return out;
}
