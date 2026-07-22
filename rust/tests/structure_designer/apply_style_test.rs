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
//!
//! Issue #413 adds the `fade_depth` field: `alpha`/`fade_depth` combine into
//! one depth-ramped alpha write per matched atom (`xray::depth_faded_alpha`),
//! so a later rule setting `alpha` alone exempts its atoms from an earlier
//! rule's fade — the issue's "opaque markers inside a faded block" use case.

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
            fade_depth: 0.0,
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

/// A `StyleRule` carrying the atom-labels `label` field (plus optional
/// selectors and `color`, for the composition tests). `label: None` = the field
/// is absent, i.e. "leave the atom's label alone".
fn style_rule_label(
    element: Option<i32>,
    tag: Option<&str>,
    color: Option<DVec3>,
    label: Option<&str>,
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
    if let Some(l) = label {
        fields.push(("label".to_string(), NetworkResult::String(l.to_string())));
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
            (
                "label".to_string(),
                DataType::Optional(Box::new(DataType::String))
            ),
            (
                "fade_depth".to_string(),
                DataType::Optional(Box::new(DataType::Float))
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
        vec![
            "element",
            "tag",
            "color",
            "alpha",
            "render_style",
            "label",
            "fade_depth"
        ]
    );
    // `Optional[String]` is exposed as a plain `String` pin (the wire layer
    // never sees `Optional`).
    let rs = nt
        .parameters
        .iter()
        .find(|p| p.name == "render_style")
        .unwrap();
    assert_eq!(rs.data_type, DataType::String);
    // Same for `fade_depth`: `Optional[Float]` surfaces as a plain `Float` pin.
    let fd = nt
        .parameters
        .iter()
        .find(|p| p.name == "fade_depth")
        .unwrap();
    assert_eq!(fd.data_type, DataType::Float);
}

// ============================================================================
// Labels (`doc/design_atom_labels.md` Phase 4)
// ============================================================================

/// Wire up an `apply_style` and evaluate it *without* the `evaluate_to_atomic`
/// panic-on-Error, for the label error-path tests.
fn eval_apply_style_raw(
    designer: &mut StructureDesigner,
    net: &str,
    molecule: NetworkResult,
    rules: NetworkResult,
) -> NetworkResult {
    let mol_id = add_value_node(designer, net, molecule);
    let rules_id = add_value_node(designer, net, rules);
    let style_id = add_apply_style_node(designer);
    designer.connect_nodes(mol_id, 0, style_id, 0);
    designer.connect_nodes(rules_id, 0, style_id, 1);
    evaluate(designer, net, style_id)
}

#[test]
fn apply_style_label_sets_literal_text() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // Label the oxygen (element 8) only; the carbons keep no label.
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![style_rule_label(Some(8), None, None, Some("here"))]),
    );
    assert_eq!(result.get_atom_label(1), None); // carbon
    assert_eq!(result.get_atom_label(2), None); // carbon
    assert_eq!(result.get_atom_label(3), Some("here"));
}

#[test]
fn apply_style_label_empty_string_clears_earlier_label() {
    // `label: ""` is the reset value, mirroring `alpha: 1.0` and
    // `render_style: "default"` — per-property last-writer-wins.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![
            style_rule_label(None, None, None, Some("everything")),
            style_rule_label(Some(6), None, None, Some("")),
        ]),
    );
    assert_eq!(result.get_atom_label(1), None); // carbon: cleared
    assert_eq!(result.get_atom_label(2), None); // carbon: cleared
    assert_eq!(result.get_atom_label(3), Some("everything")); // oxygen: untouched
}

#[test]
fn apply_style_label_element_token_expands_per_atom() {
    // The point of tokens: ONE match-all rule labels a whole structure, with
    // different text per atom. Without expansion this would need one rule per
    // element.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![style_rule_label(None, None, None, Some("{element}"))]),
    );
    assert_eq!(result.get_atom_label(1), Some("C"));
    assert_eq!(result.get_atom_label(2), Some("C"));
    assert_eq!(result.get_atom_label(3), Some("O"));
}

