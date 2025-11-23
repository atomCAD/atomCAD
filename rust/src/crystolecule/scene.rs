use super::atomic_structure::AtomicStructure;
use super::surface_point_cloud::SurfacePointCloud;
use super::surface_point_cloud::SurfacePointCloud2D;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::crystolecule::poly_mesh::PolyMesh;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

pub trait Scene<'a> {
    fn atomic_structures(&self) -> Box<dyn Iterator<Item = &AtomicStructure> + '_>;
    fn is_atom_marked(&self, atom_id: u64) -> bool;
    fn is_atom_secondary_marked(&self, atom_id: u64) -> bool;
    fn surface_point_clouds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud> + '_>;
    fn surface_point_cloud_2ds(&self) -> Box<dyn Iterator<Item = &SurfacePointCloud2D> + '_>;
    fn poly_meshes(&self) -> Box<dyn Iterator<Item = &PolyMesh> + '_>;
    fn tessellatable(&self) -> Option<Box<&dyn Tessellatable>>;
    fn get_unit_cell(&self) -> Option<&UnitCellStruct>;
}
