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
use std::cell::Cell;
use std::collections::HashSet;
use thiserror::Error;

/// Errors the v2→v3 pre-pass can surface to the load path. Both variants are
/// currently unconstructed: the helpers follow the design doc's
/// drop-with-dangling-wires policy (see "Error Policy"), so malformed shapes
/// are skipped silently with a `let Some(..) else { return Ok(()) }` guard
/// rather than rejected. `Json` is reserved for any helper that might later
/// call into `serde_json` (today migration operates purely on an
/// already-parsed `Value`); `MalformedStructure` is reserved for a future
/// condition severe enough to warrant hard-failing the load.
///
/// **Contract for future contributors adding an `Err` path:** the message must
/// locate the offending position — at minimum the network name, and where
/// applicable the node id (plus pin index for pin-level faults). The load
/// layer wraps this Display into an `io::Error` prefixed with
/// `"v2→v3 migration failed: "`, so the message is what the user sees.
#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("JSON error during migration: {0}")]
    Json(#[from] serde_json::Error),

    #[error("malformed v2 structure: {0}")]
    MalformedStructure(String),
}

// Test-only instrumentation: counts invocations of `migrate_v2_to_v3` so the
// test suite can verify the version dispatch actually skips the pre-pass for
// v3 files. Production code never reads this.
//
// Uses a `thread_local!` cell so each `#[test]` fn (which `cargo test` runs on
// its own dedicated thread) observes an independent counter — no cross-test
// contamination and no need for a serializing mutex. The load path is entirely
// synchronous, so the counter bump happens on the same thread that called
// `load_node_networks_from_file`.
thread_local! {
    static MIGRATION_CALL_COUNT: Cell<u64> = const { Cell::new(0) };
}

/// Returns the number of times [`migrate_v2_to_v3`] has been called on the
/// current thread. Tests call [`reset_migration_call_count`] before exercising
/// a load, then read this afterwards.
pub fn migration_call_count() -> u64 {
    MIGRATION_CALL_COUNT.with(|c| c.get())
}

/// Resets the current thread's [`migration_call_count`] counter. Tests call
/// this before a load that they want to observe in isolation.
pub fn reset_migration_call_count() {
    MIGRATION_CALL_COUNT.with(|c| c.set(0));
}

/// Top-level v2 → v3 pre-pass. Runs on the parsed JSON value before strict deserialization.
pub fn migrate_v2_to_v3(root: &mut Value) -> Result<(), MigrationError> {
    MIGRATION_CALL_COUNT.with(|c| c.set(c.get() + 1));
    rename_data_type_strings(root)?;
    rename_node_type_strings(root)?;
    drop_deleted_nodes_in_all_networks(root)?;
    adapt_primitives_lattice_to_structure_in_all_networks(root)?;
    synthesise_structure_for_atom_fill_in_all_networks(root)?;
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
// `atom_fill` split: v2 `atom_fill` becomes v3 `materialize`. When the v2 node
// had its `motif` or `motif_offset` pin wired, a `structure` (S) override node
// is synthesised carrying those wires; if the shape pin was also wired, a
// `get_structure` (G) and `with_structure` (W) pair splice the override into
// the shape chain immediately before `materialize`. This preserves the v2
// semantics that `atom_fill.motif` writes the motif into the materialised
// crystal — silently dropping it would replace the user's motif with the
// diamond default.
//
// See `doc/design_cnnd_migration_motif_fix.md` for the full rationale and the
// case A/B/C/D matrix:
//   `can_chain` = shape wire (v2 arg 0) is present.
//   `needs_S`   = motif (v2 arg 1) OR motif_offset (v2 arg 2) is present.
//   A: can_chain && needs_S    → G + S + W spliced into the shape chain.
//   B: can_chain && !needs_S   → rename + re-index only.
//   C: !can_chain && needs_S   → dangling S.
//   D: !can_chain && !needs_S  → rename + re-index only.
// ---------------------------------------------------------------------------

/// Describes one v2 `atom_fill` node and the synthesis required to migrate it
/// to v3. Collected in a read-only pre-pass so the mutation pass isn't fighting
/// serde_json's borrow rules.
struct AtomFillSplit {
    /// Id of the existing node being renamed from `atom_fill` to `materialize`.
    materialize_id: u64,
    /// `materialize`'s position, snapped to integers (anchor for the new G/S/W
    /// nodes, which sit to its left).
    materialize_position: [f64; 2],

    // The seven v2 atom_fill wires, lifted verbatim. An empty map is equivalent
    // to "unwired" — that's the signal we use for the can_chain / needs_S
    // predicates and for distinguishing case A / B / C / D below.
    shape_wire: serde_json::Map<String, Value>,
    motif_wire: serde_json::Map<String, Value>,
    motif_offset_wire: serde_json::Map<String, Value>,
    passivate_wire: serde_json::Map<String, Value>,
    rm_single_wire: serde_json::Map<String, Value>,
    surf_recon_wire: serde_json::Map<String, Value>,
    invert_phase_wire: serde_json::Map<String, Value>,

    /// Whether v2 arg 0 (shape) is wired. Distinguishes A/B from C/D.
    can_chain: bool,
    /// Whether v2 arg 1 (motif) or v2 arg 2 (motif_offset) is wired.
    /// Broadened beyond just motif so a user who only wired motif_offset
    /// doesn't lose their offset wire to step 4's re-index — see the design's
    /// "Why `needs_S` includes the motif_offset wire" section.
    needs_s: bool,

    /// Allocated id for the synthesized `get_structure` node (case A only).
    g_id: Option<u64>,
    /// Allocated id for the synthesized `structure` override node (cases A and C).
    s_id: Option<u64>,
    /// Allocated id for the synthesized `with_structure` node (case A only).
    w_id: Option<u64>,
}

/// Iterates every network under `root` and applies [`synthesise_structure_for_atom_fill`]
/// to each one.
fn synthesise_structure_for_atom_fill_in_all_networks(
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
        synthesise_structure_for_atom_fill(network)?;
    }
    Ok(())
}

