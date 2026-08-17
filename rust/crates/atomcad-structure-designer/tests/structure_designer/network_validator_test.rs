use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::network_validator::validate_network;
use atomcad_structure_designer::nodes::parameter::ParameterData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

#[test]
fn test_validate_empty_network() {
    let designer = setup_designer_with_network("test_network");

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    assert!(network.valid, "Empty network should be valid");
    assert!(
        network.validation_errors.is_empty(),
        "Empty network should have no validation errors"
    );
}

#[test]
fn test_validate_single_node_network() {
    let mut designer = setup_designer_with_network("test_network");

    designer.add_node("float", DVec2::new(0.0, 0.0));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    assert!(network.valid, "Network with single node should be valid");
}

#[test]
fn test_validate_connected_nodes() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));

    designer.connect_nodes(float_id, 0, sphere_id, 0);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    assert!(
        network.valid,
        "Network with valid connections should be valid"
    );
}

#[test]
fn test_validate_network_with_return_node() {
    let mut designer = setup_designer_with_network("test_network");

    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    designer.set_return_node_id(Some(sphere_id));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    assert!(
        network.valid,
        "Network with valid return node should be valid"
    );
    assert_eq!(network.return_node_id, Some(sphere_id));
}

#[test]
fn test_add_and_validate_parameter_node() {
    let mut designer = setup_designer_with_network("test_network");

    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    assert_ne!(param_id, 0, "Parameter node should be created");

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    assert!(network.valid, "Network with parameter should be valid");
    assert_eq!(
        network.node_type.parameters.len(),
        1,
        "Should have one parameter"
    );
}

#[test]
fn test_validate_with_multiple_parameters() {
    let mut designer = setup_designer_with_network("test_network");

    let _param1_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    let _param2_id = designer.add_node("parameter", DVec2::new(0.0, 100.0));
    let _param3_id = designer.add_node("parameter", DVec2::new(0.0, 200.0));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    assert!(
        network.valid,
        "Network with multiple unique parameters should be valid"
    );
    assert_eq!(
        network.node_type.parameters.len(),
        3,
        "Should have three parameters"
    );
}

/// Phase 2 of `doc/design_error_management.md` (D4): `validate_wires` must
/// accumulate — a network with two *independent* type mismatches reports
/// both, each attributed to its own destination node. (Pre-Phase-2 the pass
/// short-circuited on the first error, and which mismatch "won" followed
/// HashMap iteration order.)
#[test]
fn two_independent_type_mismatches_are_both_reported() {
    let mut designer = setup_designer_with_network("Main");

    // Two disconnected array -> array_at chains; the default ArrayData is
    // Array[Int], matching array_at's default element type.
    let array_a = designer.add_node("array", DVec2::new(0.0, 0.0));
    let array_at_a = designer.add_node("array_at", DVec2::new(200.0, 0.0));
    designer.connect_nodes(array_a, 0, array_at_a, 0);
    let array_b = designer.add_node("array", DVec2::new(0.0, 200.0));
    let array_at_b = designer.add_node("array_at", DVec2::new(200.0, 200.0));
    designer.connect_nodes(array_b, 0, array_at_b, 0);

    // Retype both sources to Array[String] — each downstream wire's type
    // check now fails independently.
    designer
        .set_array_element_type(&[], array_a, DataType::String)
        .unwrap();
    designer
        .set_array_element_type(&[], array_b, DataType::String)
        .unwrap();

    let network = designer
        .node_type_registry
        .node_networks
        .get("Main")
        .unwrap();
    // Phase 2 asserted the mismatches still flipped `valid` (isolating
    // accumulation from the semantic change); Phase 3 (cone-scoped blocking)
    // then deliberately changed that: node-attributed blocking errors poison
    // their nodes, `valid` stays true.
    assert!(
        network.valid,
        "node-attributed blocking errors must not flip `valid`"
    );
    let mismatch_nodes: Vec<Option<u64>> = network
        .validation_errors
        .iter()
        .filter(|e| e.error_text.contains("Data type mismatch"))
        .map(|e| e.node_id)
        .collect();
    assert_eq!(
        mismatch_nodes.len(),
        2,
        "both independent mismatches must be reported; got errors: {:?}",
        network
            .validation_errors
            .iter()
            .map(|e| &e.error_text)
            .collect::<Vec<_>>()
    );
    assert!(
        mismatch_nodes.contains(&Some(array_at_a)) && mismatch_nodes.contains(&Some(array_at_b)),
        "each mismatch must be attributed to its own destination node \
         ({array_at_a}, {array_at_b}); got {:?}",
        mismatch_nodes
    );
}

/// Phase 2 (D4): `validate_parameters` accumulates where safe — a duplicate
/// parameter name and an abstract parameter type elsewhere are both reported
/// in one pass, and the interface rebuild is still skipped (network invalid).
#[test]
fn parameter_errors_accumulate_across_nodes() {
    let mut designer = setup_designer_with_network("Main");

    let p1 = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    let p2 = designer.add_node("parameter", DVec2::new(0.0, 100.0));
    let p3 = designer.add_node("parameter", DVec2::new(0.0, 200.0));

    // Mutate the parameter nodes directly (the interactive paths refuse
    // these states) and re-validate through the registry-removal dance.
    let mut network = designer
        .node_type_registry
        .node_networks
        .remove("Main")
        .unwrap();
    for (node_id, name, data_type) in [
        (p1, "dup", DataType::Int),
        (p2, "dup", DataType::Int),
        (p3, "abstract_param", DataType::HasAtoms),
    ] {
        let node = network.nodes.get_mut(&node_id).unwrap();
        let param_data = node
            .data
            .as_any_mut()
            .downcast_mut::<ParameterData>()
            .unwrap();
        param_data.param_name = name.to_string();
        param_data.data_type = data_type;
    }
    let result = validate_network(&mut network, &mut designer.node_type_registry, None);
    assert!(!result.valid);
    assert!(!network.valid);

    let errors: Vec<(Option<u64>, &str)> = network
        .validation_errors
        .iter()
        .map(|e| (e.node_id, e.error_text.as_str()))
        .collect();
    assert_eq!(
        errors.len(),
        2,
        "duplicate-name and abstract-type errors must both be reported; got {:?}",
        errors
    );
    // The first occurrence (lowest node id) keeps the name; the later
    // duplicate is flagged.
    assert!(
        errors
            .iter()
            .any(|(id, text)| *id == Some(p2) && text.contains("Duplicate parameter name 'dup'")),
        "missing duplicate-name error on node {p2}; got {:?}",
        errors
    );
    assert!(
        errors
            .iter()
            .any(|(id, text)| *id == Some(p3) && text.contains("abstract type")),
        "missing abstract-type error on node {p3}; got {:?}",
        errors
    );

    designer
        .node_type_registry
        .node_networks
        .insert("Main".to_string(), network);
}

#[test]
fn test_network_output_type_from_return_node() {
    let mut designer = setup_designer_with_network("test_network");

    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    designer.set_return_node_id(Some(sphere_id));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    assert!(
        matches!(
            *network.node_type.output_type(),
            atomcad_structure_designer::data_type::DataType::Blueprint
        ),
        "Output type should be Blueprint for sphere"
    );
}
