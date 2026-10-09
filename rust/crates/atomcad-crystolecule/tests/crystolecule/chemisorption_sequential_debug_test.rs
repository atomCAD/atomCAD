//! The sequential chemisorption search, the debug view
//! (`design_chemisorption_sequential.md` §6.5 as revised in §18, §11.8): the
//! items of the panel's tree (states and next-foot steps, alternating), which
//! form an item opens on, the views built from a row's data (seated by
//! geometry alone, relaxed by replay), what they mark, the search shapes, and
//! the paths the CLI names items by.
//!
//! The shapes are checked against the search's own test (`need_against`), a
//! separate formula: every site inside a step's shape is one its foot's test
//! accepts, and every accepted site is inside.

use crate::chemisorption_sequential_support::*;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::chemisorption::sequential::debug::{
    ACCEPTED_COLOR, BONDED_COLOR, FADED_ALPHA, FOOT_COLOR,
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

fn canonical_leg_rows(tree: &SearchTree) -> impl Iterator<Item = u32> + '_ {
    leg_rows(tree).filter(|&r| tree.row(r).duplicate_of.is_none())
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

fn set(v: &[u32]) -> BTreeSet<u32> {
    v.iter().copied().collect()
}

/// The sites of a step's legs, from the tree: what its view must mark.
fn step_sites(p: &SequentialPlan, tree: &SearchTree, step: DebugItem) -> BTreeSet<u32> {
    step_legs(tree, step)
        .into_iter()
        .map(|c| match tree.row(c).kind {
            RowKind::Leg { site, .. } => p.setup.sites[site as usize].id,
            _ => unreachable!(),
        })
        .collect()
}

/// A view draws no labels (an atom's name is in its hover tooltip), and fades
/// exactly the unmarked substrate on a step, nothing on a state.
fn assert_decorated(p: &SequentialPlan, view: &DebugView) {
    let d = view.structure.decorator();
    assert!(d.atom_label.is_empty(), "no labels");
    for &id in p.setup.substrate_ids.values() {
        assert!(
            !view.structure.get_atom(id).unwrap().is_ghost(),
            "no ghosts"
        );
        let faded = view.structure.get_atom_alpha(id) < 1.0;
        let want = view.item.is_step() && !d.atom_color.contains_key(&id);
        assert_eq!(faded, want, "atom {id} of {:?}", view.item);
        if faded {
            assert_eq!(view.structure.get_atom_alpha(id), FADED_ALPHA);
        }
    }
}

// ============================================================================
// The tree as the panel lists it
// ============================================================================

#[test]
fn states_and_steps_alternate_and_each_item_knows_its_parent() {
    let (p, _) = slab_plan();
    let tree = &p.tree;
    let mut stack = vec![DebugItem::ROOT];
    let mut seen = [0, 0];
    while let Some(item) = stack.pop() {
        let children = item_children(p, None, item, true);
        for &c in &children {
            assert_eq!(c.is_step(), !item.is_step(), "{item:?} → {c:?}");
            assert_eq!(item_parent(tree, c), Some(item));
            let ancestors = item_ancestors(tree, c);
            assert_eq!(ancestors.first(), Some(&DebugItem::ROOT));
            assert_eq!(ancestors.last(), Some(&c));
            if tree.row(c.row).duplicate_of.is_none() {
                stack.push(c);
            }
        }
        match item.foot {
            Some(f) => {
                // A step's children bond its foot, one more leg than its state.
                for c in children {
                    let r = tree.row(c.row);
                    assert!(matches!(r.kind, RowKind::Leg { foot, .. } if foot == f));
                    assert_eq!(r.legs, tree.row(item.row).legs + 1);
                }
                seen[1] += 1;
            }
            None => {
                // A state's steps are the feet not bonded yet (none on the
                // tripod's three-leg rows: the plan stops there).
                let bonded: Vec<usize> = tree.path(item.row).iter().map(|l| l.foot).collect();
                let want: Vec<u32> = if (tree.row(item.row).legs as usize) < p.max_legs {
                    (0..p.setup.feet.len())
                        .filter(|f| !bonded.contains(f))
                        .map(|f| f as u32)
                        .collect()
                } else {
                    Vec::new()
                };
                let got: Vec<u32> = children.iter().map(|c| c.foot.unwrap()).collect();
                assert_eq!(got, want, "{item:?}");
                seen[0] += 1;
            }
        }
    }
    assert!(seen[0] > 100 && seen[1] > 100, "{seen:?}");
}

#[test]
fn with_duplicates_hidden_each_hypothesis_is_listed_once_under_its_canonical_path() {
    let (p, _) = slab_plan();
    let tree = &p.tree;
    let mut seen = BTreeSet::new();
    let mut stack = vec![DebugItem::ROOT];
    let mut hidden_total = 0;
    while let Some(item) = stack.pop() {
        let shown = item_children(p, None, item, false);
        let all = item_children(p, None, item, true);
        hidden_total += all.len() - shown.len();
        for c in &all {
            if let Some(canonical) = tree.row(c.row).duplicate_of {
                assert!(!shown.contains(c));
                assert!(tree.row(canonical).hypothesis.is_some());
            }
        }
        for c in shown {
            if !c.is_step()
                && let Some(h) = tree.row(c.row).hypothesis
            {
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
fn every_item_path_finds_its_item() {
    let (p, _) = slab_plan();
    let tree = &p.tree;
    let mut checked = [0, 0];
    for row in (0..tree.len() as u32).step_by(7) {
        let item = item_of_row(tree, row);
        let path = item_path(&p.setup, tree, item);
        assert_eq!(find_item(&p.setup, tree, &path), Ok(item), "path {path}");
        for step in item_children(p, None, item, true) {
            if step.is_step() {
                let path = item_path(&p.setup, tree, step);
                assert_eq!(find_item(&p.setup, tree, &path), Ok(step), "path {path}");
                checked[1] += 1;
            }
        }
        checked[0] += 1;
    }
    assert!(checked[1] > 10, "{checked:?}");
    assert_eq!(find_item(&p.setup, tree, ""), Ok(DebugItem::ROOT));
    assert_eq!(find_item(&p.setup, tree, "root"), Ok(DebugItem::ROOT));
    // A foot row's number names the root's step for that foot.
    let foot_row = tree.children(0)[1];
    assert_eq!(
        find_item(&p.setup, tree, &format!("#{foot_row}")),
        Ok(DebugItem::step(0, 1))
    );
    // A foot alone is the root's step; after legs, the step from that state.
    let foot = &p.setup.feet[0];
    assert_eq!(
        find_item(&p.setup, tree, &format!("O{}", foot.id)),
        Ok(DebugItem::step(0, 0))
    );
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
    assert_eq!(find_item(&p.setup, tree, &named), Ok(DebugItem::state(leg)));
    // Errors name what is wrong.
    assert!(find_item(&p.setup, tree, "#99999999").is_err());
    let site = p.setup.sites[0].id;
    assert!(
        find_item(&p.setup, tree, &format!("{site}"))
            .unwrap_err()
            .contains("not a foot")
    );
    assert!(
        find_item(&p.setup, tree, &format!("{},{site}", foot.id))
            .unwrap_err()
            .contains("only the last")
    );
}

// ============================================================================
// Which form an item opens on
// ============================================================================

#[test]
fn before_a_run_legs_and_their_steps_are_seated_and_the_root_and_its_steps_posed() {
    let (p, _) = slab_plan();
    let tree = &p.tree;
    let root = row_forms(p, None, DebugItem::ROOT);
    assert_eq!(root.default, DebugForm::Posed);
    assert!(!root.seated && !root.relaxed);
    for f in next_feet(p, None, 0) {
        assert_eq!(
            row_forms(p, None, DebugItem::step(0, f)).default,
            DebugForm::Posed
        );
    }
    for row in canonical_leg_rows(tree).step_by(11) {
        let forms = row_forms(p, None, DebugItem::state(row));
        assert_eq!(forms.default, DebugForm::Seated);
        assert!(forms.seated && !forms.relaxed);
        assert!(!needs_relaxation(
            p,
            None,
            DebugItem::state(row),
            DebugForm::Seated
        ));
        for f in next_feet(p, None, row) {
            let step = DebugItem::step(row, f);
            let forms = row_forms(p, None, step);
            assert_eq!(forms.default, DebugForm::Seated, "a step reads the seating");
            assert!(forms.seated && !forms.relaxed);
        }
    }
}

#[test]
fn after_a_run_a_leg_opens_relaxed_when_it_was_and_its_steps_keep_the_form_their_test_read() {
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
    let mut seen = 0;
    for row in canonical_leg_rows(tree) {
        let forms = row_forms(p, rep, DebugItem::state(row));
        let relaxed = relaxation(&r.report, row).is_some();
        assert_eq!(forms.relaxed, relaxed);
        assert_eq!(
            forms.default,
            if relaxed {
                DebugForm::Relaxed
            } else {
                DebugForm::Seated
            },
            "row {row}"
        );
        // A relaxed two-leg row with children opens relaxed now; its step
        // still shows the seating its ring was searched from.
        if relaxed && tree.row(row).legs == 2 && !tree.children(row).is_empty() {
            for f in next_feet(p, rep, row) {
                assert_eq!(
                    row_forms(p, rep, DebugItem::step(row, f)).default,
                    DebugForm::Seated
                );
            }
            seen += 1;
        }
    }
    assert!(seen > 0);
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
        let item = DebugItem::state(row);
        let forms = row_forms(p, rep, item);
        assert!(!forms.relaxed);
        assert_eq!(forms.default, DebugForm::Seated);
        assert!(debug_view(p, rep, &r.config, item, DebugForm::Relaxed).is_err());
    }
}

// ============================================================================
// Views
// ============================================================================

#[test]
fn a_seated_state_is_the_hypothesis_seating_and_marks_only_its_bonds_and_clashes() {
    let (p, config) = slab_plan();
    let mut checked = 0;
    for row in leg_rows(&p.tree).step_by(37) {
        let view = debug_view(p, None, config, DebugItem::state(row), DebugForm::Seated).unwrap();
        let h = hyp(p, None, row);
        let seating = h.seating.clone().unwrap_or_else(|| p.setup.seat(&h.steps));
        let expected = p.setup.start_structure(&h.steps, &seating);
        assert_eq!(
            positions(&view.structure),
            positions(&expected),
            "row {row}"
        );
        assert_eq!(view.item, DebugItem::state(shown_row(&p.tree, row)));
        assert!(view.strain.is_none());
        assert!(view.shapes.is_none(), "a state draws no shapes");
        assert!(view.marks.foot.is_none() && view.marks.accepted.is_empty());
        for &(f, s) in &h.formed {
            assert!(view.marks.bonded.contains(&f) && view.marks.bonded.contains(&s));
            assert_eq!(view.structure.decorator().atom_color[&s], BONDED_COLOR);
        }
        let clashing: BTreeSet<u32> = seating.clashes.iter().flat_map(|&(a, s)| [a, s]).collect();
        assert_eq!(set(&view.marks.clashing), clashing);
        // Everything else keeps its element colour.
        let marked: BTreeSet<u32> = set(&view.marks.bonded).union(&clashing).copied().collect();
        let coloured: BTreeSet<u32> = view
            .structure
            .decorator()
            .atom_color
            .keys()
            .copied()
            .collect();
        assert_eq!(coloured, marked);
        assert_decorated(p, &view);
        checked += 1;
    }
    assert!(checked > 10);
}

#[test]
fn the_root_view_marks_nothing_and_its_steps_mark_one_foot_and_its_anchors() {
    let (p, config) = slab_plan();
    let view = root_view(p, config);
    assert_eq!(view.form, DebugForm::Posed);
    assert_eq!(
        positions(&view.structure),
        positions(&p.setup.combined),
        "posed: nothing moved"
    );
    assert_eq!(view.marks, Marks::default());
    assert!(view.structure.decorator().atom_color.is_empty());
    assert!(view.shapes.is_none());
    assert_decorated(p, &view);

    // The anchor sites of one foot, from the definition rather than the tree.
    let anchors = |f: usize, reach: f64| -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        for (s, site) in p.setup.sites.iter().enumerate() {
            let leg = Leg { foot: f, site: s };
            if p.setup.may_bond(leg) && p.setup.anchor_distance(leg) <= reach {
                out.insert(site.id);
            }
        }
        out
    };
    let feet = next_feet(p, None, 0);
    assert_eq!(feet.len(), p.setup.feet.len(), "every foot is a step");
    let mut steps = Vec::new();
    for &f in &feet {
        let step = DebugItem::step(0, f);
        let view = debug_view(p, None, config, step, DebugForm::Posed).unwrap();
        let foot = &p.setup.feet[f as usize];
        assert_eq!(view.marks.foot, Some(foot.id));
        let d = view.structure.decorator();
        assert_eq!(d.atom_color[&foot.id], FOOT_COLOR);
        assert_eq!(
            set(&view.marks.accepted),
            anchors(f as usize, config.anchor_reach)
        );
        for id in &view.marks.accepted {
            assert_eq!(d.atom_color[id], ACCEPTED_COLOR);
        }
        // The other feet are not marked: they are other steps.
        for other in p.setup.feet.iter().filter(|o| o.id != foot.id) {
            assert!(!d.atom_color.contains_key(&other.id));
        }
        // The shape: this foot's anchor_reach sphere.
        let shapes = view.shapes.as_ref().unwrap();
        assert_eq!(shapes.shapes.len(), 1);
        assert!(shapes.contains(foot.position + DVec3::X * (config.anchor_reach - 0.01)));
        assert!(shapes.sample(foot.position) > SHAPE_LEVEL);
        assert_decorated(p, &view);
        steps.push(view);
    }

    // A wider anchor_reach accepts more, and the old near misses within the
    // extra ångström are now accepted.
    let wider = SequentialSearch {
        anchor_reach: config.anchor_reach + 1.0,
        ..config.clone()
    };
    let ads = posed_stand_in(3, 0.0, DVec3::ZERO).0;
    let sub = si100_slab(5.0, 11.0);
    let p2 = plan(&ads, &sub, &wider).unwrap();
    let (mut before, mut after) = (0, 0);
    for (view, &f) in steps.iter().zip(&feet) {
        let view2 = debug_view(&p2, None, &wider, DebugItem::step(0, f), DebugForm::Posed).unwrap();
        let accepted2 = set(&view2.marks.accepted);
        assert_eq!(accepted2, anchors(f as usize, wider.anchor_reach));
        assert!(accepted2.is_superset(&set(&view.marks.accepted)));
        for &(id, miss) in &view.marks.near_misses {
            assert!(miss > 0.0 && miss <= 1.0);
            assert!(accepted2.contains(&id), "near miss {id} +{miss}");
        }
        before += view.marks.accepted.len();
        after += accepted2.len();
    }
    assert!(after > before, "{before} → {after}");
}

#[test]
fn a_step_marks_every_site_its_test_accepted_whatever_the_verdict() {
    let (p, config) = slab_plan();
    let tree = &p.tree;
    // A step under a two-leg row whose ring has mirrored legs and near misses.
    let step = canonical_leg_rows(tree)
        .filter(|&r| tree.row(r).legs == 2)
        .flat_map(|r| {
            next_feet(p, None, r)
                .into_iter()
                .map(move |f| DebugItem::step(r, f))
        })
        .find(|&s| {
            !step_near_misses(tree, s).is_empty()
                && step_legs(tree, s)
                    .iter()
                    .any(|&c| child_verdict(p, None, c) == ChildVerdict::Mirrored)
        })
        .expect("a ring with mirrored legs and near misses");
    let view = debug_view(p, None, config, step, DebugForm::Seated).unwrap();
    let want = step_sites(p, tree, step);
    assert_eq!(set(&view.marks.accepted), want);
    let d = view.structure.decorator();
    for id in &want {
        assert_eq!(d.atom_color[id], ACCEPTED_COLOR, "site {id}");
    }
    // The state's bonds stay orange; nothing is red on a step.
    let h = hyp(p, None, step.row);
    for &(_, s) in &h.formed {
        if !want.contains(&s) {
            assert_eq!(d.atom_color[&s], BONDED_COLOR);
        }
    }
    assert!(view.marks.clashing.is_empty());
    // Near misses: recorded for the CLI, one per site with the smallest
    // miss, but not drawn — the shape shows them.
    let near: BTreeSet<u32> = step_near_misses(tree, step)
        .iter()
        .map(|n| p.setup.sites[n.site as usize].id)
        .collect();
    let recorded: BTreeSet<u32> = view.marks.near_misses.iter().map(|n| n.0).collect();
    assert_eq!(recorded, near);
    for id in near.difference(&want) {
        if !view.marks.bonded.contains(id) {
            assert!(
                !d.atom_color.contains_key(id),
                "near miss {id} is not coloured"
            );
        }
    }
    assert_decorated(p, &view);
    let text = view.describe(&p.setup, tree);
    assert!(text.starts_with("next foot "), "{text}");
    assert!(text.contains("shown seated"));
    assert!(text.contains("near misses"));
}

/// The drawn shape is the test: a site lies in a step's shape iff its foot's
/// shell (one leg) or ring (two legs) test accepts it.
#[test]
fn a_steps_shell_or_ring_contains_exactly_the_sites_its_foot_accepts() {
    let (p, config) = slab_plan();
    let tree = &p.tree;
    let mut checked = [0, 0];
    for row in canonical_leg_rows(tree) {
        let legs = tree.row(row).legs as usize;
        if legs > 2 || (legs == 2 && row % 5 != 0) {
            continue;
        }
        let path = tree.path(row);
        for f in next_feet(p, None, row) {
            let step = DebugItem::step(row, f);
            let view = debug_view(p, None, config, step, DebugForm::Seated).unwrap();
            let shapes = view.shapes.as_ref().expect("a shell or a ring");
            assert_eq!(shapes.shapes.len(), 1, "one foot, one shape");
            for (s, site) in p.setup.sites.iter().enumerate() {
                let leg = Leg {
                    foot: f as usize,
                    site: s,
                };
                let accepted = p.setup.need_against(&path, leg) <= config.tolerance;
                let d = shapes.distance(site.position);
                if d.abs() < 1e-6 {
                    continue;
                }
                assert_eq!(d < 0.0, accepted, "{step:?}, site {}", site.id);
            }
        }
        checked[legs - 1] += 1;
    }
    assert!(checked[0] > 0 && checked[1] > 0, "{checked:?}");
}

#[test]
fn a_tripod_three_leg_row_has_no_steps() {
    let (p, config) = slab_plan();
    let row = leg_rows(&p.tree)
        .find(|&r| p.tree.row(r).legs == 3)
        .unwrap();
    assert!(next_feet(p, None, row).is_empty());
    assert!(debug_view(p, None, config, DebugItem::step(row, 0), DebugForm::Seated).is_err());
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
    let item = DebugItem::state(kept.row);
    assert!(!needs_relaxation(p, rep, item, DebugForm::Relaxed));
    let view = debug_view(p, rep, &r.config, item, DebugForm::Relaxed).unwrap();
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
        let item = DebugItem::state(x.row);
        assert!(needs_relaxation(p, rep, item, DebugForm::Relaxed));
        let view = debug_view(p, rep, &r.config, item, DebugForm::Relaxed).unwrap();
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
    let view = debug_view(p, rep, &r.config, DebugItem::state(dup), DebugForm::Relaxed).unwrap();
    assert_eq!(
        view.item,
        DebugItem::state(canonical),
        "a duplicate shows its canonical row"
    );
    assert!((view.strain.unwrap() - recorded).abs() < 1e-6);
}

#[test]
fn a_local_row_seats_on_its_replayed_parent_and_the_parents_step_draws_its_reach() {
    let r = four();
    let (p, rep) = (&r.plan, Some(&r.report));
    let tree = &r.report.tree;
    let local = canonical_leg_rows(tree)
        .find(|&x| tree.row(x).legs == 4)
        .unwrap();
    let item = DebugItem::state(local);
    assert!(needs_relaxation(p, rep, item, DebugForm::Seated));
    let view = debug_view(p, rep, &r.config, item, DebugForm::Seated).unwrap();
    let (start, _) = replay_start(p, &r.report, &r.config, local).unwrap();
    assert_eq!(positions(&view.structure), positions(&start));
    assert!(view.shapes.is_none(), "a state draws no shapes");
    assert!(
        next_feet(p, rep, local).is_empty(),
        "the deepest level searches nothing"
    );

    // Its parent, a relaxed three-leg state: the step for the one unbonded
    // foot is shown relaxed, and its reach sphere around that foot's relaxed
    // position contains every leg's site, each marked accepted.
    let parent = tree.row(local).parent;
    let feet = next_feet(p, rep, parent);
    assert_eq!(feet.len(), 1, "one unbonded foot");
    let step = DebugItem::step(parent, feet[0]);
    assert_eq!(row_forms(p, rep, step).default, DebugForm::Relaxed);
    let view = debug_view(p, rep, &r.config, step, DebugForm::Relaxed).unwrap();
    let shapes = view.shapes.as_ref().expect("a reach sphere");
    assert_eq!(shapes.shapes.len(), 1);
    let want = step_sites(p, tree, step);
    assert!(!want.is_empty());
    for id in &want {
        let at = view.structure.get_atom(*id).unwrap().position;
        assert!(shapes.contains(at), "leg site {id}");
    }
    assert_eq!(set(&view.marks.accepted), want);
    assert_decorated(p, &view);
    // A step has its one form: seated, it is refused.
    assert!(debug_view(p, rep, &r.config, step, DebugForm::Seated).is_err());
}
