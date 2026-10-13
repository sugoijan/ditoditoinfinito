// A frame drawn offscreen (`render/src/offscreen.rs`), copied texel for
// texel: the scene's linear light into the plain texture encoded
// (`fs_encode`), the plain texture's encoded values back decoded
// (`fs_decode`), and the scene to the sRGB target, which encodes it
// (`fs_copy`). No source is an sRGB texture.

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle over the target; uv is top-left based (y down).
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.0), vec2<f32>(2.0, 0.0), vec2<f32>(0.0, 2.0),
    );
    let c = corners[vi];
    var out: VsOut;
    out.position = vec4<f32>(c.x * 2.0 - 1.0, 1.0 - c.y * 2.0, 0.0, 1.0);
    out.uv = c;
    return out;
}

fn texel(in: VsOut) -> vec4<f32> {
    return textureSampleLevel(source, source_sampler, in.uv, 0.0);
}

@fragment
fn fs_encode(in: VsOut) -> @location(0) vec4<f32> {
    let c = clamp(texel(in), vec4<f32>(0.0), vec4<f32>(1.0));
    let low = c.rgb * 12.92;
    let high = 1.055 * pow(c.rgb, vec3<f32>(1.0 / 2.4)) - 0.055;
    return vec4<f32>(select(high, low, c.rgb <= vec3<f32>(0.0031308)), c.a);
}

@fragment
fn fs_decode(in: VsOut) -> @location(0) vec4<f32> {
    let c = texel(in);
    let low = c.rgb / 12.92;
    let high = pow((c.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return vec4<f32>(select(high, low, c.rgb <= vec3<f32>(0.04045)), c.a);
}

@fragment
fn fs_copy(in: VsOut) -> @location(0) vec4<f32> {
    return texel(in);
}
