//! Phase 2 tests for `patch_build` extraction (see
//! `doc/design_surface_patches.md` §4 / §9 Phase 2).
//!
//! The "draw, don't assemble" authoring step: extract a tile from a slab + cut
//! volume (interior real atoms + outward patch-ghosts + their bonds). The tile
//! is kept in the coordinates it was drawn in (no re-expression). The extraction
//! core (`extract_patch_tile`) and the tiling-vector validation
//! (`validate_tiling_vectors`) are plain functions, tested here without the
//! node-network machinery.

use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    Alignment, CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::patch_build::{
    extract_patch_tile, validate_tiling_vectors,
};

const CARBON: i16 = 6;
const SINGLE: u8 = 1;

/// Counts (real atoms, patch-ghost atoms) in a structure.
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

/// Finds the atom whose position is within `tol` of `pos`, if any.
fn find_atom_at(
    s: &AtomicStructure,
    pos: DVec3,
    tol: f64,
) -> Option<&rust_lib_flutter_cad::crystolecule::atomic_structure::atom::Atom> {
    s.iter_atoms()
        .find(|(_, a)| (a.position - pos).length() < tol)
        .map(|(_, a)| a)
}

// ============================================================================
// 1. Interior split: SDF ≤ ε interior (real); atoms outside are excluded.
// ============================================================================

#[test]
fn interior_split_keeps_inside_atoms_only() {
    let mut slab = AtomicStructure::new();
    slab.add_atom(CARBON, DVec3::new(0.0, 0.0, 0.0)); // d = 0, inside
    slab.add_atom(CARBON, DVec3::new(3.0, 0.0, 0.0)); // d = 3, inside
    slab.add_atom(CARBON, DVec3::new(8.0, 0.0, 0.0)); // d = 8, outside, no bond

    let cut = GeoNode::sphere(DVec3::ZERO, 5.0);
    let res = extract_patch_tile(&slab, &cut, 0.1);

    // Only the two interior atoms survive; the unbonded outside atom is dropped.
    assert_eq!(res.get_num_of_atoms(), 2);
    assert_eq!(count_real_and_ghost(&res), (2, 0));
    assert!(find_atom_at(&res, DVec3::new(0.0, 0.0, 0.0), 1e-6).is_some());
    assert!(find_atom_at(&res, DVec3::new(3.0, 0.0, 0.0), 1e-6).is_some());
}

// ============================================================================
// 2. Ghost capture: an outside atom bonded to interior → patch-ghost; an
//    outside atom only reachable at distance 2 is excluded (distance-1 only).
// ============================================================================

#[test]
fn ghost_capture_is_distance_one_only() {
    let mut slab = AtomicStructure::new();
    let a = slab.add_atom(CARBON, DVec3::new(0.0, 0.0, 0.0)); // inside
    let b = slab.add_atom(CARBON, DVec3::new(8.0, 0.0, 0.0)); // outside, bonded to A
    let c = slab.add_atom(CARBON, DVec3::new(16.0, 0.0, 0.0)); // outside, bonded to B only
    slab.add_bond(a, b, SINGLE);
    slab.add_bond(b, c, SINGLE);

    let cut = GeoNode::sphere(DVec3::ZERO, 5.0);
    let res = extract_patch_tile(&slab, &cut, 0.1);

    // A (real) + B (patch-ghost). C is distance-2 → excluded.
    assert_eq!(res.get_num_of_atoms(), 2);
    assert_eq!(count_real_and_ghost(&res), (1, 1));
    let ghost = find_atom_at(&res, DVec3::new(8.0, 0.0, 0.0), 1e-6)
        .expect("the bonded outside atom B must be present");
    assert!(ghost.is_patch_ghost(), "B must be flagged patch-ghost");
    assert!(
        find_atom_at(&res, DVec3::new(16.0, 0.0, 0.0), 1e-6).is_none(),
        "distance-2 atom C must be excluded"
    );
    // The A–B bond is kept; the B–C bond is dropped (C is not in the tile).
    assert_eq!(res.get_num_of_bonds(), 1);
}

// ============================================================================
// 3. Bond selection: interior–interior and interior–ghost kept; ghost–ghost
//    dropped.
// ============================================================================

#[test]
fn bond_selection_drops_ghost_ghost() {
    let mut slab = AtomicStructure::new();
    let a = slab.add_atom(CARBON, DVec3::new(0.0, 0.0, 0.0)); // inside
    let b = slab.add_atom(CARBON, DVec3::new(3.0, 1.0, 0.0)); // inside
    let g1 = slab.add_atom(CARBON, DVec3::new(8.0, 0.0, 0.0)); // outside, bonded to A
    let g2 = slab.add_atom(CARBON, DVec3::new(8.0, 3.0, 0.0)); // outside, bonded to B + G1
    slab.add_bond(a, b, SINGLE); // interior–interior
    slab.add_bond(a, g1, SINGLE); // interior–ghost
    slab.add_bond(b, g2, SINGLE); // interior–ghost
    slab.add_bond(g1, g2, SINGLE); // ghost–ghost (must be dropped)

    let cut = GeoNode::sphere(DVec3::ZERO, 5.0);
    let res = extract_patch_tile(&slab, &cut, 0.1);

    assert_eq!(res.get_num_of_atoms(), 4);
    assert_eq!(count_real_and_ghost(&res), (2, 2));
    // A–B, A–G1, B–G2 kept; G1–G2 dropped → 3 bonds.
    assert_eq!(res.get_num_of_bonds(), 3);
}

