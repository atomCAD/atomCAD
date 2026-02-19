use super::atom_edit_data::get_selected_atom_edit_data_mut;
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec3;

/// Add an atom at the ray-plane intersection point.
///
/// The plane passes through the closest atom to the ray (or at a default distance).
pub fn add_atom_by_ray(
    structure_designer: &mut StructureDesigner,
    atomic_number: i16,
    plane_normal: &DVec3,
    ray_start: &DVec3,
    ray_dir: &DVec3,
) {
    // Phase 1: Calculate position (immutable borrow)
    let position = {
        let atomic_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(structure) => structure,
            None => return,
        };

        let closest_atom_position = atomic_structure.find_closest_atom_to_ray(ray_start, ray_dir);
        let default_distance = 5.0;
        let plane_distance = match closest_atom_position {
            Some(atom_pos) => plane_normal.dot(atom_pos),
            None => plane_normal.dot(*ray_start) + default_distance,
        };

        let denominator = plane_normal.dot(*ray_dir);
        if denominator.abs() < 1e-6 {
            return;
        }

        let t = (plane_distance - plane_normal.dot(*ray_start)) / denominator;
        if t < 0.0 {
            return;
        }

        *ray_start + *ray_dir * t
    };

    // Phase 2: Add atom to diff
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.add_atom_to_diff(atomic_number, position);
}
