//! Components of a skin's SVG (`docs/plans/noteskin-scene.md`): a
//! `<symbol>` with an `id` and a `viewBox` is a component, in the note's
//! coordinates (the origin at the note's center, up the lane, 64 units a
//! lane; its `viewBox` the part of them it covers). Its children are its
//! parts, in drawing order: one with an `id` a part of that name, which a
//! skin's bindings and roles name (`data-role` gives its role); a run of
//! children without one a part of its own, unnamed. Definitions among them
//! (`defs`, gradients, masks, clip paths, filters, titles) are none.
//!
//! A part with `data-frames` is a group whose children are its frames,
//! one shown at a time (a charge's stages): each frame is drawn on its own,
//! as a part is, its group's attributes kept.
//!
//! Each part is drawn on its own, by the same code as any SVG of a skin:
//! [`part_svg`] makes the part's own document, what of the file the part
//! reaches (through `use`, `href` and `url(#…)`, theirs too) as
//! definitions, inside their ancestors so that what they inherit is as in
//! the file, and the part drawn through `use`, over the component's
//! `viewBox`, so a part keeps its place and every gradient, mask and
//! filter it uses. Ids nothing in it names are left out or made the
//! same, so the same part of two components is the same document, which
//! a shell draws once. Its `width` and `height` are the `viewBox`'s, the
//! size the skin sees; `data-raster-scale` on the symbol (else the file's
//! root) caps its drawing as on a file.

use std::collections::{HashMap, HashSet};

use crate::svg::{self, Element};

/// A component of an SVG file.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgComponent {
    pub id: String,
    /// `[x, y, width, height]` in the note's coordinates.
    pub view_box: [f64; 4],
    pub parts: Vec<SvgPart>,
}

/// A part of a component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvgPart {
    /// Its `id`; `None` for a run of children without one.
    pub name: Option<String>,
    /// Its `data-role`.
    pub role: Option<String>,
    /// Its frames' count: its children's with `data-frames`, else 0.
    pub frames: usize,
}

/// Children of a component that define rather than draw.
const DEFINITIONS: &[&str] = &[
    "defs",
    "title",
    "desc",
    "linearGradient",
    "radialGradient",
    "mask",
    "clipPath",
    "filter",
    "symbol",
];

/// The id given to the elements of an unnamed part in its document.
fn unnamed_id(component: &str, part: usize) -> String {
    format!("ddi-part-{component}-{part}")
}

/// A `viewBox`'s four numbers, its width and height positive.
fn view_box(value: &str) -> Option<[f64; 4]> {
    let n: Vec<f64> = value
        .split([' ', ',', '\t', '\n', '\r'])
        .filter(|s| !s.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [x, y, w, h] = <[f64; 4]>::try_from(n).ok()?;
    (w > 0.0 && h > 0.0 && [x, y, w, h].iter().all(|v| v.is_finite())).then_some([x, y, w, h])
}

/// The symbols that are components: the root's children and its `defs`'.
fn symbols(root: &Element) -> impl Iterator<Item = &Element> {
    root.children.iter().flat_map(|c| {
        let inner: Box<dyn Iterator<Item = &Element>> = if c.name == "defs" {
            Box::new(c.children.iter())
        } else {
            Box::new(std::iter::once(c))
        };
        inner.filter(|e| e.name == "symbol" && e.attribute("id").is_some())
    })
}

/// A framed part's frames: its children that draw.
fn frames(part: &Element) -> impl Iterator<Item = &Element> {
    part.children
        .iter()
        .filter(|c| !DEFINITIONS.contains(&c.name.as_str()))
}

/// A component's parts, each the children it draws.
fn parts(symbol: &Element) -> Vec<(SvgPart, Vec<&Element>)> {
    let mut out: Vec<(SvgPart, Vec<&Element>)> = Vec::new();
    for child in &symbol.children {
        if DEFINITIONS.contains(&child.name.as_str()) {
            continue;
        }
        match child.attribute("id") {
            Some(id) => out.push((
                SvgPart {
                    name: Some(id.to_string()),
                    role: child.attribute("data-role").map(str::to_string),
                    frames: if child.attribute("data-frames").is_some() {
                        frames(child).count()
                    } else {
                        0
                    },
                },
                vec![child],
            )),
            None => match out.last_mut() {
                Some((p, run)) if p.name.is_none() => run.push(child),
                _ => out.push((
                    SvgPart {
                        name: None,
                        role: None,
                        frames: 0,
                    },
                    vec![child],
                )),
            },
        }
    }
    out
}

fn component(symbol: &Element) -> Result<SvgComponent, String> {
    let id = symbol.attribute("id").unwrap_or_default();
    let view_box = symbol
        .attribute("viewBox")
        .and_then(view_box)
        .ok_or_else(|| format!("the component {id} needs a viewBox"))?;
    Ok(SvgComponent {
        id: id.to_string(),
        view_box,
        parts: parts(symbol).into_iter().map(|(p, _)| p).collect(),
    })
}

/// The components of an SVG file (checked as a skin's SVG is), in order.
pub fn components(text: &str) -> Result<Vec<SvgComponent>, String> {
    svg::check_svg(text)?;
    let root = svg::parse(text)?;
    symbols(&root).map(component).collect()
}

/// Part `part` of component `id` of an SVG file (its frame `frame`, for
/// a framed part) as a document of its own, drawn as any SVG of a skin is.
pub fn part_svg(text: &str, id: &str, part: usize, frame: Option<usize>) -> Result<String, String> {
    let root = svg::parse(text)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    File::new(text, &root).part(id, part, frame)
}

/// A parsed file the documents of its parts and ramps are made from.
struct File<'a> {
    text: &'a str,
    root: &'a Element,
    ids: HashMap<&'a str, &'a Element>,
    /// Whether its documents may give ids of their own (`ddi-part…`): it
    /// has none of those already.
    own: bool,
}

