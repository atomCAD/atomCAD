//! P3 of `doc/design_isosurface_level.md` — the editor's kernel seam.
//!
//! The panel's *widgets* are covered by the design's manual walkthrough
//! (`feedback_manual_test_for_editor_ui`). Three things here are not widget
//! behaviour and must not hide behind that rule: the shape of
//! `APIValueDistribution` and its three empty states, the **invalidation** of
//! that struct across a rewire (the failure the design calls silent — a
//! correct-looking histogram belonging to the previous field), and the **undo
//! coalescing** a log slider makes mandatory.

use atomcad_crystolecule::field::{
    FieldBounds, GridGeometry, ScalarField, distribution::HISTOGRAM_BINS,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::nodes::import_cube::{ImportCubeData, LoadedCube};
use atomcad_structure_designer::nodes::isosurface::{IsosurfaceNodeData, LevelMode};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::f64::DVec2;
use glam::f64::DVec3;
use rust_lib_flutter_cad::api::structure_designer::field_distribution_api::{
    APIDistributionState, APIValueDistribution, describe_field, isosurface_level_distribution,
};
use std::sync::Arc;

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

fn add_loaded_import_cube_node(designer: &mut StructureDesigner, fixture: &str) -> u64 {
    let file_path = cube_fixture(fixture);
    let node_id = designer.add_node("import_cube", DVec2::new(-400.0, 0.0));
    let cube = atomcad_crystolecule::io::cube_loader::load_cube(&file_path, true)
        .expect("fixture should parse");
    let loaded = LoadedCube::from_cube_file(cube).expect("fixture should carry a field");

    let mut data = ImportCubeData::new();
    data.file_name = Some(file_path);
    data.loaded = Some(loaded);
    designer.set_node_network_data(node_id, Box::new(data));
    node_id
}

fn add_isosurface_node(designer: &mut StructureDesigner, data: IsosurfaceNodeData) -> u64 {
    let node_id = designer.add_node("isosurface", DVec2::ZERO);
    designer.set_node_network_data(node_id, Box::new(data));
    node_id
}

/// One loaded `import_cube` wired into one `isosurface`.
fn wired(fixture: &str, data: IsosurfaceNodeData) -> (StructureDesigner, u64, u64) {
    let mut designer = setup_designer();
    let cube_id = add_loaded_import_cube_node(&mut designer, fixture);
    let iso_id = add_isosurface_node(&mut designer, data);
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    (designer, cube_id, iso_id)
}

fn fetch(designer: &mut StructureDesigner, node_id: u64) -> APIValueDistribution {
    isosurface_level_distribution(designer, &[], node_id)
        .expect("an isosurface node always reports something")
}

// ============================================================================
// Shape
// ============================================================================

#[test]
fn the_reported_shape_matches_the_histogram_behind_it() {
    let (mut designer, _, iso_id) = wired("p2z_11x11x11.cube", IsosurfaceNodeData::default());
    let reported = fetch(&mut designer, iso_id);

    assert_eq!(reported.state, APIDistributionState::Available);
    assert_eq!(reported.bin_mass.len(), HISTOGRAM_BINS);
    assert_eq!(
        reported.bin_edges.len(),
        reported.bin_mass.len() + 1,
        "one more edge than bins, so a bin can be drawn as a span"
    );
    assert_eq!(
        reported.cumulative_at_or_above.len(),
        reported.bin_edges.len(),
        "the cumulative curve is sampled at the edges, not at the bins"
    );

    // The curve is a normalized suffix sum: 1 at the smallest edge, 0 past the
    // largest, monotonically non-increasing in between. Without this the
    // overlay could be drawn upside down and still look plausible.
    assert!((reported.cumulative_at_or_above[0] - 1.0).abs() < 1e-9);
    assert_eq!(*reported.cumulative_at_or_above.last().unwrap(), 0.0);
    assert!(
        reported
            .cumulative_at_or_above
            .windows(2)
            .all(|pair| pair[0] >= pair[1])
    );

    assert!(reported.nonzero_min > 0.0 && reported.nonzero_max > reported.nonzero_min);
    assert!(reported.is_exact, "an 11^3 field is well under the limit");
}

#[test]
fn the_resolved_pair_is_the_nodes_own_and_carries_the_auto_basis() {
    // Under `Auto` the panel must print the *resolved* pair, not the dormant
    // stored numbers, and name the basis — the only signal that auto guessed.
    let (mut designer, _, iso_id) = wired("p2z_11x11x11.cube", IsosurfaceNodeData::default());
    let reported = fetch(&mut designer, iso_id);

    assert!(reported.has_resolved_level && reported.has_resolved_fraction);
    assert_eq!(
        reported.auto_basis, "signed field",
        "`p2z` is signed, so auto takes the fraction branch"
    );
    // Same number the extractor draws at: the level must sit inside the very
    // distribution the panel is plotting.
    assert!(reported.resolved_fraction > 0.0 && reported.resolved_fraction <= 1.0);
    assert!(
        reported.resolved_level >= reported.nonzero_min
            && reported.resolved_level <= reported.nonzero_max
    );
}

#[test]
fn the_basis_is_empty_outside_auto() {
    let (mut designer, _, iso_id) = wired(
        "p2z_11x11x11.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Absolute,
            level: 0.02,
            ..Default::default()
        },
    );
    let reported = fetch(&mut designer, iso_id);
    assert!(reported.auto_basis.is_empty());
    assert!(reported.has_resolved_level);
    assert_eq!(reported.resolved_level, 0.02);
}

