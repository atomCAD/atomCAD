//! `record_get` (`doc/design_record_get_set.md` Phase 1): the path core
//! (`record_path.rs`, `nodes/record_path_rows.rs`) and the node.
//!
//! Most tests build `Box { p: Point, size: Float }` / `Point { x, y: Float }`
//! and a `record_get` on `Box` with rows `[p.x, size]`, each row wired to its
//! own downstream `expr` (`ex` reads `p.x`, `es` reads `size`), then edit the
//! defs and check pins, wires, values and warnings.

use super::library_links_refresh_test::edit_lib;
use super::library_links_support::*;
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::library_links::is_frozen;
use atomcad_structure_designer::node_data::{DragDirection, MissingDefs};
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type::PinOutputType;
use atomcad_structure_designer::node_type_registry::{
    FieldId, RecordFieldEdit, RecordTypeDef, RecordTypeDefError,
};
use atomcad_structure_designer::nodes::record_get::RecordGetData;
use atomcad_structure_designer::nodes::record_path_rows::derived_pin_names;
use atomcad_structure_designer::record_path::{
    DanglingReason, PathSegment, PathSpec, ResolvedPath, format_path, format_path_spec, parse_path,
    parse_path_spec, resolve_names, resolve_path,
};
use atomcad_structure_designer::structure_designer::{DragSource, StructureDesigner};
use atomcad_structure_designer::text_format::serialize_network;
use glam::f64::DVec2;
use std::path::Path;

// ============================================================================
// Helpers
// ============================================================================

fn named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

fn point_def(name: &str) -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        name,
        vec![
            ("x".to_string(), DataType::Float),
            ("y".to_string(), DataType::Float),
        ],
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
    d.add_record_type_def(point_def("Point")).unwrap();
    d.add_record_type_def(box_def("Box", "Point")).unwrap();
    d
}

