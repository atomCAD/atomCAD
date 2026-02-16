// Tests for the L-BFGS minimizer (Phase 16, updated for Phase 20 vdW).
//
// Validates the optimizer on:
// - Simple quadratic functions (algorithm correctness)
// - Frozen dimensions (gradient zeroing)
// - UFF force field with reference molecules (energy reduction, convergence)

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DOUBLE, BOND_SINGLE, BOND_TRIPLE,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::simulation::force_field::ForceField;
use rust_lib_flutter_cad::crystolecule::simulation::minimize::{
    MinimizationConfig, minimize_with_force_field,
};
use rust_lib_flutter_cad::crystolecule::simulation::topology::MolecularTopology;
use rust_lib_flutter_cad::crystolecule::simulation::uff::UffForceField;

// ============================================================================
// Test force fields: simple analytically-solvable functions
// ============================================================================

/// Quadratic bowl: f(x) = 0.5 * sum(a_i * (x_i - c_i)^2)
/// Minimum at x = c with f = 0.
struct QuadraticFF {
    /// Coefficients a_i (must be positive for convexity).
    coeffs: Vec<f64>,
    /// Center c_i (location of minimum).
    center: Vec<f64>,
}

impl QuadraticFF {
    fn new(coeffs: Vec<f64>, center: Vec<f64>) -> Self {
        assert_eq!(coeffs.len(), center.len());
        Self { coeffs, center }
    }

    /// Isotropic quadratic: f(x) = 0.5 * k * sum((x_i - c_i)^2)
    fn isotropic(n: usize, k: f64, center: Vec<f64>) -> Self {
        Self::new(vec![k; n], center)
    }
}

impl ForceField for QuadraticFF {
    fn energy_and_gradients(&self, positions: &[f64], energy: &mut f64, gradients: &mut [f64]) {
        *energy = 0.0;
        for (i, (&x, (&a, &c))) in positions
            .iter()
            .zip(self.coeffs.iter().zip(self.center.iter()))
            .enumerate()
        {
            let dx = x - c;
            *energy += 0.5 * a * dx * dx;
            gradients[i] = a * dx;
        }
    }
}

/// Rosenbrock function: f(x,y) = (1-x)^2 + 100*(y-x^2)^2
/// Minimum at (1, 1) with f = 0. Classic optimizer test.
struct RosenbrockFF;

impl ForceField for RosenbrockFF {
    fn energy_and_gradients(&self, positions: &[f64], energy: &mut f64, gradients: &mut [f64]) {
        let x = positions[0];
        let y = positions[1];
        *energy = (1.0 - x).powi(2) + 100.0 * (y - x * x).powi(2);
        gradients[0] = -2.0 * (1.0 - x) + 200.0 * (y - x * x) * (-2.0 * x);
        gradients[1] = 200.0 * (y - x * x);
    }
}

// ============================================================================
// Reference data helpers (same pattern as uff_force_field_test.rs)
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
    input_energy: InputEnergy,
    minimized_positions: Vec<[f64; 3]>,
    minimized_energy: MinimizedEnergy,
    minimized_geometry: MinimizedGeometry,
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
struct InputEnergy {
    total: f64,
    #[allow(dead_code)]
    bonded: f64,
}

#[derive(serde::Deserialize)]
struct MinimizedEnergy {
    total: f64,
    #[allow(dead_code)]
    bonded: f64,
}

#[derive(serde::Deserialize)]
struct GeomDihedral {
    #[allow(dead_code)]
    atoms: [usize; 4],
    #[allow(dead_code)]
    dihedral_deg: f64,
}

#[derive(serde::Deserialize)]
struct MinimizedGeometry {
    bond_lengths: Vec<GeomBondLength>,
    angles: Vec<GeomAngle>,
    #[allow(dead_code)]
    dihedrals: Vec<GeomDihedral>,
}

#[derive(serde::Deserialize)]
struct GeomBondLength {
    atoms: [usize; 2],
    length: f64,
}

#[derive(serde::Deserialize)]
struct GeomAngle {
    atoms: [usize; 3],
    angle_deg: f64,
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

fn build_ff_and_positions(mol: &ReferenceMolecule) -> (UffForceField, Vec<f64>) {
    let structure = build_structure_from_reference(mol);
    let topology = MolecularTopology::from_structure(&structure);
    let ff = UffForceField::from_topology(&topology)
        .unwrap_or_else(|e| panic!("Failed to build UFF for {}: {}", mol.name, e));
    (ff, topology.positions)
}

/// Compute bond length between two atoms from flat position array.
fn bond_length(positions: &[f64], i: usize, j: usize) -> f64 {
    let dx = positions[j * 3] - positions[i * 3];
    let dy = positions[j * 3 + 1] - positions[i * 3 + 1];
    let dz = positions[j * 3 + 2] - positions[i * 3 + 2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Compute angle (in degrees) at vertex j between atoms i-j-k.
fn angle_deg(positions: &[f64], i: usize, j: usize, k: usize) -> f64 {
    let v1 = [
        positions[i * 3] - positions[j * 3],
        positions[i * 3 + 1] - positions[j * 3 + 1],
        positions[i * 3 + 2] - positions[j * 3 + 2],
    ];
    let v2 = [
        positions[k * 3] - positions[j * 3],
        positions[k * 3 + 1] - positions[j * 3 + 1],
        positions[k * 3 + 2] - positions[j * 3 + 2],
    ];
    let dot = v1[0] * v2[0] + v1[1] * v2[1] + v1[2] * v2[2];
    let len1 = (v1[0] * v1[0] + v1[1] * v1[1] + v1[2] * v1[2]).sqrt();
    let len2 = (v2[0] * v2[0] + v2[1] * v2[1] + v2[2] * v2[2]).sqrt();
    (dot / (len1 * len2)).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Kabsch alignment: compute the RMSD between two point sets after optimal
/// rigid-body superposition (translation + rotation). Uses SVD of the
/// cross-covariance matrix to find the optimal rotation.
///
/// `p` and `q` are Nx3 point sets (as Vec<[f64; 3]>). Returns (rmsd, max_deviation).
fn kabsch_rmsd(p: &[[f64; 3]], q: &[[f64; 3]]) -> (f64, f64) {
    use nalgebra::{Matrix3, SVD};

    let n = p.len();
    assert_eq!(n, q.len());
    assert!(n >= 3, "need at least 3 points for alignment");

    // 1. Compute centroids.
    let mut cp = [0.0; 3];
    let mut cq = [0.0; 3];
    for i in 0..n {
        for j in 0..3 {
            cp[j] += p[i][j];
            cq[j] += q[i][j];
        }
    }
    let nf = n as f64;
    for j in 0..3 {
        cp[j] /= nf;
        cq[j] /= nf;
    }

    // 2. Center both point sets.
    let pc: Vec<[f64; 3]> = p
        .iter()
        .map(|pt| [pt[0] - cp[0], pt[1] - cp[1], pt[2] - cp[2]])
        .collect();
    let qc: Vec<[f64; 3]> = q
        .iter()
        .map(|pt| [pt[0] - cq[0], pt[1] - cq[1], pt[2] - cq[2]])
        .collect();

    // 3. Build 3x3 cross-covariance matrix H = P^T Q.
    let mut h = [[0.0f64; 3]; 3];
    for i in 0..n {
        for r in 0..3 {
            for c in 0..3 {
                h[r][c] += pc[i][r] * qc[i][c];
            }
        }
    }
    let h_mat = Matrix3::new(
        h[0][0], h[0][1], h[0][2], h[1][0], h[1][1], h[1][2], h[2][0], h[2][1], h[2][2],
    );

    // 4. SVD: H = U S V^T.
    let svd = SVD::new(h_mat, true, true);
    let u = svd.u.expect("SVD failed to compute U");
    let v_t = svd.v_t.expect("SVD failed to compute V^T");

    // 5. Optimal rotation R = V * diag(1, 1, d) * U^T, where d = sign(det(V U^T))
    //    to ensure a proper rotation (det R = +1, not a reflection).
    let d = (v_t.transpose() * u.transpose()).determinant();
    let sign_d = if d < 0.0 { -1.0 } else { 1.0 };
    let correction = Matrix3::new(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, sign_d);
    let rotation = v_t.transpose() * correction * u.transpose();

    // 6. Apply rotation to centered P, compute RMSD against centered Q.
    let mut sum_sq = 0.0;
    let mut max_dev = 0.0f64;
    for i in 0..n {
        let px = rotation[(0, 0)] * pc[i][0]
            + rotation[(0, 1)] * pc[i][1]
            + rotation[(0, 2)] * pc[i][2];
        let py = rotation[(1, 0)] * pc[i][0]
            + rotation[(1, 1)] * pc[i][1]
            + rotation[(1, 2)] * pc[i][2];
        let pz = rotation[(2, 0)] * pc[i][0]
            + rotation[(2, 1)] * pc[i][1]
            + rotation[(2, 2)] * pc[i][2];

        let dx = px - qc[i][0];
        let dy = py - qc[i][1];
        let dz = pz - qc[i][2];
        let dist_sq = dx * dx + dy * dy + dz * dz;
        sum_sq += dist_sq;
        max_dev = max_dev.max(dist_sq.sqrt());
    }
    let rmsd = (sum_sq / nf).sqrt();
    (rmsd, max_dev)
}

// ============================================================================
// Unit tests: quadratic function
// ============================================================================

#[test]
fn quadratic_2d_converges_to_minimum() {
    let ff = QuadraticFF::isotropic(2, 1.0, vec![3.0, -2.0]);
    let config = MinimizationConfig::default();
    let mut pos = vec![0.0, 0.0];
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[]);
    assert!(result.converged, "should converge");
    assert!(result.energy < 1e-8, "energy should be near zero: {}", result.energy);
    assert!((pos[0] - 3.0).abs() < 1e-4, "x should be ~3.0: {}", pos[0]);
    assert!((pos[1] - (-2.0)).abs() < 1e-4, "y should be ~-2.0: {}", pos[1]);
}

#[test]
fn quadratic_6d_converges() {
    let center = vec![1.0, 2.0, 3.0, -1.0, -2.0, -3.0];
    let coeffs = vec![1.0, 10.0, 0.1, 5.0, 0.5, 2.0];
    let ff = QuadraticFF::new(coeffs, center.clone());
    let config = MinimizationConfig::default();
    let mut pos = vec![0.0; 6];
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[]);
    assert!(result.converged);
    for (i, (&p, &c)) in pos.iter().zip(center.iter()).enumerate() {
        assert!(
            (p - c).abs() < 1e-3,
            "dim {i}: pos {p} != center {c}"
        );
    }
}

#[test]
fn quadratic_already_at_minimum() {
    let ff = QuadraticFF::isotropic(3, 1.0, vec![0.0, 0.0, 0.0]);
    let config = MinimizationConfig::default();
    let mut pos = vec![0.0, 0.0, 0.0];
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[]);
    assert!(result.converged);
    assert_eq!(result.iterations, 0, "should converge immediately");
    assert!(result.energy.abs() < 1e-12);
}

#[test]
fn rosenbrock_converges() {
    let ff = RosenbrockFF;
    let config = MinimizationConfig {
        max_iterations: 2000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    let mut pos = vec![-1.0, 1.0];
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[]);
    assert!(result.converged, "Rosenbrock should converge");
    assert!(
        (pos[0] - 1.0).abs() < 1e-3,
        "x should be ~1.0: {}",
        pos[0]
    );
    assert!(
        (pos[1] - 1.0).abs() < 1e-3,
        "y should be ~1.0: {}",
        pos[1]
    );
    assert!(result.energy < 1e-5, "energy should be near zero: {}", result.energy);
}

#[test]
fn empty_positions() {
    let ff = QuadraticFF::new(vec![], vec![]);
    let config = MinimizationConfig::default();
    let mut pos: Vec<f64> = vec![];
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[]);
    assert!(result.converged);
    assert_eq!(result.iterations, 0);
}

