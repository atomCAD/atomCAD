//! Phase 2 tests for the `apply_style` node and the `StyleRule` built-in
//! record def. See `doc/design_style_rules.md` §Phase 2.
//!
//! `apply_style` is a `HasAtoms`-polymorphic metadata-only pass-through: it
//! reads an `Array[Record(Named("StyleRule"))]` from its optional `rules` pin
//! and writes per-atom color/alpha decorator overrides on matched atoms. It has
//! no stored properties (rules are wire-only).
//!
//! What we verify here:
//! - Built-in def: `StyleRule` resolves via `lookup_record_type_def`; every
//!   registry mutation of it (add/delete/rename/update) is rejected, as is
//!   creating a user type named `StyleRule`.
//! - Pass-through: unwired `rules` and a wired empty array both leave the
//!   decorator untouched; Crystal→Crystal and Molecule→Molecule preserve the
//!   concrete variant.
//! - Matching: element-only, tag-only, AND, and match-all (no selectors),
//!   including tags applied upstream by a `tag` node (the end-to-end pipeline).
//! - Ordering: per-property last-writer-wins across two overlapping rules.
//! - Alpha: clamps; `alpha == 1.0` removes an entry set by an upstream `xray`
//!   (shared decorator field composition).
//! - Errors: non-array pin, element outside `i16`, empty/whitespace tag — each
//!   names the rule index. Unknown tag name and unmatched element match nothing
//!   without error.
//!
//! Phase 4 (`doc/design_style_rules.md` §Phase 4) adds the `render_style`
//! field: setting `"ball_and_stick"` / `"space_filling"` writes the per-atom
//! override, `"default"` clears one, an invalid string errors naming it, and
//! the non-serialized def growth exposes the new `record_construct` pin.

use glam::Vec3;
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{AtomRenderStyle, AtomicStructure};
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef, RecordTypeDefError,
};
use rust_lib_flutter_cad::structure_designer::nodes::tag::TagData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::nodes::xray::XrayData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::cell::RefCell;

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

fn add_apply_style_node(designer: &mut StructureDesigner) -> u64 {
    designer.add_node("apply_style", DVec2::new(200.0, 0.0))
}

fn add_xray_node(designer: &mut StructureDesigner, network_name: &str, alpha: f64) -> u64 {
    let xray_id = designer.add_node("xray", DVec2::new(100.0, 0.0));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.set_node_network_data(
        xray_id,
        Box::new(XrayData {
            alpha,
            opaque_depth: 0.0,
        }),
    );
    xray_id
}

fn add_tag_node(designer: &mut StructureDesigner, network_name: &str, name: &str) -> u64 {
    let tag_id = designer.add_node("tag", DVec2::new(100.0, 0.0));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.set_node_network_data(
        tag_id,
        Box::new(TagData {
            name: name.to_string(),
            available_tags: RefCell::new(Vec::new()),
        }),
    );
    tag_id
}

fn evaluate(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
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

/// A simple linear molecule: 2 C atoms + 1 O atom (ids 1, 2, 3).
fn carbon_oxygen_structure() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    let o = s.add_atom(8, DVec3::new(3.0, 0.0, 0.0));
    s.add_bond(c1, c2, BOND_SINGLE);
    s.add_bond(c2, o, BOND_SINGLE);
    s
}

fn molecule_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: structure,
        geo_tree_root: None,
    })
}

/// Build a `StyleRule` record with only the fields provided (absent = unset).
fn style_rule(
    element: Option<i32>,
    tag: Option<&str>,
    color: Option<DVec3>,
    alpha: Option<f64>,
) -> NetworkResult {
    let mut fields = Vec::new();
    if let Some(e) = element {
        fields.push(("element".to_string(), NetworkResult::Int(e)));
    }
    if let Some(t) = tag {
        fields.push(("tag".to_string(), NetworkResult::String(t.to_string())));
    }
    if let Some(c) = color {
        fields.push(("color".to_string(), NetworkResult::Vec3(c)));
    }
    if let Some(a) = alpha {
        fields.push(("alpha".to_string(), NetworkResult::Float(a)));
    }
    NetworkResult::record(fields)
}