// ============================================================================
// Empty states — three, all normal, none an error
// ============================================================================

#[test]
fn no_field_wired_reports_itself() {
    let mut designer = setup_designer();
    let iso_id = add_isosurface_node(&mut designer, IsosurfaceNodeData::default());
    let reported = fetch(&mut designer, iso_id);
    assert_eq!(reported.state, APIDistributionState::NoField);
    assert!(reported.bin_mass.is_empty());
    assert!(
        !reported.has_resolved_level,
        "there is no surface, so there is no number to grey out"
    );
}

/// A field with no stored samples — the analytic case. No *node* produces one
/// yet (a future Molden orbital will be the first), so the only way to reach
/// that arm is to hand one in. Mirrors `auto_level_test.rs`'s `Analytic`.
#[derive(Debug)]
struct Analytic;

impl ScalarField for Analytic {
    fn sample(&self, point: DVec3) -> f64 {
        (-point.length_squared()).exp()
    }
    fn data_bounds(&self) -> Option<FieldBounds> {
        None
    }
    fn suggested_bounds(&self) -> FieldBounds {
        FieldBounds::new(DVec3::splat(-1.0), DVec3::splat(1.0))
    }
    fn native_grid(&self) -> Option<GridGeometry> {
        None
    }
    fn value_range(&self) -> Option<(f64, f64)> {
        Some((0.0, 1.0))
    }
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

#[test]
fn an_analytic_field_reports_itself_rather_than_erroring() {
    let field: Arc<dyn ScalarField> = Arc::new(Analytic);
    let reported = describe_field(&field);
    assert_eq!(reported.state, APIDistributionState::AnalyticField);
    assert!(reported.bin_mass.is_empty());
    assert!(
        reported.message.to_lowercase().contains("analytic"),
        "the caption must name which empty state this is, got: {}",
        reported.message
    );
    // Distinguishable from "no field wired", which is the point of having both.
    let mut designer = setup_designer();
    let iso_id = add_isosurface_node(&mut designer, IsosurfaceNodeData::default());
    assert_eq!(
        fetch(&mut designer, iso_id).state,
        APIDistributionState::NoField
    );
}

#[test]
fn an_unevaluatable_upstream_reports_itself_and_says_why() {
    // An `import_cube` with no file loaded is the "upstream not yet evaluated"
    // state: a wire exists, but nothing comes down it.
    let mut designer = setup_designer();
    let cube_id = designer.add_node("import_cube", DVec2::new(-400.0, 0.0));
    designer.set_node_network_data(cube_id, Box::new(ImportCubeData::new()));
    let iso_id = add_isosurface_node(&mut designer, IsosurfaceNodeData::default());
    designer.connect_nodes(cube_id, 0, iso_id, 0);

    let reported = fetch(&mut designer, iso_id);
    assert_eq!(reported.state, APIDistributionState::NotEvaluated);
    assert!(
        !reported.message.is_empty(),
        "the caption has to say which of the three states this is"
    );
    assert!(reported.bin_mass.is_empty());
}

#[test]
fn a_non_isosurface_node_is_the_only_none() {
    let mut designer = setup_designer();
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    assert!(isosurface_level_distribution(&mut designer, &[], sphere_id).is_none());
    assert!(isosurface_level_distribution(&mut designer, &[], 9999).is_none());
}

// ============================================================================
// Probe hygiene
// ============================================================================

#[test]
fn an_editor_probe_does_not_push_console_entries() {
    // `evaluate_node_argument` runs on demand from a property panel, possibly on
    // every repaint. Everything `with_eval_context` parks on the designer at
    // end-of-pass has to be put back, or selecting a node would spam the Console
    // with a fresh copy of every upstream `print` — and would hand the profiling
    // panel the probe's numbers as if they were the next refresh's.
    let mut designer = setup_designer();
    let upstream = designer.add_node("print", DVec2::new(-200.0, 0.0));
    let downstream = designer.add_node("print", DVec2::ZERO);
    designer.connect_nodes(upstream, 0, downstream, 0);
    designer.take_print_log();

    let probed = designer.evaluate_node_argument(&[], downstream, 0);
    assert!(
        matches!(probed, NetworkResult::String(_)),
        "the probe must actually have evaluated the upstream node"
    );
    assert!(
        designer.take_print_log().is_empty(),
        "a panel repaint must not push console entries"
    );
}

// ============================================================================
// Invalidation
// ============================================================================

#[test]
fn rewiring_the_field_pin_changes_the_reported_distribution() {
    // The failure this guards is silent: a correct-looking histogram belonging
    // to the *previous* field. Two fetches with a rewire between them must not
    // agree.
    let (mut designer, first_cube, iso_id) =
        wired("p2z_11x11x11.cube", IsosurfaceNodeData::default());
    let before = fetch(&mut designer, iso_id);

    let _ = first_cube;
    // A non-array input pin takes one wire, so connecting a second source
    // *replaces* the first — the rewire the panel has to survive.
    let second_cube = add_loaded_import_cube_node(&mut designer, "water_density_17x15x19.cube");
    designer.connect_nodes(second_cube, 0, iso_id, 0);
    let after = fetch(&mut designer, iso_id);

    assert_eq!(after.state, APIDistributionState::Available);
    assert!(
        before.nonzero_max != after.nonzero_max || before.bin_mass != after.bin_mass,
        "the second fetch still describes the first field"
    );
}

// ============================================================================
// Drag coalescing
// ============================================================================

/// Write `level_fraction` the way the editor's `_update` does — a whole-struct
/// replacement through the shared node-data setter.
fn write_fraction(designer: &mut StructureDesigner, node_id: u64, fraction: f64) {
    designer.set_node_network_data_scoped(
        &[],
        node_id,
        Box::new(IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level_fraction: fraction,
            ..Default::default()
        }),
    );
}

