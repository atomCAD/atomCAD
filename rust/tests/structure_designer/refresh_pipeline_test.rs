//! Phase 0 of `doc/design_zero_ary_closure_body_display.md` — **refresh-pipeline
//! characterization tests**, written *before* the Phase 1 re-keying refactor
//! (`StructureDesignerScene.node_data`: `HashMap<u64, _>` → `HashMap<NodeRef, _>`).
//!
//! The refresh pipeline (`StructureDesigner::refresh` → `refresh_full` /
//! `refresh_partial`) and the invisible-node LRU cache (`move_to_cache` /
//! `restore_from_cache` / `invalidate_cached_nodes` /
//! `update_cached_displayed_pins`) are among the least-tested paths in the
//! codebase, and the re-keying refactor has two *silent* failure modes that
//! "existing tests stay green" would not catch:
//!
//! 1. **Silent cache miss** — inconsistent key construction makes
//!    `restore_from_cache` quietly fail; the node re-evaluates, the output is
//!    still correct, no test fails, but the fast visibility-toggle path is dead.
//! 2. **Stale restore** — a missed `invalidate_cached_nodes` entry means
//!    hide → edit upstream → show renders **stale** geometry.
//!
//! Both are observable only by asking *"did the node actually re-evaluate?"*.
//! The probe for that is a `print` node (`execute_only: false`, so it fires on
//! every evaluation pass, not just Execute) wired into the chain feeding the
//! displayed node; `take_print_log()` then counts evaluations without any
//! test-only hook in production code.
//!
//! Everything here is driven through `StructureDesigner::refresh` with the
//! designer's own pending changes — the same entry point the API layer uses —
//! *not* through direct `NetworkEvaluator` calls, because the behavior under
//! test lives in the refresh orchestration, not in evaluation.
//!
//! **During Phase 1 these tests must be updated mechanically — key type only.**
//! Any other edit needed to keep them green is a red flag that behavior drifted.

use glam::IVec3;
use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::nodes::cuboid::CuboidData;
use rust_lib_flutter_cad::structure_designer::nodes::string::StringData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::structure_designer_scene::NodeOutput;

// ============================================================================
// Fixture
// ============================================================================

/// Node ids of the probe network built by [`setup_probe_chain`].
///
/// ```text
///   cuboid ──> materialize ──> tag ──> structure_move
///                               ^
///   string ──> print ───────────┘  (tag.name, pin 1)
/// ```
///
/// * `tag` / `structure_move` are the *probed* displayed nodes: both produce
///   `NodeOutput::Atomic`, i.e. real viewport output.
/// * `print` sits upstream of both, so any evaluation that reaches them fires
///   exactly one print entry — the re-evaluation counter.
/// * `cuboid` is the upstream node whose stored data the invalidation tests
///   mutate (changing `extent` changes the materialized atom count, so a stale
///   restore is directly observable).
/// * `structure_move` is the multi-output node (pins `result` / `diff`) used by
///   the pin-display and interactive-pin tests.
struct ProbeChain {
    cuboid: u64,
    materialize: u64,
    string: u64,
    print: u64,
    tag: u64,
    structure_move: u64,
}

impl ProbeChain {
    fn all(&self) -> [u64; 6] {
        [
            self.cuboid,
            self.materialize,
            self.string,
            self.print,
            self.tag,
            self.structure_move,
        ]
    }
}

