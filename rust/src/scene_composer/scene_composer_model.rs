
use glam::f64::DVec3;
use glam::f64::DQuat;
use crate::common::atomic_structure::AtomicStructure;
use crate::scene_composer::cluster_frame_gadget::ClusterFrameGadget;
use crate::api::scene_composer_api_types::SelectModifier;
use std::collections::HashSet;
use crate::common::atomic_structure::Cluster;
use crate::util::transform::Transform;
use crate::api::scene_composer_api_types::APISceneComposerTool;

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

pub struct SceneComposerModel {
    pub model: AtomicStructure,
    pub selected_frame_gadget: Option<Box<ClusterFrameGadget>>,
    pub active_tool: SceneComposerTool,
}

impl SceneComposerModel {
    pub fn new() -> Self {
        Self {
          model: AtomicStructure::new(),
          selected_frame_gadget: None,
          active_tool: SceneComposerTool::Default,
        }
    }
 
    pub fn reset(&mut self) {
        self.active_tool = SceneComposerTool::Default;
        self.recreate_selected_frame_gadget();
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

    pub fn get_selected_cluster_ids(&self) -> Vec<u64> {
        self.model.clusters
          .iter()
          .filter(|(_, cluster)| cluster.selected)
          .map(|(id, _)| *id)
          .collect()
    }

    pub fn get_selected_frame_transform(&self) -> Option<Transform> {
        // Return a cloned transform if a frame gadget is selected
        self.selected_frame_gadget.as_ref().map(|gadget| gadget.transform.clone())
    }

    pub fn set_selected_frame_transform(&mut self, transform: Transform) {
        if let Some(gadget) = self.selected_frame_gadget.as_mut() {
            gadget.transform = transform;
            self.sync_gadget_to_model();
        }
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

}
