//! Row editing for `record_get` / `record_set` (`doc/design_record_get_set.md`
//! Phase 3): `merge_rows` through the `StructureDesigner` setters and the text
//! format, schema changes, dangling rows in by-name edits, undo, `.cnnd`
//! round trips and loader healing.
//!
//! Most tests build `Box { p: Point, size: Float }` / `Point { x, y: Float }`,
//! a `Box` value `b`, and a `record_get` `g` and/or a `record_set` `s` on
//! `Box`, then edit the rows and check pins, wires, values and undo.

use super::library_links_support::*;
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type_registry::{FieldId, RecordFieldEdit, RecordTypeDef};
use atomcad_structure_designer::nodes::record_get::RecordGetData;
use atomcad_structure_designer::nodes::record_set::RecordSetData;
use atomcad_structure_designer::record_path::{PathSegment, PathSpec, parse_path_spec};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::{TextValue, serialize_network};
use serde_json::Value;

// ============================================================================
// Helpers
// ============================================================================

fn named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

fn def(name: &str, fields: &[(&str, DataType)]) -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        name,
        fields
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect(),
    )
}

/// A designer with `Point { x, y }` and `Box { p: Point, size }` and an
/// active, empty `Main`.
fn setup() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network("Main");
    d.set_active_node_network_name(Some("Main".to_string()));
    d.add_record_type_def(def(
        "Point",
        &[("x", DataType::Float), ("y", DataType::Float)],
    ))
    .unwrap();
    d.add_record_type_def(def(
        "Box",
        &[("p", named("Point")), ("size", DataType::Float)],
    ))
    .unwrap();
    d
}

/// The `Box` value `b`: `p = (1.5, 2.5)`, `size = 4.0`; plus spare floats
/// `f9`, `f7`, `f8`.
const BOX_VALUE: &str = "fx = float { value: 1.5 }
fy = float { value: 2.5 }
fs = float { value: 4.0 }
f9 = float { value: 9.0 }
f7 = float { value: 7.0 }
f8 = float { value: 8.0 }
pt = record_construct { schema: \"Point\", x: fx, y: fy }
b = record_construct { schema: \"Box\", p: pt, size: fs }
";

fn expr(name: &str, input: &str, add: &str) -> String {
    format!(
        "{name} = expr {{ a: {input}, expression: \"a + {add}\", parameters: [{{ name: \"a\", data_type: Float }}] }}\n"
    )
}

/// `setup` plus `BOX_VALUE` plus `code`.
fn with(code: &str) -> StructureDesigner {
    let mut d = setup();
    edit(&mut d, "Main", &format!("{}{}", BOX_VALUE, code));
    d
}

/// `g` with rows `[p.x, size]`, read by `ex` (+100) and `es` (+200).
fn get2() -> StructureDesigner {
    with(&format!(
        "g = record_get {{ schema: \"Box\", paths: [\"p.x\", \"size\"], record: b }}\n{}{}",
        expr("ex", "g", "100.0"),
        expr("es", "g.size", "200.0")
    ))
}

/// `g` with rows `[p.x, p.y, size]`, read by `ex`, `ey` (+300) and `es`.
fn get3() -> StructureDesigner {
    with(&format!(
        "g = record_get {{ schema: \"Box\", paths: [\"p.x\", \"p.y\", \"size\"], record: b }}\n{}{}{}",
        expr("ex", "g", "100.0"),
        expr("ey", "g.p_y", "300.0"),
        expr("es", "g.size", "200.0")
    ))
}

/// Applies `code` to `Main` as an incremental edit (not `--replace`) and
/// returns its warnings; errors fail the test.
fn update(d: &mut StructureDesigner, code: &str) -> Vec<String> {
    d.set_active_node_network_name(Some("Main".to_string()));
    let outcome = d.ai_text_edit(code, false);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}\n{}",
        outcome.result.errors,
        code
    );
    d.validate_active_network();
    outcome.result.warnings
}

fn specs(paths: &[&str]) -> Vec<PathSpec> {
    paths.iter().map(|p| parse_path_spec(p).unwrap()).collect()
}

fn network<'a>(d: &'a StructureDesigner, name: &str) -> &'a NodeNetwork {
    &d.node_type_registry.node_networks[name]
}

fn id(d: &StructureDesigner, name: &str) -> u64 {
    node_id(d, "Main", name)
}

