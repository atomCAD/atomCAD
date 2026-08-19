//! `.cube` import, against the committed fixtures in `rust/tests/fixtures/cube/`.
//!
//! Those fixtures come from `scripts/make_cube_fixtures.py tests`, but the
//! committed file is the artifact under test: every assertion here is against a
//! literal a reviewer can check by eye, never against whatever the generator
//! happened to emit.

use atomcad_crystolecule::field::ScalarField;
use atomcad_crystolecule::io::cube_loader::{
    BOHR_TO_ANGSTROM, CubeError, CubeFile, load_cube, load_cube_from_str,
};
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_test_support::fixture_path_str;
use glam::DVec3;

fn cube_fixture(name: &str) -> String {
    fixture_path_str(&format!("cube/{name}"))
}

fn load(name: &str) -> CubeFile {
    load_cube(&cube_fixture(name), true).unwrap_or_else(|e| panic!("{name} should load, but: {e}"))
}

fn load_err(name: &str) -> CubeError {
    match load_cube(&cube_fixture(name), true) {
        Ok(_) => panic!("{name} should have been rejected"),
        Err(e) => e,
    }
}

fn positions_sorted(
    structure: &atomcad_crystolecule::atomic_structure::AtomicStructure,
) -> Vec<(i16, DVec3)> {
    let mut atoms: Vec<(i16, DVec3)> = structure
        .atoms_values()
        .map(|a| (a.atomic_number, a.position))
        .collect();
    atoms.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.x.partial_cmp(&b.1.x).unwrap())
            .then(a.1.z.partial_cmp(&b.1.z).unwrap())
    });
    atoms
}

// --- the ramp: axis order, the bug class that hides in symmetric fixtures ----

/// THE test of the phase. Any axis transposition or mirroring in the value
/// block is invisible in an axis-symmetric fixture; the ramp encodes its own
/// index, on three *different* dimensions, so a transposition cannot even
/// preserve the shape.
#[test]
fn ramp_sample_at_every_grid_point_returns_its_own_index_code() {
    let cube = load("ramp_3x4x5.cube");
    let field = &cube.fields[0];
    let grid = field.native_grid().unwrap();
    assert_eq!(grid.dims, [3, 4, 5]);

    for i in 0..3 {
        for j in 0..4 {
            for k in 0..5 {
                let expected = (100 * i + 10 * j + k) as f64;
                let point = grid.sample_position(i, j, k);
                assert!(
                    (field.sample(point) - expected).abs() < 1e-6,
                    "sample ({i},{j},{k}) at {point} gave {} not {expected}",
                    field.sample(point)
                );
            }
        }
    }
}

#[test]
fn ramp_grid_is_read_in_angstrom_with_the_node_centered_bounds_convention() {
    let cube = load("ramp_3x4x5.cube");
    let grid = cube.fields[0].native_grid().unwrap();
    assert!(grid.origin.abs_diff_eq(DVec3::ZERO, 1e-12));
    // The file says 1.889726 Bohr per step, which is 1.0 Angstrom.
    assert!(
        (grid.spacing() - DVec3::ONE).length() < 1e-5,
        "{}",
        grid.spacing()
    );
    // 3x4x5 samples span 2x3x4 steps: through the outermost samples, not half a
    // voxel past them.
    let bounds = cube.fields[0].suggested_bounds();
    assert!(bounds.min.abs_diff_eq(DVec3::ZERO, 1e-12));
    assert!(bounds.max.abs_diff_eq(DVec3::new(2.0, 3.0, 4.0), 1e-5));
}

#[test]
fn ramp_interpolates_trilinearly_between_stored_samples() {
    let cube = load("ramp_3x4x5.cube");
    let field = &cube.fields[0];
    // Midway between (0,0,0)=0 and (1,0,0)=100.
    assert!((field.sample(DVec3::new(0.5, 0.0, 0.0)) - 50.0).abs() < 1e-4);
    // Centre of the first cell: mean of 0, 1, 10, 11, 100, 101, 110, 111.
    assert!((field.sample(DVec3::splat(0.5)) - 55.5).abs() < 1e-4);
}

#[test]
fn ramp_value_range_matches_the_fixtures_literal_extremes() {
    let cube = load("ramp_3x4x5.cube");
    let (min, max) = cube.fields[0].value_range().unwrap();
    assert_eq!(min, 0.0);
    assert_eq!(max, 234.0); // 100*2 + 10*3 + 4
}

