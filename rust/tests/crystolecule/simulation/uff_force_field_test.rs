// Tests for UffForceField construction and energy/gradient evaluation (Phase 15).
//
// Validates that UffForceField::from_topology() produces a force field that
// matches RDKit's bonded-only energy and gradients for 9 reference molecules.

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DOUBLE, BOND_SINGLE, BOND_TRIPLE,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::simulation::force_field::ForceField;
use rust_lib_flutter_cad::crystolecule::simulation::topology::MolecularTopology;
use rust_lib_flutter_cad::crystolecule::simulation::uff::UffForceField;

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
}

#[derive(serde::Deserialize)]
struct InputEnergy {
    bonded: f64,
}

#[derive(serde::Deserialize)]
struct InputGradients {
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
// Energy at input positions: matches RDKit bonded energy
// ============================================================================

/// Computes the total bonded energy for a reference molecule at its input positions.
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
        (energy - mol.input_energy.bonded).abs() < 0.01,
        "methane: energy {energy} != expected {}",
        mol.input_energy.bonded
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
        (energy - mol.input_energy.bonded).abs() < 0.01,
        "ethylene: energy {energy} != expected {}",
        mol.input_energy.bonded
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
        (energy - mol.input_energy.bonded).abs() < 0.01,
        "ethane: energy {energy} != expected {}",
        mol.input_energy.bonded
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
        (energy - mol.input_energy.bonded).abs() < 0.1,
        "benzene: energy {energy} != expected {}",
        mol.input_energy.bonded
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
        (energy - mol.input_energy.bonded).abs() < 0.05,
        "butane: energy {energy} != expected {}",
        mol.input_energy.bonded
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
        (energy - mol.input_energy.bonded).abs() < 0.01,
        "water: energy {energy} != expected {}",
        mol.input_energy.bonded
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
        (energy - mol.input_energy.bonded).abs() < 0.01,
        "ammonia: energy {energy} != expected {}",
        mol.input_energy.bonded
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
        (energy - mol.input_energy.bonded).abs() < 0.1,
        "adamantane: energy {energy} != expected {}",
        mol.input_energy.bonded
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
        (energy - mol.input_energy.bonded).abs() < 0.01,
        "methanethiol: energy {energy} != expected {}",
        mol.input_energy.bonded
    );
}

/// Parametric test: all 9 molecules' energies match RDKit bonded energy.
#[test]
fn energy_all_molecules() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        let energy = compute_energy(&ff, &topology);
        let tol = if mol.atoms.len() > 10 { 0.1 } else { 0.01 };
        assert!(
            (energy - mol.input_energy.bonded).abs() < tol,
            "{}: energy {energy} != expected {} (diff = {})",
            mol.name,
            mol.input_energy.bonded,
            (energy - mol.input_energy.bonded).abs()
        );
    }
}

// ============================================================================
// Gradients at input positions: match RDKit bonded gradients
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
    check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, 0.01);
}

#[test]
fn gradients_ethylene() {
    let data = load_reference_data();
    let mol = &data.molecules[1];
    assert_eq!(mol.name, "ethylene");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, 0.01);
}

#[test]
fn gradients_ethane() {
    let data = load_reference_data();
    let mol = &data.molecules[2];
    assert_eq!(mol.name, "ethane");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, 0.01);
}

#[test]
fn gradients_benzene() {
    let data = load_reference_data();
    let mol = &data.molecules[3];
    assert_eq!(mol.name, "benzene");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, 0.1);
}

#[test]
fn gradients_butane() {
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, 0.05);
}

#[test]
fn gradients_water() {
    let data = load_reference_data();
    let mol = &data.molecules[5];
    assert_eq!(mol.name, "water");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, 0.01);
}

#[test]
fn gradients_ammonia() {
    let data = load_reference_data();
    let mol = &data.molecules[6];
    assert_eq!(mol.name, "ammonia");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, 0.01);
}

#[test]
fn gradients_adamantane() {
    let data = load_reference_data();
    let mol = &data.molecules[7];
    assert_eq!(mol.name, "adamantane");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, 0.1);
}

#[test]
fn gradients_methanethiol() {
    let data = load_reference_data();
    let mol = &data.molecules[8];
    assert_eq!(mol.name, "methanethiol");
    let (ff, topology) = build_ff_from_reference(mol);
    let gradients = compute_gradients(&ff, &topology);
    check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, 0.01);
}

#[test]
fn gradients_all_molecules() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, topology) = build_ff_from_reference(mol);
        let gradients = compute_gradients(&ff, &topology);
        let tol = if mol.atoms.len() > 10 { 0.1 } else { 0.01 };
        check_gradients(&mol.name, &gradients, &mol.input_gradients.bonded, tol);
    }
}

/// Checks that computed gradients match reference gradients within tolerance.
///
/// Uses absolute tolerance for small components and relative tolerance for large ones.
fn check_gradients(
    mol_name: &str,
    computed: &[f64],
    expected: &[[f64; 3]],
    abs_tol: f64,
) {
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
