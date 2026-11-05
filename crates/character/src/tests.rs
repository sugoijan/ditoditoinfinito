use super::*;
use serde_json::{Value, json};
fn fixture() -> (Value, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut add = |data: &[f32], kind: &str, width: usize| {
        let offset = bytes.len();
        for x in data {
            bytes.extend(x.to_le_bytes())
        }
        let view = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":data.len()*4}));
        let i = accessors.len();
        accessors.push(
            json!({"bufferView":view,"componentType":5126,"count":data.len()/width,"type":kind}),
        );
        i
    };
    let p = add(&[0., 0., 0., 1., 0., 0., 0., 1., 0.], "VEC3", 3);
    let n = add(&[0., 0., 1., 0., 0., 1., 0., 0., 1.], "VEC3", 3);
    let w = add(&[1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.], "VEC4", 4);
    let inverse = add(&math::IDENTITY, "MAT4", 16);
    let times = add(&[0., 1.], "SCALAR", 1);
    let translations = add(&[0., 0., 0., 0., 0., 1.], "VEC3", 3);
    let rotations = add(&[0., 0., 0., 1., 0., 0., 1., 0.], "VEC4", 4);
    let morph = add(&[1., 0., 0., 0., 0., 0., 0., 0., 0.], "VEC3", 3);
    let weights = add(&[0., 1.], "SCALAR", 1);
    let mut ushort = |values: &[u16], kind: &str, width: usize| {
        let offset = bytes.len();
        for x in values {
            bytes.extend(x.to_le_bytes())
        }
        while bytes.len() % 4 != 0 {
            bytes.push(0)
        }
        let view = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":values.len()*2}));
        let i = accessors.len();
        accessors.push(
            json!({"bufferView":view,"componentType":5123,"count":values.len()/width,"type":kind}),
        );
        i
    };
    let joints = ushort(&[0; 12], "VEC4", 4);
    let indices = ushort(&[0, 1, 2], "SCALAR", 1);
    (
        json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0,1]}],"buffers":[{"byteLength":bytes.len()}],"bufferViews":views,"accessors":accessors,
    "nodes":[{"name":"joint"},{"mesh":0,"skin":0}],"skins":[{"joints":[0],"inverseBindMatrices":inverse}],"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1,1,1,1]}}],
    "meshes":[{"primitives":[{"attributes":{"POSITION":p,"NORMAL":n,"JOINTS_0":joints,"WEIGHTS_0":w},"indices":indices,"material":0,"targets":[{"POSITION":morph}]}],"weights":[0]}],
    "animations":[{"samplers":[{"input":times,"output":translations},{"input":times,"output":rotations},{"input":times,"output":weights}],"channels":[{"sampler":0,"target":{"node":0,"path":"translation"}},{"sampler":1,"target":{"node":0,"path":"rotation"}},{"sampler":2,"target":{"node":1,"path":"weights"}}]}]}),
        bytes,
    )
}
fn encode(mut d: Value, mut bin: Vec<u8>) -> Vec<u8> {
    d["buffers"][0]["byteLength"] = json!(bin.len());
    let mut js = serde_json::to_vec(&d).unwrap();
    while js.len() % 4 != 0 {
        js.push(b' ')
    }
    while bin.len() % 4 != 0 {
        bin.push(0)
    }
    let mut b = Vec::new();
    for x in [
        0x46546c67,
        2,
        (28 + js.len() + bin.len()) as u32,
        js.len() as u32,
        0x4e4f534a,
    ] {
        b.extend(x.to_le_bytes())
    }
    b.extend(js);
    b.extend((bin.len() as u32).to_le_bytes());
    b.extend(0x004e4942_u32.to_le_bytes());
    b.extend(bin);
    b
}
#[test]
fn interpolation_morph_before_skin_and_endpoints() {
    let (d, b) = fixture();
    let a = Character::load(&encode(d, b)).unwrap();
    let mut p = a.new_pose();
    a.sample(0.5, &mut p);
    assert!((p.positions[0][1] - 0.5).abs() < 1e-6);
    assert!((p.positions[0][2] - 0.5).abs() < 1e-6);
    a.sample(4., &mut p);
    assert!((p.positions[0][0] + 1.).abs() < 1e-6);
    assert!((p.positions[0][2] - 1.).abs() < 1e-6);
    a.sample(-1., &mut p);
    assert_eq!(p.positions[0], [0.; 3]);
    a.sample(f32::NAN, &mut p);
    assert_eq!(p.positions[0], [0.; 3]);
}
#[test]
fn truncation_never_panics() {
    let (d, b) = fixture();
    let glb = encode(d, b);
    for i in 0..glb.len() {
        assert!(Character::load(&glb[..i]).is_err());
    }
}
#[test]
fn malformed_data_and_unsupported_channels_fail() {
    let (d, b) = fixture();
    for mutation in 0..6 {
        let mut d = d.clone();
        match mutation {
            0 => d["bufferViews"][0]["byteLength"] = json!(1),
            1 => d["nodes"][0]["children"] = json!([0]),
            2 => d["animations"][0]["samplers"][0]["interpolation"] = json!("CUBICSPLINE"),
            3 => d["accessors"][0]["count"] = json!(u64::MAX),
            4 => d["buffers"][0]["uri"] = json!("https://example.com/a.bin"),
            _ => {
                d["accessors"][0]["sparse"] = json!({"count":1,"indices":{"bufferView":0,"componentType":5126},"values":{"bufferView":0}})
            }
        };
        assert!(
            Character::load(&encode(d, b.clone())).is_err(),
            "{mutation}"
        );
    }
}
#[test]
fn shortest_rotation_path() {
    let a = math::slerp([0., 0., 0., 1.], [0., 0., 0., -1.], 0.5);
    assert_eq!(a, [0., 0., 0., 1.]);
}
fn append_floats(
    d: &mut Value,
    bin: &mut Vec<u8>,
    data: &[f32],
    kind: &str,
    width: usize,
) -> usize {
    let offset = bin.len();
    for value in data {
        bin.extend(value.to_le_bytes());
    }
    let view = d["bufferViews"].as_array().unwrap().len();
    d["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":offset,"byteLength":data.len()*4}));
    let accessor = d["accessors"].as_array().unwrap().len();
    d["accessors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"bufferView":view,"componentType":5126,"count":data.len()/width,"type":kind}));
    accessor
}

