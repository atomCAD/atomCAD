use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::geo_tree::GeoNode;
use crate::geo_tree::batched_implicit_evaluator::BatchedImplicitEvaluator;
use crate::structure_designer::evaluator::network_result::NetworkResult;
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
