//! `expr` reads named records (`doc/design_anonymous_record_readers.md`,
//! Part 3 / Phase 4), through `StructureDesigner`: the cache populator
//! settles a pending `r.x` on load, repair and every op, the property panel's
//! setter and the text editor see the settled type, and a def edit re-derives
//! it. `Expr::validate_with_defs` alone is tested in the expr harness
//! (`tests/expr/expr_named_record_access_test.rs`).

use super::library_links_support::{edit as edit_network, new_design, open, save};
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::invariants::check_document_invariants;
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::node_type_registry::RecordTypeDef;
use atomcad_structure_designer::nodes::expr::ExprData;
use atomcad_structure_designer::structure_designer::StructureDesigner;

const NET: &str = "Main";

/// `Box = { size: Float, w: Float }`.
fn box_def() -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        "Box",
        vec![
            ("size".to_string(), DataType::Float),
            ("w".to_string(), DataType::Float),
        ],
    )
}

/// The upstream every test reads: `bx` is a `Box` of `2.5`s.
const BOX_SOURCE: &str = "x = float { value: 2.5 }
bx = record_construct { schema: \"Box\", size: x, w: x }
";

fn setup(code: &str) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    d.add_record_type_def(box_def()).unwrap();
    edit(&mut d, &format!("{}{}", BOX_SOURCE, code));
    d
}

