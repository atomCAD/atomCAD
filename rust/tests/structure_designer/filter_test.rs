//! Static/structural tests for the `filter` node.
//!
//! Phase 5 of `doc/design_zones.md` retired the function-pin `f` parameter:
//! `filter` is now driven by an inline zone body with one zone-input pin
//! (`element`) and one zone-output pin (`keep: Bool`). End-to-end evaluation
//! coverage moved to `zones_test.rs`; this file keeps the static-shape tests
//! (registration, calculate_custom_node_type, text properties, serde, clone,
//! .cnnd roundtrip).

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::filter::FilterData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
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

fn props_to_hashmap(props: Vec<(String, TextValue)>) -> HashMap<String, TextValue> {
    props.into_iter().collect()
}

// ============================================================================
// Default values & registration
// ============================================================================

#[test]
fn test_filter_default() {
    let data = FilterData::default();
    assert_eq!(data.element_type, DataType::Float);
}

#[test]
fn test_filter_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("filter");
    assert!(node_type.is_some(), "filter should be registered");
    let nt = node_type.unwrap();
    assert_eq!(nt.name, "filter");
    assert!(nt.public);
    // Closures Phase 4 re-added an optional `f` (predicate function value) pin
    // alongside `xs`; the predicate body still lives inside the zone (used when
    // `f` is disconnected).
    assert_eq!(nt.parameters.len(), 2);
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.parameters[1].name, "f");
    assert!(matches!(nt.parameters[1].data_type, DataType::Function(_)));
    assert_eq!(nt.output_pins.len(), 1);
    assert_eq!(
        *nt.output_type(),
        DataType::Iterator(Box::new(DataType::Float))
    );

    // Zone-input pin: element (T). Zone-output: keep (Bool).
    assert_eq!(nt.zone_input_pins.len(), 1);
    assert_eq!(nt.zone_input_pins[0].name, "element");
    assert_eq!(nt.zone_output_pins.len(), 1);
    assert_eq!(nt.zone_output_pins[0].name, "keep");
    assert_eq!(nt.zone_output_pins[0].data_type, DataType::Bool);
}

// ============================================================================
// calculate_custom_node_type tests
// ============================================================================

#[test]
fn test_filter_custom_type_int() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("filter").unwrap();
    let data = FilterData {
        element_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(custom.parameters.len(), 2);
    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Int))
    );
    // `f`: (element_type) -> Bool.
    assert_eq!(
        custom.parameters[1].data_type,
        DataType::Function(rust_lib_flutter_cad::structure_designer::data_type::FunctionType {
            parameter_types: vec![DataType::Int],
            output_type: Box::new(DataType::Bool),
        })
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Iterator(Box::new(DataType::Int))
    );

    assert_eq!(custom.zone_input_pins.len(), 1);
    assert_eq!(custom.zone_input_pins[0].fixed_type(), Some(&DataType::Int));
    assert_eq!(custom.zone_output_pins.len(), 1);
    assert_eq!(custom.zone_output_pins[0].data_type, DataType::Bool);
}

#[test]
fn test_filter_custom_type_ivec3() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("filter").unwrap();
    let data = FilterData {
        element_type: DataType::IVec3,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::IVec3))
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Iterator(Box::new(DataType::IVec3))
    );
    assert_eq!(
        custom.zone_input_pins[0].fixed_type(),
        Some(&DataType::IVec3)
    );
    assert_eq!(custom.zone_output_pins[0].data_type, DataType::Bool);
}

// ============================================================================
// Text properties roundtrip
// ============================================================================

#[test]
fn test_filter_text_properties_roundtrip() {
    let original = FilterData {
        element_type: DataType::IVec3,
    };
    let props = original.get_text_properties();
    assert_eq!(props.len(), 1);

    let mut restored = FilterData::default();
    let props_map = props_to_hashmap(props);
    restored.set_text_properties(&props_map).unwrap();

    assert_eq!(restored.element_type, original.element_type);
}

#[test]
fn test_filter_text_properties_values() {
    let data = FilterData {
        element_type: DataType::IVec3,
    };
    let props = data.get_text_properties();
    assert_eq!(
        props[0],
        (
            "element_type".to_string(),
            TextValue::DataType(DataType::IVec3)
        )
    );
}

// ============================================================================
// Serde roundtrip
// ============================================================================

#[test]
fn test_filter_data_serde_roundtrip() {
    let original = FilterData {
        element_type: DataType::IVec3,
    };
    let json = serde_json::to_value(&original).unwrap();
    assert_eq!(json["element_type"], "IVec3");
    let restored: FilterData = serde_json::from_value(json).unwrap();
    assert_eq!(restored.element_type, original.element_type);
}

// ============================================================================
// clone_box
// ============================================================================

#[test]
fn test_filter_clone_box() {
    let data = FilterData {
        element_type: DataType::Int,
    };
    let cloned = data.clone_box();
    let props = cloned.get_text_properties();
    let map = props_to_hashmap(props);
    assert_eq!(
        map.get("element_type"),
        Some(&TextValue::DataType(DataType::Int))
    );
}

// ============================================================================
// .cnnd save/load roundtrip
// ============================================================================

#[test]
fn test_filter_cnnd_roundtrip() {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use tempfile::tempdir;

    let mut designer = setup_designer_with_network("main");

    let f_id = designer.add_node("filter", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        f_id,
        Box::new(FilterData {
            element_type: DataType::IVec3,
        }),
    );
    designer.validate_active_network();

    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("filter.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save should succeed");

    let mut registry2 = NodeTypeRegistry::new();
    let _load = load_node_networks_from_file(&mut registry2, path.to_str().unwrap())
        .expect("load should succeed");

    let network = registry2
        .node_networks
        .get("main")
        .expect("main network should survive roundtrip");
    let (_, node) = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "filter")
        .expect("filter node should survive roundtrip");
    let data = node
        .data
        .as_any_ref()
        .downcast_ref::<FilterData>()
        .expect("filter node should carry FilterData");
    assert_eq!(
        data.element_type,
        DataType::IVec3,
        "element_type should survive .cnnd roundtrip"
    );
}
