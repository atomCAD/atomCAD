//! `record_set` (`doc/design_record_get_set.md` Phase 2): the node and
//! `NetworkResult::with_record_path_replaced`.
//!
//! Most tests build `Box { p: Point, size: Float }` / `Point { x, y: Float }`
//! and a `record_set` `s` on `Box` fed by a `Box` value `b` (`p = (1.5, 2.5)`,
//! `size = 4.0`), then read the fields of `s`'s output.

use super::library_links_support::*;
use atomcad_structure_designer::data_type::{DataType, FunctionType, RecordType};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::library_links::is_frozen;
use atomcad_structure_designer::node_data::DragDirection;
use atomcad_structure_designer::node_network::{FunctionPinRole, NodeNetwork};
use atomcad_structure_designer::node_type_registry::{RecordFieldEdit, RecordTypeDef};
use atomcad_structure_designer::nodes::record_set::RecordSetData;
use atomcad_structure_designer::structure_designer::{DragSource, StructureDesigner};
use atomcad_structure_designer::text_format::TextValue;
use glam::f64::DVec2;
use std::path::Path;

// ============================================================================
// Helpers
// ============================================================================

fn named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

fn floats(name: &str, fields: &[&str]) -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        name,
        fields
            .iter()
            .map(|f| (f.to_string(), DataType::Float))
            .collect(),
    )
}

fn box_def(name: &str, point: &str) -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        name,
        vec![
            ("p".to_string(), named(point)),
            ("size".to_string(), DataType::Float),
        ],
    )
}

/// A designer with `Point` and `Box` and an active, empty `Main`.
fn setup() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network("Main");
    d.set_active_node_network_name(Some("Main".to_string()));
    d.add_record_type_def(floats("Point", &["x", "y"])).unwrap();
    d.add_record_type_def(box_def("Box", "Point")).unwrap();
    d
}

/// The `Box` value `b`: `p = (1.5, 2.5)`, `size = 4.0`; plus two spare
/// floats, `f9 = 9.0` and `f7 = 7.0`.
const BOX_VALUE: &str = "fx = float { value: 1.5 }
fy = float { value: 2.5 }
fs = float { value: 4.0 }
f9 = float { value: 9.0 }
f7 = float { value: 7.0 }
pt = record_construct { schema: \"Point\", x: fx, y: fy }
b = record_construct { schema: \"Box\", p: pt, size: fs }
";

/// `setup` plus `BOX_VALUE` plus `code`.
fn with(code: &str) -> StructureDesigner {
    let mut d = setup();
    edit(&mut d, "Main", &format!("{}{}", BOX_VALUE, code));
    d
}

/// The standard network: rows `[p.x, size]`, `p_x` ← `f9`, `size` ← `f7`.
fn standard() -> StructureDesigner {
    with(
        "s = record_set { schema: \"Box\", paths: [\"p.x\", \"size\"], record: b, p_x: f9, size: f7 }\n",
    )
}

fn network<'a>(d: &'a StructureDesigner, name: &str) -> &'a NodeNetwork {
    &d.node_type_registry.node_networks[name]
}

fn in_pins_in(net: &NodeNetwork, id: u64) -> Vec<String> {
    net.nodes[&id]
        .custom_node_type
        .as_ref()
        .expect("record_set has a cached layout")
        .parameters
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

fn in_pins(d: &StructureDesigner, name: &str) -> Vec<String> {
    in_pins_in(network(d, "Main"), node_id(d, "Main", name))
}

/// The source nodes wired into `arg` of `name` in `Main`.
fn sources(d: &StructureDesigner, name: &str, arg: usize) -> Vec<u64> {
    network(d, "Main").nodes[&node_id(d, "Main", name)]
        .arguments
        .get(arg)
        .map(|a| a.incoming_wires.iter().map(|w| w.source_node_id).collect())
        .unwrap_or_default()
}

/// Whether input `arg` of `name` reads `source`.
fn reads(d: &StructureDesigner, name: &str, arg: usize, source: &str) -> bool {
    sources(d, name, arg).contains(&node_id(d, "Main", source))
}

/// `(text, blocking)` of every validation error on `name`.
fn errors_on(d: &StructureDesigner, name: &str) -> Vec<(String, bool)> {
    let id = node_id(d, "Main", name);
    network(d, "Main")
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(id))
        .map(|e| (e.error_text.clone(), e.blocking))
        .collect()
}

