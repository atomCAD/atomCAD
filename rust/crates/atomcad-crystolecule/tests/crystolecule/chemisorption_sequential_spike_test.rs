//! Phase 0 spike of the sequential chemisorption search
//! (`design_chemisorption_sequential.md` §12, outside the repository). Branch
//! only; every test here is `#[ignore]`d and run by hand in release:
//!
//! ```text
//! cargo test -p atomcad-crystolecule --release --test crystolecule spike_ -- --ignored --nocapture --test-threads 1
//! ```
//!
//! Two halves:
//!
//! - `spike_capture_old_engine_golden` runs the **old** all-at-once engine
//!   while it still exists and writes its bond sets and energies to
//!   `chemisorption_golden/old_engine.json`: the 20-pose brute force of old
//!   §8.6 (stand-in tripod), the hexapod from one pose, ethylene and water.
//! - `spike_sequential_*` prototype the new geometric phase (anchor, sphere,
//!   two-shell ring, local up, mirror check, Kabsch seating, θ rule, clash
//!   detection, separated reference) and measure what §10 asks: coverage
//!   against the brute force up to lattice translations, tolerance
//!   calibration, seating quality, where mirror-pruned and clashing seatings
//!   relax to, bond divergence, and how far hexapod feet 4–6 move.
//!
//! Prototype code, deliberately self-contained: Phase 1 rewrites it as the
//! engine. The transfer rule (§5) is not prototyped; it reads only site
//! positions and settles nothing the spike is asked.

use atomcad_crystolecule::atomic_constants::ATOM_INFO;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::atomic_structure_utils::{auto_create_bonds, remove_single_bond_atoms};
use atomcad_crystolecule::chemisorption::relax::relax;
use atomcad_crystolecule::chemisorption::{
    ChemisorptionSearch, TransferDirection, TransferRule, evaluate, free_valence, plan,
};
use atomcad_crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use atomcad_crystolecule::hydrogen_passivation::{AddHydrogensOptions, add_hydrogens};
use atomcad_crystolecule::lattice_fill::{LatticeFillConfig, LatticeFillOptions, fill_lattice};
use atomcad_crystolecule::mechanosynth::fit::rigid_fit;
use atomcad_crystolecule::simulation::uff::params::{calc_bond_rest_length, get_uff_params};
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_geo_tree::GeoNode;
use atomcad_util::daabox::DAABox;
use glam::{DQuat, DVec3};
use rayon::prelude::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::Instant;

const H: i16 = 1;
const C: i16 = 6;
const O: i16 = 8;
const SI: i16 = 14;

const A_C: f64 = 3.567;
const A_SI: f64 = 5.431;

/// Feet height above the dimer layer, as in old §8.6.
const LIFT: f64 = 1.8;
/// The listing window of old §8.6 (kcal/mol).
const WINDOW: f64 = 30.0;

// ============================================================================
// Fixtures (copied from chemisorption_test.rs: the spike must not drift from
// the fixtures the golden data is captured on)
// ============================================================================

fn diamond_cluster(center: DVec3, radius: f64) -> AtomicStructure {
    let fcc = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(0.0, 0.5, 0.5),
        DVec3::new(0.5, 0.0, 0.5),
        DVec3::new(0.5, 0.5, 0.0),
    ];
    let mut s = AtomicStructure::new();
    for i in -2..=2 {
        for j in -2..=2 {
            for k in -2..=2 {
                let cell = DVec3::new(i as f64, j as f64, k as f64);
                for f in fcc {
                    for b in [DVec3::ZERO, DVec3::splat(0.25)] {
                        let p = (cell + f + b) * A_C;
                        if p.distance(center) <= radius {
                            s.add_atom(C, p);
                        }
                    }
                }
            }
        }
    }
    auto_create_bonds(&mut s);
    remove_single_bond_atoms(&mut s, true);
    s
}

fn stand_in(legs: usize) -> (AtomicStructure, Vec<u32>) {
    let mut s = diamond_cluster(DVec3::splat(A_C * 0.125), 3.6);
    add_hydrogens(&mut s, &AddHydrogensOptions::default());
    let q = DQuat::from_rotation_arc(DVec3::splat(-1.0).normalize(), -DVec3::Z);
    s.transform(&q, &DVec3::ZERO);
    let mut bottom: Vec<(u32, u32, DVec3)> = Vec::new();
    for (&id, a) in s.iter_atoms() {
        if a.atomic_number != C {
            continue;
        }
        for bond in &a.bonds {
            let h = s.get_atom(bond.other_atom_id()).unwrap();
            if h.atomic_number == H && (h.position - a.position).normalize().z < -0.99 {
                bottom.push((id, bond.other_atom_id(), a.position));
            }
        }
    }
    assert_eq!(bottom.len(), 6);
    bottom.sort_by(|a, b| {
        let (ra, rb) = (a.2.truncate().length(), b.2.truncate().length());
        rb.total_cmp(&ra).then(a.0.cmp(&b.0))
    });
    let mut foot_ids = Vec::new();
    for &(_, h, cp) in bottom.iter().take(legs) {
        s.set_atomic_number(h, O);
        s.set_atom_position(h, cp - DVec3::Z * 1.43);
        foot_ids.push(h);
    }
    let foot_z = s.get_atom(foot_ids[0]).unwrap().position.z;
    s.transform(&DQuat::IDENTITY, &DVec3::new(0.0, 0.0, -foot_z));
    (s, foot_ids)
}

fn axis_aligned_box(min: DVec3, max: DVec3) -> GeoNode {
    GeoNode::intersection_3d(vec![
        GeoNode::half_space(DVec3::new(-1.0, 0.0, 0.0), DVec3::new(min.x, 0.0, 0.0)),
        GeoNode::half_space(DVec3::new(1.0, 0.0, 0.0), DVec3::new(max.x, 0.0, 0.0)),
        GeoNode::half_space(DVec3::new(0.0, -1.0, 0.0), DVec3::new(0.0, min.y, 0.0)),
        GeoNode::half_space(DVec3::new(0.0, 1.0, 0.0), DVec3::new(0.0, max.y, 0.0)),
        GeoNode::half_space(DVec3::new(0.0, 0.0, -1.0), DVec3::new(0.0, 0.0, min.z)),
        GeoNode::half_space(DVec3::new(0.0, 0.0, 1.0), DVec3::new(0.0, 0.0, max.z)),
    ])
}

fn si100_slab(cells: f64, mobile_radius: f64) -> AtomicStructure {
    let mut motif = DEFAULT_ZINCBLENDE_MOTIF.clone();
    for p in &mut motif.parameters {
        p.default_atomic_number = SI;
    }
    let max = DVec3::new(cells * A_SI, cells * A_SI, 1.5 * A_SI);
    let config = LatticeFillConfig {
        unit_cell: UnitCellStruct::new(
            DVec3::new(A_SI, 0.0, 0.0),
            DVec3::new(0.0, A_SI, 0.0),
            DVec3::new(0.0, 0.0, A_SI),
        ),
        motif,
        parameter_element_values: HashMap::new(),
        geometry: axis_aligned_box(DVec3::ZERO, max),
        motif_offset: DVec3::ZERO,
        regions: Vec::new(),
    };
    let options = LatticeFillOptions {
        hydrogen_passivation: false,
        remove_unbonded_atoms: true,
        remove_single_bond_atoms: true,
        reconstruct_surface: true,
        invert_phase: false,
        rebond_concave_clashes: true,
        passivation_element: H,
    };
    let region = DAABox::new(DVec3::splat(-5.0), max + DVec3::splat(5.0));
    let mut s = fill_lattice(&config, &options, &region).atomic_structure;
    let top = s
        .atoms_values()
        .filter(|a| a.bonds.len() == 3)
        .map(|a| a.position.z)
        .fold(f64::MIN, f64::max);
    s.transform(
        &DQuat::IDENTITY,
        &DVec3::new(-max.x / 2.0, -max.y / 2.0, -top),
    );
    let ids: Vec<u32> = s.atom_ids().copied().collect();
    for id in ids {
        let p = s.get_atom(id).unwrap().position;
        if p.z < -2.0 || p.truncate().length() > mobile_radius {
            s.set_atom_frozen(id, true);
        }
    }
    s
}

