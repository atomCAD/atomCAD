//! `structure_compare`: comparing two clusters up to a tolerance and a motion
//! (`doc/design_structure_comparison.md` §3, Phase 1 tests in §6).
//!
//! Three layers: a **brute-force oracle** that is D2 written out literally
//! (every same-element bijection, its least-squares fit, the max deviation),
//! against which small random pairs are checked; **known answers** on larger
//! clusters, where the expected verdict comes from the construction; and the
//! pieces underneath (the matched comparison, the grouping).
//!
//! Every randomized test asserts its own coverage: a property over pairs that
//! are almost never equal would pass with every check deleted.

use crate::test_rng::Rng;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::rigid_fit::rigid_fit;
use atomcad_crystolecule::structure_compare::invariants;
use atomcad_crystolecule::structure_compare::motion::{PairContext, Proposal, Verification};
use atomcad_crystolecule::structure_compare::{
    Cluster, CompareError, CompareSettings, Comparison, Grouping, Mismatch, Motion, PreCheck,
    Proposals, compare, compare_structures, compare_structures_matched, group_in_order,
};
use glam::{DMat3, DQuat, DVec3};
use std::time::Instant;

const H: i16 = 1;
const C: i16 = 6;
const N: i16 = 7;
const O: i16 = 8;
const F: i16 = 9;
const SI: i16 = 14;
const CL: i16 = 17;
const BR: i16 = 35;

const MOTIONS: [Motion; 4] = [
    Motion::None,
    Motion::Translation,
    Motion::Rigid,
    Motion::RigidMirror,
];

// ============================================================================
// Raw clusters and their transformations
// ============================================================================

#[derive(Debug, Clone)]
struct Raw {
    positions: Vec<DVec3>,
    elements: Vec<i16>,
    bonds: Vec<(usize, usize, u8)>,
}

impl Raw {
    fn cluster(&self) -> Cluster {
        Cluster::from_parts(self.positions.clone(), self.elements.clone(), &self.bonds).unwrap()
    }

    fn len(&self) -> usize {
        self.positions.len()
    }

    fn bond_order(&self, i: usize, j: usize) -> Option<u8> {
        self.bonds
            .iter()
            .find(|&&(x, y, _)| (x, y) == (i, j) || (x, y) == (j, i))
            .map(|b| b.2)
    }

    fn moved(&self, r: DMat3, t: DVec3) -> Raw {
        Raw {
            positions: self.positions.iter().map(|p| r * *p + t).collect(),
            ..self.clone()
        }
    }

    /// Atom `k` of the result is atom `order[k]` of `self`.
    fn permuted(&self, order: &[usize]) -> Raw {
        let mut new_index = vec![0; order.len()];
        for (k, &old) in order.iter().enumerate() {
            new_index[old] = k;
        }
        Raw {
            positions: order.iter().map(|&i| self.positions[i]).collect(),
            elements: order.iter().map(|&i| self.elements[i]).collect(),
            bonds: self
                .bonds
                .iter()
                .map(|&(i, j, o)| (new_index[i], new_index[j], o))
                .collect(),
        }
    }

    fn jittered(&self, rng: &mut Rng, max: f64) -> Raw {
        Raw {
            positions: self
                .positions
                .iter()
                .map(|p| *p + rng.unit() * rng.range(0.0, max))
                .collect(),
            ..self.clone()
        }
    }

    fn structure(&self) -> AtomicStructure {
        let mut s = AtomicStructure::new();
        for (p, &e) in self.positions.iter().zip(&self.elements) {
            s.add_atom(e, *p);
        }
        for &(i, j, o) in &self.bonds {
            s.add_bond(i as u32 + 1, j as u32 + 1, o);
        }
        s
    }
}

fn shuffle(rng: &mut Rng, n: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        order.swap(i, rng.below(i + 1));
    }
    order
}

fn reflection() -> DMat3 {
    DMat3::from_diagonal(DVec3::new(-1.0, 1.0, 1.0))
}

fn random_translation(rng: &mut Rng) -> DVec3 {
    DVec3::new(
        rng.range(-5.0, 5.0),
        rng.range(-5.0, 5.0),
        rng.range(-5.0, 5.0),
    )
}

/// A random motion of the given kind.
fn random_motion(rng: &mut Rng, kind: Motion) -> (DMat3, DVec3) {
    match kind {
        Motion::None => (DMat3::IDENTITY, DVec3::ZERO),
        Motion::Translation => (DMat3::IDENTITY, random_translation(rng)),
        Motion::Rigid => (DMat3::from_quat(rng.rotation()), random_translation(rng)),
        Motion::RigidMirror => (
            DMat3::from_quat(rng.rotation()) * reflection(),
            random_translation(rng),
        ),
    }
}

/// A copy of `a` under `(r, t)`, jittered by up to `jitter`, shuffled. Returns
/// the copy and, for each of its atoms, the atom of `a` it came from.
fn copy_of(rng: &mut Rng, a: &Raw, r: DMat3, t: DVec3, jitter: f64) -> (Raw, Vec<usize>) {
    let order = shuffle(rng, a.len());
    let b = a.moved(r, t).jittered(rng, jitter).permuted(&order);
    (b, order)
}

/// A small random cluster: `n` atoms from `palette`, every same-element
/// distance at least `min_same`, optionally with random bonds.
/// Positions are drawn in a cube of half-width `half`.
fn random_small(
    rng: &mut Rng,
    n: usize,
    palette: &[i16],
    min_same: f64,
    half: f64,
    bonded: bool,
) -> Raw {
    let elements: Vec<i16> = (0..n).map(|_| palette[rng.below(palette.len())]).collect();
    random_positions_for(rng, elements, min_same, half, bonded)
}

fn random_positions_for(
    rng: &mut Rng,
    elements: Vec<i16>,
    min_same: f64,
    half: f64,
    bonded: bool,
) -> Raw {
    'retry: loop {
        let mut positions: Vec<DVec3> = Vec::new();
        for &e in &elements {
            let mut placed = false;
            for _ in 0..1000 {
                let p = DVec3::new(
                    rng.range(-half, half),
                    rng.range(-half, half),
                    rng.range(-half, half),
                );
                if positions
                    .iter()
                    .zip(&elements)
                    .all(|(q, &f)| f != e || q.distance(p) >= min_same)
                {
                    positions.push(p);
                    placed = true;
                    break;
                }
            }
            if !placed {
                continue 'retry;
            }
        }
        let mut bonds = Vec::new();
        if bonded {
            for i in 0..elements.len() {
                for j in (i + 1)..elements.len() {
                    if rng.f64() < 0.3 {
                        bonds.push((i, j, if rng.f64() < 0.7 { 1 } else { 2 }));
                    }
                }
            }
        }
        return Raw {
            positions,
            elements,
            bonds,
        };
    }
}

