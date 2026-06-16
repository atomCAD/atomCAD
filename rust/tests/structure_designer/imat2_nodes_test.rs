//! Phase 2 tests for the three IMat2 constructor nodes:
//! `imat2_rows`, `imat2_cols`, `imat2_diag`.

use glam::f64::DVec2;
use glam::i32::IVec2;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type::{
    generic_node_data_loader, generic_node_data_saver,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::imat2_cols::IMat2ColsData;
use rust_lib_flutter_cad::structure_designer::nodes::imat2_diag::IMat2DiagData;
use rust_lib_flutter_cad::structure_designer::nodes::imat2_rows::IMat2RowsData;
use rust_lib_flutter_cad::structure_designer::nodes::ivec2::IVec2Data;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn evaluate(designer: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn set_node_data<T: NodeData + 'static>(designer: &mut StructureDesigner, node_id: u64, data: T) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut("test").unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = Box::new(data);
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

fn extract_imat2(result: NetworkResult) -> [[i32; 2]; 2] {
    match result {
        NetworkResult::IMat2(m) => m,
        other => panic!("Expected IMat2 result, got {}", other.to_display_string()),
    }
}

// ---------------------------------------------------------------------------
// imat2_rows
// ---------------------------------------------------------------------------

#[test]
fn default_imat2_rows_outputs_identity() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat2_rows", DVec2::ZERO);
    designer.validate_active_network();

    let m = extract_imat2(evaluate(&designer, id));
    assert_eq!(m, [[1, 0], [0, 1]]);
}

#[test]
fn imat2_rows_with_stored_matrix_outputs_stored_value() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat2_rows", DVec2::ZERO);
    set_node_data(
        &mut designer,
        id,
        IMat2RowsData {
            matrix: [[2, 1], [1, 2]],
        },
    );
    designer.validate_active_network();

    let m = extract_imat2(evaluate(&designer, id));
    assert_eq!(m, [[2, 1], [1, 2]]);
}

#[test]
fn imat2_rows_with_wired_a_input_overrides_row_zero_only() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat2_rows", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        id,
        IMat2RowsData {
            // Row `a` should be overridden by the wire; row `b` from storage.
            matrix: [[7, 7], [4, 5]],
        },
    );

    let row_a_id = designer.add_node("ivec2", DVec2::ZERO);
    set_node_data(
        &mut designer,
        row_a_id,
        IVec2Data {
            value: IVec2::new(1, 2),
        },
    );
    designer.connect_nodes(row_a_id, 0, id, 0);
    designer.validate_active_network();

    let m = extract_imat2(evaluate(&designer, id));
    assert_eq!(m, [[1, 2], [4, 5]]);
}

// ---------------------------------------------------------------------------
// imat2_cols
// ---------------------------------------------------------------------------

#[test]
fn default_imat2_cols_outputs_identity() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat2_cols", DVec2::ZERO);
    designer.validate_active_network();

    let m = extract_imat2(evaluate(&designer, id));
    assert_eq!(m, [[1, 0], [0, 1]]);
}

#[test]
fn imat2_cols_with_two_wired_ivec2_inputs_produces_column_composed_matrix() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat2_cols", DVec2::new(400.0, 0.0));

    // col_a = (1,2), col_b = (3,4)
    // Expected row-major matrix:
    //   [[1,3],
    //    [2,4]]
    let col_a_id = designer.add_node("ivec2", DVec2::new(0.0, 0.0));
    let col_b_id = designer.add_node("ivec2", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        col_a_id,
        IVec2Data {
            value: IVec2::new(1, 2),
        },
    );
    set_node_data(
        &mut designer,
        col_b_id,
        IVec2Data {
            value: IVec2::new(3, 4),
        },
    );
    designer.connect_nodes(col_a_id, 0, id, 0);
    designer.connect_nodes(col_b_id, 0, id, 1);
    designer.validate_active_network();

    let m = extract_imat2(evaluate(&designer, id));
    assert_eq!(m, [[1, 3], [2, 4]]);
}

// ---------------------------------------------------------------------------
// imat2_diag
// ---------------------------------------------------------------------------

#[test]
fn imat2_diag_with_wired_ivec2_produces_diagonal_matrix() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat2_diag", DVec2::new(200.0, 0.0));

    let v_id = designer.add_node("ivec2", DVec2::ZERO);
    set_node_data(
        &mut designer,
        v_id,
        IVec2Data {
            value: IVec2::new(2, 3),
        },
    );
    designer.connect_nodes(v_id, 0, id, 0);
    designer.validate_active_network();

    let m = extract_imat2(evaluate(&designer, id));
    assert_eq!(m, [[2, 0], [0, 3]]);
}

#[test]
fn default_imat2_diag_outputs_identity() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat2_diag", DVec2::ZERO);
    designer.validate_active_network();

    let m = extract_imat2(evaluate(&designer, id));
    assert_eq!(m, [[1, 0], [0, 1]]);
}

// ---------------------------------------------------------------------------
// NodeData serializer/loader roundtrip
// ---------------------------------------------------------------------------

fn roundtrip_node_data<T>(node_type_name: &str, data: T) -> Box<dyn NodeData>
where
    T: NodeData + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static + Clone,
{
    let mut boxed: Box<dyn NodeData> = Box::new(data);
    let value = generic_node_data_saver::<T>(boxed.as_mut(), None)
        .unwrap_or_else(|e| panic!("{} saver failed: {}", node_type_name, e));
    generic_node_data_loader::<T>(&value, None)
        .unwrap_or_else(|e| panic!("{} loader failed: {}", node_type_name, e))
}

#[test]
fn imat2_rows_node_data_roundtrips_non_default_matrix() {
    let original = IMat2RowsData {
        matrix: [[2, 1], [1, 2]],
    };
    let restored = roundtrip_node_data("imat2_rows", original.clone());
    let restored_ref = restored
        .as_ref()
        .as_any_ref()
        .downcast_ref::<IMat2RowsData>()
        .expect("downcast");
    assert_eq!(restored_ref.matrix, original.matrix);
}

#[test]
fn imat2_cols_node_data_roundtrips_non_default_matrix() {
    let original = IMat2ColsData {
        matrix: [[2, 1], [1, 2]],
    };
    let restored = roundtrip_node_data("imat2_cols", original.clone());
    let restored_ref = restored
        .as_ref()
        .as_any_ref()
        .downcast_ref::<IMat2ColsData>()
        .expect("downcast");
    assert_eq!(restored_ref.matrix, original.matrix);
}

#[test]
fn imat2_diag_node_data_roundtrips_non_default_vector() {
    let original = IMat2DiagData {
        v: IVec2::new(2, 3),
    };
    let restored = roundtrip_node_data("imat2_diag", original.clone());
    let restored_ref = restored
        .as_ref()
        .as_any_ref()
        .downcast_ref::<IMat2DiagData>()
        .expect("downcast");
    assert_eq!(restored_ref.v, original.v);
}

// ---------------------------------------------------------------------------
// get_node_type() registration — simple integrity checks
// ---------------------------------------------------------------------------

#[test]
fn all_three_imat2_constructors_registered_with_expected_types() {
    let registry = NodeTypeRegistry::new();

    for name in ["imat2_rows", "imat2_cols", "imat2_diag"] {
        let nt = registry
            .built_in_node_types
            .get(name)
            .unwrap_or_else(|| panic!("{} not registered", name));
        assert_eq!(
            nt.output_type().to_string(),
            "IMat2",
            "{} output type",
            name
        );
        assert!(nt.public, "{} should be public", name);
    }
}
