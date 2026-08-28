//! P4 of `doc/design_isosurface_level.md` — the colour domain's fit and the
//! seam the editor reads it through.
//!
//! The panel's *widgets* are covered by the design's manual walkthrough
//! (`feedback_manual_test_for_editor_ui`). What is here is not widget
//! behaviour:
//!
//! - **the whole point of Part 4** — that the statistic is restricted to the
//!   *surface*, and that the volume's extrema are one or two orders of magnitude
//!   wider, which is why "never auto-fitted" stopped being true;
//! - **the fourth empty state**, which exists because this distribution lives on
//!   the scene rather than in an evaluation: a node nobody is displaying has no
//!   surface to measure;
//! - **that a fit writes concrete numbers and nothing else** — no mode, no flag,
//!   nothing a later quality change could re-resolve.

use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::nodes::import_cube::{ImportCubeData, LoadedCube};
use atomcad_structure_designer::nodes::isosurface::{IsosurfaceNodeData, LevelMode};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::f64::DVec2;
use rust_lib_flutter_cad::api::structure_designer::field_distribution_api::{
    APISurfaceDistributionState, APISurfaceValueDistribution, isosurface_color_distribution,
};

// ============================================================================
// Helpers
// ============================================================================

/// The density envelope everything here is measured on — the same `0.002`
/// convention `auto` picks for this field.
const DENSITY_LEVEL: f64 = 0.002;

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    // `StructureDesigner::new()` loads the real user preferences, so the
    // extraction quality has to be pinned or this file's numbers depend on
    // whoever ran it last.
    designer
        .preferences
        .geometry_visualization_preferences
        .isosurface_quality_multiplier = 1.0;
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn add_loaded_import_cube_node(designer: &mut StructureDesigner, fixture: &str, y: f64) -> u64 {
    let file_path = fixture_path_str(&format!("cube/{}", fixture));
    let node_id = designer.add_node("import_cube", DVec2::new(-400.0, y));
    let cube = atomcad_crystolecule::io::cube_loader::load_cube(&file_path, true)
        .expect("fixture should parse");
    let loaded = LoadedCube::from_cube_file(cube).expect("fixture should carry a field");

    let mut data = ImportCubeData::new();
    data.file_name = Some(file_path);
    data.loaded = Some(loaded);
    designer.set_node_network_data(node_id, Box::new(data));
    node_id
}

fn absolute_level(level: f64) -> IsosurfaceNodeData {
    IsosurfaceNodeData {
        level_mode: LevelMode::Absolute,
        level,
        ..IsosurfaceNodeData::default()
    }
}

fn full_refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

/// A density into `field`, a potential into `color_field`, both loaded, the
/// `isosurface` node displayed and one refresh run — the arrangement the whole
/// of Part 4 is about.
fn esp_painted_density() -> (StructureDesigner, u64, u64) {
    let mut designer = setup_designer();
    let density = add_loaded_import_cube_node(&mut designer, "water_density_17x15x19.cube", 0.0);
    let esp = add_loaded_import_cube_node(&mut designer, "water_esp_17x15x19.cube", 200.0);
    let iso = designer.add_node("isosurface", DVec2::ZERO);
    designer.set_node_network_data(iso, Box::new(absolute_level(DENSITY_LEVEL)));
    designer.connect_nodes(density, 0, iso, 0);
    designer.connect_nodes(esp, 0, iso, 1);
    designer.set_node_display(iso, true);
    full_refresh(&mut designer);
    (designer, iso, esp)
}

fn fetch(designer: &StructureDesigner, node_id: u64) -> APISurfaceValueDistribution {
    isosurface_color_distribution(designer, &[], node_id)
        .expect("an isosurface node always reports something")
}

/// The `color_field`'s range over the **whole box**, which is what a naive fit
/// would have used.
fn volume_half_range(designer: &mut StructureDesigner, esp_node: u64) -> f64 {
    match designer.evaluate_node_output(&[], esp_node, 0) {
        NetworkResult::ScalarField(field) => {
            let (min, max) = field.value_range().expect("a sampled field has a range");
            min.abs().max(max.abs())
        }
        _ => panic!("the esp import_cube should evaluate to a ScalarField"),
    }
}

// ============================================================================
// The measurement that motivates the whole of Part 4
// ============================================================================

#[test]
fn the_surface_band_is_an_order_of_magnitude_inside_the_volume_range() {
    let (mut designer, iso, esp) = esp_painted_density();
    let reported = fetch(&designer, iso);

    assert_eq!(reported.state, APISurfaceDistributionState::Available);
    assert!(reported.is_signed, "an electrostatic potential is signed");
    assert!(reported.can_fit, "{}", reported.fit_blocked_reason);

    let volume = volume_half_range(&mut designer, esp);
    let surface = reported.fit_max;
    assert!(
        surface * 10.0 < volume,
        "the surface band ({surface}) must be far inside the volume range \
         ({volume}) — that gap is the entire argument for restricting the \
         population to the surface"
    );

    // The measured proxy on this fixture is 0.0909 against a volume 1.4215.
    // **Not to three significant figures**: this is an area-weighted percentile
    // over mesh vertices at a preference-driven resolution, while that number is
    // a voxel-band proxy on the file grid.
    assert!(
        (0.05..=0.12).contains(&surface),
        "the symmetric fit should land near the 0.09 the fixture's header \
         records, got {surface}"
    );
    assert_eq!(
        reported.fit_min, -reported.fit_max,
        "a signed colour field fits symmetrically, so the ramp's white stays \
         on zero"
    );
}

