//! `ValueDistribution` — how a field's magnitude is distributed over its
//! samples, and the two queries that turn that into an isolevel.
//!
//! The number a user actually wants when choosing an isosurface level is rarely
//! the isovalue: it is *how much of the field the surface encloses*. This module
//! is the coordinate change between the two.
//!
//! The **enclosed fraction** is the share of the field's total integrated `|v|`
//! that lies inside `{ |v| >= iso }`, each sample read as a density:
//!
//! ```text
//! sorted  = sort(|v| over all samples, DESCENDING)
//! cumsum  = prefix sums of sorted
//! total   = cumsum[last]
//! iso_for_fraction(f)  = sorted[ smallest k with cumsum[k] >= f * total ]
//! fraction_for_iso(v)  = cumsum[ last k with sorted[k] >= v ] / total
//! ```
//!
//! **Mass, not count.** On a 96³ box holding one `exp(-2r)` blob, mass
//! `f = 0.72` gives `iso = 2.33e-2` — 0.058% of the voxels, a molecular
//! envelope. The *voxel-count* 0.72 percentile gives `1.61e-13`: the vacuum,
//! effectively the whole box. Eleven orders apart, because almost all of any
//! cube box is vacuum and anything weighted by voxel count is dominated by it.
//!
//! **`V_cell` cancels.** `∫|v| dV = Σ|v_i| · V_cell` with
//! `V_cell = |det(axes)|` constant over the grid, so the *ratio* of two such
//! integrals is unaffected by spacing or shear. That is why nothing here takes a
//! [`crate::field::GridGeometry`] and no determinant appears anywhere: a
//! distribution is a property of the samples alone.
//!
//! **Boundary samples are not half-weighted.** Exact trapezoidal integration on
//! a node-centered grid weights the outermost planes by 1/2. That is negligible
//! for an isolated system; for a periodic supercell the boundary plane is a
//! periodic image and is genuinely double-counted, worth well under a percent on
//! a ratio. Documented rather than corrected — correcting it needs a periodicity
//! classification this crate does not have.
//!
//! Design doc: `doc/design_isosurface_level.md`, Part 1.

/// Sample count at or above which the exact sorted structure is not built.
///
/// **A memory decision, not a speed one.** The magnitudes keep the storage width
/// (`f32`) and only the prefix sums need `f64`, so [`ExactCumulative`] costs 12
/// bytes per sample: a 144³ cube is 3.0M samples ≈ 36 MB, while a 247×247×164
/// one is 10,005,476 samples ≈ 120 MB — affordable once, not once per field in a
/// network holding several. Above the limit the histogram resolves a percentile
/// to within a bin, which is far finer than any decision made from it.
pub const EXACT_SAMPLE_LIMIT: usize = 4_000_000;

/// Log-spaced bins in every [`LogHistogram`].
///
/// Over the widest span an `f32` field can occupy (~48 decades from the smallest
/// magnitude a chemistry code writes to the largest) this is ~5% per bin — an
/// order of magnitude finer than any threshold read off it.
pub const HISTOGRAM_BINS: usize = 2048;

/// The accumulation exponent every field uses today: `Σ|v_i|`.
///
/// The exponent is a parameter rather than a hardcoded `abs` because it is the
/// sole hook for a future declared field kind: an *amplitude* (a molecular
/// orbital) integrates as `|ψ|²`, so it would be accumulated at exponent 2.
/// Nothing selects that yet.
pub const DEFAULT_EXPONENT: i32 = 1;

/// Below this `log10` span the nonzero range is treated as degenerate (every
/// sample the same magnitude) and padded, so the bin width is never zero.
const MIN_LOG_SPAN: f64 = 1e-9;

/// Half-span the histogram is padded to when the nonzero range is degenerate.
const DEGENERATE_LOG_PAD: f64 = 0.5;

