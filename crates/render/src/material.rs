//! Materials (`docs/plans/noteskin-scene.md`): a WGSL `paint` function the
//! fragment stage runs for a part's pixels, given what `Material`
//! carries (`material_prelude.wgsl`), its sRGB-encoded result masked by
//! the part's alpha and tinted by the quad's color (`material.wgsl`).
//! [`compile`] checks a skin's function before any pipeline is made: no
//! entry points, bindings or global variables of its own, no loops (WGSL
//! has no recursion, so its cost per pixel is bounded), and the whole
//! shader valid for WebGL2's capabilities. The engine has no paints of its
//! own but the wave its long-note metrics declare ([`HOLD_WAVE`]).

/// The most bytes of a material's source.
pub const MAX_SOURCE: usize = 16 << 10;
/// The most expressions a material's `paint` evaluates with every call
/// inlined (a pixel's work, which no loops leaves bounded only by this).
pub const MAX_INLINED: u64 = 1 << 16;
/// Parameters' slots a frame ([`MaterialFrame::params`]).
pub const SLOTS: usize = 64;

/// A material's parameters: eight `vec4`s, `Material::params`.
pub type MaterialParams = [[f32; 4]; 8];

/// The material the long-note metrics' wave is drawn with
/// ([`crate::textured::TexturedPipeline`] registers it first).
pub const HOLD_WAVE: u32 = 0;

/// [`MaterialQuad::material`] with this bit: the note layer's own material
/// of that index ([`crate::noteskin::NoteLayer::skin_materials`]), which
/// the renderer registers and names by its id before the frame is drawn.
pub const LOCAL: u32 = 1 << 31;

/// The source of the long-note metrics' wave.
pub(crate) const HOLD_WAVE_SOURCE: &str = include_str!("materials/hold_wave.wgsl");

const PRELUDE: &str = include_str!("material_prelude.wgsl");
const STUB: &str = include_str!("material_stub.wgsl");
const STAGE: &str = include_str!("material.wgsl");
const TEXTURED: &str = include_str!("textured.wgsl");

/// Names of the shader a material is compiled into, which it may not use.
const RESERVED: [&str; 12] = [
    "vs_main",
    "fs_main",
    "fs_material",
    "texel_of",
    "shade",
    "Globals",
    "Instance",
    "VsOut",
    "globals",
    "image",
    "image_sampler",
    "DdiMaterials",
];

/// A quad's material: which one, its parameters' slot, where the quad
/// lies in material coordinates and its own inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialQuad {
    /// A registered material ([`crate::textured::TexturedPipeline::add_material`],
    /// or [`HOLD_WAVE`]).
    pub material: u32,
    /// Its parameters' slot in this frame's [`MaterialFrame::params`].
    pub slot: u16,
    /// The unit square (-0.5..0.5) to material coordinates, as a quad's
    /// transform maps it to the screen: `x' = a0 x + a2 y + o0`,
    /// `y' = a1 x + a3 y + o1`.
    pub axes: [f32; 4],
    pub origin: [f32; 2],
    /// `Material::phase`, `seed` and `value`.
    pub phase: f32,
    pub seed: f32,
    pub value: [f32; 4],
    /// Its images' registered textures (`image_at`, `ramp`), or
    /// [`crate::textured::UNTEXTURED`].
    pub images: [u32; 2],
}

/// A frame's materials' inputs shared by every quad.
#[derive(Clone, Copy, Debug, Default)]
pub struct MaterialFrame<'a> {
    /// The parameters' slots quads name ([`MaterialQuad::slot`]); past
    /// [`SLOTS`], left out.
    pub params: &'a [MaterialParams],
    pub beat: f32,
    pub seconds: f32,
    /// Colors are made in linear light (the skin's `srgb`): a material's
    /// encoded colors are converted.
    pub linear: bool,
}

