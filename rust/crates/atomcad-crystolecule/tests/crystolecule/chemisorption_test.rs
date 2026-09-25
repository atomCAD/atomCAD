//! Tests for `chemisorption`: the `plan` enumeration on hand-built geometry,
//! the bond-energy tables, ranking, transfers, the two known-answer cases on
//! Si(100)-2×1 (ethylene by bond forming, water by an H transfer), and
//! (ignored) the pruning calibration.
//!
//! Fixtures are generic stand-ins, never a real tool design: a 26-carbon
//! diamondoid cage whose (111) bottom face carries six downward C–H, turned
//! into radical O feet — the outer three for the tripod (5.04 Å apart), all six
//! for the hexapod.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::atomic_structure_utils::{auto_create_bonds, remove_single_bond_atoms};
use atomcad_crystolecule::chemisorption::score::{
    bond_enthalpy, pauling_estimate_kj, tabulated_enthalpy_kj,
};
use atomcad_crystolecule::chemisorption::{
    BondInventory, BondKind, CHANGED_TAG, Candidate, ChemisorptionError, ChemisorptionSearch,
    SearchPlan, Side, StrainTerms, Transfer, TransferDirection, TransferRule, evaluate,
    free_valence, input_fingerprint, listed_count, plan, rank_candidates, search,
};
use atomcad_crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use atomcad_crystolecule::hydrogen_passivation::terminator_bond_length;
use atomcad_crystolecule::hydrogen_passivation::{AddHydrogensOptions, add_hydrogens};
use atomcad_crystolecule::lattice_fill::{LatticeFillConfig, LatticeFillOptions, fill_lattice};
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_geo_tree::GeoNode;
use atomcad_util::daabox::DAABox;
use glam::{DQuat, DVec3};
use std::collections::{BTreeSet, HashMap};

const H: i16 = 1;
const B: i16 = 5;
const C: i16 = 6;
const N: i16 = 7;
const O: i16 = 8;
const SI: i16 = 14;

const A_C: f64 = 3.567;
const A_SI: f64 = 5.431;

// ============================================================================
// Fixtures
// ============================================================================

/// Three bond directions of an sp3 atom whose open valence points along `up`.
fn tetrahedral_below(up: DVec3) -> [DVec3; 3] {
    let q = DQuat::from_rotation_arc(DVec3::Z, up.normalize());
    let (sin, cos) = ((8.0f64 / 9.0).sqrt(), -1.0 / 3.0);
    [0.0f64, 120.0, 240.0].map(|deg| {
        let phi = deg.to_radians();
        q * DVec3::new(sin * phi.cos(), sin * phi.sin(), cos)
    })
}

/// A silyl at `p` with `h_count` hydrogens, its dangling bonds pointing up:
/// 3 = SiH3 (one free valence), 2 = SiH2 (two), 4 = SiH4 (none).
fn add_silyl(s: &mut AtomicStructure, p: DVec3, h_count: usize) -> u32 {
    let si = s.add_atom(SI, p);
    let mut dirs: Vec<DVec3> = tetrahedral_below(DVec3::Z).to_vec();
    dirs.push(DVec3::Z);
    for d in dirs.into_iter().take(h_count) {
        let h = s.add_atom(H, p + d * 1.48);
        s.add_bond(si, h, BOND_SINGLE);
    }
    si
}

/// A radical O foot at `p` hanging from a methyl above it: CH3–O•.
fn add_methoxy(s: &mut AtomicStructure, p: DVec3) -> u32 {
    let o = s.add_atom(O, p);
    let c = s.add_atom(C, p + DVec3::Z * 1.43);
    s.add_bond(o, c, BOND_SINGLE);
    for d in tetrahedral_below(-DVec3::Z) {
        let h = s.add_atom(H, p + DVec3::Z * 1.43 + d * 1.09);
        s.add_bond(c, h, BOND_SINGLE);
    }
    o
}

/// One structure with a radical O foot at each position (each its own
/// methoxy; `plan` never relaxes, so rigidity is irrelevant to enumeration).
fn feet(positions: &[DVec3]) -> (AtomicStructure, Vec<u32>) {
    let mut s = AtomicStructure::new();
    let ids = positions.iter().map(|&p| add_methoxy(&mut s, p)).collect();
    (s, ids)
}

fn sites(positions: &[DVec3], h_count: usize) -> (AtomicStructure, Vec<u32>) {
    let mut s = AtomicStructure::new();
    let ids = positions
        .iter()
        .map(|&p| add_silyl(&mut s, p, h_count))
        .collect();
    (s, ids)
}

fn config(reach: f64, pair_tolerance: f64) -> ChemisorptionSearch {
    ChemisorptionSearch {
        reach,
        pair_tolerance,
        ..Default::default()
    }
}

/// Every planned hypothesis's formed bonds, in *input* atom ids, sorted.
fn planned_sets(p: &SearchPlan) -> BTreeSet<Vec<(u32, u32)>> {
    let back_a: HashMap<u32, u32> = p.adsorbate_ids.iter().map(|(&k, &v)| (v, k)).collect();
    let back_s: HashMap<u32, u32> = p.substrate_ids.iter().map(|(&k, &v)| (v, k)).collect();
    p.hypotheses
        .iter()
        .map(|h| {
            let mut v: Vec<(u32, u32)> = h
                .formed
                .iter()
                .map(|(a, s)| (back_a[a], back_s[s]))
                .collect();
            v.sort_unstable();
            v
        })
        .collect()
}

fn assert_stats_add_up(p: &SearchPlan) {
    let s = &p.stats;
    assert_eq!(
        s.considered,
        s.pruned_valence
            + s.pruned_pair_tolerance
            + s.duplicates
            + s.to_relax
            + usize::from(s.truncated),
        "{s:?}"
    );
    assert_eq!(s.to_relax, p.hypotheses.len());
}

/// The diamond lattice points of a sphere, bonded, one-bond atoms removed.
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

/// The stand-in cage, (111) face down, with radical O feet on `legs` of its
/// six bottom carbons: 3 = the outer triangle (the tripod), 6 = all (the
/// hexapod). Returns the molecule and its foot ids; the feet lie in the plane
/// `z = 0`, around the z axis.
fn stand_in(legs: usize) -> (AtomicStructure, Vec<u32>) {
    let mut s = diamond_cluster(DVec3::splat(A_C * 0.125), 3.6);
    add_hydrogens(&mut s, &AddHydrogensOptions::default());
    let q = DQuat::from_rotation_arc(DVec3::splat(-1.0).normalize(), -DVec3::Z);
    s.transform(&q, &DVec3::ZERO);

    // The bottom carbons carry an H pointing straight down.
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
    assert_eq!(bottom.len(), 6, "the cage's (111) face has six C–H");
    // The outer triangle first (farthest from the axis), then the inner one.
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

/// A bare, reconstructed Si(100)-2×1 slab, `cells` cubic cells wide and 1.5
/// deep, moved so its dimer layer is at `z = 0` and its lateral centre on the
/// z axis. Atoms deeper than 2 Å or farther than `mobile_radius` from the axis
/// are frozen, as a proxy's rim would be. The slab's side faces are bare too,
/// and every atom on them is a site: keep them out of reach (5 cells is
/// enough for a search near the axis; 3 is not).
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
    // The dimer layer, not the highest atom: the fill leaves unreconstructed
    // atoms along the slab's top edges (two bonds each) ~0.4 Å higher.
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

/// The surface dimer nearest the axis: two bonded three-coordinate atoms of
/// the dimer layer.
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
        .expect("the slab has surface dimers");
    (site, partner_of(site).unwrap())
}

