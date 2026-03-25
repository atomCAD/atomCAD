// Phase 1 unit tests for multi-output pin data structures.

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::EvalOutput;
use rust_lib_flutter_cad::structure_designer::node_type::OutputPinDefinition;

// ===== OutputPinDefinition tests =====

#[test]
fn test_output_pin_definition_single() {
    let pins = OutputPinDefinition::single(DataType::Geometry);
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].name, "result");
    assert_eq!(pins[0].data_type, DataType::Geometry);
}

#[test]
fn test_output_pin_definition_single_none() {
    let pins = OutputPinDefinition::single(DataType::None);
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].data_type, DataType::None);
}

// ===== NodeType accessor tests =====

#[test]
fn test_node_type_output_type_accessor() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(*sphere_type.output_type(), DataType::Geometry);
}

#[test]
fn test_node_type_get_output_pin_type_pin0() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(sphere_type.get_output_pin_type(0), DataType::Geometry);
}

#[test]
fn test_node_type_get_output_pin_type_function_pin() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    let fn_type = sphere_type.get_output_pin_type(-1);
    assert!(matches!(fn_type, DataType::Function(_)));
}

#[test]
fn test_node_type_get_output_pin_type_out_of_range() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(sphere_type.get_output_pin_type(1), DataType::None);
    assert_eq!(sphere_type.get_output_pin_type(99), DataType::None);
}

#[test]
fn test_node_type_output_pin_count() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(sphere_type.output_pin_count(), 1);
}

#[test]
fn test_node_type_has_multi_output_single() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert!(!sphere_type.has_multi_output());
}

// ===== EvalOutput tests =====

#[test]
fn test_eval_output_single() {
    let output = EvalOutput::single(NetworkResult::Float(42.0));
    assert_eq!(output.results.len(), 1);
    assert!(matches!(output.primary(), NetworkResult::Float(v) if *v == 42.0));
}

#[test]
fn test_eval_output_multi() {
    let output = EvalOutput::multi(vec![NetworkResult::Float(1.0), NetworkResult::Int(2)]);
    assert_eq!(output.results.len(), 2);
    assert!(matches!(output.primary(), NetworkResult::Float(v) if *v == 1.0));
}

#[test]
fn test_eval_output_get_valid_index() {
    let output = EvalOutput::multi(vec![NetworkResult::Float(1.0), NetworkResult::Int(2)]);
    assert!(matches!(output.get(0), NetworkResult::Float(v) if v == 1.0));
    assert!(matches!(output.get(1), NetworkResult::Int(2)));
}

#[test]
fn test_eval_output_get_out_of_range() {
    let output = EvalOutput::single(NetworkResult::Float(1.0));
    assert!(matches!(output.get(1), NetworkResult::None));
    assert!(matches!(output.get(99), NetworkResult::None));
}

#[test]
fn test_eval_output_get_negative_index() {
    // Negative indices (like -1 for function pin) are handled by the evaluator,
    // not by EvalOutput. get() treats them as out of range.
    let output = EvalOutput::single(NetworkResult::Float(1.0));
    // -1 as i32 cast to usize wraps to a very large number, so get() returns None
    assert!(matches!(output.get(-1), NetworkResult::None));
}

#[test]
fn test_eval_output_primary() {
    let output = EvalOutput::single(NetworkResult::Error("test".to_string()));
    assert!(matches!(output.primary(), NetworkResult::Error(_)));
}
