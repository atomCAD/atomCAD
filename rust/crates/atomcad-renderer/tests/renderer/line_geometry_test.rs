// The Rust twins of `line_mesh.wgsl`'s arithmetic
// (doc/design_screen_space_lines.md D4, Phase 2): the near-plane clip, the
// zero-length fallback, the corner expansion and the edge coverage.
//
// The clip tests build their matrices with the real `Camera` in both
// projections: perspective is `perspective_rh_gl`, whose `z = 0` lies at about
// 2 × znear, not at znear, so a textbook 0..1 projection would test the wrong
// thing.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::line_geometry::{
    CORNERS, MIN_SCREEN_LENGTH_PX, clip_to_near_plane, edge_coverage, expand_corner, half_width_px,
    screen_direction, segment_distance, to_screen_px,
};
use glam::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};

const VIEWPORT: Vec2 = Vec2::new(800.0, 600.0);

fn camera(orthographic: bool) -> Camera {
    let mut camera = Camera::default_pose((VIEWPORT.x / VIEWPORT.y) as f64);
    camera.orthographic = orthographic;
    camera
}

fn view_proj(camera: &Camera) -> Mat4 {
    camera.build_view_projection_matrix().as_mat4()
}

fn to_clip(camera: &Camera, p: Vec3) -> Vec4 {
    view_proj(camera) * p.extend(1.0)
}

/// A world point `distance` in front of the eye along the view direction.
fn ahead(camera: &Camera, distance: f32) -> Vec3 {
    let eye = camera.eye.as_vec3();
    eye + (camera.target.as_vec3() - eye).normalize() * distance
}

/// A world point on the view axis offset sideways, so a segment between two
/// such points is not seen end-on.
fn ahead_offset(camera: &Camera, distance: f32, sideways: f32) -> Vec3 {
    let right = (camera.target - camera.eye)
        .cross(camera.up)
        .normalize()
        .as_vec3();
    ahead(camera, distance) + right * sideways
}

// ---- Near-plane clip ---------------------------------------------------------

#[test]
fn both_ends_in_front_are_unchanged() {
    for orthographic in [false, true] {
        let camera = camera(orthographic);
        let c0 = to_clip(&camera, ahead_offset(&camera, 10.0, -1.0));
        let c1 = to_clip(&camera, ahead_offset(&camera, 40.0, 1.0));
        assert!(c0.z >= 0.0 && c1.z >= 0.0);
        assert_eq!(clip_to_near_plane(c0, c1), Some((c0, c1)));
    }
}

#[test]
fn both_ends_behind_give_nothing() {
    for orthographic in [false, true] {
        let camera = camera(orthographic);
        let c0 = to_clip(&camera, ahead_offset(&camera, -5.0, -1.0));
        let c1 = to_clip(&camera, ahead_offset(&camera, -20.0, 1.0));
        assert!(c0.z < 0.0 && c1.z < 0.0);
        assert_eq!(clip_to_near_plane(c0, c1), None);
        assert_eq!(
            expand_corner(c0, c1, 0, 1.0, 1.0, VIEWPORT),
            None,
            "a segment entirely behind the camera draws a degenerate quad"
        );
    }
}

#[test]
fn an_end_behind_is_moved_onto_the_plane_along_the_segment() {
    for orthographic in [false, true] {
        let camera = camera(orthographic);
        let behind = ahead_offset(&camera, -10.0, -3.0);
        let front = ahead_offset(&camera, 20.0, 2.0);
        let c_behind = to_clip(&camera, behind);
        let c_front = to_clip(&camera, front);
        assert!(c_behind.z < 0.0 && c_front.z > 0.0);

        for (c0, c1, behind_is_first) in [(c_behind, c_front, true), (c_front, c_behind, false)] {
            let (k0, k1) = clip_to_near_plane(c0, c1).expect("partly visible");
            let (clipped, kept, kept_original) = if behind_is_first {
                (k0, k1, c1)
            } else {
                (k1, k0, c0)
            };
            assert_eq!(kept, kept_original, "the end in front is untouched");
            assert!(clipped.z.abs() < 1e-4, "clipped onto z = 0: {clipped}");
            assert!(clipped.w > 0.0, "z >= 0 implies w > 0: {clipped}");
            // On the original clip-space segment (clip space is linear in the
            // world, so this is the world segment too).
            let t = (clipped.x - c_behind.x) / (c_front.x - c_behind.x);
            let on_segment = c_behind.lerp(c_front, t);
            assert!(
                (on_segment - clipped).length() < 1e-3,
                "{on_segment} vs {clipped}"
            );
            // Its projection is finite: no wedge through infinity.
            let s = to_screen_px(clipped, VIEWPORT);
            assert!(s.is_finite(), "{s}");
        }
    }
}

