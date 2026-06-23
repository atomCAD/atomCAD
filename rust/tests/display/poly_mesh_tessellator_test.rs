// Tests for wireframe tessellation, in particular the coplanar-edge
// suppression added for issue #366 (improve visibility of wireframe mode).

use glam::f64::DVec3;
use rust_lib_flutter_cad::display::poly_mesh::PolyMesh;
use rust_lib_flutter_cad::display::poly_mesh_tessellator::tessellate_poly_mesh_to_line_mesh;
use rust_lib_flutter_cad::display::preferences::MeshSmoothing;
use rust_lib_flutter_cad::renderer::line_mesh::LineMesh;

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

/// Number of line segments emitted into a LineMesh (two indices per segment).
fn segment_count(mesh: &LineMesh) -> usize {
    mesh.indices.len() / 2
}

/// A unit square in the z=0 plane split into two coplanar triangles that share
/// the diagonal edge v0-v2. The diagonal is an interior line between coplanar
/// faces; the four perimeter edges are boundary (single-face) edges.
fn coplanar_split_quad() -> PolyMesh {
    let mut mesh = PolyMesh::new(false, false);
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
    let v2 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
    let v3 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
    mesh.add_face(vec![v0, v1, v2]);
    mesh.add_face(vec![v0, v2, v3]);
    mesh.compute_face_normals();
    mesh
}

/// Two unit quads sharing edge v0-v1, folded 90° apart (one in the z=0 plane,
/// one in the y=0 plane). The shared edge is a real feature edge.
fn folded_quads() -> PolyMesh {
    let mut mesh = PolyMesh::new(false, false);
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
    // First quad in z=0 plane
    let v2 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
    let v3 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
    // Second quad folded up into the y=0 plane
    let v4 = mesh.add_vertex(DVec3::new(1.0, 0.0, 1.0));
    let v5 = mesh.add_vertex(DVec3::new(0.0, 0.0, 1.0));
    mesh.add_face(vec![v0, v1, v2, v3]);
    mesh.add_face(vec![v0, v1, v4, v5]);
    mesh.compute_face_normals();
    mesh
}

fn line_mesh_for(poly_mesh: &PolyMesh, hide_coplanar: bool) -> LineMesh {
    let mut line_mesh = LineMesh::new();
    tessellate_poly_mesh_to_line_mesh(
        poly_mesh,
        &mut line_mesh,
        MeshSmoothing::Smooth,
        WHITE,
        WHITE,
        hide_coplanar,
    );
    line_mesh
}

#[test]
fn coplanar_interior_edge_drawn_when_suppression_off() {
    let mesh = coplanar_split_quad();
    // 5 unique edges: 4 perimeter + 1 diagonal.
    assert_eq!(segment_count(&line_mesh_for(&mesh, false)), 5);
}

#[test]
fn coplanar_interior_edge_hidden_when_suppression_on() {
    let mesh = coplanar_split_quad();
    // The shared diagonal between the two coplanar triangles is dropped,
    // leaving only the 4 perimeter (boundary) edges.
    assert_eq!(segment_count(&line_mesh_for(&mesh, true)), 4);
}

#[test]
fn non_coplanar_shared_edge_is_kept_even_with_suppression() {
    let mesh = folded_quads();
    // 7 unique edges (4 + 4 - 1 shared). The shared edge folds 90°, so it is a
    // real feature edge and must survive suppression.
    assert_eq!(segment_count(&line_mesh_for(&mesh, true)), 7);
    assert_eq!(segment_count(&line_mesh_for(&mesh, false)), 7);
}
