use std::f32::consts::PI;
use std::sync::Arc;

use crate::commands::{Arg, Call};
use crate::model::{AnimatedTexture, Animation, Geometry, Rest};
use crate::text::{RunMetrics, TextMeasure, TextStyle};

pub type Rgba = [f32; 4];

/// What tweens interpolate (`Actor::TweenState`, the parts skins use).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TweenState {
    pub pos: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    /// Per corner (`diffuse[4]`): upper left, upper right, lower left,
    /// lower right.
    pub diffuse: [Rgba; 4],
    pub glow: Rgba,
    /// A sprite's crop, fractions of its size cut away: left, top, right,
    /// bottom (`TweenState::crop`, `cropleft` and the rest).
    pub crop: [f32; 4],
    /// A sprite's faded edges, fractions of its size: left, top, right,
    /// bottom (`TweenState::fade`, `fadeleft` and the rest).
    pub fade: [f32; 4],
}

impl Default for TweenState {
    /// `TweenState::Init`.
    fn default() -> TweenState {
        TweenState {
            pos: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            diffuse: [[1.0; 4]; 4],
            glow: [1.0, 1.0, 1.0, 0.0],
            crop: [0.0; 4],
            fade: [0.0; 4],
        }
    }
}

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_n<const N: usize>(t: f32, a: [f32; N], b: [f32; N]) -> [f32; N] {
    std::array::from_fn(|i| lerp(t, a[i], b[i]))
}

impl TweenState {
    /// `MakeWeightedAverage`.
    fn between(a: &TweenState, b: &TweenState, t: f32) -> TweenState {
        TweenState {
            pos: lerp_n(t, a.pos, b.pos),
            rotation: lerp_n(t, a.rotation, b.rotation),
            scale: lerp_n(t, a.scale, b.scale),
            diffuse: std::array::from_fn(|i| lerp_n(t, a.diffuse[i], b.diffuse[i])),
            glow: lerp_n(t, a.glow, b.glow),
            crop: lerp_n(t, a.crop, b.crop),
            fade: lerp_n(t, a.fade, b.fade),
        }
    }

    /// The upper left corner's color: what `GetDiffuse` answers and a
    /// frame hands down to its children (`ActorFrame::DrawPrimitives`).
    pub fn diffuse(&self) -> Rgba {
        self.diffuse[0]
    }
}

/// An interpolator (`ITween`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    Accelerate,
    Decelerate,
    Spring,
    /// A one-dimensional cubic Bézier through four values
    /// (`InterpolateBezier1D`).
    Bezier1D([f32; 4]),
    /// A two-dimensional cubic Bézier, y read at x (`InterpolateBezier2D`).
    Bezier2D([f32; 8]),
}

impl Ease {
    pub fn apply(self, f: f32) -> f32 {
        match self {
            Ease::Linear => f,
            Ease::Accelerate => f * f,
            Ease::Decelerate => 1.0 - (1.0 - f) * (1.0 - f),
            Ease::Spring => 1.0 - (f * PI * 2.5).cos() / (1.0 + f * 3.0),
            Ease::Bezier1D(c) => cubic(c[0], c[1], c[2], c[3], f),
            Ease::Bezier2D(c) => {
                // The curve's x is monotonic for the tweens skins use; find
                // the parameter whose x is `f` by bisection.
                let (mut lo, mut hi) = (0.0f32, 1.0f32);
                for _ in 0..40 {
                    let mid = (lo + hi) / 2.0;
                    if cubic(c[0], c[2], c[4], c[6], mid) < f {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                cubic(c[1], c[3], c[5], c[7], (lo + hi) / 2.0)
            }
        }
    }
}

fn cubic(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let s = 1.0 - t;
    s * s * s * a + 3.0 * s * s * t * b + 3.0 * s * t * t * c + t * t * t * d
}

/// `02 Actor.lua`'s Bézier tweens.
const SMOOTH: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const BOUNCE_BEGIN: [f32; 8] = [0.0, 0.0, 0.42, -0.42, 2.0 / 3.0, 0.3, 1.0, 1.0];
const BOUNCE_END: [f32; 8] = [0.0, 0.0, 1.0 / 3.0, 0.7, 0.58, 1.42, 1.0, 1.0];
const DROP: [f32; 8] = [0.0, 0.0, 1.0 / 3.0, 1.0, 2.0 / 3.0, 0.5, 1.0, 1.0];

#[derive(Clone, Debug, PartialEq)]
struct Tween {
    dest: TweenState,
    ease: Ease,
    time: f32,
    left: f32,
    /// `queuecommand`'s command, played when the tween begins.
    command: Option<String>,
}

/// What is left of one update's tweening ([`Actor::tween_step`]).
#[derive(Clone, Copy, Debug)]
pub struct TweenSteps {
    dt: f32,
    /// Tweens begun so far: a command that queues itself with no time
    /// between would otherwise never end.
    guard: u32,
}

impl TweenSteps {
    pub fn new(dt: f32) -> TweenSteps {
        TweenSteps { dt, guard: 0 }
    }
}

/// `Actor::Effect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    DiffuseBlink,
    DiffuseShift,
    DiffuseRamp,
    GlowBlink,
    GlowShift,
    GlowRamp,
    Rainbow,
    Wag,
    Bounce,
    Bob,
    Pulse,
    Spin,
    Vibrate,
}

/// What an effect counts in (`Actor::EffectClock`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    /// Seconds of the actor's own updates.
    Timer,
    /// Seconds since the program started.
    TimerGlobal,
    /// The song's beat.
    Beat,
    /// The song's seconds.
    Music,
    BeatNoOffset,
    MusicNoOffset,
}

/// The clocks at an update.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clocks {
    /// Seconds since the program (the play) started.
    pub timer: f32,
    pub music_seconds: f32,
    pub beat: f32,
    pub music_seconds_no_offset: f32,
    pub beat_no_offset: f32,
}

/// `BlendMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Add,
    Subtract,
    Modulate,
    /// `BlendMode_NoEffect`: leaves the color as it is (`glBlendFunc(GL_ZERO,
    /// GL_ONE)`); what a mask is drawn with ([`Actor`]'s `MaskSource`),
    /// writing only depth.
    NoEffect,
    /// `BlendMode_CopySrc`: the source replaces what is there.
    CopySrc,
    /// `BlendMode_AlphaMask` and `BlendMode_AlphaKnockOut`: the color
    /// left alone, the target's alpha scaled by the source's (or by one
    /// less it).
    AlphaMask,
    AlphaKnockOut,
    /// `BlendMode_AlphaMultiply`: the source times its alpha, replacing
    /// what is there.
    AlphaMultiply,
    /// `BlendMode_WeightedMultiply` (Lua's `Blend.Multiply`): twice what is
    /// there times the source.
    WeightedMultiply,
    /// `BlendMode_InvertDest`: the source less what is there.
    InvertDest,
    /// A `BlendMode_` StepMania does not have: drawn as normal.
    Other,
}

/// The depth buffer's state for an actor (`Actor::SetGlobalRenderStates`):
/// what StepMania's masks are made of. `MaskSource` draws an actor into
/// depth only ([`Blend::NoEffect`], `write`), nearer than the rest (its
/// z-bias); a `MaskDest` actor tests depth and is drawn only where no
/// mask is (`WriteOnPass`), or only inside one (`WriteOnFail`). Texels
/// StepMania's alpha test drops (alpha 0.01 or less) write nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Depth {
    pub write: bool,
    pub test: ZTest,
    /// `clearzbuffer`: depth is cleared before the actor draws.
    pub clear: bool,
}

/// `ZTestMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ZTest {
    #[default]
    Off,
    /// Drawn where nothing nearer was drawn.
    WriteOnPass,
    /// Drawn only behind what was drawn.
    WriteOnFail,
}

/// The texture id of an untextured sprite ([`Sprite::quad`]): its color
/// alone.
pub const UNTEXTURED: u32 = u32::MAX;

/// A frame of a sprite's animation (`Sprite::State`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    /// `[left, top, right, bottom]`, fractions of the image.
    pub rect: [f32; 4],
    pub delay: f32,
}

/// A sprite: a texture and its states.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    /// The renderer's texture (an index it hands out).
    pub texture: u32,
    /// UI images: center-crop to the drawn rectangle after the texture size is known.
    pub cover: bool,
    /// UI images: fit whole inside the drawn rectangle, centered, once
    /// the texture size is known (a CD title in its box).
    pub contain: bool,
    /// Rounded corners, a soft edge, an outline, saturation: our
    /// extension, not StepMania's ([`Shape`]).
    pub shape: Shape,
    pub states: Vec<State>,
    /// The texture's frames (what `Frames` and `SetStateProperties` index)
    /// and the size one is shown at.
    pub frames: Vec<[f32; 4]>,
    pub frame_size: [f32; 2],
    /// The size a frame is shown at (`m_size`).
    pub size: [f32; 2],
    current: usize,
    secs_into_state: f32,
    /// Texture coordinates scroll by this per second (`texcoordvelocity`).
    pub texcoord_velocity: [f32; 2],
    /// The texture translation (scrolling, `customtexturerect`'s origin).
    pub texcoord_offset: [f32; 2],
    /// Multiplies a StepMania texture offset (a fraction of the padded
    /// texture) into a fraction of the image (`Sheet::from_padded`).
    pub padded_to_image: [f32; 2],
    /// Texture coordinates set outright (`customtexturerect`, scrolling):
    /// the states no longer pick the frame.
    pub custom: Option<[f32; 4]>,
    use_effect_clock_for_texcoords: bool,
}

impl Sprite {
    pub fn new(texture: u32, states: Vec<State>, size: [f32; 2]) -> Sprite {
        let states = if states.is_empty() {
            vec![State {
                rect: [0.0, 0.0, 1.0, 1.0],
                delay: 0.1,
            }]
        } else {
            states
        };
        Sprite {
            texture,
            cover: false,
            contain: false,
            frames: states.iter().map(|s| s.rect).collect(),
            frame_size: size,
            states,
            size,
            current: 0,
            secs_into_state: 0.0,
            texcoord_velocity: [0.0; 2],
            texcoord_offset: [0.0; 2],
            padded_to_image: [1.0; 2],
            custom: None,
            use_effect_clock_for_texcoords: false,
            shape: Shape::PLAIN,
        }
    }

    /// `Def.Quad`: a sprite of no texture (`Quad::Quad` loads the blank
    /// default one), `size` big until `setsize` or `zoomto` changes it.
    pub fn quad(size: [f32; 2]) -> Sprite {
        Sprite::new(UNTEXTURED, Vec::new(), size)
    }

    pub fn state(&self) -> usize {
        self.current
    }

    fn animation_length(&self) -> f32 {
        self.states.iter().map(|s| s.delay).sum()
    }

    /// `Sprite::SetState`: clamped, the time into it reset.
    pub fn set_state(&mut self, i: i64) {
        self.current = i.clamp(0, self.states.len() as i64 - 1) as usize;
        self.secs_into_state = 0.0;
    }

    /// `Sprite::SetSecondsIntoAnimation`.
    pub fn set_seconds_into_animation(&mut self, seconds: f32) {
        self.set_state(0);
        self.secs_into_state = seconds;
        self.advance();
    }

    /// `Sprite::UpdateAnimationState`: a state whose delay is (about) 0
    /// lasts forever.
    fn advance(&mut self) {
        const MIN: f32 = 0.0001;
        if self.states.len() <= 1 {
            return;
        }
        // At most one pass per state per call for a time far past the end
        // (the loop below would run as long as the time is large).
        let length = self.animation_length();
        if length > MIN && self.secs_into_state > length * 2.0 {
            self.secs_into_state %= length;
        }
        while self.states[self.current].delay > MIN
            && self.secs_into_state + MIN > self.states[self.current].delay
        {
            self.secs_into_state -= self.states[self.current].delay;
            self.current = (self.current + 1) % self.states.len();
        }
    }
}

/// What an actor is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// `Def.Actor`: draws nothing.
    Actor,
    Sprite(Sprite),
    Frame(Vec<Actor>),
    Model(Box<Model>),
    /// A run of text: ours, not StepMania's `BitmapText` (bitmap fonts come
    /// with themes).
    Text(Text),
}

/// A run of text drawn by whatever draws text ([`TextMeasure`]), one line.
/// Its box is the run's advance wide and a line high
/// ([`TextStyle::line_height`]); the alignment places that box, as a
/// sprite's.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    text: Arc<str>,
    style: TextStyle,
    /// `maxwidth`: a run wider than this (before zoom) is squeezed to it
    /// (`BitmapText::UpdateBaseZoom`, here without touching the base
    /// zoom); 0 for none.
    pub max_width: f32,
    /// `None` until measured again ([`Actor::measure`]).
    metrics: Option<RunMetrics>,
    /// A run much wider than `max_width` is cut and ends with "…"
    /// rather than squeezed past [`ELLIPSIS_SQUEEZE`] (our extension,
    /// command `ellipsis`; StepMania only squeezes).
    pub ellipsis: bool,
    /// What is drawn when it was cut.
    cut: Option<Arc<str>>,
}

/// How far a run with [`Text::ellipsis`] is squeezed before it is cut.
pub const ELLIPSIS_SQUEEZE: f32 = 0.85;

impl Text {
    pub fn new(text: impl Into<Arc<str>>, style: TextStyle) -> Text {
        Text {
            text: text.into(),
            style,
            max_width: 0.0,
            metrics: None,
            ellipsis: false,
            cut: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// What is drawn: the text, or its cut with "…".
    pub fn shown(&self) -> &Arc<str> {
        self.cut.as_ref().unwrap_or(&self.text)
    }

    pub fn style(&self) -> &TextStyle {
        &self.style
    }

    /// `settext`: measured again before it is drawn.
    pub fn set_text(&mut self, text: &str) {
        if *self.text != *text {
            self.text = text.into();
            self.metrics = None;
            // Its cut was of the old text: until measured, the text.
            self.cut = None;
        }
    }

    pub fn set_style(&mut self, style: TextStyle) {
        if self.style != style {
            self.style = style;
            self.metrics = None;
            self.cut = None;
        }
    }

    /// The run's metrics, measured if they are not known (and the run
    /// cut, with [`Text::ellipsis`], when squeezing would take it below
    /// [`ELLIPSIS_SQUEEZE`]).
    pub fn measure(&mut self, measure: &mut dyn TextMeasure) -> RunMetrics {
        if let Some(m) = self.metrics {
            return m;
        }
        let mut m = measure.measure(&self.text, &self.style);
        self.cut = None;
        let room = self.max_width / ELLIPSIS_SQUEEZE;
        if self.ellipsis && self.max_width > 0.0 && m.advance > room {
            // The longest start that fits with "…", by halves.
            let ends: Vec<usize> = self.text.char_indices().map(|(i, _)| i).skip(1).collect();
            let (mut lo, mut hi) = (0usize, ends.len());
            let mut best: Option<(String, RunMetrics)> = None;
            while lo < hi {
                let mid = (lo + hi) / 2;
                let s = format!("{}…", self.text[..ends[mid]].trim_end());
                let mm = measure.measure(&s, &self.style);
                if mm.advance <= room {
                    best = Some((s, mm));
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            let (s, mm) = best.unwrap_or_else(|| {
                let s = "…".to_string();
                let mm = measure.measure(&s, &self.style);
                (s, mm)
            });
            self.cut = Some(s.into());
            m = mm;
        }
        self.metrics = Some(m);
        m
    }

    /// The last measurement; `None` after a change, until measured.
    pub fn metrics(&self) -> Option<RunMetrics> {
        self.metrics
    }

    /// The box: the advance (0 until measured) by a line.
    pub fn size(&self) -> [f32; 2] {
        [
            self.metrics.map_or(0.0, |m| m.advance),
            self.style.line_height(),
        ]
    }

    /// The horizontal squeeze `maxwidth` asks for: never more than 1.
    pub fn squeeze(&self) -> f32 {
        let width = self.size()[0];
        if self.max_width > 0.0 && width > 0.0 {
            (self.max_width / width).min(1.0)
        } else {
            1.0
        }
    }
}

/// A 3D model (`Model`, `src/Model.cpp`): its meshes and bones (shared), its
/// materials' textures, and where its animations are.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub geometry: Arc<Geometry>,
    pub animation: Arc<Animation>,
    pub materials: Vec<ModelMaterial>,
    /// The bones at rest (from [`Model::animation`]).
    pub rest: Arc<Vec<Rest>>,
    /// Each bone's matrix now (`m_Final`).
    pub bones: Vec<Mat4>,
    /// The bones' frame (30 a second), its rate, and whether it loops
    /// (`Model::AdvanceFrame`).
    pub frame: f32,
    pub rate: f32,
    pub looping: bool,
    /// `SetDefaultAnimation`: whether it names the one animation there is
    /// ("default"), and its rate; played again when the frame runs out.
    pub default_animation: Option<(bool, f32)>,
    /// Seconds into the textures' animation (`SetSecondsIntoAnimation`).
    pub seconds: f32,
    /// `setstate`: every material's frame; `None`: by time.
    pub state: Option<usize>,
}

/// A model's material, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelMaterial {
    /// Unlit ([`crate::model::Material::unlit_color`]).
    pub color: Rgba,
    /// The renderer's texture of each frame of [`ModelMaterial::texture`];
    /// empty: untextured.
    pub frames: Vec<u32>,
    pub texture: AnimatedTexture,
}

impl Model {
    /// A model as `Model::LoadPieces` leaves it: its "default" animation
    /// playing from its first frame, looping, its vertices that follow a
    /// bone of their own made relative to it.
    pub fn new(
        mut geometry: Geometry,
        animation: Animation,
        materials: Vec<ModelMaterial>,
    ) -> Model {
        let rest = animation.rest();
        crate::model::to_rest(&mut geometry, &rest);
        let mut bones = rest.iter().map(|r| r.absolute).collect();
        animation.pose(&rest, 0.0, &mut bones);
        Model {
            geometry: Arc::new(geometry),
            animation: Arc::new(animation),
            materials,
            rest: Arc::new(rest),
            bones,
            frame: 0.0,
            rate: 1.0,
            looping: true,
            default_animation: None,
            seconds: 0.0,
            state: None,
        }
    }

    /// Seconds of the textures' animation, the longest material's
    /// (`Model::GetAnimationLengthSeconds`).
    pub fn animation_length(&self) -> f32 {
        self.materials
            .iter()
            .map(|m| m.texture.length())
            .fold(0.0, f32::max)
    }

    /// `GetNumStates`: the most frames a material has.
    pub fn num_states(&self) -> usize {
        self.materials
            .iter()
            .map(|m| m.frames.len())
            .max()
            .unwrap_or(0)
    }

    /// One update (`Model::Update`): the bones' frame and pose, the
    /// textures' time.
    fn advance(&mut self, dt: f32) {
        if dt.is_finite() {
            self.seconds += dt;
        }
        if self.geometry.meshes.is_empty() || !dt.is_finite() {
            return;
        }
        // `AdvanceFrame`.
        let total = self.animation.total_frames as f32;
        self.frame += 30.0 * dt * self.rate;
        if !(self.frame >= 0.0 && self.frame < total) {
            match self.default_animation {
                // `PlayAnimation` again: from its start, at its rate.
                Some((true, rate)) => {
                    self.frame = 0.0;
                    self.rate = rate;
                }
                // An animation that is not there: nothing changes.
                Some((false, _)) => {}
                // `wrap`; of no frames, past every key.
                None if self.looping => {
                    self.frame = if total > 0.0 {
                        self.frame.rem_euclid(total)
                    } else {
                        f32::NAN
                    };
                }
                None => self.frame = self.frame.clamp(0.0, total),
            }
        }
        self.animation.pose(&self.rest, self.frame, &mut self.bones);
    }

    /// What the materials show now: texture, color (times `diffuse`, as
    /// `DrawPrimitives` multiplies ambient, diffuse and emissive by the
    /// actor's), translation.
    fn materials_now(&self, diffuse: Rgba) -> Vec<(Option<u32>, Rgba, [f32; 2])> {
        // Each material wraps it by its own length.
        let seconds = self.seconds;
        self.materials
            .iter()
            .map(|m| {
                let frame = match self.state {
                    Some(st) => st.min(m.frames.len().saturating_sub(1)),
                    None => m.texture.frame_at(seconds),
                };
                let color = [
                    (m.color[0] * diffuse[0]).clamp(0.0, 1.0),
                    (m.color[1] * diffuse[1]).clamp(0.0, 1.0),
                    (m.color[2] * diffuse[2]).clamp(0.0, 1.0),
                    (m.color[3] * diffuse[3]).clamp(0.0, 1.0),
                ];
                (
                    m.frames.get(frame).copied(),
                    color,
                    m.texture.translation(seconds),
                )
            })
            .collect()
    }
}

/// A model to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelDraw {
    /// Model space (MilkShape's: y up) to screen pixels, z toward the
    /// viewer; a column-major 4×4.
    pub transform: [f32; 16],
    pub geometry: Arc<Geometry>,
    /// Per material: its texture now (`None`: none), color, translation.
    pub materials: Vec<(Option<u32>, Rgba, [f32; 2])>,
    /// Each bone's matrix (MilkShape's space), for the meshes and vertices
    /// that follow one.
    pub bones: Vec<Mat4>,
    pub diffuse: Rgba,
    pub glow: Rgba,
    pub blend: Blend,
    pub depth: Depth,
    /// What sees it: the camera of the `fov` frame around, as the quads
    /// beside it are seen.
    pub camera: FrameCamera,
}

/// The camera the `fov` frames around an actor set (`ActorFrame::BeginDraw`
/// → `RageDisplay::LoadMenuPerspective`, which loads the projection every
/// actor inside is drawn with until the frame's `EndDraw` pops it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FrameCamera {
    /// No `fov` frame around: the drawer's own (the note field's 45° in
    /// play, `Player::PushPlayerMatrix`). Quads are drawn orthographically
    /// in it (at z = 0 the two agree), models in that perspective.
    #[default]
    Drawer,
    /// An `fov` frame's.
    Perspective(Perspective),
    /// An `fov(0)` frame's: orthographic, depth clipped to ±1000 virtual
    /// pixels (`GetOrthoMatrix(0, W, H, 0, -1000, +1000)`); `depth` is
    /// that bound on the target.
    Orthographic { depth: f32 },
}

