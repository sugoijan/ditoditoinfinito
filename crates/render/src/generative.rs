//! Generative backgrounds' pass (`docs/plans/phase-8-gaps.md`, step 5):
//! a look of `ddi_library::generative` drawn from its [`Signals`] into a
//! texture, which the backdrop then shows as it shows a song's picture
//! (in the background frame, crossfaded, under the field filter's band;
//! [`crate::Renderer::add_generated`]). As the design study drew them
//! (`tools/background-design/visuals.js`): a look over the whole texture
//! (`generative.wgsl`'s `fs_look`), then strokes over it (the field's
//! lines, the rain's outlines), all in encoded values, so the texture
//! holds encoded values on any target ([`FORMAT`]).

use bytemuck::{Pod, Zeroable};
use ddi_library::generative::{BAR_COUNT, BARS, FIELD, KALEIDOSCOPE, Look, Palette, RAIN, Signals};
use ddi_library::music::BANDS;

/// The generated texture's format: encoded values, as the study's canvas
/// held them, whatever the target.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Points round an outline.
const POINTS: usize = 120;
/// Segments along a line of the field.
const LINE_STEPS: u32 = 160;
/// Strokes a frame draws at most.
const MAX_STROKES: usize = 64;

/// The start menu's rain outlines (`assets/ui/menu-rain.svg`), in its
/// 64-unit square: Infinito's arrow, pointing up, and its onigiri.
const ARROW: &str = "M32 4L53.813 25.813A8.75 8.75 0 0 1 41.438 38.187L40.75 37.499V47.626A8.75 8.75 0 0 1 23.25 47.626V37.499L22.562 38.187A8.75 8.75 0 0 1 10.187 25.813Z";
const ONIGIRI: &str = "M24.924 20.762A8.17 8.17 0 0 1 39.076 20.762L52.052 43.238A8.17 8.17 0 0 1 44.976 55.493L43.246 55.493A1.377 1.377 0 0 1 41.869 54.116L41.869 41.448A1.377 1.377 0 0 0 40.492 40.07L23.508 40.07A1.377 1.377 0 0 0 22.131 41.448L22.131 54.116A1.377 1.377 0 0 1 20.754 55.493L19.024 55.493A8.17 8.17 0 0 1 11.948 43.238Z";
/// The rain's line weights, in units, a row of depth each (the menu's).
const WEIGHTS: [f32; 8] = [
    4.0657, 3.2953, 2.7704, 2.3897, 2.101, 1.8746, 1.6922, 1.5421,
];

/// What a frame of a generated background draws.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub look: Look,
    pub palette: Palette,
    pub signals: &'a Signals,
    /// How bright the look is drawn, 0 to 1 (the player's background
    /// brightness against its default).
    pub light: f32,
    /// Draw over the picture given to [`GenerativePipeline::set_picture`]
    /// (at `picture_brightness`), or fold it, for the kaleidoscope.
    pub picture: bool,
    pub picture_brightness: f32,
}

/// `generative.wgsl`'s `Look`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct LookUniform {
    size: [f32; 4],
    flags: [f32; 4],
    c0: [f32; 4],
    c1: [f32; 4],
    c2: [f32; 4],
    c3: [f32; 4],
    tint: [f32; 4],
    fit: [f32; 4],
    kaleido: [f32; 4],
    kaleido2: [f32; 4],
    kaleido3: [f32; 4],
    field: [f32; 4],
    bar_shape: [f32; 4],
    bar_more: [f32; 4],
    bars: [[f32; 4]; BAR_COUNT / 4],
    peaks: [[f32; 4]; BAR_COUNT / 4],
    bar_colors: [[f32; 4]; BAR_COUNT],
}

/// `generative.wgsl`'s `Strokes`, less its shapes (written once, after).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct StrokesHead {
    size: [f32; 4],
    lines: [f32; 4],
    bands: [[f32; 4]; BANDS / 4],
}

/// One stroke: `generative.wgsl`'s `Stroke`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct Stroke {
    place: [f32; 4],
    color: [f32; 4],
    line: [f32; 4],
    kind: [f32; 4],
}

const SHAPES_SIZE: usize = 2 * POINTS * 16;

