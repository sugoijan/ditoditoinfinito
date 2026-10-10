//! Actors as StepMania runs them: the tween queue, effects on their
//! clocks, sprite states, commands, and what is drawn (`src/Actor.cpp`,
//! `src/Tween.cpp`, `src/Sprite.cpp`, `src/ActorFrame.cpp`,
//! `Themes/_fallback/Scripts/02 Actor.lua`, 5_1-new). Positions are in
//! StepMania pixels, a lane being 64; rotations in degrees, positive
//! clockwise on screen (y grows downwards).

use std::f32::consts::PI;

use crate::commands::{Arg, Call};

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
}

/// An actor.
#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    pub name: String,
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
            self.push(actor, "commands play each other without end; stopped".into());
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
        self.timing.iter().sum()
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

    fn update_depth(&mut self, mut dt: f32, clocks: &Clocks, problems: &mut Problems, depth: u32) {
        if depth > MAX_DEPTH {
            return;
        }
        if self.hibernate > 0.0 {
            self.hibernate -= dt;
            if self.hibernate > 0.0 {
                return;
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
        self.update_tweening(dt, problems, depth);
        let effect_delta = self.effect_delta;
        match &mut self.kind {
            Kind::Frame(children) => {
                for c in children {
                    c.update_depth(dt, clocks, problems, depth + 1);
                }
            }
            Kind::Sprite(sprite) => {
                if self.animating {
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
            Kind::Actor => {}
        }
    }

    /// `Actor::UpdateTweening`.
    fn update_tweening(&mut self, mut dt: f32, problems: &mut Problems, depth: u32) {
        let mut guard = 0;
        while !self.tweens.is_empty() && dt > 0.0 {
            guard += 1;
            if guard > 1000 {
                problems.push(&self.name, "tweens queue themselves without end".into());
                self.stop_tweening();
                return;
            }
            let t = &mut self.tweens[0];
            let beginning = t.left == t.time;
            let step = t.left.min(dt);
            t.left -= step;
            dt -= step;
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
            if let Some(c) = command {
                self.play_depth(&c, problems, depth + 1);
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
            problems.push(name, format!("{}: tween time must not be negative", call.method));
        };
        match call.method.as_str() {
            // Tweens.
            "linear" | "accelerate" | "decelerate" | "spring" | "smooth" | "bouncebegin" | "bounceend" | "drop" | "sleep" => {
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
            "zoom" => self.dest().scale = [num(0)?; 3],
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
                if let Kind::Sprite(s) = &mut self.kind {
                    s.set_state(i);
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
                if let Kind::Sprite(s) = &mut self.kind {
                    s.set_seconds_into_animation(t);
                }
            }
            "texcoordvelocity" => {
                let v = [num(0)?, num(1)?];
                if let Kind::Sprite(s) = &mut self.kind {
                    // Padded-texture units, as StepMania's coordinates are.
                    s.texcoord_velocity = [v[0] * s.padded_to_image[0], v[1] * s.padded_to_image[1]];
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
    pub fn draw(&mut self, transform: [f32; 6], diffuse: Rgba, glow: Rgba, out: &mut Vec<Quad>) {
        self.draw_depth(&transform, diffuse, glow, out, 0);
    }

    fn draw_depth(
        &mut self,
        parent: &Mat,
        diffuse: Rgba,
        glow: Rgba,
        out: &mut Vec<Quad>,
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
        match &mut self.kind {
            Kind::Actor => {}
            Kind::Sprite(sprite) => {
                let sized = mul(&m, &[size[0], 0.0, 0.0, size[1], 0.0, 0.0]);
                out.push(Quad {
                    transform: sized,
                    texture: sprite.texture,
                    uv: sprite.custom.unwrap_or(sprite.states[sprite.current].rect),
                    texcoord_offset: sprite.texcoord_offset,
                    diffuse: s.diffuse,
                    glow: s.glow,
                    blend: self.blend,
                });
            }
            Kind::Frame(children) => {
                for c in children {
                    c.draw_depth(&m, s.diffuse, s.glow, out, depth + 1);
                }
            }
        }
    }
}

/// The identity transform.
pub const TRANSFORM_IDENTITY: [f32; 6] = IDENTITY;

#[cfg(test)]
mod tests {
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

    #[test]
    fn frames_hand_down_diffuse_and_place_children() {
        let child = Actor::new(Kind::Sprite(Sprite::new(1, vec![], [64.0, 32.0])));
        let mut f = Actor::new(Kind::Frame(vec![child]));
        run(&mut f, "diffuse,1,0.5,1,0.5;x,100;rotationz,90");
        run(&mut f.children_mut()[0], "y,10;diffusealpha,0.5");
        let mut out = Vec::new();
        f.draw(TRANSFORM_IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        assert_eq!(out.len(), 1);
        let q = out[0];
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
        assert!(close(out[0].transform[5], 16.0));
    }

    #[test]
    fn runaway_commands_stop() {
        // Twice per level, 32 levels deep: billions of calls without a
        // budget.
        let mut a = Actor::new(Kind::Actor);
        a.set_command("Init", parse("playcommand,'Init';playcommand,'Init'").unwrap());
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
        let m = out[0].transform;
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
