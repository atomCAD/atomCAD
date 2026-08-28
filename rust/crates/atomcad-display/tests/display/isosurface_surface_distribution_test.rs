//! The surface-restricted colour distribution and the two fits
//! (`doc/design_isosurface_level.md` Part 4, P4).
//!
//! Three of these rows are the ones the design says cannot be skipped, because
//! without a *constructed* answer they would assert nothing:
//!
//! - **area weighting** — a mesh whose two halves carry equal area but a 4:1
//!   triangle count. "Different from the count-weighted answer, and correct" is
//!   only checkable against a geometry whose right answer is known by hand.
//! - **the vertex floor** — the fit must *refuse*, not write a plausible wrong
//!   number, which is the one path by which a quality preference could still be
//!   baked into a saved `.cnnd`.
//! - **quality independence** — the same fit at two extraction multipliers.

use std::sync::Arc;

use atomcad_crystolecule::field::{
    GridGeometry, IsosurfaceColoring, IsosurfaceData, LevelBasis, ScalarField,
};
use atomcad_display::isosurface::surface_distribution::MIN_FIT_VERTICES;
use atomcad_display::isosurface::{
    ExtractionSettings, SurfaceComponent, SurfaceMesh, SurfaceValueDistribution,
    extract_isosurface, surface_color_distribution,
};
use glam::{DVec3, Vec3};

use crate::isosurface_common::{AnalyticField, linear_ramp, sample_onto_grid, sphere_bump};

// ============================================================================
// Helpers
// ============================================================================

fn field_data(
    field: Arc<dyn ScalarField>,
    level: f64,
    color_field: Arc<dyn ScalarField>,
) -> IsosurfaceData {
    IsosurfaceData {
        field,
        level,
        level_basis: LevelBasis::Absolute,
        coloring: IsosurfaceColoring::Field {
            field: color_field,
            range: (-1.0, 1.0),
            colormap: Default::default(),
        },
        alpha: 1.0,
    }
}

fn settings(quality_multiplier: f64) -> ExtractionSettings {
    ExtractionSettings {
        fallback_spacing: 0.12,
        quality_multiplier,
        ..ExtractionSettings::default()
    }
}

/// A flat rectangular strip in the `z = 0` plane spanning `x` in `[0, 1]`,
/// `y` in `[0, 1]`, whose two halves carry **equal area** and unequal triangle
/// counts.
///
/// The left half is one column of quads, the right half `fine_columns` of them,
/// so the fine half holds `fine_columns` times as many triangles over the same
/// area. This is the whole point: an area-weighted median lands at the
/// geometric midpoint `x = 0.5` regardless, and a vertex-count-weighted one is
/// pulled toward the fine half.
fn split_resolution_strip(fine_columns: usize) -> SurfaceMesh {
    let mut positions: Vec<Vec3> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Column boundaries in x: one coarse cell over [0, 0.5], then `fine_columns`
    // equal cells over [0.5, 1].
    let mut boundaries = vec![0.0f32, 0.5];
    for column in 1..=fine_columns {
        boundaries.push(0.5 + 0.5 * column as f32 / fine_columns as f32);
    }

    for x in &boundaries {
        let base = positions.len() as u32;
        positions.push(Vec3::new(*x, 0.0, 0.0));
        positions.push(Vec3::new(*x, 1.0, 0.0));
        if base >= 2 {
            // Two triangles closing the quad against the previous column.
            indices.extend_from_slice(&[base - 2, base - 1, base]);
            indices.extend_from_slice(&[base - 1, base + 1, base]);
        }
    }

    let count = indices.len() as u32;
    SurfaceMesh {
        normals: vec![Vec3::Z; positions.len()],
        albedo: vec![Vec3::ONE; positions.len()],
        components: vec![SurfaceComponent {
            first_index: 0,
            index_count: count,
            centroid: Vec3::new(0.5, 0.5, 0.0),
        }],
        positions,
        indices,
        alpha: 1.0,
    }
}

/// The median of the colour values weighted by **vertex count** — the wrong
/// answer this design exists to avoid, computed here so the right one has
/// something to be different from.
fn count_weighted_median(mesh: &SurfaceMesh, color: &dyn ScalarField) -> f64 {
    let mut values: Vec<f64> = mesh
        .positions
        .iter()
        .map(|p| color.sample(p.as_dvec3()))
        .collect();
    values.sort_by(|a, b| a.total_cmp(b));
    values[values.len() / 2]
}

// ============================================================================
// Area weighting
// ============================================================================

