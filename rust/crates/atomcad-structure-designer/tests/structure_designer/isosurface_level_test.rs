//! The `isosurface` node's **level modes** — P2 of
//! `doc/design_isosurface_level.md`.
//!
//! The *rule* that `Auto` applies is a field property and is tested in
//! `atomcad-crystolecule`'s `auto_level_test.rs`, where an analytic field can be
//! written in six lines. What is tested here is the *node*: which stored number
//! each mode reads, what `eval` does with a bad one, what the text format emits
//! and infers, what validation says about a wire the mode ignores, and what the
//! readout prints.
//!
//! `StructureDesigner::new()` loads the **real** user preferences file, so a
//! test that depends on an extraction preference pins it explicitly.

use std::collections::{HashMap, HashSet};

use atomcad_crystolecule::field::{AutoBasis, DENSITY_LEVEL, LevelBasis};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::nodes::float::FloatData;
use atomcad_structure_designer::nodes::import_cube::{ImportCubeData, LoadedCube};
use atomcad_structure_designer::nodes::isosurface::{
    DEFAULT_LEVEL, DEFAULT_LEVEL_FRACTION, IsosurfaceNodeData, LevelMode,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::TextValue;
use atomcad_test_support::fixture_path_str;
use glam::f64::DVec2;

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

fn add_loaded_import_cube_node(designer: &mut StructureDesigner, file_path: &str) -> u64 {
    let node_id = designer.add_node("import_cube", DVec2::new(-400.0, 0.0));
    let cube = atomcad_crystolecule::io::cube_loader::load_cube(file_path, true)
        .expect("fixture should parse");
    let loaded = LoadedCube::from_cube_file(cube).expect("fixture should carry a field");

    let mut data = ImportCubeData::new();
    data.file_name = Some(file_path.to_string());
    data.loaded = Some(loaded);
    designer.set_node_network_data(node_id, Box::new(data));
    node_id
}

fn add_isosurface_node(designer: &mut StructureDesigner, data: IsosurfaceNodeData) -> u64 {
    let node_id = designer.add_node("isosurface", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(node_id, Box::new(data));
    node_id
}

fn evaluate_pin(designer: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&stack, node_id, 0, registry, false, &mut context)
}

/// A designer with one loaded `import_cube` wired into one `isosurface`.
fn wired(fixture: &str, data: IsosurfaceNodeData) -> (StructureDesigner, u64, u64) {
    let mut designer = setup_designer();
    let cube_id = add_loaded_import_cube_node(&mut designer, &cube_fixture(fixture));
    let iso_id = add_isosurface_node(&mut designer, data);
    designer.connect_nodes(cube_id, 0, iso_id, 0);
    (designer, cube_id, iso_id)
}

fn expect_isosurface(result: NetworkResult) -> atomcad_crystolecule::field::IsosurfaceData {
    match result {
        NetworkResult::Isosurface(data) => data,
        other => panic!("expected an Isosurface, got {}", other.to_display_string()),
    }
}

fn expect_error(result: NetworkResult) -> String {
    match result {
        NetworkResult::Error(message) => message,
        other => panic!(
            "expected an evaluation error, got {}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Defaults
// ============================================================================

#[test]
fn a_new_node_defaults_to_auto() {
    // `node_data_creator` takes no arguments and has no field to look at, so a
    // field-dependent default cannot be computed at node creation — which is
    // the whole reason `Auto` exists as a mode rather than as a one-off guess.
    let data = IsosurfaceNodeData::default();
    assert_eq!(data.level_mode, LevelMode::Auto);
    assert_eq!(data.level, DEFAULT_LEVEL);
    assert_eq!(data.level_fraction, DEFAULT_LEVEL_FRACTION);
}

// ============================================================================
// eval — the three modes
// ============================================================================

#[test]
fn auto_resolves_from_the_field_and_reports_its_basis() {
    let (designer, _, iso_id) = wired("water_density_17x15x19.cube", IsosurfaceNodeData::default());
    let data = expect_isosurface(evaluate_pin(&designer, iso_id));

    assert_eq!(data.level, DENSITY_LEVEL);
    assert_eq!(data.level_basis, LevelBasis::Auto(AutoBasis::DensityLike));

    // The stored `0.02` is dormant under auto and must not have been used.
    assert_ne!(data.level, DEFAULT_LEVEL);
}

#[test]
fn fraction_mode_resolves_through_the_distribution() {
    let (designer, _, iso_id) = wired(
        "water_density_17x15x19.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level_fraction: 0.9,
            ..Default::default()
        },
    );
    let data = expect_isosurface(evaluate_pin(&designer, iso_id));

    let expected = data
        .field
        .value_distribution()
        .expect("a sampled field has a distribution")
        .iso_for_fraction(0.9)
        .expect("a non-empty field resolves");
    assert_eq!(
        data.level, expected,
        "fraction mode is exactly `iso_for_fraction`, not an approximation of it"
    );
    assert_eq!(data.level_basis, LevelBasis::Fraction(0.9));
}

#[test]
fn absolute_mode_is_unchanged_by_this_feature() {
    let (designer, _, iso_id) = wired(
        "water_density_17x15x19.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Absolute,
            level: 0.0035,
            ..Default::default()
        },
    );
    let data = expect_isosurface(evaluate_pin(&designer, iso_id));

    assert_eq!(data.level, 0.0035);
    assert_eq!(data.alpha, 0.4);
    // The only difference from before the level modes existed is the readout
    // passenger, which nothing downstream reads.
    assert_eq!(data.level_basis, LevelBasis::Absolute);
}

#[test]
fn the_level_pin_feeds_whichever_property_is_live() {
    // One pin, not two: a second would always be inert. The consequence is that
    // flipping the mode silently changes what a wired value *means* — which is
    // why the subtitle spells the two out differently.
    let mut designer = setup_designer();
    let cube_id =
        add_loaded_import_cube_node(&mut designer, &cube_fixture("water_density_17x15x19.cube"));
    let iso_id = add_isosurface_node(
        &mut designer,
        IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level_fraction: 0.5,
            ..Default::default()
        },
    );
    designer.connect_nodes(cube_id, 0, iso_id, 0);

    let float_id = designer.add_node("float", DVec2::new(-400.0, 200.0));
    designer.set_node_network_data(float_id, Box::new(FloatData { value: 0.9 }));
    designer.connect_nodes(float_id, 0, iso_id, 2);

    let data = expect_isosurface(evaluate_pin(&designer, iso_id));
    assert_eq!(
        data.level_basis,
        LevelBasis::Fraction(0.9),
        "the wire drove the fraction, not the isovalue"
    );
}

#[test]
fn the_level_pin_is_ignored_under_auto() {
    let mut designer = setup_designer();
    let cube_id =
        add_loaded_import_cube_node(&mut designer, &cube_fixture("water_density_17x15x19.cube"));
    let iso_id = add_isosurface_node(&mut designer, IsosurfaceNodeData::default());
    designer.connect_nodes(cube_id, 0, iso_id, 0);

    // A value that would be a perfectly legal absolute level, so a node that
    // wrongly consumed it would still produce a surface and pass a weaker test.
    let float_id = designer.add_node("float", DVec2::new(-400.0, 200.0));
    designer.set_node_network_data(float_id, Box::new(FloatData { value: 0.05 }));
    designer.connect_nodes(float_id, 0, iso_id, 2);

    let data = expect_isosurface(evaluate_pin(&designer, iso_id));
    assert_eq!(data.level, DENSITY_LEVEL, "auto still chose the level");
    assert_eq!(data.level_basis, LevelBasis::Auto(AutoBasis::DensityLike));
}

#[test]
fn a_wired_level_pin_under_auto_is_a_non_blocking_warning() {
    // Warning, not error: the node still produces a good surface, so nothing
    // downstream may be blocked. What is wrong is only that the wire does
    // nothing, and the user has no other way to learn that.
    let data = IsosurfaceNodeData::default();
    assert!(
        data.get_data_error(&HashSet::new()).is_none(),
        "an unwired auto node has nothing to report"
    );

    let error = data
        .get_data_error(&HashSet::from(["level".to_string()]))
        .expect("a wired level pin under auto must be reported");
    assert!(!error.blocking, "the surface is still good — do not poison");
    assert!(
        error.message.contains("ignored in auto mode"),
        "got: {}",
        error.message
    );

    // Not reported in the modes that do consume it.
    for mode in [LevelMode::Absolute, LevelMode::Fraction] {
        let data = IsosurfaceNodeData {
            level_mode: mode,
            ..Default::default()
        };
        assert!(
            data.get_data_error(&HashSet::from(["level".to_string()]))
                .is_none(),
            "{mode:?} consumes the pin, so there is nothing to warn about"
        );
    }
}

// ============================================================================
// eval — the failure paths
// ============================================================================

#[test]
fn a_fraction_outside_the_open_unit_interval_is_rejected() {
    // Spelled-out NaN, matching the absolute check: `!(f > 0.0 && f < 1.0)`
    // would swallow it silently, and a wired `expr` produces one easily.
    for fraction in [0.0, 1.0, 1.5, -0.2, f64::NAN] {
        let data = IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level_fraction: fraction,
            ..Default::default()
        };
        let stored = data
            .get_data_error(&HashSet::new())
            .expect("validation must reject it without evaluating");
        assert!(stored.blocking, "there is no surface to draw");
        assert!(
            stored.message.contains("strictly between 0 and 1"),
            "got: {}",
            stored.message
        );

        let (designer, _, iso_id) = wired("water_density_17x15x19.cube", data);
        let message = expect_error(evaluate_pin(&designer, iso_id));
        assert!(
            message.contains("strictly between 0 and 1"),
            "eval must reject it too — validation never sees a wired value: {message}"
        );
    }
}