/// Builds the probe network with **nothing** displayed and no refresh run yet.
///
/// Every node is displayed on creation (`NodeNetwork::add_node` calls
/// `set_node_display(id, true)`), so the helper explicitly hides all of them;
/// each test then opts the node it probes back in. The default display policy
/// is `Manual`, so nothing re-displays them behind our back.
fn setup_probe_chain() -> (StructureDesigner, ProbeChain) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let cuboid = designer.add_node("cuboid", DVec2::new(0.0, 0.0));
    set_cuboid_extent(&mut designer, cuboid, 3);

    let materialize = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    designer.connect_nodes(cuboid, 0, materialize, 0);

    let string = designer.add_node("string", DVec2::new(0.0, 200.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let data = network
            .get_node_network_data_mut(string)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<StringData>()
            .unwrap();
        data.value = "probe".to_string();
    }

    let print = designer.add_node("print", DVec2::new(200.0, 200.0));
    designer.connect_nodes(string, 0, print, 0);

    let tag = designer.add_node("tag", DVec2::new(400.0, 0.0));
    designer.connect_nodes(materialize, 0, tag, 0);
    designer.connect_nodes(print, 0, tag, 1);

    let structure_move = designer.add_node("structure_move", DVec2::new(600.0, 0.0));
    designer.connect_nodes(tag, 0, structure_move, 0);

    designer.validate_active_network();

    let ids = ProbeChain {
        cuboid,
        materialize,
        string,
        print,
        tag,
        structure_move,
    };
    for id in ids.all() {
        designer.set_node_display(id, false);
    }

    (designer, ids)
}

fn set_cuboid_extent(designer: &mut StructureDesigner, cuboid_id: u64, extent: i32) {
    designer.set_node_network_data_scoped(
        &[],
        cuboid_id,
        Box::new(CuboidData {
            min_corner: IVec3::ZERO,
            extent: IVec3::splat(extent),
            subdivision: 1,
        }),
    );
}

/// Runs a **full** refresh and drains the print log, so the caller starts from
/// a known scene + a zeroed evaluation counter.
fn full_refresh_and_reset_counter(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
    designer.take_print_log();
}

/// Runs a refresh with whatever the designer has accumulated, asserting the
/// mode really is `Partial` — every cache assertion in this file is meaningless
/// under a Full refresh (which rebuilds the scene and the cache from scratch).
fn partial_refresh(designer: &mut StructureDesigner) {
    let changes = designer.get_pending_changes();
    assert!(
        changes.is_partial(),
        "these tests characterize the PARTIAL refresh path; \
         something upstream escalated to {:?}",
        changes.mode
    );
    designer.refresh(&changes);
}

/// Number of evaluations since the last drain (one `print` entry per pass that
/// reaches the probe chain).
fn evaluations_since_last_check(designer: &mut StructureDesigner) -> usize {
    designer.take_print_log().len()
}

fn scene_has(designer: &StructureDesigner, node_id: u64) -> bool {
    designer
        .last_generated_structure_designer_scene
        .node_data
        .contains_key(&node_id)
}

/// Atom count of a displayed node's pin-0 output. Panics unless the entry
/// exists and carries atoms — both are part of what these tests assert.
fn scene_atom_count(designer: &StructureDesigner, node_id: u64) -> usize {
    let entry = designer
        .last_generated_structure_designer_scene
        .node_data
        .get(&node_id)
        .expect("node should have a scene entry");
    match &entry.output {
        NodeOutput::Atomic(structure, _) => structure.get_num_of_atoms(),
        other => panic!(
            "expected an Atomic scene output, got {}",
            match other {
                NodeOutput::None => "None",
                NodeOutput::PolyMesh(_) => "PolyMesh",
                NodeOutput::SurfacePointCloud(_) => "SurfacePointCloud",
                NodeOutput::SurfacePointCloud2D(_) => "SurfacePointCloud2D",
                NodeOutput::DrawingPlane(_) => "DrawingPlane",
                NodeOutput::Atomic(..) => unreachable!(),
            }
        ),
    }
}

/// `from_selected_node` on a displayed node's pin-0 atomic output — the flag
/// Step 4.5 of `refresh_partial` exists to keep in sync with the selection.
fn scene_from_selected_node(designer: &StructureDesigner, node_id: u64) -> bool {
    let entry = designer
        .last_generated_structure_designer_scene
        .node_data
        .get(&node_id)
        .expect("node should have a scene entry");
    match &entry.output {
        NodeOutput::Atomic(structure, _) => structure.decorator().from_selected_node,
        _ => panic!("expected an Atomic scene output"),
    }
}

