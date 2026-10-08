//! Shared by the sequential chemisorption tests: the fixtures (the same ones
//! the old engine's tests and its golden data use), a seeded generator, the
//! independent oracle of §11.1 and the `audit` of §11.5.
//!
//! The oracle and the audit deliberately share **no loop structure** with the
//! engine. The oracle walks every ordered tuple of legs with plain nested
//! loops and checks each tuple's conditions as written in the design; only
//! the per-leg predicates (`pair_need`, `acceptor`, `mirror`, …) are the
//! engine's, since a second copy of a formula would carry the same slip.

#![allow(dead_code)]

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::atomic_structure_utils::{auto_create_bonds, remove_single_bond_atoms};
use atomcad_crystolecule::chemisorption::relax::relax;
use atomcad_crystolecule::chemisorption::sequential::{
    GEOMETRIC_LEGS, Leg, Mirror, SearchReport, SequentialPlan, SequentialSearch, Setup, Step,
};
use atomcad_crystolecule::chemisorption::{BondInventory, BondKind, HypothesisKey, change_key};
use atomcad_crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use atomcad_crystolecule::hydrogen_passivation::{AddHydrogensOptions, add_hydrogens};
use atomcad_crystolecule::lattice_fill::{LatticeFillConfig, LatticeFillOptions, fill_lattice};
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_geo_tree::GeoNode;
use atomcad_util::daabox::DAABox;
use glam::{DQuat, DVec3};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const H: i16 = 1;
pub const C: i16 = 6;
pub const O: i16 = 8;
pub const SI: i16 = 14;

pub const A_C: f64 = 3.567;
pub const A_SI: f64 = 5.431;

/// Feet height above the dimer layer, as in old §8.6 and the golden data.
pub const LIFT: f64 = 1.8;

// ============================================================================
// A seeded generator (no new dependency)
// ============================================================================

/// SplitMix64.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// A uniformly random unit vector.
    pub fn unit(&mut self) -> DVec3 {
        loop {
            let v = DVec3::new(
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            );
            let l = v.length();
            if l > 0.1 && l <= 1.0 {
                return v / l;
            }
        }
    }

    /// A uniformly random rotation.
    pub fn rotation(&mut self) -> DQuat {
        loop {
            let q = DQuat::from_xyzw(
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            );
            let l = q.length();
            if l > 0.1 && l <= 1.0 {
                return q.normalize();
            }
        }
    }
}

// ============================================================================
// Fixtures
// ============================================================================

/// Three bond directions of an sp3 atom whose open valence points along `up`.
pub fn tetrahedral_below(up: DVec3) -> [DVec3; 3] {
    let q = DQuat::from_rotation_arc(DVec3::Z, up.normalize());
    let (sin, cos) = ((8.0f64 / 9.0).sqrt(), -1.0 / 3.0);
    [0.0f64, 120.0, 240.0].map(|deg| {
        let phi = deg.to_radians();
        q * DVec3::new(sin * phi.cos(), sin * phi.sin(), cos)
    })
}

/// A silyl at `p` with `h_count` hydrogens, its dangling bonds along `up`:
/// 3 = SiH3 (one free valence), 2 = SiH2 (two), 4 = SiH4 (none).
pub fn add_silyl_along(s: &mut AtomicStructure, p: DVec3, up: DVec3, h_count: usize) -> u32 {
    let si = s.add_atom(SI, p);
    let mut dirs: Vec<DVec3> = tetrahedral_below(up).to_vec();
    dirs.push(up.normalize());
    for d in dirs.into_iter().take(h_count) {
        let h = s.add_atom(H, p + d * 1.48);
        s.add_bond(si, h, BOND_SINGLE);
    }
    si
}

pub fn add_silyl(s: &mut AtomicStructure, p: DVec3, h_count: usize) -> u32 {
    add_silyl_along(s, p, DVec3::Z, h_count)
}

