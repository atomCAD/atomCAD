//! Tests for `proxy_cut` — the bond-hop proxy cut (`doc/design_proxy_node.md`).
//!
//! Phase 1 covers the analysis half: riders, distances, the keep set (`fill`
//! and `rm_single`), the severed-bond caps, the frozen / high lists and
//! `nearest_dropped`. Nothing here mutates a structure.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use atomcad_crystolecule::hydrogen_passivation::terminator_bond_length;
use atomcad_crystolecule::lattice_fill::{LatticeFillConfig, LatticeFillOptions, fill_lattice};
use atomcad_crystolecule::motif::Motif;
use atomcad_crystolecule::proxy_cut::{
    CapPlacement, ProxyError, ProxyOptions, bond_distances, classify_riders, plan_proxy,
    severed_bond_caps,
};
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_geo_tree::GeoNode;
use atomcad_util::daabox::DAABox;
use glam::f64::DVec3;
use std::collections::HashMap;
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

fn default_options(hops: u32) -> ProxyOptions {
    ProxyOptions {
        hops,
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
        free: 6,
        fill: false,
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

    let options = ProxyOptions {
        hops: 3,
        free: 6,
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
        free: 6,
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

    let base = ProxyOptions {
        hops: 1,
        free: 6,
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

    let options = ProxyOptions {
        hops: 1,
        free: 6,
        fill: false,
        rm_single: true,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[focus], &options).expect("plan");
    assert_eq!(plan.kept, vec![focus], "a source is never dropped");
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
fn frozen_sets_shrink_as_free_grows() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    for f in 0..5u32 {
        let tighter = plan_proxy(
            s,
            &[source],
            &ProxyOptions {
                hops: 5,
                free: f,
                ..Default::default()
            },
        )
        .expect("plan");
        let looser = plan_proxy(
            s,
            &[source],
            &ProxyOptions {
                hops: 5,
                free: f + 1,
                ..Default::default()
            },
        )
        .expect("plan");
        let small: std::collections::HashSet<u32> = looser.frozen.iter().copied().collect();
        for id in &small {
            assert!(tighter.frozen.contains(id), "free={f}: atom {id} unfrozen");
        }
        assert!(looser.frozen.len() <= tighter.frozen.len());
    }
}

// =============================================================================
// Frozen (§4.6) and high (§4.7)
// =============================================================================

#[test]
fn frozen_is_exactly_distance_beyond_free() {
    let s = silicon_cube();
    let source = cube_center_source(s);

    let plan = plan_proxy(s, &[source], &default_options(6)).expect("plan");
    for id in &plan.frozen {
        assert!(plan.distance[id] > 3);
    }
    for id in kept_heavy(s, &plan) {
        if plan.distance[&id] > 3 {
            assert!(plan.frozen.contains(&id), "atom {id} should be frozen");
        }
    }
    // Everything fill restored lies beyond `hops` and so beyond `free`.
    for id in &plan.filled {
        assert!(plan.frozen.contains(id), "filled atom {id} must be frozen");
    }

    // `free >= hops` freezes nothing on the plain cut.
    let plain = plan_proxy(
        s,
        &[source],
        &ProxyOptions {
            hops: 4,
            free: 4,
            fill: false,
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
        free: 6,
        ..Default::default()
    };
    let plan = plan_proxy(&s, &[ring[0]], &options).expect("plan");
    let d = plan.nearest_dropped.expect("a dropped neighbour");
    assert!((d - 3.0).abs() < 1e-9, "nearest_dropped = {d}");
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
            free: 6,
            ..Default::default()
        },
    )
    .expect("plan");
    assert_eq!(plan.dropped, Vec::<u32>::new());
    assert_eq!(plan.nearest_dropped, None);
}

#[test]
fn nearest_dropped_on_the_bulk_cube_is_comfortably_far() {
    let s = silicon_cube();
    let source = cube_center_source(s);
    let plan = plan_proxy(s, &[source], &default_options(6)).expect("plan");
    let d = plan
        .nearest_dropped
        .expect("the bulk always drops something");
    assert!(d > 4.0, "nearest_dropped = {d}");
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
