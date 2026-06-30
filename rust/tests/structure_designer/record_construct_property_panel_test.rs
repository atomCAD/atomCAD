//! Phase 1 tests for the `record_construct` property panel backend
//! (`doc/design_record_construct_property_panel.md`).
//!
//! Covers the `wired > literal > pass-None-through` priority added to
//! `record_construct.rs::eval` and the `RecordConstructData.literal_values`
//! storage path that the panel's `set_record_construct_literal` /
//! `clear_record_construct_literal` FFI functions drive through
//! `set_node_network_data`.
//!
//! The FFI getter / setter / clear wrappers themselves are thin
//! `&self` / `clone+mutate+set_node_network_data` shells that route through
//! the global `CAD_INSTANCE` — they are exercised end-to-end via Flutter
//! integration tests in Phase 2. Here we cover the core eval semantics and
//! the storage path directly against `StructureDesigner` so the
//! priority-and-coercion logic is verifiable in isolation.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

// ---------------------------------------------------------------------------
// Helpers (mirror the patterns used in record_types_phase3_test.rs)
// ---------------------------------------------------------------------------

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn evaluate_node_pin0(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> NetworkResult {
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
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

/// Authored order [y, x] — non-alphabetical on purpose, to exercise the
/// distinction between authored pin order and canonical storage order.
fn point_def() -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        "Point".to_string(),
        vec![
            ("y".to_string(), DataType::Int),
            ("x".to_string(), DataType::Int),
        ],
    )
}

/// Returns the value of the named field from a `NetworkResult::Record`,
/// panicking if the result is not a record or the field is missing.
fn record_field(result: &NetworkResult, name: &str) -> NetworkResult {
    let NetworkResult::Record(fields) = result else {
        panic!("expected Record, got {:?}", result.to_display_string());
    };
    fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| panic!("missing field `{}` in record", name))
}

// ---------------------------------------------------------------------------
// Eval branch: wired > literal > None
// ---------------------------------------------------------------------------

#[test]
fn unwired_field_uses_stored_literal() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let construct = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    let mut data = RecordConstructData {
        schema: "Point".to_string(),
        ..Default::default()
    };
    data.literal_values
        .insert("y".to_string(), TextValue::Int(11));
    data.literal_values
        .insert("x".to_string(), TextValue::Int(22));
    set_node_data(&mut designer, "test", construct, Box::new(data));
    designer.validate_active_network();

    let result = evaluate_node_pin0(&designer, "test", construct);
    assert!(
        matches!(record_field(&result, "y"), NetworkResult::Int(11)),
        "stored y literal should drive eval, got record = {}",
        result.to_display_string()
    );
    assert!(
        matches!(record_field(&result, "x"), NetworkResult::Int(22)),
        "stored x literal should drive eval, got record = {}",
        result.to_display_string()
    );
}

#[test]
fn wired_pin_overrides_stored_literal() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let int_node = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        int_node,
        Box::new(IntData { value: 99 }),
    );

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    let mut data = RecordConstructData {
        schema: "Point".to_string(),
        ..Default::default()
    };
    // Stored literals for both fields — both will be replaced by the
    // wire on y (pin 0) but the stored x literal still drives eval.
    data.literal_values
        .insert("y".to_string(), TextValue::Int(11));
    data.literal_values
        .insert("x".to_string(), TextValue::Int(22));
    set_node_data(&mut designer, "test", construct, Box::new(data));
    designer.validate_active_network();

    // Wire the int node into the `y` pin (authored index 0).
    designer.connect_nodes(int_node, 0, construct, 0);

    let result = evaluate_node_pin0(&designer, "test", construct);
    assert!(
        matches!(record_field(&result, "y"), NetworkResult::Int(99)),
        "wired y should override stored literal (11), got {}",
        result.to_display_string()
    );
    assert!(
        matches!(record_field(&result, "x"), NetworkResult::Int(22)),
        "unwired x should still use stored literal (22), got {}",
        result.to_display_string()
    );
}

