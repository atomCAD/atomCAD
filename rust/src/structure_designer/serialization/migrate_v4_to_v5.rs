//! Migration of `.cnnd` save files from serialization version 4 (the last
//! version on `main`) to version 5 (the shape the `zones` branch saves).
//!
//! Design: see `doc/design_zones_migration.md`.
//!
//! Phase 3 scope (this drop, on top of Phase 2): the closure-wrapping
//! transformation is now executed. Main's legacy partial-application
//! convention is **parameters first, captures last**: the source's first `K`
//! (= the HOF's arity) input pins are per-call parameters (must be unwired)
//! and the trailing `N_total - K` pins are captures (must be wired). For
//! every legacy `HOF.f`-wire whose source matches this shape, a new `closure`
//! node is synthesised in the parent network with a body containing a clone
//! of the source node; the clone's first `K` arguments read the closure's
//! `ZoneInput` pins (parameters), and the trailing arguments forward the
//! original captures at `source_scope_depth = 1` to the parent network. The
//! HOF's `f` argument is rewired to point at the new closure's pin 0. After
//! every rewrite in a network finishes, any source node whose only consumers
//! were the rewritten `-1` wires is deleted from the parent network (orphan
//! cleanup — see §"Source-node cleanup").
//!
//! Wires that fall into `NoOp` (source has exactly `K` free inputs) are
//! preserved unchanged — the function-pin synthesizer on the zones branch
//! handles them directly. Wires in `Skip` (unwired pin after a wired pin,
//! `N_total < K`, missing source) are logged and left untouched.
//!
//! The migration's *only* job is to rewrite legacy HOF `f`-wires. Everything
//! else about v5 (zones, body wires, the `incoming_wires` storage shape,
//! default body sizes, the `collapse_mode` field) is handled transparently by
//! `#[serde(default)]` and the custom `Argument` deserializer — no migration
//! code required. See the design doc's "What's already handled by serde"
//! section for the full inventory.

use serde_json::Value;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use super::migrate_v2_to_v3::MigrationError;

// Test-only instrumentation: counts invocations of `migrate_v4_to_v5` so the
// test suite can verify the version dispatch actually skips the pre-pass for
// v5 files. Production code never reads this. Mirrors the v2→v3 / v3→v4
// counters; see `migrate_v2_to_v3.rs` for the thread-locality rationale.
thread_local! {
    static MIGRATION_CALL_COUNT: Cell<u64> = const { Cell::new(0) };
}

/// Returns the number of times [`migrate_v4_to_v5`] has been called on the
/// current thread.
pub fn migration_call_count() -> u64 {
    MIGRATION_CALL_COUNT.with(|c| c.get())
}

/// Resets the current thread's [`migration_call_count`] counter.
pub fn reset_migration_call_count() {
    MIGRATION_CALL_COUNT.with(|c| c.set(0));
}

// ---------------------------------------------------------------------------
// v5 HOF-pin table
// ---------------------------------------------------------------------------

/// Frozen v5 HOF table: `(node_type_name, f_pin_index, expected_arity)`.
/// Hardcoded — not read from the live `NodeTypeRegistry` — so future registry
/// changes (renames, arity changes on HOFs) don't retroactively alter how a
/// v4 file gets up-converted. Mirrors `migrate_v3_to_v4::ITERATOR_PINS_V4`'s
/// rationale.
pub(crate) const HOF_F_PINS_V5: &[(&str, /*f_index*/ usize, /*arity*/ usize)] = &[
    ("map", 1, 1),
    ("filter", 1, 1),
    ("fold", 2, 2),
    ("foreach", 1, 1),
];

/// Returns `(f_index, arity)` if `node_type_name` is a HOF; otherwise `None`.
fn hof_lookup(node_type_name: &str) -> Option<(usize, usize)> {
    HOF_F_PINS_V5
        .iter()
        .find(|(name, _, _)| *name == node_type_name)
        .map(|(_, f_idx, arity)| (*f_idx, *arity))
}

// ---------------------------------------------------------------------------
// Action classification
// ---------------------------------------------------------------------------

