//! Tests for `proxy_cut` — the bond-hop proxy cut (`doc/design_proxy_node.md`).
//!
//! The analysis half — riders, distances, the keep set (`fill` and
//! `rm_single`), the severed-bond caps, the frozen / high lists and
//! `nearest_dropped` — plans without touching a structure; the apply half
//! carries a plan out, reports on it, and is what `proxy_cut` drives by tag
//! name.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::atomic_structure_utils::empirical_formula;
use atomcad_crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use atomcad_crystolecule::hydrogen_passivation::{
    AddHydrogensOptions, add_hydrogens, terminator_bond_length,
};
use atomcad_crystolecule::lattice_fill::{LatticeFillConfig, LatticeFillOptions, fill_lattice};
use atomcad_crystolecule::motif::Motif;
use atomcad_crystolecule::proxy_cut::{
    CapPlacement, ProxyError, ProxyOptions, apply_proxy, bond_distances, classify_riders,
    plan_proxy, proxy_cut, severed_bond_caps,
};
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_geo_tree::GeoNode;
use atomcad_util::daabox::DAABox;
use glam::f64::DVec3;
use rustc_hash::FxHashMap;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

// =============================================================================
// Fixtures
// =============================================================================

/// Silicon lattice constant (Å) and the Si–Si bond it implies.
const SI_A: f64 = 5.431;
const SI_SI: f64 = SI_A * 0.4330127018922193; // a·√3/4
const SI_H: f64 = 1.48;

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

fn silicon_motif() -> Motif {
    let mut motif = DEFAULT_ZINCBLENDE_MOTIF.clone();
    for p in &mut motif.parameters {
        p.default_atomic_number = 14;
    }
    motif
}

/// A hydrogen-passivated, unreconstructed silicon cube `cells` unit cells a
/// side. The `hops = 6` filled cut around the centre atom reaches at most
/// ~12 Å, so eight cells (43 Å) keeps every shell clear of the surface.
fn build_silicon_cube(cells: f64) -> AtomicStructure {
    let cell = UnitCellStruct::new(
        DVec3::new(SI_A, 0.0, 0.0),
        DVec3::new(0.0, SI_A, 0.0),
        DVec3::new(0.0, 0.0, SI_A),
    );
    let config = LatticeFillConfig {
        unit_cell: cell,
        motif: silicon_motif(),
        parameter_element_values: HashMap::new(),
        geometry: axis_aligned_box(DVec3::ZERO, DVec3::splat(cells * SI_A)),
        motif_offset: DVec3::ZERO,
        regions: Vec::new(),
    };
    let options = LatticeFillOptions {
        hydrogen_passivation: true,
        remove_unbonded_atoms: true,
        remove_single_bond_atoms: false,
        reconstruct_surface: false,
        invert_phase: false,
        rebond_concave_clashes: true,
        passivation_element: 1,
    };
    let margin = 5.0;
    let fill_region = DAABox::new(DVec3::splat(-margin), DVec3::splat(cells * SI_A + margin));
    fill_lattice(&config, &options, &fill_region).atomic_structure
}

/// The eight-cell cube, built once for the whole harness.
fn silicon_cube() -> &'static AtomicStructure {
    static CUBE: OnceLock<AtomicStructure> = OnceLock::new();
    CUBE.get_or_init(|| build_silicon_cube(8.0))
}

/// The silicon atom nearest the centre of the cube — the single source of
/// every bulk-row test.
fn cube_center_source(structure: &AtomicStructure) -> u32 {
    let center = DVec3::splat(4.0 * SI_A);
    structure
        .atoms_values()
        .filter(|a| a.atomic_number == 14)
        .min_by(|a, b| {
            a.position
                .distance_squared(center)
                .total_cmp(&b.position.distance_squared(center))
        })
        .expect("the cube has silicon atoms")
        .id
}

fn add(structure: &mut AtomicStructure, z: i16, position: DVec3) -> u32 {
    structure.add_atom(z, position)
}

fn bond(structure: &mut AtomicStructure, a: u32, b: u32) {
    structure.add_bond(a, b, BOND_SINGLE);
}

/// A planar six-ring of carbon centred on `center`, radius 1.4 Å, in the
/// xy-plane. Every atom has two bonds, so none of them is a rider.
fn six_ring(structure: &mut AtomicStructure, center: DVec3) -> Vec<u32> {
    let mut ids = Vec::new();
    for i in 0..6 {
        let angle = std::f64::consts::TAU * (i as f64) / 6.0;
        ids.push(add(
            structure,
            6,
            center + DVec3::new(1.4 * angle.cos(), 1.4 * angle.sin(), 0.0),
        ));
    }
    for i in 0..6 {
        bond(structure, ids[i], ids[(i + 1) % 6]);
    }
    ids
}

/// A straight carbon chain of `n` atoms along +x at 1.54 Å, hydrogen-capped at
/// both ends so that every chain atom is heavy.
fn capped_chain(structure: &mut AtomicStructure, origin: DVec3, n: usize) -> Vec<u32> {
    let ids: Vec<u32> = (0..n)
        .map(|i| add(structure, 6, origin + DVec3::new(1.54 * i as f64, 0.0, 0.0)))
        .collect();
    for i in 1..n {
        bond(structure, ids[i - 1], ids[i]);
    }
    let head = add(structure, 1, origin - DVec3::new(1.09, 0.0, 0.0));
    bond(structure, ids[0], head);
    let tail = add(
        structure,
        1,
        origin + DVec3::new(1.54 * (n - 1) as f64 + 1.09, 0.0, 0.0),
    );
    bond(structure, ids[n - 1], tail);
    ids
}

fn sorted_ids(mut ids: Vec<u32>) -> Vec<u32> {
    ids.sort_unstable();
    ids
}

/// The crate defaults at a given `hops`, with `rm_single` **off**: the §4.3
/// figures and every nesting property asserted below are stated for the
/// untrimmed cut.
fn default_options(hops: u32) -> ProxyOptions {
    ProxyOptions {
        hops,
        rm_single: false,
        ..Default::default()
    }
}

/// The closest two caps come to each other, over the whole plan.
fn min_cap_pair(caps: &[CapPlacement]) -> Option<f64> {
    let mut best: Option<f64> = None;
    for (i, a) in caps.iter().enumerate() {
        for b in &caps[i + 1..] {
            let d = a.position.distance(b.position);
            if best.is_none_or(|x| d < x) {
                best = Some(d);
            }
        }
    }
    best
}

// =============================================================================
// Riders (§4.1)
// =============================================================================

#[test]
fn rider_is_any_singly_bonded_atom() {
    let mut s = AtomicStructure::new();
    let chain = capped_chain(&mut s, DVec3::ZERO, 3);
    // A singly bonded heavy adatom hanging off the middle carbon.
    let adatom = add(&mut s, 14, DVec3::new(1.54, 1.9, 0.0));
    bond(&mut s, chain[1], adatom);
    // An isolated atom, no bonds at all.
    let lone = add(&mut s, 6, DVec3::new(0.0, 10.0, 0.0));

    let riders = classify_riders(&s);

    // The two hydrogen caps are riders mapped to their hosts.
    let hydrogens: Vec<u32> = s
        .atoms_values()
        .filter(|a| a.atomic_number == 1)
        .map(|a| a.id)
        .collect();
    assert_eq!(hydrogens.len(), 2);
    assert_eq!(riders.get(&hydrogens[0]), Some(&chain[0]));
    assert_eq!(riders.get(&hydrogens[1]), Some(&chain[2]));

    // A singly bonded heavy adatom is a rider too — a rider is a graph
    // property, not an element property.
    assert_eq!(riders.get(&adatom), Some(&chain[1]));

    // An atom with no bond is heavy, not a rider.
    assert!(!riders.contains_key(&lone));
    // The chain atoms all have two or more bonds.
    for id in &chain {
        assert!(!riders.contains_key(id));
    }
}

#[test]
fn rider_source_promotes_its_host_and_is_kept() {
    let mut s = AtomicStructure::new();
    let chain = capped_chain(&mut s, DVec3::ZERO, 3);
    let head_h = s
        .atoms_values()
        .find(|a| a.atomic_number == 1)
        .expect("a cap")
        .id;

    let options = ProxyOptions {
        hops: 0,
        fill: false,
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[head_h], &options).expect("plan");

    // The host is at distance 0, not the rider.
    assert_eq!(plan.distance.get(&chain[0]), Some(&0));
    assert!(!plan.distance.contains_key(&head_h));
    // The rider follows its host into the keep set.
    assert_eq!(plan.kept, sorted_ids(vec![chain[0], head_h]));
}

