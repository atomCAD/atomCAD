//! Table D of `doc/design_isosurface_node.md` P2: **lattice and resolution.**
//!
//! The rows that hold the design's §Resolution ruling in place: march the
//! field's own index space on `GridGeometry::axes`, subdivide by an integer,
//! flip the winding when the axis triple is left-handed, fall back to a spacing
//! only when there is no native grid, and refuse an over-budget grid before
//! touching it.

use std::sync::Arc;

use atomcad_crystolecule::field::{GridGeometry, ScalarField};
use atomcad_display::isosurface::{
    ExtractionError, ExtractionSettings, Lattice, extract_isosurface,
};
use glam::DVec3;

use crate::isosurface_common::{
    BUMP_LEVEL, assert_surface_invariants, bump_radius, extract, phase_data, sample_onto_grid,
    sphere_bump,
};

/// A grid with genuine shear: every axis leans on the ones before it, and none
/// of the three lengths tells you where the samples are.
fn sheared_grid() -> GridGeometry {
    let axes = [
        DVec3::new(0.25, 0.0, 0.0),
        DVec3::new(0.08, 0.25, 0.0),
        DVec3::new(0.05, 0.06, 0.25),
    ];
    let steps = 24.0;
    GridGeometry {
        origin: -(axes[0] + axes[1] + axes[2]) * steps * 0.5,
        axes,
        dims: [25, 25, 25],
    }
}

/// Same, but with a left-handed axis triple — which `.cube` writers do emit.
fn left_handed_grid() -> GridGeometry {
    GridGeometry {
        origin: DVec3::new(-3.0, -3.0, 3.0),
        axes: [
            DVec3::new(0.25, 0.0, 0.0),
            DVec3::new(0.0, 0.25, 0.0),
            DVec3::new(0.0, 0.0, -0.25),
        ],
        dims: [25, 25, 25],
    }
}

#[test]
fn a_field_with_no_native_grid_uses_the_fallback_spacing() {
    let field = sphere_bump();
    assert!(
        field.native_grid().is_none(),
        "the fixture must be analytic"
    );

    let settings = ExtractionSettings {
        fallback_spacing: 0.2,
        ..ExtractionSettings::default()
    };
    let lattice = Lattice::choose(field.as_ref(), &settings);
    let bounds = field.suggested_bounds();
    assert_eq!(
        lattice.cell_dims(),
        [(bounds.size().x / 0.2).ceil() as usize; 3],
        "the fallback must tile suggested_bounds() at the fallback spacing"
    );

    let mesh = extract_isosurface(&phase_data(field.clone(), BUMP_LEVEL), &settings)
        .expect("within budget");
    assert_surface_invariants(&mesh, field.as_ref(), BUMP_LEVEL, 0.05, "fallback lattice");
    assert!(!mesh.is_empty());
}

#[test]
fn a_sheared_grid_places_vertices_by_its_axes_not_its_spacings() {
    // The assertion that fails if `GridGeometry::spacing()` was used instead of
    // `axes`: rebuilding an axis-aligned lattice from three lengths rotates the
    // samples into the wrong places, and the vertices land nowhere near `R`.
    let analytic = sphere_bump();
    let field: Arc<dyn ScalarField> =
        Arc::new(sample_onto_grid(sheared_grid(), |p| analytic.sample(p)));
    let mesh = extract(field.clone(), BUMP_LEVEL);
    assert!(!mesh.is_empty());

    let expected = bump_radius(BUMP_LEVEL);
    for position in &mesh.positions {
        let radius = position.as_dvec3().length();
        assert!(
            (radius - expected).abs() < 0.05,
            "vertex at radius {radius}, expected {expected}"
        );
    }
}

#[test]
fn a_sheared_grid_stays_closed_and_wound_outward() {
    let analytic = sphere_bump();
    let field: Arc<dyn ScalarField> =
        Arc::new(sample_onto_grid(sheared_grid(), |p| analytic.sample(p)));
    let mesh = extract(field.clone(), BUMP_LEVEL);
    assert_surface_invariants(&mesh, field.as_ref(), BUMP_LEVEL, 0.05, "sheared");
    assert_eq!(mesh.components.len(), 1);
}

#[test]
fn a_left_handed_axis_triple_flips_the_winding() {
    let grid = left_handed_grid();
    let lattice = Lattice::Native { grid, subdiv: 1 };
    assert!(
        lattice.flips_winding(),
        "the fixture must actually be left-handed"
    );

    let analytic = sphere_bump();
    let field: Arc<dyn ScalarField> = Arc::new(sample_onto_grid(grid, |p| analytic.sample(p)));
    let mesh = extract(field.clone(), BUMP_LEVEL);
    // `assert_outward_winding` is a signed volume, so it is exactly the
    // "triangles came out mirrored" detector. Without the flip it is negative.
    assert_surface_invariants(&mesh, field.as_ref(), BUMP_LEVEL, 0.05, "left-handed");
    assert!(!mesh.is_empty());
}

#[test]
fn subdiv_one_marches_the_stored_samples() {
    let grid = sheared_grid();
    let lattice = Lattice::Native { grid, subdiv: 1 };
    for i in 0..grid.dims[0] {
        for j in 0..grid.dims[1] {
            for k in 0..grid.dims[2] {
                assert_eq!(
                    lattice.corner_position(i, j, k),
                    grid.sample_position(i, j, k),
                    "cell corner ({i}, {j}, {k}) is not a stored sample point"
                );
            }
        }
    }
    assert_eq!(lattice.cell_dims(), [24, 24, 24]);
}

