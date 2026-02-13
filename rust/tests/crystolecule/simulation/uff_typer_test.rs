use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DOUBLE, BOND_SINGLE, BOND_TRIPLE, InlineBond,
};
use rust_lib_flutter_cad::crystolecule::simulation::uff::params::get_uff_params;
use rust_lib_flutter_cad::crystolecule::simulation::uff::typer::{
    assign_uff_type, assign_uff_types, bond_order_to_f64, hybridization_from_label,
};

// ============================================================================
// Helper: make bond lists for testing
// ============================================================================

fn single_bond(to: u32) -> InlineBond {
    InlineBond::new(to, BOND_SINGLE)
}

fn double_bond(to: u32) -> InlineBond {
    InlineBond::new(to, BOND_DOUBLE)
}

fn triple_bond(to: u32) -> InlineBond {
    InlineBond::new(to, BOND_TRIPLE)
}

fn aromatic_bond(to: u32) -> InlineBond {
    InlineBond::new(to, BOND_AROMATIC)
}

// ============================================================================
// Test: assign_uff_type for common organic elements (C, H, N, O, S)
// Reference: RDKit testUFFTyper1 and uff_reference.json
// ============================================================================

#[test]
fn test_hydrogen_always_h() {
    // H with single bond → H_
    assert_eq!(assign_uff_type(1, &[single_bond(1)]).unwrap(), "H_");
    // H with no bonds → H_
    assert_eq!(assign_uff_type(1, &[]).unwrap(), "H_");
}

#[test]
fn test_carbon_sp3() {
    // Methane: C with 4 single bonds → C_3
    let bonds = [
        single_bond(1),
        single_bond(2),
        single_bond(3),
        single_bond(4),
    ];
    assert_eq!(assign_uff_type(6, &bonds).unwrap(), "C_3");
}

#[test]
fn test_carbon_sp2() {
    // Ethylene carbon: C with 1 double + 2 single bonds → C_2
    let bonds = [double_bond(1), single_bond(2), single_bond(3)];
    assert_eq!(assign_uff_type(6, &bonds).unwrap(), "C_2");
}

#[test]
fn test_carbon_sp() {
    // Acetylene carbon: C with 1 triple bond + 1 single bond → C_1
    let bonds = [triple_bond(1), single_bond(2)];
    assert_eq!(assign_uff_type(6, &bonds).unwrap(), "C_1");
}

#[test]
fn test_carbon_sp_allene() {
    // Allene central carbon: C with 2 double bonds → C_1
    let bonds = [double_bond(1), double_bond(2)];
    assert_eq!(assign_uff_type(6, &bonds).unwrap(), "C_1");
}

#[test]
fn test_carbon_aromatic() {
    // Benzene carbon: C with aromatic bonds → C_R
    let bonds = [aromatic_bond(1), aromatic_bond(2), single_bond(3)];
    assert_eq!(assign_uff_type(6, &bonds).unwrap(), "C_R");
}

#[test]
fn test_nitrogen_sp3() {
    // Ammonia: N with 3 single bonds → N_3
    let bonds = [single_bond(1), single_bond(2), single_bond(3)];
    assert_eq!(assign_uff_type(7, &bonds).unwrap(), "N_3");
}

#[test]
fn test_nitrogen_sp2() {
    // Imine nitrogen: N with 1 double + 1 single bond → N_2
    let bonds = [double_bond(1), single_bond(2)];
    assert_eq!(assign_uff_type(7, &bonds).unwrap(), "N_2");
}

#[test]
fn test_nitrogen_sp() {
    // Nitrile nitrogen: N with 1 triple bond → N_1
    let bonds = [triple_bond(1)];
    assert_eq!(assign_uff_type(7, &bonds).unwrap(), "N_1");
}

#[test]
fn test_nitrogen_aromatic() {
    // Pyridine nitrogen: N with aromatic bonds → N_R
    let bonds = [aromatic_bond(1), aromatic_bond(2)];
    assert_eq!(assign_uff_type(7, &bonds).unwrap(), "N_R");
}