/// The palette's colors, encoded, 0 to 1.
fn colors(palette: Palette) -> [[f32; 3]; 4] {
    palette.colors().map(|c| c.map(|v| f32::from(v) / 255.0))
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
}

fn with(c: [f32; 3], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

fn scaled(c: [f32; 3], k: f32) -> [f32; 3] {
    c.map(|v| v * k)
}

/// The study's deterministic seed, in f64 as the menu's rain has it
/// (`ddi_theme`'s `rain.rs`), so both shells pick the same.
fn seed(i: f64, salt: f64) -> f64 {
    ((i * 127.1 + salt * 311.7).sin() * 43758.5453).rem_euclid(1.0)
}

/// The kaleidoscope's piece turns a step each beat, eased out.
fn eased(beat: f32) -> f32 {
    let f = beat - beat.floor();
    beat.floor() + (1.0 - (1.0 - f).powi(3))
}

/// The look's uniform for a `size` texture; `picture`: the size of the
/// picture drawn, if one is.
fn look_uniform(
    frame: &Frame,
    size: [f32; 2],
    picture: Option<[u32; 2]>,
    linear: bool,
) -> LookUniform {
    let s = frame.signals;
    let [w, h] = size;
    let cs = colors(frame.palette);
    let light = frame.light.clamp(0.0, 1.0);
    let look = match frame.look {
        Look::Bars => 0.0,
        Look::Kaleidoscope => 1.0,
        Look::Field => 2.0,
        Look::Rain => 3.0,
    };
    let mut u = LookUniform::zeroed();
    u.size = [w, h, h / 540.0, light];
    u.flags = [
        look,
        if picture.is_some() { 1.0 } else { 0.0 },
        if linear { 1.0 } else { 0.0 },
        frame.picture_brightness.clamp(0.0, 1.0),
    ];
    u.c0 = with(cs[0], 1.0);
    u.c1 = with(cs[1], 1.0);
    u.c2 = with(cs[2], 1.0);
    u.c3 = with(cs[3], 1.0);
    // The picture covering the texture.
    if let Some([iw, ih]) = picture {
        let (scale, offset) = crate::background::cover_uv(w, h, iw as f32, ih as f32);
        u.fit = [scale[0], scale[1], offset[0], offset[1]];
    }
    // The kaleidoscope's piece: turned a step each beat, wandering over
    // the picture, kept inside it (a cell's points lie within 0.62 of
    // its middle).
    let k = KALEIDOSCOPE;
    let turn = eased(s.beat) * k.step.to_radians();
    let wander = s.wander;
    let middle = if let Some([iw, ih]) = picture {
        let aspect = iw as f32 / ih.max(1) as f32;
        let m = 0.62 * k.piece;
        let along = |lo: f32, hi: f32, v: f32| {
            if lo < hi {
                lo + (hi - lo) * (0.5 + 0.5 * v)
            } else {
                (lo + hi) / 2.0
            }
        };
        u.kaleido2[2] = aspect;
        [
            along(m, aspect - m, (wander * 1.3).sin()),
            along(m, 1.0 - m, (wander * 0.9).cos()),
        ]
    } else {
        u.kaleido2[2] = 1.0;
        [
            4.0 + 3.0 * (wander * 1.3).sin(),
            4.0 + 3.0 * (wander * 0.9).cos(),
        ]
    };
    u.kaleido = [turn, k.piece, middle[0], middle[1]];
    u.kaleido2[0] = k.segments;
    u.kaleido2[1] = k.radius;
    u.kaleido2[3] = 1.0 - k.darken;
    u.kaleido3 = [k.desaturate, k.tint, 0.0, 0.0];
    let pulse = if s.beat >= 0.0 {
        k.pulse * (-(s.beat - s.beat.floor()) * 5.0).exp()
    } else {
        0.0
    };
    u.tint = with(cs[3], pulse);
    // The field, brighter with the loudness.
    let f = FIELD;
    u.field = [
        f.scale,
        f.contrast,
        f.bright * (1.0 - f.bright_amount) + f.bright * f.bright_amount * 2.0 * s.loudness,
        0.0,
    ];
    // The bars, from the low bands on the left.
    let b = BARS;
    u.bar_shape = [b.height, b.gap, b.segments, b.segment_gap];
    u.bar_more = [b.opacity, BAR_COUNT as f32, 0.0, 0.0];
    for i in 0..BAR_COUNT {
        u.bars[i / 4][i % 4] = s.bars[i];
        u.peaks[i / 4][i % 4] = s.peaks[i];
        let c = mix3(cs[2], cs[3], i as f32 / (BAR_COUNT - 1) as f32);
        u.bar_colors[i] = with(scaled(c, light), 1.0);
    }
    u
}

/// The field's lines: their strokes and the strokes' uniform head.
fn lines(frame: &Frame, size: [f32; 2], out: &mut Vec<Stroke>) -> StrokesHead {
    let s = frame.signals;
    let [w, h] = size;
    let units = h / 540.0;
    let f = FIELD;
    let cs = colors(frame.palette);
    let n = f.lines;
    for l in 0..n {
        let c = mix3(cs[2], cs[3], l as f32 / (n.max(2) - 1) as f32);
        out.push(Stroke {
            place: [0.0, (l + 1) as f32 / (n + 1) as f32 * h, 0.0, 0.0],
            color: with(scaled(c, frame.light), f.line_opacity),
            line: [f.line_width * units, 0.0, 0.0, 0.0],
            kind: [2.0, l as f32, 0.0, 0.0],
        });
    }
    let mut bands = [[0.0; 4]; BANDS / 4];
    for (i, v) in s.lines.iter().enumerate() {
        bands[i / 4][i % 4] = *v;
    }
    StrokesHead {
        size: [w, h, n as f32, f.cycles],
        lines: [s.time * f.line_speed * 2.0, f.amplitude * units, 0.0, 0.0],
        bands,
    }
}

/// The rain's outlines at the frame's second, as the menu's rain falls
/// (`ddi_theme`'s `rain.rs`, as the study draws it over a 960 × 540
/// layout scaled to the texture): a share moving, each chosen as it
/// comes in at the top, zooming with the bass or waving with the treble.
fn rain(frame: &Frame, size: [f32; 2], out: &mut Vec<Stroke>) {
    let s = frame.signals;
    let r = RAIN;
    let [w, h] = size;
    let units = h / 540.0;
    let cs = colors(frame.palette);
    let time = f64::from(s.time) * r.speed;
    for i in 0..r.count {
        let i = i as f64;
        let depth = seed(i, 2.0);
        let rate = 8.0 * (0.8 + depth * 1.2);
        let travel = seed(i, 1.0) * 680.0 + (time + 8.0) * rate;
        let identity = i + (travel / 680.0).floor() * 4099.0;
        let y = -70.0 + travel.rem_euclid(680.0);
        let x = -50.0 + (seed(i, 3.0) * 1060.0 + (time + 8.0) * rate * 0.17).rem_euclid(1060.0);
        let fade = ((y + 70.0) / 80.0).min((610.0 - y) / 80.0).clamp(0.0, 1.0);
        let onigiri = seed(identity, 6.0) < 0.01;
        let column = (seed(identity, 21.0) * 4.0).floor();
        let row = (depth * 7.0).round() as usize;
        // Quieter round the middle, as the menu's.
        let distance = ((x - 480.0).hypot(y - 290.0) - 60.0).max(0.0) / 370.0;
        let quiet = if distance < 0.6 {
            0.58 + (0.23 - 0.58) * distance / 0.6
        } else {
            0.23 * (1.0 - distance).max(0.0) / 0.4
        };
        let alpha = ((0.13 + depth * 0.24) * fade * (1.0 - quiet)) as f32 * r.opacity;
        let alpha = alpha.min(1.0);
        if alpha <= 0.0 {
            continue;
        }
        let moving = seed(identity, 9.0) < r.moving;
        let zooming = moving && seed(identity, 13.0) < r.zooming;
        let zoom = if zooming { 1.0 + r.zoom * s.zoom } else { 1.0 };
        let side = 72.0 * (0.22 + depth as f32 * 0.36) * (64.0 / 56.0) * units * r.size;
        let color = mix3(cs[2], cs[3], depth as f32);
        let amplitude = if moving && !zooming {
            r.amplitude * s.wave
        } else {
            0.0
        };
        let phase = f64::from(s.time)
            * std::f64::consts::TAU
            * f64::from(r.frequency)
            * (0.7 + seed(i, 11.0) * 0.6);
        out.push(Stroke {
            place: [
                (x / 960.0) as f32 * w,
                (y / 540.0) as f32 * h,
                side * zoom / 64.0,
                if onigiri {
                    0.0
                } else {
                    column as f32 * std::f32::consts::FRAC_PI_2
                },
            ],
            color: with(scaled(color, frame.light), alpha),
            // The line keeps its width as the outline zooms.
            line: [
                WEIGHTS[row.min(7)] * side / 64.0,
                amplitude,
                phase.rem_euclid(std::f64::consts::TAU) as f32,
                r.waves,
            ],
            kind: [if onigiri { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        });
    }
}

/// Points evenly along an SVG path's outline (`M`, `L`, `H`, `V`,
/// circular `A` and `Z`, absolute, as the rain's are), with their
/// normals and how far round they are.
fn outline(d: &str) -> [[f32; 4]; POINTS] {
    // The path, flattened finely.
    let mut fine: Vec<[f64; 2]> = Vec::new();
    let mut numbers = Vec::new();
    let mut command = ' ';
    let mut at = [0.0f64; 2];
    let mut start = at;
    let mut text = d.chars().peekable();
    let flush = |command: char,
                 numbers: &mut Vec<f64>,
                 fine: &mut Vec<[f64; 2]>,
                 at: &mut [f64; 2],
                 start: &mut [f64; 2]| {
        match command {
            'M' if numbers.len() >= 2 => {
                *at = [numbers[0], numbers[1]];
                *start = *at;
                fine.push(*at);
            }
            'L' => {
                for p in numbers.as_chunks::<2>().0 {
                    *at = *p;
                    fine.push(*at);
                }
            }
            'H' => {
                for x in numbers.iter() {
                    at[0] = *x;
                    fine.push(*at);
                }
            }
            'V' => {
                for y in numbers.iter() {
                    at[1] = *y;
                    fine.push(*at);
                }
            }
            'A' => {
                for a in numbers.as_chunks::<7>().0 {
                    let to = [a[5], a[6]];
                    arc(*at, to, a[0], a[3] != 0.0, a[4] != 0.0, fine);
                    *at = to;
                }
            }
            'Z' => {
                *at = *start;
                fine.push(*at);
            }
            _ => {}
        }
        numbers.clear();
    };
    let mut number = String::new();
    while let Some(c) = text.next() {
        let ends = |number: &mut String, numbers: &mut Vec<f64>| {
            if !number.is_empty() {
                numbers.push(number.parse().unwrap_or(0.0));
                number.clear();
            }
        };
        if c.is_ascii_alphabetic() {
            ends(&mut number, &mut numbers);
            flush(command, &mut numbers, &mut fine, &mut at, &mut start);
            command = c;
        } else if c == '-' && !number.is_empty() {
            ends(&mut number, &mut numbers);
            number.push(c);
        } else if c.is_ascii_digit() || c == '.' || c == '-' {
            if c == '.' && number.contains('.') {
                ends(&mut number, &mut numbers);
            }
            number.push(c);
        } else {
            ends(&mut number, &mut numbers);
        }
        if text.peek().is_none() {
            ends(&mut number, &mut numbers);
            flush(command, &mut numbers, &mut fine, &mut at, &mut start);
        }
    }
    // Its length along the way, then points at equal steps of it.
    let mut lengths = vec![0.0];
    for p in fine.windows(2) {
        let l = lengths.last().copied().unwrap_or(0.0);
        lengths.push(l + (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]));
    }
    let total = lengths.last().copied().unwrap_or(0.0).max(1e-9);
    let mut points = [[0.0f64; 2]; POINTS];
    let mut seg = 0;
    for (k, p) in points.iter_mut().enumerate() {
        let want = k as f64 / POINTS as f64 * total;
        while seg + 2 < lengths.len() && lengths[seg + 1] < want {
            seg += 1;
        }
        let (a, b) = (fine[seg], fine[(seg + 1).min(fine.len() - 1)]);
        let span = (lengths[seg + 1] - lengths[seg]).max(1e-12);
        let t = ((want - lengths[seg]) / span).clamp(0.0, 1.0);
        *p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    }
    // Normals from the neighbors, as the study's.
    std::array::from_fn(|k| {
        let (a, b) = (points[(k + POINTS - 1) % POINTS], points[(k + 1) % POINTS]);
        let (tx, ty) = (b[0] - a[0], b[1] - a[1]);
        let l = tx.hypot(ty).max(1e-12);
        [
            points[k][0] as f32,
            points[k][1] as f32,
            (ty / l) as f32,
            (-tx / l) as f32,
        ]
    })
}

/// An SVG arc of radius `r` (circular, unrotated) from `from` to `to`,
/// flattened into `out` (its end included).
fn arc(from: [f64; 2], to: [f64; 2], r: f64, large: bool, sweep: bool, out: &mut Vec<[f64; 2]>) {
    let (x1, y1) = ((from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0);
    let mut r = r.abs().max(1e-9);
    let lambda = (x1 * x1 + y1 * y1) / (r * r);
    if lambda > 1.0 {
        r *= lambda.sqrt();
    }
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let num = (r * r * r * r - r * r * y1 * y1 - r * r * x1 * x1).max(0.0);
    let den = (r * r * y1 * y1 + r * r * x1 * x1).max(1e-18);
    let coef = sign * (num / den).sqrt();
    let (cx1, cy1) = (coef * y1, -coef * x1);
    let center = [cx1 + (from[0] + to[0]) / 2.0, cy1 + (from[1] + to[1]) / 2.0];
    let start = ((y1 - cy1) / r).atan2((x1 - cx1) / r);
    let mut delta = ((-y1 - cy1) / r).atan2((-x1 - cx1) / r) - start;
    let tau = std::f64::consts::TAU;
    if !sweep && delta > 0.0 {
        delta -= tau;
    } else if sweep && delta < 0.0 {
        delta += tau;
    }
    let steps = ((delta.abs() * r).ceil() as usize * 4).max(8);
    for k in 1..=steps {
        let a = start + delta * k as f64 / steps as f64;
        out.push([center[0] + r * a.cos(), center[1] + r * a.sin()]);
    }
}

/// The generated backgrounds' pipelines, and what they draw with.
pub struct GenerativePipeline {
    look: wgpu::RenderPipeline,
    stroke: wgpu::RenderPipeline,
    look_layout: wgpu::BindGroupLayout,
    look_buffer: wgpu::Buffer,
    strokes_buffer: wgpu::Buffer,
    strokes_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    sampler: wgpu::Sampler,
    group: wgpu::BindGroup,
    /// The picture bound: its size, and whether it samples as linear
    /// light.
    picture: Option<([u32; 2], bool)>,
    strokes: Vec<Stroke>,
}

impl GenerativePipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> GenerativePipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-generative"),
            source: wgpu::ShaderSource::Wgsl(include_str!("generative.wgsl").into()),
        });
        let look_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-generative-look"),
            size: std::mem::size_of::<LookUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let look_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-generative-look-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let look_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-generative-look-layout"),
            bind_group_layouts: &[Some(&look_layout)],
            immediate_size: 0,
        });
        let look = crate::gpu::fullscreen_pipeline(
            device,
            "ddi-generative-look-pipeline",
            &look_pipeline_layout,
            &shader,
            "fs_look",
            FORMAT,
            None,
        );
        let strokes_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-generative-strokes"),
            size: (std::mem::size_of::<StrokesHead>() + SHAPES_SIZE) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shapes: Vec<[f32; 4]> = outline(ARROW).into_iter().chain(outline(ONIGIRI)).collect();
        queue.write_buffer(
            &strokes_buffer,
            std::mem::size_of::<StrokesHead>() as u64,
            bytemuck::cast_slice(&shapes),
        );
        let strokes_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-generative-strokes-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let strokes_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ddi-generative-strokes-bg"),
            layout: &strokes_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 3,
                resource: strokes_buffer.as_entire_binding(),
            }],
        });
        let stroke_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-generative-stroke-layout"),
            bind_group_layouts: &[Some(&strokes_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x4,
            1 => Float32x4,
            2 => Float32x4,
            3 => Float32x4,
        ];
        let stroke = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ddi-generative-stroke-pipeline"),
            layout: Some(&stroke_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_stroke"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Stroke>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_stroke"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-generative-instances"),
            size: (MAX_STROKES * std::mem::size_of::<Stroke>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ddi-generative-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blank = crate::gpu::texture_2d(
            device,
            "ddi-generative-blank",
            [1, 1],
            FORMAT,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING,
            &[],
        )
        .create_view(&Default::default());
        let group = Self::look_group(device, &look_layout, &look_buffer, &blank, &sampler);
        GenerativePipeline {
            look,
            stroke,
            look_layout,
            look_buffer,
            strokes_buffer,
            strokes_group,
            instances,
            sampler,
            group,
            picture: None,
            strokes: Vec::with_capacity(MAX_STROKES),
        }
    }

    fn look_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        crate::gpu::uniform_texture_group(
            device,
            "ddi-generative-look-bg",
            layout,
            buffer.as_entire_binding(),
            view,
            sampler,
        )
    }

    /// The picture drawn under the look, or folded by the kaleidoscope
    /// (`picture`: its view, size and whether it samples as linear light,
    /// [`crate::Renderer::background_picture`]); `None`: none.
    pub fn set_picture(
        &mut self,
        device: &wgpu::Device,
        picture: Option<(&wgpu::TextureView, [u32; 2], bool)>,
    ) {
        match picture {
            Some((view, size, linear)) => {
                self.group = Self::look_group(
                    device,
                    &self.look_layout,
                    &self.look_buffer,
                    view,
                    &self.sampler,
                );
                self.picture = Some((size, linear));
            }
            None if self.picture.is_some() => {
                let blank = crate::gpu::texture_2d(
                    device,
                    "ddi-generative-blank",
                    [1, 1],
                    FORMAT,
                    1,
                    wgpu::TextureUsages::TEXTURE_BINDING,
                    &[],
                )
                .create_view(&Default::default());
                self.group = Self::look_group(
                    device,
                    &self.look_layout,
                    &self.look_buffer,
                    &blank,
                    &self.sampler,
                );
                self.picture = None;
            }
            None => {}
        }
    }

    /// Whether a picture is bound.
    pub fn has_picture(&self) -> bool {
        self.picture.is_some()
    }

    /// Draws `frame` into `target` (a [`FORMAT`] texture of `size`
    /// pixels).
    pub fn draw(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        frame: &Frame,
    ) {
        let size_f = [size[0].max(1) as f32, size[1].max(1) as f32];
        let picture = self.picture.filter(|_| frame.picture);
        let uniform = look_uniform(
            frame,
            size_f,
            picture.map(|p| p.0),
            picture.is_some_and(|p| p.1),
        );
        queue.write_buffer(&self.look_buffer, 0, bytemuck::bytes_of(&uniform));
        self.strokes.clear();
        let mut head = StrokesHead::zeroed();
        head.size = [size_f[0], size_f[1], 0.0, 0.0];
        let vertices = match frame.look {
            Look::Field => {
                head = lines(frame, size_f, &mut self.strokes);
                2 * (LINE_STEPS + 1)
            }
            Look::Rain => {
                rain(frame, size_f, &mut self.strokes);
                2 * (POINTS as u32 + 1)
            }
            Look::Bars | Look::Kaleidoscope => 0,
        };
        self.strokes.truncate(MAX_STROKES);
        if !self.strokes.is_empty() {
            queue.write_buffer(&self.strokes_buffer, 0, bytemuck::bytes_of(&head));
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&self.strokes));
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ddi-generative-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.look);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
        if !self.strokes.is_empty() {
            pass.set_pipeline(&self.stroke);
            pass.set_bind_group(0, &self.strokes_group, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            pass.draw(0..vertices, 0..self.strokes.len() as u32);
        }
    }
}