#[test]
fn sampling_outside_the_box_returns_exactly_zero_not_an_error() {
    let cube = load("ramp_3x4x5.cube");
    let field = &cube.fields[0];
    assert_eq!(field.sample(DVec3::new(-0.5, 0.0, 0.0)), 0.0);
    assert_eq!(field.sample(DVec3::new(0.0, 0.0, 100.0)), 0.0);
}

#[test]
fn a_single_atom_file_produces_no_units_warning() {
    // Fewer than two atoms means no distances to check; the plausibility check
    // must stay silent rather than invent a ratio.
    let cube = load("ramp_3x4x5.cube");
    assert_eq!(cube.atoms.get_num_of_atoms(), 1);
    assert_eq!(cube.units_warning, None);
}

#[test]
fn every_fixtures_value_range_matches_the_extremes_of_its_stored_samples() {
    for name in [
        "ramp_3x4x5.cube",
        "p2z_11x11x11.cube",
        "water_bohr.cube",
        "water_angstrom.cube",
        "two_fragments.cube",
    ] {
        let cube = load(name);
        let field = &cube.fields[0];
        let samples = field.samples_slice();
        let min = samples.iter().copied().fold(f32::INFINITY, f32::min) as f64;
        let max = samples.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
        assert_eq!(field.value_range(), Some((min, max)), "{name}");
    }
}

// --- the synthetic 2p_z: signs and gradients --------------------------------

/// `z * exp(-0.25 * r^2)`, matching `p2z()` in `scripts/make_cube_fixtures.py`.
fn p2z_value(p: DVec3) -> f64 {
    p.z * (-0.25 * p.length_squared()).exp()
}

/// Its exact gradient, differentiated by hand.
fn p2z_analytic_gradient(p: DVec3) -> DVec3 {
    let e = (-0.25 * p.length_squared()).exp();
    DVec3::new(
        -0.5 * p.x * p.z * e,
        -0.5 * p.y * p.z * e,
        (1.0 - 0.5 * p.z * p.z) * e,
    )
}

#[test]
fn signed_values_survive_the_round_trip_and_change_sign_across_the_nodal_plane() {
    let cube = load("p2z_11x11x11.cube");
    let field = &cube.fields[0];
    let grid = field.native_grid().unwrap();
    assert_eq!(grid.dims, [11, 11, 11]);

    // k = 5 is the z = 0 plane; below it the field is negative, above positive,
    // and on it exactly zero. Nothing clamps the negative half away.
    for i in 0..11 {
        for j in 0..11 {
            assert!(field.sample_at_index(i, j, 0) < 0.0);
            assert_eq!(field.sample_at_index(i, j, 5), 0.0);
            assert!(field.sample_at_index(i, j, 10) > 0.0);
        }
    }
    let (min, max) = field.value_range().unwrap();
    assert!(min < 0.0 && max > 0.0);
    assert!((min + max).abs() < 1e-6, "the field is antisymmetric in z");
}

#[test]
fn sampled_values_match_the_formula_the_fixture_was_generated_from() {
    let cube = load("p2z_11x11x11.cube");
    let field = &cube.fields[0];
    let grid = field.native_grid().unwrap();
    for (i, j, k) in [(0, 0, 0), (3, 7, 2), (5, 5, 8), (10, 10, 10)] {
        let p = grid.sample_position(i, j, k);
        let stored = field.sample_at_index(i, j, k);
        assert!(
            (stored - p2z_value(p)).abs() < 1e-5,
            "({i},{j},{k}) at {p}: stored {stored}, formula {}",
            p2z_value(p)
        );
    }
}

#[test]
fn gradient_matches_the_analytic_derivative_of_the_generating_formula() {
    let cube = load("p2z_11x11x11.cube");
    let field = &cube.fields[0];
    let grid = field.native_grid().unwrap();

    // Central differences at the fixture's 0.4 A spacing, against a peak
    // analytic gradient magnitude of 1.0. Interior points only, since the
    // boundary planes fall back to a one-sided difference.
    const TOLERANCE: f64 = 0.05;
    let mut worst = 0.0f64;
    for i in 1..10 {
        for j in 1..10 {
            for k in 1..10 {
                let p = grid.sample_position(i, j, k);
                let error = (field.gradient(p) - p2z_analytic_gradient(p)).length();
                worst = worst.max(error);
                assert!(
                    error < TOLERANCE,
                    "({i},{j},{k}) at {p}: gradient {} vs analytic {}",
                    field.gradient(p),
                    p2z_analytic_gradient(p)
                );
            }
        }
    }
    // Guard against the tolerance silently becoming vacuous.
    assert!(worst > 1e-4, "worst error {worst} looks suspiciously exact");
}

