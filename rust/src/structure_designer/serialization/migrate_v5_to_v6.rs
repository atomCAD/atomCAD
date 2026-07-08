//! Migration of `.cnnd` save files from serialization version 5 to version 6.
//!
//! Design: see `doc/design_degree_angle_inputs.md` (Phase 3).
//!
//! In v6 `free_rot`'s angle input switches from radians to degrees (issue
//! #384): the stored `FreeRotData` field is renamed `angle` → `angle_degrees`
//! and its value converted with `f64::to_degrees`, and any wire feeding the
//! `angle` pin (argument index 1) has a synthesised `degrees(x)` expr node
//! inserted on it so the *radian* value the upstream node produces is turned
//! into degrees before it reaches the now-degree-interpreting `free_rot`.
//!
//! The wire-splicing is **unconditional** — even when the source is a plain
//! `float` node (a uniform rule is simpler than a prettier special case, and
//! the inserted `degrees(x)` node is visible and user-deletable).
//!
//! Two properties matter and are covered by tests:
//! - **Recursion:** the pass walks every network *and every zone body at every
//!   depth*, keyed on a node's `zone` field being present — NOT on a hardcoded
//!   HOF-name list. Zone bodies live on any zone-bearing node (`map`/`filter`/
//!   `fold`/`foreach` but also `closure`/`zip_with`), all shipped pre-v6, so a
//!   name list would silently skip the latter two and a `free_rot` inside such
//!   a body would keep its old `data.angle` key and fail strict
//!   deserialization after the field rename.
//! - **Idempotency + determinism:** every per-`free_rot` action is gated on the
//!   radian-era `data.angle` key still being present, which no longer exists
//!   after the rename — so a re-run is a no-op and no second expr node is ever
//!   synthesised. Synthesised ids are allocated in `free_rot`-id-sorted order
//!   (one angle pin per `free_rot`, so the key is unique) for byte-identical
//!   output across runs.
//!
//! Frozen at the v6 release: hardcoded node-type names (`free_rot`, the
//! synthesised `expr`), the `angle` pin index (1), and the `degrees(x)`
//! expression string — never read from the live registry.

use serde_json::Value;
use std::cell::Cell;

use super::migrate_v2_to_v3::MigrationError;

// Test-only instrumentation: counts invocations of `migrate_v5_to_v6` so the
// test suite can verify the version dispatch skips the pre-pass for v6 files.
// Production code never reads this. Mirrors the v2→v3 / v3→v4 counters; see
// `migrate_v2_to_v3` for the thread-locality rationale.
thread_local! {
    static MIGRATION_CALL_COUNT: Cell<u64> = const { Cell::new(0) };
}

/// Returns the number of times [`migrate_v5_to_v6`] has been called on the
/// current thread.
pub fn migration_call_count() -> u64 {
    MIGRATION_CALL_COUNT.with(|c| c.get())
}

/// Resets the current thread's [`migration_call_count`] counter.
pub fn reset_migration_call_count() {
    MIGRATION_CALL_COUNT.with(|c| c.set(0));
}

/// Horizontal offset for the synthesised `degrees(x)` expr node, placed to the
/// **left** of the `free_rot` it feeds (`expr_width` + a small gap). Frozen at
/// the v6 release; matches the integer-snapped placement convention of v3→v4.
const EXPR_NODE_OFFSET_X: f64 = 160.0;

/// Argument index of `free_rot`'s `angle` pin. Pin 0 is `input`, pin 1 is
/// `angle`. Frozen at the v6 release.
const FREE_ROT_ANGLE_PIN: usize = 1;

/// Top-level v5 → v6 pre-pass. Runs on the parsed JSON value before strict
/// deserialization. Walks every network (and every nested zone body) and
/// migrates its `free_rot` nodes.
pub fn migrate_v5_to_v6(root: &mut Value) -> Result<(), MigrationError> {
    MIGRATION_CALL_COUNT.with(|c| c.set(c.get() + 1));

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
        migrate_scope(network)?;
    }

    Ok(())
}

/// Migrate one scope — a top-level network OR a zone body — then recurse into
/// every zone body nested within it. A body is any node's non-null `zone`
/// field; keying on the field (rather than an HOF-name list) covers
/// `closure` / `zip_with` bodies too.
fn migrate_scope(network: &mut Value) -> Result<(), MigrationError> {
    migrate_free_rot_nodes(network)?;

    if let Some(nodes) = network.get_mut("nodes").and_then(|v| v.as_array_mut()) {
        for node in nodes {
            if let Some(zone) = node.get_mut("zone").filter(|z| z.is_object()) {
                migrate_scope(zone)?;
            }
        }
    }

    Ok(())
}