fn value(d: &mut StructureDesigner, name: &str, pin: i32) -> NetworkResult {
    d.set_active_node_network_name(Some("Main".to_string()));
    let id = node_id(d, "Main", name);
    d.evaluate_node_output(&[], id, pin)
}

/// The field at `path` of a record value, as a float.
fn field(v: &NetworkResult, path: &[&str]) -> f64 {
    match v.extract_record_path(path) {
        Some(NetworkResult::Float(f)) => *f,
        other => panic!(
            "no Float at {:?} in {}: {:?}",
            path,
            v.to_display_string(),
            other.map(|o| o.to_display_string())
        ),
    }
}

fn s_data(d: &StructureDesigner) -> RecordSetData {
    network(d, "Main").nodes[&node_id(d, "Main", "s")]
        .data
        .as_any_ref()
        .downcast_ref::<RecordSetData>()
        .unwrap()
        .clone()
}

fn s_data_mut(d: &mut StructureDesigner) -> &mut RecordSetData {
    let id = node_id(d, "Main", "s");
    d.node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .get_mut(&id)
        .unwrap()
        .data
        .as_any_mut()
        .downcast_mut::<RecordSetData>()
        .unwrap()
}

fn set_fields(d: &mut StructureDesigner, def: &str, fields: &[&str]) {
    d.update_record_type_def(
        def,
        fields
            .iter()
            .map(|f| (f.to_string(), DataType::Float))
            .collect(),
    )
    .unwrap();
}

fn existing(d: &StructureDesigner, def: &str, field: &str, new_name: &str) -> RecordFieldEdit {
    let f = d
        .node_type_registry
        .lookup_record_type_def(def)
        .unwrap()
        .fields
        .iter()
        .find(|f| f.name == field)
        .unwrap()
        .clone();
    RecordFieldEdit {
        id: Some(f.id),
        name: new_name.to_string(),
        data_type: f.data_type,
        hint: None,
    }
}

fn output_type(d: &StructureDesigner, name: &str) -> Option<DataType> {
    let net = network(d, "Main");
    let node = &net.nodes[&node_id(d, "Main", name)];
    d.node_type_registry.resolve_output_type(node, net, 0)
}

fn expect_error(v: NetworkResult) -> String {
    match v {
        NetworkResult::Error(e) => e,
        other => panic!("expected an Error, got {}", other.to_display_string()),
    }
}

// ============================================================================
// 1. Top-level and nested fields; pass-through
// ============================================================================

#[test]
fn sets_a_top_level_and_a_nested_field_and_passes_the_rest_through() {
    let mut d = standard();
    assert_eq!(in_pins(&d, "s"), vec!["record", "p_x", "size"]);
    let s = &network(&d, "Main").nodes[&node_id(&d, "Main", "s")];
    let nt = s.custom_node_type.as_ref().unwrap();
    assert_eq!(nt.parameters[0].data_type, named("Box"));
    assert_eq!(nt.parameters[0].id, None);
    assert!(nt.parameters[1..].iter().all(|p| p.id.is_some()));
    assert!(
        nt.parameters[1..]
            .iter()
            .all(|p| p.data_type == DataType::Float)
    );
    assert_eq!(output_type(&d, "s"), Some(named("Box")));
    assert!(errors_on(&d, "s").is_empty(), "{:?}", errors_on(&d, "s"));

    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "x"]), 9.0);
    assert_eq!(
        field(&v, &["p", "y"]),
        2.5,
        "a sibling in the nested record"
    );
    assert_eq!(field(&v, &["size"]), 7.0);
}

