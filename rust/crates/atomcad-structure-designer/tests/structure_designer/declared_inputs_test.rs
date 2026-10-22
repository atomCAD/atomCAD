//! Declared inputs on `expr` (`doc/design_wire_declared_inputs.md`, Phase 1):
//! the open port (`add_input_from_wire`, its hover check) and Take type from
//! wire (`wire_type_offers`, `take_types_from_wires`), driven through
//! `StructureDesigner`. The name derivation itself is tabled in
//! `open_port_name_test.rs`; the read-only refusals are in the library-linking
//! mutation alphabet (`library_links_readonly_test.rs`).

use crate::undo_test::assert_undo_redo_roundtrip;
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::declared_input_ops::{
    ADD_INPUT_FROM_WIRE_DESCRIPTION, TAKE_TYPE_FROM_WIRE_DESCRIPTION,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type_registry::RecordTypeDef;
use atomcad_structure_designer::nodes::expr::{ExprData, ExprParameter};
use atomcad_structure_designer::nodes::record_construct::RecordConstructData;
use atomcad_structure_designer::nodes::value::add_typed_value_node;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_changes::{
    RefreshMode, StructureDesignerChanges,
};
use atomcad_structure_designer::text_format::TextValue;
use glam::f64::DVec2;
use std::collections::HashMap;

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

fn id_in(net: &NodeNetwork, name: &str) -> u64 {
    net.nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node '{}'", name))
        .id
}

fn id(d: &StructureDesigner, name: &str) -> u64 {
    id_in(network(d), name)
}

fn expr_at<'a>(d: &'a StructureDesigner, scope: &[u64], node_id: u64) -> &'a ExprData {
    d.get_node_network_data_scoped(scope, node_id)
        .unwrap()
        .as_any_ref()
        .downcast_ref::<ExprData>()
        .unwrap()
}

fn expr(d: &StructureDesigner, node_id: u64) -> &ExprData {
    expr_at(d, &[], node_id)
}

fn params(d: &StructureDesigner, node_id: u64) -> Vec<(String, DataType)> {
    expr(d, node_id)
        .parameters
        .iter()
        .map(|p| (p.name.clone(), p.data_type.clone()))
        .collect()
}

