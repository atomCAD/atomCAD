use super::super::mesh::Mesh;
use super::super::mesh::Vertex;
use super::super::mesh::Material;
use glam::f32::Vec3;
use glam::f32::Quat;

  // provide the positions in counter clockwise order
  pub fn tessellate_quad(
    output_mesh: &mut Mesh,
    pos0: &Vec3,
    pos1: &Vec3,
    pos2: &Vec3,
    pos3: &Vec3,
    normal: &Vec3,
    material: &Material,
  ) {
    let index0 = output_mesh.add_vertex(Vertex::new(pos0, normal, material));
    let index1 = output_mesh.add_vertex(Vertex::new(pos1, normal, material));
    let index2 = output_mesh.add_vertex(Vertex::new(pos2, normal, material));
    let index3 = output_mesh.add_vertex(Vertex::new(pos3, normal, material));
    output_mesh.add_quad(index0, index1, index2, index3);
}

pub fn tessellate_cuboid(
  output_mesh: &mut Mesh,
  center: &Vec3,
  size: &Vec3,
  rotator: &Quat,
  top_material: &Material,
  bottom_material: &Material,
  side_material: &Material,
) {
  // Create vertices for cuboid
  let half_size = size * 0.5;
  let vertices = [
    // Top face vertices
    center + rotator.mul_vec3(Vec3::new(-half_size.x, half_size.y, -half_size.z)),
    center + rotator.mul_vec3(Vec3::new(half_size.x, half_size.y, -half_size.z)),
    center + rotator.mul_vec3(Vec3::new(half_size.x, half_size.y, half_size.z)),
    center + rotator.mul_vec3(Vec3::new(-half_size.x, half_size.y, half_size.z)),
    // Bottom face vertices
    center + rotator.mul_vec3(Vec3::new(-half_size.x, - half_size.y, -half_size.z)),
    center + rotator.mul_vec3(Vec3::new(half_size.x, - half_size.y, -half_size.z)),
    center + rotator.mul_vec3(Vec3::new(half_size.x, - half_size.y, half_size.z)),
    center + rotator.mul_vec3(Vec3::new(-half_size.x, - half_size.y, half_size.z)),
  ];

  // Add the six faces of the cuboid
  // Top face
  tessellate_quad(
    output_mesh,
    &vertices[3], &vertices[2], &vertices[1], &vertices[0],
    &rotator.mul_vec3(Vec3::Y),
    &top_material
  );

  // Bottom face
  tessellate_quad(
    output_mesh,
    &vertices[4], &vertices[5], &vertices[6], &vertices[7],
    &rotator.mul_vec3(-Vec3::Y),
    &bottom_material
  );

  // Front face
  tessellate_quad(
    output_mesh,
    &vertices[2], &vertices[3], &vertices[7], &vertices[6],
    &rotator.mul_vec3(Vec3::Z),
    &side_material
  );

  // Back face
  tessellate_quad(
    output_mesh,
    &vertices[0], &vertices[1], &vertices[5], &vertices[4],
    &rotator.mul_vec3(-Vec3::Z),
    &side_material
  );

  // Right face
  tessellate_quad(
    output_mesh,
    &vertices[1], &vertices[2], &vertices[6], &vertices[5],
    &rotator.mul_vec3(Vec3::X),
    &side_material
  );

  // Left face
  tessellate_quad(
    output_mesh,
    &vertices[3], &vertices[0], &vertices[4], &vertices[7],
    &rotator.mul_vec3(-Vec3::X),
    &side_material
  );
}

pub fn tessellate_circle_sheet (
    output_mesh: &mut Mesh,
    center: &Vec3,
    normal: &Vec3,
    radius: f32,
    divisions: u32,
    material: &Material,
) 
{
  let rotation = Quat::from_rotation_arc(Vec3::new(0.0, 1.0, 0.0), *normal);

  let center_index = output_mesh.add_vertex(Vertex::new(
    &center,
    &normal,
    material,
  ));

  let index_start = output_mesh.vertices.len() as u32;

  for x in 0..divisions {
    let u = (x as f32) / (divisions as f32); // u runs from 0 to 1
    let theta = u * 2.0 * std::f32::consts::PI; // From 0 to 2*PI
    let out_normal = Vec3::new(theta.sin(), 0.0, theta.cos());

    let position = center + rotation.mul_vec3(out_normal * radius);
    
    output_mesh.add_vertex(Vertex::new(
      &position,
      &normal,
      material,
    ));

    let offset = index_start + x;
    let next_offset = index_start + (x + 1) % divisions;
    
    output_mesh.add_triangle(center_index, offset, next_offset);
  }
}

