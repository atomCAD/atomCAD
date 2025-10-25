use glam::i32::IVec3;
use glam::f32::Vec3;
use glam::DQuat;
use glam::DVec3;
use crate::util::hit_test_utils::get_closest_point_on_first_ray;
use crate::util::hit_test_utils::sphere_hit_test;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use std::collections::HashSet;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

/// Precomputed geometry values for half space operations
/// This struct contains commonly used calculated values to avoid code duplication
/// between tessellation and hit testing functions
pub struct HalfSpaceGeometry {
    /// The center position in world space
    pub center_pos: DVec3,
    /// The normalized plane normal direction in world space
    pub plane_normal: DVec3,
    /// The shifted center position (center_pos + shift applied)
    pub shifted_center: DVec3,
    /// The handle position (shifted_center + accessibility offset along normal)
    pub handle_position: DVec3,
}

pub const CENTER_SPHERE_RADIUS: f64 = 0.5;
pub const CENTER_SPHERE_HORIZONTAL_DIVISIONS: u32 = 16;
pub const CENTER_SPHERE_VERTICAL_DIVISIONS: u32 = 16;
// Constants for shift drag handle
pub const SHIFT_HANDLE_ACCESSIBILITY_OFFSET: f64 = 3.0;
pub const SHIFT_HANDLE_AXIS_RADIUS: f64 = 0.2;
pub const SHIFT_HANDLE_CYLINDER_RADIUS: f64 = 0.5;
pub const SHIFT_HANDLE_CYLINDER_LENGTH: f64 = 2.0;
pub const SHIFT_HANDLE_DIVISIONS: u32 = 16;

// Constants for miller index disc visualization
pub const MILLER_INDEX_DISC_DISTANCE: f64 = 5.0; // Distance from center to place discs
pub const MILLER_INDEX_DISC_RADIUS: f64 = 0.5;   // Radius of each disc
pub const MILLER_INDEX_DISC_THICKNESS: f64 = 0.06; // Thickness of each disc
pub const MILLER_INDEX_DISC_DIVISIONS: u32 = 16;  // Number of divisions for disc cylinder

/// Visualization type for the half space
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HalfSpaceVisualization {
    /// Visualize as a plane (square)
    Plane,
    /// Visualize as a cuboid
    Cuboid,
}

/// Calculate common geometry values for half space operations
/// This function precomputes frequently used values to avoid duplication
/// between tessellation and hit testing functions
pub fn calculate_half_space_geometry(
    unit_cell: &UnitCellStruct,
    center: &IVec3,
    miller_index: &IVec3,
    shift: f64,
) -> HalfSpaceGeometry {
    let center_pos = unit_cell.ivec3_lattice_to_real(center);
    
    // Get crystallographically correct plane properties (normal and d-spacing)
    let plane_props = unit_cell.ivec3_miller_index_to_plane_props(miller_index);
    
    // Calculate shift distance as multiples of d-spacing
    let shift_distance = shift * plane_props.d_spacing;
    let shifted_center = center_pos + plane_props.normal * shift_distance;
    let handle_position = shifted_center + plane_props.normal * SHIFT_HANDLE_ACCESSIBILITY_OFFSET;

    HalfSpaceGeometry {
        center_pos,
        plane_normal: plane_props.normal,
        shifted_center,
        handle_position,
    }
}

// Calculate the continuous shift value of a half space based on its centerm miller index and
// a mouse ray. The handle offset is the distance from the plane center to the handle.
// Useful dragging a half plane shift handle.
pub fn get_dragged_shift(unit_cell: &UnitCellStruct, miller_index: &IVec3, center: &IVec3, ray_origin: &DVec3, ray_direction: &DVec3, handle_offset: f64) -> f64 {
    let center_pos = unit_cell.ivec3_lattice_to_real(center);
    
    // Get crystallographically correct plane properties (normal and d-spacing)
    let plane_props = unit_cell.ivec3_miller_index_to_plane_props(miller_index);

    // Find where on the 'normal ray' the mouse ray is closest (in real space)
    let distance_along_normal = get_closest_point_on_first_ray(
        &center_pos,
        &plane_props.normal,
        &ray_origin,
        &ray_direction
    );

    let real_space_distance = distance_along_normal - handle_offset;

    // Convert the real space distance to shift units (multiples of d-spacing)
    return real_space_distance / plane_props.d_spacing;
}

pub fn tessellate_center_sphere(output_mesh: &mut Mesh, center_pos: &DVec3) {
    tessellator::tessellate_sphere(
        output_mesh,
        center_pos,
        CENTER_SPHERE_RADIUS,
        CENTER_SPHERE_HORIZONTAL_DIVISIONS, // number sections when dividing by horizontal lines
        CENTER_SPHERE_VERTICAL_DIVISIONS, // number of sections when dividing by vertical lines
        &Material::new(&Vec3::new(0.95, 0.0, 0.0), 0.3, 0.0));
}

