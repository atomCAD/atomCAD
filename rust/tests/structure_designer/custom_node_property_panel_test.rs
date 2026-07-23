//! Tests for the custom-node property panel backend (Phase 1).
//!
//! Covers `StructureDesigner::resolve_parameter_default` — the new core helper
//! that resolves a custom node's parameter `default` pin in isolation — and the
//! `CustomNodeData.literal_values` storage path that the panel's
//! `set_custom_node_literal` / `clear_custom_node_literal` FFI functions drive
//! through `set_node_network_data`.

use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::CustomNodeData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

/// Build a subnetwork "Inner" with a single Int parameter. `add_node`
/// auto-names the first parameter "param0". When `connect_default` is true,
/// the parameter node's `default` input pin (argument 0) is wired to an `int`
/// node holding `default_value`.
fn build_inner_subnetwork(
    designer: &mut StructureDesigner,
    connect_default: bool,
    default_value: i32,
) -> u64 {
    designer.add_node_network("Inner");
    designer.set_active_node_network_name(Some("Inner".to_string()));

    let param_id = designer.add_node("parameter", DVec2::new(200.0, 0.0));

    if connect_default {
        let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));
        designer.set_node_network_data(
            int_id,
            Box::new(IntData {
                value: default_value,
            }),
        );
        // parameter node's only input pin (index 0) is `default`.
        designer.connect_nodes(int_id, 0, param_id, 0);
    }

    designer.set_return_node_id(Some(param_id));
    designer.validate_active_network();
    param_id
}

/// `NetworkResult` does not implement `Debug`; render it for assert messages.
fn describe(result: &Option<NetworkResult>) -> String {
    match result {
        Some(r) => r.to_display_string(),
        None => "None".to_string(),
    }
}

fn evaluate_pin0(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&stack, node_id, 0, registry, false, &mut context)
}

#[test]
fn resolve_parameter_default_returns_connected_default_pin_value() {
    let mut designer = StructureDesigner::new();
    build_inner_subnetwork(&mut designer, true, 7);

    let resolved = designer.resolve_parameter_default("Inner", "param0");
    assert!(
        matches!(resolved, Some(NetworkResult::Int(7))),
        "expected the connected default pin (int = 7), got {}",
        describe(&resolved)
    );
}

#[test]
fn resolve_parameter_default_unconnected_default_pin_is_not_a_simple_value() {
    let mut designer = StructureDesigner::new();
    build_inner_subnetwork(&mut designer, false, 0);

    // An unconnected default pin yields no usable value — the getter rejects
    // it and the panel falls back to the type-zero placeholder. We only assert
    // it is not a simple Int, which is all `network_result_to_api_literal`
    // needs to reject it.
    let resolved = designer.resolve_parameter_default("Inner", "param0");
    assert!(
        !matches!(resolved, Some(NetworkResult::Int(_))),
        "an unconnected default pin must not resolve to an Int, got {}",
        describe(&resolved)
    );
}

#[test]
fn resolve_parameter_default_returns_none_for_missing_subnetwork_or_param() {
    let mut designer = StructureDesigner::new();
    build_inner_subnetwork(&mut designer, true, 7);

    assert!(
        designer
            .resolve_parameter_default("NoSuchNetwork", "param0")
            .is_none(),
        "missing subnetwork must resolve to None"
    );
    assert!(
        designer
            .resolve_parameter_default("Inner", "no_such_param")
            .is_none(),
        "missing parameter name must resolve to None"
    );
}

