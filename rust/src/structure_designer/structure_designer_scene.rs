use crate::common::scene::Scene;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::common::surface_point_cloud::SurfacePointCloud2D;
use crate::renderer::tessellator::tessellator::Tessellatable;
use std::collections::HashMap;
use crate::common::quad_mesh::QuadMesh;

pub struct StructureDesignerScene {
    pub atomic_structures: Vec<AtomicStructure>,
    pub surface_point_clouds: Vec<SurfacePointCloud>,
    pub surface_point_cloud_2ds: Vec<SurfacePointCloud2D>,
    pub quad_meshes: Vec<QuadMesh>,

    pub tessellatable: Option<Box<dyn Tessellatable>>,

    pub node_errors: HashMap<u64, String>,
}

impl StructureDesignerScene {
    pub fn new() -> Self {
        Self {
            atomic_structures: Vec::new(),
            surface_point_clouds: Vec::new(),
            surface_point_cloud_2ds: Vec::new(),
            quad_meshes: Vec::new(),
            tessellatable: None,
            node_errors: HashMap::new(),
        }
    }

    pub fn merge(&mut self, other: StructureDesignerScene) {
        self.atomic_structures.extend(other.atomic_structures);
        self.surface_point_clouds.extend(other.surface_point_clouds);
        self.surface_point_cloud_2ds.extend(other.surface_point_cloud_2ds);
        self.quad_meshes.extend(other.quad_meshes);
        self.node_errors.extend(other.node_errors);
        
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

    fn is_atom_marked(&self, atom_id: u64) -> bool {
        false // Default implementation: no atom is marked by default
    }

    fn is_atom_secondary_marked(&self, atom_id: u64) -> bool {
        false // Default implementation: no atom is secondary marked by default
    }

    fn surface_point_clouds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud> + '_> {
        Box::new(self.surface_point_clouds.iter())
    }

    fn surface_point_cloud_2ds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud2D> + '_> {
        Box::new(self.surface_point_cloud_2ds.iter())
    }

    fn quad_meshes(&self) -> Box<dyn Iterator<Item = &QuadMesh> + '_> {
        Box::new(self.quad_meshes.iter())
    }

    fn tessellatable(&self) -> Option<Box<&dyn Tessellatable>> {
        self.tessellatable.as_deref().map(Box::new)
    }
}
