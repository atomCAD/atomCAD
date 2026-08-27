//! `auto_level` — the rule that picks an isolevel from a field alone.
//!
//! Two tiers, per `doc/design_isosurface_level.md` Part 7. Fields built **in
//! code** pin the tolerance and the two error paths, where an answer is
//! computable by hand. The three **committed** `.cube` fixtures pin the three
//! branches the loader has to cross: accept, reject and signed. Neither tier
//! calibrates the constants — that is the 16-file zoo's job, and its evidence
//! lives in the design document.
//!
//! Design doc: `doc/design_isosurface_level.md`, Part 2 and Part 7 §P2.

use atomcad_crystolecule::field::{
    AutoBasis, DENSITY_LEVEL, FieldBounds, GridGeometry, LevelResolutionError, MAX_LEVEL_RATIO,
    SampledField, ScalarField, auto_level,
};
use atomcad_crystolecule::io::cube_loader::load_cube;
use atomcad_test_support::fixture_path_str;
use glam::DVec3;

// --- helpers ----------------------------------------------------------------

fn unit_grid(dims: [usize; 3]) -> GridGeometry {
    GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims,
    }
}

/// The first field of a committed `.cube` fixture, as the `import_cube` node
/// would produce it.
fn fixture_field(name: &str) -> SampledField {
    let path = fixture_path_str(&format!("cube/{}", name));
    let cube = load_cube(&path, true).expect("the fixture parses");
    cube.fields
        .into_iter()
        .next()
        .expect("the fixture carries a field")
}

/// A field with no stored samples at all — the analytic case (a future Molden
/// orbital). `value_range` is reported so the signedness test still has
/// something to read; `value_distribution` takes the trait default of `None`.
#[derive(Debug)]
struct Analytic {
    range: Option<(f64, f64)>,
}

impl ScalarField for Analytic {
    fn sample(&self, point: DVec3) -> f64 {
        (-point.length_squared()).exp()
    }
    fn data_bounds(&self) -> Option<FieldBounds> {
        None
    }
    fn suggested_bounds(&self) -> FieldBounds {
        FieldBounds::new(DVec3::splat(-1.0), DVec3::splat(1.0))
    }
    fn native_grid(&self) -> Option<GridGeometry> {
        None
    }
    fn value_range(&self) -> Option<(f64, f64)> {
        self.range
    }
    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

/// A blob of `exp(-r)` on an `n³` grid — non-negative, cusped enough that its
/// localized scale lands comfortably inside the plausibility window.
fn blob_samples(n: usize) -> Vec<f32> {
    let centre = (n - 1) as f64 / 2.0;
    let mut samples = Vec::with_capacity(n * n * n);
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                let r = DVec3::new(i as f64 - centre, j as f64 - centre, k as f64 - centre)
                    .length()
                    * 0.4;
                samples.push((-r).exp() as f32);
            }
        }
    }
    samples
}

// --- the committed fixtures: one per branch ---------------------------------

#[test]
fn a_non_negative_density_takes_the_absolute_convention() {
    // `water_density_17x15x19`'s localized scale is 1.8393e-2, a ratio of 9.2
    // against `0.002` — well under `MAX_LEVEL_RATIO`, so the window accepts.
    let field = fixture_field("water_density_17x15x19.cube");
    let (level, basis) = auto_level(&field).expect("a non-negative sampled field always resolves");

    assert_eq!(level, DENSITY_LEVEL);
    assert_eq!(basis, AutoBasis::DensityLike);
    assert_eq!(basis.label(), "non-negative, density-like");

    // And the ratio the branch turned on, so a change to either constant fails
    // here rather than silently re-branching.
    let localized = field
        .value_distribution()
        .unwrap()
        .iso_for_fraction(0.72)
        .unwrap();
    let ratio = localized / DENSITY_LEVEL;
    assert!(
        (ratio - 9.2).abs() < 0.3,
        "expected the fixture's documented ratio of 9.2, got {ratio}"
    );
    assert!(ratio < MAX_LEVEL_RATIO);
}

#[test]
fn a_bounded_analysis_field_is_rejected_by_the_plausibility_window() {
    // The ELF/RDG failure mode: `0.002` is a valid number in *some* field's
    // units, and in this one it would swallow the whole box. The window catches
    // it by comparing against the field's own localized scale (ratio 250).
    let field = fixture_field("elf_like_17x15x19.cube");
    let (level, basis) = auto_level(&field).expect("a non-negative sampled field always resolves");

    assert_eq!(basis, AutoBasis::Atypical);
    assert_eq!(basis.label(), "non-negative, atypical");
    assert!(
        (level - 0.5).abs() < 1e-3,
        "the fallback is the field's own 0.72 scale, ~0.5000; got {level}"
    );
    assert!(
        level > MAX_LEVEL_RATIO * DENSITY_LEVEL,
        "the fallback must be the number that failed the window"
    );
}