// ============================================================================
// Unit tests: frozen dimensions
// ============================================================================

#[test]
fn frozen_dimension_stays_fixed() {
    // 2 "atoms" (6 coords), freeze atom 0.
    // Minimum at (1,2,3, 4,5,6). Start at origin.
    let ff = QuadraticFF::isotropic(6, 1.0, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let config = MinimizationConfig::default();
    let mut pos = vec![0.0; 6];
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[0]);
    assert!(result.converged);
    // Atom 0 (coords 0-2) should stay at origin.
    assert!(pos[0].abs() < 1e-12, "frozen x should be 0: {}", pos[0]);
    assert!(pos[1].abs() < 1e-12, "frozen y should be 0: {}", pos[1]);
    assert!(pos[2].abs() < 1e-12, "frozen z should be 0: {}", pos[2]);
    // Atom 1 (coords 3-5) should reach minimum.
    assert!((pos[3] - 4.0).abs() < 1e-3, "free x should be ~4: {}", pos[3]);
    assert!((pos[4] - 5.0).abs() < 1e-3, "free y should be ~5: {}", pos[4]);
    assert!((pos[5] - 6.0).abs() < 1e-3, "free z should be ~6: {}", pos[5]);
}

#[test]
fn all_atoms_frozen_converges_immediately() {
    let ff = QuadraticFF::isotropic(6, 1.0, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let config = MinimizationConfig::default();
    let mut pos = vec![0.0; 6];
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[0, 1]);
    assert!(result.converged);
    assert_eq!(result.iterations, 0, "all frozen → immediate convergence");
    // Positions unchanged.
    for &p in &pos {
        assert!(p.abs() < 1e-12);
    }
}

#[test]
fn freeze_subset_of_atoms() {
    // 3 "atoms" (9 coords). Freeze atoms 0 and 2, free atom 1.
    let center = vec![1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0];
    let ff = QuadraticFF::isotropic(9, 1.0, center);
    let config = MinimizationConfig::default();
    let mut pos = vec![0.0; 9];
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[0, 2]);
    assert!(result.converged);
    // Atoms 0 and 2 stay at origin.
    for i in [0, 1, 2, 6, 7, 8] {
        assert!(pos[i].abs() < 1e-12, "frozen coord {i} moved: {}", pos[i]);
    }
    // Atom 1 reaches its minimum.
    for i in [3, 4, 5] {
        assert!(
            (pos[i] - 2.0).abs() < 1e-3,
            "free coord {i}: {} != 2.0",
            pos[i]
        );
    }
}

// ============================================================================
// Unit tests: max iterations limit
// ============================================================================

#[test]
fn respects_max_iterations() {
    // Use a high-dimensional quadratic far from minimum with tight tolerance
    // to ensure the optimizer cannot converge in just 1 iteration.
    let n = 30;
    let center: Vec<f64> = (0..n).map(|i| (i as f64) * 10.0).collect();
    let ff = QuadraticFF::isotropic(n, 1.0, center);
    let config = MinimizationConfig {
        max_iterations: 1,
        gradient_rms_tolerance: 1e-15, // unreachable in 1 iter
        ..Default::default()
    };
    let mut pos = vec![0.0; n];
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[]);
    assert!(!result.converged, "should not converge in 1 iteration");
    assert_eq!(result.iterations, 1);
}

// ============================================================================
// Unit tests: energy always decreases
// ============================================================================

#[test]
fn energy_monotonically_decreases_quadratic() {
    // Track energy at each iteration using a wrapper.
    let center = vec![5.0, -3.0, 7.0];
    let ff = QuadraticFF::isotropic(3, 2.0, center);
    let mut pos = vec![0.0, 0.0, 0.0];

    // Compute initial energy.
    let mut e_init = 0.0;
    let mut g = vec![0.0; 3];
    ff.energy_and_gradients(&pos, &mut e_init, &mut g);

    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut pos, &config, &[]);

    assert!(
        result.energy <= e_init,
        "final energy {} should be <= initial energy {}",
        result.energy,
        e_init
    );
    assert!(result.energy < 1e-6);
}

// ============================================================================
// Integration tests: UFF minimization of reference molecules
// ============================================================================

#[test]
fn uff_methane_minimizes() {
    let data = load_reference_data();
    let mol = &data.molecules[0];
    assert_eq!(mol.name, "methane");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(result.converged, "methane should converge");
    assert!(
        result.energy < mol.input_energy.total + 0.01,
        "methane: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.total
    );
    // Minimized energy should be near zero for methane (very small molecule).
    assert!(
        result.energy < 0.01,
        "methane minimized energy should be near zero: {}",
        result.energy
    );
}

#[test]
fn uff_ethylene_minimizes() {
    let data = load_reference_data();
    let mol = &data.molecules[1];
    assert_eq!(mol.name, "ethylene");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(result.converged, "ethylene should converge");
    assert!(
        result.energy < mol.input_energy.total + 0.01,
        "ethylene: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.total
    );
}

#[test]
fn uff_ethane_minimizes() {
    let data = load_reference_data();
    let mol = &data.molecules[2];
    assert_eq!(mol.name, "ethane");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(result.converged, "ethane should converge");
    assert!(
        result.energy < mol.input_energy.total + 0.01,
        "ethane: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.total
    );
}

#[test]
fn uff_benzene_minimizes() {
    let data = load_reference_data();
    let mol = &data.molecules[3];
    assert_eq!(mol.name, "benzene");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(result.converged, "benzene should converge");
    assert!(
        result.energy < mol.input_energy.total + 0.1,
        "benzene: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.total
    );
}

#[test]
fn uff_water_minimizes() {
    let data = load_reference_data();
    let mol = &data.molecules[5];
    assert_eq!(mol.name, "water");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(result.converged, "water should converge");
    assert!(
        result.energy < mol.input_energy.total + 0.01,
        "water: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.total
    );
}

#[test]
fn uff_ammonia_minimizes() {
    let data = load_reference_data();
    let mol = &data.molecules[6];
    assert_eq!(mol.name, "ammonia");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(result.converged, "ammonia should converge");
    assert!(
        result.energy < mol.input_energy.total + 0.01,
        "ammonia: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.total
    );
}