/// •CH2–CH2•, the 1,2-diradical, its C–C axis along `axis`, centred at
/// `center`, dangling bonds pointing down. Returns the molecule and its C.
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

// ============================================================================
// Free valence
// ============================================================================

#[test]
fn free_valence_counts_bond_orders_against_the_element_valence() {
    let (s, ids) = sites(&[DVec3::ZERO], 3);
    assert_eq!(free_valence(&s, ids[0]), 1);
    let (s, ids) = sites(&[DVec3::ZERO], 2);
    assert_eq!(free_valence(&s, ids[0]), 2);
    let (s, ids) = sites(&[DVec3::ZERO], 4);
    assert_eq!(free_valence(&s, ids[0]), 0);
    let (s, ids) = feet(&[DVec3::ZERO]);
    assert_eq!(free_valence(&s, ids[0]), 1, "a methoxy O has one");

    // A radical carbon with three single bonds keeps its free valence even
    // though UFF would type it sp2.
    let (s, [c, _]) = ethanediyl(DVec3::ZERO, DVec3::X);
    assert_eq!(free_valence(&s, c), 1);
}

// ============================================================================
// Enumeration (§8.1)
// ============================================================================

/// Two dimers, 3.84 Å apart along y, 2.4 Å dimer length along x.
fn two_dimers() -> [DVec3; 4] {
    [
        DVec3::new(-1.2, 0.0, 0.0),
        DVec3::new(1.2, 0.0, 0.0),
        DVec3::new(-1.2, 3.84, 0.0),
        DVec3::new(1.2, 3.84, 0.0),
    ]
}

#[test]
fn one_foot_over_two_dimers_bonds_to_each_site_within_reach() {
    let (ads, foot_ids) = feet(&[DVec3::new(1.2, 0.0, 2.0)]);
    let (sub, site_ids) = sites(&two_dimers(), 3);
    let p = plan(&ads, &sub, &config(3.5, 1.0)).unwrap();
    // Both atoms of the dimer beneath are within 3.5 Å; the other dimer (4.3 Å
    // and more) is not.
    assert_eq!(p.stats.feet, 1);
    assert_eq!(p.stats.sites_in_reach, 2);
    assert_eq!(p.stats.to_relax, 2);
    assert_eq!(p.stats.considered, 2);
    assert_eq!(
        planned_sets(&p),
        BTreeSet::from([
            vec![(foot_ids[0], site_ids[0])],
            vec![(foot_ids[0], site_ids[1])],
        ])
    );
    assert!(!p.stats.truncated);
    assert_stats_add_up(&p);
}

/// Feet 2.4 Å apart over sites A, B (2.4 Å apart: a match) and C (2.77 Å
/// from each: off by 0.37 Å).
fn two_feet_three_sites() -> (AtomicStructure, Vec<u32>, AtomicStructure, Vec<u32>) {
    let (ads, feet_ids) = feet(&[DVec3::new(-1.2, 0.0, 2.0), DVec3::new(1.2, 0.0, 2.0)]);
    let (sub, site_ids) = sites(
        &[
            DVec3::new(-1.2, 0.0, 0.0),
            DVec3::new(1.2, 0.0, 0.0),
            DVec3::new(0.0, 2.5, 0.0),
        ],
        3,
    );
    (ads, feet_ids, sub, site_ids)
}

#[test]
fn pair_tolerance_prunes_a_site_pair_that_misses_the_foot_spacing() {
    let (ads, feet_ids, sub, site_ids) = two_feet_three_sites();
    let (f1, f2) = (feet_ids[0], feet_ids[1]);
    let (a, b, c) = (site_ids[0], site_ids[1], site_ids[2]);

    let p = plan(&ads, &sub, &config(3.5, 0.3)).unwrap();
    let sets = planned_sets(&p);
    // Six single bonds, and the two double bindings whose site spacing
    // matches: A–B straight and crossed.
    assert_eq!(p.stats.to_relax, 8, "{sets:?}");
    assert!(sets.contains(&vec![(f1, a), (f2, b)]));
    assert!(sets.contains(&vec![(f1, b), (f2, a)]));
    assert!(!sets.contains(&vec![(f1, a), (f2, c)]));
    // Per first-foot site the second foot tries A, B and C: one of them is
    // the same site (valence), and every pairing with C misses by 0.37 Å.
    assert_eq!(p.stats.pruned_valence, 3);
    assert_eq!(p.stats.pruned_pair_tolerance, 4);
    assert_eq!(p.stats.considered, 15);
    assert_stats_add_up(&p);

    // With the check off, the C pairings are valid too.
    let off = plan(&ads, &sub, &config(3.5, 0.0)).unwrap();
    assert_eq!(off.stats.to_relax, 12);
    assert_eq!(off.stats.pruned_pair_tolerance, 0);
    assert!(planned_sets(&off).contains(&vec![(f1, a), (f2, c)]));
    assert_stats_add_up(&off);
}

#[test]
fn an_h_capped_atom_is_never_a_site() {
    let (ads, _) = feet(&[DVec3::new(0.0, 0.0, 2.0)]);
    let (sub, _) = sites(&[DVec3::ZERO], 4);
    let p = plan(&ads, &sub, &config(3.5, 1.0)).unwrap();
    assert_eq!(p.stats.feet, 0);
    assert_eq!(p.stats.to_relax, 0);
    assert!(p.hypotheses.is_empty());
}

#[test]
fn a_two_dangling_bond_site_takes_two_bonds() {
    let (ads, feet_ids) = feet(&[DVec3::new(-1.2, 0.0, 2.0), DVec3::new(1.2, 0.0, 2.0)]);
    let (f1, f2) = (feet_ids[0], feet_ids[1]);

    // SiH2: valence 2. Both feet on it passes the valence rule; the pair
    // tolerance (site distance 0 against foot distance 2.4) is what prunes it.
    let (sub, site_ids) = sites(&[DVec3::ZERO], 2);
    let s = site_ids[0];
    let on = plan(&ads, &sub, &config(3.5, 1.0)).unwrap();
    assert_eq!(on.stats.pruned_pair_tolerance, 1);
    assert_eq!(on.stats.pruned_valence, 0);
    assert!(!planned_sets(&on).contains(&vec![(f1, s), (f2, s)]));
    let off = plan(&ads, &sub, &config(3.5, 0.0)).unwrap();
    assert!(planned_sets(&off).contains(&vec![(f1, s), (f2, s)]));
    assert_eq!(off.stats.to_relax, 3);

    // SiH3: valence 1, so the second bond is a valence prune.
    let (sub, _) = sites(&[DVec3::ZERO], 3);
    let one = plan(&ads, &sub, &config(3.5, 0.0)).unwrap();
    assert_eq!(one.stats.pruned_valence, 1);
    assert_eq!(one.stats.to_relax, 2);
    assert_stats_add_up(&one);
}

#[test]
fn no_bond_forms_between_two_frozen_atoms() {
    let (mut ads, feet_ids) = feet(&[DVec3::new(-1.2, 0.0, 2.0)]);
    let (mut sub, site_ids) = sites(&[DVec3::new(-1.2, 0.0, 0.0), DVec3::new(1.2, 0.0, 0.0)], 3);
    ads.set_atom_frozen(feet_ids[0], true);
    sub.set_atom_frozen(site_ids[0], true);
    let p = plan(&ads, &sub, &config(3.5, 1.0)).unwrap();
    // Frozen foot and frozen site: excluded. Frozen foot, free site: allowed.
    assert_eq!(
        planned_sets(&p),
        BTreeSet::from([vec![(feet_ids[0], site_ids[1])]])
    );
}

