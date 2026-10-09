//! The sequential chemisorption search, the local phase (legs 4 and later,
//! `design_chemisorption_sequential.md` §4.6, §11.6): the planted binding,
//! what is relaxed, the oracle extended to the local levels, dedupe of binding
//! orders and the canonical parent, replay, the transfer rule per leg, the
//! filters, the budget and determinism.
//!
//! The five-foot run is shared (it relaxes ~240 hypotheses, ~20 s in debug);
//! the tests that change a setting use the four-foot fixture, ~1–3 s each.

use crate::chemisorption_sequential_support::*;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::chemisorption::sequential::*;
use atomcad_crystolecule::chemisorption::{
    BondInventory, BondKind, TransferDirection, TransferRule,
};
use glam::DVec3;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// The stand-in cage with `legs` feet (4: the outer triangle and one inner
/// foot; 6: the hexapod), over a planted site under each foot (a bond length
/// straight down) and a decoy site under the feet `decoys` lists, 2.5 Å
/// further out. Sites are silyls with a frozen Si: the substrate stays put,
/// its H relax. Returns the planted bonds, input ids, sorted.
fn planted(legs: usize, decoys: &[usize]) -> (AtomicStructure, AtomicStructure, Vec<(u32, u32)>) {
    let (ads, feet) = stand_in(legs);
    let b = rest_length(O, SI);
    let mut sub = AtomicStructure::new();
    let mut planted = Vec::new();
    for (i, &f) in feet.iter().enumerate() {
        let p = ads.get_atom(f).unwrap().position;
        planted.push((f, add_silyl(&mut sub, p - DVec3::Z * b, 3)));
        if decoys.contains(&i) {
            let out = DVec3::new(p.x, p.y, 0.0).normalize_or(DVec3::X);
            add_silyl(&mut sub, p - DVec3::Z * b + out * 2.5, 3);
        }
    }
    let si: Vec<u32> = sub
        .atoms_values()
        .filter(|a| a.atomic_number == SI)
        .map(|a| a.id)
        .collect();
    for id in si {
        sub.set_atom_frozen(id, true);
    }
    planted.sort_unstable();
    (ads, sub, planted)
}

/// Leg 1 on the planted sites only, every relaxation kept.
fn config(formed_bonds: Option<usize>) -> SequentialSearch {
    SequentialSearch {
        anchor_reach: 1.8,
        tolerance: 0.0,
        formed_bonds,
        top_n: 100_000,
        energy_window: f64::INFINITY,
        ..Default::default()
    }
}

struct Run {
    config: SequentialSearch,
    plan: SequentialPlan,
    report: SearchReport,
    planted: Vec<(u32, u32)>,
}

fn run(ads: &AtomicStructure, sub: &AtomicStructure, config: SequentialSearch) -> Run {
    let plan = plan(ads, sub, &config).unwrap();
    let report = evaluate(&plan, &config, None).unwrap();
    assert_stats_add_up(&plan);
    audit(&plan, &report, &config);
    assert_tree_consistent_after_run(&plan, &report);
    Run {
        config,
        plan,
        report,
        planted: Vec::new(),
    }
}

/// Five feet, five planted sites, five legs: two local levels (4 and 5).
fn five() -> &'static Run {
    static RUN: OnceLock<Run> = OnceLock::new();
    RUN.get_or_init(|| {
        let (ads, sub, planted) = planted(5, &[]);
        Run {
            planted,
            ..run(&ads, &sub, config(Some(5)))
        }
    })
}

