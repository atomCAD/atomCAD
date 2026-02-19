use crate::crystolecule::atomic_structure::atomic_structure_decorator::WireframeSphereVisuals;
use crate::renderer::line_mesh::LineMesh;
use glam::f32::Vec3;
use glam::f64::DVec3;

/// Gray color for wireframe sphere/ring.
const WIREFRAME_COLOR: [f32; 3] = [0.376, 0.376, 0.376]; // #606060

/// Number of segments per great circle.
const SPHERE_SEGMENTS: u32 = 48;

/// Tessellate a wireframe circle into a LineMesh.
///
/// Generates `segments` line segments forming a circle at `center` with given `radius`
/// in the plane defined by `normal`.
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
    let arb = if n.x.abs() < 0.9 {
        DVec3::X
    } else {
        DVec3::Y
    };
    let u = n.cross(arb).normalize();
    let v = n.cross(u);

    let step = std::f64::consts::TAU / segments as f64;
    let mut prev = *center + u * radius;

    for i in 1..=segments {
        let angle = step * i as f64;
        let (sin_a, cos_a) = angle.sin_cos();
        let curr = *center + (u * cos_a + v * sin_a) * radius;

        line_mesh.add_line_with_uniform_color(
            &Vec3::new(prev.x as f32, prev.y as f32, prev.z as f32),
            &Vec3::new(curr.x as f32, curr.y as f32, curr.z as f32),
            color,
        );

        prev = curr;
    }
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
pub fn tessellate_guided_wireframe(
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
