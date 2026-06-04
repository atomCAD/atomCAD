//! Phase 0 tests for the reflow-on-footprint-change primitives
//! (`doc/design_reflow_on_footprint_change.md`):
//!
//! - `CompositeCommand` undo/redo ordering,
//! - `combine_refresh_modes` folding table,
//! - `StructureDesigner::reflow_for_footprint_change` spatial behaviour
//!   (single scope + the cascade across two body levels), pure — no undo wiring.

use glam::f64::DVec2;
use std::sync::{Arc, Mutex};

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_inlining::instance_size;
use rust_lib_flutter_cad::structure_designer::node_network::CollapseMode;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::undo::commands::composite::CompositeCommand;
use rust_lib_flutter_cad::structure_designer::undo::{
    UndoCommand, UndoContext, UndoRefreshMode, combine_refresh_modes,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Estimated rendered size of the node at `scope_path` / `id`, using the same
/// authority (`node_inlining::instance_size`) that reflow uses internally.
fn node_size(designer: &StructureDesigner, scope_path: &[u64], id: u64) -> DVec2 {
    let net = designer.get_scope_network(scope_path).unwrap();
    let node = net.nodes.get(&id).unwrap();
    instance_size(node, &designer.node_type_registry)
}

fn pos(designer: &StructureDesigner, scope_path: &[u64], id: u64) -> DVec2 {
    designer
        .get_scope_network(scope_path)
        .unwrap()
        .nodes
        .get(&id)
        .unwrap()
        .position
}

fn grew(new: DVec2, old: DVec2) -> DVec2 {
    (new - old).max(DVec2::ZERO)
}

// ---------------------------------------------------------------------------
// CompositeCommand
// ---------------------------------------------------------------------------

/// An `UndoCommand` that appends a tag to a shared log on undo/redo, so a test
/// can observe the order children are invoked in.
#[derive(Debug)]
struct RecordingCommand {
    tag: String,
    log: Arc<Mutex<Vec<String>>>,
    refresh: UndoRefreshMode,
}

impl UndoCommand for RecordingCommand {
    fn description(&self) -> &str {
        &self.tag
    }

    fn undo(&self, _ctx: &mut UndoContext) {
        self.log.lock().unwrap().push(format!("undo:{}", self.tag));
    }

    fn redo(&self, _ctx: &mut UndoContext) {
        self.log.lock().unwrap().push(format!("redo:{}", self.tag));
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        self.refresh.clone()
    }
}

fn recording(
    tag: &str,
    log: &Arc<Mutex<Vec<String>>>,
    refresh: UndoRefreshMode,
) -> RecordingCommand {
    RecordingCommand {
        tag: tag.to_string(),
        log: Arc::clone(log),
        refresh,
    }
}

#[test]
fn composite_redo_forward_undo_reverse_order() {
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let composite = CompositeCommand {
        description: "compound".to_string(),
        commands: vec![
            Box::new(recording("a", &log, UndoRefreshMode::Lightweight)),
            Box::new(recording("b", &log, UndoRefreshMode::Lightweight)),
            Box::new(recording("c", &log, UndoRefreshMode::Lightweight)),
        ],
    };

    let mut designer = setup_designer_with_network("main");
    let mut ctx = UndoContext {
        node_type_registry: &mut designer.node_type_registry,
        active_network_name: &mut designer.active_node_network_name,
    };

    composite.redo(&mut ctx);
    composite.undo(&mut ctx);

    assert_eq!(composite.description(), "compound");
    assert_eq!(
        *log.lock().unwrap(),
        vec![
            // redo: forward order
            "redo:a", "redo:b", "redo:c", // undo: reverse order
            "undo:c", "undo:b", "undo:a",
        ]
    );
}

#[test]
fn composite_refresh_mode_is_strongest_child() {
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Lightweight + NodeDataChanged ⇒ NodeDataChanged.
    let c1 = CompositeCommand {
        description: "c1".to_string(),
        commands: vec![
            Box::new(recording("a", &log, UndoRefreshMode::Lightweight)),
            Box::new(recording(
                "b",
                &log,
                UndoRefreshMode::NodeDataChanged(vec![7]),
            )),
        ],
    };
    assert!(matches!(
        c1.refresh_mode(),
        UndoRefreshMode::NodeDataChanged(ids) if ids == vec![7]
    ));

    // Any Full dominates.
    let c2 = CompositeCommand {
        description: "c2".to_string(),
        commands: vec![
            Box::new(recording(
                "a",
                &log,
                UndoRefreshMode::NodeDataChanged(vec![1]),
            )),
            Box::new(recording("b", &log, UndoRefreshMode::Full)),
        ],
    };
    assert!(matches!(c2.refresh_mode(), UndoRefreshMode::Full));
}

// ---------------------------------------------------------------------------
// combine_refresh_modes
// ---------------------------------------------------------------------------

#[test]
fn combine_refresh_modes_table() {
    // Empty ⇒ Lightweight.
    assert!(matches!(
        combine_refresh_modes(std::iter::empty()),
        UndoRefreshMode::Lightweight
    ));

    // All Lightweight ⇒ Lightweight.
    assert!(matches!(
        combine_refresh_modes([UndoRefreshMode::Lightweight, UndoRefreshMode::Lightweight]),
        UndoRefreshMode::Lightweight
    ));

    // Lightweight contributes nothing; NodeDataChanged survives.
    assert!(matches!(
        combine_refresh_modes([
            UndoRefreshMode::Lightweight,
            UndoRefreshMode::NodeDataChanged(vec![1, 2]),
            UndoRefreshMode::Lightweight,
        ]),
        UndoRefreshMode::NodeDataChanged(ids) if ids == vec![1, 2]
    ));

    // Multiple NodeDataChanged ⇒ union (concatenation).
    assert!(matches!(
        combine_refresh_modes([
            UndoRefreshMode::NodeDataChanged(vec![1]),
            UndoRefreshMode::NodeDataChanged(vec![2, 3]),
        ]),
        UndoRefreshMode::NodeDataChanged(ids) if ids == vec![1, 2, 3]
    ));

    // Any Full dominates, regardless of order.
    assert!(matches!(
        combine_refresh_modes([UndoRefreshMode::Lightweight, UndoRefreshMode::Full]),
        UndoRefreshMode::Full
    ));
    assert!(matches!(
        combine_refresh_modes([
            UndoRefreshMode::NodeDataChanged(vec![1]),
            UndoRefreshMode::Full,
            UndoRefreshMode::NodeDataChanged(vec![2]),
        ]),
        UndoRefreshMode::Full
    ));
}

// ---------------------------------------------------------------------------
// reflow_for_footprint_change — single scope
// ---------------------------------------------------------------------------

#[test]
fn reflow_single_scope_pushes_lower_right_neighbour() {
    let mut designer = setup_designer_with_network("main");

    // An expanded `map` HOF at the origin is the node that will grow (its
    // rendered footprint follows its body content).
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));
    let old_size = node_size(&designer, &[], map_id);

    // Trigger growth FIRST: drop a node far out in the map's body so its
    // rendered body (and thus the map's footprint) expands well past the
    // default. `add_node_scoped` now reflows the body's owning scope itself
    // (Case C, Phase 3), so we grow *before* placing the neighbours — there is
    // nothing for that internal reflow to push yet, leaving the manual
    // `reflow_for_footprint_change` below as the sole reflow under test.
    designer.add_node_scoped(&[map_id], "union", DVec2::new(1200.0, 1200.0), None);

    // A neighbour strictly in the lower-right sweep band (past both the right
    // and bottom edges of the map's *original* footprint) — it should shift on
    // both axes by the growth delta. Placed via top-level `add_node`, which
    // does not reflow.
    let lower_right =
        designer.add_node("union", DVec2::new(old_size.x + 500.0, old_size.y + 500.0));
    let lr_before = pos(&designer, &[], lower_right);

    // A neighbour completely above-and-left of the map — never reached by the
    // rightward/downward growth, so it must stay put.
    let safe = designer.add_node("union", DVec2::new(-600.0, -600.0));
    let safe_before = pos(&designer, &[], safe);

    let new_size = node_size(&designer, &[], map_id);
    let delta = grew(new_size, old_size);
    assert!(
        delta.x > 0.0 && delta.y > 0.0,
        "expected the map to grow on both axes, got delta {delta:?}"
    );

    let scoped_moves = designer.reflow_for_footprint_change(&[], map_id, &[old_size]);

    // One scope touched (the top-level network).
    assert_eq!(scoped_moves.len(), 1);
    let sm = &scoped_moves[0];
    assert_eq!(sm.scope_path, Vec::<u64>::new());

    // The lower-right neighbour moved by exactly the delta; the grown node and
    // the safe node are absent from the move list.
    assert_eq!(
        sm.moves.len(),
        1,
        "only the lower-right neighbour should move"
    );
    let (moved_id, old_pos, new_pos) = sm.moves[0];
    assert_eq!(moved_id, lower_right);
    assert_eq!(old_pos, lr_before);
    assert_eq!(new_pos, lr_before + delta);

    // The actual stored positions match the reported moves.
    assert_eq!(pos(&designer, &[], lower_right), lr_before + delta);
    assert_eq!(pos(&designer, &[], safe), safe_before);
}

