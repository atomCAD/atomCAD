//! The sequential chemisorption search, the debug view
//! (`design_chemisorption_sequential.md` §6.5, §11.8): which form a row opens
//! on, the views built from a row's data (seated by geometry alone, relaxed
//! by replay), what they mark, the search shapes, and the paths the CLI names
//! rows by.
//!
//! The shapes are checked against the search's own test (`need_against`), a
//! separate formula: every site inside a drawn shape is one a foot's test
//! accepts, and every accepted site is inside.

use crate::chemisorption_sequential_support::*;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::chemisorption::sequential::debug::{
    BONDED_COLOR, CANDIDATE_COLOR, FOOT_COLOR, NEAR_MISS_COLOR,
};
use atomcad_crystolecule::chemisorption::sequential::*;
use atomcad_crystolecule::field::ScalarField;
use glam::DVec3;
use std::collections::BTreeSet;
use std::sync::OnceLock;

// ============================================================================
// Fixtures
// ============================================================================

/// The tripod over the Si(100) slab at the defaults: plan only.
fn slab_plan() -> &'static (SequentialPlan, SequentialSearch) {
    static PLAN: OnceLock<(SequentialPlan, SequentialSearch)> = OnceLock::new();
    PLAN.get_or_init(|| {
        let ads = posed_stand_in(3, 0.0, DVec3::ZERO).0;
        let sub = si100_slab(5.0, 11.0);
        let config = SequentialSearch::default();
        (plan(&ads, &sub, &config).unwrap(), config)
    })
}

/// The stand-in cage with `legs` feet over a planted silyl under each foot (a
/// bond length straight down, frozen Si), and a decoy site 2.5 Å further out
/// under the feet `decoys` lists — as in the local-phase tests.
fn planted(legs: usize, decoys: &[usize]) -> (AtomicStructure, AtomicStructure) {
    let (ads, feet) = stand_in(legs);
    let b = rest_length(O, SI);
    let mut sub = AtomicStructure::new();
    for (i, &f) in feet.iter().enumerate() {
        let p = ads.get_atom(f).unwrap().position;
        add_silyl(&mut sub, p - DVec3::Z * b, 3);
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
    (ads, sub)
}

struct Run {
    config: SequentialSearch,
    plan: SequentialPlan,
    report: SearchReport,
}

fn run(ads: &AtomicStructure, sub: &AtomicStructure, config: SequentialSearch) -> Run {
    let plan = plan(ads, sub, &config).unwrap();
    let report = evaluate(&plan, &config, None).unwrap();
    Run {
        config,
        plan,
        report,
    }
}

/// Four feet over their planted sites, two decoys, `formed_bonds = 4`, only
/// the best candidate kept: three-leg parents relaxed but not listed, a local
/// level reached by several binding orders, most relaxations outside top N.
fn four() -> &'static Run {
    static RUN: OnceLock<Run> = OnceLock::new();
    RUN.get_or_init(|| {
        let (ads, sub) = planted(4, &[0, 1]);
        run(
            &ads,
            &sub,
            SequentialSearch {
                anchor_reach: 1.8,
                tolerance: 0.0,
                reach: 3.5,
                formed_bonds: Some(4),
                top_n: 1,
                energy_window: f64::INFINITY,
                ..Default::default()
            },
        )
    })
}

fn leg_rows(tree: &SearchTree) -> impl Iterator<Item = u32> + '_ {
    (0..tree.len() as u32).filter(|&r| matches!(tree.row(r).kind, RowKind::Leg { .. }))
}

fn positions(s: &AtomicStructure) -> Vec<(u32, [u64; 3])> {
    let mut v: Vec<_> = s
        .atoms_values()
        .map(|a| (a.id, a.position.to_array().map(f64::to_bits)))
        .collect();
    v.sort_unstable();
    v
}

fn hyp<'a>(plan: &'a SequentialPlan, report: Option<&'a SearchReport>, row: u32) -> &'a Hypothesis {
    let tree = tree_of(plan, report);
    let index = tree.row(shown_row(tree, row)).hypothesis.unwrap() as usize;
    match report {
        Some(r) => r.hypothesis(plan, index),
        None => &plan.hypotheses[index],
    }
}

