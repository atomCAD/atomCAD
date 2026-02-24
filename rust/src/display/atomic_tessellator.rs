use crate::crystolecule::atomic_constants::{ATOM_INFO, DEFAULT_ATOM_INFO};
use crate::crystolecule::atomic_structure::{
    Atom, AtomDisplayState, AtomicStructure, BondReference,
};
use crate::renderer::tessellator::tessellator::{self, OccluderSphere};
// Scene trait removed - is_atom_marked was deprecated and always returned false
use crate::display::preferences::{
    AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
};
use crate::renderer::atom_impostor_mesh::AtomImpostorMesh;
use crate::renderer::bond_impostor_mesh::BondImpostorMesh;
use crate::renderer::mesh::{Material, Mesh};
use crate::util::timer::Timer;
use glam::f32::Vec3;
use glam::f64::DVec3;

pub struct AtomicTessellatorParams {
    pub ball_and_stick_sphere_horizontal_divisions: u32, // Ball-and-stick sphere horizontal divisions
    pub ball_and_stick_sphere_vertical_divisions: u32,   // Ball-and-stick sphere vertical divisions
    pub space_filling_sphere_horizontal_divisions: u32, // Space-filling sphere horizontal divisions
    pub space_filling_sphere_vertical_divisions: u32,   // Space-filling sphere vertical divisions
    pub cylinder_divisions: u32,
}

// atom radius factor for the 'balls and sticks' view
const BAS_ATOM_RADIUS_FACTOR: f64 = 0.5;

// radius of a bond cylinder (stick) in the 'balls and sticks' view
pub const BAS_STICK_RADIUS: f64 = 0.1;

// radius of each cylinder in a multi-bond (double/triple/quadruple)
const MULTI_BOND_CYLINDER_RADIUS: f64 = 0.06;
// offset distance from bond axis center to each parallel cylinder center
const MULTI_BOND_OFFSET: f64 = 0.10;

// color for primary markers (bright yellow)
const MARKER_COLOR: Vec3 = Vec3::new(1.0, 1.0, 0.0);
// color for secondary markers (blue)
const SECONDARY_MARKER_COLOR: Vec3 = Vec3::new(0.0, 0.5, 1.0);

// color for atom delete markers in diff structures (red)
const DELETE_MARKER_COLOR: Vec3 = Vec3::new(0.9, 0.1, 0.1);
// fixed radius for atom delete markers (Angstroms)
const DELETE_MARKER_RADIUS: f64 = 0.5;
// roughness for atom delete markers
const DELETE_MARKER_ROUGHNESS: f32 = 0.5;
// color for anchor arrows in diff structures (orange)
const ANCHOR_ARROW_COLOR: Vec3 = Vec3::new(1.0, 0.6, 0.0);
// radius for anchor point spheres (Angstroms)
const ANCHOR_SPHERE_RADIUS: f64 = 0.3;
// radius for anchor arrow cylinders (Angstroms)
const ANCHOR_ARROW_RADIUS: f64 = 0.05;

/// Helper function to determine if an atom should be culled based on depth
fn should_cull_atom(
    atom: &Atom,
    atomic_viz_prefs: &AtomicStructureVisualizationPreferences,
) -> bool {
    match atomic_viz_prefs.visualization {
        AtomicStructureVisualization::BallAndStick => {
            if let Some(cull_depth) = atomic_viz_prefs.ball_and_stick_cull_depth {
                atom.in_crystal_depth > cull_depth as f32
            } else {
                false
            }
        }
        AtomicStructureVisualization::SpaceFilling => {
            if let Some(cull_depth) = atomic_viz_prefs.space_filling_cull_depth {
                atom.in_crystal_depth > cull_depth as f32
            } else {
                false
            }
        }
    }
}

