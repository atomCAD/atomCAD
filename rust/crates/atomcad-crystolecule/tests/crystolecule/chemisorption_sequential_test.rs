//! The sequential chemisorption search, geometric phase
//! (`design_chemisorption_sequential.md` §11): the predicates, a hand-worked
//! case, the oracle, planted bindings, the metamorphic tests, the rules and
//! the tree. Everything here is `plan` only, so nothing is relaxed; the tests
//! that relax are in `chemisorption_sequential_relax_test.rs`.

use crate::chemisorption_sequential_support::*;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::chemisorption::sequential::*;
use atomcad_crystolecule::chemisorption::{
    BondInventory, ChemisorptionError, Side, TransferDirection, TransferRule, input_fingerprint,
};
use glam::{DMat3, DQuat, DVec3};
use std::collections::{BTreeMap, BTreeSet};

const H_TO_SUBSTRATE: TransferRule = TransferRule {
    element: H,
    direction: TransferDirection::ToSubstrate,
};

fn cfg(anchor_reach: f64, tolerance: f64) -> SequentialSearch {
    SequentialSearch {
        anchor_reach,
        tolerance,
        clash_filter: false,
        ..Default::default()
    }
}

/// Methanol, CH3–O–H, its O at `p`, the O–H along `h_dir`. Returns (O, H).
fn add_methanol(s: &mut AtomicStructure, p: DVec3, h_dir: DVec3) -> (u32, u32) {
    let o = add_methoxy(s, p);
    let h = s.add_atom(H, p + h_dir.normalize() * 0.96);
    s.add_bond(o, h, BOND_SINGLE);
    (o, h)
}

/// H to the substrate, with only the tagged O as feet: untagged, every C–H
/// carbon of a methyl would be a donor foot too.
fn oh_feet(reach: f64) -> SequentialSearch {
    SequentialSearch {
        transfers: vec![H_TO_SUBSTRATE],
        adsorbate_tag: Some("foot".into()),
        reach,
        ..cfg(2.0, 0.0)
    }
}

/// Methanol with its O tagged as the foot.
fn add_tagged_methanol(s: &mut AtomicStructure, p: DVec3, h_dir: DVec3) -> (u32, u32) {
    let (o, h) = add_methanol(s, p, h_dir);
    s.add_atom_tag(o, "foot").unwrap();
    (o, h)
}

fn legs_of(h: &Hypothesis) -> Vec<(usize, usize)> {
    let mut v: Vec<(usize, usize)> = h.steps.iter().map(|s| (s.leg.foot, s.leg.site)).collect();
    v.sort_unstable();
    v
}

fn by_legs(p: &SequentialPlan) -> BTreeMap<Vec<(usize, usize)>, &Hypothesis> {
    p.hypotheses.iter().map(|h| (legs_of(h), h)).collect()
}

fn tripod_over_slab() -> (AtomicStructure, AtomicStructure) {
    (posed_stand_in(3, 0.0, DVec3::ZERO).0, si100_slab(5.0, 11.0))
}

// ============================================================================
// Predicates (§11.3 unit cases)
// ============================================================================

#[test]
fn rest_lengths_are_uff_single_bonds_of_bare_atoms() {
    assert!((rest_length(O, SI) - 1.716).abs() < 1e-3);
    assert!((rest_length(C, SI) - 1.867).abs() < 1e-3);
    assert!((rest_length(H, SI) - 1.471).abs() < 1e-3);
    assert_eq!(rest_length(O, SI), rest_length(SI, O));
}

/// Two feet 5 Å apart, the first anchored over its site; the second site is
/// moved along x through both bounds of the shell. The test is `≤`: a site
/// exactly on a bound passes at that tolerance and fails just below it.
#[test]
fn the_shell_includes_both_bounds() {
    let b = rest_length(O, SI);
    let d = 5.0;
    let slack = 2.0 * b;
    for (r, bound) in [(d + slack + 0.4, "outer"), (d - slack - 0.3, "inner")] {
        let (ads, _) = methoxy_feet(&[DVec3::ZERO, DVec3::new(d, 0.0, 0.0)]);
        let (sub, _) = silyl_sites(&[DVec3::new(0.0, 0.0, -2.0), DVec3::new(r, 0.0, -2.0)], 3);
        let probe = plan(&ads, &sub, &cfg(2.5, 0.0)).unwrap();
        let need = probe
            .setup
            .pair_need(Leg { foot: 0, site: 0 }, Leg { foot: 1, site: 1 });
        let expected = if bound == "outer" {
            r - (d + slack)
        } else {
            (d - slack) - r
        };
        assert!((need - expected).abs() < 2e-9, "{bound}: need {need}");

        let found = |tol: f64| {
            let p = plan(&ads, &sub, &cfg(2.5, tol)).unwrap();
            by_legs(&p).contains_key(&vec![(0, 0), (1, 1)])
        };
        assert!(found(need), "{bound}: on the bound");
        assert!(
            !found(f64::from_bits(need.to_bits() - 1)),
            "{bound}: just inside"
        );
        assert!(found(need + 0.01));
    }
}

/// Feet closer than `b₁ + b₂` make the inner bound negative: the shell is a
/// solid ball, and a site at any distance up to the outer bound passes.
#[test]
fn a_compact_pair_of_feet_has_a_solid_shell() {
    let (ads, _) = methoxy_feet(&[DVec3::ZERO, DVec3::new(1.5, 0.0, 0.0)]);
    let (sub, _) = silyl_sites(&[DVec3::new(0.0, 0.0, -1.7), DVec3::new(0.0, 0.0, -1.7)], 3);
    let p = plan(&ads, &sub, &cfg(2.0, 0.0)).unwrap();
    assert_eq!(
        p.setup
            .pair_need(Leg { foot: 0, site: 0 }, Leg { foot: 1, site: 1 }),
        0.0
    );
}

#[test]
fn the_local_up_is_the_open_side_normal() {
    // Silyls whose H point down: exactly up.
    let (ads, _) = methoxy_feet(&[DVec3::new(0.0, 0.0, 2.0)]);
    let ring: Vec<DVec3> = (0..5)
        .map(|i| {
            let a = (i as f64 * 72.0).to_radians();
            DVec3::new(3.0 * a.cos(), 3.0 * a.sin(), 0.0)
        })
        .collect();
    let (sub, _) = silyl_sites(&ring, 3);
    let p = plan(&ads, &sub, &cfg(2.0, 0.0)).unwrap();
    let up = p.setup.local_up(&[0, 1, 2, 3, 4]);
    assert!((up - DVec3::Z).length() < 1e-9, "{up}");

    // The reconstructed Si(100) surface: up at the central dimer, and down
    // once the slab is turned over.
    let slab = si100_slab(5.0, 11.0);
    let (site, partner) = central_dimer(&slab);
    let p = plan(&ads, &slab, &cfg(2.0, 0.0)).unwrap();
    let index = |id: u32| {
        let c = p.setup.substrate_ids[&id];
        p.setup.sites.iter().position(|s| s.id == c).unwrap()
    };
    let up = p.setup.local_up(&[index(site), index(partner)]);
    assert!(up.z > 0.95, "{up}");
    let flipped = moved(
        &slab,
        DQuat::from_rotation_x(std::f64::consts::PI),
        DVec3::ZERO,
    );
    let p = plan(&ads, &flipped, &cfg(2.0, 0.0)).unwrap();
    let up = p.setup.local_up(&[index(site), index(partner)]);
    assert!(up.z < -0.95, "{up}");
}

/// The two-leg θ rule puts the body above its bonds, clash-free.
#[test]
fn two_legs_are_seated_body_up() {
    let (ads, slab) = tripod_over_slab();
    let p = plan(
        &ads,
        &slab,
        &SequentialSearch {
            formed_bonds: Some(2),
            ..cfg(3.5, 0.0)
        },
    )
    .unwrap();
    let setup = &p.setup;
    let mut checked = 0;
    for h in p.hypotheses.iter().filter(|h| h.legs() == 2) {
        let seating = h.seating.as_ref().unwrap();
        // Only top-face pairs: the side faces have their own normals.
        let sites: Vec<DVec3> = h
            .steps
            .iter()
            .map(|s| setup.sites[s.leg.site].position)
            .collect();
        if sites.iter().any(|s| s.z.abs() > 0.5) {
            continue;
        }
        let centroid = setup
            .posed()
            .iter()
            .map(|&x| seating.apply(x))
            .sum::<DVec3>()
            / setup.posed().len() as f64;
        let height = (centroid - (sites[0] + sites[1]) / 2.0).dot(seating.up);
        assert!(height > 2.0, "{:?}: body {height:.2} Å above", legs_of(h));
        assert!(!seating.clashes(), "{:?}", legs_of(h));
        checked += 1;
    }
    assert!(checked > 10);
}

/// One-leg bindings are listed only for a one-foot adsorbate (D8).
#[test]
fn one_leg_bindings_are_candidates_only_for_a_one_foot_adsorbate() {
    let (sub, _) = silyl_sites(&[DVec3::ZERO, DVec3::new(6.0, 0.0, 0.0)], 3);
    let (one, _) = methoxy_feet(&[DVec3::new(0.0, 0.0, 1.8)]);
    let p = plan(&one, &sub, &cfg(2.0, 0.0)).unwrap();
    assert_eq!(p.hypotheses.len(), 1);
    assert!(p.hypotheses[0].candidate);

    let (two, _) = methoxy_feet(&[DVec3::new(0.0, 0.0, 1.8), DVec3::new(6.0, 0.0, 1.8)]);
    let p = plan(&two, &sub, &cfg(2.0, 0.0)).unwrap();
    assert_eq!(p.stats.anchors, 2);
    for h in &p.hypotheses {
        assert_eq!(h.candidate, h.legs() == 2, "{:?}", legs_of(h));
    }
    // A cap of one leg leaves nothing to list for a two-foot adsorbate.
    let p = plan(
        &two,
        &sub,
        &SequentialSearch {
            max_formed_bonds: Some(1),
            ..cfg(2.0, 0.0)
        },
    )
    .unwrap();
    assert_eq!(p.stats.candidates, 0);
    assert_eq!(p.stats.anchors, 2);
}

// ============================================================================
// Hand-worked (§11.3)
// ============================================================================