#[test]
fn an_all_zero_field_is_a_descriptive_error_in_both_field_reading_modes() {
    // Written to a temp file rather than committed: an all-zero `.cube` is a
    // perfectly valid file but a useless fixture, and it has to cross the loader
    // for this to be a *node* test rather than a restatement of
    // `auto_level_test`. Both modes must reach a descriptive error — not a
    // panic, and not a silent zero level, which the extractor would turn into an
    // empty mesh indistinguishable from a broken import.
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("all_zero.cube");
    std::fs::write(
        &path,
        "all-zero field\nno mass anywhere\n    1    0.000000    0.000000    0.000000\n\
             3    1.000000    0.000000    0.000000\n\
             3    0.000000    1.000000    0.000000\n\
             3    0.000000    0.000000    1.000000\n\
             1    1.000000    0.000000    0.000000    0.000000\n"
            .to_string()
            + &"  0.00000E+00".repeat(27),
    )
    .expect("write");

    for mode in [LevelMode::Auto, LevelMode::Fraction] {
        let mut designer = setup_designer();
        let cube_id = add_loaded_import_cube_node(&mut designer, path.to_str().unwrap());
        let iso_id = add_isosurface_node(
            &mut designer,
            IsosurfaceNodeData {
                level_mode: mode,
                ..Default::default()
            },
        );
        designer.connect_nodes(cube_id, 0, iso_id, 0);

        let message = expect_error(evaluate_pin(&designer, iso_id));
        assert!(
            message.contains("entirely zero"),
            "{mode:?} must say what is wrong, got: {message}"
        );
    }
}

