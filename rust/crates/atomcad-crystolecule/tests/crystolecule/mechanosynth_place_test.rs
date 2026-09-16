//! P2 tests for the placement engine: the role rule, the rigid fit, the search
//! bounds, the bond-derived fallback, dedupe and ranking, the diagnostics, and
//! the two end-to-end claims — every candidate replays, and a re-authored build
//! is exact. See `doc/design_mechanosynth_editor.md` §Phases, Phase 2.
//!
//! `place_ops.json` holds one operation per shape the engine has to
//! distinguish, and `place_workpiece.xyz` hosts every one of them exactly, in
//! sub-clusters far enough apart that nothing cross-matches. The two files are
//! generated from the same constants this file repeats below; keep them in
//! step.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_crystolecule::mechanosynth::{
    Applicability, BuildScript, Candidate, EXACT_FIT_RESIDUAL, GhostBondKind, GhostKind,
    HighlightTags, MechanosynthError, NEAR_MISS_FACTOR, OpLibrary, Operation,
    RESIDUAL_RANK_EPSILON, Step, applicable_ops, applicable_ops_where, apply_step,
    compare_structures, describe_mismatches, load_build_script, load_library, parse_library, place,
    place_with_stats, preview_atoms, preview_bonds, replay, resolve_tolerance,
};
use atomcad_test_support::{fixture_path, fixture_path_str};
use glam::{DMat3, DQuat, DVec3};

const H: i16 = 1;
const C: i16 = 6;
const N: i16 = 7;
const O: i16 = 8;
const SI: i16 = 14;

/// The geometry constants `place_ops.json` and `place_workpiece.xyz` are built
/// from.
const BOND: f64 = 2.352;
const HB: f64 = 1.48;
const DIMER_D: f64 = 3.50;

/// The tolerance the placement tests run at unless they are about tolerance.
/// `place_ops.json` states none, so this is also `resolve_tolerance`'s answer.
const TOL: f64 = 0.05;

/// Ideal coordinates, so structural comparisons are far tighter than any match
/// tolerance.
const EXACT: f64 = 1e-9;

// ============================================================================
// Fixtures and helpers
// ============================================================================

fn library() -> OpLibrary {
    load_library(&fixture_path("mechanosynth/place_ops.json")).expect("place_ops.json should parse")
}

fn workpiece() -> AtomicStructure {
    load_xyz(&fixture_path_str("mechanosynth/place_workpiece.xyz"), true)
        .expect("place_workpiece.xyz should load")
}

fn gold_script() -> BuildScript {
    load_build_script(&fixture_path("mechanosynth/gold_build.json"))
        .expect("gold_build.json should parse")
}

/// The four tetrahedral directions the fixtures are built on.
fn tetra(i: usize) -> DVec3 {
    let s = 1.0 / 3.0f64.sqrt();
    [
        DVec3::new(s, s, s),
        DVec3::new(s, -s, -s),
        DVec3::new(-s, s, -s),
        DVec3::new(-s, -s, s),
    ][i]
}

/// Sub-cluster origins of `place_workpiece.xyz`.
const A: DVec3 = DVec3::new(0.0, 0.0, 0.0); // 3-coordinate Si, three Si frames
const B: DVec3 = DVec3::new(20.0, 0.0, 0.0); // Si with two partners at DIMER_D
const CC: DVec3 = DVec3::new(0.0, 20.0, 0.0); // planar C/O/N
const D: DVec3 = DVec3::new(20.0, 20.0, 0.0); // planar Si/C/N/O
const E: DVec3 = DVec3::new(0.0, 0.0, 20.0); // Si with three H
const F: DVec3 = DVec3::new(20.0, 0.0, 20.0); // one C, one H
const G: DVec3 = DVec3::new(0.0, 0.0, -20.0); // Si with C/N/O frames, turned 90°

/// The only atom within a hair of `pos`, or a panic naming what was there. The
/// window is wider than the 1e-6 the fixtures round to and far narrower than
/// the gap between any two atoms in them.
fn at(s: &AtomicStructure, pos: DVec3) -> u32 {
    let found = s.get_atoms_in_radius(&pos, 1e-4);
    assert_eq!(found.len(), 1, "expected exactly one atom at {pos:?}");
    found[0]
}

fn position(s: &AtomicStructure, atom_id: u32) -> DVec3 {
    s.get_atom(atom_id).expect("atom exists").position
}

fn element(s: &AtomicStructure, atom_id: u32) -> i16 {
    s.get_atom(atom_id).expect("atom exists").atomic_number
}

fn op<'a>(lib: &'a OpLibrary, name: &str) -> &'a Operation {
    lib.get(name).unwrap_or_else(|| panic!("{name} in fixture"))
}

fn assert_same(a: &AtomicStructure, b: &AtomicStructure) {
    let mismatches = compare_structures(a, b, 1e-6);
    assert!(
        mismatches.is_empty(),
        "structures differ:\n{}",
        describe_mismatches(&mismatches)
    );
}

fn expect_err(result: Result<Vec<Candidate>, MechanosynthError>) -> MechanosynthError {
    match result {
        Ok(candidates) => panic!("expected an error, got {} candidates", candidates.len()),
        Err(e) => e,
    }
}

fn only(candidates: Vec<Candidate>) -> Candidate {
    assert_eq!(
        candidates.len(),
        1,
        "expected exactly one candidate, got {}",
        candidates.len()
    );
    candidates.into_iter().next().expect("one candidate")
}

/// Two rigid transforms agree to `EXACT`.
fn assert_rigid_eq(step: &Step, r: &DMat3, t: DVec3) {
    for i in 0..3 {
        assert!(
            step.r.col(i).abs_diff_eq(r.col(i), 1e-9),
            "rotation column {i}: {:?} vs {:?}",
            step.r.col(i),
            r.col(i)
        );
    }
    assert!(
        step.t.abs_diff_eq(t, 1e-9),
        "translation: {:?} vs {:?}",
        step.t,
        t
    );
}

/// A proper rotation that is nothing like the identity and is the same on every
/// run — "random" in the sense the design means, without an RNG in a test.
fn odd_rotation() -> DMat3 {
    DMat3::from_quat(DQuat::from_euler(glam::EulerRot::XYZ, 0.731, -1.219, 2.407))
}

/// `odd_rotation` with a reflection folded in, so `det = −1`.
fn odd_improper() -> DMat3 {
    odd_rotation() * DMat3::from_cols(DVec3::X, DVec3::Y, DVec3::new(0.0, 0.0, -1.0))
}

/// A copy of `source` rigidly moved by `(r, t)`, atom ids preserved in order.
fn transformed(source: &AtomicStructure, r: DMat3, t: DVec3) -> AtomicStructure {
    let mut moved = source.clone();
    for atom_id in source.atom_ids().cloned().collect::<Vec<u32>>() {
        let p = position(source, atom_id);
        moved.set_atom_position(atom_id, r * p + t);
    }
    moved
}

/// A structure holding only the atoms of `source` within `radius` of `centre`,
/// bonds re-inferred. Used to isolate one sub-cluster from the fixture.
fn cluster(source: &AtomicStructure, centre: DVec3, radius: f64) -> AtomicStructure {
    let mut out = AtomicStructure::new();
    for atom_id in source.get_atoms_in_radius(&centre, radius) {
        out.add_atom(element(source, atom_id), position(source, atom_id));
    }
    atomcad_crystolecule::atomic_structure_utils::auto_create_bonds(&mut out);
    out
}

