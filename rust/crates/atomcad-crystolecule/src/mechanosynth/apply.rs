//! Applying build steps to a workpiece.
//!
//! Matching is nearest-atom-within-tolerance on position and element, using the
//! spatial grid [`AtomicStructure`] already maintains, so a step costs O(pattern
//! size). Position and element find the atoms; **bonds and bond counts then
//! verify that the atoms found are the ones the pattern was written about**.
//!
//! That second pass is [`check_pattern`], and it is one function with three
//! call sites — the replay's [`match_before`] (target side and tool side alike)
//! and the placement engine's two paths — which is what makes "a candidate is
//! offerable if and only if the step it produces replays" a fact rather than a
//! discipline. A script whose bonds do not hold was generated against a
//! different workpiece, and saying so is the whole point: atomCAD is for
//! atomically precise manufacturing, and a workpiece whose bond model is wrong
//! is not a model of anything.
//!
//! A third pass, [`worst_contact`], asks where the step's atoms actually land:
//! **two atoms not bonded to each other must not be closer than a fraction of
//! their covalent-radius sum.** It is shared the same way, and it too runs
//! before the first mutation, because [`apply_matched`] is infallible by design
//! and there is nothing to roll back. See
//! `doc/design_mechanosynth_pattern_checks.md` §2 and §5.

use super::schema::{
    BondMismatch, BuildScript, CLASH_SEARCH_RADIUS, Clash, DEFAULT_TOLERANCE, DegreeMismatch,
    MechanosynthError, NoMatch, OpLibrary, Operation, PATTERN_POSITION_EPSILON, Pattern,
    PatternElement, Step,
};
use crate::atomic_constants::{ATOM_INFO, DEFAULT_ATOM_INFO, element_symbol};
use crate::atomic_structure::{AtomicStructure, BondReference};
use glam::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};

fn pattern_element_symbol(element: PatternElement) -> String {
    match element {
        PatternElement::Any => "*".to_string(),
        PatternElement::Element(z) => element_symbol(z),
    }
}

/// The tolerance in force for a replay: the library's, else
/// [`DEFAULT_TOLERANCE`]. There is exactly one for a whole replay.
///
/// A build script's own `tolerance` used to win over the library's. It no
/// longer does: a build script is an array of records on a wire now, and an
/// array has no header to carry a second value. See
/// `doc/design_mechanosynth_editor.md`.
pub fn resolve_tolerance(library: &OpLibrary) -> f64 {
    library.tolerance.unwrap_or(DEFAULT_TOLERANCE)
}

/// How many steps `step` asks for: `k` means "the first `k` steps applied",
/// a negative `step` means "all", and anything past the end clamps.
pub fn steps_applied(step: i32, step_count: usize) -> usize {
    if step < 0 {
        step_count
    } else {
        (step as usize).min(step_count)
    }
}

/// What one step did to the workpiece, in atom ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepEffect {
    /// The atoms this step **changed** and left in place. An atom is in the
    /// list when the step
    ///
    /// - added it,
    /// - moved it,
    /// - changed its element (compared against the workpiece's *current*
    ///   element, not against the `before` pattern's slot),
    /// - added, deleted or re-ordered a bond it is an endpoint of, or
    /// - deleted an atom it was bonded to.
    ///
    /// Deleted atoms are not in the list — they no longer exist — but their
    /// bonded neighbours stand in for them, so a pure abstraction still points
    /// at the site it acted on. This is what [`HighlightTags::current`] is
    /// painted on.
    ///
    /// **Derived from effect, not from pattern membership.** Listing every id
    /// present in both patterns would light up a donation's frame atoms — the
    /// host's bonded neighbours, named only to fix the orientation — on every
    /// step. Those fail every clause above by construction, so they drop out
    /// with no flag in the file and no exclusion pass here, while the reacting
    /// atoms of every existing operation pass one. See
    /// `doc/design_mechanosynth_editor.md`.
    pub touched: Vec<u32>,
    /// The atoms this step **created** — the ids only `after` names. A subset
    /// of `touched`, split out because "which layer built this atom" is a
    /// question only about creation: an atom a step merely *moved* belongs to
    /// the layer that made it, not to the one that nudged it.
    pub added: Vec<u32>,
}