pub fn tessellate_shift_drag_handle(
    output_mesh: &mut Mesh,
    center: &IVec3,
    miller_index: &IVec3,
    dragged_shift: f64,
    unit_cell: &UnitCellStruct) {
    let geometry = calculate_half_space_geometry(unit_cell, center, miller_index, dragged_shift);

    // Define materials
    let axis_material = Material::new(&Vec3::new(0.7, 0.7, 0.7), 1.0, 0.0); // Neutral gray
    let handle_material = Material::new(&Vec3::new(0.2, 0.6, 0.9), 0.5, 0.0); // Blue for handle

    // Tessellate the axis cylinder (thin connection from center to handle)
    tessellator::tessellate_cylinder(
        output_mesh,
        &geometry.handle_position,
        &geometry.center_pos,
        SHIFT_HANDLE_AXIS_RADIUS,
        SHIFT_HANDLE_DIVISIONS,
        &axis_material,
        false, // No caps needed
        None,
        None
    );

    // Tessellate the handle cylinder (thicker, draggable part)
    // Place handle centered at the offset position with length along normal direction
    let handle_start = geometry.handle_position - geometry.plane_normal * (SHIFT_HANDLE_CYLINDER_LENGTH / 2.0);
    let handle_end = geometry.handle_position + geometry.plane_normal * (SHIFT_HANDLE_CYLINDER_LENGTH / 2.0);

    tessellator::tessellate_cylinder(
        output_mesh,
        &handle_end,
        &handle_start,
        SHIFT_HANDLE_CYLINDER_RADIUS,
        SHIFT_HANDLE_DIVISIONS,
        &handle_material,
        true, // Include caps for the handle
        None,
        None
    );
}

pub fn tessellate_plane_grid(
    output_mesh: &mut Mesh,
    center: &IVec3,
    miller_index: &IVec3,
    shift: i32,
    unit_cell: &UnitCellStruct,
) {
    let geometry = calculate_half_space_geometry(unit_cell, center, miller_index, shift as f64);
    let plane_rotator = DQuat::from_rotation_arc(DVec3::Y, geometry.plane_normal);

    let roughness: f32 = 1.0;
    let metallic: f32 = 0.0;
    let outside_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);
    let inside_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);
    let side_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);      

    let thickness = 0.05;

    // A grid representing the plane
    tessellator::tessellate_grid(
        output_mesh,
        &geometry.shifted_center,
        &plane_rotator,
        thickness,
        40.0,
        40.0,
        0.05,
        1.0,
        &outside_material,
        &inside_material,
        &side_material);    
}

/// Tessellates discs representing each possible miller index
/// These discs are positioned at a fixed distance from the center in the direction of each miller index
/// The current miller index disc is highlighted with a yellowish-orange color
pub fn tessellate_miller_indices_discs(
    output_mesh: &mut Mesh,
    center_pos: &DVec3,
    miller_index: &IVec3,
    possible_miller_indices: &HashSet<IVec3>,
    max_miller_index: i32,
    unit_cell: &UnitCellStruct,
) {
    // Material for regular discs - blue color
    let disc_material = Material::new(&Vec3::new(0.0, 0.3, 0.9), 0.3, 0.0);
        
    // Material for the current miller index disc - yellowish orange color
    let current_disc_material = Material::new(&Vec3::new(1.0, 0.6, 0.0), 0.3, 0.0);
        
    // Create a red material for the inside/bottom face of regular discs
    let red_material = Material::new(&Vec3::new(0.95, 0.0, 0.0), 0.3, 0.0);

    // Get the simplified version of the current miller index for comparison
    let simplified_current_miller = simplify_miller_index(*miller_index);

    // Iterate through all possible miller indices
    for miller_index in possible_miller_indices {
        // Get the crystallographically correct plane normal for this miller index
        let direction = unit_cell.ivec3_miller_index_to_normal(&miller_index);

        // Calculate the position for the disc
        let disc_center = *center_pos + direction * MILLER_INDEX_DISC_DISTANCE;
            
        // Calculate start and end points for the disc (thin cylinder)
        let disc_start = disc_center - direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
        let disc_end = disc_center + direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
            
        // Get the dynamic disc radius based on the max miller index
        let disc_radius = get_miller_index_disc_radius(max_miller_index);
            
        // Check if this is the current miller index (compare simplified forms)
        let is_current = *miller_index == simplified_current_miller;
            
        // Choose material based on whether this is the current miller index
        let material = if is_current {
            &current_disc_material
        } else {
            &disc_material
        };

        // Tessellate the disc
        tessellator::tessellate_cylinder(
            output_mesh,
            &disc_start,
            &disc_end,
            disc_radius,
            MILLER_INDEX_DISC_DIVISIONS,
            material,
            true, // Cap the ends
            // If current disc, use the same orange material for top face
            // Otherwise use red material for inside/bottom face
            if is_current { Some(material) } else { Some(&red_material) },
            None,
        );
    }
}