#[test]
fn rider_whose_host_is_a_rider_cannot_anchor_a_cut() {
    // An isolated diatomic: both atoms have exactly one bond.
    let mut s = AtomicStructure::new();
    let a = add(&mut s, 6, DVec3::ZERO);
    let b = add(&mut s, 1, DVec3::new(1.09, 0.0, 0.0));
    bond(&mut s, a, b);

    let riders = classify_riders(&s);
    assert_eq!(riders.get(&a), Some(&b));
    assert_eq!(riders.get(&b), Some(&a));

    let err = plan_proxy(&s, &[b], &ProxyOptions::default()).unwrap_err();
    assert!(matches!(err, ProxyError::NoFocusAtoms));
}

// =============================================================================
// Distances (§4.2)
// =============================================================================

#[test]
fn distances_are_graph_distances_over_heavy_atoms() {
    let mut s = AtomicStructure::new();
    let chain = capped_chain(&mut s, DVec3::ZERO, 6);
    let ring = six_ring(&mut s, DVec3::new(0.0, 20.0, 0.0));

    let riders = classify_riders(&s);

    let d = bond_distances(&s, &[chain[0]], &riders);
    for (i, id) in chain.iter().enumerate() {
        assert_eq!(d.get(id), Some(&(i as u32)), "chain atom {i}");
    }
    // Riders are never traversed and get no distance.
    for (rider, _) in riders.iter() {
        assert!(!d.contains_key(rider), "rider {rider} got a distance");
    }
    // A second component gets no distance.
    for id in &ring {
        assert!(
            !d.contains_key(id),
            "ring atom {id} is in another component"
        );
    }

    // The six-ring: 0,1,2,3,2,1 around the cycle.
    let dr = bond_distances(&s, &[ring[0]], &riders);
    assert_eq!(
        ring.iter().map(|id| dr[id]).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 2, 1]
    );
}

#[test]
fn multi_source_fragments_merge_and_a_third_is_dropped() {
    let mut s = AtomicStructure::new();
    let tool = capped_chain(&mut s, DVec3::new(0.0, 0.0, 4.0), 4);
    let surface = six_ring(&mut s, DVec3::ZERO);
    let bystander = six_ring(&mut s, DVec3::new(0.0, 40.0, 0.0));

    let options = ProxyOptions {
        hops: 6,
        rim: 0,
        fill: false,
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[tool[0], surface[0]], &options).expect("plan");

    for id in tool.iter().chain(surface.iter()) {
        assert!(plan.kept.contains(id), "atom {id} should be kept");
    }
    for id in &bystander {
        assert!(plan.dropped.contains(id), "atom {id} should be dropped");
        assert!(!plan.distance.contains_key(id));
    }
}

#[test]
fn hops_zero_keeps_the_sources_and_their_riders_only() {
    let mut s = AtomicStructure::new();
    let ring = six_ring(&mut s, DVec3::ZERO);
    let h = add(&mut s, 1, DVec3::new(1.4, 0.0, 1.09));
    bond(&mut s, ring[0], h);

    let options = ProxyOptions {
        hops: 0,
        fill: false,
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[ring[0]], &options).expect("plan");
    assert_eq!(plan.kept, sorted_ids(vec![ring[0], h]));
}

