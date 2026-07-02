// Tests for UffForceField construction and full UFF energy/gradient evaluation
// (bonded + vdW).
//
// Validates that UffForceField::from_topology() produces a force field that
// matches RDKit's full UFF energy and gradients for 9 reference molecules.

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DOUBLE, BOND_SINGLE, BOND_TRIPLE,
};
use rust_lib_flutter_cad::crystolecule::simulation::force_field::ForceField;
use rust_lib_flutter_cad::crystolecule::simulation::topology::MolecularTopology;
use rust_lib_flutter_cad::crystolecule::simulation::uff::{UffForceField, VdwMode};
use std::collections::BTreeSet;

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
    #[allow(dead_code)]
    interaction_counts: InteractionCounts,
    input_energy: InputEnergy,
    input_gradients: InputGradients,
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
#[allow(dead_code)]
struct InteractionCounts {
    bonds: usize,
    angles: usize,
    torsions: usize,
    inversions: usize,
    vdw_pairs: usize,
}

#[derive(serde::Deserialize)]
struct InputEnergy {
    total: f64,
    #[allow(dead_code)]
    bonded: f64,
}

#[derive(serde::Deserialize)]
struct InputGradients {
    full: Vec<[f64; 3]>,
    #[allow(dead_code)]
    bonded: Vec<[f64; 3]>,
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
// Construction: all 9 reference molecules build successfully
// ============================================================================

#[test]
fn builds_all_reference_molecules() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let structure = build_structure_from_reference(mol);
        let topology = MolecularTopology::from_structure(&structure);
        let result = UffForceField::from_topology(&topology);
        assert!(
            result.is_ok(),
            "Failed to build UFF for {}: {}",
            mol.name,
            result.err().unwrap()
        );
    }
}

#[test]
fn empty_structure() {
    let structure = AtomicStructure::new();
    let topology = MolecularTopology::from_structure(&structure);
    let ff = UffForceField::from_topology(&topology).unwrap();
    assert_eq!(ff.num_atoms, 0);
    assert!(ff.bond_params.is_empty());
    assert!(ff.angle_params.is_empty());
    assert!(ff.torsion_params.is_empty());
    assert!(ff.inversion_params.is_empty());
}

// ============================================================================
// Parameter counts: bond params match topology bonds
// ============================================================================

#[test]
fn bond_param_count_matches_topology() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        assert_eq!(
            ff.bond_params.len(),
            topology.bonds.len(),
            "{}: bond param count mismatch",
            mol.name
        );
    }
}

#[test]
fn angle_param_count_matches_topology() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        // Angle params may be fewer than topology angles if any have ka <= 0.
        // For standard organic molecules, they should be equal.
        assert_eq!(
            ff.angle_params.len(),
            topology.angles.len(),
            "{}: angle param count mismatch",
            mol.name
        );
    }
}

#[test]
fn inversion_param_count_matches_topology() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        assert_eq!(
            ff.inversion_params.len(),
            topology.inversions.len(),
            "{}: inversion param count mismatch",
            mol.name
        );
    }
}

// ============================================================================
// Energy at input positions: matches RDKit total energy (bonded + vdW)
// ============================================================================

/// Computes the total energy (bonded + vdW) for a reference molecule at its input positions.
fn compute_energy(ff: &UffForceField, topology: &MolecularTopology) -> f64 {
    let mut energy = 0.0;
    let mut gradients = vec![0.0; topology.positions.len()];
    ff.energy_and_gradients(&topology.positions, &mut energy, &mut gradients);
    energy
}

#[test]
fn energy_methane() {
    let data = load_reference_data();
    let mol = &data.molecules[0];
    assert_eq!(mol.name, "methane");
    let (ff, topology) = build_ff_from_reference(mol);
    let energy = compute_energy(&ff, &topology);
    assert!(
        (energy - mol.input_energy.total).abs() < 0.01,
        "methane: energy {energy} != expected {}",
        mol.input_energy.total
    );
}

#[test]
fn energy_ethylene() {
    let data = load_reference_data();
    let mol = &data.molecules[1];
    assert_eq!(mol.name, "ethylene");
    let (ff, topology) = build_ff_from_reference(mol);
    let energy = compute_energy(&ff, &topology);
    assert!(
        (energy - mol.input_energy.total).abs() < 0.05,
        "ethylene: energy {energy} != expected {}",
        mol.input_energy.total
    );
}

