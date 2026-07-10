//! Tests for the `diff` output pin on `atom_replace` and `atom_cut` — Phase 4 of
//! `doc/design_diff_outputs_for_atom_ops.md` (issue #295).
//!
//! Uses the `value`-node harness and the shared `assert_node_diff_roundtrip`
//! helper from `diff_test_support.rs`.

use glam::f64::{DVec2, DVec3};
use glam::i32::IVec2;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, DELETED_SITE_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::apply_diff;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

use crate::diff_test_support::{APPLY_TOLERANCE, assert_node_diff_roundtrip, evaluate_pin};

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
    value: NetworkResult,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.add_node("value", DVec2::ZERO, 0, Box::new(ValueData { value }))
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
        alignment: Alignment::Aligned,
        alignment_reason: None,
    })
}

/// A Blueprint value carrying `geo` as its cutter/region volume.
fn blueprint_value(geo: GeoNode) -> NetworkResult {
    NetworkResult::Blueprint(BlueprintData {
        structure: Structure::diamond(),
        geo_tree_root: geo,
        alignment: Alignment::Aligned,
        alignment_reason: None,
    })
}

/// Sets text properties on a node's data.
fn set_props(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    props: Vec<(&str, TextValue)>,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let map: HashMap<String, TextValue> =
        props.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    node.data.set_text_properties(&map).unwrap();
}

/// Adds a node of `node_type` wired to `input_id` pin 0 → its pin 0. Returns id.
fn add_op(designer: &mut StructureDesigner, node_type: &str, input_id: u64) -> u64 {
    let id = designer.add_node(node_type, DVec2::new(200.0, 0.0));
    designer.connect_nodes(input_id, 0, id, 0);
    id
}

/// Extracts the diff atoms out of a node's pin-1 value, asserting it's a diff.
fn diff_atoms(designer: &StructureDesigner, net: &str, node_id: u64) -> AtomicStructure {
    match evaluate_pin(designer, net, node_id, 1) {
        NetworkResult::Molecule(m) => {
            assert!(m.atoms.is_diff(), "pin 1 must be a diff (is_diff == true)");
            m.atoms
        }
        other => panic!(
            "expected Molecule diff on pin 1, got {:?}",
            other.infer_data_type()
        ),
    }
}

fn result_atoms(designer: &StructureDesigner, net: &str, node_id: u64) -> AtomicStructure {
    match evaluate_pin(designer, net, node_id, 0) {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!(
            "expected atomic result on pin 0, got {:?}",
            other.infer_data_type()
        ),
    }
}

/// A small bonded molecule of carbons that a rule can replace / a cutter can cut.
fn sample_molecule() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c = s.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    s.add_bond(a, b, BOND_SINGLE);
    s.add_bond(b, c, BOND_SINGLE);
    s
}

/// Replacement-rule text property from `(from, to)` atomic-number pairs.
fn replacements_prop(rules: &[(i32, i32)]) -> (&'static str, TextValue) {
    let items = rules
        .iter()
        .map(|(f, t)| TextValue::IVec2(IVec2::new(*f, *t)))
        .collect();
    ("replacements", TextValue::Array(items))
}

// ============================================================================
// atom_replace — Test 1: mandatory §3.0 roundtrip (Molecule + Crystal)
// ============================================================================

#[test]
fn atom_replace_diff_roundtrip_molecule() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let op = add_op(&mut designer, "atom_replace", value_id);
    // C → N on every carbon.
    set_props(&mut designer, net, op, vec![replacements_prop(&[(6, 7)])]);
    assert_node_diff_roundtrip(&designer, net, op, &input);
}

#[test]
fn atom_replace_diff_roundtrip_crystal() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(input.clone()));
    let op = add_op(&mut designer, "atom_replace", value_id);
    set_props(&mut designer, net, op, vec![replacements_prop(&[(6, 7)])]);
    assert_node_diff_roundtrip(&designer, net, op, &input);
}

// ============================================================================
// atom_replace — Test 1b: element swap ⇒ anchored same-position atoms, new
// element; diff count == number of replaced atoms only.
// ============================================================================

