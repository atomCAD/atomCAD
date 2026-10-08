//! The sequential search against the old all-at-once engine, through the
//! golden data captured in Phase 0 while that engine still existed
//! (`chemisorption_golden/old_engine.json`, `design_chemisorption_sequential.md`
//! §11.7, §13.1), and the regression snapshots.
//!
//! The windows are per leg count: a binding is "in the window" when it is
//! within 30 kcal/mol of the best binding of the same number of legs, from
//! the same run. Strains of different leg counts do not compare (§4.8).

use crate::chemisorption_sequential_support::*;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::chemisorption::relax::relax;
use atomcad_crystolecule::chemisorption::sequential::*;
use atomcad_crystolecule::chemisorption::{TransferDirection, TransferRule};
use glam::DVec3;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::PathBuf;

const WINDOW: f64 = 30.0;

const H_TO_SUBSTRATE: TransferRule = TransferRule {
    element: H,
    direction: TransferDirection::ToSubstrate,
};

fn golden() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/crystolecule/chemisorption_golden/old_engine.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("the golden data")).unwrap()
}

/// One old candidate, in input ids.
#[derive(Debug, Clone)]
struct Old {
    pose: usize,
    formed: Vec<(u32, u32)>,
    sites: Vec<DVec3>,
    transfers: Vec<(u32, u32, u32)>,
    strain: f64,
    energy: f64,
}

