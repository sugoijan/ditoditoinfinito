//! MilkShape 3D ASCII models, as StepMania reads them for noteskins
//! (`docs/plans/models.md`): meshes (`RageModelGeometry::LoadMilkshapeAscii`,
//! `src/RageModelGeometry.cpp:96-286`), materials
//! (`Model::LoadMilkshapeAscii`, `src/Model.cpp:126-275`), bones and their key
//! frames (`msAnimation::LoadMilkshapeAsciiBones`, `src/ModelTypes.cpp:236-369`),
//! and the `[AnimatedTexture]` files materials may name
//! (`AnimatedTexture::Load`, `src/ModelTypes.cpp:59-130`); 5_1-new @ 825467b.
//!
//! A file holds any of the three blocks (`Meshes:`, `Materials:`, `Bones:`),
//! each found by its header line wherever it is; outside a block, lines
//! starting with `//` and lines that are not a header are skipped, inside one
//! every line is data. Models are untrusted: counts are checked against the
//! lines left before anything is allocated, and the total of vertices is
//! StepMania's (16-bit indices). What they fill is `ddi_actor::model`'s.

use std::fmt;

use ddi_actor::commands::scan_float;
use ddi_actor::model::{AnimatedTexture, Animation, Bone, Geometry, Material, Mesh, Vertex};

use crate::ini::Ini;

/// The most vertices a model may have, over its meshes (StepMania indexes
/// them with 16 bits).
pub const MAX_VERTICES: usize = 65_535;

/// The most bones a model may have (StepMania keeps a vertex's bone in a
/// signed byte: no vertex can follow more than 128 of them).
pub const MAX_BONES: usize = 1024;

/// The most triangles a model may have, over its meshes (every model of
/// the local corpora has fewer than 2,000): each is drawn per note.
pub const MAX_TRIANGLES: usize = 16_384;

/// Items reserved for ahead of a count (a count is checked against the
/// lines left, not against the lines each item takes).
const RESERVE: usize = 256;

/// Why a model did not load.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelError {
    /// The 1-based line, 0 for the file as a whole.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

fn error(line: usize, message: impl Into<String>) -> ModelError {
    ModelError {
        line,
        message: message.into(),
    }
}

/// The file's lines as `RageFile::GetLine` gives them (`\r\n` or `\n`).
struct Lines<'a> {
    lines: Vec<&'a str>,
    at: usize,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Lines<'a> {
        Lines {
            lines: text
                .split('\n')
                .map(|l| l.strip_suffix('\r').unwrap_or(l))
                .collect(),
            at: 0,
        }
    }

    /// The 1-based number of the line [`Lines::next`] returned last.
    fn number(&self) -> usize {
        self.at
    }

    fn left(&self) -> usize {
        self.lines.len() - self.at
    }

    fn next(&mut self) -> Option<&'a str> {
        let l = self.lines.get(self.at).copied();
        self.at += 1;
        l
    }

    /// A data line inside a block.
    fn data(&mut self) -> Result<&'a str, ModelError> {
        let l = self.next();
        l.ok_or_else(|| error(self.number(), "the file ends inside a block"))
    }

    /// The count line of a list, checked against the lines left (each item
    /// takes one line at least).
    fn count(&mut self, what: &str) -> Result<usize, ModelError> {
        let line = self.data()?;
        let n =
            leading_int(line).ok_or_else(|| error(self.number(), format!("no {what} count")))?;
        let n = usize::try_from(n)
            .map_err(|_| error(self.number(), format!("a negative {what} count")))?;
        if n > self.left() {
            return Err(error(
                self.number(),
                format!("{n} {what} but the file ends first"),
            ));
        }
        Ok(n)
    }

    /// Skips to the line after the header `prefix` (`"Meshes: %d"`) and
    /// returns its count; `None` when the file has none.
    fn header(&mut self, prefix: &str) -> Result<Option<usize>, ModelError> {
        while let Some(line) = self.next() {
            if line.starts_with("//") {
                continue;
            }
            if let Some(rest) = line.strip_prefix(prefix)
                && let Some(n) = leading_int(rest)
            {
                return usize::try_from(n)
                    .map(Some)
                    .map_err(|_| error(self.number(), format!("a negative count after {prefix}")));
            }
        }
        Ok(None)
    }
}

