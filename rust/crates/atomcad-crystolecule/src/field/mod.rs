//! Volumetric scalar fields — the `ScalarField` contract and its sampled
//! implementation.
//!
//! A scalar field is a function of 3D real space returning one number per
//! point: a molecular orbital amplitude, an electron density, an electrostatic
//! potential. It arrives either as samples on a regular grid (a `.cube` file,
//! see [`crate::io::cube_loader`]) or, one day, as an analytic expression
//! evaluated from a basis set (a `.molden` file).
//!
//! **Every coordinate crossing this interface is real-space Ångström**, matching
//! [`crate::atomic_structure::AtomicStructure`]. Each loader converts from its
//! file's units exactly once, at load time; no consumer ever sees Bohr. Field
//! *values*, by contrast, are passed through unconverted in whatever atomic unit
//! the source quantity uses — converting them would invalidate every published
//! threshold convention in the chemistry literature.
//!
//! Design doc: `doc/design_scalar_fields.md`.

use glam::{DMat3, DVec3};
use thiserror::Error;

/// Fallback finite-difference step for [`ScalarField::gradient`], Ångström.
///
/// Used only when a field reports no [`ScalarField::native_grid`] and does not
/// override `gradient`. Comfortably below any spacing a cube writer produces
/// (PySCF's default 80^3 box lands near 0.15-0.20 Å) and far above `f64`
/// cancellation noise.
pub const DEFAULT_GRADIENT_STEP: f64 = 0.05;

/// How far outside a sampled field's box a point may sit and still be treated
/// as *on* the boundary, in fractional-index units scaled by the axis span.
///
/// Without it the out-of-bounds rule is a knife edge, and the last sample plane
/// of a real file is unreachable: a `.cube` writer emits its step vector in
/// Bohr with six decimals, so converting back to Ångström reproduces the
/// nominal spacing only to ~1e-7 relative. On a grid a user believes has 1.0 Å
/// spacing, the outermost plane then lands a few times 1e-7 Å *past* the box
/// and `sample` returns `0.0` instead of the stored value — a jump of the full
/// data range from a rounding error the user cannot see. Scaling by the span
/// keeps the guard effective on the far face of a large grid, where the same
/// relative error is proportionally larger in index units.
///
/// A millionth of a grid's extent is far below any physically meaningful
/// distance, so nothing legitimately out of bounds is captured by it.
pub const BOUNDARY_INDEX_TOLERANCE: f64 = 1e-6;

/// Things that can be wrong with a grid description.
#[derive(Debug, Error, PartialEq)]
pub enum FieldError {
    #[error("grid dimension along axis {axis} is zero")]
    ZeroDimension { axis: usize },

    #[error("grid {dims:?} needs {expected} samples, got {actual}")]
    SampleCountMismatch {
        dims: [usize; 3],
        expected: usize,
        actual: usize,
    },

    #[error("grid axis vectors are degenerate (not invertible)")]
    DegenerateAxes,

    #[error("sample {index} is not finite ({value})")]
    NonFiniteSample { index: usize, value: f32 },
}

/// An axis-aligned box in real space, Ångström.
///
/// No general AABB type exists in the workspace to reuse. If one is wanted
/// elsewhere later this can move to `atomcad-util`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldBounds {
    pub min: DVec3,
    pub max: DVec3,
}

impl FieldBounds {
    pub fn new(min: DVec3, max: DVec3) -> Self {
        Self { min, max }
    }

    /// Inclusive on both faces — a point exactly on the boundary is inside.
    pub fn contains(&self, point: DVec3) -> bool {
        point.x >= self.min.x
            && point.y >= self.min.y
            && point.z >= self.min.z
            && point.x <= self.max.x
            && point.y <= self.max.y
            && point.z <= self.max.z
    }

    pub fn size(&self) -> DVec3 {
        self.max - self.min
    }

    pub fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }
}

