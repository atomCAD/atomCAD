use crate::renderer::mesh::Mesh;
use crate::kernel::surface_point_cloud::SurfacePoint;
use crate::kernel::surface_point_cloud::SurfacePointCloud;
use super::tessellator;
use glam::f32::Vec3;
use glam::f32::Quat;

pub fn tessellate_surface_point_cloud(output_mesh: &mut Mesh, surface_point_cloud: &SurfacePointCloud) {
  // Iterate through all surface points and add them to the tessellator
  for point in &surface_point_cloud.points {
    tessellate_surface_point(output_mesh, point);
  }
}

pub fn tessellate_surface_point(output_mesh: &mut Mesh, point: &SurfacePoint) {
  let roughness: f32 = 0.5;
  let metallic: f32 = 0.0;
  let outside_albedo = Vec3::new(0.0, 0.0, 1.0);
  let inside_albedo = Vec3::new(1.0, 0.0, 0.0);
  let side_albedo = Vec3::new(0.5, 0.5, 0.5);

  // Create rotation quaternion from surface normal to align cuboid
  let rotator = Quat::from_rotation_arc(Vec3::Y, point.normal);

  // Create vertices for cuboid
  let half_size = Vec3::new(0.15, 0.04, 0.15); // x, y, z extents
  let vertices = [
    // Top face vertices
    point.position + rotator.mul_vec3(Vec3::new(-half_size.x, 0.0, -half_size.z)),
    point.position + rotator.mul_vec3(Vec3::new(half_size.x, 0.0, -half_size.z)),
    point.position + rotator.mul_vec3(Vec3::new(half_size.x, 0.0, half_size.z)),
    point.position + rotator.mul_vec3(Vec3::new(-half_size.x, 0.0, half_size.z)),
    // Bottom face vertices
    point.position + rotator.mul_vec3(Vec3::new(-half_size.x, - 2.0 * half_size.y, -half_size.z)),
    point.position + rotator.mul_vec3(Vec3::new(half_size.x, - 2.0 * half_size.y, -half_size.z)),
    point.position + rotator.mul_vec3(Vec3::new(half_size.x, - 2.0 * half_size.y, half_size.z)),
    point.position + rotator.mul_vec3(Vec3::new(-half_size.x, - 2.0 * half_size.y, half_size.z)),
  ];

  // Add the six faces of the cuboid
  // Top face
  tessellator::tessellate_quad(
    output_mesh,
    &vertices[3], &vertices[2], &vertices[1], &vertices[0],
    &rotator.mul_vec3(Vec3::Y),
    &outside_albedo, roughness, metallic
  );

  // Bottom face
  tessellator::tessellate_quad(
    output_mesh,
    &vertices[4], &vertices[5], &vertices[6], &vertices[7],
    &rotator.mul_vec3(-Vec3::Y),
    &inside_albedo, roughness, metallic
  );

  // Front face
  tessellator::tessellate_quad(
    output_mesh,
    &vertices[2], &vertices[3], &vertices[7], &vertices[6],
    &rotator.mul_vec3(Vec3::Z),
    &side_albedo, roughness, metallic
  );

  // Back face
  tessellator::tessellate_quad(
    output_mesh,
    &vertices[0], &vertices[1], &vertices[5], &vertices[4],
    &rotator.mul_vec3(-Vec3::Z),
    &side_albedo, roughness, metallic
  );

  // Right face
  tessellator::tessellate_quad(
    output_mesh,
    &vertices[1], &vertices[2], &vertices[6], &vertices[5],
    &rotator.mul_vec3(Vec3::X),
    &side_albedo, roughness, metallic
  );

  // Left face
  tessellator::tessellate_quad(
    output_mesh,
    &vertices[3], &vertices[0], &vertices[4], &vertices[7],
    &rotator.mul_vec3(-Vec3::X),
    &side_albedo, roughness, metallic
  );
}
