// Tests for converting 2D CSG sketches into display PolyMeshes.
//
// Regression coverage for issue #372: in wireframe mode (triangulate_2d =
// false) 2D geometry was lifted onto the world XY plane instead of being mapped
// onto the drawing plane. Solid mode (triangulate_2d = true) always mapped
// correctly. After the fix, both modes place the geometry on the drawing plane.

use glam::f64::DVec2;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::drawing_plane::DrawingPlane;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::display::csg_to_poly_mesh::convert_csg_sketch_to_poly_mesh;
use rust_lib_flutter_cad::geo_tree::GeoNode;

/// A drawing plane tilted off the world XY plane: the (1,1,1) Miller plane
/// through the origin. Its normal has a non-trivial component on every axis, so
/// geometry mapped onto it cannot lie flat in any world axis plane.
fn tilted_plane() -> DrawingPlane {
    DrawingPlane::new(
        UnitCellStruct::cubic_diamond(),
        IVec3::new(1, 1, 1), // (111) plane
        IVec3::ZERO,         // centered at the origin (no real-space translation)
        0,                   // no shift along the normal
        1,                   // subdivision
    )
    .expect("(111) drawing plane should construct")
}

/// Real-space normal of the (111) plane, used to test plane membership.
fn tilted_plane_normal(plane: &DrawingPlane) -> glam::f64::DVec3 {
    let u_real = plane.unit_cell.ivec3_lattice_to_real(&plane.u_axis);
    let v_real = plane.unit_cell.ivec3_lattice_to_real(&plane.v_axis);
    u_real.cross(v_real).normalize()
}

fn circle_sketch() -> rust_lib_flutter_cad::geo_tree::csg_types::CSGSketch {
    GeoNode::circle(DVec2::new(0.0, 0.0), 5.0)
        .to_csg_sketch()
        .expect("circle should convert to a CSG sketch")
}

/// Wireframe mode (triangulate_2d = false) must place the outline on the drawing
/// plane, not on the world XY plane. Before the fix every vertex had z ≈ 0.
#[test]
fn wireframe_sketch_lands_on_drawing_plane_not_xy() {
    let plane = tilted_plane();
    let poly_mesh = convert_csg_sketch_to_poly_mesh(circle_sketch(), false, &plane);

    assert!(
        !poly_mesh.vertices.is_empty(),
        "expected a non-empty outline mesh"
    );

    // A circle of radius 5 mapped onto the (111) plane through the origin spans
    // a meaningful range in z. The pre-fix bug pinned every vertex to z = 0.
    let max_abs_z = poly_mesh
        .vertices
        .iter()
        .map(|v| v.position.z.abs())
        .fold(0.0_f64, f64::max);
    assert!(
        max_abs_z > 0.5,
        "wireframe outline should rise off the XY plane (max |z| = {max_abs_z})"
    );
}

/// Every vertex of the wireframe outline must lie on the (111) plane through the
/// origin: its dot product with the plane normal is ~0.
#[test]
fn wireframe_vertices_lie_on_plane() {
    let plane = tilted_plane();
    let normal = tilted_plane_normal(&plane);
    let poly_mesh = convert_csg_sketch_to_poly_mesh(circle_sketch(), false, &plane);

    for v in &poly_mesh.vertices {
        let dist = v.position.dot(normal);
        assert!(
            dist.abs() < 1e-6,
            "vertex {:?} is off the drawing plane (signed distance {dist})",
            v.position
        );
    }
}

/// Solid and wireframe modes should agree on which plane the geometry sits on:
/// both produce vertices on the (111) plane through the origin.
#[test]
fn solid_and_wireframe_share_the_drawing_plane() {
    let plane = tilted_plane();
    let normal = tilted_plane_normal(&plane);

    let solid = convert_csg_sketch_to_poly_mesh(circle_sketch(), true, &plane);
    let wireframe = convert_csg_sketch_to_poly_mesh(circle_sketch(), false, &plane);

    for mesh in [&solid, &wireframe] {
        for v in &mesh.vertices {
            assert!(
                v.position.dot(normal).abs() < 1e-6,
                "vertex {:?} is off the shared drawing plane",
                v.position
            );
        }
    }
}