#[test]
fn extra_runtime_fields_pass_through() {
    let mut d = setup();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Tagged",
        vec![
            ("p".to_string(), named("Point")),
            ("size".to_string(), DataType::Float),
            ("tag".to_string(), DataType::Int),
        ],
    ))
    .unwrap();
    edit(
        &mut d,
        "Main",
        &format!(
            "{}tg = int {{ value: 5 }}
t = record_construct {{ schema: \"Tagged\", p: pt, size: fs, tag: tg }}
s = record_set {{ schema: \"Box\", paths: [\"p.x\"], record: t, p_x: f9 }}
",
            BOX_VALUE
        ),
    );
    assert!(reads(&d, "s", 0, "t"), "a wider record fits the schema");
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "x"]), 9.0);
    assert!(matches!(
        v.extract_record_field("tag"),
        Some(NetworkResult::Int(5))
    ));
}

#[test]
fn a_fresh_record_set_is_an_identity_on_its_schema() {
    let mut d = with("s = record_set { schema: \"Box\", record: b }\n");
    assert_eq!(in_pins(&d, "s"), vec!["record"]);
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "x"]), 1.5);
    assert_eq!(field(&v, &["size"]), 4.0);

    edit(&mut d, "Main", "s = record_set {}\n");
    assert!(matches!(value(&mut d, "s", 0), NetworkResult::None));
}

// ============================================================================
// 2. Conversion at the wire
// ============================================================================

#[test]
fn an_int_wired_into_a_float_leaf_converts() {
    let mut d = with(
        "i = int { value: 3 }
s = record_set { schema: \"Box\", paths: [\"size\"], record: b, size: i }
",
    );
    assert!(reads(&d, "s", 1, "i"));
    assert!(errors_on(&d, "s").is_empty(), "{:?}", errors_on(&d, "s"));
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["size"]), 3.0);
}

// ============================================================================
// 3. Literals and no-op rows
// ============================================================================

#[test]
fn an_unwired_row_uses_its_literal_or_leaves_the_field_unchanged() {
    let mut d = with("s = record_set { schema: \"Box\", paths: [\"p.x\", \"size\"], record: b }\n");
    // No literal: both rows are no-ops.
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "x"]), 1.5);
    assert_eq!(field(&v, &["size"]), 4.0);

    // A literal on `size` (the text format stores none until Phase 3).
    s_data_mut(&mut d).rows[1].literal = Some(TextValue::Float(6.5));
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["size"]), 6.5);
    assert_eq!(field(&v, &["p", "x"]), 1.5);

    // An Int literal converts to the Float pin; a String one does not and
    // leaves the field alone.
    s_data_mut(&mut d).rows[1].literal = Some(TextValue::Int(2));
    assert_eq!(field(&value(&mut d, "s", 0), &["size"]), 2.0);
    s_data_mut(&mut d).rows[1].literal = Some(TextValue::String("no".to_string()));
    assert_eq!(field(&value(&mut d, "s", 0), &["size"]), 4.0);

    // A wire beats the literal.
    s_data_mut(&mut d).rows[1].literal = Some(TextValue::Float(6.5));
    let outcome = d.ai_text_edit("s = record_set { size: f7 }\n", false);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    assert_eq!(s_data(&d).rows[1].literal, Some(TextValue::Float(6.5)));
    assert_eq!(field(&value(&mut d, "s", 0), &["size"]), 7.0);
}

// ============================================================================
// 4. Overlapping rows apply top to bottom
// ============================================================================

