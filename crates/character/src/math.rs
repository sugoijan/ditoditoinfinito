//! Column-major rigid transforms and shortest-path quaternion interpolation.
pub type Mat4 = [f32; 16];
pub const IDENTITY: Mat4 = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}
pub fn point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}
pub fn vector(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2])
}
pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-20);
    v.map(|x| x / l)
}
pub fn trs(t: [f32; 3], q: [f32; 4]) -> Mat4 {
    let [x, y, z, w] = q;
    [
        1. - 2. * (y * y + z * z),
        2. * (x * y + z * w),
        2. * (x * z - y * w),
        0.,
        2. * (x * y - z * w),
        1. - 2. * (x * x + z * z),
        2. * (y * z + x * w),
        0.,
        2. * (x * z + y * w),
        2. * (y * z - x * w),
        1. - 2. * (x * x + y * y),
        0.,
        t[0],
        t[1],
        t[2],
        1.,
    ]
}
pub fn slerp(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>();
    if dot < 0. {
        b = b.map(|x| -x);
        dot = -dot;
    }
    let mut q = if dot > 0.9995 {
        std::array::from_fn(|i| a[i] + t * (b[i] - a[i]))
    } else {
        let angle = dot.clamp(-1., 1.).acos();
        let s = angle.sin();
        std::array::from_fn(|i| (a[i] * ((1. - t) * angle).sin() + b[i] * (t * angle).sin()) / s)
    };
    let l = q.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-20);
    for x in &mut q {
        *x /= l;
    }
    q
}
