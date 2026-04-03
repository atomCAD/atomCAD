use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use rust_lib_flutter_cad::crystolecule::io::cif::load_cif;
use rust_lib_flutter_cad::crystolecule::motif::{Motif, MotifBond, ParameterElement, Site};
use rust_lib_flutter_cad::crystolecule::motif_bond_inference::infer_motif_bonds;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use std::collections::HashSet;

fn diamond_unit_cell() -> UnitCellStruct {
    UnitCellStruct::cubic_diamond()
}

/// Create the 8 diamond sites in the same order as DEFAULT_ZINCBLENDE_MOTIF
/// (CORNER, FACE_Z, FACE_Y, FACE_X, INTERIOR1-4), all carbon (Z=6).
fn diamond_sites() -> Vec<Site> {
    vec![
        Site { atomic_number: 6, position: DVec3::new(0.0, 0.0, 0.0) },       // CORNER
        Site { atomic_number: 6, position: DVec3::new(0.5, 0.5, 0.0) },       // FACE_Z
        Site { atomic_number: 6, position: DVec3::new(0.5, 0.0, 0.5) },       // FACE_Y
        Site { atomic_number: 6, position: DVec3::new(0.0, 0.5, 0.5) },       // FACE_X
        Site { atomic_number: 6, position: DVec3::new(0.25, 0.25, 0.25) },    // INTERIOR1
        Site { atomic_number: 6, position: DVec3::new(0.25, 0.75, 0.75) },    // INTERIOR2
        Site { atomic_number: 6, position: DVec3::new(0.75, 0.25, 0.75) },    // INTERIOR3
        Site { atomic_number: 6, position: DVec3::new(0.75, 0.75, 0.25) },    // INTERIOR4
    ]
}

/// NaCl sites: 4 Na + 4 Cl in the conventional cubic cell.
fn nacl_sites() -> Vec<Site> {
    vec![
        // Na sublattice (FCC at origin)
        Site { atomic_number: 11, position: DVec3::new(0.0, 0.0, 0.0) },
        Site { atomic_number: 11, position: DVec3::new(0.0, 0.5, 0.5) },
        Site { atomic_number: 11, position: DVec3::new(0.5, 0.0, 0.5) },
        Site { atomic_number: 11, position: DVec3::new(0.5, 0.5, 0.0) },
        // Cl sublattice (FCC at 0.5,0.5,0.5)
        Site { atomic_number: 17, position: DVec3::new(0.5, 0.5, 0.5) },
        Site { atomic_number: 17, position: DVec3::new(0.5, 0.0, 0.0) },
        Site { atomic_number: 17, position: DVec3::new(0.0, 0.5, 0.0) },
        Site { atomic_number: 17, position: DVec3::new(0.0, 0.0, 0.5) },
    ]
}

fn nacl_unit_cell() -> UnitCellStruct {
    UnitCellStruct::from_parameters(5.62, 5.62, 5.62, 90.0, 90.0, 90.0)
}

/// Represent a bond as a canonical set element for comparison.
/// (min_site, max_site, canonical_offset_x, canonical_offset_y, canonical_offset_z)
type BondKey = (usize, usize, i32, i32, i32);

fn bond_to_key(bond: &MotifBond) -> BondKey {
    let i = bond.site_1.site_index;
    let j = bond.site_2.site_index;
    let dx = bond.site_2.relative_cell.x - bond.site_1.relative_cell.x;
    let dy = bond.site_2.relative_cell.y - bond.site_1.relative_cell.y;
    let dz = bond.site_2.relative_cell.z - bond.site_1.relative_cell.z;

    if i < j {
        (i, j, dx, dy, dz)
    } else if i > j {
        (j, i, -dx, -dy, -dz)
    } else if (dx, dy, dz) >= (-dx, -dy, -dz) {
        (i, j, dx, dy, dz)
    } else {
        (j, i, -dx, -dy, -dz)
    }
}

fn bond_keys(bonds: &[MotifBond]) -> HashSet<BondKey> {
    bonds.iter().map(bond_to_key).collect()
}

// --- Diamond bond inference tests ---

#[test]
fn diamond_infer_bonds_count() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);
    assert_eq!(bonds.len(), 16, "Diamond should have 16 bonds; got {}", bonds.len());
}

#[test]
fn diamond_infer_bonds_match_zincblende_motif() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    let inferred = infer_motif_bonds(&sites, &[], &uc, 1.15);

    let expected = &DEFAULT_ZINCBLENDE_MOTIF.bonds;

    let inferred_keys = bond_keys(&inferred);
    let expected_keys = bond_keys(expected);

    assert_eq!(
        inferred_keys.len(),
        expected_keys.len(),
        "Bond count mismatch: inferred {} vs expected {}",
        inferred_keys.len(),
        expected_keys.len()
    );

    for key in &expected_keys {
        assert!(
            inferred_keys.contains(key),
            "Missing expected bond: site {} → site {} at offset ({},{},{})",
            key.0, key.1, key.2, key.3, key.4
        );
    }

    for key in &inferred_keys {
        assert!(
            expected_keys.contains(key),
            "Unexpected inferred bond: site {} → site {} at offset ({},{},{})",
            key.0, key.1, key.2, key.3, key.4
        );
    }
}