/// Applies one step to `workpiece` in place.
///
/// `step_number` is 1-based and appears in the failure message only.
///
/// `tolerance` and `clash` are the two values a library states and a replay
/// resolves once — [`resolve_tolerance`] and
/// [`OpLibrary::clash_factor`](super::OpLibrary::clash_factor). They are
/// parameters rather than a borrowed library so that a caller with a pattern
/// and no file, and every test, can vary them.
pub fn apply_step(
    workpiece: &mut AtomicStructure,
    op: &Operation,
    step: &Step,
    step_number: usize,
    tolerance: f64,
    clash: f64,
) -> Result<StepEffect, MechanosynthError> {
    let matched = match_before(
        workpiece,
        &op.name,
        &op.before,
        step,
        step_number,
        tolerance,
        None,
    )?;
    verify_clash(
        workpiece,
        &op.name,
        &op.before,
        &op.after,
        &matched,
        step,
        step_number,
        clash,
        None,
    )?;
    Ok(apply_matched(
        workpiece, &op.before, &op.after, step, matched,
    ))
}

/// One way a matched pattern disagrees with the workpiece it matched.
///
/// Positional rather than named, so the one caller that has a step to name
/// ([`match_before`]) turns it into an error while the two that do not — the
/// placement engine's search and its tool check — turn it into a prune or a
/// refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PatternMismatch {
    /// The matched atom of `id` has `found` bonds where the pattern says
    /// `expected`.
    Degree {
        id: i64,
        expected: u32,
        found: usize,
    },
    /// The pair `(a, b)` carries `found` where the pattern says `expected`;
    /// `None` on either side is "no bond". The bond list is **closed-world**
    /// within a pattern, so an unlisted pair states `expected: None`.
    Bond {
        a: i64,
        b: i64,
        expected: Option<u8>,
        found: Option<u8>,
    },
}

/// The second pass of a match: the pattern's bonds and bond counts against the
/// workpiece atoms the first pass found.
///
/// Two predicates, stated once for the whole engine:
///
/// - **`deg`**: a `before` atom stating one requires the matched workpiece atom
///   to have exactly that many bonds, of any order, counting each bond once.
/// - **closed-world bonds**: for every *pair* of pattern atoms, a listed bond
///   means the workpiece has a bond of that order between the matched atoms, and
///   an unlisted pair means the workpiece has none. Bonds to atoms **outside**
///   the pattern are not constrained — that is what `deg` is for.
///
/// `skip_degree_of` exempts one pattern id from the degree check. The placement
/// engine passes the clicked atom's role there: a wrong degree on the atom the
/// user actually clicked is a coverage report to show, not a dead branch to
/// prune. Replay has no click and passes `None`.
///
/// Returns the first disagreement, `None` when the match holds.
pub(super) fn check_pattern(
    workpiece: &AtomicStructure,
    pattern: &Pattern,
    matched: &FxHashMap<i64, u32>,
    skip_degree_of: Option<i64>,
) -> Option<PatternMismatch> {
    for pattern_atom in &pattern.atoms {
        let Some(expected) = pattern_atom.deg else {
            continue;
        };
        if skip_degree_of == Some(pattern_atom.id) {
            continue;
        }
        let Some(atom) = matched
            .get(&pattern_atom.id)
            .and_then(|atom_id| workpiece.get_atom(*atom_id))
        else {
            continue;
        };
        let found = atom.bonds.len();
        if found != expected as usize {
            return Some(PatternMismatch::Degree {
                id: pattern_atom.id,
                expected,
                found,
            });
        }
    }

    for (i, first) in pattern.atoms.iter().enumerate() {
        for second in &pattern.atoms[i + 1..] {
            let (Some(&a), Some(&b)) = (matched.get(&first.id), matched.get(&second.id)) else {
                continue;
            };
            let key = (first.id.min(second.id), first.id.max(second.id));
            let expected = pattern
                .bonds
                .iter()
                .find(|bond| bond.key() == key)
                .map(|bond| bond.order);
            let found = workpiece.bond_order_between(a, b);
            if expected != found {
                return Some(PatternMismatch::Bond {
                    a: key.0,
                    b: key.1,
                    expected,
                    found,
                });
            }
        }
    }

    None
}

