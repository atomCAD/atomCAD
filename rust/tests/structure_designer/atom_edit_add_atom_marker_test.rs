//! Tests for issue #386: adding an atom bonded to a *base* atom must record the
//! clicked base anchor as an UNCHANGED marker (a pure bond-endpoint reference),
//! not a full element+position override, so that:
//!   1. `apply_diff` lets the base atom's *current* element/position flow through
//!      (a later Si→C change upstream is not forced back to Si), and
//!   2. diff-only minimization (`FreezeBase`) does not drag the anchor around.
//!
//! Also covers the write-back rule: in the other minimize modes, where a
//! marker-matched atom *is* allowed to move, the marker must be promoted in place
//! to a real anchored diff atom rather than having its stored position moved
//! (which would break its base match and drop it as an orphan).

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIAtomEditTool;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, UNCHANGED_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::apply_diff;
use rust_lib_flutter_cad::crystolecule::guided_placement::{
    BondLengthMode, BondMode, Hybridization,
};
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, GuidedPlacementStartResult, MinimizeFreezeMode, minimize_atom_edit,
    start_guided_placement,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// =============================================================================
// Helpers
// =============================================================================

fn setup_atom_edit_with_base(base: AtomicStructure) -> StructureDesigner {
    use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
        MoleculeData, NetworkResult,
    };
    use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;

    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let value_data = Box::new(ValueData {
        value: NetworkResult::Molecule(MoleculeData {
            atoms: base,
            geo_tree_root: None,
        }),
    });
    let value_id = network.add_node("value", DVec2::ZERO, 0, value_data);

    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, atom_edit_id, 0);
    designer.select_node(atom_edit_id);
    refresh(&mut designer);
    designer.undo_stack.clear();
    designer
}

fn refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

fn get_data_mut(designer: &mut StructureDesigner) -> &mut AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let data = network.get_node_network_data_mut(node_id).unwrap();
    data.as_any_mut().downcast_mut::<AtomEditData>().unwrap()
}

/// Evaluates the atom_edit node's result pin (pin 0) and returns the structure.
fn evaluate_result(designer: &StructureDesigner) -> AtomicStructure {
    use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
        NetworkEvaluationContext, NetworkStackElement,
    };
    use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;

    let network_name = designer.active_node_network_name.as_ref().unwrap();
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let mut context = NetworkEvaluationContext::new();
    let result = designer.network_evaluator.evaluate(
        &network_stack,
        node_id,
        0,
        &designer.node_type_registry,
        false,
        &mut context,
    );
    match result {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        _ => panic!("Expected an atomic result on pin 0"),
    }
}

// =============================================================================
// Semantic payoff (issue #386 ★2): a marker anchor does not pin the base element
// =============================================================================

/// The whole point of recording the anchor as an UNCHANGED marker: when the base
/// changes an atom's element upstream (Si→C), re-applying the same diff must NOT
/// force it back to the element that happened to be there at edit time.
#[test]
fn marker_anchor_lets_base_element_change_flow_through() {
    // Diff as the (fixed) Add Atom tool now records it: a bare UNCHANGED marker at
    // the anchor position (no element, no anchor). Here just the anchor, no partner
    // atom — enough to observe pass-through element behavior.
    let make_marker_diff = || {
        let mut diff = AtomicStructure::new_diff();
        diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::ZERO);
        diff
    };

    // Base originally has Silicon at the anchor.
    let mut base_si = AtomicStructure::new();
    base_si.add_atom(14, DVec3::ZERO);
    let r_si = apply_diff(&base_si, &make_marker_diff(), 0.1);
    assert_eq!(r_si.result.get_num_of_atoms(), 1);
    assert_eq!(
        r_si.result.atoms_values().next().unwrap().atomic_number,
        14,
        "marker should pass the Silicon base atom through unchanged"
    );

    // Upstream later changes that atom to Carbon. The SAME diff must now yield C.
    let mut base_c = AtomicStructure::new();
    base_c.add_atom(6, DVec3::ZERO);
    let r_c = apply_diff(&base_c, &make_marker_diff(), 0.1);
    assert_eq!(
        r_c.result.atoms_values().next().unwrap().atomic_number,
        6,
        "marker must NOT force the element back to Silicon after a Si→C base change"
    );
}

