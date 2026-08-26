//! The merged transparent isosurface mesh: an ordinary triangle [`Mesh`] plus
//! the pooled component ranges the back-to-front draw orders.
//!
//! # Why a wrapper rather than a field on `Mesh`
//!
//! Every other consumer of `Mesh` is opaque and has no components. Bolting a
//! `Vec` onto `Mesh` itself would put a permanently-empty vector on the main
//! mesh, the lightweight mesh and every gadget mesh, and would say nothing
//! about which of them the sort applies to. This type says it.
//!
//! # Why the components are pooled, not per surface
//!
//! `tessellate_scene_content` builds one mesh per pipeline, merging every
//! displayed node, so by the time geometry reaches the renderer there is no
//! per-surface anything left — and that is the correct granularity anyway: two
//! orbitals on screen interpenetrate exactly the way two lobes of one orbital
//! do, so a per-surface sort would be wrong in the same way an unsorted draw
//! is. Which node a component came from is therefore deliberately not
//! recorded. See `doc/design_isosurface_node.md` §The fix: sort components.

use crate::mesh::Mesh;
use glam::f32::Vec3;

/// One connected component of the merged mesh — a lobe — as a contiguous slice
/// of the index buffer, with the world-space centroid that is its sort key.
///
/// The renderer's counterpart of `atomcad_display::isosurface::SurfaceComponent`,
/// declared separately because `atomcad-renderer` sits *below* `atomcad-display`
/// in the crate DAG and cannot name it.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SurfaceComponentRange {
    pub first_index: u32,
    pub index_count: u32,
    /// World-space centroid; the sort key.
    pub centroid: Vec3,
}

/// The merged transparent surface mesh.
///
/// Vertices carry their own `alpha` (baked in by the tessellator from each
/// surface's scalar opacity), so surfaces at different alphas coexist here
/// correctly — which is the whole reason alpha is a vertex attribute rather
/// than a uniform.
#[derive(Debug, Default)]
pub struct TransparentSurfaceMesh {
    pub mesh: Mesh,
    /// Flat pool spanning every transparent surface on screen, in tessellation
    /// order. Ranges index [`Self::mesh`]'s index buffer.
    pub components: Vec<SurfaceComponentRange>,
}

impl TransparentSurfaceMesh {
    pub fn new() -> Self {
        Self {
            mesh: Mesh::new(),
            components: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.mesh.indices.is_empty()
    }
}

/// How the merged transparent surface mesh is drawn.
///
/// Three modes rather than one because the design's open question — *is
/// two-pass transparency good enough, or is a per-triangle sort required?* —
/// is only answerable by comparing them on the same scene at the same camera,
/// and once components are contiguous ranges the two losers are nearly free:
/// [`TwoPass`](Self::TwoPass) is "skip the sort" and
/// [`SinglePass`](Self::SinglePass) is "one draw, no culling".
///
/// **They are scaffolding.** Delete the two comparison modes once
/// `doc/design_isosurface_node.md` records whether `ComponentSorted` beats
/// `TwoPass` on a multi-lobe signed field and whether either is good enough
/// that OIT is unnecessary. Without a stated removal criterion they become
/// permanent options every future renderer change must keep working.
///
/// Only informative on the right subject: a density envelope is one
/// near-convex component and looks identical in all three.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum SurfaceTransparencyMode {
    /// One draw, no culling. The control.
    SinglePass,
    /// Back faces then front faces over the whole mesh, components unordered.
    TwoPass,
    /// Components back-to-front, two-pass within each.
    #[default]
    ComponentSorted,
}