/// Turns a [`PatternMismatch`] into the error a *step* fails with.
fn name_mismatch(
    workpiece: &AtomicStructure,
    matched: &FxHashMap<i64, u32>,
    mismatch: PatternMismatch,
    op_name: &str,
    step: &Step,
    step_number: usize,
    participant_of: Option<&dyn Fn(u32) -> String>,
) -> MechanosynthError {
    let label = |pattern_id: i64| {
        participant_of
            .zip(matched.get(&pattern_id))
            .map(|(label, atom_id)| label(*atom_id))
            .unwrap_or_default()
    };
    match mismatch {
        PatternMismatch::Degree {
            id,
            expected,
            found,
        } => MechanosynthError::DegreeMismatch(Box::new(DegreeMismatch {
            step: step_number,
            op: op_name.to_string(),
            t_x: step.t.x,
            t_y: step.t.y,
            t_z: step.t.z,
            atom_id: id,
            element: matched
                .get(&id)
                .and_then(|atom_id| workpiece.get_atom(*atom_id))
                .map_or_else(
                    || "?".to_string(),
                    |atom| element_symbol(atom.atomic_number),
                ),
            expected,
            found,
            participant: label(id),
        })),
        PatternMismatch::Bond {
            a,
            b,
            expected,
            found,
        } => MechanosynthError::BondMismatch(Box::new(BondMismatch {
            step: step_number,
            op: op_name.to_string(),
            t_x: step.t.x,
            t_y: step.t.y,
            t_z: step.t.z,
            a,
            b,
            expected,
            found,
            participant: label(a),
        })),
    }
}

/// Which workpiece atom each `before` atom of the pattern is, or the failure
/// that says why one of them is nowhere.
///
/// The positional pass alone: [`match_before`] is this followed by
/// [`check_pattern`], and the two are separable because the scene replay has a
/// question to ask **between** them — which participant did the match land in?
/// A tool side that matched a base atom, or a step that matched across a
/// workpiece and a reservoir, is better reported as exactly that than as the
/// bond mismatch it necessarily also is: "the tool side of tool 0 matched the C
/// of base" says why, where "the bond between atoms 1 and 2 is missing" only
/// says what.
///
/// Every `before` atom must match a distinct workpiece atom. With ideal
/// coordinates the tolerance is far below half a bond length, so ambiguity does
/// not arise; if two candidates are within tolerance the nearest wins.
///
/// `participant_of` labels the atom a failed match found instead, so the
/// message can say where it looked. `None` for a caller with no scene.
pub(super) fn match_positions(
    workpiece: &AtomicStructure,
    op_name: &str,
    before: &Pattern,
    step: &Step,
    step_number: usize,
    tolerance: f64,
    participant_of: Option<&dyn Fn(u32) -> String>,
) -> Result<FxHashMap<i64, u32>, MechanosynthError> {
    let mut matched: FxHashMap<i64, u32> = FxHashMap::default();
    let mut claimed: FxHashSet<u32> = FxHashSet::default();

    for pattern_atom in &before.atoms {
        let target = step.place(pattern_atom.pos);
        let found = workpiece.nearest_unclaimed_atom(
            target,
            tolerance,
            pattern_atom.element.required_atomic_number(),
            |atom_id| claimed.contains(&atom_id),
        );
        match found {
            Some(found) => {
                matched.insert(pattern_atom.id, found.atom_id);
                claimed.insert(found.atom_id);
            }
            None => {
                let participant = participant_of
                    .zip(workpiece.nearest_atom(target))
                    .map(|(label, nearest)| label(nearest.atom_id))
                    .unwrap_or_default();
                return Err(MechanosynthError::NoMatch(Box::new(NoMatch {
                    step: step_number,
                    op: op_name.to_string(),
                    t_x: step.t.x,
                    t_y: step.t.y,
                    t_z: step.t.z,
                    atom_id: pattern_atom.id,
                    element: pattern_element_symbol(pattern_atom.element),
                    tolerance,
                    nearest: describe_nearest(workpiece, target),
                    participant,
                })));
            }
        }
    }
    Ok(matched)
}

/// The pattern's bonds and bond counts against the atoms
/// [`match_positions`] found, as the error a *step* fails with.
///
/// Split out so the scene replay can run its participant checks first; the
/// predicate itself is [`check_pattern`], shared with the placement engine.
pub(super) fn verify_pattern(
    workpiece: &AtomicStructure,
    op_name: &str,
    before: &Pattern,
    matched: &FxHashMap<i64, u32>,
    step: &Step,
    step_number: usize,
    participant_of: Option<&dyn Fn(u32) -> String>,
) -> Result<(), MechanosynthError> {
    match check_pattern(workpiece, before, matched, None) {
        None => Ok(()),
        Some(mismatch) => Err(name_mismatch(
            workpiece,
            matched,
            mismatch,
            op_name,
            step,
            step_number,
            participant_of,
        )),
    }
}

