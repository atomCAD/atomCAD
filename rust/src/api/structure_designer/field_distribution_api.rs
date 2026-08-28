//! Value-distribution readouts for the `isosurface` node's editor.
//!
//! See `doc/design_isosurface_level.md` Part 5 §Plumbing. The isosurface editor
//! is otherwise pure node-data editing; this module is the one place it reaches
//! into the kernel, because the histogram and the dual readout describe the
//! *field*, which no stored property knows anything about.
//!
//! **The resolved pair comes from the node's own evaluation**, not from a query
//! re-run here: the number the panel prints must be the number the extractor
//! draws at, and the only way to guarantee that is to read it off the
//! `IsosurfaceData` the node produced.
//!
//! **This module must stay listed in `flutter_rust_bridge.yaml`'s `rust_input`.**
//! A module missing from that list generates nothing, silently.

use crate::api::api_common::with_mut_cad_instance_or;
use atomcad_crystolecule::field::{LevelBasis, ScalarField, distribution::ValueDistribution};
use atomcad_display::isosurface::{MIN_FIT_VERTICES, SurfaceValueDistribution};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::nodes::isosurface::IsosurfaceNodeData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_scene::NodeOutput;
use std::sync::Arc;

/// Why the editor has no distribution to plot. Three of the four are **normal**
/// states, not errors: the panel says which one it is in a caption and keeps its
/// shape (Part 5 §Empty states).
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIDistributionState {
    /// A real distribution over stored samples — the only state with data.
    Available,
    /// Nothing is wired into the `field` pin.
    NoField,
    /// The field has no stored samples (analytic), so there is nothing to bin.
    AnalyticField,
    /// The upstream has not produced a field yet, or failed. `message` says
    /// what, when there is anything to say.
    NotEvaluated,
}

/// The `isosurface` level histogram, the cumulative curve, and the resolved
/// `(isovalue, fraction, basis)` for the node's current setting.
///
/// Bins are log-spaced over the field's **nonzero** magnitude range; exact zeros
/// have no bin and are reported by `zero_count` alone. Per-bin `mass` is
/// `Σ|v|^exponent`, never a sample count — a count-weighted plot is one vacuum
/// spike.
pub struct APIValueDistribution {
    /// Which of the four states below the rest of this struct describes.
    pub state: APIDistributionState,
    /// Free-text detail for `NotEvaluated` (the upstream error). Empty otherwise.
    pub message: String,

    /// Bin edges as **magnitudes** (not logs), ascending. `bin_mass.len() + 1`
    /// entries. Empty unless `state` is `Available`.
    pub bin_edges: Vec<f64>,
    /// Mass in each bin, ascending in magnitude.
    pub bin_mass: Vec<f64>,
    /// Share of the total mass at or above each edge, in `[0, 1]`, one entry per
    /// edge — the cumulative overlay, and the curve that relates the fraction
    /// slider's travel to the plot's `log10 |v|` axis.
    pub cumulative_at_or_above: Vec<f64>,
    /// Smallest nonzero `|v|`. `0.0` for an all-zero field.
    pub nonzero_min: f64,
    /// Largest nonzero `|v|`. `0.0` for an all-zero field.
    pub nonzero_max: f64,
    /// Samples that are exactly zero — a line under the plot, never a bin.
    pub zero_count: u64,
    /// `false` when the distribution was built from the log histogram rather
    /// than the exact sorted samples (a field above `EXACT_SAMPLE_LIMIT`), so
    /// the readout can say the numbers are accurate to a bin.
    pub is_exact: bool,

    /// The isovalue the surface is actually drawn at. Meaningless unless
    /// `has_resolved_level`.
    pub resolved_level: f64,
    /// What that isovalue encloses, as a share of `∫|v|`. Meaningless unless
    /// `has_resolved_fraction` — an analytic field has no distribution to read
    /// it from.
    pub resolved_fraction: f64,
    /// Whether `resolved_level` means anything.
    pub has_resolved_level: bool,
    /// Whether `resolved_fraction` means anything.
    pub has_resolved_fraction: bool,
    /// The `Auto` basis label (`non-negative, density-like`, `signed field`, …),
    /// empty outside `Auto`. Rendered verbatim: `AutoBasis::label` is the only
    /// place these strings live, so the panel and the pin readout cannot drift.
    pub auto_basis: String,

