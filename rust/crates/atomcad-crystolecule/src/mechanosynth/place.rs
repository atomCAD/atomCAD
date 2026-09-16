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
//! - **A candidate is offerable exactly when the step it produces replays.**
//!   The pattern checks — closed-world bonds and `deg` — run here as prunes in
//!   the assignment search and there as [`check_pattern`], which is the same
//!   function; the one exception is the *clicked* atom's own degree, which is a
//!   [`Refusal`] on the candidate rather than a dead branch, because "this host
//!   has 4 bonds and the operation needs 3" is the coverage report the editor
//!   exists to give. See `doc/design_mechanosynth_pattern_checks.md` §4.

use super::apply::{Contact, PatternMismatch, check_pattern, describe_nearest, worst_contact};
use super::fit::{rank_of, rigid_fit};
use super::scene::ToolBinding;
use super::schema::{
    MechanosynthError, OpLibrary, Operation, PATTERN_POSITION_EPSILON, Pattern, PatternAtom,
    PatternElement, Step,
};
use crate::atomic_constants::element_symbol;
use crate::atomic_structure::AtomicStructure;
use crate::guided_placement::{
    Hybridization, Sp2CandidateResult, Sp3CandidateResult, Sp3Case1Result, compute_sp1_candidates,
    compute_sp2_candidates, compute_sp3_candidates, detect_hybridization, gather_bond_directions,
};
use glam::{DMat3, DVec3};
use rustc_hash::FxHashMap;

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
    /// The closest pair of atoms this candidate would leave unbonded and
    /// touching, when it would leave any within the check's reach.
    ///
    /// Reported whether or not it blocks: *how close does this reaction come*
    /// is the measurement behind the block factor, and the one a library
    /// author needs when deciding whether to state a `clash` of their own.
    pub contact: Option<Contact>,
    /// Why this candidate cannot be committed, when it cannot.
    ///
    /// A candidate carrying one is kept — it must be previewable, since the
    /// reason is about the ghost the user is looking at — and is not offerable.
    /// See [`Refusal`] and [`Applicability::offerable`].
    pub refusal: Option<Refusal>,
}

/// Why a candidate cannot be committed.
///
/// There is deliberately **no bond variant**: a bond mismatch is pairwise, so
/// the assignment search prunes it and no candidate is ever produced. A degree
/// mismatch on a non-clicked atom is pruned the same way. Only the clicked
/// atom's own degree reaches a candidate, because that one is a report about the
/// atom the user chose rather than about the search.
///
/// A **clash**, by contrast, is a property of the whole placement rather than
/// of a pair of pattern atoms, so it cannot be pruned: it is known only once a
/// fit exists, and it is what tells the row's good orientation from its
/// flipped one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refusal {
    /// The clicked atom has `found` bonds where the role it was given states
    /// `expected`. Row-level by construction: the clicked atom is the same in
    /// every candidate of one call.
    Degree { expected: u32, found: usize },
    /// The step would leave an atom it places closer to an atom it does not
    /// bond to than the library's factor allows. Per **candidate**: a row's
    /// proper fit can be clean while its mirrored one lands in the bulk.
    Clash(Contact),
}

impl Refusal {
    /// The refusal in the words a row shows where its residual would be.
    pub fn reason(&self, op: &str) -> String {
        match self {
            Refusal::Degree { expected, found } => {
                format!("{op} needs a host with {expected} bond(s); the clicked atom has {found}")
            }
            Refusal::Clash(contact) => contact.reason(),
        }
    }
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
    let clash = library.clash_factor();

    // --- 1. the clicked atom's role, decided once -----------------------------
    let role = role_atom(operation, clicked_atom.atomic_number).ok_or_else(|| {
        MechanosynthError::NoRole {
            op: operation.name.clone(),
            element: clicked_element.clone(),
            accepted: accepted_elements(operation),
        }
    })?;

    // The clicked atom's own degree is the one pattern check the search does not
    // prune. A wrong element means the operation has nothing to say about this
    // atom, so the sweep drops it; a wrong *degree* on an atom the operation
    // would act on is a coverage report, so the search runs anyway and every
    // candidate it produces carries the refusal.
    let role_refusal = role.deg.and_then(|expected| {
        let found = clicked_atom.bonds.len();
        (found != expected as usize).then_some(Refusal::Degree { expected, found })
    });

