//! `ValueDistribution` — the mass-weighted magnitude distribution behind the
//! isosurface node's enclosed-fraction coordinate.
//!
//! Every field here is built in code rather than loaded from a fixture: a
//! distribution test wants a field whose answer is computable by hand or
//! analytic, and a loop gives that better than any file can. The one exception
//! is the optional zoo tier at the bottom, which is calibration evidence rather
//! than a contract.
//!
//! Design doc: `doc/design_isosurface_level.md`, Part 1 and Part 7 §P1.

use atomcad_crystolecule::field::distribution::{HISTOGRAM_BINS, ValueDistribution};
use atomcad_crystolecule::field::{GridGeometry, SampledField, ScalarField};
use glam::DVec3;
use std::sync::Arc;

// --- oracles ----------------------------------------------------------------

/// 4×4×4 = 64 samples: four at `10.0`, sixty at `1.0`.
///
/// Total mass is exactly `40 + 60 = 100`, so every fraction is a percentage you
/// can check in your head — and the high shell is **40% of the mass but 6.25% of
/// the samples**, which is the mass-versus-count trap in one artifact.
fn two_level_samples() -> Vec<f32> {
    let mut samples = vec![1.0f32; 64];
    for sample in samples.iter_mut().take(4) {
        *sample = 10.0;
    }
    samples
}

/// `exp(-r^2 / 2σ^2)` with `σ = 1`, on an `n³` grid spanning ±4σ.
///
/// Read as a density, a spherical Gaussian's enclosed-mass fraction has a closed
/// form, so the expected isovalue is analytic and independent of both σ and the
/// grid: `iso_for_fraction(f) = exp(-x_f)` where `P(3/2, x_f) = f`, the
/// regularized lower incomplete gamma. See [`GAUSSIAN_ORACLE`].
fn gaussian_samples(n: usize) -> Vec<f32> {
    let half_extent = 4.0f64;
    let step = 2.0 * half_extent / (n - 1) as f64;
    let mut samples = Vec::with_capacity(n * n * n);
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                let p = DVec3::new(
                    -half_extent + step * i as f64,
                    -half_extent + step * j as f64,
                    -half_extent + step * k as f64,
                );
                samples.push((-0.5 * p.length_squared()).exp() as f32);
            }
        }
    }
    samples
}

/// `(fraction, expected isovalue)` for [`gaussian_samples`] — the continuum
/// answer, which no finite grid reproduces exactly.
const GAUSSIAN_ORACLE: [(f64, f64); 4] = [
    (0.30, 0.490_747),
    (0.50, 0.306_362),
    (0.72, 0.147_076),
    (0.90, 0.043_906),
];

/// An axis-aligned unit-spaced grid of the given dimensions.
fn unit_grid(dims: [usize; 3]) -> GridGeometry {
    GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims,
    }
}

fn relative_error(actual: f64, expected: f64) -> f64 {
    (actual - expected).abs() / expected.abs()
}

// --- the two-level oracle ---------------------------------------------------

#[test]
fn two_level_field_answers_both_queries_by_hand() {
    let distribution = ValueDistribution::from_samples(&two_level_samples());
    assert!(distribution.is_exact());
    assert_eq!(distribution.total(), 100.0);
    assert_eq!(distribution.zero_count(), 0);
    assert_eq!(distribution.nonzero_range(), Some((1.0, 10.0)));

    // The four high samples carry 40 of the 100, so any fraction they can cover
    // resolves to their level and the first one they cannot falls to 1.
    assert_eq!(distribution.iso_for_fraction(0.25), Some(10.0));
    assert_eq!(distribution.iso_for_fraction(0.40), Some(10.0));
    assert_eq!(distribution.iso_for_fraction(0.41), Some(1.0));

    // An isovalue between the two levels encloses exactly the high shell.
    assert_eq!(distribution.fraction_for_iso(5.0), Some(0.40));
    // Above every sample, and below the smallest.
    assert_eq!(distribution.fraction_for_iso(20.0), Some(0.0));
    assert_eq!(distribution.fraction_for_iso(0.5), Some(1.0));
}