#[test]
fn unwired_field_without_literal_short_circuits_to_none() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let construct = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    let mut data = RecordConstructData {
        schema: "Point".to_string(),
        ..Default::default()
    };
    // Only `y` has a stored literal; `x` is unwired and unstored — the
    // record should short-circuit to None.
    data.literal_values
        .insert("y".to_string(), TextValue::Int(11));
    set_node_data(&mut designer, "test", construct, Box::new(data));
    designer.validate_active_network();

    let result = evaluate_node_pin0(&designer, "test", construct);
    assert!(
        matches!(result, NetworkResult::None),
        "expected None, got {}",
        result.to_display_string()
    );
}

#[test]
fn type_mismatched_stored_literal_falls_back_to_none() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let construct = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    let mut data = RecordConstructData {
        schema: "Point".to_string(),
        ..Default::default()
    };
    // Field `y` is Int in the def; a stored String can't coerce to Int,
    // so the field falls through to `evaluate_arg` on the unwired pin,
    // yielding None and short-circuiting the record.
    data.literal_values
        .insert("y".to_string(), TextValue::String("not an int".to_string()));
    data.literal_values
        .insert("x".to_string(), TextValue::Int(22));
    set_node_data(&mut designer, "test", construct, Box::new(data));
    designer.validate_active_network();

    let result = evaluate_node_pin0(&designer, "test", construct);
    assert!(
        matches!(result, NetworkResult::None),
        "mismatched stored literal should fall back to None, got {}",
        result.to_display_string()
    );
}

#[test]
fn orphan_literal_entry_is_inert() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let construct = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    let mut data = RecordConstructData {
        schema: "Point".to_string(),
        ..Default::default()
    };
    // `z` is not a field of Point — the entry must be ignored, not produce
    // an extra record field or interfere with eval.
    data.literal_values
        .insert("y".to_string(), TextValue::Int(11));
    data.literal_values
        .insert("x".to_string(), TextValue::Int(22));
    data.literal_values
        .insert("z".to_string(), TextValue::Int(999));
    set_node_data(&mut designer, "test", construct, Box::new(data));
    designer.validate_active_network();

    let result = evaluate_node_pin0(&designer, "test", construct);
    let NetworkResult::Record(fields) = &result else {
        panic!("expected Record, got {}", result.to_display_string());
    };
    let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        vec!["x", "y"],
        "orphan `z` entry must not appear in the record"
    );
    assert!(matches!(record_field(&result, "y"), NetworkResult::Int(11)));
    assert!(matches!(record_field(&result, "x"), NetworkResult::Int(22)));
}

#[test]
fn int_to_float_coercion_through_to_network_result() {
    // Use a def whose field type is Float; store an Int literal and verify
    // the eval branch's `to_network_result` coerces it (Int → Float).
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "FRecord".to_string(),
            vec![("f".to_string(), DataType::Float)],
        ))
        .unwrap();

    let construct = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    let mut data = RecordConstructData {
        schema: "FRecord".to_string(),
        ..Default::default()
    };
    data.literal_values
        .insert("f".to_string(), TextValue::Int(7));
    set_node_data(&mut designer, "test", construct, Box::new(data));
    designer.validate_active_network();

    let result = evaluate_node_pin0(&designer, "test", construct);
    let f = record_field(&result, "f");
    let NetworkResult::Float(v) = f else {
        panic!("expected Float, got {}", f.to_display_string());
    };
    assert!((v - 7.0).abs() < 1e-12, "expected 7.0, got {}", v);
}

// ---------------------------------------------------------------------------
// Storage round-trip — exercises the same path the FFI setter/clear uses
// ---------------------------------------------------------------------------