#[test]
fn an_analytic_field_in_fraction_mode_says_the_mode_needs_samples() {
    // No node produces an analytic field yet — a future Molden orbital will be
    // the first — so the *message* is pinned where the field can be built, in
    // `auto_level_test`. What is pinned here is that the node reuses that one
    // wording rather than inventing its own.
    use atomcad_crystolecule::field::LevelResolutionError;
    let message = LevelResolutionError::AnalyticFieldInFractionMode.to_string();
    assert!(message.contains("stored samples"), "got: {message}");
    assert!(message.contains("absolute"), "got: {message}");
}

// ============================================================================
// The level does not depend on extraction quality
// ============================================================================

#[test]
fn the_resolved_level_does_not_move_with_the_quality_preference() {
    // The distribution is over **stored samples**, never the extraction
    // lattice. If it were not, a quality knob would change a semantic value —
    // the exact thing the sibling design splits preferences from node data for.
    let mut levels = Vec::new();
    for quality in [0.5, 1.0, 2.0] {
        let (mut designer, _, iso_id) = wired(
            "p2z_11x11x11.cube",
            IsosurfaceNodeData {
                level_mode: LevelMode::Fraction,
                level_fraction: 0.72,
                ..Default::default()
            },
        );
        let mut prefs = designer.preferences.clone();
        prefs
            .geometry_visualization_preferences
            .isosurface_quality_multiplier = quality;
        designer.set_preferences(prefs);

        levels.push(expect_isosurface(evaluate_pin(&designer, iso_id)).level);
    }
    assert_eq!(levels[0], levels[1]);
    assert_eq!(levels[1], levels[2]);
}

// ============================================================================
// Handover: auto -> fraction / absolute
// ============================================================================

