//! Phase 0 tests for the reflow-on-footprint-change primitives
//! (`doc/design_reflow_on_footprint_change.md`):
//!
//! - `CompositeCommand` undo/redo ordering,
//! - `combine_refresh_modes` folding table,
//! - `StructureDesigner::reflow_for_footprint_change` spatial behaviour
//!   (single scope + the cascade across two body levels), pure — no undo wiring.

use glam::f64::DVec2;
use std::sync::{Arc, Mutex};

use rust_lib_flutter_cad::structure_designer::node_inlining::instance_size;
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

    // A neighbour strictly in the lower-right sweep band (past both the right
    // and bottom edges of the map's *original* footprint) — it should shift on
    // both axes by the growth delta.
    let lower_right =
        designer.add_node("union", DVec2::new(old_size.x + 500.0, old_size.y + 500.0));
    let lr_before = pos(&designer, &[], lower_right);

    // A neighbour completely above-and-left of the map — never reached by the
    // rightward/downward growth, so it must stay put.
    let safe = designer.add_node("union", DVec2::new(-600.0, -600.0));
    let safe_before = pos(&designer, &[], safe);

    // Trigger growth: drop a node far out in the map's body so its rendered
    // body (and thus the map's footprint) expands well past the default.
    designer.add_node_scoped(&[map_id], "union", DVec2::new(1200.0, 1200.0), None);

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

    // A sibling of m2 inside m1's body, in m2's lower-right sweep band.
    let m2_size_pre = node_size(&designer, &[m1], m2);
    let s_mid = designer.add_node_scoped(
        &[m1],
        "union",
        DVec2::new(m2_size_pre.x + 500.0, m2_size_pre.y + 500.0),
        None,
    );

    // A sibling of m1 in the top-level network, in m1's lower-right sweep band.
    let m1_size_pre = node_size(&designer, &[], m1);
    let s_top = designer.add_node(
        "union",
        DVec2::new(m1_size_pre.x + 500.0, m1_size_pre.y + 500.0),
    );

    // Capture the pre-edit footprints (both ancestors) before triggering growth.
    let m2_old = node_size(&designer, &[m1], m2);
    let m1_old = node_size(&designer, &[], m1);
    let s_mid_before = pos(&designer, &[m1], s_mid);
    let s_top_before = pos(&designer, &[], s_top);

    // Trigger: grow m2's body a lot. This grows m2 (in m1's body) and, because
    // m1's rendered body recurses into m2, grows m1 (in the top-level network).
    designer.add_node_scoped(&[m1, m2], "union", DVec2::new(1500.0, 1500.0), None);

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
