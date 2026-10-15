//! The sequential chemisorption search, the tests that relax
//! (`design_chemisorption_sequential.md` §11.4–11.7): the audit on every
//! candidate, what is relaxed, the metamorphic tests on strains, determinism,
//! the budget, the run control and the two known answers. Fixtures are kept
//! small; the stand-in tripod and hexapod relax only in the release-only
//! snapshot tests of `chemisorption_sequential_golden_test.rs`.

use crate::chemisorption_sequential_support::*;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::chemisorption::relax::relax;
use atomcad_crystolecule::chemisorption::sequential::*;
use atomcad_crystolecule::chemisorption::{
    BondInventory, CHANGED_TAG, ChemisorptionError, DistinctSettings, PASSIVANT_TAG,
    TransferDirection, TransferRule, distinct,
};
use atomcad_crystolecule::simulation::uff::VdwMode;
use atomcad_util::job_control::JobControl;
use glam::{DQuat, DVec3};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const H_TO_SUBSTRATE: TransferRule = TransferRule {
    element: H,
    direction: TransferDirection::ToSubstrate,
};

/// Everything kept and nothing windowed out, so a test sees every relaxation.
fn keep_all(config: SequentialSearch) -> SequentialSearch {
    SequentialSearch {
        top_n: 100_000,
        energy_window: f64::INFINITY,
        ..config
    }
}

fn run(
    ads: &AtomicStructure,
    sub: &AtomicStructure,
    config: &SequentialSearch,
) -> (SequentialPlan, SearchReport) {
    let p = plan(ads, sub, config).unwrap();
    let r = evaluate(&p, config, None).unwrap();
    audit(&p, &r, config);
    (p, r)
}

fn strains(p: &SequentialPlan, r: &SearchReport) -> BTreeMap<InputChange, f64> {
    r.candidates
        .iter()
        .map(|c| (input_change(&p.setup, &c.key()), c.strain))
        .collect()
}

