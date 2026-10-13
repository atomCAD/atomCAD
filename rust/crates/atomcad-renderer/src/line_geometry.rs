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

/// Coverage of a fragment `distance` render pixels from the segment: a
/// one-render-pixel soft edge (not scaled by `pixel_scale`, D2), and a
/// sub-pixel line drawn one pixel wide at proportionally less coverage, so it
/// fades rather than breaking into dots.
pub fn edge_coverage(distance: f32, width_px: f32, pixel_scale: f32) -> f32 {
    let width = width_px * pixel_scale;
    let coverage = (half_width_px(width_px, pixel_scale) + 0.5 - distance).clamp(0.0, 1.0);
    coverage * width.min(1.0)
}
