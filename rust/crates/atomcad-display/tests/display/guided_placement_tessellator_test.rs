// The guided-placement sphere and ring: closed polylines at 1.5 px
// (doc/design_screen_space_lines.md D8, Phase 3).

use atomcad_crystolecule::atomic_structure::atomic_structure_decorator::{
    WireframeRingVisuals, WireframeSphereVisuals,
};
use atomcad_display::guided_placement_tessellator::{
    WIREFRAME_WIDTH_PX, tessellate_guided_wireframe_ring, tessellate_guided_wireframe_sphere,
};
use atomcad_renderer::line_mesh::{LineMesh, LineSegmentInstance};
use glam::f32::Vec3;
use glam::f64::DVec3;

const SEGMENTS_PER_CIRCLE: usize = 48;

fn length(segment: &LineSegmentInstance) -> f32 {
    Vec3::from_array(segment.p0).distance(Vec3::from_array(segment.p1))
}

/// Checks that `segments` is one closed loop: each starts where the previous
/// ended, the last ends where the first started, and each dash phase is the
/// arc length before it.
fn assert_closed_polyline(segments: &[LineSegmentInstance]) {
    let mut arc_length = 0.0f32;
    for (i, segment) in segments.iter().enumerate() {
        let next = &segments[(i + 1) % segments.len()];
        assert_eq!(segment.p1, next.p0, "segment {i} does not join the next");
        assert!(
            (segment.dash_phase - arc_length).abs() < 1e-4,
            "segment {i}: phase {} vs arc length {arc_length}",
            segment.dash_phase
        );
        arc_length += length(segment);
    }
}

#[test]
fn the_ring_is_one_closed_polyline() {
    let radius = 2.0;
    let mut mesh = LineMesh::new();
    tessellate_guided_wireframe_ring(
        &mut mesh,
        &WireframeRingVisuals {
            center: DVec3::new(1.0, 2.0, 3.0),
            normal: DVec3::new(0.0, 1.0, 1.0),
            radius,
        },
    );
    assert_eq!(mesh.len(), SEGMENTS_PER_CIRCLE);
    assert_closed_polyline(&mesh.instances);

    // The last phase plus the last length is the perimeter of the 48-gon.
    let last = &mesh.instances[SEGMENTS_PER_CIRCLE - 1];
    let perimeter = last.dash_phase + length(last);
    let expected = 2.0
        * SEGMENTS_PER_CIRCLE as f32
        * radius as f32
        * (std::f32::consts::PI / SEGMENTS_PER_CIRCLE as f32).sin();
    assert!(
        (perimeter - expected).abs() < 1e-3,
        "{perimeter} vs {expected}"
    );

    assert_eq!(WIREFRAME_WIDTH_PX, 1.5);
    for segment in &mesh.instances {
        assert_eq!(segment.width_px, WIREFRAME_WIDTH_PX);
        assert_eq!(segment.dash_off_px, 0.0);
    }
}

#[test]
fn the_sphere_is_three_closed_polylines() {
    let mut mesh = LineMesh::new();
    tessellate_guided_wireframe_sphere(
        &mut mesh,
        &WireframeSphereVisuals {
            center: DVec3::ZERO,
            radius: 1.5,
            preview_position: None,
        },
    );
    assert_eq!(mesh.len(), 3 * SEGMENTS_PER_CIRCLE);
    for circle in mesh.instances.chunks(SEGMENTS_PER_CIRCLE) {
        assert_closed_polyline(circle);
    }
    for segment in &mesh.instances {
        assert_eq!(segment.width_px, WIREFRAME_WIDTH_PX);
        assert!((Vec3::from_array(segment.p0).length() - 1.5).abs() < 1e-5);
    }
}
