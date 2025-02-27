use super::atomic_structure::AtomicStructure;
use super::surface_point_cloud::SurfacePointCloud;
use super::gadget_state::HalfSpaceGadgetState;

pub struct Scene {
    pub atomic_structures: Vec<AtomicStructure>,
    pub surface_point_clouds: Vec<SurfacePointCloud>,

    pub half_space_gadget_state: Option<HalfSpaceGadgetState>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            atomic_structures: Vec::new(),
            surface_point_clouds: Vec::new(),
            half_space_gadget_state: None,
        }
    }

    pub fn merge(&mut self, other: Scene) {
        self.atomic_structures.extend(other.atomic_structures);
        self.surface_point_clouds.extend(other.surface_point_clouds);
        
        if let Some(gadget_state) = other.half_space_gadget_state {
            self.half_space_gadget_state = Some(gadget_state);
        }
    }
}