impl<'a> File<'a> {
    fn new(text: &'a str, root: &'a Element) -> Self {
        fn collect<'a>(e: &'a Element, ids: &mut HashMap<&'a str, &'a Element>) {
            if let Some(id) = e.attribute("id") {
                ids.entry(id).or_insert(e);
            }
            e.children.iter().for_each(|c| collect(c, ids));
        }
        let mut ids = HashMap::new();
        collect(root, &mut ids);
        let own = !ids.keys().any(|k| k.starts_with("ddi-part"));
        File {
            text,
            root,
            ids,
            own,
        }
    }

    fn part(&self, id: &str, part: usize, frame: Option<usize>) -> Result<String, String> {
        let symbol = symbols(self.root)
            .find(|s| s.attribute("id") == Some(id))
            .ok_or_else(|| format!("no component {id}"))?;
        self.part_of(symbol, &component(symbol)?, &parts(symbol), part, frame)
    }

    /// [`File::part`] of `symbol`, its component `c` and its `parts`.
    fn part_of(
        &self,
        symbol: &Element,
        c: &SvgComponent,
        parts: &[(SvgPart, Vec<&'a Element>)],
        part: usize,
        frame: Option<usize>,
    ) -> Result<String, String> {
        let id = &c.id;
        let (info, elements) = parts
            .get(part)
            .ok_or_else(|| format!("the component {id} has no part {part}"))?;
        let used = self.reached(elements);
        // Ids the document gives: a part's own (when nothing it draws
        // names it, so that the same part of two components is the same
        // document) and an unnamed part's elements'.
        let own = self.own;
        let mut edits = Edits::default();
        let mut uses = String::new();
        match &info.name {
            Some(name) => {
                if let Some(frame) = frame {
                    if frame >= info.frames {
                        return Err(format!("the part {name} has no frame {frame}"));
                    }
                    // The other frames hidden, the group's attributes kept.
                    for (n, f) in frames(elements[0]).enumerate() {
                        if n == frame {
                            continue;
                        }
                        if f.attribute("display").is_some() {
                            return Err(format!("a frame of {name} sets display"));
                        }
                        edits
                            .inserts
                            .push((f.name_end, " display=\"none\"".to_string()));
                    }
                }
                let drawn = if own && !used.names.contains(name.as_str()) {
                    edits.renamed = Some((elements[0] as *const Element, "ddi-part"));
                    "ddi-part"
                } else {
                    name
                };
                uses.push_str(&format!("<use xlink:href=\"#{drawn}\"/>"));
            }
            None if frame.is_some() => {
                return Err(format!("an unnamed part of {id} has no frames"));
            }
            None => {
                for (n, e) in elements.iter().enumerate() {
                    let given = if own {
                        format!("ddi-part-{n}")
                    } else {
                        format!("{}-{n}", unnamed_id(id, part))
                    };
                    edits.inserts.push((e.name_end, format!(" id=\"{given}\"")));
                    uses.push_str(&format!("<use xlink:href=\"#{given}\"/>"));
                }
            }
        }
        let mut whole = used.elements;
        whole.extend(elements.iter().map(|e| *e as *const Element));
        let content = self.pruned(&whole, &edits);
        let [x, y, w, h] = c.view_box;
        let scale = symbol
            .attribute("data-raster-scale")
            .or_else(|| self.root.attribute("data-raster-scale"))
            .map(|s| format!(" data-raster-scale=\"{s}\""))
            .unwrap_or_default();
        Ok(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             width=\"{}\" height=\"{}\" viewBox=\"{x} {y} {w} {h}\"{scale}><defs>{content}</defs>{uses}</svg>",
            w.round().max(1.0),
            h.round().max(1.0),
        ))
    }

    /// What drawing `elements` reaches through references (`use`, a
    /// gradient's `href`, `url(#…)` in any attribute), theirs too: the
    /// elements and the names.
    fn reached(&self, elements: &[&'a Element]) -> Reached<'a> {
        fn refs<'a>(e: &'a Element, out: &mut Vec<&'a str>) {
            for (n, v) in &e.attributes {
                if n == "href" || n.ends_with(":href") {
                    if let Some(id) = v.strip_prefix('#') {
                        out.push(id);
                    }
                    continue;
                }
                let mut rest = v.as_str();
                while let Some(i) = rest.find("url(") {
                    rest = rest[i + 4..].trim_start_matches(['\'', '"', ' ']);
                    let Some(r) = rest.strip_prefix('#') else {
                        break;
                    };
                    let len = r.find([')', '\'', '"', ' ']).unwrap_or(r.len());
                    out.push(&r[..len]);
                    rest = &r[len..];
                }
            }
            e.children.iter().for_each(|c| refs(c, out));
        }
        let mut out = Reached::default();
        let mut todo = Vec::new();
        elements.iter().for_each(|e| refs(e, &mut todo));
        while let Some(id) = todo.pop() {
            if !out.names.insert(id) {
                continue;
            }
            if let Some(e) = self.ids.get(id) {
                out.elements.insert(*e as *const Element);
                refs(e, &mut todo);
            }
        }
        out
    }

    /// The file's content with only the elements `whole` (each with all
    /// it holds) and their ancestors (with only what leads to them, their
    /// ids left out: nothing names them, and properties still inherit as
    /// in the file), in the file's order.
    fn pruned(&self, whole: &HashSet<*const Element>, edits: &Edits) -> String {
        fn leads(e: &Element, whole: &HashSet<*const Element>) -> bool {
            whole.contains(&(e as *const Element)) || e.children.iter().any(|c| leads(c, whole))
        }
        fn emit(
            file: &File,
            e: &Element,
            whole: &HashSet<*const Element>,
            edits: &Edits,
            out: &mut String,
        ) {
            let text = file.text;
            if whole.contains(&(e as *const Element)) {
                let mut at = e.span.0;
                if let Some((_, id)) = edits.renamed.filter(|(r, _)| std::ptr::eq(*r, e)) {
                    let closes = text[e.span.0..e.content.0].ends_with("/>");
                    out.push_str(&open_tag(e, Some(id), closes));
                    at = e.content.0;
                }
                for (offset, insert) in &edits.inserts {
                    if (at..e.span.1).contains(offset) {
                        out.push_str(&text[at..*offset]);
                        out.push_str(insert);
                        at = *offset;
                    }
                }
                out.push_str(&text[at..e.span.1]);
            } else if leads(e, whole) {
                out.push_str(&open_tag(e, None, false));
                for c in &e.children {
                    emit(file, c, whole, edits, out);
                }
                out.push_str(&format!("</{}>", e.name));
            }
        }
        let mut out = String::new();
        let mut edits_sorted = edits.clone();
        edits_sorted.inserts.sort_by_key(|(offset, _)| *offset);
        for c in &self.root.children {
            emit(self, c, whole, &edits_sorted, &mut out);
        }
        out
    }
}

/// What a part reaches ([`File::reached`]).
#[derive(Default)]
struct Reached<'a> {
    elements: HashSet<*const Element>,
    names: HashSet<&'a str>,
}

