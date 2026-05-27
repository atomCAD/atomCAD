//! Migration of `.cnnd` save files from serialization version 4 (the last
//! version on `main`) to version 5 (the shape the `zones` branch saves).
//!
//! Design: see `doc/design_zones_migration.md`.
//!
//! Phase 2 scope (this drop, on top of Phase 1): the detection algorithm and
//! the skip-with-warning branch are wired up. For every HOF (`map`, `filter`,
//! `fold`, `foreach`) whose `f` argument carries a `-1`-source wire, the
//! source node's input wiring is partitioned into prefix-wired captures vs.
//! suffix-unwired parameters; an action is classified per the design doc's
//! bucketing table. **Phase 2 emits warnings for both `ClosureWrap` and
//! `Skip` actions and never mutates the JSON.** Phase 3 will replace the
//! `ClosureWrap` warning with the real closure-wrapping transformation.
//!
//! Files without any HOF.f wires (or with HOF.f wires that fall in the
//! `NoOp` bucket — source with exactly `K` free inputs) are passed through
//! untouched.
//!
//! The migration's *only* future job is to rewrite legacy HOF `f`-wires that
//! used the v4 "extras-as-prefix partial application" rule into the new
//! `closure`-node shape that the zones branch's structural-arity rule
//! expects. Everything else about v5 (zones, body wires, the `incoming_wires`
//! storage shape, default body sizes, the `collapse_mode` field) is handled
//! transparently by `#[serde(default)]` and the custom `Argument`
//! deserializer — no migration code required. See the design doc's "What's
//! already handled by serde" section for the full inventory.

use serde_json::Value;
use std::cell::Cell;
use std::collections::HashMap;

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
///
/// Phase 2 emits warnings for `ClosureWrap` and `Skip`, leaves `NoOp` alone,
/// and never mutates the JSON. Phase 3 will replace the `ClosureWrap`
/// warning branch with the real closure-wrapping transformation and add a
/// closure-id allocation pass between detection and execution.
#[derive(Debug)]
pub(crate) struct DetectedAction {
    pub hof_id: u64,
    /// Index of the HOF's `f` parameter on its `arguments` array. Recorded
    /// during detection so Phase 3's execution pass can rewire `arguments[f_index]`
    /// without redoing the HOF table lookup.
    #[allow(dead_code)] // Read by Phase 3.
    pub f_index: usize,
    pub src_id: u64,
    pub kind: ActionKind,
}

#[derive(Debug)]
pub(crate) enum ActionKind {
    /// Source has exactly `K` inputs and every input pin is unwired — the
    /// new function-pin synthesizer (`build_node_function_closure`) handles
    /// this directly on the zones branch, so the wire is kept as-is.
    NoOp,
    /// Source has `N_total > K` inputs with a clean prefix-wired (capture)
    /// followed by suffix-unwired (parameter) partition. Phase 3 will wrap
    /// the source in a `closure` node; Phase 2 logs and skips.
    ClosureWrap {
        #[allow(dead_code)] // Wired in by Phase 3.
        capture_count: usize,
    },
    /// Anything else — wired-after-unwired, `N_total < K`, missing source
    /// node, etc. Phase 2 and Phase 3 both log and leave the wire untouched.
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
        // `SourcePin` is an externally-tagged enum:
        // `{ "NodeOutput": { "pin_index": N } }` or
        // `{ "ZoneInput":  { "pin_index": N } }`. Migration only cares about
        // `NodeOutput`; a `ZoneInput` source on a v4-stamped file is
        // impossible but treated defensively as "wire we don't touch".
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

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Read-only pass: scan one network for HOF.f wires that need attention and
/// classify each per the design doc's bucketing table. Returns one
/// `DetectedAction` per detected wire.
///
/// Does not mutate `network` — the execution pass (Phase 3) will. Phase 2
/// just inspects the returned actions, emits warnings, and moves on.
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
                kind: ActionKind::Skip {
                    reason: format!("source node id {} missing from network", src_id),
                },
            });
            continue;
        };

        // Partition source arguments into prefix-wired captures vs.
        // suffix-unwired parameters. Missing-or-empty `arguments` is treated
        // as "no inputs" — see Phase 2 Gotcha in the design doc.
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
            kind,
        });
    }

    actions
}

/// Implements the design doc's bucketing table on a single HOF.f source:
///
/// | Source's input wiring (N_total / K) | Action |
/// |---|---|
/// | `N_total == K` and every input pin is unwired | `NoOp` |
/// | `N_total > K`, first `N_total - K` wired, trailing `K` unwired | `ClosureWrap { capture_count }` |
/// | Anything else (wired-after-unwired, `N_total < K`, etc.) | `Skip { reason }` |
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

    // The first `capture_count` pins must all be wired; the trailing
    // `arity` pins must all be unwired. Anything else is malformed.
    for (i, arg) in src_args.iter().enumerate() {
        let wired = argument_is_wired(arg);
        let should_be_wired = i < capture_count;
        if wired != should_be_wired {
            return ActionKind::Skip {
                reason: format!(
                    "source input pin {} is {} but the prefix-wired/suffix-unwired layout expected it {}",
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
// Top-level entry
// ---------------------------------------------------------------------------

/// Top-level v4 → v5 pre-pass. Runs on the parsed JSON value before strict
/// deserialization.
///
/// Phase 2: per network, detect HOF.f actions and log warnings for
/// `ClosureWrap` (Phase 3 will synthesize) and `Skip`; `NoOp` actions are
/// silent. The JSON is not mutated. Phase 3 will add an id-allocation pass
/// between detection and execution and replace the `ClosureWrap` warning
/// with the real closure-wrapping transformation.
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

        let actions = detect_hof_f_actions(network);
        for action in &actions {
            match &action.kind {
                ActionKind::NoOp => {}
                ActionKind::ClosureWrap { capture_count } => {
                    eprintln!(
                        "v4→v5: skipping HOF f-wire on network={}, hof_id={}, reason=closure synthesis pending (Phase 3), capture_count={}, src_id={}",
                        network_name, action.hof_id, capture_count, action.src_id,
                    );
                }
                ActionKind::Skip { reason } => {
                    eprintln!(
                        "v4→v5: skipping HOF f-wire on network={}, hof_id={}, reason={}, src_id={}",
                        network_name, action.hof_id, reason, action.src_id,
                    );
                }
            }
        }
    }

    Ok(())
}