    /// Whether the node's stored **`level`** still encloses exactly what the
    /// surface currently encloses.
    ///
    /// The mode dropdown converts: switching to `Absolute` writes the resolved
    /// magnitude so the surface does not move. Done unconditionally that would
    /// mangle a typed constant, because `iso → f → iso` is **not** a bijection
    /// (design §The two queries are not a bijection) — `iso_for_fraction` can
    /// only return a *stored sample*, so a `0.002` that went through fraction
    /// mode comes back as `0.00200034…`. When this flag is set the stored number
    /// already describes the current surface, so the switch keeps it verbatim
    /// and the round trip is lossless.
    ///
    /// The comparison is on the **enclosed fraction**, not on the magnitude:
    /// every isovalue between two adjacent samples encloses the same set, and
    /// "same set" is what "the surface did not move" means here.
    pub stored_level_matches: bool,
    /// The same for the stored **`level_fraction`**, so a `Fraction → Absolute →
    /// Fraction` round trip returns the fraction the user typed rather than the
    /// tied-group overshoot.
    pub stored_fraction_matches: bool,
}

impl APIValueDistribution {
    fn empty(state: APIDistributionState, message: String) -> Self {
        Self {
            state,
            message,
            bin_edges: Vec::new(),
            bin_mass: Vec::new(),
            cumulative_at_or_above: Vec::new(),
            nonzero_min: 0.0,
            nonzero_max: 0.0,
            zero_count: 0,
            is_exact: true,
            resolved_level: 0.0,
            resolved_fraction: 0.0,
            has_resolved_level: false,
            has_resolved_fraction: false,
            auto_basis: String::new(),
            stored_level_matches: false,
            stored_fraction_matches: false,
        }
    }

    fn from_distribution(distribution: &ValueDistribution) -> Self {
        let histogram = distribution.histogram();
        let bins = histogram.bin_count();
        let bin_edges: Vec<f64> = (0..=bins).map(|index| histogram.edge(index)).collect();
        let total = histogram.total();
        // Normalized here rather than in Dart so the plot and the readout divide
        // by the same total; `total == 0` is an all-zero field, where every
        // point of the curve is honestly 0.
        let cumulative_at_or_above: Vec<f64> = histogram
            .mass_at_or_above()
            .iter()
            .map(|&mass| if total > 0.0 { mass / total } else { 0.0 })
            .collect();
        let (nonzero_min, nonzero_max) = distribution.nonzero_range().unwrap_or((0.0, 0.0));
        Self {
            state: APIDistributionState::Available,
            message: String::new(),
            bin_edges,
            bin_mass: histogram.mass().to_vec(),
            cumulative_at_or_above,
            nonzero_min,
            nonzero_max,
            zero_count: distribution.zero_count() as u64,
            is_exact: distribution.is_exact(),
            resolved_level: 0.0,
            resolved_fraction: 0.0,
            has_resolved_level: false,
            has_resolved_fraction: false,
            auto_basis: String::new(),
            stored_level_matches: false,
            stored_fraction_matches: false,
        }
    }