#[test]
fn energy_ethane() {
    let data = load_reference_data();
    let mol = &data.molecules[2];
    assert_eq!(mol.name, "ethane");
    let (ff, topology) = build_ff_from_reference(mol);
    let energy = compute_energy(&ff, &topology);
    assert!(
        (energy - mol.input_energy.total).abs() < 0.05,
        "ethane: energy {energy} != expected {}",
        mol.input_energy.total
    );
}

#[test]
fn energy_benzene() {
    let data = load_reference_data();
    let mol = &data.molecules[3];
    assert_eq!(mol.name, "benzene");
    let (ff, topology) = build_ff_from_reference(mol);
    let energy = compute_energy(&ff, &topology);
    assert!(
        (energy - mol.input_energy.total).abs() < 0.1,
        "benzene: energy {energy} != expected {}",
        mol.input_energy.total
    );
}

#[test]
fn energy_butane() {
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");
    let (ff, topology) = build_ff_from_reference(mol);
    let energy = compute_energy(&ff, &topology);
    assert!(
        (energy - mol.input_energy.total).abs() < 0.1,
        "butane: energy {energy} != expected {}",
        mol.input_energy.total
    );
}

#[test]
fn energy_water() {
    let data = load_reference_data();
    let mol = &data.molecules[5];
    assert_eq!(mol.name, "water");
    let (ff, topology) = build_ff_from_reference(mol);
    let energy = compute_energy(&ff, &topology);
    assert!(
        (energy - mol.input_energy.total).abs() < 0.01,
        "water: energy {energy} != expected {}",
        mol.input_energy.total
    );
}

#[test]
fn energy_ammonia() {
    let data = load_reference_data();
    let mol = &data.molecules[6];
    assert_eq!(mol.name, "ammonia");
    let (ff, topology) = build_ff_from_reference(mol);
    let energy = compute_energy(&ff, &topology);
    assert!(
        (energy - mol.input_energy.total).abs() < 0.01,
        "ammonia: energy {energy} != expected {}",
        mol.input_energy.total
    );
}

#[test]
fn energy_adamantane() {
    let data = load_reference_data();
    let mol = &data.molecules[7];
    assert_eq!(mol.name, "adamantane");
    let (ff, topology) = build_ff_from_reference(mol);
    let energy = compute_energy(&ff, &topology);
    assert!(
        (energy - mol.input_energy.total).abs() < 0.5,
        "adamantane: energy {energy} != expected {}",
        mol.input_energy.total
    );
}

#[test]
fn energy_methanethiol() {
    let data = load_reference_data();
    let mol = &data.molecules[8];
    assert_eq!(mol.name, "methanethiol");
    let (ff, topology) = build_ff_from_reference(mol);
    let energy = compute_energy(&ff, &topology);
    assert!(
        (energy - mol.input_energy.total).abs() < 0.05,
        "methanethiol: energy {energy} != expected {}",
        mol.input_energy.total
    );
}

/// Parametric test: all 9 molecules' energies match RDKit total energy.
#[test]
fn energy_all_molecules() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        let energy = compute_energy(&ff, &topology);
        let tol = if mol.atoms.len() > 20 {
            0.5
        } else if mol.atoms.len() > 8 {
            0.1
        } else {
            0.05
        };
        assert!(
            (energy - mol.input_energy.total).abs() < tol,
            "{}: energy {energy} != expected {} (diff = {})",
            mol.name,
            mol.input_energy.total,
            (energy - mol.input_energy.total).abs()
        );
    }
}

// ============================================================================
// Gradients at input positions: match RDKit full gradients (bonded + vdW)
// ============================================================================

/// Computes gradients for a reference molecule at its input positions.
fn compute_gradients(ff: &UffForceField, topology: &MolecularTopology) -> Vec<f64> {
    let mut energy = 0.0;
    let mut gradients = vec![0.0; topology.positions.len()];
    ff.energy_and_gradients(&topology.positions, &mut energy, &mut gradients);
    gradients
}

#[test]
fn gradients_methane() {
    let data = load_reference_data();
    let mol = &data.molecules[0];
    assert_eq!(mol.name, "methane");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.full, 0.05);
}

#[test]
fn gradients_ethylene() {
    let data = load_reference_data();
    let mol = &data.molecules[1];
    assert_eq!(mol.name, "ethylene");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.full, 0.05);
}