/// Every workpiece atom the candidate's `before` pattern will match, in pattern
/// order, resolved the way [`apply_step`] resolves them.
///
/// The contract between the two halves of the engine: what `place` reports in
/// `roles` is what the replay will find.
fn replay_matches(
    workpiece: &AtomicStructure,
    operation: &Operation,
    step: &Step,
    tolerance: f64,
) -> Vec<(i64, u32)> {
    let mut claimed: Vec<u32> = Vec::new();
    let mut matched = Vec::new();
    for pattern_atom in &operation.before.atoms {
        let found = workpiece.nearest_unclaimed_atom(
            step.place(pattern_atom.pos),
            tolerance,
            pattern_atom.element.required_atomic_number(),
            |atom_id| claimed.contains(&atom_id),
        );
        let found = found.unwrap_or_else(|| {
            panic!(
                "candidate for {} leaves before atom {} unmatched",
                operation.name, pattern_atom.id
            )
        });
        claimed.push(found.atom_id);
        matched.push((pattern_atom.id, found.atom_id));
    }
    matched
}

/// The shared assertion: a candidate replays, its `roles` are the atoms the
/// replay matches, and it adds exactly the atoms only `after` names.
///
/// The design's wording was "the atoms it touches are exactly the workpiece
/// atoms in `roles`"; `touched` is derived from *effect* since P1, so a frame
/// atom is in `roles` and not touched, and a deletion's bonded neighbour is
/// touched and not in `roles`. The match map is the invariant that was meant.
fn assert_candidate_replays(
    base: &AtomicStructure,
    operation: &Operation,
    candidate: &Candidate,
    tolerance: f64,
) {
    let mut workpiece = base.clone();
    let effect = apply_step(&mut workpiece, operation, &candidate.step, 1, tolerance)
        .unwrap_or_else(|e| panic!("candidate for {} should replay: {e}", operation.name));

    let mut expected = candidate.roles.clone();
    expected.sort_unstable();
    let mut found = replay_matches(base, operation, &candidate.step, tolerance);
    found.sort_unstable();
    assert_eq!(
        expected, found,
        "{}: roles disagree with what the replay matched",
        operation.name
    );

    let added = operation
        .after
        .atoms
        .iter()
        .filter(|atom| !operation.before.has(atom.id))
        .count();
    assert_eq!(
        effect.added.len(),
        added,
        "{}: added-atom count",
        operation.name
    );
    assert!(
        candidate.exact == (candidate.residual < EXACT_FIT_RESIDUAL),
        "{}: `exact` must mean `residual < EXACT_FIT_RESIDUAL`",
        operation.name
    );
}

// ============================================================================
// The role rule
// ============================================================================

#[test]
fn clicking_either_end_of_an_abstraction_places_the_same_step() {
    let lib = library();
    let s = workpiece();
    let host = at(&s, F);
    let hydrogen = at(&s, F + tetra(0) * 1.09);

    let from_host = only(place(&s, &lib, "habst_pair", host, TOL).expect("host click places"));
    let from_hydrogen =
        only(place(&s, &lib, "habst_pair", hydrogen, TOL).expect("hydrogen click places"));

    // The origin atom is the C, so the C click takes role 1 and the H click —
    // which no other slot admits — takes role 2. Same reaction either way.
    assert_eq!(from_host.role, 1);
    assert_eq!(from_hydrogen.role, 2);
    assert!(
        from_host.step.t.abs_diff_eq(from_hydrogen.step.t, EXACT),
        "{:?} vs {:?}",
        from_host.step.t,
        from_hydrogen.step.t
    );
    for i in 0..3 {
        assert!(
            from_host
                .step
                .r
                .col(i)
                .abs_diff_eq(from_hydrogen.step.r.col(i), EXACT),
            "rotation column {i} differs"
        );
    }
}

#[test]
fn the_clicked_atom_of_an_asymmetric_op_is_the_one_that_reacts() {
    let lib = library();
    let s = workpiece();
    let partner = at(&s, B + DVec3::new(DIMER_D, 0.0, 0.0));

    let candidate = only(place(&s, &lib, "asym_add", partner, TOL).expect("places"));
    assert_eq!(candidate.role, 1, "the clicked atom takes the origin role");

    let mut after = s.clone();
    apply_step(&mut after, op(&lib, "asym_add"), &candidate.step, 1, TOL).expect("replays");
    let added = after
        .get_atoms_in_radius(&position(&s, partner), HB + 1e-6)
        .into_iter()
        .filter(|id| element(&after, *id) == H)
        .count();
    assert_eq!(added, 1, "the H lands on the clicked atom");
    assert!(
        after
            .get_atoms_in_radius(&position(&s, at(&s, B)), HB + 1e-6)
            .into_iter()
            .all(|id| element(&after, id) != H),
        "and not on its neighbour"
    );
}

#[test]
fn a_silicon_click_on_a_wildcard_frame_donation_is_always_the_origin_role() {
    let lib = library();
    let s = workpiece();
    // Roles 2, 3 and 4 are `"*"` and admit Si as readily as role 1 does; the
    // origin clause decides, and it decides the same way every time.
    let candidate = only(place(&s, &lib, "hdon_frame", at(&s, A), TOL).expect("places"));
    assert_eq!(candidate.role, 1);
}

#[test]
fn an_op_with_no_atom_at_the_origin_falls_to_the_smallest_eligible_id() {
    let lib = library();
    let s = workpiece();
    let candidates = place(&s, &lib, "off_origin_all", at(&s, B), TOL).expect("places");
    assert_eq!(candidates.len(), 2, "two partners at the pattern distance");
    for candidate in &candidates {
        assert_eq!(
            candidate.role, 2,
            "ids are 2 and 3; neither is at the origin"
        );
    }
}

#[test]
fn a_failed_fit_names_the_role_it_tried_instead_of_retrying_another() {
    let lib = library();
    let s = workpiece();
    // A frame atom of the A site. It would fit `hdon_frame` perfectly as role
    // 2, 3 or 4 — and the rule gave it role 1, which has no three neighbours.
    let frame = at(&s, A + tetra(0) * BOND);
    match expect_err(place(&s, &lib, "hdon_frame", frame, TOL)) {
        MechanosynthError::NoPlacement { op, role, .. } => {
            assert_eq!(op, "hdon_frame");
            assert_eq!(role, 1);
        }
        other => panic!("expected NoPlacement, got {other}"),
    }
}

#[test]
fn every_candidate_of_one_call_plays_the_same_role() {
    let lib = library();
    let s = workpiece();
    let candidates = place(&s, &lib, "dimerize", at(&s, B), TOL).expect("places");
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().all(|c| c.role == 1));
}

// ============================================================================
// The fit
// ============================================================================

#[test]
fn a_one_atom_abstraction_fits_exactly_at_the_clicked_position() {
    let lib = library();
    let s = workpiece();
    let hydrogen = at(&s, E + tetra(0) * HB);

    let candidate = only(place(&s, &lib, "habst", hydrogen, TOL).expect("places"));
    assert_eq!(candidate.step.r, DMat3::IDENTITY);
    assert!(candidate.step.t.abs_diff_eq(position(&s, hydrogen), EXACT));
    assert_eq!(candidate.residual, 0.0);
    assert!(candidate.exact);
    assert!(!candidate.mirrored);
    assert!(!candidate.approximate);
}