fn scene_displayed_pins(designer: &StructureDesigner, node_id: u64) -> Vec<i32> {
    let mut pins: Vec<i32> = designer
        .last_generated_structure_designer_scene
        .node_data
        .get(&node_id)
        .expect("node should have a scene entry")
        .displayed_pins
        .iter()
        .copied()
        .collect();
    pins.sort();
    pins
}

fn cached_count(designer: &StructureDesigner) -> usize {
    designer
        .last_generated_structure_designer_scene
        .cached_node_count()
}

// ============================================================================
// The fixture itself
// ============================================================================

#[test]
fn probe_chain_evaluates_once_per_displayed_node_pass() {
    // Guards the counter technique the rest of the file depends on: exactly one
    // print entry per evaluation of the displayed node, and the chain really
    // does produce atoms.
    let (mut designer, ids) = setup_probe_chain();
    designer.set_node_display(ids.tag, true);

    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);

    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "one displayed node downstream of `print` should fire exactly one entry"
    );
    assert!(
        scene_atom_count(&designer, ids.tag) > 0,
        "the probe chain must materialize atoms, else the output assertions are vacuous"
    );
}

// ============================================================================
// Hide → cache → show → restore (silent-cache-miss guard)
// ============================================================================

#[test]
fn hide_caches_the_scene_entry_and_show_restores_it_without_reevaluation() {
    let (mut designer, ids) = setup_probe_chain();
    designer.set_node_display(ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);

    let baseline_atoms = scene_atom_count(&designer, ids.tag);
    assert_eq!(
        cached_count(&designer),
        0,
        "full refresh starts a fresh cache"
    );

    // --- hide: the entry leaves `node_data` and lands in the invisible cache
    designer.set_node_display(ids.tag, false);
    partial_refresh(&mut designer);

    assert!(
        !scene_has(&designer, ids.tag),
        "a hidden node must not keep a live scene entry"
    );
    assert_eq!(
        cached_count(&designer),
        1,
        "the hidden node's scene data moves to the invisible cache"
    );
    assert_eq!(
        evaluations_since_last_check(&mut designer),
        0,
        "hiding a node evaluates nothing"
    );

    // --- show: restored from cache, NOT re-evaluated
    designer.set_node_display(ids.tag, true);
    partial_refresh(&mut designer);

    assert!(scene_has(&designer, ids.tag), "showing restores the entry");
    assert_eq!(
        cached_count(&designer),
        0,
        "restoring pops the entry out of the cache"
    );
    assert_eq!(
        evaluations_since_last_check(&mut designer),
        0,
        "THE fast path: a cached entry is restored without re-evaluating the node"
    );
    assert_eq!(
        scene_atom_count(&designer, ids.tag),
        baseline_atoms,
        "the restored output must be identical to the one that was cached"
    );
}

// ============================================================================
// Invalidation (stale-restore guard)
// ============================================================================

#[test]
fn upstream_data_change_invalidates_the_cache_so_show_re_evaluates() {
    let (mut designer, ids) = setup_probe_chain();
    designer.set_node_display(ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);

    let baseline_atoms = scene_atom_count(&designer, ids.tag);

    designer.set_node_display(ids.tag, false);
    partial_refresh(&mut designer);
    assert_eq!(cached_count(&designer), 1);
    assert_eq!(evaluations_since_last_check(&mut designer), 0);

    // Upstream edit while the probed node is hidden, through the normal
    // mutation path (`set_node_network_data_scoped` marks the node dirty).
    set_cuboid_extent(&mut designer, ids.cuboid, 4);
    partial_refresh(&mut designer);

    assert_eq!(
        cached_count(&designer),
        0,
        "the cached entry of a node downstream of the edit must be invalidated"
    );
    assert_eq!(
        evaluations_since_last_check(&mut designer),
        0,
        "nothing is displayed, so the invalidation itself evaluates nothing"
    );

    // Show again: no stale restore is possible, so it must re-evaluate.
    designer.set_node_display(ids.tag, true);
    partial_refresh(&mut designer);

    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "the invalidated node must be re-evaluated, not restored"
    );
    let new_atoms = scene_atom_count(&designer, ids.tag);
    assert_ne!(
        new_atoms, baseline_atoms,
        "the restored output must reflect the new upstream value \
         (baseline {baseline_atoms}, now {new_atoms})"
    );
}

