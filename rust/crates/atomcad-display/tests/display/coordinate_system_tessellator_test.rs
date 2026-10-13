// Line styles of the ground grid, the axes and the drawing-plane grid
// (doc/design_screen_space_lines.md D8, Phase 3), and the grids'
// level-of-detail slots (D9, Phase 4).

use atomcad_crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use atomcad_crystolecule::drawing_plane::DrawingPlane;
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_display::coordinate_system_tessellator::{
    AXIS_WIDTH_PX, DRAWING_PLANE_X_AXIS_COLOR, DRAWING_PLANE_Y_AXIS_COLOR, GRID_MAJOR_WIDTH_PX,
    GRID_MINOR_WIDTH_PX, LATTICE_A_COLOR, LATTICE_AXIS_DASH_PX, LATTICE_B_COLOR, LATTICE_C_COLOR,
    X_AXIS_COLOR, Y_AXIS_COLOR, Z_AXIS_COLOR, tessellate_background_coordinate_system,
    tessellate_drawing_plane_grid_and_axes,
};
use atomcad_display::preferences::BackgroundPreferences;
use atomcad_renderer::line_lod::LodSlot;
use atomcad_renderer::line_mesh::{LineMesh, LineSegmentInstance};
use glam::f32::Vec3;
use glam::f64::{DVec2, DVec3};

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

// ---- Level-of-detail slots (D9, Phase 4) -------------------------------------

/// The slot ids of the segments of `color`, deduplicated.
fn slot_ids(mesh: &LineMesh, color: [f32; 4]) -> Vec<u32> {
    let mut ids: Vec<u32> = with_color(mesh, color).iter().map(|s| s.lod_slot).collect();
    ids.sort();
    ids.dedup();
    ids
}

fn assert_slot(slot: &LodSlot, normal: DVec3, spacing: f64) {
    assert_eq!(slot.plane_point, Vec3::ZERO);
    let n = slot.plane_normal.as_dvec3();
    assert!((n.length() - 1.0).abs() < 1e-6);
    assert!(
        n.cross(normal.normalize()).length() < 1e-6,
        "normal {n:?}, expected ±{normal:?}"
    );
    assert!(
        (slot.spacing as f64 - spacing).abs() < 1e-5 * spacing,
        "spacing {}, expected {spacing}",
        slot.spacing
    );
}

#[test]
fn the_ground_grid_registers_a_minor_and_a_major_slot() {
    let mesh = tessellate_background_coordinate_system(None, &preferences(true, true));
    let cell = UnitCellStruct::cubic_diamond();
    assert_eq!(mesh.lod_slots.len(), 2);
    assert_slot(&mesh.lod_slots[0], DVec3::Z, cell.a.length());
    assert_slot(&mesh.lod_slots[1], DVec3::Z, 10.0 * cell.a.length());

    assert_eq!(slot_ids(&mesh, rgba(GRID)), vec![1]);
    assert_eq!(slot_ids(&mesh, rgba(GRID_STRONG)), vec![2]);
    for color in [X_AXIS_COLOR, Y_AXIS_COLOR, Z_AXIS_COLOR] {
        assert_eq!(slot_ids(&mesh, opaque(color)), vec![0]);
    }
}

