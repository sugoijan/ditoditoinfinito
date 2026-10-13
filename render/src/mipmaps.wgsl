// One mip level from the one above it (`render/src/mipmaps.rs`): each
// texel the bilinear average of the 2 × 2 it covers.

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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(source, source_sampler, in.uv, 0.0);
}
