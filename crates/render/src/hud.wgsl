// Accepted play HUD. Color math is in encoded sRGB, converted only at output.
struct Globals {
    screen: vec4<f32>, bar: vec4<f32>, combo_rect: vec4<f32>, mark: vec4<f32>,
    life: vec4<f32>, motion: vec4<f32>, combo: vec4<f32>, digits: array<vec4<f32>, 3>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) kind: u32,
};
@vertex fn vs_main(@builtin(vertex_index) vertex: u32, @builtin(instance_index) kind: u32) -> Vertex {
    var corners = array<vec2<f32>, 6>(vec2(0.,0.),vec2(1.,0.),vec2(0.,1.),vec2(0.,1.),vec2(1.,0.),vec2(1.,1.));
    var rect = g.bar;
    if kind == 1u { rect = g.combo_rect; }
    if kind == 2u { rect = g.mark; }
    let uv = corners[vertex];
    let p = rect.xy + uv * rect.zw;
    return Vertex(vec4(p / g.screen.xy * vec2(2.,-2.) + vec2(-1.,1.), 0., 1.), uv, kind);
}

fn hash(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2(.1031, .103));
    q += dot(q, q.yx + vec2(33.33));
    return fract((q.x + q.y) * q.x);
}
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p); let f = fract(p); let u = f * f * (vec2(3.) - 2. * f);
    return mix(mix(hash(i), hash(i + vec2(1.,0.)), u.x),
        mix(hash(i + vec2(0.,1.)), hash(i + vec2(1.,1.)), u.x), u.y);
}
fn ramp(at: f32) -> vec3<f32> {
    var colors = array<vec3<f32>, 8>(
        vec3(49.,213.,255.),vec3(107.,91.,255.),vec3(195.,61.,255.),vec3(255.,78.,185.),
        vec3(255.,136.,61.),vec3(227.,236.,62.),vec3(72.,239.,155.),vec3(49.,213.,255.));
    let x = fract(at) * 7.; let i = u32(floor(x));
    return mix(colors[i], colors[i+1u], fract(x)) / 255.;
}
fn life(uv: vec2<f32>) -> vec4<f32> {
    let p = uv * vec2(5.2, .32); let t = g.motion.x;
    let d = vec2(noise(p * .6 + vec2(t, t * .23)),
        noise(p * .6 + vec2(8.7 - t * .31, 2.3 + t))) - vec2(.5);
    let w = p + d * 1.35;
    let folds = noise(w + vec2(-t * .15, t * .37));
    let q = fract((w.x * .18 + w.y * .12 + 1.25 * folds) * 2. - 97. / 360.);
    let green = clamp(.25 + .5 * uv.x + cos(q * 6.2831853) * .125, 0., 1.);
    var color = vec3(36.,33.,43.) / 255.;
    if uv.x < g.life.z && uv.x >= max(g.life.x, g.life.y) {
        let red = mix(vec3(138.,25.,44.), vec3(221.,88.,54.), green) / 255.;
        color = mix(color, red, g.life.w);
    }
    if uv.x < g.life.y && uv.x >= g.life.x {
        color = mix(vec3(21.,82.,158.), vec3(131.,201.,244.), green) / 255.;
    }
    if uv.x < g.life.x {
        color = mix(vec3(0.,92.,69.), vec3(34.,195.,85.), green) / 255.;
        let vivid = mix(ramp(g.motion.y + .43 * q), ramp(g.motion.y + .43 * (q-1.)), smoothstep(.5,1.,q));
        color = mix(color, vivid, g.motion.z);
        color = mix(color, vec3(223.,105.,94.) / 255., .9 * g.motion.w);
    }
    let radius = g.bar.w * .5;
    let b = abs((uv - vec2(.5)) * g.bar.zw) - g.bar.zw * .5 + vec2(radius);
    let distance = length(max(b, vec2(0.))) + min(max(b.x,b.y),0.) - radius;
    return vec4(color, 1. - smoothstep(-.5,.5,distance));
}
fn combo(uv: vec2<f32>) -> vec4<f32> {
    if g.combo.x < 1. { return vec4(0.); }
    let p = uv * g.combo_rect.zw / g.combo.z;
    var distance = -16.;
    // Union the neighboring glyphs before drawing the outline. The gap
    // remains transparent even where two adjacent digits' rings overlap.
    for (var i = 0u; i < 10u; i += 1u) {
        if f32(i) >= g.combo.x { break; }
        let local = p - vec2(f32(i) * 32., 0.);
        if local.x >= .5 && local.x <= 63.5 {
            let digit = g.digits[i / 4u][i % 4u];
            let sample_uv = vec2(digit * 64. + local.x, clamp(local.y,.5,95.5)) / vec2(640.,352.);
            distance = max(distance, (textureSampleLevel(atlas, atlas_sampler, sample_uv, 0.).r - .5) * 32.);
        }
    }
    distance *= g.combo.z;
    let fill = smoothstep(-.65, .65, distance);
    let tier = g.combo.y;
    let inverted = tier >= 10.;
    let s = g.combo.z / select(1.,1.2,inverted);
    let gap = 2.5 * s; let outline = g.combo.w * s;
    var ring = 0.;
    if tier >= 1. {
        ring = smoothstep(-gap - outline - .65, -gap - outline + .65, distance)
            * (1. - smoothstep(-gap - .65, -gap + .65, distance));
    }
    let base = select((tier - 1.) / 10., g.motion.y, inverted);
    // The study's 360x116 canvas gradient, independent of numeral count.
    let axis = vec2(min(g.screen.x, 360. * s) * .5, 116. * s);
    let at = clamp(.5 + dot((uv-vec2(.5)) * g.combo_rect.zw, axis) / dot(axis, axis), 0., 1.) * 4.;
    let stop = min(floor(at), 3.);
    let colored = mix(ramp(base + stop * .09), ramp(base + (stop+1.) * .09), at-stop);
    let white = vec3(242.,238.,233.) / 255.;
    let fill_color = select(white, colored, inverted);
    let ring_color = select(colored, white, inverted);
    let alpha = fill + ring;
    return vec4((fill_color * fill + ring_color * ring) / max(alpha,.00001), alpha);
}
fn watermark(uv: vec2<f32>) -> vec4<f32> {
    let p = clamp(uv * vec2(640.,128.), vec2(.5), vec2(639.5,127.5));
    let letters = textureSampleLevel(atlas, atlas_sampler, (p + vec2(0.,96.)) / vec2(640.,352.), 0.).r;
    let rings = textureSampleLevel(atlas, atlas_sampler, (p + vec2(0.,224.)) / vec2(640.,352.), 0.).r;
    let alpha = letters + rings;
    return vec4((vec3(242.,238.,233.) * letters + vec3(163.,156.,149.) * rings) / (255. * max(alpha,.00001)), alpha * .1);
}
@fragment fn fs_main(v: Vertex) -> @location(0) vec4<f32> {
    var color: vec4<f32>;
    if v.kind == 0u { color = life(v.uv); }
    else if v.kind == 1u { color = combo(v.uv); }
    else { color = watermark(v.uv); }
    if g.screen.z > .5 {
        color = vec4(select(color.rgb / 12.92, pow((color.rgb + vec3(.055)) / 1.055, vec3(2.4)), color.rgb > vec3(.04045)), color.a);
    }
    return color;
}
