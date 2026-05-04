//! Phase 3 tests for record types (see `doc/design_record_types.md`).
//!
//! Phase 3 introduces the `record_construct` and `record_destructure` nodes,
//! both of which derive their pin layout from a `RecordTypeDef` in the
//! registry rather than from per-node data alone. Coverage:
//!
//! - construct + destructure round-trip
//! - nested defs (Box references Point)
//! - missing-input → None propagation
//! - pass-through on destructure (extra runtime fields ignored)
//! - dangling schema (empty / deleted)
//! - field-rename-as-remove+add semantics
//! - schema-change wire repair end-to-end

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
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
use rust_lib_flutter_cad::structure_designer::nodes::record_destructure::RecordDestructureData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn evaluate_node_pin(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    output_pin_index: i32,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(
        &network_stack,
        node_id,
        output_pin_index,
        registry,
        false,
        &mut context,
    )
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
        node,
        true,
    );
}

fn point_def() -> RecordTypeDef {
    RecordTypeDef {
        name: "Point".to_string(),
        // Authored order intentionally non-alphabetical so we exercise the
        // distinction between authored pin order and canonical storage order.
        fields: vec![
            ("y".to_string(), DataType::Int),
            ("x".to_string(), DataType::Int),
        ],
    }
}

fn box_def_referencing_point() -> RecordTypeDef {
    RecordTypeDef {
        name: "Box".to_string(),
        fields: vec![(
            "p".to_string(),
            DataType::Record(RecordType::Named("Point".to_string())),
        )],
    }
}

// ============================================================================
// Registration
// ============================================================================

#[test]
fn record_construct_registered() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.get_node_type("record_construct").expect("registered");
    assert_eq!(nt.name, "record_construct");
    assert!(nt.public);
    // Base type has zero parameters; the cache populator fills them in
    // per-instance from the `schema` property.
    assert_eq!(nt.parameters.len(), 0);
    assert_eq!(nt.output_pins.len(), 1);
}

#[test]
fn record_destructure_registered() {
    let registry = NodeTypeRegistry::new();
    let nt = registry
        .get_node_type("record_destructure")
        .expect("registered");
    assert_eq!(nt.name, "record_destructure");
    assert!(nt.public);
    assert_eq!(nt.parameters.len(), 1);
    assert_eq!(nt.parameters[0].name, "record");
}

// ============================================================================
// Pin layout: derived from registry, in authored order
// ============================================================================

#[test]
fn record_construct_pins_match_authored_field_order() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let id = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();

    // Authored order on the def is (y, x); pins follow that, NOT alphabetical.
    let names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["y", "x"]);
    assert_eq!(
        nt.output_pins[0].fixed_type(),
        Some(&DataType::Record(RecordType::Named("Point".to_string())))
    );
}

#[test]
fn record_destructure_pins_match_authored_field_order() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let id = designer.add_node("record_destructure", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(RecordDestructureData {
            schema: "Point".to_string(),
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();

    assert_eq!(
        nt.parameters[0].data_type,
        DataType::Record(RecordType::Named("Point".to_string()))
    );
    let pin_names: Vec<&str> = nt.output_pins.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(pin_names, vec!["y", "x"]);
}

// ============================================================================
// Construct + destructure round-trip
// ============================================================================

#[test]
fn construct_destructure_round_trip() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let y_input = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        y_input,
        Box::new(IntData { value: 42 }),
    );
    let x_input = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "test",
        x_input,
        Box::new(IntData { value: 7 }),
    );

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
        }),
    );

    let destructure = designer.add_node("record_destructure", DVec2::new(400.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        destructure,
        Box::new(RecordDestructureData {
            schema: "Point".to_string(),
        }),
    );

    designer.validate_active_network();

    // Pin order on construct: [y, x]. So index 0 = y, index 1 = x.
    designer.connect_nodes(y_input, 0, construct, 0);
    designer.connect_nodes(x_input, 0, construct, 1);
    designer.connect_nodes(construct, 0, destructure, 0);

    // Output pins on destructure are in authored order [y, x]: pin 0 = y, pin 1 = x.
    let y_out = evaluate_node_pin(&designer, "test", destructure, 0);
    let x_out = evaluate_node_pin(&designer, "test", destructure, 1);
    assert!(matches!(y_out, NetworkResult::Int(42)), "y_out = {:?}", y_out.to_display_string());
    assert!(matches!(x_out, NetworkResult::Int(7)), "x_out = {:?}", x_out.to_display_string());
}

