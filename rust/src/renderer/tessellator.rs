use super::mesh::Mesh;
use super::mesh::Vertex;
use crate::kernel::atomic_structure::AtomicStructure;
use crate::kernel::atomic_structure::Atom;
use crate::kernel::atomic_structure::Bond;
use crate::kernel::surface_point_cloud::SurfacePoint;
use glam::f32::Vec3;
use glam::f32::Quat;
use std::collections::HashMap;
use lazy_static::lazy_static;

/*
 * Tessellator is able to tessellate atoms, bonds and surface points into a triangle mesh
 */
pub struct Tessellator {
  pub output_mesh: Mesh,
  sphere_horizontal_divisions: u32, // number sections when dividing by horizontal lines
  sphere_vertical_divisions: u32, // number of sections when dividing by vertical lines
  cylinder_divisions: u32,
}

#[derive(Clone)]
pub struct AtomInfo {
    radius: f32,
    color: Vec3,
}

// atom radius factor for the 'balls and sticks' view
const BAS_ATOM_RADIUS_FACTOR: f32 = 0.5;

// radius of a bond cylinder (stick) in the 'balls and sticks' view
const BAS_STICK_RADIUS: f32 = 0.1; 

lazy_static! {
    static ref DEFAULT_ATOM_INFO: AtomInfo = AtomInfo {
        radius: 0.7,
        color: Vec3::new(0.5, 0.5, 0.5)  // Default gray for unknown atoms
    };

    static ref ATOM_INFO: HashMap<i32, AtomInfo> = {
        let mut m = HashMap::new();
        // Values based on common atomic radii (in Angstroms) and typical visualization colors
        m.insert(1, AtomInfo { radius: 0.25, color: Vec3::new(1.0, 1.0, 1.0) });  // Hydrogen - white
        m.insert(6, AtomInfo { radius: 0.70, color: Vec3::new(0.1, 1.0, 0.1) });  // Carbon - dark grey
        m.insert(7, AtomInfo { radius: 0.65, color: Vec3::new(0.2, 0.2, 1.0) });  // Nitrogen - blue
        m.insert(8, AtomInfo { radius: 0.60, color: Vec3::new(1.0, 0.0, 0.0) });  // Oxygen - red
        m
    };
}

impl Tessellator {

  pub fn new() -> Self {
    Self {
      output_mesh: Mesh::new(),
      sphere_horizontal_divisions: 8,
      sphere_vertical_divisions: 16,
      cylinder_divisions: 16,
    }
  }

  pub fn set_sphere_divisions(&mut self, arg_sphere_horizontal_divisions: u32, arg_sphere_vertical_divisions: u32) {
    self.sphere_horizontal_divisions = arg_sphere_horizontal_divisions;
    self.sphere_vertical_divisions = arg_sphere_vertical_divisions;
  }

  pub fn set_cylinder_divisions(&mut self, arg_cylinder_divisions: u32) {
    self.cylinder_divisions = arg_cylinder_divisions;
  }

  pub fn add_atom(&mut self, _model: &AtomicStructure, atom: &Atom) {
    let atom_info = ATOM_INFO.get(&atom.atomic_number)
        .unwrap_or(&DEFAULT_ATOM_INFO);
        
    let scaled_radius = atom_info.radius * BAS_ATOM_RADIUS_FACTOR;
    self.add_sphere(&atom.position, scaled_radius, &atom_info.color, 0.3, 0.0);
  }

  pub fn add_surface_point(&mut self, point: &SurfacePoint) {
    let roughness: f32 = 0.5;
    let metallic: f32 = 0.0;
    let outside_albedo = Vec3::new(0.0, 0.0, 1.0);
    let inside_albedo = Vec3::new(1.0, 0.0, 0.0);
    let side_albedo = Vec3::new(0.5, 0.5, 0.5);

    // Create rotation quaternion from surface normal to align cuboid
    let rotator = Quat::from_rotation_arc(Vec3::Y, point.normal);

    // Create vertices for cuboid
    let half_size = Vec3::new(0.06, 0.02, 0.06); // x, y, z extents
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
    self.add_quad(
        &vertices[3], &vertices[2], &vertices[1], &vertices[0],
        &rotator.mul_vec3(Vec3::Y),
        &outside_albedo, roughness, metallic
    );

    // Bottom face
    self.add_quad(
        &vertices[4], &vertices[5], &vertices[6], &vertices[7],
        &rotator.mul_vec3(-Vec3::Y),
        &inside_albedo, roughness, metallic
    );

    // Front face
    self.add_quad(
        &vertices[2], &vertices[3], &vertices[7], &vertices[6],
        &rotator.mul_vec3(Vec3::Z),
        &side_albedo, roughness, metallic
    );

    // Back face
    self.add_quad(
        &vertices[0], &vertices[1], &vertices[5], &vertices[4],
        &rotator.mul_vec3(-Vec3::Z),
        &side_albedo, roughness, metallic
    );

    // Right face
    self.add_quad(
        &vertices[1], &vertices[2], &vertices[6], &vertices[5],
        &rotator.mul_vec3(Vec3::X),
        &side_albedo, roughness, metallic
    );

    // Left face
    self.add_quad(
        &vertices[3], &vertices[0], &vertices[4], &vertices[7],
        &rotator.mul_vec3(-Vec3::X),
        &side_albedo, roughness, metallic
    );
  }