/// Where a sampled field's samples sit in real space, Ångström.
///
/// General enough for a sheared grid even though PySCF only ever writes
/// axis-aligned ones — the `.cube` format permits shear, and the generality
/// costs one matrix instead of three scalars.
///
/// This is what [`ScalarField::native_grid`] hands back, so it is the complete
/// answer to "where exactly are the stored samples": origin, three axis vectors
/// and counts, with no convention left implicit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridGeometry {
    /// Position of sample `(0, 0, 0)`, Ångström. **Node-centered**: this IS a
    /// sample point, not a voxel corner.
    pub origin: DVec3,
    /// Step vectors along the three grid axes, Ångström.
    pub axes: [DVec3; 3],
    /// Sample counts along each axis.
    pub dims: [usize; 3],
}

impl GridGeometry {
    /// Total number of samples the grid describes.
    pub fn sample_count(&self) -> usize {
        self.dims[0] * self.dims[1] * self.dims[2]
    }

    /// Per-axis step lengths, Ångström. Convenience for consumers choosing their
    /// own lattice; exact only for an axis-aligned grid, so a consumer that must
    /// handle shear uses [`GridGeometry::axes`] directly.
    pub fn spacing(&self) -> DVec3 {
        DVec3::new(
            self.axes[0].length(),
            self.axes[1].length(),
            self.axes[2].length(),
        )
    }

    /// Box through the outermost sample points.
    ///
    /// Cube grids are node-centered, so with `dims = [nx, ny, nz]` the box spans
    /// `nx - 1` steps while containing `nx` samples — it is **not** extended by
    /// half a voxel. Both conventions exist in the wild and picking one silently
    /// is how a half-voxel offset creeps in.
    ///
    /// For a sheared grid the parallelepiped through the outermost samples is
    /// not itself axis-aligned, so this returns its axis-aligned hull — computed
    /// over all eight corners, which reduces to `origin .. origin + (dims-1)*axes`
    /// in the axis-aligned case.
    pub fn bounds(&self) -> FieldBounds {
        let edges = [
            self.axes[0] * self.dims[0].saturating_sub(1) as f64,
            self.axes[1] * self.dims[1].saturating_sub(1) as f64,
            self.axes[2] * self.dims[2].saturating_sub(1) as f64,
        ];
        let mut min = self.origin;
        let mut max = self.origin;
        for mask in 0..8u32 {
            let mut corner = self.origin;
            for (axis, edge) in edges.iter().enumerate() {
                if mask & (1 << axis) != 0 {
                    corner += *edge;
                }
            }
            min = min.min(corner);
            max = max.max(corner);
        }
        FieldBounds::new(min, max)
    }

    /// Real-space position of sample `(i, j, k)`.
    pub fn sample_position(&self, i: usize, j: usize, k: usize) -> DVec3 {
        self.origin + self.axes[0] * i as f64 + self.axes[1] * j as f64 + self.axes[2] * k as f64
    }
}

/// A scalar function of 3D real space — sampled from a grid, or evaluated
/// analytically. Coordinates are real-space Ångström; values are passed through
/// in whatever atomic unit the source quantity uses.
///
/// `Send + Sync` is required so that sampling consumers can evaluate in parallel
/// batches, mirroring `atomcad_geo_tree`'s `BatchedImplicitEvaluator`. Nothing
/// evaluates in parallel today — the node evaluator is single-threaded — but the
/// bound is free to hold now and expensive to add later.
pub trait ScalarField: Send + Sync + std::fmt::Debug {
    /// Value at `point`. Outside [`ScalarField::data_bounds`] this returns
    /// exactly `0.0`. Never errors, never returns NaN.
    ///
    /// The out-of-bounds rule is `0.0`, not an error: a finite cube box is a
    /// *window* onto a field that decays to zero, so `0.0` is the physically
    /// correct answer just outside it, and it keeps every consumer free of an
    /// error path in its innermost loop.
    fn sample(&self, point: DVec3) -> f64;

    /// Batched evaluation. Precondition: `out.len() == points.len()`.
    ///
    /// The default loops over [`ScalarField::sample`]; implementations with
    /// per-batch setup cost (Gaussian evaluation) override this.
    fn sample_batch(&self, points: &[DVec3], out: &mut [f64]) {
        debug_assert_eq!(points.len(), out.len());
        for (p, o) in points.iter().zip(out.iter_mut()) {
            *o = self.sample(*p);
        }
    }

