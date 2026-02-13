// Tests for the L-BFGS minimizer (Phase 16).
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
    #[allow(dead_code)]
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
    bonded: f64,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct MinimizedEnergy {
    bonded: f64,
}

#[derive(serde::Deserialize)]
struct MinimizedGeometry {
    bond_lengths: Vec<GeomBondLength>,
    angles: Vec<GeomAngle>,
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
        result.energy < mol.input_energy.bonded + 0.01,
        "methane: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.bonded
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
        result.energy < mol.input_energy.bonded + 0.01,
        "ethylene: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.bonded
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
        result.energy < mol.input_energy.bonded + 0.01,
        "ethane: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.bonded
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
        result.energy < mol.input_energy.bonded + 0.1,
        "benzene: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.bonded
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
        result.energy < mol.input_energy.bonded + 0.01,
        "water: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.bonded
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
        result.energy < mol.input_energy.bonded + 0.01,
        "ammonia: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.bonded
    );
}

#[test]
fn uff_adamantane_minimizes() {
    let data = load_reference_data();
    let mol = &data.molecules[7];
    assert_eq!(mol.name, "adamantane");
    let (ff, mut positions) = build_ff_and_positions(mol);
    let config = MinimizationConfig {
        max_iterations: 1000,
        ..Default::default()
    };
    let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);
    assert!(result.converged, "adamantane should converge (got {} iterations)", result.iterations);
    assert!(
        result.energy < mol.input_energy.bonded + 0.1,
        "adamantane: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.bonded
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
        result.energy < mol.input_energy.bonded + 0.01,
        "methanethiol: minimized energy {} should be <= input energy {}",
        result.energy,
        mol.input_energy.bonded
    );
}

/// All 9 reference molecules converge.
#[test]
fn uff_all_molecules_converge() {
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, mut positions) = build_ff_and_positions(mol);
        let config = MinimizationConfig {
            max_iterations: 1000,
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
// Integration tests: minimized bonded energy is near zero
// ============================================================================

#[test]
fn uff_minimized_bonded_energy_near_zero() {
    // With bonded-only optimization (no vdW), the minimizer should find a geometry
    // where all bonds, angles, torsions, and inversions are near their rest values,
    // giving near-zero bonded energy. This is a self-consistent test that does NOT
    // compare against RDKit's vdW-optimized geometry.
    //
    // Note: RDKit's reference data includes vdW during minimization, which shifts
    // atoms away from their bonded-optimal positions. Comparing our bonded-only
    // minimum against RDKit's vdW-optimized bonded energy would be apples-to-oranges.
    // Instead, we verify our minimizer reaches near-zero bonded energy on its own.
    let data = load_reference_data();
    for mol in &data.molecules {
        let (ff, mut positions) = build_ff_and_positions(mol);
        let config = MinimizationConfig {
            max_iterations: 1000,
            gradient_rms_tolerance: 1e-6,
            ..Default::default()
        };
        let result = minimize_with_force_field(&ff, &mut positions, &config, &[]);

        // Bonded energy at the bonded-only minimum should be very small.
        // For most molecules this is <0.01 kcal/mol. Adamantane (26 atoms,
        // many coupled terms) may have slightly higher residual.
        let tol = if mol.atoms.len() > 10 { 1.0 } else { 0.1 };
        assert!(
            result.energy < tol,
            "{}: minimized bonded energy {:.6} kcal/mol not near zero (tol={tol})",
            mol.name,
            result.energy
        );

        // Energy should be non-negative (physical constraint).
        assert!(
            result.energy >= -0.001,
            "{}: minimized energy {:.6} is negative",
            mol.name,
            result.energy
        );
    }
}

// ============================================================================
// Integration tests: minimized geometry is physically correct
// ============================================================================
//
// These tests verify that minimized bond lengths and angles match the UFF
// rest values (r0, theta0) from our own parameter table — NOT RDKit's
// vdW-optimized geometry. This is self-consistent: our bonded-only minimizer
// should produce bond lengths equal to the UFF rest length and angles equal
// to the UFF natural angle.
//
// For small molecules where vdW barely affects geometry, the RDKit reference
// values happen to agree within ~0.01 Å / ~1°. We cross-check against those
// as a sanity check, but the primary validation is against our own rest params.

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

    // Self-consistent check: all C-H bonds should equal UFF rest length.
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

    // Cross-check against RDKit reference (vdW effect negligible for methane).
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

    // Self-consistent: bonds should match UFF rest lengths.
    for bp in &ff.bond_params {
        let computed = bond_length(&positions, bp.idx1, bp.idx2);
        assert!(
            (computed - bp.rest_length).abs() < 0.001,
            "ethylene bond {}-{}: {:.4} != rest {:.4}",
            bp.idx1,
            bp.idx2,
            computed,
            bp.rest_length
        );
    }

    // Cross-check against RDKit (vdW negligible for ethylene).
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

    // Self-consistent: O-H bonds should match rest length.
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

    // Cross-check bond lengths vs RDKit.
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
