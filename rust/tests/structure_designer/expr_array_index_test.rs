//! Integration test: expr node with array indexing survives a `.cnnd` save/load
//! roundtrip and re-validates to the expected element type.

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
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

/// Indexes a literal array inside the expression so the test does not need to
/// declare a parameter of array type — the text format has no native syntax for
/// `Array[T]` parameter `data_type` values, but inline array literals exercise
/// the same indexing code paths through validate / evaluate / save / load.
const EXPR_TEXT: &str = "[ivec3(1,2,3), ivec3(4,5,6)][i]";

#[test]
fn test_expr_array_index_cnnd_roundtrip() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        &format!(
            r#"
                indexer = expr {{
                    expression: "{}",
                    parameters: [{{ name: "i", data_type: Int }}]
                }}
            "#,
            EXPR_TEXT
        ),
        true,
    );
    assert!(
        result.success,
        "edit_network should succeed: {:?}",
        result.errors
    );

    // Verify in-memory state.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let (node_id, _) = network
            .nodes
            .iter()
            .find(|(_, n)| n.node_type_name == "expr")
            .expect("an expr node should exist");
        let data = network
            .get_node_network_data(*node_id)
            .expect("node data should exist");
        let expr_data = data
            .as_any_ref()
            .downcast_ref::<ExprData>()
            .expect("expr node should carry ExprData");
        assert_eq!(expr_data.expression, EXPR_TEXT);
        assert_eq!(
            expr_data.output_type,
            Some(DataType::IVec3),
            "indexing Array[IVec3] yields IVec3"
        );
        assert!(expr_data.expr.is_some(), "expression should be parsed");
    }

    // Save then reload into a fresh registry.
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("array_index.cnnd");
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
        expr_data.expression, EXPR_TEXT,
        "expression text survives roundtrip"
    );
    assert_eq!(
        expr_data.output_type,
        Some(DataType::IVec3),
        "output type validates after roundtrip"
    );
    assert!(
        expr_data.expr.is_some(),
        "expression is parsed after roundtrip"
    );
}
