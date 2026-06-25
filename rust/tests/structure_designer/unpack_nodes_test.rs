use glam::f64::DVec2;
use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Evaluate a specific output pin of a node in the named network.
fn evaluate_pin(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    output_pin_index: i32,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(
        &network_stack,
        node_id,
        output_pin_index,
        registry,
        false,
        &mut context,
    )
}

fn set_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

fn expect_vec3(result: NetworkResult, expected: DVec3) {
    match result {
        NetworkResult::Vec3(v) => {
            assert!(
                (v - expected).length() < 1e-9,
                "expected Vec3({expected:?}), got Vec3({v:?})"
            );
        }
        other => panic!("expected Vec3, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Registration
// ============================================================================

#[test]
fn test_lattice_vecs_unpack_registered() {
    let registry = NodeTypeRegistry::new();
    let nt = registry
        .get_node_type("lattice_vecs_unpack")
        .expect("lattice_vecs_unpack should be registered");
    assert_eq!(nt.name, "lattice_vecs_unpack");
    assert!(nt.public);
    assert_eq!(nt.parameters.len(), 1);
    assert_eq!(nt.parameters[0].name, "lattice_vecs");
    assert_eq!(nt.parameters[0].data_type, DataType::LatticeVecs);
    assert_eq!(nt.output_pins.len(), 3);
    assert_eq!(nt.output_pins[0].name, "a");
    assert_eq!(nt.output_pins[1].name, "b");
    assert_eq!(nt.output_pins[2].name, "c");
    for pin in &nt.output_pins {
        assert_eq!(pin.fixed_type(), Some(&DataType::Vec3));
    }
}

// ============================================================================
// Evaluation: diamond default
// ============================================================================

#[test]
fn test_lattice_vecs_unpack_diamond_default() {
    let mut designer = setup_designer_with_network("test");

    // lattice_vecs node with default (diamond) data → cubic cell.
    let lv_id = designer.add_node("lattice_vecs", DVec2::new(0.0, 0.0));
    let unpack_id = designer.add_node("lattice_vecs_unpack", DVec2::new(200.0, 0.0));
    designer.validate_active_network();
    designer.connect_nodes(lv_id, 0, unpack_id, 0);

    let s = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
    expect_vec3(
        evaluate_pin(&designer, "test", unpack_id, 0),
        DVec3::new(s, 0.0, 0.0),
    );
    expect_vec3(
        evaluate_pin(&designer, "test", unpack_id, 1),
        DVec3::new(0.0, s, 0.0),
    );
    expect_vec3(
        evaluate_pin(&designer, "test", unpack_id, 2),
        DVec3::new(0.0, 0.0, s),
    );
}

// ============================================================================
// Evaluation: non-orthogonal cell (basis vectors overridden)
// ============================================================================

#[test]
fn test_lattice_vecs_unpack_non_orthogonal_passthrough() {
    let mut designer = setup_designer_with_network("test");

    let a = DVec3::new(4.0, 0.0, 0.0);
    let b = DVec3::new(1.5, 3.5, 0.0);
    let c = DVec3::new(0.5, 0.7, 5.0);

    let va = designer.add_node("vec3", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", va, Box::new(Vec3Data { value: a }));
    let vb = designer.add_node("vec3", DVec2::new(0.0, 100.0));
    set_node_data(&mut designer, "test", vb, Box::new(Vec3Data { value: b }));
    let vc = designer.add_node("vec3", DVec2::new(0.0, 200.0));
    set_node_data(&mut designer, "test", vc, Box::new(Vec3Data { value: c }));

    let lv_id = designer.add_node("lattice_vecs", DVec2::new(200.0, 0.0));
    let unpack_id = designer.add_node("lattice_vecs_unpack", DVec2::new(400.0, 0.0));
    designer.validate_active_network();

    designer.connect_nodes(va, 0, lv_id, 0);
    designer.connect_nodes(vb, 0, lv_id, 1);
    designer.connect_nodes(vc, 0, lv_id, 2);
    designer.connect_nodes(lv_id, 0, unpack_id, 0);

    expect_vec3(evaluate_pin(&designer, "test", unpack_id, 0), a);
    expect_vec3(evaluate_pin(&designer, "test", unpack_id, 1), b);
    expect_vec3(evaluate_pin(&designer, "test", unpack_id, 2), c);
}

// ============================================================================
// Evaluation: no input wired → all pins None
// ============================================================================

#[test]
fn test_lattice_vecs_unpack_no_input_yields_none() {
    let mut designer = setup_designer_with_network("test");

    let unpack_id = designer.add_node("lattice_vecs_unpack", DVec2::new(0.0, 0.0));
    designer.validate_active_network();

    for pin in 0..3 {
        match evaluate_pin(&designer, "test", unpack_id, pin) {
            NetworkResult::None => {}
            other => panic!(
                "pin {pin}: expected None, got {:?}",
                other.to_display_string()
            ),
        }
    }
}