#[test]
fn diamond_infer_bonds_all_single_multiplicity() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);
    for bond in &bonds {
        assert_eq!(bond.multiplicity, 1, "All diamond bonds should be single bonds");
    }
}

#[test]
fn diamond_infer_bonds_site1_always_zero_cell() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);
    for bond in &bonds {
        assert_eq!(
            bond.site_1.relative_cell,
            IVec3::ZERO,
            "site_1 should always be in cell (0,0,0)"
        );
    }
}

#[test]
fn diamond_each_interior_bonds_to_four_neighbors() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);

    // Interior sites are indices 4-7
    for interior_idx in 4..=7 {
        let count = bonds
            .iter()
            .filter(|b| b.site_1.site_index == interior_idx || b.site_2.site_index == interior_idx)
            .count();
        assert_eq!(
            count, 4,
            "Interior site {} should have 4 bonds; got {}",
            interior_idx, count
        );
    }
}

#[test]
fn diamond_serialize_motif_text() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);

    let motif = Motif {
        parameters: vec![],
        sites: sites.clone(),
        bonds: bonds.clone(),
        bonds_by_site1_index: vec![vec![]; sites.len()],
        bonds_by_site2_index: vec![vec![]; sites.len()],
    };

    let text = motif.to_text_format();

    // Verify the text contains the expected number of bond lines
    let bond_lines: Vec<&str> = text.lines().filter(|l| l.starts_with("bond ")).collect();
    assert_eq!(bond_lines.len(), 16, "Should have 16 bond lines in serialized text");

    // Verify all 8 site lines are present
    let site_lines: Vec<&str> = text.lines().filter(|l| l.starts_with("site ")).collect();
    assert_eq!(site_lines.len(), 8, "Should have 8 site lines");
}

// --- NaCl bond inference tests ---

#[test]
fn nacl_infer_bonds_count() {
    let sites = nacl_sites();
    let uc = nacl_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);

    // NaCl: each Na bonds to 6 Cl neighbors and each Cl bonds to 6 Na neighbors.
    // In the conventional cell with 4 Na + 4 Cl, total unique bonds = 4 * 6 / 2 * 2 = 24.
    // Actually: 4 Na × 6 bonds each = 24, but each bond is shared → 24 unique bonds.
    // Wait: each Na has 6 nearest Cl neighbors. 4 Na × 6 = 24 total bond endpoints from Na side.
    // Each bond connects one Na to one Cl, so 24 unique bonds.
    assert_eq!(bonds.len(), 24, "NaCl should have 24 bonds (6 per ion); got {}", bonds.len());
}

#[test]
fn nacl_each_na_bonds_to_six_cl() {
    let sites = nacl_sites();
    let uc = nacl_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);

    // Na sites are indices 0-3
    for na_idx in 0..4 {
        let neighbors: Vec<&MotifBond> = bonds
            .iter()
            .filter(|b| b.site_1.site_index == na_idx || b.site_2.site_index == na_idx)
            .collect();
        assert_eq!(
            neighbors.len(), 6,
            "Na site {} should have 6 bonds; got {}",
            na_idx, neighbors.len()
        );

        // All neighbors should be Cl (indices 4-7)
        for bond in &neighbors {
            let other = if bond.site_1.site_index == na_idx {
                bond.site_2.site_index
            } else {
                bond.site_1.site_index
            };
            assert!(
                (4..8).contains(&other),
                "Na site {} bonded to non-Cl site {}",
                na_idx, other
            );
        }
    }
}

#[test]
fn nacl_no_same_element_bonds() {
    let sites = nacl_sites();
    let uc = nacl_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);

    for bond in &bonds {
        let z1 = sites[bond.site_1.site_index].atomic_number;
        let z2 = sites[bond.site_2.site_index].atomic_number;
        assert_ne!(
            z1, z2,
            "NaCl should not have same-element bonds: site {} (Z={}) – site {} (Z={})",
            bond.site_1.site_index, z1, bond.site_2.site_index, z2
        );
    }
}

// --- Cross-cell bond canonicalization tests ---

#[test]
fn no_duplicate_bonds() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);

    let keys = bond_keys(&bonds);
    assert_eq!(
        keys.len(),
        bonds.len(),
        "All bonds should be unique; {} bonds but {} unique keys",
        bonds.len(),
        keys.len()
    );
}

