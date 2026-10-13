// Line styles of the ground grid, the axes and the drawing-plane grid
// (doc/design_screen_space_lines.md D8, Phase 3).

use atomcad_crystolecule::drawing_plane::DrawingPlane;
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_display::coordinate_system_tessellator::{
    AXIS_WIDTH_PX, DRAWING_PLANE_X_AXIS_COLOR, DRAWING_PLANE_Y_AXIS_COLOR, GRID_MAJOR_WIDTH_PX,
    GRID_MINOR_WIDTH_PX, LATTICE_A_COLOR, LATTICE_AXIS_DASH_PX, LATTICE_B_COLOR, LATTICE_C_COLOR,
    X_AXIS_COLOR, Y_AXIS_COLOR, Z_AXIS_COLOR, tessellate_background_coordinate_system,
    tessellate_drawing_plane_grid_and_axes,
};
use atomcad_display::preferences::BackgroundPreferences;
use atomcad_renderer::line_mesh::{LineMesh, LineSegmentInstance};
use glam::f64::DVec3;

const GRID_SIZE: i32 = 12;
const GRID: [u8; 3] = [10, 20, 30];
const GRID_STRONG: [u8; 3] = [40, 50, 60];
const LATTICE_GRID: [u8; 3] = [70, 80, 90];
const LATTICE_GRID_STRONG: [u8; 3] = [100, 110, 120];
const PLANE_GRID: [u8; 3] = [130, 140, 150];
const PLANE_GRID_STRONG: [u8; 3] = [160, 170, 180];

/// Lines of one family of a `±GRID_SIZE` grid, and how many of them are
/// major: every tenth line from the origin (−10, 0, 10).
const LINES_PER_FAMILY: usize = (2 * GRID_SIZE + 1) as usize;
const MAJOR_PER_FAMILY: usize = 3;

fn preferences(show_axes: bool, show_grid: bool) -> BackgroundPreferences {
    BackgroundPreferences {
        show_axes,
        show_grid,
        grid_size: GRID_SIZE,
        grid_color: GRID,
        grid_strong_color: GRID_STRONG,
        show_lattice_axes: true,
        show_lattice_grid: true,
        lattice_grid_color: LATTICE_GRID,
        lattice_grid_strong_color: LATTICE_GRID_STRONG,
        drawing_plane_grid_color: PLANE_GRID,
        drawing_plane_grid_strong_color: PLANE_GRID_STRONG,
        unit_cell_wireframe_color: [0, 0, 0],
    }
}

fn rgba(color: [u8; 3]) -> [f32; 4] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        1.0,
    ]
}

fn opaque(color: [f32; 3]) -> [f32; 4] {
    [color[0], color[1], color[2], 1.0]
}

fn with_color(mesh: &LineMesh, color: [f32; 4]) -> Vec<LineSegmentInstance> {
    mesh.instances
        .iter()
        .copied()
        .filter(|s| s.color == color)
        .collect()
}

fn assert_solid(segment: &LineSegmentInstance, width_px: f32) {
    assert_eq!(segment.width_px, width_px);
    assert_eq!((segment.dash_on_px, segment.dash_off_px), (0.0, 0.0));
    assert_eq!(segment.color[3], 1.0);
}

/// Checks a grid's minor and major lines: their counts over both families,
/// and their widths.
fn assert_grid(mesh: &LineMesh, minor_color: [u8; 3], major_color: [u8; 3]) {
    let minor = with_color(mesh, rgba(minor_color));
    let major = with_color(mesh, rgba(major_color));
    assert_eq!(minor.len(), 2 * (LINES_PER_FAMILY - MAJOR_PER_FAMILY));
    assert_eq!(major.len(), 2 * MAJOR_PER_FAMILY);
    for segment in &minor {
        assert_solid(segment, GRID_MINOR_WIDTH_PX);
    }
    for segment in &major {
        assert_solid(segment, GRID_MAJOR_WIDTH_PX);
    }
}

/// A rhombohedral cell: none of a, b, c is along a Cartesian axis, and a, b
/// are not in the XY plane.
fn skewed_cell() -> UnitCellStruct {
    UnitCellStruct::new(
        DVec3::new(2.0, 2.0, 0.0),
        DVec3::new(0.0, 2.0, 2.0),
        DVec3::new(2.0, 0.0, 2.0),
    )
}

