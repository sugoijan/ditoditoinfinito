// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Fit-oriented model evaluation. Every value uses exactly the operations,
//! operands and order of the original `Model::geometry` and `Model::transforms`,
//! so results are bit-for-bit identical; only work whose result is unused or
//! unchanged is skipped. Tests compare against the retained original code.
use crate::{IDENTITY, Matrix, Model, Parameters, Point, mul, point, vector};

/// Fingertip skin vertices, derived from `Model::tips` at load. Each digit's
/// vertices are grouped by their ordered nonzero influences, stored as
/// structure-of-arrays "slots" so a group's skinning is a uniform loop. The
/// tip reduction still visits vertices in their original order.
pub(crate) struct Skin {
    x: [[f64; 144]; 2],
    y: [[f64; 144]; 2],
    z: [[f64; 144]; 2],
    w: [[[f64; 144]; 4]; 2],
    groups: [Vec<Group>; 2],
    /// Original vertex index to slot.
    slot: [[u8; 144]; 2],
    /// Joint matrices (`g` indices) that any fingertip vertex references.
    pub(crate) bones: u32,
    /// Per digit, the joints influencing its vertices.
    pub(crate) influences: [u32; 2],
    /// Every vertex's skin weights sum to one (within float32 rounding), so a
    /// root translation moves each fingertip patch by exactly that offset.
    pub(crate) rigid: bool,
}

#[derive(Clone, Copy)]
struct Group {
    start: usize,
    end: usize,
    count: usize,
    bones: [usize; 4],
    /// One influence, every weight exactly 1: `1 * t` is `t`, bit for bit.
    unit: bool,
}

impl Default for Skin {
    fn default() -> Self {
        Self {
            x: [[0.; 144]; 2],
            y: [[0.; 144]; 2],
            z: [[0.; 144]; 2],
            w: [[[0.; 144]; 4]; 2],
            groups: [Vec::new(), Vec::new()],
            slot: [[0; 144]; 2],
            bones: 0,
            influences: [0; 2],
            rigid: false,
        }
    }
}

/// The pose quantities the fitting objective reads.
#[derive(Clone, Copy)]
pub(crate) struct Eval {
    pub(crate) tips: [Point; 2],
    pub(crate) com: Point,
    pub(crate) knuckles: [f64; 2],
}

/// Cached kinematics of one pose: the root, each digit's local chain, the
/// palm/digit body transforms and the joint matrices the skin references,
/// plus reusable fingertip working storage (every slot is written before use).
#[derive(Clone)]
pub(crate) struct Kin {
    root: Matrix,
    chain: [[Matrix; 3]; 2],
    b: [Matrix; 7],
    g: [Matrix; 19],
    /// The mass correction in the palm frame; depends on the digit controls only.
    local: Point,
    work: SkinWork,
}

/// Per digit: skinned patch coordinates by slot and each slot's weight in the
/// fingertip (kept for the analytic derivatives); and the exponent arguments
/// of the digit being skinned.
#[derive(Clone)]
pub(crate) struct SkinWork {
    out: [[[f64; 144]; 3]; 2],
    weight: [[f64; 144]; 2],
    arg: [f64; 144],
}

impl Default for Kin {
    fn default() -> Self {
        Self {
            root: IDENTITY,
            chain: [[IDENTITY; 3]; 2],
            b: [IDENTITY; 7],
            g: [IDENTITY; 19],
            local: [0.; 3],
            work: SkinWork {
                out: [[[0.; 144]; 3]; 2],
                weight: [[0.; 144]; 2],
                arg: [0.; 144],
            },
        }
    }
}

