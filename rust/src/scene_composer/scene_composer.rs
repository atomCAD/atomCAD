use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure::Cluster;
use crate::common::gadget::Gadget;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::scene::Scene;
use crate::common::xyz_loader::load_xyz;
use crate::common::xyz_loader::XyzError;
use crate::common::xyz_saver::save_xyz;
use crate::common::xyz_saver::XyzSaveError;
use glam::f64::DVec3;
use glam::f64::DQuat;
use crate::util::transform::Transform;
use crate::common::atomic_structure_utils::{auto_create_bonds, detect_bonded_substructures};
use crate::api::api_types::SelectModifier;
use crate::scene_composer::cluster_frame_gadget::ClusterFrameGadget;
use crate::api::api_types::APISceneComposerTool;
use std::collections::HashSet;
use crate::scene_composer::commands::scene_composer_command::SceneComposerCommand;

pub enum SceneComposerTool {
  Default,
  Align(AlignToolState),
  AtomInfo(AtomInfoToolState),
  Distance(DistanceToolState),
}

pub struct AlignToolState {
  // IDs of atoms used as reference to align the frame
  // can contain 0 to 3 elements (3 elements when all reference atoms are chosen)
  pub reference_atom_ids: Vec<u64>,
}

pub struct AtomInfoToolState {
  pub atom_id: Option<u64>,
}

pub struct DistanceToolState {
  pub atom_ids: Vec<u64>,
}

pub struct SceneComposer {
    pub model: AtomicStructure,
    pub selected_frame_gadget: Option<Box<ClusterFrameGadget>>,
    pub active_tool: SceneComposerTool,
    pub history: Vec<Box<dyn SceneComposerCommand>>,
    pub next_history_index: usize, // Next index (the one that was last executed plus one) in the history vector.
}

impl SceneComposer {
  pub fn new() -> Self {
    Self {
      model: AtomicStructure::new(),
      selected_frame_gadget: None,
      active_tool: SceneComposerTool::Default,
      history: Vec::new(),
      next_history_index: 0,
    }
  }

  pub fn new_model(&mut self) {
    self.model = AtomicStructure::new();
    self.selected_frame_gadget = None;
    self.active_tool = SceneComposerTool::Default;
  }

  pub fn import_xyz(&mut self, file_path: &str) -> Result<(), XyzError> {
    let mut model = load_xyz(&file_path)?;
    auto_create_bonds(&mut model);
    detect_bonded_substructures(&mut model);
    if self.model.get_num_of_atoms() > 0 {
      self.model.add_atomic_structure(&model);
    } else {
      self.model = model;
    }
    Ok(())
  }

  pub fn export_xyz(&self, file_path: &str) -> Result<(), XyzSaveError> {
    save_xyz(&self.model, file_path)
  }

  pub fn set_selected_frame_transform(&mut self, transform: Transform) {
    if let Some(gadget) = self.selected_frame_gadget.as_mut() {
        gadget.transform = transform;
        self.sync_gadget_to_model();
    }
  }

  pub fn get_selected_frame_transform(&self) -> Option<Transform> {
    // Return a cloned transform if a frame gadget is selected
    self.selected_frame_gadget.as_ref().map(|gadget| gadget.transform.clone())
  }


  
  pub fn translate_along_local_axis(&mut self, axis_index: u32, translation: f64) {
    if let Some(gadget) = self.selected_frame_gadget.as_mut() {
      let dir = match axis_index {
        0 => DVec3::new(1.0, 0.0, 0.0),
        1 => DVec3::new(0.0, 1.0, 0.0),
        2 => DVec3::new(0.0, 0.0, 1.0),
        _ => DVec3::new(0.0, 0.0, 0.0)
      };

      gadget.transform.translation += gadget.transform.rotation.mul_vec3(dir) * translation;
      self.sync_gadget_to_model();
    }
  }

