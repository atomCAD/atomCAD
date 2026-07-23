//! Phase A1 of `doc/design_blueprint_region_atom_edits.md` — the optional
//! `region: Blueprint` pin on `atom_replace`, gated through
//! `map_atomic_in_region`. Covers: disconnected-pin equivalence, half-space
//! regional replacement, boundary/margin capture, disjoint no-op, phase
//! preservation, and a wired-region `.cnnd` round-trip.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    BlueprintData, CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::atom_replace::AtomReplaceData;
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

/// Add a `value` node carrying an already-built `NetworkResult`.
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

fn crystal_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: structure,
        geo_tree_root: None,
        alignment: Default::default(),
        alignment_reason: None,
    })
}

/// Wrap a `GeoNode` SDF as a region Blueprint value (structure is ignored by
/// the region machinery, so a diamond placeholder is fine).
fn blueprint_value(geo_tree_root: GeoNode) -> NetworkResult {
    NetworkResult::Blueprint(BlueprintData {
        structure: Structure::diamond(),
        geo_tree_root,
        alignment: Default::default(),
        alignment_reason: None,
    })
}

fn add_replace_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    replacements: Vec<(i16, i16)>,
) -> u64 {
    let replace_id = designer.add_node("atom_replace", DVec2::new(200.0, 0.0));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.set_node_network_data(replace_id, Box::new(AtomReplaceData { replacements }));
    replace_id
}

fn evaluate_to_atomic_in(
    registry: &NodeTypeRegistry,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
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

fn evaluate_to_atomic(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
    evaluate_to_atomic_in(&designer.node_type_registry, network_name, node_id)
}

fn evaluate_to_result(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

/// A line of carbons along +x at the given x coordinates, all element 6.
fn carbons_at(xs: &[f64]) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    for &x in xs {
        s.add_atom(6, DVec3::new(x, 0.0, 0.0));
    }
    s
}

/// Count atoms of a given element.
fn count_element(s: &AtomicStructure, atomic_number: i16) -> usize {
    s.iter_atoms()
        .filter(|(_, a)| a.atomic_number == atomic_number)
        .count()
}

// ============================================================================
// Tests
// ============================================================================

/// Disconnected `region` pin → identical to today's global replacement.
#[test]
fn region_disconnected_matches_global() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 0.0, 1.0, 3.0])),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]); // C→Si
    designer.connect_nodes(value_id, 0, replace_id, 0);
    // region pin (index 2) left unconnected

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(
        count_element(&result, 14),
        4,
        "all carbons replaced globally"
    );
    assert_eq!(count_element(&result, 6), 0);
}

/// Half-space region (normal +x, plane at origin) → only atoms with x ≤ margin
/// are replaced; the rest pass through unchanged.
#[test]
fn region_half_space_replaces_only_in_region() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-2.0, -1.0, 1.0, 2.0])),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]); // C→Si
    designer.connect_nodes(value_id, 0, replace_id, 0);

    // Region = half-space sdf = +x · (p - origin); in-region (sdf ≤ margin)
    // is the x ≤ 0 side. Captures x = -2 and x = -1.
    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO)),
    );
    designer.connect_nodes(region_id, 0, replace_id, 2);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(count_element(&result, 14), 2, "two in-region carbons → Si");
    assert_eq!(
        count_element(&result, 6),
        2,
        "two out-of-region carbons kept"
    );

    // Verify it is specifically the x ≤ 0 atoms that changed.
    for (_, atom) in result.iter_atoms() {
        if atom.position.x <= 0.0 {
            assert_eq!(atom.atomic_number, 14, "x={} should be Si", atom.position.x);
        } else {
            assert_eq!(atom.atomic_number, 6, "x={} should stay C", atom.position.x);
        }
    }
}

/// A boundary-coincident region captures atoms sitting on the plane (sdf = 0)
/// and atoms within `DEFAULT_REGION_MARGIN` (0.1 Å), but not beyond it.
#[test]
fn region_boundary_coincident_captures_margin() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // x = 0.0 (on plane), 0.05 (within margin), 0.2 (beyond margin)
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0, 0.05, 0.2])),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO)),
    );
    designer.connect_nodes(region_id, 0, replace_id, 2);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(
        count_element(&result, 14),
        2,
        "on-plane and within-margin atoms captured"
    );
    for (_, atom) in result.iter_atoms() {
        if atom.position.x <= 0.1 {
            assert_eq!(atom.atomic_number, 14);
        } else {
            assert_eq!(atom.atomic_number, 6, "x=0.2 is beyond margin");
        }
    }
}

/// A region whose volume is disjoint from the structure → no atom is in-region
/// → no-op (well-defined).
#[test]
fn region_disjoint_is_noop() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0, 1.0, 2.0])),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    // Sphere far away from the atoms (centered at x=1000, radius 1).
    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::sphere(DVec3::new(1000.0, 0.0, 0.0), 1.0)),
    );
    designer.connect_nodes(region_id, 0, replace_id, 2);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(
        count_element(&result, 6),
        3,
        "nothing in region → unchanged"
    );
    assert_eq!(count_element(&result, 14), 0);
}