/// Three methoxy feet on an equilateral triangle of side 6 Å (z = 0, the
/// methyls above), over five silyl sites at z = −1.7. With b(O–Si) = 1.716 the
/// shell at tolerance 0 is 2.568 ≤ r ≤ 9.432 for every pair of feet.
///
/// ```text
///   sites   A (0,0)  B (6,0)  C (3,5.196) — under feet 1, 2, 3
///           D (1.5,0)          E (13,0)
///   r       AB AC BC 6   AD 1.5 ✗   BD 4.5   BE 7   CD 5.41   AE 13 ✗   CE 11.27 ✗   DE 11.5 ✗
/// ```
///
/// Anchors at reach 2.0: 1A, 2B, 3C (each 1.7 Å below its foot). 1D is 2.27 Å
/// away: a near miss by 0.27 Å.
fn hand_worked() -> (AtomicStructure, AtomicStructure) {
    let s3 = 3f64.sqrt();
    let (ads, _) = methoxy_feet(&[
        DVec3::ZERO,
        DVec3::new(6.0, 0.0, 0.0),
        DVec3::new(3.0, 3.0 * s3, 0.0),
    ]);
    let (sub, _) = silyl_sites(
        &[
            DVec3::new(0.0, 0.0, -1.7),
            DVec3::new(6.0, 0.0, -1.7),
            DVec3::new(3.0, 3.0 * s3, -1.7),
            DVec3::new(1.5, 0.0, -1.7),
            DVec3::new(13.0, 0.0, -1.7),
        ],
        3,
    );
    (ads, sub)
}

#[test]
fn the_hand_worked_case() {
    const A: usize = 0;
    const B: usize = 1;
    const C: usize = 2;
    const D: usize = 3;
    const E: usize = 4;
    let (ads, sub) = hand_worked();
    let config = cfg(2.0, 0.0);
    let p = plan(&ads, &sub, &config).unwrap();
    let got = by_legs(&p);
    let key = |legs: &[(usize, usize)]| {
        let mut v = legs.to_vec();
        v.sort_unstable();
        v
    };

    // Leg 1: the three anchors.
    let anchors = [vec![(0, A)], vec![(1, B)], vec![(2, C)]];
    // Leg 2. From 1A: 2 and 3 on B or C (A is the site itself, D is 1.5 Å
    // from A: inside the inner bound; E 13 Å: outside). From 2B: every other
    // site passes (BA 6, BC 6, BD 4.5, BE 7). From 3C: A, B, D (CE 11.27 ✗).
    let pairs = [
        [(0, A), (1, B)],
        [(0, A), (1, C)],
        [(0, A), (2, B)],
        [(0, A), (2, C)],
        [(1, B), (0, C)],
        [(1, B), (0, D)],
        [(1, B), (0, E)],
        [(1, B), (2, A)],
        [(1, B), (2, C)],
        [(1, B), (2, D)],
        [(1, B), (2, E)],
        [(2, C), (0, B)],
        [(2, C), (0, D)],
        [(2, C), (1, A)],
        [(2, C), (1, D)],
    ];
    // Leg 3: the site triples whose three spacings all pass are {A,B,C} and
    // {B,C,D}; an assignment needs one anchored leg. The even permutations
    // of the foot triangle are proper, the odd ones mirrored.
    let triples: [([usize; 3], Mirror); 7] = [
        ([A, B, C], Mirror::Proper),
        ([A, C, B], Mirror::Mirrored),
        ([B, A, C], Mirror::Mirrored),
        ([C, B, A], Mirror::Mirrored),
        ([D, B, C], Mirror::Proper),
        ([B, D, C], Mirror::Mirrored),
        ([C, B, D], Mirror::Mirrored),
    ];
    let mut want: BTreeSet<Vec<(usize, usize)>> = BTreeSet::new();
    want.extend(anchors.iter().map(|a| key(a)));
    want.extend(pairs.iter().map(|p| key(p)));
    for (sites, verdict) in &triples {
        let legs: Vec<(usize, usize)> = (0..3).map(|f| (f, sites[f])).collect();
        let h = got
            .get(&key(&legs))
            .unwrap_or_else(|| panic!("missing {legs:?}"));
        assert_eq!(h.mirror, Some(*verdict), "{legs:?}");
        assert_eq!(h.candidate, *verdict != Mirror::Mirrored);
        want.insert(key(&legs));
    }
    let have: BTreeSet<_> = got.keys().cloned().collect();
    assert_eq!(have, want);

    let s = &p.stats;
    assert_eq!((s.anchors, s.sphere_pairs, s.torus_triples), (3, 15, 7));
    assert_eq!((s.pruned_mirror, s.mirror_undecided), (5, 0));
    // Duplicates: 2B–1A, 3C–1A, 3C–2B at leg 2; at leg 3 each triple is
    // reached once per two-leg subset that is itself a hypothesis.
    assert_eq!(s.duplicates, 3 + 9);
    assert_eq!(s.pruned_valence, 0);
    assert_eq!(s.candidates, 15 + 2);
    assert_stats_add_up(&p);
    assert_matches_oracle(&p, &config, "hand-worked");

    // The one near miss of leg 1: 1D, 0.267 Å beyond the anchor reach.
    let foot1 = p.tree.children(0)[0];
    let near = p.tree.near_misses(foot1);
    assert_eq!(near.len(), 1);
    assert_eq!((near[0].foot, near[0].site), (0, D as u32));
    assert!((near[0].miss as f64 - ((1.5f64.powi(2) + 1.7f64.powi(2)).sqrt() - 2.0)).abs() < 1e-5);

    // Filters on the same case: three legs only, or at most two.
    let p3 = plan(
        &ads,
        &sub,
        &SequentialSearch {
            formed_bonds: Some(3),
            ..config.clone()
        },
    )
    .unwrap();
    let relaxed: BTreeSet<_> = p3
        .to_relax
        .iter()
        .map(|&i| legs_of(&p3.hypotheses[i]))
        .collect();
    assert_eq!(
        relaxed,
        BTreeSet::from([
            key(&[(0, A), (1, B), (2, C)]),
            key(&[(0, D), (1, B), (2, C)])
        ])
    );
    let p2 = plan(
        &ads,
        &sub,
        &SequentialSearch {
            max_formed_bonds: Some(2),
            ..config
        },
    )
    .unwrap();
    assert_eq!(p2.stats.torus_triples, 0);
    assert_eq!(p2.stats.candidates, 15);
}

// ============================================================================
// The oracle (§11.1)
// ============================================================================

#[test]
fn plan_equals_the_oracle_on_the_tripod() {
    let (ads, slab) = tripod_over_slab();
    for tol in [0.0, 0.5] {
        let config = cfg(3.5, tol);
        let p = plan(&ads, &slab, &config).unwrap();
        assert_stats_add_up(&p);
        assert_matches_oracle(&p, &config, &format!("tripod tol {tol}"));
    }
}

#[test]
fn plan_equals_the_oracle_on_the_hexapod() {
    let slab = si100_slab(5.0, 11.0);
    let (ads, _) = posed_stand_in(6, 0.0, DVec3::ZERO);
    let config = cfg(3.5, 0.5);
    let p = plan(&ads, &slab, &config).unwrap();
    assert!(p.stats.local_phase);
    assert!(p.stats.pruned_valence > 0, "two feet compete for one site");
    assert_stats_add_up(&p);
    assert_matches_oracle(&p, &config, "hexapod");
}

#[test]
fn plan_equals_the_oracle_on_ethylene_and_water() {
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    let (eth, _) = ethanediyl((ps + pp) / 2.0 + DVec3::Z * 2.0, pp - ps);
    let config = cfg(4.5, 0.5);
    let p = plan(&eth, &slab, &config).unwrap();
    assert_stats_add_up(&p);
    assert_matches_oracle(&p, &config, "ethylene");

    let w = water(ps + DVec3::Z * 1.9, pp - ps);
    let config = SequentialSearch {
        transfers: vec![H_TO_SUBSTRATE],
        ..cfg(4.5, 0.5)
    };
    let p = plan(&w, &slab, &config).unwrap();
    assert_stats_add_up(&p);
    assert_matches_oracle(&p, &config, "water");
}

/// A small random case: 2–4 feet (some donating OH), 5–10 sites with one or
/// two free valences, random frozen atoms and random settings.
fn random_case(rng: &mut Rng) -> (AtomicStructure, AtomicStructure, SequentialSearch) {
    let mut ads = AtomicStructure::new();
    let mut donors = false;
    for _ in 0..2 + rng.below(3) {
        let p = DVec3::new(
            rng.range(-4.0, 4.0),
            rng.range(-4.0, 4.0),
            rng.range(0.0, 0.6),
        );
        let o = if rng.below(3) == 0 {
            donors = true;
            let dir = DVec3::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0), -0.4);
            add_methanol(&mut ads, p, dir).0
        } else {
            add_methoxy(&mut ads, p)
        };
        if rng.below(6) == 0 {
            ads.set_atom_frozen(o, true);
        }
    }
    let mut sub = AtomicStructure::new();
    for _ in 0..5 + rng.below(6) {
        let p = DVec3::new(
            rng.range(-6.0, 6.0),
            rng.range(-6.0, 6.0),
            rng.range(-2.4, -1.2),
        );
        let si = add_silyl(&mut sub, p, 2 + usize::from(rng.below(3) != 0));
        if rng.below(4) == 0 {
            sub.set_atom_frozen(si, true);
        }
    }
    let config = SequentialSearch {
        anchor_reach: rng.range(1.8, 4.0),
        tolerance: rng.range(0.0, 1.2),
        reach: rng.range(1.5, 4.5),
        clash_filter: rng.below(2) == 0,
        transfers: if donors {
            vec![H_TO_SUBSTRATE]
        } else {
            Vec::new()
        },
        formed_bonds: [None, None, Some(2), Some(3)][rng.below(4)],
        max_formed_bonds: [None, None, Some(2)][rng.below(3)],
        ..Default::default()
    };
    (ads, sub, config)
}

#[test]
fn plan_equals_the_oracle_on_random_cases() {
    let mut inventories = 0;
    for seed in 0..300 {
        let mut rng = Rng::new(seed);
        let (ads, sub, mut config) = random_case(&mut rng);
        let p = plan(&ads, &sub, &config).unwrap();
        let what = format!("seed {seed}");
        assert_stats_add_up(&p);
        assert_matches_oracle(&p, &config, &what);
        // Once more with an inventory filter taken from what was found.
        let found: BTreeSet<BondInventory> =
            p.hypotheses.iter().map(|h| h.inventory.clone()).collect();
        if let Some(inv) = found.iter().nth(rng.below(found.len().max(1))) {
            config.bond_inventory = Some(inv.clone());
            config.formed_bonds = None;
            config.max_formed_bonds = None;
            let p = plan(&ads, &sub, &config).unwrap();
            assert_stats_add_up(&p);
            assert_matches_oracle(&p, &config, &format!("{what} + inventory {inv}"));
            assert!(
                p.hypotheses
                    .iter()
                    .filter(|h| h.candidate)
                    .all(|h| &h.inventory == inv)
            );
            inventories += 1;
        }
    }
    assert!(inventories > 100);
}

