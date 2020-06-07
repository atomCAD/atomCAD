#version 450

layout(location = 0) in vec2 quad_coordinates;

layout(location = 0) out vec4 out_color;
// layout(depth_greater) out float gl_FragDepth;

void main() {
    float mag = length(quad_coordinates);
    if (mag > 1)
        discard;
    
    out_color = vec4(0.96, 0.26, 0.82, 1.0);
    // gl_FragDepth = 1 - mag;
}