/// The tree after a run, local rows included: every hypothesis is one
/// canonical row whose path is its steps; every duplicate row points at a
/// canonical row of the same bonds, has no children, and is counted on its
/// parent; a row with three or more legs has children only if it was
/// relaxed.
fn assert_tree_consistent_after_run(p: &SequentialPlan, r: &SearchReport) {
    let tree = &r.tree;
    let total = p.hypotheses.len() + r.local.len();
    let mut canonical = 0;
    let relaxed: BTreeSet<u32> = r.relaxed.iter().map(|x| x.row).collect();
    for (i, row) in tree.rows().iter().enumerate() {
        let i = i as u32;
        let path = tree.path(i);
        if let Some(h) = row.hypothesis {
            canonical += 1;
            let h = r.hypothesis(p, h as usize);
            assert_eq!(h.row, i);
            let legs: Vec<Leg> = h.steps.iter().map(|s| s.leg).collect();
            assert_eq!(path, legs, "row {i}");
            assert_eq!(h.legs(), row.legs as usize);
        }
        if let Some(c) = row.duplicate_of {
            assert!(row.hypothesis.is_none());
            assert!(tree.children(i).is_empty(), "a duplicate has no children");
            let target = r.hypothesis(p, tree.row(c).hypothesis.unwrap() as usize);
            let mut mine = path.clone();
            mine.sort();
            let mut theirs: Vec<Leg> = target.steps.iter().map(|s| s.leg).collect();
            theirs.sort();
            assert_eq!(mine, theirs, "a duplicate has the same bonds");
        }
        let dups = tree
            .children(i)
            .iter()
            .filter(|&&c| tree.row(c).duplicate_of.is_some())
            .count();
        assert_eq!(row.duplicates as usize, dups, "row {i}");
        if row.legs as usize >= GEOMETRIC_LEGS && !tree.children(i).is_empty() {
            assert!(relaxed.contains(&i), "row {i} grew without a relaxation");
        }
    }
    assert_eq!(canonical, total);
    let keys: BTreeSet<_> = r.candidates.iter().map(|c| c.key()).collect();
    assert_eq!(
        keys.len(),
        r.candidates.len(),
        "a change set is listed once"
    );
}

fn strains(p: &SequentialPlan, r: &SearchReport) -> BTreeMap<InputChange, f64> {
    r.candidates
        .iter()
        .map(|c| (input_change(&p.setup, &c.key()), c.strain))
        .collect()
}

// ============================================================================
// The planted binding, what is relaxed
// ============================================================================

#[test]
fn the_planted_binding_is_found_by_the_local_phase() {
    let run = five();
    let (p, r) = (&run.plan, &run.report);
    assert!(p.stats.local_phase);
    assert_eq!(p.max_legs, 5);
    assert_eq!(
        r.stats.local.iter().map(|l| l.legs).collect::<Vec<_>>(),
        [4, 5]
    );
    let c = r
        .candidates
        .iter()
        .find(|c| input_change(&p.setup, &c.key()).0 == run.planted)
        .expect("the planted five-leg binding is relaxed and listed");
    assert!(c.converged && !c.seating_clash);
    assert!(
        c.hypothesis >= p.hypotheses.len(),
        "a local-phase hypothesis"
    );
    assert_eq!(r.hypothesis(p, c.hypothesis).row, c.row);
}

/// With `formed_bonds = 5` every shallower state is relaxed only as a parent:
/// the three-leg ones by the plan, the four-leg ones by the first local
/// level. None of them is a candidate.
#[test]
fn parents_outside_the_filter_are_relaxed_but_never_listed() {
    let run = five();
    let (p, r) = (&run.plan, &run.report);
    assert_eq!(p.stats.candidates, 0);
    assert_eq!(p.stats.parents, p.to_relax.len() + p.stats.pruned_clash);
    assert!(p.to_relax.iter().all(|&i| p.hypotheses[i].legs() == 3));
    let level4 = &r.stats.local[0];
    assert!(level4.relaxed > 0 && level4.candidates == 0);
    assert!(r.candidates.iter().all(|c| c.formed.len() == 5));
    assert!(!r.candidates.is_empty());
    // Every three-leg relaxation is a parent of the first local level.
    assert_eq!(level4.parents, p.to_relax.len());
}