#[test]
fn frame_atoms_recover_the_rotation_of_a_moved_host() {
    let lib = library();
    let base = cluster(&workpiece(), G, 4.0);
    let settled = only(place(&base, &lib, "hdon_uniq", at(&base, G), TOL).expect("places"));

    // Move the whole host rigidly and the fit has to follow it exactly: the
    // recovered transform is the move composed with the one it found before.
    let (r, t) = (odd_rotation(), DVec3::new(-3.7, 11.2, 0.9));
    let moved = transformed(&base, r, t);
    let found = only(place(&moved, &lib, "hdon_uniq", at(&moved, r * G + t), TOL).expect("places"));

    assert!(!found.mirrored);
    assert!(found.residual < 1e-9, "residual {}", found.residual);
    assert_rigid_eq(&found.step, &(r * settled.step.r), r * settled.step.t + t);
}

#[test]
fn a_mirrored_host_is_found_by_the_improper_fit() {
    let lib = library();
    let base = cluster(&workpiece(), G, 4.0);
    let settled = only(place(&base, &lib, "hdon_uniq", at(&base, G), TOL).expect("places"));

    let (r, t) = (odd_improper(), DVec3::new(2.5, -4.0, 6.25));
    let moved = transformed(&base, r, t);
    let found = only(place(&moved, &lib, "hdon_uniq", at(&moved, r * G + t), TOL).expect("places"));

    assert!(
        found.mirrored,
        "the host is the mirror image of the pattern's"
    );
    assert!(found.step.r.determinant() < 0.0);
    assert!(found.residual < 1e-9, "residual {}", found.residual);
    assert_rigid_eq(&found.step, &(r * settled.step.r), r * settled.step.t + t);
}

#[test]
fn a_chiral_op_drops_the_mirrored_fit() {
    let lib = library();
    let s = workpiece();
    let host = at(&s, D);

    let plain = place(&s, &lib, "land4", host, TOL).expect("places");
    assert_eq!(plain.len(), 2, "the two sides of the plane");
    assert!(!plain[0].mirrored, "the proper fit ranks first");
    assert!(plain[1].mirrored);

    let chiral = only(place(&s, &lib, "land4_chiral", host, TOL).expect("places"));
    assert!(!chiral.mirrored);
}

#[test]
fn the_wrong_environment_variant_is_rejected_at_the_tight_gate() {
    let lib = library();
    let s = workpiece();
    let host = at(&s, A);

    // The A site's back-bonds are 2.352 Å; `hdon_frame_wide` states 2.472. At
    // 0.05 Å nothing in that environment is congruent to the pattern.
    match expect_err(place(&s, &lib, "hdon_frame_wide", host, 0.05)) {
        MechanosynthError::NoPlacement { op, .. } => assert_eq!(op, "hdon_frame_wide"),
        other => panic!("expected NoPlacement, got {other}"),
    }

    // At the old 0.3 Å the same fit is accepted — and visibly inexact, which is
    // the whole point of reporting the residual.
    let loose = place(&s, &lib, "hdon_frame_wide", host, 0.3)
        .expect("places at 0.3")
        .remove(0);
    assert!(!loose.exact, "residual {}", loose.residual);
    assert!(
        loose.residual > 0.10 && loose.residual < 0.13,
        "residual {}",
        loose.residual
    );
    // The right variant is exact in the same environment, so ranking by
    // residual would always prefer it.
    let right = only(place(&s, &lib, "hdon_frame", host, 0.3).expect("places"));
    assert!(right.exact && right.residual < loose.residual);
}

#[test]
fn the_gate_is_the_max_per_atom_residual_not_the_rms() {
    let lib = library();
    let mut s = cluster(&workpiece(), G, 4.0);
    // Push one frame atom sideways: far enough to ruin the fit, close enough
    // that it stays in the neighbourhood and survives the pairwise prune.
    let strayed = s
        .atom_ids()
        .cloned()
        .find(|id| element(&s, *id) == O)
        .expect("the G cluster has one oxygen frame atom");
    let along = (position(&s, strayed) - G).normalize();
    let sideways = along.cross(DVec3::Z).normalize();
    let radius = position(&s, strayed).distance(G);
    s.set_atom_position(
        strayed,
        G + (along * radius + sideways * 0.12).normalize() * radius,
    );

    let host = at(&s, G);
    let candidate = only(place(&s, &lib, "hdon_uniq", host, 0.5).expect("places at a loose gate"));

    let operation = op(&lib, "hdon_uniq");
    let distances: Vec<f64> = candidate
        .roles
        .iter()
        .map(|(pattern_id, atom_id)| {
            let pattern = operation.before.atom(*pattern_id).expect("before atom");
            candidate
                .step
                .place(pattern.pos)
                .distance(position(&s, *atom_id))
        })
        .collect();
    let max = distances.iter().copied().fold(0.0, f64::max);
    let rms = (distances.iter().map(|d| d * d).sum::<f64>() / distances.len() as f64).sqrt();
    assert!(
        (candidate.residual - max).abs() < 1e-12,
        "residual is the max"
    );
    assert!(
        rms < max * 0.9,
        "the fixture must separate the two: {rms} vs {max}"
    );

    // A gate between the two accepts under an RMS rule and rejects under this
    // one. The rejection is what keeps a badly placed atom out.
    let between = 0.5 * (rms + max);
    match expect_err(place(&s, &lib, "hdon_uniq", host, between)) {
        MechanosynthError::NoPlacement { .. } => {}
        other => panic!("expected NoPlacement, got {other}"),
    }
}

#[test]
fn a_dimerisation_yields_one_candidate_per_available_partner() {
    let lib = library();
    let s = workpiece();

    let two = place(&s, &lib, "dimerize", at(&s, B), TOL).expect("places");
    assert_eq!(two.len(), 2);
    let operation = op(&lib, "dimerize");
    let mut landings: Vec<[i64; 3]> = two
        .iter()
        .map(|candidate| {
            let moved = candidate
                .step
                .place(operation.after.atom(2).expect("atom 2").pos);
            [
                (moved.x * 1e6).round() as i64,
                (moved.y * 1e6).round() as i64,
                (moved.z * 1e6).round() as i64,
            ]
        })
        .collect();
    landings.sort_unstable();
    landings.dedup();
    assert_eq!(landings.len(), 2, "two partners are two reactions");

    let one = place(
        &s,
        &lib,
        "dimerize",
        at(&s, B + DVec3::new(DIMER_D, 0.0, 0.0)),
        TOL,
    )
    .expect("places");
    assert_eq!(one.len(), 1, "that atom has only one partner");
}

#[test]
fn a_planar_op_with_a_planar_after_yields_one_candidate() {
    let lib = library();
    let s = workpiece();
    // The mirror fit maps the plane to itself and places the added atom in the
    // same spot, so it is the same reaction and is deduped away.
    let candidate = only(place(&s, &lib, "planar3", at(&s, CC), TOL).expect("places"));
    assert!(!candidate.mirrored);
    assert!(candidate.exact);
}

// ============================================================================
// Search bounds
// ============================================================================