/// A larger random cluster: atoms at least 1.05 Å apart in a ball, C / H / Si,
/// bonded below 1.7 Å.
fn random_large(rng: &mut Rng, n: usize) -> Raw {
    let radius = (0.55 * n as f64).cbrt() * 1.1;
    let mut positions: Vec<DVec3> = Vec::new();
    while positions.len() < n {
        let p = rng.unit() * radius * rng.f64().cbrt();
        if positions.iter().all(|q| q.distance(p) >= 1.05) {
            positions.push(p);
        }
    }
    let elements = (0..n)
        .map(|_| match rng.below(10) {
            0..=4 => C,
            5..=7 => H,
            _ => SI,
        })
        .collect();
    let mut bonds = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if positions[i].distance(positions[j]) < 1.7 {
                bonds.push((i, j, 1));
            }
        }
    }
    Raw {
        positions,
        elements,
        bonds,
    }
}

// ============================================================================
// The oracle: D2, literally
// ============================================================================

/// The max deviation the least-squares fit of the allowed kind leaves on the
/// matched points (`b_points[k]` is matched with `a_points[k]`), the better
/// handedness under `rigid_mirror`.
fn fit_deviation(a_points: &[DVec3], b_points: &[DVec3], kind: Motion) -> f64 {
    let n = a_points.len() as f64;
    match kind {
        Motion::None => a_points
            .iter()
            .zip(b_points)
            .map(|(a, b)| a.distance(*b))
            .fold(0.0, f64::max),
        Motion::Translation => {
            let t = a_points.iter().copied().sum::<DVec3>() / n
                - b_points.iter().copied().sum::<DVec3>() / n;
            a_points
                .iter()
                .zip(b_points)
                .map(|(a, b)| a.distance(*b + t))
                .fold(0.0, f64::max)
        }
        Motion::Rigid => rigid_fit(b_points, a_points, false).unwrap().residual,
        Motion::RigidMirror => rigid_fit(b_points, a_points, false)
            .unwrap()
            .residual
            .min(rigid_fit(b_points, a_points, true).unwrap().residual),
    }
}

/// Equal iff some same-element bijection (agreeing on every bond, when bonds
/// are compared) has its least-squares fit within `tolerance`.
fn brute_force_equal(a: &Raw, b: &Raw, kind: Motion, tolerance: f64, bonds: bool) -> bool {
    if a.len() != b.len() {
        return false;
    }
    fn extend(
        a: &Raw,
        b: &Raw,
        kind: Motion,
        tolerance: f64,
        bonds: bool,
        assigned: &mut Vec<usize>,
        used: &mut Vec<bool>,
    ) -> bool {
        let i = assigned.len();
        if i == a.len() {
            let a_points = &a.positions;
            let b_points: Vec<DVec3> = assigned.iter().map(|&j| b.positions[j]).collect();
            return fit_deviation(a_points, &b_points, kind) <= tolerance;
        }
        for j in 0..b.len() {
            if used[j] || b.elements[j] != a.elements[i] {
                continue;
            }
            if bonds && (0..i).any(|k| a.bond_order(k, i) != b.bond_order(assigned[k], j)) {
                continue;
            }
            used[j] = true;
            assigned.push(j);
            let found = extend(a, b, kind, tolerance, bonds, assigned, used);
            assigned.pop();
            used[j] = false;
            if found {
                return true;
            }
        }
        false
    }
    extend(
        a,
        b,
        kind,
        tolerance,
        bonds,
        &mut Vec::new(),
        &mut vec![false; b.len()],
    )
}

// ============================================================================
// Checked comparison: the invariants every Comparison must satisfy
// ============================================================================

fn settings(motion: Motion, tolerance: f64, compare_bonds: bool) -> CompareSettings {
    CompareSettings {
        motion,
        tolerance,
        compare_bonds,
        ..CompareSettings::default()
    }
}

/// `compare`, plus the three-outcome rule (§3.4) and, for "equal", D2 checked
/// independently on the returned matching.
fn checked(a: &Raw, b: &Raw, s: &CompareSettings) -> Comparison {
    let c = compare(&a.cluster(), &b.cluster(), s).unwrap();
    assert!(
        !(c.equal && c.undecided),
        "undecided never comes with equal"
    );
    if c.equal {
        assert!(c.rejected_by.is_none() && c.mismatches.is_empty());
        assert_verdict_is_d2(a, b, &c, s);
    } else if c.rejected_by.is_some() {
        assert!(c.mismatches.is_empty() && !c.undecided);
    } else {
        assert!(
            !c.mismatches.is_empty(),
            "an unequal, unrejected pair names its mismatches"
        );
    }
    c
}

fn assert_verdict_is_d2(a: &Raw, b: &Raw, c: &Comparison, s: &CompareSettings) {
    assert_eq!(c.matching.len(), a.len(), "one pair per atom");
    let mut seen = vec![false; b.len()];
    for &(i, j) in &c.matching {
        assert!(!seen[j], "one to one");
        seen[j] = true;
        assert_eq!(a.elements[i], b.elements[j], "same element");
    }
    if s.compare_bonds {
        for &(i, j) in &c.matching {
            for &(k, l) in &c.matching {
                assert_eq!(a.bond_order(i, k), b.bond_order(j, l), "bonds agree");
            }
        }
    }
    let a_points: Vec<DVec3> = c.matching.iter().map(|&(i, _)| a.positions[i]).collect();
    let b_points: Vec<DVec3> = c.matching.iter().map(|&(_, j)| b.positions[j]).collect();
    let deviation = fit_deviation(&a_points, &b_points, s.motion);
    assert!(
        deviation <= s.tolerance,
        "the least-squares fit of the returned matching leaves {deviation} > {}",
        s.tolerance
    );
}

// ============================================================================
// Against the oracle
// ============================================================================

