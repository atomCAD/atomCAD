//! Rust twins of the arithmetic in `line_mesh.wgsl`
//! (`doc/design_screen_space_lines.md` D4, Testing layer 1).
//!
//! The shader is the implementation; these functions repeat its math line for
//! line so it can be unit-tested on the CPU. Each WGSL function names its twin
//! here, and a change to one must be made to the other. A twin can drift from
//! the WGSL silently, so the GPU harness (`tests/renderer_gpu.rs`) is the
//! cross-check.

use glam::{Vec2, Vec4, Vec4Swizzles};

/// Below this screen length, in render pixels, a segment has no direction and
/// is drawn as a dot (D4 step 4).
pub const MIN_SCREEN_LENGTH_PX: f32 = 1e-4;

/// Which end (0 = `p0`, 1 = `p1`) and which side (−1, +1) each of the six
/// vertices of a segment's quad sits at: two triangles, (0,−)(1,−)(1,+) and
/// (0,−)(1,+)(0,+). Twin of `CORNERS` in `line_mesh.wgsl`.
pub const CORNERS: [(u32, f32); 6] = [
    (0, -1.0),
    (1, -1.0),
    (1, 1.0),
    (0, -1.0),
    (1, 1.0),
    (0, 1.0),
];

/// Clips the clip-space segment `c0`–`c1` against the plane `z = 0`, where
/// wgpu's rasteriser clips, before any divide by `w` (D4 step 2). `None` when
/// both ends are behind it. An end exactly on the plane counts as in front.
///
/// For both of the camera's projections `z ≥ 0` implies `w > 0`, so the
/// result can be divided by `w`.
pub fn clip_to_near_plane(c0: Vec4, c1: Vec4) -> Option<(Vec4, Vec4)> {
    match (c0.z >= 0.0, c1.z >= 0.0) {
        (true, true) => Some((c0, c1)),
        (false, false) => None,
        (false, true) => Some((c0.lerp(c1, c0.z / (c0.z - c1.z)), c1)),
        (true, false) => Some((c0, c1.lerp(c0, c1.z / (c1.z - c0.z)))),
    }
}

/// A clip-space point in render pixels from the viewport centre, y up.
pub fn to_screen_px(clip: Vec4, viewport_size_px: Vec2) -> Vec2 {
    clip.xy() / clip.w * viewport_size_px * 0.5
}

/// The unit screen direction from `s0` to `s1` and the distance between them.
/// Below [`MIN_SCREEN_LENGTH_PX`] the direction is `(1, 0)` instead of NaN
/// (D4 step 4).
pub fn screen_direction(s0: Vec2, s1: Vec2) -> (Vec2, f32) {
    let d = s1 - s0;
    let length = d.length();
    if length > MIN_SCREEN_LENGTH_PX {
        (d / length, length)
    } else {
        (Vec2::X, length)
    }
}

/// Half the drawn width in render pixels: never under half a pixel, so a
/// sub-pixel line is drawn one pixel wide and faded instead (D4).
pub fn half_width_px(width_px: f32, pixel_scale: f32) -> f32 {
    0.5 * (width_px * pixel_scale).max(1.0)
}

/// One vertex of a segment's quad, as the vertex shader emits it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineCorner {
    /// Clip-space position: the expanded corner, at the `w` of its own end.
    pub clip: Vec4,
    /// Signed distance across the line, render pixels.
    pub across: f32,
    /// Distance along the line from `p0`, render pixels.
    pub along: f32,
    /// The segment's screen length, render pixels.
    pub length: f32,
}

/// Twin of `vs_main`: vertex `vertex_index` (0..6) of the quad for the
/// clip-space segment `c0`–`c1`. `None` = the segment is entirely behind the
/// near plane and the shader emits a degenerate quad.
pub fn expand_corner(
    c0: Vec4,
    c1: Vec4,
    vertex_index: u32,
    width_px: f32,
    pixel_scale: f32,
    viewport_size_px: Vec2,
) -> Option<LineCorner> {
    let (c0, c1) = clip_to_near_plane(c0, c1)?;
    let s0 = to_screen_px(c0, viewport_size_px);
    let s1 = to_screen_px(c1, viewport_size_px);
    let (dir, length) = screen_direction(s0, s1);
    let normal = Vec2::new(-dir.y, dir.x);
    let pad = half_width_px(width_px, pixel_scale) + 1.0;

    let (end, side) = CORNERS[vertex_index as usize];
    let (base, clip, along) = if end == 0 {
        (s0, c0, -pad)
    } else {
        (s1, c1, length + pad)
    };
    let along_offset = if end == 0 { -pad } else { pad };
    let corner_px = base + dir * along_offset + normal * side * pad;
    let ndc = corner_px / (viewport_size_px * 0.5);
    Some(LineCorner {
        clip: Vec4::new(ndc.x * clip.w, ndc.y * clip.w, clip.z, clip.w),
        across: side * pad,
        along,
        length,
    })
}

