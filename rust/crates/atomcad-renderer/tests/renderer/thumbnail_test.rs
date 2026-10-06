//! GPU-free pieces of node network thumbnails
//! (`doc/design_network_thumbnails.md`): content bounds and automatic framing
//! (D4), the downsample (D5), PNG round trip, and the change comparison (D3).

use atomcad_renderer::atom_impostor_mesh::{AtomImpostorMesh, AtomImpostorVertex};
use atomcad_renderer::bond_impostor_mesh::{BondImpostorMesh, BondImpostorVertex};
use atomcad_renderer::camera::Camera;
use atomcad_renderer::line_mesh::LineMesh;
use atomcad_renderer::mesh::{Mesh, Vertex};
use atomcad_renderer::thumbnail::{
    THUMBNAIL_SIZE, content_bounds, decode_png, downsample_2x2, encode_png, fit_camera_to_bounds,
    images_differ, square_camera, thumbnail_changed,
};
use atomcad_renderer::transparent_impostor_mesh::TransparentImpostorMesh;
use glam::f64::{DVec3, DVec4};

fn vec_approx_eq(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-9
}

fn atom_vertex(center: [f32; 3], radius: f32) -> AtomImpostorVertex {
    AtomImpostorVertex {
        center_position: center,
        quad_offset: [0.0, 0.0],
        radius,
        albedo: [1.0, 1.0, 1.0],
        roughness: 0.5,
        metallic: 0.0,
        rim_color: [0.0; 4],
    }
}

fn empty_meshes() -> (
    Mesh,
    LineMesh,
    AtomImpostorMesh,
    BondImpostorMesh,
    TransparentImpostorMesh,
    Mesh,
) {
    (
        Mesh::new(),
        LineMesh::new(),
        AtomImpostorMesh::new(),
        BondImpostorMesh::new(),
        TransparentImpostorMesh::new(),
        Mesh::new(),
    )
}

#[test]
fn empty_content_has_no_bounds() {
    let (main, wire, atoms, bonds, transparent, iso) = empty_meshes();
    assert_eq!(
        content_bounds(&main, &wire, &atoms, &bonds, &transparent, &iso),
        None
    );
}

#[test]
fn atoms_are_grown_by_their_radius() {
    let (main, wire, mut atoms, bonds, transparent, iso) = empty_meshes();
    atoms.vertices.push(atom_vertex([0.0, 0.0, 0.0], 1.0));
    atoms.vertices.push(atom_vertex([4.0, 0.0, 0.0], 0.5));
    atoms.indices.extend([0, 1, 0]);
    let (min, max) = content_bounds(&main, &wire, &atoms, &bonds, &transparent, &iso).unwrap();
    assert!(vec_approx_eq(min, DVec3::new(-1.0, -1.0, -1.0)));
    assert!(vec_approx_eq(max, DVec3::new(4.5, 1.0, 1.0)));
}

#[test]
fn bonds_triangles_and_vertices_without_indices() {
    let (mut main, wire, atoms, mut bonds, transparent, iso) = empty_meshes();
    bonds.vertices.push(BondImpostorVertex {
        start_position: [0.0, 0.0, 0.0],
        end_position: [0.0, 0.0, 10.0],
        quad_offset: [0.0, 0.0],
        radius: 0.25,
        color: [1.0, 1.0, 1.0],
    });
    bonds.indices.extend([0, 0, 0]);
    // A mesh with vertices but no indices draws nothing, so it is not content.
    main.vertices.push(Vertex {
        position: [100.0, 100.0, 100.0],
        normal: [0.0, 0.0, 1.0],
        albedo: [1.0, 1.0, 1.0],
        roughness: 0.5,
        metallic: 0.0,
        alpha: 1.0,
    });
    let (min, max) = content_bounds(&main, &wire, &atoms, &bonds, &transparent, &iso).unwrap();
    assert!(vec_approx_eq(min, DVec3::new(-0.25, -0.25, -0.25)));
    assert!(vec_approx_eq(max, DVec3::new(0.25, 0.25, 10.25)));
}