    fn with_resolved(
        mut self,
        level: f64,
        basis: LevelBasis,
        field: &dyn ScalarField,
        stored: &IsosurfaceNodeData,
    ) -> Self {
        self.resolved_level = level;
        self.has_resolved_level = true;
        let distribution = field.value_distribution();
        // Read back from the *resolved* level in every mode, including
        // `Fraction`, rather than echoing the requested `f`: the two differ by
        // the mass of the samples tied with the chosen isovalue, and what the
        // picture encloses is this one. Same rule as the pin readout's
        // `describe_isosurface_level`.
        if let Some(fraction) = distribution.and_then(|d| d.fraction_for_iso(level)) {
            self.resolved_fraction = fraction;
            self.has_resolved_fraction = true;
            // Both stored coordinates are checked against the *same* quantity —
            // the fraction the drawn surface encloses — so the two answers
            // cannot disagree about what "unchanged" means. Exact equality is
            // right rather than a tolerance: every value here comes out of the
            // same two queries over the same samples, so a match is a match.
            let d = distribution.expect("a fraction was resolved, so there is a distribution");
            self.stored_level_matches = d.fraction_for_iso(stored.level) == Some(fraction);
            self.stored_fraction_matches = d
                .iso_for_fraction(stored.level_fraction)
                .and_then(|iso| d.fraction_for_iso(iso))
                == Some(fraction);
        }
        if let LevelBasis::Auto(auto) = basis {
            self.auto_basis = auto.label().to_string();
        }
        self
    }
}

/// Build the plottable part from a field, or report why there is none.
///
/// `pub` so the `AnalyticField` empty state is testable: no node produces an
/// analytic field yet (a future Molden orbital will be the first), so the only
/// way to reach that arm is to hand one in directly.
#[flutter_rust_bridge::frb(ignore)]
pub fn describe_field(field: &Arc<dyn ScalarField>) -> APIValueDistribution {
    match field.value_distribution() {
        Some(distribution) => APIValueDistribution::from_distribution(distribution),
        None => APIValueDistribution::empty(
            APIDistributionState::AnalyticField,
            // Panel caption: a clause, not a paragraph. What it *means* is in
            // the node description.
            "Analytic field — no stored samples to plot.".to_string(),
        ),
    }
}

/// The distribution behind an `isosurface` node's **level** control, computed
/// against an explicit designer.
///
/// The FRB entry point below is a thin wrapper over this; the logic lives here
/// so it can be tested without the global `CAD_INSTANCE`.
///
/// `None` only when (`scope_path`, `node_id`) is not an `isosurface` node —
/// every genuine "nothing to show" is one of [`APIDistributionState`]'s three
/// empty states, so the panel can name which and keep its shape.
#[flutter_rust_bridge::frb(ignore)]
pub fn isosurface_level_distribution(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIValueDistribution> {
    // Guard on the node type first: the two evaluations below are meaningless
    // on anything else, and `None` is reserved for exactly this case. The stored
    // data is cloned out rather than borrowed because the evaluations below need
    // `designer` mutably.
    let stored = designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<IsosurfaceNodeData>()?
        .clone();

    // The node's own output carries the field *and* the resolved pair, so the
    // happy path is one pass. Anything else falls back to the `field` argument
    // alone (pin 0), which is what distinguishes "no field wired" from "the
    // level did not resolve" — the histogram is still worth drawing in the
    // second case, just without a marker.
    match designer.evaluate_node_output(scope_path, node_id, 0) {
        NetworkResult::Isosurface(data) => Some(describe_field(&data.field).with_resolved(
            data.level,
            data.level_basis,
            data.field.as_ref(),
            &stored,
        )),
        other => {
            let level_error = match other {
                NetworkResult::Error(message) => message,
                _ => String::new(),
            };
            Some(
                match designer.evaluate_node_argument(scope_path, node_id, 0) {
                    NetworkResult::ScalarField(field) => describe_field(&field),
                    NetworkResult::None => {
                        APIValueDistribution::empty(APIDistributionState::NoField, String::new())
                    }
                    NetworkResult::Error(message) => {
                        APIValueDistribution::empty(APIDistributionState::NotEvaluated, message)
                    }
                    _ => {
                        APIValueDistribution::empty(APIDistributionState::NotEvaluated, level_error)
                    }
                },
            )
        }
    }
}

/// Flutter entry point for [`isosurface_level_distribution`].
///
/// Refetch on **any** network-change notification, not only on selection
/// change: rewiring `field` or reloading the `.cube` must invalidate this, and
/// the failure mode — a correct-looking histogram belonging to the previous
/// field — is silent. The Flutter side gets that for free by calling this from
/// `build`, which Provider re-runs on every `notifyListeners`.
#[flutter_rust_bridge::frb(sync)]
pub fn get_isosurface_level_distribution(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIValueDistribution> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                isosurface_level_distribution(
                    &mut cad_instance.structure_designer,
                    &scope_path,
                    node_id,
                )
            },
            None,
        )
    }
}