/// The hand-worked triangle of the plan tests, small enough to relax often:
/// three methoxy feet 6 Å apart over five silyls.
fn triangle() -> (AtomicStructure, AtomicStructure) {
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

fn triangle_config() -> SequentialSearch {
    keep_all(SequentialSearch {
        anchor_reach: 2.0,
        tolerance: 0.0,
        clash_filter: false,
        ..Default::default()
    })
}

/// The stand-in tripod rigidly over three silyls, each bond along the body
/// normal: the binding is the pose itself.
fn planted_tripod() -> (AtomicStructure, AtomicStructure, Vec<(u32, u32)>) {
    let (ads, feet) = stand_in(3);
    let b = rest_length(O, SI);
    let mut sub = AtomicStructure::new();
    let mut planted = Vec::new();
    for &f in &feet {
        let p = ads.get_atom(f).unwrap().position - DVec3::Z * b;
        planted.push((f, add_silyl(&mut sub, p, 3)));
    }
    planted.sort_unstable();
    (ads, sub, planted)
}

/// The stand-in tripod over Si(100), sites restricted to the dimers near it:
/// a real, rigid binding problem small enough to relax in every test.
fn tagged_tripod(radius: f64) -> (AtomicStructure, AtomicStructure) {
    let (ads, _) = posed_stand_in(3, 0.0, DVec3::ZERO);
    let mut slab = si100_slab(5.0, 11.0);
    tag_dimers(&mut slab, radius);
    (ads, slab)
}

// ============================================================================
// Planted bindings and the reference state
// ============================================================================

#[test]
fn a_relaxed_planted_binding_is_a_candidate() {
    let (ads, sub, planted) = planted_tripod();
    let config = keep_all(SequentialSearch {
        anchor_reach: 1.8,
        tolerance: 0.0,
        ..Default::default()
    });
    let (p, r) = run(&ads, &sub, &config);
    let c = r
        .candidates
        .iter()
        .find(|c| input_change(&p.setup, &c.key()).0 == planted)
        .expect("the planted binding is relaxed and listed");
    assert!(c.converged);
    assert!(!c.seating_clash);
    // Every changed atom is tagged for highlighting, and nothing else.
    let tagged: BTreeSet<u32> = c
        .structure
        .atoms_with_tag(CHANGED_TAG)
        .into_iter()
        .collect();
    let changed: BTreeSet<u32> = p.hypotheses[c.hypothesis]
        .changed_atoms()
        .into_iter()
        .collect();
    assert_eq!(tagged, changed);
    assert_eq!(p.hypotheses[c.hypothesis].row, c.row);
}

/// The reference is the separated state: the adsorbate relaxed alone plus the
/// substrate relaxed alone.
#[test]
fn strain_is_measured_from_the_separated_state() {
    let (ads, sub) = triangle();
    let config = triangle_config();
    let (_, r) = run(&ads, &sub, &config);
    let settings = atomcad_crystolecule::chemisorption::RelaxSettings::default();
    let (mut a, mut s) = (ads.clone(), sub.clone());
    let want = relax(&mut a, &settings).unwrap().energy + relax(&mut s, &settings).unwrap().energy;
    assert!((r.reference_energy - want).abs() < 1e-9);
    for c in &r.candidates {
        assert!((c.terms.total() - c.strain).abs() < 1e-6);
    }
}

// ============================================================================
// What is relaxed (§4.8, §11.6)
// ============================================================================

#[test]
fn only_candidates_are_relaxed() {
    let (ads, sub) = triangle();
    // Three legs only: no two-leg hypothesis is relaxed, though the three-leg
    // ones are built from them.
    let config = SequentialSearch {
        formed_bonds: Some(3),
        ..triangle_config()
    };
    let (p, r) = run(&ads, &sub, &config);
    assert_eq!(r.stats.relaxed, 2);
    assert!(
        r.relaxed
            .iter()
            .all(|x| p.hypotheses[x.hypothesis].legs() == 3)
    );
    assert!(r.candidates.iter().all(|c| c.formed.len() == 3));

    // Unfiltered: every two-leg hypothesis is relaxed and listed, even those
    // that three-leg bindings extend ("stuck on two legs" is a result).
    let config = triangle_config();
    let (p, r) = run(&ads, &sub, &config);
    assert_eq!(r.stats.relaxed, p.stats.candidates);
    let two: BTreeSet<_> = p
        .hypotheses
        .iter()
        .filter(|h| h.legs() == 2)
        .map(|h| h.key())
        .collect();
    let listed: BTreeSet<_> = r
        .candidates
        .iter()
        .filter(|c| c.formed.len() == 2)
        .map(|c| c.key())
        .collect();
    assert_eq!(listed, two);
    assert_eq!(two.len(), 15);
}

/// Filtering during enumeration loses nothing: the same bond sets and the
/// same strains, bit for bit, as filtering the unfiltered result.
#[test]
fn the_filters_commute_with_the_search() {
    let (ads, sub) = triangle();
    let (p, r) = run(&ads, &sub, &triangle_config());
    let all = strains(&p, &r);
    for legs in [2, 3] {
        let (q, s) = run(
            &ads,
            &sub,
            &SequentialSearch {
                formed_bonds: Some(legs),
                ..triangle_config()
            },
        );
        let want: BTreeMap<_, _> = all
            .iter()
            .filter(|(k, _)| k.0.len() == legs)
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        assert_eq!(strains(&q, &s), want, "{legs} legs");
    }
    let inv: BondInventory = "formed 2× O–Si".parse().unwrap();
    let (q, s) = run(
        &ads,
        &sub,
        &SequentialSearch {
            bond_inventory: Some(inv.clone()),
            ..triangle_config()
        },
    );
    let want: BTreeMap<_, _> = r
        .candidates
        .iter()
        .filter(|c| c.bond_inventory == inv)
        .map(|c| (input_change(&p.setup, &c.key()), c.strain))
        .collect();
    assert_eq!(strains(&q, &s), want);
}

// ============================================================================
// Metamorphic tests on strains (§11.4)
// ============================================================================

fn assert_same_strains(
    a: &BTreeMap<InputChange, f64>,
    b: &BTreeMap<InputChange, f64>,
    tol: f64,
    what: &str,
) {
    assert_eq!(
        a.keys().collect::<Vec<_>>(),
        b.keys().collect::<Vec<_>>(),
        "{what}"
    );
    for (k, x) in a {
        assert!((x - b[k]).abs() < tol, "{what}: {k:?}: {x} vs {}", b[k]);
    }
}

fn tagged_config() -> SequentialSearch {
    keep_all(SequentialSearch {
        substrate_tag: Some("dimer".into()),
        ..Default::default()
    })
}

#[test]
fn a_rigid_motion_keeps_every_strain() {
    let (ads, slab) = tagged_tripod(5.0);
    let config = tagged_config();
    let (p, r) = run(&ads, &slab, &config);
    assert!(r.candidates.len() > 10);
    let q = DQuat::from_euler(glam::EulerRot::XYZ, 0.7, -1.2, 2.1);
    let t = DVec3::new(11.0, -4.0, 7.5);
    let (p2, r2) = run(&moved(&ads, q, t), &moved(&slab, q, t), &config);
    assert_same_strains(&strains(&p, &r), &strains(&p2, &r2), 1e-3, "rigid motion");
}

/// Two poses that share a change set seat it identically, so they give it
/// the same strain: seating leaks no pose information.
#[test]
fn two_poses_give_a_shared_bond_set_the_same_strain() {
    let (a0, slab) = tagged_tripod(5.0);
    let (a1, _) = posed_stand_in(3, 7.0, DVec3::new(0.4, -0.3, 0.2));
    let config = tagged_config();
    let (p, r) = run(&a0, &slab, &config);
    let (p2, r2) = run(&a1, &slab, &config);
    let (a, b) = (strains(&p, &r), strains(&p2, &r2));
    let mut shared = 0;
    for (k, x) in &a {
        if let Some(y) = b.get(k) {
            assert!((x - y).abs() < 1e-3, "{k:?}: {x} vs {y}");
            shared += 1;
        }
    }
    assert!(shared >= 10, "{shared} shared");
}

#[test]
fn relabelling_keeps_every_strain() {
    let (ads, slab) = tagged_tripod(5.0);
    let config = tagged_config();
    let (p, r) = run(&ads, &slab, &config);
    let order = |s: &AtomicStructure| {
        let mut ids: Vec<u32> = s.atom_ids().copied().collect();
        ids.sort_unstable();
        ids.reverse();
        ids
    };
    let (ads2, ma) = relabelled(&ads, &order(&ads));
    let (slab2, ms) = relabelled(&slab, &order(&slab));
    let (p2, r2) = run(&ads2, &slab2, &config);
    let mapped: BTreeMap<InputChange, f64> = strains(&p, &r)
        .into_iter()
        .map(|((f, m), v)| {
            let mut f: Vec<(u32, u32)> = f.iter().map(|(a, s)| (ma[a], ms[s])).collect();
            f.sort_unstable();
            ((f, m), v)
        })
        .collect();
    assert_same_strains(&mapped, &strains(&p2, &r2), 1e-3, "relabelled");
}

#[test]
fn one_thread_and_many_give_the_same_result() {
    let (ads, sub) = triangle();
    let config = SequentialSearch {
        top_n: 5,
        ..triangle_config()
    };
    let on = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| run(&ads, &sub, &config))
    };
    let ((p1, r1), (p4, r4)) = (on(1), on(4));
    assert_eq!(p1.tree, p4.tree);
    let view = |r: &SearchReport| -> Vec<_> {
        r.candidates
            .iter()
            .map(|c| (c.key(), c.strain.to_bits()))
            .collect()
    };
    assert_eq!(view(&r1), view(&r4));
    assert_eq!(r1.relaxed, r4.relaxed);
}