fn central_dimer(slab: &AtomicStructure) -> (u32, u32) {
    let in_layer = |id: u32| {
        let a = slab.get_atom(id).unwrap();
        a.position.z.abs() < 0.3 && a.bonds.len() == 3
    };
    let partner_of = |id: u32| {
        slab.get_atom(id)
            .unwrap()
            .bonds
            .iter()
            .map(|b| b.other_atom_id())
            .find(|&n| in_layer(n))
    };
    let site = slab
        .atom_ids()
        .copied()
        .filter(|&id| in_layer(id) && partner_of(id).is_some())
        .min_by(|&a, &b| {
            let len = |id: u32| slab.get_atom(id).unwrap().position.truncate().length();
            len(a).total_cmp(&len(b)).then(a.cmp(&b))
        })
        .unwrap();
    (site, partner_of(site).unwrap())
}

fn ethanediyl(center: DVec3, axis: DVec3) -> (AtomicStructure, [u32; 2]) {
    let axis = axis.normalize();
    let side = DVec3::Z.cross(axis).normalize();
    let mut s = AtomicStructure::new();
    let mut cs = [0; 2];
    for (i, sign) in [-1.0, 1.0].into_iter().enumerate() {
        let p = center + axis * (sign * 0.77);
        let c = s.add_atom(C, p);
        for side_sign in [-1.0, 1.0] {
            let d = (axis * sign * 0.33 + side * side_sign * 0.82 + DVec3::Z * 0.47).normalize();
            let h = s.add_atom(H, p + d * 1.09);
            s.add_bond(c, h, BOND_SINGLE);
        }
        cs[i] = c;
    }
    s.add_bond(cs[0], cs[1], BOND_SINGLE);
    (s, cs)
}

fn water(o: DVec3, toward: DVec3) -> AtomicStructure {
    let u = DVec3::new(toward.x, toward.y, 0.0).normalize();
    let dir = |deg: f64| {
        let t = deg.to_radians();
        u * t.cos() + DVec3::Z * t.sin()
    };
    let mut s = AtomicStructure::new();
    let oid = s.add_atom(O, o);
    for deg in [-20.0, 84.5] {
        let h = s.add_atom(H, o + dir(deg) * 0.96);
        s.add_bond(oid, h, BOND_SINGLE);
    }
    s
}

/// The 20 poses of old §8.6: 5 rotations × 4 shifts, feet `LIFT` above.
fn poses() -> Vec<(f64, DVec3)> {
    let mut out = Vec::new();
    for rot_deg in [0.0, 30.0, 45.0, 60.0, 90.0] {
        for shift in [
            DVec3::ZERO,
            DVec3::new(1.92, 0.0, 0.0),
            DVec3::new(0.0, 1.92, 0.0),
            DVec3::new(1.36, 1.36, 0.0),
        ] {
            out.push((rot_deg, shift));
        }
    }
    out
}

fn posed_stand_in(legs: usize, rot_deg: f64, shift: DVec3) -> (AtomicStructure, Vec<u32>) {
    let (mut ads, feet) = stand_in(legs);
    let q = DQuat::from_rotation_z(rot_deg.to_radians());
    ads.transform(&q, &(shift + DVec3::Z * LIFT));
    (ads, feet)
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/crystolecule/chemisorption_golden/old_engine.json")
}

fn v3(p: DVec3) -> Value {
    json!([p.x, p.y, p.z])
}

// ============================================================================
// Golden data of the old engine
// ============================================================================

/// Runs the old engine once and returns every relaxed candidate, in input ids.
fn old_engine_run(
    ads: &AtomicStructure,
    slab: &AtomicStructure,
    cfg: &ChemisorptionSearch,
) -> Value {
    let cfg = ChemisorptionSearch {
        top_n: 1_000_000,
        energy_window: f64::INFINITY,
        ..cfg.clone()
    };
    let start = Instant::now();
    let p = plan(ads, slab, &cfg).unwrap();
    let report = evaluate(&p, &cfg, None).unwrap();
    let back_a: HashMap<u32, u32> = p.adsorbate_ids.iter().map(|(&k, &v)| (v, k)).collect();
    let back_s: HashMap<u32, u32> = p.substrate_ids.iter().map(|(&k, &v)| (v, k)).collect();
    let input = |id: u32| back_a.get(&id).or(back_s.get(&id)).copied().unwrap();
    let candidates: Vec<Value> = report
        .candidates
        .iter()
        .map(|c| {
            let mut formed: Vec<(u32, u32)> = c
                .formed
                .iter()
                .map(|&(a, s)| (back_a[&a], back_s[&s]))
                .collect();
            formed.sort_unstable();
            let sites: Vec<Value> = formed
                .iter()
                .map(|&(_, s)| v3(slab.get_atom(s).unwrap().position))
                .collect();
            let transfers: Vec<Value> = c
                .transfers
                .iter()
                .map(|t| json!([input(t.donor), input(t.moved), input(t.acceptor)]))
                .collect();
            json!({
                "formed": formed,
                "site_positions": sites,
                "transfers": transfers,
                "inventory": c.bond_inventory.to_string(),
                "strain": c.strain,
                "energy": c.energy,
                "converged": c.converged,
                "worst_bond_ratio": c.worst_bond_ratio,
            })
        })
        .collect();
    println!(
        "  old engine: {} relaxed in {:.1} s",
        report.stats.relaxed,
        start.elapsed().as_secs_f64()
    );
    json!({
        "reach": cfg.reach,
        "max_iterations": cfg.max_iterations,
        "reference_energy": report.reference.energy,
        "relaxed": report.stats.relaxed,
        "unconverged": report.stats.unconverged,
        "candidates": candidates,
    })
}

#[test]
#[ignore]
fn spike_capture_old_engine_golden() {
    let mut out = serde_json::Map::new();

    // The brute force of old §8.6, pair tolerance gone (it is the default
    // engine now).
    let slab = si100_slab(5.0, 11.0);
    let mut poses_json = Vec::new();
    for (rot_deg, shift) in poses() {
        println!("tripod rot {rot_deg} shift {shift:?}");
        let (ads, _) = posed_stand_in(3, rot_deg, shift);
        let mut run = old_engine_run(&ads, &slab, &ChemisorptionSearch::default());
        run["rot_deg"] = json!(rot_deg);
        run["shift"] = v3(shift);
        poses_json.push(run);
    }
    out.insert(
        "tripod_brute_force".into(),
        json!({
            "fixture": "stand_in(3) over si100_slab(5.0, 11.0), feet 1.8 Å above the dimer layer, \
                        rotated about z then shifted; ids are input ids",
            "poses": poses_json,
        }),
    );

    println!("hexapod pose 0");
    let (ads, _) = posed_stand_in(6, 0.0, DVec3::ZERO);
    let run = old_engine_run(&ads, &slab, &ChemisorptionSearch::default());
    out.insert(
        "hexapod_pose0".into(),
        json!({"fixture": "stand_in(6) over si100_slab(5.0, 11.0), rot 0, shift 0", "run": run}),
    );

    // The two known answers, set up exactly as their tests do.
    let slab9 = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab9);
    let (ps, pp) = (
        slab9.get_atom(site).unwrap().position,
        slab9.get_atom(partner).unwrap().position,
    );
    println!("ethylene");
    let (eth, _) = ethanediyl((ps + pp) / 2.0 + DVec3::Z * 2.0, pp - ps);
    let run = old_engine_run(
        &eth,
        &slab9,
        &ChemisorptionSearch {
            reach: 4.5,
            ..Default::default()
        },
    );
    out.insert(
        "ethylene".into(),
        json!({"fixture": "ethanediyl 2 Å over central_dimer of si100_slab(5.0, 9.0), reach 4.5", "run": run}),
    );
    println!("water");
    let w = water(ps + DVec3::Z * 1.9, pp - ps);
    let run = old_engine_run(
        &w,
        &slab9,
        &ChemisorptionSearch {
            reach: 4.5,
            transfers: vec![TransferRule {
                element: H,
                direction: TransferDirection::ToSubstrate,
            }],
            max_transfers: Some(1),
            ..Default::default()
        },
    );
    out.insert(
        "water".into(),
        json!({"fixture": "water 1.9 Å over central_dimer of si100_slab(5.0, 9.0), reach 4.5, H to_substrate, max 1", "run": run}),
    );

    let path = golden_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&Value::Object(out)).unwrap(),
    )
    .unwrap();
    println!("written {}", path.display());
}

// ============================================================================
// The sequential prototype
// ============================================================================