/// Replaces each v2 `atom_fill` node in a single network with a v3 `materialize` node,
/// optionally splicing a `get_structure` + `structure` + `with_structure` triplet into the
/// shape chain to preserve the user's motif / motif_offset wires.
///
/// Always:
/// - the existing node is renamed to `materialize` (both `node_type_name` and the `data_type`
///   tag); its arguments are re-indexed to the v3 layout and its `NodeData` loses the
///   `motif_offset` field (carries over `parameter_element_value_definition`,
///   `hydrogen_passivation`, `remove_single_bond_atoms_before_passivation`,
///   `surface_reconstruction`, `invert_phase`).
///
/// Per case (see module-level comment for the matrix):
/// - **A** (shape + motif/offset wired): synthesises `G = get_structure`, `S = structure`,
///   `W = with_structure`. The shape wire is fanned into both the original chain (cloned
///   into `W.shape` and `G.input`) and `G` extracts its Structure; `S` overrides the motif
///   / motif_offset on top of that base; `W` puts the patched Structure back onto the
///   Blueprint flowing into `materialize`.
/// - **B** (shape wired, motif/offset unwired): rename + re-index only.
/// - **C** (shape unwired, motif/offset wired): synthesises `S` only — dangling, since
///   there is no shape chain to splice it into. The file was already invalid in v2 in
///   this state, so a dangling `S` is strictly better than dropping the wires.
/// - **D** (nothing wired beyond defaults): rename + re-index only.
fn synthesise_structure_for_atom_fill(network_json: &mut Value) -> Result<(), MigrationError> {
    let Some(next_id_val) = network_json.get("next_node_id").and_then(|v| v.as_u64()) else {
        return Ok(());
    };
    let mut next_id = next_id_val;

    let mut splits: Vec<AtomFillSplit> = Vec::new();
    if let Some(nodes) = network_json.get("nodes").and_then(|v| v.as_array()) {
        for node in nodes {
            let Some(type_name) = node.get("node_type_name").and_then(|v| v.as_str()) else {
                continue;
            };
            if type_name != "atom_fill" {
                continue;
            }
            let Some(id) = node.get("id").and_then(|v| v.as_u64()) else {
                continue;
            };

            // v2 saved files can have fewer arguments than the declared 7 — the arg-count
            // repair on load pads them, but serialization writes the padded form only if
            // nothing truncated it first. Read each index defensively; absent positions
            // contribute an empty wire map (equivalent to an unwired pin).
            let args = node.get("arguments").and_then(|v| v.as_array());
            let pick_wire = |idx: usize| -> serde_json::Map<String, Value> {
                args.and_then(|a| a.get(idx))
                    .and_then(|arg| arg.get("argument_output_pins"))
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default()
            };

            let shape_wire = pick_wire(0);
            let motif_wire = pick_wire(1);
            let motif_offset_wire = pick_wire(2);

            let can_chain = !shape_wire.is_empty();
            let needs_s = !motif_wire.is_empty() || !motif_offset_wire.is_empty();

            // Snap the new-node positions to integers for the same reason as the primitive
            // adaptation pass: f64 subtraction near a fractional bit pattern occasionally
            // round-trips to a neighbouring ULP through serde_json's shortest-decimal
            // emit/parse, breaking `cnnd_roundtrip_test`. Integer positions round-trip
            // exactly.
            let materialize_position = node
                .get("position")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    let x = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    [x.round(), y.round()]
                })
                .unwrap_or([0.0, 0.0]);

            // Allocate ids in the order documented in the design (G, S, W) so the
            // produced JSON is deterministic across runs and double-migration is byte-
            // identical on the second call.
            let (g_id, s_id, w_id) = match (needs_s, can_chain) {
                (true, true) => {
                    let g = next_id;
                    let s = next_id + 1;
                    let w = next_id + 2;
                    next_id += 3;
                    (Some(g), Some(s), Some(w))
                }
                (true, false) => {
                    let s = next_id;
                    next_id += 1;
                    (None, Some(s), None)
                }
                (false, _) => (None, None, None),
            };

            splits.push(AtomFillSplit {
                materialize_id: id,
                materialize_position,
                shape_wire,
                motif_wire,
                motif_offset_wire,
                passivate_wire: pick_wire(3),
                rm_single_wire: pick_wire(4),
                surf_recon_wire: pick_wire(5),
                invert_phase_wire: pick_wire(6),
                can_chain,
                needs_s,
                g_id,
                s_id,
                w_id,
            });
        }
    }

    if splits.is_empty() {
        return Ok(());
    }

    if let Some(n) = network_json.get_mut("next_node_id") {
        *n = Value::from(next_id);
    }

    let Some(nodes) = network_json.get_mut("nodes").and_then(|v| v.as_array_mut()) else {
        return Ok(());
    };

    // Rewrite each existing atom_fill node into a materialize node. The wire-swap is
    // explicit — the validator's argument-count repair would otherwise truncate to 5 args
    // and leave motif / motif_offset sitting at positions 1 and 2, where v3 `materialize`
    // expects Bool pins.
    for split in &splits {
        let Some(node) = nodes
            .iter_mut()
            .find(|n| n.get("id").and_then(|v| v.as_u64()) == Some(split.materialize_id))
        else {
            continue;
        };
        if let Some(Value::String(n)) = node.get_mut("node_type_name") {
            *n = "materialize".to_string();
        }
        if let Some(Value::String(t)) = node.get_mut("data_type") {
            *t = "materialize".to_string();
        }

        // In case A the materialize.shape pin must point at W — replacing the v2 shape
        // chain. In every other case the original shape wire (which may itself be empty)
        // becomes materialize.shape unchanged.
        let shape_into_materialize: serde_json::Map<String, Value> = if let Some(w) = split.w_id {
            let mut m = serde_json::Map::new();
            m.insert(w.to_string(), Value::from(0));
            m
        } else {
            split.shape_wire.clone()
        };

        if let Some(obj) = node.as_object_mut() {
            obj.insert(
                "arguments".to_string(),
                serde_json::json!([
                    { "argument_output_pins": Value::Object(shape_into_materialize) },
                    { "argument_output_pins": Value::Object(split.passivate_wire.clone()) },
                    { "argument_output_pins": Value::Object(split.rm_single_wire.clone()) },
                    { "argument_output_pins": Value::Object(split.surf_recon_wire.clone()) },
                    { "argument_output_pins": Value::Object(split.invert_phase_wire.clone()) },
                ]),
            );
        }
        // Translate AtomFillData → MaterializeData: drop the `motif_offset` field; the rest
        // (parameter_element_value_definition, hydrogen_passivation,
        // remove_single_bond_atoms_before_passivation, surface_reconstruction, invert_phase)
        // carries over verbatim.
        if let Some(data) = node.get_mut("data").and_then(|v| v.as_object_mut()) {
            data.remove("motif_offset");
        }
    }

    // Append the synthesised G / S / W nodes (the cases that do nothing here are B and D).
    // Order: G, then S, then W — matches id allocation order and keeps the output array
    // deterministic for snapshot / round-trip comparisons.
    for split in splits {
        if !split.needs_s {
            continue;
        }
        let m_pos = split.materialize_position;

        // G (case A only): wire arg 0 to a clone of the original shape wire.
        if let (Some(g), true) = (split.g_id, split.can_chain) {
            let g_pos = [m_pos[0] - 330.0, m_pos[1] - 40.0];
            nodes.push(build_get_structure_node(g, g_pos, split.shape_wire.clone()));
        }

        // S (cases A and C): wire arg 0 to G if chained, else leave empty (dangling base);
        // wire arg 2 ← motif_wire and arg 3 ← motif_offset_wire.
        let s_id = split.s_id.expect("needs_s implies s_id is allocated");
        let s_pos = [m_pos[0] - 200.0, m_pos[1] - 40.0];
        let mut s_base_wire = serde_json::Map::new();
        if let Some(g) = split.g_id {
            s_base_wire.insert(g.to_string(), Value::from(0));
        }
        nodes.push(build_structure_override_node(
            s_id,
            s_pos,
            s_base_wire,
            split.motif_wire,
            split.motif_offset_wire,
        ));

        // W (case A only): wire arg 0 to a clone of the original shape wire and arg 1 to S.
        if let (Some(w), true) = (split.w_id, split.can_chain) {
            let w_pos = [m_pos[0] - 90.0, m_pos[1]];
            let mut w_struct_wire = serde_json::Map::new();
            w_struct_wire.insert(s_id.to_string(), Value::from(0));
            nodes.push(build_with_structure_node(
                w,
                w_pos,
                split.shape_wire.clone(),
                w_struct_wire,
            ));
        }
    }

    Ok(())
}

