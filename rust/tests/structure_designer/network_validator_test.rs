use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

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
            network.node_type.output_type,
            rust_lib_flutter_cad::structure_designer::data_type::DataType::Geometry
        ),
        "Output type should be Geometry for sphere"
    );
}
