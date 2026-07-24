use glam::f64::DVec3;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::node_network::NodeRef;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::structure_designer_scene::{
    NodeOutput, NodeSceneData,
};

/// Helper: create a designer with some node data injected directly into the scene.
/// This avoids full network evaluation and focuses on testing the raycast logic.
fn setup_designer_with_scene_atoms(
    atoms_per_node: Vec<(u64, Vec<(i16, DVec3)>)>,
) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    for (node_id, atoms) in atoms_per_node {
        let mut structure = AtomicStructure::new();
        for (atomic_number, position) in atoms {
            structure.add_atom(atomic_number, position);
        }
        let scene_data = NodeSceneData::new(NodeOutput::Atomic(structure, None));
        designer
            .last_generated_structure_designer_scene
            .node_data
            .insert(NodeRef::top(node_id), scene_data);
    }

    designer
}

// --- Single node hit ---

#[test]
fn test_single_node_hit() {
    // Place a carbon atom at the origin
    let designer = setup_designer_with_scene_atoms(vec![(1, vec![(6, DVec3::ZERO)])]);

    // Cast a ray along the Z axis toward the origin
    let ray_origin = DVec3::new(0.0, 0.0, -50.0);
    let ray_direction = DVec3::new(0.0, 0.0, 1.0);

    let hits = designer.raytrace_per_node(
        &ray_origin,
        &ray_direction,
        &AtomicStructureVisualization::BallAndStick,
    );

    assert_eq!(hits.len(), 1, "Should hit exactly one node");
    assert_eq!(hits[0].node_ref.node_id, 1);
    assert!(hits[0].distance > 0.0, "Distance should be positive");
}

// --- No hits ---

#[test]
fn test_no_hits() {
    // Place an atom at the origin
    let designer = setup_designer_with_scene_atoms(vec![(1, vec![(6, DVec3::ZERO)])]);

    // Cast a ray that misses (far off to the side)
    let ray_origin = DVec3::new(100.0, 100.0, -50.0);
    let ray_direction = DVec3::new(0.0, 0.0, 1.0);

    let hits = designer.raytrace_per_node(
        &ray_origin,
        &ray_direction,
        &AtomicStructureVisualization::BallAndStick,
    );

    assert!(hits.is_empty(), "Ray should miss all nodes");
}

// --- Multiple nodes at different distances ---

#[test]
fn test_multiple_nodes_different_distances() {
    // Node 1: atom at z=0, Node 2: atom at z=10
    let designer = setup_designer_with_scene_atoms(vec![
        (1, vec![(6, DVec3::new(0.0, 0.0, 0.0))]),
        (2, vec![(6, DVec3::new(0.0, 0.0, 10.0))]),
    ]);

    let ray_origin = DVec3::new(0.0, 0.0, -50.0);
    let ray_direction = DVec3::new(0.0, 0.0, 1.0);

    let hits = designer.raytrace_per_node(
        &ray_origin,
        &ray_direction,
        &AtomicStructureVisualization::BallAndStick,
    );

    assert_eq!(hits.len(), 2, "Should hit both nodes");
    // Results are sorted by distance, so node 1 (z=0) should be first
    assert_eq!(hits[0].node_ref.node_id, 1, "Closer node should be first");
    assert_eq!(hits[1].node_ref.node_id, 2, "Farther node should be second");
    assert!(
        hits[0].distance < hits[1].distance,
        "Hits should be sorted by ascending distance"
    );
}

// --- Overlapping nodes within epsilon ---

#[test]
fn test_overlapping_nodes_within_epsilon() {
    // Two nodes with atoms at nearly the same position (within 0.1 Å)
    let designer = setup_designer_with_scene_atoms(vec![
        (1, vec![(6, DVec3::new(0.0, 0.0, 0.0))]),
        (2, vec![(6, DVec3::new(0.0, 0.0, 0.05))]),
    ]);

    let ray_origin = DVec3::new(0.0, 0.0, -50.0);
    let ray_direction = DVec3::new(0.0, 0.0, 1.0);

    let hits = designer.raytrace_per_node(
        &ray_origin,
        &ray_direction,
        &AtomicStructureVisualization::BallAndStick,
    );

    assert_eq!(hits.len(), 2, "Should hit both overlapping nodes");
    // The difference in distances should be very small (within overlap epsilon)
    let distance_diff = (hits[0].distance - hits[1].distance).abs();
    assert!(
        distance_diff < 0.1,
        "Overlapping nodes should have distances within 0.1 Å, got diff={}",
        distance_diff
    );
}

