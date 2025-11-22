use crate::renderer::tessellator::tessellator::{self, OccluderSphere};
use crate::common::atomic_structure::{Atom, AtomicStructure, AtomDisplayState, Bond};
use crate::common::common_constants::{ATOM_INFO, DEFAULT_ATOM_INFO};
// Scene trait removed - is_atom_marked was deprecated and always returned false
use crate::renderer::mesh::{Mesh, Material};
use crate::renderer::atom_impostor_mesh::AtomImpostorMesh;
use crate::renderer::bond_impostor_mesh::BondImpostorMesh;
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

pub fn tessellate_atomic_structure(output_mesh: &mut Mesh, atomic_structure: &AtomicStructure, params: &AtomicTessellatorParams, atomic_viz_prefs: &AtomicStructureVisualizationPreferences) {
  let _timer = Timer::new("Atomic tessellation");

  // Pre-allocate mesh capacity for worst-case scenario (no compression)
  let total_atoms = atomic_structure.atoms.len();
  let (h_div, v_div) = match atomic_viz_prefs.visualization {
    AtomicStructureVisualization::BallAndStick => (
      params.ball_and_stick_sphere_horizontal_divisions,
      params.ball_and_stick_sphere_vertical_divisions
    ),
    AtomicStructureVisualization::SpaceFilling => (
      params.space_filling_sphere_horizontal_divisions,
      params.space_filling_sphere_vertical_divisions
    ),
  };
  
  // Worst-case vertices per sphere: (h_div * (v_div - 1)) + 2 (poles)
  let vertices_per_sphere = (h_div * (v_div - 1)) + 2;
  // Worst-case triangles per sphere: h_div * 2 + (h_div * (v_div - 2) * 2)
  let triangles_per_sphere = h_div * 2 + (h_div * (v_div - 2) * 2);
  let indices_per_sphere = triangles_per_sphere * 3;
  
  // Reserve capacity for worst case (all atoms tessellated, no compression)
  let estimated_vertices = total_atoms * vertices_per_sphere as usize;
  let estimated_indices = total_atoms * indices_per_sphere as usize;
  
  output_mesh.vertices.reserve(estimated_vertices);
  output_mesh.indices.reserve(estimated_indices);

  // Create reusable data structures for all sphere tessellations
  let mut reusable_occludable_mesh = tessellator::OccludableMesh::new();
  let mut reusable_occluder_array = OccluderArray::new();

  let mut culled_count = 0;
  let mut tessellated_count = 0;
  
  for (id, atom) in atomic_structure.atoms.iter() {
    // Get effective display state (considering scene markers)
    let display_state = get_atom_display_state(*id, atomic_structure);
    
    // Apply depth culling if enabled
    if should_cull_atom(atom, atomic_viz_prefs) {
      // Skip tessellating this atom - it's too deep inside and can't be seen
      culled_count += 1;
      continue;
    }
    
    tessellated_count += 1;
    tessellate_atom(output_mesh, atomic_structure, &atom, params, display_state, &atomic_viz_prefs.visualization, &mut reusable_occludable_mesh, &mut reusable_occluder_array);
  }
  
  // Only tessellate bonds for ball-and-stick visualization
  if atomic_viz_prefs.visualization == AtomicStructureVisualization::BallAndStick {
    for (_id, bond) in atomic_structure.bonds.iter() {
      if should_render_bond(bond, atomic_structure, atomic_viz_prefs) {
        tessellate_bond(output_mesh, atomic_structure, &bond, params);
      }
    }
  }

  println!("Atomic tessellation: {:?} visualization, {} atoms tessellated, {} atoms culled", 
           atomic_viz_prefs.visualization, tessellated_count, culled_count);
}

pub fn get_displayed_atom_radius(atom: &Atom, visualization: &AtomicStructureVisualization) -> f64 {
  let atom_info = ATOM_INFO.get(&(atom.atomic_number as i32))
    .unwrap_or(&DEFAULT_ATOM_INFO);
  
  match visualization {
    AtomicStructureVisualization::BallAndStick => atom_info.covalent_radius * BAS_ATOM_RADIUS_FACTOR,
    AtomicStructureVisualization::SpaceFilling => atom_info.van_der_waals_radius,
  }
}

/// Shared helper to get atom color and material properties based on selection state
fn get_atom_color_and_material(atom: &Atom) -> (Vec3, f32, f32) {
  let atom_info = ATOM_INFO.get(&(atom.atomic_number as i32))
    .unwrap_or(&DEFAULT_ATOM_INFO);

  let atom_color = if atom.is_selected() {
    to_selected_color(&atom_info.color)
  } else { 
    atom_info.color
  };
  
  let roughness = if atom.is_selected() { 0.15 } else { 0.25 };
  let metallic = 0.0;
  
  (atom_color, roughness, metallic)
}

