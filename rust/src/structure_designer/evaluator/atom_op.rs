use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::structure_designer::evaluator::network_result::NetworkResult;

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
