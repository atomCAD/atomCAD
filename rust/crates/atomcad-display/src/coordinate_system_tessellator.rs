use glam::f64::DVec3;

use crate::preferences::BackgroundPreferences;
use atomcad_crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use atomcad_crystolecule::drawing_plane::DrawingPlane;
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_renderer::line_mesh::{LineMesh, LineStyle};

// Constants for coordinate system visualization
pub const X_AXIS_COLOR: [f32; 3] = [1.0, 0.0, 0.0]; // Red for X-axis
pub const Y_AXIS_COLOR: [f32; 3] = [0.0, 1.0, 0.0]; // Green for Y-axis
pub const Z_AXIS_COLOR: [f32; 3] = [0.0, 0.0, 1.0]; // Blue for Z-axis

pub const DRAWING_PLANE_X_AXIS_COLOR: [f32; 3] = [1.0, 0.3, 0.3];
pub const DRAWING_PLANE_Y_AXIS_COLOR: [f32; 3] = [0.5, 1.0, 0.0];

// Lattice basis vector colors (for non-Cartesian aligned lattices)
pub const LATTICE_A_COLOR: [f32; 3] = [0.0, 1.0, 1.0]; // Cyan for a-vector
pub const LATTICE_B_COLOR: [f32; 3] = [1.0, 0.0, 1.0]; // Magenta for b-vector
pub const LATTICE_C_COLOR: [f32; 3] = [1.0, 1.0, 0.0]; // Yellow for c-vector

// Line weights in logical pixels (doc/design_screen_space_lines.md D8). The
// drawing-plane grid and axes use the same ones as the ground grid and axes.
pub const GRID_MINOR_WIDTH_PX: f32 = 1.0;
pub const GRID_MAJOR_WIDTH_PX: f32 = 1.5;
pub const AXIS_WIDTH_PX: f32 = 2.0;
/// Non-Cartesian lattice axes are dashed: `(on, off)` in logical pixels.
pub const LATTICE_AXIS_DASH_PX: (f32, f32) = (6.0, 4.0);

// Threshold for considering a vector aligned with a Cartesian axis
const ALIGNMENT_THRESHOLD: f64 = 0.99;

