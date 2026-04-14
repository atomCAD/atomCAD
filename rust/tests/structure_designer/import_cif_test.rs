use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::nodes::import_cif::ImportCifData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

fn fixture_path(name: &str) -> String {
    format!("{}/tests/fixtures/cif/{}", env!("CARGO_MANIFEST_DIR"), name)
}

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn evaluate_pin(designer: &StructureDesigner, node_id: u64, pin_index: i32) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&stack, node_id, pin_index, registry, false, &mut context)
}

fn add_import_cif_node(designer: &mut StructureDesigner, file_path: &str) -> u64 {
    let node_id = designer.add_node("import_cif", DVec2::new(0.0, 0.0));

    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    if let Some(data) = node.data.as_any_mut().downcast_mut::<ImportCifData>() {
        data.file_name = Some(file_path.to_string());
    }

    node_id
}

fn add_import_cif_node_with_params(
    designer: &mut StructureDesigner,
    file_path: &str,
    use_cif_bonds: bool,
    infer_bonds: bool,
    bond_tolerance: f64,
) -> u64 {
    let node_id = designer.add_node("import_cif", DVec2::new(0.0, 0.0));

    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    if let Some(data) = node.data.as_any_mut().downcast_mut::<ImportCifData>() {
        data.file_name = Some(file_path.to_string());
        data.use_cif_bonds = use_cif_bonds;
        data.infer_bonds = infer_bonds;
        data.bond_tolerance = bond_tolerance;
    }

    node_id
}

// ============================================================================
// Pin 0: UnitCell output tests
// ============================================================================

#[test]
fn import_cif_diamond_unit_cell_output() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node(&mut designer, &fixture_path("diamond.cif"));

    let result = evaluate_pin(&designer, node_id, 0);
    match result {
        NetworkResult::LatticeVecs(uc) => {
            let a = uc.a.length();
            let b = uc.b.length();
            let c = uc.c.length();
            assert!((a - 3.56679).abs() < 0.01, "a = {}", a);
            assert!((b - 3.56679).abs() < 0.01, "b = {}", b);
            assert!((c - 3.56679).abs() < 0.01, "c = {}", c);
        }
        NetworkResult::Error(e) => panic!("Expected UnitCell, got Error: {}", e),
        _ => panic!("Expected UnitCell result on pin 0"),
    }
}

#[test]
fn import_cif_nacl_unit_cell_output() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node(&mut designer, &fixture_path("nacl.cif"));

    let result = evaluate_pin(&designer, node_id, 0);
    match result {
        NetworkResult::LatticeVecs(uc) => {
            let a = uc.a.length();
            assert!((a - 5.62).abs() < 0.01, "NaCl a = {}", a);
        }
        NetworkResult::Error(e) => panic!("Expected UnitCell, got Error: {}", e),
        _ => panic!("Expected UnitCell result on pin 0"),
    }
}

#[test]
fn import_cif_hexagonal_unit_cell_output() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node(&mut designer, &fixture_path("hexagonal.cif"));

    let result = evaluate_pin(&designer, node_id, 0);
    match result {
        NetworkResult::LatticeVecs(uc) => {
            let a = uc.a.length();
            let b = uc.b.length();
            assert!(
                (a - b).abs() < 0.01,
                "Hexagonal should have a == b: a={}, b={}",
                a,
                b
            );
        }
        NetworkResult::Error(e) => panic!("Expected UnitCell, got Error: {}", e),
        _ => panic!("Expected UnitCell result on pin 0"),
    }
}

// ============================================================================
// Pin 1: Atomic output tests
// ============================================================================

#[test]
fn import_cif_diamond_atomic_output() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node(&mut designer, &fixture_path("diamond.cif"));

    let result = evaluate_pin(&designer, node_id, 1);
    if let Some(structure) = result.clone().extract_atomic() {
            assert_eq!(
                structure.get_num_of_atoms(),
                8,
                "Diamond should have 8 atoms"
            );
            // All should be carbon (Z=6)
            for atom in structure.atoms_values() {
                assert_eq!(atom.atomic_number, 6, "All atoms should be carbon");
            }
        } else if let NetworkResult::Error(e) = &result { panic!("Expected Atomic, got Error: {}", e); } else { panic!("Expected Atomic result on pin 1"); }
}

