use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure::Atom;
use crate::common::atomic_structure::Bond;
use crate::common::atomic_structure::AtomDisplayState;
use crate::common::common_constants::DEFAULT_ATOM_INFO;
use crate::common::common_constants::ATOM_INFO;
use crate::common::scene::Scene;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualizationPreferences;
use super::tessellator;
use glam::f32::Vec3;
use glam::f64::DVec3;

pub struct AtomicTessellatorParams {
  pub sphere_horizontal_divisions: u32, // number sections when dividing by horizontal lines
  pub sphere_vertical_divisions: u32, // number of sections when dividing by vertical lines
  pub cylinder_divisions: u32,
}

// atom radius factor for the 'balls and sticks' view
const BAS_ATOM_RADIUS_FACTOR: f64 = 0.5;

// radius of a bond cylinder (stick) in the 'balls and sticks' view
pub const BAS_STICK_RADIUS: f64 = 0.1; 

// color for primary markers (bright yellow)
const MARKER_COLOR: Vec3 = Vec3::new(1.0, 1.0, 0.0);
// color for secondary markers (blue)
const SECONDARY_MARKER_COLOR: Vec3 = Vec3::new(0.0, 0.5, 1.0);

/// Helper function to determine if an atom should be culled based on depth
fn should_cull_atom(atom: &Atom, atomic_viz_prefs: &AtomicStructureVisualizationPreferences) -> bool {
  if let Some(cull_depth) = atomic_viz_prefs.ball_and_stick_cull_depth {
    atom.in_crystal_depth > cull_depth
  } else {
    false
  }
}

pub fn tessellate_atomic_structure<'a, S: Scene<'a>>(output_mesh: &mut Mesh, selected_clusters_mesh: &mut Mesh, atomic_structure: &AtomicStructure, params: &AtomicTessellatorParams, scene: &S, atomic_viz_prefs: &AtomicStructureVisualizationPreferences) {
  for (id, atom) in atomic_structure.atoms.iter() {
    // Get display state from the decorator and override it to Marked if scene.is_atom_marked is true
    let mut display_state = atomic_structure.decorator.get_atom_display_state(*id);
    if scene.is_atom_marked(*id) {
      display_state = AtomDisplayState::Marked;
    } else if scene.is_atom_secondary_marked(*id) {
      display_state = AtomDisplayState::SecondaryMarked;
    }
    
    // Apply depth culling if enabled
    if should_cull_atom(atom, atomic_viz_prefs) {
      // Skip tessellating this atom - it's too deep inside and can't be seen
      continue;
    }
    
    tessellate_atom(output_mesh, selected_clusters_mesh, atomic_structure, &atom, params, display_state);
  }
  
  for (_id, bond) in atomic_structure.bonds.iter() {
    // Check if both atoms of the bond should be rendered
    let atom1 = atomic_structure.atoms.get(&bond.atom_id1);
    let atom2 = atomic_structure.atoms.get(&bond.atom_id2);
    
    if let (Some(atom1), Some(atom2)) = (atom1, atom2) {
      // Only tessellate the bond if both atoms are being rendered (not culled)
      if !should_cull_atom(atom1, atomic_viz_prefs) && !should_cull_atom(atom2, atomic_viz_prefs) {
        tessellate_bond(output_mesh, selected_clusters_mesh, atomic_structure, &bond, params);
      }
    }
  }
}

pub fn get_displayed_atom_radius(atom: &Atom) -> f64 {
  let atom_info = ATOM_INFO.get(&atom.atomic_number)
    .unwrap_or(&DEFAULT_ATOM_INFO);
  atom_info.radius * BAS_ATOM_RADIUS_FACTOR
}

pub fn tessellate_atom(output_mesh: &mut Mesh, selected_clusters_mesh: &mut Mesh, _model: &AtomicStructure, atom: &Atom, params: &AtomicTessellatorParams, display_state: AtomDisplayState) {
  let atom_info = ATOM_INFO.get(&atom.atomic_number)
    .unwrap_or(&DEFAULT_ATOM_INFO);

  let cluster_selected = _model.get_cluster(atom.cluster_id).is_some() && _model.get_cluster(atom.cluster_id).unwrap().selected;
  let selected = atom.selected || cluster_selected;

  // Determine atom color based on selection state (not marking state)
  let atom_color = if selected {
    to_selected_color(&atom_info.color)
  } else { 
    atom_info.color
  };
  
  // Render the atom sphere with its normal color
  tessellator::tessellate_sphere(
    if cluster_selected { selected_clusters_mesh } else { output_mesh },
    &atom.position,
    get_displayed_atom_radius(atom),
    params.sphere_horizontal_divisions,
    params.sphere_vertical_divisions,
    &Material::new(
      &atom_color, 
      if selected { 0.2 } else { 0.8 },
      0.0),
  );
  
  // Add a 3D crosshair for marked atoms
  match display_state {
    AtomDisplayState::Marked | AtomDisplayState::SecondaryMarked => {
      // Select color based on display state
      let marker_color = match display_state {
        AtomDisplayState::Marked => MARKER_COLOR,         // Yellow for primary marked atoms
        AtomDisplayState::SecondaryMarked => SECONDARY_MARKER_COLOR, // Blue for secondary marked atoms
        _ => unreachable!() // This branch already ensures we're in one of the two marked states
      };
      
      // Calculate crosshair dimensions
      let radius = get_displayed_atom_radius(atom);
      let half_length = radius * 1.5;
      let crosshair_radius = radius * 0.4;

      // Render the crosshair
      tessellator::tessellate_crosshair_3d(
        if cluster_selected { selected_clusters_mesh } else { output_mesh },
        &DVec3::new(atom.position.x, atom.position.y, atom.position.z),
        half_length,
        crosshair_radius,
        params.cylinder_divisions,
        &Material::new(&marker_color, 1.0, 0.0),
        true
      );
    },
    AtomDisplayState::Normal => {
      // No marker for normal atoms
    }
  }
}

fn to_selected_color(_color: &Vec3) -> Vec3 {
  Vec3::new(1.0, 0.2, 1.0) // Bright magenta for selected atoms
}

pub fn tessellate_bond(output_mesh: &mut Mesh, selected_clusters_mesh: &mut Mesh, model: &AtomicStructure, bond: &Bond, params: &AtomicTessellatorParams) {
  let atom_pos1 = model.get_atom(bond.atom_id1).unwrap().position;
  let atom_pos2 = model.get_atom(bond.atom_id2).unwrap().position;

  let cluster_id1 = model.get_atom(bond.atom_id1).unwrap().cluster_id;
  let cluster_id2 = model.get_atom(bond.atom_id2).unwrap().cluster_id;

  let cluster_selected1 = model.get_cluster(cluster_id1).is_some() && model.get_cluster(cluster_id1).unwrap().selected;
  let cluster_selected2 = model.get_cluster(cluster_id2).is_some() && model.get_cluster(cluster_id2).unwrap().selected;

  let selected = bond.selected || cluster_selected1 || cluster_selected2;

  let color = if selected {
    to_selected_color(&Vec3::new(0.8, 0.8, 0.8))
  } else {
    Vec3::new(0.8, 0.8, 0.8)
  };

  tessellator::tessellate_cylinder(
    if cluster_selected1 && cluster_selected2 { selected_clusters_mesh } else { output_mesh },
    &atom_pos2,
    &atom_pos1,
    BAS_STICK_RADIUS,
    params.cylinder_divisions,
    &Material::new(
      &color, 
      if selected { 0.2 } else { 0.8 }, 
      0.0),
    false,
    None,
    None
  );
}

