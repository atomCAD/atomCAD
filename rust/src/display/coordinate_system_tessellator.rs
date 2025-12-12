use glam::f64::DVec3;
use glam::f32::Vec3;

use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::drawing_plane::DrawingPlane;
use crate::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use crate::renderer::line_mesh::LineMesh;
use crate::api::structure_designer::structure_designer_preferences::BackgroundPreferences;

// Constants for coordinate system visualization
pub const X_AXIS_COLOR: [f32; 3] = [1.0, 0.0, 0.0]; // Red for X-axis
pub const Y_AXIS_COLOR: [f32; 3] = [0.0, 1.0, 0.0]; // Green for Y-axis
pub const Z_AXIS_COLOR: [f32; 3] = [0.0, 0.0, 1.0]; // Blue for Z-axis

// Lattice basis vector colors (for non-Cartesian aligned lattices)
pub const LATTICE_A_COLOR: [f32; 3] = [0.0, 1.0, 1.0]; // Cyan for a-vector
pub const LATTICE_B_COLOR: [f32; 3] = [1.0, 0.0, 1.0]; // Magenta for b-vector
pub const LATTICE_C_COLOR: [f32; 3] = [1.0, 1.0, 0.0]; // Yellow for c-vector

// Dotted line parameters for lattice axes
const DOT_LENGTH: f32 = 0.5; // Length of each dot in Angstroms
const GAP_LENGTH: f32 = 0.5; // Gap between dots in Angstroms

// Threshold for considering a vector aligned with a Cartesian axis
const ALIGNMENT_THRESHOLD: f64 = 0.99;

/// Tessellates a coordinate system visualization with:
/// - RGB colored coordinate axes (X=red, Y=green, Z=blue) aligned with Cartesian coordinates
/// - Dotted lattice axes for unit cell basis vectors that are not aligned with Cartesian axes
/// - A lattice grid following the unit cell's a and b basis vectors
/// - Enhanced grid lines every 10 units
pub fn tessellate_coordinate_system(output_mesh: &mut LineMesh, unit_cell: &UnitCellStruct, background_preferences: &BackgroundPreferences) {
    // Skip rendering if grid is disabled
    if !background_preferences.show_grid {
        return;
    }

    // Origin point
    let origin = DVec3::new(0.0, 0.0, 0.0);
    let cs_size = background_preferences.grid_size as f64;

    // Cartesian coordinate axes (always displayed)
    let x_axis_end = origin + DVec3::new(cs_size, 0.0, 0.0);
    let y_axis_end = origin + DVec3::new(0.0, cs_size, 0.0);
    let z_axis_end = origin + DVec3::new(0.0, 0.0, cs_size);
    
    add_axis_line(output_mesh, &origin, &x_axis_end, &X_AXIS_COLOR);
    add_axis_line(output_mesh, &origin, &y_axis_end, &Y_AXIS_COLOR);
    add_axis_line(output_mesh, &origin, &z_axis_end, &Z_AXIS_COLOR);
    
    // Lattice basis vectors (only if not aligned with Cartesian axes)
    if background_preferences.show_lattice_axes {
        add_lattice_axes_if_non_cartesian(output_mesh, unit_cell, cs_size);
    }
    
    // Grid rendering: single lattice grid or dual grid system
    if is_lattice_xy_aligned(unit_cell) {
        // Single grid mode: lattice is aligned with Cartesian XY
        // Display only the lattice grid with primary colors
        tessellate_grid_with_origin(
            output_mesh,
            &origin,
            &unit_cell.a,
            &unit_cell.b,
            background_preferences.grid_size,
            &get_grid_primary_color(background_preferences),
            &get_grid_secondary_color(background_preferences),
        );
    } else {
        // Dual grid mode: lattice not aligned with Cartesian XY
        // Display Cartesian grid with diamond spacing (primary colors)
        let cartesian_spacing = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
        tessellate_grid_with_origin(
            output_mesh,
            &origin,
            &(DVec3::new(cartesian_spacing, 0.0, 0.0)),
            &(DVec3::new(0.0, cartesian_spacing, 0.0)),
            background_preferences.grid_size,
            &get_grid_primary_color(background_preferences),
            &get_grid_secondary_color(background_preferences),
        );
        
        // Optionally display lattice grid with secondary colors
        if background_preferences.show_lattice_grid {
            tessellate_grid_with_origin(
                output_mesh,
                &origin,
                &unit_cell.a,
                &unit_cell.b,
                background_preferences.grid_size,
                &get_lattice_grid_primary_color(background_preferences),
                &get_lattice_grid_secondary_color(background_preferences),
            );
        }
    }
}

