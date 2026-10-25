// Infinito's flowing color field, also editable in the shader playground.
// Original domain-warped noise; hhholographic is a visual reference only.
// The SVG silhouette stays fixed. Seconds evolve the spatial field;
// the bound phase independently loops the accepted Vivid palette window.
// params[0..2] retain the spiral material's coordinate/palette parameters.
// params[3]: spatial scale, warp strength, evolution speed, contour density.
fn paint(m: Material) -> vec4<f32> {
    let p0 = m.params[0];
    let p1 = m.params[1];
    let flow = m.params[3];
    let angle = m.params[2].x * PI / 180.0;
    let local = vec2<f32>(cos(angle) * m.pos.x + sin(angle) * m.pos.y,
                         -sin(angle) * m.pos.x + cos(angle) * m.pos.y);
    let p = (local - p1.zw) / p1.y * flow.x;
    let t = m.seconds * flow.z;
    // Different noise trajectories continuously reshape the folds. There is
    // no phase wrap or modulo on time, and no repeated set of animation frames.
    let drift = vec2<f32>(
        noise(p * 0.6 + vec2<f32>(t, t * 0.23)),
        noise(p * 0.6 + vec2<f32>(8.7 - t * 0.31, 2.3 + t))
    ) - vec2<f32>(0.5);
    let warped = p + drift * flow.y;
    let folds = noise(warped + vec2<f32>(-t * 0.15, t * 0.37));
    let field = dot(warped, vec2<f32>(0.18, 0.12)) + 1.25 * folds;
    let q = fract(field * flow.w - p0.z / 360.0);
    if m.params[2].y > 0.5 {
        // Bind.Note's 64-beat phase adds floor(fract(noteBeat) * 64).
        // Remove the song beat modulo 64, rounding off float noise. This
        // recovers timing on the full chart grid, including triplets, rather
        // than conflating all notes of the same denominator.
        let tick = floor(fmod(m.phase - m.beat + 0.25, 64.0));
        let family = u32(fmod(ceil(tick / 16.0) - 1.0, 4.0));
        // The game's Rainbow families and their light gradient endpoints.
        let base = array<vec3<f32>,4>(
            vec3<f32>(1.0,0.45,0.75), vec3<f32>(0.3,0.65,1.0),
            vec3<f32>(0.65,0.4,1.0), vec3<f32>(1.0,0.6,0.25));
        let light = array<vec3<f32>,4>(
            vec3<f32>(1.0,0.82,0.92), vec3<f32>(0.35,0.95,1.0),
            vec3<f32>(0.95,0.45,0.95), vec3<f32>(1.0,0.92,0.35));
        // A smooth closed cycle travels through the spiral once per beat.
        let wave = 0.5 - 0.5 * cos(TAU * (q - m.beat));
        return vec4<f32>(mix(base[family], light[family], wave), 1.0);
    }
    // The same closed palette window as the accepted spiral material.
    let a = ramp(0u, m.phase + p0.w * q);
    let b = ramp(0u, m.phase + p0.w * (q - 1.0));
    let u = smoothstep(0.0, 1.0, (q - (1.0 - p1.x)) / p1.x);
    return vec4<f32>(mix(a, b, u), 1.0);
}
