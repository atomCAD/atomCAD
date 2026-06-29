use crate::crystolecule::motif::Motif;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::geo_tree::GeoNode;
use crate::geo_tree::implicit_geometry::ImplicitGeometry3D;
use glam::f64::DVec3;
use std::collections::HashMap;

// ============================================================================
// Configuration Structures
// ============================================================================

/// Configuration for lattice filling operation.
/// Contains all the input data needed to perform the fill.
pub struct LatticeFillConfig {
    /// The unit cell defining the crystal lattice
    pub unit_cell: UnitCellStruct,

    /// The motif defining atomic positions and bonds within each unit cell
    pub motif: Motif,

    /// Map of parameter names to atomic numbers (e.g., "PRIMARY" -> 6 for carbon)
    pub parameter_element_values: HashMap<String, i16>,

    /// The geometry to fill with atoms (implicit surface)
    pub geometry: GeoNode,

    /// Offset applied in motif space (fractional lattice coordinates)
    pub motif_offset: DVec3,

    /// Ordered list of regions overriding the global (root) settings within
    /// their volumes. Empty = today's behavior (global settings everywhere).
    /// Regions apply in array order via a per-field painter's algorithm
    /// (see [`SettingsResolver`]).
    pub regions: Vec<RegionSpec>,
}

/// One materialization region: a volume (SDF) paired with per-field settings
/// overrides. Extracted from a `MaterializeRegion` record by the node layer
/// (Part B of `doc/design_blueprint_region_atom_edits.md`). An unset (`None`)
/// settings field means "inherit" — the value is resolved from earlier matching
/// regions and ultimately from the root settings.
pub struct RegionSpec {
    /// Region volume as an SDF, in absolute real (Å) coordinates.
    pub geometry: GeoNode,

    /// Membership tolerance in Å: a point `p` is in this region iff
    /// `geometry.implicit_eval_3d(p) <= margin`. Resolved value (record value
    /// or [`super::fill_algorithm::DEFAULT_REGION_MARGIN`]). Negative margins
    /// shrink the region (e.g. to exclude the boundary layer).
    pub margin: f64,

    pub passivate: Option<bool>,
    pub rm_single: Option<bool>,
    pub surf_recon: Option<bool>,
    pub invert_phase: Option<bool>,
    /// Remove zero-bond (lone) atoms; mirrors materialize's `rm_unbonded` (#363).
    pub rm_unbonded: Option<bool>,
}

/// Resolves effective lattice-fill settings at a point by layering an ordered
/// list of regions on top of the root settings (the node's own booleans).
///
/// Resolution is **per field**: for each settings field, walk the regions
/// last → first and take the value from the first (latest-in-array) region that
/// *contains* the point and has that field set; if none does, fall back to the
/// root. A region that sets only one field transparently inherits all others.
///
/// See §B3 of `doc/design_blueprint_region_atom_edits.md`.
pub struct SettingsResolver<'a> {
    pub root: &'a LatticeFillOptions,
    pub regions: &'a [RegionSpec],
}

impl SettingsResolver<'_> {
    /// Resolve the effective settings at point `p` via the per-field painter's
    /// algorithm. With no regions this returns the root settings unchanged.
    pub fn resolve_at(&self, p: DVec3) -> LatticeFillOptions {
        let mut opts = LatticeFillOptions {
            hydrogen_passivation: self.root.hydrogen_passivation,
            remove_unbonded_atoms: self.root.remove_unbonded_atoms,
            remove_single_bond_atoms: self.root.remove_single_bond_atoms,
            reconstruct_surface: self.root.reconstruct_surface,
            invert_phase: self.root.invert_phase,
        };

        if self.regions.is_empty() {
            return opts;
        }

        // Track which fields are still unresolved (root is the final fallback).
        let mut have_passivate = false;
        let mut have_rm_unbonded = false;
        let mut have_rm_single = false;
        let mut have_surf_recon = false;
        let mut have_invert = false;

        // Walk regions last → first; first containing region with a field set wins.
        for region in self.regions.iter().rev() {
            if have_passivate
                && have_rm_unbonded
                && have_rm_single
                && have_surf_recon
                && have_invert
            {
                break; // all fields resolved — no need to test further regions
            }

            // Membership: SDF at p must be within the region's margin.
            if region.geometry.implicit_eval_3d(&p) > region.margin {
                continue;
            }

            if !have_passivate {
                if let Some(v) = region.passivate {
                    opts.hydrogen_passivation = v;
                    have_passivate = true;
                }
            }
            if !have_rm_unbonded {
                if let Some(v) = region.rm_unbonded {
                    opts.remove_unbonded_atoms = v;
                    have_rm_unbonded = true;
                }
            }
            if !have_rm_single {
                if let Some(v) = region.rm_single {
                    opts.remove_single_bond_atoms = v;
                    have_rm_single = true;
                }
            }
            if !have_surf_recon {
                if let Some(v) = region.surf_recon {
                    opts.reconstruct_surface = v;
                    have_surf_recon = true;
                }
            }
            if !have_invert {
                if let Some(v) = region.invert_phase {
                    opts.invert_phase = v;
                    have_invert = true;
                }
            }
        }

        opts
    }

    /// `root_val || any region sets the field to Some(true)`. Replaces the old
    /// global gate: a step runs when it is enabled at the root *or* in any
    /// region (the per-position resolution then decides per atom).
    pub fn enabled_anywhere(
        &self,
        root_val: bool,
        get: impl Fn(&RegionSpec) -> Option<bool>,
    ) -> bool {
        root_val || self.regions.iter().any(|r| get(r) == Some(true))
    }
}