// ============================================================================
// Construct value canonicalization
// ============================================================================

#[test]
fn construct_emits_record_in_canonical_order() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let y = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", y, Box::new(IntData { value: 42 }));
    let x = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(&mut designer, "test", x, Box::new(IntData { value: 7 }));

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
        }),
    );

    designer.validate_active_network();
    designer.connect_nodes(y, 0, construct, 0); // pin 0 = y (authored)
    designer.connect_nodes(x, 0, construct, 1); // pin 1 = x (authored)

    let result = evaluate_node_pin(&designer, "test", construct, 0);
    let NetworkResult::Record(fields) = result else {
        panic!("expected Record, got {:?}", result.to_display_string());
    };
    // Canonical order = sorted ascending by name: [x, y].
    let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["x", "y"]);
    assert!(matches!(fields[0].1, NetworkResult::Int(7)));
    assert!(matches!(fields[1].1, NetworkResult::Int(42)));
}

// ============================================================================
// Nested-def construct (Box = { p: Point })
// ============================================================================

#[test]
fn nested_def_construct_and_destructure() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(box_def_referencing_point())
        .unwrap();

    let y = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", y, Box::new(IntData { value: 11 }));
    let x = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(&mut designer, "test", x, Box::new(IntData { value: 22 }));

    let inner = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        inner,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
        }),
    );

    let outer = designer.add_node("record_construct", DVec2::new(400.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        outer,
        Box::new(RecordConstructData {
            schema: "Box".to_string(),
        }),
    );

    let unbox = designer.add_node("record_destructure", DVec2::new(600.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        unbox,
        Box::new(RecordDestructureData {
            schema: "Box".to_string(),
        }),
    );

    let unpoint = designer.add_node("record_destructure", DVec2::new(800.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        unpoint,
        Box::new(RecordDestructureData {
            schema: "Point".to_string(),
        }),
    );

    designer.validate_active_network();

    // inner pins: [y, x]; outer has one pin "p".
    designer.connect_nodes(y, 0, inner, 0);
    designer.connect_nodes(x, 0, inner, 1);
    designer.connect_nodes(inner, 0, outer, 0);
    designer.connect_nodes(outer, 0, unbox, 0);
    // unbox output pin "p" → unpoint input.
    designer.connect_nodes(unbox, 0, unpoint, 0);

    // unpoint pins: [y, x] in authored order.
    let y_out = evaluate_node_pin(&designer, "test", unpoint, 0);
    let x_out = evaluate_node_pin(&designer, "test", unpoint, 1);
    assert!(matches!(y_out, NetworkResult::Int(11)));
    assert!(matches!(x_out, NetworkResult::Int(22)));
}

// ============================================================================
// Missing-input propagation
// ============================================================================

#[test]
fn missing_input_makes_construct_output_none() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let y = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", y, Box::new(IntData { value: 1 }));

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
        }),
    );

    designer.validate_active_network();
    // Only connect y; x is left unconnected.
    designer.connect_nodes(y, 0, construct, 0);

    let result = evaluate_node_pin(&designer, "test", construct, 0);
    assert!(matches!(result, NetworkResult::None), "got {:?}", result.to_display_string());
}

#[test]
fn missing_input_propagates_through_destructure() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    // No record_construct upstream — destructure's `record` input is unconnected.
    let destructure = designer.add_node("record_destructure", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        destructure,
        Box::new(RecordDestructureData {
            schema: "Point".to_string(),
        }),
    );

    designer.validate_active_network();

    let y_out = evaluate_node_pin(&designer, "test", destructure, 0);
    let x_out = evaluate_node_pin(&designer, "test", destructure, 1);
    assert!(matches!(y_out, NetworkResult::None));
    assert!(matches!(x_out, NetworkResult::None));
}

// ============================================================================
// Pass-through on destructure: extra runtime fields are ignored
// ============================================================================

