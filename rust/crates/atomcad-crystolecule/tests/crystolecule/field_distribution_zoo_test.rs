//! Optional confirmation of `ValueDistribution` against the simulation team's
//! cube zoo — 16 real PySCF/QE fields, ~375 MB, outside the repository.
//!
//! **This is calibration evidence, not a contract.** Every constant in
//! `doc/design_isosurface_level.md` was measured here, and a synthetic fixture
//! can be tuned to accept or reject on demand, so only real producers can say
//! whether `LOCALIZED_FRACTION = 0.72` and `MAX_LEVEL_RATIO = 125` are right.
//! What a committed fixture pins is the *branch*; what this pins is the
//! calibration.
//!
//! **Gated, and loudly skipped.** Point `ATOMCAD_CUBE_ZOO` at the directory to
//! run these; without it each test prints why it did nothing and passes. Keying
//! them to an absolute path would make the suite unrunnable for anyone else, and
//! since no CI runs `cargo test` at all, a silent skip would go quietly dead the
//! day that directory moved. Run these by hand before a release.
//!
//! ```text
//! ATOMCAD_CUBE_ZOO=/c/cube-examples-2026-08-25 cargo test -j 4 zoo -- --nocapture
//! ```

use atomcad_crystolecule::field::distribution::{HISTOGRAM_BINS, ValueDistribution};
use atomcad_crystolecule::field::{GridGeometry, SampledField, ScalarField};
use atomcad_crystolecule::io::cube_loader::load_cube;

/// The two constants `LevelMode::Auto` will compare against
/// (`doc/design_isosurface_level.md` §The plausibility window). They live with
/// the node, not with the distribution, so they are restated here rather than
/// imported — this file is about whether the *measurement* still lands where the
/// design says it does.
const DENSITY_LEVEL: f64 = 0.002;
const MAX_LEVEL_RATIO: f64 = 125.0;

/// The fraction `Auto` resolves a signed or atypical field at.
const LOCALIZED_FRACTION: f64 = 0.72;

/// The zoo root, or `None` with a printed reason.
fn zoo_root(test: &str) -> Option<std::path::PathBuf> {
    match std::env::var("ATOMCAD_CUBE_ZOO") {
        Ok(path) if !path.trim().is_empty() => {
            let path = std::path::PathBuf::from(path.trim());
            if path.is_dir() {
                return Some(path);
            }
            println!("SKIPPED {test}: ATOMCAD_CUBE_ZOO = {path:?} is not a directory");
            None
        }
        _ => {
            println!("SKIPPED {test}: set ATOMCAD_CUBE_ZOO to the cube zoo directory to run it");
            None
        }
    }
}

fn load_field(root: &std::path::Path, relative: &str) -> SampledField {
    let path = root.join(relative);
    let cube = load_cube(path.to_str().expect("zoo path is UTF-8"), false)
        .unwrap_or_else(|error| panic!("loading {path:?}: {error}"));
    cube.fields
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("{path:?} carried no field"))
}

/// `iso_for_fraction(0.72)` and its ratio to the fixed density level — the two
/// numbers `Auto`'s plausibility window is built from.
fn localized_iso_and_ratio(field: &SampledField) -> (f64, f64) {
    let iso = field
        .value_distribution()
        .expect("a sampled field always has a distribution")
        .iso_for_fraction(LOCALIZED_FRACTION)
        .expect("no zoo field is entirely zero");
    (iso, iso / DENSITY_LEVEL)
}

/// The same field with its box cropped about its centre to `scale` of each
/// dimension — the confounder the calibration could not otherwise see, since
/// every file in the zoo carries one padding convention (PySCF's default).
fn crop_about_centre(field: &SampledField, scale: f64) -> SampledField {
    let grid = field.grid();
    let mut dims = [0usize; 3];
    let mut start = [0usize; 3];
    for axis in 0..3 {
        dims[axis] = ((grid.dims[axis] as f64 * scale).round() as usize).clamp(2, grid.dims[axis]);
        start[axis] = (grid.dims[axis] - dims[axis]) / 2;
    }

    let mut samples = Vec::with_capacity(dims[0] * dims[1] * dims[2]);
    for i in 0..dims[0] {
        for j in 0..dims[1] {
            for k in 0..dims[2] {
                samples
                    .push(field.sample_at_index(start[0] + i, start[1] + j, start[2] + k) as f32);
            }
        }
    }

    SampledField::new(
        GridGeometry {
            origin: grid.sample_position(start[0], start[1], start[2]),
            axes: grid.axes,
            dims,
        },
        samples,
    )
    .expect("a crop of a valid grid is a valid grid")
}

/// The four densities small enough to crop, with the ratio each gives as
/// shipped. `si-gemcut` is a density too but at 10M samples it is exercised on
/// its own below.
const CROPPABLE_DENSITIES: [(&str, f64); 4] = [
    ("ch3-radical/ch3_density.cube", 34.4),
    ("esp-sigma-hole/ch3cl.cube", 65.0),
    ("nacl-ecp/nacl.cube", 21.5),
    ("si-cluster-vacancy/si-cluster-S3-vacancy.cube", 43.5),
];

/// The two non-negative impostors the plausibility window exists to reject.
const IMPOSTORS: [(&str, f64); 2] = [
    (
        "si-cluster-derivatives/si-cluster-S3-vacancy_elf.cube",
        248.0,
    ),
    (
        "si-cluster-derivatives/si-cluster-S3-vacancy_rdg.cube",
        271_284.0,
    ),
];