fn uff_label(z: i16) -> &'static str {
    match z {
        H => "H_",
        C => "C_3",
        O => "O_3",
        SI => "Si3",
        _ => panic!("no spike label for element {z}"),
    }
}

fn rest_length(a: i16, b: i16) -> f64 {
    calc_bond_rest_length(
        1.0,
        get_uff_params(uff_label(a)).unwrap(),
        get_uff_params(uff_label(b)).unwrap(),
    )
}

fn covalent_radius(z: i16) -> f64 {
    ATOM_INFO[&(z as i32)].covalent_radius
}

/// A leg: (foot index, site index).
type Leg = (usize, usize);
/// A hypothesis: its legs, sorted.
type Key = Vec<Leg>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mirror {
    Proper,
    Mirrored,
    Undecided,
}

/// The abstention margin of the mirror check (§4.5), sin 15°.
const MIRROR_MARGIN: f64 = 0.2588;
/// Clash: heavy pair closer than this fraction of the covalent-radius sum.
const CLASH_FRACTION: f64 = 0.6;
/// θ rule step (degrees).
const THETA_STEP: f64 = 10.0;

struct Seq {
    combined: AtomicStructure,
    /// Combined ids of the adsorbate's atoms, and of the substrate's.
    ads: Vec<u32>,
    feet: Vec<u32>,
    foot_pos: Vec<DVec3>,
    foot_el: Vec<i16>,
    sites: Vec<u32>,
    site_pos: Vec<DVec3>,
    site_el: Vec<i16>,
    site_valence: Vec<usize>,
    /// Is the site on the top face (|z| < 1 Å)?
    site_top: Vec<bool>,
    /// Centroid of the adsorbate's heavy atoms other than the feet (posed).
    body: DVec3,
    /// Per foot: adsorbate atoms within two bonds of it (itself included).
    near_foot: Vec<HashSet<u32>>,
    /// Substrate heavy atoms (id, position, covalent radius).
    sub_heavy: Vec<(u32, DVec3, f64)>,
    /// Every substrate atom's position (for the local up).
    sub_all: Vec<DVec3>,
    /// Input id → combined id.
    ads_ids: HashMap<u32, u32>,
    sub_ids: HashMap<u32, u32>,
}

impl Seq {
    fn new(ads_in: &AtomicStructure, sub_in: &AtomicStructure) -> Seq {
        let mut combined = AtomicStructure::new();
        let ads_ids: HashMap<u32, u32> = combined
            .add_atomic_structure(ads_in)
            .unwrap()
            .into_iter()
            .collect();
        let sub_ids: HashMap<u32, u32> = combined
            .add_atomic_structure(sub_in)
            .unwrap()
            .into_iter()
            .collect();
        let mut ads: Vec<u32> = ads_ids.values().copied().collect();
        ads.sort_unstable();
        let mut sub: Vec<u32> = sub_ids.values().copied().collect();
        sub.sort_unstable();
        let atom = |id: u32| combined.get_atom(id).unwrap();

        let feet: Vec<u32> = ads
            .iter()
            .copied()
            .filter(|&id| free_valence(&combined, id) > 0)
            .collect();
        let sites: Vec<u32> = sub
            .iter()
            .copied()
            .filter(|&id| free_valence(&combined, id) > 0)
            .collect();
        let foot_set: HashSet<u32> = feet.iter().copied().collect();
        let heavy_body: Vec<DVec3> = ads
            .iter()
            .filter(|&&id| atom(id).atomic_number != H && !foot_set.contains(&id))
            .map(|&id| atom(id).position)
            .collect();
        let body = heavy_body.iter().copied().sum::<DVec3>() / heavy_body.len() as f64;

        let near_foot = feet
            .iter()
            .map(|&f| {
                let mut seen = HashSet::from([f]);
                let mut queue = VecDeque::from([(f, 0)]);
                while let Some((id, depth)) = queue.pop_front() {
                    if depth == 2 {
                        continue;
                    }
                    for b in &atom(id).bonds {
                        let n = b.other_atom_id();
                        if seen.insert(n) {
                            queue.push_back((n, depth + 1));
                        }
                    }
                }
                seen
            })
            .collect();

        Seq {
            foot_pos: feet.iter().map(|&id| atom(id).position).collect(),
            foot_el: feet.iter().map(|&id| atom(id).atomic_number).collect(),
            site_pos: sites.iter().map(|&id| atom(id).position).collect(),
            site_el: sites.iter().map(|&id| atom(id).atomic_number).collect(),
            site_valence: sites
                .iter()
                .map(|&id| free_valence(&combined, id))
                .collect(),
            site_top: sites
                .iter()
                .map(|&id| atom(id).position.z.abs() < 1.0)
                .collect(),
            sub_heavy: sub
                .iter()
                .filter(|&&id| atom(id).atomic_number != H)
                .map(|&id| {
                    let a = atom(id);
                    (id, a.position, covalent_radius(a.atomic_number))
                })
                .collect(),
            sub_all: sub.iter().map(|&id| atom(id).position).collect(),
            body,
            near_foot,
            feet,
            sites,
            ads,
            ads_ids,
            sub_ids,
            combined,
        }
    }

    fn b(&self, (f, s): Leg) -> f64 {
        rest_length(self.foot_el[f], self.site_el[s])
    }

    /// How much tolerance a pair of legs needs to pass the shell test (0 when
    /// it passes at tolerance 0).
    fn pair_need(&self, a: Leg, b: Leg) -> f64 {
        let d = self.foot_pos[a.0].distance(self.foot_pos[b.0]);
        let bsum = self.b(a) + self.b(b);
        let r = self.site_pos[a.1].distance(self.site_pos[b.1]);
        (r - (d + bsum)).max((d - bsum) - r).max(0.0)
    }

    fn need(&self, key: &[Leg]) -> f64 {
        let mut worst: f64 = 0.0;
        for i in 0..key.len() {
            for j in i + 1..key.len() {
                worst = worst.max(self.pair_need(key[i], key[j]));
            }
        }
        worst
    }

    fn anchored(&self, key: &[Leg], anchor_reach: f64) -> bool {
        key.iter()
            .any(|&(f, s)| self.foot_pos[f].distance(self.site_pos[s]) <= anchor_reach)
    }

    fn valence_ok(&self, key: &[Leg]) -> bool {
        let mut used: HashMap<usize, usize> = HashMap::new();
        let mut feet = HashSet::new();
        for &(f, s) in key {
            if !feet.insert(f) {
                return false;
            }
            let u = used.entry(s).or_insert(0);
            *u += 1;
            if *u > self.site_valence[s] {
                return false;
            }
        }
        true
    }

    /// The geometric phase (§4.2–4.4), up to three legs.
    fn enumerate(&self, anchor_reach: f64, sphere_tol: f64, torus_tol: f64) -> Levels {
        let mut lv = Levels::default();
        for f1 in 0..self.feet.len() {
            for s1 in 0..self.sites.len() {
                if self.foot_pos[f1].distance(self.site_pos[s1]) > anchor_reach {
                    continue;
                }
                lv.anchors += 1;
                for f2 in 0..self.feet.len() {
                    if f2 == f1 {
                        continue;
                    }
                    for s2 in 0..self.sites.len() {
                        let key = sorted(vec![(f1, s1), (f2, s2)]);
                        if !self.valence_ok(&key) {
                            continue;
                        }
                        if self.pair_need((f1, s1), (f2, s2)) > sphere_tol {
                            continue;
                        }
                        lv.sphere_ordered += 1;
                        lv.two.insert(key);
                    }
                }
            }
        }
        for two in &lv.two {
            for f3 in 0..self.feet.len() {
                if two.iter().any(|&(f, _)| f == f3) {
                    continue;
                }
                for s3 in 0..self.sites.len() {
                    let leg = (f3, s3);
                    if self.pair_need(two[0], leg) > torus_tol
                        || self.pair_need(two[1], leg) > torus_tol
                    {
                        continue;
                    }
                    let mut key = two.clone();
                    key.push(leg);
                    let key = sorted(key);
                    if !self.valence_ok(&key) {
                        continue;
                    }
                    lv.torus_ordered += 1;
                    lv.three.insert(key);
                }
            }
        }
        lv
    }

