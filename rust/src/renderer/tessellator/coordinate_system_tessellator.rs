use glam::f64::DVec3;
use glam::f32::Vec3;
use crate::renderer::line_mesh::LineMesh;
use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

// Constants for coordinate system visualization
pub const CS_SIZE: i32 = 50;
pub const GRID_UNIT: f64 = common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
pub const GRID_PRIMARY_COLOR: [f32; 3] = [0.52, 0.52, 0.52]; // Light gray for regular grid lines
pub const GRID_SECONDARY_COLOR: [f32; 3] = [0.35, 0.35, 0.35]; // Darker gray for emphasized grid lines (every 10th)
pub const X_AXIS_COLOR: [f32; 3] = [1.0, 0.0, 0.0]; // Red for X-axis
pub const Y_AXIS_COLOR: [f32; 3] = [0.0, 1.0, 0.0]; // Green for Y-axis
pub const Z_AXIS_COLOR: [f32; 3] = [0.0, 0.0, 1.0]; // Blue for Z-axis

/// Tessellates a coordinate system visualization with:
/// - RGB colored coordinate axes (X=red, Y=green, Z=blue) aligned with unit cell basis vectors
/// - A lattice grid following the unit cell's a and c basis vectors
/// - Enhanced grid lines every 10 units
pub fn tessellate_coordinate_system(output_mesh: &mut LineMesh, unit_cell: &UnitCellStruct) {
    // Origin point
    let origin = DVec3::new(0.0, 0.0, 0.0);
    
    // Coordinate axes using unit cell basis vectors
    // Scale the basis vectors to the desired display size
    let cs_size = CS_SIZE as f64;
    let x_end = origin + unit_cell.a * cs_size;
    let y_end = origin + unit_cell.b * cs_size;
    let z_end = origin + unit_cell.c * cs_size;
    
    // Add coordinate axes
    add_axis_line(output_mesh, &origin, &x_end, &X_AXIS_COLOR);
    add_axis_line(output_mesh, &origin, &y_end, &Y_AXIS_COLOR);
    add_axis_line(output_mesh, &origin, &z_end, &Z_AXIS_COLOR);
    
    // Create grid based on unit cell lattice
    tessellate_unit_cell_grid(output_mesh, unit_cell);
}

/// Adds a single axis line from start to end with the specified color
fn add_axis_line(output_mesh: &mut LineMesh, start: &DVec3, end: &DVec3, color: &[f32; 3]) {
    let start_vec3 = Vec3::new(start.x as f32, start.y as f32, start.z as f32);
    let end_vec3 = Vec3::new(end.x as f32, end.y as f32, end.z as f32);
    
    output_mesh.add_line_with_uniform_color(&start_vec3, &end_vec3, color);
}

/// Creates a grid based on the unit cell lattice structure
/// The grid follows the unit cell's a and c basis vectors (treating them as the "XZ" plane equivalent)
fn tessellate_unit_cell_grid(output_mesh: &mut LineMesh, unit_cell: &UnitCellStruct) {
    let origin = DVec3::new(0.0, 0.0, 0.0);
    
    // Calculate the number of lines needed in each direction
    let line_count = 2 * CS_SIZE + 1;
    let grid_range = CS_SIZE as i32;
    
    // Create grid lines parallel to the 'a' basis vector (varying along 'c' direction)
    for i in -grid_range..=grid_range {
        let is_emphasized = i % 10 == 0;
        let color = if is_emphasized { GRID_SECONDARY_COLOR } else { GRID_PRIMARY_COLOR };
        
        // Line runs from -CS_SIZE*a to +CS_SIZE*a, offset by i*c
        let offset = unit_cell.c * (i as f64);
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
    
    // Create grid lines parallel to the 'c' basis vector (varying along 'a' direction)
    for i in -grid_range..=grid_range {
        let is_emphasized = i % 10 == 0;
        let color = if is_emphasized { GRID_SECONDARY_COLOR } else { GRID_PRIMARY_COLOR };
        
        // Line runs from -CS_SIZE*c to +CS_SIZE*c, offset by i*a
        let offset = unit_cell.a * (i as f64);
        let start = origin + offset - unit_cell.c * (grid_range as f64);
        let end = origin + offset + unit_cell.c * (grid_range as f64);
        
        // Special case: don't draw over the 'c' axis when i=0
        if i == 0 {
            // Only draw the negative part
            let mid = origin + offset;
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &mid.as_vec3(), &color);
        } else {
            output_mesh.add_line_with_uniform_color(&start.as_vec3(), &end.as_vec3(), &color);
        }
    }
}