#[test]
fn stored_literal_overrides_default_pin_at_eval() {
    let mut designer = StructureDesigner::new();
    build_inner_subnetwork(&mut designer, true, 7);

    // "main" with one unwired "Inner" call site.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let call_id = designer.add_node("Inner", DVec2::new(0.0, 0.0));
    designer.validate_active_network();

    // No literal stored: the call site evaluates to the default pin's value.
    assert!(
        matches!(
            evaluate_pin0(&designer, "main", call_id),
            NetworkResult::Int(7)
        ),
        "with no stored literal the call site must use the default pin (7)"
    );

    // Store a literal — the path `set_custom_node_literal` drives: clone the
    // node's CustomNodeData, mutate the map, push it back via
    // `set_node_network_data`.
    let mut data = designer
        .get_active_node_network()
        .and_then(|n| n.nodes.get(&call_id))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<CustomNodeData>())
        .cloned()
        .expect("an Inner call node must carry CustomNodeData");
    data.literal_values
        .insert("param0".to_string(), TextValue::Int(20));
    designer.set_node_network_data(call_id, Box::new(data));

    assert!(
        matches!(
            evaluate_pin0(&designer, "main", call_id),
            NetworkResult::Int(20)
        ),
        "a stored literal must override the default pin"
    );

    // Clear the literal — the call site falls back to the default pin again.
    let mut data = designer
        .get_active_node_network()
        .and_then(|n| n.nodes.get(&call_id))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<CustomNodeData>())
        .cloned()
        .expect("CustomNodeData must still be present");
    data.literal_values.remove("param0");
    designer.set_node_network_data(call_id, Box::new(data));

    assert!(
        matches!(
            evaluate_pin0(&designer, "main", call_id),
            NetworkResult::Int(7)
        ),
        "clearing the literal must restore the default pin value"
    );
}

#[test]
fn custom_node_literal_survives_save_load_roundtrip() {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };

    let mut designer = StructureDesigner::new();
    build_inner_subnetwork(&mut designer, true, 7);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let call_id = designer.add_node("Inner", DVec2::new(0.0, 0.0));
    designer.validate_active_network();

    // Store a literal on the call site.
    let mut data = designer
        .get_active_node_network()
        .and_then(|n| n.nodes.get(&call_id))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<CustomNodeData>())
        .cloned()
        .expect("an Inner call node must carry CustomNodeData");
    data.literal_values
        .insert("param0".to_string(), TextValue::Int(20));
    designer.set_node_network_data(call_id, Box::new(data));

    // Save to a temp .cnnd and reload into a fresh registry.
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let temp_file = temp_dir.path().join("roundtrip.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &temp_file,
        false,
        &std::collections::HashMap::new(),
    )
    .expect("save failed");

    let mut registry2 =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_file.to_str().unwrap()).expect("load failed");

    let main2 = registry2.node_networks.get("main").expect("main missing");
    let call2 = main2.nodes.get(&call_id).expect("call node missing");

    // The reloaded custom node must still carry CustomNodeData with the literal.
    let custom_data = call2.data.as_any_ref().downcast_ref::<CustomNodeData>();
    assert!(
        custom_data.is_some(),
        "reloaded custom node lost its CustomNodeData (data_type serialized as no_data?)"
    );
    assert_eq!(
        custom_data.unwrap().literal_values.get("param0"),
        Some(&TextValue::Int(20)),
        "stored literal value must survive save/load"
    );
}

/// Legacy/public `.cnnd` files (saved before custom-node literal persistence
/// existed) wrote custom node instances with `data_type: "no_data"`. Their
/// literal values are gone for good, but the node must still load as an
/// (empty) `CustomNodeData` — not `NoData` — so its parameters stay editable
/// after load. We simulate a legacy file by saving a normal one and rewriting
/// the custom node's `data_type` back to `"no_data"` on disk before reloading.
#[test]
fn legacy_no_data_custom_node_loads_as_editable_custom_node_data() {
    use rust_lib_flutter_cad::structure_designer::node_data::CustomNodeData;
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };

    let mut designer = StructureDesigner::new();
    build_inner_subnetwork(&mut designer, true, 7);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let call_id = designer.add_node("Inner", DVec2::new(0.0, 0.0));
    designer.validate_active_network();

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let temp_file = temp_dir.path().join("legacy.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &temp_file,
        false,
        &std::collections::HashMap::new(),
    )
    .expect("save failed");

    // Rewrite every "custom_node" data_type back to "no_data" to mimic a file
    // written by the pre-fix serializer.
    let json = std::fs::read_to_string(&temp_file).expect("read");
    let json = json.replace("\"custom_node\"", "\"no_data\"");
    assert!(
        json.contains("\"no_data\""),
        "test setup: expected a custom node to rewrite to no_data"
    );
    std::fs::write(&temp_file, json).expect("write");

    let mut registry2 =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_file.to_str().unwrap()).expect("load failed");

    let main2 = registry2.node_networks.get("main").expect("main missing");
    let call2 = main2.nodes.get(&call_id).expect("call node missing");
    assert!(
        call2
            .data
            .as_any_ref()
            .downcast_ref::<CustomNodeData>()
            .is_some(),
        "a legacy no_data custom node must load as editable CustomNodeData, not NoData"
    );
}
