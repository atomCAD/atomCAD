//! Phase 2 tests for the six matrix constructor nodes:
//! `imat3_rows`, `imat3_cols`, `imat3_diag`, `mat3_rows`, `mat3_cols`, `mat3_diag`.

use glam::f64::DVec2;
use glam::f64::{DMat3, DVec3};
use glam::i32::IVec3;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type::{
    generic_node_data_loader, generic_node_data_saver,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::imat3_cols::IMat3ColsData;
use rust_lib_flutter_cad::structure_designer::nodes::imat3_diag::IMat3DiagData;
use rust_lib_flutter_cad::structure_designer::nodes::imat3_rows::IMat3RowsData;
use rust_lib_flutter_cad::structure_designer::nodes::ivec3::IVec3Data;
use rust_lib_flutter_cad::structure_designer::nodes::mat3_cols::Mat3ColsData;
use rust_lib_flutter_cad::structure_designer::nodes::mat3_diag::Mat3DiagData;
use rust_lib_flutter_cad::structure_designer::nodes::mat3_rows::Mat3RowsData;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
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
        node,
        true,
    );
}

fn extract_imat3(result: NetworkResult) -> [[i32; 3]; 3] {
    match result {
        NetworkResult::IMat3(m) => m,
        other => panic!("Expected IMat3 result, got {}", other.to_display_string()),
    }
}

fn extract_mat3(result: NetworkResult) -> DMat3 {
    match result {
        NetworkResult::Mat3(m) => m,
        other => panic!("Expected Mat3 result, got {}", other.to_display_string()),
    }
}

// ---------------------------------------------------------------------------
// imat3_rows
// ---------------------------------------------------------------------------

#[test]
fn default_imat3_rows_outputs_identity() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat3_rows", DVec2::ZERO);
    designer.validate_active_network();

    let m = extract_imat3(evaluate(&designer, id));
    assert_eq!(m, [[1, 0, 0], [0, 1, 0], [0, 0, 1]]);
}

#[test]
fn imat3_rows_with_stored_matrix_outputs_stored_value() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat3_rows", DVec2::ZERO);
    set_node_data(
        &mut designer,
        id,
        IMat3RowsData {
            matrix: [[2, 1, 0], [0, 2, 1], [1, 0, 2]],
        },
    );
    designer.validate_active_network();

    let m = extract_imat3(evaluate(&designer, id));
    assert_eq!(m, [[2, 1, 0], [0, 2, 1], [1, 0, 2]]);
}

#[test]
fn imat3_rows_with_wired_a_input_overrides_row_zero_only() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat3_rows", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        id,
        IMat3RowsData {
            // Stored rows: a=(7,7,7), b=(4,5,6), c=(9,8,7). Row `a` should be
            // overridden by the wire; rows `b`, `c` should come from storage.
            matrix: [[7, 7, 7], [4, 5, 6], [9, 8, 7]],
        },
    );

    let row_a_id = designer.add_node("ivec3", DVec2::ZERO);
    set_node_data(
        &mut designer,
        row_a_id,
        IVec3Data {
            value: IVec3::new(1, 2, 3),
        },
    );
    designer.connect_nodes(row_a_id, 0, id, 0);
    designer.validate_active_network();

    let m = extract_imat3(evaluate(&designer, id));
    assert_eq!(m, [[1, 2, 3], [4, 5, 6], [9, 8, 7]]);
}

// ---------------------------------------------------------------------------
// imat3_cols
// ---------------------------------------------------------------------------

#[test]
fn default_imat3_cols_outputs_identity() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat3_cols", DVec2::ZERO);
    designer.validate_active_network();

    let m = extract_imat3(evaluate(&designer, id));
    assert_eq!(m, [[1, 0, 0], [0, 1, 0], [0, 0, 1]]);
}

#[test]
fn imat3_cols_with_three_wired_ivec3_inputs_produces_column_composed_matrix() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat3_cols", DVec2::new(400.0, 0.0));

    // col_a = (1,2,3), col_b = (4,5,6), col_c = (7,8,9)
    // Expected row-major matrix:
    //   [[1,4,7],
    //    [2,5,8],
    //    [3,6,9]]
    let col_a_id = designer.add_node("ivec3", DVec2::new(0.0, 0.0));
    let col_b_id = designer.add_node("ivec3", DVec2::new(0.0, 100.0));
    let col_c_id = designer.add_node("ivec3", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        col_a_id,
        IVec3Data {
            value: IVec3::new(1, 2, 3),
        },
    );
    set_node_data(
        &mut designer,
        col_b_id,
        IVec3Data {
            value: IVec3::new(4, 5, 6),
        },
    );
    set_node_data(
        &mut designer,
        col_c_id,
        IVec3Data {
            value: IVec3::new(7, 8, 9),
        },
    );
    designer.connect_nodes(col_a_id, 0, id, 0);
    designer.connect_nodes(col_b_id, 0, id, 1);
    designer.connect_nodes(col_c_id, 0, id, 2);
    designer.validate_active_network();

    let m = extract_imat3(evaluate(&designer, id));
    assert_eq!(m, [[1, 4, 7], [2, 5, 8], [3, 6, 9]]);
}