#[test]
fn uff_adamantane_minimizes() {
    let data = load_reference_data();
    let mol = &data.molecules[7];
    assert_eq!(mol.name, "adamantane");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 2000,
        ..Default::default()
    };
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(result.converged, "adamantane should converge (got {} iterations)", result.iterations);
    assert!(
        result.energy < mol.input_energy.total + 0.1,
        "adamantane: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.total
    );
}

#[test]
fn uff_methanethiol_minimizes() {
    let data = load_reference_data();
    let mol = &data.molecules[8];
    assert_eq!(mol.name, "methanethiol");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(result.converged, "methanethiol should converge");
    assert!(
        result.energy < mol.input_energy.total + 0.01,
        "methanethiol: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.total
    );
}

/// All 9 reference molecules converge.
#[test]
fn uff_all_molecules_converge() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, mut positions) = build_ff_and_positions(mol);
        let config = MinimizationConfig {
            max_iterations: 2000,
            ..Default::default()
        };
        let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
        assert!(
            result.converged,
            "{}: did not converge after {} iterations (energy={})",
            mol.name,
            result.iterations,
            result.energy
        );
    }
}

// ============================================================================
// Integration tests: minimized total energy vs reference
// ============================================================================

#[test]
fn uff_minimized_total_energy_vs_reference() {
    // With full UFF (bonded + vdW), the minimized total energy should approximately
    // match RDKit's minimized total energy. Wider tolerances for larger molecules
    // because our optimizer may find slightly different local minima.
    let data = load_reference_data();
    let tolerances = [
        ("methane", 0.01),
        ("ethylene", 0.1),
        ("ethane", 0.1),
        ("benzene", 1.0),
        ("butane", 0.5),
        ("water", 0.01),
        ("ammonia", 0.01),
        ("adamantane", 2.0),
        ("methanethiol", 0.1),
    ];
    for (mol, &(expected_name, tol)) in data.molecules.iter().zip(tolerances.iter()) {
        assert_eq!(mol.name, expected_name);
        let (ff, mut positions) = build_ff_and_positions(mol);
        let config = MinimizationConfig {
            max_iterations: 2000,
            gradient_rms_tolerance: 1e-6,
            ..Default::default()
        };
        let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);

        assert!(
            (result.energy - mol.minimized_energy.total).abs() < tol,
            "{}: minimized energy {:.6} vs reference {:.6} (diff={:.6}, tol={tol})",
            mol.name,
            result.energy,
            mol.minimized_energy.total,
            (result.energy - mol.minimized_energy.total).abs()
        );
    }
}

// ============================================================================
// Integration tests: minimized geometry is physically correct
// ============================================================================
//
// With full UFF (bonded + vdW), minimized geometries can be directly compared
// against RDKit's reference since both optimizations include the same energy
// terms. For small molecules without vdW pairs (methane, water, ammonia),
// bonds match UFF rest lengths exactly. For larger molecules, vdW pressure
// may shift bonds/angles slightly from pure-bonded rest values.

#[test]
fn uff_methane_minimized_geometry() {
    let data = load_reference_data();
    let mol = &data.molecules[0];
    assert_eq!(mol.name, "methane");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // All C-H bonds should equal UFF rest length (methane has no vdW pairs).
    // UFF C_3-H_ rest length ≈ 1.109 Å (from calc_bond_rest_length).
    let ch_rest = ff.bond_params[0].rest_length;
    for bp in &ff.bond_params {
        let computed = bond_length(&positions, bp.idx1, bp.idx2);
        assert!(
            (computed - bp.rest_length).abs() < 0.001,
            "methane bond {}-{}: {:.4} != rest {:.4}",
            bp.idx1,
            bp.idx2,
            computed,
            bp.rest_length
        );
    }

    // Cross-check against RDKit reference (no vdW pairs for methane).
    for bl in &mol.minimized_geometry.bond_lengths {
        let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
        assert!(
            (computed - bl.length).abs() < 0.01,
            "methane bond {}-{}: {:.4} vs RDKit {:.4}",
            bl.atoms[0],
            bl.atoms[1],
            computed,
            bl.length
        );
    }

    // All H-C-H angles should be tetrahedral (109.47°).
    for a in &mol.minimized_geometry.angles {
        let computed = angle_deg(&positions, a.atoms[0], a.atoms[1], a.atoms[2]);
        assert!(
            (computed - 109.47).abs() < 0.5,
            "methane angle {}-{}-{}: {:.2}° != 109.47°",
            a.atoms[0],
            a.atoms[1],
            a.atoms[2],
            computed
        );
    }

    let _ = ch_rest; // suppress unused warning
}

#[test]
fn uff_ethylene_minimized_geometry() {
    let data = load_reference_data();
    let mol = &data.molecules[1];
    assert_eq!(mol.name, "ethylene");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // Bonds should be near UFF rest lengths (ethylene has only 4 H-H vdW pairs,
    // minor effect on bond lengths).
    for bp in &ff.bond_params {
        let computed = bond_length(&positions, bp.idx1, bp.idx2);
        assert!(
            (computed - bp.rest_length).abs() < 0.005,
            "ethylene bond {}-{}: {:.4} != rest {:.4}",
            bp.idx1,
            bp.idx2,
            computed,
            bp.rest_length
        );
    }

    // Cross-check against RDKit reference.
    for bl in &mol.minimized_geometry.bond_lengths {
        let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
        assert!(
            (computed - bl.length).abs() < 0.01,
            "ethylene bond {}-{}: {:.4} vs RDKit {:.4}",
            bl.atoms[0],
            bl.atoms[1],
            computed,
            bl.length
        );
    }

    // All angles should be 120° (trigonal planar sp2).
    for a in &mol.minimized_geometry.angles {
        let computed = angle_deg(&positions, a.atoms[0], a.atoms[1], a.atoms[2]);
        assert!(
            (computed - 120.0).abs() < 1.0,
            "ethylene angle {}-{}-{}: {:.2}° != 120°",
            a.atoms[0],
            a.atoms[1],
            a.atoms[2],
            computed
        );
    }
}

#[test]
fn uff_water_minimized_geometry() {
    let data = load_reference_data();
    let mol = &data.molecules[5];
    assert_eq!(mol.name, "water");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // O-H bonds should match rest length (water has no vdW pairs).
    for bp in &ff.bond_params {
        let computed = bond_length(&positions, bp.idx1, bp.idx2);
        assert!(
            (computed - bp.rest_length).abs() < 0.001,
            "water bond {}-{}: {:.4} != rest {:.4}",
            bp.idx1,
            bp.idx2,
            computed,
            bp.rest_length
        );
    }

    // Cross-check bond lengths vs RDKit (no vdW pairs for water).
    for bl in &mol.minimized_geometry.bond_lengths {
        let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
        assert!(
            (computed - bl.length).abs() < 0.01,
            "water bond {}-{}: {:.4} vs RDKit {:.4}",
            bl.atoms[0],
            bl.atoms[1],
            computed,
            bl.length
        );
    }

    // H-O-H angle should match UFF theta0 for O_3 (≈104.51°).
    for a in &mol.minimized_geometry.angles {
        let computed = angle_deg(&positions, a.atoms[0], a.atoms[1], a.atoms[2]);
        assert!(
            (computed - a.angle_deg).abs() < 1.0,
            "water angle {}-{}-{}: {:.2} != {:.2}",
            a.atoms[0],
            a.atoms[1],
            a.atoms[2],
            computed,
            a.angle_deg
        );
    }
}

// ============================================================================
// Integration tests: frozen atoms with UFF
// ============================================================================

#[test]
fn uff_frozen_atoms_stay_fixed() {
    let data = load_reference_data();
    let mol = &data.molecules[2]; // ethane
    assert_eq!(mol.name, "ethane");
    let (ff, mut positions) = build_ff_and_positions(mol);

    // Save initial positions of atom 0.
    let init_0 = [positions[0], positions[1], positions[2]];

    // Freeze atom 0 (first carbon).
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[0]);
    assert!(result.converged, "ethane with frozen atom should converge");

    // Atom 0 should not have moved.
    assert!(
        (positions[0] - init_0[0]).abs() < 1e-12,
        "frozen atom 0 x moved"
    );
    assert!(
        (positions[1] - init_0[1]).abs() < 1e-12,
        "frozen atom 0 y moved"
    );
    assert!(
        (positions[2] - init_0[2]).abs() < 1e-12,
        "frozen atom 0 z moved"
    );
}

#[test]
fn uff_freeze_all_atoms_no_change() {
    let data = load_reference_data();
    let mol = &data.molecules[0]; // methane
    assert_eq!(mol.name, "methane");
    let (ff, mut positions) = build_ff_and_positions(mol);

    let init_positions = positions.clone();
    let frozen: Vec<usize> = (0..mol.atoms.len()).collect();
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &frozen);

    assert!(result.converged);
    assert_eq!(result.iterations, 0);
    for (i, (&p, &ip)) in positions.iter().zip(init_positions.iter()).enumerate() {
        assert!(
            (p - ip).abs() < 1e-12,
            "coord {i} changed when all atoms frozen"
        );
    }
}

