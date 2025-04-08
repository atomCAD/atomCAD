use super::atomic_structure::AtomicStructure;
use super::surface_point_cloud::SurfacePointCloud;
use crate::renderer::tessellator::tessellator::Tessellatable;

pub trait Scene<'a> {
    fn atomic_structures(&self) -> Box<dyn Iterator<Item = &AtomicStructure> + '_>;
    fn is_atom_marked(&self, atom_id: u64) -> bool;
    fn surface_point_clouds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud> + '_>;
    fn tessellatable(&self) -> Option<Box<&dyn Tessellatable>>;
}
