//! Rendering-decoration tests for the placement guideline tool (issue #368).
//!
//! `eval(decorate=true)` populates the output's `decorator.guideline_visuals`
//! from the active Guideline tool's `Active` line (applied to both the result and
//! diff pins); `Define`, `decorate=false`, and no Guideline tool all leave it
//! `None`. Tessellation/GPU is exempt per the testing policy. See
//! `doc/atom_edit/design_atom_guidelines.md`.

use glam::f64::{DVec2, DVec3};

use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, AtomEditTool, AtomRef, Guideline, GuidelineDragState, GuidelinePhase,
    GuidelineTool,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

const EPS: f64 = 1e-9;

// =============================================================================
// Helpers
// =============================================================================

fn setup_atom_edit() -> (StructureDesigner, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    let node_id = designer.add_node("atom_edit", DVec2::ZERO);
    (designer, node_id)
}

fn data_mut(designer: &mut StructureDesigner, node_id: u64) -> &mut AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let data = network.get_node_network_data_mut(node_id).unwrap();
    data.as_any_mut().downcast_mut::<AtomEditData>().unwrap()
}

/// Evaluate one output pin of `node_id` and return its atoms' guideline visuals
/// (a clone, to release the borrow on `designer`).
fn eval_guideline_visuals(
    designer: &StructureDesigner,
    node_id: u64,
    output_pin_index: i32,
    decorate: bool,
) -> Option<
    rust_lib_flutter_cad::crystolecule::atomic_structure::atomic_structure_decorator::GuidelineVisuals,
>{
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let result = evaluator.evaluate(
        &network_stack,
        node_id,
        output_pin_index,
        registry,
        decorate,
        &mut context,
    );
    let atoms = match result {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("expected atomic result, got {:?}", other.infer_data_type()),
    };
    atoms.decorator().guideline_visuals.clone()
}

/// A guideline along +z through (1,2,3) with the marker at t = 4.
fn sample_guideline() -> Guideline {
    let mut g = Guideline::new(DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.0, 0.0, 1.0));
    g.t = 4.0;
    g
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn no_guideline_leaves_visuals_none() {
    let (designer, node_id) = setup_atom_edit();
    // No guideline set on the node.
    assert!(eval_guideline_visuals(&designer, node_id, 0, true).is_none());
    assert!(eval_guideline_visuals(&designer, node_id, 1, true).is_none());
}

// =============================================================================
// Phase 2 — rendering from the new tool state (AtomEditTool::Guideline)
// =============================================================================

/// Put the data into the Guideline tool's `Active` phase with the given line.
fn enter_active_tool(data: &mut AtomEditData, g: Guideline) {
    data.active_tool = AtomEditTool::Guideline(GuidelineTool {
        phase: GuidelinePhase::Active {
            guideline: g,
            picked: None,
            drag: GuidelineDragState::Idle,
        },
        entered_direction: DVec3::ZERO,
        remembered_t: g.t,
        pending: None,
    });
}

#[test]
fn tool_active_populates_both_pin_visuals() {
    let (mut designer, node_id) = setup_atom_edit();
    enter_active_tool(data_mut(&mut designer, node_id), sample_guideline());

    for pin in [0, 1] {
        let visuals = eval_guideline_visuals(&designer, node_id, pin, true)
            .expect("Active tool should carry guideline visuals on both pins");
        assert!((visuals.origin - DVec3::new(1.0, 2.0, 3.0)).length() < EPS);
        assert!((visuals.direction - DVec3::new(0.0, 0.0, 1.0)).length() < EPS);
        assert!((visuals.marker_t - 4.0).abs() < EPS);
    }
}

#[test]
fn tool_define_phase_leaves_visuals_none() {
    let (mut designer, node_id) = setup_atom_edit();
    // Entering the tool starts in `Define` — no line yet, so nothing is drawn.
    data_mut(&mut designer, node_id).active_tool = AtomEditTool::Guideline(GuidelineTool::new());

    assert!(eval_guideline_visuals(&designer, node_id, 0, true).is_none());
    assert!(eval_guideline_visuals(&designer, node_id, 1, true).is_none());
}

#[test]
fn tool_active_decorate_false_leaves_visuals_none() {
    let (mut designer, node_id) = setup_atom_edit();
    enter_active_tool(data_mut(&mut designer, node_id), sample_guideline());

    assert!(eval_guideline_visuals(&designer, node_id, 0, false).is_none());
    assert!(eval_guideline_visuals(&designer, node_id, 1, false).is_none());
}

#[test]
fn tool_picked_atom_is_highlighted_in_diff_view() {
    let (mut designer, node_id) = setup_atom_edit();
    let data = data_mut(&mut designer, node_id);
    let id = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 4.0));
    let mut g = Guideline::new(DVec3::ZERO, DVec3::new(0.0, 0.0, 1.0));
    g.t = 4.0;
    data.active_tool = AtomEditTool::Guideline(GuidelineTool {
        phase: GuidelinePhase::Active {
            guideline: g,
            picked: Some(AtomRef::Diff(id)),
            drag: GuidelineDragState::Idle,
        },
        entered_direction: DVec3::ZERO,
        remembered_t: g.t,
        pending: None,
    });

    // Pin 1 (diff): the picked diff atom should carry a non-Normal display state.
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator =
        rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::NetworkEvaluator::new(
        );
    let mut context =
        rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext::new(
        );
    let network_stack = vec![
        rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::NetworkStackElement {
            node_network: network,
            node_id: 0,
        },
    ];
    let result = evaluator.evaluate(&network_stack, node_id, 1, registry, true, &mut context);
    let atoms = match result {
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("expected molecule diff, got {:?}", other.infer_data_type()),
    };
    let state = atoms.decorator().get_atom_display_state(id);
    assert!(
        !matches!(
            state,
            rust_lib_flutter_cad::crystolecule::atomic_structure::AtomDisplayState::Normal
        ),
        "picked atom should be highlighted, got {:?}",
        state
    );
}