impl GenerativePipeline {
    /// Draws `frame` into the first level of `target` (a [`FORMAT`]
    /// texture of mipmaps, made for the 2D pass:
    /// `ddi_shell::generative::Stills`), submitted at once.
    pub fn draw_still(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::Texture,
        frame: &Frame,
    ) {
        let size = [target.width().max(1), target.height().max(1)];
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            base_mip_level: 0,
            mip_level_count: Some(1),
            ..Default::default()
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ddi-generative-still"),
        });
        self.draw(queue, &mut encoder, &view, size, frame);
        // Its own submission: the pass's uniforms are written for each.
        queue.submit(Some(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame<'a>(look: Look, signals: &'a Signals) -> Frame<'a> {
        Frame {
            look,
            palette: Palette::Ink,
            signals,
            light: 1.0,
            picture: false,
            picture_brightness: 0.4,
        }
    }

    #[test]
    fn the_shader_validates_and_writes_for_webgl2_with_its_uniforms_sizes() {
        let module = naga::front::wgsl::parse_str(include_str!("generative.wgsl")).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let span = |name: &str| {
            let (_, t) = module
                .types
                .iter()
                .find(|(_, t)| t.name.as_deref() == Some(name))
                .unwrap();
            let naga::TypeInner::Struct { span, .. } = t.inner else {
                panic!("{name} is not a struct")
            };
            span as usize
        };
        assert_eq!(span("Look"), std::mem::size_of::<LookUniform>());
        assert_eq!(
            span("Strokes"),
            std::mem::size_of::<StrokesHead>() + SHAPES_SIZE
        );
        // A uniform block WebGL2 always allows.
        assert!(span("Strokes") <= 16 * 1024);
        for (stage, entry) in [
            (naga::ShaderStage::Vertex, "vs_main"),
            (naga::ShaderStage::Fragment, "fs_look"),
            (naga::ShaderStage::Vertex, "vs_stroke"),
            (naga::ShaderStage::Fragment, "fs_stroke"),
        ] {
            let mut output = String::new();
            let options = naga::back::glsl::Options {
                version: naga::back::glsl::Version::Embedded {
                    version: 300,
                    is_webgl: true,
                },
                ..Default::default()
            };
            let pipeline = naga::back::glsl::PipelineOptions {
                shader_stage: stage,
                entry_point: entry.into(),
                multiview: None,
            };
            naga::back::glsl::Writer::new(
                &mut output,
                &module,
                &info,
                &options,
                &pipeline,
                Default::default(),
            )
            .unwrap()
            .write()
            .unwrap();
        }
    }

    #[test]
    fn the_outlines_go_evenly_round_their_shapes() {
        let arrow = outline(ARROW);
        // Its tip at the top, and every point in its square.
        assert!(
            arrow
                .iter()
                .any(|p| (p[0] - 32.0).abs() < 0.5 && (p[1] - 4.0).abs() < 0.5)
        );
        for p in arrow.iter().chain(&outline(ONIGIRI)) {
            assert!(
                (0.0..=64.0).contains(&p[0]) && (0.0..=64.0).contains(&p[1]),
                "{p:?}"
            );
            assert!(((p[2].hypot(p[3])) - 1.0).abs() < 1e-3);
        }
        // Even steps round it (none of zero length, none long).
        let steps: Vec<f32> = (0..POINTS)
            .map(|k| {
                let (a, b) = (arrow[k], arrow[(k + 1) % POINTS]);
                (b[0] - a[0]).hypot(b[1] - a[1])
            })
            .collect();
        let (lo, hi) = steps
            .iter()
            .fold((f32::MAX, 0.0f32), |(lo, hi), s| (lo.min(*s), hi.max(*s)));
        assert!(lo > 0.5 && hi < 1.6, "{lo} {hi}");
        // The onigiri's rounded bottom corners reach below its middle.
        assert!(outline(ONIGIRI).iter().any(|p| p[1] > 55.0));
    }

    #[test]
    fn the_arcs_bend_the_way_their_flags_say() {
        let mut out = Vec::new();
        arc([0.0, 0.0], [2.0, 0.0], 1.0, false, true, &mut out);
        // A half circle, through (1, -1) with y down and the sweep set.
        assert!(
            out.iter()
                .any(|p| (p[0] - 1.0).abs() < 0.05 && (p[1] + 1.0).abs() < 0.05),
            "{out:?}"
        );
        assert!((out.last().unwrap()[0] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn the_rains_outlines_move_as_chosen_where_they_come_in() {
        let mut quiet = Signals::beat_only(30.0, 60.0);
        let mut loud = quiet;
        loud.zoom = 1.0;
        loud.wave = 1.0;
        let (mut a, mut b) = (Vec::new(), Vec::new());
        rain(&frame(Look::Rain, &quiet), [960.0, 540.0], &mut a);
        rain(&frame(Look::Rain, &loud), [960.0, 540.0], &mut b);
        assert_eq!(a.len(), b.len());
        assert!(a.len() > 30, "{}", a.len());
        let zoomed = a
            .iter()
            .zip(&b)
            .filter(|(a, b)| b.place[2] > a.place[2] * 1.2)
            .count();
        let waved = b.iter().filter(|s| s.line[1] > 0.0).count();
        assert!(zoomed > 0 && waved > 0, "{zoomed} {waved}");
        // A share of them, not all.
        assert!(zoomed + waved < a.len() / 2, "{zoomed} {waved}");
        // Zooming keeps the line's width; nothing waves when quiet.
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(a.line[0], b.line[0]);
            assert_eq!(a.line[1], 0.0);
        }
        quiet.time = 31.0;
        let mut later = Vec::new();
        rain(&frame(Look::Rain, &quiet), [960.0, 540.0], &mut later);
        assert_ne!(a, later);
    }

    #[test]
    fn the_looks_uniform_follows_the_music_and_the_palette() {
        let mut s = Signals::beat_only(10.0, 4.5);
        s.bars[0] = 0.8;
        s.peaks[23] = 0.5;
        s.loudness = 1.0;
        let u = look_uniform(&frame(Look::Bars, &s), [1920.0, 1080.0], None, false);
        assert_eq!(u.size, [1920.0, 1080.0, 2.0, 1.0]);
        assert_eq!((u.bars[0][0], u.peaks[5][3]), (0.8, 0.5));
        // Ink's middle on the left, its high on the right.
        let ink = colors(Palette::Ink);
        assert_eq!(u.bar_colors[0][..3], ink[2]);
        assert_eq!(u.bar_colors[23][..3], ink[3]);
        // Half a beat on: half the step turned, eased, and the pulse gone.
        assert!((u.kaleido[0] - (4.0 + 1.0 - 0.125) * 15f32.to_radians()).abs() < 1e-5);
        assert!(u.tint[3] < 0.12 * 0.1);
        // The field at its loudest, 0.35 × (0.65 + 0.7).
        assert!((u.field[2] - 0.35 * 1.35).abs() < 1e-6);
        // Dimmer with less light, the ground too.
        let mut f = frame(Look::Bars, &s);
        f.light = 0.5;
        let dim = look_uniform(&f, [1920.0, 1080.0], None, false);
        assert_eq!(dim.size[3], 0.5);
        assert_eq!(dim.bar_colors[0][0], ink[2][0] * 0.5);
        // A picture: covered, and the kaleidoscope's piece kept on it.
        let pic = look_uniform(
            &frame(Look::Kaleidoscope, &s),
            [1920.0, 1080.0],
            Some([640, 480]),
            true,
        );
        assert_eq!(pic.flags[1..3], [1.0, 1.0]);
        let m = 0.62 * KALEIDOSCOPE.piece;
        assert!(pic.kaleido[2] >= m && pic.kaleido[2] <= 640.0 / 480.0 - m);
        assert!(pic.kaleido[3] >= m && pic.kaleido[3] <= 1.0 - m);
    }

    #[test]
    fn the_lines_wave_with_their_bands() {
        let mut s = Signals::beat_only(2.0, 0.0);
        s.lines = [0.5; BANDS];
        let mut out = Vec::new();
        let head = lines(&frame(Look::Field, &s), [960.0, 540.0], &mut out);
        assert_eq!(out.len(), FIELD.lines);
        assert_eq!(head.size, [960.0, 540.0, 10.0, FIELD.cycles]);
        assert_eq!(head.bands[7][3], 0.5);
        assert!((out[0].place[1] - 540.0 / 11.0).abs() < 1e-4);
        assert_eq!(out[0].color[3], FIELD.line_opacity);
    }
}
