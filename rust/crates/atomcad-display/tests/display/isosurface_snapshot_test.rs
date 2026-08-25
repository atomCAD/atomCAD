//! Table E of `doc/design_isosurface_node.md` P2: **regression snapshots.**
//!
//! A *summary* is snapshotted — counts, bounding box, per-component centroid, a
//! positions checksum — never the full vertex list, which is unreviewable in
//! `cargo insta review` and rewrites wholesale on any harmless reorder.

use std::sync::Arc;

use atomcad_crystolecule::field::ScalarField;
use atomcad_crystolecule::io::cube_loader::load_cube;
use atomcad_display::isosurface::{ExtractionSettings, SurfaceMesh, extract_isosurface};
use atomcad_test_support::fixture_path_str;

use crate::isosurface_common::{
    assert_closed_manifold, assert_components_partition_indices, phase_data, settings_for_snapshots,
};

fn cube_field(name: &str) -> Arc<dyn ScalarField> {
    let cube = load_cube(&fixture_path_str(&format!("cube/{name}")), false)
        .unwrap_or_else(|error| panic!("loading {name}: {error}"));
    Arc::new(cube.fields.into_iter().next().expect("one field"))
}

/// Counts, extent, per-component centroids and a coordinate checksum.
///
/// The checksum is a rounded coordinate sum: sensitive to a vertex moving,
/// blind to the last bits of `f32`, and readable in a review diff.
fn summarize(mesh: &SurfaceMesh) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "vertices: {}\ntriangles: {}\ncomponents: {}\n",
        mesh.vertex_count(),
        mesh.triangle_count(),
        mesh.components.len()
    ));
    if mesh.positions.is_empty() {
        return out;
    }
    let mut min = mesh.positions[0];
    let mut max = mesh.positions[0];
    let mut checksum = 0.0f64;
    for position in &mesh.positions {
        min = min.min(*position);
        max = max.max(*position);
        checksum += (position.x + position.y + position.z) as f64;
    }
    out.push_str(&format!(
        "bounds: [{:.4}, {:.4}, {:.4}] .. [{:.4}, {:.4}, {:.4}]\n",
        min.x, min.y, min.z, max.x, max.y, max.z
    ));
    out.push_str(&format!("checksum: {checksum:.4}\n"));
    for (i, component) in mesh.components.iter().enumerate() {
        out.push_str(&format!(
            "component {i}: {} triangles, centroid [{:.4}, {:.4}, {:.4}], albedo [{:.3}, {:.3}, {:.3}]\n",
            component.index_count / 3,
            component.centroid.x,
            component.centroid.y,
            component.centroid.z,
            mesh.albedo[mesh.indices[component.first_index as usize] as usize].x,
            mesh.albedo[mesh.indices[component.first_index as usize] as usize].y,
            mesh.albedo[mesh.indices[component.first_index as usize] as usize].z,
        ));
    }
    out
}

fn snapshot(field: Arc<dyn ScalarField>, level: f64) -> String {
    let mesh = extract_isosurface(&phase_data(field, level), &settings_for_snapshots())
        .expect("within budget");
    // A snapshot of a *broken* surface is worse than no snapshot: it blesses
    // the breakage. Every level below is chosen above its fixture's boundary
    // maximum so the level set stays inside the sampled box; these two guards
    // are what would catch that choice going stale.
    assert_closed_manifold(&mesh, "snapshot fixture");
    assert_components_partition_indices(&mesh, "snapshot fixture");
    summarize(&mesh)
}

/// `water_bohr.cube` reaches `0.3905` on its boundary planes and `1.4753`
/// inside, so both levels here sit above the first and below the second.
#[test]
fn water_cube_envelope_at_two_levels() {
    let field = cube_field("water_bohr.cube");
    insta::assert_snapshot!("water_bohr_level_0_50", snapshot(field.clone(), 0.50));
    insta::assert_snapshot!("water_bohr_level_1_00", snapshot(field, 1.00));
}

/// `p2z_11x11x11.cube` reaches `0.7358` on its boundary and `0.8437` at the
/// lobe maxima — a narrow window, and the reason a "nice round" level like
/// `0.05` would snapshot two clipped caps rather than two lobes.
#[test]
fn p2z_cube_both_sign_passes() {
    let field = cube_field("p2z_11x11x11.cube");
    insta::assert_snapshot!("p2z_level_0_78", snapshot(field, 0.78));
}

#[test]
fn a_finer_quality_multiplier_changes_only_the_mesh() {
    // The architectural claim of the design, in test form: the same value at a
    // different quality setting yields a different mesh and nothing else.
    let field = cube_field("p2z_11x11x11.cube");
    let data = phase_data(field, 0.78);
    let coarse = extract_isosurface(&data, &settings_for_snapshots()).expect("within budget");
    let fine = extract_isosurface(
        &data,
        &ExtractionSettings {
            quality_multiplier: 3.0,
            ..settings_for_snapshots()
        },
    )
    .expect("within budget");

    assert_eq!(coarse.components.len(), fine.components.len());
    assert!(fine.triangle_count() > coarse.triangle_count());
    insta::assert_snapshot!("p2z_level_0_78_subdiv_3", summarize(&fine));
}