/// A pair for the oracle tests, drawn so that both verdicts are common.
fn oracle_pair(rng: &mut Rng, case: usize, tolerance: f64, min_same: f64, half: f64) -> (Raw, Raw) {
    let n = 4 + rng.below(5);
    let palettes: [&[i16]; 3] = [&[C, H], &[C, H, O], &[SI, H, C]];
    let palette = palettes[rng.below(3)];
    let bonded = rng.f64() < 0.7;
    let a = random_small(rng, n, palette, min_same, half, bonded);
    let rigid = |rng: &mut Rng| random_motion(rng, Motion::Rigid);
    let b = match case % 8 {
        // A moved copy of each kind in turn, jittered by up to 0.2–1.5
        // tolerances, so that every kind has equal and unequal pairs.
        0..=3 => {
            let (r, t) = random_motion(rng, MOTIONS[case % 4]);
            let jitter = rng.range(0.2, 1.5) * tolerance;
            copy_of(rng, &a, r, t, jitter).0
        }
        // The mirror image.
        4 => {
            let (r, t) = rigid(rng);
            copy_of(rng, &a, r * reflection(), t, 0.5 * tolerance).0
        }
        // One atom moved by about a tolerance.
        5 => {
            let mut moved = a.clone();
            let k = rng.below(n);
            moved.positions[k] += rng.unit() * tolerance * rng.range(0.7, 1.3);
            let (r, t) = rigid(rng);
            copy_of(rng, &moved, r, t, 0.0).0
        }
        // One bond changed.
        6 => {
            let mut changed = a.clone();
            if changed.bonds.is_empty() || rng.f64() < 0.3 {
                let i = rng.below(n);
                let j = (i + 1 + rng.below(n - 1)) % n;
                if changed.bond_order(i, j).is_none() {
                    changed.bonds.push((i, j, 1));
                }
            } else {
                let k = rng.below(changed.bonds.len());
                if rng.f64() < 0.5 {
                    changed.bonds.remove(k);
                } else {
                    changed.bonds[k].2 = 3 - changed.bonds[k].2;
                }
            }
            let (r, t) = rigid(rng);
            copy_of(rng, &changed, r, t, 0.3 * tolerance).0
        }
        // An unrelated cluster with the same element counts — or, now and
        // then, one element changed.
        _ => {
            let mut elements = a.elements.clone();
            if rng.f64() < 0.3 {
                elements[0] = if elements[0] == H { C } else { H };
            }
            random_positions_for(rng, elements, min_same, half, bonded)
        }
    };
    (a, b)
}

#[derive(Default, Debug)]
struct Tally {
    equal: [usize; 4],
    unequal: [usize; 4],
    rejections: [usize; 5],
    axes_alone_missed: usize,
}

#[test]
fn pipeline_agrees_with_the_oracle() {
    let mut rng = Rng::new(20261013);
    let mut tally = Tally::default();
    for case in 0..320 {
        let tolerance = if case % 3 == 0 { 0.3 } else { 0.15 };
        // §3.1's precondition: every same-element distance above 4·tolerance.
        let (a, b) = oracle_pair(&mut rng, case, tolerance, 4.0 * tolerance + 0.05, 1.8);
        let (ca, cb) = (a.cluster(), b.cluster());
        for (m, &motion) in MOTIONS.iter().enumerate() {
            for bonds in [false, true] {
                let s = settings(motion, tolerance, bonds);
                let expected = brute_force_equal(&a, &b, motion, tolerance, bonds);
                let c = checked(&a, &b, &s);
                assert_eq!(
                    c.equal, expected,
                    "case {case}, {motion:?}, bonds {bonds}: pipeline {} oracle {expected}\n{a:?}\n{b:?}\n{c:?}",
                    c.equal
                );
                if expected {
                    tally.equal[m] += 1;
                } else {
                    tally.unequal[m] += 1;
                }

                // Every pre-check, called directly, is exact.
                let checks = [
                    invariants::element_counts(&ca, &cb),
                    invariants::bond_counts(&ca, &cb),
                    invariants::labels(&ca, &cb),
                    invariants::eigenvalues(&ca, &cb, motion, tolerance),
                    invariants::distances(&ca, &cb, tolerance),
                ];
                for (k, check) in checks.iter().enumerate() {
                    // The two bond checks are necessary only when bonds count.
                    let applies = bonds || !matches!(k, 1 | 2);
                    if check.is_err() && applies {
                        assert!(
                            !expected,
                            "case {case}: pre-check {k} rejected an equal pair: {check:?}"
                        );
                        tally.rejections[k] += 1;
                    }
                }

                // The two proposal paths against each other (rigid motions).
                if matches!(motion, Motion::Rigid | Motion::RigidMirror) {
                    let only = |principal_axes, anchors| CompareSettings {
                        proposals: Proposals {
                            principal_axes,
                            anchors,
                        },
                        ..s
                    };
                    let anchors_alone = checked(&a, &b, &only(false, true));
                    assert_eq!(
                        anchors_alone.equal, expected,
                        "case {case}, {motion:?}, bonds {bonds}: anchors alone disagree"
                    );
                    let axes_alone = checked(&a, &b, &only(true, false));
                    assert!(!axes_alone.equal || expected);
                    if expected && !axes_alone.equal {
                        tally.axes_alone_missed += 1;
                    }
                }
            }
        }
    }
    println!("{tally:?}");
    for (m, motion) in MOTIONS.iter().enumerate() {
        assert!(
            tally.equal[m] >= 20 && tally.unequal[m] >= 20,
            "coverage for {motion:?}: {tally:?}"
        );
    }
    for (k, &count) in tally.rejections.iter().enumerate() {
        assert!(count > 0, "pre-check {k} never rejected: {tally:?}");
    }
}