/// Two silicons `separation` apart, the first at the origin — the smallest
/// workpiece `dimerize` can be asked about.
fn pair(separation: f64) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    s.add_atom(SI, DVec3::ZERO);
    s.add_atom(SI, DVec3::new(separation, 0.0, 0.0));
    s
}

#[test]
fn the_neighbourhood_reaches_exactly_extent_plus_tolerance() {
    let lib = library();
    let epsilon = 1e-4;

    let inside = pair(DIMER_D + TOL - epsilon);
    let clicked = at(&inside, DVec3::ZERO);
    assert_eq!(
        place(&inside, &lib, "dimerize", clicked, TOL)
            .expect("a partner just inside the reach is found")
            .len(),
        1
    );

    let outside = pair(DIMER_D + TOL + epsilon);
    let clicked = at(&outside, DVec3::ZERO);
    match expect_err(place(&outside, &lib, "dimerize", clicked, TOL)) {
        MechanosynthError::NoPlacement { .. } => {}
        other => panic!("expected NoPlacement, got {other}"),
    }
}

#[test]
fn no_two_roles_are_assigned_the_same_workpiece_atom() {
    let lib = library();
    let s = workpiece();
    for (operation, clicked, candidates) in sweep(&s, &lib, TOL) {
        for candidate in candidates {
            let mut ids: Vec<u32> = candidate.roles.iter().map(|(_, id)| *id).collect();
            let before = ids.len();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(
                ids.len(),
                before,
                "{} at atom {clicked} assigned an atom twice",
                operation.name
            );
        }
    }
}

#[test]
fn the_assignment_search_stays_pruned_on_a_crowded_neighbourhood() {
    let lib = library();
    // The A site, buried in 200 atoms scattered by a fixed generator so the run
    // is the same every time. `hdon_frame` is the worst case in the library:
    // four before atoms, three of them wildcards, so only the pairwise prune
    // stands between the search and N³ partial assignments.
    let mut s = AtomicStructure::new();
    s.add_atom(SI, DVec3::ZERO);
    for i in 0..3 {
        s.add_atom(SI, tetra(i) * BOND);
    }
    let mut seed: u64 = 0x2026_0914;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) as f64) / ((1u64 << 31) as f64) - 0.5
    };
    for _ in 0..200 {
        s.add_atom(SI, DVec3::new(next(), next(), next()) * 10.0);
    }

    let clicked = at(&s, DVec3::ZERO);
    let (candidates, stats) =
        place_with_stats(&s, &lib, "hdon_frame", clicked, TOL).expect("places");
    assert!(!candidates.is_empty());
    assert!(
        stats.assignments_visited < 200,
        "visited {} partial assignments in a neighbourhood of {}",
        stats.assignments_visited,
        stats.neighbourhood
    );
}

// ============================================================================
// The bond-derived fallback
// ============================================================================

#[test]
fn an_op_without_frame_atoms_is_oriented_from_the_hosts_bonds() {
    let lib = library();
    let s = workpiece();
    let host = at(&s, A);

    // Three back-bonds leave exactly one free tetrahedral direction.
    let candidate = only(place(&s, &lib, "hdon_bare", host, TOL).expect("places"));
    assert!(candidate.approximate, "the library did not state this");
    assert!(!candidate.mirrored);
    assert!(candidate.step.r.determinant() > 0.0, "a proper rotation");
    assert!(
        (candidate.step.r * DVec3::Z).abs_diff_eq(tetra(3), 1e-9),
        "local +z takes the free direction, got {:?}",
        candidate.step.r * DVec3::Z
    );
    assert!(candidate.step.t.abs_diff_eq(position(&s, host), EXACT));
}

#[test]
fn naming_frame_atoms_removes_the_approximation() {
    let lib = library();
    let s = workpiece();
    let candidate = only(place(&s, &lib, "hdon_frame", at(&s, A), TOL).expect("places"));
    assert!(!candidate.approximate);
    assert!(candidate.exact);
}

#[test]
fn a_one_atom_op_that_adds_nothing_off_the_origin_needs_no_orientation() {
    let lib = library();
    let s = workpiece();
    let candidate = only(place(&s, &lib, "recolor", at(&s, A), TOL).expect("places"));
    assert!(!candidate.approximate, "nothing to orient");
    assert_eq!(candidate.step.r, DMat3::IDENTITY);
    assert_eq!(candidate.residual, 0.0);
}

#[test]
fn a_saturated_host_has_no_direction_to_derive() {
    let lib = library();
    let mut s = AtomicStructure::new();
    let host = s.add_atom(SI, DVec3::ZERO);
    for i in 0..4 {
        let h = s.add_atom(H, tetra(i) * HB);
        s.add_bond(host, h, 1);
    }
    match expect_err(place(&s, &lib, "hdon_bare", host, TOL)) {
        MechanosynthError::NoPlacement { op, nearest, .. } => {
            assert_eq!(op, "hdon_bare");
            assert!(
                nearest.contains("no free bonding direction"),
                "message was: {nearest}"
            );
        }
        other => panic!("expected NoPlacement, got {other}"),
    }
}

// ============================================================================
// Dedupe and rank
// ============================================================================

#[test]
fn transforms_that_reach_the_same_after_state_collapse_to_the_proper_one() {
    let lib = library();
    let s = workpiece();
    // Three interchangeable `"*"` frame atoms admit six assignments — three
    // proper fits and three mirrored ones — and all six place the hydrogen in
    // the same spot, because every one of them fixes the fourth direction.
    let candidate = only(place(&s, &lib, "hdon_frame", at(&s, A), TOL).expect("places"));
    assert!(!candidate.mirrored, "the survivor is the proper fit");
}

/// A framed abstraction whose three `"*"` frame atoms sit at *different*
/// radii, so the six ways of assigning them to a symmetric host produce six
/// visibly different transforms — and yet one after state, because the step
/// only deletes the hydrogen and keeps everything else exactly where the
/// workpiece has it.
fn framed_abstraction_library() -> OpLibrary {
    let pos = |v: DVec3| format!("[{}, {}, {}]", v.x, v.y, v.z);
    let frames = [
        tetra(0) * (BOND - 0.02),
        tetra(1) * BOND,
        tetra(2) * (BOND + 0.02),
    ];
    let json = format!(
        r#"{{
          "format": "atomcad-msops/2",
          "ops": [
            {{
              "name": "habst_framed",
              "method": "spontaneous",
              "before": {{ "atoms": [
                {{ "id": 1, "el": "Si", "pos": [0.0, 0.0, 0.0] }},
                {{ "id": 2, "el": "*", "pos": {f0} }},
                {{ "id": 3, "el": "*", "pos": {f1} }},
                {{ "id": 4, "el": "*", "pos": {f2} }},
                {{ "id": 5, "el": "H", "pos": {h} }}
              ], "bonds": [] }},
              "after": {{ "atoms": [
                {{ "id": 1, "el": "Si", "pos": [0.0, 0.0, 0.0] }},
                {{ "id": 2, "el": "*", "pos": {f0} }},
                {{ "id": 3, "el": "*", "pos": {f1} }},
                {{ "id": 4, "el": "*", "pos": {f2} }}
              ], "bonds": [] }}
            }}
          ]
        }}"#,
        f0 = pos(frames[0]),
        f1 = pos(frames[1]),
        f2 = pos(frames[2]),
        h = pos(tetra(3) * HB),
    );
    parse_library(&json, "framed_abstraction.json").expect("the inline library should parse")
}

