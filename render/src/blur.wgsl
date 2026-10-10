// The blurred extension around the background frame
// (docs/plans/video-backgrounds.md, step 5): the backdrop is drawn small,
// blurred in two passes (`fs_blur`, one direction each) and stretched over
// the screen, dimmed (`fs_composite`). Colours stay premultiplied by alpha
// throughout: the small target starts transparent.

struct Blur {
    // One texel along the blur's direction (unused by the composite).
    step: vec2<f32>,
    // The composite's brightness multiplier.
    brightness: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> blur: Blur;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // One triangle over the whole target; uv is top-left based (y down).
    let c = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var out: VsOut;
    out.position = vec4<f32>(c.x * 2.0 - 1.0, 1.0 - c.y * 2.0, 0.0, 1.0);
    out.uv = c;
    return out;
}

// A Gaussian of σ = 3 texels, 13 taps.
@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -6; i <= 6; i++) {
        let x = f32(i);
        let w = exp(-x * x / 18.0);
        sum += w * textureSampleLevel(source, source_sampler, in.uv + blur.step * x, 0.0);
        total += w;
    }
    return sum / total;
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(source, source_sampler, in.uv, 0.0);
    return vec4<f32>(c.rgb * blur.brightness, c.a);
}
