use crate::common::scene::Scene;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::common::surface_point_cloud::SurfacePointCloud2D;
use crate::renderer::tessellator::tessellator::Tessellatable;
use std::collections::HashMap;
use std::any::Any;
use crate::common::poly_mesh::PolyMesh;
use crate::structure_designer::geo_tree::GeoNode;

// StructureDesignerScene is a struct that holds the scene to be rendered in the structure designer.
pub struct StructureDesignerScene {
    pub geo_trees: Vec<GeoNode>,
    pub atomic_structures: Vec<AtomicStructure>,
    pub surface_point_clouds: Vec<SurfacePointCloud>,
    pub surface_point_cloud_2ds: Vec<SurfacePointCloud2D>,
    pub poly_meshes: Vec<PolyMesh>,

    pub tessellatable: Option<Box<dyn Tessellatable>>,

    pub node_errors: HashMap<u64, String>,
    pub selected_node_eval_cache: Option<Box<dyn Any>>,
}

impl StructureDesignerScene {
    pub fn new() -> Self {
        Self {
            geo_trees: Vec::new(),
            atomic_structures: Vec::new(),
            surface_point_clouds: Vec::new(),
            surface_point_cloud_2ds: Vec::new(),
            poly_meshes: Vec::new(),
            tessellatable: None,
            node_errors: HashMap::new(),
            selected_node_eval_cache: None,
        }
    }

    pub fn merge(&mut self, other: StructureDesignerScene) {
        self.geo_trees.extend(other.geo_trees);
        self.atomic_structures.extend(other.atomic_structures);
        self.surface_point_clouds.extend(other.surface_point_clouds);
        self.surface_point_cloud_2ds.extend(other.surface_point_cloud_2ds);
        self.poly_meshes.extend(other.poly_meshes);
        self.node_errors.extend(other.node_errors);
        
        // Take the eval cache from other if we don't have one
        if self.selected_node_eval_cache.is_none() && other.selected_node_eval_cache.is_some() {
            self.selected_node_eval_cache = other.selected_node_eval_cache;
        }
        
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

    fn is_atom_marked(&self, _atom_id: u64) -> bool {
        false // Default implementation: no atom is marked by default
    }

    fn is_atom_secondary_marked(&self, _atom_id: u64) -> bool {
        false // Default implementation: no atom is secondary marked by default
    }

    fn surface_point_clouds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud> + '_> {
        Box::new(self.surface_point_clouds.iter())
    }

    fn surface_point_cloud_2ds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud2D> + '_> {
        Box::new(self.surface_point_cloud_2ds.iter())
    }

    fn poly_meshes(&self) -> Box<dyn Iterator<Item = &PolyMesh> + '_> {
        Box::new(self.poly_meshes.iter())
    }

    fn tessellatable(&self) -> Option<Box<&dyn Tessellatable>> {
        self.tessellatable.as_deref().map(Box::new)
    }
}