// ============================================================================
// Planted bindings (§11.2)
// ============================================================================

/// Plants a binding: the tripod at a random pose, a random bond direction
/// per foot, and a silyl site at `Fᵢ − bᵢ·dᵢ` for each, its dangling bond
/// along `dᵢ`. Returns the molecule, the substrate and the planted legs.
fn plant(
    rng: &mut Rng,
    directions: impl Fn(&mut Rng, usize) -> DVec3,
    feet: usize,
) -> (AtomicStructure, AtomicStructure, Vec<(u32, u32)>) {
    let (mut ads, foot_ids) = stand_in(3);
    let q = rng.rotation();
    let t = DVec3::new(
        rng.range(-5.0, 5.0),
        rng.range(-5.0, 5.0),
        rng.range(-5.0, 5.0),
    );
    ads.transform(&q, &t);
    let b = rest_length(O, SI);
    let mut sub = AtomicStructure::new();
    let mut planted = Vec::new();
    for (i, &f) in foot_ids.iter().take(feet).enumerate() {
        let d = directions(rng, i);
        let site = ads.get_atom(f).unwrap().position - d * b;
        planted.push((f, add_silyl_along(&mut sub, site, d, 3)));
    }
    planted.sort_unstable();
    (ads, sub, planted)
}

#[test]
fn every_planted_binding_is_found_at_tolerance_0() {
    for seed in 0..200 {
        let mut rng = Rng::new(1000 + seed);
        let feet = 2 + (seed as usize % 2);
        // Any direction at all, including bonds tilted against each other.
        let (ads, sub, planted) = plant(&mut rng, |r, _| r.unit(), feet);
        let p = plan(&ads, &sub, &cfg(1.8, 0.0)).unwrap();
        assert!(
            plan_bond_sets(&p).contains(&planted),
            "seed {seed}: planted {planted:?} not among {} hypotheses",
            p.hypotheses.len()
        );
    }
}

/// With every bond along the body-side normal of the foot plane, the local up
/// is that normal, the seating puts each foot exactly on its seating point
/// (a proper rotation) and the planted binding is a proper candidate.
#[test]
fn a_planted_binding_along_the_body_normal_is_proper_and_exact() {
    for seed in 0..50 {
        let mut rng = Rng::new(3000 + seed);
        let (mut ads, foot_ids) = stand_in(3);
        let q = rng.rotation();
        let t = DVec3::new(
            rng.range(-5.0, 5.0),
            rng.range(-5.0, 5.0),
            rng.range(-5.0, 5.0),
        );
        ads.transform(&q, &t);
        let normal = q * DVec3::Z;
        let b = rest_length(O, SI);
        let mut sub = AtomicStructure::new();
        let mut planted = Vec::new();
        for &f in &foot_ids {
            let site = ads.get_atom(f).unwrap().position - normal * b;
            planted.push((f, add_silyl_along(&mut sub, site, normal, 3)));
        }
        planted.sort_unstable();
        let p = plan(&ads, &sub, &cfg(1.8, 0.0)).unwrap();
        let (back_a, back_s) = back_maps(&p.setup);
        let h = p
            .hypotheses
            .iter()
            .find(|h| {
                let mut v: Vec<(u32, u32)> = h
                    .formed
                    .iter()
                    .map(|(a, s)| (back_a[a], back_s[s]))
                    .collect();
                v.sort_unstable();
                v == planted
            })
            .unwrap_or_else(|| panic!("seed {seed}: planted binding missing"));
        assert_eq!(h.mirror, Some(Mirror::Proper), "seed {seed}");
        assert!(h.candidate);
        let seating = h.seating.as_ref().unwrap();
        assert!(
            (seating.up - normal).length() < 1e-9,
            "seed {seed}: up {}",
            seating.up
        );
        assert!(
            seating.residual < 1e-6,
            "seed {seed}: residual {}",
            seating.residual
        );
        assert!((seating.rotation.determinant() - 1.0).abs() < 1e-9);
        // The seating moves nothing: the pose already is the binding.
        for (i, &x) in p.setup.posed().iter().enumerate() {
            assert!(
                seating.apply(x).distance(x) < 1e-6,
                "seed {seed}: atom {i} moved"
            );
        }
    }
}

/// Feet of two elements (an O radical and a C radical, so `b₁ ≠ b₂`), their
/// bonds pointing opposite ways: the site spacing then reaches the shell's
/// bound `d ± (b₁ + b₂)`, and only the right slack keeps it.
#[test]
fn planted_mixed_element_feet_are_found_at_the_bound() {
    let (b_o, b_c) = (rest_length(O, SI), rest_length(C, SI));
    assert!((b_o - b_c).abs() > 0.1);
    for seed in 0..60 {
        let mut rng = Rng::new(4500 + seed);
        let mut ads = AtomicStructure::new();
        let o = add_methoxy(&mut ads, DVec3::ZERO);
        // A methyl radical 4 Å away: •CH3, its H above.
        let cp = DVec3::new(4.0, rng.range(-1.0, 1.0), rng.range(-0.5, 0.5));
        let c = ads.add_atom(C, cp);
        for d in tetrahedral_below(-DVec3::Z) {
            let h = ads.add_atom(H, cp + d * 1.09);
            ads.add_bond(c, h, BOND_SINGLE);
        }
        let axis = (cp - DVec3::ZERO).normalize();
        // Bonds along ±axis (the extremes) or random.
        let (d_o, d_c) = match seed % 3 {
            0 => (axis, -axis),
            1 => (-axis, axis),
            _ => (rng.unit(), rng.unit()),
        };
        let mut sub = AtomicStructure::new();
        let so = add_silyl_along(&mut sub, -d_o * b_o, d_o, 3);
        let sc = add_silyl_along(&mut sub, cp - d_c * b_c, d_c, 3);
        // An anchor reach between b(O–Si) and b(C–Si): only the O foot can
        // be leg 1, so the pair is tested in one order only, and a slack
        // built from the wrong foot's bond cannot hide behind the other.
        let p = plan(&ads, &sub, &cfg(0.5 * (b_o + b_c), 0.0)).unwrap();
        let mut want = vec![(o, so), (c, sc)];
        want.sort_unstable();
        assert!(plan_bond_sets(&p).contains(&want), "seed {seed}");
    }
}

#[test]
fn degenerate_shapes_are_planted_and_found() {
    let b_c = rest_length(C, SI);
    for seed in 0..40 {
        let mut rng = Rng::new(4000 + seed);
        // Ethanediyl: the feet 1.54 Å apart, well inside b₁ + b₂ = 3.7 Å.
        let (eth, cs) = ethanediyl(DVec3::ZERO, rng.unit());
        let mut sub = AtomicStructure::new();
        let mut planted = Vec::new();
        for &c in &cs {
            let d = rng.unit();
            let site = eth.get_atom(c).unwrap().position - d * b_c;
            planted.push((c, add_silyl_along(&mut sub, site, d, 3)));
        }
        planted.sort_unstable();
        let p = plan(&eth, &sub, &cfg(2.0, 0.0)).unwrap();
        assert!(
            plan_bond_sets(&p).contains(&planted),
            "seed {seed}: ethanediyl"
        );

        // One SiH2 site taking both carbons: on the C–C bisector, a bond
        // length from each.
        let (c0, c1) = (
            eth.get_atom(cs[0]).unwrap().position,
            eth.get_atom(cs[1]).unwrap().position,
        );
        let axis = (c1 - c0).normalize();
        let perp = axis.any_orthonormal_vector();
        let off = (b_c * b_c - (c0.distance(c1) / 2.0).powi(2)).sqrt();
        let mut sub = AtomicStructure::new();
        let si = add_silyl_along(&mut sub, (c0 + c1) / 2.0 - perp * off, perp, 2);
        let p = plan(&eth, &sub, &cfg(2.0, 0.0)).unwrap();
        let mut want = vec![(cs[0], si), (cs[1], si)];
        want.sort_unstable();
        assert!(
            plan_bond_sets(&p).contains(&want),
            "seed {seed}: two feet on one site"
        );
    }

    // A third foot beyond the F₁F₂ segment, and nearly collinear feet.
    let b = rest_length(O, SI);
    for (name, third) in [
        ("beyond", DVec3::new(7.0, 0.8, 0.0)),
        ("collinear", DVec3::new(6.0, 0.2, 0.0)),
    ] {
        for seed in 0..40 {
            let mut rng = Rng::new(5000 + seed);
            let (ads, ids) = methoxy_feet(&[DVec3::ZERO, DVec3::new(3.0, 0.0, 0.0), third]);
            let mut sub = AtomicStructure::new();
            let mut planted = Vec::new();
            for &f in &ids {
                let d = DVec3::new(rng.range(-0.4, 0.4), rng.range(-0.4, 0.4), 1.0).normalize();
                let site = ads.get_atom(f).unwrap().position - d * b;
                planted.push((f, add_silyl_along(&mut sub, site, d, 3)));
            }
            planted.sort_unstable();
            let p = plan(&ads, &sub, &cfg(1.8, 0.0)).unwrap();
            assert!(plan_bond_sets(&p).contains(&planted), "{name} seed {seed}");
            if name == "collinear" {
                let (back_a, _) = back_maps(&p.setup);
                let h = p.hypotheses.iter().find(|h| {
                    h.legs() == 3 && h.formed.iter().all(|(a, _)| ids.contains(&back_a[a]))
                });
                assert_eq!(h.unwrap().mirror, Some(Mirror::Undecided), "seed {seed}");
            }
        }
    }
}

// ============================================================================
// Metamorphic tests (§11.4), plan level
// ============================================================================

fn stats_without_time(p: &SequentialPlan) -> PlanStats {
    PlanStats {
        seconds: 0.0,
        ..p.stats.clone()
    }
}

fn verdicts(p: &SequentialPlan) -> BTreeMap<InputChange, (Option<Mirror>, bool)> {
    p.hypotheses
        .iter()
        .map(|h| (input_change(&p.setup, &h.key()), (h.mirror, h.candidate)))
        .collect()
}