#[test]
fn long_clip_keeps_key_and_aggregate_allocation_bounds() {
    let (mut d, mut b) = fixture();
    let times: Vec<_> = (0..12_001).map(|i| i as f32 / 120.).collect();
    let input = append_floats(&mut d, &mut b, &times, "SCALAR", 1);
    let output = append_floats(&mut d, &mut b, &vec![0.; times.len() * 3], "VEC3", 3);
    d["animations"][0]["samplers"][0] = json!({"input":input,"output":output});
    let asset = Character::load(&encode(d.clone(), b.clone())).unwrap();
    assert_eq!(asset.duration, 100.);
    let mut pose = asset.new_pose();
    asset.sample(99., &mut pose);
    assert!(pose.positions.iter().flatten().all(|x| x.is_finite()));

    let excessive: Vec<_> = (0..20_001).map(|i| i as f32 / 120.).collect();
    let too_many = append_floats(&mut d, &mut b, &excessive, "SCALAR", 1);
    d["animations"][0]["samplers"][0]["input"] = json!(too_many);
    assert_eq!(
        Character::load(&encode(d.clone(), b.clone()))
            .err()
            .unwrap(),
        "animation times"
    );

    d["animations"][0]["samplers"] = json!([{"input":input,"output":output}]);
    for _ in 2..102 {
        d["nodes"].as_array_mut().unwrap().push(json!({}));
    }
    d["animations"][0]["channels"] = Value::Array(
        (0..102)
            .filter(|&n| n != 1)
            .map(|n| json!({"sampler":0,"target":{"node":n,"path":"translation"}}))
            .collect(),
    );
    assert_eq!(
        Character::load(&encode(d, b)).err().unwrap(),
        "animation scalar budget"
    );
}