#[test]
fn overlapping_rows_apply_top_to_bottom() {
    let new_point = "q = record_construct { schema: \"Point\", x: f7, y: f7 }\n";
    let mut d = with(&format!(
        "{}s = record_set {{ schema: \"Box\", paths: [\"p\", \"p.x\"], record: b, p: q, p_x: f9 }}\n",
        new_point
    ));
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "x"]), 9.0, "p.x adjusts the new p");
    assert_eq!(field(&v, &["p", "y"]), 7.0);

    let mut d = with(&format!(
        "{}s = record_set {{ schema: \"Box\", paths: [\"p.x\", \"p\"], record: b, p: q, p_x: f9 }}\n",
        new_point
    ));
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "x"]), 7.0, "p replaces the adjusted p");
    assert_eq!(field(&v, &["p", "y"]), 7.0);
}

// ============================================================================
// 5. None
// ============================================================================

fn with_opt(code: &str) -> StructureDesigner {
    let mut d = setup();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Opt",
        vec![
            (
                "v".to_string(),
                DataType::Optional(Box::new(DataType::Float)),
            ),
            ("w".to_string(), DataType::Float),
        ],
    ))
    .unwrap();
    // `gv` is a Float pin carrying `None` (the unset `v` of `unset`).
    edit(
        &mut d,
        "Main",
        &format!(
            "{}unset = record_construct {{ schema: \"Opt\", w: fs }}
gv = record_get {{ schema: \"Opt\", paths: [\"v\"], record: unset }}
{}",
            BOX_VALUE, code
        ),
    );
    d
}

#[test]
fn none_on_a_required_leaf_collapses_the_output() {
    let mut d =
        with_opt("s = record_set { schema: \"Box\", paths: [\"size\"], record: b, size: gv }\n");
    assert!(matches!(value(&mut d, "gv", 0), NetworkResult::None));
    assert!(matches!(value(&mut d, "s", 0), NetworkResult::None));
}

#[test]
fn none_on_an_optional_leaf_unsets_the_field() {
    let mut d = with_opt(
        "full = record_construct { schema: \"Opt\", v: f9, w: fs }
s = record_set { schema: \"Opt\", paths: [\"v\"], record: full, v: gv }
",
    );
    // The pin of an `Optional[Float]` leaf is a plain Float.
    let s = &network(&d, "Main").nodes[&node_id(&d, "Main", "s")];
    assert_eq!(
        s.custom_node_type.as_ref().unwrap().parameters[1].data_type,
        DataType::Float
    );
    let full = value(&mut d, "full", 0);
    assert_eq!(field(&full, &["v"]), 9.0);
    let v = value(&mut d, "s", 0);
    assert!(
        matches!(v.extract_record_field("v"), Some(NetworkResult::None)),
        "an explicit None: {}",
        v.to_display_string()
    );
    assert_eq!(field(&v, &["w"]), 4.0);
}

// ============================================================================
// 6. Errors are forwarded verbatim
// ============================================================================

#[test]
fn errors_on_the_record_or_a_row_are_forwarded_verbatim() {
    let mut d = with(
        "boxes = array { element_type: Record(Box), elements: [] }
nob = array_at { array: boxes, element_type: Record(Box), index: 0 }
fl = array { element_type: Float, elements: [] }
nof = array_at { array: fl, element_type: Float, index: 0 }
s = record_set { schema: \"Box\", paths: [\"size\"], record: nob, size: f7 }
s2 = record_set { schema: \"Box\", paths: [\"size\"], record: b, size: nof }
",
    );
    let upstream = expect_error(value(&mut d, "nob", 0));
    let e = expect_error(value(&mut d, "s", 0));
    assert!(e.contains(&upstream), "{} / {}", e, upstream);
    assert_eq!(
        e.matches("error in").count(),
        upstream.matches("error in").count() + 1,
        "one wrap, by the evaluator's chaining hub, none by the node: {}",
        e
    );

    let upstream = expect_error(value(&mut d, "nof", 0));
    let e = expect_error(value(&mut d, "s2", 0));
    assert!(e.contains(&upstream), "{} / {}", e, upstream);
    assert_eq!(
        e.matches("error in").count(),
        upstream.matches("error in").count() + 1,
        "{}",
        e
    );
}

// ============================================================================
// 7. Function pin into map.f
// ============================================================================

/// Two boxes through `sequence`, and `m` = `map` over them with `f: @s`;
/// with `collect`, `c` collects `m` into an `Array[Box]`.
fn mapped(s_code: &str, collect: bool) -> StructureDesigner {
    with(&format!(
        "b2 = record_construct {{ schema: \"Box\", p: pt, size: f9 }}
seq = sequence {{ element_type: Record(Box), count: 2, `0`: b, `1`: b2 }}
{}m = map {{ xs: seq, input_type: Record(Box), output_type: Record(Box), f: @s }}
{}",
        s_code,
        if collect {
            "c = collect { iter: m, element_type: Record(Box) }\n"
        } else {
            ""
        }
    ))
}

#[test]
fn the_function_pin_into_map_updates_every_record() {
    let mut d = mapped(
        "s = record_set { schema: \"Box\", paths: [\"size\"], size: f7 }\n",
        true,
    );
    assert_eq!(
        output_type(&d, "m"),
        Some(DataType::Iterator(Box::new(named("Box"))))
    );
    let errors: Vec<_> = network(&d, "Main")
        .validation_errors
        .iter()
        .map(|e| e.error_text.clone())
        .collect();
    assert!(errors.is_empty(), "{:?}", errors);
    match value(&mut d, "c", 0) {
        NetworkResult::Array(items) => {
            assert_eq!(items.len(), 2);
            for item in &items {
                assert_eq!(field(item, &["size"]), 7.0, "the captured wire");
            }
            assert_eq!(field(&items[0], &["p", "x"]), 1.5);
        }
        other => panic!("{}", other.to_display_string()),
    }
}

#[test]
fn an_unwired_row_becomes_a_parameter_of_the_mapped_function() {
    let d = mapped(
        "s = record_set { schema: \"Box\", paths: [\"p.x\", \"size\"], p_x: f9 }\n",
        false,
    );
    let partial = DataType::Function(FunctionType::new(vec![DataType::Float], named("Box")));
    assert_eq!(
        output_type(&d, "m"),
        Some(DataType::Iterator(Box::new(partial)))
    );
}

// ============================================================================
// 8. Leaf retype
// ============================================================================

/// The design doc said repair drops the incompatible wire; the codebase keeps
/// a mistyped wire and flags it instead, as for `record_construct`
/// (`record_field_rename_wire_loss_test.rs`, the incompatible-retype guard).
#[test]
fn a_leaf_retype_retypes_the_pin_and_flags_an_incompatible_wire() {
    let mut d = standard();
    let s_id = node_id(&d, "Main", "s");
    d.update_record_type_def(
        "Box",
        vec![
            ("p".to_string(), named("Point")),
            ("size".to_string(), DataType::String),
        ],
    )
    .unwrap();
    d.validate_active_network();
    let s = &network(&d, "Main").nodes[&s_id];
    assert_eq!(
        s.custom_node_type.as_ref().unwrap().parameters[2].data_type,
        DataType::String
    );
    assert!(reads(&d, "s", 2, "f7"), "the wire is kept, by row id");
    assert!(reads(&d, "s", 1, "f9"), "the other row is untouched");
    let errors = errors_on(&d, "s");
    assert!(
        errors.iter().any(|(_, blocking)| *blocking),
        "the Float wire into a String pin is a type error: {:?}",
        errors
    );

    // A compatible retype keeps the wire with no error: Float → Vec3 does
    // not exist, but a `Float` leaf retyped to `Unit` (the discard
    // widening) accepts any wire.
    d.update_record_type_def(
        "Box",
        vec![
            ("p".to_string(), named("Point")),
            ("size".to_string(), DataType::Unit),
        ],
    )
    .unwrap();
    d.validate_active_network();
    assert!(reads(&d, "s", 2, "f7"));
    assert!(errors_on(&d, "s").is_empty(), "{:?}", errors_on(&d, "s"));
}

// ============================================================================
// 9. Nested rename / delete; frozen nodes
// ============================================================================

#[test]
fn a_nested_rename_keeps_the_input_wire() {
    let mut d = standard();
    let edits = vec![
        existing(&d, "Point", "x", "px"),
        existing(&d, "Point", "y", "y"),
    ];
    d.update_record_type_def_with_ids("Point", edits).unwrap();
    d.validate_active_network();
    assert_eq!(in_pins(&d, "s"), vec!["record", "p_px", "size"]);
    assert!(reads(&d, "s", 1, "f9"));
    assert!(reads(&d, "s", 2, "f7"));
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "px"]), 9.0);
}

#[test]
fn a_nested_delete_dangles_the_row_and_the_other_row_still_works() {
    let mut d = standard();
    set_fields(&mut d, "Point", &["y"]);
    d.validate_active_network();
    assert_eq!(in_pins(&d, "s"), vec!["record", "size"]);
    assert!(reads(&d, "s", 1, "f7"), "size's wire moved with its pin");
    assert!(
        !network(&d, "Main").nodes[&node_id(&d, "Main", "s")]
            .arguments
            .iter()
            .any(|a| a
                .incoming_wires
                .iter()
                .any(|w| w.source_node_id == node_id(&d, "Main", "f9"))),
        "the dangling row's wire is dropped"
    );
    let errors = errors_on(&d, "s");
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert!(!errors[0].1, "a dangling row is a warning");
    assert!(errors[0].0.contains("`p.x`"), "{}", errors[0].0);
    assert_eq!(s_data(&d).rows.len(), 2, "the row is kept");
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["size"]), 7.0);

    assert!(d.undo());
    d.validate_active_network();
    assert_eq!(in_pins(&d, "s"), vec!["record", "p_x", "size"]);
    assert!(reads(&d, "s", 1, "f9"));
    assert!(reads(&d, "s", 2, "f7"));
    assert!(errors_on(&d, "s").is_empty());
}

