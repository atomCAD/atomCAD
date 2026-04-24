use glam::f64::DVec2;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::ivec3::IVec3Data;
use rust_lib_flutter_cad::structure_designer::nodes::supercell::SupercellData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn evaluate(designer: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn set_node_data<T: rust_lib_flutter_cad::structure_designer::node_data::NodeData + 'static>(
    designer: &mut StructureDesigner,
    node_id: u64,
    data: T,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut("test").unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = Box::new(data);
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        node,
        true,
    );
}

fn extract_structure(result: NetworkResult) -> Structure {
    match result {
        NetworkResult::Structure(s) => s,
        other => panic!(
            "Expected Structure result, got {}",
            other.to_display_string()
        ),
    }
}

/// Convenience: build a supercell node fed by a bare `structure` node (which
/// emits diamond defaults). Returns (supercell_node_id, structure_node_id).
fn build_diamond_plus_supercell(designer: &mut StructureDesigner) -> (u64, u64) {
    let structure_id = designer.add_node("structure", DVec2::ZERO);
    let supercell_id = designer.add_node("supercell", DVec2::new(200.0, 0.0));
    designer.connect_nodes(structure_id, 0, supercell_id, 0);
    (supercell_id, structure_id)
}

#[test]
fn default_supercell_node_passes_structure_through_unchanged() {
    let mut designer = setup_designer();
    let (supercell_id, _) = build_diamond_plus_supercell(&mut designer);

    let s = extract_structure(evaluate(&designer, supercell_id));
    let diamond = Structure::diamond();

    // Identity matrix ⇒ same number of sites / bonds as diamond.
    assert_eq!(s.motif.sites.len(), diamond.motif.sites.len());
    assert_eq!(s.motif.bonds.len(), diamond.motif.bonds.len());
    assert_eq!(
        s.lattice_vecs.cell_length_a,
        DIAMOND_UNIT_CELL_SIZE_ANGSTROM
    );
}

#[test]
fn set_text_properties_uses_stored_matrix() {
    let mut designer = setup_designer();
    let (supercell_id, _) = build_diamond_plus_supercell(&mut designer);

    // Set rows a=(2,0,0), b=(0,2,0), c=(0,0,2) via text properties → det=8.
    let mut props = HashMap::new();
    props.insert("a".to_string(), TextValue::IVec3(IVec3::new(2, 0, 0)));
    props.insert("b".to_string(), TextValue::IVec3(IVec3::new(0, 2, 0)));
    props.insert("c".to_string(), TextValue::IVec3(IVec3::new(0, 0, 2)));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test")
            .unwrap();
        let node = network.nodes.get_mut(&supercell_id).unwrap();
        node.data.set_text_properties(&props).unwrap();
    }
    designer.validate_active_network();

    let s = extract_structure(evaluate(&designer, supercell_id));
    let diamond = Structure::diamond();
    assert_eq!(s.motif.sites.len(), 8 * diamond.motif.sites.len());
    assert_eq!(s.motif.bonds.len(), 8 * diamond.motif.bonds.len());
    // Lattice grew 2× along each axis.
    assert!(
        (s.lattice_vecs.cell_length_a - 2.0 * DIAMOND_UNIT_CELL_SIZE_ANGSTROM).abs() < 1e-9,
        "lattice a = {}",
        s.lattice_vecs.cell_length_a
    );
}

#[test]
fn diagonal_pin_overrides_stored_matrix() {
    let mut designer = setup_designer();
    let (supercell_id, _) = build_diamond_plus_supercell(&mut designer);

    // Stored matrix: 3×3×3 (if it were used, det=27 and 8·27=216 sites).
    set_node_data(
        &mut designer,
        supercell_id,
        SupercellData {
            matrix: [[3, 0, 0], [0, 3, 0], [0, 0, 3]],
        },
    );

    // Diagonal pin carrying (2, 2, 2) → effective matrix diag(2,2,2), det=8.
    let diag_id = designer.add_node("ivec3", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        diag_id,
        IVec3Data {
            value: IVec3::new(2, 2, 2),
        },
    );
    designer.connect_nodes(diag_id, 0, supercell_id, 1);
    designer.validate_active_network();

    let s = extract_structure(evaluate(&designer, supercell_id));
    let diamond = Structure::diamond();
    // Must match the diagonal override (8×), NOT the stored matrix (27×).
    assert_eq!(s.motif.sites.len(), 8 * diamond.motif.sites.len());
    assert!((s.lattice_vecs.cell_length_a - 2.0 * DIAMOND_UNIT_CELL_SIZE_ANGSTROM).abs() < 1e-9);
}

