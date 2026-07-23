//! Phase 3 of `doc/design_atom_tags.md` — the `tag` / `untag` region-gated
//! named-per-atom-group nodes. Covers, per the phase plan: region gating (all
//! atoms when disconnected, in-region only when wired); the wired `name` pin
//! overriding the stored property; chained accumulation and removal; `untag ""`
//! clearing all tags in-region; empty-name and 32-name-limit localized errors;
//! concrete-phase pass-through; non-Blueprint region error; a text-format
//! round-trip of the `name` property (including exotic names); and an
//! `atom_union` integration exercising the node-level `add_atomic_structure`
//! path.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    BlueprintData, CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

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

fn crystal_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: structure,
        geo_tree_root: None,
        alignment: Default::default(),
        alignment_reason: None,
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

/// Adds a `tag` (or `untag`) node with a stored `name` property.
fn add_tag_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_type: &str,
    pos: DVec2,
    name: &str,
) -> u64 {
    let node_id = designer.add_node(node_type, pos);
    set_name_property(designer, network_name, node_id, name);
    node_id
}

fn set_name_property(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    name: &str,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let mut props = HashMap::new();
    props.insert("name".to_string(), TextValue::String(name.to_string()));
    node.data.set_text_properties(&props).unwrap();
}

fn evaluate(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
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

fn evaluate_to_atomic(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
    match evaluate(designer, network_name, node_id) {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(e) => panic!("Expected Atomic result, got Error: {}", e),
        other => panic!("Expected Atomic result, got {:?}", other.infer_data_type()),
    }
}

/// A line of carbons along +x at the given x coordinates.
fn carbons_at(xs: &[f64]) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    for &x in xs {
        s.add_atom(6, DVec3::new(x, 0.0, 0.0));
    }
    s
}

/// Tag names carried by the atom nearest `x`, in bit order.
fn tags_at(s: &AtomicStructure, x: f64) -> Vec<String> {
    let (id, _) = s
        .iter_atoms()
        .find(|(_, a)| (a.position.x - x).abs() < 1e-6)
        .unwrap_or_else(|| panic!("no atom near x={}", x));
    s.atom_tags(*id)
        .into_iter()
        .map(|t| t.to_string())
        .collect()
}

/// Half-space whose in-region (`sdf ≤ margin`) side is `x ≤ 0`.
fn region_x_le_0() -> NetworkResult {
    blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO))
}

// ============================================================================
// Region gating
// ============================================================================

/// Disconnected `region` → every atom is tagged.
#[test]
fn tag_region_disconnected_tags_all() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let tag_id = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "surface");
    designer.connect_nodes(value_id, 0, tag_id, 0);

    let result = evaluate_to_atomic(&designer, net, tag_id);
    assert_eq!(tags_at(&result, -1.0), vec!["surface"], "both atoms tagged");
    assert_eq!(tags_at(&result, 1.0), vec!["surface"], "both atoms tagged");
}

/// With a half-space region, only in-region atoms are tagged.
#[test]
fn tag_region_tags_only_in_region() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let tag_id = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "left");
    designer.connect_nodes(value_id, 0, tag_id, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, tag_id, 2);

    let result = evaluate_to_atomic(&designer, net, tag_id);
    assert_eq!(
        tags_at(&result, -1.0),
        vec!["left"],
        "in-region atom tagged"
    );
    assert!(
        tags_at(&result, 1.0).is_empty(),
        "out-of-region atom untagged"
    );
}

// ============================================================================
// Wired name pin overrides stored property
// ============================================================================

/// A wired `name` pin (pin 1) overrides the stored property.
#[test]
fn tag_wired_name_overrides_property() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let tag_id = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "stored");
    designer.connect_nodes(value_id, 0, tag_id, 0);
    let name_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        NetworkResult::String("wired".to_string()),
    );
    designer.connect_nodes(name_id, 0, tag_id, 1);

    let result = evaluate_to_atomic(&designer, net, tag_id);
    assert_eq!(tags_at(&result, 0.0), vec!["wired"], "wired name wins");
}

// ============================================================================
// Composition — chaining, untag, blanket clear
// ============================================================================