/// `lib.cnnd` with `Point` (and `Box` on the linked `Point`).
fn write_lib(path: &Path, with_box: bool) {
    let mut d = new_design(path);
    d.add_record_type_def(floats("Point", &["x", "y"])).unwrap();
    if with_box {
        d.add_record_type_def(box_def("Box", "Point")).unwrap();
    }
    d.delete_node_network("Main").ok();
    save(&mut d);
}

fn s_frozen(d: &StructureDesigner) -> bool {
    let s = node_id(d, "Main", "s");
    is_frozen(&network(d, "Main").nodes[&s], &d.node_type_registry)
}

/// The host's `s` on `schema`, rows `[p.x, size]` fed by `f9` / `f7`, and
/// (with `point_user`) a node on `l.Point` so `uses` records it.
fn host_code(schema: &str, point_user: bool) -> String {
    let mut code = format!(
        "f9 = float {{ value: 9.0 }}
f7 = float {{ value: 7.0 }}
s = record_set {{ schema: \"{}\", paths: [\"p.x\", \"size\"], p_x: f9, size: f7 }}
",
        schema
    );
    if point_user {
        code.push_str("pu = record_get { schema: \"l.Point\", paths: [\"x\"] }\n");
    }
    code
}

fn assert_frozen_and_wired(d: &mut StructureDesigner) {
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert!(s_frozen(d));
    let errors = errors_on(d, "s");
    assert_eq!(errors.len(), 1, "only the frozen-node error: {:?}", errors);
    assert!(errors[0].1);
    assert!(reads(d, "s", 1, "f9"));
    assert!(reads(d, "s", 2, "f7"));
}

#[test]
fn a_frozen_record_set_keeps_its_nested_input_wire() {
    for point_user in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("lib.cnnd");
        let host = dir.path().join("host.cnnd");
        write_lib(&lib, true);
        let mut d = new_design(&host);
        d.link_library("lib.cnnd", "l").unwrap();
        edit(&mut d, "Main", &host_code("l.Box", point_user));
        assert_eq!(in_pins(&d, "s"), vec!["record", "p_x", "size"]);
        save(&mut d);

        std::fs::remove_file(&lib).unwrap();
        let mut d = open(&host);
        assert_frozen_and_wired(&mut d);
        if point_user {
            // Both recorded interfaces: the pins are rebuilt from them.
            assert_eq!(in_pins(&d, "s"), vec!["record", "p_x", "size"]);
        }
        ok(check_invariants(&d));
    }
}

#[test]
fn a_nested_def_under_a_missing_mount_freezes_a_record_set_on_a_local_schema() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.cnnd");
    let host = dir.path().join("host.cnnd");
    write_lib(&lib, false);
    {
        let mut d = new_design(&host);
        d.link_library("lib.cnnd", "l").unwrap();
        d.add_record_type_def(box_def("Box", "l.Point")).unwrap();
        edit(&mut d, "Main", &host_code("Box", false));
        assert_eq!(in_pins(&d, "s"), vec!["record", "p_x", "size"]);
        save(&mut d);
    }

    let parked = dir.path().join("lib.cnnd.parked");
    std::fs::rename(&lib, &parked).unwrap();
    let mut d = open(&host);
    assert_frozen_and_wired(&mut d);
    save(&mut d);

    std::fs::rename(&parked, &lib).unwrap();
    let mut d = open(&host);
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert!(!s_frozen(&d));
    assert_eq!(in_pins(&d, "s"), vec!["record", "p_x", "size"]);
    assert!(reads(&d, "s", 1, "f9"));
    assert!(reads(&d, "s", 2, "f7"));
    assert!(errors_on(&d, "s").is_empty(), "{:?}", errors_on(&d, "s"));
}