/// `(source node, source pin, depth)` of every wire into each input pin.
fn wires_into(
    d: &StructureDesigner,
    scope: &[u64],
    node_id: u64,
) -> Vec<Vec<(u64, SourcePin, u8)>> {
    d.get_scope_network(scope).unwrap().nodes[&node_id]
        .arguments
        .iter()
        .map(|arg| {
            arg.incoming_wires
                .iter()
                .map(|w| (w.source_node_id, w.source_pin, w.source_scope_depth))
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

/// The network as saved, for "nothing changed" checks.
fn fingerprint(d: &mut StructureDesigner) -> String {
    let snapshot = d.snapshot_network(NET).unwrap();
    serde_json::to_string(&snapshot).unwrap()
}

/// A network with one of every kind of source the refusal table needs:
/// `v` (Vec3), `p` (a node with an abstract output type), `g` (an `expr`
/// whose function pin is a non-nullary function), `nothing` (a `None`-typed
/// value), `i` (an `int`, a node without declared inputs) and `e` (the
/// destination `expr`).
fn refusal_network() -> StructureDesigner {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
i = int { value: 1 }
g = expr { expression: \"q + 1\", parameters: [{ name: \"q\", data_type: Int }] }
e = expr { expression: \"1\", parameters: [] }
cl = closure { kind: \"custom\", params: [], type_args: [HasAtoms], body { m = atom_edit {} output m } }
p = apply { f: cl }
",
    );
    let net = d.node_type_registry.node_networks.get_mut(NET).unwrap();
    let nothing = add_typed_value_node(
        net,
        DVec2::new(0.0, 400.0),
        NetworkResult::None,
        DataType::None,
    );
    net.nodes.get_mut(&nothing).unwrap().custom_name = Some("nothing".to_string());
    d.validate_active_network();
    // `p` (an `apply` of a closure declared `() -> HasAtoms`) really has an
    // abstract output: the drop must see `HasAtoms`.
    let p = id(&d, "p");
    assert_eq!(
        d.resolve_wire_source_type(&[], p, OUT, 0),
        Some(DataType::HasAtoms)
    );
    d
}

// ---------------------------------------------------------------------------
// The open port
// ---------------------------------------------------------------------------

#[test]
fn a_drop_appends_a_parameter_of_the_sources_type_and_wires_it() {
    let mut d = setup(
        "a = int { value: 2 }
v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"x * 2\", parameters: [{ name: \"x\", data_type: Int }], x: a }
",
    );
    let (a, v, e) = (id(&d, "a"), id(&d, "v"), id(&d, "e"));
    let pushes = d.undo_stack.push_count();

    let added = d.add_input_from_wire(&[], e, v, OUT, 0).unwrap();

    // `expr` names a new parameter with the next free letter, and offers the
    // source's own name as a rename.
    assert_eq!(added.name, "y");
    assert_eq!(added.source_name.as_deref(), Some("v"));
    assert_eq!(added.pin_index, 1);
    assert_eq!(
        params(&d, e),
        vec![
            ("x".to_string(), DataType::Int),
            ("y".to_string(), DataType::Vec3)
        ]
    );
    let new_param = &expr(&d, e).parameters[1];
    assert_eq!(new_param.id, Some(added.pin_id));
    assert_eq!(
        wires_into(&d, &[], e),
        vec![vec![(a, OUT, 0)], vec![(v, OUT, 0)]],
        "the earlier wire is untouched and the new one lands on the new pin"
    );
    assert_eq!(
        expr(&d, e).expression,
        "x * 2",
        "the expression is never edited"
    );
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
    assert_eq!(d.undo_stack.push_count(), pushes + 1, "one undo step");
    assert_eq!(
        d.undo_stack.undo_description(),
        Some(ADD_INPUT_FROM_WIRE_DESCRIPTION)
    );
}

#[test]
fn the_pin_index_is_the_new_parameters_row() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
w = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"1\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
",
    );
    let (v, w, e) = (id(&d, "v"), id(&d, "w"), id(&d, "e"));
    let first = d.add_input_from_wire(&[], e, v, OUT, 0).unwrap();
    let second = d.add_input_from_wire(&[], e, w, OUT, 0).unwrap();
    // `pendingNameFocus` keys on the index: it must be the row of the
    // parameter list the editor shows (`get_expr_data` lists `parameters` in
    // order).
    let rows = &expr(&d, e).parameters;
    assert_eq!(rows[first.pin_index].name, first.name);
    assert_eq!(rows[second.pin_index].name, second.name);
    assert_eq!((first.pin_index, second.pin_index), (2, 3));
}

#[test]
fn successive_drops_on_an_expr_take_the_next_free_letter() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"1\", parameters: [] }
",
    );
    let (v, e) = (id(&d, "v"), id(&d, "e"));
    assert_eq!(d.add_input_from_wire(&[], e, v, OUT, 0).unwrap().name, "x");
    assert_eq!(d.add_input_from_wire(&[], e, v, OUT, 0).unwrap().name, "y");
}

#[test]
fn a_renamed_letter_is_handed_out_again() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"1\", parameters: [{ name: \"radius\", data_type: Float }, { name: \"y\", data_type: Float }] }
",
    );
    let (v, e) = (id(&d, "v"), id(&d, "e"));
    assert_eq!(d.add_input_from_wire(&[], e, v, OUT, 0).unwrap().name, "x");
    assert_eq!(d.add_input_from_wire(&[], e, v, OUT, 0).unwrap().name, "z");
}

#[test]
fn a_second_drop_of_the_same_source_on_a_record_is_deduplicated() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
r = record_construct {}
",
    );
    let (v, r) = (id(&d, "v"), id(&d, "r"));
    let first = d.add_input_from_wire(&[], r, v, OUT, 0).unwrap();
    assert_eq!(first.name, "v");
    assert_eq!(first.source_name, None, "the name already is the source's");
    assert_eq!(
        d.add_input_from_wire(&[], r, v, OUT, 0).unwrap().name,
        "v_2"
    );
}