// ============================================================================
// The steric check
// ============================================================================

/// The closest pair of atoms a step would leave **unbonded and touching**.
///
/// One end is always an atom the step *places* — adds, or moves. Every other
/// pair is unchanged by the step, and the scene's input check — the same rule,
/// applied once to the structures the engine is handed — has already had its
/// say about those.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// Centre-to-centre distance, Å.
    pub distance: f64,
    /// `distance` over the sum of the two covalent radii. Below the library's
    /// factor the step is blocked; see [`CLASH_BLOCK`](super::CLASH_BLOCK).
    pub ratio: f64,
    /// The `after` pattern id of the placed atom, and the element it ends up
    /// being.
    pub placed: (i64, i16),
    /// The atom it comes close to: the workpiece atom it is, when the
    /// workpiece has one, and its element. `None` for an atom the *same* step
    /// places — two placed atoms too close to each other are a pair like any
    /// other, and that one is a fault in the library rather than in the host.
    pub other: (Option<u32>, i16),
}

impl Contact {
    /// `the Cl of atom 481`, or `the Cl it also places`.
    pub fn other_label(&self) -> String {
        let element = element_symbol(self.other.1);
        match self.other.0 {
            Some(atom_id) => format!("the {element} of atom {atom_id}"),
            None => format!("the {element} it also places"),
        }
    }

    /// The contact in the words a refused candidate shows where its residual
    /// would be.
    pub fn reason(&self) -> String {
        format!(
            "would put {} {:.2} Å from {}",
            element_symbol(self.placed.1),
            self.distance,
            self.other_label(),
        )
    }
}

/// The covalent radius of an element, falling back to the table's default for
/// one it does not list. Never zero, so a ratio is always defined.
pub(super) fn covalent_radius(z: i16) -> f64 {
    ATOM_INFO
        .get(&(z as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO)
        .covalent_radius
}

/// One atom of the after state, named the way the check has to name it: by the
/// workpiece atom it is, or — for an atom this step places — by its pattern id,
/// because a moved atom is in two places at once until the step is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Node {
    Existing(u32),
    Placed(i64),
}

/// An atom the step adds or moves, in the after state, with everything it ends
/// up bonded to — which is exactly what it must *not* be compared against.
struct PlacedAtom {
    id: i64,
    pos: DVec3,
    element: i16,
    bonded: FxHashSet<Node>,
}

