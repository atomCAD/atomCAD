use super::atomic_structure::AtomicStructure;
use super::surface_point_cloud::SurfacePointCloud;
use super::surface_point_cloud::SurfacePointCloud2D;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::quad_mesh::QuadMesh;
use crate::renderer::tessellator::quad_mesh_tessellator::MeshSmoothing;

pub trait Scene<'a> {
    fn atomic_structures(&self) -> Box<dyn Iterator<Item = &AtomicStructure> + '_>;
    fn is_atom_marked(&self, atom_id: u64) -> bool;
    fn is_atom_secondary_marked(&self, atom_id: u64) -> bool;
    fn surface_point_clouds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud> + '_>;
    fn surface_point_cloud_2ds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud2D> + '_>;
    fn quad_meshes(&self) -> Box<dyn Iterator<Item = &QuadMesh> + '_>;
    fn get_quad_mesh_smoothing(&self) -> MeshSmoothing {
        // Default implementation returns standard smooth normals
        MeshSmoothing::Smooth
    }
    fn tessellatable(&self) -> Option<Box<&dyn Tessellatable>>;
}