#[test]
fn without_a_fourth_leg_there_is_no_local_phase() {
    let (ads, sub, _) = planted(5, &[]);
    for settings in [
        SequentialSearch {
            max_formed_bonds: Some(3),
            ..config(None)
        },
        config(Some(3)),
        SequentialSearch {
            bond_inventory: Some("formed 3× O–Si".parse().unwrap()),
            ..config(None)
        },
    ] {
        let p = plan(&ads, &sub, &settings).unwrap();
        assert!(!p.stats.local_phase && p.max_legs == 3);
        assert_eq!(p.stats.parents, 0);
        assert!(p.hypotheses.iter().all(|h| !h.parent));
    }
    // A tripod never has one.
    let (ads, sub, _) = planted(3, &[]);
    let p = plan(&ads, &sub, &config(None)).unwrap();
    assert!(!p.stats.local_phase && p.max_legs == 3);
}

// ============================================================================
// The oracle, extended to the local levels (§11.6)
// ============================================================================

/// The children a naive scan of one relaxed parent accepts: every unbonded
/// foot against every site within `reach` of the foot's relaxed position,
/// with valence left, the transfer rule and the inventory filter, as
/// (foot, site, acceptor); and its near misses, as (foot, site).
type NaiveChildren = (
    BTreeSet<(usize, usize, Option<usize>)>,
    BTreeSet<(usize, usize)>,
);

fn naive_children(
    setup: &Setup,
    config: &SequentialSearch,
    parent: &Hypothesis,
    relaxed: &AtomicStructure,
) -> NaiveChildren {
    let mut left: Vec<usize> = setup.sites.iter().map(|s| s.valence).collect();
    for s in &parent.steps {
        left[s.leg.site] -= 1;
        if let Some(a) = s.acceptor {
            left[a] -= 1;
        }
    }
    let at = |id: u32| relaxed.get_atom(id).unwrap().position;
    let mut accepted = BTreeSet::new();
    let mut near = BTreeSet::new();
    for (f, foot) in setup.feet.iter().enumerate() {
        if parent.steps.iter().any(|s| s.leg.foot == f) {
            continue;
        }
        for (s, site) in setup.sites.iter().enumerate() {
            if foot.frozen && site.frozen {
                continue;
            }
            let d = at(foot.id).distance(at(site.id));
            if d > config.reach {
                if d <= config.reach + 1.0 && left[s] > 0 {
                    near.insert((f, s));
                }
                continue;
            }
            if left[s] == 0 {
                continue;
            }
            let acceptor = if foot.donates.is_some() {
                let mut l = left.clone();
                l[s] -= 1;
                match setup.acceptor(s, &l) {
                    Some(a) => Some(a),
                    None => continue,
                }
            } else {
                None
            };
            if let Some(target) = &config.bond_inventory {
                let mut inv: BondInventory = parent.inventory.clone();
                *inv.formed
                    .entry(BondKind::new(foot.element, site.element, 1))
                    .or_insert(0) += 1;
                let fits = inv
                    .formed
                    .iter()
                    .all(|(k, n)| *n <= target.formed.get(k).copied().unwrap_or(0));
                if !fits {
                    continue;
                }
            }
            accepted.insert((f, s, acceptor));
        }
    }
    (accepted, near)
}