/// The fitted camera keeps the view direction, looks at the box centre and
/// sees the whole bounding sphere: every corner projects inside the frame.
fn assert_corners_inside(camera: &Camera, bounds: (DVec3, DVec3)) {
    let view_proj = camera.build_view_projection_matrix();
    let (min, max) = bounds;
    for i in 0..8 {
        let corner = DVec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        let clip = view_proj * DVec4::new(corner.x, corner.y, corner.z, 1.0);
        let ndc = clip.truncate() / clip.w;
        assert!(
            ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0,
            "corner {corner} at {ndc}"
        );
        // Inside the clip range too: perspective_rh_gl maps to [-1, 1],
        // orthographic_rh to [0, 1].
        assert!(
            ndc.z >= -1.0 && ndc.z <= 1.0,
            "corner {corner} clipped: {ndc}"
        );
    }
}

#[test]
fn fit_perspective_keeps_the_direction_and_frames_the_content() {
    let mut camera = Camera::default_pose(2.0);
    camera.eye = DVec3::new(0.0, -5.0, 0.0);
    camera.target = DVec3::ZERO;
    camera.up = DVec3::Z;
    let bounds = (DVec3::new(10.0, 10.0, 10.0), DVec3::new(30.0, 20.0, 12.0));

    let fitted = fit_camera_to_bounds(&camera, bounds);

    let center = DVec3::new(20.0, 15.0, 11.0);
    let radius = (bounds.1 - bounds.0).length() / 2.0;
    assert!(vec_approx_eq(fitted.target, center));
    let dir = (fitted.eye - fitted.target).normalize();
    assert!(vec_approx_eq(dir, DVec3::new(0.0, -1.0, 0.0)));
    let distance = (fitted.eye - fitted.target).length();
    assert!((distance - 1.1 * radius / (camera.fovy / 2.0).sin()).abs() < 1e-9);
    assert_eq!(fitted.aspect, 1.0);
    assert!(fitted.znear > 0.0 && fitted.znear < distance - radius);
    assert!(fitted.zfar > distance + radius);
    assert!(vec_approx_eq(fitted.up, DVec3::Z));
    assert_corners_inside(&fitted, bounds);
}

#[test]
fn fit_orthographic_sets_the_half_height() {
    let mut camera = Camera::default_pose(1.0);
    camera.orthographic = true;
    camera.ortho_half_height = 1000.0;
    let bounds = (DVec3::new(-3.0, -4.0, 0.0), DVec3::new(3.0, 4.0, 0.0));

    let fitted = fit_camera_to_bounds(&camera, bounds);

    assert!(fitted.orthographic);
    assert!((fitted.ortho_half_height - 1.1 * 5.0).abs() < 1e-9);
    assert_corners_inside(&fitted, bounds);
}

#[test]
fn fit_handles_a_point_and_a_very_large_structure() {
    let camera = Camera::default_pose(1.0);
    let point = (DVec3::splat(7.0), DVec3::splat(7.0));
    let fitted = fit_camera_to_bounds(&camera, point);
    assert!(fitted.eye.is_finite() && (fitted.eye - fitted.target).length() > 0.0);
    assert!(fitted.znear > 0.0);

    // The million-atom showcase is a few hundred nanometres across: the clip
    // range must follow the fit, not the live camera's fixed zfar = 2400.
    let huge = (DVec3::splat(-3000.0), DVec3::splat(3000.0));
    let fitted = fit_camera_to_bounds(&camera, huge);
    assert!(fitted.zfar > 2400.0);
    assert_corners_inside(&fitted, huge);
}

