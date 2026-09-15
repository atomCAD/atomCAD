//! The rigid fit shared by the placement engine and the tool binder.
//!
//! One least-squares superposition (Horn's quaternion form of Kabsch), used
//! twice: [`place`](super::place) fits an operation's `before` pattern onto a
//! candidate assignment of workpiece atoms, and
//! [`tool_pose`](super::tool_pose) fits a tool type's frame onto the four
//! tagged atoms of the molecule that plays it. Both want the same three
//! answers — the rotation, the translation and the max per-atom residual —
//! and a second implementation of any of them would be a second set of
//! conventions to get wrong.

use glam::{DMat3, DQuat, DVec3};

/// Below this the centred pattern is treated as rank-deficient along a
/// direction — a single point, or a collinear pair. Å.
pub(super) const RANK_EPSILON: f64 = 1e-9;

/// A rigid transform and how well it fits: `p_world = r · p_local + t`.
pub(super) struct Fit {
    pub r: DMat3,
    pub t: DVec3,
    /// Max per-atom distance the fit leaves behind, Å. The **max**, not the
    /// RMS: one atom badly placed is a wrong fit however well the others agree.
    pub residual: f64,
}

/// 0 for a single point (or coincident points), 1 for a collinear set, 2 for
/// anything that spans a plane. Distinguishing 2 from 3 is not needed: both
/// determine a rotation.
pub(super) fn rank_of(points: &[DVec3]) -> usize {
    let centroid = centroid(points);
    let centred: Vec<DVec3> = points.iter().map(|p| *p - centroid).collect();
    let Some(first) = centred
        .iter()
        .copied()
        .max_by(|a, b| a.length().total_cmp(&b.length()))
        .filter(|v| v.length() > RANK_EPSILON)
    else {
        return 0;
    };
    let axis = first.normalize();
    if centred
        .iter()
        .any(|v| v.cross(axis).length() > RANK_EPSILON)
    {
        2
    } else {
        1
    }
}

fn centroid(points: &[DVec3]) -> DVec3 {
    if points.is_empty() {
        return DVec3::ZERO;
    }
    points.iter().copied().sum::<DVec3>() / points.len() as f64
}

/// The rigid transform taking `local` onto `world` in the least-squares sense,
/// and the max per-atom distance it leaves behind.
///
/// `mirrored` asks for the improper fit. The mirror cannot be recovered from the
/// proper one, so it is a second fit — of the pattern reflected through a fixed
/// plane, whose proper fit composed with that reflection is the best improper
/// transform.
///
/// Rank-deficient inputs are resolved rather than left to the eigen solver,
/// which would answer an undetermined question with whichever vector its sweeps
/// happened to produce: a single point fits with the identity, a collinear pair
/// with the shortest arc between the two axes.
pub(super) fn rigid_fit(local: &[DVec3], world: &[DVec3], mirrored: bool) -> Option<Fit> {
    if local.len() != world.len() || local.is_empty() {
        return None;
    }
    const MIRROR: DMat3 = DMat3::from_cols(DVec3::X, DVec3::Y, DVec3::new(0.0, 0.0, -1.0));
    let reflected: Vec<DVec3>;
    let source = if mirrored {
        reflected = local.iter().map(|p| MIRROR * *p).collect();
        &reflected[..]
    } else {
        local
    };

    let source_centroid = centroid(source);
    let world_centroid = centroid(world);
    let centred_source: Vec<DVec3> = source.iter().map(|p| *p - source_centroid).collect();
    let centred_world: Vec<DVec3> = world.iter().map(|p| *p - world_centroid).collect();

    let rotation = match rank_of(source) {
        0 => DMat3::IDENTITY,
        1 => {
            let index = (0..centred_source.len())
                .max_by(|&a, &b| {
                    centred_source[a]
                        .length()
                        .total_cmp(&centred_source[b].length())
                })
                .expect("non-empty");
            let from = centred_source[index];
            let to = centred_world[index];
            if from.length() < RANK_EPSILON || to.length() < RANK_EPSILON {
                DMat3::IDENTITY
            } else {
                DMat3::from_quat(DQuat::from_rotation_arc(from.normalize(), to.normalize()))
            }
        }
        _ => kabsch(&centred_source, &centred_world),
    };

    let r = if mirrored {
        rotation * MIRROR
    } else {
        rotation
    };
    let t = world_centroid - rotation * source_centroid;
    let residual = local
        .iter()
        .zip(world)
        .map(|(p, q)| (r * *p + t).distance(*q))
        .fold(0.0, f64::max);
    Some(Fit { r, t, residual })
}