/// Every local level's paths are exactly what a naive scan of each relaxed
/// parent accepts, and its near misses what one more ångström of `reach`
/// adds. The parents are rebuilt by `replay`, which must reproduce the
/// strain the search recorded for them. A sample of each level's parents
/// (every replay is a relaxation; level 5's replay through their three-leg
/// parents too).
#[test]
fn each_local_level_equals_a_naive_scan_of_its_relaxed_parents() {
    let run = five();
    let (p, r, config) = (&run.plan, &run.report, &run.config);
    let recorded: BTreeMap<u32, f64> = r.relaxed.iter().map(|x| (x.row, x.strain)).collect();
    let tree = &r.tree;
    let parents: Vec<u32> = tree
        .rows()
        .iter()
        .enumerate()
        .filter(|(i, row)| {
            row.hypothesis
                .is_some_and(|h| r.hypothesis(p, h as usize).parent)
                && recorded.contains_key(&(*i as u32))
        })
        .map(|(i, _)| i as u32)
        .collect();
    let level = |legs: u8| -> Vec<u32> {
        parents
            .iter()
            .copied()
            .filter(|&i| tree.row(i).legs == legs)
            .collect()
    };
    let (level4, level5) = (level(3), level(4));
    assert_eq!(level4.len(), r.stats.local[0].parents);
    assert_eq!(level5.len(), r.stats.local[1].parents);
    let sample: Vec<u32> = level4
        .iter()
        .step_by(12)
        .chain(level5.iter().step_by(3))
        .copied()
        .collect();
    let replayed: Vec<Replayed> = sample
        .par_iter()
        .map(|&row| replay(p, r, config, row).unwrap())
        .collect();
    let mut with_children = 0;
    for (row, replayed) in sample.into_iter().zip(replayed) {
        let h = r.hypothesis(p, tree.row(row).hypothesis.unwrap() as usize);
        assert!(
            (replayed.strain - recorded[&row]).abs() < 1e-9,
            "row {row}: replay {} vs recorded {}",
            replayed.strain,
            recorded[&row]
        );
        let (want, want_near) = naive_children(&p.setup, config, h, &replayed.structure);
        let got: BTreeSet<_> = tree
            .children(row)
            .iter()
            .map(|&c| match tree.row(c).kind {
                RowKind::Leg {
                    foot,
                    site,
                    acceptor,
                } => (foot as usize, site as usize, acceptor.map(|a| a as usize)),
                k => panic!("{k:?}"),
            })
            .collect();
        assert_eq!(got, want, "row {row}");
        assert_eq!(got.len(), tree.children(row).len(), "one row per path");
        let near: BTreeSet<_> = tree
            .near_misses(row)
            .iter()
            .map(|m| (m.foot as usize, m.site as usize))
            .collect();
        assert_eq!(near, want_near, "row {row}: near misses");
        with_children += usize::from(!got.is_empty());
    }
    assert!(with_children > 5, "{with_children}");
}

// ============================================================================
// Dedupe of binding orders, the canonical parent, replay
// ============================================================================

/// Two binding orders that reach one change set are both relaxed (their
/// start geometries differ: different relaxed parents); the lower strain is
/// kept, and its parent is the canonical one. Replaying either row gives the
/// strain recorded for that row.
#[test]
fn dedupe_keeps_the_lower_strain_and_its_parent() {
    let run = five();
    let (p, r, config) = (&run.plan, &run.report, &run.config);
    let tree = &r.tree;
    let strain: BTreeMap<u32, f64> = r.relaxed.iter().map(|x| (x.row, x.strain)).collect();
    let mut duplicates = 0;
    for (i, row) in tree.rows().iter().enumerate() {
        let Some(c) = row.duplicate_of else { continue };
        if (row.legs as usize) <= GEOMETRIC_LEGS {
            continue;
        }
        duplicates += 1;
        let (mine, kept) = (strain[&(i as u32)], strain[&c]);
        assert!(
            kept < mine || (kept == mine && c < i as u32),
            "row {i}: kept {kept} over {mine}"
        );
        // The relaxation of a duplicate is recorded under the kept hypothesis.
        let x = r.relaxed.iter().find(|x| x.row == i as u32).unwrap();
        assert_eq!(x.hypothesis, tree.row(c).hypothesis.unwrap() as usize);
    }
    assert_eq!(
        duplicates,
        r.stats.local.iter().map(|l| l.duplicates).sum::<usize>()
    );
    assert!(duplicates > 10);

    // Replay: a duplicate row and its canonical row, at level 5 (two levels
    // of replay each).
    let (i, row) = tree
        .rows()
        .iter()
        .enumerate()
        .find(|(_, row)| row.legs == 5 && row.duplicate_of.is_some())
        .expect("a level-5 duplicate");
    let c = row.duplicate_of.unwrap();
    for row in [i as u32, c] {
        let x = replay(p, r, config, row).unwrap();
        assert!((x.strain - strain[&row]).abs() < 1e-9, "row {row}");
        assert_eq!(x.steps.len(), 5);
    }
    // The canonical row's replay is the listed candidate's structure.
    let candidate = r.candidates.iter().find(|k| k.row == c).unwrap();
    let x = replay(p, r, config, c).unwrap();
    for a in candidate.structure.atoms_values() {
        let b = x.structure.get_atom(a.id).unwrap();
        assert!(a.position.distance(b.position) < 1e-9);
    }
}

