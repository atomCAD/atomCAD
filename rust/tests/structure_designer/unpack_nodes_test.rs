use glam::f64::DVec2;
use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
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

fn expect_float(result: NetworkResult, expected: f64) {
    match result {
        NetworkResult::Float(v) => {
            assert!(
                (v - expected).abs() < 1e-9,
                "expected Float({expected}), got Float({v})"
            );
        }
        other => panic!("expected Float, got {:?}", other.to_display_string()),
    }
}

/// Build a `lattice_vecs` node whose basis vectors are `a`/`b`/`c` (wired through
/// three `vec3` nodes), returning the designer and the `lattice_vecs` node id. The
/// node's eval recomputes the crystallographic parameters from these vectors via
/// `UnitCellStruct::new`, so this is the canonical way to feed a custom cell.
fn lattice_vecs_from_basis(designer: &mut StructureDesigner, a: DVec3, b: DVec3, c: DVec3) -> u64 {
    let va = designer.add_node("vec3", DVec2::new(0.0, 0.0));
    set_node_data(designer, "test", va, Box::new(Vec3Data { value: a }));
    let vb = designer.add_node("vec3", DVec2::new(0.0, 100.0));
    set_node_data(designer, "test", vb, Box::new(Vec3Data { value: b }));
    let vc = designer.add_node("vec3", DVec2::new(0.0, 200.0));
    set_node_data(designer, "test", vc, Box::new(Vec3Data { value: c }));

    let lv_id = designer.add_node("lattice_vecs", DVec2::new(200.0, 0.0));
    designer.validate_active_network();
    designer.connect_nodes(va, 0, lv_id, 0);
    designer.connect_nodes(vb, 0, lv_id, 1);
    designer.connect_nodes(vc, 0, lv_id, 2);
    lv_id
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

// ============================================================================
// lattice_vecs_params: registration
// ============================================================================

#[test]
fn test_lattice_vecs_params_registered() {
    let registry = NodeTypeRegistry::new();
    let nt = registry
        .get_node_type("lattice_vecs_params")
        .expect("lattice_vecs_params should be registered");
    assert_eq!(nt.name, "lattice_vecs_params");
    assert!(nt.public);
    assert_eq!(nt.parameters.len(), 1);
    assert_eq!(nt.parameters[0].name, "lattice_vecs");
    assert_eq!(nt.parameters[0].data_type, DataType::LatticeVecs);

    let expected: [(&str, DataType); 8] = [
        ("a", DataType::Float),
        ("b", DataType::Float),
        ("c", DataType::Float),
        ("alpha", DataType::Float),
        ("beta", DataType::Float),
        ("gamma", DataType::Float),
        ("lengths", DataType::Vec3),
        ("angles", DataType::Vec3),
    ];
    assert_eq!(nt.output_pins.len(), expected.len());
    for (pin, (name, ty)) in nt.output_pins.iter().zip(expected.iter()) {
        assert_eq!(&pin.name, name);
        assert_eq!(pin.fixed_type(), Some(ty));
    }
}

// ============================================================================
// lattice_vecs_params: diamond default
// ============================================================================

#[test]
fn test_lattice_vecs_params_diamond_default() {
    let mut designer = setup_designer_with_network("test");

    let lv_id = designer.add_node("lattice_vecs", DVec2::new(0.0, 0.0));
    let params_id = designer.add_node("lattice_vecs_params", DVec2::new(200.0, 0.0));
    designer.validate_active_network();
    designer.connect_nodes(lv_id, 0, params_id, 0);

    let s = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

    // Lengths a/b/c on pins 0/1/2.
    expect_float(evaluate_pin(&designer, "test", params_id, 0), s);
    expect_float(evaluate_pin(&designer, "test", params_id, 1), s);
    expect_float(evaluate_pin(&designer, "test", params_id, 2), s);

    // Angles alpha/beta/gamma on pins 3/4/5 — all 90 degrees for a cubic cell.
    expect_float(evaluate_pin(&designer, "test", params_id, 3), 90.0);
    expect_float(evaluate_pin(&designer, "test", params_id, 4), 90.0);
    expect_float(evaluate_pin(&designer, "test", params_id, 5), 90.0);

    // Packed Vec3s on pins 6 (lengths) and 7 (angles).
    expect_vec3(
        evaluate_pin(&designer, "test", params_id, 6),
        DVec3::new(s, s, s),
    );
    expect_vec3(
        evaluate_pin(&designer, "test", params_id, 7),
        DVec3::new(90.0, 90.0, 90.0),
    );
}

// ============================================================================
// lattice_vecs_params: triclinic cell — verifies angle mapping + degrees
// ============================================================================

#[test]
fn test_lattice_vecs_params_triclinic_angle_mapping() {
    let mut designer = setup_designer_with_network("test");

    // Distinct lengths and distinct non-90 angles so each pin is uniquely
    // identifiable. from_parameters uses the convention alpha=b∠c, beta=a∠c,
    // gamma=a∠b; lattice_vecs (via UnitCellStruct::new) round-trips it back.
    let len_a = 4.0;
    let len_b = 5.0;
    let len_c = 6.0;
    let alpha = 70.0;
    let beta = 80.0;
    let gamma = 100.0;
    let uc = UnitCellStruct::from_parameters(len_a, len_b, len_c, alpha, beta, gamma);

    let lv_id = lattice_vecs_from_basis(&mut designer, uc.a, uc.b, uc.c);
    let params_id = designer.add_node("lattice_vecs_params", DVec2::new(400.0, 0.0));
    designer.validate_active_network();
    designer.connect_nodes(lv_id, 0, params_id, 0);

    // Lengths.
    expect_float(evaluate_pin(&designer, "test", params_id, 0), len_a);
    expect_float(evaluate_pin(&designer, "test", params_id, 1), len_b);
    expect_float(evaluate_pin(&designer, "test", params_id, 2), len_c);

    // Angles (degrees), in alpha/beta/gamma order.
    expect_float(evaluate_pin(&designer, "test", params_id, 3), alpha);
    expect_float(evaluate_pin(&designer, "test", params_id, 4), beta);
    expect_float(evaluate_pin(&designer, "test", params_id, 5), gamma);

    // Packed Vec3s.
    expect_vec3(
        evaluate_pin(&designer, "test", params_id, 6),
        DVec3::new(len_a, len_b, len_c),
    );
    expect_vec3(
        evaluate_pin(&designer, "test", params_id, 7),
        DVec3::new(alpha, beta, gamma),
    );
}

// ============================================================================
// lattice_vecs_params: no input wired → all 8 pins None
// ============================================================================

#[test]
fn test_lattice_vecs_params_no_input_yields_none() {
    let mut designer = setup_designer_with_network("test");

    let params_id = designer.add_node("lattice_vecs_params", DVec2::new(0.0, 0.0));
    designer.validate_active_network();

    for pin in 0..8 {
        match evaluate_pin(&designer, "test", params_id, pin) {
            NetworkResult::None => {}
            other => panic!(
                "pin {pin}: expected None, got {:?}",
                other.to_display_string()
            ),
        }
    }
}