    /// §3: from the centroid of substrate atoms within 5 Å of the sites to
    /// the centroid of the sites.
    fn local_up(&self, key: &[Leg]) -> DVec3 {
        let sites: Vec<DVec3> = key.iter().map(|&(_, s)| self.site_pos[s]).collect();
        let sc = sites.iter().copied().sum::<DVec3>() / sites.len() as f64;
        let near: Vec<DVec3> = self
            .sub_all
            .iter()
            .copied()
            .filter(|p| sites.iter().any(|s| s.distance(*p) <= 5.0))
            .collect();
        let nc = near.iter().copied().sum::<DVec3>() / near.len() as f64;
        (sc - nc).normalize()
    }

    fn mirror(&self, key: &[Leg], up: DVec3) -> Mirror {
        let f: Vec<DVec3> = key.iter().map(|&(f, _)| self.foot_pos[f]).collect();
        let p: Vec<DVec3> = key
            .iter()
            .map(|&l| self.site_pos[l.1] + up * self.b(l))
            .collect();
        let nf = (f[1] - f[0]).cross(f[2] - f[0]);
        let fbar = (f[0] + f[1] + f[2]) / 3.0;
        let v = self.body - fbar;
        let cf = nf.dot(v) / (nf.length() * v.length());
        let ns = (p[1] - p[0]).cross(p[2] - p[0]);
        let cs = ns.dot(up) / ns.length();
        if cf.abs() < MIRROR_MARGIN
            || cs.abs() < MIRROR_MARGIN
            || !cf.is_finite()
            || !cs.is_finite()
        {
            return Mirror::Undecided;
        }
        if cf.signum() == cs.signum() {
            Mirror::Proper
        } else {
            Mirror::Mirrored
        }
    }

    /// The posed position of every adsorbate atom, in `self.ads` order.
    fn posed(&self) -> Vec<DVec3> {
        self.ads
            .iter()
            .map(|&id| self.combined.get_atom(id).unwrap().position)
            .collect()
    }

    /// Clashing heavy pairs of a seating (§4.5), with its exclusions.
    fn clashes(&self, key: &[Leg], seated: &[DVec3]) -> Vec<(u32, u32)> {
        let mut excluded_ads: HashSet<u32> = HashSet::new();
        for &(f, _) in key {
            excluded_ads.extend(self.near_foot[f].iter().copied());
        }
        let excluded_sub: HashSet<u32> = key.iter().map(|&(_, s)| self.sites[s]).collect();
        let centre = key.iter().map(|&(_, s)| self.site_pos[s]).sum::<DVec3>() / key.len() as f64;
        let mut out = Vec::new();
        for (i, &aid) in self.ads.iter().enumerate() {
            let a = self.combined.get_atom(aid).unwrap();
            if a.atomic_number == H || excluded_ads.contains(&aid) {
                continue;
            }
            let ra = covalent_radius(a.atomic_number);
            let pa = seated[i];
            if pa.distance(centre) > 20.0 {
                continue;
            }
            for &(sid, ps, rs) in &self.sub_heavy {
                if excluded_sub.contains(&sid) {
                    continue;
                }
                if pa.distance(ps) < CLASH_FRACTION * (ra + rs) {
                    out.push((aid, sid));
                }
            }
        }
        out
    }

    /// Seats a hypothesis (§4.5): Kabsch for three legs, the θ rule for two.
    fn seat(&self, key: &[Leg]) -> Seating {
        let up = self.local_up(key);
        let posed = self.posed();
        let p: Vec<DVec3> = key
            .iter()
            .map(|&l| self.site_pos[l.1] + up * self.b(l))
            .collect();
        let f: Vec<DVec3> = key.iter().map(|&(f, _)| self.foot_pos[f]).collect();
        if key.len() >= 3 {
            let fit = rigid_fit(&f, &p, false).unwrap();
            let seated: Vec<DVec3> = posed.iter().map(|&x| fit.r * x + fit.t).collect();
            let clashes = self.clashes(key, &seated);
            return Seating {
                positions: seated,
                residual: fit.residual,
                clashes,
                up,
            };
        }
        assert_eq!(key.len(), 2);
        let (mf, mp) = ((f[0] + f[1]) / 2.0, (p[0] + p[1]) / 2.0);
        let axis = (p[1] - p[0]).normalize();
        let q0 = DQuat::from_rotation_arc((f[1] - f[0]).normalize(), axis);
        let base: Vec<DVec3> = posed.iter().map(|&x| q0 * (x - mf) + mp).collect();
        let mut best: Option<(usize, f64, Vec<DVec3>, Vec<(u32, u32)>)> = None;
        let steps = (360.0 / THETA_STEP) as usize;
        for k in 0..steps {
            let q = DQuat::from_axis_angle(axis, (k as f64 * THETA_STEP).to_radians());
            let seated: Vec<DVec3> = base.iter().map(|&x| q * (x - mp) + mp).collect();
            let clashes = self.clashes(key, &seated);
            let height = (seated.iter().copied().sum::<DVec3>() / seated.len() as f64).dot(up);
            let better = match &best {
                None => true,
                Some((n, h, _, _)) => clashes.len() < *n || (clashes.len() == *n && height > *h),
            };
            if better {
                best = Some((clashes.len(), height, seated, clashes));
            }
        }
        let (_, _, seated, clashes) = best.unwrap();
        let residual = key
            .iter()
            .enumerate()
            .map(|(i, &(fi, _))| {
                let idx = self.ads.iter().position(|&id| id == self.feet[fi]).unwrap();
                seated[idx].distance(p[i])
            })
            .fold(0.0, f64::max);
        Seating {
            positions: seated,
            residual,
            clashes,
            up,
        }
    }

    /// The start geometry of a seating: the combined structure with the
    /// adsorbate moved and the legs bonded.
    fn start_structure(&self, key: &[Leg], seating: &Seating) -> AtomicStructure {
        let mut s = self.combined.clone();
        for (i, &id) in self.ads.iter().enumerate() {
            s.set_atom_position(id, seating.positions[i]);
        }
        for &(f, site) in key {
            s.add_bond(self.feet[f], self.sites[site], BOND_SINGLE);
        }
        s
    }

    fn relax_key(&self, key: &[Leg], cfg: &ChemisorptionSearch) -> Relaxed {
        let seating = self.seat(key);
        let mut s = self.start_structure(key, &seating);
        let r = relax(&mut s, cfg).unwrap();
        let pos = |id: u32| s.get_atom(id).unwrap().position;
        let bonds: Vec<DVec3> = key
            .iter()
            .map(|&(f, site)| (pos(self.feet[f]) - pos(self.sites[site])).normalize())
            .collect();
        let mut divergence: f64 = 0.0;
        for i in 0..bonds.len() {
            for j in i + 1..bonds.len() {
                divergence = divergence.max(bonds[i].dot(bonds[j]).clamp(-1.0, 1.0).acos());
            }
        }
        let formed_ratio = key
            .iter()
            .map(|&l| pos(self.feet[l.0]).distance(pos(self.sites[l.1])) / self.b(l))
            .fold(0.0, f64::max);
        Relaxed {
            energy: r.energy,
            converged: r.converged,
            iterations: r.iterations,
            formed_ratio,
            divergence_deg: divergence.to_degrees(),
            residual: seating.residual,
            clashes: seating.clashes.len(),
            foot_positions: self.feet.iter().map(|&f| pos(f)).collect(),
            seated_foot_positions: self
                .feet
                .iter()
                .map(|&f| {
                    let i = self.ads.iter().position(|&id| id == f).unwrap();
                    seating.positions[i]
                })
                .collect(),
        }
    }

    /// E(adsorbate relaxed alone) + E(substrate relaxed alone), §4.8.
    fn separated_reference(
        ads: &AtomicStructure,
        sub: &AtomicStructure,
        cfg: &ChemisorptionSearch,
    ) -> f64 {
        let mut a = ads.clone();
        let mut s = sub.clone();
        relax(&mut a, cfg).unwrap().energy + relax(&mut s, cfg).unwrap().energy
    }

    /// Input-id legs (foot, site) → a key of this setup, if both are a foot
    /// and a site here.
    fn key_of_input(&self, formed: &[(u32, u32)]) -> Option<Key> {
        let mut key = Vec::new();
        for &(f, s) in formed {
            let fc = self.ads_ids[&f];
            let sc = self.sub_ids[&s];
            key.push((
                self.feet.iter().position(|&x| x == fc)?,
                self.sites.iter().position(|&x| x == sc)?,
            ));
        }
        Some(sorted(key))
    }

    fn site_at(&self, p: DVec3) -> Option<usize> {
        self.site_pos.iter().position(|q| q.distance(p) < 0.05)
    }
}