// ============================================================================
// The transfer rule, fixed per leg (§5, §11.6)
// ============================================================================

/// The four-foot fixture with an OH inner foot (its H pointing down) and a
/// decoy site under it to take the H: the H rule applies when that foot bonds,
/// in the geometric phase or as the fourth leg. The O feet are tagged, or
/// every C–H carbon would be a donor foot.
fn four_with_oh() -> (AtomicStructure, AtomicStructure) {
    let (mut ads, sub, _) = planted(4, &[3]);
    let (_, feet) = stand_in(4);
    let o = feet[3];
    let p = ads.get_atom(o).unwrap().position;
    let h = ads.add_atom(H, p - DVec3::Z * 0.97 + DVec3::X * 0.2);
    ads.add_bond(o, h, BOND_SINGLE);
    for &f in &feet {
        ads.add_atom_tag(f, "foot").unwrap();
    }
    (ads, sub)
}

#[test]
fn a_child_keeps_its_parents_transfers_and_adds_only_its_own() {
    let (ads, sub) = four_with_oh();
    let config = SequentialSearch {
        adsorbate_tag: Some("foot".into()),
        transfers: vec![TransferRule {
            element: H,
            direction: TransferDirection::ToSubstrate,
        }],
        ..config(Some(4))
    };
    let run = run(&ads, &sub, config);
    let (p, r) = (&run.plan, &run.report);
    let setup = &p.setup;
    assert_eq!(setup.feet.iter().filter(|f| f.donates.is_some()).count(), 1);
    let (mut inherited, mut own) = (0, 0);
    for h in &r.local {
        let parent_row = r.tree.row(h.row).parent;
        let parent = r.hypothesis(p, r.tree.row(parent_row).hypothesis.unwrap() as usize);
        assert_eq!(h.steps[..h.steps.len() - 1], parent.steps[..]);
        let step = *h.steps.last().unwrap();
        let k = parent.transfers.len();
        assert_eq!(h.transfers[..k], parent.transfers[..], "transfers kept");
        match step.acceptor {
            None => assert_eq!(h.transfers.len(), k),
            Some(a) => {
                assert_eq!(h.transfers.len(), k + 1);
                // The rule, as the leg is added: nearest site with valence
                // left after the parent and this bond, never its own site.
                let mut left: Vec<usize> = setup.sites.iter().map(|s| s.valence).collect();
                for s in &h.steps {
                    left[s.leg.site] -= 1;
                }
                for s in &parent.steps {
                    if let Some(a) = s.acceptor {
                        left[a] -= 1;
                    }
                }
                assert_eq!(setup.acceptor(step.leg.site, &left), Some(a));
                assert_eq!(h.transfers[k].acceptor, setup.sites[a].id);
                own += 1;
            }
        }
        if k > 0 {
            inherited += 1;
        }
        // A site an earlier leg's H took is never bonded later.
        for s in &parent.steps {
            if let Some(a) = s.acceptor {
                assert_ne!(step.leg.site, a);
            }
        }
    }
    assert!(own > 0 && inherited > 0, "own {own}, inherited {inherited}");
    // The four-leg candidates carry the transfer whichever level fixed it.
    for c in &r.candidates {
        assert_eq!(c.formed.len(), 4);
        assert_eq!(c.transfers.len(), 1);
        assert_eq!(
            c.bond_inventory.to_string(),
            "formed 1× H–Si, 4× O–Si; broken 1× H–O"
        );
    }
    assert!(!r.candidates.is_empty());
}