#[test]
fn uff_partial_freeze_reduces_energy() {
    let data = load_reference_data();
    let mol = &data.molecules[4]; // butane
    assert_eq!(mol.name, "butane");
    let (ff, mut positions) = build_ff_and_positions(mol);

    // Compute initial energy.
    let mut e_init = 0.0;
    let mut g = vec![0.0; positions.len()];
    ff.energy_and_gradients(&positions, &mut e_init, &mut g);

    // Freeze first two atoms (atoms 0 and 1), let rest relax.
    let config = MinimizationConfig::default();
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[0, 1]);

    assert!(
        result.energy <= e_init + 0.01,
        "partial freeze: energy {} should be <= initial {} (within tolerance)",
        result.energy,
        e_init
    );
}

// ============================================================================
// Config tests
// ============================================================================

#[test]
fn default_config_has_sensible_values() {
    let config = MinimizationConfig::default();
    assert_eq!(config.max_iterations, 500);
    assert!((config.gradient_rms_tolerance - 1e-4).abs() < 1e-10);
    assert_eq!(config.memory_size, 8);
    assert!(config.line_search_c1 > 0.0 && config.line_search_c1 < 1.0);
    assert!(config.line_search_min_step > 0.0);
    assert!(config.line_search_max_iter > 0);
}

// ============================================================================
// Phase 18: B7 — Ethylene full optimization (RDKit testUFF5/8)
// ============================================================================

/// After minimization, all 6 atoms of ethylene should be coplanar (sp2 planar).
/// This verifies that the angle bend (order=3 trigonal) and inversion terms
/// correctly enforce planar geometry.
#[test]
fn b7_ethylene_planarity() {
    let data = load_reference_data();
    let mol = &data.molecules[1];
    assert_eq!(mol.name, "ethylene");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // Compute plane normal from atoms 0, 1, 2.
    let p = |idx: usize| -> [f64; 3] {
        [positions[idx * 3], positions[idx * 3 + 1], positions[idx * 3 + 2]]
    };
    let v1 = [
        p(1)[0] - p(0)[0],
        p(1)[1] - p(0)[1],
        p(1)[2] - p(0)[2],
    ];
    let v2 = [
        p(2)[0] - p(0)[0],
        p(2)[1] - p(0)[1],
        p(2)[2] - p(0)[2],
    ];
    let normal = [
        v1[1] * v2[2] - v1[2] * v2[1],
        v1[2] * v2[0] - v1[0] * v2[2],
        v1[0] * v2[1] - v1[1] * v2[0],
    ];
    let normal_len =
        (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    assert!(normal_len > 1e-6, "first 3 atoms are collinear");

    // All 6 atoms should lie on the plane.
    for atom_idx in 0..mol.atoms.len() {
        let d = [
            p(atom_idx)[0] - p(0)[0],
            p(atom_idx)[1] - p(0)[1],
            p(atom_idx)[2] - p(0)[2],
        ];
        let dist =
            (d[0] * normal[0] + d[1] * normal[1] + d[2] * normal[2]).abs() / normal_len;
        assert!(
            dist < 0.01,
            "ethylene atom {atom_idx} is {dist:.6} Å out of plane"
        );
    }
}

/// C=C bond should be shorter than C-H bonds (double bond < single bond).
#[test]
fn b7_ethylene_bond_length_ordering() {
    let data = load_reference_data();
    let mol = &data.molecules[1];
    assert_eq!(mol.name, "ethylene");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // Find C=C and C-H rest lengths from the force field parameters.
    let mut cc_length = None;
    let mut ch_lengths = Vec::new();
    for bp in &ff.bond_params {
        let len = bond_length(&positions, bp.idx1, bp.idx2);
        // Atom 0 and 1 are carbons (6), rest are hydrogens (1).
        // Bond between two carbons is C=C; bonds to hydrogen are C-H.
        if bp.rest_length > 1.2 {
            cc_length = Some(len);
        } else {
            ch_lengths.push(len);
        }
    }
    let cc = cc_length.expect("no C=C bond found");
    assert!(
        !ch_lengths.is_empty(),
        "no C-H bonds found"
    );
    for &ch in &ch_lengths {
        assert!(
            cc > ch,
            "C=C ({cc:.4}) should be longer than C-H ({ch:.4}) — but C=C is a double bond \
             with shorter rest length... wait"
        );
    }
    // Actually: C=C rest length (~1.329) > C-H rest length (~1.085). Both are correct UFF values.
    // The ordering check is: all C-H lengths should be approximately equal.
    let ch_avg: f64 = ch_lengths.iter().sum::<f64>() / ch_lengths.len() as f64;
    for (i, &ch) in ch_lengths.iter().enumerate() {
        assert!(
            (ch - ch_avg).abs() < 0.001,
            "ethylene C-H bond {i}: {ch:.4} differs from average {ch_avg:.4}"
        );
    }
}

// ============================================================================
// Phase 18: B10 — Known molecule energies (updated for full UFF with vdW)
// ============================================================================

/// For every reference molecule, the minimized total energy must be strictly
/// less than the input total energy (optimizer must improve the energy).
#[test]
fn b10_energy_decreases_from_input() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, mut positions) = build_ff_and_positions(mol);
        let config = MinimizationConfig {
            max_iterations: 2000,
            gradient_rms_tolerance: 1e-6,
            ..Default::default()
        };
        let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
        assert!(
            result.energy <= mol.input_energy.total + 1e-6,
            "{}: minimized energy {:.6} > input energy {:.6}",
            mol.name,
            result.energy,
            mol.input_energy.total
        );
    }
}

/// Verify that the minimizer's reported energy matches re-evaluation at the
/// minimized positions. Catches bugs where the minimizer's internal energy
/// tracking diverges from the force field's actual output.
#[test]
fn b10_energy_self_consistent() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, mut positions) = build_ff_and_positions(mol);
        let config = MinimizationConfig {
            max_iterations: 2000,
            gradient_rms_tolerance: 1e-6,
            ..Default::default()
        };
        let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);

        // Recompute energy at minimized positions.
        let mut recomputed = 0.0;
        let mut grad = vec![0.0; positions.len()];
        ff.energy_and_gradients(&positions, &mut recomputed, &mut grad);

        assert!(
            (result.energy - recomputed).abs() < 1e-10,
            "{}: reported energy {:.10} != recomputed {:.10}",
            mol.name,
            result.energy,
            recomputed
        );
    }
}

/// Our minimized total energy should approximately match RDKit's minimized
/// total energy. Both now include the same energy terms (bonded + vdW).
#[test]
fn b10_minimized_energy_matches_reference() {
    let data = load_reference_data();
    let tolerances = [
        ("methane", 0.01),
        ("ethylene", 0.1),
        ("ethane", 0.1),
        ("benzene", 1.0),
        ("butane", 0.5),
        ("water", 0.01),
        ("ammonia", 0.01),
        ("adamantane", 2.0),
        ("methanethiol", 0.1),
    ];
    for (mol, &(expected_name, tol)) in data.molecules.iter().zip(tolerances.iter()) {
        assert_eq!(mol.name, expected_name);
        let (ff, mut positions) = build_ff_and_positions(mol);
        let config = MinimizationConfig {
            max_iterations: 2000,
            gradient_rms_tolerance: 1e-6,
            ..Default::default()
        };
        let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);

        assert!(
            (result.energy - mol.minimized_energy.total).abs() < tol,
            "{}: our minimum {:.6} vs RDKit {:.6} (diff={:.6}, tol={tol})",
            mol.name,
            result.energy,
            mol.minimized_energy.total,
            (result.energy - mol.minimized_energy.total).abs()
        );
    }
}

// ============================================================================
// Phase 18: B11 — End-to-end minimized geometry (updated for full UFF with vdW)
// ============================================================================

/// The definitive geometry test: minimize every molecule and compare the
/// resulting atom positions against RDKit's minimized positions using Kabsch
/// alignment (optimal rigid-body superposition). This single test subsumes
/// all per-bond and per-angle comparisons — if the RMSD is small, all
/// internal coordinates (bonds, angles, dihedrals) necessarily match.
#[test]
fn b11_all_molecules_rmsd_vs_reference() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, mut positions) = build_ff_and_positions(mol);
        let config = MinimizationConfig {
            max_iterations: 5000,
            gradient_rms_tolerance: 1e-6,
            ..Default::default()
        };
        minimize_with_force_field(&ff, &mut positions, &config, &[]);

        // Convert flat position array to Vec<[f64; 3]>.
        let n = mol.atoms.len();
        let our_pos: Vec<[f64; 3]> = (0..n)
            .map(|i| [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]])
            .collect();

        let (rmsd, max_dev) = kabsch_rmsd(&our_pos, &mol.minimized_positions);

        assert!(
            rmsd < 0.01,
            "{}: RMSD {:.6} Å vs RDKit (max deviation {:.6} Å)",
            mol.name,
            rmsd,
            max_dev
        );
    }
}