// ---------------------------------------------------------------------------
// imat3_diag
// ---------------------------------------------------------------------------

#[test]
fn imat3_diag_with_wired_ivec3_produces_diagonal_matrix() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat3_diag", DVec2::new(200.0, 0.0));

    let v_id = designer.add_node("ivec3", DVec2::ZERO);
    set_node_data(
        &mut designer,
        v_id,
        IVec3Data {
            value: IVec3::new(2, 3, 5),
        },
    );
    designer.connect_nodes(v_id, 0, id, 0);
    designer.validate_active_network();

    let m = extract_imat3(evaluate(&designer, id));
    assert_eq!(m, [[2, 0, 0], [0, 3, 0], [0, 0, 5]]);
}

#[test]
fn default_imat3_diag_outputs_identity() {
    let mut designer = setup_designer();
    let id = designer.add_node("imat3_diag", DVec2::ZERO);
    designer.validate_active_network();

    let m = extract_imat3(evaluate(&designer, id));
    assert_eq!(m, [[1, 0, 0], [0, 1, 0], [0, 0, 1]]);
}

// ---------------------------------------------------------------------------
// mat3_rows / mat3_cols / mat3_diag — representative tests
// ---------------------------------------------------------------------------

#[test]
fn default_mat3_rows_outputs_identity() {
    let mut designer = setup_designer();
    let id = designer.add_node("mat3_rows", DVec2::ZERO);
    designer.validate_active_network();

    let m = extract_mat3(evaluate(&designer, id));
    assert!(m.abs_diff_eq(DMat3::IDENTITY, 1e-12));
}

