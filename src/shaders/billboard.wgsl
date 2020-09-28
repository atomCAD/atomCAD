# This shader uses an interesting technique that is sometimes
# called progrmamable vertex pulling.
#
# Instead of providing a vertex buffer, we instead provide an array
# of points in a storage buffer and then call draw with <number of points> * 6
#
# This is apparently faster than instancing for small meshes.
# See slide 20 of https://www.slideshare.net/DevCentralAMD/vertex-shader-tricks-bill-bilodeau.
#
# THIS FILE CURRENTLY ISN'T USED, but will be once wgsl is more mature.

import "GLSL.std.450" as std;

# vertex shader
type Camera = struct {
    [[offset 0]] world_mx: mat4x4<f32>;
    [[offset 64]] projection_mx: mat4x4<f32>;
    [[offset 128]] inv_view_mx: mat3x3<f32>; # Effectively array<vec4<f32>, 3>
};

type Atom = struct {
    [[offset 0]] pos: vec3<f32>;
    [[offset 12]] kind: u32;
};
type Atoms = [[stride 16]] array<Atom>;

[[binding 0, set 0]] var<uniform> camera: Camera;
[[binding 1, set 0]] var<storage_buffer> atoms: Atoms;

[[location 0]] var<out> out_pos_clipspace: vec4<f32>;
[[location 1]] var<out> out_uv: vec2<f32>;
[[location 2, interpolate flat]] var<out> out_color: vec3<f32>;

[[builtin vertex_idx]] var<in> gl_vertex_index: i32;
[[builtin position]] var<out> gl_position: vec4<f32>;

const sphere_radius: f32 = 1.0;
const tri_coords: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
    vec2<f32>(1.73, -1.0),
    vec2<f32>(-1.73, -1.0),
    vec2<f32>(0.0, 2.0)
);

fn main_vert() -> void {
    var particle_index: u32 = gl_vertex_index / 3;
    var in_tri_index: u32 = gl_vertex_index % 3;

    out_uv = tri_coords[in_tri_index];

    var pos_objectspace: vec3<f32> = camera.inv_view_mx * (sphere_radius * vec3<f32>(out_uv, 1.0));

    var atom: Atom = atoms[particle_index];

    if (atom.kind == 0) {
        out_color = vec3<f32>(0.96, 0.26, 0.82);
    } else {
        out_color = vec3<f32>(0.23, 0.26, 0.82);
    }

    var pos_worldspace: vec3<f32> = vec4<f32>(atom.pos + pos_objectspace, 1.0);

    out_pos_clipspace = camera.world_mx * pos_worldspace;
    gl_position = out_pos_clipspace;
}
entry_point vertex as "main" = main_vert;

# fragment shader
[[location 0]] var<in> in_pos_clipspace: vec4<f32>;
[[location 1]] var<in> in_uv: vec2<f32>;
[[location 2, interpolate flat]] var<in> in_color: vec3<f32>;

[[location 0]] var<out> frag_color: vec4<f32>;
[[builtin frag_depth]] var<out> gl_frag_depth: f32;

fn main_frag() -> void {
    var dist: f32 = std::distance(in_uv);
    if (dist > sphere_radius) {
        discard;
    }

    var frag_pos_clip: vec4<f32> = in_pos_clipspace + uniforms.projection_mx[2] * (1.0 + std::sqrt(1 - dist * dist));
    
    gl_frag_depth = frag_pos_clip.z / frag_pos_clip.w;
    frag_color = vec4<f32>(in_color, 1.0);
}
entry_point fragment as "main" = main_frag;
