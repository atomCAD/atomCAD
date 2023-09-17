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
    midpoint: vec3<f32>,
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
    // Each vertex actually belongs to a block of six: two triangles are drawn for
    // each bond, three vertices are designated per triangle. We get the bond index:
    let bond_idx = in.index / 6u;

    // Now, we process and decode texture data. 2D textures let us store more data
    // than a 1D texture. We use a texture rather than a plain buffer for portability.
    // This bitwise work just cuts the bond index into its most and least significant
    // halves. The max texture size is 2048, which is 2^11. We use the MS and LS half
    // as ordinates:
    //                       bottom 11 bits          top 11 bits
    let coord = vec2<u32>(bond_idx & 0x000007ffu, bond_idx >> 11u);

    // With that coordinate, we now get the data from each struct
    let a1_index = textureLoad(bonds_a1, coord, 0).x;
    let a2_index = textureLoad(bonds_a2, coord, 0).x;
    let bond_order = textureLoad(bonds_order, coord, 0).x;

    let a1_coord = vec2<u32>(a1_index & 0x000007ffu, a1_index >> 11u);
    let a2_coord = vec2<u32>(a2_index & 0x000007ffu, a2_index >> 11u);

    // Bonds only store atom indexes. We look up the atom coords:
    let a1_pos = textureLoad(atoms_pos, a1_coord, 0).xyz;
    let a2_pos = textureLoad(atoms_pos, a2_coord, 0).xyz;

    let bond = Bond(a1_pos, a2_pos, bond_order);

    let part_fragment_transform = mat4x4<f32>(
        in.part_fragment_transform_0,
        in.part_fragment_transform_1,
        in.part_fragment_transform_2,
        in.part_fragment_transform_3
    );

    // Currently the bond rendering is done very sloppily. Rather than rendering the
    // bond in proper 3D, bonds are drawn in a plane parallel to the screen at a depth
    // halfway between the endpoint atoms. This should be fixed when someone gets the
    // time to.

    // To begin, we construct a rectangle in screen space. We want it to look like it
    // connects the atoms, so we make it as wide as the distance between the final
    // rendered coordinates of the endpoint atoms on the screen. ss = screen space
    let start_pos_ss = (camera.view * part_fragment_transform * vec4<f32>(bond.start_pos.xyz, 1.0)).xy;
    let end_pos_ss = (camera.view * part_fragment_transform * vec4<f32>(bond.end_pos.xyz, 1.0)).xy;
    let displacement_ss = start_pos_ss - end_pos_ss;
    let apparent_bond_length = length(displacement_ss);

    // We first imagine constructing a square. This means mapping from our vertex index
    // to 45º + 90º * which_corner_we're_at. This is nontrivial, because two corners
    // will have two vertices and two corners will just get one - the corners where
    // the triangles meet are double counted.
    //
    // Branchless equivalent of:
    // var angle = pi / 2.0 * (0.5 + f32(in.index % 3u))
    // if in.index % 6u >= 3u {
    //     angle += pi;
    // }
    let square_angle = pi * ((0.5 + f32(in.index % 3u)) / 2.0 + f32((in.index % 6u) / 3u));

    // Makes a rectangle - this looks like a weird use of sin and cos but we care about the
    // end-to-end length, not the length of the diagonal! aa = axis-aligned
    let aa_vertex = vec2(0.5 * apparent_bond_length * sign(cos(square_angle)), 0.5 * bar_width * sign(sin(square_angle)));

    // Now we want to rotate the rectangle so it is parallel to the on-screen bond
    // displacement:
    let screen_angle = atan2(displacement_ss.y, displacement_ss.x);

    // Now we rotate it to the correct angle (this is a rotation matrix in longform)
    var vertex = aa_vertex;
    let csa = cos(screen_angle);
    let ssa = sin(screen_angle);
    vertex.x = csa * aa_vertex.x - ssa * aa_vertex.y;
    vertex.y = ssa * aa_vertex.x + csa * aa_vertex.y;
    
    let midpoint = (bond.start_pos + bond.end_pos).xyz / 2.0;
    
    // ws = worldspace
    let camera_right_ws = vec3<f32>(camera.view[0][0], camera.view[1][0], camera.view[2][0]);
    let camera_up_ws = vec3<f32>(camera.view[0][1], camera.view[1][1], camera.view[2][1]);
    let position_ws =
        (part_fragment_transform * vec4<f32>(midpoint, 1.0))
        + vec4<f32>(
            vertex.x * camera_right_ws +
            vertex.y * camera_up_ws,
            0.0
        );

    let position_clip_space = camera.projection_view * position_ws;
    let center_view_space = camera.view * vec4<f32>(midpoint, 0.0);
    let position_view_space = camera.view * position_ws;

    // This creates our uv - a coordinate for each vertex on the square ranging from 0 to 1.
    // We want this so that we have a normalized value for "how far along the bond is this
    // pixel" later - we need that for shading.
    let uv = vec2(sign(cos(square_angle)) + 1.0, sign(sin(square_angle)) + 1.0) / 2.0;

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
    // This adds curvature, but we don't really want it because the bond floats
    // in space between the atoms. So only when the atoms happen to lie in a plane
    // parallel to the screen will this render properly
    // let z = sqrt(bar_width * bar_width - in.uv.y * in.uv.y);
    // let in_pos_clipspace = in.position_clip_space + camera.projection[2] * z;
    let in_pos_clipspace = in.position_clip_space;
    let depth = in_pos_clipspace.z / in_pos_clipspace.w;

    let normal = vec4(normalize(in.position_view_space.xyz - in.center_view_space.xyz), 0.0);
    let color = vec3(1.0, 1.0, 1.0);
    let brightness = sin(in.uv.y * pi * (2.0 * f32(in.order) - 1.0));

    if brightness < 0.0 {
        discard;
    }

    return BondFragmentOutput(depth, vec4(color * brightness, 1.0), normal);
}
