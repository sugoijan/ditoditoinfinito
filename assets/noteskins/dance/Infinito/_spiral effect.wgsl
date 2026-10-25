// A fixed SVG mask tinted by params[0], or an effect image rotated inside a
// square mask. params[1]: image mode, rotation variation, independent salt.
// The two W1 burst passes share a salt so their silhouettes stay aligned.
fn paint(m: Material) -> vec4<f32> {
    if (m.params[1].x < 0.5) {
        return m.params[0];
    }
    let angle = TAU * hash12(vec2<f32>(m.seed, m.params[1].z)) * m.params[1].y;
    let p = m.uv - vec2<f32>(0.5);
    let uv = vec2<f32>(cos(angle) * p.x + sin(angle) * p.y,
                      -sin(angle) * p.x + cos(angle) * p.y) + vec2<f32>(0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(m.params[0].rgb, image_at(0u, uv).a * m.params[0].a);
}