#[test]
fn fill_restores_a_heavy_atom_bridging_two_sources() {
    // A "V": two sources with one shared neighbour, which the plain `hops = 0`
    // cut drops and `fill` restores.
    let mut s = AtomicStructure::new();
    let left = add(&mut s, 6, DVec3::new(-1.54, 0.0, 0.0));
    let bridge = add(&mut s, 6, DVec3::ZERO);
    let right = add(&mut s, 6, DVec3::new(1.54, 0.0, 0.0));
    // Keep the two sources off the rider list.
    let left2 = add(&mut s, 6, DVec3::new(-1.54, 1.54, 0.0));
    let right2 = add(&mut s, 6, DVec3::new(1.54, 1.54, 0.0));
    bond(&mut s, left, bridge);
    bond(&mut s, bridge, right);
    bond(&mut s, left, left2);
    bond(&mut s, right, right2);

    let plain = plan_proxy(
        &s,
        &[left, right],
        &ProxyOptions {
            hops: 0,
            fill: false,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    assert!(!plain.kept.contains(&bridge));

    let filled = plan_proxy(
        &s,
        &[left, right],
        &ProxyOptions {
            hops: 0,
            fill: true,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    assert!(filled.kept.contains(&bridge));
    assert_eq!(filled.filled, vec![bridge]);
    assert_eq!(filled.fill_rounds, 1);
    // Its true BFS distance is beyond `hops`.
    assert_eq!(filled.distance[&bridge], 1);
}

// =============================================================================
// The §4.3 fill table, on bulk silicon
// =============================================================================

/// Dropped heavy atoms with two or more kept heavy neighbours — the shared
/// sites that would receive a clashing pair of caps.
fn shared_sites(structure: &AtomicStructure, kept_heavy: &[u32]) -> usize {
    use std::collections::{HashMap as Map, HashSet as Set};
    let riders = classify_riders(structure);
    let kept: Set<u32> = kept_heavy.iter().copied().collect();
    let mut counts: Map<u32, usize> = Map::new();
    for &a in kept_heavy {
        let Some(atom) = structure.get_atom(a) else {
            continue;
        };
        for b in atom.bonds.iter().map(|b| b.other_atom_id()) {
            if riders.contains_key(&b) || kept.contains(&b) {
                continue;
            }
            *counts.entry(b).or_insert(0) += 1;
        }
    }
    counts.values().filter(|&&c| c >= 2).count()
}

/// The kept **heavy** atoms of a plan, in id order.
fn kept_heavy(
    structure: &AtomicStructure,
    plan: &atomcad_crystolecule::proxy_cut::ProxyPlan,
) -> Vec<u32> {
    let riders = classify_riders(structure);
    plan.kept
        .iter()
        .copied()
        .filter(|id| !riders.contains_key(id))
        .collect()
}

fn farthest_hop(plan: &atomcad_crystolecule::proxy_cut::ProxyPlan, heavy: &[u32]) -> u32 {
    heavy
        .iter()
        .filter_map(|id| plan.distance.get(id).copied())
        .max()
        .unwrap_or(0)
}

#[test]
fn fill_table_bulk_silicon_hops_4() {
    let s = silicon_cube();
    let source = cube_center_source(s);

    let plain = plan_proxy(
        s,
        &[source],
        &ProxyOptions {
            hops: 4,
            fill: false,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    let plain_heavy = kept_heavy(s, &plain);
    assert_eq!(plain_heavy.len(), 83, "kept heavy atoms before fill");
    assert_eq!(shared_sites(s, &plain_heavy), 40, "shared sites");
    let before = min_cap_pair(&plain.caps).expect("cap pairs");
    assert!(
        (before - 1.42).abs() < 0.01,
        "clashing cap pair before fill: {before}"
    );

    let filled = plan_proxy(s, &[source], &default_options(4)).expect("plan");
    let filled_heavy = kept_heavy(s, &filled);
    assert_eq!(filled_heavy.len(), 165, "kept heavy atoms after fill");
    assert_eq!(filled.fill_rounds, 4, "fill rounds");
    assert_eq!(shared_sites(s, &filled_heavy), 0, "no shared site survives");
    let after = min_cap_pair(&filled.caps).expect("cap pairs");
    assert!(
        (after - 2.42).abs() < 0.01,
        "closest cap pair after fill: {after}"
    );
    assert_eq!(farthest_hop(&filled, &filled_heavy), 8, "farthest hop");

    // Every atom fill restored lies beyond `hops`.
    for id in &filled.filled {
        assert!(filled.distance[id] > 4, "filled atom {id} within hops");
    }
    assert_eq!(filled.filled.len(), 165 - 83);
}

#[test]
fn fill_table_bulk_silicon_hops_6() {
    let s = silicon_cube();
    let source = cube_center_source(s);

    let plain = plan_proxy(
        s,
        &[source],
        &ProxyOptions {
            hops: 6,
            fill: false,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    assert_eq!(kept_heavy(s, &plain).len(), 239, "kept heavy before fill");

    let filled = plan_proxy(s, &[source], &default_options(6)).expect("plan");
    let filled_heavy = kept_heavy(s, &filled);
    assert_eq!(filled_heavy.len(), 455, "kept heavy after fill");
    assert_eq!(filled.fill_rounds, 6, "fill rounds");
    assert_eq!(farthest_hop(&filled, &filled_heavy), 12, "farthest hop");
    let after = min_cap_pair(&filled.caps).expect("cap pairs");
    assert!((after - 2.42).abs() < 0.01, "closest cap pair: {after}");
}

/// Phase 5: the figures are a property of the **cut**, not of the workpiece.
///
/// Doubling the cube to sixteen cells a side — ~33k silicon atoms plus their
/// surface hydrogens — must reproduce the §4.3 rows byte for byte. Two things
/// are actually under test. The BFS is deliberately *unbounded* (§7.3 step 3),
/// so it walks the whole component whatever its size; if it ever grew a
/// `hops`-bounded early exit, `farthest_hop` and the fill rounds would still
/// look right on the small cube and drift here. And `nearest_dropped` queries
/// the spatial grid rather than scanning (§7.3 step 11) — on the bigger cube
/// the dropped set is an order of magnitude larger while the free set is
/// identical, so a scan would show up as time rather than as a wrong number.
///
/// No timing assertion: this is about the figures, and a threshold would be
/// the flakiest line in the harness.
#[test]
fn the_fill_figures_do_not_depend_on_the_workpiece_size() {
    let small = silicon_cube();
    let big = build_silicon_cube(16.0);

    // 32768 silicon atoms against 4096, but the passivating hydrogens scale
    // with the *area*, so the totals are ~6.6x rather than 8x.
    assert!(
        big.atom_ids().count() > 6 * small.atom_ids().count(),
        "the big cube really is the bigger one: {} vs {}",
        big.atom_ids().count(),
        small.atom_ids().count()
    );

    for hops in [4u32, 6] {
        let small_plan = plan_proxy(small, &[cube_center_source(small)], &default_options(hops))
            .expect("plan the small cube");
        let big_plan = plan_proxy(&big, &[cube_center_source(&big)], &default_options(hops))
            .expect("plan the big cube");

        let small_heavy = kept_heavy(small, &small_plan);
        let big_heavy = kept_heavy(&big, &big_plan);

        assert_eq!(
            small_heavy.len(),
            big_heavy.len(),
            "kept heavy atoms at hops = {hops}"
        );
        assert_eq!(
            small_plan.fill_rounds, big_plan.fill_rounds,
            "fill rounds at hops = {hops}"
        );
        assert_eq!(
            small_plan.filled.len(),
            big_plan.filled.len(),
            "fill-restored atoms at hops = {hops}"
        );
        assert_eq!(
            small_plan.caps.len(),
            big_plan.caps.len(),
            "caps at hops = {hops}"
        );
        assert_eq!(
            farthest_hop(&small_plan, &small_heavy),
            farthest_hop(&big_plan, &big_heavy),
            "farthest hop at hops = {hops}"
        );
        assert_eq!(
            shared_sites(&big, &big_heavy),
            0,
            "fill converged on the big cube at hops = {hops}"
        );

        let small_pair = min_cap_pair(&small_plan.caps).expect("cap pairs");
        let big_pair = min_cap_pair(&big_plan.caps).expect("cap pairs");
        assert!(
            (small_pair - big_pair).abs() < 1e-9,
            "closest cap pair at hops = {hops}: {small_pair} vs {big_pair}"
        );

        // `nearest_dropped` is the one figure that *should* move: the small
        // cube's free atoms sit near its own surface, so beyond the cut there
        // is nothing to find within the 8 A radius, while the big cube is bulk
        // in every direction. Both are `Some` here, and the big cube's value is
        // the honest one.
        assert!(
            big_plan.nearest_dropped.is_some(),
            "the big cube has dropped heavy atoms near its free ones"
        );
    }
}

// =============================================================================
// Caps (§4.5)
// =============================================================================

#[test]
fn cap_sits_on_the_old_bond_vector_at_the_terminator_length() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    let plan = plan_proxy(s, &[source], &default_options(3)).expect("plan");
    assert!(!plan.caps.is_empty());

    for cap in &plan.caps {
        let host = s.get_atom(cap.host).expect("host").position;
        let severed = s.get_atom(cap.severed).expect("severed").position;
        let expected = host + (severed - host).normalize() * SI_H;
        assert!(
            cap.position.distance(expected) < 1e-9,
            "cap {cap:?} off the bond vector"
        );
        assert!((cap.position.distance(host) - SI_H).abs() < 1e-9);
    }
}

#[test]
fn fluorine_caps_use_the_halogen_length() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    let options = ProxyOptions {
        hops: 3,
        passivant_element: 9,
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(s, &[source], &options).expect("plan");
    let si_f = atomcad_crystolecule::atomic_constants::halogen_bond_length(14, 9);
    assert!((si_f - 1.60).abs() < 1e-12);
    for cap in &plan.caps {
        let host = s.get_atom(cap.host).expect("host").position;
        assert!((cap.position.distance(host) - si_f).abs() < 1e-9);
    }
}

#[test]
fn caps_are_placed_on_severed_bonds_only() {
    // A saturated focus carbon (four bonds) and an unsaturated one (three).
    let mut s = AtomicStructure::new();
    let center = add(&mut s, 14, DVec3::ZERO);
    let arms = [
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(-1.0, -1.0, 1.0),
        DVec3::new(-1.0, 1.0, -1.0),
        DVec3::new(1.0, -1.0, -1.0),
    ];
    let mut neighbors = Vec::new();
    for dir in arms {
        let n = add(&mut s, 14, dir.normalize() * SI_SI);
        bond(&mut s, center, n);
        // Give each neighbour a second heavy bond so it is not a rider.
        let far = add(&mut s, 14, dir.normalize() * (2.0 * SI_SI));
        bond(&mut s, n, far);
        let farther = add(&mut s, 14, dir.normalize() * (3.0 * SI_SI));
        bond(&mut s, far, farther);
        neighbors.push(n);
    }

    // Saturated source: `hops = 0` severs all four bonds, so four caps.
    let plan = plan_proxy(
        &s,
        &[center],
        &ProxyOptions {
            hops: 0,
            fill: false,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    assert_eq!(plan.caps.len(), 4);
    assert!(plan.caps.iter().all(|c| c.host == center));

    // An unsaturated source: delete one arm entirely. The missing bond is not
    // a severed bond, so it receives no cap — the radical survives the cut.
    let mut s2 = AtomicStructure::new();
    let c2 = add(&mut s2, 14, DVec3::ZERO);
    for dir in &arms[..3] {
        let n = add(&mut s2, 14, dir.normalize() * SI_SI);
        bond(&mut s2, c2, n);
        let far = add(&mut s2, 14, dir.normalize() * (2.0 * SI_SI));
        bond(&mut s2, n, far);
        let farther = add(&mut s2, 14, dir.normalize() * (3.0 * SI_SI));
        bond(&mut s2, far, farther);
    }
    let plan2 = plan_proxy(
        &s2,
        &[c2],
        &ProxyOptions {
            hops: 0,
            fill: false,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    assert_eq!(plan2.caps.len(), 3, "no cap for the never-existing bond");
}

#[test]
fn passivate_off_plans_no_caps() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    let options = ProxyOptions {
        hops: 3,
        passivate: false,
        rm_single: false,
        ..Default::default()
    };
    assert!(
        plan_proxy(s, &[source], &options)
            .expect("plan")
            .caps
            .is_empty()
    );
}

#[test]
fn severed_bond_caps_is_driven_by_the_keep_predicate() {
    let mut s = AtomicStructure::new();
    let chain = capped_chain(&mut s, DVec3::ZERO, 4);
    let riders = classify_riders(&s);
    let keep = [chain[0], chain[1]];
    let caps = severed_bond_caps(&s, &|id| keep.contains(&id), &riders, 1);
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].host, chain[1]);
    assert_eq!(caps[0].severed, chain[2]);
}

// =============================================================================
// rm_single (§4.4)
// =============================================================================

#[test]
fn rm_single_cascades_along_a_chain_and_takes_the_riders() {
    // A six-ring with a four-atom chain hanging off it. `hops = 3` drops the
    // chain's last atom by distance; the rest unwinds one round at a time,
    // which is why a single pass would not do (§4.4).
    let mut s = AtomicStructure::new();
    let ring = six_ring(&mut s, DVec3::ZERO);
    let mut chain = Vec::new();
    let mut riders_of_chain = Vec::new();
    let mut previous = ring[0];
    for i in 1..=4 {
        let atom = add(&mut s, 6, DVec3::new(1.4 + 1.54 * i as f64, 0.0, 0.0));
        bond(&mut s, previous, atom);
        let h = add(&mut s, 1, DVec3::new(1.4 + 1.54 * i as f64, 1.09, 0.0));
        bond(&mut s, atom, h);
        chain.push(atom);
        riders_of_chain.push(h);
        previous = atom;
    }

    // `rim: hops` freezes the whole cut, which is what makes every atom
    // eligible for the trim: it only ever drops atoms in the frozen rim
    // (§4.4). The interior-protection rule has its own test below.
    let options = ProxyOptions {
        hops: 3,
        rim: 3,
        fill: false,
        rm_single: true,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[ring[0]], &options).expect("plan");

    // The whole ring survives: every ring atom keeps two kept heavy
    // neighbours (the ring's diameter is 3 hops).
    for id in &ring {
        assert!(plan.kept.contains(id), "ring atom {id} should survive");
    }
    // The chain unwinds completely, and every rider goes with its host.
    for id in chain.iter().chain(riders_of_chain.iter()) {
        assert!(!plan.kept.contains(id), "atom {id} should be dropped");
        assert!(plan.dropped.contains(id));
    }

    // Without the flag the same cut keeps the first three chain atoms.
    let kept_without = plan_proxy(
        &s,
        &[ring[0]],
        &ProxyOptions {
            rm_single: false,
            ..options.clone()
        },
    )
    .expect("plan");
    for id in &chain[..3] {
        assert!(kept_without.kept.contains(id));
    }
}

#[test]
fn rm_single_leaves_input_singly_bonded_atoms_and_rider_hosts_alone() {
    let mut s = AtomicStructure::new();
    let ring = six_ring(&mut s, DVec3::ZERO);
    // A heavy adatom, singly bonded in the input: a rider, so it is never
    // counted and never dropped on its own account.
    let adatom = add(&mut s, 14, DVec3::new(1.4, 0.0, 2.35));
    bond(&mut s, ring[0], adatom);

    let options = ProxyOptions {
        hops: 6,
        rim: 0,
        fill: false,
        rm_single: true,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[ring[3]], &options).expect("plan");
    // The ring survives (every atom has two kept neighbours) and so does the
    // rider, because its host survives.
    for id in ring.iter().chain(std::iter::once(&adatom)) {
        assert!(plan.kept.contains(id), "atom {id} should survive");
    }
}

#[test]
fn rm_single_runs_after_fill() {
    // `bridge` has one kept heavy neighbour on the plain cut but two once fill
    // has restored its partner, so the order of the two steps is observable.
    let mut s = AtomicStructure::new();
    let ring = six_ring(&mut s, DVec3::ZERO);
    let a = add(&mut s, 6, DVec3::new(1.4 + 1.54, 0.0, 0.0));
    let b = add(&mut s, 6, DVec3::new(1.4 + 1.54, 1.54, 0.0));
    bond(&mut s, ring[0], a);
    bond(&mut s, ring[1], b);
    let bridge = add(&mut s, 6, DVec3::new(1.4 + 3.0, 0.8, 0.0));
    bond(&mut s, a, bridge);
    bond(&mut s, b, bridge);

    // `rim: hops`, so the whole cut is rim and the trim is unguarded here.
    let base = ProxyOptions {
        hops: 1,
        rim: 1,
        rm_single: true,
        ..Default::default()
    };
    // Sources: two ring atoms, so `a` and `b` are at distance 1 and kept,
    // while `bridge` is at distance 2 and only fill can bring it back.
    let no_fill = plan_proxy(
        &s,
        &[ring[0], ring[1]],
        &ProxyOptions {
            fill: false,
            ..base.clone()
        },
    )
    .expect("plan");
    assert!(!no_fill.kept.contains(&bridge));
    // Without fill, `a` and `b` are singly attached and the cascade takes them.
    assert!(!no_fill.kept.contains(&a));
    assert!(!no_fill.kept.contains(&b));

    let with_fill = plan_proxy(&s, &[ring[0], ring[1]], &base).expect("plan");
    assert!(with_fill.kept.contains(&bridge));
    assert!(with_fill.kept.contains(&a));
    assert!(with_fill.kept.contains(&b));
}

#[test]
fn rm_single_never_drops_a_source() {
    // A lone focus atom on a chain hanging off a ring, with `hops = 1` so the
    // cascade eats everything around it. The source must survive anyway.
    let mut s = AtomicStructure::new();
    let ring = six_ring(&mut s, DVec3::ZERO);
    let focus = add(&mut s, 6, DVec3::new(1.4 + 1.54, 0.0, 0.0));
    bond(&mut s, ring[0], focus);
    let mut previous = focus;
    let mut tail = Vec::new();
    for i in 2..=4 {
        let atom = add(&mut s, 6, DVec3::new(1.4 + 1.54 * i as f64, 0.0, 0.0));
        bond(&mut s, previous, atom);
        tail.push(atom);
        previous = atom;
    }

    // `rim: hops`, so nothing is protected by the rim rule and the source
    // exemption is the only thing standing between the cascade and the atom.
    let options = ProxyOptions {
        hops: 1,
        rim: 1,
        fill: false,
        rm_single: true,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[focus], &options).expect("plan");
    assert_eq!(plan.kept, vec![focus], "a source is never dropped");
}

/// The cascade reaches a fixpoint with nothing left singly attached — and it
/// does so *within the rim*, at the default `rim: 1`. The interior guard of
/// §4.4 therefore costs nothing on a bulk cut: everything the trim wants to
/// remove is in the outermost shell anyway. (A dangling atom the guard does
/// leave is only reachable on a chain-like topology, which is the case the
/// guard exists for.)
/// §4.4's interior rule: the trim only ever drops atoms the cut **freezes**,
/// so the cascade cannot walk a chain into the relaxed interior, and the atoms
/// near the focus are safe whatever the topology.
///
/// Same fixture as the cascade test — a six-ring with a four-atom chain — cut
/// at `hops: 3`. With the whole cut frozen (`rim: 3`) the chain unwinds
/// completely; with `rim: 1` only the chain atom in the outermost shell may
/// go, and at `rim: 0` nothing may.
#[test]
fn rm_single_never_reaches_the_relaxed_interior() {
    let mut s = AtomicStructure::new();
    let ring = six_ring(&mut s, DVec3::ZERO);
    let mut chain = Vec::new();
    let mut previous = ring[0];
    for i in 1..=4 {
        let atom = add(&mut s, 6, DVec3::new(1.4 + 1.54 * i as f64, 0.0, 0.0));
        bond(&mut s, previous, atom);
        let h = add(&mut s, 1, DVec3::new(1.4 + 1.54 * i as f64, 1.09, 0.0));
        bond(&mut s, atom, h);
        chain.push(atom);
        previous = atom;
    }
    // `chain[k]` is at distance k + 1 from the source; `chain[3]` is beyond
    // `hops` and is what the distance cut drops, starting the cascade.
    let plan_of = |rim: u32| {
        plan_proxy(
            &s,
            &[ring[0]],
            &ProxyOptions {
                hops: 3,
                rim,
                fill: false,
                rm_single: true,
                ..Default::default()
            },
        )
        .expect("plan")
    };

    // rim 0: the free depth is `hops`, so no kept atom is eligible at all and
    // the chain keeps everything the distance cut kept.
    let none = plan_of(0);
    for id in &chain[..3] {
        assert!(none.kept.contains(id), "rim 0 must trim nothing: {id}");
    }

    // rim 1: the free depth is 2, so only `chain[2]` (distance 3) may go. The
    // cascade would take `chain[1]` next — the rule is what stops it.
    let one = plan_of(1);
    assert!(
        !one.kept.contains(&chain[2]),
        "the outermost chain atom goes"
    );
    assert!(
        one.kept.contains(&chain[1]),
        "distance 2 is inside the free depth and must survive the cascade"
    );
    assert!(one.kept.contains(&chain[0]));

    // rim 3: the whole cut is rim, so the cascade runs to the source.
    let all = plan_of(3);
    for id in &chain {
        assert!(!all.kept.contains(id), "rim 3 unwinds the chain: {id}");
    }
    assert!(
        all.kept.contains(&ring[0]),
        "the source survives regardless"
    );

    // And the general statement, on the bulk cube: nothing the trim dropped
    // was inside the free depth.
    let cube = silicon_cube();
    let source = cube_center_source(cube);
    // `fill: false` on purpose: the filled boundary is {111}-faceted and leaves
    // the trim nothing to do, so the plain cut is what exercises the rule here.
    let options = ProxyOptions {
        hops: 5,
        rim: 2,
        fill: false,
        rm_single: true,
        ..Default::default()
    };
    let trimmed = plan_proxy(cube, &[source], &options).expect("plan");
    let untrimmed = plan_proxy(
        cube,
        &[source],
        &ProxyOptions {
            rm_single: false,
            ..options.clone()
        },
    )
    .expect("plan");
    let kept_now: std::collections::HashSet<u32> = trimmed.kept.iter().copied().collect();
    let mut dropped_by_trim = 0usize;
    for id in &untrimmed.kept {
        if kept_now.contains(id) {
            continue;
        }
        dropped_by_trim += 1;
        let d = untrimmed.distance.get(id).copied().unwrap_or(u32::MAX);
        assert!(
            d > options.free_hops(),
            "atom {id} at distance {d} is inside the free depth {}",
            options.free_hops()
        );
    }
    assert!(dropped_by_trim > 0, "CONTROL: the trim did drop something");
}

#[test]
fn rm_single_reaches_a_fixpoint_on_the_bulk_cube() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    let options = ProxyOptions {
        hops: 4,
        rm_single: true,
        ..Default::default()
    };
    let plan = plan_proxy(s, &[source], &options).expect("plan");
    let riders = classify_riders(s);
    let heavy = kept_heavy(s, &plan);
    let kept: std::collections::HashSet<u32> = heavy.iter().copied().collect();
    for &a in &heavy {
        if a == source {
            continue;
        }
        let atom = s.get_atom(a).expect("atom");
        let input_heavy: Vec<u32> = atom
            .bonds
            .iter()
            .map(|b| b.other_atom_id())
            .filter(|n| !riders.contains_key(n))
            .collect();
        if input_heavy.len() <= 1 {
            continue;
        }
        let kept_count = input_heavy.iter().filter(|n| kept.contains(n)).count();
        assert_ne!(kept_count, 1, "atom {a} left singly attached");
    }
}

// =============================================================================
// Monotonicity (§4.4, §4.3)
// =============================================================================

#[test]
fn keep_sets_are_monotonic_in_hops() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    for fill in [false, true] {
        for k in 3..7u32 {
            let smaller = plan_proxy(
                s,
                &[source],
                &ProxyOptions {
                    hops: k,
                    fill,
                    rm_single: false,
                    ..Default::default()
                },
            )
            .expect("plan");
            let larger = plan_proxy(
                s,
                &[source],
                &ProxyOptions {
                    hops: k + 1,
                    fill,
                    rm_single: false,
                    ..Default::default()
                },
            )
            .expect("plan");
            let big: std::collections::HashSet<u32> = larger.kept.iter().copied().collect();
            for id in &smaller.kept {
                assert!(big.contains(id), "fill={fill} k={k}: atom {id} lost");
            }
        }
    }
}

#[test]
fn frozen_sets_grow_as_rim_grows() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    for r in 0..5u32 {
        let thinner = plan_proxy(
            s,
            &[source],
            &ProxyOptions {
                hops: 5,
                rim: r,
                rm_single: false,
                ..Default::default()
            },
        )
        .expect("plan");
        let thicker = plan_proxy(
            s,
            &[source],
            &ProxyOptions {
                hops: 5,
                rim: r + 1,
                rm_single: false,
                ..Default::default()
            },
        )
        .expect("plan");
        let small: std::collections::HashSet<u32> = thinner.frozen.iter().copied().collect();
        for id in &small {
            assert!(thicker.frozen.contains(id), "rim={r}: atom {id} unfrozen");
        }
        assert!(thinner.frozen.len() <= thicker.frozen.len());
    }
}

// =============================================================================
// Frozen (§4.6) and high (§4.7)
// =============================================================================

#[test]
fn frozen_is_exactly_distance_beyond_the_free_depth() {
    let s = silicon_cube();
    let source = cube_center_source(s);

    // hops 6, rim 3 — the free depth is 3.
    let plan = plan_proxy(
        s,
        &[source],
        &ProxyOptions {
            hops: 6,
            rim: 3,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    for id in &plan.frozen {
        assert!(plan.distance[id] > 3);
    }
    for id in kept_heavy(s, &plan) {
        if plan.distance[&id] > 3 {
            assert!(plan.frozen.contains(&id), "atom {id} should be frozen");
        }
    }
    // Everything fill restored lies beyond `hops` and so beyond the free depth.
    for id in &plan.filled {
        assert!(plan.frozen.contains(id), "filled atom {id} must be frozen");
    }

    // `rim: 0` freezes nothing on the plain cut.
    let plain = plan_proxy(
        s,
        &[source],
        &ProxyOptions {
            hops: 4,
            rim: 0,
            fill: false,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    assert!(plain.frozen.is_empty());
}

#[test]
fn core_tags_the_first_shells_and_nothing_else() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    let riders = classify_riders(s);

    let none = plan_proxy(s, &[source], &default_options(4)).expect("plan");
    assert!(none.high.is_empty());

    let zero = plan_proxy(
        s,
        &[source],
        &ProxyOptions {
            hops: 4,
            core: Some(0),
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    assert_eq!(zero.high, vec![source]);

    let one = plan_proxy(
        s,
        &[source],
        &ProxyOptions {
            hops: 4,
            core: Some(1),
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    assert_eq!(one.high.len(), 5, "the source and its four neighbours");
    for id in &one.high {
        assert!(one.distance[id] <= 1);
        // Riders follow at apply time, never in the plan's list.
        assert!(!riders.contains_key(id));
    }
}

// =============================================================================
// nearest_dropped (§7.3 step 11)
// =============================================================================

#[test]
fn nearest_dropped_finds_a_cut_away_steric_neighbour() {
    let mut s = AtomicStructure::new();
    let ring = six_ring(&mut s, DVec3::ZERO);
    // A "wall" fragment 3.0 Å above ring[0], bonded to nothing in the ring.
    let above = s.get_atom(ring[0]).expect("ring atom").position + DVec3::new(0.0, 0.0, 3.0);
    let w0 = add(&mut s, 6, above);
    let w1 = add(&mut s, 6, above + DVec3::new(0.0, 0.0, 1.5));
    let w2 = add(&mut s, 6, above + DVec3::new(1.3, 0.0, 0.75));
    bond(&mut s, w0, w1);
    bond(&mut s, w1, w2);
    bond(&mut s, w2, w0);

    let options = ProxyOptions {
        hops: 6,
        rim: 0,
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[ring[0]], &options).expect("plan");
    let d = plan.nearest_dropped.expect("a dropped neighbour");
    assert!((d - 3.0).abs() < 1e-9, "nearest_dropped = {d}");

    // It is the *unbonded* wall that qualifies: bond it to the ring and the
    // same atoms become the structure continuing, which the stat ignores.
    let mut attached = s.clone();
    bond(&mut attached, ring[0], w0);
    let plan = plan_proxy(&attached, &[ring[0]], &options).expect("plan");
    assert_eq!(
        plan.nearest_dropped, None,
        "a fragment the cluster is bonded to is not a steric neighbour"
    );
}

#[test]
fn nearest_dropped_is_none_when_nothing_is_dropped() {
    let mut s = AtomicStructure::new();
    let ring = six_ring(&mut s, DVec3::ZERO);
    let plan = plan_proxy(
        &s,
        &[ring[0]],
        &ProxyOptions {
            hops: 6,
            rim: 0,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    assert_eq!(plan.dropped, Vec::<u32>::new());
    assert_eq!(plan.nearest_dropped, None);
}

/// A cut into the bulk has **no** steric neighbour, at any setting: everything
/// it dropped is the same lattice continuing past the boundary, and a cap
/// already stands on the first severed bond.
///
/// The regression this pins is the panel's advisory firing on a textbook cut.
/// A free atom sits within `hops - rim` and a dropped one starts past `hops`,
/// so at the node's shipped defaults the two are two bonds apart — 3.84 Å on
/// silicon, under the ~4 Å line §3.3 reads the stat against. Counting the
/// workpiece made "an unbonded neighbour this close was cut away" the *normal*
/// state of the report.
#[test]
fn nearest_dropped_ignores_the_workpiece_continuing_past_the_cut() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    for (hops, rim) in [(3u32, 1u32), (3, 0), (4, 1), (6, 3), (6, 0)] {
        let plan = plan_proxy(
            s,
            &[source],
            &ProxyOptions {
                hops,
                rim,
                ..Default::default()
            },
        )
        .expect("plan");
        assert!(
            plan.nearest_dropped.is_none_or(|d| d > 4.0),
            "hops {hops} rim {rim}: nearest_dropped = {:?}, which would put a \
             steric warning on an ordinary bulk cut",
            plan.nearest_dropped
        );
    }

    // At the shipped defaults the answer is a real distance to a real detached
    // atom — 6.65 Å, comfortably clear of the ~4 Å line the panel warns at, where
    // counting the bonded continuation gave 3.84 Å and a permanent warning.
    let defaults = plan_proxy(s, &[source], &ProxyOptions::default()).expect("plan");
    let d = defaults
        .nearest_dropped
        .expect("the bulk has detached atoms too");
    assert!((d - 6.65).abs() < 0.01, "nearest_dropped = {d}");

    // The atoms excluded are exactly the ones the cluster is attached to: with
    // every one of them counted, the figure is the 3.84 Å of two Si-Si bonds.
    let plan = plan_proxy(s, &[source], &ProxyOptions::default()).expect("plan");
    let riders = classify_riders(s);
    let kept: std::collections::HashSet<u32> = plan.kept.iter().copied().collect();
    let free: Vec<u32> = plan
        .kept
        .iter()
        .copied()
        .filter(|id| !plan.frozen.contains(id))
        .collect();
    let mut naive: Option<f64> = None;
    for &d in &plan.dropped {
        if riders.contains_key(&d) {
            continue;
        }
        let dp = s.get_atom(d).expect("dropped atom").position;
        for &f in &free {
            let fp = s.get_atom(f).expect("free atom").position;
            let dist = dp.distance(fp);
            if naive.is_none_or(|best| dist < best) {
                naive = Some(dist);
            }
        }
    }
    let naive = naive.expect("the bulk drops plenty");
    assert!(
        (naive - 3.84).abs() < 0.05,
        "the excluded atoms are the bonded continuation: {naive}"
    );
    assert!(kept.contains(&source));
}

// =============================================================================
// Errors and determinism
// =============================================================================

#[test]
fn no_sources_is_an_error() {
    let mut s = AtomicStructure::new();
    six_ring(&mut s, DVec3::ZERO);
    let err = plan_proxy(&s, &[], &ProxyOptions::default()).unwrap_err();
    assert!(matches!(err, ProxyError::NoFocusAtoms));
}

#[test]
fn a_disallowed_passivant_is_an_error() {
    let mut s = AtomicStructure::new();
    let ring = six_ring(&mut s, DVec3::ZERO);
    let options = ProxyOptions {
        passivant_element: 2,
        rm_single: false,
        ..Default::default()
    };
    let err = plan_proxy(&s, &[ring[0]], &options).unwrap_err();
    assert!(matches!(err, ProxyError::BadPassivant(2)), "{err:?}");
}

#[test]
fn plans_are_deterministic_and_sorted() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    let options = ProxyOptions {
        hops: 5,
        core: Some(2),
        rm_single: true,
        ..Default::default()
    };
    let a = plan_proxy(s, &[source], &options).expect("plan");
    let b = plan_proxy(s, &[source], &options).expect("plan");
    assert_eq!(a, b);

    let sorted = |v: &[u32]| v.windows(2).all(|w| w[0] < w[1]);
    assert!(sorted(&a.kept));
    assert!(sorted(&a.dropped));
    assert!(sorted(&a.filled));
    assert!(sorted(&a.frozen));
    assert!(sorted(&a.high));
    assert!(
        a.caps
            .windows(2)
            .all(|w| (w[0].host, w[0].severed) < (w[1].host, w[1].severed))
    );
}

// =============================================================================
// terminator_bond_length (§7.5)
// =============================================================================

#[test]
fn terminator_bond_length_matches_the_general_passivation_path() {
    use atomcad_crystolecule::atomic_constants::halogen_bond_length;

    assert!((terminator_bond_length(14, 1) - 1.48).abs() < 1e-12);
    assert!((terminator_bond_length(6, 1) - 1.09).abs() < 1e-12);
    for passivant in [9i16, 17, 35, 53] {
        for host in [6i16, 14, 32, 7] {
            assert_eq!(
                terminator_bond_length(host, passivant),
                halogen_bond_length(host, passivant),
                "host {host}, passivant {passivant}"
            );
        }
    }
}

// =============================================================================
// Apply (§7.4)
// =============================================================================

/// The bonds of a structure counted by walking the atoms — the cross-check on
/// `get_num_of_bonds` after the deletions and the caps.
fn walked_bond_count(structure: &AtomicStructure) -> usize {
    let ends: usize = structure
        .atoms_values()
        .map(|a| a.bonds.iter().filter(|b| !b.is_delete_marker()).count())
        .sum();
    assert_eq!(ends % 2, 0, "every bond is stored on both of its atoms");
    ends / 2
}

/// Everything about a structure that `apply_proxy` could possibly disturb —
/// used to assert that a failed apply left the input untouched.
fn fingerprint(structure: &AtomicStructure) -> String {
    let mut atoms: Vec<String> = structure
        .atoms_values()
        .map(|a| {
            let mut neighbors: Vec<u32> = a
                .bonds
                .iter()
                .filter(|b| !b.is_delete_marker())
                .map(|b| b.other_atom_id())
                .collect();
            neighbors.sort_unstable();
            format!(
                "{}:{}:{:?}:{}:{}:{:?}",
                a.id, a.atomic_number, a.position, a.flags, a.tag_bits, neighbors
            )
        })
        .collect();
    atoms.sort();
    format!("{:?}|{}", structure.tag_names(), atoms.join(";"))
}

/// The atom ids carrying a tag, in id order.
fn tagged(structure: &AtomicStructure, name: &str) -> Vec<u32> {
    sorted_ids(structure.atoms_with_tag(name))
}

#[test]
fn apply_on_the_bulk_cube_matches_its_plan() {
    let mut s = silicon_cube().clone();
    let source = cube_center_source(&s);
    let plan = plan_proxy(&s, &[source], &default_options(4)).expect("plan");
    let dropped: HashSet<u32> = plan.dropped.iter().copied().collect();
    let planned_caps = plan.caps.clone();
    let stats = apply_proxy(&mut s, &plan).expect("apply");

    // The output is exactly the kept atoms plus the caps.
    assert_eq!(stats.heavy, 165, "the filled four-hop cut of §4.3");
    assert_eq!(stats.riders, 0, "the cut is deep inside the cube");
    assert_eq!(stats.caps, planned_caps.len());
    assert_eq!(
        s.atoms_values().count(),
        stats.heavy + stats.riders + stats.caps
    );

    // No atom kept a bond to a dropped id, and the bond counter is in step
    // with the atoms.
    for atom in s.atoms_values() {
        for bond in atom.bonds.iter().filter(|b| !b.is_delete_marker()) {
            let other = bond.other_atom_id();
            assert!(!dropped.contains(&other), "bond to dropped atom {other}");
            assert!(s.get_atom(other).is_some(), "bond to missing atom {other}");
        }
    }
    assert_eq!(s.get_num_of_bonds(), walked_bond_count(&s));

    // Every planned cap is there: one terminator on its host, single bond, at
    // the planned position, flagged as passivation.
    for cap in &planned_caps {
        let host = s.get_atom(cap.host).expect("the host survived the cut");
        let placed: Vec<u32> = host
            .bonds
            .iter()
            .filter(|b| !b.is_delete_marker())
            .map(|b| b.other_atom_id())
            .filter(|&n| {
                s.get_atom(n)
                    .is_some_and(|a| a.position.distance(cap.position) < 1e-9)
            })
            .collect();
        assert_eq!(placed.len(), 1, "exactly one cap at the planned position");
        let cap_atom = s.get_atom(placed[0]).expect("the cap");
        assert_eq!(cap_atom.atomic_number, 1);
        assert!(cap_atom.is_hydrogen_passivation(), "cap carries the flag");
        assert_eq!(
            cap_atom
                .bonds
                .iter()
                .filter(|b| !b.is_delete_marker())
                .count(),
            1,
            "a cap is bonded once"
        );
        let order = host
            .bonds
            .iter()
            .find(|b| b.other_atom_id() == placed[0])
            .expect("the host's bond to its cap")
            .bond_order();
        assert_eq!(order, BOND_SINGLE);
    }

    // Nothing is left unsaturated: the rim is fully capped.
    assert_eq!(stats.open_valences, 0);
}

#[test]
fn apply_is_deterministic() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    let plan = plan_proxy(s, &[source], &default_options(3)).expect("plan");

    let mut first = s.clone();
    let stats_a = apply_proxy(&mut first, &plan).expect("apply");
    let mut second = s.clone();
    let stats_b = apply_proxy(&mut second, &plan).expect("apply");

    assert_eq!(stats_a, stats_b);
    assert_eq!(fingerprint(&first), fingerprint(&second));
}

// =============================================================================
// Frozen at apply time (§4.6)
// =============================================================================

/// A five-carbon chain with a hydrogen at each end, a heavy adatom rider on
/// `c1`, and `c0` frozen in the input.
fn frozen_chain_fixture() -> (AtomicStructure, Vec<u32>, u32, u32) {
    let mut s = AtomicStructure::new();
    let chain = capped_chain(&mut s, DVec3::ZERO, 5);
    let head_h = s
        .atoms_values()
        .find(|a| a.atomic_number == 1)
        .expect("the head cap")
        .id;
    let adatom = add(&mut s, 14, DVec3::new(1.54, 1.9, 0.0));
    bond(&mut s, chain[1], adatom);
    s.set_atom_frozen(chain[0], true);
    (s, chain, head_h, adatom)
}

#[test]
fn frozen_flags_are_only_ever_set() {
    let (mut s, chain, head_h, adatom) = frozen_chain_fixture();
    let options = ProxyOptions {
        hops: 2,
        rim: 1,
        fill: false,
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[chain[0]], &options).expect("plan");
    // The distance rule alone would freeze only `c2`.
    assert_eq!(plan.frozen, vec![chain[2]]);
    let cap_host = plan.caps[0].host;
    assert_eq!(cap_host, chain[2], "the one severed bond is c2–c3");
    let stats = apply_proxy(&mut s, &plan).expect("apply");

    let is_frozen = |id: u32| s.get_atom(id).expect("kept").is_frozen();
    // Frozen in the input at distance 0 — the flag is never cleared.
    assert!(is_frozen(chain[0]));
    // Frozen by the distance rule, and its cap follows.
    assert!(is_frozen(chain[2]));
    let cap = s
        .atoms_values()
        .find(|a| a.atomic_number == 1 && a.is_hydrogen_passivation())
        .expect("the cap");
    assert!(cap.is_frozen(), "a cap follows its host");
    // A rider follows its host either way: `head_h` hangs off the atom that
    // was frozen in the input, the adatom off a free one.
    assert!(is_frozen(head_h), "rider of a host frozen in the input");
    assert!(!is_frozen(chain[1]), "c1 is within `free`");
    assert!(!is_frozen(adatom), "a free rider of a free host stays free");

    assert_eq!(stats.free + stats.frozen, s.atoms_values().count());
    assert_eq!(stats.frozen, 4, "c0, its H, c2 and c2's cap");
    assert_eq!(stats.free, 2, "c1 and its adatom");
}

// =============================================================================
// `in_crystal_depth` (§4.8)
// =============================================================================

#[test]
fn the_cut_clears_in_crystal_depth_on_every_atom() {
    let (mut s, chain, head_h, adatom) = frozen_chain_fixture();
    // The depths a cut through the bulk would arrive with.
    for (i, &id) in chain.iter().enumerate() {
        s.set_atom_in_crystal_depth(id, 3.0 * (i as f32 + 1.0));
    }
    s.set_atom_in_crystal_depth(head_h, 3.0);
    s.set_atom_in_crystal_depth(adatom, 6.0);
    let options = ProxyOptions {
        hops: 2,
        rim: 1,
        fill: false,
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[chain[0]], &options).expect("plan");
    assert_eq!(plan.caps[0].host, chain[2], "the one severed bond is c2–c3");
    apply_proxy(&mut s, &plan).expect("apply");

    // Kept heavy atoms, their riders and the cap alike: the proxy is a
    // free-standing cluster, so nothing in it is buried any more.
    for atom in s.atoms_values() {
        assert_eq!(
            atom.in_crystal_depth, 0.0,
            "atom {} left at depth {}",
            atom.id, atom.in_crystal_depth
        );
    }
}

#[test]
fn the_bulk_cut_leaves_nothing_behind_the_culling_threshold() {
    let cube = silicon_cube();
    let source = cube_center_source(cube);
    let options = ProxyOptions {
        hops: 4,
        rim: 1,
        fill: false,
        rm_single: true,
        ..Default::default()
    };
    // The cut is deep enough in the bulk that its atoms arrive well past the
    // 8 Å ball-and-stick culling threshold — this is the case that used to
    // render as free-floating hydrogens.
    let deepest_input = plan_proxy(cube, &[source], &options)
        .expect("plan")
        .kept
        .iter()
        .filter_map(|id| cube.get_atom(*id))
        .map(|a| a.in_crystal_depth)
        .fold(0.0f32, f32::max);
    assert!(
        deepest_input > 8.0,
        "fixture no longer exercises depth culling: deepest kept atom is {deepest_input} Å"
    );

    let plan = plan_proxy(cube, &[source], &options).expect("plan");
    let mut cut = cube.clone();
    apply_proxy(&mut cut, &plan).expect("apply");
    assert!(
        cut.atoms_values().all(|a| a.in_crystal_depth == 0.0),
        "every atom of the cut reads as surface"
    );
}

// =============================================================================
// The `high` tag (§4.7)
// =============================================================================

#[test]
fn high_is_inherited_by_riders_and_caps() {
    let (mut s, chain, head_h, adatom) = frozen_chain_fixture();
    let options = ProxyOptions {
        hops: 2,
        rim: 1,
        fill: false,
        core: Some(2),
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[chain[0]], &options).expect("plan");
    assert_eq!(plan.high, sorted_ids(vec![chain[0], chain[1], chain[2]]));
    apply_proxy(&mut s, &plan).expect("apply");

    let cap = s
        .atoms_values()
        .find(|a| a.atomic_number == 1 && a.is_hydrogen_passivation())
        .expect("the cap")
        .id;
    // The three heavy atoms, the two riders and the cap of a high host.
    assert_eq!(
        tagged(&s, "high"),
        sorted_ids(vec![chain[0], chain[1], chain[2], head_h, adatom, cap])
    );
}

#[test]
fn high_already_on_an_atom_beyond_core_is_kept() {
    let (mut s, chain, _head_h, _adatom) = frozen_chain_fixture();
    s.add_atom_tag(chain[2], "high").expect("tag");
    let options = ProxyOptions {
        hops: 2,
        rim: 1,
        fill: false,
        core: Some(0),
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[chain[0]], &options).expect("plan");
    assert_eq!(plan.high, vec![chain[0]], "core 0 is the sources only");
    apply_proxy(&mut s, &plan).expect("apply");

    // The tag is added, never removed: `c2` keeps the one it came with.
    assert!(s.atom_has_tag(chain[2], "high"));
    assert!(s.atom_has_tag(chain[0], "high"));
}

#[test]
fn core_none_leaves_the_tag_table_alone() {
    let (mut s, chain, _head_h, _adatom) = frozen_chain_fixture();
    let before = s.tag_names().to_vec();
    let options = ProxyOptions {
        hops: 2,
        rim: 1,
        fill: false,
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[chain[0]], &options).expect("plan");
    assert!(plan.high.is_empty());
    apply_proxy(&mut s, &plan).expect("apply");
    assert_eq!(s.tag_names(), before.as_slice());
    assert!(s.tag_names().is_empty());
}

#[test]
fn a_full_tag_table_fails_before_the_first_mutation() {
    let (mut s, chain, _head_h, _adatom) = frozen_chain_fixture();
    // 32 live names, none of them `high`.
    for i in 0..32 {
        s.add_atom_tag(chain[0], &format!("t{i}")).expect("tag");
    }
    let options = ProxyOptions {
        hops: 2,
        rim: 1,
        fill: false,
        core: Some(0),
        rm_single: false,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[chain[0]], &options).expect("plan");
    let before = fingerprint(&s);

    let err = apply_proxy(&mut s, &plan).expect_err("the tag table is full");
    assert!(matches!(err, ProxyError::Tag(_)), "{err}");
    assert_eq!(
        fingerprint(&s),
        before,
        "the intern-first rule leaves a failed cut's input untouched"
    );
}

// =============================================================================
// Radicals and proxies of proxies
// =============================================================================

/// A silicon with only three bonds — a radical apex — each neighbour saturated
/// with hydrogens so that none of them is a rider.
fn radical_apex() -> (AtomicStructure, u32) {
    let mut s = AtomicStructure::new();
    let apex = add(&mut s, 14, DVec3::ZERO);
    let dirs = [
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(-1.0, -1.0, 1.0),
        DVec3::new(-1.0, 1.0, -1.0),
    ];
    for dir in dirs {
        let unit = dir.normalize();
        let neighbor = add(&mut s, 14, unit * SI_SI);
        bond(&mut s, apex, neighbor);
        // Three hydrogens each, so the neighbour is saturated and heavy.
        for offset in [
            DVec3::new(1.0, 1.0, 1.0),
            DVec3::new(-1.0, -1.0, 1.0),
            DVec3::new(-1.0, 1.0, -1.0),
        ] {
            let h = add(&mut s, 1, unit * SI_SI + offset.normalize() * SI_H);
            bond(&mut s, neighbor, h);
        }
    }
    (s, apex)
}

#[test]
fn a_radical_focus_atom_is_never_capped() {
    let (mut s, apex) = radical_apex();
    let plan = plan_proxy(&s, &[apex], &default_options(1)).expect("plan");
    assert!(
        plan.caps.is_empty(),
        "nothing was severed, nothing is capped"
    );
    let stats = apply_proxy(&mut s, &plan).expect("apply");

    // The apex keeps its three bonds and its open valence.
    assert_eq!(
        s.get_atom(apex)
            .expect("the apex")
            .bonds
            .iter()
            .filter(|b| !b.is_delete_marker())
            .count(),
        3
    );
    assert_eq!(stats.caps, 0);
    assert_eq!(stats.open_valences, 1);

    // `passivate` on a clone would add exactly that many atoms — the shared
    // `open_valence_slots` is what both figures come from.
    let mut clone = s.clone();
    let added = add_hydrogens(&mut clone, &AddHydrogensOptions::default()).atoms_added;
    assert_eq!(added, stats.open_valences);
}

#[test]
fn a_proxy_of_a_proxy_equals_a_direct_cut() {
    let cube = silicon_cube();
    let source = cube_center_source(cube);

    // The six-hop proxy, then a four-hop cut out of it.
    let mut proxy = cube.clone();
    let outer = plan_proxy(&proxy, &[source], &default_options(6)).expect("plan");
    apply_proxy(&mut proxy, &outer).expect("apply");
    let nested = plan_proxy(&proxy, &[source], &default_options(4)).expect("plan");

    // The same four-hop cut taken straight from the workpiece.
    let direct = plan_proxy(cube, &[source], &default_options(4)).expect("plan");

    assert_eq!(
        kept_heavy(&proxy, &nested),
        kept_heavy(cube, &direct),
        "the same heavy atoms, with the same ids"
    );
    let nested_caps: Vec<DVec3> = nested.caps.iter().map(|c| c.position).collect();
    let direct_caps: Vec<DVec3> = direct.caps.iter().map(|c| c.position).collect();
    assert_eq!(nested_caps.len(), direct_caps.len());
    for (a, b) in nested_caps.iter().zip(&direct_caps) {
        assert!(a.distance(*b) < 1e-9, "cap moved: {a} vs {b}");
    }
}

// =============================================================================
// Stats (§3.3)
// =============================================================================

#[test]
fn stats_report_the_cut_on_the_bulk_cube() {
    let cube = silicon_cube();
    let source = cube_center_source(cube);

    let mut filled = cube.clone();
    let plan = plan_proxy(&filled, &[source], &default_options(4)).expect("plan");
    let planned_filled = plan.filled.len();
    let stats = apply_proxy(&mut filled, &plan).expect("apply");

    assert_eq!(stats.formula, format!("Si165H{}", stats.caps));
    assert_eq!(stats.filled, planned_filled);
    assert_eq!(stats.filled, 165 - 83);
    assert_eq!(stats.fill_rounds, 4);
    assert_eq!(stats.farthest_hop, 8, "a size figure, not a rim figure");
    assert_eq!(stats.free_hops, 3, "hops 4 − rim 1");
    let pair = stats.min_cap_pair.expect("the rim has cap pairs");
    assert!((pair - 2.42).abs() < 0.01, "clean silicon rim: {pair}");

    // Without fill the same cut keeps the clashing pairs of §4.3.
    let mut plain = cube.clone();
    let plain_plan = plan_proxy(
        &plain,
        &[source],
        &ProxyOptions {
            hops: 4,
            fill: false,
            rm_single: false,
            ..Default::default()
        },
    )
    .expect("plan");
    let plain_stats = apply_proxy(&mut plain, &plain_plan).expect("apply");
    assert_eq!(plain_stats.heavy, 83);
    assert_eq!(plain_stats.filled, 0);
    assert_eq!(plain_stats.fill_rounds, 0);
    let plain_pair = plain_stats.min_cap_pair.expect("cap pairs");
    assert!(
        (plain_pair - 1.42).abs() < 0.01,
        "the unphysical shared site: {plain_pair}"
    );
}

#[test]
fn min_cap_pair_is_none_when_no_two_caps_are_close() {
    // A seven-carbon chain cut one hop either side of the middle: the two caps
    // end up 5.26 Å apart, well beyond `CAP_PAIR_RADIUS`.
    let mut s = AtomicStructure::new();
    let chain = capped_chain(&mut s, DVec3::ZERO, 7);
    let plan = plan_proxy(&s, &[chain[3]], &default_options(1)).expect("plan");
    assert_eq!(plan.caps.len(), 2);
    let stats = apply_proxy(&mut s, &plan).expect("apply");
    assert_eq!(stats.caps, 2);
    assert_eq!(stats.min_cap_pair, None);
    assert_eq!(
        stats.nearest_dropped, None,
        "the rest of the chain is the structure continuing past the cut, not a \
         steric neighbour"
    );
}

// =============================================================================
// `proxy_cut` by tag name
// =============================================================================

#[test]
fn proxy_cut_resolves_the_focus_tag() {
    let (mut s, chain, _head_h, _adatom) = frozen_chain_fixture();
    s.add_atom_tag(chain[0], "focus").expect("tag");
    let options = ProxyOptions {
        hops: 2,
        rim: 1,
        fill: false,
        rm_single: false,
        ..Default::default()
    };

    // The same structure through the two halves by hand.
    let mut by_hand = s.clone();
    let plan = plan_proxy(&by_hand, &[chain[0]], &options).expect("plan");
    let expected = apply_proxy(&mut by_hand, &plan).expect("apply");

    let stats = proxy_cut(&mut s, "focus", &options).expect("cut");
    assert_eq!(stats, expected);
    assert_eq!(fingerprint(&s), fingerprint(&by_hand), "the same ids");
}

#[test]
fn proxy_cut_promotes_a_tagged_rider_to_its_host() {
    let (mut s, chain, head_h, _adatom) = frozen_chain_fixture();
    s.add_atom_tag(head_h, "focus").expect("tag");
    let options = ProxyOptions {
        hops: 1,
        fill: false,
        rm_single: false,
        ..Default::default()
    };
    let stats = proxy_cut(&mut s, "focus", &options).expect("cut");

    // The host anchored the cut, and the rider came along with it.
    assert!(s.get_atom(chain[0]).is_some());
    assert!(s.get_atom(chain[1]).is_some());
    assert!(s.get_atom(head_h).is_some());
    assert!(s.get_atom(chain[2]).is_none(), "two hops out is gone");
    assert_eq!(stats.heavy, 2);
}

#[test]
fn proxy_cut_without_the_tag_is_an_error() {
    let (mut s, _chain, _head_h, _adatom) = frozen_chain_fixture();
    let err = proxy_cut(&mut s, "focus", &ProxyOptions::default()).expect_err("no focus");
    assert!(matches!(err, ProxyError::NoFocusAtoms));
}

// =============================================================================
// `empirical_formula` (§7.5)
// =============================================================================

#[test]
fn empirical_formula_orders_by_count_with_hydrogen_last() {
    // Methane: a bare count of one, and H last although it is the majority.
    let mut methane = AtomicStructure::new();
    let c = add(&mut methane, 6, DVec3::ZERO);
    for pos in [
        DVec3::new(0.63, 0.63, 0.63),
        DVec3::new(-0.63, -0.63, 0.63),
        DVec3::new(-0.63, 0.63, -0.63),
        DVec3::new(0.63, -0.63, -0.63),
    ] {
        let h = add(&mut methane, 1, pos);
        bond(&mut methane, c, h);
    }
    assert_eq!(empirical_formula(&methane), "CH4");

    // Water. Hydrogen is last whatever its count, so this reads `OH2` —
    // the §7.5 rule, not Hill notation.
    let mut water = AtomicStructure::new();
    let o = add(&mut water, 8, DVec3::ZERO);
    for pos in [DVec3::new(0.76, 0.59, 0.0), DVec3::new(-0.76, 0.59, 0.0)] {
        let h = add(&mut water, 1, pos);
        bond(&mut water, o, h);
    }
    assert_eq!(empirical_formula(&water), "OH2");

    // Descending count, ties by symbol, hydrogen last.
    let mut mixed = AtomicStructure::new();
    for i in 0..3 {
        add(&mut mixed, 14, DVec3::new(i as f64, 0.0, 0.0));
    }
    for i in 0..2 {
        add(&mut mixed, 6, DVec3::new(i as f64, 2.0, 0.0));
    }
    add(&mut mixed, 8, DVec3::new(0.0, 4.0, 0.0));
    add(&mut mixed, 7, DVec3::new(1.0, 4.0, 0.0));
    for i in 0..5 {
        add(&mut mixed, 1, DVec3::new(i as f64, 6.0, 0.0));
    }
    assert_eq!(empirical_formula(&mixed), "Si3C2NOH5");
}

#[test]
fn empirical_formula_skips_markers_and_resolves_parameters() {
    let mut s = AtomicStructure::new();
    add(&mut s, 6, DVec3::ZERO);
    // A delete marker and an unchanged marker carry no element.
    add(&mut s, 0, DVec3::new(1.0, 0.0, 0.0));
    add(&mut s, -1, DVec3::new(2.0, 0.0, 0.0));
    // A parameter element resolves to what it stands for.
    add(&mut s, -100, DVec3::new(3.0, 0.0, 0.0));
    add(&mut s, -100, DVec3::new(4.0, 0.0, 0.0));
    let mut overrides = FxHashMap::default();
    overrides.insert(-100i16, 14i16);
    s.set_effective_atomic_numbers(overrides);

    assert_eq!(empirical_formula(&s), "Si2C");
}