/// Mass per log-spaced magnitude bin, plus the running mass at or above each
/// bin's lower edge.
///
/// **No per-bin sample counts.** Nothing needs them: the editor plots mass (a
/// count-weighted plot is one vacuum spike — see the module docs), and the
/// `Auto` rule is a ratio of two isovalues. Counts would only invite the
/// count-weighted percentile this whole design exists to avoid.
///
/// Bins are laid out over `[nonzero_min, nonzero_max]` in `log10` space; exact
/// zeros have no home there and are reported separately by
/// [`ValueDistribution::zero_count`].
#[derive(Debug, Clone)]
pub struct LogHistogram {
    /// `log10` of edge 0.
    log_lo: f64,
    /// `log10` of edge `bin_count()`.
    log_hi: f64,
    /// `Σ |v_i|^exponent` for the samples in each bin, ascending in magnitude.
    mass: Vec<f64>,
    /// Suffix sums of `mass`, length `mass.len() + 1`; the last entry is `0.0`.
    /// `mass_at_or_above[i]` is the mass at or above `edge(i)`.
    mass_at_or_above: Vec<f64>,
}

impl LogHistogram {
    /// A histogram over no nonzero samples at all: every bin empty, no
    /// meaningful span.
    fn empty() -> Self {
        Self {
            log_lo: 0.0,
            log_hi: 0.0,
            mass: vec![0.0; HISTOGRAM_BINS],
            mass_at_or_above: vec![0.0; HISTOGRAM_BINS + 1],
        }
    }

    /// Bin the magnitudes of `samples` (zeros and non-finite values skipped)
    /// over the span `[lo, hi]`, accumulating `|v|^exponent`.
    fn build(samples: &[f32], exponent: i32, lo: f64, hi: f64) -> Self {
        let mut log_lo = lo.log10();
        let mut log_hi = hi.log10();
        if log_hi - log_lo <= MIN_LOG_SPAN {
            // Every nonzero sample has (near enough) the same magnitude. Pad so
            // the bin width is finite; the reported nonzero range is stored
            // separately and stays exact.
            log_lo -= DEGENERATE_LOG_PAD;
            log_hi += DEGENERATE_LOG_PAD;
        }
        let span = log_hi - log_lo;

        let mut mass = vec![0.0f64; HISTOGRAM_BINS];
        for &sample in samples {
            let magnitude = sample.abs() as f64;
            if magnitude == 0.0 || !magnitude.is_finite() {
                continue;
            }
            let position = (magnitude.log10() - log_lo) / span * HISTOGRAM_BINS as f64;
            mass[clamp_bin(position)] += magnitude.powi(exponent);
        }

        let mut mass_at_or_above = vec![0.0f64; HISTOGRAM_BINS + 1];
        for bin in (0..HISTOGRAM_BINS).rev() {
            mass_at_or_above[bin] = mass_at_or_above[bin + 1] + mass[bin];
        }

        Self {
            log_lo,
            log_hi,
            mass,
            mass_at_or_above,
        }
    }

    /// Number of bins — always [`HISTOGRAM_BINS`], exposed so a plotting
    /// consumer does not have to import the constant.
    pub fn bin_count(&self) -> usize {
        self.mass.len()
    }

    /// Magnitude of bin edge `index`, `0 ..= bin_count()`.
    pub fn edge(&self, index: usize) -> f64 {
        let t = index as f64 / self.bin_count() as f64;
        10f64.powf(self.log_lo + t * (self.log_hi - self.log_lo))
    }

    /// Mass in each bin, ascending in magnitude. The editor's y axis.
    pub fn mass(&self) -> &[f64] {
        &self.mass
    }

    /// Suffix sums of [`LogHistogram::mass`], length `bin_count() + 1`. The
    /// editor's cumulative overlay, and what both queries binary-search.
    pub fn mass_at_or_above(&self) -> &[f64] {
        &self.mass_at_or_above
    }

    /// Total binned mass. Exact zeros contribute nothing at any exponent, so
    /// this is the field's whole mass despite them being excluded from the bins.
    pub fn total(&self) -> f64 {
        self.mass_at_or_above[0]
    }

    /// Index of the bin holding `magnitude`, saturating at both ends.
    fn bin_of(&self, magnitude: f64) -> usize {
        let span = self.log_hi - self.log_lo;
        if span <= 0.0 {
            return 0;
        }
        clamp_bin((magnitude.log10() - self.log_lo) / span * self.bin_count() as f64)
    }