#[test]
fn partial_binding_is_enumerated_and_capped_and_never_empty() {
    let (ads, _, sub, _) = two_feet_three_sites();
    let all = plan(&ads, &sub, &config(3.5, 0.3)).unwrap();
    let counts: BTreeSet<usize> = all.hypotheses.iter().map(|h| h.formed.len()).collect();
    assert_eq!(counts, BTreeSet::from([1, 2]), "no zero-change hypothesis");

    let capped = plan(
        &ads,
        &sub,
        &ChemisorptionSearch {
            max_formed_bonds: Some(1),
            ..config(3.5, 0.3)
        },
    )
    .unwrap();
    assert_eq!(capped.stats.to_relax, 6);
    assert!(capped.hypotheses.iter().all(|h| h.formed.len() == 1));
    assert_stats_add_up(&capped);
}

#[test]
fn plan_is_deterministic_and_deduplicated() {
    let (ads, _, sub, _) = two_feet_three_sites();
    let a = plan(&ads, &sub, &config(3.5, 0.0)).unwrap();
    let b = plan(&ads, &sub, &config(3.5, 0.0)).unwrap();
    assert_eq!(a.hypotheses, b.hypotheses);
    let keys: BTreeSet<_> = a.hypotheses.iter().map(|h| h.key()).collect();
    assert_eq!(keys.len(), a.hypotheses.len());
    // Bond forming alone cannot produce a duplicate: the assignment fixes the
    // bond set. Duplicates arrive with transfers.
    assert_eq!(a.stats.duplicates, 0);
}

#[test]
fn the_budget_truncates_and_says_so() {
    let (ads, _, sub, _) = two_feet_three_sites();
    let small = plan(
        &ads,
        &sub,
        &ChemisorptionSearch {
            budget: 3,
            ..config(3.5, 0.3)
        },
    )
    .unwrap();
    assert_eq!(small.stats.to_relax, 3);
    assert!(small.stats.truncated);
    assert_stats_add_up(&small);

    // Exactly as many valid hypotheses as the budget is not a truncation.
    let exact = plan(
        &ads,
        &sub,
        &ChemisorptionSearch {
            budget: 8,
            ..config(3.5, 0.3)
        },
    )
    .unwrap();
    assert_eq!(exact.stats.to_relax, 8);
    assert!(!exact.stats.truncated);
}

#[test]
fn reactive_tags_select_atoms_per_side() {
    let (mut ads, feet_ids, mut sub, site_ids) = two_feet_three_sites();
    ads.add_atom_tag(feet_ids[0], "foot").unwrap();
    sub.add_atom_tag(site_ids[2], "site").unwrap();
    let p = plan(
        &ads,
        &sub,
        &ChemisorptionSearch {
            adsorbate_tag: Some("foot".into()),
            substrate_tag: Some("site".into()),
            ..config(3.5, 0.3)
        },
    )
    .unwrap();
    assert_eq!(
        planned_sets(&p),
        BTreeSet::from([vec![(feet_ids[0], site_ids[2])]])
    );

    // An empty tag means "all atoms".
    let all = plan(
        &ads,
        &sub,
        &ChemisorptionSearch {
            adsorbate_tag: Some(String::new()),
            ..config(3.5, 0.3)
        },
    )
    .unwrap();
    assert_eq!(all.stats.to_relax, 8);

    // A tag no atom carries is an error, not "found nothing".
    let err = plan(
        &ads,
        &sub,
        &ChemisorptionSearch {
            substrate_tag: Some("sites".into()),
            ..config(3.5, 0.3)
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ChemisorptionError::UnknownTag { side: Side::Substrate, ref tag } if tag == "sites"
    ));
}

#[test]
fn invalid_settings_are_rejected() {
    let (ads, _, sub, _) = two_feet_three_sites();
    for bad in [
        config(0.0, 1.0),
        config(3.5, -1.0),
        ChemisorptionSearch {
            budget: 0,
            ..Default::default()
        },
        ChemisorptionSearch {
            max_formed_bonds: Some(0),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            plan(&ads, &sub, &bad),
            Err(ChemisorptionError::InvalidConfig(_))
        ));
    }
}

#[test]
fn plan_on_the_stand_in_hexapod_is_fast() {
    let slab = si100_slab(5.0, 9.0);
    let (mut ads, _) = stand_in(6);
    ads.transform(&DQuat::IDENTITY, &DVec3::new(0.0, 0.0, 1.8));
    let p = plan(&ads, &slab, &ChemisorptionSearch::default()).unwrap();
    println!(
        "hexapod plan: {} feet, {} sites, {} considered, {} to relax, {:.4} s",
        p.stats.feet, p.stats.sites_in_reach, p.stats.considered, p.stats.to_relax, p.stats.seconds
    );
    assert_eq!(p.stats.feet, 6);
    assert!(p.stats.to_relax > 0);
    assert_stats_add_up(&p);
    // §7.4: `plan` runs on every evaluation, so it must stay cheap. The
    // release bound is the design's; a debug build is an order slower.
    let bound = if cfg!(debug_assertions) { 2.0 } else { 0.1 };
    assert!(p.stats.seconds < bound, "plan took {} s", p.stats.seconds);
}

// ============================================================================
// Scoring (§8.3)
// ============================================================================

#[test]
fn the_enthalpy_table_is_symmetric_and_converted_once() {
    assert_eq!(tabulated_enthalpy_kj(SI, O), Some(452.0));
    assert_eq!(tabulated_enthalpy_kj(O, SI), Some(452.0));
    assert_eq!(tabulated_enthalpy_kj(C, SI), Some(318.0));
    assert_eq!(tabulated_enthalpy_kj(32, 32), Some(186.0));
    let (d, estimated) = bond_enthalpy(SI, O).unwrap();
    assert!((d - 452.0 / 4.184).abs() < 1e-12);
    assert!(!estimated);
}

#[test]
fn pauling_estimates_track_the_table() {
    // A self-check that catches a mistyped value: for pairs the table has,
    // Pauling's estimate from the homonuclear values is within 15 %.
    for (a, b) in [(SI, O), (SI, H), (SI, 17), (C, H), (O, H), (C, O)] {
        let table = tabulated_enthalpy_kj(a, b).unwrap();
        let estimate = pauling_estimate_kj(a, b).unwrap();
        let rel = (estimate - table).abs() / table;
        assert!(
            rel < 0.15,
            "{a}–{b}: estimate {estimate:.1} vs table {table}"
        );
    }
    let si_o = pauling_estimate_kj(SI, O).unwrap();
    assert!((si_o - 412.9).abs() < 0.5, "Si–O estimate {si_o}");
}

#[test]
fn a_missing_pair_is_estimated_and_an_unknown_element_is_an_error() {
    assert_eq!(tabulated_enthalpy_kj(SI, N), None);
    let (d, estimated) = bond_enthalpy(SI, N).unwrap();
    assert!(estimated);
    assert!(d > 0.0);
    assert!(matches!(
        bond_enthalpy(SI, B),
        Err(ChemisorptionError::UnscoredElement { ref element }) if element == "B"
    ));

    // `plan` knows the estimated pairs before anything is relaxed…
    let mut ads = AtomicStructure::new();
    let n = ads.add_atom(N, DVec3::new(0.0, 0.0, 2.0));
    for d in tetrahedral_below(-DVec3::Z).into_iter().take(2) {
        let h = ads.add_atom(H, DVec3::new(0.0, 0.0, 2.0) + d * 1.01);
        ads.add_bond(n, h, BOND_SINGLE);
    }
    let (sub, _) = sites(&[DVec3::ZERO], 3);
    let p = plan(&ads, &sub, &config(3.5, 1.0)).unwrap();
    assert_eq!(p.stats.estimated_pairs, vec![BondKind::new(N, SI, 1)]);
    assert_eq!(p.stats.estimated_pairs[0].pair_label(), "N–Si");

    // …and refuses an element it cannot score.
    let mut boron = AtomicStructure::new();
    boron.add_atom(B, DVec3::new(0.0, 0.0, 2.0));
    assert!(matches!(
        plan(&boron, &sub, &config(3.5, 1.0)),
        Err(ChemisorptionError::UnscoredElement { ref element }) if element == "B"
    ));
}

