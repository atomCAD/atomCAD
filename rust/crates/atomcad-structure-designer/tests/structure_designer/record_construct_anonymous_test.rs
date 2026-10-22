//! Anonymous `record_construct` (`doc/design_wire_declared_inputs.md`,
//! Phase 2): a `record_construct` with no schema declares its own fields, gets
//! the open port and Take type from wire through `DeclaredInputs`, switches
//! schema in one undo step that restores the wires a switch drops, and keeps
//! the record-def names its field types mention visible to every reference
//! walker. The text-format round trip is in
//! `text_format_roundtrip_corpus_test.rs`, the `.cnnd` one in the integration
//! harness, the FFI surface in `rust/tests/structure_designer_api/`.

use super::library_links_support::{edit as edit_network, new_design, open, save};
use crate::undo_test::assert_undo_redo_roundtrip;
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::invariants::{InvariantKind, check_network_invariants};
use atomcad_structure_designer::library_links::is_frozen;
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type_registry::{
    RecordTypeDef, collect_record_refs_in_network, collect_record_refs_in_node,
};
use atomcad_structure_designer::nodes::expr::ExprData;
use atomcad_structure_designer::nodes::record_construct::{
    ANON_FIELD_PIN_ID_BASE, LocalRecordField, RecordConstructData,
};
use atomcad_structure_designer::nodes::value::add_typed_value_node;
use atomcad_structure_designer::record_construct_ops::{
    EDIT_RECORD_FIELDS_DESCRIPTION, SET_RECORD_SCHEMA_DESCRIPTION,
};
use atomcad_structure_designer::structure_designer::{DragSource, StructureDesigner};
use atomcad_structure_designer::text_format::TextValue;
use glam::f64::{DVec2, DVec3};
use std::collections::HashSet;

const NET: &str = "Main";
const OUT: SourcePin = SourcePin::NodeOutput { pin_index: 0 };
const FUNCTION_PIN: SourcePin = SourcePin::NodeOutput { pin_index: -1 };

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A designer whose active network `Main` holds `code` (text format),
/// validated.
fn setup(code: &str) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    edit(&mut d, code);
    d
}