    /// Lower edge of the tightest bin that still encloses `fraction` of the
    /// mass — the histogram's answer to `iso_for_fraction`, accurate to a bin.
    fn iso_for_fraction(&self, fraction: f64) -> f64 {
        let target = fraction * self.total();
        // `mass_at_or_above` is non-increasing, so the entries satisfying
        // `>= target` form a prefix; the last of them is the tightest threshold
        // that still encloses `target`.
        let leading =
            self.mass_at_or_above[..self.bin_count()].partition_point(|&mass| mass >= target);
        self.edge(leading.saturating_sub(1))
    }

    /// Mass at or above `magnitude`, over the total — accurate to one bin's
    /// mass, since the bin containing `magnitude` is counted whole.
    fn fraction_for_iso(&self, magnitude: f64) -> f64 {
        let total = self.total();
        if total <= 0.0 {
            return 0.0;
        }
        (self.mass_at_or_above[self.bin_of(magnitude)] / total).clamp(0.0, 1.0)
    }

    /// Heap footprint: two `f64` vectors over the bin count.
    fn estimate_memory_bytes(&self) -> usize {
        (self.mass.capacity() + self.mass_at_or_above.capacity()) * std::mem::size_of::<f64>()
    }
}

/// A fractional bin position to a valid index. Written once because the build
/// loop and the lookup must agree exactly on the saturating behaviour at both
/// ends, and a NaN position (`magnitude` of zero never reaches here, but a
/// caller's arbitrary isovalue can be anything) must land in bin 0 rather than
/// panic on an out-of-range index.
fn clamp_bin(position: f64) -> usize {
    if position >= HISTOGRAM_BINS as f64 {
        HISTOGRAM_BINS - 1
    } else if position > 0.0 {
        (position as usize).min(HISTOGRAM_BINS - 1)
    } else {
        0
    }
}

/// Descending magnitudes with their prefix sums — the exact answer to both
/// queries, built only below [`EXACT_SAMPLE_LIMIT`].
///
/// The split widths are the point: magnitudes keep the field's own `f32`, only
/// the running sums need `f64`. 12 bytes per sample, against 16 if both were
/// `f64`.
#[derive(Debug, Clone)]
struct ExactCumulative {
    /// Nonzero magnitudes, sorted DESCENDING. Zeros are excluded — they
    /// contribute no mass and would only lengthen every binary search.
    magnitudes: Vec<f32>,
    /// `cumulative[k] = Σ_{j<=k} magnitudes[j]^exponent`, so it is ascending.
    cumulative: Vec<f64>,
}

impl ExactCumulative {
    fn build(samples: &[f32], exponent: i32) -> Self {
        let mut magnitudes: Vec<f32> = samples
            .iter()
            .map(|sample| sample.abs())
            .filter(|magnitude| *magnitude != 0.0 && magnitude.is_finite())
            .collect();
        // Descending. Unstable is fine: equal magnitudes are interchangeable and
        // the prefix sums do not care which came first.
        magnitudes.sort_unstable_by(|a, b| b.total_cmp(a));

        let mut cumulative = Vec::with_capacity(magnitudes.len());
        let mut running = 0.0f64;
        for &magnitude in &magnitudes {
            running += (magnitude as f64).powi(exponent);
            cumulative.push(running);
        }

        Self {
            magnitudes,
            cumulative,
        }
    }

    fn total(&self) -> f64 {
        self.cumulative.last().copied().unwrap_or(0.0)
    }

    /// The stored magnitude at the requested fraction: the largest isovalue
    /// whose enclosed mass still reaches `fraction * total`.
    ///
    /// Only called when there is at least one nonzero sample, so `magnitudes` is
    /// never empty here.
    fn iso_for_fraction(&self, fraction: f64) -> f64 {
        let target = fraction * self.total();
        let k = self
            .cumulative
            .partition_point(|&sum| sum < target)
            .min(self.magnitudes.len() - 1);
        self.magnitudes[k] as f64
    }

