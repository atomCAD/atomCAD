//! Turning "this operation, this clicked atom" into placed steps.
//!
//! [`apply_step`](super::apply_step) is the second half of the engine: given a
//! step, it rewrites the workpiece. This is the first half — given a click, it
//! produces the step. A pure function over an [`AtomicStructure`] and an
//! [`OpLibrary`], so it is testable without a node, a network or a viewport.
//!
//! Four rules carry the design; each is easy to erode and each has tests:
//!
//! - **The clicked atom's role is fixed before the search runs.** An atom often
//!   admits several `before` slots — both ends of a dimerization are bare Si,
//!   and a `"*"` slot admits anything. Offering the roles as candidates beside
//!   the orientation candidates would turn one click into a dialog, so the role
//!   is decided by rule (`role_atom`) and **never revisited**: a fit that
//!   fails in the assigned role is an error naming that role, not a retry in
//!   another one. A silent role change would place the reaction on a different
//!   atom from the one the user clicked.
//! - **Orientation comes from the operation, not from the application.** A
//!   one-atom `before` pattern carries no orientation; a library states it by
//!   listing the host's bonded neighbours as `"*"` *frame atoms* that appear
//!   unchanged in `after`, and the fit then recovers the rotation exactly.
//!   Deriving the direction from the host's bonds survives only as the
//!   fallback (`free_directions`) for libraries that name no frame atoms, and
//!   every candidate it produces is flagged [`Candidate::approximate`].
//! - **The tolerance never reaches a coordinate.** It decides whether a fit is
//!   accepted; the step it produces places atoms at exactly `r · p + t`. So the
//!   result is exact iff `(r, t)` is exact, which is what
//!   [`Candidate::residual`] reports and what the editor shows as a chip. See
//!   `doc/design_mechanosynth_editor.md` §Exactness.
//! - **Two candidates are one when they produce the same after state**, not
//!   when they share `(r, t)`. Three tetrahedral `"*"` frame atoms around a
//!   fixed host admit six assignments and six transforms, but one reaction; two
//!   dimerization partners are two reactions and stay two candidates.

use super::apply::describe_nearest;
use super::schema::{
    MechanosynthError, OpLibrary, Operation, PATTERN_POSITION_EPSILON, PatternAtom, PatternElement,
    Step,
};
use crate::atomic_constants::element_symbol;
use crate::atomic_structure::AtomicStructure;
use crate::guided_placement::{
    Hybridization, Sp2CandidateResult, Sp3CandidateResult, Sp3Case1Result, compute_sp1_candidates,
    compute_sp2_candidates, compute_sp3_candidates, detect_hybridization, gather_bond_directions,
};
use glam::{DMat3, DQuat, DVec3};

/// A fit at or below this max per-atom residual (Å) counts as *exact*: it
/// reproduces the coordinates a generator would have written, to the 1e-6 Å the
/// files round to and well beyond. The generator's own congruence threshold.
pub const EXACT_FIT_RESIDUAL: f64 = 1e-4;

/// Two placements whose after states agree to this distance (Å) are one
/// candidate. Far below any tolerance and far above the ~1e-12 Å a congruent
/// fit leaves behind.
const SAME_AFTER_STATE_EPSILON: f64 = 1e-6;

/// Two residuals this close (Å) are not a ranking signal — they are the same
/// fit seen through floating-point noise. Without the bucket, two exact fits at
/// 1e-16 and 3e-16 would order by which one the arithmetic happened to favour,
/// and "proper before mirrored" would never get a say.
pub const RESIDUAL_RANK_EPSILON: f64 = 1e-6;

/// Below this the centred pattern is treated as rank-deficient along a
/// direction — a single point, or a collinear pair. Å.
const RANK_EPSILON: f64 = 1e-9;

