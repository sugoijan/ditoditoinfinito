use std::f32::consts::PI;
use std::sync::Arc;

use crate::commands::{Arg, Call};
use crate::model::{AnimatedTexture, Animation, Geometry, Rest};

pub type Rgba = [f32; 4];

/// What tweens interpolate (`Actor::TweenState`, the parts skins use).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TweenState {
    pub pos: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub diffuse: Rgba,
    pub glow: Rgba,
}

impl Default for TweenState {
    /// `TweenState::Init`.
    fn default() -> TweenState {
        TweenState {
            pos: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            diffuse: [1.0; 4],
            glow: [1.0, 1.0, 1.0, 0.0],
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
            diffuse: lerp_n(t, a.diffuse, b.diffuse),
            glow: lerp_n(t, a.glow, b.glow),
        }
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
    /// Masks and the rest: drawn as normal, reported.
    Other,
}

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
        }
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

    /// What the materials show now: texture, colour (times `diffuse`, as
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
    /// Per material: its texture now (`None`: none), colour, translation.
    pub materials: Vec<(Option<u32>, Rgba, [f32; 2])>,
    /// Each bone's matrix (MilkShape's space), for the meshes and vertices
    /// that follow one.
    pub bones: Vec<Mat4>,
    pub diffuse: Rgba,
    pub glow: Rgba,
    pub blend: Blend,
}

/// What an actor draws: quads and models, in drawing order.
#[derive(Clone, Debug, PartialEq)]
pub enum Drawn {
    Quad(Quad),
    Model(ModelDraw),
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
    pub halign: f32,
    pub valign: f32,
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
    commands: Vec<(String, Vec<Call>)>,
    rng: u64,
}

/// The methods [`Actor::call`] runs (the setters a skin's Lua may call);
/// `playcommand` is the Lua driver's in a Lua skin.
pub const METHODS: &[&str] = &[
    "MaskDest",
    "MaskSource",
    "SetAllStateDelays",
    "SetDrawByZPosition",
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
    "cullmode",
    "customtexturerect",
    "decelerate",
    "diffuse",
    "diffusealpha",
    "diffuseblink",
    "diffusecolor",
    "diffuseramp",
    "diffuseshift",
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
    "finishtweening",
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
    "pause",
    "play",
    "pulse",
    "queuecommand",
    "rainbow",
    "rotationx",
    "rotationy",
    "rotationz",
    "set_use_effect_clock_for_texcoords",
    "setsize",
    "setstate",
    "shadowlength",
    "shadowlengthx",
    "shadowlengthy",
    "sleep",
    "smooth",
    "spin",
    "spring",
    "stopeffect",
    "stoptweening",
    "texcoordvelocity",
    "thump",
    "valign",
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
    "zoomx",
    "zoomy",
    "zoomz",
    "ztest",
    "ztestmode",
    "zwrite",
];

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
    /// `[left, top, right, bottom]` of the image, before `texcoord_offset`.
    pub uv: [f32; 4],
    pub texcoord_offset: [f32; 2],
    pub diffuse: Rgba,
    pub glow: Rgba,
    pub blend: Blend,
}

type Mat = [f32; 6];