#[test]
fn mass_weighting_and_count_weighting_disagree_by_construction() {
    let samples = two_level_samples();
    let distribution = ValueDistribution::from_samples(&samples);

    // Mass: the high shell is 40% of the total, so the 0.40 quantile is 10.
    assert_eq!(distribution.iso_for_fraction(0.40), Some(10.0));

    // Count: the same shell is 4 of 64 samples — 6.25% — so the *count* 0.40
    // percentile is 1. This is the failure mode the whole parameterization
    // exists to avoid; on a real cube box it is eleven orders of magnitude.
    let mut descending = samples.clone();
    descending.sort_unstable_by(|a, b| b.total_cmp(a));
    let count_quantile = descending[(0.40 * descending.len() as f64) as usize];
    assert_eq!(count_quantile, 1.0);
    assert_eq!(4.0 / 64.0, 0.0625);
}

// --- the Gaussian analytic oracle -------------------------------------------

#[test]
fn gaussian_isovalues_match_the_incomplete_gamma_oracle() {
    let distribution = ValueDistribution::from_samples(&gaussian_samples(65));
    // 2% is measured, not assumed: `iso_for_fraction` returns a *stored sample*,
    // so it approaches the continuum answer only as the grid refines — 18% at
    // 17³, 3.0% at 33³, 1.1% at 65³. Do not run this coarse, and do not tighten
    // it to look better; the round-trip below is the resolution-free check.
    for (fraction, expected) in GAUSSIAN_ORACLE {
        let iso = distribution.iso_for_fraction(fraction).unwrap();
        assert!(
            relative_error(iso, expected) < 0.02,
            "f = {fraction}: got {iso:e}, expected {expected:e}",
        );
    }
}

#[test]
fn fraction_round_trips_through_isovalue_at_any_resolution() {
    // The tight check, and the one that does not care about grid resolution:
    // `iso_for_fraction` returns the tightest stored magnitude still enclosing
    // `f`, so reading the fraction back can only overshoot — never undershoot.
    //
    // **The overshoot is one *tied group*, not one sample.** A Gaussian on a
    // symmetric grid is massively degenerate — every point of an octahedral
    // shell has the identical magnitude — and `fraction_for_iso` is a step
    // function whose step includes all of them. At 17³ that is 8.9% of the mass
    // at `f = 0.30`, so a `1/N` bound is wrong by two orders of magnitude and
    // would be wrong on any real field with a flat region. The honest bound is
    // the mass of the samples that tie with the answer, computed here from the
    // same samples the distribution was built from.
    for n in [17usize, 33, 65] {
        let samples = gaussian_samples(n);
        let distribution = ValueDistribution::from_samples(&samples);
        let total = distribution.total();
        for (fraction, _) in GAUSSIAN_ORACLE {
            let iso = distribution.iso_for_fraction(fraction).unwrap();
            let back = distribution.fraction_for_iso(iso).unwrap();
            let tied: f64 = samples
                .iter()
                .filter(|sample| sample.abs() as f64 == iso)
                .map(|sample| sample.abs() as f64)
                .sum();
            let slack = tied / total;
            assert!(
                back >= fraction && back - fraction <= slack,
                "n = {n}, f = {fraction}: round-tripped to {back} (slack {slack})",
            );
        }
    }
}

#[test]
fn the_same_fraction_gives_the_same_isovalue_on_a_finer_grid() {
    // Grid independence: the fraction is a property of the *field*, so refining
    // the sampling must not move the level it resolves to. Both grids sit within
    // their own oracle error above (3.0% at 33³, 1.1% at 65³), so they agree
    // with each other to the sum of those.
    let coarse = ValueDistribution::from_samples(&gaussian_samples(33));
    let fine = ValueDistribution::from_samples(&gaussian_samples(65));
    for (fraction, _) in GAUSSIAN_ORACLE {
        let a = coarse.iso_for_fraction(fraction).unwrap();
        let b = fine.iso_for_fraction(fraction).unwrap();
        assert!(
            relative_error(a, b) < 0.05,
            "f = {fraction}: 33³ gave {a:e}, 65³ gave {b:e}",
        );
    }
}