#[test]
fn a_multi_output_source_names_the_input_after_its_pin() {
    let mut d = setup(
        "u = lattice_vecs_unpack {}
e = expr { expression: \"1\", parameters: [] }
r = record_construct {}
",
    );
    let (u, e, r) = (id(&d, "u"), id(&d, "e"), id(&d, "r"));
    let b = SourcePin::NodeOutput { pin_index: 1 };
    assert_eq!(d.add_input_from_wire(&[], r, u, b, 0).unwrap().name, "b");
    let added = d.add_input_from_wire(&[], e, u, b, 0).unwrap();
    assert_eq!(added.name, "x");
    assert_eq!(added.source_name.as_deref(), Some("b"));
    assert_eq!(params(&d, e), vec![("x".to_string(), DataType::Vec3)]);
}

#[test]
fn a_polymorphic_source_declares_the_resolved_type() {
    let mut d = setup("e = expr { expression: \"1\", parameters: [] }");
    let src = d.add_node("atom_edit", DVec2::new(0.0, 200.0));
    let tag = d.add_node("tag", DVec2::new(200.0, 200.0));
    d.connect_nodes(src, 0, tag, 0);
    let e = id(&d, "e");

    d.add_input_from_wire(&[], e, tag, OUT, 0).unwrap();

    // `tag` declares `SameAsInput` over a `HasAtoms` pin; the parameter takes
    // the concrete type flowing through it, never the abstract one.
    assert_eq!(params(&d, e)[0].1, DataType::Molecule);
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
}

/// Each refusal: the hover check says why, the drop says the same, and
/// neither changes the network or pushes an undo step.
#[test]
fn refusals_leave_the_network_unchanged_and_push_nothing() {
    let mut d = refusal_network();
    let e = id(&d, "e");
    let i = id(&d, "i");
    let cases: Vec<(&str, u64, u64, SourcePin)> = vec![
        ("abstract source", e, id(&d, "p"), OUT),
        ("function source", e, id(&d, "g"), FUNCTION_PIN),
        ("None-typed source", e, id(&d, "nothing"), OUT),
        ("destination without a port", i, id(&d, "v"), OUT),
        ("own output", e, e, OUT),
    ];
    for (label, dest, source, pin) in cases {
        let before = fingerprint(&mut d);
        let pushes = d.undo_stack.push_count();
        let reason = d.can_add_input_from_wire(&[], dest, source, pin, 0);
        let result = d.add_input_from_wire(&[], dest, source, pin, 0);
        assert!(reason.is_some(), "{}: the hover check accepted it", label);
        assert_eq!(result.err(), reason, "{}: the drop disagrees", label);
        assert_eq!(
            before,
            fingerprint(&mut d),
            "{}: the network changed",
            label
        );
        assert_eq!(
            pushes,
            d.undo_stack.push_count(),
            "{}: pushed an undo step",
            label
        );
    }
}

#[test]
fn a_drop_from_the_parent_scope_into_a_map_body_is_a_capture() {
    let mut d = setup(
        "f = float { value: 1.5 }
r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, body { g = int { value: 1 } output g } }
",
    );
    let (f, mp) = (id(&d, "f"), id(&d, "mp"));
    let body_expr = d.add_node_scoped(&[mp], "expr", DVec2::new(10.0, 10.0), None);
    assert!(
        d.can_add_input_from_wire(&[mp], body_expr, f, OUT, 1)
            .is_none()
    );

    assert_undo_redo_roundtrip(&mut d, |d| {
        let added = d.add_input_from_wire(&[mp], body_expr, f, OUT, 1).unwrap();
        assert_eq!(added.name, "x");
        assert_eq!(added.source_name.as_deref(), Some("f"));
        assert_eq!(added.pin_index, 0);
    });

    // After the roundtrip helper's final undo, redo to inspect the result.
    assert!(d.redo());
    let data = expr_at(&d, &[mp], body_expr);
    assert_eq!(data.parameters[0].data_type, DataType::Float);
    assert_eq!(wires_into(&d, &[mp], body_expr)[0], vec![(f, OUT, 1)]);
}

#[test]
fn a_drop_from_a_zone_input_names_the_input_after_the_pin() {
    let mut d = setup(
        "r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, body { g = int { value: 1 } output g } }
",
    );
    let mp = id(&d, "mp");
    let body_expr = d.add_node_scoped(&[mp], "expr", DVec2::new(10.0, 10.0), None);
    let element = SourcePin::ZoneInput { pin_index: 0 };

    let added = d
        .add_input_from_wire(&[mp], body_expr, mp, element, 1)
        .unwrap();

    assert_eq!(added.name, "x");
    assert_eq!(added.source_name.as_deref(), Some("element"));
    let data = expr_at(&d, &[mp], body_expr);
    assert_eq!(data.parameters[0].data_type, DataType::Int);
    assert_eq!(wires_into(&d, &[mp], body_expr)[0], vec![(mp, element, 1)]);
}