/// Like `style_rule`, but also carries the Phase-4 `render_style` field when
/// `render_style` is `Some` (absent = leave the atom's render style alone).
fn style_rule_rs(
    element: Option<i32>,
    tag: Option<&str>,
    color: Option<DVec3>,
    alpha: Option<f64>,
    render_style: Option<&str>,
) -> NetworkResult {
    let mut fields = Vec::new();
    if let Some(e) = element {
        fields.push(("element".to_string(), NetworkResult::Int(e)));
    }
    if let Some(t) = tag {
        fields.push(("tag".to_string(), NetworkResult::String(t.to_string())));
    }
    if let Some(c) = color {
        fields.push(("color".to_string(), NetworkResult::Vec3(c)));
    }
    if let Some(a) = alpha {
        fields.push(("alpha".to_string(), NetworkResult::Float(a)));
    }
    if let Some(rs) = render_style {
        fields.push((
            "render_style".to_string(),
            NetworkResult::String(rs.to_string()),
        ));
    }
    NetworkResult::record(fields)
}

fn rules_array(rules: Vec<NetworkResult>) -> NetworkResult {
    NetworkResult::Array(rules)
}

/// Wire `molecule → apply_style.0` and `rules → apply_style.1`, then evaluate.
fn eval_apply_style(
    designer: &mut StructureDesigner,
    net: &str,
    molecule: NetworkResult,
    rules: NetworkResult,
) -> AtomicStructure {
    let mol_id = add_value_node(designer, net, molecule);
    let rules_id = add_value_node(designer, net, rules);
    let style_id = add_apply_style_node(designer);
    designer.connect_nodes(mol_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);
    evaluate_to_atomic(designer, net, style_id)
}

// ============================================================================
// Built-in def: StyleRule
// ============================================================================

#[test]
fn style_rule_resolves_via_lookup() {
    let registry = NodeTypeRegistry::new();
    let def = registry
        .lookup_record_type_def("StyleRule")
        .expect("StyleRule should resolve via built_in_record_type_defs");
    assert_eq!(def.name, "StyleRule");
    let fields: Vec<(String, DataType)> = def
        .fields
        .iter()
        .map(|f| (f.name.clone(), f.data_type.clone()))
        .collect();
    assert_eq!(
        fields,
        vec![
            (
                "element".to_string(),
                DataType::Optional(Box::new(DataType::Int))
            ),
            (
                "tag".to_string(),
                DataType::Optional(Box::new(DataType::String))
            ),
            (
                "color".to_string(),
                DataType::Optional(Box::new(DataType::Vec3))
            ),
            (
                "alpha".to_string(),
                DataType::Optional(Box::new(DataType::Float))
            ),
            (
                "render_style".to_string(),
                DataType::Optional(Box::new(DataType::String))
            ),
        ]
    );
}

#[test]
fn style_rule_name_is_taken() {
    let registry = NodeTypeRegistry::new();
    assert!(registry.name_is_taken("StyleRule"));
    assert!(registry.is_built_in_record_type_def("StyleRule"));
}

#[test]
fn style_rule_mutation_guards() {
    let mut registry = NodeTypeRegistry::new();
    assert!(matches!(
        registry
            .add_record_type_def(RecordTypeDef::new("StyleRule".to_string()))
            .unwrap_err(),
        RecordTypeDefError::BuiltIn(ref s) if s == "StyleRule"
    ));
    // Delete is a no-op that leaves the def resolvable.
    assert!(registry.delete_record_type_def("StyleRule").is_none());
    assert!(registry.lookup_record_type_def("StyleRule").is_some());
    assert!(matches!(
        registry
            .rename_record_type_def("StyleRule", "MyStyle")
            .unwrap_err(),
        RecordTypeDefError::BuiltIn(_)
    ));
    assert!(matches!(
        registry
            .update_record_type_def("StyleRule", vec![])
            .unwrap_err(),
        RecordTypeDefError::BuiltIn(_)
    ));
}

#[test]
fn add_node_network_named_style_rule_rejected_by_namespace() {
    let designer = StructureDesigner::default();
    assert!(designer.node_type_registry.name_is_taken("StyleRule"));
}