// ============================================================================
// The tree as the panel lists it
// ============================================================================

#[test]
fn with_duplicates_hidden_each_hypothesis_is_listed_once_under_its_canonical_path() {
    let (p, _) = slab_plan();
    let tree = &p.tree;
    let mut seen = BTreeSet::new();
    let mut stack = vec![0u32];
    let mut hidden_total = 0;
    while let Some(row) = stack.pop() {
        let shown = tree.visible_children(row, false);
        let all = tree.visible_children(row, true);
        let hidden = all.len() - shown.len();
        assert_eq!(tree.row(row).duplicates as usize, hidden, "row {row}");
        hidden_total += hidden;
        for &c in &all {
            if let Some(canonical) = tree.row(c).duplicate_of {
                assert!(!shown.contains(&c));
                assert!(tree.row(canonical).hypothesis.is_some());
            }
        }
        for c in shown {
            if let Some(h) = tree.row(c).hypothesis {
                assert!(seen.insert(h), "hypothesis {h} listed twice");
            }
            stack.push(c);
        }
    }
    assert_eq!(seen.len(), p.hypotheses.len(), "every hypothesis is listed");
    assert_eq!(hidden_total, p.stats.duplicates);
    assert!(hidden_total > 0, "the fixture has duplicates to hide");
}

#[test]
fn every_row_path_finds_its_row() {
    let (p, _) = slab_plan();
    let tree = &p.tree;
    for row in 0..tree.len() as u32 {
        let path = row_path(&p.setup, tree, row);
        assert_eq!(find_row(&p.setup, tree, &path), Ok(row), "path {path}");
    }
    assert_eq!(find_row(&p.setup, tree, ""), Ok(0));
    assert_eq!(find_row(&p.setup, tree, "#7"), Ok(7));
    // Element prefixes, en dashes and spaces are allowed.
    let leg = leg_rows(tree).find(|&r| tree.row(r).legs == 2).unwrap();
    let named = tree
        .path(leg)
        .iter()
        .map(|l| {
            let f = &p.setup.feet[l.foot];
            let s = &p.setup.sites[l.site];
            format!("O{} – Si{}", f.id, s.id)
        })
        .collect::<Vec<_>>()
        .join(", ");
    assert_eq!(find_row(&p.setup, tree, &named), Ok(leg));
    // Errors name what is wrong.
    assert!(find_row(&p.setup, tree, "#99999999").is_err());
    let site = p.setup.sites[0].id;
    assert!(
        find_row(&p.setup, tree, &format!("{site}"))
            .unwrap_err()
            .contains("not a foot")
    );
    let foot = p.setup.feet[0].id;
    assert!(find_row(&p.setup, tree, &format!("{foot}-{site},{foot}")).is_err());
}

// ============================================================================
// Which form a row opens on (§6.5, "Seated or relaxed")
// ============================================================================

#[test]
fn before_a_run_every_leg_row_opens_seated_and_the_root_and_feet_posed() {
    let (p, _) = slab_plan();
    for row in 0..p.tree.len() as u32 {
        let forms = row_forms(p, None, row);
        match p.tree.row(row).kind {
            RowKind::Leg { .. } => {
                assert_eq!(forms.default, DebugForm::Seated);
                assert!(forms.seated && !forms.relaxed);
                assert!(!needs_relaxation(p, None, row, DebugForm::Seated));
            }
            _ => {
                assert_eq!(forms.default, DebugForm::Posed);
                assert!(!forms.seated && !forms.relaxed);
            }
        }
    }
}