fn out_pins(d: &StructureDesigner, name: &str) -> Vec<String> {
    network(d, "Main").nodes[&id(d, name)]
        .custom_node_type
        .as_ref()
        .unwrap()
        .output_pins
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

fn in_pins(d: &StructureDesigner, name: &str) -> Vec<String> {
    network(d, "Main").nodes[&id(d, name)]
        .custom_node_type
        .as_ref()
        .unwrap()
        .parameters
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

/// `(source node, output pin)` of every wire into `arg` of `node` in `net`.
fn wires_in(net: &NodeNetwork, node: u64, arg: usize) -> Vec<(u64, i32)> {
    net.nodes[&node]
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
    let g = id(d, "g");
    wires_in(network(d, "Main"), id(d, dest), 0)
        .into_iter()
        .find(|(s, _)| *s == g)
        .map(|(_, p)| p)
}

/// Whether input `arg` of `name` reads `source`.
fn reads(d: &StructureDesigner, name: &str, arg: usize, source: &str) -> bool {
    let source = id(d, source);
    wires_in(network(d, "Main"), id(d, name), arg)
        .iter()
        .any(|(s, _)| *s == source)
}

fn errors_on(d: &StructureDesigner, name: &str) -> Vec<(String, bool)> {
    let node = id(d, name);
    network(d, "Main")
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(node))
        .map(|e| (e.error_text.clone(), e.blocking))
        .collect()
}

fn value(d: &mut StructureDesigner, name: &str, pin: i32) -> NetworkResult {
    d.set_active_node_network_name(Some("Main".to_string()));
    let node = id(d, name);
    d.evaluate_node_output(&[], node, pin)
}

fn float(v: NetworkResult) -> f64 {
    match v {
        NetworkResult::Float(f) => f,
        other => panic!("expected a Float, got {}", other.to_display_string()),
    }
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

fn g_data(d: &StructureDesigner) -> RecordGetData {
    network(d, "Main").nodes[&id(d, "g")]
        .data
        .as_any_ref()
        .downcast_ref::<RecordGetData>()
        .unwrap()
        .clone()
}

fn s_data_of(d: &StructureDesigner, name: &str) -> RecordSetData {
    network(d, "Main").nodes[&id(d, name)]
        .data
        .as_any_ref()
        .downcast_ref::<RecordSetData>()
        .unwrap()
        .clone()
}

fn s_data(d: &StructureDesigner) -> RecordSetData {
    s_data_of(d, "s")
}

fn row_ids(
    rows: &[impl AsRef<atomcad_structure_designer::nodes::record_path_rows::RecordPathRow>],
) -> Vec<u64> {
    rows.iter().map(|r| r.as_ref().id.unwrap()).collect()
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

fn rename_point_x(d: &mut StructureDesigner, new_name: &str) {
    let edit = |field: &str, name: &str| {
        let f = d
            .node_type_registry
            .lookup_record_type_def("Point")
            .unwrap()
            .fields
            .iter()
            .find(|f| f.name == field)
            .unwrap()
            .clone();
        RecordFieldEdit {
            id: Some(f.id),
            name: name.to_string(),
            data_type: f.data_type,
            hint: None,
        }
    };
    let edits = vec![edit("x", new_name), edit("y", "y")];
    d.update_record_type_def_with_ids("Point", edits).unwrap();
    d.validate_active_network();
}

fn main_text(d: &StructureDesigner) -> String {
    serialize_network(network(d, "Main"), &d.node_type_registry, Some("Main"))
}

/// `Main`'s undo snapshot as JSON, with the arrays that come from hash-map
/// iteration sorted.
fn fingerprint(d: &mut StructureDesigner) -> Value {
    fn normalize(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, val) in map.iter_mut() {
                    if matches!(
                        key.as_str(),
                        "nodes" | "displayed_node_ids" | "displayed_output_pins"
                    ) && let Value::Array(arr) = val
                    {
                        arr.sort_by_key(|v| v.to_string());
                    }
                    normalize(val);
                }
            }
            Value::Array(arr) => arr.iter_mut().for_each(normalize),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(d.snapshot_network("Main").unwrap()).unwrap();
    normalize(&mut value);
    value
}

fn pushes(d: &StructureDesigner) -> u64 {
    d.undo_stack.push_count()
}

/// Replaces `Main` with its own text and returns (before, after).
fn replace_with_own_text(d: &mut StructureDesigner) -> (String, String) {
    let before = main_text(d);
    d.set_active_node_network_name(Some("Main".to_string()));
    let outcome = d.ai_text_edit(&before, true);
    assert!(
        outcome
            .result
            .errors
            .iter()
            .all(|e| !e.contains("Error setting properties")),
        "{:?}",
        outcome.result.errors
    );
    // Validation warnings (a dangling row's) are expected; the editor's own
    // complaints about keys are not.
    assert!(
        outcome
            .result
            .warnings
            .iter()
            .all(|w| !w.contains("property") && !w.contains("literal")),
        "{:?}",
        outcome.result.warnings
    );
    d.validate_active_network();
    (before, main_text(d))
}

// ============================================================================
// 1. Removing a middle row: wires follow ids; undo, redo, no-op
// ============================================================================

#[test]
fn removing_a_middle_record_get_row_keeps_the_other_output_wires() {
    let mut d = get3();
    let g = id(&d, "g");
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "p_y", "size"]);
    let before = fingerprint(&mut d);
    let n = pushes(&d);

    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["p.x", "size"]))
        .unwrap();
    assert_eq!(pushes(&d), n + 1);
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1), "size moved to pin 1");
    assert_eq!(reads_pin(&d, "ey"), None, "the removed row's wire drops");
    assert_eq!(float(value(&mut d, "ex", 0)), 101.5);
    assert_eq!(float(value(&mut d, "es", 0)), 204.0);
    let after = fingerprint(&mut d);

    assert!(d.undo());
    assert_eq!(fingerprint(&mut d), before);
    assert_eq!(reads_pin(&d, "ey"), Some(1));
    assert!(d.redo());
    assert_eq!(fingerprint(&mut d), after);

    // Re-sending the unchanged rows pushes nothing.
    let n = pushes(&d);
    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["p.x", "size"]))
        .unwrap();
    assert_eq!(pushes(&d), n);
}