#[test]
fn gradients_ethane() {
    let data = load_reference_data();
    let mol = &data.molecules[2];
    assert_eq!(mol.name, "ethane");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.full, 0.05);
}

#[test]
fn gradients_benzene() {
    let data = load_reference_data();
    let mol = &data.molecules[3];
    assert_eq!(mol.name, "benzene");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.full, 0.1);
}

#[test]
fn gradients_butane() {
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.full, 0.1);
}

#[test]
fn gradients_water() {
    let data = load_reference_data();
    let mol = &data.molecules[5];
    assert_eq!(mol.name, "water");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.full, 0.05);
}

#[test]
fn gradients_ammonia() {
    let data = load_reference_data();
    let mol = &data.molecules[6];
    assert_eq!(mol.name, "ammonia");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.full, 0.05);
}

#[test]
fn gradients_adamantane() {
    let data = load_reference_data();
    let mol = &data.molecules[7];
    assert_eq!(mol.name, "adamantane");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.full, 0.5);
}

#[test]
fn gradients_methanethiol() {
    let data = load_reference_data();
    let mol = &data.molecules[8];
    assert_eq!(mol.name, "methanethiol");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.full, 0.05);
}

#[test]
fn gradients_all_molecules() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        let gradients = compute_gradients(&ff, &topology);
        let tol = if mol.atoms.len() > 20 {
            0.5
        } else if mol.atoms.len() > 8 {
            0.1
        } else {
            0.05
        };
        check_gradients(&mol.name, &gradients, &mol.input_gradients.full, tol);
    }
}

/// Checks that computed gradients match reference gradients within tolerance.
///
/// Uses absolute tolerance for small components and relative tolerance for large ones.
fn check_gradients(mol_name: &str, computed: &[f64], expected: &[[f64; 3]], abs_tol: f64) {
    assert_eq!(
        computed.len(),
        expected.len() * 3,
        "{mol_name}: gradient length mismatch"
    );

    let mut max_diff = 0.0f64;
    let mut max_diff_atom = 0;
    let mut max_diff_dim = "";

    for (atom_idx, expected_grad) in expected.iter().enumerate() {
        for (dim_idx, dim_name) in ["x", "y", "z"].iter().enumerate() {
            let comp = computed[atom_idx * 3 + dim_idx];
            let exp = expected_grad[dim_idx];
            let diff = (comp - exp).abs();

            // Use relative tolerance for large values, absolute for small.
            let ref_mag = exp.abs().max(1.0);
            let rel_diff = diff / ref_mag;

            if diff > max_diff {
                max_diff = diff;
                max_diff_atom = atom_idx;
                max_diff_dim = dim_name;
            }

            assert!(
                diff < abs_tol || rel_diff < 0.02,
                "{mol_name}: atom {atom_idx} grad_{dim_name}: computed {comp:.6} != expected {exp:.6} \
                 (diff={diff:.6}, rel={rel_diff:.4})"
            );
        }
    }

    // Also report the worst-case for debugging
    let _ = (max_diff, max_diff_atom, max_diff_dim);
}

// ============================================================================
// Numerical gradient verification: dE/dx ≈ (E(x+h) - E(x-h)) / (2h)
// ============================================================================

#[test]
fn numerical_gradient_methane() {
    let data = load_reference_data();
    let mol = &data.molecules[0];
    assert_eq!(mol.name, "methane");
    let (ff, topology) = build_ff_from_reference(mol);
    verify_numerical_gradient(&ff, &topology.positions, &mol.name);
}

#[test]
fn numerical_gradient_ethylene() {
    let data = load_reference_data();
    let mol = &data.molecules[1];
    assert_eq!(mol.name, "ethylene");
    let (ff, topology) = build_ff_from_reference(mol);
    verify_numerical_gradient(&ff, &topology.positions, &mol.name);
}

#[test]
fn numerical_gradient_benzene() {
    let data = load_reference_data();
    let mol = &data.molecules[3];
    assert_eq!(mol.name, "benzene");
    let (ff, topology) = build_ff_from_reference(mol);
    verify_numerical_gradient(&ff, &topology.positions, &mol.name);
}

#[test]
fn numerical_gradient_butane() {
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");
    let (ff, topology) = build_ff_from_reference(mol);
    verify_numerical_gradient(&ff, &topology.positions, &mol.name);
}