/// The worst non-bonded contact the step would create, or `None` when it
/// creates none.
///
/// **One rule: two atoms that are not bonded to each other must not be closer
/// than a fraction of their covalent-radius sum; bonded atoms are never
/// checked.** A bond is the library's statement that the two belong at bond
/// distance, whatever that distance is, and the engine has no better opinion.
/// Everything else is a non-bonded contact and one threshold applies to all of
/// them. See `doc/design_mechanosynth_pattern_checks.md` §5.
///
/// **Nothing is mutated to run it.** [`apply_matched`] is infallible by design
/// and there is no rollback of a half-applied step, so this works from the
/// match and the patterns alone: a placed atom's position is `step.place(p)`,
/// its bonds are what the rewrite leaves it with, and its neighbours are the
/// workpiece atoms around it minus the ones the step deletes and minus the old
/// positions of the ones it moves. That is the same information
/// [`preview_atoms`](super::preview_atoms) derives, which is why a blocked
/// candidate is still previewable.
///
/// Takes the two patterns rather than an [`Operation`] so that a tool side —
/// which is a before/after pair in the tool's own frame — goes through the very
/// same function.
pub(super) fn worst_contact(
    workpiece: &AtomicStructure,
    before: &Pattern,
    after: &Pattern,
    step: &Step,
    matched: &FxHashMap<i64, u32>,
) -> Option<Contact> {
    // An id is *placed* when the step adds it, or keeps it somewhere else. A
    // kept atom is not: `apply_matched` never snaps one, so it stays where the
    // workpiece has it and its contacts are not this step's doing.
    let is_placed = |id: i64| match (before.atom(id), after.atom(id)) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(b), Some(a)) => b.pos.distance(a.pos) > PATTERN_POSITION_EPSILON,
    };

    let placed_ids: Vec<i64> = after
        .atoms
        .iter()
        .map(|atom| atom.id)
        .filter(|id| is_placed(*id))
        .collect();
    if placed_ids.is_empty() {
        return None;
    }

    // The workpiece atoms that will not be there afterwards, and the ones that
    // will be somewhere else. Both drop out of the neighbour scan: a deleted
    // atom is gone, and a moved one is compared at its new position instead.
    let deleted: FxHashSet<u32> = before
        .atoms
        .iter()
        .filter(|atom| !after.has(atom.id))
        .filter_map(|atom| matched.get(&atom.id).copied())
        .collect();
    let moved_from: FxHashMap<u32, i64> = placed_ids
        .iter()
        .filter_map(|id| matched.get(id).map(|atom_id| (*atom_id, *id)))
        .collect();

    let node_of_pattern = |id: i64| -> Option<Node> {
        if is_placed(id) {
            Some(Node::Placed(id))
        } else {
            matched.get(&id).copied().map(Node::Existing)
        }
    };
    let node_of_atom = |atom_id: u32| -> Node {
        match moved_from.get(&atom_id) {
            Some(id) => Node::Placed(*id),
            None => Node::Existing(atom_id),
        }
    };

    let mut atoms: Vec<PlacedAtom> = Vec::with_capacity(placed_ids.len());
    for &id in &placed_ids {
        let after_atom = after.atom(id).expect("placed ids come from `after`");
        let current = matched
            .get(&id)
            .and_then(|atom_id| workpiece.get_atom(*atom_id));
        let element = match after_atom.element {
            PatternElement::Element(z) => z,
            // `"*"` on a moved id leaves the workpiece atom's element alone.
            PatternElement::Any => current.map_or(0, |atom| atom.atomic_number),
        };

        let mut bonded: FxHashSet<Node> = FxHashSet::default();
        if before.has(id) {
            // Moved: the bonds it has, minus the ones only `before` lists,
            // plus the ones only `after` does.
            if let Some(atom) = current {
                for bond in &atom.bonds {
                    let other = bond.other_atom_id();
                    if deleted.contains(&other) {
                        continue;
                    }
                    bonded.insert(node_of_atom(other));
                }
            }
            for bond in &before.bonds {
                let Some(other) = bond.other_end(id) else {
                    continue;
                };
                if after.bonds.iter().any(|kept| kept.key() == bond.key()) {
                    continue;
                }
                if let Some(node) = node_of_pattern(other) {
                    bonded.remove(&node);
                }
            }
        }
        for bond in &after.bonds {
            let Some(other) = bond.other_end(id) else {
                continue;
            };
            if let Some(node) = node_of_pattern(other) {
                bonded.insert(node);
            }
        }

        atoms.push(PlacedAtom {
            id,
            pos: step.place(after_atom.pos),
            element,
            bonded,
        });
    }

    let mut worst: Option<Contact> = None;
    let mut consider = |placed: &PlacedAtom, other: (Option<u32>, i16), position: DVec3| {
        let distance = placed.pos.distance(position);
        let ratio = distance / (covalent_radius(placed.element) + covalent_radius(other.1));
        if worst.is_some_and(|best: Contact| best.ratio <= ratio) {
            return;
        }
        worst = Some(Contact {
            distance,
            ratio,
            placed: (placed.id, placed.element),
            other,
        });
    };

    for placed in &atoms {
        // The other atoms this step places, at the positions it places them.
        for partner in &atoms {
            if partner.id == placed.id || placed.bonded.contains(&Node::Placed(partner.id)) {
                continue;
            }
            consider(placed, (None, partner.element), partner.pos);
        }
        // Everything the workpiece already has around it, the step's own
        // casualties and travellers excepted.
        for atom_id in workpiece.get_atoms_in_radius(&placed.pos, CLASH_SEARCH_RADIUS) {
            if deleted.contains(&atom_id) || moved_from.contains_key(&atom_id) {
                continue;
            }
            if placed.bonded.contains(&Node::Existing(atom_id)) {
                continue;
            }
            let Some(atom) = workpiece.get_atom(atom_id) else {
                continue;
            };
            consider(placed, (Some(atom_id), atom.atomic_number), atom.position);
        }
    }

    worst
}