// ============================================================================
// Node signature
// ============================================================================

#[test]
fn apply_style_pin_signature() {
    let registry = NodeTypeRegistry::new();
    let nt = registry
        .get_node_type("apply_style")
        .expect("apply_style registered");
    assert_eq!(nt.parameters.len(), 2);
    assert_eq!(nt.parameters[0].name, "molecule");
    assert_eq!(nt.parameters[0].data_type, DataType::HasAtoms);
    assert_eq!(nt.parameters[1].name, "rules");
    assert_eq!(
        nt.parameters[1].data_type,
        DataType::Array(Box::new(DataType::Record(RecordType::Named(
            "StyleRule".to_string()
        ))))
    );
    // Single output, no diff pin.
    assert_eq!(nt.output_pins.len(), 1);
}

// ============================================================================
// Pass-through
// ============================================================================

#[test]
fn apply_style_unwired_rules_passes_through() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let style_id = add_apply_style_node(&mut designer);
    designer.connect_nodes(mol_id, 0, style_id, 0);

    let result = evaluate_to_atomic(&designer, net, style_id);
    // No rules → decorator untouched.
    for id in [1, 2, 3] {
        assert_eq!(result.get_atom_color(id), None);
        assert_eq!(result.get_atom_alpha(id), 1.0);
    }
    assert_eq!(result.atom_ids().count(), 3);
}

#[test]
fn apply_style_empty_array_passes_through() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![]),
    );
    for id in [1, 2, 3] {
        assert_eq!(result.get_atom_color(id), None);
        assert_eq!(result.get_atom_alpha(id), 1.0);
    }
}

#[test]
fn apply_style_preserves_crystal_variant() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let crystal = NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: carbon_oxygen_structure(),
        geo_tree_root: None,
        alignment: Default::default(),
        alignment_reason: None,
    });
    let mol_id = add_value_node(&mut designer, net, crystal);
    // Match-all rule sets a color.
    let rules_id = add_value_node(
        &mut designer,
        net,
        rules_array(vec![style_rule(
            None,
            None,
            Some(DVec3::new(1.0, 0.0, 0.0)),
            None,
        )]),
    );
    let style_id = add_apply_style_node(&mut designer);
    designer.connect_nodes(mol_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);

    match evaluate(&designer, net, style_id) {
        NetworkResult::Crystal(c) => {
            assert_eq!(c.atoms.get_atom_color(1), Some(Vec3::new(1.0, 0.0, 0.0)));
        }
        other => panic!("Expected Crystal, got {:?}", other.infer_data_type()),
    }
}

// ============================================================================
// Matching
// ============================================================================

#[test]
fn apply_style_element_only_selector() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Color carbons (element 6) red; oxygen untouched.
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![style_rule(
            Some(6),
            None,
            Some(DVec3::new(1.0, 0.0, 0.0)),
            None,
        )]),
    );
    assert_eq!(result.get_atom_color(1), Some(Vec3::new(1.0, 0.0, 0.0)));
    assert_eq!(result.get_atom_color(2), Some(Vec3::new(1.0, 0.0, 0.0)));
    assert_eq!(result.get_atom_color(3), None); // oxygen
}

#[test]
fn apply_style_match_all_no_selectors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![style_rule(None, None, None, Some(0.4))]),
    );
    for id in [1, 2, 3] {
        assert_eq!(result.get_atom_alpha(id), 0.4);
    }
}

#[test]
fn apply_style_tag_only_selector_direct() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Tag only the oxygen atom directly on the structure.
    let mut structure = carbon_oxygen_structure();
    structure.add_atom_tag(3, "special").unwrap();
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(structure),
        rules_array(vec![style_rule(
            None,
            Some("special"),
            Some(DVec3::new(0.0, 1.0, 0.0)),
            None,
        )]),
    );
    assert_eq!(result.get_atom_color(1), None);
    assert_eq!(result.get_atom_color(2), None);
    assert_eq!(result.get_atom_color(3), Some(Vec3::new(0.0, 1.0, 0.0)));
}