/// The host of [`framed_abstraction_library`]: an ideal tetrahedral Si with
/// three silicon neighbours and the hydrogen the operation takes.
fn framed_abstraction_host() -> (AtomicStructure, u32) {
    let mut s = AtomicStructure::new();
    let host = s.add_atom(SI, DVec3::ZERO);
    for i in 0..3 {
        let neighbour = s.add_atom(SI, tetra(i) * BOND);
        s.add_bond(host, neighbour, 1);
    }
    let h = s.add_atom(H, tetra(3) * HB);
    s.add_bond(host, h, 1);
    (s, host)
}

#[test]
fn a_kept_atom_counts_as_itself_rather_than_as_where_the_fit_puts_it() {
    let lib = framed_abstraction_library();
    let (s, host) = framed_abstraction_host();
    // Every assignment deletes the same hydrogen and leaves the four silicons
    // untouched, so there is one after state however the frame was fitted —
    // even though the fits differ by ~0.01 Å in where they *imagine* the kept
    // silicons are.
    let candidate = only(place(&s, &lib, "habst_framed", host, TOL).expect("places"));
    assert!(!candidate.mirrored, "the survivor is the proper fit");
    // The fixture is only a test of the keying if the fits it collapses are
    // far apart on the scale the collapse used to quantise at (1e-6 Å).
    assert!(
        candidate.residual > 1e-3,
        "the frame radii should make the fits genuinely differ, residual was {}",
        candidate.residual
    );
}

/// A bridge over three interchangeable `"*"` frame atoms: the step adds no
/// atom, moves none and deletes none, so every assignment leaves the same four
/// atoms exactly where they were and the candidates differ *only* in which
/// neighbour the new bond goes to.
fn framed_bridge_library() -> OpLibrary {
    let pos = |v: DVec3| format!("[{}, {}, {}]", v.x, v.y, v.z);
    let atoms = format!(
        r#"{{ "id": 1, "el": "Si", "pos": [0.0, 0.0, 0.0] }},
           {{ "id": 2, "el": "*", "pos": {} }},
           {{ "id": 3, "el": "*", "pos": {} }},
           {{ "id": 4, "el": "*", "pos": {} }}"#,
        pos(tetra(0) * DIMER_D),
        pos(tetra(1) * DIMER_D),
        pos(tetra(2) * DIMER_D),
    );
    let json = format!(
        r#"{{
          "format": "atomcad-msops/2",
          "ops": [
            {{
              "name": "bridge_framed",
              "method": "spontaneous",
              "before": {{ "atoms": [{atoms}], "bonds": [] }},
              "after": {{ "atoms": [{atoms}], "bonds": [[1, 2]] }}
            }}
          ]
        }}"#
    );
    parse_library(&json, "framed_bridge.json").expect("the inline library should parse")
}

/// The host of [`framed_bridge_library`]: a silicon with three unbonded
/// neighbours at exactly equal distances, so nothing but the bond can tell one
/// assignment from another.
fn framed_bridge_host() -> (AtomicStructure, u32, Vec<u32>) {
    let mut s = AtomicStructure::new();
    let host = s.add_atom(SI, DVec3::ZERO);
    let neighbours = (0..3).map(|i| s.add_atom(SI, tetra(i) * DIMER_D)).collect();
    (s, host, neighbours)
}

#[test]
fn bonding_a_different_frame_atom_is_a_different_candidate() {
    let lib = framed_bridge_library();
    let (s, host, neighbours) = framed_bridge_host();
    let candidates = place(&s, &lib, "bridge_framed", host, TOL).expect("places");
    // Three neighbours, three reactions. The six assignments and their mirrors
    // collapse in threes — permuting the two frame atoms the step does not
    // bond changes nothing — but never across the bond.
    assert_eq!(
        candidates.len(),
        3,
        "expected one candidate per neighbour the bond could go to"
    );
    let bonded: Vec<u32> = candidates
        .iter()
        .map(|c| {
            c.roles
                .iter()
                .find(|(pattern_id, _)| *pattern_id == 2)
                .expect("the bonded frame atom is assigned")
                .1
        })
        .collect();
    for neighbour in &neighbours {
        assert!(
            bonded.contains(neighbour),
            "no candidate bonds atom {neighbour}; they bond {bonded:?}"
        );
    }
    for candidate in &candidates {
        assert!(!candidate.mirrored, "each survivor is a proper fit");
        assert_eq!(preview_atoms(&s, op(&lib, "bridge_framed"), candidate), []);
        assert_eq!(
            preview_bonds(&s, op(&lib, "bridge_framed"), candidate).len(),
            1,
            "a bridge previews as exactly one added bond"
        );
    }
}

#[test]
fn candidates_are_ranked_and_the_ranking_is_stable() {
    let lib = library();
    let s = workpiece();
    for (operation, clicked, candidates) in sweep(&s, &lib, TOL) {
        for pair in candidates.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let key = |c: &Candidate| {
                (
                    (c.residual / RESIDUAL_RANK_EPSILON).round() as i64,
                    c.mirrored,
                    c.approximate,
                )
            };
            assert!(
                key(a) <= key(b),
                "{} at atom {clicked} is out of order: {:?} then {:?}",
                operation.name,
                key(a),
                key(b)
            );
        }
        let again = place(&s, &lib, &operation.name, clicked, TOL).expect("places again");
        assert_eq!(
            candidates, again,
            "{} at atom {clicked} is not stable across calls",
            operation.name
        );
    }
}

// ============================================================================
// Diagnostics
// ============================================================================

#[test]
fn an_operation_the_library_does_not_have_is_named() {
    let lib = library();
    let s = workpiece();
    match expect_err(place(&s, &lib, "no_such_op", at(&s, A), TOL)) {
        MechanosynthError::UnknownOp { op, library } => {
            assert_eq!(op, "no_such_op");
            assert!(library.contains("place_ops.json"));
        }
        other => panic!("expected UnknownOp, got {other}"),
    }
}

#[test]
fn an_inadmissible_element_lists_what_the_op_does_accept() {
    let lib = library();
    let s = workpiece();
    // `planar3` acts on C, O and N; the A site is silicon.
    match expect_err(place(&s, &lib, "planar3", at(&s, A), TOL)) {
        MechanosynthError::NoRole {
            op,
            element,
            accepted,
        } => {
            assert_eq!(op, "planar3");
            assert_eq!(element, "Si");
            assert_eq!(accepted, "C, O, N");
        }
        other => panic!("expected NoRole, got {other}"),
    }
}

#[test]
fn a_missing_partner_is_named_with_its_distance() {
    let lib = library();
    let s = workpiece();
    // The D site is silicon with no silicon anywhere near it.
    match expect_err(place(&s, &lib, "dimerize", at(&s, D), TOL)) {
        MechanosynthError::NoPlacement {
            op,
            role,
            element,
            nearest,
        } => {
            assert_eq!(op, "dimerize");
            assert_eq!(role, 1);
            assert_eq!(element, "Si");
            assert!(nearest.contains("Si"), "message was: {nearest}");
            assert!(nearest.contains(" Å"), "message was: {nearest}");
        }
        other => panic!("expected NoPlacement, got {other}"),
    }
}

