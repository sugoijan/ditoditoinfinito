//! The accepted play HUD, shared by both shells. All state is presentation
//! only; the frame remains the authority for life, danger and combo.
use bytemuck::{Pod, Zeroable};
use ddi_chart::Layout;
use ddi_engine::frame::Frame;

use crate::scene::{FieldGeometry, RenderOptions, field_span};

const SHADER: &str = include_str!("hud.wgsl");

/// The life gauge's width in lanes: Single's field, which 22% of a
/// 1280×720 screen was (the study's width) within a few pixels.
const LIFE_LANES: f32 = 4.0;

/// How much of the gauge a share of life fills: less than its share below
/// full, so that being short of full looks it (half life fills 30%, a loss
/// from full shows 1.8 times its size) while low life stays readable (a
/// tenth still fills about 3%). Drawing only: the warning, the rules and
/// every number keep the true life.
fn shown(life: f32) -> f32 {
    const FIRMNESS: f32 = 0.8;
    let life = life.clamp(0.0, 1.0);
    life * (1.0 - FIRMNESS + FIRMNESS * life)
}

pub(crate) fn scale(geo: &FieldGeometry) -> f32 {
    geo.arrow / 72.0
}

/// Rectangles are left, top, width, height, in target pixels.
///
/// The life gauge is centered over the field, `span` lanes wide (each
/// player's over theirs, where a side's field is a player's), four lanes
/// wide whatever the layout: it scales with the notes, never the screen.
pub(crate) fn life_rect(geo: &FieldGeometry, span: f32) -> [f32; 4] {
    let s = scale(geo);
    let w = geo.arrow * LIFE_LANES;
    [geo.center_x(span) - w / 2.0, 22.0 * s, w, 8.0 * s]
}

pub(crate) fn mark_rect(geo: &FieldGeometry) -> [f32; 4] {
    let s = scale(geo);
    let h = 170.0 * 176.0 / 974.0;
    [24.0 * s, geo.height - (22.0 + h) * s, 170.0 * s, h * s]
}

/// Where a play's frame shows what the announcer speaks of, each
/// `[left, top, right, bottom]` as shares of the frame (the announcer
/// studio rings them over its picture of the play).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HudSpots {
    pub life: [f32; 4],
    pub combo: [f32; 4],
    pub receptors: [f32; 4],
}

/// Where a play at `combo` shows them on a field of `geo` (the combo at its resting
/// size, at least three digits wide: its place, shown or not, which keeps
/// still as a combo grows or breaks).
pub(crate) fn spots(combo: u32, layout: &Layout, geo: &FieldGeometry) -> HudSpots {
    let share = |[l, t, r, b]: [f32; 4]| {
        [
            l / geo.width.max(1.0),
            t / geo.height.max(1.0),
            r / geo.width.max(1.0),
            b / geo.height.max(1.0),
        ]
    };
    let span = field_span(layout);
    let s = scale(geo);
    let [x, y, w, h] = life_rect(geo, span);
    let life = [x, y, x + w, y + h];
    let center = geo.center_x(span);
    let tier = (combo / 100).min(10);
    let count = ((combo.checked_ilog10().unwrap_or(0) + 1) as f32).max(3.0);
    let cs = s * if tier == 10 { 1.2 } else { 1.0 };
    let cs = cs.min((geo.width - 16.0 * s).max(1.0) / (count * 32.0 + 32.0));
    let half = (count * 32.0 + 32.0) * cs / 2.0;
    let top = geo.height / 2.0 - 16.0 * cs;
    let place = [center - half, top, center + half, top + 96.0 * cs];
    let receptors = crate::scene::receptor_rows(geo, layout)
        .reduce(ddi_actor::rect::union)
        .unwrap_or_default();
    HudSpots {
        life: share(life),
        combo: share(place),
        receptors: share(receptors),
    }
}

#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    to: f32,
    at: f64,
}

fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

#[derive(Debug)]
struct Motion {
    at: f64,
    song: f64,
    life: f32,
    fill: f32,
    trail: f32,
    trail_alpha: f32,
    damage: Option<Tween>,
    recovery: Option<Tween>,
    full: f32,
    evolution: f64,
    phase: f64,
    combo: u32,
    pulse_at: f64,
}

