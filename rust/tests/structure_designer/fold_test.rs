use glam::f64::DVec2;
use glam::i32::IVec3;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use rust_lib_flutter_cad::structure_designer::text_format::edit_network;
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

fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
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
        node,
        true,
    );
}

fn props_to_hashmap(props: Vec<(String, TextValue)>) -> HashMap<String, TextValue> {
    props.into_iter().collect()
}

fn edit_designer_network(
    designer: &mut StructureDesigner,
    network_name: &str,
    code: &str,
    replace: bool,
) -> rust_lib_flutter_cad::structure_designer::text_format::EditResult {
    let mut network = designer
        .node_type_registry
        .node_networks
        .remove(network_name)
        .unwrap();
    let result = edit_network(&mut network, &designer.node_type_registry, code, replace);
    designer
        .node_type_registry
        .node_networks
        .insert(network_name.to_string(), network);
    designer.validate_active_network();
    result
}

fn find_node_id(designer: &StructureDesigner, network_name: &str, node_type_name: &str) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let (id, _) = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == node_type_name)
        .unwrap_or_else(|| panic!("expected a `{}` node in `{}`", node_type_name, network_name));
    *id
}

fn extract_int(result: NetworkResult) -> i32 {
    match result {
        NetworkResult::Int(v) => v,
        other => panic!("expected Int, got {}", other.to_display_string()),
    }
}

fn expect_error(result: NetworkResult, expected_message: &str) {
    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains(expected_message),
                "expected error containing {:?}, got {:?}",
                expected_message,
                msg
            );
        }
        other => panic!(
            "expected Error containing {:?}, got {}",
            expected_message,
            other.to_display_string()
        ),
    }
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
    assert_eq!(nt.parameters.len(), 3);
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.parameters[1].name, "init");
    assert_eq!(nt.parameters[2].name, "f");
    assert_eq!(nt.output_pins.len(), 1);
    assert_eq!(*nt.output_type(), DataType::Float);
}

// ============================================================================
// calculate_custom_node_type tests
// ============================================================================

#[test]
fn test_fold_custom_type_int_int() {
    use rust_lib_flutter_cad::structure_designer::data_type::FunctionType;

    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("fold").unwrap();
    let data = FoldData {
        element_type: DataType::Int,
        accumulator_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Array(Box::new(DataType::Int))
    );
    assert_eq!(custom.parameters[1].data_type, DataType::Int);
    assert_eq!(
        custom.parameters[2].data_type,
        DataType::Function(FunctionType {
            parameter_types: vec![DataType::Int, DataType::Int],
            output_type: Box::new(DataType::Int),
        })
    );
    assert_eq!(*custom.output_type(), DataType::Int);
}

#[test]
fn test_fold_custom_type_ivec3_int() {
    use rust_lib_flutter_cad::structure_designer::data_type::FunctionType;

    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("fold").unwrap();
    let data = FoldData {
        element_type: DataType::IVec3,
        accumulator_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Array(Box::new(DataType::IVec3))
    );
    assert_eq!(custom.parameters[1].data_type, DataType::Int);
    assert_eq!(
        custom.parameters[2].data_type,
        DataType::Function(FunctionType {
            parameter_types: vec![DataType::Int, DataType::IVec3],
            output_type: Box::new(DataType::Int),
        })
    );
    assert_eq!(*custom.output_type(), DataType::Int);
}

// ============================================================================
// Evaluation: basic sum (Int, Int)
// ============================================================================

#[test]
fn test_fold_int_sum() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 4 }
            i = int { value: 0 }
            combine = expr {
                expression: "acc + elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 10);
}

#[test]
fn test_fold_int_sum_with_init_offset() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 4 }
            i = int { value: 100 }
            combine = expr {
                expression: "acc + elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 110);
}

