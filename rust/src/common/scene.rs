use super::atomic_structure::AtomicStructure;
use super::surface_point_cloud::SurfacePointCloud;
use crate::renderer::tessellator::tessellator::Tessellatable;

pub struct Scene {
    pub atomic_structures: Vec<AtomicStructure>,
    pub surface_point_clouds: Vec<SurfacePointCloud>,

    pub tessellatable: Option<Box<dyn Tessellatable>>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            atomic_structures: Vec::new(),
            surface_point_clouds: Vec::new(),
            tessellatable: None,
        }
    }

    pub fn merge(&mut self, other: Scene) {
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