/// Changes to a part's elements in its document: attributes inserted at
/// offsets of the file, and an element whose id is given another.
#[derive(Clone, Default)]
struct Edits {
    inserts: Vec<(usize, String)>,
    renamed: Option<(*const Element, &'static str)>,
}

/// An element's start tag written again from its attributes, its id
/// `id` (none for `None`).
fn open_tag(e: &Element, id: Option<&str>, closes: bool) -> String {
    let mut out = format!("<{}", e.name);
    if let Some(id) = id {
        out.push_str(&format!(" id=\"{id}\""));
    }
    for (n, v) in &e.attributes {
        if n == "id" {
            continue;
        }
        let v = v
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('"', "&quot;");
        out.push_str(&format!(" {n}=\"{v}\""));
    }
    out.push_str(if closes { "/>" } else { ">" });
    out
}

/// Where a part's (or a frame's) image is found, as the shells read it: a
/// path of the skin's folder, `file#component/part` or
/// `file#component/part/frame`.
pub fn part_path(file: &str, component: &str, part: usize, frame: Option<usize>) -> String {
    match frame {
        Some(f) => format!("{file}#{component}/{part}/{f}"),
        None => format!("{file}#{component}/{part}"),
    }
}

/// A [`part_path`] taken apart: the file, the component, the part and the
/// frame; `None` for a plain file's path.
pub fn split_part_path(path: &str) -> Option<(&str, &str, usize, Option<usize>)> {
    let (file, rest) = path.rsplit_once('#')?;
    if !rest.contains('/') {
        return None;
    }
    let mut fields = rest.split('/');
    let component = fields.next()?;
    let part = fields.next()?.parse().ok()?;
    let frame = match fields.next() {
        Some(f) => Some(f.parse().ok()?),
        None => None,
    };
    fields
        .next()
        .is_none()
        .then_some((file, component, part, frame))
}

/// A gradient's image path, as a material's image (`file#id`): the
/// gradient drawn across 256 pixels, a ramp of its colors.
pub fn ramp_path(file: &str, id: &str) -> String {
    format!("{file}#{id}")
}

/// A [`ramp_path`] taken apart: the file and the gradient's id.
pub fn split_ramp_path(path: &str) -> Option<(&str, &str)> {
    let (file, id) = path.rsplit_once('#')?;
    (!id.is_empty() && !id.contains('/')).then_some((file, id))
}

/// The pixels of a ramp's image.
pub const RAMP_SIZE: (u32, u32) = (256, 4);

/// Gradient `id` of an SVG file drawn across [`RAMP_SIZE`] (its own
/// attributes kept: an `objectBoundingBox` gradient from left to right
/// fills it end to end), as a document drawn as any SVG of a skin is.
pub fn ramp_svg(text: &str, id: &str) -> Result<String, String> {
    let root = svg::parse(text)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let file = File::new(text, &root);
    let gradient = *file
        .ids
        .get(id)
        .ok_or_else(|| format!("no gradient {id}"))?;
    if !matches!(gradient.name.as_str(), "linearGradient" | "radialGradient") {
        return Err(format!("{id} is a <{}>, not a gradient", gradient.name));
    }
    let mut whole = file.reached(&[gradient]).elements;
    whole.insert(gradient as *const Element);
    let (w, h) = RAMP_SIZE;
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
         width=\"{w}\" height=\"{h}\" data-raster-scale=\"1\"><defs>{}</defs>\
         <rect width=\"{w}\" height=\"{h}\" fill=\"url(#{id})\"/></svg>",
        file.pruned(&whole, &Edits::default())
    ))
}

/// The document of an image a skin's file makes (a component's part or
/// frame, a gradient's ramp) at `path`, from that file's `text`; `None`
/// for a plain file's path.
pub fn derived_svg(path: &str, text: &str) -> Option<Result<String, String>> {
    if let Some((_, component, part, frame)) = split_part_path(path) {
        return Some(part_svg(text, component, part, frame));
    }
    split_ramp_path(path).map(|(_, id)| ramp_svg(text, id))
}

/// An SVG file's components (checked as [`components`] does), and every
/// part's and frame's image path with its document ([`part_svg`]): what a
/// shell draws for a component file, the file parsed once.
pub fn part_documents(file: &str, text: &str) -> Result<PartDocuments, String> {
    svg::check_svg(text)?;
    let root = svg::parse(text)?;
    let doc = File::new(text.strip_prefix('\u{feff}').unwrap_or(text), &root);
    let mut out = PartDocuments::default();
    let mut bytes = 0;
    for symbol in symbols(&root) {
        let c = component(symbol)?;
        let parts = parts(symbol);
        for (n, (part, _)) in parts.iter().enumerate() {
            let frames: Vec<Option<usize>> = match part.frames {
                0 => vec![None],
                count => (0..count).map(Some).collect(),
            };
            for f in frames {
                let document = doc.part_of(symbol, &c, &parts, n, f)?;
                bytes += document.len();
                if bytes > MAX_PART_DOCUMENTS {
                    return Err(format!(
                        "its parts' documents pass {} MB together (each holds what its part \
                         reaches: many parts reaching one large definition)",
                        MAX_PART_DOCUMENTS >> 20
                    ));
                }
                out.documents.push((part_path(file, &c.id, n, f), document));
            }
        }
        out.components.push(c);
    }
    Ok(out)
}

/// The most text a file's part documents ([`part_documents`]) may hold
/// together: about its own size in practice, as each holds only what its
/// part reaches, unless many parts reach one large definition.
pub const MAX_PART_DOCUMENTS: usize = 8 * svg::MAX_SVG;

/// An SVG file's components and every part's and frame's image path with
/// its document ([`part_documents`]).
#[derive(Debug, Default)]
pub struct PartDocuments {
    pub components: Vec<SvgComponent>,
    pub documents: Vec<(String, String)>,
}

/// A clock a binding reads (`docs/plans/noteskin-scene.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    /// The song's beat, and for a note its own: in a cycle, Vivid's
    /// displacement by the note's subdivision (StepMania's, as
    /// `crate::notes::animation_percent` gives it); outside one, beats
    /// from the note's (negative before it).
    Note,
    /// The song's beat.
    Beat,
    /// The song's seconds.
    Seconds,
    /// The song's seconds over the roll's timing window (a binding's
    /// length scales it, as `RollAnimationTimingWindowRatio` would).
    Roll,
    /// Seconds since the lane's last press, and its last release.
    Press,
    Release,
    /// Seconds since the judgment an explosion's instance shows (or since
    /// its hold began, for `Holding` and `Rolling`); for a receptor, since
    /// the lane's last press was judged. Once a hold has ended, a cycle on
    /// it finishes and does not start again (the tail: what was under way
    /// fades out, nothing new begins).
    Hit,
    /// Seconds since a `Holding` or `Rolling` instance's hold ended (0
    /// while it lasts, and for any other instance).
    Ended,
}

impl Clock {
    pub fn parse(name: &str) -> Option<Clock> {
        Some(match name {
            "note" => Clock::Note,
            "beat" => Clock::Beat,
            "seconds" => Clock::Seconds,
            "roll" => Clock::Roll,
            "press" => Clock::Press,
            "release" => Clock::Release,
            "hit" => Clock::Hit,
            "ended" => Clock::Ended,
            _ => return None,
        })
    }
}

/// How a curve goes from one key to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    /// Slow then fast (StepMania's `accelerate`: `t^power`), fast then
    /// slow (`decelerate`: `1 - (1 - t)^power`), the power 2 unless given;
    /// slow at both ends (`smooth`).
    In(f32),
    Out(f32),
    InOut,
    /// StepMania's `spring`: past the end and back, settling.
    Spring,
}