#[test]
fn a_drag_leaves_one_undo_entry_not_one_per_tick() {
    let (mut designer, _, iso_id) = wired(
        "p2z_11x11x11.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level_fraction: 0.9,
            ..Default::default()
        },
    );
    let baseline = designer.undo_stack.history_len();

    designer.begin_node_data_drag(vec![], iso_id);
    for tick in 1..=10 {
        write_fraction(&mut designer, iso_id, 0.9 + 0.009 * tick as f64);
    }
    designer.end_node_data_drag();

    assert_eq!(
        designer.undo_stack.history_len(),
        baseline + 1,
        "a log slider is the canonical undo-flooding case"
    );

    // And one Ctrl-Z returns the whole drag, not its last tick.
    designer.undo();
    let restored = designer
        .get_node_network_data_scoped(&[], iso_id)
        .and_then(|data| data.as_any_ref().downcast_ref::<IsosurfaceNodeData>())
        .map(|data| data.level_fraction)
        .expect("the node survives the undo");
    assert!((restored - 0.9).abs() < 1e-12);
}

#[test]
fn a_no_op_drag_pushes_nothing() {
    let (mut designer, _, iso_id) = wired(
        "p2z_11x11x11.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level_fraction: 0.9,
            ..Default::default()
        },
    );
    let baseline = designer.undo_stack.history_len();
    designer.begin_node_data_drag(vec![], iso_id);
    designer.end_node_data_drag();
    assert_eq!(designer.undo_stack.history_len(), baseline);
}

