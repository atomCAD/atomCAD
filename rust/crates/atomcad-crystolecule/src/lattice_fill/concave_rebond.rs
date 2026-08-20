//! Concave-corner rebonding — see `doc/design_concave_rebonding.md`.
//!
//! The (100) 2×1 dimer search is all-or-nothing: a surface atom whose partner
//! is missing, or classifies to a different facet, is silently dropped and
//! keeps both dangling bonds. Passivation then gives it two terminators. Over
//! open vacuum that is harmless, but at a **concave corner** — a (100) terrace
//! meeting an ascending wall — the second terminator points straight at a
//! terminator on the wall and the two land ~1.5 Å apart (H) or ~0.5 Å apart
//! (Cl). The real surface resolves that by dropping both and letting the two
//! host atoms bond directly across the corner: a *rebonded step edge*.
//!
//! This pass performs exactly that rewrite. It is **coordination-neutral** —
//! each host loses one terminator bond and gains one host–host bond — so it can
//! neither over-coordinate an atom nor create new single-bond atoms.
//!
//! It runs **after** `hydrogen_passivate`, and that ordering is forced:
//! `hydrogen_passivate` decides "is this bond dangling?" from the *motif*, not
//! from the atom's actual bonds, so a shortcut bond added earlier would not stop
//! it placing the terminator as well (design D1).

use crate::atomic_constants::{ATOM_INFO, is_allowed_passivant};
use crate::atomic_structure::AtomicStructure;
use crate::lattice_fill::placed_atom_tracker::PlacedAtomTracker;
use glam::f64::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};

// ============================================================================
// Criterion constants (design §5)
// ============================================================================

/// Test 3 — two terminators clash when they are closer than this fraction of
/// the sum of their van der Waals radii.
///
/// Measured on the `concave_rebond_test` fixture: real clashes sit at 0.634 of
/// the H–H vdW sum and the nearest legitimate contact at 1.195, so 0.75 has
/// ~1.2× headroom below and ~1.6× margin above with nothing in between.
///
/// The fraction (rather than an absolute distance) is what lets one constant
/// cover every host/terminator combination: the same geometry gives 1.52 Å in
/// Si–H and 0.74 Å in C–H.
const CLASH_FRACTION: f64 = 0.75;

/// Test 4 — the two hosts must be no further apart than this multiple of the
/// second-neighbour separation `a/√2`, the distance a rebond has to span.
/// Keeps the pass from stitching a narrow trench shut.
const HOST_SEPARATION_FACTOR: f64 = 1.05;

/// Test 5 — each dangling bond must point within 60° of the other host, i.e.
/// the two really do face each other across a concave corner rather than
/// brushing past each other on a flat face.
const FACING_COS: f64 = 0.5;

/// Fallback van der Waals radius (Å) for an element missing from `ATOM_INFO`.
/// Only reachable for a terminator element that passed `is_allowed_passivant`,
/// so in practice unreachable; hydrogen's radius is the conservative choice.
const FALLBACK_VDW_RADIUS: f64 = 1.20;

fn van_der_waals_radius(atomic_number: i16) -> f64 {
    ATOM_INFO
        .get(&(atomic_number as i32))
        .map(|info| info.van_der_waals_radius)
        .unwrap_or(FALLBACK_VDW_RADIUS)
}

/// A passivation terminator and the lattice atom it hangs off.
struct Terminator {
    id: u32,
    host: u32,
    position: DVec3,
    vdw: f64,
}

/// One accepted-so-far clash, before the greedy pass decides whether to apply it.
struct Candidate {
    distance: f64,
    terminator_a: u32,
    terminator_b: u32,
    host_a: u32,
    host_b: u32,
}