#[test]
fn the_median_is_weighted_by_area_not_by_vertex_count() {
    let mesh = split_resolution_strip(8);
    let color = linear_ramp();
    let data = field_data(sphere_bump(), 0.6, color.clone());

    let distribution =
        surface_color_distribution(&mesh, &data).expect("a field-coloured mesh has a distribution");

    // Equal areas either side of x = 0.5, and the colour is x, so half the area
    // lies below 0.5. Bin resolution is what the few percent of slack is for.
    assert!(
        (distribution.p50() - 0.5).abs() < 0.03,
        "area-weighted median should sit at the geometric midpoint, got {}",
        distribution.p50()
    );

    // The counter-example: the fine half holds 8x the vertices over the same
    // area, so a count-weighted median is dragged well into it.
    let count_median = count_weighted_median(&mesh, color.as_ref());
    assert!(
        count_median > 0.6,
        "the count-weighted median should be pulled toward the fine half; \
         got {count_median}, which would make this test assert nothing"
    );
}

// ============================================================================
// The two fits
// ============================================================================

#[test]
fn a_signed_colour_field_fits_symmetrically_about_zero() {
    // Values are `x` over a strip spanning [0, 1], but the *field* declares a
    // signed range, and the field's declaration is what picks the fit: a signed
    // quantity keeps its diverging ramp centred on zero even where this
    // particular surface only samples one side of it.
    let color: Arc<dyn ScalarField> = Arc::new(AnalyticField::new(
        "signed_ramp",
        |p| p.x,
        5.0,
        Some((-1.0, 1.0)),
    ));
    let mesh = split_resolution_strip(400);
    let data = field_data(sphere_bump(), 0.6, color);

    let distribution = surface_color_distribution(&mesh, &data).expect("distribution");
    assert!(distribution.is_signed());

    let (min, max) = distribution.fit().expect("above the vertex floor");
    assert_eq!(min, -max, "a signed fit is exactly [-q, +q]");
    assert!(max > 0.0);
    // Zero maps to the midpoint of a symmetric domain, which is where a
    // diverging ramp puts its white.
    assert!(((min + max) / 2.0).abs() < 1e-12);
    assert!(
        (max - distribution.abs_p98()).abs() < 1e-12,
        "q is p98 of |v| on the surface"
    );
}

#[test]
fn a_non_negative_colour_field_fits_to_the_percentile_span() {
    let color: Arc<dyn ScalarField> = Arc::new(AnalyticField::new(
        "non_negative_ramp",
        |p| p.x,
        5.0,
        Some((0.0, 1.0)),
    ));
    let mesh = split_resolution_strip(400);
    let data = field_data(sphere_bump(), 0.6, color);

    let distribution = surface_color_distribution(&mesh, &data).expect("distribution");
    assert!(!distribution.is_signed());

    let (min, max) = distribution.fit().expect("above the vertex floor");
    assert_eq!((min, max), (distribution.p2(), distribution.p98()));
    assert!(max > min, "the fit never emits an inverted domain");
    // The band is inside the surface's own range and excludes the top tail —
    // which is the whole reason percentiles are used rather than the extremes.
    let (value_min, value_max) = distribution.value_range();
    assert!(min >= value_min && max < value_max);
}

// ============================================================================
// The vertex floor
// ============================================================================

#[test]
fn a_surface_below_the_vertex_floor_refuses_to_fit() {
    // Two columns of quads: six vertices, three orders of magnitude under the
    // floor. It still *plots* — the distribution exists and carries a range —
    // and only the fit refuses.
    let mesh = split_resolution_strip(2);
    let data = field_data(sphere_bump(), 0.6, linear_ramp());

    let distribution = surface_color_distribution(&mesh, &data).expect("distribution");
    assert!(distribution.vertex_count() < MIN_FIT_VERTICES);
    assert!(
        distribution.fit().is_none(),
        "a coarse surface must refuse rather than write a plausible wrong number"
    );
    let (min, max) = distribution.value_range();
    assert!(max > min, "the plot still has something to draw");
}

// ============================================================================
// Independence from the quality preference
// ============================================================================