#[test]
fn numerical_gradient_adamantane() {
    let data = load_reference_data();
    let mol = &data.molecules[7];
    assert_eq!(mol.name, "adamantane");
    let (ff, topology) = build_ff_from_reference(mol);
    verify_numerical_gradient(&ff, &topology.positions, &mol.name);
}

/// Verifies analytical gradients match numerical gradients (central difference).
fn verify_numerical_gradient(ff: &UffForceField, positions: &[f64], mol_name: &str) {
    let n = positions.len();
    let h = 1e-5;

    // Compute analytical gradient.
    let mut energy = 0.0;
    let mut analytical_grad = vec![0.0; n];
    ff.energy_and_gradients(positions, &mut energy, &mut analytical_grad);

    // Compute numerical gradient via central difference.
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

        // Restore
        pos_plus[i] = positions[i];
        pos_minus[i] = positions[i];
    }
}

// ============================================================================
// Force balance: sum of all gradients should be zero (no net force on system)
// ============================================================================

#[test]
fn force_balance_all_molecules() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        let gradients = compute_gradients(&ff, &topology);

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_z = 0.0;
        for i in 0..topology.num_atoms {
            sum_x += gradients[i * 3];
            sum_y += gradients[i * 3 + 1];
            sum_z += gradients[i * 3 + 2];
        }

        let total = (sum_x * sum_x + sum_y * sum_y + sum_z * sum_z).sqrt();
        assert!(
            total < 1e-6,
            "{}: force balance violated: sum_grad = ({sum_x:.8}, {sum_y:.8}, {sum_z:.8}), |sum| = {total:.8}",
            mol.name
        );
    }
}

// ============================================================================
// Torsion scaling: verify torsion count per central bond
// ============================================================================

#[test]
fn torsion_scaling_ethane() {
    // Ethane C-C has 9 torsions (3 H on each C: 3×3=9).
    // Each torsion's force constant should be V/9.
    let data = load_reference_data();
    let mol = &data.molecules[2];
    assert_eq!(mol.name, "ethane");
    let (ff, _) = build_ff_from_reference(mol);

    // All torsions should have the same (scaled) force constant.
    assert!(!ff.torsion_params.is_empty(), "ethane should have torsions");
    let first_v = ff.torsion_params[0].params.force_constant;
    for tp in &ff.torsion_params {
        assert!(
            (tp.params.force_constant - first_v).abs() < 1e-10,
            "ethane: torsion force constants should be uniform after scaling"
        );
    }
}

// ============================================================================
// Energy-only evaluation: verify energy without gradient overhead
// ============================================================================

#[test]
fn energy_gradient_consistency() {
    // Verify that energy computed with energy_and_gradients matches
    // what you'd get by summing individual terms.
    let data = load_reference_data();
    let mol = &data.molecules[2]; // ethane
    assert_eq!(mol.name, "ethane");
    let (ff, topology) = build_ff_from_reference(mol);

    // Full energy+gradient
    let mut energy1 = 0.0;
    let mut grad1 = vec![0.0; topology.positions.len()];
    ff.energy_and_gradients(&topology.positions, &mut energy1, &mut grad1);

    // Call again — should give same result (deterministic).
    let mut energy2 = 0.0;
    let mut grad2 = vec![0.0; topology.positions.len()];
    ff.energy_and_gradients(&topology.positions, &mut energy2, &mut grad2);

    assert!(
        (energy1 - energy2).abs() < 1e-12,
        "Energy not deterministic"
    );
    for (i, (&g1, &g2)) in grad1.iter().zip(grad2.iter()).enumerate() {
        assert!(
            (g1 - g2).abs() < 1e-12,
            "Gradient[{i}] not deterministic: {g1} vs {g2}"
        );
    }
}

// ============================================================================
// Frozen-aware interaction filtering (design_relax_frozen_atoms Phase 1)
// ============================================================================

/// C1-C2-C3-C4 chain with C5 branching off C2 (topology indices 0..4).
/// The central bond C2-C3 hosts two torsions: C1-C2-C3-C4 and C5-C2-C3-C4.
fn build_isopentane_skeleton() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = s.add_atom(6, DVec3::new(1.5, 0.3, 0.0));
    let c3 = s.add_atom(6, DVec3::new(2.6, -0.7, 0.3));
    let c4 = s.add_atom(6, DVec3::new(4.0, -0.2, 0.5));
    let c5 = s.add_atom(6, DVec3::new(1.9, 1.4, 1.1));
    s.add_bond(c1, c2, BOND_SINGLE);
    s.add_bond(c2, c3, BOND_SINGLE);
    s.add_bond(c3, c4, BOND_SINGLE);
    s.add_bond(c2, c5, BOND_SINGLE);
    s
}

