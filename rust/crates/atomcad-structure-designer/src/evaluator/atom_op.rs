//! Shared seams for atom-operation nodes: **region gating** and **diff output
//! pins**. Both are patterns spanning many node files, so the helpers live here
//! rather than being re-implemented per node.
//!
//! # Region gating (`doc/design_blueprint_region_atom_edits.md` Part A)
//!
//! `passivate`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, and the
//! metadata-edit pair `freeze` / `unfreeze` each carry an optional
//! `region: Blueprint` input pin **as their last pin**. Disconnected
//! (`NetworkResult::None`) → operate on **all** atoms (the unchanged legacy
//! behavior); connected → only atoms whose position is inside the region volume,
//! where membership is `region_geo.implicit_eval_3d(pos) <=
//! DEFAULT_REGION_MARGIN` (the 0.1 Å constant shared from
//! `crystolecule::lattice_fill`).
//!
//! [`map_atomic_in_region`] is the seam (sibling of `map_atomic`): it
//! batch-computes membership over all atom positions and hands the per-atom
//! `in_region: &dyn Fn(u32) -> bool` predicate to the mutation closure. A
//! `region == None` short-circuits to "all in-region", so it subsumes
//! `map_atomic`. Rules to preserve when adding a region-gated op:
//!
//! - Test the **host / existing** atom, never a newly-created one
//!   (`infer_bonds` is one-endpoint-inside).
//! - Evaluate `region` with `evaluate_arg` (optional), **not**
//!   `evaluate_arg_required`.
//! - **Multiple regions = chained nodes.** There is no multi-region pin here;
//!   the painter's-algorithm pattern is unique to `materialize.regions` (Part B).
//! - No `.cnnd` migration is needed — the new pin appears unconnected on
//!   existing nodes.
//!
//! # Diff output pins (issue #295, `doc/design_diff_outputs_for_atom_ops.md`)
//!
//! `relax`, the four movement nodes (`free_move` / `free_rot` /
//! `structure_move` / `structure_rot`), `atom_replace`, and `atom_cut` each
//! carry a second `diff` output pin (pin 1) alongside `result` (pin 0) — the
//! same two-pin shape as `atom_edit`:
//!
//! ```text
//! output_pins = [same_as_input("result", <input pin name>), fixed("diff", Molecule)]
//! ```
//!
//! (the input pin name is `"molecule"` for relax/atom_replace/atom_cut,
//! `"input"` for the movement nodes). The primitive is
//! `crystolecule::atomic_structure_diff::extract_diff(before, after, eps)`,
//! which derives an applyable `Molecule` diff from a before/after pair **by atom
//! id** — all these nodes mutate a clone in place, so ids are stable (see the
//! design doc's §1.5 id-stability audit).
//!
//! The pattern: snapshot `before` *before* the mutation, run the existing
//! mutation, `extract_diff`, return `EvalOutput::multi([result, diff_pin])`.
//! **Every** error / early-return path must return two-pin errors
//! (`multi(vec![err.clone(), err])`) — do *not* copy `atom_edit`'s
//! `EvalOutput::single(error)`, which degrades pin 1 to `None`. `relax` and the
//! movement nodes snapshot inline; `atom_replace` / `atom_cut` route through
//! `map_atomic` / `map_atomic_in_region` (which consume the input) and so use
//! [`snapshot_atoms`], [`diff_output_pin`], and [`eval_output_with_diff`] here.
//!
//! `relax` additionally has a serde-defaulted `diff_min_move: f64` prune
//! property (the eps passed to `extract_diff`; the only FRB regen in the
//! feature); the others always pass eps = 0.0. Movement / atom_replace /
//! atom_cut on Blueprint or non-atomic inputs produce an **empty** diff (§2.3),
//! and movement diffs capture **atom motion only** (§2.4). There is no `.cnnd`
//! migration and no snapshot churn (output pins live on `NodeType`), and these
//! nodes must **not** override `default_display_all_output_pins` — a diff draws
//! viewport geometry, so the pin-0-only default is correct. Explicitly out of
//! scope: `passivate`, `remove_hydrogen`, `infer_bonds`, `freeze` / `unfreeze`,
//! `atom_union`.

use crate::evaluator::network_result::{MoleculeData, NetworkResult};
use crate::node_data::EvalOutput;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure_diff::extract_diff;
use atomcad_geo_tree::GeoNode;
use atomcad_geo_tree::batched_implicit_evaluator::BatchedImplicitEvaluator;
use std::collections::HashSet;

/// Applies a transformation to the `AtomicStructure` inside a `Crystal` or `Molecule`
/// `NetworkResult`, preserving the concrete variant and any associated
/// `structure` / `geo_tree_root` metadata.
///
/// This is the shared implementation of the `SameAsInput` output-type preservation
/// contract for polymorphic atom-operation nodes: Crystal-in → Crystal-out,
/// Molecule-in → Molecule-out. Non-atomic inputs yield a `NetworkResult::Error`.
pub fn map_atomic<F>(input: NetworkResult, f: F) -> NetworkResult
where
    F: FnOnce(AtomicStructure) -> AtomicStructure,
{
    match input {
        NetworkResult::Crystal(mut c) => {
            c.atoms = f(c.atoms);
            NetworkResult::Crystal(c)
        }
        NetworkResult::Molecule(mut m) => {
            m.atoms = f(m.atoms);
            NetworkResult::Molecule(m)
        }
        other => NetworkResult::Error(format!(
            "atom op received non-atomic input: {:?}",
            other.infer_data_type()
        )),
    }
}