  pub fn rotate_around_local_axis(&mut self, axis_index: u32, angle_degrees: f64) {
    if let Some(gadget) = self.selected_frame_gadget.as_mut() {
      // Create a rotation axis based on the axis_index
      let axis = match axis_index {
        0 => DVec3::new(1.0, 0.0, 0.0), // X-axis
        1 => DVec3::new(0.0, 1.0, 0.0), // Y-axis
        2 => DVec3::new(0.0, 0.0, 1.0), // Z-axis
        _ => DVec3::new(0.0, 0.0, 0.0)  // Invalid axis defaults to no rotation
      };
      
      // Convert degrees to radians
      let angle_radians = angle_degrees.to_radians();
      
      // Create rotation quaternion in local space
      let local_axis = gadget.transform.rotation.mul_vec3(axis);
      let rotation = DQuat::from_axis_angle(local_axis, angle_radians);
      
      // Apply the rotation to the current rotation
      gadget.transform.rotation = rotation * gadget.transform.rotation;
      self.sync_gadget_to_model();
    }
  }

  pub fn is_frame_locked_to_atoms(&self) -> bool {
    // Return the frame_locked_to_atoms value if a frame gadget is selected, otherwise return false
    self.selected_frame_gadget.as_ref().map_or(false, |gadget| gadget.frame_locked_to_atoms)
  }

  pub fn set_frame_locked_to_atoms(&mut self, locked: bool) {
    if let Some(gadget) = self.selected_frame_gadget.as_mut() {
      gadget.frame_locked_to_atoms = locked;
      self.sync_gadget_to_model();
    }
  }

  // Returns the cluster id of the cluster that was selected or deselected, or None if no cluster was hit
  pub fn select_cluster_by_ray(&mut self, ray_start: &DVec3, ray_dir: &DVec3, select_modifier: SelectModifier) -> Option<u64> {
    let selected_atom_id = self.model.hit_test(ray_start, ray_dir)?; 
    let atom = self.model.get_atom(selected_atom_id)?;
    let cluster_id = atom.cluster_id;
    self.select_cluster_by_id(atom.cluster_id, select_modifier);
    Some(cluster_id)
  }

  pub fn select_cluster_by_id(&mut self, cluster_id: u64, select_modifier: SelectModifier) -> HashSet<u64> {
    let inverted_cluster_ids = self.model.select_cluster(cluster_id, select_modifier);
    self.recreate_selected_frame_gadget();
    inverted_cluster_ids
  }

  pub fn invert_cluster_selections(&mut self, inverted_cluster_ids: &HashSet<u64>) {
    self.model.invert_cluster_selections(inverted_cluster_ids);
    self.recreate_selected_frame_gadget();
  } 

  pub fn set_active_tool(&mut self, tool: APISceneComposerTool) {
    self.active_tool = match tool {
      APISceneComposerTool::Default => SceneComposerTool::Default,
      APISceneComposerTool::Align => SceneComposerTool::Align(AlignToolState {
        reference_atom_ids: Vec::new(),
      }),
      APISceneComposerTool::AtomInfo => SceneComposerTool::AtomInfo(AtomInfoToolState {
        atom_id: None,
      }),
      APISceneComposerTool::Distance => SceneComposerTool::Distance(DistanceToolState {
        atom_ids: Vec::new(),
      }),
    };
  }

  pub fn get_active_tool(&self) -> APISceneComposerTool {
    match &self.active_tool {
      SceneComposerTool::Default => APISceneComposerTool::Default,
      SceneComposerTool::Align(_) => APISceneComposerTool::Align,
      SceneComposerTool::AtomInfo(_) => APISceneComposerTool::AtomInfo,
      SceneComposerTool::Distance(_) => APISceneComposerTool::Distance,
    }
  }

  pub fn get_available_tools(&self) -> Vec<APISceneComposerTool> {
    let mut available_tools = vec![APISceneComposerTool::Default];

    if !self.get_selected_cluster_ids().is_empty() {
      available_tools.push(APISceneComposerTool::Align);
    }

    available_tools.push(APISceneComposerTool::AtomInfo);
    available_tools.push(APISceneComposerTool::Distance);

    available_tools
  }

