//! Phase 4 of `doc/design_xray_node.md` — vertex-layout guard for the merged
//! transparent impostor mesh.
//!
//! `TransparentImpostorVertex::desc()` hand-writes the attribute offsets, which
//! is exactly the kind of thing that silently drifts when a field is added or
//! reordered. This GPU-free test asserts the declared `desc()` offsets and the
//! array stride agree with the real `#[repr(C)]` field layout (via
//! `offset_of!`), and that every field is covered by exactly one attribute with
//! the right shader location and format. It also exercises the CPU mesh's
//! `add_atom_quad` / `add_bond_quad` bookkeeping (4 vertices + 6 indices + 1
//! sort center per quad).

use rust_lib_flutter_cad::renderer::transparent_impostor_mesh::{
    TransparentImpostorMesh, TransparentImpostorVertex,
};
use std::mem::offset_of;

/// Every attribute the layout must declare: (shader_location, byte offset,
/// format). Offsets come from `offset_of!` so the expectation tracks the actual
/// struct layout rather than repeating the hand-written numbers.
fn expected_attributes() -> Vec<(u32, u64, wgpu::VertexFormat)> {
    vec![
        (
            0,
            offset_of!(TransparentImpostorVertex, kind) as u64,
            wgpu::VertexFormat::Uint32,
        ),
        (
            1,
            offset_of!(TransparentImpostorVertex, position_a) as u64,
            wgpu::VertexFormat::Float32x3,
        ),
        (
            2,
            offset_of!(TransparentImpostorVertex, position_b) as u64,
            wgpu::VertexFormat::Float32x3,
        ),
        (
            3,
            offset_of!(TransparentImpostorVertex, quad_offset) as u64,
            wgpu::VertexFormat::Float32x2,
        ),
        (
            4,
            offset_of!(TransparentImpostorVertex, radius) as u64,
            wgpu::VertexFormat::Float32,
        ),
        (
            5,
            offset_of!(TransparentImpostorVertex, color) as u64,
            wgpu::VertexFormat::Float32x3,
        ),
        (
            6,
            offset_of!(TransparentImpostorVertex, alpha) as u64,
            wgpu::VertexFormat::Float32,
        ),
        (
            7,
            offset_of!(TransparentImpostorVertex, roughness) as u64,
            wgpu::VertexFormat::Float32,
        ),
        (
            8,
            offset_of!(TransparentImpostorVertex, metallic) as u64,
            wgpu::VertexFormat::Float32,
        ),
        (
            9,
            offset_of!(TransparentImpostorVertex, rim_color) as u64,
            wgpu::VertexFormat::Float32x4,
        ),
    ]
}

/// The struct is 20 tightly-packed 4-byte fields = 80 bytes, and the vertex
/// buffer stride must match `size_of`.
#[test]
fn vertex_size_and_stride_agree() {
    assert_eq!(
        std::mem::size_of::<TransparentImpostorVertex>(),
        80,
        "20 4-byte fields = 80 bytes; update the shader + desc() if this changes"
    );
    let layout = TransparentImpostorVertex::desc();
    assert_eq!(
        layout.array_stride,
        std::mem::size_of::<TransparentImpostorVertex>() as u64,
        "array_stride must equal the vertex size"
    );
    assert_eq!(layout.step_mode, wgpu::VertexStepMode::Vertex);
}

/// Each declared `desc()` attribute must match the real field offset, in the
/// right location, with the right format — the classic hand-written-offset slip.
#[test]
fn desc_offsets_match_field_layout() {
    let layout = TransparentImpostorVertex::desc();
    let expected = expected_attributes();

    assert_eq!(
        layout.attributes.len(),
        expected.len(),
        "attribute count drifted from the field list"
    );

    for (attr, (loc, offset, format)) in layout.attributes.iter().zip(expected.iter()) {
        assert_eq!(attr.shader_location, *loc, "shader_location mismatch");
        assert_eq!(
            attr.offset, *offset,
            "offset for shader_location {} does not match the struct field",
            loc
        );
        assert_eq!(
            attr.format, *format,
            "format for shader_location {} is wrong",
            loc
        );
    }
}

/// Every field is covered by exactly one attribute (no gaps, no duplicate
/// locations), and the last attribute plus its size lands within the stride.
#[test]
fn attributes_cover_the_struct_without_gaps() {
    let layout = TransparentImpostorVertex::desc();

    // Locations are 0..=9, unique and contiguous.
    let mut locations: Vec<u32> = layout
        .attributes
        .iter()
        .map(|a| a.shader_location)
        .collect();
    locations.sort_unstable();
    assert_eq!(locations, (0..10).collect::<Vec<_>>());

    // No attribute runs past the stride.
    for attr in layout.attributes.iter() {
        let size = attr.format.size();
        assert!(
            attr.offset + size <= layout.array_stride,
            "attribute at offset {} (+{} bytes) overruns stride {}",
            attr.offset,
            size,
            layout.array_stride
        );
    }
}

/// `add_atom_quad` emits 4 vertices, 6 indices, and one sort center at the atom
/// center; the vertices all carry `kind == 0` and the supplied alpha.
#[test]
fn add_atom_quad_bookkeeping() {
    let mut mesh = TransparentImpostorMesh::new();
    let center = glam::f32::Vec3::new(1.0, 2.0, 3.0);
    mesh.add_atom_quad(
        &center,
        0.7,
        &[0.1, 0.2, 0.3],
        0.4,
        0.0,
        &[1.0, 1.0, 1.0, 1.0],
        0.5,
    );

    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.indices.len(), 6);
    assert_eq!(mesh.quad_centers.len(), 1);
    assert_eq!(mesh.quad_centers[0], center);
    for v in &mesh.vertices {
        assert_eq!(v.kind, 0);
        assert!((v.alpha - 0.5).abs() < 1e-6);
    }
    // Winding: 0-1-2 / 0-2-3 (base index 0).
    assert_eq!(mesh.indices, vec![0, 1, 2, 2, 3, 0]);
}

/// `add_bond_quad` emits `kind == 1` vertices and records the segment midpoint.
#[test]
fn add_bond_quad_records_midpoint() {
    let mut mesh = TransparentImpostorMesh::new();
    let start = glam::f32::Vec3::new(-2.0, 0.0, 0.0);
    let end = glam::f32::Vec3::new(4.0, 0.0, 0.0);
    mesh.add_bond_quad(&start, &end, 0.2, &[0.5, 0.5, 0.5], 0.3);

    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.quad_centers.len(), 1);
    assert_eq!(mesh.quad_centers[0], glam::f32::Vec3::new(1.0, 0.0, 0.0));
    for v in &mesh.vertices {
        assert_eq!(v.kind, 1);
        // Bonds carry no atom-only appearance.
        assert_eq!(v.roughness, 0.0);
        assert_eq!(v.metallic, 0.0);
        assert_eq!(v.rim_color, [0.0, 0.0, 0.0, 0.0]);
    }
}
