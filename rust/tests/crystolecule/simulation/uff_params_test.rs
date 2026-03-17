use rust_lib_flutter_cad::crystolecule::simulation::uff::params::{
    AMIDE_BOND_ORDER, G, LAMBDA, UFF_PARAMS, calc_bond_force_constant, calc_bond_rest_length,
    get_uff_params,
};
use serde::Deserialize;
use std::fs;

// ============================================================================
// Helper: assert float equality with tolerance
// ============================================================================

fn assert_approx_eq(actual: f64, expected: f64, tol: f64, msg: &str) {
    let diff = (actual - expected).abs();
    assert!(
        diff < tol,
        "{msg}: expected {expected}, got {actual} (diff={diff}, tol={tol})"
    );
}

// ============================================================================
// Test A1: Parameter table spot-checks
// ============================================================================
// Validates exact values for key atom types: C_3, N_3, O_3, H_, Si3
// Reference: RDKit testUFFParams (testUFFForceField.cpp)

#[test]
fn test_a1_param_table_count() {
    assert_eq!(UFF_PARAMS.len(), 127);
}

#[test]
fn test_a1_constants() {
    assert_approx_eq(LAMBDA, 0.1332, 1e-10, "LAMBDA");
    assert_approx_eq(G, 332.06, 1e-10, "G");
    assert_approx_eq(AMIDE_BOND_ORDER, 1.41, 1e-10, "AMIDE_BOND_ORDER");
}

#[test]
fn test_a1_lookup_missing() {
    assert!(get_uff_params("NONEXISTENT").is_none());
}

#[test]
fn test_a1_hydrogen() {
    let p = get_uff_params("H_").expect("H_ not found");
    assert_eq!(p.label, "H_");
    assert_approx_eq(p.r1, 0.354, 1e-10, "H_ r1");
    assert_approx_eq(p.theta0, 180.0, 1e-10, "H_ theta0");
    assert_approx_eq(p.x1, 2.886, 1e-10, "H_ x1");
    assert_approx_eq(p.d1, 0.044, 1e-10, "H_ d1");
    assert_approx_eq(p.zeta, 12.0, 1e-10, "H_ zeta");
    assert_approx_eq(p.z1, 0.712, 1e-10, "H_ z1");
    assert_approx_eq(p.v1, 0.0, 1e-10, "H_ v1");
    assert_approx_eq(p.u1, 0.0, 1e-10, "H_ u1");
    assert_approx_eq(p.gmp_xi, 4.528, 1e-10, "H_ gmp_xi");
    assert_approx_eq(p.gmp_hardness, 6.9452, 1e-10, "H_ gmp_hardness");
    assert_approx_eq(p.gmp_radius, 0.371, 1e-10, "H_ gmp_radius");
}

#[test]
fn test_a1_carbon_sp3() {
    // Reference: RDKit testUFFParams: C_3: r1=0.757, theta0=109.47, x1=3.851,
    // D1=0.105, Z1=1.912, V1=2.119
    let p = get_uff_params("C_3").expect("C_3 not found");
    assert_eq!(p.label, "C_3");
    assert_approx_eq(p.r1, 0.757, 1e-10, "C_3 r1");
    assert_approx_eq(p.theta0, 109.47, 1e-10, "C_3 theta0");
    assert_approx_eq(p.x1, 3.851, 1e-10, "C_3 x1");
    assert_approx_eq(p.d1, 0.105, 1e-10, "C_3 d1");
    assert_approx_eq(p.z1, 1.912, 1e-10, "C_3 z1");
    assert_approx_eq(p.v1, 2.119, 1e-10, "C_3 v1");
    assert_approx_eq(p.gmp_xi, 5.343, 1e-10, "C_3 gmp_xi");
}

#[test]
fn test_a1_carbon_aromatic() {
    let p = get_uff_params("C_R").expect("C_R not found");
    assert_approx_eq(p.r1, 0.729, 1e-10, "C_R r1");
    assert_approx_eq(p.theta0, 120.0, 1e-10, "C_R theta0");
}

#[test]
fn test_a1_carbon_sp2() {
    let p = get_uff_params("C_2").expect("C_2 not found");
    assert_approx_eq(p.r1, 0.732, 1e-10, "C_2 r1");
    assert_approx_eq(p.theta0, 120.0, 1e-10, "C_2 theta0");
}

