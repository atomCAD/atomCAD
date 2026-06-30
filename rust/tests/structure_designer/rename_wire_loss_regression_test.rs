//! Regression test for catastrophic wire loss triggered by a rename-induced
//! global repair pass.
//!
//! See `reports/rename_wire_loss_investigation_2026-06-17.md`.
//!
//! ## What the user observed
//!
//! After a batch of namespace/leaf/record-def renames (`TTPL → proxy(111)`,
//! moving a couple of networks to `proxy(111)-to-tool`, moving record defs into
//! folders), 127 of 357 wires were silently deleted across the whole project —
//! including in networks that were *not* part of any rename.
//!
//! ## Diagnosis
//!
//! The loss is pure deletion (no rerouting) and is perfectly confined to node
//! types whose pin layout / `custom_node_type` is *derived* rather than
//! statically fixed: `parameter`, `expr`, `map`, `filter`, `foreach`, `collect`,
//! `product`, `record_construct`, `record_destructure`. Every fixed-arity node
//! (and every custom-network instance) survived.
//!
//! The mechanism: a rename that touches a record def calls
//! `NodeTypeRegistry::repair_all_networks()`, which re-derives every node's
//! `custom_node_type` with `refresh_args = true`. When the re-derived layout
//! does not match the cached one by parameter id/name (or the cache is absent),
//! `NodeNetwork::set_custom_node_type` rebuilds the `arguments` vector and drops
//! the incoming wires. This is the same bug class already special-cased for the
//! `apply` node (which gets `refresh_args = false`); the other derived-layout
//! node types were never given the same protection.
//!
//! ## The fixture
//!
//! `before.cnnd` is the user's actual known-good project (saved
//! 2026-06-16 17:51 CET) immediately before the corrupting session. We load it,
//! confirm the wires are present, then perform a record-def rename (a clean
//! method-internal trigger of `repair_all_networks`, matching the user's
//! `named_Crystal → util.named_Crystal` move) and assert the wires survive.
//!
//! These assertions are expected to FAIL until the bug is fixed.

use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::canonicalize;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::node_network::{IncomingWire, Node, NodeNetwork};
use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, Parameter};
use rust_lib_flutter_cad::structure_designer::node_type_registry::RecordTypeDef;
use rust_lib_flutter_cad::structure_designer::nodes::closure::ClosureData;
use rust_lib_flutter_cad::structure_designer::nodes::collect::CollectData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

const FIXTURE: &str = "tests/fixtures/rename_wire_loss/before.cnnd";

fn load_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer
        .load_node_networks(FIXTURE)
        .unwrap_or_else(|e| panic!("fixture failed to load: {}", e));
    designer
}

/// Number of incoming wires on `node_id`'s `arg_index`-th argument in the
/// top-level network `network_name`. Panics if the network or node is missing.
fn incoming_wire_count(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    arg_index: usize,
) -> usize {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap_or_else(|| panic!("network '{}' not found", network_name));
    let node = network
        .nodes
        .get(&node_id)
        .unwrap_or_else(|| panic!("node {} not found in '{}'", node_id, network_name));
    node.arguments
        .get(arg_index)
        .map(|a| a.incoming_wires.len())
        .unwrap_or(0)
}

/// The three probe wires below are a representative slice of the 127 lost wires.
/// Each is `(network, node_id, arg_index, dest_node_type)`. All three carry
/// exactly one incoming wire in `before.cnnd`.
///
/// - `experiment.freeform_prism` is NOT touched by any rename — it is a pure
///   victim of the global `repair_all_networks` pass.
/// - `TTPL.hexirod(111)` node 20 is the `parameter "half_width"` whose
///   default-value wire was the spot-checked example in the report.
/// - `TTPL.0_combinatorics` node 5 is a `collect` node (a different
///   derived-layout type) in a heavily-damaged network.
const PROBES: &[(&str, u64, usize, &str)] = &[
    ("experiment.freeform_prism", 31, 0, "parameter"),
    ("TTPL.hexirod(111)", 20, 0, "parameter"),
    ("TTPL.0_combinatorics", 5, 0, "collect"),
];

#[test]
fn probe_wires_present_after_load() {
    // Sanity: the fixture loads with the probe wires intact. If this fails the
    // fixture or load path changed, not the bug under test.
    let designer = load_designer();
    for &(net, node, arg, kind) in PROBES {
        assert_eq!(
            incoming_wire_count(&designer, net, node, arg),
            1,
            "{} node {} ({}) arg{} should have its wire right after load",
            net,
            node,
            kind,
            arg
        );
    }
}

