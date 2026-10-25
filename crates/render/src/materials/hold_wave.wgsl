// The wave `HoldStencilMaterial`, `RollStencilMaterial` and
// `LiftStencilMaterial` paint (the `HoldMaterial*` metrics,
// `crates/noteskin/src/metrics.rs`): a phase along an angle, warped by
// one sine, through a loop of three colors (stops at 0, 0.3, 0.5, 0.7
// and 1: the first, second, third, second, first).
// params[0]: the angle's sine and cosine, the period, the warp's height;
// params[1]: the warp's wavelengths across and along the note;
// params[2], [3], [4]: the colors. value.x scales the colors (a hold let
// go greys).

fn paint(m: Material) -> vec4<f32> {
    let p = m.params;
    let warp = p[0].w * sin(TAU * (m.pos.x / p[1].x + m.pos.y / p[1].y));
    let q = fract((p[0].x * m.pos.x + p[0].y * m.pos.y + warp) / p[0].z + m.phase);
    var c: vec3<f32>;
    if (q <= 0.3) {
        c = mix(p[2].rgb, p[3].rgb, q / 0.3);
    } else if (q <= 0.5) {
        c = mix(p[3].rgb, p[4].rgb, (q - 0.3) / 0.2);
    } else if (q <= 0.7) {
        c = mix(p[4].rgb, p[3].rgb, (q - 0.5) / 0.2);
    } else {
        c = mix(p[3].rgb, p[2].rgb, (q - 0.7) / 0.3);
    }
    return vec4<f32>(c * m.value.x, 1.0);
}
