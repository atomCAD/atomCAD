//! Anonymous `record_destructure` (`doc/design_anonymous_record_readers.md`,
//! Phase 1): the three modes, adopting the record type of a wire on
//! drag-place, on connect while nothing depends on the reader and by *Take
//! type from wire*, the output-wire remap across a type change, the schema
//! dropdown as one undo step, and the text format. The drag popup is in the
//! api harness (`drag_adapter_test.rs`), the cross-document paste in
//! `clipboard_test.rs`, the round-trip corpus in
//! `text_format_roundtrip_corpus_test.rs`.

use super::library_links_support::{edit as edit_network, new_design, open, save};
use crate::undo_test::assert_undo_redo_roundtrip;
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::invariants::{
    InvariantKind, check_document_invariants, check_network_invariants,
};
use atomcad_structure_designer::library_links::is_frozen;
use atomcad_structure_designer::node_data::{DragDirection, NodeData};
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type_registry::{
    FieldId, RecordFieldEdit, RecordTypeDef, collect_record_refs_in_node,
};
use atomcad_structure_designer::nodes::local_record_fields::{
    ANON_FIELD_PIN_ID_BASE, LocalRecordField,
};
use atomcad_structure_designer::nodes::record_destructure::RecordDestructureData;
use atomcad_structure_designer::nodes::value::add_typed_value_node;
use atomcad_structure_designer::record_input_type_ops::{
    CONNECT_WIRE_DESCRIPTION, SET_RECORD_INPUT_SCHEMA_DESCRIPTION, is_uncommitted_record_reader,
};
use atomcad_structure_designer::structure_designer::{DragSource, StructureDesigner};
use atomcad_structure_designer::text_format::serialize_network;
use glam::f64::{DVec2, DVec3};

const NET: &str = "Main";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn def(name: &str, fields: &[&str]) -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        name,
        fields
            .iter()
            .map(|f| (f.to_string(), DataType::Float))
            .collect(),
    )
}

/// A designer whose active network `Main` holds `code`, with the defs
/// `Box { a, b }` and `Foo { b, c }` (all `Float`), validated.
fn setup(code: &str) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    d.add_record_type_def(def("Box", &["a", "b"])).unwrap();
    d.add_record_type_def(def("Foo", &["b", "c"])).unwrap();
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

fn scope_net<'a>(d: &'a StructureDesigner, scope: &[u64]) -> &'a NodeNetwork {
    d.get_scope_network(scope).unwrap()
}

fn network(d: &StructureDesigner) -> &NodeNetwork {
    scope_net(d, &[])
}

fn id_in(d: &StructureDesigner, scope: &[u64], name: &str) -> u64 {
    scope_net(d, scope)
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node '{}'", name))
        .id
}

fn id(d: &StructureDesigner, name: &str) -> u64 {
    id_in(d, &[], name)
}

fn reader_in(d: &StructureDesigner, scope: &[u64], node_id: u64) -> RecordDestructureData {
    scope_net(d, scope).nodes[&node_id]
        .data
        .as_any_ref()
        .downcast_ref::<RecordDestructureData>()
        .expect("a record_destructure")
        .clone()
}

fn reader(d: &StructureDesigner, node_id: u64) -> RecordDestructureData {
    reader_in(d, &[], node_id)
}