/// Like [`map_atomic`], but the closure additionally receives a membership
/// predicate telling it which atoms lie inside an optional region volume.
///
/// `region == None` → every atom is in-region (exactly [`map_atomic`]'s
/// behavior; the helper subsumes the old one so each op keeps a single code
/// path). `region == Some(geo)` → membership of an atom is decided by the
/// region SDF at the atom's **raw real-space position**:
/// `geo.implicit_eval_3d(atom.position) ≤ margin`. `geo_tree_root` is already
/// in absolute real (Å) coordinates, so there is **no** unit-cell rescaling
/// (contrast the legacy `atom_cut`, which divides by `unit_cell_size` — that
/// is a known bug, not a pattern to copy). See
/// `doc/design_blueprint_region_atom_edits.md` §A3.
///
/// Membership is precomputed once via [`BatchedImplicitEvaluator`] over all
/// atom positions (parallel-friendly batch), yielding a `HashSet<atom_id>`
/// consulted by the predicate handed to `f`. Newly created atoms are never
/// membership-tested — the predicate only knows the atoms present when the
/// node is entered.
pub fn map_atomic_in_region<F>(
    input: NetworkResult,
    region: Option<&GeoNode>,
    margin: f64,
    f: F,
) -> NetworkResult
where
    F: FnOnce(AtomicStructure, &dyn Fn(u32) -> bool) -> AtomicStructure,
{
    map_atomic(input, move |structure| match region {
        None => {
            let all_in_region = |_atom_id: u32| true;
            f(structure, &all_in_region)
        }
        Some(geo) => {
            // Batch-evaluate the region SDF at every atom position.
            let mut evaluator = BatchedImplicitEvaluator::new_with_threading(geo, true);
            let atom_ids: Vec<u32> = structure
                .iter_atoms()
                .map(|(atom_id, atom)| {
                    evaluator.add_point(atom.position);
                    *atom_id
                })
                .collect();
            let sdf_values = evaluator.flush();

            let in_region: HashSet<u32> = atom_ids
                .iter()
                .zip(sdf_values.iter())
                .filter(|&(_, &sdf)| sdf <= margin)
                .map(|(&atom_id, _)| atom_id)
                .collect();

            let predicate = move |atom_id: u32| in_region.contains(&atom_id);
            f(structure, &predicate)
        }
    })
}

/// Clones the `AtomicStructure` out of an atomic `NetworkResult`
/// (`Crystal` / `Molecule`) so it can serve as the `before` snapshot for a diff.
/// Non-atomic results yield `None` — `map_atomic` will already have turned such
/// an input into an `Error` result, so the diff step is skipped.
pub fn snapshot_atoms(result: &NetworkResult) -> Option<AtomicStructure> {
    match result {
        NetworkResult::Crystal(c) => Some(c.atoms.clone()),
        NetworkResult::Molecule(m) => Some(m.atoms.clone()),
        _ => None,
    }
}

/// Wraps an extracted diff as the `Molecule` value carried on an atom op's
/// `diff` output pin (issue #295, `doc/design_diff_outputs_for_atom_ops.md` §2).
/// Anchor arrows are shown, matching `atom_composediff` / the movement nodes.
pub fn diff_output_pin(mut diff: AtomicStructure) -> NetworkResult {
    diff.decorator_mut().show_anchor_arrows = true;
    NetworkResult::Molecule(MoleculeData {
        atoms: diff,
        geo_tree_root: None,
    })
}

/// Builds the two-pin `EvalOutput` for a `map_atomic`-based atom op that exposes
/// a `diff` pin: pin 0 is the mutation `result`, pin 1 is
/// `extract_diff(before, after)`. Errors propagate on **both** pins so diff
/// consumers never silently see `None` on pin 1 (§2). `before` is the
/// pre-mutation snapshot from [`snapshot_atoms`] (`None` for a non-atomic input,
/// which `map_atomic` already converted into an `Error` result).
pub fn eval_output_with_diff(result: NetworkResult, before: Option<AtomicStructure>) -> EvalOutput {
    if let NetworkResult::Error(_) = result {
        return EvalOutput::multi(vec![result.clone(), result]);
    }
    let after = match &result {
        NetworkResult::Crystal(c) => Some(&c.atoms),
        NetworkResult::Molecule(m) => Some(&m.atoms),
        _ => None,
    };
    match (after, before) {
        (Some(after), Some(before)) => {
            // Atom ids are stable across the in-place mutation (§1.5), so the
            // diff is an exact id-keyed comparison (ε = 0.0).
            let diff = extract_diff(&before, after, 0.0);
            EvalOutput::multi(vec![result, diff_output_pin(diff)])
        }
        // Non-atomic, non-error result — unreachable for these nodes; mirror the
        // value on both pins to stay well-formed.
        _ => EvalOutput::multi(vec![result.clone(), result]),
    }
}
