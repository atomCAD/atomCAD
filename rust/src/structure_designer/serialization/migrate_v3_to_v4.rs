//! Migration of `.cnnd` save files from serialization version 3 (pre-iterators)
//! to version 4 (post-iterators).
//!
//! Design: see `doc/design_iterators.md` ("Backward compatibility" section).
//!
//! In v4 the output pin type of `range` / `map` / `filter` / `product` flips
//! from `[T]` to `Iter[T]`. v3 files may carry wires from those outputs into
//! pins typed `[T]` — under v4 typing those wires become invalid because
//! `Iter[T] → [T]` is not an implicit conversion. This pre-pass synthesises a
//! `collect` node on every such wire so v3 files load with their original
//! semantics intact.
//!
//! Phase 5 scope (this drop, on top of Phase 4): `("fold", 0)` joins the
//! `ITERATOR_PINS_V4` table so wires from an iterator producer (range / map /
//! filter / product) into `fold.xs` do not get a `collect` synthesised on
//! them — `fold.xs` is now natively `Iter[T]`. No new `produces_iter` arm:
//! `fold` is a terminal consumer, never a producer. The `product` arm still
//! returns `None` (filled in by Phase 6) and the transitive custom-network
//! detection is still stubbed (filled in by Phase 7 —
//! `compute_iterator_producer_set` returns an empty map for now).

use serde_json::Value;
use std::cell::Cell;
use std::collections::HashMap;

use super::migrate_v2_to_v3::MigrationError;

// Test-only instrumentation: counts invocations of `migrate_v3_to_v4` so the
// test suite can verify the version dispatch actually skips the pre-pass for
// v4 files. Production code never reads this. Mirrors the v2→v3 counter; see
// that module for the thread-locality rationale.
thread_local! {
    static MIGRATION_CALL_COUNT: Cell<u64> = const { Cell::new(0) };
}

/// Returns the number of times [`migrate_v3_to_v4`] has been called on the
/// current thread.
pub fn migration_call_count() -> u64 {
    MIGRATION_CALL_COUNT.with(|c| c.get())
}

/// Resets the current thread's [`migration_call_count`] counter.
pub fn reset_migration_call_count() {
    MIGRATION_CALL_COUNT.with(|c| c.set(0));
}

// ---------------------------------------------------------------------------
// v4 iterator-pin table
// ---------------------------------------------------------------------------

/// Built-in pins that natively accept `Iter[T]` in v4. Hardcoded — not read
/// from the live `NodeTypeRegistry` — so future registry changes (renames,
/// new iterator pins) don't retroactively alter how a v3 file gets up-
/// converted. Mirrors `migrate_v2_to_v3::PRIMITIVE_LATTICE_PIN`'s rationale.
///
/// Entries are added by phase. Phase 3 ships `("collect", 0)` (the synthesis
/// target). Phase 4 adds `("map", 0)` and `("filter", 0)` (so wires from one
/// iterator producer into another's `xs` pin do not get a `collect` inserted
/// on them). Phase 5 adds `("fold", 0)`. The variable-arity `product` pin is
/// special-cased separately.
const ITERATOR_PINS_V4: &[(&str, usize)] = &[
    ("map", 0),     // xs — Phase 4
    ("filter", 0),  // xs — Phase 4
    ("fold", 0),    // xs — Phase 5
    ("collect", 0), // iter
];

/// Returns true if `(node_type_name, param_index)` is a built-in v4 pin that
/// natively accepts `Iter[T]` and therefore does not need a `collect`
/// inserted on the wire feeding it.
fn is_iterator_pin_v4(node_type_name: &str, param_index: usize) -> bool {
    // `product` is special: variable axis count, every parameter is an
    // iterator pin. Phase 6 wires up the matching `produces_iter` arm; the
    // predicate is harmless to gate on the type name alone — predicate (A)
    // just won't fire for product sources until Phase 6.
    if node_type_name == "product" {
        return true;
    }
    ITERATOR_PINS_V4
        .iter()
        .any(|(name, idx)| *name == node_type_name && *idx == param_index)
}