/// Shared helper to determine if a bond should be rendered (both atoms not culled)
fn should_render_bond(bond: &Bond, atomic_structure: &AtomicStructure, atomic_viz_prefs: &AtomicStructureVisualizationPreferences) -> bool {
  let atom1 = atomic_structure.atoms.get(&bond.atom_id1);
  let atom2 = atomic_structure.atoms.get(&bond.atom_id2);
  
  if let (Some(atom1), Some(atom2)) = (atom1, atom2) {
    !should_cull_atom(atom1, atomic_viz_prefs) && !should_cull_atom(atom2, atomic_viz_prefs)
  } else {
    false
  }
}

/// Shared helper to get bond color based on selection state
fn get_bond_color(bond: &Bond) -> Vec3 {
  let base_color = Vec3::new(0.8, 0.8, 0.8);
  if bond.selected {
    to_selected_color(&base_color)
  } else {
    base_color
  }
}

/// Shared helper to get the effective display state for an atom
/// Gets display state from the atomic structure's decorator
/// (Scene markers were deprecated and always returned false)
fn get_atom_display_state(atom_id: u32, atomic_structure: &AtomicStructure) -> AtomDisplayState {
  atomic_structure.decorator.get_atom_display_state(atom_id)
}

/// Maximum number of bonds per atom (reasonable upper bound)
const MAX_OCCLUDERS: usize = 32;

/// Pre-allocated occluder sphere array to avoid allocations
struct OccluderArray {
    spheres: [OccluderSphere; MAX_OCCLUDERS],
    count: usize,
}

impl OccluderArray {
    fn new() -> Self {
        Self {
            spheres: [OccluderSphere { center: Vec3::ZERO, radius: 0.0 }; MAX_OCCLUDERS],
            count: 0,
        }
    }
    
    fn clear(&mut self) {
        self.count = 0;
    }
    
    fn push(&mut self, sphere: OccluderSphere) {
        debug_assert!(self.count < MAX_OCCLUDERS, "Too many occluder spheres");
        if self.count < MAX_OCCLUDERS {
            self.spheres[self.count] = sphere;
            self.count += 1;
        }
    }
    
    fn as_slice(&self) -> &[OccluderSphere] {
        &self.spheres[..self.count]
    }
}

// Calculate occluder spheres for space-filling visualization
fn calculate_occluder_spheres(atom: &Atom, atomic_structure: &AtomicStructure, visualization: &AtomicStructureVisualization, occluder_array: &mut OccluderArray) {
  occluder_array.clear();
  
  // Only calculate occlusion for space-filling mode
  if *visualization != AtomicStructureVisualization::SpaceFilling {
    return;
  }
  
  // Use atom's direct bond_ids vector for O(1) neighbor access
  for &bond_id in &atom.bond_ids {
    if let Some(bond) = atomic_structure.bonds.get(&bond_id) {
      // Find the neighbor atom (the other atom in this bond)
      let neighbor_atom_id = if bond.atom_id1 == atom.id {
        bond.atom_id2
      } else {
        bond.atom_id1
      };
      
      if let Some(neighbor) = atomic_structure.atoms.get(&neighbor_atom_id) {
        let neighbor_radius = get_displayed_atom_radius(neighbor, visualization);
        
        occluder_array.push(OccluderSphere {
          center: neighbor.position.as_vec3(),
          radius: neighbor_radius as f32,
        });
      }
    }
  }
}