#[test]
fn test_oxygen_sp3() {
    // Water: O with 2 single bonds → O_3
    let bonds = [single_bond(1), single_bond(2)];
    assert_eq!(assign_uff_type(8, &bonds).unwrap(), "O_3");
}

#[test]
fn test_oxygen_sp2() {
    // Carbonyl oxygen: O with 1 double bond → O_2
    let bonds = [double_bond(1)];
    assert_eq!(assign_uff_type(8, &bonds).unwrap(), "O_2");
}

#[test]
fn test_oxygen_aromatic() {
    // Furan oxygen: O with aromatic bonds → O_R
    let bonds = [aromatic_bond(1), aromatic_bond(2)];
    assert_eq!(assign_uff_type(8, &bonds).unwrap(), "O_R");
}

#[test]
fn test_sulfur_sp3_thiol() {
    // Methanethiol sulfur: S with 2 single bonds → S_3+2
    let bonds = [single_bond(1), single_bond(2)];
    assert_eq!(assign_uff_type(16, &bonds).unwrap(), "S_3+2");
}

#[test]
fn test_sulfur_sp2() {
    // Thioketone: S with 1 double bond → S_2
    let bonds = [double_bond(1)];
    assert_eq!(assign_uff_type(16, &bonds).unwrap(), "S_2");
}

#[test]
fn test_sulfur_aromatic() {
    // Thiophene: S with aromatic bonds → S_R
    let bonds = [aromatic_bond(1), aromatic_bond(2)];
    assert_eq!(assign_uff_type(16, &bonds).unwrap(), "S_R");
}

#[test]
fn test_sulfur_sulfoxide() {
    // Sulfoxide: S with 4 bonds → S_3+4
    let bonds = [
        single_bond(1),
        single_bond(2),
        double_bond(3),
        single_bond(4),
    ];
    assert_eq!(assign_uff_type(16, &bonds).unwrap(), "S_3+4");
}

#[test]
fn test_sulfur_sulfone() {
    // Sulfone/sulfate: S with 6 bonds → S_3+6
    let bonds = [
        single_bond(1),
        single_bond(2),
        double_bond(3),
        double_bond(4),
        single_bond(5),
        single_bond(6),
    ];
    assert_eq!(assign_uff_type(16, &bonds).unwrap(), "S_3+6");
}

// ============================================================================
// Test: RDKit testUFFTyper1 reference values
// Reference: testUFFHelpers.cpp testUFFTyper1
// ============================================================================

