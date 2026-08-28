//! The colour field's distribution **on the extracted surface** — the statistic
//! the colour domain is fitted from.
//!
//! `doc/design_isosurface_level.md` Part 4. This is a *different* statistic from
//! the level's [`ValueDistribution`](atomcad_crystolecule::field::distribution::ValueDistribution),
//! and the differences are not cosmetic:
//!
//! | | surface field | colour field |
//! |---|---|---|
//! | Population | all stored samples | colour field **at the surface's vertices** |
//! | Weight | mass, `\|v\|` | **surface area** |
//! | Sign | folded to `\|v\|` | **kept signed** |
//! | Parameter | one magnitude | an interval |
//! | Query | `iso_for_fraction(f)` | percentiles `p2` / `p98` |
//!
//! **Why the surface and not the volume.** Measured on the zoo's `ch3cl.cube` /
//! `ch3cl_esp.cube`, the ESP over the whole box reaches `+9.72e+1`, while on the
//! `0.002` density envelope its `p98` is `4.00e-2` — **2427x** apart. Fitting a
//! colour ramp to the volume's extrema paints the entire envelope one flat
//! colour, which is why the node's `range` carried the comment "never
//! auto-fitted". Restricting the population to the surface removes that
//! objection entirely; nothing else about it changes.
//!
//! **Area weighting, not vertex count.** Marching cubes puts vertices where the
//! geometry is busy, not where the area is, so a crumpled region would otherwise
//! dominate the percentile. Each vertex carries a third of every triangle it
//! belongs to.
//!
//! **Why this lives here.** The mesh does not exist until the display
//! conversion — `IsosurfaceData` is a *specification*, and marching cubes runs
//! one stage later, where the extraction preferences are visible. So the
//! statistic is computed where the mesh is and parked on `NodeSceneData` beside
//! the mesh it describes.
//!
//! **The fit is gated on a vertex floor.** The statistic is read off a mesh
//! whose resolution comes from a *quality preference*, so two presses at two
//! quality settings must not give two answers. Decimating the `ch3cl` grid keeps
//! `p98` inside 0.3 % down to ~180 band samples and then breaks (3.61e-2 at 78).
//! Below [`MIN_FIT_VERTICES`] the fit refuses rather than writing a plausible
//! wrong number into a saved document.

use atomcad_crystolecule::field::{IsosurfaceColoring, IsosurfaceData, ScalarField};
use glam::DVec3;

use super::SurfaceMesh;

/// Vertices a surface must carry before its colour domain may be fitted.
///
/// "A few hundred" from the decimation table in the module docs: the statistic
/// is stable to 0.3 % while the surface is adequately sampled and breaks only
/// when the sample count collapses. This is the one gate that keeps a quality
/// preference out of a saved `.cnnd`.
pub const MIN_FIT_VERTICES: usize = 300;

/// Bars in the panel's span plot.
///
/// A signed **linear** axis over a narrow range (`±0.08` for a real ESP) in a
/// sidebar-width plot: far fewer bars than the level histogram's 2048 log bins,
/// and nothing queries them — the percentiles are computed exactly, at
/// construction, from the unbinned values.
pub const SURFACE_HISTOGRAM_BINS: usize = 96;

/// Half-width the histogram span is padded to when every surface value is the
/// same, so the bin width is never zero.
const DEGENERATE_PAD: f64 = 0.5;

/// The colour field's area-weighted, signed distribution over one extracted
/// surface.
///
/// **Summary statistics only.** Nothing downstream asks for an arbitrary
/// percentile — the fit needs `p2`, `p98` and `p98(|v|)`, and the plot needs a
/// histogram — so the sorted per-vertex arrays are consumed at construction and
/// dropped. That keeps this a fixed ~1.5 KB regardless of how many vertices the
/// surface has, which matters because it is parked on `NodeSceneData` and the
/// invisible-node cache budgets against that.
#[derive(Debug, Clone)]
pub struct SurfaceValueDistribution {
    vertex_count: usize,
    total_area: f64,
    value_min: f64,
    value_max: f64,
    p2: f64,
    p50: f64,
    p98: f64,
    abs_p98: f64,
    signed: bool,
    bin_edges: Vec<f64>,
    bin_weight: Vec<f64>,
}

impl SurfaceValueDistribution {
    /// Vertices the statistics were taken over.
    pub fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    /// Total surface area, Ångström². The percentile weights sum to this.
    pub fn total_area(&self) -> f64 {
        self.total_area
    }

    /// Smallest and largest colour value anywhere on the surface.
    pub fn value_range(&self) -> (f64, f64) {
        (self.value_min, self.value_max)
    }

    /// Area-weighted 2nd percentile of the signed value.
    pub fn p2(&self) -> f64 {
        self.p2
    }