#[test]
fn no_self_bonds() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.15);

    for bond in &bonds {
        let same_site = bond.site_1.site_index == bond.site_2.site_index;
        let same_cell = bond.site_1.relative_cell == bond.site_2.relative_cell;
        assert!(
            !(same_site && same_cell),
            "Self-bond found at site {}",
            bond.site_1.site_index
        );
    }
}

// --- Tolerance tests ---

#[test]
fn lower_tolerance_fewer_bonds() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    // Very low tolerance should find no bonds
    let bonds = infer_motif_bonds(&sites, &[], &uc, 0.5);
    assert!(
        bonds.len() < 16,
        "Lower tolerance should find fewer bonds; got {}",
        bonds.len()
    );
}

#[test]
fn higher_tolerance_more_bonds() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    // Higher tolerance should find more bonds (second-nearest neighbors)
    let bonds = infer_motif_bonds(&sites, &[], &uc, 1.8);
    assert!(
        bonds.len() > 16,
        "Higher tolerance should find more bonds; got {}",
        bonds.len()
    );
}

// --- Parameter element resolution tests ---

#[test]
fn parameter_elements_resolved_for_bond_inference() {
    // Same diamond sites but using parameter element references (negative atomic_number)
    let parameters = vec![ParameterElement {
        name: "PRIMARY".to_string(),
        default_atomic_number: 6,
    }];
    let sites: Vec<Site> = diamond_sites()
        .into_iter()
        .map(|s| Site {
            atomic_number: -1, // references parameter 0 (PRIMARY)
            position: s.position,
        })
        .collect();

    let uc = diamond_unit_cell();
    let bonds = infer_motif_bonds(&sites, &parameters, &uc, 1.15);
    assert_eq!(bonds.len(), 16, "Parameter elements should resolve to carbon; got {} bonds", bonds.len());
}

// --- End-to-end CIF + bond inference tests ---

fn fixture_path(name: &str) -> String {
    format!("{}/tests/fixtures/cif/{}", env!("CARGO_MANIFEST_DIR"), name)
}

#[test]
fn diamond_cif_end_to_end_bond_inference() {
    let result = load_cif(&fixture_path("diamond.cif"), None).unwrap();

    // Convert CIF expanded atoms to motif sites
    let sites: Vec<Site> = result
        .atoms
        .iter()
        .map(|a| Site {
            atomic_number: a.atomic_number,
            position: a.fract,
        })
        .collect();

    assert_eq!(sites.len(), 8, "Diamond should have 8 expanded sites");

    let bonds = infer_motif_bonds(&sites, &[], &result.unit_cell, 1.15);
    assert_eq!(bonds.len(), 16, "Diamond CIF should produce 16 bonds; got {}", bonds.len());

    // Each atom in diamond has exactly 4 bonds (sp3 tetrahedral)
    for site_idx in 0..8 {
        let count = bonds
            .iter()
            .filter(|b| b.site_1.site_index == site_idx || b.site_2.site_index == site_idx)
            .count();
        assert_eq!(
            count, 4,
            "Diamond site {} should have 4 bonds; got {}",
            site_idx, count
        );
    }
}

#[test]
fn nacl_cif_end_to_end_bond_inference() {
    let result = load_cif(&fixture_path("nacl.cif"), None).unwrap();

    let sites: Vec<Site> = result
        .atoms
        .iter()
        .map(|a| Site {
            atomic_number: a.atomic_number,
            position: a.fract,
        })
        .collect();

    assert_eq!(sites.len(), 8);

    let bonds = infer_motif_bonds(&sites, &[], &result.unit_cell, 1.15);
    assert_eq!(bonds.len(), 24, "NaCl CIF should produce 24 bonds; got {}", bonds.len());

    // Each ion should have 6 nearest neighbors of opposite type
    for site_idx in 0..8 {
        let count = bonds
            .iter()
            .filter(|b| b.site_1.site_index == site_idx || b.site_2.site_index == site_idx)
            .count();
        assert_eq!(
            count, 6,
            "NaCl site {} should have 6 bonds; got {}",
            site_idx, count
        );
    }
}

// --- Deterministic ordering test ---

#[test]
fn bonds_are_sorted_deterministically() {
    let sites = diamond_sites();
    let uc = diamond_unit_cell();
    let bonds1 = infer_motif_bonds(&sites, &[], &uc, 1.15);
    let bonds2 = infer_motif_bonds(&sites, &[], &uc, 1.15);

    assert_eq!(bonds1.len(), bonds2.len());
    for (a, b) in bonds1.iter().zip(bonds2.iter()) {
        assert_eq!(a.site_1.site_index, b.site_1.site_index);
        assert_eq!(a.site_2.site_index, b.site_2.site_index);
        assert_eq!(a.site_1.relative_cell, b.site_1.relative_cell);
        assert_eq!(a.site_2.relative_cell, b.site_2.relative_cell);
    }
}