/// Chained `tag "a"` → `tag "b"` accumulates both on the overlap; `untag "a"`
/// removes only `a`.
#[test]
fn tag_chain_accumulates_and_untag_removes_one() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let tag_a = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "a");
    designer.connect_nodes(value_id, 0, tag_a, 0);
    let tag_b = add_tag_node(&mut designer, net, "tag", DVec2::new(400.0, 0.0), "b");
    designer.connect_nodes(tag_a, 0, tag_b, 0);

    let result = evaluate_to_atomic(&designer, net, tag_b);
    assert_eq!(
        tags_at(&result, 0.0),
        vec!["a", "b"],
        "both tags accumulate"
    );

    // untag "a" removes only a.
    let untag_a = add_tag_node(&mut designer, net, "untag", DVec2::new(600.0, 0.0), "a");
    designer.connect_nodes(tag_b, 0, untag_a, 0);
    let result = evaluate_to_atomic(&designer, net, untag_a);
    assert_eq!(tags_at(&result, 0.0), vec!["b"], "only a removed");
}

/// `untag ""` (empty name) clears all tags on in-region atoms and leaves
/// out-of-region tags alone.
#[test]
fn untag_empty_clears_all_in_region() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    // Tag both atoms with "a" and "b".
    let tag_a = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "a");
    designer.connect_nodes(value_id, 0, tag_a, 0);
    let tag_b = add_tag_node(&mut designer, net, "tag", DVec2::new(400.0, 0.0), "b");
    designer.connect_nodes(tag_a, 0, tag_b, 0);

    // untag "" only in x ≤ 0.
    let untag_all = add_tag_node(&mut designer, net, "untag", DVec2::new(600.0, 0.0), "");
    designer.connect_nodes(tag_b, 0, untag_all, 0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, untag_all, 2);

    let result = evaluate_to_atomic(&designer, net, untag_all);
    assert!(
        tags_at(&result, -1.0).is_empty(),
        "in-region atom cleared of all tags"
    );
    assert_eq!(
        tags_at(&result, 1.0),
        vec!["a", "b"],
        "out-of-region atom keeps its tags"
    );
}

/// Re-tagging an already-tagged atom is a no-op (idempotent); `untag` of an
/// absent name is a no-op too.
#[test]
fn tag_is_idempotent_and_untag_absent_is_noop() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let tag1 = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "x");
    designer.connect_nodes(value_id, 0, tag1, 0);
    let tag2 = add_tag_node(&mut designer, net, "tag", DVec2::new(400.0, 0.0), "x");
    designer.connect_nodes(tag1, 0, tag2, 0);
    // Untag a name the atom never carried.
    let untag = add_tag_node(
        &mut designer,
        net,
        "untag",
        DVec2::new(600.0, 0.0),
        "absent",
    );
    designer.connect_nodes(tag2, 0, untag, 0);

    let result = evaluate_to_atomic(&designer, net, untag);
    assert_eq!(tags_at(&result, 0.0), vec!["x"], "single tag, no duplicate");
}

// ============================================================================
// Errors
// ============================================================================

/// Empty name on `tag` → localized error.
#[test]
fn tag_empty_name_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let tag_id = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "   ");
    designer.connect_nodes(value_id, 0, tag_id, 0);

    match evaluate(&designer, net, tag_id) {
        NetworkResult::Error(msg) => {
            assert!(msg.contains("tag"), "error mentions the node: {}", msg);
            assert!(msg.contains("empty"), "error mentions empty name: {}", msg);
        }
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}

/// 33 distinct live tag names through chained `tag` nodes → localized error on
/// the offending node; upstream (32-name) result is unaffected.
#[test]
fn tag_limit_errors_on_offending_node() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );

    // Chain 33 tag nodes, each adding a distinct name to the single atom.
    let mut prev = value_id;
    let mut nodes = Vec::new();
    for i in 0..33 {
        let id = add_tag_node(
            &mut designer,
            net,
            "tag",
            DVec2::new(200.0 + i as f64 * 50.0, 0.0),
            &format!("t{}", i),
        );
        designer.connect_nodes(prev, 0, id, 0);
        nodes.push(id);
        prev = id;
    }

    // The 32nd node (index 31) succeeds with 32 live names.
    let result = evaluate_to_atomic(&designer, net, nodes[31]);
    assert_eq!(tags_at(&result, 0.0).len(), 32, "32 distinct names fit");

    // The 33rd node (index 32) fails with a localized limit error.
    match evaluate(&designer, net, nodes[32]) {
        NetworkResult::Error(msg) => {
            assert!(msg.contains("tag"), "error mentions the node: {}", msg);
            assert!(msg.contains("limit"), "error mentions the limit: {}", msg);
        }
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}

