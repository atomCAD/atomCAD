// Tests for poly mesh tessellation: the coplanar-edge suppression of the
// wireframe (issue #366) and the inside copy of the solid (issue #342).

use atomcad_display::poly_mesh::PolyMesh;
use atomcad_display::poly_mesh_tessellator::{
    CSG_WIREFRAME_WIDTH_PX, tessellate_poly_mesh, tessellate_poly_mesh_to_line_mesh,
};
use atomcad_display::preferences::MeshSmoothing;
use atomcad_renderer::line_mesh::LineMesh;
use atomcad_renderer::mesh::{Material, Mesh};
use glam::f32::Vec3;
use glam::f64::DVec3;

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

/// Number of line segments emitted into a LineMesh.
fn segment_count(mesh: &LineMesh) -> usize {
    mesh.segments().count()
}

/// A unit square in the z=0 plane split into two coplanar triangles that share
/// the diagonal edge v0-v2. The diagonal is an interior line between coplanar
/// faces; the four perimeter edges are boundary (single-face) edges.
fn coplanar_split_quad() -> PolyMesh {
    let mut mesh = PolyMesh::new(false);
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
    let mut mesh = PolyMesh::new(false);
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

/// doc/design_screen_space_lines.md D8: a CSG wireframe edge is 1 px, solid.
#[test]
fn wireframe_edges_are_one_pixel_solid() {
    assert_eq!(CSG_WIREFRAME_WIDTH_PX, 1.0);
    let line_mesh = line_mesh_for(&folded_quads(), false);
    assert!(!line_mesh.is_empty());
    for segment in &line_mesh.instances {
        assert_eq!(segment.width_px, CSG_WIREFRAME_WIDTH_PX);
        assert_eq!(segment.dash_off_px, 0.0);
        assert_eq!(segment.color, [1.0, 1.0, 1.0, 1.0]);
    }
}

const SMOOTHING_MODES: [MeshSmoothing; 3] = [
    MeshSmoothing::Smooth,
    MeshSmoothing::Sharp,
    MeshSmoothing::SmoothingGroupBased,
];

/// A closed tetrahedron, every face counter-clockwise seen from outside — the
/// shape every CSG result has.
fn closed_tetrahedron() -> PolyMesh {
    let mut mesh = PolyMesh::new(false);
    let a = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let b = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
    let c = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
    let d = mesh.add_vertex(DVec3::new(0.0, 0.0, 1.0));
    mesh.add_face(vec![a, c, b]);
    mesh.add_face(vec![a, b, d]);
    mesh.add_face(vec![a, d, c]);
    mesh.add_face(vec![b, c, d]);
    mesh.compute_face_normals();
    mesh.detect_sharp_edges(30.0, true);
    mesh
}

fn solid_mesh_for(
    poly_mesh: &PolyMesh,
    smoothing: MeshSmoothing,
    inside: Option<&Material>,
) -> Mesh {
    let mut mesh = Mesh::new();
    let outside = Material::new(&Vec3::new(0.0, 1.0, 0.0), 1.0, 0.0);
    tessellate_poly_mesh(poly_mesh, &mut mesh, smoothing, &outside, inside, None);
    mesh
}

/// Issue #342: a closed CSG mesh gets an inside copy too, so it does not
/// vanish when the camera is inside it. The outside copy is wound to face away
/// from the solid and the inside copy to face into it, which is what makes
/// back-face culling show exactly one of them; every inside vertex is lit with
/// the negated normal of the outside vertex at the same place.
#[test]
fn closed_mesh_is_drawn_from_both_sides() {
    let red = Material::new(&Vec3::new(1.0, 0.0, 0.0), 1.0, 0.0);
    let centroid = Vec3::splat(0.25);
    for smoothing in SMOOTHING_MODES {
        let poly_mesh = closed_tetrahedron();
        let one_sided = solid_mesh_for(&poly_mesh, smoothing.clone(), None);
        let two_sided = solid_mesh_for(&poly_mesh, smoothing.clone(), Some(&red));
        assert_eq!(
            two_sided.indices.len(),
            2 * one_sided.indices.len(),
            "{smoothing:?}: the inside copy doubles the triangles"
        );

        let (mut outside_count, mut inside_count) = (0, 0);
        for tri in two_sided.indices.chunks(3) {
            let v: Vec<_> = tri
                .iter()
                .map(|&i| &two_sided.vertices[i as usize])
                .collect();
            let p: Vec<Vec3> = v.iter().map(|v| Vec3::from(v.position)).collect();
            let winding = (p[1] - p[0]).cross(p[2] - p[0]);
            let outward = (p[0] + p[1] + p[2]) / 3.0 - centroid;
            let is_inside = v[0].albedo == [1.0, 0.0, 0.0];
            let sign = if is_inside { -1.0 } else { 1.0 };
            assert!(sign * winding.dot(outward) > 0.0, "{smoothing:?}: winding");
            if is_inside {
                inside_count += 1;
            } else {
                outside_count += 1;
            }
        }
        assert_eq!(inside_count, outside_count, "{smoothing:?}");

        for inside in two_sided
            .vertices
            .iter()
            .filter(|v| v.albedo == [1.0, 0.0, 0.0])
        {
            let flipped = Vec3::from(inside.normal) * -1.0;
            assert!(
                two_sided.vertices.iter().any(|outside| {
                    outside.albedo != [1.0, 0.0, 0.0]
                        && outside.position == inside.position
                        && Vec3::from(outside.normal).distance(flipped) < 1e-6
                }),
                "{smoothing:?}: inside normal is not the negated outside normal"
            );
        }
    }
}
