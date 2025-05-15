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
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) atom_center: vec3<f32>,
    @location(2) atom_radius: f32,
    @location(3) quad_coord: vec2<f32>,
    @location(4) color: vec3<f32>,
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

    out.clip_position = position_world_to_clip(world_position);
    out.world_position = world_position;
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

    let hit_point = ray_origin + t * ray_dir;
    let normal = normalize(hit_point - in.atom_center);

    // Calculate proper depth
    let clip_pos = position_world_to_clip(hit_point);
    let depth = clip_pos.z / clip_pos.w;

    // Improved lighting with rim lighting
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let view_dir = normalize(ray_origin - hit_point);

    let n_dot_l = dot(normal, light_dir);
    let diffuse = max(n_dot_l, 0.0);

    // Rim lighting for better sphere definition
    let rim_power = 1.0 - max(dot(normal, view_dir), 0.0);
    let rim = pow(rim_power, 3.0) * 0.5;

    // Simple specular (Blinn-Phong)
    let half_dir = normalize(light_dir + view_dir);
    let specular = pow(max(dot(normal, half_dir), 0.0), 32.0) * 0.5;

    // Ambient occlusion approximation based on screen position
    let ao = 1.0 - (dist_sq * 0.2);

    // Color based on normal (for debugging) or sphere position
    var base_color = in.color;

    // Combine lighting
    let ambient = 0.15;
    let lighting = (ambient + diffuse * ao) * base_color + specular + rim * 0.3;

    var out: FragmentOutput;
    out.depth = depth;
    out.color = vec4<f32>(lighting, 1.0);
    return out;
}

// End of File
