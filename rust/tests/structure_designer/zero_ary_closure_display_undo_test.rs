//! Phase 3 of `doc/design_zero_ary_closure_body_display.md` (issue #409) —
//! **per-pin toggle undo + API surface + persistence**.
//!
//! Phase 2 brought the *node-level* display toggle inside a 0-ary closure body
//! to parity with the top level (scope-extended `SetNodeDisplayCommand`, change
//! tracking, eligibility-gated collection); its tests live in
//! `zero_ary_closure_display_test.rs`. This file covers what Phase 3 adds:
//!
//! * `toggle_output_pin_display_scoped` + the scope-extended
//!   `SetOutputPinDisplayCommand` — a per-pin toggle on a **body** node is
//!   undoable and redoable, and resolves through the body network (never the
//!   top-level one, which routinely holds a node with the same numeric id).
//! * The eager live/cached scene update keys by the node's own `NodeRef`.
//! * Persistence: body `displayed_nodes` / `displayed_output_pins` survive a
//!   `.cnnd` round-trip, **dormant** (under a parameterized closure) as well as
//!   active.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_network::NodeRef;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use tempfile::tempdir;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// `ClosureData` for an n-ary `Custom` closure returning `ret`. `n == 0` is the
/// thunk shape this feature is about; `n >= 1` makes the body ineligible.
fn custom_closure_data(param_types: Vec<DataType>, ret: DataType) -> ClosureData {
    let param_names: Vec<String> = (0..param_types.len()).map(|i| format!("p{i}")).collect();
    let mut type_args = param_types;
    type_args.push(ret);
    ClosureData {
        kind: ClosureKind::Custom,
        type_args,
        param_names,
        custom_label: None,
    }
}

fn add_custom_closure(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    position: DVec2,
    param_types: Vec<DataType>,
    ret: DataType,
) -> u64 {
    let id = designer.add_node_scoped(scope_path, "closure", position, None);
    designer.set_node_network_data_scoped(
        scope_path,
        id,
        Box::new(custom_closure_data(param_types, ret)),
    );
    id
}

/// A 0-ary closure whose body ends in a genuinely **multi-output** node:
///
/// ```text
///   closure (0-ary, -> Crystal)
///     body:  sphere ─> materialize ─> structure_move ──> (zone output)
///                                     ^ pins: 0 "result", 1 "diff"
/// ```
///
/// Returns `(designer, closure_id, structure_move_id)`. Only `structure_move`
/// is left displayed (with the default pin set `{0}`); everything else is
/// hidden so the scene contains exactly the probed entry.
fn setup_multi_output_body(network: &str) -> (StructureDesigner, u64, u64) {
    let mut designer = setup_designer_with_network(network);

    let closure = add_custom_closure(
        &mut designer,
        &[],
        DVec2::new(200.0, 0.0),
        vec![],
        DataType::Crystal,
    );
    let body = [closure];

    let sphere = designer.add_node_scoped(&body, "sphere", DVec2::new(0.0, 0.0), None);
    let materialize = designer.add_node_scoped(&body, "materialize", DVec2::new(200.0, 0.0), None);
    designer.connect_nodes_scoped(&body, sphere, 0, materialize, 0);
    let mv = designer.add_node_scoped(&body, "structure_move", DVec2::new(400.0, 0.0), None);
    designer.connect_nodes_scoped(&body, materialize, 0, mv, 0);
    designer.connect_zone_output_wire(&body, mv, 0, 0);

    designer.validate_active_network();
    assert!(
        designer
            .get_active_node_network()
            .expect("active network")
            .valid,
        "the fixture must be a valid network, else nothing evaluates"
    );

    designer.set_node_display(closure, false);
    for id in [sphere, materialize] {
        designer.set_node_display_scoped(&body, id, false);
    }

    (designer, closure, mv)
}

fn refresh(designer: &mut StructureDesigner) {
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

fn full_refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    refresh(designer);
}