/// A truncated run relaxes a prefix of the plan order.
#[test]
fn a_truncated_run_relaxes_a_prefix() {
    let (ads, sub) = triangle();
    let (full, r_full) = run(&ads, &sub, &triangle_config());
    let (p, r) = run(
        &ads,
        &sub,
        &SequentialSearch {
            budget: 6,
            ..triangle_config()
        },
    );
    assert!(p.stats.truncated && r.stats.plan.truncated);
    assert_eq!(r.stats.relaxed, 6);
    let keys = |p: &SequentialPlan, r: &SearchReport| -> Vec<_> {
        r.relaxed
            .iter()
            .map(|x| p.hypotheses[x.hypothesis].key())
            .collect()
    };
    assert_eq!(keys(&p, &r)[..], keys(&full, &r_full)[..6]);
}

// ============================================================================
// Run control, vdW modes
// ============================================================================

#[test]
fn a_search_reports_progress_and_can_be_cancelled() {
    let (ads, sub) = triangle();
    let config = triangle_config();
    let control = Arc::new(JobControl::new());
    let r = search(&ads, &sub, &config, Some(&control)).unwrap();
    let progress = control.snapshot();
    assert_eq!(progress.total, Some(r.stats.relaxed as u64 + 1));
    assert_eq!(Some(progress.done), progress.total);
    assert_eq!(progress.phase, "Relaxing");

    let cancelled = Arc::new(JobControl::new());
    cancelled.cancel();
    assert!(matches!(
        search(&ads, &sub, &config, Some(&cancelled)),
        Err(ChemisorptionError::Cancelled)
    ));
}