impl Ease {
    pub fn parse(name: &str) -> Option<Ease> {
        Some(match name {
            "linear" => Ease::Linear,
            "in" | "accelerate" => Ease::In(2.0),
            "out" | "decelerate" => Ease::Out(2.0),
            "inout" | "smooth" => Ease::InOut,
            "spring" => Ease::Spring,
            _ => return None,
        })
    }

    fn apply(self, t: f32) -> f32 {
        match self {
            Ease::Linear => t,
            Ease::In(p) => t.powf(p),
            Ease::Out(p) => 1.0 - (1.0 - t).powf(p),
            Ease::InOut => t * t * (3.0 - 2.0 * t),
            Ease::Spring => ddi_actor::Ease::Spring.apply(t),
        }
    }
}

/// What a binding makes of its clock.
#[derive(Clone, Debug, PartialEq)]
pub enum Function {
    /// From `from` to `to` over each cycle of `length` (the clock moved by
    /// `offset`).
    Linear {
        length: f32,
        from: f32,
        to: f32,
        offset: f32,
    },
    /// The cycle of `length` in `count` steps: 0 to `count - 1`.
    Steps {
        length: f32,
        count: u32,
        offset: f32,
    },
    /// Keys `(time, value)` in order, eased between; held before the first
    /// and after the last, or repeated every `length`.
    Curve {
        keys: Vec<(f32, f32)>,
        ease: Ease,
        length: Option<f32>,
    },
    /// From `min` to `max`, fixed by the instance's seed and `salt`: the
    /// same for a note (or a judgment's effect) every frame, different
    /// between them, and between parts of other salts.
    Random { min: f32, max: f32, salt: u32 },
}

impl Function {
    /// The cycle's length, for a function that repeats.
    fn cycle(&self) -> Option<f32> {
        match self {
            Function::Linear { length, .. } | Function::Steps { length, .. } => Some(*length),
            Function::Curve { length, .. } => *length,
            Function::Random { .. } => None,
        }
    }
}

/// A property of a part a binding sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    /// Which frame shows (floored, wrapped to the frames).
    Frame,
    /// Between the part's two `Tint` colors, 0 to 1.
    TintMix,
    /// Shown while above 0.5.
    Visible,
    Opacity,
    /// In the component's units (the note's coordinates), and degrees.
    X,
    Y,
    Rotation,
    Zoom,
    /// The material's `phase`.
    Phase,
}

impl Property {
    pub fn parse(name: &str) -> Option<Property> {
        Some(match name {
            "Frame" => Property::Frame,
            "TintMix" => Property::TintMix,
            "Visible" => Property::Visible,
            "Opacity" => Property::Opacity,
            "X" => Property::X,
            "Y" => Property::Y,
            "Rotation" => Property::Rotation,
            "Zoom" => Property::Zoom,
            "Phase" => Property::Phase,
            _ => return None,
        })
    }
}

/// A property set from a clock.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub property: Property,
    pub clock: Clock,
    pub function: Function,
}

/// What the clocks read for one drawn component.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clocks {
    pub beat: f32,
    pub seconds: f32,
    /// The note's beat, for a note's component.
    pub note: Option<f32>,
    /// The roll's scaled timing window, in seconds (0: none).
    pub roll_window: f32,
    /// Seconds since the lane's last press and release (`None`: never).
    pub press: Option<f32>,
    pub release: Option<f32>,
    /// Seconds since the judgment an explosion's instance shows.
    pub hit: Option<f32>,
    /// The instance's seed, 0 to 1 (`Bind.Random`).
    pub seed: f32,
    /// The `hit` clock's value when a held instance's hold ended (`None`
    /// while it lasts, and for other instances).
    pub ended: Option<f32>,
}

/// Seconds a press or release long past counts as.
const LONG_AGO: f32 = 1.0e6;

impl Binding {
    /// Its value at `clocks`.
    pub fn value(&self, clocks: &Clocks) -> f32 {
        let cycle = self.function.cycle().filter(|l| *l > 0.0);
        let clock = match self.clock {
            Clock::Beat => clocks.beat,
            Clock::Seconds => clocks.seconds,
            Clock::Roll if clocks.roll_window > 0.0 => clocks.seconds / clocks.roll_window,
            Clock::Roll => 0.0,
            Clock::Press => clocks.press.unwrap_or(LONG_AGO),
            Clock::Release => clocks.release.unwrap_or(LONG_AGO),
            Clock::Hit => clocks.hit.unwrap_or(LONG_AGO),
            Clock::Ended => match (clocks.hit, clocks.ended) {
                (Some(hit), Some(ended)) => (hit - ended).max(0.0),
                _ => 0.0,
            },
            Clock::Note => match (clocks.note, cycle) {
                // Vivid: the subdivision's fraction quantized down to a
                // step of the cycle (`QuantizeDown`), as whole beats.
                (Some(note), Some(length)) => clocks.beat + (note.rem_euclid(1.0) * length).floor(),
                (Some(note), None) => clocks.beat - note,
                (None, _) => clocks.beat,
            },
        };
        // A cycle on `hit` past the one under way when its hold ended
        // stays at its end.
        let finished = |length: f32, offset: f32| match (self.clock, clocks.ended) {
            (Clock::Hit, Some(ended)) => {
                ((clock + offset) / length).floor() > ((ended + offset) / length).floor()
            }
            _ => false,
        };
        let place = |length: f32, offset: f32| {
            if finished(length, offset) {
                1.0
            } else {
                ((clock + offset) / length).rem_euclid(1.0)
            }
        };
        match &self.function {
            Function::Random { min, max, salt } => {
                // SplitMix64 of the seed's bits and the salt.
                let mut z = u64::from(clocks.seed.to_bits()) ^ (u64::from(*salt) << 32);
                z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
                z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                z ^= z >> 31;
                let u = (z >> 40) as f32 / (1u64 << 24) as f32;
                min + (max - min) * u
            }
            Function::Linear {
                length,
                from,
                to,
                offset,
            } => {
                let u = if *length > 0.0 {
                    place(*length, *offset)
                } else {
                    0.0
                };
                from + (to - from) * u
            }
            Function::Steps {
                length,
                count,
                offset,
            } => {
                if *length <= 0.0 || *count == 0 {
                    return 0.0;
                }
                ((place(*length, *offset) * *count as f32).floor()).min(*count as f32 - 1.0)
            }
            Function::Curve { keys, ease, length } => {
                let t = match length {
                    Some(l) if *l > 0.0 && finished(*l, 0.0) => *l,
                    Some(l) if *l > 0.0 => clock.rem_euclid(*l),
                    _ => clock,
                };
                let Some(first) = keys.first() else {
                    return 0.0;
                };
                if t <= first.0 {
                    return first.1;
                }
                for pair in keys.windows(2) {
                    let ((t0, v0), (t1, v1)) = (pair[0], pair[1]);
                    if t <= t1 {
                        let u = if t1 > t0 { (t - t0) / (t1 - t0) } else { 1.0 };
                        return v0 + (v1 - v0) * ease.apply(u);
                    }
                }
                keys.last().map_or(0.0, |k| k.1)
            }
        }
    }
}