// ============================================================================
// The colour domain — `doc/design_isosurface_level.md` Part 4
// ============================================================================

/// Why the colour group has no distribution to plot.
///
/// **Four states, not three**, and the extra one is what distinguishes this
/// from the level's [`APIDistributionState`]: the colour distribution is a
/// statistic of the *extracted mesh*, which does not exist until the display
/// conversion runs, so a node nobody is displaying has nothing to report.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APISurfaceDistributionState {
    /// A real distribution over the surface's vertices.
    Available,
    /// Nothing is wired into `color_field`, so the surface is painted by phase
    /// and there is no second quantity to describe.
    NoColorField,
    /// The surface has not been extracted: the node is not displayed, the
    /// extraction was refused, or the level encloses nothing. `message` says
    /// which.
    NotExtracted,
}

/// The colour field's distribution over one extracted surface, and the domain a
/// fit would write.
///
/// **A separate type from [`APIValueDistribution`], deliberately.** Every axis
/// of the two differs — population, weight, sign, parameter shape, query — and
/// forcing a signed area-weighted interval statistic through a magnitude-mass
/// struct would misname every field. See `doc/design_isosurface_level.md`
/// Part 4's table.
pub struct APISurfaceValueDistribution {
    /// Which of the three states below the rest of this struct describes.
    pub state: APISurfaceDistributionState,
    /// Panel caption for the two empty states — a clause, not a paragraph.
    /// Empty when `state` is `Available`.
    pub message: String,

    /// Bin edges as **signed values**, ascending and linearly spaced;
    /// `bin_weight.len() + 1` entries. Linear rather than log because a colour
    /// domain crosses zero, which no log axis can.
    pub bin_edges: Vec<f64>,
    /// Surface **area** in each bin, not a vertex count — marching cubes puts
    /// vertices where the geometry is busy, not where the area is.
    pub bin_weight: Vec<f64>,
    /// Smallest colour value anywhere on the surface.
    pub value_min: f64,
    /// Largest colour value anywhere on the surface.
    pub value_max: f64,
    /// Area-weighted 2nd percentile — the span fit's low end, and the axis's.
    pub p2: f64,
    /// Area-weighted 98th percentile.
    pub p98: f64,
    /// Surface vertices the statistics were taken over.
    pub vertex_count: u64,
    /// Whether the colour field takes negative values, which is what picks the
    /// symmetric fit over the span one.
    pub is_signed: bool,

    /// Low end of the domain the fit button would write. Meaningless unless
    /// `can_fit`.
    pub fit_min: f64,
    /// High end of the same. Meaningless unless `can_fit`.
    pub fit_max: f64,
    /// Whether the fit button is offerable at all.
    pub can_fit: bool,
    /// Why not, when `can_fit` is false — the button's tooltip. Empty otherwise.
    pub fit_blocked_reason: String,
}

impl APISurfaceValueDistribution {
    fn empty(state: APISurfaceDistributionState, message: &str) -> Self {
        Self {
            state,
            message: message.to_string(),
            bin_edges: Vec::new(),
            bin_weight: Vec::new(),
            value_min: 0.0,
            value_max: 0.0,
            p2: 0.0,
            p98: 0.0,
            vertex_count: 0,
            is_signed: false,
            fit_min: 0.0,
            fit_max: 0.0,
            can_fit: false,
            fit_blocked_reason: message.to_string(),
        }
    }

