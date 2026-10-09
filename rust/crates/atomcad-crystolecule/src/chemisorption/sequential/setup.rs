//! The fixed context of one sequential search, and every per-leg predicate
//! the search applies: the shell test, the transfer rule, the local up, the
//! mirror check, seating and clash detection.
//!
//! The predicates are public on purpose. The test oracle enumerates every
//! tuple of legs with its own loops and calls these, so it checks the
//! enumeration, pruning and dedupe around them; the predicates themselves are
//! tested directly.

use crate::atomic_constants::ATOM_INFO;
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure::inline_bond::BOND_SINGLE;
use crate::chemisorption::atoms::{free_valence, reactive_atoms};
use crate::chemisorption::config::{ChemisorptionError, Side};
use crate::chemisorption::transfer::{Transfer, TransferDirection, apply_transfers};
use crate::rigid_fit::rigid_fit;
use crate::simulation::uff::params::{calc_bond_rest_length, get_uff_params};
use crate::simulation::uff::typer::assign_uff_type;
use glam::{DMat3, DQuat, DVec3};
use rustc_hash::FxHashMap;
use std::collections::{BTreeSet, VecDeque};

use super::config::SequentialSearch;

/// The local up is estimated from the substrate atoms within this of the
/// bonded sites (Å, §3).
pub const LOCAL_UP_RADIUS: f64 = 5.0;
/// The mirror check abstains when either handedness sign is this close to
/// zero: `|N·v| / (|N|·|v|)` below sin 15° (§4.5); and, by the same margin,
/// when the foot triangle is nearly collinear ([`triangle_quality`]).
pub const MIRROR_MARGIN: f64 = 0.258_819_045_102_520_8;
/// Below this [`triangle_quality`] the seating points are collinear to
/// rounding, and the mirror check abstains.
pub const DEGENERATE_TRIANGLE: f64 = 1e-6;
/// A seating clashes when a heavy adsorbate atom and a heavy substrate atom
/// are closer than this fraction of their covalent radii's sum (§4.5).
pub const CLASH_FRACTION: f64 = 0.6;
/// The two-leg θ rule samples the rotation about the bond axis in these steps
/// (degrees).
pub const THETA_STEP_DEG: f64 = 10.0;
/// A site that fails a level's test by at most this much is a near miss (Å).
pub const NEAR_MISS_BAND: f64 = 1.0;
/// The shell test's allowance for floating-point rounding (Å).
pub const ROUNDING: f64 = 1e-9;

/// One leg's choice: a foot and a site, as indices into [`Setup::feet`] and
/// [`Setup::sites`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Leg {
    pub foot: usize,
    pub site: usize,
}

/// One leg as it was bound: its bond, and for a donating foot the site its
/// atom went to (fixed when the leg is added, §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Step {
    pub leg: Leg,
    /// Index into [`Setup::sites`].
    pub acceptor: Option<usize>,
}

/// An adsorbate atom that can bond to a site.
#[derive(Debug, Clone)]
pub struct Foot {
    /// Combined id.
    pub id: u32,
    pub element: i16,
    /// Posed position.
    pub position: DVec3,
    pub frozen: bool,
    /// `Some(element)` when the foot bonds by donating one monovalent atom of
    /// that element (an OH leg's H): it has no free valence of its own.
    pub donates: Option<i16>,
    /// The atoms it can donate, combined ids, sorted. Empty unless `donates`.
    pub donatable: Vec<u32>,
    /// Adsorbate atoms within two bonds of it, itself included, sorted. The
    /// clash check ignores them while the foot is bonded.
    pub near: Vec<u32>,
}

/// A substrate atom with a free valence.
#[derive(Debug, Clone)]
pub struct Site {
    /// Combined id.
    pub id: u32,
    pub element: i16,
    pub position: DVec3,
    pub frozen: bool,
    /// Free valence: how many legs and transferred atoms it can take.
    pub valence: usize,
}

/// The mirror check's verdict on a three-leg assignment (§4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mirror {
    /// A proper rotation puts the feet on their seating points, body up.
    Proper,
    /// Only a reflection would: Kabsch would seat the body in the substrate.
    Mirrored,
    /// A sign is too close to zero to tell; treated as proper.
    Undecided,
}

/// A rigid placement of the adsorbate: `p_seated = rotation · p_posed +
/// translation`, for every adsorbate atom.
#[derive(Debug, Clone, PartialEq)]
pub struct Seating {
    pub rotation: DMat3,
    pub translation: DVec3,
    /// The local up it was seated against.
    pub up: DVec3,
    /// The largest distance of a bonded foot from its seating point (Å).
    pub residual: f64,
    /// Clashing heavy-atom pairs `(adsorbate atom, substrate atom)`, combined
    /// ids, sorted.
    pub clashes: Vec<(u32, u32)>,
    /// Per donating step, in step order: the atom that moves (combined id),
    /// the donatable atom nearest its acceptor once seated.
    pub moved: Vec<u32>,
}

