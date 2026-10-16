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

struct ModelUniform {
  model_matrix: mat4x4<f32>,
  normal_matrix: mat4x4<f32>,  // For transforming normals
};
@group(1) @binding(0)
var<uniform> model: ModelUniform;

// ---- lighting ----
// `lighting.wgsl` is inserted here (`pipelines::compose_lit_shader`):
// `camera_backward`, `camera_view_vector`, `light_surface`, `to_display`,
// `apply_depth_cue`.

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) albedo: vec3<f32>,
    @location(3) roughness: f32,
    @location(4) metallic: f32,
    // Per-vertex opacity. Every opaque producer writes 1.0 (`Vertex::new`), so
    // this is a no-op for every mesh but the transparent isosurface one.
    @location(5) alpha: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) albedo: vec3<f32>,
    @location(3) roughness: f32,
    @location(4) metallic: f32,
    @location(5) alpha: f32,
};

// Vertex shader

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    
    // Apply model transform to position and normal
    let model_position = (model.model_matrix * vec4<f32>(input.position, 1.0)).xyz;
    let model_normal = normalize((model.normal_matrix * vec4<f32>(input.normal, 0.0)).xyz);
    
    output.clip_position = camera.view_proj * vec4<f32>(model_position, 1.0);
    output.world_position = model_position;
    output.normal = model_normal;
    output.roughness = input.roughness;
    output.metallic = input.metallic;
    output.albedo = input.albedo;
    output.alpha = input.alpha;
    return output;
}

// The normal of the face a fragment lies on, from the screen derivatives of
// its world position, turned towards the viewer (doc/design_shadows.md D7).
// The shading normal of a smooth mesh is interpolated and differs from the
// face; biasing the shadow lookup by the face is what keeps a coarse
// tessellation free of terminator bands. Falls back to the shading normal
// where the derivatives are degenerate (a face seen edge-on).
fn face_normal(dwx: vec3<f32>, dwy: vec3<f32>, world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let c = cross(dwx, dwy);
    if dot(c, c) < 1.0e-30 {
        return normal;
    }
    let face = normalize(c);
    if dot(face, camera_view_vector(world_position)) < 0.0 {
        return -face;
    }
    return face;
}

// Fragment shader

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Derivatives first, in uniform control flow (this shader never
    // discards, but the rule is the impostors').
    let geo_normal = face_normal(
        dpdxFine(input.world_position),
        dpdyFine(input.world_position),
        input.world_position,
        input.normal,
    );
    let color = apply_depth_cue(to_display(light_surface(
        input.world_position,
        input.normal,
        input.albedo,
        input.roughness,
        input.metallic,
        FULLY_EXPOSED,
        key_visibility(shadow_visibility(input.world_position, geo_normal)),
    )), input.world_position);

    // Non-premultiplied source-over, matching both existing transparent
    // pipelines. Opaque meshes carry `alpha == 1.0` and are unchanged.
    return vec4<f32>(color, input.alpha);
}