/// Add `code` to the active network (incremental text edit), validated.
fn edit(d: &mut StructureDesigner, code: &str) {
    let outcome = d.ai_text_edit(code, false);
    assert!(
        outcome.result.errors.is_empty(),
        "edit failed: {:?}\n{}",
        outcome.result.errors,
        code
    );
    d.validate_active_network();
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

fn rc(d: &StructureDesigner, node_id: u64) -> &RecordConstructData {
    d.record_construct_data(&[], node_id)
        .expect("a record_construct")
}

fn fields(d: &StructureDesigner, node_id: u64) -> Vec<(String, DataType)> {
    rc(d, node_id)
        .fields
        .iter()
        .map(|f| (f.name.clone(), f.data_type.clone()))
        .collect()
}

fn field_ids(d: &StructureDesigner, node_id: u64) -> Vec<u64> {
    rc(d, node_id).fields.iter().map(|f| f.id).collect()
}

fn pin_ids(d: &StructureDesigner, node_id: u64) -> Vec<Option<u64>> {
    let node = &network(d).nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .parameters
        .iter()
        .map(|p| p.id)
        .collect()
}

fn output_type(d: &StructureDesigner, node_id: u64) -> DataType {
    let node = &network(d).nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .output_type()
        .clone()
}

/// The source node of every wire into each input pin.
fn sources_into(d: &StructureDesigner, node_id: u64) -> Vec<Vec<u64>> {
    network(d).nodes[&node_id]
        .arguments
        .iter()
        .map(|arg| {
            arg.incoming_wires
                .iter()
                .map(|w| w.source_node_id)
                .collect()
        })
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

fn has_mismatch(d: &StructureDesigner, node_id: u64) -> bool {
    errors_on(d, node_id)
        .iter()
        .any(|e| e.starts_with("Data type mismatch"))
}

fn anon(fields: &[(&str, DataType)]) -> DataType {
    DataType::Record(RecordType::anonymous(
        fields
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect(),
    ))
}

fn named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

fn owned(fields: &[(&str, DataType)]) -> Vec<(String, DataType)> {
    fields
        .iter()
        .map(|(n, t)| (n.to_string(), t.clone()))
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

fn float_field(record: &NetworkResult, name: &str) -> f64 {
    match record.extract_record_field(name) {
        Some(NetworkResult::Float(v)) => *v,
        other => panic!(
            "field '{}' is not a Float: {:?}",
            name,
            other.map(|r| r.to_display_string())
        ),
    }
}

fn set_literal(d: &mut StructureDesigner, node_id: u64, field: &str, value: TextValue) {
    let mut data = rc(d, node_id).clone();
    data.literal_values.insert(field.to_string(), value);
    d.set_node_network_data_scoped(&[], node_id, Box::new(data));
}

fn set_expression(d: &mut StructureDesigner, node_id: u64, expression: &str) {
    let mut data = d
        .get_node_network_data_scoped(&[], node_id)
        .unwrap()
        .as_any_ref()
        .downcast_ref::<ExprData>()
        .unwrap()
        .clone();
    data.expression = expression.to_string();
    d.set_node_network_data_scoped(&[], node_id, Box::new(data));
    d.validate_active_network();
}

fn expr_param_types(d: &StructureDesigner, node_id: u64) -> Vec<DataType> {
    d.get_node_network_data_scoped(&[], node_id)
        .unwrap()
        .as_any_ref()
        .downcast_ref::<ExprData>()
        .unwrap()
        .parameters
        .iter()
        .map(|p| p.data_type.clone())
        .collect()
}

/// The network as saved, for "nothing changed" checks.
fn fingerprint(d: &mut StructureDesigner) -> String {
    serde_json::to_string(&d.snapshot_network(NET).unwrap()).unwrap()
}

/// `r` with fields `a: Float` (unwired, literal 4.0), `b: Int` (fed by `y`)
/// and `c: Vec3` (fed by `z`).
fn three_field_network() -> StructureDesigner {
    let mut d = setup(
        "y = int { value: 2 }
z = vec3 { x: 1.0, y: 2.0, z: 3.0 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Int }, { name: \"c\", data_type: Vec3 }], b: y, c: z }
",
    );
    let r = id(&d, "r");
    set_literal(&mut d, r, "a", TextValue::Float(4.0));
    d
}

// ---------------------------------------------------------------------------
// Node type and eval
// ---------------------------------------------------------------------------

#[test]
fn an_anonymous_node_has_a_pin_per_field_and_the_anonymous_record_as_output() {
    let d = setup(
        "x = float { value: 1.5 }
i = int { value: 2 }
r = record_construct { fields: [{ name: \"b\", data_type: Int }, { name: \"a\", data_type: Float }], b: i, a: x }
",
    );
    let r = id(&d, "r");
    let ids = field_ids(&d, r);
    assert_eq!(
        pin_ids(&d, r),
        ids.iter()
            .map(|id| Some(ANON_FIELD_PIN_ID_BASE | id))
            .collect::<Vec<_>>(),
        "pins in authored order, carrying the base-tagged local ids"
    );
    assert_eq!(
        output_type(&d, r),
        anon(&[("a", DataType::Float), ("b", DataType::Int)])
    );
    assert!(errors_on(&d, r).is_empty(), "{:?}", errors_on(&d, r));

    let value = eval(&d, r);
    assert_eq!(float_field(&value, "a"), 1.5);
    assert!(matches!(
        value.extract_record_field("b"),
        Some(NetworkResult::Int(2))
    ));
}

#[test]
fn an_unwired_field_uses_its_literal_or_makes_the_record_none() {
    let mut d = setup(
        "x = float { value: 1.5 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Float }], a: x }
",
    );
    let r = id(&d, "r");
    assert!(
        matches!(eval(&d, r), NetworkResult::None),
        "an unwired required field with no literal"
    );

    set_literal(&mut d, r, "b", TextValue::Float(2.5));
    let value = eval(&d, r);
    assert_eq!(float_field(&value, "a"), 1.5);
    assert_eq!(
        float_field(&value, "b"),
        2.5,
        "the literal of the unwired field"
    );
}

#[test]
fn zero_fields_give_the_empty_record() {
    let d = setup("r = record_construct {}\n");
    let r = id(&d, "r");
    assert_eq!(output_type(&d, r), anon(&[]));
    assert!(matches!(eval(&d, r), NetworkResult::Record(f) if f.is_empty()));
}

/// The output wires into an `expr` parameter declared with the same
/// anonymous type, and the expression reads a field.
#[test]
fn the_output_feeds_an_expr_that_reads_a_field() {
    let d = setup(
        "x = float { value: 1.5 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }], a: x }
e = expr { expression: \"p.a * 2.0\", parameters: [{ name: \"p\", data_type: {a: Float} }], p: r }
",
    );
    let e = id(&d, "e");
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
    assert!(matches!(eval(&d, e), NetworkResult::Float(v) if v == 3.0));
}

#[test]
fn declared_inputs_exist_only_in_anonymous_mode() {
    let anonymous = RecordConstructData::default();
    assert!(anonymous.declared_inputs().is_some());
    let mut named = RecordConstructData {
        schema: "Box".to_string(),
        ..Default::default()
    };
    assert!(named.declared_inputs().is_none());
    assert!(named.declared_inputs_mut().is_none());
}

// ---------------------------------------------------------------------------
// The open port
// ---------------------------------------------------------------------------

#[test]
fn a_drop_appends_a_field_of_the_sources_type_and_wires_it() {
    let mut d = setup(
        "x = float { value: 1.5 }
v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }], a: x }
",
    );
    let (x, v, r) = (id(&d, "x"), id(&d, "v"), id(&d, "r"));
    let pushes = d.undo_stack.push_count();

    let added = d.add_input_from_wire(&[], r, v, OUT, 0).unwrap();

    assert_eq!(added.name, "v");
    assert_eq!(added.pin_index, 1);
    assert_eq!(added.pin_id, ANON_FIELD_PIN_ID_BASE | field_ids(&d, r)[1]);
    assert_eq!(
        fields(&d, r),
        owned(&[("a", DataType::Float), ("v", DataType::Vec3)])
    );
    assert_eq!(sources_into(&d, r), vec![vec![x], vec![v]]);
    assert_eq!(
        output_type(&d, r),
        anon(&[("a", DataType::Float), ("v", DataType::Vec3)])
    );
    assert_eq!(d.undo_stack.push_count(), pushes + 1, "one undo step");
    assert!(matches!(
        eval(&d, r).extract_record_field("v"),
        Some(NetworkResult::Vec3(p)) if *p == DVec3::new(1.0, 2.0, 3.0)
    ));
}

#[test]
fn a_drop_on_a_record_construct_is_one_undo_step() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
r = record_construct {}
",
    );
    let (v, r) = (id(&d, "v"), id(&d, "r"));
    assert_undo_redo_roundtrip(&mut d, |d| {
        d.add_input_from_wire(&[], r, v, OUT, 0).unwrap();
    });
}

/// A function value can be a record field (unlike an `expr` parameter), and
/// a second drop of the same source is deduplicated.
#[test]
fn a_function_source_is_a_field_and_names_are_deduplicated() {
    let mut d = setup(
        "g = expr { expression: \"q + 1\", parameters: [{ name: \"q\", data_type: Int }] }
r = record_construct {}
",
    );
    let (g, r) = (id(&d, "g"), id(&d, "r"));
    d.add_input_from_wire(&[], r, g, FUNCTION_PIN, 0).unwrap();
    let second = d.add_input_from_wire(&[], r, g, FUNCTION_PIN, 0).unwrap();
    assert_eq!(second.name, "g_2");
    let types: Vec<DataType> = fields(&d, r).into_iter().map(|(_, t)| t).collect();
    assert!(matches!(types[0], DataType::Function(_)), "{:?}", types);
}

/// The refusals of Phase 1 that apply to a record field: a `None`-typed
/// source, a `Unit` source, and a named-mode destination (its fields are its
/// def's). Each leaves the network unchanged, pushes nothing, and the hover
/// check refuses exactly when the drop does.
#[test]
fn refusals_leave_the_network_unchanged_and_push_nothing() {
    let mut d = setup(
        "x = float { value: 1.0 }
named = record_construct {}
anon = record_construct {}
",
    );
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Box",
        vec![("a".to_string(), DataType::Float)],
    ))
    .unwrap();
    let named_node = id(&d, "named");
    d.set_record_construct_schema(&[], named_node, "Box")
        .unwrap();
    let net = d.node_type_registry.node_networks.get_mut(NET).unwrap();
    let nothing = add_typed_value_node(
        net,
        DVec2::new(0.0, 400.0),
        NetworkResult::None,
        DataType::None,
    );
    let unit = add_typed_value_node(
        net,
        DVec2::new(0.0, 500.0),
        NetworkResult::None,
        DataType::Unit,
    );
    d.validate_active_network();
    let (x, anon_node) = (id(&d, "x"), id(&d, "anon"));

    for (label, dest, source) in [
        ("None-typed source", anon_node, nothing),
        ("Unit source", anon_node, unit),
        ("named-mode destination", named_node, x),
    ] {
        let before = fingerprint(&mut d);
        let pushes = d.undo_stack.push_count();
        let hover = d.can_add_input_from_wire(&[], dest, source, OUT, 0);
        let drop = d.add_input_from_wire(&[], dest, source, OUT, 0);
        assert!(drop.is_err(), "{}: the drop was accepted", label);
        assert!(hover.is_some(), "{}: the hover check accepted", label);
        assert_eq!(
            fingerprint(&mut d),
            before,
            "{}: changed the network",
            label
        );
        assert_eq!(d.undo_stack.push_count(), pushes, "{}: pushed", label);
    }
}

/// The sequence the drag-aware add uses for "drag an output into empty space
/// → `record_construct`" (wired up in Phase 4): the node is added unadapted
/// from a `FromOutput` drag, then the dragged source is dropped on its open
/// port, giving a one-field anonymous record named after the source.
#[test]
fn drag_add_then_open_port_drop_gives_a_one_field_record() {
    use atomcad_structure_designer::node_data::DragDirection;
    let mut d = setup("gap = float { value: 0.5 }\n");
    let gap = id(&d, "gap");
    let r = d.add_node_with_drag_source(
        "record_construct",
        DVec2::new(300.0, 0.0),
        Some(DragSource {
            source_type: DataType::Float,
            direction: DragDirection::FromOutput,
        }),
    );
    assert!(rc(&d, r).is_anonymous());
    assert!(rc(&d, r).fields.is_empty(), "FromOutput is not adapted");

    let added = d.add_input_from_wire(&[], r, gap, OUT, 0).unwrap();

    assert_eq!(added.pin_index, 0);
    assert_eq!(fields(&d, r), owned(&[("gap", DataType::Float)]));
    assert_eq!(sources_into(&d, r), vec![vec![gap]]);
    assert_eq!(float_field(&eval(&d, r), "gap"), 0.5);
}

// ---------------------------------------------------------------------------
// Editing the field list
// ---------------------------------------------------------------------------

#[test]
fn a_rename_keeps_the_wire_and_moves_the_literal() {
    let mut d = three_field_network();
    let (y, z, r) = (id(&d, "y"), id(&d, "z"), id(&d, "r"));
    let ids = field_ids(&d, r);

    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_construct_fields(
            &[],
            r,
            owned(&[
                ("alpha", DataType::Float),
                ("beta", DataType::Int),
                ("c", DataType::Vec3),
            ]),
        )
        .unwrap();
    });
    d.redo();

    assert_eq!(field_ids(&d, r), ids, "ids kept");
    assert_eq!(sources_into(&d, r), vec![vec![], vec![y], vec![z]]);
    let literals = &rc(&d, r).literal_values;
    assert_eq!(literals.get("alpha"), Some(&TextValue::Float(4.0)));
    assert!(!literals.contains_key("a"));
    assert_eq!(
        d.undo_stack.undo_description(),
        Some(EDIT_RECORD_FIELDS_DESCRIPTION)
    );
}