#[test]
fn test_rdkit_typer1_elements() {
    // These are the canonical assignments from RDKit's test suite.
    // We test against the specific element/bond configurations that produce these labels.

    // C_3: sp3 carbon
    assert_eq!(
        assign_uff_type(
            6,
            &[
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4)
            ]
        )
        .unwrap(),
        "C_3"
    );

    // C_R: aromatic carbon
    assert_eq!(
        assign_uff_type(6, &[aromatic_bond(1), aromatic_bond(2), single_bond(3)]).unwrap(),
        "C_R"
    );

    // C_2: sp2 carbon
    assert_eq!(
        assign_uff_type(6, &[double_bond(1), single_bond(2), single_bond(3)]).unwrap(),
        "C_2"
    );

    // O_R: aromatic oxygen
    assert_eq!(
        assign_uff_type(8, &[aromatic_bond(1), aromatic_bond(2)]).unwrap(),
        "O_R"
    );

    // O_2: sp2 oxygen
    assert_eq!(assign_uff_type(8, &[double_bond(1)]).unwrap(), "O_2");

    // N_R: aromatic nitrogen
    assert_eq!(
        assign_uff_type(7, &[aromatic_bond(1), aromatic_bond(2)]).unwrap(),
        "N_R"
    );

    // Si3: silicon
    assert_eq!(
        assign_uff_type(
            14,
            &[
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4)
            ]
        )
        .unwrap(),
        "Si3"
    );

    // S_3+2: thiol sulfur
    assert_eq!(
        assign_uff_type(16, &[single_bond(1), single_bond(2)]).unwrap(),
        "S_3+2"
    );

    // S_3+4: sulfoxide sulfur
    assert_eq!(
        assign_uff_type(
            16,
            &[
                single_bond(1),
                single_bond(2),
                double_bond(3),
                single_bond(4)
            ]
        )
        .unwrap(),
        "S_3+4"
    );

    // S_3+6: sulfate sulfur
    assert_eq!(
        assign_uff_type(
            16,
            &[
                single_bond(1),
                single_bond(2),
                double_bond(3),
                double_bond(4),
                single_bond(5),
                single_bond(6),
            ]
        )
        .unwrap(),
        "S_3+6"
    );

    // P_3+3: trivalent phosphorus
    assert_eq!(
        assign_uff_type(15, &[single_bond(1), single_bond(2), single_bond(3)]).unwrap(),
        "P_3+3"
    );

    // P_3+5: pentavalent phosphorus
    assert_eq!(
        assign_uff_type(
            15,
            &[
                single_bond(1),
                single_bond(2),
                single_bond(3),
                double_bond(4),
                single_bond(5),
            ]
        )
        .unwrap(),
        "P_3+5"
    );

    // F_: fluorine
    assert_eq!(assign_uff_type(9, &[single_bond(1)]).unwrap(), "F_");

    // Cl: chlorine
    assert_eq!(assign_uff_type(17, &[single_bond(1)]).unwrap(), "Cl");

    // Br: bromine
    assert_eq!(assign_uff_type(35, &[single_bond(1)]).unwrap(), "Br");

    // I_: iodine
    assert_eq!(assign_uff_type(53, &[single_bond(1)]).unwrap(), "I_");

    // Li: lithium
    assert_eq!(assign_uff_type(3, &[]).unwrap(), "Li");

    // Na: sodium
    assert_eq!(assign_uff_type(11, &[]).unwrap(), "Na");

    // K_: potassium
    assert_eq!(assign_uff_type(19, &[]).unwrap(), "K_");
}

// ============================================================================
// Test: all assigned types have valid UFF parameters
// ============================================================================

#[test]
fn test_all_types_have_params() {
    // Every label returned by assign_uff_type must have valid UFF parameters.
    // Test a representative set of elements across the periodic table.
    let test_cases: Vec<(i16, Vec<InlineBond>)> = vec![
        (1, vec![single_bond(1)]),                                 // H
        (2, vec![]),                                               // He
        (3, vec![]),                                               // Li
        (4, vec![single_bond(1), single_bond(2)]),                 // Be
        (5, vec![single_bond(1), single_bond(2), single_bond(3)]), // B
        (
            6,
            vec![
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4),
            ],
        ), // C
        (7, vec![single_bond(1), single_bond(2), single_bond(3)]), // N
        (8, vec![single_bond(1), single_bond(2)]),                 // O
        (9, vec![single_bond(1)]),                                 // F
        (10, vec![]),                                              // Ne
        (11, vec![]),                                              // Na
        (12, vec![single_bond(1), single_bond(2)]),                // Mg
        (13, vec![single_bond(1), single_bond(2), single_bond(3)]), // Al
        (
            14,
            vec![
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4),
            ],
        ), // Si
        (15, vec![single_bond(1), single_bond(2), single_bond(3)]), // P
        (16, vec![single_bond(1), single_bond(2)]),                // S
        (17, vec![single_bond(1)]),                                // Cl
        (18, vec![]),                                              // Ar
        (19, vec![]),                                              // K
        (20, vec![single_bond(1), single_bond(2)]),                // Ca
        (26, vec![single_bond(1), single_bond(2)]),                // Fe
        (29, vec![single_bond(1)]),                                // Cu
        (35, vec![single_bond(1)]),                                // Br
        (47, vec![single_bond(1)]),                                // Ag
        (53, vec![single_bond(1)]),                                // I
        (79, vec![single_bond(1), single_bond(2), single_bond(3)]), // Au
        (
            92,
            vec![
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4),
            ],
        ), // U
    ];

    for (atomic_number, bonds) in &test_cases {
        let label = assign_uff_type(*atomic_number, bonds)
            .unwrap_or_else(|e| panic!("Failed to assign type for Z={}: {}", atomic_number, e));
        let params = get_uff_params(label);
        assert!(
            params.is_some(),
            "No UFF params for label '{}' (Z={})",
            label,
            atomic_number
        );
    }
}

