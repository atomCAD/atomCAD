//! Tests for the `cuboid` node's `subdivision` parameter (issue #395).
//!
//! `subdivision` refines the lattice grid: both `min_corner` and `extent` are
//! measured in units of 1/subdivision of a unit cell. So a subdivided cuboid is
//! geometrically identical to one authored at the finer scale with
//! `subdivision = 1`. These tests probe the emitted geo_tree SDF to confirm both
//! the corner and the extent scale (mechadense's clarification on the issue).

use glam::f64::{DVec2, DVec3};
use glam::i32::IVec3;
use rust_lib_flutter_cad::geo_tree::implicit_geometry::ImplicitGeometry3D;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    BlueprintData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::cuboid::CuboidData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

const NET: &str = "cuboid_sub_test";

fn setup() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));
    designer
}

/// Adds a cuboid node with the given data and returns its id.
fn add_cuboid(designer: &mut StructureDesigner, data: CuboidData) -> u64 {
    let id = designer.add_node("cuboid", DVec2::ZERO);
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(NET).unwrap();
    network.nodes.get_mut(&id).unwrap().data = Box::new(data);
    designer.validate_active_network();
    id
}

fn evaluate(designer: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn extract_blueprint(result: NetworkResult) -> BlueprintData {
    match result {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("expected Blueprint, got Error: {}", e),
        other => panic!("expected Blueprint, got {:?}", other.infer_data_type()),
    }
}

/// A spread of probe points covering inside, surface, and outside the shapes.
fn probe_points() -> Vec<DVec3> {
    let mut pts = Vec::new();
    for x in [-2.0, 0.5, 1.0, 1.5, 2.5, 3.0, 4.0, 6.0] {
        for y in [-1.0, 0.0, 2.0, 3.5, 5.0] {
            for z in [-3.0, 0.5, 3.0, 7.0] {
                pts.push(DVec3::new(x, y, z));
            }
        }
    }
    pts
}

/// A cuboid at `subdivision = k` with coordinates scaled by `k` must be
/// geometrically identical (same SDF everywhere) to the same cuboid at
/// `subdivision = 1`. This verifies that BOTH `min_corner` and `extent` are
/// divided by the subdivision.
#[test]
fn subdivided_cuboid_matches_finer_scale_cuboid() {
    let mut designer = setup();

    // subdivision=2, coords scaled by 2 → occupies lattice box [1..2]^3-ish region.
    let subdivided = add_cuboid(
        &mut designer,
        CuboidData {
            min_corner: IVec3::new(2, 4, 6),
            extent: IVec3::new(2, 2, 2),
            subdivision: 2,
        },
    );
    // subdivision=1 reference at the finer scale: (2,4,6)/2 = (1,2,3), (2,2,2)/2 = (1,1,1).
    let reference = add_cuboid(
        &mut designer,
        CuboidData {
            min_corner: IVec3::new(1, 2, 3),
            extent: IVec3::new(1, 1, 1),
            subdivision: 1,
        },
    );

    let sub_geo = extract_blueprint(evaluate(&designer, subdivided)).geo_tree_root;
    let ref_geo = extract_blueprint(evaluate(&designer, reference)).geo_tree_root;

    for p in probe_points() {
        let a = sub_geo.implicit_eval_3d(&p);
        let b = ref_geo.implicit_eval_3d(&p);
        assert!(
            (a - b).abs() < 1e-9,
            "SDF mismatch at {:?}: subdivided={}, reference={}",
            p,
            a,
            b
        );
    }
}

/// Increasing the subdivision (with the same integer coords) shrinks the cuboid:
/// a point that lies inside the un-subdivided cuboid must fall outside the same
/// cuboid at `subdivision = 4`.
#[test]
fn higher_subdivision_shrinks_cuboid() {
    let mut designer = setup();

    let whole = add_cuboid(
        &mut designer,
        CuboidData {
            min_corner: IVec3::new(0, 0, 0),
            extent: IVec3::new(2, 2, 2),
            subdivision: 1,
        },
    );
    let shrunk = add_cuboid(
        &mut designer,
        CuboidData {
            min_corner: IVec3::new(0, 0, 0),
            extent: IVec3::new(2, 2, 2),
            subdivision: 4,
        },
    );

    let whole_geo = extract_blueprint(evaluate(&designer, whole)).geo_tree_root;
    let shrunk_geo = extract_blueprint(evaluate(&designer, shrunk)).geo_tree_root;

    // Real-space point corresponding to lattice coord (1,1,1): well inside the
    // whole cuboid (extent 2), but outside the quarter-scale one (extent 0.5).
    let structure = rust_lib_flutter_cad::crystolecule::structure::Structure::diamond();
    let inside_pt = structure
        .lattice_vecs
        .dvec3_lattice_to_real(&DVec3::new(1.0, 1.0, 1.0));

    assert!(
        whole_geo.implicit_eval_3d(&inside_pt) < 0.0,
        "point should be inside the un-subdivided cuboid"
    );
    assert!(
        shrunk_geo.implicit_eval_3d(&inside_pt) > 0.0,
        "point should be outside the subdivision=4 cuboid"
    );
}

/// `subdivision` is clamped to a minimum of 1 at eval, so 0 or negative values
/// behave like 1 (no divide-by-zero, no inversion).
#[test]
fn nonpositive_subdivision_clamps_to_one() {
    let mut designer = setup();

    let clamped = add_cuboid(
        &mut designer,
        CuboidData {
            min_corner: IVec3::new(1, 2, 3),
            extent: IVec3::new(1, 1, 1),
            subdivision: 0,
        },
    );
    let one = add_cuboid(
        &mut designer,
        CuboidData {
            min_corner: IVec3::new(1, 2, 3),
            extent: IVec3::new(1, 1, 1),
            subdivision: 1,
        },
    );

    let clamped_geo = extract_blueprint(evaluate(&designer, clamped)).geo_tree_root;
    let one_geo = extract_blueprint(evaluate(&designer, one)).geo_tree_root;

    for p in probe_points() {
        let a = clamped_geo.implicit_eval_3d(&p);
        let b = one_geo.implicit_eval_3d(&p);
        assert!(
            (a - b).abs() < 1e-9,
            "SDF mismatch at {:?}: clamped={}, one={}",
            p,
            a,
            b
        );
    }
}