#[test]
fn a_vdw_cutoff_works() {
    let (ads, sub) = triangle();
    let config = SequentialSearch {
        vdw_mode: VdwMode::Cutoff(6.0),
        formed_bonds: Some(3),
        ..triangle_config()
    };
    let (_, r) = run(&ads, &sub, &config);
    assert_eq!(r.candidates.len(), 2);
}

// ============================================================================
// Known answers (§11.7)
// ============================================================================

/// Ethylene over a dimer: di-σ (both C on one dimer) ranks below the
/// end-bridge across two dimers, as in the old engine (39.8 against 43.2
/// kcal/mol in the spike).
#[test]
fn ethylene_di_sigma_ranks_below_the_end_bridge() {
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    let (eth, _) = ethanediyl((ps + pp) / 2.0 + DVec3::Z * 2.0, pp - ps);
    // Sites on the facet only: the fixture's frozen rim has unreconstructed
    // atoms with two dangling bonds, which the shell reaches and which bind
    // ethylene lower than any dimer does (32 kcal/mol, an artefact).
    let mut slab = slab;
    tag_dimers(&mut slab, f64::INFINITY);
    let config = keep_all(SequentialSearch {
        anchor_reach: 4.5,
        formed_bonds: Some(2),
        substrate_tag: Some("dimer".into()),
        ..Default::default()
    });
    let (p, r) = run(&eth, &slab, &config);
    let (_, back) = back_maps(&p.setup);
    let pair = |c: &Candidate| (back[&c.formed[0].1], back[&c.formed[1].1]);
    let dimer_atom = |id: u32| {
        let a = slab.get_atom(id).unwrap();
        a.position.z.abs() < 0.3 && a.bonds.len() == 3
    };
    let best = &r.candidates[0];
    let (a, b) = pair(best);
    assert!(
        slab.has_bond_between(a, b),
        "the best two-leg binding is di-σ"
    );
    assert!(best.converged);
    let end_bridge = r
        .candidates
        .iter()
        .find(|c| {
            let (a, b) = pair(c);
            !slab.has_bond_between(a, b) && dimer_atom(a) && dimer_atom(b)
        })
        .expect("an end-bridge across two dimers");
    assert!(best.strain < end_bridge.strain);
    println!(
        "di-σ {:.1}, end-bridge {:.1}",
        best.strain, end_bridge.strain
    );
    assert!((39.0..40.5).contains(&best.strain) && (end_bridge.strain - 43.2).abs() < 1.0);
}

/// Water over the central dimer: OH on one dimer atom, H on its partner, by
/// construction of the rule.
#[test]
fn water_relaxes_with_its_hydrogen_on_the_dimer_partner() {
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
    let (p, r) = run(&w, &slab, &config);
    let (_, back) = back_maps(&p.setup);
    let c = r
        .candidates
        .iter()
        .find(|c| back[&c.formed[0].1] == site)
        .expect("O on the site below it");
    assert_eq!(back[&c.transfers[0].acceptor], partner);
    assert_eq!(
        c.bond_inventory.to_string(),
        "formed 1× H–Si, 1× O–Si; broken 1× H–O"
    );
    assert!(c.converged);
}

// ============================================================================
// include_mirrored (design_chemisorb_mirrors_and_passivation.md §3)
// ============================================================================

/// A mirrored start relaxes like any other (converged or marked unconverged,
/// never an error), and its candidate says it was mirrored. Off, none does.
#[test]
fn a_mirrored_start_relaxes_and_is_flagged() {
    let (ads, sub) = triangle();
    let config = SequentialSearch {
        include_mirrored: true,
        ..triangle_config()
    };
    let (p, r) = run(&ads, &sub, &config);
    assert!(p.stats.mirrored_kept > 0);
    assert_eq!(
        r.candidates.iter().filter(|c| c.mirrored).count(),
        p.stats.mirrored_kept,
        "everything relaxed and kept"
    );
    for c in &r.candidates {
        let h = r.hypothesis(&p, c.hypothesis);
        assert_eq!(c.mirrored, h.mirror == Some(Mirror::Mirrored));
        assert!(c.strain.is_finite());
    }

    let (_, off) = run(&ads, &sub, &triangle_config());
    assert!(off.candidates.iter().all(|c| !c.mirrored));
    assert_eq!(
        off.candidates.len() + p.stats.mirrored_kept,
        r.candidates.len()
    );
}

