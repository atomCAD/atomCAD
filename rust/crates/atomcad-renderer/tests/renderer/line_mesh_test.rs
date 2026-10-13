// The instanced `LineMesh` and the blend state of the line pipeline
// (doc/design_screen_space_lines.md D1, D6, D7; Phase 2).

use atomcad_renderer::line_mesh::{LineDash, LineMesh, LineSegmentInstance, LineStyle};
use atomcad_renderer::renderer::STRAIGHT_ALPHA_BLENDING;
use glam::f32::Vec3;
use std::mem::offset_of;

#[test]
fn a_segment_instance_is_60_bytes() {
    assert_eq!(std::mem::size_of::<LineSegmentInstance>(), 60);
}

#[test]
fn the_instance_layout_matches_the_struct() {
    let desc = LineSegmentInstance::desc();
    assert_eq!(desc.array_stride, 60);
    assert_eq!(desc.step_mode, wgpu::VertexStepMode::Instance);
    let offsets: Vec<(u32, u64, wgpu::VertexFormat)> = desc
        .attributes
        .iter()
        .map(|a| (a.shader_location, a.offset, a.format))
        .collect();
    use wgpu::VertexFormat::*;
    assert_eq!(
        offsets,
        vec![
            (0, offset_of!(LineSegmentInstance, p0) as u64, Float32x3),
            (1, offset_of!(LineSegmentInstance, p1) as u64, Float32x3),
            (2, offset_of!(LineSegmentInstance, color) as u64, Float32x4),
            (3, offset_of!(LineSegmentInstance, width_px) as u64, Float32),
            (
                4,
                offset_of!(LineSegmentInstance, dash_on_px) as u64,
                Float32
            ),
            (
                5,
                offset_of!(LineSegmentInstance, dash_off_px) as u64,
                Float32
            ),
            (
                6,
                offset_of!(LineSegmentInstance, dash_phase) as u64,
                Float32
            ),
            (7, offset_of!(LineSegmentInstance, lod_slot) as u64, Uint32),
        ]
    );
}

#[test]
fn uniform_color_shortcut_is_a_solid_opaque_one_pixel_line() {
    let mut mesh = LineMesh::new();
    assert!(mesh.is_empty());
    mesh.add_line_with_uniform_color(
        &Vec3::new(1.0, 2.0, 3.0),
        &Vec3::new(4.0, 5.0, 6.0),
        &[0.25, 0.5, 0.75],
    );
    assert_eq!(mesh.len(), 1);
    assert_eq!(
        mesh.instances[0],
        LineSegmentInstance {
            p0: [1.0, 2.0, 3.0],
            p1: [4.0, 5.0, 6.0],
            color: [0.25, 0.5, 0.75, 1.0],
            width_px: 1.0,
            dash_on_px: 0.0,
            dash_off_px: 0.0,
            dash_phase: 0.0,
            lod_slot: 0,
        }
    );
}

#[test]
fn add_segment_writes_the_style() {
    let style = LineStyle::solid([1.0, 0.0, 0.0])
        .with_width(2.5)
        .with_alpha(0.5)
        .dashed(6.0, 4.0);
    assert_eq!(style.color, [1.0, 0.0, 0.0, 0.5]);
    assert_eq!(style.width_px, 2.5);
    assert_eq!(
        style.dash,
        Some(LineDash {
            on_px: 6.0,
            off_px: 4.0
        })
    );

    let mut mesh = LineMesh::new();
    mesh.add_segment(&Vec3::ZERO, &Vec3::X, &style);
    let segment = mesh.instances[0];
    assert_eq!(segment.color, [1.0, 0.0, 0.0, 0.5]);
    assert_eq!(segment.width_px, 2.5);
    assert_eq!((segment.dash_on_px, segment.dash_off_px), (6.0, 4.0));
    assert_eq!(segment.dash_phase, 0.0);
    assert_eq!(segment.lod_slot, 0);
}

#[test]
fn segments_yields_the_ends_in_emission_order() {
    let mut mesh = LineMesh::new();
    let style = LineStyle::solid([1.0, 1.0, 1.0]);
    mesh.add_segment(&Vec3::ZERO, &Vec3::X, &style);
    mesh.add_segment(&Vec3::Y, &Vec3::Z, &style);
    let segments: Vec<(Vec3, Vec3)> = mesh.segments().collect();
    assert_eq!(segments, vec![(Vec3::ZERO, Vec3::X), (Vec3::Y, Vec3::Z)]);
}

/// The alpha component must be `One, OneMinusSrcAlpha`: with `SrcAlpha` a
/// fringe fragment of alpha 0.5 over the opaque clear would leave a
/// destination alpha of 0.75, and every anti-aliased pixel of an exported PNG
/// or a thumbnail would come out translucent (D6).
#[test]
fn blending_keeps_the_destination_opaque() {
    use wgpu::{BlendComponent, BlendFactor, BlendOperation};
    assert_eq!(
        STRAIGHT_ALPHA_BLENDING.color,
        BlendComponent {
            src_factor: BlendFactor::SrcAlpha,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        }
    );
    assert_eq!(
        STRAIGHT_ALPHA_BLENDING.alpha,
        BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        }
    );
}
