//! Migration of `.cnnd` save files from serialization version 2 (pre-lattice-space-refactoring)
//! to version 3 (post-refactoring).
//!
//! Design: see `doc/design_cnnd_migration_v2_to_v3.md`.
//!
//! The entry point [`migrate_v2_to_v3`] operates on a `serde_json::Value` before the file
//! is deserialized into the strict `SerializableNodeTypeRegistryNetworks` struct. This lets
//! it rewrite type-name strings, DataType strings, and synthesize new nodes where the shape
//! of the network itself has changed — things serde-level compat cannot express.

use serde_json::Value;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("JSON error during migration: {0}")]
    Json(#[from] serde_json::Error),

    #[error("malformed v2 structure: {0}")]
    MalformedStructure(String),
}

/// Top-level v2 → v3 pre-pass. Runs on the parsed JSON value before strict deserialization.
pub fn migrate_v2_to_v3(root: &mut Value) -> Result<(), MigrationError> {
    rename_data_type_strings(root)?;
    rename_node_type_strings(root)?;
    drop_deleted_nodes_in_all_networks(root)?;
    adapt_primitives_lattice_to_structure_in_all_networks(root)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// DataType renames
// ---------------------------------------------------------------------------

/// Rewrites the v2 DataType names (`Geometry`, `UnitCell`, `Atomic`) to their v3 counterparts
/// (`Blueprint`, `LatticeVecs`, `Molecule`) everywhere a DataType string can appear in a v2
/// save file: the custom-network type signature (`parameters`, `output_pins`, legacy
/// `output_type`), and four per-node `NodeData` fields that embed a DataType
/// (`parameter`, `expr`, `map`, `sequence`). Array wrapping is preserved in both its string
/// form (`"[Atomic]"`) and its serde-enum form (`{"Array": "Atomic"}`).
///
/// See the "Where DataType strings appear in saved v2 files" and "`Atomic` needs a different
/// treatment — not a rename" subsections of the design doc for why these are the only
/// locations and why `Atomic` maps to the concrete `Molecule` rather than the abstract
/// `HasAtoms`.
fn rename_data_type_strings(root: &mut Value) -> Result<(), MigrationError> {
    let Some(node_networks) = root.get_mut("node_networks").and_then(|v| v.as_array_mut()) else {
        return Ok(());
    };
    for entry in node_networks {
        let Some(entry_arr) = entry.as_array_mut() else {
            continue;
        };
        let Some(network) = entry_arr.get_mut(1) else {
            continue;
        };
        rename_data_types_in_network(network)?;
    }
    Ok(())
}

fn rename_data_types_in_network(network: &mut Value) -> Result<(), MigrationError> {
    // Custom-network type signature: parameters, output_pins, legacy output_type.
    if let Some(node_type) = network.get_mut("node_type") {
        if let Some(params) = node_type
            .get_mut("parameters")
            .and_then(|v| v.as_array_mut())
        {
            for p in params {
                if let Some(dt) = p.get_mut("data_type") {
                    rename_data_type_string_in_value(dt);
                }
            }
        }
        if let Some(output_pins) = node_type
            .get_mut("output_pins")
            .and_then(|v| v.as_array_mut())
        {
            for op in output_pins {
                if let Some(dt) = op.get_mut("data_type") {
                    rename_data_type_string_in_value(dt);
                }
            }
        }
        if let Some(ot) = node_type.get_mut("output_type") {
            if !ot.is_null() {
                rename_data_type_string_in_value(ot);
            }
        }
    }

    // Per-node NodeData fields that embed a DataType (four node types).
    if let Some(nodes) = network.get_mut("nodes").and_then(|v| v.as_array_mut()) {
        for node in nodes {
            rename_data_types_in_node(node);
        }
    }
    Ok(())
}

fn rename_data_types_in_node(node: &mut Value) {
    let Some(node_type_name) = node
        .get("node_type_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    else {
        return;
    };
    let Some(data) = node.get_mut("data") else {
        return;
    };
    match node_type_name.as_str() {
        "parameter" => {
            if let Some(dt) = data.get_mut("data_type") {
                rename_data_type_in_value(dt);
            }
            if let Some(dts) = data.get_mut("data_type_str") {
                if !dts.is_null() {
                    rename_data_type_string_in_value(dts);
                }
            }
        }
        "expr" => {
            if let Some(params) = data.get_mut("parameters").and_then(|v| v.as_array_mut()) {
                for p in params {
                    if let Some(dt) = p.get_mut("data_type") {
                        rename_data_type_in_value(dt);
                    }
                    if let Some(dts) = p.get_mut("data_type_str") {
                        if !dts.is_null() {
                            rename_data_type_string_in_value(dts);
                        }
                    }
                }
            }
        }
        "map" => {
            if let Some(dt) = data.get_mut("input_type") {
                rename_data_type_in_value(dt);
            }
            if let Some(dt) = data.get_mut("output_type") {
                rename_data_type_in_value(dt);
            }
        }
        "sequence" => {
            if let Some(dt) = data.get_mut("element_type") {
                rename_data_type_in_value(dt);
            }
        }
        _ => {}
    }
}

/// Rewrites a JSON value that came from a `String`-typed DataType field
/// (e.g. `SerializableOutputPin.data_type`, `SerializableParameter.data_type`,
/// the legacy `output_type: Option<String>`, or `data_type_str`). The value
/// is expected to be a JSON string in the `DataType::Display` format, which
/// wraps arrays as `"[…]"`.
fn rename_data_type_string_in_value(v: &mut Value) {
    if let Value::String(s) = v {
        let renamed = rename_data_type_display_string(s);
        if renamed != *s {
            *v = Value::String(renamed);
        }
    }
}

/// Rewrites a JSON value that came from a `DataType`-typed field with serde's
/// default enum encoding: primitive variants are plain strings, `Array(inner)`
/// is `{"Array": <inner>}`. The inner value recurses by the same rule.
/// Plain strings on this path also get rewritten by the same `Display`-form
/// rule since primitive variants share that spelling.
fn rename_data_type_in_value(v: &mut Value) {
    match v {
        Value::String(s) => {
            let renamed = rename_data_type_display_string(s);
            if renamed != *s {
                *v = Value::String(renamed);
            }
        }
        Value::Object(map) => {
            if let Some(inner) = map.get_mut("Array") {
                rename_data_type_in_value(inner);
            }
        }
        _ => {}
    }
}

/// Applies the primitive rename table to a DataType spelled in `Display` form
/// (arrays shown as `"[…]"`). The bracket nesting is preserved verbatim.
/// Unknown names (including already-v3 names) pass through unchanged.
fn rename_data_type_display_string(s: &str) -> String {
    let bytes = s.as_bytes();
    let opening = bytes.iter().take_while(|&&b| b == b'[').count();
    let closing = bytes.iter().rev().take_while(|&&b| b == b']').count();
    let depth = opening.min(closing);
    if depth * 2 >= s.len() {
        return s.to_string();
    }
    let core = &s[depth..s.len() - depth];
    let renamed_core = match core {
        "Atomic" => "Molecule",
        "Geometry" => "Blueprint",
        "UnitCell" => "LatticeVecs",
        _ => return s.to_string(),
    };
    format!("{}{}{}", "[".repeat(depth), renamed_core, "]".repeat(depth))
}

// ---------------------------------------------------------------------------
// Node type renames
// ---------------------------------------------------------------------------

/// Rewrites every `node_type_name` string reference — on each `SerializableNode`,
/// on each network's own `node_type.name`, on the network tuple key, and on each
/// node's `data_type` tag (which mirrors the node-type name) — using the v2 → v3
/// node rename table. References to still-v3 names pass through unchanged.
///
/// Keys and self-names are renamed together so custom-network lookups stay
/// consistent with the renamed reference strings.
fn rename_node_type_strings(root: &mut Value) -> Result<(), MigrationError> {
    let Some(node_networks) = root.get_mut("node_networks").and_then(|v| v.as_array_mut()) else {
        return Ok(());
    };
    for entry in node_networks {
        let Some(entry_arr) = entry.as_array_mut() else {
            continue;
        };
        if let Some(Value::String(key)) = entry_arr.get_mut(0) {
            if let Some(new) = rename_node_type_name(key) {
                *key = new;
            }
        }
        let Some(network) = entry_arr.get_mut(1) else {
            continue;
        };
        if let Some(node_type) = network.get_mut("node_type") {
            if let Some(Value::String(name)) = node_type.get_mut("name") {
                if let Some(new) = rename_node_type_name(name) {
                    *name = new;
                }
            }
        }
        if let Some(nodes) = network.get_mut("nodes").and_then(|v| v.as_array_mut()) {
            for node in nodes {
                if let Some(Value::String(n)) = node.get_mut("node_type_name") {
                    if let Some(new) = rename_node_type_name(n) {
                        *n = new;
                    }
                }
                // `SerializableNode.data_type` is the tag used to dispatch the
                // polymorphic NodeData loader; it mirrors `node_type_name` and
                // must track it through the rename.
                if let Some(Value::String(t)) = node.get_mut("data_type") {
                    if let Some(new) = rename_node_type_name(t) {
                        *t = new;
                    }
                }
            }
        }
    }
    Ok(())
}

fn rename_node_type_name(s: &str) -> Option<String> {
    // `lattice_symop` is intentionally absent — it's deleted in the next phase,
    // not renamed.
    match s {
        "unit_cell" => Some("lattice_vecs".to_string()),
        "atom_lmove" | "lattice_move" => Some("structure_move".to_string()),
        "atom_lrot" | "lattice_rot" => Some("structure_rot".to_string()),
        "atom_move" => Some("free_move".to_string()),
        "atom_rot" => Some("free_rot".to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Deleted-node drop
// ---------------------------------------------------------------------------

/// Node types whose implementations were removed in v3. Instances of these types in a v2
/// file are dropped by the migration; any wires referencing them on downstream nodes are
/// disconnected, leaving the consuming argument empty so network validation surfaces the
/// missing input to the user on first open.
const DELETED_NODE_TYPES: &[&str] = &["atom_trans", "lattice_symop"];

fn is_deleted_node_type(name: &str) -> bool {
    DELETED_NODE_TYPES.contains(&name)
}

/// Iterates every network under `root` and applies [`drop_deleted_nodes`] to each one.
fn drop_deleted_nodes_in_all_networks(root: &mut Value) -> Result<(), MigrationError> {
    let Some(node_networks) = root.get_mut("node_networks").and_then(|v| v.as_array_mut()) else {
        return Ok(());
    };
    for entry in node_networks {
        let Some(entry_arr) = entry.as_array_mut() else {
            continue;
        };
        let Some(network) = entry_arr.get_mut(1) else {
            continue;
        };
        drop_deleted_nodes(network)?;
    }
    Ok(())
}

/// Removes nodes of deleted v2 types (`atom_trans`, `lattice_symop`) from a single network
/// and severs every stored reference to them:
/// - drops the node entries themselves;
/// - disconnects downstream wires by removing the deleted node's id from every remaining
///   node's `argument_output_pins` (the argument slot itself is kept in place — empty —
///   so the validator reports a missing required input, which is the intended signal);
/// - clears `return_node_id` if it pointed at a deleted node;
/// - drops entries in `displayed_node_ids` and `displayed_output_pins` referring to
///   deleted ids, so the downstream deserializer doesn't trip over dangling references.
///
/// Upstream wires from a deleted node vanish for free: they lived inside the deleted
/// node's own `arguments` vector, which is removed along with the node.
fn drop_deleted_nodes(network_json: &mut Value) -> Result<(), MigrationError> {
    let deleted_ids = collect_deleted_node_ids(network_json);
    if deleted_ids.is_empty() {
        return Ok(());
    }

    if let Some(nodes) = network_json.get_mut("nodes").and_then(|v| v.as_array_mut()) {
        nodes.retain(|node| {
            let name = node.get("node_type_name").and_then(|v| v.as_str());
            !matches!(name, Some(n) if is_deleted_node_type(n))
        });

        for node in nodes.iter_mut() {
            let Some(args) = node.get_mut("arguments").and_then(|v| v.as_array_mut()) else {
                continue;
            };
            for arg in args {
                let Some(pins) = arg
                    .get_mut("argument_output_pins")
                    .and_then(|v| v.as_object_mut())
                else {
                    continue;
                };
                pins.retain(|key, _| {
                    key.parse::<u64>()
                        .map(|id| !deleted_ids.contains(&id))
                        .unwrap_or(true)
                });
            }
        }
    }

    let return_was_deleted = network_json
        .get("return_node_id")
        .and_then(|v| v.as_u64())
        .map(|id| deleted_ids.contains(&id))
        .unwrap_or(false);
    if return_was_deleted {
        if let Some(obj) = network_json.as_object_mut() {
            obj.insert("return_node_id".to_string(), Value::Null);
        }
    }

    retain_display_entries(network_json, "displayed_node_ids", &deleted_ids);
    retain_display_entries(network_json, "displayed_output_pins", &deleted_ids);

    Ok(())
}

fn collect_deleted_node_ids(network_json: &Value) -> HashSet<u64> {
    let mut ids = HashSet::new();
    let Some(nodes) = network_json.get("nodes").and_then(|v| v.as_array()) else {
        return ids;
    };
    for node in nodes {
        let Some(name) = node.get("node_type_name").and_then(|v| v.as_str()) else {
            continue;
        };
        if !is_deleted_node_type(name) {
            continue;
        }
        if let Some(id) = node.get("id").and_then(|v| v.as_u64()) {
            ids.insert(id);
        }
    }
    ids
}

/// Drops entries from a `Vec<(u64, …)>`-shaped field (first tuple element is the node id)
/// whose id is in `deleted_ids`. Shared by `displayed_node_ids` and `displayed_output_pins`.
fn retain_display_entries(network_json: &mut Value, field: &str, deleted_ids: &HashSet<u64>) {
    let Some(arr) = network_json.get_mut(field).and_then(|v| v.as_array_mut()) else {
        return;
    };
    arr.retain(|entry| {
        entry
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_u64())
            .map(|id| !deleted_ids.contains(&id))
            .unwrap_or(true)
    });
}

// ---------------------------------------------------------------------------
// Stubs for later phases (scaffolding present so the module layout is stable)
// ---------------------------------------------------------------------------

/// For each `atom_fill` node `N` in a network, synthesize a new `structure` source node `S`,
/// move motif/offset wires from `N` to `S`, re-index `N`'s remaining args to the v3
/// `materialize` layout, rename `N` to `materialize`, and translate its NodeData.
#[allow(dead_code)]
fn synthesise_structure_for_atom_fill(_network_json: &mut Value) -> Result<(), MigrationError> {
    Ok(())
}

/// Primitives whose v2 `unit_cell: LatticeVecs` input became a v3 `structure: Structure`
/// input. The tuple's second element is the argument index of that pin (stable between v2
/// and v3). Update this table when a new primitive ships.
const PRIMITIVE_LATTICE_PIN: &[(&str, usize)] = &[
    ("cuboid", 2),
    ("sphere", 2),
    ("extrude", 1),
    ("half_space", 0),
    ("drawing_plane", 0),
    ("facet_shell", 0),
];

fn primitive_lattice_pin_index(node_type_name: &str) -> Option<usize> {
    PRIMITIVE_LATTICE_PIN
        .iter()
        .find(|(name, _)| *name == node_type_name)
        .map(|(_, idx)| *idx)
}

/// Iterates every network under `root` and applies [`adapt_primitives_lattice_to_structure`]
/// to each one.
fn adapt_primitives_lattice_to_structure_in_all_networks(
    root: &mut Value,
) -> Result<(), MigrationError> {
    let Some(node_networks) = root.get_mut("node_networks").and_then(|v| v.as_array_mut()) else {
        return Ok(());
    };
    for entry in node_networks {
        let Some(entry_arr) = entry.as_array_mut() else {
            continue;
        };
        let Some(network) = entry_arr.get_mut(1) else {
            continue;
        };
        adapt_primitives_lattice_to_structure(network)?;
    }
    Ok(())
}

/// Describes one primitive whose old `lattice_vecs` input held a wire that must be rerouted
/// through a newly-synthesized `structure` adapter. Collected in a read-only pre-pass so the
/// subsequent mutation pass doesn't fight serde_json's borrow rules.
struct PrimitiveAdaptation {
    /// The primitive node's id (lookup key for the mutation pass).
    primitive_id: u64,
    /// Argument index on the primitive that holds the old `unit_cell` / new `structure` pin.
    primitive_arg_index: usize,
    /// The wire map (`argument_output_pins`) lifted off the primitive — becomes the adapter's
    /// `lattice_vecs` arg (index 1) verbatim.
    original_wire: serde_json::Map<String, Value>,
    /// Pre-allocated id for the new `structure` adapter node.
    new_structure_node_id: u64,
    /// Position to place the adapter at, offset left of the primitive so auto-layout on the
    /// next open is not disrupted.
    new_structure_position: [f64; 2],
}

/// For each primitive node (cuboid, sphere, extrude, half_space, drawing_plane, facet_shell)
/// whose v2 `unit_cell` input held a live wire, insert a synthesized `structure` adapter
/// node between the source and the primitive's new `structure` input.
///
/// The adapter's `lattice_vecs` input (arg 1) takes the original wire; its output (pin 0)
/// feeds the primitive's `structure` input. The adapter's other inputs (`structure`, `motif`,
/// `motif_offset`) are left unwired so their diamond defaults apply — this preserves the v2
/// semantics where the primitive's lattice context came solely from the `unit_cell` wire.
///
/// Runs after the deleted-node drop so primitives that were wired to `lattice_symop` see
/// their pin as already unwired and are correctly skipped.
fn adapt_primitives_lattice_to_structure(
    network_json: &mut Value,
) -> Result<(), MigrationError> {
    let Some(next_id_val) = network_json.get("next_node_id").and_then(|v| v.as_u64()) else {
        return Ok(());
    };
    let mut next_id = next_id_val;

    let mut adaptations: Vec<PrimitiveAdaptation> = Vec::new();
    if let Some(nodes) = network_json.get("nodes").and_then(|v| v.as_array()) {
        for node in nodes {
            let Some(type_name) = node.get("node_type_name").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(pin_index) = primitive_lattice_pin_index(type_name) else {
                continue;
            };
            let Some(id) = node.get("id").and_then(|v| v.as_u64()) else {
                continue;
            };
            let Some(args) = node.get("arguments").and_then(|v| v.as_array()) else {
                continue;
            };
            let Some(arg) = args.get(pin_index) else {
                continue;
            };
            let Some(wire_obj) = arg.get("argument_output_pins").and_then(|v| v.as_object()) else {
                continue;
            };
            if wire_obj.is_empty() {
                continue;
            }
            // Snap the adapter position to integers. The primitive's real-world
            // fractional position minus a fixed offset occasionally lands on an f64
            // bit pattern whose `serde_json` round-trip flips the last ULP (the
            // emitted decimal is shortest-round-trip, but the parser resolves it to
            // a neighbour), breaking `cnnd_roundtrip_test`. Integer positions are
            // exact in f64 and always round-trip.
            let position = node
                .get("position")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    let x = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    [(x - 150.0).round(), y.round()]
                })
                .unwrap_or([-150.0, 0.0]);
            adaptations.push(PrimitiveAdaptation {
                primitive_id: id,
                primitive_arg_index: pin_index,
                original_wire: wire_obj.clone(),
                new_structure_node_id: next_id,
                new_structure_position: position,
            });
            next_id += 1;
        }
    }

    if adaptations.is_empty() {
        return Ok(());
    }

    if let Some(n) = network_json.get_mut("next_node_id") {
        *n = Value::from(next_id);
    }

    let Some(nodes) = network_json.get_mut("nodes").and_then(|v| v.as_array_mut()) else {
        return Ok(());
    };

    // Rewire primitives to point at the synthesized adapters (one wire-swap each).
    for adaptation in &adaptations {
        let Some(node) = nodes.iter_mut().find(|n| {
            n.get("id").and_then(|v| v.as_u64()) == Some(adaptation.primitive_id)
        }) else {
            continue;
        };
        let Some(arg) = node
            .get_mut("arguments")
            .and_then(|v| v.as_array_mut())
            .and_then(|arr| arr.get_mut(adaptation.primitive_arg_index))
        else {
            continue;
        };
        let Some(map) = arg
            .get_mut("argument_output_pins")
            .and_then(|v| v.as_object_mut())
        else {
            continue;
        };
        map.clear();
        map.insert(
            adaptation.new_structure_node_id.to_string(),
            Value::from(0),
        );
    }

    // Append the synthesized `structure` adapter nodes. Keys and shape mirror
    // `SerializableNode` / `StructureData`; `custom_name` is omitted so the loader
    // auto-names the adapter (e.g. "structure1"), matching how a freshly-added
    // node would look to the user.
    for adaptation in adaptations {
        nodes.push(build_structure_adapter_node(
            adaptation.new_structure_node_id,
            adaptation.new_structure_position,
            Value::Object(adaptation.original_wire),
        ));
    }

    Ok(())
}

fn build_structure_adapter_node(id: u64, position: [f64; 2], lattice_wire: Value) -> Value {
    serde_json::json!({
        "id": id,
        "node_type_name": "structure",
        "position": [position[0], position[1]],
        "arguments": [
            { "argument_output_pins": {} },
            { "argument_output_pins": lattice_wire },
            { "argument_output_pins": {} },
            { "argument_output_pins": {} },
        ],
        "data_type": "structure",
        "data": {}
    })
}