#[test]
fn import_cif_diamond_atomic_has_bonds() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node(&mut designer, &fixture_path("diamond.cif"));

    let result = evaluate_pin(&designer, node_id, 1);
    if let Some(structure) = result.clone().extract_atomic() {
            let total_bonds: usize = structure.atoms_values().map(|a| a.bonds.len()).sum();
            assert!(
                total_bonds > 0,
                "Diamond with infer_bonds=true should have bonds"
            );
        } else if let NetworkResult::Error(e) = &result { panic!("Expected Atomic, got Error: {}", e); } else { panic!("Expected Atomic result on pin 1"); }
}

#[test]
fn import_cif_nacl_atomic_output() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node(&mut designer, &fixture_path("nacl.cif"));

    let result = evaluate_pin(&designer, node_id, 1);
    if let Some(structure) = result.clone().extract_atomic() {
            assert_eq!(
                structure.get_num_of_atoms(),
                8,
                "NaCl should have 8 atoms in conventional cell"
            );
            let na_count = structure
                .atoms_values()
                .filter(|a| a.atomic_number == 11)
                .count();
            let cl_count = structure
                .atoms_values()
                .filter(|a| a.atomic_number == 17)
                .count();
            assert_eq!(na_count, 4, "NaCl: 4 Na atoms");
            assert_eq!(cl_count, 4, "NaCl: 4 Cl atoms");
        } else if let NetworkResult::Error(e) = &result { panic!("Expected Atomic, got Error: {}", e); } else { panic!("Expected Atomic result on pin 1"); }
}

// ============================================================================
// Pin 2: Motif output tests
// ============================================================================

#[test]
fn import_cif_diamond_motif_output() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node(&mut designer, &fixture_path("diamond.cif"));

    let result = evaluate_pin(&designer, node_id, 2);
    match result {
        NetworkResult::Motif(motif) => {
            assert_eq!(motif.sites.len(), 8, "Diamond motif should have 8 sites");
            for site in &motif.sites {
                assert_eq!(site.atomic_number, 6, "All motif sites should be carbon");
            }
            assert_eq!(
                motif.bonds.len(),
                16,
                "Diamond motif should have 16 tetrahedral bonds, got {}",
                motif.bonds.len()
            );
        }
        NetworkResult::Error(e) => panic!("Expected Motif, got Error: {}", e),
        _ => panic!("Expected Motif result on pin 2"),
    }
}

#[test]
fn import_cif_diamond_motif_no_bonds() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node_with_params(
        &mut designer,
        &fixture_path("diamond.cif"),
        false,
        false,
        1.15,
    );

    let result = evaluate_pin(&designer, node_id, 2);
    match result {
        NetworkResult::Motif(motif) => {
            assert_eq!(motif.sites.len(), 8, "Diamond motif should have 8 sites");
            assert_eq!(
                motif.bonds.len(),
                0,
                "With both bond options disabled, should have 0 bonds"
            );
        }
        NetworkResult::Error(e) => panic!("Expected Motif, got Error: {}", e),
        _ => panic!("Expected Motif result on pin 2"),
    }
}

// ============================================================================
// Bond tolerance parameter tests
// ============================================================================

#[test]
fn import_cif_diamond_very_low_tolerance_no_bonds() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node_with_params(
        &mut designer,
        &fixture_path("diamond.cif"),
        false,
        true,
        0.1,
    );

    let result = evaluate_pin(&designer, node_id, 2);
    match result {
        NetworkResult::Motif(motif) => {
            assert_eq!(
                motif.bonds.len(),
                0,
                "Very low tolerance should yield no bonds, got {}",
                motif.bonds.len()
            );
        }
        NetworkResult::Error(e) => panic!("Expected Motif, got Error: {}", e),
        _ => panic!("Expected Motif result on pin 2"),
    }
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn import_cif_no_file_specified_error() {
    let mut designer = setup_designer();
    let _node_id = designer.add_node("import_cif", DVec2::new(0.0, 0.0));

    let result = evaluate_pin(&designer, _node_id, 0);
    assert!(
        matches!(result, NetworkResult::Error(_)),
        "Should error when no file specified"
    );
}

#[test]
fn import_cif_nonexistent_file_error() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node(&mut designer, "/nonexistent/path/file.cif");

    let result = evaluate_pin(&designer, node_id, 0);
    assert!(
        matches!(result, NetworkResult::Error(_)),
        "Should error for nonexistent file"
    );
}