/// Outside §3.1's precondition (same-element atoms closer than 4·tolerance)
/// the nearest-atom pairing may miss, but "equal" still implies the oracle.
#[test]
fn equal_is_exact_outside_the_precondition() {
    let mut rng = Rng::new(77);
    let (mut equal, mut missed, mut close_pairs) = (0, 0, 0);
    for case in 0..200 {
        let tolerance = 0.15;
        let (a, b) = oracle_pair(&mut rng, case, tolerance, 0.25, 0.8);
        let crowded = (0..a.len()).any(|i| {
            (0..i).any(|j| {
                a.elements[i] == a.elements[j]
                    && a.positions[i].distance(a.positions[j]) < 4.0 * tolerance
            })
        });
        close_pairs += crowded as usize;
        for motion in MOTIONS {
            for bonds in [false, true] {
                let s = settings(motion, tolerance, bonds);
                let expected = brute_force_equal(&a, &b, motion, tolerance, bonds);
                let c = checked(&a, &b, &s);
                assert!(!c.equal || expected, "case {case}: a wrong merge");
                equal += c.equal as usize;
                missed += (expected && !c.equal) as usize;
            }
        }
    }
    println!(
        "outside the precondition: {equal} equal, {missed} missed, {close_pairs} crowded clusters"
    );
    assert!(equal >= 50 && close_pairs >= 50, "coverage");
}

// ============================================================================
// Known answers on larger clusters
// ============================================================================

#[test]
fn invariance_on_large_clusters_matches_the_known_matching() {
    let mut rng = Rng::new(31);
    let mut verdicts = [[0usize; 2]; 4];
    for case in 0..24 {
        let n = 30 + rng.below(121);
        let a = random_large(&mut rng, n);
        let tolerance = 0.15;
        let kind = MOTIONS[case % 4];
        let (r, t) = random_motion(&mut rng, kind);
        let jitter = if case % 2 == 0 { 0.5 } else { 1.0 } * tolerance;
        let (b, origin) = copy_of(&mut rng, &a, r, t, jitter);
        for (m, &motion) in MOTIONS.iter().enumerate() {
            // The expected verdict is computed, not assumed: the least-squares
            // fit of the known matching against the tolerance.
            let a_points: Vec<DVec3> = origin.iter().map(|&i| a.positions[i]).collect();
            let expected = fit_deviation(&a_points, &b.positions, motion) <= tolerance;
            let c = checked(&a, &b, &settings(motion, tolerance, true));
            assert_eq!(
                c.equal, expected,
                "case {case} (n {n}, moved by {kind:?}), compared under {motion:?}"
            );
            verdicts[m][expected as usize] += 1;
        }
    }
    println!("verdicts per motion [unequal, equal]: {verdicts:?}");
    for (m, v) in verdicts.iter().enumerate() {
        assert!(v[0] >= 3 && v[1] >= 3, "{:?}: {v:?}", MOTIONS[m]);
    }
}

/// A carbon with four different substituents.
fn chiral_centre() -> Raw {
    let directions = [
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(1.0, -1.0, -1.0),
        DVec3::new(-1.0, 1.0, -1.0),
        DVec3::new(-1.0, -1.0, 1.0),
    ];
    let substituents = [(H, 1.09), (F, 1.35), (CL, 1.77), (BR, 1.94)];
    let mut positions = vec![DVec3::ZERO];
    let mut elements = vec![C];
    let mut bonds = Vec::new();
    for (k, (d, (e, length))) in directions.iter().zip(substituents).enumerate() {
        positions.push(d.normalize() * length);
        elements.push(e);
        bonds.push((0, k + 1, 1));
    }
    Raw {
        positions,
        elements,
        bonds,
    }
}

#[test]
fn a_chiral_cluster_equals_its_mirror_image_only_under_rigid_mirror() {
    let mut rng = Rng::new(41);
    let a = chiral_centre();
    let (r, t) = random_motion(&mut rng, Motion::RigidMirror);
    let (b, _) = copy_of(&mut rng, &a, r, t, 0.0);
    assert!(checked(&a, &b, &settings(Motion::RigidMirror, 0.15, true)).equal);
    assert!(!checked(&a, &b, &settings(Motion::Rigid, 0.15, true)).equal);
    // `translation` rejects a rotated copy, `rigid` the mirror image: each
    // kind rejects the motions it does not allow.
    let (r, t) = random_motion(&mut rng, Motion::Rigid);
    let (rotated, _) = copy_of(&mut rng, &a, r, t, 0.0);
    assert!(checked(&a, &rotated, &settings(Motion::Rigid, 0.15, true)).equal);
    assert!(!checked(&a, &rotated, &settings(Motion::Translation, 0.15, true)).equal);
}

/// Two silicon sites with their back-bond neighbours, a C over the first and
/// an O over the second (or swapped), bonded to the sites and to each other.
fn two_site_binding(sites: [DVec3; 2], back: &[(usize, DVec3)], swapped: bool) -> Raw {
    let mut positions = vec![sites[0], sites[1]];
    let mut elements = vec![SI, SI];
    let mut bonds = vec![(0, 1, 1)];
    for &(site, p) in back {
        positions.push(p);
        elements.push(SI);
        bonds.push((site, positions.len() - 1, 1));
    }
    let up = DVec3::new(0.0, 0.0, 1.9);
    let feet = if swapped { [O, C] } else { [C, O] };
    for (site, element) in feet.into_iter().enumerate() {
        positions.push(sites[site] + up);
        elements.push(element);
        bonds.push((site, positions.len() - 1, 1));
    }
    let n = positions.len();
    bonds.push((n - 2, n - 1, 1));
    Raw {
        positions,
        elements,
        bonds,
    }
}

#[test]
fn feet_swapped_over_inequivalent_sites_are_never_equal() {
    let sites = [DVec3::ZERO, DVec3::new(2.4, 0.0, 0.0)];
    let back = [
        (0, DVec3::new(-0.8, 1.5, -1.5)),
        (0, DVec3::new(-0.8, -1.5, -1.5)),
        (1, DVec3::new(3.2, 1.6, -1.4)),
    ];
    let a = two_site_binding(sites, &back, false);
    let b = two_site_binding(sites, &back, true);
    let mut rng = Rng::new(42);
    let (r, t) = random_motion(&mut rng, Motion::Rigid);
    let (b, _) = copy_of(&mut rng, &b, r, t, 0.0);
    for motion in MOTIONS {
        for bonds in [false, true] {
            assert!(
                !checked(&a, &b, &settings(motion, 0.15, bonds)).equal,
                "{motion:?}, bonds {bonds}"
            );
        }
    }
}