#[test]
fn taking_over_from_auto_leaves_the_surface_where_it_was() {
    // The editor's mode dropdown pre-fills from the *resolved* pair, so the
    // surface must not move on the toggle. Both coordinates are exact, and the
    // dormant one is left untouched.
    let (designer, _, iso_id) = wired("p2z_11x11x11.cube", IsosurfaceNodeData::default());
    let auto = expect_isosurface(evaluate_pin(&designer, iso_id));
    let resolved_fraction = auto
        .field
        .value_distribution()
        .unwrap()
        .fraction_for_iso(auto.level)
        .unwrap();

    let (to_absolute, _, absolute_id) = wired(
        "p2z_11x11x11.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Absolute,
            level: auto.level,
            ..Default::default()
        },
    );
    assert_eq!(
        expect_isosurface(evaluate_pin(&to_absolute, absolute_id)).level,
        auto.level
    );

    let (to_fraction, _, fraction_id) = wired(
        "p2z_11x11x11.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level_fraction: resolved_fraction,
            // The dormant absolute number is deliberately left at its default —
            // the panel's greyed row must still show what it showed before.
            ..Default::default()
        },
    );
    let handed_over = expect_isosurface(evaluate_pin(&to_fraction, fraction_id));
    assert_eq!(
        handed_over.level, auto.level,
        "the fraction read back from the resolved level must resolve to it again"
    );
}

// ============================================================================
// Text format
// ============================================================================

fn text_props(data: &IsosurfaceNodeData) -> HashMap<String, TextValue> {
    data.get_text_properties().into_iter().collect()
}

#[test]
fn both_level_numbers_are_emitted_in_every_mode() {
    // Emitting only the live one would silently discard the other, and the two
    // properties exist precisely so the dormant one survives a mode toggle. The
    // text format is a round-trip path — copy/paste, the CLI, an AI edit.
    for mode in [LevelMode::Auto, LevelMode::Absolute, LevelMode::Fraction] {
        let data = IsosurfaceNodeData {
            level_mode: mode,
            level: 0.0035,
            level_fraction: 0.935,
            ..Default::default()
        };
        let props = text_props(&data);
        assert_eq!(props.get("level"), Some(&TextValue::Float(0.0035)));
        assert_eq!(props.get("level_fraction"), Some(&TextValue::Float(0.935)));
        assert!(matches!(
            props.get("level_mode"),
            Some(TextValue::String(_))
        ));

        let mut restored = IsosurfaceNodeData::default();
        restored.set_text_properties(&props).expect("reads back");
        assert_eq!(restored.level_mode, mode);
        assert_eq!(restored.level, 0.0035);
        assert_eq!(restored.level_fraction, 0.935);
    }
}

#[test]
fn naming_a_level_property_implies_its_mode() {
    // Without this, `isosurface { level: 0.002 }` would store into the dead slot
    // while a defaulted `Auto` ignored it and chose its own level.
    let mut absolute = IsosurfaceNodeData::default();
    absolute
        .set_text_properties(&HashMap::from([(
            "level".to_string(),
            TextValue::Float(0.002),
        )]))
        .expect("a lone level is legal");
    assert_eq!(absolute.level_mode, LevelMode::Absolute);
    assert_eq!(absolute.level, 0.002);

    let mut fraction = IsosurfaceNodeData::default();
    fraction
        .set_text_properties(&HashMap::from([(
            "level_fraction".to_string(),
            TextValue::Float(0.9),
        )]))
        .expect("a lone fraction is legal");
    assert_eq!(fraction.level_mode, LevelMode::Fraction);
    assert_eq!(fraction.level_fraction, 0.9);
}

#[test]
fn naming_both_level_properties_without_a_mode_is_an_error() {
    // An error naming both properties, not a silent precedence rule. The
    // emitted form never trips it, since that always names the mode.
    let mut data = IsosurfaceNodeData::default();
    let message = data
        .set_text_properties(&HashMap::from([
            ("level".to_string(), TextValue::Float(0.002)),
            ("level_fraction".to_string(), TextValue::Float(0.9)),
        ]))
        .expect_err("ambiguous");
    assert!(message.contains("level_mode"), "got: {message}");
    assert_eq!(
        data.level_mode,
        LevelMode::Auto,
        "a rejected edit must not have applied half of itself"
    );
}

