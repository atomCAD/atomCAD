use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure::Atom;
use crate::common::atomic_structure::Bond;
use crate::common::common_constants::DEFAULT_ATOM_INFO;
use crate::common::common_constants::ATOM_INFO;
use crate::common::scene::Scene;
use super::tessellator;
use glam::f32::Vec3;

pub struct AtomicTessellatorParams {
  pub sphere_horizontal_divisions: u32, // number sections when dividing by horizontal lines
  pub sphere_vertical_divisions: u32, // number of sections when dividing by vertical lines
  pub cylinder_divisions: u32,
}

// atom radius factor for the 'balls and sticks' view
const BAS_ATOM_RADIUS_FACTOR: f64 = 0.5;

// radius of a bond cylinder (stick) in the 'balls and sticks' view
const BAS_STICK_RADIUS: f64 = 0.1; 

// color for marked atoms (bright yellow)
const MARKED_ATOM_COLOR: Vec3 = Vec3::new(1.0, 1.0, 0.0);

pub fn tessellate_atomic_structure<'a, S: Scene<'a>>(output_mesh: &mut Mesh, selected_clusters_mesh: &mut Mesh, atomic_structure: &AtomicStructure, params: &AtomicTessellatorParams, scene: &S) {
  for (id, atom) in atomic_structure.atoms.iter() {
    tessellate_atom(output_mesh, selected_clusters_mesh, atomic_structure, &atom, params, scene.is_atom_marked(*id));
  }
  for (_id, bond) in atomic_structure.bonds.iter() {
    tessellate_bond(output_mesh, selected_clusters_mesh, atomic_structure, &bond, params);
  }
}

pub fn get_displayed_atom_radius(atom: &Atom) -> f64 {
  let atom_info = ATOM_INFO.get(&atom.atomic_number)
    .unwrap_or(&DEFAULT_ATOM_INFO);
  atom_info.radius * BAS_ATOM_RADIUS_FACTOR
}

pub fn tessellate_atom(output_mesh: &mut Mesh, selected_clusters_mesh: &mut Mesh, _model: &AtomicStructure, atom: &Atom, params: &AtomicTessellatorParams, is_marked: bool) {
  let atom_info = ATOM_INFO.get(&atom.atomic_number)
    .unwrap_or(&DEFAULT_ATOM_INFO);

  let cluster_selected = _model.get_cluster(atom.cluster_id).is_some() && _model.get_cluster(atom.cluster_id).unwrap().selected;
  let selected = atom.selected || cluster_selected;

  let color = if is_marked {
    // Yellow color for marked atoms
    MARKED_ATOM_COLOR
  } else if selected {
    to_selected_color(&atom_info.color)
  } else { 
    atom_info.color
  };

  tessellator::tessellate_sphere(
    if cluster_selected { selected_clusters_mesh } else { output_mesh },
    &atom.position,
    get_displayed_atom_radius(atom),
    params.sphere_horizontal_divisions,
    params.sphere_vertical_divisions,
    &Material::new(
      &color, 
      if selected { 0.2 } else { 0.8 },
      0.0),
  );
}

fn to_selected_color(color: &Vec3) -> Vec3 {
  color * 0.7
}

pub fn tessellate_bond(output_mesh: &mut Mesh, selected_clusters_mesh: &mut Mesh, model: &AtomicStructure, bond: &Bond, params: &AtomicTessellatorParams) {
  let atom_pos1 = model.get_atom(bond.atom_id1).unwrap().position;
  let atom_pos2 = model.get_atom(bond.atom_id2).unwrap().position;

  let cluster_id1 = model.get_atom(bond.atom_id1).unwrap().cluster_id;
  let cluster_id2 = model.get_atom(bond.atom_id2).unwrap().cluster_id;

  let cluster_selected1 = model.get_cluster(cluster_id1).is_some() && model.get_cluster(cluster_id1).unwrap().selected;
  let cluster_selected2 = model.get_cluster(cluster_id2).is_some() && model.get_cluster(cluster_id2).unwrap().selected;

  tessellator::tessellate_cylinder(
    if cluster_selected1 && cluster_selected2 { selected_clusters_mesh } else { output_mesh },
    &atom_pos2,
    &atom_pos1,
    BAS_STICK_RADIUS,
    params.cylinder_divisions,
    &Material::new(&Vec3::new(0.95, 0.93, 0.88), 0.4, 0.8),
    false,
  );
}