#[test]
fn feet_swapped_over_a_symmetric_dimer_are_the_same_binding() {
    // The dimer and its back-bonds are symmetric under the twofold axis
    // through the dimer's midpoint along z, which swaps the two sites.
    let sites = [DVec3::new(-1.2, 0.0, 0.0), DVec3::new(1.2, 0.0, 0.0)];
    let back = [
        (0, DVec3::new(-2.0, 1.9, -1.2)),
        (0, DVec3::new(-2.0, -1.9, -1.2)),
        (1, DVec3::new(2.0, 1.9, -1.2)),
        (1, DVec3::new(2.0, -1.9, -1.2)),
    ];
    let a = two_site_binding(sites, &back, false);
    let b = two_site_binding(sites, &back, true);
    let mut rng = Rng::new(43);
    let (r, t) = random_motion(&mut rng, Motion::Rigid);
    let (moved, _) = copy_of(&mut rng, &b, r, t, 0.02);
    assert!(checked(&a, &moved, &settings(Motion::Rigid, 0.15, true)).equal);
    assert!(checked(&a, &moved, &settings(Motion::RigidMirror, 0.15, true)).equal);
    // Unmoved, `none` and `translation` cannot swap the sites.
    assert!(!checked(&a, &b, &settings(Motion::None, 0.15, true)).equal);
    assert!(!checked(&a, &b, &settings(Motion::Translation, 0.15, true)).equal);
}

#[test]
fn a_different_bond_order_is_rejected_by_the_bond_counts() {
    let mut rng = Rng::new(44);
    let a = random_large(&mut rng, 30);
    assert!(!a.bonds.is_empty());
    let mut b = a.clone();
    b.bonds[0].2 = 2;
    assert!(checked(&a, &b, &settings(Motion::RigidMirror, 0.15, false)).equal);
    let c = checked(&a, &b, &settings(Motion::RigidMirror, 0.15, true));
    assert!(!c.equal);
    let rejection = c.rejected_by.expect("a pre-check decides");
    assert_eq!(rejection.check, PreCheck::BondCounts);
    assert_eq!(c.proposals, 0, "no motion is tried");
    assert!(rejection.reason.contains("C–C") || rejection.reason.contains("–"));
}

#[test]
fn the_same_bond_counts_on_different_atoms_fail_the_final_check() {
    // A rectangle: no symmetry of it takes the two long sides onto the two
    // short ones, and every count and label agrees.
    let positions = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(2.0, 0.0, 0.0),
        DVec3::new(2.0, 1.5, 0.0),
        DVec3::new(0.0, 1.5, 0.0),
    ];
    let a = Raw {
        positions: positions.clone(),
        elements: vec![C; 4],
        bonds: vec![(0, 1, 1), (2, 3, 1)],
    };
    let b = Raw {
        positions,
        elements: vec![C; 4],
        bonds: vec![(0, 3, 1), (1, 2, 1)],
    };
    let c = checked(&a, &b, &settings(Motion::RigidMirror, 0.15, true));
    assert!(!c.equal);
    assert!(c.rejected_by.is_none(), "{:?}", c.rejected_by);
    assert!(c.mismatches.iter().any(|m| matches!(
        m,
        Mismatch::BondOnlyInA { .. } | Mismatch::BondOnlyInB { .. }
    )));
    assert!(checked(&a, &b, &settings(Motion::RigidMirror, 0.15, false)).equal);
}

fn collinear() -> Raw {
    let elements = vec![C, O, C, N, H];
    let positions = (0..5)
        .map(|k| DVec3::new(1.2 * k as f64, 0.0, 0.0))
        .collect();
    Raw {
        positions,
        elements,
        bonds: (0..4).map(|k| (k, k + 1, 1)).collect(),
    }
}

/// A six-ring of carbons, with an H on each when `hydrogens`.
fn ring(hydrogens: bool) -> Raw {
    let mut raw = Raw {
        positions: Vec::new(),
        elements: Vec::new(),
        bonds: Vec::new(),
    };
    for k in 0..6 {
        let angle = k as f64 * std::f64::consts::FRAC_PI_3;
        raw.positions
            .push(DVec3::new(angle.cos(), angle.sin(), 0.0) * 1.39);
        raw.elements.push(C);
        raw.bonds.push((k, (k + 1) % 6, 1));
    }
    if hydrogens {
        for k in 0..6 {
            raw.positions.push(raw.positions[k] * (2.48 / 1.39));
            raw.elements.push(H);
            raw.bonds.push((k, 6 + k, 1));
        }
    }
    raw
}

fn threefold() -> Raw {
    let mut raw = Raw {
        positions: vec![DVec3::new(0.0, 0.0, 0.38)],
        elements: vec![N],
        bonds: Vec::new(),
    };
    for k in 0..3 {
        let angle = k as f64 * 2.0 * std::f64::consts::FRAC_PI_3;
        raw.positions
            .push(DVec3::new(angle.cos(), angle.sin(), 0.0) * 0.94);
        raw.elements.push(H);
        raw.bonds.push((0, k + 1, 1));
    }
    raw
}

#[test]
fn degenerate_shapes_equal_their_moved_copies() {
    let mut rng = Rng::new(45);
    let only_anchors = CompareSettings {
        proposals: Proposals {
            principal_axes: false,
            anchors: true,
        },
        ..CompareSettings::default()
    };
    let only_axes = CompareSettings {
        proposals: Proposals {
            principal_axes: true,
            anchors: false,
        },
        ..CompareSettings::default()
    };
    let mut axes_failed = 0;
    for (name, shape, jitter) in [
        ("collinear", collinear(), 0.0),
        ("benzene", ring(true), 0.01),
        ("ring", ring(false), 0.01),
        ("threefold", threefold(), 0.01),
    ] {
        let a = shape.jittered(&mut rng, jitter);
        for _ in 0..5 {
            let (r, t) = random_motion(&mut rng, Motion::Rigid);
            let (b, _) = copy_of(&mut rng, &a, r, t, jitter);
            assert!(checked(&a, &b, &CompareSettings::default()).equal, "{name}");
            assert!(
                checked(&a, &b, &only_anchors).equal,
                "{name}, anchors alone"
            );
            axes_failed += !checked(&a, &b, &only_axes).equal as usize;
        }
    }
    // The fallback was needed: the principal axes alone missed some of these.
    assert!(axes_failed > 0);
}