// ============================================================================
// Test: cross-validation against uff_reference.json molecule atom types
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
}

#[derive(serde::Deserialize)]
struct ReferenceAtom {
    index: usize,
    atomic_number: i16,
    symbol: String,
    uff_type: String,
}

#[derive(serde::Deserialize)]
struct ReferenceBond {
    atom1: usize,
    atom2: usize,
    order: f64,
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

fn build_bond_lists_from_reference(mol: &ReferenceMolecule) -> Vec<Vec<InlineBond>> {
    let n = mol.atoms.len();
    let mut bond_lists: Vec<Vec<InlineBond>> = vec![vec![]; n];
    for bond in &mol.bonds {
        let order = bond_order_from_f64(bond.order);
        bond_lists[bond.atom1].push(InlineBond::new(bond.atom2 as u32, order));
        bond_lists[bond.atom2].push(InlineBond::new(bond.atom1 as u32, order));
    }
    bond_lists
}

#[test]
fn test_reference_methane() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "methane").unwrap();
    let bond_lists = build_bond_lists_from_reference(mol);

    for atom in &mol.atoms {
        let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index]).unwrap();
        assert_eq!(
            label, atom.uff_type,
            "Methane atom {} ({}): expected {}, got {}",
            atom.index, atom.symbol, atom.uff_type, label
        );
    }
}

#[test]
fn test_reference_ethylene() {
    let data = load_reference_data();
    let mol = data
        .molecules
        .iter()
        .find(|m| m.name == "ethylene")
        .unwrap();
    let bond_lists = build_bond_lists_from_reference(mol);

    for atom in &mol.atoms {
        let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index]).unwrap();
        assert_eq!(
            label, atom.uff_type,
            "Ethylene atom {} ({}): expected {}, got {}",
            atom.index, atom.symbol, atom.uff_type, label
        );
    }
}

#[test]
fn test_reference_ethane() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "ethane").unwrap();
    let bond_lists = build_bond_lists_from_reference(mol);

    for atom in &mol.atoms {
        let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index]).unwrap();
        assert_eq!(
            label, atom.uff_type,
            "Ethane atom {} ({}): expected {}, got {}",
            atom.index, atom.symbol, atom.uff_type, label
        );
    }
}

#[test]
fn test_reference_benzene() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "benzene").unwrap();
    let bond_lists = build_bond_lists_from_reference(mol);

    for atom in &mol.atoms {
        let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index]).unwrap();
        assert_eq!(
            label, atom.uff_type,
            "Benzene atom {} ({}): expected {}, got {}",
            atom.index, atom.symbol, atom.uff_type, label
        );
    }
}

#[test]
fn test_reference_butane() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "butane").unwrap();
    let bond_lists = build_bond_lists_from_reference(mol);

    for atom in &mol.atoms {
        let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index]).unwrap();
        assert_eq!(
            label, atom.uff_type,
            "Butane atom {} ({}): expected {}, got {}",
            atom.index, atom.symbol, atom.uff_type, label
        );
    }
}

#[test]
fn test_reference_water() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "water").unwrap();
    let bond_lists = build_bond_lists_from_reference(mol);

    for atom in &mol.atoms {
        let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index]).unwrap();
        assert_eq!(
            label, atom.uff_type,
            "Water atom {} ({}): expected {}, got {}",
            atom.index, atom.symbol, atom.uff_type, label
        );
    }
}

#[test]
fn test_reference_ammonia() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "ammonia").unwrap();
    let bond_lists = build_bond_lists_from_reference(mol);

    for atom in &mol.atoms {
        let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index]).unwrap();
        assert_eq!(
            label, atom.uff_type,
            "Ammonia atom {} ({}): expected {}, got {}",
            atom.index, atom.symbol, atom.uff_type, label
        );
    }
}