    fn from_distribution(distribution: &SurfaceValueDistribution) -> Self {
        let (value_min, value_max) = distribution.value_range();
        let fit = distribution.fit();
        // The refusal is a sentence rather than a flag because the button's
        // tooltip is the only place it can be said: a surface too coarse to fit
        // still plots, so nothing else on screen would explain a dead button.
        let fit_blocked_reason = match fit {
            Some(_) => String::new(),
            None if distribution.vertex_count() < MIN_FIT_VERTICES => format!(
                "Surface too coarse to fit: {} vertices, {} needed. \
                 Raise the isosurface extraction quality in Preferences.",
                distribution.vertex_count(),
                MIN_FIT_VERTICES
            ),
            None => "The colour field is constant over this surface — \
                     there is no range to fit."
                .to_string(),
        };
        Self {
            state: APISurfaceDistributionState::Available,
            message: String::new(),
            bin_edges: distribution.bin_edges().to_vec(),
            bin_weight: distribution.bin_weight().to_vec(),
            value_min,
            value_max,
            p2: distribution.p2(),
            p98: distribution.p98(),
            vertex_count: distribution.vertex_count() as u64,
            is_signed: distribution.is_signed(),
            fit_min: fit.map_or(0.0, |(min, _)| min),
            fit_max: fit.map_or(0.0, |(_, max)| max),
            can_fit: fit.is_some(),
            fit_blocked_reason,
        }
    }
}

/// The distribution behind an `isosurface` node's **colour domain**, computed
/// against an explicit designer.
///
/// **This is a scene read, not an evaluation**, and that is the whole shape of
/// it. The mesh the statistic describes is produced by the display conversion
/// (`generate_isosurface_output` -> `extract_isosurface`), one stage after the
/// value the level distribution's probe re-evaluates — so `evaluate_node_output`
/// would hand back the `IsosurfaceData` *specification*, which has no vertices.
/// Reading the scene instead also means there is no `print_log` / profiling
/// state to save and restore, and no evaluation cost per panel rebuild.
///
/// `None` only when (`scope_path`, `node_id`) is not an `isosurface` node.
#[flutter_rust_bridge::frb(ignore)]
pub fn isosurface_color_distribution(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APISurfaceValueDistribution> {
    designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<IsosurfaceNodeData>()?;

    let scene_node = designer
        .last_generated_structure_designer_scene
        .node_data
        .get(&NodeRef::scoped(scope_path, node_id));

    Some(match scene_node {
        None => APISurfaceValueDistribution::empty(
            APISurfaceDistributionState::NotExtracted,
            "Display the node to measure its surface.",
        ),
        Some(scene) => match (&scene.surface_color_distribution, &scene.output) {
            (Some(distribution), _) => APISurfaceValueDistribution::from_distribution(distribution),
            (None, NodeOutput::Isosurface(mesh)) if mesh.is_empty() => {
                APISurfaceValueDistribution::empty(
                    APISurfaceDistributionState::NotExtracted,
                    "This level encloses nothing — the surface is empty.",
                )
            }
            (None, NodeOutput::Isosurface(_)) => APISurfaceValueDistribution::empty(
                APISurfaceDistributionState::NoColorField,
                "Wire `color_field` to paint the surface by a second quantity.",
            ),
            // Extraction produced no mesh at all — the cell-budget refusal is
            // the reachable case, and it already reports itself as a red error
            // on the node, so this caption stays short.
            (None, _) => APISurfaceValueDistribution::empty(
                APISurfaceDistributionState::NotExtracted,
                "The surface was not extracted.",
            ),
        },
    })
}

/// Flutter entry point for [`isosurface_color_distribution`].
///
/// Called from `build` like its level sibling, so any network change
/// invalidates it. Unlike the sibling it is a pure read of the last generated
/// scene, so the cost is a hash lookup and two vector clones.
#[flutter_rust_bridge::frb(sync)]
pub fn get_isosurface_color_distribution(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APISurfaceValueDistribution> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                isosurface_color_distribution(
                    &cad_instance.structure_designer,
                    &scope_path,
                    node_id,
                )
            },
            None,
        )
    }
}