/// Add `code` to the active network (incremental text edit), validated,
/// asserting that it applied with no error and no warning.
fn edit(d: &mut StructureDesigner, code: &str) {
    let outcome = d.ai_text_edit(code, false);
    assert!(
        outcome.result.errors.is_empty() && outcome.result.warnings.is_empty(),
        "edit: errors {:?}, warnings {:?}\n{}",
        outcome.result.errors,
        outcome.result.warnings,
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

fn expr_data(d: &StructureDesigner, node_id: u64) -> ExprData {
    network(d).nodes[&node_id]
        .data
        .as_any_ref()
        .downcast_ref::<ExprData>()
        .expect("an expr")
        .clone()
}

/// The output type the node's pin 0 declares.
fn output_type(d: &StructureDesigner, node_id: u64) -> DataType {
    let node = &network(d).nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .output_type()
        .clone()
}

fn eval(d: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &d.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    NetworkEvaluator::new().evaluate(
        &[NetworkStackElement::root(network)],
        node_id,
        0,
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

fn assert_invariants(d: &StructureDesigner) {
    let violations = check_document_invariants(&d.node_type_registry);
    assert!(violations.is_empty(), "{:#?}", violations);
}

fn float(result: NetworkResult) -> f64 {
    match result {
        NetworkResult::Float(v) => v,
        other => panic!("expected a Float, got {}", other.to_display_string()),
    }
}

#[test]
fn a_named_field_types_as_the_def_field_and_evaluates() {
    let d = setup("y = expr { expression: \"r.size * 2\", r: bx }\n");
    let y = id(&d, "y");
    assert_eq!(
        expr_data(&d, y).parameters[0].data_type,
        DataType::Record(RecordType::Named("Box".to_string())),
        "the wire declared `r` as the named record"
    );
    assert_eq!(output_type(&d, y), DataType::Float);
    assert_eq!(expr_data(&d, y).error, None);
    assert!(errors_on(&d, y).is_empty(), "{:?}", errors_on(&d, y));
    assert_eq!(float(eval(&d, y)), 5.0);
    assert_invariants(&d);
}

/// One text edit declares `r` from its wire, which leaves `y` pending until
/// the editor re-derives it; `z.x` is wired from `y` in the same edit and
/// must find it typed. A consumer declared from `y`'s type works the same.
#[test]
fn a_text_edit_wires_a_consumer_of_a_named_field_in_the_same_edit() {
    let d = setup(
        "y = expr { expression: \"r.size\", r: bx }
z = vec3 { x: y }
e = expr { expression: \"v + 1\", v: y }
",
    );
    let (y, z, e) = (id(&d, "y"), id(&d, "z"), id(&d, "e"));
    assert_eq!(network(&d).nodes[&z].arguments[0].get_node_id(), Some(y));
    assert_eq!(expr_data(&d, e).parameters[0].data_type, DataType::Float);
    assert_eq!(float(eval(&d, e)), 3.5);
    assert!(errors_on(&d, z).is_empty(), "{:?}", errors_on(&d, z));
    assert_invariants(&d);
}

/// The consumer statement comes first: it is retried after `y` is declared.
#[test]
fn statement_order_does_not_matter() {
    let d = setup(
        "z = vec3 { x: y }
y = expr { expression: \"r.size\", r: bx }
",
    );
    let (y, z) = (id(&d, "y"), id(&d, "z"));
    assert_eq!(network(&d).nodes[&z].arguments[0].get_node_id(), Some(y));
    assert_invariants(&d);
}

#[test]
fn a_missing_field_names_the_def() {
    let d = setup("y = expr { expression: \"r.q\", r: bx }\n");
    let y = id(&d, "y");
    assert_eq!(output_type(&d, y), DataType::None);
    assert_eq!(
        expr_data(&d, y).error.as_deref(),
        Some("Validation error: Record `Box` has no field 'q'")
    );
}

/// The property panel's setter types the expression with the defs, so the
/// panel's error and the node's validation error agree with the text path.
#[test]
fn the_property_panel_setter_types_with_the_defs() {
    let mut d = setup("y = expr { expression: \"r.size\", r: bx }\n");
    let y = id(&d, "y");

    let mut data = expr_data(&d, y);
    data.expression = "r.w + r.size".to_string();
    d.set_node_network_data_scoped(&[], y, Box::new(data));
    assert_eq!(output_type(&d, y), DataType::Float);
    assert!(errors_on(&d, y).is_empty(), "{:?}", errors_on(&d, y));
    assert_eq!(float(eval(&d, y)), 5.0);

    let mut data = expr_data(&d, y);
    data.expression = "r.q".to_string();
    d.set_node_network_data_scoped(&[], y, Box::new(data));
    assert_eq!(
        errors_on(&d, y),
        vec!["Validation error: Record `Box` has no field 'q'".to_string()]
    );
    assert_invariants(&d);
}

#[test]
fn retyping_the_field_re_derives_the_output_type_through_repair() {
    let mut d = setup("y = expr { expression: \"r.size\", r: bx }\n");
    let y = id(&d, "y");
    assert_eq!(output_type(&d, y), DataType::Float);

    d.update_record_type_def(
        "Box",
        vec![
            ("size".to_string(), DataType::Int),
            ("w".to_string(), DataType::Float),
        ],
    )
    .unwrap();
    assert_eq!(output_type(&d, y), DataType::Int);

    // Deleting the field turns the access into the def-naming error.
    d.update_record_type_def("Box", vec![("w".to_string(), DataType::Float)])
        .unwrap();
    assert_eq!(output_type(&d, y), DataType::None);
    assert_eq!(
        expr_data(&d, y).error.as_deref(),
        Some("Validation error: Record `Box` has no field 'size'")
    );
    assert_invariants(&d);
}

#[test]
fn renaming_the_def_keeps_the_access_typed() {
    let mut d = setup("y = expr { expression: \"r.size\", r: bx }\n");
    let y = id(&d, "y");
    d.rename_record_type_def("Box", "Crate").unwrap();
    assert_eq!(
        expr_data(&d, y).parameters[0].data_type,
        DataType::Record(RecordType::Named("Crate".to_string()))
    );
    assert_eq!(output_type(&d, y), DataType::Float);
    assert_eq!(expr_data(&d, y).error, None);
    assert_eq!(float(eval(&d, y)), 2.5);
    assert_invariants(&d);
}

/// `Opt = { size: Float, extra: Optional[Float] }`, with `extra` unset.
#[test]
fn an_optional_field_reads_as_its_inner_type_and_yields_none_when_unset() {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Opt",
        vec![
            ("size".to_string(), DataType::Float),
            (
                "extra".to_string(),
                DataType::Optional(Box::new(DataType::Float)),
            ),
        ],
    ))
    .unwrap();
    edit(
        &mut d,
        "x = float { value: 2.5 }
o = record_construct { schema: \"Opt\", size: x }
y = expr { expression: \"r.extra\", r: o }
",
    );
    let y = id(&d, "y");
    assert_eq!(output_type(&d, y), DataType::Float);
    assert!(matches!(eval(&d, y), NetworkResult::None));
    assert_invariants(&d);
}

/// The loader's `parse_and_validate` has no defs and leaves `r.size` pending;
/// the load path's populate settles it with no edit.
#[test]
fn a_loaded_file_gives_the_right_output_type_without_an_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("named_access.cnnd");
    {
        let mut d = new_design(&path);
        d.add_record_type_def(box_def()).unwrap();
        edit_network(
            &mut d,
            NET,
            &format!(
                "{}y = expr {{ expression: \"r.size * 2\", r: bx }}\n",
                BOX_SOURCE
            ),
        );
        save(&mut d);
    }
    let mut d = open(&path);
    d.set_active_node_network_name(Some(NET.to_string()));
    d.validate_active_network();
    let y = id(&d, "y");
    assert_eq!(output_type(&d, y), DataType::Float);
    assert_eq!(expr_data(&d, y).error, None);
    assert!(errors_on(&d, y).is_empty(), "{:?}", errors_on(&d, y));
    assert_eq!(float(eval(&d, y)), 5.0);
    assert_invariants(&d);
}
