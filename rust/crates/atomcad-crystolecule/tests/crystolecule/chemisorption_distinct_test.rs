//! What a chemisorption search lists (`chemisorption::distinct`,
//! `doc/design_structure_comparison.md` §5, Phase 2): the two tags, the local
//! cluster, and `unique`'s merges on a real search.
//!
//! The fixture is a one-foot silyl over Si(100)-2×1 with every dimer atom near
//! the axis a site, so the kept candidates are the same binding on equivalent
//! sites. Whether two of them *should* merge is decided independently of the
//! comparison: from the site positions and the unrelaxed slab, by looking for
//! a symmetry of the slab that takes one site to the other.

use crate::chemisorption_sequential_support::*;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::atomic_structure::{AtomicStructure, TagError};
use atomcad_crystolecule::chemisorption::sequential::*;
use atomcad_crystolecule::chemisorption::*;
use atomcad_crystolecule::structure_compare::{Cluster, Motion, compare};
use glam::{DMat3, DVec3};
use std::collections::BTreeSet;
use std::time::Instant;

const H_TO_SUBSTRATE: TransferRule = TransferRule {
    element: H,
    direction: TransferDirection::ToSubstrate,
};

/// Every kept candidate, nothing windowed out.
fn keep_all(config: SequentialSearch) -> SequentialSearch {
    SequentialSearch {
        top_n: 100_000,
        energy_window: f64::INFINITY,
        ..config
    }
}

/// SiH3 posed over the centre of a Si(100) slab, the dimer atoms within 9 Å
/// of the axis the only sites, every one within `anchor_reach` a leg 1.
fn silyl_on_si100(anchor_reach: f64) -> (AtomicStructure, AtomicStructure, SequentialSearch) {
    let mut slab = si100_slab(5.0, 11.0);
    tag_dimers(&mut slab, 9.0);
    let mut silyl = AtomicStructure::new();
    add_silyl_along(&mut silyl, DVec3::new(0.0, 0.0, LIFT + 0.5), -DVec3::Z, 3);
    let config = keep_all(SequentialSearch {
        substrate_tag: Some("dimer".to_string()),
        anchor_reach,
        ..Default::default()
    });
    (silyl, slab, config)
}

fn search(
    ads: &AtomicStructure,
    sub: &AtomicStructure,
    config: &SequentialSearch,
) -> (SequentialPlan, SearchReport) {
    let p = plan(ads, sub, config).unwrap();
    let r = evaluate(&p, config, None).unwrap();
    (p, r)
}

/// The silyl search: twelve dimer atoms, each one candidate.
fn silyl_search() -> (SequentialPlan, SearchReport) {
    silyl_search_with(false)
}

/// …with `passivate_sites`: every other dimer atom capped by an H.
fn silyl_search_with(passivate_sites: bool) -> (SequentialPlan, SearchReport) {
    let (ads, sub, config) = silyl_on_si100(8.0);
    let config = SequentialSearch {
        passivate_sites,
        ..config
    };
    let (p, r) = search(&ads, &sub, &config);
    assert!(r.candidates.len() >= 8, "{} kept", r.candidates.len());
    assert!(r.candidates.iter().all(|c| c.formed.len() == 1));
    (p, r)
}

fn settings(unique: Option<Motion>) -> DistinctSettings {
    DistinctSettings {
        unique,
        ..DistinctSettings::default()
    }
}

/// The site a one-leg candidate bonded to, where it was before relaxing.
fn site_position(p: &SequentialPlan, c: &Candidate) -> DVec3 {
    p.setup.combined.get_atom(c.formed[0].1).unwrap().position
}

// ============================================================================
// The independent check: is there a symmetry of the slab taking a to b?
// ============================================================================

/// The linear parts a symmetry of a (100) slab with axis-aligned cubic cell
/// can have, about the vertical: the four rotations by quarter turns, then the
/// four reflections (across x, y and the two diagonals).
fn point_ops() -> [DMat3; 8] {
    let m = |xx: f64, xy: f64, yx: f64, yy: f64| {
        DMat3::from_cols(DVec3::new(xx, yx, 0.0), DVec3::new(xy, yy, 0.0), DVec3::Z)
    };
    [
        m(1.0, 0.0, 0.0, 1.0),
        m(0.0, -1.0, 1.0, 0.0),
        m(-1.0, 0.0, 0.0, -1.0),
        m(0.0, 1.0, -1.0, 0.0),
        m(-1.0, 0.0, 0.0, 1.0),
        m(1.0, 0.0, 0.0, -1.0),
        m(0.0, 1.0, 1.0, 0.0),
        m(0.0, -1.0, -1.0, 0.0),
    ]
}