#[test]
fn the_d8_weights() {
    assert_eq!(GRID_MINOR_WIDTH_PX, 1.0);
    assert_eq!(GRID_MAJOR_WIDTH_PX, 1.5);
    assert_eq!(AXIS_WIDTH_PX, 2.0);
    assert_eq!(LATTICE_AXIS_DASH_PX, (6.0, 4.0));
}

#[test]
fn ground_grid_minor_and_major_lines_have_their_widths() {
    let mesh = tessellate_background_coordinate_system(None, &preferences(false, true));
    // Cubic diamond is XY-aligned: a single grid, two families.
    assert_eq!(mesh.len(), 2 * LINES_PER_FAMILY);
    assert_grid(&mesh, GRID, GRID_STRONG);
}

#[test]
fn cartesian_axes_are_solid() {
    let mesh = tessellate_background_coordinate_system(None, &preferences(true, false));
    // Cubic diamond's basis is Cartesian: no lattice axes.
    assert_eq!(mesh.len(), 3);
    for color in [X_AXIS_COLOR, Y_AXIS_COLOR, Z_AXIS_COLOR] {
        let axis = with_color(&mesh, opaque(color));
        assert_eq!(axis.len(), 1);
        assert_solid(&axis[0], AXIS_WIDTH_PX);
        assert_eq!(axis[0].p0, [0.0, 0.0, 0.0]);
    }
}

#[test]
fn non_cartesian_lattice_axes_are_dashed_in_pixels() {
    let cell = skewed_cell();
    let mesh = tessellate_background_coordinate_system(Some(&cell), &preferences(true, false));
    assert_eq!(mesh.len(), 6, "three Cartesian and three lattice axes");

    for color in [X_AXIS_COLOR, Y_AXIS_COLOR, Z_AXIS_COLOR] {
        assert_solid(&with_color(&mesh, opaque(color))[0], AXIS_WIDTH_PX);
    }
    let (on_px, off_px) = LATTICE_AXIS_DASH_PX;
    for (color, basis) in [
        (LATTICE_A_COLOR, cell.a),
        (LATTICE_B_COLOR, cell.b),
        (LATTICE_C_COLOR, cell.c),
    ] {
        let axis = with_color(&mesh, opaque(color));
        // One segment, not world-space pieces: the dashes are the shader's.
        assert_eq!(axis.len(), 1);
        let axis = axis[0];
        assert_eq!(axis.width_px, AXIS_WIDTH_PX);
        assert_eq!((axis.dash_on_px, axis.dash_off_px), (on_px, off_px));
        assert_eq!(axis.dash_phase, 0.0);
        let end = (basis * GRID_SIZE as f64).as_vec3().to_array();
        assert_eq!((axis.p0, axis.p1), ([0.0, 0.0, 0.0], end));
    }
}

#[test]
fn dual_grid_mode_styles_both_grids() {
    let cell = skewed_cell();
    let mesh = tessellate_background_coordinate_system(Some(&cell), &preferences(false, true));
    assert_eq!(mesh.len(), 4 * LINES_PER_FAMILY);
    assert_grid(&mesh, GRID, GRID_STRONG);
    assert_grid(&mesh, LATTICE_GRID, LATTICE_GRID_STRONG);
}

#[test]
fn drawing_plane_grid_and_axes_match_the_ground_grid() {
    let mut mesh = LineMesh::new();
    tessellate_drawing_plane_grid_and_axes(
        &mut mesh,
        &DrawingPlane::default(),
        &preferences(true, true),
    );
    assert_eq!(mesh.len(), 2 * LINES_PER_FAMILY + 2);
    assert_grid(&mesh, PLANE_GRID, PLANE_GRID_STRONG);
    for color in [DRAWING_PLANE_X_AXIS_COLOR, DRAWING_PLANE_Y_AXIS_COLOR] {
        let axis = with_color(&mesh, opaque(color));
        assert_eq!(axis.len(), 1);
        assert_solid(&axis[0], AXIS_WIDTH_PX);
    }
}

#[test]
fn no_line_is_tagged_with_a_lod_slot_yet() {
    let mesh =
        tessellate_background_coordinate_system(Some(&skewed_cell()), &preferences(true, true));
    assert!(mesh.instances.iter().all(|s| s.lod_slot == 0));
}