#[test]
fn refinement_recovers_a_swapped_pairing() {
    // Ten carbons close to the z axis, two hydrogens 0.65 Å apart across it.
    // A proposal turned 120° about z moves every carbon by under 3·tolerance,
    // and pairs each hydrogen with the other's copy: a complete pairing with a
    // swap. (Under the precondition a swap needs a turn of this size — a few
    // degrees moves both hydrogens the same way and cannot swap them.) The
    // carbons dominate the refit, which turns back to the identity and
    // un-swaps the hydrogens.
    let mut a = Raw {
        positions: vec![DVec3::new(0.325, 0.0, 0.0), DVec3::new(-0.325, 0.0, 0.0)],
        elements: vec![H, H],
        bonds: Vec::new(),
    };
    for (k, z) in [-5, -4, -3, -2, -1, 1, 2, 3, 4, 5].into_iter().enumerate() {
        let angle = 1.3 * k as f64;
        a.positions
            .push(DVec3::new(angle.cos() * 0.22, angle.sin() * 0.22, z as f64));
        a.elements.push(C);
    }
    let b = a.clone();
    let s = settings(Motion::Rigid, 0.15, false);
    let (ca, cb) = (a.cluster(), b.cluster());
    let context = PairContext::new(&ca, &cb, &s);
    let turn = Proposal {
        r: DMat3::from_quat(DQuat::from_rotation_z(120f64.to_radians())),
        t: DVec3::ZERO,
        improper: false,
    };
    let first = context.pair(&turn);
    assert_eq!(first[0], Some(1), "the first pairing swaps the hydrogens");
    assert_eq!(first[1], Some(0));
    assert!(first.iter().all(Option::is_some), "and is complete");
    match context.verify(&turn) {
        Verification::Equal(found) => {
            assert!(found.matching.contains(&(0, 0)) && found.matching.contains(&(1, 1)));
        }
        Verification::NotEqual(attempt) => panic!("refinement did not recover: {attempt:?}"),
    }
}

#[test]
fn pairing_never_crosses_elements() {
    // A C–H pair 0.3 Å apart, closer than 3·tolerance: under a proposal that
    // shifts B by that much, B's C lands on A's H.
    let a = Raw {
        positions: vec![
            DVec3::ZERO,
            DVec3::new(0.3, 0.0, 0.0),
            DVec3::new(0.0, 1.5, 0.0),
            DVec3::new(0.0, 0.0, 1.5),
        ],
        elements: vec![C, H, O, N],
        bonds: Vec::new(),
    };
    let s = settings(Motion::Rigid, 0.15, false);
    let (ca, cb) = (a.cluster(), a.cluster());
    let context = PairContext::new(&ca, &cb, &s);
    let shift = Proposal {
        r: DMat3::IDENTITY,
        t: DVec3::new(0.3, 0.0, 0.0),
        improper: false,
    };
    // Nearest by position, A's H would take B's C.
    let moved_c = shift.apply(a.positions[0]);
    let moved_h = shift.apply(a.positions[1]);
    assert!(moved_c.distance(a.positions[1]) < moved_h.distance(a.positions[1]));
    let pairing = context.pair(&shift);
    for (i, j) in pairing.iter().enumerate() {
        if let Some(j) = j {
            assert_eq!(
                a.elements[i], a.elements[*j],
                "atom {i} paired across elements"
            );
        }
    }
    assert_eq!(pairing[1], Some(1), "A's H takes B's H");
}

#[test]
fn translation_finds_a_shifted_copy_and_rejects_a_rotated_one() {
    let mut rng = Rng::new(46);
    let a = random_large(&mut rng, 20);
    let s = settings(Motion::Translation, 0.15, true);
    let shift = random_translation(&mut rng);
    let (shifted, _) = copy_of(&mut rng, &a, DMat3::IDENTITY, shift, 0.03);
    let c = checked(&a, &shifted, &s);
    assert!(c.equal);
    assert_eq!(c.proposals, 1, "its single proposal");
    let turned = DMat3::from_quat(DQuat::from_rotation_z(30f64.to_radians()));
    let (rotated, _) = copy_of(&mut rng, &a, turned, DVec3::ZERO, 0.0);
    let c = checked(&a, &rotated, &s);
    assert!(!c.equal);
    assert!(c.proposals <= 1);
}

#[test]
fn the_work_cap_makes_a_pair_undecided() {
    // A ring: its in-plane principal axes are set by the jitter alone, so the
    // first proposal misses and one proposal is all the cap allows.
    let mut rng = Rng::new(47);
    let a = ring(false).jittered(&mut rng, 0.01);
    let (r, t) = random_motion(&mut rng, Motion::Rigid);
    let (b, _) = copy_of(&mut rng, &a, r, t, 0.01);
    assert!(checked(&a, &b, &CompareSettings::default()).equal);
    let capped = CompareSettings {
        max_proposals: 1,
        ..CompareSettings::default()
    };
    let c = checked(&a, &b, &capped);
    assert!(c.undecided && !c.equal, "{c:?}");
    assert_eq!(c.proposals, 1);
}

#[test]
fn a_near_miss_reports_the_copys_motion_and_the_displaced_atom() {
    // A planar cluster with one atom lifted out of the plane by 3·tolerance:
    // the distances barely change (the lift is perpendicular to every
    // in-plane distance), so the pre-checks pass and the motion search runs.
    let mut rng = Rng::new(48);
    let tolerance = 0.15;
    let mut a = random_large(&mut rng, 40);
    a.positions = Vec::new();
    while a.positions.len() < 16 {
        let p = DVec3::new(rng.range(-3.0, 3.0), rng.range(-3.0, 3.0), 0.0);
        if a.positions.iter().all(|q| q.distance(p) >= 1.05) {
            a.positions.push(p);
        }
    }
    a.elements.truncate(16);
    a.bonds.clear();
    let k = 5;
    let mut lifted = a.clone();
    lifted.positions[k].z += 3.0 * tolerance;
    let (r, t) = random_motion(&mut rng, Motion::Rigid);
    let (b, origin) = copy_of(&mut rng, &lifted, r, t, 0.0);

    let c = checked(&a, &b, &settings(Motion::RigidMirror, tolerance, true));
    assert!(!c.equal && c.rejected_by.is_none(), "{c:?}");
    let fit = c.transform.expect("a best fit");
    for (j, &i) in origin.iter().enumerate() {
        if i != k {
            assert!(
                (fit.r * b.positions[j] + fit.t).distance(a.positions[i]) < tolerance,
                "the reported fit is the copy's motion"
            );
        }
    }
    // The displaced atom is named from both sides, and nothing else is.
    assert_eq!(c.mismatches.len(), 2, "{:?}", c.mismatches);
    for m in &c.mismatches {
        match m {
            Mismatch::UnmatchedInB { position, .. } => {
                assert!(position.distance(a.positions[k]) < 1e-9)
            }
            Mismatch::UnmatchedInA { position, .. } => {
                // B's displaced atom, where the reported fit puts it.
                let j = origin.iter().position(|&i| i == k).unwrap();
                assert!(position.distance(fit.r * b.positions[j] + fit.t) < 1e-9);
                assert!(position.distance(a.positions[k]) > tolerance);
            }
            other => panic!("unexpected {other}"),
        }
    }
}