#[test]
fn singular_matrix_surfaces_as_error() {
    let mut designer = setup_designer();
    let (supercell_id, _) = build_diamond_plus_supercell(&mut designer);

    // All-zero row → degenerate.
    set_node_data(
        &mut designer,
        supercell_id,
        SupercellData {
            matrix: [[1, 0, 0], [0, 0, 0], [0, 0, 1]],
        },
    );
    designer.validate_active_network();

    let result = evaluate(&designer, supercell_id);
    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains("supercell"),
                "error should mention supercell: {}",
                msg
            );
            assert!(
                msg.to_lowercase().contains("degenerate")
                    || msg.to_lowercase().contains("linearly dependent"),
                "error should describe degeneracy: {}",
                msg
            );
        }
        other => panic!(
            "expected Error result for singular matrix, got {}",
            other.to_display_string()
        ),
    }
}

#[test]
fn negative_determinant_surfaces_as_error() {
    let mut designer = setup_designer();
    let (supercell_id, _) = build_diamond_plus_supercell(&mut designer);

    // Swap two rows → det = -1 → left-handed.
    set_node_data(
        &mut designer,
        supercell_id,
        SupercellData {
            matrix: [[0, 1, 0], [1, 0, 0], [0, 0, 1]],
        },
    );
    designer.validate_active_network();

    let result = evaluate(&designer, supercell_id);
    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("left-handed")
                    || msg.to_lowercase().contains("negative"),
                "error should describe handedness: {}",
                msg
            );
        }
        other => panic!(
            "expected Error result for negative determinant, got {}",
            other.to_display_string()
        ),
    }
}

#[test]
fn unwired_structure_input_defaults_to_diamond() {
    let mut designer = setup_designer();
    // Supercell with no structure input wired — should default to diamond
    // (identity matrix ⇒ pass-through diamond structure).
    let supercell_id = designer.add_node("supercell", DVec2::ZERO);
    designer.validate_active_network();

    let s = extract_structure(evaluate(&designer, supercell_id));
    let diamond = Structure::diamond();
    assert_eq!(s.motif.sites.len(), diamond.motif.sites.len());
    assert_eq!(s.motif.bonds.len(), diamond.motif.bonds.len());
    assert_eq!(
        s.lattice_vecs.cell_length_a,
        DIAMOND_UNIT_CELL_SIZE_ANGSTROM
    );
}

#[test]
fn subtitle_shows_determinant_when_diagonal_unconnected() {
    let data = SupercellData {
        matrix: [[2, 0, 0], [0, 2, 0], [0, 0, 2]],
    };
    let connected: std::collections::HashSet<String> = std::collections::HashSet::new();
    let subtitle = rust_lib_flutter_cad::structure_designer::node_data::NodeData::get_subtitle(
        &data, &connected,
    );
    assert_eq!(subtitle.as_deref(), Some("det = 8"));

    let singular = SupercellData {
        matrix: [[1, 0, 0], [0, 0, 0], [0, 0, 1]],
    };
    let sub2 = rust_lib_flutter_cad::structure_designer::node_data::NodeData::get_subtitle(
        &singular, &connected,
    );
    assert!(sub2.as_deref().unwrap().contains("singular"));

    let handed = SupercellData {
        matrix: [[0, 1, 0], [1, 0, 0], [0, 0, 1]],
    };
    let sub3 = rust_lib_flutter_cad::structure_designer::node_data::NodeData::get_subtitle(
        &handed, &connected,
    );
    assert!(sub3.as_deref().unwrap().contains("left-handed"));
}

#[test]
fn subtitle_hides_determinant_when_diagonal_connected() {
    let data = SupercellData {
        matrix: [[2, 0, 0], [0, 2, 0], [0, 0, 2]],
    };
    let mut connected = std::collections::HashSet::new();
    connected.insert("diagonal".to_string());
    let subtitle = rust_lib_flutter_cad::structure_designer::node_data::NodeData::get_subtitle(
        &data, &connected,
    );
    // When diagonal is connected, the stored matrix is overridden, so no
    // concrete determinant should be shown.
    assert_eq!(subtitle.as_deref(), Some("det = ?"));
}

#[test]
fn text_properties_roundtrip_preserves_matrix() {
    let original = SupercellData {
        matrix: [[2, 1, 0], [0, 2, 1], [1, 0, 2]],
    };
    let props: HashMap<String, TextValue> =
        rust_lib_flutter_cad::structure_designer::node_data::NodeData::get_text_properties(
            &original,
        )
        .into_iter()
        .collect();

    let mut restored = SupercellData {
        matrix: [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
    };
    rust_lib_flutter_cad::structure_designer::node_data::NodeData::set_text_properties(
        &mut restored,
        &props,
    )
    .unwrap();

    assert_eq!(restored.matrix, original.matrix);
}