/// One HOF.f wire detected during the read-only pre-pass. Identifies the
/// destination (HOF node id + `f` argument index) and the source node id.
/// The `kind` field records which row of the design-doc bucketing table
/// the wire fell into.
#[derive(Debug)]
pub(crate) struct DetectedAction {
    pub hof_id: u64,
    /// Index of the HOF's `f` parameter on its `arguments` array.
    pub f_index: usize,
    pub src_id: u64,
    /// HOF type name (`"map"` / `"filter"` / `"fold"` / `"foreach"`). Kept
    /// here so the execution pass doesn't have to re-resolve it from the node
    /// JSON in the network.
    pub hof_type: String,
    pub kind: ActionKind,
}

#[derive(Debug)]
pub(crate) enum ActionKind {
    /// Source has exactly `K` inputs and every input pin is unwired — the
    /// new function-pin synthesizer (`build_node_function_closure`) handles
    /// this directly on the zones branch, so the wire is kept as-is.
    NoOp,
    /// Source has `N_total > K` inputs with a clean prefix-unwired (parameter)
    /// followed by suffix-wired (capture) partition. Phase 3 wraps the source
    /// in a `closure` node. (Main's partial-application convention is
    /// **parameters first, captures last** — see
    /// `data_type.rs::can_be_converted_to`'s Function arm on the legacy main
    /// branch: "F contains all parameters of G as its first parameters; F may
    /// have additional parameters after G's parameters.")
    ///
    /// `capture_count` = `N_total - K` is carried for diagnostics and to
    /// distinguish the bucket from `NoOp` (`capture_count == 0`); the
    /// execution pass recomputes positions from the HOF's arity directly
    /// (parameters at index `[0..arity)`, captures at `[arity..N_total)`).
    ClosureWrap {
        #[allow(dead_code)] // Diagnostic only.
        capture_count: usize,
    },
    /// Anything else — unwired pin after a wired pin, `N_total < K`, missing
    /// source node, etc. Phase 3 logs and leaves the wire untouched.
    Skip { reason: String },
}

// ---------------------------------------------------------------------------
// Wire-shape helpers (dual v4 / v5 reads)
// ---------------------------------------------------------------------------

/// Reads a single source `(node_id, pin_index)` from an argument's wire
/// storage, handling both the v4 shape (`argument_output_pins`: HashMap) and
/// the v5 shape (`incoming_wires`: array). Returns `None` for an unwired
/// argument or a wire shape this migration doesn't recognize (e.g.
/// `ZoneInput` source, which can't exist on a v4 file but is handled
/// defensively).
///
/// For multi-wire arguments (only possible if a v5 file is hand-edited
/// before migration), takes the first wire deterministically — HOF.f pins
/// hold at most one wire by construction.
fn read_argument_source(arg: &Value) -> Option<(u64, i32)> {
    if let Some(wires) = arg.get("incoming_wires").and_then(|v| v.as_array()) {
        let first = wires.first()?;
        let src_id = first.get("source_node_id").and_then(|v| v.as_u64())?;
        let source_pin = first.get("source_pin")?;
        let node_output = source_pin.get("NodeOutput")?;
        let pin_index = node_output.get("pin_index").and_then(|v| v.as_i64())?;
        return Some((src_id, pin_index as i32));
    }
    if let Some(pins) = arg.get("argument_output_pins").and_then(|v| v.as_object()) {
        let (src_id_str, src_pin_val) = pins.iter().next()?;
        let src_id = src_id_str.parse::<u64>().ok()?;
        let src_pin = src_pin_val.as_i64()?;
        return Some((src_id, src_pin as i32));
    }
    None
}

/// Returns `true` iff the argument carries at least one inbound wire,
/// under either the v4 (`argument_output_pins`) or v5 (`incoming_wires`)
/// storage shape.
fn argument_is_wired(arg: &Value) -> bool {
    if let Some(wires) = arg.get("incoming_wires").and_then(|v| v.as_array()) {
        return !wires.is_empty();
    }
    if let Some(pins) = arg.get("argument_output_pins").and_then(|v| v.as_object()) {
        return !pins.is_empty();
    }
    false
}