/// `Main`: a `Box` value, `g` reading `paths`, and `ex` / `es` reading
/// `g.<ex_pin>` / `g.<es_pin>`.
fn main_code(paths: &str, ex_pin: &str, es_pin: &str) -> String {
    format!(
        "fx = float {{ value: 1.5 }}
fy = float {{ value: 2.5 }}
fs = float {{ value: 4.0 }}
pt = record_construct {{ schema: \"Point\", x: fx, y: fy }}
b = record_construct {{ schema: \"Box\", p: pt, size: fs }}
g = record_get {{ schema: \"Box\", paths: {paths}, record: b }}
ex = expr {{ a: {ex_pin}, expression: \"a + 100.0\", parameters: [{{ name: \"a\", data_type: Float }}] }}
es = expr {{ a: {es_pin}, expression: \"a + 200.0\", parameters: [{{ name: \"a\", data_type: Float }}] }}
"
    )
}

/// `standard` without the `Box` value: only `g`, `ex` and `es`, so no other
/// node names a def a test deletes below the designer.
fn standard_unfed() -> StructureDesigner {
    let mut d = setup();
    edit(
        &mut d,
        "Main",
        "g = record_get { schema: \"Box\", paths: [\"p.x\", \"size\"] }
ex = expr { a: g, expression: \"a + 100.0\", parameters: [{ name: \"a\", data_type: Float }] }
es = expr { a: g.size, expression: \"a + 200.0\", parameters: [{ name: \"a\", data_type: Float }] }
",
    );
    d
}

/// The standard network: rows `[p.x, size]`, `ex` ← pin 0, `es` ← pin 1.
fn standard() -> StructureDesigner {
    let mut d = setup();
    edit(
        &mut d,
        "Main",
        &main_code("[\"p.x\", \"size\"]", "g", "g.size"),
    );
    d
}

fn network<'a>(d: &'a StructureDesigner, name: &str) -> &'a NodeNetwork {
    &d.node_type_registry.node_networks[name]
}

fn out_pins_in(net: &NodeNetwork, id: u64) -> Vec<String> {
    net.nodes[&id]
        .custom_node_type
        .as_ref()
        .expect("record_get has a cached layout")
        .output_pins
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

fn out_pins(d: &StructureDesigner, name: &str) -> Vec<String> {
    out_pins_in(network(d, "Main"), node_id(d, "Main", name))
}

/// The `(source node, output pin)` pairs wired into `arg` of `id` in `net`.
fn wires_in(net: &NodeNetwork, id: u64, arg: usize) -> Vec<(u64, i32)> {
    net.nodes[&id]
        .arguments
        .get(arg)
        .map(|a| {
            a.incoming_wires
                .iter()
                .filter_map(|w| match w.source_pin {
                    SourcePin::NodeOutput { pin_index } => Some((w.source_node_id, pin_index)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The output pin of `g` that `dest`'s first argument reads, if wired.
fn reads_pin(d: &StructureDesigner, dest: &str) -> Option<i32> {
    let g = node_id(d, "Main", "g");
    wires_in(network(d, "Main"), node_id(d, "Main", dest), 0)
        .into_iter()
        .find(|(s, _)| *s == g)
        .map(|(_, p)| p)
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

fn float(v: NetworkResult) -> f64 {
    match v {
        NetworkResult::Float(f) => f,
        other => panic!("expected a Float, got {}", other.to_display_string()),
    }
}

fn field_id(d: &StructureDesigner, def: &str, field: &str) -> FieldId {
    d.node_type_registry
        .lookup_record_type_def(def)
        .unwrap()
        .fields
        .iter()
        .find(|f| f.name == field)
        .unwrap_or_else(|| panic!("no field {} in {}", field, def))
        .id
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

fn set_point_fields(d: &mut StructureDesigner, fields: &[&str]) {
    d.update_record_type_def(
        "Point",
        fields
            .iter()
            .map(|f| (f.to_string(), DataType::Float))
            .collect(),
    )
    .unwrap();
    d.validate_active_network();
}

fn main_text(d: &StructureDesigner) -> String {
    serialize_network(network(d, "Main"), &d.node_type_registry, Some("Main"))
}

fn g_data(d: &StructureDesigner) -> RecordGetData {
    network(d, "Main").nodes[&node_id(d, "Main", "g")]
        .data
        .as_any_ref()
        .downcast_ref::<RecordGetData>()
        .unwrap()
        .clone()
}

// ============================================================================
// 1. Values and pin names
// ============================================================================

#[test]
fn top_level_and_nested_paths_read_their_fields() {
    let mut d = standard();
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(float(value(&mut d, "g", 0)), 1.5);
    assert_eq!(float(value(&mut d, "g", 1)), 4.0);
    assert_eq!(float(value(&mut d, "ex", 0)), 101.5);
    assert_eq!(float(value(&mut d, "es", 0)), 204.0);
    assert!(errors_on(&d, "g").is_empty(), "{:?}", errors_on(&d, "g"));
    // Pin types are the leaves' types; the input is the schema.
    let g = &network(&d, "Main").nodes[&node_id(&d, "Main", "g")];
    let nt = g.custom_node_type.as_ref().unwrap();
    assert_eq!(nt.parameters.len(), 1);
    assert_eq!(nt.parameters[0].name, "record");
    assert_eq!(nt.parameters[0].data_type, named("Box"));
    for pin in &nt.output_pins {
        assert_eq!(pin.data_type, PinOutputType::Fixed(DataType::Float));
        assert!(pin.id.is_some(), "row pins carry the row id");
    }
    // The text format prints the paths back.
    assert!(
        main_text(&d).contains("paths: [\"p.x\", \"size\"]"),
        "{}",
        main_text(&d)
    );
}

#[test]
fn a_fresh_record_get_has_the_placeholder_pin() {
    let mut d = setup();
    edit(&mut d, "Main", "g = record_get { schema: \"Box\" }\n");
    assert_eq!(out_pins(&d, "g"), vec!["result"]);
    assert!(matches!(value(&mut d, "g", 0), NetworkResult::None));
    edit(&mut d, "Main", "g = record_get {}\n");
    assert_eq!(out_pins(&d, "g"), vec!["result"]);
}

#[test]
fn bad_paths_are_rejected_by_the_text_format() {
    for paths in [
        "[\"p.z\"]",
        "[\"q\"]",
        "[\"size.x\"]",
        "[\"p.x\", \"p.x\"]",
        "[\"p.`x\"]",
        "[\"\"]",
    ] {
        let mut d = setup();
        d.set_active_node_network_name(Some("Main".to_string()));
        let outcome = d.ai_text_edit(
            &format!("g = record_get {{ schema: \"Box\", paths: {} }}\n", paths),
            true,
        );
        assert!(!outcome.result.errors.is_empty(), "{} was accepted", paths);
    }
}

// ============================================================================
// 2. None / Error input
// ============================================================================

#[test]
fn none_and_error_inputs_reach_every_pin() {
    let mut d = setup();
    edit(
        &mut d,
        "Main",
        "g = record_get { schema: \"Box\", paths: [\"p.x\", \"size\"] }
arr = array { element_type: Record(Box), elements: [] }
at = array_at { array: arr, element_type: Record(Box), index: 0 }
h = record_get { schema: \"Box\", paths: [\"p.x\", \"size\"], record: at }
",
    );
    for pin in [0, 1] {
        assert!(matches!(value(&mut d, "g", pin), NetworkResult::None));
    }
    let err = value(&mut d, "at", 0);
    assert!(
        matches!(err, NetworkResult::Error(_)),
        "{}",
        err.to_display_string()
    );
    for pin in [0, 1] {
        let v = value(&mut d, "h", pin);
        assert!(
            matches!(v, NetworkResult::Error(_)),
            "pin {}: {}",
            pin,
            v.to_display_string()
        );
    }
}

// ============================================================================
// 3. Optional leaf
// ============================================================================

#[test]
fn an_optional_leaf_is_a_plain_pin_and_passes_none() {
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
    edit(
        &mut d,
        "Main",
        "fw = float { value: 1.0 }
o = record_construct { schema: \"Opt\", w: fw }
g = record_get { schema: \"Opt\", paths: [\"v\", \"w\"], record: o }
",
    );
    let net = network(&d, "Main");
    let nt = net.nodes[&node_id(&d, "Main", "g")]
        .custom_node_type
        .clone()
        .unwrap();
    assert_eq!(
        nt.output_pins[0].data_type,
        PinOutputType::Fixed(DataType::Float)
    );
    assert!(matches!(value(&mut d, "g", 0), NetworkResult::None));
    assert_eq!(float(value(&mut d, "g", 1)), 1.0);
}

// ============================================================================
// 4. Anonymous record type inside a named def
// ============================================================================

#[test]
fn a_path_through_an_anonymous_record_type_resolves_by_name() {
    let mut d = setup();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Holder",
        vec![(
            "inner".to_string(),
            DataType::Record(RecordType::anonymous(vec![
                ("a".to_string(), DataType::Float),
                ("b".to_string(), DataType::Int),
            ])),
        )],
    ))
    .unwrap();
    edit(
        &mut d,
        "Main",
        "lit = expr { expression: \"{a: 1.5, b: 2}\", parameters: [] }
hd = record_construct { schema: \"Holder\", inner: lit }
g = record_get { schema: \"Holder\", paths: [\"inner.a\", \"inner.b\"], record: hd }
",
    );
    assert_eq!(out_pins(&d, "g"), vec!["inner_a", "inner_b"]);
    let data = g_data(&d);
    assert!(data.rows[0].path[0].field_id.is_some(), "named def: by id");
    assert_eq!(
        data.rows[0].path[1],
        PathSegment {
            field_id: None,
            name: "a".to_string()
        },
        "anonymous record type: by name"
    );
    assert_eq!(float(value(&mut d, "g", 0)), 1.5);
    assert!(matches!(value(&mut d, "g", 1), NetworkResult::Int(2)));
}

// ============================================================================
// 5. Pin-name dedup
// ============================================================================

#[test]
fn colliding_pin_names_are_deduplicated_and_outputs_reserve_nothing() {
    let mut d = setup();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Inner",
        vec![("b".to_string(), DataType::Float)],
    ))
    .unwrap();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Dd",
        vec![
            ("a".to_string(), named("Inner")),
            ("a_b".to_string(), DataType::Float),
            ("schema".to_string(), DataType::Float),
        ],
    ))
    .unwrap();
    edit(
        &mut d,
        "Main",
        "g = record_get { schema: \"Dd\", paths: [\"a.b\", \"a_b\", \"schema\"] }\n",
    );
    assert_eq!(out_pins(&d, "g"), vec!["a_b", "a_b__2", "schema"]);

    // The helper directly: reserved names count as taken, dangling rows take
    // no part, and a natural `__2` name is not stolen.
    let r = |names: &[&str]| {
        Some(ResolvedPath {
            names: names.iter().map(|s| s.to_string()).collect(),
            leaf_type: DataType::Float,
        })
    };
    assert_eq!(
        derived_pin_names(
            &[
                r(&["record"]),
                None,
                r(&["a_b__2"]),
                r(&["a", "b"]),
                r(&["a_b"])
            ],
            &["record"]
        ),
        vec![
            Some("record__2".to_string()),
            None,
            Some("a_b__2".to_string()),
            Some("a_b".to_string()),
            Some("a_b__3".to_string()),
        ]
    );
}

// ============================================================================
// 6. Rename in the nested def
// ============================================================================

#[test]
fn a_nested_field_rename_renames_the_pin_and_keeps_the_wire() {
    let mut d = standard();
    let edits = vec![
        existing(&d, "Point", "x", "px"),
        existing(&d, "Point", "y", "y"),
    ];
    d.update_record_type_def_with_ids("Point", edits).unwrap();
    d.validate_active_network();
    assert_eq!(out_pins(&d, "g"), vec!["p_px", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
    assert_eq!(float(value(&mut d, "ex", 0)), 101.5);
    assert!(main_text(&d).contains("paths: [\"p.px\", \"size\"]"));
    assert!(errors_on(&d, "g").is_empty());
}

// ============================================================================
// 7. Reorder, delete, schema delete, retype
// ============================================================================

#[test]
fn a_nested_reorder_changes_nothing() {
    let mut d = standard();
    let edits = vec![
        existing(&d, "Point", "y", "y"),
        existing(&d, "Point", "x", "x"),
    ];
    d.update_record_type_def_with_ids("Point", edits).unwrap();
    d.validate_active_network();
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
    assert_eq!(float(value(&mut d, "ex", 0)), 101.5);
}

#[test]
fn a_nested_field_delete_dangles_the_row_and_undo_restores_it() {
    let mut d = standard();
    set_point_fields(&mut d, &["y"]);

    assert_eq!(out_pins(&d, "g"), vec!["size"]);
    assert_eq!(reads_pin(&d, "ex"), None, "the dangling row's wire drops");
    assert_eq!(reads_pin(&d, "es"), Some(0));
    assert_eq!(float(value(&mut d, "es", 0)), 204.0);
    let errors = errors_on(&d, "g");
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert!(!errors[0].1, "a dangling row is a warning");
    assert!(errors[0].0.contains("`p.x`"), "{}", errors[0].0);
    assert!(errors[0].0.contains("`Box`"), "{}", errors[0].0);
    // The row is kept, printed with the dangling marker.
    assert_eq!(g_data(&d).rows.len(), 2);
    assert!(
        main_text(&d).contains("paths: [\"!p.x\", \"size\"]"),
        "{}",
        main_text(&d)
    );

    assert!(d.undo());
    d.validate_active_network();
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
    assert!(errors_on(&d, "g").is_empty());
    assert_eq!(float(value(&mut d, "ex", 0)), 101.5);
}

#[test]
fn deleting_the_schema_def_dangles_every_row() {
    let mut d = standard_unfed();
    // The reference check refuses this through the designer (test 9), so go
    // below it, as a library refresh or a hand-edited file can.
    d.node_type_registry.delete_record_type_def("Box").unwrap();
    d.node_type_registry.repair_all_networks();
    d.validate_active_network();
    assert_eq!(out_pins(&d, "g"), vec!["result"]);
    let errors = errors_on(&d, "g");
    assert_eq!(errors.len(), 2, "{:?}", errors);
    assert!(errors.iter().all(|(_, blocking)| !blocking));
    assert!(matches!(value(&mut d, "g", 0), NetworkResult::None));
}

#[test]
fn retyping_an_inner_field_away_from_a_record_dangles_only_its_paths() {
    let mut d = standard();
    d.update_record_type_def(
        "Box",
        vec![
            ("p".to_string(), DataType::Float),
            ("size".to_string(), DataType::Float),
        ],
    )
    .unwrap();
    d.validate_active_network();
    assert_eq!(out_pins(&d, "g"), vec!["size"]);
    assert_eq!(reads_pin(&d, "es"), Some(0));
    let errors = errors_on(&d, "g");
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert!(errors[0].0.contains("not a record"), "{}", errors[0].0);
}

// ============================================================================
// 8. The R3 output-pin id remap reaches record_get
// ============================================================================

#[test]
fn output_wires_follow_their_row_when_an_earlier_row_dangles() {
    // Rows [p.x, size]: size moves from pin 1 to pin 0, its wire follows.
    let mut d = standard();
    set_point_fields(&mut d, &["y"]);
    assert_eq!(reads_pin(&d, "es"), Some(0));
    assert_eq!(reads_pin(&d, "ex"), None);

    // Control, rows [size, p.x]: size stays on pin 0.
    let mut d = setup();
    edit(
        &mut d,
        "Main",
        &main_code("[\"size\", \"p.x\"]", "g.p_x", "g"),
    );
    assert_eq!(reads_pin(&d, "es"), Some(0));
    assert_eq!(reads_pin(&d, "ex"), Some(1));
    set_point_fields(&mut d, &["y"]);
    assert_eq!(reads_pin(&d, "es"), Some(0));
    assert_eq!(reads_pin(&d, "ex"), None);
}

// ============================================================================
// 9. Def rename, namespace move, reference check
// ============================================================================

#[test]
fn a_def_rename_and_a_namespace_move_rewrite_the_schema() {
    let mut d = standard();
    d.rename_record_type_def("Box", "geo.Crate").unwrap();
    d.validate_active_network();
    assert_eq!(g_data(&d).schema, "geo.Crate");
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "es"), Some(1));

    assert!(d.rename_namespace("geo", "shapes"));
    d.validate_active_network();
    assert_eq!(g_data(&d).schema, "shapes.Crate");
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(float(value(&mut d, "ex", 0)), 101.5);
    assert!(errors_on(&d, "g").is_empty());
}

#[test]
fn deleting_a_def_used_only_by_a_record_get_is_refused() {
    let mut d = setup();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Lone",
        vec![("v".to_string(), DataType::Float)],
    ))
    .unwrap();
    edit(
        &mut d,
        "Main",
        "g = record_get { schema: \"Lone\", paths: [\"v\"] }\n",
    );
    assert!(matches!(
        d.delete_record_type_def("Lone"),
        Err(RecordTypeDefError::Referenced(..))
    ));
}

// ============================================================================
// 10. Zone body
// ============================================================================

#[test]
fn a_record_get_in_a_zone_body_follows_def_edits() {
    let mut d = setup();
    edit(
        &mut d,
        "Main",
        "fx = float { value: 1.5 }
fy = float { value: 2.5 }
fs = float { value: 4.0 }
pt = record_construct { schema: \"Point\", x: fx, y: fy }
b = record_construct { schema: \"Box\", p: pt, size: fs }
r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Float, body { g = record_get { schema: \"Box\", paths: [\"p.x\", \"size\"], record: ^b } e = expr { a: g.size, expression: \"a + 1.0\", parameters: [{ name: \"a\", data_type: Float }] } output e } }
",
    );
    let body = |d: &StructureDesigner| -> NodeNetwork {
        let mp = node_id(d, "Main", "mp");
        (**network(d, "Main").nodes[&mp].zone.as_ref().unwrap()).clone()
    };
    let named_in = |net: &NodeNetwork, name: &str| {
        net.nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some(name))
            .unwrap()
            .id
    };

    let edits = vec![
        existing(&d, "Point", "x", "px"),
        existing(&d, "Point", "y", "y"),
    ];
    d.update_record_type_def_with_ids("Point", edits).unwrap();
    d.validate_active_network();
    let b = body(&d);
    let (g, e) = (named_in(&b, "g"), named_in(&b, "e"));
    assert_eq!(out_pins_in(&b, g), vec!["p_px", "size"]);
    assert_eq!(wires_in(&b, e, 0), vec![(g, 1)]);

    set_point_fields(&mut d, &["y"]);
    let b = body(&d);
    assert_eq!(out_pins_in(&b, g), vec!["size"]);
    assert_eq!(wires_in(&b, e, 0), vec![(g, 0)], "remapped in the body");
    assert!(
        b.validation_errors
            .iter()
            .any(|err| err.node_id == Some(g) && !err.blocking && err.error_text.contains("p.x")),
        "{:?}",
        b.validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// 11–13. Library linking and frozen nodes
// ============================================================================

/// `lib.cnnd` with `Point { x, y: Float }` and `Box { p: Point, size: Float }`.
fn write_point_box_lib(path: &Path, with_box: bool) {
    let mut d = new_design(path);
    d.add_record_type_def(point_def("Point")).unwrap();
    if with_box {
        d.add_record_type_def(box_def("Box", "Point")).unwrap();
    }
    d.delete_node_network("Main").ok();
    save(&mut d);
}

/// The host network on the linked `l.Box`; with `point_user`, a second node
/// whose schema is `l.Point`, so the `uses` table records it.
fn linked_main(point_user: bool) -> String {
    let mut code = "fx = float { value: 1.5 }
fy = float { value: 2.5 }
fs = float { value: 4.0 }
pt = record_construct { schema: \"l.Point\", x: fx, y: fy }
b = record_construct { schema: \"l.Box\", p: pt, size: fs }
g = record_get { schema: \"l.Box\", paths: [\"p.x\", \"size\"], record: b }
ex = expr { a: g, expression: \"a + 100.0\", parameters: [{ name: \"a\", data_type: Float }] }
es = expr { a: g.size, expression: \"a + 200.0\", parameters: [{ name: \"a\", data_type: Float }] }
"
    .to_string();
    if point_user {
        code.push_str("pu = record_get { schema: \"l.Point\", paths: [\"x\"], record: pt }\n");
    }
    code
}

struct LinkWs {
    _dir: tempfile::TempDir,
    lib: std::path::PathBuf,
    host: std::path::PathBuf,
}

fn linked_workspace(point_user: bool) -> LinkWs {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.cnnd");
    let host = dir.path().join("host.cnnd");
    write_point_box_lib(&lib, true);
    let mut d = new_design(&host);
    d.link_library("lib.cnnd", "l").unwrap();
    edit(&mut d, "Main", &linked_main(point_user));
    save(&mut d);
    LinkWs {
        _dir: dir,
        lib,
        host,
    }
}

fn g_frozen(d: &StructureDesigner) -> bool {
    let g = node_id(d, "Main", "g");
    is_frozen(&network(d, "Main").nodes[&g], &d.node_type_registry)
}

#[test]
fn a_renamed_library_field_keeps_the_path() {
    let ws = linked_workspace(false);
    let mut d = open(&ws.host);
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    edit_lib(&ws.lib, |l| {
        let edits = vec![
            existing(l, "Point", "x", "px"),
            existing(l, "Point", "y", "y"),
        ];
        l.update_record_type_def_with_ids("Point", edits).unwrap();
    });
    d.refresh_library("l").expect("refresh");
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert_eq!(out_pins(&d, "g"), vec!["p_px", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
    assert!(errors_on(&d, "g").is_empty());
    assert_eq!(float(value(&mut d, "ex", 0)), 101.5);

    // And on open.
    let mut d = open(&ws.host);
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert_eq!(out_pins(&d, "g"), vec!["p_px", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
}

#[test]
fn a_library_field_deleted_and_re_added_dangles_the_row() {
    let ws = linked_workspace(false);
    let mut d = open(&ws.host);
    edit_lib(&ws.lib, |l| {
        set_fields(l, "Point", &["y"]);
        set_fields(l, "Point", &["x", "y"]);
    });
    d.refresh_library("l").expect("refresh");
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert!(!g_frozen(&d));
    assert_eq!(out_pins(&d, "g"), vec!["size"]);
    assert_eq!(reads_pin(&d, "es"), Some(0));
    assert_eq!(reads_pin(&d, "ex"), None);
    let errors = errors_on(&d, "g");
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert!(!errors[0].1);
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

#[test]
fn the_schema_under_a_missing_mount_freezes_the_node() {
    let ws = linked_workspace(false);
    std::fs::remove_file(&ws.lib).unwrap();
    let mut d = open(&ws.host);
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert!(g_frozen(&d));
    // One frozen-node error, no dangling-row warnings.
    let errors = errors_on(&d, "g");
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert!(errors[0].1);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
}

#[test]
fn a_frozen_record_get_keeps_its_nested_pin_from_the_recorded_interfaces() {
    // `Point` recorded in `uses` (a node on `l.Point`): a fresh open with the
    // library missing lays both recorded interfaces over the live defs.
    let ws = linked_workspace(true);
    std::fs::remove_file(&ws.lib).unwrap();
    let mut d = open(&ws.host);
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert!(g_frozen(&d));
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
    ok(check_invariants(&d));
}

#[test]
fn a_frozen_record_get_keeps_its_cached_layout_when_a_nested_def_is_not_recorded() {
    // `Point` not recorded: when the defs go away during a session the node
    // keeps the layout it had.
    let ws = linked_workspace(false);
    let mut d = open(&ws.host);
    edit_lib(&ws.lib, |l| {
        l.delete_record_type_def("Box").unwrap();
        l.delete_record_type_def("Point").unwrap();
    });
    d.refresh_library("l").expect("refresh");
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert!(g_frozen(&d));
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
    ok(check_invariants(&d));
}

#[test]
fn a_nested_def_under_a_missing_mount_freezes_a_record_get_on_a_local_schema() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.cnnd");
    let host = dir.path().join("host.cnnd");
    write_point_box_lib(&lib, false);
    {
        let mut d = new_design(&host);
        d.link_library("lib.cnnd", "l").unwrap();
        d.add_record_type_def(box_def("Box", "l.Point")).unwrap();
        edit(
            &mut d,
            "Main",
            "g = record_get { schema: \"Box\", paths: [\"p.x\", \"size\"] }
ex = expr { a: g, expression: \"a + 100.0\", parameters: [{ name: \"a\", data_type: Float }] }
es = expr { a: g.size, expression: \"a + 200.0\", parameters: [{ name: \"a\", data_type: Float }] }
",
        );
        assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
        save(&mut d);
    }

    // The library goes missing: frozen, no dangling row, both wires kept.
    let parked = dir.path().join("lib.cnnd.parked");
    std::fs::rename(&lib, &parked).unwrap();
    let mut d = open(&host);
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert!(g_frozen(&d));
    let errors = errors_on(&d, "g");
    assert_eq!(errors.len(), 1, "only the frozen-node error: {:?}", errors);
    assert!(errors[0].1);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
    save(&mut d);

    // Back with the library: both pins, both wires.
    std::fs::rename(&parked, &lib).unwrap();
    let mut d = open(&host);
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert!(!g_frozen(&d));
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
    assert!(errors_on(&d, "g").is_empty(), "{:?}", errors_on(&d, "g"));
}

#[test]
fn a_really_deleted_local_nested_def_dangles_the_row() {
    let mut d = standard_unfed();
    d.node_type_registry
        .delete_record_type_def("Point")
        .unwrap();
    d.node_type_registry.repair_all_networks();
    d.validate_active_network();
    assert!(!g_frozen(&d));
    assert_eq!(out_pins(&d, "g"), vec!["size"]);
    assert_eq!(reads_pin(&d, "ex"), None);
    assert_eq!(reads_pin(&d, "es"), Some(0));
    let errors = errors_on(&d, "g");
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert!(!errors[0].1);
    assert!(errors[0].0.contains("`Point`"), "{}", errors[0].0);
    assert!(errors[0].0.contains("`p.x`"), "{}", errors[0].0);
}

// ============================================================================
// 14. extract_record_path
// ============================================================================

#[test]
fn extract_record_path_hits_and_misses() {
    let point = NetworkResult::record(vec![
        ("x".to_string(), NetworkResult::Float(1.5)),
        ("y".to_string(), NetworkResult::Float(2.5)),
    ]);
    let bx = NetworkResult::record(vec![
        ("p".to_string(), point),
        ("size".to_string(), NetworkResult::Float(4.0)),
    ]);
    assert!(matches!(
        bx.extract_record_path(&["p", "x"]),
        Some(NetworkResult::Float(v)) if *v == 1.5
    ));
    assert!(matches!(
        bx.extract_record_path(&["size"]),
        Some(NetworkResult::Float(v)) if *v == 4.0
    ));
    assert!(
        bx.extract_record_path(&["q", "x"]).is_none(),
        "miss part-way"
    );
    assert!(
        bx.extract_record_path(&["p", "z"]).is_none(),
        "missing leaf"
    );
    assert!(
        bx.extract_record_path(&["size", "x"]).is_none(),
        "non-record part-way"
    );
}

// ============================================================================
// 15. Drag-aware add
// ============================================================================

#[test]
fn drag_aware_add_adopts_a_named_record_schema() {
    let mut d = setup();
    let id = d.add_node_with_drag_source(
        "record_get",
        DVec2::new(0.0, 0.0),
        Some(DragSource {
            source_type: named("Box"),
            direction: DragDirection::FromOutput,
        }),
    );
    let data = |d: &StructureDesigner, id: u64| {
        network(d, "Main").nodes[&id]
            .data
            .as_any_ref()
            .downcast_ref::<RecordGetData>()
            .unwrap()
            .schema
            .clone()
    };
    assert_eq!(data(&d, id), "Box");

    let id = d.add_node_with_drag_source(
        "record_get",
        DVec2::new(0.0, 200.0),
        Some(DragSource {
            source_type: DataType::Record(RecordType::anonymous(vec![(
                "a".to_string(),
                DataType::Float,
            )])),
            direction: DragDirection::FromOutput,
        }),
    );
    assert_ne!(id, 0);
    assert_eq!(data(&d, id), "");

    // Its output is not a record: dragging from an input does not adapt it.
    let id = d.add_node_with_drag_source(
        "record_get",
        DVec2::new(0.0, 400.0),
        Some(DragSource {
            source_type: named("Box"),
            direction: DragDirection::FromInput,
        }),
    );
    assert_eq!(data(&d, id), "");
}

// ============================================================================
// 16. The path core directly
// ============================================================================

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn path_text_round_trips() {
    for (text, spec) in [
        ("p.x", PathSpec::Live(names(&["p", "x"]))),
        ("size", PathSpec::Live(names(&["size"]))),
        ("`a.b`.c", PathSpec::Live(names(&["a.b", "c"]))),
        ("`!a`.b", PathSpec::Live(names(&["!a", "b"]))),
        ("!p.x", PathSpec::Dangling(names(&["p", "x"]))),
        ("!`!a`.`b.c`", PathSpec::Dangling(names(&["!a", "b.c"]))),
        ("z-shift.x", PathSpec::Live(names(&["z-shift", "x"]))),
    ] {
        assert_eq!(parse_path_spec(text), Ok(spec.clone()), "{}", text);
        assert_eq!(format_path_spec(&spec), text);
    }
    // A segment starting with `!` is quoted only in first position.
    assert_eq!(format_path(&names(&["a", "!b"])), "a.!b");
    assert_eq!(parse_path("a.!b"), Ok(names(&["a", "!b"])));
    for bad in ["", "a..b", ".a", "a.", "`a", "a.`b", "`a`b", "a`b`", "!"] {
        assert!(parse_path_spec(bad).is_err(), "{:?} parsed", bad);
    }
}

#[test]
fn resolve_names_and_resolve_path_report_where_and_why() {
    let d = setup();
    let defs = d.node_type_registry.record_defs();
    assert_eq!(resolve_names("Box", &names(&["q"]), defs), Err(0));
    assert_eq!(resolve_names("Box", &names(&["p", "z"]), defs), Err(1));
    assert_eq!(resolve_names("Box", &names(&["size", "x"]), defs), Err(1));
    assert_eq!(resolve_names("Nope", &names(&["p"]), defs), Err(0));
    let segs = resolve_names("Box", &names(&["p", "x"]), defs).unwrap();
    assert_eq!(segs[0].field_id, Some(field_id(&d, "Box", "p").0));
    assert_eq!(segs[1].field_id, Some(field_id(&d, "Point", "x").0));

    let ok = resolve_path("Box", &segs, defs).unwrap();
    assert_eq!(ok.names, names(&["p", "x"]));
    assert_eq!(ok.leaf_type, DataType::Float);

    assert_eq!(
        resolve_path("Nope", &segs, defs),
        Err(DanglingReason::DefMissing("Nope".to_string()))
    );
    let gone = vec![
        segs[0].clone(),
        PathSegment {
            field_id: Some(999),
            name: "x".to_string(),
        },
    ];
    assert_eq!(
        resolve_path("Box", &gone, defs),
        Err(DanglingReason::FieldGone { depth: 1 }),
        "an id that is gone is not resolved by its name"
    );
    let through_leaf = vec![
        PathSegment {
            field_id: Some(field_id(&d, "Box", "size").0),
            name: "size".to_string(),
        },
        segs[1].clone(),
    ];
    assert_eq!(
        resolve_path("Box", &through_leaf, defs),
        Err(DanglingReason::NotARecord { depth: 0 })
    );

    // A nested def that is missing.
    let mut d2 = setup();
    d2.node_type_registry
        .delete_record_type_def("Point")
        .unwrap();
    assert_eq!(
        resolve_path("Box", &segs, d2.node_type_registry.record_defs()),
        Err(DanglingReason::DefMissing("Point".to_string()))
    );
}

#[test]
fn the_builder_refuses_only_on_a_missing_def() {
    use atomcad_structure_designer::node_data::NodeData;
    let mut d = standard();
    let base = d.node_type_registry.built_in_node_types["record_get"].clone();
    let data = g_data(&d);
    let defs = d.node_type_registry.record_defs();
    assert!(
        data.build_record_node_type(&base, defs, MissingDefs::Refuse)
            .is_some()
    );
    assert_eq!(data.record_path_defs(defs), vec!["Point".to_string()]);

    // A deleted field is a dangling row, not a refusal.
    set_point_fields(&mut d, &["y"]);
    let defs = d.node_type_registry.record_defs();
    let nt = data
        .build_record_node_type(&base, defs, MissingDefs::Refuse)
        .unwrap();
    assert_eq!(nt.output_pins.len(), 1);

    // A missing nested def is.
    d.node_type_registry
        .delete_record_type_def("Point")
        .unwrap();
    let defs = d.node_type_registry.record_defs();
    assert!(
        data.build_record_node_type(&base, defs, MissingDefs::Refuse)
            .is_none()
    );
    assert_eq!(data.record_path_defs(defs), vec!["Point".to_string()]);
    assert!(
        data.build_record_node_type(&base, defs, MissingDefs::Dangle)
            .is_some()
    );
}

// ============================================================================
// Text round trip (`query` → `--replace` is a no-op)
// ============================================================================

/// Replaces `Main` with its own text and returns (before, after).
fn replace_with_own_text(d: &mut StructureDesigner) -> (String, String) {
    let before = main_text(d);
    d.set_active_node_network_name(Some("Main".to_string()));
    let outcome = d.ai_text_edit(&before, true);
    // Validation verdicts are folded into `errors` (a frozen node has one);
    // the edit itself must have applied.
    assert!(
        outcome
            .result
            .errors
            .iter()
            .all(|e| !e.contains("Error setting properties")),
        "{:?}",
        outcome.result.errors
    );
    d.validate_active_network();
    (before, main_text(d))
}

#[test]
fn query_then_replace_is_a_no_op() {
    // Live rows, after a nested rename (current names are printed).
    let mut d = standard();
    let edits = vec![
        existing(&d, "Point", "x", "px"),
        existing(&d, "Point", "y", "y"),
    ];
    d.update_record_type_def_with_ids("Point", edits).unwrap();
    let (before, after) = replace_with_own_text(&mut d);
    assert_eq!(before, after);
    assert_eq!(out_pins(&d, "g"), vec!["p_px", "size"]);
    assert_eq!(reads_pin(&d, "es"), Some(1));
    assert_eq!(float(value(&mut d, "ex", 0)), 101.5);
}

#[test]
fn a_frozen_record_get_survives_query_then_replace() {
    let ws = linked_workspace(true);
    std::fs::remove_file(&ws.lib).unwrap();
    let mut d = open(&ws.host);
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    assert!(g_frozen(&d));
    let (before, after) = replace_with_own_text(&mut d);
    assert_eq!(before, after);
    assert!(before.contains("paths: [\"p.x\", \"size\"]"), "{}", before);
    assert!(g_frozen(&d));
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
}

#[test]
fn a_dangling_row_round_trips_through_replace() {
    // The getter prints `!p.x`, and since Phase 3 the setter takes it back
    // (`record_path_rows_test.rs` has the details).
    let mut d = standard();
    set_point_fields(&mut d, &["y"]);
    let (before, after) = replace_with_own_text(&mut d);
    assert_eq!(before, after);
    assert!(after.contains("\"!p.x\""), "{}", after);
    assert_eq!(g_data(&d).rows.len(), 2);
}
