//! The isosurface *specification* — a surface to be extracted at display time.
//!
//! This is the value half of `doc/design_isosurface_node.md`: the `isosurface`
//! node produces an [`IsosurfaceData`], which travels the network as
//! `NetworkResult::Isosurface` and is turned into an actual triangle mesh only
//! at the display conversion, by the extractor in `atomcad-display`.
//!
//! **The specification carries semantic parameters only.** Isolevel and paint
//! are here; extraction *resolution* is not — that is a quality setting and
//! lives in `GeometryVisualizationPreferences`, exactly as it does for
//! `Blueprint`. See the design document's "Where the extraction happens".
//!
//! It lives in this crate, beside the [`ScalarField`](super::ScalarField) it
//! wraps, because every `NetworkResult` payload comes from the domain layer or
//! from `structure_designer` itself; display types enter one stage later, in
//! `NodeOutput`. The extracted mesh (`SurfaceMesh`) accordingly lives in
//! `atomcad-display`.

use std::sync::Arc;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use super::ScalarField;

/// A surface to be extracted at display time. Carries the *specification*,
/// never the mesh.
///
/// Held inline in `NetworkResult`, not behind an `Arc`: at most two `Arc`s plus
/// a handful of scalars is already a cheap clone, and `BlueprintData` is inline
/// for the same reason.
///
/// **Narrowing is selective, and the split is by consumer.** Every property is
/// authored as an `f64`/`DVec3` at `TextValue::Float` precision. The ones the
/// *renderer* consumes narrow to `f32`/[`Vec3`] here — [`alpha`](Self::alpha)
/// and the two [`IsosurfaceColoring::Phase`] colors — while the ones compared
/// against *field values* stay `f64`: [`level`](Self::level), and the
/// `range` inside [`IsosurfaceColoring::Field`]. [`ScalarField::sample`]
/// returns `f64`, so narrowing a threshold would introduce a rounding
/// difference between the comparison and the data it is compared to.
#[derive(Debug, Clone)]
pub struct IsosurfaceData {
    /// The field whose level set is drawn.
    pub field: Arc<dyn ScalarField>,
    /// Level **magnitude**, strictly positive. Extraction runs at `+level` and
    /// at `-level`, so the sign is not a user choice — see the node's
    /// validation, which rejects `level <= 0`.
    pub level: f64,
    /// How the extracted surface is painted.
    pub coloring: IsosurfaceColoring,
    /// Opacity in `0..=1`. `>= 1.0` takes the opaque fast path in the scene
    /// tessellator (compare `>=`, not `==`, so a slider landing on `0.9999999`
    /// still takes it).
    pub alpha: f32,
}

/// The 2x2 of the sibling design settled: *signedness* (read from
/// [`ScalarField::value_range`] at extraction time) decides how many components
/// are extracted; this discriminant decides how they are painted. Neither
/// consults the other.
#[derive(Debug, Clone)]
pub enum IsosurfaceColoring {
    /// No color field — one solid color per sign pass.
    Phase {
        /// Color of the `+level` lobe, 0-1 RGB.
        positive: Vec3,
        /// Color of the `-level` lobe, 0-1 RGB.
        negative: Vec3,
    },
    /// Color field supplied — per-vertex colormap over `range`.
    Field {
        /// Sampled per surface vertex to pick a color. Commonly a different
        /// quantity from the surface field (density painted by electrostatic
        /// potential), but nothing stops it being the same `Arc`.
        field: Arc<dyn ScalarField>,
        /// Colormap domain, `(min, max)`. **Never auto-fitted**: these
        /// quantities span orders of magnitude around the nuclei, so fitting to
        /// the extrema paints the whole surface one flat color.
        range: (f64, f64),
        colormap: Colormap,
    },
}

/// Named color ramp for [`IsosurfaceColoring::Field`].
///
/// One variant, because the only reachable pairing today is a density colored
/// by an electrostatic potential; NCI's blue-green-red map has no producer yet.
/// The enum exists so that adding one is a one-line change and the serialized
/// form is forward-compatible.
///
/// This is node data that round-trips through the `.cnnd`, which is the second
/// reason the type lives in this crate rather than in `atomcad-display`: that
/// crate has no `serde` dependency and should not acquire one to host a
/// persisted enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Colormap {
    /// Diverging blue-white-red — the conventional ESP map.
    #[default]
    BlueWhiteRed,
}

impl IsosurfaceData {
    /// Approximate heap footprint of the field data this value refers to.
    ///
    /// **Each distinct `Arc` is counted once.** An `IsosurfaceData` can reach
    /// the same allocation twice — once as [`field`](Self::field), once as
    /// [`IsosurfaceColoring::Field::field`] — and a `SampledField` holds a
    /// `Vec<f32>` of megabytes, so summing the two blindly would double-count
    /// the common case of a field painted with itself.
    ///
    /// Sharing *across* results (the same field also live as a
    /// `NetworkResult::ScalarField` elsewhere in the same evaluation pass) stays
    /// double-counted, unchanged from how the plain `ScalarField` payload
    /// already behaves: correcting that would need a pass-wide pointer set,
    /// which the estimator has no access to.
    pub fn estimate_field_memory_bytes(&self) -> usize {
        let mut bytes = self.field.estimate_memory_bytes();
        if let IsosurfaceColoring::Field { field, .. } = &self.coloring
            && !Arc::ptr_eq(&self.field, field)
        {
            bytes += field.estimate_memory_bytes();
        }
        bytes
    }
}
