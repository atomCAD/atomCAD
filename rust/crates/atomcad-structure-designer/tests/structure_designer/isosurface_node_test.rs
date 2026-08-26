//! `isosurface` node tests (P3 of `doc/design_isosurface_node.md`).
//!
//! The marching-cubes maths is covered exhaustively in `atomcad-display`'s
//! `isosurface_*` suites. What is tested here is the *node* and the *display
//! conversion*:
//!
//! - `eval` packages the wired field, the level (stored or wired) and the paint
//!   into a `NetworkResult::Isosurface`, and rejects a non-positive level;
//! - a level above the field's range is **silent** — a value, an empty surface
//!   and no error entry, which is the design's deliberate choice and not an
//!   oversight worth "fixing" later;
//! - the display conversion runs the extractor and, when the cell budget is
//!   breached, reports through `context.node_errors` at the **scoped**
//!   `NodeRef` without overwriting an error `eval` already recorded.
//!
//! `StructureDesigner::new()` loads the **real** user preferences file, so
//! every test that depends on an extraction preference pins it explicitly
//! rather than trusting the default.

use std::collections::HashSet;

use atomcad_crystolecule::field::{Colormap, IsosurfaceColoring, ScalarField};
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::nodes::closure::{ClosureData, ClosureKind};
use atomcad_structure_designer::nodes::float::FloatData;
use atomcad_structure_designer::nodes::import_cube::{ImportCubeData, LoadedCube};
use atomcad_structure_designer::nodes::isosurface::IsosurfaceNodeData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_scene::NodeOutput;
use atomcad_test_support::fixture_path_str;
use glam::Vec3;
use glam::f64::{DVec2, DVec3};

// ============================================================================
// Helpers
// ============================================================================

fn cube_fixture(name: &str) -> String {
    fixture_path_str(&format!("cube/{}", name))
}

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

/// Pin the three extraction preferences so a test never depends on whatever
/// the developer's `preferences.json` happens to say.
fn set_extraction_prefs(
    designer: &mut StructureDesigner,
    quality_multiplier: f64,
    cell_budget: usize,
) {
    let mut prefs = designer.preferences.clone();
    prefs
        .geometry_visualization_preferences
        .isosurface_quality_multiplier = quality_multiplier;
    prefs
        .geometry_visualization_preferences
        .isosurface_fallback_spacing = 0.15;
    prefs
        .geometry_visualization_preferences
        .isosurface_cell_budget = cell_budget;
    designer.set_preferences(prefs);
}

fn full_refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

/// An `import_cube` node with its payload already loaded — the state the import
/// action leaves the node in.
fn add_loaded_import_cube_node(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    file_path: &str,
) -> u64 {
    let node_id =
        designer.add_node_scoped(scope_path, "import_cube", DVec2::new(-400.0, 0.0), None);
    let cube = atomcad_crystolecule::io::cube_loader::load_cube(file_path, true)
        .expect("fixture should parse");
    let loaded = LoadedCube::from_cube_file(cube).expect("fixture should carry a field");

    let mut data = ImportCubeData::new();
    data.file_name = Some(file_path.to_string());
    data.loaded = Some(loaded);
    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(data));
    node_id
}

fn add_isosurface_node(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    data: IsosurfaceNodeData,
) -> u64 {
    let node_id = designer.add_node_scoped(scope_path, "isosurface", DVec2::new(0.0, 0.0), None);
    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(data));
    node_id
}

fn evaluate_pin(designer: &StructureDesigner, node_id: u64, pin_index: i32) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&stack, node_id, pin_index, registry, false, &mut context)
}

// ============================================================================
// eval
// ============================================================================