    /// Gradient at `point`, in value-units per Ångström. Used for isosurface
    /// normals.
    ///
    /// The default is a central difference stepped at half the native grid
    /// spacing when [`ScalarField::native_grid`] is `Some`, and at
    /// [`DEFAULT_GRADIENT_STEP`] otherwise. Both concrete implementations are
    /// expected to override it, so this is a fallback, not the norm.
    fn gradient(&self, point: DVec3) -> DVec3 {
        let step = match self.native_grid() {
            Some(grid) => {
                let half = grid.spacing() * 0.5;
                DVec3::new(
                    if half.x > 0.0 {
                        half.x
                    } else {
                        DEFAULT_GRADIENT_STEP
                    },
                    if half.y > 0.0 {
                        half.y
                    } else {
                        DEFAULT_GRADIENT_STEP
                    },
                    if half.z > 0.0 {
                        half.z
                    } else {
                        DEFAULT_GRADIENT_STEP
                    },
                )
            }
            None => DVec3::splat(DEFAULT_GRADIENT_STEP),
        };
        DVec3::new(
            central_difference(self, point, DVec3::X * step.x),
            central_difference(self, point, DVec3::Y * step.y),
            central_difference(self, point, DVec3::Z * step.z),
        )
    }

    /// Region outside which [`ScalarField::sample`] is defined to return `0.0`.
    /// `None` = defined everywhere (analytic sources).
    fn data_bounds(&self) -> Option<FieldBounds>;

    /// Box a consumer should sample when it has no better instruction.
    ///
    /// For a sampled source this is the box through the outermost sample points
    /// (see [`GridGeometry::bounds`]). For an analytic source it is derived from
    /// atom positions plus a margin — which is why this is not `Option`: only
    /// the field knows *where* to look.
    fn suggested_bounds(&self) -> FieldBounds;

    /// The field's intrinsic sample lattice, when it has one. `Some` for a
    /// sampled source; `None` for an analytic source, which has no preferred
    /// lattice at all.
    ///
    /// A consumer that wants zero information loss should use this grid verbatim
    /// when it is `Some`: sampling a stored field *anywhere else* blends eight
    /// stored values per point, which smooths the field for no gain.
    ///
    /// This is a **fidelity fast path, not the interface.** Every consumer must
    /// still work correctly from `sample` alone when this returns `None`; a
    /// consumer that only functions when it is `Some` is broken and will fail
    /// against the first analytic field it meets.
    fn native_grid(&self) -> Option<GridGeometry>;

    /// Minimum and maximum over the field's data. `None` for an analytic source,
    /// which has no data to scan until something samples it.
    ///
    /// This stands in for a semantic type tag. `min >= 0` means the field is
    /// non-negative, so a consumer can skip negative-level extraction; the span
    /// sets a log slider's bounds. Both are *derived*, so they work for any
    /// scalar quantity chemistry produces, not just the three or four anyone
    /// thought to name.
    fn value_range(&self) -> Option<(f64, f64)>;
}

/// One axis of the trait's default central difference.
fn central_difference<F: ScalarField + ?Sized>(field: &F, point: DVec3, offset: DVec3) -> f64 {
    let h = offset.length();
    if h <= 0.0 {
        return 0.0;
    }
    (field.sample(point + offset) - field.sample(point - offset)) / (2.0 * h)
}

/// A field stored as samples on a regular grid, with trilinear interpolation
/// between them.
///
/// `f32` storage, not `f64`: the source data has nothing like `f64` precision,
/// and halving the footprint matters when a single field is a few megabytes.
/// Interpolation is done in `f64`.
#[derive(Clone)]
pub struct SampledField {
    grid: GridGeometry,
    /// Inverse of the 3x3 matrix whose columns are `grid.axes`, cached at
    /// construction so `sample` does not rebuild it per call. Lives here and not
    /// on [`GridGeometry`], which stays a small `Copy` description of where the
    /// samples are.
    inv_basis: DMat3,
    /// Row-major with the LAST axis contiguous: index
    /// `(i * dims[1] + j) * dims[2] + k`. This matches the `.cube` traversal
    /// order, so the loader fills it sequentially with no transposition.
    samples: Vec<f32>,
    /// Min and max over `samples`.
    value_range: (f64, f64),
}

