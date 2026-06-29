//! Phase A3 of `doc/design_blueprint_region_atom_edits.md` — the `freeze` /
//! `unfreeze` region-gated metadata-edit nodes plus the `relax`-honors-frozen
//! behavior. Covers, per the phase plan: set/clear the frozen flag in-region
//! (and globally when the `region` pin is disconnected); `relax` holds frozen
//! atoms fixed while moving free ones; composition (`freeze(A) → freeze(B)` =
//! union frozen); and a `.cnnd` round-trip of a network containing the new
//! nodes with and without a wired `region`.

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
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
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

fn frozen_at(s: &AtomicStructure, x: f64) -> bool {
    s.iter_atoms()
        .find(|(_, a)| (a.position.x - x).abs() < 1e-6)
        .map(|(_, a)| a.is_frozen())
        .unwrap_or_else(|| panic!("no atom near x={}", x))
}

fn count_frozen(s: &AtomicStructure) -> usize {
    s.iter_atoms().filter(|(_, a)| a.is_frozen()).count()
}

/// Half-space whose in-region (`sdf ≤ margin`) side is `x ≤ 0`.
fn region_x_le_0() -> NetworkResult {
    blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO))
}

// ============================================================================
// freeze
// ============================================================================

/// Disconnected `region` → every atom is frozen.
#[test]
fn freeze_region_disconnected_freezes_all() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let node_id = add_simple_node(&mut designer, "freeze");
    designer.connect_nodes(value_id, 0, node_id, 0);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(count_frozen(&result), 2, "both carbons frozen");
}

/// With a half-space region, only in-region atoms are frozen.
#[test]
fn freeze_region_freezes_only_in_region() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let node_id = add_simple_node(&mut designer, "freeze");
    designer.connect_nodes(value_id, 0, node_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, node_id, 1);

    let result = evaluate_to_atomic(&designer, net, node_id);
    assert_eq!(count_frozen(&result), 1, "only the x=-1 carbon frozen");
    assert!(frozen_at(&result, -1.0), "in-region atom frozen");
    assert!(!frozen_at(&result, 1.0), "out-of-region atom not frozen");
}

// ============================================================================
// unfreeze
// ============================================================================

/// `unfreeze` clears the flag — globally when disconnected, in-region when wired.
#[test]
fn unfreeze_clears_in_region_only() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Pre-freeze both carbons (disconnected freeze), then unfreeze in-region.
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let freeze_id = add_simple_node(&mut designer, "freeze");
    designer.connect_nodes(value_id, 0, freeze_id, 0);

    let unfreeze_id = add_simple_node(&mut designer, "unfreeze");
    designer.connect_nodes(freeze_id, 0, unfreeze_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, unfreeze_id, 1);

    let result = evaluate_to_atomic(&designer, net, unfreeze_id);
    assert_eq!(count_frozen(&result), 1, "only out-of-region stays frozen");
    assert!(!frozen_at(&result, -1.0), "in-region atom unfrozen");
    assert!(frozen_at(&result, 1.0), "out-of-region atom still frozen");
}

/// Disconnected `unfreeze` clears the flag on every atom.
#[test]
fn unfreeze_region_disconnected_clears_all() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let freeze_id = add_simple_node(&mut designer, "freeze");
    designer.connect_nodes(value_id, 0, freeze_id, 0);
    let unfreeze_id = add_simple_node(&mut designer, "unfreeze");
    designer.connect_nodes(freeze_id, 0, unfreeze_id, 0);

    let result = evaluate_to_atomic(&designer, net, unfreeze_id);
    assert_eq!(count_frozen(&result), 0, "all atoms unfrozen");
}

// ============================================================================
// Composition — "multiple regions = chained nodes"
// ============================================================================