#[test]
fn after_a_run_rows_open_on_the_form_the_next_step_uses() {
    // Unfiltered: two-leg candidates are relaxed and have three-leg children.
    let (ads, sub) = planted(3, &[0]);
    let r = run(
        &ads,
        &sub,
        SequentialSearch {
            anchor_reach: 1.8,
            tolerance: 0.0,
            top_n: 100,
            energy_window: f64::INFINITY,
            ..Default::default()
        },
    );
    let (p, rep) = (&r.plan, Some(&r.report));
    let tree = &r.report.tree;
    let mut seen = (0, 0);
    for row in leg_rows(tree).filter(|&x| tree.row(x).duplicate_of.is_none()) {
        let forms = row_forms(p, rep, row);
        let legs = tree.row(row).legs;
        let relaxed = relaxation(&r.report, row).is_some();
        assert_eq!(forms.relaxed, relaxed);
        if legs == 2 && relaxed && !tree.children(row).is_empty() {
            assert_eq!(
                forms.default,
                DebugForm::Seated,
                "a two-leg row with children"
            );
            seen.0 += 1;
        }
        if legs == 3 && relaxed {
            assert_eq!(forms.default, DebugForm::Relaxed, "a relaxed three-leg row");
            seen.1 += 1;
        }
        if !relaxed {
            assert_eq!(
                forms.default,
                DebugForm::Seated,
                "row {row} has no relaxation"
            );
        }
    }
    assert!(seen.0 > 0 && seen.1 > 0, "{seen:?}");

    // Two legs only: a two-leg candidate is a leaf and opens relaxed.
    let r2 = run(
        &ads,
        &sub,
        SequentialSearch {
            formed_bonds: Some(2),
            ..r.config.clone()
        },
    );
    let leaves: Vec<u32> = leg_rows(&r2.report.tree)
        .filter(|&x| relaxation(&r2.report, x).is_some())
        .collect();
    assert!(!leaves.is_empty());
    for row in leaves {
        assert!(r2.report.tree.children(row).is_empty());
        assert_eq!(
            row_forms(&r2.plan, Some(&r2.report), row).default,
            DebugForm::Relaxed
        );
    }
}

#[test]
fn a_clash_pruned_or_budget_cut_row_opens_seated() {
    // The slab tripod, budget 1: one relaxation, the clashing seatings pruned.
    let ads = posed_stand_in(3, 0.0, DVec3::ZERO).0;
    let sub = si100_slab(5.0, 11.0);
    let config = SequentialSearch {
        budget: 1,
        ..Default::default()
    };
    let r = run(&ads, &sub, config);
    let (p, rep) = (&r.plan, Some(&r.report));
    let clash = p
        .hypotheses
        .iter()
        .find(|h| h.seating.as_ref().is_some_and(|s| s.clashes()))
        .expect("a clashing seating");
    let cut = p
        .hypotheses
        .iter()
        .enumerate()
        .find(|(i, h)| {
            h.candidate && !h.seating.as_ref().unwrap().clashes() && !p.to_relax.contains(i)
        })
        .expect("a candidate the budget cut");
    for row in [clash.row, cut.1.row] {
        let forms = row_forms(p, rep, row);
        assert!(!forms.relaxed);
        assert_eq!(forms.default, DebugForm::Seated);
        assert!(debug_view(p, rep, &r.config, row, DebugForm::Relaxed).is_err());
    }
}

// ============================================================================
// Views
// ============================================================================

#[test]
fn a_seated_view_is_the_hypothesis_seating_and_marks_its_bonds() {
    let (p, config) = slab_plan();
    let mut checked = 0;
    for row in leg_rows(&p.tree).step_by(37) {
        let view = debug_view(p, None, config, row, DebugForm::Seated).unwrap();
        let h = hyp(p, None, row);
        let seating = h.seating.clone().unwrap_or_else(|| p.setup.seat(&h.steps));
        let expected = p.setup.start_structure(&h.steps, &seating);
        assert_eq!(
            positions(&view.structure),
            positions(&expected),
            "row {row}"
        );
        assert_eq!(view.row, shown_row(&p.tree, row));
        assert!(view.strain.is_none());
        for &(f, s) in &h.formed {
            assert!(view.marks.bonded.contains(&f) && view.marks.bonded.contains(&s));
            assert_eq!(view.structure.decorator().atom_color[&s], BONDED_COLOR);
        }
        let clashing: BTreeSet<u32> = seating.clashes.iter().flat_map(|&(a, s)| [a, s]).collect();
        assert!(clashing.iter().all(|id| view.marks.clashing.contains(id)));
        checked += 1;
    }
    assert!(checked > 10);
}