/// Contrast test documenting the *old* behavior that #386 fixes: a full
/// element+position copy (what the Add Atom tool used to record for a base
/// anchor) overrides the base element, so a Si→C base change is silently undone.
#[test]
fn full_copy_anchor_pins_the_stale_element() {
    // Old-style anchor: a real Silicon atom copied at the anchor position, no
    // anchor set (matched by position) — exactly what add_atom_recorded produced.
    let make_full_copy_diff = || {
        let mut diff = AtomicStructure::new_diff();
        diff.add_atom(14, DVec3::ZERO); // Silicon copy
        diff
    };

    // Base is now Carbon, but the stale full copy forces it back to Silicon.
    let mut base_c = AtomicStructure::new();
    base_c.add_atom(6, DVec3::ZERO);
    let r = apply_diff(&base_c, &make_full_copy_diff(), 0.1);
    assert_eq!(
        r.result.atoms_values().next().unwrap().atomic_number,
        14,
        "full-copy anchor overrides the base element (the #386 bug this change removes)"
    );
}

// =============================================================================
// Change 1: Add Atom on a base anchor records an UNCHANGED marker
// =============================================================================

/// Firing the Add Atom tool at a base atom promotes that anchor into the diff as
/// an UNCHANGED marker, not a full element+position copy.
#[test]
fn add_atom_on_base_anchor_records_unchanged_marker() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::ZERO); // a lone (bare) carbon anchor
    let mut designer = setup_atom_edit_with_base(base);

    get_data_mut(&mut designer).set_active_tool(APIAtomEditTool::AddAtom);

    // Ray straight down the -Z axis onto the carbon at the origin.
    let ray_start = DVec3::new(0.0, 0.0, 10.0);
    let ray_dir = DVec3::new(0.0, 0.0, -1.0);
    let result = start_guided_placement(
        &mut designer,
        &ray_start,
        &ray_dir,
        6,    // element being added (irrelevant to the anchor)
        None, // Auto hybridization → marker path
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );
    assert!(
        matches!(result, GuidedPlacementStartResult::Started { .. }),
        "guided placement should start on a bare atom, got {result:?}"
    );

    let diff = &get_data_mut(&mut designer).diff;
    assert_eq!(
        diff.get_num_of_atoms(),
        1,
        "only the anchor should have been promoted into the diff"
    );
    let anchor = diff.atoms_values().next().unwrap();
    assert_eq!(
        anchor.atomic_number, UNCHANGED_ATOMIC_NUMBER,
        "base anchor must be recorded as an UNCHANGED marker, not a full copy"
    );
    assert!(
        diff.anchor_position(anchor.id).is_none(),
        "a bond-endpoint marker must not carry an anchor position"
    );
}

/// Exception to change 1: when the user has a non-Auto hybridization override
/// active, the anchor IS being edited (a per-atom flag), which cannot live on a
/// marker — so the tool falls back to a full element+position promotion.
#[test]
fn add_atom_on_base_anchor_with_hybridization_override_promotes_fully() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::ZERO);
    let mut designer = setup_atom_edit_with_base(base);

    get_data_mut(&mut designer).set_active_tool(APIAtomEditTool::AddAtom);

    let ray_start = DVec3::new(0.0, 0.0, 10.0);
    let ray_dir = DVec3::new(0.0, 0.0, -1.0);
    let result = start_guided_placement(
        &mut designer,
        &ray_start,
        &ray_dir,
        6,
        Some(Hybridization::Sp2), // non-Auto → full promotion path
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );
    assert!(matches!(result, GuidedPlacementStartResult::Started { .. }));

    let diff = &get_data_mut(&mut designer).diff;
    let anchor = diff.atoms_values().next().unwrap();
    assert_eq!(
        anchor.atomic_number, 6,
        "with a hybridization override the anchor must be a real (non-marker) diff atom"
    );
    assert!(!anchor.is_unchanged_marker());
}

// =============================================================================
// Change 2 + 3: minimization and UNCHANGED markers
// =============================================================================

/// Builds an atom_edit whose diff is exactly what the fixed Add Atom tool would
/// produce when bonding a new hydrogen to a base carbon: an UNCHANGED marker at
/// the carbon plus an added hydrogen bonded to it. Returns (designer, marker_id,
/// hydrogen_id).
fn setup_marker_plus_added(hydrogen_pos: DVec3) -> (StructureDesigner, u32, u32) {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::ZERO); // carbon base anchor
    let mut designer = setup_atom_edit_with_base(base);

    let (marker_id, h_id) = {
        let data = get_data_mut(&mut designer);
        let marker_id = data.diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::ZERO);
        let h_id = data.diff.add_atom(1, hydrogen_pos);
        data.diff.add_bond(marker_id, h_id, BOND_SINGLE);
        (marker_id, h_id)
    };
    refresh(&mut designer);
    (designer, marker_id, h_id)
}

