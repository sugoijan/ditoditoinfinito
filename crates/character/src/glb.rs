use crate::{Character, Morph, Node, Path, Primitive, SurfaceKind, Track, Vertex};
use serde_json::Value;
use std::collections::BTreeSet;
const MAX_VERTICES: usize = 50_000;
const MAX_NODES: usize = 256;
const MAX_MORPHS: usize = 96;
const MAX_ANIMATION_KEYS: usize = 20_000;
const MAX_ANIMATION_SCALARS: usize = 4_000_000;
fn array(v: &Value) -> Result<&[Value], String> {
    v.as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "expected array".into())
}
fn uint(v: &Value) -> Result<usize, String> {
    v.as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| "expected unsigned index".into())
}
fn number(v: &Value) -> Result<f32, String> {
    let n = v.as_f64().ok_or("expected number")?;
    if !n.is_finite() || n.abs() > 1e6 {
        return Err("number outside finite character limits".into());
    }
    Ok(n as f32)
}
fn at<'a>(v: &'a Value, key: &str, i: usize) -> Result<&'a Value, String> {
    array(&v[key])?
        .get(i)
        .ok_or_else(|| format!("{key} index out of bounds"))
}
fn fixed<const N: usize>(v: &Value, default: [f32; N]) -> Result<[f32; N], String> {
    if v.is_null() {
        return Ok(default);
    }
    let a = array(v)?;
    if a.len() != N {
        return Err("vector length".into());
    }
    let mut r = [0.; N];
    for (x, v) in r.iter_mut().zip(a) {
        *x = number(v)?
    }
    Ok(r)
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b).ok_or_else(|| "byte range overflow".into())
}
fn times(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b).ok_or_else(|| "byte range overflow".into())
}
fn word(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..add(offset, 4)?)
            .ok_or("truncated GLB")?
            .try_into()
            .unwrap(),
    ))
}
struct Reader<'a> {
    doc: Value,
    bin: &'a [u8],
}
impl Reader<'_> {
    fn view(&self, i: usize) -> Result<&[u8], String> {
        let v = at(&self.doc, "bufferViews", i)?;
        if uint(&v["buffer"])? != 0 {
            return Err("external buffer".into());
        }
        let off = v.get("byteOffset").map(uint).transpose()?.unwrap_or(0);
        let len = uint(&v["byteLength"])?;
        self.bin
            .get(off..add(off, len)?)
            .ok_or_else(|| "buffer view outside BIN".into())
    }
    fn data(
        &self,
        view: usize,
        offset: usize,
        count: usize,
        width: usize,
        component: usize,
        stride: Option<usize>,
    ) -> Result<Vec<f32>, String> {
        let size = match component {
            5121 => 1,
            5123 => 2,
            5125 | 5126 => 4,
            _ => return Err("unsupported accessor component".into()),
        };
        let element = times(size, width)?;
        let stride = stride.unwrap_or(element);
        if stride < element || stride % size != 0 || offset % size != 0 {
            return Err("misaligned accessor".into());
        }
        let bytes = self.view(view)?;
        let end = add(
            add(offset, times(count.saturating_sub(1), stride)?)?,
            element,
        )?;
        if end > bytes.len() {
            return Err("accessor outside buffer view".into());
        }
        let mut result = Vec::with_capacity(times(count, width)?);
        for i in 0..count {
            for j in 0..width {
                let at = offset + i * stride + j * size;
                let n = match component {
                    5121 => f32::from(bytes[at]),
                    5123 => f32::from(u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())),
                    5125 => u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as f32,
                    _ => f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
                };
                if !n.is_finite() {
                    return Err("non-finite accessor".into());
                }
                result.push(n)
            }
        }
        Ok(result)
    }
    fn accessor(&self, i: usize, kind: &str, components: &[usize]) -> Result<Vec<f32>, String> {
        let a = at(&self.doc, "accessors", i)?;
        if a["type"] != kind || a["normalized"].as_bool() == Some(true) {
            return Err("unsupported accessor type/normalization".into());
        }
        let component = uint(&a["componentType"])?;
        if !components.contains(&component) {
            return Err("accessor component type".into());
        }
        let count = uint(&a["count"])?;
        let width = match kind {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            _ => return Err("accessor shape".into()),
        };
        let len = times(count, width)?;
        if count == 0 || len > 4_000_000 {
            return Err("accessor limit".into());
        }
        let mut result = if let Some(v) = a.get("bufferView") {
            let v = uint(v)?;
            let stride = at(&self.doc, "bufferViews", v)?
                .get("byteStride")
                .map(uint)
                .transpose()?;
            self.data(
                v,
                a.get("byteOffset").map(uint).transpose()?.unwrap_or(0),
                count,
                width,
                component,
                stride,
            )?
        } else {
            vec![0.; len]
        };
        if let Some(s) = a.get("sparse") {
            let n = uint(&s["count"])?;
            if n == 0 || n > count {
                return Err("sparse count".into());
            }
            let iv = &s["indices"];
            let index_type = uint(&iv["componentType"])?;
            if ![5121, 5123, 5125].contains(&index_type) {
                return Err("sparse index component type".into());
            }
            let indices = self.data(
                uint(&iv["bufferView"])?,
                iv.get("byteOffset").map(uint).transpose()?.unwrap_or(0),
                n,
                1,
                index_type,
                None,
            )?;
            let vv = &s["values"];
            let values = self.data(
                uint(&vv["bufferView"])?,
                vv.get("byteOffset").map(uint).transpose()?.unwrap_or(0),
                n,
                width,
                component,
                None,
            )?;
            let mut previous = None;
            for (j, &id) in indices.iter().enumerate() {
                let id = id as usize;
                if id >= count || previous.is_some_and(|p| id <= p) {
                    return Err("sparse indices must be increasing and in bounds".into());
                }
                previous = Some(id);
                result[id * width..(id + 1) * width]
                    .copy_from_slice(&values[j * width..(j + 1) * width]);
            }
        }
        Ok(result)
    }
    fn attribute(
        &self,
        p: &Value,
        name: &str,
        kind: &str,
        components: &[usize],
    ) -> Result<Vec<f32>, String> {
        self.accessor(uint(&p["attributes"][name])?, kind, components)
    }
}
fn unit(v: &[f32]) -> bool {
    (v.iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 0.002
}
fn visit(i: usize, nodes: &[Node], marks: &mut [u8], order: &mut Vec<usize>) -> Result<(), String> {
    if marks[i] == 1 {
        return Err("node hierarchy cycle".into());
    }
    if marks[i] == 2 {
        return Ok(());
    }
    marks[i] = 1;
    if let Some(p) = nodes[i].parent {
        visit(p, nodes, marks, order)?
    }
    marks[i] = 2;
    order.push(i);
    Ok(())
}
pub(super) fn load(bytes: &[u8]) -> Result<Character, String> {
    if bytes.len() > 16 * 1024 * 1024
        || word(bytes, 0)? != 0x46546c67
        || word(bytes, 4)? != 2
        || word(bytes, 8)? as usize != bytes.len()
    {
        return Err("invalid GLB header or size".into());
    }
    let json_len = word(bytes, 12)? as usize;
    if json_len > 4 * 1024 * 1024 || json_len % 4 != 0 || word(bytes, 16)? != 0x4e4f534a {
        return Err("invalid GLB JSON chunk".into());
    }
    let end = add(20, json_len)?;
    let json = bytes.get(20..end).ok_or("truncated JSON")?;
    let bin_len = word(bytes, end)? as usize;
    if bin_len % 4 != 0
        || word(bytes, end + 4)? != 0x004e4942
        || add(end + 8, bin_len)? != bytes.len()
    {
        return Err("invalid GLB BIN chunk".into());
    }
    let doc: Value = serde_json::from_slice(json).map_err(|e| format!("GLB JSON: {e}"))?;
    let bin = &bytes[end + 8..];
    let r = Reader { doc, bin };
    let d = &r.doc;
    if d["asset"]["version"] != "2.0"
        || d.get("extensionsRequired")
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
    {
        return Err("unsupported glTF version/extensions".into());
    }
    if array(&d["buffers"])?.len() != 1 || d["buffers"][0].get("uri").is_some() {
        return Err("GLB must be self-contained".into());
    }
    let declared = uint(&d["buffers"][0]["byteLength"])?;
    if declared > bin.len() || bin.len() - declared > 3 {
        return Err("BIN length mismatch".into());
    }
    if array(&d["meshes"])?.len() != 1
        || array(&d["skins"])?.len() != 1
        || array(&d["animations"])?.len() != 1
    {
        return Err("expected one mesh, skin and clip".into());
    }
    let raw_nodes = array(&d["nodes"])?;
    if raw_nodes.is_empty() || raw_nodes.len() > MAX_NODES {
        return Err("node limit".into());
    }
    let mesh_node = raw_nodes
        .iter()
        .position(|n| n.get("mesh").is_some())
        .ok_or("missing mesh node")?;
    if uint(&raw_nodes[mesh_node]["mesh"])? != 0
        || uint(&raw_nodes[mesh_node]["skin"])? != 0
        || raw_nodes.iter().filter(|n| n.get("mesh").is_some()).count() != 1
    {
        return Err("unsupported mesh instance".into());
    }
    let mut nodes = Vec::new();
    for n in raw_nodes {
        if n.get("matrix").is_some()
            || n.get("camera").is_some()
            || fixed(&n["scale"], [1.; 3])? != [1.; 3]
        {
            return Err("only rigid TRS nodes are supported".into());
        }
        let translation = fixed(&n["translation"], [0.; 3])?;
        let rotation = fixed(&n["rotation"], [0., 0., 0., 1.])?;
        if !unit(&rotation) {
            return Err("non-unit node rotation".into());
        }
        nodes.push(Node {
            parent: None,
            translation,
            rotation,
        });
    }
    for (i, n) in raw_nodes.iter().enumerate() {
        if let Some(c) = n.get("children") {
            for c in array(c)? {
                let child = uint(c)?;
                if child >= nodes.len() || child == i || nodes[child].parent.replace(i).is_some() {
                    return Err("invalid node parent".into());
                }
            }
        }
    }
    let mut order = Vec::new();
    let mut marks = vec![0; nodes.len()];
    for i in 0..nodes.len() {
        visit(i, &nodes, &mut marks, &mut order)?
    }
    if nodes[mesh_node].parent.is_some()
        || nodes[mesh_node].translation != [0.; 3]
        || nodes[mesh_node].rotation != [0., 0., 0., 1.]
    {
        return Err("mesh node must be identity root".into());
    }
    let skin = &d["skins"][0];
    let joints: Vec<usize> = array(&skin["joints"])?
        .iter()
        .map(uint)
        .collect::<Result<_, _>>()?;
    if joints.is_empty()
        || joints.len() > MAX_NODES
        || joints.iter().any(|i| *i >= nodes.len())
        || joints.iter().collect::<BTreeSet<_>>().len() != joints.len()
    {
        return Err("joint indices".into());
    }
    let ibm = r.accessor(uint(&skin["inverseBindMatrices"])?, "MAT4", &[5126])?;
    if ibm.len() != joints.len() * 16 {
        return Err("inverse bind count".into());
    }
    let inverse_bind = ibm
        .chunks_exact(16)
        .map(|m| m.try_into().unwrap())
        .collect();
    let mesh = &d["meshes"][0];
    let prim = array(&mesh["primitives"])?;
    if prim.is_empty() || prim.len() > 16 {
        return Err("primitive limit".into());
    }
    let p = &prim[0];
    let positions = r.attribute(p, "POSITION", "VEC3", &[5126])?;
    let normals = r.attribute(p, "NORMAL", "VEC3", &[5126])?;
    let j = r.attribute(p, "JOINTS_0", "VEC4", &[5121, 5123])?;
    let w = r.attribute(p, "WEIGHTS_0", "VEC4", &[5126])?;
    let count = positions.len() / 3;
    if count > MAX_VERTICES
        || normals.len() != count * 3
        || j.len() != count * 4
        || w.len() != count * 4
    {
        return Err("vertex attribute count/limit".into());
    }
    let mut vertices = Vec::with_capacity(count);
    for i in 0..count {
        let position: [f32; 3] = positions[i * 3..i * 3 + 3].try_into().unwrap();
        let normal: [f32; 3] = normals[i * 3..i * 3 + 3].try_into().unwrap();
        let weights: [f32; 4] = w[i * 4..i * 4 + 4].try_into().unwrap();
        let js = &j[i * 4..i * 4 + 4];
        if !unit(&normal)
            || position.iter().any(|x| x.abs() > 1000.)
            || weights.iter().any(|w| !(0.0..=1.0).contains(w))
            || (weights.iter().sum::<f32>() - 1.).abs() > 0.001
            || js.iter().any(|x| *x as usize >= joints.len())
        {
            return Err("invalid skin vertex".into());
        }
        vertices.push(Vertex {
            position,
            normal,
            joints: std::array::from_fn(|k| js[k] as u16),
            weights,
        })
    }
    let target: &[Value] = p.get("targets").map(array).transpose()?.unwrap_or(&[]);
    if target.len() > MAX_MORPHS {
        return Err("morph limit".into());
    }
    let mut morphs = Vec::new();
    let mut total_offsets = 0;
    for t in target {
        if t.as_object()
            .is_none_or(|o| o.len() != 1 || !o.contains_key("POSITION"))
        {
            return Err("only position morphs supported".into());
        }
        let values = r.accessor(uint(&t["POSITION"])?, "VEC3", &[5126])?;
        if values.len() != count * 3 || values.iter().any(|x| x.abs() > 1000.) {
            return Err("morph size/limit".into());
        }
        let offsets: Vec<_> = values
            .chunks_exact(3)
            .enumerate()
            .filter(|(_, v)| v.iter().any(|x| *x != 0.))
            .map(|(i, v)| (i, v.try_into().unwrap()))
            .collect();
        total_offsets += offsets.len();
        if total_offsets > 2_000_000 {
            return Err("morph allocation limit".into());
        }
        morphs.push(Morph { offsets });
    }
    let initial_weights = if let Some(w) = mesh.get("weights") {
        array(w)?
            .iter()
            .map(number)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![0.; morphs.len()]
    };
    if initial_weights.len() != morphs.len() {
        return Err("mesh morph weight count".into());
    }
    let mut indices = Vec::new();
    let mut primitives = Vec::new();
    for q in prim {
        if q["attributes"] != p["attributes"]
            || q["targets"] != p["targets"]
            || q.get("mode").is_some_and(|x| x.as_u64() != Some(4))
        {
            return Err("expected shared triangle mesh attributes/targets".into());
        }
        let v = r.accessor(uint(&q["indices"])?, "SCALAR", &[5121, 5123, 5125])?;
        if v.len() % 3 != 0
            || indices.len() + v.len() > 300_000
            || v.iter().any(|x| *x as usize >= count)
        {
            return Err("triangle indices".into());
        }
        let start = indices.len() as u32;
        indices.extend(v.iter().map(|x| *x as u32));
        let material = at(d, "materials", uint(&q["material"])?)?;
        let pbr = &material["pbrMetallicRoughness"];
        if material.get("normalTexture").is_some() || pbr.get("baseColorTexture").is_some() {
            return Err("textured materials unsupported".into());
        }
        let color = fixed(&pbr["baseColorFactor"], [1.; 4])?;
        if color.iter().any(|x| !(0.0..=1.0).contains(x))
            || color[3] != 1.
            || material.get("alphaMode").is_some_and(|x| x != "OPAQUE")
        {
            return Err("only opaque base colors supported".into());
        }
        primitives.push(Primitive {
            indices: start..indices.len() as u32,
            color,
            double_sided: material["doubleSided"].as_bool().unwrap_or(false),
            surface: match material["name"].as_str().unwrap_or("") {
                "Rim" => SurfaceKind::Rim,
                "Interior" => SurfaceKind::Interior,
                "Diagnostic mat" => SurfaceKind::DiagnosticMat,
                "Nail" => SurfaceKind::Nail,
                _ => SurfaceKind::Skin,
            },
        });
    }
    let animation = &d["animations"][0];
    let mut tracks = Vec::new();
    let mut seen = BTreeSet::new();
    let mut duration = 0.0_f32;
    let mut animation_scalars = 0_usize;
    for c in array(&animation["channels"])? {
        if tracks.len() > MAX_NODES * 2 {
            return Err("animation channel limit".into());
        }
        let node = uint(&c["target"]["node"])?;
        let name = c["target"]["path"]
            .as_str()
            .ok_or("missing animation path")?;
        if node >= nodes.len() || !seen.insert((node, name.to_owned())) {
            return Err("duplicate or invalid animation target".into());
        }
        let (path, width, kind) = match name {
            "translation" if node != mesh_node => (Path::Translation, 3, "VEC3"),
            "rotation" if node != mesh_node => (Path::Rotation, 4, "VEC4"),
            "weights" if node == mesh_node => (Path::Weights, morphs.len(), "SCALAR"),
            _ => return Err("unsupported animation channel".into()),
        };
        let sampler = at(animation, "samplers", uint(&c["sampler"])?)?;
        if sampler.get("interpolation").is_some_and(|x| x != "LINEAR") {
            return Err("only LINEAR tracks supported".into());
        }
        let times = r.accessor(uint(&sampler["input"])?, "SCALAR", &[5126])?;
        if times.len() > MAX_ANIMATION_KEYS
            || times[0] < 0.
            || times.windows(2).any(|w| w[0] >= w[1])
            || times.last().unwrap() > &3600.
        {
            return Err("animation times".into());
        }
        duration = duration.max(*times.last().unwrap());
        let values = r.accessor(uint(&sampler["output"])?, kind, &[5126])?;
        animation_scalars = add(animation_scalars, add(times.len(), values.len())?)?;
        if animation_scalars > MAX_ANIMATION_SCALARS {
            return Err("animation scalar budget".into());
        }
        if width == 0
            || values.len() != times.len() * width
            || values.iter().any(|x| x.abs() > 1000.)
        {
            return Err("animation output count/limit".into());
        }
        if matches!(path, Path::Rotation) && values.chunks_exact(4).any(|q| !unit(q)) {
            return Err("non-unit animation quaternion".into());
        }
        tracks.push(Track {
            node,
            path,
            times,
            values,
            width,
        });
    }
    if duration <= 0. {
        return Err("empty animation".into());
    }
    let mut fingertips = Vec::new();
    if let Some(tips) = d["extras"]["ddi"]["fingertips"].as_object() {
        for v in tips.values() {
            let ids = array(v)?.iter().map(uint).collect::<Result<Vec<_>, _>>()?;
            if ids.is_empty() || ids.len() > 2048 || ids.iter().any(|i| *i >= count) {
                return Err("fingertip indices".into());
            }
            fingertips.push(ids);
        }
    }
    let joint_names = joints
        .iter()
        .map(|&i| raw_nodes[i]["name"].as_str().unwrap_or("").to_owned())
        .collect();
    Ok(Character {
        vertices,
        indices,
        primitives,
        duration,
        joint_names,
        fingertips,
        nodes,
        order,
        joints,
        inverse_bind,
        morphs,
        initial_weights,
        tracks,
    })
}
