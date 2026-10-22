//! Promoting an anonymous `record_construct` to a named def
//! (`doc/design_wire_declared_inputs.md`, Phase 3): "Name this record…"
//! creates the def from the node's fields and points the node at it in one
//! undo step, keeping every wire and literal; the dialog's "Same shape as:"
//! list offers the defs with exactly the node's fields, and choosing one is
//! the ordinary schema switch. The FFI pair is a thin wrapper and is not
//! tested separately.

use super::library_links_support::{edit as edit_network, new_design};
use crate::undo_test::assert_undo_redo_roundtrip;
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::node_type_registry::RecordTypeDef;
use atomcad_structure_designer::nodes::record_construct::RecordConstructData;
use atomcad_structure_designer::record_construct_ops::NAME_RECORD_DESCRIPTION;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::TextValue;

const NET: &str = "Main";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(code: &str) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    edit(&mut d, code);
    d
}

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

fn pin_names_and_ids(d: &StructureDesigner, node_id: u64) -> Vec<(String, Option<u64>)> {
    let node = &network(d).nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .parameters
        .iter()
        .map(|p| (p.name.clone(), p.id))
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

fn set_literal(d: &mut StructureDesigner, node_id: u64, field: &str, value: TextValue) {
    let mut data = rc(d, node_id).clone();
    data.literal_values.insert(field.to_string(), value);
    d.set_node_network_data_scoped(&[], node_id, Box::new(data));
}

fn def_fields(d: &StructureDesigner, name: &str) -> Vec<(String, DataType)> {
    d.node_type_registry
        .lookup_record_type_def(name)
        .unwrap_or_else(|| panic!("no def '{}'", name))
        .fields
        .iter()
        .map(|f| (f.name.clone(), f.data_type.clone()))
        .collect()
}

fn def_field_ids(d: &StructureDesigner, name: &str) -> Vec<u64> {
    d.node_type_registry
        .lookup_record_type_def(name)
        .unwrap_or_else(|| panic!("no def '{}'", name))
        .fields
        .iter()
        .map(|f| f.id.0)
        .collect()
}

fn owned(fields: &[(&str, DataType)]) -> Vec<(String, DataType)> {
    fields
        .iter()
        .map(|(n, t)| (n.to_string(), t.clone()))
        .collect()
}

fn anon(fields: &[(&str, DataType)]) -> DataType {
    DataType::Record(RecordType::anonymous(owned(fields)))
}

fn fingerprint(d: &mut StructureDesigner) -> String {
    serde_json::to_string(&d.snapshot_network(NET).unwrap()).unwrap()
}

/// A rendering of the record value that does not depend on how it was built.
fn record_value(value: &NetworkResult) -> String {
    assert!(matches!(value, NetworkResult::Record(_)), "not a record");
    value.to_display_string()
}

/// `r` with fields `b: Int` (fed by `y`), `a: Float` (unwired, literal 4.0)
/// and `c: Vec3` (fed by `z`) — authored order is not alphabetical — and an
/// `expr` `e` whose parameter `p` was declared with `r`'s anonymous type and
/// reads `p.a`.
fn promotion_network() -> StructureDesigner {
    let mut d = setup(
        "y = int { value: 2 }
z = vec3 { x: 1.0, y: 2.0, z: 3.0 }
r = record_construct { fields: [{ name: \"b\", data_type: Int }, { name: \"a\", data_type: Float }, { name: \"c\", data_type: Vec3 }], b: y, c: z }
e = expr { expression: \"p.a * 2.0\", parameters: [{ name: \"p\", data_type: {a: Float, b: Int, c: Vec3} }], p: r }
",
    );
    let r = id(&d, "r");
    set_literal(&mut d, r, "a", TextValue::Float(4.0));
    d.validate_active_network();
    d
}

// ---------------------------------------------------------------------------
// Promotion
// ---------------------------------------------------------------------------

#[test]
fn promotion_creates_the_def_and_keeps_every_wire_and_literal() {
    let mut d = promotion_network();
    // Another def of this file: the new def's field ids are claimed above
    // every id the file has handed out (`claim_field_ids`), not `0..n`.
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Old",
        owned(&[("u", DataType::Int), ("v", DataType::Int)]),
    ))
    .unwrap();
    let old_ids = def_field_ids(&d, "Old");

    let (r, e) = (id(&d, "r"), id(&d, "e"));
    let wires_before = sources_into(&d, r);
    let downstream_before = sources_into(&d, e);
    let value_before = record_value(&eval(&d, r));
    let next_field_id = rc(&d, r).next_field_id;

    d.promote_record_construct(&[], r, "Box").unwrap();

    assert_eq!(
        def_fields(&d, "Box"),
        owned(&[
            ("b", DataType::Int),
            ("a", DataType::Float),
            ("c", DataType::Vec3)
        ]),
        "the fields in authored order with the same types"
    );
    let ids = def_field_ids(&d, "Box");
    assert!(
        ids.iter().all(|id| old_ids.iter().all(|old| id > old)),
        "fresh field ids {:?}, above the other def's {:?}",
        ids,
        old_ids
    );
    let def = d.node_type_registry.lookup_record_type_def("Box").unwrap();
    assert!(def.fields.iter().all(|f| f.hint.is_none()), "no hints");

    let data = rc(&d, r);
    assert_eq!(data.schema, "Box");
    assert!(data.fields.is_empty(), "named mode has no local fields");
    assert_eq!(
        data.literal_values.get("a"),
        Some(&TextValue::Float(4.0)),
        "the literal survives"
    );
    assert_eq!(data.next_field_id, next_field_id, "the floor never drops");
    assert!(network(&d).nodes[&r].data.declared_inputs().is_none());

    assert_eq!(
        pin_names_and_ids(&d, r),
        vec![
            ("b".to_string(), Some(ids[0])),
            ("a".to_string(), Some(ids[1])),
            ("c".to_string(), Some(ids[2])),
        ],
        "the pins carry the def's field ids"
    );
    assert_eq!(
        output_type(&d, r),
        DataType::Record(RecordType::Named("Box".to_string()))
    );
    assert_eq!(
        sources_into(&d, r),
        wires_before,
        "every input wire survives"
    );
    assert_eq!(
        sources_into(&d, e),
        downstream_before,
        "the downstream wire survives"
    );
    assert!(errors_on(&d, r).is_empty(), "{:?}", errors_on(&d, r));
    assert_eq!(
        record_value(&eval(&d, r)),
        value_before,
        "eval is unchanged"
    );
}

/// Downstream nodes are not touched: an `expr` whose parameter was declared
/// with the anonymous type keeps that type, still validates, still reads
/// `p.a`, and is offered no retype (the named source has exactly its shape).
#[test]
fn a_downstream_expr_declared_with_the_anonymous_type_keeps_working() {
    let mut d = promotion_network();
    let (r, e) = (id(&d, "r"), id(&d, "e"));
    assert!(matches!(eval(&d, e), NetworkResult::Float(v) if v == 8.0));

    d.promote_record_construct(&[], r, "Box").unwrap();

    let parameter_type = d
        .get_node_network_data_scoped(&[], e)
        .unwrap()
        .as_any_ref()
        .downcast_ref::<atomcad_structure_designer::nodes::expr::ExprData>()
        .unwrap()
        .parameters[0]
        .data_type
        .clone();
    assert_eq!(
        parameter_type,
        anon(&[
            ("a", DataType::Float),
            ("b", DataType::Int),
            ("c", DataType::Vec3)
        ])
    );
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
    assert!(matches!(eval(&d, e), NetworkResult::Float(v) if v == 8.0));
    assert!(d.wire_type_offers(&[], e).iter().all(Option::is_none));
}

/// One undo step removes the def and restores the anonymous node with its
/// wires; redo brings both back, the def with the same field ids.
/// `assert_undo_redo_roundtrip` compares networks only, so the registry is
/// checked here.
#[test]
fn promotion_undoes_and_redoes_in_one_step() {
    let mut d = promotion_network();
    let r = id(&d, "r");
    let local_before = rc(&d, r).fields.clone();
    let wires_before = sources_into(&d, r);
    let mut promoted_ids = Vec::new();

    assert_undo_redo_roundtrip(&mut d, |d| {
        d.promote_record_construct(&[], r, "Box").unwrap();
        promoted_ids = def_field_ids(d, "Box");
    });

    // The roundtrip leaves the promotion undone.
    assert!(d.node_type_registry.lookup_record_type_def("Box").is_none());
    let data = rc(&d, r);
    assert!(data.is_anonymous());
    assert_eq!(data.fields, local_before);
    assert_eq!(sources_into(&d, r), wires_before);
    assert_eq!(
        d.undo_stack.redo_description(),
        Some(NAME_RECORD_DESCRIPTION)
    );

    d.redo();
    assert_eq!(def_field_ids(&d, "Box"), promoted_ids);
    assert_eq!(rc(&d, r).schema, "Box");
    assert_eq!(sources_into(&d, r), wires_before);
    assert!(errors_on(&d, r).is_empty(), "{:?}", errors_on(&d, r));
    assert_eq!(
        d.undo_stack.undo_description(),
        Some(NAME_RECORD_DESCRIPTION)
    );
}

/// Each refusal leaves no def, no undo step and an unchanged network.
#[test]
fn refusals_create_no_def_and_push_nothing() {
    let mut d = promotion_network();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Taken",
        owned(&[("a", DataType::Float)]),
    ))
    .unwrap();
    edit(
        &mut d,
        "n = record_construct { schema: \"Taken\" }
empty = record_construct {}
",
    );
    let (r, n, empty) = (id(&d, "r"), id(&d, "n"), id(&d, "empty"));

    let cases: Vec<(&str, u64, &str)> = vec![
        ("a def's name", r, "Taken"),
        ("a network's name", r, NET),
        ("a built-in def's name", r, "ElementMapping"),
        ("an invalid name", r, ""),
        ("an invalid name", r, " Box"),
        ("a named-mode node", n, "Box"),
        ("a node with no fields", empty, "Box"),
    ];
    for (label, node_id, name) in cases {
        let defs = d.node_type_registry.record_type_defs.len();
        let pushes = d.undo_stack.push_count();
        let before = fingerprint(&mut d);
        assert!(
            d.promote_record_construct(&[], node_id, name).is_err(),
            "{}: '{}' should be refused",
            label,
            name
        );
        assert_eq!(
            d.node_type_registry.record_type_defs.len(),
            defs,
            "{}",
            label
        );
        assert_eq!(d.undo_stack.push_count(), pushes, "{}", label);
        assert_eq!(fingerprint(&mut d), before, "{}", label);
    }
    assert!(d.node_type_registry.lookup_record_type_def("Box").is_none());
}

// ---------------------------------------------------------------------------
// "Same shape as:"
// ---------------------------------------------------------------------------

#[test]
fn matching_lists_exactly_the_defs_with_the_same_fields() {
    let mut d = setup(
        "x = float { value: 1.0 }
i = int { value: 2 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Int }], a: x, b: i }
",
    );
    let defs: Vec<(&str, Vec<(String, DataType)>)> = vec![
        (
            "Exact",
            owned(&[("a", DataType::Float), ("b", DataType::Int)]),
        ),
        (
            "ExactReordered",
            owned(&[("b", DataType::Int), ("a", DataType::Float)]),
        ),
        (
            "Wider",
            owned(&[
                ("a", DataType::Float),
                ("b", DataType::Int),
                ("c", DataType::Int),
            ]),
        ),
        ("Narrower", owned(&[("a", DataType::Float)])),
        (
            "OtherType",
            owned(&[("a", DataType::Float), ("b", DataType::Float)]),
        ),
        (
            "OtherName",
            owned(&[("a", DataType::Float), ("bb", DataType::Int)]),
        ),
        (
            "WithOptional",
            owned(&[
                ("a", DataType::Float),
                ("b", DataType::Optional(Box::new(DataType::Int))),
            ]),
        ),
    ];
    for (name, fields) in defs {
        d.add_record_type_def(RecordTypeDef::from_named_fields(name, fields))
            .unwrap();
    }
    let r = id(&d, "r");
    assert_eq!(
        d.matching_record_defs_for(&[], r),
        vec!["Exact".to_string(), "ExactReordered".to_string()]
    );

    // A named-mode node and a node with no fields list nothing.
    d.set_record_construct_schema(&[], r, "Exact").unwrap();
    assert!(d.matching_record_defs_for(&[], r).is_empty());
    edit(&mut d, "empty = record_construct {}\n");
    let empty = id(&d, "empty");
    assert!(d.matching_record_defs_for(&[], empty).is_empty());
}