impl Seating {
    pub fn apply(&self, p: DVec3) -> DVec3 {
        self.rotation * p + self.translation
    }

    pub fn clashes(&self) -> bool {
        !self.clashes.is_empty()
    }
}

/// A uniform hash grid over a point set, answering "which points lie within
/// `r` of `p`".
#[derive(Debug, Clone, Default)]
struct PointGrid {
    cell: f64,
    cells: FxHashMap<(i32, i32, i32), Vec<usize>>,
}

impl PointGrid {
    fn new(points: impl Iterator<Item = (usize, DVec3)>, cell: f64) -> Self {
        let mut grid = PointGrid {
            cell,
            cells: FxHashMap::default(),
        };
        for (i, p) in points {
            grid.cells.entry(grid.key(p)).or_default().push(i);
        }
        grid
    }

    fn key(&self, p: DVec3) -> (i32, i32, i32) {
        let k = (p / self.cell).floor();
        (k.x as i32, k.y as i32, k.z as i32)
    }

    /// Every point index whose cell is within `r` of `p`'s cell; the caller
    /// checks the distance.
    fn near(&self, p: DVec3, r: f64, mut f: impl FnMut(usize)) {
        let span = (r / self.cell).ceil() as i32;
        let (cx, cy, cz) = self.key(p);
        for dx in -span..=span {
            for dy in -span..=span {
                for dz in -span..=span {
                    if let Some(v) = self.cells.get(&(cx + dx, cy + dy, cz + dz)) {
                        v.iter().copied().for_each(&mut f);
                    }
                }
            }
        }
    }
}

/// The UFF rest length of a single bond between two elements, each typed as
/// a bare atom (sp3 for C, O, N, Si): the length the search seats and tests
/// a new bond at.
pub fn rest_length(a: i16, b: i16) -> f64 {
    let params = |z: i16| assign_uff_type(z, &[]).ok().and_then(get_uff_params);
    match (params(a), params(b)) {
        (Some(pa), Some(pb)) => calc_bond_rest_length(1.0, pa, pb),
        _ => 1.5,
    }
}

fn covalent_radius(z: i16) -> f64 {
    ATOM_INFO
        .get(&(z as i32))
        .map_or(0.7, |i| i.covalent_radius)
}

/// The fixed context of one search: the combined structure at the pose, the
/// feet and the sites, and the lookup structures the predicates use.
#[derive(Debug, Clone)]
pub struct Setup {
    /// Adsorbate and substrate merged, at the given pose, no bond changes.
    pub combined: AtomicStructure,
    /// Input atom id → combined id, per side.
    pub adsorbate_ids: FxHashMap<u32, u32>,
    pub substrate_ids: FxHashMap<u32, u32>,
    /// In combined-id order.
    pub feet: Vec<Foot>,
    /// In combined-id order, so a lower index is a lower id.
    pub sites: Vec<Site>,
    /// Every adsorbate atom, combined ids, sorted, and its posed position.
    pub adsorbate_atoms: Vec<u32>,
    adsorbate_posed: Vec<DVec3>,
    /// Heavy adsorbate atoms: (index into `adsorbate_atoms`, id, covalent
    /// radius).
    adsorbate_heavy: Vec<(usize, u32, f64)>,
    /// Centroid of the adsorbate's heavy atoms other than the feet; `None`
    /// when there are none (the mirror check then abstains).
    pub body: Option<DVec3>,
    /// The search's reach, for the transfer rule.
    reach: f64,
    /// `bond[foot][site]` element-pair rest lengths, by element class.
    foot_class: Vec<usize>,
    site_class: Vec<usize>,
    bond: Vec<Vec<f64>>,
    /// Every substrate atom's position, and a grid over them.
    substrate_positions: Vec<DVec3>,
    substrate_grid: PointGrid,
    /// Heavy substrate atoms: (combined id, position, covalent radius).
    substrate_heavy: Vec<(u32, DVec3, f64)>,
    heavy_grid: PointGrid,
    max_heavy_radius: f64,
    /// The atoms a relaxation can move: every adsorbate atom and every
    /// unfrozen substrate atom, combined ids, sorted. A local-phase state
    /// stores only their positions (§4.6, "Frontier memory").
    pub movable: Vec<u32>,
}

