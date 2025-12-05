use glam::f64::DVec3;
use glam::f32::Vec3;
use crate::renderer::line_mesh::LineMesh;
use crate::crystolecule::atomic_constants;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
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
    
    // Local variable to control lattice axes display (will be moved to preferences later)
    let show_lattice_axes = true;
    
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
    if show_lattice_axes {
        add_lattice_axes_if_non_cartesian(output_mesh, unit_cell, cs_size);
    }
    
    // Create grid based on unit cell lattice
    tessellate_unit_cell_grid(output_mesh, unit_cell, background_preferences);
}

/// Adds a single solid axis line from start to end with the specified color
fn add_axis_line(output_mesh: &mut LineMesh, start: &DVec3, end: &DVec3, color: &[f32; 3]) {
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

/// Checks if a vector is aligned with a Cartesian axis
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

/// Creates a grid based on the unit cell lattice structure
/// The grid follows the unit cell's a and b basis vectors (treating them as the "XY" plane equivalent)
fn tessellate_unit_cell_grid(output_mesh: &mut LineMesh, unit_cell: &UnitCellStruct, background_preferences: &BackgroundPreferences) {
    let origin = DVec3::new(0.0, 0.0, 0.0);
    
    // Calculate the number of lines needed in each direction
    let grid_size = background_preferences.grid_size;
    let grid_range = grid_size as i32;
    
    // Convert APIIVec3 colors to [f32; 3] arrays (normalize from 0-255 to 0.0-1.0)
    let grid_primary_color = [
        background_preferences.grid_color.x as f32 / 255.0,
        background_preferences.grid_color.y as f32 / 255.0,
        background_preferences.grid_color.z as f32 / 255.0,
    ];
    let grid_secondary_color = [
        background_preferences.grid_strong_color.x as f32 / 255.0,
        background_preferences.grid_strong_color.y as f32 / 255.0,
        background_preferences.grid_strong_color.z as f32 / 255.0,
    ];
    
    // Create grid lines parallel to the 'a' basis vector (varying along 'b' direction)
    for i in -grid_range..=grid_range {
        let is_emphasized = i % 10 == 0;
        let color = if is_emphasized { grid_secondary_color } else { grid_primary_color };
        
        // Line runs from -grid_size*a to +grid_size*a, offset by i*b
        let offset = unit_cell.b * (i as f64);
        let start = origin + offset - unit_cell.a * (grid_range as f64);
        let end = origin + offset + unit_cell.a * (grid_range as f64);
        
        // Special case: don't draw over the 'a' axis when i=0
        if i == 0 {
            // Only draw the negative part
            let mid = origin + offset;
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &mid.as_vec3(), &color);
        } else {
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &end.as_vec3(), &color);
        }
    }
    
    // Create grid lines parallel to the 'b' basis vector (varying along 'a' direction)
    for i in -grid_range..=grid_range {
        let is_emphasized = i % 10 == 0;
        let color = if is_emphasized { grid_secondary_color } else { grid_primary_color };
        
        // Line runs from -grid_size*b to +grid_size*b, offset by i*a
        let offset = unit_cell.a * (i as f64);
        let start = origin + offset - unit_cell.b * (grid_range as f64);
        let end = origin + offset + unit_cell.b * (grid_range as f64);
        
        // Special case: don't draw over the 'b' axis when i=0
        if i == 0 {
            // Only draw the negative part
            let mid = origin + offset;
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &mid.as_vec3(), &color);
        } else {
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &end.as_vec3(), &color);
        }
    }
}
















