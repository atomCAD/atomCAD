// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// This shader uses an interesting technique that is sometimes called
// progrmamable vertex pulling.
//
// Instead of providing a vertex buffer, we instead provide an array of points
// in a storage buffer and then call draw with <number of points> * 6
//
// This is apparently faster than instancing for small meshes.
//
// See slide 20 of
// https://www.slideshare.net/DevCentralAMD/vertex-shader-tricks-bill-bilodeau.

struct Camera {
    projection: mat4x4<f32>,
    view: mat4x4<f32>,
    projection_view: mat4x4<f32>,
};

struct Vertex {
    xy: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct Atom {
    pos: vec3<f32>,
    kind: u32,
};

struct Bond {
    start_pos: vec3<f32>,
    end_pos: vec3<f32>,
    order: u32,
};

@group(1) @binding(0)
var atoms_pos: texture_2d<f32>;

@group(2) @binding(0)
var bonds_a1: texture_2d<u32>;
@group(2) @binding(1)
var bonds_a2: texture_2d<u32>;
@group(2) @binding(2)
var bonds_order: texture_2d<u32>;

struct BondVertexInput {
    @builtin(vertex_index)
    index: u32,
    @location(0)
    part_fragment_transform_0: vec4<f32>,
    @location(1)
    part_fragment_transform_1: vec4<f32>,
    @location(2)
    part_fragment_transform_2: vec4<f32>,
    @location(3)
    part_fragment_transform_3: vec4<f32>,
};

struct BondVertexOutput {
    @builtin(position)
    position: vec4<f32>,
    @location(0)
    uv: vec2<f32>,
    @location(1)
    position_clip_space: vec4<f32>,
    @location(2)
    position_view_space: vec4<f32>,
    @location(3) @interpolate(flat)
    center_view_space: vec4<f32>,
    @location(4) @interpolate(flat)
    order: u32
};

const pi = 3.14159265359;
const bar_width = 0.8;

@vertex
fn vs_main(in: BondVertexInput) -> BondVertexOutput {
    let idx = in.index / 6u;
    let coord = vec2<u32>(idx & 0x000007ffu, idx >> 11u);
    let a1_index = textureLoad(bonds_a1, coord, 0).x;
    let a2_index = textureLoad(bonds_a2, coord, 0).x;
    let bond_order = textureLoad(bonds_order, coord, 0).x;

    let a1_coord = vec2<u32>(a1_index & 0x000007ffu, a1_index >> 11u);
    let a2_coord = vec2<u32>(a2_index & 0x000007ffu, a2_index >> 11u);

    let a1_pos = textureLoad(atoms_pos, a1_coord, 0).xyz;
    let a2_pos = textureLoad(atoms_pos, a2_coord, 0).xyz;

    let bond = Bond(a1_pos, a2_pos, bond_order);
    let part_fragment_transform = mat4x4<f32>(
        in.part_fragment_transform_0,
        in.part_fragment_transform_1,
        in.part_fragment_transform_2,
        in.part_fragment_transform_3
    );
    
    // Equivalent to:
    // var angle = pi / 2.0 * (0.5 + f32(in.index % 3u))
    // if in.index % 6u >= 3u {
    //     angle += pi;
    // }
    let angle = pi * ((0.5 + f32(in.index % 3u)) / 2.0 + f32((in.index % 6u) / 3u));
    let start_pos = (camera.view * part_fragment_transform * vec4<f32>(bond.start_pos.xyz, 1.0)).xy;
    let end_pos = (camera.view * part_fragment_transform * vec4<f32>(bond.end_pos.xyz, 1.0)).xy;
    let displacement = start_pos - end_pos;
    let length = length(displacement);
    let screen_angle = atan2(displacement.y, displacement.x);

    let uv = vec2(sign(cos(angle)) + 1.0, sign(sin(angle)) + 1.0) / 2.0;
    // make a rectangle - this looks like a weird use of sin and cos but we care about the
    // end-to-end length, not the length of the diagonal! This is the axis-aligned rectangle
    let aa_vertex = vec2(0.5 * length * sign(cos(angle)), 0.5 * bar_width * sign(sin(angle)));
    // Now we rotate it to the correct angle (this is a rotation matrix in longform)
    var vertex = aa_vertex;
    let csa = cos(screen_angle);
    let ssa = sin(screen_angle);
    vertex.x = csa * aa_vertex.x - ssa * aa_vertex.y;
    vertex.y = ssa * aa_vertex.x + csa * aa_vertex.y;
    
    let pos = (bond.start_pos + bond.end_pos).xyz / 2.0;
    let position = part_fragment_transform * vec4<f32>(pos, 1.0);

    let camera_right_worldspace = vec3<f32>(camera.view[0][0], camera.view[1][0], camera.view[2][0]);
    let camera_up_worldspace = vec3<f32>(camera.view[0][1], camera.view[1][1], camera.view[2][1]);
    let position_worldspace = vec4<f32>(
        position.xyz +
        vertex.x * camera_right_worldspace +
        vertex.y * camera_up_worldspace,
        1.0
    );

    let position_clip_space = camera.projection_view * position_worldspace;
    let center_view_space = camera.view * vec4<f32>(pos, 0.0);
    let position_view_space = camera.view * position_worldspace;

    return BondVertexOutput(position_clip_space, uv, position_clip_space, position_view_space, center_view_space, bond.order);
}

alias BondFragmentInput = BondVertexOutput;

struct BondFragmentOutput {
    @builtin(frag_depth)
    depth: f32,
    @location(0)
    color: vec4<f32>,
    @location(1)
    normal: vec4<f32>,
}

@fragment
fn fs_main(in: BondFragmentInput) -> BondFragmentOutput {
    let z = sqrt(bar_width * bar_width - in.uv.y * in.uv.y);
    let in_pos_clipspace = in.position_clip_space + camera.projection[2] * z;
    let depth = in_pos_clipspace.z / in_pos_clipspace.w;

    let normal = vec4(normalize(in.position_view_space.xyz - in.center_view_space.xyz), 0.0);
    let color = vec3(1.0, 1.0, 1.0);
    let brightness = sin(in.uv.y * pi * (2.0 * f32(in.order) - 1.0));

    if brightness < 0.0 {
        discard;
    }

    return BondFragmentOutput(depth, vec4(color * brightness, 1.0), normal);
}