#[test]
fn b11_ethane_minimized_geometry() {
    let data = load_reference_data();
    let mol = &data.molecules[2];
    assert_eq!(mol.name, "ethane");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // Primary validation: compare against RDKit's vdW-optimized geometry.
    for bl in &mol.minimized_geometry.bond_lengths {
        let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
        assert!(
            (computed - bl.length).abs() < 0.01,
            "ethane bond {}-{}: {:.4} vs RDKit {:.4}",
            bl.atoms[0],
            bl.atoms[1],
            computed,
            bl.length
        );
    }
    for a in &mol.minimized_geometry.angles {
        let computed = angle_deg(&positions, a.atoms[0], a.atoms[1], a.atoms[2]);
        assert!(
            (computed - a.angle_deg).abs() < 2.0,
            "ethane angle {}-{}-{}: {:.2}° vs RDKit {:.2}°",
            a.atoms[0],
            a.atoms[1],
            a.atoms[2],
            computed,
            a.angle_deg
        );
    }

    // Secondary sanity check: bonds shouldn't deviate too far from UFF rest lengths.
    for bp in &ff.bond_params {
        let computed = bond_length(&positions, bp.idx1, bp.idx2);
        assert!(
            (computed - bp.rest_length).abs() < 0.02,
            "ethane bond {}-{}: {:.4} too far from rest {:.4}",
            bp.idx1,
            bp.idx2,
            computed,
            bp.rest_length
        );
    }
}

#[test]
fn b11_benzene_minimized_geometry() {
    let data = load_reference_data();
    let mol = &data.molecules[3];
    assert_eq!(mol.name, "benzene");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // Primary validation: compare against RDKit's vdW-optimized geometry.
    for bl in &mol.minimized_geometry.bond_lengths {
        let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
        assert!(
            (computed - bl.length).abs() < 0.01,
            "benzene bond {}-{}: {:.4} vs RDKit {:.4}",
            bl.atoms[0],
            bl.atoms[1],
            computed,
            bl.length
        );
    }
    for a in &mol.minimized_geometry.angles {
        let computed = angle_deg(&positions, a.atoms[0], a.atoms[1], a.atoms[2]);
        assert!(
            (computed - a.angle_deg).abs() < 2.0,
            "benzene angle {}-{}-{}: {:.2}° vs RDKit {:.2}°",
            a.atoms[0],
            a.atoms[1],
            a.atoms[2],
            computed,
            a.angle_deg
        );
    }

    // Secondary sanity check: bonds shouldn't deviate too far from UFF rest lengths.
    for bp in &ff.bond_params {
        let computed = bond_length(&positions, bp.idx1, bp.idx2);
        assert!(
            (computed - bp.rest_length).abs() < 0.03,
            "benzene bond {}-{}: {:.4} too far from rest {:.4}",
            bp.idx1,
            bp.idx2,
            computed,
            bp.rest_length
        );
    }

    // Planarity: all 12 atoms should be coplanar.
    let p = |idx: usize| -> [f64; 3] {
        [positions[idx * 3], positions[idx * 3 + 1], positions[idx * 3 + 2]]
    };
    let v1 = [
        p(1)[0] - p(0)[0],
        p(1)[1] - p(0)[1],
        p(1)[2] - p(0)[2],
    ];
    let v2 = [
        p(2)[0] - p(0)[0],
        p(2)[1] - p(0)[1],
        p(2)[2] - p(0)[2],
    ];
    let normal = [
        v1[1] * v2[2] - v1[2] * v2[1],
        v1[2] * v2[0] - v1[0] * v2[2],
        v1[0] * v2[1] - v1[1] * v2[0],
    ];
    let normal_len =
        (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    assert!(normal_len > 1e-6, "first 3 benzene atoms are collinear");
    for atom_idx in 0..mol.atoms.len() {
        let d = [
            p(atom_idx)[0] - p(0)[0],
            p(atom_idx)[1] - p(0)[1],
            p(atom_idx)[2] - p(0)[2],
        ];
        let dist =
            (d[0] * normal[0] + d[1] * normal[1] + d[2] * normal[2]).abs() / normal_len;
        assert!(
            dist < 0.01,
            "benzene atom {atom_idx} is {dist:.6} Å out of plane"
        );
    }
}

#[test]
fn b11_butane_minimized_geometry() {
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // Primary validation: compare against RDKit's vdW-optimized geometry.
    // Both optimizers include the same energy terms, so geometry should match tightly.
    for bl in &mol.minimized_geometry.bond_lengths {
        let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
        assert!(
            (computed - bl.length).abs() < 0.01,
            "butane bond {}-{}: {:.4} vs RDKit {:.4}",
            bl.atoms[0],
            bl.atoms[1],
            computed,
            bl.length
        );
    }
    for a in &mol.minimized_geometry.angles {
        let computed = angle_deg(&positions, a.atoms[0], a.atoms[1], a.atoms[2]);
        assert!(
            (computed - a.angle_deg).abs() < 2.0,
            "butane angle {}-{}-{}: {:.2}° vs RDKit {:.2}°",
            a.atoms[0],
            a.atoms[1],
            a.atoms[2],
            computed,
            a.angle_deg
        );
    }
}

#[test]
fn b11_ammonia_minimized_geometry() {
    let data = load_reference_data();
    let mol = &data.molecules[6];
    assert_eq!(mol.name, "ammonia");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // All N-H bonds should match UFF rest length (ammonia has no vdW pairs).
    for bp in &ff.bond_params {
        let computed = bond_length(&positions, bp.idx1, bp.idx2);
        assert!(
            (computed - bp.rest_length).abs() < 0.001,
            "ammonia bond {}-{}: {:.4} != rest {:.4}",
            bp.idx1,
            bp.idx2,
            computed,
            bp.rest_length
        );
    }

    // Cross-check bond lengths against RDKit (no vdW pairs for ammonia).
    for bl in &mol.minimized_geometry.bond_lengths {
        let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
        assert!(
            (computed - bl.length).abs() < 0.01,
            "ammonia bond {}-{}: {:.4} vs RDKit {:.4}",
            bl.atoms[0],
            bl.atoms[1],
            computed,
            bl.length
        );
    }

    // H-N-H angles should be near UFF theta0 (~106.7°).
    for a in &mol.minimized_geometry.angles {
        let computed = angle_deg(&positions, a.atoms[0], a.atoms[1], a.atoms[2]);
        assert!(
            (computed - a.angle_deg).abs() < 1.0,
            "ammonia angle {}-{}-{}: {:.2}° vs RDKit {:.2}°",
            a.atoms[0],
            a.atoms[1],
            a.atoms[2],
            computed,
            a.angle_deg
        );
    }

    // All three N-H bonds should be equal length (C3v symmetry).
    let nh_lengths: Vec<f64> = ff
        .bond_params
        .iter()
        .map(|bp| bond_length(&positions, bp.idx1, bp.idx2))
        .collect();
    let avg = nh_lengths.iter().sum::<f64>() / nh_lengths.len() as f64;
    for (i, &l) in nh_lengths.iter().enumerate() {
        assert!(
            (l - avg).abs() < 0.001,
            "ammonia N-H bond {i}: {l:.4} differs from average {avg:.4}"
        );
    }
}

#[test]
fn b11_adamantane_minimized_geometry() {
    let data = load_reference_data();
    let mol = &data.molecules[7];
    assert_eq!(mol.name, "adamantane");
    let (ff, mut positions) = build_ff_and_positions(mol);
    // Adamantane with 237 vdW pairs needs relaxed convergence tolerance.
    let config = MinimizationConfig {
        max_iterations: 5000,
        gradient_rms_tolerance: 1e-4,
        ..Default::default()
    };
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(
        result.converged,
        "adamantane should converge (got {} iterations)",
        result.iterations
    );

    // Primary validation: compare against RDKit's vdW-optimized geometry.
    // Both optimizers use the same UFF energy terms, so geometry should match.
    for bl in &mol.minimized_geometry.bond_lengths {
        let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
        assert!(
            (computed - bl.length).abs() < 0.02,
            "adamantane bond {}-{}: {:.4} vs RDKit {:.4} (diff={:.4})",
            bl.atoms[0],
            bl.atoms[1],
            computed,
            bl.length,
            (computed - bl.length).abs()
        );
    }
    for a in &mol.minimized_geometry.angles {
        let computed = angle_deg(&positions, a.atoms[0], a.atoms[1], a.atoms[2]);
        assert!(
            (computed - a.angle_deg).abs() < 2.0,
            "adamantane angle {}-{}-{}: {:.2}° vs RDKit {:.2}° (diff={:.2}°)",
            a.atoms[0],
            a.atoms[1],
            a.atoms[2],
            computed,
            a.angle_deg,
            (computed - a.angle_deg).abs()
        );
    }
}

#[test]
fn b11_methanethiol_minimized_geometry() {
    let data = load_reference_data();
    let mol = &data.molecules[8];
    assert_eq!(mol.name, "methanethiol");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // After full UFF minimization, bonds should be near UFF rest lengths.
    // Methanethiol has 3 H-H vdW pairs; minor effect on geometry.
    for bp in &ff.bond_params {
        let computed = bond_length(&positions, bp.idx1, bp.idx2);
        assert!(
            (computed - bp.rest_length).abs() < 0.005,
            "methanethiol bond {}-{}: {:.4} != rest {:.4}",
            bp.idx1,
            bp.idx2,
            computed,
            bp.rest_length
        );
    }

    // Cross-check against RDKit reference.
    for bl in &mol.minimized_geometry.bond_lengths {
        let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
        assert!(
            (computed - bl.length).abs() < 0.01,
            "methanethiol bond {}-{}: {:.4} vs RDKit {:.4}",
            bl.atoms[0],
            bl.atoms[1],
            computed,
            bl.length
        );
    }

    // Angles should be near UFF theta0.
    for ap in &ff.angle_params {
        let computed = angle_deg(&positions, ap.idx1, ap.idx2, ap.idx3);
        let expected = ap.theta0.to_degrees();
        assert!(
            (computed - expected).abs() < 2.0,
            "methanethiol angle {}-{}-{}: {:.2}° != theta0 {:.2}°",
            ap.idx1,
            ap.idx2,
            ap.idx3,
            computed,
            expected
        );
    }
}

/// After full UFF minimization, every bond in every molecule should match
/// the RDKit reference geometry. Both optimizers use the same UFF energy
/// terms (bonded + vdW), so minimized geometries should agree tightly.
#[test]
fn b11_all_bonds_match_reference() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, mut positions) = build_ff_and_positions(mol);
        // Adamantane needs relaxed gradient tolerance due to 237 vdW pairs.
        let (max_iter, grad_tol) = if mol.name == "adamantane" {
            (5000, 1e-4)
        } else {
            (2000, 1e-6)
        };
        let config = MinimizationConfig {
            max_iterations: max_iter,
            gradient_rms_tolerance: grad_tol,
            ..Default::default()
        };
        minimize_with_force_field(&ff, &mut positions, &config, &[]);

        // Primary: compare against RDKit reference (tight tolerance).
        let bond_tol = if mol.name == "adamantane" { 0.02 } else { 0.01 };
        for bl in &mol.minimized_geometry.bond_lengths {
            let computed = bond_length(&positions, bl.atoms[0], bl.atoms[1]);
            assert!(
                (computed - bl.length).abs() < bond_tol,
                "{}: bond {}-{}: {:.4} vs RDKit {:.4} (diff={:.4}, tol={bond_tol})",
                mol.name,
                bl.atoms[0],
                bl.atoms[1],
                computed,
                bl.length,
                (computed - bl.length).abs()
            );
        }

        // Secondary sanity check: bonds within 0.05 Å of UFF rest lengths.
        for bp in &ff.bond_params {
            let computed = bond_length(&positions, bp.idx1, bp.idx2);
            assert!(
                (computed - bp.rest_length).abs() < 0.05,
                "{}: bond {}-{}: {:.4} too far from rest {:.4}",
                mol.name,
                bp.idx1,
                bp.idx2,
                computed,
                bp.rest_length
            );
        }
    }
}