/// A radical O foot at `p` hanging from a methyl above it: CH3–O•.
pub fn add_methoxy(s: &mut AtomicStructure, p: DVec3) -> u32 {
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
/// methoxy; `plan` never relaxes, so rigidity is irrelevant to it).
pub fn methoxy_feet(positions: &[DVec3]) -> (AtomicStructure, Vec<u32>) {
    let mut s = AtomicStructure::new();
    let ids = positions.iter().map(|&p| add_methoxy(&mut s, p)).collect();
    (s, ids)
}

/// A silyl site at each position, dangling bonds up.
pub fn silyl_sites(positions: &[DVec3], h_count: usize) -> (AtomicStructure, Vec<u32>) {
    let mut s = AtomicStructure::new();
    let ids = positions
        .iter()
        .map(|&p| add_silyl(&mut s, p, h_count))
        .collect();
    (s, ids)
}

/// The diamond lattice points of a sphere, bonded, one-bond atoms removed.
pub fn diamond_cluster(center: DVec3, radius: f64) -> AtomicStructure {
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
/// hexapod). The feet lie in the plane `z = 0`, around the z axis, the body
/// above them.
pub fn stand_in(legs: usize) -> (AtomicStructure, Vec<u32>) {
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
    assert_eq!(bottom.len(), 6, "the cage's (111) face has six C–H");
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

/// The stand-in rotated `rot_deg` about z, shifted, its feet `LIFT` above the
/// dimer layer: the poses of the golden data.
pub fn posed_stand_in(legs: usize, rot_deg: f64, shift: DVec3) -> (AtomicStructure, Vec<u32>) {
    let (mut ads, feet) = stand_in(legs);
    let q = DQuat::from_rotation_z(rot_deg.to_radians());
    ads.transform(&q, &(shift + DVec3::Z * LIFT));
    (ads, feet)
}

pub fn axis_aligned_box(min: DVec3, max: DVec3) -> GeoNode {
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
/// deep, its dimer layer at `z = 0` and its lateral centre on the z axis.
/// Atoms deeper than 2 Å or farther than `mobile_radius` from the axis are
/// frozen. Its bare side and bottom faces are sites too.
pub fn si100_slab(cells: f64, mobile_radius: f64) -> AtomicStructure {
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

/// The reconstructed dimer atoms of a slab within `radius` of its axis,
/// tagged `dimer`: the facet, without the fixture's bare edges and faces.
pub fn tag_dimers(slab: &mut AtomicStructure, radius: f64) {
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

/// The surface dimer nearest the axis.
pub fn central_dimer(slab: &AtomicStructure) -> (u32, u32) {
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

/// •CH2–CH2•, its C–C axis along `axis`, centred at `center`, dangling bonds
/// pointing down. Returns the molecule and its C.
pub fn ethanediyl(center: DVec3, axis: DVec3) -> (AtomicStructure, [u32; 2]) {
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

/// Water with its O at `o`, both H in the vertical plane through `toward`,
/// one leaning 20° below it, the other nearly straight up.
pub fn water(o: DVec3, toward: DVec3) -> AtomicStructure {
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

/// A copy of `s` moved rigidly: `p → q·p + t`.
pub fn moved(s: &AtomicStructure, q: DQuat, t: DVec3) -> AtomicStructure {
    let mut out = s.clone();
    out.transform(&q, &t);
    out
}

/// A copy of `s` with its atoms added in the order `order` gives (a
/// permutation of the sorted atom ids), so every id is relabelled. Returns the
/// copy and the map old id → new id.
pub fn relabelled(s: &AtomicStructure, order: &[u32]) -> (AtomicStructure, HashMap<u32, u32>) {
    let mut out = AtomicStructure::new();
    let mut map = HashMap::new();
    for &id in order {
        let a = s.get_atom(id).unwrap();
        let n = out.add_atom(a.atomic_number, a.position);
        out.set_atom_frozen(n, a.is_frozen());
        for tag in s.atom_tags(id) {
            out.add_atom_tag(n, tag).unwrap();
        }
        map.insert(id, n);
    }
    for &id in order {
        for b in &s.get_atom(id).unwrap().bonds {
            let other = b.other_atom_id();
            if id < other {
                out.add_bond(map[&id], map[&other], b.bond_order());
            }
        }
    }
    (out, map)
}

// ============================================================================
// Reading a plan in input ids
// ============================================================================

/// A change set in *input* ids: formed `(foot, site)` sorted, transfers
/// `(donor, acceptor)` sorted.
pub type InputChange = (Vec<(u32, u32)>, Vec<(u32, u32)>);

pub fn back_maps(setup: &Setup) -> (HashMap<u32, u32>, HashMap<u32, u32>) {
    (
        setup.adsorbate_ids.iter().map(|(&k, &v)| (v, k)).collect(),
        setup.substrate_ids.iter().map(|(&k, &v)| (v, k)).collect(),
    )
}

pub fn input_change(setup: &Setup, key: &HypothesisKey) -> InputChange {
    let (a, s) = back_maps(setup);
    let input = |id: u32| a.get(&id).or(s.get(&id)).copied().unwrap();
    let mut formed: Vec<(u32, u32)> = key
        .0
        .iter()
        .map(|&(x, y)| {
            // Keys are (min, max); the adsorbate side was added first, so
            // its ids are the smaller ones.
            (a[&x.min(y)], s[&x.max(y)])
        })
        .collect();
    formed.sort_unstable();
    let mut moves: Vec<(u32, u32)> = key
        .1
        .iter()
        .map(|&(d, _, acc)| (input(d), input(acc)))
        .collect();
    moves.sort_unstable();
    (formed, moves)
}

/// Every hypothesis of a plan as an input-id change set.
pub fn plan_changes(p: &SequentialPlan) -> BTreeSet<InputChange> {
    p.hypotheses
        .iter()
        .map(|h| input_change(&p.setup, &h.key()))
        .collect()
}

/// The formed bonds of every hypothesis, input ids, sorted.
pub fn plan_bond_sets(p: &SequentialPlan) -> BTreeSet<Vec<(u32, u32)>> {
    plan_changes(p).into_iter().map(|c| c.0).collect()
}

pub fn assert_stats_add_up(p: &SequentialPlan) {
    let s = &p.stats;
    if s.truncated {
        return;
    }
    assert_eq!(
        s.paths,
        s.pruned_valence
            + s.pruned_no_acceptor
            + s.pruned_filter
            + s.duplicates
            + s.anchors
            + s.sphere_pairs
            + s.torus_triples,
        "{s:?}"
    );
    assert_eq!(
        s.anchors + s.sphere_pairs + s.torus_triples,
        p.hypotheses.len()
    );
    assert_eq!(
        s.candidates + s.parents,
        s.to_relax + s.pruned_clash,
        "{s:?}"
    );
    assert_eq!(s.to_relax, p.to_relax.len());
    assert!(s.seating_clashes >= s.pruned_clash);
    assert_eq!(
        s.candidates,
        p.hypotheses.iter().filter(|h| h.candidate).count()
    );
    assert_eq!(
        s.parents,
        p.hypotheses
            .iter()
            .filter(|h| h.parent && !h.candidate)
            .count()
    );
    // Parents are the unmirrored three-leg hypotheses, and only with a local
    // phase to follow.
    for h in &p.hypotheses {
        assert_eq!(
            h.parent,
            s.local_phase && h.legs() == GEOMETRIC_LEGS && h.mirror != Some(Mirror::Mirrored)
        );
        assert_eq!(h.seating.is_some(), h.candidate || h.parent);
    }
    assert_eq!(
        s.pruned_mirror,
        p.hypotheses
            .iter()
            .filter(|h| h.mirror == Some(Mirror::Mirrored))
            .count()
    );
}

// ============================================================================
// The oracle (§11.1)
// ============================================================================

/// What the oracle says about one change set.
#[derive(Debug, Clone, PartialEq)]
pub struct OracleHypothesis {
    pub legs: usize,
    pub mirror: Option<Mirror>,
    pub candidate: bool,
    pub inventory: BondInventory,
}

fn le(have: &BTreeMap<BondKind, usize>, cap: &BTreeMap<BondKind, usize>) -> bool {
    have.iter()
        .all(|(k, n)| *n <= cap.get(k).copied().unwrap_or(0))
}

/// Naive enumeration of the geometric phase: every ordered tuple of up to
/// three legs with distinct feet, each tuple checked against the conditions
/// of §4.2–4.5 as the design states them, the transfer rule applied in tuple
/// order, then normalized and deduplicated by change set.
pub fn oracle(
    setup: &Setup,
    config: &SequentialSearch,
) -> BTreeMap<HypothesisKey, OracleHypothesis> {
    let feet = setup.feet.len();
    let sites = setup.sites.len();
    let inventory_legs = config
        .bond_inventory
        .as_ref()
        .map(|t| t.formed_count() - t.broken_count());
    let cap = [
        Some(GEOMETRIC_LEGS),
        Some(feet),
        config.max_formed_bonds,
        config.formed_bonds,
        inventory_legs,
    ]
    .into_iter()
    .flatten()
    .min()
    .unwrap();

    let mut out = BTreeMap::new();
    // All tuples (f₁, s₁), (f₂, s₂), (f₃, s₃), each prefix a tuple too.
    let mut tuple: Vec<Leg> = Vec::new();
    fn visit(
        setup: &Setup,
        config: &SequentialSearch,
        cap: usize,
        tuple: &mut Vec<Leg>,
        out: &mut BTreeMap<HypothesisKey, OracleHypothesis>,
        feet: usize,
        sites: usize,
    ) {
        if !tuple.is_empty() {
            if let Some((key, h)) = judge(setup, config, tuple) {
                out.entry(key).or_insert(h);
            } else {
                // Every condition is on a prefix too: a rejected tuple has no
                // valid extension.
                return;
            }
        }
        if tuple.len() == cap {
            return;
        }
        for f in 0..feet {
            if tuple.iter().any(|l| l.foot == f) {
                continue;
            }
            for s in 0..sites {
                tuple.push(Leg { foot: f, site: s });
                visit(setup, config, cap, tuple, out, feet, sites);
                tuple.pop();
            }
        }
    }
    visit(setup, config, cap, &mut tuple, &mut out, feet, sites);
    out
}

/// The conditions on one whole tuple, `None` when it fails one.
fn judge(
    setup: &Setup,
    config: &SequentialSearch,
    tuple: &[Leg],
) -> Option<(HypothesisKey, OracleHypothesis)> {
    // No bond between two frozen atoms.
    if tuple.iter().any(|&l| !setup.may_bond(l)) {
        return None;
    }
    // Leg 1 within anchor_reach of its posed foot.
    if setup.anchor_distance(tuple[0]) > config.anchor_reach {
        return None;
    }
    // Every later leg on the shell of every earlier one.
    for j in 1..tuple.len() {
        for i in 0..j {
            if setup.pair_need(tuple[i], tuple[j]) > config.tolerance {
                return None;
            }
        }
    }
    // Valence and the transfer rule, in tuple order.
    let mut left: Vec<usize> = setup.sites.iter().map(|s| s.valence).collect();
    let mut steps = Vec::new();
    let mut inventory = BondInventory::default();
    for &leg in tuple {
        if left[leg.site] == 0 {
            return None;
        }
        left[leg.site] -= 1;
        let foot = &setup.feet[leg.foot];
        *inventory
            .formed
            .entry(BondKind::new(
                foot.element,
                setup.sites[leg.site].element,
                1,
            ))
            .or_insert(0) += 1;
        let acceptor = match foot.donates {
            None => None,
            Some(x) => {
                let a = setup.acceptor(leg.site, &left)?;
                left[a] -= 1;
                *inventory
                    .formed
                    .entry(BondKind::new(x, setup.sites[a].element, 1))
                    .or_insert(0) += 1;
                *inventory
                    .broken
                    .entry(BondKind::new(x, foot.element, 1))
                    .or_insert(0) += 1;
                Some(a)
            }
        };
        steps.push(Step { leg, acceptor });
    }
    // The inventory filter rejects a tuple whose bond kinds exceed it.
    if let Some(target) = &config.bond_inventory
        && !(le(&inventory.formed, &target.formed) && le(&inventory.broken, &target.broken))
    {
        return None;
    }
    let (formed, transfers) = setup.changes(&steps, None);
    let key = change_key(&formed, &transfers);
    let legs = tuple.len();
    let mirror = (legs == 3).then(|| {
        let l = [tuple[0], tuple[1], tuple[2]];
        setup.mirror(l, setup.local_up(&l.map(|l| l.site)))
    });
    let one_foot = setup.feet.len() == 1;
    let candidate = mirror != Some(Mirror::Mirrored)
        && (legs >= 2 || one_foot)
        && config.max_formed_bonds.is_none_or(|m| legs <= m)
        && config.formed_bonds.is_none_or(|n| legs == n)
        && config
            .bond_inventory
            .as_ref()
            .is_none_or(|t| *t == inventory);
    Some((
        key,
        OracleHypothesis {
            legs,
            mirror,
            candidate,
            inventory,
        },
    ))
}

/// The plan's hypotheses in the oracle's shape.
pub fn plan_as_oracle(p: &SequentialPlan) -> BTreeMap<HypothesisKey, OracleHypothesis> {
    p.hypotheses
        .iter()
        .map(|h| {
            (
                h.key(),
                OracleHypothesis {
                    legs: h.legs(),
                    mirror: h.mirror,
                    candidate: h.candidate,
                    inventory: h.inventory.clone(),
                },
            )
        })
        .collect()
}

/// `plan` equals the oracle, both ways, and its per-level counts match.
pub fn assert_matches_oracle(p: &SequentialPlan, config: &SequentialSearch, what: &str) {
    let want = oracle(&p.setup, config);
    let got = plan_as_oracle(p);
    let missing: Vec<_> = want.keys().filter(|k| !got.contains_key(*k)).collect();
    let extra: Vec<_> = got.keys().filter(|k| !want.contains_key(*k)).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{what}: plan vs oracle: {} missing, {} extra; first missing {:?}, first extra {:?}",
        missing.len(),
        extra.len(),
        missing.first(),
        extra.first()
    );
    for (k, w) in &want {
        assert_eq!(&got[k], w, "{what}: {k:?}");
    }
    let level = |n: usize| want.values().filter(|h| h.legs == n).count();
    assert_eq!(p.stats.anchors, level(1), "{what}");
    assert_eq!(p.stats.sphere_pairs, level(2), "{what}");
    assert_eq!(p.stats.torus_triples, level(3), "{what}");
    assert_tree_consistent(p, what);
}

/// The recorded tree against the plan (§11.8): every hypothesis is exactly
/// one canonical row whose path is its steps; every duplicate row points at
/// a canonical row of the same change set and is counted on its parent.
pub fn assert_tree_consistent(p: &SequentialPlan, what: &str) {
    let tree = &p.tree;
    let mut canonical = 0;
    for (i, row) in tree.rows().iter().enumerate() {
        let i = i as u32;
        let path = tree.path(i);
        if let Some(h) = row.hypothesis {
            canonical += 1;
            let h = &p.hypotheses[h as usize];
            assert_eq!(h.row, i, "{what}");
            let legs: Vec<Leg> = h.steps.iter().map(|s| s.leg).collect();
            assert_eq!(path, legs, "{what}: row {i}");
        }
        if let Some(c) = row.duplicate_of {
            assert!(row.hypothesis.is_none());
            assert!(
                tree.children(i).is_empty(),
                "{what}: a duplicate has no children"
            );
            let target = &p.hypotheses[tree.row(c).hypothesis.unwrap() as usize];
            let mut mine = path.clone();
            mine.sort();
            let mut theirs: Vec<Leg> = target.steps.iter().map(|s| s.leg).collect();
            theirs.sort();
            assert_eq!(mine, theirs, "{what}: a duplicate has the same bonds");
        }
        let dups = tree
            .children(i)
            .iter()
            .filter(|&&c| tree.row(c).duplicate_of.is_some())
            .count();
        assert_eq!(row.duplicates as usize, dups, "{what}: row {i}");
        assert_eq!(
            tree.visible_children(i, false).len() + dups,
            tree.children(i).len()
        );
    }
    assert_eq!(canonical, p.hypotheses.len(), "{what}");
    let duplicate_rows = tree
        .rows()
        .iter()
        .filter(|r| r.duplicate_of.is_some())
        .count();
    assert_eq!(duplicate_rows, p.stats.duplicates, "{what}");
}

// ============================================================================
// The audit (§11.5)
// ============================================================================

fn valence_of(z: i16) -> Option<usize> {
    match z {
        1 | 9 | 17 | 35 | 53 => Some(1),
        8 | 16 => Some(2),
        7 | 15 => Some(3),
        6 | 14 | 32 => Some(4),
        _ => None,
    }
}

fn bonds_of(s: &AtomicStructure) -> BTreeSet<(u32, u32)> {
    let mut out = BTreeSet::new();
    for a in s.atoms_values() {
        for b in a.bonds.iter().filter(|b| !b.is_delete_marker()) {
            let o = b.other_atom_id();
            out.insert((a.id.min(o), a.id.max(o)));
        }
    }
    out
}

/// Re-derives every candidate of a report from its structure instead of
/// trusting the hypothesis.
pub fn audit(p: &SequentialPlan, report: &SearchReport, config: &SequentialSearch) {
    let base = &p.setup.combined;
    let before = bonds_of(base);
    let element = |id: u32| base.get_atom(id).unwrap().atomic_number;
    let settings = atomcad_crystolecule::chemisorption::ChemisorptionSearch {
        max_iterations: 0,
        vdw_mode: config.vdw_mode.clone(),
        ..Default::default()
    };
    for c in &report.candidates {
        let after = bonds_of(&c.structure);
        let added: BTreeSet<_> = after.difference(&before).copied().collect();
        let removed: BTreeSet<_> = before.difference(&after).copied().collect();
        let norm = |a: u32, b: u32| (a.min(b), a.max(b));
        let want_added: BTreeSet<_> = c
            .formed
            .iter()
            .map(|&(a, b)| norm(a, b))
            .chain(c.transfers.iter().map(|t| norm(t.moved, t.acceptor)))
            .collect();
        let want_removed: BTreeSet<_> =
            c.transfers.iter().map(|t| norm(t.donor, t.moved)).collect();
        assert_eq!(added, want_added, "audit: bonds added");
        assert_eq!(removed, want_removed, "audit: bonds removed");

        // The inventory from the bond changes alone.
        let mut inventory = BondInventory::default();
        for &(a, b) in &added {
            *inventory
                .formed
                .entry(BondKind::new(element(a), element(b), 1))
                .or_insert(0) += 1;
        }
        for &(a, b) in &removed {
            *inventory
                .broken
                .entry(BondKind::new(element(a), element(b), 1))
                .or_insert(0) += 1;
        }
        assert_eq!(inventory, c.bond_inventory, "audit: inventory");
        assert_eq!(c.formed.len(), added.len() - c.transfers.len());

        // Formed bonds near their rest lengths.
        for &(a, b) in &want_added {
            let pa = c.structure.get_atom(a).unwrap().position;
            let pb = c.structure.get_atom(b).unwrap().position;
            let rest = atomcad_crystolecule::chemisorption::sequential::rest_length(
                element(a),
                element(b),
            );
            let ratio = pa.distance(pb) / rest;
            assert!(
                (0.8..1.3).contains(&ratio),
                "audit: formed bond {a}–{b} at {ratio:.3} × rest (strain {:.1})",
                c.strain
            );
        }

        // Valence, frozen atoms, conservation.
        assert_eq!(c.structure.get_num_of_atoms(), base.get_num_of_atoms());
        for a in c.structure.atoms_values() {
            let orig = base.get_atom(a.id).expect("conserved atoms");
            assert_eq!(a.atomic_number, orig.atomic_number);
            if let Some(v) = valence_of(a.atomic_number) {
                let n = a.bonds.iter().filter(|b| !b.is_delete_marker()).count();
                assert!(n <= v, "audit: atom {} has {n} bonds", a.id);
            }
            if orig.is_frozen() {
                assert!(
                    a.position.distance(orig.position) < 1e-9,
                    "audit: frozen moved"
                );
            }
        }
        for &(a, b) in &added {
            assert!(
                !(base.get_atom(a).unwrap().is_frozen() && base.get_atom(b).unwrap().is_frozen()),
                "audit: bond between two frozen atoms"
            );
        }

        // The strain, recomputed.
        let mut s = c.structure.clone();
        let energy = relax(&mut s, &settings).unwrap().energy;
        assert!(
            (energy - report.reference_energy - c.strain).abs() < 1e-6,
            "audit: strain {} recomputes as {}",
            c.strain,
            energy - report.reference_energy
        );
    }
    // The statistics: the relaxations the plan predicted are the ones made,
    // each local level's add up, and every relaxation is listed once.
    let stats = &report.stats;
    if !p.stats.truncated {
        assert_eq!(stats.relaxed, p.to_relax.len() + stats.local_relaxed);
    }
    assert_eq!(report.relaxed.len(), stats.relaxed);
    assert!(stats.relaxed <= config.budget);
    assert_eq!(
        stats.local_relaxed,
        stats.local.iter().map(|l| l.relaxed).sum::<usize>()
    );
    assert_eq!(
        stats.truncated,
        p.stats.truncated || stats.local.iter().any(|l| l.truncated)
    );
    for (i, l) in stats.local.iter().enumerate() {
        assert_eq!(l.legs, GEOMETRIC_LEGS + 1 + i);
        assert_eq!(
            l.paths,
            l.pruned_valence + l.pruned_no_acceptor + l.pruned_filter + l.duplicates + l.hypotheses,
            "{l:?}"
        );
        assert_eq!(l.relaxed == l.to_relax, !l.truncated, "{l:?}");
        assert!(l.relaxed <= l.to_relax);
    }
    assert_eq!(
        report.local.len(),
        stats.local.iter().map(|l| l.hypotheses).sum::<usize>()
    );
    let rows: BTreeSet<u32> = report.relaxed.iter().map(|r| r.row).collect();
    assert_eq!(rows.len(), report.relaxed.len(), "a row is relaxed once");
}