    /// Area-weighted median. Not used by the fit — it is what makes the
    /// area-weighting rule *testable*, since a mesh whose two halves carry equal
    /// area but unequal triangle counts separates it from the count-weighted
    /// median by a measurable amount.
    pub fn p50(&self) -> f64 {
        self.p50
    }

    /// Area-weighted 98th percentile of the signed value.
    pub fn p98(&self) -> f64 {
        self.p98
    }

    /// Area-weighted 98th percentile of `|v|` — the symmetric fit's half-width.
    pub fn abs_p98(&self) -> f64 {
        self.abs_p98
    }

    /// Whether the colour field takes negative values, which is what selects the
    /// symmetric fit. Read from
    /// [`ScalarField::value_range`] when the field declares one, and from the
    /// surface's own values otherwise (an analytic field declares nothing).
    pub fn is_signed(&self) -> bool {
        self.signed
    }

    /// Bin edges of the plot's linear, signed axis — `bin_weight().len() + 1`.
    pub fn bin_edges(&self) -> &[f64] {
        &self.bin_edges
    }

    /// Surface area falling in each bin.
    pub fn bin_weight(&self) -> &[f64] {
        &self.bin_weight
    }

    /// The `(min, max)` the fit button would write, or `None` when the surface
    /// is too coarse to fit or carries no spread at all.
    ///
    /// Two fits, chosen by the colour field's signedness:
    ///
    /// - **signed**: symmetric `[-q, +q]` with `q = p98(|v|)`. Symmetry is not
    ///   cosmetic — `BlueWhiteRed` is diverging and its white must sit at zero,
    ///   or the sign can no longer be read off the picture.
    /// - **non-negative**: `[p2, p98]`.
    ///
    /// It never emits an inverted domain: `max > min` is the extractor's guard,
    /// and a domain that fails it flattens the surface to the ramp's midpoint.
    pub fn fit(&self) -> Option<(f64, f64)> {
        if self.vertex_count < MIN_FIT_VERTICES {
            return None;
        }
        let (min, max) = if self.signed {
            (-self.abs_p98, self.abs_p98)
        } else {
            (self.p2, self.p98)
        };
        (max > min).then_some((min, max))
    }

    /// Approximate heap footprint — the two fixed-length bin vectors.
    pub fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + (self.bin_edges.capacity() + self.bin_weight.capacity()) * std::mem::size_of::<f64>()
    }
}

/// The colour distribution of `mesh` under `data`'s coloring, or `None` when
/// there is nothing to describe.
///
/// `None` for a `Phase`-coloured surface (there is no colour field), and for an
/// empty mesh. A surface painted by a field always gets a distribution, even one
/// below the vertex floor — the panel still plots it and says why the fit is
/// unavailable.
///
/// **The colour field is re-sampled here rather than threaded out of the
/// extractor.** It is the same one sample per vertex the albedo pass already
/// takes, and keeping it separate means the extractor's hot loop does not grow a
/// second output for a statistic only the panel reads.
pub fn surface_color_distribution(
    mesh: &SurfaceMesh,
    data: &IsosurfaceData,
) -> Option<SurfaceValueDistribution> {
    let IsosurfaceColoring::Field {
        field: color_field, ..
    } = &data.coloring
    else {
        return None;
    };
    if mesh.positions.is_empty() || mesh.indices.len() < 3 {
        return None;
    }

    let values: Vec<f64> = mesh
        .positions
        .iter()
        .map(|p| color_field.sample(p.as_dvec3()))
        .collect();
    let weights = vertex_areas(mesh);

    build(values, weights, signedness(color_field.as_ref(), mesh))
}

/// Whether the colour field takes negative values.
///
/// The field's own declaration wins when it has one — it covers the whole box,
/// and a signed field whose surface happens to sample only positive values is
/// still a signed field, so its ramp must stay centred on zero. An analytic
/// field declares nothing, and then the surface's own values are all there is.
fn signedness(color_field: &dyn ScalarField, mesh: &SurfaceMesh) -> bool {
    match color_field.value_range() {
        Some((min, _)) => min < 0.0,
        None => mesh
            .positions
            .iter()
            .any(|p| color_field.sample(p.as_dvec3()) < 0.0),
    }
}

/// A third of each incident triangle's area, per vertex — see the module docs on
/// why this is not a vertex count.
fn vertex_areas(mesh: &SurfaceMesh) -> Vec<f64> {
    let mut areas = vec![0.0f64; mesh.positions.len()];
    for triangle in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if a >= areas.len() || b >= areas.len() || c >= areas.len() {
            continue;
        }
        let pa: DVec3 = mesh.positions[a].as_dvec3();
        let pb: DVec3 = mesh.positions[b].as_dvec3();
        let pc: DVec3 = mesh.positions[c].as_dvec3();
        let third = (pb - pa).cross(pc - pa).length() * 0.5 / 3.0;
        areas[a] += third;
        areas[b] += third;
        areas[c] += third;
    }
    areas
}

