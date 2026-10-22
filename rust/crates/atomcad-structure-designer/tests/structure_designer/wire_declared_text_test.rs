//! Wire-declared inputs, Phase 5 (`doc/design_wire_declared_inputs.md`): in
//! the text format a wire to a name a declared-inputs node does not have yet
//! declares it, a fresh `expr` has no parameters, and every path that names a
//! new `expr` parameter hands out the next free letter.

use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::nodes::expr::ExprData;
use atomcad_structure_designer::nodes::record_construct::RecordConstructData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::serialize_network;
use glam::f64::DVec2;

const NET: &str = "Main";
const OUT: SourcePin = SourcePin::NodeOutput { pin_index: 0 };

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn designer() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.preferences.node_display_preferences.display_policy =
        atomcad_structure_designer::preferences::NodeDisplayPolicy::Manual;
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    d
}

/// Apply `code` incrementally; returns the edit's warnings. Fails on errors.
fn edit(d: &mut StructureDesigner, code: &str) -> Vec<String> {
    let outcome = d.ai_text_edit(code, false);
    assert!(
        outcome.result.errors.is_empty(),
        "edit failed: {:?}\n{}",
        outcome.result.errors,
        code
    );
    d.validate_active_network();
    outcome.result.warnings
}

fn network(d: &StructureDesigner) -> &NodeNetwork {
    d.get_scope_network(&[]).unwrap()
}

fn id(d: &StructureDesigner, name: &str) -> u64 {
    network(d)
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node '{}'", name))
        .id
}

fn expr(d: &StructureDesigner, node_id: u64) -> &ExprData {
    d.get_node_network_data_scoped(&[], node_id)
        .unwrap()
        .as_any_ref()
        .downcast_ref::<ExprData>()
        .unwrap()
}

fn params(d: &StructureDesigner, name: &str) -> Vec<(String, DataType)> {
    expr(d, id(d, name))
        .parameters
        .iter()
        .map(|p| (p.name.clone(), p.data_type.clone()))
        .collect()
}

fn wired_sources(d: &StructureDesigner, name: &str) -> Vec<Vec<u64>> {
    network(d).nodes[&id(d, name)]
        .arguments
        .iter()
        .map(|a| a.incoming_wires.iter().map(|w| w.source_node_id).collect())
        .collect()
}

fn errors_on(d: &StructureDesigner, node_id: u64) -> Vec<String> {
    network(d)
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(node_id))
        .map(|e| e.error_text.clone())
        .collect()
}

fn eval(d: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &d.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let network_stack = vec![NetworkStackElement::root(network)];
    NetworkEvaluator::new().evaluate(
        &network_stack,
        node_id,
        0,
        registry,
        false,
        &mut NetworkEvaluationContext::new(),
    )
}

fn text(d: &StructureDesigner) -> String {
    serialize_network(network(d), &d.node_type_registry, Some(NET))
}

fn s(v: &str) -> String {
    v.to_string()
}

// ---------------------------------------------------------------------------
// The fresh expr
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_expr_has_no_parameters_and_evaluates_to_zero() {
    let mut d = designer();
    let e = d.add_node("expr", DVec2::ZERO);
    d.validate_active_network();
    assert!(expr(&d, e).parameters.is_empty());
    assert_eq!(expr(&d, e).expression, "0");
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
    assert!(matches!(eval(&d, e), NetworkResult::Int(0)));
}

// ---------------------------------------------------------------------------
// The text format: a wire declares
// ---------------------------------------------------------------------------

#[test]
fn wiring_undeclared_names_declares_them_with_the_sources_types() {
    let mut d = designer();
    let warnings = edit(
        &mut d,
        "a = float { value: 1.5 }
v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"v * a\", a: a, v: v }
",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        params(&d, "e"),
        vec![(s("a"), DataType::Float), (s("v"), DataType::Vec3)],
        "declared in the order written, typed by the source"
    );
    assert_eq!(
        wired_sources(&d, "e"),
        vec![vec![id(&d, "a")], vec![id(&d, "v")]]
    );
    let e = id(&d, "e");
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
    assert!(matches!(eval(&d, e), NetworkResult::Vec3(_)));
}

/// The text the old default made work: `x` used to exist (typed `Int`), and
/// now it is declared — with the source's type rather than `Int`.
#[test]
fn the_old_default_x_spelling_still_works() {
    let mut d = designer();
    let warnings = edit(
        &mut d,
        "val = float { value: 2.5 }
doubled = expr { expression: \"x * 2\", x: val }
",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(params(&d, "doubled"), vec![(s("x"), DataType::Float)]);
    assert!(matches!(eval(&d, id(&d, "doubled")), NetworkResult::Float(v) if v == 5.0));
}

#[test]
fn an_explicit_parameter_list_wins() {
    let mut d = designer();
    let warnings = edit(
        &mut d,
        "a = int { value: 1 }
b = int { value: 2 }
e = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Int }], x: a, y: b }
",
    );
    assert!(
        warnings.iter().any(|w| w.contains("'y' not found")),
        "{warnings:?}"
    );
    assert_eq!(params(&d, "e"), vec![(s("x"), DataType::Int)]);
}

#[test]
fn a_literal_declares_nothing() {
    let mut d = designer();
    edit(&mut d, "e = expr { expression: \"1\", y: 5 }\n");
    assert!(params(&d, "e").is_empty(), "a literal carries no type");
}