#[test]
fn eval_packages_the_wired_field_and_the_stored_level() {
    let mut designer = setup_designer();
    let cube_id = add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
    let iso_id = add_isosurface_node(
        &mut designer,
        &[],
        IsosurfaceNodeData {
            level: 0.031,
            ..Default::default()
        },
    );
    designer.connect_nodes(cube_id, 0, iso_id, 0);

    match evaluate_pin(&designer, iso_id, 0) {
        NetworkResult::Isosurface(data) => {
            assert_eq!(data.level, 0.031, "the stored level reaches the value");
            assert!(
                data.field.native_grid().is_some(),
                "the wired sampled field reaches the value"
            );
            assert!(
                matches!(
                    data.coloring,
                    atomcad_crystolecule::field::IsosurfaceColoring::Phase { .. }
                ),
                "with no color field wired the coloring is per-sign"
            );
        }
        other => panic!(
            "isosurface should output an Isosurface, got {}",
            other.to_display_string()
        ),
    }
}

#[test]
fn a_wired_level_pin_overrides_the_stored_property() {
    let mut designer = setup_designer();
    let cube_id = add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
    let iso_id = add_isosurface_node(
        &mut designer,
        &[],
        IsosurfaceNodeData {
            level: 0.02,
            ..Default::default()
        },
    );
    designer.connect_nodes(cube_id, 0, iso_id, 0);

    // Pin 2 is `level`; pin 1 is `color_field`.
    let float_id = designer.add_node("float", DVec2::new(-400.0, 200.0));
    designer.set_node_network_data(float_id, Box::new(FloatData { value: 0.005 }));
    designer.connect_nodes(float_id, 0, iso_id, 2);

    match evaluate_pin(&designer, iso_id, 0) {
        NetworkResult::Isosurface(data) => assert_eq!(
            data.level, 0.005,
            "the wired level wins over the stored property"
        ),
        other => panic!("expected an Isosurface, got {}", other.to_display_string()),
    }
}

#[test]
fn a_non_positive_level_is_an_evaluation_error() {
    for level in [0.0, -0.02] {
        let mut designer = setup_designer();
        let cube_id =
            add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
        let iso_id = add_isosurface_node(
            &mut designer,
            &[],
            IsosurfaceNodeData {
                level,
                ..Default::default()
            },
        );
        designer.connect_nodes(cube_id, 0, iso_id, 0);

        match evaluate_pin(&designer, iso_id, 0) {
            NetworkResult::Error(message) => assert!(
                message.contains("level must be greater than 0"),
                "the message should say why, got: {message}"
            ),
            other => panic!(
                "level {level} must be rejected, got {}",
                other.to_display_string()
            ),
        }
    }
}

// ----------------------------------------------------------------------------
// color_field (P5)
// ----------------------------------------------------------------------------

#[test]
fn a_wired_color_field_selects_the_field_arm_and_carries_the_stored_domain() {
    let mut designer = setup_designer();
    let cube_id = add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
    let color_id =
        add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("ramp_3x4x5.cube"));
    let iso_id = add_isosurface_node(
        &mut designer,
        &[],
        IsosurfaceNodeData {
            color_min: -0.031,
            color_max: 0.062,
            colormap: Colormap::BlueWhiteRed,
            ..Default::default()
        },
    );
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    // Pin 1 is `color_field`.
    designer.connect_nodes(color_id, 0, iso_id, 1);

    match evaluate_pin(&designer, iso_id, 0) {
        NetworkResult::Isosurface(data) => match data.coloring {
            IsosurfaceColoring::Field {
                range,
                colormap,
                field,
            } => {
                assert_eq!(
                    range,
                    (-0.031, 0.062),
                    "the stored domain reaches the value unnarrowed — it is compared                      against f64 field values"
                );
                assert_eq!(colormap, Colormap::BlueWhiteRed);
                assert_eq!(
                    field.value_range(),
                    Some((0.0, 234.0)),
                    "the *color* field is the one on pin 1, not the surface field"
                );
            }
            IsosurfaceColoring::Phase { .. } => {
                panic!("a wired color_field must select the Field arm")
            }
        },
        other => panic!("expected an Isosurface, got {}", other.to_display_string()),
    }
}

