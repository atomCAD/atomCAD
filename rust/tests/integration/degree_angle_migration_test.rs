//! v5 → v6 migration tests for issue #384 (`free_rot` radians → degrees).
//!
//! Design: `doc/design_degree_angle_inputs.md` (Phase 3). Follows the pattern
//! of `iterator_migration_test.rs` (v3→v4): structural assertions on the JSON
//! transform, idempotency, recursion into zone bodies, and an end-to-end
//! load → validate → evaluate pass through the real file pipeline.

use glam::f64::DVec2;
use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;

use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::network_validator::validate_network;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::float::FloatData;
use rust_lib_flutter_cad::structure_designer::serialization::migrate_v5_to_v6::{
    migrate_v5_to_v6, migration_call_count, reset_migration_call_count,
};
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use serde_json::{Value, json};
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// JSON builders (minimal shapes the migration reads)
// ---------------------------------------------------------------------------

/// A `free_rot` node JSON. `angle_radians` goes into the radian-era `data.angle`
/// key; `angle_arg` is the JSON for the angle pin (argument index 1).
fn free_rot_node(id: u64, angle_radians: f64, angle_arg: Value) -> Value {
    json!({
        "id": id,
        "node_type_name": "free_rot",
        "custom_name": format!("free_rot{}", id),
        "position": [300.0, 100.0],
        "arguments": [
            { "incoming_wires": [] },
            angle_arg,
            { "incoming_wires": [] },
            { "incoming_wires": [] }
        ],
        "data_type": "free_rot",
        "data": { "angle": angle_radians, "rot_axis": [0.0, 0.0, 1.0], "pivot_point": [0.0, 0.0, 0.0] }
    })
}

/// A modern-shape angle wire from `(src_id, pin 0)`, local scope.
fn wire_from(src_id: u64) -> Value {
    json!({
        "incoming_wires": [
            { "source_node_id": src_id, "source_pin": { "NodeOutput": { "pin_index": 0 } }, "source_scope_depth": 0 }
        ]
    })
}

fn empty_arg() -> Value {
    json!({ "incoming_wires": [] })
}

/// Wraps one or more nodes into a v5 root with a single `main` network.
fn v5_root(next_node_id: u64, nodes: Value) -> Value {
    json!({
        "version": 5,
        "node_networks": [
            ["main", { "next_node_id": next_node_id, "nodes": nodes }]
        ]
    })
}

/// Finds the single node of a given type in the `main` network's `nodes`.
fn find_node<'a>(root: &'a Value, type_name: &str) -> &'a Value {
    let nodes = root["node_networks"][0][1]["nodes"].as_array().unwrap();
    let matches: Vec<&Value> = nodes
        .iter()
        .filter(|n| n["node_type_name"] == json!(type_name))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one '{}' node, found {}",
        type_name,
        matches.len()
    );
    matches[0]
}

fn main_nodes(root: &Value) -> &Vec<Value> {
    root["node_networks"][0][1]["nodes"].as_array().unwrap()
}

