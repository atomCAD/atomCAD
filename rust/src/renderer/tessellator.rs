use super::mesh::Mesh;
use super::mesh::Vertex;
use crate::kernel::model::Model;
use crate::kernel::model::Atom;
use crate::kernel::model::Bond;
use glam::f32::Vec3;
use glam::f32::Quat;

/*
 * Tessellator is able to tessellate atoms and bonds into a triangle mesh
 */
pub struct Tessellator {
  pub output_mesh: Mesh,
  sphere_horizontal_divisions: u32, // number sections when dividing by horizontal lines
  sphere_vertical_divisions: u32, // number of sections when dividing by vertical lines
  cylinder_divisions: u32,
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

  pub fn add_atom(&mut self, model: &Model, atom: &Atom) {
    // TODO: atomic radii. also enum for view type (atomic radii depend on that too)
    // TODO: color depends on atomic number and selection
    self.add_sphere(&atom.position, 1.0, &Vec3::new(0.8, 0.0, 0.0), 0.3, 0.0);
  }

  pub fn add_bond(&mut self, model: &Model, bond: &Bond) {
    let atom_pos1 = model.get_atom(bond.atom_id1).unwrap().position;
    let atom_pos2 = model.get_atom(bond.atom_id2).unwrap().position;
    // TODO: radius
    self.add_cylinder(&atom_pos2, &atom_pos1, 0.3, &Vec3::new(0.95, 0.93, 0.88), 0.4, 0.8);
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