// ============================================================================
// 10. with_record_path_replaced
// ============================================================================

#[test]
fn with_record_path_replaced_replaces_inserts_and_reports_depth() {
    let point = NetworkResult::record(vec![
        ("x".to_string(), NetworkResult::Float(1.5)),
        ("y".to_string(), NetworkResult::Float(2.5)),
    ]);
    let bx = NetworkResult::record(vec![
        ("p".to_string(), point),
        ("size".to_string(), NetworkResult::Float(4.0)),
        ("tag".to_string(), NetworkResult::Int(5)),
    ]);

    let v = bx
        .with_record_path_replaced(&["size"], NetworkResult::Float(7.0))
        .unwrap();
    assert_eq!(field(&v, &["size"]), 7.0);
    assert_eq!(field(&v, &["p", "x"]), 1.5);
    assert!(matches!(
        v.extract_record_field("tag"),
        Some(NetworkResult::Int(5))
    ));

    let v = bx
        .with_record_path_replaced(&["p", "x"], NetworkResult::Float(9.0))
        .unwrap();
    assert_eq!(field(&v, &["p", "x"]), 9.0);
    assert_eq!(field(&v, &["p", "y"]), 2.5);
    assert_eq!(field(&v, &["size"]), 4.0);
    assert_eq!(field(&bx, &["p", "x"]), 1.5, "the input is not touched");

    // A missing leaf is inserted, in canonical order.
    let v = bx
        .with_record_path_replaced(&["p", "a"], NetworkResult::Float(0.5))
        .unwrap();
    match v.extract_record_field("p") {
        Some(NetworkResult::Record(fields)) => {
            let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, vec!["a", "x", "y"]);
        }
        other => panic!("{:?}", other.map(|o| o.to_display_string())),
    }

    // A missing middle record, a non-record middle, a non-record input.
    assert_eq!(
        bx.with_record_path_replaced(&["q", "x"], NetworkResult::Float(0.0))
            .err(),
        Some(1)
    );
    assert_eq!(
        bx.with_record_path_replaced(&["size", "x"], NetworkResult::Float(0.0))
            .err(),
        Some(1)
    );
    assert_eq!(
        NetworkResult::Float(1.0)
            .with_record_path_replaced(&["x"], NetworkResult::Float(0.0))
            .err(),
        Some(0)
    );
}

