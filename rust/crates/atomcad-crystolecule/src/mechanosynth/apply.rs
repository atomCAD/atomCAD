//! Applying build steps to a workpiece.
//!
//! Matching is nearest-atom-within-tolerance on position and element, using the
//! spatial grid [`AtomicStructure`] already maintains, so a step costs O(pattern
//! size). Bonds take no part in matching: position plus element is sufficient on
//! a lattice, and checking bonds would only add a way for a correct script to
//! fail.

use super::schema::{
    BuildScript, DEFAULT_TOLERANCE, MechanosynthError, NoMatch, OpLibrary, Operation,
    PATTERN_POSITION_EPSILON, Pattern, PatternElement, Step,
};
use crate::atomic_constants::element_symbol;
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
pub fn apply_step(
    workpiece: &mut AtomicStructure,
    op: &Operation,
    step: &Step,
    step_number: usize,
    tolerance: f64,
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
    Ok(apply_matched(
        workpiece, &op.before, &op.after, step, matched,
    ))
}

/// Which workpiece atom each `before` atom of the pattern is, or the failure
/// that says why one of them is nowhere.
///
/// Split from [`apply_matched`] because the scene replay has a question to ask
/// **between** the two: which participant did the match land in? A step that
/// matched across a workpiece and a reservoir has no answer and must leave the
/// scene untouched, so the check cannot come after the rewrite.
///
/// Every `before` atom must match a distinct workpiece atom. With ideal
/// coordinates the tolerance is far below half a bond length, so ambiguity does
/// not arise; if two candidates are within tolerance the nearest wins.
///
/// `participant_of` labels the atom a failed match found instead, so the
/// message can say where it looked. `None` for a caller with no scene.
pub(super) fn match_before(
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

/// Rewrites `workpiece` by the `before` → `after` difference, given the match
/// [`match_before`] found. Infallible: every way of failing was the match.
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
    // Bond rules apply to the surviving atoms only and are idempotent: a bond
    // whose endpoint was deleted went with the atom, deleting an absent bond is
    // a no-op, and adding one the workpiece already has just sets its order.
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