#[test]
fn a_stale_atom_id_is_an_error_rather_than_a_panic() {
    let lib = library();
    let s = workpiece();
    match expect_err(place(&s, &lib, "habst", 9999, TOL)) {
        MechanosynthError::NoSuchAtom { atom_id } => assert_eq!(atom_id, 9999),
        other => panic!("expected NoSuchAtom, got {other}"),
    }
}

// ============================================================================
// Every candidate replays
// ============================================================================

/// Every (operation, clickable atom) pair of the fixture that places at all.
fn sweep<'a>(
    s: &AtomicStructure,
    lib: &'a OpLibrary,
    tolerance: f64,
) -> Vec<(&'a Operation, u32, Vec<Candidate>)> {
    let mut out = Vec::new();
    for operation in &lib.ops {
        for atom_id in s.atom_ids().cloned().collect::<Vec<u32>>() {
            if let Ok(candidates) = place(s, lib, &operation.name, atom_id, tolerance) {
                assert!(
                    !candidates.is_empty(),
                    "{} at atom {atom_id}: Ok must never be empty",
                    operation.name
                );
                out.push((operation, atom_id, candidates));
            }
        }
    }
    assert!(
        out.len() > 20,
        "the sweep should exercise the whole library"
    );
    out
}

#[test]
fn every_candidate_the_engine_offers_replays() {
    let lib = library();
    let s = workpiece();
    let mut seen: Vec<&str> = Vec::new();
    for (operation, _clicked, candidates) in sweep(&s, &lib, TOL) {
        if !seen.contains(&operation.name.as_str()) {
            seen.push(&operation.name);
        }
        for candidate in &candidates {
            assert_candidate_replays(&s, operation, candidate, TOL);
        }
    }
    // Every operation but the deliberately-unfittable variant finds a host.
    for operation in &lib.ops {
        if operation.name == "hdon_frame_wide" {
            continue;
        }
        assert!(
            seen.contains(&operation.name.as_str()),
            "{} never placed anywhere in the fixture",
            operation.name
        );
    }
}

// ============================================================================
// Round-trip exactness
// ============================================================================

#[test]
fn re_authoring_a_generated_build_by_clicking_reproduces_it_exactly() {
    let lib = library();
    let base = workpiece();
    let gold_build = gold_script();
    let tolerance = resolve_tolerance(&lib);

    let gold = replay(&base, &lib, &gold_build, -1, HighlightTags::default())
        .expect("the gold build replays");

    // Re-author it: for every step, click the atom the operation acts on and
    // take the candidate whose rotation the generator wrote.
    let mut authored = base.clone();
    for (index, gold_step) in gold_build.steps.iter().enumerate() {
        let operation = op(&lib, &gold_step.op);
        let role = operation
            .before
            .atoms
            .iter()
            .find(|atom| atom.pos.length() <= 1e-6)
            .unwrap_or(&operation.before.atoms[0]);
        let clicked = authored
            .nearest_unclaimed_atom(
                gold_step.place(role.pos),
                tolerance,
                role.element.required_atomic_number(),
                |_| false,
            )
            .unwrap_or_else(|| panic!("step {} has no atom to click", index + 1))
            .atom_id;

        let candidates = place(&authored, &lib, &gold_step.op, clicked, tolerance)
            .unwrap_or_else(|e| panic!("step {} should place: {e}", index + 1));
        let candidate = candidates
            .iter()
            .find(|candidate| {
                (0..3).all(|i| {
                    candidate
                        .step
                        .r
                        .col(i)
                        .abs_diff_eq(gold_step.r.col(i), 1e-9)
                })
            })
            .unwrap_or_else(|| {
                panic!(
                    "step {} ({}): no candidate matches the generator's rotation",
                    index + 1,
                    gold_step.op
                )
            });

        assert!(
            candidate.residual < EXACT_FIT_RESIDUAL,
            "step {} ({}) fits only to {} Å",
            index + 1,
            gold_step.op,
            candidate.residual
        );
        assert!(!candidate.approximate, "step {} is approximate", index + 1);
        assert!(
            candidate.step.t.abs_diff_eq(gold_step.t, 1e-9),
            "step {} translation: {:?} vs {:?}",
            index + 1,
            candidate.step.t,
            gold_step.t
        );
        apply_step(
            &mut authored,
            operation,
            &candidate.step,
            index + 1,
            tolerance,
        )
        .unwrap_or_else(|e| panic!("step {} should replay: {e}", index + 1));
    }

    assert_same(&authored, &gold);
}

#[test]
fn the_fixture_library_warns_about_its_one_unconventional_operation() {
    let lib = library();
    assert_eq!(lib.warnings.len(), 1, "warnings: {:?}", lib.warnings);
    assert!(
        lib.warnings[0].contains("off_origin_all"),
        "warning was: {}",
        lib.warnings[0]
    );
}

#[test]
fn the_fixture_workpiece_holds_the_environments_the_ops_need() {
    let s = workpiece();
    assert_eq!(element(&s, at(&s, A)), SI);
    assert_eq!(element(&s, at(&s, CC)), C);
    assert_eq!(element(&s, at(&s, CC + DVec3::new(1.4, 0.0, 0.0))), O);
    assert_eq!(element(&s, at(&s, CC + DVec3::new(0.0, 1.4, 0.0))), N);
    assert_eq!(element(&s, at(&s, E + tetra(0) * HB)), H);
    assert_eq!(element(&s, at(&s, F)), C);
    assert_eq!(element(&s, at(&s, F + tetra(0) * 1.09)), H);
    assert_eq!(
        s.get_atom(at(&s, A)).expect("host").bonds.len(),
        3,
        "the donation host keeps exactly one free direction"
    );
}

// ============================================================================
// Applicability: what can be done here (P3)
// ============================================================================

/// The sub-cluster-A host: the three-coordinate Si at the origin, which the
/// donation ops were written for.
fn host_a(workpiece: &AtomicStructure) -> u32 {
    at(workpiece, A)
}

fn row<'a>(rows: &'a [Applicability], op: &str) -> &'a Applicability {
    rows.iter()
        .find(|row| row.op == op)
        .unwrap_or_else(|| panic!("{op} should be in the offer list: {:?}", names(rows)))
}

fn names(rows: &[Applicability]) -> Vec<&str> {
    rows.iter().map(|row| row.op.as_str()).collect()
}

#[test]
fn a_click_lists_the_operations_that_fit_and_the_ones_that_nearly_do() {
    let lib = library();
    let workpiece = workpiece();
    let rows = applicable_ops(&workpiece, &lib, host_a(&workpiece), TOL, None);

    // The donation the cluster was built for fits exactly.
    let fitting = row(&rows, "hdon_frame");
    assert!(fitting.fits);
    assert_eq!(fitting.candidates.len(), 1);
    assert!(fitting.near_miss.is_none());
    assert!(fitting.best_residual < EXACT_FIT_RESIDUAL);

    // Its second environment variant does not, and says how far off it is
    // rather than vanishing — the coverage report the design asks for.
    let miss = row(&rows, "hdon_frame_wide");
    assert!(!miss.fits);
    assert!(miss.candidates.is_empty());
    let near_miss = miss.near_miss.as_ref().expect("a near miss keeps its fit");
    assert!(near_miss.residual > TOL, "{}", near_miss.residual);
    assert!(near_miss.residual <= TOL * NEAR_MISS_FACTOR);
    assert_eq!(miss.best_residual, near_miss.residual);

    // An operation with nothing to say about this atom is absent entirely: a
    // wrong element (`habst` wants H), or no partner within reach (`dimerize`).
    for absent in ["habst", "planar3", "dimerize", "land4"] {
        assert!(
            !rows.iter().any(|row| row.op == absent),
            "{absent} should not be listed: {:?}",
            names(&rows)
        );
    }
}

