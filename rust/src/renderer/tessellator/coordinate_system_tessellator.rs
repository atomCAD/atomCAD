use glam::f64::DVec3;
use glam::f32::Vec3;
use crate::renderer::line_mesh::LineMesh;
use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;
use crate::api::structure_designer::structure_designer_preferences::BackgroundPreferences;

// Constants for coordinate system visualization
pub const X_AXIS_COLOR: [f32; 3] = [1.0, 0.0, 0.0]; // Red for X-axis
pub const Y_AXIS_COLOR: [f32; 3] = [0.0, 1.0, 0.0]; // Green for Y-axis
pub const Z_AXIS_COLOR: [f32; 3] = [0.0, 0.0, 1.0]; // Blue for Z-axis

/// Tessellates a coordinate system visualization with:
/// - RGB colored coordinate axes (X=red, Y=green, Z=blue) aligned with unit cell basis vectors
/// - A lattice grid following the unit cell's a and b basis vectors
/// - Enhanced grid lines every 10 units
pub fn tessellate_coordinate_system(output_mesh: &mut LineMesh, unit_cell: &UnitCellStruct, background_preferences: &BackgroundPreferences) {
    // Skip rendering if grid is disabled
    if !background_preferences.show_grid {
        return;
    }
    
    // Origin point
    let origin = DVec3::new(0.0, 0.0, 0.0);
    
    // Coordinate axes using unit cell basis vectors
    // Scale the basis vectors to the desired display size
    let cs_size = background_preferences.grid_size as f64;
    let x_end = origin + unit_cell.a * cs_size;
    let y_end = origin + unit_cell.b * cs_size;
    let z_end = origin + unit_cell.c * cs_size;
    
    // Add coordinate axes
    add_axis_line(output_mesh, &origin, &x_end, &X_AXIS_COLOR);
    add_axis_line(output_mesh, &origin, &y_end, &Y_AXIS_COLOR);
    add_axis_line(output_mesh, &origin, &z_end, &Z_AXIS_COLOR);
    
    // Create grid based on unit cell lattice
    tessellate_unit_cell_grid(output_mesh, unit_cell, background_preferences);
}

/// Adds a single axis line from start to end with the specified color
fn add_axis_line(output_mesh: &mut LineMesh, start: &DVec3, end: &DVec3, color: &[f32; 3]) {
    let start_vec3 = Vec3::new(start.x as f32, start.y as f32, start.z as f32);
    let end_vec3 = Vec3::new(end.x as f32, end.y as f32, end.z as f32);
    
    output_mesh.add_line_with_uniform_color(&start_vec3, &end_vec3, color);
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