// ============================================================================
// 4. Shared-boundary closure: an atom exactly on the cut surface (SDF = 0 ≤ ε)
//    is interior in *both* adjacent tiles — i.e. real, not a ghost.
// ============================================================================

#[test]
fn atom_on_cut_surface_is_interior_real() {
    let mut slab = AtomicStructure::new();
    slab.add_atom(CARBON, DVec3::new(0.0, 0.0, 0.0)); // inside
    slab.add_atom(CARBON, DVec3::new(5.0, 0.0, 0.0)); // exactly on the r=5 surface

    let cut = GeoNode::sphere(DVec3::ZERO, 5.0);
    let res = extract_patch_tile(&slab, &cut, 0.1);

    assert_eq!(res.get_num_of_atoms(), 2);
    assert_eq!(count_real_and_ghost(&res), (2, 0));
    let on_surface =
        find_atom_at(&res, DVec3::new(5.0, 0.0, 0.0), 1e-6).expect("surface atom must be present");
    assert!(
        !on_surface.is_patch_ghost(),
        "an atom on the shared cut face is a real boundary atom, not a ghost"
    );
}

// ============================================================================
// 5. Coordinate frame: the extracted atoms keep their **authored absolute**
//    coordinates — no re-expression. This is what makes `patch_latticefill`'s
//    default `origin` reproduce the reconstruction in place.
// ============================================================================

#[test]
fn coordinate_frame_is_absolute_authored() {
    // Two interior atoms drawn at arbitrary absolute positions (deliberately
    // not near the lattice origin).
    let a_real = DVec3::new(12.5, 1.0, 1.0);
    let b_real = DVec3::new(22.5, 1.0, 1.0);

    let mut slab = AtomicStructure::new();
    slab.add_atom(CARBON, a_real);
    slab.add_atom(CARBON, b_real);

    // Sphere covering both atoms; centred between them.
    let cut = GeoNode::sphere(DVec3::new(17.5, 1.0, 1.0), 8.0);
    let res = extract_patch_tile(&slab, &cut, 0.1);

    // Both atoms remain exactly where they were drawn — coordinates untouched.
    assert!(
        find_atom_at(&res, a_real, 1e-9).is_some(),
        "atom A keeps its authored absolute position"
    );
    assert!(
        find_atom_at(&res, b_real, 1e-9).is_some(),
        "atom B keeps its authored absolute position"
    );
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

// ============================================================================
// 7. Validation: 1 ≤ len ≤ 3 and linear independence; degenerate vectors error.
// ============================================================================

#[test]
fn tiling_vector_count_is_bounded() {
    assert!(validate_tiling_vectors(&[]).is_err());
    assert!(validate_tiling_vectors(&[IVec3::new(1, 0, 0)]).is_ok());
    assert!(
        validate_tiling_vectors(&[
            IVec3::new(1, 0, 0),
            IVec3::new(0, 1, 0),
            IVec3::new(0, 0, 1),
            IVec3::new(1, 1, 1),
        ])
        .is_err()
    );
}

#[test]
fn single_zero_tiling_vector_is_degenerate() {
    assert!(validate_tiling_vectors(&[IVec3::ZERO]).is_err());
}

#[test]
fn two_dependent_tiling_vectors_error() {
    // Collinear → linearly dependent.
    assert!(validate_tiling_vectors(&[IVec3::new(1, 0, 0), IVec3::new(2, 0, 0)]).is_err());
    // Independent pair is fine.
    assert!(validate_tiling_vectors(&[IVec3::new(1, 0, 0), IVec3::new(0, 1, 0)]).is_ok());
}

#[test]
fn three_coplanar_tiling_vectors_error() {
    // The third is a combination of the first two (all in z = 0) → dependent.
    assert!(
        validate_tiling_vectors(&[
            IVec3::new(1, 0, 0),
            IVec3::new(0, 1, 0),
            IVec3::new(1, 1, 0),
        ])
        .is_err()
    );
    // A genuine 3D basis is fine.
    assert!(
        validate_tiling_vectors(&[
            IVec3::new(1, 0, 0),
            IVec3::new(0, 1, 0),
            IVec3::new(0, 0, 1),
        ])
        .is_ok()
    );
}