/// One `free_rot` node whose `angle` pin is wired and therefore needs a
/// synthesised `degrees(x)` expr node spliced onto the wire. Collected in a
/// read-only pre-pass, then sorted before id allocation so the produced JSON
/// is byte-identical across runs (mirrors v3→v4's `WireRewrite`).
struct WiredRewrite {
    /// The `free_rot` node's id (unique sort key — one angle pin per node).
    free_rot_id: u64,
    /// The wires that were on the `angle` pin, as `incoming_wires`-shaped JSON
    /// objects, moved **verbatim** onto the synthesised expr node's `x` pin
    /// (preserves `source_scope_depth` / `ZoneInput` capture semantics — the
    /// expr node lands in the same scope as the `free_rot`).
    angle_wires: Vec<Value>,
    /// The `free_rot`'s position, snapped to integers — anchor for the
    /// synthesised expr node placement.
    free_rot_position: [f64; 2],
}

/// Migrate every `free_rot` node directly in this scope (not recursing into
/// bodies — [`migrate_scope`] handles that).
fn migrate_free_rot_nodes(network: &mut Value) -> Result<(), MigrationError> {
    let Some(next_id_val) = network.get("next_node_id").and_then(|v| v.as_u64()) else {
        return Ok(());
    };

    // Read-only pre-pass: collect the wired `free_rot` nodes. Every action here
    // and in the mutation passes is gated on the radian-era `data.angle` key
    // still being present, which guarantees idempotency (after the rename the
    // key is gone, so a re-run finds nothing and synthesises no second node).
    let mut rewrites: Vec<WiredRewrite> = Vec::new();
    if let Some(nodes) = network.get("nodes").and_then(|v| v.as_array()) {
        for node in nodes {
            if !is_unmigrated_free_rot(node) {
                continue;
            }
            let Some(id) = node.get("id").and_then(|v| v.as_u64()) else {
                continue;
            };
            let angle_wires = read_incoming_wires(node, FREE_ROT_ANGLE_PIN);
            if angle_wires.is_empty() {
                // Unwired angle pin: only the stored-value rename applies (done
                // in the mutation pass below), no expr node.
                continue;
            }
            rewrites.push(WiredRewrite {
                free_rot_id: id,
                angle_wires,
                free_rot_position: read_position(node),
            });
        }
    }

    // Deterministic order — one angle pin per `free_rot`, so `free_rot_id` is
    // a unique key.
    rewrites.sort_by_key(|r| r.free_rot_id);

    // Allocate synthesised expr ids in sorted order.
    let mut next_id = next_id_val;
    let allocations: Vec<u64> = rewrites
        .iter()
        .map(|_| {
            let id = next_id;
            next_id += 1;
            id
        })
        .collect();

    // Mutation pass 1: rename `data.angle` → `data.angle_degrees` (with the
    // radian→degree conversion) on EVERY unmigrated `free_rot` in this scope,
    // wired or not.
    if let Some(nodes) = network.get_mut("nodes").and_then(|v| v.as_array_mut()) {
        for node in nodes.iter_mut() {
            if node.get("node_type_name").and_then(|v| v.as_str()) == Some("free_rot") {
                rename_angle_to_degrees(node);
            }
        }
    }

    if rewrites.is_empty() {
        return Ok(());
    }

    // Bump `next_node_id` once, past every synthesised id.
    if let Some(n) = network.get_mut("next_node_id") {
        *n = Value::from(next_id);
    }

    let nodes = network
        .get_mut("nodes")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| MigrationError::MalformedStructure("nodes array missing".to_string()))?;

    // Mutation pass 2a: repoint each wired `free_rot`'s angle pin at its
    // synthesised expr node.
    for (rewrite, &expr_id) in rewrites.iter().zip(allocations.iter()) {
        if let Some(node) = nodes
            .iter_mut()
            .find(|n| n.get("id").and_then(|v| v.as_u64()) == Some(rewrite.free_rot_id))
        {
            set_argument_to_single_wire(node, FREE_ROT_ANGLE_PIN, expr_id);
        }
    }

    // Mutation pass 2b: append the synthesised `degrees(x)` expr nodes in
    // id-allocation order.
    for (rewrite, &expr_id) in rewrites.iter().zip(allocations.iter()) {
        nodes.push(build_degrees_expr_node(
            expr_id,
            rewrite.free_rot_position,
            &rewrite.angle_wires,
        ));
    }

    Ok(())
}

/// True iff `node` is a `free_rot` that still carries the radian-era
/// `data.angle` key (i.e. has not already been migrated). This single gate
/// drives both correctness (only radian-era nodes are touched) and idempotency
/// (a re-run sees `data.angle_degrees` and skips).
fn is_unmigrated_free_rot(node: &Value) -> bool {
    node.get("node_type_name").and_then(|v| v.as_str()) == Some("free_rot")
        && node.get("data").and_then(|d| d.get("angle")).is_some()
}