/// The stand-in tripod is achiral and its three feet are equivalent, so a
/// mirrored start cannot reach a binding the proper search lacks: each
/// mirrored candidate is either `unique`-merged with a proper one, or more
/// strained than the best proper candidate of its inventory. A mirrored
/// candidate that ranks first and stays apart would be a bug. Not every one
/// has to merge: seated on two legs, a rigid tripod usually cannot swing its
/// third foot across and ends strained instead.
///
/// The site Si are frozen, so the sites form a rigid surface. Over three free
/// silyls every assignment is the same molecule, and a mirrored start can
/// settle in another conformer of it, 0.03 kcal/mol below the proper one
/// (measured): not a different binding, but not one `unique` merges either.
#[test]
fn a_mirrored_start_of_an_achiral_tripod_finds_nothing_new() {
    let (ads, mut sub, _) = planted_tripod();
    let si: Vec<u32> = sub
        .atoms_values()
        .filter(|a| a.atomic_number == SI)
        .map(|a| a.id)
        .collect();
    for id in si {
        sub.set_atom_frozen(id, true);
    }
    let config = keep_all(SequentialSearch {
        anchor_reach: 1.8,
        tolerance: 0.0,
        clash_filter: false,
        include_mirrored: true,
        ..Default::default()
    });
    let (p, r) = run(&ads, &sub, &config);
    assert!(p.stats.mirrored_kept > 0);
    let d = distinct(&p.setup, &r, &DistinctSettings::default()).unwrap();
    let group = |i: usize| d.candidates[i].copy_of.unwrap_or(i);
    let (mut merged, mut strained, mut checked) = (0, 0, 0);
    for (i, c) in r.candidates.iter().enumerate().filter(|(_, c)| c.mirrored) {
        let best_proper = r
            .candidates
            .iter()
            .filter(|o| !o.mirrored && o.bond_inventory == c.bond_inventory)
            .map(|o| o.strain)
            .fold(f64::INFINITY, f64::min);
        assert!(best_proper.is_finite(), "a proper binding of {c:?}");
        let with_proper = r
            .candidates
            .iter()
            .enumerate()
            .any(|(j, o)| !o.mirrored && group(j) == group(i));
        if with_proper {
            merged += 1;
        } else {
            assert!(
                c.strain > best_proper,
                "a mirrored candidate stays apart and ranks first: {} vs {best_proper}",
                c.strain
            );
            strained += 1;
        }
        checked += 1;
    }
    assert_eq!(checked, p.stats.mirrored_kept);
    println!("mirrored candidates: {merged} merged with a proper one, {strained} more strained");
}

// ============================================================================
// passivate_sites (design_chemisorb_mirrors_and_passivation.md §4)
// ============================================================================

fn passivated(config: SequentialSearch) -> SequentialSearch {
    SequentialSearch {
        passivate_sites: true,
        ..config
    }
}

/// The passivation key a candidate's structure carries: its `cs_passivant`
/// H by host.
fn key_of(c: &Candidate) -> PassivationKey {
    let mut hosts: BTreeMap<u32, usize> = BTreeMap::new();
    for h in c.structure.atoms_with_tag(PASSIVANT_TAG) {
        let atom = c.structure.get_atom(h).unwrap();
        let host = atom.bonds.iter().find(|b| !b.is_delete_marker()).unwrap();
        *hosts.entry(host.other_atom_id()).or_insert(0) += 1;
    }
    hosts.into_iter().collect()
}

/// Every site's open valence is capped and nothing else (the audit checks
/// the hosts, the atom count and the strain against the key's reference);
/// the relaxed rows say which relaxations were passivated.
#[test]
fn a_passivated_candidate_caps_every_open_site_valence() {
    let (ads, sub) = triangle();
    let config = passivated(triangle_config());
    let (p, r) = run(&ads, &sub, &config);
    assert!(!r.candidates.is_empty());
    for c in &r.candidates {
        // Five silyls, one valence each: the candidate's legs use some, the
        // rest get one H.
        assert_eq!(c.passivants, 5 - c.formed.len(), "{:?}", c.formed);
        assert_eq!(
            c.structure.get_num_of_atoms(),
            p.setup.combined.get_num_of_atoms() + c.passivants
        );
    }
    for x in &r.relaxed {
        assert_eq!(x.passivated, p.hypotheses[x.hypothesis].candidate);
    }
}