/// A checked material: the shader its pipelines are made from.
#[derive(Clone, Debug)]
pub struct Compiled {
    pub(crate) source: String,
    /// A hash of `source`, to find its pipelines by each frame.
    pub(crate) key: u64,
}

/// Why a material was refused: what, and the line of its source when
/// there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialError {
    pub line: Option<usize>,
    pub message: String,
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for MaterialError {}

fn refuse(line: Option<usize>, message: impl Into<String>) -> MaterialError {
    MaterialError {
        line,
        message: message.into(),
    }
}

/// The material's own line of `location` in a shader whose material
/// begins after `before` lines.
fn own_line(location: Option<naga::SourceLocation>, before: usize) -> Option<usize> {
    let line = location?.line_number as usize;
    (line > before).then(|| line - before)
}

/// Whether `block` holds a loop, and the span of the first.
fn first_loop(block: &naga::Block) -> Option<naga::Span> {
    for (statement, span) in block.span_iter() {
        let inner = match statement {
            naga::Statement::Loop { .. } => return Some(*span),
            naga::Statement::Block(b) => first_loop(b),
            naga::Statement::If { accept, reject, .. } => {
                first_loop(accept).or_else(|| first_loop(reject))
            }
            naga::Statement::Switch { cases, .. } => cases.iter().find_map(|c| first_loop(&c.body)),
            _ => None,
        };
        if inner.is_some() {
            return inner;
        }
    }
    None
}

/// What each of `module`'s functions evaluates with the functions it
/// calls inlined, in expressions, by function index: naga lists a
/// function after those it calls, and WGSL has no recursion, so one pass
/// in order sees each callee's first.
fn inlined_costs(module: &naga::Module) -> Vec<u64> {
    fn calls(block: &naga::Block, out: &mut Vec<naga::Handle<naga::Function>>) {
        for statement in block.iter() {
            match statement {
                naga::Statement::Call { function, .. } => out.push(*function),
                naga::Statement::Block(b) => calls(b, out),
                naga::Statement::If { accept, reject, .. } => {
                    calls(accept, out);
                    calls(reject, out);
                }
                naga::Statement::Switch { cases, .. } => {
                    cases.iter().for_each(|c| calls(&c.body, out));
                }
                naga::Statement::Loop {
                    body, continuing, ..
                } => {
                    calls(body, out);
                    calls(continuing, out);
                }
                _ => {}
            }
        }
    }
    let mut costs: Vec<u64> = Vec::with_capacity(module.functions.len());
    let mut callees = Vec::new();
    for (_, f) in module.functions.iter() {
        callees.clear();
        calls(&f.body, &mut callees);
        let cost = callees.iter().fold(f.expressions.len() as u64, |sum, h| {
            sum.saturating_add(costs.get(h.index()).copied().unwrap_or(u64::MAX))
        });
        costs.push(cost);
    }
    costs
}

/// The names a module declares: its types', constants' and functions'.
fn names(module: &naga::Module) -> Vec<String> {
    let types = module.types.iter().filter_map(|(_, t)| t.name.clone());
    let constants = module.constants.iter().filter_map(|(_, c)| c.name.clone());
    let functions = module.functions.iter().filter_map(|(_, f)| f.name.clone());
    types.chain(constants).chain(functions).collect()
}

