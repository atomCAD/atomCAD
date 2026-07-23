use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::array_at::ArrayAtData;
use rust_lib_flutter_cad::structure_designer::nodes::bool::BoolData;
use rust_lib_flutter_cad::structure_designer::nodes::float::FloatData;
use rust_lib_flutter_cad::structure_designer::nodes::if_else::IfData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// ============================================================================
// Helpers (mirrors array_at_test.rs)
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
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

/// Adds an `int` node holding `value`.
fn add_int(designer: &mut StructureDesigner, network: &str, value: i32, y: f64) -> u64 {
    let id = designer.add_node("int", DVec2::new(0.0, y));
    set_node_data(designer, network, id, Box::new(IntData { value }));
    id
}

/// Adds a `bool` node holding `value`.
fn add_bool(designer: &mut StructureDesigner, network: &str, value: bool, y: f64) -> u64 {
    let id = designer.add_node("bool", DVec2::new(0.0, y));
    set_node_data(designer, network, id, Box::new(BoolData { value }));
    id
}

/// Adds an `if` node with the given value type.
fn add_if(designer: &mut StructureDesigner, network: &str, value_type: DataType, x: f64) -> u64 {
    let id = designer.add_node("if", DVec2::new(x, 0.0));
    set_node_data(designer, network, id, Box::new(IfData { value_type }));
    id
}

/// Adds an `array_at` node reading index 0 of an empty `Array[Int]`, which
/// evaluates to a "length 0" error. Used to prove a branch is *not* evaluated:
/// if it were, the `if` output would be that error.
fn add_erroring_int_source(designer: &mut StructureDesigner, network: &str, y: f64) -> u64 {
    let seq_id = designer.add_node("sequence", DVec2::new(-200.0, y));
    set_node_data(
        designer,
        network,
        seq_id,
        Box::new(SequenceData {
            element_type: DataType::Int,
            input_count: 1,
        }),
    );
    let at_id = designer.add_node("array_at", DVec2::new(-100.0, y));
    set_node_data(
        designer,
        network,
        at_id,
        Box::new(ArrayAtData {
            element_type: DataType::Int,
            index: 0,
        }),
    );
    designer.validate_active_network();
    // Wire empty sequence into array_at's `array` pin (pin 0). Index defaults to
    // 0 (stored), so eval yields an out-of-bounds error against a length-0 array.
    designer.connect_nodes(seq_id, 0, at_id, 0);
    at_id
}

// ============================================================================
// Registration & defaults
// ============================================================================

#[test]
fn test_if_default() {
    let data = IfData::default();
    assert_eq!(data.value_type, DataType::Float);
}

#[test]
fn test_if_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("if");
    assert!(node_type.is_some(), "if should be registered");
    let nt = node_type.unwrap();
    assert_eq!(nt.name, "if");
    assert!(nt.public);
    assert_eq!(nt.parameters.len(), 3);
    assert_eq!(nt.parameters[0].name, "cond");
    assert_eq!(nt.parameters[0].data_type, DataType::Bool);
    assert_eq!(nt.parameters[1].name, "then");
    assert_eq!(nt.parameters[2].name, "else");
    assert_eq!(nt.output_pins.len(), 1);
}

// ============================================================================
// calculate_custom_node_type
// ============================================================================

#[test]
fn test_if_custom_type_int() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("if").unwrap();
    let data = IfData {
        value_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    // cond stays Bool; then/else and output take the value type.
    assert_eq!(custom.parameters[0].data_type, DataType::Bool);
    assert_eq!(custom.parameters[1].data_type, DataType::Int);
    assert_eq!(custom.parameters[2].data_type, DataType::Int);
    assert_eq!(*custom.output_type(), DataType::Int);
}