#[test]
fn reflow_no_growth_returns_empty() {
    let mut designer = setup_designer_with_network("main");
    let union_id = designer.add_node("union", DVec2::new(0.0, 0.0));
    let neighbour = designer.add_node("union", DVec2::new(500.0, 500.0));
    let neighbour_before = pos(&designer, &[], neighbour);

    // Pass the node's *current* size as `old` — nothing grew, so delta == 0.
    let current = node_size(&designer, &[], union_id);
    let scoped_moves = designer.reflow_for_footprint_change(&[], union_id, &[current]);

    assert!(scoped_moves.is_empty());
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before);
}

// ---------------------------------------------------------------------------
// reflow_for_footprint_change — cascade across two body levels
// ---------------------------------------------------------------------------

#[test]
fn reflow_cascades_across_two_body_levels() {
    let mut designer = setup_designer_with_network("main");

    // Top-level expanded `map` m1.
    let m1 = designer.add_node("map", DVec2::new(0.0, 0.0));
    // Nested `map` m2 inside m1's body (add_node_scoped runs ensure_zone_init).
    let m2 = designer.add_node_scoped(&[m1], "map", DVec2::new(0.0, 0.0), None);

    // Capture the pre-trigger footprints (both ancestors) — the bands the
    // neighbours below sit just outside of, and the `old_sizes` the manual
    // reflow re-measures growth against.
    let m2_old = node_size(&designer, &[m1], m2);
    let m1_old = node_size(&designer, &[], m1);

    // Trigger growth FIRST, before any sibling neighbours exist. This grows m2
    // (in m1's body) and, because m1's rendered body recurses into m2, grows m1
    // (in the top-level network). `add_node_scoped` now reflows internally
    // (Case C, Phase 3); doing it before placing neighbours leaves nothing for
    // that pass to push, so the manual reflow below is the only one acting on
    // the tracked neighbours.
    designer.add_node_scoped(&[m1, m2], "union", DVec2::new(1500.0, 1500.0), None);

    // A sibling of m2 inside m1's body, in m2's *pre-trigger* lower-right band.
    let s_mid = designer.add_node_scoped(
        &[m1],
        "union",
        DVec2::new(m2_old.x + 500.0, m2_old.y + 500.0),
        None,
    );
    // A sibling of m1 in the top-level network, in m1's *pre-trigger* band.
    let s_top = designer.add_node("union", DVec2::new(m1_old.x + 500.0, m1_old.y + 500.0));

    let s_mid_before = pos(&designer, &[m1], s_mid);
    let s_top_before = pos(&designer, &[], s_top);

    // Start the cascade in m1's body for m2; it climbs one level to the top.
    let scoped_moves = designer.reflow_for_footprint_change(&[m1], m2, &[m2_old, m1_old]);

    assert_eq!(
        scoped_moves.len(),
        2,
        "expected one entry per scope touched"
    );

    // Entry 0: the body scope [m1] — s_mid pushed by m2's growth.
    let body_entry = scoped_moves
        .iter()
        .find(|sm| sm.scope_path == vec![m1])
        .expect("expected a ScopedMoves for the [m1] body scope");
    let m2_new = node_size(&designer, &[m1], m2);
    let delta_mid = grew(m2_new, m2_old);
    assert!(delta_mid.x > 0.0 && delta_mid.y > 0.0);
    assert_eq!(body_entry.moves.len(), 1);
    let (mid_id, mid_old, mid_new) = body_entry.moves[0];
    assert_eq!(mid_id, s_mid);
    assert_eq!(mid_old, s_mid_before);
    assert_eq!(mid_new, s_mid_before + delta_mid);

    // Entry 1: the top-level scope [] — s_top pushed by m1's growth.
    let top_entry = scoped_moves
        .iter()
        .find(|sm| sm.scope_path.is_empty())
        .expect("expected a ScopedMoves for the top-level scope");
    let m1_new = node_size(&designer, &[], m1);
    let delta_top = grew(m1_new, m1_old);
    assert!(delta_top.x > 0.0 && delta_top.y > 0.0);
    assert_eq!(top_entry.moves.len(), 1);
    let (top_moved_id, top_old, top_new) = top_entry.moves[0];
    assert_eq!(top_moved_id, s_top);
    assert_eq!(top_old, s_top_before);
    assert_eq!(top_new, s_top_before + delta_top);

    // Stored positions reflect the reported moves.
    assert_eq!(pos(&designer, &[m1], s_mid), s_mid_before + delta_mid);
    assert_eq!(pos(&designer, &[], s_top), s_top_before + delta_top);
}