/// The output pin names, in pin order.
fn pins_in(d: &StructureDesigner, scope: &[u64], node_id: u64) -> Vec<String> {
    let node = &scope_net(d, scope).nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .output_pins
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

fn pins(d: &StructureDesigner, node_id: u64) -> Vec<String> {
    pins_in(d, &[], node_id)
}

fn input_type(d: &StructureDesigner, node_id: u64) -> DataType {
    let node = &network(d).nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .parameters[0]
        .data_type
        .clone()
}

/// `(source node, source pin)` of every wire into each input pin.
fn wires_into_in(d: &StructureDesigner, scope: &[u64], node_id: u64) -> Vec<Vec<(u64, i32)>> {
    scope_net(d, scope).nodes[&node_id]
        .arguments
        .iter()
        .map(|arg| {
            arg.incoming_wires
                .iter()
                .filter_map(|w| match w.source_pin {
                    SourcePin::NodeOutput { pin_index } => Some((w.source_node_id, pin_index)),
                    SourcePin::ZoneInput { .. } => None,
                })
                .collect()
        })
        .collect()
}

fn wires_into(d: &StructureDesigner, node_id: u64) -> Vec<Vec<(u64, i32)>> {
    wires_into_in(d, &[], node_id)
}

/// The output pin of `reader` the single wire into `consumer`'s pin 0 reads,
/// by name; `None` when the pin has no wire.
fn fed_by(d: &StructureDesigner, consumer: u64, reader: u64) -> Option<String> {
    let wires = wires_into(d, consumer);
    let (source, pin) = *wires.first()?.first()?;
    assert_eq!(source, reader);
    Some(pins(d, reader)[pin as usize].clone())
}

fn anon(fields: &[(&str, DataType)]) -> RecordType {
    RecordType::anonymous(
        fields
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect(),
    )
}

fn floats(names: &[&str]) -> RecordType {
    anon(
        &names
            .iter()
            .map(|n| (*n, DataType::Float))
            .collect::<Vec<_>>(),
    )
}

fn named(name: &str) -> RecordType {
    RecordType::Named(name.to_string())
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

fn errors_on(d: &StructureDesigner, node_id: u64) -> Vec<String> {
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

/// The network as saved, for "nothing changed" checks.
fn fingerprint(d: &mut StructureDesigner) -> String {
    serde_json::to_string(&d.snapshot_network(NET).unwrap()).unwrap()
}

fn query(d: &StructureDesigner) -> String {
    serialize_network(network(d), &d.node_type_registry, Some(NET))
}

/// `e` outputs `{a: 1.0, b: 2.0}`, `d` is an unset reader.
const AB_AND_UNSET: &str = "e = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
d = record_destructure {}
";

/// `e` (`{a, b}`) adopted by `d`, whose `a` and `b` feed `ca` and `cb`.
fn committed_ab() -> StructureDesigner {
    let mut d = setup(AB_AND_UNSET);
    let (e, r) = (id(&d, "e"), id(&d, "d"));
    d.connect_nodes(e, 0, r, 0);
    edit(
        &mut d,
        "ca = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.a }
cb = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.b }
",
    );
    d
}

// ---------------------------------------------------------------------------
// The unset mode
// ---------------------------------------------------------------------------

#[test]
fn an_unset_reader_takes_any_record_and_nothing_else() {
    let mut d = setup(
        "x = float { value: 1.0 }
bx = record_construct { schema: \"Box\", a: x, b: x }
an = expr { expression: \"{p: 1.0}\", parameters: [] }
d = record_destructure {}
",
    );
    let arr = add_typed_value_node(
        d.get_scope_network_mut(&[]).unwrap(),
        DVec2::new(0.0, 400.0),
        NetworkResult::Array(vec![]),
        DataType::Array(Box::new(DataType::Record(named("Box")))),
    );
    let dangling = add_typed_value_node(
        d.get_scope_network_mut(&[]).unwrap(),
        DVec2::new(0.0, 500.0),
        NetworkResult::None,
        DataType::Record(named("Nope")),
    );
    let r = id(&d, "d");
    assert_eq!(
        input_type(&d, r),
        DataType::Record(RecordType::anonymous(vec![]))
    );
    assert!(d.can_connect_nodes(id(&d, "bx"), 0, r, 0), "a named record");
    assert!(
        d.can_connect_nodes(id(&d, "an"), 0, r, 0),
        "an anonymous record"
    );
    assert!(!d.can_connect_nodes(id(&d, "x"), 0, r, 0), "a Float");
    assert!(!d.can_connect_nodes(arr, 0, r, 0), "an array of records");
    assert!(!d.can_connect_nodes(dangling, 0, r, 0), "a dangling name");
}

/// A file saved before this change holds an unset reader as `"schema": ""`
/// with no wire. It loads with the `{}` input and no validation error, and
/// the data is written back the same way.
#[test]
fn a_legacy_unset_reader_loads_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.cnnd");
    {
        let mut d = new_design(&path);
        edit_network(&mut d, NET, "d = record_destructure {}\n");
        save(&mut d);
    }
    let text = std::fs::read_to_string(&path).unwrap();
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    let node = file["node_networks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n[0] == NET)
        .unwrap()[1]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["node_type_name"] == "record_destructure")
        .unwrap()
        .clone();
    assert_eq!(node["data"], serde_json::json!({ "schema": "" }));

    let mut d = open(&path);
    d.set_active_node_network_name(Some(NET.to_string()));
    d.validate_active_network();
    let r = id(&d, "d");
    assert_eq!(
        input_type(&d, r),
        DataType::Record(RecordType::anonymous(vec![]))
    );
    assert!(network(&d).validation_errors.is_empty());
    assert_invariants(&d);
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

#[test]
fn an_anonymous_destructure_outputs_each_field() {
    let mut d = setup(
        "e = expr { expression: \"{a: 1, b: vec3(1.0, 2.0, 3.0)}\", parameters: [] }
d = record_destructure {}
",
    );
    let (e, r) = (id(&d, "e"), id(&d, "d"));
    d.connect_nodes(e, 0, r, 0);
    assert_eq!(pins(&d, r), vec!["a", "b"]);
    assert!(matches!(eval_pin(&d, r, 0), NetworkResult::Int(1)));
    assert!(matches!(
        eval_pin(&d, r, 1),
        NetworkResult::Vec3(v) if v == DVec3::new(1.0, 2.0, 3.0)
    ));
    assert_invariants(&d);
}

#[test]
fn an_unset_optional_field_outputs_none() {
    let mut d = setup("x = float { value: 2.0 }\n");
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Opt",
        vec![
            ("a".to_string(), DataType::Float),
            (
                "o".to_string(),
                DataType::Optional(Box::new(DataType::Float)),
            ),
        ],
    ))
    .unwrap();
    edit(
        &mut d,
        "oc = record_construct { schema: \"Opt\", a: x }
d = record_destructure { record: oc }
",
    );
    let r = id(&d, "d");
    assert_eq!(reader(&d, r).schema, "Opt", "the wire declared the type");
    assert!(matches!(eval_pin(&d, r, 0), NetworkResult::Float(v) if v == 2.0));
    assert!(matches!(eval_pin(&d, r, 1), NetworkResult::None));
}

#[test]
fn an_unset_reader_with_a_record_wired_evaluates_to_its_placeholder() {
    let d = setup(
        "e = expr { expression: \"{a: 1.0}\", parameters: [] }
d = record_destructure { schema: \"\", record: e }
",
    );
    let r = id(&d, "d");
    assert_eq!(
        reader(&d, r).fields,
        vec![],
        "an explicit schema keeps it unset"
    );
    assert_eq!(wires_into(&d, r), vec![vec![(id(&d, "e"), 0)]]);
    assert!(matches!(eval_pin(&d, r, 0), NetworkResult::None));
    assert!(errors_on(&d, r).is_empty(), "{:?}", errors_on(&d, r));
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

#[test]
fn an_anonymous_reader_survives_save_and_load() {
    let mut d = committed_ab();
    let (r, ca, cb) = (id(&d, "d"), id(&d, "ca"), id(&d, "cb"));
    let data = serde_json::to_value(reader(&d, r)).unwrap();
    assert!(data.get("fields").is_some() && data.get("next_field_id").is_some());
    let wires = (wires_into(&d, r), wires_into(&d, ca), wires_into(&d, cb));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("anon.cnnd");
    d.save_node_networks_as(&path.to_string_lossy()).unwrap();
    let mut d2 = open(&path);
    d2.set_active_node_network_name(Some(NET.to_string()));
    d2.validate_active_network();
    let (r2, ca2, cb2) = (id(&d2, "d"), id(&d2, "ca"), id(&d2, "cb"));
    assert_eq!(serde_json::to_value(reader(&d2, r2)).unwrap(), data);
    assert_eq!(
        (
            wires_into(&d2, r2),
            wires_into(&d2, ca2),
            wires_into(&d2, cb2)
        ),
        wires
    );
    assert_eq!(fed_by(&d2, cb2, r2).as_deref(), Some("b"));
    assert!(matches!(eval_pin(&d2, cb2, 0), NetworkResult::Float(v) if v == 2.0));
    assert_invariants(&d2);
}

/// `fields` and `next_field_id` are skipped when empty, so a named reader is
/// written exactly as before.
#[test]
fn a_named_reader_is_written_as_before() {
    let saved = serde_json::to_value(RecordDestructureData::named("Box")).unwrap();
    assert_eq!(saved, serde_json::json!({ "schema": "Box" }));
}

// ---------------------------------------------------------------------------
// Refused adoption
// ---------------------------------------------------------------------------

/// A source with a field the reader cannot hold (`Unit`): the connect still
/// makes its wire and records its ordinary step, the reader stays unset, and
/// Take type from wire reports the reason and changes nothing.
#[test]
fn a_field_the_reader_cannot_hold_refuses_the_adoption() {
    let mut d = setup("d = record_destructure {}\n");
    let src = add_typed_value_node(
        d.get_scope_network_mut(&[]).unwrap(),
        DVec2::new(0.0, 300.0),
        NetworkResult::None,
        DataType::Record(anon(&[("u", DataType::Unit)])),
    );
    let r = id(&d, "d");
    let pushes = d.undo_stack.push_count();
    d.connect_nodes(src, 0, r, 0);
    assert_eq!(d.undo_stack.push_count(), pushes + 1);
    assert_eq!(wires_into(&d, r), vec![vec![(src, 0)]], "the wire is made");
    assert_eq!(reader(&d, r).fields, vec![], "and the reader stays unset");

    let offers = d.wire_type_offers(&[], r);
    assert_eq!(
        offers,
        vec![Some(DataType::Record(anon(&[("u", DataType::Unit)])))],
        "the stale wire is offered"
    );
    let before = fingerprint(&mut d);
    let pushes = d.undo_stack.push_count();
    let refusal = d.take_types_from_wires(&[], r, &[0]).unwrap_err();
    assert!(refusal.contains("Unit"), "{}", refusal);
    assert_eq!(fingerprint(&mut d), before);
    assert_eq!(d.undo_stack.push_count(), pushes);

    assert!(d.undo());
    assert_eq!(wires_into(&d, r), vec![vec![]], "the connect's undo");
    assert_invariants(&d);
}

// ---------------------------------------------------------------------------
// Drag-place and field order
// ---------------------------------------------------------------------------

fn drag_place(d: &mut StructureDesigner, source_type: DataType) -> u64 {
    d.add_node_with_drag_source(
        "record_destructure",
        DVec2::new(400.0, 0.0),
        Some(DragSource {
            source_type,
            direction: DragDirection::FromOutput,
        }),
    )
}

#[test]
fn drag_place_takes_the_type_of_the_source() {
    let mut d = setup("x = float { value: 1.0 }\n");
    let r = drag_place(&mut d, DataType::Record(floats(&["b", "a"])));
    assert_eq!(pins(&d, r), vec!["a", "b"], "an anonymous source");
    assert_eq!(reader(&d, r).schema, "");
    let r = drag_place(&mut d, DataType::Record(named("Box")));
    assert_eq!(reader(&d, r).schema, "Box", "a named source: named mode");
    assert_eq!(pins(&d, r), vec!["a", "b"]);
    assert_invariants(&d);
}

/// An anonymous `record_construct` built `b, a` destructures as `b, a` —
/// through drag-place (once the drop's wire is connected), through connect
/// and through Take type from wire.
#[test]
fn an_anonymous_record_construct_source_keeps_its_authored_order() {
    let mut d = setup(
        "x = float { value: 1.0 }
rc = record_construct { fields: [{ name: \"b\", data_type: Float }, { name: \"a\", data_type: Float }], b: x, a: x }
u = record_destructure {}
",
    );
    let rc = id(&d, "rc");
    let ab = DataType::Record(floats(&["a", "b"]));

    let placed = drag_place(&mut d, ab.clone());
    assert_eq!(pins(&d, placed), vec!["a", "b"], "the type alone is sorted");
    d.connect_nodes(rc, 0, placed, 0);
    assert_eq!(pins(&d, placed), vec!["b", "a"], "drag-place");

    let u = id(&d, "u");
    d.connect_nodes(rc, 0, u, 0);
    assert_eq!(pins(&d, u), vec!["b", "a"], "connect");

    // Take type from wire: `u` is committed, and `rc` gains `c`.
    edit(
        &mut d,
        "cb = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: u.b }\n",
    );
    d.set_record_construct_fields(
        &[],
        rc,
        vec![
            ("b".to_string(), DataType::Float),
            ("a".to_string(), DataType::Float),
            ("c".to_string(), DataType::Float),
        ],
    )
    .unwrap();
    d.take_types_from_wires(&[], u, &[0]).unwrap();
    assert_eq!(pins(&d, u), vec!["b", "a", "c"], "take type from wire");
    assert_eq!(fed_by(&d, id(&d, "cb"), u).as_deref(), Some("b"));
    assert_invariants(&d);
}

// ---------------------------------------------------------------------------
// Connect
// ---------------------------------------------------------------------------

#[test]
fn connecting_into_an_unset_reader_adopts_in_one_undo_step() {
    let mut d = setup(AB_AND_UNSET);
    let (e, r) = (id(&d, "e"), id(&d, "d"));
    assert_undo_redo_roundtrip(&mut d, |d| d.connect_nodes(e, 0, r, 0));
    assert_eq!(pins(&d, r), vec!["result"], "undo removed the pins");
    assert!(d.redo());
    assert_eq!(pins(&d, r), vec!["a", "b"], "redo restored them");
    assert_eq!(wires_into(&d, r), vec![vec![(e, 0)]]);
    assert_eq!(
        d.undo_stack.undo_description(),
        Some(CONNECT_WIRE_DESCRIPTION)
    );
    let ids: Vec<u64> = reader(&d, r).fields.iter().map(|f| f.id).collect();
    let node = &network(&d).nodes[&r];
    let pin_ids: Vec<Option<u64>> = d
        .node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .output_pins
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(
        pin_ids,
        ids.iter()
            .map(|id| Some(ANON_FIELD_PIN_ID_BASE | id))
            .collect::<Vec<_>>()
    );
    assert_invariants(&d);
}

#[test]
fn connecting_inside_a_map_body_adopts_in_the_bodys_undo_step() {
    let mut d = setup(
        "rg = range { count: 2 }
mp = map { xs: rg, input_type: Int, output_type: Int, body {
  e = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
  d = record_destructure {}
  g = int { value: 1 }
  output g
} }
",
    );
    let mp = id(&d, "mp");
    let (e, r) = (id_in(&d, &[mp], "e"), id_in(&d, &[mp], "d"));
    assert_undo_redo_roundtrip(&mut d, |d| d.connect_nodes_scoped(&[mp], e, 0, r, 0));
    assert_eq!(pins_in(&d, &[mp], r), vec!["result"]);
    assert!(d.redo());
    assert_eq!(pins_in(&d, &[mp], r), vec!["a", "b"]);
    assert_eq!(wires_into_in(&d, &[mp], r), vec![vec![(e, 0)]]);
    assert_eq!(d.undo_stack.undo_description(), Some("Connect wire"));
    assert_invariants(&d);
}

/// Adoption grows the reader, and the neighbour below it moves out of the
/// way — in the same undo step as the connect.
#[test]
fn adopting_reflows_the_neighbour_below_in_the_same_step() {
    let mut d = setup(
        "e = expr { expression: \"{a: 1.0, b: 2.0, c: 3.0, d: 4.0, f: 5.0}\", parameters: [] }
r = record_destructure {}
n = int { value: 1 }
",
    );
    let (e, r, n) = (id(&d, "e"), id(&d, "r"), id(&d, "n"));
    {
        let net = d.get_scope_network_mut(&[]).unwrap();
        net.nodes.get_mut(&e).unwrap().position = DVec2::new(0.0, 0.0);
        net.nodes.get_mut(&r).unwrap().position = DVec2::new(300.0, 0.0);
        net.nodes.get_mut(&n).unwrap().position = DVec2::new(300.0, 90.0);
    }
    let before = network(&d).nodes[&n].position;
    assert_undo_redo_roundtrip(&mut d, |d| d.connect_nodes(e, 0, r, 0));
    assert_eq!(network(&d).nodes[&n].position, before, "undo moved it back");
    assert!(d.redo());
    assert_eq!(pins(&d, r).len(), 5);
    assert!(
        network(&d).nodes[&n].position.y > before.y,
        "the neighbour moved down: {:?}",
        network(&d).nodes[&n].position
    );
}

/// A reader whose function pin feeds `map.f` is used, though none of its
/// field pins is.
#[test]
fn a_reader_used_through_its_function_pin_is_committed() {
    let mut d = setup(
        "e = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
d = record_destructure {}
rg = range { count: 2 }
mp = map { xs: rg, input_type: Int, output_type: Int, body { g = int { value: 1 } output g } }
",
    );
    let (e, r, mp) = (id(&d, "e"), id(&d, "d"), id(&d, "mp"));
    d.connect_nodes(e, 0, r, 0);
    let f_index = d
        .node_type_registry
        .get_node_type_for_node(&network(&d).nodes[&mp])
        .unwrap()
        .parameters
        .iter()
        .position(|p| p.name == "f")
        .unwrap();
    d.connect_nodes(r, -1, mp, f_index);
    assert!(!is_uncommitted_record_reader(network(&d), None, r));
}

#[test]
fn an_uncommitted_reader_is_repurposed_and_a_committed_one_is_not() {
    let mut d = setup(
        "ab = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
bc = expr { expression: \"{b: 1.0, c: 2.0}\", parameters: [] }
abz = expr { expression: \"{a: 1.0, b: 2.0, z: 3.0}\", parameters: [] }
d = record_destructure {}
",
    );
    let (ab, bc, abz, r) = (id(&d, "ab"), id(&d, "bc"), id(&d, "abz"), id(&d, "d"));
    d.connect_nodes(ab, 0, r, 0);
    assert_eq!(pins(&d, r), vec!["a", "b"]);
    assert!(d.can_connect_nodes(bc, 0, r, 0), "uncommitted: any record");
    d.connect_nodes(bc, 0, r, 0);
    assert_eq!(pins(&d, r), vec!["b", "c"], "repurposed");

    d.connect_nodes(ab, 0, r, 0);
    edit(
        &mut d,
        "ca = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.a }\n",
    );
    let ca = id(&d, "ca");
    assert!(!d.can_connect_nodes(bc, 0, r, 0), "committed: refused");
    assert!(d.can_connect_nodes(abz, 0, r, 0), "a wider one converts");
    d.connect_nodes(abz, 0, r, 0);
    assert_eq!(pins(&d, r), vec!["a", "b"], "and is not adopted");
    assert_eq!(fed_by(&d, ca, r).as_deref(), Some("a"));
    assert_eq!(
        d.wire_type_offers(&[], r),
        vec![Some(DataType::Record(floats(&["a", "b", "z"])))],
        "it is offered instead"
    );
    assert_invariants(&d);
}

/// The return node and a capture from a body are dependencies no wire in the
/// network shows.
#[test]
fn a_return_node_and_a_captured_reader_are_committed() {
    // The return node.
    let mut d = setup(
        "ab = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
abz = expr { expression: \"{a: 1.0, b: 2.0, z: 3.0}\", parameters: [] }
d = record_destructure {}
",
    );
    let (ab, abz, r) = (id(&d, "ab"), id(&d, "abz"), id(&d, "d"));
    d.connect_nodes(ab, 0, r, 0);
    edit(&mut d, "output d\n");
    assert!(!is_uncommitted_record_reader(network(&d), None, r));
    d.connect_nodes(abz, 0, r, 0);
    assert_eq!(
        pins(&d, r),
        vec!["a", "b"],
        "the return node keeps its type"
    );

    // A capture from a body, one level and two levels down.
    for nested in [false, true] {
        let mut d = setup(
            "ab = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
abz = expr { expression: \"{a: 1.0, b: 2.0, z: 3.0}\", parameters: [] }
d = record_destructure {}
rg = range { count: 2 }
",
        );
        let (ab, abz, r) = (id(&d, "ab"), id(&d, "abz"), id(&d, "d"));
        d.connect_nodes(ab, 0, r, 0);
        let body = if nested {
            "mp = map { xs: rg, input_type: Int, output_type: Int, body {
  r2 = range { count: 1 }
  inner = map { xs: r2, input_type: Int, output_type: Float, body {
    c = expr { expression: \"q\", parameters: [{ name: \"q\", data_type: Float }], q: ^^d.a }
    output c
  } }
  g = int { value: 1 }
  output g
} }
"
        } else {
            "mp = map { xs: rg, input_type: Int, output_type: Float, body {
  c = expr { expression: \"q\", parameters: [{ name: \"q\", data_type: Float }], q: ^d.a }
  output c
} }
"
        };
        edit(&mut d, body);
        assert!(
            !is_uncommitted_record_reader(network(&d), None, r),
            "nested: {}",
            nested
        );
        d.connect_nodes(abz, 0, r, 0);
        assert_eq!(pins(&d, r), vec!["a", "b"], "nested: {}", nested);
        let mp = id(&d, "mp");
        let consumer_scope = if nested {
            vec![mp, id_in(&d, &[mp], "inner")]
        } else {
            vec![mp]
        };
        let c = id_in(&d, &consumer_scope, "c");
        let wire = scope_net(&d, &consumer_scope).nodes[&c].arguments[0].incoming_wires[0].clone();
        assert_eq!(wire.source_node_id, r, "the capture stays");
        assert_eq!(wire.source_pin, SourcePin::NodeOutput { pin_index: 0 });
        assert_invariants(&d);
    }
}

// ---------------------------------------------------------------------------
// Take type from wire
// ---------------------------------------------------------------------------

#[test]
fn take_type_from_wire_follows_an_upstream_record_construct() {
    let mut d = setup(
        "x = float { value: 1.0 }
rc = record_construct { fields: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Float }], a: x, b: x }
d = record_destructure { record: rc }
ca = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.a }
cb = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.b }
",
    );
    let (rc, r, ca, cb) = (id(&d, "rc"), id(&d, "d"), id(&d, "ca"), id(&d, "cb"));
    assert_eq!(pins(&d, r), vec!["a", "b"]);
    let ids_before: Vec<u64> = reader(&d, r).fields.iter().map(|f| f.id).collect();

    // `rc` gains `c`: a wider source, still valid, but stale.
    d.set_record_construct_fields(
        &[],
        rc,
        vec![
            ("a".to_string(), DataType::Float),
            ("b".to_string(), DataType::Float),
            ("c".to_string(), DataType::Float),
        ],
    )
    .unwrap();
    assert!(d.wire_type_offers(&[], r)[0].is_some());
    assert_undo_redo_roundtrip(&mut d, |d| d.take_types_from_wires(&[], r, &[0]).unwrap());
    assert!(d.redo());
    assert_eq!(pins(&d, r), vec!["a", "b", "c"]);
    let ids_after: Vec<u64> = reader(&d, r).fields.iter().map(|f| f.id).collect();
    assert_eq!(&ids_after[..2], &ids_before[..], "ids kept by name");
    assert_eq!(fed_by(&d, ca, r).as_deref(), Some("a"));
    assert_eq!(fed_by(&d, cb, r).as_deref(), Some("b"));
    assert_eq!(
        d.wire_type_offers(&[], r),
        vec![None],
        "nothing left to offer"
    );

    // `rc` renames `b` to `q`: the wire on `b` goes, `q` appears.
    d.set_record_construct_fields(
        &[],
        rc,
        vec![
            ("a".to_string(), DataType::Float),
            ("q".to_string(), DataType::Float),
            ("c".to_string(), DataType::Float),
        ],
    )
    .unwrap();
    d.take_types_from_wires(&[], r, &[0]).unwrap();
    assert_eq!(pins(&d, r), vec!["a", "q", "c"]);
    assert_eq!(fed_by(&d, ca, r).as_deref(), Some("a"));
    assert_eq!(fed_by(&d, cb, r), None, "the wire on b is dropped");
    assert_invariants(&d);
}

#[test]
fn staleness_decides_the_offer() {
    let mut d = committed_ab();
    let r = id(&d, "d");
    assert_eq!(d.wire_type_offers(&[], r), vec![None], "the adopted wire");
    edit(
        &mut d,
        "x = float { value: 1.0 }
bx = record_construct { schema: \"Box\", a: x, b: x }
ab2 = expr { expression: \"{b: 1.0, a: 2.0}\", parameters: [] }
abz = expr { expression: \"{a: 1.0, b: 2.0, z: 3.0}\", parameters: [] }
",
    );
    let cases = [
        ("bx", false, "a named source of the same shape"),
        ("ab2", false, "an anonymous source of the same shape"),
        ("abz", true, "a wider source"),
    ];
    for (source, offered, label) in cases {
        let s = id(&d, source);
        d.connect_nodes(s, 0, r, 0);
        assert_eq!(
            pins(&d, r),
            vec!["a", "b"],
            "{}: committed, no adoption",
            label
        );
        assert_eq!(
            d.wire_type_offers(&[], r)[0].is_some(),
            offered,
            "{}",
            label
        );
    }
    // A source that does not convert (the wire is made below the gate, as a
    // def edit would leave it).
    edit(
        &mut d,
        "bc = expr { expression: \"{b: 1.0, c: 2.0}\", parameters: [] }\n",
    );
    let bc = id(&d, "bc");
    d.get_scope_network_mut(&[])
        .unwrap()
        .connect_nodes(bc, 0, r, 0, false);
    assert_eq!(
        d.wire_type_offers(&[], r),
        vec![Some(DataType::Record(floats(&["b", "c"])))],
        "a source that does not convert"
    );
    d.take_types_from_wires(&[], r, &[0]).unwrap();
    assert_eq!(d.wire_type_offers(&[], r), vec![None], "after taking it");
    assert_invariants(&d);
}

// ---------------------------------------------------------------------------
// Mode switches
// ---------------------------------------------------------------------------

#[test]
fn mode_switches_keep_wires_by_name() {
    // Anonymous {a, b} -> Box{a, b} through the dropdown.
    let mut d = committed_ab();
    let (r, ca, cb) = (id(&d, "d"), id(&d, "ca"), id(&d, "cb"));
    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_input_schema(&[], r, "Box").unwrap()
    });
    assert!(d.redo());
    assert_eq!(
        d.undo_stack.undo_description(),
        Some(SET_RECORD_INPUT_SCHEMA_DESCRIPTION)
    );
    assert_eq!(reader(&d, r).schema, "Box");
    assert!(reader(&d, r).fields.is_empty());
    assert_eq!(fed_by(&d, ca, r).as_deref(), Some("a"));
    assert_eq!(fed_by(&d, cb, r).as_deref(), Some("b"));

    // Named Box -> anonymous {a, b, z} through Take type from wire.
    edit(
        &mut d,
        "abz = expr { expression: \"{a: 1.0, b: 2.0, z: 3.0}\", parameters: [] }\n",
    );
    d.connect_nodes(id(&d, "abz"), 0, r, 0);
    assert_eq!(reader(&d, r).schema, "Box", "committed");
    d.take_types_from_wires(&[], r, &[0]).unwrap();
    assert_eq!(reader(&d, r).schema, "");
    assert_eq!(pins(&d, r), vec!["a", "b", "z"]);
    assert_eq!(fed_by(&d, ca, r).as_deref(), Some("a"));
    assert_eq!(fed_by(&d, cb, r).as_deref(), Some("b"));
    assert_invariants(&d);
}

#[test]
fn none_from_the_dropdown_unsets_the_reader_and_undo_restores_its_wires() {
    let mut d = committed_ab();
    let (r, ca, cb) = (id(&d, "d"), id(&d, "ca"), id(&d, "cb"));
    assert_undo_redo_roundtrip(&mut d, |d| d.set_record_input_schema(&[], r, "").unwrap());
    assert_eq!(
        fed_by(&d, ca, r).as_deref(),
        Some("a"),
        "undo restored them"
    );
    assert!(d.redo());
    assert!(reader(&d, r).fields.is_empty() && reader(&d, r).schema.is_empty());
    assert_eq!(pins(&d, r), vec!["result"]);
    assert_eq!((fed_by(&d, ca, r), fed_by(&d, cb, r)), (None, None));
    assert_eq!(
        wires_into(&d, r),
        vec![vec![(id(&d, "e"), 0)]],
        "the input wire stays"
    );

    // Picking the current schema changes nothing.
    let pushes = d.undo_stack.push_count();
    d.set_record_input_schema(&[], r, "").unwrap();
    assert_eq!(d.undo_stack.push_count(), pushes);
    assert_invariants(&d);
}

fn field_id(d: &StructureDesigner, def: &str, field: &str) -> FieldId {
    d.node_type_registry
        .lookup_record_type_def(def)
        .unwrap()
        .fields
        .iter()
        .find(|f| f.name == field)
        .unwrap()
        .id
}

/// Two defs whose counters both start at 0 (as in a file saved before the
/// file-level floor): `Box.a` and `Foo.c` share a `FieldId`.
fn colliding_defs() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    for def in [def("Box", &["a", "b"]), def("Foo", &["c", "b"])] {
        d.node_type_registry
            .record_type_defs
            .insert(def.name.clone(), def);
    }
    edit(
        &mut d,
        "x = float { value: 1.0 }
bx = record_construct { schema: \"Box\", a: x, b: x }
d = record_destructure { schema: \"Box\", record: bx }
ea = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.a }
eb = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.b }
",
    );
    d
}

/// The step-1 test of the design, rewritten: a schema switch keeps the wire
/// on the field both defs share by name, drops the other — even when its id
/// collides with a field of the new def — and is one undo step that restores
/// both. (Before, the switch went through `SetNodeData` and kept both wires
/// by pin index: the wire on `a` silently read `Foo.b`, the one on `b`
/// `Foo.c`.)
#[test]
fn a_schema_switch_keeps_wires_by_name_never_by_a_colliding_id() {
    let mut d = colliding_defs();
    assert_eq!(
        field_id(&d, "Box", "a"),
        field_id(&d, "Foo", "c"),
        "precondition"
    );
    let (r, ea, eb) = (id(&d, "d"), id(&d, "ea"), id(&d, "eb"));
    assert_undo_redo_roundtrip(&mut d, |d| {
        d.set_record_input_schema(&[], r, "Foo").unwrap()
    });
    assert_eq!(fed_by(&d, ea, r).as_deref(), Some("a"), "undo restored a");
    assert_eq!(fed_by(&d, eb, r).as_deref(), Some("b"), "and b");
    assert!(d.redo());
    assert_eq!(pins(&d, r), vec!["c", "b"]);
    assert_eq!(fed_by(&d, ea, r), None, "a is dropped, not moved onto c");
    assert_eq!(fed_by(&d, eb, r).as_deref(), Some("b"));
}

/// The same between a local def and a linked library's, whose ids come from
/// their own files' counters.
#[test]
fn a_schema_switch_between_a_local_and_a_linked_def_keeps_wires_by_name() {
    let ws = tempfile::tempdir().unwrap();
    let lib = ws.path().join("lib.cnnd");
    {
        let mut l = new_design(&lib);
        l.add_record_type_def(def("Box", &["a", "b"])).unwrap();
        save(&mut l);
    }
    let host = ws.path().join("host.cnnd");
    let mut h = new_design(&host);
    h.link_library("lib.cnnd", "lib").unwrap();
    h.add_record_type_def(def("Foo", &["c", "b"])).unwrap();
    assert_eq!(
        field_id(&h, "lib.Box", "a"),
        field_id(&h, "Foo", "c"),
        "precondition: two files' counters"
    );
    edit_network(
        &mut h,
        NET,
        "x = float { value: 1.0 }
bx = record_construct { schema: \"lib.Box\", a: x, b: x }
d = record_destructure { schema: \"lib.Box\", record: bx }
ea = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.a }
eb = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.b }
",
    );
    let (r, ea, eb) = (id(&h, "d"), id(&h, "ea"), id(&h, "eb"));
    h.set_record_input_schema(&[], r, "Foo").unwrap();
    assert_eq!(fed_by(&h, ea, r), None);
    assert_eq!(fed_by(&h, eb, r).as_deref(), Some("b"));
    assert!(h.undo());
    assert_eq!(fed_by(&h, ea, r).as_deref(), Some("a"));
}

/// `repair_node_network`'s R3 pass stays id-only: a field deleted from a def
/// and re-added under the same name is a new field, and the old one's wire
/// is dropped rather than re-pointed by name.
#[test]
fn a_def_edit_drops_a_deleted_fields_wire_even_when_a_new_field_takes_its_name() {
    let mut d = setup(
        "x = float { value: 1.0 }
bx = record_construct { schema: \"Box\", a: x, b: x }
d = record_destructure { schema: \"Box\", record: bx }
ea = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: d.a }
",
    );
    let (r, ea) = (id(&d, "d"), id(&d, "ea"));
    let b = field_id(&d, "Box", "b");
    d.update_record_type_def_with_ids(
        "Box",
        vec![
            RecordFieldEdit {
                id: Some(b),
                name: "b".to_string(),
                data_type: DataType::Float,
                hint: None,
            },
            RecordFieldEdit {
                id: None,
                name: "a".to_string(),
                data_type: DataType::Float,
                hint: None,
            },
        ],
    )
    .unwrap();
    assert_eq!(pins(&d, r), vec!["b", "a"]);
    assert_eq!(fed_by(&d, ea, r), None);
}

// ---------------------------------------------------------------------------
// Local fields that name a def
// ---------------------------------------------------------------------------

/// A local field typed `Record(Named(Box))` is reached by the reference hooks
/// with no change to them: a rename rewrites it, and the reference check
/// blocks deleting the def.
#[test]
fn a_local_field_naming_a_def_follows_renames_and_blocks_deletion() {
    let mut d = setup(
        "x = float { value: 1.0 }
bx = record_construct { schema: \"Box\", a: x, b: x }
w = record_construct { fields: [{ name: \"p\", data_type: Record(Box) }], p: bx }
d = record_destructure { record: w }
",
    );
    let r = id(&d, "d");
    let field_type = |d: &StructureDesigner| reader(d, r).fields[0].data_type.clone();
    assert_eq!(field_type(&d), DataType::Record(named("Box")));
    let mut refs = Vec::new();
    collect_record_refs_in_node(&network(&d).nodes[&r], &mut |name, _| {
        refs.push(name.to_string())
    });
    assert!(
        refs.contains(&"Box".to_string()),
        "the hook sees the field type"
    );
    d.rename_record_type_def("Box", "Crate").unwrap();
    assert_eq!(field_type(&d), DataType::Record(named("Crate")));
    assert!(d.delete_record_type_def("Crate").is_err(), "blocked");
    assert_invariants(&d);
}

/// A local field typed by a linked library's def freezes the reader when the
/// library drops the def, and its wires survive.
#[test]
fn a_local_field_typed_by_a_missing_linked_def_freezes_the_reader() {
    let ws = tempfile::tempdir().unwrap();
    let lib = ws.path().join("lib.cnnd");
    {
        let mut l = new_design(&lib);
        l.add_record_type_def(def("Box", &["a"])).unwrap();
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
bx = record_construct { schema: \"lib.Box\", a: x }
w = record_construct { fields: [{ name: \"p\", data_type: Record(`lib.Box`) }], p: bx }
d = record_destructure { record: w }
",
        );
        assert_eq!(
            reader(&h, id(&h, "d")).fields[0].data_type,
            DataType::Record(named("lib.Box"))
        );
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
    let (w, r) = (id(&h, "w"), id(&h, "d"));
    assert!(is_frozen(&network(&h).nodes[&r], &h.node_type_registry));
    assert_eq!(wires_into(&h, r), vec![vec![(w, 0)]], "the wire survives");
}

#[test]
fn a_reader_with_a_schema_and_local_fields_violates_an_invariant() {
    let mut d = setup("d = record_destructure { schema: \"Box\" }\n");
    let r = id(&d, "d");
    let mut data = reader(&d, r);
    data.fields.push(LocalRecordField {
        id: 0,
        name: "a".to_string(),
        data_type: DataType::Float,
    });
    data.next_field_id = 1;
    d.get_scope_network_mut(&[])
        .unwrap()
        .nodes
        .get_mut(&r)
        .unwrap()
        .data = Box::new(data);
    let violations = check_network_invariants(network(&d), &d.node_type_registry);
    assert!(
        violations
            .iter()
            .any(|v| v.kind == InvariantKind::NamedRecordWithLocalFields),
        "{:#?}",
        violations
    );
}

// ---------------------------------------------------------------------------
// Text format
// ---------------------------------------------------------------------------

/// `query` → `--replace` changes nothing, for each mode.
#[test]
fn query_then_replace_is_a_no_op_for_every_mode() {
    let mut d = setup(
        "x = float { value: 1.0 }
bx = record_construct { schema: \"Box\", a: x, b: x }
e = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
named = record_destructure { schema: \"Box\", record: bx }
anon = record_destructure { record: e }
unset = record_destructure { schema: \"\", record: e }
ca = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Float }], x: anon.b }
",
    );
    assert_eq!(reader(&d, id(&d, "anon")).fields.len(), 2);
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
    assert!(reader(&d, id(&d, "unset")).fields.is_empty());
}

#[test]
fn a_wire_declares_the_type_and_explicit_fields_win() {
    let d = setup(
        "e = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
declared = record_destructure { record: e }
listed = record_destructure { fields: [{ name: \"a\", data_type: Float }], record: e }
",
    );
    let declared = reader(&d, id(&d, "declared"));
    let names: Vec<&str> = declared.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"], "the wire declared it");
    let listed = reader(&d, id(&d, "listed"));
    let names: Vec<&str> = listed.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["a"], "an explicit fields wins");
}

/// A wire *from* the reader written before the reader's own statement waits
/// for the reader's pins and is made with no warning.
#[test]
fn a_wire_from_the_reader_may_come_before_its_statement() {
    let mut d = setup("");
    let outcome = d.ai_text_edit(
        "x = expr { expression: \"v\", parameters: [{ name: \"v\", data_type: Float }], v: d.a }
e = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
d = record_destructure { record: e }
",
        false,
    );
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
    d.validate_active_network();
    let (x, r) = (id(&d, "x"), id(&d, "d"));
    assert_eq!(fed_by(&d, x, r).as_deref(), Some("a"));
    assert!(matches!(eval_pin(&d, x, 0), NetworkResult::Float(v) if v == 1.0));
}

#[test]
fn text_rejects_fields_on_a_named_reader() {
    let mut d = setup("d = record_destructure { schema: \"Box\" }\n");
    let outcome = d.ai_text_edit(
        "d = record_destructure { fields: [{ name: \"a\", data_type: Float }] }\n",
        false,
    );
    assert!(!outcome.result.errors.is_empty() || !outcome.result.warnings.is_empty());
    assert_eq!(reader(&d, id(&d, "d")).schema, "Box");
}

/// An adopted anonymous type with a nested record keeps it as one field.
#[test]
fn a_nested_record_is_one_field() {
    let mut d = setup(AB_AND_UNSET);
    edit(
        &mut d,
        "n = expr { expression: \"{p: {x: 1}, s: 2.0}\", parameters: [] }\n",
    );
    let (n, r) = (id(&d, "n"), id(&d, "d"));
    d.connect_nodes(n, 0, r, 0);
    let data = reader(&d, r);
    assert_eq!(
        data.record_typed_input()
            .unwrap()
            .record_input_type()
            .unwrap(),
        anon(&[
            ("p", DataType::Record(anon(&[("x", DataType::Int)]))),
            ("s", DataType::Float)
        ])
    );
    assert_invariants(&d);
}