#[test]
fn the_fit_agrees_across_extraction_quality_multipliers() {
    // A real extraction this time, so the mesh really is preference-driven: the
    // whole point of the vertex floor is that the answer stops moving once the
    // surface is adequately sampled.
    //
    // The field has to be a **sampled** one. `quality_multiplier` subdivides a
    // *native* grid and is ignored outright by the analytic fallback lattice, so
    // an analytic fixture here would extract the same mesh twice and the test
    // would pass without testing anything.
    let grid = GridGeometry {
        origin: DVec3::splat(-2.0),
        axes: [
            DVec3::new(0.125, 0.0, 0.0),
            DVec3::new(0.0, 0.125, 0.0),
            DVec3::new(0.0, 0.0, 0.125),
        ],
        // Fine enough that even the un-subdivided extraction clears the vertex
        // floor: the point of the row is the two answers agreeing, not the
        // floor refusing.
        dims: [33, 33, 33],
    };
    let analytic = sphere_bump();
    let sampled: Arc<dyn ScalarField> = Arc::new(sample_onto_grid(grid, |p| analytic.sample(p)));
    let data = field_data(sampled, 0.6, linear_ramp());

    let coarse = extract_isosurface(&data, &settings(1.0)).expect("within budget");
    let fine = extract_isosurface(&data, &settings(2.0)).expect("within budget");

    let coarse_fit = surface_color_distribution(&coarse, &data)
        .expect("distribution")
        .fit()
        .expect("above the floor");
    let fine_fit = surface_color_distribution(&fine, &data)
        .expect("distribution")
        .fit()
        .expect("above the floor");

    assert!(
        fine.vertex_count() > coarse.vertex_count() * 2,
        "the two meshes must actually differ, or this asserts nothing"
    );
    let span = coarse_fit.1 - coarse_fit.0;
    assert!(
        (fine_fit.0 - coarse_fit.0).abs() < span * 0.05
            && (fine_fit.1 - coarse_fit.1).abs() < span * 0.05,
        "fits should agree within a few percent: {coarse_fit:?} vs {fine_fit:?}"
    );
}

// ============================================================================
// The states with nothing to describe
// ============================================================================

#[test]
fn a_phase_coloured_surface_has_no_colour_distribution() {
    let mesh = split_resolution_strip(8);
    let data = IsosurfaceData {
        field: sphere_bump(),
        level: 0.6,
        level_basis: LevelBasis::Absolute,
        coloring: IsosurfaceColoring::Phase {
            positive: Vec3::X,
            negative: Vec3::Y,
        },
        alpha: 1.0,
    };
    assert!(surface_color_distribution(&mesh, &data).is_none());
}

#[test]
fn an_empty_mesh_has_no_colour_distribution() {
    let data = field_data(sphere_bump(), 0.6, linear_ramp());
    assert!(surface_color_distribution(&SurfaceMesh::default(), &data).is_none());
}

#[test]
fn a_constant_colour_field_has_a_distribution_but_no_fit() {
    let color: Arc<dyn ScalarField> = Arc::new(AnalyticField::new(
        "constant",
        |_| 0.25,
        5.0,
        Some((0.25, 0.25)),
    ));
    let mesh = split_resolution_strip(400);
    let data = field_data(sphere_bump(), 0.6, color);

    let distribution = surface_color_distribution(&mesh, &data).expect("distribution");
    assert_eq!(distribution.value_range(), (0.25, 0.25));
    assert!(
        distribution.fit().is_none(),
        "there is no range to fit, and an inverted or empty domain would flatten \
         the surface to the ramp's midpoint"
    );
}

// ============================================================================
// The constructed seam
// ============================================================================

#[test]
fn percentiles_are_weighted_steps_over_the_supplied_weights() {
    // Straight at the seam, with an answer that can be read off by hand: three
    // values, all the weight on the middle one.
    let distribution = SurfaceValueDistribution::from_weighted_values(
        vec![-1.0, 0.0, 1.0],
        vec![1e-6, 1.0, 1e-6],
        true,
    )
    .expect("some weight");
    assert_eq!(distribution.p50(), 0.0);
    assert_eq!(distribution.value_range(), (-1.0, 1.0));
    assert!((distribution.total_area() - (1.0 + 2e-6)).abs() < 1e-12);

    // Zero-weight vertices drop out entirely rather than voting.
    let weightless =
        SurfaceValueDistribution::from_weighted_values(vec![5.0, 6.0], vec![0.0, 0.0], false);
    assert!(weightless.is_none());
}

#[test]
fn a_vertexs_weight_is_a_third_of_each_incident_triangle() {
    // One unit right triangle: area 1/2, so each of its three vertices carries
    // 1/6 and the total is the area itself.
    let mesh = SurfaceMesh {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        normals: vec![Vec3::Z; 3],
        albedo: vec![Vec3::ONE; 3],
        indices: vec![0, 1, 2],
        components: vec![SurfaceComponent {
            first_index: 0,
            index_count: 3,
            centroid: Vec3::ZERO,
        }],
        alpha: 1.0,
    };
    let data = field_data(sphere_bump(), 0.6, linear_ramp());
    let distribution = surface_color_distribution(&mesh, &data).expect("distribution");
    assert!((distribution.total_area() - 0.5).abs() < 1e-9);
    assert_eq!(distribution.vertex_count(), 3);
    // The colour is `x`, so the three vertices read 0, 1, 0.
    assert_eq!(distribution.value_range(), (0.0, 1.0));
}