#[test]
fn removing_a_middle_record_set_row_keeps_the_other_input_wires() {
    let mut d = with(
        "s = record_set { schema: \"Box\", paths: [\"p.x\", \"p.y\", \"size\"], record: b, p_x: f9, p_y: f7, size: f8 }\n",
    );
    let s = id(&d, "s");
    let before = fingerprint(&mut d);

    d.set_record_set_data(&[], s, "Box".to_string(), specs(&["p.x", "size"]))
        .unwrap();
    assert_eq!(in_pins(&d, "s"), vec!["record", "p_x", "size"]);
    assert!(reads(&d, "s", 1, "f9"));
    assert!(reads(&d, "s", 2, "f8"), "size's wire followed it to pin 2");
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "x"]), 9.0);
    assert_eq!(
        field(&v, &["p", "y"]),
        2.5,
        "the removed row no longer sets"
    );
    assert_eq!(field(&v, &["size"]), 8.0);
    let after = fingerprint(&mut d);

    assert!(d.undo());
    assert_eq!(fingerprint(&mut d), before);
    assert!(d.redo());
    assert_eq!(fingerprint(&mut d), after);

    let n = pushes(&d);
    d.set_record_set_data(&[], s, "Box".to_string(), specs(&["p.x", "size"]))
        .unwrap();
    assert_eq!(pushes(&d), n);
}

// ============================================================================
// 2. In-place path edit (positional fallback), literals
// ============================================================================

#[test]
fn editing_a_path_in_place_keeps_its_wire() {
    let mut d = get2();
    let g = id(&d, "g");
    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["p.y", "size"]))
        .unwrap();
    assert_eq!(out_pins(&d, "g"), vec!["p_y", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(float(value(&mut d, "ex", 0)), 102.5, "now reads p.y");
    assert_eq!(row_ids(&g_data(&d).rows), vec![0, 1]);
}

#[test]
fn a_literal_follows_its_row_while_it_still_fits() {
    let mut d = with(
        "s = record_set { schema: \"Box\", paths: [\"p.x\", \"size\"], record: b, p_x: 3.0 }\n",
    );
    let s = id(&d, "s");
    d.set_record_set_data(&[], s, "Box".to_string(), specs(&["p.y", "size"]))
        .unwrap();
    let data = s_data(&d);
    assert_eq!(data.rows[0].literal, Some(TextValue::Float(3.0)));
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "y"]), 3.0);
    assert_eq!(field(&v, &["p", "x"]), 1.5);

    // A literal that does not convert to the new leaf type is dropped.
    d.add_record_type_def(def(
        "Tag",
        &[("w", DataType::Float), ("label", DataType::String)],
    ))
    .unwrap();
    update(
        &mut d,
        "t = record_set { schema: \"Tag\", paths: [\"w\"], w: 3.0 }\n",
    );
    let t = id(&d, "t");
    let row_id = s_data_of(&d, "t").rows[0].row.id;
    d.set_record_set_data(&[], t, "Tag".to_string(), specs(&["label"]))
        .unwrap();
    let data = s_data_of(&d, "t");
    assert_eq!(data.rows[0].row.id, row_id, "same row");
    assert_eq!(data.rows[0].literal, None);
}

// ============================================================================
// 3–4. No positional steal; ids are never reused
// ============================================================================