#[test]
fn apply_style_label_element_token_honors_name_overrides() {
    // A motif parameter element must label `P1` — the same symbol the hover
    // popup shows for it. The override map is a membership test; its mapped
    // String is the parameter's display name, which a label does not use.
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::param_index_to_atomic_number;
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    let mut s = AtomicStructure::new();
    let p1 = s.add_atom(param_index_to_atomic_number(0), DVec3::ZERO);
    let p2 = s.add_atom(param_index_to_atomic_number(1), DVec3::new(1.5, 0.0, 0.0));
    s.decorator_mut()
        .element_name_overrides
        .insert(param_index_to_atomic_number(0), "Anything".to_string());
    s.decorator_mut()
        .element_name_overrides
        .insert(param_index_to_atomic_number(1), "Else".to_string());

    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(s),
        rules_array(vec![style_rule_label(None, None, None, Some("{element}"))]),
    );
    assert_eq!(result.get_atom_label(p1), Some("P1"));
    assert_eq!(result.get_atom_label(p2), Some("P2"));
}

#[test]
fn apply_style_label_element_token_unknown_number_falls_back_to_x() {
    // `DEFAULT_ATOM_INFO`'s symbol, exactly as the popup falls back.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mut s = AtomicStructure::new();
    let unknown = s.add_atom(999, DVec3::ZERO);

    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(s),
        rules_array(vec![style_rule_label(None, None, None, Some("{element}"))]),
    );
    assert_eq!(result.get_atom_label(unknown), Some("X"));
}

#[test]
fn apply_style_label_tag_token_takes_rule_selector() {
    // When the rule HAS a tag selector, `{tag}` is unambiguous by construction:
    // the rule only matched atoms carrying it.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mut s = carbon_oxygen_structure();
    s.add_atom_tag(1, "surface").unwrap();
    s.add_atom_tag(1, "dopant").unwrap();

    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(s),
        rules_array(vec![style_rule_label(
            None,
            Some("dopant"),
            None,
            Some("{tag}"),
        )]),
    );
    // The selector wins over "first tag" — atom 1 carries both.
    assert_eq!(result.get_atom_label(1), Some("dopant"));
    assert_eq!(result.get_atom_label(2), None);
}

#[test]
fn apply_style_label_tag_token_falls_back_to_first_tag() {
    // Selector-less rule: `{tag}` documents itself as the atom's FIRST tag
    // (`atom_tags` returns names in bit order), and an untagged atom yields
    // empty — which clears rather than drawing an empty label.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mut s = carbon_oxygen_structure();
    s.add_atom_tag(1, "surface").unwrap();
    s.add_atom_tag(1, "dopant").unwrap();
    s.add_atom_tag(2, "dopant").unwrap();

    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(s),
        rules_array(vec![style_rule_label(None, None, None, Some("{tag}"))]),
    );
    assert_eq!(result.get_atom_label(1), Some("surface")); // first by bit order
    assert_eq!(result.get_atom_label(2), Some("dopant"));
    assert_eq!(result.get_atom_label(3), None); // untagged => empty => cleared
}

#[test]
fn apply_style_label_brace_escapes_and_mixed_literals() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![style_rule_label(
            Some(8),
            None,
            None,
            Some("{{{element}}} is it"),
        )]),
    );
    assert_eq!(result.get_atom_label(3), Some("{O} is it"));
}

#[test]
fn apply_style_label_unknown_token_errors_naming_rule_and_token() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style_raw(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![
            style_rule_label(None, None, None, Some("fine")),
            style_rule_label(None, None, None, Some("{elemental}")),
        ]),
    );
    match result {
        NetworkResult::Error(e) => {
            assert!(e.contains("rules[1]"), "error should name the rule: {}", e);
            assert!(e.contains("label"), "error should name the field: {}", e);
            assert!(
                e.contains("{elemental}"),
                "error should name the offending token: {}",
                e
            );
        }
        other => panic!("Expected Error, got {:?}", other.infer_data_type()),
    }
}

#[test]
fn apply_style_label_unterminated_and_unescaped_braces_error() {
    let net = "test";
    for template in ["{element", "50% }"] {
        let mut designer = setup_designer_with_network(net);
        let result = eval_apply_style_raw(
            &mut designer,
            net,
            molecule_value(carbon_oxygen_structure()),
            rules_array(vec![style_rule_label(None, None, None, Some(template))]),
        );
        match result {
            NetworkResult::Error(e) => assert!(
                e.contains("rules[0].label"),
                "error should be localized for {:?}: {}",
                template,
                e
            ),
            other => panic!(
                "Expected Error for {:?}, got {:?}",
                template,
                other.infer_data_type()
            ),
        }
    }
}

