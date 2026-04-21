//! Tests for step 6.4 of the Crystal / Molecule split: wire validation,
//! polymorphic-output resolution, and output-type propagation into custom
//! network types.
//!
//! See `doc/design_crystal_molecule_split.md` §6.4.

use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_data::NoData;
use rust_lib_flutter_cad::structure_designer::node_network::{Argument, Node, NodeNetwork};
use rust_lib_flutter_cad::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, no_data_loader, no_data_saver,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn toy_node_type(
    name: &str,
    parameters: Vec<Parameter>,
    output_pins: Vec<OutputPinDefinition>,
) -> NodeType {
    NodeType {
        name: name.to_string(),
        description: String::new(),
        summary: None,
        category: rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory::OtherBuiltin,
        parameters,
        output_pins,
        public: true,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    }
}

fn make_node(id: u64, node_type_name: &str, arg_count: usize) -> Node {
    Node {
        id,
        node_type_name: node_type_name.to_string(),
        custom_name: None,
        position: DVec2::ZERO,
        arguments: (0..arg_count).map(|_| Argument::new()).collect(),
        data: Box::new(NoData {}),
        custom_node_type: None,
    }
}

/// A registry with a polymorphic node `poly` (abstract `Atomic` input + `SameAsInput`
/// output), a concrete-source `crystal_src` (produces `Crystal`), a concrete-source
/// `molecule_src` (produces `Molecule`), plus an array-polymorphic node `arr_poly`
/// (`Array[Atomic]` input + `SameAsArrayElements` output).
fn build_polymorphic_registry() -> NodeTypeRegistry {
    let mut registry = NodeTypeRegistry::new();

    registry.built_in_node_types.insert(
        "crystal_src".to_string(),
        toy_node_type(
            "crystal_src",
            vec![],
            OutputPinDefinition::single(DataType::Crystal),
        ),
    );
    registry.built_in_node_types.insert(
        "molecule_src".to_string(),
        toy_node_type(
            "molecule_src",
            vec![],
            OutputPinDefinition::single(DataType::Molecule),
        ),
    );
    registry.built_in_node_types.insert(
        "poly".to_string(),
        toy_node_type(
            "poly",
            vec![Parameter {
                id: None,
                name: "in".to_string(),
                data_type: DataType::HasAtoms,
            }],
            OutputPinDefinition::single_same_as("in"),
        ),
    );
    registry.built_in_node_types.insert(
        "arr_poly".to_string(),
        toy_node_type(
            "arr_poly",
            vec![Parameter {
                id: None,
                name: "arr".to_string(),
                data_type: DataType::Array(Box::new(DataType::HasAtoms)),
            }],
            OutputPinDefinition::single_same_as_array_elements("arr"),
        ),
    );

    registry
}

fn empty_network(name: &str) -> NodeNetwork {
    NodeNetwork::new(NodeType {
        name: name.to_string(),
        description: String::new(),
        summary: None,
        category: rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory::OtherBuiltin,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::None),
        public: true,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    })
}

#[test]
fn polymorphic_output_with_unconnected_input_flags_node_invalid() {
    // poly has a SameAsInput("in") output. With "in" unconnected, the output
    // cannot resolve, so validation must flag the node invalid.
    let mut registry = build_polymorphic_registry();
    let mut network = empty_network("test");
    let poly = make_node(1, "poly", 1);
    network.nodes.insert(1, poly);
    registry.node_networks.insert("test".to_string(), network);

    // Drive the full validator through the public StructureDesigner API by
    // swapping a freshly built designer's registry entry with ours.
    let mut designer = StructureDesigner::new();
    designer.node_type_registry = registry;
    designer.set_active_node_network_name(Some("test".to_string()));
    designer.validate_active_network();

    let network = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    assert!(
        !network.valid,
        "poly with unconnected abstract input must be invalid"
    );
}

#[test]
fn mixed_phase_array_into_same_as_array_elements_flags_node_invalid() {
    // arr_poly has an Array[Atomic] input + SameAsArrayElements("arr") output.
    // Feeding it both Crystal and Molecule should fail resolution and flag
    // the node invalid.
    let mut registry = build_polymorphic_registry();
    let mut network = empty_network("test");

    let crystal = make_node(1, "crystal_src", 0);
    let molecule = make_node(2, "molecule_src", 0);
    let mut arr_poly = make_node(3, "arr_poly", 1);
    arr_poly.arguments[0].argument_output_pins.insert(1, 0);
    arr_poly.arguments[0].argument_output_pins.insert(2, 0);
    network.nodes.insert(1, crystal);
    network.nodes.insert(2, molecule);
    network.nodes.insert(3, arr_poly);
    registry.node_networks.insert("test".to_string(), network);

    let mut designer = StructureDesigner::new();
    designer.node_type_registry = registry;
    designer.set_active_node_network_name(Some("test".to_string()));
    designer.validate_active_network();

    let network = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    assert!(
        !network.valid,
        "arr_poly fed mixed Crystal+Molecule must be invalid"
    );
}