pub fn simplify_miller_index(miller_index: IVec3) -> IVec3 {
    // Get absolute values for checking divisibility
    let abs_x = miller_index.x.abs();
    let abs_y = miller_index.y.abs();
    let abs_z = miller_index.z.abs();

    // Set max_divisor to the maximum of the absolute values of the components
    // This is an optimization as we don't need to check divisors larger than the largest component
    let max_divisor = abs_x.max(abs_y).max(abs_z);
    for divisor in (2..=max_divisor).rev() {
        // Check if all components are divisible by the divisor
        if abs_x % divisor == 0 && abs_y % divisor == 0 && abs_z % divisor == 0 {
            return IVec3::new(
                miller_index.x / divisor,
                miller_index.y / divisor,
                miller_index.z / divisor,
            );
        }
    }

    // If no common divisor found, return the original miller index
    miller_index
}

pub fn get_miller_index_disc_radius(max_miller_index: i32) -> f64 {
    let divisor = f64::max(max_miller_index as f64 - 1.0, 1.0);
    MILLER_INDEX_DISC_RADIUS / divisor
}

/// Hit test the central sphere handle at the given center position
/// Returns Some(t) if the ray hits the sphere, None otherwise
pub fn hit_test_center_sphere(
    unit_cell: &UnitCellStruct,
    center: &IVec3,
    ray_origin: &DVec3,
    ray_direction: &DVec3
) -> Option<f64> {
    // We only need center_pos for this function, so calculate it directly
    // rather than using the full geometry struct
    let center_pos = unit_cell.ivec3_lattice_to_real(center);
    
    // Test central sphere
    sphere_hit_test(
        &center_pos,
        CENTER_SPHERE_RADIUS,
        ray_origin,
        ray_direction
    )
}

/// Hit test the shift handle cylinder at the given center, miller index and shift
/// Returns Some(t) if the ray hits the cylinder, None otherwise
pub fn hit_test_shift_handle(
    unit_cell: &UnitCellStruct,
    center: &IVec3,
    miller_index: &IVec3,
    shift: f64,
    ray_origin: &DVec3,
    ray_direction: &DVec3
) -> Option<f64> {
    let geometry = calculate_half_space_geometry(unit_cell, center, miller_index, shift);
    
    // Calculate handle cylinder start and end points
    let handle_start = geometry.handle_position - geometry.plane_normal * (SHIFT_HANDLE_CYLINDER_LENGTH / 2.0);
    let handle_end = geometry.handle_position + geometry.plane_normal * (SHIFT_HANDLE_CYLINDER_LENGTH / 2.0);
    
    // Test shift handle cylinder
    cylinder_hit_test(
        &handle_end,
        &handle_start,
        SHIFT_HANDLE_CYLINDER_RADIUS,
        ray_origin,
        ray_direction
    )
}

/// Tests if any miller index disc is hit by the given ray
/// Returns the miller index of the hit disc (closest to ray origin), or None if no disc was hit
pub fn hit_test_miller_indices_discs(
    unit_cell: &UnitCellStruct,
    center_pos: &DVec3,
    possible_miller_indices: &HashSet<IVec3>,
    max_miller_index: i32,
    ray_origin: DVec3,
    ray_direction: DVec3) -> Option<IVec3> {
    //let _timer = Timer::new("hit_test_miller_indices_discs");

    let mut closest_hit: Option<(f64, IVec3)> = None;
        
    // Get the disc radius based on max miller index
    let disc_radius = get_miller_index_disc_radius(max_miller_index);
        
    // Iterate through all possible miller indices
    for miller_index in possible_miller_indices {
        // Get the crystallographically correct plane normal for this miller index
        let direction = unit_cell.ivec3_miller_index_to_normal(&miller_index);
            
        // Calculate the position for the disc
        let disc_center = *center_pos + direction * MILLER_INDEX_DISC_DISTANCE;
            
        // Calculate start and end points for the disc (thin cylinder)
        let disc_start = disc_center - direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
        let disc_end = disc_center + direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
            
        // Test if the ray hits this disc
        if let Some(t) = cylinder_hit_test(
            &disc_end,
            &disc_start,
            disc_radius,
            &ray_origin,
            &ray_direction
        ) {
            // If this is the closest hit so far, record it
            match closest_hit {
                None => closest_hit = Some((t, *miller_index)),
                Some((closest_t, _)) if t < closest_t => closest_hit = Some((t, *miller_index)),
                _ => {}
            }
        }
    }
    // Return just the miller index of the closest hit disc, if any
    closest_hit.map(|(_, miller_index)| miller_index)
}

pub fn generate_possible_miller_indices(max_miller_index: i32) -> HashSet<IVec3> {
    let mut possible_miller_indices: HashSet<IVec3> = HashSet::new();
    
    // Iterate through all combinations within the max_miller_index range
    for h in -max_miller_index..=max_miller_index {
        for k in -max_miller_index..=max_miller_index {
            for l in -max_miller_index..=max_miller_index {
                // Skip the origin (0,0,0) as it's not a valid direction
                if h == 0 && k == 0 && l == 0 {
                    continue;
                }
                
                // Create the miller index and reduce it to simplest form
                let miller = IVec3::new(h, k, l);
                let simplified = simplify_miller_index(miller);
                
                // Add the simplified miller index to the set
                possible_miller_indices.insert(simplified);
            }
        }
    }
    
    // Return the set of possible miller indices
    possible_miller_indices
}