//! [`SurfaceMesh`] → renderer [`Mesh`].
//!
//! The last hop of the isosurface pipeline: the extractor has already done
//! everything that needs the field (positions, gradient normals, per-vertex
//! albedo, outward winding, component ranges), so this is a straight append
//! with one index rebase.
//!
//! **Per-vertex albedo is why nothing here takes a `Material`.** Every other
//! tessellator in this crate paints its output with one or two materials chosen
//! by the caller; an isosurface carries its colour on the vertices — a solid
//! colour per sign pass today, a colormap in `Field` mode — so the material a
//! caller could pass would only be thrown away. The roughness/metallic pair is
//! fixed here instead.

use atomcad_renderer::mesh::{Material, Mesh, Vertex};
use atomcad_renderer::transparent_surface_mesh::{SurfaceComponentRange, TransparentSurfaceMesh};
use glam::f32::Vec3;

/// Slightly glossy, fully dielectric. An isosurface reads as a soft membrane
/// rather than a machined solid, and a little specular is what makes a smooth
/// lobe's curvature legible without an edge to catch the light.
const ISOSURFACE_ROUGHNESS: f32 = 0.45;
const ISOSURFACE_METALLIC: f32 = 0.0;

use super::SurfaceMesh;

/// Appends `surface` to the **opaque** `mesh`, rebasing its indices onto
/// whatever is already there.
///
/// The surface's own `alpha` is ignored — a caller reaching this function has
/// already decided the surface is opaque (`alpha >= 1.0`), and every vertex is
/// written with `alpha = 1.0` so the shader change is a no-op for the opaque
/// mesh. Component ranges are not consulted either: they exist to order
/// transparent draws back-to-front, and an opaque surface needs no ordering.
pub fn tessellate_surface_mesh(mesh: &mut Mesh, surface: &SurfaceMesh) {
    append_vertices_and_indices(mesh, surface, 1.0);
}

/// Appends `surface` to the merged **transparent** surface mesh, baking its
/// scalar `alpha` onto every vertex it contributes and re-basing its component
/// ranges onto the pooled list.
///
/// Baking alpha per vertex is the only level at which per-surface opacity
/// survives the merge: the renderer holds one mesh per pipeline, so two
/// displayed surfaces at different alphas end up in this one buffer and a
/// per-mesh uniform could only describe one of them.
///
/// The component ranges *are* consulted here, unlike in the opaque path — they
/// are what the per-frame back-to-front draw orders, and pooling them across
/// surfaces is deliberate: two orbitals on screen interpenetrate exactly the
/// way two lobes of one orbital do.
pub fn tessellate_surface_mesh_transparent(
    target: &mut TransparentSurfaceMesh,
    surface: &SurfaceMesh,
) {
    if surface.is_empty() {
        return;
    }

    // Captured before the append: `first_index` values in `surface.components`
    // are relative to `surface.indices`, so they shift by however many indices
    // the merged buffer already holds.
    let index_base = target.mesh.indices.len() as u32;
    append_vertices_and_indices(&mut target.mesh, surface, surface.alpha);

    target.components.reserve(surface.components.len());
    for component in &surface.components {
        target.components.push(SurfaceComponentRange {
            first_index: index_base + component.first_index,
            index_count: component.index_count,
            centroid: component.centroid,
        });
    }
}

fn append_vertices_and_indices(mesh: &mut Mesh, surface: &SurfaceMesh, alpha: f32) {
    if surface.is_empty() {
        return;
    }

    let base = mesh.vertices.len() as u32;
    mesh.vertices.reserve(surface.positions.len());
    for (i, position) in surface.positions.iter().enumerate() {
        // The extractor guarantees these three are the same length; index
        // defensively anyway so a future producer bug degrades to a grey
        // surface rather than a panic in the render path.
        let normal = surface.normals.get(i).copied().unwrap_or(Vec3::Y);
        let albedo = surface.albedo.get(i).copied().unwrap_or(Vec3::splat(0.5));
        mesh.add_vertex(Vertex::new_translucent(
            position,
            &normal,
            &Material::new(&albedo, ISOSURFACE_ROUGHNESS, ISOSURFACE_METALLIC),
            alpha,
        ));
    }

    mesh.indices.reserve(surface.indices.len());
    for index in &surface.indices {
        mesh.indices.push(base + index);
    }
}