impl Motion {
    fn new(now: f64, song: f64, life: f32, combo: u32) -> Self {
        Self {
            at: now,
            song,
            life,
            fill: life,
            trail: life,
            trail_alpha: 0.0,
            damage: None,
            recovery: None,
            full: if life >= 1.0 { 1.0 } else { 0.0 },
            evolution: 4.0,
            phase: 0.626,
            combo,
            pulse_at: f64::NEG_INFINITY,
        }
    }

    fn sample(&mut self, now: f64, reduced: bool) {
        if let Some(t) = self.recovery {
            let progress = ((now - t.at) / 0.130) as f32;
            self.fill = t.from + (t.to - t.from) * ease(progress);
            if progress >= 1.0 || reduced {
                self.fill = self.life;
                self.recovery = None;
            }
        }
        self.trail_alpha = 0.0;
        if let Some(t) = self.damage {
            let progress = ((now - t.at - 0.080) / 0.380) as f32;
            if reduced {
                self.trail = t.from;
                self.trail_alpha = (1.0 - (now - t.at) as f32 / 0.200).clamp(0.0, 1.0);
            } else {
                self.trail = t.from + (t.to - t.from) * ease(progress);
                self.trail_alpha = 1.0;
            }
            if (reduced && self.trail_alpha == 0.0) || (!reduced && progress >= 1.0) {
                self.damage = None;
                self.trail_alpha = 0.0;
            }
        }
        if self.damage.is_none() {
            self.trail = self.fill;
        }
    }