pub fn tessellate_atom(output_mesh: &mut Mesh, _model: &AtomicStructure, atom: &Atom, params: &AtomicTessellatorParams, display_state: AtomDisplayState, visualization: &AtomicStructureVisualization, reusable_occludable_mesh: &mut tessellator::OccludableMesh, reusable_occluder_array: &mut OccluderArray) {
  let atom_info = ATOM_INFO.get(&(atom.atomic_number as i32))
    .unwrap_or(&DEFAULT_ATOM_INFO);

  //if atom.atomic_number == 1 {
  //  return; // Temporarily test without Hydrogen
  //}

  // Use shared helper for color and material calculation
  let (atom_color, roughness, metallic) = get_atom_color_and_material(atom);
  
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
  calculate_occluder_spheres(atom, _model, visualization, reusable_occluder_array);
  
  // Render the atom sphere with occlusion culling if in space-filling mode
  if *visualization == AtomicStructureVisualization::SpaceFilling && reusable_occluder_array.count > 0 {
    tessellator::tessellate_sphere_with_occlusion(
      output_mesh,
      reusable_occludable_mesh,
      &atom.position,
      get_displayed_atom_radius(atom, visualization),
      horizontal_divisions,
      vertical_divisions,
      &Material::new(
        &atom_color, 
        roughness,
        metallic),
      reusable_occluder_array.as_slice(),
    );
  } else {
    // Use regular tessellation for ball-and-stick or when no occlusion
    tessellator::tessellate_sphere(
      output_mesh,
      &atom.position,
      get_displayed_atom_radius(atom, visualization),
      horizontal_divisions,
      vertical_divisions,
      &Material::new(
        &atom_color, 
        roughness,
        metallic),
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
        output_mesh,
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

pub fn tessellate_bond(output_mesh: &mut Mesh, model: &AtomicStructure, bond: &Bond, params: &AtomicTessellatorParams) {
  let atom_pos1 = model.get_atom(bond.atom_id1).unwrap().position;
  let atom_pos2 = model.get_atom(bond.atom_id2).unwrap().position;

  let selected = bond.selected;

  // Use shared helper for bond color calculation
  let color = get_bond_color(bond);

  tessellator::tessellate_cylinder(
    output_mesh,
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

// ============================================================================
// IMPOSTOR TESSELLATION METHODS
// ============================================================================

/// Main entry point for impostor-based atomic structure tessellation
pub fn tessellate_atomic_structure_impostors(
  atom_impostor_mesh: &mut AtomImpostorMesh, 
  bond_impostor_mesh: &mut BondImpostorMesh, 
  atomic_structure: &AtomicStructure, 
  atomic_viz_prefs: &AtomicStructureVisualizationPreferences
) {
  let _timer = Timer::new("Atomic impostor tessellation");

  // Pre-allocate impostor mesh capacity (much smaller than triangle tessellation)
  let total_atoms = atomic_structure.atoms.len();
  let total_bonds = atomic_structure.bonds.len();
  
  // Each atom = 4 vertices + 6 indices, each bond = 4 vertices + 6 indices
  atom_impostor_mesh.vertices.reserve(total_atoms * 4);
  atom_impostor_mesh.indices.reserve(total_atoms * 6);
  bond_impostor_mesh.vertices.reserve(total_bonds * 4);
  bond_impostor_mesh.indices.reserve(total_bonds * 6);

  let mut culled_count = 0;
  let mut tessellated_count = 0;
  
  // Tessellate atoms
  for (id, atom) in atomic_structure.atoms.iter() {
    // Get effective display state from decorator
    let display_state = get_atom_display_state(*id, atomic_structure);
    
    // Apply depth culling if enabled
    if should_cull_atom(atom, atomic_viz_prefs) {
      culled_count += 1;
      continue;
    }
    
    tessellated_count += 1;
    tessellate_atom_impostor(atom_impostor_mesh, atom, display_state, &atomic_viz_prefs.visualization);
  }
  
  // Only tessellate bonds for ball-and-stick visualization
  if atomic_viz_prefs.visualization == AtomicStructureVisualization::BallAndStick {
    for (_id, bond) in atomic_structure.bonds.iter() {
      if should_render_bond(bond, atomic_structure, atomic_viz_prefs) {
        tessellate_bond_impostor(bond_impostor_mesh, atomic_structure, bond);
      }
    }
  }

  println!("Atomic impostor tessellation: {:?} visualization, {} atoms tessellated, {} atoms culled", 
           atomic_viz_prefs.visualization, tessellated_count, culled_count);
}

/// Tessellate a single atom as an impostor (4 vertices, 6 indices)
pub fn tessellate_atom_impostor(
  atom_impostor_mesh: &mut AtomImpostorMesh,
  atom: &Atom, 
  display_state: AtomDisplayState,
  visualization: &AtomicStructureVisualization
) {
  let radius = get_displayed_atom_radius(atom, visualization) as f32;
  let (color, roughness, metallic) = get_atom_color_and_material(atom);
  
  // Add the atom quad to the impostor mesh
  atom_impostor_mesh.add_atom_quad(
    &atom.position.as_vec3(),
    radius,
    &color.to_array(),
    roughness,
    metallic
  );
  
  // TODO: Handle markers for marked atoms (AtomDisplayState::Marked, AtomDisplayState::SecondaryMarked)
  // For now, we'll skip marker rendering in impostors - can be added later if needed
  match display_state {
    AtomDisplayState::Marked | AtomDisplayState::SecondaryMarked => {
      // Marker rendering for impostors could be implemented later
      // This would require additional impostor quads or a separate marker system
    },
    AtomDisplayState::Normal => {
      // No marker for normal atoms
    }
  }
}

/// Tessellate a single bond as an impostor (4 vertices, 6 indices)
pub fn tessellate_bond_impostor(
  bond_impostor_mesh: &mut BondImpostorMesh,
  atomic_structure: &AtomicStructure,
  bond: &Bond
) {
  let atom_pos1 = atomic_structure.get_atom(bond.atom_id1).unwrap().position;
  let atom_pos2 = atomic_structure.get_atom(bond.atom_id2).unwrap().position;
  
  let color = get_bond_color(bond);
  
  // Add the bond quad to the impostor mesh
  bond_impostor_mesh.add_bond_quad(
    &atom_pos1.as_vec3(),
    &atom_pos2.as_vec3(),
    BAS_STICK_RADIUS as f32,
    &color.to_array()
  );
}