impl Setup {
    /// Combines the inputs and finds the feet and the sites (§4.1).
    pub fn new(
        adsorbate: &AtomicStructure,
        substrate: &AtomicStructure,
        config: &SequentialSearch,
    ) -> Result<Setup, ChemisorptionError> {
        let mut combined = AtomicStructure::new();
        let adsorbate_ids = combined.add_atomic_structure(adsorbate)?;
        let substrate_ids = combined.add_atomic_structure(substrate)?;
        let ads_reactive = reactive_atoms(
            adsorbate,
            &adsorbate_ids,
            &config.adsorbate_tag,
            Side::Adsorbate,
        )?;
        let sub_reactive = reactive_atoms(
            substrate,
            &substrate_ids,
            &config.substrate_tag,
            Side::Substrate,
        )?;
        let atom = |id: u32| combined.get_atom(id).expect("combined atom");

        let mut donated: Vec<i16> = config
            .transfers
            .iter()
            .filter(|r| r.direction == TransferDirection::ToSubstrate)
            .map(|r| r.element)
            .collect();
        donated.sort_unstable();
        donated.dedup();

        let mut feet = Vec::new();
        for &id in &ads_reactive {
            let a = atom(id);
            let mut foot = Foot {
                id,
                element: a.atomic_number,
                position: a.position,
                frozen: a.is_frozen(),
                donates: None,
                donatable: Vec::new(),
                near: Vec::new(),
            };
            if free_valence(&combined, id) == 0 {
                // No valence of its own: a foot only if it can donate an atom
                // under a rule — the first rule element it carries.
                let Some((element, atoms)) = donated.iter().find_map(|&z| {
                    let atoms = donatable_atoms(&combined, id, z);
                    (!atoms.is_empty()).then_some((z, atoms))
                }) else {
                    continue;
                };
                foot.donates = Some(element);
                foot.donatable = atoms;
            }
            foot.near = within_two_bonds(&combined, id);
            feet.push(foot);
        }

        let sites: Vec<Site> = sub_reactive
            .iter()
            .filter_map(|&id| {
                let valence = free_valence(&combined, id);
                (valence > 0).then(|| {
                    let a = atom(id);
                    Site {
                        id,
                        element: a.atomic_number,
                        position: a.position,
                        frozen: a.is_frozen(),
                        valence,
                    }
                })
            })
            .collect();

        let mut adsorbate_atoms: Vec<u32> = adsorbate_ids.values().copied().collect();
        adsorbate_atoms.sort_unstable();
        let adsorbate_posed: Vec<DVec3> = adsorbate_atoms
            .iter()
            .map(|&id| atom(id).position)
            .collect();
        let adsorbate_heavy: Vec<(usize, u32, f64)> = adsorbate_atoms
            .iter()
            .enumerate()
            .map(|(i, &id)| (i, id, atom(id).atomic_number))
            .filter(|&(_, _, z)| z != 1)
            .map(|(i, id, z)| (i, id, covalent_radius(z)))
            .collect();
        let foot_ids: BTreeSet<u32> = feet.iter().map(|f| f.id).collect();
        let body_atoms: Vec<DVec3> = adsorbate_atoms
            .iter()
            .filter(|&&id| atom(id).atomic_number != 1 && !foot_ids.contains(&id))
            .map(|&id| atom(id).position)
            .collect();
        let body = (!body_atoms.is_empty())
            .then(|| body_atoms.iter().copied().sum::<DVec3>() / body_atoms.len() as f64);

        // Rest lengths per (foot element, site element).
        let classes = |elements: Vec<i16>| -> (Vec<usize>, Vec<i16>) {
            let mut distinct = elements.clone();
            distinct.sort_unstable();
            distinct.dedup();
            let class = elements
                .iter()
                .map(|z| distinct.binary_search(z).expect("listed"))
                .collect();
            (class, distinct)
        };
        let (foot_class, foot_elements) = classes(feet.iter().map(|f| f.element).collect());
        let (site_class, site_elements) = classes(sites.iter().map(|s| s.element).collect());
        let bond = foot_elements
            .iter()
            .map(|&a| site_elements.iter().map(|&b| rest_length(a, b)).collect())
            .collect();

        let mut substrate_atoms: Vec<u32> = substrate_ids.values().copied().collect();
        substrate_atoms.sort_unstable();
        let substrate_positions: Vec<DVec3> = substrate_atoms
            .iter()
            .map(|&id| atom(id).position)
            .collect();
        let substrate_grid = PointGrid::new(
            substrate_positions.iter().copied().enumerate(),
            LOCAL_UP_RADIUS,
        );
        let substrate_heavy: Vec<(u32, DVec3, f64)> = substrate_atoms
            .iter()
            .map(|&id| atom(id))
            .filter(|a| a.atomic_number != 1)
            .map(|a| (a.id, a.position, covalent_radius(a.atomic_number)))
            .collect();
        let heavy_grid = PointGrid::new(substrate_heavy.iter().map(|h| h.1).enumerate(), 2.0);
        let max_heavy_radius = substrate_heavy.iter().map(|h| h.2).fold(0.0, f64::max);
        let mut movable: Vec<u32> = adsorbate_atoms
            .iter()
            .copied()
            .chain(
                substrate_atoms
                    .iter()
                    .copied()
                    .filter(|&id| !atom(id).is_frozen()),
            )
            .collect();
        movable.sort_unstable();

        Ok(Setup {
            combined,
            adsorbate_ids,
            substrate_ids,
            feet,
            sites,
            adsorbate_atoms,
            adsorbate_posed,
            adsorbate_heavy,
            body,
            reach: config.reach,
            foot_class,
            site_class,
            bond,
            substrate_positions,
            substrate_grid,
            substrate_heavy,
            heavy_grid,
            max_heavy_radius,
            movable,
        })
    }