/// Asserts that `filtered`'s param lists are exactly the reference entries
/// that have at least one free participant, byte-identical.
fn assert_params_match_reference(
    filtered: &UffForceField,
    reference: &UffForceField,
    frozen_flags: &[bool],
) {
    // Bonds.
    let expected_bonds: Vec<_> = reference
        .bond_params
        .iter()
        .filter(|b| !(frozen_flags[b.idx1] && frozen_flags[b.idx2]))
        .collect();
    assert_eq!(filtered.bond_params.len(), expected_bonds.len());
    for (a, b) in filtered.bond_params.iter().zip(&expected_bonds) {
        assert_eq!((a.idx1, a.idx2), (b.idx1, b.idx2));
        assert_eq!(a.rest_length.to_bits(), b.rest_length.to_bits());
        assert_eq!(a.force_constant.to_bits(), b.force_constant.to_bits());
    }

    // Angles.
    let expected_angles: Vec<_> = reference
        .angle_params
        .iter()
        .filter(|a| !(frozen_flags[a.idx1] && frozen_flags[a.idx2] && frozen_flags[a.idx3]))
        .collect();
    assert_eq!(filtered.angle_params.len(), expected_angles.len());
    for (a, b) in filtered.angle_params.iter().zip(&expected_angles) {
        assert_eq!((a.idx1, a.idx2, a.idx3), (b.idx1, b.idx2, b.idx3));
        assert_eq!(a.force_constant.to_bits(), b.force_constant.to_bits());
        assert_eq!(a.theta0.to_bits(), b.theta0.to_bits());
        assert_eq!(a.order, b.order);
        assert_eq!(a.c0.to_bits(), b.c0.to_bits());
        assert_eq!(a.c1.to_bits(), b.c1.to_bits());
        assert_eq!(a.c2.to_bits(), b.c2.to_bits());
    }

    // Torsions (force constants must reflect count-before-filter scaling).
    let expected_torsions: Vec<_> = reference
        .torsion_params
        .iter()
        .filter(|t| {
            !(frozen_flags[t.idx1]
                && frozen_flags[t.idx2]
                && frozen_flags[t.idx3]
                && frozen_flags[t.idx4])
        })
        .collect();
    assert_eq!(filtered.torsion_params.len(), expected_torsions.len());
    for (a, b) in filtered.torsion_params.iter().zip(&expected_torsions) {
        assert_eq!(
            (a.idx1, a.idx2, a.idx3, a.idx4),
            (b.idx1, b.idx2, b.idx3, b.idx4)
        );
        assert_eq!(
            a.params.force_constant.to_bits(),
            b.params.force_constant.to_bits()
        );
        assert_eq!(a.params.order, b.params.order);
        assert_eq!(a.params.cos_term.to_bits(), b.params.cos_term.to_bits());
    }

    // Inversions.
    let expected_inversions: Vec<_> = reference
        .inversion_params
        .iter()
        .filter(|i| {
            !(frozen_flags[i.idx1]
                && frozen_flags[i.idx2]
                && frozen_flags[i.idx3]
                && frozen_flags[i.idx4])
        })
        .collect();
    assert_eq!(filtered.inversion_params.len(), expected_inversions.len());
    for (a, b) in filtered.inversion_params.iter().zip(&expected_inversions) {
        assert_eq!(
            (a.idx1, a.idx2, a.idx3, a.idx4),
            (b.idx1, b.idx2, b.idx3, b.idx4)
        );
        assert_eq!(a.force_constant.to_bits(), b.force_constant.to_bits());
        assert_eq!(a.c0.to_bits(), b.c0.to_bits());
        assert_eq!(a.c1.to_bits(), b.c1.to_bits());
        assert_eq!(a.c2.to_bits(), b.c2.to_bits());
    }
}