// ---------------------------------------------------------------------------
// Phase 1 — Case B: set_collapse_mode reflow + single-step undo
// (doc/design_reflow_on_footprint_change.md §"Case B")
// ---------------------------------------------------------------------------

fn collapse_mode(designer: &StructureDesigner, scope_path: &[u64], id: u64) -> CollapseMode {
    designer
        .get_scope_network(scope_path)
        .unwrap()
        .nodes
        .get(&id)
        .unwrap()
        .collapse_mode
}

/// Expanding a top-level compact HOF pushes its lower-right neighbour out of the
/// way, and a single undo restores **both** the collapse mode and the neighbour
/// position; redo re-applies both.
#[test]
fn set_collapse_mode_expand_pushes_neighbour_single_step_undo() {
    let mut designer = setup_designer_with_network("main");
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));
    // Grow the body so expansion is a meaningful footprint change.
    designer.add_node_scoped(&[map_id], "union", DVec2::new(1200.0, 1200.0), None);

    // Compact it first (Auto/expanded → Collapsed is a shrink, so it moves
    // nothing) to establish a small starting footprint.
    designer.set_collapse_mode(&[], map_id, CollapseMode::Collapsed);
    let compact_size = node_size(&designer, &[], map_id);

    // Neighbour in the compact footprint's lower-right sweep band.
    let neighbour = designer.add_node(
        "union",
        DVec2::new(compact_size.x + 500.0, compact_size.y + 500.0),
    );
    let neighbour_before = pos(&designer, &[], neighbour);
    designer.undo_stack.clear();

    // Expand → footprint grows → neighbour pushed by the growth delta.
    designer.set_collapse_mode(&[], map_id, CollapseMode::Expanded);
    let expanded_size = node_size(&designer, &[], map_id);
    let delta = grew(expanded_size, compact_size);
    assert!(
        delta.x > 0.0 && delta.y > 0.0,
        "expected the map to grow on both axes, got delta {delta:?}"
    );
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before + delta);
    assert_eq!(
        collapse_mode(&designer, &[], map_id),
        CollapseMode::Expanded
    );

    // Single-step undo restores BOTH the mode and the neighbour position.
    assert!(designer.undo());
    assert_eq!(
        collapse_mode(&designer, &[], map_id),
        CollapseMode::Collapsed
    );
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before);
    // It really was one step — nothing left to undo (stack was cleared above).
    assert!(!designer.undo_stack.can_undo());

    // Redo re-applies both in one step.
    assert!(designer.redo());
    assert_eq!(
        collapse_mode(&designer, &[], map_id),
        CollapseMode::Expanded
    );
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before + delta);
}