/// Prints a summary — dims and value range — never the samples themselves.
impl std::fmt::Debug for SampledField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SampledField {{ dims: {}x{}x{}, origin: {}, spacing: {}, range: {:.6e}..{:.6e} }}",
            self.grid.dims[0],
            self.grid.dims[1],
            self.grid.dims[2],
            self.grid.origin,
            self.grid.spacing(),
            self.value_range.0,
            self.value_range.1,
        )
    }
}

impl SampledField {
    /// Build a field from a grid description and its samples, in the layout
    /// documented on [`SampledField::samples_slice`].
    ///
    /// Validates dimensions, sample count, axis invertibility and finiteness,
    /// and computes the value range, in one pass over the samples.
    pub fn new(grid: GridGeometry, samples: Vec<f32>) -> Result<Self, FieldError> {
        for (axis, &n) in grid.dims.iter().enumerate() {
            if n == 0 {
                return Err(FieldError::ZeroDimension { axis });
            }
        }
        let expected = grid.sample_count();
        if samples.len() != expected {
            return Err(FieldError::SampleCountMismatch {
                dims: grid.dims,
                expected,
                actual: samples.len(),
            });
        }

        let basis = DMat3::from_cols(grid.axes[0], grid.axes[1], grid.axes[2]);
        if basis.determinant().abs() < 1e-30 {
            return Err(FieldError::DegenerateAxes);
        }
        let inv_basis = basis.inverse();

        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for (index, &value) in samples.iter().enumerate() {
            if !value.is_finite() {
                return Err(FieldError::NonFiniteSample { index, value });
            }
            let value = value as f64;
            if value < min {
                min = value;
            }
            if value > max {
                max = value;
            }
        }

        Ok(Self {
            grid,
            inv_basis,
            samples,
            value_range: (min, max),
        })
    }

    /// The grid this field is stored on.
    pub fn grid(&self) -> GridGeometry {
        self.grid
    }

    /// Raw samples, row-major with the LAST axis contiguous: sample `(i, j, k)`
    /// is at index `(i * dims[1] + j) * dims[2] + k`.
    pub fn samples_slice(&self) -> &[f32] {
        &self.samples
    }

    /// Stored value at grid indices, without interpolation. Panics on
    /// out-of-range indices — this is the raw accessor.
    pub fn sample_at_index(&self, i: usize, j: usize, k: usize) -> f64 {
        self.samples[self.flat_index(i, j, k)] as f64
    }

    #[inline]
    fn flat_index(&self, i: usize, j: usize, k: usize) -> usize {
        (i * self.grid.dims[1] + j) * self.grid.dims[2] + k
    }

    /// Fractional index coordinates of a real-space point: `(0,0,0)` is sample
    /// `(0,0,0)` and `(1,0,0)` is sample `(1,0,0)`.
    #[inline]
    fn fractional_index(&self, point: DVec3) -> DVec3 {
        self.inv_basis * (point - self.grid.origin)
    }

    /// Split one fractional coordinate into `(lower index, blend factor)`, or
    /// `None` when it falls outside the stored range. The `is_finite` guard is
    /// load-bearing: a NaN satisfies neither comparison and would otherwise slip
    /// through as an in-range index.
    ///
    /// The boundary carries a tolerance of [`BOUNDARY_INDEX_TOLERANCE`] *of the
    /// axis span* — see that constant for why a knife-edge comparison here is
    /// not good enough.
    #[inline]
    fn axis_cell(f: f64, n: usize) -> Option<(usize, f64)> {
        let last = (n - 1) as f64;
        let tolerance = BOUNDARY_INDEX_TOLERANCE * last.max(1.0);
        if !f.is_finite() || f < -tolerance || f > last + tolerance {
            return None;
        }
        // Inside, but possibly a hair past a face: snap onto it so the blend
        // factors stay in [0, 1] and the corner indices stay in range.
        let f = f.clamp(0.0, last);
        if n == 1 {
            return Some((0, 0.0));
        }
        let i0 = (f.floor() as usize).min(n - 2);
        Some((i0, f - i0 as f64))
    }