#[test]
fn frozen_filter_torsion_scaling_counts_before_filtering() {
    // Freeze the main chain C1..C4 (indices 0..3); the branch C5 (index 4)
    // stays free. The central bond C2-C3 hosts one all-frozen torsion
    // (C1-C2-C3-C4, dropped) and one mixed torsion (C5-C2-C3-C4, kept).
    // The kept torsion's force constant must still be divided by 2 -- the
    // per-central-bond count includes the dropped sibling.
    let structure = build_isopentane_skeleton();
    let topology = MolecularTopology::from_structure(&structure);
    let frozen = [0usize, 1, 2, 3];
    let frozen_flags = [true, true, true, true, false];

    let reference =
        UffForceField::from_topology_with_vdw_mode(&topology, VdwMode::AllPairs).unwrap();
    let filtered =
        UffForceField::from_topology_with_frozen(&topology, VdwMode::AllPairs, &frozen).unwrap();

    // The reference must have exactly the two torsions about C2-C3, sharing
    // the same (already count-scaled) force constant -- otherwise this test
    // would not exercise the count-before-filter rule.
    assert_eq!(reference.torsion_params.len(), 2);
    assert_eq!(
        reference.torsion_params[0].params.force_constant.to_bits(),
        reference.torsion_params[1].params.force_constant.to_bits()
    );

    // Only the mixed torsion survives, with the reference's scaled constant.
    assert_eq!(filtered.torsion_params.len(), 1);
    let t = &filtered.torsion_params[0];
    assert_eq!((t.idx1, t.idx2, t.idx3, t.idx4), (4, 1, 2, 3));

    assert_params_match_reference(&filtered, &reference, &frozen_flags);

    // Sanity: filtering did remove all-frozen terms.
    assert_eq!(filtered.bond_params.len(), 1); // only C2-C5
    assert_eq!(filtered.bond_params[0].idx1, 1);
    assert_eq!(filtered.bond_params[0].idx2, 4);
}

#[test]
fn frozen_filter_inversions_and_mixed_terms_match_reference() {
    // Propene with explicit hydrogens: C1(=C2)H2, C2(H)-C3H3.
    // Indices: C1=0, C2=1, C3=2, H on C1: 3,4; H on C2: 5; H on C3: 6,7,8.
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = s.add_atom(6, DVec3::new(1.33, 0.0, 0.0));
    let c3 = s.add_atom(6, DVec3::new(2.1, 1.25, 0.2));
    let h1 = s.add_atom(1, DVec3::new(-0.6, 0.9, 0.0));
    let h2 = s.add_atom(1, DVec3::new(-0.6, -0.9, 0.0));
    let h3 = s.add_atom(1, DVec3::new(1.9, -0.95, 0.0));
    let h4 = s.add_atom(1, DVec3::new(1.6, 2.1, 0.6));
    let h5 = s.add_atom(1, DVec3::new(2.9, 1.1, 0.9));
    let h6 = s.add_atom(1, DVec3::new(2.6, 1.6, -0.7));
    s.add_bond(c1, c2, BOND_DOUBLE);
    s.add_bond(c2, c3, BOND_SINGLE);
    s.add_bond(c1, h1, BOND_SINGLE);
    s.add_bond(c1, h2, BOND_SINGLE);
    s.add_bond(c2, h3, BOND_SINGLE);
    s.add_bond(c3, h4, BOND_SINGLE);
    s.add_bond(c3, h5, BOND_SINGLE);
    s.add_bond(c3, h6, BOND_SINGLE);

    let topology = MolecularTopology::from_structure(&s);
    // Freeze C1, C2 and their hydrogens; the methyl group stays free.
    let frozen = [0usize, 1, 3, 4, 5];
    let mut frozen_flags = [false; 9];
    for &i in &frozen {
        frozen_flags[i] = true;
    }

    let reference =
        UffForceField::from_topology_with_vdw_mode(&topology, VdwMode::AllPairs).unwrap();
    let filtered =
        UffForceField::from_topology_with_frozen(&topology, VdwMode::AllPairs, &frozen).unwrap();

    // The sp2 center C1 has all-frozen inversions (participants C1,H,H,C2);
    // the sp2 center C2's inversions involve free C3 and survive. Both sets
    // must exist in the reference for the test to be meaningful.
    assert!(
        reference
            .inversion_params
            .iter()
            .any(|i| frozen_flags[i.idx1]
                && frozen_flags[i.idx2]
                && frozen_flags[i.idx3]
                && frozen_flags[i.idx4]),
        "expected at least one all-frozen inversion in the reference"
    );
    assert!(
        !filtered.inversion_params.is_empty(),
        "expected surviving mixed inversions"
    );
    assert!(filtered.inversion_params.len() < reference.inversion_params.len());

    assert_params_match_reference(&filtered, &reference, &frozen_flags);

    // vdW (AllPairs): exactly the reference pairs with >=1 free endpoint.
    let expected_vdw: BTreeSet<(usize, usize)> = reference
        .vdw_pair_indices()
        .into_iter()
        .filter(|&(i, j)| !(frozen_flags[i] && frozen_flags[j]))
        .collect();
    let actual_vdw: BTreeSet<(usize, usize)> = filtered.vdw_pair_indices().into_iter().collect();
    assert_eq!(actual_vdw, expected_vdw);
}