#[test]
fn apply_style_element_and_tag_and() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Tag one carbon (id 1) and the oxygen (id 3). Rule matches element==6 AND
    // tag=="hot" ⇒ only atom 1.
    let mut structure = carbon_oxygen_structure();
    structure.add_atom_tag(1, "hot").unwrap();
    structure.add_atom_tag(3, "hot").unwrap();
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(structure),
        rules_array(vec![style_rule(
            Some(6),
            Some("hot"),
            Some(DVec3::new(0.0, 0.0, 1.0)),
            None,
        )]),
    );
    assert_eq!(result.get_atom_color(1), Some(Vec3::new(0.0, 0.0, 1.0)));
    assert_eq!(result.get_atom_color(2), None); // carbon, not tagged
    assert_eq!(result.get_atom_color(3), None); // tagged, but oxygen
}

#[test]
fn apply_style_tag_from_upstream_tag_node() {
    // End-to-end: a `tag` node tags the whole structure "grp", then
    // `apply_style` colors atoms carrying that tag. This is the pipeline the
    // feature exists for.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let tag_id = add_tag_node(&mut designer, net, "grp");
    let rules_id = add_value_node(
        &mut designer,
        net,
        rules_array(vec![style_rule(
            None,
            Some("grp"),
            Some(DVec3::new(1.0, 1.0, 0.0)),
            None,
        )]),
    );
    let style_id = add_apply_style_node(&mut designer);
    designer.connect_nodes(mol_id, 0, tag_id, 0);
    designer.connect_nodes(tag_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);

    let result = evaluate_to_atomic(&designer, net, style_id);
    for id in [1, 2, 3] {
        assert_eq!(result.get_atom_color(id), Some(Vec3::new(1.0, 1.0, 0.0)));
    }
}

// ============================================================================
// Ordering: per-property last-writer-wins
// ============================================================================

#[test]
fn apply_style_ordered_per_property_last_writer_wins() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Rule 1 sets color+alpha on carbons; rule 2 sets only color on carbons.
    // Overlap ⇒ rule 2's color, rule 1's alpha.
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![
            style_rule(Some(6), None, Some(DVec3::new(1.0, 0.0, 0.0)), Some(0.5)),
            style_rule(Some(6), None, Some(DVec3::new(0.0, 0.0, 1.0)), None),
        ]),
    );
    // Carbons: color = rule 2 (blue), alpha = rule 1 (0.5).
    assert_eq!(result.get_atom_color(1), Some(Vec3::new(0.0, 0.0, 1.0)));
    assert_eq!(result.get_atom_alpha(1), 0.5);
    assert_eq!(result.get_atom_color(2), Some(Vec3::new(0.0, 0.0, 1.0)));
    assert_eq!(result.get_atom_alpha(2), 0.5);
    // Oxygen untouched.
    assert_eq!(result.get_atom_color(3), None);
    assert_eq!(result.get_atom_alpha(3), 1.0);
}

// ============================================================================
// Alpha semantics
// ============================================================================

#[test]
fn apply_style_alpha_clamps_low() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![style_rule(Some(8), None, None, Some(-0.5))]),
    );
    // Negative alpha clamps to 0.0 on the oxygen.
    assert_eq!(result.get_atom_alpha(3), 0.0);
}

#[test]
fn apply_style_alpha_one_removes_upstream_xray_entry() {
    // xray ghosts every atom to 0.5; apply_style's `alpha: 1.0` restores full
    // opacity on the shared decorator field (last writer wins).
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let xray_id = add_xray_node(&mut designer, net, 0.5);
    let rules_id = add_value_node(
        &mut designer,
        net,
        rules_array(vec![style_rule(None, None, None, Some(1.0))]),
    );
    let style_id = add_apply_style_node(&mut designer);
    designer.connect_nodes(mol_id, 0, xray_id, 0);
    designer.connect_nodes(xray_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);

    // Sanity: after xray alone, atoms are ghosted.
    let ghosted = evaluate_to_atomic(&designer, net, xray_id);
    assert_eq!(ghosted.get_atom_alpha(1), 0.5);

    // After apply_style alpha 1.0, opacity restored.
    let result = evaluate_to_atomic(&designer, net, style_id);
    for id in [1, 2, 3] {
        assert_eq!(result.get_atom_alpha(id), 1.0);
    }
}