#[test]
fn a_reorder_keeps_every_wire_on_its_field() {
    let mut d = three_field_network();
    let (y, z, r) = (id(&d, "y"), id(&d, "z"), id(&d, "r"));
    let ids = field_ids(&d, r);

    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_construct_fields(
            &[],
            r,
            owned(&[
                ("c", DataType::Vec3),
                ("a", DataType::Float),
                ("b", DataType::Int),
            ]),
        )
        .unwrap();
    });
    d.redo();

    assert_eq!(field_ids(&d, r), vec![ids[2], ids[0], ids[1]]);
    assert_eq!(sources_into(&d, r), vec![vec![z], vec![], vec![y]]);
    assert_eq!(
        rc(&d, r).literal_values.get("a"),
        Some(&TextValue::Float(4.0))
    );
}

#[test]
fn a_delete_drops_its_wire_and_literal_only() {
    let mut d = three_field_network();
    let (y, z, r) = (id(&d, "y"), id(&d, "z"), id(&d, "r"));

    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_construct_fields(
            &[],
            r,
            owned(&[("b", DataType::Int), ("c", DataType::Vec3)]),
        )
        .unwrap();
    });
    d.redo();

    assert_eq!(sources_into(&d, r), vec![vec![y], vec![z]]);
    assert!(rc(&d, r).literal_values.is_empty(), "a's literal is gone");

    // Deleting a wired field drops its wire.
    d.set_record_construct_fields(&[], r, owned(&[("c", DataType::Vec3)]))
        .unwrap();
    assert_eq!(sources_into(&d, r), vec![vec![z]]);
}

