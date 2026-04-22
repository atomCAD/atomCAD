//! Phase 2 unit tests for motif-symmetry detection.
//!
//! See `doc/design_blueprint_alignment.md` §5. The detection answers "does
//! this unit-cell symmetry axis also preserve the motif decoration?" — which
//! governs whether a `structure_rot` operation should degrade alignment to
//! `MotifUnaligned`.

use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::motif_symmetry::rotation_preserves_motif;
use rust_lib_flutter_cad::crystolecule::structure::Structure;

// Diamond cubic axes, as ordered by `analyze_cubic_symmetries`:
//   0..=2   : a, b, c (4-fold)
//   3..=6   : body diagonals [111], [1̄11], [11̄1], [111̄] (3-fold)
//   7..=12  : face diagonals (2-fold)

#[test]
fn four_fold_around_a_axis_is_not_a_diamond_motif_symmetry() {
    // 90° around the a-axis maps diamond's INTERIOR sites into fractional
    // positions like (0.25, 0.25, 0.75) that are not motif sites — so the
    // motif is NOT preserved even though the lattice is.
    let structure = Structure::diamond();
    assert!(!rotation_preserves_motif(
        &structure,
        Some(0),
        1,
        IVec3::ZERO
    ));
}

#[test]
fn two_steps_of_four_fold_axis_is_a_diamond_motif_symmetry() {
    // 180° around the a-axis is a proper C2 rotation of the diamond motif.
    let structure = Structure::diamond();
    assert!(rotation_preserves_motif(
        &structure,
        Some(0),
        2,
        IVec3::ZERO
    ));
}

#[test]
fn three_fold_around_body_diagonal_is_a_diamond_motif_symmetry() {
    // 120° around [111] cyclically permutes (x,y,z) → (z,x,y), which maps
    // the diamond motif to itself (INTERIOR1 fixed, INTERIOR2/3/4 permute,
    // CORNER fixed, FACE_{X,Y,Z} permute).
    let structure = Structure::diamond();
    assert!(rotation_preserves_motif(
        &structure,
        Some(3),
        1,
        IVec3::ZERO
    ));
}

#[test]
fn two_fold_face_diagonal_is_not_a_diamond_motif_symmetry() {
    // 180° around [110] sends INTERIOR1 (0.25,0.25,0.25) to (0.25,0.25,-0.25)
    // = (0.25,0.25,0.75), which is not a diamond motif site.
    let structure = Structure::diamond();
    assert!(!rotation_preserves_motif(
        &structure,
        Some(7),
        1,
        IVec3::ZERO
    ));
}

#[test]
fn identity_rotations_are_always_preserved() {
    let structure = Structure::diamond();
    // step folds to 0
    assert!(rotation_preserves_motif(
        &structure,
        Some(3),
        0,
        IVec3::ZERO
    ));
    // step folds to n_fold and wraps to 0
    assert!(rotation_preserves_motif(
        &structure,
        Some(0),
        4,
        IVec3::ZERO
    ));
    // axis_index None → identity
    assert!(rotation_preserves_motif(&structure, None, 1, IVec3::ZERO));
}

#[test]
fn pivot_does_not_affect_motif_symmetry_rotations_at_zero_offset() {
    // With motif_offset = 0, a motif-preserving rotation should stay
    // motif-preserving for any integer-lattice pivot (the pivot shift is a
    // lattice translation, which is absorbed into the cell-offset bookkeeping).
    let structure = Structure::diamond();
    assert!(rotation_preserves_motif(
        &structure,
        Some(3),
        1,
        IVec3::new(2, -1, 3)
    ));
}

#[test]
fn nonzero_motif_offset_breaks_rotation_symmetry_around_origin_pivot() {
    // A rotation that preserves the motif at offset 0 no longer does so once
    // the motif is shifted off the pivot: the rotated offset points somewhere
    // else, so site positions (= L·f + offset) don't land back on motif sites.
    let mut structure = Structure::diamond();
    structure.motif_offset = DVec3::new(0.1, 0.0, 0.0);
    assert!(!rotation_preserves_motif(
        &structure,
        Some(3),
        1,
        IVec3::ZERO
    ));
}

#[test]
fn triclinic_lattice_has_no_rotations_to_test() {
    // With no symmetry axes available, any call short-circuits to true
    // (identity only).
    use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
    let triclinic = UnitCellStruct::from_parameters(5.0, 6.0, 7.0, 75.0, 85.0, 95.0);
    let structure = Structure::from_lattice_vecs(triclinic);
    assert!(rotation_preserves_motif(
        &structure,
        Some(0),
        1,
        IVec3::ZERO
    ));
}
