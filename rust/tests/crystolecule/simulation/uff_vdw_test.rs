// Tests for van der Waals (nonbonded) energy implementation (Phase 1 of vdW plan).
//
// C1: vdW parameter tests (combination rules, reference comparison)
// C2: vdW energy unit tests (equilibrium, repulsive, attractive, gradient)
// C3: nonbonded pair enumeration tests (counts, exclusions, symmetry)
// C4: full force field with vdW (total energy, gradients, numerical gradient)

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DOUBLE, BOND_SINGLE, BOND_TRIPLE,
};
use rust_lib_flutter_cad::crystolecule::simulation::force_field::ForceField;
use rust_lib_flutter_cad::crystolecule::simulation::topology::MolecularTopology;
use rust_lib_flutter_cad::crystolecule::simulation::uff::{UffForceField, VdwMode};
use rust_lib_flutter_cad::crystolecule::simulation::uff::energy::{
    VdwParams, vdw_energy, vdw_energy_and_gradient,
};
use rust_lib_flutter_cad::crystolecule::simulation::uff::params::{
    calc_vdw_distance, calc_vdw_well_depth, get_uff_params,
};

// ============================================================================
// Helpers: load reference data, build structures
// ============================================================================

#[derive(serde::Deserialize)]
struct ReferenceData {
    molecules: Vec<ReferenceMolecule>,
}

#[derive(serde::Deserialize)]
struct ReferenceMolecule {
    name: String,
    atoms: Vec<ReferenceAtom>,
    bonds: Vec<ReferenceBond>,
    input_positions: Vec<[f64; 3]>,
    interaction_counts: InteractionCounts,
    input_energy: InputEnergy,
    input_gradients: InputGradients,
    #[serde(default)]
    vdw_params: Vec<ReferenceVdwParam>,
}

#[derive(serde::Deserialize)]
struct ReferenceAtom {
    atomic_number: i16,
}

#[derive(serde::Deserialize)]
struct ReferenceBond {
    atom1: usize,
    atom2: usize,
    order: f64,
}

#[derive(serde::Deserialize)]
struct InteractionCounts {
    #[allow(dead_code)]
    bonds: usize,
    #[allow(dead_code)]
    angles: usize,
    vdw_pairs: usize,
}

#[derive(serde::Deserialize)]
struct InputEnergy {
    total: f64,
    bonded: f64,
}

#[derive(serde::Deserialize)]
struct InputGradients {
    full: Vec<[f64; 3]>,
    #[allow(dead_code)]
    bonded: Vec<[f64; 3]>,
}

#[derive(serde::Deserialize)]
struct ReferenceVdwParam {
    atoms: [usize; 2],
    x_ij: f64,
    #[serde(rename = "D_ij")]
    d_ij: f64,
}

fn load_reference_data() -> ReferenceData {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/crystolecule/simulation/test_data/uff_reference.json"
    );
    let content = std::fs::read_to_string(path).expect("Failed to read uff_reference.json");
    serde_json::from_str(&content).expect("Failed to parse uff_reference.json")
}

fn bond_order_from_f64(order: f64) -> u8 {
    if (order - 1.0).abs() < 0.01 {
        BOND_SINGLE
    } else if (order - 1.5).abs() < 0.01 {
        BOND_AROMATIC
    } else if (order - 2.0).abs() < 0.01 {
        BOND_DOUBLE
    } else if (order - 3.0).abs() < 0.01 {
        BOND_TRIPLE
    } else {
        BOND_SINGLE
    }
}

fn build_structure_from_reference(mol: &ReferenceMolecule) -> AtomicStructure {
    let mut structure = AtomicStructure::new();

    for (i, atom) in mol.atoms.iter().enumerate() {
        let pos = DVec3::new(
            mol.input_positions[i][0],
            mol.input_positions[i][1],
            mol.input_positions[i][2],
        );
        let id = structure.add_atom(atom.atomic_number, pos);
        assert_eq!(id, (i + 1) as u32);
    }

    for bond in &mol.bonds {
        let order = bond_order_from_f64(bond.order);
        structure.add_bond((bond.atom1 + 1) as u32, (bond.atom2 + 1) as u32, order);
    }

    structure
}