/// The references are relaxed up front, one per distinct key among the
/// relaxed candidates, and the progress total announced covers them.
#[test]
fn one_reference_is_relaxed_per_key_before_the_candidates() {
    let (ads, sub) = triangle();
    let config = passivated(triangle_config());
    let control = Arc::new(JobControl::new());
    let r = search(&ads, &sub, &config, Some(&control)).unwrap();
    let p = plan(&ads, &sub, &config).unwrap();
    let keys: BTreeSet<PassivationKey> = p
        .to_relax
        .iter()
        .map(|&i| &p.hypotheses[i])
        .filter(|h| h.candidate)
        .map(|h| {
            p.setup.passivation_key(
                &p.setup
                    .start_structure(&h.steps, h.seating.as_ref().unwrap()),
            )
        })
        .collect();
    let have: BTreeSet<PassivationKey> = r.references.keys().cloned().collect();
    assert_eq!(have, keys);
    assert!(keys.len() < r.candidates.len(), "candidates share keys");
    let progress = control.snapshot();
    assert_eq!(
        progress.total,
        Some((r.stats.relaxed + 1 + r.references.len()) as u64)
    );
    assert_eq!(Some(progress.done), progress.total);
}

/// Two feet over two sites that both use: nothing is left to cap, and the
/// setting changes nothing (§4: the passivated path and its reference reduce
/// to the bare ones).
#[test]
fn with_nothing_to_cap_the_setting_changes_nothing() {
    let (ads, _) = methoxy_feet(&[DVec3::ZERO, DVec3::new(3.0, 0.0, 0.0)]);
    let (sub, _) = silyl_sites(&[DVec3::new(0.0, 0.0, -1.7), DVec3::new(3.0, 0.0, -1.7)], 3);
    let config = keep_all(SequentialSearch {
        anchor_reach: 4.0,
        tolerance: 0.0,
        formed_bonds: Some(2),
        clash_filter: false,
        ..Default::default()
    });
    let (p0, off) = run(&ads, &sub, &config);
    let (_, on) = run(&ads, &sub, &passivated(config.clone()));
    assert!(!off.candidates.is_empty());
    assert_eq!(off.candidates.len(), on.candidates.len());
    for (a, b) in off.candidates.iter().zip(&on.candidates) {
        assert_eq!(a.key(), b.key());
        assert_eq!(b.passivants, 0);
        assert_eq!(a.strain.to_bits(), b.strain.to_bits());
        assert_eq!(a.terms, b.terms);
        for id in p0.setup.combined.atom_ids() {
            assert_eq!(
                a.structure.get_atom(*id).unwrap().position,
                b.structure.get_atom(*id).unwrap().position
            );
        }
    }
}

/// A site made by deleting a surface H keeps the hydrogen-passivation flag;
/// it is a site all the same, and it is capped.
#[test]
fn a_site_flagged_as_passivated_is_still_capped() {
    let (ads, mut sub) = triangle();
    let sites: Vec<u32> = sub
        .atoms_values()
        .filter(|a| a.atomic_number == SI)
        .map(|a| a.id)
        .collect();
    for &id in &sites {
        sub.set_atom_hydrogen_passivation(id, true);
    }
    let config = passivated(SequentialSearch {
        formed_bonds: Some(3),
        ..triangle_config()
    });
    let (_, r) = run(&ads, &sub, &config);
    assert!(!r.candidates.is_empty());
    assert!(r.candidates.iter().all(|c| c.passivants == 2));
}

