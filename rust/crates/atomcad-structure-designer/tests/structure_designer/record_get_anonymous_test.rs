//! Anonymous `record_get` / `record_set` (`doc/design_anonymous_record_readers.md`,
//! Phase 3): the root of the path rows is the node's mode — a def, the
//! anonymous type it took from its wire, or `{}` when unset — and the type is
//! adopted on drag-place, on connect while nothing depends on the node, and by
//! *Take type from wire*. The path core on an anonymous root is unit-tested in
//! `record_path_rows_test.rs`, the path-picker tree in the api harness
//! (`record_get_set_api_test.rs`), the round-trip corpus in
//! `text_format_roundtrip_corpus_test.rs`.

use crate::undo_test::assert_undo_redo_roundtrip;
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::invariants::check_document_invariants;
use atomcad_structure_designer::node_data::DragDirection;
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type_registry::{RecordFieldEdit, RecordTypeDef};
use atomcad_structure_designer::nodes::record_get::RecordGetData;
use atomcad_structure_designer::nodes::record_set::RecordSetData;
use atomcad_structure_designer::record_input_type_ops::{
    CONNECT_WIRE_DESCRIPTION, SET_RECORD_INPUT_SCHEMA_DESCRIPTION, is_uncommitted_record_reader,
};
use atomcad_structure_designer::record_path::PathSpec;
use atomcad_structure_designer::structure_designer::{DragSource, StructureDesigner};
use atomcad_structure_designer::text_format::{TextValue, serialize_network};
use glam::f64::DVec2;

const NET: &str = "Main";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

/// A designer whose active network `Main` holds `code`, with the defs
/// `Point { x, y: Float }` and `Pair { p: Point, s: Float }`, validated.
fn setup(code: &str) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Point",
        vec![
            ("x".to_string(), DataType::Float),
            ("y".to_string(), DataType::Float),
        ],
    ))
    .unwrap();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Pair",
        vec![
            ("p".to_string(), named("Point")),
            ("s".to_string(), DataType::Float),
        ],
    ))
    .unwrap();
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