#[test]
fn omission_cannot_reset_the_mode_to_auto() {
    // `set_text_properties` is applied to the *existing* node data and is only
    // called when at least one literal property is present, so `isosurface { }`
    // on a node already in fraction mode leaves it there. `level_mode: auto` is
    // the only way back.
    let mut data = IsosurfaceNodeData {
        level_mode: LevelMode::Fraction,
        level_fraction: 0.9,
        ..Default::default()
    };
    data.set_text_properties(&HashMap::from([(
        "alpha".to_string(),
        TextValue::Float(0.8),
    )]))
    .expect("an unrelated property is legal");
    assert_eq!(data.level_mode, LevelMode::Fraction);
    assert_eq!(data.level_fraction, 0.9);

    data.set_text_properties(&HashMap::from([(
        "level_mode".to_string(),
        TextValue::String("auto".to_string()),
    )]))
    .expect("naming it is legal");
    assert_eq!(data.level_mode, LevelMode::Auto);
}

#[test]
fn an_unknown_level_mode_spelling_is_rejected_by_name() {
    let mut data = IsosurfaceNodeData::default();
    let message = data
        .set_text_properties(&HashMap::from([(
            "level_mode".to_string(),
            TextValue::String("automatic".to_string()),
        )]))
        .expect_err("not a spelling");
    assert!(message.contains("automatic"), "got: {message}");
    assert!(
        message.contains("auto, absolute, fraction"),
        "got: {message}"
    );
}

// ============================================================================
// Subtitle and readout
// ============================================================================

#[test]
fn the_subtitle_names_the_mode_and_the_live_number() {
    let auto = IsosurfaceNodeData::default();
    assert_eq!(auto.get_subtitle(&HashSet::new()), Some("auto".to_string()));
    assert_eq!(
        auto.get_subtitle(&HashSet::from(["level".to_string()])),
        Some("auto".to_string()),
        "the wire is ignored under auto, so there is nothing for it to hide"
    );

    let fraction = IsosurfaceNodeData {
        level_mode: LevelMode::Fraction,
        level_fraction: 0.72,
        ..Default::default()
    };
    assert_eq!(
        fraction.get_subtitle(&HashSet::new()),
        Some("level: 72.0% (fraction)".to_string()),
        "spelled differently from an isovalue on purpose — 0.02 is a valid fraction too"
    );
}

#[test]
fn the_readout_states_what_the_surface_encloses() {
    let (designer, _, iso_id) = wired("water_density_17x15x19.cube", IsosurfaceNodeData::default());
    let result = evaluate_pin(&designer, iso_id);
    let shown = result.to_display_string();

    // Load-bearing wording: never "% of the electron density". On an orbital
    // amplitude the conventionally enclosed quantity is the square of the
    // amplitude, so the friendlier paraphrase would be false.
    assert!(shown.contains("of ∫|v|"), "got: {shown}");
    assert!(
        !shown.contains("electron density"),
        "the paraphrase the design forbids: {shown}"
    );
    // ... and no unit suffix, since two of the fields this must serve (ELF, RDG)
    // are dimensionless.
    assert!(!shown.contains("a.u."), "got: {shown}");
    assert!(
        shown.contains("auto: non-negative, density-like"),
        "a guess must be visible as a guess: {shown}"
    );

    let detailed = result.to_detailed_string();
    assert!(detailed.contains("of ∫|v|"), "got: {detailed}");
    assert!(
        detailed.contains("auto: non-negative, density-like"),
        "got: {detailed}"
    );

    // The percentage is exactly `fraction_for_iso` of the resolved level, to one
    // decimal, in every mode.
    let data = expect_isosurface(result);
    let fraction = data
        .field
        .value_distribution()
        .unwrap()
        .fraction_for_iso(data.level)
        .unwrap();
    assert!(
        shown.contains(&format!("encloses {:.1}% of ∫|v|", fraction * 100.0)),
        "got: {shown}"
    );
}

#[test]
fn the_basis_is_absent_outside_auto() {
    let (designer, _, iso_id) = wired(
        "water_density_17x15x19.cube",
        IsosurfaceNodeData {
            level_mode: LevelMode::Absolute,
            level: 0.002,
            ..Default::default()
        },
    );
    let shown = evaluate_pin(&designer, iso_id).to_display_string();
    assert!(shown.contains("of ∫|v|"), "got: {shown}");
    assert!(
        !shown.contains("auto:"),
        "there is no guess to disclose here: {shown}"
    );
}
