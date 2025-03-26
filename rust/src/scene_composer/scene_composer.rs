use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure::Cluster;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::scene::Scene;
use crate::common::xyz_loader::load_xyz;
use crate::common::xyz_loader::XyzError;
use glam::f64::DVec3;
use glam::f64::DQuat;
use crate::util::transform::Transform;
use crate::common::atomic_structure_utils::{auto_create_bonds, detect_bonded_substructures};
use crate::api::api_types::SelectModifier;
use crate::scene_composer::cluster_frame_gadget::ClusterFrameGadget;

pub struct SceneComposer {
    pub model: AtomicStructure,
    pub selected_frame_gadget: Option<Box<ClusterFrameGadget>>,
}

impl SceneComposer {
  pub fn new() -> Self {
    Self {
      model: AtomicStructure::new(),
      selected_frame_gadget: None,
    }
  }

  pub fn import_xyz(&mut self, file_path: &str) -> Result<(), XyzError> {
    self.model = load_xyz(&file_path)?;
    auto_create_bonds(&mut self.model);
    detect_bonded_substructures(&mut self.model);
    Ok(())
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

  pub fn select_cluster_by_id(&mut self, cluster_id: u64, select_modifier: SelectModifier) {
    self.model.select_cluster(cluster_id, select_modifier);
    self.recreate_selected_frame_gadget();
  }

  fn get_selected_cluster_ids(&self) -> Vec<u64> {
    self.model.clusters
      .iter()
      .filter(|(_, cluster)| cluster.selected)
      .map(|(id, _)| *id)
      .collect()
  }

  fn sync_gadget_to_model(&mut self) {
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
          last_synced_transform: selected_clusters[0].frame_transform.clone(),
          frame_locked_to_atoms: selected_clusters[0].frame_locked_to_atoms
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
      last_synced_transform: Transform::new(avg_translation, avg_rotation),
      frame_locked_to_atoms: true
    }));
  }
}

impl<'a> Scene<'a> for SceneComposer {
  fn atomic_structures(&self) -> Box<dyn Iterator<Item = &AtomicStructure> + '_> {
    Box::new(std::iter::once(&self.model))
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