/// Tessellates a coordinate system visualization with:
/// - RGB colored coordinate axes (X=red, Y=green, Z=blue) aligned with Cartesian coordinates
/// - Dashed lattice axes for unit cell basis vectors that are not aligned with Cartesian axes
/// - A lattice grid following the unit cell's a and b basis vectors
/// - Enhanced grid lines every 10 units
pub fn tessellate_coordinate_system(
    output_mesh: &mut LineMesh,
    unit_cell: &UnitCellStruct,
    background_preferences: &BackgroundPreferences,
) {
    // Skip rendering if both axes and grid are disabled
    if !background_preferences.show_axes && !background_preferences.show_grid {
        return;
    }

    let origin = DVec3::new(0.0, 0.0, 0.0);
    let cs_size = background_preferences.grid_size as f64;

    // Cartesian coordinate axes (X=red, Y=green, Z=blue)
    if background_preferences.show_axes {
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
    }

    // Grid rendering: single lattice grid or dual grid system
    if background_preferences.show_grid {
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
}

pub fn tessellate_background_coordinate_system(
    unit_cell: Option<&UnitCellStruct>,
    background_preferences: &BackgroundPreferences,
) -> LineMesh {
    let mut line_mesh = LineMesh::new();

    let unit_cell_to_use = unit_cell
        .cloned()
        .unwrap_or_else(UnitCellStruct::cubic_diamond);
    tessellate_coordinate_system(&mut line_mesh, &unit_cell_to_use, background_preferences);

    line_mesh
}

pub fn tessellate_drawing_plane_grid_and_axes(
    output_mesh: &mut LineMesh,
    drawing_plane: &DrawingPlane,
    background_preferences: &BackgroundPreferences,
) {
    let origin_2d = glam::f64::DVec2::ZERO;
    let origin = drawing_plane.real_2d_to_world_3d(&origin_2d);

    // DrawingPlane::effective_unit_cell is expressed in plane-local XY coordinates.
    // Use its (a,b) vectors as 2D steps, then map plane-local positions into
    // world space via drawing_plane.real_2d_to_world_3d.
    let u_step_2d = glam::f64::DVec2::new(
        drawing_plane.effective_unit_cell.a.x,
        drawing_plane.effective_unit_cell.a.y,
    );
    let v_step_2d = glam::f64::DVec2::new(
        drawing_plane.effective_unit_cell.b.x,
        drawing_plane.effective_unit_cell.b.y,
    );

    let grid_primary_color: [f32; 3] = [
        background_preferences.drawing_plane_grid_color[0] as f32 / 255.0,
        background_preferences.drawing_plane_grid_color[1] as f32 / 255.0,
        background_preferences.drawing_plane_grid_color[2] as f32 / 255.0,
    ];
    let grid_secondary_color: [f32; 3] = [
        background_preferences.drawing_plane_grid_strong_color[0] as f32 / 255.0,
        background_preferences.drawing_plane_grid_strong_color[1] as f32 / 255.0,
        background_preferences.drawing_plane_grid_strong_color[2] as f32 / 255.0,
    ];

    let grid_range = background_preferences.grid_size;
    let axis_length_steps = background_preferences.grid_size as f64;
    let u_axis_end_2d = u_step_2d * axis_length_steps;
    let v_axis_end_2d = v_step_2d * axis_length_steps;

    // Lines parallel to the u direction (vary along v)
    for i in -grid_range..=grid_range {
        let style = grid_line_style(i, &grid_primary_color, &grid_secondary_color);

        let offset_2d = v_step_2d * (i as f64);
        let start_2d = origin_2d + offset_2d - u_step_2d * (grid_range as f64);
        let end_2d = origin_2d + offset_2d + u_step_2d * (grid_range as f64);

        let start = drawing_plane.real_2d_to_world_3d(&start_2d);
        let end = drawing_plane.real_2d_to_world_3d(&end_2d);

        // Avoid z-fighting with the drawing-plane u-axis.
        // Only skip the portion that is actually covered by the axis.
        if i == 0 {
            let mid_2d = origin_2d;
            let mid = drawing_plane.real_2d_to_world_3d(&mid_2d);
            output_mesh.add_segment(&start.as_vec3(), &mid.as_vec3(), &style);

            let axis_end = drawing_plane.real_2d_to_world_3d(&u_axis_end_2d);
            let t_axis_end = axis_length_steps;
            let t_line_end = grid_range as f64;
            if t_axis_end < t_line_end {
                output_mesh.add_segment(&axis_end.as_vec3(), &end.as_vec3(), &style);
            }
        } else {
            output_mesh.add_segment(&start.as_vec3(), &end.as_vec3(), &style);
        }
    }

    // Lines parallel to the v direction (vary along u)
    for i in -grid_range..=grid_range {
        let style = grid_line_style(i, &grid_primary_color, &grid_secondary_color);

        let offset_2d = u_step_2d * (i as f64);
        let start_2d = origin_2d + offset_2d - v_step_2d * (grid_range as f64);
        let end_2d = origin_2d + offset_2d + v_step_2d * (grid_range as f64);

        let start = drawing_plane.real_2d_to_world_3d(&start_2d);
        let end = drawing_plane.real_2d_to_world_3d(&end_2d);

        // Avoid z-fighting with the drawing-plane v-axis.
        // Only skip the portion that is actually covered by the axis.
        if i == 0 {
            let mid_2d = origin_2d;
            let mid = drawing_plane.real_2d_to_world_3d(&mid_2d);
            output_mesh.add_segment(&start.as_vec3(), &mid.as_vec3(), &style);

            let axis_end = drawing_plane.real_2d_to_world_3d(&v_axis_end_2d);
            let t_axis_end = axis_length_steps;
            let t_line_end = grid_range as f64;
            if t_axis_end < t_line_end {
                output_mesh.add_segment(&axis_end.as_vec3(), &end.as_vec3(), &style);
            }
        } else {
            output_mesh.add_segment(&start.as_vec3(), &end.as_vec3(), &style);
        }
    }

    // Axes: follow the effective unit-cell directions (u_step_2d, v_step_2d)
    let u_axis_end = drawing_plane.real_2d_to_world_3d(&u_axis_end_2d);
    let v_axis_end = drawing_plane.real_2d_to_world_3d(&v_axis_end_2d);

    add_axis_line(
        output_mesh,
        &origin,
        &u_axis_end,
        &DRAWING_PLANE_X_AXIS_COLOR,
    );
    add_axis_line(
        output_mesh,
        &origin,
        &v_axis_end,
        &DRAWING_PLANE_Y_AXIS_COLOR,
    );
}

/// Adds a single solid axis line from start to end with the specified color
pub fn add_axis_line(output_mesh: &mut LineMesh, start: &DVec3, end: &DVec3, color: &[f32; 3]) {
    output_mesh.add_segment(
        &start.as_vec3(),
        &end.as_vec3(),
        &LineStyle::solid(*color).with_width(AXIS_WIDTH_PX),
    );
}

/// Adds a lattice-axis line from start to end with the specified color,
/// dashed in screen pixels so the pattern reads the same at every zoom.
fn add_dashed_axis_line(output_mesh: &mut LineMesh, start: &DVec3, end: &DVec3, color: &[f32; 3]) {
    let (on_px, off_px) = LATTICE_AXIS_DASH_PX;
    output_mesh.add_segment(
        &start.as_vec3(),
        &end.as_vec3(),
        &LineStyle::solid(*color)
            .with_width(AXIS_WIDTH_PX)
            .dashed(on_px, off_px),
    );
}

/// Grid line `i` (counted from the origin): every tenth is a major line,
/// heavier and in the strong colour.
fn grid_line_style(i: i32, minor_color: &[f32; 3], major_color: &[f32; 3]) -> LineStyle {
    if i % 10 == 0 {
        LineStyle::solid(*major_color).with_width(GRID_MAJOR_WIDTH_PX)
    } else {
        LineStyle::solid(*minor_color).with_width(GRID_MINOR_WIDTH_PX)
    }
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

/// Adds lattice axes as dashed lines, but only for vectors not aligned with Cartesian axes
fn add_lattice_axes_if_non_cartesian(
    output_mesh: &mut LineMesh,
    unit_cell: &UnitCellStruct,
    cs_size: f64,
) {
    let origin = DVec3::new(0.0, 0.0, 0.0);

    // Add a-vector if not aligned with Cartesian axes
    if !is_aligned_with_cartesian_axis(&unit_cell.a) {
        let a_end = origin + unit_cell.a * cs_size;
        add_dashed_axis_line(output_mesh, &origin, &a_end, &LATTICE_A_COLOR);
    }

    // Add b-vector if not aligned with Cartesian axes
    if !is_aligned_with_cartesian_axis(&unit_cell.b) {
        let b_end = origin + unit_cell.b * cs_size;
        add_dashed_axis_line(output_mesh, &origin, &b_end, &LATTICE_B_COLOR);
    }

    // Add c-vector if not aligned with Cartesian axes
    if !is_aligned_with_cartesian_axis(&unit_cell.c) {
        let c_end = origin + unit_cell.c * cs_size;
        add_dashed_axis_line(output_mesh, &origin, &c_end, &LATTICE_C_COLOR);
    }
}

/// Helper to convert preference grid color to [f32; 3]
fn get_grid_primary_color(background_preferences: &BackgroundPreferences) -> [f32; 3] {
    [
        background_preferences.grid_color[0] as f32 / 255.0,
        background_preferences.grid_color[1] as f32 / 255.0,
        background_preferences.grid_color[2] as f32 / 255.0,
    ]
}

/// Helper to convert preference strong grid color to [f32; 3]
fn get_grid_secondary_color(background_preferences: &BackgroundPreferences) -> [f32; 3] {
    [
        background_preferences.grid_strong_color[0] as f32 / 255.0,
        background_preferences.grid_strong_color[1] as f32 / 255.0,
        background_preferences.grid_strong_color[2] as f32 / 255.0,
    ]
}

/// Helper to convert lattice grid color to [f32; 3]
fn get_lattice_grid_primary_color(background_preferences: &BackgroundPreferences) -> [f32; 3] {
    [
        background_preferences.lattice_grid_color[0] as f32 / 255.0,
        background_preferences.lattice_grid_color[1] as f32 / 255.0,
        background_preferences.lattice_grid_color[2] as f32 / 255.0,
    ]
}

/// Helper to convert lattice strong grid color to [f32; 3]
fn get_lattice_grid_secondary_color(background_preferences: &BackgroundPreferences) -> [f32; 3] {
    [
        background_preferences.lattice_grid_strong_color[0] as f32 / 255.0,
        background_preferences.lattice_grid_strong_color[1] as f32 / 255.0,
        background_preferences.lattice_grid_strong_color[2] as f32 / 255.0,
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
    let grid_range = grid_size;

    // Create grid lines parallel to the u vector (varying along v direction)
    for i in -grid_range..=grid_range {
        let style = grid_line_style(i, primary_color, secondary_color);

        // Line runs from -grid_size*u to +grid_size*u, offset by i*v
        let offset = v_vector * (i as f64);
        let start = *origin + offset - u_vector * (grid_range as f64);
        let end = *origin + offset + u_vector * (grid_range as f64);

        // Special case: don't draw over the u-axis when i=0
        if i == 0 {
            // Only draw the negative part
            let mid = *origin + offset;
            output_mesh.add_segment(&start.as_vec3(), &mid.as_vec3(), &style);
        } else {
            output_mesh.add_segment(&start.as_vec3(), &end.as_vec3(), &style);
        }
    }

    // Create grid lines parallel to the v vector (varying along u direction)
    for i in -grid_range..=grid_range {
        let style = grid_line_style(i, primary_color, secondary_color);

        // Line runs from -grid_size*v to +grid_size*v, offset by i*u
        let offset = u_vector * (i as f64);
        let start = *origin + offset - v_vector * (grid_range as f64);
        let end = *origin + offset + v_vector * (grid_range as f64);

        // Special case: don't draw over the v-axis when i=0
        if i == 0 {
            // Only draw the negative part
            let mid = *origin + offset;
            output_mesh.add_segment(&start.as_vec3(), &mid.as_vec3(), &style);
        } else {
            output_mesh.add_segment(&start.as_vec3(), &end.as_vec3(), &style);
        }
    }
}