#[test]
fn record_def_rename_does_not_drop_unrelated_wires() {
    let mut designer = load_designer();

    // Precondition: wires present after load.
    for &(net, node, arg, _) in PROBES {
        assert_eq!(
            incoming_wire_count(&designer, net, node, arg),
            1,
            "precondition: {} node {} arg{} wire present after load",
            net,
            node,
            arg
        );
    }

    // Trigger: rename a record def. This calls `repair_all_networks()`, which
    // re-derives every node's `custom_node_type` with `refresh_args = true`.
    // Matches the user's `named_Crystal → util.named_Crystal` move.
    designer
        .rename_record_type_def("named_Crystal", "util.named_Crystal")
        .expect("record def rename should succeed");

    // The rename touches a record def, not any of these nodes' wiring. Every
    // probe wire must survive. (Regression: they are silently deleted.)
    for &(net, node, arg, kind) in PROBES {
        assert_eq!(
            incoming_wire_count(&designer, net, node, arg),
            1,
            "WIRE LOST: {} node {} ({}) arg{} dropped its incoming wire during \
             the rename-triggered repair pass",
            net,
            node,
            kind,
            arg
        );
    }
}

// ---------------------------------------------------------------------------
// Minimal reproduction
// ---------------------------------------------------------------------------
//
// `minimal.cnnd` is the smallest project that triggers the bug (2.3 KB vs the
// 451 KB real project):
//
//   - one record def `R = { x: Int }` (the rename trigger; referenced by
//     nothing — the cache clear is global and unconditional)
//   - one network `victim` with a single derived-layout node: a `parameter`
//     whose default-value pin (arg0) is wired from an `int` node.
//
// The exact mechanism, end to end:
//   1. Load populates every node's `custom_node_type` cache (refresh_args=false
//      preserves args). The parameter wire is intact.
//   2. `rename_record_type_def("R", "R2")` calls
//      `rewrite_record_name_in_registry`, which walks EVERY node in EVERY
//      network and unconditionally sets `node.custom_node_type = None`
//      (node_type_registry.rs ~line 2685, "clear it defensively").
//   3. `repair_all_networks()` re-derives each cache with `refresh_args = true`.
//      With the cache now `None`, `NodeNetwork::set_custom_node_type` takes the
//      rebuild branch, finds no old type to copy argument wires from, and
//      replaces `arguments` with fresh empties — dropping the wire.
//
// Fixed-arity nodes (`int`, `sphere`, …) are immune because their
// `calculate_custom_node_type` returns `None`, so `set_custom_node_type(None,
// _)` never rebuilds their arguments. Only built-in node types that override
// `calculate_custom_node_type` to return `Some` (parameter, expr, map, filter,
// fold, foreach, collect, product, record_construct, record_destructure) are
// affected.

const MINIMAL_FIXTURE: &str = "tests/fixtures/rename_wire_loss/minimal.cnnd";

fn load_minimal() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer
        .load_node_networks(MINIMAL_FIXTURE)
        .unwrap_or_else(|e| panic!("minimal fixture failed to load: {}", e));
    designer
}

#[test]
fn minimal_parameter_default_wire_present_after_load() {
    let designer = load_minimal();
    assert_eq!(
        incoming_wire_count(&designer, "victim", 2, 0),
        1,
        "parameter (node 2) default wire should be present right after load"
    );
}

#[test]
fn minimal_record_rename_drops_parameter_default_wire() {
    let mut designer = load_minimal();

    assert_eq!(
        incoming_wire_count(&designer, "victim", 2, 0),
        1,
        "precondition: parameter default wire present after load"
    );

    // Rename the (unrelated) record def. This alone reproduces the bug.
    designer
        .rename_record_type_def("R", "R2")
        .expect("record def rename should succeed");

    assert_eq!(
        incoming_wire_count(&designer, "victim", 2, 0),
        1,
        "WIRE LOST: the parameter's default-value wire was dropped by the \
         rename-triggered repair pass (cache cleared to None in \
         rewrite_record_name_in_registry, then rebuilt with refresh_args=true)"
    );
}

// ---------------------------------------------------------------------------
// (T3) Undo / redo of a record-def rename
// ---------------------------------------------------------------------------
//
// `RenameRecordTypeDefCommand::{undo,redo}` both route through
// `rename_record_type_def_unchecked` + `repair_all_networks`, so they re-run the
// fixed (in-place recompute) path. The wire must survive the rename, the undo,
// and the redo — and the def name must round-trip.