#[test]
fn shearing_the_grid_does_not_move_the_distribution() {
    // `∫|v| dV = Σ|v_i| · V_cell` with `V_cell` constant, so the cell volume
    // cancels out of every ratio: spacing and shear are invisible here. The
    // guard is that the distribution is built from samples alone — no
    // determinant, no geometry — and this is what would break if that changed.
    let samples = gaussian_samples(17);
    let upright = SampledField::new(unit_grid([17, 17, 17]), samples.clone()).unwrap();
    let sheared = SampledField::new(
        GridGeometry {
            origin: DVec3::new(-1.0, 2.0, 0.5),
            axes: [
                DVec3::new(0.25, 0.0, 0.0),
                DVec3::new(0.11, 0.25, 0.0),
                DVec3::new(0.03, 0.07, 0.25),
            ],
            dims: [17, 17, 17],
        },
        samples,
    )
    .unwrap();

    let a = upright.value_distribution().unwrap();
    let b = sheared.value_distribution().unwrap();
    assert_eq!(a.total(), b.total());
    for (fraction, _) in GAUSSIAN_ORACLE {
        assert_eq!(a.iso_for_fraction(fraction), b.iso_for_fraction(fraction));
    }
    assert_eq!(a.fraction_for_iso(0.1), b.fraction_for_iso(0.1));
}

// --- the exact / histogram boundary -----------------------------------------

#[test]
fn a_forced_limit_crosses_into_the_histogram_without_a_giant_field() {
    // The seam that makes the histogram path testable at all: an 11³ field is
    // 1331 samples, so a limit of 500 puts it on the far side of the boundary
    // that `EXACT_SAMPLE_LIMIT` otherwise only a 4-million-sample file reaches.
    let samples = gaussian_samples(11);
    assert_eq!(samples.len(), 1331);
    assert!(ValueDistribution::with_limit(&samples, 1, 4_000_000).is_exact());
    assert!(!ValueDistribution::with_limit(&samples, 1, 500).is_exact());
}

#[test]
fn the_histogram_agrees_with_the_exact_structure_to_within_a_bin() {
    let samples = gaussian_samples(33);
    let exact = ValueDistribution::with_limit(&samples, 1, usize::MAX);
    let binned = ValueDistribution::with_limit(&samples, 1, 1);
    assert!(exact.is_exact());
    assert!(!binned.is_exact());

    // One bin, as a relative width: the bins span the field's own nonzero range
    // in log10, so this is the quantization the histogram path can ever be off
    // by. Doubled for slack — either answer may sit anywhere inside its bin.
    let (lo, hi) = exact.nonzero_range().unwrap();
    let bin_width = 2.0 * ((hi / lo).log10() / HISTOGRAM_BINS as f64);
    for (fraction, _) in GAUSSIAN_ORACLE {
        let a = exact.iso_for_fraction(fraction).unwrap();
        let b = binned.iso_for_fraction(fraction).unwrap();
        assert!(
            (a / b).log10().abs() <= bin_width,
            "f = {fraction}: exact {a:e}, binned {b:e}",
        );
        // And the fraction read back from the exact answer agrees too.
        let fa = exact.fraction_for_iso(a).unwrap();
        let fb = binned.fraction_for_iso(a).unwrap();
        assert!((fa - fb).abs() < 0.01, "f = {fraction}: {fa} vs {fb}");
    }
}