impl Skin {
    pub(crate) fn new(m: &Model) -> Self {
        let mut s = Self {
            rigid: m
                .tips
                .iter()
                .flatten()
                .all(|v| (v.w.iter().sum::<f64>() - 1.).abs() < 1e-6),
            ..Self::default()
        };
        for d in 0..2 {
            let mut keys: Vec<(usize, [usize; 4], usize)> = Vec::new();
            for (i, v) in m.tips[d].iter().enumerate() {
                let mut bones = [usize::MAX; 4];
                let mut count = 0;
                for j in 0..4 {
                    if v.w[j] != 0. {
                        bones[count] = v.b[j];
                        count += 1;
                        s.influences[d] |= 1 << v.b[j];
                    }
                }
                keys.push((count, bones, i));
            }
            s.bones |= s.influences[d];
            // Stable: equal influence patterns keep their original order.
            keys.sort_by_key(|k| (k.0, k.1));
            for (slot, &(count, bones, i)) in keys.iter().enumerate() {
                let v = &m.tips[d][i];
                s.x[d][slot] = v.p[0];
                s.y[d][slot] = v.p[1];
                s.z[d][slot] = v.p[2];
                let mut k = 0;
                for j in 0..4 {
                    if v.w[j] != 0. {
                        s.w[d][k][slot] = v.w[j];
                        k += 1;
                    }
                }
                s.slot[d][i] = slot as u8;
                match s.groups[d].last_mut() {
                    Some(g) if g.count == count && g.bones == bones => g.end = slot + 1,
                    _ => s.groups[d].push(Group {
                        start: slot,
                        end: slot + 1,
                        count,
                        bones,
                        unit: false,
                    }),
                }
            }
            for g in &mut s.groups[d] {
                g.unit = g.count == 1 && s.w[d][0][g.start..g.end].iter().all(|w| *w == 1.);
            }
        }
        s
    }
}

impl Model {
    fn root(&self, q: &Parameters) -> Matrix {
        let [ax, ay, az] = [
            -1.40 + q[11].to_radians(),
            -0.32 + q[12].to_radians(),
            q[13].to_radians(),
        ];
        let (sx, cx) = ax.sin_cos();
        let (sy, cy) = ay.sin_cos();
        let (sz, cz) = az.sin_cos();
        [
            [
                cz * cy,
                cz * sy * sx - sz * cx,
                cz * sy * cx + sz * sx,
                q[8],
            ],
            [
                sz * cy,
                sz * sy * sx + cz * cx,
                sz * sy * cx - cz * sx,
                q[9],
            ],
            [-sy, cy * sx, cy * cx, q[10]],
            [0., 0., 0., 1.],
        ]
    }

    /// One digit joint (`i` = digit * 3 + joint) applied to the chain `p`.
    fn joint(&self, q: &Parameters, p: &Matrix, d: usize, j: usize) -> Matrix {
        let i = d * 3 + j;
        let bend = q[d * 4 + j];
        let (sa, ca) = bend.to_radians().sin_cos();
        let (ss, cs) = if j == 0 {
            q[d * 4 + 3].to_radians().sin_cos()
        } else {
            (0., 1.)
        };
        let u = (bend * 4.).clamp(0., 440.);
        let k = (u.floor() as usize).min(439);
        let shift =
            self.shifts[i][k] + (u - k as f64) * (self.shifts[i][k + 1] - self.shifts[i][k]);
        let dr = self.directions[i].map(|v| v * shift);
        let delta = [
            [cs, -ss, 0., dr[0]],
            [ca * ss, ca * cs, sa, dr[1]],
            [-sa * ss, -sa * cs, ca, dr[2]],
            [0., 0., 0., 1.],
        ];
        mul(&mul(p, &self.locals[i]), &delta)
    }

    /// Joint matrix `g[bone]` from body transform `b` (bones 3..19).
    fn bone(&self, bone: usize, b: &Matrix) -> Matrix {
        mul(&mul(&self.a, b), &self.bind[bone - 3])
    }

    /// Recompute the skin's joint matrices for bones in `mask`, given the
    /// body transforms. Bones 1 and 2 follow the palm (bone 3).
    fn skin_bones(&self, kin: &mut Kin, mask: u32) {
        let need = mask & self.skin.bones;
        let palm_moved = need & 0b110 != 0;
        for bone in 3..19 {
            if need & (1 << bone) != 0 || (bone == 3 && palm_moved) {
                let i = bone - 3;
                kin.g[bone] = if i < 7 {
                    self.bone(bone, &kin.b[i])
                } else {
                    self.bone(bone, &mul(&kin.root, &self.fixed[i]))
                };
            }
        }
        if palm_moved {
            let palm = self.helper_origins[3];
            let moved = point(&kin.g[3], palm);
            for i in 1..3 {
                let mut g = IDENTITY;
                for k in 0..3 {
                    g[k][3] = moved[k] - palm[k];
                }
                kin.g[i] = g;
            }
        }
    }