#[test]
fn same_kind_array_into_same_as_array_elements_resolves_and_is_valid() {
    // Two Crystal sources into an Array[Atomic] pin should resolve to Crystal.
    let mut registry = build_polymorphic_registry();
    let mut network = empty_network("test");

    let c1 = make_node(1, "crystal_src", 0);
    let c2 = make_node(2, "crystal_src", 0);
    let mut arr_poly = make_node(3, "arr_poly", 1);
    arr_poly.arguments[0].argument_output_pins.insert(1, 0);
    arr_poly.arguments[0].argument_output_pins.insert(2, 0);
    network.nodes.insert(1, c1);
    network.nodes.insert(2, c2);
    network.nodes.insert(3, arr_poly);
    registry.node_networks.insert("test".to_string(), network);

    let mut designer = StructureDesigner::new();
    designer.node_type_registry = registry;
    designer.set_active_node_network_name(Some("test".to_string()));
    designer.validate_active_network();

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    assert!(network.valid, "uniform-Crystal array must be valid");

    // Check that resolve_output_type returns Crystal for arr_poly's output pin,
    // idempotent relative to the cache used by the validator.
    let arr_poly_node = network.nodes.get(&3).unwrap();
    let resolved = registry.resolve_output_type(arr_poly_node, network, 0);
    assert_eq!(resolved, Some(DataType::Crystal));
    // Re-resolve — pure function, must return the same value.
    let resolved2 = registry.resolve_output_type(arr_poly_node, network, 0);
    assert_eq!(resolved, resolved2);
}

#[test]
fn resolve_output_type_is_idempotent_against_validation_cache() {
    // Two Molecule sources into an Array[Atomic] pin must resolve to Molecule.
    let mut registry = build_polymorphic_registry();
    let mut network = empty_network("test");

    let m = make_node(1, "molecule_src", 0);
    let mut poly = make_node(2, "poly", 1);
    poly.arguments[0].argument_output_pins.insert(1, 0);
    network.nodes.insert(1, m);
    network.nodes.insert(2, poly);
    registry.node_networks.insert("test".to_string(), network);

    let mut designer = StructureDesigner::new();
    designer.node_type_registry = registry;
    designer.set_active_node_network_name(Some("test".to_string()));
    designer.validate_active_network();

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    assert!(network.valid);

    let poly_node = network.nodes.get(&2).unwrap();
    let first = registry.resolve_output_type(poly_node, network, 0);
    let second = registry.resolve_output_type(poly_node, network, 0);
    assert_eq!(first, Some(DataType::Molecule));
    assert_eq!(first, second, "resolution must be idempotent");
}

#[test]
fn parameter_pin_cannot_be_assigned_an_abstract_type() {
    // A `parameter` node whose data_type is Atomic (abstract) must cause
    // validation to fail.
    let mut designer = StructureDesigner::new();
    designer.add_node_network("inner");
    designer.set_active_node_network_name(Some("inner".to_string()));
    let param_id = designer.add_node("parameter", DVec2::ZERO);

    // Edit the parameter node's data to declare an abstract type.
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
    let existing: &dyn NodeData = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap()
        .nodes
        .get(&param_id)
        .unwrap()
        .data
        .as_ref();
    let mut new_data: ParameterData = existing
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .unwrap()
        .clone();
    new_data.data_type = DataType::HasAtoms;
    designer.set_node_network_data(param_id, Box::new(new_data));

    designer.validate_active_network();
    let network = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap();
    assert!(
        !network.valid,
        "parameter node with abstract type must fail validation"
    );
}

#[test]
fn custom_network_output_pin_is_fixed_concrete_when_return_uses_same_as_input() {
    // Custom network with a concrete `Crystal` parameter feeding a `poly`
    // node as the return node. `update_network_output_type` must synthesise
    // a `Fixed(Crystal)` output pin on the enclosing network.
    let mut designer = StructureDesigner::new();
    // Install poly + crystal_src into the live registry so the custom
    // network can reference them.
    let toy = build_polymorphic_registry();
    for (name, nt) in toy.built_in_node_types {
        designer
            .node_type_registry
            .built_in_node_types
            .insert(name, nt);
    }

    designer.add_node_network("inner");
    designer.set_active_node_network_name(Some("inner".to_string()));

    // Add a parameter node with DataType::Crystal.
    let param_id = designer.add_node("parameter", DVec2::ZERO);
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
    let existing: &dyn NodeData = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap()
        .nodes
        .get(&param_id)
        .unwrap()
        .data
        .as_ref();
    let mut new_data: ParameterData = existing
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .unwrap()
        .clone();
    new_data.data_type = DataType::Crystal;
    designer.set_node_network_data(param_id, Box::new(new_data));

    // Add a poly node and connect the parameter node's output to it.
    let poly_id = designer.add_node("poly", DVec2::new(100.0, 0.0));
    designer.connect_nodes(param_id, 0, poly_id, 0);
    designer.set_return_node_id(Some(poly_id));
    designer.validate_active_network();

    let registry = &designer.node_type_registry;
    let inner = registry.node_networks.get("inner").unwrap();
    assert!(inner.valid, "inner must be valid");
    assert_eq!(inner.node_type.output_pins.len(), 1);
    assert_eq!(
        inner.node_type.output_pins[0].fixed_type(),
        Some(&DataType::Crystal),
        "custom network's output pin must be Fixed(Crystal) after substitution"
    );
}