/// How a part blends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PartBlend {
    #[default]
    Normal,
    Add,
}

/// The space a material's coordinates are in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Space {
    /// The component's: turned and zoomed with it.
    #[default]
    Note,
    /// The lane's: the component's place, not its turn or zoom (a field
    /// that stays upright whatever the arrow's direction).
    Lane,
}

/// A part painted by a skin's material (`Def.Material`,
/// `docs/plans/noteskin-scene.md`): its WGSL, the space and origin of its
/// coordinates, its parameters and images.
#[derive(Clone, Debug, PartialEq)]
pub struct PartMaterial {
    /// The WGSL file and its text (`fn paint(m: Material) -> vec4<f32>`).
    pub file: String,
    pub source: String,
    pub space: Space,
    /// Where the material's coordinates are 0, in the component's units.
    pub origin: [f32; 2],
    /// `Material::params`.
    pub params: [[f32; 4]; 8],
    /// Its images' texture ids (at most two; a gradient's ramp or a file).
    pub images: Vec<u32>,
    /// The renderer's index for it, once compiled (the renderer's own).
    pub compiled: Option<u32>,
}

/// A part of a component an element draws.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: Option<String>,
    pub role: Option<String>,
    /// Its image's texture id, or one a frame.
    pub textures: Vec<u32>,
    /// Painted by a material over its image's alpha.
    pub material: Option<Box<PartMaterial>>,
    /// The colors its image is multiplied by (sRGB-encoded, with alpha),
    /// mixed by `TintMix`: white unless `Tint` gives them.
    pub tint: [[f32; 4]; 2],
    pub blend: PartBlend,
    /// Its properties' values without a binding: opacity, x, y, rotation,
    /// zoom, frame, phase, tint mix; and whether it shows.
    pub base: [f32; 8],
    pub visible: bool,
    pub bindings: Vec<Binding>,
    /// An explosion's part: the judgments it shows for (`W1` … `W5`,
    /// `Held`, `HitMine`; `Holding` and `Rolling` while a long note is
    /// held); empty, every judgment.
    pub when: Vec<String>,
    /// An explosion's part: only for a bright judgment (`true`, the combo
    /// past the skin's threshold) or a dim one (`false`).
    pub bright: Option<bool>,
}

/// The order of [`Part::base`].
const BASE: [Property; 8] = [
    Property::Opacity,
    Property::X,
    Property::Y,
    Property::Rotation,
    Property::Zoom,
    Property::Frame,
    Property::Phase,
    Property::TintMix,
];

impl Part {
    /// A part showing its image, with nothing bound.
    pub fn new(name: Option<String>, role: Option<String>, textures: Vec<u32>) -> Part {
        Part {
            name,
            role,
            textures,
            material: None,
            tint: [[1.0; 4]; 2],
            blend: PartBlend::Normal,
            base: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
            visible: true,
            bindings: Vec::new(),
            when: Vec::new(),
            bright: None,
        }
    }

    /// Whether an explosion's instance for judgment `name` (bright or not)
    /// shows this part: one it names, or for an empty list any judgment
    /// but a held long note's.
    pub fn shows_for(&self, name: &str, bright: bool) -> bool {
        let named = if self.when.is_empty() {
            !matches!(name, "Holding" | "Rolling")
        } else {
            self.when.iter().any(|w| w == name)
        };
        named && self.bright.is_none_or(|b| b == bright)
    }

    /// A property's value at `clocks`: its last binding's, else its own.
    pub fn value(&self, property: Property, clocks: &Clocks) -> f32 {
        if let Some(b) = self.bindings.iter().rev().find(|b| b.property == property) {
            return b.value(clocks);
        }
        match BASE.iter().position(|p| *p == property) {
            Some(i) => self.base[i],
            None => f32::from(u8::from(self.visible)),
        }
    }

    /// Sets a property's value without a binding.
    pub fn set(&mut self, property: Property, value: f32) {
        match BASE.iter().position(|p| *p == property) {
            Some(i) => self.base[i] = value,
            None => self.visible = value > 0.5,
        }
    }

    /// This part as drawn at `clocks`, if it shows.
    pub fn pose(&self, clocks: &Clocks) -> Option<PartPose<'_>> {
        let mut v = self.base;
        let mut visible = self.visible;
        for b in &self.bindings {
            let value = b.value(clocks);
            match BASE.iter().position(|p| *p == b.property) {
                Some(i) => v[i] = value,
                None => visible = value > 0.5,
            }
        }
        let [opacity, x, y, rotation, zoom, frame, phase, mix] = v;
        let mix = mix.clamp(0.0, 1.0);
        let [from, to] = self.tint;
        let tint = std::array::from_fn(|i| from[i] + (to[i] - from[i]) * mix);
        if !visible || opacity <= 0.0 || self.textures.is_empty() {
            return None;
        }
        let frames = self.textures.len();
        let frame = (frame.floor() as i64).rem_euclid(frames as i64) as usize;
        Some(PartPose {
            part: self,
            texture: self.textures[frame],
            blend: self.blend,
            opacity: opacity.min(1.0),
            x,
            y,
            rotation,
            zoom,
            phase,
            tint,
        })
    }
}

/// A part as drawn at some clocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartPose<'a> {
    pub part: &'a Part,
    pub texture: u32,
    pub blend: PartBlend,
    pub opacity: f32,
    /// Moved by `x`, `y`, turned by `rotation` degrees and zoomed by `zoom`
    /// about the component's origin, in its units.
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub zoom: f32,
    pub phase: f32,
    /// Its tint now (sRGB-encoded, with alpha).
    pub tint: [f32; 4],
}

/// A component an element draws (`Def.Component`), compiled when the skin
/// loads: its parts in drawing order over its `viewBox`, all turned by
/// `rotation` degrees (a skin turns one arrow for each lane).
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub file: String,
    pub id: String,
    pub view_box: [f32; 4],
    pub rotation: f32,
    pub zoom: f32,
    pub parts: Vec<Part>,
    /// An explosion's: how long each instance shows, in seconds.
    pub duration: f32,
    /// The whole component's own animation: its `X`, `Y`, `Rotation`
    /// (added to its turn) and `Zoom` (times its zoom) bindings, turning
    /// and zooming about `pivot` (its units).
    pub bindings: Vec<Binding>,
    pub pivot: [f32; 2],
}

/// A component's own placement at some clocks ([`Component::placement`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub zoom: f32,
}