#[test]
fn an_unmatched_begin_cannot_silently_disable_undo() {
    // A disposed widget can lose the `end`. The next `begin` must close the
    // stale session rather than leave node-data recording off forever.
    let (mut designer, _, iso_id) = wired(
        "p2z_11x11x11.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level_fraction: 0.9,
            ..Default::default()
        },
    );
    let baseline = designer.undo_stack.history_len();

    designer.begin_node_data_drag(vec![], iso_id);
    write_fraction(&mut designer, iso_id, 0.95);
    // ... `end` never arrives.
    designer.begin_node_data_drag(vec![], iso_id);
    write_fraction(&mut designer, iso_id, 0.99);
    designer.end_node_data_drag();

    assert_eq!(
        designer.undo_stack.history_len(),
        baseline + 2,
        "one entry per closed session, and none lost"
    );
}

#[test]
fn an_edit_to_another_node_during_a_drag_still_records() {
    // The coalescing guard is keyed on the address, not global suppression.
    let (mut designer, _, iso_id) = wired(
        "p2z_11x11x11.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level_fraction: 0.9,
            ..Default::default()
        },
    );
    let other_id = add_isosurface_node(&mut designer, IsosurfaceNodeData::default());
    let baseline = designer.undo_stack.history_len();

    designer.begin_node_data_drag(vec![], iso_id);
    write_fraction(&mut designer, iso_id, 0.95);
    write_fraction(&mut designer, other_id, 0.5);
    designer.end_node_data_drag();

    assert_eq!(
        designer.undo_stack.history_len(),
        baseline + 2,
        "the unrelated edit keeps its own undo entry"
    );
}

// ============================================================================
// Mode switching — the mode is a unit, not a second parked value
// ============================================================================

/// The panel's mode switch, as the editor performs it: fill the coordinate the
/// new mode makes live from the level currently drawn, **unless** the parked
/// number already describes that same surface, in which case keep it verbatim.
///
/// Mirrors `_changeMode` in `isosurface_editor.dart`. Kept here rather than
/// asserted through the widget because what it protects is a *numeric* property
/// — a typed constant surviving a round trip — not a widget behaviour.
fn switch_mode(designer: &mut StructureDesigner, node_id: u64, mode: LevelMode) {
    let reported = fetch(designer, node_id);
    let mut next = stored_data(designer, node_id);
    next.level_mode = mode;
    match mode {
        LevelMode::Fraction => {
            if reported.has_resolved_fraction && !reported.stored_fraction_matches {
                // Mirrors `IsosurfaceEditor.clampFraction`: the enclosed
                // fraction can be exactly 1.0 (a level at or below the smallest
                // sample encloses everything) while the property is validated
                // as strictly inside (0, 1).
                next.level_fraction = reported
                    .resolved_fraction
                    .clamp(FRACTION_EPSILON, 1.0 - FRACTION_EPSILON);
            }
        }
        LevelMode::Absolute => {
            if reported.has_resolved_level && !reported.stored_level_matches {
                next.level = reported.resolved_level;
            }
        }
        // Auto consults neither stored number, so there is nothing to convert.
        LevelMode::Auto => {}
    }
    designer.set_node_network_data_scoped(&[], node_id, Box::new(next));
}

/// Mirrors `IsosurfaceEditor.FRACTION_EPSILON`.
const FRACTION_EPSILON: f64 = 1e-6;

fn stored_data(designer: &StructureDesigner, node_id: u64) -> IsosurfaceNodeData {
    designer
        .get_node_network_data_scoped(&[], node_id)
        .and_then(|data| data.as_any_ref().downcast_ref::<IsosurfaceNodeData>())
        .expect("an isosurface node")
        .clone()
}

fn set_fraction(designer: &mut StructureDesigner, node_id: u64, fraction: f64) {
    let mut data = stored_data(designer, node_id);
    data.level_fraction = fraction;
    designer.set_node_network_data_scoped(&[], node_id, Box::new(data));
}

#[test]
fn leaving_auto_takes_over_the_level_auto_arrived_at() {
    let (mut designer, _, iso_id) = wired("p2z_11x11x11.cube", IsosurfaceNodeData::default());
    let auto = fetch(&mut designer, iso_id);
    assert!(auto.has_resolved_level && auto.has_resolved_fraction);

    switch_mode(&mut designer, iso_id, LevelMode::Fraction);
    let taken_over = fetch(&mut designer, iso_id);
    assert_eq!(
        taken_over.resolved_level, auto.resolved_level,
        "the surface must not move when auto is taken over"
    );
    assert!(taken_over.auto_basis.is_empty());
}