/// Reads a node's `arguments[pin_index]` and returns its incoming wires as
/// `incoming_wires`-shaped JSON objects, transparently handling **both** wire
/// storage shapes:
/// - the current `incoming_wires` list (authoritative when present, even if
///   empty — matches the custom `Argument` deserializer's precedence);
/// - the legacy `argument_output_pins` map (emitted by the v2→v3 / v3→v4
///   passes, so chained old files arrive with this shape around exactly the
///   `free_rot` nodes this pass rewrites).
fn read_incoming_wires(node: &Value, pin_index: usize) -> Vec<Value> {
    let Some(arg) = node
        .get("arguments")
        .and_then(|v| v.as_array())
        .and_then(|a| a.get(pin_index))
    else {
        return Vec::new();
    };

    // `incoming_wires` present → authoritative, even if empty (mirrors the
    // `Argument` deserializer, which ignores `argument_output_pins` whenever
    // `incoming_wires` is present).
    if let Some(wires) = arg.get("incoming_wires").and_then(|v| v.as_array()) {
        return wires.clone();
    }

    // Legacy map shape: `{ "<source_node_id>": <output_pin_index> }`. Convert
    // to the modern shape, sorted by source id for deterministic output.
    if let Some(pins) = arg.get("argument_output_pins").and_then(|v| v.as_object()) {
        let mut entries: Vec<(u64, i64)> = pins
            .iter()
            .filter_map(|(k, v)| Some((k.parse::<u64>().ok()?, v.as_i64()?)))
            .collect();
        entries.sort();
        return entries
            .into_iter()
            .map(|(src_id, pin)| {
                serde_json::json!({
                    "source_node_id": src_id,
                    "source_pin": { "NodeOutput": { "pin_index": pin } },
                    "source_scope_depth": 0
                })
            })
            .collect();
    }

    Vec::new()
}

/// Renames `data.angle` → `data.angle_degrees` in place, converting the value
/// from radians to degrees. No-op if `data.angle` is absent (already migrated
/// or missing) — this keeps the pass idempotent.
fn rename_angle_to_degrees(node: &mut Value) {
    let Some(data) = node.get_mut("data").and_then(|v| v.as_object_mut()) else {
        return;
    };
    let Some(angle) = data.remove("angle") else {
        return;
    };
    let radians = angle.as_f64().unwrap_or(0.0);
    data.insert(
        "angle_degrees".to_string(),
        Value::from(radians.to_degrees()),
    );
}

/// Replaces `node.arguments[pin_index]` with a single plain wire from
/// `(src_id, pin 0)` in the modern `incoming_wires` shape.
fn set_argument_to_single_wire(node: &mut Value, pin_index: usize, src_id: u64) {
    let Some(args) = node.get_mut("arguments").and_then(|v| v.as_array_mut()) else {
        return;
    };
    let Some(arg) = args.get_mut(pin_index) else {
        return;
    };
    *arg = serde_json::json!({
        "incoming_wires": [
            {
                "source_node_id": src_id,
                "source_pin": { "NodeOutput": { "pin_index": 0 } },
                "source_scope_depth": 0
            }
        ]
    });
}

/// Reads a node's `position` and snaps both coordinates to integers. Snapping
/// avoids f64 round-trip ULP drift (same rationale as v3→v4's `read_position`).
fn read_position(node: &Value) -> [f64; 2] {
    node.get("position")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let x = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
            [x.round(), y.round()]
        })
        .unwrap_or([0.0, 0.0])
}

/// Synthesises a `degrees(x)` expr node. Placed `EXPR_NODE_OFFSET_X` to the
/// left of the `free_rot` it feeds, same y. Its `x` pin carries `angle_wires`
/// (moved verbatim from the `free_rot`'s angle pin). The `data` shape mirrors
/// exactly what a live-authored single-`Float`-parameter expr node saves.
fn build_degrees_expr_node(id: u64, free_rot_position: [f64; 2], angle_wires: &[Value]) -> Value {
    let position = [
        free_rot_position[0] - EXPR_NODE_OFFSET_X,
        free_rot_position[1],
    ];
    serde_json::json!({
        "id": id,
        "node_type_name": "expr",
        "custom_name": "to_degrees",
        "position": [position[0], position[1]],
        "arguments": [
            { "incoming_wires": angle_wires }
        ],
        "data_type": "expr",
        "data": {
            "parameters": [
                { "id": null, "name": "x", "data_type": "Float", "data_type_str": null }
            ],
            "expression": "degrees(x)"
        }
    })
}