#[test]
fn a_reorder_does_not_steal_an_id_positionally() {
    let mut d = get2();
    let g = id(&d, "g");
    // [a, b] → [c, a]: `a` (p.x) keeps its id and its wire.
    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["p.y", "p.x"]))
        .unwrap();
    assert_eq!(out_pins(&d, "g"), vec!["p_y", "p_x"]);
    assert_eq!(reads_pin(&d, "ex"), Some(1));
    assert_eq!(float(value(&mut d, "ex", 0)), 101.5);
    assert_eq!(reads_pin(&d, "es"), None);
    assert_eq!(row_ids(&g_data(&d).rows), vec![2, 0]);
}

#[test]
fn removing_the_newest_row_and_adding_one_mints_a_new_id() {
    let mut d = get2();
    let g = id(&d, "g");
    assert_eq!(g_data(&d).next_row_id, 2);
    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["p.x"]))
        .unwrap();
    assert_eq!(g_data(&d).next_row_id, 2);
    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["p.x", "p.y"]))
        .unwrap();
    assert_eq!(row_ids(&g_data(&d).rows), vec![0, 2]);
    assert_eq!(g_data(&d).next_row_id, 3);
}

// ============================================================================
// 5. Rejections leave the node unchanged
// ============================================================================

#[test]
fn bad_path_lists_are_rejected_and_change_nothing() {
    let mut d = with(&format!(
        "g = record_get {{ schema: \"Box\", paths: [\"p.x\", \"size\"], record: b }}\n{}s = record_set {{ schema: \"Box\", paths: [\"p.x\"], record: b, p_x: 3.0 }}\n",
        expr("ex", "g", "100.0")
    ));
    let (g, s) = (id(&d, "g"), id(&d, "s"));
    let bad: Vec<Vec<PathSpec>> = vec![
        specs(&["p.x", "p.x"]),
        vec![PathSpec::Live(vec![])],
        specs(&["p.z"]),
        specs(&["size.x"]),
        specs(&["q"]),
    ];
    let before = fingerprint(&mut d);
    let n = pushes(&d);
    for paths in bad {
        assert!(
            d.set_record_get_data(&[], g, "Box".to_string(), paths.clone())
                .is_err(),
            "{:?}",
            paths
        );
        assert!(
            d.set_record_set_data(&[], s, "Box".to_string(), paths.clone())
                .is_err(),
            "{:?}",
            paths
        );
    }
    assert!(
        d.set_record_get_data(&[], s, "Box".to_string(), specs(&["size"]))
            .is_err(),
        "not a record_get"
    );
    assert_eq!(pushes(&d), n);
    assert_eq!(fingerprint(&mut d), before);
}

// ============================================================================
// 6. Schema change
// ============================================================================

/// `get2` without the `Box` value, plus `Box2 { size, p: Point, extra }` and
/// `Box3 { p: Point }`.
fn schema_setup() -> StructureDesigner {
    let mut d = setup();
    d.add_record_type_def(def(
        "Box2",
        &[
            ("size", DataType::Float),
            ("p", named("Point")),
            ("extra", DataType::Float),
        ],
    ))
    .unwrap();
    d.add_record_type_def(def("Box3", &[("p", named("Point"))]))
        .unwrap();
    edit(
        &mut d,
        "Main",
        &format!(
            "g = record_get {{ schema: \"Box\", paths: [\"p.x\", \"size\"] }}\n{}{}",
            expr("ex", "g", "100.0"),
            expr("es", "g.size", "200.0")
        ),
    );
    d
}

#[test]
fn a_schema_change_re_resolves_rows_by_name_and_keeps_their_ids() {
    let mut d = schema_setup();
    let g = id(&d, "g");
    let original = fingerprint(&mut d);

    d.set_record_get_data(&[], g, "Box2".to_string(), specs(&["p.x", "size"]))
        .unwrap();
    let data = g_data(&d);
    assert_eq!(data.schema, "Box2");
    assert_eq!(row_ids(&data.rows), vec![0, 1]);
    assert_eq!(
        data.rows[1].path[0].field_id,
        Some(field_id(&d, "Box2", "size").0),
        "re-resolved against the new def"
    );
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "ex"), Some(0));
    assert_eq!(reads_pin(&d, "es"), Some(1));
    let on_box2 = fingerprint(&mut d);

    // `Box3` has no `size`: that row goes dangling (its spec claims it).
    d.set_record_get_data(&[], g, "Box3".to_string(), specs(&["p.x", "size"]))
        .unwrap();
    let data = g_data(&d);
    assert_eq!(row_ids(&data.rows), vec![0, 1]);
    assert_eq!(out_pins(&d, "g"), vec!["p_x"]);
    assert_eq!(reads_pin(&d, "es"), None);
    let errors = errors_on(&d, "g");
    assert!(
        errors.len() == 1 && !errors[0].1 && errors[0].0.contains("`size`"),
        "{:?}",
        errors
    );
    assert!(
        main_text(&d).contains("paths: [\"p.x\", \"!size\"]"),
        "{}",
        main_text(&d)
    );

    assert!(d.undo());
    assert_eq!(fingerprint(&mut d), on_box2);
    assert!(d.undo());
    assert_eq!(fingerprint(&mut d), original);
}

