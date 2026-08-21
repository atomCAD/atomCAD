//! Tests for `Structure::is_approximately_equal` — the tolerance-aware structural
//! equality used by CSG nodes to require that all inputs share the same crystal
//! field (lattice + motif + motif_offset).

use atomcad_crystolecule::motif::{MotifBond, Site, SiteSpecifier};
use atomcad_crystolecule::structure::Structure;
use glam::f64::DVec3;
use glam::i32::IVec3;

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

// ---------------------------------------------------------------------------
// Memory size estimation (`doc/design_eval_memoization.md` D6)
// ---------------------------------------------------------------------------

#[test]
fn motif_and_structure_size_above_their_bare_struct_and_grow_with_content() {
    use atomcad_crystolecule::motif::{Motif, ParameterElement};
    use atomcad_util::memory_size_estimator::MemorySizeEstimator;

    let empty = Motif {
        parameters: Vec::new(),
        sites: Vec::new(),
        bonds: Vec::new(),
        bonds_by_site1_index: Vec::new(),
        bonds_by_site2_index: Vec::new(),
    };
    assert!(empty.estimate_memory_bytes() >= std::mem::size_of::<Motif>());

    let populated = Motif {
        parameters: vec![ParameterElement {
            name: "X".to_string(),
            default_atomic_number: 6,
        }],
        sites: (0..64)
            .map(|i| Site {
                atomic_number: 6,
                position: DVec3::splat(i as f64 * 0.01),
            })
            .collect(),
        bonds: (0..32)
            .map(|i| MotifBond {
                site_1: SiteSpecifier {
                    site_index: i,
                    relative_cell: IVec3::ZERO,
                },
                site_2: SiteSpecifier {
                    site_index: i + 1,
                    relative_cell: IVec3::ZERO,
                },
                multiplicity: 1,
            })
            .collect(),
        bonds_by_site1_index: vec![vec![0usize; 4]; 64],
        bonds_by_site2_index: vec![vec![0usize; 4]; 64],
    };
    assert!(
        populated.estimate_memory_bytes() > empty.estimate_memory_bytes(),
        "a motif with sites, bonds and index maps must size above an empty one"
    );

    // A `Structure` carries its motif, so it must track it — and it must not
    // double-count the inline `Motif` header.
    let mut structure = Structure::diamond();
    let with_default_motif = structure.estimate_memory_bytes();
    structure.motif = populated.clone();
    let with_populated_motif = structure.estimate_memory_bytes();

    assert!(with_populated_motif > with_default_motif);
    assert_eq!(
        with_populated_motif,
        std::mem::size_of::<Structure>() - std::mem::size_of::<Motif>()
            + populated.estimate_memory_bytes()
    );
}