/// Collapsing (a shrink) moves nothing and records a single bare command.
#[test]
fn set_collapse_mode_collapse_moves_nothing() {
    let mut designer = setup_designer_with_network("main");
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));
    designer.add_node_scoped(&[map_id], "union", DVec2::new(1200.0, 1200.0), None);

    // map is Auto/expanded; put a neighbour in its expanded lower-right band.
    let expanded_size = node_size(&designer, &[], map_id);
    let neighbour = designer.add_node(
        "union",
        DVec2::new(expanded_size.x + 500.0, expanded_size.y + 500.0),
    );
    let neighbour_before = pos(&designer, &[], neighbour);
    designer.undo_stack.clear();

    // Collapse (shrink) — the gap left behind is harmless; pull nothing inward.
    designer.set_collapse_mode(&[], map_id, CollapseMode::Collapsed);
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before);

    // Single step (no bundled moves): one undo empties the cleared stack.
    assert!(designer.undo());
    assert_eq!(collapse_mode(&designer, &[], map_id), CollapseMode::Auto);
    assert!(!designer.undo_stack.can_undo());
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before);
}

/// A nested HOF expanded inside an outer HOF's body grows the body, cascading
/// the reflow to the outer's parent (top-level) network; one undo step restores
/// the mode and every reflowed position across both scopes.
#[test]
fn set_collapse_mode_nested_expand_cascades_and_undoes_single_step() {
    let mut designer = setup_designer_with_network("main");
    let outer = designer.add_node("map", DVec2::new(0.0, 0.0));
    let inner = designer.add_node_scoped(&[outer], "map", DVec2::new(0.0, 0.0), None);
    // Grow the inner body so expanding `inner` is a real footprint change.
    designer.add_node_scoped(&[outer, inner], "union", DVec2::new(1200.0, 1200.0), None);

    // Compact `inner` first so expansion is the growing direction.
    designer.set_collapse_mode(&[outer], inner, CollapseMode::Collapsed);

    // Sibling of `inner` inside the outer body, in `inner`'s lower-right band.
    let inner_compact = node_size(&designer, &[outer], inner);
    let s_body = designer.add_node_scoped(
        &[outer],
        "union",
        DVec2::new(inner_compact.x + 500.0, inner_compact.y + 500.0),
        None,
    );
    // Sibling of `outer` at top level, in `outer`'s lower-right band.
    let outer_size = node_size(&designer, &[], outer);
    let s_top = designer.add_node(
        "union",
        DVec2::new(outer_size.x + 500.0, outer_size.y + 500.0),
    );

    let s_body_before = pos(&designer, &[outer], s_body);
    let s_top_before = pos(&designer, &[], s_top);
    let inner_old = node_size(&designer, &[outer], inner);
    let outer_old = node_size(&designer, &[], outer);
    designer.undo_stack.clear();

    // Expand `inner`: grows the outer body and cascades to the top level.
    designer.set_collapse_mode(&[outer], inner, CollapseMode::Expanded);

    let delta_body = grew(node_size(&designer, &[outer], inner), inner_old);
    let delta_top = grew(node_size(&designer, &[], outer), outer_old);
    assert!(delta_body.x > 0.0 && delta_body.y > 0.0);
    assert!(delta_top.x > 0.0 && delta_top.y > 0.0);
    assert_eq!(pos(&designer, &[outer], s_body), s_body_before + delta_body);
    assert_eq!(pos(&designer, &[], s_top), s_top_before + delta_top);

    // One undo restores the mode and both reflowed positions.
    assert!(designer.undo());
    assert_eq!(
        collapse_mode(&designer, &[outer], inner),
        CollapseMode::Collapsed
    );
    assert_eq!(pos(&designer, &[outer], s_body), s_body_before);
    assert_eq!(pos(&designer, &[], s_top), s_top_before);
    assert!(!designer.undo_stack.can_undo());

    // Redo re-applies all of it.
    assert!(designer.redo());
    assert_eq!(
        collapse_mode(&designer, &[outer], inner),
        CollapseMode::Expanded
    );
    assert_eq!(pos(&designer, &[outer], s_body), s_body_before + delta_body);
    assert_eq!(pos(&designer, &[], s_top), s_top_before + delta_top);
}