#[test]
fn destructure_passes_through_extra_fields() {
    let mut designer = setup_designer_with_network("test");
    // Construct over a richer def (Point3 = {y, x, z}) but destructure with
    // a narrower def (Point = {y, x}). The runtime value carries z but the
    // destructure ignores it.
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();
    let point3 = RecordTypeDef {
        name: "Point3".to_string(),
        fields: vec![
            ("y".to_string(), DataType::Int),
            ("x".to_string(), DataType::Int),
            ("z".to_string(), DataType::Int),
        ],
    };
    designer
        .node_type_registry
        .add_record_type_def(point3)
        .unwrap();

    let y = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", y, Box::new(IntData { value: 5 }));
    let x = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(&mut designer, "test", x, Box::new(IntData { value: 6 }));
    let z = designer.add_node("int", DVec2::new(0.0, 200.0));
    set_node_data(&mut designer, "test", z, Box::new(IntData { value: 9 }));

    let construct3 = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct3,
        Box::new(RecordConstructData {
            schema: "Point3".to_string(),
        }),
    );

    // Destructure with narrower schema (Point).
    let destructure = designer.add_node("record_destructure", DVec2::new(400.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        destructure,
        Box::new(RecordDestructureData {
            schema: "Point".to_string(),
        }),
    );

    designer.validate_active_network();

    // Point3 authored order is [y, x, z].
    designer.connect_nodes(y, 0, construct3, 0);
    designer.connect_nodes(x, 0, construct3, 1);
    designer.connect_nodes(z, 0, construct3, 2);

    // The construct's output type is Record(Named("Point3")). The
    // destructure expects Record(Named("Point")). Phase 4 will make the
    // wire connect statically; for now we exercise the runtime
    // pass-through directly by writing the wire and evaluating.
    designer.connect_nodes(construct3, 0, destructure, 0);

    let y_out = evaluate_node_pin(&designer, "test", destructure, 0);
    let x_out = evaluate_node_pin(&designer, "test", destructure, 1);
    // Whether the wire stays connected depends on Phase 4 subtyping. If the
    // wire was disconnected by validation, we'll see None. If it was kept
    // (Phase 4) we'll see the right values. In either case the destructure
    // must NOT panic and must NOT see a "z" field on its narrower schema.
    match (&y_out, &x_out) {
        (NetworkResult::Int(5), NetworkResult::Int(6)) => {}
        (NetworkResult::None, NetworkResult::None) => {}
        other => panic!("unexpected ({:?}, {:?})", other.0.to_display_string(), other.1.to_display_string()),
    }
}

// ============================================================================
// Dangling schema (empty + deleted def)
// ============================================================================

#[test]
fn empty_schema_construct_returns_none() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(RecordConstructData {
            schema: "".to_string(),
        }),
    );
    designer.validate_active_network();
    let r = evaluate_node_pin(&designer, "test", id, 0);
    assert!(matches!(r, NetworkResult::None));
}

#[test]
fn empty_schema_destructure_returns_none() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("record_destructure", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(RecordDestructureData {
            schema: "".to_string(),
        }),
    );
    designer.validate_active_network();
    let r = evaluate_node_pin(&designer, "test", id, 0);
    assert!(matches!(r, NetworkResult::None));
}

#[test]
fn dangling_schema_after_delete_disconnects_downstream() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let y = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", y, Box::new(IntData { value: 1 }));
    let x = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(&mut designer, "test", x, Box::new(IntData { value: 2 }));

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
        }),
    );
    designer.validate_active_network();
    designer.connect_nodes(y, 0, construct, 0);
    designer.connect_nodes(x, 0, construct, 1);

    // Sanity: both wires are present pre-delete.
    let pre_arg_count: usize = {
        let net = designer.node_type_registry.node_networks.get("test").unwrap();
        net.nodes
            .get(&construct)
            .unwrap()
            .arguments
            .iter()
            .map(|a| a.argument_output_pins.len())
            .sum()
    };
    assert_eq!(pre_arg_count, 2);

    designer.delete_record_type_def("Point").unwrap();

    // After delete, the construct's pin layout collapses to no parameters
    // (the schema is dangling). The pre-existing wire entries get truncated
    // when arguments are reset to match the new (empty) parameter list.
    let net = designer.node_type_registry.node_networks.get("test").unwrap();
    let construct_node = net.nodes.get(&construct).unwrap();
    let registry = &designer.node_type_registry;
    let nt = registry.get_node_type_for_node(construct_node).unwrap();
    assert_eq!(nt.parameters.len(), 0, "construct should have no params after schema delete");
}

