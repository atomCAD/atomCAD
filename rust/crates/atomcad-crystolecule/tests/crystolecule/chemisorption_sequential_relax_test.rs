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
    BondInventory, CHANGED_TAG, ChemisorptionError, TransferDirection, TransferRule,
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

/// The reconstructed dimer atoms of a slab within `radius` of its axis,
/// tagged `dimer`: the facet, without the fixture's bare edges and faces.
fn tag_dimers(slab: &mut AtomicStructure, radius: f64) {
    let ids: Vec<u32> = slab
        .atoms_values()
        .filter(|a| {
            a.position.z.abs() < 0.3
                && a.bonds.len() == 3
                && a.position.truncate().length() < radius
        })
        .map(|a| a.id)
        .collect();
    for id in ids {
        slab.add_atom_tag(id, "dimer").unwrap();
    }
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
    let settings = atomcad_crystolecule::chemisorption::ChemisorptionSearch::default();
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