#[test]
fn a_rigid_motion_of_the_whole_input_changes_nothing() {
    let (ads, slab) = tripod_over_slab();
    let config = SequentialSearch {
        clash_filter: true,
        ..cfg(3.5, 0.5)
    };
    let base = plan(&ads, &slab, &config).unwrap();
    for seed in 0..3 {
        let mut rng = Rng::new(6000 + seed);
        let (q, t) = (
            rng.rotation(),
            DVec3::new(rng.range(-20.0, 20.0), 3.0, rng.range(-9.0, 9.0)),
        );
        let p = plan(&moved(&ads, q, t), &moved(&slab, q, t), &config).unwrap();
        assert_eq!(verdicts(&p), verdicts(&base), "seed {seed}");
        assert_eq!(
            stats_without_time(&p),
            stats_without_time(&base),
            "seed {seed}"
        );
    }
}

#[test]
fn relabelling_the_atoms_changes_nothing_but_the_ids() {
    let (ads, slab) = tripod_over_slab();
    let config = SequentialSearch {
        clash_filter: true,
        ..cfg(3.5, 0.5)
    };
    let base = plan(&ads, &slab, &config).unwrap();
    let mut rng = Rng::new(7);
    let shuffle = |s: &AtomicStructure, rng: &mut Rng| {
        let mut ids: Vec<u32> = s.atom_ids().copied().collect();
        ids.sort_unstable();
        for i in (1..ids.len()).rev() {
            ids.swap(i, rng.below(i + 1));
        }
        relabelled(s, &ids)
    };
    let (ads2, map_a) = shuffle(&ads, &mut rng);
    let (slab2, map_s) = shuffle(&slab, &mut rng);
    let p = plan(&ads2, &slab2, &config).unwrap();
    let mapped: BTreeMap<InputChange, (Option<Mirror>, bool)> = verdicts(&base)
        .into_iter()
        .map(|((formed, moves), v)| {
            let mut f: Vec<(u32, u32)> = formed.iter().map(|(a, s)| (map_a[a], map_s[s])).collect();
            f.sort_unstable();
            assert!(moves.is_empty());
            ((f, moves), v)
        })
        .collect();
    assert_eq!(verdicts(&p), mapped);
    assert_eq!(stats_without_time(&p), stats_without_time(&base));
}

/// Seating reads the set of legs, not the order they were bound in: bit for
/// bit the same transform for every permutation.
#[test]
fn the_seating_of_a_bond_set_ignores_the_binding_order() {
    let (ads, slab) = tripod_over_slab();
    let p = plan(&ads, &slab, &cfg(3.5, 0.5)).unwrap();
    let mut checked = 0;
    for h in p.hypotheses.iter().filter(|h| h.candidate && h.legs() >= 2) {
        let want = h.seating.as_ref().unwrap();
        let mut steps = h.steps.clone();
        steps.reverse();
        let got = p.setup.seat(&steps);
        assert_eq!(got.rotation, want.rotation);
        assert_eq!(got.translation, want.translation);
        assert_eq!(got.clashes, want.clashes);
        if steps.len() == 3 {
            steps.swap(0, 1);
            assert_eq!(p.setup.seat(&steps).rotation, want.rotation);
        }
        checked += 1;
    }
    assert!(checked > 500);
}

/// The same bond set seated from two different poses lands in the same place
/// (to rounding): seating leaks no pose information (two and three legs).
#[test]
fn the_seating_of_a_bond_set_ignores_the_pose() {
    let slab = si100_slab(5.0, 11.0);
    let (a0, _) = posed_stand_in(3, 0.0, DVec3::ZERO);
    let (a1, _) = posed_stand_in(3, 7.0, DVec3::new(0.4, -0.3, 0.2));
    let config = cfg(3.5, 0.5);
    let (p0, p1) = (
        plan(&a0, &slab, &config).unwrap(),
        plan(&a1, &slab, &config).unwrap(),
    );
    let seated = |p: &SequentialPlan, h: &Hypothesis| -> Vec<DVec3> {
        let s = h.seating.as_ref().unwrap();
        p.setup.posed().iter().map(|&x| s.apply(x)).collect()
    };
    let other: BTreeMap<InputChange, &Hypothesis> = p1
        .hypotheses
        .iter()
        .map(|h| (input_change(&p1.setup, &h.key()), h))
        .collect();
    let mut shared = 0;
    for h in p0
        .hypotheses
        .iter()
        .filter(|h| h.candidate && h.legs() >= 2)
    {
        let Some(g) = other.get(&input_change(&p0.setup, &h.key())) else {
            continue;
        };
        if !g.candidate {
            continue;
        }
        let (x, y) = (seated(&p0, h), seated(&p1, g));
        let worst = x
            .iter()
            .zip(&y)
            .map(|(a, b)| a.distance(*b))
            .fold(0.0, f64::max);
        assert!(worst < 1e-6, "{:?}: seatings differ by {worst}", legs_of(h));
        shared += 1;
    }
    assert!(shared > 100, "only {shared} shared bond sets");
}

#[test]
fn raising_a_reach_or_the_tolerance_never_removes_a_hypothesis() {
    let (ads, slab) = tripod_over_slab();
    let changes = |c: SequentialSearch| plan_changes(&plan(&ads, &slab, &c).unwrap());
    let mut last = BTreeSet::new();
    for tol in [0.0, 0.3, 0.5, 1.0] {
        let now = changes(cfg(3.5, tol));
        assert!(last.is_subset(&now), "tolerance {tol}");
        last = now;
    }
    let mut last = BTreeSet::new();
    for reach in [2.0, 3.5, 4.5] {
        let now = changes(cfg(reach, 0.5));
        assert!(last.is_subset(&now), "anchor reach {reach}");
        last = now;
    }
    // reach is the transfer rule's: water over the slab.
    let slab9 = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab9);
    let (ps, pp) = (
        slab9.get_atom(site).unwrap().position,
        slab9.get_atom(partner).unwrap().position,
    );
    let w = water(ps + DVec3::Z * 1.9, pp - ps);
    let mut last = BTreeSet::new();
    for reach in [1.0, 2.5, 3.0, 4.0] {
        let c = SequentialSearch {
            transfers: vec![H_TO_SUBSTRATE],
            reach,
            ..cfg(4.5, 0.5)
        };
        let now = plan_changes(&plan(&w, &slab9, &c).unwrap());
        assert!(last.is_subset(&now), "reach {reach}");
        last = now;
    }
    assert!(!last.is_empty());
}

/// A row's near misses are exactly what one more ångström of tolerance (or
/// of anchor reach) adds to its children.
#[test]
fn near_misses_are_what_one_more_angstrom_adds() {
    let (ads, slab) = tripod_over_slab();
    let children = |p: &SequentialPlan, row: u32| -> BTreeSet<(u32, u32)> {
        p.tree
            .children(row)
            .iter()
            .map(|&c| match p.tree.row(c).kind {
                RowKind::Leg { foot, site, .. } => (foot, site),
                _ => unreachable!(),
            })
            .collect()
    };
    let rows_by_path = |p: &SequentialPlan| -> BTreeMap<Vec<Leg>, u32> {
        (0..p.tree.len() as u32)
            .filter(|&r| {
                p.tree.row(r).duplicate_of.is_none() && !matches!(p.tree.row(r).kind, RowKind::Root)
            })
            .map(|r| {
                let mut key = p.tree.path(r);
                if let RowKind::Foot(f) = p.tree.row(r).kind {
                    key = vec![Leg {
                        foot: f as usize,
                        site: usize::MAX,
                    }];
                }
                (key, r)
            })
            .collect()
    };
    for (low, high) in [
        (cfg(3.5, 0.0), cfg(3.5, 1.0)),
        (cfg(2.5, 0.5), cfg(3.5, 0.5)),
    ] {
        let (pl, ph) = (
            plan(&ads, &slab, &low).unwrap(),
            plan(&ads, &slab, &high).unwrap(),
        );
        assert_eq!(
            pl.stats.pruned_valence, 0,
            "no valence contention on this fixture"
        );
        let high_rows = rows_by_path(&ph);
        let mut checked = 0;
        for (path, r) in rows_by_path(&pl) {
            let legs = pl.tree.row(r).legs;
            // Leg-1 rows when the anchor reach moved; leg-1 and leg-2 rows when the tolerance did.
            let moved_test = if low.anchor_reach != high.anchor_reach {
                legs == 0
            } else {
                legs == 1 || legs == 2
            };
            if !moved_test {
                continue;
            }
            let mut want = children(&pl, r);
            want.extend(pl.tree.near_misses(r).iter().map(|n| (n.foot, n.site)));
            assert_eq!(children(&ph, high_rows[&path]), want, "{path:?}");
            checked += 1;
        }
        assert!(checked > 0);
    }
}

/// An H on a site removes exactly the hypotheses that bond to it.
#[test]
fn blocking_a_site_removes_exactly_its_hypotheses() {
    let (ads, slab) = tripod_over_slab();
    let config = cfg(3.5, 0.5);
    let base = plan(&ads, &slab, &config).unwrap();
    let (site, _) = central_dimer(&slab);
    let mut blocked = slab.clone();
    let p = blocked.get_atom(site).unwrap().position;
    let h = blocked.add_atom(H, p + DVec3::Z * 1.48);
    blocked.add_bond(site, h, BOND_SINGLE);
    let after = plan(&ads, &blocked, &config).unwrap();
    let want: BTreeSet<_> = plan_changes(&base)
        .into_iter()
        .filter(|(formed, _)| formed.iter().all(|&(_, s)| s != site))
        .collect();
    assert!(want.len() < base.hypotheses.len());
    assert_eq!(plan_changes(&after), want);
}