/// The other direction, which is the one that would regress silently: a project
/// saved with a colormap domain must stop painting by it once the wire is gone.
#[test]
fn removing_the_color_field_wire_reverts_to_phase() {
    let mut designer = setup_designer();
    let cube_id = add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
    let color_id =
        add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("ramp_3x4x5.cube"));
    let iso_id = add_isosurface_node(&mut designer, &[], IsosurfaceNodeData::default());
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    designer.connect_nodes(color_id, 0, iso_id, 1);

    assert!(
        matches!(
            evaluate_pin(&designer, iso_id, 0),
            NetworkResult::Isosurface(data)
                if matches!(data.coloring, IsosurfaceColoring::Field { .. })
        ),
        "precondition: a wired color_field means the Field arm"
    );

    assert!(designer.select_wire(color_id, 0, iso_id, 1));
    designer.delete_selected();

    match evaluate_pin(&designer, iso_id, 0) {
        NetworkResult::Isosurface(data) => {
            let defaults = IsosurfaceNodeData::default();
            assert!(
                matches!(
                    data.coloring,
                    IsosurfaceColoring::Phase { positive, negative }
                        if positive == defaults.positive_color.as_vec3()
                            && negative == defaults.negative_color.as_vec3()
                ),
                "an unwired color_field paints per sign, with the stored phase colors"
            );
        }
        other => panic!("expected an Isosurface, got {}", other.to_display_string()),
    }
}

/// A domain entered the wrong way round is **not** an error. It reaches the
/// value verbatim and `sample_colormap` maps it to the ramp's midpoint — which
/// is the state a user is in halfway through typing a negative minimum, where a
/// red badge would be noise rather than information.
#[test]
fn an_inverted_color_domain_is_carried_rather_than_rejected() {
    let mut designer = setup_designer();
    let cube_id = add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
    let color_id =
        add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("ramp_3x4x5.cube"));
    let iso_id = add_isosurface_node(
        &mut designer,
        &[],
        IsosurfaceNodeData {
            color_min: 0.05,
            color_max: -0.05,
            ..Default::default()
        },
    );
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    designer.connect_nodes(color_id, 0, iso_id, 1);

    match evaluate_pin(&designer, iso_id, 0) {
        NetworkResult::Isosurface(data) => assert!(
            matches!(
                data.coloring,
                IsosurfaceColoring::Field { range, .. } if range == (0.05, -0.05)
            ),
            "the inverted domain is carried, not normalized and not rejected"
        ),
        other => panic!("expected an Isosurface, got {}", other.to_display_string()),
    }
}

#[test]
fn an_unwired_field_is_an_evaluation_error() {
    let mut designer = setup_designer();
    let iso_id = add_isosurface_node(&mut designer, &[], IsosurfaceNodeData::default());

    assert!(
        evaluate_pin(&designer, iso_id, 0).is_error(),
        "the field input is required"
    );
}

/// The design's deliberate silence: a level nothing in the field reaches is a
/// *legal* value producing an *empty* surface, not an error. The value range is
/// one pin-hover away instead, which is why `to_detailed_string` for
/// `ScalarField` was promoted from optional to required.
#[test]
fn a_level_above_the_fields_range_is_a_value_and_no_error() {
    let mut designer = setup_designer();
    set_extraction_prefs(&mut designer, 1.0, 16_000_000);
    let cube_id = add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
    let iso_id = add_isosurface_node(
        &mut designer,
        &[],
        IsosurfaceNodeData {
            level: 1.0e9,
            ..Default::default()
        },
    );
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    designer.set_node_display(iso_id, true);

    assert!(
        matches!(
            evaluate_pin(&designer, iso_id, 0),
            NetworkResult::Isosurface(_)
        ),
        "an unreachable level still produces a value"
    );

    full_refresh(&mut designer);
    assert_eq!(
        designer
            .last_generated_structure_designer_scene
            .get_node_error(&[], iso_id),
        None,
        "an empty surface is silent by design"
    );

    match &designer
        .last_generated_structure_designer_scene
        .node_data
        .get(&NodeRef::top(iso_id))
        .expect("the displayed node has a scene entry")
        .output
    {
        NodeOutput::Isosurface(mesh) => assert!(
            mesh.is_empty(),
            "nothing in the field reaches this level, so the mesh is empty"
        ),
        other => panic!(
            "expected an Isosurface output, got {}",
            node_output_name(other)
        ),
    }
}