fn build_ff_from_reference(mol: &ReferenceMolecule) -> (UffForceField, MolecularTopology) {
    let structure = build_structure_from_reference(mol);
    let topology = MolecularTopology::from_structure(&structure);
    let ff = UffForceField::from_topology(&topology)
        .unwrap_or_else(|e| panic!("Failed to build UFF for {}: {}", mol.name, e));
    (ff, topology)
}

// ============================================================================
// C1: vdW parameter tests
// ============================================================================

#[test]
fn c1_vdw_combination_rules_same_type() {
    // C_3-C_3: x_ij = 3.851, D_ij = 0.105 (same atom → geometric mean = self)
    let c3 = get_uff_params("C_3").unwrap();
    let x_ij = calc_vdw_distance(c3, c3);
    let d_ij = calc_vdw_well_depth(c3, c3);
    assert!((x_ij - 3.851).abs() < 1e-6, "C_3-C_3 x_ij: {x_ij}");
    assert!((d_ij - 0.105).abs() < 1e-6, "C_3-C_3 D_ij: {d_ij}");

    // H_-H_: x_ij = 2.886, D_ij = 0.044
    let h = get_uff_params("H_").unwrap();
    let x_ij = calc_vdw_distance(h, h);
    let d_ij = calc_vdw_well_depth(h, h);
    assert!((x_ij - 2.886).abs() < 1e-6, "H_-H_ x_ij: {x_ij}");
    assert!((d_ij - 0.044).abs() < 1e-6, "H_-H_ D_ij: {d_ij}");
}

#[test]
fn c1_vdw_combination_rules_cross_pair() {
    // C_R-H_: x_ij = sqrt(3.851 * 2.886), D_ij = sqrt(0.105 * 0.044)
    let c_r = get_uff_params("C_R").unwrap();
    let h = get_uff_params("H_").unwrap();
    let x_ij = calc_vdw_distance(c_r, h);
    let d_ij = calc_vdw_well_depth(c_r, h);

    let expected_x = (3.851_f64 * 2.886).sqrt();
    let expected_d = (0.105_f64 * 0.044).sqrt();
    assert!(
        (x_ij - expected_x).abs() < 1e-4,
        "C_R-H_ x_ij: {x_ij} != {expected_x}"
    );
    assert!(
        (d_ij - expected_d).abs() < 1e-6,
        "C_R-H_ D_ij: {d_ij} != {expected_d}"
    );
}

#[test]
fn c1_vdw_combination_rules_n3_o3() {
    let n3 = get_uff_params("N_3").unwrap();
    let o3 = get_uff_params("O_3").unwrap();
    let x_ij = calc_vdw_distance(n3, o3);
    let d_ij = calc_vdw_well_depth(n3, o3);

    let expected_x = (n3.x1 * o3.x1).sqrt();
    let expected_d = (n3.d1 * o3.d1).sqrt();
    assert!((x_ij - expected_x).abs() < 1e-10);
    assert!((d_ij - expected_d).abs() < 1e-10);
}

#[test]
fn c1_vdw_params_vs_reference() {
    // For molecules with reference vdw_params (benzene, butane, adamantane),
    // verify our x_ij and D_ij match for the 1-5+ pairs the reference lists.
    let data = load_reference_data();
    for mol in &data.molecules {
        if mol.vdw_params.is_empty() {
            continue;
        }

        let (ff, _) = build_ff_from_reference(mol);

        for ref_vdw in &mol.vdw_params {
            // Find this pair in our vdw_params (our list is a superset)
            let found = ff.vdw_params().iter().find(|vp| {
                (vp.idx1 == ref_vdw.atoms[0] && vp.idx2 == ref_vdw.atoms[1])
                    || (vp.idx1 == ref_vdw.atoms[1] && vp.idx2 == ref_vdw.atoms[0])
            });
            assert!(
                found.is_some(),
                "{}: reference pair ({}, {}) not found in our vdw_params",
                mol.name,
                ref_vdw.atoms[0],
                ref_vdw.atoms[1]
            );
            let vp = found.unwrap();
            assert!(
                (vp.x_ij - ref_vdw.x_ij).abs() < 1e-4,
                "{}: pair ({}, {}) x_ij {:.6} != ref {:.6}",
                mol.name,
                ref_vdw.atoms[0],
                ref_vdw.atoms[1],
                vp.x_ij,
                ref_vdw.x_ij
            );
            assert!(
                (vp.d_ij - ref_vdw.d_ij).abs() < 1e-6,
                "{}: pair ({}, {}) D_ij {:.6} != ref {:.6}",
                mol.name,
                ref_vdw.atoms[0],
                ref_vdw.atoms[1],
                vp.d_ij,
                ref_vdw.d_ij
            );
        }
    }
}

