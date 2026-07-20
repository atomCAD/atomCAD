//! v7 → v8 migration tests for issue #405 (`add_hydrogen` → `passivate`).
//!
//! Design: `doc/design_halogen_passivation.md` (D3, Phase 2). Mirrors
//! `export_atoms_migration_test.rs` (v6→v7): structural assertions on the JSON
//! pre-pass (both reference keys rewritten, tree-wide incl. a zone body,
//! non-node `data_type` encodings left alone, idempotent), plus an end-to-end
//! load through the real file pipeline whose data assertion downcasts to
//! `PassivateData` — the regression guard for the `data_type` rewrite (a
//! `node_type_name`-only rename reloads the node as `NoData`, which the downcast
//! catches).

use glam::f64::DVec2;
use std::collections::HashMap;

use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::passivate::PassivateData;
use rust_lib_flutter_cad::structure_designer::serialization::migrate_v7_to_v8::{
    migrate_v7_to_v8, migration_call_count, reset_migration_call_count,
};
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use serde_json::{Value, json};
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// Structural JSON pre-pass assertions
// ---------------------------------------------------------------------------

/// A serialized built-in `add_hydrogen` node with the historical empty data
/// payload. Both the `node_type_name` and the polymorphic `data_type` tag carry
/// the type name — the migration must rewrite both.
fn add_hydrogen_node(id: u64) -> Value {
    json!({
        "id": id,
        "node_type_name": "add_hydrogen",
        "custom_name": format!("add_hydrogen{}", id),
        "position": [10.0, 20.0],
        "arguments": [
            { "incoming_wires": [] },
            { "incoming_wires": [] }
        ],
        "data_type": "add_hydrogen",
        "data": {}
    })
}

#[test]
fn top_level_add_hydrogen_rewrites_both_type_name_keys() {
    let mut root = json!({
        "version": 7,
        "node_networks": [
            ["main", { "next_node_id": 2, "nodes": [add_hydrogen_node(1)] }]
        ]
    });
    migrate_v7_to_v8(&mut root).unwrap();

    let node = &root["node_networks"][0][1]["nodes"][0];
    assert_eq!(node["node_type_name"], json!("passivate"));
    assert_eq!(
        node["data_type"],
        json!("passivate"),
        "the polymorphic data_type tag must be rewritten too, or the loader \
         falls back to NoData"
    );
    // The data payload is untouched.
    assert_eq!(node["data"], json!({}));
}

#[test]
fn add_hydrogen_inside_map_body_is_rewritten() {
    // The whole-tree walk must reach nodes at any zone-body depth (keyed on the
    // JSON structure, not an HOF-name list) — the classic migration miss.
    let mut root = json!({
        "version": 7,
        "node_networks": [
            ["main", {
                "next_node_id": 2,
                "nodes": [{
                    "id": 1,
                    "node_type_name": "map",
                    "position": [0.0, 0.0],
                    "arguments": [{ "incoming_wires": [] }],
                    "data_type": "map",
                    "data": {},
                    "zone": {
                        "next_node_id": 6,
                        "nodes": [add_hydrogen_node(5)]
                    }
                }]
            }]
        ]
    });
    migrate_v7_to_v8(&mut root).unwrap();

    let body_node = &root["node_networks"][0][1]["nodes"][0]["zone"]["nodes"][0];
    assert_eq!(body_node["node_type_name"], json!("passivate"));
    assert_eq!(body_node["data_type"], json!("passivate"));
}

#[test]
fn data_type_enum_encodings_are_not_rewritten() {
    // `data_type` is also used for serialized `DataType`s on parameters/pins.
    // Those hold enum encodings (`"Int"`, `{"Record": …}`), never a node-type
    // name, so they must be left alone — the value gate (`== "add_hydrogen"`)
    // guarantees this.
    let mut root = json!({
        "version": 7,
        "node_networks": [
            ["main", {
                "next_node_id": 2,
                "nodes": [{
                    "id": 1,
                    "node_type_name": "parameter",
                    "position": [0.0, 0.0],
                    "arguments": [],
                    "data_type": "parameter",
                    "data": {
                        "parameters": [
                            { "id": null, "name": "x", "data_type": "Float" }
                        ]
                    }
                }]
            }]
        ]
    });
    let before = root.clone();
    migrate_v7_to_v8(&mut root).unwrap();
    assert_eq!(
        root, before,
        "no add_hydrogen reference → nothing rewritten"
    );
}

