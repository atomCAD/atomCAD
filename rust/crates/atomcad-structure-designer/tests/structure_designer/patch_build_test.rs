//! The one `patch_build` test that is genuinely node-level: it feeds the
//! extraction the two `NetworkResult` variants the `source` pin accepts.
//!
//! The extraction core itself (`extract_patch_tile`, `validate_tiling_vectors`)
//! lives in `atomcad_crystolecule::patch` and is tested there
//! (`crates/atomcad-crystolecule/tests/crystolecule/patch_build_test.rs`).

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::patch::extract_patch_tile;
use atomcad_crystolecule::structure::Structure;
use atomcad_geo_tree::GeoNode;
use atomcad_structure_designer::evaluator::network_result::{
    Alignment, CrystalData, MoleculeData, NetworkResult,
};
use glam::f64::DVec3;

const CARBON: i16 = 6;
const SINGLE: u8 = 1;

/// Counts (real atoms, patch-ghost atoms) in a structure. Deliberately a copy
/// of the helper in the crystolecule-side `patch_build_test.rs`: the two files
/// are in different crates now.
fn count_real_and_ghost(s: &AtomicStructure) -> (usize, usize) {
    let mut real = 0;
    let mut ghost = 0;
    for (_, atom) in s.iter_atoms() {
        if atom.is_patch_ghost() {
            ghost += 1;
        } else {
            real += 1;
        }
    }
    (real, ghost)
}

// ============================================================================
// 6. HasAtoms input: a Crystal source and a Molecule source carrying the same
//    atoms yield the same tile (only atoms are read).
// ============================================================================

#[test]
fn crystal_and_molecule_sources_yield_same_tile() {
    let mut slab = AtomicStructure::new();
    let a = slab.add_atom(CARBON, DVec3::new(0.0, 0.0, 0.0));
    let b = slab.add_atom(CARBON, DVec3::new(8.0, 0.0, 0.0));
    slab.add_bond(a, b, SINGLE);

    let crystal = NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: slab.clone(),
        geo_tree_root: None,
        alignment: Alignment::Aligned,
        alignment_reason: None,
    });
    let molecule = NetworkResult::Molecule(MoleculeData {
        atoms: slab.clone(),
        geo_tree_root: None,
    });

    let cut = GeoNode::sphere(DVec3::ZERO, 5.0);

    let from_crystal = extract_patch_tile(&crystal.extract_atomic().unwrap(), &cut, 0.1);
    let from_molecule = extract_patch_tile(&molecule.extract_atomic().unwrap(), &cut, 0.1);

    assert_eq!(
        from_crystal.get_num_of_atoms(),
        from_molecule.get_num_of_atoms()
    );
    assert_eq!(
        from_crystal.get_num_of_bonds(),
        from_molecule.get_num_of_bonds()
    );
    assert_eq!(
        count_real_and_ghost(&from_crystal),
        count_real_and_ghost(&from_molecule)
    );
}