// ============================================================================
// C2: vdW energy unit tests
// ============================================================================

fn make_two_atom_positions(r: f64) -> Vec<f64> {
    // Atom 0 at origin, atom 1 at (r, 0, 0)
    vec![0.0, 0.0, 0.0, r, 0.0, 0.0]
}

#[test]
fn c2_vdw_energy_at_equilibrium() {
    // At r = x_ij, E = D * (-2 * 1 + 1) = -D_ij
    let c_r = get_uff_params("C_R").unwrap();
    let h = get_uff_params("H_").unwrap();
    let x_ij = calc_vdw_distance(c_r, h);
    let d_ij = calc_vdw_well_depth(c_r, h);

    let params = VdwParams {
        idx1: 0,
        idx2: 1,
        x_ij,
        d_ij,
    };
    let positions = make_two_atom_positions(x_ij);
    let e = vdw_energy(&params, &positions);
    assert!(
        (e - (-d_ij)).abs() < 1e-10,
        "At equilibrium: E={e}, expected {}",
        -d_ij
    );
}

#[test]
fn c2_vdw_energy_repulsive() {
    // At r = 0.5 * x_ij, ratio = 2, E = D * (-2*64 + 4096) = D * 3968
    let c_r = get_uff_params("C_R").unwrap();
    let h = get_uff_params("H_").unwrap();
    let x_ij = calc_vdw_distance(c_r, h);
    let d_ij = calc_vdw_well_depth(c_r, h);

    let params = VdwParams {
        idx1: 0,
        idx2: 1,
        x_ij,
        d_ij,
    };
    let r = 0.5 * x_ij;
    let positions = make_two_atom_positions(r);
    let e = vdw_energy(&params, &positions);
    let expected = d_ij * 3968.0;
    assert!(
        (e - expected).abs() < 1e-6,
        "Repulsive: E={e}, expected {expected}"
    );
    assert!(e > 0.0, "Repulsive energy should be positive");
}

#[test]
fn c2_vdw_energy_attractive() {
    // At r = 1.5 * x_ij, ratio = 1/1.5
    let c_r = get_uff_params("C_R").unwrap();
    let h = get_uff_params("H_").unwrap();
    let x_ij = calc_vdw_distance(c_r, h);
    let d_ij = calc_vdw_well_depth(c_r, h);

    let params = VdwParams {
        idx1: 0,
        idx2: 1,
        x_ij,
        d_ij,
    };
    let r = 1.5 * x_ij;
    let positions = make_two_atom_positions(r);
    let e = vdw_energy(&params, &positions);

    let ratio = 1.0 / 1.5_f64;
    let ratio6 = ratio.powi(6);
    let ratio12 = ratio6 * ratio6;
    let expected = d_ij * (-2.0 * ratio6 + ratio12);
    assert!(
        (e - expected).abs() < 1e-10,
        "Attractive: E={e}, expected {expected}"
    );
    assert!(e < 0.0, "Attractive energy should be negative");
    assert!(e > -d_ij, "Should be less negative than well depth");
}

#[test]
fn c2_vdw_energy_zero_at_infinity() {
    let c_r = get_uff_params("C_R").unwrap();
    let h = get_uff_params("H_").unwrap();
    let x_ij = calc_vdw_distance(c_r, h);
    let d_ij = calc_vdw_well_depth(c_r, h);

    let params = VdwParams {
        idx1: 0,
        idx2: 1,
        x_ij,
        d_ij,
    };
    let r = 100.0 * x_ij;
    let positions = make_two_atom_positions(r);
    let e = vdw_energy(&params, &positions);
    assert!(
        e.abs() < 1e-10,
        "At 100*x_ij: |E|={} should be tiny",
        e.abs()
    );
}