pub fn tessellate_atomic_structure(
    output_mesh: &mut Mesh,
    atomic_structure: &AtomicStructure,
    params: &AtomicTessellatorParams,
    atomic_viz_prefs: &AtomicStructureVisualizationPreferences,
) {
    let _timer = Timer::new("Atomic tessellation");

    // Pre-allocate mesh capacity for worst-case scenario (no compression)
    let total_atoms = atomic_structure.get_num_of_atoms();
    let (h_div, v_div) = match atomic_viz_prefs.visualization {
        AtomicStructureVisualization::BallAndStick => (
            params.ball_and_stick_sphere_horizontal_divisions,
            params.ball_and_stick_sphere_vertical_divisions,
        ),
        AtomicStructureVisualization::SpaceFilling => (
            params.space_filling_sphere_horizontal_divisions,
            params.space_filling_sphere_vertical_divisions,
        ),
    };

    // Worst-case vertices per sphere: (h_div * (v_div - 1)) + 2 (poles)
    let vertices_per_sphere = (h_div * (v_div - 1)) + 2;
    // Worst-case triangles per sphere: h_div * 2 + (h_div * (v_div - 2) * 2)
    let triangles_per_sphere = h_div * 2 + (h_div * (v_div - 2) * 2);
    let indices_per_sphere = triangles_per_sphere * 3;

    // Reserve capacity for worst case (all atoms tessellated, no compression)
    let estimated_vertices = total_atoms * vertices_per_sphere as usize;
    let estimated_indices = total_atoms * indices_per_sphere as usize;

    output_mesh.vertices.reserve(estimated_vertices);
    output_mesh.indices.reserve(estimated_indices);

    // Create reusable data structures for all sphere tessellations
    let mut reusable_occludable_mesh = tessellator::OccludableMesh::new();
    let mut reusable_occluder_array = OccluderArray::new();

    let mut culled_count = 0;
    let mut tessellated_count = 0;

    for (id, atom) in atomic_structure.iter_atoms() {
        // Get effective display state (considering scene markers)
        let display_state = get_atom_display_state(*id, atomic_structure);

        // Apply depth culling if enabled
        if should_cull_atom(atom, atomic_viz_prefs) {
            // Skip tessellating this atom - it's too deep inside and can't be seen
            culled_count += 1;
            continue;
        }

        tessellated_count += 1;
        tessellate_atom(
            output_mesh,
            atomic_structure,
            atom,
            params,
            display_state,
            &atomic_viz_prefs.visualization,
            &mut reusable_occludable_mesh,
            &mut reusable_occluder_array,
        );
    }

    // Only tessellate bonds for ball-and-stick visualization
    if atomic_viz_prefs.visualization == AtomicStructureVisualization::BallAndStick {
        // Iterate inline bonds - each bond only once using atom ID ordering
        for atom in atomic_structure.atoms_values() {
            // Skip bonds if this atom is culled
            if should_cull_atom(atom, atomic_viz_prefs) {
                continue;
            }

            for bond in &atom.bonds {
                let other_atom_id = bond.other_atom_id();
                // Only tessellate each bond once
                if atom.id < other_atom_id {
                    if let Some(other_atom) = atomic_structure.get_atom(other_atom_id) {
                        // Skip bond if the other atom is culled
                        if should_cull_atom(other_atom, atomic_viz_prefs) {
                            continue;
                        }

                        // Bond delete markers in diff structures render as red
                        if bond.is_delete_marker() && atomic_structure.is_diff() {
                            tessellate_bond_delete_marker(
                                output_mesh,
                                atomic_structure,
                                atom,
                                other_atom,
                                params,
                            );
                        } else {
                            tessellate_bond_inline(
                                output_mesh,
                                atomic_structure,
                                atom,
                                other_atom,
                                bond.bond_order(),
                                params,
                            );
                        }
                    }
                }
            }
        }
    }

    // Render anchor arrows for diff structures when enabled
    if atomic_structure.is_diff() && atomic_structure.decorator().show_anchor_arrows {
        for (&atom_id, &anchor_pos) in atomic_structure.anchor_positions() {
            if let Some(atom) = atomic_structure.get_atom(atom_id) {
                // Small red sphere at anchor position
                tessellator::tessellate_sphere(
                    output_mesh,
                    &anchor_pos,
                    ANCHOR_SPHERE_RADIUS,
                    params.ball_and_stick_sphere_horizontal_divisions,
                    params.ball_and_stick_sphere_vertical_divisions,
                    &Material::new(&DELETE_MARKER_COLOR, DELETE_MARKER_ROUGHNESS, 0.0),
                );

                // Orange cylinder from anchor to atom's current position
                tessellator::tessellate_cylinder(
                    output_mesh,
                    &anchor_pos,
                    &atom.position,
                    ANCHOR_ARROW_RADIUS,
                    params.cylinder_divisions,
                    &Material::new(&ANCHOR_ARROW_COLOR, 0.5, 0.0),
                    false,
                    None,
                    None,
                );
            }
        }
    }

    println!(
        "Atomic tessellation: {:?} visualization, {} atoms tessellated, {} atoms culled",
        atomic_viz_prefs.visualization, tessellated_count, culled_count
    );
}