#[test]
fn minimal_record_rename_undo_redo_keeps_wire() {
    let mut designer = load_minimal();
    designer.set_active_node_network_name(Some("victim".to_string()));

    assert_eq!(
        incoming_wire_count(&designer, "victim", 2, 0),
        1,
        "precondition: parameter default wire present after load"
    );

    designer
        .rename_record_type_def("R", "R2")
        .expect("rename should succeed");
    assert_eq!(
        incoming_wire_count(&designer, "victim", 2, 0),
        1,
        "wire must survive the forward rename"
    );
    assert!(
        designer
            .node_type_registry
            .record_type_defs
            .contains_key("R2")
    );

    assert!(designer.undo(), "undo should be available");
    assert_eq!(
        incoming_wire_count(&designer, "victim", 2, 0),
        1,
        "wire must survive undo of the rename"
    );
    assert!(
        designer
            .node_type_registry
            .record_type_defs
            .contains_key("R"),
        "def name should be R again after undo"
    );

    assert!(designer.redo(), "redo should be available");
    assert_eq!(
        incoming_wire_count(&designer, "victim", 2, 0),
        1,
        "wire must survive redo of the rename"
    );
    assert!(
        designer
            .node_type_registry
            .record_type_defs
            .contains_key("R2")
    );
}

// ---------------------------------------------------------------------------
// (T4) `canonicalize` site composed by hand
// ---------------------------------------------------------------------------
//
// There is no natural `canonicalize -> repair` path in production (canonicalize
// is only called on load, followed by a `refresh_args = false` repopulate), so
// we compose the dangerous sequence by hand: `canonicalize_network` clears the
// caches to `None` (state B), then `repair_node_network` re-derives them with
// `refresh_args = true`. The `parameter` default wire must survive — guaranteed
// here by Change 1's positional-preservation net in `set_custom_node_type`.

#[test]
fn canonicalize_then_repair_keeps_parameter_wire() {
    let mut designer = load_minimal();

    let mut net = designer
        .node_type_registry
        .node_networks
        .remove("victim")
        .expect("victim network present");

    assert_eq!(
        net.nodes
            .get(&2)
            .and_then(|n| n.arguments.first())
            .map(|a| a.incoming_wires.len())
            .unwrap_or(0),
        1,
        "precondition: parameter default wire present before the dangerous sequence"
    );

    // The dangerous sequence: clear caches (state B), then a refresh_args=true
    // repair over the now-None caches.
    canonicalize::canonicalize_network(&mut net);
    designer.node_type_registry.repair_node_network(&mut net);

    assert_eq!(
        net.nodes
            .get(&2)
            .and_then(|n| n.arguments.first())
            .map(|a| a.incoming_wires.len())
            .unwrap_or(0),
        1,
        "WIRE LOST: canonicalize -> repair(refresh_args=true) dropped the \
         parameter default wire (Change 1 should have preserved it positionally)"
    );

    designer
        .node_type_registry
        .node_networks
        .insert("victim".to_string(), net);
}

// ---------------------------------------------------------------------------
// (T5) Change 1 direct unit tests — the grow / shrink branches are NOT
// reachable via the structure-preserving rename/canonicalize ops (pin count
// never changes there), so they need direct `set_custom_node_type` coverage.
// ---------------------------------------------------------------------------

/// Build a `Node` of `node_type_name` with `marker_sources.len()` arguments,
/// each carrying a single incoming wire whose `source_node_id` is the marker
/// (so we can track which wire ends up at which index). `cache` is the node's
/// `custom_node_type` (use `None` to model the stale state B).
fn make_node_with_markers(
    node_type_name: &str,
    marker_sources: &[u64],
    cache: Option<NodeType>,
) -> Node {
    let designer = StructureDesigner::new();
    let base = designer
        .node_type_registry
        .built_in_node_types
        .get(node_type_name)
        .expect("built-in type");
    let data = (base.node_data_creator)();

    let mut net = NodeNetwork::new_empty();
    let id = net.add_node(node_type_name, DVec2::ZERO, marker_sources.len(), data);
    let mut node = net.nodes.remove(&id).unwrap();
    for (i, &src) in marker_sources.iter().enumerate() {
        node.arguments[i].incoming_wires = vec![IncomingWire::node_output(src, 0)];
    }
    node.custom_node_type = cache;
    node
}