/// Options controlling the filling behavior
pub struct LatticeFillOptions {
    /// Whether to add hydrogen atoms to dangling bonds
    pub hydrogen_passivation: bool,

    /// Whether to remove unbonded (zero-bond) atoms before passivation
    pub remove_unbonded_atoms: bool,

    /// Whether to remove single-bond atoms before passivation
    pub remove_single_bond_atoms: bool,

    /// Whether to perform surface reconstruction (e.g., diamond (100) 2×1 dimers)
    pub reconstruct_surface: bool,

    pub invert_phase: bool,
}

/// Results from lattice filling operation
pub struct LatticeFillResult {
    /// The resulting atomic structure
    pub atomic_structure: crate::crystolecule::atomic_structure::AtomicStructure,
}

#[derive(Debug, Clone)]
pub struct LatticeFillStatistics {
    pub fill_box_calls: i32,
    pub do_fill_box_calls: i32,
    pub do_fill_box_total_size: DVec3,
    pub motif_cells_processed: i32,
    pub atoms: i32,
    pub bonds: i32,
    pub total_depth: f64,
    pub max_depth: f64,
    pub non_batched_evaluations: i32,
    pub batched_evaluations: i32,
    pub surface_reconstructions: i32,
}

impl Default for LatticeFillStatistics {
    fn default() -> Self {
        Self::new()
    }
}

impl LatticeFillStatistics {
    pub fn new() -> Self {
        LatticeFillStatistics {
            fill_box_calls: 0,
            do_fill_box_calls: 0,
            do_fill_box_total_size: DVec3::ZERO,
            motif_cells_processed: 0,
            atoms: 0,
            bonds: 0,
            total_depth: 0.0,
            max_depth: f64::NEG_INFINITY,
            non_batched_evaluations: 0,
            batched_evaluations: 0,
            surface_reconstructions: 0,
        }
    }

    pub fn get_average_do_fill_box_size(&self) -> DVec3 {
        if self.do_fill_box_calls > 0 {
            self.do_fill_box_total_size / (self.do_fill_box_calls as f64)
        } else {
            DVec3::ZERO
        }
    }

    pub fn get_average_depth(&self) -> f64 {
        if self.atoms > 0 {
            self.total_depth / (self.atoms as f64)
        } else {
            0.0
        }
    }

    pub fn log_statistics(&self) {
        println!("LatticeFill Statistics:");
        println!("  fill_box calls: {}", self.fill_box_calls);
        println!("  do_fill_box calls: {}", self.do_fill_box_calls);
        let avg_size = self.get_average_do_fill_box_size();
        println!(
            "  average do_fill_box size: ({:.3}, {:.3}, {:.3})",
            avg_size.x, avg_size.y, avg_size.z
        );
        println!("  motif cells processed: {}", self.motif_cells_processed);
        println!("  atoms added: {}", self.atoms);
        println!("  bonds created: {}", self.bonds);
        if self.surface_reconstructions > 0 {
            println!(
                "  surface reconstructions: {}",
                self.surface_reconstructions
            );
        }
        println!(
            "  evaluations: {} non-batched, {} batched",
            self.non_batched_evaluations, self.batched_evaluations
        );
        if self.atoms > 0 {
            println!("  average depth: {:.3} Å", self.get_average_depth());
            println!("  max depth: {:.3} Å", self.max_depth);
        }
    }
}
