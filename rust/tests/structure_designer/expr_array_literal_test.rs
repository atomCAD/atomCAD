//! Integration tests for the array-literal feature on the `expr` node:
//! - A zero-parameter expr node can be used as a pure inline literal source.
//! - The expression survives a `.cnnd` save/load roundtrip.
//! - The output type is `Array[IVec3]` and downstream `Array[IVec3]` consumers
//!   accept the connection.
//!
//! Uses the text-format `edit_network` API to set the expression, since that
//! mirrors the production code path (and is the path used by the CLI / AI
//! integration).

use glam::i32::IVec3;
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

/// Apply text-format edits to a named network using the same temporary-removal
/// dance the existing tests use to dodge borrow-checker conflicts.
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
fn test_zero_param_expr_evaluates_array_literal() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            literal = expr {
                expression: "[ivec3(1,2,3), ivec3(4,5,6)]",
                parameters: []
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
    assert!(
        expr_data.parameters.is_empty(),
        "expr should have zero parameters; got {} ({:?})",
        expr_data.parameters.len(),
        expr_data
            .parameters
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        expr_data.output_type,
        Some(DataType::Array(Box::new(DataType::IVec3))),
        "output type should validate to Array[IVec3]"
    );
    assert!(
        expr_data.expr.is_some(),
        "expression should be parsed (got error: {:?})",
        expr_data.error
    );
}

#[test]
fn test_zero_param_expr_array_literal_evaluation_values() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            literal = expr {
                expression: "[ivec3(1,2,3), ivec3(4,5,6)]",
                parameters: []
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

    let result = parsed.evaluate(
        &HashMap::new(),
        rust_lib_flutter_cad::expr::validation::get_function_implementations(),
    );
    match result {
        NetworkResult::Array(elements) => {
            assert_eq!(elements.len(), 2);
            match (&elements[0], &elements[1]) {
                (NetworkResult::IVec3(a), NetworkResult::IVec3(b)) => {
                    assert_eq!(*a, IVec3::new(1, 2, 3));
                    assert_eq!(*b, IVec3::new(4, 5, 6));
                }
                _ => panic!("expected two IVec3 elements"),
            }
        }
        other => panic!("expected Array result, got {}", other.to_display_string()),
    }
}

#[test]
fn test_zero_param_expr_array_literal_cnnd_roundtrip() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            literal = expr {
                expression: "[ivec3(1,2,3), ivec3(4,5,6)]",
                parameters: []
            }
        "#,
        true,
    );
    assert!(
        result.success,
        "edit_network should succeed: {:?}",
        result.errors
    );

    // Save to a temp .cnnd file.
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("zero_param_array.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save should succeed");

    // Reload into a fresh registry.
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
        expr_data.expression, "[ivec3(1,2,3), ivec3(4,5,6)]",
        "expression text survives roundtrip"
    );
    assert!(
        expr_data.parameters.is_empty(),
        "expression should have no parameters after roundtrip"
    );
    assert_eq!(
        expr_data.output_type,
        Some(DataType::Array(Box::new(DataType::IVec3))),
        "output type validates after roundtrip"
    );
    assert!(
        expr_data.expr.is_some(),
        "expression is parsed after roundtrip"
    );
}

#[test]
fn test_array_literal_connection_to_array_consumer() {
    // Validate that the output type Array[IVec3] from a zero-param expr literal
    // would be accepted by a downstream Array[IVec3] input pin via DataType
    // conversion rules.
    let source_type = DataType::Array(Box::new(DataType::IVec3));
    let dest_type = DataType::Array(Box::new(DataType::IVec3));
    assert!(
        DataType::can_be_converted_to(&source_type, &dest_type),
        "Array[IVec3] -> Array[IVec3] should be a permitted connection"
    );

    // And that a permitted element-wise upcast also flows: Array[IVec3] -> Array[Vec3].
    let promoted = DataType::Array(Box::new(DataType::Vec3));
    assert!(
        DataType::can_be_converted_to(&source_type, &promoted),
        "Array[IVec3] should convert element-wise to Array[Vec3]"
    );
}