#[test]
fn a_bond_inventory_reads_and_scores() {
    let mut inv = BondInventory::default();
    inv.formed.insert(BondKind::new(O, SI, 1), 1);
    inv.formed.insert(BondKind::new(H, SI, 1), 1);
    inv.broken.insert(BondKind::new(O, H, 1), 1);
    assert_eq!(inv.to_string(), "formed 1× H–Si, 1× O–Si; broken 1× H–O");
    let (e, estimated) = inv.bond_energy().unwrap();
    assert!((e - (463.0 - 452.0 - 318.0) / 4.184).abs() < 1e-9);
    assert!(!estimated);
}

fn fake(score: f64, formed: Vec<(u32, u32)>) -> Candidate {
    Candidate {
        structure: AtomicStructure::new(),
        formed,
        transfers: Vec::new(),
        bond_inventory: BondInventory::default(),
        strain: score,
        bond_energy: 0.0,
        score,
        estimated: false,
        terms: StrainTerms::default(),
        converged: true,
        worst_bond_ratio: 1.0,
        energy: 0.0,
    }
}

#[test]
fn ranking_breaks_ties_by_the_bond_set_and_lists_top_n_within_the_window() {
    let mut cs = vec![
        fake(-10.0, vec![(3, 9)]),
        fake(-20.0, vec![(1, 7)]),
        fake(-10.0, vec![(2, 8)]),
        fake(-10.0, vec![(9, 2)]),
        fake(15.0, vec![(1, 8)]),
    ];
    rank_candidates(&mut cs);
    let order: Vec<Vec<(u32, u32)>> = cs.iter().map(|c| c.formed.clone()).collect();
    assert_eq!(
        order,
        vec![
            vec![(1, 7)],
            vec![(2, 8)],
            vec![(9, 2)], // normalized to (2, 9)
            vec![(3, 9)],
            vec![(1, 8)],
        ]
    );
    assert_eq!(listed_count(&cs, 10, 30.0), 4, "15 is 35 above the best");
    assert_eq!(listed_count(&cs, 2, 30.0), 2);
    assert_eq!(listed_count(&cs, 10, 5.0), 1);
    assert_eq!(listed_count(&[], 10, 30.0), 0);
}

#[test]
fn evaluate_is_deterministic_and_self_consistent() {
    // •OH over three silyl radicals: small, so debug relaxations are quick.
    let mut ads = AtomicStructure::new();
    let o = ads.add_atom(O, DVec3::new(0.3, 0.2, 2.2));
    let h = ads.add_atom(H, DVec3::new(0.3, 0.2, 3.17));
    ads.add_bond(o, h, BOND_SINGLE);
    let (mut sub, site_ids) = sites(
        &[
            DVec3::ZERO,
            DVec3::new(2.6, 0.0, 0.0),
            DVec3::new(0.0, 2.9, 0.0),
        ],
        3,
    );
    for id in sub.atom_ids().copied().collect::<Vec<_>>() {
        if !site_ids.contains(&id) {
            sub.set_atom_frozen(id, true);
        }
    }
    let cfg = config(3.5, 1.0);
    let a = search(&ads, &sub, &cfg).unwrap();
    let b = search(&ads, &sub, &cfg).unwrap();
    assert_eq!(a.candidates.len(), 3);
    for (x, y) in a.candidates.iter().zip(&b.candidates) {
        assert_eq!(x.formed, y.formed);
        assert_eq!(x.score.to_bits(), y.score.to_bits());
    }
    assert_eq!(a.stats.relaxed, 3);
    assert_eq!(a.stats.to_relax, 3);
    assert_eq!(a.reference.score, 0.0);
    assert!(a.reference.formed.is_empty());
    for c in &a.candidates {
        assert!((c.score - (c.strain + c.bond_energy)).abs() < 1e-9);
        assert!((c.terms.total() - c.strain).abs() < 1e-6, "{:?}", c.terms);
        // One O–Si bond formed.
        assert!((c.bond_energy + 452.0 / 4.184).abs() < 1e-9);
        let changed: BTreeSet<u32> = c
            .structure
            .atoms_with_tag(CHANGED_TAG)
            .into_iter()
            .collect();
        assert_eq!(changed, BTreeSet::from([c.formed[0].0, c.formed[0].1]));
        assert!(c.structure.has_bond_between(c.formed[0].0, c.formed[0].1));
    }
    assert!(a.candidates.windows(2).all(|w| w[0].score <= w[1].score));
}

/// The van der Waals cutoff — atomCAD's default simulation preference, which
/// the `chemisorb` node follows — keeps a neighbour list instead of pair
/// parameters. The per-term breakdown must still work (it used to panic in
/// `vdw_params`) and still sum to the strain.
#[test]
fn evaluate_works_with_a_vdw_cutoff() {
    use atomcad_crystolecule::simulation::uff::VdwMode;

    let mut ads = AtomicStructure::new();
    let o = ads.add_atom(O, DVec3::new(0.3, 0.2, 2.2));
    let h = ads.add_atom(H, DVec3::new(0.3, 0.2, 3.17));
    ads.add_bond(o, h, BOND_SINGLE);
    let (mut sub, site_ids) = sites(&[DVec3::ZERO, DVec3::new(2.6, 0.0, 0.0)], 3);
    for id in sub.atom_ids().copied().collect::<Vec<_>>() {
        if !site_ids.contains(&id) {
            sub.set_atom_frozen(id, true);
        }
    }
    let cfg = ChemisorptionSearch {
        vdw_mode: VdwMode::Cutoff(6.0),
        ..config(3.5, 1.0)
    };
    let report = search(&ads, &sub, &cfg).unwrap();
    assert_eq!(report.candidates.len(), 2);
    for c in &report.candidates {
        assert!((c.terms.total() - c.strain).abs() < 1e-6, "{:?}", c.terms);
    }
}