#[test]
fn c2_vdw_numerical_gradient() {
    // Central difference verification for several configurations.
    let c_r = get_uff_params("C_R").unwrap();
    let x_ij = calc_vdw_distance(c_r, c_r);
    let d_ij = calc_vdw_well_depth(c_r, c_r);
    let h = 1e-5;

    let test_configs: Vec<(&str, Vec<f64>)> = vec![
        ("0.8*x_ij", make_two_atom_positions(0.8 * x_ij)),
        ("1.0*x_ij", make_two_atom_positions(1.0 * x_ij)),
        ("1.2*x_ij", make_two_atom_positions(1.2 * x_ij)),
        ("2.0*x_ij", make_two_atom_positions(2.0 * x_ij)),
        ("diagonal", {
            let d = 1.0 * x_ij / 3.0_f64.sqrt();
            vec![0.0, 0.0, 0.0, d, d, d]
        }),
        ("non-origin", {
            let r = 1.1 * x_ij;
            vec![1.0, 2.0, 3.0, 1.0 + r, 2.0, 3.0]
        }),
    ];

    for (label, positions) in &test_configs {
        let params = VdwParams {
            idx1: 0,
            idx2: 1,
            x_ij,
            d_ij,
        };

        let n = positions.len();
        let mut analytical_grad = vec![0.0; n];
        let _energy = vdw_energy_and_gradient(&params, positions, &mut analytical_grad);

        let mut pos_plus = positions.clone();
        let mut pos_minus = positions.clone();

        for i in 0..n {
            pos_plus[i] = positions[i] + h;
            pos_minus[i] = positions[i] - h;

            let e_plus = vdw_energy(&params, &pos_plus);
            let e_minus = vdw_energy(&params, &pos_minus);

            let numerical = (e_plus - e_minus) / (2.0 * h);
            let analytical = analytical_grad[i];
            let diff = (analytical - numerical).abs();
            let ref_mag = analytical.abs().max(numerical.abs()).max(1e-8);
            let rel_error = diff / ref_mag;

            assert!(
                rel_error < 0.01 || diff < 1e-6,
                "vdW gradient [{label}] coord {i}: analytical={analytical:.8}, \
                 numerical={numerical:.8}, rel_error={rel_error:.6}"
            );

            pos_plus[i] = positions[i];
            pos_minus[i] = positions[i];
        }
    }
}

#[test]
fn c2_vdw_force_balance() {
    // Sum of gradients on both atoms should be zero (Newton's third law).
    let c_r = get_uff_params("C_R").unwrap();
    let x_ij = calc_vdw_distance(c_r, c_r);
    let d_ij = calc_vdw_well_depth(c_r, c_r);

    for factor in &[0.8, 1.0, 1.2, 2.0] {
        let params = VdwParams {
            idx1: 0,
            idx2: 1,
            x_ij,
            d_ij,
        };
        let positions = make_two_atom_positions(factor * x_ij);
        let mut gradients = vec![0.0; 6];
        vdw_energy_and_gradient(&params, &positions, &mut gradients);

        let sum_x = gradients[0] + gradients[3];
        let sum_y = gradients[1] + gradients[4];
        let sum_z = gradients[2] + gradients[5];
        let total = (sum_x * sum_x + sum_y * sum_y + sum_z * sum_z).sqrt();
        assert!(
            total < 1e-10,
            "Force balance violated at r={factor}*x_ij: |sum|={total}"
        );
    }
}

#[test]
fn c2_vdw_energy_only_matches_gradient_version() {
    let c_r = get_uff_params("C_R").unwrap();
    let x_ij = calc_vdw_distance(c_r, c_r);
    let d_ij = calc_vdw_well_depth(c_r, c_r);

    for factor in &[0.8, 1.0, 1.2, 2.0] {
        let params = VdwParams {
            idx1: 0,
            idx2: 1,
            x_ij,
            d_ij,
        };
        let positions = make_two_atom_positions(factor * x_ij);
        let e_only = vdw_energy(&params, &positions);
        let mut gradients = vec![0.0; 6];
        let e_with_grad = vdw_energy_and_gradient(&params, &positions, &mut gradients);
        assert!(
            (e_only - e_with_grad).abs() < 1e-12,
            "Energy mismatch at r={factor}*x_ij: {e_only} != {e_with_grad}"
        );
    }
}

