//! StepMania 3.9's and OpenITG's noteskin format (`docs/research/noteskins.md`
//! section 11; `docs/plans/legacy-skins.md`): which skins are in it, and its
//! actor files read into one node tree, as `XNode` holds them: `.actor`
//! (an INI whose `[Actor]` section is a node), `.sprite` (`[Sprite]`),
//! `.model` (`[Model]`) and `.xml` (a tree of elements, children under
//! `<children>`). `crate::build` makes actors of the nodes. Files are
//! untrusted: the XML reader is strict and bounded.

use crate::actor::{Actor, Kind, Problems, State};
use crate::build::{Builder, Built, builtin_sprite, fallback_art, image_sprite, model_actor};
use crate::builtin::{LEGACY_NOTE_TYPES, Virtual};
use crate::commands::{Call, parse_legacy};
use crate::ini::{Ini, string_to_float, string_to_int};
use crate::load::{Button, Source};
use crate::metrics::Part;
use crate::skin::{FileRef, Folder};
use std::sync::Arc;

/// A folder in the old format: actor files StepMania 5 skins never use
/// (`.sprite`, `.actor`, `.model`), and no `NoteSkin.lua`.
pub fn is_legacy(folder: &Folder) -> bool {
    !folder.has_file("NoteSkin.lua")
        && folder.files.iter().any(|f| {
            !f.contains('/') && {
                let lower = f.to_ascii_lowercase();
                [".sprite", ".actor", ".model"]
                    .iter()
                    .any(|x| lower.ends_with(x))
            }
        })
}

/// Whether skin `name` of `game`, or a skin it falls back on among
/// `folders`, is in the old format.
pub fn is_legacy_chain(folders: &[Folder], game: &str, name: &str) -> bool {
    let mut current = name.to_string();
    for _ in 0..20 {
        let Some(f) = folders
            .iter()
            .find(|f| f.game.eq_ignore_ascii_case(game) && f.name.eq_ignore_ascii_case(&current))
        else {
            return false;
        };
        if is_legacy(f) {
            return true;
        }
        // A StepMania 5 skin, whatever it falls back on.
        if f.has_file("NoteSkin.lua") {
            return false;
        }
        let ini = Ini::parse(f.text("metrics.ini").unwrap_or(""));
        match ini.get("Global", "FallbackNoteSkin") {
            Some(next) if !next.is_empty() => current = next.to_string(),
            _ => return false,
        }
    }
    false
}

/// An actor node: its element name (an INI section's, an XML tag's), its
/// attributes in order, its child elements.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    /// `GetAttrValue`: case-sensitive, the first of the name.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// `GetChild`.
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// The `[section]` of an INI file as a node (`IniFile::GetChild`).
    pub fn from_ini(text: &str, section: &str) -> Option<Node> {
        let ini = Ini::parse(text);
        ini.section_names().any(|s| s == section).then(|| Node {
            name: section.to_string(),
            attrs: ini
                .section(section)
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            children: Vec::new(),
        })
    }
}

/// Most elements an XML actor file may have.
const MAX_NODES: usize = 4096;
/// Most nested elements.
const MAX_DEPTH: usize = 32;
/// Most attributes an element (or keys a section) may have: a sprite of
/// 256 frames.
const MAX_ATTRIBUTES: usize = 520;

/// An XML actor file's root element (`XNode::LoadFromFile`): elements with
/// attributes, comments, a declaration; text between elements is ignored.
pub fn parse_xml(text: &str) -> Result<Node, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        text,
        i: 0,
        nodes: 0,
    };
    p.misc()?;
    let root = p.element(0)?;
    p.misc()?;
    if p.i < p.s.len() {
        return Err(format!("text after the root element at byte {}", p.i));
    }
    Ok(root)
}

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
    nodes: usize,
}

impl Parser<'_> {
    fn starts(&self, w: &str) -> bool {
        self.s[self.i..].starts_with(w.as_bytes())
    }

    fn skip_space(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    /// Skips up to and past `end`.
    fn skip_past(&mut self, end: &str) -> Result<(), String> {
        match self.text[self.i..].find(end) {
            Some(n) => {
                self.i += n + end.len();
                Ok(())
            }
            None => Err(format!("unclosed {end:?}")),
        }
    }

    /// Whitespace, comments, declarations and processing instructions.
    fn misc(&mut self) -> Result<(), String> {
        loop {
            self.skip_space();
            if self.starts("<!--") {
                self.skip_past("-->")?;
            } else if self.starts("<?") {
                self.skip_past("?>")?;
            } else if self.starts("<!") {
                self.skip_past(">")?;
            } else if self.starts("\u{feff}") {
                self.i += 3;
            } else {
                return Ok(());
            }
        }
    }

    fn name(&mut self) -> Result<String, String> {
        let start = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_alphanumeric() || b"_-.:".contains(&self.s[self.i]))
        {
            self.i += 1;
        }
        if start == self.i {
            return Err(format!("a name expected at byte {start}"));
        }
        Ok(self.text[start..self.i].to_string())
    }

    fn element(&mut self, depth: usize) -> Result<Node, String> {
        if depth >= MAX_DEPTH {
            return Err("elements nested too deep".into());
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err("too many elements".into());
        }
        if !self.starts("<") {
            return Err(format!("an element expected at byte {}", self.i));
        }
        self.i += 1;
        let mut node = Node {
            name: self.name()?,
            ..Node::default()
        };
        loop {
            self.skip_space();
            if self.starts("/>") {
                self.i += 2;
                return Ok(node);
            }
            if self.starts(">") {
                self.i += 1;
                break;
            }
            let key = self.name()?;
            self.skip_space();
            if !self.starts("=") {
                return Err(format!("{key}: '=' expected"));
            }
            self.i += 1;
            self.skip_space();
            let quote = match self.s.get(self.i) {
                Some(&q @ (b'"' | b'\'')) => q,
                _ => return Err(format!("{key}: a quoted value expected")),
            };
            self.i += 1;
            let start = self.i;
            while self.i < self.s.len() && self.s[self.i] != quote {
                self.i += 1;
            }
            if self.i >= self.s.len() {
                return Err(format!("{key}: unclosed value"));
            }
            let value = unescape(&self.text[start..self.i]);
            self.i += 1;
            if node.attrs.len() >= MAX_ATTRIBUTES {
                return Err(format!(
                    "<{}> has more than {MAX_ATTRIBUTES} attributes",
                    node.name
                ));
            }
            node.attrs.push((key, value));
        }
        // Content: child elements, comments, ignored text, then the end tag.
        loop {
            while self.i < self.s.len() && self.s[self.i] != b'<' {
                self.i += 1;
            }
            if self.i >= self.s.len() {
                return Err(format!("<{}> is not closed", node.name));
            }
            if self.starts("</") {
                self.i += 2;
                let end = self.name()?;
                if end != node.name {
                    return Err(format!("<{}> closed by </{end}>", node.name));
                }
                self.skip_space();
                if !self.starts(">") {
                    return Err(format!("</{end}: '>' expected"));
                }
                self.i += 1;
                return Ok(node);
            }
            if self.starts("<!--") || self.starts("<?") || self.starts("<!") {
                self.misc()?;
                continue;
            }
            let child = self.element(depth + 1)?;
            node.children.push(child);
        }
    }
}

