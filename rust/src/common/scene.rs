use super::atomic_structure::AtomicStructure;
use super::surface_point_cloud::SurfacePointCloud;
use crate::structure_editor::gadgets::gadget::Gadget;

pub struct Scene {
    pub atomic_structures: Vec<AtomicStructure>,
    pub surface_point_clouds: Vec<SurfacePointCloud>,

    pub gadget: Option<Box<dyn Gadget>>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            atomic_structures: Vec::new(),
            surface_point_clouds: Vec::new(),
            gadget: None,
        }
    }

    pub fn merge(&mut self, other: Scene) {
        self.atomic_structures.extend(other.atomic_structures);
        self.surface_point_clouds.extend(other.surface_point_clouds);
        
        match other.gadget {
            None => {}, // Do nothing if empty
            Some(other_gadget) => {
                self.gadget = Some(other_gadget)
            },
        }
    }
}