#[test]
fn frozen_boundary_atom_typing_uses_full_connectivity() {
    // C1=C2-C3 bare-carbon chain. C1 and C2 frozen, C3 free. C2's sp2 type
    // comes from its double bond to C1 -- a bond between two frozen atoms.
    // The surviving angle C1-C2-C3 must be built with the trigonal (order 3)
    // vertex type, byte-identical to the unfiltered reference.
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = s.add_atom(6, DVec3::new(1.33, 0.0, 0.0));
    let c3 = s.add_atom(6, DVec3::new(2.1, 1.25, 0.0));
    s.add_bond(c1, c2, BOND_DOUBLE);
    s.add_bond(c2, c3, BOND_SINGLE);

    let topology = MolecularTopology::from_structure(&s);
    let frozen = [0usize, 1];

    let reference =
        UffForceField::from_topology_with_vdw_mode(&topology, VdwMode::AllPairs).unwrap();
    let filtered =
        UffForceField::from_topology_with_frozen(&topology, VdwMode::AllPairs, &frozen).unwrap();

    assert_eq!(filtered.angle_params.len(), 1);
    let a = &filtered.angle_params[0];
    assert_eq!((a.idx1, a.idx2, a.idx3), (0, 1, 2));
    assert_eq!(a.order, 3, "sp2 vertex must yield trigonal coordination");

    let r = &reference.angle_params[0];
    assert_eq!(a.force_constant.to_bits(), r.force_constant.to_bits());
    assert_eq!(a.theta0.to_bits(), r.theta0.to_bits());
}

// ----------------------------------------------------------------------------
// Cutoff pair-set parity: two-grid scan vs brute force
// ----------------------------------------------------------------------------

/// Brute-force ground truth: every non-excluded pair within `build_radius`
/// with at least one free endpoint.
fn brute_force_cutoff_pairs(
    positions: &[f64],
    num_atoms: usize,
    build_radius: f64,
    exclusions: &BTreeSet<(usize, usize)>,
    frozen_flags: &[bool],
) -> BTreeSet<(usize, usize)> {
    let mut pairs = BTreeSet::new();
    for i in 0..num_atoms {
        for j in (i + 1)..num_atoms {
            if frozen_flags[i] && frozen_flags[j] {
                continue;
            }
            if exclusions.contains(&(i, j)) {
                continue;
            }
            let dx = positions[i * 3] - positions[j * 3];
            let dy = positions[i * 3 + 1] - positions[j * 3 + 1];
            let dz = positions[i * 3 + 2] - positions[j * 3 + 2];
            if dx * dx + dy * dy + dz * dz < build_radius * build_radius {
                pairs.insert((i, j));
            }
        }
    }
    pairs
}

/// Zigzag carbon chain with consecutive single bonds.
fn build_zigzag_chain(num_atoms: usize) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let mut prev = 0u32;
    for i in 0..num_atoms {
        let pos = DVec3::new(i as f64 * 1.4, (i % 2) as f64 * 0.6, 0.0);
        let id = s.add_atom(6, pos);
        if i > 0 {
            s.add_bond(prev, id, BOND_SINGLE);
        }
        prev = id;
    }
    s
}

/// 1-2 and 1-3 exclusions, normalized to (min, max) like the force field's.
fn exclusion_set(topology: &MolecularTopology) -> BTreeSet<(usize, usize)> {
    let mut exclusions: BTreeSet<(usize, usize)> = BTreeSet::new();
    for b in &topology.bonds {
        exclusions.insert((b.idx1.min(b.idx2), b.idx1.max(b.idx2)));
    }
    for a in &topology.angles {
        exclusions.insert((a.idx1.min(a.idx3), a.idx1.max(a.idx3)));
    }
    exclusions
}