#[test]
fn perspective_clips_at_about_twice_znear() {
    let camera = camera(false);
    let znear = camera.znear as f32;
    let just_inside = to_clip(&camera, ahead(&camera, 2.0 * znear + 0.01));
    let just_outside = to_clip(&camera, ahead(&camera, 2.0 * znear - 0.01));
    assert!(just_inside.z > 0.0 && just_outside.z < 0.0);
}

#[test]
fn an_end_exactly_on_the_plane_counts_as_in_front() {
    let c0 = Vec4::new(0.3, -0.2, 0.0, 3.0);
    let c1 = Vec4::new(1.0, 1.0, 0.5, 10.0);
    assert_eq!(clip_to_near_plane(c0, c1), Some((c0, c1)));
    assert_eq!(clip_to_near_plane(c1, c0), Some((c1, c0)));
}

// ---- Zero-length fallback ------------------------------------------------------

#[test]
fn a_segment_seen_end_on_has_the_fallback_direction() {
    let (dir, length) = screen_direction(Vec2::new(5.0, 5.0), Vec2::new(5.0, 5.0));
    assert_eq!(dir, Vec2::X);
    assert_eq!(length, 0.0);

    let tiny = Vec2::new(MIN_SCREEN_LENGTH_PX * 0.5, 0.0);
    assert_eq!(screen_direction(Vec2::ZERO, tiny).0, Vec2::X);

    let (dir, length) = screen_direction(Vec2::ZERO, Vec2::new(0.0, -2.0));
    assert_eq!(dir, Vec2::new(0.0, -1.0));
    assert_eq!(length, 2.0);
}

#[test]
fn a_segment_along_the_view_axis_expands_to_a_finite_square_dot() {
    for orthographic in [false, true] {
        let camera = camera(orthographic);
        // Both ends project to the screen centre.
        let c0 = to_clip(&camera, ahead(&camera, 10.0));
        let c1 = to_clip(&camera, ahead(&camera, 30.0));
        let pad = half_width_px(2.0, 1.0) + 1.0;
        for index in 0..6 {
            let corner = expand_corner(c0, c1, index, 2.0, 1.0, VIEWPORT).unwrap();
            assert!(corner.clip.is_finite(), "{corner:?}");
            let s = to_screen_px(corner.clip, VIEWPORT);
            assert!(
                (s.x.abs() - pad).abs() < 1e-2 && (s.y.abs() - pad).abs() < 1e-2,
                "corner {index} at {s}, expected a {pad}-px square around the centre"
            );
        }
    }
}

// ---- Corner expansion ----------------------------------------------------------

#[test]
fn corners_surround_the_segment_at_half_width_plus_a_pixel() {
    let s0 = Vec2::new(-100.0, 20.0);
    let s1 = Vec2::new(150.0, 20.0);
    // Clip coordinates of those screen points at two different depths.
    let to_clip_at = |s: Vec2, z: f32, w: f32| (s / (VIEWPORT * 0.5) * w).extend(z).extend(w);
    let c0 = to_clip_at(s0, 0.3, 4.0);
    let c1 = to_clip_at(s1, 0.9, 20.0);

    let width_px = 3.0;
    let pad = half_width_px(width_px, 2.0) + 1.0;
    assert_eq!(pad, 4.0);
    for (index, (end, side)) in CORNERS.iter().enumerate() {
        let corner = expand_corner(c0, c1, index as u32, width_px, 2.0, VIEWPORT).unwrap();
        let (base, clip) = if *end == 0 { (s0, c0) } else { (s1, c1) };
        // Each corner keeps its own end's depth and w.
        assert_eq!(corner.clip.zw(), clip.zw());
        let s = to_screen_px(corner.clip, VIEWPORT);
        let along_offset = if *end == 0 { -pad } else { pad };
        let expected = base + Vec2::new(along_offset, side * pad);
        assert!(
            (s - expected).length() < 1e-3,
            "corner {index}: {s} vs {expected}"
        );
        assert_eq!(corner.across, side * pad);
        assert_eq!(corner.length, 250.0);
        let expected_along = if *end == 0 { -pad } else { 250.0 + pad };
        assert_eq!(corner.along, expected_along);
    }
}