#[test]
fn a_top_level_drop_is_one_undo_step() {
    let mut d = setup(
        "a = int { value: 2 }
v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: Int }], x: a }
",
    );
    let (v, e) = (id(&d, "v"), id(&d, "e"));
    assert_undo_redo_roundtrip(&mut d, |d| {
        d.add_input_from_wire(&[], e, v, OUT, 0).unwrap();
    });
}

/// `NetworkSnapshotCommand`'s `Full` refresh rebuilds derived state: the
/// `expr`'s output type, the validation errors, and — through the network's
/// interface — the output type of an instance of the network elsewhere.
#[test]
fn undo_and_redo_of_a_drop_leave_derived_state_current() {
    let mut d = StructureDesigner::new();
    d.add_node_network("Sub");
    d.add_node_network(NET);
    d.set_active_node_network_name(Some("Sub".to_string()));
    edit(
        &mut d,
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"x * 2.0\", parameters: [] }
output e
",
    );
    d.set_active_node_network_name(Some(NET.to_string()));
    let instance = d.add_node("Sub", DVec2::ZERO);
    d.set_active_node_network_name(Some("Sub".to_string()));

    let instance_type = |d: &StructureDesigner| {
        let main = &d.node_type_registry.node_networks[NET];
        d.node_type_registry
            .get_node_type_for_node(&main.nodes[&instance])
            .unwrap()
            .output_type()
            .clone()
    };
    let (v, e) = {
        let net = &d.node_type_registry.node_networks["Sub"];
        (id_in(net, "v"), id_in(net, "e"))
    };
    let derived = |d: &mut StructureDesigner| {
        let output = expr(d, e).output_type.clone();
        let errors: Vec<String> = network(d)
            .validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect();
        // A freshly validated copy must agree.
        d.validate_active_network();
        let fresh_errors: Vec<String> = network(d)
            .validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect();
        assert_eq!(errors, fresh_errors, "validation errors went stale");
        output
    };

    // The expression names `x` before the input exists: no output type yet.
    assert_eq!(derived(&mut d), None);
    assert_eq!(instance_type(&d), DataType::None);

    d.add_input_from_wire(&[], e, v, OUT, 0).unwrap();
    assert_eq!(derived(&mut d), Some(DataType::Vec3));
    assert_eq!(instance_type(&d), DataType::Vec3);

    assert!(d.undo());
    assert_eq!(derived(&mut d), None);
    assert_eq!(instance_type(&d), DataType::None);

    assert!(d.redo());
    assert_eq!(derived(&mut d), Some(DataType::Vec3));
    assert_eq!(instance_type(&d), DataType::Vec3);
}

#[test]
fn a_later_rename_keeps_the_wire() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"1\", parameters: [] }
",
    );
    let (v, e) = (id(&d, "v"), id(&d, "e"));
    let added = d.add_input_from_wire(&[], e, v, OUT, 0).unwrap();

    // The property panel's rename: the parameter list with the new name.
    let mut data = expr(&d, e).clone();
    let mut props = HashMap::new();
    props.insert(
        "parameters".to_string(),
        TextValue::Array(vec![TextValue::Object(vec![
            ("name".to_string(), TextValue::String("offset".to_string())),
            ("data_type".to_string(), TextValue::DataType(DataType::Vec3)),
        ])]),
    );
    data.set_text_properties(&props).unwrap();
    d.set_node_network_data_scoped(&[], e, Box::new(data));

    let param = &expr(&d, e).parameters[0];
    assert_eq!(param.name, "offset");
    assert_eq!(param.id, Some(added.pin_id), "the id survives the rename");
    assert_eq!(wires_into(&d, &[], e), vec![vec![(v, OUT, 0)]]);
}

// ---------------------------------------------------------------------------
// Take type from wire
// ---------------------------------------------------------------------------