/// Two disjoint-region `freeze` nodes leave the union frozen. Region A freezes
/// x ≤ -0.9 (captures x = -2); region B freezes x ≥ 0.9 (captures x = +2); the
/// middle x = 0 carbon is left mobile.
#[test]
fn composition_two_chained_freezes_union() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-2.0, 0.0, 2.0])),
    );

    let node_a = add_simple_node(&mut designer, "freeze");
    designer.connect_nodes(value_id, 0, node_a, 0);
    let region_a = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::new(-1.0, 0.0, 0.0))),
    );
    designer.connect_nodes(region_a, 0, node_a, 1);

    let node_b = add_simple_node(&mut designer, "freeze");
    designer.connect_nodes(node_a, 0, node_b, 0);
    let region_b = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 400.0),
        blueprint_value(GeoNode::half_space(DVec3::NEG_X, DVec3::new(1.0, 0.0, 0.0))),
    );
    designer.connect_nodes(region_b, 0, node_b, 1);

    let result = evaluate_to_atomic(&designer, net, node_b);
    assert_eq!(count_frozen(&result), 2, "union of A and B frozen");
    assert!(frozen_at(&result, -2.0), "region A atom frozen");
    assert!(frozen_at(&result, 2.0), "region B atom frozen");
    assert!(!frozen_at(&result, 0.0), "middle atom mobile");
}

// ============================================================================
// relax honors the frozen flag
// ============================================================================

/// A frozen atom holds its position under `relax` while its mobile bonded
/// neighbor moves toward the equilibrium bond length. Two carbons bonded at a
/// stretched 2.0 Å separation; freeze the left one (via the global `freeze`
/// node) → after relax, the left atom is unmoved and the right atom has moved.
#[test]
fn relax_holds_frozen_atom_fixed() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    let mut s = AtomicStructure::new();
    let left = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let right = s.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    s.add_bond(left, right, BOND_SINGLE);

    let value_id = add_value_node(&mut designer, net, DVec2::ZERO, molecule_value(s));

    // Freeze only the left atom via a region x ≤ 0.5 (captures x = 0).
    let freeze_id = add_simple_node(&mut designer, "freeze");
    designer.connect_nodes(value_id, 0, freeze_id, 0);
    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::new(0.5, 0.0, 0.0))),
    );
    designer.connect_nodes(region_id, 0, freeze_id, 1);

    let relax_id = add_simple_node(&mut designer, "relax");
    designer.connect_nodes(freeze_id, 0, relax_id, 0);

    let result = evaluate_to_atomic(&designer, net, relax_id);

    let left_x = result
        .iter_atoms()
        .find(|(_, a)| a.is_frozen())
        .map(|(_, a)| a.position.x)
        .expect("frozen atom present");
    let right_x = result
        .iter_atoms()
        .find(|(_, a)| !a.is_frozen())
        .map(|(_, a)| a.position.x)
        .expect("mobile atom present");

    assert!(
        left_x.abs() < 1e-6,
        "frozen left atom must not move (was 0.0, got {})",
        left_x
    );
    assert!(
        (right_x - 2.0).abs() > 1e-3,
        "mobile right atom should relax toward equilibrium (still at {})",
        right_x
    );
    assert!(
        right_x < 2.0,
        "mobile atom should move inward toward the frozen atom (got {})",
        right_x
    );
}

// ============================================================================
// Serialization
// ============================================================================

/// A network with `freeze` (no region) and `unfreeze` (wired region) round-trips
/// through `.cnnd` — node types and the region wire survive.
#[test]
fn freeze_unfreeze_cnnd_roundtrip() {
    let net = "Main";
    let mut designer = setup_designer_with_network(net);

    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let freeze_id = add_simple_node(&mut designer, "freeze");
    designer.connect_nodes(value_id, 0, freeze_id, 0);

    let unfreeze_id = add_simple_node(&mut designer, "unfreeze");
    designer.connect_nodes(freeze_id, 0, unfreeze_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, unfreeze_id, 1);
    designer.validate_active_network();

    let temp_dir = tempdir().expect("temp dir");
    let temp_file = temp_dir.path().join("freeze.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &temp_file,
        false,
        &std::collections::HashMap::new(),
    )
    .expect("save");

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_file.to_str().unwrap()).expect("reload");

    let network = registry2.node_networks.get(net).unwrap();
    let type_of = |id: u64| network.nodes.get(&id).unwrap().node_type_name.as_str();
    assert_eq!(type_of(freeze_id), "freeze");
    assert_eq!(type_of(unfreeze_id), "unfreeze");

    // freeze's molecule wire survives; unfreeze's region wire (pin 1) survives.
    assert_eq!(
        network.nodes.get(&freeze_id).unwrap().arguments[0].get_source_pin(value_id),
        Some(0),
        "freeze.molecule ← value"
    );
    assert_eq!(
        network.nodes.get(&unfreeze_id).unwrap().arguments[1].get_source_pin(region_id),
        Some(0),
        "unfreeze.region ← region value"
    );
}