/// A used site with a valence left and an acceptor with one left after its
/// transferred H each get one H: water over two SiH2.
#[test]
fn leftover_valence_on_a_used_site_and_an_acceptor_is_capped() {
    let (sub, ids) = silyl_sites(&[DVec3::new(0.0, 0.0, -1.7), DVec3::new(2.4, 0.0, -1.7)], 2);
    let w = water(DVec3::new(0.0, 0.0, 0.3), DVec3::X);
    let config = passivated(keep_all(SequentialSearch {
        transfers: vec![H_TO_SUBSTRATE],
        anchor_reach: 2.5,
        ..Default::default()
    }));
    let (p, r) = run(&w, &sub, &config);
    let (_, back) = back_maps(&p.setup);
    let c = r
        .candidates
        .iter()
        .find(|c| back[&c.formed[0].1] == ids[0])
        .expect("O on the first SiH2, H on the second");
    assert_eq!(back[&c.transfers[0].acceptor], ids[1]);
    assert_eq!(c.passivants, 2);
    let key: Vec<(u32, usize)> = key_of(c).into_iter().map(|(h, n)| (back[&h], n)).collect();
    assert_eq!(key, vec![(ids[0], 1), (ids[1], 1)]);
}

/// The reference carries the candidate's atoms: the adsorbate, the substrate
/// and the same H on the same hosts; and relaxed from scratch, outside the
/// search's map, it has the same energy.
#[test]
fn the_reference_conserves_atoms_and_is_a_function_of_its_key() {
    let (ads, sub) = triangle();
    let config = passivated(triangle_config());
    let (p, r) = run(&ads, &sub, &config);
    let elements = |s: &AtomicStructure| {
        let mut count: BTreeMap<i16, usize> = BTreeMap::new();
        for a in s.atoms_values() {
            *count.entry(a.atomic_number).or_insert(0) += 1;
        }
        count
    };
    let (_, back) = back_maps(&p.setup);
    let mut checked = BTreeSet::new();
    for c in &r.candidates {
        let key = key_of(c);
        let reference = p.setup.passivated_substrate(&p.substrate, &key);
        let mut both = elements(&reference);
        for (z, n) in elements(&ads) {
            *both.entry(z).or_insert(0) += n;
        }
        assert_eq!(both, elements(&c.structure));
        // The same H on the same hosts.
        let mut hosts: BTreeMap<u32, usize> = BTreeMap::new();
        for a in reference.atoms_values() {
            if p.substrate.get_atom(a.id).is_none() {
                *hosts.entry(a.bonds[0].other_atom_id()).or_insert(0) += 1;
            }
        }
        let want: BTreeMap<u32, usize> = key.iter().map(|&(h, n)| (back[&h], n)).collect();
        assert_eq!(hosts, want);
        if checked.insert(key.clone()) && checked.len() <= 3 {
            let fresh = passivated_reference(&p, &config, &key).unwrap();
            assert!((fresh.energy - r.references[&key].energy).abs() < 1e-9);
        }
    }
}

/// Two binding orders onto a site with a valence left share one key and so
/// one reference; and the references do not change with the thread count.
#[test]
fn candidates_with_one_key_share_one_reference_whatever_the_threads() {
    let (ads, _) = methoxy_feet(&[DVec3::ZERO, DVec3::new(3.0, 0.0, 0.0)]);
    let mut sub = AtomicStructure::new();
    let s0 = add_silyl(&mut sub, DVec3::new(0.0, 0.0, -1.7), 2);
    add_silyl(&mut sub, DVec3::new(3.0, 0.0, -1.7), 3);
    add_silyl(&mut sub, DVec3::new(9.0, 0.0, -1.7), 3);
    let config = passivated(keep_all(SequentialSearch {
        anchor_reach: 4.0,
        tolerance: 0.0,
        formed_bonds: Some(2),
        clash_filter: false,
        ..Default::default()
    }));
    let on = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| run(&ads, &sub, &config))
    };
    let ((p, r1), (_, r4)) = (on(1), on(4));
    let (_, back) = back_maps(&p.setup);
    // Both feet on s0 and its neighbour, in either order: s0 keeps one
    // valence either way, and the far silyl is capped in full.
    let shared: Vec<&Candidate> = r1
        .candidates
        .iter()
        .filter(|c| key_of(c).iter().any(|&(h, n)| back[&h] == s0 && n == 1))
        .collect();
    assert!(shared.len() >= 2, "two candidates leave s0 one valence");
    let keys: BTreeSet<PassivationKey> = shared.iter().map(|c| key_of(c)).collect();
    assert_eq!(keys.len(), 1, "one key");
    let all: BTreeSet<PassivationKey> = r1.candidates.iter().map(key_of).collect();
    assert_eq!(r1.references.len(), all.len());
    let bits = |r: &SearchReport| -> Vec<_> {
        r.references
            .iter()
            .map(|(k, v)| (k.clone(), v.energy.to_bits()))
            .collect()
    };
    assert_eq!(bits(&r1), bits(&r4));
    let view = |r: &SearchReport| -> Vec<_> {
        r.candidates
            .iter()
            .map(|c| (c.key(), c.strain.to_bits(), c.passivants))
            .collect()
    };
    assert_eq!(view(&r1), view(&r4));
    assert_eq!(r1.relaxed, r4.relaxed);
}