#[test]
fn a_schema_only_text_edit_keeps_the_rows() {
    let mut d = schema_setup();
    update(&mut d, "g = record_get { schema: \"Box2\" }\n");
    let data = g_data(&d);
    assert_eq!(data.schema, "Box2");
    assert_eq!(row_ids(&data.rows), vec![0, 1]);
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    assert_eq!(reads_pin(&d, "es"), Some(1));
}

#[test]
fn a_schema_change_uses_current_names_not_stale_stored_ones() {
    // After `Point.x` → `px`, the row's stored name is still `x`. The new
    // schema's point type has both `px` and `x`: the row must bind to `px`.
    for through_text in [false, true] {
        let mut d = schema_setup();
        rename_point_x(&mut d, "px");
        assert_eq!(g_data(&d).rows[0].path[1].name, "x", "stored name is stale");
        d.add_record_type_def(def(
            "Point2",
            &[("px", DataType::Float), ("x", DataType::Float)],
        ))
        .unwrap();
        d.add_record_type_def(def(
            "BoxP2",
            &[("p", named("Point2")), ("size", DataType::Float)],
        ))
        .unwrap();
        if through_text {
            update(&mut d, "g = record_get { schema: \"BoxP2\" }\n");
        } else {
            let g = id(&d, "g");
            d.set_record_get_data(&[], g, "BoxP2".to_string(), specs(&["p.px", "size"]))
                .unwrap();
        }
        let data = g_data(&d);
        assert_eq!(
            data.rows[0].path[1].field_id,
            Some(field_id(&d, "Point2", "px").0),
            "through_text: {}",
            through_text
        );
        assert_eq!(out_pins(&d, "g"), vec!["p_px", "size"]);
        assert_eq!(reads_pin(&d, "ex"), Some(0));
    }
}

// ============================================================================
// 7. Dangling rows in by-name edits
// ============================================================================

#[test]
fn a_dangling_row_survives_edits_of_the_other_rows() {
    let mut d = get2();
    let g = id(&d, "g");
    set_point_fields(&mut d, &["y"]);
    let dangling = g_data(&d).rows[0].clone();
    assert_eq!(reads_pin(&d, "es"), Some(0));

    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["!p.x", "size", "p.y"]))
        .unwrap();
    let data = g_data(&d);
    assert_eq!(data.rows[0], dangling, "kept verbatim");
    assert_eq!(row_ids(&data.rows), vec![0, 1, 2]);
    assert_eq!(out_pins(&d, "g"), vec!["size", "p_y"]);
    assert_eq!(reads_pin(&d, "es"), Some(0), "size's wire untouched");

    // Echoing the rows back is a no-op.
    let n = pushes(&d);
    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["!p.x", "size", "p.y"]))
        .unwrap();
    assert_eq!(pushes(&d), n);

    // Leaving the dangling row out deletes it.
    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["size", "p.y"]))
        .unwrap();
    assert_eq!(row_ids(&g_data(&d).rows), vec![1, 2]);
    assert!(errors_on(&d, "g").is_empty(), "{:?}", errors_on(&d, "g"));
}

#[test]
fn a_dangling_record_set_row_keeps_its_literal() {
    let mut d = with(
        "s = record_set { schema: \"Box\", paths: [\"p.x\", \"size\"], record: b, p_x: 3.0 }\n",
    );
    let s = id(&d, "s");
    set_point_fields(&mut d, &["y"]);
    d.set_record_set_data(&[], s, "Box".to_string(), specs(&["!p.x", "size", "p.y"]))
        .unwrap();
    let data = s_data(&d);
    assert_eq!(data.rows[0].literal, Some(TextValue::Float(3.0)));
    assert_eq!(in_pins(&d, "s"), vec!["record", "size", "p_y"]);
}

#[test]
fn a_live_path_in_a_dangling_row_s_place_takes_its_id() {
    let mut d = get2();
    let g = id(&d, "g");
    set_point_fields(&mut d, &["y"]);
    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["p.y", "size"]))
        .unwrap();
    assert_eq!(row_ids(&g_data(&d).rows), vec![0, 1]);
    assert!(errors_on(&d, "g").is_empty());
}