#[test]
fn set_literal_round_trips_through_set_node_network_data() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let construct = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
            ..Default::default()
        }),
    );
    designer.validate_active_network();

    // Drive the same "clone + mutate + set_node_network_data" path that the
    // FFI `set_record_construct_literal` uses.
    let mut data = designer
        .get_active_node_network()
        .and_then(|n| n.nodes.get(&construct))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<RecordConstructData>())
        .cloned()
        .expect("record_construct must carry RecordConstructData");
    data.literal_values
        .insert("x".to_string(), TextValue::Int(5));
    data.literal_values
        .insert("y".to_string(), TextValue::Int(8));
    designer.set_node_network_data(construct, Box::new(data));

    let result = evaluate_node_pin0(&designer, "test", construct);
    assert!(matches!(record_field(&result, "x"), NetworkResult::Int(5)));
    assert!(matches!(record_field(&result, "y"), NetworkResult::Int(8)));

    // Clear one — same shape as `clear_record_construct_literal`.
    let mut data = designer
        .get_active_node_network()
        .and_then(|n| n.nodes.get(&construct))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<RecordConstructData>())
        .cloned()
        .expect("data must still be present");
    data.literal_values.remove("x");
    designer.set_node_network_data(construct, Box::new(data));

    // Removing the only contributor to x makes the whole record None
    // (x is unwired and now unstored — short-circuits per
    // `unwired_field_without_literal_short_circuits_to_none`).
    let result = evaluate_node_pin0(&designer, "test", construct);
    assert!(
        matches!(result, NetworkResult::None),
        "expected None after clearing the only contributor to x, got {}",
        result.to_display_string()
    );
}

// ---------------------------------------------------------------------------
// Schema property writes preserve literal_values
// ---------------------------------------------------------------------------

#[test]
fn schema_change_preserves_literal_values_for_matching_fields() {
    // Stored entries for fields that *also* exist on the new schema must
    // survive the schema switch. This is exercised through the same
    // "clone preserving literal_values + set_node_network_data" path the
    // FFI `set_record_construct_data` uses (see structure_designer_api.rs).
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "PointPlusZ".to_string(),
            vec![
                ("y".to_string(), DataType::Int),
                ("x".to_string(), DataType::Int),
                ("z".to_string(), DataType::Int),
            ],
        ))
        .unwrap();

    let construct = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    let mut data = RecordConstructData {
        schema: "Point".to_string(),
        ..Default::default()
    };
    data.literal_values
        .insert("y".to_string(), TextValue::Int(11));
    data.literal_values
        .insert("x".to_string(), TextValue::Int(22));
    set_node_data(&mut designer, "test", construct, Box::new(data));
    designer.validate_active_network();

    // Schema switch, preserving the literal_values map.
    let existing = designer
        .get_active_node_network()
        .and_then(|n| n.nodes.get(&construct))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<RecordConstructData>())
        .cloned()
        .expect("record_construct data");
    designer.set_node_network_data(
        construct,
        Box::new(RecordConstructData {
            schema: "PointPlusZ".to_string(),
            literal_values: existing.literal_values,
        }),
    );

    // y and x stored literals carry over; z is unwired and unstored, so
    // the record short-circuits to None — exactly the inert-orphan
    // semantics under the new schema where the old `Point` fields still
    // match by name.
    let result = evaluate_node_pin0(&designer, "test", construct);
    assert!(
        matches!(result, NetworkResult::None),
        "expected None (z unset), got {}",
        result.to_display_string()
    );

    // Add z, all three fields fill in from stored literals.
    let mut data = designer
        .get_active_node_network()
        .and_then(|n| n.nodes.get(&construct))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<RecordConstructData>())
        .cloned()
        .expect("data");
    data.literal_values
        .insert("z".to_string(), TextValue::Int(33));
    designer.set_node_network_data(construct, Box::new(data));

    let result = evaluate_node_pin0(&designer, "test", construct);
    assert!(matches!(record_field(&result, "y"), NetworkResult::Int(11)));
    assert!(matches!(record_field(&result, "x"), NetworkResult::Int(22)));
    assert!(matches!(record_field(&result, "z"), NetworkResult::Int(33)));
}
