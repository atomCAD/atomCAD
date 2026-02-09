use crate::display::surface_point_cloud::SurfacePoint;
use crate::display::surface_point_cloud::SurfacePoint2D;
use crate::display::surface_point_cloud::SurfacePointCloud;
use crate::display::surface_point_cloud::SurfacePointCloud2D;
use crate::renderer::mesh::Material;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator;
use glam::f32::Vec3;
use glam::f64::DQuat;
use glam::f64::DVec3;

pub fn tessellate_surface_point_cloud(
    output_mesh: &mut Mesh,
    surface_point_cloud: &SurfacePointCloud,
) {
    // Iterate through all surface points and add them to the tessellator
    for point in &surface_point_cloud.points {
        tessellate_surface_point(output_mesh, point);
    }
}

pub fn tessellate_surface_point(output_mesh: &mut Mesh, point: &SurfacePoint) {
    let roughness: f32 = 0.5;
    let metallic: f32 = 0.0;
    let outside_material = Material::new(&Vec3::new(0.0, 0.6, 0.0), roughness, metallic);
    let inside_material = Material::new(&Vec3::new(1.0, 0.0, 0.0), roughness, metallic);
    let side_material = Material::new(&Vec3::new(0.0, 0.0, 1.0), roughness, metallic);

    // Create rotation quaternion from surface normal to align cuboid
    let rotator = DQuat::from_rotation_arc(DVec3::Z, point.normal);

    let size = DVec3::new(0.3, 0.3, 0.08); // x, y, z extents

    tessellator::tessellate_cuboid(
        output_mesh,
        &(point.position - point.normal * size.z * 0.5),
        &size,
        &rotator,
        &outside_material,
        &inside_material,
        &side_material,
    );
}

pub fn tessellate_surface_point_cloud_2d(
    output_mesh: &mut Mesh,
    surface_point_cloud: &SurfacePointCloud2D,
) {
    // Iterate through all surface points and add them to the tessellator
    for point in &surface_point_cloud.points {
        tessellate_surface_point_2d(output_mesh, point);
    }
}

pub fn tessellate_surface_point_2d(output_mesh: &mut Mesh, point: &SurfacePoint2D) {
    let roughness: f32 = 0.5;
    let metallic: f32 = 0.0;
    let outside_material = Material::new(&Vec3::new(0.0, 0.6, 0.0), roughness, metallic);
    let inside_material = Material::new(&Vec3::new(1.0, 0.0, 0.0), roughness, metallic);
    let side_material = Material::new(&Vec3::new(0.0, 0.0, 1.0), roughness, metallic);

    // Create rotation quaternion from surface normal to align cuboid
    let position_3d = DVec3::new(point.position.x, point.position.y, 0.0);
    let normal_3d = DVec3::new(point.normal.x, point.normal.y, 0.0);
    let rotator = DQuat::from_rotation_arc(DVec3::Z, normal_3d);

    let size = DVec3::new(0.3, 0.2, 0.3); // x, y, z extents

    tessellator::tessellate_cuboid(
        output_mesh,
        &(position_3d - normal_3d * size.z * 0.5),
        &size,
        &rotator,
        &outside_material,
        &inside_material,
        &side_material,
    );
}