#[test]
fn no_two_variants_of_one_family_fit_the_same_atom_at_the_tight_gate() {
    let lib = library();
    let workpiece = workpiece();
    let rows = applicable_ops(&workpiece, &lib, host_a(&workpiece), TOL, None);
    let fitting: Vec<&str> = rows
        .iter()
        .filter(|row| row.fits)
        .map(|row| row.op.as_str())
        .collect();
    assert!(
        fitting.contains(&"hdon_frame") && !fitting.contains(&"hdon_frame_wide"),
        "the gate must resolve the variant: {fitting:?}"
    );
}

#[test]
fn every_row_is_applicable_or_a_near_miss_and_never_both() {
    let lib = library();
    let workpiece = workpiece();
    for atom_id in workpiece.atom_ids().cloned().collect::<Vec<u32>>() {
        for row in applicable_ops(&workpiece, &lib, atom_id, TOL, None) {
            assert_eq!(
                row.fits,
                !row.candidates.is_empty(),
                "{}: `fits` must mean `candidates` is non-empty",
                row.op
            );
            assert_eq!(
                row.fits,
                row.near_miss.is_none(),
                "{}: a row is one or the other",
                row.op
            );
            assert_eq!(row.best_residual, row.preview().residual, "{}", row.op);
            if row.fits {
                assert!(row.best_residual <= TOL, "{}", row.op);
            } else {
                assert!(row.best_residual > TOL, "{}", row.op);
            }
        }
    }
}

#[test]
fn a_rows_candidates_are_exactly_what_place_returns_for_that_operation() {
    let lib = library();
    let workpiece = workpiece();
    for atom_id in workpiece.atom_ids().cloned().collect::<Vec<u32>>() {
        for row in applicable_ops(&workpiece, &lib, atom_id, TOL, None) {
            if !row.fits {
                continue;
            }
            let direct = place(&workpiece, &lib, &row.op, atom_id, TOL)
                .unwrap_or_else(|e| panic!("{} fits, so place must succeed: {e}", row.op));
            assert_eq!(
                direct, row.candidates,
                "{}: the sweep and the single call must agree",
                row.op
            );
        }
    }
}

#[test]
fn an_atom_no_operation_accepts_returns_an_empty_list_rather_than_an_error() {
    let lib = library();
    let mut workpiece = AtomicStructure::new();
    // Argon: no operation in the fixture library names it, and it bonds to
    // nothing, so even the bond-derived fallback has nothing to offer.
    workpiece.add_atom(18, DVec3::ZERO);
    let atom_id = *workpiece.atom_ids().next().expect("one atom");
    assert!(applicable_ops(&workpiece, &lib, atom_id, TOL, None).is_empty());
}

#[test]
fn the_near_miss_gate_is_ten_times_the_tolerance() {
    let lib = library();
    let workpiece = workpiece();
    let host = host_a(&workpiece);
    // `hdon_frame_wide`'s residual on this host is the one number both sides of
    // the gate are measured against.
    let rows = applicable_ops(&workpiece, &lib, host, TOL, None);
    let residual = row(&rows, "hdon_frame_wide").best_residual;

    // Just inside: the tolerance that puts the miss at exactly the factor.
    let inside = residual / NEAR_MISS_FACTOR * 1.001;
    assert!(
        applicable_ops(&workpiece, &lib, host, inside, None)
            .iter()
            .any(|row| row.op == "hdon_frame_wide"),
        "a fit at just under 10x the tolerance is still reported"
    );
    let outside = residual / NEAR_MISS_FACTOR * 0.999;
    assert!(
        !applicable_ops(&workpiece, &lib, host, outside, None)
            .iter()
            .any(|row| row.op == "hdon_frame_wide"),
        "a fit at just over 10x the tolerance is not"
    );
}

#[test]
fn the_offer_list_sorts_applicable_first_then_by_residual_and_is_stable() {
    let lib = library();
    let workpiece = workpiece();
    let host = host_a(&workpiece);
    let rows = applicable_ops(&workpiece, &lib, host, TOL, None);

    if let Some(first_miss) = rows.iter().position(|row| !row.fits) {
        assert!(
            rows[first_miss..].iter().all(|row| !row.fits),
            "every applicable row comes before every near miss: {:?}",
            names(&rows)
        );
    }
    for pair in rows.windows(2) {
        if pair[0].fits == pair[1].fits {
            assert!(
                pair[0].best_residual <= pair[1].best_residual + RESIDUAL_RANK_EPSILON,
                "residual order broken: {:?}",
                names(&rows)
            );
        }
    }
    assert_eq!(rows, applicable_ops(&workpiece, &lib, host, TOL, None));
}

#[test]
fn admitting_everything_is_the_unfiltered_sweep_and_admitting_nothing_is_empty() {
    // `applicable_ops_where` is the seam the editor's mute set hangs off
    // (`doc/design_mechanosynth_op_muting.md`). The engine knows only the
    // predicate, so the two boundary policies are what pin its meaning.
    let lib = library();
    let workpiece = workpiece();
    let host = host_a(&workpiece);

    assert_eq!(
        applicable_ops_where(&workpiece, &lib, host, TOL, None, &|_| true),
        applicable_ops(&workpiece, &lib, host, TOL, None),
        "admitting every operation is the plain sweep, row for row"
    );
    assert!(
        applicable_ops_where(&workpiece, &lib, host, TOL, None, &|_| false).is_empty(),
        "and a rejected operation is indistinguishable from one the library never had"
    );
}

#[test]
fn a_rejected_operation_leaves_the_other_rows_exactly_as_they_were() {
    // Filtering must not perturb the ranking of what survives: the offer list
    // a user reads with four operations muted has to be the same list minus
    // four rows, not a differently ordered one.
    let lib = library();
    let workpiece = workpiece();
    let host = host_a(&workpiece);
    let all = applicable_ops(&workpiece, &lib, host, TOL, None);
    let dropped = all[0].op.clone();

    let filtered =
        applicable_ops_where(&workpiece, &lib, host, TOL, None, &|op| op.name != dropped);
    let expected: Vec<_> = all.into_iter().filter(|row| row.op != dropped).collect();
    assert_eq!(filtered, expected);
}

#[test]
fn an_operation_oriented_from_bonds_is_offered_and_flagged_approximate() {
    let lib = library();
    let workpiece = workpiece();
    let rows = applicable_ops(&workpiece, &lib, host_a(&workpiece), TOL, None);
    let bare = row(&rows, "hdon_bare");
    assert!(bare.fits, "the fallback still produces a placement");
    assert!(
        bare.approximate,
        "and it must say the coordinates came from the application"
    );
}

// ============================================================================
// Ghost previews
// ============================================================================

