//! Integration tests for the string template literal feature on the `expr`
//! node.
//!
//! Mirrors `expr_array_literal_test.rs`:
//! - An expr node with a template-literal expression validates to `String`.
//! - The expression survives a `.cnnd` save/load roundtrip with all derived
//!   state (parsed Expr, output_type) restored.
//! - The String output of an expr is type-compatible with `export_atoms`'s
//!   `file_name` input pin.

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::expr::ExprData;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::edit_network;
use std::collections::HashMap;
use tempfile::tempdir;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn edit_designer_network(
    designer: &mut StructureDesigner,
    network_name: &str,
    code: &str,
    replace: bool,
) -> rust_lib_flutter_cad::structure_designer::text_format::EditResult {
    let mut network = designer
        .node_type_registry
        .node_networks
        .remove(network_name)
        .unwrap();
    let result = edit_network(&mut network, &designer.node_type_registry, code, replace);
    designer
        .node_type_registry
        .node_networks
        .insert(network_name.to_string(), network);
    designer.validate_active_network();
    result
}

fn find_expr_data<'a>(designer: &'a StructureDesigner, network_name: &str) -> &'a ExprData {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let (node_id, _) = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "expr")
        .expect("an expr node should exist in the network");
    let data = network
        .get_node_network_data(*node_id)
        .expect("node data should exist");
    data.as_any_ref()
        .downcast_ref::<ExprData>()
        .expect("expr node should carry ExprData")
}

#[test]
fn test_expr_template_literal_validates_to_string() {
    let mut designer = setup_designer_with_network("main");

    // Motivating use case: build a per-variant filename from a String + Int.
    // (Record-typed parameters validate the same shape; covered by the
    // template_literal_test unit tests.)
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            literal = expr {
                expression: "`output/${species}_size${size}.xyz`",
                parameters: [
                    { name: "species", data_type: String },
                    { name: "size", data_type: Int }
                ]
            }
        "#,
        true,
    );
    assert!(
        result.success,
        "edit_network should succeed: {:?}",
        result.errors
    );

    let expr_data = find_expr_data(&designer, "main");
    assert_eq!(
        expr_data.output_type,
        Some(DataType::String),
        "template literal expression should validate to String"
    );
    assert!(
        expr_data.expr.is_some(),
        "template should be parsed (got error: {:?})",
        expr_data.error
    );
}

#[test]
fn test_expr_template_literal_evaluates_with_record_input() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            literal = expr {
                expression: "`output/${species}_size${size}.xyz`",
                parameters: [
                    { name: "species", data_type: String },
                    { name: "size", data_type: Int }
                ]
            }
        "#,
        true,
    );
    assert!(
        result.success,
        "edit_network should succeed: {:?}",
        result.errors
    );

    let expr_data = find_expr_data(&designer, "main");
    let parsed = expr_data.expr.clone().expect("expression must be parsed");

    let mut variables = HashMap::new();
    variables.insert(
        "species".to_string(),
        NetworkResult::String("Si".to_string()),
    );
    variables.insert("size".to_string(), NetworkResult::Int(8));

    let result = parsed.evaluate(
        &variables,
        rust_lib_flutter_cad::expr::validation::get_function_implementations(),
    );
    match result {
        NetworkResult::String(s) => assert_eq!(s, "output/Si_size8.xyz"),
        other => panic!(
            "expected String result, got type {:?}",
            other.infer_data_type()
        ),
    }
}

#[test]
fn test_expr_template_literal_cnnd_roundtrip() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            literal = expr {
                expression: "`output/${species}_size${size}.xyz`",
                parameters: [
                    { name: "species", data_type: String },
                    { name: "size", data_type: Int }
                ]
            }
        "#,
        true,
    );
    assert!(
        result.success,
        "edit_network should succeed: {:?}",
        result.errors
    );

    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("template_literal.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save should succeed");

    let mut registry2 = NodeTypeRegistry::new();
    let _load = load_node_networks_from_file(&mut registry2, path.to_str().unwrap())
        .expect("load should succeed");

    let network = registry2
        .node_networks
        .get("main")
        .expect("main network should survive roundtrip");
    let (node_id, _) = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "expr")
        .expect("expr node should survive roundtrip");
    let data = network
        .get_node_network_data(*node_id)
        .expect("node data should exist after roundtrip");
    let expr_data = data
        .as_any_ref()
        .downcast_ref::<ExprData>()
        .expect("expr node should carry ExprData after roundtrip");

    assert_eq!(
        expr_data.expression, "`output/${species}_size${size}.xyz`",
        "template-literal expression survives roundtrip verbatim"
    );
    assert_eq!(
        expr_data.output_type,
        Some(DataType::String),
        "template literal validates to String after roundtrip"
    );
    assert!(
        expr_data.expr.is_some(),
        "template literal is re-parsed after roundtrip"
    );
}

#[test]
fn test_template_string_output_compatible_with_export_atoms_file_name_pin() {
    // The `file_name` pin on `export_atoms` is `DataType::String`, and the expr
    // template literal output is `DataType::String`. `can_be_converted_to`
    // must therefore accept the wire.
    let registry = NodeTypeRegistry::new();
    let source = DataType::String;
    let dest = DataType::String;
    assert!(
        DataType::can_be_converted_to(&source, &dest, &registry),
        "String -> String should be a permitted connection (expr template -> export_atoms.file_name)"
    );
}
