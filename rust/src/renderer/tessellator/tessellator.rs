use super::super::mesh::Mesh;
use super::super::mesh::Vertex;
use super::super::mesh::Material;
use glam::f32::Vec3;
use glam::f64::DQuat;
use glam::f64::DVec3;

pub trait Tessellatable {
  fn tessellate(&self, output_mesh: &mut Mesh);

  // Explicit conversion to Box<dyn Tessellatable
  fn as_tessellatable(&self) -> Box<dyn Tessellatable>;
}

// provide the positions in counter clockwise order
pub fn tessellate_quad(
    output_mesh: &mut Mesh,
    pos0: &DVec3,
    pos1: &DVec3,
    pos2: &DVec3,
    pos3: &DVec3,
    normal: &DVec3,
    material: &Material,
) {
    let index0 = output_mesh.add_vertex(Vertex::new(&pos0.as_vec3(), &normal.as_vec3(), material));
    let index1 = output_mesh.add_vertex(Vertex::new(&pos1.as_vec3(), &normal.as_vec3(), material));
    let index2 = output_mesh.add_vertex(Vertex::new(&pos2.as_vec3(), &normal.as_vec3(), material));
    let index3 = output_mesh.add_vertex(Vertex::new(&pos3.as_vec3(), &normal.as_vec3(), material));
    output_mesh.add_quad(index0, index1, index2, index3);
}

pub fn tessellate_cuboid(
  output_mesh: &mut Mesh,
  center: &DVec3,
  size: &DVec3,
  rotator: &DQuat,
  top_material: &Material,
  bottom_material: &Material,
  side_material: &Material,
) {
  // Create vertices for cuboid
  let half_size = size * 0.5;
  let vertices = [
    // Top face vertices
    center + rotator.mul_vec3(DVec3::new(-half_size.x, half_size.y, -half_size.z)),
    center + rotator.mul_vec3(DVec3::new(half_size.x, half_size.y, -half_size.z)),
    center + rotator.mul_vec3(DVec3::new(half_size.x, half_size.y, half_size.z)),
    center + rotator.mul_vec3(DVec3::new(-half_size.x, half_size.y, half_size.z)),
    // Bottom face vertices
    center + rotator.mul_vec3(DVec3::new(-half_size.x, - half_size.y, -half_size.z)),
    center + rotator.mul_vec3(DVec3::new(half_size.x, - half_size.y, -half_size.z)),
    center + rotator.mul_vec3(DVec3::new(half_size.x, - half_size.y, half_size.z)),
    center + rotator.mul_vec3(DVec3::new(-half_size.x, - half_size.y, half_size.z)),
  ];

  // Add the six faces of the cuboid
  // Top face
  tessellate_quad(
    output_mesh,
    &vertices[3], &vertices[2], &vertices[1], &vertices[0],
    &rotator.mul_vec3(DVec3::Y),
    &top_material
  );

  // Bottom face
  tessellate_quad(
    output_mesh,
    &vertices[4], &vertices[5], &vertices[6], &vertices[7],
    &rotator.mul_vec3(-DVec3::Y),
    &bottom_material
  );

  // Front face
  tessellate_quad(
    output_mesh,
    &vertices[2], &vertices[3], &vertices[7], &vertices[6],
    &rotator.mul_vec3(DVec3::Z),
    &side_material
  );

  // Back face
  tessellate_quad(
    output_mesh,
    &vertices[0], &vertices[1], &vertices[5], &vertices[4],
    &rotator.mul_vec3(-DVec3::Z),
    &side_material
  );

  // Right face
  tessellate_quad(
    output_mesh,
    &vertices[1], &vertices[2], &vertices[6], &vertices[5],
    &rotator.mul_vec3(DVec3::X),
    &side_material
  );

  // Left face
  tessellate_quad(
    output_mesh,
    &vertices[3], &vertices[0], &vertices[4], &vertices[7],
    &rotator.mul_vec3(-DVec3::X),
    &side_material
  );
}

