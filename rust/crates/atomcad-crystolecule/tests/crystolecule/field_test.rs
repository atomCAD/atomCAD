//! `ScalarField` / `SampledField` behaviour on hand-built grids.
//!
//! Everything here constructs its grid in memory, so it exercises the contract
//! itself — bounds convention, interpolation, the out-of-bounds rule, the
//! trait's *default* gradient — with no file format in the way. The
//! fixture-driven half lives in `io/cube_loader_test.rs`.

use atomcad_crystolecule::field::{
    DEFAULT_GRADIENT_STEP, FieldBounds, FieldError, GridGeometry, SampledField, ScalarField,
};
use glam::DVec3;

/// An axis-aligned grid with unit spacing along each axis, origin at the first
/// sample, and `value(i, j, k) = 100i + 10j + k`.
fn unit_ramp(dims: [usize; 3]) -> SampledField {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims,
    };
    let mut samples = Vec::with_capacity(grid.sample_count());
    for i in 0..dims[0] {
        for j in 0..dims[1] {
            for k in 0..dims[2] {
                samples.push((100 * i + 10 * j + k) as f32);
            }
        }
    }
    SampledField::new(grid, samples).unwrap()
}

// --- bounds convention ------------------------------------------------------

#[test]
fn bounds_run_through_the_outermost_samples_not_half_a_voxel_past_them() {
    // Node-centered: 3x4x5 samples span 2x3x4 steps, NOT 3x4x5.
    let field = unit_ramp([3, 4, 5]);
    let bounds = field.suggested_bounds();
    assert_eq!(bounds.min, DVec3::ZERO);
    assert_eq!(bounds.max, DVec3::new(2.0, 3.0, 4.0));
    assert_eq!(field.data_bounds(), Some(bounds));
}

#[test]
fn bounds_of_a_negative_axis_grid_are_still_min_max_ordered() {
    // The `.cube` format permits a negative step vector; `bounds()` must return
    // an ordered AABB rather than a min that is larger than its max.
    let grid = GridGeometry {
        origin: DVec3::new(5.0, 0.0, 0.0),
        axes: [-DVec3::X, DVec3::Y, DVec3::Z],
        dims: [3, 2, 2],
    };
    let bounds = grid.bounds();
    assert_eq!(bounds.min, DVec3::new(3.0, 0.0, 0.0));
    assert_eq!(bounds.max, DVec3::new(5.0, 1.0, 1.0));
}

#[test]
fn spacing_and_sample_position_describe_the_same_lattice() {
    let grid = GridGeometry {
        origin: DVec3::new(-1.0, -2.0, -3.0),
        axes: [DVec3::X * 0.5, DVec3::Y * 0.25, DVec3::Z * 2.0],
        dims: [4, 4, 4],
    };
    assert_eq!(grid.spacing(), DVec3::new(0.5, 0.25, 2.0));
    assert_eq!(
        grid.sample_position(2, 1, 3),
        DVec3::new(-1.0 + 1.0, -2.0 + 0.25, -3.0 + 6.0)
    );
    assert_eq!(grid.sample_count(), 64);
}

#[test]
fn field_bounds_contains_is_inclusive_on_both_faces() {
    let bounds = FieldBounds::new(DVec3::ZERO, DVec3::splat(1.0));
    assert!(bounds.contains(DVec3::ZERO));
    assert!(bounds.contains(DVec3::splat(1.0)));
    assert!(!bounds.contains(DVec3::new(1.0, 1.0, 1.000_001)));
    assert_eq!(bounds.size(), DVec3::splat(1.0));
    assert_eq!(bounds.center(), DVec3::splat(0.5));
}

// --- sampling ---------------------------------------------------------------

#[test]
fn sampling_at_an_exact_grid_point_returns_that_stored_sample() {
    let field = unit_ramp([3, 4, 5]);
    for i in 0..3 {
        for j in 0..4 {
            for k in 0..5 {
                let expected = (100 * i + 10 * j + k) as f64;
                let point = DVec3::new(i as f64, j as f64, k as f64);
                assert_eq!(field.sample(point), expected, "at ({i},{j},{k})");
            }
        }
    }
}