fn read_run(run: &Value, pose: usize) -> Vec<Old> {
    let v3 = |p: &Value| {
        DVec3::new(
            p[0].as_f64().unwrap(),
            p[1].as_f64().unwrap(),
            p[2].as_f64().unwrap(),
        )
    };
    let id = |v: &Value| v.as_u64().unwrap() as u32;
    run["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| Old {
            pose,
            formed: c["formed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (id(&p[0]), id(&p[1])))
                .collect(),
            sites: c["site_positions"]
                .as_array()
                .unwrap()
                .iter()
                .map(v3)
                .collect(),
            transfers: c["transfers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| (id(&t[0]), id(&t[1]), id(&t[2])))
                .collect(),
            strain: c["strain"].as_f64().unwrap(),
            energy: c["energy"].as_f64().unwrap(),
        })
        .collect()
}

/// The candidates within `WINDOW` of the best of their leg count, by `score`.
fn windowed(all: &[Old], score: impl Fn(&Old) -> f64) -> Vec<Old> {
    let mut best: BTreeMap<usize, f64> = BTreeMap::new();
    for c in all {
        let e = best.entry(c.formed.len()).or_insert(f64::INFINITY);
        *e = e.min(score(c));
    }
    all.iter()
        .filter(|c| score(c) - best[&c.formed.len()] <= WINDOW)
        .cloned()
        .collect()
}

fn ethylene_setup() -> (AtomicStructure, AtomicStructure) {
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    (
        ethanediyl((ps + pp) / 2.0 + DVec3::Z * 2.0, pp - ps).0,
        slab,
    )
}

fn water_setup() -> (AtomicStructure, AtomicStructure) {
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    (water(ps + DVec3::Z * 1.9, pp - ps), slab)
}

// ============================================================================
// The same-pose differential
// ============================================================================

/// From the same pose, with `anchor_reach` at the old `reach` and the default
/// tolerance, every old bond set of two or more legs within the window is
/// among the new hypotheses (the old engine also enumerated geometrically
/// impossible ones, which the shells rightly drop — none of them is in the
/// window).
#[test]
fn the_old_engines_windowed_bindings_are_hypotheses() {
    let g = golden();
    let slab = si100_slab(5.0, 11.0);
    let cases: Vec<(&str, AtomicStructure, AtomicStructure, &Value, f64, usize)> = vec![
        (
            "tripod",
            posed_stand_in(3, 0.0, DVec3::ZERO).0,
            slab.clone(),
            &g["tripod_brute_force"]["poses"][0],
            3.5,
            3,
        ),
        (
            "hexapod",
            posed_stand_in(6, 0.0, DVec3::ZERO).0,
            slab,
            &g["hexapod_pose0"]["run"],
            3.5,
            GEOMETRIC_LEGS,
        ),
        {
            let (a, s) = ethylene_setup();
            ("ethylene", a, s, &g["ethylene"]["run"], 4.5, 2)
        },
    ];
    for (name, ads, sub, run, reach, max_legs) in cases {
        let p = plan(
            &ads,
            &sub,
            &SequentialSearch {
                anchor_reach: reach,
                ..Default::default()
            },
        )
        .unwrap();
        let new = plan_bond_sets(&p);
        let old: Vec<Old> = windowed(&read_run(run, 0), |c| c.strain)
            .into_iter()
            .filter(|c| (2..=max_legs).contains(&c.formed.len()))
            .collect();
        assert!(!old.is_empty(), "{name}");
        for c in &old {
            let mut f = c.formed.clone();
            f.sort_unstable();
            assert!(
                new.contains(&f),
                "{name}: old binding {f:?} (strain {:.1}) missing",
                c.strain
            );
        }
        println!("{name}: {} windowed old bindings, all found", old.len());
    }
}

/// Water, the one fixture with transfers: every old O–site bond is a
/// hypothesis, and its H goes where the rule says — the nearest free site,
/// which on Si(100) is the dimer partner. The old engine tried every
/// acceptor within reach; the rule keeps one per site (D9).
#[test]
fn water_keeps_every_old_binding_with_the_rules_acceptor() {
    let g = golden();
    let (w, slab) = water_setup();
    let config = SequentialSearch {
        anchor_reach: 4.5,
        transfers: vec![H_TO_SUBSTRATE],
        ..Default::default()
    };
    let p = plan(&w, &slab, &config).unwrap();
    let changes = plan_changes(&p);
    let by_bond: HashMap<_, _> = changes
        .iter()
        .cloned()
        .collect::<HashMap<Vec<(u32, u32)>, _>>();
    let old = read_run(&g["water"]["run"], 0);
    let mut collapsed = 0;
    // Pure transfers (an H moved, no bond formed) have no place in a
    // leg-by-leg search (§5).
    for c in windowed(&old, |c| c.strain)
        .into_iter()
        .filter(|c| !c.formed.is_empty())
    {
        let moves = by_bond
            .get(&c.formed)
            .unwrap_or_else(|| panic!("old {:?} missing", c.formed));
        let (_, acceptor) = moves[0];
        assert!(
            slab.has_bond_between(c.formed[0].1, acceptor),
            "the H goes to the dimer partner"
        );
        if (c.transfers[0].0, c.transfers[0].2) != moves[0] {
            collapsed += 1;
        }
    }
    println!(
        "water: {} old candidates, {} new; {collapsed} old acceptors differ from the rule's",
        old.len(),
        changes.len()
    );
}

// ============================================================================
// Coverage against the 20-pose brute force (the acceptance test)
// ============================================================================

/// The in-plane translations of the reconstructed surface: shifts that map
/// every interior dimer atom onto a dimer atom.
fn surface_translations(slab: &AtomicStructure) -> Vec<DVec3> {
    let top: Vec<DVec3> = slab
        .atoms_values()
        .filter(|a| a.position.z.abs() < 0.3 && a.bonds.len() == 3)
        .map(|a| a.position)
        .collect();
    let a0 = top
        .iter()
        .copied()
        .min_by(|a, b| a.truncate().length().total_cmp(&b.truncate().length()))
        .unwrap();
    let inner = |p: DVec3| p.truncate().length() < 9.0;
    let mut out = Vec::new();
    for &b in &top {
        let t = b - a0;
        if t.length() < 0.05 || t.length() > 16.0 {
            continue;
        }
        let mut checked = 0;
        let maps = top.iter().all(|&p| {
            if !inner(p) || !inner(p + t) {
                return true;
            }
            checked += 1;
            top.iter().any(|&q| q.distance(p + t) < 0.05)
        });
        if maps && checked >= 4 {
            out.push(t);
        }
    }
    out
}

/// The stand-in tripod in **one** run finds every binding of two or more legs
/// that the 20-pose brute force of old §8.6 found within the window, up to
/// the slab's lattice translations. Both windows of the spike: the old
/// per-pose one (strain against each pose's own reference) and the per-leg
/// count one (against the separated reference).
#[test]
fn one_run_covers_the_twenty_pose_brute_force() {
    let g = golden();
    let slab = si100_slab(5.0, 11.0);
    let (ads, _) = posed_stand_in(3, 0.0, DVec3::ZERO);
    let p = plan(&ads, &slab, &SequentialSearch::default()).unwrap();
    let translations = surface_translations(&slab);
    assert!(
        translations.len() >= 8,
        "{} translations",
        translations.len()
    );

    // Every new hypothesis as (foot, site position) legs.
    let (back_a, _) = back_maps(&p.setup);
    let new: Vec<BTreeMap<u32, DVec3>> = p
        .hypotheses
        .iter()
        .map(|h| {
            h.formed
                .iter()
                .map(|(a, s)| (back_a[a], p.setup.combined.get_atom(*s).unwrap().position))
                .collect()
        })
        .collect();
    let covered = |c: &Old| {
        let legs: BTreeMap<u32, DVec3> = c
            .formed
            .iter()
            .map(|&(f, _)| f)
            .zip(c.sites.iter().copied())
            .collect();
        new.iter().any(|n| {
            if n.keys().ne(legs.keys()) {
                return false;
            }
            let (f0, p0) = legs.iter().next().unwrap();
            let t = n[f0] - *p0;
            let lattice = t.length() < 0.05
                || translations
                    .iter()
                    .any(|u| u.distance(t) < 0.05 || u.distance(-t) < 0.05);
            lattice && legs.iter().all(|(f, p)| (*p + t).distance(n[f]) < 0.05)
        })
    };

    let mut old = Vec::new();
    for (i, pose) in g["tripod_brute_force"]["poses"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        old.extend(read_run(pose, i));
    }
    // Per pose, against its own reference.
    let mut per_pose = Vec::new();
    for i in 0..20 {
        let of_pose: Vec<Old> = old.iter().filter(|c| c.pose == i).cloned().collect();
        per_pose.extend(windowed(&of_pose, |c| c.strain));
    }
    // Per leg count, against the separated reference.
    let settings = atomcad_crystolecule::chemisorption::ChemisorptionSearch::default();
    let (mut a, mut s) = (ads.clone(), slab.clone());
    let separated =
        relax(&mut a, &settings).unwrap().energy + relax(&mut s, &settings).unwrap().energy;
    let per_legs = windowed(&old, |c| c.energy - separated);

    for (name, set) in [("per pose", per_pose), ("per leg count", per_legs)] {
        let set: Vec<&Old> = set.iter().filter(|c| c.formed.len() >= 2).collect();
        assert!(!set.is_empty());
        let missed: Vec<_> = set
            .iter()
            .filter(|c| !covered(c))
            .map(|c| (c.pose, &c.formed))
            .collect();
        assert!(missed.is_empty(), "{name}: missed {missed:?}");
        println!(
            "{name}: {} windowed brute-force bindings, all covered",
            set.len()
        );
    }
}

// ============================================================================
// Regression snapshots (§11.7)
// ============================================================================

fn stats_text(p: &SequentialPlan) -> String {
    let s = PlanStats {
        seconds: 0.0,
        ..p.stats.clone()
    };
    format!("{s:#?}")
}

fn change_text(
    p: &SequentialPlan,
    key: &atomcad_crystolecule::chemisorption::HypothesisKey,
) -> String {
    let (formed, moves) = input_change(&p.setup, key);
    let f: Vec<String> = formed.iter().map(|(a, s)| format!("{a}–{s}")).collect();
    let m: Vec<String> = moves.iter().map(|(d, a)| format!(" H {d}→{a}")).collect();
    format!("{}{}", f.join(" "), m.concat())
}

fn plan_snapshot(p: &SequentialPlan) -> String {
    let mut out = stats_text(p);
    let _ = writeln!(out, "\nfirst 10 to relax:");
    for &i in p.to_relax.iter().take(10) {
        let _ = writeln!(out, "  {}", change_text(p, &p.hypotheses[i].key()));
    }
    out
}

fn report_snapshot(p: &SequentialPlan, r: &SearchReport) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "relaxed {}, unconverged {}",
        r.stats.relaxed, r.stats.unconverged
    );
    for c in &r.candidates {
        let _ = writeln!(
            out,
            "{:>9.2}  {:<34} {}",
            c.strain,
            c.bond_inventory.to_string(),
            change_text(p, &c.key())
        );
    }
    out
}