/// The point operations a motion allows: the identity for a translation, the
/// rotations for a proper motion, all eight with a mirror.
fn allowed_ops(motion: Motion) -> Vec<DMat3> {
    let ops = point_ops();
    match motion {
        Motion::None => Vec::new(),
        Motion::Translation => ops[..1].to_vec(),
        Motion::Rigid => ops[..4].to_vec(),
        Motion::RigidMirror => ops.to_vec(),
    }
}

/// Whether `x ↦ b + R(x − a)`, for some allowed `R`, maps every substrate atom
/// within 5 Å of `a` onto a substrate atom (0.1 Å): the two sites are
/// equivalent on the unrelaxed slab. `none` relates a site to itself only.
fn sites_related(p: &SequentialPlan, motion: Motion, a: DVec3, b: DVec3) -> bool {
    if motion == Motion::None {
        return a.distance(b) < 1e-6;
    }
    let substrate: Vec<DVec3> = p
        .setup
        .substrate_ids
        .values()
        .map(|&id| p.setup.combined.get_atom(id).unwrap().position)
        .collect();
    let near: Vec<DVec3> = substrate
        .iter()
        .copied()
        .filter(|q| q.distance(a) < 5.0)
        .collect();
    allowed_ops(motion).iter().any(|r| {
        near.iter().all(|&q| {
            let image = b + *r * (q - a);
            substrate.iter().any(|s| s.distance(image) < 0.1)
        })
    })
}

// ============================================================================
// The tags and the local cluster
// ============================================================================

#[test]
fn the_adsorbate_tag_is_on_exactly_the_adsorbates_atoms() {
    let (p, r) = silyl_search();
    let d = distinct(&p.setup, &r, &settings(None)).unwrap();
    for c in &d.candidates {
        assert_eq!(
            c.structure.atoms_with_tag(ADSORBATE_TAG),
            p.setup.adsorbate_atoms
        );
    }
}

/// Water with an H transfer rule: the moved H keeps the adsorbate tag on the
/// substrate, and reaches its acceptor at hop 1 (a seed H is expanded from).
#[test]
fn a_transferred_hydrogen_keeps_the_adsorbate_tag_and_reaches_its_acceptor() {
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    let w = water(ps + DVec3::Z * 1.9, pp - ps);
    let config = keep_all(SequentialSearch {
        transfers: vec![H_TO_SUBSTRATE],
        ..Default::default()
    });
    let (p, r) = search(&w, &slab, &config);
    let one_hop = DistinctSettings {
        unique: None,
        local_hops: 1,
        ..DistinctSettings::default()
    };
    let d = distinct(&p.setup, &r, &one_hop).unwrap();
    let mut transferred = 0;
    for (c, listed) in r.candidates.iter().zip(&d.candidates) {
        let s = &listed.structure;
        assert_eq!(s.atoms_with_tag(ADSORBATE_TAG), p.setup.adsorbate_atoms);
        for t in &c.transfers {
            transferred += 1;
            assert!(s.atom_has_tag(t.moved, ADSORBATE_TAG));
            assert!(s.bond_order_between(t.moved, t.acceptor).is_some());
            assert!(s.atom_has_tag(t.acceptor, LOCAL_TAG), "acceptor at hop 1");
            // …and only through the H: the acceptor (the site's dimer
            // partner) is no neighbour of the O foot, the only heavy seed, so
            // without expanding the H seed it would be at hop 2.
            let foot = c.formed[0].0;
            assert!(s.bond_order_between(foot, t.acceptor).is_none());
        }
    }
    assert!(transferred > 0, "no candidate transferred its H");
}

#[test]
fn zero_hops_is_the_adsorbate() {
    let (p, r) = silyl_search();
    let zero = DistinctSettings {
        unique: None,
        local_hops: 0,
        ..DistinctSettings::default()
    };
    let d = distinct(&p.setup, &r, &zero).unwrap();
    for c in &d.candidates {
        assert_eq!(
            c.structure.atoms_with_tag(LOCAL_TAG),
            p.setup.adsorbate_atoms
        );
    }
}

