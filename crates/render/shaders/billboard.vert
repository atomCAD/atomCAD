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

layout(set = 0, binding = 0) uniform Camera {
    mat4 projection;
    mat4 view;
    mat4 projection_view;
} camera;

struct Element {
    vec3 color;
    float radius;
};

layout(set = 0, binding = 1) readonly buffer PeriodicTable {
    Element elements[118];
} periodic_table;

struct Atom {
    vec3 pos;
    uint kind;
};

layout(set = 1, binding = 1, std430) readonly buffer Atoms {
    uvec2 fragment_id; // high and low

    Atom atoms[]; // this must be aligned to 16 bytes.
};

// struct Bivec {
//     float xy;
//     float xz;
//     float yz;
// }

// struct Rotor {
//     float s;
//     Bivec bv;
// };

layout(location = 0) in mat4 part_fragment_transform; // fragement * part transformation

layout(location = 0) out vec2 uv;
layout(location = 1) out vec4 position_clip_space;
layout(location = 2) flat out Element element;
layout(location = 4) flat out vec4 center_view_space;
layout(location = 5) out vec4 position_view_space;

const vec2 vertices[3] = {
    vec2(1.73, -1.0),
    vec2(-1.73, -1.0),
    vec2(0.0, 2.0)
};

// vec3 rotate_by_rotor(Rotor rotor, vec3 point) {
//     const float fx = rotor.s * point.x + rotor.bv.xy * point.y + rotor.bv.xz * point.z;
//     const float fy = rotor.s * point.y - rotor.bv.xy * point.x + rotor.bv.yz * point.z;
//     const float fz = rotor.s * point.z - rotor.bv.xz * point.x - rotor.bv.yz * point.y;
//     const float fw = rotor.bv.xy * point.z - rotor.bv.xz * point.y + rotor.bv.yz * point.x;

//     return vec3(
//         rotor.s * fx + rotor.bv.xy * fy + rotor.bv.xz * fz + rotor.bv.yz * fw,
//         rotor.s * fy - rotor.bv.xy * fx - rotor.bv.xz * fw + rotor.bv.yz * fz,
//         rotor.s * fz + rotor.bv.xy * fw - rotor.bv.xz * fx - rotor.bv.yz * fy,
//     );
// }

void main(void) {
    const Atom atom = atoms[gl_VertexIndex / 3];
    element = periodic_table.elements[atom.kind & 0x7f];
    const vec2 vertex = element.radius * vertices[gl_VertexIndex % 3];

    const vec4 position = part_fragment_transform * vec4(atom.pos, 1.0);

    const vec3 camera_right_worldspace = vec3(camera.view[0][0], camera.view[1][0], camera.view[2][0]);
    const vec3 camera_up_worldspace = vec3(camera.view[0][1], camera.view[1][1], camera.view[2][1]);
    const vec4 position_worldspace = vec4(
        position.xyz +
        vertex.x * camera_right_worldspace +
        vertex.y * camera_up_worldspace,
        1.0
    );

    position_clip_space = camera.projection_view * position_worldspace;
    // position_clip_space = camera.projection_view * position_worldspace;
    uv = vertex;
    // sphere_radius = element.radius;
    center_view_space = camera.view * vec4(atom.pos, 0.0);
    position_view_space = camera.view * position_worldspace;
    gl_Position = position_clip_space;
}
