/*
 * This shader uses an interesting technique that is sometimes
 * called progrmamable vertex pulling.
 *
 * Instead of providing a vertex buffer, we instead provide an array
 * of points in a storage buffer and then call draw with <number of points> * 6
 *
 * This is apparently faster than instancing for small meshes.
 *
 * See slide 20 of https://www.slideshare.net/DevCentralAMD/vertex-shader-tricks-bill-bilodeau.
 */
#version 450

layout(set = 0, binding = 0) uniform Uniforms {
    mat4 world_mx;
    mat3 inv_view_mx; // Effectively vec4[3]?
    float sphere_radius;
} uniforms;

struct Atom {
    vec3 pos;
};

/*
 * Will this work? The alignment of vec3 in arrays with std430
 * is quite hazy.
 */
layout(set = 0, binding = 1) buffer Points {
    Atom atom_buffer[];
};

layout(location = 0) out vec2 quad_coordinates;

void main() {
    uint particle_index = gl_VertexIndex / 6;
    uint vertex_in_tri = abs(3 - gl_VertexIndex % 6);

    quad_coordinates = vec2(
        bool(vertex_in_tri & 1) ? -1.0 : 1.0,
        bool(vertex_in_tri & 2) ? -1.0 : 1.0
    );

    vec3 position = uniforms.inv_view_mx *
        (uniforms.sphere_radius * vec3(
           quad_coordinates,
            1.0
        ));

    gl_Position = uniforms.world_mx * vec4(
        position + atom_buffer[particle_index].pos,
        1.0
    );
}
