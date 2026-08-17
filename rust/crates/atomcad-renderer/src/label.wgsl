// Atom labels: SDF glyph quads billboarded in front of their atom.
// See `doc/design_atom_labels.md` §Renderer strategy.
//
// WGSL has no cross-module sharing, so the camera/model uniforms and the camera
// basis helpers below are copied from `atom_impostor.wgsl`.

struct CameraUniform {
  view_proj: mat4x4<f32>,
  view_matrix: mat4x4<f32>,
  proj_matrix: mat4x4<f32>,
  camera_position: vec3<f32>,
  head_light_dir: vec3<f32>,
  is_orthographic: f32,      // 1.0 = orthographic, 0.0 = perspective
  ortho_half_height: f32,    // Half height for orthographic projection (for zoom level)
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct ModelUniform {
  model_matrix: mat4x4<f32>,
  normal_matrix: mat4x4<f32>,  // For transforming normals
};
@group(1) @binding(0)
var<uniform> model: ModelUniform;

// The SDF font atlas. Group 2 exists only for this pipeline — the six other
// pipelines share a two-group layout (camera, model) and are untouched.
@group(2) @binding(0)
var atlas: texture_2d<f32>;
@group(2) @binding(1)
var atlas_sampler: sampler;

struct LabelVertexInput {
    @location(0) anchor_position: vec3<f32>,
    @location(1) plane_offset: vec2<f32>,
    @location(2) depth_offset: f32,
    @location(3) glyph_uv: vec2<f32>,
}

struct LabelVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) glyph_uv: vec2<f32>,
}

// ============================================================================
// Camera Helper Functions (copied from atom_impostor.wgsl)
// ============================================================================

// Camera basis vectors in world space, extracted from the view matrix rows
// (the rotation part of a look-at matrix stores the camera axes as rows).
fn camera_right() -> vec3<f32> {
    return vec3<f32>(camera.view_matrix[0][0], camera.view_matrix[1][0], camera.view_matrix[2][0]);
}

fn camera_up() -> vec3<f32> {
    return vec3<f32>(camera.view_matrix[0][1], camera.view_matrix[1][1], camera.view_matrix[2][1]);
}

// Points from the scene toward the eye (right-handed view space +Z).
fn camera_backward() -> vec3<f32> {
    return vec3<f32>(camera.view_matrix[0][2], camera.view_matrix[1][2], camera.view_matrix[2][2]);
}

// ============================================================================
// Vertex stage
// ============================================================================

@vertex
fn vs_main(input: LabelVertexInput) -> LabelVertexOutput {
    var output: LabelVertexOutput;

    // Model matrix first, exactly as atom_impostor.wgsl does — the billboard is
    // then built in world space, so the camera basis is not model-transformed.
    // Every mesh is drawn with an identity transform today; applying it anyway
    // costs nothing and avoids a latent bug the day something sets a real one.
    let anchor = (model.model_matrix * vec4<f32>(input.anchor_position, 1.0)).xyz;

    // Unlike atom_impostor.wgsl, this uses the camera-row basis in BOTH
    // projection modes rather than switching to a per-atom eye-facing basis
    // under perspective. That looks like an inconsistency but is deliberate: a
    // sphere impostor needs the eye-facing basis so its quad covers the sphere,
    // whereas text needs a screen-aligned basis to stay upright and legible. A
    // per-atom eye-facing basis would visibly tilt labels toward the edges of a
    // perspective view.
    //
    // Moving along camera_backward() by `depth_offset` reduces the point's
    // view-space depth by exactly that amount in both projection modes, so the
    // label lands just in front of the sphere's nearest extent and depth-tests
    // normally against everything else.
    let world = anchor
              + camera_right() * input.plane_offset.x
              + camera_up() * input.plane_offset.y
              + camera_backward() * input.depth_offset;

    output.clip_position = camera.view_proj * vec4<f32>(world, 1.0);
    output.glyph_uv = input.glyph_uv;
    return output;
}

// ============================================================================
// Fragment stage
// ============================================================================

// White fill with a black outline, fixed. A label sits in front of its atom but
// extends past the silhouette onto whatever is behind it, so it needs contrast
// against both the atom's albedo and the background; white-on-black-outline is
// readable against anything, for zero fields and no per-atom logic. The outline
// is nearly free — a second distance band on a sample the shader already took.
const FILL_COLOR: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);
const OUTLINE_COLOR: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);
// Distance band below the 0.5 edge, in SDF units. Must stay well inside the
// atlas's encoded spread (`font_metrics::SDF_SPREAD_EM`) or the band clips flat
// and the outline degrades to a hard edge.
const OUTLINE_WIDTH: f32 = 0.15;
// Below this coverage, write no color and no depth — empty texels must not
// pollute the depth buffer.
const ALPHA_DISCARD: f32 = 0.01;

@fragment
fn fs_main(input: LabelVertexOutput) -> @location(0) vec4<f32> {
    let d = textureSample(atlas, atlas_sampler, input.glyph_uv).r; // 0.5 = glyph edge
    let w = fwidth(d);
    let fill = smoothstep(0.5 - w, 0.5 + w, d);
    let outline = smoothstep(0.5 - OUTLINE_WIDTH - w, 0.5 - OUTLINE_WIDTH + w, d);
    if outline < ALPHA_DISCARD {
        discard;
    }
    return vec4<f32>(mix(OUTLINE_COLOR, FILL_COLOR, fill), outline);
}