#[test]
fn bin_quantization_does_not_flip_the_auto_plausibility_branch() {
    // `LevelMode::Auto` compares `iso_for_fraction(0.72)` against a fixed
    // density level, and takes the fixed one only when the ratio is below
    // `MAX_LEVEL_RATIO` (`doc/design_isosurface_level.md` §The plausibility
    // window). Both constants live with the node, not here — this test cares
    // only that the *decision* survives the exact-to-histogram switch, so it
    // restates them locally and walks a field right across the boundary.
    const DENSITY_LEVEL: f64 = 0.002;
    const MAX_LEVEL_RATIO: f64 = 125.0;

    // A Gaussian scaled so `iso_for_fraction(0.72)` lands within a few percent
    // of the threshold — the worst case for a quantized answer.
    let unit = ValueDistribution::from_samples(&gaussian_samples(33));
    let unit_iso = unit.iso_for_fraction(0.72).unwrap();
    let scale_to_threshold = MAX_LEVEL_RATIO * DENSITY_LEVEL / unit_iso;

    for offset in [0.90f64, 0.97, 0.99, 1.01, 1.03, 1.10] {
        let scale = (scale_to_threshold * offset) as f32;
        let samples: Vec<f32> = gaussian_samples(33).iter().map(|v| v * scale).collect();
        let exact = ValueDistribution::with_limit(&samples, 1, usize::MAX);
        let binned = ValueDistribution::with_limit(&samples, 1, 1);
        let exact_ratio = exact.iso_for_fraction(0.72).unwrap() / DENSITY_LEVEL;
        let binned_ratio = binned.iso_for_fraction(0.72).unwrap() / DENSITY_LEVEL;
        assert_eq!(
            exact_ratio <= MAX_LEVEL_RATIO,
            binned_ratio <= MAX_LEVEL_RATIO,
            "offset {offset}: exact ratio {exact_ratio}, binned ratio {binned_ratio}",
        );
    }
}

// --- zeros, emptiness, and the ends of the range ----------------------------

#[test]
fn deep_vacuum_below_f32_lands_in_the_zero_count_not_the_lowest_bin() {
    // A real density reaches 1.6e-75 on disk; `f32` storage flushes that to
    // zero, so the histogram's lower edge is the storage floor and not the
    // file's. The readout must not claim otherwise — and `total` must not move.
    let mut samples = vec![1e-75f64 as f32; 100];
    samples.extend([1.0f32; 4]);
    let distribution = ValueDistribution::from_samples(&samples);

    assert_eq!(distribution.zero_count(), 100);
    assert_eq!(distribution.total(), 4.0);
    assert_eq!(distribution.nonzero_range(), Some((1.0, 1.0)));
    assert_eq!(distribution.histogram().total(), 4.0);
}

#[test]
fn an_all_zero_field_has_a_distribution_but_no_level_to_offer() {
    let distribution = ValueDistribution::from_samples(&vec![0.0f32; 64]);
    assert_eq!(distribution.total(), 0.0);
    assert_eq!(distribution.zero_count(), 64);
    assert_eq!(distribution.nonzero_range(), None);
    // `None`, not a panic and not a silently zero level: the caller turns this
    // into a descriptive evaluation error.
    assert_eq!(distribution.iso_for_fraction(0.72), None);
    assert_eq!(distribution.fraction_for_iso(0.002), None);
}

#[test]
fn zeros_are_counted_and_excluded_from_the_bins() {
    let mut samples = vec![0.0f32; 40];
    samples.extend(std::iter::repeat_n(2.0f32, 10));
    samples.extend(std::iter::repeat_n(8.0f32, 5));
    let distribution = ValueDistribution::from_samples(&samples);

    assert_eq!(distribution.zero_count(), 40);
    assert_eq!(distribution.total(), 60.0);
    assert_eq!(distribution.nonzero_range(), Some((2.0, 8.0)));
    // Every bin's mass together is the whole mass — the zeros contributed none
    // of it and occupy none of the bins.
    let binned: f64 = distribution.histogram().mass().iter().sum();
    assert!((binned - 60.0).abs() < 1e-9);
    assert_eq!(distribution.histogram().mass().len(), HISTOGRAM_BINS);
    assert_eq!(
        distribution.histogram().mass_at_or_above().len(),
        HISTOGRAM_BINS + 1
    );
}