impl FrameCamera {
    /// The perspective quads are seen in; `None`: orthographic.
    pub fn perspective(&self) -> Option<Perspective> {
        match self {
            FrameCamera::Perspective(p) => Some(*p),
            FrameCamera::Drawer | FrameCamera::Orthographic { .. } => None,
        }
    }
}

/// A run of text to draw ([`Kind::Text`]).
#[derive(Clone, Debug, PartialEq)]
pub struct TextDraw {
    pub text: Arc<str>,
    pub style: TextStyle,
    pub metrics: RunMetrics,
    /// The run's space (the pen at the origin on the baseline, x along the
    /// run, y down, in virtual pixels at the style's size) to the target's
    /// pixels: `x' = m[0]x + m[2]y + m[4]`, `y' = m[1]x + m[3]y + m[5]`,
    /// `maxwidth`'s squeeze included. The scale it has is the one to
    /// rasterise at.
    pub transform: [f32; 6],
    /// Per corner of the run's box, as [`Quad::diffuse`].
    pub diffuse: [Rgba; 4],
    pub glow: Rgba,
    pub blend: Blend,
    pub depth: Depth,
    /// Under an `fov` frame: the run's space to the target's pixels with
    /// z toward the viewer (a column-major 4×4), and the camera that sees
    /// it ([`Perspective::project`]); `transform` is then its orthographic
    /// view.
    pub perspective: Option<(Mat4, Perspective)>,
    /// A motion blur on the target, as [`Quad::blur`].
    pub blur: [f32; 2],
}

impl TextDraw {
    /// [`TextDraw::transform`] from the line box's top left instead of
    /// the pen.
    pub fn line_box_transform(&self) -> [f32; 6] {
        shifted_local(&self.transform, [0.0, -self.metrics.baseline])
    }

    /// [`TextDraw::perspective`] from the line box's top left instead of
    /// the pen.
    pub fn line_box_perspective(&self) -> Option<(Mat4, Perspective)> {
        self.perspective.map(|(m, camera)| {
            let top = translation4([0.0, -self.metrics.baseline, 0.0]);
            (mul4(&m, &top), camera)
        })
    }
}

/// `m` after a move of `by` in its own space.
fn shifted_local(m: &Mat, by: [f32; 2]) -> Mat {
    mul(m, &[1.0, 0.0, 0.0, 1.0, by[0], by[1]])
}

/// A frame's clipping rectangle ([`Actor::clip`]): what is drawn between
/// [`Drawn::ClipPush`] and its [`Drawn::ClipPop`] is cut to it, and to
/// every clip around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    /// The rectangle's corners on the target, as seen (upper left, upper
    /// right, lower left, lower right of the frame's rectangle).
    pub corners: [[f32; 2]; 4],
    /// `[left, top, right, bottom]` on the target: the corners' bounds
    /// within the enclosing clips' scissor. Never empty (a frame whose
    /// clip leaves nothing draws nothing).
    pub scissor: [f32; 4],
    /// Whether the rectangle is the scissor alone (it stays axis-aligned:
    /// no rotation, skew or perspective); otherwise the corners' quad
    /// cuts too (a stencil).
    pub exact: bool,
}

/// What an actor draws, in drawing order.
#[derive(Clone, Debug, PartialEq)]
pub enum Drawn {
    Quad(Quad),
    Model(ModelDraw),
    Text(TextDraw),
    ClipPush(Clip),
    ClipPop,
}

/// Where StepMania's screen is on the target: what an `fov` frame's camera
/// is made from (its vanishing point, by default the screen's centre, and
/// its distance, `ActorFrame::BeginDraw`), and what a shadow's offset is
/// measured in (`TranslateWorld`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Virtual pixels (the screen's, [`crate::screen::VirtualScreen`]) to
    /// the target's.
    pub transform: [f32; 6],
    /// `SCREEN_WIDTH`, `SCREEN_HEIGHT`.
    pub size: [f32; 2],
}

impl Default for View {
    /// StepMania's 16:9 screen, 480 high, drawn as it is.
    fn default() -> View {
        View {
            transform: IDENTITY,
            size: [854.0, 480.0],
        }
    }
}

impl View {
    /// The target's pixels per virtual pixel.
    fn scale(&self) -> f32 {
        scale_of(&self.transform)
    }

    /// A vector of the screen's on the target.
    fn vector(&self, v: [f32; 2]) -> [f32; 2] {
        let t = &self.transform;
        [t[0] * v[0] + t[2] * v[1], t[1] * v[0] + t[3] * v[1]]
    }

    fn point(&self, p: [f32; 2]) -> [f32; 2] {
        apply(&self.transform, p)
    }
}

/// A camera an `fov` frame sets, in the target's pixels
/// (`RageDisplay::LoadMenuPerspective`): the eye `distance` in front of
/// the screen's plane, over the vanishing point; the plane z = 0 is seen
/// as it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Perspective {
    pub vanish: [f32; 2],
    pub distance: f32,
    /// The frustum's near and far planes, as distances from the eye (1
    /// and `distance + 1000` virtual pixels: `GetFrustumMatrix(…, 1,
    /// fDistCameraFromImage + 1000)`).
    pub near: f32,
    pub far: f32,
}

impl Perspective {
    /// `LoadMenuPerspective(fov, SCREEN_WIDTH, SCREEN_HEIGHT, vanish)`
    /// (`src/RageDisplay.cpp:573-611`): the eye where the screen's width
    /// spans `fov` degrees; `None` for 0, which is orthographic.
    pub fn new(view: &View, fov: f32, vanish: [f32; 2]) -> Option<Perspective> {
        if fov == 0.0 || !fov.is_finite() {
            return None;
        }
        let theta = fov.clamp(0.1, 179.9).to_radians() / 2.0;
        let k = view.scale();
        let distance = view.size[0] / 2.0 / theta.tan();
        Some(Perspective {
            vanish: view.point(vanish),
            distance: distance * k,
            near: k,
            far: (distance + 1000.0) * k,
        })
    }

    /// Where the point (target pixels, z toward the viewer) is seen, and
    /// its `w` (its distance from the eye over the screen's); `None`
    /// outside the frustum's depth.
    pub fn project(&self, p: [f32; 3]) -> Option<([f32; 2], f32)> {
        let depth = self.distance - p[2];
        if !(depth >= self.near && depth <= self.far) {
            return None;
        }
        let s = self.distance / depth;
        Some((
            [
                self.vanish[0] + (p[0] - self.vanish[0]) * s,
                self.vanish[1] + (p[1] - self.vanish[1]) * s,
            ],
            depth / self.distance,
        ))
    }

    /// The points (in the plane z = 0) under `m`, seen: where and their
    /// `w`, as [`Perspective::project`]; `None` when one is outside the
    /// depth.
    pub fn project_quad(
        &self,
        m: &Mat4,
        points: [[f32; 2]; 4],
    ) -> Option<([[f32; 2]; 4], [f32; 4])> {
        let mut seen = [[0.0; 2]; 4];
        let mut w = [0.0; 4];
        for (i, [x, y]) in points.into_iter().enumerate() {
            (seen[i], w[i]) = self.project(transform_point(m, [x, y, 0.0]))?;
        }
        Some((seen, w))
    }
}

/// A quad's corners as a camera sees them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projected {
    /// Upper left, upper right, lower left, lower right, on the target.
    pub corners: [[f32; 2]; 4],
    /// Each corner's `w` ([`Perspective::project`]), for texturing in
    /// perspective.
    pub w: [f32; 4],
}

impl Projected {
    /// The local `[-0.5, 0.5]²` square under `transform` (z toward the
    /// viewer), seen by `camera`; `None` when a corner is outside its
    /// depth.
    fn of(camera: &Perspective, transform: &Mat4) -> Option<Projected> {
        let (corners, w) = camera.project_quad(
            transform,
            [[-0.5, -0.5], [0.5, -0.5], [-0.5, 0.5], [0.5, 0.5]],
        )?;
        Some(Projected { corners, w })
    }
}

/// An actor.
#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    pub name: String,
    /// The actor's handle in its skin's Lua (`0`: none).
    pub id: u32,
    pub kind: Kind,
    current: TweenState,
    start: TweenState,
    tweens: Vec<Tween>,
    pub base_rotation: [f32; 3],
    pub base_scale: [f32; 3],
    pub base_alpha: f32,
    pub visible: bool,
    pub blend: Blend,
    pub depth: Depth,
    pub halign: f32,
    pub valign: f32,
    /// `draworder`: where a frame puts the actor among its children when
    /// it sorts them ([`Actor::sort_by_draw_order`]).
    pub draw_order: i32,
    /// `shadowlength`: a sprite or text draws a copy of itself this far
    /// right and down, in the screen's pixels, under it (`Sprite::DrawTexture`).
    pub shadow: [f32; 2],
    /// `shadowcolor`: the copy's color, its alpha times the actor's.
    pub shadow_color: Rgba,
    /// `motionblur`: what it draws, and everything inside it, smeared
    /// this far along its own y axis (in its units, centered), as a list's
    /// rows moving fast are. Our extension, not StepMania's: drawn only by
    /// the 2D pass ([`Quad::blur`]).
    pub motion_blur: f32,
    /// A frame's `fov` (`ActorFrame::m_fFOV`): -1 keeps the camera around
    /// it, 0 is orthographic, anything else the degrees the screen's width
    /// spans ([`Perspective::new`]).
    pub fov: f32,
    /// A frame's `vanishpoint`, in the screen's pixels; `None`: its centre.
    pub vanish: Option<[f32; 2]>,
    /// A frame's `SetDrawByZPosition`: children drawn by their z, far
    /// first (stable).
    pub draw_by_z: bool,
    /// A frame's clipping rectangle, `[left, top, right, bottom]` in its
    /// own space: our own, not StepMania's ([`Clip`]).
    pub clip: Option<[f32; 4]>,
    effect: Effect,
    secs_into_effect: f32,
    effect_delta: f32,
    /// Ramp to half, hold at half, ramp to full, hold at full, hold at zero.
    timing: [f32; 5],
    effect_offset: f32,
    /// OpenITG's `effectdelay`: seconds held at full after each period
    /// (StepMania 3.9 skins' commands; StepMania 5 has `effecttiming`).
    effect_delay: f32,
    clock: Clock,
    magnitude: [f32; 3],
    effect_color1: Rgba,
    effect_color2: Rgba,
    hibernate: f32,
    animating: bool,
    update_rate: f32,
    /// Shared, so playing one does not copy it.
    commands: Vec<(String, Arc<[Call]>)>,
    rng: u64,
}

/// The methods [`Actor::call`] runs (the setters a skin's Lua may call);
/// `playcommand` is the Lua driver's in a Lua skin.
pub const METHODS: &[&str] = &[
    "MaskDest",
    "MaskSource",
    "SetAllStateDelays",
    "SetDrawByZPosition",
    "SetFOV",
    "SetHeight",
    "SetSecondsIntoAnimation",
    "SetTextureFiltering",
    "SetUpdateRate",
    "SetWidth",
    "accelerate",
    "addrotationz",
    "addx",
    "addy",
    "addz",
    "animate",
    "basealpha",
    "baserotationx",
    "baserotationy",
    "baserotationz",
    "basezoom",
    "basezoomx",
    "basezoomy",
    "blend",
    "bob",
    "bounce",
    "bouncebegin",
    "bounceend",
    "clearzbuffer",
    "cornerradius",
    "cropbottom",
    "cropleft",
    "cropright",
    "croptop",
    "cullmode",
    "customtexturerect",
    "decelerate",
    "diffuse",
    "diffusealpha",
    "diffuseblink",
    "diffusebottomedge",
    "diffusecolor",
    "diffuseleftedge",
    "diffuselowerleft",
    "diffuselowerright",
    "diffuseramp",
    "diffuserightedge",
    "diffuseshift",
    "diffusetopedge",
    "diffuseupperleft",
    "diffuseupperright",
    "draworder",
    "drop",
    "effect_hold_at_full",
    "effectclock",
    "effectcolor1",
    "effectcolor2",
    "effectmagnitude",
    "effectoffset",
    "effectperiod",
    "effecttiming",
    "fadebottom",
    "fadeleft",
    "faderight",
    "fadetop",
    "finishtweening",
    "ellipsis",
    "fov",
    "glow",
    "glowblink",
    "glowramp",
    "glowshift",
    "halign",
    "heartbeat",
    "hibernate",
    "horizalign",
    "hurrytweening",
    "linear",
    "maxwidth",
    "motionblur",
    "outline",
    "pause",
    "play",
    "pulse",
    "queuecommand",
    "rainbow",
    "rotationx",
    "rotationy",
    "rotationz",
    "saturation",
    "scaletocover",
    "scaletofit",
    "set_use_effect_clock_for_texcoords",
    "setsize",
    "setstate",
    "settext",
    "shadowcolor",
    "shadowlength",
    "shadowlengthx",
    "shadowlengthy",
    "sleep",
    "smooth",
    "softedge",
    "spin",
    "spring",
    "stopeffect",
    "stoptweening",
    "stretchto",
    "texcoordvelocity",
    "thump",
    "valign",
    "vanishpoint",
    "vertalign",
    "vibrate",
    "visible",
    "wag",
    "x",
    "xy",
    "y",
    "z",
    "zbuffer",
    "zoom",
    "zoomto",
    "zoomtoheight",
    "zoomtowidth",
    "zoomx",
    "zoomy",
    "zoomz",
    "ztest",
    "ztestmode",
    "zwrite",
];

/// How a sprite's quad is cut and colored beyond StepMania's (our
/// extension for the game's own theme, not StepMania's; commands
/// `cornerradius`, `softedge`, `outline`, `saturation`): sizes are the
/// sprite's own units (before zoom). The plain values draw StepMania's
/// quad, exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// The corners' radius (capped at half the shorter side).
    pub radius: f32,
    /// A soft edge this wide each way of the shape, which is the quad
    /// inset by as much (a shadow: a quad as large as the shape and twice
    /// this).
    pub soft: f32,
    /// Only an outline this thick, inside the edge (0: filled).
    pub outline: f32,
    /// 1 as it is, 0 grey.
    pub saturation: f32,
}

impl Shape {
    pub const PLAIN: Shape = Shape {
        radius: 0.0,
        soft: 0.0,
        outline: 0.0,
        saturation: 1.0,
    };

    pub fn is_plain(&self) -> bool {
        *self == Shape::PLAIN
    }
}

impl Default for Shape {
    fn default() -> Shape {
        Shape::PLAIN
    }
}

/// What a getter returns to Lua.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f32),
    Str(String),
    Bool(bool),
    Color(Rgba),
    /// Several results.
    Many(Vec<Value>),
}

/// Commands that went wrong, and methods this runtime does not have.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Problems {
    pub messages: Vec<String>,
    /// Calls made since the last top-level play or update.
    calls: u32,
}

/// Calls one top-level play or update may make: a command that plays
/// itself twice per level would otherwise make billions.
const MAX_CALLS: u32 = 20_000;

impl Problems {
    /// Counts a call; false once the budget is spent (reported once).
    fn spend(&mut self, actor: &str) -> bool {
        self.calls += 1;
        if self.calls == MAX_CALLS + 1 {
            self.push(
                actor,
                "commands play each other without end; stopped".into(),
            );
        }
        self.calls <= MAX_CALLS
    }

    fn push(&mut self, actor: &str, m: String) {
        if self.messages.len() < 64 {
            self.messages.push(if actor.is_empty() {
                m
            } else {
                format!("{actor}: {m}")
            });
        }
    }
}

