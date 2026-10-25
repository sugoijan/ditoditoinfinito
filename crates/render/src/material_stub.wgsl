// What `material.wgsl` gives a material besides the prelude, declared
// alone, for checking a material by itself (`material::compile`).

fn image_at(i: u32, uv: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(0.0);
}

fn ramp(i: u32, t: f32) -> vec3<f32> {
    return vec3<f32>(0.0);
}