#[test]
fn test_fold_empty_array_returns_init_unchanged() {
    let mut designer = setup_designer_with_network("main");

    // f's body is irrelevant here: f must never be called.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 0, step: 1, count: 0 }
            i = int { value: 42 }
            combine = expr {
                expression: "acc + elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 42);
}

#[test]
fn test_fold_singleton_array_calls_f_once() {
    let mut designer = setup_designer_with_network("main");

    // xs = [5], init = 0, f(acc, elem) = acc + elem ⇒ result is 5.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 5, step: 1, count: 1 }
            i = int { value: 0 }
            combine = expr {
                expression: "acc + elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 5);
}

#[test]
fn test_fold_int_product() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 3 }
            i = int { value: 1 }
            combine = expr {
                expression: "acc * elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 6);
}

#[test]
fn test_fold_int_max_via_if_then_else() {
    use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
    use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;

    let mut designer = setup_designer_with_network("main");

    // Build [3,1,4,1,5,9,2,6] via int + sequence nodes.
    let values = [3, 1, 4, 1, 5, 9, 2, 6];
    let mut int_ids: Vec<u64> = Vec::new();
    for (idx, &v) in values.iter().enumerate() {
        let id = designer.add_node("int", DVec2::new(0.0, 60.0 * idx as f64));
        set_node_data(&mut designer, "main", id, Box::new(IntData { value: v }));
        int_ids.push(id);
    }
    let seq_id = designer.add_node("sequence", DVec2::new(120.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        seq_id,
        Box::new(SequenceData {
            element_type: DataType::Int,
            input_count: values.len(),
        }),
    );
    designer.validate_active_network();
    for (i, &nid) in int_ids.iter().enumerate() {
        designer.connect_nodes(nid, 0, seq_id, i);
    }

    // init = -1 (below all elements). Use if-then-else for max.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            init0 = int { value: -1 }
            combine = expr {
                expression: "if elem > acc then elem else acc",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: sequence1, init: init0, f: @combine }
        "#,
        false,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 9);
}

// ============================================================================
// Evaluation: Acc differs from T (IVec3 elements, Int accumulator)
// ============================================================================

#[test]
fn test_fold_ivec3_into_int_accumulator() {
    use rust_lib_flutter_cad::structure_designer::nodes::ivec3::IVec3Data;
    use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;

    let mut designer = setup_designer_with_network("main");

    let v0 = designer.add_node("ivec3", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        v0,
        Box::new(IVec3Data {
            value: IVec3::new(1, 2, 3),
        }),
    );
    let v1 = designer.add_node("ivec3", DVec2::new(0.0, 60.0));
    set_node_data(
        &mut designer,
        "main",
        v1,
        Box::new(IVec3Data {
            value: IVec3::new(4, 5, 6),
        }),
    );
    let seq_id = designer.add_node("sequence", DVec2::new(120.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        seq_id,
        Box::new(SequenceData {
            element_type: DataType::IVec3,
            input_count: 2,
        }),
    );
    designer.validate_active_network();
    designer.connect_nodes(v0, 0, seq_id, 0);
    designer.connect_nodes(v1, 0, seq_id, 1);

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            init0 = int { value: 0 }
            combine = expr {
                expression: "acc + elem.x + elem.y + elem.z",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: IVec3 }
                ]
            }
            fld = fold { element_type: IVec3, accumulator_type: Int, xs: sequence1, init: init0, f: @combine }
        "#,
        false,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 21);
}

// ============================================================================
// Evaluation: order matters — left-to-right
// ============================================================================

#[test]
fn test_fold_left_to_right_order() {
    let mut designer = setup_designer_with_network("main");

    // f(acc, elem) = acc * 10 + elem; xs = [1,2,3], init = 0.
    // Left-to-right: 0*10+1=1, 1*10+2=12, 12*10+3=123.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 3 }
            i = int { value: 0 }
            combine = expr {
                expression: "acc * 10 + elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 123);
}

// ============================================================================
// Evaluation: missing-input errors
// ============================================================================

#[test]
fn test_fold_xs_unconnected_yields_error() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            i = int { value: 0 }
            combine = expr {
                expression: "acc + elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, init: i, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    expect_error(
        evaluate_node(&designer, "main", fold_id),
        "xs input is missing",
    );
}

#[test]
fn test_fold_init_unconnected_yields_error() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 3 }
            combine = expr {
                expression: "acc + elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    expect_error(
        evaluate_node(&designer, "main", fold_id),
        "init input is missing",
    );
}

#[test]
fn test_fold_f_unconnected_yields_error() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 3 }
            i = int { value: 0 }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    expect_error(
        evaluate_node(&designer, "main", fold_id),
        "f input is missing",
    );
}

#[test]
fn test_fold_all_unconnected_reports_xs_first() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            fld = fold { element_type: Int, accumulator_type: Int }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    expect_error(
        evaluate_node(&designer, "main", fold_id),
        "xs input is missing",
    );
}

#[test]
fn test_fold_empty_xs_with_unconnected_f_still_errors() {
    let mut designer = setup_designer_with_network("main");

    // xs is wired (empty array) and init is wired, but f is not — the
    // required-input check must fire even when xs would have been empty.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 0, step: 1, count: 0 }
            i = int { value: 7 }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    expect_error(
        evaluate_node(&designer, "main", fold_id),
        "f input is missing",
    );
}

#[test]
fn test_fold_empty_xs_with_unconnected_init_errors() {
    let mut designer = setup_designer_with_network("main");

    // xs is wired (empty array) and f is wired, but init is not — the
    // required-input check must fire even when xs would have been empty.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 0, step: 1, count: 0 }
            combine = expr {
                expression: "acc + elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    expect_error(
        evaluate_node(&designer, "main", fold_id),
        "init input is missing",
    );
}

// ============================================================================
// Evaluation: f errors mid-iteration propagate
// ============================================================================

#[test]
fn test_fold_f_error_propagates() {
    // Predicate errors when elem == 2 (out-of-bounds array access). Confirms
    // that fold propagates errors mid-iteration.
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 4 }
            i = int { value: 0 }
            combine = expr {
                expression: "acc + [10, 20][elem]",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int }
                ]
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    let result = evaluate_node(&designer, "main", fold_id);
    assert!(
        matches!(result, NetworkResult::Error(_)),
        "expected Error, got {}",
        result.to_display_string()
    );
}

// ============================================================================
// Evaluation: partial application of trailing params (pre-bound factor)
// ============================================================================