pub fn tessellate_drawing_plane_grid_and_axes(
    output_mesh: &mut LineMesh,
    drawing_plane: &DrawingPlane,
    background_preferences: &BackgroundPreferences,
) {
    if !background_preferences.show_grid {
        return;
    }

    let origin = drawing_plane.real_2d_to_world_3d(&glam::f64::DVec2::ZERO);
    let u_vector = drawing_plane.effective_unit_cell.a;
    let v_vector = drawing_plane.effective_unit_cell.b;

    let grid_primary_color: [f32; 3] = [
        background_preferences.lattice_grid_color.x as f32 / 255.0,
        background_preferences.lattice_grid_color.y as f32 / 255.0,
        background_preferences.lattice_grid_color.z as f32 / 255.0,
    ];
    let grid_secondary_color: [f32; 3] = [
        background_preferences.lattice_grid_strong_color.x as f32 / 255.0,
        background_preferences.lattice_grid_strong_color.y as f32 / 255.0,
        background_preferences.lattice_grid_strong_color.z as f32 / 255.0,
    ];

    tessellate_grid_with_origin(
        output_mesh,
        &origin,
        &u_vector,
        &v_vector,
        background_preferences.grid_size,
        &grid_primary_color,
        &grid_secondary_color,
    );

    let axis_length = background_preferences.grid_size as f64;
    add_axis_line(
        output_mesh,
        &origin,
        &(origin + u_vector.normalize() * axis_length),
        &X_AXIS_COLOR,
    );
    add_axis_line(
        output_mesh,
        &origin,
        &(origin + v_vector.normalize() * axis_length),
        &Y_AXIS_COLOR,
    );
}

/// Adds a single solid axis line from start to end with the specified color
pub fn add_axis_line(output_mesh: &mut LineMesh, start: &DVec3, end: &DVec3, color: &[f32; 3]) {
    let start_vec3 = Vec3::new(start.x as f32, start.y as f32, start.z as f32);
    let end_vec3 = Vec3::new(end.x as f32, end.y as f32, end.z as f32);
    
    output_mesh.add_line_with_uniform_color(&start_vec3, &end_vec3, color);
}

/// Adds a dotted axis line from start to end with the specified color
fn add_dotted_axis_line(output_mesh: &mut LineMesh, start: &DVec3, end: &DVec3, color: &[f32; 3]) {
    let start_vec3 = Vec3::new(start.x as f32, start.y as f32, start.z as f32);
    let end_vec3 = Vec3::new(end.x as f32, end.y as f32, end.z as f32);
    
    output_mesh.add_dotted_line(&start_vec3, &end_vec3, color, DOT_LENGTH, GAP_LENGTH);
}

/// Checks if a vector is aligned with a Cartesian axis (X, Y, or Z)
fn is_aligned_with_cartesian_axis(vec: &DVec3) -> bool {
    let normalized = vec.normalize();
    
    // Check alignment with X axis
    if normalized.dot(DVec3::new(1.0, 0.0, 0.0)).abs() > ALIGNMENT_THRESHOLD {
        return true;
    }
    
    // Check alignment with Y axis
    if normalized.dot(DVec3::new(0.0, 1.0, 0.0)).abs() > ALIGNMENT_THRESHOLD {
        return true;
    }
    
    // Check alignment with Z axis
    if normalized.dot(DVec3::new(0.0, 0.0, 1.0)).abs() > ALIGNMENT_THRESHOLD {
        return true;
    }
    
    false
}

/// Checks if the lattice a,b vectors are aligned with Cartesian X,Y plane
/// Returns true if: a is aligned with ±X AND b is aligned with ±Y
fn is_lattice_xy_aligned(unit_cell: &UnitCellStruct) -> bool {
    let a_norm = unit_cell.a.normalize();
    let b_norm = unit_cell.b.normalize();
    
    let x_axis = DVec3::new(1.0, 0.0, 0.0);
    let y_axis = DVec3::new(0.0, 1.0, 0.0);
    
    // Check if a is aligned with ±X
    let a_aligned_with_x = a_norm.dot(x_axis).abs() > ALIGNMENT_THRESHOLD;
    
    // Check if b is aligned with ±Y
    let b_aligned_with_y = b_norm.dot(y_axis).abs() > ALIGNMENT_THRESHOLD;
    
    a_aligned_with_x && b_aligned_with_y
}