/// An H placed inside the bound molecule's body is placed anyway (D8), and
/// the relaxation converges or is marked unconverged, never errors: a
/// methoxy foot bonds the silyl below it, and a second silyl hangs above
/// the molecule, its open valence pointing down into the methyl.
#[test]
fn an_h_placed_inside_the_molecule_is_kept_and_relaxed() {
    let mut ads = AtomicStructure::new();
    add_methoxy(&mut ads, DVec3::ZERO);
    let mut sub = AtomicStructure::new();
    let under = add_silyl(&mut sub, DVec3::new(0.0, 0.0, -1.7), 3);
    let crowded = add_silyl_along(&mut sub, DVec3::new(0.0, 0.0, 4.0), -DVec3::Z, 3);
    let config = passivated(keep_all(SequentialSearch {
        anchor_reach: 1.8,
        clash_filter: false,
        ..Default::default()
    }));
    let (p, r) = run(&ads, &sub, &config);
    let (_, back) = back_maps(&p.setup);
    let c = r
        .candidates
        .iter()
        .find(|c| back[&c.formed[0].1] == under)
        .expect("the foot on the silyl under it");
    assert_eq!(c.passivants, 1);
    // The H on the crowded silyl started inside the methyl.
    let crowded = p.setup.substrate_ids[&crowded];
    let start = {
        let h = &p.hypotheses[c.hypothesis];
        let mut s = p
            .setup
            .start_structure(&h.steps, h.seating.as_ref().unwrap());
        let (_, added) = p.setup.passivate(&mut s);
        let at = s.get_atom(added[0]).unwrap().position;
        assert!(s.has_bond_between(added[0], crowded));
        p.setup
            .adsorbate_atoms
            .iter()
            .map(|&id| s.get_atom(id).unwrap().position.distance(at))
            .fold(f64::INFINITY, f64::min)
    };
    assert!(start < 1.2, "the H starts {start:.2} Å from the molecule");
    assert!(c.strain.is_finite());
}

/// `cs_passivant` is on exactly the placed H; with it, inputs carrying 29 tag
/// names hit the tag limit (an error, never a panic), and without it 29 still
/// work.
#[test]
fn the_passivant_tag_marks_the_placed_h_and_counts_toward_the_limit() {
    let (ads, sub) = triangle();
    let (_, r) = run(&ads, &sub, &passivated(triangle_config()));
    for c in &r.candidates {
        let tagged = c.structure.atoms_with_tag(PASSIVANT_TAG);
        assert_eq!(tagged.len(), c.passivants);
        assert!(
            tagged
                .iter()
                .all(|&id| c.structure.get_atom(id).unwrap().atomic_number == H)
        );
    }

    // 20 names on the substrate, 9 on the adsorbate.
    let (mut tagged, mut tagged_ads) = (sub.clone(), ads.clone());
    let ids: Vec<u32> = tagged.atom_ids().copied().take(20).collect();
    for (k, id) in ids.into_iter().enumerate() {
        tagged.add_atom_tag(id, &format!("t{k}")).unwrap();
    }
    let ids: Vec<u32> = tagged_ads.atom_ids().copied().take(9).collect();
    for (k, id) in ids.into_iter().enumerate() {
        tagged_ads.add_atom_tag(id, &format!("a{k}")).unwrap();
    }
    assert_eq!(tagged.tag_names().len() + tagged_ads.tag_names().len(), 29);
    let base = SequentialSearch {
        formed_bonds: Some(3),
        ..triangle_config()
    };
    let distinct_of = |config: &SequentialSearch| {
        let p = plan(&tagged_ads, &tagged, config).unwrap();
        evaluate(&p, config, None)
            .and_then(|r| distinct(&p.setup, &r, &DistinctSettings::default()).map(|_| ()))
    };
    assert!(distinct_of(&base).is_ok());
    assert!(matches!(
        distinct_of(&passivated(base)),
        Err(ChemisorptionError::Tag(_))
    ));
}
