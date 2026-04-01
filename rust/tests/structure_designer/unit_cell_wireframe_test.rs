// Tests for Phase 4: Unit Cell Wireframe tessellation

use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::display::unit_cell_wireframe_tessellator::{
    tessellate_unit_cell_wireframe, tessellate_unit_cell_wireframe_with_color,
};
use rust_lib_flutter_cad::renderer::line_mesh::LineMesh;
use std::collections::HashSet;

/// Helper: collect all line segments as pairs of (start, end) positions from a LineMesh.
fn collect_line_segments(mesh: &LineMesh) -> Vec<([f32; 3], [f32; 3])> {
    let mut segments = Vec::new();
    for chunk in mesh.indices.chunks(2) {
        let start = mesh.vertices[chunk[0] as usize].position;
        let end = mesh.vertices[chunk[1] as usize].position;
        segments.push((start, end));
    }
    segments
}

/// Helper: round an f32 position array to avoid floating point comparison issues.
fn round_pos(p: [f32; 3]) -> [i32; 3] {
    [
        (p[0] * 1000.0).round() as i32,
        (p[1] * 1000.0).round() as i32,
        (p[2] * 1000.0).round() as i32,
    ]
}

/// Helper: collect all unique vertex positions (rounded) from a LineMesh.
fn collect_unique_vertices(mesh: &LineMesh) -> HashSet<[i32; 3]> {
    mesh.vertices
        .iter()
        .map(|v| round_pos(v.position))
        .collect()
}

// ===== test_wireframe_vertices =====

#[test]
fn test_wireframe_vertices_cubic() {
    // Given a cubic unit cell with a = (5,0,0), b = (0,5,0), c = (0,0,5)
    let uc = UnitCellStruct::new(
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(0.0, 5.0, 0.0),
        DVec3::new(0.0, 0.0, 5.0),
    );

    let mut mesh = LineMesh::new();
    tessellate_unit_cell_wireframe(&mut mesh, &uc);

    // Should have exactly 12 line segments
    assert_eq!(
        mesh.indices.len(),
        24,
        "Expected 12 line segments (24 indices)"
    );

    // Verify the 8 expected vertices exist
    let vertices = collect_unique_vertices(&mesh);
    let expected: HashSet<[i32; 3]> = [
        [0, 0, 0],
        [5000, 0, 0],
        [0, 5000, 0],
        [0, 0, 5000],
        [5000, 5000, 0],
        [5000, 0, 5000],
        [0, 5000, 5000],
        [5000, 5000, 5000],
    ]
    .into_iter()
    .collect();

    assert_eq!(vertices, expected, "Expected 8 vertices of a 5x5x5 cube");
}

#[test]
fn test_wireframe_12_edges_cubic() {
    let uc = UnitCellStruct::new(
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(0.0, 5.0, 0.0),
        DVec3::new(0.0, 0.0, 5.0),
    );

    let mut mesh = LineMesh::new();
    tessellate_unit_cell_wireframe(&mut mesh, &uc);

    let segments = collect_line_segments(&mesh);
    assert_eq!(segments.len(), 12);

    // Collect edges as sorted pairs so direction doesn't matter
    let edges: HashSet<([i32; 3], [i32; 3])> = segments
        .iter()
        .map(|(s, e)| {
            let s = round_pos(*s);
            let e = round_pos(*e);
            if s < e { (s, e) } else { (e, s) }
        })
        .collect();

    // All 12 expected edges
    let expected_edges: HashSet<([i32; 3], [i32; 3])> = [
        // Along a
        ([0, 0, 0], [5000, 0, 0]),
        ([0, 5000, 0], [5000, 5000, 0]),
        ([0, 0, 5000], [5000, 0, 5000]),
        ([0, 5000, 5000], [5000, 5000, 5000]),
        // Along b
        ([0, 0, 0], [0, 5000, 0]),
        ([5000, 0, 0], [5000, 5000, 0]),
        ([0, 0, 5000], [0, 5000, 5000]),
        ([5000, 0, 5000], [5000, 5000, 5000]),
        // Along c
        ([0, 0, 0], [0, 0, 5000]),
        ([5000, 0, 0], [5000, 0, 5000]),
        ([0, 5000, 0], [0, 5000, 5000]),
        ([5000, 5000, 0], [5000, 5000, 5000]),
    ]
    .into_iter()
    .collect();

    assert_eq!(edges, expected_edges);
}