  // provide the positions in counter clockwise order
  fn add_quad(
    &mut self,
    pos0: &Vec3,
    pos1: &Vec3,
    pos2: &Vec3,
    pos3: &Vec3,
    normal: &Vec3,
    albedo: &Vec3,
    roughness: f32,
    metallic: f32,
  ) {
    let index0 = self.output_mesh.add_vertex(Vertex::new(pos0, normal, albedo, roughness, metallic));
    let index1 = self.output_mesh.add_vertex(Vertex::new(pos1, normal, albedo, roughness, metallic));
    let index2 = self.output_mesh.add_vertex(Vertex::new(pos2, normal, albedo, roughness, metallic));
    let index3 = self.output_mesh.add_vertex(Vertex::new(pos3, normal, albedo, roughness, metallic));
    self.output_mesh.add_quad(index0, index1, index2, index3);
  }

  pub fn add_bond(&mut self, model: &AtomicStructure, bond: &Bond) {
    let atom_pos1 = model.get_atom(bond.atom_id1).unwrap().position;
    let atom_pos2 = model.get_atom(bond.atom_id2).unwrap().position;
    self.add_cylinder(&atom_pos2, &atom_pos1, BAS_STICK_RADIUS, &Vec3::new(0.95, 0.93, 0.88), 0.4, 0.8);
  }

  fn add_sphere(&mut self, center: &Vec3, radius: f32, albedo: &Vec3, roughness: f32, metallic: f32) {

    // ---------- Add vertices ----------

    let north_pole_index = self.output_mesh.add_vertex(Vertex::new(
      &Vec3::new(center.x, center.y + radius, center.z),
      &Vec3::new(0.0, 1.0, 0.0),
      albedo,
      roughness,
      metallic,
    ));

    let south_pole_index = self.output_mesh.add_vertex(Vertex::new(
      &Vec3::new(center.x, center.y - radius, center.z),
      &Vec3::new(0.0, -1.0, 0.0),
      albedo,
      roughness,
      metallic,
    ));

    let non_pole_index_start = self.output_mesh.vertices.len() as u32;

    for y in 1..self.sphere_vertical_divisions {
      let v = (y as f32) / (self.sphere_vertical_divisions as f32); // v runs from 0 to 1
      let phi = v * std::f32::consts::PI; // From 0 to PI (latitude)    
      for x in 0..self.sphere_horizontal_divisions {
        let u = (x as f32) / (self.sphere_horizontal_divisions as f32); // u runs from 0 to 1
        let theta = u * 2.0 * std::f32::consts::PI; // From 0 to 2*PI (longitude)

        let normal = Vec3::new(theta.sin() * phi.sin(), phi.cos(), theta.cos() * phi.sin());
        let position = normal * radius + center;

        self.output_mesh.add_vertex(Vertex::new(
          &position,
          &normal,
          albedo,
          roughness,
          metallic,
        ));
      } // end of for x
    } // end of for y

    // ---------- add indices ----------

    // Add north pole triangles
    for x in 0..self.sphere_horizontal_divisions {
      self.output_mesh.add_triangle(
        north_pole_index,
        non_pole_index_start + x % self.sphere_horizontal_divisions,
        non_pole_index_start + (x + 1) % self.sphere_horizontal_divisions,
      );
    }

    // Add south pole triangles
    let last_longitude_index_start = non_pole_index_start + (self.sphere_vertical_divisions - 2) * self.sphere_horizontal_divisions;
    for x in 0..self.sphere_horizontal_divisions {
      self.output_mesh.add_triangle(
        south_pole_index,
        last_longitude_index_start + (x + 1) % self.sphere_horizontal_divisions,
        last_longitude_index_start + x % self.sphere_horizontal_divisions,
      );
    }

    // Add quads
    for y in 1..(self.sphere_vertical_divisions - 1) {
      let offset = non_pole_index_start + (y - 1) * self.sphere_horizontal_divisions;
      for x in 0..self.sphere_horizontal_divisions {
        self.output_mesh.add_quad(
          offset + (x + 1) % self.sphere_horizontal_divisions,
          offset + x % self.sphere_horizontal_divisions,
          offset + self.sphere_horizontal_divisions + x % self.sphere_horizontal_divisions,
          offset + self.sphere_horizontal_divisions + (x + 1) % self.sphere_horizontal_divisions,
        );
      }
    }
  }

  fn add_cylinder(&mut self, top_center: &Vec3, bottom_center: &Vec3, radius: f32, albedo: &Vec3, roughness: f32, metallic: f32) {
    let center = (top_center + bottom_center) * 0.5;
    let dir = (top_center - bottom_center).normalize();
    let length = (top_center - bottom_center).length();
    let rotation = Quat::from_rotation_arc(Vec3::new(0.0, 1.0, 0.0), dir);
    let index_start = self.output_mesh.vertices.len() as u32;
    for x in 0..self.cylinder_divisions {
      let u = (x as f32) / (self.cylinder_divisions as f32); // u runs from 0 to 1
      let theta = u * 2.0 * std::f32::consts::PI; // From 0 to 2*PI

      let normal = Vec3::new(theta.sin(), 0.0, theta.cos());
      let bottom_position = center + rotation.mul_vec3(Vec3::new(0.0, -length * 0.5, 0.0) + normal * radius);
      let top_position = center + rotation.mul_vec3(Vec3::new(0.0, length * 0.5, 0.0) + normal * radius);

      self.output_mesh.add_vertex(Vertex::new(
        &bottom_position,
        &normal,
        albedo,
        roughness,
        metallic,
      ));

      self.output_mesh.add_vertex(Vertex::new(
        &top_position,
        &normal,
        albedo,
        roughness,
        metallic,
      ));

      let offset = index_start + 2 * x;
      let next_offset = index_start + 2 * ((x + 1) % self.cylinder_divisions);

      self.output_mesh.add_quad(
        offset, // bottom
        next_offset, // next bottom
        next_offset + 1, // next top
        offset + 1 // top
      );
    }
  }
}