/// One textured quad to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    /// Local `[-0.5, 0.5]²` to screen: `x' = m[0]x + m[2]y + m[4]`,
    /// `y' = m[1]x + m[3]y + m[5]`, already scaled by the quad's size.
    pub transform: [f32; 6],
    pub texture: u32,
    /// UI images: center-crop to the drawn rectangle after the texture size is known.
    pub cover: bool,
    /// UI images: fit whole inside the drawn rectangle, centered.
    pub contain: bool,
    /// `[left, top, right, bottom]` of the image, before `texcoord_offset`.
    pub uv: [f32; 4],
    pub texcoord_offset: [f32; 2],
    /// Per corner: upper left, upper right, lower left, lower right
    /// (`Sprite::DrawTexture` gives each vertex its own).
    pub diffuse: [Rgba; 4],
    pub glow: Rgba,
    pub blend: Blend,
    pub depth: Depth,
    /// Under an `fov` frame: the corners as its camera sees them;
    /// `transform` is then their orthographic view.
    pub projected: Option<Projected>,
    /// Its sprite's [`Shape`], and the size it is in (units before zoom).
    pub shape: Shape,
    pub size: [f32; 2],
    /// A motion blur ([`Actor::motion_blur`] here and around it): the
    /// quad smeared along this vector on the target, centered; `[0, 0]`:
    /// none.
    pub blur: [f32; 2],
}

type Mat = [f32; 6];

const IDENTITY: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `a` then `b`: `b ∘ a` in the `x' = m·x` sense, i.e. `parent * local`.
pub fn mul(parent: &Mat, local: &Mat) -> Mat {
    [
        parent[0] * local[0] + parent[2] * local[1],
        parent[1] * local[0] + parent[3] * local[1],
        parent[0] * local[2] + parent[2] * local[3],
        parent[1] * local[2] + parent[3] * local[3],
        parent[0] * local[4] + parent[2] * local[5] + parent[4],
        parent[1] * local[4] + parent[3] * local[5] + parent[5],
    ]
}

/// The point `p` under `m`: `x' = m[0]x + m[2]y + m[4]`,
/// `y' = m[1]x + m[3]y + m[5]`.
pub fn apply(m: &Mat, p: [f32; 2]) -> [f32; 2] {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}

/// The lengths `m` scales by on average: the square root of its
/// determinant's size.
pub fn scale_of(m: &Mat) -> f32 {
    (m[0] * m[3] - m[1] * m[2]).abs().sqrt()
}

const MAX_DEPTH: u32 = 32;
/// `BeginTweening`: more than this many queued tweens is a runaway command.
const MAX_TWEENS: usize = 50;

impl Actor {
    pub fn new(kind: Kind) -> Actor {
        let mut a = Actor {
            name: String::new(),
            id: 0,
            kind,
            current: TweenState::default(),
            start: TweenState::default(),
            tweens: Vec::new(),
            base_rotation: [0.0; 3],
            base_scale: [1.0; 3],
            base_alpha: 1.0,
            visible: true,
            blend: Blend::Normal,
            depth: Depth::default(),
            halign: 0.5,
            valign: 0.5,
            draw_order: 0,
            shadow: [0.0; 2],
            // `Actor::InitState`.
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            motion_blur: 0.0,
            fov: -1.0,
            vanish: None,
            draw_by_z: false,
            clip: None,
            effect: Effect::None,
            secs_into_effect: 0.0,
            effect_delta: 0.0,
            timing: [0.0; 5],
            effect_offset: 0.0,
            effect_delay: 0.0,
            clock: Clock::Timer,
            magnitude: [0.0, 0.0, 10.0],
            effect_color1: [1.0; 4],
            effect_color2: [1.0; 4],
            hibernate: 0.0,
            animating: true,
            update_rate: 1.0,
            commands: Vec::new(),
            rng: 0x853c_49e6_748f_ea9b,
        };
        a.set_effect_period(1.0);
        a
    }

    pub fn current(&self) -> &TweenState {
        &self.current
    }

    pub fn effect(&self) -> Effect {
        self.effect
    }

    pub fn tweening(&self) -> bool {
        !self.tweens.is_empty()
    }

    /// `GetTweenTimeLeft`: seconds until its tweens (and its hibernation)
    /// end; a frame's, the longest of its own and its children's after its
    /// hibernation (`ActorFrame::GetTweenTimeLeft`).
    pub fn tween_time_left(&self) -> f32 {
        let own = self.hibernate + self.tweens.iter().map(|t| t.left).sum::<f32>();
        self.children()
            .iter()
            .map(|c| self.hibernate + c.tween_time_left())
            .fold(own, f32::max)
    }

    /// `InitState`, a frame's children first (`ActorFrame::InitState`):
    /// tweens stopped, its state, base transform, alignment, effect,
    /// shadow, draw order and blend back to their defaults, a sprite's
    /// animation at its first state (`Sprite::InitState`). What it is
    /// (its kind, texture, size, commands) stays. StepMania runs it on a
    /// background each time it comes on (`UpdateCurBGChange`).
    pub fn init_state(&mut self) {
        let fresh = Actor::new(Kind::Actor);
        for c in self.children_mut() {
            c.init_state();
        }
        self.tweens.clear();
        self.start = fresh.start;
        self.current = fresh.current;
        self.base_rotation = fresh.base_rotation;
        self.base_scale = fresh.base_scale;
        self.base_alpha = fresh.base_alpha;
        self.halign = fresh.halign;
        self.valign = fresh.valign;
        self.effect = fresh.effect;
        self.secs_into_effect = 0.0;
        self.effect_delta = 0.0;
        self.timing = fresh.timing;
        self.effect_offset = 0.0;
        self.clock = fresh.clock;
        self.magnitude = fresh.magnitude;
        self.effect_color1 = fresh.effect_color1;
        self.effect_color2 = fresh.effect_color2;
        self.visible = true;
        self.shadow = fresh.shadow;
        self.shadow_color = fresh.shadow_color;
        self.animating = true;
        self.hibernate = 0.0;
        self.draw_order = 0;
        self.blend = fresh.blend;
        self.depth = fresh.depth;
        if let Kind::Sprite(s) = &mut self.kind {
            s.current = 0;
            s.secs_into_state = 0.0;
        }
    }

    pub fn children(&self) -> &[Actor] {
        match &self.kind {
            Kind::Frame(c) => c,
            _ => &[],
        }
    }

    pub fn children_mut(&mut self) -> &mut [Actor] {
        match &mut self.kind {
            Kind::Frame(c) => c,
            _ => &mut [],
        }
    }

    /// Sets command `name` (without `Command`), replacing one already there.
    pub fn set_command(&mut self, name: &str, calls: Vec<Call>) {
        match self.commands.iter_mut().find(|(n, _)| n == name) {
            Some((_, c)) => *c = calls.into(),
            None => self.commands.push((name.to_string(), calls.into())),
        }
    }

    /// Adds `calls` after command `name`'s, as `t .. { NameCommand = … }`
    /// chains two functions (`MergeTables`, `02 ActorDef.lua:12-37`).
    pub fn add_command(&mut self, name: &str, calls: Vec<Call>) {
        match self.commands.iter_mut().find(|(n, _)| n == name) {
            Some((_, c)) => *c = c.iter().cloned().chain(calls).collect(),
            None => self.commands.push((name.to_string(), calls.into())),
        }
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.commands.iter().any(|(n, _)| n == name)
    }

    /// `SetDiffuse`/`SetGlow` as `NoteDisplay::DrawActor` calls them for a
    /// note: the color of the appearance mods and a hold's life.
    pub fn set_note_colors(&mut self, diffuse: Rgba, glow: Rgba) {
        let d = self.dest();
        d.diffuse = [diffuse; 4];
        d.glow = glow;
    }

    /// `SetPRZForActor` with no movement mods: the field places a note, so
    /// its own position, rotation and zoom are reset (its base rotation
    /// stays).
    pub fn reset_placement(&mut self) {
        let d = self.dest();
        d.pos = [0.0; 3];
        d.rotation = [0.0; 3];
        d.scale = [1.0; 3];
    }

    /// Every sprite texture the actor and its children draw.
    pub fn textures(&self) -> Vec<u32> {
        match &self.kind {
            Kind::Sprite(s) if s.texture == UNTEXTURED => Vec::new(),
            Kind::Sprite(s) => vec![s.texture],
            Kind::Frame(c) => c.iter().flat_map(Actor::textures).collect(),
            Kind::Model(m) => m.materials.iter().flat_map(|m| m.frames.clone()).collect(),
            Kind::Actor | Kind::Text(_) => Vec::new(),
        }
    }

    /// Measures the text runs changed since they were last measured
    /// ([`Text::set_text`], `settext`), so that their size is known to
    /// the alignment, `maxwidth`, `zoomto` and the getters. A screen calls
    /// it after its updates, before drawing: commands run where nothing
    /// measures (a skin's or a theme's Lua), so a run changed by one has
    /// its old size until then.
    pub fn measure(&mut self, measure: &mut dyn TextMeasure) {
        self.each(&mut |a| {
            if let Kind::Text(t) = &mut a.kind {
                t.measure(measure);
            }
        });
    }

    /// Forgets every run's metrics, so the next [`Actor::measure`] asks
    /// again (the measurer learnt better than it answered).
    pub fn forget_metrics(&mut self) {
        self.each(&mut |a| {
            if let Kind::Text(t) = &mut a.kind {
                t.metrics = None;
                t.cut = None;
            }
        });
    }

    /// `ActorFrame::SortByDrawOrder`: the children by draw order, those of
    /// equal order kept as they are (`stable_sort`, `ActorFrame.cpp:548-552`).
    /// StepMania sorts once a frame's children are loaded, after their
    /// `Init` and before the frame's own (`LoadChildrenFromNode`), and when
    /// Lua asks; a later `draworder` waits for the next sort. Whether the
    /// order changed.
    pub fn sort_by_draw_order(&mut self) -> bool {
        let Kind::Frame(children) = &mut self.kind else {
            return false;
        };
        if children.is_sorted_by_key(|c| c.draw_order) {
            return false;
        }
        children.sort_by_key(|c| c.draw_order);
        true
    }

    /// The state setters write: the last queued tween's end, or the
    /// current state (`DestTweenState`).
    fn dest(&mut self) -> &mut TweenState {
        match self.tweens.last_mut() {
            Some(t) => &mut t.dest,
            None => &mut self.current,
        }
    }

    // Tweening (`Actor::BeginTweening` and the rest).

    pub fn begin_tweening(&mut self, time: f32, ease: Ease) {
        let time = time.max(0.0);
        if self.tweens.len() > MAX_TWEENS {
            self.finish_tweening();
        }
        let dest = self.tweens.last().map_or(self.current, |t| t.dest);
        self.tweens.push(Tween {
            dest,
            ease,
            time,
            left: time,
            command: None,
        });
    }

    pub fn stop_tweening(&mut self) {
        self.tweens.clear();
    }

    pub fn finish_tweening(&mut self) {
        if let Some(t) = self.tweens.last() {
            self.current = t.dest;
        }
        self.stop_tweening();
    }

    pub fn hurry_tweening(&mut self, factor: f32) {
        for t in &mut self.tweens {
            t.left *= factor;
            t.time *= factor;
        }
    }

    /// `f` on this actor and every actor inside it.
    fn each(&mut self, f: &mut dyn FnMut(&mut Actor)) {
        f(self);
        if let Kind::Frame(children) = &mut self.kind {
            for c in children {
                c.each(f);
            }
        }
    }

    /// `Actor::Sleep`.
    pub fn sleep(&mut self, time: f32) {
        self.begin_tweening(time, Ease::Linear);
        self.begin_tweening(0.0, Ease::Linear);
    }

    /// `Actor::QueueCommand`.
    pub fn queue_command(&mut self, name: &str) {
        self.begin_tweening(0.0, Ease::Linear);
        if let Some(t) = self.tweens.last_mut() {
            t.command = Some(name.to_string());
        }
    }

    // Effects.

    fn effect_period(&self) -> f32 {
        self.timing.iter().sum::<f32>() + self.effect_delay
    }

    /// `SetEffectPeriod`: half up, half down.
    pub fn set_effect_period(&mut self, time: f32) {
        self.timing = [time / 2.0, 0.0, time / 2.0, 0.0, 0.0];
    }

    /// `ResetEffectTimeIfDifferent`.
    fn switch_effect(&mut self, effect: Effect) {
        if self.effect != effect {
            self.effect = effect;
            self.secs_into_effect = 0.0;
        }
    }

    fn set_color_effect(&mut self, effect: Effect, period: f32, c1: Rgba, c2: Rgba) {
        self.switch_effect(effect);
        self.set_effect_period(period);
        self.effect_color1 = c1;
        self.effect_color2 = c2;
    }

    /// How far through its cycle the effect is, `0..=1`
    /// (`Actor::PreDraw`).
    fn percent_through_effect(&self) -> f32 {
        let period = self.effect_period();
        if period <= 0.0 {
            return 0.0;
        }
        let t = (self.secs_into_effect + self.effect_offset).rem_euclid(period);
        let [rth, hah, rtf, haf, _] = self.timing;
        let haf = haf + self.effect_delay;
        let (a, b, c) = (rth + hah, rth + hah + rtf, rth + hah + rtf + haf);
        if t < rth {
            t / rth * 0.5
        } else if t < a {
            0.5
        } else if t < b {
            0.5 + (t - a) / rtf * 0.5
        } else if t < c {
            1.0
        } else {
            0.0
        }
    }

    /// The state drawn: the current one with the effect applied.
    pub fn with_effects(&mut self) -> TweenState {
        let mut s = self.current;
        if self.effect == Effect::None {
            return s;
        }
        let p = self.percent_through_effect();
        let blink_on = p > 0.5;
        let between = ((p + 0.25) * 2.0 * PI).sin() / 2.0 + 0.5;
        // `fOriginalAlpha`: the upper left corner's.
        let alpha = s.diffuse[0][3];
        let (c1, c2) = (self.effect_color1, self.effect_color2);
        let mix = |t: f32| -> Rgba { std::array::from_fn(|i| c1[i] * t + c2[i] * (1.0 - t)) };
        // Every corner the effect's color, at the actor's alpha.
        let corners = |mut c: Rgba| {
            c[3] *= alpha;
            [c; 4]
        };
        match self.effect {
            Effect::None | Effect::Spin => {}
            Effect::DiffuseBlink => s.diffuse = corners(if blink_on { c1 } else { c2 }),
            Effect::DiffuseShift => s.diffuse = corners(mix(between)),
            Effect::DiffuseRamp => s.diffuse = corners(mix(p)),
            Effect::GlowBlink => {
                s.glow = if blink_on { c1 } else { c2 };
                s.glow[3] *= alpha;
            }
            Effect::GlowShift => {
                s.glow = mix(between);
                s.glow[3] *= alpha;
            }
            Effect::GlowRamp => {
                s.glow = mix(p);
                s.glow[3] *= alpha;
            }
            Effect::Rainbow => {
                let a = between * 2.0 * PI;
                s.diffuse = [[
                    a.cos() * 0.5 + 0.5,
                    (a + PI * 2.0 / 3.0).cos() * 0.5 + 0.5,
                    (a + PI * 4.0 / 3.0).cos() * 0.5 + 0.5,
                    alpha,
                ]; 4];
            }
            Effect::Wag => {
                let k = (p * 2.0 * PI).sin();
                for i in 0..3 {
                    s.rotation[i] += self.magnitude[i] * k;
                }
            }
            Effect::Vibrate => {
                let zoom = self.current.scale[0];
                for i in 0..3 {
                    let r = self.random() * 2.0 - 1.0;
                    s.pos[i] += self.magnitude[i] * r * zoom;
                }
            }
            Effect::Bounce => {
                let k = (p * PI).sin();
                for i in 0..3 {
                    s.pos[i] += self.magnitude[i] * k;
                }
            }
            Effect::Bob => {
                let k = (p * PI * 2.0).sin();
                for i in 0..3 {
                    s.pos[i] += self.magnitude[i] * k;
                }
            }
            Effect::Pulse => {
                let k = (p * PI).sin();
                let zoom = lerp(k, self.magnitude[0], self.magnitude[1]);
                for (i, scale) in s.scale.iter_mut().enumerate() {
                    *scale *= zoom * lerp(k, c1[i], c2[i]);
                }
            }
        }
        s
    }

    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    // Updates.

    /// `Actor::Update` for `dt` seconds of the actor's timer, at `clocks`.
    pub fn update(&mut self, dt: f32, clocks: &Clocks, problems: &mut Problems) {
        problems.calls = 0;
        self.update_depth(dt.max(0.0), clocks, problems, 0);
    }

    fn update_depth(&mut self, dt: f32, clocks: &Clocks, problems: &mut Problems, depth: u32) {
        if depth > MAX_DEPTH {
            return;
        }
        let Some(dt) = self.update_begin(dt, clocks) else {
            return;
        };
        let mut tweens = TweenSteps::new(dt);
        while let Some(c) = self.tween_step(&mut tweens, problems) {
            self.play_depth(&c, problems, depth + 1);
        }
        self.update_end(dt);
        if let Kind::Frame(children) = &mut self.kind {
            for c in children {
                c.update_depth(dt, clocks, problems, depth + 1);
            }
        }
    }

    /// The start of this actor's own `Update` (not its children's): the
    /// hibernation, a frame's update rate, the effect clock. The seconds
    /// left for its tweens, its animation and its children, or `None`
    /// while it hibernates.
    ///
    /// A host that plays queued commands itself (a skin's Lua) updates an
    /// actor as [`Actor::update`] does: this, [`Actor::tween_step`] until
    /// it returns `None` (playing each command it returns), then
    /// [`Actor::update_end`], then the children.
    pub fn update_begin(&mut self, dt: f32, clocks: &Clocks) -> Option<f32> {
        let mut dt = dt.max(0.0);
        if self.hibernate > 0.0 {
            self.hibernate -= dt;
            if self.hibernate > 0.0 {
                return None;
            }
            dt = -self.hibernate;
            self.hibernate = 0.0;
        }
        if let Kind::Frame(_) = self.kind {
            dt *= self.update_rate;
        }
        // `UpdateInternal`: the effect clock.
        let global = |now: f32, a: &mut Actor| {
            a.effect_delta = now - a.secs_into_effect;
            a.secs_into_effect = now;
        };
        match self.clock {
            Clock::Timer => {
                self.secs_into_effect += dt;
                self.effect_delta = dt;
                let period = self.effect_period();
                if self.secs_into_effect > period {
                    self.secs_into_effect -= period;
                }
            }
            Clock::TimerGlobal => global(clocks.timer, self),
            Clock::Beat => global(clocks.beat, self),
            Clock::Music => global(clocks.music_seconds, self),
            Clock::BeatNoOffset => global(clocks.beat_no_offset, self),
            Clock::MusicNoOffset => global(clocks.music_seconds_no_offset, self),
        }
        if self.effect == Effect::Spin {
            for i in 0..3 {
                self.current.rotation[i] = (self.current.rotation[i]
                    + self.effect_delta * self.magnitude[i])
                    .rem_euclid(360.0);
            }
        }
        Some(dt)
    }