/// The pin set stored in the body network (sorted, for stable comparison).
fn stored_pins(designer: &StructureDesigner, scope_path: &[u64], node_id: u64) -> Vec<i32> {
    let mut pins: Vec<i32> = designer
        .get_scope_network(scope_path)
        .expect("scope network")
        .get_displayed_pins(node_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    pins.sort();
    pins
}

/// The pin set carried by the *scene* entry keyed by the node's scoped ref.
fn scene_pins(designer: &StructureDesigner, node_ref: &NodeRef) -> Vec<i32> {
    let mut pins: Vec<i32> = designer
        .last_generated_structure_designer_scene
        .node_data
        .get(node_ref)
        .expect("node should have a scene entry")
        .displayed_pins
        .iter()
        .copied()
        .collect();
    pins.sort();
    pins
}

// ============================================================================
// Per-pin toggle undo / redo
// ============================================================================

#[test]
fn body_pin_display_toggle_round_trips_through_undo_and_redo() {
    let (mut designer, closure, mv) = setup_multi_output_body("main");
    let body = [closure];
    let mv_ref = NodeRef::scoped(&body, mv);

    full_refresh(&mut designer);
    assert_eq!(
        scene_pins(&designer, &mv_ref),
        vec![0],
        "a freshly displayed multi-output body node shows pin 0 only"
    );
    designer.undo_stack.clear();

    // Show pin 1 as well.
    designer.toggle_output_pin_display_scoped(&body, mv, 1);
    assert_eq!(
        scene_pins(&designer, &mv_ref),
        vec![0, 1],
        "the live scoped scene entry is updated eagerly, before any refresh"
    );
    refresh(&mut designer);
    assert_eq!(stored_pins(&designer, &body, mv), vec![0, 1]);
    assert_eq!(scene_pins(&designer, &mv_ref), vec![0, 1]);

    assert!(designer.undo());
    refresh(&mut designer);
    assert_eq!(
        stored_pins(&designer, &body, mv),
        vec![0],
        "undo must restore the BODY network's pin set"
    );
    assert_eq!(scene_pins(&designer, &mv_ref), vec![0]);

    assert!(designer.redo());
    refresh(&mut designer);
    assert_eq!(stored_pins(&designer, &body, mv), vec![0, 1]);
    assert_eq!(scene_pins(&designer, &mv_ref), vec![0, 1]);
}

#[test]
fn body_pin_toggle_that_empties_the_pin_set_undoes_back_to_displayed() {
    // Removing the last pin drops the node out of `displayed_nodes` entirely
    // (`set_pin_displayed`), so the command has to restore the whole
    // `NodeDisplayState`, not just the pin set — the atomicity that motivated
    // storing `Option<NodeDisplayState>` in the first place.
    let (mut designer, closure, mv) = setup_multi_output_body("main");
    let body = [closure];
    let mv_ref = NodeRef::scoped(&body, mv);

    full_refresh(&mut designer);
    designer.undo_stack.clear();

    designer.toggle_output_pin_display_scoped(&body, mv, 0);
    refresh(&mut designer);
    assert!(
        !designer
            .get_scope_network(&body)
            .unwrap()
            .is_node_displayed(mv),
        "removing the last pin hides the node"
    );
    assert!(
        !designer
            .last_generated_structure_designer_scene
            .node_data
            .contains_key(&mv_ref),
        "a hidden body node must have no scene entry"
    );

    assert!(designer.undo());
    refresh(&mut designer);
    assert_eq!(stored_pins(&designer, &body, mv), vec![0]);
    assert_eq!(scene_pins(&designer, &mv_ref), vec![0]);
}

#[test]
fn body_pin_toggle_undo_does_not_disturb_a_colliding_top_level_node() {
    let (mut designer, closure, mv) = setup_multi_output_body("main");
    let body = [closure];

    // Body ids come from the body's own `next_node_id` counter, so a top-level
    // node sharing `mv`'s numeric id is the normal case, not a contrivance.
    // Grow the top-level network until that twin exists.
    while !designer
        .get_active_node_network()
        .unwrap()
        .nodes
        .contains_key(&mv)
    {
        designer.add_node("sphere", DVec2::new(0.0, 400.0));
    }

    // Distinguish the twin's pin set from the body node's so a mix-up shows.
    designer.toggle_output_pin_display(mv, 2);
    assert_eq!(stored_pins(&designer, &[], mv), vec![0, 2]);
    designer.undo_stack.clear();

    // Toggle a pin on the *body* node with the same id, then undo.
    designer.toggle_output_pin_display_scoped(&body, mv, 1);
    assert_eq!(stored_pins(&designer, &body, mv), vec![0, 1]);
    assert!(designer.undo());

    assert_eq!(
        stored_pins(&designer, &[], mv),
        vec![0, 2],
        "undoing a body-scoped pin toggle must not touch the top-level node with the same id"
    );
    assert_eq!(stored_pins(&designer, &body, mv), vec![0]);
}

#[test]
fn pin_toggle_on_a_missing_body_node_pushes_no_command() {
    // The `old_display_state != new_display_state` guard is the top-level
    // no-op rule; the scoped path must honour it too (and must not panic on a
    // ref that resolves to no node).
    let (mut designer, closure, _mv) = setup_multi_output_body("main");
    let body = [closure];
    designer.undo_stack.clear();

    designer.toggle_output_pin_display_scoped(&body, u64::MAX, 0);

    assert!(
        !designer.undo_stack.can_undo(),
        "a toggle that changed nothing must not push an undo command"
    );
}

#[test]
fn pin_toggle_in_an_ineligible_scope_stores_a_dormant_pin_set() {
    let (mut designer, closure, mv) = setup_multi_output_body("main");
    let body = [closure];

    // Give the closure a parameter: the body's display flags go dormant.
    designer.set_node_network_data_scoped(
        &[],
        closure,
        Box::new(custom_closure_data(vec![DataType::Int], DataType::Crystal)),
    );

    designer.toggle_output_pin_display_scoped(&body, mv, 1);
    assert_eq!(
        stored_pins(&designer, &body, mv),
        vec![0, 1],
        "the pin set is stored (dormant) — the API does not reject ineligible scopes"
    );

    full_refresh(&mut designer);
    assert!(
        !designer
            .last_generated_structure_designer_scene
            .node_data
            .contains_key(&NodeRef::scoped(&body, mv)),
        "a dormant pin set must not produce a scene entry"
    );
}

// ============================================================================
// Persistence (.cnnd round-trip)
// ============================================================================

#[test]
fn body_display_flags_survive_a_cnnd_round_trip_active_and_dormant() {
    let mut designer = setup_designer_with_network("main");

    // An ELIGIBLE 0-ary closure whose body node shows pins {0, 1}.
    let eligible = add_custom_closure(
        &mut designer,
        &[],
        DVec2::new(0.0, 0.0),
        vec![],
        DataType::Crystal,
    );
    let eligible_body = [eligible];
    let sphere = designer.add_node_scoped(&eligible_body, "sphere", DVec2::ZERO, None);
    let materialize =
        designer.add_node_scoped(&eligible_body, "materialize", DVec2::new(200.0, 0.0), None);
    designer.connect_nodes_scoped(&eligible_body, sphere, 0, materialize, 0);
    let shown = designer.add_node_scoped(
        &eligible_body,
        "structure_move",
        DVec2::new(400.0, 0.0),
        None,
    );
    designer.connect_nodes_scoped(&eligible_body, materialize, 0, shown, 0);
    designer.connect_zone_output_wire(&eligible_body, shown, 0, 0);
    designer.set_node_display_scoped(&eligible_body, sphere, false);
    designer.set_node_display_scoped(&eligible_body, materialize, false);
    designer.toggle_output_pin_display_scoped(&eligible_body, shown, 1);

    // An INELIGIBLE (1-ary) closure whose body node keeps a *dormant* flag.
    let dormant_closure = add_custom_closure(
        &mut designer,
        &[],
        DVec2::new(0.0, 400.0),
        vec![DataType::Int],
        DataType::Int,
    );
    let dormant_body = [dormant_closure];
    let dormant_node = designer.add_node_scoped(&dormant_body, "int", DVec2::ZERO, None);
    designer.connect_zone_output_wire(&dormant_body, dormant_node, 0, 0);
    designer.toggle_output_pin_display_scoped(&dormant_body, dormant_node, 2);

    designer.validate_active_network();

    assert_eq!(stored_pins(&designer, &eligible_body, shown), vec![0, 1]);
    assert_eq!(
        stored_pins(&designer, &dormant_body, dormant_node),
        vec![0, 2]
    );

    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("body_display.cnnd");
    let path_str = path.to_str().unwrap().to_string();
    designer
        .save_node_networks_as(&path_str)
        .expect("save should succeed");

    let mut reloaded = StructureDesigner::new();
    reloaded
        .load_node_networks(&path_str)
        .expect("load should succeed");
    assert_eq!(
        reloaded.active_node_network_name.as_deref(),
        Some("main"),
        "the round-tripped file should reopen on the same network"
    );

    assert_eq!(
        stored_pins(&reloaded, &eligible_body, shown),
        vec![0, 1],
        "an active body pin set must survive the round-trip"
    );
    assert_eq!(
        stored_pins(&reloaded, &dormant_body, dormant_node),
        vec![0, 2],
        "a DORMANT body pin set must survive the round-trip too"
    );
    assert!(
        !reloaded
            .get_scope_network(&eligible_body)
            .unwrap()
            .is_node_displayed(sphere),
        "a body node hidden before the save must stay hidden after the load"
    );

    // The reloaded eligible body still renders — the flags are live, not inert.
    full_refresh(&mut reloaded);
    assert_eq!(
        scene_pins(&reloaded, &NodeRef::scoped(&eligible_body, shown)),
        vec![0, 1]
    );
    assert!(
        !reloaded
            .last_generated_structure_designer_scene
            .node_data
            .contains_key(&NodeRef::scoped(&dormant_body, dormant_node)),
        "the dormant body must still contribute nothing to the scene"
    );
}

/// Guard for the fixture's central assumption: `structure_move` really does
/// have two output pins, so the pin-set assertions above are meaningful.
#[test]
fn structure_move_is_a_multi_output_node() {
    let (designer, closure, mv) = setup_multi_output_body("main");
    let node = designer
        .get_scope_network(&[closure])
        .unwrap()
        .nodes
        .get(&mv)
        .unwrap();
    let node_type = designer
        .node_type_registry
        .get_node_type_for_node(node)
        .expect("structure_move node type");
    assert!(
        node_type.output_pins.len() >= 2,
        "the pin-toggle fixture needs a genuinely multi-output node"
    );
}
