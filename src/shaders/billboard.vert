#version 450

layout(set = 0, binding = 0, std140) uniform Uniforms {
    mat4 world_mx;
    mat3 inv_view_mx; // Effectively vec4[3]?
    float sphere_radius;
} uniforms;

/*
 * Will this work? The alignment of vec3 in arrays with std430
 * is quite hazy.
 */
layout(set = 0, binding = 1, std430) buffer Points {
    vec3 points_buffer[];
};

layout(location = 0) out vec2 quad_coordinates;

void main() {
    uint particle_index = gl_VertexIndex / 6;
    uint vertex_in_quad = gl_VertexIndex % 6;

    quad_coordinates = vec2(
        bool(vertex_in_quad % 2) ? 1.0 : -1.0,
        bool(vertex_in_quad & 2) ? -1.0 : 1.0
    );

    vec3 position = uniforms.inv_view_mx *
        (uniforms.sphere_radius * vec3(
           quad_coordinates,
            1.0
        ));

    gl_Position = uniforms.world_mx * vec4(
        position + points_buffer[particle_index],
        1.0
    );
}