#[test]
fn an_added_field_never_reuses_a_deleted_fields_id() {
    let mut d = three_field_network();
    let r = id(&d, "r");
    let ids = field_ids(&d, r);
    d.set_record_construct_fields(
        &[],
        r,
        owned(&[("a", DataType::Float), ("b", DataType::Int)]),
    )
    .unwrap();
    let floor = rc(&d, r).next_field_id;

    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_construct_fields(
            &[],
            r,
            owned(&[
                ("a", DataType::Float),
                ("b", DataType::Int),
                ("d", DataType::Bool),
            ]),
        )
        .unwrap();
    });
    d.redo();

    let new_id = field_ids(&d, r)[2];
    assert_eq!(new_id, floor, "taken from next_field_id");
    assert!(!ids.contains(&new_id), "c's id {} is not reused", ids[2]);
}

/// A field that takes another type drops its literal (it was written for the
/// old type); one that keeps its type keeps it.
#[test]
fn a_retype_in_the_list_drops_the_literal() {
    let mut d = three_field_network();
    let r = id(&d, "r");
    d.set_record_construct_fields(
        &[],
        r,
        owned(&[
            ("a", DataType::Int),
            ("b", DataType::Int),
            ("c", DataType::Vec3),
        ]),
    )
    .unwrap();
    assert!(rc(&d, r).literal_values.is_empty());
}

#[test]
fn invalid_field_lists_are_refused_and_change_nothing() {
    let mut d = three_field_network();
    let r = id(&d, "r");
    for (label, list) in [
        (
            "duplicate",
            owned(&[("a", DataType::Float), ("a", DataType::Int)]),
        ),
        ("reserved", owned(&[("schema", DataType::Float)])),
        ("not an identifier", owned(&[("a b", DataType::Float)])),
        ("leading digit", owned(&[("1a", DataType::Float)])),
        (
            "Optional",
            owned(&[("a", DataType::Optional(Box::new(DataType::Float)))]),
        ),
        ("Unit", owned(&[("a", DataType::Unit)])),
        ("None", owned(&[("a", DataType::None)])),
    ] {
        let before = fingerprint(&mut d);
        let pushes = d.undo_stack.push_count();
        assert!(
            d.set_record_construct_fields(&[], r, list).is_err(),
            "{} was accepted",
            label
        );
        assert_eq!(fingerprint(&mut d), before, "{} changed the network", label);
        assert_eq!(d.undo_stack.push_count(), pushes, "{} pushed", label);
    }

    // A named-mode node's fields are its def's.
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Box",
        vec![("a".to_string(), DataType::Float)],
    ))
    .unwrap();
    d.set_record_construct_schema(&[], r, "Box").unwrap();
    assert!(
        d.set_record_construct_fields(&[], r, owned(&[("q", DataType::Float)]))
            .is_err()
    );

    // An edit that changes nothing pushes nothing.
    let mut d = three_field_network();
    let r = id(&d, "r");
    let pushes = d.undo_stack.push_count();
    d.set_record_construct_fields(
        &[],
        r,
        owned(&[
            ("a", DataType::Float),
            ("b", DataType::Int),
            ("c", DataType::Vec3),
        ]),
    )
    .unwrap();
    assert_eq!(d.undo_stack.push_count(), pushes);
}