#[test]
fn data_change_and_show_in_one_refresh_also_re_evaluates() {
    // Same scenario, but with the edit and the visibility toggle batched into a
    // single refresh. Note this case is guarded **twice** over: Step 2 evicts
    // the cache entry so Step 3's restore misses, *and* Step 4 independently
    // adds the node to the evaluation set because it is displayed and downstream
    // of the data change. Verified by mutation: disabling
    // `invalidate_cached_nodes` alone does not break this test (Step 4 still
    // re-evaluates) — it breaks the two-refresh case above. Both are kept: this
    // one pins the batched ordering, that one pins the invalidation itself.
    let (mut designer, ids) = setup_probe_chain();
    designer.set_node_display(ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);
    let baseline_atoms = scene_atom_count(&designer, ids.tag);

    designer.set_node_display(ids.tag, false);
    partial_refresh(&mut designer);
    assert_eq!(cached_count(&designer), 1);
    let _ = evaluations_since_last_check(&mut designer);

    set_cuboid_extent(&mut designer, ids.cuboid, 4);
    designer.set_node_display(ids.tag, true);
    partial_refresh(&mut designer);

    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "a displayed node downstream of a batched data change must be \
         re-evaluated, never served from the cache"
    );
    assert_ne!(scene_atom_count(&designer, ids.tag), baseline_atoms);
}

// ============================================================================
// Pin display (`update_cached_displayed_pins`)
// ============================================================================

#[test]
fn pin_display_toggles_flow_through_the_live_entry_and_the_invisible_cache() {
    let (mut designer, ids) = setup_probe_chain();
    let mv = ids.structure_move;
    designer.set_node_display(mv, true);
    full_refresh_and_reset_counter(&mut designer);

    assert_eq!(
        scene_displayed_pins(&designer, mv),
        vec![0],
        "a freshly displayed multi-output node shows pin 0 only"
    );

    // --- toggling a pin on a VISIBLE node: the live entry is updated eagerly,
    //     but the node is also marked visibility-changed, and since it is not
    //     in the invisible cache the refresh re-evaluates it.
    designer.toggle_output_pin_display(mv, 1);
    assert_eq!(
        scene_displayed_pins(&designer, mv),
        vec![0, 1],
        "the live scene entry's displayed_pins is updated without waiting for a refresh"
    );
    partial_refresh(&mut designer);
    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "a visible node has no cache entry to restore from, so it re-evaluates"
    );
    assert_eq!(scene_displayed_pins(&designer, mv), vec![0, 1]);

    // --- hide: the entry (with both pins) goes to the cache
    designer.set_node_display(mv, false);
    partial_refresh(&mut designer);
    assert_eq!(cached_count(&designer), 1);
    assert_eq!(evaluations_since_last_check(&mut designer), 0);

    // --- toggling a pin while HIDDEN re-displays the node with ONLY that pin.
    //     `NodeNetwork::set_pin_displayed` recreates the display state from an
    //     empty pin set, so the previously displayed pin 0 is dropped. This is
    //     today's behavior; the assertion pins it down so the Phase 1 refactor
    //     cannot change it silently.
    designer.toggle_output_pin_display(mv, 1);
    partial_refresh(&mut designer);

    assert!(scene_has(&designer, mv));
    assert_eq!(
        evaluations_since_last_check(&mut designer),
        0,
        "the cached entry is restored — `update_cached_displayed_pins` is what \
         keeps its pin set correct without re-evaluation"
    );
    assert_eq!(
        scene_displayed_pins(&designer, mv),
        vec![1],
        "the restored entry carries the pin set written by update_cached_displayed_pins"
    );
}

// ============================================================================
// Selection changes (Step 4.5)
// ============================================================================