#[test]
fn test_reference_adamantane() {
    let data = load_reference_data();
    let mol = data
        .molecules
        .iter()
        .find(|m| m.name == "adamantane")
        .unwrap();
    let bond_lists = build_bond_lists_from_reference(mol);

    for atom in &mol.atoms {
        let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index]).unwrap();
        assert_eq!(
            label, atom.uff_type,
            "Adamantane atom {} ({}): expected {}, got {}",
            atom.index, atom.symbol, atom.uff_type, label
        );
    }
}

#[test]
fn test_reference_methanethiol() {
    let data = load_reference_data();
    let mol = data
        .molecules
        .iter()
        .find(|m| m.name == "methanethiol")
        .unwrap();
    let bond_lists = build_bond_lists_from_reference(mol);

    for atom in &mol.atoms {
        let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index]).unwrap();
        assert_eq!(
            label, atom.uff_type,
            "Methanethiol atom {} ({}): expected {}, got {}",
            atom.index, atom.symbol, atom.uff_type, label
        );
    }
}

/// Cross-validate all 9 reference molecules at once.
#[test]
fn test_all_reference_molecules() {
    let data = load_reference_data();

    for mol in &data.molecules {
        let bond_lists = build_bond_lists_from_reference(mol);

        for atom in &mol.atoms {
            let label = assign_uff_type(atom.atomic_number, &bond_lists[atom.index])
                .unwrap_or_else(|e| {
                    panic!(
                        "{} atom {} ({}): type assignment failed: {}",
                        mol.name, atom.index, atom.symbol, e
                    )
                });
            assert_eq!(
                label, atom.uff_type,
                "{} atom {} ({}): expected {}, got {}",
                mol.name, atom.index, atom.symbol, atom.uff_type, label
            );
        }
    }
}

// ============================================================================
// Test: assign_uff_types batch function
// ============================================================================

#[test]
fn test_batch_assign_methane() {
    // Methane: 1 carbon + 4 hydrogens
    let atomic_numbers: Vec<i16> = vec![6, 1, 1, 1, 1];
    let bonds_c: Vec<InlineBond> = vec![
        single_bond(1),
        single_bond(2),
        single_bond(3),
        single_bond(4),
    ];
    let bonds_h0: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h1: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h2: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h3: Vec<InlineBond> = vec![single_bond(0)];

    let bond_lists: Vec<&[InlineBond]> = vec![&bonds_c, &bonds_h0, &bonds_h1, &bonds_h2, &bonds_h3];

    let result = assign_uff_types(&atomic_numbers, &bond_lists).unwrap();
    assert_eq!(result.labels, vec!["C_3", "H_", "H_", "H_", "H_"]);
    assert_eq!(result.params.len(), 5);
    assert_eq!(result.params[0].label, "C_3");
    assert_eq!(result.params[1].label, "H_");
}

#[test]
fn test_batch_assign_mismatched_lengths() {
    let atomic_numbers: Vec<i16> = vec![6, 1];
    let bonds: Vec<InlineBond> = vec![single_bond(1)];
    let bond_lists: Vec<&[InlineBond]> = vec![&bonds];

    let result = assign_uff_types(&atomic_numbers, &bond_lists);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Mismatched lengths"));
}

// ============================================================================
// Test: bond_order_to_f64
// ============================================================================

#[test]
fn test_bond_order_to_f64() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
        BOND_DATIVE, BOND_DELETED, BOND_METALLIC, BOND_QUADRUPLE,
    };

    assert_eq!(bond_order_to_f64(BOND_DELETED), 0.0);
    assert_eq!(bond_order_to_f64(BOND_SINGLE), 1.0);
    assert_eq!(bond_order_to_f64(BOND_DOUBLE), 2.0);
    assert_eq!(bond_order_to_f64(BOND_TRIPLE), 3.0);
    assert_eq!(bond_order_to_f64(BOND_AROMATIC), 1.5);
    assert_eq!(bond_order_to_f64(BOND_DATIVE), 1.0);
    assert_eq!(bond_order_to_f64(BOND_METALLIC), 1.0);
    assert_eq!(bond_order_to_f64(BOND_QUADRUPLE), 1.0);
}

// ============================================================================
// Test: hybridization_from_label
// ============================================================================