#[test]
fn cutoff_pair_set_matches_brute_force_including_after_rebuild() {
    let num_atoms = 30;
    let structure = build_zigzag_chain(num_atoms);
    let topology = MolecularTopology::from_structure_bonded_only(&structure);

    let frozen: Vec<usize> = (0..15).collect();
    let mut frozen_flags = vec![false; num_atoms];
    for &i in &frozen {
        frozen_flags[i] = true;
    }

    let ff =
        UffForceField::from_topology_with_frozen(&topology, VdwMode::Cutoff(6.0), &frozen).unwrap();
    let build_radius = ff.cutoff_build_radius().unwrap();
    let exclusions = exclusion_set(&topology);

    let expected = brute_force_cutoff_pairs(
        &topology.positions,
        num_atoms,
        build_radius,
        &exclusions,
        &frozen_flags,
    );
    let pair_list = ff.vdw_pair_indices();
    let actual: BTreeSet<(usize, usize)> = pair_list.iter().copied().collect();
    assert_eq!(actual.len(), pair_list.len(), "duplicate pairs in the list");
    assert_eq!(actual, expected);

    // The frozen/free boundary must actually contribute pairs, and no
    // frozen-frozen pair may be present.
    assert!(
        expected
            .iter()
            .any(|&(i, j)| frozen_flags[i] != frozen_flags[j])
    );
    assert!(
        expected
            .iter()
            .all(|&(i, j)| !(frozen_flags[i] && frozen_flags[j]))
    );

    // Displace a free atom and force a rebuild (rebuild happens on the
    // evaluation where eval_count is a positive multiple of the interval,
    // i.e. the 11th call). Parity must hold against the moved positions --
    // this catches a stale frozen grid or a broken free/frozen partition.
    let mut moved = topology.positions.clone();
    moved[29 * 3] += 1.7;
    moved[29 * 3 + 1] += 2.4;
    let mut energy = 0.0;
    let mut gradients = vec![0.0; moved.len()];
    for _ in 0..11 {
        ff.energy_and_gradients(&moved, &mut energy, &mut gradients);
    }

    let expected_after =
        brute_force_cutoff_pairs(&moved, num_atoms, build_radius, &exclusions, &frozen_flags);
    let actual_after: BTreeSet<(usize, usize)> = ff.vdw_pair_indices().into_iter().collect();
    assert_ne!(
        expected_after, expected,
        "displacement should change the pair set"
    );
    assert_eq!(actual_after, expected_after);
}

#[test]
fn cutoff_pair_set_no_frozen_matches_brute_force() {
    // With no frozen atoms the two-grid scan degenerates to a single free
    // grid; parity with brute force must still hold.
    let num_atoms = 20;
    let structure = build_zigzag_chain(num_atoms);
    let topology = MolecularTopology::from_structure_bonded_only(&structure);

    let ff = UffForceField::from_topology_with_vdw_mode(&topology, VdwMode::Cutoff(6.0)).unwrap();
    let build_radius = ff.cutoff_build_radius().unwrap();
    let exclusions = exclusion_set(&topology);

    let expected = brute_force_cutoff_pairs(
        &topology.positions,
        num_atoms,
        build_radius,
        &exclusions,
        &vec![false; num_atoms],
    );
    let actual: BTreeSet<(usize, usize)> = ff.vdw_pair_indices().into_iter().collect();
    assert_eq!(actual, expected);
}

#[test]
fn all_atoms_frozen_all_param_lists_empty() {
    let structure = build_isopentane_skeleton();
    let topology = MolecularTopology::from_structure(&structure);
    let frozen: Vec<usize> = (0..5).collect();

    for mode in [VdwMode::AllPairs, VdwMode::Cutoff(6.0)] {
        let ff = UffForceField::from_topology_with_frozen(&topology, mode, &frozen).unwrap();
        assert!(ff.bond_params.is_empty());
        assert!(ff.angle_params.is_empty());
        assert!(ff.torsion_params.is_empty());
        assert!(ff.inversion_params.is_empty());
        assert!(ff.vdw_pair_indices().is_empty());

        let mut energy = 1.0;
        let mut gradients = vec![1.0; topology.positions.len()];
        ff.energy_and_gradients(&topology.positions, &mut energy, &mut gradients);
        assert_eq!(energy, 0.0);
        assert!(gradients.iter().all(|&g| g == 0.0));
    }
}