#[test]
fn the_root_view_marks_every_foot_and_its_anchor_sites_and_follows_anchor_reach() {
    let (p, config) = slab_plan();
    let view = root_view(p, config);
    assert_eq!(view.form, DebugForm::Posed);
    assert_eq!(
        positions(&view.structure),
        positions(&p.setup.combined),
        "posed: nothing moved"
    );
    let feet: Vec<u32> = p.setup.feet.iter().map(|f| f.id).collect();
    assert_eq!(view.marks.feet, feet);
    for f in &feet {
        assert_eq!(view.structure.decorator().atom_color[f], FOOT_COLOR);
        assert!(view.structure.decorator().atom_label.contains_key(f));
    }
    // The anchor sites, from the definition rather than the tree.
    let anchors = |reach: f64| -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        for (f, _) in p.setup.feet.iter().enumerate() {
            for (s, site) in p.setup.sites.iter().enumerate() {
                let leg = Leg { foot: f, site: s };
                if p.setup.may_bond(leg) && p.setup.anchor_distance(leg) <= reach {
                    out.insert(site.id);
                }
            }
        }
        out
    };
    let marked: BTreeSet<u32> = view
        .marks
        .candidates
        .iter()
        .chain(&view.marks.clashing)
        .chain(&view.marks.mirrored)
        .chain(&view.marks.undecided)
        .copied()
        .collect();
    assert_eq!(marked, anchors(config.anchor_reach));
    // The shapes: an anchor_reach sphere per foot.
    let shapes = view.shapes.as_ref().unwrap();
    assert_eq!(shapes.shapes.len(), feet.len());
    for f in &p.setup.feet {
        assert!(shapes.contains(f.position + DVec3::X * (config.anchor_reach - 0.01)));
        assert!(shapes.sample(f.position) > SHAPE_LEVEL);
    }
    // Unmarked substrate atoms are ghosted; marked ones are not.
    for &id in p.setup.substrate_ids.values() {
        let ghost = view.structure.get_atom(id).unwrap().is_ghost();
        let coloured = view.structure.decorator().atom_color.contains_key(&id);
        assert_eq!(ghost, !coloured, "atom {id}");
    }

    // A wider anchor_reach marks more, and draws bigger spheres.
    let wider = SequentialSearch {
        anchor_reach: config.anchor_reach + 1.0,
        ..config.clone()
    };
    let ads = posed_stand_in(3, 0.0, DVec3::ZERO).0;
    let sub = si100_slab(5.0, 11.0);
    let p2 = plan(&ads, &sub, &wider).unwrap();
    let view2 = root_view(&p2, &wider);
    let marked2: BTreeSet<u32> = view2
        .marks
        .candidates
        .iter()
        .chain(&view2.marks.clashing)
        .chain(&view2.marks.mirrored)
        .chain(&view2.marks.undecided)
        .copied()
        .collect();
    assert_eq!(marked2, anchors(wider.anchor_reach));
    assert!(marked2.len() > marked.len());
    // The old near misses within the extra ångström are now anchors.
    for &(id, miss) in &view.marks.near_misses {
        assert!(miss > 0.0 && miss <= 1.0);
        assert!(marked2.contains(&id), "near miss {id} +{miss}");
    }
}