#[test]
fn settings_and_clusters_are_validated() {
    let a = chiral_centre().cluster();
    for tolerance in [0.0, -0.1, 0.31, f64::NAN] {
        let s = settings(Motion::Rigid, tolerance, true);
        assert!(matches!(
            compare(&a, &a, &s),
            Err(CompareError::Tolerance(_))
        ));
    }
    assert!(compare(&a, &a, &settings(Motion::Rigid, 0.3, true)).is_ok());
    let structure = chiral_centre().structure();
    assert!(matches!(
        Cluster::from_tag(&structure, "nope"),
        Err(CompareError::UnknownTag(_))
    ));
    assert!(matches!(
        Cluster::from_atoms(&structure, &[]),
        Err(CompareError::EmptyCluster)
    ));
    let whole = Cluster::whole(&structure).unwrap();
    assert_eq!(whole.len(), 5);
    assert_eq!(whole.bonds().len(), 4);
    // Bonds leaving the cluster are dropped.
    let part = Cluster::from_atoms(&structure, &[1, 2, 3]).unwrap();
    assert_eq!(part.bonds().len(), 2);
}

#[test]
fn motion_serializes_in_the_text_format_spelling() {
    for motion in MOTIONS {
        let json = serde_json::to_string(&motion).unwrap();
        assert_eq!(json, format!("\"{}\"", motion.name()));
        assert_eq!(serde_json::from_str::<Motion>(&json).unwrap(), motion);
    }
}

// ============================================================================
// The matched comparison (moved from mechanosynth)
// ============================================================================

#[test]
fn compare_structures_matched_returns_a_consistent_matching() {
    let mut rng = Rng::new(49);
    let a = random_large(&mut rng, 25);
    let mut b = a.clone();
    b.positions[3] += DVec3::new(0.5, 0.0, 0.0);
    let (sa, sb) = (a.structure(), b.structure());
    let matched = compare_structures_matched(&sa, &sb, 0.1, true);
    assert_eq!(matched.pairs.len(), 24);
    assert!(!matched.pairs.iter().any(|&(x, _)| x == 4));
    for &(x, y) in &matched.pairs {
        assert_eq!(x, y);
    }
    assert_eq!(matched.mismatches.len(), 2);
    assert_eq!(
        compare_structures(&sa, &sb, 0.1),
        compare_structures_matched(&sa, &sb, 0.1, false).mismatches
    );
}

#[test]
fn the_same_element_flag_turns_an_element_difference_into_unmatched_atoms() {
    let a = chiral_centre();
    let mut b = a.clone();
    b.elements[1] = O;
    let (sa, sb) = (a.structure(), b.structure());
    // Off: today's behaviour, an element difference on a matched pair.
    let off = compare_structures_matched(&sa, &sb, 0.1, false);
    assert_eq!(off.pairs.len(), 5);
    assert!(matches!(
        off.mismatches[..],
        [Mismatch::ElementDiffers { .. }]
    ));
    // On: no partner at all.
    let on = compare_structures_matched(&sa, &sb, 0.1, true);
    assert_eq!(on.pairs.len(), 4);
    assert!(on.mismatches.iter().all(|m| matches!(
        m,
        Mismatch::UnmatchedInA { .. } | Mismatch::UnmatchedInB { .. }
    )));
}

// ============================================================================
// Grouping
// ============================================================================

/// Six base clusters, four jittered moved copies of each, in shuffled order;
/// returns the clusters and each one's base.
fn copies_of_bases(rng: &mut Rng, n: usize) -> (Vec<Raw>, Vec<usize>) {
    let bases: Vec<Raw> = (0..6).map(|_| random_large(rng, n)).collect();
    let mut items = Vec::new();
    for (k, base) in bases.iter().enumerate() {
        for _ in 0..4 {
            let kind = MOTIONS[2 + rng.below(2)];
            let (r, t) = random_motion(rng, kind);
            items.push((copy_of(rng, base, r, t, 0.02).0, k));
        }
    }
    let order = shuffle(rng, items.len());
    order.into_iter().map(|i| items[i].clone()).unzip()
}

fn grouping_in_pool(clusters: &[Cluster], s: &CompareSettings, threads: usize) -> Grouping {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap()
        .install(|| group_in_order(clusters, s).unwrap())
}

fn assert_consistent(g: &Grouping) {
    for (i, &rep) in g.class_of.iter().enumerate() {
        assert!(rep <= i, "a representative comes first");
        assert_eq!(g.class_of[rep], rep, "a representative is listed");
        let copies = g
            .class_of
            .iter()
            .enumerate()
            .filter(|&(j, &r)| r == rep && j != rep)
            .count();
        assert_eq!(g.copies[rep], copies);
        if rep != i {
            assert_eq!(g.copies[i], 0);
        }
    }
    let listed: Vec<usize> = (0..g.class_of.len())
        .filter(|&i| g.class_of[i] == i)
        .collect();
    assert_eq!(g.listed, listed);
}

#[test]
fn grouping_keeps_the_first_member_of_each_class_whatever_the_thread_count() {
    let mut rng = Rng::new(50);
    let (raws, bases) = copies_of_bases(&mut rng, 18);
    let clusters: Vec<Cluster> = raws.iter().map(Raw::cluster).collect();
    let s = CompareSettings::default();
    let g1 = grouping_in_pool(&clusters, &s, 1);
    for threads in [2, 8] {
        assert_eq!(
            grouping_in_pool(&clusters, &s, threads),
            g1,
            "{threads} threads"
        );
    }
    assert_consistent(&g1);
    for (i, &base) in bases.iter().enumerate() {
        let first = bases.iter().position(|&b| b == base).unwrap();
        assert_eq!(g1.class_of[i], first, "cluster {i}");
    }
    assert_eq!(g1.listed.len(), 6);
    assert_eq!(g1.undecided, 0);
}

