//! Phase A2 of `doc/design_blueprint_region_atom_edits.md` — rolling the
//! optional `region: Blueprint` pin out to `add_hydrogen`, `remove_hydrogen`,
//! and `infer_bonds` via the shared `map_atomic_in_region` helper. Covers, per
//! the phase plan: disconnected-pin equivalence, per-node regional behavior,
//! the host-atom membership rule (`add_hydrogen` host in-region even when the
//! new H lands out-of-region; `remove_hydrogen` strips an out-of-boundary H
//! whose host is in-region), `infer_bonds` one-endpoint-inside + untouched-bond
//! preservation, and the load-bearing composition claim (two chained
//! region-gated ops == two separate single-region passes).

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    BlueprintData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::infer_bonds::InferBondsData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    pos: DVec2,
    value: NetworkResult,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.add_node("value", pos, 0, Box::new(ValueData { value }))
}

fn molecule_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: structure,
        geo_tree_root: None,
    })
}

/// Wrap a `GeoNode` SDF as a region Blueprint value (structure is ignored).
fn blueprint_value(geo_tree_root: GeoNode) -> NetworkResult {
    NetworkResult::Blueprint(BlueprintData {
        structure: Structure::diamond(),
        geo_tree_root,
        alignment: Default::default(),
        alignment_reason: None,
    })
}

fn add_simple_node(designer: &mut StructureDesigner, node_type: &str) -> u64 {
    designer.add_node(node_type, DVec2::new(200.0, 0.0))
}

fn add_infer_bonds_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    additive: bool,
) -> u64 {
    let id = designer.add_node("infer_bonds", DVec2::new(200.0, 0.0));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.set_node_network_data(
        id,
        Box::new(InferBondsData {
            additive,
            bond_tolerance: 1.15,
        }),
    );
    id
}

fn evaluate_to_atomic(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    let result = evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context);
    match result {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(e) => panic!("Expected Atomic result, got Error: {}", e),
        other => panic!("Expected Atomic result, got {:?}", other.infer_data_type()),
    }
}

/// A line of carbons along +x at the given x coordinates, all element 6.
fn carbons_at(xs: &[f64]) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    for &x in xs {
        s.add_atom(6, DVec3::new(x, 0.0, 0.0));
    }
    s
}

fn count_element(s: &AtomicStructure, atomic_number: i16) -> usize {
    s.iter_atoms()
        .filter(|(_, a)| a.atomic_number == atomic_number)
        .count()
}

/// Number of hydrogens bonded to the atom at the given position (within eps).
fn hydrogens_on_atom_at(s: &AtomicStructure, x: f64) -> usize {
    let host = s
        .iter_atoms()
        .find(|(_, a)| a.atomic_number == 6 && (a.position.x - x).abs() < 1e-6)
        .map(|(id, _)| *id);
    let Some(host) = host else { return 0 };
    let atom = s.get_atom(host).unwrap();
    atom.bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .filter(|b| {
            s.get_atom(b.other_atom_id())
                .is_some_and(|n| n.atomic_number == 1)
        })
        .count()
}

/// Half-space whose in-region (`sdf ≤ margin`) side is `x ≤ 0`.
fn region_x_le_0() -> NetworkResult {
    blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO))
}

// ============================================================================
// add_hydrogen
// ============================================================================

/// Disconnected `region` → every undersaturated atom is passivated (today's
/// behavior). Two lone sp3 carbons → 4 H each → 8 H total.
#[test]
fn add_hydrogen_region_disconnected_matches_global() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let node_id = add_simple_node(&mut designer, "passivate");
    designer.connect_nodes(value_id, 0, node_id, 0);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(
        count_element(&result, 1),
        8,
        "both carbons fully passivated"
    );
}

/// With a half-space region, only in-region host carbons gain hydrogens; the
/// out-of-region carbon is left bare.
#[test]
fn add_hydrogen_region_passivates_only_in_region() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let node_id = add_simple_node(&mut designer, "passivate");
    designer.connect_nodes(value_id, 0, node_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, node_id, 1);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(
        count_element(&result, 1),
        4,
        "only the x=-1 carbon passivated"
    );
    assert_eq!(
        hydrogens_on_atom_at(&result, -1.0),
        4,
        "in-region host has 4 H"
    );
    assert_eq!(
        hydrogens_on_atom_at(&result, 1.0),
        0,
        "out-of-region host bare"
    );
}