/// One way of placing an operation at the clicked atom.
///
/// Every candidate replays: [`apply_step`](super::apply_step) with
/// [`Candidate::step`] and the same tolerance succeeds on the same workpiece,
/// and the atoms it *matches* are exactly [`Candidate::roles`]. Not the atoms it
/// *touches*: that list is derived from effect
/// ([`StepEffect::touched`](super::StepEffect::touched)), so a frame atom is a
/// role and is not touched, and a deletion's bonded neighbour is touched and is
/// not a role.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The placed step: operation name, `r` and `t`. Metadata is empty — the
    /// editor copies it from the neighbouring step, which is a question about
    /// the build, not about the geometry.
    pub step: Step,
    /// The `before` pattern id the clicked atom plays. The same on every
    /// candidate of one call, because the role is decided before the search.
    pub role: i64,
    /// Pattern id → workpiece atom id, frame atoms included.
    pub roles: Vec<(i64, u32)>,
    /// Max per-atom distance of the fit, Å. The **max**, not the RMS: one atom
    /// badly placed is a wrong placement however well the others agree.
    pub residual: f64,
    /// `residual < EXACT_FIT_RESIDUAL`.
    ///
    /// A bond-derived fallback candidate is exact *by residual* — its one
    /// `before` atom is the clicked atom — and still [`approximate`](Self::approximate)
    /// in orientation. The two flags answer different questions.
    pub exact: bool,
    /// `det r < 0`: the fit is a mirrored one. Dropped for a `chiral` operation.
    pub mirrored: bool,
    /// The orientation was derived from the host's bonds because the operation
    /// named no frame atoms — coordinates from the application rather than from
    /// the library.
    pub approximate: bool,
}

/// What the assignment search did, for the pruning-regression test. A pruning
/// bug shows up here as a number rather than as a slow test.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlaceStats {
    /// Workpiece atoms the search ranged over.
    pub neighbourhood: usize,
    /// Partial assignments visited, complete ones included. The role atom's own
    /// assignment is not counted — the search never chooses it.
    pub assignments_visited: u64,
}

/// Places `op` at the clicked workpiece atom.
///
/// `tolerance` is the caller's: the replayer passes
/// [`resolve_tolerance(library)`](super::resolve_tolerance), and it is a
/// parameter so tests can vary it. It gates acceptance and nothing else — no
/// coordinate this function produces depends on its value.
///
/// `Ok` always holds at least one candidate, ranked best first. Every way of
/// ending with none is an `Err`, so the diagnostic travels with the failure
/// instead of beside an empty list.
pub fn place(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    op: &str,
    clicked: u32,
    tolerance: f64,
) -> Result<Vec<Candidate>, MechanosynthError> {
    place_with_stats(workpiece, library, op, clicked, tolerance).map(|(candidates, _)| candidates)
}