pub fn tessellate_circle_sheet (
    output_mesh: &mut Mesh,
    center: &DVec3,
    normal: &DVec3,
    radius: f64,
    divisions: u32,
    material: &Material,
) 
{
  let rotation = DQuat::from_rotation_arc(DVec3::new(0.0, 1.0, 0.0), *normal);

  let center_index = output_mesh.add_vertex(Vertex::new(
    &center.as_vec3(),
    &normal.as_vec3(),
    material,
  ));

  let index_start = output_mesh.vertices.len() as u32;

  for x in 0..divisions {
    let u = (x as f64) / (divisions as f64); // u runs from 0 to 1
    let theta = u * 2.0 * std::f64::consts::PI; // From 0 to 2*PI
    let out_normal = DVec3::new(theta.sin(), 0.0, theta.cos());

    let position = center + rotation.mul_vec3(out_normal * radius);
    
    output_mesh.add_vertex(Vertex::new(
      &position.as_vec3(),
      &normal.as_vec3(),
      material,
    ));

    let offset = index_start + x;
    let next_offset = index_start + (x + 1) % divisions;
    
    output_mesh.add_triangle(center_index, offset, next_offset);
  }
}

pub fn tessellate_sphere(
    output_mesh: &mut Mesh,
    center: &DVec3,
    radius: f64,
    horizontal_divisions: u32, // number sections when dividing by horizontal lines
    vertical_divisions: u32, // number of sections when dividing by vertical lines
    material: &Material,
) {

  // ---------- Add vertices ----------

  let north_pole_index = output_mesh.add_vertex(Vertex::new(
    &Vec3::new(center.x as f32, (center.y + radius) as f32, center.z as f32),
    &Vec3::new(0.0, 1.0, 0.0),
    material,
  ));

  let south_pole_index = output_mesh.add_vertex(Vertex::new(
    &Vec3::new(center.x as f32, (center.y - radius) as f32, center.z as f32),
    &Vec3::new(0.0, -1.0, 0.0),
    material,
  ));

  let non_pole_index_start = output_mesh.vertices.len() as u32;

  for y in 1..vertical_divisions {
    let v = (y as f64) / (vertical_divisions as f64); // v runs from 0 to 1
    let phi = v * std::f64::consts::PI; // From 0 to PI (latitude)    
    for x in 0..horizontal_divisions {
      let u = (x as f64) / (horizontal_divisions as f64); // u runs from 0 to 1
      let theta = u * 2.0 * std::f64::consts::PI; // From 0 to 2*PI (longitude)

      let normal = DVec3::new(theta.sin() * phi.sin(), phi.cos(), theta.cos() * phi.sin());
      let position = normal * radius + center;

      output_mesh.add_vertex(Vertex::new(
        &position.as_vec3(),
        &normal.as_vec3(),
        material,
      ));
    } // end of for x
  } // end of for y

  // ---------- add indices ----------

  // Add north pole triangles
  for x in 0..horizontal_divisions {
    output_mesh.add_triangle(
      north_pole_index,
      non_pole_index_start + x % horizontal_divisions,
      non_pole_index_start + (x + 1) % horizontal_divisions,
    );
  }

  // Add south pole triangles
  let last_longitude_index_start = non_pole_index_start + (vertical_divisions - 2) * horizontal_divisions;
  for x in 0..horizontal_divisions {
    output_mesh.add_triangle(
      south_pole_index,
      last_longitude_index_start + (x + 1) % horizontal_divisions,
      last_longitude_index_start + x % horizontal_divisions,
    );
  }

  // Add quads
  for y in 1..(vertical_divisions - 1) {
    let offset = non_pole_index_start + (y - 1) * horizontal_divisions;
    for x in 0..horizontal_divisions {
      output_mesh.add_quad(
        offset + (x + 1) % horizontal_divisions,
        offset + x % horizontal_divisions,
        offset + horizontal_divisions + x % horizontal_divisions,
        offset + horizontal_divisions + (x + 1) % horizontal_divisions,
      );
    }
  }
}