// ---------------------------------------------------------------------------
// Phase 2 — Case A: f-pin disconnect reflow + single-step undo
// (doc/design_reflow_on_footprint_change.md §"Case A")
// ---------------------------------------------------------------------------

/// Build an `Int→Int` `map` at `scope_path` whose own inline body is grown by a
/// `union` (so expansion is a clear footprint change), with a `closure` wired
/// into its `f` pin so the map renders **compact** (Auto mode, f connected).
/// Returns `(map_id, closure_id)`. Removing the f wire — directly or by deleting
/// the closure — flips the map to expanded.
fn add_collapsed_map_with_closure_f(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    map_pos: DVec2,
    closure_pos: DVec2,
) -> (u64, u64) {
    let map_id = designer.add_node_scoped(scope_path, "map", map_pos, None);
    designer.set_node_network_data_scoped(
        scope_path,
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    // Grow the map's own inline body so the expand is a meaningful footprint
    // change once `f` is disconnected and the body becomes visible again.
    let map_body: Vec<u64> = scope_path.iter().copied().chain([map_id]).collect();
    designer.add_node_scoped(&map_body, "union", DVec2::new(1200.0, 1200.0), None);

    let closure_id = designer.add_node_scoped(scope_path, "closure", closure_pos, None);
    designer.set_node_network_data_scoped(
        scope_path,
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
    );
    // f is map parameter index 1; wiring it makes the map compact (Auto mode).
    designer.connect_nodes_scoped(scope_path, closure_id, 0, map_id, 1);

    let f_wired = !designer
        .get_scope_network(scope_path)
        .unwrap()
        .nodes
        .get(&map_id)
        .unwrap()
        .arguments[1]
        .incoming_wires
        .is_empty();
    assert!(
        f_wired,
        "closure should be wired into the map's f pin (map compact)"
    );

    (map_id, closure_id)
}

/// Deleting the `f` **wire** feeding a compact top-level `map` expands it and
/// pushes its lower-right neighbour; a single undo restores **both** the wire
/// (map compact again) and the neighbour position; redo re-applies both.
#[test]
fn delete_f_wire_expands_map_and_reflows_single_step_undo() {
    let mut designer = setup_designer_with_network("main");
    let (map_id, closure_id) = add_collapsed_map_with_closure_f(
        &mut designer,
        &[],
        DVec2::new(0.0, 0.0),
        DVec2::new(-400.0, 0.0),
    );

    let compact_size = node_size(&designer, &[], map_id);
    let neighbour = designer.add_node(
        "union",
        DVec2::new(compact_size.x + 500.0, compact_size.y + 500.0),
    );
    let neighbour_before = pos(&designer, &[], neighbour);
    designer.undo_stack.clear();

    // Select and delete the f wire.
    assert!(designer.select_wire(closure_id, 0, map_id, 1));
    designer.delete_selected();

    let expanded_size = node_size(&designer, &[], map_id);
    let delta = grew(expanded_size, compact_size);
    assert!(
        delta.x > 0.0 && delta.y > 0.0,
        "map should expand once its f wire is gone, got delta {delta:?}"
    );
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before + delta);

    // One undo restores the wire (map back to compact) and the neighbour.
    assert!(designer.undo());
    assert_eq!(node_size(&designer, &[], map_id), compact_size);
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before);
    assert!(!designer.undo_stack.can_undo());

    // Redo re-applies both.
    assert!(designer.redo());
    assert_eq!(node_size(&designer, &[], map_id), expanded_size);
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before + delta);
}