#[test]
fn switching_between_the_two_manual_modes_never_moves_the_surface() {
    // The mode is the *unit* the one level is expressed in, so a switch converts
    // rather than swapping in a differently-valued parked number.
    let (mut designer, _, iso_id) = wired(
        "p2z_11x11x11.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Absolute,
            level: 0.02,
            // Deliberately *not* the matching fraction: under the old
            // two-parked-values model this is the number the switch would have
            // adopted, moving the surface.
            level_fraction: 0.5,
            ..Default::default()
        },
    );
    let before = fetch(&mut designer, iso_id);
    assert_eq!(before.resolved_level, 0.02);

    switch_mode(&mut designer, iso_id, LevelMode::Fraction);
    assert_eq!(
        fetch(&mut designer, iso_id).resolved_fraction,
        before.resolved_fraction,
        "the surface encloses the same share after the switch"
    );

    switch_mode(&mut designer, iso_id, LevelMode::Absolute);
    assert_eq!(
        fetch(&mut designer, iso_id).resolved_fraction,
        before.resolved_fraction
    );
}

#[test]
fn a_typed_constant_survives_a_round_trip_through_fraction_mode() {
    // `iso -> f -> iso` is **not** a bijection: `iso_for_fraction` can only
    // return a stored sample, so an unconditional conversion would hand `0.002`
    // back as a nearby sample value. The stored number is kept verbatim while it
    // still encloses what the surface encloses — which is exactly the case where
    // the user changed nothing.
    let (mut designer, _, iso_id) = wired(
        "water_density_17x15x19.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Absolute,
            level: 0.002,
            ..Default::default()
        },
    );
    switch_mode(&mut designer, iso_id, LevelMode::Fraction);
    switch_mode(&mut designer, iso_id, LevelMode::Absolute);

    assert_eq!(
        stored_data(&designer, iso_id).level,
        0.002,
        "the constant the user typed must come back unchanged, not as a sample value"
    );
}

#[test]
fn a_fraction_that_was_actually_changed_is_converted_rather_than_kept() {
    // The other half of the same rule: once the fraction has moved, the parked
    // absolute number no longer describes the surface and must be overwritten.
    let (mut designer, _, iso_id) = wired(
        "water_density_17x15x19.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Absolute,
            level: 0.002,
            ..Default::default()
        },
    );
    switch_mode(&mut designer, iso_id, LevelMode::Fraction);
    set_fraction(&mut designer, iso_id, 0.5);
    let dragged = fetch(&mut designer, iso_id);

    switch_mode(&mut designer, iso_id, LevelMode::Absolute);
    let after = fetch(&mut designer, iso_id);
    assert_ne!(
        after.resolved_level, 0.002,
        "the stale 0.002 must not be resurrected"
    );
    assert_eq!(
        after.resolved_fraction, dragged.resolved_fraction,
        "the switch converts, so the surface stays put"
    );
}

#[test]
fn switching_back_to_auto_returns_to_the_fields_own_level() {
    // The one switch that *is* allowed to move the surface: auto consults
    // neither stored number and re-derives from the field.
    let (mut designer, _, iso_id) = wired("p2z_11x11x11.cube", IsosurfaceNodeData::default());
    let auto = fetch(&mut designer, iso_id);

    switch_mode(&mut designer, iso_id, LevelMode::Fraction);
    set_fraction(&mut designer, iso_id, 0.5);
    assert_ne!(
        fetch(&mut designer, iso_id).resolved_level,
        auto.resolved_level
    );

    switch_mode(&mut designer, iso_id, LevelMode::Auto);
    let back = fetch(&mut designer, iso_id);
    assert_eq!(back.resolved_level, auto.resolved_level);
    assert_eq!(back.auto_basis, auto.auto_basis);
}

#[test]
fn with_no_field_a_switch_falls_back_to_the_parked_number() {
    // Nothing to convert through, which is why both properties stay in storage
    // even though only one is ever on screen.
    let mut designer = setup_designer();
    let iso_id = add_isosurface_node(
        &mut designer,
        IsosurfaceNodeData {
            level_mode: LevelMode::Absolute,
            level: 0.02,
            level_fraction: 0.9,
            ..Default::default()
        },
    );
    assert_eq!(
        fetch(&mut designer, iso_id).state,
        APIDistributionState::NoField
    );

    switch_mode(&mut designer, iso_id, LevelMode::Fraction);
    let stored = stored_data(&designer, iso_id);
    assert_eq!(
        stored.level_fraction, 0.9,
        "the parked fraction is used as-is"
    );
    assert_eq!(stored.level, 0.02, "and the other number is left alone");
}