/// C's `%d` at the start of `s` (after whitespace).
fn leading_int(s: &str) -> Option<i64> {
    let s = s.trim_start();
    let end = s
        .char_indices()
        .take_while(|&(i, c)| c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')))
        .count();
    s[..end].parse().ok()
}

/// C's `%f` on each whitespace-separated field of `s`: `n` numbers, the rest
/// of the line ignored.
fn floats<const N: usize>(s: &str) -> Option<[f32; N]> {
    let mut out = [0.0; N];
    let mut fields = s.split_whitespace();
    for v in &mut out {
        *v = strtof(fields.next()?)?;
    }
    Some(out)
}

/// The number at the start of `f`, as `strtof` takes it: a sign, digits
/// with a point, an exponent if digits follow it; or `inf`, `infinity`,
/// `nan`.
fn strtof(f: &str) -> Option<f32> {
    if let Some((v, _)) = scan_float(f) {
        return Some(v);
    }
    // No mantissa digits: past the sign and a point, a word.
    let b = f.as_bytes();
    let sign = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let i = sign + usize::from(b.get(sign) == Some(&b'.'));
    let word = b.get(i..i + 3);
    let sign = if b.first() == Some(&b'-') { -1.0 } else { 1.0 };
    if word.is_some_and(|w| w.eq_ignore_ascii_case(b"inf")) {
        Some(sign * f32::INFINITY)
    } else if word.is_some_and(|w| w.eq_ignore_ascii_case(b"nan")) {
        Some(f32::NAN)
    } else {
        None
    }
}

fn ints<const N: usize>(s: &str) -> Option<[i64; N]> {
    let mut out = [0; N];
    let mut fields = s.split_whitespace();
    for v in &mut out {
        *v = leading_int(fields.next()?)?;
    }
    Some(out)
}

/// `"text"` at the start of the line, up to `max` characters; `None` when the
/// line does not start with a quote or the text is longer (`%31[^"]`).
fn quoted(line: &str, max: usize) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('"')?;
    let end = rest.find('"').unwrap_or(rest.len());
    let text = &rest[..end];
    if text.chars().count() > max {
        return None;
    }
    Some((text, rest.get(end + 1..).unwrap_or("")))
}