#[test]
fn plan_snapshots() {
    let slab = si100_slab(5.0, 11.0);
    for legs in [3, 6] {
        let (ads, _) = posed_stand_in(legs, 0.0, DVec3::ZERO);
        let p = plan(&ads, &slab, &SequentialSearch::default()).unwrap();
        insta::assert_snapshot!(
            format!("sequential_plan_stand_in_{legs}"),
            plan_snapshot(&p)
        );
    }
    let (eth, sub) = ethylene_setup();
    let p = plan(
        &eth,
        &sub,
        &SequentialSearch {
            anchor_reach: 4.5,
            ..Default::default()
        },
    )
    .unwrap();
    insta::assert_snapshot!("sequential_plan_ethylene", plan_snapshot(&p));
    let (w, sub) = water_setup();
    let p = plan(
        &w,
        &sub,
        &SequentialSearch {
            transfers: vec![H_TO_SUBSTRATE],
            ..Default::default()
        },
    )
    .unwrap();
    insta::assert_snapshot!("sequential_plan_water", plan_snapshot(&p));
}

#[test]
fn candidate_snapshots_ethylene_and_water() {
    let (eth, mut sub) = ethylene_setup();
    let ids: Vec<u32> = sub
        .atoms_values()
        .filter(|a| a.position.z.abs() < 0.3 && a.bonds.len() == 3)
        .map(|a| a.id)
        .collect();
    for id in ids {
        sub.add_atom_tag(id, "dimer").unwrap();
    }
    let config = SequentialSearch {
        anchor_reach: 4.5,
        formed_bonds: Some(2),
        substrate_tag: Some("dimer".into()),
        top_n: 8,
        ..Default::default()
    };
    let p = plan(&eth, &sub, &config).unwrap();
    let r = evaluate(&p, &config, None).unwrap();
    insta::assert_snapshot!("sequential_candidates_ethylene", report_snapshot(&p, &r));

    let (w, sub) = water_setup();
    let config = SequentialSearch {
        transfers: vec![H_TO_SUBSTRATE],
        ..Default::default()
    };
    let p = plan(&w, &sub, &config).unwrap();
    let r = evaluate(&p, &config, None).unwrap();
    insta::assert_snapshot!("sequential_candidates_water", report_snapshot(&p, &r));
}