/// Two OH feet (the inner foot and the outer one nearest it) whose planted
/// sites share one nearest free site `X`, and a second acceptor `Y`
/// farther from both: whichever OH binds first sends its H to `X`, the
/// other to `Y`.
fn competing_oh() -> (AtomicStructure, AtomicStructure) {
    let (mut ads, mut sub, planted) = planted(4, &[]);
    let (_, feet) = stand_in(4);
    let pos = |s: &AtomicStructure, id: u32| s.get_atom(id).unwrap().position;
    let inner = feet[3];
    let outer = *feet[..3]
        .iter()
        .min_by(|&&a, &&b| {
            let d = |f: u32| pos(&ads, f).distance(pos(&ads, inner));
            d(a).total_cmp(&d(b))
        })
        .unwrap();
    let site = |f: u32| pos(&sub, planted.iter().find(|p| p.0 == f).unwrap().1);
    let (pi, po) = (site(inner), site(outer));
    let mid = (pi + po) / 2.0;
    // Y on the side away from the other planted sites.
    let centre = planted.iter().map(|q| pos(&sub, q.1)).sum::<DVec3>() / planted.len() as f64;
    let mut across = (po - pi).cross(DVec3::Z).normalize();
    if across.dot(mid - centre) < 0.0 {
        across = -across;
    }
    let x = mid - DVec3::Z * 1.8;
    let y = mid + across * 1.9 - DVec3::Z * 0.6;
    // Their free valences point where an atom can arrive: X's sideways, out
    // from under the planted sites (straight up it would aim between them),
    // Y's up.
    for (p, free) in [(x, -across), (y, DVec3::Z)] {
        let si = add_silyl_along(&mut sub, p, free, 3);
        sub.set_atom_frozen(si, true);
    }
    // X is the nearest site to both planted sites, Y the next, both within
    // the default reach.
    for p in [pi, po] {
        let mut others: Vec<f64> = planted
            .iter()
            .map(|q| pos(&sub, q.1))
            .filter(|&q| q != p)
            .map(|q| q.distance(p))
            .collect();
        others.sort_by(f64::total_cmp);
        let (dx, dy) = (x.distance(p), y.distance(p));
        assert!(
            dx < dy && dy < others[0] && dy < 3.0,
            "{dx} {dy} {others:?}"
        );
    }
    for f in [inner, outer] {
        let o = pos(&ads, f);
        let h = ads.add_atom(H, o - DVec3::Z * 0.97 + DVec3::X * 0.2);
        ads.add_bond(f, h, BOND_SINGLE);
    }
    for &f in &feet {
        ads.add_atom_tag(f, "foot").unwrap();
    }
    (ads, sub)
}

/// The binding order decides which OH's H takes the shared acceptor, so two
/// orders of the same bonds are two hypotheses (§4.7), in the local phase as
/// in the geometric one: a local level must not merge them.
#[test]
fn two_binding_orders_with_different_acceptors_stay_two_hypotheses() {
    let (ads, sub) = competing_oh();
    let config = SequentialSearch {
        adsorbate_tag: Some("foot".into()),
        transfers: vec![TransferRule {
            element: H,
            direction: TransferDirection::ToSubstrate,
        }],
        ..config(Some(4))
    };
    let run = run(&ads, &sub, config);
    let (p, r) = (&run.plan, &run.report);
    let mut by_bonds: BTreeMap<Vec<(u32, u32)>, BTreeSet<_>> = BTreeMap::new();
    for h in &r.local {
        let (formed, moves) = input_change(&p.setup, &h.key());
        by_bonds.entry(formed).or_default().insert(moves);
    }
    let split = by_bonds.values().filter(|m| m.len() > 1).count();
    assert!(split > 0, "no bond set reached with two acceptor choices");
    // …and they are listed as two candidates.
    let mut listed: BTreeMap<Vec<(u32, u32)>, usize> = BTreeMap::new();
    for c in &r.candidates {
        *listed
            .entry(input_change(&p.setup, &c.key()).0)
            .or_default() += 1;
    }
    assert!(listed.values().any(|&n| n > 1));
}