#[test]
fn a_signed_field_takes_the_fraction_branch() {
    // No absolute convention survives delocalization for a signed field (an
    // orbital's nominal 0.02-0.05 degrades as ~1/sqrt(N)), so the fraction is
    // the coordinate there.
    let field = fixture_field("p2z_11x11x11.cube");
    let (level, basis) = auto_level(&field).expect("a signed sampled field resolves too");

    assert_eq!(basis, AutoBasis::Signed);
    assert_eq!(basis.label(), "signed field");
    assert!(
        (level - 3.0799e-1).abs() < 1e-4,
        "expected the documented 3.0799e-01, got {level}"
    );
}

// --- the tolerance ----------------------------------------------------------

#[test]
fn one_voxel_of_negative_noise_does_not_flip_the_branch() {
    // What `NEGATIVE_TOLERANCE` buys over a raw `min >= 0`. A plane-wave density
    // interpolated onto a grid rings slightly negative; on a real silicon
    // cluster the two branches are a factor of 43 apart, so a raw sign test
    // would turn one noise voxel into a 43x wrong level, silently.
    let mut samples = blob_samples(11);
    let (_, basis) = auto_level(
        &SampledField::new(unit_grid([11, 11, 11]), samples.clone()).expect("well-formed"),
    )
    .expect("resolves");
    assert_eq!(
        basis,
        AutoBasis::DensityLike,
        "the field starts non-negative"
    );

    samples[0] = -1e-12;
    let (_, noisy_basis) = auto_level(
        &SampledField::new(unit_grid([11, 11, 11]), samples.clone()).expect("well-formed"),
    )
    .expect("resolves");
    assert_eq!(
        noisy_basis,
        AutoBasis::DensityLike,
        "one voxel at -1e-12 is numerical noise, not a signed field"
    );

    // A *genuine* negative lobe still flips it. The smallest real one in the
    // calibration zoo sits at 2.7% of its field's scale, four orders of
    // magnitude above the tolerance — this is 50%.
    samples[0] = -0.5;
    let (_, signed_basis) =
        auto_level(&SampledField::new(unit_grid([11, 11, 11]), samples).expect("well-formed"))
            .expect("resolves");
    assert_eq!(signed_basis, AutoBasis::Signed);
}

// --- the two error paths ----------------------------------------------------

#[test]
fn an_all_zero_field_is_a_descriptive_error_rather_than_a_panic() {
    // The combination easy to miss: `value_distribution` returns `Some` while
    // both of its queries return `None`, so neither may be an `unwrap`.
    let field = SampledField::new(unit_grid([4, 4, 4]), vec![0.0f32; 64])
        .expect("an all-zero field is valid");
    assert!(field.value_distribution().is_some());
    assert!(
        field
            .value_distribution()
            .unwrap()
            .iso_for_fraction(0.72)
            .is_none()
    );

    assert_eq!(
        auto_level(&field),
        Err(LevelResolutionError::AllZeroField),
        "no level encloses anything in a field with no mass"
    );
    assert!(
        LevelResolutionError::AllZeroField
            .to_string()
            .contains("entirely zero"),
        "the message must say what is wrong: {}",
        LevelResolutionError::AllZeroField
    );
}

#[test]
fn an_analytic_field_takes_the_convention_unchecked_or_refuses() {
    // Non-negative: there is no distribution to check `0.002` against, so it is
    // taken unchecked and the basis says so.
    let non_negative = Analytic {
        range: Some((0.0, 1.0)),
    };
    let (level, basis) =
        auto_level(&non_negative).expect("a non-negative analytic field falls back");
    assert_eq!(level, DENSITY_LEVEL);
    assert_eq!(basis, AutoBasis::Unchecked);
    assert_eq!(basis.label(), "non-negative, unchecked");

    // Signed: no absolute convention applies and there is no distribution to
    // take a fraction of, so there is no honest level to offer.
    let signed = Analytic {
        range: Some((-1.0, 1.0)),
    };
    assert_eq!(
        auto_level(&signed),
        Err(LevelResolutionError::AnalyticSignedFieldInAutoMode)
    );

    // A field that reports no range at all is *not known* to be signed, which
    // routes to the same unchecked fallback rather than to an error.
    let unknown = Analytic { range: None };
    let (_, unknown_basis) = auto_level(&unknown).expect("unknown signedness is not an error");
    assert_eq!(unknown_basis, AutoBasis::Unchecked);
}