fn build_get_structure_node(
    id: u64,
    position: [f64; 2],
    input_wire: serde_json::Map<String, Value>,
) -> Value {
    serde_json::json!({
        "id": id,
        "node_type_name": "get_structure",
        "position": [position[0], position[1]],
        "arguments": [
            { "argument_output_pins": Value::Object(input_wire) },
        ],
        "data_type": "get_structure",
        "data": {}
    })
}

fn build_structure_override_node(
    id: u64,
    position: [f64; 2],
    base_wire: serde_json::Map<String, Value>,
    motif_wire: serde_json::Map<String, Value>,
    motif_offset_wire: serde_json::Map<String, Value>,
) -> Value {
    serde_json::json!({
        "id": id,
        "node_type_name": "structure",
        "position": [position[0], position[1]],
        "arguments": [
            { "argument_output_pins": Value::Object(base_wire) },
            { "argument_output_pins": {} },
            { "argument_output_pins": Value::Object(motif_wire) },
            { "argument_output_pins": Value::Object(motif_offset_wire) },
        ],
        "data_type": "structure",
        "data": {}
    })
}

fn build_with_structure_node(
    id: u64,
    position: [f64; 2],
    shape_wire: serde_json::Map<String, Value>,
    structure_wire: serde_json::Map<String, Value>,
) -> Value {
    serde_json::json!({
        "id": id,
        "node_type_name": "with_structure",
        "position": [position[0], position[1]],
        "arguments": [
            { "argument_output_pins": Value::Object(shape_wire) },
            { "argument_output_pins": Value::Object(structure_wire) },
        ],
        "data_type": "with_structure",
        "data": {}
    })
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
fn adapt_primitives_lattice_to_structure(network_json: &mut Value) -> Result<(), MigrationError> {
    let Some(next_id_val) = network_json.get("next_node_id").and_then(|v| v.as_u64()) else {
        return Ok(());
    };
    let mut next_id = next_id_val;

    // Index node types by id so we can peek at a wire's source type. Used below to
    // keep this pass idempotent: a primitive whose pin already points at a
    // `structure` node has been adapted by a previous run and must not be
    // re-adapted. (In production the version dispatch prevents re-entry, but the
    // test suite's double-migration idempotence check exercises this path
    // directly — per the design doc's "class-of-bug guard".)
    let node_type_by_id: std::collections::HashMap<u64, String> = network_json
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| {
                    let id = n.get("id").and_then(|v| v.as_u64())?;
                    let name = n.get("node_type_name").and_then(|v| v.as_str())?;
                    Some((id, name.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();

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
            // Idempotence guard: if every source on this pin already points at a
            // `structure` node, this primitive was adapted on a prior run.
            // Skip — re-adapting would chain a second adapter behind the first.
            let all_sources_already_structure = wire_obj.keys().all(|k| {
                k.parse::<u64>()
                    .ok()
                    .and_then(|src_id| node_type_by_id.get(&src_id))
                    .map(|n| n == "structure")
                    .unwrap_or(false)
            });
            if all_sources_already_structure {
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
        let Some(node) = nodes
            .iter_mut()
            .find(|n| n.get("id").and_then(|v| v.as_u64()) == Some(adaptation.primitive_id))
        else {
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
        map.insert(adaptation.new_structure_node_id.to_string(), Value::from(0));
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
