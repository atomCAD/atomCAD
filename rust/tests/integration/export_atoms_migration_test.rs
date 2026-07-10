//! v6 → v7 migration tests for issue #353 (`export_xyz` → `export_atoms`).
//!
//! Design: `doc/design_export_atoms_node.md` ("`.cnnd` migration: v6 → v7").
//! Follows the pattern of `degree_angle_migration_test.rs` (v5→v6): structural
//! assertions on the JSON pre-pass (both reference keys rewritten, tree-wide,
//! payload untouched, idempotent), plus an end-to-end load through the real file
//! pipeline whose data assertion downcasts to `ExportAtomsData` and checks the
//! stored `file_name` **value** — the regression guard for the `data_type`
//! rewrite (a `node_type_name`-only rename reloads as `NoData` and only a value
//! check catches it).

use glam::f64::DVec2;
use std::collections::HashMap;

use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::export_atoms::ExportAtomsData;
use rust_lib_flutter_cad::structure_designer::nodes::string::StringData;
use rust_lib_flutter_cad::structure_designer::serialization::migrate_v6_to_v7::{
    migrate_v6_to_v7, migration_call_count, reset_migration_call_count,
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

/// A serialized built-in `export_xyz` node with a stored `file_name`. Both the
/// `node_type_name` and the polymorphic `data_type` tag carry the type name —
/// the migration must rewrite both.
fn export_xyz_node(id: u64, file_name: &str) -> Value {
    json!({
        "id": id,
        "node_type_name": "export_xyz",
        "custom_name": format!("export_xyz{}", id),
        "position": [10.0, 20.0],
        "arguments": [
            { "incoming_wires": [] },
            { "incoming_wires": [] },
            { "incoming_wires": [] }
        ],
        "data_type": "export_xyz",
        "data": { "file_name": file_name }
    })
}

#[test]
fn top_level_export_xyz_rewrites_both_type_name_keys() {
    let mut root = json!({
        "version": 6,
        "node_networks": [
            ["main", { "next_node_id": 2, "nodes": [export_xyz_node(1, "out.xyz")] }]
        ]
    });
    migrate_v6_to_v7(&mut root).unwrap();

    let node = &root["node_networks"][0][1]["nodes"][0];
    assert_eq!(node["node_type_name"], json!("export_atoms"));
    assert_eq!(
        node["data_type"],
        json!("export_atoms"),
        "the polymorphic data_type tag must be rewritten too, or the loader \
         falls back to NoData and drops file_name"
    );
    // The data payload is untouched.
    assert_eq!(node["data"]["file_name"], json!("out.xyz"));
}

#[test]
fn export_xyz_inside_foreach_body_is_rewritten() {
    // The whole-tree walk must reach nodes at any zone-body depth (keyed on the
    // JSON structure, not an HOF-name list).
    let mut root = json!({
        "version": 6,
        "node_networks": [
            ["main", {
                "next_node_id": 2,
                "nodes": [{
                    "id": 1,
                    "node_type_name": "foreach",
                    "position": [0.0, 0.0],
                    "arguments": [{ "incoming_wires": [] }],
                    "data_type": "foreach",
                    "data": {},
                    "zone": {
                        "next_node_id": 6,
                        "nodes": [export_xyz_node(5, "body.mol")]
                    }
                }]
            }]
        ]
    });
    migrate_v6_to_v7(&mut root).unwrap();

    let body_node = &root["node_networks"][0][1]["nodes"][0]["zone"]["nodes"][0];
    assert_eq!(body_node["node_type_name"], json!("export_atoms"));
    assert_eq!(body_node["data_type"], json!("export_atoms"));
    assert_eq!(body_node["data"]["file_name"], json!("body.mol"));
}

#[test]
fn data_type_enum_encodings_are_not_rewritten() {
    // `data_type` is also used for serialized `DataType`s on parameters/pins.
    // Those hold enum encodings (`"Float"`, `{"Record": …}`), never a node-type
    // name, so they must be left alone — the value gate (`== "export_xyz"`)
    // guarantees this.
    let mut root = json!({
        "version": 6,
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
    migrate_v6_to_v7(&mut root).unwrap();
    assert_eq!(root, before, "no export_xyz reference → nothing rewritten");
}

#[test]
fn user_network_named_export_xyz_is_left_untouched() {
    // A (hypothetical, very old) user network literally named `export_xyz`: the
    // name lives under the `node_networks` map key and the network's own
    // `node_type.name`, neither of which is a rewritten reference key. It merely
    // becomes un-shadowed once the built-in vacates the name.
    let mut root = json!({
        "version": 6,
        "node_networks": [
            ["export_xyz", {
                "next_node_id": 1,
                "node_type": { "name": "export_xyz", "parameters": [] },
                "nodes": []
            }]
        ]
    });
    migrate_v6_to_v7(&mut root).unwrap();
    assert_eq!(root["node_networks"][0][0], json!("export_xyz"));
    assert_eq!(
        root["node_networks"][0][1]["node_type"]["name"],
        json!("export_xyz")
    );
}

#[test]
fn migration_is_idempotent() {
    let mut root = json!({
        "version": 6,
        "node_networks": [
            ["main", { "next_node_id": 2, "nodes": [export_xyz_node(1, "out.xyz")] }]
        ]
    });
    migrate_v6_to_v7(&mut root).unwrap();
    let after_first = serde_json::to_string(&root).unwrap();
    migrate_v6_to_v7(&mut root).unwrap();
    let after_second = serde_json::to_string(&root).unwrap();
    assert_eq!(
        after_first, after_second,
        "second run is a no-op — the old string is gone after the first"
    );
}

// ---------------------------------------------------------------------------
// End-to-end: real load pipeline (version dispatch + downcast value check)
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

/// Recursively renames `export_atoms` → `export_xyz` in every `node_type_name`
/// / `data_type` reference key (the inverse of the migration), so a file saved
/// by the current (v7) serializer faithfully represents a genuine v6 save.
fn downgrade_export_atoms_to_v6(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if (key == "node_type_name" || key == "data_type")
                    && child.as_str() == Some("export_atoms")
                {
                    *child = json!("export_xyz");
                } else {
                    downgrade_export_atoms_to_v6(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                downgrade_export_atoms_to_v6(item);
            }
        }
        _ => {}
    }
}

/// Builds `string("wired.mol") → export_atoms.file_name` in memory, saves it
/// (v7), then downgrades the on-disk JSON to a genuine v6 shape (rename the
/// type-name references back to `export_xyz`, `version → 6`). Returns the path
/// and the two node ids `(string_id, export_id)`.
fn write_v6_export_file(dir: &std::path::Path) -> (String, u64, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let string_id = designer.add_node("string", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        string_id,
        Box::new(StringData {
            value: "wired.mol".to_string(),
        }),
    );

    let export_id = designer.add_node("export_atoms", DVec2::new(300.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        export_id,
        Box::new(ExportAtomsData {
            file_name: "stored.xyz".to_string(),
        }),
    );
    // Wire the string into the `file_name` pin (index 1).
    designer.connect_nodes(string_id, 0, export_id, 1);

    let v7_path = dir.join("built_v7.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &v7_path,
        false,
        &HashMap::new(),
    )
    .unwrap();

    let raw = std::fs::read_to_string(&v7_path).unwrap();
    let mut root: Value = serde_json::from_str(&raw).unwrap();
    root["version"] = json!(6);
    downgrade_export_atoms_to_v6(&mut root);

    let path = dir.join("v6_export.cnnd");
    std::fs::write(&path, serde_json::to_string_pretty(&root).unwrap()).unwrap();
    (path.to_str().unwrap().to_string(), string_id, export_id)
}

#[test]
fn v6_export_xyz_loads_as_export_atoms_with_data_and_wire_intact() {
    let dir = tempdir().unwrap();
    let (path, string_id, export_id) = write_v6_export_file(dir.path());

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &path).expect("v6 load failed");

    let net = registry.node_networks.get("main").unwrap();
    let export = net.nodes.get(&export_id).expect("export node present");

    // Renamed type.
    assert_eq!(export.node_type_name, "export_atoms");

    // Data assertion by VALUE, not just type name: this is the regression guard
    // for the `data_type` rewrite. A `node_type_name`-only rename would reload
    // this node's data as `NoData`, and this downcast would fail.
    let data = export
        .data
        .as_ref()
        .as_any_ref()
        .downcast_ref::<ExportAtomsData>()
        .expect("data must load as ExportAtomsData (data_type rewrite guard)");
    assert_eq!(data.file_name, "stored.xyz");

    // The `file_name` wire (string → pin 1) survived the migration.
    let file_name_arg = &export.arguments[1];
    assert_eq!(file_name_arg.incoming_wires.len(), 1);
    assert_eq!(file_name_arg.incoming_wires[0].source_node_id, string_id);
}

#[test]
fn resave_after_v6_load_emits_version_7() {
    let dir = tempdir().unwrap();
    let (path, _string_id, _export_id) = write_v6_export_file(dir.path());

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &path).expect("v6 load failed");

    let out = dir.path().join("resaved.cnnd");
    save_node_networks_to_file(&mut registry, &out, false, &HashMap::new()).unwrap();

    let raw = std::fs::read_to_string(&out).unwrap();
    let root: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(root["version"], json!(7), "re-save must emit version 7");
}

#[test]
fn v7_file_skips_the_migration_pass() {
    // A v7 file (no downgrade) must not invoke migrate_v6_to_v7.
    let dir = tempdir().unwrap();

    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let export_id = designer.add_node("export_atoms", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        export_id,
        Box::new(ExportAtomsData {
            file_name: "out.xyz".to_string(),
        }),
    );

    let path = dir.path().join("native_v7.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .unwrap();

    reset_migration_call_count();
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, path.to_str().unwrap()).expect("v7 load failed");
    assert_eq!(
        migration_call_count(),
        0,
        "a v7 file must not invoke migrate_v6_to_v7"
    );
}