#[test]
fn test_fold_combine_with_prebound_factor() {
    let mut designer = setup_designer_with_network("main");

    // factor is captured into the closure once; combine(acc, elem) uses both
    // its iteration variables (acc, elem) AND the captured `factor`.
    // f(acc, elem) = acc + factor*elem; init=0; xs=[1,2,3]; factor=10
    // ⇒ 0+10*1=10, 10+10*2=30, 30+10*3=60.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 3 }
            i = int { value: 0 }
            f10 = int { value: 10 }
            combine = expr {
                expression: "acc + factor * elem",
                parameters: [
                    { name: "acc", data_type: Int },
                    { name: "elem", data_type: Int },
                    { name: "factor", data_type: Int }
                ],
                factor: f10
            }
            fld = fold { element_type: Int, accumulator_type: Int, xs: r, init: i, f: @combine }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let fold_id = find_node_id(&designer, "main", "fold");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 60);
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
// Text format roundtrip (serialize_network → edit_network)
// ============================================================================

#[test]
fn test_fold_text_format_roundtrip_matching_types() {
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, OutputPinDefinition};
    use rust_lib_flutter_cad::structure_designer::text_format::{edit_network, serialize_network};

    let registry = NodeTypeRegistry::new();

    let create_network = || {
        let node_type = NodeType {
            name: "test".to_string(),
            description: "Test network".to_string(),
            summary: None,
            category: NodeTypeCategory::Custom,
            parameters: vec![],
            output_pins: OutputPinDefinition::single(DataType::Int),
            public: true,
            node_data_creator: || {
                Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {})
            },
            node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
            node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
        };
        NodeNetwork::new(node_type)
    };

    let mut network = create_network();
    let result = edit_network(
        &mut network,
        &registry,
        r#"
            fld = fold { element_type: Int, accumulator_type: Int }
            output fld
        "#,
        true,
    );
    assert!(
        result.success,
        "Initial edit should succeed: {:?}",
        result.errors
    );

    let f_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "fold")
        .unwrap();
    let props: HashMap<String, TextValue> = f_node.data.get_text_properties().into_iter().collect();
    assert_eq!(
        props.get("element_type"),
        Some(&TextValue::DataType(DataType::Int))
    );
    assert_eq!(
        props.get("accumulator_type"),
        Some(&TextValue::DataType(DataType::Int))
    );

    let serialized = serialize_network(&network, &registry, None);
    assert!(
        serialized.contains("fold"),
        "serialized text should contain 'fold'"
    );
    assert!(
        serialized.contains("element_type: Int"),
        "serialized text should contain element_type: {}",
        serialized
    );
    assert!(
        serialized.contains("accumulator_type: Int"),
        "serialized text should contain accumulator_type: {}",
        serialized
    );

    let mut network2 = create_network();
    let result2 = edit_network(&mut network2, &registry, &serialized, true);
    assert!(
        result2.success,
        "Roundtrip edit should succeed: {:?}",
        result2.errors
    );
    assert_eq!(
        network.nodes.len(),
        network2.nodes.len(),
        "networks should have same number of nodes"
    );
}

#[test]
fn test_fold_text_format_roundtrip_cross_type() {
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, OutputPinDefinition};
    use rust_lib_flutter_cad::structure_designer::text_format::{edit_network, serialize_network};

    let registry = NodeTypeRegistry::new();

    let create_network = || {
        let node_type = NodeType {
            name: "test".to_string(),
            description: "Test network".to_string(),
            summary: None,
            category: NodeTypeCategory::Custom,
            parameters: vec![],
            output_pins: OutputPinDefinition::single(DataType::Int),
            public: true,
            node_data_creator: || {
                Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {})
            },
            node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
            node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
        };
        NodeNetwork::new(node_type)
    };

    let mut network = create_network();
    let result = edit_network(
        &mut network,
        &registry,
        r#"
            fld = fold { element_type: IVec3, accumulator_type: Int }
            output fld
        "#,
        true,
    );
    assert!(
        result.success,
        "Initial edit should succeed: {:?}",
        result.errors
    );

    let serialized = serialize_network(&network, &registry, None);
    assert!(
        serialized.contains("element_type: IVec3"),
        "serialized text should contain element_type: {}",
        serialized
    );
    assert!(
        serialized.contains("accumulator_type: Int"),
        "serialized text should contain accumulator_type: {}",
        serialized
    );

    let mut network2 = create_network();
    let result2 = edit_network(&mut network2, &registry, &serialized, true);
    assert!(
        result2.success,
        "Roundtrip edit should succeed: {:?}",
        result2.errors
    );
    assert_eq!(network.nodes.len(), network2.nodes.len());

    let f_node = network2
        .nodes
        .values()
        .find(|n| n.node_type_name == "fold")
        .unwrap();
    let props: HashMap<String, TextValue> = f_node.data.get_text_properties().into_iter().collect();
    assert_eq!(
        props.get("element_type"),
        Some(&TextValue::DataType(DataType::IVec3))
    );
    assert_eq!(
        props.get("accumulator_type"),
        Some(&TextValue::DataType(DataType::Int))
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