/// Adds lattice axes as dotted lines, but only for vectors not aligned with Cartesian axes
fn add_lattice_axes_if_non_cartesian(output_mesh: &mut LineMesh, unit_cell: &UnitCellStruct, cs_size: f64) {
    let origin = DVec3::new(0.0, 0.0, 0.0);
    
    // Add a-vector if not aligned with Cartesian axes
    if !is_aligned_with_cartesian_axis(&unit_cell.a) {
        let a_end = origin + unit_cell.a * cs_size;
        add_dotted_axis_line(output_mesh, &origin, &a_end, &LATTICE_A_COLOR);
    }
    
    // Add b-vector if not aligned with Cartesian axes
    if !is_aligned_with_cartesian_axis(&unit_cell.b) {
        let b_end = origin + unit_cell.b * cs_size;
        add_dotted_axis_line(output_mesh, &origin, &b_end, &LATTICE_B_COLOR);
    }
    
    // Add c-vector if not aligned with Cartesian axes
    if !is_aligned_with_cartesian_axis(&unit_cell.c) {
        let c_end = origin + unit_cell.c * cs_size;
        add_dotted_axis_line(output_mesh, &origin, &c_end, &LATTICE_C_COLOR);
    }
}

/// Helper to convert preference grid color to [f32; 3]
fn get_grid_primary_color(background_preferences: &BackgroundPreferences) -> [f32; 3] {
    [
        background_preferences.grid_color.x as f32 / 255.0,
        background_preferences.grid_color.y as f32 / 255.0,
        background_preferences.grid_color.z as f32 / 255.0,
    ]
}

/// Helper to convert preference strong grid color to [f32; 3]
fn get_grid_secondary_color(background_preferences: &BackgroundPreferences) -> [f32; 3] {
    [
        background_preferences.grid_strong_color.x as f32 / 255.0,
        background_preferences.grid_strong_color.y as f32 / 255.0,
        background_preferences.grid_strong_color.z as f32 / 255.0,
    ]
}

/// Helper to convert lattice grid color to [f32; 3]
fn get_lattice_grid_primary_color(background_preferences: &BackgroundPreferences) -> [f32; 3] {
    [
        background_preferences.lattice_grid_color.x as f32 / 255.0,
        background_preferences.lattice_grid_color.y as f32 / 255.0,
        background_preferences.lattice_grid_color.z as f32 / 255.0,
    ]
}

/// Helper to convert lattice strong grid color to [f32; 3]
fn get_lattice_grid_secondary_color(background_preferences: &BackgroundPreferences) -> [f32; 3] {
    [
        background_preferences.lattice_grid_strong_color.x as f32 / 255.0,
        background_preferences.lattice_grid_strong_color.y as f32 / 255.0,
        background_preferences.lattice_grid_strong_color.z as f32 / 255.0,
    ]
}

/// Generic grid tessellation function
/// Creates a grid based on two basis vectors (u and v)
/// The grid follows these vectors to create a parallelogram grid pattern
pub fn tessellate_grid_with_origin(
    output_mesh: &mut LineMesh,
    origin: &DVec3,
    u_vector: &DVec3,
    v_vector: &DVec3,
    grid_size: i32,
    primary_color: &[f32; 3],
    secondary_color: &[f32; 3],
) {
    let grid_range = grid_size as i32;

    // Create grid lines parallel to the u vector (varying along v direction)
    for i in -grid_range..=grid_range {
        let is_emphasized = i % 10 == 0;
        let color = if is_emphasized { secondary_color } else { primary_color };
        
        // Line runs from -grid_size*u to +grid_size*u, offset by i*v
        let offset = v_vector * (i as f64);
        let start = *origin + offset - u_vector * (grid_range as f64);
        let end = *origin + offset + u_vector * (grid_range as f64);
        
        // Special case: don't draw over the u-axis when i=0
        if i == 0 {
            // Only draw the negative part
            let mid = *origin + offset;
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &mid.as_vec3(), color);
        } else {
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &end.as_vec3(), color);
        }
    }
    
    // Create grid lines parallel to the v vector (varying along u direction)
    for i in -grid_range..=grid_range {
        let is_emphasized = i % 10 == 0;
        let color = if is_emphasized { secondary_color } else { primary_color };
        
        // Line runs from -grid_size*v to +grid_size*v, offset by i*u
        let offset = u_vector * (i as f64);
        let start = *origin + offset - v_vector * (grid_range as f64);
        let end = *origin + offset + v_vector * (grid_range as f64);
        
        // Special case: don't draw over the v-axis when i=0
        if i == 0 {
            // Only draw the negative part
            let mid = *origin + offset;
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &mid.as_vec3(), color);
        } else {
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &end.as_vec3(), color);
        }
    }
}
