/// The hop rule on a hand-built chain: A (seed) – S1 – S2 – S3, an H on S1 and
/// on S2, and a bridging H between S1 and S4 that must never be walked
/// through.
#[test]
fn the_local_cluster_stops_at_hydrogens_and_adds_their_caps() {
    let mut s = AtomicStructure::new();
    let x = |k: f64| DVec3::new(2.3 * k, 0.0, 0.0);
    let a = s.add_atom(C, x(0.0));
    let s1 = s.add_atom(SI, x(1.0));
    let s2 = s.add_atom(SI, x(2.0));
    let s3 = s.add_atom(SI, x(3.0));
    let h1 = s.add_atom(H, x(1.0) + DVec3::Y * 1.5);
    let h2 = s.add_atom(H, x(2.0) + DVec3::Y * 1.5);
    let bridge = s.add_atom(H, x(1.0) - DVec3::Y * 1.2);
    let s4 = s.add_atom(SI, x(1.0) - DVec3::Y * 2.4);
    for (u, v) in [
        (a, s1),
        (s1, s2),
        (s2, s3),
        (s1, h1),
        (s2, h2),
        (s1, bridge),
        (bridge, s4),
    ] {
        s.add_bond(u, v, BOND_SINGLE);
    }
    let set = |v: &[u32]| v.iter().copied().collect::<BTreeSet<u32>>();
    assert_eq!(set(&local_cluster(&s, &[a], 0)), set(&[a]));
    assert_eq!(set(&local_cluster(&s, &[a], 1)), set(&[a, s1, h1, bridge]));
    assert_eq!(
        set(&local_cluster(&s, &[a], 2)),
        set(&[a, s1, s2, h1, h2, bridge])
    );
    assert_eq!(
        set(&local_cluster(&s, &[a], 5)),
        set(&[a, s1, s2, s3, h1, h2, bridge]),
        "S4 is reached only through the bridging H"
    );
    // A seed H is expanded from: from the bridge alone, S1 and S4 are hop 1.
    assert_eq!(
        set(&local_cluster(&s, &[bridge], 1)),
        set(&[bridge, s1, s4, h1])
    );
}

// ============================================================================
// Merges
// ============================================================================

/// Every pair `unique` merged is a pair of sites a symmetry of the slab of the
/// allowed kind relates, and every such pair is merged: the classes are
/// exactly the slab's orbits of sites. Covers "merges are sound" and "merges
/// happen" — translation copies under `translation`, mirror pairs under
/// `rigid_mirror` — both checked from the site coordinates alone.
#[test]
fn merges_are_exactly_the_slabs_equivalent_sites() {
    merges_are_the_site_orbits(false);
}

/// The passivant H are part of the compared cluster, so this pins that
/// symmetric bindings receive symmetric H (§4).
#[test]
fn merges_are_the_site_orbits_with_passivated_sites() {
    merges_are_the_site_orbits(true);
}