#[test]
fn square_camera_keeps_the_pose_and_vertical_fov() {
    let camera = Camera::default_pose(16.0 / 9.0);
    let square = square_camera(&camera);
    assert_eq!(square.aspect, 1.0);
    assert_eq!(square.fovy, camera.fovy);
    assert_eq!(square.eye, camera.eye);
}

#[test]
fn downsample_averages_each_2x2_block() {
    // 4x2 RGBA: two output pixels.
    let mut pixels = Vec::new();
    for (r, g) in [(0u8, 0u8), (100, 4), (200, 8), (255, 12)] {
        pixels.extend([r, g, 0, 255]);
    }
    for (r, g) in [(0u8, 0u8), (100, 4), (200, 8), (255, 12)] {
        pixels.extend([r, g, 0, 255]);
    }
    let out = downsample_2x2(&pixels, 4, 2);
    assert_eq!(out, vec![50, 2, 0, 255, 228, 10, 0, 255]);
}

fn solid(size: u32, rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat((size * size) as usize)
}

#[test]
fn png_round_trips() {
    let mut image = solid(THUMBNAIL_SIZE, [10, 20, 30, 255]);
    image[0..4].copy_from_slice(&[255, 0, 0, 255]);
    let png = encode_png(&image, THUMBNAIL_SIZE, THUMBNAIL_SIZE).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let (decoded, w, h) = decode_png(&png).unwrap();
    assert_eq!((w, h), (THUMBNAIL_SIZE, THUMBNAIL_SIZE));
    assert_eq!(decoded, image);
}

#[test]
fn identical_images_do_not_differ() {
    let a = solid(THUMBNAIL_SIZE, [10, 20, 30, 255]);
    assert!(!images_differ(&a, &a.clone()));
}

#[test]
fn renderer_noise_does_not_count_as_a_change() {
    let a = solid(THUMBNAIL_SIZE, [100, 100, 100, 255]);
    let mut b = a.clone();
    // Every pixel off by a few levels: below the per-pixel threshold.
    for v in b.chunks_exact_mut(4) {
        v[0] = 103;
        v[1] = 96;
    }
    assert!(!images_differ(&a, &b));
    // A handful of pixels off by a lot: below the changed-pixel fraction
    // (0.5% of 16384 pixels is 81).
    for v in b.chunks_exact_mut(4).take(80) {
        v[2] = 0;
    }
    assert!(!images_differ(&a, &b));
}

#[test]
fn a_real_change_counts() {
    let a = solid(THUMBNAIL_SIZE, [100, 100, 100, 255]);
    let mut b = a.clone();
    for v in b.chunks_exact_mut(4).take(200) {
        v[0] = 255;
    }
    assert!(images_differ(&a, &b));
}

#[test]
fn thumbnail_changed_against_the_stored_png() {
    let a = solid(THUMBNAIL_SIZE, [100, 100, 100, 255]);
    let png = encode_png(&a, THUMBNAIL_SIZE, THUMBNAIL_SIZE).unwrap();
    // No stored image: always store.
    assert!(thumbnail_changed(None, &a, THUMBNAIL_SIZE, THUMBNAIL_SIZE));
    assert!(!thumbnail_changed(
        Some(&png),
        &a,
        THUMBNAIL_SIZE,
        THUMBNAIL_SIZE
    ));
    let other = solid(THUMBNAIL_SIZE, [0, 0, 0, 255]);
    assert!(thumbnail_changed(
        Some(&png),
        &other,
        THUMBNAIL_SIZE,
        THUMBNAIL_SIZE
    ));
    // Undecodable or differently sized stored data is replaced.
    assert!(thumbnail_changed(
        Some(b"not a png"),
        &a,
        THUMBNAIL_SIZE,
        THUMBNAIL_SIZE
    ));
    let small = encode_png(&solid(4, [100, 100, 100, 255]), 4, 4).unwrap();
    assert!(thumbnail_changed(
        Some(&small),
        &a,
        THUMBNAIL_SIZE,
        THUMBNAIL_SIZE
    ));
}