// ---------------------------------------------------------------------------
// Take type from wire
// ---------------------------------------------------------------------------

#[test]
fn taking_a_fields_type_keeps_its_pin_and_wire_and_drops_its_literal() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }] }
",
    );
    let (v, r) = (id(&d, "v"), id(&d, "r"));
    d.connect_nodes(v, 0, r, 0);
    set_literal(&mut d, r, "a", TextValue::Float(2.0));
    let pins = pin_ids(&d, r);
    assert!(has_mismatch(&d, r));
    assert_eq!(d.wire_type_offers(&[], r), vec![Some(DataType::Vec3)]);

    d.take_types_from_wires(&[], r, &[0]).unwrap();

    assert_eq!(fields(&d, r), owned(&[("a", DataType::Vec3)]));
    assert_eq!(pin_ids(&d, r), pins, "the pin id is kept");
    assert_eq!(sources_into(&d, r), vec![vec![v]], "the wire is kept");
    assert!(!has_mismatch(&d, r));
    assert!(
        rc(&d, r).literal_values.is_empty(),
        "the literal is dropped"
    );

    // Undo restores the type and the literal.
    d.undo();
    assert_eq!(fields(&d, r), owned(&[("a", DataType::Float)]));
    assert_eq!(
        rc(&d, r).literal_values.get("a"),
        Some(&TextValue::Float(2.0))
    );
    d.redo();
    d.undo();
    assert_undo_redo_roundtrip(&mut d, |d| {
        d.take_types_from_wires(&[], r, &[0]).unwrap();
    });
}

/// The design's rename scenario: a field rename changes the record's type,
/// the downstream `expr` keeps its wire and reports the mismatch, Take type
/// from wire repairs the parameter, and the expression's own field access is
/// the user's to fix.
#[test]
fn renaming_a_field_end_to_end() {
    let mut d = setup(
        "x = float { value: 1.5 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }], a: x }
e = expr { expression: \"1.0\", parameters: [] }
",
    );
    let (x, r, e) = (id(&d, "x"), id(&d, "r"), id(&d, "e"));
    let added = d.add_input_from_wire(&[], e, r, OUT, 0).unwrap();
    assert_eq!(added.name, "r");
    assert_eq!(
        expr_param_types(&d, e),
        vec![anon(&[("a", DataType::Float)])]
    );
    set_expression(&mut d, e, "r.a");
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));

    d.set_record_construct_fields(&[], r, owned(&[("b", DataType::Float)]))
        .unwrap();

    assert_eq!(sources_into(&d, r), vec![vec![x]], "r's own wire is kept");
    assert_eq!(sources_into(&d, e), vec![vec![r]], "e's wire is kept");
    assert!(has_mismatch(&d, e), "{:?}", errors_on(&d, e));
    assert_eq!(
        d.wire_type_offers(&[], e),
        vec![Some(anon(&[("b", DataType::Float)]))]
    );

    d.take_types_from_wires(&[], e, &[0]).unwrap();
    assert!(!has_mismatch(&d, e));
    // `r.a` now reports its own error: in the expression (what the editor
    // shows under the text) and when the node evaluates.
    let expr_error = d
        .get_node_network_data_scoped(&[], e)
        .unwrap()
        .as_any_ref()
        .downcast_ref::<ExprData>()
        .unwrap()
        .error
        .clone();
    assert!(
        expr_error.as_deref().is_some_and(|m| m.contains("'a'")),
        "{:?}",
        expr_error
    );
    assert!(matches!(eval(&d, e), NetworkResult::Error(_)));

    set_expression(&mut d, e, "r.b");
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
    assert!(matches!(eval(&d, e), NetworkResult::Float(v) if v == 1.5));
}

/// The design's add-a-field scenario: the wider record still fits the
/// declared parameter (width subtyping), so there is no error — but the
/// parameter is offered the wider type, and taking it lets the new field read.
#[test]
fn adding_a_field_end_to_end() {
    let mut d = setup(
        "x = float { value: 1.5 }
y = float { value: 2.5 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }], a: x }
e = expr { expression: \"1.0\", parameters: [] }
",
    );
    let (y, r, e) = (id(&d, "y"), id(&d, "r"), id(&d, "e"));
    d.add_input_from_wire(&[], e, r, OUT, 0).unwrap();
    set_expression(&mut d, e, "r.a");

    d.add_input_from_wire(&[], r, y, OUT, 0).unwrap();

    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
    let wider = anon(&[("a", DataType::Float), ("y", DataType::Float)]);
    assert_eq!(d.wire_type_offers(&[], e), vec![Some(wider.clone())]);

    d.take_types_from_wires(&[], e, &[0]).unwrap();
    assert_eq!(expr_param_types(&d, e), vec![wider]);
    set_expression(&mut d, e, "r.y");
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
    assert!(matches!(eval(&d, e), NetworkResult::Float(v) if v == 2.5));
}