#[test]
fn the_reported_shape_matches_the_histogram_behind_it() {
    let (designer, iso, _) = esp_painted_density();
    let reported = fetch(&designer, iso);

    assert!(!reported.bin_weight.is_empty());
    assert_eq!(
        reported.bin_edges.len(),
        reported.bin_weight.len() + 1,
        "one more edge than bins, so a bin can be drawn as a span"
    );
    assert!(
        reported
            .bin_edges
            .windows(2)
            .all(|pair| pair[1] > pair[0] - 1e-18),
        "edges are ascending"
    );
    assert!(reported.value_max > reported.value_min);
    assert!(reported.p2 >= reported.value_min && reported.p98 <= reported.value_max);
    assert!(reported.p98 > reported.p2);
    assert!(reported.vertex_count > 0);
    assert!(reported.fit_blocked_reason.is_empty());
}

// ============================================================================
// The empty states
// ============================================================================

#[test]
fn a_hidden_node_reports_not_extracted_rather_than_erroring() {
    // **The fourth empty state.** The distribution lives on the scene, and there
    // is no scene entry for a node nobody is displaying — so the panel must say
    // *display the node*, not offer a fit button that silently does nothing.
    let (mut designer, iso, _) = esp_painted_density();
    designer.set_node_display(iso, false);
    full_refresh(&mut designer);

    let reported = fetch(&designer, iso);
    assert_eq!(reported.state, APISurfaceDistributionState::NotExtracted);
    assert!(!reported.can_fit);
    assert!(
        reported.message.to_lowercase().contains("display"),
        "the caption must say what to do about it, got {:?}",
        reported.message
    );
}

#[test]
fn an_unwired_color_field_reports_no_colour_field() {
    let mut designer = setup_designer();
    let density = add_loaded_import_cube_node(&mut designer, "water_density_17x15x19.cube", 0.0);
    let iso = designer.add_node("isosurface", DVec2::ZERO);
    designer.set_node_network_data(iso, Box::new(absolute_level(DENSITY_LEVEL)));
    designer.connect_nodes(density, 0, iso, 0);
    designer.set_node_display(iso, true);
    full_refresh(&mut designer);

    let reported = fetch(&designer, iso);
    assert_eq!(reported.state, APISurfaceDistributionState::NoColorField);
    assert!(!reported.can_fit);
    assert!(reported.bin_weight.is_empty());
}

#[test]
fn a_node_that_is_not_an_isosurface_reports_nothing_at_all() {
    // `None` is reserved for "wrong node type": every genuine "nothing to show"
    // is one of the three states above, so the panel can name which and keep its
    // shape.
    let mut designer = setup_designer();
    let float_id = designer.add_node("float", DVec2::ZERO);
    assert!(isosurface_color_distribution(&designer, &[], float_id).is_none());
}

// ============================================================================
// What a fit writes, and what it must not
// ============================================================================

#[test]
fn the_fit_is_concrete_numbers_and_nothing_else() {
    let (mut designer, iso, _) = esp_painted_density();
    let fitted = fetch(&designer, iso);
    assert!(fitted.can_fit);

    // The editor's write, verbatim: the same whole-data replacement the two
    // range text fields already use, which is how it becomes undoable without
    // being special.
    let mut data = absolute_level(DENSITY_LEVEL);
    data.color_min = fitted.fit_min;
    data.color_max = fitted.fit_max;
    designer.set_node_network_data(iso, Box::new(data));
    full_refresh(&mut designer);

    let stored = designer
        .get_node_network_data_scoped(&[], iso)
        .expect("node")
        .as_any_ref()
        .downcast_ref::<IsosurfaceNodeData>()
        .expect("isosurface node data")
        .clone();
    assert_eq!(stored.color_min, fitted.fit_min);
    assert_eq!(stored.color_max, fitted.fit_max);
    assert!(stored.color_max > stored.color_min);
    // No mode, no flag: nothing on the node records that the numbers came from a
    // fit, so nothing can re-resolve them later.
    assert_eq!(stored.level_mode, LevelMode::Absolute);

    // And the saved domain does not move when the extraction quality does —
    // the reason the fit is a one-shot editor action rather than a mode.
    designer
        .preferences
        .geometry_visualization_preferences
        .isosurface_quality_multiplier = 2.0;
    full_refresh(&mut designer);

    let after = designer
        .get_node_network_data_scoped(&[], iso)
        .expect("node")
        .as_any_ref()
        .downcast_ref::<IsosurfaceNodeData>()
        .expect("isosurface node data")
        .clone();
    assert_eq!(after.color_min, fitted.fit_min);
    assert_eq!(after.color_max, fitted.fit_max);
}

#[test]
fn the_fit_barely_moves_when_the_extraction_quality_does() {
    // The vertex floor's purpose, at the API seam: the *reported* fit is
    // re-measured on every refresh, so if it drifted with the quality
    // preference, pressing the button at two settings would write two documents.
    let (mut designer, iso, _) = esp_painted_density();
    let coarse = fetch(&designer, iso);

    designer
        .preferences
        .geometry_visualization_preferences
        .isosurface_quality_multiplier = 2.0;
    full_refresh(&mut designer);
    let fine = fetch(&designer, iso);

    assert!(
        fine.vertex_count > coarse.vertex_count,
        "the two extractions must actually differ, or this asserts nothing"
    );
    let span = coarse.fit_max - coarse.fit_min;
    assert!(
        (fine.fit_max - coarse.fit_max).abs() < span * 0.1,
        "fits should agree closely: {} vs {}",
        coarse.fit_max,
        fine.fit_max
    );
}