pub fn tessellate_cylinder(
    output_mesh: &mut Mesh,
    top_center: &DVec3,
    bottom_center: &DVec3,
    radius: f64,
    divisions: u32,
    material: &Material,
    include_top_and_bottom: bool) {
  let center = (top_center + bottom_center) * 0.5;
  let dir = (top_center - bottom_center).normalize();
  let up = dir;
  let down = dir * -1.0;
  let length = (top_center - bottom_center).length();
  let rotation = DQuat::from_rotation_arc(DVec3::new(0.0, 1.0, 0.0), dir);
  
  let index_start = output_mesh.vertices.len() as u32;
  for x in 0..divisions {
    let u = (x as f64) / (divisions as f64); // u runs from 0 to 1
    let theta = u * 2.0 * std::f64::consts::PI; // From 0 to 2*PI

    let normal = DVec3::new(theta.sin(), 0.0, theta.cos());
    let bottom_position = center + rotation.mul_vec3(DVec3::new(0.0, -length * 0.5, 0.0) + normal * radius);
    let top_position = center + rotation.mul_vec3(DVec3::new(0.0, length * 0.5, 0.0) + normal * radius);

    output_mesh.add_vertex(Vertex::new(
      &bottom_position.as_vec3(),
      &normal.as_vec3(),
      material,
    ));

    output_mesh.add_vertex(Vertex::new(
      &top_position.as_vec3(),
      &normal.as_vec3(),
      material,
    ));

    let offset = index_start + 2 * x;
    let next_offset = index_start + 2 * ((x + 1) % divisions);

    output_mesh.add_quad(
      offset, // bottom
      next_offset, // next bottom
      next_offset + 1, // next top
      offset + 1 // top
    );
  }

  if include_top_and_bottom {
    tessellate_circle_sheet (
      output_mesh,
      &top_center,
      &up,
      radius,
      divisions,
      material,
    );

    tessellate_circle_sheet (
      output_mesh,
      &bottom_center,
      &down,
      radius,
      divisions,
      material,
    );
  }
}

pub fn tessellate_crosshair_3d(
    output_mesh: &mut Mesh,
    center: &DVec3,
    half_length: f64,
    radius: f64,
    divisions: u32,
    material: &Material,
    include_caps: bool) {
  // Create points for the X-axis cylinder
  let x_top = center + DVec3::new(half_length, 0.0, 0.0);
  let x_bottom = center + DVec3::new(-half_length, 0.0, 0.0);
  
  // Create points for the Y-axis cylinder
  let y_top = center + DVec3::new(0.0, half_length, 0.0);
  let y_bottom = center + DVec3::new(0.0, -half_length, 0.0);
  
  // Create points for the Z-axis cylinder
  let z_top = center + DVec3::new(0.0, 0.0, half_length);
  let z_bottom = center + DVec3::new(0.0, 0.0, -half_length);
  
  // Tessellate the X-axis cylinder
  tessellate_cylinder(
    output_mesh,
    &x_top,
    &x_bottom,
    radius,
    divisions,
    material,
    include_caps
  );
  
  // Tessellate the Y-axis cylinder
  tessellate_cylinder(
    output_mesh,
    &y_top,
    &y_bottom,
    radius,
    divisions,
    material,
    include_caps
  );
  
  // Tessellate the Z-axis cylinder
  tessellate_cylinder(
    output_mesh,
    &z_top,
    &z_bottom,
    radius,
    divisions,
    material,
    include_caps
  );
}

pub fn tessellate_grid(
    output_mesh: &mut Mesh,
    center: &DVec3,
    rotator: &DQuat,
    thickness: f64,
    width: f64,
    height: f64,
    line_width: f64,
    grid_unit: f64,
    top_material: &Material,
    bottom_material: &Material,
    side_material: &Material,
) {

  let horiz_divisions = (width / grid_unit).ceil() as u32;
  let vert_divisions = (height / grid_unit).ceil() as u32;

  let start_x =  - width * 0.5;
  let start_z =  - height * 0.5;
  for x in 0..horiz_divisions {

    let cuboid_center = center + rotator.mul_vec3(DVec3::new(start_x + (x as f64) * grid_unit, -thickness * 0.5, 0.0));

    tessellate_cuboid(
      output_mesh,
      &cuboid_center,
      &(DVec3::new(line_width, thickness, height)),
      rotator,
      top_material,
      bottom_material,
      side_material,
    );    
  }
  for z in 0..vert_divisions {

    let cuboid_center = center + rotator.mul_vec3(DVec3::new(0.0, -thickness * 0.5, start_z + (z as f64) * grid_unit));

    tessellate_cuboid(
      output_mesh,
      &cuboid_center,
      &(DVec3::new(width, thickness, line_width)),
      rotator,
      top_material,
      bottom_material,
      side_material,
    );
  }
}