#[test]
fn trilinear_interpolation_blends_the_eight_surrounding_samples() {
    let field = unit_ramp([3, 4, 5]);
    // Halfway along one axis at a time reads off the ramp coefficients directly.
    assert_eq!(field.sample(DVec3::new(0.5, 0.0, 0.0)), 50.0);
    assert_eq!(field.sample(DVec3::new(0.0, 0.5, 0.0)), 5.0);
    assert_eq!(field.sample(DVec3::new(0.0, 0.0, 0.5)), 0.5);
    // The centre of one cell is the mean of its eight corners:
    // (0 + 1 + 10 + 11 + 100 + 101 + 110 + 111) / 8 = 55.5
    assert_eq!(field.sample(DVec3::splat(0.5)), 55.5);
}

#[test]
fn sampling_outside_the_box_returns_exactly_zero() {
    let field = unit_ramp([3, 4, 5]);
    for point in [
        DVec3::new(-0.001, 0.0, 0.0),
        DVec3::new(2.001, 0.0, 0.0),
        DVec3::new(0.0, -1.0, 0.0),
        DVec3::new(0.0, 0.0, 4.001),
        DVec3::splat(1000.0),
    ] {
        assert_eq!(field.sample(point), 0.0, "at {point}");
    }
    // Exactly on the far face is still inside.
    assert_eq!(field.sample(DVec3::new(2.0, 3.0, 4.0)), 234.0);
}

#[test]
fn a_point_a_float_hair_past_the_far_face_still_reads_the_boundary_sample() {
    // A `.cube` writer emits its step vector in Bohr with six decimals, so the
    // Ångström spacing this fixture *nominally* has (1.0) round-trips to
    // 0.999_999_934. The last sample plane then sits a few times 1e-7 Å inside
    // where the user thinks it is, and a knife-edge bounds test would answer a
    // sample at the nominal position with 0.0 — a jump of the whole data range
    // from a rounding error nobody can see. See `BOUNDARY_INDEX_TOLERANCE`.
    let spacing = 1.889_726 * 0.529_177_210_903;
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X * spacing, DVec3::Y * spacing, DVec3::Z * spacing],
        dims: [3, 4, 5],
    };
    let mut samples = Vec::with_capacity(grid.sample_count());
    for i in 0..3 {
        for j in 0..4 {
            for k in 0..5 {
                samples.push((100 * i + 10 * j + k) as f32);
            }
        }
    }
    let field = SampledField::new(grid, samples).unwrap();

    assert!(
        spacing < 1.0,
        "the round-trip should land just short of 1.0"
    );
    assert_eq!(field.sample(DVec3::new(2.0, 3.0, 4.0)), 234.0);
    assert_eq!(field.sample(DVec3::new(0.0, 0.0, 4.0)), 4.0);

    // The tolerance is a hair, not a margin: a point genuinely off the end is
    // still out of bounds.
    assert_eq!(field.sample(DVec3::new(0.0, 0.0, 4.01)), 0.0);
    assert_eq!(field.sample(DVec3::new(-0.01, 0.0, 0.0)), 0.0);
}

#[test]
fn sampling_a_non_finite_point_returns_zero_rather_than_a_bogus_index() {
    let field = unit_ramp([3, 4, 5]);
    assert_eq!(field.sample(DVec3::new(f64::NAN, 0.0, 0.0)), 0.0);
    assert_eq!(field.sample(DVec3::new(f64::INFINITY, 0.0, 0.0)), 0.0);
}

#[test]
fn a_one_sample_wide_axis_is_sampleable() {
    // A 1-wide axis has no `i0 + 1` to blend towards; the plane through it must
    // still read back rather than panic.
    let field = unit_ramp([1, 4, 5]);
    assert_eq!(field.sample(DVec3::new(0.0, 1.0, 2.0)), 12.0);
    assert_eq!(field.sample(DVec3::new(0.0, 1.5, 2.0)), 17.0);
    assert_eq!(field.sample(DVec3::new(0.5, 1.0, 2.0)), 0.0);
    assert_eq!(field.suggested_bounds().max, DVec3::new(0.0, 3.0, 4.0));
}

#[test]
fn sample_batch_agrees_with_sample() {
    let field = unit_ramp([3, 4, 5]);
    let points = [
        DVec3::new(0.25, 1.5, 2.0),
        DVec3::new(2.0, 3.0, 4.0),
        DVec3::splat(-1.0),
    ];
    let mut out = [0.0; 3];
    field.sample_batch(&points, &mut out);
    for (point, value) in points.iter().zip(out.iter()) {
        assert_eq!(*value, field.sample(*point));
    }
}

// --- gradient ---------------------------------------------------------------