/// [`worst_contact`], as the error a *step* fails with when the factor blocks
/// it.
///
/// Runs after [`verify_pattern`] and **before** [`apply_matched`], at replay
/// and at placement alike, which is what makes "a candidate is offerable iff
/// the step it produces replays" hold for the steric rule too.
#[allow(clippy::too_many_arguments)]
pub(super) fn verify_clash(
    workpiece: &AtomicStructure,
    op_name: &str,
    before: &Pattern,
    after: &Pattern,
    matched: &FxHashMap<i64, u32>,
    step: &Step,
    step_number: usize,
    clash: f64,
    participant_of: Option<&dyn Fn(u32) -> String>,
) -> Result<(), MechanosynthError> {
    let Some(contact) = worst_contact(workpiece, before, after, step, matched) else {
        return Ok(());
    };
    if contact.ratio >= clash {
        return Ok(());
    }
    let sum = covalent_radius(contact.placed.1) + covalent_radius(contact.other.1);
    Err(MechanosynthError::Clash(Box::new(Clash {
        step: step_number,
        op: op_name.to_string(),
        t_x: step.t.x,
        t_y: step.t.y,
        t_z: step.t.z,
        placed_id: contact.placed.0,
        placed_element: element_symbol(contact.placed.1),
        other: contact.other_label(),
        distance: contact.distance,
        ratio: contact.ratio,
        factor: clash,
        limit: clash * sum,
        participant: participant_of
            .zip(contact.other.0)
            .map(|(label, atom_id)| label(atom_id))
            .unwrap_or_default(),
    })))
}

/// The worst non-bonded contact `step` would create on `workpiece`, for a
/// caller that wants to *measure* rather than to gate — the replay-floor
/// fixture, and anything else asking how close a legitimate build comes.
///
/// Fails exactly where [`apply_step`] would fail to match.
pub fn step_contact(
    workpiece: &AtomicStructure,
    op: &Operation,
    step: &Step,
    step_number: usize,
    tolerance: f64,
) -> Result<Option<Contact>, MechanosynthError> {
    let matched = match_before(
        workpiece,
        &op.name,
        &op.before,
        step,
        step_number,
        tolerance,
        None,
    )?;
    Ok(worst_contact(
        workpiece, &op.before, &op.after, step, &matched,
    ))
}

/// [`match_positions`] followed by [`verify_pattern`]: the whole match, for a
/// caller with no scene to interpose a participant check.
pub(super) fn match_before(
    workpiece: &AtomicStructure,
    op_name: &str,
    before: &Pattern,
    step: &Step,
    step_number: usize,
    tolerance: f64,
    participant_of: Option<&dyn Fn(u32) -> String>,
) -> Result<FxHashMap<i64, u32>, MechanosynthError> {
    let matched = match_positions(
        workpiece,
        op_name,
        before,
        step,
        step_number,
        tolerance,
        participant_of,
    )?;
    verify_pattern(
        workpiece,
        op_name,
        before,
        &matched,
        step,
        step_number,
        participant_of,
    )?;
    Ok(matched)
}