#[test]
fn subdiv_two_halves_the_step_and_keeps_the_surface_closed() {
    let grid = GridGeometry {
        origin: DVec3::splat(-2.0),
        axes: [
            DVec3::new(0.4, 0.0, 0.0),
            DVec3::new(0.0, 0.4, 0.0),
            DVec3::new(0.0, 0.0, 0.4),
        ],
        dims: [11, 11, 11],
    };
    let lattice = Lattice::Native { grid, subdiv: 2 };
    assert_eq!(lattice.cell_dims(), [20, 20, 20]);
    assert_eq!(lattice.cell_count(), 8000);

    let analytic = sphere_bump();
    let field: Arc<dyn ScalarField> = Arc::new(sample_onto_grid(grid, |p| analytic.sample(p)));
    let settings = ExtractionSettings {
        quality_multiplier: 2.0,
        ..ExtractionSettings::default()
    };
    let mesh = extract_isosurface(&phase_data(field.clone(), BUMP_LEVEL), &settings)
        .expect("within budget");
    assert_surface_invariants(&mesh, field.as_ref(), BUMP_LEVEL, 0.05, "subdiv 2");

    // Finer cells, more triangles — the visible half of "resolution lives
    // outside the value".
    let coarse = extract(field.clone(), BUMP_LEVEL);
    assert!(
        mesh.triangle_count() > coarse.triangle_count(),
        "subdiv 2 gave {} triangles against {} at subdiv 1",
        mesh.triangle_count(),
        coarse.triangle_count()
    );
}

#[test]
fn the_quality_multiplier_rounds_and_clamps() {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims: [5, 5, 5],
    };
    let field = sample_onto_grid(grid, |p| p.x);
    let subdiv_for = |multiplier: f64| {
        let settings = ExtractionSettings {
            quality_multiplier: multiplier,
            ..ExtractionSettings::default()
        };
        match Lattice::choose(&field, &settings) {
            Lattice::Native { subdiv, .. } => subdiv,
            Lattice::Fallback { .. } => panic!("a sampled field has a native grid"),
        }
    };
    // Below 1 does not coarsen: the field's own grid is the floor.
    assert_eq!(subdiv_for(0.5), 1);
    assert_eq!(subdiv_for(1.0), 1);
    assert_eq!(subdiv_for(2.4), 2);
    assert_eq!(subdiv_for(2.5), 3);
    // And a garbage value from a hand-edited preferences file does not panic.
    assert_eq!(subdiv_for(f64::NAN), 1);
    assert_eq!(subdiv_for(-4.0), 1);
}

#[test]
fn an_axis_of_one_sample_yields_no_cells() {
    // `SampledField::new` accepts a `dims` of 1 — only `0` is rejected — so the
    // extractor has to survive it.
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims: [5, 1, 5],
    };
    let field: Arc<dyn ScalarField> = Arc::new(sample_onto_grid(grid, |p| p.x));
    let lattice = Lattice::Native { grid, subdiv: 1 };
    assert_eq!(lattice.cell_dims(), [4, 0, 4]);
    assert_eq!(lattice.cell_count(), 0);

    let mesh = extract(field, 0.5);
    assert!(mesh.is_empty(), "a degenerate grid produced geometry");
}

#[test]
fn an_over_budget_native_grid_is_refused_before_allocating() {
    let grid = GridGeometry {
        origin: DVec3::splat(-2.0),
        axes: [
            DVec3::new(0.08, 0.0, 0.0),
            DVec3::new(0.0, 0.08, 0.0),
            DVec3::new(0.0, 0.0, 0.08),
        ],
        dims: [50, 50, 50],
    };
    let analytic = sphere_bump();
    let field: Arc<dyn ScalarField> = Arc::new(sample_onto_grid(grid, |p| analytic.sample(p)));
    let settings = ExtractionSettings {
        quality_multiplier: 8.0,
        ..ExtractionSettings::default()
    };
    // (50 - 1) * 8 = 392 cells per axis.
    let cells = 392u128 * 392 * 392;
    assert!(cells > settings.cell_budget as u128);

    let error =
        extract_isosurface(&phase_data(field, BUMP_LEVEL), &settings).expect_err("over budget");
    assert_eq!(
        error,
        ExtractionError::CellBudgetExceeded {
            cells,
            budget: 16_000_000,
            quality_multiplier: 8.0,
        }
    );

    // All three moving parts are named, because none of them is a node property
    // the user can see from the node.
    let message = error.to_string();
    assert!(message.contains("60,236,288"), "{message}");
    assert!(message.contains("16,000,000"), "{message}");
    assert!(
        message.contains("isosurface_quality_multiplier (currently 8)"),
        "{message}"
    );
    assert!(message.contains("isosurface_cell_budget"), "{message}");
}

#[test]
fn an_over_budget_fallback_lattice_is_refused_before_allocating() {
    // The analytic branch has no natural ceiling at all, so the budget is the
    // only guard. Actually marching this would take longer than the heat death
    // of the sun; returning promptly is the proof that the check is pre-flight.
    let settings = ExtractionSettings {
        fallback_spacing: 0.0001,
        ..ExtractionSettings::default()
    };
    let error = extract_isosurface(&phase_data(sphere_bump(), BUMP_LEVEL), &settings)
        .expect_err("over budget");
    let ExtractionError::CellBudgetExceeded { cells, budget, .. } = error;
    assert_eq!(cells, 60_000u128 * 60_000 * 60_000);
    assert_eq!(budget, 16_000_000);
}
