//! Cutting a **simulation proxy** out of a large structure around tagged focus
//! atoms — the algorithm behind the `proxy` node. See
//! `doc/design_proxy_node.md`.
//!
//! A proxy is a cluster cut around a reaction site, capped where the cut
//! severed bonds and frozen beyond a given shell so the interior still feels
//! bulk. The cut follows the **bond graph** rather than a sphere: distance is
//! hops over covalent bonds counted over heavy atoms only, so a proxy never
//! contains a fragment the site is not bonded to and the `hops` series is a
//! discrete convergence series.
//!
//! The module is deliberately free of every node-network concept — it takes an
//! [`AtomicStructure`], a list of atom ids and a plain options struct, and it
//! is meant to be usable on its own (§7.1 of the design). Analysis and
//! mutation are split: [`plan_proxy`] decides everything and mutates nothing,
//! `apply_proxy` is the only function that touches the structure.

use crate::atomic_constants::is_allowed_passivant;
use crate::atomic_structure::{AtomicStructure, TagError};
use crate::hydrogen_passivation::terminator_bond_length;
use glam::f64::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

/// The tag `core` paints on the ONIOM high layer. There is no `low` tag:
/// untagged *is* low (§4.7).
pub const HIGH_TAG: &str = "high";

/// Search radius for `ProxyStats::nearest_dropped` (Å). The stat is read
/// against a ~4 Å threshold (§3.3); anything farther is reported as `None`.
pub const NEAREST_DROPPED_RADIUS: f64 = 8.0;

/// Search radius for `ProxyStats::min_cap_pair` (Å): past the 2.42 Å of a
/// clean silicon rim and the ~2 Å that marks an unphysical one.
pub const CAP_PAIR_RADIUS: f64 = 3.0;

// ============================================================================
// Options and errors
// ============================================================================

/// Everything the cut is tunable by. Unsigned on purpose: the module cannot
/// express "-1 means off" except as an `Option`, which is what `core` is. The
/// node's negative-value rules (§4.8) are validation done *before* this struct
/// is built.
#[derive(Debug, Clone, PartialEq)]
pub struct ProxyOptions {
    /// Keep heavy atoms whose bond distance from a source is at most this.
    pub hops: u32,
    /// Heavy atoms farther than this get the frozen flag. Note `hops` plays no
    /// part in freezing: on the plain cut `free >= hops` freezes nothing, and
    /// an atom `fill` restored beyond `free` is frozen whatever `hops` is.
    pub free: u32,
    /// Keep every dropped heavy atom that bridges two or more kept heavy
    /// atoms, to a fixpoint (§4.3). On by default: without it the diamond
    /// lattice leaves clashing cap pairs 1.42 Å apart on silicon.
    pub fill: bool,
    /// Drop heavy atoms the cut left with a single heavy neighbour, to a
    /// fixpoint (§4.4). Off by default so the `hops` series stays monotonic.
    pub rm_single: bool,
    /// Cap every severed bond with a terminator along the old bond vector.
    pub passivate: bool,
    /// Terminator element (atomic number); H, F, Cl, Br or I.
    pub passivant_element: i16,
    /// Tag heavy atoms within this distance as [`HIGH_TAG`]. `None` disables.
    pub core: Option<u32>,
}