impl Component {
    /// Its own `X`, `Y`, `Rotation` and `Zoom` at `clocks` (beyond its turn
    /// and zoom).
    pub fn placement(&self, clocks: &Clocks) -> Placement {
        let mut p = Placement {
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            zoom: 1.0,
        };
        for b in &self.bindings {
            let v = b.value(clocks);
            match b.property {
                Property::X => p.x = v,
                Property::Y => p.y = v,
                Property::Rotation => p.rotation = v,
                Property::Zoom => p.zoom = v,
                _ => {}
            }
        }
        p
    }

    /// Its parts as drawn at `clocks`, those shown, in drawing order.
    pub fn pose<'a>(&'a self, clocks: &'a Clocks) -> impl Iterator<Item = PartPose<'a>> + 'a {
        self.parts.iter().filter_map(move |part| part.pose(clocks))
    }
}

/// Where a long note's material coordinates are 0 along it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    /// At the head: the paint stays with the head as the note shrinks.
    #[default]
    Head,
    /// At the tail.
    Tail,
}

/// A hold or roll drawn by components of the skin's SVG (`Def.LongNote`,
/// `docs/plans/noteskin-scene.md`): its head at the note, its body a tile
/// repeated from the head's join to its cap, its cap at the tail, each a
/// component whose parts' roles (`data-role`) say what they are to it:
/// `band` (where the fill paints), `envelope` (around which the shadows
/// fall), `shadow` (outside the envelope), `overlap-shadow` (on the band,
/// a roll's), `pulse` (a roll's, outside the band), and on the head
/// `protect` (taken from the body's band and shading under the head) and
/// `outside` (the envelope around the head). A head's parts without a role
/// are drawn over the rest.
#[derive(Clone, Debug, PartialEq)]
pub struct LongNote {
    pub head: Option<Component>,
    /// Its `viewBox` is one tile: `pitch` tall from 0.
    pub body: Component,
    /// Its `viewBox` is around the tail (its origin).
    pub cap: Option<Component>,
    /// The body's repeat, in units along the note.
    pub pitch: f32,
    /// Where the body starts below the head (its join to the head).
    pub join: f32,
    /// What paints the band: a material, its `Phase` and their bindings
    /// (a part with no image of its own).
    pub fill: Option<Part>,
    pub anchor: Anchor,
}

/// A lift drawn by components of the skin's SVG (`Def.Lift`,
/// `docs/plans/noteskin-scene.md`): its head a component drawn as a tap's
/// (its parts may have materials of their own), and a trail leading it,
/// shown over the distance the note travels in `seconds` (at least a
/// quarter of a beat), faded toward its far end: the trail component's
/// `trail` part a mask the fill paints, its `shadow` drawn under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Lift {
    pub head: Component,
    /// Its `viewBox` from the note's center: up the lane (negative y) is
    /// ahead of the note; drawn only from the lead's far end.
    pub trail: Option<Component>,
    /// The lead's time, its fade's exponent and opacity.
    pub seconds: f32,
    pub fade: f32,
    pub opacity: f32,
    /// What paints the trail, as a long note's fill.
    pub fill: Option<Part>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARROW: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96">
  <defs>
    <linearGradient id="g"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient>
    <symbol id="arrow" viewBox="-48 -48 96 96">
      <title>An arrow</title>
      <circle r="40" fill="#000" opacity="0.5"/>
      <path id="fill" data-role="mask" d="M0 -32 L32 0 L-32 0 Z" fill="url(#g)"/>
      <rect x="-4" y="0" width="8" height="24" fill="#fff"/>
      <rect x="-2" y="2" width="4" height="20" fill="#888"/>
      <g id="charge" data-frames="" opacity="0.5"><rect x="-30" y="28" width="12" height="6" fill="#0f0"/><rect x="18" y="28" width="12" height="6" fill="#0f0"/></g>
    </symbol>
  </defs>