// ============================================================================
// 11. Reserved names
// ============================================================================

#[test]
fn rows_named_like_the_block_s_own_keys_get_a_suffix() {
    let mut d = setup();
    d.add_record_type_def(floats("Rsv", &["record", "schema", "visible", "z"]))
        .unwrap();
    edit(
        &mut d,
        "Main",
        // Not `record_construct`: its own `schema` property shadows a field
        // pin named `schema`.
        "r = expr { expression: \"{record: 9.0, schema: 9.0, visible: 9.0, z: 9.0}\", parameters: [] }
f1 = float { value: 1.0 }
s = record_set { schema: \"Rsv\", paths: [\"record\", \"schema\", \"visible\"], record: r, record__2: f1, visible__2: f1 }
",
    );
    assert_eq!(
        in_pins(&d, "s"),
        vec!["record", "record__2", "schema__2", "visible__2"]
    );
    assert!(reads(&d, "s", 0, "r"));
    assert!(reads(&d, "s", 1, "f1"));
    assert!(reads(&d, "s", 3, "f1"));
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["record"]), 1.0);
    assert_eq!(field(&v, &["schema"]), 9.0);
    assert_eq!(field(&v, &["visible"]), 1.0);
}

// ============================================================================
// 12. Function-pin roles follow their row
// ============================================================================