#[test]
fn atom_replace_element_swap_diff_shape() {
    let net = "test";
    // Two carbons + one oxygen; the rule only matches carbon.
    let mut input = AtomicStructure::new();
    let a = input.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = input.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c = input.add_atom(8, DVec3::new(3.0, 0.0, 0.0));
    input.add_bond(a, b, BOND_SINGLE);
    input.add_bond(b, c, BOND_SINGLE);

    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let op = add_op(&mut designer, "atom_replace", value_id);
    set_props(&mut designer, net, op, vec![replacements_prop(&[(6, 7)])]); // C → N

    let diff = diff_atoms(&designer, net, op);
    // Exactly the two carbons participate; the oxygen is untouched → absent.
    assert_eq!(
        diff.get_num_of_atoms(),
        2,
        "only replaced atoms enter the diff"
    );
    for (_, atom) in diff.iter_atoms() {
        assert_eq!(
            atom.atomic_number, 7,
            "replaced atoms carry the new element"
        );
        // Anchored at the (unchanged) base position, and the position itself is
        // unchanged (element-only edit).
        assert!(
            diff.has_anchor_position(atom.id),
            "a modified atom is anchored to its base position"
        );
        assert_eq!(
            diff.anchor_position(atom.id).copied(),
            Some(atom.position),
            "element-only replace keeps the position, so anchor == position"
        );
    }

    assert_node_diff_roundtrip(&designer, net, op, &input);
}

// ============================================================================
// atom_replace — Test 2: deletion rule (to == 0) ⇒ delete markers; roundtrip.
// ============================================================================

#[test]
fn atom_replace_deletion_rule_diff() {
    let net = "test";
    // Two carbons + one oxygen; delete all carbons (C → 0).
    let mut input = AtomicStructure::new();
    let a = input.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = input.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c = input.add_atom(8, DVec3::new(3.0, 0.0, 0.0));
    input.add_bond(a, b, BOND_SINGLE);
    input.add_bond(b, c, BOND_SINGLE);

    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let op = add_op(&mut designer, "atom_replace", value_id);
    set_props(&mut designer, net, op, vec![replacements_prop(&[(6, 0)])]); // C → delete

    let diff = diff_atoms(&designer, net, op);
    let delete_markers = diff
        .iter_atoms()
        .filter(|(_, a)| a.atomic_number == DELETED_SITE_ATOMIC_NUMBER)
        .count();
    assert_eq!(delete_markers, 2, "one delete marker per deleted carbon");

    assert_node_diff_roundtrip(&designer, net, op, &input);
}

// ============================================================================
// atom_replace — Test 3: region pin ⇒ diff contains only in-region replacements.
// ============================================================================

#[test]
fn atom_replace_region_scoped_diff() {
    let net = "test";
    // One carbon inside the region (origin), one far outside.
    let mut input = AtomicStructure::new();
    input.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    input.add_atom(6, DVec3::new(10.0, 0.0, 0.0));

    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let op = add_op(&mut designer, "atom_replace", value_id);
    set_props(&mut designer, net, op, vec![replacements_prop(&[(6, 7)])]); // C → N

    // Region: a sphere of radius 2 Å around the origin (real-space, no lattice
    // scaling for the region pin). Only the origin atom is inside.
    let region_id = add_value_node(
        &mut designer,
        net,
        blueprint_value(GeoNode::sphere(DVec3::ZERO, 2.0)),
    );
    designer.connect_nodes(region_id, 0, op, 2); // region pin index 2

    let diff = diff_atoms(&designer, net, op);
    assert_eq!(
        diff.get_num_of_atoms(),
        1,
        "only the in-region replacement enters the diff"
    );
    let atom = diff.iter_atoms().next().unwrap().1;
    assert_eq!(atom.atomic_number, 7);
    assert_eq!(atom.position, DVec3::new(0.0, 0.0, 0.0));

    assert_node_diff_roundtrip(&designer, net, op, &input);
}

// ============================================================================
// atom_cut — Test 4: mandatory §3.0 roundtrip (Molecule + Crystal) + id
// stability + one delete marker per cut atom, no cut-bond entries.
// ============================================================================

/// Builds a network `value(input) → atom_cut(cutter)` and returns (designer, cut_id).
/// The cutter keeps atoms within ~one diamond cell (~3.57 Å) of the origin.
fn build_atom_cut(input_value: NetworkResult) -> (StructureDesigner, &'static str, u64) {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, input_value);
    let cut = add_op(&mut designer, "atom_cut", value_id);
    // Cutter: unit sphere at origin. atom_cut evaluates the SDF at pos/unit_cell,
    // so an atom is kept iff |pos| <= 1 cell ≈ 3.567 Å.
    let cutter = blueprint_value(GeoNode::sphere(DVec3::ZERO, 1.0));
    let cutter_array = add_value_node(&mut designer, net, NetworkResult::Array(vec![cutter]));
    designer.connect_nodes(cutter_array, 0, cut, 1); // cutters pin index 1
    (designer, net, cut)
}

