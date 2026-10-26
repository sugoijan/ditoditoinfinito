// Adapter for Infinito's actual material source and shared material prelude.
// Palette stops: assets/noteskins/dance/Infinito/_spiral components.svg.
fn ramp(i: u32, t: f32) -> vec3<f32> {
    let palette = array<vec3<f32>, 8>(
        vec3<f32>(49.0, 213.0, 255.0) / 255.0,
        vec3<f32>(107.0, 91.0, 255.0) / 255.0,
        vec3<f32>(195.0, 61.0, 255.0) / 255.0,
        vec3<f32>(255.0, 78.0, 185.0) / 255.0,
        vec3<f32>(255.0, 136.0, 61.0) / 255.0,
        vec3<f32>(227.0, 236.0, 62.0) / 255.0,
        vec3<f32>(72.0, 239.0, 155.0) / 255.0,
        vec3<f32>(49.0, 213.0, 255.0) / 255.0
    );
    let at = fract(t) * 7.0;
    let stop = min(u32(floor(at)), 6u);
    return mix(palette[stop], palette[stop + 1u], fract(at));
}

// The paint at `pos` of the field, `clock` the material's seconds and
// phase (`Instance::blur.zw`).
fn vivid(pos: vec2<f32>, clock: vec2<f32>) -> vec3<f32> {
    var m: Material;
    m.pos = pos;
    m.seconds = clock.x;
    m.phase = clock.y;
    m.params[0] = vec4<f32>(120.6, 1.0, 97.0, 0.43);
    m.params[1] = vec4<f32>(0.5, 1.089, 0.0, 8.712);
    m.params[2] = vec4<f32>(0.0);
    m.params[3] = vec4<f32>(0.021, 1.35, 0.3, 2.0);
    return paint(m).rgb;
}