// ---------------------------------------------------------------------------
// Switching the schema
// ---------------------------------------------------------------------------

/// `r` with fields `a` (fed by `x`, literal 9) and `b` (fed by `y`), and the
/// def `Foo { b, c }`.
fn mode_switch_network() -> StructureDesigner {
    let mut d = setup(
        "x = int { value: 1 }
y = int { value: 2 }
r = record_construct { fields: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }], a: x, b: y }
",
    );
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Foo",
        vec![
            ("b".to_string(), DataType::Int),
            ("c".to_string(), DataType::Int),
        ],
    ))
    .unwrap();
    let r = id(&d, "r");
    set_literal(&mut d, r, "a", TextValue::Int(9));
    d
}

fn assert_switched_to_foo(d: &StructureDesigner, r: u64) {
    let y = id(d, "y");
    let data = rc(d, r);
    assert_eq!(data.schema, "Foo");
    assert!(data.fields.is_empty(), "local fields are cleared");
    assert!(data.literal_values.is_empty(), "a's literal is dropped");
    assert_eq!(
        sources_into(d, r),
        vec![vec![y], vec![]],
        "b keeps its wire (by name); none lands on c"
    );
}

#[test]
fn picking_a_def_keeps_the_same_named_wires_and_undoes_in_one_step() {
    let mut d = mode_switch_network();
    let (x, y, r) = (id(&d, "x"), id(&d, "y"), id(&d, "r"));

    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_construct_schema(&[], r, "Foo").unwrap();
    });
    // The roundtrip leaves the switch undone: both wires, both local fields
    // and the literal are back.
    assert_eq!(sources_into(&d, r), vec![vec![x], vec![y]]);
    assert_eq!(rc(&d, r).literal_values.get("a"), Some(&TextValue::Int(9)));

    d.redo();
    assert_switched_to_foo(&d, r);
    assert_eq!(
        d.undo_stack.undo_description(),
        Some(SET_RECORD_SCHEMA_DESCRIPTION)
    );
}

/// The case the id base exists for: local ids that equal `Foo`'s `FieldId`s
/// numerically. Without the base, id-first matching would hand `a`'s wire to
/// `c`.
#[test]
fn local_ids_that_collide_with_field_ids_still_match_by_name() {
    let mut d = mode_switch_network();
    let r = id(&d, "r");
    let foo_ids: Vec<u64> = d
        .node_type_registry
        .lookup_record_type_def("Foo")
        .unwrap()
        .fields
        .iter()
        .map(|f| f.id.0)
        .collect();
    // a gets c's id, b gets a fresh one above both.
    let mut data = rc(&d, r).clone();
    let other = foo_ids.iter().max().unwrap() + 1;
    data.fields[0].id = foo_ids[1];
    data.fields[1].id = other;
    data.next_field_id = other + 1;
    d.set_node_network_data_scoped(&[], r, Box::new(data));
    d.validate_active_network();
    assert_eq!(
        sources_into(&d, r).iter().filter(|s| !s.is_empty()).count(),
        2
    );

    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_construct_schema(&[], r, "Foo").unwrap();
    });
    d.redo();
    assert_switched_to_foo(&d, r);
}

#[test]
fn a_named_to_named_switch_that_drops_a_wire_undoes_in_one_step() {
    let mut d = mode_switch_network();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Bar",
        vec![("b".to_string(), DataType::Int)],
    ))
    .unwrap();
    let (x, y, r) = (id(&d, "x"), id(&d, "y"), id(&d, "r"));
    d.set_record_construct_schema(&[], r, "Foo").unwrap();
    d.connect_nodes(x, 0, r, 1);
    assert_eq!(sources_into(&d, r), vec![vec![y], vec![x]]);

    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_construct_schema(&[], r, "Bar").unwrap();
    });
    d.redo();
    assert_eq!(sources_into(&d, r), vec![vec![y]]);
}

/// Picking "none" on a named node leaves an anonymous node with no fields
/// (and no literals), which outputs `{}` and has the open port.
#[test]
fn picking_no_schema_gives_an_empty_anonymous_node() {
    let mut d = mode_switch_network();
    let r = id(&d, "r");
    d.set_record_construct_schema(&[], r, "Foo").unwrap();
    set_literal(&mut d, r, "b", TextValue::Int(3));

    d.set_record_construct_schema(&[], r, "").unwrap();

    let data = rc(&d, r);
    assert!(data.is_anonymous() && data.fields.is_empty());
    assert!(data.literal_values.is_empty());
    assert!(sources_into(&d, r).is_empty());
    assert_eq!(output_type(&d, r), anon(&[]));
    assert!(network(&d).nodes[&r].data.declared_inputs().is_some());

    // Setting the current schema is a no-op.
    let pushes = d.undo_stack.push_count();
    d.set_record_construct_schema(&[], r, "").unwrap();
    assert_eq!(d.undo_stack.push_count(), pushes);
}

// ---------------------------------------------------------------------------
// Record-def names in local field types
// ---------------------------------------------------------------------------