#[test]
fn a_dangling_path_naming_no_row_is_recreated_as_a_broken_row() {
    // A `--replace` builds every node afresh, so a `!path` usually has no
    // old row to claim; the row is recreated, and stays broken.
    let mut d = get2();
    let g = id(&d, "g");
    d.set_record_get_data(&[], g, "Box".to_string(), specs(&["p.x", "size", "!q.r"]))
        .unwrap();
    let data = g_data(&d);
    assert_eq!(row_ids(&data.rows), vec![0, 1, 2]);
    assert_eq!(
        data.rows[2].path,
        vec![
            PathSegment::gone("q".to_string()),
            PathSegment::gone("r".to_string())
        ]
    );
    assert_eq!(out_pins(&d, "g"), vec!["p_x", "size"]);
    let errors = errors_on(&d, "g");
    assert!(
        errors.len() == 1 && !errors[0].1 && errors[0].0.contains("`q.r`"),
        "{:?}",
        errors
    );
    assert!(
        main_text(&d).contains("paths: [\"p.x\", \"size\", \"!q.r\"]"),
        "{}",
        main_text(&d)
    );
}

#[test]
fn a_recreated_dangling_row_does_not_bind_to_a_re_added_field() {
    let mut d = get2();
    set_point_fields(&mut d, &["y"]);
    let (before, after) = replace_with_own_text(&mut d);
    assert_eq!(before, after);
    assert!(after.contains("\"!p.x\""), "{}", after);
    // A new `x` is a different field: the row stays broken.
    set_point_fields(&mut d, &["y", "x"]);
    assert_eq!(out_pins(&d, "g"), vec!["size"]);
    assert!(main_text(&d).contains("\"!p.x\""), "{}", main_text(&d));
}

// ============================================================================
// 8. Text format
// ============================================================================

#[test]
fn the_canonical_example_parses_and_wires() {
    let mut d = setup();
    d.set_active_node_network_name(Some("Main".to_string()));
    let code = format!(
        "{}g = record_get {{ schema: \"Box\", paths: [\"p.x\", \"size\"], record: b }}
s = record_set {{ schema: \"Box\", paths: [\"p.x\", \"size\"], record: b, p_x: 3.0, size: g.size }}
",
        BOX_VALUE
    );
    let outcome = d.ai_text_edit(&code, true);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    assert!(
        outcome.result.warnings.is_empty(),
        "the block creates the `p_x` key it sets: {:?}",
        outcome.result.warnings
    );
    d.validate_active_network();
    assert_eq!(s_data(&d).rows[0].literal, Some(TextValue::Float(3.0)));
    assert!(reads(&d, "s", 0, "b"));
    assert!(reads(&d, "s", 2, "g"));
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["p", "x"]), 3.0);
    assert_eq!(field(&v, &["p", "y"]), 2.5);
    assert_eq!(field(&v, &["size"]), 4.0);
    let text = main_text(&d);
    assert!(
        text.contains(
            "s = record_set { record: b, size: g.size, schema: \"Box\", paths: [\"p.x\", \"size\"], p_x: 3.0 }"
        ),
        "{}",
        text
    );
}

