//! Regression tests for the `structure_move` drag gizmo (issue #411).
//!
//! The gizmo used to read the **stored** `lattice_subdivision` while `eval` read
//! the **resolved** one, so wiring `2` into the `subdivision` pin made the gizmo
//! travel twice as far as the object it moves. The resolved value now rides on
//! `StructureMoveEvalCache`, which is what `provide_gadget` reads.

use glam::f64::{DVec2, DVec3};
use glam::i32::IVec3;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::cuboid::CuboidData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::structure_move::{
    StructureMoveData, StructureMoveEvalCache,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn set_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

/// `materialize(cuboid) → structure_move`, with `structure_move` selected,
/// displayed and evaluated so its eval cache and gizmo exist.
///
/// When `wired_subdivision` is `Some(n)`, an `int(n)` node is wired into the
/// `subdivision` pin (index 2) while the stored field stays at
/// `stored_subdivision` — the divergence issue #411 is about.
fn setup_move_with_gizmo(
    stored_subdivision: i32,
    wired_subdivision: Option<i32>,
) -> (StructureDesigner, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        cuboid_id,
        Box::new(CuboidData {
            min_corner: IVec3::ZERO,
            extent: IVec3::splat(4),
            subdivision: 1,
        }),
    );

    let mat_id = designer.add_node("materialize", DVec2::new(150.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);

    let mv_id = designer.add_node("structure_move", DVec2::new(300.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        mv_id,
        Box::new(StructureMoveData {
            translation: IVec3::ZERO,
            lattice_subdivision: stored_subdivision,
        }),
    );
    designer.connect_nodes(mat_id, 0, mv_id, 0);

    if let Some(n) = wired_subdivision {
        let int_id = designer.add_node("int", DVec2::new(150.0, 150.0));
        set_node_data(
            &mut designer,
            "main",
            int_id,
            Box::new(IntData { value: n }),
        );
        designer.connect_nodes(int_id, 0, mv_id, 2);
    }

    designer.validate_active_network();
    designer.select_node(mv_id);
    designer.set_node_display(mv_id, true);
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);

    (designer, mv_id)
}

fn cached_unit_cell_a_length(designer: &StructureDesigner) -> f64 {
    designer
        .get_selected_node_eval_cache()
        .expect("the selected structure_move must populate its eval cache")
        .downcast_ref::<StructureMoveEvalCache>()
        .expect("the cache must be the structure_move gadget's cache type")
        .unit_cell
        .a
        .length()
}

fn cached_subdivision(designer: &StructureDesigner) -> i32 {
    designer
        .get_selected_node_eval_cache()
        .expect("the selected structure_move must populate its eval cache")
        .downcast_ref::<StructureMoveEvalCache>()
        .expect("the cache must be the structure_move gadget's cache type")
        .lattice_subdivision
}

fn stored_translation(designer: &StructureDesigner, node_id: u64) -> IVec3 {
    designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<StructureMoveData>()
        .unwrap()
        .translation
}

/// A mouse ray that crosses the world x axis at `x` (pointing straight down),
/// so `get_dragged_axis_offset` along the +x handle reports exactly `x`.
fn ray_crossing_x_axis_at(x: f64) -> (DVec3, DVec3) {
    (DVec3::new(x, 100.0, 0.0), DVec3::new(0.0, -1.0, 0.0))
}

/// Grab the +x handle at the origin and drag it `distance` Å along +x.
fn drag_x_handle(designer: &mut StructureDesigner, distance: f64) {
    let (start_origin, dir) = ray_crossing_x_axis_at(0.0);
    designer.gadget_start_drag(0, start_origin, dir);
    let (end_origin, dir) = ray_crossing_x_axis_at(distance);
    designer.gadget_drag(0, end_origin, dir);
    designer.gadget_end_drag();
}

// ============================================================================
// Tests
// ============================================================================

/// The core of issue #411: a wired `subdivision` must reach the gizmo. Before
/// the fix the cache carried only the unit cell, so `provide_gadget` fell back
/// to the stored `1`.
#[test]
fn wired_subdivision_reaches_the_eval_cache() {
    let (designer, _) = setup_move_with_gizmo(1, Some(2));
    assert_eq!(
        cached_subdivision(&designer),
        2,
        "the cache must carry the resolved subdivision, not the stored field"
    );
}

/// With no wire the resolved value is the stored field — the path that already
/// worked, pinned so the fix can't invert the precedence.
#[test]
fn unwired_subdivision_falls_back_to_the_stored_field() {
    let (designer, _) = setup_move_with_gizmo(3, None);
    assert_eq!(cached_subdivision(&designer), 3);
}

/// End-to-end: dragging the +x handle by exactly one cell must move the object
/// by exactly one cell, whatever the subdivision. With `subdivision = 2` that
/// means a stored translation of `2` (2 half-cells). Before the fix the gizmo
/// counted in whole cells and produced `1`, so the object trailed at half the
/// gizmo's travel — the reported symptom.
#[test]
fn dragging_one_cell_with_wired_subdivision_moves_one_cell() {
    let (mut designer, mv_id) = setup_move_with_gizmo(1, Some(2));
    let cell = cached_unit_cell_a_length(&designer);

    drag_x_handle(&mut designer, cell);

    assert_eq!(
        stored_translation(&designer, mv_id),
        IVec3::new(2, 0, 0),
        "one cell of gizmo travel is 2 subdivision steps when subdivision = 2"
    );
}

/// The same drag with a *stored* subdivision (no wire) — the path that was
/// already correct, kept as the control for the test above.
#[test]
fn dragging_one_cell_with_stored_subdivision_moves_one_cell() {
    let (mut designer, mv_id) = setup_move_with_gizmo(2, None);
    let cell = cached_unit_cell_a_length(&designer);

    drag_x_handle(&mut designer, cell);

    assert_eq!(stored_translation(&designer, mv_id), IVec3::new(2, 0, 0));
}

/// Sub-cell travel is what subdivision exists for: a cell-and-a-half drag lands
/// on the half-cell step instead of being rounded to a whole cell.
#[test]
fn sub_cell_drag_lands_on_a_subdivision_step() {
    let (mut designer, mv_id) = setup_move_with_gizmo(1, Some(2));
    let cell = cached_unit_cell_a_length(&designer);

    drag_x_handle(&mut designer, cell * 1.5);

    assert_eq!(
        stored_translation(&designer, mv_id),
        IVec3::new(3, 0, 0),
        "1.5 cells is 3 half-cell steps; a subdivision-blind gizmo would round to 2"
    );
}