/// Phase preservation: a Crystal input with a region stays a Crystal.
#[test]
fn region_preserves_crystal_phase() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        crystal_value(carbons_at(&[-1.0, 1.0])),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);
    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO)),
    );
    designer.connect_nodes(region_id, 0, replace_id, 2);

    match evaluate_to_result(&designer, net, replace_id) {
        NetworkResult::Crystal(c) => {
            assert_eq!(count_element(&c.atoms, 14), 1, "in-region C → Si");
            assert_eq!(count_element(&c.atoms, 6), 1, "out-of-region C kept");
        }
        other => panic!(
            "Crystal-in must yield Crystal-out, got {:?}",
            other.infer_data_type()
        ),
    }
}

/// Phase preservation: a Molecule input with a region stays a Molecule.
#[test]
fn region_preserves_molecule_phase() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);
    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO)),
    );
    designer.connect_nodes(region_id, 0, replace_id, 2);

    match evaluate_to_result(&designer, net, replace_id) {
        NetworkResult::Molecule(m) => {
            assert_eq!(count_element(&m.atoms, 14), 1);
            assert_eq!(count_element(&m.atoms, 6), 1);
        }
        other => panic!(
            "Molecule-in must yield Molecule-out, got {:?}",
            other.infer_data_type()
        ),
    }
}

/// Delete-by-region: target 0 deletes only in-region atoms, leaving the
/// out-of-region atoms (and any bonds among them) intact.
#[test]
fn region_delete_only_in_region() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mut s = carbons_at(&[-2.0, -1.0, 1.0, 2.0]);
    // bond the two out-of-region atoms so we can confirm it survives
    let ids: Vec<u32> = s.atom_ids().copied().collect();
    // ids correspond to insertion order: [-2, -1, 1, 2]
    s.add_bond(ids[2], ids[3], BOND_SINGLE);
    let value_id = add_value_node(&mut designer, net, DVec2::ZERO, molecule_value(s));
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 0)]); // delete C
    designer.connect_nodes(value_id, 0, replace_id, 0);
    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO)),
    );
    designer.connect_nodes(region_id, 0, replace_id, 2);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(result.atom_ids().count(), 2, "two in-region atoms deleted");
    assert_eq!(result.get_num_of_bonds(), 1, "out-of-region bond intact");
    for (_, atom) in result.iter_atoms() {
        assert!(atom.position.x > 0.0, "only x>0 atoms remain");
    }
}

/// A network with a *wired* `region` pin survives a `.cnnd` round-trip with the
/// wire intact (backward-compat: no migration; the region wire to pin index 2
/// is ordinary wire plumbing). Note: `value` nodes carry a `#[serde(skip)]`
/// payload, so re-evaluation after reload is not meaningful — we assert the
/// structural survival of the region wire instead.
#[test]
fn region_wired_cnnd_roundtrip() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);
    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO)),
    );
    designer.connect_nodes(region_id, 0, replace_id, 2);
    // Make the replace node the return node so it survives any pruning.
    designer.set_return_node_id(Some(replace_id));

    // Save → reload into a fresh registry.
    let temp_dir = tempdir().expect("temp dir");
    let temp_file = temp_dir.path().join("region_roundtrip.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &temp_file,
        false,
        &Default::default(),
    )
    .expect("save");

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_file.to_str().unwrap()).expect("reload");

    // The region wire (destination pin index 2 on the replace node) survived,
    // pointing back at the region node.
    let network2 = registry2.node_networks.get(net).unwrap();
    let replace2 = network2.nodes.get(&replace_id).unwrap();
    assert!(
        replace2.arguments.len() >= 3,
        "atom_replace should have at least 3 argument slots (molecule, rules, region), got {}",
        replace2.arguments.len()
    );
    assert_eq!(
        replace2.arguments[2].get_node_id(),
        Some(region_id),
        "region wire (pin index 2) should survive the round-trip pointing at the region node"
    );
}

/// Backward-compat: an `atom_replace` node deserialized with only the two
/// pre-region argument slots (molecule, rules) is repaired to expose the new
/// `region` pin unconnected — no migration, no version bump.
#[test]
fn region_backward_compat_argument_padding() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0, 1.0])),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    // Simulate a pre-region node: truncate its argument slots to the old count
    // of 2 (molecule, rules), as an old `.cnnd` would have stored them.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(net)
            .unwrap();
        let node = network.nodes.get_mut(&replace_id).unwrap();
        node.arguments.truncate(2);
        assert_eq!(node.arguments.len(), 2);
    }

    // Validation/repair should pad the arguments back up to the node type's
    // parameter count (3) with the region pin unconnected.
    designer.validate_active_network();

    let network = designer.node_type_registry.node_networks.get(net).unwrap();
    let node = network.nodes.get(&replace_id).unwrap();
    assert!(
        node.arguments.len() >= 3,
        "region pin should be padded in, got {} argument slots",
        node.arguments.len()
    );
    assert!(
        node.arguments[2].is_empty(),
        "padded region pin should be unconnected"
    );

    // And the node still evaluates (globally, since region is unconnected).
    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(count_element(&result, 14), 2, "global replace still works");
}
