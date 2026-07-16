//! Phase 3 of `doc/design_atom_labels.md` — vertex-layout guard for the atom
//! label mesh.
//!
//! `LabelVertex::desc()` hand-writes its attribute offsets as cumulative
//! `size_of::<[f32; N]>()` expressions — the same fragile idiom
//! `TransparentImpostorVertex` uses, and this test is the only thing standing
//! between a miscount and silently garbled glyphs. The offsets come from
//! `offset_of!` so the expectation tracks the real `#[repr(C)]` layout rather
//! than repeating the hand-written numbers.
//!
//! Also covers `add_glyph_quad`'s bookkeeping, including the deliberate
//! y-crossing between plane offsets (y up) and atlas UVs (y down).

use glam::f32::Vec3;
use rust_lib_flutter_cad::renderer::label_mesh::{LabelMesh, LabelVertex};
use std::mem::offset_of;

/// Every attribute the layout must declare: (shader_location, byte offset,
/// format).
fn expected_attributes() -> Vec<(u32, u64, wgpu::VertexFormat)> {
    vec![
        (
            0,
            offset_of!(LabelVertex, anchor_position) as u64,
            wgpu::VertexFormat::Float32x3,
        ),
        (
            1,
            offset_of!(LabelVertex, plane_offset) as u64,
            wgpu::VertexFormat::Float32x2,
        ),
        (
            2,
            offset_of!(LabelVertex, depth_offset) as u64,
            wgpu::VertexFormat::Float32,
        ),
        (
            3,
            offset_of!(LabelVertex, glyph_uv) as u64,
            wgpu::VertexFormat::Float32x2,
        ),
    ]
}

/// The hand-written `desc()` offsets agree with the real struct layout, and each
/// shader location gets the format the shader declares.
#[test]
fn label_vertex_desc_matches_repr_c_layout() {
    let desc = LabelVertex::desc();
    let actual: Vec<(u32, u64, wgpu::VertexFormat)> = desc
        .attributes
        .iter()
        .map(|a| (a.shader_location, a.offset, a.format))
        .collect();

    assert_eq!(
        actual,
        expected_attributes(),
        "LabelVertex::desc() drifted from the #[repr(C)] field layout"
    );
}

/// The stride is the struct size — 8 f32, no padding.
#[test]
fn label_vertex_stride_is_the_struct_size() {
    assert_eq!(
        LabelVertex::desc().array_stride,
        std::mem::size_of::<LabelVertex>() as u64
    );
    assert_eq!(std::mem::size_of::<LabelVertex>(), 8 * 4);
}

/// The vertex step mode is per-vertex (each corner carries its own UV).
#[test]
fn label_vertex_step_mode_is_per_vertex() {
    assert_eq!(LabelVertex::desc().step_mode, wgpu::VertexStepMode::Vertex);
}

/// One glyph = 4 vertices + 6 indices, all sharing the anchor and depth offset.
#[test]
fn add_glyph_quad_emits_one_quad() {
    let mut mesh = LabelMesh::new();
    let anchor = Vec3::new(1.0, 2.0, 3.0);
    let base = mesh.add_glyph_quad(
        &anchor,
        [-0.5, -0.25],
        [0.5, 0.75],
        [0.1, 0.2],
        [0.3, 0.4],
        1.75,
    );

    assert_eq!(base, 0);
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.indices, vec![0, 1, 2, 2, 3, 0]);

    for v in &mesh.vertices {
        assert_eq!(v.anchor_position, [1.0, 2.0, 3.0]);
        assert_eq!(v.depth_offset, 1.75);
    }
}

/// Corner order is bottom-left, bottom-right, top-right, top-left — and each
/// corner's UV is crossed on the vertical axis, because plane offsets are y up
/// while atlas UVs are y down. Getting this wrong renders every glyph upside
/// down, which no layout test would catch.
#[test]
fn add_glyph_quad_pairs_corners_with_vertically_flipped_uvs() {
    let mut mesh = LabelMesh::new();
    mesh.add_glyph_quad(
        &Vec3::ZERO,
        [-1.0, -2.0],
        [3.0, 4.0],
        [0.1, 0.2], // uv of the cell's TOP-left
        [0.3, 0.4], // uv of the cell's BOTTOM-right
        0.0,
    );

    let corners: Vec<([f32; 2], [f32; 2])> = mesh
        .vertices
        .iter()
        .map(|v| (v.plane_offset, v.glyph_uv))
        .collect();

    assert_eq!(
        corners,
        vec![
            ([-1.0, -2.0], [0.1, 0.4]), // bottom-left  → uv_min.x, uv_max.y
            ([3.0, -2.0], [0.3, 0.4]),  // bottom-right → uv_max.x, uv_max.y
            ([3.0, 4.0], [0.3, 0.2]),   // top-right    → uv_max.x, uv_min.y
            ([-1.0, 4.0], [0.1, 0.2]),  // top-left     → uv_min.x, uv_min.y
        ]
    );
}

/// Successive quads chain their base indices rather than restarting.
#[test]
fn add_glyph_quad_chains_base_indices() {
    let mut mesh = LabelMesh::new();
    for _ in 0..3 {
        mesh.add_glyph_quad(
            &Vec3::ZERO,
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 1.0],
            0.0,
        );
    }

    assert_eq!(mesh.vertices.len(), 12);
    assert_eq!(mesh.indices.len(), 18);
    assert_eq!(&mesh.indices[6..12], &[4, 5, 6, 6, 7, 4]);
}

/// A fresh mesh is empty, and `Default` agrees with `new`.
#[test]
fn new_label_mesh_is_empty() {
    let mesh = LabelMesh::default();
    assert!(mesh.vertices.is_empty());
    assert!(mesh.indices.is_empty());
    assert_eq!(mesh.memory_usage_bytes(), 0);
}