  pub fn select_align_atom_by_id(&mut self, atom_id: u64) -> bool {
    match &mut self.active_tool {
      SceneComposerTool::Align(align_state) => {
        // Check if the atom exists in the model
        if self.model.get_atom(atom_id).is_none() {
          return false;
        }

        // If we already have 3 reference atoms and are adding a fourth,
        // clear the list and add this as the first atom of a new selection
        if align_state.reference_atom_ids.len() >= 3 {
          align_state.reference_atom_ids.clear();
        }

        // Add the atom ID to our reference list
        align_state.reference_atom_ids.push(atom_id);
        
        // Try to align the frame to the reference atoms
        self.align_frame_to_atoms();
        
        true
      },
      _ => false, // Not in align tool mode
    }
  }
  
  // Returns the atom id that was selected for alignment, or None if no atom was hit
  pub fn select_align_atom_by_ray(&mut self, ray_start: &DVec3, ray_dir: &DVec3) -> Option<u64> {
    // Find the atom along the ray
    let selected_atom_id = self.model.hit_test(ray_start, ray_dir)?;
    
    // Try to select this atom for alignment
    if self.select_align_atom_by_id(selected_atom_id) {
      Some(selected_atom_id)
    } else {
      None
    }
  }

  pub fn select_atom_info_atom_by_id(&mut self, atom_id: u64) -> bool {
    match &mut self.active_tool {
      SceneComposerTool::AtomInfo(atom_info_state) => {
        // Check if the atom exists in the model
        if self.model.get_atom(atom_id).is_none() {
          return false;
        }

        atom_info_state.atom_id = Some(atom_id);

        true
      },
      _ => false, // Not in align tool mode
    }
  }

  // Returns the atom id that was selected for alignment, or None if no atom was hit
  pub fn select_atom_info_atom_by_ray(&mut self, ray_start: &DVec3, ray_dir: &DVec3) -> Option<u64> {
    let selected_atom_id = self.model.hit_test(ray_start, ray_dir)?;
    
    // Try to select this atom for alignment
    if self.select_atom_info_atom_by_id(selected_atom_id) {
      Some(selected_atom_id)
    } else {
      None
    }
  }

  pub fn get_atom_info_atom_id(&self) -> Option<u64> {
    // Check if the active tool is the align tool
    match &self.active_tool {
      SceneComposerTool::AtomInfo(atom_info_state) => {
        atom_info_state.atom_id
      },
      _ => None,
    }
  }

  pub fn select_distance_atom_by_id(&mut self, atom_id: u64) -> bool {
    match &mut self.active_tool {
      SceneComposerTool::Distance(distance_state) => {
        // Check if the atom exists in the model
        if self.model.get_atom(atom_id).is_none() {
          return false;
        }

        // If we already have 2 reference atoms and are adding a third,
        // clear the list and add this as the first atom of a new selection
        if distance_state.atom_ids.len() >= 2 {
          distance_state.atom_ids.clear();
        }

        // Add the atom ID to our reference list
        distance_state.atom_ids.push(atom_id);
        
        true
      },
      _ => false, // Not in distance tool mode
    }
  }
  
  // Returns the atom id that was selected for distance, or None if no atom was hit
  pub fn select_distance_atom_by_ray(&mut self, ray_start: &DVec3, ray_dir: &DVec3) -> Option<u64> {
    // Find the atom along the ray
    let selected_atom_id = self.model.hit_test(ray_start, ray_dir)?;
    
    // Try to select this atom for distance
    if self.select_distance_atom_by_id(selected_atom_id) {
      Some(selected_atom_id)
    } else {
      None
    }
  }