/// `Inner { b, gone }` and `Odd { a_b, a: Inner, `c.d`, `!e`, schema }`.
fn odd_setup() -> StructureDesigner {
    let mut d = setup();
    d.add_record_type_def(def(
        "Inner",
        &[("b", DataType::Float), ("gone", DataType::Float)],
    ))
    .unwrap();
    d.add_record_type_def(def(
        "Odd",
        &[
            ("a_b", DataType::Float),
            ("a", named("Inner")),
            ("c.d", DataType::Float),
            ("!e", DataType::Float),
            ("schema", DataType::Float),
        ],
    ))
    .unwrap();
    edit(
        &mut d,
        "Main",
        "g = record_get { schema: \"Odd\", paths: [\"a.b\", \"a_b\", \"`c.d`\", \"`!e`\", \"a.gone\"] }
s = record_set { schema: \"Odd\", paths: [\"a_b\", \"a.b\", \"schema\", \"a.gone\"], a_b__2: 1.0, schema__2: 2.0, a_gone: 5.0 }
",
    );
    d
}

#[test]
fn query_then_replace_is_a_no_op_with_every_corner() {
    let mut d = odd_setup();
    assert_eq!(
        out_pins(&d, "g"),
        vec!["a_b", "a_b__2", "c.d", "!e", "a_gone"]
    );
    assert_eq!(
        in_pins(&d, "s"),
        vec!["record", "a_b", "a_b__2", "schema__2", "a_gone"]
    );
    // Dangle `a.gone` on both nodes.
    d.update_record_type_def("Inner", vec![("b".to_string(), DataType::Float)])
        .unwrap();
    d.validate_active_network();

    let (before, after) = replace_with_own_text(&mut d);
    assert_eq!(before, after);
    for expected in [
        "paths: [\"a.b\", \"a_b\", \"`c.d`\", \"`!e`\", \"!a.gone\"]",
        "paths: [\"a_b\", \"a.b\", \"schema\", \"!a.gone\"], a_b__2: 1.0, schema__2: 2.0 }",
    ] {
        assert!(after.contains(expected), "{}\n--- in\n{}", expected, after);
    }
    assert!(
        !after.contains("a_gone"),
        "a dangling row's literal is not printed"
    );
    assert_eq!(errors_on(&d, "g").len(), 1);
    assert_eq!(errors_on(&d, "s").len(), 1);
}

#[test]
fn one_block_changes_schema_paths_and_a_literal() {
    let mut d = with("s = record_set { schema: \"Box\", paths: [\"p.x\"], record: b, p_x: 3.0 }\n");
    d.add_record_type_def(def("BoxS", &[("size", DataType::Float)]))
        .unwrap();
    let warnings = update(
        &mut d,
        "s = record_set { schema: \"BoxS\", paths: [\"size\"], size: 6.0 }\n",
    );
    assert!(warnings.is_empty(), "{:?}", warnings);
    let data = s_data(&d);
    assert_eq!(data.schema, "BoxS");
    assert_eq!(data.rows.len(), 1);
    assert_eq!(data.rows[0].row.path[0].name, "size");
    assert_eq!(data.rows[0].literal, Some(TextValue::Float(6.0)));
    let v = value(&mut d, "s", 0);
    assert_eq!(field(&v, &["size"]), 6.0);
}

#[test]
fn a_literal_replaces_a_wire_and_a_wire_hides_a_literal() {
    let mut d = with(
        "s = record_set { schema: \"Box\", paths: [\"p.x\", \"size\"], record: b, p_x: f9 }\n",
    );
    // A literal on a wired row without a stored literal: accepted, and it
    // clears the wire.
    let warnings = update(&mut d, "s = record_set { p_x: 3.0 }\n");
    assert!(warnings.is_empty(), "{:?}", warnings);
    assert!(!reads(&d, "s", 1, "f9"));
    assert_eq!(s_data(&d).rows[0].literal, Some(TextValue::Float(3.0)));
    assert_eq!(field(&value(&mut d, "s", 0), &["p", "x"]), 3.0);

    // Wired again: the stored literal stays but only the wire is printed.
    update(&mut d, "s = record_set { p_x: f9 }\n");
    assert_eq!(s_data(&d).rows[0].literal, Some(TextValue::Float(3.0)));
    let text = main_text(&d);
    assert!(text.contains("p_x: f9"), "{}", text);
    assert!(!text.contains("p_x: 3.0"), "{}", text);

    // A literal that does not fit the field is an error.
    d.set_active_node_network_name(Some("Main".to_string()));
    let outcome = d.ai_text_edit("s = record_set { size: \"big\" }\n", false);
    assert!(!outcome.result.errors.is_empty());
    assert_eq!(s_data(&d).rows[1].literal, None);
}

#[test]
fn an_unknown_key_still_warns() {
    let mut d = with("s = record_set { schema: \"Box\", paths: [\"size\"], record: b }\n");
    let warnings = update(&mut d, "s = record_set { p_x: 3.0 }\n");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("Unknown property 'p_x'")),
        "{:?}",
        warnings
    );
}

// ============================================================================
// 9. `.cnnd` round trip and healing
// ============================================================================

#[test]
fn record_get_set_cnnd_roundtrip() {
    let mut d = with(&format!(
        "g = record_get {{ schema: \"Box\", paths: [\"p.x\", \"size\"], record: b }}\n{}{}s = record_set {{ schema: \"Box\", paths: [\"p.x\", \"p.y\", \"size\"], record: b, p_x: 3.0, p_y: 5.0, size: f7 }}\n",
        expr("ex", "g", "100.0"),
        expr("es", "g.size", "200.0")
    ));
    set_point_fields(&mut d, &["x"]); // `p.y` dangles on `s`
    let text = main_text(&d);
    let (g, s) = (g_data(&d), s_data(&d));
    assert!(text.contains("\"!p.y\""), "{}", text);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rows.cnnd");
    d.save_node_networks_as(&path.to_string_lossy()).unwrap();
    let mut d2 = open(&path);
    d2.set_active_node_network_name(Some("Main".to_string()));
    d2.validate_active_network();
    assert_eq!(main_text(&d2), text);
    assert_eq!(g_data(&d2), g);
    assert_eq!(s_data(&d2), s, "the dangling row's literal survives too");
    assert_eq!(float(value(&mut d2, "ex", 0)), 101.5);
    assert_eq!(field(&value(&mut d2, "s", 0), &["size"]), 7.0);
}