// ============================================================================
// Display conversion
// ============================================================================

#[test]
fn a_displayed_signed_field_extracts_both_lobes() {
    let mut designer = setup_designer();
    set_extraction_prefs(&mut designer, 1.0, 16_000_000);
    // `p2z_11x11x11.cube` is an analytic 2p_z sampled onto a grid: signed, with
    // one lobe either side of the nodal plane.
    let cube_id =
        add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("p2z_11x11x11.cube"));
    let iso_id = add_isosurface_node(
        &mut designer,
        &[],
        IsosurfaceNodeData {
            level: 0.05,
            ..Default::default()
        },
    );
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    designer.set_node_display(iso_id, true);
    full_refresh(&mut designer);

    let NodeOutput::Isosurface(mesh) = &designer
        .last_generated_structure_designer_scene
        .node_data
        .get(&NodeRef::top(iso_id))
        .expect("scene entry")
        .output
    else {
        panic!("a displayed isosurface must convert to NodeOutput::Isosurface");
    };

    assert_eq!(
        mesh.components.len(),
        2,
        "a 2p_z at a level inside its range is exactly two lobes"
    );
    assert!(!mesh.positions.is_empty());
    assert_eq!(
        mesh.positions.len(),
        mesh.normals.len(),
        "one normal per position"
    );
    assert_eq!(
        mesh.positions.len(),
        mesh.albedo.len(),
        "one albedo per position"
    );

    // The two lobes are painted with the two phase colors, and nothing else.
    let defaults = IsosurfaceNodeData::default();
    let positive = defaults.positive_color.as_vec3();
    let negative = defaults.negative_color.as_vec3();
    assert!(
        mesh.albedo.iter().all(|c| *c == positive || *c == negative),
        "in Phase mode every vertex carries one of the two phase colors"
    );
    assert!(
        mesh.albedo.iter().any(|c| *c == positive) && mesh.albedo.iter().any(|c| *c == negative),
        "both sign passes contributed"
    );
}