#[test]
fn gradient_of_a_linear_ramp_is_its_exact_slope() {
    let field = unit_ramp([3, 4, 5]);
    let gradient = field.gradient(DVec3::new(1.0, 1.0, 1.0));
    assert!(
        (gradient - DVec3::new(100.0, 10.0, 1.0)).length() < 1e-9,
        "got {gradient}"
    );
}

#[test]
fn gradient_scales_with_the_grid_spacing_not_the_index_step() {
    // Same values, half the spacing: the real-space gradient doubles.
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X * 0.5, DVec3::Y * 0.5, DVec3::Z * 0.5],
        dims: [3, 4, 5],
    };
    let mut samples = Vec::with_capacity(grid.sample_count());
    for i in 0..3 {
        for j in 0..4 {
            for k in 0..5 {
                samples.push((100 * i + 10 * j + k) as f32);
            }
        }
    }
    let field = SampledField::new(grid, samples).unwrap();
    let gradient = field.gradient(DVec3::splat(0.5));
    assert!(
        (gradient - DVec3::new(200.0, 20.0, 2.0)).length() < 1e-9,
        "got {gradient}"
    );
}

#[test]
fn gradient_outside_the_box_is_zero() {
    let field = unit_ramp([3, 4, 5]);
    assert_eq!(field.gradient(DVec3::splat(-5.0)), DVec3::ZERO);
}

/// A field that deliberately does NOT override `gradient`, so the trait's
/// default central difference is what gets exercised. It reports no native
/// grid, which is also the analytic-source shape Molden will need.
#[derive(Debug)]
struct QuadraticField;

impl ScalarField for QuadraticField {
    fn sample(&self, point: DVec3) -> f64 {
        point.x * point.x + 2.0 * point.y + 3.0 * point.z
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
        None
    }
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

#[test]
fn the_default_gradient_central_differences_at_the_documented_step() {
    let field = QuadraticField;
    let gradient = field.gradient(DVec3::new(2.0, 0.0, 0.0));
    // d/dx of x^2 at x = 2 is 4, and a central difference of a quadratic is
    // exact regardless of step size.
    assert!((gradient.x - 4.0).abs() < 1e-9, "got {gradient}");
    assert!((gradient.y - 2.0).abs() < 1e-9, "got {gradient}");
    assert!((gradient.z - 3.0).abs() < 1e-9, "got {gradient}");

    // …and the step it used really is the documented fallback: a step of h
    // makes the *linear* terms exact but leaves a signature in a cubic one.
    let cubed = CubicField;
    let expected = 3.0 * 4.0 + DEFAULT_GRADIENT_STEP * DEFAULT_GRADIENT_STEP;
    assert!(
        (cubed.gradient(DVec3::new(2.0, 0.0, 0.0)).x - expected).abs() < 1e-9,
        "the default gradient should step at DEFAULT_GRADIENT_STEP"
    );
}

/// `x^3`, whose central difference at step `h` is `3x^2 + h^2` — so the error
/// term names the step the default `gradient` actually used.
#[derive(Debug)]
struct CubicField;

impl ScalarField for CubicField {
    fn sample(&self, point: DVec3) -> f64 {
        point.x * point.x * point.x
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
        None
    }
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

#[test]
fn an_unbounded_analytic_style_field_reports_none_for_the_optional_halves() {
    // The two asymmetries the contract exists to absorb. A consumer that
    // assumes `Some` here is the one that will break on the first Molden field.
    let field = QuadraticField;
    assert!(field.data_bounds().is_none());
    assert!(field.native_grid().is_none());
    assert!(field.value_range().is_none());
    assert_eq!(field.suggested_bounds().max, DVec3::splat(1.0));
}

// --- value range and construction errors ------------------------------------

#[test]
fn value_range_spans_the_stored_samples() {
    let field = unit_ramp([3, 4, 5]);
    assert_eq!(field.value_range(), Some((0.0, 234.0)));
}

#[test]
fn native_grid_round_trips_the_construction_geometry() {
    let field = unit_ramp([3, 4, 5]);
    let grid = field.native_grid().expect("sampled fields have a grid");
    assert_eq!(grid.dims, [3, 4, 5]);
    assert_eq!(grid.origin, DVec3::ZERO);
    assert_eq!(grid.axes, [DVec3::X, DVec3::Y, DVec3::Z]);
    assert_eq!(grid, field.grid());
}

#[test]
fn a_zero_dimension_is_rejected() {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims: [3, 0, 5],
    };
    assert_eq!(
        SampledField::new(grid, vec![]).unwrap_err(),
        FieldError::ZeroDimension { axis: 1 }
    );
}

#[test]
fn a_sample_count_mismatch_is_rejected() {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims: [2, 2, 2],
    };
    assert_eq!(
        SampledField::new(grid, vec![0.0; 7]).unwrap_err(),
        FieldError::SampleCountMismatch {
            dims: [2, 2, 2],
            expected: 8,
            actual: 7,
        }
    );
}