#[test]
fn apply_style_label_bad_token_applies_nothing() {
    // Templates are parsed before ANY rule is applied, so a bad token in a
    // later rule fails the whole pin rather than leaving the earlier rule's
    // color half-written onto the structure. Same contract as every other
    // field's error — and the reason the error is worth surfacing at all.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style_raw(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![
            style_rule_label(None, None, Some(DVec3::new(1.0, 0.0, 0.0)), Some("ok")),
            style_rule_label(None, None, None, Some("{nope}")),
        ]),
    );
    // An Error, not a structure carrying the first rule's color/label.
    assert!(
        matches!(result, NetworkResult::Error(_)),
        "expected Error, got {:?}",
        result.infer_data_type()
    );
}

#[test]
fn apply_style_label_wrong_type_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style_raw(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![NetworkResult::record(vec![(
            "label".to_string(),
            NetworkResult::Int(7),
        )])]),
    );
    match result {
        NetworkResult::Error(e) => {
            assert!(e.contains("rules[0].label"), "{}", e);
            assert!(e.contains("expected String"), "{}", e);
        }
        other => panic!("Expected Error, got {:?}", other.infer_data_type()),
    }
}

#[test]
fn apply_style_label_composes_with_other_properties() {
    // A later rule setting only `color` must leave the label intact —
    // last-writer-wins is PER PROPERTY, not per rule.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![
            style_rule_label(
                Some(8),
                None,
                Some(DVec3::new(1.0, 0.0, 0.0)),
                Some("{element}"),
            ),
            style_rule(Some(8), None, Some(DVec3::new(0.0, 1.0, 0.0)), None),
        ]),
    );
    assert_eq!(result.get_atom_label(3), Some("O"));
    assert_eq!(result.get_atom_color(3), Some(Vec3::new(0.0, 1.0, 0.0)));
}

#[test]
fn record_construct_style_rule_label_pin_preserves_wires() {
    // Adding `label` to the non-serialized `StyleRule` def is non-breaking: a
    // `record_construct` with schema `StyleRule` re-derives its pins from the
    // current def, so it gains the new field's pin — and a wire into a field
    // that already existed survives the repair pass that does it.
    use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    let color_id = add_value_node(
        &mut designer,
        net,
        NetworkResult::Vec3(DVec3::new(1.0, 0.0, 0.0)),
    );
    let rc_id = designer.add_node("record_construct", DVec2::new(100.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut(net).unwrap();
        let node = network.nodes.get_mut(&rc_id).unwrap();
        node.data = Box::new(RecordConstructData {
            schema: "StyleRule".to_string(),
            ..Default::default()
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }
    // `color` is pin index 2 in the def's authored field order.
    designer.connect_nodes(color_id, 0, rc_id, 2);

    // Re-run the repair pass — the same mechanism a def change triggers.
    {
        let registry = &mut designer.node_type_registry;
        let mut network = registry.node_networks.get(net).unwrap().clone();
        registry.repair_node_network(&mut network);
        registry.node_networks.insert(net.to_string(), network);
    }

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(net).unwrap();
    let node = network.nodes.get(&rc_id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();
    let names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "element",
            "tag",
            "color",
            "alpha",
            "render_style",
            "label",
            "fade_depth"
        ]
    );
    // The pre-existing wire survived the def growth.
    assert!(
        !node.arguments[2].incoming_wires.is_empty(),
        "the color wire must survive the def gaining a `label` field"
    );
}

// ============================================================================
// fade_depth (issue #413)
// ============================================================================

/// A `StyleRule` carrying the depth-fade fields: optional selectors plus
/// `alpha` / `fade_depth` (absent = unset).
fn style_rule_fade(
    element: Option<i32>,
    tag: Option<&str>,
    alpha: Option<f64>,
    fade_depth: Option<f64>,
) -> NetworkResult {
    let mut fields = Vec::new();
    if let Some(e) = element {
        fields.push(("element".to_string(), NetworkResult::Int(e)));
    }
    if let Some(t) = tag {
        fields.push(("tag".to_string(), NetworkResult::String(t.to_string())));
    }
    if let Some(a) = alpha {
        fields.push(("alpha".to_string(), NetworkResult::Float(a)));
    }
    if let Some(f) = fade_depth {
        fields.push(("fade_depth".to_string(), NetworkResult::Float(f)));
    }
    NetworkResult::record(fields)
}

/// The three-atom structure with authored crystal depths: atom 1 at the
/// surface (0 Å), atom 2 halfway (4 Å), atom 3 deep (8 Å).
fn depth_graded_structure() -> AtomicStructure {
    let mut s = carbon_oxygen_structure();
    s.set_atom_in_crystal_depth(1, 0.0);
    s.set_atom_in_crystal_depth(2, 4.0);
    s.set_atom_in_crystal_depth(3, 8.0);
    s
}

fn assert_alpha_near(actual: f32, expected: f32, atom_id: u32) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "atom {}: alpha {} != expected {}",
        atom_id,
        actual,
        expected
    );
}

