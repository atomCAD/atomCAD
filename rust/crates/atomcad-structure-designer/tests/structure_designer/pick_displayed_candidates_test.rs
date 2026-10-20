//! `StructureDesigner::pick_displayed_candidates`: the overlap rule shared by
//! click-to-activate (`viewport_pick`) and the output trace
//! (`doc/design_output_driven_editing.md`). These characterize the rule as
//! `viewport_pick` applied it in the API layer before the move.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::visualization::AtomicStructureVisualization;
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::structure_designer::{DisplayedPick, StructureDesigner};
use atomcad_structure_designer::structure_designer_scene::{NodeOutput, NodeSceneData};
use glam::f64::DVec3;

/// A designer whose scene holds one carbon per `(node id, scope path, z)`, all
/// on the z axis, with the ball-and-stick pick radius pinned.
fn designer_with(atoms: Vec<(u64, Vec<u64>, f64)>) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer
        .preferences
        .atomic_structure_visualization_preferences
        .visualization = AtomicStructureVisualization::BallAndStick;
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    for (node_id, scope_path, z) in atoms {
        let mut structure = AtomicStructure::new();
        structure.add_atom(6, DVec3::new(0.0, 0.0, z));
        designer
            .last_generated_structure_designer_scene
            .node_data
            .insert(
                NodeRef {
                    scope_path,
                    node_id,
                },
                NodeSceneData::new(NodeOutput::Atomic(structure, None)),
            );
    }
    designer
}

fn set_active(designer: &mut StructureDesigner, node_id: u64) {
    designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap()
        .active_node_id = Some(node_id);
}

fn pick(designer: &StructureDesigner, privilege_active: bool) -> DisplayedPick {
    designer.pick_displayed_candidates(
        &DVec3::new(0.0, 0.0, -50.0),
        &DVec3::new(0.0, 0.0, 1.0),
        privilege_active,
    )
}

fn ids(pick: &DisplayedPick) -> Vec<u64> {
    match pick {
        DisplayedPick::NoHit => vec![],
        DisplayedPick::Single(hit) => vec![hit.node_ref.node_id],
        DisplayedPick::Overlapping(hits) => hits.iter().map(|h| h.node_ref.node_id).collect(),
    }
}

#[test]
fn no_hit() {
    let designer = designer_with(vec![]);
    assert_eq!(pick(&designer, true), DisplayedPick::NoHit);
}

#[test]
fn one_hit() {
    let designer = designer_with(vec![(1, vec![], 0.0)]);
    let result = pick(&designer, true);
    assert!(matches!(result, DisplayedPick::Single(_)));
    assert_eq!(ids(&result), vec![1]);
}

#[test]
fn two_hits_within_the_overlap_epsilon_are_both_candidates() {
    let designer = designer_with(vec![(1, vec![], 0.0), (2, vec![], 0.05)]);
    let result = pick(&designer, true);
    assert!(matches!(result, DisplayedPick::Overlapping(_)));
    assert_eq!(ids(&result), vec![1, 2]);
}

#[test]
fn two_hits_farther_apart_give_the_nearest() {
    let designer = designer_with(vec![(1, vec![], 0.0), (2, vec![], 10.0)]);
    let result = pick(&designer, true);
    assert!(matches!(result, DisplayedPick::Single(_)));
    assert_eq!(ids(&result), vec![1]);
}

#[test]
fn an_active_node_among_overlapping_hits_is_privileged_only_when_asked() {
    let mut designer = designer_with(vec![(1, vec![], 0.0), (2, vec![], 0.05)]);
    set_active(&mut designer, 2);

    let privileged = pick(&designer, true);
    assert!(matches!(privileged, DisplayedPick::Single(_)));
    assert_eq!(ids(&privileged), vec![2]);

    let unprivileged = pick(&designer, false);
    assert!(matches!(unprivileged, DisplayedPick::Overlapping(_)));
    assert_eq!(ids(&unprivileged), vec![1, 2]);
}

#[test]
fn an_active_node_behind_the_overlap_is_not_privileged() {
    let mut designer = designer_with(vec![(1, vec![], 0.0), (2, vec![], 10.0)]);
    set_active(&mut designer, 2);
    assert_eq!(ids(&pick(&designer, true)), vec![1]);
}

#[test]
fn body_node_hits_are_not_candidates() {
    let designer = designer_with(vec![(1, vec![9], 0.0), (2, vec![], 10.0)]);
    assert_eq!(ids(&pick(&designer, true)), vec![2]);
}