/// Host-membership invariant: a host sitting *just* inside the region is
/// passivated even though its tetrahedral hydrogens land partly outside the
/// region — newly created atoms are never membership-tested.
#[test]
fn add_hydrogen_h_across_boundary_still_placed() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Single carbon at x = -0.05 (just inside x ≤ 0). Its 4 tetrahedral H sit
    // ~0.63 Å along ±x, so two of them land at x > 0.1 (outside the region).
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-0.05])),
    );
    let node_id = add_simple_node(&mut designer, "passivate");
    designer.connect_nodes(value_id, 0, node_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, node_id, 1);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(
        count_element(&result, 1),
        4,
        "all 4 H placed despite some landing out-of-region"
    );
    // Confirm at least one H is actually outside the region (x > margin).
    let any_h_outside = result
        .iter_atoms()
        .any(|(_, a)| a.atomic_number == 1 && a.position.x > 0.1);
    assert!(
        any_h_outside,
        "expected a passivating H to land outside the region"
    );
}

// ============================================================================
// remove_hydrogen
// ============================================================================

/// Build two C–H units, one on each side of the plane, and bond each H to its
/// carbon. `s` returns (structure, [c_left_id, h_left_id, c_right_id, h_right_id]).
fn two_ch_units(left_h_x: f64, right_h_x: f64) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c_left = s.add_atom(6, DVec3::new(-0.5, 0.0, 0.0));
    let h_left = s.add_atom(1, DVec3::new(left_h_x, 0.0, 0.0));
    s.add_bond(c_left, h_left, BOND_SINGLE);
    let c_right = s.add_atom(6, DVec3::new(0.5, 0.0, 0.0));
    let h_right = s.add_atom(1, DVec3::new(right_h_x, 0.0, 0.0));
    s.add_bond(c_right, h_right, BOND_SINGLE);
    s
}

/// Disconnected `region` → all H stripped globally.
#[test]
fn remove_hydrogen_region_disconnected_matches_global() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(two_ch_units(-1.0, 1.0)),
    );
    let node_id = add_simple_node(&mut designer, "remove_hydrogen");
    designer.connect_nodes(value_id, 0, node_id, 0);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(count_element(&result, 1), 0, "all H stripped");
    assert_eq!(count_element(&result, 6), 2, "carbons untouched");
}

/// With a region, only H whose host carbon is in-region are stripped. The left
/// carbon (x = -0.5) is in-region; the right carbon (x = +0.5) is not.
#[test]
fn remove_hydrogen_region_strips_only_in_region_hosts() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(two_ch_units(-1.0, 1.0)),
    );
    let node_id = add_simple_node(&mut designer, "remove_hydrogen");
    designer.connect_nodes(value_id, 0, node_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, node_id, 1);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(
        count_element(&result, 1),
        1,
        "only the in-region host's H removed"
    );
    // The surviving H is the one bonded to the right (out-of-region) carbon.
    let surviving_h = result
        .iter_atoms()
        .find(|(_, a)| a.atomic_number == 1)
        .map(|(_, a)| a.position.x);
    assert_eq!(surviving_h, Some(1.0), "right (out-of-region) H survives");
}

/// Host-membership invariant: an H sitting *outside* the region is still
/// stripped when its host carbon is in-region. Host at x = -0.5 (in-region),
/// its H placed at x = +0.2 (out-of-region) → removed.
#[test]
fn remove_hydrogen_strips_out_of_boundary_h_with_in_region_host() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Left C–H: host in-region, H placed at x = +0.2 (outside region).
    // Right C–H: host out-of-region, H at x = +1.5.
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(two_ch_units(0.2, 1.5)),
    );
    let node_id = add_simple_node(&mut designer, "remove_hydrogen");
    designer.connect_nodes(value_id, 0, node_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, node_id, 1);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(
        count_element(&result, 1),
        1,
        "out-of-boundary H stripped (host in-region); other H survives"
    );
    let surviving_h = result
        .iter_atoms()
        .find(|(_, a)| a.atomic_number == 1)
        .map(|(_, a)| a.position.x);
    assert_eq!(
        surviving_h,
        Some(1.5),
        "only the out-of-region host's H survives"
    );
}

// ============================================================================
// infer_bonds
// ============================================================================

/// Disconnected `region` → behaves like the global infer (clear + re-infer all).
/// Three collinear carbons at spacing 1.0 Å bond into a 2-bond chain.
#[test]
fn infer_bonds_region_disconnected_matches_global() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 0.0, 1.0])),
    );
    let node_id = add_infer_bonds_node(&mut designer, net, false);
    designer.connect_nodes(value_id, 0, node_id, 0);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(result.get_num_of_bonds(), 2, "chain of 2 bonds inferred");
}

