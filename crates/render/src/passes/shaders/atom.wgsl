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

struct Element {
    color: vec3<f32>,
    radius: f32,
};

struct PeriodicTable {
    elements: array<Element, 118>,
};

struct Vertex {
    xy: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;
@group(0) @binding(1)
var<uniform> periodic_table: PeriodicTable;
@group(0) @binding(2)
var<uniform> vertices: array<Vertex, 3>;

struct Atom {
    pos: vec3<f32>,
    kind: u32,
};

@group(1) @binding(0)
var atoms_pos: texture_2d<f32>;
@group(1) @binding(1)
var atoms_kind: texture_2d<u32>;

struct AtomVertexInput {
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

struct AtomVertexOutput {
    @builtin(position)
    position: vec4<f32>,
    @location(0)
    uv: vec2<f32>,
    @location(1)
    position_clip_space: vec4<f32>,
    @location(2) @interpolate(flat)
    element_vec: vec4<f32>,
    @location(4) @interpolate(flat)
    center_view_space: vec4<f32>,
    @location(5)
    position_view_space: vec4<f32>,
};

@vertex
fn vs_main(in: AtomVertexInput) -> AtomVertexOutput {
    let idx = in.index / 3u;
    let coord = vec2<u32>(idx & 0x000007ffu, idx >> 11u);
    let texel_pos = textureLoad(atoms_pos, coord, 0);
    let texel_kind = textureLoad(atoms_kind, coord, 0);
    let atom = Atom(texel_pos.xyz, texel_kind.x);
    let element = periodic_table.elements[atom.kind & 0x7fu];
    let element_vec = vec4<f32>(element.color, element.radius);
    let vertex = element.radius * vertices[in.index % 3u].xy;

    let part_fragment_transform = mat4x4<f32>(
        in.part_fragment_transform_0,
        in.part_fragment_transform_1,
        in.part_fragment_transform_2,
        in.part_fragment_transform_3
    );
    let position = part_fragment_transform * vec4<f32>(atom.pos, 1.0);

    let camera_right_worldspace = vec3<f32>(camera.view[0][0], camera.view[1][0], camera.view[2][0]);
    let camera_up_worldspace = vec3<f32>(camera.view[0][1], camera.view[1][1], camera.view[2][1]);
    let position_worldspace = vec4<f32>(
        position.xyz +
        vertex.x * camera_right_worldspace +
        vertex.y * camera_up_worldspace,
        1.0
    );

    let position_clip_space = camera.projection_view * position_worldspace;
    let center_view_space = camera.view * vec4<f32>(atom.pos, 0.0);
    let position_view_space = camera.view * position_worldspace;

    return AtomVertexOutput(position_clip_space, vertex, position_clip_space, element_vec, center_view_space, position_view_space);
}

alias AtomFragmentInput = AtomVertexOutput;

struct AtomFragmentOutput {
    @builtin(frag_depth)
    depth: f32,
    @location(0)
    color: vec4<f32>,
    @location(1)
    normal: vec4<f32>,
}

fn map(value: f32, low1: f32, high1: f32, low2: f32, high2: f32) -> f32 {
    return low2 + (value - low1) * (high2 - low2) / (high1 - low1);
}

@fragment
fn fs_main(in: AtomFragmentInput) -> AtomFragmentOutput {
    let element = Element(in.element_vec.xyz, in.element_vec.w);
    let dist = length(in.uv);
    if (dist > element.radius) {
        discard;
    }

    let z = sqrt(element.radius * element.radius - dist * dist);
    let in_pos_clipspace = in.position_clip_space + camera.projection[2] * z;

    let depth = in_pos_clipspace.z / in_pos_clipspace.w;

    let color = vec4(
        element.color * map(z, 0.0, element.radius, 0.25, 1.0),
        1.0
    );
    let normal = vec4(normalize(in.position_view_space.xyz - in.center_view_space.xyz), 0.0);

    return AtomFragmentOutput(depth, color, normal);
}

// End of File