#[test]
fn more_legs_outrank_fewer_by_the_bond_energy_term() {
    // The stand-in hexapod over six frozen silyl radicals, one straight below
    // each foot, the reach short enough that a foot sees only its own site:
    // the 63 hypotheses are the non-empty subsets of legs.
    let (ads, foot_ids) = stand_in(6);
    let positions: Vec<DVec3> = foot_ids
        .iter()
        .map(|&f| ads.get_atom(f).unwrap().position - DVec3::Z * 1.7)
        .collect();
    let (mut sub, _) = sites(&positions, 3);
    for id in sub.atom_ids().copied().collect::<Vec<_>>() {
        sub.set_atom_frozen(id, true);
    }
    let cfg = ChemisorptionSearch {
        max_iterations: 500,
        ..config(2.2, 1.0)
    };
    let report = search(&ads, &sub, &cfg).unwrap();
    assert_eq!(report.candidates.len(), 63);
    let best = &report.candidates[0];
    assert_eq!(best.formed.len(), 6, "all six legs bound ranks first");
    let four: Vec<&Candidate> = report
        .candidates
        .iter()
        .filter(|c| c.formed.len() == 4)
        .collect();
    assert!(four.iter().all(|c| c.score > best.score));
    // ΔE_bond is what orders them: six O–Si bonds against four.
    let d_osi = 452.0 / 4.184;
    assert!((best.bond_energy + 6.0 * d_osi).abs() < 1e-9);
    assert!((four[0].bond_energy + 4.0 * d_osi).abs() < 1e-9);
    // Candidates with the same inventory rank in strain order.
    for n in 1..=6 {
        let same: Vec<f64> = report
            .candidates
            .iter()
            .filter(|c| c.formed.len() == n)
            .map(|c| c.strain)
            .collect();
        assert!(same.windows(2).all(|w| w[0] <= w[1]), "{n} legs: {same:?}");
    }
    println!(
        "hexapod: 6 legs strain {:.1} score {:.1}; best 4 legs strain {:.1} score {:.1}",
        best.strain, best.score, four[0].strain, four[0].score
    );
}

// ============================================================================
// Known answer (§8.5): bond forming only
// ============================================================================

/// Ethylene on Si(100)-2×1 ends as the di-σ species on top of one dimer (a
/// four-membered Si–C–C–Si ring), established by experiment and DFT, rather
/// than bridging two dimers of a row or bonding once. With bond forming only,
/// ethylene is posed as its 1,2-diradical over one dimer. Pair pruning is off
/// and the reach long enough to include the neighbouring dimers, so the
/// end-bridge alternatives are relaxed and ranked, not pruned.
///
/// UFF gets the order right with little to spare: on this proxy the di-σ
/// beats the best end-bridge by ~4 kcal/mol (−104.2 against −100.4), and on a
/// 3-cell slab, whose frozen rim is closer, the end-bridge won by 4.7.
#[test]
fn ethanediyl_on_si100_ranks_di_sigma_on_one_dimer_first() {
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    let (ads, _) = ethanediyl((ps + pp) / 2.0 + DVec3::Z * 2.0, pp - ps);
    let cfg = config(4.5, 0.0);
    let report = search(&ads, &slab, &cfg).unwrap();
    let s = &report.reference.structure;
    let site_pair = |c: &Candidate| (c.formed[0].1, c.formed[1].1);

    let doubles: Vec<&Candidate> = report
        .candidates
        .iter()
        .filter(|c| c.formed.len() == 2)
        .collect();
    let end_bridge = doubles
        .iter()
        .find(|c| {
            let (a, b) = site_pair(c);
            !s.has_bond_between(a, b)
        })
        .expect("the reach includes a binding across two dimers");

    // Rank 1 bonds the two atoms of one dimer (the one below, or an
    // equivalent neighbour the molecule slid to).
    let best = &report.candidates[0];
    assert_eq!(best.formed.len(), 2);
    let (a, b) = site_pair(best);
    assert!(s.has_bond_between(a, b), "rank 1 bonds dimer partners");
    assert!(best.converged);
    assert!(best.score < end_bridge.score);
    let best_single = report
        .candidates
        .iter()
        .find(|c| c.formed.len() == 1)
        .unwrap();
    assert!(best.score < best_single.score);
    println!(
        "di-σ: score {:.1} (strain {:.1}); best end-bridge {:.1}; best single {:.1}",
        best.score, best.strain, end_bridge.score, best_single.score
    );
}

// ============================================================================
// Pruning calibration (§8.6) — by hand, release build:
//   cargo test --release -j 4 -p atomcad-crystolecule --test crystolecule \
//       chemisorption_pruning_calibration -- --ignored --nocapture
// ============================================================================

/// The largest `| |s_i − s_j| − |f_i − f_j| |` over a hypothesis's bond pairs
/// at the unrelaxed pose: the pair tolerance it needs to survive pruning.
fn needed_tolerance(combined: &AtomicStructure, formed: &[(u32, u32)]) -> f64 {
    let pos = |id: u32| combined.get_atom(id).unwrap().position;
    let mut worst: f64 = 0.0;
    for (i, &(fi, si)) in formed.iter().enumerate() {
        for &(fj, sj) in &formed[i + 1..] {
            worst = worst.max((pos(si).distance(pos(sj)) - pos(fi).distance(pos(fj))).abs());
        }
    }
    worst
}

/// The stand-in tripod over bare Si(100)-2×1 in 20 poses (5 rotations × 4
/// shifts), brute force (pair tolerance off) against `plan` at the default
/// tolerance: no candidate within the listing window (30 kcal/mol) of its
/// pose's best may be pruned.
///
/// Result of 2026-09-24 (recorded in the design doc, §8.6): the best
/// candidate is a two-leg binding in every pose and needs up to 1.45 Å; the
/// full three-leg binding, ~23 kcal/mol higher, needs 2.64 Å (feet 5.04 Å
/// apart on sites a dimer row apart). The design's 1.0 Å lost the best
/// candidate in most poses, 2.0 Å lost the three-leg binding in 14 of 20; the
/// default is 3.0 Å, which keeps ~75 % of the brute-force relaxations.
#[test]
#[ignore]
fn chemisorption_pruning_calibration() {
    let slab = si100_slab(5.0, 11.0);
    let default_delta = ChemisorptionSearch::default().pair_tolerance;
    let window = 30.0;
    let mut needed_best: f64 = 0.0;
    let mut needed_window: f64 = 0.0;
    let mut lost_in_window = 0;
    let (mut brute_total, mut pruned_total) = (0, 0);
    for rot_deg in [0.0, 30.0, 45.0, 60.0, 90.0] {
        for shift in [
            DVec3::ZERO,
            DVec3::new(1.92, 0.0, 0.0),
            DVec3::new(0.0, 1.92, 0.0),
            DVec3::new(1.36, 1.36, 0.0),
        ] {
            let (mut ads, _) = stand_in(3);
            let q = DQuat::from_rotation_z(f64::to_radians(rot_deg));
            ads.transform(&q, &(shift + DVec3::Z * 1.8));
            let brute_cfg = ChemisorptionSearch {
                pair_tolerance: 0.0,
                ..Default::default()
            };
            let brute_plan = plan(&ads, &slab, &brute_cfg).unwrap();
            let brute = evaluate(&brute_plan, &brute_cfg).unwrap();
            let pruned = plan(&ads, &slab, &ChemisorptionSearch::default()).unwrap();
            let kept: BTreeSet<_> = pruned.hypotheses.iter().map(|h| h.key()).collect();
            brute_total += brute.stats.relaxed;
            pruned_total += pruned.stats.to_relax;

            let best = &brute.candidates[0];
            needed_best = needed_best.max(needed_tolerance(&brute_plan.combined, &best.formed));
            let mut lost = Vec::new();
            for c in brute
                .candidates
                .iter()
                .filter(|c| c.score - best.score <= window)
            {
                let n = needed_tolerance(&brute_plan.combined, &c.formed);
                needed_window = needed_window.max(n);
                if !kept.contains(&c.key()) {
                    lost.push(format!(
                        "+{:.1} ({} legs, δ {:.2})",
                        c.score - best.score,
                        c.formed.len(),
                        n
                    ));
                }
            }
            lost_in_window += lost.len();
            println!(
                "rot {rot_deg:>4}° shift ({:.2},{:.2}): brute {} / pruned {}; \
                 best {:.1} kcal/mol, {} legs; lost within window: {:?}",
                shift.x,
                shift.y,
                brute.stats.relaxed,
                pruned.stats.to_relax,
                best.score,
                best.formed.len(),
                lost
            );
        }
    }
    println!(
        "calibration at δ = {default_delta} Å: {pruned_total} of {brute_total} brute-force \
         relaxations kept; best candidates need δ ≤ {needed_best:.2} Å; the {window} kcal/mol \
         window needs δ ≤ {needed_window:.2} Å ({lost_in_window} candidates lost)"
    );
    assert_eq!(
        lost_in_window, 0,
        "the default pair tolerance prunes a candidate of the listing window"
    );
}