// ============================================================================
// Errors and no-error non-matches
// ============================================================================

#[test]
fn apply_style_non_array_rules_errors() {
    // A non-array value on the `rules` pin is rejected. The declared pin type
    // `Array[Record(StyleRule)]` means the wire-layer conversion catches a raw
    // `Int` before eval; either way the node produces an `Error` rather than
    // silently ignoring the malformed input (the eval-side `other =>` guard is
    // the belt-and-suspenders fallback for values that pass conversion but
    // aren't arrays).
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let rules_id = add_value_node(&mut designer, net, NetworkResult::Int(7));
    let style_id = add_apply_style_node(&mut designer);
    designer.connect_nodes(mol_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);

    assert!(
        matches!(evaluate(&designer, net, style_id), NetworkResult::Error(_)),
        "non-array rules input must yield an Error"
    );
}

#[test]
fn apply_style_element_out_of_i16_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let rules_id = add_value_node(
        &mut designer,
        net,
        rules_array(vec![style_rule(Some(40_000), None, None, Some(0.5))]),
    );
    let style_id = add_apply_style_node(&mut designer);
    designer.connect_nodes(mol_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);

    let NetworkResult::Error(msg) = evaluate(&designer, net, style_id) else {
        panic!("expected Error");
    };
    assert!(
        msg.contains("rules[0]") && msg.contains("40000") && msg.contains("out of range"),
        "got: {}",
        msg
    );
}

#[test]
fn apply_style_empty_tag_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    // A second rule (index 1) has a whitespace-only tag.
    let rules_id = add_value_node(
        &mut designer,
        net,
        rules_array(vec![
            style_rule(Some(6), None, None, Some(0.5)),
            style_rule(None, Some("   "), None, Some(0.5)),
        ]),
    );
    let style_id = add_apply_style_node(&mut designer);
    designer.connect_nodes(mol_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);

    let NetworkResult::Error(msg) = evaluate(&designer, net, style_id) else {
        panic!("expected Error");
    };
    assert!(
        msg.contains("rules[1]") && msg.contains("empty"),
        "got: {}",
        msg
    );
}

#[test]
fn apply_style_unknown_tag_and_unmatched_element_no_error() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Rule 1: tag "ghost" absent from the structure's table → matches nothing.
    // Rule 2: element 79 (gold) carried by no atom → matches nothing.
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![
            style_rule(None, Some("ghost"), Some(DVec3::new(1.0, 0.0, 0.0)), None),
            style_rule(Some(79), None, Some(DVec3::new(1.0, 0.0, 0.0)), None),
        ]),
    );
    // No error, and no atom styled.
    for id in [1, 2, 3] {
        assert_eq!(result.get_atom_color(id), None);
    }
}

// ============================================================================
// Phase 4: render_style
// ============================================================================

#[test]
fn apply_style_render_style_sets_override() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Space-fill the oxygen (element 8); carbons keep the global default.
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![style_rule_rs(
            Some(8),
            None,
            None,
            None,
            Some("space_filling"),
        )]),
    );
    assert_eq!(result.get_atom_render_style(1), None); // carbon
    assert_eq!(result.get_atom_render_style(2), None); // carbon
    assert_eq!(
        result.get_atom_render_style(3),
        Some(AtomRenderStyle::SpaceFilling)
    );
}

#[test]
fn apply_style_render_style_ball_and_stick_variant() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![style_rule_rs(
            Some(6),
            None,
            None,
            None,
            Some("ball_and_stick"),
        )]),
    );
    assert_eq!(
        result.get_atom_render_style(1),
        Some(AtomRenderStyle::BallAndStick)
    );
    assert_eq!(
        result.get_atom_render_style(2),
        Some(AtomRenderStyle::BallAndStick)
    );
    assert_eq!(result.get_atom_render_style(3), None); // oxygen
}