#[test]
fn selection_change_re_evaluates_previous_and_current_and_flips_from_selected_node() {
    let (mut designer, ids) = setup_probe_chain();
    designer.set_node_display(ids.materialize, true);
    designer.set_node_display(ids.tag, true);
    full_refresh_and_reset_counter(&mut designer);

    assert!(!scene_from_selected_node(&designer, ids.tag));
    assert!(!scene_from_selected_node(&designer, ids.materialize));

    // --- select `tag`: only the current selection needs re-evaluation.
    designer.select_node(ids.tag);
    partial_refresh(&mut designer);

    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "`tag` (the new selection, downstream of print) re-evaluates"
    );
    assert!(
        scene_from_selected_node(&designer, ids.tag),
        "the newly selected node's output must be decorated as selected"
    );
    assert!(!scene_from_selected_node(&designer, ids.materialize));

    // --- move the selection: previous AND current must both re-evaluate.
    designer.select_node(ids.materialize);
    partial_refresh(&mut designer);

    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "the previous selection (`tag`) re-evaluates to clear its flag; \
         `materialize` is upstream of `print` so it adds no entry"
    );
    assert!(
        !scene_from_selected_node(&designer, ids.tag),
        "the deselected node's flag must be cleared"
    );
    assert!(
        scene_from_selected_node(&designer, ids.materialize),
        "the newly selected node's flag must be set"
    );
}

// ============================================================================
// Active-node scene lookups
// ============================================================================

#[test]
fn active_node_lookups_survive_a_partial_refresh() {
    let (mut designer, ids) = setup_probe_chain();
    let mv = ids.structure_move;
    designer.set_node_display(mv, true);
    designer.select_node(mv);
    full_refresh_and_reset_counter(&mut designer);

    // `get_selected_node_interactive_pin` and the scene's selected-node unit
    // cell both look the active node up in `node_data` by its id — the two
    // lookups Phase 1 has to re-key.
    assert_eq!(
        designer.get_selected_node_interactive_pin(),
        Some(0),
        "the interactive pin is the lowest displayed output pin"
    );
    let unit_cell_a = designer
        .last_generated_structure_designer_scene
        .unit_cell
        .as_ref()
        .expect("the selected node's Crystal output carries a unit cell")
        .a
        .length();

    // A partial refresh driven by an upstream edit must leave both intact.
    set_cuboid_extent(&mut designer, ids.cuboid, 4);
    partial_refresh(&mut designer);

    assert_eq!(
        evaluations_since_last_check(&mut designer),
        1,
        "the displayed node downstream of the edit re-evaluates"
    );
    assert_eq!(designer.get_selected_node_interactive_pin(), Some(0));
    let unit_cell_a_after = designer
        .last_generated_structure_designer_scene
        .unit_cell
        .as_ref()
        .expect("the selected node's unit cell must survive a partial refresh")
        .a
        .length();
    assert!(
        (unit_cell_a_after - unit_cell_a).abs() < 1e-9,
        "unit cell must be unchanged: {unit_cell_a} -> {unit_cell_a_after}"
    );
}

#[test]
fn interactive_pin_follows_the_lowest_displayed_pin() {
    let (mut designer, ids) = setup_probe_chain();
    let mv = ids.structure_move;
    designer.set_node_display(mv, true);
    designer.select_node(mv);
    full_refresh_and_reset_counter(&mut designer);

    assert_eq!(designer.get_selected_node_interactive_pin(), Some(0));

    // Show pin 1 as well, then hide pin 0: the interactive pin moves to 1.
    designer.toggle_output_pin_display(mv, 1);
    designer.toggle_output_pin_display(mv, 0);
    partial_refresh(&mut designer);

    assert_eq!(
        scene_displayed_pins(&designer, mv),
        vec![1],
        "pin 0 hidden, pin 1 shown"
    );
    assert_eq!(
        designer.get_selected_node_interactive_pin(),
        Some(1),
        "the interactive pin is now the diff pin"
    );
}
