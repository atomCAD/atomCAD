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

struct Atom {
    pos: vec3<f32>,
    kind: u32,
};

@group(0) @binding(0)
var<uniform> camera: Camera;
@group(0) @binding(1)
var<storage> vertices: array<vec2<f32>, 3>;
@group(0) @binding(2)
var<storage> periodic_table: PeriodicTable;

@group(1) @binding(0)
var<storage> fragment_id: vec2<u32>; // high and low
@group(1) @binding(1)
var<storage> atoms: array<Atom>;

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
    let atom = atoms[in.index / 3u];
    let element = periodic_table.elements[atom.kind & 0x7fu];
    let element_vec = vec4<f32>(element.color, element.radius);
    let vertex = element.radius * vertices[in.index % 3u];

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

// End of File