fn main_next_node_id(root: &Value) -> u64 {
    root["node_networks"][0][1]["next_node_id"]
        .as_u64()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Stored-value conversion + field rename (unwired angle pin)
// ---------------------------------------------------------------------------

#[test]
fn unwired_free_rot_renames_and_converts_stored_angle() {
    let mut root = v5_root(9, json!([free_rot_node(8, FRAC_PI_2, empty_arg())]));
    migrate_v5_to_v6(&mut root).unwrap();

    let free_rot = find_node(&root, "free_rot");
    assert!(
        free_rot["data"].get("angle").is_none(),
        "the radian-era `angle` key must be gone after migration"
    );
    let degrees = free_rot["data"]["angle_degrees"].as_f64().unwrap();
    assert!(
        (degrees - 90.0).abs() < 1e-9,
        "PI/2 radians must convert to 90 degrees, got {}",
        degrees
    );
    // No synthesized node, no id churn for an unwired pin.
    assert_eq!(main_nodes(&root).len(), 1);
    assert_eq!(main_next_node_id(&root), 9);
}

// ---------------------------------------------------------------------------
// Wired angle pin: synthesize `degrees(x)` on the wire, unconditionally
// ---------------------------------------------------------------------------

/// Shared assertions for a synthesized `degrees(x)` expr node feeding a
/// `free_rot`'s angle pin. `expr_id` is the expected new node id; `src_id` is
/// the original upstream source that must now feed the expr node's `x` pin.
fn assert_degrees_node_spliced(root: &Value, expr_id: u64, src_id: u64) {
    let expr = find_node(root, "expr");
    assert_eq!(expr["id"].as_u64().unwrap(), expr_id);
    assert_eq!(expr["custom_name"], json!("to_degrees"));
    assert_eq!(expr["data"]["expression"], json!("degrees(x)"));
    assert_eq!(expr["data"]["parameters"][0]["name"], json!("x"));
    assert_eq!(expr["data"]["parameters"][0]["data_type"], json!("Float"));

    // The expr node's `x` pin carries the original source wire.
    let expr_wires = expr["arguments"][0]["incoming_wires"].as_array().unwrap();
    assert_eq!(expr_wires.len(), 1);
    assert_eq!(expr_wires[0]["source_node_id"].as_u64().unwrap(), src_id);

    // The free_rot's angle pin (index 1) now points at the expr node, pin 0.
    let free_rot = find_node(root, "free_rot");
    let angle_wires = free_rot["arguments"][1]["incoming_wires"]
        .as_array()
        .unwrap();
    assert_eq!(angle_wires.len(), 1);
    assert_eq!(angle_wires[0]["source_node_id"].as_u64().unwrap(), expr_id);
    assert_eq!(
        angle_wires[0]["source_pin"]["NodeOutput"]["pin_index"]
            .as_i64()
            .unwrap(),
        0
    );
}

#[test]
fn wired_angle_from_float_gets_degrees_node_inserted() {
    // float(id 1) → free_rot.angle (id 2). next_node_id = 3.
    let float = json!({
        "id": 1, "node_type_name": "float", "position": [0.0, 0.0],
        "arguments": [{ "incoming_wires": [] }],
        "data_type": "float", "data": { "value": 1.5 }
    });
    let mut root = v5_root(3, json!([float, free_rot_node(2, 0.0, wire_from(1))]));
    migrate_v5_to_v6(&mut root).unwrap();

    assert_eq!(main_nodes(&root).len(), 3, "one expr node synthesized");
    assert_eq!(main_next_node_id(&root), 4, "next_node_id bumped past expr");
    assert_degrees_node_spliced(&root, 3, 1);
    // The free_rot itself is still migrated (field renamed).
    assert!(
        find_node(&root, "free_rot")["data"]
            .get("angle_degrees")
            .is_some()
    );
}

#[test]
fn wired_angle_from_expr_source_also_gets_degrees_node() {
    // An upstream expr(id 1) → free_rot.angle(id 2). The rule is uniform: a
    // conversion node is inserted regardless of the source node type.
    let upstream_expr = json!({
        "id": 1, "node_type_name": "expr", "position": [0.0, 0.0],
        "arguments": [],
        "data_type": "expr",
        "data": { "parameters": [], "expression": "0.5" }
    });
    let mut root = v5_root(
        3,
        json!([upstream_expr, free_rot_node(2, 0.0, wire_from(1))]),
    );
    migrate_v5_to_v6(&mut root).unwrap();

    // Two expr nodes now (the original + synthesized); locate by custom_name.
    let nodes = main_nodes(&root);
    let synthesized: Vec<&Value> = nodes
        .iter()
        .filter(|n| n["custom_name"] == json!("to_degrees"))
        .collect();
    assert_eq!(synthesized.len(), 1, "exactly one to_degrees node inserted");
    let syn = synthesized[0];
    assert_eq!(syn["data"]["expression"], json!("degrees(x)"));
    assert_eq!(
        syn["arguments"][0]["incoming_wires"][0]["source_node_id"]
            .as_u64()
            .unwrap(),
        1
    );
}

// ---------------------------------------------------------------------------
// Legacy `argument_output_pins` wire shape (chained v2/v3 files)
// ---------------------------------------------------------------------------

#[test]
fn wired_angle_in_legacy_shape_is_read_and_rewired() {
    // The angle pin uses the legacy map shape `{ "1": 0 }` (as emitted by the
    // v2→v3 / v3→v4 passes). The migration must read it and move it verbatim
    // onto the synthesized expr node, emitting the modern `incoming_wires`.
    let float = json!({
        "id": 1, "node_type_name": "float", "position": [0.0, 0.0],
        "arguments": [{ "argument_output_pins": {} }],
        "data_type": "float", "data": { "value": 1.5 }
    });
    let legacy_angle = json!({ "argument_output_pins": { "1": 0 } });
    let mut root = v5_root(3, json!([float, free_rot_node(2, 0.0, legacy_angle)]));
    migrate_v5_to_v6(&mut root).unwrap();

    assert_degrees_node_spliced(&root, 3, 1);
    // The wire moved onto the expr node is now in the modern shape with depth 0.
    let expr = find_node(&root, "expr");
    let w = &expr["arguments"][0]["incoming_wires"][0];
    assert_eq!(
        w["source_pin"]["NodeOutput"]["pin_index"].as_i64().unwrap(),
        0
    );
    assert_eq!(w["source_scope_depth"].as_u64().unwrap(), 0);
}

// ---------------------------------------------------------------------------
// Recursion into zone bodies (keyed on the `zone` field, not HOF names)
// ---------------------------------------------------------------------------

/// A zone-bearing node (`type_name`) whose body contains `body_nodes`.
fn zone_owner(id: u64, type_name: &str, body_next_id: u64, body_nodes: Value) -> Value {
    json!({
        "id": id,
        "node_type_name": type_name,
        "position": [0.0, 0.0],
        "arguments": [{ "incoming_wires": [] }],
        "data_type": type_name,
        "data": {},
        "zone": { "next_node_id": body_next_id, "nodes": body_nodes }
    })
}

#[test]
fn free_rot_inside_map_body_is_migrated_with_capture_wire_verbatim() {
    // A `free_rot` inside a `map` body, its angle pin a CAPTURE
    // (source_scope_depth = 2, a ZoneInput reference). The wire must move onto
    // the synthesized expr node verbatim (depth + ZoneInput preserved), and the
    // expr id must be allocated from the BODY's next_node_id (5), not the top
    // network's.
    let capture_wire = json!({
        "incoming_wires": [
            { "source_node_id": 7, "source_pin": { "ZoneInput": { "pin_index": 0 } }, "source_scope_depth": 2 }
        ]
    });
    let body_free_rot = free_rot_node(4, FRAC_PI_2, capture_wire);
    let map = zone_owner(1, "map", 5, json!([body_free_rot]));
    let mut root = v5_root(2, json!([map]));
    migrate_v5_to_v6(&mut root).unwrap();

    // Reach into the map body.
    let body = &main_nodes(&root)[0]["zone"];
    assert_eq!(body["next_node_id"].as_u64().unwrap(), 6, "body id bumped");
    let body_nodes = body["nodes"].as_array().unwrap();
    assert_eq!(body_nodes.len(), 2, "expr node synthesized inside the body");

    let expr = body_nodes
        .iter()
        .find(|n| n["node_type_name"] == json!("expr"))
        .unwrap();
    assert_eq!(
        expr["id"].as_u64().unwrap(),
        5,
        "expr allocated from body next_id"
    );
    // Capture wire preserved verbatim on the expr's `x` pin.
    let w = &expr["arguments"][0]["incoming_wires"][0];
    assert_eq!(w["source_node_id"].as_u64().unwrap(), 7);
    assert_eq!(
        w["source_pin"]["ZoneInput"]["pin_index"].as_u64().unwrap(),
        0
    );
    assert_eq!(w["source_scope_depth"].as_u64().unwrap(), 2);

    // The body free_rot is migrated (field renamed) and now points at the expr.
    let fr = body_nodes
        .iter()
        .find(|n| n["node_type_name"] == json!("free_rot"))
        .unwrap();
    assert!(fr["data"].get("angle_degrees").is_some());
    assert_eq!(
        fr["arguments"][1]["incoming_wires"][0]["source_node_id"]
            .as_u64()
            .unwrap(),
        5
    );
}

#[test]
fn free_rot_inside_closure_body_is_migrated() {
    // `closure` is zone-bearing but NOT one of the four HOF names — this locks
    // the zone-field-keyed recursion (a name-list recursion would miss it, and
    // the body free_rot's `data.angle` would fail strict deserialization after
    // the rename).
    let body_free_rot = free_rot_node(4, FRAC_PI_2, empty_arg());
    let closure = zone_owner(1, "closure", 5, json!([body_free_rot]));
    let mut root = v5_root(2, json!([closure]));
    migrate_v5_to_v6(&mut root).unwrap();

    let body_nodes = main_nodes(&root)[0]["zone"]["nodes"].as_array().unwrap();
    let fr = &body_nodes[0];
    assert!(
        fr["data"].get("angle").is_none() && fr["data"].get("angle_degrees").is_some(),
        "free_rot inside a closure body must be migrated"
    );
    let degrees = fr["data"]["angle_degrees"].as_f64().unwrap();
    assert!((degrees - 90.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Determinism & idempotency
// ---------------------------------------------------------------------------

#[test]
fn two_wired_free_rots_allocate_ids_in_sorted_order() {
    // free_rot ids 2 and 5, both wired from float(1). next_node_id = 6.
    // Expr ids must be allocated in free_rot-id-sorted order: 6 for #2, 7 for #5.
    let float = json!({
        "id": 1, "node_type_name": "float", "position": [0.0, 0.0],
        "arguments": [{ "incoming_wires": [] }],
        "data_type": "float", "data": { "value": 1.0 }
    });
    let mut root = v5_root(
        6,
        json!([
            float,
            free_rot_node(5, 0.0, wire_from(1)),
            free_rot_node(2, 0.0, wire_from(1))
        ]),
    );
    migrate_v5_to_v6(&mut root).unwrap();

    let nodes = main_nodes(&root);
    // Map free_rot id -> the expr id feeding its angle pin.
    let expr_for = |fr_id: u64| -> u64 {
        let fr = nodes
            .iter()
            .find(|n| n["id"].as_u64() == Some(fr_id))
            .unwrap();
        fr["arguments"][1]["incoming_wires"][0]["source_node_id"]
            .as_u64()
            .unwrap()
    };
    assert_eq!(expr_for(2), 6, "lowest free_rot id gets the first new id");
    assert_eq!(expr_for(5), 7);
    assert_eq!(main_next_node_id(&root), 8);
}

#[test]
fn migration_is_idempotent() {
    let float = json!({
        "id": 1, "node_type_name": "float", "position": [0.0, 0.0],
        "arguments": [{ "incoming_wires": [] }],
        "data_type": "float", "data": { "value": 1.5 }
    });
    let mut root = v5_root(3, json!([float, free_rot_node(2, FRAC_PI_2, wire_from(1))]));

    migrate_v5_to_v6(&mut root).unwrap();
    let after_first = serde_json::to_string(&root).unwrap();
    migrate_v5_to_v6(&mut root).unwrap();
    let after_second = serde_json::to_string(&root).unwrap();

    assert_eq!(
        after_first, after_second,
        "second run must be a no-op — the `data.angle` gate is gone after the first"
    );
}

// ---------------------------------------------------------------------------
// End-to-end: real load pipeline (version dispatch + validate + evaluate)
// ---------------------------------------------------------------------------

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

/// Builds a valid v5 `.cnnd` on disk containing `sphere → free_rot`, with the
/// angle pin wired from `float(angle_radians)` when `wire_angle` is true.
///
/// The skeleton is produced by the **real serializer** (build in memory → save)
/// so all node data is well-formed, then the on-disk JSON is downgraded to a
/// genuine v5 shape: `version → 5`, and every `free_rot`'s `angle_degrees`
/// field renamed back to the radian-era `angle` (value converted deg→rad, the
/// exact inverse of the migration, so the file faithfully represents a pre-v6
/// save). Returns the file path.
fn write_v5_sphere_free_rot(dir: &std::path::Path, wire_angle: bool, angle_radians: f64) -> String {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let sphere = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    let free_rot = designer.add_node("free_rot", DVec2::new(400.0, 0.0));
    designer.connect_nodes(sphere, 0, free_rot, 0);

    if wire_angle {
        let float_id = designer.add_node("float", DVec2::new(150.0, 150.0));
        set_node_data(
            &mut designer,
            "main",
            float_id,
            Box::new(FloatData {
                value: angle_radians,
            }),
        );
        designer.connect_nodes(float_id, 0, free_rot, 1);
    }

    // Save as v6, then downgrade the JSON to v5 on disk.
    let v6_path = dir.join("built_v6.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &v6_path,
        false,
        &HashMap::new(),
    )
    .unwrap();

    let raw = std::fs::read_to_string(&v6_path).unwrap();
    let mut root: Value = serde_json::from_str(&raw).unwrap();
    root["version"] = json!(5);
    downgrade_free_rot_data_to_v5(&mut root);

    let path = dir.join("v5_sphere_free_rot.cnnd");
    std::fs::write(&path, serde_json::to_string_pretty(&root).unwrap()).unwrap();
    path.to_str().unwrap().to_string()
}

/// Rewrites every `free_rot` node's `data.angle_degrees` back to the v5
/// `data.angle` (radians) across all networks — the inverse of the migration.
fn downgrade_free_rot_data_to_v5(root: &mut Value) {
    let Some(networks) = root["node_networks"].as_array_mut() else {
        return;
    };
    for entry in networks {
        let Some(nodes) = entry[1]["nodes"].as_array_mut() else {
            continue;
        };
        for node in nodes {
            if node["node_type_name"] != json!("free_rot") {
                continue;
            }
            if let Some(data) = node["data"].as_object_mut()
                && let Some(deg) = data.remove("angle_degrees")
            {
                let radians = deg.as_f64().unwrap_or(0.0).to_radians();
                data.insert("angle".to_string(), json!(radians));
            }
        }
    }
}

fn load_and_validate(path: &str) -> NodeTypeRegistry {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, path)
        .unwrap_or_else(|e| panic!("failed to load {}: {}", path, e));
    let order = registry.get_networks_in_dependency_order();
    for name in order {
        let ptr = &mut registry as *mut NodeTypeRegistry;
        unsafe {
            if let Some(net) = (*ptr).node_networks.get_mut(&name) {
                validate_network(net, &mut *ptr, None);
            }
        }
    }
    registry
}

fn evaluate(registry: &NodeTypeRegistry, network: &str, node_id: u64) -> NetworkResult {
    let net = registry.node_networks.get(network).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement {
        node_network: net,
        node_id: 0,
    }];
    evaluator.evaluate(&stack, node_id, 0, registry, false, &mut context)
}

fn find_id_by_type(registry: &NodeTypeRegistry, network: &str, type_name: &str) -> Option<u64> {
    let net = registry.node_networks.get(network).unwrap();
    net.nodes
        .values()
        .find(|n| n.node_type_name == type_name)
        .map(|n| n.id)
}

#[test]
fn unwired_v5_file_loads_validates_and_evaluates() {
    let dir = tempdir().unwrap();
    let path = write_v5_sphere_free_rot(dir.path(), false, FRAC_PI_2);
    let registry = load_and_validate(&path);

    let net = registry.node_networks.get("main").unwrap();
    assert!(
        net.valid,
        "migrated network must validate; errors={:?}",
        net.validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );
    // No synthesized node for the unwired case.
    assert_eq!(net.nodes.len(), 2);

    // free_rot on a Blueprint yields a Blueprint (no error).
    let fr_id = find_id_by_type(&registry, "main", "free_rot").unwrap();
    match evaluate(&registry, "main", fr_id) {
        NetworkResult::Blueprint(_) => {}
        NetworkResult::Error(e) => panic!("free_rot evaluated to Error: {}", e),
        other => panic!("expected Blueprint, got {:?}", other.infer_data_type()),
    }
}

#[test]
fn wired_v5_file_synthesizes_degrees_node_and_validates() {
    let dir = tempdir().unwrap();
    let path = write_v5_sphere_free_rot(dir.path(), true, FRAC_PI_2);
    let registry = load_and_validate(&path);

    let net = registry.node_networks.get("main").unwrap();
    // sphere + free_rot + float + synthesized expr = 4 nodes.
    assert_eq!(
        net.nodes.len(),
        4,
        "expected a synthesized degrees(x) node; got {:?}",
        net.nodes
            .values()
            .map(|n| n.node_type_name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        net.valid,
        "migrated+wired network must validate (expr parses, degrees resolves); errors={:?}",
        net.validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );

    // The synthesized expr node exists and evaluates the free_rot cleanly.
    let expr_id = find_id_by_type(&registry, "main", "expr").expect("expr node must exist");
    assert!(matches!(
        evaluate(&registry, "main", expr_id),
        NetworkResult::Float(_)
    ));

    let fr_id = find_id_by_type(&registry, "main", "free_rot").unwrap();
    match evaluate(&registry, "main", fr_id) {
        NetworkResult::Blueprint(_) => {}
        NetworkResult::Error(e) => panic!("free_rot evaluated to Error: {}", e),
        other => panic!("expected Blueprint, got {:?}", other.infer_data_type()),
    }
}

#[test]
fn v6_file_skips_the_migration_pass() {
    // A v6 file must not run the v5→v6 pass.
    let dir = tempdir().unwrap();
    let path = write_v5_sphere_free_rot(dir.path(), false, FRAC_PI_2);
    // Rewrite the on-disk file to v6 with the already-migrated field name.
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut root: Value = serde_json::from_str(&raw).unwrap();
    root["version"] = json!(6);
    // Rename the free_rot's `angle` → `angle_degrees` (v6 shape).
    let nodes = root["node_networks"][0][1]["nodes"].as_array_mut().unwrap();
    for n in nodes {
        if n["node_type_name"] == json!("free_rot") {
            let data = n["data"].as_object_mut().unwrap();
            let a = data.remove("angle").unwrap();
            data.insert("angle_degrees".to_string(), a);
        }
    }
    std::fs::write(&path, serde_json::to_string_pretty(&root).unwrap()).unwrap();

    reset_migration_call_count();
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &path).expect("v6 load failed");
    assert_eq!(
        migration_call_count(),
        0,
        "a v6 file must not invoke migrate_v5_to_v6"
    );
}