// ---------------------------------------------------------------------------
// Top-level entry
// ---------------------------------------------------------------------------

/// Top-level v3 → v4 pre-pass. Runs on the parsed JSON value before strict
/// deserialization. Walks every network and inserts a `collect` node on each
/// wire that carries an `Iter[T]` source into a non-iterator destination.
pub fn migrate_v3_to_v4(root: &mut Value) -> Result<(), MigrationError> {
    MIGRATION_CALL_COUNT.with(|c| c.set(c.get() + 1));

    let iter_producers = compute_iterator_producer_set(root);

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
        insert_collect_for_iter_to_array_wires(network, &iter_producers)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Iterator-producer set (custom networks)
// ---------------------------------------------------------------------------

/// For each custom network whose v4 output type is `Iter[T]`, the element
/// type T (encoded as a `DataType` JSON value). Computed transitively: a
/// custom network whose return node is one of the four built-in producers,
/// or another custom network in this map, is an iterator source.
///
/// Phase 7 fills in this fixed-point pass. Phase 3 ships an empty map —
/// only built-in producers are recognised.
fn compute_iterator_producer_set(_root: &Value) -> HashMap<String, Value> {
    HashMap::new()
}

/// If `(network_name, return_node_type_name)` identifies an iterator
/// producer, returns the element type T as a `DataType` JSON value.
///
/// Phase 3 wires up the `range` arm; Phase 4 fills in `map`/`filter`;
/// Phase 6 fills in `product`; Phase 7 enables transitive custom-network
/// resolution by reading from `iter_producers`.
fn produces_iter(node: &Value, iter_producers: &HashMap<String, Value>) -> Option<Value> {
    let type_name = node.get("node_type_name").and_then(|v| v.as_str())?;
    match type_name {
        "range" => Some(Value::String("Int".to_string())),
        "map" => {
            // `MapData.output_type` is already a serialized `DataType` JSON
            // value (e.g. the string `"Int"`, or a tagged object like
            // `{"Array": "Int"}`); the synthesised `collect` consumes it as
            // its `element_type` directly. Defensive validation against
            // malformed values is added in Phase 7 per the design doc's
            // error policy.
            node.get("data").and_then(|d| d.get("output_type")).cloned()
        }
        "filter" => {
            // `FilterData.element_type` plays the same role for `filter`'s
            // output element type as `MapData.output_type` does for `map`.
            node.get("data")
                .and_then(|d| d.get("element_type"))
                .cloned()
        }
        "product" => {
            // Phase 6 will fill this in:
            //   Record(Named(node.data.target)) → element type T.
            None
        }
        _ => {
            // Phase 7: transitive custom-network resolution.
            iter_producers.get(type_name).cloned()
        }
    }
}

// ---------------------------------------------------------------------------
// Per-wire transformation
// ---------------------------------------------------------------------------

/// One wire in the network that needs a `collect` synthesised on it. Collected
/// in a read-only pre-pass so the mutation pass doesn't fight `serde_json`'s
/// borrow rules. Sorted into a deterministic order before the id-allocation
/// pass so the produced JSON is byte-identical across runs (mirrors v2→v3's
/// `splits` / `adaptations` patterns).
struct WireRewrite {
    /// Destination node id (the consumer of the iterator).
    dst_node_id: u64,
    /// Argument index on the destination's `arguments` array.
    dst_param_index: usize,
    /// Source node id (the iterator producer).
    src_node_id: u64,
    /// Output pin index on the source.
    src_pin_index: u64,
    /// Source node's position, snapped to integers (anchor for the synthesised
    /// `collect` node placement).
    src_position: [f64; 2],
    /// Element type T for the synthesised `collect`'s `element_type`. Already
    /// encoded as a `DataType` JSON value.
    element_type: Value,
}

/// Per-network mutation pass: for each wire matching predicates (A) and (B),
/// allocate a new node id, synthesise a `collect`, and rewire the
/// destination's argument to point at the new node.
fn insert_collect_for_iter_to_array_wires(
    network_json: &mut Value,
    iter_producers: &HashMap<String, Value>,
) -> Result<(), MigrationError> {
    let Some(next_id_val) = network_json.get("next_node_id").and_then(|v| v.as_u64()) else {
        return Ok(());
    };

    // Index nodes by id once for both predicates: predicate (A) needs the
    // source node's type, predicate (B) needs nothing extra here (it's a
    // table lookup on the destination's type) but the destination's type
    // is most easily looked up from the same map.
    let nodes_by_id: HashMap<u64, &Value> = network_json
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| {
                    let id = n.get("id").and_then(|v| v.as_u64())?;
                    Some((id, n))
                })
                .collect()
        })
        .unwrap_or_default();

    // Read-only pre-pass: collect every wire that needs rewriting.
    let mut rewrites: Vec<WireRewrite> = Vec::new();
    if let Some(nodes) = network_json.get("nodes").and_then(|v| v.as_array()) {
        for dst_node in nodes {
            let Some(dst_id) = dst_node.get("id").and_then(|v| v.as_u64()) else {
                continue;
            };
            let Some(dst_type_name) = dst_node.get("node_type_name").and_then(|v| v.as_str())
            else {
                continue;
            };
            let Some(args) = dst_node.get("arguments").and_then(|v| v.as_array()) else {
                continue;
            };
            for (dst_param_index, arg) in args.iter().enumerate() {
                let Some(pins) = arg.get("argument_output_pins").and_then(|v| v.as_object()) else {
                    continue;
                };
                for (src_id_str, src_pin_val) in pins {
                    let Ok(src_id) = src_id_str.parse::<u64>() else {
                        continue;
                    };
                    let Some(src_pin_index) = src_pin_val.as_u64() else {
                        continue;
                    };
                    let Some(src_node) = nodes_by_id.get(&src_id) else {
                        continue;
                    };

                    // Predicate (A): does the source produce `Iter[T]` in v4?
                    let Some(element_type) = produces_iter(src_node, iter_producers) else {
                        continue;
                    };

                    // Predicate (B): does the destination NOT natively accept
                    // `Iter[T]` on this pin?
                    if pin_accepts_iter_v4(dst_type_name, dst_param_index, dst_node) {
                        continue;
                    }

                    let src_position = read_position(src_node);

                    rewrites.push(WireRewrite {
                        dst_node_id: dst_id,
                        dst_param_index,
                        src_node_id: src_id,
                        src_pin_index,
                        src_position,
                        element_type,
                    });
                }
            }
        }
    }

    if rewrites.is_empty() {
        return Ok(());
    }

    // Deterministic order — see "ID, position, and ordering rules" in the
    // design doc. Sort key matches the doc:
    // (dst_node_id, dst_param_index, src_node_id, src_pin_index).
    rewrites.sort_by(|a, b| {
        (
            a.dst_node_id,
            a.dst_param_index,
            a.src_node_id,
            a.src_pin_index,
        )
            .cmp(&(
                b.dst_node_id,
                b.dst_param_index,
                b.src_node_id,
                b.src_pin_index,
            ))
    });

    // Allocate ids in sorted order; bump `next_node_id` once at the end.
    let mut next_id = next_id_val;
    let allocations: Vec<u64> = rewrites
        .iter()
        .map(|_| {
            let id = next_id;
            next_id += 1;
            id
        })
        .collect();

    if let Some(n) = network_json.get_mut("next_node_id") {
        *n = Value::from(next_id);
    }

    // Mutation pass: rewire each destination then append the synthesised
    // `collect` nodes. Done in two stages to keep mutable borrows tidy.
    let nodes = network_json
        .get_mut("nodes")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| MigrationError::MalformedStructure("nodes array missing".to_string()))?;

    for (rewrite, &new_id) in rewrites.iter().zip(allocations.iter()) {
        // Rewire dst_node.arguments[dst_param_index]: remove the
        // {src_node_id: src_pin_index} entry and replace with {new_id: 0}.
        let Some(dst_node) = nodes
            .iter_mut()
            .find(|n| n.get("id").and_then(|v| v.as_u64()) == Some(rewrite.dst_node_id))
        else {
            continue;
        };
        let Some(args) = dst_node.get_mut("arguments").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        let Some(arg) = args.get_mut(rewrite.dst_param_index) else {
            continue;
        };
        let Some(pins) = arg
            .get_mut("argument_output_pins")
            .and_then(|v| v.as_object_mut())
        else {
            continue;
        };
        pins.remove(&rewrite.src_node_id.to_string());
        pins.insert(new_id.to_string(), Value::from(0u64));
    }

    // Append synthesised `collect` nodes in id-allocation order.
    for (rewrite, &new_id) in rewrites.iter().zip(allocations.iter()) {
        nodes.push(build_collect_node(
            new_id,
            rewrite.src_position,
            rewrite.src_node_id,
            rewrite.src_pin_index,
            rewrite.element_type.clone(),
        ));
    }

    Ok(())
}