pub fn tessellate_cone(
    output_mesh: &mut Mesh,
    apex: &DVec3,
    base_center: &DVec3,
    radius: f64,
    divisions: u32,
    material: &Material,
    include_base: bool) {
  
  let dir = (apex - base_center).normalize();
  let down = -dir;
  let rotation = DQuat::from_rotation_arc(DVec3::new(0.0, 1.0, 0.0), dir);
  
  // Base vertices indices will start here
  let base_index_start = output_mesh.vertices.len() as u32;
  
  // First pass: create base vertices in a circular pattern
  let mut base_positions = Vec::with_capacity(divisions as usize);
  let mut base_normals = Vec::with_capacity(divisions as usize);
  
  for x in 0..divisions {
    let u = (x as f64) / (divisions as f64); // u runs from 0 to 1
    let theta = u * 2.0 * std::f64::consts::PI; // From 0 to 2*PI

    // Calculate the position on the base circle
    let circle_point = DVec3::new(theta.sin(), 0.0, theta.cos());
    let base_position = base_center + rotation.mul_vec3(circle_point * radius);
    base_positions.push(base_position);
    
    // Calculate normal for this segment
    // The normal is perpendicular to both:
    // 1. The vector from apex to base point
    // 2. The tangent vector to the circle at that point
    let apex_to_base = base_position - *apex;
    let tangent = DVec3::new(theta.cos(), 0.0, -theta.sin());
    let rotated_tangent = rotation.mul_vec3(tangent);
    let normal = apex_to_base.cross(rotated_tangent).normalize();
    base_normals.push(normal);
    
    // Add the base vertex with its normal
    output_mesh.add_vertex(Vertex::new(
      &base_position.as_vec3(),
      &normal.as_vec3(),
      material,
    ));
  }
  
  // Second pass: create triangles with separate apex vertices for each segment
  for x in 0..divisions {
    let current_base_index = base_index_start + x;
    let next_base_index = base_index_start + ((x + 1) % divisions);
    
    // Calculate the normal for this apex instance - use the same normal as the face
    // This is the average of the two base normals for smooth shading
    let apex_normal = (base_normals[x as usize] + base_normals[((x + 1) % divisions) as usize]) * 0.5;
    
    // Add a new apex vertex with this specific normal
    let apex_index = output_mesh.add_vertex(Vertex::new(
      &apex.as_vec3(),
      &apex_normal.as_vec3(),
      material,
    ));
    
    // Create triangular face connecting the base vertices to this specific apex vertex
    output_mesh.add_triangle(
      apex_index,
      current_base_index,
      next_base_index
    );
  }

  // Optionally create the base circle
  if include_base {
    tessellate_circle_sheet(
      output_mesh,
      base_center,
      &down,
      radius,
      divisions,
      material,
    );
  }
}

pub fn tessellate_arrow(
  output_mesh: &mut Mesh,
  start_center: &DVec3,
  axis_dir: &DVec3,
  cylinder_radius: f64,
  cone_radius: f64,
  divisions: u32,
  cylinder_length: f64,
  cone_length: f64,
  cone_offset: f64,
  material: &Material) {
    tessellate_cylinder(
      output_mesh,
      &(start_center + axis_dir * cylinder_length),
      &start_center,
      cylinder_radius,
      divisions,
      material,
      true
    );

    tessellate_cone(
      output_mesh,
      &(start_center + axis_dir * (cylinder_length - cone_offset + cone_length)),
      &(start_center + axis_dir * (cylinder_length - cone_offset)),
      cone_radius,
      divisions,
      material,
      true
    );
}