/// Deleting the `f`-**source node** (the closure) disconnects `f` the same way:
/// the map expands and its neighbour is pushed, restored in one undo step.
#[test]
fn delete_f_source_node_expands_map_and_reflows() {
    let mut designer = setup_designer_with_network("main");
    let (map_id, closure_id) = add_collapsed_map_with_closure_f(
        &mut designer,
        &[],
        DVec2::new(0.0, 0.0),
        DVec2::new(-400.0, 0.0),
    );

    let compact_size = node_size(&designer, &[], map_id);
    let neighbour = designer.add_node(
        "union",
        DVec2::new(compact_size.x + 500.0, compact_size.y + 500.0),
    );
    let neighbour_before = pos(&designer, &[], neighbour);
    designer.undo_stack.clear();

    // Delete the closure node that feeds f.
    assert!(designer.select_node(closure_id));
    designer.delete_selected();

    let expanded_size = node_size(&designer, &[], map_id);
    let delta = grew(expanded_size, compact_size);
    assert!(delta.x > 0.0 && delta.y > 0.0);
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before + delta);

    // Single undo restores the closure, the wire, and the neighbour position.
    assert!(designer.undo());
    assert_eq!(node_size(&designer, &[], map_id), compact_size);
    assert_eq!(pos(&designer, &[], neighbour), neighbour_before);
    assert!(!designer.undo_stack.can_undo());
}

/// Deleting the `f` wire of a compact `map` **inside a zone body** expands it
/// and pushes its in-body neighbour; the move rides the `EditZoneBodyCommand`
/// after-snapshot, so one undo restores the whole body in a single step.
#[test]
fn delete_f_wire_in_body_reflows_body_single_step_undo() {
    let mut designer = setup_designer_with_network("main");
    let outer = designer.add_node("map", DVec2::new(0.0, 0.0));

    let (inner_map, inner_closure) = add_collapsed_map_with_closure_f(
        &mut designer,
        &[outer],
        DVec2::new(0.0, 0.0),
        DVec2::new(-400.0, 0.0),
    );

    let compact_size = node_size(&designer, &[outer], inner_map);
    let neighbour = designer.add_node_scoped(
        &[outer],
        "union",
        DVec2::new(compact_size.x + 500.0, compact_size.y + 500.0),
        None,
    );
    let neighbour_before = pos(&designer, &[outer], neighbour);
    designer.undo_stack.clear();

    // Select + delete the f wire inside the outer body.
    assert!(designer.select_wire_scoped(&[outer], inner_closure, 0, inner_map, 1));
    designer.delete_selected_scoped(&[outer]);

    let expanded_size = node_size(&designer, &[outer], inner_map);
    let delta = grew(expanded_size, compact_size);
    assert!(
        delta.x > 0.0 && delta.y > 0.0,
        "inner map should expand inside the body, got delta {delta:?}"
    );
    assert_eq!(
        pos(&designer, &[outer], neighbour),
        neighbour_before + delta
    );

    // One undo restores the body wholesale: wire back, map compact, neighbour
    // back. (Body-scope moves ride the EditZoneBodyCommand after-snapshot.)
    assert!(designer.undo());
    assert_eq!(node_size(&designer, &[outer], inner_map), compact_size);
    assert_eq!(pos(&designer, &[outer], neighbour), neighbour_before);
    assert!(!designer.undo_stack.can_undo());

    // Redo re-applies the expansion + reflow.
    assert!(designer.redo());
    assert_eq!(
        pos(&designer, &[outer], neighbour),
        neighbour_before + delta
    );
}

