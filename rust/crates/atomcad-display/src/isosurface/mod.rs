//! Marching-cubes extraction: `IsosurfaceData` (the *specification*) into
//! [`SurfaceMesh`] (a renderer-shaped triangle mesh).
//!
//! This is stage 1 → stage 2 of the pipeline in
//! `doc/design_isosurface_node.md`. The node's value is resolution-free and
//! lives in `atomcad-crystolecule`; the mesh is resolution-fixed and lives
//! here, because `atomcad-display` is what turns domain values into things the
//! renderer draws. The governing rule: **semantic parameters live in the value,
//! quality parameters live in preferences** — isolevel is semantic, extraction
//! resolution is not.
//!
//! # Invariants this module owns
//!
//! - **Ångström in, Ångström out.** `ScalarField` coordinates are real-space
//!   Ångström and so are [`SurfaceMesh::positions`]. Nothing here converts
//!   units.
//! - **Closed surfaces.** Guaranteed by construction, not by inspection — see
//!   [`case_table`] for the face-locality argument. The two-pass transparency
//!   draw assumes every view ray crosses the surface an even number of times,
//!   so a hole presents as "transparency is buggy", not as "extraction is
//!   buggy".
//! - **Outward winding.** Triangles are counter-clockwise seen from outside,
//!   consistent with the per-vertex normal `-sign * normalize(grad psi)`. Two
//!   independent things can invert it — the case table's own vertex order and a
//!   left-handed lattice — and both are handled here.
//!
//! # No extraction cache
//!
//! Deliberately absent, and out of scope rather than left to taste. Scene
//! generation runs at user-action frequency, not per frame (`move_camera` never
//! calls it), and an isosurface has no subtree structure for a
//! `csg_conversion_cache`-style recursive cache to harvest. A cache would need
//! a sound key — an `Arc` address is not one, since the allocation can be freed
//! and the address reused — plus an eviction policy and an invalidation story.
//! If timings ever say otherwise, that is its own design.

pub mod case_table;
pub mod colormap;
pub mod extract;
pub mod lattice;

pub use colormap::sample_colormap;
pub use extract::extract_isosurface;
pub use lattice::Lattice;

use glam::Vec3;

/// Quality knobs for one extraction. These come from
/// `GeometryVisualizationPreferences`, never from node data: baking a
/// resolution into the `.cnnd` would make a quality setting part of the
/// document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractionSettings {
    /// Integer subdivision of the field's native grid, rounded and clamped to
    /// `>= 1`. `2` halves the step. Values below `1` do not coarsen — a sampled
    /// field's own grid is the floor.
    pub quality_multiplier: f64,
    /// Cell size used when the field reports no native grid, Ångström.
    pub fallback_spacing: f64,
    /// Ceiling on marching-cubes **cells** — not triangles and not bytes.
    /// Cells are the only one of the three knowable *before* doing the work, so
    /// the check is pre-flight and costs nothing.
    pub cell_budget: usize,
}

impl Default for ExtractionSettings {
    fn default() -> Self {
        Self {
            quality_multiplier: 1.0,
            fallback_spacing: 0.15,
            cell_budget: 16_000_000,
        }
    }
}

/// Why an extraction produced no mesh.
///
/// One variant today. It is reported from the display conversion rather than
/// from `eval`, because node data cannot see preferences and so cannot know the
/// cell count — see the design's §Preferences for the five details that make
/// that reporting path work.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtractionError {
    CellBudgetExceeded {
        cells: u128,
        budget: usize,
        quality_multiplier: f64,
    },
}

impl std::fmt::Display for ExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractionError::CellBudgetExceeded {
                cells,
                budget,
                quality_multiplier,
            } => write!(
                f,
                "extraction grid is {} cells, over the {} budget; \
                 lower isosurface_quality_multiplier (currently {}) \
                 or raise isosurface_cell_budget",
                grouped(*cells),
                grouped(*budget as u128),
                quality_multiplier,
            ),
        }
    }
}

impl std::error::Error for ExtractionError {}

/// Thousands separators. All three numbers in the budget message are large and
/// none of them is a node property the user can see from the node, so they are
/// worth making readable.
fn grouped(value: u128) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// One extracted surface: positions, normals, per-vertex albedo, and the
/// connected components the transparency sort orders.
///
/// A new type rather than a `PolyMesh` reuse because **`PolyMesh` carries no
/// per-vertex color** — `poly_mesh_tessellator` assigns one `Material` per mesh
/// — and its face-centric adjacency exists for `detect_sharp_edges`, the
/// opposite of what a smooth marching-cubes surface wants.
#[derive(Debug, Clone, Default)]
pub struct SurfaceMesh {
    /// Real-space Ångström.
    pub positions: Vec<Vec3>,
    /// Outward normals from the field gradient.
    pub normals: Vec<Vec3>,
    /// Per-vertex albedo: solid per sign pass in `Phase` mode, colormapped in
    /// `Field` mode.
    pub albedo: Vec<Vec3>,
    pub indices: Vec<u32>,
    /// Connected components, each a contiguous slice of [`indices`](Self::indices).
    /// Ordering these back-to-front at draw time is what makes multi-lobe
    /// transparency correct.
    pub components: Vec<SurfaceComponent>,
    /// One value for the whole surface, straight from `IsosurfaceData::alpha`.
    /// It stays scalar *here* because a `SurfaceMesh` is one node's output; the
    /// scene tessellator reads it to pick which isosurface mesh this surface
    /// joins and to bake it onto every vertex it contributes.
    pub alpha: f32,
}

impl SurfaceMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }
}

/// One connected component of a [`SurfaceMesh`] — a lobe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceComponent {
    pub first_index: u32,
    pub index_count: u32,
    /// World-space centroid; the sort key.
    pub centroid: Vec3,
}