    /// Which cell a point lands in, or `None` when it is outside the grid.
    #[inline]
    fn locate(&self, point: DVec3) -> Option<[(usize, f64); 3]> {
        let f = self.fractional_index(point);
        Some([
            Self::axis_cell(f.x, self.grid.dims[0])?,
            Self::axis_cell(f.y, self.grid.dims[1])?,
            Self::axis_cell(f.z, self.grid.dims[2])?,
        ])
    }

    /// Trilinear blend of an arbitrary per-corner quantity — used for both the
    /// value and the gradient, which differ only in what sits at each corner.
    #[inline]
    fn trilinear<T, G>(cell: [(usize, f64); 3], corner: G) -> T
    where
        G: Fn(usize, usize, usize) -> T,
        T: std::ops::Mul<f64, Output = T> + std::ops::Add<Output = T>,
    {
        let (i0, tx) = cell[0];
        let (j0, ty) = cell[1];
        let (k0, tz) = cell[2];
        // Staying on the lower index when the blend factor is exactly zero keeps
        // a 1-wide axis (where there is no `i0 + 1`) in range; every term that
        // would have used the upper corner carries weight zero anyway.
        let i1 = if tx == 0.0 { i0 } else { i0 + 1 };
        let j1 = if ty == 0.0 { j0 } else { j0 + 1 };
        let k1 = if tz == 0.0 { k0 } else { k0 + 1 };
        corner(i0, j0, k0) * ((1.0 - tx) * (1.0 - ty) * (1.0 - tz))
            + corner(i0, j0, k1) * ((1.0 - tx) * (1.0 - ty) * tz)
            + corner(i0, j1, k0) * ((1.0 - tx) * ty * (1.0 - tz))
            + corner(i0, j1, k1) * ((1.0 - tx) * ty * tz)
            + corner(i1, j0, k0) * (tx * (1.0 - ty) * (1.0 - tz))
            + corner(i1, j0, k1) * (tx * (1.0 - ty) * tz)
            + corner(i1, j1, k0) * (tx * ty * (1.0 - tz))
            + corner(i1, j1, k1) * (tx * ty * tz)
    }

    /// Central difference of the stored samples along one grid axis, in *index*
    /// units. One-sided on the boundary planes.
    fn index_derivative(&self, axis: usize, i: usize, j: usize, k: usize) -> f64 {
        let n = self.grid.dims[axis];
        if n < 2 {
            return 0.0;
        }
        let indices = [i, j, k];
        let at = |shifted: usize| {
            let mut p = indices;
            p[axis] = shifted;
            self.sample_at_index(p[0], p[1], p[2])
        };
        let here = indices[axis];
        if here == 0 {
            at(1) - at(0)
        } else if here == n - 1 {
            at(n - 1) - at(n - 2)
        } else {
            (at(here + 1) - at(here - 1)) * 0.5
        }
    }
}

impl ScalarField for SampledField {
    fn sample(&self, point: DVec3) -> f64 {
        let Some(cell) = self.locate(point) else {
            return 0.0;
        };
        Self::trilinear(cell, |i, j, k| self.sample_at_index(i, j, k))
    }

    /// Central differences **on the stored samples directly**, not on
    /// interpolated values: per-corner index-space derivatives are computed from
    /// stored neighbours and then blended trilinearly. Exact with respect to the
    /// stored data, cheaper than three interpolated pairs, and precisely what
    /// isosurface normals want.
    ///
    /// The index-space gradient maps to real space through the transpose of the
    /// inverse basis, since `value(p) = v(inv_basis * (p - origin))`.
    fn gradient(&self, point: DVec3) -> DVec3 {
        let Some(cell) = self.locate(point) else {
            return DVec3::ZERO;
        };
        let index_gradient = Self::trilinear(cell, |i, j, k| {
            DVec3::new(
                self.index_derivative(0, i, j, k),
                self.index_derivative(1, i, j, k),
                self.index_derivative(2, i, j, k),
            )
        });
        self.inv_basis.transpose() * index_gradient
    }

    fn data_bounds(&self) -> Option<FieldBounds> {
        Some(self.grid.bounds())
    }

    fn suggested_bounds(&self) -> FieldBounds {
        self.grid.bounds()
    }

    fn native_grid(&self) -> Option<GridGeometry> {
        Some(self.grid)
    }

    fn value_range(&self) -> Option<(f64, f64)> {
        Some(self.value_range)
    }
}