// ============================================================================
// Multi-block selection test
// ============================================================================

#[test]
fn import_cif_multi_block_first_block_default() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node(&mut designer, &fixture_path("multi_block.cif"));

    let result = evaluate_pin(&designer, node_id, 0);
    assert!(
        matches!(result, NetworkResult::LatticeVecs(_)),
        "First block should parse successfully"
    );
}

#[test]
fn import_cif_multi_block_select_by_name() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("import_cif", DVec2::new(0.0, 0.0));

    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    if let Some(data) = node.data.as_any_mut().downcast_mut::<ImportCifData>() {
        data.file_name = Some(fixture_path("multi_block.cif"));
        data.block_name = Some("nacl".to_string());
    }

    let result = evaluate_pin(&designer, node_id, 1);
    if let Some(structure) = result.clone().extract_atomic() {
            let has_na = structure.atoms_values().any(|a| a.atomic_number == 11);
            let has_cl = structure.atoms_values().any(|a| a.atomic_number == 17);
            assert!(
                has_na && has_cl,
                "NaCl block should contain Na and Cl atoms"
            );
        } else if let NetworkResult::Error(e) = &result { panic!("Expected Atomic, got Error: {}", e); } else { panic!("Expected Atomic result"); }
}

// ============================================================================
// Text properties roundtrip tests
// ============================================================================

fn props_to_hashmap(props: Vec<(String, TextValue)>) -> HashMap<String, TextValue> {
    props.into_iter().collect()
}

#[test]
fn import_cif_text_properties_roundtrip_defaults() {
    let original = ImportCifData {
        file_name: Some("test.cif".to_string()),
        block_name: None,
        use_cif_bonds: true,
        infer_bonds: true,
        bond_tolerance: 1.15,
        cached_result: None,
    };

    let props = original.get_text_properties();
    let mut restored = original.clone();
    let props_map = props_to_hashmap(props);
    restored.set_text_properties(&props_map).unwrap();

    assert_eq!(
        original.get_text_properties(),
        restored.get_text_properties(),
    );
}

#[test]
fn import_cif_text_properties_roundtrip_non_defaults() {
    let original = ImportCifData {
        file_name: Some("structures/nacl.cif".to_string()),
        block_name: Some("nacl_block".to_string()),
        use_cif_bonds: false,
        infer_bonds: false,
        bond_tolerance: 1.3,
        cached_result: None,
    };

    let props = original.get_text_properties();
    let mut restored = original.clone();
    let props_map = props_to_hashmap(props);
    restored.set_text_properties(&props_map).unwrap();

    assert_eq!(
        original.get_text_properties(),
        restored.get_text_properties(),
    );
}

// ============================================================================
// Serialization roundtrip tests
// ============================================================================

#[test]
fn import_cif_serde_roundtrip() {
    let original = ImportCifData {
        file_name: Some("test.cif".to_string()),
        block_name: Some("diamond".to_string()),
        use_cif_bonds: false,
        infer_bonds: true,
        bond_tolerance: 1.2,
        cached_result: None,
    };

    let json = serde_json::to_value(&original).unwrap();
    let restored: ImportCifData = serde_json::from_value(json).unwrap();

    assert_eq!(original.file_name, restored.file_name);
    assert_eq!(original.block_name, restored.block_name);
    assert_eq!(original.use_cif_bonds, restored.use_cif_bonds);
    assert_eq!(original.infer_bonds, restored.infer_bonds);
    assert!((original.bond_tolerance - restored.bond_tolerance).abs() < 1e-10);
    assert!(
        restored.cached_result.is_none(),
        "cached_result should not be serialized"
    );
}

#[test]
fn import_cif_serde_roundtrip_none_fields() {
    let original = ImportCifData::new();
    let json = serde_json::to_value(&original).unwrap();
    let restored: ImportCifData = serde_json::from_value(json).unwrap();

    assert_eq!(original.file_name, restored.file_name);
    assert_eq!(original.block_name, restored.block_name);
    assert_eq!(original.use_cif_bonds, restored.use_cif_bonds);
    assert_eq!(original.infer_bonds, restored.infer_bonds);
    assert!((original.bond_tolerance - restored.bond_tolerance).abs() < 1e-10);
}

// ============================================================================
// Node type registration test
// ============================================================================

