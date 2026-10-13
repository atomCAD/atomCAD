use atomcad_crystolecule::atomic_structure::atomic_structure_decorator::{
    WireframeRingVisuals, WireframeSphereVisuals,
};
use atomcad_renderer::line_mesh::{LineMesh, LineStyle};
use glam::f32::Vec3;
use glam::f64::DVec3;

/// Gray color for wireframe sphere/ring.
const WIREFRAME_COLOR: [f32; 3] = [0.376, 0.376, 0.376]; // #606060

/// Line weight of the sphere and ring, logical pixels
/// (doc/design_screen_space_lines.md D8).
pub const WIREFRAME_WIDTH_PX: f32 = 1.5;

/// Number of segments per great circle.
const SPHERE_SEGMENTS: u32 = 48;

/// Tessellate a wireframe circle into a LineMesh.
///
/// Generates a closed polyline of `segments` line segments forming a circle at
/// `center` with given `radius` in the plane defined by `normal`, so a dash
/// pattern would run continuously around it.
pub fn tessellate_wireframe_circle(
    line_mesh: &mut LineMesh,
    center: &DVec3,
    radius: f64,
    normal: &DVec3,
    segments: u32,
    color: &[f32; 3],
) {
    // Build orthonormal basis from normal
    let n = normal.normalize();
    let arb = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = n.cross(arb).normalize();
    let v = n.cross(u);

    let step = std::f64::consts::TAU / segments as f64;
    let points: Vec<Vec3> = (0..segments)
        .map(|i| {
            let (sin_a, cos_a) = (step * i as f64).sin_cos();
            (*center + (u * cos_a + v * sin_a) * radius).as_vec3()
        })
        .collect();

    line_mesh.add_polyline(
        &points,
        true,
        &LineStyle::solid(*color).with_width(WIREFRAME_WIDTH_PX),
    );
}

/// Tessellate a wireframe sphere (3 great circles: XY, XZ, YZ planes).
pub fn tessellate_wireframe_sphere(
    line_mesh: &mut LineMesh,
    center: &DVec3,
    radius: f64,
    color: &[f32; 3],
) {
    // Three orthogonal great circles
    tessellate_wireframe_circle(line_mesh, center, radius, &DVec3::Z, SPHERE_SEGMENTS, color);
    tessellate_wireframe_circle(line_mesh, center, radius, &DVec3::Y, SPHERE_SEGMENTS, color);
    tessellate_wireframe_circle(line_mesh, center, radius, &DVec3::X, SPHERE_SEGMENTS, color);
}

/// Tessellate wireframe sphere visuals from the decorator into a LineMesh.
pub fn tessellate_guided_wireframe_sphere(
    line_mesh: &mut LineMesh,
    sphere_visuals: &WireframeSphereVisuals,
) {
    tessellate_wireframe_sphere(
        line_mesh,
        &sphere_visuals.center,
        sphere_visuals.radius,
        &WIREFRAME_COLOR,
    );
}

/// Tessellate wireframe ring visuals from the decorator into a LineMesh.
pub fn tessellate_guided_wireframe_ring(
    line_mesh: &mut LineMesh,
    ring_visuals: &WireframeRingVisuals,
) {
    tessellate_wireframe_circle(
        line_mesh,
        &ring_visuals.center,
        ring_visuals.radius,
        &ring_visuals.normal,
        SPHERE_SEGMENTS,
        &WIREFRAME_COLOR,
    );
}