#[test]
fn the_two_triangles_cover_the_quad() {
    // Corner order: (0,-)(1,-)(1,+) and (0,-)(1,+)(0,+): every corner of the
    // quad is used and the shared diagonal is (0,-)–(1,+).
    let mut corners: Vec<(u32, i32)> = CORNERS.iter().map(|(e, s)| (*e, *s as i32)).collect();
    corners.sort();
    corners.dedup();
    assert_eq!(corners, vec![(0, -1), (0, 1), (1, -1), (1, 1)]);
    assert_eq!(CORNERS[0], CORNERS[3]);
    assert_eq!(CORNERS[2], CORNERS[4]);
}

// ---- Coverage ------------------------------------------------------------------

#[test]
fn distance_is_across_in_the_body_and_to_the_end_beyond_it() {
    assert_eq!(segment_distance(5.0, -0.75, 10.0), 0.75);
    assert_eq!(segment_distance(-3.0, 4.0, 10.0), 5.0);
    assert_eq!(segment_distance(13.0, -4.0, 10.0), 5.0);
}

#[test]
fn coverage_at_the_centre_the_edge_and_the_far_fringe() {
    for width in [1.0f32, 2.0, 5.0] {
        let hw = half_width_px(width, 1.0);
        assert_eq!(edge_coverage(0.0, width, 1.0), 1.0);
        assert_eq!(
            edge_coverage(hw, width, 1.0),
            0.5,
            "edge of a {width} px line"
        );
        assert_eq!(edge_coverage(hw + 0.5, width, 1.0), 0.0);
        assert_eq!(edge_coverage(hw + 3.0, width, 1.0), 0.0);
    }
}

/// Summed down one column of pixels, the coverage of a horizontal line is its
/// width, wherever the line sits between pixel centres.
fn column_sum(width_px: f32, pixel_scale: f32, centre_offset: f32) -> f32 {
    (-10..=10)
        .map(|y| edge_coverage((y as f32 - centre_offset).abs(), width_px, pixel_scale))
        .sum()
}

#[test]
fn summed_coverage_equals_the_width() {
    for offset in [0.0, 0.25, 0.5, 0.8] {
        for width in [1.0f32, 2.0, 3.5] {
            let sum = column_sum(width, 1.0, offset);
            assert!(
                (sum - width).abs() < 1e-5,
                "{width} px at offset {offset}: {sum}"
            );
        }
        // In a 2× target a line covers twice as many render pixels.
        let sum = column_sum(2.0, 2.0, offset);
        assert!(
            (sum - 4.0).abs() < 1e-5,
            "2 px at 2×, offset {offset}: {sum}"
        );
    }
}

#[test]
fn a_sub_pixel_line_fades_instead_of_thinning() {
    for width in [0.1f32, 0.25, 0.6] {
        // Drawn 1 px wide…
        assert_eq!(half_width_px(width, 1.0), 0.5);
        for offset in [0.0, 0.3, 0.5] {
            // …at proportionally less coverage, which still sums to the width.
            let sum = column_sum(width, 1.0, offset);
            assert!(
                (sum - width).abs() < 1e-5,
                "{width} px at offset {offset}: {sum}"
            );
        }
        assert!((edge_coverage(0.0, width, 1.0) - width).abs() < 1e-6);
    }
    // A sub-pixel logical width that the scale lifts to a full pixel is not
    // faded.
    assert_eq!(edge_coverage(0.0, 0.5, 2.0), 1.0);
}