/// The meshes of a model file (`Meshes:`); no block: none.
pub fn meshes(text: &str) -> Result<Geometry, ModelError> {
    let mut lines = Lines::new(text);
    let Some(count) = lines.header("Meshes: ")? else {
        return Ok(Geometry::default());
    };
    if count > lines.left() {
        return Err(error(lines.number(), "more meshes than lines"));
    }
    let mut meshes = Vec::with_capacity(count.min(RESERVE));
    let mut total = 0usize;
    // Triangles drawn, the merge below counted.
    let mut drawn = 0usize;
    for _ in 0..count {
        let line = lines.data()?;
        let (name, rest) = quoted(line, 31)
            .filter(|(n, _)| !n.is_empty())
            .ok_or_else(|| {
                error(
                    lines.number(),
                    "a mesh without a name (or one over 31 characters)",
                )
            })?;
        let [_flags, material] =
            ints::<2>(rest).ok_or_else(|| error(lines.number(), "a mesh without its material"))?;
        // A byte, then signed: -1 (255) is none.
        let material = match (material as u8) as i8 {
            m if m < 0 => None,
            m => Some(m as usize),
        };
        let n = lines.count("vertices")?;
        total += n;
        if total > MAX_VERTICES {
            return Err(error(
                lines.number(),
                format!("more than {MAX_VERTICES} vertices"),
            ));
        }
        let mut vertices = Vec::with_capacity(n);
        for _ in 0..n {
            let line = lines.data()?;
            let flags = leading_int(line)
                .ok_or_else(|| error(lines.number(), "a vertex without its flags"))?;
            let rest = line
                .trim_start()
                .split_once(char::is_whitespace)
                .map_or("", |(_, r)| r);
            let [x, y, z, u, v] =
                floats::<5>(rest).ok_or_else(|| error(lines.number(), "a malformed vertex"))?;
            let bone = rest
                .split_whitespace()
                .nth(5)
                .and_then(leading_int)
                .unwrap_or(-1);
            vertices.push(Vertex {
                pos: [x, y, z],
                uv: [u, v],
                bone: usize::try_from(bone).ok(),
                fixed: [flags & 1 != 0, flags & 2 != 0],
            });
        }
        // Normals: read past (lighting is off for noteskins).
        let n = lines.count("normals")?;
        for _ in 0..n {
            let line = lines.data()?;
            floats::<3>(line).ok_or_else(|| error(lines.number(), "a malformed normal"))?;
        }
        let n = lines.count("triangles")?;
        drawn += n;
        if drawn > MAX_TRIANGLES {
            return Err(error(
                lines.number(),
                format!("more than {MAX_TRIANGLES} triangles"),
            ));
        }
        let mut triangles = Vec::with_capacity(n);
        for _ in 0..n {
            let line = lines.data()?;
            let [_flags, a, b, c, ..] =
                ints::<8>(line).ok_or_else(|| error(lines.number(), "a malformed triangle"))?;
            let index = |i: i64| {
                u16::try_from(i)
                    .ok()
                    .filter(|&i| usize::from(i) < vertices.len())
                    .ok_or_else(|| error(lines.number(), "a triangle's vertex out of range"))
            };
            triangles.push([index(a)?, index(b)?, index(c)?]);
        }
        let mut mesh = Mesh {
            name: name.to_string(),
            material,
            vertices,
            triangles,
            bone: None,
        };
        optimize_bones(&mut mesh);
        meshes.push(mesh);
    }
    // `LoadMilkshapeAscii`: two meshes of one name are drawn as the first
    // with the second's triangles added (on its material, under its bone),
    // and the second still on its own.
    if let [first, second] = meshes.as_mut_slice()
        && first.name == second.name
    {
        if drawn + second.triangles.len() > MAX_TRIANGLES {
            return Err(ModelError {
                line: 0,
                message: format!("more than {MAX_TRIANGLES} triangles"),
            });
        }
        let shift = first.vertices.len();
        first.vertices.extend_from_slice(&second.vertices);
        // Within MAX_VERTICES over both, so within 16 bits.
        first.triangles.extend(
            second
                .triangles
                .iter()
                .map(|t| t.map(|i| (usize::from(i) + shift) as u16)),
        );
    }
    Ok(Geometry { meshes })
}

/// `OptimizeBones`: a mesh whose vertices all follow one bone follows it as
/// a whole.
fn optimize_bones(mesh: &mut Mesh) {
    let Some(first) = mesh.vertices.first().map(|v| v.bone) else {
        return;
    };
    if first.is_some() && mesh.vertices.iter().all(|v| v.bone == first) {
        mesh.bone = first;
        for v in &mut mesh.vertices {
            v.bone = None;
        }
    }
}