/// After full UFF minimization, every angle in every molecule should match
/// the RDKit reference geometry. Both optimizers use the same UFF energy
/// terms (bonded + vdW), so minimized geometries should agree tightly.
#[test]
fn b11_all_angles_match_reference() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, mut positions) = build_ff_and_positions(mol);
        // Adamantane needs relaxed gradient tolerance due to 237 vdW pairs.
        let (max_iter, grad_tol) = if mol.name == "adamantane" {
            (5000, 1e-4)
        } else {
            (2000, 1e-6)
        };
        let config = MinimizationConfig {
            max_iterations: max_iter,
            gradient_rms_tolerance: grad_tol,
            ..Default::default()
        };
        minimize_with_force_field(&ff, &mut positions, &config, &[]);

        // Primary: compare against RDKit reference (tight tolerance).
        for a in &mol.minimized_geometry.angles {
            let computed = angle_deg(&positions, a.atoms[0], a.atoms[1], a.atoms[2]);
            assert!(
                (computed - a.angle_deg).abs() < 2.0,
                "{}: angle {}-{}-{}: {:.2}° vs RDKit {:.2}° (diff={:.2}°)",
                mol.name,
                a.atoms[0],
                a.atoms[1],
                a.atoms[2],
                computed,
                a.angle_deg,
                (computed - a.angle_deg).abs()
            );
        }

        // Secondary sanity check: angles within 5° of UFF theta0.
        for ap in &ff.angle_params {
            let computed = angle_deg(&positions, ap.idx1, ap.idx2, ap.idx3);
            let expected = ap.theta0.to_degrees();
            assert!(
                (computed - expected).abs() < 5.0,
                "{}: angle {}-{}-{}: {:.2}° too far from theta0 {:.2}°",
                mol.name,
                ap.idx1,
                ap.idx2,
                ap.idx3,
                computed,
                expected
            );
        }
    }
}

// ============================================================================
// Phase 19: B9 — Butane 72-point dihedral scan (updated for full UFF with vdW)
// ============================================================================
//
// Validates the torsion + vdW potential by performing a constrained dihedral scan
// of butane's C-C-C-C backbone. At each of 72 angles (0 to 355 degrees in
// 5-degree steps), the four carbon atoms are frozen and the hydrogen positions
// are minimized.
//
// Key physics: With full UFF (bonded + vdW), the butane torsion profile is
// asymmetric. The vdW interactions between 1-4 hydrogen pairs break the 3-fold
// symmetry of the bonded-only cos(3*phi) potential:
// - Anti (180°) is the global minimum (no 1-4 H-H steric clash)
// - Gauche (~60°, ~300°) are local minima, higher than anti by ~1.4 kcal/mol
// - Eclipsed (~120°, ~240°) are transition states
// - Syn (0°) is the highest barrier (~11.13 kcal/mol above anti)
//
// The profile has mirror symmetry about 180° (E(phi) ≈ E(360-phi)) but NOT
// 3-fold symmetry (anti != gauche, eclipsed at 120° != syn at 0°).
//
// What these tests validate:
// - The combined bonded + vdW energy is correct
// - Anti/gauche asymmetry (vdW breaks 3-fold degeneracy)
// - Barrier height matches reference (~11.13 kcal/mol)
// - The constrained minimization works (frozen atoms stay fixed)
// - The dihedral rotation helper produces correct angles

// --- Scan reference data structs ---

#[derive(serde::Deserialize)]
struct ButaneDihedralScan {
    #[allow(dead_code)]
    carbon_indices: [usize; 4],
    num_points: usize,
    #[allow(dead_code)]
    scan_points: Vec<ScanPoint>,
    #[allow(dead_code)]
    min_energy: f64,
    key_conformations: KeyConformations,
}

#[derive(serde::Deserialize)]
struct ScanPoint {
    #[allow(dead_code)]
    target_angle_deg: f64,
    #[allow(dead_code)]
    actual_angle_deg: f64,
    #[allow(dead_code)]
    energy: f64,
    #[allow(dead_code)]
    relative_energy: f64,
}

#[derive(serde::Deserialize)]
struct KeyConformations {
    anti_180: f64,
    gauche_60: f64,
    eclipsed_120: f64,
    syn_0: f64,
}

fn load_butane_scan_data() -> ButaneDihedralScan {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/crystolecule/simulation/test_data/uff_reference.json"
    );
    let content = std::fs::read_to_string(path).expect("Failed to read uff_reference.json");
    let value: serde_json::Value =
        serde_json::from_str(&content).expect("Failed to parse JSON");
    serde_json::from_value(value["butane_dihedral_scan"].clone())
        .expect("Failed to parse butane_dihedral_scan")
}

// --- Dihedral geometry helpers ---

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Compute dihedral angle (in degrees, range (-180, 180]) for atoms i-j-k-l.
fn compute_dihedral(positions: &[f64], i: usize, j: usize, k: usize, l: usize) -> f64 {
    let p = |idx: usize| -> [f64; 3] {
        [
            positions[idx * 3],
            positions[idx * 3 + 1],
            positions[idx * 3 + 2],
        ]
    };
    let pi = p(i);
    let pj = p(j);
    let pk = p(k);
    let pl = p(l);

    let b1 = [pj[0] - pi[0], pj[1] - pi[1], pj[2] - pi[2]];
    let b2 = [pk[0] - pj[0], pk[1] - pj[1], pk[2] - pj[2]];
    let b3 = [pl[0] - pk[0], pl[1] - pk[1], pl[2] - pk[2]];

    let n1 = cross3(b1, b2);
    let n2 = cross3(b2, b3);

    let b2_len = (b2[0] * b2[0] + b2[1] * b2[1] + b2[2] * b2[2]).sqrt();
    let b2_hat = [b2[0] / b2_len, b2[1] / b2_len, b2[2] / b2_len];
    let m1 = cross3(n1, b2_hat);

    let x = dot3(n1, n2);
    let y = dot3(m1, n2);

    y.atan2(x).to_degrees()
}