/// Rewrites `workpiece` by the `before` → `after` difference, given the match
/// [`match_before`] found. Infallible: every way of failing was the match,
/// bonds and bond counts included.
pub(super) fn apply_matched(
    workpiece: &mut AtomicStructure,
    before: &Pattern,
    after: &Pattern,
    step: &Step,
    matched: FxHashMap<i64, u32>,
) -> StepEffect {
    let mut matched = matched;

    // --- 2. what the step will change ---------------------------------------
    // Read before anything is mutated, because "did this bond change" is a
    // question about the workpiece as the step found it.
    let before_bonds: FxHashMap<(i64, i64), u8> =
        before.bonds.iter().map(|b| (b.key(), b.order)).collect();
    let after_bonds: FxHashMap<(i64, i64), u8> =
        after.bonds.iter().map(|b| (b.key(), b.order)).collect();

    // Pattern ids at either end of a bond this step actually rewrites. A bond
    // rule that is a no-op on this workpiece — deleting one that is not there,
    // adding one it already has at the same order — changes nothing and so
    // touches nobody.
    let mut bond_changed: FxHashSet<i64> = FxHashSet::default();
    let note_bond_change = |bond_changed: &mut FxHashSet<i64>, key: (i64, i64)| {
        bond_changed.insert(key.0);
        bond_changed.insert(key.1);
    };
    for key in before_bonds.keys() {
        if after_bonds.contains_key(key) {
            continue;
        }
        let (Some(&a), Some(&b)) = (matched.get(&key.0), matched.get(&key.1)) else {
            continue;
        };
        if workpiece.bond_order_between(a, b).is_some() {
            note_bond_change(&mut bond_changed, *key);
        }
    }
    for (key, &order) in after_bonds.iter() {
        if before_bonds.get(key) == Some(&order) {
            continue; // present in both with the same order: untouched
        }
        // An endpoint only `after` names has not been added yet, so there is
        // no current bond and the rule certainly changes something.
        let current = match (matched.get(&key.0), matched.get(&key.1)) {
            (Some(&a), Some(&b)) => workpiece.bond_order_between(a, b),
            _ => None,
        };
        if current != Some(order) {
            note_bond_change(&mut bond_changed, *key);
        }
    }

    // --- 3. apply -----------------------------------------------------------
    let mut touched: Vec<u32> = Vec::new();
    let mut added: Vec<u32> = Vec::new();
    fn mark(touched: &mut Vec<u32>, atom_id: u32) {
        if !touched.contains(&atom_id) {
            touched.push(atom_id);
        }
    }

    // Ids only in `before`: delete. Every bond the atom had — to pattern atoms
    // or to any other workpiece atom — goes with it. The neighbours are noted
    // first: a deletion's footprint is the atoms it was bonded to, which is all
    // that is left to highlight after an abstraction.
    let mut deletion_neighbours: Vec<u32> = Vec::new();
    for pattern_atom in &before.atoms {
        if after.has(pattern_atom.id) {
            continue;
        }
        let atom_id = matched[&pattern_atom.id];
        if let Some(atom) = workpiece.get_atom(atom_id) {
            deletion_neighbours.extend(atom.bonds.iter().map(|bond| bond.other_atom_id()));
        }
        workpiece.delete_atom(atom_id);
    }

    // Ids in both: keep, move and/or replace. Position and element are compared
    // between the two *patterns*, never against the workpiece, so a kept atom
    // stays exactly where the workpiece has it and is not snapped to the
    // pattern position.
    for after_atom in &after.atoms {
        let Some(before_atom) = before.atom(after_atom.id) else {
            continue;
        };
        let atom_id = matched[&after_atom.id];
        let mut changed = bond_changed.contains(&after_atom.id);
        if before_atom.pos.distance(after_atom.pos) > PATTERN_POSITION_EPSILON {
            workpiece.set_atom_position(atom_id, step.place(after_atom.pos));
            changed = true;
        }
        // `*` on the `after` side keeps the workpiece atom's current element.
        // A concrete element that the atom already has is not a change either:
        // the comparison is against the workpiece, so `"*" → "C"` on a carbon
        // leaves a frame atom untouched.
        if let PatternElement::Element(z) = after_atom.element
            && workpiece.get_atom(atom_id).map(|atom| atom.atomic_number) != Some(z)
        {
            workpiece.set_atomic_number(atom_id, z);
            changed = true;
        }
        if changed {
            mark(&mut touched, atom_id);
        }
    }

    // Ids only in `after`: add. Validation guarantees a concrete element here.
    for after_atom in &after.atoms {
        if before.has(after_atom.id) {
            continue;
        }
        let PatternElement::Element(z) = after_atom.element else {
            unreachable!("parse rejects \"*\" on an added atom");
        };
        let atom_id = workpiece.add_atom(z, step.place(after_atom.pos));
        matched.insert(after_atom.id, atom_id);
        mark(&mut touched, atom_id);
        added.push(atom_id);
    }

    // --- 4. bonds -----------------------------------------------------------
    // Bond rules apply to the surviving atoms only. Since `/3` they are also
    // never no-ops: the closed-world check in `match_before` has already
    // established that every listed bond exists at the order the pattern states
    // and that every unlisted pair does not, so "delete a bond that is not
    // there" and "add one the workpiece already has" cannot occur — a step that
    // would have done either failed to match.
    for key in before_bonds.keys() {
        if after_bonds.contains_key(key) {
            continue;
        }
        let (Some(&a), Some(&b)) = (matched.get(&key.0), matched.get(&key.1)) else {
            continue;
        };
        workpiece.delete_bond(&BondReference {
            atom_id1: a,
            atom_id2: b,
        });
    }

    for (key, &order) in after_bonds.iter() {
        if before_bonds.get(key) == Some(&order) {
            continue; // present in both with the same order: untouched
        }
        let (Some(&a), Some(&b)) = (matched.get(&key.0), matched.get(&key.1)) else {
            continue;
        };
        workpiece.add_bond_checked(a, b, order);
    }

    // A neighbour may itself have been deleted by this step, or already be in
    // the list as a changed atom; only survivors are reported, each once.
    for neighbour_id in deletion_neighbours {
        if workpiece.get_atom(neighbour_id).is_some() {
            mark(&mut touched, neighbour_id);
        }
    }

    StepEffect { touched, added }
}