// ---------------------------------------------------------------------------
// Phase 3 — Case C: in-body growth cascade
// (doc/design_reflow_on_footprint_change.md §"Case C")
// ---------------------------------------------------------------------------

/// Adding a node inside a top-level `map`'s body grows the body, which grows the
/// `map`'s rendered footprint in the parent (top-level) network, pushing the
/// `map`'s lower-right sibling there. The added node itself rides the
/// `EditZoneBodyCommand` after-snapshot, the sibling shift rides a bundled
/// `MoveNodesCommand`, and one undo step restores **both**.
#[test]
fn add_node_in_body_pushes_parent_sibling_single_step_undo() {
    let mut designer = setup_designer_with_network("main");
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));

    // Sibling of the (empty, expanded) map in its lower-right sweep band at the
    // top level — it should shift once the map's footprint grows.
    let map_old = node_size(&designer, &[], map_id);
    let sibling = designer.add_node("union", DVec2::new(map_old.x + 500.0, map_old.y + 500.0));
    let sibling_before = pos(&designer, &[], sibling);
    designer.undo_stack.clear();

    // Grow the map's body by dropping a node far out inside it. The body — and
    // hence the map's parent-network footprint — expands.
    designer.add_node_scoped(&[map_id], "union", DVec2::new(1200.0, 1200.0), None);

    let map_new = node_size(&designer, &[], map_id);
    let delta = grew(map_new, map_old);
    assert!(
        delta.x > 0.0 && delta.y > 0.0,
        "adding a body node should grow the map's footprint, got delta {delta:?}"
    );
    assert_eq!(pos(&designer, &[], sibling), sibling_before + delta);

    // One undo step removes the body node AND restores the sibling position.
    assert!(designer.undo());
    assert_eq!(node_size(&designer, &[], map_id), map_old);
    assert_eq!(pos(&designer, &[], sibling), sibling_before);
    assert!(!designer.undo_stack.can_undo());

    // Redo re-applies the body edit and the reflow together.
    assert!(designer.redo());
    assert_eq!(node_size(&designer, &[], map_id), map_new);
    assert_eq!(pos(&designer, &[], sibling), sibling_before + delta);
}

/// Adding a node inside a nested `map`'s body (scope `[outer, inner]`) cascades:
/// `inner` grows in the `outer` body (pushing `inner`'s in-body sibling) and the
/// `outer` body grows past its stored size, growing `outer` at the top level
/// (pushing `outer`'s grandparent sibling). One undo step restores the inner
/// body edit and every reflowed position across both ancestor scopes.
#[test]
fn add_node_in_nested_body_cascades_to_grandparent_single_step_undo() {
    let mut designer = setup_designer_with_network("main");
    let outer = designer.add_node("map", DVec2::new(0.0, 0.0));
    let inner = designer.add_node_scoped(&[outer], "map", DVec2::new(0.0, 0.0), None);

    // Sibling of `inner` inside the outer body, in `inner`'s lower-right band.
    let inner_old0 = node_size(&designer, &[outer], inner);
    let s_mid = designer.add_node_scoped(
        &[outer],
        "union",
        DVec2::new(inner_old0.x + 500.0, inner_old0.y + 500.0),
        None,
    );
    // Sibling of `outer` at top level, in `outer`'s lower-right band.
    let outer_old0 = node_size(&designer, &[], outer);
    let s_top = designer.add_node(
        "union",
        DVec2::new(outer_old0.x + 500.0, outer_old0.y + 500.0),
    );

    let s_mid_before = pos(&designer, &[outer], s_mid);
    let s_top_before = pos(&designer, &[], s_top);
    let inner_old = node_size(&designer, &[outer], inner);
    let outer_old = node_size(&designer, &[], outer);
    designer.undo_stack.clear();

    // Grow `inner`'s body a lot — cascades up two scope levels.
    designer.add_node_scoped(&[outer, inner], "union", DVec2::new(1500.0, 1500.0), None);

    let delta_mid = grew(node_size(&designer, &[outer], inner), inner_old);
    let delta_top = grew(node_size(&designer, &[], outer), outer_old);
    assert!(delta_mid.x > 0.0 && delta_mid.y > 0.0);
    assert!(delta_top.x > 0.0 && delta_top.y > 0.0);
    assert_eq!(pos(&designer, &[outer], s_mid), s_mid_before + delta_mid);
    assert_eq!(pos(&designer, &[], s_top), s_top_before + delta_top);

    // One undo restores the inner body edit and both ancestor positions.
    assert!(designer.undo());
    assert_eq!(node_size(&designer, &[outer], inner), inner_old);
    assert_eq!(pos(&designer, &[outer], s_mid), s_mid_before);
    assert_eq!(pos(&designer, &[], s_top), s_top_before);
    assert!(!designer.undo_stack.can_undo());

    // Redo re-applies the whole cascade in one step.
    assert!(designer.redo());
    assert_eq!(pos(&designer, &[outer], s_mid), s_mid_before + delta_mid);
    assert_eq!(pos(&designer, &[], s_top), s_top_before + delta_top);
}