// ============================================================================
// C3: nonbonded pair enumeration tests
// ============================================================================

#[test]
fn c3_pair_counts() {
    let data = load_reference_data();
    let expected_counts = [
        ("methane", 0),
        ("ethylene", 4),
        ("ethane", 9),
        ("benzene", 36),
        ("butane", 54),
        ("water", 0),
        ("ammonia", 0),
        ("adamantane", 237),
        ("methanethiol", 3),
    ];

    for (name, expected) in &expected_counts {
        let mol = data
            .molecules
            .iter()
            .find(|m| m.name == *name)
            .unwrap_or_else(|| panic!("Molecule {name} not found"));
        let structure = build_structure_from_reference(mol);
        let topology = MolecularTopology::from_structure(&structure);
        assert_eq!(
            topology.nonbonded_pairs.len(),
            *expected,
            "{name}: nonbonded pair count"
        );
    }
}

#[test]
fn c3_pair_exclusions_ethane() {
    // Ethane: 8 atoms, 7 bonds, 12 angles.
    // Should have 9 NB pairs: all H-H 1-4 pairs (cross-methyl).
    // Ethane topology indices: 0=C, 1=C, 2-4=H(on C0), 5-7=H(on C1)
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "ethane").unwrap();
    let structure = build_structure_from_reference(mol);
    let topology = MolecularTopology::from_structure(&structure);

    assert_eq!(topology.nonbonded_pairs.len(), 9);

    // Build bond set and angle endpoint set for exclusion verification
    let mut bond_pairs: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();
    for bond in &topology.bonds {
        bond_pairs.insert((bond.idx1.min(bond.idx2), bond.idx1.max(bond.idx2)));
    }
    let mut angle_pairs: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();
    for angle in &topology.angles {
        angle_pairs.insert((angle.idx1.min(angle.idx3), angle.idx1.max(angle.idx3)));
    }

    for pair in &topology.nonbonded_pairs {
        let key = (pair.idx1.min(pair.idx2), pair.idx1.max(pair.idx2));
        assert!(
            !bond_pairs.contains(&key),
            "NB pair ({}, {}) is a bonded pair!",
            pair.idx1,
            pair.idx2
        );
        assert!(
            !angle_pairs.contains(&key),
            "NB pair ({}, {}) is an angle endpoint pair!",
            pair.idx1,
            pair.idx2
        );
    }
}

#[test]
fn c3_pair_symmetry_benzene() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "benzene").unwrap();
    let structure = build_structure_from_reference(mol);
    let topology = MolecularTopology::from_structure(&structure);

    // All pairs have idx1 < idx2
    for pair in &topology.nonbonded_pairs {
        assert!(
            pair.idx1 < pair.idx2,
            "Pair ({}, {}) not ordered",
            pair.idx1,
            pair.idx2
        );
    }

    // No duplicate pairs
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for pair in &topology.nonbonded_pairs {
        let key = (pair.idx1, pair.idx2);
        assert!(
            seen.insert(key),
            "Duplicate pair ({}, {})",
            pair.idx1,
            pair.idx2
        );
    }
}

#[test]
fn c3_pair_counts_superset_of_reference() {
    // For molecules with reference vdw_pairs > 0, our count > reference count
    // (because we include 1-4 pairs they don't).
    let data = load_reference_data();
    let superset_molecules = [
        ("benzene", 36, 15),
        ("butane", 54, 27),
        ("adamantane", 237, 141),
    ];

    for (name, our_expected, ref_count) in &superset_molecules {
        let mol = data.molecules.iter().find(|m| m.name == *name).unwrap();
        let structure = build_structure_from_reference(mol);
        let topology = MolecularTopology::from_structure(&structure);

        assert_eq!(
            topology.nonbonded_pairs.len(),
            *our_expected,
            "{name}: our pair count"
        );
        assert_eq!(
            mol.interaction_counts.vdw_pairs, *ref_count,
            "{name}: reference pair count"
        );
        assert!(
            topology.nonbonded_pairs.len() > mol.interaction_counts.vdw_pairs,
            "{name}: our count should exceed reference count"
        );
    }
}

// ============================================================================
// C4: full force field with vdW
// ============================================================================

