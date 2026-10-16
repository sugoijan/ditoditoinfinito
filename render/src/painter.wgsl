// The 2D pass (`painter.rs`): each instance is a rectangle of its own
// space placed on the screen by a 2D affine transform, sampling a
// rectangle of an image, of a text page's coverage, or nothing, tinted
// with one colour a corner, perhaps smeared along a motion blur. Colours and texels are encoded values with
// straight alpha, drawn to a plain (non-sRGB) target: blending happens
// in encoded values, as StepMania and the browser blend
// (`docs/plans/ui-core.md`, P11).

struct Globals {
    // Pixels -> clip space: x' = x * sx + tx, y' = y * sy + ty
    scale: vec2<f32>,
    translate: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct Instance {
    // The linear part's columns, pixels per unit: (m0, m1) is where x
    // goes, (m2, m3) y; and where the origin goes, in pixels. Seen in
    // perspective: the top left and top right corners, in pixels.
    @location(0) axes: vec4<f32>,
    @location(1) origin: vec2<f32>,
    // Bit 0: the texture holds coverage in red (a text page); bit 1: no
    // texture (the colours alone); bit 2: StepMania's glow (the colours
    // with the texture's alpha); bit 3: seen in perspective.
    @location(2) mode: u32,
    // left, top, right, bottom in the instance's space. In perspective:
    // the bottom right and bottom left corners, in pixels.
    @location(3) rect: vec4<f32>,
    // The texture's rectangle, in fractions of it.
    @location(4) uv: vec4<f32>,
    // Top left, top right, bottom right, bottom left.
    @location(5) c0: vec4<f32>,
    @location(6) c1: vec4<f32>,
    @location(7) c2: vec4<f32>,
    @location(8) c3: vec4<f32>,
    // The corners' w (top left, top right, bottom right, bottom left).
    @location(9) w: vec4<f32>,
    // Bit 4 (cut): the corners' radius and a soft edge, in the rect's
    // units per axis; an outline's thickness (shape2.xy). Bit 5: the
    // saturation (shape2.z). Our extension for the game's theme.
    @location(10) shape: vec4<f32>,
    @location(11) shape2: vec4<f32>,
    // Bit 6: a motion blur along blur.xy, in the rect's units (our
    // extension; never in perspective).
    @location(12) blur: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) mode: u32,
    // Where in the rect's space (cut quads).
    @location(3) local: vec2<f32>,
    @location(4) @interpolate(flat) rect: vec4<f32>,
    @location(5) @interpolate(flat) shape: vec4<f32>,
    @location(6) @interpolate(flat) shape2: vec4<f32>,
    // Pixels per unit along x and y.
    @location(7) @interpolate(flat) ppu: vec2<f32>,
    // A motion blur: its vector in the rect's units, and the texture's
    // fractions per unit along x and y.
    @location(8) @interpolate(flat) blur: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    // The corner's colour, in the order of `corners`.
    var colors = array<vec4<f32>, 6>(inst.c0, inst.c1, inst.c2, inst.c0, inst.c2, inst.c3);
    let t = corners[vi];
    var out: VsOut;
    if ((inst.mode & 8u) != 0u) {
        // Each corner where it is seen, divided by its w again on the way:
        // texturing in perspective.
        var which = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
        var points = array<vec2<f32>, 4>(inst.axes.xy, inst.axes.zw, inst.rect.xy, inst.rect.zw);
        let k = which[vi];
        let w = inst.w[k];
        out.position = vec4<f32>((points[k] * globals.scale + globals.translate) * w, 0.0, w);
    } else if ((inst.mode & 64u) != 0u) {
        // Blurred: the rect grows by half the blur each way, the texture's
        // rectangle with it; what lies outside the quad draws nothing.
        let size = inst.rect.zw - inst.rect.xy;
        let grow = abs(inst.blur.xy) * 0.5 * sign(size);
        let rect = vec4<f32>(inst.rect.xy - grow, inst.rect.zw + grow);
        var per = vec2<f32>(0.0);
        if (size.x != 0.0) {
            per.x = (inst.uv.z - inst.uv.x) / size.x;
        }
        if (size.y != 0.0) {
            per.y = (inst.uv.w - inst.uv.y) / size.y;
        }
        let uv = vec4<f32>(inst.uv.xy - grow * per, inst.uv.zw + grow * per);
        let local = mix(rect.xy, rect.zw, t);
        let px = inst.origin + inst.axes.xy * local.x + inst.axes.zw * local.y;
        out.position = vec4<f32>(px * globals.scale + globals.translate, 0.0, 1.0);
        out.uv = mix(uv.xy, uv.zw, t);
        out.color = colors[vi];
        out.mode = inst.mode;
        out.local = local;
        out.rect = inst.rect;
        out.shape = inst.shape;
        out.shape2 = inst.shape2;
        out.ppu = vec2<f32>(length(inst.axes.xy), length(inst.axes.zw));
        out.blur = vec4<f32>(inst.blur.xy, per);
        return out;
    } else {
        let local = mix(inst.rect.xy, inst.rect.zw, t);
        let px = inst.origin + inst.axes.xy * local.x + inst.axes.zw * local.y;
        out.position = vec4<f32>(px * globals.scale + globals.translate, 0.0, 1.0);
    }
    out.uv = mix(inst.uv.xy, inst.uv.zw, t);
    out.color = colors[vi];
    out.mode = inst.mode;
    out.local = mix(inst.rect.xy, inst.rect.zw, t);
    out.rect = inst.rect;
    out.shape = inst.shape;
    out.shape2 = inst.shape2;
    out.ppu = vec2<f32>(length(inst.axes.xy), length(inst.axes.zw));
    out.blur = vec4<f32>(0.0);
    return out;
}