#[test]
fn import_cif_node_registered() {
    let designer = StructureDesigner::new();
    let node_type = designer.node_type_registry.get_node_type("import_cif");
    assert!(node_type.is_some(), "import_cif should be registered");

    let nt = node_type.unwrap();
    assert_eq!(nt.parameters.len(), 5, "Should have 5 parameters");
    assert_eq!(nt.output_pins.len(), 3, "Should have 3 output pins");
    assert_eq!(nt.output_pins[0].name, "unit_cell");
    assert_eq!(nt.output_pins[1].name, "atoms");
    assert_eq!(nt.output_pins[2].name, "motif");
}

// ============================================================================
// With CIF bonds tests
// ============================================================================

#[test]
fn import_cif_with_bonds_file_uses_cif_bonds() {
    let mut designer = setup_designer();
    let node_id = add_import_cif_node_with_params(
        &mut designer,
        &fixture_path("with_bonds.cif"),
        true,
        false,
        1.15,
    );

    let result = evaluate_pin(&designer, node_id, 1);
    if let Some(structure) = result.clone().extract_atomic() {
            let total_bonds: usize = structure.atoms_values().map(|a| a.bonds.len()).sum();
            assert!(
                total_bonds > 0,
                "With use_cif_bonds=true on a CIF that has bonds, should have bonds"
            );
        } else if let NetworkResult::Error(e) = &result { panic!("Expected Atomic, got Error: {}", e); } else { panic!("Expected Atomic result on pin 1"); }
}

/// Regression test: CIF bonds for atoms with fractional coordinates outside [0,1)
/// must not produce long-distance motif bonds. The with_bonds.cif file has atoms
/// like O(1) at x=1.02958 and C(21) at x=-0.00758 — after symmetry expansion
/// these wrap into [0,1), and the motif bond relative_cell must account for this.
#[test]
fn import_cif_with_bonds_no_spurious_long_bonds() {
    use rust_lib_flutter_cad::crystolecule::io::cif::load_cif_extended;
    use rust_lib_flutter_cad::structure_designer::nodes::import_cif::build_cif_import_result;

    let cif_result = load_cif_extended(&fixture_path("with_bonds.cif"), None).unwrap();

    let count_long_bonds =
        |import: &rust_lib_flutter_cad::structure_designer::nodes::import_cif::CifImportResult| {
            import
                .motif
                .bonds
                .iter()
                .filter(|b| {
                    let s1 = &import.motif.sites[b.site_1.site_index];
                    let s2 = &import.motif.sites[b.site_2.site_index];
                    let pos1 = import.unit_cell.dvec3_lattice_to_real(&s1.position);
                    let f2 = glam::DVec3::new(
                        s2.position.x + b.site_2.relative_cell.x as f64,
                        s2.position.y + b.site_2.relative_cell.y as f64,
                        s2.position.z + b.site_2.relative_cell.z as f64,
                    );
                    let pos2 = import.unit_cell.dvec3_lattice_to_real(&f2);
                    glam::DVec3::distance(pos1, pos2) > 3.0
                })
                .count()
        };

    // CIF bonds path
    let import_cif = build_cif_import_result(&cif_result, true, false, 1.15).unwrap();
    assert_eq!(import_cif.motif.bonds.len(), 46);
    assert_eq!(
        count_long_bonds(&import_cif),
        0,
        "CIF bonds should have no long bonds"
    );

    // Inferred bonds path
    let import_inferred = build_cif_import_result(&cif_result, false, true, 1.15).unwrap();
    assert_eq!(
        count_long_bonds(&import_inferred),
        0,
        "Inferred bonds should have no long bonds"
    );
}

// ============================================================================
// Subtitle test
// ============================================================================

#[test]
fn import_cif_subtitle_shows_filename() {
    let data = ImportCifData {
        file_name: Some("diamond.cif".to_string()),
        block_name: None,
        use_cif_bonds: true,
        infer_bonds: true,
        bond_tolerance: 1.15,
        cached_result: None,
    };

    let connected = std::collections::HashSet::new();
    assert_eq!(
        data.get_subtitle(&connected),
        Some("diamond.cif".to_string())
    );

    let mut connected_with_file = std::collections::HashSet::new();
    connected_with_file.insert("file_name".to_string());
    assert_eq!(data.get_subtitle(&connected_with_file), None);
}