#[test]
fn c4_total_energy_vs_reference() {
    let data = load_reference_data();
    let tolerances = [
        ("methane", 0.01),
        ("ethylene", 0.05),
        ("ethane", 0.05),
        ("benzene", 0.1),
        ("butane", 0.1),
        ("water", 0.01),
        ("ammonia", 0.01),
        ("adamantane", 0.5),
        ("methanethiol", 0.05),
    ];

    for (name, tol) in &tolerances {
        let mol = data.molecules.iter().find(|m| m.name == *name).unwrap();
        let (ff, topology) = build_ff_from_reference(mol);
        let mut energy = 0.0;
        let mut gradients = vec![0.0; topology.positions.len()];
        ff.energy_and_gradients(&topology.positions, &mut energy, &mut gradients);

        assert!(
            (energy - mol.input_energy.total).abs() < *tol,
            "{name}: total energy {energy:.6} != expected {:.6} (diff={:.6}, tol={tol})",
            mol.input_energy.total,
            (energy - mol.input_energy.total).abs()
        );
    }
}

#[test]
fn c4_total_gradients_vs_reference() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        let mut energy = 0.0;
        let mut gradients = vec![0.0; topology.positions.len()];
        ff.energy_and_gradients(&topology.positions, &mut energy, &mut gradients);

        let tol = if mol.atoms.len() > 20 {
            0.5
        } else if mol.atoms.len() > 10 {
            0.1
        } else {
            0.05
        };

        assert_eq!(
            gradients.len(),
            mol.input_gradients.full.len() * 3,
            "{}: gradient length mismatch",
            mol.name
        );

        for (atom_idx, expected_grad) in mol.input_gradients.full.iter().enumerate() {
            for (dim_idx, dim_name) in ["x", "y", "z"].iter().enumerate() {
                let comp = gradients[atom_idx * 3 + dim_idx];
                let exp = expected_grad[dim_idx];
                let diff = (comp - exp).abs();
                let ref_mag = exp.abs().max(1.0);
                let rel_diff = diff / ref_mag;

                assert!(
                    diff < tol || rel_diff < 0.02,
                    "{}: atom {atom_idx} grad_{dim_name}: computed {comp:.6} != expected {exp:.6} \
                     (diff={diff:.6}, rel={rel_diff:.4})",
                    mol.name
                );
            }
        }
    }
}

#[test]
fn c4_numerical_gradient_benzene() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "benzene").unwrap();
    let (ff, topology) = build_ff_from_reference(mol);
    verify_numerical_gradient(&ff, &topology.positions, "benzene");
}

#[test]
fn c4_numerical_gradient_butane() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "butane").unwrap();
    let (ff, topology) = build_ff_from_reference(mol);
    verify_numerical_gradient(&ff, &topology.positions, "butane");
}

fn verify_numerical_gradient(ff: &UffForceField, positions: &[f64], mol_name: &str) {
    let n = positions.len();
    let h = 1e-5;

    let mut energy = 0.0;
    let mut analytical_grad = vec![0.0; n];
    ff.energy_and_gradients(positions, &mut energy, &mut analytical_grad);

    let mut pos_plus = positions.to_vec();
    let mut pos_minus = positions.to_vec();

    for i in 0..n {
        pos_plus[i] = positions[i] + h;
        pos_minus[i] = positions[i] - h;

        let mut e_plus = 0.0;
        let mut e_minus = 0.0;
        let mut grad_dummy = vec![0.0; n];
        ff.energy_and_gradients(&pos_plus, &mut e_plus, &mut grad_dummy);
        ff.energy_and_gradients(&pos_minus, &mut e_minus, &mut grad_dummy);

        let numerical = (e_plus - e_minus) / (2.0 * h);
        let analytical = analytical_grad[i];
        let diff = (analytical - numerical).abs();
        let ref_mag = analytical.abs().max(numerical.abs()).max(1e-8);
        let rel_error = diff / ref_mag;

        assert!(
            rel_error < 0.01 || diff < 1e-6,
            "{mol_name}: coordinate {i}: analytical={analytical:.8}, numerical={numerical:.8}, \
             rel_error={rel_error:.6}"
        );

        pos_plus[i] = positions[i];
        pos_minus[i] = positions[i];
    }
}

