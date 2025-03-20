use crate::common::atomic_structure::AtomicStructure;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::scene::Scene;
use crate::common::xyz_loader::load_xyz;
use crate::common::xyz_loader::XyzError;
use crate::common::atomic_structure_utils::auto_create_bonds;

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
    Ok(())
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