#[test]
fn test_a1_nitrogen_sp3() {
    let p = get_uff_params("N_3").expect("N_3 not found");
    assert_approx_eq(p.r1, 0.7, 1e-10, "N_3 r1");
    assert_approx_eq(p.theta0, 106.7, 1e-10, "N_3 theta0");
    assert_approx_eq(p.z1, 2.544, 1e-10, "N_3 z1");
    assert_approx_eq(p.gmp_xi, 6.899, 1e-10, "N_3 gmp_xi");
}

#[test]
fn test_a1_oxygen_sp3() {
    let p = get_uff_params("O_3").expect("O_3 not found");
    assert_approx_eq(p.r1, 0.658, 1e-10, "O_3 r1");
    assert_approx_eq(p.theta0, 104.51, 1e-10, "O_3 theta0");
    assert_approx_eq(p.z1, 2.3, 1e-10, "O_3 z1");
    assert_approx_eq(p.gmp_xi, 8.741, 1e-10, "O_3 gmp_xi");
}

#[test]
fn test_a1_silicon() {
    let p = get_uff_params("Si3").expect("Si3 not found");
    assert_approx_eq(p.r1, 1.117, 1e-10, "Si3 r1");
    assert_approx_eq(p.theta0, 109.47, 1e-10, "Si3 theta0");
    assert_approx_eq(p.z1, 2.323, 1e-10, "Si3 z1");
    assert_approx_eq(p.v1, 1.225, 1e-10, "Si3 v1");
}

#[test]
fn test_a1_sulfur_types() {
    let p = get_uff_params("S_3+2").expect("S_3+2 not found");
    assert_approx_eq(p.r1, 1.064, 1e-10, "S_3+2 r1");
    assert_approx_eq(p.theta0, 92.1, 1e-10, "S_3+2 theta0");

    let p4 = get_uff_params("S_3+4").expect("S_3+4 not found");
    assert_approx_eq(p4.r1, 1.049, 1e-10, "S_3+4 r1");

    let p6 = get_uff_params("S_3+6").expect("S_3+6 not found");
    assert_approx_eq(p6.r1, 1.027, 1e-10, "S_3+6 r1");
}

#[test]
fn test_a1_halogens() {
    let f = get_uff_params("F_").expect("F_ not found");
    assert_approx_eq(f.r1, 0.668, 1e-10, "F_ r1");

    let cl = get_uff_params("Cl").expect("Cl not found");
    assert_approx_eq(cl.r1, 1.044, 1e-10, "Cl r1");

    let br = get_uff_params("Br").expect("Br not found");
    assert_approx_eq(br.r1, 1.192, 1e-10, "Br r1");

    let i = get_uff_params("I_").expect("I_ not found");
    assert_approx_eq(i.r1, 1.382, 1e-10, "I_ r1");
}

#[test]
fn test_a1_phosphorus_types() {
    let p3 = get_uff_params("P_3+3").expect("P_3+3 not found");
    assert_approx_eq(p3.r1, 1.101, 1e-10, "P_3+3 r1");

    let p5 = get_uff_params("P_3+5").expect("P_3+5 not found");
    assert_approx_eq(p5.r1, 1.056, 1e-10, "P_3+5 r1");
}

// ============================================================================
// Test A2: Bond rest length and force constant calculations
// ============================================================================
// Reference: RDKit testUFF1 (testUFFForceField.cpp):
//   C_3–C_3: r0=1.514, k=699.5918
//   C_2=C_2: r0=1.32883, k=1034.69
//   C_3–N_3: r0=1.451071, k=1057.27

#[test]
fn test_a2_bond_c3_c3_single() {
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    // For homonuclear bond: rBO = 0 (ln(1)=0), rEN = 0 (same Xi)
    // r0 = 0.757 + 0.757 = 1.514
    assert_approx_eq(r0, 1.514, 1e-3, "C_3-C_3 r0");
    assert_approx_eq(kb, 699.5918, 0.1, "C_3-C_3 kb");
}

#[test]
fn test_a2_bond_c2_c2_double() {
    let c2 = get_uff_params("C_2").unwrap();
    let r0 = calc_bond_rest_length(2.0, c2, c2);
    let kb = calc_bond_force_constant(r0, c2, c2);

    assert_approx_eq(r0, 1.32883, 1e-3, "C_2=C_2 r0");
    assert_approx_eq(kb, 1034.69, 0.2, "C_2=C_2 kb");
}