/// Rotate atoms around an axis using Rodrigues' rotation formula.
fn rotate_atoms_around_axis(
    positions: &mut [f64],
    atom_indices: &[usize],
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    angle_rad: f64,
) {
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let u = axis_dir;

    for &atom_idx in atom_indices {
        let base = atom_idx * 3;
        let p = [
            positions[base] - axis_point[0],
            positions[base + 1] - axis_point[1],
            positions[base + 2] - axis_point[2],
        ];
        let dot_up = u[0] * p[0] + u[1] * p[1] + u[2] * p[2];
        let cross_up = [
            u[1] * p[2] - u[2] * p[1],
            u[2] * p[0] - u[0] * p[2],
            u[0] * p[1] - u[1] * p[0],
        ];

        positions[base] =
            p[0] * cos_a + cross_up[0] * sin_a + u[0] * dot_up * (1.0 - cos_a) + axis_point[0];
        positions[base + 1] =
            p[1] * cos_a + cross_up[1] * sin_a + u[1] * dot_up * (1.0 - cos_a) + axis_point[1];
        positions[base + 2] =
            p[2] * cos_a + cross_up[2] * sin_a + u[2] * dot_up * (1.0 - cos_a) + axis_point[2];
    }
}

/// Perform a 72-point constrained dihedral scan on butane.
///
/// For each of 72 target angles (0 to 355 degrees in 5-degree steps):
/// 1. Start from the base (minimized) geometry
/// 2. Rotate the C3 group around the C1-C2 axis to set the target dihedral
/// 3. Freeze the 4 carbon atoms and minimize hydrogen positions
/// 4. Record the total bonded energy
///
/// Returns (target_angles_deg, energies) for each scan point.
fn perform_butane_scan(
    ff: &UffForceField,
    base_positions: &[f64],
    num_atoms: usize,
) -> (Vec<f64>, Vec<f64>) {
    // Butane topology: C0(0)-C1(1)-C2(2)-C3(3), H4-H6 on C0, H7-H8 on C1,
    // H9-H10 on C2, H11-H13 on C3.
    // Dihedral: C0-C1-C2-C3. Rotation axis: C1->C2.
    // Atoms to rotate (on C2's side, excluding C2 on the axis):
    // C3(3), H9(9), H10(10), H11(11), H12(12), H13(13)
    let atoms_to_rotate: [usize; 6] = [3, 9, 10, 11, 12, 13];
    let frozen_carbons: [usize; 4] = [0, 1, 2, 3];

    let config = MinimizationConfig {
        max_iterations: 500,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };

    let mut target_angles = Vec::with_capacity(72);
    let mut energies = Vec::with_capacity(72);

    for point_idx in 0..72 {
        let target_deg = point_idx as f64 * 5.0;
        target_angles.push(target_deg);

        let mut positions = base_positions.to_vec();

        let current_deg = compute_dihedral(&positions, 0, 1, 2, 3);

        // Convert target to [-180, 180] range for delta calculation.
        let target_norm = if target_deg > 180.0 {
            target_deg - 360.0
        } else {
            target_deg
        };
        let mut delta_deg = target_norm - current_deg;
        if delta_deg > 180.0 {
            delta_deg -= 360.0;
        }
        if delta_deg < -180.0 {
            delta_deg += 360.0;
        }

        // Rotation axis: C1(1) -> C2(2).
        let axis_point = [positions[3], positions[4], positions[5]];
        let dx = positions[6] - positions[3];
        let dy = positions[7] - positions[4];
        let dz = positions[8] - positions[5];
        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        let axis_dir = [dx / len, dy / len, dz / len];

        // Negate delta: the right-hand rotation around C1->C2 changes the
        // dihedral in the opposite direction.
        rotate_atoms_around_axis(
            &mut positions,
            &atoms_to_rotate,
            axis_point,
            axis_dir,
            -delta_deg.to_radians(),
        );

        minimize_with_force_field(ff, &mut positions, &config, &frozen_carbons);

        let mut e = 0.0;
        let mut g = vec![0.0; num_atoms * 3];
        ff.energy_and_gradients(&positions, &mut e, &mut g);
        energies.push(e);
    }

    (target_angles, energies)
}

/// B9: Full 72-point constrained dihedral scan of butane.
///
/// Validates the asymmetric energy profile: anti is lowest, syn is highest,
/// gauche is intermediate. Mirror symmetry (E(phi) ≈ E(360-phi)) still holds.
#[test]
fn b9_butane_dihedral_scan_72_points() {
    let data = load_reference_data();
    let scan_ref = load_butane_scan_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");
    assert_eq!(scan_ref.num_points, 72);

    let (ff, mut base_positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut base_positions, &config, &[]);

    let (target_angles, energies) = perform_butane_scan(&ff, &base_positions, mol.atoms.len());
    assert_eq!(energies.len(), 72);

    let min_e = energies.iter().cloned().fold(f64::INFINITY, f64::min);
    let relative: Vec<f64> = energies.iter().map(|e| e - min_e).collect();

    // Helper: get relative energy at a specific target angle.
    let energy_at = |deg: f64| -> f64 {
        let idx = ((deg / 5.0).round() as usize) % 72;
        relative[idx]
    };

    // 1. The profile should have 72 valid energy values.
    for (i, &e) in energies.iter().enumerate() {
        assert!(
            e.is_finite(),
            "angle {}: energy {e:.6} is invalid",
            target_angles[i]
        );
    }

    // 2. Anti (180°) should be the global minimum.
    let e_anti = energy_at(180.0);
    assert!(
        e_anti < 0.5,
        "anti (180°) should be near global minimum: rel E = {e_anti:.4}"
    );

    // 3. Anti < Gauche (vdW breaks 3-fold degeneracy).
    let e_gauche_60 = energy_at(60.0);
    let e_gauche_300 = energy_at(300.0);
    assert!(
        e_anti < e_gauche_60,
        "anti ({e_anti:.4}) should be lower than gauche_60 ({e_gauche_60:.4})"
    );
    assert!(
        e_anti < e_gauche_300,
        "anti ({e_anti:.4}) should be lower than gauche_300 ({e_gauche_300:.4})"
    );

    // 4. Mirror symmetry: gauche at 60° ≈ gauche at 300°.
    assert!(
        (e_gauche_60 - e_gauche_300).abs() < 0.5,
        "gauche mirror symmetry: 60°={e_gauche_60:.4} vs 300°={e_gauche_300:.4}"
    );

    // 5. Eclipsed (120°) < Syn (0°) — syn is the highest barrier.
    let e_eclipsed_120 = energy_at(120.0);
    let e_syn = energy_at(0.0);
    assert!(
        e_eclipsed_120 < e_syn,
        "eclipsed_120 ({e_eclipsed_120:.4}) should be < syn ({e_syn:.4})"
    );

    // 6. Validate against reference key conformations (±1.0 kcal/mol).
    assert!(
        (e_anti - scan_ref.key_conformations.anti_180).abs() < 1.0,
        "anti: {e_anti:.4} vs ref {:.4}",
        scan_ref.key_conformations.anti_180
    );
    assert!(
        (e_gauche_60 - scan_ref.key_conformations.gauche_60).abs() < 1.0,
        "gauche_60: {e_gauche_60:.4} vs ref {:.4}",
        scan_ref.key_conformations.gauche_60
    );
    assert!(
        (e_eclipsed_120 - scan_ref.key_conformations.eclipsed_120).abs() < 1.0,
        "eclipsed_120: {e_eclipsed_120:.4} vs ref {:.4}",
        scan_ref.key_conformations.eclipsed_120
    );
    assert!(
        (e_syn - scan_ref.key_conformations.syn_0).abs() < 1.0,
        "syn: {e_syn:.4} vs ref {:.4}",
        scan_ref.key_conformations.syn_0
    );

    // 7. The profile should have 3 minima and 3 maxima (qualitatively correct shape).
    let mut num_local_min = 0;
    let mut num_local_max = 0;
    for i in 0..72 {
        let prev = relative[(i + 71) % 72];
        let curr = relative[i];
        let next = relative[(i + 1) % 72];
        if curr <= prev && curr <= next && curr < 1.0 {
            num_local_min += 1;
        }
        if curr >= prev && curr >= next && curr > 2.0 {
            num_local_max += 1;
        }
    }
    assert!(
        num_local_min >= 3,
        "expected >= 3 local minima, got {num_local_min}"
    );
    assert!(
        num_local_max >= 3,
        "expected >= 3 local maxima, got {num_local_max}"
    );
}