// ============================================================================
// Transfers (§6.1, §8.2)
// ============================================================================

const H_TO_SUBSTRATE: TransferRule = TransferRule {
    element: H,
    direction: TransferDirection::ToSubstrate,
};
const H_TO_ADSORBATE: TransferRule = TransferRule {
    element: H,
    direction: TransferDirection::ToAdsorbate,
};

fn transfer_config(
    reach: f64,
    rules: &[TransferRule],
    max_transfers: usize,
) -> ChemisorptionSearch {
    ChemisorptionSearch {
        transfers: rules.to_vec(),
        max_transfers,
        ..config(reach, 3.0)
    }
}

/// Methanol, CH3–O–H, its O at `p`, the O–H along `h_dir`. Returns (O, C, H).
fn add_methanol(s: &mut AtomicStructure, p: DVec3, h_dir: DVec3) -> (u32, u32, u32) {
    let o = add_methoxy(s, p);
    let c = s.get_atom(o).unwrap().bonds[0].other_atom_id();
    let h = s.add_atom(H, p + h_dir.normalize() * 0.96);
    s.add_bond(o, h, BOND_SINGLE);
    (o, c, h)
}

/// One methanol whose O sits 2 Å above site A, its H leaning towards site B
/// 2.5 Å away (H–A 1.95 Å, H–B 2.34 Å, O–B 3.2 Å).
fn methanol_over_two_sites() -> (AtomicStructure, (u32, u32, u32), AtomicStructure, Vec<u32>) {
    let mut ads = AtomicStructure::new();
    let ids = add_methanol(
        &mut ads,
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(1.0, 0.0, -0.3),
    );
    let (sub, site_ids) = sites(&[DVec3::ZERO, DVec3::new(2.5, 0.0, 0.0)], 3);
    (ads, ids, sub, site_ids)
}

/// Water with its O at `o`, both H in the vertical plane through `toward`,
/// one leaning 20° below it, the other nearly straight up (H–O–H 104.5°).
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

/// One planned hypothesis in input atom ids: formed `(foot, site)` and
/// transfers `(donor, moved, acceptor)`, each sorted.
type Change = (Vec<(u32, u32)>, Vec<(u32, u32, u32)>);

fn planned_changes(p: &SearchPlan) -> BTreeSet<Change> {
    let back: HashMap<u32, u32> = p
        .adsorbate_ids
        .iter()
        .chain(p.substrate_ids.iter())
        .map(|(&k, &v)| (v, k))
        .collect();
    p.hypotheses
        .iter()
        .map(|h| {
            let mut formed: Vec<(u32, u32)> =
                h.formed.iter().map(|(a, s)| (back[a], back[s])).collect();
            formed.sort_unstable();
            let mut moves: Vec<(u32, u32, u32)> = h
                .transfers
                .iter()
                .map(|t| (back[&t.donor], back[&t.moved], back[&t.acceptor]))
                .collect();
            moves.sort_unstable();
            (formed, moves)
        })
        .collect()
}

#[test]
fn an_oh_oxygen_bonds_only_when_its_h_is_transferred() {
    let (ads, (o, _, h), sub, s) = methanol_over_two_sites();
    let (a, b) = (s[0], s[1]);

    // Bond forming alone: the OH oxygen is saturated, nothing can bond.
    let p = plan(&ads, &sub, &config(3.5, 3.0)).unwrap();
    assert!(p.hypotheses.is_empty());
    assert_eq!(p.stats.transfer_candidates, 0);

    // With H → substrate the O can bond, but only to the site its H did not
    // take (each site has one valence).
    let p = plan(&ads, &sub, &transfer_config(3.5, &[H_TO_SUBSTRATE], 1)).unwrap();
    assert_stats_add_up(&p);
    assert_eq!(p.stats.transfer_candidates, 2);
    assert_eq!(
        planned_changes(&p),
        BTreeSet::from([
            (vec![], vec![(o, h, a)]),
            (vec![], vec![(o, h, b)]),
            (vec![(o, a)], vec![(o, h, b)]),
            (vec![(o, b)], vec![(o, h, a)]),
        ])
    );
    for hyp in &p.hypotheses {
        let t = hyp.transfers[0];
        assert!(hyp.formed.iter().all(|&(f, _)| f == t.donor));
        assert_eq!(
            hyp.inventory.broken,
            [(BondKind::new(O, H, 1), 1)].into_iter().collect()
        );
    }
}

#[test]
fn a_pure_abstraction_is_a_hypothesis_and_the_direction_is_respected() {
    // A radical O foot 1.5 Å above the top H of a saturated SiH4.
    let mut ads = AtomicStructure::new();
    let o = add_methoxy(&mut ads, DVec3::new(0.0, 0.0, 3.0));
    let (sub, s) = sites(&[DVec3::ZERO], 4);
    let top_h = sub
        .atoms_values()
        .find(|at| at.atomic_number == H && at.position.z > 1.0)
        .unwrap()
        .id;

    // No site: the SiH4 is saturated.
    assert!(
        plan(&ads, &sub, &config(3.5, 3.0))
            .unwrap()
            .hypotheses
            .is_empty()
    );

    // Abstraction: the H moves to the foot, and nothing else can happen (the
    // foot is then saturated).
    let p = plan(&ads, &sub, &transfer_config(3.5, &[H_TO_ADSORBATE], 1)).unwrap();
    assert_stats_add_up(&p);
    assert_eq!(
        planned_changes(&p),
        BTreeSet::from([(vec![], vec![(s[0], top_h, o)])])
    );

    // The other direction offers nothing here: the adsorbate's H are methyl
    // H, and the substrate has no atom to take one.
    let p = plan(&ads, &sub, &transfer_config(3.5, &[H_TO_SUBSTRATE], 1)).unwrap();
    assert!(p.hypotheses.is_empty());
    assert_eq!(p.stats.transfer_candidates, 0);
}