    // --- 5. fallback: a one-atom pattern that needs an orientation ------------
    // Checked before the fit rather than after it, because the fit *would*
    // succeed — with `r = identity`, which is an orientation the library never
    // stated and the workpiece never justified. A candidate ranked first on a
    // residual of zero would then be committed by the one-click rule.
    if needs_derived_orientation(operation) {
        let candidates = fallback_candidates(
            workpiece,
            operation,
            role,
            clicked,
            clicked_pos,
            role_refusal,
        );
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
        let candidates = measure_contacts(workpiece, operation, clash, candidates);
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
        before: &operation.before,
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
                contact: None,
                refusal: role_refusal,
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

    let candidates = measure_contacts(workpiece, operation, clash, candidates);
    Ok((rank(workpiece, operation, candidates), stats))
}

/// Fills in every candidate's [`Contact`] and, where the factor blocks it, its
/// [`Refusal::Clash`].
///
/// The same predicate the replay runs ([`worst_contact`]), over the same match,
/// which is what keeps "a candidate is offerable iff the step it produces
/// replays" true of the steric rule. A candidate already refused for the
/// clicked atom's degree keeps that refusal: it is the row-level reason, and it
/// is the one a user can act on.
fn measure_contacts(
    workpiece: &AtomicStructure,
    op: &Operation,
    clash: f64,
    candidates: Vec<Candidate>,
) -> Vec<Candidate> {
    candidates
        .into_iter()
        .map(|mut candidate| {
            let matched: FxHashMap<i64, u32> = candidate.roles.iter().copied().collect();
            candidate.contact =
                worst_contact(workpiece, &op.before, &op.after, &candidate.step, &matched);
            if candidate.refusal.is_none()
                && let Some(contact) = candidate.contact.filter(|c| c.ratio < clash)
            {
                candidate.refusal = Some(Refusal::Clash(contact));
            }
            candidate
        })
        .collect()
}

// ============================================================================
// The role rule
// ============================================================================

/// The `before` atom the clicked atom plays: among the operation's **anchors**
/// whose element admits the click (`"*"` admits all), the one at the origin of
/// the operation's frame, else the smallest anchor id. `None` when no anchor
/// admits it, which means the operation does not act on this atom.
///
/// The anchors, not the whole `before` pattern. The old rule fell back to the
/// smallest *eligible* id, which made every atom of every pattern clickable —
/// frame atoms included — whenever the origin atom's element happened not to
/// admit the click, and a click on a terminator chlorine would be given the
/// `"*"` frame role of an operation that has nothing to do with chlorine. A
/// frame atom is by definition one the operation does not touch, and a click is
/// a statement about where the reaction should happen. See
/// `doc/design_mechanosynth_pattern_checks.md` §4.3.
fn role_atom(op: &Operation, atomic_number: i16) -> Option<&PatternAtom> {
    let eligible = || {
        op.anchor_atoms()
            .filter(move |atom| atom.element.matches(atomic_number))
    };
    eligible()
        .filter(|atom| atom.pos.length() <= PATTERN_POSITION_EPSILON)
        .min_by_key(|atom| atom.id)
        .or_else(|| eligible().min_by_key(|atom| atom.id))
}

/// The element symbols the operation's **anchors** accept, for the `NoRole`
/// message.
fn accepted_elements(op: &Operation) -> String {
    let mut symbols: Vec<String> = Vec::new();
    for atom in op.anchor_atoms() {
        let symbol = match atom.element {
            PatternElement::Any => "*".to_string(),
            PatternElement::Element(z) => element_symbol(z),
        };
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }
    if symbols.is_empty() {
        // An operation whose `before` names no atom with an anchor id: the
        // origin-convention warning already said so at load time, and here it
        // simply has nothing to be clicked on.
        return "nothing".to_string();
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
/// rather than the depth, and three prunes do all the work, applied at the same
/// point as each atom is considered:
///
/// - the candidate atom's bond count must be the `deg` its slot states — one
///   lookup, before the pairwise loop runs at all;
/// - a pair of assigned atoms must be as far apart in the workpiece as they are
///   in the pattern, to within twice the tolerance (each end may be off by one);
/// - and its bond to every already-assigned atom — **the role atom included** —
///   must be exactly what the pattern's closed-world bond list says.
///
/// The two new ones are O(1) apiece, so they make the search cheaper rather than
/// dearer: on a crowded neighbourhood the `deg` lookup rejects a scattered atom
/// outright and the bond check rejects one that merely happens to be at the
/// right distance. The role atom's own degree is deliberately *not* pruned here:
/// its mismatch is a [`Refusal`] on every candidate, not a dead branch.
struct Search<'a> {
    workpiece: &'a AtomicStructure,
    neighbourhood: &'a [u32],
    others: &'a [&'a PatternAtom],
    /// The whole `before` pattern, for its bond list.
    before: &'a Pattern,
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
            if pattern_atom
                .deg
                .is_some_and(|deg| atom.bonds.len() != deg as usize)
            {
                continue;
            }
            let fits = self.assigned.iter().all(|(other, other_id)| {
                let pattern_distance = pattern_atom.pos.distance(other.pos);
                let world_distance = self
                    .workpiece
                    .get_atom(*other_id)
                    .map(|a| a.position.distance(atom.position))
                    .unwrap_or(f64::INFINITY);
                if (world_distance - pattern_distance).abs() > self.slack {
                    return false;
                }
                self.bond_agrees(pattern_atom.id, other.id, atom_id, *other_id)
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

    /// Whether the workpiece agrees with the pattern about the bond between two
    /// pattern ids. An unlisted pair asserts "no bond", which is what makes
    /// `bridge` on an already bonded pair refuse itself with no special rule.
    fn bond_agrees(&self, a: i64, b: i64, a_atom: u32, b_atom: u32) -> bool {
        let key = (a.min(b), a.max(b));
        let expected = self
            .before
            .bonds
            .iter()
            .find(|bond| bond.key() == key)
            .map(|bond| bond.order);
        expected == self.workpiece.bond_order_between(a_atom, b_atom)
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

/// Whether an improper fit of this pattern says anything a proper one does not.
///
/// It does exactly when the pattern spans a plane: reflecting a point or a line
/// changes only the rotation that was undetermined anyway, so a "mirrored"
/// candidate there would be the same reaction wearing a different `r`.
fn mirror_fit_is_meaningful(local: &[DVec3]) -> bool {
    rank_of(local) >= 2
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
    refusal: Option<Refusal>,
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
                contact: None,
                refusal,
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
/// is left: **placeable before refused**, then by residual, then proper before
/// mirrored, then exact before approximate.
///
/// Refused last so that index 0 of an offerable row is always a candidate that
/// can be committed, and a row's default preview is never a blocked ghost. Two
/// candidates with the same after state place the same atoms and so have the
/// same contact, which is why the collapse never has to arbitrate between a
/// blocked and a clean one.
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
        a.refusal
            .is_some()
            .cmp(&b.refusal.is_some())
            .then(bucket(a.residual).cmp(&bucket(b.residual)))
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

/// A description of what a step will leave behind, canonically ordered so two
/// orders of the same set compare equal.
type AfterState = Vec<KeyEntry>;

/// One entry of an [`AfterState`]. `Ord` is derived only so a key can be put in
/// a canonical order; the order between variants means nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum KeyEntry {
    /// An atom the step adds or moves: where it lands, quantised, and what it
    /// ends up being.
    Placed {
        element: i64,
        x: i64,
        y: i64,
        z: i64,
    },
    /// An atom the step leaves where the workpiece has it, and what it ends up
    /// being — a kept position with a new element is an element swap.
    Kept { atom: Endpoint, element: i64 },
    /// An atom the step deletes.
    Deleted { atom: Endpoint },
    /// A bond the step adds, deletes or re-orders. `order` is the order the
    /// bond ends up with, and zero when the step deletes it.
    Bond { a: Endpoint, b: Endpoint, order: u8 },
}

/// How an atom is named inside a key.
///
/// By the workpiece atom it matched wherever the step has one — the identity is
/// exact, where a position is a quantised guess — and by where it lands only
/// when the step adds it and there is no workpiece atom to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Endpoint {
    Matched(u32),
    Lands(i64, i64, i64),
}

/// Every `after` atom, every bond the step changes, and the workpiece atoms the
/// step deletes.
///
/// An atom the step **adds or moves** is keyed by its placed position and
/// element: positions rather than transforms, because a symmetric pattern
/// reaches one after state by several transforms — the three cyclic
/// permutations of a tetrahedral frame are three rotations and one reaction.
///
/// An atom the step **keeps** is keyed by the workpiece atom it matched. A kept
/// atom does not move — `apply_matched` never snaps one to its pattern position
/// — so where the fit imagines it to be says nothing about the state the step
/// leaves behind, and quantising that imagined position only invites two
/// transforms that reach the same state to straddle a bucket boundary.
///
/// **Bonds, because atoms alone do not say where a bond went.** Keying a kept
/// atom by identity rather than by position is what makes that necessary: the
/// key is an unordered set, so two assignments of a symmetric frame keep the
/// same atoms and differ *only* in which of them the step bonds. `bridge` over
/// wildcard frame atoms is exactly that shape, and collapsing its assignments
/// would hand the user one candidate for two different reactions. The changed
/// bonds are enumerated the way [`preview_bonds`] enumerates them, so what a
/// candidate is keyed by and what it previews as cannot drift apart.
///
/// Deletions as well as placements, because a pure abstraction has no placed
/// atom to tell two candidates apart.
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
    // The atoms the step keeps exactly where they are. Everything else in
    // `after` is placed, and everything in `before` alone is deleted.
    let is_kept = |id: i64| {
        let (Some(before), Some(after)) = (op.before.atom(id), op.after.atom(id)) else {
            return false;
        };
        before.pos.distance(after.pos) <= PATTERN_POSITION_EPSILON
    };
    let name = |id: i64| match matched(id) {
        Some(atom_id) => Endpoint::Matched(atom_id),
        // Only an added atom has no workpiece atom to be named by.
        None => match op.after.atom(id) {
            Some(atom) => {
                let pos = candidate.step.place(atom.pos);
                Endpoint::Lands(quantise(pos.x), quantise(pos.y), quantise(pos.z))
            }
            None => Endpoint::Lands(i64::MIN, i64::MIN, i64::MIN),
        },
    };

    let mut key: AfterState = Vec::new();
    for atom in &op.after.atoms {
        let element = match atom.element {
            PatternElement::Element(z) => z as i64,
            // `"*"` on a kept id leaves the workpiece atom's element alone, so
            // that is the element this step ends up with.
            PatternElement::Any => matched(atom.id)
                .and_then(|atom_id| workpiece.get_atom(atom_id))
                .map_or(0, |a| a.atomic_number as i64),
        };
        if is_kept(atom.id) {
            key.push(KeyEntry::Kept {
                atom: name(atom.id),
                element,
            });
        } else {
            let pos = candidate.step.place(atom.pos);
            key.push(KeyEntry::Placed {
                element,
                x: quantise(pos.x),
                y: quantise(pos.y),
                z: quantise(pos.z),
            });
        }
    }

    for atom in &op.before.atoms {
        if op.after.has(atom.id) {
            continue;
        }
        key.push(KeyEntry::Deleted {
            atom: name(atom.id),
        });
    }

    let bond = |a: i64, b: i64, order: u8| {
        // Canonical within the pair, so one bond has one entry however the
        // pattern wrote its endpoints.
        let (a, b) = (name(a), name(b));
        KeyEntry::Bond {
            a: a.min(b),
            b: a.max(b),
            order,
        }
    };
    for changed in &op.after.bonds {
        let existing = op
            .before
            .bonds
            .iter()
            .find(|other| other.key() == changed.key());
        match existing {
            Some(before) if before.order == changed.order => continue,
            _ => key.push(bond(changed.a, changed.b, changed.order)),
        }
    }
    for gone in &op.before.bonds {
        if op.after.bonds.iter().any(|other| other.key() == gone.key()) {
            continue;
        }
        key.push(bond(gone.a, gone.b, 0));
    }

    key.sort_unstable();
    key
}

// ============================================================================
// Applicability: what can be done here
// ============================================================================

/// How far past the library's tolerance a fit is still worth *reporting*.
///
/// A library that states 0.05 Å reports misses out to 0.5 Å, which is the range
/// in which "this host is not an environment the library was calculated for" is
/// a useful thing to say; beyond it the pattern is simply somewhere else. A
/// constant rather than a per-library value until a library complains — see the
/// open question in `doc/design_mechanosynth_editor.md`.
pub const NEAR_MISS_FACTOR: f64 = 10.0;

/// What one library operation has to say about one clicked atom.
///
/// A row is **either** applicable (`candidates` non-empty, `near_miss` `None`)
/// **or** a near miss (`candidates` empty, `near_miss` `Some`) — never both, and
/// never neither. Keeping the over-gate fit in its own field rather than mixed
/// into `candidates` is what makes "a near miss cannot be committed" a
/// type-level fact instead of a filter every caller has to remember.
#[derive(Debug, Clone, PartialEq)]
pub struct Applicability {
    pub op: String,
    /// Candidates within the library tolerance, ranked; empty for a near miss.
    pub candidates: Vec<Candidate>,
    /// The best fit found *outside* the gate, kept so a near-miss row can be
    /// previewed and measured rather than merely counted.
    pub near_miss: Option<Candidate>,
    /// Best residual of `candidates`, or of `near_miss`.
    pub best_residual: f64,
    /// `best_residual <= tolerance`, i.e. this row is applicable.
    pub fits: bool,
    /// Every candidate of this row came from the bond-derived fallback.
    pub approximate: bool,
    /// What this row's tool has to say, when tools are wired and the operation
    /// is [`Method::Tip`](super::Method::Tip). `None` otherwise — with tools
    /// unwired no row carries a tool annotation, which is the modelling use of
    /// the editor exactly as it was before tools existed.
    pub tool: Option<ToolReadiness>,
}

/// Whether the tool a `tip` operation needs can perform it here and now.
///
/// A row whose tool is not ready is offered **below the rule with the near
/// misses, dimmed and unselectable** — for the same reason a near miss is not
/// selectable: it cannot be committed, because its tool side would not match.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolReadiness {
    /// The operation's tool type.
    pub tool_type: String,
    /// The state the bound tool is in, `None` when no molecule plays the type
    /// or the type carries no symbolic state.
    pub state: Option<String>,
    /// Whether the tool is bound, in the right state, and its tool side
    /// matches at its pose.
    pub ready: bool,
    /// Why not, in the words a row shows where the residual would be —
    /// *habst_tool is spent*, *no molecule tagged `probe` on the tools pin*.
    pub reason: Option<String>,
}

impl Applicability {
    /// Whether this row can be committed: it fits geometrically, at least one
    /// of its candidates carries no refusal, **and**, when the operation needs a
    /// tool, that tool is ready. The one question the offer list and the commit
    /// path both ask.
    ///
    /// A row is only as blocked as *all* of its candidates, which matters
    /// because the interesting rows are mixed — the edge-host donation of
    /// `doc/design_mechanosynth_pattern_checks.md` §5.2 has one good fit and one
    /// refused one, and the good one has to stay offerable.
    pub fn offerable(&self) -> bool {
        self.fits
            && self
                .candidates
                .iter()
                .any(|candidate| candidate.refusal.is_none())
            && self.tool.as_ref().is_none_or(|tool| tool.ready)
    }

    /// The reason this row cannot be committed, when **every** way of placing it
    /// is refused. `None` for a row with at least one placeable candidate, which
    /// stays above the rule and dims only the candidates that are refused.
    pub fn blocked(&self) -> Option<String> {
        if self.candidates.is_empty() {
            return None;
        }
        let first = self.candidates.first()?.refusal?;
        self.candidates
            .iter()
            .all(|candidate| candidate.refusal.is_some())
            .then(|| first.reason(&self.op))
    }

    /// The candidate a row is previewed by: its first real one, else its
    /// near miss. Never `None` — a row always holds one or the other.
    pub fn preview(&self) -> &Candidate {
        self.preview_at(0)
            .expect("an Applicability row holds candidates or a near miss")
    }

    /// The `index`-th candidate a row can be previewed by.
    ///
    /// A **near miss** has no `candidates` at all — its one rejected fit lives
    /// in `near_miss`, deliberately kept out of the placeable list — so for
    /// such a row only index 0 exists. Indexing `candidates` directly would
    /// make a near-miss row unpreviewable, which is the one thing it is for.
    pub fn preview_at(&self, index: usize) -> Option<&Candidate> {
        if self.candidates.is_empty() {
            return if index == 0 {
                self.near_miss.as_ref()
            } else {
                None
            };
        }
        self.candidates.get(index)
    }
}

/// One entry per library operation that has anything to say about `clicked`,
/// ranked; never an error, because "nothing applies here" is an answer.
///
/// This is a **wrapper over [`place`]**, not a second search: each row's
/// `candidates` are exactly what `place` returns for that operation and that
/// atom at `tolerance`, so the sweep and a single call cannot disagree. An
/// operation that fails at the gate is retried once at
/// `tolerance * NEAR_MISS_FACTOR` so a miss can report *how far* off it was —
/// a precalculated library's real limit is the set of environments its
/// generator enumerated, and that limit should be readable at the point of use.
///
/// Cost is one `place` per operation (two for a miss). Patterns hold a handful
/// of atoms and the assignment search is pruned, so a twenty-operation library
/// is one short sweep: cheap **per click**, and to be treated as too expensive
/// per mouse-move.
pub fn applicable_ops(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    clicked: u32,
    tolerance: f64,
    bindings: Option<&[ToolBinding]>,
) -> Vec<Applicability> {
    applicable_ops_where(workpiece, library, clicked, tolerance, bindings, &|_| true)
}

/// [`applicable_ops`], restricted to the operations `admit` accepts.
///
/// The predicate is the **caller's policy** — the editor passes "not muted"
/// (`doc/design_mechanosynth_op_muting.md`) — and this crate deliberately does
/// not know what a muted operation is. The restriction happens *before*
/// `place` is called, so a filtered sweep costs what the shorter library would.
///
/// A rejected operation is indistinguishable from one the library never had:
/// the caller that filters owes the user a statement that it did so, because
/// an empty list is otherwise read as a coverage report on the whole library.
pub fn applicable_ops_where(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    clicked: u32,
    tolerance: f64,
    bindings: Option<&[ToolBinding]>,
    admit: &dyn Fn(&Operation) -> bool,
) -> Vec<Applicability> {
    let clash = library.clash_factor();
    let mut rows: Vec<Applicability> = Vec::new();
    for operation in &library.ops {
        if !admit(operation) {
            continue;
        }
        let row = match place(workpiece, library, &operation.name, clicked, tolerance) {
            Ok(candidates) => {
                let best_residual = candidates[0].residual;
                Applicability {
                    op: operation.name.clone(),
                    approximate: candidates.iter().all(|candidate| candidate.approximate),
                    best_residual,
                    fits: true,
                    candidates,
                    near_miss: None,
                    tool: None,
                }
            }
            // The operation said no at the gate. Ask again with the gate
            // widened, and keep the best fit that is still outside the tight
            // one — the `> tolerance` filter is what keeps the two fields
            // mutually exclusive even in the hair's-breadth case where the
            // wider neighbourhood turns up an assignment the tight search
            // never ranged over.
            Err(_) => {
                let relaxed = place(
                    workpiece,
                    library,
                    &operation.name,
                    clicked,
                    tolerance * NEAR_MISS_FACTOR,
                );
                let Some(miss) = relaxed.ok().and_then(|candidates| {
                    candidates
                        .into_iter()
                        .find(|candidate| candidate.residual > tolerance)
                }) else {
                    continue;
                };
                Applicability {
                    op: operation.name.clone(),
                    candidates: Vec::new(),
                    best_residual: miss.residual,
                    approximate: miss.approximate,
                    fits: false,
                    near_miss: Some(miss),
                    tool: None,
                }
            }
        };
        // The tool check runs **only** for an operation that already fits the
        // clicked atom: asking a tool whether it could perform a reaction that
        // does not apply here has no answer worth showing, and the check costs
        // one nearest-atom match per row.
        let mut row = row;
        if row.fits {
            row.tool = tool_readiness(workpiece, operation, bindings, tolerance, clash);
        }
        rows.push(row);
    }

    // `place`'s own order with one key in front of it: an operation that fits
    // outranks one that merely came close, however close it came. A row whose
    // *tool* is not ready cannot be committed either, so it sorts with the near
    // misses rather than among the offers.
    let bucket = |residual: f64| (residual / RESIDUAL_RANK_EPSILON).round() as i64;
    rows.sort_by(|a, b| {
        b.offerable()
            .cmp(&a.offerable())
            .then(b.fits.cmp(&a.fits))
            .then(bucket(a.best_residual).cmp(&bucket(b.best_residual)))
            .then(a.preview().mirrored.cmp(&b.preview().mirrored))
            .then(a.approximate.cmp(&b.approximate))
            // Two rows can tie on all four; the op name keeps the list stable
            // across calls.
            .then_with(|| a.op.cmp(&b.op))
    });
    rows
}

/// Whether the tool `operation` needs is bound, in the right state, and shaped
/// the way its tool side expects at its parked pose.
///
/// One nearest-atom match of a few atoms, at a pose already solved — no search.
/// The state check comes first because *habst_tool is spent* is the message a
/// process author can act on, but the geometric check runs even when the label
/// agrees: the geometry is what a viewer sees and the label is the thing that
/// can be wrong.
fn tool_readiness(
    workpiece: &AtomicStructure,
    operation: &Operation,
    bindings: Option<&[ToolBinding]>,
    tolerance: f64,
    clash: f64,
) -> Option<ToolReadiness> {
    let bindings = bindings?;
    let tool_side = operation.tool.as_ref()?;
    let tool_type = tool_side.tool_type.clone();

    let Some(binding) = bindings
        .iter()
        .find(|binding| binding.tool_type == tool_type)
    else {
        return Some(ToolReadiness {
            reason: Some(format!("no molecule tagged `{tool_type}` on the tools pin")),
            tool_type,
            state: None,
            ready: false,
        });
    };

    if let Some(required) = &tool_side.from
        && binding.state.as_deref() != Some(required.as_str())
    {
        let found = binding
            .state
            .clone()
            .unwrap_or_else(|| "stateless".to_string());
        return Some(ToolReadiness {
            reason: Some(format!("{tool_type} is {found}, not {required}")),
            tool_type,
            state: binding.state.clone(),
            ready: false,
        });
    }

    let mut tool_step = Step::new(operation.name.clone(), binding.pose.t);
    tool_step.r = binding.pose.r;
    let mut matched: FxHashMap<i64, u32> = FxHashMap::default();
    for pattern_atom in &tool_side.before.atoms {
        let target = tool_step.place(pattern_atom.pos);
        let found = workpiece.nearest_unclaimed_atom(
            target,
            tolerance,
            pattern_atom.element.required_atomic_number(),
            |atom_id| matched.values().any(|claimed| *claimed == atom_id),
        );
        match found {
            Some(found) => {
                matched.insert(pattern_atom.id, found.atom_id);
            }
            None => {
                return Some(ToolReadiness {
                    reason: Some(format!(
                        "{tool_type}: {}",
                        describe_nearest(workpiece, target)
                    )),
                    tool_type,
                    state: binding.state.clone(),
                    ready: false,
                });
            }
        }
    }

    // The same second pass the replay runs, so a tool whose cargo bond is
    // missing is dimmed at placement exactly as it fails at replay. This path
    // does not go through `match_before` — it has its own nearest-atom loop at
    // the bound pose — which is why the predicate is a shared function rather
    // than a step inside that one.
    if let Some(mismatch) = check_pattern(workpiece, &tool_side.before, &matched, None) {
        return Some(ToolReadiness {
            reason: Some(format!("{tool_type}: {}", describe_tool_mismatch(mismatch))),
            tool_type,
            state: binding.state.clone(),
            ready: false,
        });
    }

    // And the steric rule, on the tool's own rewrite: a tool whose cargo would
    // land inside something is dimmed at placement exactly as it fails at
    // replay.
    if let Some(contact) = worst_contact(
        workpiece,
        &tool_side.before,
        &tool_side.after,
        &tool_step,
        &matched,
    )
    .filter(|contact| contact.ratio < clash)
    {
        return Some(ToolReadiness {
            reason: Some(format!("{tool_type}: {}", contact.reason())),
            tool_type,
            state: binding.state.clone(),
            ready: false,
        });
    }

    Some(ToolReadiness {
        tool_type,
        state: binding.state.clone(),
        ready: true,
        reason: None,
    })
}

/// A tool-side [`PatternMismatch`] in the words a dimmed row shows.
fn describe_tool_mismatch(mismatch: PatternMismatch) -> String {
    match mismatch {
        PatternMismatch::Degree {
            id,
            expected,
            found,
        } => format!("its atom {id} needs {expected} bond(s) and has {found}"),
        PatternMismatch::Bond {
            a,
            b,
            expected: Some(order),
            found: None,
        } => format!("the bond between its atoms {a} and {b} (order {order}) is missing"),
        PatternMismatch::Bond {
            a,
            b,
            expected: None,
            ..
        } => format!("its atoms {a} and {b} are bonded, and the operation needs them apart"),
        PatternMismatch::Bond {
            a,
            b,
            expected: Some(want),
            found: Some(got),
        } => format!("the bond between its atoms {a} and {b} is order {got}, not {want}"),
    }
}

// ============================================================================
// Previewing a candidate
// ============================================================================

/// What a preview atom is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhostKind {
    /// The step creates it.
    Added,
    /// The step removes it.
    Deleted,
    /// The step keeps it and puts it somewhere else.
    Moved,
    /// The step keeps it where it is and changes its element. Without this the
    /// preview of an element-swap operation would be empty, which reads as
    /// "nothing would happen".
    Changed,
}

/// One atom of a candidate's ghost preview, in workpiece coordinates.
///
/// Only the atoms a step *changes*: a kept atom — a frame atom, a host that
/// only gains a bond — has nothing to draw, and drawing it would put a ghost on
/// top of an atom that is already there.
#[derive(Debug, Clone, PartialEq)]
pub struct GhostAtom {
    pub kind: GhostKind,
    /// Where it ends up. For a deletion, where the atom is now.
    pub position: DVec3,
    /// Where it came from — the tail of a `Moved` atom's arrow. Equal to
    /// [`position`](Self::position) for the other two kinds.
    pub from: DVec3,
    pub atomic_number: i16,
}

/// What a step would do to one **bond**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhostBondKind {
    /// The step creates it.
    Added,
    /// The step removes it.
    Deleted,
    /// The step keeps it and changes its order.
    Changed,
}