#[test]
fn a_row_marks_its_children_by_verdict_and_labels_its_near_misses() {
    let (p, config) = slab_plan();
    let tree = &p.tree;
    // A two-leg row whose ring has mirrored children and near misses.
    let row = leg_rows(tree)
        .filter(|&r| tree.row(r).legs == 2 && tree.row(r).duplicate_of.is_none())
        .find(|&r| {
            !tree.near_misses(r).is_empty()
                && tree
                    .children(r)
                    .iter()
                    .any(|&c| child_verdict(p, None, c) == ChildVerdict::Mirrored)
        })
        .expect("a two-leg row with mirrored children and near misses");
    let view = debug_view(p, None, config, row, DebugForm::Seated).unwrap();
    let mut want = [
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
    ];
    for &c in tree.children(row) {
        let RowKind::Leg { site, .. } = tree.row(c).kind else {
            unreachable!()
        };
        let i = match child_verdict(p, None, c) {
            ChildVerdict::Candidate => 0,
            ChildVerdict::Mirrored => 1,
            ChildVerdict::Undecided => 2,
            ChildVerdict::Clash => 3,
        };
        want[i].insert(p.setup.sites[site as usize].id);
    }
    let set = |v: &Vec<u32>| v.iter().copied().collect::<BTreeSet<u32>>();
    assert_eq!(set(&view.marks.candidates), want[0]);
    assert_eq!(set(&view.marks.mirrored), want[1]);
    assert_eq!(set(&view.marks.undecided), want[2]);
    assert!(want[3].is_subset(&set(&view.marks.clashing)));
    // Near misses: one entry per site, the smallest miss, labelled.
    let near: BTreeSet<u32> = tree
        .near_misses(row)
        .iter()
        .map(|n| p.setup.sites[n.site as usize].id)
        .collect();
    let marked: BTreeSet<u32> = view.marks.near_misses.iter().map(|n| n.0).collect();
    assert_eq!(marked, near);
    let d = view.structure.decorator();
    for &(id, miss) in &view.marks.near_misses {
        assert!(d.atom_label[&id].contains(&format!("+{miss:.2}")));
        if !set(&view.marks.candidates).contains(&id)
            && !view.marks.bonded.contains(&id)
            && !set(&view.marks.mirrored).contains(&id)
            && !set(&view.marks.undecided).contains(&id)
            && !set(&view.marks.clashing).contains(&id)
        {
            assert_eq!(d.atom_color[&id], NEAR_MISS_COLOR);
        }
    }
    for id in &want[0] {
        if !view.marks.bonded.contains(id) && !view.marks.clashing.contains(id) {
            assert_eq!(d.atom_color[id], CANDIDATE_COLOR);
        }
    }
    let text = view.describe(&p.setup, tree);
    assert!(text.starts_with(&format!("row #{row}: ")), "{text}");
    assert!(text.contains("shown seated"));
}

/// The drawn shapes are the test: a site lies in one iff some unbonded foot's
/// shell (one leg) or ring (two legs) test accepts it.
#[test]
fn the_sphere_and_ring_shapes_contain_exactly_the_sites_the_test_accepts() {
    let (p, config) = slab_plan();
    let tree = &p.tree;
    let mut checked = [0, 0];
    for row in leg_rows(tree).filter(|&r| tree.row(r).duplicate_of.is_none()) {
        let legs = tree.row(row).legs as usize;
        if legs > 2 || (legs == 2 && row % 5 != 0) {
            continue;
        }
        let view = debug_view(p, None, config, row, DebugForm::Seated).unwrap();
        let shapes = view.shapes.as_ref().expect("a sphere or a ring");
        let path = tree.path(row);
        for (s, site) in p.setup.sites.iter().enumerate() {
            let accepted = (0..p.setup.feet.len())
                .filter(|f| path.iter().all(|l| l.foot != *f))
                .any(|f| p.setup.need_against(&path, Leg { foot: f, site: s }) <= config.tolerance);
            let d = shapes.distance(site.position);
            if d.abs() < 1e-6 {
                continue;
            }
            assert_eq!(d < 0.0, accepted, "row {row}, site {}", site.id);
        }
        checked[legs - 1] += 1;
    }
    assert!(checked[0] > 0 && checked[1] > 0, "{checked:?}");
}

#[test]
fn a_tripod_three_leg_row_draws_no_shapes() {
    let (p, config) = slab_plan();
    let row = leg_rows(&p.tree)
        .find(|&r| p.tree.row(r).legs == 3)
        .unwrap();
    let view = debug_view(p, None, config, row, DebugForm::Seated).unwrap();
    assert!(view.shapes.is_none());
}

// ============================================================================
// Relaxed rows: the kept candidates, and replay outside top N
// ============================================================================