    /// Mass of `{ |v_i| >= magnitude }` over the total.
    ///
    /// **A search, not a lookup.** The argument is almost never a stored
    /// sample — it is a level typed in absolute mode, a fixed convention, a
    /// slider position — so reading it as an index into `magnitudes` would be
    /// undefined for every real call.
    fn fraction_for_iso(&self, magnitude: f64) -> f64 {
        let count = self
            .magnitudes
            .partition_point(|&stored| (stored as f64) >= magnitude);
        if count == 0 {
            return 0.0;
        }
        (self.cumulative[count - 1] / self.total()).clamp(0.0, 1.0)
    }

    fn estimate_memory_bytes(&self) -> usize {
        self.magnitudes.capacity() * std::mem::size_of::<f32>()
            + self.cumulative.capacity() * std::mem::size_of::<f64>()
    }
}

/// The magnitude distribution of a field's stored samples.
///
/// Built from samples alone — see the module docs for why no grid geometry is
/// involved. Both queries return `Option`, `None` when the field carries no mass
/// at all: an all-zero field has no level to offer, and returning `None` is what
/// lets a caller turn that into a descriptive error rather than a divide by
/// zero.
#[derive(Debug)]
pub struct ValueDistribution {
    /// `Σ |v_i|^exponent` over every sample.
    total: f64,
    /// The exponent `total` and every bin mass were accumulated at.
    exponent: i32,
    /// Descending-sorted magnitudes with prefix sums. `None` at or above
    /// [`EXACT_SAMPLE_LIMIT`] samples.
    exact: Option<ExactCumulative>,
    /// Always present: it is both the large-field query path *and* what the
    /// editor plots. The exact structure is a refinement on top of it, not an
    /// alternative to it.
    histogram: LogHistogram,
    /// Smallest and largest nonzero magnitude, `None` when there are none.
    ///
    /// **`f32` storage flushes deep vacuum to zero**, so the lower end is the
    /// storage floor, not the source file's: a density reaching `1.6e-75` on
    /// disk contributes to [`ValueDistribution::zero_count`] here instead. A
    /// readout must not claim otherwise.
    nonzero_range: Option<(f64, f64)>,
    /// Exactly-zero samples. Excluded from the bins — log space has no home for
    /// them — and reported separately so "all vacuum" is visible rather than
    /// silently absent.
    zero_count: usize,
}

impl ValueDistribution {
    /// Build a distribution over `samples` at [`DEFAULT_EXPONENT`].
    pub fn from_samples(samples: &[f32]) -> Self {
        Self::with_limit(samples, DEFAULT_EXPONENT, EXACT_SAMPLE_LIMIT)
    }

    /// Build at an explicit accumulation exponent — see [`DEFAULT_EXPONENT`].
    pub fn from_samples_with_exponent(samples: &[f32], exponent: i32) -> Self {
        Self::with_limit(samples, exponent, EXACT_SAMPLE_LIMIT)
    }

    /// Build with an explicit exact-structure limit.
    ///
    /// **A test seam, deliberately public.** Without it the only way to reach
    /// the histogram-only path is a field of more than four million samples,
    /// which as a committed fixture means 100+ MB of ASCII floats parsed on
    /// every test run. With it an 11³ field and a limit of 500 crosses the same
    /// boundary in microseconds.
    pub fn with_limit(samples: &[f32], exponent: i32, exact_sample_limit: usize) -> Self {
        let mut zero_count = 0usize;
        let mut nonzero_min = f64::INFINITY;
        let mut nonzero_max = 0.0f64;
        for &sample in samples {
            // Non-finite samples cannot reach here from `SampledField`, which
            // rejects them at construction. Skipping rather than propagating
            // keeps this constructor total for any other caller.
            let magnitude = sample.abs() as f64;
            if !magnitude.is_finite() {
                continue;
            }
            if magnitude == 0.0 {
                zero_count += 1;
                continue;
            }
            if magnitude < nonzero_min {
                nonzero_min = magnitude;
            }
            if magnitude > nonzero_max {
                nonzero_max = magnitude;
            }
        }

        if nonzero_max == 0.0 {
            return Self {
                total: 0.0,
                exponent,
                exact: None,
                histogram: LogHistogram::empty(),
                nonzero_range: None,
                zero_count,
            };
        }

        let histogram = LogHistogram::build(samples, exponent, nonzero_min, nonzero_max);
        let exact =
            (samples.len() < exact_sample_limit).then(|| ExactCumulative::build(samples, exponent));
        let total = match &exact {
            // Prefer the exact sum when it exists, so `fraction_for_iso` below
            // the smallest sample returns exactly 1.0 rather than 1.0 minus the
            // rounding difference between two accumulation orders.
            Some(exact) => exact.total(),
            None => histogram.total(),
        };

        Self {
            total,
            exponent,
            exact,
            histogram,
            nonzero_range: Some((nonzero_min, nonzero_max)),
            zero_count,
        }
    }