impl Default for ProxyOptions {
    fn default() -> Self {
        Self {
            hops: 6,
            free: 3,
            fill: true,
            rm_single: false,
            passivate: true,
            passivant_element: 1,
            core: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("no focus atoms")]
    NoFocusAtoms,
    #[error("passivant element {0} is not one of H, F, Cl, Br, I")]
    BadPassivant(i16),
    #[error(transparent)]
    Tag(#[from] TagError),
}

// ============================================================================
// Plan
// ============================================================================

/// One terminator to place: on `host`, along the old bond to `severed`.
#[derive(Debug, Clone, PartialEq)]
pub struct CapPlacement {
    pub host: u32,
    pub severed: u32,
    pub position: DVec3,
}

/// Everything decided, nothing mutated. Every `Vec` is sorted by atom id
/// (`caps` by `(host, severed)`), so the ids new caps receive at apply time do
/// not depend on hash-map iteration order.
#[derive(Debug, Clone, PartialEq)]
pub struct ProxyPlan {
    /// The options the plan was made with. `apply_proxy` takes no options of
    /// its own, so nothing can be passed that disagrees with the keep set and
    /// caps already decided.
    pub options: ProxyOptions,
    /// Bond distance of every heavy atom reachable from a source. Atoms in
    /// other components are absent. Not bounded by `hops`: the atoms `fill`
    /// restores need their true distance.
    pub distance: FxHashMap<u32, u32>,
    /// Heavy atoms and riders that survive.
    pub kept: Vec<u32>,
    /// Heavy atoms and riders that go.
    pub dropped: Vec<u32>,
    pub caps: Vec<CapPlacement>,
    /// Heavy atoms `fill` restored. Every one has `distance > hops`.
    pub filled: Vec<u32>,
    /// Synchronous fill rounds that collected something.
    pub fill_rounds: usize,
    /// Heavy atoms the cut freezes; riders and caps follow at apply time.
    pub frozen: Vec<u32>,
    /// Heavy atoms the cut tags [`HIGH_TAG`]; riders and caps follow.
    pub high: Vec<u32>,
    /// Closest dropped heavy atom to any atom that will be *free* after the
    /// cut, Å. Planned rather than measured afterwards because the dropped
    /// atoms are gone once `apply_proxy` has run. `None` when nothing dropped
    /// lies within [`NEAREST_DROPPED_RADIUS`].
    pub nearest_dropped: Option<f64>,
}

// ============================================================================
// Graph helpers
// ============================================================================

/// Bonded neighbours of an atom, sorted and deduplicated, delete-marker bonds
/// skipped.
fn neighbors(structure: &AtomicStructure, atom_id: u32) -> Vec<u32> {
    let Some(atom) = structure.get_atom(atom_id) else {
        return Vec::new();
    };
    let mut ids: Vec<u32> = atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .map(|b| b.other_atom_id())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The neighbours of an atom that are **heavy** in the §4.1 sense — not
/// riders. Riders are never traversed and count in no tally.
fn heavy_neighbors(
    structure: &AtomicStructure,
    riders: &FxHashMap<u32, u32>,
    atom_id: u32,
) -> Vec<u32> {
    let mut ids = neighbors(structure, atom_id);
    ids.retain(|n| !riders.contains_key(n));
    ids
}

/// Classify the **riders**: an atom with exactly one bond — a hydrogen, a
/// halogen passivant, any monovalent terminator, and also a singly bonded
/// heavy adatom. Maps each rider to its one heavy neighbour, its *host*.
///
/// A rider is a graph property, not an element property. An atom with **no**
/// bond is heavy (and unreachable, so it is dropped unless it is a source).
pub fn classify_riders(structure: &AtomicStructure) -> FxHashMap<u32, u32> {
    let mut riders = FxHashMap::default();
    for (&id, atom) in structure.iter_atoms() {
        let mut hosts = atom
            .bonds
            .iter()
            .filter(|b| !b.is_delete_marker())
            .map(|b| b.other_atom_id());
        if let Some(host) = hosts.next()
            && hosts.next().is_none()
        {
            riders.insert(id, host);
        }
    }
    riders
}

/// Multi-source breadth-first search over heavy atoms, every source at
/// distance 0.
///
/// `sources` must already be heavy ([`plan_proxy`] promotes riders before
/// calling this; a rider passed here is skipped). Unbounded: every heavy atom
/// in the sources' components gets a distance, atoms in other components get
/// none.
///
/// Multi-source is what makes the motivating case work — before the reaction a
/// tool apex is not bonded to the surface, so a search from the apex alone
/// would never reach a surface atom.
pub fn bond_distances(
    structure: &AtomicStructure,
    sources: &[u32],
    riders: &FxHashMap<u32, u32>,
) -> FxHashMap<u32, u32> {
    let mut distance: FxHashMap<u32, u32> = FxHashMap::default();
    let mut queue: VecDeque<u32> = VecDeque::new();

    let mut seeds: Vec<u32> = sources.to_vec();
    seeds.sort_unstable();
    seeds.dedup();
    for seed in seeds {
        if riders.contains_key(&seed) || structure.get_atom(seed).is_none() {
            continue;
        }
        if distance.insert(seed, 0).is_none() {
            queue.push_back(seed);
        }
    }

    while let Some(current) = queue.pop_front() {
        let d = distance[&current];
        for n in heavy_neighbors(structure, riders, current) {
            if let std::collections::hash_map::Entry::Vacant(slot) = distance.entry(n) {
                slot.insert(d + 1);
                queue.push_back(n);
            }
        }
    }
    distance
}

/// The severed-bond caps of a keep set: one cap per bond from a kept heavy
/// atom to a heavy atom that is not kept, on the old bond vector at the
/// host–terminator length. `riders` says which atoms are not heavy.
///
/// This is the **link-atom construction** — the cap sits exactly where the
/// removed neighbour was, on the true lattice direction. It caps *only severed
/// bonds*, so an atom that was unsaturated in the input (a tool apex, a
/// T-centre carbon) stays unsaturated.
///
/// The result is sorted by `(host, severed)`.
pub fn severed_bond_caps(
    structure: &AtomicStructure,
    is_kept: &dyn Fn(u32) -> bool,
    riders: &FxHashMap<u32, u32>,
    passivant_element: i16,
) -> Vec<CapPlacement> {
    let mut hosts: Vec<u32> = structure.atom_ids().copied().collect();
    hosts.sort_unstable();

    let mut caps = Vec::new();
    for host in hosts {
        if riders.contains_key(&host) || !is_kept(host) {
            continue;
        }
        let Some(atom) = structure.get_atom(host) else {
            continue;
        };
        let host_position = atom.position;
        let host_element = structure.effective_atomic_number(atom);
        let length = terminator_bond_length(host_element, passivant_element);

        for severed in heavy_neighbors(structure, riders, host) {
            if is_kept(severed) {
                continue;
            }
            let Some(other) = structure.get_atom(severed) else {
                continue;
            };
            let delta = other.position - host_position;
            let norm = delta.length();
            if norm <= f64::EPSILON {
                continue;
            }
            caps.push(CapPlacement {
                host,
                severed,
                position: host_position + delta / norm * length,
            });
        }
    }
    caps
}

// ============================================================================
// plan_proxy
// ============================================================================

/// Analysis only: decide the whole cut without touching the structure.
/// `sources` are atom ids (a rider promotes its host, §4.1).
///
/// The steps are §7.3 of the design, in order: riders, sources, distances,
/// plain cut, `fill`, `rm_single`, riders follow, caps, frozen, high,
/// `nearest_dropped`.
pub fn plan_proxy(
    structure: &AtomicStructure,
    sources: &[u32],
    options: &ProxyOptions,
) -> Result<ProxyPlan, ProxyError> {
    // 1. Riders.
    let riders = classify_riders(structure);

    // 2. Sources. A rider promotes its host; a rider whose host is itself a
    //    rider is an isolated diatomic and cannot anchor a cut.
    let mut source_ids: Vec<u32> = Vec::new();
    for &raw in sources {
        if structure.get_atom(raw).is_none() {
            continue;
        }
        match riders.get(&raw) {
            Some(&host) if riders.contains_key(&host) => continue,
            Some(&host) => source_ids.push(host),
            None => source_ids.push(raw),
        }
    }
    source_ids.sort_unstable();
    source_ids.dedup();
    if source_ids.is_empty() {
        return Err(ProxyError::NoFocusAtoms);
    }
    if !is_allowed_passivant(options.passivant_element) {
        return Err(ProxyError::BadPassivant(options.passivant_element));
    }
    let source_set: FxHashSet<u32> = source_ids.iter().copied().collect();

    // 3. Distances.
    let distance = bond_distances(structure, &source_ids, &riders);

    // 4. Plain cut.
    let mut kept_heavy: FxHashSet<u32> = distance
        .iter()
        .filter(|&(_, &d)| d <= options.hops)
        .map(|(&id, _)| id)
        .collect();

    // 5. Fill: keep every dropped heavy atom bridging two or more kept heavy
    //    atoms, to a fixpoint. `pending[b]` accumulates across rounds, so each
    //    kept atom contributes to its neighbours exactly once — the round it
    //    spends on the frontier.
    let mut filled: Vec<u32> = Vec::new();
    let mut fill_rounds = 0usize;
    if options.fill {
        let mut pending: FxHashMap<u32, u32> = FxHashMap::default();
        let mut frontier: Vec<u32> = kept_heavy.iter().copied().collect();
        frontier.sort_unstable();
        loop {
            let mut collected: Vec<u32> = Vec::new();
            for &a in &frontier {
                for b in heavy_neighbors(structure, &riders, a) {
                    if kept_heavy.contains(&b) {
                        continue;
                    }
                    let count = pending.entry(b).or_insert(0);
                    *count += 1;
                    // Exactly 2, so an atom is collected once however many
                    // kept neighbours it turns out to have.
                    if *count == 2 {
                        collected.push(b);
                    }
                }
            }
            if collected.is_empty() {
                break;
            }
            collected.sort_unstable();
            for &b in &collected {
                kept_heavy.insert(b);
            }
            filled.extend_from_slice(&collected);
            fill_rounds += 1;
            frontier = collected;
        }
    }
    filled.sort_unstable();

    // 6. rm_single, on the converged boundary. A kept heavy atom that is not a
    //    source, has exactly one kept heavy neighbour and had more than one
    //    heavy neighbour in the input is dropped — to a fixpoint, because
    //    removing one lowers its inward neighbour's count.
    if options.rm_single {
        let mut worklist: Vec<u32> = kept_heavy.iter().copied().collect();
        worklist.sort_unstable();
        loop {
            let mut batch: Vec<u32> = Vec::new();
            for &a in &worklist {
                if !kept_heavy.contains(&a) || source_set.contains(&a) {
                    continue;
                }
                let hn = heavy_neighbors(structure, &riders, a);
                // Atoms singly bonded in the input (an adatom, a terminal
                // group) are never touched on their own account.
                if hn.len() <= 1 {
                    continue;
                }
                if hn.iter().filter(|n| kept_heavy.contains(n)).count() == 1 {
                    batch.push(a);
                }
            }
            if batch.is_empty() {
                break;
            }
            for &a in &batch {
                kept_heavy.remove(&a);
            }
            let mut next: Vec<u32> = Vec::new();
            for &a in &batch {
                for n in heavy_neighbors(structure, &riders, a) {
                    if kept_heavy.contains(&n) {
                        next.push(n);
                    }
                }
            }
            next.sort_unstable();
            next.dedup();
            worklist = next;
        }
        filled.retain(|id| kept_heavy.contains(id));
    }

    // 7. Riders follow their host.
    let mut kept: Vec<u32> = kept_heavy.iter().copied().collect();
    for (&rider, &host) in &riders {
        if kept_heavy.contains(&host) {
            kept.push(rider);
        }
    }
    kept.sort_unstable();
    let kept_set: FxHashSet<u32> = kept.iter().copied().collect();
    let mut dropped: Vec<u32> = structure
        .atom_ids()
        .copied()
        .filter(|id| !kept_set.contains(id))
        .collect();
    dropped.sort_unstable();

    // 8. Caps.
    let caps = if options.passivate {
        severed_bond_caps(
            structure,
            &|id| kept_heavy.contains(&id),
            &riders,
            options.passivant_element,
        )
    } else {
        Vec::new()
    };

    // 9. Frozen. `hops` plays no part here (§4.6).
    let mut frozen: Vec<u32> = kept_heavy
        .iter()
        .copied()
        .filter(|id| distance.get(id).copied().unwrap_or(u32::MAX) > options.free)
        .collect();
    frozen.sort_unstable();

    // 10. High.
    let mut high: Vec<u32> = match options.core {
        Some(core) => kept_heavy
            .iter()
            .copied()
            .filter(|id| distance.get(id).copied().unwrap_or(u32::MAX) <= core)
            .collect(),
        None => Vec::new(),
    };
    high.sort_unstable();

    // 11. nearest_dropped: the closest dropped heavy atom to any atom that
    //     will be *free* after the cut. A small value means an unbonded
    //     neighbour close enough to matter sterically was cut away.
    let frozen_set: FxHashSet<u32> = frozen.iter().copied().collect();
    let dropped_heavy: FxHashSet<u32> = dropped
        .iter()
        .copied()
        .filter(|id| !riders.contains_key(id))
        .collect();
    let is_free_heavy = |id: u32| {
        kept_heavy.contains(&id)
            && !frozen_set.contains(&id)
            && !structure.get_atom(id).is_some_and(|a| a.is_frozen())
    };
    let mut nearest_dropped: Option<f64> = None;
    if !dropped_heavy.is_empty() {
        for &id in &kept {
            let free = match riders.get(&id) {
                Some(&host) => is_free_heavy(host),
                None => is_free_heavy(id),
            };
            if !free {
                continue;
            }
            let Some(atom) = structure.get_atom(id) else {
                continue;
            };
            let position = atom.position;
            for hit in structure.get_atoms_in_radius(&position, NEAREST_DROPPED_RADIUS) {
                if !dropped_heavy.contains(&hit) {
                    continue;
                }
                let Some(other) = structure.get_atom(hit) else {
                    continue;
                };
                let d = position.distance(other.position);
                if nearest_dropped.is_none_or(|best| d < best) {
                    nearest_dropped = Some(d);
                }
            }
        }
    }

    Ok(ProxyPlan {
        options: options.clone(),
        distance,
        kept,
        dropped,
        caps,
        filled,
        fill_rounds,
        frozen,
        high,
        nearest_dropped,
    })
}