/// `e` has four Float parameters: `a` fed by a Vec3 (stale), `b` fed by an
/// Int (compatible), `c` unwired, `d` fed by a non-nullary function (stale,
/// but not a type an `expr` parameter can take).
fn offers_network() -> StructureDesigner {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
i = int { value: 1 }
g = expr { expression: \"q + 1\", parameters: [{ name: \"q\", data_type: Int }] }
e = expr { expression: \"a * 2\", parameters: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Float }, { name: \"c\", data_type: Float }, { name: \"d\", data_type: Float }] }
",
    );
    let (v, i, g, e) = (id(&d, "v"), id(&d, "i"), id(&d, "g"), id(&d, "e"));
    d.connect_nodes(v, 0, e, 0);
    d.connect_nodes(i, 0, e, 1);
    d.connect_nodes(g, -1, e, 3);
    d
}

#[test]
fn offers_are_made_only_on_stale_inputs_the_node_can_take() {
    let d = offers_network();
    let e = id(&d, "e");
    assert_eq!(
        d.wire_type_offers(&[], e),
        vec![Some(DataType::Vec3), None, None, None]
    );
    // One entry per input pin, in pin (= editor row) order.
    assert_eq!(
        d.wire_type_offers(&[], e).len(),
        expr(&d, e).parameters.len()
    );
}

#[test]
fn a_node_without_declared_inputs_has_no_offers() {
    let d = offers_network();
    assert!(d.wire_type_offers(&[], id(&d, "i")).is_empty());
}

#[test]
fn taking_the_type_retypes_in_place_and_follows_through() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
i = int { value: 1 }
e = expr { expression: \"a * 2\", parameters: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Int }], b: i }
",
    );
    let (v, i, e) = (id(&d, "v"), id(&d, "i"), id(&d, "e"));
    d.connect_nodes(v, 0, e, 0);
    d.set_node_display(e, true);
    let ids_before: Vec<Option<u64>> = expr(&d, e).parameters.iter().map(|p| p.id).collect();
    assert!(has_mismatch(&d, e));
    assert_eq!(expr(&d, e).output_type, Some(DataType::Float));

    // Evaluate once, so a result computed under the old type could linger.
    let changes = StructureDesignerChanges {
        mode: RefreshMode::Full,
        ..Default::default()
    };
    d.refresh(&changes);

    d.take_types_from_wires(&[], e, &[0]).unwrap();

    let data = expr(&d, e);
    assert_eq!(data.parameters[0].name, "a");
    assert_eq!(data.parameters[0].data_type, DataType::Vec3);
    assert_eq!(data.parameters[0].data_type_str, None);
    assert_eq!(
        data.parameters.iter().map(|p| p.id).collect::<Vec<_>>(),
        ids_before,
        "ids are kept"
    );
    assert_eq!(data.expression, "a * 2", "the expression is never edited");
    assert_eq!(
        wires_into(&d, &[], e),
        vec![vec![(v, OUT, 0)], vec![(i, OUT, 0)]],
        "the wires are kept"
    );
    assert!(errors_on(&d, e).is_empty(), "{:?}", errors_on(&d, e));
    assert_eq!(data.output_type, Some(DataType::Vec3));
    assert_eq!(
        d.undo_stack.undo_description(),
        Some(TAKE_TYPE_FROM_WIRE_DESCRIPTION)
    );

    // The evaluated value follows: nothing computed before the retype is
    // served after it.
    let changes = d.get_pending_changes();
    d.refresh(&changes);
    let shown = d
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], e)
        .expect("the displayed expr has an output string");
    assert!(
        shown[0].contains('2') && shown[0].contains('4') && shown[0].contains('6'),
        "expected the Vec3 (2, 4, 6), got {:?}",
        shown
    );
}

#[test]
fn two_stale_inputs_taken_together_are_one_undo_step() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
w = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"a + b\", parameters: [{ name: \"a\", data_type: Float }, { name: \"b\", data_type: Float }] }
",
    );
    let (v, w, e) = (id(&d, "v"), id(&d, "w"), id(&d, "e"));
    d.connect_nodes(v, 0, e, 0);
    d.connect_nodes(w, 0, e, 1);
    assert_undo_redo_roundtrip(&mut d, |d| {
        d.take_types_from_wires(&[], e, &[0, 1]).unwrap();
        assert_eq!(params(d, e)[0].1, DataType::Vec3);
        assert_eq!(params(d, e)[1].1, DataType::Vec3);
    });
}