#[test]
fn an_array_of_wires_declares_nothing() {
    let mut d = designer();
    let warnings = edit(
        &mut d,
        "a = int { value: 1 }
e = expr { expression: \"1\", y: [a] }
",
    );
    assert!(!warnings.is_empty(), "{warnings:?}");
    assert!(params(&d, "e").is_empty());
}

#[test]
fn a_reserved_name_is_refused_with_its_reason() {
    let mut d = designer();
    let warnings = edit(
        &mut d,
        "a = float { value: 1.0 }
e = expr { expression: \"1\", pi: a }
",
    );
    assert!(
        warnings.iter().any(|w| w.contains("reserved")),
        "{warnings:?}"
    );
    assert!(params(&d, "e").is_empty());
}

#[test]
fn a_source_the_node_cannot_take_is_refused_with_its_reason() {
    let mut d = designer();
    let warnings = edit(
        &mut d,
        "r = range { count: 3 }
e = expr { expression: \"1\", xs: r }
",
    );
    assert!(
        warnings.iter().any(|w| w.contains("iterator")),
        "{warnings:?}"
    );
    assert!(params(&d, "e").is_empty());
}

/// `outer` is written first but its source, `inner`, only gets an output type
/// once its own parameter is declared: the retry rounds resolve the chain.
#[test]
fn a_chain_of_declarations_resolves_in_any_order() {
    let mut d = designer();
    let warnings = edit(
        &mut d,
        "outer = expr { expression: \"y * 2.0\", y: inner }
inner = expr { expression: \"x + 1.0\", x: a }
a = float { value: 1.0 }
",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(params(&d, "inner"), vec![(s("x"), DataType::Float)]);
    assert_eq!(params(&d, "outer"), vec![(s("y"), DataType::Float)]);
    assert!(matches!(eval(&d, id(&d, "outer")), NetworkResult::Float(v) if v == 4.0));
}

#[test]
fn an_incremental_edit_appends_to_the_existing_parameters() {
    let mut d = designer();
    edit(
        &mut d,
        "a = int { value: 1 }
b = float { value: 2.0 }
e = expr { expression: \"x\", x: a }
",
    );
    let pin_id_before = expr(&d, id(&d, "e")).parameters[0].id;
    let warnings = edit(&mut d, "e = expr { expression: \"x + y\", y: b }\n");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        params(&d, "e"),
        vec![(s("x"), DataType::Int), (s("y"), DataType::Float)]
    );
    assert_eq!(expr(&d, id(&d, "e")).parameters[0].id, pin_id_before);
    assert_eq!(
        wired_sources(&d, "e"),
        vec![vec![id(&d, "a")], vec![id(&d, "b")]],
        "the earlier wire stays on its pin"
    );
}

#[test]
fn an_anonymous_record_construct_declares_fields_the_same_way() {
    let mut d = designer();
    let warnings = edit(
        &mut d,
        "f = float { value: 1.5 }
v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
r = record_construct { size: f, at: v }
",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    let r = id(&d, "r");
    let data = d
        .get_node_network_data_scoped(&[], r)
        .unwrap()
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .unwrap();
    let fields: Vec<(String, DataType)> = data
        .fields
        .iter()
        .map(|f| (f.name.clone(), f.data_type.clone()))
        .collect();
    assert_eq!(
        fields,
        vec![(s("size"), DataType::Float), (s("at"), DataType::Vec3)]
    );
    assert!(errors_on(&d, r).is_empty(), "{:?}", errors_on(&d, r));
    let expected = DataType::Record(RecordType::anonymous(vec![
        (s("at"), DataType::Vec3),
        (s("size"), DataType::Float),
    ]));
    let node = &network(&d).nodes[&r];
    assert_eq!(
        *d.node_type_registry
            .get_node_type_for_node(node)
            .unwrap()
            .output_type(),
        expected
    );
}

#[test]
fn a_declared_network_survives_a_replace_unchanged() {
    let mut d = designer();
    edit(
        &mut d,
        "a = float { value: 1.5 }
e = expr { expression: \"x * 2\", x: a }
r = record_construct { e: e }
",
    );
    let first = text(&d);
    assert!(first.contains("parameters: ["), "{first}");
    let outcome = d.ai_text_edit(&first, true);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    assert_eq!(text(&d), first);
}

// ---------------------------------------------------------------------------
// One naming rule for every path that adds an expr parameter
// ---------------------------------------------------------------------------

#[test]
fn the_add_button_name_is_the_next_free_letter() {
    let mut d = designer();
    edit(
        &mut d,
        "e = expr { expression: \"1\", parameters: [{ name: \"x\", data_type: Int }, { name: \"z\", data_type: Int }] }
r = record_construct {}
i = int { value: 1 }
",
    );
    assert_eq!(d.next_declared_input_name(&[], id(&d, "e")), Some(s("y")));
    assert_eq!(
        d.next_declared_input_name(&[], id(&d, "r")),
        Some(s("field")),
        "a record has no pool: the fallback"
    );
    assert_eq!(d.next_declared_input_name(&[], id(&d, "i")), None);
}

#[test]
fn the_hover_preview_names_what_the_drop_creates() {
    let mut d = designer();
    edit(
        &mut d,
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
r = range { count: 2 }
e = expr { expression: \"1\", parameters: [{ name: \"x\", data_type: Int }] }
",
    );
    let (v, r, e) = (id(&d, "v"), id(&d, "r"), id(&d, "e"));
    let preview = d.preview_add_input_from_wire(&[], e, v, OUT, 0);
    assert_eq!(preview, Ok(s("y")));
    assert!(d.preview_add_input_from_wire(&[], e, r, OUT, 0).is_err());
    let added = d.add_input_from_wire(&[], e, v, OUT, 0).unwrap();
    assert_eq!(Ok(added.name), preview);
}