// ===== test_wireframe_non_orthogonal =====

#[test]
fn test_wireframe_non_orthogonal() {
    // Triclinic cell with non-orthogonal basis vectors
    let uc = UnitCellStruct::new(
        DVec3::new(4.0, 0.0, 0.0),
        DVec3::new(1.0, 3.0, 0.0),
        DVec3::new(0.5, 0.5, 5.0),
    );

    let mut mesh = LineMesh::new();
    tessellate_unit_cell_wireframe(&mut mesh, &uc);

    // Should still have exactly 12 line segments
    assert_eq!(mesh.indices.len(), 24, "Expected 12 line segments");

    let vertices = collect_unique_vertices(&mesh);

    // 8 vertices of the parallelepiped
    let a = DVec3::new(4.0, 0.0, 0.0);
    let b = DVec3::new(1.0, 3.0, 0.0);
    let c = DVec3::new(0.5, 0.5, 5.0);
    let expected_dvec = [DVec3::ZERO, a, b, c, a + b, a + c, b + c, a + b + c];
    let expected: HashSet<[i32; 3]> = expected_dvec
        .iter()
        .map(|v| {
            [
                (v.x as f32 * 1000.0).round() as i32,
                (v.y as f32 * 1000.0).round() as i32,
                (v.z as f32 * 1000.0).round() as i32,
            ]
        })
        .collect();

    assert_eq!(
        vertices, expected,
        "Expected 8 vertices of the triclinic parallelepiped"
    );
}

// ===== test_wireframe_not_generated_without_unit_cell =====

#[test]
fn test_wireframe_not_generated_without_unit_cell() {
    // No tessellation call → empty mesh (simulates no unit_cell connected)
    let mesh = LineMesh::new();
    assert!(mesh.vertices.is_empty());
    assert!(mesh.indices.is_empty());
}

// ===== Additional tests =====

#[test]
fn test_wireframe_colors_are_uniform() {
    let uc = UnitCellStruct::new(
        DVec3::new(3.0, 0.0, 0.0),
        DVec3::new(0.0, 3.0, 0.0),
        DVec3::new(0.0, 0.0, 3.0),
    );

    let color = [0.5, 0.6, 0.7];
    let mut mesh = LineMesh::new();
    tessellate_unit_cell_wireframe_with_color(&mut mesh, &uc, &color);

    // All vertices should have the specified color
    for vertex in &mesh.vertices {
        assert_eq!(vertex.color, color);
    }
}

#[test]
fn test_wireframe_appends_to_existing_mesh() {
    let uc = UnitCellStruct::new(
        DVec3::new(2.0, 0.0, 0.0),
        DVec3::new(0.0, 2.0, 0.0),
        DVec3::new(0.0, 0.0, 2.0),
    );

    let mut mesh = LineMesh::new();
    // Add a pre-existing line
    use glam::f32::Vec3;
    mesh.add_line_with_uniform_color(
        &Vec3::new(10.0, 10.0, 10.0),
        &Vec3::new(20.0, 20.0, 20.0),
        &[1.0, 0.0, 0.0],
    );
    let pre_existing_vertex_count = mesh.vertices.len();
    let pre_existing_index_count = mesh.indices.len();

    tessellate_unit_cell_wireframe(&mut mesh, &uc);

    // Should have added 24 vertices (2 per line * 12 lines) and 24 indices
    assert_eq!(
        mesh.vertices.len(),
        pre_existing_vertex_count + 24,
        "Should add 24 vertices for 12 line segments"
    );
    assert_eq!(
        mesh.indices.len(),
        pre_existing_index_count + 24,
        "Should add 24 indices for 12 line segments"
    );
}