/// Returns `true` iff the argument references `source_id` as a source, under
/// either the v4 or v5 wire-storage shape. Used by `cleanup_orphan_sources`
/// to decide whether a now-rewritten source still has any live consumer.
fn argument_references_source(arg: &Value, source_id: u64) -> bool {
    if let Some(wires) = arg.get("incoming_wires").and_then(|v| v.as_array()) {
        for w in wires {
            if w.get("source_node_id").and_then(|v| v.as_u64()) == Some(source_id) {
                return true;
            }
        }
    }
    if let Some(pins) = arg.get("argument_output_pins").and_then(|v| v.as_object()) {
        if pins.contains_key(&source_id.to_string()) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Read-only pass: scan one network for HOF.f wires that need attention and
/// classify each per the design doc's bucketing table.
pub(crate) fn detect_hof_f_actions(network: &Value) -> Vec<DetectedAction> {
    let Some(nodes) = network.get("nodes").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    // Index nodes by id once for source lookup.
    let nodes_by_id: HashMap<u64, &Value> = nodes
        .iter()
        .filter_map(|n| {
            let id = n.get("id").and_then(|v| v.as_u64())?;
            Some((id, n))
        })
        .collect();

    let mut actions = Vec::new();
    for hof_node in nodes {
        let Some(hof_id) = hof_node.get("id").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(hof_type_name) = hof_node.get("node_type_name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some((f_index, arity)) = hof_lookup(hof_type_name) else {
            continue;
        };

        let Some(args) = hof_node.get("arguments").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(f_arg) = args.get(f_index) else {
            continue;
        };

        // Only act on `-1`-source wires — that is, the legacy function-pin
        // convention. Anything else (a 0-pin source, an already-closure
        // value source, etc.) is left alone.
        let Some((src_id, src_pin_idx)) = read_argument_source(f_arg) else {
            continue;
        };
        if src_pin_idx != -1 {
            continue;
        }

        // Look up the source node by id in the same network. (HOF.f wires
        // never cross scopes on v4 — there are no scopes on v4.)
        let Some(src_node) = nodes_by_id.get(&src_id) else {
            actions.push(DetectedAction {
                hof_id,
                f_index,
                src_id,
                hof_type: hof_type_name.to_string(),
                kind: ActionKind::Skip {
                    reason: format!("source node id {} missing from network", src_id),
                },
            });
            continue;
        };

        // Partition source arguments into prefix-unwired parameters vs.
        // suffix-wired captures (main's parameters-first, captures-last
        // convention). Missing-or-empty `arguments` is treated as "no inputs"
        // — see Phase 2 Gotcha in the design doc.
        let src_args: Vec<&Value> = src_node
            .get("arguments")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        let n_total = src_args.len();

        let kind = classify_source_arity(n_total, arity, &src_args);
        actions.push(DetectedAction {
            hof_id,
            f_index,
            src_id,
            hof_type: hof_type_name.to_string(),
            kind,
        });
    }

    actions
}

/// Implements the bucketing table on a single HOF.f source. Main's
/// partial-application convention is **parameters first, captures last**:
/// the source's first `K` (= the HOF's arity) inputs are parameters (must be
/// unwired), and the trailing `N_total - K` inputs are captures (must be
/// wired).
///
/// | Source's input wiring (N_total / K) | Action |
/// |---|---|
/// | `N_total == K` and every input pin is unwired | `NoOp` |
/// | `N_total > K`, first `K` unwired, trailing `N_total - K` wired | `ClosureWrap { capture_count }` |
/// | Anything else (wired-then-unwired, `N_total < K`, etc.) | `Skip { reason }` |
fn classify_source_arity(n_total: usize, arity: usize, src_args: &[&Value]) -> ActionKind {
    if n_total < arity {
        return ActionKind::Skip {
            reason: format!(
                "source has {} input pin(s) but HOF expects arity {}",
                n_total, arity
            ),
        };
    }

    let capture_count = n_total - arity;

    // The first `arity` pins must all be unwired (per-call parameters); the
    // trailing `capture_count` pins must all be wired (captures, pre-evaluated
    // once at HOF eval time). Anything else is malformed.
    for (i, arg) in src_args.iter().enumerate() {
        let wired = argument_is_wired(arg);
        let should_be_wired = i >= arity;
        if wired != should_be_wired {
            return ActionKind::Skip {
                reason: format!(
                    "source input pin {} is {} but the prefix-unwired/suffix-wired layout expected it {}",
                    i,
                    if wired { "wired" } else { "unwired" },
                    if should_be_wired { "wired" } else { "unwired" },
                ),
            };
        }
    }

    if capture_count == 0 {
        ActionKind::NoOp
    } else {
        ActionKind::ClosureWrap { capture_count }
    }
}

// ---------------------------------------------------------------------------
// ClosureData synthesis
// ---------------------------------------------------------------------------

/// Build the `{ kind, type_args, param_names, custom_label }` JSON blob for
/// the synthesised `closure` node from the HOF type name and its `data` blob.
///
/// `DataType` values are passed through as-is (the design doc spells out
/// "copied as `serde_json::Value` into the closure's `data.type_args` array
/// unchanged"). Missing keys fall back to `"None"` — matches the defensive
/// parsing convention used by `migrate_v3_to_v4::validate_data_type_or_none`.
fn build_closure_data(hof_type: &str, hof_data: &Value) -> Value {
    let pick = |key: &str| -> Value {
        hof_data
            .get(key)
            .cloned()
            .unwrap_or_else(|| Value::String("None".to_string()))
    };

    let (kind, type_args): (&str, Vec<Value>) = match hof_type {
        "map" => ("Map", vec![pick("input_type"), pick("output_type")]),
        "filter" => ("Filter", vec![pick("element_type")]),
        "fold" => (
            "Fold",
            vec![pick("accumulator_type"), pick("element_type")],
        ),
        "foreach" => ("Foreach", vec![pick("input_type")]),
        // Unreachable in practice — the caller has already established
        // `hof_type` is in `HOF_F_PINS_V5`. Belt-and-braces fallback.
        _ => ("Map", vec![Value::String("None".to_string())]),
    };

    // `custom_label` has `#[serde(default)]` (not `skip_serializing_if`), so
    // emitting `null` round-trips cleanly. The design doc's example writes
    // `null` explicitly — keep that to make the synthesised output easy to
    // recognise by eye.
    serde_json::json!({
        "kind": kind,
        "type_args": type_args,
        "param_names": [],
        "custom_label": null,
    })
}

// ---------------------------------------------------------------------------
// Body NodeNetwork synthesis
// ---------------------------------------------------------------------------

/// Build the body clone's `arguments` array: one entry per input pin on the
/// original source node. The first `arity` are *parameter wires* (reading
/// `ZoneInput` on the new closure node); the trailing `capture_count` are
/// *capture wires* (reaching `source_scope_depth: 1` to the parent network).
/// This mirrors main's partial-application convention — parameters first,
/// captures last.
fn build_body_arguments(source_args: &[Value], arity: usize, closure_id: u64) -> Vec<Value> {
    source_args
        .iter()
        .enumerate()
        .map(|(i, src_arg)| {
            if i < arity {
                // Parameter: read closure's ZoneInput at index `i`.
                serde_json::json!({
                    "incoming_wires": [
                        {
                            "source_node_id": closure_id,
                            "source_pin": { "ZoneInput": { "pin_index": i } },
                            "source_scope_depth": 1,
                        }
                    ]
                })
            } else {
                // Capture: forward the original wire's (source_node_id, pin_index)
                // up one scope level. Reads either the v4 or v5 wire shape on the
                // source side; emits v5 shape on the new body wire.
                // Should not return None: classification already verified
                // every capture pin is wired. Belt-and-braces fallback.
                let (orig_src_id, orig_pin) = read_argument_source(src_arg).unwrap_or((0, 0));
                serde_json::json!({
                    "incoming_wires": [
                        {
                            "source_node_id": orig_src_id,
                            "source_pin": { "NodeOutput": { "pin_index": orig_pin } },
                            "source_scope_depth": 1,
                        }
                    ]
                })
            }
        })
        .collect()
}

/// Build the body `NodeNetwork` JSON. Contains exactly one node: a clone of
/// the source with body-local id `1`. The body's `node_type` placeholder
/// matches what `NodeNetwork::new_empty()` produces for runtime-created
/// bodies (empty name, `OtherBuiltin` category, single `result` output pin
/// of type `None`) — see `doc/design_zones_migration.md` §"Step 4".
fn build_body_network(source_node: &Value, arity: usize, closure_id: u64) -> Value {
    let src_args: Vec<Value> = source_node
        .get("arguments")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let body_args = build_body_arguments(&src_args, arity, closure_id);

    let src_type_name = source_node
        .get("node_type_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let src_data_type = source_node
        .get("data_type")
        .and_then(|v| v.as_str())
        .unwrap_or(&src_type_name)
        .to_string();
    let src_data = source_node
        .get("data")
        .cloned()
        .unwrap_or(Value::Null);

    // Optional fields on the cloned body node: preserve `custom_name` if the
    // source had one, omit otherwise (matches what live code emits — see
    // §"Step 4" notes on `skip_serializing_if`).
    let mut body_node = serde_json::Map::new();
    body_node.insert("id".to_string(), Value::from(1u64));
    body_node.insert(
        "node_type_name".to_string(),
        Value::String(src_type_name.clone()),
    );
    if let Some(custom_name) = source_node.get("custom_name").cloned() {
        if !custom_name.is_null() {
            body_node.insert("custom_name".to_string(), custom_name);
        }
    }
    // Position the clone at a fixed offset inside the body. The exact value
    // doesn't matter for correctness (the renderer treats body coordinates
    // independently); 40,40 keeps it inside the default 320×180 body.
    body_node.insert(
        "position".to_string(),
        serde_json::json!([40.0, 40.0]),
    );
    body_node.insert("arguments".to_string(), Value::Array(body_args));
    body_node.insert("data_type".to_string(), Value::String(src_data_type));
    body_node.insert("data".to_string(), src_data);

    serde_json::json!({
        "next_node_id": 2u64,
        "node_type": {
            "name": "",
            "description": "",
            "summary": null,
            "category": "OtherBuiltin",
            "parameters": [],
            "output_pins": [
                { "name": "result", "data_type": "None" }
            ]
        },
        "nodes": [ Value::Object(body_node) ],
        "return_node_id": null,
        "displayed_node_ids": []
    })
}

// ---------------------------------------------------------------------------
// Closure node synthesis
// ---------------------------------------------------------------------------

/// Build the full `closure` node JSON for one rewrite — including its body,
/// `zone_output_arguments`, position, and `ClosureData`.
fn build_closure_node(
    new_id: u64,
    hof_node: &Value,
    source_node: &Value,
    hof_type: &str,
    arity: usize,
) -> Value {
    let hof_data = hof_node
        .get("data")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let closure_data = build_closure_data(hof_type, &hof_data);

    // Position the closure 160 units to the left of the HOF — keeps it
    // visually upstream. Snap to integers to match the v3→v4 convention and
    // avoid f64 round-trip drift.
    let hof_position = hof_node.get("position").and_then(|v| v.as_array());
    let (hx, hy) = match hof_position {
        Some(arr) => (
            arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
            arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
        ),
        None => (0.0, 0.0),
    };
    let position = [(hx - 160.0).round(), hy.round()];

    let body = build_body_network(source_node, arity, new_id);

    // The closure has exactly one zone-output pin in every preset kind; it
    // reads body-local node 1's pin 0.
    let zone_output_arguments = serde_json::json!([
        {
            "incoming_wires": [
                {
                    "source_node_id": 1u64,
                    "source_pin": { "NodeOutput": { "pin_index": 0 } },
                    "source_scope_depth": 0,
                }
            ]
        }
    ]);

    serde_json::json!({
        "id": new_id,
        "node_type_name": "closure",
        "position": [position[0], position[1]],
        "arguments": [],
        "data_type": "closure",
        "data": closure_data,
        "zone": body,
        "zone_output_arguments": zone_output_arguments,
    })
}

// ---------------------------------------------------------------------------
// Per-network execution
// ---------------------------------------------------------------------------

/// Rewires the HOF's `f` argument to point at the new closure node's pin 0,
/// writing in the v5 `incoming_wires` shape. Other arguments on the HOF are
/// left in whichever shape they had — serde converts at load.
fn rewire_hof_f_arg(hof_node: &mut Value, f_index: usize, closure_id: u64) {
    let Some(args) = hof_node.get_mut("arguments").and_then(|v| v.as_array_mut()) else {
        return;
    };
    let Some(arg) = args.get_mut(f_index) else {
        return;
    };
    *arg = serde_json::json!({
        "incoming_wires": [
            {
                "source_node_id": closure_id,
                "source_pin": { "NodeOutput": { "pin_index": 0 } },
                "source_scope_depth": 0,
            }
        ]
    });
}

/// Reads `network.next_node_id`, falling back to `max(node ids) + 1` if the
/// field is missing or non-integer.
fn next_node_id_for_network(network: &Value) -> u64 {
    if let Some(v) = network.get("next_node_id").and_then(|v| v.as_u64()) {
        return v;
    }
    let max_id = network
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| n.get("id").and_then(|v| v.as_u64()))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    max_id + 1
}

/// Scan every remaining `Argument` in the network's nodes (both `arguments`
/// and `zone_output_arguments`) and count references to each id in
/// `rewritten_sources`. Source nodes whose count drops to zero are deleted
/// from `network.nodes` and pruned from `displayed_node_ids` /
/// `displayed_output_pins`.
fn cleanup_orphan_sources(network: &mut Value, rewritten_sources: &HashSet<u64>) {
    if rewritten_sources.is_empty() {
        return;
    }

    // Phase 1 — find which rewritten source ids are now unreferenced.
    let Some(nodes) = network.get("nodes").and_then(|v| v.as_array()) else {
        return;
    };
    let mut alive: HashSet<u64> = HashSet::new();
    for node in nodes {
        for arg in node
            .get("arguments")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            for &src in rewritten_sources {
                if argument_references_source(arg, src) {
                    alive.insert(src);
                }
            }
        }
        for arg in node
            .get("zone_output_arguments")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            for &src in rewritten_sources {
                if argument_references_source(arg, src) {
                    alive.insert(src);
                }
            }
        }
    }
    let to_delete: HashSet<u64> = rewritten_sources
        .iter()
        .copied()
        .filter(|s| !alive.contains(s))
        .collect();
    if to_delete.is_empty() {
        return;
    }

    // Phase 2 — drop the orphans from `nodes` and from any display lists.
    if let Some(nodes_mut) = network.get_mut("nodes").and_then(|v| v.as_array_mut()) {
        nodes_mut.retain(|n| {
            n.get("id")
                .and_then(|v| v.as_u64())
                .map(|id| !to_delete.contains(&id))
                .unwrap_or(true)
        });
    }
    if let Some(disp) = network
        .get_mut("displayed_node_ids")
        .and_then(|v| v.as_array_mut())
    {
        disp.retain(|entry| {
            let id = entry
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_u64());
            match id {
                Some(id) => !to_delete.contains(&id),
                None => true,
            }
        });
    }
    if let Some(disp) = network
        .get_mut("displayed_output_pins")
        .and_then(|v| v.as_array_mut())
    {
        disp.retain(|entry| {
            let id = entry
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_u64());
            match id {
                Some(id) => !to_delete.contains(&id),
                None => true,
            }
        });
    }
    // If the orphan happened to be `return_node_id`, blank it out — the
    // network's output type is propagated separately; leaving a dangling id
    // would surface as a load-time validation error.
    let return_is_orphan = network
        .get("return_node_id")
        .and_then(|v| v.as_u64())
        .map(|id| to_delete.contains(&id))
        .unwrap_or(false);
    if return_is_orphan {
        if let Some(obj) = network.as_object_mut() {
            obj.insert("return_node_id".to_string(), Value::Null);
        }
    }
}