/// A `NodeType` (cloned from a real built-in so its fn pointers are valid) whose
/// `parameters` are replaced with the given `(name, id)` list.
fn node_type_with_params(node_type_name: &str, params: &[(&str, Option<u64>)]) -> NodeType {
    let designer = StructureDesigner::new();
    let mut nt = designer
        .node_type_registry
        .built_in_node_types
        .get(node_type_name)
        .expect("built-in type")
        .clone();
    nt.parameters = params
        .iter()
        .map(|(name, id)| Parameter {
            id: *id,
            name: name.to_string(),
            data_type: DataType::Int,
        })
        .collect();
    nt
}

/// The marker (`source_node_id`) of the first wire on argument `i`, or `None`
/// if that argument has no wire / does not exist.
fn marker_at(node: &Node, i: usize) -> Option<u64> {
    node.arguments
        .get(i)
        .and_then(|a| a.incoming_wires.first())
        .map(|w| w.source_node_id)
}

#[test]
fn set_custom_node_type_none_cache_equal_count_preserves_wires() {
    // Equal count, no old cache (state B) + refresh_args=true → all wires kept.
    let mut node = make_node_with_markers("union", &[10, 20, 30], None);
    let nt = node_type_with_params("union", &[("a", None), ("b", None), ("c", None)]);
    node.set_custom_node_type(Some(nt), true);

    assert_eq!(node.arguments.len(), 3);
    assert_eq!(marker_at(&node, 0), Some(10));
    assert_eq!(marker_at(&node, 1), Some(20));
    assert_eq!(marker_at(&node, 2), Some(30));
}

#[test]
fn set_custom_node_type_none_cache_grow_preserves_prefix_and_pads() {
    // Grow (new count > current args), no old cache → existing wires preserved
    // in place, new slots empty.
    let mut node = make_node_with_markers("union", &[10, 20], None);
    let nt = node_type_with_params(
        "union",
        &[("a", None), ("b", None), ("c", None), ("d", None)],
    );
    node.set_custom_node_type(Some(nt), true);

    assert_eq!(node.arguments.len(), 4);
    assert_eq!(marker_at(&node, 0), Some(10));
    assert_eq!(marker_at(&node, 1), Some(20));
    assert_eq!(marker_at(&node, 2), None, "new slot should be empty");
    assert_eq!(marker_at(&node, 3), None, "new slot should be empty");
}

#[test]
fn set_custom_node_type_none_cache_shrink_keeps_prefix_drops_tail() {
    // Shrink (new count < current args), no old cache → prefix kept, tail dropped.
    let mut node = make_node_with_markers("union", &[10, 20, 30], None);
    let nt = node_type_with_params("union", &[("a", None), ("b", None)]);
    node.set_custom_node_type(Some(nt), true);

    assert_eq!(node.arguments.len(), 2);
    assert_eq!(marker_at(&node, 0), Some(10));
    assert_eq!(marker_at(&node, 1), Some(20));
}

#[test]
fn set_custom_node_type_with_old_cache_reorder_still_moves_wires_by_id() {
    // Non-regression: with an old cache present, the existing copy-by-id path
    // must be unchanged — a reordered parameter list moves each wire to its
    // matched slot (guards against Change 1 altering the with-old-cache path).
    let old = node_type_with_params("union", &[("a", Some(1)), ("b", Some(2))]);
    // args: a(slot0)=10, b(slot1)=20
    let mut node = make_node_with_markers("union", &[10, 20], Some(old));
    // New layout swaps the order by id: b(id2) first, a(id1) second.
    let new = node_type_with_params("union", &[("b", Some(2)), ("a", Some(1))]);
    node.set_custom_node_type(Some(new), true);

    assert_eq!(node.arguments.len(), 2);
    assert_eq!(
        marker_at(&node, 0),
        Some(20),
        "b's wire (was slot1) should move to slot0"
    );
    assert_eq!(
        marker_at(&node, 1),
        Some(10),
        "a's wire (was slot0) should move to slot1"
    );
}

// ---------------------------------------------------------------------------
// (T7) Change 3 assertion actually guards
// ---------------------------------------------------------------------------
//
// White-box: force a derived node (the `parameter`) into state B (cache = None)
// and run the invariant-check entry point (`validate_active_network`). The
// debug-only `debug_assert!` at the end of `validate_network` must fire. Gated
// to debug builds, where `debug_assert!` is active (the full suite runs in
// debug; this only documents that the assert has teeth).