const IDENTITY: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `a` then `b`: `b ∘ a` in the `x' = m·x` sense, i.e. `parent * local`.
fn mul(parent: &Mat, local: &Mat) -> Mat {
    [
        parent[0] * local[0] + parent[2] * local[1],
        parent[1] * local[0] + parent[3] * local[1],
        parent[0] * local[2] + parent[2] * local[3],
        parent[1] * local[2] + parent[3] * local[3],
        parent[0] * local[4] + parent[2] * local[5] + parent[4],
        parent[1] * local[4] + parent[3] * local[5] + parent[5],
    ]
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
            halign: 0.5,
            valign: 0.5,
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
            Some((_, c)) => *c = calls,
            None => self.commands.push((name.to_string(), calls)),
        }
    }

    /// Adds `calls` after command `name`'s, as `t .. { NameCommand = … }`
    /// chains two functions (`MergeTables`, `02 ActorDef.lua:12-37`).
    pub fn add_command(&mut self, name: &str, calls: Vec<Call>) {
        match self.commands.iter_mut().find(|(n, _)| n == name) {
            Some((_, c)) => c.extend(calls),
            None => self.commands.push((name.to_string(), calls)),
        }
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.commands.iter().any(|(n, _)| n == name)
    }

    /// `SetDiffuse`/`SetGlow` as `NoteDisplay::DrawActor` calls them for a
    /// note: the colour of the appearance mods and a hold's life.
    pub fn set_note_colors(&mut self, diffuse: Rgba, glow: Rgba) {
        let d = self.dest();
        d.diffuse = diffuse;
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
            Kind::Sprite(s) => vec![s.texture],
            Kind::Frame(c) => c.iter().flat_map(Actor::textures).collect(),
            Kind::Model(m) => m.materials.iter().flat_map(|m| m.frames.clone()).collect(),
            Kind::Actor => Vec::new(),
        }
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
        let alpha = s.diffuse[3];
        let (c1, c2) = (self.effect_color1, self.effect_color2);
        let mix = |t: f32| -> Rgba { std::array::from_fn(|i| c1[i] * t + c2[i] * (1.0 - t)) };
        match self.effect {
            Effect::None | Effect::Spin => {}
            Effect::DiffuseBlink => {
                s.diffuse = if blink_on { c1 } else { c2 };
                s.diffuse[3] *= alpha;
            }
            Effect::DiffuseShift => {
                s.diffuse = mix(between);
                s.diffuse[3] *= alpha;
            }
            Effect::DiffuseRamp => {
                s.diffuse = mix(p);
                s.diffuse[3] *= alpha;
            }
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
                s.diffuse = [
                    a.cos() * 0.5 + 0.5,
                    (a + PI * 2.0 / 3.0).cos() * 0.5 + 0.5,
                    (a + PI * 4.0 / 3.0).cos() * 0.5 + 0.5,
                    alpha,
                ];
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
            .map(|(_, c)| c.clone())
        else {
            return;
        };
        for call in &calls {
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
            "finishtweening" | "hurrytweening" => tree(self),
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
        // `RageColor::FromStackCompat`: a colour table, or four numbers.
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
            // Colour.
            "diffuse" => self.dest().diffuse = color(0)?,
            "diffusealpha" => self.dest().diffuse[3] = num(0)?,
            "diffusecolor" => {
                let c = color(0)?;
                let d = self.dest();
                d.diffuse[..3].copy_from_slice(&c[..3]);
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
            // Drawn the same or not at all here.
            "SetTextureFiltering"
            | "draworder"
            | "fov"
            | "SetDrawByZPosition"
            | "zbuffer"
            | "ztest"
            | "zwrite"
            | "clearzbuffer"
            | "shadowlength"
            | "shadowlengthx"
            | "shadowlengthy"
            | "MaskSource"
            | "MaskDest"
            | "cullmode"
            | "ztestmode" => {}
            other => return Err(format!("{other} is not supported")),
        }
        Ok(())
    }

    /// What getter `method` answers (`Actor`'s and `Sprite`'s Lua
    /// methods that read state, `src/Actor.cpp`, `src/Sprite.cpp`); `None`
    /// for anything else. Positions, zooms and colours are the destination
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
            "GetRotationZ" => Value::Num(d.rotation[2]),
            "GetBaseZoomX" => Value::Num(self.base_scale[0]),
            "GetBaseZoomY" => Value::Num(self.base_scale[1]),
            "GetBaseZoomZ" => Value::Num(self.base_scale[2]),
            "GetBaseRotationX" => Value::Num(self.base_rotation[0]),
            "GetBaseRotationY" => Value::Num(self.base_rotation[1]),
            "GetBaseRotationZ" => Value::Num(self.base_rotation[2]),
            "GetDiffuse" => Value::Color(d.diffuse),
            "GetDiffuseAlpha" => Value::Num(d.diffuse[3]),
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
            _ => [0.0; 2],
        }
    }

    // Drawing.

    /// Appends what the actor draws at `transform` (its parent's), under the
    /// parent's diffuse and glow (`ActorFrame::DrawPrimitives` hands them
    /// down; `Actor::PreDraw` multiplies and screens them in).
    pub fn draw(&mut self, transform: [f32; 6], diffuse: Rgba, glow: Rgba, out: &mut Vec<Drawn>) {
        // The same placement in 3D: the 2D part, depth scaled alike.
        let t = transform;
        let k = (t[0] * t[3] - t[1] * t[2]).abs().sqrt();
        let parent3: Mat4 = [
            t[0], t[1], 0.0, 0.0, t[2], t[3], 0.0, 0.0, 0.0, 0.0, k, 0.0, t[4], t[5], 0.0, 1.0,
        ];
        self.draw_depth(&transform, &parent3, diffuse, glow, out, 0);
    }

    fn draw_depth(
        &mut self,
        parent: &Mat,
        parent3: &Mat4,
        diffuse: Rgba,
        glow: Rgba,
        out: &mut Vec<Drawn>,
        depth: u32,
    ) {
        if !self.visible || self.hibernate > 0.0 || depth > MAX_DEPTH {
            return;
        }
        let mut s = self.with_effects();
        let mut internal = diffuse;
        internal[3] *= self.base_alpha;
        if internal != [1.0; 4] {
            for (d, k) in s.diffuse.iter_mut().zip(internal) {
                *d *= k;
            }
        }
        if glow[3] > 0.0 {
            // Screen.
            for (g, k) in s.glow.iter_mut().zip(glow) {
                *g = *g + k - k * *g;
            }
        }
        // `PartiallyOpaque`.
        if !(s.diffuse[3] > 0.0 || s.glow[3] > 0.0) {
            return;
        }
        // Translate, rotate, scale, then the alignment (`BeginDraw`). The
        // rotation is `RageMatrixRotationXYZ` seen from the front: x goes to
        // (cZ·cY, sZ·cX) and y to (−sZ·cY, cZ·cX) (`RageMath.cpp:319-327`).
        let rot = [
            (s.rotation[0] + self.base_rotation[0]).to_radians(),
            (s.rotation[1] + self.base_rotation[1]).to_radians(),
            (s.rotation[2] + self.base_rotation[2]).to_radians(),
        ];
        let (sz, cz) = rot[2].sin_cos();
        let (cx, cy) = (rot[0].cos(), rot[1].cos());
        let scale = [
            s.scale[0] * self.base_scale[0],
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
        let m = mul(parent, &local);
        // In 3D (models, and what they are drawn under): the same order,
        // the rotation `Rx(-x)·Ry(-y)·Rz(z)` (`RageMatrixRotationXYZ`).
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
        let m3 = mul4(parent3, &local3);
        match &mut self.kind {
            Kind::Actor => {}
            Kind::Sprite(sprite) => {
                let sized = mul(&m, &[size[0], 0.0, 0.0, size[1], 0.0, 0.0]);
                out.push(Drawn::Quad(Quad {
                    transform: sized,
                    texture: sprite.texture,
                    uv: sprite.custom.unwrap_or(sprite.states[sprite.current].rect),
                    texcoord_offset: sprite.texcoord_offset,
                    diffuse: s.diffuse,
                    glow: s.glow,
                    blend: self.blend,
                }));
            }
            Kind::Model(model) => {
                // MilkShape's y is up (`Model::DrawPrimitives` scales y by
                // -1).
                out.push(Drawn::Model(ModelDraw {
                    transform: mul4(&m3, &scale4([1.0, -1.0, 1.0])),
                    geometry: model.geometry.clone(),
                    materials: model.materials_now(s.diffuse),
                    bones: model.bones.clone(),
                    diffuse: s.diffuse,
                    glow: s.glow,
                    blend: self.blend,
                }));
            }
            Kind::Frame(children) => {
                for c in children {
                    c.draw_depth(&m, &m3, s.diffuse, s.glow, out, depth + 1);
                }
            }
        }
    }
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
        assert!(close(a.current().diffuse[3], 0.75));
        // finishtweening jumps to the end.
        run(&mut a, "finishtweening");
        assert_eq!(a.current().diffuse[3], 1.0);
        // sleep holds, then the queued value lands.
        run(&mut a, "sleep,0.5;diffusealpha,0");
        a.update(0.25, &clocks, &mut p);
        assert_eq!(a.current().diffuse[3], 1.0);
        a.update(0.26, &clocks, &mut p);
        assert_eq!(a.current().diffuse[3], 0.0);
        // A zero-length tween waits for an update with time in it.
        run(&mut a, "linear,0;diffusealpha,1");
        a.update(0.0, &clocks, &mut p);
        assert_eq!(a.current().diffuse[3], 0.0);
        a.update(0.01, &clocks, &mut p);
        assert_eq!(a.current().diffuse[3], 1.0);
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
            a.with_effects().diffuse[0]
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
        // At the start of the cycle the shift is all colour 1.
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
                Drawn::Model(_) => None,
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
        // The material's colour times the diffuse, its frame and scroll by
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
        assert_eq!(q.diffuse, [1.0, 0.5, 1.0, 0.25]);
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
}