/// Numbered feet in the local phase: leg 4 is only ever `foot4`, so nothing
/// is deduplicated, and which competing OH is numbered first decides where
/// the H's go.
#[test]
fn numbered_feet_fix_the_order_in_the_local_phase_too() {
    let (plain, sub) = competing_oh();
    let (_, feet) = stand_in(4);
    let config = SequentialSearch {
        adsorbate_tag: Some("foot".into()),
        transfers: vec![TransferRule {
            element: H,
            direction: TransferDirection::ToSubstrate,
        }],
        ..config(Some(4))
    };
    // The two OH feet: the inner one (`feet[3]`) and the outer one nearest
    // it. Each takes the shared acceptor when it bonds first: inner last,
    // then inner first and the outer OH last.
    let pos = |i: usize| plain.get_atom(feet[i]).unwrap().position;
    let outer = (0..3)
        .min_by(|&a, &b| pos(a).distance(pos(3)).total_cmp(&pos(b).distance(pos(3))))
        .unwrap();
    let rest: Vec<usize> = (0..3).filter(|&i| i != outer).collect();
    let mut moves_by_order = Vec::new();
    for order in [[rest[0], rest[1], outer, 3], [3, rest[0], rest[1], outer]] {
        let mut ads = plain.clone();
        for (n, &i) in order.iter().enumerate() {
            ads.remove_atom_tag(feet[i], "foot");
            ads.add_atom_tag(feet[i], &format!("foot{}", n + 1))
                .unwrap();
        }
        let run = run(&ads, &sub, config.clone());
        let (p, r) = (&run.plan, &run.report);
        assert!(p.setup.ordered);
        assert!(!r.local.is_empty());
        for h in &r.local {
            let feet: Vec<usize> = h.steps.iter().map(|s| s.leg.foot).collect();
            assert_eq!(feet, vec![0, 1, 2, 3]);
        }
        assert_eq!(p.stats.duplicates, 0);
        assert!(r.stats.local.iter().all(|l| l.duplicates == 0));
        // One order: one set of moves per bond set.
        let mut by_bonds: BTreeMap<Vec<(u32, u32)>, BTreeSet<_>> = BTreeMap::new();
        for h in &r.local {
            let (formed, moves) = input_change(&p.setup, &h.key());
            by_bonds.entry(formed).or_default().insert(moves);
        }
        assert!(by_bonds.values().all(|m| m.len() == 1));
        moves_by_order.push(by_bonds);
    }
    let (late, early) = (&moves_by_order[0], &moves_by_order[1]);
    let differ = late
        .iter()
        .filter(|(bonds, m)| early.get(*bonds).is_some_and(|e| e != *m))
        .count();
    assert!(differ > 0, "the order never moved an H");
}

// ============================================================================
// Filters, budget, determinism
// ============================================================================

/// An inventory filter that fixes the depth at four legs gives exactly the
/// `formed_bonds = 4` search: the leg cap comes from it, and the local level
/// prunes nothing more. Unfiltered, the same four-leg candidates are listed
/// beside the shallower ones.
#[test]
fn the_filters_commute_with_the_local_phase() {
    let (ads, sub, _) = planted(4, &[]);
    let by_count = run(&ads, &sub, config(Some(4)));
    let by_inventory = run(
        &ads,
        &sub,
        SequentialSearch {
            bond_inventory: Some("formed 4× O–Si".parse().unwrap()),
            ..config(None)
        },
    );
    assert_eq!(by_inventory.plan.max_legs, 4);
    let four = strains(&by_count.plan, &by_count.report);
    assert!(!four.is_empty());
    assert_eq!(four, strains(&by_inventory.plan, &by_inventory.report));
    let all = run(&ads, &sub, config(None));
    let all_four: BTreeMap<_, _> = strains(&all.plan, &all.report)
        .into_iter()
        .filter(|(k, _)| k.0.len() == 4)
        .collect();
    assert_eq!(all_four, four);
    assert!(all.report.candidates.iter().any(|c| c.formed.len() == 3));
}

