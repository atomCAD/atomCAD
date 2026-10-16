//! Vertex-layout guard for the opaque atom and bond impostors
//! (`doc/design_depth_perception.md` Testing layer 1).
//!
//! Phase 4 appended a `Snorm8x4` exposure to `AtomImpostorVertex` and two to
//! `BondImpostorVertex`. Their `desc()` offsets are written by hand, and a
//! wrong one shows up only as a pipeline-creation panic in the GPU harness,
//! which CI does not run. So each attribute's offset is checked against the
//! field's `offset_of!`, and the stride against `size_of`.

use atomcad_renderer::atom_impostor_mesh::{AtomImpostorMesh, AtomImpostorVertex, FULLY_EXPOSED};
use atomcad_renderer::bond_impostor_mesh::{BondImpostorMesh, BondImpostorVertex};
use glam::f32::Vec3;
use std::mem::{offset_of, size_of};

fn assert_layout(
    layout: wgpu::VertexBufferLayout<'static>,
    stride: usize,
    expected: &[(u32, usize, wgpu::VertexFormat)],
) {
    assert_eq!(layout.array_stride, stride as u64, "stride");
    assert_eq!(layout.attributes.len(), expected.len(), "attribute count");
    for (location, offset, format) in expected {
        let attribute = layout
            .attributes
            .iter()
            .find(|a| a.shader_location == *location)
            .unwrap_or_else(|| panic!("no attribute at location {location}"));
        assert_eq!(attribute.offset, *offset as u64, "location {location}");
        assert_eq!(attribute.format, *format, "location {location}");
    }
    // Every byte of the vertex is covered exactly once, in order.
    let mut attributes: Vec<_> = layout.attributes.to_vec();
    attributes.sort_by_key(|a| a.offset);
    let mut end = 0;
    for attribute in &attributes {
        assert_eq!(
            attribute.offset, end,
            "a gap or an overlap before {attribute:?}"
        );
        end += attribute.format.size();
    }
    assert_eq!(end, stride as u64, "the attributes do not reach the stride");
}

#[test]
fn the_atom_impostor_layout_matches_the_struct() {
    use wgpu::VertexFormat::*;
    assert_eq!(size_of::<AtomImpostorVertex>(), 64);
    assert_layout(
        AtomImpostorVertex::desc(),
        size_of::<AtomImpostorVertex>(),
        &[
            (
                0,
                offset_of!(AtomImpostorVertex, center_position),
                Float32x3,
            ),
            (1, offset_of!(AtomImpostorVertex, quad_offset), Float32x2),
            (2, offset_of!(AtomImpostorVertex, radius), Float32),
            (3, offset_of!(AtomImpostorVertex, albedo), Float32x3),
            (4, offset_of!(AtomImpostorVertex, roughness), Float32),
            (5, offset_of!(AtomImpostorVertex, metallic), Float32),
            (6, offset_of!(AtomImpostorVertex, rim_color), Float32x4),
            (7, offset_of!(AtomImpostorVertex, exposure), Snorm8x4),
        ],
    );
}

#[test]
fn the_bond_impostor_layout_matches_the_struct() {
    use wgpu::VertexFormat::*;
    assert_eq!(size_of::<BondImpostorVertex>(), 56);
    assert_layout(
        BondImpostorVertex::desc(),
        size_of::<BondImpostorVertex>(),
        &[
            (0, offset_of!(BondImpostorVertex, start_position), Float32x3),
            (1, offset_of!(BondImpostorVertex, end_position), Float32x3),
            (2, offset_of!(BondImpostorVertex, quad_offset), Float32x2),
            (3, offset_of!(BondImpostorVertex, radius), Float32),
            (4, offset_of!(BondImpostorVertex, color), Float32x3),
            (5, offset_of!(BondImpostorVertex, exposure_start), Snorm8x4),
            (6, offset_of!(BondImpostorVertex, exposure_end), Snorm8x4),
        ],
    );
}

#[test]
fn the_plain_producers_write_fully_exposed_vertices() {
    let mut atoms = AtomImpostorMesh::new();
    atoms.add_atom_quad(&Vec3::ZERO, 1.0, &[1.0; 3], 0.5, 0.0, &[0.0; 4]);
    assert!(atoms.vertices.iter().all(|v| v.exposure == FULLY_EXPOSED));

    let mut bonds = BondImpostorMesh::new();
    bonds.add_bond_quad(&Vec3::ZERO, &Vec3::X, 0.2, &[1.0; 3]);
    assert!(
        bonds
            .vertices
            .iter()
            .all(|v| v.exposure_start == FULLY_EXPOSED && v.exposure_end == FULLY_EXPOSED)
    );
}

#[test]
fn the_exposed_producers_write_their_values_on_every_corner() {
    let packed = [10, -20, 30, 64];
    let mut atoms = AtomImpostorMesh::new();
    atoms.add_atom_quad_with_exposure(&Vec3::ZERO, 1.0, &[1.0; 3], 0.5, 0.0, &[0.0; 4], packed);
    assert_eq!(atoms.vertices.len(), 4);
    assert!(atoms.vertices.iter().all(|v| v.exposure == packed));

    let end = [0, 0, 0, 5];
    let mut bonds = BondImpostorMesh::new();
    bonds.add_bond_quad_with_exposure(&Vec3::ZERO, &Vec3::X, 0.2, &[1.0; 3], [packed, end]);
    assert_eq!(bonds.vertices.len(), 4);
    assert!(
        bonds
            .vertices
            .iter()
            .all(|v| v.exposure_start == packed && v.exposure_end == end)
    );
}
