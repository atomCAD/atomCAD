//! Static/structural tests for the `fold` node.
//!
//! Phase 5 of `doc/design_zones.md` retired the function-pin `f` parameter:
//! `fold` is now driven by an inline zone body with two zone-input pins
//! (`acc`, `element`) and one zone-output pin (`new_acc`). End-to-end
//! evaluation coverage moved to `zones_test.rs`; this file keeps the
//! static-shape tests (registration, calculate_custom_node_type, text
//! properties, serde, clone, .cnnd roundtrip).

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
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
fn test_fold_default() {
    let data = FoldData::default();
    assert_eq!(data.element_type, DataType::Float);
    assert_eq!(data.accumulator_type, DataType::Float);
}

#[test]
fn test_fold_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("fold");
    assert!(node_type.is_some(), "fold should be registered");
    let nt = node_type.unwrap();
    assert_eq!(nt.name, "fold");
    assert!(nt.public);
    // Phase 5: `f` is gone — only external inputs are `xs` and `init`. The
    // combining body lives inside the zone.
    assert_eq!(nt.parameters.len(), 2);
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.parameters[1].name, "init");
    assert_eq!(nt.output_pins.len(), 1);
    assert_eq!(*nt.output_type(), DataType::Float);

    // Zone-input pins: acc (A) and element (T). Zone-output: new_acc (A).
    assert_eq!(nt.zone_input_pins.len(), 2);
    assert_eq!(nt.zone_input_pins[0].name, "acc");
    assert_eq!(nt.zone_input_pins[1].name, "element");
    assert_eq!(nt.zone_output_pins.len(), 1);
    assert_eq!(nt.zone_output_pins[0].name, "new_acc");
}

// ============================================================================
// calculate_custom_node_type tests
// ============================================================================

#[test]
fn test_fold_custom_type_int_int() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("fold").unwrap();
    let data = FoldData {
        element_type: DataType::Int,
        accumulator_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(custom.parameters.len(), 2);
    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Int))
    );
    assert_eq!(custom.parameters[1].data_type, DataType::Int);
    assert_eq!(*custom.output_type(), DataType::Int);

    assert_eq!(custom.zone_input_pins.len(), 2);
    assert_eq!(custom.zone_input_pins[0].fixed_type(), Some(&DataType::Int));
    assert_eq!(custom.zone_input_pins[1].fixed_type(), Some(&DataType::Int));
    assert_eq!(custom.zone_output_pins.len(), 1);
    assert_eq!(custom.zone_output_pins[0].data_type, DataType::Int);
}

#[test]
fn test_fold_custom_type_ivec3_int() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("fold").unwrap();
    let data = FoldData {
        element_type: DataType::IVec3,
        accumulator_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::IVec3))
    );
    assert_eq!(custom.parameters[1].data_type, DataType::Int);
    assert_eq!(*custom.output_type(), DataType::Int);

    // acc: Int, element: IVec3
    assert_eq!(custom.zone_input_pins[0].fixed_type(), Some(&DataType::Int));
    assert_eq!(
        custom.zone_input_pins[1].fixed_type(),
        Some(&DataType::IVec3)
    );
    assert_eq!(custom.zone_output_pins[0].data_type, DataType::Int);
}

// ============================================================================
// Text properties roundtrip
// ============================================================================

#[test]
fn test_fold_text_properties_roundtrip() {
    let original = FoldData {
        element_type: DataType::IVec3,
        accumulator_type: DataType::Int,
    };
    let props = original.get_text_properties();
    assert_eq!(props.len(), 2);

    let mut restored = FoldData::default();
    let props_map = props_to_hashmap(props);
    restored.set_text_properties(&props_map).unwrap();

    assert_eq!(restored.element_type, original.element_type);
    assert_eq!(restored.accumulator_type, original.accumulator_type);
}

#[test]
fn test_fold_text_properties_values() {
    let data = FoldData {
        element_type: DataType::IVec3,
        accumulator_type: DataType::Int,
    };
    let props = data.get_text_properties();
    let map = props_to_hashmap(props);
    assert_eq!(
        map.get("element_type"),
        Some(&TextValue::DataType(DataType::IVec3))
    );
    assert_eq!(
        map.get("accumulator_type"),
        Some(&TextValue::DataType(DataType::Int))
    );
}

// ============================================================================
// Serde roundtrip
// ============================================================================

#[test]
fn test_fold_data_serde_roundtrip() {
    let original = FoldData {
        element_type: DataType::IVec3,
        accumulator_type: DataType::Int,
    };
    let json = serde_json::to_value(&original).unwrap();
    assert_eq!(json["element_type"], "IVec3");
    assert_eq!(json["accumulator_type"], "Int");
    let restored: FoldData = serde_json::from_value(json).unwrap();
    assert_eq!(restored.element_type, original.element_type);
    assert_eq!(restored.accumulator_type, original.accumulator_type);
}

// ============================================================================
// clone_box
// ============================================================================

#[test]
fn test_fold_clone_box() {
    let data = FoldData {
        element_type: DataType::Int,
        accumulator_type: DataType::Float,
    };
    let cloned = data.clone_box();
    let props = cloned.get_text_properties();
    let map = props_to_hashmap(props);
    assert_eq!(
        map.get("element_type"),
        Some(&TextValue::DataType(DataType::Int))
    );
    assert_eq!(
        map.get("accumulator_type"),
        Some(&TextValue::DataType(DataType::Float))
    );
}

// ============================================================================
// .cnnd save/load roundtrip
// ============================================================================

#[test]
fn test_fold_cnnd_roundtrip() {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use tempfile::tempdir;

    let mut designer = setup_designer_with_network("main");

    let f_id = designer.add_node("fold", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        f_id,
        Box::new(FoldData {
            element_type: DataType::IVec3,
            accumulator_type: DataType::Int,
        }),
    );
    designer.validate_active_network();

    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("fold.cnnd");
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
        .find(|(_, n)| n.node_type_name == "fold")
        .expect("fold node should survive roundtrip");
    let data = node
        .data
        .as_any_ref()
        .downcast_ref::<FoldData>()
        .expect("fold node should carry FoldData");
    assert_eq!(data.element_type, DataType::IVec3);
    assert_eq!(data.accumulator_type, DataType::Int);
}