#[test]
fn test_a2_bond_c3_n3_single() {
    let c3 = get_uff_params("C_3").unwrap();
    let n3 = get_uff_params("N_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, n3);
    let kb = calc_bond_force_constant(r0, c3, n3);

    assert_approx_eq(r0, 1.451071, 1e-3, "C_3-N_3 r0");
    assert_approx_eq(kb, 1057.27, 0.2, "C_3-N_3 kb");
}

#[test]
fn test_a2_bond_homonuclear_symmetry() {
    // calc_bond_rest_length(bo, A, B) == calc_bond_rest_length(bo, B, A)
    let c3 = get_uff_params("C_3").unwrap();
    let n3 = get_uff_params("N_3").unwrap();

    let r0_cn = calc_bond_rest_length(1.0, c3, n3);
    let r0_nc = calc_bond_rest_length(1.0, n3, c3);
    assert_approx_eq(r0_cn, r0_nc, 1e-10, "bond rest length symmetry");

    let kb_cn = calc_bond_force_constant(r0_cn, c3, n3);
    let kb_nc = calc_bond_force_constant(r0_nc, n3, c3);
    assert_approx_eq(kb_cn, kb_nc, 1e-10, "bond force constant symmetry");
}

#[test]
fn test_a2_bond_single_bond_order_no_correction() {
    // For bond order 1.0, the Pauling correction rBO = -lambda*(ri+rj)*ln(1.0) = 0
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    // For homonuclear: rEN = 0 too, so r0 = ri + rj exactly
    assert_approx_eq(r0, c3.r1 + c3.r1, 1e-10, "single bond homonuclear = 2*r1");
}

#[test]
fn test_a2_bond_order_decreases_length() {
    // Higher bond order → shorter bond
    let c2 = get_uff_params("C_2").unwrap();
    let r0_single = calc_bond_rest_length(1.0, c2, c2);
    let r0_double = calc_bond_rest_length(2.0, c2, c2);
    let r0_triple = calc_bond_rest_length(3.0, c2, c2);

    assert!(
        r0_single > r0_double,
        "double bond should be shorter than single"
    );
    assert!(
        r0_double > r0_triple,
        "triple bond should be shorter than double"
    );
}

// ============================================================================
// Test A2: Cross-check against reference JSON bond parameters
// ============================================================================
// Validates bond rest lengths and force constants against RDKit-generated
// reference data for all 9 test molecules.

#[derive(Deserialize)]
struct ReferenceData {
    molecules: Vec<MoleculeData>,
}

#[derive(Deserialize)]
struct MoleculeData {
    name: String,
    atoms: Vec<AtomData>,
    bonds: Vec<BondData>,
    bond_params: Vec<BondParamData>,
}

#[derive(Deserialize)]
struct AtomData {
    #[allow(dead_code)]
    index: usize,
    #[allow(dead_code)]
    atomic_number: i32,
    #[allow(dead_code)]
    symbol: String,
    uff_type: String,
}

#[derive(Deserialize)]
struct BondData {
    atom1: usize,
    atom2: usize,
    order: f64,
}

#[derive(Deserialize)]
struct BondParamData {
    atoms: [usize; 2],
    kb: f64,
    r0: f64,
}

fn load_reference_data() -> ReferenceData {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/crystolecule/simulation/test_data/uff_reference.json"
    );
    let json = fs::read_to_string(path).expect("failed to read uff_reference.json");
    serde_json::from_str(&json).expect("failed to parse uff_reference.json")
}

#[test]
fn test_a2_reference_json_bond_params_all_molecules() {
    let data = load_reference_data();

    for mol in &data.molecules {
        // Build a lookup from (atom1, atom2) → bond order
        let bond_order_map: std::collections::HashMap<(usize, usize), f64> = mol
            .bonds
            .iter()
            .map(|b| ((b.atom1, b.atom2), b.order))
            .collect();

        for bp in &mol.bond_params {
            let atom1_idx = bp.atoms[0];
            let atom2_idx = bp.atoms[1];

            let uff_type1 = &mol.atoms[atom1_idx].uff_type;
            let uff_type2 = &mol.atoms[atom2_idx].uff_type;

            let params1 = get_uff_params(uff_type1).unwrap_or_else(|| {
                panic!(
                    "{}: no UFF params for type '{uff_type1}' (atom {atom1_idx})",
                    mol.name
                )
            });
            let params2 = get_uff_params(uff_type2).unwrap_or_else(|| {
                panic!(
                    "{}: no UFF params for type '{uff_type2}' (atom {atom2_idx})",
                    mol.name
                )
            });

            // Find bond order from the bond list
            let bond_order = bond_order_map
                .get(&(atom1_idx, atom2_idx))
                .or_else(|| bond_order_map.get(&(atom2_idx, atom1_idx)))
                .unwrap_or_else(|| {
                    panic!(
                        "{}: no bond between atoms {atom1_idx} and {atom2_idx}",
                        mol.name
                    )
                });

            let r0 = calc_bond_rest_length(*bond_order, params1, params2);
            let kb = calc_bond_force_constant(r0, params1, params2);

            assert_approx_eq(
                r0,
                bp.r0,
                1e-4,
                &format!(
                    "{}: bond {atom1_idx}-{atom2_idx} ({uff_type1}-{uff_type2}, order={bond_order}) r0",
                    mol.name
                ),
            );
            assert_approx_eq(
                kb,
                bp.kb,
                0.01,
                &format!(
                    "{}: bond {atom1_idx}-{atom2_idx} ({uff_type1}-{uff_type2}, order={bond_order}) kb",
                    mol.name
                ),
            );
        }
    }
}