/// One bond of a candidate's ghost preview, its endpoints in workpiece
/// coordinates.
///
/// Without these a **bond-only operation previews as nothing at all**, which
/// reads as a broken preview rather than as "this adds a bond". Several
/// operations in a real library are exactly that: `bridge` and `bridge_c` in
/// the silicon set have identical `before` and `after` atom lists and differ
/// only in that `after` carries the bond.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GhostBond {
    pub kind: GhostBondKind,
    pub from: DVec3,
    pub to: DVec3,
}

/// The ghost preview of one candidate: what the step would add, delete and
/// move, ready to be drawn over the workpiece.
///
/// A pure function of the workpiece, the operation and the candidate, so the
/// editor can preview a row of the offer list without applying anything — and
/// so a *near miss*, which must never be applied, can still be shown.
pub fn preview_atoms(
    workpiece: &AtomicStructure,
    op: &Operation,
    candidate: &Candidate,
) -> Vec<GhostAtom> {
    let matched = |pattern_id: i64| {
        candidate
            .roles
            .iter()
            .find(|(id, _)| *id == pattern_id)
            .and_then(|(_, atom_id)| workpiece.get_atom(*atom_id))
    };
    let mut ghosts = Vec::new();

    for atom in &op.after.atoms {
        let position = candidate.step.place(atom.pos);
        match op.before.atom(atom.id) {
            // Kept. A change of place or of element is visible; a bond change
            // is not something a ghost *atom* can show, and the row's badges
            // say what the operation is.
            Some(before) => {
                let Some(current) = matched(atom.id) else {
                    continue;
                };
                let element = element_of(atom.element, Some(current.atomic_number));
                if before.pos.distance(atom.pos) > PATTERN_POSITION_EPSILON {
                    ghosts.push(GhostAtom {
                        kind: GhostKind::Moved,
                        position,
                        from: current.position,
                        atomic_number: element,
                    });
                } else if element != current.atomic_number {
                    ghosts.push(GhostAtom {
                        kind: GhostKind::Changed,
                        position,
                        from: position,
                        atomic_number: element,
                    });
                }
            }
            None => ghosts.push(GhostAtom {
                kind: GhostKind::Added,
                position,
                from: position,
                atomic_number: element_of(atom.element, None),
            }),
        }
    }

    for atom in &op.before.atoms {
        if op.after.has(atom.id) {
            continue;
        }
        let Some(current) = matched(atom.id) else {
            continue;
        };
        ghosts.push(GhostAtom {
            kind: GhostKind::Deleted,
            position: current.position,
            from: current.position,
            atomic_number: current.atomic_number,
        });
    }

    ghosts
}

