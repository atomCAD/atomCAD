const PI: f32 = 3.14159265359;

// Reusing the camera uniform structure from mesh.wgsl
struct CameraUniform {
  view_proj: mat4x4<f32>,
  camera_position: vec3<f32>,
  head_light_dir: vec3<f32>,
  is_orthographic: f32,      // 1.0 = orthographic, 0.0 = perspective
  ortho_half_height: f32,    // Half height for orthographic projection (for zoom level)
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

// Line vertex has position and color
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

// Vertex shader for lines
@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    
    // Apply model transform to position
    let model_position = (model.model_matrix * vec4<f32>(input.position, 1.0)).xyz;
    
    output.clip_position = camera.view_proj * vec4<f32>(model_position, 1.0);
    output.color = input.color;
    return output;
}

// Fragment shader for lines - simply output the color
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