#[test]
fn apply_style_fade_depth_ramps_alpha_with_depth() {
    // alpha 0.8, fade_depth 8: surface atom keeps 0.8, the 4 Å atom is at the
    // smoothstep midpoint (0.8 × 0.5 = 0.4), the 8 Å atom is fully transparent.
    // Same ramp as the xray node (`depth_faded_alpha`), baked per atom.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(depth_graded_structure()),
        rules_array(vec![style_rule_fade(None, None, Some(0.8), Some(8.0))]),
    );
    assert_alpha_near(result.get_atom_alpha(1), 0.8, 1);
    assert_alpha_near(result.get_atom_alpha(2), 0.4, 2);
    assert_alpha_near(result.get_atom_alpha(3), 0.0, 3);
}

#[test]
fn apply_style_fade_depth_without_alpha_uses_opaque_surface() {
    // A rule setting only `fade_depth` writes alpha with a surface value of
    // 1.0: surface atoms stay fully opaque (entry removed), deeper atoms fade.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(depth_graded_structure()),
        rules_array(vec![style_rule_fade(None, None, None, Some(8.0))]),
    );
    assert_alpha_near(result.get_atom_alpha(1), 1.0, 1);
    assert_alpha_near(result.get_atom_alpha(2), 0.5, 2);
    assert_alpha_near(result.get_atom_alpha(3), 0.0, 3);
}

#[test]
fn apply_style_fade_exemption_by_later_alpha_rule() {
    // THE issue #413 use case: fade a whole block, then exempt specific tagged
    // atoms with a later rule. `alpha`/`fade_depth` write the alpha property as
    // one unit, so the later rule's plain `alpha: 1.0` fully overwrites the
    // faded value — the tagged deep atom renders opaque.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mut s = depth_graded_structure();
    s.add_atom_tag(3, "keep").unwrap();
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(s),
        rules_array(vec![
            style_rule_fade(None, None, Some(0.5), Some(8.0)),
            style_rule_fade(None, Some("keep"), Some(1.0), None),
        ]),
    );
    assert_alpha_near(result.get_atom_alpha(1), 0.5, 1); // surface, faded rule
    assert_alpha_near(result.get_atom_alpha(2), 0.25, 2); // midpoint of the ramp
    assert_alpha_near(result.get_atom_alpha(3), 1.0, 3); // deep but exempted
}

#[test]
fn apply_style_fade_depth_zero_applies_alpha_uniformly() {
    // `fade_depth: 0` (and any non-positive value) disables the ramp — the
    // rule degenerates to a plain uniform alpha write, depth ignored.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style(
        &mut designer,
        net,
        molecule_value(depth_graded_structure()),
        rules_array(vec![style_rule_fade(None, None, Some(0.5), Some(0.0))]),
    );
    for id in [1, 2, 3] {
        assert_alpha_near(result.get_atom_alpha(id), 0.5, id);
    }
}

#[test]
fn apply_style_fade_depth_wrong_type_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let result = eval_apply_style_raw(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
        rules_array(vec![NetworkResult::record(vec![(
            "fade_depth".to_string(),
            NetworkResult::String("deep".to_string()),
        )])]),
    );
    match result {
        NetworkResult::Error(e) => {
            assert!(e.contains("rules[0].fade_depth"), "{}", e);
            assert!(e.contains("expected Float"), "{}", e);
        }
        other => panic!("Expected Error, got {:?}", other.infer_data_type()),
    }
}