    /// `Actor::UpdateTweening` up to the next queued command that begins:
    /// the command (to play now, after which the tweening goes on), or
    /// `None` once `steps`' seconds are used up.
    pub fn tween_step(
        &mut self,
        steps: &mut TweenSteps,
        problems: &mut Problems,
    ) -> Option<String> {
        while !self.tweens.is_empty() && steps.dt > 0.0 {
            steps.guard += 1;
            if steps.guard > 1000 {
                problems.push(&self.name, "tweens queue themselves without end".into());
                self.stop_tweening();
                return None;
            }
            let t = &mut self.tweens[0];
            let beginning = t.left == t.time;
            let step = t.left.min(steps.dt);
            t.left -= step;
            steps.dt -= step;
            let command = if beginning { t.command.take() } else { None };
            if beginning {
                self.start = self.current;
            }
            let t = &self.tweens[0];
            if t.left == 0.0 {
                self.current = t.dest;
                self.tweens.remove(0);
            } else {
                let through = 1.0 - t.left / t.time;
                self.current = TweenState::between(&self.start, &t.dest, t.ease.apply(through));
            }
            if command.is_some() {
                return command;
            }
        }
        None
    }

    /// The end of this actor's own `Update`: a sprite's or a model's
    /// animation.
    pub fn update_end(&mut self, dt: f32) {
        let effect_delta = self.effect_delta;
        // `Model::Update`: the update's own time, paused or not.
        if let Kind::Model(model) = &mut self.kind {
            model.advance(dt);
        }
        if let Kind::Sprite(sprite) = &mut self.kind
            && self.animating
        {
            sprite.secs_into_state += effect_delta;
            if sprite.secs_into_state < 0.0 {
                let length = sprite.animation_length();
                if length > 0.0 {
                    sprite.secs_into_state = sprite.secs_into_state.rem_euclid(length);
                }
            }
            sprite.advance();
            let d = if sprite.use_effect_clock_for_texcoords {
                effect_delta
            } else {
                dt
            };
            for i in 0..2 {
                let v = sprite.texcoord_offset[i] + d * sprite.texcoord_velocity[i];
                sprite.texcoord_offset[i] = v - v.floor();
            }
        }
    }

    // Commands.

    /// `playcommand`: this actor's command `name`, then its children's.
    pub fn play(&mut self, name: &str, problems: &mut Problems) {
        problems.calls = 0;
        self.play_depth(name, problems, 0);
    }

    /// This actor's own command only (`PlayCommandNoRecurse`), as `Init`.
    pub fn play_own(&mut self, name: &str, problems: &mut Problems) {
        problems.calls = 0;
        self.play_own_depth(name, problems, 0);
    }