#[test]
fn test_if_custom_type_structural() {
    // A structural value type (Crystal) — exactly the case `expr` can't select.
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("if").unwrap();
    let data = IfData {
        value_type: DataType::Crystal,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(custom.parameters[1].data_type, DataType::Crystal);
    assert_eq!(custom.parameters[2].data_type, DataType::Crystal);
    assert_eq!(*custom.output_type(), DataType::Crystal);
}

// ============================================================================
// Evaluation: branch selection
// ============================================================================

#[test]
fn test_if_true_selects_then() {
    let mut designer = setup_designer_with_network("test");
    let cond = add_bool(&mut designer, "test", true, 0.0);
    let then_id = add_int(&mut designer, "test", 42, 100.0);
    let else_id = add_int(&mut designer, "test", 99, 200.0);
    let if_id = add_if(&mut designer, "test", DataType::Int, 300.0);
    designer.validate_active_network();

    designer.connect_nodes(cond, 0, if_id, 0);
    designer.connect_nodes(then_id, 0, if_id, 1);
    designer.connect_nodes(else_id, 0, if_id, 2);

    match evaluate_node(&designer, "test", if_id) {
        NetworkResult::Int(v) => assert_eq!(v, 42),
        other => panic!("Expected Int(42), got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_if_false_selects_else() {
    let mut designer = setup_designer_with_network("test");
    let cond = add_bool(&mut designer, "test", false, 0.0);
    let then_id = add_int(&mut designer, "test", 42, 100.0);
    let else_id = add_int(&mut designer, "test", 99, 200.0);
    let if_id = add_if(&mut designer, "test", DataType::Int, 300.0);
    designer.validate_active_network();

    designer.connect_nodes(cond, 0, if_id, 0);
    designer.connect_nodes(then_id, 0, if_id, 1);
    designer.connect_nodes(else_id, 0, if_id, 2);

    match evaluate_node(&designer, "test", if_id) {
        NetworkResult::Int(v) => assert_eq!(v, 99),
        other => panic!("Expected Int(99), got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Evaluation: laziness (the untaken branch is never evaluated)
// ============================================================================

#[test]
fn test_if_true_does_not_evaluate_else() {
    // else branch is an erroring source; if it were evaluated the output would
    // be that error. cond=true must select `then` and skip `else` entirely.
    let mut designer = setup_designer_with_network("test");
    let cond = add_bool(&mut designer, "test", true, 0.0);
    let then_id = add_int(&mut designer, "test", 7, 100.0);
    let else_id = add_erroring_int_source(&mut designer, "test", 200.0);
    let if_id = add_if(&mut designer, "test", DataType::Int, 300.0);
    designer.validate_active_network();

    designer.connect_nodes(cond, 0, if_id, 0);
    designer.connect_nodes(then_id, 0, if_id, 1);
    designer.connect_nodes(else_id, 0, if_id, 2);

    match evaluate_node(&designer, "test", if_id) {
        NetworkResult::Int(v) => assert_eq!(v, 7),
        other => panic!(
            "Expected Int(7) (else not evaluated), got {:?}",
            other.to_display_string()
        ),
    }
}

#[test]
fn test_if_false_does_not_evaluate_then() {
    let mut designer = setup_designer_with_network("test");
    let cond = add_bool(&mut designer, "test", false, 0.0);
    let then_id = add_erroring_int_source(&mut designer, "test", 100.0);
    let else_id = add_int(&mut designer, "test", 7, 200.0);
    let if_id = add_if(&mut designer, "test", DataType::Int, 300.0);
    designer.validate_active_network();

    designer.connect_nodes(cond, 0, if_id, 0);
    designer.connect_nodes(then_id, 0, if_id, 1);
    designer.connect_nodes(else_id, 0, if_id, 2);

    match evaluate_node(&designer, "test", if_id) {
        NetworkResult::Int(v) => assert_eq!(v, 7),
        other => panic!(
            "Expected Int(7) (then not evaluated), got {:?}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Evaluation: optional pins (unwired → None)
// ============================================================================

#[test]
fn test_if_unwired_cond_is_inert() {
    // No cond wired → node is inert, outputs None (even with branches wired).
    let mut designer = setup_designer_with_network("test");
    let then_id = add_int(&mut designer, "test", 42, 100.0);
    let else_id = add_int(&mut designer, "test", 99, 200.0);
    let if_id = add_if(&mut designer, "test", DataType::Int, 300.0);
    designer.validate_active_network();

    designer.connect_nodes(then_id, 0, if_id, 1);
    designer.connect_nodes(else_id, 0, if_id, 2);

    match evaluate_node(&designer, "test", if_id) {
        NetworkResult::None => {}
        other => panic!("Expected None, got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_if_unwired_taken_branch_yields_none() {
    // cond=true but `then` unwired → output None (graceful, not an error).
    let mut designer = setup_designer_with_network("test");
    let cond = add_bool(&mut designer, "test", true, 0.0);
    let else_id = add_int(&mut designer, "test", 99, 200.0);
    let if_id = add_if(&mut designer, "test", DataType::Int, 300.0);
    designer.validate_active_network();

    designer.connect_nodes(cond, 0, if_id, 0);
    designer.connect_nodes(else_id, 0, if_id, 2);

    match evaluate_node(&designer, "test", if_id) {
        NetworkResult::None => {}
        other => panic!("Expected None, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Evaluation: error propagation from cond
// ============================================================================

#[test]
fn test_if_cond_error_propagates() {
    // A cond input that errors propagates the error (does not select a branch).
    // Build an erroring Bool-ish source by wiring an erroring int into cond is
    // type-incompatible; instead we drive cond from an erroring array_at typed
    // Bool.
    let mut designer = setup_designer_with_network("test");

    let seq_id = designer.add_node("sequence", DVec2::new(-200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        seq_id,
        Box::new(SequenceData {
            element_type: DataType::Bool,
            input_count: 1,
        }),
    );
    let cond_at = designer.add_node("array_at", DVec2::new(-100.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        cond_at,
        Box::new(ArrayAtData {
            element_type: DataType::Bool,
            index: 0,
        }),
    );
    let then_id = add_int(&mut designer, "test", 42, 100.0);
    let else_id = add_int(&mut designer, "test", 99, 200.0);
    let if_id = add_if(&mut designer, "test", DataType::Int, 300.0);
    designer.validate_active_network();

    designer.connect_nodes(seq_id, 0, cond_at, 0); // empty Bool array → error
    designer.connect_nodes(cond_at, 0, if_id, 0);
    designer.connect_nodes(then_id, 0, if_id, 1);
    designer.connect_nodes(else_id, 0, if_id, 2);

    match evaluate_node(&designer, "test", if_id) {
        NetworkResult::Error(_) => {}
        other => panic!("Expected Error, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Text properties roundtrip
// ============================================================================

#[test]
fn test_if_text_properties_roundtrip() {
    let original = IfData {
        value_type: DataType::Crystal,
    };
    let props = original.get_text_properties();
    assert_eq!(props.len(), 1);

    let mut restored = IfData::default();
    let props_map = props_to_hashmap(props);
    restored.set_text_properties(&props_map).unwrap();

    assert_eq!(restored.value_type, original.value_type);
}

#[test]
fn test_if_text_properties_values() {
    let data = IfData {
        value_type: DataType::Int,
    };
    let props = data.get_text_properties();
    assert_eq!(
        props[0],
        ("value_type".to_string(), TextValue::DataType(DataType::Int))
    );
}

// ============================================================================
// Serde roundtrip
// ============================================================================

#[test]
fn test_if_data_serde_roundtrip() {
    let original = IfData {
        value_type: DataType::IVec3,
    };
    let json = serde_json::to_value(&original).unwrap();
    assert_eq!(json["value_type"], "IVec3");
    let restored: IfData = serde_json::from_value(json).unwrap();
    assert_eq!(restored.value_type, original.value_type);
}

// ============================================================================
// clone_box
// ============================================================================

#[test]
fn test_if_clone_box() {
    let data = IfData {
        value_type: DataType::Float,
    };
    let cloned = data.clone_box();
    let map = props_to_hashmap(cloned.get_text_properties());
    assert_eq!(
        map.get("value_type"),
        Some(&TextValue::DataType(DataType::Float))
    );
}

// ============================================================================
// Float value type (also exercises the non-Int path)
// ============================================================================

#[test]
fn test_if_float_value_type() {
    let mut designer = setup_designer_with_network("test");
    let cond = add_bool(&mut designer, "test", false, 0.0);

    let then_f = designer.add_node("float", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "test",
        then_f,
        Box::new(FloatData { value: 1.5 }),
    );
    let else_f = designer.add_node("float", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        "test",
        else_f,
        Box::new(FloatData { value: 2.5 }),
    );

    let if_id = add_if(&mut designer, "test", DataType::Float, 300.0);
    designer.validate_active_network();

    designer.connect_nodes(cond, 0, if_id, 0);
    designer.connect_nodes(then_f, 0, if_id, 1);
    designer.connect_nodes(else_f, 0, if_id, 2);

    match evaluate_node(&designer, "test", if_id) {
        NetworkResult::Float(v) => assert!((v - 2.5).abs() < 1e-9, "got {}", v),
        other => panic!("Expected Float(2.5), got {:?}", other.to_display_string()),
    }
}