/// The materials of a model file (`Materials:`); no block: none.
pub fn materials(text: &str) -> Result<Vec<Material>, ModelError> {
    let mut lines = Lines::new(text);
    let Some(count) = lines.header("Materials: ")? else {
        return Ok(Vec::new());
    };
    if count > lines.left() {
        return Err(error(lines.number(), "more materials than lines"));
    }
    let mut out = Vec::with_capacity(count.min(RESERVE));
    for _ in 0..count {
        let line = lines.data()?;
        let (name, _) =
            quoted(line, 255).ok_or_else(|| error(lines.number(), "a material without a name"))?;
        let mut color = |what: &str| -> Result<[f32; 4], ModelError> {
            let line = lines.data()?;
            floats::<4>(line).ok_or_else(|| {
                error(
                    lines.number(),
                    format!("a material's {what} is not four numbers"),
                )
            })
        };
        let ambient = color("ambient")?;
        let diffuse = color("diffuse")?;
        let specular = color("specular")?;
        let emissive = color("emissive")?;
        let mut number = |what: &str| -> Result<f32, ModelError> {
            let line = lines.data()?;
            floats::<1>(line).map(|[v]| v).ok_or_else(|| {
                error(
                    lines.number(),
                    format!("a material's {what} is not a number"),
                )
            })
        };
        let shininess = number("shininess")?;
        let transparency = number("transparency")?;
        // A texture line that does not parse is no texture.
        let mut texture = || -> Result<String, ModelError> {
            let line = lines.data()?;
            Ok(quoted(line, 255).map_or(String::new(), |(t, _)| clean_path(t)))
        };
        let texture_path = texture()?;
        let alpha_texture = texture()?;
        out.push(Material {
            name: name.to_string(),
            ambient,
            diffuse,
            specular,
            emissive,
            shininess,
            transparency,
            texture: texture_path,
            alpha_texture,
        });
    }
    Ok(out)
}

