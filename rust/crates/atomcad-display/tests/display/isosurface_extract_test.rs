//! Table C of `doc/design_isosurface_node.md` P2: **analytic fields.**
//!
//! End-to-end plausibility, backed by tables A and B rather than carrying the
//! argument alone. See `isosurface_common` for why every fixture here is a
//! decaying bump rather than a signed distance.

use std::sync::Arc;

use atomcad_crystolecule::field::{
    Colormap, GridGeometry, IsosurfaceColoring, IsosurfaceData, LevelBasis, ScalarField,
};
use atomcad_display::isosurface::{ExtractionSettings, extract_isosurface, sample_colormap};
use glam::{DVec3, Vec3};

use crate::isosurface_common::{
    BUMP_LEVEL, NEGATIVE_COLOR, POSITIVE_COLOR, assert_finite, assert_surface_invariants,
    bump_radius, component_positions, concentric_shells, euler_characteristic, extract,
    linear_ramp, p2z, phase_data, sample_onto_grid, sphere_bump, torus_bump, triangles, twin_bumps,
};

fn settings_with_spacing(fallback_spacing: f64) -> ExtractionSettings {
    ExtractionSettings {
        fallback_spacing,
        ..ExtractionSettings::default()
    }
}

#[test]
fn analytic_sphere_vertices_land_on_the_radius() {
    let field = sphere_bump();
    let mesh = extract(field.clone(), BUMP_LEVEL);
    assert_surface_invariants(&mesh, field.as_ref(), BUMP_LEVEL, 0.02, "sphere bump");
    assert_finite(&mesh, "sphere bump");

    let expected = bump_radius(BUMP_LEVEL);
    for position in &mesh.positions {
        let radius = position.as_dvec3().length();
        assert!(
            (radius - expected).abs() < 0.02,
            "vertex at radius {radius}, expected {expected}"
        );
    }

    assert_eq!(mesh.components.len(), 1, "a sphere is one component");
    assert!(
        (800..8000).contains(&mesh.triangle_count()),
        "triangle count {} is outside the plausible band for a 0.12 A lattice",
        mesh.triangle_count()
    );
}

#[test]
fn analytic_sphere_normals_point_radially_outward() {
    let mesh = extract(sphere_bump(), BUMP_LEVEL);
    for (position, normal) in mesh.positions.iter().zip(&mesh.normals) {
        let radial = position.normalize();
        assert!(
            radial.dot(*normal) > 0.999,
            "normal {normal:?} is not the outward radial direction {radial:?}"
        );
    }
}

#[test]
fn analytic_sphere_is_topologically_a_sphere() {
    let mesh = extract(sphere_bump(), BUMP_LEVEL);
    assert_eq!(euler_characteristic(&mesh), 2, "V - E + F for a sphere");
}

#[test]
fn torus_keeps_its_handle() {
    // The sphere's `== 2` catches neither a handle welded shut nor a handle
    // invented; genus 1 does.
    let field = torus_bump();
    let mesh = extract_isosurface(
        &phase_data(field.clone(), BUMP_LEVEL),
        &settings_with_spacing(0.2),
    )
    .expect("within budget");
    assert_surface_invariants(&mesh, field.as_ref(), BUMP_LEVEL, 0.05, "torus");
    assert_eq!(mesh.components.len(), 1, "a torus is one component");
    assert_eq!(euler_characteristic(&mesh), 0, "V - E + F for a torus");
}

#[test]
fn concentric_shells_are_two_components_with_coincident_centroids() {
    let field = concentric_shells();
    let mesh = extract_isosurface(
        &phase_data(field.clone(), BUMP_LEVEL),
        &settings_with_spacing(0.2),
    )
    .expect("within budget");
    assert_surface_invariants(&mesh, field.as_ref(), BUMP_LEVEL, 0.05, "shells");
    assert_eq!(mesh.components.len(), 2, "an inner and an outer sphere");

    // Both centroids sit at the origin, which is the documented limit of the
    // centroid sort: their relative draw order is arbitrary. Asserting the
    // *count* rather than the order is deliberate — see the design's §The fix.
    for component in &mesh.components {
        assert!(
            component.centroid.length() < 0.05,
            "centroid {:?} is not at the common centre",
            component.centroid
        );
    }
    // Sorted, because component order is by first vertex index — a march-order
    // detail, not a spatial one.
    let mut radii: Vec<f64> = (0..2)
        .map(|c| {
            let points = component_positions(&mesh, c);
            points.iter().map(|p| p.length()).sum::<f64>() / points.len() as f64
        })
        .collect();
    radii.sort_by(f64::total_cmp);
    let expected = bump_radius(BUMP_LEVEL);
    assert!(
        (radii[0] - (2.0 - expected)).abs() < 0.05,
        "inner shell {radii:?}"
    );
    assert!(
        (radii[1] - (2.0 + expected)).abs() < 0.05,
        "outer shell {radii:?}"
    );
}