    fn play_own_depth(&mut self, name: &str, problems: &mut Problems, depth: u32) {
        if depth > MAX_DEPTH {
            problems.push(&self.name, format!("{name} plays itself without end"));
            return;
        }
        let Some(calls) = self
            .commands
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| Arc::clone(c))
        else {
            return;
        };
        for call in calls.iter() {
            if !problems.spend(&self.name) {
                return;
            }
            if let Err(e) = self.call(call, problems, depth) {
                // A Lua error stops the rest of the command.
                problems.push(&self.name, format!("{name}: {e}"));
                break;
            }
        }
    }

    fn play_depth(&mut self, name: &str, problems: &mut Problems, depth: u32) {
        self.play_own_depth(name, problems, depth);
        if let Kind::Frame(children) = &mut self.kind {
            for c in children {
                c.play_depth(name, problems, depth + 1);
            }
        }
    }

    /// The native work method `method` does on this actor beyond a fixed
    /// cost, in actors and tweens visited and states written (the Lua host
    /// charges fuel for it).
    pub fn work(&self, method: &str) -> usize {
        fn tree(a: &Actor) -> usize {
            1 + a.tweens.len() + a.children().iter().map(tree).sum::<usize>()
        }
        match method {
            "finishtweening" | "hurrytweening" | "SortByDrawOrder" => tree(self),
            "SetAllStateDelays" => match &self.kind {
                Kind::Sprite(s) => s.states.len(),
                _ => 0,
            },
            _ => 0,
        }
    }

    /// Runs one method call.
    pub fn call(&mut self, call: &Call, problems: &mut Problems, depth: u32) -> Result<(), String> {
        let a = &call.args;
        let num = |i: usize| -> Result<f32, String> {
            match a.get(i) {
                Some(Arg::Num(n)) => Ok(*n),
                // Lua converts numeric strings.
                Some(Arg::Str(s)) => s
                    .trim()
                    .parse()
                    .map_err(|_| format!("{}: argument {} is not a number", call.method, i + 1)),
                _ => Err(format!(
                    "{}: argument {} is not a number",
                    call.method,
                    i + 1
                )),
            }
        };
        let text = |i: usize| -> Result<String, String> {
            match a.get(i) {
                Some(Arg::Str(s)) => Ok(s.clone()),
                Some(Arg::Num(n)) => Ok(n.to_string()),
                _ => Err(format!(
                    "{}: argument {} is not a string",
                    call.method,
                    i + 1
                )),
            }
        };
        // `BIArg`: a boolean, or a number (non-zero is true).
        let flag = |i: usize| -> Result<bool, String> {
            match a.get(i) {
                Some(Arg::Bool(b)) => Ok(*b),
                Some(Arg::Num(n)) => Ok(*n != 0.0),
                Some(Arg::Nil) | None => Ok(false),
                _ => Err(format!(
                    "{}: argument {} is not a boolean",
                    call.method,
                    i + 1
                )),
            }
        };
        // `RageColor::FromStackCompat`: a color table, or four numbers.
        let color = |i: usize| -> Result<Rgba, String> {
            match a.get(i) {
                Some(Arg::Color(c)) => Ok(*c),
                _ => Ok([num(i)?, num(i + 1)?, num(i + 2)?, num(i + 3)?]),
            }
        };
        // A negative time is reported and the call does nothing; the rest
        // of the command goes on (`LunaActor::linear` and the like).
        let tween_time = || -> Result<Option<f32>, String> {
            let t = num(0)?;
            Ok((t >= 0.0).then_some(t))
        };
        let negative = |problems: &mut Problems, name: &str| {
            problems.push(
                name,
                format!("{}: tween time must not be negative", call.method),
            );
        };
        match call.method.as_str() {
            // Tweens.
            "linear" | "accelerate" | "decelerate" | "spring" | "smooth" | "bouncebegin"
            | "bounceend" | "drop" | "sleep" => {
                let Some(t) = tween_time()? else {
                    negative(problems, &self.name);
                    return Ok(());
                };
                match call.method.as_str() {
                    "linear" => self.begin_tweening(t, Ease::Linear),
                    "accelerate" => self.begin_tweening(t, Ease::Accelerate),
                    "decelerate" => self.begin_tweening(t, Ease::Decelerate),
                    "spring" => self.begin_tweening(t, Ease::Spring),
                    "smooth" => self.begin_tweening(t, Ease::Bezier1D(SMOOTH)),
                    "bouncebegin" => self.begin_tweening(t, Ease::Bezier2D(BOUNCE_BEGIN)),
                    "bounceend" => self.begin_tweening(t, Ease::Bezier2D(BOUNCE_END)),
                    "drop" => self.begin_tweening(t, Ease::Bezier2D(DROP)),
                    _ => self.sleep(t),
                }
            }
            "stoptweening" => self.stop_tweening(),
            // Frames pass these on to their children
            // (`PropagateActorFrameCommand`, `ActorFrame.cpp`).
            "finishtweening" => self.each(&mut |a| a.finish_tweening()),
            "hurrytweening" => {
                let f = num(0)?;
                if f < 0.0 {
                    return Err(format!("hurrytweening({f}): the factor cannot be negative"));
                }
                self.each(&mut |a| a.hurry_tweening(f));
            }
            // Position, rotation, zoom.
            "x" => self.dest().pos[0] = num(0)?,
            "y" => self.dest().pos[1] = num(0)?,
            "z" => self.dest().pos[2] = num(0)?,
            "xy" => {
                let (x, y) = (num(0)?, num(1)?);
                let d = self.dest();
                d.pos[0] = x;
                d.pos[1] = y;
            }
            "addx" => self.dest().pos[0] += num(0)?,
            "addy" => self.dest().pos[1] += num(0)?,
            "addz" => self.dest().pos[2] += num(0)?,
            // `SetZoom`: all three.
            "zoom" => {
                let z = num(0)?;
                let d = self.dest();
                d.scale = [z; 3];
            }
            "zoomx" => self.dest().scale[0] = num(0)?,
            "zoomy" => self.dest().scale[1] = num(0)?,
            "zoomz" => self.dest().scale[2] = num(0)?,
            "zoomto" => {
                let (w, h) = (num(0)?, num(1)?);
                let size = self.size();
                let d = self.dest();
                if size[0] != 0.0 {
                    d.scale[0] = w / size[0];
                }
                if size[1] != 0.0 {
                    d.scale[1] = h / size[1];
                }
            }
            // `ZoomToWidth`, `ZoomToHeight`: one side's zoom only.
            "zoomtowidth" | "zoomtoheight" => {
                let i = usize::from(call.method == "zoomtoheight");
                let (v, size) = (num(0)?, self.size());
                if size[i] != 0.0 {
                    self.dest().scale[i] = v / size[i];
                }
            }
            // `StretchTo` (a rectangle's corners): placed at its centre,
            // zoomed on each axis to fill it; `ScaleToCover` and
            // `ScaleToFitInside`: one zoom, the larger or the smaller,
            // turned half round on an axis whose side is negative
            // (`Actor::ScaleTo`).
            "stretchto" | "scaletocover" | "scaletofit" => {
                let (l, t, r, b) = (num(0)?, num(1)?, num(2)?, num(3)?);
                let (w, h) = (r - l, b - t);
                let size = self.size();
                let zoom = [w / size[0], h / size[1]].map(|z| if z.is_finite() { z } else { 0.0 });
                let stretch = call.method == "stretchto";
                let d = self.dest();
                d.pos[0] = l + w / 2.0;
                d.pos[1] = t + h / 2.0;
                if stretch {
                    d.scale[0] = zoom[0];
                    d.scale[1] = zoom[1];
                } else {
                    let [x, y] = zoom.map(f32::abs);
                    let z = if call.method == "scaletocover" {
                        x.max(y)
                    } else {
                        x.min(y)
                    };
                    d.scale = [z; 3];
                    if w < 0.0 {
                        d.rotation[1] = 180.0;
                    }
                    if h < 0.0 {
                        d.rotation[0] = 180.0;
                    }
                }
            }
            // Crops and faded edges (`Actor.h:448-456`): left, top,
            // right, bottom.
            "cropleft" => self.dest().crop[0] = num(0)?,
            "croptop" => self.dest().crop[1] = num(0)?,
            "cropright" => self.dest().crop[2] = num(0)?,
            "cropbottom" => self.dest().crop[3] = num(0)?,
            "fadeleft" => self.dest().fade[0] = num(0)?,
            "fadetop" => self.dest().fade[1] = num(0)?,
            "faderight" => self.dest().fade[2] = num(0)?,
            "fadebottom" => self.dest().fade[3] = num(0)?,
            "rotationx" => self.dest().rotation[0] = num(0)?,
            "rotationy" => self.dest().rotation[1] = num(0)?,
            "rotationz" => self.dest().rotation[2] = num(0)?,
            "addrotationz" => self.dest().rotation[2] += num(0)?,
            "basezoom" => self.base_scale = [num(0)?; 3],
            "basezoomx" => self.base_scale[0] = num(0)?,
            "basezoomy" => self.base_scale[1] = num(0)?,
            "baserotationx" => self.base_rotation[0] = num(0)?,
            "baserotationy" => self.base_rotation[1] = num(0)?,
            "baserotationz" => self.base_rotation[2] = num(0)?,
            // Color, of every corner or of some (`Actor.h:460-472`).
            "diffuse" => self.dest().diffuse = [color(0)?; 4],
            "diffusealpha" => {
                let a = num(0)?;
                for c in &mut self.dest().diffuse {
                    c[3] = a;
                }
            }
            "diffusecolor" => {
                let c = color(0)?;
                for d in &mut self.dest().diffuse {
                    d[..3].copy_from_slice(&c[..3]);
                }
            }
            "diffuseupperleft" | "diffuseupperright" | "diffuselowerleft" | "diffuselowerright"
            | "diffusetopedge" | "diffuserightedge" | "diffusebottomedge" | "diffuseleftedge" => {
                let c = color(0)?;
                let corners: &[usize] = match call.method.as_str() {
                    "diffuseupperleft" => &[0],
                    "diffuseupperright" => &[1],
                    "diffuselowerleft" => &[2],
                    "diffuselowerright" => &[3],
                    "diffusetopedge" => &[0, 1],
                    "diffuserightedge" => &[1, 3],
                    "diffusebottomedge" => &[2, 3],
                    _ => &[0, 2],
                };
                let d = self.dest();
                for &i in corners {
                    d.diffuse[i] = c;
                }
            }
            "glow" => self.dest().glow = color(0)?,
            "basealpha" => self.base_alpha = num(0)?,
            "visible" => {
                // `BIArg` on nil is a Lua error.
                if matches!(a.first(), None | Some(Arg::Nil)) {
                    return Err("visible: argument 1 is not a boolean".into());
                }
                self.visible = flag(0)?
            }
            "blend" => {
                self.blend = match text(0)?.as_str() {
                    "BlendMode_Normal" => Blend::Normal,
                    "BlendMode_Add" => Blend::Add,
                    "BlendMode_Subtract" => Blend::Subtract,
                    "BlendMode_Modulate" => Blend::Modulate,
                    "BlendMode_NoEffect" => Blend::NoEffect,
                    // The rest of `BlendModeNames` (`src/RageTypes.cpp:118-155`).
                    "BlendMode_CopySrc" => Blend::CopySrc,
                    "BlendMode_AlphaMask" => Blend::AlphaMask,
                    "BlendMode_AlphaKnockOut" => Blend::AlphaKnockOut,
                    "BlendMode_AlphaMultiply" => Blend::AlphaMultiply,
                    "BlendMode_WeightedMultiply" => Blend::WeightedMultiply,
                    "BlendMode_InvertDest" => Blend::InvertDest,
                    s if s.starts_with("BlendMode_") => Blend::Other,
                    s => return Err(format!("blend: {s} is not a blend mode")),
                }
            }
            "horizalign" => {
                self.halign = match text(0)?.as_str() {
                    "HorizAlign_Left" => 0.0,
                    "HorizAlign_Center" => 0.5,
                    "HorizAlign_Right" => 1.0,
                    s => return Err(format!("horizalign: {s} is not an alignment")),
                }
            }
            "vertalign" => {
                self.valign = match text(0)?.as_str() {
                    "VertAlign_Top" => 0.0,
                    "VertAlign_Middle" => 0.5,
                    "VertAlign_Bottom" => 1.0,
                    s => return Err(format!("vertalign: {s} is not an alignment")),
                }
            }
            "halign" => self.halign = num(0)?,
            "valign" => self.valign = num(0)?,
            // Effects (`LunaActor`'s defaults).
            "diffuseblink" => {
                self.set_color_effect(Effect::DiffuseBlink, 1.0, [0.5, 0.5, 0.5, 0.5], [1.0; 4])
            }
            "diffuseshift" => {
                self.set_color_effect(Effect::DiffuseShift, 1.0, [0.0, 0.0, 0.0, 1.0], [1.0; 4])
            }
            "diffuseramp" => {
                self.set_color_effect(Effect::DiffuseRamp, 1.0, [0.0, 0.0, 0.0, 1.0], [1.0; 4])
            }
            "glowblink" => self.set_color_effect(
                Effect::GlowBlink,
                1.0,
                [1.0, 1.0, 1.0, 0.2],
                [1.0, 1.0, 1.0, 0.8],
            ),
            "glowshift" => self.set_color_effect(
                Effect::GlowShift,
                1.0,
                [1.0, 1.0, 1.0, 0.2],
                [1.0, 1.0, 1.0, 0.8],
            ),
            "glowramp" => self.set_color_effect(
                Effect::GlowRamp,
                1.0,
                [1.0, 1.0, 1.0, 0.2],
                [1.0, 1.0, 1.0, 0.8],
            ),
            "rainbow" => {
                self.switch_effect(Effect::Rainbow);
                self.set_effect_period(2.0);
            }
            "wag" => {
                self.switch_effect(Effect::Wag);
                self.set_effect_period(2.0);
                self.magnitude = [0.0, 0.0, 20.0];
            }
            "bounce" => {
                // `SetEffectBounce` always restarts.
                self.effect = Effect::Bounce;
                self.set_effect_period(2.0);
                self.magnitude = [0.0, 20.0, 0.0];
                self.secs_into_effect = 0.0;
            }
            "bob" => {
                if self.effect != Effect::Bob || self.effect_period() != 2.0 {
                    self.effect = Effect::Bob;
                    self.set_effect_period(2.0);
                    self.secs_into_effect = 0.0;
                }
                self.magnitude = [0.0, 20.0, 0.0];
            }
            "pulse" => {
                self.effect = Effect::Pulse;
                self.set_effect_period(2.0);
                self.magnitude[0] = 0.5;
                self.magnitude[1] = 1.0;
            }
            "spin" => {
                self.effect = Effect::Spin;
                self.magnitude = [0.0, 0.0, 180.0];
            }
            "vibrate" => {
                self.effect = Effect::Vibrate;
                self.magnitude = [10.0; 3];
            }
            // `02 Actor.lua`.
            "thump" | "heartbeat" => {
                let period = match a.first() {
                    None | Some(Arg::Nil) => 1.0,
                    _ => num(0)?,
                };
                self.effect = Effect::Pulse;
                self.set_effect_period(2.0);
                self.timing = if call.method == "thump" {
                    // `effecttiming(0,0,0.75p,0.25p)`: hold at zero is the
                    // fourth argument.
                    [0.0, 0.0, 0.75 * period, 0.0, 0.25 * period]
                } else {
                    [0.0, 0.125 * period, 0.125 * period, 0.0, 0.75 * period]
                };
                self.magnitude = [1.0, 1.125, 1.0];
            }
            "stopeffect" => self.effect = Effect::None,
            "effectcolor1" => self.effect_color1 = color(0)?,
            "effectcolor2" => self.effect_color2 = color(0)?,
            "effectperiod" => {
                let p = num(0)?;
                if p > 0.0 {
                    self.set_effect_period(p);
                } else {
                    problems.push(
                        &self.name,
                        format!("effect period ({p}) must be positive; ignoring"),
                    );
                }
            }
            "effecttiming" => {
                // (ramp to half, hold at half, ramp to full, hold at zero
                // [, hold at full]).
                let (rth, hah, rtf, haz) = (num(0)?, num(1)?, num(2)?, num(3)?);
                let haf = match a.get(4) {
                    Some(Arg::Num(n)) => *n,
                    _ => 0.0,
                };
                let t = [rth, hah, rtf, haf, haz];
                if t.iter().any(|&v| v < 0.0) {
                    return Err("effect timings must not be negative".into());
                }
                if t.iter().all(|&v| v <= 0.0) {
                    return Err("effect timings must not all be zero".into());
                }
                self.timing = t;
            }
            "effect_hold_at_full" => {
                let haf = num(0)?;
                if haf < 0.0 {
                    return Err("effect timings must not be negative".into());
                }
                self.timing[3] = haf;
            }
            "effectoffset" => self.effect_offset = num(0)?,
            // Not a StepMania 5 method: only StepMania 3.9 skins' commands
            // call it (`crate::commands::parse_legacy`).
            "effectdelay" => self.effect_delay = num(0)?,
            "effectclock" => {
                let s = text(0)?.to_ascii_lowercase();
                self.clock = match s.as_str() {
                    "timer" => Clock::Timer,
                    "timerglobal" => Clock::TimerGlobal,
                    "beat" | "bgm" => Clock::Beat,
                    "music" => Clock::Music,
                    "musicnooffset" => Clock::MusicNoOffset,
                    "beatnooffset" => Clock::BeatNoOffset,
                    // Cabinet lights: never lit here.
                    _ => {
                        problems.push(&self.name, format!("effect clock {s} is not supported"));
                        self.clock
                    }
                };
            }
            "effectmagnitude" => self.magnitude = [num(0)?, num(1)?, num(2)?],
            // Commands.
            "playcommand" => {
                let name = text(0)?;
                self.play_depth(&name, problems, depth + 1);
            }
            "queuecommand" => self.queue_command(&text(0)?),
            "hibernate" => self.hibernate = num(0)?,
            // Sprite and animation.
            "animate" => self.animating = flag(0)?,
            "play" => self.animating = true,
            "pause" => self.animating = false,
            "setstate" => {
                let i = num(0)? as i64;
                match &mut self.kind {
                    Kind::Sprite(s) => s.set_state(i),
                    Kind::Model(m) => m.state = Some(i.max(0) as usize),
                    _ => {}
                }
            }
            // `PlayAnimation`: "default" is the one animation a model has;
            // playing it again starts it over.
            "playanimation" => {
                let name = text(0)?;
                let rate = num(1)?;
                if let Kind::Model(m) = &mut self.kind
                    && name == "default"
                {
                    m.frame = 0.0;
                    m.rate = rate;
                }
            }
            "SetDefaultAnimation" => {
                let name = text(0)?;
                let rate = a.get(1).map_or(Ok(1.0), |_| num(1))?;
                if let Kind::Model(m) = &mut self.kind {
                    m.default_animation = (!name.is_empty()).then_some((name == "default", rate));
                }
            }
            "rate" => {
                let r = num(0)?;
                if let Kind::Model(m) = &mut self.kind {
                    m.rate = r;
                }
            }
            "loop" => {
                let l = flag(0)?;
                if let Kind::Model(m) = &mut self.kind {
                    m.looping = l;
                }
            }
            "position" => {
                let seconds = num(0)?;
                if let Kind::Model(m) = &mut self.kind {
                    m.frame = (30.0 * seconds).clamp(0.0, m.animation.total_frames as f32);
                }
            }
            "SetAllStateDelays" => {
                let d = num(0)?;
                if let Kind::Sprite(s) = &mut self.kind {
                    for st in &mut s.states {
                        st.delay = d;
                    }
                }
            }
            "SetSecondsIntoAnimation" => {
                let t = num(0)?;
                match &mut self.kind {
                    Kind::Sprite(s) => s.set_seconds_into_animation(t),
                    Kind::Model(m) => {
                        m.seconds = t;
                        m.state = None;
                    }
                    _ => {}
                }
            }
            // Our extension, not StepMania's.
            "ellipsis" => {
                let on = flag(0)?;
                if let Kind::Text(t) = &mut self.kind
                    && t.ellipsis != on
                {
                    t.ellipsis = on;
                    t.metrics = None;
                }
            }
            // Our extensions (`Actor::motion_blur`, `Shape`), not
            // StepMania's.
            "motionblur" => self.motion_blur = num(0)?.max(0.0),
            "cornerradius" | "softedge" | "outline" | "saturation" => {
                let v = num(0)?;
                if let Kind::Sprite(sp) = &mut self.kind {
                    let shape = &mut sp.shape;
                    match call.method.as_str() {
                        "cornerradius" => shape.radius = v.max(0.0),
                        "softedge" => shape.soft = v.max(0.0),
                        "outline" => shape.outline = v.max(0.0),
                        _ => shape.saturation = v.clamp(0.0, 1.0),
                    }
                }
            }
            "texcoordvelocity" => {
                let v = [num(0)?, num(1)?];
                if let Kind::Sprite(s) = &mut self.kind {
                    // Padded-texture units, as StepMania's coordinates are.
                    s.texcoord_velocity =
                        [v[0] * s.padded_to_image[0], v[1] * s.padded_to_image[1]];
                    // `SetCustomTextureCoords`: the frame stays as it is.
                    if v != [0.0; 2] && s.custom.is_none() {
                        s.custom = Some(s.states[s.current].rect);
                    }
                }
            }
            "set_use_effect_clock_for_texcoords" => {
                let b = flag(0)?;
                if let Kind::Sprite(s) = &mut self.kind {
                    s.use_effect_clock_for_texcoords = b;
                }
            }
            "customtexturerect" => {
                let r = [num(0)?, num(1)?, num(2)?, num(3)?];
                if let Kind::Sprite(s) = &mut self.kind {
                    let k = s.padded_to_image;
                    s.custom = Some([r[0] * k[0], r[1] * k[1], r[2] * k[0], r[3] * k[1]]);
                }
            }
            "setsize" => {
                let size = [num(0)?, num(1)?];
                if let Kind::Sprite(s) = &mut self.kind {
                    s.size = size;
                }
            }
            "SetWidth" | "SetHeight" => {
                let v = num(0)?;
                if let Kind::Sprite(s) = &mut self.kind {
                    s.size[usize::from(call.method == "SetHeight")] = v;
                }
            }
            "SetUpdateRate" => {
                let r = num(0)?;
                if r <= 0.0 {
                    return Err(format!(
                        "SetUpdateRate({r}): the rate must be greater than 0"
                    ));
                }
                self.update_rate = r;
            }
            // Drawing order. Frames' alone in StepMania, any actor's here.
            "draworder" => self.draw_order = num(0)? as i32,
            "SortByDrawOrder" => {
                self.sort_by_draw_order();
            }
            // `BArg`: Lua's truth, so 0 is true.
            "SetDrawByZPosition" => {
                self.draw_by_z = !matches!(a.first(), None | Some(Arg::Nil | Arg::Bool(false)))
            }
            // Shadows (`Actor.h:571-574`).
            "shadowlength" => self.shadow = [num(0)?; 2],
            "shadowlengthx" => self.shadow[0] = num(0)?,
            "shadowlengthy" => self.shadow[1] = num(0)?,
            "shadowcolor" => self.shadow_color = color(0)?,
            // Perspective (`ActorFrame.cpp:628-641`).
            "fov" | "SetFOV" => self.fov = num(0)?,
            "vanishpoint" => self.vanish = Some([num(0)?, num(1)?]),
            // Depth: StepMania's masks (`Actor.cpp:1834-1840`; `MaskSource`
            // and `MaskDest`, `02 Actor.lua:234-250`).
            "zbuffer" => {
                let on = flag(0)?;
                self.depth.test = if on { ZTest::WriteOnPass } else { ZTest::Off };
                self.depth.write = on;
            }
            "ztest" => {
                self.depth.test = if flag(0)? {
                    ZTest::WriteOnPass
                } else {
                    ZTest::Off
                }
            }
            "ztestmode" => {
                self.depth.test = match text(0)?.as_str() {
                    "ZTestMode_Off" => ZTest::Off,
                    "ZTestMode_WriteOnPass" => ZTest::WriteOnPass,
                    "ZTestMode_WriteOnFail" => ZTest::WriteOnFail,
                    s => return Err(format!("ztestmode: {s} is not a z test mode")),
                }
            }
            "zwrite" => self.depth.write = flag(0)?,
            "clearzbuffer" => self.depth.clear = flag(0)?,
            "MaskSource" => {
                // `noclear`, despite its name, clears when true.
                if matches!(a.first(), Some(Arg::Bool(true))) {
                    self.depth.clear = true;
                }
                self.depth.write = true;
                self.blend = Blend::NoEffect;
            }
            "MaskDest" => self.depth.test = ZTest::WriteOnPass,
            // Text (ours).
            "settext" => {
                let s = text(0)?;
                if let Kind::Text(t) = &mut self.kind {
                    t.set_text(&s);
                }
            }
            "maxwidth" => {
                let w = num(0)?;
                if let Kind::Text(t) = &mut self.kind {
                    if t.ellipsis && t.max_width != w {
                        t.metrics = None;
                    }
                    t.max_width = w;
                }
            }
            // Drawn the same here.
            "SetTextureFiltering" | "cullmode" => {}
            other => return Err(format!("{other} is not supported")),
        }
        Ok(())
    }

    /// What getter `method` answers (`Actor`'s and `Sprite`'s Lua
    /// methods that read state, `src/Actor.cpp`, `src/Sprite.cpp`); `None`
    /// for anything else. Positions, zooms and colors are the destination
    /// of the tweens queued so far, as `DestTweenState` is.
    pub fn getter(&self, method: &str) -> Option<Value> {
        let d = self.tweens.last().map_or(&self.current, |t| &t.dest);
        let size = self.size();
        let zoom = |i: usize| d.scale[i] * self.base_scale[i];
        Some(match method {
            "GetName" => Value::Str(self.name.clone()),
            "GetX" => Value::Num(d.pos[0]),
            "GetY" => Value::Num(d.pos[1]),
            "GetZ" => Value::Num(d.pos[2]),
            "GetZoom" | "GetZoomX" => Value::Num(d.scale[0]),
            "GetZoomY" => Value::Num(d.scale[1]),
            "GetZoomZ" => Value::Num(d.scale[2]),
            "GetRotationX" => Value::Num(d.rotation[0]),
            "GetRotationY" => Value::Num(d.rotation[1]),
            "GetCropLeft" => Value::Num(d.crop[0]),
            "GetCropTop" => Value::Num(d.crop[1]),
            "GetCropRight" => Value::Num(d.crop[2]),
            "GetCropBottom" => Value::Num(d.crop[3]),
            "GetRotationZ" => Value::Num(d.rotation[2]),
            "GetBaseZoomX" => Value::Num(self.base_scale[0]),
            "GetBaseZoomY" => Value::Num(self.base_scale[1]),
            "GetBaseZoomZ" => Value::Num(self.base_scale[2]),
            "GetBaseRotationX" => Value::Num(self.base_rotation[0]),
            "GetBaseRotationY" => Value::Num(self.base_rotation[1]),
            "GetBaseRotationZ" => Value::Num(self.base_rotation[2]),
            "GetDiffuse" => Value::Color(d.diffuse[0]),
            "GetDiffuseAlpha" => Value::Num(d.diffuse[0][3]),
            "GetGlow" => Value::Color(d.glow),
            "GetVisible" => Value::Bool(self.visible),
            "GetWidth" => Value::Num(size[0]),
            "GetHeight" => Value::Num(size[1]),
            "GetZoomedWidth" => Value::Num(size[0] * zoom(0)),
            "GetZoomedHeight" => Value::Num(size[1] * zoom(1)),
            "GetTweenTimeLeft" => Value::Num(self.tweens.iter().map(|t| t.left).sum()),
            "GetSecsIntoEffect" => Value::Num(self.secs_into_effect),
            "GetEffectDelta" => Value::Num(self.effect_delta),
            "GetEffectMagnitude" => {
                return Some(Value::Many(self.magnitude.map(Value::Num).to_vec()));
            }
            "GetHAlign" => Value::Num(self.halign),
            "GetVAlign" => Value::Num(self.valign),
            "GetNumStates" => Value::Num(match &self.kind {
                Kind::Sprite(s) => s.states.len() as f32,
                Kind::Model(m) => m.num_states() as f32,
                _ => 0.0,
            }),
            "GetState" => Value::Num(match &self.kind {
                Kind::Sprite(s) => s.current as f32,
                _ => 0.0,
            }),
            "GetAnimationLengthSeconds" => Value::Num(match &self.kind {
                Kind::Sprite(s) => s.animation_length(),
                Kind::Model(m) => m.animation_length(),
                _ => 0.0,
            }),
            "GetNumChildren" => Value::Num(self.children().len() as f32),
            _ => return None,
        })
    }

    /// The size the actor is drawn at (a sprite's frame).
    pub fn size(&self) -> [f32; 2] {
        match &self.kind {
            Kind::Sprite(s) => s.size,
            Kind::Text(t) => t.size(),
            _ => [0.0; 2],
        }
    }

    // Drawing.

    /// Appends what the actor draws at `transform` (its parent's), under the
    /// parent's diffuse and glow (`ActorFrame::DrawPrimitives` hands them
    /// down; `Actor::PreDraw` multiplies and screens them in), on
    /// StepMania's 16:9 screen drawn as it is ([`View::default`]).
    pub fn draw(&mut self, transform: [f32; 6], diffuse: Rgba, glow: Rgba, out: &mut Vec<Drawn>) {
        self.draw_in(&View::default(), transform, diffuse, glow, out);
    }

    /// [`Actor::draw`] on `view`, which `fov` frames' cameras and shadows'
    /// offsets are measured on. Text is drawn as last measured
    /// ([`Actor::measure`]).
    pub fn draw_in(
        &mut self,
        view: &View,
        transform: [f32; 6],
        diffuse: Rgba,
        glow: Rgba,
        out: &mut Vec<Drawn>,
    ) {
        // The same placement in 3D: the 2D part, depth scaled alike.
        let t = transform;
        let k = scale_of(&t);
        let m3: Mat4 = [
            t[0], t[1], 0.0, 0.0, t[2], t[3], 0.0, 0.0, 0.0, 0.0, k, 0.0, t[4], t[5], 0.0, 1.0,
        ];
        let under = Under {
            m: transform,
            m3,
            diffuse,
            glow,
            camera: FrameCamera::Drawer,
            scissor: None,
            blur: [0.0; 2],
        };
        self.draw_depth(&under, view, out, 0);
    }

    fn draw_depth(&mut self, under: &Under, view: &View, out: &mut Vec<Drawn>, depth: u32) {
        if !self.visible || self.hibernate > 0.0 || depth > MAX_DEPTH {
            return;
        }
        let mut s = self.with_effects();
        let mut internal = under.diffuse;
        internal[3] *= self.base_alpha;
        if internal != [1.0; 4] {
            for corner in &mut s.diffuse {
                for (d, k) in corner.iter_mut().zip(internal) {
                    *d *= k;
                }
            }
        }
        if under.glow[3] > 0.0 {
            // Screen.
            for (g, k) in s.glow.iter_mut().zip(under.glow) {
                *g = *g + k - k * *g;
            }
        }
        // `PartiallyOpaque`.
        let opaque = s.diffuse.iter().any(|c| c[3] > 0.0);
        if !(opaque || s.glow[3] > 0.0) {
            return;
        }
        let Placement {
            rot,
            scale,
            size,
            align,
            local,
        } = self.placement(&s);
        let m = mul(&under.m, &local);
        // In 3D (models, perspective, and what they are drawn under): the
        // same order, the rotation `Rx(-x)·Ry(-y)·Rz(z)`
        // (`RageMatrixRotationXYZ`).
        let local3 = mul4(
            &mul4(
                &translation4([s.pos[0], s.pos[1], s.pos[2]]),
                &mul4(
                    &mul4(&rotation_x4(-rot[0]), &rotation_y4(-rot[1])),
                    &rotation_z4(rot[2]),
                ),
            ),
            &mul4(
                &scale4([scale[0], scale[1], s.scale[2] * self.base_scale[2]]),
                &translation4([align[0], align[1], 0.0]),
            ),
        );
        let m3 = mul4(&under.m3, &local3);
        // Its own motion blur, along its y on the target, adds to what is
        // around it.
        let blur = if self.motion_blur > 0.0 {
            [
                under.blur[0] + m[2] * self.motion_blur,
                under.blur[1] + m[3] * self.motion_blur,
            ]
        } else {
            under.blur
        };
        // A sprite's or a run's shadow: itself in the shadow's color, moved
        // in the screen's pixels (`TranslateWorld`), drawn first
        // (`Sprite::DrawTexture`, `BitmapText::DrawPrimitives`).
        let shadow = |alpha: f32| {
            let mut c = self.shadow_color;
            c[3] *= alpha;
            (self.shadow != [0.0; 2]).then(|| (view.vector(self.shadow), c))
        };
        let (blend, depth_state, framed) = (self.blend, self.depth, under.camera);
        let camera = framed.perspective();
        match &mut self.kind {
            Kind::Actor => {}
            Kind::Sprite(sprite) => {
                let sized = mul(&m, &[size[0], 0.0, 0.0, size[1], 0.0, 0.0]);
                let sized3 = mul4(&m3, &scale4([size[0], size[1], 1.0]));
                let uv_of =
                    |sprite: &Sprite| sprite.custom.unwrap_or(sprite.states[sprite.current].rect);
                let quad = |transform: Mat,
                            transform3: &Mat4,
                            diffuse,
                            glow,
                            uv: [f32; 4],
                            whole: bool| {
                    let projected = match &camera {
                        Some(c) => Some(Projected::of(c, transform3)?),
                        None => None,
                    };
                    Some(Drawn::Quad(Quad {
                        transform,
                        texture: sprite.texture,
                        cover: sprite.cover,
                        contain: sprite.contain,
                        uv,
                        texcoord_offset: sprite.texcoord_offset,
                        diffuse,
                        glow,
                        blend,
                        depth: depth_state,
                        projected,
                        // Our shapes are of the whole quad: a piece is
                        // StepMania's plain one.
                        shape: if whole {
                            sprite.shape
                        } else {
                            Shape::default()
                        },
                        size,
                        blur,
                    }))
                };
                // `Sprite::DrawPrimitives`: the sprite as it is, or cropped,
                // or its faded edges as pieces of their own around the
                // inside (each drawn as `DrawTexture` draws it).
                for piece in pieces(&s).into_iter().flatten() {
                    let Some((sub, uv)) = piece.cut(&sized, &sized3, uv_of(sprite)) else {
                        continue;
                    };
                    let seen = piece.diffuse.iter().any(|c| c[3] > 0.0);
                    if !seen && s.glow[3] <= 0.0 {
                        continue;
                    }
                    // Any corner seen draws the diffuse pass and the
                    // shadow (`DrawTexture`).
                    if seen && let Some((by, c)) = shadow(piece.diffuse[0][3]) {
                        out.extend(quad(
                            shifted(&sub.0, by),
                            &shifted4(&sub.1, by),
                            [c; 4],
                            [0.0; 4],
                            uv,
                            piece.whole,
                        ));
                    }
                    let diffuse = if seen { piece.diffuse } else { [[0.0; 4]; 4] };
                    out.extend(quad(sub.0, &sub.1, diffuse, s.glow, uv, piece.whole));
                }
            }
            Kind::Model(model) => {
                // MilkShape's y is up (`Model::DrawPrimitives` scales y by
                // -1).
                out.push(Drawn::Model(ModelDraw {
                    transform: mul4(&m3, &scale4([1.0, -1.0, 1.0])),
                    geometry: model.geometry.clone(),
                    materials: model.materials_now(s.diffuse[0]),
                    bones: model.bones.clone(),
                    diffuse: s.diffuse[0],
                    glow: s.glow,
                    blend,
                    depth: depth_state,
                    camera: framed,
                }));
            }
            Kind::Text(text) => {
                let metrics = text.metrics().unwrap_or_default();
                // The pen: the box's left, on the baseline.
                let pen = [-size[0] / 2.0, -size[1] / 2.0 + metrics.baseline];
                let run = mul(&m, &[1.0, 0.0, 0.0, 1.0, pen[0], pen[1]]);
                let run3 = mul4(&m3, &translation4([pen[0], pen[1], 0.0]));
                let draw = |transform, transform3, diffuse, glow| {
                    Drawn::Text(TextDraw {
                        text: text.shown().clone(),
                        style: text.style.clone(),
                        metrics,
                        transform,
                        diffuse,
                        glow,
                        blend,
                        depth: depth_state,
                        perspective: camera.map(|c| (transform3, c)),
                        blur,
                    })
                };
                // `BitmapText::DrawPrimitives` draws nothing but the glow
                // while the upper left corner is transparent.
                let seen = s.diffuse[0][3] != 0.0;
                if seen && let Some((by, c)) = shadow(s.diffuse[0][3]) {
                    out.push(draw(
                        shifted(&run, by),
                        shifted4(&run3, by),
                        [c; 4],
                        [0.0; 4],
                    ));
                }
                if seen || s.glow[3] > 0.0 {
                    out.push(draw(run, run3, s.diffuse, s.glow));
                }
            }
            Kind::Frame(children) => {
                // `ActorFrame::BeginDraw`: a camera of the frame's own for
                // what is inside it.
                let framed = if self.fov != -1.0 {
                    let vanish = self
                        .vanish
                        .unwrap_or([view.size[0] / 2.0, view.size[1] / 2.0]);
                    match Perspective::new(view, self.fov, vanish) {
                        Some(p) => FrameCamera::Perspective(p),
                        None => FrameCamera::Orthographic {
                            depth: 1000.0 * view.scale(),
                        },
                    }
                } else {
                    framed
                };
                let camera = framed.perspective();
                let mut scissor = under.scissor;
                let clipped = match self.clip {
                    Some(r) => {
                        let rect: Mat = [
                            r[2] - r[0],
                            0.0,
                            0.0,
                            r[3] - r[1],
                            (r[0] + r[2]) / 2.0,
                            (r[1] + r[3]) / 2.0,
                        ];
                        let Some(clip) = clip(&mul(&m, &rect), &m3, rect, camera, scissor) else {
                            // Nothing of it can be seen.
                            return;
                        };
                        scissor = Some(clip.scissor);
                        out.push(Drawn::ClipPush(clip));
                        true
                    }
                    None => false,
                };
                let inside = Under {
                    m,
                    m3,
                    diffuse: s.diffuse[0],
                    glow: s.glow,
                    camera: framed,
                    scissor,
                    blur,
                };
                if self.draw_by_z {
                    // `ActorUtil::SortByZPosition`: far first, stable.
                    let mut order: Vec<usize> = (0..children.len()).collect();
                    order.sort_by(|&a, &b| {
                        let (za, zb) = (children[a].current.pos[2], children[b].current.pos[2]);
                        za.partial_cmp(&zb).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    for i in order {
                        children[i].draw_depth(&inside, view, out, depth + 1);
                    }
                } else {
                    for c in children {
                        c.draw_depth(&inside, view, out, depth + 1);
                    }
                }
                if clipped {
                    out.push(Drawn::ClipPop);
                }
            }
        }
    }
}

impl Actor {
    /// Translate, rotate, scale, then the alignment (`BeginDraw`), for
    /// state `s`. The rotation is `RageMatrixRotationXYZ` seen from the
    /// front: x goes to (cZ·cY, sZ·cX) and y to (−sZ·cY, cZ·cX)
    /// (`RageMath.cpp:319-327`).
    fn placement(&self, s: &TweenState) -> Placement {
        let rot = [
            (s.rotation[0] + self.base_rotation[0]).to_radians(),
            (s.rotation[1] + self.base_rotation[1]).to_radians(),
            (s.rotation[2] + self.base_rotation[2]).to_radians(),
        ];
        let (sz, cz) = rot[2].sin_cos();
        let (cx, cy) = (rot[0].cos(), rot[1].cos());
        // `maxwidth` squeezes a run (a base zoom in `BitmapText`).
        let squeeze = match &self.kind {
            Kind::Text(t) => t.squeeze(),
            _ => 1.0,
        };
        let scale = [
            s.scale[0] * self.base_scale[0] * squeeze,
            s.scale[1] * self.base_scale[1],
        ];
        let size = self.size();
        let align = [
            lerp(self.halign, size[0] / 2.0, -size[0] / 2.0),
            lerp(self.valign, size[1] / 2.0, -size[1] / 2.0),
        ];
        let ax = [scale[0] * cy * cz, scale[0] * cx * sz];
        let ay = [-scale[1] * cy * sz, scale[1] * cx * cz];
        let local: Mat = [
            ax[0],
            ax[1],
            ay[0],
            ay[1],
            s.pos[0] + ax[0] * align[0] + ay[0] * align[1],
            s.pos[1] + ax[1] * align[0] + ay[1] * align[1],
        ];
        Placement {
            rot,
            scale,
            size,
            align,
            local,
        }
    }

    /// Where the actor is in its parent now, as [`Actor::draw`] places it
    /// but from the current tween state without effects (a `bob` or a
    /// `vibrate` does not move it here): from its own space, where its box
    /// ([`Actor::size`]) is `[-w/2, w/2] × [-h/2, h/2]` and a frame's
    /// children are placed, to its parent's (`x' = m[0]x + m[2]y + m[4]`,
    /// `y' = m[1]x + m[3]y + m[5]`). Rotations about x and y foreshorten
    /// as drawn; an `fov` frame's camera is not applied. For hit-testing.
    pub fn local_transform(&self) -> [f32; 6] {
        self.placement(&self.current).local
    }

    /// `hibernate`: drawn and updated again once its time is up.
    pub fn hibernating(&self) -> bool {
        self.hibernate > 0.0
    }

    /// Whether an update may change what the actor or anything inside it
    /// draws: tweens queued, an effect, a hibernation to wake from, a
    /// playing sprite's frames or texture scrolling, a model that animates.
    /// A hidden actor counts only its tweens (a queued command may show
    /// it). A screen with nothing in motion need not be drawn again until
    /// something changes it (`docs/plans/ui-core.md`, P13).
    pub fn in_motion(&self) -> bool {
        if !self.tweens.is_empty() {
            return true;
        }
        if !self.visible {
            return false;
        }
        let own = self.effect != Effect::None
            || self.hibernate > 0.0
            || match &self.kind {
                Kind::Sprite(s) => {
                    self.animating
                        && ((s.states.len() > 1 && s.custom.is_none())
                            || s.texcoord_velocity != [0.0; 2])
                }
                Kind::Model(m) => {
                    (m.state.is_none() && m.animation_length() > 0.0)
                        || (!m.geometry.meshes.is_empty()
                            && m.animation.total_frames > 1
                            && m.rate != 0.0
                            && !matches!(m.default_animation, Some((false, _))))
                }
                Kind::Actor | Kind::Frame(_) | Kind::Text(_) => false,
            };
        own || self.children().iter().any(Actor::in_motion)
    }
}

/// A part of a sprite as `Sprite::DrawPrimitives` draws it: the cut away
/// (left, top, right, bottom, fractions of the sprite) and its corners'
/// colors.
#[derive(Clone, Copy)]
struct Piece {
    crop: [f32; 4],
    diffuse: [Rgba; 4],
    /// The sprite whole, uncut.
    whole: bool,
}

impl Piece {
    /// The piece's quad, from the sprite's (`sized`: the unit square on
    /// the target, in 2D and 3D) and its image `uv`; `None` when nothing is
    /// left of it (`DrawTexture` bails when cropped all the way).
    #[allow(clippy::type_complexity)]
    fn cut(&self, sized: &Mat, sized3: &Mat4, uv: [f32; 4]) -> Option<((Mat, Mat4), [f32; 4])> {
        if self.whole {
            return Some(((*sized, *sized3), uv));
        }
        let [l, t, r, b] = self.crop;
        if l + r >= 1.0 || t + b >= 1.0 {
            return None;
        }
        // The vertices' crop is clamped (a negative crop draws no more).
        let [l, t, r, b] = [l, t, r, b].map(|v| v.clamp(0.0, 1.0));
        let (x0, x1, y0, y1) = (-0.5 + l, 0.5 - r, -0.5 + t, 0.5 - b);
        let sub: Mat = [x1 - x0, 0.0, 0.0, y1 - y0, (x0 + x1) / 2.0, (y0 + y1) / 2.0];
        let sub3: Mat4 = [
            sub[0], 0.0, 0.0, 0.0, 0.0, sub[3], 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, sub[4], sub[5], 0.0,
            1.0,
        ];
        let at = |f: f32, a: f32, b: f32| a + (b - a) * f;
        let uv = [
            at(l, uv[0], uv[2]),
            at(t, uv[1], uv[3]),
            at(1.0 - r, uv[0], uv[2]),
            at(1.0 - b, uv[1], uv[3]),
        ];
        Some(((mul(sized, &sub), mul4(sized3, &sub3)), uv))
    }
}

/// `Sprite::DrawPrimitives`: the sprite whole; cropped; or, with faded
/// edges, the inside and each faded edge as a piece of its own, its outer
/// side transparent (the corners where two fades meet are left out, as
/// StepMania lops them off). In drawing order, `None` for the pieces not
/// drawn (no allocation a sprite a frame).
fn pieces(s: &TweenState) -> [Option<Piece>; 5] {
    let (crop, fade, d) = (s.crop, s.fade, s.diffuse);
    if fade.iter().all(|f| *f <= 0.0) {
        let whole = Piece {
            crop,
            diffuse: d,
            whole: crop == [0.0; 4],
        };
        return [Some(whole), None, None, None, None];
    }
    let [fl, ft, fr, fb] = fade;
    let mut size = fade;
    // A crop leaving less than the fades: the fades share what is left.
    let horiz = 1.0 - (crop[0] + crop[2]);
    if fl + fr > 0.0 && horiz < fl + fr {
        let left = fl / (fl + fr);
        size[0] = left * horiz;
        size[2] = (1.0 - left) * horiz;
    }
    let vert = 1.0 - (crop[1] + crop[3]);
    if ft + fb > 0.0 && vert < ft + fb {
        let top = ft / (ft + fb);
        size[1] = top * vert;
        size[3] = (1.0 - top) * vert;
    }
    // The alpha of each fade's inner side: `SCALE(size, dist, 0, 1, 0)`.
    let inner = |size: f32, dist: f32| {
        if dist == 0.0 { 1.0 } else { size / dist }
    };
    let [la, ta, ra, ba] = [
        inner(size[0], fl),
        inner(size[1], ft),
        inner(size[2], fr),
        inner(size[3], fb),
    ];
    // Corners: upper left, upper right, lower left, lower right.
    let (ul, ur, ll, lr) = (0, 1, 2, 3);
    let alpha = |mut c: Rgba, k: f32| {
        c[3] *= k;
        c
    };
    let mut out = [None; 5];
    out[0] = Some(Piece {
        crop: [crop[0] + fl, crop[1] + ft, crop[2] + fr, crop[3] + fb],
        diffuse: d,
        whole: false,
    });
    if size[0] > 0.001 {
        out[1] = Some(Piece {
            crop: [
                crop[0],
                crop[1] + ft,
                1.0 - (crop[0] + size[0]),
                crop[3] + fb,
            ],
            diffuse: [
                alpha(d[ul], 0.0),
                alpha(d[ul], la),
                alpha(d[ll], 0.0),
                alpha(d[ll], la),
            ],
            whole: false,
        });
    }
    if size[2] > 0.001 {
        out[2] = Some(Piece {
            crop: [
                1.0 - (crop[2] + size[2]),
                crop[1] + ft,
                crop[2],
                crop[3] + fb,
            ],
            diffuse: [
                alpha(d[ur], ra),
                alpha(d[ur], 0.0),
                alpha(d[lr], ra),
                alpha(d[lr], 0.0),
            ],
            whole: false,
        });
    }
    if size[1] > 0.001 {
        out[3] = Some(Piece {
            crop: [
                crop[0] + fl,
                crop[1],
                crop[2] + fr,
                1.0 - (crop[1] + size[1]),
            ],
            diffuse: [
                alpha(d[ul], 0.0),
                alpha(d[ur], 0.0),
                alpha(d[ul], ta),
                alpha(d[ur], ta),
            ],
            whole: false,
        });
    }
    if size[3] > 0.001 {
        out[4] = Some(Piece {
            crop: [
                crop[0] + fl,
                1.0 - (crop[3] + size[3]),
                crop[2] + fr,
                crop[3],
            ],
            diffuse: [
                alpha(d[ll], ba),
                alpha(d[lr], ba),
                alpha(d[ll], 0.0),
                alpha(d[lr], 0.0),
            ],
            whole: false,
        });
    }
    out
}

/// An actor's placement in its parent ([`Actor::placement`]).
struct Placement {
    rot: [f32; 3],
    scale: [f32; 2],
    size: [f32; 2],
    align: [f32; 2],
    local: Mat,
}

/// What a frame hands its children to draw under.
struct Under {
    /// To the target, in 2D and in 3D (z toward the viewer).
    m: Mat,
    m3: Mat4,
    diffuse: Rgba,
    glow: Rgba,
    /// The camera the `fov` frames around set.
    camera: FrameCamera,
    /// The clips around, on the target.
    scissor: Option<[f32; 4]>,
    /// The motion blur around, on the target ([`Actor::motion_blur`]).
    blur: [f32; 2],
}

/// `m` moved by `by` on the target.
fn shifted(m: &Mat, by: [f32; 2]) -> Mat {
    [m[0], m[1], m[2], m[3], m[4] + by[0], m[5] + by[1]]
}

fn shifted4(m: &Mat4, by: [f32; 2]) -> Mat4 {
    mul4(&translation4([by[0], by[1], 0.0]), m)
}

/// A frame's clip: the unit square to its rectangle (`rect`, in the
/// frame's space) to the target under `sized` (2D) or, seen by `camera`,
/// under `m3`; within the clips around (`scissor`). `None` when none of it
/// is left.
fn clip(
    sized: &Mat,
    m3: &Mat4,
    rect: Mat,
    camera: Option<Perspective>,
    scissor: Option<[f32; 4]>,
) -> Option<Clip> {
    const EPS: f32 = 1e-5;
    let (corners, exact) = match camera {
        Some(c) => {
            let rect3: Mat4 = [
                rect[0], rect[1], 0.0, 0.0, rect[2], rect[3], 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
                rect[4], rect[5], 0.0, 1.0,
            ];
            (Projected::of(&c, &mul4(m3, &rect3))?.corners, false)
        }
        None => {
            let m = sized;
            let corners = [[-0.5, -0.5], [0.5, -0.5], [-0.5, 0.5], [0.5, 0.5]].map(|p| apply(m, p));
            // Turned by a multiple of 90° at most.
            let straight =
                (m[1].abs() < EPS && m[2].abs() < EPS) || (m[0].abs() < EPS && m[3].abs() < EPS);
            (corners, straight)
        }
    };
    let mut b = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for c in corners {
        b = [
            b[0].min(c[0]),
            b[1].min(c[1]),
            b[2].max(c[0]),
            b[3].max(c[1]),
        ];
    }
    if let Some(s) = scissor {
        b = [
            b[0].max(s[0]),
            b[1].max(s[1]),
            b[2].min(s[2]),
            b[3].min(s[3]),
        ];
    }
    (b[0] < b[2] && b[1] < b[3]).then_some(Clip {
        corners,
        scissor: b,
        exact,
    })
}

/// A column-major 4×4 matrix.
pub type Mat4 = [f32; 16];

/// `a · b`: `b` first.
pub fn mul4(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

fn translation4(t: [f32; 3]) -> Mat4 {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, t[0], t[1], t[2], 1.0,
    ]
}

fn scale4(k: [f32; 3]) -> Mat4 {
    [
        k[0], 0.0, 0.0, 0.0, 0.0, k[1], 0.0, 0.0, 0.0, 0.0, k[2], 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn rotation_x4(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    [
        1.0, 0.0, 0.0, 0.0, 0.0, c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn rotation_y4(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    [
        c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn rotation_z4(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    [
        c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

/// Where `transform` puts the point `p`.
pub fn transform_point(transform: &Mat4, p: [f32; 3]) -> [f32; 3] {
    let m = transform;
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// The identity transform.
pub const TRANSFORM_IDENTITY: [f32; 6] = IDENTITY;

#[cfg(test)]
mod tests {
    #[test]
    fn every_listed_method_is_one_call_runs() {
        for &m in super::METHODS {
            let mut a = super::Actor::new(super::Kind::Frame(Vec::new()));
            let args = vec![super::Arg::Num(1.0); 4];
            let call = super::Call {
                method: m.to_string(),
                args,
            };
            let r = a.call(&call, &mut super::Problems::default(), 0);
            assert!(
                !matches!(&r, Err(e) if e.contains("is not supported")),
                "{m}: {r:?}"
            );
        }
    }

    use super::*;
    use crate::commands::parse;

    fn run(actor: &mut Actor, commands: &str) -> Problems {
        let mut p = Problems::default();
        for c in parse(commands).unwrap() {
            if let Err(e) = actor.call(&c, &mut p, 0) {
                p.messages.push(e);
            }
        }
        p
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn eases_are_stepmanias() {
        assert_eq!(Ease::Accelerate.apply(0.5), 0.25);
        assert_eq!(Ease::Decelerate.apply(0.5), 0.75);
        assert!(close(Ease::Spring.apply(0.0), 0.0));
        assert!(close(Ease::Spring.apply(1.0), 1.0 - (2.5 * PI).cos() / 4.0));
        // `smooth` is smoothstep.
        assert!(close(
            Ease::Bezier1D(SMOOTH).apply(0.25),
            3.0 * 0.0625 - 2.0 * 0.015625
        ));
        assert!(close(Ease::Bezier2D(DROP).apply(0.0), 0.0));
        assert!(close(Ease::Bezier2D(DROP).apply(1.0), 1.0));
        // Linear control points give a straight line.
        assert!(close(
            Ease::Bezier2D([
                0.0,
                0.0,
                1.0 / 3.0,
                1.0 / 3.0,
                2.0 / 3.0,
                2.0 / 3.0,
                1.0,
                1.0
            ])
            .apply(0.3),
            0.3
        ));
    }

    #[test]
    fn tweens_run_in_order_and_setters_write_the_last() {
        let mut a = Actor::new(Kind::Actor);
        let clocks = Clocks::default();
        let mut p = Problems::default();
        // The receptor's `W1Command`.
        run(&mut a, "stoptweening;zoom,.75;linear,0.11;zoom,1");
        assert_eq!(a.current().scale[0], 0.75);
        a.update(0.055, &clocks, &mut p);
        assert!(close(a.current().scale[0], 0.875));
        a.update(0.1, &clocks, &mut p);
        assert_eq!(a.current().scale[0], 1.0);
        assert!(!a.tweening());
        // Two tweens; time left over carries into the next.
        run(
            &mut a,
            "diffusealpha,1;linear,1;diffusealpha,0;decelerate,1;diffusealpha,1",
        );
        a.update(1.5, &clocks, &mut p);
        assert!(close(a.current().diffuse()[3], 0.75));
        // finishtweening jumps to the end.
        run(&mut a, "finishtweening");
        assert_eq!(a.current().diffuse()[3], 1.0);
        // sleep holds, then the queued value lands.
        run(&mut a, "sleep,0.5;diffusealpha,0");
        a.update(0.25, &clocks, &mut p);
        assert_eq!(a.current().diffuse()[3], 1.0);
        a.update(0.26, &clocks, &mut p);
        assert_eq!(a.current().diffuse()[3], 0.0);
        // A zero-length tween waits for an update with time in it.
        run(&mut a, "linear,0;diffusealpha,1");
        a.update(0.0, &clocks, &mut p);
        assert_eq!(a.current().diffuse()[3], 0.0);
        a.update(0.01, &clocks, &mut p);
        assert_eq!(a.current().diffuse()[3], 1.0);
        assert!(p.messages.is_empty());
    }

    #[test]
    fn queued_and_played_commands() {
        let mut a = Actor::new(Kind::Frame(vec![Actor::new(Kind::Actor)]));
        a.set_command("Go", parse("zoom,2;queuecommand,'Later'").unwrap());
        a.set_command("Later", parse("zoom,3").unwrap());
        a.children_mut()[0].set_command("Go", parse("x,5").unwrap());
        let mut p = Problems::default();
        a.play("Go", &mut p);
        assert_eq!(a.current().scale[0], 2.0);
        assert_eq!(a.children()[0].current().pos[0], 5.0);
        a.update(0.01, &Clocks::default(), &mut p);
        assert_eq!(a.current().scale[0], 3.0);
        // A command playing itself is stopped.
        a.set_command("Loop", parse("playcommand,'Loop'").unwrap());
        a.play("Loop", &mut p);
        assert!(!p.messages.is_empty());
        // An error stops the rest of that command.
        let mut b = Actor::new(Kind::Actor);
        b.set_command("C", parse("zoom,2;nosuchmethod;zoom,3").unwrap());
        let mut p = Problems::default();
        b.play("C", &mut p);
        assert_eq!(b.current().scale[0], 2.0);
        assert_eq!(p.messages.len(), 1);
    }

    #[test]
    fn the_receptors_beat_pulse() {
        // common's `[ReceptorArrow] InitCommand`, then dance/default's
        // timing.
        let mut a = Actor::new(Kind::Actor);
        run(
            &mut a,
            "effectclock,'beat';diffuseramp;effectcolor1,color(\".8,.8,.8,1\");effectcolor2,color(\"1,1,1,1\");effecttiming,.2,0,.8,0;effectoffset,.05",
        );
        let mut p = Problems::default();
        let at = |a: &mut Actor, beat: f32| {
            a.update(
                0.016,
                &Clocks {
                    beat,
                    ..Clocks::default()
                },
                &mut Problems::default(),
            );
            a.with_effects().diffuse()[0]
        };
        // 0.1 into the 0.2 ramp up (with the offset): half way to 0.5.
        assert!(close(at(&mut a, 3.05), 0.25 * 0.8 + 0.75));
        // Then down to full over 0.8.
        assert!(close(at(&mut a, 3.55), {
            let pct = 0.5 + 0.4 / 0.8 * 0.5;
            0.8 * pct + (1.0 - pct)
        }));
        a.update(0.0, &Clocks::default(), &mut p);
    }

    #[test]
    fn effects_follow_their_formulas() {
        let mut p = Problems::default();
        // pulse: zoom between 0.5 and 1 on a 2 s period.
        let mut a = Actor::new(Kind::Actor);
        run(&mut a, "pulse");
        a.update(0.5, &Clocks::default(), &mut p);
        // 0.5 s of 2 = a quarter: percent 0.25 → sin(π/4).
        assert!(close(
            a.with_effects().scale[0],
            lerp((0.25 * PI).sin(), 0.5, 1.0)
        ));
        // spin on the beat: 180° a beat.
        let mut s = Actor::new(Kind::Actor);
        run(&mut s, "spin;effectclock,'beat'");
        s.update(
            0.0,
            &Clocks {
                beat: 1.0,
                ..Clocks::default()
            },
            &mut p,
        );
        s.update(
            0.0,
            &Clocks {
                beat: 1.5,
                ..Clocks::default()
            },
            &mut p,
        );
        assert!(close(s.current().rotation[2], 270.0));
        // glowshift on HoldingOn: alpha 0 to 0.7 every 0.1 s.
        let mut g = Actor::new(Kind::Actor);
        run(
            &mut g,
            "diffusealpha,1;glowshift;effectcolor1,color(\"1,1,1,0\");effectcolor2,color(\"1,1,1,.7\");effectperiod,.1",
        );
        // At the start of the cycle the shift is all color 1.
        assert!(close(g.with_effects().glow[3], 0.0));
        g.update(0.025, &Clocks::default(), &mut p);
        // A quarter in (percent 0.25): sin(π) / 2 + 0.5 = half way.
        assert!(close(g.with_effects().glow[3], 0.35));
        // thump: effecttiming(0,0,0.75,0.25) in common's order.
        let mut t = Actor::new(Kind::Actor);
        run(&mut t, "thump");
        assert_eq!(t.timing, [0.0, 0.0, 0.75, 0.0, 0.25]);
        // A transparent actor does not glow.
        let mut d = Actor::new(Kind::Actor);
        run(&mut d, "diffusealpha,0;glowblink");
        assert_eq!(d.with_effects().glow[3], 0.0);
        assert!(
            matches!(run(&mut d, "effecttiming,0,0,0,0").messages.as_slice(), [m] if m.contains("zero"))
        );
    }

    #[test]
    fn sprites_step_through_their_states() {
        let states = (0..4)
            .map(|i| State {
                rect: [i as f32 / 4.0, 0.0, (i + 1) as f32 / 4.0, 1.0],
                delay: 0.1,
            })
            .collect();
        let mut a = Actor::new(Kind::Sprite(Sprite::new(7, states, [64.0, 64.0])));
        let mut p = Problems::default();
        a.update(0.25, &Clocks::default(), &mut p);
        let Kind::Sprite(s) = &a.kind else { panic!() };
        assert_eq!(s.state(), 2);
        // SetSecondsIntoAnimation rewinds then plays forward.
        run(&mut a, "SetSecondsIntoAnimation,0.35");
        let Kind::Sprite(s) = &a.kind else { panic!() };
        assert_eq!(s.state(), 3);
        // animate,false holds; setstate clamps.
        run(&mut a, "animate,false;setstate,9");
        a.update(1.0, &Clocks::default(), &mut p);
        let Kind::Sprite(s) = &a.kind else { panic!() };
        assert_eq!(s.state(), 3);
        // A huge time does not spin forever.
        let Kind::Sprite(s) = &mut a.kind else {
            panic!()
        };
        s.set_seconds_into_animation(1e9);
        assert!(s.state() < 4);
    }

    fn quads(out: &[Drawn]) -> Vec<Quad> {
        out.iter()
            .filter_map(|d| match d {
                Drawn::Quad(q) => Some(*q),
                _ => None,
            })
            .collect()
    }

    fn model() -> Actor {
        use crate::model::{AnimatedTexture, Geometry, Mesh};
        Actor::new(Kind::Model(Box::new(Model {
            // A model without meshes does not animate (`AdvanceFrame`).
            geometry: Arc::new(Geometry {
                meshes: vec![Mesh {
                    name: "empty".into(),
                    material: Some(0),
                    vertices: Vec::new(),
                    triangles: Vec::new(),
                    bone: None,
                }],
            }),
            animation: Arc::new(crate::model::Animation {
                bones: Vec::new(),
                total_frames: 60,
            }),
            materials: vec![ModelMaterial {
                color: [0.5, 1.0, 1.0, 1.0],
                frames: vec![3, 4],
                // `TexVelocityX=1`, `a.png` and `b.png` a second each.
                texture: AnimatedTexture {
                    frames: vec![("a.png".into(), 1.0), ("b.png".into(), 1.0)],
                    velocity: [1.0, 0.0],
                    offset: [0.0; 2],
                    translate: vec![[0.0; 2]; 2],
                },
            }],
            rest: Arc::new(Vec::new()),
            bones: Vec::new(),
            frame: 0.0,
            rate: 1.0,
            looping: true,
            default_animation: None,
            seconds: 0.0,
            state: None,
        })))
    }

    #[test]
    fn models_are_placed_in_3d_and_animate() {
        let mut m = model();
        let mut f = Actor::new(Kind::Frame(vec![]));
        run(&mut m, "x,10;rotationx,90;diffuse,0.5,1,1,1");
        if let Kind::Frame(c) = &mut f.kind {
            c.push(m);
        }
        run(&mut f, "zoom,2");
        let mut out = Vec::new();
        f.draw(
            [1.0, 0.0, 0.0, 1.0, 100.0, 0.0],
            [1.0; 4],
            [1.0, 1.0, 1.0, 0.0],
            &mut out,
        );
        let [Drawn::Model(d)] = out.as_slice() else {
            panic!("{out:?}")
        };
        // MilkShape's y is up: (0, 1, 0) goes up the screen, which the
        // x rotation of 90° turns into depth, which the frame's zoom
        // (`SetZoom`: x, y and z) scales too; x is the frame's zoomed place.
        let p = transform_point(&d.transform, [0.0, 1.0, 0.0]);
        assert!(close(p[0], 120.0), "{p:?}");
        assert!(close(p[1], 0.0), "{p:?}");
        assert!(close(p[2], 2.0), "{p:?}");
        // The material's color times the diffuse, its frame and scroll by
        // time.
        assert_eq!(d.materials[0].0, Some(3));
        assert_eq!(d.materials[0].1, [0.25, 1.0, 1.0, 1.0]);
        let Kind::Frame(c) = &mut f.kind else {
            panic!()
        };
        run(&mut c[0], "SetSecondsIntoAnimation,1.5");
        out.clear();
        f.draw(TRANSFORM_IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        let Drawn::Model(d) = &out[0] else { panic!() };
        assert_eq!(d.materials[0].0, Some(4));
        assert!(close(d.materials[0].2[0], 0.75));
        // `setstate` holds a frame; the bones' frame advances 30 a second
        // and loops.
        let Kind::Frame(c) = &mut f.kind else {
            panic!()
        };
        run(&mut c[0], "setstate,0;rate,2");
        c[0].update_end(1.5);
        let Kind::Model(model) = &c[0].kind else {
            panic!()
        };
        assert_eq!(model.state, Some(0));
        assert!(close(model.frame, 30.0), "{}", model.frame);
        // With a default animation, running out plays it again from its
        // start, at its rate; one that is not there holds the frame.
        run(&mut c[0], "SetDefaultAnimation,'default',0.5");
        c[0].update_end(1.0);
        let Kind::Model(model) = &c[0].kind else {
            panic!()
        };
        assert_eq!((model.frame, model.rate), (0.0, 0.5));
        run(&mut c[0], "SetDefaultAnimation,'walk';rate,1");
        c[0].update_end(3.0);
        let Kind::Model(model) = &c[0].kind else {
            panic!()
        };
        assert!(close(model.frame, 90.0), "{}", model.frame);
    }

    #[test]
    fn frames_hand_down_diffuse_and_place_children() {
        let child = Actor::new(Kind::Sprite(Sprite::new(1, vec![], [64.0, 32.0])));
        let mut f = Actor::new(Kind::Frame(vec![child]));
        run(&mut f, "diffuse,1,0.5,1,0.5;x,100;rotationz,90");
        run(&mut f.children_mut()[0], "y,10;diffusealpha,0.5");
        let mut out = Vec::new();
        f.draw(TRANSFORM_IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        assert_eq!(out.len(), 1);
        let q = quads(&out)[0];
        assert_eq!(q.diffuse, [[1.0, 0.5, 1.0, 0.25]; 4]);
        // The child's y,10 turned by the frame's 90°: x moves by -10.
        assert!(close(q.transform[4], 90.0) && close(q.transform[5], 0.0));
        // Width along y after the turn.
        assert!(close(q.transform[1], 64.0) && close(q.transform[2], -32.0));
        // An invisible or transparent actor draws nothing.
        run(&mut f, "diffusealpha,0");
        out.clear();
        f.draw(TRANSFORM_IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        assert!(out.is_empty());
        // Vertical alignment moves by half the size.
        let mut s = Actor::new(Kind::Sprite(Sprite::new(1, vec![], [64.0, 32.0])));
        run(&mut s, "vertalign,top");
        out.clear();
        s.draw(TRANSFORM_IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        assert!(close(quads(&out)[0].transform[5], 16.0));
    }

    #[test]
    fn runaway_commands_stop() {
        // Twice per level, 32 levels deep: billions of calls without a
        // budget.
        let mut a = Actor::new(Kind::Actor);
        a.set_command(
            "Init",
            parse("playcommand,'Init';playcommand,'Init'").unwrap(),
        );
        let mut p = Problems::default();
        let t = std::time::Instant::now();
        a.play("Init", &mut p);
        assert!(t.elapsed().as_secs() < 2);
        assert!(p.messages.iter().any(|m| m.contains("without end")));
    }

    #[test]
    fn x_and_y_rotations_follow_stepmanias_matrix() {
        // baserotationy 180 with baserotationz 90: x goes to (0, 1)… times
        // cos 180 = -1 for the x part only.
        let mut a = Actor::new(Kind::Sprite(Sprite::new(1, vec![], [1.0, 1.0])));
        run(&mut a, "baserotationy,180;baserotationz,90");
        let mut out = Vec::new();
        a.draw(TRANSFORM_IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        let m = quads(&out)[0].transform;
        // x: (cZ·cY, sZ·cX) = (0, 1); y: (−sZ·cY, cZ·cX) = (1, 0).
        assert!(close(m[0], 0.0) && close(m[1], 1.0));
        assert!(close(m[2], 1.0) && close(m[3], 0.0));
    }

    /// A 100 by 100 sprite's quads after `commands`: each one's left,
    /// top, right, bottom on the target, its image's, and its corners'
    /// alphas.
    fn cut(commands: &str) -> Vec<([f32; 4], [f32; 4], [f32; 4])> {
        let mut a = Actor::new(Kind::Sprite(Sprite::new(
            7,
            vec![State {
                rect: [0.0, 0.0, 1.0, 0.5],
                delay: 0.0,
            }],
            [100.0, 100.0],
        )));
        assert!(run(&mut a, commands).messages.is_empty());
        draw_all(&mut a)
            .iter()
            .filter_map(|d| match d {
                Drawn::Quad(q) => {
                    let m = q.transform;
                    let (x0, x1) = (m[4] - m[0] / 2.0, m[4] + m[0] / 2.0);
                    let (y0, y1) = (m[5] - m[3] / 2.0, m[5] + m[3] / 2.0);
                    Some((
                        [x0, y0, x1, y1].map(|v| (v * 100.0).round() / 100.0),
                        q.uv.map(|v| (v * 1000.0).round() / 1000.0),
                        q.diffuse.map(|c| (c[3] * 100.0).round() / 100.0),
                    ))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn crops_cut_the_quad_and_its_image() {
        // The sprite spans -50..50 each way.
        assert_eq!(
            cut("cropleft,0.25"),
            [([-25.0, -50.0, 50.0, 50.0], [0.25, 0.0, 1.0, 0.5], [1.0; 4])]
        );
        assert_eq!(
            cut("cropright,0.25;croptop,0.5"),
            [([-50.0, 0.0, 25.0, 50.0], [0.0, 0.25, 0.75, 0.5], [1.0; 4])]
        );
        assert_eq!(
            cut("cropbottom,0.1"),
            [([-50.0, -50.0, 50.0, 40.0], [0.0, 0.0, 1.0, 0.45], [1.0; 4])]
        );
        // Cropped all the way: nothing (`DrawTexture` bails).
        assert!(cut("cropleft,0.5;cropright,0.5").is_empty());
        // A negative crop draws no more than the sprite.
        assert_eq!(cut("cropleft,-1")[0].0, [-50.0, -50.0, 50.0, 50.0]);
    }

    #[test]
    fn fades_are_pieces_of_their_own_transparent_outside() {
        // The inside, then the left edge: transparent on its left.
        assert_eq!(
            cut("fadeleft,0.2"),
            [
                ([-30.0, -50.0, 50.0, 50.0], [0.2, 0.0, 1.0, 0.5], [1.0; 4]),
                (
                    [-50.0, -50.0, -30.0, 50.0],
                    [0.0, 0.0, 0.2, 0.5],
                    [0.0, 1.0, 0.0, 1.0]
                ),
            ]
        );
        assert_eq!(cut("faderight,0.2")[1].2, [1.0, 0.0, 1.0, 0.0]);
        assert_eq!(cut("fadetop,0.2")[1].2, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(cut("fadebottom,0.2")[1].2, [1.0, 1.0, 0.0, 0.0]);
        // Fading both ways leaves the corners out.
        let both = cut("fadeleft,0.2;fadetop,0.2");
        assert_eq!(both[0].0, [-30.0, -30.0, 50.0, 50.0]);
        assert_eq!(both[1].0, [-50.0, -30.0, -30.0, 50.0]);
        assert_eq!(both[2].0, [-30.0, -50.0, 50.0, -30.0]);
        // A crop leaving less than the fade: the fade's inner side is as
        // opaque as the fade got.
        // (Nothing is left inside.)
        let cropped = cut("fadeleft,0.5;cropright,0.75");
        assert_eq!(cropped.len(), 1);
        assert_eq!(cropped[0].0, [-50.0, -50.0, -25.0, 50.0]);
        assert_eq!(cropped[0].2, [0.0, 0.5, 0.0, 0.5]);
        // The diffuse's own alpha carries through.
        assert_eq!(
            cut("diffusealpha,0.5;fadeleft,0.2")[1].2,
            [0.0, 0.5, 0.0, 0.5]
        );
    }

    #[test]
    fn crops_and_fades_tween() {
        let mut a = Actor::new(Kind::Sprite(Sprite::quad([100.0, 100.0])));
        run(&mut a, "linear,1;cropleft,0.5;fadetop,0.4");
        let mut p = Problems::default();
        a.update(0.5, &Clocks::default(), &mut p);
        assert!(close(a.current().crop[0], 0.25));
        assert!(close(a.current().fade[1], 0.2));
        a.update(0.5, &Clocks::default(), &mut p);
        assert_eq!(a.current().crop, [0.5, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn negative_tweens_are_skipped_and_frames_pass_finishing_on() {
        let mut a = Actor::new(Kind::Actor);
        let p = run(&mut a, "linear,-1;zoom,2");
        assert_eq!(a.current().scale[0], 2.0);
        assert_eq!(p.messages.len(), 1);
        let mut f = Actor::new(Kind::Frame(vec![Actor::new(Kind::Actor)]));
        run(&mut f.children_mut()[0], "linear,1;zoom,3");
        run(&mut f, "finishtweening");
        assert_eq!(f.children()[0].current().scale[0], 3.0);
        assert!(!run(&mut a, "visible").messages.is_empty());
    }

    fn draw_all(a: &mut Actor) -> Vec<Drawn> {
        let mut out = Vec::new();
        a.draw(TRANSFORM_IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        out
    }

    fn frame_of(children: Vec<Actor>) -> Actor {
        Actor::new(Kind::Frame(children))
    }

    #[test]
    fn quads_and_corner_colors() {
        let mut q = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        assert!(q.textures().is_empty());
        run(
            &mut q,
            "zoomto,20,10;diffuse,1,0,0,1;diffusebottomedge,0,0,1,0.5;diffuseupperright,0,1,0,1",
        );
        let out = draw_all(&mut q);
        let [Drawn::Quad(d)] = out.as_slice() else {
            panic!("{out:?}")
        };
        assert_eq!(d.texture, UNTEXTURED);
        assert!(close(d.transform[0], 20.0) && close(d.transform[3], 10.0));
        assert_eq!(
            d.diffuse,
            [
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 0.5],
                [0.0, 0.0, 1.0, 0.5]
            ]
        );
        // `GetDiffuse` is the upper left corner; `diffusealpha` sets every
        // corner's alpha, `diffusecolor` every corner's color.
        assert_eq!(
            q.getter("GetDiffuse"),
            Some(Value::Color([1.0, 0.0, 0.0, 1.0]))
        );
        run(&mut q, "diffusealpha,0.25;diffusecolor,1,1,1,1");
        assert_eq!(q.current().diffuse, [[1.0, 1.0, 1.0, 0.25]; 4]);
        // Tweens move each corner.
        run(
            &mut q,
            "diffuselowerleft,1,1,1,1;linear,1;diffuselowerleft,0,0,0,1",
        );
        q.update(0.5, &Clocks::default(), &mut Problems::default());
        assert_eq!(q.current().diffuse[2], [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(q.current().diffuse[0], [1.0, 1.0, 1.0, 0.25]);
        // A color effect paints every corner, at the upper left's alpha.
        run(&mut q, "finishtweening;diffuseblink");
        assert_eq!(q.with_effects().diffuse, [[1.0, 1.0, 1.0, 0.25]; 4]);
        // Only the upper left corner is handed down (`ActorFrame::DrawPrimitives`);
        // any corner seen draws.
        let mut f = frame_of(vec![Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])))]);
        run(&mut f, "diffusealpha,0;diffuseupperleft,0.5,0.5,0.5,0.5");
        let out = draw_all(&mut f);
        assert_eq!(quads(&out)[0].diffuse, [[0.5, 0.5, 0.5, 0.5]; 4]);
        let mut corner = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        run(&mut corner, "diffusealpha,0;diffuselowerright,1,1,1,1");
        assert_eq!(draw_all(&mut corner).len(), 1);
    }

    #[test]
    fn text_is_measured_aligned_and_squeezed() {
        use crate::text::FixedMeasure;
        let style = TextStyle::new("sans-serif", 20.0);
        // Ten characters of 10 pixels; a line of 25, the baseline at 20.
        let mut t = Actor::new(Kind::Text(Text::new("0123456789", style.clone())));
        assert_eq!(t.size(), [0.0, 25.0]);
        t.measure(&mut FixedMeasure);
        assert_eq!(t.size(), [100.0, 25.0]);
        assert_eq!(t.getter("GetWidth"), Some(Value::Num(100.0)));
        run(
            &mut t,
            "x,300;y,50;horizalign,'HorizAlign_Left';vertalign,'VertAlign_Top'",
        );
        let out = draw_all(&mut t);
        let [Drawn::Text(d)] = out.as_slice() else {
            panic!("{out:?}")
        };
        // The pen: the box's left at x, the baseline 20 below its top.
        assert_eq!(&*d.text, "0123456789");
        assert!(
            close(d.transform[4], 300.0) && close(d.transform[5], 70.0),
            "{:?}",
            d.transform
        );
        assert_eq!(d.metrics.advance, 100.0);
        let top_left = d.line_box_transform();
        assert!(close(top_left[4], 300.0) && close(top_left[5], 50.0));
        // Centered, zoomed: the pen half the zoomed width left of x.
        run(&mut t, "horizalign,'HorizAlign_Center';zoom,2");
        let out = draw_all(&mut t);
        let Drawn::Text(d) = &out[0] else { panic!() };
        assert!(close(d.transform[0], 2.0) && close(d.transform[4], 200.0));
        // `maxwidth` squeezes it (before zoom), alignment included.
        run(&mut t, "maxwidth,50");
        let out = draw_all(&mut t);
        let Drawn::Text(d) = &out[0] else { panic!() };
        assert!(close(d.transform[0], 1.0) && close(d.transform[3], 2.0));
        assert!(close(d.transform[4], 250.0), "{:?}", d.transform);
        // `settext` measures again; until then the old size stands.
        run(&mut t, "settext,'01234'");
        assert_eq!(t.size(), [0.0, 25.0]);
        t.measure(&mut FixedMeasure);
        assert_eq!(t.size(), [50.0, 25.0]);
        // A shadow: the run again, under it, in the shadow's color.
        run(&mut t, "shadowlength,3;diffusealpha,0.5");
        let out = draw_all(&mut t);
        let [Drawn::Text(shadow), Drawn::Text(run_)] = out.as_slice() else {
            panic!("{out:?}")
        };
        assert_eq!(shadow.diffuse, [[0.0, 0.0, 0.0, 0.25]; 4]);
        assert!(close(shadow.transform[4] - run_.transform[4], 3.0));
        assert!(close(shadow.transform[5] - run_.transform[5], 3.0));
        assert_eq!(shadow.style, style);
    }

    #[test]
    fn frames_clip_their_children() {
        let child = || Actor::new(Kind::Sprite(Sprite::quad([10.0, 10.0])));
        let mut inner = frame_of(vec![child()]);
        inner.clip = Some([0.0, 0.0, 100.0, 100.0]);
        run(&mut inner, "x,50");
        let mut outer = frame_of(vec![inner, child()]);
        outer.clip = Some([-20.0, -20.0, 120.0, 80.0]);
        run(&mut outer, "zoom,2;x,10");
        let out = draw_all(&mut outer);
        let [
            Drawn::ClipPush(a),
            Drawn::ClipPush(b),
            Drawn::Quad(_),
            Drawn::ClipPop,
            Drawn::Quad(_),
            Drawn::ClipPop,
        ] = out.as_slice()
        else {
            panic!("{out:?}")
        };
        // The outer: its rectangle zoomed and moved.
        assert_eq!(a.scissor, [-30.0, -40.0, 250.0, 160.0]);
        assert!(a.exact);
        assert_eq!(a.corners[3], [250.0, 160.0]);
        // The inner: its own, cut to the outer's.
        assert_eq!(b.scissor, [110.0, 0.0, 250.0, 160.0]);
        // Turned, it is a bound and the corners cut too.
        let mut turned = frame_of(vec![child()]);
        turned.clip = Some([0.0, 0.0, 10.0, 10.0]);
        run(&mut turned, "rotationz,45");
        let Drawn::ClipPush(c) = &draw_all(&mut turned)[0] else {
            panic!()
        };
        assert!(!c.exact);
        assert!(close(c.scissor[2] - c.scissor[0], 200f32.sqrt()));
        // A quarter turn stays a scissor.
        run(&mut turned, "rotationz,90");
        let Drawn::ClipPush(c) = &draw_all(&mut turned)[0] else {
            panic!()
        };
        assert!(c.exact);
        // Nothing left: nothing drawn, no clip.
        let mut gone = frame_of(vec![child()]);
        gone.clip = Some([0.0, 0.0, 0.0, 10.0]);
        assert!(draw_all(&mut gone).is_empty());
    }

    #[test]
    fn draw_order_sorts_frames_stably() {
        let named = |name: &str, order: &str| {
            let mut a = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
            a.name = name.into();
            if !order.is_empty() {
                run(&mut a, &format!("draworder,{order}"));
            }
            a
        };
        let mut f = frame_of(vec![
            named("a", "10"),
            named("b", ""),
            named("c", "-5.7"),
            named("d", "10"),
            named("e", ""),
        ]);
        assert!(f.sort_by_draw_order());
        let names: Vec<&str> = f.children().iter().map(|c| c.name.as_str()).collect();
        // -5.7 is -5 (`IArg`); equal orders keep their places.
        assert_eq!(names, ["c", "b", "e", "a", "d"]);
        assert!(!f.sort_by_draw_order());
        // `SetDrawByZPosition`: far first when drawn, the children as
        // they are.
        let mut z = frame_of(vec![named("near", ""), named("far", ""), named("mid", "")]);
        run(&mut z.children_mut()[0], "z,5;diffuse,1,0,0,1");
        run(&mut z.children_mut()[1], "z,-5;diffuse,0,1,0,1");
        run(&mut z, "SetDrawByZPosition,true");
        let reds: Vec<f32> = quads(&draw_all(&mut z))
            .iter()
            .map(|q| q.diffuse[0][0])
            .collect();
        assert_eq!(reds, [0.0, 1.0, 1.0]);
        let greens: Vec<f32> = quads(&draw_all(&mut z))
            .iter()
            .map(|q| q.diffuse[0][1])
            .collect();
        assert_eq!(greens, [1.0, 1.0, 0.0]);
        assert_eq!(z.children()[0].name, "near");
    }

    #[test]
    fn sprites_draw_their_shadows_first() {
        let mut s = Actor::new(Kind::Sprite(Sprite::new(4, vec![], [10.0, 10.0])));
        run(
            &mut s,
            "shadowlength,2;diffuse,1,0,0,0.5;glow,1,1,1,1;rotationz,90",
        );
        let view = View {
            transform: [3.0, 0.0, 0.0, 3.0, 0.0, 0.0],
            size: [854.0, 480.0],
        };
        let mut out = Vec::new();
        s.draw_in(
            &view,
            [3.0, 0.0, 0.0, 3.0, 30.0, 30.0],
            [1.0; 4],
            [1.0, 1.0, 1.0, 0.0],
            &mut out,
        );
        let [Drawn::Quad(shadow), Drawn::Quad(q)] = out.as_slice() else {
            panic!("{out:?}")
        };
        // Black at half the actor's alpha, no glow, the same texture.
        assert_eq!(shadow.diffuse, [[0.0, 0.0, 0.0, 0.25]; 4]);
        assert_eq!(shadow.glow[3], 0.0);
        assert_eq!(shadow.texture, 4);
        // Moved in the screen's pixels (3 target pixels each), not turned
        // with the sprite.
        assert!(close(shadow.transform[4] - q.transform[4], 6.0));
        assert!(close(shadow.transform[5] - q.transform[5], 6.0));
        // `shadowcolor`; a frame draws none of its own.
        run(&mut s, "shadowcolor,1,1,1,1;shadowlengthx,0");
        let out = draw_all(&mut s);
        assert_eq!(quads(&out)[0].diffuse[0], [1.0, 1.0, 1.0, 0.5]);
        assert!(close(
            quads(&out)[0].transform[4],
            quads(&out)[1].transform[4]
        ));
        let mut f = frame_of(vec![]);
        run(&mut f, "shadowlength,2");
        assert!(draw_all(&mut f).is_empty());
    }

    #[test]
    fn fov_frames_see_their_children_in_perspective() {
        let child = || Actor::new(Kind::Sprite(Sprite::quad([10.0, 10.0])));
        let mut f = frame_of(vec![child(), child()]);
        run(&mut f, "fov,90");
        run(&mut f.children_mut()[0], "x,427;y,240");
        run(&mut f.children_mut()[1], "x,527;y,240;z,100");
        let view = View::default();
        let mut out = Vec::new();
        f.draw_in(
            &view,
            TRANSFORM_IDENTITY,
            [1.0; 4],
            [1.0, 1.0, 1.0, 0.0],
            &mut out,
        );
        let q = quads(&out);
        // At z = 0 the screen is seen as it is; the eye is 427 pixels away
        // (90° across 854).
        let p = q[0].projected.unwrap();
        assert!(close(p.corners[0][0], 422.0) && close(p.corners[3][1], 245.0));
        assert!(close(p.w[0], 1.0));
        // Nearer the eye, bigger and away from the vanishing point.
        let p = q[1].projected.unwrap();
        let s = 427.0 / 327.0;
        assert!(close(p.corners[0][0], 427.0 + 95.0 * s), "{p:?}");
        assert!(close(p.corners[3][0] - p.corners[0][0], 10.0 * s));
        assert!(close(p.w[0], 327.0 / 427.0));
        // The orthographic view stays in `transform`.
        assert!(close(q[1].transform[4], 527.0));
        // A frame of fov 0 inside is orthographic again; one of -1 keeps
        // the camera.
        let mut inner = frame_of(vec![child()]);
        run(&mut inner, "fov,0");
        let mut keeps = frame_of(vec![child()]);
        run(&mut keeps, "fov,-1");
        let mut g = frame_of(vec![inner, keeps]);
        run(&mut g, "fov,60;vanishpoint,0,0");
        let mut out = Vec::new();
        g.draw_in(
            &view,
            TRANSFORM_IDENTITY,
            [1.0; 4],
            [1.0, 1.0, 1.0, 0.0],
            &mut out,
        );
        let q = quads(&out);
        assert_eq!(q[0].projected, None);
        assert!(q[1].projected.is_some());
        // Behind the eye: not drawn.
        let mut behind = frame_of(vec![child()]);
        run(&mut behind, "fov,90");
        run(&mut behind.children_mut()[0], "z,500");
        assert!(draw_all(&mut behind).is_empty());
        // Models carry the camera.
        let mut m = frame_of(vec![model()]);
        run(&mut m, "fov,45");
        let Drawn::Model(d) = &draw_all(&mut m)[0] else {
            panic!()
        };
        let FrameCamera::Perspective(c) = d.camera else {
            panic!("{:?}", d.camera)
        };
        assert!(close(c.distance, 427.0 / (22.5f32).to_radians().tan()));
        assert_eq!(c.vanish, [427.0, 240.0]);
        // The frustum's planes, as its quads have them (`GetFrustumMatrix`:
        // 1 and the distance + 1000).
        assert!(close(c.near, 1.0) && close(c.far, c.distance + 1000.0));
        // Under no frame: the drawer's; under `fov(0)`: orthographic, as
        // quads there are, depth to ±1000 pixels (on a target at twice the
        // scale, 2000).
        let Drawn::Model(d) = &draw_all(&mut model())[0] else {
            panic!()
        };
        assert_eq!(d.camera, FrameCamera::Drawer);
        let mut flat = frame_of(vec![model()]);
        run(&mut flat, "fov,0");
        let mut out = Vec::new();
        let doubled = View {
            transform: [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            ..View::default()
        };
        flat.draw_in(
            &doubled,
            [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            [1.0; 4],
            [1.0, 1.0, 1.0, 0.0],
            &mut out,
        );
        let Drawn::Model(d) = &out[0] else { panic!() };
        assert_eq!(d.camera, FrameCamera::Orthographic { depth: 2000.0 });
    }

    #[test]
    fn masks_are_depth_states() {
        let mut source = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        run(&mut source, "MaskSource,true");
        let Drawn::Quad(q) = &draw_all(&mut source)[0] else {
            panic!()
        };
        assert_eq!(q.blend, Blend::NoEffect);
        assert_eq!(
            q.depth,
            Depth {
                write: true,
                test: ZTest::Off,
                clear: true
            }
        );
        // Without `true` the depth is kept (`noclear`, named backwards).
        let mut kept = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        run(&mut kept, "MaskSource;MaskDest");
        assert_eq!(
            kept.depth,
            Depth {
                write: true,
                test: ZTest::WriteOnPass,
                clear: false
            }
        );
        let mut dest = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        run(
            &mut dest,
            "ztestmode,'ZTestMode_WriteOnFail';blend,'BlendMode_NoEffect'",
        );
        assert_eq!(dest.depth.test, ZTest::WriteOnFail);
        assert_eq!(dest.blend, Blend::NoEffect);
        run(&mut dest, "zbuffer,false");
        assert_eq!(dest.depth, Depth::default());
        // StepMania's other modes by their enum names; an unknown one draws
        // as normal.
        for (name, blend) in [
            ("WeightedMultiply", Blend::WeightedMultiply),
            ("InvertDest", Blend::InvertDest),
            ("CopySrc", Blend::CopySrc),
            ("AlphaMultiply", Blend::AlphaMultiply),
            ("AlphaMask", Blend::AlphaMask),
            ("AlphaKnockOut", Blend::AlphaKnockOut),
            ("Sparkle", Blend::Other),
        ] {
            run(&mut dest, &format!("blend,'BlendMode_{name}'"));
            assert_eq!(dest.blend, blend);
        }
    }

    #[test]
    fn local_transforms_place_as_drawn() {
        let mut a = Actor::new(Kind::Sprite(Sprite::quad([40.0, 20.0])));
        run(
            &mut a,
            "xy,100,50;zoomx,2;rotationz,30;halign,0;basezoomy,0.5",
        );
        let mut out = Vec::new();
        a.draw(IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        let Some(Drawn::Quad(q)) = out.first() else {
            panic!("{out:?}");
        };
        // The drawn quad is the unit square sized to the box, placed.
        assert_eq!(
            q.transform,
            mul(&a.local_transform(), &[40.0, 0.0, 0.0, 20.0, 0.0, 0.0])
        );
        // Effects do not move it.
        run(&mut a, "bob");
        let before = a.local_transform();
        a.update(0.3, &Clocks::default(), &mut Problems::default());
        assert_eq!(a.local_transform(), before);
    }

    #[test]
    fn in_motion_while_anything_may_change() {
        let mut a = Actor::new(Kind::Frame(vec![Actor::new(Kind::Sprite(Sprite::quad(
            [1.0; 2],
        )))]));
        assert!(!a.in_motion());
        run(&mut a.children_mut()[0], "linear,0.5;x,10");
        assert!(a.in_motion());
        a.update(0.6, &Clocks::default(), &mut Problems::default());
        assert!(!a.in_motion());
        run(&mut a.children_mut()[0], "diffuseshift");
        assert!(a.in_motion());
        run(&mut a.children_mut()[0], "stopeffect;hibernate,1");
        assert!(a.in_motion() && a.children()[0].hibernating());
        a.update(1.5, &Clocks::default(), &mut Problems::default());
        assert!(!a.in_motion());
        // Hidden: only tweens count; scrolling counts while playing.
        run(&mut a.children_mut()[0], "texcoordvelocity,1,0;pause");
        assert!(!a.in_motion());
        run(&mut a.children_mut()[0], "play");
        assert!(a.in_motion());
        run(&mut a.children_mut()[0], "visible,0");
        assert!(!a.in_motion());
        run(&mut a.children_mut()[0], "sleep,1;visible,1");
        assert!(a.in_motion());
    }
}