#[test]
fn coplanar_axis_vectors_are_rejected() {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::X + DVec3::Y],
        dims: [2, 2, 2],
    };
    assert_eq!(
        SampledField::new(grid, vec![0.0; 8]).unwrap_err(),
        FieldError::DegenerateAxes
    );
}

#[test]
fn a_non_finite_sample_is_rejected() {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims: [2, 2, 2],
    };
    let mut samples = vec![0.0f32; 8];
    samples[3] = f32::NAN;
    assert!(matches!(
        SampledField::new(grid, samples),
        Err(FieldError::NonFiniteSample { index: 3, .. })
    ));
}

#[test]
fn debug_prints_a_summary_and_never_the_samples() {
    let field = unit_ramp([3, 4, 5]);
    let printed = format!("{field:?}");
    assert!(printed.contains("3x4x5"), "{printed}");
    assert!(!printed.contains("111"), "{printed}");
}

// --- sheared grids ----------------------------------------------------------

#[test]
fn a_sheared_grid_samples_and_differentiates_in_real_space() {
    // The `.cube` format permits non-orthogonal axes even though PySCF never
    // writes them. `value = i` in index space, with the j axis tilted into x.
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Z],
        dims: [3, 3, 2],
    };
    let mut samples = Vec::with_capacity(grid.sample_count());
    for i in 0..3 {
        for _j in 0..3 {
            for _k in 0..2 {
                samples.push(i as f32);
            }
        }
    }
    let field = SampledField::new(grid, samples).unwrap();

    // Sample (1, 2, 0) sits at (1,0,0) + 2*(1,1,0) = (3, 2, 0) and holds i = 1.
    assert_eq!(field.sample(DVec3::new(3.0, 2.0, 0.0)), 1.0);
    // In real space the stored value is `x - y`, so the gradient is (1, -1, 0).
    // Dropping the transpose when mapping the index-space gradient back would
    // give (1, 0, 0) here, which is why the shear case is worth a test at all.
    let gradient = field.gradient(DVec3::new(1.5, 0.5, 0.0));
    assert!(
        (gradient - DVec3::new(1.0, -1.0, 0.0)).length() < 1e-9,
        "{gradient}"
    );
}

// ---------------------------------------------------------------------------
// `ScalarField::estimate_memory_bytes` (`doc/design_eval_memoization.md` D6 R1)
// ---------------------------------------------------------------------------

#[test]
fn a_sampled_field_sizes_with_its_grid() {
    // The whole reason the method is on the trait: from outside, a small field
    // and a large one are indistinguishable, yet the grid is the largest single
    // payload a memory-bounded cache holding field values can hold.
    let small = unit_ramp([3, 3, 3]);
    let large = unit_ramp([30, 30, 30]);

    let small_bytes = small.estimate_memory_bytes();
    let large_bytes = large.estimate_memory_bytes();

    assert!(
        large_bytes > small_bytes,
        "a 1000x larger grid must size above a small one: {large_bytes} vs {small_bytes}"
    );

    // The *difference* must account for the extra samples. Stated as a
    // difference rather than a ratio because a 27-sample grid is dominated by
    // the struct header, which would dilute any ratio at the small end without
    // saying anything about the estimator.
    let extra_samples = 30 * 30 * 30 - 3 * 3 * 3;
    assert!(
        large_bytes - small_bytes >= extra_samples * std::mem::size_of::<f32>(),
        "the estimate must cover the extra sample storage"
    );

    // And the estimate must at least cover the raw sample array.
    assert!(large_bytes >= 30 * 30 * 30 * std::mem::size_of::<f32>());
}

#[test]
fn a_field_reached_through_the_trait_object_still_reports_its_grid() {
    // How the evaluator actually holds a field: `Arc<dyn ScalarField>`.
    use std::sync::Arc;
    let field: Arc<dyn ScalarField> = Arc::new(unit_ramp([8, 8, 8]));
    assert!(field.estimate_memory_bytes() >= 8 * 8 * 8 * std::mem::size_of::<f32>());
}