pub fn tessellate_sphere(
    output_mesh: &mut Mesh,
    center: &Vec3,
    radius: f32,
    horizontal_divisions: u32, // number sections when dividing by horizontal lines
    vertical_divisions: u32, // number of sections when dividing by vertical lines
    material: &Material,
) {

  // ---------- Add vertices ----------

  let north_pole_index = output_mesh.add_vertex(Vertex::new(
    &Vec3::new(center.x, center.y + radius, center.z),
    &Vec3::new(0.0, 1.0, 0.0),
    material,
  ));

  let south_pole_index = output_mesh.add_vertex(Vertex::new(
    &Vec3::new(center.x, center.y - radius, center.z),
    &Vec3::new(0.0, -1.0, 0.0),
    material,
  ));

  let non_pole_index_start = output_mesh.vertices.len() as u32;

  for y in 1..vertical_divisions {
    let v = (y as f32) / (vertical_divisions as f32); // v runs from 0 to 1
    let phi = v * std::f32::consts::PI; // From 0 to PI (latitude)    
    for x in 0..horizontal_divisions {
      let u = (x as f32) / (horizontal_divisions as f32); // u runs from 0 to 1
      let theta = u * 2.0 * std::f32::consts::PI; // From 0 to 2*PI (longitude)

      let normal = Vec3::new(theta.sin() * phi.sin(), phi.cos(), theta.cos() * phi.sin());
      let position = normal * radius + center;

      output_mesh.add_vertex(Vertex::new(
        &position,
        &normal,
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
    top_center: &Vec3,
    bottom_center: &Vec3,
    radius: f32,
    divisions: u32,
    material: &Material,
    include_top_and_bottom: bool) {
  let center = (top_center + bottom_center) * 0.5;
  let dir = (top_center - bottom_center).normalize();
  let up = dir;
  let down = dir * -1.0;
  let length = (top_center - bottom_center).length();
  let rotation = Quat::from_rotation_arc(Vec3::new(0.0, 1.0, 0.0), dir);
  
  let index_start = output_mesh.vertices.len() as u32;
  for x in 0..divisions {
    let u = (x as f32) / (divisions as f32); // u runs from 0 to 1
    let theta = u * 2.0 * std::f32::consts::PI; // From 0 to 2*PI

    let normal = Vec3::new(theta.sin(), 0.0, theta.cos());
    let bottom_position = center + rotation.mul_vec3(Vec3::new(0.0, -length * 0.5, 0.0) + normal * radius);
    let top_position = center + rotation.mul_vec3(Vec3::new(0.0, length * 0.5, 0.0) + normal * radius);

    output_mesh.add_vertex(Vertex::new(
      &bottom_position,
      &normal,
      material,
    ));

    output_mesh.add_vertex(Vertex::new(
      &top_position,
      &normal,
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

pub fn tessellate_grid(
    output_mesh: &mut Mesh,
    center: &Vec3,
    rotator: &Quat,
    thickness: f32,
    width: f32,
    height: f32,
    line_width: f32,
    grid_unit: f32,
    top_material: &Material,
    bottom_material: &Material,
    side_material: &Material,
) {

  let horiz_divisions = (width / grid_unit).ceil() as u32;
  let vert_divisions = (height / grid_unit).ceil() as u32;

  let start_x =  (- width * 0.5);
  let start_z =  (- height * 0.5);
  for x in 0..horiz_divisions {

    let cuboid_center = center + rotator.mul_vec3(Vec3::new(start_x + (x as f32) * grid_unit, -thickness * 0.5, 0.0));

    tessellate_cuboid(
      output_mesh,
      &cuboid_center,
      &(Vec3::new(line_width, thickness, height)),
      rotator,
      top_material,
      bottom_material,
      side_material,
    );    
  }
  for z in 0..vert_divisions {

    let cuboid_center = center + rotator.mul_vec3(Vec3::new(0.0, -thickness * 0.5, start_z + (z as f32) * grid_unit));

    tessellate_cuboid(
      output_mesh,
      &cuboid_center,
      &(Vec3::new(width, thickness, line_width)),
      rotator,
      top_material,
      bottom_material,
      side_material,
    );
  }

      /*
  tessellate_cuboid(
    output_mesh,
    center,
    &(Vec3::new(width, thickness, height)),
    rotator,
    top_material,
    bottom_material,
    side_material,
  );
  */
}