/// Collects the passivation terminators (design §6).
///
/// A terminator (a) has exactly one bond, (b) is of a monovalent passivant
/// element, and (c) is **not recorded in the `PlacedAtomTracker`**.
///
/// Test (c) is the exact discriminator, and the reason this needs no new
/// bookkeeping: `record_atom` is called for every motif-placed atom and for
/// nothing else, so *tracked = lattice atom, untracked = terminator added by
/// passivation*. Identifying terminators by element alone would misread a motif
/// that legitimately contains a monovalent halogen (a salt, an organic crystal)
/// as being covered in terminators.
///
/// Returned sorted by id so everything downstream is order-independent.
fn collect_terminators(structure: &AtomicStructure, tracked: &FxHashSet<u32>) -> Vec<Terminator> {
    let mut terminators: Vec<Terminator> = structure
        .iter_atoms()
        .filter_map(|(id, atom)| {
            if atom.bonds.len() != 1 || !is_allowed_passivant(atom.atomic_number) {
                return None;
            }
            if tracked.contains(id) {
                return None; // a lattice atom, not a terminator
            }
            let host = atom.bonds[0].other_atom_id();
            if !tracked.contains(&host) {
                return None; // hosts are lattice atoms by construction
            }
            Some(Terminator {
                id: *id,
                host,
                position: atom.position,
                vdw: van_der_waals_radius(atom.atomic_number),
            })
        })
        .collect();
    terminators.sort_unstable_by_key(|t| t.id);
    terminators
}