  // Aligns the selected frame to the reference atoms based on how many atoms are selected
  fn align_frame_to_atoms(&mut self) {
    if self.selected_frame_gadget.is_none() {
      return;
    }

    let reference_atom_ids = match &self.active_tool {
      SceneComposerTool::Align(align_state) => &align_state.reference_atom_ids,
      _ => return, // Not in align tool mode
    };

    if reference_atom_ids.is_empty() {
      return;
    }
    
    // Get atom positions
    let mut atom_positions: Vec<DVec3> = Vec::new();
    for atom_id in reference_atom_ids {
      if let Some(atom) = self.model.get_atom(*atom_id) {
        atom_positions.push(atom.position);
      } else {
        // Skip invalid atom IDs
        continue;
      }
    }

    if atom_positions.is_empty() {
      return;
    }

    let current_transform = self.selected_frame_gadget.as_ref().unwrap().transform.clone();
    let mut new_transform = current_transform.clone();
    
    // Perform alignment based on how many atoms we have
    if atom_positions.len() >= 1 {
      // First atom: Set position to this atom
      new_transform.translation = atom_positions[0];
    }
    
    if atom_positions.len() >= 2 {
      // Second atom: Orient X axis from first to second atom
      let x_axis = (atom_positions[1] - atom_positions[0]).normalize();
      
      // Find the rotation from the current X axis (1,0,0 in local space) to the desired X axis
      let current_x_axis = new_transform.rotation.mul_vec3(DVec3::new(1.0, 0.0, 0.0));
      
      // Calculate rotation to align current X axis with desired X axis
      let rotation_to_x = DQuat::from_rotation_arc(current_x_axis, x_axis);
      
      // Apply this rotation to the current rotation
      new_transform.rotation = rotation_to_x * new_transform.rotation;
    }
    
    if atom_positions.len() >= 3 {
      // Calculate the current X axis in global coordinates
      let global_x_axis = new_transform.rotation.mul_vec3(DVec3::new(1.0, 0.0, 0.0));
      
      // Vector from atom1 to atom3
      let atom1_to_atom3 = atom_positions[2] - atom_positions[0];
      
      // Project atom1_to_atom3 onto the X axis to get the component along X
      let projection = atom1_to_atom3.dot(global_x_axis) * global_x_axis;

      let perpendicular = atom1_to_atom3 - projection;

      if perpendicular.length_squared() > 0.00001 {
        let new_z_axis = perpendicular.normalize();
        
        // Get the current Z axis in global coordinates
        let global_z_axis = new_transform.rotation.mul_vec3(DVec3::new(0.0, 0.0, 1.0));

        let angle = global_z_axis.angle_between(new_z_axis);
        
        // Determine rotation direction (cross product gives the axis of rotation)
        let cross = global_z_axis.cross(new_z_axis);
        let sign = if cross.dot(global_x_axis) < 0.0 { -1.0 } else { 1.0 };
        
        // Create a rotation quaternion around the X axis
        let x_rotation = DQuat::from_axis_angle(global_x_axis, sign * angle);
        
        // Apply this rotation to align the Z axis with the new_z_axis
        new_transform.rotation = x_rotation * new_transform.rotation;
      }
    }
    
    // Update the frame transform
    self.set_selected_frame_transform(new_transform);
  }
  
  fn get_selected_cluster_ids(&self) -> Vec<u64> {
    self.model.clusters
      .iter()
      .filter(|(_, cluster)| cluster.selected)
      .map(|(id, _)| *id)
      .collect()
  }

  pub fn sync_gadget_to_model(&mut self) {
    let selected_cluster_ids = self.get_selected_cluster_ids();

    if let Some(gadget) = &mut self.selected_frame_gadget {
      
      // Calculate the delta transform from last_synced_transform to the current transform
      let delta_transform = gadget.transform.delta_from(&gadget.last_synced_transform);
      
      // Transform all atoms in all selected clusters if frame is locked to atoms
      if gadget.frame_locked_to_atoms {
        for cluster_id in &selected_cluster_ids {
          if let Some(cluster) = self.model.clusters.get(&cluster_id) {
            // Get all atom IDs in the cluster
            let atom_ids: Vec<u64> = cluster.atom_ids.iter().copied().collect();
            
            // Apply the delta transform to each atom in the cluster using transform_atom
            for atom_id in atom_ids {
              self.model.transform_atom(atom_id, &delta_transform.rotation, &delta_transform.translation);
            }
          }
        }
      }

      // Update the frame transform for single selection
      if selected_cluster_ids.len() == 1 {
        let cluster_id = selected_cluster_ids[0];
        if let Some(cluster) = self.model.clusters.get_mut(&cluster_id) {
          cluster.frame_transform = gadget.transform.clone();
          cluster.frame_locked_to_atoms = gadget.frame_locked_to_atoms;
        }
      }

      // Update the last synced transform
      gadget.last_synced_transform = gadget.transform.clone();
    }
  }

