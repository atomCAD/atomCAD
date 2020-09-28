#version 450

struct Element {
    vec3 color;
    float radius;
};

layout(set = 0, binding = 0) uniform Camera {
    mat4 projection;
    mat4 view;
    mat4 projection_view;
} camera;

layout(location = 0) in vec2 uv;
layout(location = 1) in vec4 position_clip_space;
layout(location = 2) flat in Element element;

layout(location = 0) out vec4 color;
layout(depth_greater) out float gl_FragDepth;

void main(void) {
    const float dist = length(uv);
    if (dist > element.radius)
        discard;

    const float z = sqrt(element.radius*element.radius - dist*dist);
    // const float z = sphere_radius - dist;
    const vec4 fragment_position_clip = position_clip_space + camera.projection[2] * z;

    gl_FragDepth =  fragment_position_clip.z / fragment_position_clip.w;
    color = vec4(element.color, 1.0);
}
