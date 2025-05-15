// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

#import bevy_pbr::{
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}

struct ElementProperties {
    color: vec3<f32>,
    radius: f32,
}

struct PeriodicTable {
    // Elemental identity is low 7 bits of kind, so max 128 elements
    elements: array<ElementProperties, 118>,
}

@group(0) @binding(1) var<uniform> periodic_table: PeriodicTable;

struct AtomVertexInput {
    @builtin(vertex_index) index: u32,
    @location(0) quad_position: vec3<f32>,
    @location(1) atom_position: vec3<f32>,
    @location(2) atom_kind: u32,
}

struct AtomVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) quad_coord: vec2<f32>,
    @location(2) @interpolate(flat) atom_center: vec3<f32>,
    @location(3) @interpolate(flat) atom_radius: f32,
    @location(4) @interpolate(flat) color: vec3<f32>,
    @location(5) clip_position: vec4<f32>,
}

@vertex
fn vertex(vertex: AtomVertexInput) -> AtomVertexOutput {
    var out: AtomVertexOutput;

    let atom_center = vertex.atom_position;
    let element_id = vertex.atom_kind & 0x7Fu; // Extract low 7 bits
    let element = periodic_table.elements[element_id];
    let atom_radius = element.radius;

    // Get camera right and up vectors in world space
    // Note: view_from_world transforms from world to view space,
    // so we need to use the transpose for world space directions
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;

    // Scale by radius and create billboard
    let world_offset = (vertex.quad_position.x * right + vertex.quad_position.y * up) * atom_radius;
    let world_position = atom_center + world_offset;

    let clip_position = position_world_to_clip(world_position);

    out.position = clip_position;
    out.world_position = world_position;
    out.clip_position = clip_position;
    out.atom_center = atom_center;
    out.atom_radius = atom_radius;
    out.quad_coord = vertex.quad_position.xy;
    out.color = element.color;
    return out;
}

struct FragmentOutput {
    @builtin(frag_depth) depth: f32,
    @location(0) color: vec4<f32>,
}

@fragment
fn fragment(in: AtomVertexOutput) -> FragmentOutput {
    // Early discard for fragments outside the circle
    let dist_sq = dot(in.quad_coord, in.quad_coord);
    if dist_sq > 1.0 {
        discard;
    }

    // Ray-sphere intersection using view space for better precision
    let ray_origin = view.world_position;
    let ray_dir = normalize(in.world_position - ray_origin);

    // Optimized ray-sphere intersection
    let oc = in.atom_center - ray_origin;
    let tca = dot(oc, ray_dir);

    // Early out if sphere is behind camera
    if tca < 0.0 {
        discard;
    }

    let d2 = dot(oc, oc) - tca * tca;
    let radius2 = in.atom_radius * in.atom_radius;

    if d2 > radius2 {
        discard;
    }

    // Calculate intersection
    let thc = sqrt(radius2 - d2);
    let t0 = tca - thc;
    let t1 = tca + thc;

    // Use the nearest positive intersection
    let t = select(t1, t0, t0 > 0.0);
    if t < 0.0 {
        discard;
    }

    // Calculate the sphere surface z-offset (in quad space)
    let z_normalized = sqrt(1.0 - dist_sq);
    let z_offset = z_normalized * in.atom_radius;

    // Adjust clip position using projection matrix
    let proj = view.clip_from_view;
    let adjusted_clip_pos = in.clip_position + proj[2] * z_offset;
    let depth = adjusted_clip_pos.z / adjusted_clip_pos.w;

    let hit_point = ray_origin + t * ray_dir;
    let normal = normalize(hit_point - in.atom_center);

    // Simple lighting
    let brightness = map(z_normalized, 0.0, 1.0, 0.25, 1.0);

    var out: FragmentOutput;
    out.depth = depth;
    out.color = vec4<f32>(in.color * brightness, 1.0);
    return out;
}

fn map(value: f32, low1: f32, high1: f32, low2: f32, high2: f32) -> f32 {
    return low2 + (value - low1) * (high2 - low2) / (high1 - low1);
}

// End of File