/// The tail of a match-failure message: what *is* near the position the step
/// looked at. The placement engine builds the same sentence, so both halves of
/// the engine fail in the same words.
pub fn describe_nearest(workpiece: &AtomicStructure, target: DVec3) -> String {
    match workpiece.nearest_atom(target) {
        Some(found) => {
            let element = workpiece.get_atom(found.atom_id).map_or_else(
                || "?".to_string(),
                |atom| element_symbol(atom.atomic_number),
            );
            format!("nearest atom is {} at {:.2} Å", element, found.distance)
        }
        None => "the workpiece has no atoms".to_string(),
    }
}

/// The atom tags [`replay`] paints, each optional. `None` everywhere paints
/// nothing and interns no tag name, which is what the engine tests and any
/// non-UI caller want.
///
/// Every tag is cleared from the base clone before anything is applied, so an
/// upstream `mechanosynth` node's highlights never leak into a downstream
/// one's.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HighlightTags<'a> {
    /// Painted on the last applied step's [`StepEffect::touched`] atoms.
    pub current: Option<&'a str>,
    /// Painted on every atom **created** by an applied step that still exists.
    pub added: Option<&'a str>,
    /// Painted on every atom created by an applied step whose `layer` equals
    /// the last applied step's `layer`. Empty when that layer is
    /// [`NO_LAYER`](super::schema::NO_LAYER), because "no particular layer" is
    /// not a layer to highlight.
    ///
    /// `added` and `layer` are painted on **base atoms only**: they mean "what
    /// the build created on the workpiece", and an abstracted H sitting on the
    /// tip, or dumped on a reservoir, is cargo rather than construction.
    pub layer: Option<&'a str>,
    /// Painted on every atom of every wired tool molecule.
    pub tool: Option<&'a str>,
    /// Painted on every atom of every wired reservoir.
    pub feedstock: Option<&'a str>,
}

/// Replays the first `step` steps of `script` onto a copy of `base`.
///
/// `step = k` means "the first `k` steps applied", `step = 0` is the untouched
/// base, a negative `step` or one past the end means the full build. Steps are
/// numbered from 1 in messages, so "step 17" is `steps[16]`.
///
/// `tags` selects which highlights to paint; see [`HighlightTags`]. Membership
/// of the `added` and `layer` sets is decided by **the script's own metadata**,
/// never by geometry — an atom a step *moved* was not created by it, so it
/// stays out of that step's layer, which is right: it belongs to the layer
/// below.
///
/// A step that fails to match aborts the whole replay; the partial state is
/// reachable by asking for one step fewer.
///
/// This is [`replay_scene`](super::replay_scene) with no feedstocks and no
/// tools, which is exactly what it means: with nothing wired, the scene *is*
/// the workpiece, no binding runs, and the tool side of every operation is
/// skipped.
pub fn replay(
    base: &AtomicStructure,
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    tags: HighlightTags<'_>,
) -> Result<AtomicStructure, MechanosynthError> {
    super::replay_scene(base, &[], &[], library, script, step, tags).map(|scene| scene.structure)
}

/// Clears `tag` from every atom and paints it on the surviving ids of `atoms`.
///
/// The clear happens whenever a tag is requested, including at step 0 and for
/// an empty set: any pre-existing value on the base — e.g. from an upstream
/// `mechanosynth` node — is not this replay's. `atoms_with_tag` is empty when
/// the name is not interned, so this is a no-op on a structure that has never
/// seen the tag.
pub(super) fn paint(workpiece: &mut AtomicStructure, tag: Option<&str>, atoms: Vec<u32>) {
    let Some(tag) = tag else { return };
    for atom_id in workpiece.atoms_with_tag(tag) {
        workpiece.remove_atom_tag(atom_id, tag);
    }
    for atom_id in atoms {
        // The tag table is 32 slots wide and may be full; a highlight is
        // cosmetic, so losing it is not worth failing a replay over.
        let _ = workpiece.add_atom_tag(atom_id, tag);
    }
}