    fn update(&mut self, now: f64, song: f64, life: f32, combo: u32, reduced: bool) {
        // A seek or another session must never animate damage from the old play.
        if now < self.at || song < self.song - 0.001 {
            *self = Self::new(now, song, life, combo);
        }
        self.sample(now, reduced);
        let old = self.life;
        self.life = life;
        if life < old {
            let from = old.max(self.trail).max(self.fill);
            self.damage = Some(Tween {
                from,
                to: life,
                at: now,
            });
            self.trail = from;
            self.trail_alpha = 1.0;
            self.fill = life;
            self.recovery = None;
            self.full = 0.0;
        } else if life > old {
            if reduced {
                self.fill = life;
                self.recovery = None;
            } else {
                self.recovery = Some(Tween {
                    from: self.fill,
                    to: life,
                    at: now,
                });
            }
        }
        if combo / 100 > self.combo / 100 {
            self.pulse_at = now;
        }
        if combo < self.combo || reduced {
            self.pulse_at = f64::NEG_INFINITY;
        }
        self.combo = combo;
        let dt = (now - self.at).max(0.0);
        let full = life >= 1.0 && self.recovery.is_none();
        self.full = if !full {
            0.0
        } else if reduced {
            1.0
        } else {
            (self.full + dt as f32 / 0.240).min(1.0)
        };
        if !reduced {
            self.evolution += dt.min(0.1) * (1.0 + 1.5 * f64::from(self.full));
            self.phase = (self.phase - dt.min(0.1) / 5.0).rem_euclid(1.0);
        }
        self.at = now;
        self.song = song;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    screen: [f32; 4], // width, height, linear, visible
    bar: [f32; 4],
    combo_rect: [f32; 4],
    mark: [f32; 4],
    life: [f32; 4],   // fill, true life, trail, trail alpha
    motion: [f32; 4], // evolution, phase, full mix, danger
    combo: [f32; 4],  // count, tier, scale, outline width
    digits: [[f32; 4]; 3],
}

pub(crate) struct Hud {
    pipeline: wgpu::RenderPipeline,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
    atlas: wgpu::BindGroup,
    linear: bool,
    motion: Option<Motion>,
    visible: bool,
    minis: Minis,
}

/// The least a miniature's combo is scaled (the play's is 1 at a 72 px
/// lane): smaller, its digits stop reading, so a small miniature's combo
/// is larger than its lanes would make it.
pub const MINI_COMBO_MIN_SCALE: f32 = 0.3;

/// The miniatures' HUDs (`crate::scene::Miniature`): each one's life gauge
/// and combo drawn by the play's own shader, with its own motion (damage
/// and recovery trails, the pulse), each from its own place in one
/// uniform buffer.
struct Minis {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::PipelineLayout,
    bgl: wgpu::BindGroupLayout,
    /// The buffer, its group and how many it holds.
    buffer: Option<(wgpu::Buffer, wgpu::BindGroup, usize)>,
    stride: u64,
    motions: Vec<Motion>,
    count: usize,
}

impl Hud {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        linear: bool,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-play-hud"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let (buffer, bgl, group) = crate::gpu::uniform_group(
            device,
            "ddi-play-hud",
            std::mem::size_of::<Globals>(),
            wgpu::ShaderStages::VERTEX_FRAGMENT,
        );
        let texture_layout = crate::gpu::texture_sampler_layout(device, "ddi-hud-atlas");
        let texture = crate::gpu::texture_2d(
            device,
            "ddi-hud-atlas",
            [640, 352],
            wgpu::TextureFormat::R8Unorm,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            &[],
        );
        queue.write_texture(
            texture.as_image_copy(),
            include_bytes!("../../../assets/ui/play-hud.r8"),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(640),
                rows_per_image: Some(352),
            },
            texture.size(),
        );
        let sampler = crate::gpu::linear_sampler(device, "ddi-hud-atlas");
        let atlas = crate::gpu::texture_sampler_group(
            device,
            "ddi-hud-atlas",
            &texture_layout,
            &texture.create_view(&Default::default()),
            &sampler,
        );
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-play-hud"),
            bind_group_layouts: &[Some(&bgl), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = Self::pipeline(device, &shader, &layout, format);
        let mini_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-play-hud-minis-bgl"),
            entries: &[crate::gpu::uniform_entry(
                0,
                wgpu::ShaderStages::VERTEX_FRAGMENT,
                true,
            )],
        });
        let mini_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-play-hud-minis"),
            bind_group_layouts: &[Some(&mini_bgl), Some(&texture_layout)],
            immediate_size: 0,
        });
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
        let size = std::mem::size_of::<Globals>() as u64;
        let minis = Minis {
            pipeline: Self::pipeline(device, &shader, &mini_layout, format),
            layout: mini_layout,
            bgl: mini_bgl,
            buffer: None,
            stride: size.div_ceil(align) * align,
            motions: Vec::new(),
            count: 0,
        };
        Self {
            pipeline,
            shader,
            layout,
            buffer,
            group,
            atlas,
            linear,
            motion: None,
            visible: false,
            minis,
        }
    }

    fn pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        crate::gpu::fullscreen_pipeline(
            device,
            "ddi-play-hud",
            layout,
            shader,
            "fs_main",
            format,
            Some(wgpu::BlendState::ALPHA_BLENDING),
        )
    }

    pub fn retarget(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.pipeline = Self::pipeline(device, &self.shader, &self.layout, format);
        self.minis.pipeline = Self::pipeline(device, &self.shader, &self.minis.layout, format);
    }

    pub fn reset(&mut self) {
        self.motion = None;
        self.minis.motions.clear();
    }

    /// The miniatures' life gauges and combos for this frame, on a
    /// `screen` (`[width, height]`) target, their fields `span` lanes wide:
    /// each placed on its field as [`life_rect`] and the play's combo are
    /// on the play's field (the gauge from its field's top, its lanes'
    /// width; the combo in its middle), scaled by its lane, the combo no
    /// smaller than [`MINI_COMBO_MIN_SCALE`].
    pub fn prepare_minis(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        minis: &[crate::scene::Miniature],
        screen: [f32; 2],
        span: f32,
        opts: &RenderOptions,
    ) {
        let n = minis.len();
        self.minis.count = if self.visible { n } else { 0 };
        if self.minis.count == 0 {
            return;
        }
        let stride = self.minis.stride;
        if self.minis.buffer.as_ref().is_none_or(|(_, _, c)| *c < n) {
            let capacity = n.next_power_of_two().max(8);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ddi-play-hud-minis"),
                size: stride * capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ddi-play-hud-minis-bg"),
                layout: &self.minis.bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(std::mem::size_of::<Globals>() as u64),
                    }),
                }],
            });
            self.minis.buffer = Some((buffer, group, capacity));
        }
        let mut bytes = vec![0u8; stride as usize * n];
        for (i, m) in minis.iter().enumerate() {
            let (geo, frame) = (&m.geometry, &m.frame);
            let now = opts.hud_time.unwrap_or(frame.song_time);
            let life = frame.life.clamp(0.0, 1.0);
            let combo = if m.out { 0 } else { frame.combo };
            if self.minis.motions.len() <= i {
                self.minis
                    .motions
                    .push(Motion::new(now, frame.song_time, life, combo));
            }
            let motion = &mut self.minis.motions[i];
            motion.update(now, frame.song_time, life, combo, opts.reduced_motion);
            let s = scale(geo);
            let [top, bottom] = [m.panel[1], m.panel[3]];
            let center = geo.center_x(span);
            let w = geo.arrow * LIFE_LANES;
            let tier = (combo / 100).min(10);
            let pulse =
                1.0 + 0.07 * (1.0 - ((now - motion.pulse_at) / 0.280).clamp(0.0, 1.0)) as f32;
            let count = combo.checked_ilog10().unwrap_or(0) + 1;
            let mut digits = [[0.0; 4]; 3];
            let mut value = combo;
            for d in (0..count as usize).rev() {
                digits[d / 4][d % 4] = (value % 10) as f32;
                value /= 10;
            }
            let count = count as f32;
            let cs = s.max(MINI_COMBO_MIN_SCALE) * if tier == 10 { 1.2 } else { 1.0 } * pulse;
            let width = (count * 32.0 + 32.0) * cs;
            let globals = Globals {
                screen: [screen[0], screen[1], f32::from(self.linear), 1.0],
                bar: [center - w / 2.0, top + 22.0 * s, w, 8.0 * s],
                combo_rect: [
                    center - width / 2.0,
                    (top + bottom) / 2.0 - 16.0 * cs,
                    width,
                    96.0 * cs,
                ],
                mark: [0.0; 4],
                life: [
                    shown(motion.fill),
                    shown(life),
                    shown(motion.trail),
                    motion.trail_alpha,
                ],
                motion: [
                    if opts.reduced_motion {
                        4.0
                    } else {
                        motion.evolution as f32
                    },
                    if opts.reduced_motion {
                        0.626
                    } else {
                        motion.phase as f32
                    },
                    motion.full,
                    f32::from(frame.danger || life <= 0.25),
                ],
                combo: [
                    if combo >= 4 { count } else { 0.0 },
                    tier as f32,
                    cs,
                    (3.5 + tier.saturating_sub(1).min(8) as f32 * 0.08)
                        * if tier == 10 { 1.2 } else { 1.0 },
                ],
                digits,
            };
            let at = i * stride as usize;
            let g = bytemuck::bytes_of(&globals);
            bytes[at..at + g.len()].copy_from_slice(g);
        }
        self.minis.motions.truncate(n);
        if let Some((buffer, _, _)) = &self.minis.buffer {
            queue.write_buffer(buffer, 0, &bytes);
        }
    }

    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        opts: RenderOptions,
        quit: Option<[f32; 3]>,
    ) {
        self.visible = opts.preview.is_none();
        if !self.visible {
            self.reset();
            return;
        }
        let now = opts.hud_time.unwrap_or(frame.song_time);
        let life = frame.life.clamp(0.0, 1.0);
        let m = self
            .motion
            .get_or_insert_with(|| Motion::new(now, frame.song_time, life, frame.combo));
        m.update(now, frame.song_time, life, frame.combo, opts.reduced_motion);
        let tier = (frame.combo / 100).min(10);
        let s = scale(geo);
        let pulse = 1.0 + 0.07 * (1.0 - ((now - m.pulse_at) / 0.280).clamp(0.0, 1.0)) as f32;
        let count = frame.combo.checked_ilog10().unwrap_or(0) + 1;
        let mut digits = [[0.0; 4]; 3];
        let mut value = frame.combo;
        for i in (0..count as usize).rev() {
            digits[i / 4][i % 4] = (value % 10) as f32;
            value /= 10;
        }
        let count = count as f32;
        let cs = s * if tier == 10 { 1.2 } else { 1.0 } * pulse;
        let cs = cs.min((geo.width - 16.0 * s).max(1.0) / (count * 32.0 + 32.0));
        let width = (count * 32.0 + 32.0) * cs;
        let mut mark = mark_rect(geo);
        let overlaps = |r: [f32; 4]| {
            mark[0] < r[2] && mark[0] + mark[2] > r[0] && mark[1] < r[3] && mark[1] + mark[3] > r[1]
        };
        if crate::scene::receptor_rows(geo, layout).any(overlaps)
            || quit.is_some_and(|[x, y, r]| overlaps([x - r, y - r, x + r, y + r]))
        {
            mark[2] = 0.0;
        }
        let globals = Globals {
            screen: [geo.width, geo.height, f32::from(self.linear), 1.0],
            bar: life_rect(geo, field_span(layout)),
            combo_rect: [
                geo.center_x(field_span(layout)) - width / 2.0,
                geo.height / 2.0 - 16.0 * cs,
                width,
                96.0 * cs,
            ],
            mark,
            life: [shown(m.fill), shown(life), shown(m.trail), m.trail_alpha],
            motion: [
                if opts.reduced_motion {
                    4.0
                } else {
                    m.evolution as f32
                },
                if opts.reduced_motion {
                    0.626
                } else {
                    m.phase as f32
                },
                m.full,
                // The selected 25% warning, or an earlier ruleset warning.
                f32::from(frame.danger || life <= 0.25),
            ],
            combo: [
                if frame.combo >= 4 { count } else { 0.0 },
                tier as f32,
                cs,
                (3.5 + tier.saturating_sub(1).min(8) as f32 * 0.08)
                    * if tier == 10 { 1.2 } else { 1.0 },
            ],
            digits,
        };
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&globals));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if !self.visible {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_bind_group(1, &self.atlas, &[]);
        pass.draw(0..6, 0..3);
        if let (n @ 1.., Some((_, group, _))) = (self.minis.count, &self.minis.buffer) {
            pass.set_pipeline(&self.minis.pipeline);
            pass.set_bind_group(1, &self.atlas, &[]);
            for i in 0..n {
                let offset = (i as u64 * self.minis.stride) as u32;
                pass.set_bind_group(0, group, &[offset]);
                // The gauge and the combo; no watermark.
                pass.draw(0..6, 0..2);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_compiles_for_webgl2() {
        let module = naga::front::wgsl::parse_str(SHADER).expect("HUD WGSL");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("HUD validation");
        for (shader_stage, entry) in [
            (naga::ShaderStage::Vertex, "vs_main"),
            (naga::ShaderStage::Fragment, "fs_main"),
        ] {
            let options = naga::back::glsl::Options {
                version: naga::back::glsl::Version::Embedded {
                    version: 300,
                    is_webgl: true,
                },
                ..Default::default()
            };
            let pipeline = naga::back::glsl::PipelineOptions {
                shader_stage,
                entry_point: entry.into(),
                multiview: None,
            };
            let mut glsl = String::new();
            naga::back::glsl::Writer::new(
                &mut glsl,
                &module,
                &info,
                &options,
                &pipeline,
                naga::proc::BoundsCheckPolicies::default(),
            )
            .expect("GLSL writer")
            .write()
            .expect("GLSL ES 3.0 HUD");
        }
    }

    #[test]
    fn the_gauge_fills_less_than_lifes_share_below_full() {
        assert_eq!(shown(0.0), 0.0);
        assert_eq!(shown(1.0), 1.0);
        assert!((shown(0.5) - 0.3).abs() < 1e-6);
        assert!((shown(0.1) - 0.028).abs() < 1e-6);
        // Ordered as life is, so the trails keep their places around it.
        let mut last = 0.0;
        for i in 1..=1000 {
            let s = shown(i as f32 / 1000.0);
            assert!(s > last && s <= i as f32 / 1000.0);
            last = s;
        }
        // A loss from full shows larger than it is.
        assert!(1.0 - shown(0.95) > 0.05 * 1.7);
    }

    #[test]
    fn the_spots_are_where_the_hud_draws_as_shares_of_the_frame() {
        let layout = ddi_chart::Layout::dance_single();
        let geo = FieldGeometry::new(1280.0, 720.0, field_span(&layout), false);
        let s = spots(70, &layout, &geo);
        let [x, y, w, h] = life_rect(&geo, field_span(&layout));
        let want = [x / 1280.0, y / 720.0, (x + w) / 1280.0, (y + h) / 720.0];
        assert_eq!(s.life, want);
        // Centered on the field, below the middle's top.
        assert!(((s.combo[0] + s.combo[2]) / 2.0 - 0.5).abs() < 1e-4);
        assert!(s.combo[1] < 0.5 && s.combo[3] > 0.5);
        // Its place stays as the combo breaks, and widens past 999.
        assert_eq!(spots(0, &layout, &geo).combo, s.combo);
        let wide = spots(1000, &layout, &geo).combo;
        assert!(wide[2] - wide[0] > s.combo[2] - s.combo[0]);
        // The receptors, near the top, over the field.
        assert!(s.receptors[1] < 0.3 && s.receptors[3] < 0.5);
        assert!(s.receptors[0] < s.life[0] && s.receptors[2] > s.life[2]);
        // Reversed: near the bottom.
        let reversed = FieldGeometry::new(1280.0, 720.0, field_span(&layout), true);
        assert!(spots(70, &layout, &reversed).receptors[1] > 0.5);
    }

    #[test]
    fn interrupted_damage_and_recovery_keep_the_true_life_boundary() {
        let mut m = Motion::new(0.0, 0.0, 1.0, 99);
        m.update(0.01, 0.01, 0.5, 0, false);
        assert_eq!(m.fill, 0.5);
        assert_eq!(m.trail, 1.0);
        assert_eq!(m.full, 0.0);
        m.update(0.04, 0.04, 0.7, 4, false);
        assert_eq!(m.fill, 0.5);
        assert_eq!(m.damage.unwrap().to, 0.5);
        m.update(0.10, 0.10, 0.7, 4, false);
        assert!(m.fill > 0.5 && m.fill < 0.7);
        assert!(m.trail > 0.7);
        m.update(0.11, 0.11, 0.4, 0, false);
        assert_eq!(m.fill, 0.4);
        assert!(m.recovery.is_none());
        m.update(1.0, 1.0, 0.4, 0, false);
        assert!(m.damage.is_none());
        assert_eq!(m.trail_alpha, 0.0);
    }

    #[test]
    fn reduced_motion_snaps_recovery_fades_damage_and_freezes_texture() {
        let mut m = Motion::new(0.0, 0.0, 0.8, 99);
        m.update(0.01, 0.01, 0.4, 0, true);
        m.update(0.11, 0.11, 0.6, 100, true);
        assert_eq!(m.fill, 0.6);
        assert_eq!(m.trail, 0.8);
        assert!((m.trail_alpha - 0.5).abs() < 0.001);
        assert_eq!(m.evolution, 4.0);
        assert_eq!(m.phase, 0.626);
        assert!(m.pulse_at.is_infinite());
        m.update(0.3, 0.3, 1.0, 100, true);
        assert_eq!(m.fill, 1.0);
        assert_eq!(m.full, 1.0);
        assert_eq!(m.trail_alpha, 0.0);
    }

    #[test]
    fn seek_resets_trails_and_combo_pulse() {
        let mut m = Motion::new(10.0, 10.0, 1.0, 99);
        m.update(10.1, 10.1, 0.5, 100, false);
        m.update(10.2, 0.0, 0.8, 0, false);
        assert_eq!(m.fill, 0.8);
        assert!(m.damage.is_none() && m.recovery.is_none());
        assert!(m.pulse_at.is_infinite());
    }

    #[test]
    fn full_life_waits_for_recovery_and_damage_leaves_vivid_immediately() {
        let mut m = Motion::new(0.0, 0.0, 0.5, 99);
        m.update(0.01, 0.01, 1.0, 100, false);
        m.update(0.10, 0.10, 1.0, 100, false);
        assert!(m.fill < 1.0);
        assert_eq!(m.full, 0.0);
        m.update(0.15, 0.15, 1.0, 100, false);
        assert_eq!(m.fill, 1.0);
        assert!(m.full > 0.0 && m.full < 1.0);
        m.update(0.40, 0.40, 1.0, 100, false);
        assert_eq!(m.full, 1.0);
        m.update(0.41, 0.41, 0.95, 0, false);
        assert_eq!(m.full, 0.0);
        assert_eq!(m.fill, 0.95);
    }
}
