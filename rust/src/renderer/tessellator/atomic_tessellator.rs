use crate::renderer::tessellator::tessellator::{self, OccluderSphere};
use crate::common::atomic_structure::{Atom, AtomicStructure, AtomDisplayState, Bond};
use crate::common::common_constants::{ATOM_INFO, DEFAULT_ATOM_INFO};
use crate::common::scene::Scene;
use crate::renderer::mesh::{Mesh, Material};
use crate::api::structure_designer::structure_designer_preferences::{AtomicStructureVisualizationPreferences, AtomicStructureVisualization};
use glam::f64::DVec3;
use glam::f32::Vec3;
use crate::util::timer::Timer;

pub struct AtomicTessellatorParams {
  pub ball_and_stick_sphere_horizontal_divisions: u32, // Ball-and-stick sphere horizontal divisions
  pub ball_and_stick_sphere_vertical_divisions: u32,   // Ball-and-stick sphere vertical divisions
  pub space_filling_sphere_horizontal_divisions: u32,  // Space-filling sphere horizontal divisions
  pub space_filling_sphere_vertical_divisions: u32,    // Space-filling sphere vertical divisions
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
  match atomic_viz_prefs.visualization {
    AtomicStructureVisualization::BallAndStick => {
      if let Some(cull_depth) = atomic_viz_prefs.ball_and_stick_cull_depth {
        atom.in_crystal_depth > cull_depth as f32
      } else {
        false
      }
    }
    AtomicStructureVisualization::SpaceFilling => {
      if let Some(cull_depth) = atomic_viz_prefs.space_filling_cull_depth {
        atom.in_crystal_depth > cull_depth as f32
      } else {
        false
      }
    }
  }
}

pub fn tessellate_atomic_structure<'a, S: Scene<'a>>(output_mesh: &mut Mesh, selected_clusters_mesh: &mut Mesh, atomic_structure: &AtomicStructure, params: &AtomicTessellatorParams, scene: &S, atomic_viz_prefs: &AtomicStructureVisualizationPreferences) {
  let _timer = Timer::new("Atomic tessellation");

  let mut culled_count = 0;
  let mut tessellated_count = 0;
  
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
      culled_count += 1;
      continue;
    }
    
    tessellated_count += 1;
    tessellate_atom(output_mesh, selected_clusters_mesh, atomic_structure, &atom, params, display_state, &atomic_viz_prefs.visualization);
  }
  
  // Only tessellate bonds for ball-and-stick visualization
  if atomic_viz_prefs.visualization == AtomicStructureVisualization::BallAndStick {
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

  println!("Atomic tessellation: {:?} visualization, {} atoms tessellated, {} atoms culled", 
           atomic_viz_prefs.visualization, tessellated_count, culled_count);
}

pub fn get_displayed_atom_radius(atom: &Atom, visualization: &AtomicStructureVisualization) -> f64 {
  let atom_info = ATOM_INFO.get(&atom.atomic_number)
    .unwrap_or(&DEFAULT_ATOM_INFO);
  
  match visualization {
    AtomicStructureVisualization::BallAndStick => atom_info.covalent_radius * BAS_ATOM_RADIUS_FACTOR,
    AtomicStructureVisualization::SpaceFilling => atom_info.van_der_waals_radius,
  }
}

// Calculate occluder spheres for space-filling visualization
fn calculate_occluder_spheres(atom: &Atom, atomic_structure: &AtomicStructure, visualization: &AtomicStructureVisualization) -> Vec<OccluderSphere> {
  let mut occluder_spheres = Vec::new();
  
  // Only calculate occlusion for space-filling mode
  if *visualization != AtomicStructureVisualization::SpaceFilling {
    return occluder_spheres;
  }
  
  // Find bonded neighbors
  for (_bond_id, bond) in atomic_structure.bonds.iter() {
    let neighbor_id = if bond.atom_id1 == atom.id {
      bond.atom_id2
    } else if bond.atom_id2 == atom.id {
      bond.atom_id1
    } else {
      continue; // This bond doesn't involve our atom
    };
    
    if let Some(neighbor) = atomic_structure.atoms.get(&neighbor_id) {
      let neighbor_info = ATOM_INFO.get(&neighbor.atomic_number).unwrap_or(&DEFAULT_ATOM_INFO);
      let neighbor_radius = neighbor_info.van_der_waals_radius;
      
      // Simple approach: use the neighbor's Van der Waals sphere as an occluder
      // Any vertex of our atom that falls inside this sphere will be culled
      occluder_spheres.push(OccluderSphere {
        center: neighbor.position.as_vec3(),
        radius: neighbor_radius as f32,
      });
    }
  }
  
  occluder_spheres
}

pub fn tessellate_atom(output_mesh: &mut Mesh, selected_clusters_mesh: &mut Mesh, _model: &AtomicStructure, atom: &Atom, params: &AtomicTessellatorParams, display_state: AtomDisplayState, visualization: &AtomicStructureVisualization) {
  let atom_info = ATOM_INFO.get(&atom.atomic_number)
    .unwrap_or(&DEFAULT_ATOM_INFO);

  //if atom.atomic_number == 1 {
  //  return; // Temporarily test without Hydrogen
  //}

  let cluster_selected = _model.get_cluster(atom.cluster_id).is_some() && _model.get_cluster(atom.cluster_id).unwrap().selected;
  let selected = atom.selected || cluster_selected;

  // Determine atom color based on selection state (not marking state)
  let atom_color = if selected {
    to_selected_color(&atom_info.color)
  } else { 
    atom_info.color
  };
  
  // Get appropriate tessellation parameters based on visualization mode
  let (horizontal_divisions, vertical_divisions) = match visualization {
    AtomicStructureVisualization::BallAndStick => (
      params.ball_and_stick_sphere_horizontal_divisions,
      params.ball_and_stick_sphere_vertical_divisions
    ),
    AtomicStructureVisualization::SpaceFilling => (
      params.space_filling_sphere_horizontal_divisions,
      params.space_filling_sphere_vertical_divisions
    ),
  };
  
  // Calculate occluder spheres for occlusion culling
  let occluder_spheres = calculate_occluder_spheres(atom, _model, visualization);
  
  // Render the atom sphere with occlusion culling if in space-filling mode
  if *visualization == AtomicStructureVisualization::SpaceFilling && !occluder_spheres.is_empty() {
    tessellator::tessellate_sphere_with_occlusion(
      if cluster_selected { selected_clusters_mesh } else { output_mesh },
      &atom.position,
      get_displayed_atom_radius(atom, visualization),
      horizontal_divisions,
      vertical_divisions,
      &Material::new(
        &atom_color, 
        if selected { 0.2 } else { 0.8 },
        0.0),
      &occluder_spheres,
    );
  } else {
    // Use regular tessellation for ball-and-stick or when no occlusion
    tessellator::tessellate_sphere(
      if cluster_selected { selected_clusters_mesh } else { output_mesh },
      &atom.position,
      get_displayed_atom_radius(atom, visualization),
      horizontal_divisions,
      vertical_divisions,
      &Material::new(
        &atom_color, 
        if selected { 0.2 } else { 0.8 },
        0.0),
    );
  }
  
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
      let radius = get_displayed_atom_radius(atom, visualization);
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

