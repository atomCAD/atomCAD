//! Phase 4 of `doc/design_isosurface_node.md`: the vertex alpha attribute and
//! the back-to-front component sort.
//!
//! Both are GPU-free — the vertex layout is arithmetic over `size_of`, and the
//! sort is a permutation of a list of centroids — so they belong here rather
//! than behind a device.

use atomcad_renderer::gpu_mesh::ModelUniform;
use atomcad_renderer::mesh::{Material, Vertex};
use atomcad_renderer::surface_sort::sorted_component_order;
use atomcad_renderer::transparent_surface_mesh::SurfaceComponentRange;
use glam::f32::{Mat4, Vec3};

fn material() -> Material {
    Material::new(&Vec3::new(0.2, 0.4, 0.9), 0.45, 0.0)
}

/// Every pre-existing producer goes through `Vertex::new`, so opacity has to
/// default to fully opaque or the whole opaque scene would fade.
#[test]
fn plain_vertices_are_opaque() {
    let vertex = Vertex::new(&Vec3::ZERO, &Vec3::Y, &material());
    assert_eq!(vertex.alpha, 1.0);
}

#[test]
fn translucent_vertices_keep_their_alpha() {
    let vertex = Vertex::new_translucent(&Vec3::ZERO, &Vec3::Y, &material(), 0.4);
    assert_eq!(vertex.alpha, 0.4);
    // Everything else must match the opaque constructor: the two differ in one
    // channel and nothing else.
    let opaque = Vertex::new(&Vec3::ZERO, &Vec3::Y, &material());
    assert_eq!(vertex.position, opaque.position);
    assert_eq!(vertex.normal, opaque.normal);
    assert_eq!(vertex.albedo, opaque.albedo);
    assert_eq!(vertex.roughness, opaque.roughness);
    assert_eq!(vertex.metallic, opaque.metallic);
}

/// The hand-written `desc()` offsets are the one place a `#[repr(C)]` struct
/// and its GPU description can silently disagree: a wrong trailing offset reads
/// garbage rather than failing.
#[test]
fn vertex_layout_matches_the_struct() {
    let desc = Vertex::desc();
    assert_eq!(desc.attributes.len(), 6, "one attribute per vertex field");
    assert_eq!(
        desc.array_stride as usize,
        std::mem::size_of::<Vertex>(),
        "stride must be the whole struct, padding included"
    );

    let alpha = desc
        .attributes
        .iter()
        .find(|a| a.shader_location == 5)
        .expect("alpha attribute");
    assert_eq!(alpha.format, wgpu::VertexFormat::Float32);
    assert_eq!(
        alpha.offset as usize,
        std::mem::size_of::<Vertex>() - std::mem::size_of::<f32>(),
        "alpha is the trailing f32"
    );
    // No padding anywhere: 12 tightly-packed f32 fields.
    assert_eq!(
        std::mem::size_of::<Vertex>(),
        std::mem::size_of::<[f32; 12]>()
    );
}

/// Regression guard on the *rejected* placement. Alpha went on the vertex
/// partly because a bare trailing `f32` on a uniform breaks the 16-byte WGSL
/// boundary silently — issue #269 was exactly that bug in `CameraUniform`.
#[test]
fn model_uniform_did_not_grow() {
    assert_eq!(std::mem::size_of::<ModelUniform>(), 128);
}

fn component(first_index: u32, centroid: Vec3) -> SurfaceComponentRange {
    SurfaceComponentRange {
        first_index,
        index_count: 3,
        centroid,
    }
}

/// A camera at `+z` looking back at the origin: view-space z is then the
/// negated world z, so the *smallest* world z is the farthest and must come
/// first.
fn view_from_positive_z() -> Mat4 {
    Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y)
}

#[test]
fn components_come_back_farthest_first() {
    let components = [
        component(0, Vec3::new(0.0, 0.0, 1.0)),
        component(3, Vec3::new(0.0, 0.0, -4.0)),
        component(6, Vec3::new(0.0, 0.0, 2.5)),
    ];
    let order = sorted_component_order(&components, &view_from_positive_z());
    assert_eq!(order, vec![1, 0, 2]);
}

/// The pool spans every transparent surface on screen, so components from two
/// different `isosurface` nodes must interleave by depth rather than clustering
/// by node. Two orbitals on screen interpenetrate exactly the way two lobes of
/// one orbital do.
#[test]
fn components_from_two_surfaces_interleave_by_depth() {
    // First surface contributed components 0 and 1, second contributed 2 and 3.
    let components = [
        component(0, Vec3::new(0.0, 0.0, 3.0)),
        component(3, Vec3::new(0.0, 0.0, -3.0)),
        component(6, Vec3::new(0.0, 0.0, 1.0)),
        component(9, Vec3::new(0.0, 0.0, -1.0)),
    ];
    let order = sorted_component_order(&components, &view_from_positive_z());
    assert_eq!(order, vec![1, 3, 2, 0]);
}

/// The order is a function of the camera, which is why the sort runs per moved
/// frame rather than once at tessellation time.
#[test]
fn reversing_the_camera_reverses_the_order() {
    let components = [
        component(0, Vec3::new(0.0, 0.0, 1.0)),
        component(3, Vec3::new(0.0, 0.0, -4.0)),
        component(6, Vec3::new(0.0, 0.0, 2.5)),
    ];
    let behind = Mat4::look_at_rh(Vec3::new(0.0, 0.0, -10.0), Vec3::ZERO, Vec3::Y);
    let order = sorted_component_order(&components, &behind);
    assert_eq!(order, vec![2, 0, 1]);
}

/// NaN centroids must not panic the render path — `total_cmp` gives a total
/// order, so the result is merely arbitrary, not a crash.
#[test]
fn nan_centroids_do_not_panic() {
    let components = [
        component(0, Vec3::new(0.0, 0.0, 1.0)),
        component(3, Vec3::new(f32::NAN, 0.0, 0.0)),
    ];
    let order = sorted_component_order(&components, &view_from_positive_z());
    assert_eq!(order.len(), 2);
}

#[test]
fn no_components_is_an_empty_order() {
    assert!(sorted_component_order(&[], &view_from_positive_z()).is_empty());
}