/// Non-Blueprint on the `region` pin → localized `NetworkResult::Error`.
#[test]
fn tag_non_blueprint_region_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let tag_id = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "x");
    designer.connect_nodes(value_id, 0, tag_id, 0);
    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        molecule_value(carbons_at(&[5.0])),
    );
    designer.connect_nodes(region_id, 0, tag_id, 2);

    match evaluate(&designer, net, tag_id) {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains("tag.region"),
                "error localized to tag.region: {}",
                msg
            );
        }
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}

// ============================================================================
// Phase pass-through
// ============================================================================

/// Crystal in → Crystal out; Molecule in → Molecule out.
#[test]
fn tag_concrete_phase_passes_through() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    let crystal_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        crystal_value(carbons_at(&[0.0])),
    );
    let tag_c = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "c");
    designer.connect_nodes(crystal_id, 0, tag_c, 0);
    match evaluate(&designer, net, tag_c) {
        NetworkResult::Crystal(c) => {
            assert_eq!(c.atoms.atom_tags(1), vec!["c"], "tag recorded on crystal");
        }
        other => panic!(
            "Crystal in must come out Crystal, got {:?}",
            other.infer_data_type()
        ),
    }

    let molecule_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        molecule_value(carbons_at(&[0.0])),
    );
    let tag_m = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 200.0), "m");
    designer.connect_nodes(molecule_id, 0, tag_m, 0);
    match evaluate(&designer, net, tag_m) {
        NetworkResult::Molecule(m) => {
            assert_eq!(m.atoms.atom_tags(1), vec!["m"], "tag recorded on molecule");
        }
        other => panic!(
            "Molecule in must come out Molecule, got {:?}",
            other.infer_data_type()
        ),
    }
}

// ============================================================================
// atom_union integration (node-level add_atomic_structure path)
// ============================================================================

/// A `crystal_value` fed through `exit_structure` gives the tag wire a concrete
/// `Molecule` output type (the `value` stub alone is typed `None`, which the
/// polymorphic `SameAsInput` can't resolve for `atom_union`'s single→array
/// broadcast — real concrete-typed sources are the point here).
fn add_tagged_molecule(
    designer: &mut StructureDesigner,
    net: &str,
    y: f64,
    x: f64,
    tag_name: &str,
) -> u64 {
    let val = add_value_node(
        designer,
        net,
        DVec2::new(0.0, y),
        crystal_value(carbons_at(&[x])),
    );
    let exit = designer.add_node("exit_structure", DVec2::new(200.0, y));
    designer.connect_nodes(val, 0, exit, 0);
    let tag = add_tag_node(designer, net, "tag", DVec2::new(400.0, y), tag_name);
    designer.connect_nodes(exit, 0, tag, 0);
    tag
}

/// Two molecules tagged with **different** names at the same bit position
/// (each structure interns its own name into bit 0) union to a structure that
/// carries the correct name on each atom.
#[test]
fn atom_union_merges_distinct_tags_by_name() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    let tag_a = add_tagged_molecule(&mut designer, net, 0.0, -1.0, "alpha");
    let tag_b = add_tagged_molecule(&mut designer, net, 200.0, 1.0, "beta");

    let union_id = designer.add_node("atom_union", DVec2::new(600.0, 100.0));
    designer.connect_nodes(tag_a, 0, union_id, 0);
    designer.connect_nodes(tag_b, 0, union_id, 0);

    let result = evaluate_to_atomic(&designer, net, union_id);
    assert_eq!(
        tags_at(&result, -1.0),
        vec!["alpha"],
        "first atom keeps its name"
    );
    assert_eq!(
        tags_at(&result, 1.0),
        vec!["beta"],
        "second atom keeps its name (remapped bit)"
    );
}