/// Water's H goes to the nearest free site; blocking that one sends it to the
/// next nearest, and blocking every site within reach drops the leg.
#[test]
fn blocking_an_acceptor_moves_the_hydrogen_or_drops_the_leg() {
    let mut ads = AtomicStructure::new();
    add_tagged_methanol(
        &mut ads,
        DVec3::new(0.0, 0.0, 1.8),
        DVec3::new(1.0, 0.0, -0.3),
    );
    let positions = [
        DVec3::ZERO,
        DVec3::new(2.4, 0.0, 0.0),
        DVec3::new(0.0, 2.6, 0.0),
    ];
    let config = oh_feet(3.0);
    let acceptor = |hs: [usize; 3]| {
        let mut sub = AtomicStructure::new();
        let ids: Vec<u32> = positions
            .iter()
            .zip(hs)
            .map(|(&p, h)| add_silyl(&mut sub, p, h))
            .collect();
        let p = plan(&ads, &sub, &config).unwrap();
        let mut seen = Vec::new();
        for h in &p.hypotheses {
            let (_, back) = back_maps(&p.setup);
            seen.push(back[&h.transfers[0].acceptor]);
        }
        (ids, seen, p.stats.pruned_no_acceptor)
    };
    let (ids, seen, dropped) = acceptor([3, 3, 3]);
    assert_eq!((seen, dropped), (vec![ids[1]], 0), "nearest");
    let (ids, seen, _) = acceptor([3, 4, 3]);
    assert_eq!(
        seen,
        vec![ids[2]],
        "next nearest once the nearest is capped"
    );
    let (_, seen, dropped) = acceptor([3, 4, 4]);
    assert_eq!((seen.len(), dropped), (0, 1), "no acceptor within reach");
}

#[test]
fn the_filters_plan_exactly_the_matching_candidates() {
    let (ads, slab) = tripod_over_slab();
    let base = plan(&ads, &slab, &cfg(3.5, 0.5)).unwrap();
    let candidates = |p: &SequentialPlan| -> BTreeSet<InputChange> {
        p.hypotheses
            .iter()
            .filter(|h| h.candidate)
            .map(|h| input_change(&p.setup, &h.key()))
            .collect()
    };
    for legs in [2, 3] {
        let p = plan(
            &ads,
            &slab,
            &SequentialSearch {
                formed_bonds: Some(legs),
                ..cfg(3.5, 0.5)
            },
        )
        .unwrap();
        let want: BTreeSet<_> = base
            .hypotheses
            .iter()
            .filter(|h| h.candidate && h.legs() == legs)
            .map(|h| input_change(&base.setup, &h.key()))
            .collect();
        assert_eq!(candidates(&p), want, "{legs} legs");
        assert!(
            p.hypotheses
                .iter()
                .filter(|h| h.legs() < legs)
                .all(|h| !h.candidate)
        );
    }
    let inv: BondInventory = "formed 3× O–Si".parse().unwrap();
    let p = plan(
        &ads,
        &slab,
        &SequentialSearch {
            bond_inventory: Some(inv.clone()),
            ..cfg(3.5, 0.5)
        },
    )
    .unwrap();
    let want: BTreeSet<_> = base
        .hypotheses
        .iter()
        .filter(|h| h.candidate && h.inventory == inv)
        .map(|h| input_change(&base.setup, &h.key()))
        .collect();
    assert_eq!(candidates(&p), want);
}

#[test]
fn the_budget_truncates_a_prefix_and_says_so() {
    let (ads, slab) = tripod_over_slab();
    let full = plan(&ads, &slab, &cfg(3.5, 0.5)).unwrap();
    let p = plan(
        &ads,
        &slab,
        &SequentialSearch {
            budget: 100,
            ..cfg(3.5, 0.5)
        },
    )
    .unwrap();
    assert!(p.stats.truncated && !full.stats.truncated);
    assert_eq!(p.to_relax.len(), 100);
    let keys = |p: &SequentialPlan| -> Vec<_> {
        p.to_relax.iter().map(|&i| p.hypotheses[i].key()).collect()
    };
    assert_eq!(keys(&p)[..], keys(&full)[..100]);
}

#[test]
fn plan_is_deterministic() {
    let (ads, slab) = tripod_over_slab();
    let (a, b) = (
        plan(&ads, &slab, &cfg(3.5, 0.5)).unwrap(),
        plan(&ads, &slab, &cfg(3.5, 0.5)).unwrap(),
    );
    assert_eq!(a.hypotheses, b.hypotheses);
    assert_eq!(a.tree, b.tree);
    assert_eq!(stats_without_time(&a), stats_without_time(&b));
}

// ============================================================================
// Rules (§11.6)
// ============================================================================

/// A clash is two heavy atoms closer than 0.6 × their covalent radii's sum
/// (O–Si ≈ 1.06 Å), not an ordinary close contact; the bonded foot, its
/// neighbours within two bonds, its site and every H are ignored.
#[test]
fn clashes_catch_atoms_through_each_other_and_nothing_else() {
    let clashes = |o2_to_x: f64, with_h: bool, y_offset: f64| {
        // Foot O1 bonded to site S; a second methoxy O2 `o2_to_x` above a
        // lone heavy atom X; a heavy atom Y next to the foot's own methyl C.
        let (ads, _) = methoxy_feet(&[DVec3::new(0.0, 0.0, 1.7), DVec3::new(6.0, 0.0, o2_to_x)]);
        let mut sub = AtomicStructure::new();
        add_silyl(&mut sub, DVec3::ZERO, 3);
        let x = sub.add_atom(SI, DVec3::new(6.0, 0.0, 0.0));
        if with_h {
            let h = sub.add_atom(H, DVec3::new(6.0, 0.0, o2_to_x - 0.2));
            sub.add_bond(x, h, BOND_SINGLE);
        }
        sub.add_atom(SI, DVec3::new(0.0, y_offset, 1.7 + 1.43));
        let p = plan(&ads, &sub, &cfg(2.0, 0.0)).unwrap();
        p.setup
            .clashes(&[Leg { foot: 0, site: 0 }], DMat3::IDENTITY, DVec3::ZERO)
            .len()
    };
    assert_eq!(clashes(1.3, false, 5.0), 0, "a close contact");
    assert_eq!(clashes(0.9, false, 5.0), 1, "atoms through each other");
    assert_eq!(
        clashes(0.9, true, 5.0),
        1,
        "an H in between changes nothing"
    );
    assert_eq!(
        clashes(1.3, false, 0.3),
        0,
        "the foot's own methyl is ignored"
    );
    assert_eq!(clashes(0.9, false, 0.3), 1, "…but nothing else near it");

    // A real seating turned upside down, about a horizontal axis through its
    // sites, sinks the body into the slab and is caught.
    let (ads, slab) = tripod_over_slab();
    let p = plan(
        &ads,
        &slab,
        &SequentialSearch {
            formed_bonds: Some(3),
            ..cfg(3.5, 0.5)
        },
    )
    .unwrap();
    let setup = &p.setup;
    let h = p
        .hypotheses
        .iter()
        .find(|h| {
            h.candidate
                && !h.seating.as_ref().unwrap().clashes()
                && h.steps
                    .iter()
                    .all(|x| setup.sites[x.leg.site].position.z.abs() < 0.5)
        })
        .expect("a clash-free three-leg seating on the top face");
    let s = h.seating.as_ref().unwrap();
    let legs: Vec<Leg> = h.steps.iter().map(|x| x.leg).collect();
    let centre = legs
        .iter()
        .map(|l| setup.sites[l.site].position)
        .sum::<DVec3>()
        / 3.0;
    let flip = DMat3::from_axis_angle(s.up.any_orthonormal_vector(), std::f64::consts::PI);
    let (rotation, translation) = (
        flip * s.rotation,
        centre - flip * centre + flip * s.translation,
    );
    let flipped = setup.clashes(&legs, rotation, translation);
    assert!(!flipped.is_empty());
    for (a, _) in flipped {
        assert!(legs.iter().all(|l| !setup.feet[l.foot].near.contains(&a)));
        assert_ne!(setup.combined.get_atom(a).unwrap().atomic_number, H);
    }
}

#[test]
fn the_clash_filter_prunes_exactly_the_clashing_candidates() {
    let (ads, slab) = tripod_over_slab();
    let off = plan(&ads, &slab, &cfg(3.5, 0.5)).unwrap();
    let on = plan(
        &ads,
        &slab,
        &SequentialSearch {
            clash_filter: true,
            ..cfg(3.5, 0.5)
        },
    )
    .unwrap();
    assert_eq!(off.stats.pruned_clash, 0);
    assert_eq!(off.stats.to_relax, off.stats.candidates);
    assert!(off.stats.seating_clashes > 0);
    assert_eq!(on.stats.seating_clashes, off.stats.seating_clashes);
    assert_eq!(on.stats.pruned_clash, on.stats.seating_clashes);
    let kept: BTreeSet<_> = on
        .to_relax
        .iter()
        .map(|&i| on.hypotheses[i].key())
        .collect();
    let want: BTreeSet<_> = off
        .to_relax
        .iter()
        .map(|&i| &off.hypotheses[i])
        .filter(|h| !h.seating.as_ref().unwrap().clashes())
        .map(|h| h.key())
        .collect();
    assert_eq!(kept, want);
}

/// Every mirror-pruned hypothesis of the fixtures would also be caught by the
/// clash check: the mirror check removes nothing the clash filter keeps.
#[test]
fn mirror_pruned_hypotheses_also_clash() {
    let slab = si100_slab(5.0, 11.0);
    for legs in [3, 6] {
        let (ads, _) = posed_stand_in(legs, 0.0, DVec3::ZERO);
        let p = plan(&ads, &slab, &cfg(3.5, 0.5)).unwrap();
        let mut n = 0;
        for h in p
            .hypotheses
            .iter()
            .filter(|h| h.mirror == Some(Mirror::Mirrored))
        {
            assert!(
                p.setup.seat(&h.steps).clashes(),
                "{legs} feet: {:?}",
                legs_of(h)
            );
            n += 1;
        }
        assert!(n > 100);
    }
}

/// The planted proper binding with two feet's sites swapped is its mirror
/// image; the check calls each what it is, and turning the local up towards
/// the surface never flips a verdict — past the margin it abstains.
#[test]
fn the_mirror_check_tells_a_binding_from_its_mirror_image() {
    let (ads, foot_ids) = stand_in(3);
    let b = rest_length(O, SI);
    let mut sub = AtomicStructure::new();
    for &f in &foot_ids {
        add_silyl(
            &mut sub,
            ads.get_atom(f).unwrap().position - DVec3::Z * b,
            3,
        );
    }
    let p = plan(&ads, &sub, &cfg(1.8, 0.0)).unwrap();
    let setup = &p.setup;
    let proper = [0, 1, 2].map(|i| Leg { foot: i, site: i });
    let mirrored = [
        Leg { foot: 0, site: 1 },
        Leg { foot: 1, site: 0 },
        Leg { foot: 2, site: 2 },
    ];
    let up = setup.local_up(&[0, 1, 2]);
    assert!((up - DVec3::Z).length() < 1e-9);
    assert_eq!(setup.mirror(proper, up), Mirror::Proper);
    assert_eq!(setup.mirror(mirrored, up), Mirror::Mirrored);
    for deg in 0..90 {
        let tilted = DQuat::from_rotation_x((deg as f64).to_radians()) * DVec3::Z;
        for (legs, verdict) in [(proper, Mirror::Proper), (mirrored, Mirror::Mirrored)] {
            let got = setup.mirror(legs, tilted);
            if deg < 75 {
                assert_eq!(got, verdict, "{deg}°");
            } else {
                assert!(
                    got == verdict || got == Mirror::Undecided,
                    "{deg}°: {got:?}"
                );
            }
        }
    }
}