// --- the atom block ---------------------------------------------------------

#[test]
fn the_atom_block_agrees_with_an_independent_xyz_reference() {
    let cube = load("water_bohr.cube");
    let reference = load_xyz(&cube_fixture("water_reference.xyz"), false).unwrap();

    assert_eq!(cube.atoms.get_num_of_atoms(), reference.get_num_of_atoms());
    for ((z_cube, p_cube), (z_ref, p_ref)) in positions_sorted(&cube.atoms)
        .into_iter()
        .zip(positions_sorted(&reference))
    {
        assert_eq!(z_cube, z_ref);
        assert!(
            p_cube.abs_diff_eq(p_ref, 1e-5),
            "cube {p_cube} vs xyz {p_ref}"
        );
    }
}

#[test]
fn bohr_coordinates_convert_to_chemically_sane_angstrom_bond_lengths() {
    let cube = load("water_bohr.cube");
    let atoms = positions_sorted(&cube.atoms);
    let hydrogens: Vec<DVec3> = atoms.iter().filter(|a| a.0 == 1).map(|a| a.1).collect();
    let oxygen = atoms.iter().find(|a| a.0 == 8).unwrap().1;
    assert_eq!(hydrogens.len(), 2);

    for h in &hydrogens {
        let d = oxygen.distance(*h);
        assert!((d - 0.958).abs() < 1e-3, "O-H is {d} A, expected 0.958");
    }
    let a = (hydrogens[0] - oxygen).normalize();
    let b = (hydrogens[1] - oxygen).normalize();
    let angle = a.dot(b).clamp(-1.0, 1.0).acos().to_degrees();
    assert!((angle - 104.5).abs() < 0.1, "H-O-H is {angle} deg");

    assert_eq!(cube.units_warning, None);
}

#[test]
fn auto_bonding_runs_when_asked_and_not_otherwise() {
    let bonded = load_cube(&cube_fixture("water_bohr.cube"), true).unwrap();
    assert_eq!(bonded.atoms.get_num_of_bonds(), 2);
    let bare = load_cube(&cube_fixture("water_bohr.cube"), false).unwrap();
    assert_eq!(bare.atoms.get_num_of_bonds(), 0);
}

// --- the units plausibility check: warns, never re-interprets ---------------

#[test]
fn an_angstrom_file_read_as_bohr_warns_and_is_still_read_as_bohr() {
    let cube = load("water_angstrom.cube");
    let warning = cube
        .units_warning
        .as_ref()
        .expect("an Angstrom file read as Bohr must trip the low bound");
    assert!(warning.contains("0.52"), "should name the ratio: {warning}");

    // The load-bearing half: the check must NOT rescale. These coordinates are
    // the file's Angstrom numbers *interpreted as Bohr*, i.e. 1.89x too small,
    // and they must have been left exactly that way.
    let atoms = positions_sorted(&cube.atoms);
    let oxygen = atoms.iter().find(|a| a.0 == 8).unwrap().1;
    let hydrogen = atoms.iter().find(|a| a.0 == 1).unwrap().1;
    let d = oxygen.distance(hydrogen);
    assert!(
        (d - 0.958 * BOHR_TO_ANGSTROM).abs() < 1e-4,
        "O-H is {d} A; expected the unrescaled {}",
        0.958 * BOHR_TO_ANGSTROM
    );

    // The field still loads and is usable — the warning is advisory only.
    assert!(cube.fields[0].value_range().unwrap().1 > 0.0);
}

#[test]
fn widely_separated_fragments_trip_the_high_bound_without_moving_anything() {
    let cube = load("two_fragments.cube");
    assert!(
        cube.units_warning.is_some(),
        "20 Bohr between two carbons should trip the high bound"
    );
    let atoms = positions_sorted(&cube.atoms);
    let d = atoms[0].1.distance(atoms[1].1);
    assert!(
        (d - 20.0 * BOHR_TO_ANGSTROM).abs() < 1e-6,
        "separation is {d} A; the check must never alter the parse"
    );
}