  pub fn get_history_size(&self) -> usize {
    self.history.len()
  }

  pub fn execute_command(&mut self, mut command: Box<dyn SceneComposerCommand>) -> & Box<dyn SceneComposerCommand> {
    if self.history.len() > self.next_history_index {
      self.history.drain(self.next_history_index..);
    }
    command.execute(&mut self.model, false);
    self.history.push(command);
    self.next_history_index = self.history.len();

    & self.history[self.history.len() - 1]
  }

  pub fn undo(&mut self) -> bool {
    if self.next_history_index == 0 {
      return false;
    }
    self.next_history_index -= 1;
    self.history[self.next_history_index].undo(self);
    return true;
  }

  pub fn redo(&mut self) -> bool {
    if self.next_history_index >= self.history.len() {
      return false;
    }
    self.history[self.next_history_index].execute(self, true);
    return true;
  }

  fn recreate_selected_frame_gadget(&mut self) {
    let selected_cluster_ids = self.get_selected_cluster_ids();
    
    if selected_cluster_ids.is_empty() {
        self.selected_frame_gadget = None;
        return;
    }
    
    // Collect selected clusters by their IDs
    let selected_clusters: Vec<&Cluster> = selected_cluster_ids
        .iter()
        .filter_map(|id| self.model.clusters.get(id))
        .collect();
    
    if selected_clusters.len() == 1 {
        self.selected_frame_gadget = Some(Box::new(ClusterFrameGadget {
          transform: selected_clusters[0].frame_transform.clone(),
          start_drag_transform: None,
          last_synced_transform: selected_clusters[0].frame_transform.clone(),
          frame_locked_to_atoms: selected_clusters[0].frame_locked_to_atoms,
          drag_start_rotation: DQuat::IDENTITY,
          dragging_offset: 0.0
        }));
        return;
    }
    
    // If multiple clusters are selected, calculate average transform
    let avg_translation = selected_clusters.iter()
        .map(|cluster| cluster.frame_transform.translation)
        .fold(DVec3::ZERO, |acc, t| acc + t) / selected_clusters.len() as f64;
    
    // Average rotations (this is an approximation - we're simply normalizing the sum)
    // For a more mathematically correct solution, a more complex averaging algorithm might be needed
    let mut avg_rotation = selected_clusters.iter()
        .map(|cluster| cluster.frame_transform.rotation)
        .fold(DQuat::IDENTITY, |acc, r| acc + r);
    if avg_rotation.length_squared() > 0.0 {
        avg_rotation = avg_rotation.normalize();
    } else {
        avg_rotation = DQuat::IDENTITY;
    }

    self.selected_frame_gadget = Some(Box::new(ClusterFrameGadget {
      transform: Transform::new(avg_translation, avg_rotation),
      start_drag_transform: None,
      last_synced_transform: Transform::new(avg_translation, avg_rotation),
      frame_locked_to_atoms: true,
      drag_start_rotation: DQuat::IDENTITY,
      dragging_offset: 0.0
    }));
  }

  // -------------------------------------------------------------------------------------------------------------------------
  // --- Gadget delegation methods                                                                                        ---
  // -------------------------------------------------------------------------------------------------------------------------

