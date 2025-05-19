// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

#import bevy_pbr::{
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}

struct GlobalTransform {
    matrix: mat4x4<f32>,
}

struct ElementProperties {
    color: vec3<f32>,
    radius: f32,
}

struct PeriodicTable {
    // Elemental identity is low 7 bits of kind, so max 128 elements
    elements: array<ElementProperties, 118>,
}

@group(0) @binding(1) var<uniform> global_transform: GlobalTransform;
@group(0) @binding(2) var<uniform> periodic_table: PeriodicTable;

struct BondVertexInput {
    @builtin(vertex_index) index: u32,
    @location(0) quad_position: vec3<f32>,
    @location(1) atom1_position: vec3<f32>,
    @location(2) atom1_kind: u32,
    @location(3) atom2_position: vec3<f32>,
    @location(4) atom2_kind: u32,
}

struct BondVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) atom1_center: vec3<f32>,
    @location(3) @interpolate(flat) atom1_radius: f32,
    @location(4) @interpolate(flat) atom2_center: vec3<f32>,
    @location(5) @interpolate(flat) atom2_radius: f32,
    @location(6) @interpolate(flat) bond_radius: f32,
    @location(7) @interpolate(flat) bond_color: vec3<f32>,
}

@vertex
fn vertex(vertex: BondVertexInput) -> BondVertexOutput {
    var out: BondVertexOutput;

    // Transform atom positions to world space
    let local_pos1 = vec4<f32>(vertex.atom1_position, 1.0);
    let local_pos2 = vec4<f32>(vertex.atom2_position, 1.0);
    let world_pos1 = global_transform.matrix * local_pos1;
    let world_pos2 = global_transform.matrix * local_pos2;

    let atom1_center = world_pos1.xyz;
    let atom2_center = world_pos2.xyz;

    // Get element properties
    let element_id1 = vertex.atom1_kind & 0x7Fu;
    let element_id2 = vertex.atom2_kind & 0x7Fu;
    let element1 = periodic_table.elements[element_id1];
    let element2 = periodic_table.elements[element_id2];
    let atom1_radius = element1.radius;
    let atom2_radius = element2.radius;

    // Use smaller atom radius * 0.4 for bond thickness
    let bond_radius = min(atom1_radius, atom2_radius) * 0.4;

    // Calculate bond center and direction
    let bond_center = (atom1_center + atom2_center) * 0.5;
    let bond_dir = normalize(atom2_center - atom1_center);
    let bond_length = distance(atom1_center, atom2_center);

    // Get camera right and up vectors in world space
    let camera_forward = normalize(view.world_position - bond_center);
    let right = normalize(cross(bond_dir, camera_forward));
    let up = normalize(cross(right, bond_dir));

    // Create a quad large enough to encapsulate the full bond
    // We scale the quad to be a bit larger than the bond length to ensure coverage
    let quad_width = bond_radius * 2.5; // Extra width for antialiasing
    let quad_height = bond_length * 1.1;

    // Calculate billboard points
    let billboard_pos = bond_center +
        vertex.quad_position.x * right * quad_width +
        vertex.quad_position.y * bond_dir * quad_height * 0.5;

    let clip_position = position_world_to_clip(billboard_pos);

    // Calculate UVs in range [-1,1]
    out.uv = vec2<f32>(vertex.quad_position.x * 2.0, vertex.quad_position.y * 2.0);

    // Blend the colors of the two atoms for the bond color
    out.bond_color = (element1.color + element2.color) * 0.5;

    out.position = clip_position;
    out.world_position = billboard_pos;
    out.atom1_center = atom1_center;
    out.atom2_center = atom2_center;
    out.bond_radius = bond_radius;
    out.atom1_radius = atom1_radius;
    out.atom2_radius = atom2_radius;

    return out;
}

struct FragmentOutput {
    @builtin(frag_depth) depth: f32,
    @location(0) color: vec4<f32>,
}

@fragment
fn fragment(in: BondVertexOutput) -> FragmentOutput {
    // Calculate local bond axis coordinates
    let p = in.uv;

    // Calculate signed distance to capsule
    let capsule_dist = sdCapsule(p, vec2<f32>(0.0, -1.0), vec2<f32>(0.0, 1.0), 0.5);

    // Discard fragments outside the capsule with a small antialiasing border
    if capsule_dist > 0.0 {
        discard;
    }

    // Ray origin (camera position)
    let ray_origin = view.world_position;
    let ray_dir = normalize(in.world_position - ray_origin);

    // Bond axis in world space
    let bond_start = in.atom1_center;
    let bond_end = in.atom2_center;
    let bond_dir = normalize(bond_end - bond_start);

    // Calculate closest point to camera ray on the bond axis using an approximation
    // We use the UV y-coordinate (-1 to 1) to interpolate between atom centers
    let t = (in.uv.y + 1.0) * 0.5; // Map from [-1,1] to [0,1]
    let closest_point = mix(bond_start, bond_end, t);

    // Calculate point on surface of the bond cylinder
    // (cylinder radius * normalized distance from center)
    let radial_offset = -capsule_dist * in.bond_radius;

    // We need to calculate appropriate depth adjustment based on cylinder surface
    // Use view.clip_from_view to properly project the depth
    let view_pos = view.view_from_world * vec4<f32>(in.world_position, 1.0);
    var adjusted_view_pos = view_pos;
    adjusted_view_pos.z += radial_offset * 0.5; // Adjust depth by scaled radial offset

    let clip_pos = view.clip_from_view * adjusted_view_pos;
    let depth = clip_pos.z / clip_pos.w;

    // Calculate lighting - simple gradient from center outward
    let edge_factor = smoothstep(-0.1, 0.0, capsule_dist);
    let light_factor = 1.0 - edge_factor * 0.3;

    // Apply lighting to bond color
    var out: FragmentOutput;
    out.depth = depth;
    out.color = vec4<f32>(in.bond_color * light_factor, 1.0);
    return out;
}

// Signed distance function for 2D capsule
fn sdCapsule(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - r;
}

fn map(value: f32, low1: f32, high1: f32, low2: f32, high2: f32) -> f32 {
    return low2 + (value - low1) * (high2 - low2) / (high1 - low1);
}

// End of File