    pub(crate) fn kinematics(&self, q: &Parameters, kin: &mut Kin) {
        kin.root = self.root(q);
        kin.b[0] = mul(&kin.root, &self.fixed[0]);
        for d in 0..2 {
            let mut p = IDENTITY;
            for j in 0..3 {
                p = self.joint(q, &p, d, j);
                kin.chain[d][j] = p;
                kin.b[d * 3 + j + 1] = mul(&kin.root, &p);
            }
        }
        self.skin_bones(kin, u32::MAX);
    }

    /// Skin one digit and reduce it to the fingertip point, writing the patch
    /// points in their original order when requested.
    pub(crate) fn skin_tip(
        &self,
        d: usize,
        g: &[Matrix; 19],
        work: &mut SkinWork,
        patches: Option<&mut [Point]>,
    ) -> Point {
        let s = &self.skin;
        let out = &mut work.out[d];
        for group in &s.groups[d] {
            let r = group.start..group.end;
            let [ox, oy, oz] = &mut *out;
            let at = [&s.x[d][r.clone()], &s.y[d][r.clone()], &s.z[d][r.clone()]];
            let out = [&mut ox[r.clone()], &mut oy[r.clone()], &mut oz[r.clone()]];
            let w = &s.w[d];
            let b = group.bones;
            match group.count {
                0 => skin_group::<0, false>(g, &self.inv_a, [], at, [], out),
                1 if group.unit => {
                    skin_group::<1, true>(g, &self.inv_a, [b[0]], at, [&w[0][r]], out)
                }
                1 => skin_group::<1, false>(g, &self.inv_a, [b[0]], at, [&w[0][r]], out),
                2 => {
                    let ws = [&w[0][r.clone()], &w[1][r]];
                    skin_group::<2, false>(g, &self.inv_a, [b[0], b[1]], at, ws, out)
                }
                3 => {
                    let ws = [&w[0][r.clone()], &w[1][r.clone()], &w[2][r]];
                    skin_group::<3, false>(g, &self.inv_a, [b[0], b[1], b[2]], at, ws, out)
                }
                _ => {
                    let ws = [
                        &w[0][r.clone()],
                        &w[1][r.clone()],
                        &w[2][r.clone()],
                        &w[3][r],
                    ];
                    skin_group::<4, false>(g, &self.inv_a, b, at, ws, out)
                }
            }
        }
        let slot = &s.slot[d];
        let low = lowest(&out[2], slot);
        for (a, z) in work.arg.iter_mut().zip(&out[2]) {
            *a = -(z - low) / 0.8;
        }
        let mut tip = [0.; 3];
        let mut sum = 0.;
        for &k in slot {
            let k = k as usize;
            let p = [out[0][k], out[1][k]];
            let w = work.arg[k].exp();
            work.weight[d][k] = w;
            sum += w;
            for k in 0..2 {
                tip[k] += p[k] * w;
            }
        }
        for t in &mut tip[..2] {
            *t /= sum;
        }
        tip[2] = low;
        if let Some(patches) = patches {
            for (i, &k) in slot.iter().enumerate() {
                let k = k as usize;
                patches[i] = [out[0][k], out[1][k], out[2][k]];
            }
        }
        tip
    }

    /// The mass correction in the palm frame, as `geometry` computes it.
    fn mass_correction(&self, q: &Parameters) -> Point {
        let mut f = [0.; 45];
        f[0] = 1.;
        for i in 0..8 {
            f[1 + i] = q[i] / 60.;
        }
        let mut k = 9;
        for i in 0..8 {
            for j in i..8 {
                f[k] = q[i] * q[j] / 3600.;
                k += 1;
            }
        }
        std::array::from_fn(|i| (0..45).map(|j| self.mass_correction[i][j] * f[j]).sum())
    }

    /// The mass proxy and its corrected center, as `geometry` computes them.
    fn mass_center(&self, b: &[Matrix; 7], local: &Point) -> (Point, Point) {
        let mut com = point(&b[0], self.mass_fixed);
        for i in 0..6 {
            let v = vector(&b[i + 1], self.mass_local[i]);
            for k in 0..3 {
                com[k] += v[k] + (b[i + 1][k][3] - b[0][k][3]) * self.mass_weight[i];
            }
        }
        let proxy = com;
        let bias = vector(&b[0], *local);
        for i in 0..3 {
            com[i] += bias[i];
        }
        (proxy, com)
    }