#[test]
fn c4_bonded_energy_unchanged() {
    // Regression test: verify bonded-only energy hasn't changed.
    // Manually sum bond/angle/torsion/inversion energies (no vdW) and compare to reference.
    use rust_lib_flutter_cad::crystolecule::simulation::uff::energy::{
        angle_bend_energy, bond_stretch_energy, inversion_energy, torsion_energy,
    };

    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        let positions = &topology.positions;

        let mut bonded_energy = 0.0;
        for bp in &ff.bond_params {
            bonded_energy += bond_stretch_energy(bp, positions);
        }
        for ap in &ff.angle_params {
            bonded_energy += angle_bend_energy(ap, positions);
        }
        for tp in &ff.torsion_params {
            bonded_energy += torsion_energy(tp, positions);
        }
        for ip in &ff.inversion_params {
            bonded_energy += inversion_energy(ip, positions);
        }

        let tol = if mol.atoms.len() > 10 { 0.1 } else { 0.01 };
        assert!(
            (bonded_energy - mol.input_energy.bonded).abs() < tol,
            "{}: bonded energy {bonded_energy:.6} != expected {:.6} (diff={:.6})",
            mol.name,
            mol.input_energy.bonded,
            (bonded_energy - mol.input_energy.bonded).abs()
        );
    }
}

#[test]
fn c4_vdw_contribution_positive_for_non_equilibrium() {
    // For molecules with vdW pairs, total - bonded ≈ input_energy.vdw
    let data = load_reference_data();
    use rust_lib_flutter_cad::crystolecule::simulation::uff::energy::{
        angle_bend_energy, bond_stretch_energy, inversion_energy, torsion_energy,
    };

    let check_molecules = ["benzene", "butane", "adamantane"];
    for name in &check_molecules {
        let mol = data.molecules.iter().find(|m| m.name == *name).unwrap();
        let (ff, topology) = build_ff_from_reference(mol);
        let positions = &topology.positions;

        // Total energy (bonded + vdW)
        let mut total_energy = 0.0;
        let mut gradients = vec![0.0; positions.len()];
        ff.energy_and_gradients(positions, &mut total_energy, &mut gradients);

        // Bonded-only energy
        let mut bonded_energy = 0.0;
        for bp in &ff.bond_params {
            bonded_energy += bond_stretch_energy(bp, positions);
        }
        for ap in &ff.angle_params {
            bonded_energy += angle_bend_energy(ap, positions);
        }
        for tp in &ff.torsion_params {
            bonded_energy += torsion_energy(tp, positions);
        }
        for ip in &ff.inversion_params {
            bonded_energy += inversion_energy(ip, positions);
        }

        let vdw_contribution = total_energy - bonded_energy;
        let ref_vdw = mol.input_energy.total - mol.input_energy.bonded;

        assert!(
            (vdw_contribution - ref_vdw).abs() < 0.5,
            "{name}: vdW contribution {vdw_contribution:.4} != ref {ref_vdw:.4} (diff={:.4})",
            (vdw_contribution - ref_vdw).abs()
        );
    }
}

// ============================================================================
// C5: AllPairs vs Cutoff mode comparison tests
// ============================================================================

fn build_ff_with_mode(
    mol: &ReferenceMolecule,
    mode: VdwMode,
) -> (UffForceField, MolecularTopology) {
    let structure = build_structure_from_reference(mol);
    let topology = MolecularTopology::from_structure(&structure);
    let ff = UffForceField::from_topology_with_vdw_mode(&topology, mode)
        .unwrap_or_else(|e| panic!("Failed to build UFF for {}: {}", mol.name, e));
    (ff, topology)
}