/// One-endpoint-inside: a bond forms when at least one endpoint is in-region.
/// In-region carbon at x = -0.5 and out-of-region carbon at x = +0.5 (distance
/// 1.0) bond; two fully out-of-region carbons (x = 5, 6) do not.
#[test]
fn infer_bonds_region_one_endpoint_inside() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-0.5, 0.5, 5.0, 6.0])),
    );
    let node_id = add_infer_bonds_node(&mut designer, net, false);
    designer.connect_nodes(value_id, 0, node_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, node_id, 3);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(
        result.get_num_of_bonds(),
        1,
        "only the boundary-straddling pair bonds; the fully out-of-region pair does not"
    );
    let c_in = result
        .iter_atoms()
        .find(|(_, a)| (a.position.x - (-0.5)).abs() < 1e-6)
        .map(|(id, _)| *id)
        .unwrap();
    let c_out = result
        .iter_atoms()
        .find(|(_, a)| (a.position.x - 0.5).abs() < 1e-6)
        .map(|(id, _)| *id)
        .unwrap();
    assert!(result.has_bond_between(c_in, c_out), "in/out pair bonded");
}

/// Untouched preservation: a bond between two out-of-region atoms survives even
/// though the global infer (non-additive) would clear it and not re-infer it
/// (the atoms are too far apart to be re-bonded).
#[test]
fn infer_bonds_region_preserves_out_of_region_bond() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Two out-of-region carbons at x = 5, 8 (distance 3.0 ≫ bond length),
    // manually bonded. In-region carbons at x = -1, 0 (distance 1.0) will bond.
    let mut s = carbons_at(&[-1.0, 0.0, 5.0, 8.0]);
    let ids: Vec<u32> = s.atom_ids().copied().collect();
    s.add_bond(ids[2], ids[3], BOND_SINGLE); // bond the far out-of-region pair
    let far_a = ids[2];
    let far_b = ids[3];
    let value_id = add_value_node(&mut designer, net, DVec2::ZERO, molecule_value(s));
    let node_id = add_infer_bonds_node(&mut designer, net, false);
    designer.connect_nodes(value_id, 0, node_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, node_id, 3);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert!(
        result.has_bond_between(far_a, far_b),
        "out-of-region bond preserved untouched despite non-additive clear+reinfer"
    );
    assert_eq!(
        result.get_num_of_bonds(),
        2,
        "preserved far bond + one freshly inferred in-region bond"
    );
}

/// Sanity counterpart: without a region, the same far manual bond is cleared
/// and not re-inferred (proving the preservation above is region-specific).
#[test]
fn infer_bonds_no_region_clears_far_bond() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mut s = carbons_at(&[-1.0, 0.0, 5.0, 8.0]);
    let ids: Vec<u32> = s.atom_ids().copied().collect();
    s.add_bond(ids[2], ids[3], BOND_SINGLE);
    let far_a = ids[2];
    let far_b = ids[3];
    let value_id = add_value_node(&mut designer, net, DVec2::ZERO, molecule_value(s));
    let node_id = add_infer_bonds_node(&mut designer, net, false);
    designer.connect_nodes(value_id, 0, node_id, 0);
    // region pin (index 3) left unconnected

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert!(
        !result.has_bond_between(far_a, far_b),
        "global non-additive infer clears the far manual bond"
    );
    assert_eq!(
        result.get_num_of_bonds(),
        1,
        "only the x=-1..0 bond inferred"
    );
}

// ============================================================================
// Composition — "multiple regions = chained nodes"
// ============================================================================

/// The load-bearing Part A claim: two region-gated ops in sequence accumulate
/// exactly as two separate single-region passes would. Two `add_hydrogen` nodes
/// with disjoint regions A (x ≤ -0.9) and B (x ≥ 0.9) passivate the x=-2 and
/// x=+2 carbons respectively, leaving the middle x=0 carbon bare.
#[test]
fn composition_two_chained_region_ops_accumulate() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-2.0, 0.0, 2.0])),
    );

    // Node A: region = half-space, in-region x ≤ -0.9 (captures x = -2).
    let node_a = add_simple_node(&mut designer, "passivate");
    designer.connect_nodes(value_id, 0, node_a, 0);
    let region_a = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::new(-1.0, 0.0, 0.0))),
    );
    designer.connect_nodes(region_a, 0, node_a, 1);

    // Node B: region = half-space, in-region x ≥ 0.9 (captures x = +2).
    let node_b = add_simple_node(&mut designer, "passivate");
    designer.connect_nodes(node_a, 0, node_b, 0);
    let region_b = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 400.0),
        blueprint_value(GeoNode::half_space(DVec3::NEG_X, DVec3::new(1.0, 0.0, 0.0))),
    );
    designer.connect_nodes(region_b, 0, node_b, 1);

    let result = evaluate_to_atomic(&designer, net, node_b);
    assert_eq!(count_element(&result, 1), 8, "x=-2 and x=+2 each gain 4 H");
    assert_eq!(
        hydrogens_on_atom_at(&result, -2.0),
        4,
        "region A carbon passivated"
    );
    assert_eq!(
        hydrogens_on_atom_at(&result, 2.0),
        4,
        "region B carbon passivated"
    );
    assert_eq!(
        hydrogens_on_atom_at(&result, 0.0),
        0,
        "middle carbon untouched by both"
    );
}