/// The budget is spent on the geometric phase first; a local level then
/// relaxes the first paths of its sorted order and reports the truncation.
#[test]
fn the_budget_truncates_a_local_level() {
    let (ads, sub, _) = planted(4, &[]);
    let full = run(&ads, &sub, config(Some(4)));
    let geometric = full.plan.to_relax.len();
    let level = &full.report.stats.local[0];
    assert!(level.to_relax > 2 && !level.truncated);
    let order = |r: &SearchReport| -> Vec<(u32, u64)> {
        r.relaxed[geometric..]
            .iter()
            .map(|x| (x.row, x.strain.to_bits()))
            .collect()
    };
    // The level's order: the children of the lowest-strain parents first.
    let strain: BTreeMap<u32, f64> = full
        .report
        .relaxed
        .iter()
        .map(|x| (x.row, x.strain))
        .collect();
    let parent_strains: Vec<f64> = full.report.relaxed[geometric..]
        .iter()
        .map(|x| strain[&full.report.tree.row(x.row).parent])
        .collect();
    assert!(parent_strains.is_sorted(), "{parent_strains:?}");
    let cut = run(
        &ads,
        &sub,
        SequentialSearch {
            budget: geometric + 2,
            ..config(Some(4))
        },
    );
    let l = &cut.report.stats.local[0];
    assert!(l.truncated && cut.report.stats.truncated && !cut.plan.stats.truncated);
    assert_eq!((l.relaxed, l.to_relax), (2, level.to_relax));
    assert_eq!(order(&cut.report)[..], order(&full.report)[..2]);
    assert_eq!(cut.report.stats.relaxed, geometric + 2);

    // Nothing left for the local phase: no local relaxation, no local
    // candidate, the truncation reported.
    let none = run(
        &ads,
        &sub,
        SequentialSearch {
            budget: geometric,
            ..config(Some(4))
        },
    );
    let l = &none.report.stats.local[0];
    assert!(l.truncated && l.relaxed == 0);
    assert!(none.report.candidates.is_empty());
}

/// No bond joins two frozen atoms in the local phase either: with the inner
/// foot frozen over frozen Si sites, no leg ever uses it, so there is no
/// four-leg binding at all.
#[test]
fn a_frozen_foot_never_bonds_a_frozen_site() {
    let (mut ads, sub, _) = planted(4, &[]);
    let (_, feet) = stand_in(4);
    ads.set_atom_frozen(feet[3], true);
    let run = run(&ads, &sub, config(Some(4)));
    let r = &run.report;
    let l = &r.stats.local[0];
    assert!(l.parents > 0);
    assert_eq!((l.paths, l.hypotheses), (0, 0));
    assert!(r.candidates.is_empty());
}

#[test]
fn one_thread_and_many_give_the_same_local_phase() {
    let (ads, sub, _) = planted(4, &[]);
    let on = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| run(&ads, &sub, config(Some(4))))
    };
    let (a, b) = (on(1), on(4));
    let (ra, rb) = (&a.report, &b.report);
    assert!(!ra.local.is_empty());
    assert_eq!(ra.tree, rb.tree);
    assert_eq!(ra.relaxed, rb.relaxed);
    assert_eq!(ra.local, rb.local);
    assert_eq!(ra.stats.local, rb.stats.local);
    let view = |r: &SearchReport| -> Vec<_> {
        r.candidates
            .iter()
            .map(|c| (c.key(), c.strain.to_bits()))
            .collect()
    };
    assert_eq!(view(ra), view(rb));
}

/// The hexapod over six planted sites, six legs: three local levels (4, 5
/// and 6), ~1,300 relaxations, so release only.
#[test]
#[ignore = "relaxes ~1,300 hypotheses; run in release"]
fn the_planted_hexapod_binding_is_found_three_levels_deep() {
    let (ads, sub, planted) = planted(6, &[]);
    let run = run(&ads, &sub, config(Some(6)));
    let (p, r) = (&run.plan, &run.report);
    assert_eq!(
        r.stats.local.iter().map(|l| l.legs).collect::<Vec<_>>(),
        [4, 5, 6]
    );
    assert!(!r.stats.truncated);
    let c = r
        .candidates
        .iter()
        .find(|c| input_change(&p.setup, &c.key()).0 == planted)
        .expect("the planted six-leg binding");
    assert!(c.converged);
    // Replay through two local parents reproduces it.
    let x = replay(p, r, &run.config, c.row).unwrap();
    assert!((x.strain - c.strain).abs() < 1e-9);
}
