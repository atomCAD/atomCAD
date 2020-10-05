/*
 * Full-screen blit (full-screen triangle)
 * - Fragment shader
 */
#version 450

layout(location = 0) in vec2 uv;

layout(set = 0, binding = 0) uniform sampler linear_sampler;
layout(set = 0, binding = 1) uniform texture2D color_texture;
// layout(set = 0, binding = 1) TODO: Add SSAO (multiple passes?)

layout(location = 0) out vec4 output_color;

void main(void) {
    const vec3 color = texture(sampler2D(color_texture, linear_sampler), uv).rgb;

    // TODO: do ssao stuff

    output_color = vec4(color, 1.0);
}