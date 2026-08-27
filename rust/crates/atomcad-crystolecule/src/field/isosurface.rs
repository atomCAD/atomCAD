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
    /// How [`level`](Self::level) was arrived at — see [`LevelBasis`].
    ///
    /// **A readout passenger, not a parameter.** Nothing downstream branches on
    /// it: the extractor, the lattice policy, the tessellator and every renderer
    /// path see the resolved `level` and nothing else. It rides here because the
    /// node resolves the level in `eval` and the pin readout renders the value —
    /// so this is the one channel that connects them. It is never persisted.
    pub level_basis: LevelBasis,
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

// ============================================================================
// Choosing the level - `doc/design_isosurface_level.md` Part 2
// ============================================================================

/// The conventional isovalue for an electron density, `e/bohr^3`.
///
/// `Auto` prefers this over any fraction for a non-negative field, because a
/// *fixed absolute* level tracks the van der Waals envelope better than a fixed
/// enclosed fraction does: heavier atoms hold more of their density in the core,
/// so a fixed fraction cuts further in as Z rises (measured `r/r_vdW` spread
/// 1.097 for the absolute convention against 1.333 for the fraction, across
/// C/N/O/F).
pub const DENSITY_LEVEL: f64 = 0.002;

/// The enclosed mass fraction `Auto` reads a field's own scale at.
///
/// Calibrated against the simulation team's 16-file cube zoo: it puts both
/// methyl-radical orbitals inside the conventional 0.02-0.05 band (0.0327 and
/// 0.0268) and reproduces the density-viz handoff's own worked example for the
/// T-centre spin density to two figures (3.02e-4 against its 3.4e-4).
pub const LOCALIZED_FRACTION: f64 = 0.72;

/// How many times [`DENSITY_LEVEL`] the field's own localized scale may reach
/// before `0.002` is rejected as implausible in this field's units.
///
/// **The plausibility window.** ELF (conventionally drawn at ~0.8 of a 0-1
/// range) and RDG (~0.5) would take `0.002` and swallow the whole box, so the
/// density convention is checked against `iso_for_fraction(LOCALIZED_FRACTION)`
/// — the number the other branch would have picked. Across the zoo no real
/// density exceeds a ratio of 72.4 at any box padding and no impostor drops
/// below 228; `125` sits just under the geometric midpoint of that gap
/// (`sqrt(72.4 * 228) = 129`), leaving 1.7x of headroom below and 1.8x above.
///
/// The *voxel* share above `0.002` is the obvious alternative statistic and is
/// deliberately not used: it measures the box rather than the field, moving from
/// 36.7% to 99.2% for one CH3 density as its padding is cropped, while the ratio
/// moves by 1.7x across the same crops and never leaves its band.
pub const MAX_LEVEL_RATIO: f64 = 125.0;

/// A field counts as signed once its minimum falls below this share of its own
/// magnitude scale.
///
/// **A tolerance, not `min >= 0`.** The two `Auto` branches are an order of
/// magnitude and a half apart - on a real silicon-cluster density the density
/// branch gives `0.002` and the fraction branch `8.70e-2`, a factor of 43 - so a
/// raw sign test would turn one voxel of numerical noise at `-1e-12` into a 43x
/// wrong level, silently. A plane-wave density interpolated onto a grid rings
/// slightly negative exactly that way. `1e-6` is safe by a wide margin: the
/// smallest genuine negative lobe in the calibration zoo sits at 2.7% of its
/// field's scale, four orders of magnitude above this.
pub const NEGATIVE_TOLERANCE: f64 = 1e-6;

/// Why no level could be resolved. Turned into an evaluation error by the
/// `isosurface` node, which prefixes its own name.
///
/// Every variant is a *field* property, so none of them can be caught by
/// validation - which sees node data and wires and no field at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelResolutionError {
    /// Fraction mode on a field with no stored samples.
    AnalyticFieldInFractionMode,
    /// Auto on a *signed* field with no stored samples. The non-negative case
    /// has an honest fallback ([`DENSITY_LEVEL`]); a signed one does not, since
    /// no absolute convention exists for an orbital amplitude that does not
    /// degrade with delocalization.
    AnalyticSignedFieldInAutoMode,
    /// The field is entirely zero, so its total mass is zero and no fraction
    /// resolves. Reachable from both `Auto` and `Fraction`, and the one
    /// combination easy to miss: `value_distribution` returns `Some` while both
    /// of its queries return `None`.
    AllZeroField,
}