/// [`place`], and what the assignment search cost. Test-facing.
pub fn place_with_stats(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    op: &str,
    clicked: u32,
    tolerance: f64,
) -> Result<(Vec<Candidate>, PlaceStats), MechanosynthError> {
    let operation = library
        .get(op)
        .ok_or_else(|| MechanosynthError::UnknownOp {
            library: library.file.clone(),
            op: op.to_string(),
        })?;
    let clicked_atom = workpiece
        .get_atom(clicked)
        .ok_or(MechanosynthError::NoSuchAtom { atom_id: clicked })?;
    let clicked_pos = clicked_atom.position;
    let clicked_element = element_symbol(clicked_atom.atomic_number);

    // --- 1. the clicked atom's role, decided once -----------------------------
    let role = role_atom(operation, clicked_atom.atomic_number).ok_or_else(|| {
        MechanosynthError::NoRole {
            op: operation.name.clone(),
            element: clicked_element.clone(),
            accepted: accepted_elements(operation),
        }
    })?;

    // --- 5. fallback: a one-atom pattern that needs an orientation ------------
    // Checked before the fit rather than after it, because the fit *would*
    // succeed — with `r = identity`, which is an orientation the library never
    // stated and the workpiece never justified. A candidate ranked first on a
    // residual of zero would then be committed by the one-click rule.
    if needs_derived_orientation(operation) {
        let candidates = fallback_candidates(workpiece, operation, role, clicked, clicked_pos);
        if candidates.is_empty() {
            return Err(MechanosynthError::NoPlacement {
                op: operation.name.clone(),
                role: role.id,
                element: clicked_element,
                nearest: "the operation names no frame atoms and the clicked atom has no free \
                          bonding direction, so no orientation can be derived"
                    .to_string(),
            });
        }
        return Ok((
            rank(workpiece, operation, candidates),
            PlaceStats::default(),
        ));
    }

    // --- 2. neighbourhood -----------------------------------------------------
    let extent = operation
        .before
        .atoms
        .iter()
        .map(|atom| atom.pos.distance(role.pos))
        .fold(0.0, f64::max);
    let neighbourhood = workpiece.get_atoms_in_radius(&clicked_pos, extent + tolerance);

    // --- 3. assignment search -------------------------------------------------
    let others: Vec<&PatternAtom> = operation
        .before
        .atoms
        .iter()
        .filter(|atom| atom.id != role.id)
        .collect();
    let mut search = Search {
        workpiece,
        neighbourhood: &neighbourhood,
        others: &others,
        slack: 2.0 * tolerance,
        assigned: vec![(role, clicked)],
        complete: Vec::new(),
        visited: 0,
    };
    search.descend(0);
    let Search {
        complete, visited, ..
    } = search;
    let stats = PlaceStats {
        neighbourhood: neighbourhood.len(),
        assignments_visited: visited,
    };

    // --- 4. fit ---------------------------------------------------------------
    let mut candidates = Vec::new();
    for assignment in &complete {
        let local: Vec<DVec3> = assignment.iter().map(|(atom, _)| atom.pos).collect();
        let world: Vec<DVec3> = assignment
            .iter()
            .map(|(_, id)| {
                workpiece
                    .get_atom(*id)
                    .expect("assigned atom exists")
                    .position
            })
            .collect();
        // A rank-deficient pattern — one atom, or a collinear pair — leaves the
        // rotation about the deficient direction undetermined. Its mirror image
        // differs only in that undetermined part, so only the proper fit is
        // offered; `mirror_fit_is_meaningful` is where that is decided.
        let mirrors: &[bool] = if operation.chiral || !mirror_fit_is_meaningful(&local) {
            &[false]
        } else {
            &[false, true]
        };
        for &mirrored in mirrors {
            let Some(fit) = rigid_fit(&local, &world, mirrored) else {
                continue;
            };
            if fit.residual > tolerance {
                continue;
            }
            candidates.push(Candidate {
                step: Step {
                    r: fit.r,
                    ..Step::new(&operation.name, fit.t)
                },
                role: role.id,
                roles: assignment.iter().map(|(atom, id)| (atom.id, *id)).collect(),
                residual: fit.residual,
                exact: fit.residual < EXACT_FIT_RESIDUAL,
                mirrored,
                approximate: false,
            });
        }
    }

    if candidates.is_empty() {
        return Err(MechanosynthError::NoPlacement {
            op: operation.name.clone(),
            role: role.id,
            element: clicked_element,
            nearest: missing_partner(
                workpiece,
                operation,
                role,
                clicked,
                clicked_pos,
                &neighbourhood,
            ),
        });
    }

    Ok((rank(workpiece, operation, candidates), stats))
}

// ============================================================================
// The role rule
// ============================================================================

/// The `before` atom the clicked atom plays, by the rule in
/// `doc/design_mechanosynth_editor.md` §Decisions: among the atoms whose element
/// admits the click (`"*"` admits all), the one at the origin of the operation's
/// frame, else the one with the smallest id.
///
/// The origin clause is what makes "click the atom the operation acts on" the
/// whole instruction for a library that follows the origin convention. The
/// smallest-id clause is for the libraries that do not.
fn role_atom(op: &Operation, atomic_number: i16) -> Option<&PatternAtom> {
    let eligible = || {
        op.before
            .atoms
            .iter()
            .filter(move |atom| atom.element.matches(atomic_number))
    };
    eligible()
        .filter(|atom| atom.pos.length() <= PATTERN_POSITION_EPSILON)
        .min_by_key(|atom| atom.id)
        .or_else(|| eligible().min_by_key(|atom| atom.id))
}

/// The element symbols the `before` pattern accepts, for the `NoRole` message.
fn accepted_elements(op: &Operation) -> String {
    let mut symbols: Vec<String> = Vec::new();
    for atom in &op.before.atoms {
        let symbol = match atom.element {
            PatternElement::Any => "*".to_string(),
            PatternElement::Element(z) => element_symbol(z),
        };
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }
    symbols.join(", ")
}