#[test]
fn take_refusals_change_nothing() {
    let mut d = offers_network();
    let e = id(&d, "e");
    let cases: Vec<(&str, Vec<usize>)> = vec![
        ("compatible input", vec![1]),
        ("unwired input", vec![2]),
        ("function source", vec![3]),
        ("out of range", vec![9]),
        ("empty list", vec![]),
        ("offered and not offered", vec![0, 1]),
    ];
    for (label, indices) in cases {
        let before = fingerprint(&mut d);
        let pushes = d.undo_stack.push_count();
        assert!(
            d.take_types_from_wires(&[], e, &indices).is_err(),
            "{}: accepted",
            label
        );
        assert_eq!(
            before,
            fingerprint(&mut d),
            "{}: the network changed",
            label
        );
        assert_eq!(
            pushes,
            d.undo_stack.push_count(),
            "{}: pushed an undo step",
            label
        );
    }
}

#[test]
fn offers_follow_a_polymorphic_chain_to_the_concrete_type() {
    let mut d =
        setup("e = expr { expression: \"1\", parameters: [{ name: \"m\", data_type: Crystal }] }");
    let src = d.add_node("atom_edit", DVec2::new(0.0, 200.0));
    let tag = d.add_node("tag", DVec2::new(200.0, 200.0));
    d.connect_nodes(src, 0, tag, 0);
    let e = id(&d, "e");
    d.connect_nodes(tag, 0, e, 0);
    assert!(has_mismatch(&d, e));
    assert_eq!(d.wire_type_offers(&[], e), vec![Some(DataType::Molecule)]);
}

/// A fresh `expr` with one parameter `p: declared` and expression `text`,
/// fed by `source`'s `pin`. Returns its id.
fn add_consumer(
    d: &mut StructureDesigner,
    declared: DataType,
    text: &str,
    source: u64,
    pin: SourcePin,
) -> u64 {
    let dest = d.add_node("expr", DVec2::new(600.0, 0.0));
    let mut data = expr(d, dest).clone();
    data.parameters = vec![ExprParameter {
        id: Some(1),
        name: "p".to_string(),
        data_type: declared,
        data_type_str: None,
    }];
    data.expression = text.to_string();
    d.set_node_network_data_scoped(&[], dest, Box::new(data));
    let SourcePin::NodeOutput { pin_index } = pin else {
        unreachable!()
    };
    d.connect_nodes(source, pin_index, dest, 0);
    dest
}

