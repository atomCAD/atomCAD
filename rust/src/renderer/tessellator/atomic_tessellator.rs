use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::kernel::atomic_structure::AtomicStructure;
use crate::kernel::atomic_structure::Atom;
use crate::kernel::atomic_structure::Bond;
use super::tessellator;
use glam::f32::Vec3;
use std::collections::HashMap;
use lazy_static::lazy_static;

pub struct AtomicTessellatorParams {
  pub sphere_horizontal_divisions: u32, // number sections when dividing by horizontal lines
  pub sphere_vertical_divisions: u32, // number of sections when dividing by vertical lines
  pub cylinder_divisions: u32,
}

#[derive(Clone)]
pub struct AtomInfo {
    pub radius: f32,
    pub color: Vec3,
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

pub fn tessellate_atomic_structure(output_mesh: &mut Mesh, atomic_structure: &AtomicStructure, params: &AtomicTessellatorParams) {
  for (_id, atom) in atomic_structure.atoms.iter() {
    tessellate_atom(output_mesh, atomic_structure, &atom, params);
  }
  for (_id, bond) in atomic_structure.bonds.iter() {
    tessellate_bond(output_mesh, atomic_structure, &bond, params);
  }
}

pub fn tessellate_atom(output_mesh: &mut Mesh, _model: &AtomicStructure, atom: &Atom, params: &AtomicTessellatorParams) {
  let atom_info = ATOM_INFO.get(&atom.atomic_number)
    .unwrap_or(&DEFAULT_ATOM_INFO);

  let scaled_radius = atom_info.radius * BAS_ATOM_RADIUS_FACTOR;
  tessellator::tessellate_sphere(
    output_mesh,
    &atom.position,
    scaled_radius,
    params.sphere_horizontal_divisions,
    params.sphere_vertical_divisions,
    &Material::new(&atom_info.color, 0.3, 0.0),
  );
}

pub fn tessellate_bond(output_mesh: &mut Mesh, model: &AtomicStructure, bond: &Bond, params: &AtomicTessellatorParams) {
  let atom_pos1 = model.get_atom(bond.atom_id1).unwrap().position;
  let atom_pos2 = model.get_atom(bond.atom_id2).unwrap().position;
  tessellator::tessellate_cylinder(
    output_mesh,
    &atom_pos2,
    &atom_pos1,
    BAS_STICK_RADIUS,
    params.cylinder_divisions,
    &Material::new(&Vec3::new(0.95, 0.93, 0.88), 0.4, 0.8),
    false,
  );
}