// ============================================================================
// The assignment search
// ============================================================================

/// Recursive assignment of the non-role `before` atoms to distinct
/// neighbourhood atoms.
///
/// Patterns hold a handful of atoms, so the cost is in the branching factor
/// rather than the depth, and one prune does all the work: a pair of assigned
/// atoms must be as far apart in the workpiece as they are in the pattern, to
/// within twice the tolerance (each end may be off by one tolerance).
struct Search<'a> {
    workpiece: &'a AtomicStructure,
    neighbourhood: &'a [u32],
    others: &'a [&'a PatternAtom],
    slack: f64,
    assigned: Vec<(&'a PatternAtom, u32)>,
    complete: Vec<Vec<(&'a PatternAtom, u32)>>,
    visited: u64,
}

impl<'a> Search<'a> {
    fn descend(&mut self, depth: usize) {
        if depth == self.others.len() {
            self.complete.push(self.assigned.clone());
            return;
        }
        let pattern_atom = self.others[depth];
        for &atom_id in self.neighbourhood {
            if self.assigned.iter().any(|(_, id)| *id == atom_id) {
                continue;
            }
            let Some(atom) = self.workpiece.get_atom(atom_id) else {
                continue;
            };
            if !pattern_atom.element.matches(atom.atomic_number) {
                continue;
            }
            let fits = self.assigned.iter().all(|(other, other_id)| {
                let pattern_distance = pattern_atom.pos.distance(other.pos);
                let world_distance = self
                    .workpiece
                    .get_atom(*other_id)
                    .map(|a| a.position.distance(atom.position))
                    .unwrap_or(f64::INFINITY);
                (world_distance - pattern_distance).abs() <= self.slack
            });
            if !fits {
                continue;
            }
            self.visited += 1;
            self.assigned.push((pattern_atom, atom_id));
            self.descend(depth + 1);
            self.assigned.pop();
        }
    }
}

/// The tail of a `NoPlacement` message: which `before` atom the search could not
/// satisfy, and what is nearest of that element.
///
/// The unsatisfiable atom is the one with the fewest element-compatible atoms in
/// the neighbourhood — the binding constraint, and on a failed search usually
/// the only one with none.
fn missing_partner(
    workpiece: &AtomicStructure,
    op: &Operation,
    role: &PatternAtom,
    clicked: u32,
    clicked_pos: DVec3,
    neighbourhood: &[u32],
) -> String {
    let scarcest = op
        .before
        .atoms
        .iter()
        .filter(|atom| atom.id != role.id)
        .min_by_key(|atom| {
            neighbourhood
                .iter()
                .filter(|&&id| id != clicked)
                .filter(|&&id| {
                    workpiece
                        .get_atom(id)
                        .is_some_and(|a| atom.element.matches(a.atomic_number))
                })
                .count()
        });
    let Some(scarcest) = scarcest else {
        // A one-atom pattern always fits, so the search cannot have failed for
        // want of a partner; the fit residual is what rejected it.
        return describe_nearest(workpiece, clicked_pos);
    };
    let wanted = match scarcest.element {
        PatternElement::Any => "another atom".to_string(),
        PatternElement::Element(z) => element_symbol(z),
    };
    let nearest = workpiece
        .atoms_values()
        .filter(|atom| atom.id != clicked)
        .filter(|atom| scarcest.element.matches(atom.atomic_number))
        .map(|atom| atom.position.distance(clicked_pos))
        .fold(f64::INFINITY, f64::min);
    if nearest.is_finite() {
        format!(
            "no {wanted} for before atom {} at {:.2} Å from the clicked atom; nearest {wanted} is at {nearest:.2} Å",
            scarcest.id,
            scarcest.pos.distance(role.pos),
        )
    } else {
        format!(
            "the workpiece has no {wanted} for before atom {}",
            scarcest.id
        )
    }
}

// ============================================================================
// The rigid fit
// ============================================================================

struct Fit {
    r: DMat3,
    t: DVec3,
    residual: f64,
}

/// Whether an improper fit of this pattern says anything a proper one does not.
///
/// It does exactly when the pattern spans a plane: reflecting a point or a line
/// changes only the rotation that was undetermined anyway, so a "mirrored"
/// candidate there would be the same reaction wearing a different `r`.
fn mirror_fit_is_meaningful(local: &[DVec3]) -> bool {
    rank_of(local) >= 2
}

/// 0 for a single point (or coincident points), 1 for a collinear set, 2 for
/// anything that spans a plane. Distinguishing 2 from 3 is not needed: both
/// determine a rotation.
fn rank_of(points: &[DVec3]) -> usize {
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
fn rigid_fit(local: &[DVec3], world: &[DVec3], mirrored: bool) -> Option<Fit> {
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

// ============================================================================
// The bond-derived fallback
// ============================================================================

/// Whether this operation's orientation has to be derived from the workpiece.
///
/// True only for a one-atom `before` pattern whose `after` puts an atom
/// somewhere other than on the clicked atom: one point fixes a translation and
/// nothing else, so with anything off the origin to place, the rotation is
/// undetermined. An abstraction or an element swap has nothing to orient and
/// stays an exact one-atom fit.
fn needs_derived_orientation(op: &Operation) -> bool {
    let [only] = &op.before.atoms[..] else {
        return false;
    };
    op.after
        .atoms
        .iter()
        .any(|atom| atom.pos.distance(only.pos) > PATTERN_POSITION_EPSILON)
}

/// One candidate per free bonding direction of the clicked atom, each with the
/// operation's local `+z` taken to that direction.
///
/// Labelled `approximate` because the coordinates now come from the
/// application's idea of where a bond would go, not from the library: measured
/// on Si(100) this is 4.3° off on a reconstructed dimer atom, which moves the
/// added atom by 0.15–0.17 Å. The cure is a library that names frame atoms, not
/// a better guess here.
fn fallback_candidates(
    workpiece: &AtomicStructure,
    op: &Operation,
    role: &PatternAtom,
    clicked: u32,
    clicked_pos: DVec3,
) -> Vec<Candidate> {
    free_directions(workpiece, clicked)
        .into_iter()
        .map(|direction| {
            let r = frame_taking_z_to(direction);
            Candidate {
                step: Step {
                    r,
                    ..Step::new(&op.name, clicked_pos - r * role.pos)
                },
                role: role.id,
                roles: vec![(role.id, clicked)],
                residual: 0.0,
                exact: true,
                mirrored: false,
                approximate: true,
            }
        })
        .collect()
}

/// The directions in which the clicked atom could still take a bond, from the
/// guided-placement geometry the add-atom tool uses.
///
/// Empty when the atom is saturated, and empty when the direction is a
/// continuum rather than a set — a bare atom, or a single bond with no dihedral
/// reference. There is nothing to offer there, and offering an arbitrary sample
/// of a ring would be a guess dressed as a candidate.
fn free_directions(workpiece: &AtomicStructure, atom_id: u32) -> Vec<DVec3> {
    let Some(atom) = workpiece.get_atom(atom_id) else {
        return Vec::new();
    };
    let anchor = atom.position;
    let bonds = gather_bond_directions(workpiece, atom);
    let dots = match detect_hybridization(workpiece, atom_id, None) {
        Hybridization::Sp3 => match compute_sp3_candidates(workpiece, atom_id, anchor, &bonds, 1.0)
        {
            Sp3CandidateResult::Dots(dots)
            | Sp3CandidateResult::Case1(Sp3Case1Result::FixedDots(dots)) => dots,
            Sp3CandidateResult::Case1(Sp3Case1Result::FreeRing { .. }) => Vec::new(),
        },
        Hybridization::Sp2 => {
            match compute_sp2_candidates(workpiece, atom_id, anchor, &bonds, 1.0) {
                Sp2CandidateResult::Dots(dots) => dots,
                Sp2CandidateResult::FreeRing { .. } => Vec::new(),
            }
        }
        Hybridization::Sp1 => compute_sp1_candidates(anchor, &bonds, 1.0),
    };
    dots.into_iter()
        .map(|dot| (dot.position - anchor).normalize())
        .collect()
}

/// A proper rotation taking local `+z` to `direction`. The azimuth is free, so
/// it is pinned deterministically — a candidate that moved with an unrelated
/// edit would be worse than an arbitrary one.
fn frame_taking_z_to(direction: DVec3) -> DMat3 {
    let z = direction.normalize();
    let helper = if z.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let x = helper.cross(z).normalize();
    let y = z.cross(x);
    DMat3::from_cols(x, y, z)
}

/// Collapses the candidates that produce the same after state, then orders what
/// is left: by residual, then proper before mirrored, then exact before
/// approximate.
fn rank(workpiece: &AtomicStructure, op: &Operation, candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut kept: Vec<(AfterState, Candidate)> = Vec::new();
    for candidate in candidates {
        let key = after_state_key(workpiece, op, &candidate);
        match kept.iter_mut().find(|(existing, _)| *existing == key) {
            // Same reaction, two transforms. Keep the one a user would rather
            // be handed: proper over mirrored first — a symmetric frame admits
            // both and the proper one is the honest description — then the
            // tighter fit.
            Some((_, existing)) => {
                let better = (candidate.mirrored, candidate.approximate)
                    < (existing.mirrored, existing.approximate)
                    || ((candidate.mirrored, candidate.approximate)
                        == (existing.mirrored, existing.approximate)
                        && candidate.residual < existing.residual);
                if better {
                    *existing = candidate;
                }
            }
            None => kept.push((key, candidate)),
        }
    }
    let mut candidates: Vec<Candidate> = kept.into_iter().map(|(_, c)| c).collect();
    let bucket = |residual: f64| (residual / RESIDUAL_RANK_EPSILON).round() as i64;
    candidates.sort_by(|a, b| {
        bucket(a.residual)
            .cmp(&bucket(b.residual))
            .then(a.mirrored.cmp(&b.mirrored))
            .then(a.approximate.cmp(&b.approximate))
            // Two genuinely distinct reactions can tie on all three — the two
            // partners of a dimerization do. Ordering them by where they land
            // keeps the list stable across calls.
            .then_with(|| {
                a.step
                    .t
                    .to_array()
                    .iter()
                    .zip(b.step.t.to_array().iter())
                    .map(|(x, y)| x.total_cmp(y))
                    .find(|o| o.is_ne())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    candidates
}

/// A quantised description of what a step will leave behind, canonically
/// ordered so two orders of the same set compare equal.
type AfterState = Vec<(i64, i64, i64, i64)>;

/// Every `after` atom's placed position and element, plus the workpiece atoms
/// the step deletes.
///
/// Positions rather than transforms, because a symmetric pattern reaches one
/// after state by several transforms — the three cyclic permutations of a
/// tetrahedral frame are three rotations and one reaction. Deletions as well as
/// placements, because a pure abstraction has no placed atom to tell two
/// candidates apart.
fn after_state_key(
    workpiece: &AtomicStructure,
    op: &Operation,
    candidate: &Candidate,
) -> AfterState {
    let quantise = |v: f64| (v / SAME_AFTER_STATE_EPSILON).round() as i64;
    let matched = |pattern_id: i64| {
        candidate
            .roles
            .iter()
            .find(|(id, _)| *id == pattern_id)
            .map(|(_, atom_id)| *atom_id)
    };
    let mut key: AfterState = Vec::new();
    for atom in &op.after.atoms {
        let pos = candidate.step.place(atom.pos);
        let element = match atom.element {
            PatternElement::Element(z) => z as i64,
            // `"*"` on a kept id leaves the workpiece atom's element alone, so
            // that is the element this step ends up with.
            PatternElement::Any => matched(atom.id)
                .and_then(|atom_id| workpiece.get_atom(atom_id))
                .map_or(0, |a| a.atomic_number as i64),
        };
        key.push((element, quantise(pos.x), quantise(pos.y), quantise(pos.z)));
    }
    for atom in &op.before.atoms {
        if op.after.has(atom.id) {
            continue;
        }
        // Deleted workpiece ids, tagged out of the element range so a deletion
        // can never collide with a placement.
        key.push((-1, matched(atom.id).map_or(-1, i64::from), 0, 0));
    }
    key.sort_unstable();
    key
}