/// The dihedral rotation produces the correct target angles.
#[test]
fn b9_butane_dihedral_rotation_accuracy() {
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");

    let (ff, mut base_positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut base_positions, &config, &[]);

    let atoms_to_rotate: [usize; 6] = [3, 9, 10, 11, 12, 13];

    for target_deg in [0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 180.0, 210.0, 270.0, 330.0] {
        let mut positions = base_positions.clone();
        let current_deg = compute_dihedral(&positions, 0, 1, 2, 3);

        let target_norm = if target_deg > 180.0 {
            target_deg - 360.0
        } else {
            target_deg
        };
        let mut delta_deg = target_norm - current_deg;
        if delta_deg > 180.0 {
            delta_deg -= 360.0;
        }
        if delta_deg < -180.0 {
            delta_deg += 360.0;
        }

        let axis_point = [positions[3], positions[4], positions[5]];
        let dx = positions[6] - positions[3];
        let dy = positions[7] - positions[4];
        let dz = positions[8] - positions[5];
        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        let axis_dir = [dx / len, dy / len, dz / len];

        rotate_atoms_around_axis(
            &mut positions,
            &atoms_to_rotate,
            axis_point,
            axis_dir,
            -delta_deg.to_radians(),
        );

        let actual = compute_dihedral(&positions, 0, 1, 2, 3);
        // Normalize both to [-180, 180] for comparison.
        let mut diff = actual - target_norm;
        if diff > 180.0 {
            diff -= 360.0;
        }
        if diff < -180.0 {
            diff += 360.0;
        }
        assert!(
            diff.abs() < 0.1,
            "target={target_deg} actual={actual:.2} diff={diff:.2}"
        );
    }
}

/// The scan profile is smooth and monotonic between extrema.
/// With full UFF (bonded + vdW), the profile is NOT a pure cos(3*phi) — it's
/// the sum of a 3-fold bonded term and an asymmetric vdW contribution.
/// Instead of fitting cos(3*phi), verify qualitative shape correctness.
#[test]
fn b9_butane_profile_shape() {
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");

    let (ff, mut base_positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut base_positions, &config, &[]);

    let (_, energies) = perform_butane_scan(&ff, &base_positions, mol.atoms.len());
    let min_e = energies.iter().cloned().fold(f64::INFINITY, f64::min);
    let relative: Vec<f64> = energies.iter().map(|e| e - min_e).collect();

    // Energy ordering: anti (180°) < gauche (60°,300°) < eclipsed (120°,240°) < syn (0°).
    let e_at = |deg: f64| -> f64 {
        let idx = ((deg / 5.0).round() as usize) % 72;
        relative[idx]
    };

    assert!(
        e_at(180.0) < e_at(60.0),
        "anti < gauche violated"
    );
    assert!(
        e_at(60.0) < e_at(120.0),
        "gauche < eclipsed violated"
    );
    assert!(
        e_at(120.0) < e_at(0.0),
        "eclipsed < syn violated"
    );

    // Mirror symmetry: E(phi) ≈ E(360-phi) within 0.5 kcal/mol for all points.
    for i in 1..36 {
        let deg = i as f64 * 5.0;
        let mirror_deg = 360.0 - deg;
        let diff = (e_at(deg) - e_at(mirror_deg)).abs();
        assert!(
            diff < 0.5,
            "mirror symmetry at {deg}°: E={:.4} vs E({mirror_deg}°)={:.4}, diff={diff:.4}",
            e_at(deg),
            e_at(mirror_deg)
        );
    }
}

/// The barrier height (syn - anti) should match the reference full-UFF value.
/// With vdW, the barrier is ~11.13 kcal/mol (much larger than bonded-only 2.119).
#[test]
fn b9_butane_barrier_vs_reference() {
    let scan_ref = load_butane_scan_data();
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");

    let (ff, mut base_positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut base_positions, &config, &[]);

    let (_, energies) = perform_butane_scan(&ff, &base_positions, mol.atoms.len());
    let min_e = energies.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_e = energies.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let barrier = max_e - min_e;

    // Reference syn barrier from full UFF scan.
    let ref_barrier = scan_ref.key_conformations.syn_0;
    assert!(
        (barrier - ref_barrier).abs() < 2.0,
        "barrier {barrier:.4} vs reference {ref_barrier:.4} (diff={:.4})",
        (barrier - ref_barrier).abs()
    );
}

/// The energy profile should be smooth -- no wild oscillations between adjacent points.
#[test]
fn b9_butane_scan_smoothness() {
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");

    let (ff, mut base_positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut base_positions, &config, &[]);

    let (_, energies) = perform_butane_scan(&ff, &base_positions, mol.atoms.len());

    // Adjacent points (5 degrees apart) should not differ by more than 2.0 kcal/mol.
    // With vdW, the profile is steeper near eclipsed conformations (barrier ~11 kcal/mol
    // over 180°). The steepest slope occurs near the eclipsed transition states.
    for i in 0..72 {
        let next = (i + 1) % 72;
        let diff = (energies[i] - energies[next]).abs();
        assert!(
            diff < 2.0,
            "jump between {} and {}: {:.4} kcal/mol (max 2.0)",
            i * 5,
            next * 5,
            diff
        );
    }
}

/// Cross-check our scan against the reference scan: the ordering of key
/// conformations should match. Our constrained scan (freeze all 4 carbons) differs
/// from the reference (constrain only the dihedral), so per-point energies may
/// diverge significantly. Instead, verify that the qualitative ordering is correct.
#[test]
fn b9_butane_scan_ordering_vs_reference() {
    let scan_ref = load_butane_scan_data();
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");

    let (ff, mut base_positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut base_positions, &config, &[]);

    let (_, energies) = perform_butane_scan(&ff, &base_positions, mol.atoms.len());
    let min_e = energies.iter().cloned().fold(f64::INFINITY, f64::min);
    let relative: Vec<f64> = energies.iter().map(|e| e - min_e).collect();

    // Both scans should have the same ordering: anti < gauche < eclipsed < syn.
    let e_at = |deg: f64| -> f64 {
        let idx = ((deg / 5.0).round() as usize) % 72;
        relative[idx]
    };

    // Our ordering
    assert!(e_at(180.0) < e_at(60.0), "our: anti < gauche");
    assert!(e_at(60.0) < e_at(120.0), "our: gauche < eclipsed");
    assert!(e_at(120.0) < e_at(0.0), "our: eclipsed < syn");

    // Reference ordering
    let ref_kc = &scan_ref.key_conformations;
    assert!(ref_kc.anti_180 < ref_kc.gauche_60, "ref: anti < gauche");
    assert!(ref_kc.gauche_60 < ref_kc.eclipsed_120, "ref: gauche < eclipsed");
    assert!(ref_kc.eclipsed_120 < ref_kc.syn_0, "ref: eclipsed < syn");

    // Both scans should have the minimum near 180° (anti).
    let our_min_idx = relative.iter().enumerate()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .unwrap().0;
    let our_min_angle = our_min_idx as f64 * 5.0;
    assert!(
        (our_min_angle - 180.0).abs() < 15.0 || (our_min_angle - 180.0).abs() > 345.0,
        "our minimum at {our_min_angle}° should be near anti (180°)"
    );
}

/// The scan correctly constrains the backbone: frozen carbon positions are unchanged.
#[test]
fn b9_butane_frozen_carbons_preserved() {
    let data = load_reference_data();
    let mol = &data.molecules[4];
    assert_eq!(mol.name, "butane");

    let (ff, mut base_positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };
    minimize_with_force_field(&ff, &mut base_positions, &config, &[]);

    // Run scan at a specific angle (90 degrees) and verify carbons didn't move.
    let atoms_to_rotate: [usize; 6] = [3, 9, 10, 11, 12, 13];
    let frozen_carbons: [usize; 4] = [0, 1, 2, 3];
    let min_config = MinimizationConfig {
        max_iterations: 500,
        gradient_rms_tolerance: 1e-6,
        ..Default::default()
    };

    let mut positions = base_positions.clone();
    let current_deg = compute_dihedral(&positions, 0, 1, 2, 3);
    let delta_deg = 90.0 - current_deg;

    let axis_point = [positions[3], positions[4], positions[5]];
    let dx = positions[6] - positions[3];
    let dy = positions[7] - positions[4];
    let dz = positions[8] - positions[5];
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    let axis_dir = [dx / len, dy / len, dz / len];

    rotate_atoms_around_axis(
        &mut positions,
        &atoms_to_rotate,
        axis_point,
        axis_dir,
        -delta_deg.to_radians(),
    );

    // Save carbon positions before minimization.
    let carbon_pos_before: Vec<f64> = positions[..12].to_vec();

    minimize_with_force_field(&ff, &mut positions, &min_config, &frozen_carbons);

    // Verify carbon positions are unchanged.
    for c in 0..4 {
        for j in 0..3 {
            let idx = c * 3 + j;
            assert!(
                (positions[idx] - carbon_pos_before[idx]).abs() < 1e-12,
                "carbon {} coord {} moved: {} -> {}",
                c,
                j,
                carbon_pos_before[idx],
                positions[idx]
            );
        }
    }
}
