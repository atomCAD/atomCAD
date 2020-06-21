#version 450

layout(set = 0, binding = 2) uniform Uniforms {
    mat4 projection_mx;
} uniforms;

layout(location = 0) in vec2 uv;
layout(location = 1) in vec4 position_clip_space;
layout(location = 2) flat in vec3 color;
layout(location = 3) flat in uint id;

layout(location = 0) out vec4 out_color;
layout(location = 1) out uint out_id;
layout(depth_greater) out float gl_FragDepth;

const float sphere_radius = 1.0;

void main(void) {
    float dist = length(uv);
    if (dist > 1)
        discard;

    vec4 fragment_position_clip = position_clip_space + uniforms.projection_mx[2] * (1.0 + sqrt(1 - dist*dist));
    gl_FragDepth =  fragment_position_clip.z / fragment_position_clip.w;
    
    // out_color = vec4(0.96, 0.26, 0.82, 1.0);
    out_color = vec4(color, 1.0);
    out_id = id;
}