/// P5's sharp edge, pinned as *intended behaviour* so nobody later "fixes" it
/// into an error. A color field whose box is smaller than the surface is a
/// legal, common pairing — orbitals and potentials routinely come from
/// separately computed files — and outside its box `ScalarField::sample`
/// returns `0.0` like everywhere else, so the overhang paints as whatever `0.0`
/// maps to. On a symmetric domain that is the ramp's midpoint: plain white.
///
/// Three things are asserted together because each alone would pass for the
/// wrong reason: the surface is **drawn** (not dropped), there is **no error
/// entry** (the reference guide is the only warning by design), and the
/// overhang is **exactly** the midpoint rather than some other neutral.
#[test]
fn a_surface_larger_than_its_color_field_is_drawn_neutral_and_silent() {
    let mut designer = setup_designer();
    set_extraction_prefs(&mut designer, 1.0, 16_000_000);
    // Surface: a 2p_z over a box of +/-2 A on every axis.
    let cube_id =
        add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("p2z_11x11x11.cube"));
    // Color: the ramp, whose box is [0,2] x [0,3] x [0,4] A — it does not reach
    // negative coordinates at all, so a whole half of the surface overhangs it.
    let color_id =
        add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("ramp_3x4x5.cube"));
    let iso_id = add_isosurface_node(
        &mut designer,
        &[],
        IsosurfaceNodeData {
            level: 0.05,
            // Wide and symmetric, so the out-of-box `0.0` lands on the midpoint
            // and in-box values (0..=234) are visibly to one side of it.
            color_min: -100.0,
            color_max: 100.0,
            ..Default::default()
        },
    );
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    designer.connect_nodes(color_id, 0, iso_id, 1);
    designer.set_node_display(iso_id, true);
    full_refresh(&mut designer);

    assert_eq!(
        designer
            .last_generated_structure_designer_scene
            .get_node_error(&[], iso_id),
        None,
        "the mismatched box is documented in the reference guide, not diagnosed here"
    );

    let NodeOutput::Isosurface(mesh) = &designer
        .last_generated_structure_designer_scene
        .node_data
        .get(&NodeRef::top(iso_id))
        .expect("scene entry")
        .output
    else {
        panic!("the surface must still be drawn");
    };
    assert!(
        !mesh.is_empty(),
        "the overhang does not suppress the surface"
    );

    // `sample_colormap(BlueWhiteRed, 0.0, (-100, 100))` is the exact ramp
    // midpoint, which for this map is white.
    let neutral = Vec3::ONE;
    let overhang: Vec<Vec3> = mesh
        .positions
        .iter()
        .zip(&mesh.albedo)
        .filter(|(p, _)| p.x < -0.5)
        .map(|(_, c)| *c)
        .collect();
    assert!(
        !overhang.is_empty(),
        "the fixtures must actually overhang, or this test proves nothing"
    );
    assert!(
        overhang.iter().all(|c| (*c - neutral).length() < 1e-5),
        "every vertex outside the color field's box paints the ramp midpoint"
    );

    // And the color field is genuinely being read — an all-neutral surface would
    // satisfy the check above while meaning the sampling never happened.
    assert!(
        mesh.albedo.iter().any(|c| (*c - neutral).length() > 0.05),
        "vertices inside the color field's box must be painted by its values"
    );
}

// ============================================================================
// Cell budget
// ============================================================================

/// A budget of 1 cell is under any real grid, so this exercises the refusal
/// without needing a huge quality multiplier.
const IMPOSSIBLE_BUDGET: usize = 1;

#[test]
fn a_breached_cell_budget_reports_at_the_nodes_own_ref_and_drops_the_output() {
    let mut designer = setup_designer();
    set_extraction_prefs(&mut designer, 3.0, IMPOSSIBLE_BUDGET);
    let cube_id = add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
    let iso_id = add_isosurface_node(&mut designer, &[], IsosurfaceNodeData::default());
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    designer.set_node_display(iso_id, true);
    full_refresh(&mut designer);

    let message = designer
        .last_generated_structure_designer_scene
        .get_node_error(&[], iso_id)
        .expect("a breached budget must reach the scene's node_errors");
    assert!(
        message.contains("cells") && message.contains("budget"),
        "the message names the cell count and the budget: {message}"
    );
    assert!(
        message.contains("isosurface_quality_multiplier")
            && message.contains("isosurface_cell_budget"),
        "the message names both preferences the user can move: {message}"
    );
    assert!(
        message.contains('3'),
        "the message names the current multiplier: {message}"
    );

    assert!(
        matches!(
            designer
                .last_generated_structure_designer_scene
                .node_data
                .get(&NodeRef::top(iso_id))
                .expect("scene entry")
                .output,
            NodeOutput::None
        ),
        "a refused extraction draws nothing"
    );
}