#[test]
fn transfer_candidates_respect_element_tags_frozen_atoms_and_reach() {
    let count = |ads: &AtomicStructure, sub: &AtomicStructure, cfg: &ChemisorptionSearch| {
        plan(ads, sub, cfg).unwrap().stats.transfer_candidates
    };
    let (ads, (o, c, h), sub, s) = methanol_over_two_sites();
    let cfg = transfer_config(3.5, &[H_TO_SUBSTRATE], 1);
    assert_eq!(count(&ads, &sub, &cfg), 2);

    // The element must match (no Cl here).
    let chlorine = TransferRule {
        element: 17,
        ..H_TO_SUBSTRATE
    };
    assert_eq!(count(&ads, &sub, &transfer_config(3.5, &[chlorine], 1)), 0);

    // The donor must be reactive: tags select the donor, never the H.
    let mut tagged = ads.clone();
    tagged.add_atom_tag(c, "chosen").unwrap();
    let cfg_tagged = ChemisorptionSearch {
        adsorbate_tag: Some("chosen".to_string()),
        ..cfg.clone()
    };
    assert_eq!(count(&tagged, &sub, &cfg_tagged), 0);
    tagged.add_atom_tag(o, "chosen").unwrap();
    assert_eq!(count(&tagged, &sub, &cfg_tagged), 2);

    // Any frozen D, X or A excludes the triple.
    for frozen in [o, h] {
        let mut f = ads.clone();
        f.set_atom_frozen(frozen, true);
        assert_eq!(count(&f, &sub, &cfg), 0);
    }
    let mut f = sub.clone();
    f.set_atom_frozen(s[0], true);
    assert_eq!(count(&ads, &f, &cfg), 1);

    // Reach is measured from the moving atom: at 2.5 Å the H still reaches
    // site B (2.34 Å) although the O does not (3.2 Å).
    let p = plan(&ads, &sub, &transfer_config(2.5, &[H_TO_SUBSTRATE], 1)).unwrap();
    assert_eq!(p.stats.transfer_candidates, 2);
    assert!(planned_changes(&p).contains(&(vec![(o, s[0])], vec![(o, h, s[1])])));
    let p = plan(&ads, &sub, &transfer_config(2.0, &[H_TO_SUBSTRATE], 1)).unwrap();
    assert_eq!(p.stats.transfer_candidates, 1);

    // Settings no search can honour.
    let oxygen = TransferRule {
        element: O,
        ..H_TO_SUBSTRATE
    };
    assert!(matches!(
        plan(&ads, &sub, &transfer_config(3.5, &[oxygen], 1)),
        Err(ChemisorptionError::InvalidConfig(_))
    ));
    assert!(matches!(
        plan(&ads, &sub, &transfer_config(3.5, &[H_TO_SUBSTRATE], 0)),
        Err(ChemisorptionError::InvalidConfig(_))
    ));
}

#[test]
fn max_transfers_bounds_the_total_over_all_rules() {
    // Two methanols, each over its own pair of sites.
    let mut ads = AtomicStructure::new();
    for x in [0.0, 6.0] {
        add_methanol(
            &mut ads,
            DVec3::new(x, 0.0, 2.0),
            DVec3::new(1.0, 0.0, -0.3),
        );
    }
    let (sub, _) = sites(
        &[
            DVec3::ZERO,
            DVec3::new(2.5, 0.0, 0.0),
            DVec3::new(6.0, 0.0, 0.0),
            DVec3::new(8.5, 0.0, 0.0),
        ],
        3,
    );
    let rules = [H_TO_SUBSTRATE, H_TO_ADSORBATE];
    let planned = |max: usize| {
        let p = plan(&ads, &sub, &transfer_config(3.5, &rules, max)).unwrap();
        assert_stats_add_up(&p);
        p
    };
    let most = |p: &SearchPlan| {
        p.hypotheses
            .iter()
            .map(|h| h.transfers.len())
            .max()
            .unwrap()
    };
    let (one, two) = (planned(1), planned(2));
    assert_eq!(most(&one), 1);
    assert_eq!(most(&two), 2);
    // Both legs bonded needs both H moved.
    assert!(one.hypotheses.iter().all(|h| h.formed.len() <= 1));
    assert!(two.hypotheses.iter().any(|h| h.formed.len() == 2));
}

#[test]
fn equivalent_hydrogens_on_one_donor_are_one_transfer() {
    // Water with both H pointing down, between two sites that both reach.
    let mut ads = AtomicStructure::new();
    let o = ads.add_atom(O, DVec3::new(0.0, 0.0, 2.2));
    let half_angle = 52.25f64.to_radians();
    for sign in [-1.0, 1.0] {
        let h = ads.add_atom(
            H,
            DVec3::new(
                sign * half_angle.sin() * 0.96,
                0.0,
                2.2 - half_angle.cos() * 0.96,
            ),
        );
        ads.add_bond(o, h, BOND_SINGLE);
    }
    let (sub, s) = sites(&[DVec3::new(-1.5, 0.0, 0.0), DVec3::new(1.5, 0.0, 0.0)], 3);

    let p = plan(&ads, &sub, &transfer_config(3.5, &[H_TO_SUBSTRATE], 2)).unwrap();
    assert_stats_add_up(&p);
    assert_eq!(p.stats.transfer_candidates, 4);
    // One H to either site, the OH then bonding to the other; or both H. Which
    // H moved does not matter: the shapes are (formed, acceptors).
    let back: HashMap<u32, u32> = p
        .adsorbate_ids
        .iter()
        .chain(p.substrate_ids.iter())
        .map(|(&k, &v)| (v, k))
        .collect();
    type Shape = (Vec<(u32, u32)>, Vec<u32>);
    let shapes: BTreeSet<Shape> = p
        .hypotheses
        .iter()
        .map(|h| {
            let mut acceptors: Vec<u32> = h.transfers.iter().map(|t| back[&t.acceptor]).collect();
            acceptors.sort_unstable();
            let formed = h
                .formed
                .iter()
                .map(|&(f, x)| (back[&f], back[&x]))
                .collect();
            (formed, acceptors)
        })
        .collect();
    assert_eq!(p.hypotheses.len(), 5, "{shapes:?}");
    assert_eq!(
        shapes,
        BTreeSet::from([
            (vec![], vec![s[0]]),
            (vec![], vec![s[1]]),
            (vec![(o, s[1])], vec![s[0]]),
            (vec![(o, s[0])], vec![s[1]]),
            (vec![], vec![s[0], s[1]]),
        ])
    );
    // Moving the other H, singly or crossed, was merged, not relaxed again.
    assert!(p.stats.duplicates >= 3, "{:?}", p.stats);
    let keys: BTreeSet<_> = p.hypotheses.iter().map(|h| h.key()).collect();
    assert_eq!(keys.len(), p.hypotheses.len());
}

#[test]
fn a_transferred_atom_is_seated_on_its_acceptor_from_the_side_it_came_from() {
    let (ads, (o, _, h), sub, s) = methanol_over_two_sites();
    let p = plan(&ads, &sub, &transfer_config(3.5, &[H_TO_SUBSTRATE], 1)).unwrap();
    let t = Transfer {
        donor: p.adsorbate_ids[&o],
        moved: p.adsorbate_ids[&h],
        acceptor: p.substrate_ids[&s[1]],
        element: H,
    };
    let pos = |id: u32| p.combined.get_atom(id).unwrap().position;
    let seat = t.seat(&p.combined);
    let a = pos(t.acceptor);
    assert!((seat.distance(a) - terminator_bond_length(SI, H)).abs() < 1e-9);
    let (towards_old, placed) = ((pos(t.moved) - a).normalize(), (seat - a).normalize());
    assert!(towards_old.dot(placed) > 1.0 - 1e-9);
}

#[test]
fn a_transfer_candidate_is_relaxed_and_scored_with_both_bonds() {
    let (ads, _, mut sub, s) = methanol_over_two_sites();
    for id in sub.atom_ids().copied().collect::<Vec<_>>() {
        if !s.contains(&id) {
            sub.set_atom_frozen(id, true);
        }
    }
    let report = search(&ads, &sub, &transfer_config(3.5, &[H_TO_SUBSTRATE], 1)).unwrap();
    assert_eq!(report.candidates.len(), 4);
    assert_eq!(report.stats.transfer_candidates, 2);
    for c in &report.candidates {
        let t = c.transfers[0];
        assert_eq!(c.moved(), vec![t.moved]);
        assert_eq!(c.broken(), vec![(t.donor, t.moved)]);
        let st = &c.structure;
        assert!(st.has_bond_between(t.moved, t.acceptor));
        assert!(!st.has_bond_between(t.donor, t.moved));
        let changed: BTreeSet<u32> = st.atoms_with_tag(CHANGED_TAG).into_iter().collect();
        assert!(changed.is_superset(&BTreeSet::from([t.donor, t.moved, t.acceptor])));
        // Break O–H, form Si–H, and Si–O when the O bonds.
        let formed_o = if c.formed.is_empty() { 0.0 } else { 452.0 };
        let expected = (463.0 - 318.0 - formed_o) / 4.184;
        assert!(
            (c.bond_energy - expected).abs() < 1e-9,
            "{}",
            c.bond_inventory
        );
        assert!((c.terms.total() - c.strain).abs() < 1e-6);
    }
    // The dissociated OH + H outranks a bare H transfer by the Si–O bond.
    assert_eq!(report.candidates[0].formed.len(), 1);
}