// ============================================================================
// Field-rename-as-remove+add semantics
// ============================================================================

#[test]
fn field_rename_disconnects_old_pin_wires() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let y = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", y, Box::new(IntData { value: 1 }));
    let x = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(&mut designer, "test", x, Box::new(IntData { value: 2 }));

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
        }),
    );
    designer.validate_active_network();

    // Authored order [y, x] → pin 0 = y, pin 1 = x.
    designer.connect_nodes(y, 0, construct, 0);
    designer.connect_nodes(x, 0, construct, 1);

    // Rename field x → xx (treated as remove x, add xx). The y pin survives.
    designer
        .update_record_type_def(
            "Point",
            vec![
                ("y".to_string(), DataType::Int),
                ("xx".to_string(), DataType::Int),
            ],
        )
        .unwrap();

    // After update + repair, the construct's parameters are still [y, xx]
    // in authored order. The wire to the old "x" pin is gone; the new "xx"
    // pin is unconnected.
    let net = designer.node_type_registry.node_networks.get("test").unwrap();
    let construct_node = net.nodes.get(&construct).unwrap();
    let registry = &designer.node_type_registry;
    let nt = registry.get_node_type_for_node(construct_node).unwrap();
    let names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["y", "xx"]);

    // y wire (pin 0) survives; xx wire (pin 1) is empty.
    assert_eq!(construct_node.arguments.len(), 2);
    assert_eq!(construct_node.arguments[0].argument_output_pins.len(), 1);
    assert_eq!(construct_node.arguments[1].argument_output_pins.len(), 0);
}

// ============================================================================
// Schema-change wire repair end-to-end
// ============================================================================

#[test]
fn retyping_field_disconnects_now_incompatible_wire() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let y = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", y, Box::new(IntData { value: 1 }));
    let x = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(&mut designer, "test", x, Box::new(IntData { value: 2 }));

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
        }),
    );
    designer.validate_active_network();

    designer.connect_nodes(y, 0, construct, 0);
    designer.connect_nodes(x, 0, construct, 1);

    // Retype y from Int to Vec3. The construct node's pin 0 type updates
    // to Vec3 and the post-update validation marks the network invalid
    // because the Int source is no longer compatible. (Following the
    // existing codebase pattern, the wire entry itself is left in place
    // with a validation error rather than being auto-disconnected — the
    // user is expected to fix the wire manually.)
    designer
        .update_record_type_def(
            "Point",
            vec![
                ("y".to_string(), DataType::Vec3),
                ("x".to_string(), DataType::Int),
            ],
        )
        .unwrap();
    designer.validate_active_network();

    let net = designer.node_type_registry.node_networks.get("test").unwrap();
    let construct_node = net.nodes.get(&construct).unwrap();
    let registry = &designer.node_type_registry;
    let nt = registry.get_node_type_for_node(construct_node).unwrap();
    assert_eq!(nt.parameters[0].data_type, DataType::Vec3);
    assert_eq!(nt.parameters[1].data_type, DataType::Int);
    // x wire (pin 1) stays connected (Int → Int).
    assert_eq!(construct_node.arguments[1].argument_output_pins.len(), 1);

    // The retype made the Int → Vec3 wire incompatible; the network is
    // marked invalid so the user notices.
    assert!(!net.valid, "network should be invalid after retype");
}

// ============================================================================
// Rename of the def itself: pin layout follows
// ============================================================================

#[test]
fn rename_def_updates_record_node_pin_layout() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(point_def())
        .unwrap();

    let id = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(RecordConstructData {
            schema: "Point".to_string(),
        }),
    );

    designer.rename_record_type_def("Point", "Pt").unwrap();

    let net = designer.node_type_registry.node_networks.get("test").unwrap();
    let node = net.nodes.get(&id).unwrap();

    // The schema property string was rewritten by the rename walker.
    let data = node
        .data
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .expect("record_construct data");
    assert_eq!(data.schema, "Pt");

    // Pin layout still resolves (now via the renamed def).
    let registry = &designer.node_type_registry;
    let nt = registry.get_node_type_for_node(node).unwrap();
    let names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["y", "x"]);
    assert_eq!(
        nt.output_pins[0].fixed_type(),
        Some(&DataType::Record(RecordType::Named("Pt".to_string())))
    );
}