fn merges_are_the_site_orbits(passivate_sites: bool) {
    let (p, r) = silyl_search_with(passivate_sites);
    if passivate_sites {
        assert!(r.candidates.iter().all(|c| c.passivants > 0));
    }
    let sites: Vec<DVec3> = r.candidates.iter().map(|c| site_position(&p, c)).collect();
    for motion in [
        Motion::None,
        Motion::Translation,
        Motion::Rigid,
        Motion::RigidMirror,
    ] {
        let d = distinct(&p.setup, &r, &settings(Some(motion))).unwrap();
        let class = |i: usize| d.candidates[i].copy_of.unwrap_or(i);
        let mut related_pairs = 0;
        for i in 0..sites.len() {
            for j in i + 1..sites.len() {
                let related = sites_related(&p, motion, sites[i], sites[j]);
                related_pairs += usize::from(related);
                assert_eq!(
                    class(i) == class(j),
                    related,
                    "{motion:?}: #{i} at {:?} and #{j} at {:?}",
                    sites[i],
                    sites[j]
                );
            }
        }
        if motion != Motion::None {
            assert!(
                related_pairs > 0 && d.merged > 0,
                "{motion:?} merged nothing"
            );
        }
        assert_eq!(d.undecided, 0);
    }
    // A mirror pair the fixture must contain: two sites related by a
    // reflection of the slab, merged under `rigid_mirror`.
    let d = distinct(&p.setup, &r, &settings(Some(Motion::RigidMirror))).unwrap();
    let mirror_ops = &point_ops()[4..];
    let mirror_pair = (0..sites.len())
        .flat_map(|i| (i + 1..sites.len()).map(move |j| (i, j)))
        .find(|&(i, j)| {
            let near_i: Vec<DVec3> = p
                .setup
                .substrate_ids
                .values()
                .map(|&id| p.setup.combined.get_atom(id).unwrap().position)
                .filter(|q| q.distance(sites[i]) < 5.0)
                .collect();
            mirror_ops.iter().any(|m| {
                near_i.iter().all(|&q| {
                    let image = sites[j] + *m * (q - sites[i]);
                    p.setup
                        .combined
                        .atoms_values()
                        .any(|a| a.position.distance(image) < 0.1)
                })
            })
        })
        .expect("a mirror pair of sites");
    let class = |i: usize| d.candidates[i].copy_of.unwrap_or(i);
    assert_eq!(class(mirror_pair.0), class(mirror_pair.1));
}

#[test]
fn listed_candidates_are_distinct_and_the_counts_add_up() {
    let (p, r) = silyl_search();
    for motion in [Motion::Translation, Motion::Rigid, Motion::RigidMirror] {
        let s = settings(Some(motion));
        let d = distinct(&p.setup, &r, &s).unwrap();
        let listed: Vec<usize> = d.listed().collect();
        assert_eq!(listed.len() + d.merged, r.candidates.len());
        assert_eq!(
            d.candidates.iter().map(|c| c.copies).sum::<usize>(),
            d.merged
        );
        for (i, c) in d.candidates.iter().enumerate() {
            if let Some(k) = c.copy_of {
                assert!(k < i && d.candidates[k].copy_of.is_none());
                assert_eq!(c.copies, 0);
            }
        }
        let clusters: Vec<Cluster> = listed
            .iter()
            .map(|&i| Cluster::from_tag(&d.candidates[i].structure, LOCAL_TAG).unwrap())
            .collect();
        let compare_settings = s.compare_settings().unwrap();
        for a in 0..clusters.len() {
            for b in a + 1..clusters.len() {
                let c = compare(&clusters[a], &clusters[b], &compare_settings).unwrap();
                assert!(!c.equal, "{motion:?}: listed #{a} and #{b} are equal");
            }
        }
    }
}

/// With `unique` off every kept candidate is listed, in order, as the search
/// kept it, with the two tags added and no copies.
#[test]
fn unique_off_lists_every_kept_candidate() {
    let (p, r) = silyl_search();
    let d = distinct(&p.setup, &r, &settings(None)).unwrap();
    assert_eq!(d.candidates.len(), r.candidates.len());
    assert_eq!((d.merged, d.undecided), (0, 0));
    for (c, listed) in r.candidates.iter().zip(&d.candidates) {
        assert_eq!(listed.copy_of, None);
        assert_eq!(listed.copies, 0);
        let (before, after) = (&c.structure, &listed.structure);
        assert_eq!(before.get_num_of_atoms(), after.get_num_of_atoms());
        for atom in before.atoms_values() {
            let other = after.get_atom(atom.id).unwrap();
            assert_eq!(other.position, atom.position);
            assert_eq!(other.atomic_number, atom.atomic_number);
            for tag in before.atom_tags(atom.id) {
                assert!(after.atom_has_tag(atom.id, tag));
            }
        }
        assert!(!after.atoms_with_tag(ADSORBATE_TAG).is_empty());
        assert!(!after.atoms_with_tag(LOCAL_TAG).is_empty());
        assert!(after.atoms_with_tag(LOCAL_TAG).len() > after.atoms_with_tag(ADSORBATE_TAG).len());
    }
}