/// Resolves concave-corner terminator clashes by dropping both terminators and
/// bonding their hosts.
///
/// `unpaired_surface_atoms` comes from [`super::surface_reconstruction::reconstruct_surface`]:
/// the {100} surface atoms that reconstruction classified, had enabled at their
/// own position, and still failed to pair. Requiring one of the two hosts to be
/// in that set is what makes this pass structurally safe rather than merely
/// well-tuned — see the note on gating below.
///
/// Returns the number of rebonds performed.
pub fn rebond_concave_clashes(
    structure: &mut AtomicStructure,
    atom_tracker: &PlacedAtomTracker,
    unpaired_surface_atoms: &FxHashSet<u32>,
    lattice_constant: f64,
) -> usize {
    // The gate (design §7). On an *unreconstructed* (100) face every adjacent
    // pair of surface atoms has terminators ~1.4 Å apart -- which is exactly
    // why ideal Si(100)-1x1 dihydride is unstable -- so an ungated clash rule
    // would silently dimerize the whole face, performing the reconstruction the
    // user turned off. The set is empty wherever reconstruction did not run, so
    // that cannot happen.
    if unpaired_surface_atoms.is_empty() {
        return 0;
    }

    let tracked: FxHashSet<u32> = atom_tracker.iter_atoms().map(|(_, id)| id).collect();
    let terminators = collect_terminators(structure, &tracked);
    if terminators.is_empty() {
        return 0;
    }

    let by_id: FxHashMap<u32, usize> = terminators
        .iter()
        .enumerate()
        .map(|(index, t)| (t.id, index))
        .collect();

    // Radius that cannot miss a pair: the widest terminator present, paired
    // with whichever one we are querying around.
    let max_vdw = terminators
        .iter()
        .map(|t| t.vdw)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_host_separation = HOST_SEPARATION_FACTOR * lattice_constant / 2.0_f64.sqrt();

    // --- Collect phase: read-only, so every distance below stays valid. ---
    //
    // Test 2 is applied by CONSTRUCTION, not as a filter: we seed only from
    // terminators whose host is unpaired, so `a.host` satisfies it by
    // definition. That is what keeps this loop cheap. The grid query is the
    // expensive part, and the unpaired set is tiny next to the terminator list
    // -- measured on a 61k-atom slab, 33 unpaired against ~10^4 terminators.
    // Seeding from every terminator instead made this pass ~20% of total fill
    // time; seeding from the unpaired ones leaves only the two O(n) scans above.
    //
    // It stays COMPLETE because an accepted pair needs at least one unpaired
    // host, so every such pair is reachable from whichever end is unpaired. A
    // pair with BOTH hosts unpaired is reachable from either end -- which is why
    // the old `other_id <= a.id` dedupe no longer works and `seen` replaces it.
    // (That case is real: two of the nine rebonds in the §10 fixture share a
    // host.)
    let mut seen: FxHashSet<(u32, u32)> = FxHashSet::default();
    let mut candidates: Vec<Candidate> = Vec::new();
    for a in terminators
        .iter()
        .filter(|t| unpaired_surface_atoms.contains(&t.host))
    {
        let search_radius = CLASH_FRACTION * (a.vdw + max_vdw);
        for other_id in structure.get_atoms_in_radius(&a.position, search_radius) {
            if other_id == a.id {
                continue;
            }
            let Some(&index) = by_id.get(&other_id) else {
                continue; // not a terminator
            };
            let b = &terminators[index];

            // Each unordered pair once. `get_atoms_in_radius` walks a hash grid,
            // so its order is not stable -- never let it decide anything.
            if !seen.insert((a.id.min(b.id), a.id.max(b.id))) {
                continue;
            }

            // Test 1: distinct hosts, not already bonded to each other.
            if a.host == b.host || structure.has_bond_between(a.host, b.host) {
                continue;
            }
            // Test 3: the terminators sterically clash.
            let distance = (a.position - b.position).length();
            if distance > CLASH_FRACTION * (a.vdw + b.vdw) {
                continue;
            }
            let (Some(host_a), Some(host_b)) =
                (structure.get_atom(a.host), structure.get_atom(b.host))
            else {
                continue;
            };
            let (host_a_pos, host_b_pos) = (host_a.position, host_b.position);
            // Test 4: the hosts are close enough to bond.
            if (host_a_pos - host_b_pos).length() > max_host_separation {
                continue;
            }
            // Test 5: the two dangling bonds face each other. Tests 4 and 5 are
            // purely geometric and so element-independent, which is what carries
            // the halogen case: with Cl/Br/I the terminator reaches ~0.6 A
            // further out while the test-3 threshold grows with its vdW radius,
            // and test 3 alone starts flagging ordinary contacts on a flat slab.
            // Do not "simplify" this criterion down to the distance test.
            let towards_b = (host_b_pos - host_a_pos).normalize();
            if (a.position - host_a_pos).normalize().dot(towards_b) < FACING_COS
                || (b.position - host_b_pos).normalize().dot(-towards_b) < FACING_COS
            {
                continue;
            }

            // Canonical orientation (lower terminator id first) so the sort
            // key below is exactly what the previous full-sweep loop produced.
            // Without this the greedy outcome could differ for equal-distance
            // pairs, purely because of which end we happened to seed from.
            let (lo, hi) = if a.id < b.id { (a, b) } else { (b, a) };
            candidates.push(Candidate {
                distance,
                terminator_a: lo.id,
                terminator_b: hi.id,
                host_a: lo.host,
                host_b: hi.host,
            });
        }
    }

    // Closest first, ties broken by id: the outcome must not depend on hash
    // iteration order (design §8).
    candidates.sort_by(|x, y| {
        x.distance
            .partial_cmp(&y.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(x.terminator_a.cmp(&y.terminator_a))
            .then(x.terminator_b.cmp(&y.terminator_b))
    });

    // --- Apply phase: greedy, one rebond per terminator. ---
    let mut consumed: FxHashSet<u32> = FxHashSet::default();
    let mut rebonds = 0;
    for candidate in candidates {
        if consumed.contains(&candidate.terminator_a) || consumed.contains(&candidate.terminator_b)
        {
            continue;
        }
        // An earlier rebond in this same loop may already have joined these two
        // hosts through a different terminator pair.
        if structure.has_bond_between(candidate.host_a, candidate.host_b) {
            continue;
        }

        structure.delete_atom(candidate.terminator_a);
        structure.delete_atom(candidate.terminator_b);
        structure.add_bond(candidate.host_a, candidate.host_b, 1);

        consumed.insert(candidate.terminator_a);
        consumed.insert(candidate.terminator_b);
        rebonds += 1;
    }

    rebonds
}
