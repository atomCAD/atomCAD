use glam::f64::DVec3;
use glam::f32::Vec3;
use crate::renderer::line_mesh::LineMesh;
use crate::structure_designer::common_constants;

// Constants for coordinate system visualization
pub const CS_SIZE: i32 = 50;
pub const GRID_UNIT: f64 = common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
pub const GRID_PRIMARY_COLOR: [f32; 3] = [0.52, 0.52, 0.52]; // Light gray for regular grid lines
pub const GRID_SECONDARY_COLOR: [f32; 3] = [0.35, 0.35, 0.35]; // Darker gray for emphasized grid lines (every 10th)
pub const X_AXIS_COLOR: [f32; 3] = [1.0, 0.0, 0.0]; // Red for X-axis
pub const Y_AXIS_COLOR: [f32; 3] = [0.0, 1.0, 0.0]; // Green for Y-axis
pub const Z_AXIS_COLOR: [f32; 3] = [0.0, 0.0, 1.0]; // Blue for Z-axis

/// Tessellates a coordinate system visualization with:
/// - RGB colored coordinate axes (X=red, Y=green, Z=blue)
/// - A grid on the XZ plane with gray lines
/// - Enhanced grid lines every 10 units
pub fn tessellate_coordinate_system(output_mesh: &mut LineMesh) {
    // Origin point
    let origin = DVec3::new(0.0, 0.0, 0.0);
    
    // Coordinate axes
    let cs_size = CS_SIZE as f64 * GRID_UNIT;
    let x_end = DVec3::new(cs_size, 0.0, 0.0);
    let y_end = DVec3::new(0.0, cs_size, 0.0);
    let z_end = DVec3::new(0.0, 0.0, cs_size);
    
    // Add coordinate axes
    add_axis_line(output_mesh, &origin, &x_end, &X_AXIS_COLOR);
    add_axis_line(output_mesh, &origin, &y_end, &Y_AXIS_COLOR);
    add_axis_line(output_mesh, &origin, &z_end, &Z_AXIS_COLOR);
    
    // Create grid on XZ plane
    tessellate_xz_grid(output_mesh);
}

/// Adds a single axis line from start to end with the specified color
fn add_axis_line(output_mesh: &mut LineMesh, start: &DVec3, end: &DVec3, color: &[f32; 3]) {
    let start_vec3 = Vec3::new(start.x as f32, start.y as f32, start.z as f32);
    let end_vec3 = Vec3::new(end.x as f32, end.y as f32, end.z as f32);
    
    output_mesh.add_line_with_uniform_color(&start_vec3, &end_vec3, color);
}

/// Creates a grid on the XZ plane with the origin at the center
fn tessellate_xz_grid(output_mesh: &mut LineMesh) {
    // Grid extends from -CS_SIZE to +CS_SIZE in both X and Z directions
    let grid_size = CS_SIZE as f64 * GRID_UNIT;
    let grid_half_size = grid_size;
    let grid_start = -grid_half_size;
    let grid_end = grid_half_size;
    
    // Calculate the number of lines needed in each direction
    let line_count = 2 * CS_SIZE + 1;
    
    // Create grid lines along X-axis (parallel lines to Z axis)
    for i in 0..line_count {
        let position = grid_start + (i as f64) * GRID_UNIT;
        let is_emphasized = (i - CS_SIZE) % 10 == 0;
        let color = if is_emphasized { GRID_SECONDARY_COLOR } else { GRID_PRIMARY_COLOR };

        let start = DVec3::new(position, 0.0, grid_start);
        // Special case for the line that would overlap with Z-axis (when position = 0)
        // only draw the negative part
        let end = DVec3::new(position, 0.0, if i == CS_SIZE { 0.0 } else { grid_end });

        output_mesh.add_line_with_uniform_color(&start.as_vec3(), &end.as_vec3(), &color);
    }
    
    // Create grid lines along Z-axis (parallel lines to X axis)
    for i in 0..line_count {
        let position = grid_start + (i as f64) * GRID_UNIT;
        let is_emphasized = (i - CS_SIZE) % 10 == 0;
        let color = if is_emphasized { GRID_SECONDARY_COLOR } else { GRID_PRIMARY_COLOR };

        let start = DVec3::new(grid_start, 0.0, position);
        // Special case for the line that would overlap with X-axis (when position = 0)
        // only draw the negative part
        let end = DVec3::new(if i == CS_SIZE { 0.0 } else { grid_end }, 0.0, position);

        output_mesh.add_line_with_uniform_color(&start.as_vec3(), &end.as_vec3(), &color);
    }

}