#[test]
fn sampled_field_normals_follow_central_differences_on_the_stored_samples() {
    // `SampledField` overrides `gradient` with central differences on stored
    // samples, and on a coarse grid those differ measurably from the analytic
    // gradient. The extractor must follow the data, not the function it came
    // from.
    let grid = GridGeometry {
        origin: DVec3::splat(-2.0),
        axes: [
            DVec3::new(0.4, 0.0, 0.0),
            DVec3::new(0.0, 0.4, 0.0),
            DVec3::new(0.0, 0.0, 0.4),
        ],
        dims: [11, 11, 11],
    };
    let analytic = sphere_bump();
    let sampled: Arc<dyn ScalarField> = Arc::new(sample_onto_grid(grid, |p| analytic.sample(p)));
    let mesh = extract(sampled.clone(), BUMP_LEVEL);
    assert!(!mesh.is_empty());

    let mut disagreement = 0.0f64;
    for (position, normal) in mesh.positions.iter().zip(&mesh.normals) {
        let point = position.as_dvec3();
        let from_samples = -sampled.gradient(point).normalize();
        assert!(
            from_samples.as_vec3().dot(*normal) > 0.9999,
            "normal {normal:?} does not follow the stored-sample gradient {from_samples:?}"
        );
        let from_function = -analytic.gradient(point).normalize();
        disagreement = disagreement.max(from_samples.angle_between(from_function));
    }
    assert!(
        disagreement > 1e-3,
        "the two gradients agreed to {disagreement} rad, so this grid is too fine \
         for the test to mean anything"
    );
}

#[test]
fn p2z_extracts_one_lobe_per_sign() {
    let field = p2z();
    let level = 0.3;
    let mesh = extract_isosurface(
        &phase_data(field.clone(), level),
        &settings_with_spacing(0.25),
    )
    .expect("within budget");
    assert_surface_invariants(&mesh, field.as_ref(), level, 0.03, "2p_z");
    assert_eq!(mesh.components.len(), 2, "one lobe either side of z = 0");

    let above = component_positions(&mesh, 0);
    let below = component_positions(&mesh, 1);
    assert!(
        above.iter().all(|p| p.z > 0.0),
        "the positive pass reached below the nodal plane"
    );
    assert!(
        below.iter().all(|p| p.z < 0.0),
        "the negative pass reached above the nodal plane"
    );
    assert!(mesh.components[0].centroid.z > 0.0);
    assert!(mesh.components[1].centroid.z < 0.0);

    // Each pass lands on its own signed level, not on the other's.
    for point in &above {
        assert!((field.sample(*point) - level).abs() < 0.03);
    }
    for point in &below {
        assert!((field.sample(*point) + level).abs() < 0.03);
    }
}

#[test]
fn p2z_lobes_take_their_phase_colors() {
    let mesh = extract_isosurface(&phase_data(p2z(), 0.3), &settings_with_spacing(0.25))
        .expect("within budget");
    for (position, albedo) in mesh.positions.iter().zip(&mesh.albedo) {
        let expected = if position.z > 0.0 {
            POSITIVE_COLOR
        } else {
            NEGATIVE_COLOR
        };
        assert_eq!(*albedo, expected, "vertex at {position:?}");
    }
}

/// P5. Signedness and paint are independent: the *number* of components comes
/// from `value_range()`, the *paint* from the coloring discriminant, and
/// neither consults the other. `p2z_lobes_take_their_phase_colors` pins the
/// same field under `Phase`; this is the other half of that 2x2, and it is the
/// arm a real ESP map takes — a signed field is exactly what a density painted
/// by a potential is *not*, so without this row the two-component path is only
/// ever tested unpainted.
#[test]
fn a_signed_field_with_a_color_field_keeps_both_lobes_and_paints_per_vertex() {
    let level = 0.3;
    let data = IsosurfaceData {
        field: p2z(),
        level,
        level_basis: LevelBasis::Absolute,
        coloring: IsosurfaceColoring::Field {
            field: linear_ramp(),
            range: (-2.0, 2.0),
            colormap: Colormap::BlueWhiteRed,
        },
        alpha: 1.0,
    };
    let mesh = extract_isosurface(&data, &settings_with_spacing(0.25)).expect("within budget");

    assert_eq!(
        mesh.components.len(),
        2,
        "painting by a second field must not change how many lobes are extracted"
    );
    assert_finite(&mesh, "signed + color field");
    assert_eq!(mesh.positions.len(), mesh.albedo.len());

    // Every vertex is the colormap's answer for the ramp at that position — the
    // paint follows x, not the sign of the surface field.
    for (position, albedo) in mesh.positions.iter().zip(&mesh.albedo) {
        let expected = sample_colormap(Colormap::BlueWhiteRed, f64::from(position.x), (-2.0, 2.0));
        assert!(
            (*albedo - expected).length() < 1e-5,
            "vertex at {position:?} painted {albedo:?}, expected {expected:?}"
        );
    }

    // Both lobes are painted across the ramp rather than each taking one flat
    // color: the failure this guards against is a `Field` arm that silently
    // falls back to per-sign paint for signed fields.
    for component in 0..2 {
        let reds: Vec<f32> = component_positions(&mesh, component)
            .iter()
            .map(|p| sample_colormap(Colormap::BlueWhiteRed, f64::from(p.x), (-2.0, 2.0)).x)
            .collect();
        let lo = reds.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = reds.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            hi - lo > 0.1,
            "lobe {component} is painted one flat color (red {lo}..{hi})"
        );
    }

    // Neither is the geometry disturbed by the paint.
    let unpainted = extract_isosurface(&phase_data(p2z(), level), &settings_with_spacing(0.25))
        .expect("within budget");
    assert_eq!(mesh.triangle_count(), unpainted.triangle_count());
}