// ---------------------------------------------------------------------------
// (T8) Rename must rewrite EVERY node-data DataType field, not just the subset
// the original walker knew about.
// ---------------------------------------------------------------------------
//
// `rewrite_record_name_in_registry` (and its read-mirror
// `collect_record_refs_in_network`) downcast to a hand-maintained list of
// node-data variants. Three derived-layout types that embed a `DataType` were
// missing from that list: `closure` / `apply` (`type_args: Vec<DataType>`) and
// `collect` (`element_type: DataType`). The complete reference list is
// `canonicalize::canonicalize_node_data`.
//
// The user-observed symptom: after renaming a record def into a namespace and
// reloading, body-internal `record_destructure` schemas (which WERE rewritten)
// resolved fine but the enclosing `closure`'s `type_args` (which were NOT)
// still pointed at the old name → a dangling `Record(Named(old))` reference →
// red validation error / wire fragility when the user reverted the rename.
//
// This test builds a `closure` and a `collect` node that both embed
// `Record(Named("R"))`, renames `R → R2`, and asserts the embedded references
// followed the rename (no stale `"R"` left behind).

fn record_named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

/// Returns every record-def name embedded in `dt` via `RecordType::Named`.
fn named_refs(dt: &DataType, out: &mut Vec<String>) {
    use rust_lib_flutter_cad::structure_designer::data_type::walk_data_type_record_names_mut;
    // No read-only walker exists; the `_mut` one only reads here (we never
    // mutate the name), so clone and walk the copy.
    let mut copy = dt.clone();
    walk_data_type_record_names_mut(&mut copy, &mut |n| out.push(n.clone()));
}

#[test]
fn record_rename_rewrites_closure_and_collect_type_fields() {
    let mut designer = StructureDesigner::new();

    // A record def to rename.
    designer
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "R".to_string(),
            vec![("x".to_string(), DataType::Int)],
        ))
        .expect("add record def R");

    // A network with a `closure` (type_args reference R) and a `collect`
    // (element_type references R) — the two node types that the rename walker
    // previously skipped.
    let mut net = NodeNetwork::new_empty();
    net.node_type.name = "victim".to_string();

    let closure_data = Box::new(ClosureData {
        type_args: vec![record_named("R")],
        ..Default::default()
    });
    let closure_id = net.add_node("closure", DVec2::ZERO, 0, closure_data);

    let collect_data = Box::new(CollectData {
        element_type: record_named("R"),
        ..Default::default()
    });
    let collect_id = net.add_node("collect", DVec2::new(200.0, 0.0), 1, collect_data);

    designer
        .node_type_registry
        .node_networks
        .insert("victim".to_string(), net);

    // Precondition: both embed "R".
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get("victim")
            .unwrap();
        let mut refs = Vec::new();
        let cd = net.nodes[&closure_id]
            .data
            .as_any_ref()
            .downcast_ref::<ClosureData>()
            .unwrap();
        for t in &cd.type_args {
            named_refs(t, &mut refs);
        }
        let col = net.nodes[&collect_id]
            .data
            .as_any_ref()
            .downcast_ref::<CollectData>()
            .unwrap();
        named_refs(&col.element_type, &mut refs);
        assert_eq!(refs, vec!["R", "R"], "precondition: both nodes reference R");
    }

    // The rename.
    designer
        .rename_record_type_def("R", "R2")
        .expect("rename R -> R2");

    // Both embedded references must have followed the rename — no stale "R".
    let net = designer
        .node_type_registry
        .node_networks
        .get("victim")
        .unwrap();

    let cd = net.nodes[&closure_id]
        .data
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .unwrap();
    let mut closure_refs = Vec::new();
    for t in &cd.type_args {
        named_refs(t, &mut closure_refs);
    }
    assert_eq!(
        closure_refs,
        vec!["R2"],
        "closure.type_args still references the old record name after rename \
         (rewrite_record_name_in_registry missed ClosureData)"
    );

    let col = net.nodes[&collect_id]
        .data
        .as_any_ref()
        .downcast_ref::<CollectData>()
        .unwrap();
    let mut collect_refs = Vec::new();
    named_refs(&col.element_type, &mut collect_refs);
    assert_eq!(
        collect_refs,
        vec!["R2"],
        "collect.element_type still references the old record name after rename \
         (rewrite_record_name_in_registry missed CollectData)"
    );
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "custom_node_type cache invariant violated")]
fn invariant_assertion_fires_on_forced_none_cache() {
    let mut designer = load_minimal();
    designer.set_active_node_network_name(Some("victim".to_string()));

    // Force the derived `parameter` node (id 2) into state B.
    designer
        .node_type_registry
        .node_networks
        .get_mut("victim")
        .unwrap()
        .nodes
        .get_mut(&2)
        .unwrap()
        .custom_node_type = None;

    // The invariant check at the end of validate_network should panic.
    designer.validate_active_network();
}