    /// The complete `Geometry`, including every fingertip patch point.
    pub(crate) fn geometry_into(&self, q: &Parameters, kin: &mut Kin) -> crate::Geometry {
        self.kinematics(q, kin);
        let mut o = crate::Geometry {
            proxy_com: [0.; 3],
            tips: [[0.; 3]; 2],
            patches: [[0.; 3]; 288],
            com: [0.; 3],
            knuckles: [kin.b[2][2][3], kin.b[5][2][3]],
        };
        for d in 0..2 {
            let patches = &mut o.patches[d * 144..(d + 1) * 144];
            o.tips[d] = self.skin_tip(d, &kin.g, &mut kin.work, Some(patches));
        }
        kin.local = self.mass_correction(q);
        (o.proxy_com, o.com) = self.mass_center(&kin.b, &kin.local);
        o
    }

    /// Evaluate `q`, leaving its kinematics in `kin` for perturbed columns.
    pub(crate) fn evaluate_into(&self, q: &Parameters, kin: &mut Kin) -> Eval {
        self.kinematics(q, kin);
        kin.local = self.mass_correction(q);
        self.evaluate_kinematics(kin)
    }

    fn evaluate_kinematics(&self, kin: &mut Kin) -> Eval {
        Eval {
            tips: [
                self.skin_tip(0, &kin.g, &mut kin.work, None),
                self.skin_tip(1, &kin.g, &mut kin.work, None),
            ],
            com: self.mass_center(&kin.b, &kin.local).1,
            knuckles: [kin.b[2][2][3], kin.b[5][2][3]],
        }
    }

    /// Evaluate `trial`, which differs from the pose of `base`/`base_kin` only in
    /// `control`. Unchanged transforms and unaffected fingertips are reused;
    /// `scratch` receives any recomputed kinematics.
    pub(crate) fn evaluate_perturbed(
        &self,
        trial: &Parameters,
        control: usize,
        base: &Eval,
        base_kin: &Kin,
        scratch: &mut Kin,
    ) -> Eval {
        if (8..11).contains(&control) && self.skin.rigid {
            return translated(
                base,
                trial[control] - base_kin.root[control - 8][3],
                control - 8,
            );
        }
        if control >= 8 {
            // Root controls move every body transform, but each digit's local
            // chain and the mass correction depend on the digit controls only.
            scratch.root = self.root(trial);
            scratch.chain = base_kin.chain;
            scratch.local = base_kin.local;
            scratch.b[0] = mul(&scratch.root, &self.fixed[0]);
            for d in 0..2 {
                for j in 0..3 {
                    scratch.b[d * 3 + j + 1] = mul(&scratch.root, &scratch.chain[d][j]);
                }
            }
            self.skin_bones(scratch, u32::MAX);
            return self.evaluate_kinematics(scratch);
        }
        let d = control / 4;
        let first = if control % 4 == 3 { 0 } else { control % 4 };
        // The joints of digit `d` from `first` onward (`g` indices).
        let mut affected = 0_u32;
        for j in first..3 {
            affected |= 1 << (d * 3 + j + 4);
        }
        scratch.root = base_kin.root;
        scratch.b = base_kin.b;
        let mut p = if first == 0 {
            IDENTITY
        } else {
            base_kin.chain[d][first - 1]
        };
        for j in first..3 {
            p = self.joint(trial, &p, d, j);
            scratch.b[d * 3 + j + 1] = mul(&scratch.root, &p);
        }
        for bone in 0..19 {
            if self.skin.bones & (1 << bone) != 0 && affected & (1 << bone) == 0 {
                scratch.g[bone] = base_kin.g[bone];
            }
        }
        self.skin_bones(scratch, affected);
        let tips = std::array::from_fn(|t| {
            if self.skin.influences[t] & affected == 0 {
                base.tips[t]
            } else {
                self.skin_tip(t, &scratch.g, &mut scratch.work, None)
            }
        });
        scratch.local = self.mass_correction(trial);
        Eval {
            tips,
            com: self.mass_center(&scratch.b, &scratch.local).1,
            knuckles: [scratch.b[2][2][3], scratch.b[5][2][3]],
        }
    }
}

