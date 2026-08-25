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
use glam::f32::Vec3;

/// Slightly glossy, fully dielectric. An isosurface reads as a soft membrane
/// rather than a machined solid, and a little specular is what makes a smooth
/// lobe's curvature legible without an edge to catch the light.
const ISOSURFACE_ROUGHNESS: f32 = 0.45;
const ISOSURFACE_METALLIC: f32 = 0.0;

use super::SurfaceMesh;

/// Appends `surface` to `mesh`, rebasing its indices onto whatever is already
/// there.
///
/// Component ranges are **not** consulted: they exist to order transparent
/// draws back-to-front, and an opaque surface needs no ordering. When the
/// transparent path arrives it will read them from the `SurfaceMesh` rather
/// than from the merged mesh, so nothing is lost by ignoring them here.
pub fn tessellate_surface_mesh(mesh: &mut Mesh, surface: &SurfaceMesh) {
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
        mesh.add_vertex(Vertex::new(
            position,
            &normal,
            &Material::new(&albedo, ISOSURFACE_ROUGHNESS, ISOSURFACE_METALLIC),
        ));
    }

    mesh.indices.reserve(surface.indices.len());
    for index in &surface.indices {
        mesh.indices.push(base + index);
    }
}