/// Using a listed def is the ordinary schema switch: the pins take the def's
/// authored order and every wire follows its field by name.
#[test]
fn using_an_existing_def_reorders_the_pins_and_keeps_the_wires() {
    let mut d = setup(
        "x = float { value: 1.0 }
i = int { value: 2 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Int }], a: x, b: i }
",
    );
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Pair",
        owned(&[("b", DataType::Int), ("a", DataType::Float)]),
    ))
    .unwrap();
    let (x, i, r) = (id(&d, "x"), id(&d, "i"), id(&d, "r"));
    assert_eq!(d.matching_record_defs_for(&[], r), vec!["Pair".to_string()]);

    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_construct_schema(&[], r, "Pair").unwrap();
    });
    d.redo();

    let names: Vec<String> = pin_names_and_ids(&d, r)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, vec!["b".to_string(), "a".to_string()]);
    assert_eq!(sources_into(&d, r), vec![vec![i], vec![x]]);
    assert!(errors_on(&d, r).is_empty(), "{:?}", errors_on(&d, r));
}

// ---------------------------------------------------------------------------
// Linked libraries
// ---------------------------------------------------------------------------

/// A linked def of the same shape is listed, and a name inside the linked
/// library is refused.
#[test]
fn linked_defs_are_listed_and_a_linked_name_is_refused() {
    let ws = tempfile::tempdir().unwrap();
    {
        let mut lib = new_design(&ws.path().join("lib.cnnd"));
        lib.add_record_type_def(RecordTypeDef::from_named_fields(
            "Pair",
            owned(&[("a", DataType::Float), ("b", DataType::Int)]),
        ))
        .unwrap();
        super::library_links_support::save(&mut lib);
    }
    let mut h = new_design(&ws.path().join("host.cnnd"));
    h.link_library("lib.cnnd", "lib").unwrap();
    edit_network(
        &mut h,
        NET,
        "x = float { value: 1.0 }
i = int { value: 2 }
r = record_construct { fields: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Int }], a: x, b: i }
",
    );
    h.set_active_node_network_name(Some(NET.to_string()));
    let r = id(&h, "r");
    assert_eq!(
        h.matching_record_defs_for(&[], r),
        vec!["lib.Pair".to_string()]
    );

    let pushes = h.undo_stack.push_count();
    let defs = h.node_type_registry.record_type_defs.len();
    assert!(h.promote_record_construct(&[], r, "lib.Mine").is_err());
    assert!(h.promote_record_construct(&[], r, "lib").is_err());
    assert_eq!(h.undo_stack.push_count(), pushes);
    assert_eq!(h.node_type_registry.record_type_defs.len(), defs);
    assert!(rc(&h, r).is_anonymous());

    // Pointing the node at the linked def keeps its wires.
    h.set_record_construct_schema(&[], r, "lib.Pair").unwrap();
    assert_eq!(
        sources_into(&h, r).iter().filter(|s| !s.is_empty()).count(),
        2
    );
}
