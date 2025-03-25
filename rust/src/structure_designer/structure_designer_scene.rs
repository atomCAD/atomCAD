use crate::common::scene::Scene;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::renderer::tessellator::tessellator::Tessellatable;

pub struct StructureDesignerScene {
    pub atomic_structures: Vec<AtomicStructure>,
    pub surface_point_clouds: Vec<SurfacePointCloud>,

    pub tessellatable: Option<Box<dyn Tessellatable>>,
}

impl StructureDesignerScene {
    pub fn new() -> Self {
        Self {
            atomic_structures: Vec::new(),
            surface_point_clouds: Vec::new(),
            tessellatable: None,
        }
    }

    pub fn merge(&mut self, other: StructureDesignerScene) {
        self.atomic_structures.extend(other.atomic_structures);
        self.surface_point_clouds.extend(other.surface_point_clouds);
        
        match other.tessellatable {
            None => {}, // Do nothing if empty
            Some(other_tessellatable) => {
                self.tessellatable = Some(other_tessellatable)
            },
        }
    }
}

impl<'a> Scene<'a> for StructureDesignerScene {
    fn atomic_structures(&self) -> Box<dyn Iterator<Item = &AtomicStructure> + '_> {
        Box::new(self.atomic_structures.iter())
    }

    fn surface_point_clouds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud> + '_> {
        Box::new(self.surface_point_clouds.iter())
    }

    fn tessellatable(&self) -> Option<Box<&dyn Tessellatable>> {
        self.tessellatable.as_deref().map(Box::new)
    }
}