#[test]
fn apply_style_render_style_default_clears_earlier_override() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Rule 1 space-fills all atoms; rule 2 resets the carbons to "default".
    // Overlap ⇒ carbons cleared, oxygen keeps space-filling.
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![
            style_rule_rs(None, None, None, None, Some("space_filling")),
            style_rule_rs(Some(6), None, None, None, Some("default")),
        ]),
    );
    assert_eq!(result.get_atom_render_style(1), None); // carbon, cleared
    assert_eq!(result.get_atom_render_style(2), None); // carbon, cleared
    assert_eq!(
        result.get_atom_render_style(3), // oxygen, still space-filling
        Some(AtomRenderStyle::SpaceFilling)
    );
}

#[test]
fn apply_style_render_style_invalid_string_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let rules_id = add_value_node(
        &mut designer,
        net,
        rules_array(vec![style_rule_rs(
            None,
            None,
            None,
            None,
            Some("wireframe"),
        )]),
    );
    let style_id = add_apply_style_node(&mut designer);
    designer.connect_nodes(mol_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);

    let NetworkResult::Error(msg) = evaluate(&designer, net, style_id) else {
        panic!("expected Error");
    };
    assert!(
        msg.contains("rules[0]") && msg.contains("render_style") && msg.contains("wireframe"),
        "got: {}",
        msg
    );
}

#[test]
fn apply_style_render_style_end_to_end_dopant() {
    // The headline scenario: tag an interior atom, then style that tag
    // space-filling + colored. Verify the render-style override lands (the
    // display-seam tessellation coverage lives in the display crate; here we
    // confirm the node writes the decorator through the tag pipeline).
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mut structure = carbon_oxygen_structure();
    structure.add_atom_tag(2, "dopant").unwrap();
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(structure),
        rules_array(vec![style_rule_rs(
            None,
            Some("dopant"),
            Some(DVec3::new(1.0, 0.0, 1.0)),
            None,
            Some("space_filling"),
        )]),
    );
    // Only the tagged atom is restyled and recolored.
    assert_eq!(result.get_atom_render_style(1), None);
    assert_eq!(
        result.get_atom_render_style(2),
        Some(AtomRenderStyle::SpaceFilling)
    );
    assert_eq!(result.get_atom_color(2), Some(Vec3::new(1.0, 0.0, 1.0)));
    assert_eq!(result.get_atom_render_style(3), None);
}

#[test]
fn apply_style_render_style_wrong_type_errors() {
    // A non-String value on `render_style` (e.g. an Int) → localized error.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let rules_id = add_value_node(
        &mut designer,
        net,
        rules_array(vec![NetworkResult::record(vec![(
            "render_style".to_string(),
            NetworkResult::Int(3),
        )])]),
    );
    let style_id = add_apply_style_node(&mut designer);
    designer.connect_nodes(mol_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);

    let NetworkResult::Error(msg) = evaluate(&designer, net, style_id) else {
        panic!("expected Error");
    };
    assert!(
        msg.contains("rules[0]") && msg.contains("render_style"),
        "got: {}",
        msg
    );
}

#[test]
fn record_construct_style_rule_exposes_render_style_pin() {
    // Adding `render_style` to the non-serialized `StyleRule` def is
    // non-breaking: a `record_construct` with schema `StyleRule` re-derives its
    // pins from the current def, so it exposes the new field's pin through the
    // ordinary layout-derivation path (the same mechanism `repair_node_network`
    // runs on a def change) — no migration, wires keyed by stable FieldId.
    use rust_lib_flutter_cad::structure_designer::nodes::record_construct::build_node_type_for_schema;
    let registry = NodeTypeRegistry::new();
    let base = registry
        .get_node_type("record_construct")
        .expect("record_construct registered");
    let nt = build_node_type_for_schema(base, "StyleRule", &registry);
    let names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["element", "tag", "color", "alpha", "render_style"]
    );
    // `Optional[String]` is exposed as a plain `String` pin (the wire layer
    // never sees `Optional`).
    let rs = nt
        .parameters
        .iter()
        .find(|p| p.name == "render_style")
        .unwrap();
    assert_eq!(rs.data_type, DataType::String);
}