#[test]
fn a_role_on_a_row_pin_follows_the_row_when_an_earlier_row_dangles() {
    let mut d = mapped(
        "s = record_set { schema: \"Box\", paths: [\"p.x\", \"size\"], p_x: f9, size: f7, pin_roles: { size: delayed } }\n",
        false,
    );
    let s_id = node_id(&d, "Main", "s");
    let roles = |d: &StructureDesigner| network(d, "Main").nodes[&s_id].function_pin_roles.clone();
    assert_eq!(
        roles(&d).into_iter().collect::<Vec<_>>(),
        vec![(2, FunctionPinRole::Delayed)]
    );
    let partial = DataType::Function(FunctionType::new(vec![DataType::Float], named("Box")));
    assert_eq!(
        output_type(&d, "m"),
        Some(DataType::Iterator(Box::new(partial.clone())))
    );

    set_fields(&mut d, "Point", &["y"]);
    d.validate_active_network();
    assert_eq!(in_pins(&d, "s"), vec!["record", "size"]);
    assert_eq!(
        roles(&d).into_iter().collect::<Vec<_>>(),
        vec![(1, FunctionPinRole::Delayed)],
        "the role moved with size's pin"
    );
    assert!(reads(&d, "s", 1, "f7"));
    assert_eq!(
        output_type(&d, "m"),
        Some(DataType::Iterator(Box::new(partial))),
        "map.f's function still takes size"
    );
}

// ============================================================================
// 13. Drag-aware add
// ============================================================================

#[test]
fn drag_aware_add_adopts_the_schema_from_either_direction() {
    let mut d = setup();
    let schema = |d: &StructureDesigner, id: u64| {
        network(d, "Main").nodes[&id]
            .data
            .as_any_ref()
            .downcast_ref::<RecordSetData>()
            .unwrap()
            .schema
            .clone()
    };
    for (k, direction) in [DragDirection::FromOutput, DragDirection::FromInput]
        .into_iter()
        .enumerate()
    {
        let id = d.add_node_with_drag_source(
            "record_set",
            DVec2::new(0.0, 200.0 * k as f64),
            Some(DragSource {
                source_type: named("Box"),
                direction,
            }),
        );
        assert_eq!(schema(&d, id), "Box", "{:?}", direction);
    }
    let id = d.add_node_with_drag_source(
        "record_set",
        DVec2::new(0.0, 400.0),
        Some(DragSource {
            source_type: DataType::Record(RecordType::anonymous(vec![(
                "a".to_string(),
                DataType::Float,
            )])),
            direction: DragDirection::FromOutput,
        }),
    );
    assert_eq!(schema(&d, id), "");
}