/// For every pair, the offer is exactly what the validator's verdict and the
/// node's type gate say it should be — which pins the offer's source
/// resolution to the validator's. A non-record pair is offered iff it carries
/// a mismatch error and `expr` accepts the type; a record pair iff it carries
/// one *or* the resolved fields differ.
#[test]
fn offers_agree_with_the_validator() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
i = int { value: 1 }
f = float { value: 1.5 }
b = bool { value: true }
g = expr { expression: \"q + 1\", parameters: [{ name: \"q\", data_type: Int }] }
ab = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
a = expr { expression: \"{a: 1.0}\", parameters: [] }
c = expr { expression: \"{c: 1}\", parameters: [] }
",
    );
    let float = DataType::Float;
    let only_a = anon(&[("a", DataType::Float)]);
    // (source, pin, declared, `Some(fields differ)` for a record pair, the
    // expected offer as a sanity anchor)
    let cases: Vec<(&str, SourcePin, DataType, Option<bool>, bool)> = vec![
        ("v", OUT, float.clone(), None, true),
        ("i", OUT, float.clone(), None, false),
        ("f", OUT, DataType::Int, None, false),
        ("v", OUT, DataType::IVec3, None, false),
        ("b", OUT, DataType::Int, None, true),
        ("f", OUT, DataType::Vec3, None, true),
        ("g", FUNCTION_PIN, float.clone(), None, false),
        ("ab", OUT, only_a.clone(), Some(true), true),
        ("a", OUT, only_a.clone(), Some(false), false),
        ("c", OUT, only_a.clone(), Some(true), true),
    ];
    for (source_name, pin, declared, record_differ, expected) in cases {
        let source = id(&d, source_name);
        let dest = add_consumer(&mut d, declared.clone(), "1", source, pin);
        d.validate_active_network();
        let source_type = d.resolve_wire_source_type(&[], source, pin, 0).unwrap();
        let accepted = ExprData::accepts_parameter_type(&source_type).is_ok();
        let offered = d.wire_type_offers(&[], dest)[0].is_some();
        let label = format!("{} -> {}", source_name, declared);
        let should = (has_mismatch(&d, dest) || record_differ.unwrap_or(false)) && accepted;
        assert_eq!(
            offered, should,
            "{}: offer disagrees with the validator",
            label
        );
        assert_eq!(offered, expected, "{}: unexpected verdict", label);
    }
}

#[test]
fn a_wider_record_is_offered_without_an_error_and_taking_it_lets_the_new_field_read() {
    let mut d = setup("ab = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }");
    let ab = id(&d, "ab");
    let dest = add_consumer(&mut d, anon(&[("a", DataType::Float)]), "p.b", ab, OUT);
    d.validate_active_network();
    assert!(
        !has_mismatch(&d, dest),
        "width subtyping: the wire is valid"
    );
    assert_eq!(
        expr(&d, dest).output_type,
        None,
        "`p.b` does not validate yet"
    );
    let wider = anon(&[("a", DataType::Float), ("b", DataType::Float)]);
    assert_eq!(d.wire_type_offers(&[], dest), vec![Some(wider.clone())]);

    d.take_types_from_wires(&[], dest, &[0]).unwrap();

    let data = expr(&d, dest);
    assert_eq!(data.parameters[0].data_type, wider);
    assert_eq!(data.error, None, "{:?}", data.error);
    assert_eq!(data.output_type, Some(DataType::Float));
    assert_eq!(
        d.wire_type_offers(&[], dest),
        vec![None],
        "nothing stale is left"
    );
}

/// Named sources: a def with exactly the declared anonymous shape is not
/// stale; a wider one is offered in its **anonymous** form, never by name
/// (an `expr` cannot read a named record's fields).
#[test]
fn a_named_source_is_offered_in_anonymous_form_and_only_when_it_differs() {
    let mut d = setup("f = float { value: 1.5 }");
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Box",
        vec![("a".to_string(), DataType::Float)],
    ))
    .unwrap();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Wide",
        vec![
            ("a".to_string(), DataType::Float),
            ("b".to_string(), DataType::Float),
        ],
    ))
    .unwrap();
    let construct = |d: &mut StructureDesigner, schema: &str| {
        let node = d.add_node("record_construct", DVec2::new(0.0, 300.0));
        let data = RecordConstructData {
            schema: schema.to_string(),
            ..Default::default()
        };
        d.set_node_network_data_scoped(&[], node, Box::new(data));
        node
    };
    let only_a = anon(&[("a", DataType::Float)]);

    let boxed = construct(&mut d, "Box");
    let same = add_consumer(&mut d, only_a.clone(), "p.a", boxed, OUT);
    let wide = construct(&mut d, "Wide");
    let wider = add_consumer(&mut d, only_a.clone(), "p.a", wide, OUT);
    d.validate_active_network();

    assert_eq!(d.wire_type_offers(&[], same), vec![None]);
    assert_eq!(
        d.wire_type_offers(&[], wider),
        vec![Some(anon(&[
            ("a", DataType::Float),
            ("b", DataType::Float)
        ]))]
    );
}

/// A file written before parameter ids existed: the loader gives every
/// id-less parameter a fresh id, so each pin is addressable by id (an id-less
/// parameter could never be offered a type, `declares_pin` matching ids only).
#[test]
fn legacy_id_less_parameters_get_ids_on_load() {
    use atomcad_structure_designer::declared_inputs::DeclaredInputs;
    use atomcad_structure_designer::nodes::expr::expr_data_loader;

    let json = serde_json::json!({
        "parameters": [
            { "name": "a", "data_type": "Int", "data_type_str": null },
            { "id": 7, "name": "b", "data_type": "Int", "data_type_str": null },
            { "name": "c", "data_type": "Int", "data_type_str": null }
        ],
        "expression": "a + b + c"
    });
    let loaded = expr_data_loader(&json, None).unwrap();
    let data = loaded.as_any_ref().downcast_ref::<ExprData>().unwrap();
    let ids: Vec<Option<u64>> = data.parameters.iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![Some(8), Some(7), Some(9)]);
    assert!(ids.iter().all(|id| data.declares_pin(id.unwrap())));
}
