//! `sample_field` node tests (`doc/design_scalar_fields.md` P4).
//!
//! The field's own sampling maths is covered in `atomcad-crystolecule`'s
//! `field` tests. What is tested here is the *node*: that it reads a
//! `ScalarField` off pin 0 and a point off pin 1 and hands back the value as a
//! `Float`, and — the rule this node exists to keep honest — that a point
//! outside the file's box comes back as exactly `0.0` rather than an error.
//!
//! Every expected value is checkable by eye, which is the whole point of the
//! asymmetric ramp fixture: `value(i, j, k) = 100i + 10j + k` on a 3x4x5 grid
//! with 1 Å spacing and its origin at the first sample, so sample `(i, j, k)`
//! sits at Ångström position `(i, j, k)`.

use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::nodes::import_cube::{ImportCubeData, LoadedCube};
use atomcad_structure_designer::nodes::vec3::Vec3Data;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::{DVec2, DVec3};

fn cube_fixture(name: &str) -> String {
    fixture_path_str(&format!("cube/{}", name))
}

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn evaluate_pin(designer: &StructureDesigner, node_id: u64, pin_index: i32) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&stack, node_id, pin_index, registry, false, &mut context)
}

/// An `import_cube` node with its payload already loaded, which is the state
/// the import action leaves the node in.
fn add_loaded_import_cube_node(designer: &mut StructureDesigner, file_path: &str) -> u64 {
    let node_id = designer.add_node("import_cube", DVec2::new(-400.0, 0.0));
    let cube = atomcad_crystolecule::io::cube_loader::load_cube(file_path, true)
        .expect("fixture should parse");
    let loaded = LoadedCube::from_cube_file(cube).expect("fixture should carry a field");

    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<ImportCubeData>()
        .expect("import_cube node should carry ImportCubeData");
    data.file_name = Some(file_path.to_string());
    data.loaded = Some(loaded);

    node_id
}

fn add_vec3_node(designer: &mut StructureDesigner, value: DVec3) -> u64 {
    let node_id = designer.add_node("vec3", DVec2::new(-400.0, 200.0));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<Vec3Data>()
        .expect("vec3 node should carry Vec3Data");
    data.value = value;
    node_id
}

/// The whole P4 pipeline: `import_cube.field` → `sample_field.field`, a `vec3`
/// → `sample_field.point`. Returns the `sample_field` node's id.
fn build_pipeline(designer: &mut StructureDesigner, fixture: &str, point: DVec3) -> u64 {
    let cube_id = add_loaded_import_cube_node(designer, &cube_fixture(fixture));
    let vec3_id = add_vec3_node(designer, point);
    let sample_id = designer.add_node("sample_field", DVec2::new(0.0, 0.0));
    designer.connect_nodes(cube_id, 0, sample_id, 0);
    designer.connect_nodes(vec3_id, 0, sample_id, 1);
    sample_id
}