/// Distance in render pixels from a fragment to the segment: across the line
/// in its body, to the nearer end point beyond either end (round caps).
pub fn segment_distance(along: f32, across: f32, length: f32) -> f32 {
    if along < 0.0 {
        Vec2::new(along, across).length()
    } else if along > length {
        Vec2::new(along - length, across).length()
    } else {
        across.abs()
    }
}

/// Below this `w` an unclipped `p0` counts as behind the eye, and the dash
/// pattern is measured from the clipped end instead (D5).
pub const MIN_DASH_ORIGIN_W: f32 = 1e-4;

/// Render pixels per world unit at the clip-space point `origin`, moving
/// along a segment whose clip-space change per world unit is `delta`: the
/// length of the derivative of the projection there. Constant along a
/// segment in orthographic projection, where `w` does not change.
pub fn pixels_per_world_unit(origin: Vec4, delta: Vec4, viewport_size_px: Vec2) -> f32 {
    let derivative = (delta.xy() * origin.w - origin.xy() * delta.w) / (origin.w * origin.w);
    (derivative * viewport_size_px * 0.5).length()
}

/// Where in its dash pattern a segment's clipped `p0` lies, in render pixels
/// from the start of a period: `along + dash_offset_px(..)` is the fragment's
/// position in the pattern (D5).
///
/// `unclipped` is the segment's clip-space ends before the near-plane clip,
/// `c0` the start after it, `world_length` the segment's length in the mesh's
/// coordinates (the unit of `dash_phase`) and `period_px` the scaled
/// `on + off`. The pattern is measured from the unclipped `p0` while that is
/// in front of the eye, and from the clipped one otherwise; the polyline's
/// `dash_phase` is converted with the pixels per world unit there. The result
/// is reduced into `[0, period_px)`, so the fragment's arithmetic stays exact
/// however far off screen `p0` projects. `0` when the line is solid
/// (`period_px <= 0`).
pub fn dash_offset_px(
    unclipped: (Vec4, Vec4),
    c0: Vec4,
    world_length: f32,
    dash_phase: f32,
    period_px: f32,
    viewport_size_px: Vec2,
) -> f32 {
    if period_px <= 0.0 {
        return 0.0;
    }
    let (u0, u1) = unclipped;
    let origin = if u0.w > MIN_DASH_ORIGIN_W { u0 } else { c0 };
    let shift = to_screen_px(c0, viewport_size_px).distance(to_screen_px(origin, viewport_size_px));
    let phase_px = if world_length > 0.0 {
        dash_phase * pixels_per_world_unit(origin, (u1 - u0) / world_length, viewport_size_px)
    } else {
        0.0
    };
    let offset = shift + phase_px;
    offset - period_px * (offset / period_px).floor()
}

/// Coverage of the dash pattern at `t` render pixels into it: the "on"
/// interval `[0, on_px)` of every `period_px`, with the same one-pixel soft
/// edge as the sides of the line, so dash ends are anti-aliased too. `1` when
/// the line is solid (`period_px <= 0`).
pub fn dash_coverage(t: f32, on_px: f32, period_px: f32) -> f32 {
    if period_px <= 0.0 {
        return 1.0;
    }
    // Distance from the middle of the nearest dash.
    let u = t - 0.5 * on_px;
    let u = u - period_px * (u / period_px + 0.5).floor();
    (0.5 * on_px + 0.5 - u.abs()).clamp(0.0, 1.0)
}

/// Coverage of a fragment `distance` render pixels from the segment: a
/// one-render-pixel soft edge (not scaled by `pixel_scale`, D2), and a
/// sub-pixel line drawn one pixel wide at proportionally less coverage, so it
/// fades rather than breaking into dots.
pub fn edge_coverage(distance: f32, width_px: f32, pixel_scale: f32) -> f32 {
    let width = width_px * pixel_scale;
    let coverage = (half_width_px(width_px, pixel_scale) + 0.5 - distance).clamp(0.0, 1.0);
    coverage * width.min(1.0)
}

/// The grid distance fade (D9): 1 up to `fade_start` eye distance, 0 from
/// `fade_end` on, a smoothstep between. `fade_end <= fade_start` means no
/// fade (WGSL's `smoothstep` is undefined for equal or reversed edges).
pub fn distance_fade(eye_distance: f32, fade_start: f32, fade_end: f32) -> f32 {
    if fade_end <= fade_start {
        return 1.0;
    }
    let t = ((eye_distance - fade_start) / (fade_end - fade_start)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}