#[test]
fn two_lobes_under_one_cell_apart_stay_two_components() {
    // The surface-nets failure mode: one vertex per cell cannot represent two
    // sheets, so it welds them and the component labelling — which the
    // transparency sort is keyed on — silently merges two lobes into one.
    let field = twin_bumps();
    let level = 0.0812;
    let mesh = extract(field.clone(), level);
    assert_surface_invariants(&mesh, field.as_ref(), level, 0.01, "twin bumps");
    assert_eq!(mesh.components.len(), 2, "the lobes were welded together");

    assert!(mesh.components[0].centroid.x * mesh.components[1].centroid.x < 0.0);
    let gap = (mesh.components[0].centroid.x - mesh.components[1].centroid.x).abs();
    assert!(gap > 0.5, "the two centroids are not on opposite lobes");
}

#[test]
fn a_non_negative_field_extracts_one_component() {
    // `value_range().0 >= 0` short-circuits the negative pass; a second
    // component would mean it ran anyway and found the same surface twice.
    let mesh = extract(sphere_bump(), BUMP_LEVEL);
    assert_eq!(mesh.components.len(), 1);
    assert!(mesh.albedo.iter().all(|c| *c == POSITIVE_COLOR));
}

#[test]
fn a_level_above_the_fields_range_is_empty() {
    let mesh = extract(sphere_bump(), 5.0);
    assert!(mesh.is_empty(), "nothing crosses a level above the maximum");
    assert!(mesh.components.is_empty());
    assert_finite(&mesh, "empty");
}

#[test]
fn the_colormap_is_monotonic_and_clamps() {
    let range = (-0.05, 0.05);
    let mut previous = sample_colormap(Colormap::BlueWhiteRed, -1.0, range);
    // Below the domain the value clamps to the blue end.
    assert_eq!(
        previous,
        sample_colormap(Colormap::BlueWhiteRed, -0.05, range)
    );
    for step in 0..=100 {
        let value = -0.05 + 0.1 * step as f64 / 100.0;
        let color = sample_colormap(Colormap::BlueWhiteRed, value, range);
        assert!(color.x >= previous.x - 1e-6, "red channel fell at {value}");
        assert!(color.z <= previous.z + 1e-6, "blue channel rose at {value}");
        previous = color;
    }
    // Above the domain it clamps to the red end.
    assert_eq!(
        previous,
        sample_colormap(Colormap::BlueWhiteRed, 1.0, range)
    );

    // A domain the wrong way round, or a degenerate one, maps to the midpoint
    // rather than dividing by zero.
    let flat = sample_colormap(Colormap::BlueWhiteRed, 0.3, (1.0, 1.0));
    assert!(flat.is_finite());
}

#[test]
fn a_color_field_paints_per_vertex() {
    let data = IsosurfaceData {
        field: sphere_bump(),
        level: BUMP_LEVEL,
        level_basis: LevelBasis::Absolute,
        coloring: IsosurfaceColoring::Field {
            field: linear_ramp(),
            range: (-1.0, 1.0),
            colormap: Colormap::BlueWhiteRed,
        },
        alpha: 0.4,
    };
    let mesh = extract_isosurface(&data, &settings_with_spacing(0.12)).expect("within budget");
    assert!(!mesh.is_empty());
    assert_eq!(mesh.alpha, 0.4);

    // The ramp runs along x, so the red channel must rise with x and the blue
    // channel fall — per vertex, which is the whole point of the mode.
    let mut samples: Vec<(f32, Vec3)> = mesh
        .positions
        .iter()
        .zip(&mesh.albedo)
        .map(|(p, c)| (p.x, *c))
        .collect();
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let first = samples.first().unwrap().1;
    let last = samples.last().unwrap().1;
    assert!(last.x > first.x, "red did not rise with x");
    assert!(last.z < first.z, "blue did not fall with x");

    // And the surface itself is unchanged by how it is painted.
    assert_eq!(
        triangles(&mesh).count(),
        extract(sphere_bump(), BUMP_LEVEL).triangle_count()
    );
}