#[test]
fn grouping_is_greedy_on_a_non_transitive_chain() {
    // B is within tolerance of A and of C; A and C are not within it.
    let mut rng = Rng::new(51);
    let a = random_large(&mut rng, 12);
    let step = DVec3::new(0.8 * 0.15, 0.0, 0.0);
    let mut b = a.clone();
    b.positions[0] += step;
    let mut c = a.clone();
    c.positions[0] += 2.0 * step;
    let s = settings(Motion::None, 0.15, true);
    assert!(checked(&a, &b, &s).equal && checked(&b, &c, &s).equal);
    assert!(!checked(&a, &c, &s).equal);
    let g = group_in_order(&[a.cluster(), b.cluster(), c.cluster()], &s).unwrap();
    assert_eq!(g.class_of, vec![0, 0, 2]);
    assert_eq!(g.copies, vec![1, 0, 0]);
    assert_eq!(g.listed, vec![0, 2]);
    assert_consistent(&g);
}

#[test]
fn grouping_counts_undecided_pairs() {
    let mut rng = Rng::new(47);
    let a = ring(false).jittered(&mut rng, 0.01);
    let (r, t) = random_motion(&mut rng, Motion::Rigid);
    let (b, _) = copy_of(&mut rng, &a, r, t, 0.01);
    let capped = CompareSettings {
        max_proposals: 1,
        ..CompareSettings::default()
    };
    let g = group_in_order(&[a.cluster(), b.cluster()], &capped).unwrap();
    assert_eq!(g.listed, vec![0, 1], "an undecided pair is not merged");
    assert_eq!((g.compared, g.undecided), (1, 1));
    let g = group_in_order(&[a.cluster(), b.cluster()], &CompareSettings::default()).unwrap();
    assert_eq!((g.listed.len(), g.undecided), (1, 0));
}

#[test]
fn grouping_bins_by_element_counts() {
    let mut rng = Rng::new(52);
    let a = random_large(&mut rng, 15);
    let mut other = a.clone();
    other.elements[0] = if other.elements[0] == H { C } else { H };
    let g = group_in_order(
        &[a.cluster(), other.cluster(), a.cluster()],
        &CompareSettings::default(),
    )
    .unwrap();
    assert_eq!(g.class_of, vec![0, 1, 0]);
    assert_eq!(g.compared, 1, "only the same bin is compared");
}

// ============================================================================
// Timing (release)
// ============================================================================

#[test]
#[ignore = "timing; run in release"]
fn grouping_hexapod_sized_clusters_is_fast() {
    // Twenty bindings of one composition and one bond count — the realistic
    // case, where every pair lands in the same bin and the shape pre-checks
    // and the motion search do the work: one structure with three atoms
    // moved by 0.8 Å, different atoms for each binding.
    let mut rng = Rng::new(60);
    let start_structure = random_large(&mut rng, 120);
    let bases: Vec<Raw> = (0..20)
        .map(|k| {
            let mut base = start_structure.clone();
            for i in 3 * k..3 * k + 3 {
                base.positions[i] += rng.unit() * 0.8;
            }
            base
        })
        .collect();
    let mut clusters = Vec::new();
    for base in &bases {
        for _ in 0..10 {
            let kind = MOTIONS[2 + rng.below(2)];
            let (r, t) = random_motion(&mut rng, kind);
            clusters.push(copy_of(&mut rng, base, r, t, 0.3 * 0.15).0.cluster());
        }
    }
    let order = shuffle(&mut rng, clusters.len());
    let clusters: Vec<Cluster> = order.into_iter().map(|i| clusters[i].clone()).collect();
    let s = CompareSettings::default();
    let _ = group_in_order(&clusters, &s).unwrap();
    let start = Instant::now();
    let g = group_in_order(&clusters, &s).unwrap();
    let seconds = start.elapsed().as_secs_f64();
    println!(
        "200 clusters of 120 atoms: {seconds:.3} s, {} listed, {} pairs compared, {} undecided",
        g.listed.len(),
        g.compared,
        g.undecided
    );
    assert_eq!(g.listed.len(), 20);
    assert!(seconds < 0.25, "{seconds:.3} s");
}

#[test]
#[ignore = "timing; run in release"]
fn pair_at_the_work_cap_is_fast() {
    // A 5×5×5 simple-cubic block of one element, compared without bonds (so
    // every atom has the one label), its centre first so the anchor search
    // has a candidate at every atom; against a copy with one atom moved by
    // 1.5·tolerance — every pre-check passes, no motion verifies.
    let spacing = 1.54;
    let mut positions = vec![DVec3::splat(2.0 * spacing)];
    for x in 0..5 {
        for y in 0..5 {
            for z in 0..5 {
                let p = DVec3::new(x as f64, y as f64, z as f64) * spacing;
                if p != positions[0] {
                    positions.push(p);
                }
            }
        }
    }
    let a = Raw {
        elements: vec![C; positions.len()],
        positions,
        bonds: Vec::new(),
    };
    let mut rng = Rng::new(61);
    let mut near = a.clone();
    near.positions[7] += rng.unit() * 1.5 * 0.15;
    let (r, t) = random_motion(&mut rng, Motion::Rigid);
    let (b, _) = copy_of(&mut rng, &near, r, t, 0.0);
    let s = settings(Motion::RigidMirror, 0.15, false);
    let (ca, cb) = (a.cluster(), b.cluster());
    let _ = compare(&ca, &cb, &s).unwrap();
    let runs = 5;
    let start = Instant::now();
    let mut c = None;
    for _ in 0..runs {
        c = Some(compare(&ca, &cb, &s).unwrap());
    }
    let seconds = start.elapsed().as_secs_f64() / runs as f64;
    let c = c.unwrap();
    println!(
        "{} atoms: {:.2} ms for {} proposals ({:.1} µs each), undecided {}",
        ca.len(),
        seconds * 1e3,
        c.proposals,
        seconds * 1e6 / c.proposals as f64,
        c.undecided
    );
    assert!(!c.equal && c.rejected_by.is_none());
    assert!(c.undecided, "driven to the cap");
    assert!(seconds < 0.020, "{:.2} ms", seconds * 1e3);
}