/// `pt` is the `Point` `(1.5, 2.5)`; `rc` the anonymous record
/// `{p: pt, s: 4.0}`, built `p, s`; `f9` a spare `9.0`.
const PS: &str = "fx = float { value: 1.5 }
fy = float { value: 2.5 }
fs = float { value: 4.0 }
f9 = float { value: 9.0 }
pt = record_construct { schema: \"Point\", x: fx, y: fy }
rc = record_construct { fields: [{ name: \"p\", data_type: Record(Point) }, { name: \"s\", data_type: Float }], p: pt, s: fs }
";

/// `rc`'s type: `{p: Point, s: Float}`.
fn ps_type() -> RecordType {
    RecordType::anonymous(vec![
        ("p".to_string(), named("Point")),
        ("s".to_string(), DataType::Float),
    ])
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

fn get_data(d: &StructureDesigner, node_id: u64) -> RecordGetData {
    network(d).nodes[&node_id]
        .data
        .as_any_ref()
        .downcast_ref::<RecordGetData>()
        .expect("a record_get")
        .clone()
}

fn set_data(d: &StructureDesigner, node_id: u64) -> RecordSetData {
    network(d).nodes[&node_id]
        .data
        .as_any_ref()
        .downcast_ref::<RecordSetData>()
        .expect("a record_set")
        .clone()
}

fn field_names(
    fields: &[atomcad_structure_designer::nodes::local_record_fields::LocalRecordField],
) -> Vec<&str> {
    fields.iter().map(|f| f.name.as_str()).collect()
}

/// The output pin names, in pin order.
fn out_pins(d: &StructureDesigner, node_id: u64) -> Vec<String> {
    let node = &network(d).nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .output_pins
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

/// The input pins `(name, type)`, in pin order.
fn in_pins(d: &StructureDesigner, node_id: u64) -> Vec<(String, DataType)> {
    let node = &network(d).nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .parameters
        .iter()
        .map(|p| (p.name.clone(), p.data_type.clone()))
        .collect()
}

fn output_type(d: &StructureDesigner, node_id: u64) -> DataType {
    let node = &network(d).nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .output_pins[0]
        .fixed_type()
        .cloned()
        .expect("a fixed output type")
}

/// The output pin of `reader` the single wire into `consumer`'s pin `index`
/// reads, by name; `None` when the pin has no wire.
fn fed_by(d: &StructureDesigner, consumer: u64, index: usize, reader: u64) -> Option<String> {
    let wire = network(d).nodes[&consumer]
        .arguments
        .get(index)?
        .incoming_wires
        .first()?
        .clone();
    assert_eq!(wire.source_node_id, reader);
    let SourcePin::NodeOutput { pin_index } = wire.source_pin else {
        panic!("not a node output");
    };
    Some(out_pins(d, reader)[pin_index as usize].clone())
}

fn eval_pin(d: &StructureDesigner, node_id: u64, pin: i32) -> NetworkResult {
    let registry = &d.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let network_stack = vec![NetworkStackElement::root(network)];
    NetworkEvaluator::new().evaluate(
        &network_stack,
        node_id,
        pin,
        registry,
        false,
        &mut NetworkEvaluationContext::new(),
    )
}

fn float(result: &NetworkResult) -> f64 {
    match result {
        NetworkResult::Float(v) => *v,
        other => panic!("not a Float: {:?}", other.to_display_string()),
    }
}

fn warnings_on(d: &StructureDesigner, node_id: u64) -> Vec<String> {
    network(d)
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(node_id))
        .map(|e| e.error_text.clone())
        .collect()
}

/// Every op test ends here: a bad `next_field_id` or a wire left on a missing
/// pin fails where it was made.
fn assert_invariants(d: &StructureDesigner) {
    let violations = check_document_invariants(&d.node_type_registry);
    assert!(violations.is_empty(), "{:#?}", violations);
}

fn query(d: &StructureDesigner) -> String {
    serialize_network(network(d), &d.node_type_registry, Some(NET))
}

fn drag_place(d: &mut StructureDesigner, node_type: &str, source_type: DataType) -> u64 {
    d.add_node_with_drag_source(
        node_type,
        DVec2::new(600.0, 0.0),
        Some(DragSource {
            source_type,
            direction: DragDirection::FromOutput,
        }),
    )
}

fn live(names: &[&str]) -> PathSpec {
    PathSpec::Live(names.iter().map(|n| n.to_string()).collect())
}

/// Sets the paths of the `record_get` `g`, keeping its root (the panel's
/// row edit: it sends the current schema back).
fn set_get_paths(d: &mut StructureDesigner, g: u64, paths: &[&[&str]]) {
    let schema = get_data(d, g).schema;
    d.set_record_get_data(&[], g, schema, paths.iter().map(|p| live(p)).collect())
        .unwrap();
}

fn set_set_paths(d: &mut StructureDesigner, s: u64, paths: &[&[&str]]) {
    let schema = set_data(d, s).schema;
    d.set_record_set_data(&[], s, schema, paths.iter().map(|p| live(p)).collect())
        .unwrap();
}

/// Renames `Point.x` (keeping its id).
fn rename_point_x(d: &mut StructureDesigner, new_name: &str) {
    let edits = d
        .node_type_registry
        .lookup_record_type_def("Point")
        .unwrap()
        .fields
        .clone()
        .into_iter()
        .map(|f| RecordFieldEdit {
            id: Some(f.id),
            name: if f.name == "x" {
                new_name.to_string()
            } else {
                f.name
            },
            data_type: f.data_type,
            hint: None,
        })
        .collect();
    d.update_record_type_def_with_ids("Point", edits).unwrap();
    d.validate_active_network();
}

/// Rebuilds `rc` with the fields `fields` (each `Float`, except `p` and `q`,
/// which are `Point`s).
fn set_rc_fields(d: &mut StructureDesigner, fields: &[&str]) {
    let rc = id(d, "rc");
    d.set_record_construct_fields(
        &[],
        rc,
        fields
            .iter()
            .map(|n| {
                let t = if *n == "p" || *n == "q" {
                    named("Point")
                } else {
                    DataType::Float
                };
                (n.to_string(), t)
            })
            .collect(),
    )
    .unwrap();
    d.validate_active_network();
}

const FLOAT_CONSUMER: &str = "parameters: [{ name: \"x\", data_type: Float }], expression: \"x\"";

// ---------------------------------------------------------------------------
// Modes and evaluation
// ---------------------------------------------------------------------------

#[test]
fn an_unset_reader_has_the_empty_record_as_its_input() {
    let d = setup("g = record_get {}\ns = record_set {}\n");
    let (g, s) = (id(&d, "g"), id(&d, "s"));
    let empty = DataType::Record(RecordType::anonymous(vec![]));
    assert_eq!(in_pins(&d, g), vec![("record".to_string(), empty.clone())]);
    assert_eq!(out_pins(&d, g), vec!["result"]);
    assert_eq!(in_pins(&d, s), vec![("record".to_string(), empty.clone())]);
    assert_eq!(output_type(&d, s), empty);
    assert!(warnings_on(&d, g).is_empty());
    assert!(warnings_on(&d, s).is_empty());
}

#[test]
fn record_get_reads_a_path_on_an_anonymous_root() {
    let mut d = setup(&format!("{PS}g = record_get {{ record: rc }}\n"));
    let g = id(&d, "g");
    assert_eq!(get_data(&d, g).schema, "");
    assert_eq!(field_names(&get_data(&d, g).fields), vec!["p", "s"]);
    assert_eq!(
        in_pins(&d, g)[0].1,
        DataType::Record(ps_type()),
        "the input is the root"
    );
    set_get_paths(&mut d, g, &[&["p", "x"], &["s"]]);
    assert_eq!(out_pins(&d, g), vec!["p_x", "s"]);
    assert_eq!(float(&eval_pin(&d, g, 0)), 1.5);
    assert_eq!(float(&eval_pin(&d, g, 1)), 4.0);
    assert_invariants(&d);
}

#[test]
fn record_set_replaces_only_its_field_on_an_anonymous_root() {
    let mut d = setup(&format!("{PS}s = record_set {{ record: rc }}\n"));
    let s = id(&d, "s");
    set_set_paths(&mut d, s, &[&["s"]]);
    edit(&mut d, "s = record_set { s: f9 }\n");
    assert_eq!(output_type(&d, s), DataType::Record(ps_type()));
    let out = eval_pin(&d, s, 0);
    assert_eq!(float(out.extract_record_field("s").unwrap()), 9.0);
    let p = out.extract_record_field("p").unwrap();
    assert_eq!(
        float(p.extract_record_field("x").unwrap()),
        1.5,
        "p untouched"
    );
    assert_invariants(&d);
}

// ---------------------------------------------------------------------------
// The three triggers
// ---------------------------------------------------------------------------

#[test]
fn drag_place_takes_the_type_of_the_source() {
    for node_type in ["record_get", "record_set"] {
        let mut d = setup(PS);
        let r = drag_place(&mut d, node_type, DataType::Record(ps_type()));
        assert_eq!(
            in_pins(&d, r)[0].1,
            DataType::Record(ps_type()),
            "{}",
            node_type
        );
        let r = drag_place(&mut d, node_type, named("Pair"));
        assert_eq!(
            in_pins(&d, r)[0].1,
            named("Pair"),
            "{}: named mode",
            node_type
        );
        assert_invariants(&d);
    }
    // Only `record_set` outputs a record, so only it adapts to an input drag.
    let mut d = setup(PS);
    let s = d.add_node_with_drag_source(
        "record_set",
        DVec2::new(600.0, 0.0),
        Some(DragSource {
            source_type: DataType::Record(ps_type()),
            direction: DragDirection::FromInput,
        }),
    );
    assert_eq!(output_type(&d, s), DataType::Record(ps_type()));
}

/// A reader whose outputs nothing uses is repointed by the next record wired
/// into it, and one undo step takes back the wire, the rows' resolution and
/// the pins together.
#[test]
fn connecting_into_an_uncommitted_reader_adopts_in_one_undo_step() {
    for node_type in ["record_get", "record_set"] {
        let mut d = setup(&format!(
            "{PS}ab = expr {{ expression: \"{{a: 1.0, b: 2.0}}\", parameters: [] }}
r = {node_type} {{ fields: [{{ name: \"a\", data_type: Float }}, {{ name: \"b\", data_type: Float }}], paths: [\"a\"], record: ab }}
"
        ));
        let (rc, r) = (id(&d, "rc"), id(&d, "r"));
        let row_pin = |d: &StructureDesigner| -> Vec<String> {
            if node_type == "record_get" {
                out_pins(d, r)
            } else {
                in_pins(d, r)[1..].iter().map(|(n, _)| n.clone()).collect()
            }
        };
        assert_eq!(row_pin(&d), vec!["a"], "{}", node_type);
        assert_undo_redo_roundtrip(&mut d, |d| d.connect_nodes(rc, 0, r, 0));
        assert_eq!(
            in_pins(&d, r)[0].1,
            DataType::Record(RecordType::anonymous(vec![
                ("a".to_string(), DataType::Float),
                ("b".to_string(), DataType::Float),
            ]))
        );
        assert!(d.redo());
        assert_eq!(
            d.undo_stack.undo_description(),
            Some(CONNECT_WIRE_DESCRIPTION)
        );
        assert_eq!(
            in_pins(&d, r)[0].1,
            DataType::Record(ps_type()),
            "{}",
            node_type
        );
        assert!(
            row_pin(&d).is_empty() || row_pin(&d) == vec!["result"],
            "{}: the row `a` dangles in the new root: {:?}",
            node_type,
            row_pin(&d)
        );
        assert!(
            warnings_on(&d, r)
                .iter()
                .any(|w| w.contains("path `a` no longer exists in `{p: Record(Point), s: Float}`")),
            "{}: {:?}",
            node_type,
            warnings_on(&d, r)
        );
        assert_invariants(&d);
    }
}

#[test]
fn connecting_into_an_unset_reader_adopts_and_undo_unsets_it() {
    let mut d = setup(&format!("{PS}g = record_get {{}}\n"));
    let (rc, g) = (id(&d, "rc"), id(&d, "g"));
    assert_undo_redo_roundtrip(&mut d, |d| d.connect_nodes(rc, 0, g, 0));
    assert!(get_data(&d, g).fields.is_empty(), "undo unset it again");
    assert!(d.redo());
    assert_eq!(field_names(&get_data(&d, g).fields), vec!["p", "s"]);
    assert_invariants(&d);
}

#[test]
fn take_type_from_wire_adds_a_field_and_keeps_rows_wires_and_literals() {
    let mut d = setup(&format!(
        "{PS}g = record_get {{ record: rc }}
s = record_set {{ record: rc }}
"
    ));
    let (g, s) = (id(&d, "g"), id(&d, "s"));
    set_get_paths(&mut d, g, &[&["p", "x"]]);
    set_set_paths(&mut d, s, &[&["s"]]);
    edit(
        &mut d,
        &format!("s = record_set {{ s: 7.0 }}\nc = expr {{ {FLOAT_CONSUMER}, x: g.p_x }}\n"),
    );
    let c = id(&d, "c");
    assert_eq!(set_data(&d, s).rows[0].literal, Some(TextValue::Float(7.0)));

    set_rc_fields(&mut d, &["p", "s", "t"]);
    assert!(
        d.wire_type_offers(&[], g)[0].is_some(),
        "a wider source is stale"
    );
    assert!(d.wire_type_offers(&[], s)[0].is_some());
    assert_undo_redo_roundtrip(&mut d, |d| d.take_types_from_wires(&[], g, &[0]).unwrap());
    assert!(d.redo());
    d.take_types_from_wires(&[], s, &[0]).unwrap();

    assert_eq!(field_names(&get_data(&d, g).fields), vec!["p", "s", "t"]);
    assert_eq!(
        fed_by(&d, c, 0, g).as_deref(),
        Some("p_x"),
        "the row's wire stays"
    );
    assert_eq!(field_names(&set_data(&d, s).fields), vec!["p", "s", "t"]);
    assert_eq!(
        set_data(&d, s).rows[0].literal,
        Some(TextValue::Float(7.0)),
        "the literal survives a re-adoption that keeps its row"
    );
    assert_eq!(
        output_type(&d, s),
        in_pins(&d, s)[0].1,
        "the output follows"
    );
    assert_eq!(d.wire_type_offers(&[], g), vec![None]);
    assert_invariants(&d);
}

// ---------------------------------------------------------------------------
// Commitment
// ---------------------------------------------------------------------------

#[test]
fn a_record_set_with_a_wired_row_is_committed_and_one_with_literals_is_not() {
    let mut d = setup(&format!("{PS}s = record_set {{ record: rc }}\n"));
    let s = id(&d, "s");
    set_set_paths(&mut d, s, &[&["s"]]);
    edit(&mut d, "s = record_set { s: 2.0 }\n");
    assert!(
        is_uncommitted_record_reader(network(&d), None, s),
        "a literal only"
    );
    edit(&mut d, "s = record_set { s: f9 }\n");
    assert!(
        !is_uncommitted_record_reader(network(&d), None, s),
        "a wired row"
    );
}

#[test]
fn a_record_get_used_through_its_function_pin_is_committed() {
    let mut d = setup(
        "ab = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
bc = expr { expression: \"{b: 1.0, c: 2.0}\", parameters: [] }
g = record_get { paths: [\"a\"], fields: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Float }], record: ab }
rg = range { count: 2 }
mp = map { xs: rg, input_type: Int, output_type: Int, body { k = int { value: 1 } output k } }
",
    );
    let (bc, g, mp) = (id(&d, "bc"), id(&d, "g"), id(&d, "mp"));
    assert!(d.can_connect_nodes(bc, 0, g, 0), "uncommitted: any record");
    let f_index = in_pins(&d, mp).iter().position(|(n, _)| n == "f").unwrap();
    d.connect_nodes(g, -1, mp, f_index);
    assert!(
        !network(&d).nodes[&mp].arguments[f_index]
            .incoming_wires
            .is_empty()
    );
    assert!(!is_uncommitted_record_reader(network(&d), None, g));
    assert!(!d.can_connect_nodes(bc, 0, g, 0), "committed: refused");
}

// ---------------------------------------------------------------------------
// Row identity on an anonymous root
// ---------------------------------------------------------------------------

/// Below the root the path is by `FieldId` inside `Point`, so a rename there
/// keeps the row; the root field itself is by name, so renaming it upstream
/// and re-adopting leaves a broken row.
#[test]
fn a_row_follows_a_def_rename_below_the_root_but_not_a_root_rename() {
    let mut d = setup(&format!("{PS}g = record_get {{ record: rc }}\n"));
    let g = id(&d, "g");
    set_get_paths(&mut d, g, &[&["p", "x"]]);
    edit(
        &mut d,
        &format!("c = expr {{ {FLOAT_CONSUMER}, x: g.p_x }}\n"),
    );
    let c = id(&d, "c");
    let row = &get_data(&d, g).rows[0];
    assert_eq!(row.path[0].field_id, None, "the root segment is by name");
    assert!(row.path[1].field_id.is_some(), "a field of Point is by id");

    rename_point_x(&mut d, "xx");
    assert_eq!(out_pins(&d, g), vec!["p_xx"]);
    assert_eq!(
        fed_by(&d, c, 0, g).as_deref(),
        Some("p_xx"),
        "the wire follows"
    );
    assert_eq!(float(&eval_pin(&d, g, 0)), 1.5);

    set_rc_fields(&mut d, &["q", "s"]);
    d.take_types_from_wires(&[], g, &[0]).unwrap();
    d.validate_active_network();
    assert_eq!(out_pins(&d, g), vec!["result"], "the row lost its pin");
    assert_eq!(get_data(&d, g).rows.len(), 1, "but is kept");
    assert_eq!(fed_by(&d, c, 0, g), None);
    assert_eq!(
        warnings_on(&d, g),
        vec!["path `p.xx` no longer exists in `{q: Record(Point), s: Float}`".to_string()]
    );
    assert_invariants(&d);
}

/// `record_get`'s pins carry row ids, which a type change keeps, so its
/// output wires move by id even across a switch between modes — not by the
/// derived pin names, which shift when a row above goes dangling.
#[test]
fn record_get_output_wires_follow_their_rows_when_a_pin_name_shifts() {
    let mut d = setup(
        "src = expr { expression: \"{a: {b: 1.0}, a_b: 2.0}\", parameters: [] }
g = record_get { record: src }
",
    );
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Nest",
        vec![(
            "a".to_string(),
            DataType::Record(RecordType::anonymous(vec![(
                "b".to_string(),
                DataType::Float,
            )])),
        )],
    ))
    .unwrap();
    let g = id(&d, "g");
    set_get_paths(&mut d, g, &[&["a_b"], &["a", "b"]]);
    assert_eq!(out_pins(&d, g), vec!["a_b", "a_b__2"]);
    edit(
        &mut d,
        &format!(
            "c = expr {{ {FLOAT_CONSUMER}, x: g.a_b__2 }}
"
        ),
    );
    let c = id(&d, "c");

    // Anonymous -> `Nest`, which has no `a_b`: the first row dangles and the
    // second takes its pin name.
    d.set_record_input_schema(&[], g, "Nest").unwrap();
    assert_eq!(out_pins(&d, g), vec!["a_b"], "the second row took the name");
    assert_eq!(fed_by(&d, c, 0, g).as_deref(), Some("a_b"));
    assert_eq!(
        get_data(&d, g).rows[1].id,
        Some(
            network(&d).nodes[&g]
                .custom_node_type
                .as_ref()
                .unwrap()
                .output_pins[0]
                .id
                .unwrap()
        ),
        "and its wire went with it, by row id"
    );
    assert_invariants(&d);
}

// ---------------------------------------------------------------------------
// The schema dropdown
// ---------------------------------------------------------------------------

#[test]
fn the_dropdown_switches_modes_keeping_rows_and_wires() {
    let mut d = setup(&format!("{PS}g = record_get {{ record: rc }}\n"));
    let g = id(&d, "g");
    set_get_paths(&mut d, g, &[&["p", "x"], &["s"]]);
    edit(
        &mut d,
        &format!("c = expr {{ {FLOAT_CONSUMER}, x: g.s }}\n"),
    );
    let c = id(&d, "c");

    // Anonymous {p, s} -> Pair { p, s }: the rows resolve by name.
    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_input_schema(&[], g, "Pair").unwrap()
    });
    assert!(d.redo());
    assert_eq!(
        d.undo_stack.undo_description(),
        Some(SET_RECORD_INPUT_SCHEMA_DESCRIPTION)
    );
    let data = get_data(&d, g);
    assert_eq!(data.schema, "Pair");
    assert!(data.fields.is_empty());
    assert!(data.rows[0].path[0].field_id.is_some(), "now by FieldId");
    assert_eq!(out_pins(&d, g), vec!["p_x", "s"]);
    assert_eq!(fed_by(&d, c, 0, g).as_deref(), Some("s"));

    // None: unset, every row broken, every output wire gone; undo restores.
    let before = query(&d);
    d.set_record_input_schema(&[], g, "").unwrap();
    assert_eq!(out_pins(&d, g), vec!["result"]);
    assert_eq!(get_data(&d, g).rows.len(), 2, "the rows are kept, broken");
    assert_eq!(fed_by(&d, c, 0, g), None);
    assert!(d.undo());
    assert_eq!(query(&d), before);
    assert_eq!(fed_by(&d, c, 0, g).as_deref(), Some("s"));
    assert_invariants(&d);
}

// ---------------------------------------------------------------------------
// Text format
// ---------------------------------------------------------------------------

#[test]
fn query_then_replace_is_a_no_op_for_anonymous_and_unset_readers() {
    let mut d = setup(&format!(
        "{PS}g = record_get {{ record: rc }}
s = record_set {{ record: rc }}
w = record_set {{ record: rc }}
u = record_get {{ schema: \"\", record: rc }}
"
    ));
    let (g, s, w) = (id(&d, "g"), id(&d, "s"), id(&d, "w"));
    set_get_paths(&mut d, g, &[&["p", "x"], &["s"]]);
    set_set_paths(&mut d, s, &[&["s"]]);
    set_set_paths(&mut d, w, &[&["p", "y"]]);
    edit(
        &mut d,
        &format!(
            "s = record_set {{ s: 3.0 }}\nw = record_set {{ p_y: f9 }}\nc = expr {{ {FLOAT_CONSUMER}, x: g.p_x }}\n"
        ),
    );
    let text = query(&d);
    assert!(text.contains("fields: ["), "{}", text);
    let outcome = d.ai_text_edit(&text, true);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    assert!(
        outcome.result.warnings.is_empty(),
        "{:?}",
        outcome.result.warnings
    );
    assert_eq!(query(&d), text);
    assert!(
        get_data(&d, id(&d, "u")).fields.is_empty(),
        "unset stays unset"
    );
}

#[test]
fn a_record_wire_declares_the_root_of_a_statement_without_paths() {
    let mut d = setup(PS);
    let outcome = d.ai_text_edit(
        "g = record_get { record: rc }\ns = record_set { record: rc }\n",
        false,
    );
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    assert_eq!(
        field_names(&get_data(&d, id(&d, "g")).fields),
        vec!["p", "s"]
    );
    assert_eq!(
        field_names(&set_data(&d, id(&d, "s")).fields),
        vec!["p", "s"]
    );
}

#[test]
fn paths_without_a_root_are_an_error_naming_both_properties() {
    let mut d = setup(PS);
    let outcome = d.ai_text_edit("g = record_get { paths: [\"s\"], record: rc }\n", false);
    let errors = format!("{:?}", outcome.result.errors);
    assert!(
        errors.contains("`schema`") && errors.contains("`fields`"),
        "{}",
        errors
    );
    // With its root stated, the same paths resolve.
    edit(
        &mut d,
        "h = record_get { fields: [{ name: \"s\", data_type: Float }], paths: [\"s\"], record: rc }\n",
    );
    assert_eq!(out_pins(&d, id(&d, "h")), vec!["s"]);
}

#[test]
fn text_rejects_fields_on_a_named_reader() {
    let mut d = setup(PS);
    let outcome = d.ai_text_edit(
        "g = record_get { schema: \"Pair\", fields: [{ name: \"s\", data_type: Float }] }\n",
        false,
    );
    assert!(!outcome.result.errors.is_empty());
}
