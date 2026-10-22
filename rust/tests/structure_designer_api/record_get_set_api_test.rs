//! The `record_get` / `record_set` panel API (`doc/design_record_get_set.md`
//! Phase 4): the row views, the by-path setters, the literal pair and the
//! field tree. The row edits themselves are the Phase 3 ops and are tested
//! there (`record_path_rows_test.rs`); what is tested here is the translation
//! — that a getter's rows, echoed back, name the same rows (dangling ones
//! included), and that ids never need to cross.

use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::node_type_registry::{RecordFieldEdit, RecordTypeDef};
use atomcad_structure_designer::nodes::record_set::RecordSetData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::TextValue;
use rust_lib_flutter_cad::api::structure_designer::record_path_api::{
    edit_record_get, edit_record_set, edit_record_set_literal, record_field_tree, record_get_view,
    record_path_node_field_tree, record_set_view,
};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
    APILiteralValue, APIRecordPath, APIRecordPathNodeData, APISimpleParamType,
};
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

/// `Point { x, y }`, `Box { p: Point, size }`, a `Box` value `b`, and `code`,
/// in an active `Main`.
fn with(code: &str) -> StructureDesigner {
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
    let text = format!(
        "fx = float {{ value: 1.5 }}
fy = float {{ value: 2.5 }}
fs = float {{ value: 4.0 }}
pt = record_construct {{ schema: \"Point\", x: fx, y: fy }}
b = record_construct {{ schema: \"Box\", p: pt, size: fs }}
{code}"
    );
    let outcome = d.ai_text_edit(&text, true);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    d.validate_active_network();
    d
}

/// `g` and `s`, both with rows `[p.x, size]`; `s.size` is wired.
fn both() -> StructureDesigner {
    with(
        "g = record_get { schema: \"Box\", paths: [\"p.x\", \"size\"], record: b }
s = record_set { schema: \"Box\", paths: [\"p.x\", \"size\"], record: b, size: fs }
",
    )
}

fn main_net(d: &StructureDesigner) -> &NodeNetwork {
    &d.node_type_registry.node_networks["Main"]
}

fn id_in(net: &NodeNetwork, name: &str) -> u64 {
    net.nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node {name}"))
        .id
}

fn id(d: &StructureDesigner, name: &str) -> u64 {
    id_in(main_net(d), name)
}

/// Renames `Point.x` (keeping its id).
fn rename_point_x(d: &mut StructureDesigner, new_name: &str) {
    let fields = d
        .node_type_registry
        .lookup_record_type_def("Point")
        .unwrap()
        .fields
        .clone();
    let edits = fields
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

/// Deletes `Point.x`, dangling every `p.x` row.
fn delete_point_x(d: &mut StructureDesigner) {
    d.update_record_type_def("Point", vec![("y".to_string(), DataType::Float)])
        .unwrap();
    d.validate_active_network();
}

fn names(path: &APIRecordPath) -> Vec<&str> {
    path.names.iter().map(String::as_str).collect()
}

/// The rows of a view, as the setter takes them back.
fn echo(view: &APIRecordPathNodeData) -> Vec<APIRecordPath> {
    view.rows
        .iter()
        .map(|r| APIRecordPath {
            names: r.path.names.clone(),
            dangling: r.path.dangling,
        })
        .collect()
}

fn live(names: &[&str]) -> APIRecordPath {
    APIRecordPath {
        names: names.iter().map(|n| n.to_string()).collect(),
        dangling: false,
    }
}

/// `Main`'s undo snapshot as JSON, with the hash-map-ordered arrays sorted.
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

fn float_literal(view: &APIRecordPathNodeData, row: usize) -> Option<f64> {
    match view.rows[row].literal.as_ref()?.stored_value.as_ref()? {
        APILiteralValue::Float(v) => Some(*v),
        _ => panic!("not a Float literal"),
    }
}

fn set_data<'a>(net: &'a NodeNetwork, node_id: u64) -> &'a RecordSetData {
    net.nodes[&node_id]
        .data
        .as_any_ref()
        .downcast_ref::<RecordSetData>()
        .unwrap()
}

// ============================================================================
// 1. The getters
// ============================================================================

#[test]
fn getters_report_current_names_pin_names_and_leaf_types() {
    let mut d = both();
    let (g, s) = (id(&d, "g"), id(&d, "s"));

    for view in [
        record_get_view(&d, &[], g).unwrap(),
        record_set_view(&d, &[], s).unwrap(),
    ] {
        assert_eq!(view.schema, "Box");
        assert_eq!(view.rows.len(), 2);
        assert_eq!(names(&view.rows[0].path), ["p", "x"]);
        assert!(!view.rows[0].path.dangling);
        assert_eq!(view.rows[0].pin_name.as_deref(), Some("p_x"));
        assert_eq!(view.rows[0].leaf_type.as_deref(), Some("Float"));
        assert!(view.rows[0].problem.is_none());
        assert_eq!(names(&view.rows[1].path), ["size"]);
        assert_eq!(view.rows[1].pin_name.as_deref(), Some("size"));
    }
    assert!(record_get_view(&d, &[], s).is_none(), "not a record_get");
    assert!(record_set_view(&d, &[], g).is_none(), "not a record_set");

    // Only `record_set` rows carry a literal part.
    assert!(
        record_get_view(&d, &[], g).unwrap().rows[0]
            .literal
            .is_none()
    );
    let set_view = record_set_view(&d, &[], s).unwrap();
    let lit = set_view.rows[0].literal.as_ref().unwrap();
    assert_eq!(lit.name, "p_x");
    assert!(matches!(lit.data_type, APISimpleParamType::Float));

    // A rename shows the current names.
    rename_point_x(&mut d, "px");
    let view = record_get_view(&d, &[], g).unwrap();
    assert_eq!(names(&view.rows[0].path), ["p", "px"]);
    assert_eq!(view.rows[0].pin_name.as_deref(), Some("p_px"));

    // A delete dangles the row, which reports its *stored* names (the ones
    // it had when last edited — before the rename) and its reason.
    delete_point_x(&mut d);
    for view in [
        record_get_view(&d, &[], g).unwrap(),
        record_set_view(&d, &[], s).unwrap(),
    ] {
        let row = &view.rows[0];
        assert!(row.path.dangling);
        assert_eq!(names(&row.path), ["p", "x"]);
        assert!(row.pin_name.is_none());
        assert!(row.leaf_type.is_none());
        assert!(row.literal.is_none());
        let problem = row.problem.as_deref().unwrap();
        assert!(problem.contains("no longer exists"), "{problem}");
        assert_eq!(view.rows[1].pin_name.as_deref(), Some("size"));
    }
}

// ============================================================================
// 2. Echo is a no-op; leaving a dangling row out deletes it
// ============================================================================

#[test]
fn echoing_the_rows_back_changes_nothing_and_leaving_a_dangling_row_out_deletes_it() {
    let mut d = both();
    let (g, s) = (id(&d, "g"), id(&d, "s"));
    edit_record_set_literal(&mut d, &[], s, 0, Some(APILiteralValue::Float(3.0))).unwrap();
    rename_point_x(&mut d, "px");
    delete_point_x(&mut d);

    let before = fingerprint(&mut d);
    let pushes = d.undo_stack.push_count();
    let gv = record_get_view(&d, &[], g).unwrap();
    let sv = record_set_view(&d, &[], s).unwrap();
    assert!(gv.rows[0].path.dangling && sv.rows[0].path.dangling);
    edit_record_get(&mut d, &[], g, gv.schema.clone(), echo(&gv)).unwrap();
    edit_record_set(&mut d, &[], s, sv.schema.clone(), echo(&sv)).unwrap();
    assert_eq!(d.undo_stack.push_count(), pushes, "no command pushed");
    assert_eq!(fingerprint(&mut d), before);
    assert_eq!(
        set_data(main_net(&d), s).rows[0].literal,
        Some(TextValue::Float(3.0)),
        "the dangling row kept its literal"
    );

    // The panel's delete button on the broken row: send the rest.
    let rest = |v: &APIRecordPathNodeData| echo(v).into_iter().skip(1).collect();
    edit_record_get(&mut d, &[], g, "Box".to_string(), rest(&gv)).unwrap();
    edit_record_set(&mut d, &[], s, "Box".to_string(), rest(&sv)).unwrap();
    assert_eq!(d.undo_stack.push_count(), pushes + 2);
    for view in [
        record_get_view(&d, &[], g).unwrap(),
        record_set_view(&d, &[], s).unwrap(),
    ] {
        assert_eq!(view.rows.len(), 1);
        assert_eq!(names(&view.rows[0].path), ["size"]);
    }
    assert!(d.undo());
    assert!(d.undo());
    assert_eq!(fingerprint(&mut d), before);
}

#[test]
fn a_bad_path_is_reported_and_changes_nothing() {
    let mut d = both();
    let g = id(&d, "g");
    let before = fingerprint(&mut d);
    let err = edit_record_get(
        &mut d,
        &[],
        g,
        "Box".to_string(),
        vec![live(&["p", "nope"])],
    )
    .unwrap_err();
    assert!(!err.is_empty());
    let dup = edit_record_get(
        &mut d,
        &[],
        g,
        "Box".to_string(),
        vec![live(&["size"]), live(&["size"])],
    );
    assert!(dup.is_err());
    assert_eq!(fingerprint(&mut d), before);
}

// ============================================================================
// 3. The literal pair
// ============================================================================

#[test]
fn the_literal_pair_is_keyed_by_row_index_undoable_and_type_checked() {
    let mut d = both();
    let s = id(&d, "s");

    let view = record_set_view(&d, &[], s).unwrap();
    assert_eq!(float_literal(&view, 0), None, "unset");
    assert!(!view.rows[0].literal.as_ref().unwrap().is_wired);
    assert!(
        view.rows[1].literal.as_ref().unwrap().is_wired,
        "size is wired"
    );

    let pushes = d.undo_stack.push_count();
    edit_record_set_literal(&mut d, &[], s, 0, Some(APILiteralValue::Float(3.0))).unwrap();
    assert_eq!(
        float_literal(&record_set_view(&d, &[], s).unwrap(), 0),
        Some(3.0)
    );
    assert_eq!(d.undo_stack.push_count(), pushes + 1);

    // A wired row stores a literal too; it reactivates on disconnect.
    edit_record_set_literal(&mut d, &[], s, 1, Some(APILiteralValue::Float(5.0))).unwrap();
    let view = record_set_view(&d, &[], s).unwrap();
    assert_eq!(float_literal(&view, 1), Some(5.0));
    assert!(view.rows[1].literal.as_ref().unwrap().is_wired);

    // Rejected: a value of the wrong type, a row that is not there.
    let pushes = d.undo_stack.push_count();
    assert!(
        edit_record_set_literal(
            &mut d,
            &[],
            s,
            0,
            Some(APILiteralValue::Str("abc".to_string()))
        )
        .is_err()
    );
    assert!(edit_record_set_literal(&mut d, &[], s, 7, Some(APILiteralValue::Float(1.0))).is_err());
    assert_eq!(d.undo_stack.push_count(), pushes);
    assert_eq!(
        float_literal(&record_set_view(&d, &[], s).unwrap(), 0),
        Some(3.0)
    );

    // Clear, then undo each step back.
    edit_record_set_literal(&mut d, &[], s, 0, None).unwrap();
    assert_eq!(
        float_literal(&record_set_view(&d, &[], s).unwrap(), 0),
        None
    );
    let pushes = d.undo_stack.push_count();
    edit_record_set_literal(&mut d, &[], s, 0, None).unwrap();
    assert_eq!(
        d.undo_stack.push_count(),
        pushes,
        "clearing nothing pushes nothing"
    );
    assert!(d.undo());
    assert_eq!(
        float_literal(&record_set_view(&d, &[], s).unwrap(), 0),
        Some(3.0)
    );
    assert!(d.undo());
    assert_eq!(
        float_literal(&record_set_view(&d, &[], s).unwrap(), 1),
        None
    );
    assert!(d.undo());
    assert_eq!(
        float_literal(&record_set_view(&d, &[], s).unwrap(), 0),
        None
    );
    assert!(d.redo());
    assert_eq!(
        float_literal(&record_set_view(&d, &[], s).unwrap(), 0),
        Some(3.0)
    );

    // A dangling row has no pin, so it takes no new literal.
    delete_point_x(&mut d);
    assert!(edit_record_set_literal(&mut d, &[], s, 0, Some(APILiteralValue::Float(1.0))).is_err());
}

// ============================================================================
// 4. A body-internal node, through `scope_path`
// ============================================================================

#[test]
fn the_api_reaches_a_body_internal_node_and_undo_restores_it() {
    let mut d = with(
        "r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Float, body { g = record_get { schema: \"Box\", paths: [\"p.x\", \"size\"], record: ^b } s = record_set { schema: \"Box\", paths: [\"p.x\", \"size\"], record: ^b } output g } }
",
    );
    let mp = id(&d, "mp");
    let body = |d: &StructureDesigner| -> NodeNetwork {
        (**main_net(d).nodes[&mp].zone.as_ref().unwrap()).clone()
    };
    let b0 = body(&d);
    let (g, s) = (id_in(&b0, "g"), id_in(&b0, "s"));
    let scope = [mp];
    let original = fingerprint(&mut d);

    let gv = record_get_view(&d, &scope, g).unwrap();
    assert_eq!(gv.rows.len(), 2);
    assert_eq!(gv.rows[0].pin_name.as_deref(), Some("p_x"));
    assert!(
        record_get_view(&d, &[], g).is_none_or(|v| v.rows.len() != 2),
        "the top-level id is a different node"
    );

    // Echo is a no-op here too.
    let pushes = d.undo_stack.push_count();
    edit_record_get(&mut d, &scope, g, "Box".to_string(), echo(&gv)).unwrap();
    assert_eq!(d.undo_stack.push_count(), pushes);

    edit_record_get(&mut d, &scope, g, "Box".to_string(), vec![live(&["size"])]).unwrap();
    edit_record_set_literal(&mut d, &scope, s, 0, Some(APILiteralValue::Float(3.0))).unwrap();
    assert_eq!(record_get_view(&d, &scope, g).unwrap().rows.len(), 1);
    assert_eq!(
        float_literal(&record_set_view(&d, &scope, s).unwrap(), 0),
        Some(3.0)
    );
    assert_eq!(
        set_data(&body(&d), s).rows[0].literal,
        Some(TextValue::Float(3.0)),
        "the literal landed in the body"
    );

    assert!(d.undo());
    assert!(d.undo());
    assert_eq!(fingerprint(&mut d), original);
}

// ============================================================================
// 5. The field tree
// ============================================================================

#[test]
fn the_field_tree_expands_record_fields_named_and_anonymous() {
    let mut d = both();
    d.add_record_type_def(def(
        "Tagged",
        &[
            ("box", named("Box")),
            (
                "meta",
                DataType::Record(RecordType::Anonymous(vec![(
                    "w".to_string(),
                    DataType::Int,
                )])),
            ),
            ("n", DataType::Int),
        ],
    ))
    .unwrap();

    let tree = record_field_tree(&d, "Box");
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].name, "p");
    assert!(tree[0].steppable);
    let p: Vec<&str> = tree[0].children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(p, ["x", "y"]);
    assert_eq!(tree[0].children[0].data_type, "Float");
    assert_eq!(tree[1].name, "size");
    assert!(!tree[1].steppable);
    assert!(tree[1].children.is_empty());

    let tree = record_field_tree(&d, "Tagged");
    assert!(tree[0].steppable);
    assert_eq!(tree[0].children[0].children[0].name, "x", "box.p.x");
    assert!(tree[1].steppable, "an anonymous record steps by name");
    assert_eq!(tree[1].children[0].name, "w");
    assert!(!tree[2].steppable);

    assert!(record_field_tree(&d, "Nope").is_empty());

    // A path picked from the tree resolves through the setter.
    let g = id(&d, "g");
    edit_record_get(
        &mut d,
        &[],
        g,
        "Tagged".to_string(),
        vec![live(&["box", "p", "x"]), live(&["meta", "w"])],
    )
    .unwrap();
    let view = record_get_view(&d, &[], g).unwrap();
    assert_eq!(view.rows[0].pin_name.as_deref(), Some("box_p_x"));
    assert_eq!(view.rows[1].leaf_type.as_deref(), Some("Int"));
}

// ============================================================================
// An anonymous root (`doc/design_anonymous_record_readers.md`, Phase 3)
// ============================================================================

/// A `record_get` that took `{p: Point, s: Float}` from its wire: the picker
/// tree starts at that type — `p` expands to `Point`'s fields, `s` is a leaf —
/// and the view lists the type's fields under an empty schema.
#[test]
fn the_field_tree_of_an_anonymous_root() {
    let mut d = with(
        "rc = record_construct { fields: [{ name: \"p\", data_type: Record(Point) }, { name: \"s\", data_type: Float }], p: pt, s: fs }
g = record_get { record: rc }
u = record_get {}
",
    );
    let (g, u) = (id(&d, "g"), id(&d, "u"));
    let tree = record_path_node_field_tree(&d, &[], g).unwrap();
    let top: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(top, ["p", "s"]);
    assert!(tree[0].steppable);
    let p: Vec<&str> = tree[0].children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(p, ["x", "y"]);
    assert!(!tree[1].steppable);
    assert!(
        record_path_node_field_tree(&d, &[], u).unwrap().is_empty(),
        "an unset node has no root to pick from"
    );

    edit_record_get(&mut d, &[], g, String::new(), vec![live(&["p", "x"])]).unwrap();
    let view = record_get_view(&d, &[], g).unwrap();
    assert_eq!(view.schema, "");
    let fields: Vec<&str> = view.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(fields, ["p", "s"]);
    assert_eq!(view.rows[0].pin_name.as_deref(), Some("p_x"));
    assert!(record_get_view(&d, &[], u).unwrap().fields.is_empty());
}