</svg>"##;

    #[test]
    fn components_and_their_parts() {
        let c = components(ARROW).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].id, "arrow");
        assert_eq!(c[0].view_box, [-48.0, -48.0, 96.0, 96.0]);
        let names: Vec<_> = c[0].parts.iter().map(|p| p.name.as_deref()).collect();
        // The shadow, the fill, the two unnamed rectangles as one part,
        // the charge; the title is none.
        assert_eq!(names, [None, Some("fill"), None, Some("charge")]);
        assert_eq!(c[0].parts[1].role.as_deref(), Some("mask"));
        assert_eq!(c[0].parts[3].frames, 2);
        let docs = part_documents("arrow.svg", ARROW).unwrap().documents;
        let paths: Vec<_> = docs.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            [
                "arrow.svg#arrow/0",
                "arrow.svg#arrow/1",
                "arrow.svg#arrow/2",
                "arrow.svg#arrow/3/0",
                "arrow.svg#arrow/3/1"
            ]
        );
        assert_eq!(
            split_part_path("a b.svg#arrow/3/1"),
            Some(("a b.svg", "arrow", 3, Some(1)))
        );
        assert_eq!(split_part_path("arrow.svg"), None);
        // A frame: the others hidden, the group kept.
        let second = part_svg(ARROW, "arrow", 3, Some(1)).unwrap();
        svg::check_svg(&second).unwrap();
        assert!(second.contains("<rect display=\"none\" x=\"-30\""));
        assert!(second.contains("<rect x=\"18\""));
        assert!(part_svg(ARROW, "arrow", 3, Some(2)).is_err());
    }

    #[test]
    fn bindings_read_their_clocks() {
        let at = |function: Function, clock: Clock, clocks: Clocks| {
            Binding {
                property: Property::Frame,
                clock,
                function,
            }
            .value(&clocks)
        };
        let charge = Function::Steps {
            length: 4.0,
            count: 4,
            offset: 0.0,
        };
        let beat = |beat: f32, note: f32| Clocks {
            beat,
            note: Some(note),
            ..Clocks::default()
        };
        assert_eq!(at(charge.clone(), Clock::Note, beat(0.5, 8.0)), 0.0);
        assert_eq!(at(charge.clone(), Clock::Note, beat(2.5, 8.0)), 2.0);
        // Vivid: an 8th shows the stage two beats on, as StepMania's
        // animation percent does (a quarter of the cycle a step).
        assert_eq!(at(charge.clone(), Clock::Note, beat(0.5, 8.5)), 2.0);
        assert_eq!(at(charge, Clock::Beat, beat(0.5, 8.5)), 0.0);
        let press = Function::Curve {
            keys: vec![(0.0, 0.8), (0.1, 1.0)],
            ease: Ease::Linear,
            length: None,
        };
        let pressed = |s| Clocks {
            press: s,
            ..Clocks::default()
        };
        assert_eq!(at(press.clone(), Clock::Press, pressed(Some(0.05))), 0.9);
        assert_eq!(at(press.clone(), Clock::Press, pressed(None)), 1.0);
        assert_eq!(at(press, Clock::Press, pressed(Some(-1.0))), 0.8);
        let phase = Function::Linear {
            length: 4.0,
            from: 1.0,
            to: 0.0,
            offset: 0.0,
        };
        assert_eq!(at(phase, Clock::Beat, beat(1.0, 0.0)), 0.75);
    }

    #[test]
    fn a_held_instance_finishes_its_cycle_after_the_hold_ends() {
        let at = |clock, function: &Function, hit: f32, ended: Option<f32>| {
            Binding {
                property: Property::Opacity,
                clock,
                function: function.clone(),
            }
            .value(&Clocks {
                hit: Some(hit),
                ended,
                ..Clocks::default()
            })
        };
        let linear = Function::Linear {
            length: 0.2,
            from: 1.0,
            to: 0.0,
            offset: 0.1,
        };
        let steps = Function::Steps {
            length: 0.2,
            count: 4,
            offset: 0.0,
        };
        // Held, every cycle; ended at 0.25, the one under way runs on
        // (Linear's, offset, until 0.3) and the next stays at its end.
        assert!((at(Clock::Hit, &linear, 0.35, None) - 0.75).abs() < 1e-5);
        assert!((at(Clock::Hit, &linear, 0.28, Some(0.25)) - 0.1).abs() < 1e-5);
        assert_eq!(at(Clock::Hit, &linear, 0.35, Some(0.25)), 0.0);
        assert_eq!(at(Clock::Hit, &steps, 0.45, None), 1.0);
        assert_eq!(at(Clock::Hit, &steps, 0.35, Some(0.25)), 3.0);
        assert_eq!(at(Clock::Hit, &steps, 0.45, Some(0.25)), 3.0);
        // Other clocks are left alone.
        assert!((at(Clock::Seconds, &linear, 0.35, Some(0.25)) - 0.5).abs() < 1e-5);
        // `ended`: 0 while held, then the seconds since.
        let ramp = Function::Curve {
            keys: vec![(0.0, 0.0), (1.0, 1.0)],
            ease: Ease::Linear,
            length: None,
        };
        assert_eq!(at(Clock::Ended, &ramp, 0.5, None), 0.0);
        assert!((at(Clock::Ended, &ramp, 0.5, Some(0.25)) - 0.25).abs() < 1e-5);
        assert_eq!(Clock::parse("ended"), Some(Clock::Ended));
    }

    #[test]
    fn random_values_follow_the_seed_and_springs_overshoot() {
        let random = |seed: f32, salt| {
            Binding {
                property: Property::Rotation,
                clock: Clock::Beat,
                function: Function::Random {
                    min: 0.0,
                    max: 360.0,
                    salt,
                },
            }
            .value(&Clocks {
                seed,
                beat: seed * 7.0,
                ..Clocks::default()
            })
        };
        // The same seed, the same value whatever the time; another seed
        // or salt, another.
        assert_eq!(random(0.25, 0), random(0.25, 0));
        assert_ne!(random(0.25, 0), random(0.5, 0));
        assert_ne!(random(0.25, 0), random(0.25, 1));
        assert!((0..64).all(|i| (0.0..360.0).contains(&random(i as f32 / 64.0, 3))));
        // StepMania's spring, past the end on its way.
        let spring = Function::Curve {
            keys: vec![(0.0, 0.0), (1.0, 1.0)],
            ease: Ease::Spring,
            length: None,
        };
        let at = |t| {
            Binding {
                property: Property::Zoom,
                clock: Clock::Beat,
                function: spring.clone(),
            }
            .value(&Clocks {
                beat: t,
                ..Clocks::default()
            })
        };
        assert!((0..=20).any(|i| at(i as f32 / 20.0) > 1.0));
        assert_eq!(at(0.0), 0.0);
    }

    #[test]
    fn a_component_poses_its_parts() {
        let mut charge = Part::new(Some("charge".into()), None, vec![7, 8, 9, 10]);
        charge.bindings.push(Binding {
            property: Property::Frame,
            clock: Clock::Beat,
            function: Function::Steps {
                length: 4.0,
                count: 4,
                offset: 0.0,
            },
        });
        let mut hidden = Part::new(None, None, vec![3]);
        hidden.set(Property::Visible, 0.0);
        let c = Component {
            file: "arrow.svg".into(),
            id: "arrow".into(),
            view_box: [-48.0, -48.0, 96.0, 96.0],
            rotation: 90.0,
            zoom: 1.0,
            parts: vec![Part::new(None, None, vec![1]), hidden, charge],
            duration: 1.0,
            bindings: Vec::new(),
            pivot: [0.0; 2],
        };
        let clocks = Clocks {
            beat: 3.5,
            ..Clocks::default()
        };
        let drawn: Vec<_> = c.pose(&clocks).map(|p| p.texture).collect();
        assert_eq!(drawn, [1, 10]);
    }

    #[test]
    fn a_part_is_a_document_of_its_own() {
        let fill = part_svg(ARROW, "arrow", 1, None).unwrap();
        let checked = svg::check_svg(&fill).unwrap();
        assert_eq!(checked.size, (96, 96));
        assert!(fill.contains("viewBox=\"-48 -48 96 96\""));
        // Its own id given another: nothing it draws names it.
        assert!(fill.ends_with("<use xlink:href=\"#ddi-part\"/></svg>"));
        assert!(fill.contains("<path id=\"ddi-part\" data-role=\"mask\""));
        // An unnamed run: its elements given ids, drawn in order.
        let stem = part_svg(ARROW, "arrow", 2, None).unwrap();
        svg::check_svg(&stem).unwrap();
        assert!(stem.contains("<rect id=\"ddi-part-0\" x=\"-4\""));
        assert!(
            stem.contains("<use xlink:href=\"#ddi-part-0\"/><use xlink:href=\"#ddi-part-1\"/>")
        );
        // A file that has such ids already: the part's own kept, the
        // unnamed run's named for its component.
        let taken = ARROW
            .replace("id=\"g\"", "id=\"ddi-part-g\"")
            .replace("#g)", "#ddi-part-g)");
        assert!(
            part_svg(&taken, "arrow", 1, None)
                .unwrap()
                .ends_with("<use xlink:href=\"#fill\"/></svg>")
        );
        assert!(
            part_svg(&taken, "arrow", 2, None)
                .unwrap()
                .contains("<rect id=\"ddi-part-arrow-2-0\"")
        );
        assert!(part_svg(ARROW, "arrow", 4, None).is_err());
        assert!(part_svg(ARROW, "nope", 0, None).is_err());
        let no_box = ARROW.replace(" viewBox=\"-48 -48 96 96\"", "");
        assert!(components(&no_box).unwrap_err().contains("viewBox"));
    }

    /// Gradients that reach others, a mask whose content inherits its
    /// fill from a group around it, a definition no part uses, two
    /// components with the same part, a part reaching another part and a
    /// part a definition names.
    const REACH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="64" height="64">
  <defs>
    <linearGradient id="base"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient>
    <linearGradient id="g" xlink:href="#base" x2="0" y2="1"/>
    <path id="shape" d="M-20 -20 H20 V20 H-20 Z"/>
    <g fill="#fff"><mask id="m"><use xlink:href="#shape"/></mask></g>
    <radialGradient id="unused"><stop offset="0" stop-color="#ff0"/></radialGradient>
    <linearGradient id="odd" xlink:href="#k"/>
  </defs>
  <symbol id="a" viewBox="-32 -32 64 64" opacity="0.8">
    <rect id="body" x="-30" y="-30" width="60" height="60" fill="url(#g)" mask="url( '#m' )"/>
    <circle id="dot" r="6" fill="url(#unused)"/>
    <path id="edge" d="M-28 28 H28" stroke="#0f0" stroke-width="4"/>
    <rect id="masked" x="-30" y="-30" width="60" height="60" fill="#f0f" mask="url(#me)"/>
    <mask id="me"><use xlink:href="#edge" stroke="#fff"/></mask>
    <rect id="k" width="4" height="4" fill="url(#odd)"/>
  </symbol>
  <symbol id="b" viewBox="-32 -32 64 64" opacity="0.8">
    <rect id="body2" x="-30" y="-30" width="60" height="60" fill="url(#g)" mask="url( '#m' )"/>
  </symbol>