// --- malformed input: descriptive errors, never a panic ---------------------

#[test]
fn a_truncated_value_block_says_where_it_ran_out() {
    let message = load_err("truncated.cube").to_string();
    assert!(message.contains("file ended"), "{message}");
    assert!(message.contains("value block"), "{message}");
    // 12 numbers of grid header + 5 for the one atom + the 20 samples present.
    assert!(message.contains("37 numbers"), "{message}");
}

#[test]
fn a_non_numeric_sample_names_the_offending_token() {
    let message = load_err("non_numeric.cube").to_string();
    assert!(message.contains("abc"), "{message}");
}

#[test]
fn a_zero_grid_dimension_is_rejected() {
    let message = load_err("zero_dim.cube").to_string();
    assert!(message.contains("dimension 2"), "{message}");
}

#[test]
fn extra_samples_beyond_the_declared_grid_are_rejected() {
    // The mirror image of truncation: a value block longer than the header
    // promises means the header and the body disagree about the grid.
    let mut text = std::fs::read_to_string(cube_fixture("ramp_3x4x5.cube")).unwrap();
    text.push_str("  9.99000E+02\n");
    let error = load_cube_from_str(&text, false).unwrap_err().to_string();
    assert!(error.contains("exactly 60 samples"), "{error}");
}

#[test]
fn the_multi_field_variant_is_rejected_rather_than_misparsed() {
    // Negative `natoms` is a FLAG, not a count. Until multi-field support lands
    // the honest answer is a clear refusal — the interleaved value block would
    // otherwise parse into plausible garbage.
    let error = load_err("multi_field.cube");
    assert!(
        matches!(error, CubeError::Unsupported(_)),
        "expected Unsupported, got {error:?}"
    );
    assert!(error.to_string().contains("multi-field"), "{error}");
}

#[test]
fn a_declared_values_per_point_above_one_is_also_the_multi_field_variant() {
    // Some writers emit the optional fifth number on line 3 instead of (or as
    // well as) flipping the sign of `natoms`.
    let text = std::fs::read_to_string(cube_fixture("ramp_3x4x5.cube")).unwrap();
    let with_nval = text.replacen(
        "    1    0.000000    0.000000    0.000000",
        "    1    0.000000    0.000000    0.000000    2",
        1,
    );
    let error = load_cube_from_str(&with_nval, false).unwrap_err();
    assert!(matches!(error, CubeError::Unsupported(_)), "{error:?}");
}

#[test]
fn a_declared_values_per_point_of_one_is_accepted() {
    let text = std::fs::read_to_string(cube_fixture("ramp_3x4x5.cube")).unwrap();
    let with_nval = text.replacen(
        "    1    0.000000    0.000000    0.000000",
        "    1    0.000000    0.000000    0.000000    1",
        1,
    );
    let cube = load_cube_from_str(&with_nval, false).unwrap();
    assert_eq!(cube.fields[0].value_range(), Some((0.0, 234.0)));
}

#[test]
fn an_empty_or_headerless_file_is_rejected_without_panicking() {
    for text in ["", "just one line\n", "one\ntwo\n"] {
        assert!(load_cube_from_str(text, false).is_err(), "{text:?}");
    }
}

// --- token-stream parsing, not line-by-line ---------------------------------

#[test]
fn whitespace_and_line_wrapping_in_the_body_do_not_matter() {
    // Parsing rule 3: files in the wild vary in wrapping more than the format's
    // description suggests, so everything after line 3 is a token stream.
    let text = std::fs::read_to_string(cube_fixture("ramp_3x4x5.cube")).unwrap();
    let mut lines = text.lines();
    let head: Vec<&str> = lines.by_ref().take(3).collect();
    let rest: Vec<&str> = lines.collect();

    // Re-flow everything after line 3 onto a single very long line, with wildly
    // irregular spacing and blank lines around it.
    let reflowed = format!(
        "{}\n\n   {}   \n\n",
        head.join("\n"),
        rest.join("  ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("   ")
    );
    let cube = load_cube_from_str(&reflowed, false).unwrap();
    assert_eq!(cube.fields[0].value_range(), Some((0.0, 234.0)));
    assert_eq!(cube.atoms.get_num_of_atoms(), 1);
}