/// Analytic first derivatives of the fitted quantities with respect to each
/// control, per degree or millimeter as the controls are: the fingertip
/// points, the mass center and the knuckle heights.
pub(crate) struct Derivatives {
    pub(crate) tips: [[Point; 14]; 2],
    pub(crate) com: [Point; 14],
    pub(crate) knuckles: [[f64; 14]; 2],
}

/// How a control moves a rigid body: a point `x` of it moves at
/// `w × (x − o) + v` per unit of the control.
#[derive(Clone, Copy, Default)]
struct Twist {
    w: Point,
    o: Point,
    v: Point,
}

impl Twist {
    fn point(&self, x: Point) -> Point {
        let r = cross(
            self.w,
            [x[0] - self.o[0], x[1] - self.o[1], x[2] - self.o[2]],
        );
        [r[0] + self.v[0], r[1] + self.v[1], r[2] + self.v[2]]
    }
    fn vector(&self, x: Point) -> Point {
        cross(self.w, x)
    }
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn column(m: &Matrix, j: usize) -> Point {
    [m[0][j], m[1][j], m[2][j]]
}

fn translation(m: &Matrix) -> Point {
    column(m, 3)
}

/// Weighted sums over one digit's patch vertices for the bones a control
/// moves (`T`, the vertices' positions by those bones' share of their skin
/// weights, `W`, that share): `Σ e·T`, `Σ e·W`, and for the horizontal axes
/// `Σ e·(p − tip)·T` and `Σ e·(p − tip)·W`; and `T`, `W` at the lowest vertex.
#[derive(Clone, Copy)]
struct Moments {
    bones: u32,
    t: Point,
    w: f64,
    pt: [Point; 2],
    pw: [f64; 2],
    low_t: Point,
    low_w: f64,
}

const DEG: f64 = std::f64::consts::PI / 180.;

impl Model {
    /// Analytic derivatives at the pose last evaluated into `kin` (and `base`,
    /// its evaluation). `None` for a skin whose fingertips follow the palm
    /// helper joints (bones 0–2, which a root rotation does not turn), where
    /// fits keep finite differences. Non-normalized weights and skins needing
    /// more distinct moving-bone sets than the fast buffer also use that
    /// fallback instead of assuming its fixed layout.
    ///
    /// Every control moves each body rigidly: the root translations and
    /// rotations move all of them, a digit's bend (with the joint's shift along
    /// its direction) or splay the bodies from that joint on. A fingertip is
    /// the soft minimum of its patch: horizontally the weighted mean, with
    /// weights `e = exp(−(z − low)/0.8)`, vertically the lowest point. Its
    /// horizontal derivative is `Σ e·dp/S − Σ e·(p − tip)·dz/(0.8·S)` (the
    /// lowest point's own motion cancels), which the moments give for any
    /// rigid motion of the bones involved.
    pub(crate) fn derivatives(
        &self,
        q: &Parameters,
        kin: &Kin,
        base: &Eval,
    ) -> Option<Derivatives> {
        if self.skin.bones & 0b111 != 0 || !self.skin.rigid {
            return None;
        }
        // Each control's twist and the bodies (palm 0, digit bodies 1–6) and
        // skin bones (`g` indices, body + 3) it moves.
        let mut twists = [Twist::default(); 14];
        let mut bodies = [0_u32; 14];
        let t = translation(&kin.root);
        for axis in 0..3 {
            twists[8 + axis].v[axis] = 1.;
        }
        let (sz, cz) = q[13].to_radians().sin_cos();
        let rotation_axes = [column(&kin.root, 0), [-sz, cz, 0.], [0., 0., 1.]];
        for (k, axis) in rotation_axes.into_iter().enumerate() {
            twists[11 + k] = Twist {
                w: axis.map(|v| v * DEG),
                o: t,
                v: [0.; 3],
            };
        }
        bodies[8..].fill(u32::MAX);
        for d in 0..2 {
            for j in 0..3 {
                let i = d * 3 + j;
                let frame = if j == 0 { &kin.root } else { &kin.b[i] };
                let p = crate::mul(frame, &self.locals[i]);
                let o = translation(&kin.b[i + 1]);
                let bend = q[d * 4 + j];
                let u = bend * 4.;
                let slope = if (0. ..440.).contains(&u) {
                    let k = (u.floor() as usize).min(439);
                    4. * (self.shifts[i][k + 1] - self.shifts[i][k])
                } else {
                    0.
                };
                let x = column(&p, 0);
                let direction = vector(&p, self.directions[i]);
                let moved = (j..3).map(|j| 1 << (d * 3 + j + 1)).sum::<u32>();
                twists[d * 4 + j] = Twist {
                    w: x.map(|v| -v * DEG),
                    o,
                    v: direction.map(|v| v * slope),
                };
                bodies[d * 4 + j] = moved;
                if j == 0 {
                    let (sa, ca) = bend.to_radians().sin_cos();
                    let splay = vector(&p, [0., sa, ca]);
                    twists[d * 4 + 3] = Twist {
                        w: splay.map(|v| v * DEG),
                        o,
                        v: [0.; 3],
                    };
                    bodies[d * 4 + 3] = moved;
                }
            }
        }
        let bones = |c: usize| -> u32 {
            if bodies[c] == u32::MAX {
                !0b111
            } else {
                bodies[c] << 3
            }
        };
        let mut out = Derivatives {
            tips: [[[0.; 3]; 14]; 2],
            com: [[0.; 3]; 14],
            knuckles: [[0.; 14]; 2],
        };
        for d in 0..2 {
            // The distinct sets of this digit's skin bones the controls move.
            let influences = self.skin.influences[d];
            let mut sets = [0_u32; 4];
            let mut count = 0;
            let mut which = [usize::MAX; 14];
            for (c, slot) in which.iter_mut().enumerate() {
                let set = influences & bones(c);
                if set == 0 {
                    continue;
                }
                *slot = match sets[..count].iter().position(|s| *s == set) {
                    Some(i) => i,
                    None => {
                        // More general cross-finger skins can need seven sets.
                        // Keep the bounded fast path, with the existing
                        // finite-difference fallback for unsupported layouts.
                        if count == sets.len() {
                            return None;
                        }
                        sets[count] = set;
                        count += 1;
                        count - 1
                    }
                };
            }
            let moments = self.moments(d, &sets[..count], kin, base.tips[d]);
            let sum = moments.sum;
            for c in 0..14 {
                let Some(m) = moments.sets[..moments.count].get(which[c]) else {
                    continue;
                };
                let tw = &twists[c];
                let u = {
                    let r = cross(tw.w, tw.o);
                    [tw.v[0] - r[0], tw.v[1] - r[1], tw.v[2] - r[2]]
                };
                let mean = cross(tw.w, m.t);
                let tip = &mut out.tips[d][c];
                for a in 0..2 {
                    let dz = tw.w[0] * m.pt[a][1] - tw.w[1] * m.pt[a][0] + m.pw[a] * u[2];
                    tip[a] = (mean[a] + m.w * u[a] - dz / 0.8) / sum;
                }
                tip[2] = tw.w[0] * m.low_t[1] - tw.w[1] * m.low_t[0] + m.low_w * u[2];
            }
        }
        // The mass center as `mass_center` composes it, and the knuckles.
        let b = &kin.b;
        let palm_point = point(&b[0], self.mass_fixed);
        let bias = vector(&b[0], kin.local);
        let parts: [Point; 6] = std::array::from_fn(|i| vector(&b[i + 1], self.mass_local[i]));
        let places: [Point; 7] = std::array::from_fn(|i| translation(&b[i]));
        let weight = self.mass_weight.iter().sum::<f64>();
        for c in 0..14 {
            let tw = &twists[c];
            let moved = |i: usize| bodies[c] & (1 << i) != 0;
            let mut com = [0.; 3];
            let mut add = |v: Point, scale: f64| {
                for k in 0..3 {
                    com[k] += scale * v[k];
                }
            };
            if moved(0) {
                add(tw.point(palm_point), 1.);
                add(tw.vector(bias), 1.);
                add(tw.point(places[0]), -weight);
            }
            for i in 0..6 {
                if moved(i + 1) {
                    add(tw.vector(parts[i]), 1.);
                    add(tw.point(places[i + 1]), self.mass_weight[i]);
                }
            }
            if c < 8 {
                add(vector(&b[0], self.mass_correction_derivative(q, c)), 1.);
            }
            out.com[c] = com;
            for d in 0..2 {
                if moved(d * 3 + 2) {
                    out.knuckles[d][c] = tw.point(places[d * 3 + 2])[2];
                }
            }
        }
        Some(out)
    }