// --- Empty scene ---

#[test]
fn test_empty_scene() {
    let designer = setup_designer_with_scene_atoms(vec![]);

    let ray_origin = DVec3::new(0.0, 0.0, -50.0);
    let ray_direction = DVec3::new(0.0, 0.0, 1.0);

    let hits = designer.raytrace_per_node(
        &ray_origin,
        &ray_direction,
        &AtomicStructureVisualization::BallAndStick,
    );

    assert!(hits.is_empty(), "Empty scene should return no hits");
}

// --- get_node_display_name ---

#[test]
fn test_get_node_display_name_with_custom_name() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    // add_node auto-generates a custom_name like "float1"
    let node_id = designer.add_node("float", glam::f64::DVec2::new(0.0, 0.0));

    let name = designer.get_node_display_name(node_id);
    assert_eq!(name, "float1");
}

#[test]
fn test_get_node_display_name_unknown_node() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    // Node ID 999 doesn't exist
    let name = designer.get_node_display_name(999);
    assert_eq!(name, "node #999");
}

// --- Per-atom render-style overrides (apply_style) drive the pick radius ---

/// Build a designer whose scene holds one carbon at the origin, optionally
/// carrying a render-style override (as `apply_style` would set).
fn setup_designer_with_styled_carbon(
    style: Option<rust_lib_flutter_cad::crystolecule::atomic_structure::AtomRenderStyle>,
) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    let mut structure = AtomicStructure::new();
    let atom_id = structure.add_atom(6, DVec3::ZERO);
    if let Some(style) = style {
        structure.set_atom_render_style(atom_id, style);
    }
    let scene_data = NodeSceneData::new(NodeOutput::Atomic(structure, None));
    designer
        .last_generated_structure_designer_scene
        .node_data
        .insert(NodeRef::top(1), scene_data);

    designer
}

/// A ray offset that separates carbon's two pick radii: past the ball-and-stick
/// radius (min(1.7 * 0.25, 0.77 * 0.9) = 0.425 Å) but inside the space-filling
/// vdW radius (1.7 Å).
const BETWEEN_BAS_AND_VDW: f64 = 1.0;

#[test]
fn test_space_filling_styled_atom_picks_at_vdw_radius_in_bas_scene() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomRenderStyle;

    let designer = setup_designer_with_styled_carbon(Some(AtomRenderStyle::SpaceFilling));
    let ray_origin = DVec3::new(BETWEEN_BAS_AND_VDW, 0.0, -50.0);
    let ray_direction = DVec3::new(0.0, 0.0, 1.0);

    let hits = designer.raytrace_per_node(
        &ray_origin,
        &ray_direction,
        &AtomicStructureVisualization::BallAndStick,
    );

    assert_eq!(
        hits.len(),
        1,
        "space-filling-styled atom must pick at its rendered vdW radius"
    );
}

#[test]
fn test_bas_styled_atom_does_not_pick_at_vdw_radius_in_space_filling_scene() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomRenderStyle;

    let designer = setup_designer_with_styled_carbon(Some(AtomRenderStyle::BallAndStick));
    let ray_origin = DVec3::new(BETWEEN_BAS_AND_VDW, 0.0, -50.0);
    let ray_direction = DVec3::new(0.0, 0.0, 1.0);

    let hits = designer.raytrace_per_node(
        &ray_origin,
        &ray_direction,
        &AtomicStructureVisualization::SpaceFilling,
    );

    assert!(
        hits.is_empty(),
        "B&S-styled atom must not steal picks out to the vdW radius"
    );
}

#[test]
fn test_unstyled_atom_pick_radius_follows_global_mode() {
    let designer = setup_designer_with_styled_carbon(None);
    let ray_origin = DVec3::new(BETWEEN_BAS_AND_VDW, 0.0, -50.0);
    let ray_direction = DVec3::new(0.0, 0.0, 1.0);

    let hits = designer.raytrace_per_node(
        &ray_origin,
        &ray_direction,
        &AtomicStructureVisualization::SpaceFilling,
    );
    assert_eq!(hits.len(), 1, "no-override space-filling pick unchanged");

    let hits = designer.raytrace_per_node(
        &ray_origin,
        &ray_direction,
        &AtomicStructureVisualization::BallAndStick,
    );
    assert!(hits.is_empty(), "no-override ball-and-stick pick unchanged");
}