    /// The rest length `b` of the bond a leg forms.
    pub fn bond_length(&self, leg: Leg) -> f64 {
        self.bond[self.foot_class[leg.foot]][self.site_class[leg.site]]
    }

    /// Whether a leg may bond at all: no bond forms between two frozen atoms.
    pub fn may_bond(&self, leg: Leg) -> bool {
        !(self.feet[leg.foot].frozen && self.sites[leg.site].frozen)
    }

    /// Leg 1's test quantity: the posed foot's distance to the site (§4.2).
    pub fn anchor_distance(&self, leg: Leg) -> f64 {
        self.feet[leg.foot]
            .position
            .distance(self.sites[leg.site].position)
    }

    /// How much tolerance a pair of legs needs to pass the shell test (§4.3),
    /// 0 when it passes at tolerance 0: with `d` the posed foot spacing, `b₁`,
    /// `b₂` the bond lengths and `r` the site spacing, the shell is
    /// `d − b₁ − b₂ ≤ r ≤ d + b₁ + b₂`. A pair passes at tolerance `t` iff
    /// `pair_need ≤ t`. The ring of leg 3 is this test against both bonded
    /// legs (§4.4). It absorbs [`ROUNDING`], so a site exactly on a bound —
    /// a planted binding, an ideal lattice — passes at tolerance 0 whatever
    /// the last bit of the distances.
    pub fn pair_need(&self, a: Leg, b: Leg) -> f64 {
        let d = self.feet[a.foot]
            .position
            .distance(self.feet[b.foot].position);
        let slack = self.bond_length(a) + self.bond_length(b);
        let r = self.sites[a.site]
            .position
            .distance(self.sites[b.site].position);
        ((r - (d + slack)).max((d - slack) - r) - ROUNDING).max(0.0)
    }

    /// The largest [`pair_need`](Self::pair_need) of `leg` against every
    /// bonded leg: the tolerance a new leg needs to join them.
    pub fn need_against(&self, bonded: &[Leg], leg: Leg) -> f64 {
        bonded
            .iter()
            .map(|&b| self.pair_need(b, leg))
            .fold(0.0, f64::max)
    }