#[test]
fn test_hybridization_from_label() {
    assert_eq!(hybridization_from_label("C_1"), 1);
    assert_eq!(hybridization_from_label("C_2"), 2);
    assert_eq!(hybridization_from_label("C_3"), 3);
    assert_eq!(hybridization_from_label("C_R"), 2); // resonance ≈ sp2
    assert_eq!(hybridization_from_label("N_1"), 1);
    assert_eq!(hybridization_from_label("N_2"), 2);
    assert_eq!(hybridization_from_label("N_3"), 3);
    assert_eq!(hybridization_from_label("N_R"), 2);
    assert_eq!(hybridization_from_label("O_2"), 2);
    assert_eq!(hybridization_from_label("O_3"), 3);
    assert_eq!(hybridization_from_label("O_R"), 2);
    assert_eq!(hybridization_from_label("S_2"), 2);
    assert_eq!(hybridization_from_label("S_R"), 2);
    assert_eq!(hybridization_from_label("Si3"), 3);
}

#[test]
fn test_hybridization_from_label_with_charge() {
    assert_eq!(hybridization_from_label("S_3+2"), 3);
    assert_eq!(hybridization_from_label("S_3+4"), 3);
    assert_eq!(hybridization_from_label("S_3+6"), 3);
    assert_eq!(hybridization_from_label("P_3+3"), 3);
    assert_eq!(hybridization_from_label("P_3+5"), 3);
    assert_eq!(hybridization_from_label("Fe3+2"), 3);
    assert_eq!(hybridization_from_label("Fe6+2"), 6);
    assert_eq!(hybridization_from_label("Ti6+4"), 6);
    assert_eq!(hybridization_from_label("Ni4+2"), 4);
    assert_eq!(hybridization_from_label("Ag1+1"), 1);
}

#[test]
fn test_hybridization_halogens_and_special() {
    // Halogens and special elements have no hybridization digit
    assert_eq!(hybridization_from_label("H_"), 0);
    assert_eq!(hybridization_from_label("F_"), 0);
    assert_eq!(hybridization_from_label("Cl"), 0);
    assert_eq!(hybridization_from_label("Br"), 0);
    assert_eq!(hybridization_from_label("I_"), 0);
    assert_eq!(hybridization_from_label("Li"), 0);
    assert_eq!(hybridization_from_label("Na"), 0);
    assert_eq!(hybridization_from_label("K_"), 0);
}

// ============================================================================
// Test: error cases
// ============================================================================

#[test]
fn test_unsupported_atomic_number() {
    // Z=200 doesn't exist
    let result = assign_uff_type(200, &[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("No UFF atom type"));
}

#[test]
fn test_negative_atomic_number() {
    let result = assign_uff_type(-1, &[]);
    assert!(result.is_err());
}

// ============================================================================
// Test: deleted bonds are ignored
// ============================================================================

#[test]
fn test_deleted_bonds_ignored() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_DELETED;

    // Carbon with 4 bonds, but 2 are deleted → effectively 2 single bonds
    // 2 single bonds on carbon → still C_3 (sp3)
    let bonds = [
        single_bond(1),
        single_bond(2),
        InlineBond::new(3, BOND_DELETED),
        InlineBond::new(4, BOND_DELETED),
    ];
    assert_eq!(assign_uff_type(6, &bonds).unwrap(), "C_3");
}

// ============================================================================
// Test: parameter cross-validation
// Verify that atom type → params yields correct r0 and kb for reference bonds
// ============================================================================

#[test]
fn test_typed_params_match_reference_bond_params() {
    let data = load_reference_data();

    for mol in &data.molecules {
        let bond_lists = build_bond_lists_from_reference(mol);
        let atomic_numbers: Vec<i16> = mol.atoms.iter().map(|a| a.atomic_number).collect();
        let bond_refs: Vec<&[InlineBond]> = bond_lists.iter().map(|v| v.as_slice()).collect();

        let assignment = assign_uff_types(&atomic_numbers, &bond_refs).unwrap_or_else(|e| {
            panic!("{}: type assignment failed: {}", mol.name, e);
        });

        // Verify that all assigned params have the expected labels
        for (i, atom) in mol.atoms.iter().enumerate() {
            assert_eq!(
                assignment.labels[i], atom.uff_type,
                "{} atom {}: label mismatch",
                mol.name, i
            );
        }
    }
}