    /// The mass correction's derivative with respect to digit control `c`.
    fn mass_correction_derivative(&self, q: &Parameters, c: usize) -> Point {
        let mut f = [0.; 45];
        f[1 + c] = 1. / 60.;
        let mut k = 9;
        for i in 0..8 {
            for j in i..8 {
                f[k] = (if i == c { q[j] } else { 0. } + if j == c { q[i] } else { 0. }) / 3600.;
                k += 1;
            }
        }
        std::array::from_fn(|i| (0..45).map(|j| self.mass_correction[i][j] * f[j]).sum())
    }

    /// `Moments` of digit `d` for each set of skin bones in `sets`, from the
    /// fingertip last skinned into `kin` (at `tip`), in one pass over the
    /// patch. A vertex whose bones a set holds entirely moves as its skinned
    /// point (its weights sum to one); otherwise its share is skinned apart.
    fn moments(&self, d: usize, sets: &[u32], kin: &Kin, tip: Point) -> DigitMoments {
        let s = &self.skin;
        let [x, y, z] = &kin.work.out[d];
        let e = &kin.work.weight[d];
        let empty = Moments {
            bones: 0,
            t: [0.; 3],
            w: 0.,
            pt: [[0.; 3]; 2],
            pw: [0.; 2],
            low_t: [0.; 3],
            low_w: 0.,
        };
        let mut out = DigitMoments {
            sum: e.iter().sum(),
            sets: [empty; 4],
            count: sets.len(),
        };
        for (m, &bones) in out.sets.iter_mut().zip(sets) {
            m.bones = bones;
        }
        let low = z.iter().position(|v| *v == tip[2]).unwrap_or(0);
        for group in &s.groups[d] {
            let range = group.start..group.end;
            let bones = group.bones[..group.count]
                .iter()
                .fold(0_u32, |m, b| m | (1 << b));
            // Every vertex of the group moved whole: shared by the sets that
            // hold all of its bones.
            let mut whole = None;
            for m in &mut out.sets[..out.count] {
                let inside = bones & m.bones;
                if inside == 0 {
                    continue;
                }
                if inside == bones {
                    let w = *whole.get_or_insert_with(|| {
                        let mut w = empty;
                        for k in range.clone() {
                            let p = [x[k], y[k], z[k]];
                            accumulate(&mut w, e[k], p, tip, p, 1.);
                        }
                        w
                    });
                    add_moments(m, &w);
                    if range.contains(&low) {
                        m.low_t = [x[low], y[low], z[low]];
                        m.low_w = 1.;
                    }
                    continue;
                }
                // Part of each vertex's skin: skin that part in body space.
                let mut joints = [IDENTITY; 4];
                for (j, &bone) in group.bones[..group.count].iter().enumerate() {
                    if inside & (1 << bone) != 0 {
                        joints[j] = crate::mul(&self.inv_a, &kin.g[bone]);
                    }
                }
                for k in range.clone() {
                    let mut t = [0.; 3];
                    let mut w = 0.;
                    for (j, &bone) in group.bones[..group.count].iter().enumerate() {
                        if inside & (1 << bone) != 0 {
                            let weight = s.w[d][j][k];
                            let v = point(&joints[j], [s.x[d][k], s.y[d][k], s.z[d][k]]);
                            for i in 0..3 {
                                t[i] += weight * v[i];
                            }
                            w += weight;
                        }
                    }
                    accumulate(m, e[k], [x[k], y[k], z[k]], tip, t, w);
                    if k == low {
                        m.low_t = t;
                        m.low_w = w;
                    }
                }
            }
        }
        out
    }
}

struct DigitMoments {
    /// `Σ e`, the fingertip weights' sum.
    sum: f64,
    sets: [Moments; 4],
    count: usize,
}

/// Add a vertex at `p` (weight `e`) moving with `t` (share `w`) to `m`.
#[inline(always)]
fn accumulate(m: &mut Moments, e: f64, p: Point, tip: Point, t: Point, w: f64) {
    for (sum, v) in m.t.iter_mut().zip(t) {
        *sum += e * v;
    }
    m.w += e * w;
    for a in 0..2 {
        let r = e * (p[a] - tip[a]);
        for (sum, v) in m.pt[a].iter_mut().zip(t) {
            *sum += r * v;
        }
        m.pw[a] += r * w;
    }
}

fn add_moments(m: &mut Moments, w: &Moments) {
    for i in 0..3 {
        m.t[i] += w.t[i];
    }
    m.w += w.w;
    for a in 0..2 {
        for i in 0..3 {
            m.pt[a][i] += w.pt[a][i];
        }
        m.pw[a] += w.pw[a];
    }
}

/// A pose moved by a root translation of `delta` along `axis`. Every body
/// transform, skinned fingertip patch and the mass center move rigidly with
/// the root (fingertip weights read only height differences, and skin weights
/// sum to one), and knuckle heights move with its vertical component. Exact
/// mathematically; it differs from re-evaluating the pose by rounding only.
fn translated(base: &Eval, delta: f64, axis: usize) -> Eval {
    let mut e = *base;
    for tip in &mut e.tips {
        tip[axis] += delta;
    }
    e.com[axis] += delta;
    if axis == 2 {
        for k in &mut e.knuckles {
            *k += delta;
        }
    }
    e
}

/// The original takes the minimum in vertex order. Without NaN or a zero
/// minimum its value (and bits) cannot depend on the order: NaN is skipped
/// either way and equal nonzero values are bitwise equal. Only a signed-zero
/// tie could, so a zero minimum repeats the original scan.
fn lowest(z: &[f64; 144], order: &[u8; 144]) -> f64 {
    let mut lows = [f64::INFINITY; 4];
    for z in z.as_chunks::<4>().0 {
        for k in 0..4 {
            lows[k] = lows[k].min(z[k]);
        }
    }
    let low = lows[0].min(lows[1]).min(lows[2].min(lows[3]));
    if low != 0. {
        return low;
    }
    let mut low = f64::INFINITY;
    for &k in order {
        low = low.min(z[k as usize]);
    }
    low
}

/// Skin one influence group. Per vertex: the original weighted sum of joint
/// points, in influence order from zero, then the inverse armature transform.
/// `UNIT` groups have one influence of weight exactly 1.
#[inline(always)]
fn skin_group<const N: usize, const UNIT: bool>(
    g: &[Matrix; 19],
    inv_a: &Matrix,
    bones: [usize; N],
    at: [&[f64]; 3],
    w: [&[f64]; N],
    out: [&mut [f64]; 3],
) {
    let n = at[0].len();
    let m: [Matrix; N] = bones.map(|b| g[b]);
    let [x, y, z] = at;
    let [ox, oy, oz] = out;
    let (x, y, z) = (&x[..n], &y[..n], &z[..n]);
    let (ox, oy, oz) = (&mut ox[..n], &mut oy[..n], &mut oz[..n]);
    let w: [&[f64]; N] = w.map(|w| &w[..n]);
    for v in 0..n {
        let at = [x[v], y[v], z[v]];
        let mut p = [0.; 3];
        for j in 0..N {
            let t = point(&m[j], at);
            for k in 0..3 {
                if UNIT {
                    p[k] += t[k];
                } else {
                    p[k] += w[j][v] * t[k];
                }
            }
        }
        let p = point(inv_a, p);
        ox[v] = p[0];
        oy[v] = p[1];
        oz[v] = p[2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowest_matches_the_ordered_minimum_including_zero_ties_and_nan() {
        let mut random = 7_u64;
        let mut next = || {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            (random >> 33) as usize
        };
        for case in 0..2000 {
            let mut order: [u8; 144] = std::array::from_fn(|i| i as u8);
            for i in (1..144).rev() {
                order.swap(i, next() % (i + 1));
            }
            let mut z: [f64; 144] = std::array::from_fn(|_| (next() % 1000) as f64 * 0.01 + 0.5);
            for _ in 0..case % 5 {
                let i = next() % 144;
                z[i] = [0., -0., f64::NAN, 0.25, -3.][next() % 5];
            }
            if case % 7 == 0 {
                z = [f64::NAN; 144];
            }
            let mut expected = f64::INFINITY;
            for &k in &order {
                expected = expected.min(z[k as usize]);
            }
            assert_eq!(
                lowest(&z, &order).to_bits(),
                expected.to_bits(),
                "case {case}"
            );
        }
    }
}