/// The bond half of a candidate's ghost preview.
///
/// Separate from [`preview_atoms`] rather than folded into it because the two
/// answer different questions and most callers want both; keeping them apart
/// leaves the atom list exactly what it was for consumers that only place a
/// popup by it.
///
/// **A bond-only operation has nothing in `preview_atoms` at all.** `bridge`
/// and `bridge_c` have identical `before` and `after` atom lists — same ids,
/// same positions, same elements — and differ only in that `after` carries the
/// bond. Without this function such a row previews as an empty scene, which
/// reads as a broken tool rather than as "this adds a bond".
pub fn preview_bonds(
    workpiece: &AtomicStructure,
    op: &Operation,
    candidate: &Candidate,
) -> Vec<GhostBond> {
    let matched = |pattern_id: i64| {
        candidate
            .roles
            .iter()
            .find(|(id, _)| *id == pattern_id)
            .and_then(|(_, atom_id)| workpiece.get_atom(*atom_id))
    };

    // Where an endpoint is *now*: a kept atom stays where the workpiece has it
    // (the engine never snaps one), so a deletion is drawn against the current
    // geometry.
    let current_position = |id: i64| matched(id).map(|atom| atom.position);
    // Where an endpoint *ends up*: an added or moved atom lands at `r · p + t`,
    // a kept one does not move. So an addition is drawn against the geometry
    // the step produces.
    let after_position = |id: i64| {
        let after = op.after.atom(id)?;
        match op.before.atom(id) {
            Some(before) if before.pos.distance(after.pos) <= PATTERN_POSITION_EPSILON => {
                current_position(id)
            }
            _ => Some(candidate.step.place(after.pos)),
        }
    };

    let mut bonds = Vec::new();

    for bond in &op.after.bonds {
        let existing = op
            .before
            .bonds
            .iter()
            .find(|other| other.key() == bond.key());
        let kind = match existing {
            None => GhostBondKind::Added,
            Some(before) if before.order != bond.order => GhostBondKind::Changed,
            Some(_) => continue,
        };
        if let (Some(from), Some(to)) = (after_position(bond.a), after_position(bond.b)) {
            bonds.push(GhostBond { kind, from, to });
        }
    }

    for bond in &op.before.bonds {
        if op.after.bonds.iter().any(|other| other.key() == bond.key()) {
            continue;
        }
        if let (Some(from), Some(to)) = (current_position(bond.a), current_position(bond.b)) {
            bonds.push(GhostBond {
                kind: GhostBondKind::Deleted,
                from,
                to,
            });
        }
    }

    bonds
}

/// A pattern slot's element, with `"*"` falling back to whatever the workpiece
/// atom is. `0` when neither says — impossible for an added atom, which the
/// parser requires to name an element.
fn element_of(slot: PatternElement, current: Option<i16>) -> i16 {
    match slot {
        PatternElement::Element(z) => z,
        PatternElement::Any => current.unwrap_or(0),
    }
}