    /// The transfer rule (§5): the site nearest to `from` (the site the
    /// donating foot bonded to), among the sites with valence left in
    /// `left`, not `from` itself, within `reach` of it (site to site). Ties go
    /// to the lower atom id. `None` = no acceptor, the leg is dropped.
    pub fn acceptor(&self, from: usize, left: &[usize]) -> Option<usize> {
        let origin = self.sites[from].position;
        let mut best: Option<(f64, usize)> = None;
        for (j, site) in self.sites.iter().enumerate() {
            if j == from || left[j] == 0 {
                continue;
            }
            let d = site.position.distance(origin);
            if d > self.reach {
                continue;
            }
            // Sites are in id order, so a strict comparison keeps the lower id.
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, j));
            }
        }
        best.map(|(_, j)| j)
    }

    /// The local up `û` at a set of bonded sites (§3): from the centroid of
    /// the substrate atoms within [`LOCAL_UP_RADIUS`] of any of them to the
    /// centroid of the sites. Falls back to `+z` when the two coincide.
    pub fn local_up(&self, sites: &[usize]) -> DVec3 {
        // Sorted, so the sums run in one order whatever order the legs were
        // bound in, and the result is the same bit for bit.
        let mut sorted = sites.to_vec();
        sorted.sort_unstable();
        let positions: Vec<DVec3> = sorted.iter().map(|&s| self.sites[s].position).collect();
        let centre = positions.iter().copied().sum::<DVec3>() / positions.len() as f64;
        let mut near: Vec<usize> = Vec::new();
        for &p in &positions {
            self.substrate_grid.near(p, LOCAL_UP_RADIUS, |i| {
                if self.substrate_positions[i].distance(p) <= LOCAL_UP_RADIUS {
                    near.push(i);
                }
            });
        }
        near.sort_unstable();
        near.dedup();
        let around = near
            .iter()
            .map(|&i| self.substrate_positions[i])
            .sum::<DVec3>()
            / near.len().max(1) as f64;
        let up = centre - around;
        if up.length() < 1e-9 {
            DVec3::Z
        } else {
            up.normalize()
        }
    }

    /// Each leg's seating point `pᵢ = sᵢ + bᵢ·û`.
    pub fn seating_points(&self, legs: &[Leg], up: DVec3) -> Vec<DVec3> {
        legs.iter()
            .map(|&l| self.sites[l.site].position + up * self.bond_length(l))
            .collect()
    }

    /// The mirror check of three legs (§4.5): the assignment is proper when
    /// the body lies on the same side of its feet as the open side of the
    /// seating points. Independent of the order of the legs.
    pub fn mirror(&self, legs: [Leg; 3], up: DVec3) -> Mirror {
        let Some(body) = self.body else {
            return Mirror::Undecided;
        };
        let f = legs.map(|l| self.feet[l.foot].position);
        let p = self.seating_points(&legs, up);
        let foot_normal = (f[1] - f[0]).cross(f[2] - f[0]);
        let to_body = body - (f[0] + f[1] + f[2]) / 3.0;
        let site_normal = (p[1] - p[0]).cross(p[2] - p[0]);
        let cos_f = foot_normal.dot(to_body) / (foot_normal.length() * to_body.length());
        let cos_s = site_normal.dot(up) / (site_normal.length() * up.length());
        // Nearly collinear feet abstain (§4.5). Collinear seating points only
        // when they are degenerate to rounding: their normal is then noise,
        // and which way it points would depend on the order of the legs.
        if triangle_quality(&f) < MIRROR_MARGIN
            || triangle_quality(&[p[0], p[1], p[2]]) < DEGENERATE_TRIANGLE
            || !(cos_f.is_finite() && cos_s.is_finite())
            || cos_f.abs() < MIRROR_MARGIN
            || cos_s.abs() < MIRROR_MARGIN
        {
            Mirror::Undecided
        } else if cos_f.signum() == cos_s.signum() {
            Mirror::Proper
        } else {
            Mirror::Mirrored
        }
    }

    /// The posed position of every adsorbate atom, in
    /// [`adsorbate_atoms`](Self::adsorbate_atoms) order.
    pub fn posed(&self) -> &[DVec3] {
        &self.adsorbate_posed
    }

    /// Clashing heavy-atom pairs of a rigid placement of the adsorbate (§4.5),
    /// sorted. Excluded: the bonded feet and the atoms within two bonds of
    /// them, the bonded sites, and every hydrogen.
    pub fn clashes(&self, legs: &[Leg], rotation: DMat3, translation: DVec3) -> Vec<(u32, u32)> {
        let excluded = self.exclusions(legs);
        let mut out = Vec::new();
        self.for_each_clash(&excluded, rotation, translation, |a, s| {
            out.push((a, s));
            true
        });
        out.sort_unstable();
        out
    }

    /// What the clash check ignores for these bonded legs: (adsorbate atoms,
    /// substrate atoms), each sorted.
    fn exclusions(&self, legs: &[Leg]) -> (Vec<u32>, Vec<u32>) {
        let mut ads: Vec<u32> = legs
            .iter()
            .flat_map(|l| self.feet[l.foot].near.iter().copied())
            .collect();
        ads.sort_unstable();
        ads.dedup();
        let mut sub: Vec<u32> = legs.iter().map(|l| self.sites[l.site].id).collect();
        sub.sort_unstable();
        sub.dedup();
        (ads, sub)
    }

    /// The number of clashing pairs, counting no further than `limit`.
    fn clash_count(
        &self,
        excluded: &(Vec<u32>, Vec<u32>),
        rotation: DMat3,
        translation: DVec3,
        limit: usize,
    ) -> usize {
        let mut n = 0;
        self.for_each_clash(excluded, rotation, translation, |_, _| {
            n += 1;
            n < limit
        });
        n
    }

    /// Calls `f` for each clashing pair until it returns `false`.
    fn for_each_clash(
        &self,
        (excluded_ads, excluded_sub): &(Vec<u32>, Vec<u32>),
        rotation: DMat3,
        translation: DVec3,
        mut f: impl FnMut(u32, u32) -> bool,
    ) {
        for &(i, id, ra) in &self.adsorbate_heavy {
            if excluded_ads.binary_search(&id).is_ok() {
                continue;
            }
            let p = rotation * self.adsorbate_posed[i] + translation;
            let reach = CLASH_FRACTION * (ra + self.max_heavy_radius);
            let mut go = true;
            self.heavy_grid.near(p, reach, |j| {
                let (sid, sp, rs) = self.substrate_heavy[j];
                if go
                    && p.distance(sp) < CLASH_FRACTION * (ra + rs)
                    && excluded_sub.binary_search(&sid).is_err()
                {
                    go = f(id, sid);
                }
            });
            if !go {
                return;
            }
        }
    }

    /// Seats a hypothesis (§4.5): one leg by translation, two by the θ rule,
    /// three or more by a Kabsch fit of the bonded feet onto their seating
    /// points. A function of the set of legs, never of their order, and for
    /// two or more legs never of the pose.
    ///
    /// Collinear seating points (sites along one dimer row) leave the fit's
    /// turn about their line undetermined — every turn fits equally well — so
    /// that turn is chosen as for two legs: body up, first clash-free angle.
    pub fn seat(&self, steps: &[Step]) -> Seating {
        // In leg order, not binding order, so the seating is the same bit for
        // bit whatever order the legs were bound in.
        let mut legs: Vec<Leg> = steps.iter().map(|s| s.leg).collect();
        legs.sort_unstable();
        let sites: Vec<usize> = legs.iter().map(|l| l.site).collect();
        let up = self.local_up(&sites);
        let points = self.seating_points(&legs, up);
        let feet: Vec<DVec3> = legs.iter().map(|l| self.feet[l.foot].position).collect();
        let (rotation, translation) = match legs.len() {
            0 => (DMat3::IDENTITY, DVec3::ZERO),
            1 => (DMat3::IDENTITY, points[0] - feet[0]),
            2 => {
                let (mid_f, mid_p) = ((feet[0] + feet[1]) / 2.0, (points[0] + points[1]) / 2.0);
                let foot_axis = feet[1] - feet[0];
                let point_axis = points[1] - points[0];
                let align = if foot_axis.length() < 1e-9 || point_axis.length() < 1e-9 {
                    DMat3::IDENTITY
                } else {
                    DMat3::from_quat(DQuat::from_rotation_arc(
                        foot_axis.normalize(),
                        point_axis.normalize(),
                    ))
                };
                // About the seating axis; with coincident seating points (two
                // feet on one site) about the local up instead.
                let axis = if point_axis.length() < 1e-9 {
                    up
                } else {
                    point_axis.normalize()
                };
                self.turn_body_up(&legs, align, mid_p - align * mid_f, axis, mid_p, up)
            }
            _ => {
                let fit = rigid_fit(&feet, &points, false).expect("three or more points");
                match collinear_axis(&points) {
                    Some(axis) => {
                        let pivot = points.iter().copied().sum::<DVec3>() / points.len() as f64;
                        self.turn_body_up(&legs, fit.r, fit.t, axis, pivot, up)
                    }
                    None => (fit.r, fit.t),
                }
            }
        };
        let residual = feet
            .iter()
            .zip(&points)
            .map(|(&f, &p)| (rotation * f + translation).distance(p))
            .fold(0.0, f64::max);
        let clashes = self.clashes(&legs, rotation, translation);
        let moved = steps
            .iter()
            .filter_map(|s| s.acceptor.map(|a| (s.leg.foot, a)))
            .map(|(foot, acceptor)| {
                let target = self.sites[acceptor].position;
                let seated = |id: u32| {
                    let i = self
                        .adsorbate_atoms
                        .binary_search(&id)
                        .expect("an adsorbate atom");
                    rotation * self.adsorbate_posed[i] + translation
                };
                *self.feet[foot]
                    .donatable
                    .iter()
                    .min_by(|&&a, &&b| {
                        seated(a)
                            .distance(target)
                            .total_cmp(&seated(b).distance(target))
                            .then(a.cmp(&b))
                    })
                    .expect("a donating foot has a donatable atom")
            })
            .collect();
        Seating {
            rotation,
            translation,
            up,
            residual,
            clashes,
            moved,
        }
    }

    /// The θ rule (§4.5): from a placement `(rotation, translation)` that
    /// leaves one turn free — about `axis` through `pivot` — turn so the
    /// adsorbate's centroid points as far along `up` as it can (θ = 0, the
    /// body straight above its bonds); then try θ = 0, ±10°, ±20°, … and keep
    /// the first clash-free angle, or, if every angle clashes, the first with
    /// the fewest clashing pairs. The body's height falls with |θ|, so this is
    /// the highest clash-free angle.
    ///
    /// θ = 0 is defined by the molecule and the sites alone, so the result
    /// does not depend on which placement of the free family it started from.
    fn turn_body_up(
        &self,
        legs: &[Leg],
        rotation: DMat3,
        translation: DVec3,
        axis: DVec3,
        pivot: DVec3,
        up: DVec3,
    ) -> (DMat3, DVec3) {
        let centroid =
            self.adsorbate_posed.iter().copied().sum::<DVec3>() / self.adsorbate_posed.len() as f64;
        let off_axis = |v: DVec3| v - axis * v.dot(axis);
        let body = off_axis(rotation * centroid + translation - pivot);
        let target = off_axis(up);
        let theta0 = if body.length() < 1e-9 || target.length() < 1e-9 {
            0.0
        } else {
            body.cross(target).dot(axis).atan2(body.dot(target))
        };
        let excluded = self.exclusions(legs);
        let half = (180.0 / THETA_STEP_DEG).round() as i64;
        let offsets = std::iter::once(0)
            .chain((1..half).flat_map(|j| [j, -j]))
            .chain([half]);
        let mut best: Option<(usize, DMat3, DVec3)> = None;
        for j in offsets {
            let theta = theta0 + (j as f64 * THETA_STEP_DEG).to_radians();
            let turn = DMat3::from_axis_angle(axis, theta);
            // p → turn·(R·p + t − pivot) + pivot
            let r = turn * rotation;
            let t = turn * (translation - pivot) + pivot;
            let limit = best.as_ref().map_or(usize::MAX, |b| b.0);
            let clashes = self.clash_count(&excluded, r, t, limit);
            if clashes < limit {
                best = Some((clashes, r, t));
                if clashes == 0 {
                    break;
                }
            }
        }
        let (_, r, t) = best.expect("at least one angle");
        (r, t)
    }

    /// The bond changes of a set of steps: formed `(foot, site)` pairs in step
    /// order and the transfers, in combined ids. `moved` names each donating
    /// step's moving atom (a seating's choice); without it, the lowest id.
    pub fn changes(
        &self,
        steps: &[Step],
        moved: Option<&[u32]>,
    ) -> (Vec<(u32, u32)>, Vec<Transfer>) {
        let formed = steps
            .iter()
            .map(|s| (self.feet[s.leg.foot].id, self.sites[s.leg.site].id))
            .collect();
        let transfers = steps
            .iter()
            .filter_map(|s| s.acceptor.map(|a| (s.leg.foot, a)))
            .enumerate()
            .map(|(i, (f, a))| {
                let foot = &self.feet[f];
                Transfer {
                    donor: foot.id,
                    moved: moved.map_or(foot.donatable[0], |m| m[i]),
                    acceptor: self.sites[a].id,
                    element: foot.donates.expect("a donating foot"),
                }
            })
            .collect();
        (formed, transfers)
    }

    /// The positions of the [`movable`](Self::movable) atoms in `s`, in that
    /// order: all a local-phase state keeps of a relaxed structure.
    pub fn movable_positions(&self, s: &AtomicStructure) -> Vec<DVec3> {
        self.movable
            .iter()
            .map(|&id| s.get_atom(id).expect("a combined atom").position)
            .collect()
    }

    /// Where atom `id` is in a state stored as movable positions: its stored
    /// position, or, for a frozen substrate atom, its input position.
    pub fn position_in(&self, positions: &[DVec3], id: u32) -> DVec3 {
        match self.movable.binary_search(&id) {
            Ok(i) => positions[i],
            Err(_) => {
                self.combined
                    .get_atom(id)
                    .expect("a combined atom")
                    .position
            }
        }
    }

    /// A relaxed state rebuilt from what a local-phase state keeps: the
    /// combined structure with the bond changes of `steps` applied (each
    /// transfer moving the atom `moved` names, one per donating step, in step
    /// order) and the movable atoms at `positions`. Nothing is re-seated:
    /// the positions already hold every atom where the relaxation left it.
    pub fn state_structure(
        &self,
        steps: &[Step],
        moved: &[u32],
        positions: &[DVec3],
    ) -> AtomicStructure {
        let mut s = self.combined.clone();
        for (&id, &p) in self.movable.iter().zip(positions) {
            s.set_atom_position(id, p);
        }
        let (formed, transfers) = self.changes(steps, Some(moved));
        let mut transfers = transfers.into_iter();
        // Step by step, each bond before its transfer, so a state and its
        // child apply their shared changes in one order.
        for (step, bond) in steps.iter().zip(formed) {
            s.add_bond_checked(bond.0, bond.1, BOND_SINGLE);
            if step.acceptor.is_some() {
                let t = transfers.next().expect("one transfer per donating step");
                s.delete_bond(&crate::atomic_structure::BondReference {
                    atom_id1: t.donor,
                    atom_id2: t.moved,
                });
                s.add_bond_checked(t.moved, t.acceptor, BOND_SINGLE);
            }
        }
        s
    }

    /// The start geometry of a local-phase hypothesis (§4.6): its relaxed
    /// parent (`parent_steps`, `parent_moved`, `positions`) plus one more
    /// leg, `step`. The new bond is added as it stands; a donating step moves
    /// the donatable atom nearest its acceptor in the parent, seated on the
    /// acceptor as in the geometric phase (§5). Returns the structure and that
    /// atom, if any. The parent's transferred atoms stay where they are.
    pub fn grown_structure(
        &self,
        parent_steps: &[Step],
        parent_moved: &[u32],
        positions: &[DVec3],
        step: Step,
    ) -> (AtomicStructure, Option<u32>) {
        let mut s = self.state_structure(parent_steps, parent_moved, positions);
        let foot = &self.feet[step.leg.foot];
        s.add_bond_checked(foot.id, self.sites[step.leg.site].id, BOND_SINGLE);
        let moved = step.acceptor.map(|a| {
            let acceptor = self.sites[a].id;
            let target = self.position_in(positions, acceptor);
            let moved = *foot
                .donatable
                .iter()
                .min_by(|&&a, &&b| {
                    let d = |id: u32| self.position_in(positions, id).distance(target);
                    d(a).total_cmp(&d(b)).then(a.cmp(&b))
                })
                .expect("a donating foot has a donatable atom");
            apply_transfers(
                &mut s,
                &[Transfer {
                    donor: foot.id,
                    moved,
                    acceptor,
                    element: foot.donates.expect("a donating foot"),
                }],
            );
            moved
        });
        (s, moved)
    }

    /// The start geometry of a seated hypothesis: the combined structure with
    /// the adsorbate moved rigidly, each transferred atom re-seated on its
    /// acceptor, and every new bond added.
    pub fn start_structure(&self, steps: &[Step], seating: &Seating) -> AtomicStructure {
        let mut s = self.combined.clone();
        for (i, &id) in self.adsorbate_atoms.iter().enumerate() {
            s.set_atom_position(id, seating.apply(self.adsorbate_posed[i]));
        }
        let (formed, transfers) = self.changes(steps, Some(&seating.moved));
        apply_transfers(&mut s, &transfers);
        for (a, b) in formed {
            s.add_bond_checked(a, b, BOND_SINGLE);
        }
        s
    }
}