/// A material's texture path as StepMania reads it: `\` as `/`, `./`
/// dropped, `..` collapsed where it can be.
fn clean_path(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in p.split(['\\', '/']) {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|l| *l != "..") => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// The bones of a model file (`Bones:`) and their key frames; no block: no
/// bones, an animation of no frames.
pub fn bones(text: &str) -> Result<Animation, ModelError> {
    let mut lines = Lines::new(text);
    let Some(count) = lines.header("Bones: ")? else {
        return Ok(Animation::default());
    };
    if count > lines.left() {
        return Err(error(lines.number(), "more bones than lines"));
    }
    if count > MAX_BONES {
        return Err(error(
            lines.number(),
            format!("more than {MAX_BONES} bones"),
        ));
    }
    let mut bones: Vec<Bone> = Vec::with_capacity(count.min(RESERVE));
    let mut parents: Vec<String> = Vec::with_capacity(count.min(RESERVE));
    let mut total = 0.0f32;
    for _ in 0..count {
        let line = lines.data()?;
        let (name, _) =
            quoted(line, 31).ok_or_else(|| error(lines.number(), "a bone without a name"))?;
        let line = lines.data()?;
        let (parent, _) =
            quoted(line, 31).ok_or_else(|| error(lines.number(), "a bone without its parent"))?;
        let line = lines.data()?;
        let rest = line
            .trim_start()
            .split_once(char::is_whitespace)
            .map_or("", |(_, r)| r);
        let [px, py, pz, rx, ry, rz] = floats::<6>(rest)
            .ok_or_else(|| error(lines.number(), "a bone without its position and rotation"))?;
        let mut keys = |what: &str| -> Result<Vec<(f32, [f32; 3])>, ModelError> {
            let n = lines.count(what)?;
            let mut keys = Vec::with_capacity(n);
            for _ in 0..n {
                let line = lines.data()?;
                let [t, x, y, z] = floats::<4>(line)
                    .ok_or_else(|| error(lines.number(), format!("a malformed {what} key")))?;
                total = total.max(t);
                keys.push((t, [x, y, z]));
            }
            Ok(keys)
        };
        let position_keys = keys("position")?;
        let rotation_keys = keys("rotation")?;
        bones.push(Bone {
            name: name.to_string(),
            parent: None,
            position: [px, py, pz],
            rotation: [rx, ry, rz],
            position_keys,
            rotation_keys,
        });
        parents.push(parent.to_string());
    }
    // Parents by name (the first bone of the name); one that is not there
    // is a root. A cycle is harmless: bones are placed in order, each
    // under its parent as it is then (`Animation::rest`, `Animation::pose`).
    let mut by_name = std::collections::HashMap::with_capacity(bones.len());
    for (i, b) in bones.iter().enumerate() {
        by_name.entry(b.name.clone()).or_insert(i);
    }
    for (i, b) in bones.iter_mut().enumerate() {
        let parent = &parents[i];
        b.parent = if parent.is_empty() {
            None
        } else {
            by_name.get(parent.as_str()).copied().filter(|&p| p != i)
        };
    }
    Ok(Animation {
        bones,
        total_frames: if total.is_finite() && total > 0.0 {
            total as u32
        } else {
            0
        },
    })
}

/// The most frames an animated texture has (StepMania's loop bound).
pub const MAX_FRAMES: usize = 1000;

/// An `.ini` texture (`[AnimatedTexture]`): `Frame####` with its
/// `Delay####` until the first missing one.
pub fn animated_texture(text: &str) -> AnimatedTexture {
    let ini = Ini::parse(text);
    let get = |key: &str| ini.get("AnimatedTexture", key);
    let num = |key: &str| get(key).map_or(0.0, crate::ini::string_to_float);
    let mut frames = Vec::new();
    let mut translate = Vec::new();
    for i in 0..MAX_FRAMES {
        let (Some(file), Some(delay)) =
            (get(&format!("Frame{i:04}")), get(&format!("Delay{i:04}")))
        else {
            break;
        };
        frames.push((clean_path(file), crate::ini::string_to_float(delay)));
        translate.push([
            num(&format!("TranslateX{i:04}")),
            num(&format!("TranslateY{i:04}")),
        ]);
    }
    AnimatedTexture {
        frames,
        velocity: [num("TexVelocityX"), num("TexVelocityY")],
        offset: [num("TexOffsetX"), num("TexOffsetY")],
        translate,
    }
}

#[cfg(test)]
mod tests {
    use ddi_actor::Mat4;
    use ddi_actor::model::to_rest;

    use super::*;

    /// A model of one textured square with a bone, the way MilkShape writes
    /// it (synthetic).
    const SQUARE: &str = "// MilkShape 3D ASCII\r\n\
\r\n\
Frames: 30\r\n\
Frame: 1\r\n\
\r\n\
Meshes: 2\r\n\
\"Square\" 0 0\r\n\
4\r\n\
0 -10.0 10.0 0.0 0.0 0.0 0\r\n\
0 10.0 10.0 0.0 1.0 0.0 0\r\n\
1 10.0 -10.0 0.0 1.0 1.0 0\r\n\
2 -10.0 -10.0 0.0 0.0 1.0 0\r\n\
1\r\n\
0.0 0.0 1.0\r\n\
2\r\n\
0 0 1 2 0 0 0 1\r\n\
0 0 2 3 0 0 0 1\r\n\
\"Bare\" 0 -1\r\n\
3\r\n\
0 0 0 0 0 0 -1\r\n\
0 1 0 0 0 0 0\r\n\
0 0 1 0 0 0 -1\r\n\
0\r\n\
1\r\n\
0 0 1 2 0 0 0 1\r\n\
\r\n\
Materials: 1\r\n\
\"Mat\"\r\n\
0.2 0.2 0.2 1.0\r\n\
0.8 0.8 0.8 1.0\r\n\
0.0 0.0 0.0 1.0\r\n\
0.0 0.0 0.0 1.0\r\n\
0.0\r\n\
1.0\r\n\
\".\\\\textures\\\\fill.ini\"\r\n\
\"\"\r\n\
\r\n\
Bones: 2\r\n\
\"joint1\"\r\n\
\"\"\r\n\
8 0.0 0.0 0.0 0.0 0.0 0.0\r\n\
2\r\n\
1.0 0.0 0.0 0.0\r\n\
120.0 0.0 0.0 0.0\r\n\
3\r\n\
1.0 0.0 0.0 0.0\r\n\
60.0 0.0 0.0 3.1\r\n\
120.0 0.0 0.0 6.2\r\n\
\"joint2\"\r\n\
\"joint1\"\r\n\
8 1.0 0.0 0.0 0.0 0.0 0.0\r\n\
0\r\n\
0\r\n";

    #[test]
    fn a_square_with_a_material_and_bones() {
        let g = meshes(SQUARE).unwrap();
        assert_eq!(g.meshes.len(), 2);
        let square = &g.meshes[0];
        assert_eq!(square.name, "Square");
        assert_eq!(square.material, Some(0));
        assert_eq!(square.vertices.len(), 4);
        assert_eq!(square.triangles, [[0, 1, 2], [0, 2, 3]]);
        // Every vertex on bone 0: the mesh follows it.
        assert_eq!(square.bone, Some(0));
        assert!(square.vertices.iter().all(|v| v.bone.is_none()));
        assert_eq!(square.vertices[2].fixed, [true, false]);
        assert_eq!(square.vertices[3].fixed, [false, true]);
        assert_eq!(square.vertices[1].uv, [1.0, 0.0]);
        let bare = &g.meshes[1];
        assert_eq!(bare.material, None);
        assert_eq!(bare.bone, None);
        assert_eq!(bare.vertices[1].bone, Some(0));

        let m = materials(SQUARE).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].texture, "textures/fill.ini");
        assert_eq!(m[0].alpha_texture, "");
        assert_eq!(m[0].unlit_color(), [1.0, 1.0, 1.0, 1.0]);

        let a = bones(SQUARE).unwrap();
        assert_eq!(a.bones.len(), 2);
        assert_eq!(a.total_frames, 120);
        assert_eq!(a.bones[0].rotation_keys[1], (60.0, [0.0, 0.0, 3.1]));
        assert_eq!(a.bones[1].parent, Some(0));
        assert_eq!(a.bones[1].position, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn files_without_a_block_and_malformed_ones() {
        assert_eq!(meshes("nothing here\n").unwrap(), Geometry::default());
        assert!(materials("").unwrap().is_empty());
        assert_eq!(bones("// MilkShape\n").unwrap().total_frames, 0);
        for bad in [
            // A comment inside a block is data, and not a mesh.
            "Meshes: 1\n// comment\n",
            "Meshes: 1\n\"\" 0 0\n0\n0\n0\n",
            "Meshes: 1\n\"a name far longer than thirty one chars\" 0 0\n0\n0\n0\n",
            // Counts beyond the file.
            "Meshes: 1\n\"m\" 0 0\n999999999\n",
            "Meshes: 99999\n",
            "Meshes: 1\n\"m\" 0 0\n1\n0 0 0 0 0 0 -1\n0\n1\n0 0 1 5 0 0 0 1\n",
            "Meshes: 1\n\"m\" 0 0\n1\n0 0 0 nope\n0\n0\n",
        ] {
            assert!(meshes(bad).is_err(), "{bad:?}");
        }
        assert!(materials("Materials: 1\n\"m\"\n1 1 1\n").is_err());
        // Bones naming each other as parents are placed in order, each under
        // the other as it is then.
        let a = bones(
            "Bones: 2\n\"a\"\n\"b\"\n0 1 0 0 0 0 0\n0\n1\n2 0 0 0\n\
             \"b\"\n\"a\"\n0 2 0 0 0 0 0\n0\n1\n2 0 0 0\n",
        )
        .unwrap();
        assert_eq!(a.bones[0].parent, Some(1));
        assert_eq!(a.bones[1].parent, Some(0));
        let rest = a.rest();
        // a's parent comes after it (taken as the identity), b's before.
        assert_eq!(rest[1].absolute[12..15], [3.0, 0.0, 0.0]);
        let mut pose = Vec::new();
        a.pose(&rest, 1.0, &mut pose);
        assert_eq!(pose.len(), 2);
        // Too many bones, before anything is read.
        assert!(
            bones(&format!(
                "Bones: {}\n{}",
                MAX_BONES + 1,
                "x\n".repeat(MAX_BONES * 5 + 5)
            ))
            .is_err()
        );
    }

    #[test]
    fn animated_textures_scroll_and_flip() {
        let t = animated_texture(
            "[AnimatedTexture]\nTexVelocityY=1\nTexOffsetX=0.5\nFrame0000=a.png\nDelay0000=1\nFrame0001=b.png\nDelay0001=1\nTranslateX0001=0.25\nFrame0003=c.png\nDelay0003=1\n",
        );
        // The first missing frame ends the list.
        assert_eq!(t.frames, [("a.png".into(), 1.0), ("b.png".into(), 1.0)]);
        assert_eq!(t.length(), 2.0);
        assert_eq!(t.frame_at(0.5), 0);
        assert_eq!(t.frame_at(1.5), 1);
        assert_eq!(t.frame_at(2.5), 0);
        assert_eq!(t.translation(0.0), [0.5, 0.0]);
        assert_eq!(t.translation(1.0), [0.75, 0.5]);
        let image = AnimatedTexture::image("x.png");
        assert_eq!(image.translation(0.3), [0.0, 0.0]);
    }

    #[test]
    fn paths_and_numbers_as_stepmania_reads_them() {
        assert_eq!(clean_path(".\\a\\..\\b/c.png"), "b/c.png");
        assert_eq!(clean_path("../x.png"), "../x.png");
        assert_eq!(leading_int(" 12abc"), Some(12));
        assert_eq!(leading_int("-1"), Some(-1));
        assert_eq!(floats::<2>("1.5x 2"), Some([1.5, 2.0]));
        assert_eq!(floats::<2>("1.5"), None);
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn bones_turn_between_their_keys_and_children_without_keys_stay() {
        let a = bones(SQUARE).unwrap();
        let rest = a.rest();
        // joint2 sits 1 along x from joint1, both unturned at rest.
        assert_eq!(rest[1].absolute[12..15], [1.0, 0.0, 0.0]);
        let mut pose = Vec::new();
        // Halfway from the first key (frame 1, unturned) to the second
        // (frame 60, 3.1 rad about z): slerped, 1.55 rad.
        a.pose(&rest, 30.5, &mut pose);
        let turned = |m: &Mat4, angle: f32| close(m[0], angle.cos()) && close(m[1], angle.sin());
        assert!(turned(&pose[0], 1.55), "{:?}", pose[0]);
        // Past the last key, it holds; before the first, the first.
        a.pose(&rest, 500.0, &mut pose);
        assert!(turned(&pose[0], 6.2));
        a.pose(&rest, f32::NAN, &mut pose);
        assert!(turned(&pose[0], 6.2));
        a.pose(&rest, 0.0, &mut pose);
        assert!(turned(&pose[0], 0.0));
        // A bone without keys stays at rest, not under its parent.
        assert_eq!(pose[1], rest[1].absolute);
    }

    #[test]
    fn vertices_that_follow_a_bone_are_made_relative_to_it() {
        // "Bare"'s vertex at (1, 0, 0) follows joint2, 1 along x at rest;
        // the others none.
        let text = SQUARE.replace("0 1 0 0 0 0 0\r\n", "0 1 0 0 0 0 1\r\n");
        let mut g = meshes(&text).unwrap();
        assert_eq!(g.meshes[1].bone, None);
        let rest = bones(SQUARE).unwrap().rest();
        to_rest(&mut g, &rest);
        let pos: Vec<[f32; 3]> = g.meshes[1].vertices.iter().map(|v| v.pos).collect();
        assert_eq!(pos, [[0.0; 3], [0.0; 3], [0.0, 1.0, 0.0]]);
    }

    #[test]
    fn two_meshes_of_one_name_are_merged_into_the_first() {
        let twice = SQUARE.replace("\"Bare\"", "\"Square\"");
        let g = meshes(&twice).unwrap();
        assert_eq!(g.meshes.len(), 2);
        assert_eq!(g.meshes[0].vertices.len(), 7);
        assert_eq!(g.meshes[0].triangles, [[0, 1, 2], [0, 2, 3], [4, 5, 6]]);
        assert_eq!(g.meshes[1].triangles, [[0, 1, 2]]);
    }
}
