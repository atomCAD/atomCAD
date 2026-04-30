use glam::f64::DVec2;
use glam::i32::IVec3;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::filter::FilterData;
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

fn extract_int_array(result: NetworkResult) -> Vec<i32> {
    match result {
        NetworkResult::Array(items) => items
            .into_iter()
            .map(|r| match r {
                NetworkResult::Int(v) => v,
                other => panic!("expected Int element, got {}", other.to_display_string()),
            })
            .collect(),
        other => panic!("expected Array, got {}", other.to_display_string()),
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
    assert_eq!(nt.parameters.len(), 2);
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.parameters[1].name, "f");
    assert_eq!(nt.output_pins.len(), 1);
    assert_eq!(
        *nt.output_type(),
        DataType::Array(Box::new(DataType::Float))
    );
}

// ============================================================================
// calculate_custom_node_type tests
// ============================================================================

#[test]
fn test_filter_custom_type_int() {
    use rust_lib_flutter_cad::structure_designer::data_type::FunctionType;

    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("filter").unwrap();
    let data = FilterData {
        element_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Array(Box::new(DataType::Int))
    );
    assert_eq!(
        custom.parameters[1].data_type,
        DataType::Function(FunctionType {
            parameter_types: vec![DataType::Int],
            output_type: Box::new(DataType::Bool),
        })
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Array(Box::new(DataType::Int))
    );
}

#[test]
fn test_filter_custom_type_ivec3() {
    use rust_lib_flutter_cad::structure_designer::data_type::FunctionType;

    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("filter").unwrap();
    let data = FilterData {
        element_type: DataType::IVec3,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Array(Box::new(DataType::IVec3))
    );
    assert_eq!(
        custom.parameters[1].data_type,
        DataType::Function(FunctionType {
            parameter_types: vec![DataType::IVec3],
            output_type: Box::new(DataType::Bool),
        })
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Array(Box::new(DataType::IVec3))
    );
}

// ============================================================================
// Evaluation: basic predicate filtering (Int)
// ============================================================================

#[test]
fn test_filter_int_greater_than() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 5 }
            pred = expr {
                expression: "x > 2",
                parameters: [{ name: "x", data_type: Int }]
            }
            f1 = filter { element_type: Int, xs: r, f: @pred }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    let values = extract_int_array(evaluate_node(&designer, "main", filter_id));
    assert_eq!(values, vec![3, 4, 5]);
}

#[test]
fn test_filter_int_even_predicate() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 4 }
            pred = expr {
                expression: "x % 2 == 0",
                parameters: [{ name: "x", data_type: Int }]
            }
            f1 = filter { element_type: Int, xs: r, f: @pred }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    let values = extract_int_array(evaluate_node(&designer, "main", filter_id));
    assert_eq!(values, vec![2, 4]);
}

#[test]
fn test_filter_always_true_keeps_all() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 3 }
            pred = expr {
                expression: "true",
                parameters: [{ name: "x", data_type: Int }]
            }
            f1 = filter { element_type: Int, xs: r, f: @pred }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    let values = extract_int_array(evaluate_node(&designer, "main", filter_id));
    assert_eq!(values, vec![1, 2, 3]);
}

#[test]
fn test_filter_always_false_yields_empty() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 3 }
            pred = expr {
                expression: "false",
                parameters: [{ name: "x", data_type: Int }]
            }
            f1 = filter { element_type: Int, xs: r, f: @pred }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    let values = extract_int_array(evaluate_node(&designer, "main", filter_id));
    assert!(values.is_empty(), "expected empty array, got {:?}", values);
}

#[test]
fn test_filter_empty_array_input() {
    let mut designer = setup_designer_with_network("main");

    // range with count: 0 produces an empty Array[Int].
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 0, step: 1, count: 0 }
            pred = expr {
                expression: "x > 0",
                parameters: [{ name: "x", data_type: Int }]
            }
            f1 = filter { element_type: Int, xs: r, f: @pred }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    let values = extract_int_array(evaluate_node(&designer, "main", filter_id));
    assert!(values.is_empty(), "expected empty array, got {:?}", values);
}

// ============================================================================
// Evaluation: IVec3 element type (.z > 0 predicate)
// ============================================================================