/// Two kept carbons near the origin, two cut carbons far away.
/// Bond a1-a2 straddles the cut boundary (kept→cut); bond a2-a3 is cut→cut.
fn cut_sample() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let a0 = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // keep
    let a1 = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0)); // keep
    let a2 = s.add_atom(6, DVec3::new(8.0, 0.0, 0.0)); // cut
    let a3 = s.add_atom(6, DVec3::new(9.5, 0.0, 0.0)); // cut
    s.add_bond(a0, a1, BOND_SINGLE);
    s.add_bond(a1, a2, BOND_SINGLE); // straddles the boundary
    s.add_bond(a2, a3, BOND_SINGLE); // both cut
    s
}

#[test]
fn atom_cut_diff_roundtrip_molecule() {
    let input = cut_sample();
    let (designer, net, cut) = build_atom_cut(molecule_value(input.clone()));
    assert_node_diff_roundtrip(&designer, net, cut, &input);
}

#[test]
fn atom_cut_diff_roundtrip_crystal() {
    let input = cut_sample();
    let (designer, net, cut) = build_atom_cut(crystal_value(input.clone()));
    assert_node_diff_roundtrip(&designer, net, cut, &input);
}

#[test]
fn atom_cut_diff_shape_and_id_stability() {
    let input = cut_sample();
    let (designer, net, cut) = build_atom_cut(molecule_value(input.clone()));

    // Delete-only op: the diff is exactly one delete marker per cut atom, and no
    // bond entries (cut bonds emit nothing, §1.3).
    let diff = diff_atoms(&designer, net, cut);
    assert_eq!(diff.get_num_of_atoms(), 2, "one delete marker per cut atom");
    for (_, atom) in diff.iter_atoms() {
        assert_eq!(
            atom.atomic_number, DELETED_SITE_ATOMIC_NUMBER,
            "cut atoms are delete markers"
        );
    }
    assert_eq!(
        diff.get_num_of_bonds(),
        0,
        "cut bonds produce no diff bond entries"
    );

    // §1.5 regression guard: survivors keep their `before` ids (ids 1 and 2, the
    // near-origin carbons). If this ever breaks, the id-keyed precondition broke.
    let after = result_atoms(&designer, net, cut);
    assert_eq!(after.get_num_of_atoms(), 2);
    let a0 = after.get_atom(1).expect("survivor id 1 preserved");
    let a1 = after.get_atom(2).expect("survivor id 2 preserved");
    assert_eq!(a0.position, DVec3::new(0.0, 0.0, 0.0));
    assert_eq!(a1.position, DVec3::new(1.5, 0.0, 0.0));
}

// ============================================================================
// atom_cut — Test 5: mockup → monster. The cut diff applied to a superset only
// removes the corresponding atoms.
// ============================================================================

#[test]
fn atom_cut_mockup_to_monster() {
    let mockup = cut_sample();
    let (designer, net, cut) = build_atom_cut(molecule_value(mockup.clone()));
    let diff = diff_atoms(&designer, net, cut);

    // Monster = the mockup's atoms at identical coordinates, plus far-away extra
    // atoms the cut never touched.
    let mut monster = cut_sample();
    let e0 = monster.add_atom(7, DVec3::new(-20.0, 0.0, 0.0));
    let e1 = monster.add_atom(7, DVec3::new(20.0, 5.0, 0.0));
    let extra_positions = [
        monster.get_atom(e0).unwrap().position,
        monster.get_atom(e1).unwrap().position,
    ];

    let applied = apply_diff(&monster, &diff, APPLY_TOLERANCE);
    let out = applied.result;

    // The two cut positions are gone; the extras survive.
    let has_pos = |s: &AtomicStructure, p: DVec3| {
        s.iter_atoms()
            .any(|(_, a)| (a.position - p).length() < 1e-6)
    };
    assert!(
        !has_pos(&out, DVec3::new(8.0, 0.0, 0.0)),
        "cut atom removed"
    );
    assert!(
        !has_pos(&out, DVec3::new(9.5, 0.0, 0.0)),
        "cut atom removed"
    );
    for p in extra_positions {
        assert!(has_pos(&out, p), "untouched extra atom survives");
    }
    assert!(
        has_pos(&out, DVec3::new(0.0, 0.0, 0.0)),
        "kept atom survives"
    );
    assert!(
        has_pos(&out, DVec3::new(1.5, 0.0, 0.0)),
        "kept atom survives"
    );
    assert_eq!(
        applied.stats.orphaned_tracked_atoms, 0,
        "every delete marker matched a monster atom"
    );
}
