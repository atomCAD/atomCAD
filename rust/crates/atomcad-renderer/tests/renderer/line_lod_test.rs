// Grid level of detail: the slot rule and the `LineLodUniform` it fills
// (doc/design_screen_space_lines.md D9, Phase 4).
//
// Every camera here is the real `Camera`, so the focal length and the
// orthographic scale are the ones the renderer uses.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::line_lod::{
    HYSTERESIS, LINE_LOD_SLOTS, LodSlot, SlotLod, T_DROP_PX, T_FADE_END_PX, T_FADE_START_PX,
    lod_uniform_entries, slot_lod,
};
use glam::f32::Vec3;
use glam::f64::DVec3;

const TARGET: (u32, u32) = (800, 600);

/// The ground grid's plane, `z = 0`, with lines `spacing` apart.
fn ground_slot(spacing: f32) -> LodSlot {
    LodSlot {
        plane_point: Vec3::ZERO,
        plane_normal: Vec3::Z,
        spacing,
    }
}

/// A camera looking at the origin along `view_dir`, from `distance` away.
fn looking(view_dir: DVec3, distance: f64, orthographic: bool) -> Camera {
    let mut camera = Camera::default_pose(TARGET.0 as f64 / TARGET.1 as f64);
    let view_dir = view_dir.normalize();
    camera.target = DVec3::ZERO;
    camera.eye = -view_dir * distance;
    camera.up = if view_dir.z.abs() > 0.99 {
        DVec3::Y
    } else {
        DVec3::Z
    };
    camera.orthonormalize_up();
    camera.orthographic = orthographic;
    camera
}

/// Perspective, straight down onto the ground grid from height `h`.
fn top_down(h: f64) -> Camera {
    looking(DVec3::new(0.0, 0.0, -1.0), h, false)
}

/// The focal length in render pixels of `camera` in a target `height` tall.
fn focal_px(camera: &Camera, height: u32) -> f64 {
    height as f64 / 2.0 / (camera.fovy / 2.0).tan()
}

fn no_fade(lod: SlotLod) -> bool {
    lod.fade_end <= lod.fade_start
}

#[test]
fn perspective_top_down_drops_the_slot_exactly_at_the_threshold() {
    let spacing = 1.0;
    // `s · f / h = T_drop`.
    let threshold_h = spacing * focal_px(&top_down(1.0), TARGET.1) / T_DROP_PX as f64;
    let slot = ground_slot(spacing as f32);

    let below = slot_lod(&slot, &top_down(threshold_h * 0.999), TARGET, 1.0, None);
    let above = slot_lod(&slot, &top_down(threshold_h * 1.001), TARGET, 1.0, None);
    assert!(below.visible);
    assert!(!above.visible);
}

#[test]
fn the_fade_distances_follow_the_formula() {
    let h = 20.0;
    let camera = top_down(h);
    let f = focal_px(&camera, TARGET.1);
    let lod = slot_lod(&ground_slot(1.0), &camera, TARGET, 1.0, None);
    assert!(lod.visible);
    let expected_start = (f * h / T_FADE_START_PX as f64).sqrt() as f32;
    let expected_end = (f * h / T_FADE_END_PX as f64).sqrt() as f32;
    assert!((lod.fade_start - expected_start).abs() < 1e-3 * expected_start);
    assert!((lod.fade_end - expected_end).abs() < 1e-3 * expected_end);
    assert!(lod.fade_end > lod.fade_start);
    // A visible slot starts fading no nearer than straight below the eye.
    assert!(lod.fade_start as f64 >= h);
}

#[test]
fn a_lower_camera_gets_proportionally_shorter_fade_distances() {
    let slot = ground_slot(1.0);
    // Oblique, so the height is not the eye distance: only `h` counts.
    let dir = DVec3::new(0.0, 1.0, -0.5);
    let high = looking(dir, 40.0, false);
    let low = looking(dir, 10.0, false);
    let high_lod = slot_lod(&slot, &high, TARGET, 1.0, None);
    let low_lod = slot_lod(&slot, &low, TARGET, 1.0, None);
    assert!(high_lod.visible && low_lod.visible);
    // Four times lower: half the fade distances (∝ √h).
    assert!((high_lod.fade_start / low_lod.fade_start - 2.0).abs() < 1e-3);
    assert!((high_lod.fade_end / low_lod.fade_end - 2.0).abs() < 1e-3);
}

#[test]
fn an_eye_in_the_plane_hides_the_slot() {
    let camera = looking(DVec3::new(0.0, 1.0, 0.0), 30.0, false);
    assert_eq!(camera.eye.z, 0.0);
    let lod = slot_lod(&ground_slot(1.0), &camera, TARGET, 1.0, None);
    assert!(!lod.visible);
    assert!(no_fade(lod));
}

#[test]
fn a_plane_normal_need_not_be_normalised() {
    let camera = looking(DVec3::new(0.3, 1.0, -0.6), 25.0, false);
    let unit = slot_lod(&ground_slot(1.0), &camera, TARGET, 1.0, None);
    let long = LodSlot {
        plane_normal: Vec3::new(0.0, 0.0, -7.0),
        ..ground_slot(1.0)
    };
    assert_eq!(slot_lod(&long, &camera, TARGET, 1.0, None), unit);
}