/// `b` (a `Box` builder) dropped on the open port of `r`: `r` gets a field
/// typed `Box`.
fn box_field_network() -> StructureDesigner {
    let mut d = setup(
        "x = float { value: 1.0 }
b = record_construct {}
r = record_construct {}
",
    );
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Box",
        vec![("a".to_string(), DataType::Float)],
    ))
    .unwrap();
    let (x, b, r) = (id(&d, "x"), id(&d, "b"), id(&d, "r"));
    d.set_record_construct_schema(&[], b, "Box").unwrap();
    d.connect_nodes(x, 0, b, 0);
    d.add_input_from_wire(&[], r, b, OUT, 0).unwrap();
    assert_eq!(fields(&d, r), owned(&[("b", named("Box"))]));
    d
}

#[test]
fn a_def_rename_rewrites_a_local_field_type() {
    let mut d = box_field_network();
    let (b, r) = (id(&d, "b"), id(&d, "r"));

    d.rename_record_type_def("Box", "Crate").unwrap();

    assert_eq!(fields(&d, r), owned(&[("b", named("Crate"))]));
    assert_eq!(sources_into(&d, r), vec![vec![b]]);
    assert!(errors_on(&d, r).is_empty(), "{:?}", errors_on(&d, r));
}

#[test]
fn a_local_field_type_is_a_reference_to_its_def() {
    let mut d = box_field_network();
    let (b, r) = (id(&d, "b"), id(&d, "r"));

    let mut refs = HashSet::new();
    collect_record_refs_in_network(network(&d), &mut refs);
    assert!(refs.contains("Box"));
    assert!(!refs.contains(""), "an empty schema is never a reference");

    // With the builder gone, only the field names `Box`: the delete check
    // still sees it.
    d.select_node(b);
    d.delete_selected();
    assert_eq!(fields(&d, r), owned(&[("b", named("Box"))]));
    assert!(d.delete_record_type_def("Box").is_err());

    // An anonymous node whose fields name no def reports nothing at all.
    let plain = setup("p = record_construct { fields: [{ name: \"a\", data_type: Float }] }\n");
    let mut names = Vec::new();
    collect_record_refs_in_node(&network(&plain).nodes[&id(&plain, "p")], &mut |n, _| {
        names.push(n.to_string())
    });
    assert!(names.is_empty(), "{:?}", names);
}

/// A def that went missing behind the designer's back (a hand-edited file):
/// the dangling name in the field is reported on the node, so the invariant
/// checker's `UnresolvedRecordName` is accounted for, not silent.
#[test]
fn a_missing_def_in_a_field_type_is_reported_on_the_node() {
    let mut d = box_field_network();
    let (b, r) = (id(&d, "b"), id(&d, "r"));
    d.select_node(b);
    d.delete_selected();
    d.node_type_registry.record_type_defs.remove("Box");
    d.validate_active_network();

    assert!(
        errors_on(&d, r).iter().any(|e| e.contains("'Box'")),
        "{:?}",
        errors_on(&d, r)
    );
    let violations = check_network_invariants(network(&d), &d.node_type_registry);
    let unresolved: Vec<_> = violations
        .iter()
        .filter(|v| v.kind == InvariantKind::UnresolvedRecordName && v.node_id == Some(r))
        .collect();
    assert_eq!(unresolved.len(), 1, "{:?}", violations);
    assert!(
        !unresolved[0].is_fatal(),
        "accounted for by the node's error"
    );
}