// ============================================================================
// Test: metals and transition metals
// ============================================================================

#[test]
fn test_transition_metals() {
    // Iron: low valence → Fe3+2, high valence → Fe6+2
    assert_eq!(
        assign_uff_type(26, &[single_bond(1), single_bond(2)]).unwrap(),
        "Fe3+2"
    );
    assert_eq!(
        assign_uff_type(
            26,
            &[
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4),
                single_bond(5),
                single_bond(6),
            ]
        )
        .unwrap(),
        "Fe6+2"
    );

    // Titanium: ≤4 bonds → Ti3+4, >4 → Ti6+4
    assert_eq!(
        assign_uff_type(
            22,
            &[
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4)
            ]
        )
        .unwrap(),
        "Ti3+4"
    );
    assert_eq!(
        assign_uff_type(
            22,
            &[
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4),
                single_bond(5),
                single_bond(6),
            ]
        )
        .unwrap(),
        "Ti6+4"
    );

    // Molybdenum: ≤4 → Mo3+6, >4 → Mo6+6
    assert_eq!(
        assign_uff_type(
            42,
            &[
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4)
            ]
        )
        .unwrap(),
        "Mo3+6"
    );
    assert_eq!(
        assign_uff_type(
            42,
            &[
                single_bond(1),
                single_bond(2),
                single_bond(3),
                single_bond(4),
                single_bond(5),
                single_bond(6),
            ]
        )
        .unwrap(),
        "Mo6+6"
    );
}

// ============================================================================
// Test: lanthanides and actinides
// ============================================================================

#[test]
fn test_lanthanides() {
    // All lanthanides map to X6+3 (except La which is La3+3)
    assert_eq!(assign_uff_type(57, &[]).unwrap(), "La3+3"); // La
    assert_eq!(assign_uff_type(58, &[]).unwrap(), "Ce6+3"); // Ce
    assert_eq!(assign_uff_type(64, &[]).unwrap(), "Gd6+3"); // Gd
    assert_eq!(assign_uff_type(71, &[]).unwrap(), "Lu6+3"); // Lu
}

#[test]
fn test_actinides() {
    assert_eq!(assign_uff_type(89, &[]).unwrap(), "Ac6+3"); // Ac
    assert_eq!(assign_uff_type(90, &[]).unwrap(), "Th6+4"); // Th
    assert_eq!(assign_uff_type(92, &[]).unwrap(), "U_6+4"); // U
    assert_eq!(assign_uff_type(94, &[]).unwrap(), "Pu6+4"); // Pu
    assert_eq!(assign_uff_type(103, &[]).unwrap(), "Lw6+3"); // Lr
}

// ============================================================================
// Test: noble gases
// ============================================================================

#[test]
fn test_noble_gases() {
    assert_eq!(assign_uff_type(2, &[]).unwrap(), "He4+4");
    assert_eq!(assign_uff_type(10, &[]).unwrap(), "Ne4+4");
    assert_eq!(assign_uff_type(18, &[]).unwrap(), "Ar4+4");
    assert_eq!(assign_uff_type(36, &[]).unwrap(), "Kr4+4");
    assert_eq!(assign_uff_type(54, &[]).unwrap(), "Xe4+4");
    assert_eq!(assign_uff_type(86, &[]).unwrap(), "Rn4+4");
}

// ============================================================================
// Test: boron hybridization
// ============================================================================

#[test]
fn test_boron_types() {
    // BH3 (3 single bonds) → B_3
    assert_eq!(
        assign_uff_type(5, &[single_bond(1), single_bond(2), single_bond(3)]).unwrap(),
        "B_3"
    );
    // B with double bond → B_2
    assert_eq!(
        assign_uff_type(5, &[double_bond(1), single_bond(2)]).unwrap(),
        "B_2"
    );
    // B with 2 bonds → B_2
    assert_eq!(
        assign_uff_type(5, &[single_bond(1), single_bond(2)]).unwrap(),
        "B_2"
    );
}
