//! `SurfaceMesh` → renderer mesh, opaque and transparent
//! (`doc/design_isosurface_node.md` P4).
//!
//! The merge is where per-surface alpha has to survive: the renderer holds one
//! mesh per pipeline, so two displayed surfaces at different alphas land in one
//! buffer and only a per-vertex channel can tell them apart. These tests assert
//! that, and that the pooled component ranges still address the merged index
//! buffer after rebasing.

use atomcad_display::isosurface::{
    SurfaceComponent, SurfaceMesh, tessellate_surface_mesh, tessellate_surface_mesh_transparent,
};
use atomcad_renderer::mesh::Mesh;
use atomcad_renderer::transparent_surface_mesh::TransparentSurfaceMesh;
use glam::f32::Vec3;

/// One triangle, one component, at a caller-chosen alpha and offset.
fn one_triangle_surface(offset: Vec3, alpha: f32) -> SurfaceMesh {
    let positions = vec![offset, offset + Vec3::X, offset + Vec3::Y];
    let centroid = positions.iter().copied().sum::<Vec3>() / 3.0;
    SurfaceMesh {
        positions,
        normals: vec![Vec3::Z; 3],
        albedo: vec![Vec3::new(0.2, 0.4, 0.9); 3],
        indices: vec![0, 1, 2],
        components: vec![SurfaceComponent {
            first_index: 0,
            index_count: 3,
            centroid,
        }],
        alpha,
    }
}

/// Two triangles, two components — the shape that makes rebasing observable.
fn two_component_surface(offset: Vec3, alpha: f32) -> SurfaceMesh {
    let mut surface = one_triangle_surface(offset, alpha);
    let second = offset + Vec3::new(4.0, 0.0, 0.0);
    surface
        .positions
        .extend([second, second + Vec3::X, second + Vec3::Y]);
    surface.normals.extend([Vec3::Z; 3]);
    surface.albedo.extend([Vec3::new(0.9, 0.3, 0.25); 3]);
    surface.indices.extend([3, 4, 5]);
    surface.components.push(SurfaceComponent {
        first_index: 3,
        index_count: 3,
        centroid: second + Vec3::new(1.0, 1.0, 0.0) / 3.0,
    });
    surface
}

#[test]
fn opaque_tessellation_writes_opaque_vertices() {
    let mut mesh = Mesh::new();
    // Even a surface carrying a translucent alpha is written opaque here: the
    // caller has already decided it takes the opaque path.
    tessellate_surface_mesh(&mut mesh, &one_triangle_surface(Vec3::ZERO, 0.4));
    assert_eq!(mesh.vertices.len(), 3);
    assert!(mesh.vertices.iter().all(|v| v.alpha == 1.0));
}

#[test]
fn transparent_tessellation_bakes_the_surfaces_alpha() {
    let mut target = TransparentSurfaceMesh::new();
    tessellate_surface_mesh_transparent(&mut target, &one_triangle_surface(Vec3::ZERO, 0.4));
    assert_eq!(target.mesh.vertices.len(), 3);
    assert!(target.mesh.vertices.iter().all(|v| v.alpha == 0.4));
}

/// The failure the vertex attribute exists to prevent: one alpha per pipeline
/// would force these two to agree.
#[test]
fn two_surfaces_keep_their_own_alphas_in_the_merged_mesh() {
    let mut target = TransparentSurfaceMesh::new();
    tessellate_surface_mesh_transparent(&mut target, &one_triangle_surface(Vec3::ZERO, 0.4));
    tessellate_surface_mesh_transparent(
        &mut target,
        &one_triangle_surface(Vec3::new(10.0, 0.0, 0.0), 0.85),
    );

    assert_eq!(target.mesh.vertices.len(), 6);
    let alphas: Vec<f32> = target.mesh.vertices.iter().map(|v| v.alpha).collect();
    assert_eq!(alphas, vec![0.4, 0.4, 0.4, 0.85, 0.85, 0.85]);
}

/// Ranges arrive relative to their own surface and must be rebased onto the
/// merged index buffer, or the second surface's components would address the
/// first surface's triangles.
#[test]
fn component_ranges_are_rebased_onto_the_pool() {
    let mut target = TransparentSurfaceMesh::new();
    tessellate_surface_mesh_transparent(&mut target, &two_component_surface(Vec3::ZERO, 0.4));
    tessellate_surface_mesh_transparent(
        &mut target,
        &two_component_surface(Vec3::new(10.0, 0.0, 0.0), 0.4),
    );

    assert_eq!(target.components.len(), 4);
    let starts: Vec<u32> = target.components.iter().map(|c| c.first_index).collect();
    assert_eq!(starts, vec![0, 3, 6, 9]);
    assert!(target.components.iter().all(|c| c.index_count == 3));

    // Every range addresses in-bounds indices, and together they tile the
    // buffer — the property the per-component draw calls depend on.
    let total: u32 = target.components.iter().map(|c| c.index_count).sum();
    assert_eq!(total as usize, target.mesh.indices.len());

    // ...and vertex indices are rebased too: the second surface's triangles
    // must not point at the first surface's vertices.
    assert_eq!(
        target.mesh.indices,
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
    );
}

/// Centroids are world-space and pass through untouched — they are the sort
/// key, and shifting one would reorder the draw.
#[test]
fn centroids_survive_the_merge_unchanged() {
    let surface = one_triangle_surface(Vec3::new(2.0, 3.0, 4.0), 0.5);
    let expected = surface.components[0].centroid;

    let mut target = TransparentSurfaceMesh::new();
    tessellate_surface_mesh_transparent(&mut target, &one_triangle_surface(Vec3::ZERO, 0.5));
    tessellate_surface_mesh_transparent(&mut target, &surface);

    assert_eq!(target.components[1].centroid, expected);
}

#[test]
fn an_empty_surface_contributes_nothing() {
    let mut target = TransparentSurfaceMesh::new();
    tessellate_surface_mesh_transparent(&mut target, &SurfaceMesh::default());
    assert!(target.is_empty());
    assert!(target.components.is_empty());
}