fn sample_ramp_at(point: DVec3) -> f64 {
    let mut designer = setup_designer();
    let sample_id = build_pipeline(&mut designer, "ramp_3x4x5.cube", point);
    match evaluate_pin(&designer, sample_id, 0) {
        NetworkResult::Float(value) => value,
        other => panic!(
            "sample_field should output a Float, got {:?}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Exact grid points
// ============================================================================

#[test]
fn sampling_every_exact_grid_point_returns_its_own_index_code() {
    // The asymmetric ramp on a 3x4x5 grid: any axis transposition, mirroring or
    // off-by-one in the node's plumbing shows up as a wrong index code here.
    // Build the network once and move the `vec3` node's value between points,
    // so the whole 60-point sweep costs one cube parse.
    let mut designer = setup_designer();
    let cube_id = add_loaded_import_cube_node(&mut designer, &cube_fixture("ramp_3x4x5.cube"));
    let vec3_id = add_vec3_node(&mut designer, DVec3::ZERO);
    let sample_id = designer.add_node("sample_field", DVec2::new(0.0, 0.0));
    designer.connect_nodes(cube_id, 0, sample_id, 0);
    designer.connect_nodes(vec3_id, 0, sample_id, 1);

    for i in 0..3 {
        for j in 0..4 {
            for k in 0..5 {
                {
                    let network = designer
                        .node_type_registry
                        .node_networks
                        .get_mut("test")
                        .unwrap();
                    let node = network.nodes.get_mut(&vec3_id).unwrap();
                    let data = node.data.as_any_mut().downcast_mut::<Vec3Data>().unwrap();
                    data.value = DVec3::new(i as f64, j as f64, k as f64);
                }

                let expected = (100 * i + 10 * j + k) as f64;
                match evaluate_pin(&designer, sample_id, 0) {
                    NetworkResult::Float(value) => assert!(
                        (value - expected).abs() < 1e-4,
                        "sample at ({i}, {j}, {k}) should be {expected}, got {value}"
                    ),
                    other => panic!(
                        "sample_field should output a Float, got {:?}",
                        other.to_display_string()
                    ),
                }
            }
        }
    }
}

// ============================================================================
// Interpolation
// ============================================================================

#[test]
fn sampling_a_midpoint_returns_the_average_of_its_two_neighbours() {
    // Halfway between (1,2,3) = 123 and (1,2,4) = 124 along the fastest axis.
    let value = sample_ramp_at(DVec3::new(1.0, 2.0, 3.5));
    assert!(
        (value - 123.5).abs() < 1e-4,
        "midpoint should be the average 123.5, got {value}"
    );

    // And along the slowest axis: halfway between (0,1,2) = 12 and (1,1,2) = 112.
    let value = sample_ramp_at(DVec3::new(0.5, 1.0, 2.0));
    assert!(
        (value - 62.0).abs() < 1e-4,
        "midpoint should be the average 62.0, got {value}"
    );
}

#[test]
fn sampling_a_cell_centre_returns_the_average_of_all_eight_corners() {
    // The centre of the cell whose corner is (0,0,0): the eight corner values
    // are 0, 1, 10, 11, 100, 101, 110, 111, averaging to 55.5.
    let value = sample_ramp_at(DVec3::new(0.5, 0.5, 0.5));
    assert!(
        (value - 55.5).abs() < 1e-4,
        "cell centre should be the trilinear average 55.5, got {value}"
    );
}

// ============================================================================
// Out of bounds — the rule this node exists to keep honest
// ============================================================================

#[test]
fn sampling_outside_the_box_returns_zero_and_not_an_error() {
    // The ramp's box runs 0..2 x 0..3 x 0..4 Å, so each of these is outside on
    // exactly one face — including the "just past the last plane" cases, which
    // is where a fencepost error would hide.
    for point in [
        DVec3::new(-0.5, 1.0, 1.0),
        DVec3::new(2.5, 1.0, 1.0),
        DVec3::new(1.0, -0.5, 1.0),
        DVec3::new(1.0, 3.5, 1.0),
        DVec3::new(1.0, 1.0, -0.5),
        DVec3::new(1.0, 1.0, 4.5),
        DVec3::new(1000.0, 1000.0, 1000.0),
    ] {
        let mut designer = setup_designer();
        let sample_id = build_pipeline(&mut designer, "ramp_3x4x5.cube", point);
        let result = evaluate_pin(&designer, sample_id, 0);
        assert!(
            !result.is_error(),
            "an out-of-bounds point must not be an error: {point:?}"
        );
        match result {
            NetworkResult::Float(value) => assert_eq!(
                value, 0.0,
                "out-of-bounds sample at {point:?} should be exactly 0.0"
            ),
            other => panic!(
                "sample_field should output a Float, got {:?}",
                other.to_display_string()
            ),
        }
    }
}

#[test]
fn the_outermost_sample_planes_are_still_inside() {
    // The box is *through* the outermost sample points (node-centered, not
    // extended by half a voxel), so the far corner is in-bounds and carries its
    // own index code rather than falling off the edge to 0.0.
    let value = sample_ramp_at(DVec3::new(2.0, 3.0, 4.0));
    assert!(
        (value - 234.0).abs() < 1e-4,
        "the far corner sample should be 234, got {value}"
    );
}

// ============================================================================
// Signed data survives the node
// ============================================================================

#[test]
fn negative_field_values_pass_through_unclamped() {
    // The synthetic 2p_z fixture is signed, with its nodal plane at z = 0 and
    // its oxygen at the origin. Sampling either side must give values of
    // opposite sign — a node that clamped or took a magnitude would fail here.
    let mut designer = setup_designer();
    let cube_id = add_loaded_import_cube_node(&mut designer, &cube_fixture("p2z_11x11x11.cube"));
    let vec3_id = add_vec3_node(&mut designer, DVec3::new(0.0, 0.0, 0.8));
    let sample_id = designer.add_node("sample_field", DVec2::new(0.0, 0.0));
    designer.connect_nodes(cube_id, 0, sample_id, 0);
    designer.connect_nodes(vec3_id, 0, sample_id, 1);

    let above = match evaluate_pin(&designer, sample_id, 0) {
        NetworkResult::Float(v) => v,
        other => panic!("expected a Float, got {:?}", other.to_display_string()),
    };

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test")
            .unwrap();
        let node = network.nodes.get_mut(&vec3_id).unwrap();
        let data = node.data.as_any_mut().downcast_mut::<Vec3Data>().unwrap();
        data.value = DVec3::new(0.0, 0.0, -0.8);
    }
    let below = match evaluate_pin(&designer, sample_id, 0) {
        NetworkResult::Float(v) => v,
        other => panic!("expected a Float, got {:?}", other.to_display_string()),
    };

    assert!(
        above > 0.0,
        "above the nodal plane should be positive: {above}"
    );
    assert!(
        below < 0.0,
        "below the nodal plane should be negative: {below}"
    );
    assert!(
        (above + below).abs() < 1e-4,
        "the 2p_z is antisymmetric about z = 0: {above} vs {below}"
    );
}

// ============================================================================
// Missing and errored inputs
// ============================================================================

#[test]
fn an_unwired_input_is_a_missing_input_error() {
    // Both pins are required, so each unwired pin errors on its own.
    let mut designer = setup_designer();
    let sample_id = designer.add_node("sample_field", DVec2::new(0.0, 0.0));
    assert!(
        evaluate_pin(&designer, sample_id, 0).is_error(),
        "no inputs at all should error"
    );

    let cube_id = add_loaded_import_cube_node(&mut designer, &cube_fixture("ramp_3x4x5.cube"));
    designer.connect_nodes(cube_id, 0, sample_id, 0);
    assert!(
        evaluate_pin(&designer, sample_id, 0).is_error(),
        "a missing point should still error"
    );

    let vec3_id = add_vec3_node(&mut designer, DVec3::new(1.0, 2.0, 3.0));
    designer.connect_nodes(vec3_id, 0, sample_id, 1);
    match evaluate_pin(&designer, sample_id, 0) {
        NetworkResult::Float(value) => assert!((value - 123.0).abs() < 1e-4),
        other => panic!(
            "with both inputs wired it should sample, got {:?}",
            other.to_display_string()
        ),
    }
}

#[test]
fn an_upstream_error_is_forwarded_verbatim() {
    // An `import_cube` with no file errors; `sample_field` must forward that
    // error rather than replace it with a type complaint of its own.
    let mut designer = setup_designer();
    let cube_id = designer.add_node("import_cube", DVec2::new(-400.0, 0.0));
    let vec3_id = add_vec3_node(&mut designer, DVec3::new(1.0, 2.0, 3.0));
    let sample_id = designer.add_node("sample_field", DVec2::new(0.0, 0.0));
    designer.connect_nodes(cube_id, 0, sample_id, 0);
    designer.connect_nodes(vec3_id, 0, sample_id, 1);

    match evaluate_pin(&designer, sample_id, 0) {
        NetworkResult::Error(message) => assert!(
            message.contains("No cube file imported"),
            "the upstream cause should survive, got: {message}"
        ),
        other => panic!(
            "an errored field input should error, got {:?}",
            other.to_display_string()
        ),
    }
}