#[test]
fn the_loaders_heal_row_state() {
    let d = setup();
    let rows = serde_json::json!([
        { "path": [{ "field_id": 0, "name": "p" }, { "field_id": 0, "name": "x" }] },
        { "id": 5, "path": [{ "field_id": 1, "name": "size" }] },
        { "id": 7, "path": [{ "field_id": 1, "name": "size" }], "literal": { "type": "Float", "value": 2.0 } },
        { "id": 8, "path": [] }
    ]);
    let json = serde_json::json!({ "schema": "Box", "rows": rows, "next_row_id": 0 });

    let load = |node: &str| {
        (d.node_type_registry.built_in_node_types[node].node_data_loader)(&json, None).unwrap()
    };
    let g = load("record_get");
    let g = g.as_any_ref().downcast_ref::<RecordGetData>().unwrap();
    assert_eq!(row_ids(&g.rows), vec![6, 5]);
    assert_eq!(g.next_row_id, 7);
    assert_eq!(g.rows[1].path[0].name, "size");

    let s = load("record_set");
    let s = s.as_any_ref().downcast_ref::<RecordSetData>().unwrap();
    assert_eq!(row_ids(&s.rows), vec![6, 5]);
    assert_eq!(s.next_row_id, 7);
    assert_eq!(
        s.rows[1].literal, None,
        "the later duplicate is the one dropped"
    );

    // Healing is idempotent.
    let mut again = g.clone();
    again.heal();
    assert_eq!(&again, g);
}

// ============================================================================
// 11. Body-internal nodes through `scope_path`
// ============================================================================

#[test]
fn the_setters_reach_a_body_internal_node_and_undo_restores_it() {
    let mut d = setup();
    d.add_record_type_def(def("Box3", &[("p", named("Point"))]))
        .unwrap();
    edit(
        &mut d,
        "Main",
        &format!(
            "{}r = range {{ count: 2 }}
mp = map {{ xs: r, input_type: Int, output_type: Float, body {{ g = record_get {{ schema: \"Box\", paths: [\"p.x\", \"p.y\", \"size\"], record: ^b }} e = expr {{ a: g.size, expression: \"a + 1.0\", parameters: [{{ name: \"a\", data_type: Float }}] }} s = record_set {{ schema: \"Box\", paths: [\"p.x\", \"p.y\", \"size\"], record: ^b, size: ^f7 }} output e }} }}
",
            BOX_VALUE
        ),
    );
    let mp = id(&d, "mp");
    let body = |d: &StructureDesigner| -> NodeNetwork {
        (**network(d, "Main").nodes[&mp].zone.as_ref().unwrap()).clone()
    };
    let named_in = |net: &NodeNetwork, name: &str| {
        net.nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some(name))
            .unwrap()
            .id
    };
    let b0 = body(&d);
    let (g, e, s) = (named_in(&b0, "g"), named_in(&b0, "e"), named_in(&b0, "s"));
    assert_eq!(wires_in(&b0, e, 0), vec![(g, 2)]);
    let original = fingerprint(&mut d);

    d.set_record_get_data(&[mp], g, "Box".to_string(), specs(&["p.x", "size"]))
        .unwrap();
    d.set_record_set_data(&[mp], s, "Box".to_string(), specs(&["size"]))
        .unwrap();
    let b1 = body(&d);
    assert_eq!(wires_in(&b1, e, 0), vec![(g, 1)], "remapped in the body");
    assert_eq!(b1.nodes[&s].arguments.len(), 2);
    assert_eq!(
        wires_in(&b1, s, 1).len(),
        1,
        "the size capture followed its row"
    );
    let edited = fingerprint(&mut d);

    // A schema change on a body node, then undo all the way back.
    d.set_record_get_data(&[mp], g, "Box3".to_string(), specs(&["p.x", "size"]))
        .unwrap();
    let b2 = body(&d);
    assert!(wires_in(&b2, e, 0).is_empty(), "size dangles on Box3");
    assert!(d.undo());
    assert_eq!(fingerprint(&mut d), edited);
    assert!(d.undo());
    assert!(d.undo());
    assert_eq!(fingerprint(&mut d), original);
}