#[test]
fn the_mirror_check_abstains_where_it_cannot_tell() {
    let b = rest_length(O, SI);
    let sites_under = |ads: &AtomicStructure, ids: &[u32]| {
        let mut sub = AtomicStructure::new();
        for &f in ids {
            add_silyl(
                &mut sub,
                ads.get_atom(f).unwrap().position - DVec3::Z * b,
                3,
            );
        }
        sub
    };
    let three = [0, 1, 2].map(|i| Leg { foot: i, site: i });

    // A body in its own foot plane: three methoxys whose methyls are turned
    // sideways, into the plane of the feet.
    let mut flat = AtomicStructure::new();
    let mut ids = Vec::new();
    for (i, p) in [
        DVec3::ZERO,
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(2.5, 4.3, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        let o = flat.add_atom(O, p);
        let out = (p - DVec3::new(2.5, 1.43, 0.0)).normalize();
        let c = flat.add_atom(C, p + out * 1.43);
        flat.add_bond(o, c, BOND_SINGLE);
        for d in tetrahedral_below(-out) {
            let h = flat.add_atom(H, p + out * 1.43 + d * 1.09);
            flat.add_bond(c, h, BOND_SINGLE);
        }
        let _ = i;
        ids.push(o);
    }
    let p = plan(&flat, &sites_under(&flat, &ids), &cfg(1.8, 0.0)).unwrap();
    assert_eq!(
        p.setup.mirror(three, DVec3::Z),
        Mirror::Undecided,
        "flat molecule"
    );

    // A site triangle standing steep against the local up: sites on two
    // terraces 4 Å apart.
    let (ads, ids) = methoxy_feet(&[
        DVec3::ZERO,
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(2.5, 4.3, 0.0),
    ]);
    let mut sub = AtomicStructure::new();
    for (k, &f) in ids.iter().enumerate() {
        let drop = if k == 2 { 4.0 * 4.3 } else { 0.0 };
        add_silyl(
            &mut sub,
            ads.get_atom(f).unwrap().position - DVec3::Z * (b + drop),
            3,
        );
    }
    let p = plan(&ads, &sub, &cfg(1.8, 0.0)).unwrap();
    assert_eq!(
        p.setup.mirror(three, DVec3::Z),
        Mirror::Undecided,
        "steep sites"
    );

    // Nearly collinear feet (covered by the degenerate plants too).
    let (ads, ids) = methoxy_feet(&[
        DVec3::ZERO,
        DVec3::new(3.0, 0.0, 0.0),
        DVec3::new(6.0, 0.3, 0.0),
    ]);
    let p = plan(&ads, &sites_under(&ads, &ids), &cfg(1.8, 0.0)).unwrap();
    assert_eq!(
        p.setup.mirror(three, DVec3::Z),
        Mirror::Undecided,
        "collinear feet"
    );
    // …and undecided hypotheses are candidates, relaxed as if unchecked.
    let h = &p
        .hypotheses
        .iter()
        .find(|h| h.legs() == 3 && legs_of(h) == vec![(0, 0), (1, 1), (2, 2)])
        .unwrap();
    assert!(h.candidate);
    assert_eq!(
        p.stats.mirror_undecided,
        p.hypotheses
            .iter()
            .filter(|h| h.mirror == Some(Mirror::Undecided))
            .count()
    );
}

/// Methanol feet over silyls in a row: the rule's choices, by hand.
#[test]
fn the_transfer_rule_takes_the_nearest_free_site() {
    let config = oh_feet(3.0);
    let mut ads = AtomicStructure::new();
    let (o, h) = add_tagged_methanol(
        &mut ads,
        DVec3::new(0.0, 0.0, 1.8),
        DVec3::new(1.0, 0.0, -0.3),
    );

    // Sites at x = 0, 2.4, −2.4: a tie, to the lower id (the first added).
    let (sub, s) = silyl_sites(
        &[
            DVec3::ZERO,
            DVec3::new(2.4, 0.0, 0.0),
            DVec3::new(-2.4, 0.0, 0.0),
        ],
        3,
    );
    let p = plan(&ads, &sub, &config).unwrap();
    assert_eq!(p.hypotheses.len(), 1);
    let (back_a, back_s) = back_maps(&p.setup);
    let t = p.hypotheses[0].transfers[0];
    assert_eq!(
        (back_a[&t.donor], back_a[&t.moved], back_s[&t.acceptor]),
        (o, h, s[1])
    );
    assert_eq!(
        p.hypotheses[0].inventory.to_string(),
        "formed 1× H–Si, 1× O–Si; broken 1× H–O"
    );

    // A two-valence site under the foot never takes its own foot's H.
    let mut sub = AtomicStructure::new();
    let own = add_silyl(&mut sub, DVec3::ZERO, 2);
    let other = add_silyl(&mut sub, DVec3::new(2.9, 0.0, 0.0), 3);
    let p = plan(&ads, &sub, &config).unwrap();
    let (_, back_s) = back_maps(&p.setup);
    let onto: Vec<(u32, u32)> = p
        .hypotheses
        .iter()
        .map(|h| (back_s[&h.formed[0].1], back_s[&h.transfers[0].acceptor]))
        .collect();
    assert_eq!(onto, vec![(own, other)]);

    // The reach is site to site, whatever the seating: 3.1 Å is too far.
    let (sub, _) = silyl_sites(&[DVec3::ZERO, DVec3::new(3.1, 0.0, 0.0)], 3);
    let p = plan(&ads, &sub, &config).unwrap();
    assert!(p.hypotheses.is_empty());
    assert_eq!(p.stats.pruned_no_acceptor, 1);
}

/// The moved H of a seated hypothesis sits in its acceptor's dangling bond,
/// not on the line towards where it came from. Checked against geometry of
/// its own: a three-bonded sp3 acceptor's free valence points along −Σ of
/// its bond directions, and any open slot is tetrahedral to every bond the
/// acceptor already has.
#[test]
fn a_transferred_atom_is_seated_in_its_acceptors_open_slot() {
    let config = oh_feet(3.0);
    let mut ads = AtomicStructure::new();
    // The O–H points sideways, towards the acceptor: the old rule put the H
    // ~47° off the acceptor's free valence (straight up).
    add_tagged_methanol(
        &mut ads,
        DVec3::new(0.0, 0.0, 1.8),
        DVec3::new(1.0, 0.0, -0.3),
    );
    let seated_h = |sub: &AtomicStructure| {
        let p = plan(&ads, sub, &config).unwrap();
        assert!(!p.hypotheses.is_empty());
        p.hypotheses
            .iter()
            .map(|h| {
                let seating = p.setup.seat(&h.steps);
                let s = p.setup.start_structure(&h.steps, &seating);
                let t = h.transfers[0];
                let a = s.get_atom(t.acceptor).unwrap();
                let x = s.get_atom(t.moved).unwrap().position;
                let others: Vec<DVec3> = a
                    .bonds
                    .iter()
                    .map(|b| b.other_atom_id())
                    .filter(|&id| id != t.moved)
                    .map(|id| (s.get_atom(id).unwrap().position - a.position).normalize())
                    .collect();
                (x - a.position, others)
            })
            .collect::<Vec<_>>()
    };

    // A silyl acceptor (three H below): its dangling bond points up.
    let (sub, _) = silyl_sites(&[DVec3::ZERO, DVec3::new(2.4, 0.0, 0.0)], 3);
    for (xa, others) in seated_h(&sub) {
        assert_eq!(others.len(), 3);
        let free = -others.iter().copied().sum::<DVec3>().normalize();
        assert!(
            xa.normalize().angle_between(free).to_degrees() < 1.0,
            "H at {xa:?}, free valence {free:?}"
        );
        assert!((xa.length() - 1.48).abs() < 1e-9, "Si–H length");
    }

    // An SiH2 acceptor (two open slots): whichever slot the H takes, it is
    // tetrahedral to the acceptor's two bonds.
    let mut sub = AtomicStructure::new();
    add_silyl(&mut sub, DVec3::ZERO, 3);
    add_silyl(&mut sub, DVec3::new(2.4, 0.0, 0.0), 2);
    let tetrahedral = (-1.0f64 / 3.0).acos().to_degrees();
    for (xa, others) in seated_h(&sub) {
        for o in others {
            let angle = xa.normalize().angle_between(o).to_degrees();
            assert!((angle - tetrahedral).abs() < 2.0, "{angle}°");
        }
    }
}

/// Two OH feet whose H compete for one site: the order of binding decides,
/// and the two orders are two hypotheses. A later leg never bonds a site an
/// earlier leg's H took.
#[test]
fn transfers_are_fixed_when_their_leg_is_added() {
    let mut ads = AtomicStructure::new();
    let (o1, _) = add_tagged_methanol(
        &mut ads,
        DVec3::new(0.0, 0.0, 1.8),
        DVec3::new(1.0, 0.0, -0.3),
    );
    let (o2, _) = add_tagged_methanol(
        &mut ads,
        DVec3::new(4.6, 0.0, 1.8),
        DVec3::new(-1.0, 0.0, -0.3),
    );
    // E(−2.4) A(0) B(2.3) C(4.6) D(7.0): A under o1, C under o2.
    let (sub, s) = silyl_sites(
        &[
            DVec3::new(-2.4, 0.0, 0.0),
            DVec3::ZERO,
            DVec3::new(2.3, 0.0, 0.0),
            DVec3::new(4.6, 0.0, 0.0),
            DVec3::new(7.0, 0.0, 0.0),
        ],
        3,
    );
    let (e, a, b, c, d) = (s[0], s[1], s[2], s[3], s[4]);
    let config = oh_feet(3.0);
    let p = plan(&ads, &sub, &config).unwrap();
    let mut f = vec![(o1, a), (o2, c)];
    f.sort_unstable();
    let both: BTreeSet<InputChange> = plan_changes(&p)
        .into_iter()
        .filter(|(formed, _)| *formed == f)
        .collect();
    let change = |m: [(u32, u32); 2]| {
        let mut m = m.to_vec();
        m.sort_unstable();
        (f.clone(), m)
    };
    // o1 first: its H to B, o2's to D. o2 first: its H to B (B and D tie at
    // 2.3/2.4 — B is nearer), o1's then to E.
    assert_eq!(
        both,
        BTreeSet::from([change([(o1, b), (o2, d)]), change([(o1, e), (o2, b)])])
    );
    assert_stats_add_up(&p);
    assert_matches_oracle(&p, &config, "competing H");
    // No leg bonds a site an earlier leg's H took, nor exceeds a valence.
    for h in &p.hypotheses {
        let mut used = BTreeMap::new();
        for s in &h.steps {
            *used.entry(s.leg.site).or_insert(0) += 1;
            if let Some(a) = s.acceptor {
                *used.entry(a).or_insert(0) += 1;
            }
        }
        assert!(
            used.iter()
                .all(|(&site, &n)| n <= p.setup.sites[site].valence)
        );
    }
}

#[test]
fn to_adsorbate_transfers_and_bad_settings_are_rejected() {
    let (ads, sub) = hand_worked();
    let bad = |c: SequentialSearch| {
        matches!(
            plan(&ads, &sub, &c),
            Err(ChemisorptionError::InvalidConfig(_))
        )
    };
    let base = SequentialSearch::default();
    let to_adsorbate = TransferRule {
        element: H,
        direction: TransferDirection::ToAdsorbate,
    };
    match plan(
        &ads,
        &sub,
        &SequentialSearch {
            transfers: vec![to_adsorbate],
            ..base.clone()
        },
    ) {
        Err(ChemisorptionError::InvalidConfig(msg)) => {
            assert!(msg.contains("to_adsorbate"), "{msg}")
        }
        other => panic!("{other:?}"),
    }
    assert!(bad(SequentialSearch {
        transfers: vec![TransferRule {
            element: O,
            direction: TransferDirection::ToSubstrate
        }],
        ..base.clone()
    }));
    assert!(bad(SequentialSearch {
        anchor_reach: 0.0,
        ..base.clone()
    }));
    assert!(bad(SequentialSearch {
        tolerance: -0.1,
        ..base.clone()
    }));
    assert!(bad(SequentialSearch {
        tolerance: f64::NAN,
        ..base.clone()
    }));
    assert!(bad(SequentialSearch {
        reach: f64::INFINITY,
        ..base.clone()
    }));
    assert!(bad(SequentialSearch {
        budget: 0,
        ..base.clone()
    }));
    assert!(bad(SequentialSearch {
        top_n: 0,
        ..base.clone()
    }));
    assert!(bad(SequentialSearch {
        energy_window: -1.0,
        ..base.clone()
    }));
    assert!(
        plan(
            &ads,
            &sub,
            &SequentialSearch {
                tolerance: 0.0,
                ..base
            }
        )
        .is_ok()
    );
}

#[test]
fn defaults_are_the_spikes() {
    let d = SequentialSearch::default();
    assert_eq!(
        (d.anchor_reach, d.tolerance, d.reach, d.clash_filter),
        (3.5, 0.5, 3.0, true)
    );
}

#[test]
fn tags_select_feet_and_sites_and_unknown_tags_are_errors() {
    let (mut ads, sub) = hand_worked();
    let (mut sub, _) = (sub, ());
    let feet: Vec<u32> = ads
        .atoms_values()
        .filter(|a| a.atomic_number == O)
        .map(|a| a.id)
        .collect();
    let sites: Vec<u32> = sub
        .atoms_values()
        .filter(|a| a.atomic_number == SI)
        .map(|a| a.id)
        .collect();
    ads.add_atom_tag(feet[0], "foot").unwrap();
    ads.add_atom_tag(feet[1], "foot").unwrap();
    for &s in &sites[..3] {
        sub.add_atom_tag(s, "top").unwrap();
    }
    let p = plan(
        &ads,
        &sub,
        &SequentialSearch {
            adsorbate_tag: Some("foot".into()),
            substrate_tag: Some("top".into()),
            ..cfg(2.0, 0.0)
        },
    )
    .unwrap();
    assert_eq!((p.stats.feet, p.stats.sites), (2, 3));
    let (back_a, back_s) = back_maps(&p.setup);
    for h in &p.hypotheses {
        for (f, s) in &h.formed {
            assert!(feet[..2].contains(&back_a[f]) && sites[..3].contains(&back_s[s]));
        }
    }
    match plan(
        &ads,
        &sub,
        &SequentialSearch {
            substrate_tag: Some("nope".into()),
            ..cfg(2.0, 0.0)
        },
    ) {
        Err(ChemisorptionError::UnknownTag {
            side: Side::Substrate,
            tag,
        }) => assert_eq!(tag, "nope"),
        other => panic!("{other:?}"),
    }
}

// ============================================================================
// Foot order: numbered feet (`foot1`, `foot2`, …)
// ============================================================================

/// The tripod with its feet tagged `<tag>` in the order given.
fn tripod_tagged(tags: &[(usize, &str)]) -> (AtomicStructure, AtomicStructure, Vec<u32>) {
    let (mut ads, slab) = tripod_over_slab();
    let (_, feet) = posed_stand_in(3, 0.0, DVec3::ZERO);
    for &(i, tag) in tags {
        ads.add_atom_tag(feet[i], tag).unwrap();
    }
    (ads, slab, feet)
}

fn foot_tag(tolerance: f64) -> SequentialSearch {
    SequentialSearch {
        adsorbate_tag: Some("foot".into()),
        ..cfg(3.5, tolerance)
    }
}

#[test]
fn numbered_feet_fix_the_leg_order() {
    // Against the id order, so the order cannot come from the ids.
    let (ads, slab, feet) = tripod_tagged(&[(2, "foot1"), (0, "foot2"), (1, "foot3")]);
    let config = foot_tag(0.5);
    let p = plan(&ads, &slab, &config).unwrap();
    assert!(p.setup.ordered);
    let order: Vec<u32> = p.setup.feet.iter().map(|f| f.id).collect();
    let want: Vec<u32> = [2, 0, 1]
        .iter()
        .map(|&i| p.setup.adsorbate_ids[&feet[i]])
        .collect();
    assert_eq!(order, want);
    assert_stats_add_up(&p);
    assert_matches_oracle(&p, &config, "ordered tripod");
    assert_eq!(p.stats.duplicates, 0, "one order: nothing to deduplicate");
    // Leg k is foot k on every row, and a state offers only that foot next.
    for (row, r) in p.tree.rows().iter().enumerate() {
        match r.kind {
            RowKind::Root => assert_eq!(next_feet(&p, None, 0), vec![0]),
            RowKind::Foot(f) => assert_eq!(f, 0),
            RowKind::Leg { foot, .. } => {
                assert_eq!(foot as usize + 1, r.legs as usize);
                for f in next_feet(&p, None, row as u32) {
                    assert_eq!(f as usize, r.legs as usize);
                }
            }
        }
    }
    // The orders the unordered search also tries, and only those.
    let (plain, _, _) = tripod_tagged(&[(0, "foot"), (1, "foot"), (2, "foot")]);
    let all = plan(&plain, &slab, &config).unwrap();
    assert!(!all.setup.ordered);
    let (ordered, every) = (plan_bond_sets(&p), plan_bond_sets(&all));
    assert!(p.stats.torus_triples > 0);
    assert!(ordered.is_subset(&every) && ordered.len() < every.len());
}

#[test]
fn the_foot_order_decides_which_hydrogen_goes_first() {
    // As in `transfers_are_fixed_when_their_leg_is_added`: o1 first sends its
    // H to B and o2's to D; o2 first sends its H to B and o1's to E. With
    // numbered feet only the numbered order is searched.
    let mut base = AtomicStructure::new();
    let (o1, _) = add_methanol(
        &mut base,
        DVec3::new(0.0, 0.0, 1.8),
        DVec3::new(1.0, 0.0, -0.3),
    );
    let (o2, _) = add_methanol(
        &mut base,
        DVec3::new(4.6, 0.0, 1.8),
        DVec3::new(-1.0, 0.0, -0.3),
    );
    let (sub, s) = silyl_sites(
        &[
            DVec3::new(-2.4, 0.0, 0.0),
            DVec3::ZERO,
            DVec3::new(2.3, 0.0, 0.0),
            DVec3::new(4.6, 0.0, 0.0),
            DVec3::new(7.0, 0.0, 0.0),
        ],
        3,
    );
    let (e, a, b, c, d) = (s[0], s[1], s[2], s[3], s[4]);
    let mut f = vec![(o1, a), (o2, c)];
    f.sort_unstable();
    let change = |m: [(u32, u32); 2]| {
        let mut m = m.to_vec();
        m.sort_unstable();
        (f.clone(), m)
    };
    let config = oh_feet(3.0);
    for (first, second, want) in [
        (o1, o2, change([(o1, b), (o2, d)])),
        (o2, o1, change([(o1, e), (o2, b)])),
    ] {
        let mut ads = base.clone();
        ads.add_atom_tag(first, "foot1").unwrap();
        ads.add_atom_tag(second, "foot2").unwrap();
        let p = plan(&ads, &sub, &config).unwrap();
        let both: BTreeSet<InputChange> = plan_changes(&p)
            .into_iter()
            .filter(|(formed, _)| *formed == f)
            .collect();
        assert_eq!(both, BTreeSet::from([want]));
        assert_stats_add_up(&p);
        assert_matches_oracle(&p, &config, "numbered OH feet");
    }
}

#[test]
fn numbered_feet_that_give_no_order_are_errors() {
    let config = foot_tag(0.5);
    let refused = |tags: &[(usize, &str)]| -> String {
        let (ads, slab, _) = tripod_tagged(tags);
        match plan(&ads, &slab, &config) {
            Err(ChemisorptionError::FootOrder(msg)) => msg,
            other => panic!("{tags:?}: {other:?}"),
        }
    };
    let both = refused(&[(0, "foot1"), (1, "foot2"), (2, "foot")]);
    assert!(both.contains("both 'foot' and numbered"), "{both}");
    let twice = refused(&[(0, "foot1"), (1, "foot1"), (2, "foot2")]);
    assert!(
        twice.contains("more than one atom is tagged 'foot1'"),
        "{twice}"
    );
    let two = refused(&[(0, "foot1"), (0, "foot2"), (1, "foot3")]);
    assert!(two.contains("tagged both 'foot1' and 'foot2'"), "{two}");

    // A numbered atom that cannot bond: a saturated cage carbon.
    let (mut ads, slab, feet) = tripod_tagged(&[(0, "foot1"), (1, "foot3")]);
    let carbon = ads
        .atoms_values()
        .find(|a| a.atomic_number == C)
        .unwrap()
        .id;
    ads.add_atom_tag(carbon, "foot2").unwrap();
    match plan(&ads, &slab, &config) {
        Err(ChemisorptionError::FootOrder(msg)) => {
            assert!(msg.contains("'foot2' cannot bond"), "{msg}")
        }
        other => panic!("{other:?}"),
    }

    // Gaps are fine: the numbers only order the feet.
    let (ads, slab, _) = tripod_tagged(&[(0, "foot7"), (1, "foot1"), (2, "foot3")]);
    let p = plan(&ads, &slab, &config).unwrap();
    let order: Vec<u32> = p.setup.feet.iter().map(|f| f.id).collect();
    let want: Vec<u32> = [1, 2, 0]
        .iter()
        .map(|&i| p.setup.adsorbate_ids[&feet[i]])
        .collect();
    assert_eq!(order, want);

    // Only digits make a number: `foot_1` and `footx` are other tags, so
    // nothing carries `foot`.
    let (ads, slab, _) = tripod_tagged(&[(0, "foot_1"), (1, "footx")]);
    assert!(matches!(
        plan(&ads, &slab, &config),
        Err(ChemisorptionError::UnknownTag {
            side: Side::Adsorbate,
            ..
        })
    ));
}

#[test]
fn renumbering_the_feet_changes_the_fingerprint() {
    let config = foot_tag(0.5);
    let print = |tags: &[(usize, &str)]| {
        let (ads, slab, _) = tripod_tagged(tags);
        input_fingerprint(&ads, &slab, &config)
    };
    let ordered = print(&[(0, "foot1"), (1, "foot2"), (2, "foot3")]);
    assert_ne!(ordered, print(&[(0, "foot2"), (1, "foot1"), (2, "foot3")]));
    assert_ne!(ordered, print(&[(0, "foot"), (1, "foot"), (2, "foot")]));
    // Another tag on a foot changes no search.
    assert_eq!(
        ordered,
        print(&[(0, "foot1"), (1, "foot2"), (2, "foot3"), (0, "label")])
    );
}

#[test]
fn hydrogen_blocks_and_frozen_pairs_never_bond() {
    let (ads, _) = methoxy_feet(&[DVec3::new(0.0, 0.0, 1.8), DVec3::new(4.0, 0.0, 1.8)]);
    let (mut sub, s) = silyl_sites(
        &[
            DVec3::ZERO,
            DVec3::new(4.0, 0.0, 0.0),
            DVec3::new(0.0, 4.0, 0.0),
        ],
        3,
    );
    let mut capped = sub.clone();
    let h = capped.add_atom(H, DVec3::new(0.0, 0.0, 1.48));
    capped.add_bond(s[0], h, BOND_SINGLE);
    let p = plan(&ads, &capped, &cfg(2.0, 0.0)).unwrap();
    assert_eq!(p.stats.sites, 2);
    assert!(
        p.setup
            .sites
            .iter()
            .all(|x| x.id != p.setup.substrate_ids[&s[0]])
    );

    // Frozen site under a frozen foot: never bonded; a frozen site under a
    // free foot: bonded.
    let mut frozen_ads = ads.clone();
    let o = frozen_ads
        .atoms_values()
        .find(|a| a.atomic_number == O)
        .unwrap()
        .id;
    frozen_ads.set_atom_frozen(o, true);
    sub.set_atom_frozen(s[0], true);
    sub.set_atom_frozen(s[1], true);
    let p = plan(&frozen_ads, &sub, &cfg(2.0, 0.0)).unwrap();
    let (back_a, back_s) = back_maps(&p.setup);
    for h in &p.hypotheses {
        for (f, x) in &h.formed {
            assert!(!(back_a[f] == o && back_s[x] == s[0]));
        }
    }
    assert!(
        p.hypotheses
            .iter()
            .any(|h| h.formed.iter().any(|(_, x)| back_s[x] == s[1]))
    );
}

// ============================================================================
// The tree (§11.8, plan side)
// ============================================================================

/// Each canonical row's children are exactly the (foot, site) choices its
/// level's test accepts, re-derived here from the predicates.
#[test]
fn every_row_has_exactly_the_children_its_test_accepts() {
    let (ads, slab) = tripod_over_slab();
    let config = cfg(3.5, 0.5);
    let p = plan(&ads, &slab, &config).unwrap();
    let setup = &p.setup;
    for r in 0..p.tree.len() as u32 {
        let row = p.tree.row(r);
        if row.duplicate_of.is_some() || matches!(row.kind, RowKind::Root) {
            continue;
        }
        let path = p.tree.path(r);
        if path.len() >= GEOMETRIC_LEGS
            || row
                .hypothesis
                .is_some_and(|h| p.hypotheses[h as usize].mirror == Some(Mirror::Mirrored))
        {
            assert!(p.tree.children(r).is_empty());
            continue;
        }
        let mut want = BTreeSet::new();
        let used: BTreeSet<usize> = path.iter().map(|l| l.foot).collect();
        let foot_range: Vec<usize> = match row.kind {
            RowKind::Foot(f) => vec![f as usize],
            _ => (0..setup.feet.len())
                .filter(|f| !used.contains(f))
                .collect(),
        };
        for f in foot_range {
            for s in 0..setup.sites.len() {
                let leg = Leg { foot: f, site: s };
                let pass = if path.is_empty() {
                    setup.anchor_distance(leg) <= config.anchor_reach
                } else {
                    setup.need_against(&path, leg) <= config.tolerance
                };
                if pass && setup.may_bond(leg) && path.iter().all(|l| l.site != s) {
                    want.insert((f as u32, s as u32));
                }
            }
        }
        let got: BTreeSet<(u32, u32)> = p
            .tree
            .children(r)
            .iter()
            .map(|&c| match p.tree.row(c).kind {
                RowKind::Leg { foot, site, .. } => (foot, site),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(got, want, "row {r} {path:?}");
    }
    // Hiding duplicates lists each hypothesis once; showing them adds back
    // exactly the counted rows.
    let listed: usize = (0..p.tree.len() as u32)
        .map(|r| {
            p.tree
                .visible_children(r, false)
                .iter()
                .filter(|&&c| p.tree.row(c).hypothesis.is_some())
                .count()
        })
        .sum();
    assert_eq!(listed, p.hypotheses.len());
    let shown: usize = (0..p.tree.len() as u32)
        .map(|r| p.tree.visible_children(r, true).len())
        .sum();
    let hidden: usize = p.tree.rows().iter().map(|r| r.duplicates as usize).sum();
    assert_eq!(shown, p.tree.len() - 1);
    assert_eq!(hidden, p.stats.duplicates);
}

// ============================================================================
// Known answers, plan side (§11.7)
// ============================================================================

/// Ethylene over a dimer: di-σ on the dimer and the end-bridge across two
/// dimers are both hypotheses at the defaults (with the old test's reach).
#[test]
fn ethylene_keeps_the_end_bridge() {
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    let (eth, _) = ethanediyl((ps + pp) / 2.0 + DVec3::Z * 2.0, pp - ps);
    let p = plan(
        &eth,
        &slab,
        &SequentialSearch {
            anchor_reach: 4.5,
            ..Default::default()
        },
    )
    .unwrap();
    let (_, back_s) = back_maps(&p.setup);
    let pairs: Vec<(u32, u32)> = p
        .hypotheses
        .iter()
        .filter(|h| h.legs() == 2)
        .map(|h| (back_s[&h.formed[0].1], back_s[&h.formed[1].1]))
        .collect();
    assert!(
        pairs.iter().any(|&(a, b)| slab.has_bond_between(a, b)),
        "di-σ"
    );
    let dimer_atom = |id: u32| {
        let a = slab.get_atom(id).unwrap();
        a.position.z.abs() < 0.3 && a.bonds.len() == 3
    };
    assert!(
        pairs.iter().any(|&(a, b)| !slab.has_bond_between(a, b)
            && dimer_atom(a)
            && dimer_atom(b)
            && slab
                .get_atom(a)
                .unwrap()
                .position
                .distance(slab.get_atom(b).unwrap().position)
                < 4.0),
        "an end-bridge across two neighbouring dimers"
    );
}

/// Water over the central dimer: O on a dimer atom, its H on the partner.
#[test]
fn water_gives_its_hydrogen_to_the_dimer_partner() {
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    let w = water(ps + DVec3::Z * 1.9, pp - ps);
    let p = plan(
        &w,
        &slab,
        &SequentialSearch {
            transfers: vec![H_TO_SUBSTRATE],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(p.stats.feet, 1);
    let (_, back_s) = back_maps(&p.setup);
    let mut on_site = 0;
    for h in &p.hypotheses {
        assert!(h.candidate, "one foot: one-leg bindings are the results");
        let (s, a) = (back_s[&h.formed[0].1], back_s[&h.transfers[0].acceptor]);
        // On the reconstructed surface the nearest free site is the dimer
        // partner, wherever the O lands.
        assert!(
            slab.has_bond_between(s, a),
            "H from {s} went to {a}, not its dimer partner"
        );
        if s == site {
            assert_eq!(a, partner);
            on_site += 1;
        }
    }
    assert_eq!(on_site, 1);
}

// ============================================================================
// Timing (§4.9): plan runs on every evaluation
// ============================================================================

/// The per-row seating, clash and mirror results stay in `plan` only while it
/// is fast. Measured in release on the stand-in hexapod (the largest
/// fixture), alone: ~0.05 s, against ~0.14 s before the θ rule stopped at the first
/// clash-free angle.
#[test]
#[ignore = "timing; run in release"]
fn plan_is_fast_enough_for_every_evaluation() {
    let slab = si100_slab(5.0, 11.0);
    for legs in [3, 6] {
        let (ads, _) = posed_stand_in(legs, 0.0, DVec3::ZERO);
        let config = SequentialSearch::default();
        let _ = plan(&ads, &slab, &config).unwrap();
        let p = plan(&ads, &slab, &config).unwrap();
        println!(
            "{legs} feet: plan {:.3} s, {} hypotheses, {} seated",
            p.stats.seconds,
            p.hypotheses.len(),
            p.stats.candidates
        );
        assert!(
            p.stats.seconds < 0.25,
            "{legs} feet: {:.3} s",
            p.stats.seconds
        );
    }
}