/// The stand-in tripod and hexapod relaxed at the defaults, three legs only:
/// ~900 and ~2,300 relaxations, so release only.
#[test]
#[ignore = "relaxes thousands of hypotheses; run in release"]
fn candidate_snapshots_stand_in() {
    let slab = si100_slab(5.0, 11.0);
    for legs in [3, 6] {
        let (ads, _) = posed_stand_in(legs, 0.0, DVec3::ZERO);
        let config = SequentialSearch {
            formed_bonds: Some(3),
            ..Default::default()
        };
        let p = plan(&ads, &slab, &config).unwrap();
        let r = evaluate(&p, &config, None).unwrap();
        audit(&p, &r, &config);
        insta::assert_snapshot!(
            format!("sequential_candidates_stand_in_{legs}"),
            report_snapshot(&p, &r)
        );
    }
}

/// The stand-in hexapod over the Si(100) facet near it (dimers within 7 Å of
/// the axis), six legs: the local phase on a real surface, each level's
/// statistics and the top candidates. Release only.
#[test]
#[ignore = "relaxes thousands of hypotheses; run in release"]
fn local_phase_snapshot_hexapod() {
    let mut slab = si100_slab(5.0, 11.0);
    tag_dimers(&mut slab, 7.0);
    let (ads, _) = posed_stand_in(6, 0.0, DVec3::ZERO);
    let config = SequentialSearch {
        substrate_tag: Some("dimer".into()),
        formed_bonds: Some(6),
        ..Default::default()
    };
    let p = plan(&ads, &slab, &config).unwrap();
    let r = evaluate(&p, &config, None).unwrap();
    audit(&p, &r, &config);
    let mut out = String::new();
    for l in &r.stats.local {
        let _ = writeln!(out, "{l:?}");
    }
    out.push_str(&report_snapshot(&p, &r));
    insta::assert_snapshot!("sequential_local_hexapod", out);
}