/// XML's five entities and character references.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let c = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Most actors one element's files may make.
const MAX_ACTORS: usize = 4096;
/// Most files deep an element's files may load each other.
const MAX_FILE_DEPTH: u32 = 8;

/// What building a skin's actor files may cost, for all its elements:
/// actors made, and work (an actor and each of its node's attributes, a
/// file read per 64 bytes). Skins in the format make a few thousand
/// actors; hostile files stop here instead of stalling a load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub actors: u32,
    pub work: u64,
}

/// [`Budget`] for one skin.
pub const SKIN_BUDGET: Budget = Budget {
    actors: 32_768,
    work: 4_000_000,
};

/// OpenITG's tap scores (`TapNoteScoreNames`), each with the play's name
/// for it (`ddi_render::noteskin`; `None`: the play never sends it).
const TAP_SCORES: [(&str, Option<&str>); 9] = [
    ("None", None),
    ("HitMine", Some("HitMine")),
    ("AvoidMine", None),
    ("Miss", Some("Miss")),
    ("Boo", Some("W5")),
    ("Good", Some("W4")),
    ("Great", Some("W3")),
    ("Perfect", Some("W2")),
    ("Marvelous", Some("W1")),
];

/// OpenITG's hold scores (`HoldNoteScoreNames`), the same way.
const HOLD_SCORES: [(&str, Option<&str>); 3] = [("None", None), ("NG", None), ("OK", Some("Held"))];

/// The play's names for what lights an explosion: [`TAP_SCORES`] and
/// [`HOLD_SCORES`].
const EXPLOSION_EVENTS: [&str; 8] = ["W1", "W2", "W3", "W4", "W5", "Miss", "HitMine", "Held"];

/// The part whose metrics a note element of the engine's takes
/// (`NoteDisplay::Load`: rolls take the holds').
fn note_part(element: &str) -> Option<Part> {
    let mut words = element.split(' ');
    match (words.next()?, words.next()?) {
        ("Tap", "Note") => Some(Part::TapNote),
        ("Tap", "Mine") => Some(Part::TapMine),
        ("Hold" | "Roll", part) => match part.to_ascii_lowercase().as_str() {
            "head" => Some(Part::HoldHead),
            "topcap" => Some(Part::HoldTopCap),
            "body" => Some(Part::HoldBody),
            "bottomcap" => Some(Part::HoldBottomCap),
            "tail" => Some(Part::HoldTail),
            _ => None,
        },
        _ => None,
    }
}

/// `@NOTESKIN:GetPath('<button>','<element>')`, the only Lua the format's
/// skins use in a `File` (`docs/research/noteskins.md` section 11): its
/// button and element.
fn get_path_call(expr: &str) -> Option<(&str, &str)> {
    let args = expr
        .trim()
        .strip_prefix("NOTESKIN:GetPath")?
        .trim_start()
        .strip_prefix('(')?
        .trim_end()
        .strip_suffix(')')?;
    let (button, element) = args.split_once(',')?;
    Some((quoted(button)?, quoted(element)?))
}

/// A Lua string literal's text (no escapes).
fn quoted(s: &str) -> Option<&str> {
    let s = s.trim();
    let quote = s.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    s.strip_prefix(quote)?.strip_suffix(quote)
}

/// The folder of `path` (a `/NoteSkins/…` path), with its slash.
fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..=i])
}