// Individual molecule spot-checks for key bond types

#[test]
fn test_a2_reference_methane_ch_bond() {
    // CH4: C_3-H_ single bond
    let c3 = get_uff_params("C_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, h);
    let kb = calc_bond_force_constant(r0, c3, h);

    // Reference: kb=662.138778, r0=1.109401
    assert_approx_eq(r0, 1.109401, 1e-4, "methane C-H r0");
    assert_approx_eq(kb, 662.138778, 0.01, "methane C-H kb");
}

#[test]
fn test_a2_reference_benzene_aromatic() {
    // Benzene: C_R-C_R aromatic (bond order 1.5)
    let cr = get_uff_params("C_R").unwrap();
    let r0 = calc_bond_rest_length(1.5, cr, cr);
    let kb = calc_bond_force_constant(r0, cr, cr);

    // Reference: kb=925.310108, r0=1.379256
    assert_approx_eq(r0, 1.379256, 1e-4, "benzene C_R-C_R r0");
    assert_approx_eq(kb, 925.310108, 0.01, "benzene C_R-C_R kb");
}

#[test]
fn test_a2_reference_water_oh_bond() {
    // Water: O_3-H_ single bond
    let o3 = get_uff_params("O_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, o3, h);
    let kb = calc_bond_force_constant(r0, o3, h);

    // Reference: kb=1119.990411, r0=0.990254
    assert_approx_eq(r0, 0.990254, 1e-4, "water O-H r0");
    assert_approx_eq(kb, 1119.990411, 0.01, "water O-H kb");
}

#[test]
fn test_a2_reference_ammonia_nh_bond() {
    // Ammonia: N_3-H_ single bond
    let n3 = get_uff_params("N_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, n3, h);
    let kb = calc_bond_force_constant(r0, n3, h);

    // Reference: kb=1055.890523, r0=1.044419
    assert_approx_eq(r0, 1.044419, 1e-4, "ammonia N-H r0");
    assert_approx_eq(kb, 1055.890523, 0.01, "ammonia N-H kb");
}

#[test]
fn test_a2_reference_methanethiol_cs_bond() {
    // Methanethiol: C_3-S_3+2 single bond
    let c3 = get_uff_params("C_3").unwrap();
    let s = get_uff_params("S_3+2").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, s);
    let kb = calc_bond_force_constant(r0, c3, s);

    // Reference: kb=575.241328, r0=1.813747
    assert_approx_eq(r0, 1.813747, 1e-4, "methanethiol C-S r0");
    assert_approx_eq(kb, 575.241328, 0.01, "methanethiol C-S kb");
}

#[test]
fn test_a2_reference_methanethiol_sh_bond() {
    // Methanethiol: S_3+2-H_ single bond
    let s = get_uff_params("S_3+2").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, s, h);
    let kb = calc_bond_force_constant(r0, s, h);

    // Reference: kb=458.54759, r0=1.40733
    assert_approx_eq(r0, 1.40733, 1e-4, "methanethiol S-H r0");
    assert_approx_eq(kb, 458.54759, 0.01, "methanethiol S-H kb");
}

#[test]
fn test_a2_reference_ethylene_c2h_bond() {
    // Ethylene: C_2-H_ single bond
    let c2 = get_uff_params("C_2").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, c2, h);
    let kb = calc_bond_force_constant(r0, c2, h);

    // Reference: kb=708.967804, r0=1.084416
    assert_approx_eq(r0, 1.084416, 1e-4, "ethylene C_2-H r0");
    assert_approx_eq(kb, 708.967804, 0.01, "ethylene C_2-H kb");
}