#[test]
fn test_filter_ivec3_z_positive() {
    use rust_lib_flutter_cad::structure_designer::data_type::DataType;
    use rust_lib_flutter_cad::structure_designer::nodes::ivec3::IVec3Data;
    use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;

    let mut designer = setup_designer_with_network("main");

    // Build [(0,0,1), (1,0,-1), (2,2,2), (0,0,0)] via four ivec3 nodes + sequence.
    let mut id_for = |x: i32, y: i32, z: i32, pos_y: f64| {
        let id = designer.add_node("ivec3", DVec2::new(0.0, pos_y));
        set_node_data(
            &mut designer,
            "main",
            id,
            Box::new(IVec3Data {
                value: IVec3::new(x, y, z),
            }),
        );
        id
    };
    let v0 = id_for(0, 0, 1, 0.0);
    let v1 = id_for(1, 0, -1, 60.0);
    let v2 = id_for(2, 2, 2, 120.0);
    let v3 = id_for(0, 0, 0, 180.0);

    let seq_id = designer.add_node("sequence", DVec2::new(120.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        seq_id,
        Box::new(SequenceData {
            element_type: DataType::IVec3,
            input_count: 4,
        }),
    );
    designer.validate_active_network();
    designer.connect_nodes(v0, 0, seq_id, 0);
    designer.connect_nodes(v1, 0, seq_id, 1);
    designer.connect_nodes(v2, 0, seq_id, 2);
    designer.connect_nodes(v3, 0, seq_id, 3);

    // Now use text format to add the predicate and filter, wired to the existing sequence.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            pred = expr {
                expression: "x.z > 0",
                parameters: [{ name: "x", data_type: IVec3 }]
            }
            f1 = filter { element_type: IVec3, xs: sequence1, f: @pred }
        "#,
        false,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    // Look up `f1` by name — sequence node was renamed to `sequence1` by serializer/editor;
    // simpler to find via node_type_name.
    let filter_id = find_node_id(&designer, "main", "filter");
    let result = evaluate_node(&designer, "main", filter_id);
    match result {
        NetworkResult::Array(items) => {
            let vecs: Vec<IVec3> = items
                .into_iter()
                .map(|r| match r {
                    NetworkResult::IVec3(v) => v,
                    other => panic!("expected IVec3, got {}", other.to_display_string()),
                })
                .collect();
            assert_eq!(vecs, vec![IVec3::new(0, 0, 1), IVec3::new(2, 2, 2)]);
        }
        other => panic!("expected Array, got {}", other.to_display_string()),
    }
}

// ============================================================================
// Evaluation: missing-input errors
// ============================================================================

#[test]
fn test_filter_xs_unconnected_yields_error() {
    let mut designer = setup_designer_with_network("main");

    // Wire f, leave xs unconnected.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            pred = expr {
                expression: "x > 0",
                parameters: [{ name: "x", data_type: Int }]
            }
            f1 = filter { element_type: Int, f: @pred }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    expect_error(
        evaluate_node(&designer, "main", filter_id),
        "xs input is missing",
    );
}

#[test]
fn test_filter_f_unconnected_yields_error() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 3 }
            f1 = filter { element_type: Int, xs: r }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    expect_error(
        evaluate_node(&designer, "main", filter_id),
        "f input is missing",
    );
}

#[test]
fn test_filter_both_unconnected_reports_xs_first() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            f1 = filter { element_type: Int }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    expect_error(
        evaluate_node(&designer, "main", filter_id),
        "xs input is missing",
    );
}

#[test]
fn test_filter_empty_xs_with_unconnected_f_still_errors() {
    let mut designer = setup_designer_with_network("main");

    // xs is wired (empty array), but f is not — required-input check must fire
    // even when xs would have been empty.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 0, step: 1, count: 0 }
            f1 = filter { element_type: Int, xs: r }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    expect_error(
        evaluate_node(&designer, "main", filter_id),
        "f input is missing",
    );
}

// ============================================================================
// Evaluation: predicate error mid-iteration propagates
// ============================================================================

#[test]
fn test_filter_predicate_error_propagates() {
    // Predicate references a captured array via array_at with an out-of-bounds
    // index for some elements: when `x == 2`, `arr[5]` is out of bounds and
    // expr evaluation produces an error which filter must propagate.
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 4 }
            pred = expr {
                expression: "x > 0 && [10, 20][x] > 0",
                parameters: [{ name: "x", data_type: Int }]
            }
            f1 = filter { element_type: Int, xs: r, f: @pred }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    let result = evaluate_node(&designer, "main", filter_id);
    assert!(
        matches!(result, NetworkResult::Error(_)),
        "expected error, got {}",
        result.to_display_string()
    );
}

// ============================================================================
// Evaluation: partial application of trailing params (pre-bound threshold)
// ============================================================================

#[test]
fn test_filter_predicate_with_prebound_threshold() {
    let mut designer = setup_designer_with_network("main");

    // The predicate's first parameter is the iteration variable; the second
    // (`threshold`) is wired to a constant inside the parent network. The
    // closure captures `threshold` once at wire-time.
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 1, step: 1, count: 5 }
            t = int { value: 3 }
            pred = expr {
                expression: "x > threshold",
                parameters: [
                    { name: "x", data_type: Int },
                    { name: "threshold", data_type: Int }
                ],
                threshold: t
            }
            f1 = filter { element_type: Int, xs: r, f: @pred }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let filter_id = find_node_id(&designer, "main", "filter");
    let values = extract_int_array(evaluate_node(&designer, "main", filter_id));
    assert_eq!(values, vec![4, 5]);
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
// Text format roundtrip (serialize_network → edit_network)
// ============================================================================

#[test]
fn test_filter_text_format_roundtrip() {
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
            output_pins: OutputPinDefinition::single(DataType::Array(Box::new(DataType::IVec3))),
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
            f1 = filter { element_type: IVec3 }
            output f1
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
        .find(|n| n.node_type_name == "filter")
        .unwrap();
    let props: HashMap<String, TextValue> = f_node.data.get_text_properties().into_iter().collect();
    assert_eq!(
        props.get("element_type"),
        Some(&TextValue::DataType(DataType::IVec3))
    );

    let serialized = serialize_network(&network, &registry, None);
    assert!(
        serialized.contains("filter"),
        "serialized text should contain 'filter'"
    );
    assert!(
        serialized.contains("element_type: IVec3"),
        "serialized text should contain element_type: {}",
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