/// `Frame0012` → 12: a sprite's state key (`Frame%04d`, `Delay%04d`).
fn state_index(key: &str, prefix: &str) -> Option<usize> {
    let digits = key.strip_prefix(prefix)?;
    (digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

fn visible(on: bool) -> Call {
    Call {
        method: "visible".into(),
        args: vec![crate::commands::Arg::Bool(on)],
    }
}

impl Builder<'_, '_> {
    /// An element of a skin in StepMania 3.9's format: the receptor and
    /// the explosions built natively as OpenITG draws them, a note part in
    /// note type `color`'s file when its metrics colour it, lifts by the
    /// built-in skin (the format has none) and fakes as tap notes.
    pub(crate) fn legacy_element(&mut self, button: Button, element: &str, color: &str) -> Built {
        match element {
            "Receptor" => return self.legacy_receptor(button),
            "Explosion" => return Built::Actor(self.legacy_explosion(button)),
            "Tap Lift" => return Built::BuiltIn,
            _ => {}
        }
        let element = if element == "Tap Fake" {
            "Tap Note"
        } else {
            element
        };
        let colored = note_part(element).is_some_and(|part| {
            crate::metrics::NoteDisplay::read(self.skin, button.name())
                .part(part)
                .note_color
        });
        if !colored {
            return self.element(button, element, 0);
        }
        // `NoteColorActor::Get`: the first eight types have files.
        let i = LEGACY_NOTE_TYPES
            .iter()
            .position(|t| *t == color)
            .unwrap_or(0)
            .min(7);
        self.element(button, &format!("{element} {}", LEGACY_NOTE_TYPES[i]), 0)
    }

    /// One part of the receptor or the explosions: the skin's, nothing for
    /// a blank, else (it cannot be shown) the stand-in's art `art`, or
    /// nothing for a part OpenITG would draw nothing of.
    fn legacy_part(&mut self, button: Button, element: &str, art: Option<&str>) -> Actor {
        match self.element(button, element, 1) {
            Built::Actor(a) => a,
            Built::Blank => Actor::new(Kind::Actor),
            Built::BuiltIn => match art {
                Some(art) => self.builtin_sprite(fallback_art(art)),
                None => Actor::new(Kind::Actor),
            },
        }
    }

    /// `ReceptorArrow` (`src/ReceptorArrow.cpp`): the key-press block,
    /// shown while the button is down, under the receptor, which a step
    /// gives the `[ReceptorArrow]` command of its score and then `Press`,
    /// both on the beat's clock. The receptor shown while gameplay waits
    /// (`receptor waiting`) is left out: the play starts at once. Drawn by
    /// the built-in skin when the receptor is the stand-in's.
    fn legacy_receptor(&mut self, button: Button) -> Built {
        let mut go = match self.element(button, "Receptor Go", 1) {
            Built::Actor(a) => a,
            Built::Blank => Actor::new(Kind::Actor),
            Built::BuiltIn => return Built::BuiltIn,
        };
        let beat = Self::calls("effectclock,\"beat\"");
        let mut p = Problems::default();
        go.set_command("LegacyClock", beat.clone());
        go.play_own("LegacyClock", &mut p);
        for (score, event) in TAP_SCORES {
            let Some(event) = event else { continue };
            let mut calls = Self::calls("finishtweening");
            calls.extend(self.metric_command("ReceptorArrow", &format!("{score}Command")));
            go.set_command(event, calls);
        }
        // A step without a note.
        let mut calls = Self::calls("finishtweening");
        calls.extend(self.metric_command("ReceptorArrow", "NoneCommand"));
        go.set_command("None", calls);
        let mut block = self.legacy_part(button, "KeypressBlock", None);
        block.set_command("LegacyClock", beat);
        block.play_own("LegacyClock", &mut p);
        block.visible = false;
        block.set_command("Press", vec![visible(true)]);
        block.set_command("Lift", vec![visible(false)]);
        for m in p.messages {
            self.note(m);
        }
        // Each part played its `Init` as it loaded.
        Built::Actor(Actor::new(Kind::Frame(vec![block, go])))
    }

    /// `GhostArrowRow` (`src/GhostArrow.cpp`, `src/HoldGhostArrow.cpp`):
    /// the dim and the bright row, each one actor per score
    /// (`tap explosion dim Great`), hidden until its score shows it with
    /// the `[GhostArrowDim]` or `[GhostArrowBright]` command, which hides
    /// the row's others but a mine's; then the hold explosion, drawn while
    /// a hold or roll is active, with `[HoldGhostArrow] OnCommand` run as
    /// it loads. The play names a row's score `Dim<score>` or
    /// `Bright<score>` (`W1`, `Held`, `HitMine`, …).
    fn legacy_explosion(&mut self, button: Button) -> Actor {
        let mut rows = Vec::new();
        for (row, section) in [("Dim", "GhostArrowDim"), ("Bright", "GhostArrowBright")] {
            let art = format!("Tap Explosion {row}");
            let mut children = Vec::new();
            for (score, event) in TAP_SCORES.iter().chain(&HOLD_SCORES) {
                // What the play never shows, and a miss, show nothing when
                // they cannot load.
                let fallback = match *score {
                    "HitMine" => Some("HitMine Explosion"),
                    "None" | "AvoidMine" | "Miss" | "NG" => None,
                    _ => Some(art.as_str()),
                };
                let mut a = self.legacy_part(button, &format!("{art} {score}"), fallback);
                a.visible = false;
                let shown = self.metric_command(section, &format!("{score}Command"));
                for e in EXPLOSION_EVENTS {
                    let name = format!("{row}{e}");
                    if *event == Some(e) {
                        let mut calls = vec![visible(true)];
                        calls.extend(Self::calls("stoptweening"));
                        calls.extend(shown.iter().cloned());
                        a.set_command(&name, calls);
                    } else if *score != "HitMine" {
                        let mut calls = Self::calls("stoptweening");
                        calls.push(visible(false));
                        a.set_command(&name, calls);
                    }
                }
                children.push(a);
            }
            rows.push(Actor::new(Kind::Frame(children)));
        }
        let mut hold = self.legacy_part(button, "Hold Explosion", Some("Hold Explosion"));
        let on = self.metric_command("HoldGhostArrow", "OnCommand");
        hold.set_command("LegacyOn", on);
        let mut p = Problems::default();
        hold.play_own("LegacyOn", &mut p);
        for m in p.messages {
            self.note(m);
        }
        hold.visible = false;
        for (name, on) in [
            ("HoldingOn", true),
            ("HoldingOff", false),
            ("RollOn", true),
            ("RollOff", false),
        ] {
            hold.set_command(name, vec![visible(on)]);
        }
        rows.push(hold);
        // Each part played its `Init` as it loaded.
        Actor::new(Kind::Frame(rows))
    }

    /// Charges `work` against the skin's [`Budget`].
    fn charge(&mut self, work: u64) -> Result<(), String> {
        let mut b = self.skin.legacy_budget.get();
        if b.work < work {
            b.work = 0;
            self.skin.legacy_budget.set(b);
            return Err("the skin's files take too long to build".into());
        }
        b.work -= work;
        self.skin.legacy_budget.set(b);
        Ok(())
    }

    /// Counts an actor of `attributes` attributes against [`MAX_ACTORS`]
    /// and the skin's [`Budget`].
    fn spend(&mut self, attributes: usize) -> Result<(), String> {
        self.actors += 1;
        if self.actors > MAX_ACTORS {
            return Err(format!("more than {MAX_ACTORS} actors"));
        }
        let mut b = self.skin.legacy_budget.get();
        if b.actors == 0 {
            return Err(format!(
                "the skin's files make more than {} actors",
                SKIN_BUDGET.actors
            ));
        }
        b.actors -= 1;
        self.skin.legacy_budget.set(b);
        self.charge(1 + attributes as u64)
    }

    /// `ActorUtil::MakeActor` on `file`, `depth` files in.
    fn file_actor(&mut self, file: &FileRef, depth: u32) -> Result<Actor, String> {
        if depth > MAX_FILE_DEPTH {
            return Err(format!("{}: files load each other too deep", file.path));
        }
        match self.skin.classify(file.clone()) {
            // Counted as its nodes are built.
            Source::Legacy(f) => self.legacy_file(&f, depth),
            source => {
                self.spend(0)?;
                match source {
                    Source::Image(f) => image_sprite(self.skin, self.textures, self.images, &f)
                        .ok_or_else(|| format!("{} could not be read", f.path)),
                    Source::BuiltInArt(f) => Ok(builtin_sprite(self.textures, f.name())),
                    Source::Model(f) => {
                        model_actor(self.skin, self.textures, self.images, &f, &f, Some(&f))
                    }
                    Source::Lua(f) | Source::Native(_, f) => Err(format!("{} is Lua", f.path)),
                    Source::Movie(f) => Err(format!("{} is a movie", f.path)),
                    Source::Legacy(f) | Source::Other(f) => {
                        Err(format!("{} cannot be drawn", f.path))
                    }
                }
            }
        }
    }

    /// The node of an actor file of the format: its `[<section>]` (an INI
    /// file) or its root (`.xml`), read once per skin.
    fn file_node(&mut self, file: &FileRef, section: Option<&str>) -> Result<Arc<Node>, String> {
        let key = (file.dir, file.path.clone());
        if let Some(r) = self.skin.legacy_nodes.borrow().get(&key) {
            return r.clone();
        }
        let skin = self.skin;
        let text = skin
            .file_text(file)
            .ok_or_else(|| format!("{} could not be read", file.path))?;
        self.charge(1 + text.len() as u64 / 64)?;
        let node = match section {
            Some(name) => Node::from_ini(text, name)
                .ok_or_else(|| format!("{} has no [{name}]", file.path))
                .and_then(|n| {
                    if n.attrs.len() > MAX_ATTRIBUTES {
                        Err(format!("{}: more than {MAX_ATTRIBUTES} keys", file.path))
                    } else {
                        Ok(n)
                    }
                }),
            None => parse_xml(text).map_err(|e| format!("{}: {e}", file.path)),
        }
        .map(Arc::new);
        let mut nodes = self.skin.legacy_nodes.borrow_mut();
        // Bounded: one entry a file of the skin's.
        nodes.insert(key, node.clone());
        node
    }

    /// An actor file of the format (`.actor`, `.sprite`, `.model`,
    /// `.xml`), or a stand-in's `.actor`.
    pub(crate) fn legacy_file(&mut self, file: &FileRef, depth: u32) -> Result<Actor, String> {
        if let Some(Virtual::Actor {
            button,
            element,
            rotation,
        }) = file.virt
        {
            let target = self
                .skin
                .get_path(button, element)
                .map_err(|e| e.to_string())?;
            let mut a = self.file_actor(&target, depth + 1)?;
            a.base_rotation[2] = f32::from(rotation);
            return Ok(a);
        }
        let path = self.skin.file_path(file);
        let dir = dir_of(&path);
        match file.extension().to_ascii_lowercase().as_str() {
            "xml" => {
                let node = self.file_node(file, None)?;
                self.node_actor(dir, &node, depth)
            }
            "actor" => {
                let node = self.file_node(file, Some("Actor"))?;
                self.node_actor(dir, &node, depth)
            }
            // `Sprite::LoadFromSpriteFile`.
            "sprite" => {
                let node = self.file_node(file, Some("Sprite"))?;
                self.sprite_node(dir, &node)
            }
            // `Model::Load`: the three files, no more of the section.
            "model" => {
                let node = self.file_node(file, Some("Model"))?;
                self.spend(node.attrs.len())?;
                let get = |key: &str| {
                    node.attr(key)
                        .ok_or_else(|| format!("{} is missing [Model] {key}", file.path))
                };
                self.model_files(dir, get("Meshes")?, get("Materials")?, get("Bones")?)
            }
            _ => Err(format!("{} cannot be drawn", file.path)),
        }
    }

    /// `Model::LoadPieces` on files relative to `dir`. A bones file that is
    /// not there leaves the model still: OpenITG stops on it, but the
    /// format's skins point at OpenITG's own (`../default/_down tap
    /// bones.txt`, which has no bones).
    fn model_files(
        &mut self,
        dir: &str,
        meshes: &str,
        materials: &str,
        bones: &str,
    ) -> Result<Actor, String> {
        let meshes = self.skin.resolve_path(&format!("{dir}{meshes}"))?;
        let materials = self.skin.resolve_path(&format!("{dir}{materials}"))?;
        let bones = self.skin.resolve_path(&format!("{dir}{bones}")).ok();
        let bones = bones.filter(|b| self.skin.file_text(b).is_some());
        model_actor(
            self.skin,
            self.textures,
            self.images,
            &meshes,
            &materials,
            bones.as_ref(),
        )
    }

    /// `ActorUtil::LoadFromActorFile` (`src/ActorUtil.cpp:104-264`): the
    /// node's class (`Class`, `Type`, else its name) when it is one we
    /// draw, else the actor its `File` makes, given the node's attributes.
    fn node_actor(&mut self, dir: &str, node: &Node, depth: u32) -> Result<Actor, String> {
        if node.attr("Condition").is_some() {
            return Err(format!("{}: a Condition is Lua", node.name));
        }
        let named = node.attr("Class").or_else(|| node.attr("Type"));
        let class = named.unwrap_or(&node.name);
        if class == "Sprite" {
            return self.sprite_node(dir, node);
        }
        self.spend(node.attrs.len())?;
        let file = node.attr("File");
        if file == Some("") {
            return Err(format!("{class}: a blank File"));
        }
        match class {
            "ActorFrame" => {
                let mut children = Vec::new();
                for child in node.child("children").map_or(&[][..], |c| &c.children) {
                    // An actor that fails is left out, as StepMania does
                    // when its error is ignored.
                    match self.node_actor(dir, child, depth) {
                        Ok(a) => children.push(a),
                        Err(e) => self.note(e),
                    }
                }
                let mut a = Actor::new(Kind::Frame(children));
                self.load_node(&mut a, node);
                Ok(a)
            }
            "Model" => {
                let pieces = ["Meshes", "Materials", "Bones"].map(|k| node.attr(k).unwrap_or(""));
                let mut a = if pieces.iter().all(|p| p.is_empty()) {
                    Actor::new(Kind::Actor)
                } else if pieces.iter().any(|p| p.is_empty()) {
                    return Err("a Model needs Meshes, Materials and Bones".into());
                } else {
                    self.model_files(dir, pieces[0], pieces[1], pieces[2])?
                };
                self.load_node(&mut a, node);
                Ok(a)
            }
            "Quad" | "BitmapText" | "ActorScroller" | "BGAnimation" => {
                Err(format!("{class} is not drawn"))
            }
            _ if node.attr("Text").is_some() && named.is_none() => {
                Err("BitmapText is not drawn".into())
            }
            _ => {
                let file = file.ok_or_else(|| format!("{class}: no File"))?;
                let target = match file.strip_prefix('@') {
                    Some(expr) => {
                        let (button, element) = get_path_call(expr)
                            .ok_or_else(|| format!("{file}: Lua other than NOTESKIN:GetPath"))?;
                        self.skin
                            .get_path(button, element)
                            .map_err(|e| e.to_string())?
                    }
                    None if file.starts_with('/') => self.skin.resolve_path(file)?,
                    None => self.skin.resolve_path(&format!("{dir}{file}"))?,
                };
                let mut a = self.file_actor(&target, depth + 1)?;
                self.load_node(&mut a, node);
                Ok(a)
            }
        }
    }

    /// `Sprite::LoadFromNode`: its `Texture` (relative to `dir`), the
    /// `FrameNNNN`/`DelayNNNN` states replacing the texture's (up to the
    /// first missing one), then the node's attributes.
    fn sprite_node(&mut self, dir: &str, node: &Node) -> Result<Actor, String> {
        self.spend(node.attrs.len())?;
        let Some(texture) = node.attr("Texture") else {
            let mut a = Actor::new(Kind::Actor);
            self.load_node(&mut a, node);
            return Ok(a);
        };
        let file = match self.skin.resolve_path(&format!("{dir}{texture}")) {
            Ok(f) => f,
            // Ignored, StepMania leaves the sprite without a texture.
            Err(e) => {
                self.note(format!("a sprite {e}"));
                return Ok(Actor::new(Kind::Actor));
            }
        };
        let mut a = match self.skin.classify(file) {
            Source::Image(f) => image_sprite(self.skin, self.textures, self.images, &f)
                .ok_or_else(|| format!("{} could not be read", f.path))?,
            Source::BuiltInArt(f) => builtin_sprite(self.textures, f.name()),
            other => return Err(format!("{} is not an image", other.file().path)),
        };
        if let Kind::Sprite(s) = &mut a.kind {
            // The first of each key, in one pass (`MAX_ATTRIBUTES` keys).
            let mut frames: Vec<Option<&str>> = Vec::new();
            let mut delays: Vec<Option<&str>> = Vec::new();
            for (key, value) in &node.attrs {
                for (prefix, list) in [("Frame", &mut frames), ("Delay", &mut delays)] {
                    if let Some(i) = state_index(key, prefix) {
                        if list.len() <= i {
                            list.resize(i + 1, None);
                        }
                        list[i].get_or_insert(value.as_str());
                    }
                }
            }
            let mut states = Vec::new();
            for (i, frame) in frames.iter().enumerate() {
                let Some(frame) = frame else { break };
                let frame = string_to_int(frame);
                let Some(&rect) = usize::try_from(frame).ok().and_then(|f| s.frames.get(f)) else {
                    return Err(format!(
                        "Frame{i:04} is {frame}, but {texture} has {} frames",
                        s.frames.len()
                    ));
                };
                let Some(Some(delay)) = delays.get(i) else {
                    break;
                };
                states.push(State {
                    rect,
                    delay: string_to_float(delay),
                });
            }
            if !states.is_empty() {
                s.states = states;
                s.set_state(0);
            }
        }
        self.load_node(&mut a, node);
        Ok(a)
    }

    /// `Actor::LoadFromNode` (`src/Actor.cpp:176-217`): the name, base
    /// rotations and zooms, every `…Command` (`Command` alone is `On`), and
    /// `Init` played.
    fn load_node(&mut self, a: &mut Actor, node: &Node) {
        if let Some(name) = node.attr("Name") {
            a.name = name.to_string();
        }
        for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
            if let Some(v) = node.attr(&format!("BaseRotation{axis}Degrees")) {
                a.base_rotation[i] = string_to_float(v);
            }
            if let Some(v) = node.attr(&format!("BaseZoom{axis}")) {
                a.base_scale[i] = string_to_float(v);
            }
        }
        for (key, value) in &node.attrs {
            let Some(at) = key.len().checked_sub(7) else {
                continue;
            };
            if !key
                .get(at..)
                .is_some_and(|k| k.eq_ignore_ascii_case("Command"))
            {
                continue;
            }
            let name = if at == 0 { "On" } else { &key[..at] };
            match parse_legacy(value) {
                Ok(calls) => a.set_command(name, calls),
                Err(e) => self.note(format!("{key}: {e}")),
            }
        }
        let mut p = Problems::default();
        a.play_own("Init", &mut p);
        for m in p.messages {
            self.note(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Blend;
    use crate::build::{Images, Textures, build, build_colored};
    use crate::skin::tests::folder as folder_of;
    use crate::skin::{Dir, Skin};

    struct Sizes(Vec<(&'static str, (u32, u32))>);

    impl Images for Sizes {
        fn size(&self, _dir: Dir, path: &str) -> Option<(u32, u32)> {
            self.0.iter().find(|(p, _)| *p == path).map(|(_, s)| *s)
        }

        fn max_size(&self) -> u32 {
            8192
        }
    }

    /// A one-triangle model with no material (synthetic).
    const TRIANGLE: &str = "Frames: 1\nFrame: 1\nMeshes: 1\n\"m\" 0 -1\n3\n\
        0 0 0 0 0 0 -1\n0 1 0 0 0 0 -1\n0 0 1 0 0 0 -1\n0\n1\n0 0 1 2 0 0 0 1\n\
        Materials: 0\n";

    /// A skin in the old format (synthetic): Down's notes only, per note
    /// type, an XML receptor and one explosion image.
    fn old_skin() -> Vec<Folder> {
        vec![folder_of(
            "dance",
            "old",
            &[
                (
                    "metrics.ini",
                    "[NoteDisplay]\nTapNoteAnimationIsNoteColor=1\nTapNoteAnimationLengthInBeats=2\n\
                     [ReceptorArrow]\nMarvelousCommand=zoom,0.5\n\
                     [GhostArrowDim]\nMarvelousCommand=diffusealpha,1\nHitMineCommand=blend,add;diffusealpha,1\n\
                     [HoldGhostArrow]\nOnCommand=glowshift\n",
                ),
                (
                    "Down Tap Note 4th.sprite",
                    "[Sprite]\nTexture=_arrows 4x1.png\nFrame0000=2\nDelay0000=0.5\n\
                     InitCommand=Blend,add;EffectClock,beat\n",
                ),
                (
                    "Down Tap Note 12th.sprite",
                    "[Sprite]\nTexture=_arrows 4x1.png\nFrame0000=1\nDelay0000=1\n",
                ),
                (
                    "Down Tap Note 8th.model",
                    "[Model]\nMeshes=_tri.txt\nMaterials=_tri.txt\nBones=../default/_down tap bones.txt\n",
                ),
                ("_tri.txt", TRIANGLE),
                ("_arrows 4x1.png", ""),
                (
                    "Down Receptor Go.xml",
                    "<!-- \"a comment\" -->\n<ActorFrame><children>\n\
                     <Sprite Texture=\"_arrows\" Frame0000=\"0\" Delay0000=\"1\" Frame0000=\"3\"\n\
                     InitCommand=\"diffusealpha,0.5\" PressCommand=\"zoom,0.9\" />\n\
                     </children></ActorFrame>\n",
                ),
                ("Down Tap Explosion Dim Marvelous.png", ""),
                ("Down Hold Explosion.sprite", "[Sprite]\nTexture=_arrows\n"),
            ],
        )]
    }

    fn images() -> Sizes {
        Sizes(vec![
            ("_arrows 4x1.png", (256, 64)),
            ("Down Tap Explosion Dim Marvelous.png", (64, 64)),
        ])
    }

    #[test]
    fn notes_come_per_note_type_and_other_buttons_turn_downs() {
        let folders = old_skin();
        let skin = Skin::load(&folders, "dance", "old").unwrap();
        assert!(skin.legacy);
        let mut t = Textures::default();
        let images = images();
        // Left's 4th: the stand-in's `.actor` on Down's, turned.
        let e = build_colored(&skin, Button::Left, "Tap Note", "4th", &mut t, &images);
        assert!(e.notes.is_empty(), "{:?}", e.notes);
        let Built::Actor(a) = e.built else {
            panic!("{e:?}")
        };
        assert_eq!(a.base_rotation[2], 90.0);
        // `Blend,add`: a name OpenITG passes as a string.
        assert_eq!(a.blend, Blend::Add);
        let Kind::Sprite(s) = &a.kind else { panic!() };
        assert_eq!(s.states.len(), 1);
        assert_eq!(s.states[0].rect, [0.5, 0.0, 0.75, 1.0]);
        assert_eq!(s.states[0].delay, 0.5);
        // A 48th is the 12th's: Left's through `GetPath`; Down's redirect
        // names a model (`Down Tap Note 12th.model`), which this skin's
        // 12th is not, so it fails as in OpenITG.
        let e = build_colored(&skin, Button::Left, "Tap Note", "48th", &mut t, &images);
        let Built::Actor(a) = e.built else { panic!() };
        let Kind::Sprite(s) = &a.kind else { panic!() };
        assert_eq!(s.states[0].rect, [0.25, 0.0, 0.5, 1.0]);
        let e = build_colored(&skin, Button::Down, "Tap Note", "48th", &mut t, &images);
        assert_eq!(e.built, Built::BuiltIn);
        // An 8th is a model whose bones are OpenITG's own: it does not move.
        let e = build_colored(&skin, Button::Up, "Tap Fake", "8th", &mut t, &images);
        let Built::Actor(a) = e.built else {
            panic!("{e:?}")
        };
        assert_eq!(a.base_rotation[2], 180.0);
        let Kind::Model(m) = &a.kind else { panic!() };
        assert!(m.animation.bones.is_empty());
        // A 192nd is drawn as the 64th, which the skin lacks: built in.
        let e = build_colored(&skin, Button::Down, "Tap Note", "192nd", &mut t, &images);
        assert_eq!(e.built, Built::BuiltIn);
        assert!(e.notes[0].contains("Tap Note 64th"), "{:?}", e.notes);
        // The format has no lifts.
        assert_eq!(
            build(&skin, Button::Down, "Tap Lift", &mut t, &images).built,
            Built::BuiltIn
        );
    }

    #[test]
    fn the_receptor_presses_and_takes_the_step_scores_command() {
        let folders = old_skin();
        let skin = Skin::load(&folders, "dance", "old").unwrap();
        let mut t = Textures::default();
        let e = build(&skin, Button::Down, "Receptor", &mut t, &images());
        assert!(e.notes.is_empty(), "{:?}", e.notes);
        let Built::Actor(mut r) = e.built else {
            panic!("{e:?}")
        };
        // The press block (the stand-in's `_blank`), then the receptor.
        assert_eq!(r.children().len(), 2);
        assert!(!r.children()[0].visible);
        let go = &r.children()[1];
        let Kind::Sprite(s) = &go.children()[0].kind else {
            panic!()
        };
        // The first of a repeated attribute counts.
        assert_eq!(s.states.len(), 1);
        assert_eq!(s.states[0].rect, [0.0, 0.0, 0.25, 1.0]);
        assert_eq!(go.children()[0].current().diffuse[3], 0.5);
        let mut p = Problems::default();
        for c in ["Press", "W1"] {
            r.play(c, &mut p);
        }
        assert!(p.messages.is_empty(), "{:?}", p.messages);
        assert!(r.children()[0].visible);
        assert_eq!(r.children()[1].current().scale[0], 0.5);
        assert_eq!(r.children()[1].children()[0].current().scale[0], 0.9);
        r.play("Lift", &mut p);
        assert!(!r.children()[0].visible);
    }

    #[test]
    fn an_explosion_shows_the_score_of_its_row_and_keeps_a_mines() {
        let folders = old_skin();
        let skin = Skin::load(&folders, "dance", "old").unwrap();
        let mut t = Textures::default();
        let e = build(&skin, Button::Down, "Explosion", &mut t, &images());
        assert!(e.notes.is_empty(), "{:?}", e.notes);
        let Built::Actor(mut x) = e.built else {
            panic!("{e:?}")
        };
        let mut p = Problems::default();
        let (marvelous, hit_mine) = (8, 1);
        let shown = |x: &Actor, row: usize, i: usize| x.children()[row].children()[i].visible;
        assert!(!shown(&x, 0, marvelous));
        x.play("DimW1", &mut p);
        assert!(shown(&x, 0, marvelous) && !shown(&x, 1, marvelous));
        assert_eq!(
            x.children()[0].children()[marvelous].current().diffuse[3],
            1.0
        );
        x.play("DimHitMine", &mut p);
        assert!(shown(&x, 0, hit_mine) && !shown(&x, 0, marvelous));
        assert_eq!(x.children()[0].children()[hit_mine].blend, Blend::Add);
        x.play("DimW1", &mut p);
        assert!(shown(&x, 0, hit_mine) && shown(&x, 0, marvelous));
        // The hold explosion, with its OnCommand run.
        let hold = &x.children()[2];
        assert!(!hold.visible);
        assert_eq!(hold.effect(), crate::actor::Effect::GlowShift);
        x.play("HoldingOn", &mut p);
        assert!(x.children()[2].visible);
        assert!(p.messages.is_empty(), "{:?}", p.messages);
    }

    #[test]
    fn files_that_load_each_other_stop() {
        let many = format!(
            "<ActorFrame><children>{}</children></ActorFrame>",
            "<Actor File=\"Down Receptor Go.xml\"/>".repeat(64)
        );
        let folders = [folder_of(
            "dance",
            "loop",
            &[
                ("metrics.ini", ""),
                ("Down Tap Note.actor", "[Actor]\nFile=Down Tap Note.actor\n"),
                ("Down Receptor Go.xml", &many),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "loop").unwrap();
        let mut t = Textures::default();
        let e = build(&skin, Button::Down, "Tap Note", &mut t, &images());
        assert_eq!(e.built, Built::BuiltIn);
        assert!(e.notes[0].contains("too deep"), "{:?}", e.notes);
        let e = build(&skin, Button::Down, "Receptor", &mut t, &images());
        assert!(
            e.notes.iter().any(|n| n.contains("more than 4096 actors")),
            "{:?}",
            e.notes
        );
    }

    #[test]
    fn the_defaults_pulse_receptors_and_fade_a_held_flash() {
        let folders = [folder_of(
            "dance",
            "bare",
            &[
                (
                    "metrics.ini",
                    "[NoteDisplay]\nTapNoteAnimationLengthInBeats=1\n",
                ),
                (
                    "Down Receptor Go.sprite",
                    "[Sprite]\nTexture=_arrows 4x1.png\nInitCommand=linear,1;zoom,2\n",
                ),
                ("Down KeypressBlock.sprite", "no section\n"),
                ("_arrows 4x1.png", ""),
                (
                    "Down Tap Note.sprite",
                    "[Sprite]\nTexture=_arrows 4x1.png\n",
                ),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "bare").unwrap();
        let mut t = Textures::default();
        let mut p = Problems::default();
        let clocks = crate::actor::Clocks::default();
        let Built::Actor(mut r) = build(&skin, Button::Down, "Receptor", &mut t, &images()).built
        else {
            panic!()
        };
        // A key-press block that cannot load shows nothing.
        assert_eq!(r.children()[0].kind, Kind::Actor);
        // `Init` ran once: its one-second tween is over after 1.5 s.
        r.update(1.5, &clocks, &mut p);
        assert!(!r.children()[1].tweening());
        assert_eq!(r.children()[1].current().scale[0], 2.0);
        // Marvelous pulses (StepMania 3.9's empty command is not used).
        r.play("W1", &mut p);
        assert_eq!(r.children()[1].current().scale[0], 0.75);
        // A held hold's flash fades.
        let Built::Actor(mut x) = build(&skin, Button::Down, "Explosion", &mut t, &images()).built
        else {
            panic!()
        };
        let ok = 9 + 2;
        x.play("DimHeld", &mut p);
        assert!(x.children()[0].children()[ok].visible);
        for _ in 0..20 {
            x.update(0.05, &clocks, &mut p);
        }
        assert_eq!(x.children()[0].children()[ok].current().diffuse[3], 0.0);
        assert!(p.messages.is_empty(), "{:?}", p.messages);
    }

    #[test]
    fn stepmania_5_skins_are_not_in_the_chain() {
        let folders = [
            folder_of(
                "dance",
                "old",
                &[("metrics.ini", ""), ("Down Tap Note.sprite", "")],
            ),
            folder_of(
                "dance",
                "default",
                &[("NoteSkin.lua", ""), ("metrics.ini", "")],
            ),
            folder_of(
                "dance",
                "new",
                &[
                    ("NoteSkin.lua", ""),
                    ("metrics.ini", "[Global]\nFallbackNoteSkin=old\n"),
                ],
            ),
        ];
        let skin = Skin::load(&folders, "dance", "old").unwrap();
        assert!(skin.legacy);
        assert_eq!(
            skin.chain,
            [
                Dir::Folder(0),
                Dir::BuiltIn(crate::builtin::BuiltIn::LegacyDefault),
                Dir::BuiltIn(crate::builtin::BuiltIn::LegacyCommon)
            ]
        );
        // A StepMania 5 skin on an old one stays a StepMania 5 skin.
        assert!(!is_legacy_chain(&folders, "dance", "new"));
    }

    #[test]
    fn hostile_files_stop_within_the_skins_budget() {
        let junk: String = (0..MAX_ATTRIBUTES - 8)
            .map(|i| format!("Junk{i}=1\n"))
            .collect();
        let big = format!("[Sprite]\nTexture=_arrows 4x1.png\n{junk}");
        let fan = format!(
            "<ActorFrame><children>{}</children></ActorFrame>",
            "<Actor File=\"big.sprite\"/>".repeat(4000)
        );
        let folders = [folder_of(
            "dance",
            "fan",
            &[
                (
                    "metrics.ini",
                    "[NoteDisplay]\nTapNoteAnimationIsNoteColor=1\n",
                ),
                ("big.sprite", &big),
                ("_arrows 4x1.png", ""),
                ("fan.xml", &fan),
                ("Down Tap Note 4th.redir", "fan.xml"),
                ("Down Tap Note 8th.redir", "fan.xml"),
                ("Down Tap Note 12th.redir", "fan.xml"),
                ("Down Tap Note 16th.redir", "fan.xml"),
                ("Down Tap Note 24th.redir", "fan.xml"),
                ("Down Tap Note 32nd.redir", "fan.xml"),
                ("Down Tap Note 48th.redir", "fan.xml"),
                ("Down Tap Note 64th.redir", "fan.xml"),
                ("Down Receptor Go.sprite", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "fan").unwrap();
        let start = std::time::Instant::now();
        let mut t = Textures::default();
        let mut notes = Vec::new();
        for b in &Button::ALL[..4] {
            for color in LEGACY_NOTE_TYPES {
                notes.extend(build_colored(&skin, *b, "Tap Note", color, &mut t, &images()).notes);
            }
        }
        assert!(start.elapsed().as_secs_f32() < 5.0, "{:?}", start.elapsed());
        assert!(
            notes.iter().any(|n| n.contains("take too long")),
            "{notes:?}"
        );
        // An element of too many attributes is refused.
        assert!(
            parse_xml(&format!(
                "<a {}/>",
                (0..=MAX_ATTRIBUTES)
                    .map(|i| format!("k{i}=\"\""))
                    .collect::<Vec<_>>()
                    .join(" ")
            ))
            .is_err()
        );
    }

    #[test]
    fn get_path_is_the_only_lua_read() {
        assert_eq!(
            get_path_call("NOTESKIN:GetPath('Down','Tap Note 4th')"),
            Some(("Down", "Tap Note 4th"))
        );
        assert_eq!(
            get_path_call(" NOTESKIN:GetPath ( \"Up\" , \"Receptor Go\" ) "),
            Some(("Up", "Receptor Go"))
        );
        assert_eq!(get_path_call("THEME:GetPath('G','x')"), None);
        assert_eq!(get_path_call("NOTESKIN:GetPath('Down')"), None);
    }

    fn folder(name: &str, files: &[&str], metrics: &str) -> Folder {
        // Only `metrics.ini` kept as text.
        Folder {
            game: "dance".into(),
            name: name.into(),
            files: files.iter().map(|f| f.to_string()).collect(),
            texts: vec![("metrics.ini".into(), metrics.into())],
        }
    }

    #[test]
    fn old_skins_and_the_skins_that_fall_back_on_them() {
        let old = folder(
            "old",
            &["Down Tap Note 4th.model", "metrics.ini"],
            "[NoteDisplay]\n",
        );
        let variant = folder(
            "variant",
            &["metrics.ini", "textures/Fill.png"],
            "[Global]\nFallbackNoteSkin=old\n",
        );
        let new = folder(
            "new",
            &["NoteSkin.lua", "Down Tap Note.lua", "metrics.ini"],
            "",
        );
        let folders = [old, variant, new];
        assert!(is_legacy(&folders[0]));
        assert!(!is_legacy(&folders[1]) && !is_legacy(&folders[2]));
        assert!(is_legacy_chain(&folders, "dance", "variant"));
        assert!(!is_legacy_chain(&folders, "dance", "new"));
        assert!(!is_legacy_chain(&folders, "dance", "nowhere"));
    }

    #[test]
    fn actor_files_read_into_nodes() {
        let sprite = Node::from_ini(
            "[Sprite]\nTexture=Down Tap Explosion Dim Great.png\nBaseRotationZDegrees=90\n",
            "Sprite",
        )
        .unwrap();
        assert_eq!(
            sprite.attr("Texture"),
            Some("Down Tap Explosion Dim Great.png")
        );
        assert_eq!(sprite.attr("BaseRotationZDegrees"), Some("90"));
        assert_eq!(Node::from_ini("[Other]\nA=1\n", "Sprite"), None);

        let xml = parse_xml(
            "<?xml version=\"1.0\"?>\n<!-- \"effectoffset,0.05\" -->\n<ActorFrame><children>\n\
             \t<Sprite Texture=\"_Down Receptor tex\" Frame0000=\"0\" Delay0000='0.95'\n\
             \t\tInitCommand=\"EffectClock,beat;diffuse,1,1,1,1\" PressCommand=\"zoom,0.9 &amp; more\" />\n\
             </children></ActorFrame>\n",
        )
        .unwrap();
        assert_eq!(xml.name, "ActorFrame");
        let children = xml.child("children").unwrap();
        let s = &children.children[0];
        assert_eq!(s.name, "Sprite");
        assert_eq!(s.attr("Delay0000"), Some("0.95"));
        assert_eq!(s.attr("PressCommand"), Some("zoom,0.9 & more"));
    }

    #[test]
    fn broken_and_hostile_xml_is_an_error() {
        for bad in [
            "",
            "<a>",
            "<a></b>",
            "<a b=c/>",
            "<a b=\"c/>",
            "<a/><b/>",
            "<!-- open",
        ] {
            assert!(parse_xml(bad).is_err(), "{bad:?}");
        }
        let deep = "<a>".repeat(100) + &"</a>".repeat(100);
        assert!(parse_xml(&deep).is_err());
        let many = format!("<a>{}</a>", "<b/>".repeat(10_000));
        assert!(parse_xml(&many).is_err());
        assert_eq!(unescape("&#65;&#x42;&bogus;&"), "AB&bogus;&");
    }
}