#[test]
fn migration_is_idempotent() {
    let mut root = json!({
        "version": 7,
        "node_networks": [
            ["main", { "next_node_id": 2, "nodes": [add_hydrogen_node(1)] }]
        ]
    });
    migrate_v7_to_v8(&mut root).unwrap();
    let after_first = serde_json::to_string(&root).unwrap();
    migrate_v7_to_v8(&mut root).unwrap();
    let after_second = serde_json::to_string(&root).unwrap();
    assert_eq!(
        after_first, after_second,
        "second run is a no-op — the old string is gone after the first"
    );
}

// ---------------------------------------------------------------------------
// End-to-end: real load pipeline (version dispatch + downcast value check)
// ---------------------------------------------------------------------------

/// Recursively renames `passivate` → `add_hydrogen` in every `node_type_name` /
/// `data_type` reference key (the inverse of the migration) and strips the
/// node's data down to the historical empty payload, so a file saved by the
/// current (v8) serializer faithfully represents a genuine v7 save.
fn downgrade_passivate_to_v7(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let is_passivate_node = map.get("node_type_name") == Some(&json!("passivate"));
            for (key, child) in map.iter_mut() {
                if (key == "node_type_name" || key == "data_type")
                    && child.as_str() == Some("passivate")
                {
                    *child = json!("add_hydrogen");
                } else if key == "data" && is_passivate_node {
                    // A genuine v7 add_hydrogen node had no `element` field.
                    *child = json!({});
                } else {
                    downgrade_passivate_to_v7(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                downgrade_passivate_to_v7(item);
            }
        }
        _ => {}
    }
}

/// Builds a `passivate` node in memory, saves it (v8), then downgrades the
/// on-disk JSON to a genuine v7 shape (rename the type-name references back to
/// `add_hydrogen`, empty the data, `version → 7`). Returns the path and the
/// node id.
fn write_v7_passivate_file(dir: &std::path::Path) -> (String, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let passivate_id = designer.add_node("passivate", DVec2::new(0.0, 0.0));

    let v8_path = dir.join("built_v8.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &v8_path,
        false,
        &HashMap::new(),
    )
    .unwrap();

    let raw = std::fs::read_to_string(&v8_path).unwrap();
    let mut root: Value = serde_json::from_str(&raw).unwrap();
    root["version"] = json!(7);
    downgrade_passivate_to_v7(&mut root);

    let path = dir.join("v7_passivate.cnnd");
    std::fs::write(&path, serde_json::to_string_pretty(&root).unwrap()).unwrap();
    (path.to_str().unwrap().to_string(), passivate_id)
}

#[test]
fn v7_add_hydrogen_loads_as_passivate_with_default_element() {
    let dir = tempdir().unwrap();
    let (path, passivate_id) = write_v7_passivate_file(dir.path());

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &path).expect("v7 load failed");

    let net = registry.node_networks.get("main").unwrap();
    let node = net
        .nodes
        .get(&passivate_id)
        .expect("passivate node present");

    // Renamed type.
    assert_eq!(node.node_type_name, "passivate");

    // Data assertion by downcast: this is the regression guard for the
    // `data_type` rewrite. A `node_type_name`-only rename would reload this
    // node's data as `NoData`, and this downcast would fail. The historical
    // empty payload `{}` deserializes with `element` defaulting to hydrogen.
    let data = node
        .data
        .as_ref()
        .as_any_ref()
        .downcast_ref::<PassivateData>()
        .expect("data must load as PassivateData (data_type rewrite guard)");
    assert_eq!(data.element, 1, "empty v7 data must default element to H");
}

#[test]
fn resave_after_v7_load_emits_version_8() {
    let dir = tempdir().unwrap();
    let (path, _) = write_v7_passivate_file(dir.path());

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &path).expect("v7 load failed");

    let out = dir.path().join("resaved.cnnd");
    save_node_networks_to_file(&mut registry, &out, false, &HashMap::new()).unwrap();

    let raw = std::fs::read_to_string(&out).unwrap();
    let root: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(root["version"], json!(8), "re-save must emit version 8");
}

#[test]
fn v8_file_skips_the_migration_pass() {
    // A v8 file (no downgrade) must not invoke migrate_v7_to_v8.
    let dir = tempdir().unwrap();

    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    designer.add_node("passivate", DVec2::new(0.0, 0.0));

    let path = dir.path().join("native_v8.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .unwrap();

    reset_migration_call_count();
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, path.to_str().unwrap()).expect("v8 load failed");
    assert_eq!(
        migration_call_count(),
        0,
        "a v8 file must not invoke migrate_v7_to_v8"
    );
}