#[test]
fn the_fingerprint_follows_transfers_and_ignores_an_unused_max_transfers() {
    let (ads, _, sub, _) = methanol_over_two_sites();
    let fp = |cfg: &ChemisorptionSearch| input_fingerprint(&ads, &sub, cfg);
    let off = config(3.5, 3.0);
    let off_2 = ChemisorptionSearch {
        max_transfers: 2,
        ..off.clone()
    };
    assert_eq!(
        fp(&off),
        fp(&off_2),
        "max_transfers is unread without rules"
    );
    let on = transfer_config(3.5, &[H_TO_SUBSTRATE], 1);
    assert_ne!(fp(&off), fp(&on));
    assert_ne!(fp(&on), fp(&transfer_config(3.5, &[H_TO_SUBSTRATE], 2)));
    assert_ne!(fp(&on), fp(&transfer_config(3.5, &[H_TO_ADSORBATE], 1)));
}

// ============================================================================
// Known answer (§8.5): water, by an H transfer
// ============================================================================

/// Water on Si(100)-2×1 dissociates to H + OH on the two atoms of one dimer
/// (vibrational spectroscopy and DFT agree). Posed with its O above one dimer
/// atom and an H leaning towards the partner, with H → substrate enabled and a
/// reach long enough that the H and the OH can also end on two dimers.
///
/// **UFF does not reproduce the known answer; it ties.** Dissociation itself
/// is clear — H + OH beats a bare H transfer by the Si–O bond, ~108 kcal/mol —
/// but "one dimer" against "two dimers" is the same bond inventory, so only
/// strain separates them, and UFF has no term for what really decides it (the
/// pairing of the two dangling bonds of a dimer). Over 4-, 5- and 6-cell
/// proxies, three poses and both vdW modes the two differ by at most
/// 0.4 kcal/mol and the winner flips with the pose (here the one-dimer answer
/// happens to lead by 0.2). The test pins the tie, not the order; the
/// reference guide says the same.
#[test]
fn water_on_si100_dissociates_but_ties_one_dimer_against_two() {
    let slab = si100_slab(5.0, 9.0);
    let (site, partner) = central_dimer(&slab);
    let (ps, pp) = (
        slab.get_atom(site).unwrap().position,
        slab.get_atom(partner).unwrap().position,
    );
    let ads = water(ps + DVec3::Z * 1.9, pp - ps);
    let cfg = transfer_config(4.5, &[H_TO_SUBSTRATE], 1);
    let report = search(&ads, &slab, &cfg).unwrap();
    let s = &report.reference.structure;

    let dissociated = |c: &&Candidate| c.formed.len() == 1 && c.transfers.len() == 1;
    let across = |c: &Candidate| !s.has_bond_between(c.formed[0].1, c.transfers[0].acceptor);
    let one_dimer = report
        .candidates
        .iter()
        .filter(dissociated)
        .find(|c| !across(c))
        .expect("H and OH on dimer partners is enumerated");
    let two_dimers = report
        .candidates
        .iter()
        .filter(dissociated)
        .find(|c| across(c))
        .expect("the reach includes H and OH on two dimers");

    let best = &report.candidates[0];
    assert!(dissociated(&best), "rank 1 is H + OH");
    assert!(best.converged);
    assert!(
        (one_dimer.score - two_dimers.score).abs() < 1.0,
        "UFF ties one dimer against two: {:.2} vs {:.2}",
        one_dimer.score,
        two_dimers.score
    );
    let bare = report
        .candidates
        .iter()
        .find(|c| c.formed.is_empty())
        .unwrap();
    assert!(
        bare.score - best.score > 100.0,
        "the Si–O bond separates them"
    );
    println!(
        "water: H + OH on one dimer {:.1}, on two dimers {:.1}; bare H transfer {:.1}; {} relaxed, {} duplicates",
        one_dimer.score,
        two_dimers.score,
        bare.score,
        report.stats.relaxed,
        report.stats.duplicates
    );
}

/// The design's development sample: the stand-in tripod with its three feet
/// protected as OH, each H leaning outwards and down. Every leg that bonds
/// must have handed its own H to a site, and with three transfers allowed the
/// full three-leg binding is found — over the calibration's 20 poses, since a
/// single pose may not have six sites in reach.
#[test]
fn the_tripod_with_oh_feet_binds_by_handing_off_its_hydrogens() {
    let slab = si100_slab(5.0, 9.0);
    let (mut three_leg_poses, mut hypotheses) = (0, 0);
    let mut slowest: f64 = 0.0;
    for rot_deg in [0.0, 30.0, 45.0, 60.0, 90.0] {
        for shift in [
            DVec3::ZERO,
            DVec3::new(1.92, 0.0, 0.0),
            DVec3::new(0.0, 1.92, 0.0),
            DVec3::new(1.36, 1.36, 0.0),
        ] {
            let (mut ads, feet_ids) = stand_in(3);
            let mut feet_h = HashMap::new();
            for &f in &feet_ids {
                let p = ads.get_atom(f).unwrap().position;
                let out = DVec3::new(p.x, p.y, 0.0).normalize();
                let h = ads.add_atom(H, p + (out * 0.6 - DVec3::Z * 0.8).normalize() * 0.96);
                ads.add_bond(f, h, BOND_SINGLE);
                feet_h.insert(f, h);
                // Only the feet are reactive, so the cage's C–H stay out of it.
                ads.add_atom_tag(f, "feet").unwrap();
            }
            let q = DQuat::from_rotation_z(f64::to_radians(rot_deg));
            ads.transform(&q, &(shift + DVec3::Z * 1.8));

            let cfg = ChemisorptionSearch {
                adsorbate_tag: Some("feet".to_string()),
                ..transfer_config(3.5, &[H_TO_SUBSTRATE], 3)
            };
            let start = std::time::Instant::now();
            let p = plan(&ads, &slab, &cfg).unwrap();
            slowest = slowest.max(start.elapsed().as_secs_f64());
            assert_stats_add_up(&p);
            assert!(!p.stats.truncated);

            let back: HashMap<u32, u32> = p.adsorbate_ids.iter().map(|(&k, &v)| (v, k)).collect();
            for h in &p.hypotheses {
                for &(f, _) in &h.formed {
                    assert!(
                        h.transfers
                            .iter()
                            .any(|t| t.donor == f && back[&t.moved] == feet_h[&back[&f]]),
                        "a leg bonded without handing off its H"
                    );
                }
            }
            hypotheses += p.stats.to_relax;
            if p.hypotheses.iter().any(|h| h.formed.len() == 3) {
                three_leg_poses += 1;
            }
        }
    }
    assert!(three_leg_poses > 0);
    println!(
        "OH tripod: {hypotheses} hypotheses over 20 poses, three-leg binding in {three_leg_poses}; slowest plan {slowest:.3} s"
    );
}