#[test]
fn c5_cutoff_energy_matches_allpairs_for_small_molecules() {
    // For small molecules where all atoms are well within the cutoff radius,
    // AllPairs and Cutoff modes must produce identical energies.
    let data = load_reference_data();

    // Use a large cutoff (100 A) to ensure all pairs are included.
    let cutoff = 100.0;

    for mol in &data.molecules {
        let (ff_all, topo_all) = build_ff_from_reference(mol);
        let (ff_cut, topo_cut) = build_ff_with_mode(mol, VdwMode::Cutoff(cutoff));

        let positions = &topo_all.positions;
        let n = positions.len();

        let mut energy_all = 0.0;
        let mut grad_all = vec![0.0; n];
        ff_all.energy_and_gradients(positions, &mut energy_all, &mut grad_all);

        let mut energy_cut = 0.0;
        let mut grad_cut = vec![0.0; n];
        ff_cut.energy_and_gradients(&topo_cut.positions, &mut energy_cut, &mut grad_cut);

        // Energies must match within floating-point tolerance.
        let energy_diff = (energy_all - energy_cut).abs();
        assert!(
            energy_diff < 1e-10,
            "{}: AllPairs energy {energy_all:.10} != Cutoff energy {energy_cut:.10} (diff={energy_diff:.2e})",
            mol.name
        );

        // Gradients must match within floating-point tolerance.
        for i in 0..n {
            let diff = (grad_all[i] - grad_cut[i]).abs();
            assert!(
                diff < 1e-8,
                "{}: gradient[{i}] AllPairs={:.10} != Cutoff={:.10} (diff={diff:.2e})",
                mol.name,
                grad_all[i],
                grad_cut[i]
            );
        }
    }
}

#[test]
fn c5_cutoff_numerical_gradient_verification() {
    // Verify that the cutoff-mode gradients match central-difference numerical gradients.
    let data = load_reference_data();
    let cutoff = 100.0;

    let check_molecules = ["benzene", "butane", "adamantane"];
    for name in &check_molecules {
        let mol = data.molecules.iter().find(|m| m.name == *name).unwrap();
        let (ff, topology) = build_ff_with_mode(mol, VdwMode::Cutoff(cutoff));
        let positions = &topology.positions;

        verify_numerical_gradients_cutoff(&ff, positions, name);
    }
}

fn verify_numerical_gradients_cutoff(ff: &UffForceField, positions: &[f64], mol_name: &str) {
    let n = positions.len();
    let h = 1e-5;

    let mut energy = 0.0;
    let mut analytical_grad = vec![0.0; n];
    ff.energy_and_gradients(positions, &mut energy, &mut analytical_grad);

    let mut pos_plus = positions.to_vec();
    let mut pos_minus = positions.to_vec();

    for i in 0..n {
        pos_plus[i] = positions[i] + h;
        pos_minus[i] = positions[i] - h;

        let mut e_plus = 0.0;
        let mut e_minus = 0.0;
        let mut grad_dummy = vec![0.0; n];
        ff.energy_and_gradients(&pos_plus, &mut e_plus, &mut grad_dummy);
        ff.energy_and_gradients(&pos_minus, &mut e_minus, &mut grad_dummy);

        let numerical = (e_plus - e_minus) / (2.0 * h);
        let analytical = analytical_grad[i];
        let diff = (analytical - numerical).abs();
        let ref_mag = analytical.abs().max(numerical.abs()).max(1e-8);
        let rel_error = diff / ref_mag;

        assert!(
            rel_error < 0.01 || diff < 1e-6,
            "{mol_name} cutoff: coord {i}: analytical={analytical:.8}, numerical={numerical:.8}, \
             rel_error={rel_error:.6}"
        );

        pos_plus[i] = positions[i];
        pos_minus[i] = positions[i];
    }
}

#[test]
fn c5_cutoff_with_realistic_radius() {
    // Use a realistic cutoff radius (10 A) and verify it gives results close
    // to AllPairs for molecules where all atoms fit within 10 A.
    let data = load_reference_data();
    let cutoff = 10.0;

    for mol in &data.molecules {
        let (ff_all, topo) = build_ff_from_reference(mol);
        let (ff_cut, _) = build_ff_with_mode(mol, VdwMode::Cutoff(cutoff));
        let positions = &topo.positions;
        let n = positions.len();

        let mut energy_all = 0.0;
        let mut grad_all = vec![0.0; n];
        ff_all.energy_and_gradients(positions, &mut energy_all, &mut grad_all);

        let mut energy_cut = 0.0;
        let mut grad_cut = vec![0.0; n];
        ff_cut.energy_and_gradients(positions, &mut energy_cut, &mut grad_cut);

        // For small molecules all atoms are within 10 A, so results should match.
        let energy_diff = (energy_all - energy_cut).abs();
        assert!(
            energy_diff < 1e-10,
            "{}: energy diff {energy_diff:.2e} with 10 A cutoff",
            mol.name
        );
    }
}