/// A body with enough slack to absorb the new node (it lands well within the
/// existing body bounds) does not grow the owning HOF, so reflow moves nothing
/// and a single bare `EditZoneBodyCommand` is pushed (no composite).
#[test]
fn add_node_in_body_with_slack_pushes_nothing() {
    let mut designer = setup_designer_with_network("main");
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));
    // Establish a large body so a later small node fits inside its bounds.
    designer.add_node_scoped(&[map_id], "union", DVec2::new(1500.0, 1500.0), None);

    let big_size = node_size(&designer, &[], map_id);
    let sibling = designer.add_node("union", DVec2::new(big_size.x + 500.0, big_size.y + 500.0));
    let sibling_before = pos(&designer, &[], sibling);
    designer.undo_stack.clear();

    // Add a small node well inside the existing body bounds — the body's
    // bounding box (dominated by the far union) is unchanged, so the map does
    // not grow and nothing is pushed.
    designer.add_node_scoped(&[map_id], "float", DVec2::new(100.0, 100.0), None);

    assert_eq!(node_size(&designer, &[], map_id), big_size);
    assert_eq!(pos(&designer, &[], sibling), sibling_before);

    // Single bare command: one undo (the stack was cleared above) leaves nothing.
    assert!(designer.undo());
    assert!(!designer.undo_stack.can_undo());
    assert_eq!(pos(&designer, &[], sibling), sibling_before);
}

/// Duplicating a node inside a body grows it (the copy is offset *below* the
/// original — `duplicate_node` only shifts vertically), exercising the
/// `duplicate_node_scoped` Case-C call site: the parent sibling is pushed
/// downward and one undo step restores both.
#[test]
fn duplicate_node_in_body_pushes_parent_sibling_single_step_undo() {
    let mut designer = setup_designer_with_network("main");
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));
    // A node at the body's lower edge so its duplicate (offset further down)
    // expands the body's bottom.
    let inner = designer.add_node_scoped(&[map_id], "union", DVec2::new(900.0, 900.0), None);

    let map_old = node_size(&designer, &[], map_id);
    let sibling = designer.add_node("union", DVec2::new(map_old.x + 500.0, map_old.y + 500.0));
    let sibling_before = pos(&designer, &[], sibling);
    designer.undo_stack.clear();

    designer.duplicate_node_scoped(&[map_id], inner);

    let map_new = node_size(&designer, &[], map_id);
    let delta = grew(map_new, map_old);
    // The duplicate is placed directly below the original, so the body grows
    // only on the y axis (x delta is legitimately 0).
    assert!(
        delta.x == 0.0 && delta.y > 0.0,
        "the duplicate should grow the body vertically, got delta {delta:?}"
    );
    assert_eq!(pos(&designer, &[], sibling), sibling_before + delta);

    // One undo removes the duplicate AND restores the sibling.
    assert!(designer.undo());
    assert_eq!(node_size(&designer, &[], map_id), map_old);
    assert_eq!(pos(&designer, &[], sibling), sibling_before);
    assert!(!designer.undo_stack.can_undo());
}