#[test]
fn mat3_rows_with_stored_matrix_preserves_row_major_semantics() {
    // Row-major stored: [[1,2,3],[4,5,6],[7,8,9]]
    // Row-major semantics: m * (1,0,0) == (1,4,7) (the first column = the
    // x-components of the three rows, by the row-major matrix-vector rule
    // (M·v)[i] = Σ_j m[i][j]·v[j]).
    let mut designer = setup_designer();
    let id = designer.add_node("mat3_rows", DVec2::ZERO);
    set_node_data(
        &mut designer,
        id,
        Mat3RowsData {
            matrix: [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
        },
    );
    designer.validate_active_network();

    let m = extract_mat3(evaluate(&designer, id));

    // Verify via matrix-vector product with basis vectors.
    let e0 = m * DVec3::new(1.0, 0.0, 0.0);
    assert!((e0 - DVec3::new(1.0, 4.0, 7.0)).length() < 1e-12);
    let e1 = m * DVec3::new(0.0, 1.0, 0.0);
    assert!((e1 - DVec3::new(2.0, 5.0, 8.0)).length() < 1e-12);
    let e2 = m * DVec3::new(0.0, 0.0, 1.0);
    assert!((e2 - DVec3::new(3.0, 6.0, 9.0)).length() < 1e-12);
}

#[test]
fn mat3_cols_with_three_wired_vec3_inputs_produces_column_composed_matrix() {
    let mut designer = setup_designer();
    let id = designer.add_node("mat3_cols", DVec2::new(400.0, 0.0));

    let col_a_id = designer.add_node("vec3", DVec2::new(0.0, 0.0));
    let col_b_id = designer.add_node("vec3", DVec2::new(0.0, 100.0));
    let col_c_id = designer.add_node("vec3", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        col_a_id,
        Vec3Data {
            value: DVec3::new(1.0, 2.0, 3.0),
        },
    );
    set_node_data(
        &mut designer,
        col_b_id,
        Vec3Data {
            value: DVec3::new(4.0, 5.0, 6.0),
        },
    );
    set_node_data(
        &mut designer,
        col_c_id,
        Vec3Data {
            value: DVec3::new(7.0, 8.0, 9.0),
        },
    );
    designer.connect_nodes(col_a_id, 0, id, 0);
    designer.connect_nodes(col_b_id, 0, id, 1);
    designer.connect_nodes(col_c_id, 0, id, 2);
    designer.validate_active_network();

    let m = extract_mat3(evaluate(&designer, id));

    // Columns of the output must match the wired vectors.
    assert!((m.col(0) - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-12);
    assert!((m.col(1) - DVec3::new(4.0, 5.0, 6.0)).length() < 1e-12);
    assert!((m.col(2) - DVec3::new(7.0, 8.0, 9.0)).length() < 1e-12);
}

#[test]
fn mat3_diag_with_wired_vec3_produces_diagonal_matrix() {
    let mut designer = setup_designer();
    let id = designer.add_node("mat3_diag", DVec2::new(200.0, 0.0));

    let v_id = designer.add_node("vec3", DVec2::ZERO);
    set_node_data(
        &mut designer,
        v_id,
        Vec3Data {
            value: DVec3::new(2.0, 3.0, 5.0),
        },
    );
    designer.connect_nodes(v_id, 0, id, 0);
    designer.validate_active_network();

    let m = extract_mat3(evaluate(&designer, id));
    assert!((m.col(0) - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-12);
    assert!((m.col(1) - DVec3::new(0.0, 3.0, 0.0)).length() < 1e-12);
    assert!((m.col(2) - DVec3::new(0.0, 0.0, 5.0)).length() < 1e-12);
}

// ---------------------------------------------------------------------------
// NodeData serializer/loader roundtrip (phase-2 .cnnd roundtrip proxy)
// ---------------------------------------------------------------------------
//
// Full .cnnd roundtrip is exercised by the top-level cnnd_roundtrip_test when
// sample files contain these node types. Here we verify that each constructor
// node's NodeData round-trips through its generic saver/loader pair so the
// matrix state survives serialization with a non-default value.

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
fn imat3_rows_node_data_roundtrips_non_default_matrix() {
    let original = IMat3RowsData {
        matrix: [[2, 1, 0], [0, 2, 1], [1, 0, 2]],
    };
    let restored = roundtrip_node_data("imat3_rows", original.clone());
    let restored_ref = restored
        .as_ref()
        .as_any_ref()
        .downcast_ref::<IMat3RowsData>()
        .expect("downcast");
    assert_eq!(restored_ref.matrix, original.matrix);
}

#[test]
fn imat3_cols_node_data_roundtrips_non_default_matrix() {
    let original = IMat3ColsData {
        matrix: [[2, 1, 0], [0, 2, 1], [1, 0, 2]],
    };
    let restored = roundtrip_node_data("imat3_cols", original.clone());
    let restored_ref = restored
        .as_ref()
        .as_any_ref()
        .downcast_ref::<IMat3ColsData>()
        .expect("downcast");
    assert_eq!(restored_ref.matrix, original.matrix);
}

#[test]
fn imat3_diag_node_data_roundtrips_non_default_vector() {
    let original = IMat3DiagData {
        v: IVec3::new(2, 3, 5),
    };
    let restored = roundtrip_node_data("imat3_diag", original.clone());
    let restored_ref = restored
        .as_ref()
        .as_any_ref()
        .downcast_ref::<IMat3DiagData>()
        .expect("downcast");
    assert_eq!(restored_ref.v, original.v);
}

#[test]
fn mat3_rows_node_data_roundtrips_non_default_matrix() {
    let original = Mat3RowsData {
        matrix: [[2.5, 1.0, 0.0], [0.0, 2.5, 1.0], [1.0, 0.0, 2.5]],
    };
    let restored = roundtrip_node_data("mat3_rows", original.clone());
    let restored_ref = restored
        .as_ref()
        .as_any_ref()
        .downcast_ref::<Mat3RowsData>()
        .expect("downcast");
    assert_eq!(restored_ref.matrix, original.matrix);
}

#[test]
fn mat3_cols_node_data_roundtrips_non_default_matrix() {
    let original = Mat3ColsData {
        matrix: [[2.5, 1.0, 0.0], [0.0, 2.5, 1.0], [1.0, 0.0, 2.5]],
    };
    let restored = roundtrip_node_data("mat3_cols", original.clone());
    let restored_ref = restored
        .as_ref()
        .as_any_ref()
        .downcast_ref::<Mat3ColsData>()
        .expect("downcast");
    assert_eq!(restored_ref.matrix, original.matrix);
}

#[test]
fn mat3_diag_node_data_roundtrips_non_default_vector() {
    let original = Mat3DiagData {
        v: DVec3::new(2.5, 3.5, 5.5),
    };
    let restored = roundtrip_node_data("mat3_diag", original.clone());
    let restored_ref = restored
        .as_ref()
        .as_any_ref()
        .downcast_ref::<Mat3DiagData>()
        .expect("downcast");
    assert_eq!(restored_ref.v, original.v);
}

// ---------------------------------------------------------------------------
// get_node_type() registration — simple integrity checks
// ---------------------------------------------------------------------------

#[test]
fn all_six_constructors_registered_with_expected_types() {
    let registry = NodeTypeRegistry::new();

    let cases: &[(&str, &str)] = &[
        ("imat3_rows", "IMat3"),
        ("imat3_cols", "IMat3"),
        ("imat3_diag", "IMat3"),
        ("mat3_rows", "Mat3"),
        ("mat3_cols", "Mat3"),
        ("mat3_diag", "Mat3"),
    ];
    for (name, expected_output) in cases {
        let nt = registry
            .built_in_node_types
            .get(*name)
            .unwrap_or_else(|| panic!("{} not registered", name));
        assert_eq!(
            nt.output_type().to_string(),
            *expected_output,
            "{} output type",
            name
        );
        assert!(nt.public, "{} should be public", name);
    }
}