#[test]
fn the_answer_does_not_depend_on_the_thread_count() {
    let (ads, sub, config) = silyl_on_si100(8.0);
    let on = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let (p, r) = search(&ads, &sub, &config);
                let d = distinct(&p.setup, &r, &DistinctSettings::default()).unwrap();
                let keys: Vec<_> = r.candidates.iter().map(|c| c.key()).collect();
                let classes: Vec<_> = d
                    .candidates
                    .iter()
                    .map(|c| (c.copy_of, c.copies, c.structure.atoms_with_tag(LOCAL_TAG)))
                    .collect();
                (keys, classes, d.merged, d.undecided)
            })
    };
    let one = on(1);
    assert_eq!(one, on(2));
    assert_eq!(one, on(8));
}

/// A substrate already using 31 tags: the search's `cs_changed` takes the last
/// slot, and `distinct` reports the tag limit instead of panicking.
#[test]
fn the_tag_limit_is_an_error_not_a_panic() {
    let (ads, mut sub, config) = silyl_on_si100(4.5);
    let ids: Vec<u32> = sub.atom_ids().copied().take(30).collect();
    for (k, id) in ids.into_iter().enumerate() {
        sub.add_atom_tag(id, &format!("t{k}")).unwrap();
    }
    assert_eq!(sub.tag_names().len(), 31);
    let (p, r) = search(&ads, &sub, &config);
    assert!(!r.candidates.is_empty());
    let result = distinct(&p.setup, &r, &DistinctSettings::default());
    assert!(
        matches!(result, Err(ChemisorptionError::Tag(TagError::LimitReached))),
        "{result:?}"
    );
}

#[test]
fn the_tolerance_is_validated() {
    for tolerance in [0.0, -0.1, 0.31, f64::NAN, f64::INFINITY] {
        for unique in [None, Some(Motion::Rigid)] {
            let s = DistinctSettings {
                unique,
                tolerance,
                ..DistinctSettings::default()
            };
            assert!(
                matches!(s.validate(), Err(ChemisorptionError::InvalidConfig(_))),
                "{tolerance} accepted"
            );
        }
    }
    for tolerance in [0.01, 0.15, 0.3] {
        let s = DistinctSettings {
            tolerance,
            ..DistinctSettings::default()
        };
        assert!(s.validate().is_ok());
    }
    let (p, r) = silyl_search();
    let bad = DistinctSettings {
        tolerance: 0.5,
        ..DistinctSettings::default()
    };
    assert!(matches!(
        distinct(&p.setup, &r, &bad),
        Err(ChemisorptionError::InvalidConfig(_))
    ));
}

#[test]
fn the_defaults_are_the_designs() {
    let d = DistinctSettings::default();
    assert_eq!(d.unique, Some(Motion::RigidMirror));
    assert_eq!(d.tolerance, 0.15);
    assert_eq!(d.local_hops, 2);
    assert_eq!(SequentialSearch::default().top_n, 50);
    // Every field defaults on its own when a saved value lacks it (D10).
    let partial: DistinctSettings = serde_json::from_str(r#"{"local_hops": 1}"#).unwrap();
    assert_eq!(
        partial,
        DistinctSettings {
            local_hops: 1,
            ..DistinctSettings::default()
        }
    );
    let off: DistinctSettings = serde_json::from_str(r#"{"unique": null}"#).unwrap();
    assert_eq!(off.unique, None);
}

// ============================================================================
// Timing
// ============================================================================

/// `distinct` on the stand-in hexapod's stored result, three legs, at the
/// default `top_n`, printed beside the plan's time: what an output rebuild
/// costs against an evaluation.
#[test]
#[ignore = "timing; run in release"]
fn distinct_is_fast_enough_for_every_evaluation() {
    let slab = si100_slab(5.0, 11.0);
    let (ads, _) = posed_stand_in(6, 0.0, DVec3::ZERO);
    let config = SequentialSearch {
        formed_bonds: Some(3),
        ..Default::default()
    };
    let (p, r) = search(&ads, &slab, &config);
    let _ = distinct(&p.setup, &r, &DistinctSettings::default()).unwrap();
    let start = Instant::now();
    let d = distinct(&p.setup, &r, &DistinctSettings::default()).unwrap();
    let seconds = start.elapsed().as_secs_f64();
    println!(
        "hexapod: plan {:.3} s; distinct {:.3} s over {} kept, {} merged, {} undecided",
        p.stats.seconds,
        seconds,
        r.candidates.len(),
        d.merged,
        d.undecided
    );
    assert!(seconds < 0.25, "distinct took {seconds:.3} s");
}