  pub fn gadget_hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
    if let Some(gadget) = &self.selected_frame_gadget {
      return gadget.hit_test(ray_origin, ray_direction);
    }
    None
  }

  pub fn gadget_start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    if let Some(gadget) = &mut self.selected_frame_gadget {
      gadget.start_drag(handle_index, ray_origin, ray_direction);
    }
  }

  pub fn gadget_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    if let Some(gadget) = &mut self.selected_frame_gadget {
      gadget.drag(handle_index, ray_origin, ray_direction);
    }
  }

  pub fn gadget_end_drag(&mut self) {
    if let Some(gadget) = &mut self.selected_frame_gadget {
      gadget.end_drag();
    }
  }

  pub fn get_align_tool_state_text(&self) -> String {
    // Check if the active tool is the align tool
    match &self.active_tool {
      SceneComposerTool::Align(align_state) => {
        let reference_atom_ids = &align_state.reference_atom_ids;
        
        // If no atoms are selected
        if reference_atom_ids.is_empty() {
          return String::from("Please select atom 1!");
        }
        
        // Start building the output string
        let mut result = String::new();
        
        // Add information for each selected atom
        for (i, atom_id) in reference_atom_ids.iter().enumerate() {
          if let Some(atom) = self.model.get_atom(*atom_id) {
            result.push_str(&format!(
              "Atom {}: id: {}\nX: {:.6} Y: {:.6} Z: {:.6}\n",
              i + 1,
              atom_id,
              atom.position.x,
              atom.position.y,
              atom.position.z
            ));
          }
        }
        
        // Add appropriate prompt based on how many atoms are selected
        if reference_atom_ids.len() == 1 {
          result.push_str("Please select atom 2!");
        } else if reference_atom_ids.len() == 2 {
          result.push_str("Please select atom 3!");
        }
        
        result
      },
      _ => String::new(), // Not in align tool mode, return empty string
    }
  }

pub fn get_distance_tool_state_text(&self) -> String {
  // Check if the active tool is the distance tool
  match &self.active_tool {
    SceneComposerTool::Distance(distance_state) => {
      
      // If no atoms are selected
      if distance_state.atom_ids.is_empty() {
        return String::from("Please select atom 1!");
      }
      
      // Start building the output string
      let mut result = String::new();
      
      // Add information for each selected atom
      for (i, atom_id) in distance_state.atom_ids.iter().enumerate() {
        if let Some(atom) = self.model.get_atom(*atom_id) {
          result.push_str(&format!(
            "Atom {}: id: {}\nX: {:.6} Y: {:.6} Z: {:.6}\n",
            i + 1,
            atom_id,
            atom.position.x,
            atom.position.y,
            atom.position.z
          ));
        }
      }
      
      // Add appropriate prompt based on how many atoms are selected
      if distance_state.atom_ids.len() == 1 {
        result.push_str("Please select atom 2!");
      } else {
        let atom1 = self.model.get_atom(distance_state.atom_ids[0]).unwrap();
        let atom2 = self.model.get_atom(distance_state.atom_ids[1]).unwrap();
        let distance = atom1.position.distance(atom2.position);
        result.push_str(&format!("Distance: {:.6}", distance));
      }

      result
    },
    _ => String::new(), // Not in distance tool mode, return empty string
  }
}
}


impl<'a> Scene<'a> for SceneComposer {
  fn atomic_structures(&self) -> Box<dyn Iterator<Item = &AtomicStructure> + '_> {
    Box::new(std::iter::once(&self.model))
  }

  fn is_atom_marked(&self, atom_id: u64) -> bool {
    match &self.active_tool {
      SceneComposerTool::Align(align_state) => {
        // For Align tool, check if the atom is in the reference_atom_ids list
        align_state.reference_atom_ids.contains(&atom_id)
      },
      SceneComposerTool::AtomInfo(atom_info_state) => {
        // For AtomInfo tool, check if the atom is the one selected
        atom_info_state.atom_id == Some(atom_id)
      },
      SceneComposerTool::Distance(distance_state) => {
        // For Distance tool, check if the atom is in the atom_ids list
        distance_state.atom_ids.contains(&atom_id)
      },
      // Default tool doesn't mark any atoms
      SceneComposerTool::Default => false,
    }
  }

  fn surface_point_clouds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud> + '_> {
      Box::new(std::iter::empty())
  }

  fn tessellatable(&self) -> Option<Box<&dyn Tessellatable>> {
      // Use as_deref to get a reference to the inner ClusterFrameGadget
      let frame_gadget_ref = self.selected_frame_gadget.as_deref()?;
      
      // Create a box containing a reference to the frame gadget as a Tessellatable trait object
      Some(Box::new(frame_gadget_ref as &dyn Tessellatable))
  }
}