#[test]
fn dual_grid_mode_registers_slots_for_both_grids() {
    let cell = skewed_cell();
    let mesh = tessellate_background_coordinate_system(Some(&cell), &preferences(true, true));
    assert_eq!(mesh.lod_slots.len(), 4);
    // The Cartesian grid at the diamond spacing…
    assert_slot(
        &mesh.lod_slots[0],
        DVec3::Z,
        DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
    );
    assert_slot(
        &mesh.lod_slots[1],
        DVec3::Z,
        10.0 * DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
    );
    // …and the lattice grid in the a-b plane, at the smaller line distance
    // of its parallelogram.
    let normal = cell.a.cross(cell.b);
    let spacing = normal.length() / cell.a.length().max(cell.b.length());
    assert_slot(&mesh.lod_slots[2], normal, spacing);
    assert_slot(&mesh.lod_slots[3], normal, 10.0 * spacing);

    assert_eq!(slot_ids(&mesh, rgba(GRID)), vec![1]);
    assert_eq!(slot_ids(&mesh, rgba(GRID_STRONG)), vec![2]);
    assert_eq!(slot_ids(&mesh, rgba(LATTICE_GRID)), vec![3]);
    assert_eq!(slot_ids(&mesh, rgba(LATTICE_GRID_STRONG)), vec![4]);
    for color in [
        X_AXIS_COLOR,
        Y_AXIS_COLOR,
        Z_AXIS_COLOR,
        LATTICE_A_COLOR,
        LATTICE_B_COLOR,
        LATTICE_C_COLOR,
    ] {
        assert_eq!(slot_ids(&mesh, opaque(color)), vec![0]);
    }
}

#[test]
fn axes_alone_register_no_slot() {
    let mesh =
        tessellate_background_coordinate_system(Some(&skewed_cell()), &preferences(true, false));
    assert!(mesh.lod_slots.is_empty());
    assert!(mesh.instances.iter().all(|s| s.lod_slot == 0));
}

#[test]
fn each_drawing_plane_registers_its_own_slots() {
    let plane = DrawingPlane::default();
    let mut mesh = LineMesh::new();
    tessellate_drawing_plane_grid_and_axes(&mut mesh, &plane, &preferences(true, true));
    assert_eq!(mesh.lod_slots.len(), 2);
    let u = plane.real_2d_to_world_3d(&DVec2::new(
        plane.effective_unit_cell.a.x,
        plane.effective_unit_cell.a.y,
    )) - plane.real_2d_to_world_3d(&DVec2::ZERO);
    let v = plane.real_2d_to_world_3d(&DVec2::new(
        plane.effective_unit_cell.b.x,
        plane.effective_unit_cell.b.y,
    )) - plane.real_2d_to_world_3d(&DVec2::ZERO);
    let normal = u.cross(v);
    let spacing = normal.length() / u.length().max(v.length());
    assert_slot(&mesh.lod_slots[0], normal, spacing);
    assert_slot(&mesh.lod_slots[1], normal, 10.0 * spacing);
    assert_eq!(slot_ids(&mesh, rgba(PLANE_GRID)), vec![1]);
    assert_eq!(slot_ids(&mesh, rgba(PLANE_GRID_STRONG)), vec![2]);
    for color in [DRAWING_PLANE_X_AXIS_COLOR, DRAWING_PLANE_Y_AXIS_COLOR] {
        assert_eq!(slot_ids(&mesh, opaque(color)), vec![0]);
    }

    // A second plane in the same mesh gets slots of its own.
    let first_len = mesh.len();
    tessellate_drawing_plane_grid_and_axes(&mut mesh, &plane, &preferences(true, true));
    assert_eq!(mesh.lod_slots.len(), 4);
    let mut second: Vec<u32> = mesh.instances[first_len..]
        .iter()
        .map(|s| s.lod_slot)
        .collect();
    second.sort();
    second.dedup();
    assert_eq!(second, vec![0, 3, 4]);
}

#[test]
fn each_major_slot_fades_with_its_own_grids_minor_slot() {
    let fades_with = |mesh: &LineMesh| -> Vec<u32> {
        mesh.lod_slots.iter().map(|slot| slot.fades_with).collect()
    };
    let ground = tessellate_background_coordinate_system(None, &preferences(true, true));
    assert_eq!(fades_with(&ground), vec![0, 1]);

    let dual =
        tessellate_background_coordinate_system(Some(&skewed_cell()), &preferences(true, true));
    assert_eq!(fades_with(&dual), vec![0, 1, 0, 3]);

    let mut planes = LineMesh::new();
    for _ in 0..2 {
        tessellate_drawing_plane_grid_and_axes(
            &mut planes,
            &DrawingPlane::default(),
            &preferences(true, true),
        );
    }
    assert_eq!(fades_with(&planes), vec![0, 1, 0, 3]);
}
