use crate::common::atomic_structure::AtomicStructure;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::scene::Scene;
use crate::common::xyz_loader::load_xyz;
use crate::common::xyz_loader::XyzError;
use glam::f64::DVec3;
use crate::common::atomic_structure_utils::{auto_create_bonds, detect_bonded_substructures};
use crate::api::api_types::SelectModifier;

pub struct SceneComposer {
    pub model: AtomicStructure,
}

impl SceneComposer {
  pub fn new() -> Self {
    Self {
      model: AtomicStructure::new(),
    }
  }

  pub fn import_xyz(&mut self, file_path: &str) -> Result<(), XyzError> {
    self.model = load_xyz(&file_path)?;
    auto_create_bonds(&mut self.model);
    detect_bonded_substructures(&mut self.model);
    Ok(())
  }

  // Returns the cluster id of the cluster that was selected or deselected, or None if no cluster was hit
  pub fn select_cluster_by_ray(&mut self, ray_start: &DVec3, ray_dir: &DVec3, select_modifier: SelectModifier) -> Option<u64> {
    let selected_atom_id = self.model.hit_test(ray_start, ray_dir)?; 
    let atom = self.model.get_atom(selected_atom_id)?;
    let cluster_id = atom.cluster_id;
    self.model.select_cluster(atom.cluster_id, select_modifier);
    Some(cluster_id)
  }

  pub fn select_cluster_by_id(&mut self, cluster_id: u64, select_modifier: SelectModifier) {
    self.model.select_cluster(cluster_id, select_modifier);
  }
}

impl<'a> Scene<'a> for SceneComposer {
  fn atomic_structures(&self) -> Box<dyn Iterator<Item = &AtomicStructure> + '_> {
    Box::new(std::iter::once(&self.model))
  }

  fn surface_point_clouds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud> + '_> {
      Box::new(std::iter::empty())
  }

  fn tessellatable(&self) -> Option<&dyn Tessellatable> {
      None
  }
}