struct Seating {
    positions: Vec<DVec3>,
    residual: f64,
    clashes: Vec<(u32, u32)>,
    #[allow(dead_code)]
    up: DVec3,
}

#[derive(Debug, Clone)]
struct Relaxed {
    energy: f64,
    converged: bool,
    iterations: u32,
    /// Largest formed bond / its rest length.
    formed_ratio: f64,
    divergence_deg: f64,
    residual: f64,
    clashes: usize,
    /// Every foot, relaxed and seated, in `Seq::feet` order.
    foot_positions: Vec<DVec3>,
    seated_foot_positions: Vec<DVec3>,
}

#[derive(Default)]
struct Levels {
    anchors: usize,
    sphere_ordered: usize,
    two: BTreeSet<Key>,
    torus_ordered: usize,
    three: BTreeSet<Key>,
}

fn sorted(mut v: Key) -> Key {
    v.sort_unstable();
    v
}

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
    let mut out: Vec<DVec3> = Vec::new();
    for &b in &top {
        let t = b - a0;
        if t.length() > 16.0 {
            continue;
        }
        let mut checked = 0;
        let ok = top.iter().all(|&p| {
            if !inner(p) || !inner(p + t) {
                return true;
            }
            checked += 1;
            top.iter().any(|&q| q.distance(p + t) < 0.05)
        });
        if ok && checked >= 4 {
            out.push(t);
        }
    }
    out
}

// ============================================================================
// Reading the golden data
// ============================================================================

#[derive(Debug, Clone)]
struct OldCandidate {
    pose: usize,
    formed: Vec<(u32, u32)>,
    site_positions: Vec<DVec3>,
    strain: f64,
    energy: f64,
    converged: bool,
}

fn read_golden() -> Value {
    let text =
        std::fs::read_to_string(golden_path()).expect("run spike_capture_old_engine_golden first");
    serde_json::from_str(&text).unwrap()
}

fn old_candidates(run: &Value, pose: usize) -> Vec<OldCandidate> {
    run["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| OldCandidate {
            pose,
            formed: c["formed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (p[0].as_u64().unwrap() as u32, p[1].as_u64().unwrap() as u32))
                .collect(),
            site_positions: c["site_positions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    DVec3::new(
                        p[0].as_f64().unwrap(),
                        p[1].as_f64().unwrap(),
                        p[2].as_f64().unwrap(),
                    )
                })
                .collect(),
            strain: c["strain"].as_f64().unwrap(),
            energy: c["energy"].as_f64().unwrap(),
            converged: c["converged"].as_bool().unwrap(),
        })
        .collect()
}

fn percentile(v: &mut [f64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * q).round() as usize]
}

fn relax_all(seq: &Seq, keys: &[Key], cfg: &ChemisorptionSearch) -> BTreeMap<Key, Relaxed> {
    keys.par_iter()
        .map(|k| (k.clone(), seq.relax_key(k, cfg)))
        .collect::<Vec<_>>()
        .into_iter()
        .collect()
}

// ============================================================================
// Tripod: coverage, calibration, seating, Q7, divergence
// ============================================================================

