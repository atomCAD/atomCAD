//! Tests for `Structure::is_approximately_equal` — the tolerance-aware structural
//! equality used by CSG nodes to require that all inputs share the same crystal
//! field (lattice + motif + motif_offset).

use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::motif::{MotifBond, Site, SiteSpecifier};
use rust_lib_flutter_cad::crystolecule::structure::Structure;

/// Makes a Structure derived from diamond but with a custom `motif_offset`.
fn diamond_with_offset(offset: DVec3) -> Structure {
    let mut s = Structure::diamond();
    s.motif_offset = offset;
    s
}

#[test]
fn two_identical_structures_are_equal() {
    let a = Structure::diamond();
    let b = Structure::diamond();
    assert!(a.is_approximately_equal(&b));
}

#[test]
fn motif_offset_difference_below_tolerance_is_equal() {
    // 5e-10 < 1e-9 tolerance
    let a = diamond_with_offset(DVec3::ZERO);
    let b = diamond_with_offset(DVec3::new(5e-10, 0.0, 0.0));
    assert!(a.is_approximately_equal(&b));
}

#[test]
fn motif_offset_difference_above_tolerance_is_not_equal() {
    // 1e-6 > 1e-9 tolerance
    let a = diamond_with_offset(DVec3::ZERO);
    let b = diamond_with_offset(DVec3::new(1e-6, 0.0, 0.0));
    assert!(!a.is_approximately_equal(&b));
}

#[test]
fn lattice_vecs_difference_is_not_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    // Shift a vector well beyond the 1e-5 lattice tolerance.
    b.lattice_vecs.a += DVec3::new(0.1, 0.0, 0.0);
    assert!(!a.is_approximately_equal(&b));
}

#[test]
fn motif_site_count_difference_is_not_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    b.motif.sites.pop();
    assert!(!a.is_approximately_equal(&b));
}

#[test]
fn motif_site_element_difference_is_not_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    // Change the first site's element (Carbon=6 → Silicon=14).
    b.motif.sites[0].atomic_number = 14;
    assert!(!a.is_approximately_equal(&b));
}

#[test]
fn motif_site_position_difference_above_tolerance_is_not_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    // 1e-6 > 1e-9 tolerance on fractional coords.
    b.motif.sites[0].position += DVec3::new(1e-6, 0.0, 0.0);
    assert!(!a.is_approximately_equal(&b));
}

#[test]
fn motif_site_position_difference_below_tolerance_is_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    // 5e-10 < 1e-9 tolerance.
    b.motif.sites[0].position += DVec3::new(5e-10, 0.0, 0.0);
    assert!(a.is_approximately_equal(&b));
}

#[test]
fn motif_bond_count_difference_is_not_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    b.motif.bonds.pop();
    assert!(!a.is_approximately_equal(&b));
}

#[test]
fn motif_bond_multiplicity_difference_is_not_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    b.motif.bonds[0].multiplicity += 1;
    assert!(!a.is_approximately_equal(&b));
}

#[test]
fn motif_parameter_count_difference_is_not_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    b.motif.parameters.pop();
    assert!(!a.is_approximately_equal(&b));
}

#[test]
fn motif_parameter_default_element_difference_is_not_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    b.motif.parameters[0].default_atomic_number = 14;
    assert!(!a.is_approximately_equal(&b));
}

/// Field-by-field AND — a lattice mismatch is not hidden by motif equality and
/// vice versa. Not a vacuous-equal test.
#[test]
fn lattice_match_alone_does_not_imply_structure_equality() {
    // Same lattice, different motif, different offset — structures not equal.
    let mut a = Structure::diamond();
    let mut b = Structure::diamond();
    // Lattices unchanged (same diamond unit cell).
    assert!(a.lattice_vecs.is_approximately_equal(&b.lattice_vecs));
    // Introduce a motif difference.
    b.motif.sites.pop();
    // And an offset difference (1e-3 > tolerance).
    a.motif_offset = DVec3::new(1e-3, 0.0, 0.0);
    assert!(!a.is_approximately_equal(&b));
}

/// Adding a bond with different site specifiers breaks equality even when the
/// count is unchanged.
#[test]
fn motif_bond_site_specifier_difference_is_not_equal() {
    let a = Structure::diamond();
    let mut b = Structure::diamond();
    b.motif.bonds[0].site_1 = SiteSpecifier {
        site_index: 0,
        relative_cell: IVec3::new(5, 0, 0),
    };
    assert!(!a.is_approximately_equal(&b));
}

/// Tiny sanity check so the tests cover the case where Site / MotifBond types
/// are in scope (guards against accidental import removal during refactors).
#[test]
fn site_type_is_constructible() {
    let _s = Site {
        atomic_number: 6,
        position: DVec3::ZERO,
    };
    let _b = MotifBond {
        site_1: SiteSpecifier {
            site_index: 0,
            relative_cell: IVec3::ZERO,
        },
        site_2: SiteSpecifier {
            site_index: 1,
            relative_cell: IVec3::ZERO,
        },
        multiplicity: 1,
    };
}
