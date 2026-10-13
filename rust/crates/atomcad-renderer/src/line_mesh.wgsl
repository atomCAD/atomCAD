// Screen-space lines (doc/design_screen_space_lines.md D1, D4).
//
// One instance per segment, six vertices per instance: the vertex shader
// places each corner of a quad around the segment's screen projection, at a
// width in pixels, and the fragment shader computes an anti-aliased coverage
// from the distance to the segment. No index buffer.
//
// Every function's arithmetic has a Rust twin in `line_geometry.rs`, named in
// its comment, which the CPU tests exercise. Change both together.

struct CameraUniform {
  view_proj: mat4x4<f32>,
  view_matrix: mat4x4<f32>,
  proj_matrix: mat4x4<f32>,
  camera_position: vec3<f32>,
  head_light_dir: vec3<f32>,
  is_orthographic: f32,      // 1.0 = orthographic, 0.0 = perspective
  ortho_half_height: f32,    // Half height for orthographic projection (for zoom level)
  viewport_size_px: vec2<f32>, // Render-target size in render pixels
  pixel_scale: f32,          // Render-target pixels per output pixel
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Model transform uniform
struct ModelUniform {
  model_matrix: mat4x4<f32>,
  normal_matrix: mat4x4<f32>,
};
@group(1) @binding(0)
var<uniform> model: ModelUniform;

// Grid level of detail (D9): one `vec4(visible, fade_start, fade_end, 0)` per
// slot id. Until Phase 4 every mesh binds a uniform holding "visible, no
// fade" in every entry.
struct LineLodUniform {
  slots: array<vec4<f32>, 32>,
};
@group(2) @binding(0)
var<uniform> lod: LineLodUniform;

// `LineSegmentInstance` in `line_mesh.rs`.
struct SegmentInput {
    @location(0) p0: vec3<f32>,
    @location(1) p1: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) width_px: f32,
    @location(4) dash_on_px: f32,
    @location(5) dash_off_px: f32,
    @location(6) dash_phase: f32,
    @location(7) lod_slot: u32,
};

// The screen-space values are `linear`: each corner carries its own `w`, and
// the default perspective-correct interpolation would bend them on any line
// whose ends sit at different depths (D4). Only the built-in position (and so
// depth) interpolates perspective-correctly.
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
    // Signed distance across the line, render pixels.
    @location(1) @interpolate(linear) across: f32,
    // Distance along the line from p0, render pixels.
    @location(2) @interpolate(linear) along: f32,
    // The segment's screen length, render pixels.
    @location(3) @interpolate(flat) length_px: f32,
    // Half the drawn width, render pixels.
    @location(4) @interpolate(flat) half_width: f32,
    // Width × pixel scale: below 1 the line is drawn 1 px wide and faded.
    @location(5) @interpolate(flat) scaled_width: f32,
};

// Below this screen length a segment has no direction.
// Twin: `MIN_SCREEN_LENGTH_PX`.
const MIN_SCREEN_LENGTH_PX: f32 = 1e-4;

fn degenerate() -> VertexOutput {
    var output: VertexOutput;
    // All six vertices at one point outside the clip volume: zero area.
    output.clip_position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    output.color = vec4<f32>(0.0);
    output.across = 0.0;
    output.along = 0.0;
    output.length_px = 0.0;
    output.half_width = 0.0;
    output.scaled_width = 0.0;
    return output;
}

// Twin: `to_screen_px`.
fn to_screen_px(clip: vec4<f32>) -> vec2<f32> {
    return clip.xy / clip.w * camera.viewport_size_px * 0.5;
}

// Twin: `expand_corner` (which calls `clip_to_near_plane`,
// `screen_direction` and `half_width_px`).
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, segment: SegmentInput) -> VertexOutput {
    if (lod.slots[segment.lod_slot].x < 0.5) {
        return degenerate();
    }

    var c0 = camera.view_proj * model.model_matrix * vec4<f32>(segment.p0, 1.0);
    var c1 = camera.view_proj * model.model_matrix * vec4<f32>(segment.p1, 1.0);

    // Near-plane clip in clip space, before any divide by w (D4 step 2).
    // Without it a line passing behind the camera projects through infinity.
    if (c0.z < 0.0 && c1.z < 0.0) {
        return degenerate();
    }
    if (c0.z < 0.0) {
        c0 = mix(c0, c1, c0.z / (c0.z - c1.z));
    } else if (c1.z < 0.0) {
        c1 = mix(c1, c0, c1.z / (c1.z - c0.z));
    }

    let s0 = to_screen_px(c0);
    let s1 = to_screen_px(c1);
    let d = s1 - s0;
    let length_px = length(d);
    var dir = vec2<f32>(1.0, 0.0);
    if (length_px > MIN_SCREEN_LENGTH_PX) {
        dir = d / length_px;
    }
    let normal = vec2<f32>(-dir.y, dir.x);

    let scaled_width = segment.width_px * camera.pixel_scale;
    let half_width = 0.5 * max(scaled_width, 1.0);
    let pad = half_width + 1.0; // one render pixel of fringe

    // (end, side) of the six vertices: (0,-)(1,-)(1,+) (0,-)(1,+)(0,+).
    // Twin: `CORNERS`.
    let end = select(0u, 1u, vertex_index == 1u || vertex_index == 2u || vertex_index == 4u);
    let side = select(-1.0, 1.0, vertex_index == 2u || vertex_index == 4u || vertex_index == 5u);
    var base = s0;
    var clip = c0;
    var along = -pad;
    var along_offset = -pad;
    if (end == 1u) {
        base = s1;
        clip = c1;
        along = length_px + pad;
        along_offset = pad;
    }
    let corner_px = base + dir * along_offset + normal * side * pad;
    let ndc = corner_px / (camera.viewport_size_px * 0.5);

    var output: VertexOutput;
    // Back to clip space at this end's own w, so depth interpolates
    // perspective-correctly and is tested where the line's centre is.
    output.clip_position = vec4<f32>(ndc * clip.w, clip.z, clip.w);
    output.color = segment.color;
    output.across = side * pad;
    output.along = along;
    output.length_px = length_px;
    output.half_width = half_width;
    output.scaled_width = scaled_width;
    return output;
}

// Twin: `segment_distance` and `edge_coverage`.
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var distance = abs(input.across);
    if (input.along < 0.0) {
        distance = length(vec2<f32>(input.along, input.across));
    } else if (input.along > input.length_px) {
        distance = length(vec2<f32>(input.along - input.length_px, input.across));
    }

    let coverage = clamp(input.half_width + 0.5 - distance, 0.0, 1.0)
        * min(input.scaled_width, 1.0);
    let alpha = input.color.a * coverage;
    if (alpha < 1.0 / 255.0) {
        discard;
    }
    return vec4<f32>(input.color.rgb, alpha);
}
