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
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::nodes::isosurface::IsosurfaceNodeData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
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