#[test]
fn zoo_densities_and_impostors_land_where_the_calibration_says() {
    let Some(root) = zoo_root("zoo_densities_and_impostors_land_where_the_calibration_says") else {
        return;
    };

    for (relative, expected_ratio) in CROPPABLE_DENSITIES {
        let field = load_field(&root, relative);
        let (iso, ratio) = localized_iso_and_ratio(&field);
        println!("{relative}: iso@0.72 = {iso:.3e}, ratio = {ratio:.1}");
        assert!(
            (0.9..1.1).contains(&(ratio / expected_ratio)),
            "{relative}: ratio {ratio:.1}, design says {expected_ratio}",
        );
        // Every real density is a plausible home for the fixed level.
        assert!(ratio < MAX_LEVEL_RATIO, "{relative}: ratio {ratio:.1}");
        assert!((4.0e-2..1.4e-1).contains(&iso), "{relative}: iso {iso:.3e}");
    }

    for (relative, expected_ratio) in IMPOSTORS {
        let field = load_field(&root, relative);
        let (iso, ratio) = localized_iso_and_ratio(&field);
        println!("{relative}: iso@0.72 = {iso:.3e}, ratio = {ratio:.3e}");
        assert!(
            (0.9..1.1).contains(&(ratio / expected_ratio)),
            "{relative}: ratio {ratio:.3e}, design says {expected_ratio:.3e}",
        );
        // ...and neither impostor is: `0.002` is nowhere near a level in their
        // units, which is exactly what the window measures.
        assert!(ratio > MAX_LEVEL_RATIO, "{relative}: ratio {ratio:.3e}");
    }
}

#[test]
fn zoo_padding_does_not_move_a_density_across_the_plausibility_window() {
    // The ratio is the statistic the window uses *because* it barely moves when
    // the box is redrawn, where the enclosed voxel share moves enormously — a
    // CH3 density goes from 36.7% of voxels to 65.2% at a 20% tighter box. The
    // ratio moves by ~1.7x across the same crops and never leaves its band.
    let Some(root) = zoo_root("zoo_padding_does_not_move_a_density_across_the_plausibility_window")
    else {
        return;
    };

    for (relative, _) in CROPPABLE_DENSITIES {
        let field = load_field(&root, relative);
        for scale in [1.0, 0.8, 0.6, 0.5] {
            let cropped = crop_about_centre(&field, scale);
            let (_, ratio) = localized_iso_and_ratio(&cropped);
            println!("{relative} @ {scale}x: ratio = {ratio:.1}");
            assert!(
                ratio < MAX_LEVEL_RATIO,
                "{relative} @ {scale}x: ratio {ratio:.1} left the density band",
            );
        }
    }

    for (relative, _) in IMPOSTORS {
        let field = load_field(&root, relative);
        for scale in [1.0, 0.8, 0.6, 0.5] {
            let cropped = crop_about_centre(&field, scale);
            let (_, ratio) = localized_iso_and_ratio(&cropped);
            println!("{relative} @ {scale}x: ratio = {ratio:.3e}");
            assert!(
                ratio > MAX_LEVEL_RATIO,
                "{relative} @ {scale}x: ratio {ratio:.3e} fell into the density band",
            );
        }
    }
}

#[test]
fn zoo_gemcut_crosses_the_exact_sample_limit_at_real_scale() {
    // The one file in the zoo above `EXACT_SAMPLE_LIMIT`: 247x247x164 =
    // 10,005,476 samples, which as an exact structure would be 120 MB. The
    // in-code tests force the limit down to reach this path in microseconds;
    // this is the same path with a real input at real scale.
    let Some(root) = zoo_root("zoo_gemcut_crosses_the_exact_sample_limit_at_real_scale") else {
        return;
    };

    let field = load_field(&root, "si-gemcut-2D3R-149/si-gemcut-2D3R-149-S3.cube");
    assert_eq!(field.grid().sample_count(), 10_005_476);
    let distribution = field.value_distribution().unwrap();
    assert!(
        !distribution.is_exact(),
        "10M samples must fall to the histogram path",
    );

    let (iso, ratio) = localized_iso_and_ratio(&field);
    println!("si-gemcut S3 (histogram): iso@0.72 = {iso:.3e}, ratio = {ratio:.1}");
    assert!(ratio < MAX_LEVEL_RATIO);

    // What the exact structure would have said, built here on purpose: 120 MB
    // for one field, which is precisely the cost `EXACT_SAMPLE_LIMIT` declines
    // to pay per field in a network holding several. The design's calibration
    // number for this file (ratio 44.6) is this one.
    let exact = ValueDistribution::with_limit(field.samples_slice(), 1, usize::MAX);
    let exact_iso = exact.iso_for_fraction(LOCALIZED_FRACTION).unwrap();
    println!(
        "si-gemcut S3 (exact):     iso@0.72 = {exact_iso:.3e}, ratio = {:.1}",
        exact_iso / DENSITY_LEVEL
    );
    assert!(
        (0.9..1.1).contains(&(exact_iso / DENSITY_LEVEL / 44.6)),
        "exact ratio {:.1}, design says 44.6",
        exact_iso / DENSITY_LEVEL,
    );

    // The two agree to within one bin — and the histogram answer is the *lower*
    // edge of the crossing bin, so it is biased low by construction rather than
    // scattered. Over this field's ~44 decades that is ~5%, an order of
    // magnitude below the 1.7x gap the plausibility window decides on.
    let (lo, hi) = exact.nonzero_range().unwrap();
    let bin_width = (hi / lo).log10() / HISTOGRAM_BINS as f64;
    println!(
        "one bin over this field's range: {:.2}%",
        100.0 * (10f64.powf(bin_width) - 1.0)
    );
    assert!(iso <= exact_iso, "the histogram answer must not overshoot");
    assert!(
        (exact_iso / iso).log10() <= bin_width,
        "histogram {iso:.4e} and exact {exact_iso:.4e} are more than a bin apart",
    );
}