/// A field typed by a linked library's def: when the library drops the def,
/// a refresh freezes the node, and its wires survive the refresh and a
/// save → reopen.
#[test]
fn a_field_typed_by_a_missing_linked_def_freezes_the_node() {
    let ws = tempfile::tempdir().unwrap();
    let lib = ws.path().join("lib.cnnd");
    {
        let mut l = new_design(&lib);
        l.add_record_type_def(RecordTypeDef::from_named_fields(
            "Box",
            vec![("a".to_string(), DataType::Float)],
        ))
        .unwrap();
        save(&mut l);
    }
    let host = ws.path().join("host.cnnd");
    {
        let mut h = new_design(&host);
        h.link_library("lib.cnnd", "lib").unwrap();
        edit_network(
            &mut h,
            NET,
            "x = float { value: 1.0 }
b = record_construct { schema: \"lib.Box\", a: x }
r = record_construct {}
",
        );
        let (b, r) = (id(&h, "b"), id(&h, "r"));
        h.add_input_from_wire(&[], r, b, OUT, 0).unwrap();
        assert_eq!(fields(&h, r), owned(&[("b", named("lib.Box"))]));
        save(&mut h);
    }
    {
        let mut l = open(&lib);
        l.delete_record_type_def("Box").unwrap();
        save(&mut l);
    }

    let mut h = open(&host);
    h.set_active_node_network_name(Some(NET.to_string()));
    h.refresh_library("lib").unwrap();
    let (b, r) = (id(&h, "b"), id(&h, "r"));
    assert!(is_frozen(&network(&h).nodes[&r], &h.node_type_registry));
    assert_eq!(sources_into(&h, r), vec![vec![b]], "refresh keeps the wire");

    save(&mut h);
    let mut h = open(&host);
    h.set_active_node_network_name(Some(NET.to_string()));
    let (b, r) = (id(&h, "b"), id(&h, "r"));
    assert!(is_frozen(&network(&h).nodes[&r], &h.node_type_registry));
    assert_eq!(sources_into(&h, r), vec![vec![b]], "reopen keeps the wire");
}

// ---------------------------------------------------------------------------
// Invariants and legacy files
// ---------------------------------------------------------------------------

fn violations_with(data: RecordConstructData) -> Vec<InvariantKind> {
    let mut d = setup("r = record_construct {}\n");
    let r = id(&d, "r");
    let net = d.node_type_registry.node_networks.get_mut(NET).unwrap();
    net.nodes.get_mut(&r).unwrap().data = Box::new(data);
    check_network_invariants(network(&d), &d.node_type_registry)
        .into_iter()
        .filter(|v| v.node_id == Some(r))
        .map(|v| v.kind)
        .collect()
}

fn local(id: u64, name: &str) -> LocalRecordField {
    LocalRecordField {
        id,
        name: name.to_string(),
        data_type: DataType::Float,
    }
}

#[test]
fn the_invariant_checker_sees_local_field_ids() {
    let duplicated = violations_with(RecordConstructData {
        fields: vec![local(3, "a"), local(3, "b")],
        next_field_id: 4,
        ..Default::default()
    });
    assert!(
        duplicated.contains(&InvariantKind::DuplicateFieldId),
        "{:?}",
        duplicated
    );

    let floor = violations_with(RecordConstructData {
        fields: vec![local(3, "a")],
        next_field_id: 3,
        ..Default::default()
    });
    assert!(floor.contains(&InvariantKind::FieldIdFloor), "{:?}", floor);

    let both = violations_with(RecordConstructData {
        schema: "Box".to_string(),
        fields: vec![local(0, "a")],
        next_field_id: 1,
        ..Default::default()
    });
    assert!(
        both.contains(&InvariantKind::NamedRecordWithLocalFields),
        "{:?}",
        both
    );
    assert!(InvariantKind::NamedRecordWithLocalFields.is_tier1());

    let healthy = violations_with(RecordConstructData {
        fields: vec![local(0, "a"), local(1, "b")],
        next_field_id: 2,
        ..Default::default()
    });
    assert!(healthy.is_empty(), "{:?}", healthy);
}

/// A `record_construct` saved before anonymous mode, with an empty schema,
/// loads as an anonymous node with no fields.
#[test]
fn a_legacy_empty_schema_loads_as_an_anonymous_node() {
    let d = StructureDesigner::new();
    let node_type = d
        .node_type_registry
        .built_in_node_types
        .get("record_construct")
        .unwrap();
    let json = serde_json::json!({ "schema": "", "literal_values": {} });
    let loaded = (node_type.node_data_loader)(&json, None).unwrap();
    let data = loaded
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .unwrap();
    assert!(data.is_anonymous());
    assert!(data.fields.is_empty());
    assert_eq!(data.next_field_id, 0);
}

/// Named-mode node data serializes exactly as before anonymous mode existed:
/// the two new keys are skipped while empty, so named-mode files do not churn.
#[test]
fn named_mode_data_serializes_without_the_new_keys() {
    let named = RecordConstructData {
        schema: "Box".to_string(),
        ..Default::default()
    };
    let json = serde_json::to_value(&named).unwrap();
    let keys: Vec<&String> = json.as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 2, "{:?}", keys);
    assert!(json.get("schema").is_some() && json.get("literal_values").is_some());
}

/// The `.cnnd` fixture: an anonymous node with a wired field, a field typed by
/// an anonymous record and an unwired field with a literal, read by an `expr`
/// whose parameter was declared by dropping the node's output on its open
/// port. It loads, evaluates, and survives save → reopen with its fields, ids,
/// literal and wires.
#[test]
fn the_anonymous_record_fixture_loads_evaluates_and_round_trips() {
    let path = atomcad_test_support::fixture_path(
        "record_construct_anonymous/record_construct_anonymous.cnnd",
    );
    let mut d = open(&path);
    d.set_active_node_network_name(Some(NET.to_string()));
    d.validate_active_network();
    let (r, e) = (id(&d, "r"), id(&d, "e"));
    assert_eq!(
        fields(&d, r),
        owned(&[
            ("b", DataType::Float),
            ("p", anon(&[("x", DataType::Int)])),
            ("c", DataType::Int),
        ])
    );
    assert_eq!(rc(&d, r).literal_values.get("c"), Some(&TextValue::Int(7)));
    assert!(
        network(&d).validation_errors.is_empty(),
        "{:?}",
        network(&d)
            .validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );
    assert!(matches!(eval(&d, e), NetworkResult::Float(v) if v == 12.5));

    let data = serde_json::to_value(rc(&d, r)).unwrap();
    let wires = (sources_into(&d, r), sources_into(&d, e));
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("copy.cnnd");
    d.save_node_networks_as(&copy.to_string_lossy()).unwrap();
    let mut d2 = open(&copy);
    d2.set_active_node_network_name(Some(NET.to_string()));
    let (r2, e2) = (id(&d2, "r"), id(&d2, "e"));
    assert_eq!(serde_json::to_value(rc(&d2, r2)).unwrap(), data);
    assert_eq!((sources_into(&d2, r2), sources_into(&d2, e2)), wires);
    assert!(matches!(eval(&d2, e2), NetworkResult::Float(v) if v == 12.5));
}