/// Two molecules tagged with the **same** name union to a structure where both
/// atoms carry it (name-level union, one shared bit).
#[test]
fn atom_union_merges_shared_tag() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    let tag_a = add_tagged_molecule(&mut designer, net, 0.0, -1.0, "shared");
    let tag_b = add_tagged_molecule(&mut designer, net, 200.0, 1.0, "shared");

    let union_id = designer.add_node("atom_union", DVec2::new(600.0, 100.0));
    designer.connect_nodes(tag_a, 0, union_id, 0);
    designer.connect_nodes(tag_b, 0, union_id, 0);

    let result = evaluate_to_atomic(&designer, net, union_id);
    assert_eq!(tags_at(&result, -1.0), vec!["shared"]);
    assert_eq!(tags_at(&result, 1.0), vec!["shared"]);
}

// ============================================================================
// Text-format round-trip of the name property (incl. exotic names)
// ============================================================================

/// The `name` property round-trips through the text format, including names
/// with spaces, quotes, and non-ASCII characters — the first free-form
/// user-authored `TextValue::String` node property to reach the serializer.
#[test]
fn tag_name_text_format_roundtrip() {
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
    use rust_lib_flutter_cad::structure_designer::text_format::{edit_network, serialize_network};

    // A fresh registry knows the `tag` / `untag` node types (registered as
    // built-ins); author into a standalone empty network.
    let registry = NodeTypeRegistry::new();

    let source = r#"
        t1 = tag { name: "active-site" }
        t2 = tag { name: "with spaces and \"quotes\"" }
        t3 = untag { name: "非ASCII-café" }
        t4 = untag { name: "" }
    "#;

    let mut network = make_empty_network();
    let result = edit_network(&mut network, &registry, source, true);
    assert!(result.success, "initial edit succeeds: {:?}", result.errors);
    assert_eq!(network.nodes.len(), 4);

    let serialized = serialize_network(&network, &registry, Some("test"));

    // Re-author from the serialized text and confirm it re-serializes identically.
    let mut network2 = make_empty_network();
    let result2 = edit_network(&mut network2, &registry, &serialized, true);
    assert!(
        result2.success,
        "round-trip edit succeeds: {:?}",
        result2.errors
    );
    let reserialized = serialize_network(&network2, &registry, Some("test"));
    assert_eq!(serialized, reserialized, "text round-trip is stable");
}
// ============================================================================
// Phase 5 — StructureDesigner-level setter (behind get/set_tag_data) is
// undoable/redoable
// ============================================================================

/// Setting `TagData` via the shared `set_node_network_data_scoped` seam (which
/// backs the `set_tag_data` FRB wrapper) re-evaluates with the new stored name
/// and is undoable/redoable — the "persisted mutations must be undoable" rule.
#[test]
fn tag_set_data_is_undoable() {
    use rust_lib_flutter_cad::structure_designer::nodes::tag::TagData;
    use std::cell::RefCell;

    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let tag_id = add_tag_node(&mut designer, net, "tag", DVec2::new(200.0, 0.0), "surface");
    designer.connect_nodes(value_id, 0, tag_id, 0);

    let before = evaluate_to_atomic(&designer, net, tag_id);
    assert_eq!(
        tags_at(&before, 0.0),
        vec!["surface"],
        "initial stored name"
    );

    // Set new node data through the StructureDesigner-level setter.
    designer.set_node_network_data_scoped(
        &[],
        tag_id,
        Box::new(TagData {
            name: "active-site".to_string(),
            available_tags: RefCell::new(Vec::new()),
        }),
    );
    let after = evaluate_to_atomic(&designer, net, tag_id);
    assert_eq!(
        tags_at(&after, 0.0),
        vec!["active-site"],
        "setter re-evaluates with the new name"
    );

    assert!(designer.undo(), "undo should report a change");
    let undone = evaluate_to_atomic(&designer, net, tag_id);
    assert_eq!(
        tags_at(&undone, 0.0),
        vec!["surface"],
        "undo restores the previous name"
    );

    assert!(designer.redo(), "redo should report a change");
    let redone = evaluate_to_atomic(&designer, net, tag_id);
    assert_eq!(
        tags_at(&redone, 0.0),
        vec!["active-site"],
        "redo re-applies the name"
    );
}

/// Builds an empty custom network to author test nodes into.
fn make_empty_network() -> rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork {
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    use rust_lib_flutter_cad::structure_designer::data_type::DataType;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, OutputPinDefinition};

    let node_type = NodeType {
        name: "test".to_string(),
        description: "Test network".to_string(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Molecule),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {})
        },
        node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
        node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
    };
    NodeNetwork::new(node_type)
}