/// The proper rotation taking the centred `source` points onto the centred
/// `target` points in the least-squares sense.
///
/// Horn's quaternion form rather than an SVD: the largest eigenvector of a
/// symmetric 4×4 is a few dozen lines of Jacobi rotations, it yields a proper
/// rotation by construction (no reflection case to repair), and it adds no
/// dependency.
fn kabsch(source: &[DVec3], target: &[DVec3]) -> DMat3 {
    // s[a][b] = Σ source_a · target_b, the correlation matrix of Horn 1987. The
    // index order is the half of this that is easy to get backwards: the other
    // one yields the transpose, which is a perfectly good rotation matrix and
    // simply rotates the wrong way.
    let mut s = [[0.0f64; 3]; 3];
    for (p, q) in source.iter().zip(target) {
        for a in 0..3 {
            for b in 0..3 {
                s[a][b] += p[a] * q[b];
            }
        }
    }
    let (sxx, sxy, sxz) = (s[0][0], s[0][1], s[0][2]);
    let (syx, syy, syz) = (s[1][0], s[1][1], s[1][2]);
    let (szx, szy, szz) = (s[2][0], s[2][1], s[2][2]);

    let n = [
        [sxx + syy + szz, syz - szy, szx - sxz, sxy - syx],
        [syz - szy, sxx - syy - szz, sxy + syx, szx + sxz],
        [szx - sxz, sxy + syx, -sxx + syy - szz, syz + szy],
        [sxy - syx, szx + sxz, syz + szy, -sxx - syy + szz],
    ];
    let q = largest_eigenvector_4(n);
    let quat = DQuat::from_xyzw(q[1], q[2], q[3], q[0]);
    if quat.length_squared() < RANK_EPSILON {
        return DMat3::IDENTITY;
    }
    DMat3::from_quat(quat.normalize())
}

/// The eigenvector of the largest eigenvalue of a symmetric 4×4, by cyclic
/// Jacobi rotations. Deterministic: a fixed sweep order and a fixed number of
/// sweeps, so the same input always gives the same answer bit for bit.
fn largest_eigenvector_4(matrix: [[f64; 4]; 4]) -> [f64; 4] {
    let mut a = matrix;
    let mut v = [[0.0f64; 4]; 4];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..24 {
        let off: f64 = (0..4)
            .flat_map(|p| ((p + 1)..4).map(move |q| (p, q)))
            .map(|(p, q)| a[p][q] * a[p][q])
            .sum();
        if off < 1e-30 {
            break;
        }
        for p in 0..4 {
            for q in (p + 1)..4 {
                if a[p][q] == 0.0 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                // A ← Jᵀ A J, in the two halves it factors into: the columns
                // first, then the rows of the result.
                let rotate_columns = |m: &mut [[f64; 4]; 4]| {
                    for row in m.iter_mut() {
                        let (kp, kq) = (row[p], row[q]);
                        row[p] = c * kp - s * kq;
                        row[q] = s * kp + c * kq;
                    }
                };
                rotate_columns(&mut a);
                let (row_p, row_q) = (a[p], a[q]);
                a[p] = std::array::from_fn(|k| c * row_p[k] - s * row_q[k]);
                a[q] = std::array::from_fn(|k| s * row_p[k] + c * row_q[k]);
                rotate_columns(&mut v);
            }
        }
    }
    let best = (0..4)
        .max_by(|&i, &j| a[i][i].total_cmp(&a[j][j]))
        .expect("four diagonal entries");
    [v[0][best], v[1][best], v[2][best], v[3][best]]
}