#[test]
fn a_relaxed_row_outside_top_n_is_replayed_to_its_recorded_strain() {
    let r = four();
    let (p, rep) = (&r.plan, Some(&r.report));
    let tree = &r.report.tree;
    assert_eq!(r.report.candidates.len(), 1, "top N 1");
    let kept = &r.report.candidates[0];
    assert!(!needs_relaxation(p, rep, kept.row, DebugForm::Relaxed));
    let view = debug_view(p, rep, &r.config, kept.row, DebugForm::Relaxed).unwrap();
    assert_eq!(positions(&view.structure), positions(&kept.structure));
    assert_eq!(view.strain, Some(kept.strain));

    let mut replayed = [0, 0];
    for x in r.report.relaxed.iter().filter(|x| x.row != kept.row) {
        if tree.row(x.row).duplicate_of.is_some() {
            continue;
        }
        let legs = tree.row(x.row).legs as usize;
        if replayed[legs - 3] >= 2 {
            continue;
        }
        assert!(needs_relaxation(p, rep, x.row, DebugForm::Relaxed));
        let view = debug_view(p, rep, &r.config, x.row, DebugForm::Relaxed).unwrap();
        assert!(
            (view.strain.unwrap() - x.strain).abs() < 1e-6,
            "row {}: {} vs {}",
            x.row,
            view.strain.unwrap(),
            x.strain
        );
        replayed[legs - 3] += 1;
    }
    assert!(
        replayed[0] > 0 && replayed[1] > 0,
        "three-leg parents and four-leg candidates: {replayed:?}"
    );
}

#[test]
fn a_local_row_reached_by_two_binding_orders_shows_the_kept_one() {
    let r = four();
    let (p, rep) = (&r.plan, Some(&r.report));
    let tree = &r.report.tree;
    let dup = leg_rows(tree)
        .find(|&x| tree.row(x).legs == 4 && tree.row(x).duplicate_of.is_some())
        .expect("a four-leg change set reached by two orders");
    let canonical = tree.row(dup).duplicate_of.unwrap();
    assert_ne!(
        tree.path(dup),
        tree.path(canonical),
        "different binding orders"
    );
    let recorded = relaxation(&r.report, canonical).unwrap().strain;
    let view = debug_view(p, rep, &r.config, dup, DebugForm::Relaxed).unwrap();
    assert_eq!(view.row, canonical, "a duplicate shows its canonical row");
    assert!((view.strain.unwrap() - recorded).abs() < 1e-6);
}

#[test]
fn a_local_row_seats_on_its_replayed_parent_and_a_parent_draws_its_reach() {
    let r = four();
    let (p, rep) = (&r.plan, Some(&r.report));
    let tree = &r.report.tree;
    let local = leg_rows(tree)
        .find(|&x| tree.row(x).legs == 4 && tree.row(x).duplicate_of.is_none())
        .unwrap();
    assert!(needs_relaxation(p, rep, local, DebugForm::Seated));
    let view = debug_view(p, rep, &r.config, local, DebugForm::Seated).unwrap();
    let (start, _) = replay_start(p, &r.report, &r.config, local).unwrap();
    assert_eq!(positions(&view.structure), positions(&start));
    assert!(view.shapes.is_none(), "the deepest level searches nothing");

    // Its parent, a relaxed three-leg state: the reach spheres around its
    // relaxed unbonded feet contain every child's site.
    let parent = tree.row(local).parent;
    let view = debug_view(p, rep, &r.config, parent, DebugForm::Relaxed).unwrap();
    let shapes = view.shapes.as_ref().expect("reach spheres");
    assert_eq!(shapes.shapes.len(), 1, "one unbonded foot");
    for &c in tree.children(parent) {
        let RowKind::Leg { site, .. } = tree.row(c).kind else {
            unreachable!()
        };
        let id = p.setup.sites[site as usize].id;
        let at = view.structure.get_atom(id).unwrap().position;
        assert!(shapes.contains(at), "child site {id}");
        assert!(view.marks.candidates.contains(&id) || view.marks.bonded.contains(&id));
    }
    // Seated, the same parent draws no reach: the local phase searches from
    // the relaxed positions.
    let seated = debug_view(p, rep, &r.config, parent, DebugForm::Seated).unwrap();
    assert!(seated.shapes.is_none());
}