#[test]
#[ignore]
fn spike_sequential_tripod() {
    let golden = read_golden();
    let slab = si100_slab(5.0, 11.0);
    let (ads, _) = posed_stand_in(3, 0.0, DVec3::ZERO);
    let seq = Seq::new(&ads, &slab);
    let cfg = ChemisorptionSearch::default();
    let anchor_reach = 3.5;
    println!(
        "setup: {} feet, {} sites ({} on the top face); b(O–Si) = {:.3} Å; foot spacing {:.3} Å",
        seq.feet.len(),
        seq.sites.len(),
        seq.site_top.iter().filter(|&&t| t).count(),
        rest_length(O, SI),
        seq.foot_pos[0].distance(seq.foot_pos[1]),
    );

    // ---- Per-level counts against the tolerance ---------------------------
    println!("\n== per-level counts (anchor_reach {anchor_reach} Å; sphere = torus tolerance) ==");
    println!(
        "{:>5} {:>7} {:>9} {:>7} {:>9} {:>7} {:>7} {:>7} {:>7} {:>7} {:>9}",
        "tol",
        "anchors",
        "sph.ord",
        "2-leg",
        "tor.ord",
        "3-leg",
        "proper",
        "mirror",
        "undec",
        "3 top",
        "2+3 rel"
    );
    for tol in [0.0, 0.5, 1.0, 1.5, 2.0, 3.0] {
        let lv = seq.enumerate(anchor_reach, tol, tol);
        let mut verdicts: BTreeMap<Mirror, usize> = BTreeMap::new();
        for k in &lv.three {
            *verdicts.entry(seq.mirror(k, seq.local_up(k))).or_insert(0) += 1;
        }
        let three_top = lv
            .three
            .iter()
            .filter(|k| k.iter().all(|&(_, s)| seq.site_top[s]))
            .count();
        let get = |m| verdicts.get(&m).copied().unwrap_or(0);
        println!(
            "{:>5.1} {:>7} {:>9} {:>7} {:>9} {:>7} {:>7} {:>7} {:>7} {:>7} {:>9}",
            tol,
            lv.anchors,
            lv.sphere_ordered,
            lv.two.len(),
            lv.torus_ordered,
            lv.three.len(),
            get(Mirror::Proper),
            get(Mirror::Mirrored),
            get(Mirror::Undecided),
            three_top,
            lv.two.len() + get(Mirror::Proper) + get(Mirror::Undecided),
        );
    }
    let two_top = |lv: &Levels| {
        lv.two
            .iter()
            .filter(|k| k.iter().all(|&(_, s)| seq.site_top[s]))
            .count()
    };
    let lv1 = seq.enumerate(anchor_reach, 1.0, 1.0);
    println!(
        "at tol 1.0: two-leg on top-face sites only {} of {}",
        two_top(&lv1),
        lv1.two.len()
    );

    // ---- The brute force, mapped onto this setup --------------------------
    let poses_json = golden["tripod_brute_force"]["poses"].as_array().unwrap();
    let mut old: Vec<OldCandidate> = Vec::new();
    let mut old_relaxed = 0;
    for (i, p) in poses_json.iter().enumerate() {
        old.extend(old_candidates(p, i));
        old_relaxed += p["relaxed"].as_u64().unwrap();
    }
    let e_sep = Seq::separated_reference(&ads, &slab, &cfg);
    println!(
        "\nbrute force: {} candidates over 20 poses ({} relaxations, {} unconverged); separated reference {:.2}",
        old.len(),
        old_relaxed,
        old.iter().filter(|c| !c.converged).count(),
        e_sep
    );
    let translations = surface_translations(&slab);
    println!("surface translations found: {}", translations.len());

    // Per-pose window (old §8.6) and per-leg-count window (separated reference).
    let mut best_pose: HashMap<usize, f64> = HashMap::new();
    for c in &old {
        let e = best_pose.entry(c.pose).or_insert(f64::INFINITY);
        *e = e.min(c.strain);
    }
    let mut best_legs_old: BTreeMap<usize, f64> = BTreeMap::new();
    for c in &old {
        let e = best_legs_old.entry(c.formed.len()).or_insert(f64::INFINITY);
        *e = e.min(c.energy - e_sep);
    }
    println!(
        "brute force best strain (separated reference) per leg count: {:?}",
        best_legs_old
            .iter()
            .map(|(k, v)| format!("{k} legs {v:.1}"))
            .collect::<Vec<_>>()
    );

    // Distinct brute-force patterns (≥ 2 legs), in the pose they came from.
    struct Pattern {
        legs: Vec<(u32, DVec3)>,
        in_pose_window: bool,
        in_leg_window: bool,
        best_energy: f64,
        poses: BTreeSet<usize>,
    }
    let mut patterns: Vec<Pattern> = Vec::new();
    for c in old.iter().filter(|c| c.formed.len() >= 2) {
        let legs: Vec<(u32, DVec3)> = c
            .formed
            .iter()
            .zip(&c.site_positions)
            .map(|(&(f, _), &p)| (f, p))
            .collect();
        let in_pose = c.strain - best_pose[&c.pose] <= WINDOW;
        let in_legs = c.energy - e_sep - best_legs_old[&c.formed.len()] <= WINDOW;
        // Same pattern up to a translation?
        let found = patterns.iter_mut().find(|p| {
            p.legs.len() == legs.len()
                && legs.iter().all(|(f, _)| p.legs.iter().any(|(g, _)| g == f))
                && {
                    let t = p.legs[0].1 - legs.iter().find(|(f, _)| *f == p.legs[0].0).unwrap().1;
                    p.legs.iter().all(|(g, q)| {
                        let mine = legs.iter().find(|(f, _)| f == g).unwrap().1;
                        (mine + t).distance(*q) < 0.05
                    }) && (t.length() < 0.05
                        || translations
                            .iter()
                            .any(|u| u.distance(t) < 0.05 || u.distance(-t) < 0.05))
                }
        });
        match found {
            Some(p) => {
                p.in_pose_window |= in_pose;
                p.in_leg_window |= in_legs;
                p.best_energy = p.best_energy.min(c.energy);
                p.poses.insert(c.pose);
            }
            None => patterns.push(Pattern {
                legs,
                in_pose_window: in_pose,
                in_leg_window: in_legs,
                best_energy: c.energy,
                poses: BTreeSet::from([c.pose]),
            }),
        }
    }
    println!(
        "distinct brute-force patterns of ≥2 legs (up to translation): {} ({} 2-leg, {} 3-leg); in the per-pose window {}, in the per-leg-count window {}",
        patterns.len(),
        patterns.iter().filter(|p| p.legs.len() == 2).count(),
        patterns.iter().filter(|p| p.legs.len() == 3).count(),
        patterns.iter().filter(|p| p.in_pose_window).count(),
        patterns.iter().filter(|p| p.in_leg_window).count(),
    );

    // Each pattern's anchored translates in this setup, and what tolerance
    // its pairs need (translation invariant).
    let mut all_t = translations.clone();
    all_t.push(DVec3::ZERO);
    let foot_index = |input: u32| {
        let c = seq.ads_ids[&input];
        seq.feet.iter().position(|&x| x == c).unwrap()
    };
    let translates = |p: &Pattern| -> Vec<Key> {
        let mut out = BTreeSet::new();
        for &t in &all_t {
            let key: Option<Key> = p
                .legs
                .iter()
                .map(|&(f, q)| seq.site_at(q + t).map(|s| (foot_index(f), s)))
                .collect();
            if let Some(key) = key {
                let key = sorted(key);
                if seq.valence_ok(&key) && seq.anchored(&key, anchor_reach) {
                    out.insert(key);
                }
            }
        }
        out.into_iter().collect()
    };
    println!("\n== coverage of the brute force (one sequential run from pose 0) ==");
    println!("anchor_reach  tol   per-pose window   per-leg window   all patterns");
    for ar in [3.5, 4.5, 5.5] {
        for tol in [0.0, 0.5, 1.0, 2.0] {
            let lv = seq.enumerate(ar, tol, tol);
            let mut cov = [(0, 0); 3];
            for p in &patterns {
                let anchored_translates: Vec<Key> = {
                    let mut out = BTreeSet::new();
                    for &t in &all_t {
                        let key: Option<Key> = p
                            .legs
                            .iter()
                            .map(|&(f, q)| seq.site_at(q + t).map(|s| (foot_index(f), s)))
                            .collect();
                        if let Some(key) = key {
                            let key = sorted(key);
                            if seq.valence_ok(&key) && seq.anchored(&key, ar) {
                                out.insert(key);
                            }
                        }
                    }
                    out.into_iter().collect()
                };
                let hit = anchored_translates
                    .iter()
                    .any(|k| lv.two.contains(k) || lv.three.contains(k));
                // Cross-check the enumerator against the analytic test.
                let analytic = anchored_translates.iter().any(|k| seq.need(k) <= tol);
                assert_eq!(hit, analytic, "enumerator and analytic test disagree");
                for (i, sel) in [p.in_pose_window, p.in_leg_window, true]
                    .into_iter()
                    .enumerate()
                {
                    if sel {
                        cov[i].1 += 1;
                        if hit {
                            cov[i].0 += 1;
                        }
                    }
                }
            }
            println!(
                "{:>8.1} {:>7.1}   {:>6}/{:<6}      {:>6}/{:<6}    {:>6}/{:<6}",
                ar, tol, cov[0].0, cov[0].1, cov[1].0, cov[1].1, cov[2].0, cov[2].1
            );
        }
    }
    // Which patterns are missed at the defaults, and why.
    let lv = seq.enumerate(anchor_reach, 1.0, 1.0);
    let mut needs: Vec<f64> = Vec::new();
    for p in patterns
        .iter()
        .filter(|p| p.in_leg_window || p.in_pose_window)
    {
        let ts = translates(p);
        let hit = ts
            .iter()
            .any(|k| lv.two.contains(k) || lv.three.contains(k));
        let need = ts.iter().map(|k| seq.need(k)).fold(f64::INFINITY, f64::min);
        // The need of the pattern itself (translation invariant), from its
        // own geometry: feet of this setup, sites as found.
        let own_need = {
            let mut worst: f64 = 0.0;
            for i in 0..p.legs.len() {
                for j in i + 1..p.legs.len() {
                    let (fi, qi) = p.legs[i];
                    let (fj, qj) = p.legs[j];
                    let (a, b) = (foot_index(fi), foot_index(fj));
                    let d = seq.foot_pos[a].distance(seq.foot_pos[b]);
                    let bsum = 2.0 * rest_length(O, SI);
                    let r = qi.distance(qj);
                    worst = worst.max((r - (d + bsum)).max((d - bsum) - r).max(0.0));
                }
            }
            worst
        };
        needs.push(own_need);
        if !hit {
            println!(
                "  missed: {} legs, energy {:.1} (sep. strain {:.1}), poses {:?}, anchored translates {}, need {:.2} Å (own {:.2} Å)",
                p.legs.len(),
                p.best_energy,
                p.best_energy - e_sep,
                p.poses,
                ts.len(),
                need,
                own_need
            );
        }
    }
    println!(
        "tolerance needed by the windowed patterns: max {:.2} Å, p90 {:.2} Å",
        needs.iter().copied().fold(0.0, f64::max),
        percentile(&mut needs, 0.9)
    );

    // ---- Same-pose differential (pose 0, no translation) -----------------
    let pose0: Vec<&OldCandidate> = old
        .iter()
        .filter(|c| c.pose == 0 && c.formed.len() >= 2 && c.strain - best_pose[&0] <= WINDOW)
        .collect();
    let missing0: Vec<_> = pose0
        .iter()
        .filter(|c| {
            let k = seq.key_of_input(&c.formed).unwrap();
            !(lv.two.contains(&k) || lv.three.contains(&k))
        })
        .collect();
    println!(
        "\nsame-pose differential (pose 0, ≥2 legs within window): {} of {} among the new hypotheses",
        pose0.len() - missing0.len(),
        pose0.len()
    );

    // ---- Relax everything at the default tolerance -----------------------
    let mut verdict: BTreeMap<Key, Mirror> = BTreeMap::new();
    for k in &lv.three {
        verdict.insert(k.clone(), seq.mirror(k, seq.local_up(k)));
    }
    let mut keys: Vec<Key> = lv.two.iter().cloned().collect();
    keys.extend(lv.three.iter().cloned()); // mirrored too, for Q7
    println!(
        "\nrelaxing {} hypotheses (incl. the mirror-pruned, for Q7)…",
        keys.len()
    );
    let start = Instant::now();
    let relaxed = relax_all(&seq, &keys, &cfg);
    println!("  {:.1} s", start.elapsed().as_secs_f64());

    let strain = |r: &Relaxed| r.energy - e_sep;
    let accepted = |k: &Key| k.len() == 2 || verdict[k] != Mirror::Mirrored;
    let mut best_seq: BTreeMap<usize, f64> = BTreeMap::new();
    for (k, r) in &relaxed {
        if accepted(k) {
            let e = best_seq.entry(k.len()).or_insert(f64::INFINITY);
            *e = e.min(strain(r));
        }
    }
    println!(
        "sequential best strain (separated) per leg count: {:?}",
        best_seq
            .iter()
            .map(|(k, v)| format!("{k} legs {v:.1}"))
            .collect::<Vec<_>>()
    );
    println!(
        "relaxations: sequential {} (2-leg {} + 3-leg after mirror {}) vs brute force {} over 20 poses ({:.0}/pose)",
        relaxed.keys().filter(|k| accepted(k)).count(),
        lv.two.len(),
        relaxed
            .keys()
            .filter(|k| k.len() == 3 && accepted(k))
            .count(),
        old_relaxed,
        old_relaxed as f64 / 20.0
    );

    // ---- Q3: seating quality ---------------------------------------------
    println!("\n== Q3 seating quality (accepted hypotheses) ==");
    for legs in [2, 3] {
        let rs: Vec<(&Key, &Relaxed)> = relaxed
            .iter()
            .filter(|(k, _)| k.len() == legs && accepted(k))
            .collect();
        let window: Vec<&(&Key, &Relaxed)> = rs
            .iter()
            .filter(|(_, r)| strain(r) - best_seq[&legs] <= WINDOW)
            .collect();
        let mut iters: Vec<f64> = rs.iter().map(|(_, r)| r.iterations as f64).collect();
        let mut residuals: Vec<f64> = rs.iter().map(|(_, r)| r.residual).collect();
        println!(
            "{legs} legs: {} relaxed, {} unconverged, iterations p50 {:.0} p90 {:.0} max {:.0}; seating residual p50 {:.2} max {:.2} Å; formed bond > 1.25× rest: {} (in window {}); in window {}, worst formed ratio in window {:.3}",
            rs.len(),
            rs.iter().filter(|(_, r)| !r.converged).count(),
            percentile(&mut iters, 0.5),
            percentile(&mut iters, 0.9),
            iters.iter().copied().fold(0.0, f64::max),
            percentile(&mut residuals, 0.5),
            residuals.iter().copied().fold(0.0, f64::max),
            rs.iter().filter(|(_, r)| r.formed_ratio > 1.25).count(),
            window.iter().filter(|(_, r)| r.formed_ratio > 1.25).count(),
            window.len(),
            window
                .iter()
                .map(|(_, r)| r.formed_ratio)
                .fold(0.0, f64::max),
        );
    }
    // Energies against the brute force for the same pattern.
    let mut diffs: Vec<(f64, usize, Key)> = Vec::new();
    for p in patterns.iter().filter(|p| p.in_leg_window) {
        let best = translates(p)
            .iter()
            .filter_map(|k| relaxed.get(k).filter(|_| accepted(k)).map(|r| r.energy))
            .fold(f64::INFINITY, f64::min);
        if best.is_finite() {
            diffs.push((best - p.best_energy, p.legs.len(), Vec::new()));
        }
    }
    let mut d: Vec<f64> = diffs.iter().map(|x| x.0).collect();
    println!(
        "sequential − brute-force energy of the same windowed pattern (best translate): n {}, min {:.2}, p50 {:.2}, p90 {:.2}, max {:.2} kcal/mol",
        d.len(),
        d.iter().copied().fold(f64::INFINITY, f64::min),
        percentile(&mut d, 0.5),
        percentile(&mut d, 0.9),
        d.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    // Exactly the same pattern (pose 0, no translation).
    let mut same: Vec<f64> = Vec::new();
    for c in old.iter().filter(|c| c.pose == 0 && c.formed.len() >= 2) {
        let k = seq.key_of_input(&c.formed).unwrap();
        if let Some(r) = relaxed.get(&k).filter(|_| accepted(&k)) {
            same.push(r.energy - c.energy);
        }
    }
    println!(
        "same pose, same sites (pose 0): n {}, min {:.2}, p50 {:.2}, max {:.2} kcal/mol",
        same.len(),
        same.iter().copied().fold(f64::INFINITY, f64::min),
        percentile(&mut same.clone(), 0.5),
        same.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );

    // ---- Q7: mirror-pruned and clashing seatings --------------------------
    println!("\n== Q7 where pruned seatings land ==");
    let mut mirrored: Vec<f64> = relaxed
        .iter()
        .filter(|(k, _)| k.len() == 3 && verdict[*k] == Mirror::Mirrored)
        .map(|(_, r)| strain(r) - best_seq[&3])
        .collect();
    println!(
        "mirror-pruned: {}; in the 3-leg window: {}; lowest margin above the best accepted 3-leg {:.1} kcal/mol; how many clash when seated: {}",
        mirrored.len(),
        mirrored.iter().filter(|&&m| m <= WINDOW).count(),
        mirrored.iter().copied().fold(f64::INFINITY, f64::min),
        relaxed
            .iter()
            .filter(|(k, r)| k.len() == 3 && verdict[*k] == Mirror::Mirrored && r.clashes > 0)
            .count(),
    );
    let _ = percentile(&mut mirrored, 0.5);
    // A mirror-pruned binding that lands in the window: is the same bond set
    // also reached properly (it cannot, the key is the same) — print a few.
    for (k, r) in relaxed
        .iter()
        .filter(|(k, r)| {
            k.len() == 3 && verdict[*k] == Mirror::Mirrored && strain(r) - best_seq[&3] <= WINDOW
        })
        .take(5)
    {
        println!(
            "  mirrored in window: {:?} strain {:.1} (+{:.1}), clashes {}, formed ratio {:.2}, converged {}",
            k,
            strain(r),
            strain(r) - best_seq[&3],
            r.clashes,
            r.formed_ratio,
            r.converged
        );
    }
    for legs in [2, 3] {
        let clashing: Vec<(&Key, &Relaxed)> = relaxed
            .iter()
            .filter(|(k, r)| k.len() == legs && accepted(k) && r.clashes > 0)
            .collect();
        println!(
            "{legs} legs, seating clashes: {} of {}; in the window: {}; lowest margin {:.1}",
            clashing.len(),
            relaxed
                .keys()
                .filter(|k| k.len() == legs && accepted(k))
                .count(),
            clashing
                .iter()
                .filter(|(_, r)| strain(r) - best_seq[&legs] <= WINDOW)
                .count(),
            clashing
                .iter()
                .map(|(_, r)| strain(r) - best_seq[&legs])
                .fold(f64::INFINITY, f64::min),
        );
    }
    let undecided = verdict
        .values()
        .filter(|&&v| v == Mirror::Undecided)
        .count();
    println!("mirror undecided: {undecided} of {}", verdict.len());

    // ---- Q2: bond divergence in the window --------------------------------
    println!(
        "\n== Q2 bond divergence (relaxed, accepted, within the window of their leg count) =="
    );
    for legs in [2, 3] {
        let mut div: Vec<f64> = relaxed
            .iter()
            .filter(|(k, r)| {
                k.len() == legs && accepted(k) && strain(r) - best_seq[&legs] <= WINDOW
            })
            .map(|(_, r)| r.divergence_deg)
            .collect();
        println!(
            "{legs} legs: n {}, p50 {:.0}°, p90 {:.0}°, max {:.0}°",
            div.len(),
            percentile(&mut div, 0.5),
            percentile(&mut div, 0.9),
            div.iter().copied().fold(0.0, f64::max)
        );
    }
    // And of everything relaxed (what a cap would cut).
    for cap in [60.0, 90.0, 120.0] {
        let cut = relaxed
            .iter()
            .filter(|(k, r)| accepted(k) && r.divergence_deg > cap)
            .count();
        println!(
            "relaxed divergence > {cap}°: {cut} of {}",
            relaxed.keys().filter(|k| accepted(k)).count()
        );
    }

    // The ten best accepted, for the record.
    let mut ranked: Vec<(&Key, &Relaxed)> = relaxed.iter().filter(|(k, _)| accepted(k)).collect();
    ranked.sort_by(|a, b| strain(a.1).total_cmp(&strain(b.1)));
    println!("\nbest sequential candidates (separated reference):");
    for (k, r) in ranked.iter().take(10) {
        println!(
            "  {} legs {:?} strain {:.1} converged {} clashes {} div {:.0}°",
            k.len(),
            k.iter()
                .map(|&(f, s)| (seq.feet[f], seq.sites[s]))
                .collect::<Vec<_>>(),
            strain(r),
            r.converged,
            r.clashes,
            r.divergence_deg
        );
    }
    let best3: Vec<_> = ranked
        .iter()
        .filter(|(k, _)| k.len() == 3)
        .take(5)
        .collect();
    for (k, r) in best3 {
        println!(
            "  3-leg {:?} strain {:.1} converged {} clashes {} div {:.0}° residual {:.2}",
            k.iter()
                .map(|&(f, s)| (seq.feet[f], seq.sites[s]))
                .collect::<Vec<_>>(),
            strain(r),
            r.converged,
            r.clashes,
            r.divergence_deg,
            r.residual
        );
    }
}

// ============================================================================
// Hexapod: Q1 (how far feet 4–6 move) and the level counts
// ============================================================================

#[test]
#[ignore]
fn spike_sequential_hexapod() {
    let golden = read_golden();
    let slab = si100_slab(5.0, 11.0);
    let (ads, _) = posed_stand_in(6, 0.0, DVec3::ZERO);
    let seq = Seq::new(&ads, &slab);
    let cfg = ChemisorptionSearch::default();
    let old = &golden["hexapod_pose0"]["run"];
    println!(
        "hexapod: {} feet, {} sites; old engine (pose 0, reach 3.5) relaxed {}",
        seq.feet.len(),
        seq.sites.len(),
        old["relaxed"]
    );
    for tol in [0.0, 1.0, 2.0] {
        let start = Instant::now();
        let lv = seq.enumerate(3.5, tol, tol);
        let mut verdicts: BTreeMap<Mirror, usize> = BTreeMap::new();
        for k in &lv.three {
            *verdicts.entry(seq.mirror(k, seq.local_up(k))).or_insert(0) += 1;
        }
        println!(
            "tol {tol}: anchors {}, 2-leg {}, 3-leg {} (proper {}, mirrored {}, undecided {}); enumerate {:.2} s",
            lv.anchors,
            lv.two.len(),
            lv.three.len(),
            verdicts.get(&Mirror::Proper).unwrap_or(&0),
            verdicts.get(&Mirror::Mirrored).unwrap_or(&0),
            verdicts.get(&Mirror::Undecided).unwrap_or(&0),
            start.elapsed().as_secs_f64()
        );
    }

    // Q1: relax a deterministic sample of accepted three-leg hypotheses and
    // compare the unbonded feet seated (option b) against relaxed (option a).
    let lv = seq.enumerate(3.5, 1.0, 1.0);
    let accepted: Vec<Key> = lv
        .three
        .iter()
        .filter(|k| seq.mirror(k, seq.local_up(k)) != Mirror::Mirrored)
        .cloned()
        .collect();
    let n = accepted.len();
    let sample: Vec<Key> = (0..n)
        .map(|i| accepted[i * accepted.len() / n].clone())
        .collect();
    println!(
        "Q1: relaxing {n} of {} accepted three-leg hypotheses…",
        accepted.len()
    );
    let start = Instant::now();
    let relaxed = relax_all(&seq, &sample, &cfg);
    println!("  {:.1} s", start.elapsed().as_secs_f64());
    let e_sep = Seq::separated_reference(&ads, &slab, &cfg);
    let best = relaxed
        .values()
        .map(|r| r.energy - e_sep)
        .fold(f64::INFINITY, f64::min);

    let reach = 3.0;
    let sites_near = |p: DVec3, used: &HashSet<usize>| -> BTreeSet<usize> {
        (0..seq.sites.len())
            .filter(|s| !used.contains(s) && seq.site_pos[*s].distance(p) <= reach)
            .collect()
    };
    for (label, window_only) in [
        ("all sampled", false),
        ("within 30 kcal/mol of the best sampled", true),
    ] {
        let mut moves: Vec<f64> = Vec::new();
        let (mut same_sets, mut diff_sets, mut only_a, mut only_b) = (0, 0, 0, 0);
        let mut count = 0;
        for (k, r) in &relaxed {
            if window_only && r.energy - e_sep - best > WINDOW {
                continue;
            }
            count += 1;
            let bonded: HashSet<usize> = k.iter().map(|&(f, _)| f).collect();
            let used: HashSet<usize> = k.iter().map(|&(_, s)| s).collect();
            for f in 0..seq.feet.len() {
                if bonded.contains(&f) {
                    continue;
                }
                moves.push(r.foot_positions[f].distance(r.seated_foot_positions[f]));
                let a = sites_near(r.foot_positions[f], &used);
                let b = sites_near(r.seated_foot_positions[f], &used);
                if a == b {
                    same_sets += 1;
                } else {
                    diff_sets += 1;
                }
                only_a += a.difference(&b).count();
                only_b += b.difference(&a).count();
            }
        }
        println!(
            "{label} ({count}): unbonded-foot move seated→relaxed p50 {:.2}, p90 {:.2}, max {:.2} Å; site sets within {reach} Å equal for {same_sets}, differ for {diff_sets} feet; sites only near relaxed {only_a}, only near seated {only_b}",
            percentile(&mut moves.clone(), 0.5),
            percentile(&mut moves.clone(), 0.9),
            moves.iter().copied().fold(0.0, f64::max),
        );
    }
    let unconverged = relaxed.values().filter(|r| !r.converged).count();
    let mut div: Vec<f64> = relaxed
        .values()
        .filter(|r| r.energy - e_sep - best <= WINDOW)
        .map(|r| r.divergence_deg)
        .collect();
    println!(
        "hexapod sample: {unconverged} unconverged; best 3-leg strain {best:.1}; divergence in window p50 {:.0}° max {:.0}°",
        percentile(&mut div, 0.5),
        div.iter().copied().fold(0.0, f64::max)
    );
}

// ============================================================================
// Ethylene: Q8, the end-bridge is a hypothesis; the known answer holds
// ============================================================================

#[test]
#[ignore]
fn spike_sequential_ethylene() {
    let golden = read_golden();
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    let (eth, _) = ethanediyl((ps + pp) / 2.0 + DVec3::Z * 2.0, pp - ps);
    let seq = Seq::new(&eth, &slab);
    let cfg = ChemisorptionSearch::default();
    let old = old_candidates(&golden["ethylene"]["run"], 0);
    let doubles: Vec<&OldCandidate> = old.iter().filter(|c| c.formed.len() == 2).collect();
    let lv0 = seq.enumerate(3.5, 0.0, 0.0);
    let lv1 = seq.enumerate(3.5, 1.0, 1.0);
    let is_bridge = |c: &OldCandidate| !slab.has_bond_between(c.formed[0].1, c.formed[1].1);
    let mut kept = 0;
    let mut bridges = 0;
    let mut bridges_kept = 0;
    for c in &doubles {
        let k = seq.key_of_input(&c.formed).unwrap();
        let hit = lv0.two.contains(&k);
        kept += usize::from(hit);
        if !hit {
            let nearest = k
                .iter()
                .map(|&(f, s)| seq.foot_pos[f].distance(seq.site_pos[s]))
                .fold(f64::INFINITY, f64::min);
            println!(
                "  missed: {:?} bridge {} need {:.2} Å, nearest foot–site {:.2} Å, old strain {:.1}",
                c.formed,
                is_bridge(c),
                seq.need(&k),
                nearest,
                c.strain
            );
        }
        if is_bridge(c) {
            bridges += 1;
            bridges_kept += usize::from(hit);
        }
    }
    println!(
        "ethylene: old two-bond bindings (reach 4.5) {}; among the new tol-0 hypotheses {}; end-bridges {} of which kept {}; new 2-leg at tol 0: {}, at tol 1: {}",
        doubles.len(),
        kept,
        bridges,
        bridges_kept,
        lv0.two.len(),
        lv1.two.len()
    );
    let keys: Vec<Key> = lv1.two.iter().cloned().collect();
    let relaxed = relax_all(&seq, &keys, &cfg);
    let e_sep = Seq::separated_reference(&eth, &slab, &cfg);
    let mut ranked: Vec<(&Key, &Relaxed)> = relaxed.iter().collect();
    ranked.sort_by(|a, b| a.1.energy.total_cmp(&b.1.energy));
    for (k, r) in ranked.iter().take(6) {
        let (s1, s2) = (seq.sites[k[0].1], seq.sites[k[1].1]);
        println!(
            "  {:?} strain {:.1} {} converged {} clashes {}",
            k,
            r.energy - e_sep,
            if seq.combined.has_bond_between(s1, s2) {
                "di-σ (one dimer)"
            } else {
                "across dimers"
            },
            r.converged,
            r.clashes
        );
    }
    let best_old_disigma = doubles
        .iter()
        .filter(|c| !is_bridge(c))
        .map(|c| c.energy - e_sep)
        .fold(f64::INFINITY, f64::min);
    let best_old_bridge = doubles
        .iter()
        .filter(|c| is_bridge(c))
        .map(|c| c.energy - e_sep)
        .fold(f64::INFINITY, f64::min);
    println!(
        "old engine (separated ref): di-σ {best_old_disigma:.1}, best end-bridge {best_old_bridge:.1}"
    );
}