/// Sort, accumulate, query, bin — the whole statistic in one pass over each.
///
/// Public to the crate's tests through
/// [`SurfaceValueDistribution::from_weighted_values`], so the area-weighting
/// rule can be pinned against a constructed answer.
fn build(values: Vec<f64>, weights: Vec<f64>, signed: bool) -> Option<SurfaceValueDistribution> {
    let vertex_count = values.len();
    let mut pairs: Vec<(f64, f64)> = values
        .iter()
        .copied()
        .zip(weights.iter().copied())
        .filter(|(value, weight)| value.is_finite() && weight.is_finite() && *weight > 0.0)
        .collect();
    if pairs.is_empty() {
        return None;
    }
    // Ascending by value. Unstable is fine: equal values are interchangeable and
    // the prefix sums do not care which came first.
    pairs.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));

    let total_area: f64 = pairs.iter().map(|(_, weight)| weight).sum();
    let value_min = pairs[0].0;
    let value_max = pairs[pairs.len() - 1].0;

    let p2 = weighted_percentile(&pairs, total_area, 0.02);
    let p50 = weighted_percentile(&pairs, total_area, 0.50);
    let p98 = weighted_percentile(&pairs, total_area, 0.98);

    // A separate ascending pass over the magnitudes: `p98(|v|)` is not
    // recoverable from the signed percentiles, because the two tails interleave
    // once the sign is folded away.
    let mut magnitudes: Vec<(f64, f64)> = pairs
        .iter()
        .map(|(value, weight)| (value.abs(), *weight))
        .collect();
    magnitudes.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let abs_p98 = weighted_percentile(&magnitudes, total_area, 0.98);

    let (bin_edges, bin_weight) = histogram(&pairs, value_min, value_max);

    Some(SurfaceValueDistribution {
        vertex_count,
        total_area,
        value_min,
        value_max,
        p2,
        p50,
        p98,
        abs_p98,
        signed,
        bin_edges,
        bin_weight,
    })
}

impl SurfaceValueDistribution {
    /// Build directly from per-vertex values and area weights.
    ///
    /// **A test seam, deliberately public.** The area-weighting rule is only
    /// falsifiable against a *constructed* answer — a surface whose two halves
    /// carry equal area but unequal triangle counts — and building that as a
    /// real marching-cubes mesh would test the extractor instead.
    pub fn from_weighted_values(values: Vec<f64>, weights: Vec<f64>, signed: bool) -> Option<Self> {
        build(values, weights, signed)
    }
}

/// The smallest value whose cumulative weight reaches `fraction` of the total.
///
/// `pairs` is ascending by value. A *step* answer rather than an interpolated
/// one, matching the level distribution's queries: every threshold between two
/// adjacent samples selects the same set, and returning one of the surface's own
/// values keeps the fitted domain a number the picture actually attains.
fn weighted_percentile(pairs: &[(f64, f64)], total: f64, fraction: f64) -> f64 {
    if pairs.is_empty() {
        return 0.0;
    }
    if total <= 0.0 {
        return pairs[pairs.len() / 2].0;
    }
    let target = fraction * total;
    let mut running = 0.0f64;
    for (value, weight) in pairs {
        running += weight;
        if running >= target {
            return *value;
        }
    }
    pairs[pairs.len() - 1].0
}

/// Linear, signed bins over `[lo, hi]`, weighted by area.
///
/// Linear rather than log because the surface range is narrow — a real ESP
/// envelope spans about `±0.08` — and because the axis has to cross zero, which
/// a log axis cannot do.
fn histogram(pairs: &[(f64, f64)], lo: f64, hi: f64) -> (Vec<f64>, Vec<f64>) {
    let (lo, hi) = if hi - lo > 0.0 {
        (lo, hi)
    } else {
        // Every vertex reads the same value. Pad so the bin width is finite; the
        // reported value range stays exact.
        (lo - DEGENERATE_PAD, hi + DEGENERATE_PAD)
    };
    let span = hi - lo;
    let edges: Vec<f64> = (0..=SURFACE_HISTOGRAM_BINS)
        .map(|index| lo + span * index as f64 / SURFACE_HISTOGRAM_BINS as f64)
        .collect();
    let mut weight = vec![0.0f64; SURFACE_HISTOGRAM_BINS];
    for (value, area) in pairs {
        let position = (value - lo) / span * SURFACE_HISTOGRAM_BINS as f64;
        let bin = if position >= SURFACE_HISTOGRAM_BINS as f64 {
            SURFACE_HISTOGRAM_BINS - 1
        } else if position > 0.0 {
            (position as usize).min(SURFACE_HISTOGRAM_BINS - 1)
        } else {
            0
        };
        weight[bin] += area;
    }
    (edges, weight)
}
