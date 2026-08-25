//! Where the marching cubes are — the resolution policy of
//! `doc/design_isosurface_node.md` §Resolution.
//!
//! **Extraction runs in the field's own index space, on
//! [`GridGeometry::axes`] — never on [`GridGeometry::spacing`].** That method
//! returns three axis *lengths* and its own doc comment says it is exact only
//! for an axis-aligned grid; the `.cube` format permits shear, and rebuilding
//! an axis-aligned lattice out of three lengths would silently rotate a sheared
//! field's samples into the wrong places while reintroducing the eight-value
//! trilinear blending the fidelity fast path exists to avoid. A wrong surface,
//! from a file that loaded without complaint.
//!
//! Marching index space instead makes the axis-aligned case fall out unchanged
//! and the sheared case correct, for the price of carrying `axes` rather than
//! three floats.

use atomcad_crystolecule::field::{FieldBounds, GridGeometry, ScalarField};
use glam::{DMat3, DVec3};

use super::ExtractionSettings;

/// The cell grid one extraction marches over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lattice {
    /// The field reports a [`ScalarField::native_grid`]. Cell `(i, j, k)` is
    /// the parallelepiped spanned by `grid.axes[a] / subdiv`, cornered at
    /// `grid.origin + sum_a grid.axes[a] * (index_a / subdiv)`.
    ///
    /// At `subdiv == 1` the cells are the grid's own and every corner sample is
    /// a stored value read verbatim — the contract's fidelity fast path. That
    /// is exactly why the quality preference is an **integer subdivision**
    /// rather than a free divisor: a non-integer factor would put every corner
    /// between stored samples even at "quality 1.0".
    Native { grid: GridGeometry, subdiv: u32 },
    /// The field reports no native grid (every analytic field). Axis-aligned
    /// cubes of `spacing` filling [`ScalarField::suggested_bounds`].
    Fallback { bounds: FieldBounds, spacing: f64 },
}

impl Lattice {
    /// Pick the lattice for a field, following the preferences.
    ///
    /// The quality multiplier stays an `f64` for UI continuity and is rounded
    /// here; values below `1` do **not** coarsen, because a sampled field's own
    /// grid is the floor.
    pub fn choose(field: &dyn ScalarField, settings: &ExtractionSettings) -> Self {
        match field.native_grid() {
            Some(grid) => Lattice::Native {
                grid,
                subdiv: round_subdiv(settings.quality_multiplier),
            },
            None => Lattice::Fallback {
                bounds: field.suggested_bounds(),
                spacing: settings.fallback_spacing,
            },
        }
    }

    /// Number of **cells** along each axis.
    ///
    /// Cells span adjacent sample points, so a `Native` grid of `dims` yields
    /// `(dims - 1) * subdiv` — the node-centered bounds convention of
    /// [`GridGeometry::bounds`] makes that exact. An axis of `dims == 1`
    /// contributes zero cells, which is a legal (if empty) extraction rather
    /// than an error: `SampledField::new` accepts such a grid, and only a
    /// dimension of `0` is rejected.
    pub fn cell_dims(&self) -> [usize; 3] {
        match self {
            Lattice::Native { grid, subdiv } => {
                std::array::from_fn(|a| grid.dims[a].saturating_sub(1) * *subdiv as usize)
            }
            Lattice::Fallback { bounds, spacing } => {
                let size = bounds.size();
                std::array::from_fn(|a| {
                    if !spacing.is_finite() || *spacing <= 0.0 || !size[a].is_finite() {
                        return 0;
                    }
                    let n = (size[a] / spacing).ceil();
                    if n <= 0.0 { 0 } else { n as usize }
                })
            }
        }
    }

    /// Total cell count, the quantity `isosurface_cell_budget` is checked
    /// against. `u128` so the pre-flight check cannot itself overflow on a
    /// pathological grid.
    pub fn cell_count(&self) -> u128 {
        let dims = self.cell_dims();
        dims[0] as u128 * dims[1] as u128 * dims[2] as u128
    }

    /// Real-space position of lattice corner `(i, j, k)`, Ångström.
    ///
    /// Deterministic to the bit for a given corner: two cells sharing a corner
    /// call this with the same integers and get identical coordinates, which is
    /// what lets the coincident-vertex merge key on exact positions.
    pub fn corner_position(&self, i: usize, j: usize, k: usize) -> DVec3 {
        match self {
            Lattice::Native { grid, subdiv } => {
                let s = *subdiv as f64;
                grid.origin
                    + grid.axes[0] * (i as f64 / s)
                    + grid.axes[1] * (j as f64 / s)
                    + grid.axes[2] * (k as f64 / s)
            }
            Lattice::Fallback { bounds, spacing } => {
                bounds.min + DVec3::new(i as f64, j as f64, k as f64) * *spacing
            }
        }
    }

    /// Length of the shortest cell edge, Ångström — the scale the
    /// coincident-vertex merge quantizes against.
    pub fn cell_extent(&self) -> f64 {
        match self {
            Lattice::Native { grid, subdiv } => grid
                .axes
                .iter()
                .map(|axis| axis.length() / *subdiv as f64)
                .fold(f64::INFINITY, f64::min),
            Lattice::Fallback { spacing, .. } => *spacing,
        }
    }

    /// Whether the index-space to world map mirrors, i.e. `det(m) < 0` for
    /// `m = mat3(axes[0], axes[1], axes[2])`.
    ///
    /// A left-handed axis triple turns triangles emitted counter-clockwise in
    /// index space into clockwise ones in world space, inverting the
    /// front/back-face pairing the two-pass draw depends on — so the caller
    /// flips the vertex order when this is true. `.cube` writers do emit
    /// left-handed triples. Same class of bug, and the same fix, as the csgrs
    /// `det < 0` winding correction in `structure_invert`.
    pub fn flips_winding(&self) -> bool {
        match self {
            Lattice::Native { grid, .. } => {
                DMat3::from_cols(grid.axes[0], grid.axes[1], grid.axes[2]).determinant() < 0.0
            }
            // Axis-aligned cubes with positive spacing: always right-handed.
            Lattice::Fallback { .. } => false,
        }
    }
}

/// Integer subdivision from the quality preference: rounded, clamped to `>= 1`,
/// and defensive about a garbage value reaching it from a hand-edited
/// preferences file.
fn round_subdiv(quality_multiplier: f64) -> u32 {
    if !quality_multiplier.is_finite() || quality_multiplier < 1.0 {
        return 1;
    }
    // `as` saturates, so an absurd multiplier lands on u32::MAX rather than
    // wrapping — and the cell budget refuses it a moment later.
    quality_multiplier.round() as u32
}