</svg>"##;

    #[test]
    fn a_part_keeps_only_what_it_reaches() {
        let part = |c: &str, n: usize| {
            let doc = part_svg(REACH, c, n, None).unwrap();
            svg::check_svg(&doc).unwrap();
            doc
        };
        let body = part("a", 0);
        // The same part of two components is the same document.
        assert_eq!(body, part("b", 0));
        for id in ["base", "g", "shape"] {
            assert!(body.contains(&format!("id=\"{id}\"")), "{id}");
        }
        // The group around the mask kept, for what its content inherits;
        // ids nothing names left out.
        assert!(body.contains("<g fill=\"#fff\"><mask id=\"m\">"));
        assert!(
            body.contains(
                "<symbol viewBox=\"-32 -32 64 64\" opacity=\"0.8\"><rect id=\"ddi-part\""
            )
        );
        for id in ["unused", "edge", "dot", "odd", "\"a\"", "\"b\""] {
            assert!(!body.contains(id), "{id}");
        }
        let dot = part("a", 1);
        assert!(dot.contains("id=\"unused\"") && !dot.contains("base"));
        // A part another reaches, beside it in its symbol.
        let masked = part("a", 3);
        assert!(masked.contains("<path id=\"edge\"") && masked.contains("<mask id=\"me\">"));
        assert!(!masked.contains("dot"));
        // A part a definition it reaches names keeps its name.
        let k = part("a", 4);
        assert!(k.contains("id=\"odd\"") && k.ends_with("<use xlink:href=\"#k\"/></svg>"));
        // A ramp keeps the gradients it reaches.
        let ramp = ramp_svg(REACH, "g").unwrap();
        assert!(ramp.contains("id=\"base\"") && !ramp.contains("shape"));
        // Read once for a file: the same documents.
        let docs = part_documents("r.svg", REACH).unwrap().documents;
        assert_eq!(docs[0], ("r.svg#a/0".to_string(), body));
    }

    #[test]
    fn many_parts_reaching_one_large_definition_are_refused() {
        // A 600 KB path, and `parts` parts each drawing it.
        let d: String = "M0 0"
            .chars()
            .chain("L1 1".repeat(150_000).chars())
            .collect();
        let file = |parts: usize| {
            let uses: String = (0..parts)
                .map(|n| format!("<use id=\"p{n}\" xlink:href=\"#big\"/>"))
                .collect();
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
                 width=\"64\" height=\"64\"><defs><path id=\"big\" d=\"{d}\"/>\
                 <symbol id=\"c\" viewBox=\"-32 -32 64 64\">{uses}</symbol></defs></svg>"
            )
        };
        // Twenty: 12 MB of documents from a file under a megabyte.
        let many = file(20);
        assert!(many.len() < svg::MAX_SVG);
        let e = part_documents("big.svg", &many).unwrap_err();
        assert!(e.contains("documents pass"), "{e}");
        assert_eq!(
            part_documents("big.svg", &file(3)).unwrap().documents.len(),
            3
        );
    }

    /// A part's document as it was before only what it reaches was
    /// kept: the whole file as definitions, the part drawn by its id.
    #[cfg(feature = "raster")]
    fn whole_file_part(text: &str, component: &str, name: &str) -> String {
        let root = svg::parse(text).unwrap();
        let symbol = symbols(&root)
            .find(|s| s.attribute("id") == Some(component))
            .unwrap();
        let [x, y, w, h] = component_view_box(symbol);
        let (from, to) = root.content;
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             width=\"{w}\" height=\"{h}\" viewBox=\"{x} {y} {w} {h}\"><defs>{}</defs><use xlink:href=\"#{name}\"/></svg>",
            &text[from..to]
        )
    }

    #[cfg(feature = "raster")]
    fn component_view_box(symbol: &Element) -> [f64; 4] {
        component(symbol).unwrap().view_box
    }

    #[cfg(feature = "raster")]
    #[test]
    fn a_part_draws_as_it_did_from_its_whole_file() {
        for (text, component, n, name) in [
            (REACH, "a", 0, "body"),
            (REACH, "a", 1, "dot"),
            (REACH, "a", 2, "edge"),
            (REACH, "a", 3, "masked"),
            (REACH, "b", 0, "body2"),
            (ARROW, "arrow", 1, "fill"),
        ] {
            let pruned =
                crate::raster::decode(part_svg(text, component, n, None).unwrap().as_bytes(), 1.0)
                    .unwrap();
            let whole =
                crate::raster::decode(whole_file_part(text, component, name).as_bytes(), 1.0)
                    .unwrap();
            assert_eq!(pruned.size, whole.size, "{name}");
            assert!(pruned.pixels.iter().any(|p| *p > 0), "{name} draws");
            assert!(pruned.pixels == whole.pixels, "{name} as before");
        }
    }

    #[cfg(feature = "raster")]
    #[test]
    fn a_part_draws_in_its_place() {
        let fill = part_svg(ARROW, "arrow", 1, None).unwrap();
        let drawn = crate::raster::decode(fill.as_bytes(), 1.0).unwrap();
        assert_eq!(drawn.size, (96, 96));
        let alpha = |x: usize, y: usize| drawn.pixels[(y * 96 + x) * 4 + 3];
        // The triangle's middle, above the note's center; nothing of the
        // other parts (the shadow's ring, the stem below the center).
        assert!(alpha(48, 40) > 200, "the fill");
        assert_eq!(alpha(48, 60), 0, "no stem");
        assert_eq!(alpha(10, 48), 0, "no shadow");
    }
}
