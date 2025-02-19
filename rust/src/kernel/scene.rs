use super::atomic_structure::AtomicStructure;
use super::surface_point_cloud::SurfacePointCloud;

pub struct Scene {
    pub atomic_structures: Vec<AtomicStructure>,
    pub surface_point_clouds: Vec<SurfacePointCloud>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            atomic_structures: Vec::new(),
            surface_point_clouds: Vec::new(),
        }
    }
}