impl std::fmt::Display for LevelResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LevelResolutionError::AnalyticFieldInFractionMode => write!(
                f,
                "fraction mode needs a field with stored samples, and this field is \
                 analytic (no native grid). Switch the level mode to absolute"
            ),
            LevelResolutionError::AnalyticSignedFieldInAutoMode => write!(
                f,
                "auto mode has no level to offer for a signed field with no stored \
                 samples, and this field is analytic (no native grid). Switch the \
                 level mode to absolute"
            ),
            LevelResolutionError::AllZeroField => write!(
                f,
                "the field is entirely zero, so no level encloses anything - check \
                 the upstream import"
            ),
        }
    }
}

/// The four `Auto` outcomes, in the order the rule tries them.
///
/// Carried out of evaluation for the *readout only* - nothing downstream
/// branches on it. A caller that discards it makes a guess invisible, which is
/// the failure this type exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoBasis {
    /// Non-negative and `0.002` is plausible in this field's units.
    DensityLike,
    /// Non-negative, but the plausibility window rejected `0.002` - the field's
    /// own localized scale is more than [`MAX_LEVEL_RATIO`] times it. ELF and
    /// RDG land here, and the level they get is adjustable-and-wrong rather than
    /// nothing-at-all. Do not read this basis as a claim that the level is right.
    Atypical,
    /// Signed, so no absolute convention applies and the fraction is the
    /// coordinate.
    Signed,
    /// No distribution to check `0.002` against - an analytic field. The
    /// convention is taken unchecked.
    Unchecked,
}

impl AutoBasis {
    /// The user-facing string. **The only place these are rendered**, so a
    /// readout and a panel caption cannot drift apart.
    pub fn label(self) -> &'static str {
        match self {
            AutoBasis::DensityLike => "non-negative, density-like",
            AutoBasis::Atypical => "non-negative, atypical",
            AutoBasis::Signed => "signed field",
            AutoBasis::Unchecked => "non-negative, unchecked",
        }
    }
}

/// How a resolved level was arrived at, for the readout.
///
/// **Never persisted**: not in the node's stored data and not in the `.cnnd`. It
/// is derived on every evaluation, exactly like the level it accompanies, and it
/// rides along inside [`IsosurfaceData`] only because that is the value the
/// readout is handed. Nothing downstream of the node branches on it - the
/// extractor, the tessellator and every renderer path see the resolved
/// [`IsosurfaceData::level`] and nothing else.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LevelBasis {
    /// The level was typed (or wired) as a magnitude.
    Absolute,
    /// The enclosed fraction that produced it.
    Fraction(f64),
    /// Chosen from the field - see [`AutoBasis`].
    Auto(AutoBasis),
}

/// Pick an isolevel from the field alone - the `Auto` level mode.
///
/// **Signedness picks the coordinate**, and the property that actually drives
/// the difference is the *nuclear cusp*: a total density has one and holds most
/// of its electrons in a minuscule volume (useful fractions 0.98-0.999), while
/// orbitals, spin densities and deformation densities do not (0.5-0.9). Cusped
/// fields are dominated by densities, which are non-negative, so
/// [`ScalarField::value_range`] - already read elsewhere for the component count
/// — separates the two at no cost.
///
/// **Known limitation: signed does not imply cusp-free.** A field *derived* from
/// the density inherits its cusp and can still be signed; the Laplacian of rho
/// and `sign(lambda2)*rho` both resolve to visibly wrong levels here. Accepted
/// rather than fixed - both are exotic post-processed fields, wrong at a glance,
/// with the editor's histogram beside the control. Do not claim they work.
///
/// **Auto is volatile by design**: the level moves when the field changes. A
/// figure that must not change belongs in fraction or absolute mode.
pub fn auto_level(field: &dyn ScalarField) -> Result<(f64, AutoBasis), LevelResolutionError> {
    // `None` (an analytic field that does not scan itself) is read as "not known
    // to be signed", which routes to the same unchecked fallback a non-negative
    // analytic field takes. That is the honest answer: nothing here can
    // establish signedness without samples.
    let signed = field.value_range().is_some_and(|(min, max)| {
        let scale = min.abs().max(max.abs());
        min < -NEGATIVE_TOLERANCE * scale
    });

    let Some(distribution) = field.value_distribution() else {
        if signed {
            return Err(LevelResolutionError::AnalyticSignedFieldInAutoMode);
        }
        return Ok((DENSITY_LEVEL, AutoBasis::Unchecked));
    };

    // Both queries return `Option` and neither may be unwrapped: an all-zero
    // field has `total == 0`, so `value_distribution` is `Some` while this is
    // `None`.
    let Some(localized) = distribution.iso_for_fraction(LOCALIZED_FRACTION) else {
        return Err(LevelResolutionError::AllZeroField);
    };

    if signed {
        Ok((localized, AutoBasis::Signed))
    } else if localized <= MAX_LEVEL_RATIO * DENSITY_LEVEL {
        Ok((DENSITY_LEVEL, AutoBasis::DensityLike))
    } else {
        Ok((localized, AutoBasis::Atypical))
    }
}