/// `FreezeBase` must treat the UNCHANGED-marker anchor as frozen: minimization
/// relaxes the added hydrogen but leaves the marker untouched (still a marker,
/// still at its base position — never promoted or moved).
#[test]
fn freeze_base_keeps_unchanged_marker_fixed() {
    // Hydrogen placed too close (0.6 Å) so the bond-stretch force is large.
    let (mut designer, marker_id, h_id) = setup_marker_plus_added(DVec3::new(0.6, 0.0, 0.0));

    let msg = minimize_atom_edit(&mut designer, MinimizeFreezeMode::FreezeBase)
        .expect("minimize should succeed");
    assert!(msg.contains("iterations"), "unexpected message: {msg}");

    let data = get_data_mut(&mut designer);
    // Marker must be untouched: still an UNCHANGED marker, still at the origin.
    let marker = data.diff.get_atom(marker_id).unwrap();
    assert_eq!(
        marker.atomic_number, UNCHANGED_ATOMIC_NUMBER,
        "FreezeBase must not promote the frozen marker"
    );
    assert!(
        marker.position.length() < 1e-6,
        "FreezeBase must not move the frozen marker (pos = {:?})",
        marker.position
    );
    // The added hydrogen, being free, must have relaxed away from its cramped spot.
    let h = data.diff.get_atom(h_id).unwrap();
    assert!(
        (h.position - DVec3::new(0.6, 0.0, 0.0)).length() > 0.05,
        "the free hydrogen should have moved during minimization (pos = {:?})",
        h.position
    );

    // The result still shows the carbon (base element) at the origin.
    let result = evaluate_result(&designer);
    assert_eq!(result.get_num_of_atoms(), 2);
    let carbon = result
        .atoms_values()
        .find(|a| a.atomic_number == 6)
        .expect("carbon should be present");
    assert!(carbon.position.length() < 1e-6);
}

/// In a free mode (`FreeAll`) the marker-matched atom is allowed to move. The
/// write-back must then promote the marker in place to a real anchored diff atom
/// rather than shifting its stored position — otherwise apply_diff would fail to
/// match it to the base atom and silently drop it (orphan / atom count mismatch).
#[test]
fn free_all_promotes_moved_marker_without_orphaning() {
    // Hydrogen placed far (3.0 Å): the stretched bond pulls the carbon (marker)
    // inward by ~1 Å, guaranteeing the marker moves > tolerance.
    let (mut designer, marker_id, _h_id) = setup_marker_plus_added(DVec3::new(3.0, 0.0, 0.0));

    minimize_atom_edit(&mut designer, MinimizeFreezeMode::FreeAll)
        .expect("minimize should succeed");

    // The marker moved, so it must have been promoted in place: a real carbon
    // (base element) with an anchor keeping it matched to the base atom.
    let data = get_data_mut(&mut designer);
    let promoted = data.diff.get_atom(marker_id).unwrap();
    assert_eq!(
        promoted.atomic_number, 6,
        "a moved marker must be promoted to the base element, not left as a marker"
    );
    assert!(
        !promoted.is_unchanged_marker(),
        "the moved marker must no longer be an UNCHANGED marker"
    );
    assert!(
        data.diff.anchor_position(marker_id).is_some(),
        "the promoted atom must carry an anchor so apply_diff still matches the base atom"
    );
    assert!(
        promoted.position.length() > 1e-6,
        "the promoted carbon should have moved off the origin"
    );

    // Crucially: no atom was orphaned/dropped. Result still has both atoms, with
    // the carbon present (matched via the anchor, not duplicated).
    let result = evaluate_result(&designer);
    assert_eq!(
        result.get_num_of_atoms(),
        2,
        "moving the marker must not orphan/drop or duplicate the base atom"
    );
    assert_eq!(
        result
            .atoms_values()
            .filter(|a| a.atomic_number == 6)
            .count(),
        1,
        "exactly one carbon should survive (base matched, not duplicated)"
    );
}
