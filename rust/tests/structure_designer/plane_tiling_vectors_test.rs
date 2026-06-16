//! Phase 3 tests for the `plane_tiling_vectors` helper node.
//!
//! The node turns a Miller-indexed `DrawingPlane` (supplies u_axis/v_axis) plus
//! a 2×2 integer superlattice into the `Array[IVec3]` tiling vectors consumed by
//! `patch_build.tiling_vectors`. Each row of the superlattice is one tiling
//! vector expressed as an integer combination of u and v.

use glam::f64::DVec2;
use glam::i32::{IVec2, IVec3};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::imat2_rows::IMat2RowsData;
use rust_lib_flutter_cad::structure_designer::nodes::plane_tiling_vectors::PlaneTilingVectorsData;
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

/// Pulls the plane's in-plane basis vectors so expectations stay robust to the
/// deterministic-but-non-obvious axis choice the DrawingPlane makes.
fn plane_axes(designer: &StructureDesigner, plane_node_id: u64) -> (IVec3, IVec3) {
    match evaluate(designer, plane_node_id) {
        NetworkResult::DrawingPlane(p) => (p.u_axis, p.v_axis),
        other => panic!("Expected DrawingPlane, got {}", other.to_display_string()),
    }
}

fn extract_ivec3_array(result: NetworkResult) -> Vec<IVec3> {
    match result {
        NetworkResult::Array(items) => items
            .into_iter()
            .map(|item| match item {
                NetworkResult::IVec3(v) => v,
                other => panic!("Expected IVec3 element, got {}", other.to_display_string()),
            })
            .collect(),
        other => panic!("Expected Array result, got {}", other.to_display_string()),
    }
}

/// Builds `plane_tiling_vectors` wired to a default (001) `drawing_plane`, with
/// the stored superlattice set to `matrix`. Returns (designer, node_id, u, v).
fn setup_with_stored_matrix(matrix: [[i32; 2]; 2]) -> (StructureDesigner, u64, IVec3, IVec3) {
    let mut designer = setup_designer();
    let plane_id = designer.add_node("drawing_plane", DVec2::ZERO);
    let ptv_id = designer.add_node("plane_tiling_vectors", DVec2::new(300.0, 0.0));
    set_node_data(&mut designer, ptv_id, PlaneTilingVectorsData { matrix });
    designer.connect_nodes(plane_id, 0, ptv_id, 0);
    designer.validate_active_network();

    let (u, v) = plane_axes(&designer, plane_id);
    (designer, ptv_id, u, v)
}

#[test]
fn identity_superlattice_yields_u_and_v() {
    let (designer, ptv_id, u, v) = setup_with_stored_matrix([[1, 0], [0, 1]]);
    let vecs = extract_ivec3_array(evaluate(&designer, ptv_id));
    assert_eq!(vecs, vec![u, v]);
}

#[test]
fn diagonal_superlattice_scales_each_axis() {
    // rows (2,0),(0,1) → [2u, v]
    let (designer, ptv_id, u, v) = setup_with_stored_matrix([[2, 0], [0, 1]]);
    let vecs = extract_ivec3_array(evaluate(&designer, ptv_id));
    assert_eq!(vecs, vec![u * 2, v]);
}

#[test]
fn nondiagonal_superlattice_combines_axes() {
    // √3×√3 R30°: rows (2,1),(-1,1) → [2u + v, -u + v]
    let (designer, ptv_id, u, v) = setup_with_stored_matrix([[2, 1], [-1, 1]]);
    let vecs = extract_ivec3_array(evaluate(&designer, ptv_id));
    assert_eq!(vecs, vec![u * 2 + v, u * -1 + v]);
}

#[test]
fn singular_superlattice_is_not_an_error() {
    // det == 0 (dependent rows). The node must still emit two (dependent)
    // vectors and defer the linear-independence check to patch_build.
    let (designer, ptv_id, u, v) = setup_with_stored_matrix([[1, 1], [2, 2]]);
    let vecs = extract_ivec3_array(evaluate(&designer, ptv_id));
    assert_eq!(vecs, vec![u + v, u * 2 + v * 2]);
}

#[test]
fn unconnected_plane_is_an_error() {
    let mut designer = setup_designer();
    let ptv_id = designer.add_node("plane_tiling_vectors", DVec2::ZERO);
    designer.validate_active_network();

    match evaluate(&designer, ptv_id) {
        NetworkResult::Error(_) => {}
        other => panic!(
            "Expected Error for missing plane, got {}",
            other.to_display_string()
        ),
    }
}

#[test]
fn superlattice_pin_overrides_stored_matrix() {
    let mut designer = setup_designer();
    let plane_id = designer.add_node("drawing_plane", DVec2::ZERO);
    let ptv_id = designer.add_node("plane_tiling_vectors", DVec2::new(300.0, 0.0));
    // Stored matrix is some non-identity value that must be ignored once the
    // superlattice pin is wired.
    set_node_data(
        &mut designer,
        ptv_id,
        PlaneTilingVectorsData {
            matrix: [[9, 9], [9, 9]],
        },
    );

    let mat_id = designer.add_node("imat2_rows", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        mat_id,
        IMat2RowsData {
            // rows (3,0),(0,2)
            matrix: [[3, 0], [0, 2]],
        },
    );

    designer.connect_nodes(plane_id, 0, ptv_id, 0);
    designer.connect_nodes(mat_id, 0, ptv_id, 1);
    designer.validate_active_network();

    let (u, v) = plane_axes(&designer, plane_id);
    let vecs = extract_ivec3_array(evaluate(&designer, ptv_id));
    assert_eq!(vecs, vec![u * 3, v * 2]);
}

// ---------------------------------------------------------------------------
// Subtitle + text properties
// ---------------------------------------------------------------------------

#[test]
fn subtitle_shows_determinant_when_pin_unconnected() {
    let data = PlaneTilingVectorsData {
        matrix: [[2, 1], [-1, 1]], // det = 3
    };
    let connected = std::collections::HashSet::new();
    assert_eq!(data.get_subtitle(&connected), Some("det = 3".to_string()));
}

#[test]
fn subtitle_is_unknown_when_superlattice_connected() {
    let data = PlaneTilingVectorsData::default();
    let mut connected = std::collections::HashSet::new();
    connected.insert("superlattice".to_string());
    assert_eq!(data.get_subtitle(&connected), Some("det = ?".to_string()));
}

#[test]
fn text_properties_roundtrip_rows() {
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

    let mut data = PlaneTilingVectorsData::default();
    let mut props = std::collections::HashMap::new();
    props.insert("a".to_string(), TextValue::IVec2(IVec2::new(2, 1)));
    props.insert("b".to_string(), TextValue::IVec2(IVec2::new(-1, 1)));
    data.set_text_properties(&props).unwrap();
    assert_eq!(data.matrix, [[2, 1], [-1, 1]]);

    let out = data.get_text_properties();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].0, "a");
    assert_eq!(out[1].0, "b");
}

#[test]
fn registered_with_array_ivec3_output() {
    let registry = NodeTypeRegistry::new();
    let nt = registry
        .built_in_node_types
        .get("plane_tiling_vectors")
        .expect("plane_tiling_vectors not registered");
    assert_eq!(nt.output_type().to_string(), "[IVec3]");
    assert!(nt.public);
}