// The signed distance to a rectangle of half size `half` with corners of
// radius `r`, from its centre (negative inside), in pixels.
fn rounded(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// How much of a pixel at `d` (pixels from the edge) is inside, the edge
// anti-aliased, or spread over `soft` pixels each way.
fn inside(d: f32, soft: f32) -> f32 {
    if (soft > 0.0) {
        return 1.0 - smoothstep(-soft, soft, d);
    }
    return clamp(0.5 - d, 0.0, 1.0);
}

// The share of a cut quad's pixel that is drawn.
fn cut(in: VsOut) -> f32 {
    return cut_at(in, in.local);
}

// The share drawn of a cut quad's pixel at `local`.
fn cut_at(in: VsOut, local: vec2<f32>) -> f32 {
    let ppu = in.ppu;
    let centre = (in.rect.xy + in.rect.zw) * 0.5;
    let p = (local - centre) * ppu;
    let soft = max(in.shape.z * ppu.x, in.shape.w * ppu.y);
    let half = abs(in.rect.zw - in.rect.xy) * 0.5 * ppu - vec2<f32>(soft);
    let r = clamp(min(in.shape.x * ppu.x, in.shape.y * ppu.y), 0.0, max(min(half.x, half.y), 0.0));
    let d = rounded(p, max(half, vec2<f32>(0.0)), r);
    var a = inside(d, soft);
    let stroke = min(in.shape2.x * ppu.x, in.shape2.y * ppu.y);
    if (stroke > 0.0) {
        // Outside the shape inset by the stroke too.
        a = a * (1.0 - inside(d + stroke, soft));
    }
    return a;
}

// One tap of a motion blur: the quad's colour at `uv` and `local`, as
// `fs_main` makes it before the saturation, nothing outside the quad.
fn tap(in: VsOut, uv: vec2<f32>, local: vec2<f32>, lod: f32) -> vec4<f32> {
    let lo = min(in.rect.xy, in.rect.zw);
    let hi = max(in.rect.xy, in.rect.zw);
    if (any(local < lo) || any(local > hi)) {
        return vec4<f32>(0.0);
    }
    var t = textureSampleLevel(image, image_sampler, uv, lod);
    if ((in.mode & 1u) != 0u) {
        t = vec4<f32>(1.0, 1.0, 1.0, t.r);
    }
    if ((in.mode & 2u) != 0u) {
        t = vec4<f32>(1.0);
    }
    var out = t * in.color;
    if ((in.mode & 4u) != 0u) {
        out = vec4<f32>(in.color.rgb, t.a * in.color.a);
    }
    if ((in.mode & 16u) != 0u) {
        out.a = out.a * cut_at(in, local);
    }
    return out;
}

// A motion-blurred pixel: taps along the blur, at most 2 pixels apart
// (16 at most, their places dithered so long blurs do not band),
// averaged with their alpha.
fn blurred(in: VsOut) -> vec4<f32> {
    let along = in.blur.xy;
    let pixels = length(along * in.ppu);
    let n = u32(clamp(ceil(pixels * 0.5), 2.0, 16.0));
    // Interleaved gradient noise (Jimenez 2014), a tap's fraction.
    let jitter = fract(52.9829189 * fract(dot(in.position.xy, vec2<f32>(0.06711056, 0.00583715))));
    // The mipmap a plain sample would pick: texels a pixel.
    let texels = abs(in.blur.zw) * vec2<f32>(textureDimensions(image, 0)) / max(in.ppu, vec2<f32>(1e-6));
    let lod = max(log2(max(max(texels.x, texels.y), 1e-6)), 0.0);
    var sum = vec4<f32>(0.0);
    for (var i = 0u; i < n; i = i + 1u) {
        let f = (f32(i) + jitter) / f32(n) - 0.5;
        let c = tap(in, in.uv + along * in.blur.zw * f, in.local + along * f, lod);
        sum = sum + vec4<f32>(c.rgb * c.a, c.a);
    }
    var out = vec4<f32>(0.0);
    if (sum.a > 0.0) {
        out = vec4<f32>(sum.rgb / sum.a, sum.a / f32(n));
    }
    if ((in.mode & 32u) != 0u) {
        let grey = dot(out.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        out = vec4<f32>(mix(vec3<f32>(grey), out.rgb, in.shape2.z), out.a);
    }
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    if ((in.mode & 64u) != 0u) {
        let out = blurred(in);
        // The alpha test, as below.
        if (out.a <= 0.00390625) {
            discard;
        }
        return out;
    }
    var t = texel;
    if ((in.mode & 1u) != 0u) {
        // White text: its coverage is its alpha.
        t = vec4<f32>(1.0, 1.0, 1.0, texel.r);
    }
    if ((in.mode & 2u) != 0u) {
        t = vec4<f32>(1.0);
    }
    var out = t * in.color;
    if ((in.mode & 4u) != 0u) {
        out = vec4<f32>(in.color.rgb, t.a * in.color.a);
    }
    if ((in.mode & 16u) != 0u) {
        out.a = out.a * cut(in);
    }
    if ((in.mode & 32u) != 0u) {
        let grey = dot(out.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        out = vec4<f32>(mix(vec3<f32>(grey), out.rgb, in.shape2.z), out.a);
    }
    // StepMania's alpha test, on for every draw: what is no more than
    // 1/256 opaque is not drawn (nor masks), `RageDisplay_OGL.cpp:2731`.
    if (out.a <= 0.00390625) {
        discard;
    }
    return out;
}