/// The error must disappear on the first pass where the budget fits — which it
/// does for free, because `generate_scene_scoped` clears `node_errors` at the
/// top of every pass. Worth pinning: the whole reporting path depends on that
/// clear-and-snapshot ordering.
#[test]
fn the_budget_error_clears_once_the_budget_fits() {
    let mut designer = setup_designer();
    set_extraction_prefs(&mut designer, 1.0, IMPOSSIBLE_BUDGET);
    let cube_id = add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
    let iso_id = add_isosurface_node(&mut designer, &[], IsosurfaceNodeData::default());
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    designer.set_node_display(iso_id, true);
    full_refresh(&mut designer);
    assert!(
        designer
            .last_generated_structure_designer_scene
            .get_node_error(&[], iso_id)
            .is_some(),
        "precondition: the budget is breached"
    );

    set_extraction_prefs(&mut designer, 1.0, 16_000_000);
    full_refresh(&mut designer);
    assert_eq!(
        designer
            .last_generated_structure_designer_scene
            .get_node_error(&[], iso_id),
        None,
        "raising the budget clears the error with no extra machinery"
    );
}

/// A node inside a 0-ary `closure` body is scene-evaluable, and its scene
/// entries key at the **scoped** `NodeRef`. The budget error is inserted from
/// the display conversion, which runs *before* the scope pops — so it must land
/// at the scoped address, not at the bare id.
#[test]
fn a_breached_budget_inside_a_closure_body_keys_at_the_scoped_ref() {
    let mut designer = setup_designer();
    set_extraction_prefs(&mut designer, 1.0, IMPOSSIBLE_BUDGET);

    let closure_id = designer.add_node_scoped(&[], "closure", DVec2::new(0.0, 0.0), None);
    designer.set_node_network_data_scoped(
        &[],
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Isosurface],
            param_names: Vec::new(),
            custom_label: None,
        }),
    );

    let body = [closure_id];
    let cube_id =
        add_loaded_import_cube_node(&mut designer, &body, &cube_fixture("water_bohr.cube"));
    let iso_id = add_isosurface_node(&mut designer, &body, IsosurfaceNodeData::default());
    designer.connect_nodes_scoped(&body, cube_id, 0, iso_id, 0);
    designer.set_node_display_scoped(&body, iso_id, true);
    full_refresh(&mut designer);

    assert!(
        designer
            .last_generated_structure_designer_scene
            .get_node_error(&body, iso_id)
            .is_some(),
        "the body node's budget error is addressable at its own scope"
    );
    assert_eq!(
        designer
            .last_generated_structure_designer_scene
            .get_node_error(&[], iso_id),
        None,
        "and NOT at the bare id, which a sibling top-level node could occupy"
    );
}

/// `entry().or_insert_with(..)`, not `insert`: if the node already failed in
/// `eval`, that error is the root cause and must survive the display
/// conversion's later attempt to report a budget breach.
#[test]
fn an_eval_error_survives_the_budget_message() {
    let mut designer = setup_designer();
    set_extraction_prefs(&mut designer, 1.0, IMPOSSIBLE_BUDGET);
    let cube_id = add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("water_bohr.cube"));
    // A non-positive level fails in `eval`, so the conversion never sees an
    // `Isosurface` value at all — and even if it did, it must not overwrite.
    let iso_id = add_isosurface_node(
        &mut designer,
        &[],
        IsosurfaceNodeData {
            level: -1.0,
            ..Default::default()
        },
    );
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    designer.set_node_display(iso_id, true);
    full_refresh(&mut designer);

    let message = designer
        .last_generated_structure_designer_scene
        .get_node_error(&[], iso_id)
        .expect("the eval error reaches the scene");
    assert!(
        message.contains("level must be greater than 0"),
        "the eval error is the root cause and wins: {message}"
    );
    assert!(
        !message.contains("budget"),
        "the budget message must not replace it: {message}"
    );
}

// ============================================================================
// Node data
// ============================================================================

