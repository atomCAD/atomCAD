//! Applying build steps to a workpiece.
//!
//! Matching is nearest-atom-within-tolerance on position and element, using the
//! spatial grid [`AtomicStructure`] already maintains, so a step costs O(pattern
//! size). Bonds take no part in matching: position plus element is sufficient on
//! a lattice, and checking bonds would only add a way for a correct script to
//! fail.

use super::schema::{
    BuildScript, DEFAULT_TOLERANCE, MechanosynthError, NO_LAYER, OpLibrary, Operation,
    PATTERN_POSITION_EPSILON, PatternElement, Step,
};
use crate::atomic_constants::ATOM_INFO;
use crate::atomic_structure::{AtomicStructure, BondReference};
use glam::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};

/// The element symbol for a readout, falling back to the atomic number for the
/// non-physical ones (parameter elements, debug colours).
fn element_symbol(atomic_number: i16) -> String {
    ATOM_INFO
        .get(&(atomic_number as i32))
        .map(|info| info.symbol.clone())
        .unwrap_or_else(|| format!("Z={atomic_number}"))
}

fn pattern_element_symbol(element: PatternElement) -> String {
    match element {
        PatternElement::Any => "*".to_string(),
        PatternElement::Element(z) => element_symbol(z),
    }
}

/// The tolerance in force for a replay: the script's, else the library's, else
/// [`DEFAULT_TOLERANCE`]. There is exactly one for a whole replay.
pub fn resolve_tolerance(library: &OpLibrary, script: &BuildScript) -> f64 {
    script
        .tolerance
        .or(library.tolerance)
        .unwrap_or(DEFAULT_TOLERANCE)
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
    /// The atoms this step touched *and left in place*: matched atoms that were
    /// kept, moved or replaced, the atoms it added, and the surviving atoms
    /// that were bonded to an atom it deleted. Deleted atoms are not in the
    /// list — they no longer exist — but their bonded neighbours stand in for
    /// them, so a pure abstraction still points at the site it acted on. This
    /// is what [`HighlightTags::current`] is painted on.
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
    // --- 1. match -----------------------------------------------------------
    // Every `before` atom must match a distinct workpiece atom. With ideal
    // coordinates a 0.3 Å tolerance is far below half a bond length, so
    // ambiguity does not arise; if two candidates are within tolerance the
    // nearest wins.
    let mut matched: FxHashMap<i64, u32> = FxHashMap::default();
    let mut claimed: FxHashSet<u32> = FxHashSet::default();

    for pattern_atom in &op.before.atoms {
        let target = step.place(pattern_atom.pos);
        let mut best: Option<(u32, f64)> = None;
        for candidate_id in workpiece.get_atoms_in_radius(&target, tolerance) {
            if claimed.contains(&candidate_id) {
                continue;
            }
            let Some(atom) = workpiece.get_atom(candidate_id) else {
                continue;
            };
            if !pattern_atom.element.matches(atom.atomic_number) {
                continue;
            }
            let distance = atom.position.distance(target);
            if best.is_none_or(|(_, best_distance)| distance < best_distance) {
                best = Some((candidate_id, distance));
            }
        }
        match best {
            Some((atom_id, _)) => {
                matched.insert(pattern_atom.id, atom_id);
                claimed.insert(atom_id);
            }
            None => {
                return Err(MechanosynthError::NoMatch {
                    step: step_number,
                    op: op.name.clone(),
                    t_x: step.t.x,
                    t_y: step.t.y,
                    t_z: step.t.z,
                    atom_id: pattern_atom.id,
                    element: pattern_element_symbol(pattern_atom.element),
                    tolerance,
                    nearest: describe_nearest(workpiece, target),
                });
            }
        }
    }

    // --- 2. apply -----------------------------------------------------------
    let mut touched: Vec<u32> = Vec::new();
    let mut added: Vec<u32> = Vec::new();

    // Ids only in `before`: delete. Every bond the atom had — to pattern atoms
    // or to any other workpiece atom — goes with it. The neighbours are noted
    // first: a deletion's footprint is the atoms it was bonded to, which is all
    // that is left to highlight after an abstraction.
    let mut deletion_neighbours: Vec<u32> = Vec::new();
    for pattern_atom in &op.before.atoms {
        if op.after.has(pattern_atom.id) {
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
    for after_atom in &op.after.atoms {
        let Some(before_atom) = op.before.atom(after_atom.id) else {
            continue;
        };
        let atom_id = matched[&after_atom.id];
        if before_atom.pos.distance(after_atom.pos) > PATTERN_POSITION_EPSILON {
            workpiece.set_atom_position(atom_id, step.place(after_atom.pos));
        }
        // `*` on the `after` side keeps the workpiece atom's current element.
        if let PatternElement::Element(z) = after_atom.element {
            workpiece.set_atomic_number(atom_id, z);
        }
        touched.push(atom_id);
    }

    // Ids only in `after`: add. Validation guarantees a concrete element here.
    for after_atom in &op.after.atoms {
        if op.before.has(after_atom.id) {
            continue;
        }
        let PatternElement::Element(z) = after_atom.element else {
            unreachable!("parse rejects \"*\" on an added atom");
        };
        let atom_id = workpiece.add_atom(z, step.place(after_atom.pos));
        matched.insert(after_atom.id, atom_id);
        touched.push(atom_id);
        added.push(atom_id);
    }

    // --- 3. bonds -----------------------------------------------------------
    // Bond rules apply to the surviving atoms only and are idempotent: a bond
    // whose endpoint was deleted went with the atom, deleting an absent bond is
    // a no-op, and adding one the workpiece already has just sets its order.
    let before_bonds: FxHashMap<(i64, i64), u8> =
        op.before.bonds.iter().map(|b| (b.key(), b.order)).collect();
    let after_bonds: FxHashMap<(i64, i64), u8> =
        op.after.bonds.iter().map(|b| (b.key(), b.order)).collect();

    for (key, _) in before_bonds.iter() {
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
    // the list as a kept atom; only survivors are reported, each once.
    for neighbour_id in deletion_neighbours {
        if workpiece.get_atom(neighbour_id).is_some() && !touched.contains(&neighbour_id) {
            touched.push(neighbour_id);
        }
    }

    Ok(StepEffect { touched, added })
}

/// The tail of a match-failure message: what *is* near the position the step
/// looked at. A whole-structure scan, which is affordable because the replay is
/// over either way.
fn describe_nearest(workpiece: &AtomicStructure, target: DVec3) -> String {
    let nearest = workpiece
        .atoms_values()
        .map(|atom| (atom.atomic_number, atom.position.distance(target)))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    match nearest {
        Some((atomic_number, distance)) => format!(
            "nearest atom is {} at {:.2} Å",
            element_symbol(atomic_number),
            distance
        ),
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
    pub layer: Option<&'a str>,
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
pub fn replay(
    base: &AtomicStructure,
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    tags: HighlightTags<'_>,
) -> Result<AtomicStructure, MechanosynthError> {
    super::parse::validate_script_ops(script, library)?;

    let tolerance = resolve_tolerance(library, script);
    let n = steps_applied(step, script.steps.len());

    // The layer under construction is the last applied step's, read up front so
    // the loop below can filter as it goes. `NO_LAYER` disables the layer set
    // entirely.
    let active_layer = n
        .checked_sub(1)
        .and_then(|index| script.steps.get(index))
        .map(|last| last.layer)
        .filter(|layer| *layer != NO_LAYER);

    let mut workpiece = base.clone();
    let mut last_touched: Vec<u32> = Vec::new();
    let mut created: Vec<u32> = Vec::new();
    let mut created_in_layer: Vec<u32> = Vec::new();

    for (i, script_step) in script.steps.iter().take(n).enumerate() {
        let op = library
            .get(&script_step.op)
            .expect("validate_script_ops checked every op name");
        let effect = apply_step(&mut workpiece, op, script_step, i + 1, tolerance)?;
        if tags.added.is_some() {
            created.extend(effect.added.iter().copied());
        }
        if tags.layer.is_some() && active_layer == Some(script_step.layer) {
            created_in_layer.extend(effect.added.iter().copied());
        }
        last_touched = effect.touched;
    }

    // Ids are never reused, so a created atom a later step deleted is simply
    // gone; the sets are filtered against the finished workpiece rather than
    // bookkept step by step.
    paint(&mut workpiece, tags.current, last_touched);
    paint(&mut workpiece, tags.added, created);
    paint(&mut workpiece, tags.layer, created_in_layer);

    Ok(workpiece)
}

/// Clears `tag` from every atom and paints it on the surviving ids of `atoms`.
///
/// The clear happens whenever a tag is requested, including at step 0 and for
/// an empty set: any pre-existing value on the base — e.g. from an upstream
/// `mechanosynth` node — is not this replay's. `atoms_with_tag` is empty when
/// the name is not interned, so this is a no-op on a structure that has never
/// seen the tag.
fn paint(workpiece: &mut AtomicStructure, tag: Option<&str>, atoms: Vec<u32>) {
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