/// How far a triangle is from collinear, independent of its size and of the
/// order of its corners: `2√3·|N| / Σ|edge|²`, 1 for an equilateral triangle
/// and 0 for three points on a line.
pub fn triangle_quality(t: &[DVec3; 3]) -> f64 {
    let n = (t[1] - t[0]).cross(t[2] - t[0]).length();
    let edges =
        t[0].distance_squared(t[1]) + t[1].distance_squared(t[2]) + t[2].distance_squared(t[0]);
    if edges < 1e-18 {
        return 0.0;
    }
    2.0 * 3f64.sqrt() * n / edges
}

/// The direction of the line three or more points lie on, when they are
/// collinear to rounding (every point within `DEGENERATE_TRIANGLE` × the
/// spread of the line through the two farthest apart); `None` otherwise.
pub fn collinear_axis(points: &[DVec3]) -> Option<DVec3> {
    let mut far = (0, 0, 0.0);
    for i in 0..points.len() {
        for j in i + 1..points.len() {
            let d = points[i].distance_squared(points[j]);
            if d > far.2 {
                far = (i, j, d);
            }
        }
    }
    let span = far.2.sqrt();
    if span < 1e-9 {
        return None;
    }
    let axis = (points[far.1] - points[far.0]) / span;
    let off = |p: DVec3| (p - points[far.0]).cross(axis).length();
    points
        .iter()
        .all(|&p| off(p) <= DEGENERATE_TRIANGLE * span)
        .then_some(axis)
}

