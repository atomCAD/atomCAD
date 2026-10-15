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
// `camera_backward`, `camera_view_vector`, `light_surface`, `to_display`.

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

// Fragment shader

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = to_display(light_surface(
        input.world_position,
        input.normal,
        input.albedo,
        input.roughness,
        input.metallic,
        FULLY_EXPOSED,
    ));

    // Non-premultiplied source-over, matching both existing transparent
    // pipelines. Opaque meshes carry `alpha == 1.0` and are unchanged.
    return vec4<f32>(color, input.alpha);
}
