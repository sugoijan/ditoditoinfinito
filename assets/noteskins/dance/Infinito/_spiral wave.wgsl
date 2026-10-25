// Infinito's long-note paint: broad waves, independent of the palette phase.
// params[0]: angle sine/cosine, palette period, wave height.
// params[1]: spatial wave lengths across/along the lane.
// params[2..4]: dark, middle and light colors.
// params[5].x: lift head's white overlay.
// params[6]: wave evolution speed (turns/second), secondary wave mix.
// The renderer tints dropped long notes. All pieces share lane coordinates.
fn paint(m: Material) -> vec4<f32> {
    let p = m.params;
    let t = TAU * m.seconds * p[6].x;
    let across = m.pos.x / p[1].x;
    let along = m.pos.y / p[1].y;
    // An unwrapped, low-frequency drift gently changes the bend, rather than
    // resetting its shape whenever the one-second color phase loops.
    let drift = 0.35 * (noise(vec2<f32>(along * 0.4, t * 0.13)) - 0.5);
    let primary = sin(TAU * (across + along) + t + drift);
    let secondary = sin(TAU * (across * 0.73 - along * 0.46) - t * 0.61);
    let warp = p[0].w * mix(primary, secondary, p[6].y);
    let q = fract((p[0].x * m.pos.x + p[0].y * m.pos.y + warp) / p[0].z + m.phase);
    var color: vec3<f32>;
    if q <= 0.3 {
        color = mix(p[2].rgb, p[3].rgb, q / 0.3);
    } else if q <= 0.5 {
        color = mix(p[3].rgb, p[4].rgb, (q - 0.3) / 0.2);
    } else if q <= 0.7 {
        color = mix(p[4].rgb, p[3].rgb, (q - 0.5) / 0.2);
    } else {
        color = mix(p[3].rgb, p[2].rgb, (q - 0.7) / 0.3);
    }
    return vec4<f32>(mix(color, vec3<f32>(1.0), p[5].x), 1.0);
}