#[test]
fn a_donation_previews_exactly_the_atom_it_would_add() {
    let lib = library();
    let workpiece = workpiece();
    let operation = op(&lib, "hdon_frame");
    let candidate = only(
        place(&workpiece, &lib, "hdon_frame", host_a(&workpiece), TOL).expect("hdon_frame fits"),
    );
    let ghosts = preview_atoms(&workpiece, operation, &candidate);
    assert_eq!(
        ghosts.len(),
        1,
        "only the added H is worth drawing: {ghosts:?}"
    );
    assert_eq!(ghosts[0].kind, GhostKind::Added);
    assert_eq!(ghosts[0].atomic_number, H);
    assert!(
        ghosts[0].position.abs_diff_eq(
            candidate
                .step
                .place(DVec3::new(-0.854478, -0.854478, 0.854478)),
            EXACT
        )
    );
}

#[test]
fn an_abstraction_previews_the_atom_it_would_remove() {
    let lib = library();
    let workpiece = workpiece();
    let terminator = at(&workpiece, E + tetra(0) * HB);
    let operation = op(&lib, "habst");
    let candidate = only(place(&workpiece, &lib, "habst", terminator, TOL).expect("habst fits"));
    let ghosts = preview_atoms(&workpiece, operation, &candidate);
    assert_eq!(ghosts.len(), 1);
    assert_eq!(ghosts[0].kind, GhostKind::Deleted);
    assert_eq!(ghosts[0].atomic_number, H);
    assert!(
        ghosts[0]
            .position
            .abs_diff_eq(position(&workpiece, terminator), EXACT)
    );
}

#[test]
fn a_dimerisation_previews_both_partners_moving() {
    let lib = library();
    let workpiece = workpiece();
    let operation = op(&lib, "dimerize");
    let candidate = place(&workpiece, &lib, "dimerize", at(&workpiece, B), TOL)
        .expect("dimerize fits")
        .into_iter()
        .next()
        .expect("at least one candidate");
    let ghosts = preview_atoms(&workpiece, operation, &candidate);
    assert_eq!(ghosts.len(), 2, "{ghosts:?}");
    assert!(ghosts.iter().all(|g| g.kind == GhostKind::Moved));
    for g in &ghosts {
        assert!(
            g.from.distance(g.position) > 0.4,
            "a moved ghost carries where it came from: {g:?}"
        );
    }
}

#[test]
fn an_element_swap_previews_the_atom_it_would_change() {
    let lib = library();
    let workpiece = workpiece();
    let host = host_a(&workpiece);
    let operation = op(&lib, "recolor");
    let candidate = only(place(&workpiece, &lib, "recolor", host, TOL).expect("recolor fits"));
    let ghosts = preview_atoms(&workpiece, operation, &candidate);
    // Without a `Changed` ghost this operation would preview as nothing at all,
    // which reads as "this would do nothing".
    assert_eq!(ghosts.len(), 1, "{ghosts:?}");
    assert_eq!(ghosts[0].kind, GhostKind::Changed);
    assert_eq!(ghosts[0].atomic_number, 32, "germanium");
    assert!(
        ghosts[0]
            .position
            .abs_diff_eq(position(&workpiece, host), EXACT)
    );
}

#[test]
fn a_bond_only_operation_previews_the_bond_it_would_add() {
    // The case the atoms-only preview missed entirely: `bridge` has identical
    // `before` and `after` atom lists, so it draws no ghost *atom* and used to
    // preview as an empty scene — indistinguishable from a broken tool.
    let lib = library();
    let workpiece = workpiece();
    let host = at(&workpiece, B);
    let operation = op(&lib, "bridge");
    // Two partners are in reach, exactly as for `dimerize`, whose hosts it
    // shares; either candidate previews the same way.
    let candidate = place(&workpiece, &lib, "bridge", host, TOL)
        .expect("bridge fits")
        .into_iter()
        .next()
        .expect("at least one candidate");

    assert!(
        preview_atoms(&workpiece, operation, &candidate).is_empty(),
        "nothing moves, so there is no atom to draw"
    );

    let bonds = preview_bonds(&workpiece, operation, &candidate);
    assert_eq!(bonds.len(), 1, "{bonds:?}");
    assert_eq!(bonds[0].kind, GhostBondKind::Added);
    // Drawn between the two atoms as they are: a kept atom is never snapped.
    let span = bonds[0].from.distance(bonds[0].to);
    assert!((span - DIMER_D).abs() < TOL, "{span}");
    assert!(
        bonds[0].from.abs_diff_eq(position(&workpiece, host), EXACT)
            || bonds[0].to.abs_diff_eq(position(&workpiece, host), EXACT),
        "one end is the clicked atom"
    );
}

#[test]
fn a_bond_deletion_previews_against_the_current_geometry() {
    let lib = library();
    let workpiece = workpiece();
    // Sub-cluster F is the one `habst_pair` is hosted on; its C-H is 1.09 A.
    let terminator = at(&workpiece, F + tetra(0) * 1.09);
    let operation = op(&lib, "habst_pair");
    let candidate =
        only(place(&workpiece, &lib, "habst_pair", terminator, TOL).expect("habst_pair fits"));

    let bonds = preview_bonds(&workpiece, operation, &candidate);
    assert_eq!(bonds.len(), 1, "{bonds:?}");
    assert_eq!(bonds[0].kind, GhostBondKind::Deleted);
    assert!(
        bonds[0]
            .from
            .abs_diff_eq(position(&workpiece, terminator), EXACT)
            || bonds[0]
                .to
                .abs_diff_eq(position(&workpiece, terminator), EXACT)
    );
}

#[test]
fn an_added_bond_reaching_a_moved_atom_is_drawn_where_the_atom_ends_up() {
    // `dimerize` moves both partners *and* bonds them. The stick must span the
    // after positions, not the ones the atoms are leaving.
    let lib = library();
    let workpiece = workpiece();
    let operation = op(&lib, "dimerize");
    let candidate = place(&workpiece, &lib, "dimerize", at(&workpiece, B), TOL)
        .expect("dimerize fits")
        .into_iter()
        .next()
        .expect("at least one candidate");

    let bonds = preview_bonds(&workpiece, operation, &candidate);
    assert_eq!(bonds.len(), 1, "{bonds:?}");
    assert_eq!(bonds[0].kind, GhostBondKind::Added);

    let ghosts = preview_atoms(&workpiece, operation, &candidate);
    for end in [bonds[0].from, bonds[0].to] {
        assert!(
            ghosts.iter().any(|g| g.position.abs_diff_eq(end, EXACT)),
            "each end sits on a moved ghost's destination: {end:?} vs {ghosts:?}"
        );
        assert!(
            !ghosts.iter().any(|g| g.from.abs_diff_eq(end, EXACT)),
            "and not on where it came from"
        );
    }
}

#[test]
fn every_offer_row_previews_itself() {
    let lib = library();
    let workpiece = workpiece();
    for atom_id in workpiece.atom_ids().cloned().collect::<Vec<u32>>() {
        for row in applicable_ops(&workpiece, &lib, atom_id, TOL, None) {
            let operation = op(&lib, &row.op);
            // Atoms **or** bonds: an operation whose whole effect is a bond has
            // nothing in the atom list, and `bridge` is in the fixture to keep
            // this assertion honest about that.
            let atoms = preview_atoms(&workpiece, operation, row.preview());
            let bonds = preview_bonds(&workpiece, operation, row.preview());
            assert!(
                !atoms.is_empty() || !bonds.is_empty(),
                "{} has nothing to draw on atom {atom_id}, so selecting its row \
                 would look like a no-op",
                row.op
            );
        }
    }
}
