#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 out_color;
layout(set = 0, binding = 0) uniform texture2D ui_texture;
layout(set = 0, binding = 1) uniform texture2D scene_texture;
layout(set = 0, binding = 2) uniform sampler texture_sampler;

void main() {
    vec4 ui = texture(sampler2D(ui_texture, texture_sampler), tex_coords);
    vec4 scene = texture(sampler2D(scene_texture, texture_sampler), tex_coords);
    out_color = mix(scene, ui, ui.a);
}