/// Checks a material's `paint` source and makes the shader its pipelines
/// are made from.
pub fn compile(paint: &str) -> Result<Compiled, MaterialError> {
    if paint.len() > MAX_SOURCE {
        return Err(refuse(
            None,
            format!(
                "the material is {} bytes, more than {MAX_SOURCE}",
                paint.len()
            ),
        ));
    }
    // The material with what it is given, alone: its own rules.
    let given = format!("{PRELUDE}{STUB}");
    let own = format!("{given}{paint}");
    let before = given.lines().count();
    let module = naga::front::wgsl::parse_str(&own)
        .map_err(|e| refuse(own_line(e.location(&own), before), e.message()))?;
    let line = |span: naga::Span| own_line(Some(span.location(&own)), before);
    if let Some(e) = module.entry_points.first() {
        return Err(refuse(
            line(
                e.function
                    .body
                    .span_iter()
                    .next()
                    .map_or(naga::Span::UNDEFINED, |s| *s.1),
            ),
            format!("a material declares no entry points (`{}`)", e.name),
        ));
    }
    if let Some((h, _)) = module.global_variables.iter().next() {
        return Err(refuse(
            line(module.global_variables.get_span(h)),
            "a material declares no bindings or global variables",
        ));
    }
    let prelude = names(&naga::front::wgsl::parse_str(&given).expect("the prelude parses"));
    for name in names(&module) {
        if !prelude.contains(&name)
            && (name.starts_with("ddi_") || RESERVED.contains(&name.as_str()))
        {
            return Err(refuse(None, format!("`{name}` is the engine's name")));
        }
    }
    for (h, f) in module.functions.iter() {
        if let Some(span) = first_loop(&f.body) {
            let at = line(span).or_else(|| line(module.functions.get_span(h)));
            return Err(refuse(
                at,
                "a material has no loops (the prelude's `fbm4` and `fbm8` are unrolled)",
            ));
        }
    }
    let Some((h, f)) = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some("paint"))
    else {
        return Err(refuse(
            None,
            "a material defines `fn paint(m: Material) -> vec4<f32>`",
        ));
    };
    let material =
        |ty: naga::Handle<naga::Type>| module.types[ty].name.as_deref() == Some("Material");
    let vec4 = |ty: naga::Handle<naga::Type>| {
        matches!(
            module.types[ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar::F32,
            }
        )
    };
    if f.arguments.len() != 1
        || !material(f.arguments[0].ty)
        || !f.result.as_ref().is_some_and(|r| vec4(r.ty))
    {
        return Err(refuse(
            line(module.functions.get_span(h)),
            "`paint` takes a `Material` and returns a `vec4<f32>`",
        ));
    }
    // Without loops, what a pixel costs is what `paint` evaluates with its
    // calls inlined: functions calling others twice over can make that
    // grow as fast as a loop would.
    let cost = inlined_costs(&module)[h.index()];
    if cost > MAX_INLINED {
        return Err(refuse(
            line(module.functions.get_span(h)),
            format!(
                "`paint` evaluates {cost} expressions with its calls inlined, more than {MAX_INLINED}"
            ),
        ));
    }
    // The whole shader, as WebGL2 must take it.
    let source = format!("{TEXTURED}\n{STAGE}\n{PRELUDE}{paint}");
    let before = TEXTURED.lines().count() + 1 + STAGE.lines().count() + 1 + PRELUDE.lines().count();
    let module = naga::front::wgsl::parse_str(&source)
        .map_err(|e| refuse(own_line(e.location(&source), before), e.message()))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| {
        let at = e
            .spans()
            .find_map(|(span, _)| own_line(Some(span.location(&source)), before));
        refuse(at, e.as_inner().to_string())
    })?;
    // As WebGL2 takes it: GLSL ES 3.0, which lacks some of WGSL.
    for (stage, entry) in [
        (naga::ShaderStage::Vertex, "vs_main"),
        (naga::ShaderStage::Fragment, "fs_material"),
    ] {
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
        let mut glsl = String::new();
        naga::back::glsl::Writer::new(
            &mut glsl,
            &module,
            &info,
            &options,
            &pipeline,
            naga::proc::BoundsCheckPolicies::default(),
        )
        .and_then(|mut w| w.write().map(|_| ()))
        .map_err(|e| refuse(None, format!("WebGL2 cannot run it: {e}")))?;
    }
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut h);
        h.finish()
    };
    Ok(Compiled { source, key })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hold_wave_compiles() {
        compile(HOLD_WAVE_SOURCE).expect("the wave compiles");
    }

    #[test]
    fn a_foil_with_noise_compiles() {
        let foil = "fn paint(m: Material) -> vec4<f32> {\n\
                    let n = fbm8(m.pos / 48.0 + vec2<f32>(m.beat * 0.1, m.seed));\n\
                    let t = fract(n + m.screen.y * 0.3 + m.phase);\n\
                    let c = oklab_mix(hsv(t, 0.6, 1.0), ramp(0u, t + n), 0.3);\n\
                    return vec4<f32>(c, m.mask * image_at(1u, m.uv).a);\n\
                    }\n";
        compile(foil).expect("the foil compiles");
    }

    #[test]
    fn what_a_material_may_not_do() {
        let error = |source: &str| compile(source).unwrap_err();
        let looped = "fn paint(m: Material) -> vec4<f32> {\n\
                      var v = 0.0;\n\
                      for (var i = 0; i < 4; i++) {\n\
                      v += noise(m.pos * f32(i));\n\
                      }\n\
                      return vec4<f32>(v);\n\
                      }\n";
        let e = error(looped);
        assert!(e.message.contains("no loops"), "{e}");
        assert_eq!(e.line, Some(3), "{e}");
        let bound = "@group(0) @binding(0) var<uniform> x: vec4<f32>;\n\
                     fn paint(m: Material) -> vec4<f32> { return x; }\n";
        assert!(error(bound).message.contains("no bindings"));
        let entry = "@fragment fn main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }\n\
                     fn paint(m: Material) -> vec4<f32> { return vec4<f32>(1.0); }\n";
        assert!(error(entry).message.contains("no entry points"));
        let shape = "fn paint(m: Material) -> vec3<f32> { return vec3<f32>(1.0); }\n";
        assert!(error(shape).message.contains("returns a `vec4<f32>`"));
        // No loop, but each function calling the one before twice: a
        // pixel's work doubling forty times over.
        let mut chain = String::from("fn g0(p: vec2<f32>) -> f32 { return noise(p); }\n");
        for k in 1..=40 {
            chain.push_str(&format!(
                "fn g{k}(p: vec2<f32>) -> f32 {{ return g{j}(p) + g{j}(p + 1.0); }}\n",
                j = k - 1
            ));
        }
        chain.push_str("fn paint(m: Material) -> vec4<f32> { return vec4<f32>(g40(m.pos)); }\n");
        assert!(chain.len() < MAX_SOURCE);
        let e = error(&chain);
        assert!(e.message.contains("with its calls inlined"), "{e}");
        // Sixteen octaves of the prelude's noise eight deep are fine.
        let mut many = String::from("fn paint(m: Material) -> vec4<f32> {\nvar v = 0.0;\n");
        for k in 0..16 {
            many.push_str(&format!("v += fbm8(m.pos + {k}.0);\n"));
        }
        many.push_str("return vec4<f32>(v);\n}\n");
        compile(&many).expect("sixteen fbm8 calls compile");
        assert!(
            error("fn other() {}\n")
                .message
                .contains("defines `fn paint")
        );
        let reserved =
            "fn shade() {}\nfn paint(m: Material) -> vec4<f32> { return vec4<f32>(1.0); }\n";
        assert!(
            error(reserved).message.contains("engine's name"),
            "{}",
            error(reserved)
        );
        let typo = "fn paint(m: Material) -> vec4<f32> {\n  return vec4<f32>(m.nope);\n}\n";
        assert_eq!(error(typo).line, Some(2), "{}", error(typo));
        let nested = "fn paint(m: Material) -> vec4<f32> {\n\
                      var a: array<array<f32, 2>, 2>;\n\
                      a[0][1] = m.phase;\n\
                      return vec4<f32>(a[0][1]);\n\
                      }\n";
        assert!(
            error(nested).message.contains("WebGL2"),
            "{}",
            error(nested)
        );
        let long = format!(
            "fn paint(m: Material) -> vec4<f32> {{ return vec4<f32>(1.0); }}\n//{}",
            "x".repeat(MAX_SOURCE)
        );
        assert!(error(&long).message.contains("more than"));
    }
}