#[test]
fn orthographic_drops_the_slot_by_how_far_the_plane_faces_the_view() {
    let slot = ground_slot(1.0);
    // `ortho_half_height` 10 in a 600 px target: k = 30 px per unit, so the
    // slot drops below c = T_drop / 30.
    let threshold_c = T_DROP_PX as f64 / 30.0;
    let tilted = |c: f64| {
        let mut camera = looking(DVec3::new(0.0, (1.0 - c * c).sqrt(), -c), 40.0, true);
        camera.ortho_half_height = 10.0;
        camera
    };
    let facing = slot_lod(&slot, &tilted(threshold_c * 1.01), TARGET, 1.0, None);
    let grazing = slot_lod(&slot, &tilted(threshold_c * 0.99), TARGET, 1.0, None);
    assert!(facing.visible);
    assert!(!grazing.visible);
    // No distance fade in orthographic.
    assert!(no_fade(facing));
    assert!(no_fade(grazing));
}

#[test]
fn an_exactly_edge_on_orthographic_view_hides_the_slot() {
    let mut camera = looking(DVec3::new(0.0, 1.0, 0.0), 40.0, true);
    camera.ortho_half_height = 10.0;
    let lod = slot_lod(&ground_slot(1.0), &camera, TARGET, 1.0, None);
    assert!(!lod.visible);
    assert!(no_fade(lod));
}

#[test]
fn orthographic_top_down_never_fades() {
    let mut camera = looking(DVec3::new(0.0, 0.0, -1.0), 40.0, true);
    camera.ortho_half_height = 10.0;
    let lod = slot_lod(&ground_slot(1.0), &camera, TARGET, 1.0, None);
    assert!(lod.visible);
    assert!(no_fade(lod));
}

#[test]
fn a_hidden_slot_comes_back_only_past_the_hysteresis_margin() {
    let slot = ground_slot(1.0);
    let f = focal_px(&top_down(1.0), TARGET.1);
    // The height at which the slot's spacing below the eye is `ratio · T_drop`.
    let at = |ratio: f64| top_down(f / (ratio * T_DROP_PX as f64));

    let between = at((1.0 + HYSTERESIS as f64) / 2.0);
    assert!(slot_lod(&slot, &between, TARGET, 1.0, None).visible);
    assert!(slot_lod(&slot, &between, TARGET, 1.0, Some(true)).visible);
    assert!(!slot_lod(&slot, &between, TARGET, 1.0, Some(false)).visible);

    let past = at(HYSTERESIS as f64 * 1.01);
    assert!(slot_lod(&slot, &past, TARGET, 1.0, Some(false)).visible);
}

#[test]
fn the_live_state_does_not_flip_back_between_the_thresholds() {
    let slots = [ground_slot(1.0)];
    let f = focal_px(&top_down(1.0), TARGET.1);
    let at = |ratio: f64| top_down(f / (ratio * T_DROP_PX as f64));
    let visible = |entries: [[f32; 4]; LINE_LOD_SLOTS]| entries[1][0] == 1.0;

    let mut state = Vec::new();
    let mut step = |ratio: f64| {
        visible(lod_uniform_entries(
            &slots,
            &at(ratio),
            TARGET,
            1.0,
            Some(&mut state),
        ))
    };
    // Zoom out past the drop, then back in to just above it: still hidden.
    assert!(step(2.0));
    assert!(!step(0.9));
    assert!(!step(1.1));
    assert!(!step(1.2));
    // Past the margin it returns, and stays while zooming out to just under.
    assert!(step(1.3));
    assert!(step(1.1));
    assert!(step(1.01));
    assert!(!step(0.99));
}

#[test]
fn a_2x_target_makes_the_same_choices_as_1x() {
    let slots = [ground_slot(0.5), ground_slot(1.0), ground_slot(5.0)];
    let doubled = (TARGET.0 * 2, TARGET.1 * 2);
    let mut cameras = Vec::new();
    for distance in [3.0, 15.0, 80.0, 400.0] {
        for dir in [
            DVec3::new(0.0, 0.0, -1.0),
            DVec3::new(0.0, 1.0, -0.4),
            DVec3::new(0.7, 1.0, -0.05),
        ] {
            cameras.push(looking(dir, distance, false));
            let mut ortho = looking(dir, distance, true);
            ortho.ortho_half_height = distance / 3.0;
            cameras.push(ortho);
        }
    }
    let mut both_states = 0;
    for camera in &cameras {
        for slot in &slots {
            let one = slot_lod(slot, camera, TARGET, 1.0, None);
            let two = slot_lod(slot, camera, doubled, 2.0, None);
            assert_eq!(one.visible, two.visible);
            assert!((one.fade_start - two.fade_start).abs() <= 1e-4 * one.fade_start.max(1.0));
            assert!((one.fade_end - two.fade_end).abs() <= 1e-4 * one.fade_end.max(1.0));
            both_states |= if one.visible { 1 } else { 2 };
        }
    }
    assert_eq!(both_states, 3, "the cameras cover both visible and hidden");
}

#[test]
fn the_uniform_holds_no_slot_at_zero_and_past_the_table() {
    let slots = [ground_slot(1.0), ground_slot(10.0)];
    let camera = looking(DVec3::new(0.0, 1.0, -0.5), 30.0, false);
    let entries = lod_uniform_entries(&slots, &camera, TARGET, 1.0, None);

    let always = SlotLod::ALWAYS.to_gpu();
    assert_eq!(always, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(entries[0], always);
    for (i, slot) in slots.iter().enumerate() {
        assert_eq!(
            entries[i + 1],
            slot_lod(slot, &camera, TARGET, 1.0, None).to_gpu()
        );
    }
    for entry in &entries[slots.len() + 1..] {
        assert_eq!(*entry, always);
    }
    // "No fade" is always `fade_end <= fade_start`.
    let mut ortho = camera.clone();
    ortho.orthographic = true;
    for entry in lod_uniform_entries(&slots, &ortho, TARGET, 1.0, None) {
        assert!(entry[2] <= entry[1]);
    }
}