    /// `Σ |v_i|^exponent`. Zero exactly when the field is entirely zero.
    pub fn total(&self) -> f64 {
        self.total
    }

    /// The exponent the masses were accumulated at.
    pub fn exponent(&self) -> i32 {
        self.exponent
    }

    /// Whether the exact sorted structure was built — `false` at or above
    /// [`EXACT_SAMPLE_LIMIT`], where both queries answer from the histogram.
    pub fn is_exact(&self) -> bool {
        self.exact.is_some()
    }

    /// Smallest and largest nonzero magnitude, `None` for an all-zero field.
    pub fn nonzero_range(&self) -> Option<(f64, f64)> {
        self.nonzero_range
    }

    /// Number of exactly-zero samples.
    pub fn zero_count(&self) -> usize {
        self.zero_count
    }

    /// The binned distribution — always present, at every field size.
    pub fn histogram(&self) -> &LogHistogram {
        &self.histogram
    }

    /// The isovalue enclosing `fraction` of the field's total mass.
    ///
    /// `fraction` is clamped to `[0, 1]`. `None` when the field carries no mass
    /// (`total == 0`) or `fraction` is not finite.
    pub fn iso_for_fraction(&self, fraction: f64) -> Option<f64> {
        if self.total <= 0.0 || !fraction.is_finite() {
            return None;
        }
        let fraction = fraction.clamp(0.0, 1.0);
        let (nonzero_min, nonzero_max) = self.nonzero_range?;
        Some(match &self.exact {
            Some(exact) => exact.iso_for_fraction(fraction),
            // A bin edge can sit a hair outside the real range once the
            // degenerate-span padding is involved; clamping keeps the answer a
            // level the field actually attains, as the exact path's always is.
            None => self
                .histogram
                .iso_for_fraction(fraction)
                .clamp(nonzero_min, nonzero_max),
        })
    }

    /// The share of the field's total mass enclosed by `{ |v| >= isovalue }`.
    ///
    /// The argument is a *magnitude*: a signed field's surface is extracted at
    /// `±level`, so the sign of what is passed here carries no information and
    /// is discarded. `0` when the isovalue exceeds every sample, `1` when it is
    /// at or below the smallest nonzero one. `None` when the field carries no
    /// mass or the isovalue is not finite.
    ///
    /// **This is not the inverse of [`ValueDistribution::iso_for_fraction`].**
    /// `f -> iso -> f` round-trips to within one sample's share, but
    /// `iso -> f -> iso` does not: every isovalue between two adjacent sorted
    /// samples encloses the same set, so this is a step function while
    /// `iso_for_fraction` returns one representative per step. That asymmetry is
    /// why a node offering both coordinates stores two numbers rather than
    /// converting between them.
    pub fn fraction_for_iso(&self, isovalue: f64) -> Option<f64> {
        if self.total <= 0.0 || !isovalue.is_finite() {
            return None;
        }
        let isovalue = isovalue.abs();
        let (nonzero_min, nonzero_max) = self.nonzero_range?;
        Some(match &self.exact {
            Some(exact) => exact.fraction_for_iso(isovalue),
            None if isovalue > nonzero_max => 0.0,
            None if isovalue <= nonzero_min => 1.0,
            None => self.histogram.fraction_for_iso(isovalue),
        })
    }

    /// Approximate footprint in bytes, including the heap.
    ///
    /// Reported so that [`crate::field::ScalarField::estimate_memory_bytes`] can
    /// account for a warm cache: the exact structure is the largest single
    /// allocation a field owns after its samples.
    pub fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.histogram.estimate_memory_bytes()
            + self
                .exact
                .as_ref()
                .map_or(0, ExactCumulative::estimate_memory_bytes)
    }
}