pub fn get_displayed_atom_radius(atom: &Atom, visualization: &AtomicStructureVisualization) -> f64 {
    // Delete markers use a fixed radius
    if atom.is_delete_marker() {
        return DELETE_MARKER_RADIUS;
    }

    let atom_info = ATOM_INFO
        .get(&(atom.atomic_number as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO);

    match visualization {
        AtomicStructureVisualization::BallAndStick => {
            atom_info.covalent_radius * BAS_ATOM_RADIUS_FACTOR
        }
        AtomicStructureVisualization::SpaceFilling => atom_info.van_der_waals_radius,
    }
}

/// Shared helper to get atom color and material properties based on selection state
fn get_atom_color_and_material(atom: &Atom) -> (Vec3, f32, f32) {
    // Delete markers render as red spheres
    if atom.is_delete_marker() {
        let color = if atom.is_selected() {
            to_selected_color(&DELETE_MARKER_COLOR)
        } else {
            DELETE_MARKER_COLOR
        };
        let roughness = if atom.is_selected() {
            0.15
        } else {
            DELETE_MARKER_ROUGHNESS
        };
        return (color, roughness, 0.0);
    }

    let atom_info = ATOM_INFO
        .get(&(atom.atomic_number as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO);

    let atom_color = if atom.is_selected() {
        to_selected_color(&atom_info.color)
    } else {
        atom_info.color
    };

    let roughness = if atom.is_selected() { 0.15 } else { 0.25 };
    let metallic = 0.0;

    (atom_color, roughness, metallic)
}

/// Get bond color based on bond type and selection state from decorator.
/// Aromatic=amber, Dative=teal, Metallic=steel-blue; regular bonds are grey.
/// Selection overrides all.
fn get_bond_color_inline(
    atom_id1: u32,
    atom_id2: u32,
    bond_order: u8,
    atomic_structure: &AtomicStructure,
) -> Vec3 {
    use crate::crystolecule::atomic_structure::inline_bond::{
        BOND_AROMATIC, BOND_DATIVE, BOND_METALLIC,
    };
    let base_color = match bond_order {
        BOND_AROMATIC => Vec3::new(0.9, 0.7, 0.1),  // Amber for aromatic bonds
        BOND_DATIVE => Vec3::new(0.2, 0.8, 0.7),    // Teal for dative bonds
        BOND_METALLIC => Vec3::new(0.8, 0.5, 0.2),  // Copper/bronze for metallic bonds
        _ => Vec3::new(0.8, 0.8, 0.8),              // Grey for regular bonds
    };
    let bond_ref = BondReference { atom_id1, atom_id2 };
    if atomic_structure.decorator().is_bond_selected(&bond_ref) {
        to_selected_color(&base_color)
    } else {
        base_color
    }
}

/// Shared helper to get the effective display state for an atom
/// Gets display state from the atomic structure's decorator
/// (Scene markers were deprecated and always returned false)
fn get_atom_display_state(atom_id: u32, atomic_structure: &AtomicStructure) -> AtomDisplayState {
    atomic_structure.decorator().get_atom_display_state(atom_id)
}

/// Maximum number of bonds per atom (reasonable upper bound)
const MAX_OCCLUDERS: usize = 32;

/// Pre-allocated occluder sphere array to avoid allocations
pub(crate) struct OccluderArray {
    spheres: [OccluderSphere; MAX_OCCLUDERS],
    count: usize,
}

impl OccluderArray {
    fn new() -> Self {
        Self {
            spheres: [OccluderSphere {
                center: Vec3::ZERO,
                radius: 0.0,
            }; MAX_OCCLUDERS],
            count: 0,
        }
    }

    fn clear(&mut self) {
        self.count = 0;
    }

    fn push(&mut self, sphere: OccluderSphere) {
        debug_assert!(self.count < MAX_OCCLUDERS, "Too many occluder spheres");
        if self.count < MAX_OCCLUDERS {
            self.spheres[self.count] = sphere;
            self.count += 1;
        }
    }

    fn as_slice(&self) -> &[OccluderSphere] {
        &self.spheres[..self.count]
    }
}

// Calculate occluder spheres for space-filling visualization
fn calculate_occluder_spheres(
    atom: &Atom,
    atomic_structure: &AtomicStructure,
    visualization: &AtomicStructureVisualization,
    occluder_array: &mut OccluderArray,
) {
    occluder_array.clear();

    // Only calculate occlusion for space-filling mode
    if *visualization != AtomicStructureVisualization::SpaceFilling {
        return;
    }

    // Use atom's inline bonds for neighbor access
    for bond in &atom.bonds {
        let neighbor_atom_id = bond.other_atom_id();

        if let Some(neighbor) = atomic_structure.get_atom(neighbor_atom_id) {
            let neighbor_radius = get_displayed_atom_radius(neighbor, visualization);

            occluder_array.push(OccluderSphere {
                center: neighbor.position.as_vec3(),
                radius: neighbor_radius as f32,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_atom(
    output_mesh: &mut Mesh,
    _model: &AtomicStructure,
    atom: &Atom,
    params: &AtomicTessellatorParams,
    display_state: AtomDisplayState,
    visualization: &AtomicStructureVisualization,
    reusable_occludable_mesh: &mut tessellator::OccludableMesh,
    reusable_occluder_array: &mut OccluderArray,
) {
    let _atom_info = ATOM_INFO
        .get(&(atom.atomic_number as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO);

    //if atom.atomic_number == 1 {
    //  return; // Temporarily test without Hydrogen
    //}

    // Use shared helper for color and material calculation
    let (atom_color, roughness, metallic) = get_atom_color_and_material(atom);

    // Get appropriate tessellation parameters based on visualization mode
    let (horizontal_divisions, vertical_divisions) = match visualization {
        AtomicStructureVisualization::BallAndStick => (
            params.ball_and_stick_sphere_horizontal_divisions,
            params.ball_and_stick_sphere_vertical_divisions,
        ),
        AtomicStructureVisualization::SpaceFilling => (
            params.space_filling_sphere_horizontal_divisions,
            params.space_filling_sphere_vertical_divisions,
        ),
    };

    // Calculate occluder spheres for occlusion culling
    calculate_occluder_spheres(atom, _model, visualization, reusable_occluder_array);

    // Render the atom sphere with occlusion culling if in space-filling mode
    if *visualization == AtomicStructureVisualization::SpaceFilling
        && reusable_occluder_array.count > 0
    {
        tessellator::tessellate_sphere_with_occlusion(
            output_mesh,
            reusable_occludable_mesh,
            &atom.position,
            get_displayed_atom_radius(atom, visualization),
            horizontal_divisions,
            vertical_divisions,
            &Material::new(&atom_color, roughness, metallic),
            reusable_occluder_array.as_slice(),
        );
    } else {
        // Use regular tessellation for ball-and-stick or when no occlusion
        tessellator::tessellate_sphere(
            output_mesh,
            &atom.position,
            get_displayed_atom_radius(atom, visualization),
            horizontal_divisions,
            vertical_divisions,
            &Material::new(&atom_color, roughness, metallic),
        );
    }

    // Add a 3D crosshair for marked atoms
    match display_state {
        AtomDisplayState::Marked | AtomDisplayState::SecondaryMarked => {
            // Select color based on display state
            let marker_color = match display_state {
                AtomDisplayState::Marked => MARKER_COLOR, // Yellow for primary marked atoms
                AtomDisplayState::SecondaryMarked => SECONDARY_MARKER_COLOR, // Blue for secondary marked atoms
                _ => unreachable!(), // This branch already ensures we're in one of the two marked states
            };

            // Calculate crosshair dimensions
            let radius = get_displayed_atom_radius(atom, visualization);
            let half_length = radius * 1.5;
            let crosshair_radius = radius * 0.4;

            // Render the crosshair
            tessellator::tessellate_crosshair_3d(
                output_mesh,
                &DVec3::new(atom.position.x, atom.position.y, atom.position.z),
                half_length,
                crosshair_radius,
                params.cylinder_divisions,
                &Material::new(&marker_color, 1.0, 0.0),
                true,
            );
        }
        AtomDisplayState::Normal => {
            // No marker for normal atoms
        }
    }
}

fn to_selected_color(_color: &Vec3) -> Vec3 {
    Vec3::new(1.0, 0.2, 1.0) // Bright magenta for selected atoms
}

/// Compute a unit vector perpendicular to the bond axis between two atoms.
///
/// Uses the position of a neighbor atom to define a reference plane. If no suitable
/// neighbor exists, falls back to the cardinal axis least aligned with the bond direction.
fn compute_bond_perpendicular(
    atom1: &Atom,
    atom2: &Atom,
    atomic_structure: &AtomicStructure,
) -> DVec3 {
    let bond_dir = atom2.position - atom1.position;
    let bond_len = bond_dir.length();
    if bond_len < 1e-12 {
        return DVec3::X;
    }
    let bond_axis = bond_dir / bond_len;

    const COLLINEAR_THRESHOLD: f64 = 1e-4;

    // Try neighbors of atom1 then atom2 to find a non-collinear reference
    for (anchor, exclude_id) in [(atom1, atom2.id), (atom2, atom1.id)] {
        for bond in &anchor.bonds {
            let neighbor_id = bond.other_atom_id();
            if neighbor_id == exclude_id {
                continue;
            }
            if let Some(neighbor) = atomic_structure.get_atom(neighbor_id) {
                let to_neighbor = neighbor.position - anchor.position;
                let cross = bond_axis.cross(to_neighbor);
                let cross_len = cross.length();
                if cross_len > COLLINEAR_THRESHOLD {
                    return cross / cross_len;
                }
            }
        }
    }

    // Fallback: pick the cardinal axis least aligned with the bond
    let abs_x = bond_axis.x.abs();
    let abs_y = bond_axis.y.abs();
    let abs_z = bond_axis.z.abs();
    let fallback = if abs_x <= abs_y && abs_x <= abs_z {
        DVec3::X
    } else if abs_y <= abs_z {
        DVec3::Y
    } else {
        DVec3::Z
    };
    bond_axis.cross(fallback).normalize()
}

/// Layout of cylinders for a given bond order.
struct MultiBondLayout {
    offsets: [(DVec3, f64); 4],
    count: usize,
}

/// Compute the cylinder offsets for multi-bond rendering.
fn compute_multi_bond_layout(bond_order: u8, perp: DVec3, bond_axis: DVec3) -> MultiBondLayout {
    use crate::crystolecule::atomic_structure::inline_bond::*;

    let zero = (DVec3::ZERO, 0.0);

    match bond_order {
        BOND_DOUBLE | BOND_AROMATIC => MultiBondLayout {
            offsets: [
                (perp * MULTI_BOND_OFFSET, MULTI_BOND_CYLINDER_RADIUS),
                (perp * (-MULTI_BOND_OFFSET), MULTI_BOND_CYLINDER_RADIUS),
                zero,
                zero,
            ],
            count: 2,
        },
        BOND_TRIPLE => {
            let perp2 = bond_axis.cross(perp);
            // 120-degree triangular arrangement
            let d0 = perp;
            let d1 = perp * (-0.5) + perp2 * 0.866_025_403_784_438_6;
            let d2 = perp * (-0.5) - perp2 * 0.866_025_403_784_438_6;
            MultiBondLayout {
                offsets: [
                    (d0 * MULTI_BOND_OFFSET, MULTI_BOND_CYLINDER_RADIUS),
                    (d1 * MULTI_BOND_OFFSET, MULTI_BOND_CYLINDER_RADIUS),
                    (d2 * MULTI_BOND_OFFSET, MULTI_BOND_CYLINDER_RADIUS),
                    zero,
                ],
                count: 3,
            }
        }
        BOND_QUADRUPLE => {
            let perp2 = bond_axis.cross(perp);
            // 90-degree square arrangement
            MultiBondLayout {
                offsets: [
                    (perp * MULTI_BOND_OFFSET, MULTI_BOND_CYLINDER_RADIUS),
                    (perp2 * MULTI_BOND_OFFSET, MULTI_BOND_CYLINDER_RADIUS),
                    (perp * (-MULTI_BOND_OFFSET), MULTI_BOND_CYLINDER_RADIUS),
                    (perp2 * (-MULTI_BOND_OFFSET), MULTI_BOND_CYLINDER_RADIUS),
                ],
                count: 4,
            }
        }
        _ => MultiBondLayout {
            offsets: [(DVec3::ZERO, BAS_STICK_RADIUS), zero, zero, zero],
            count: 1,
        },
    }
}

/// Tessellate bond using inline bond data
fn tessellate_bond_inline(
    output_mesh: &mut Mesh,
    atomic_structure: &AtomicStructure,
    atom1: &Atom,
    atom2: &Atom,
    bond_order: u8,
    params: &AtomicTessellatorParams,
) {
    let bond_ref = BondReference {
        atom_id1: atom1.id,
        atom_id2: atom2.id,
    };
    let selected = atomic_structure.decorator().is_bond_selected(&bond_ref);
    let color = get_bond_color_inline(atom1.id, atom2.id, bond_order, atomic_structure);
    let material = Material::new(&color, if selected { 0.2 } else { 0.8 }, 0.0);

    let bond_dir = atom2.position - atom1.position;
    if bond_dir.length() < 1e-12 {
        return;
    }
    let bond_axis = bond_dir.normalize();
    let perp = compute_bond_perpendicular(atom1, atom2, atomic_structure);
    let layout = compute_multi_bond_layout(bond_order, perp, bond_axis);

    for i in 0..layout.count {
        let (offset, radius) = layout.offsets[i];
        tessellator::tessellate_cylinder(
            output_mesh,
            &(atom2.position + offset),
            &(atom1.position + offset),
            radius,
            params.cylinder_divisions,
            &material,
            false,
            None,
            None,
        );
    }
}

/// Tessellate a bond delete marker as a red cylinder (triangle mesh path).
/// Selection (magenta) takes priority over delete marker color (red).
fn tessellate_bond_delete_marker(
    output_mesh: &mut Mesh,
    atomic_structure: &AtomicStructure,
    atom1: &Atom,
    atom2: &Atom,
    params: &AtomicTessellatorParams,
) {
    let bond_ref = BondReference {
        atom_id1: atom1.id,
        atom_id2: atom2.id,
    };
    let selected = atomic_structure.decorator().is_bond_selected(&bond_ref);
    let color = if selected {
        to_selected_color(&DELETE_MARKER_COLOR)
    } else {
        DELETE_MARKER_COLOR
    };
    let roughness = if selected {
        0.2
    } else {
        DELETE_MARKER_ROUGHNESS
    };

    tessellator::tessellate_cylinder(
        output_mesh,
        &atom2.position,
        &atom1.position,
        BAS_STICK_RADIUS,
        params.cylinder_divisions,
        &Material::new(&color, roughness, 0.0),
        false,
        None,
        None,
    );
}

// ============================================================================
// IMPOSTOR TESSELLATION METHODS
// ============================================================================

/// Main entry point for impostor-based atomic structure tessellation
pub fn tessellate_atomic_structure_impostors(
    atom_impostor_mesh: &mut AtomImpostorMesh,
    bond_impostor_mesh: &mut BondImpostorMesh,
    atomic_structure: &AtomicStructure,
    atomic_viz_prefs: &AtomicStructureVisualizationPreferences,
) {
    let _timer = Timer::new("Atomic impostor tessellation");

    // Pre-allocate impostor mesh capacity (much smaller than triangle tessellation)
    let total_atoms = atomic_structure.get_num_of_atoms();
    let total_bonds = atomic_structure.get_num_of_bonds();

    // Each atom = 4 vertices + 6 indices
    // Each bond = up to 4 quads (quadruple bonds), each quad = 4 vertices + 6 indices
    atom_impostor_mesh.vertices.reserve(total_atoms * 4);
    atom_impostor_mesh.indices.reserve(total_atoms * 6);
    bond_impostor_mesh.vertices.reserve(total_bonds * 4 * 4);
    bond_impostor_mesh.indices.reserve(total_bonds * 6 * 4);

    let mut culled_count = 0;
    let mut tessellated_count = 0;

    // Tessellate atoms
    for (id, atom) in atomic_structure.iter_atoms() {
        // Get effective display state from decorator
        let display_state = get_atom_display_state(*id, atomic_structure);

        // Apply depth culling if enabled
        if should_cull_atom(atom, atomic_viz_prefs) {
            culled_count += 1;
            continue;
        }

        tessellated_count += 1;
        tessellate_atom_impostor(
            atom_impostor_mesh,
            atom,
            display_state,
            &atomic_viz_prefs.visualization,
        );
    }

    // Only tessellate bonds for ball-and-stick visualization
    if atomic_viz_prefs.visualization == AtomicStructureVisualization::BallAndStick {
        // Iterate inline bonds - each bond only once using atom ID ordering
        for atom in atomic_structure.atoms_values() {
            // Skip bonds if this atom is culled
            if should_cull_atom(atom, atomic_viz_prefs) {
                continue;
            }

            for bond in &atom.bonds {
                let other_atom_id = bond.other_atom_id();
                // Only tessellate each bond once
                if atom.id < other_atom_id {
                    if let Some(other_atom) = atomic_structure.get_atom(other_atom_id) {
                        // Skip bond if the other atom is culled
                        if should_cull_atom(other_atom, atomic_viz_prefs) {
                            continue;
                        }

                        // Bond delete markers in diff structures render as red
                        if bond.is_delete_marker() && atomic_structure.is_diff() {
                            tessellate_bond_delete_marker_impostor(
                                bond_impostor_mesh,
                                atomic_structure,
                                atom,
                                other_atom,
                            );
                        } else {
                            tessellate_bond_impostor_inline(
                                bond_impostor_mesh,
                                atomic_structure,
                                atom,
                                other_atom,
                                bond.bond_order(),
                            );
                        }
                    }
                }
            }
        }
    }

    // Render anchor arrows for diff structures when enabled
    if atomic_structure.is_diff() && atomic_structure.decorator().show_anchor_arrows {
        for (&atom_id, &anchor_pos) in atomic_structure.anchor_positions() {
            if let Some(atom) = atomic_structure.get_atom(atom_id) {
                // Small red sphere at anchor position
                atom_impostor_mesh.add_atom_quad(
                    &anchor_pos.as_vec3(),
                    ANCHOR_SPHERE_RADIUS as f32,
                    &DELETE_MARKER_COLOR.to_array(),
                    DELETE_MARKER_ROUGHNESS,
                    0.0,
                );

                // Orange cylinder from anchor to atom's current position
                bond_impostor_mesh.add_bond_quad(
                    &anchor_pos.as_vec3(),
                    &atom.position.as_vec3(),
                    ANCHOR_ARROW_RADIUS as f32,
                    &ANCHOR_ARROW_COLOR.to_array(),
                );
            }
        }
    }
}

/// Tessellate a single atom as an impostor (4 vertices, 6 indices)
pub fn tessellate_atom_impostor(
    atom_impostor_mesh: &mut AtomImpostorMesh,
    atom: &Atom,
    display_state: AtomDisplayState,
    visualization: &AtomicStructureVisualization,
) {
    let radius = get_displayed_atom_radius(atom, visualization) as f32;
    let (color, roughness, metallic) = get_atom_color_and_material(atom);

    // Override color for marked atoms
    let color = match display_state {
        AtomDisplayState::Marked => MARKER_COLOR,
        AtomDisplayState::SecondaryMarked => SECONDARY_MARKER_COLOR,
        AtomDisplayState::Normal => color,
    };

    // Add the atom quad to the impostor mesh
    atom_impostor_mesh.add_atom_quad(
        &atom.position.as_vec3(),
        radius,
        &color.to_array(),
        roughness,
        metallic,
    );
}

/// Tessellate bond impostor using inline bond data
fn tessellate_bond_impostor_inline(
    bond_impostor_mesh: &mut BondImpostorMesh,
    atomic_structure: &AtomicStructure,
    atom1: &Atom,
    atom2: &Atom,
    bond_order: u8,
) {
    let color = get_bond_color_inline(atom1.id, atom2.id, bond_order, atomic_structure);

    let bond_dir = atom2.position - atom1.position;
    if bond_dir.length() < 1e-12 {
        return;
    }
    let bond_axis = bond_dir.normalize();
    let perp = compute_bond_perpendicular(atom1, atom2, atomic_structure);
    let layout = compute_multi_bond_layout(bond_order, perp, bond_axis);

    for i in 0..layout.count {
        let (offset, radius) = layout.offsets[i];
        let start = atom1.position + offset;
        let end = atom2.position + offset;
        bond_impostor_mesh.add_bond_quad(
            &start.as_vec3(),
            &end.as_vec3(),
            radius as f32,
            &color.to_array(),
        );
    }
}

/// Tessellate a bond delete marker as a red cylinder (impostor path).
/// Selection (magenta) takes priority over delete marker color (red).
fn tessellate_bond_delete_marker_impostor(
    bond_impostor_mesh: &mut BondImpostorMesh,
    atomic_structure: &AtomicStructure,
    atom1: &Atom,
    atom2: &Atom,
) {
    let bond_ref = BondReference {
        atom_id1: atom1.id,
        atom_id2: atom2.id,
    };
    let selected = atomic_structure.decorator().is_bond_selected(&bond_ref);
    let color = if selected {
        to_selected_color(&DELETE_MARKER_COLOR)
    } else {
        DELETE_MARKER_COLOR
    };

    bond_impostor_mesh.add_bond_quad(
        &atom1.position.as_vec3(),
        &atom2.position.as_vec3(),
        BAS_STICK_RADIUS as f32,
        &color.to_array(),
    );
}

// ============================================================================
// Guide placement tessellation
// ============================================================================

/// Color for guide dot spheres (selection magenta)
const GUIDE_DOT_COLOR: Vec3 = Vec3::new(1.0, 0.2, 1.0);
/// Radius for primary guide dots (Angstroms)
const GUIDE_DOT_PRIMARY_RADIUS: f64 = 0.20;
/// Radius for secondary guide dots (Angstroms)
const GUIDE_DOT_SECONDARY_RADIUS: f64 = 0.15;

/// Tessellate guide placement visuals (guide dot spheres + anchor-to-dot cylinders)
/// into the triangle mesh. Called after atom/bond tessellation.
pub fn tessellate_guide_placement(
    output_mesh: &mut Mesh,
    visuals: &crate::crystolecule::atomic_structure::atomic_structure_decorator::GuidePlacementVisuals,
    params: &AtomicTessellatorParams,
) {
    use crate::crystolecule::guided_placement::GuideDotType;

    for dot in &visuals.guide_dots {
        let radius = match dot.dot_type {
            GuideDotType::Primary => GUIDE_DOT_PRIMARY_RADIUS,
            GuideDotType::Secondary => GUIDE_DOT_SECONDARY_RADIUS,
        };

        // Guide dot sphere (magenta)
        tessellator::tessellate_sphere(
            output_mesh,
            &dot.position,
            radius,
            params.ball_and_stick_sphere_horizontal_divisions,
            params.ball_and_stick_sphere_vertical_divisions,
            &Material::new(&GUIDE_DOT_COLOR, 0.3, 0.0),
        );

        // Orange cylinder from anchor to guide dot
        tessellator::tessellate_cylinder(
            output_mesh,
            &visuals.anchor_pos,
            &dot.position,
            ANCHOR_ARROW_RADIUS,
            params.cylinder_divisions,
            &Material::new(&ANCHOR_ARROW_COLOR, 0.5, 0.0),
            false,
            None,
            None,
        );
    }
}

/// Tessellate guide placement visuals using impostor rendering.
pub fn tessellate_guide_placement_impostors(
    atom_impostor_mesh: &mut AtomImpostorMesh,
    bond_impostor_mesh: &mut BondImpostorMesh,
    visuals: &crate::crystolecule::atomic_structure::atomic_structure_decorator::GuidePlacementVisuals,
) {
    use crate::crystolecule::guided_placement::GuideDotType;

    for dot in &visuals.guide_dots {
        let radius = match dot.dot_type {
            GuideDotType::Primary => GUIDE_DOT_PRIMARY_RADIUS,
            GuideDotType::Secondary => GUIDE_DOT_SECONDARY_RADIUS,
        };

        // Guide dot sphere (magenta)
        atom_impostor_mesh.add_atom_quad(
            &dot.position.as_vec3(),
            radius as f32,
            &GUIDE_DOT_COLOR.to_array(),
            0.3,
            0.0,
        );

        // Orange cylinder from anchor to guide dot
        bond_impostor_mesh.add_bond_quad(
            &visuals.anchor_pos.as_vec3(),
            &dot.position.as_vec3(),
            ANCHOR_ARROW_RADIUS as f32,
            &ANCHOR_ARROW_COLOR.to_array(),
        );
    }
}