/// Predicate (B) helper: returns `true` iff the destination pin natively
/// accepts `Iter[T]` in v4 (and therefore does NOT need a `collect`
/// inserted on a wire feeding it).
///
/// For built-in destinations this is a hardcoded table lookup. For custom-
/// network destinations the check reads the pin's declared `data_type` from
/// the file's stored `node_type.parameters[dst_param_index].data_type`
/// string and returns `true` iff that string parses as `Iter[..]`. The
/// stored strings are unchanged by v4 — custom networks declare their pin
/// types as the user-authored `DataType`.
fn pin_accepts_iter_v4(dst_type_name: &str, dst_param_index: usize, dst_node: &Value) -> bool {
    if is_iterator_pin_v4(dst_type_name, dst_param_index) {
        return true;
    }

    // Custom-network destination: peek at the declared parameter type. The
    // `node_type` blob on a custom-network instance is not present in the
    // saved file — only on `NodeNetwork.node_type`. Custom-network
    // *instances* in the saved file have `data_type == "custom"` and
    // resolve their pin types via the network registry at load time. So
    // we look up the destination type's parameters from the JSON's network
    // table.
    //
    // The lookup is done in `pin_accepts_iter_v4_with_registry` which
    // takes the parsed network table; the wrapper here is for the common
    // case where we're checking a built-in.
    //
    // For Phase 3 there is no custom-network resolution because no
    // built-in produces a non-trivial `Iter[T]` yet whose downstream
    // could be a custom network with an `Iter[T]`-typed parameter — but
    // the safety guarantee still holds: a v3 custom network never
    // declared `Iter[T]` on any pin, so the predicate must answer
    // `false` for any custom destination, which is what this fallback
    // does (we don't have the registry threaded in; built-in lookup
    // already returned `false`).
    let _ = dst_node; // suppress unused warning until Phase 7 plumbing arrives
    false
}

/// Reads a node's `position` array and snaps both coordinates to integers.
/// Snapping avoids the f64 round-trip ULP drift that breaks
/// `cnnd_roundtrip_test` (documented in `migrate_v2_to_v3.rs:631-633`).
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

/// Synthesises a `collect` node JSON value. Position is 130 units to the
/// right of the source node, on the same y, snapped to integers.
fn build_collect_node(
    id: u64,
    src_position: [f64; 2],
    src_node_id: u64,
    src_pin_index: u64,
    element_type: Value,
) -> Value {
    let position = [src_position[0] + 130.0, src_position[1]];
    let mut wire = serde_json::Map::new();
    wire.insert(src_node_id.to_string(), Value::from(src_pin_index));
    serde_json::json!({
        "id": id,
        "node_type_name": "collect",
        "position": [position[0], position[1]],
        "arguments": [
            { "argument_output_pins": Value::Object(wire) }
        ],
        "data_type": "collect",
        "data": { "element_type": element_type }
    })
}