#[test]
fn every_property_survives_a_text_property_round_trip() {
    use atomcad_structure_designer::node_data::NodeData;
    use atomcad_structure_designer::text_format::TextValue;
    use std::collections::HashMap;

    let original = IsosurfaceNodeData {
        level: 0.0123,
        positive_color: DVec3::new(0.1, 0.2, 0.3),
        negative_color: DVec3::new(0.4, 0.5, 0.6),
        alpha: 0.75,
        colormap: Colormap::BlueWhiteRed,
        color_min: -0.25,
        color_max: 0.35,
    };

    let props: HashMap<String, TextValue> = original.get_text_properties().into_iter().collect();
    assert_eq!(
        props.get("colormap"),
        Some(&TextValue::String("blue_white_red".to_string())),
        "the colormap is spelled out, not serialized as an index"
    );

    let mut restored = IsosurfaceNodeData::default();
    restored
        .set_text_properties(&props)
        .expect("its own properties must read back");

    assert_eq!(restored.level, original.level);
    assert_eq!(restored.positive_color, original.positive_color);
    assert_eq!(restored.negative_color, original.negative_color);
    assert_eq!(restored.alpha, original.alpha);
    assert_eq!(restored.colormap, original.colormap);
    assert_eq!(restored.color_min, original.color_min);
    assert_eq!(restored.color_max, original.color_max);
}

#[test]
fn an_unknown_colormap_name_is_rejected_rather_than_defaulted() {
    use atomcad_structure_designer::node_data::NodeData;
    use atomcad_structure_designer::text_format::TextValue;
    use std::collections::HashMap;

    let mut data = IsosurfaceNodeData::default();
    let props: HashMap<String, TextValue> = [(
        "colormap".to_string(),
        TextValue::String("viridis".to_string()),
    )]
    .into_iter()
    .collect();

    let error = data
        .set_text_properties(&props)
        .expect_err("an unknown ramp must not silently become the default");
    assert!(error.contains("viridis"), "the message names it: {error}");
}

#[test]
fn the_subtitle_shows_the_level_only_while_the_pin_is_unwired() {
    use atomcad_structure_designer::node_data::NodeData;

    let data = IsosurfaceNodeData {
        level: 0.02,
        ..Default::default()
    };
    assert_eq!(
        data.get_subtitle(&HashSet::new()),
        Some("level: 0.0200".to_string())
    );
    assert_eq!(
        data.get_subtitle(&HashSet::from(["level".to_string()])),
        None,
        "a wired level makes the stored one misleading, so it is hidden"
    );
}

// ============================================================================

fn node_output_name(output: &NodeOutput) -> &'static str {
    match output {
        NodeOutput::Atomic(..) => "Atomic",
        NodeOutput::SurfacePointCloud(_) => "SurfacePointCloud",
        NodeOutput::SurfacePointCloud2D(_) => "SurfacePointCloud2D",
        NodeOutput::PolyMesh(_) => "PolyMesh",
        NodeOutput::DrawingPlane(_) => "DrawingPlane",
        NodeOutput::Isosurface(_) => "Isosurface",
        NodeOutput::None => "None",
    }
}

/// Not an assertion so much as a specimen: prints the readout a user actually
/// hovers, so a change to its shape is visible in `cargo test -- --nocapture`
/// rather than only in the app.
#[test]
fn the_hover_readout_of_a_real_cube_names_what_the_field_is() {
    let mut designer = setup_designer();
    let cube_id =
        add_loaded_import_cube_node(&mut designer, &[], &cube_fixture("p2z_11x11x11.cube"));

    let shown = evaluate_pin(&designer, cube_id, 0).to_display_string();
    println!("--- import_cube.field hover readout ---\n{shown}\n---");

    // The description is what tells a reader these values are an orbital
    // amplitude (levels ~0.05) rather than a density (levels ~0.002). Nothing
    // in the numbers themselves says so.
    assert!(shown.contains("Synthetic 2p_z on oxygen"), "got: {shown}");
    assert!(
        shown.contains("grid:   11 x 11 x 11 samples (1,331)"),
        "got: {shown}"
    );
    assert!(shown.contains("(signed)"), "a 2p_z has both lobes: {shown}");
}