#[test]
fn fraction_for_iso_saturates_rather_than_indexing() {
    // `fraction_for_iso` is a search over magnitudes, never a position in the
    // sorted array — so an argument outside the stored range is an ordinary
    // answer, not an out-of-bounds one. Both paths must agree on that.
    for limit in [usize::MAX, 1] {
        let distribution = ValueDistribution::with_limit(&two_level_samples(), 1, limit);
        assert_eq!(distribution.fraction_for_iso(1e6), Some(0.0));
        assert_eq!(distribution.fraction_for_iso(1e-30), Some(1.0));
        assert_eq!(distribution.fraction_for_iso(0.0), Some(1.0));
        // A signed field is extracted at ±level, so the sign carries no
        // information here.
        assert_eq!(
            distribution.fraction_for_iso(-5.0),
            distribution.fraction_for_iso(5.0)
        );
        assert_eq!(distribution.fraction_for_iso(f64::NAN), None);
        assert_eq!(distribution.iso_for_fraction(f64::NAN), None);
    }
}

// --- the exponent parameter -------------------------------------------------

#[test]
fn exponent_two_accumulates_squares_on_a_signed_field() {
    // The sole hook for a future declared field kind: an amplitude integrates as
    // |ψ|², so it would be accumulated at exponent 2. Nothing selects that yet,
    // so this pins the mechanism rather than a policy.
    let samples = [3.0f32, -4.0, 0.0, -3.0, 4.0];

    let linear = ValueDistribution::from_samples(&samples);
    assert_eq!(linear.total(), 14.0); // 3 + 4 + 3 + 4
    assert_eq!(linear.exponent(), 1);

    let squared = ValueDistribution::from_samples_with_exponent(&samples, 2);
    assert_eq!(squared.total(), 50.0); // 9 + 16 + 9 + 16
    assert_eq!(squared.exponent(), 2);
    assert_eq!(squared.zero_count(), 1);

    // The two 4s are 32 of 50 under squaring but only 8 of 14 under |v|, so the
    // exponent moves the level a fraction resolves to.
    assert_eq!(squared.iso_for_fraction(0.64), Some(4.0));
    assert_eq!(linear.iso_for_fraction(0.64), Some(3.0));
}

// --- the cache on `SampledField` --------------------------------------------

#[test]
fn a_sampled_field_caches_its_distribution_and_clones_share_it() {
    let field = SampledField::new(unit_grid([4, 4, 4]), two_level_samples()).unwrap();
    // Warm the cache through the trait, which is how every real caller reaches
    // it.
    assert_eq!(field.value_distribution().unwrap().total(), 100.0);

    let clone = field.clone();
    // Assert *identity*, not presence: a deep copy and a rebuild would both pass
    // a naive "the clone has a distribution" check, and both are exactly what
    // the `Arc` exists to prevent.
    assert!(Arc::ptr_eq(
        &field.shared_value_distribution(),
        &clone.shared_value_distribution()
    ));
}

#[test]
fn a_warm_distribution_is_visible_to_the_memory_estimate() {
    // A memory-bounded value cache bounds itself with `estimate_memory_bytes`;
    // a cached distribution is the one other allocation of comparable size, so
    // it must not be invisible there.
    let field = SampledField::new(unit_grid([16, 16, 16]), gaussian_samples(16)).unwrap();
    let cold = field.estimate_memory_bytes();
    let _ = field.value_distribution();
    assert!(
        field.estimate_memory_bytes() > cold,
        "cold {cold}, warm {}",
        field.estimate_memory_bytes()
    );
}

#[test]
fn an_analytic_field_reports_no_distribution() {
    // The trait default. Every consumer must handle it — an analytic source has
    // no stored samples to distribute.
    #[derive(Debug)]
    struct Analytic;
    impl ScalarField for Analytic {
        fn sample(&self, point: DVec3) -> f64 {
            (-point.length_squared()).exp()
        }
        fn data_bounds(&self) -> Option<atomcad_crystolecule::field::FieldBounds> {
            None
        }
        fn suggested_bounds(&self) -> atomcad_crystolecule::field::FieldBounds {
            atomcad_crystolecule::field::FieldBounds::new(DVec3::splat(-1.0), DVec3::splat(1.0))
        }
        fn native_grid(&self) -> Option<GridGeometry> {
            None
        }
        fn value_range(&self) -> Option<(f64, f64)> {
            None
        }
        fn estimate_memory_bytes(&self) -> usize {
            std::mem::size_of::<Self>()
        }
    }
    assert!(Analytic.value_distribution().is_none());
}