/// Per-network mutation pass: classify HOF.f wires, allocate closure ids in
/// deterministic order, build and append closure nodes, rewire each HOF.f,
/// then run orphan-source cleanup.
fn migrate_network(
    network: &mut Value,
    network_name: &str,
) -> Result<(), MigrationError> {
    let actions = detect_hof_f_actions(network);
    if actions.is_empty() {
        return Ok(());
    }

    // Partition: `NoOp` is silent and untouched; `Skip` logs and is untouched;
    // `ClosureWrap` advances to the id-allocation pass.
    //
    // The fifth tuple entry is the HOF's arity (= the number of leading
    // parameter pins on the source). The captured `ClosureWrap.capture_count`
    // is `src.arguments.len() - arity` — recoverable from the snapshot, so we
    // only carry the value the execution pass needs.
    let mut wraps: Vec<(u64, usize, u64, String, usize)> = Vec::new(); // (hof_id, f_index, src_id, hof_type, arity)
    for action in &actions {
        match &action.kind {
            ActionKind::NoOp => {}
            ActionKind::Skip { reason } => {
                eprintln!(
                    "v4→v5: skipping HOF f-wire on network={}, hof_id={}, reason={}, src_id={}",
                    network_name, action.hof_id, reason, action.src_id,
                );
            }
            ActionKind::ClosureWrap { capture_count: _ } => {
                let arity = hof_lookup(&action.hof_type)
                    .map(|(_, a)| a)
                    .unwrap_or(0);
                wraps.push((
                    action.hof_id,
                    action.f_index,
                    action.src_id,
                    action.hof_type.clone(),
                    arity,
                ));
            }
        }
    }
    if wraps.is_empty() {
        return Ok(());
    }

    // Deterministic id allocation: sort by (hof_id, f_index, src_id).
    wraps.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));

    let mut next_id = next_node_id_for_network(network);
    let allocations: Vec<u64> = wraps
        .iter()
        .map(|_| {
            let id = next_id;
            next_id += 1;
            id
        })
        .collect();

    // Snapshot the original source nodes' JSON before any mutation: the
    // body-clone step needs to read them, and the orphan-cleanup step may
    // delete them. Cloning keeps the borrow off `network` for the mutation
    // pass below.
    let mut hof_snapshots: HashMap<u64, Value> = HashMap::new();
    let mut src_snapshots: HashMap<u64, Value> = HashMap::new();
    if let Some(nodes) = network.get("nodes").and_then(|v| v.as_array()) {
        for n in nodes {
            let Some(id) = n.get("id").and_then(|v| v.as_u64()) else {
                continue;
            };
            // Cheap: store under any ids we'll need later. (A node can be
            // both a HOF and a source — store unconditionally.)
            hof_snapshots.insert(id, n.clone());
            src_snapshots.insert(id, n.clone());
        }
    }

    // Build the new closure nodes (with their body clones).
    let mut new_closure_nodes: Vec<Value> = Vec::with_capacity(wraps.len());
    let mut rewritten_sources: HashSet<u64> = HashSet::new();
    for (rewrite, &new_id) in wraps.iter().zip(allocations.iter()) {
        let (hof_id, _f_index, src_id, hof_type, arity) = rewrite;
        let Some(hof_node) = hof_snapshots.get(hof_id) else {
            continue;
        };
        let Some(src_node) = src_snapshots.get(src_id) else {
            continue;
        };
        let closure = build_closure_node(new_id, hof_node, src_node, hof_type.as_str(), *arity);
        new_closure_nodes.push(closure);
        rewritten_sources.insert(*src_id);
    }

    // Mutation pass: rewire each HOF.f, then append the new closure nodes,
    // bump `next_node_id`, run orphan cleanup.
    {
        let Some(nodes_mut) = network
            .get_mut("nodes")
            .and_then(|v| v.as_array_mut())
        else {
            return Err(MigrationError::MalformedStructure(
                "nodes array missing".to_string(),
            ));
        };
        for (rewrite, &new_id) in wraps.iter().zip(allocations.iter()) {
            let (hof_id, f_index, _src_id, _hof_type, _cc) = rewrite;
            if let Some(hof_node) = nodes_mut
                .iter_mut()
                .find(|n| n.get("id").and_then(|v| v.as_u64()) == Some(*hof_id))
            {
                rewire_hof_f_arg(hof_node, *f_index, new_id);
            }
        }
        for closure in new_closure_nodes {
            nodes_mut.push(closure);
        }
    }

    if let Some(n) = network.get_mut("next_node_id") {
        *n = Value::from(next_id);
    } else if let Some(obj) = network.as_object_mut() {
        obj.insert("next_node_id".to_string(), Value::from(next_id));
    }

    cleanup_orphan_sources(network, &rewritten_sources);

    Ok(())
}

// ---------------------------------------------------------------------------
// Top-level entry
// ---------------------------------------------------------------------------

/// Top-level v4 → v5 pre-pass. Runs on the parsed JSON value before strict
/// deserialization.
pub fn migrate_v4_to_v5(root: &mut Value) -> Result<(), MigrationError> {
    MIGRATION_CALL_COUNT.with(|c| c.set(c.get() + 1));

    let Some(node_networks) = root.get_mut("node_networks").and_then(|v| v.as_array_mut()) else {
        return Ok(());
    };
    for entry in node_networks {
        let Some(entry_arr) = entry.as_array_mut() else {
            continue;
        };
        let network_name = entry_arr
            .first()
            .and_then(|v| v.as_str())
            .unwrap_or("<unnamed>")
            .to_string();
        let Some(network) = entry_arr.get_mut(1) else {
            continue;
        };

        migrate_network(network, &network_name)?;
    }

    Ok(())
}