/// The atoms of element `z` that `donor` can give away: bonded to it alone,
/// by a single bond, and unfrozen (the atom must move). Sorted.
fn donatable_atoms(s: &AtomicStructure, donor: u32, z: i16) -> Vec<u32> {
    let Some(d) = s.get_atom(donor) else {
        return Vec::new();
    };
    let mut out: Vec<u32> = d
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker() && b.bond_order() == BOND_SINGLE)
        .map(|b| b.other_atom_id())
        .filter(|&x| {
            s.get_atom(x).is_some_and(|a| {
                a.atomic_number == z
                    && !a.is_frozen()
                    && a.bonds.iter().filter(|b| !b.is_delete_marker()).count() == 1
            })
        })
        .collect();
    out.sort_unstable();
    out
}

/// `id` and the atoms within two bonds of it, sorted.
fn within_two_bonds(s: &AtomicStructure, id: u32) -> Vec<u32> {
    let mut seen = BTreeSet::from([id]);
    let mut queue = VecDeque::from([(id, 0)]);
    while let Some((a, depth)) = queue.pop_front() {
        if depth == 2 {
            continue;
        }
        let Some(atom) = s.get_atom(a) else {
            continue;
        };
        for b in atom.bonds.iter().filter(|b| !b.is_delete_marker()) {
            if seen.insert(b.other_atom_id()) {
                queue.push_back((b.other_atom_id(), depth + 1));
            }
        }
    }
    seen.into_iter().collect()
}
