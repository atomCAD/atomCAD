use super::mesh::Mesh;
use super::mesh::Vertex;
use crate::kernel::model::Model;
use crate::kernel::model::Atom;
use crate::kernel::model::Bond;

/*
 * Tessellator is able to tessellate atoms and bonds into a triangle mesh
 */
pub struct Tessellator {
  output_mesh: Mesh,
  sphere_horizontal_divisions: i32, // number sections when dividing by horizontal lines
  sphere_vertical_divisions: i32, // number of sections when dividing by vertical lines
  cylinder_divisions: i32,
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

  pub fn set_sphere_divisions(&mut self, arg_sphere_horizontal_divisions: i32, arg_sphere_vertical_divisions: i32) {
    self.sphere_horizontal_divisions = arg_sphere_horizontal_divisions;
    self.sphere_vertical_divisions = arg_sphere_vertical_divisions;
  }

  pub fn set_cylinder_divisions(&mut self, arg_cylinder_divisions: i32) {
    self.cylinder_divisions = arg_cylinder_divisions;
  }

  pub fn add_atom(&mut self, model: &Model, atom: &Atom) {
    //TODO***
  }

  pub fn add_bond(&mut self, model: &Model, bond: &Bond) {
    //TODO***
  }

  fn add_sphere(&mut self) {
    // TODO***
  }

  fn add_cylinder(&mut self) {
    // TODO***
  }
}
