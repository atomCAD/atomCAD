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
    mat4 projection_mx;
    mat3 inv_view_mx; // Effectively vec4[3]?
    uvec2 cursor;
} uniforms;

struct Atom {
    vec3 pos;
    uint kind;
};

/*
 * Will this work? The alignment of vec3 in arrays with std430
 * is quite hazy.
 */
layout(set = 0, binding = 1, std430) buffer Points {
    Atom atom_buffer[];
};

layout(location = 0) out vec2 uv;
layout(location = 1) out vec4 position_clip_space;
layout(location = 2) flat out vec3 color;
layout(location = 3) flat out uint id;

void main(void) {
    // Look into whether using triangles instead of quads is more efficient with very large quantities of atoms.
    uint particle_index = gl_VertexIndex / 6;
    uint vertex_in_tri = abs(3 - gl_VertexIndex % 6);

    id = particle_index;

    uv = vec2(
        bool(vertex_in_tri & 1) ? -1.0 : 1.0,
        bool(vertex_in_tri & 2) ? -1.0 : 1.0
    );

    vec3 position_objectspace = uniforms.inv_view_mx * vec3(uv, 1.0);
    
    Atom atom = atom_buffer[particle_index];

    if (atom.kind == 0)
        color = vec3(0.96, 0.26, 0.82);
    else
        color = vec3(0.23, 0.26, 0.82);

    vec4 position_worldspace = vec4(atom.pos + position_objectspace, 1.0);

    position_clip_space = uniforms.world_mx * position_worldspace;

    gl_Position = position_clip_space;

    // // color = atom.color;

    // object_quad_coordinates = position + atom.pos;

    // gl_Position = uniforms.world_mx * vec4(object_quad_coordinates, 1.0);
}
