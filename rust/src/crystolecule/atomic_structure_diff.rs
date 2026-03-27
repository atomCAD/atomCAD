//! Diff application algorithm for atomic structures.
//!
//! Applies a diff (an `AtomicStructure` with `is_diff = true`) to a base structure,
//! producing a new result structure. The diff can add, delete, replace, and move atoms,
//! as well as add, delete, and override bonds.
//!
//! This lives in the crystolecule module because diff application is a fundamental
//! operation on atomic structures, not specific to any particular node type.

use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure::UNCHANGED_ATOMIC_NUMBER;
use crate::crystolecule::atomic_structure::inline_bond::BOND_DELETED;
use rustc_hash::FxHashMap;

/// Result of applying a diff to a base structure.
#[derive(Debug, Clone)]
pub struct DiffApplicationResult {
    /// The resulting atomic structure (always has `is_diff = false`).
    pub result: AtomicStructure,
    /// Maps every result atom to its origin (base or diff).
    pub provenance: DiffProvenance,
    /// Summary statistics of the diff application.
    pub stats: DiffStats,
}

/// Tracks the origin of every atom in the result structure.
#[derive(Debug, Clone)]
pub struct DiffProvenance {
    /// result_atom_id → where it came from
    pub sources: FxHashMap<u32, AtomSource>,
    /// base_atom_id → result_atom_id (for base pass-throughs and matched atoms)
    pub base_to_result: FxHashMap<u32, u32>,
    /// diff_atom_id → result_atom_id (for diff atoms present in result)
    pub diff_to_result: FxHashMap<u32, u32>,
}

/// Describes where a result atom originated from.
#[derive(Debug, Clone, PartialEq)]
pub enum AtomSource {
    /// Base atom NOT touched by the diff (pass-through).
    BasePassthrough(u32),
    /// Diff atom that matched a base atom (replacement or move).
    DiffMatchedBase { diff_id: u32, base_id: u32 },
    /// Diff atom with no base match (new addition).
    DiffAdded(u32),
}

/// Summary statistics of a diff application.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffStats {
    pub atoms_added: u32,
    pub atoms_deleted: u32,
    pub atoms_modified: u32,
    pub bonds_added: u32,
    pub bonds_deleted: u32,
    /// Anchored diff atoms whose base atom no longer exists (skipped).
    pub orphaned_tracked_atoms: u32,
    /// Delete markers that found no base atom to delete (no-op).
    pub unmatched_delete_markers: u32,
    /// Diff bonds where one or both endpoints were missing from the result (skipped).
    pub orphaned_bonds: u32,
    /// UNCHANGED markers that matched a base atom (bond endpoint references).
    pub unchanged_references: u32,
}

/// Internal: a matched pair between a diff atom and a base atom.
struct DiffMatch {
    diff_id: u32,
    base_id: u32,
}

/// Applies a diff to a base structure, producing a new result.
///
/// # Arguments
/// * `base` - The base atomic structure to apply the diff to.
/// * `diff` - The diff structure (`is_diff` should be `true`). Contains atoms to add,
///   delete markers (atomic_number = 0), replacements, and moves (via anchor positions).
/// * `tolerance` - Positional matching tolerance in Angstroms. Diff atoms are matched to
///   base atoms within this distance. Default: 0.1 A.
///
/// # Algorithm
/// 1. Match diff atoms to base atoms by position (greedy nearest-first).
/// 2. Apply atom effects: additions, deletions, replacements, moves.
/// 3. Resolve bonds using the two-step algorithm (base pass-through + new diff bonds).
/// 4. Return the result with provenance and stats.
pub fn apply_diff(
    base: &AtomicStructure,
    diff: &AtomicStructure,
    tolerance: f64,
) -> DiffApplicationResult {
    let tolerance_sq = tolerance * tolerance;

    // Step 1: Match diff atoms to base atoms
    let (matches, unmatched_diff_ids) = match_diff_atoms(base, diff, tolerance_sq);

    // Build lookup maps from the matching
    let mut base_to_diff: FxHashMap<u32, u32> = FxHashMap::default();
    for m in &matches {
        base_to_diff.insert(m.base_id, m.diff_id);
    }

    // Step 2: Build the result structure and track provenance + stats
    let mut result = AtomicStructure::new();
    let mut provenance = DiffProvenance {
        sources: FxHashMap::default(),
        base_to_result: FxHashMap::default(),
        diff_to_result: FxHashMap::default(),
    };
    let mut stats = DiffStats::default();

    // Track which base atoms are matched (to know pass-throughs)
    // Also track base atoms that are deleted (matched by a delete marker)
    let mut deleted_base_ids: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();

    // Process matched diff atoms first
    for m in &matches {
        let diff_atom = diff.get_atom(m.diff_id).unwrap();
        let base_atom = base.get_atom(m.base_id).unwrap();

        if diff_atom.is_delete_marker() {
            // Matched delete marker → base atom is removed
            deleted_base_ids.insert(m.base_id);
            stats.atoms_deleted += 1;
        } else if diff_atom.is_unchanged_marker() {
            // Matched UNCHANGED marker → base atom passes through unchanged
            // but we still record the mapping so bond resolution works
            let result_id = result.add_atom(base_atom.atomic_number, base_atom.position);
            result.copy_atom_metadata(result_id, base_atom);
            provenance.sources.insert(
                result_id,
                AtomSource::DiffMatchedBase {
                    diff_id: m.diff_id,
                    base_id: m.base_id,
                },
            );
            provenance.base_to_result.insert(m.base_id, result_id);
            provenance.diff_to_result.insert(m.diff_id, result_id);
            stats.unchanged_references += 1;
        } else {
            // Matched normal atom → replacement/move
            // Use the diff atom's position (which may differ from base for moves)
            let result_id = result.add_atom(diff_atom.atomic_number, diff_atom.position);
            result.copy_atom_metadata(result_id, diff_atom);
            provenance.sources.insert(
                result_id,
                AtomSource::DiffMatchedBase {
                    diff_id: m.diff_id,
                    base_id: m.base_id,
                },
            );
            provenance.base_to_result.insert(m.base_id, result_id);
            provenance.diff_to_result.insert(m.diff_id, result_id);
            stats.atoms_modified += 1;
        }
    }

    // Process unmatched diff atoms (additions)
    for &diff_id in &unmatched_diff_ids {
        let diff_atom = diff.get_atom(diff_id).unwrap();

        if diff_atom.is_delete_marker() {
            // Unmatched delete marker → no-op (trying to delete something that doesn't exist)
            stats.unmatched_delete_markers += 1;
            continue;
        }

        if diff_atom.is_unchanged_marker() {
            // Unmatched UNCHANGED marker → base atom no longer exists.
            // Drop this reference (and any bonds attached to it).
            stats.orphaned_tracked_atoms += 1;
            continue;
        }

        if diff.has_anchor_position(diff_id) {
            // Tracked atom whose base atom no longer exists → skip.
            // Anchored diff atoms were created to match/modify a specific base atom.
            // If the anchor doesn't match any base atom, the base was deleted upstream
            // and this tracked atom should disappear with it.
            // Only genuinely added atoms (no anchor) survive as additions.
            stats.orphaned_tracked_atoms += 1;
            continue;
        }

        let result_id = result.add_atom(diff_atom.atomic_number, diff_atom.position);
        result.copy_atom_metadata(result_id, diff_atom);
        provenance
            .sources
            .insert(result_id, AtomSource::DiffAdded(diff_id));
        provenance.diff_to_result.insert(diff_id, result_id);
        stats.atoms_added += 1;
    }

    // Process unmatched base atoms (pass-throughs)
    for (_, base_atom) in base.iter_atoms() {
        if base_to_diff.contains_key(&base_atom.id) {
            // This base atom was matched by a diff atom — already handled above
            continue;
        }
        // Not matched and not deleted → pass through
        let result_id = result.add_atom(base_atom.atomic_number, base_atom.position);
        result.copy_atom_metadata(result_id, base_atom);
        provenance
            .sources
            .insert(result_id, AtomSource::BasePassthrough(base_atom.id));
        provenance.base_to_result.insert(base_atom.id, result_id);
    }

    // Step 3: Resolve bonds
    resolve_bonds(
        base,
        diff,
        &mut result,
        &base_to_diff,
        &deleted_base_ids,
        &provenance,
        &mut stats,
    );

    DiffApplicationResult {
        result,
        provenance,
        stats,
    }
}

/// Greedy nearest-first matching of diff atoms to base atoms.
///
/// For each diff atom, determines the match position (anchor if available, else atom position),
/// then finds the nearest unmatched base atom within tolerance_sq. Processes diff atoms in
/// order of closest match distance first to minimize ambiguity.
fn match_diff_atoms(
    base: &AtomicStructure,
    diff: &AtomicStructure,
    tolerance_sq: f64,
) -> (Vec<DiffMatch>, Vec<u32>) {
    // Collect all diff atoms with their match positions and best distances
    struct DiffCandidate {
        diff_id: u32,
        match_pos: glam::f64::DVec3,
        best_dist_sq: f64,
        best_base_id: Option<u32>,
    }

    let mut candidates: Vec<DiffCandidate> = Vec::new();

    for (_, diff_atom) in diff.iter_atoms() {
        let match_pos = diff
            .anchor_position(diff_atom.id)
            .copied()
            .unwrap_or(diff_atom.position);

        // Find nearest base atom to match position using spatial grid
        let nearby = base.get_atoms_in_radius(&match_pos, tolerance_sq.sqrt());
        let mut best_dist_sq = f64::MAX;
        let mut best_base_id = None;

        for &base_id in &nearby {
            if let Some(base_atom) = base.get_atom(base_id) {
                let dist_sq = match_pos.distance_squared(base_atom.position);
                if dist_sq <= tolerance_sq && dist_sq < best_dist_sq {
                    best_dist_sq = dist_sq;
                    best_base_id = Some(base_id);
                }
            }
        }

        candidates.push(DiffCandidate {
            diff_id: diff_atom.id,
            match_pos: match_pos,
            best_dist_sq,
            best_base_id,
        });
    }

    // Sort by best match distance (closest first) for greedy assignment
    candidates.sort_by(|a, b| a.best_dist_sq.partial_cmp(&b.best_dist_sq).unwrap());

    // Greedy assignment
    let mut matched_base_ids: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();
    let mut matches: Vec<DiffMatch> = Vec::new();
    let mut unmatched_diff_ids: Vec<u32> = Vec::new();

    for candidate in &candidates {
        if let Some(base_id) = candidate.best_base_id {
            if !matched_base_ids.contains(&base_id) {
                // Claim this base atom
                matched_base_ids.insert(base_id);
                matches.push(DiffMatch {
                    diff_id: candidate.diff_id,
                    base_id,
                });
                continue;
            }
        }

        // Best match was already claimed or no match at all — re-search excluding claimed atoms
        let nearby = base.get_atoms_in_radius(&candidate.match_pos, tolerance_sq.sqrt());
        let mut found = false;
        let mut best_dist_sq = f64::MAX;
        let mut best_base_id = 0u32;

        for &base_id in &nearby {
            if matched_base_ids.contains(&base_id) {
                continue;
            }
            if let Some(base_atom) = base.get_atom(base_id) {
                let dist_sq = candidate.match_pos.distance_squared(base_atom.position);
                if dist_sq <= tolerance_sq && dist_sq < best_dist_sq {
                    best_dist_sq = dist_sq;
                    best_base_id = base_id;
                    found = true;
                }
            }
        }

        if found {
            matched_base_ids.insert(best_base_id);
            matches.push(DiffMatch {
                diff_id: candidate.diff_id,
                base_id: best_base_id,
            });
        } else {
            unmatched_diff_ids.push(candidate.diff_id);
        }
    }

    (matches, unmatched_diff_ids)
}

/// Resolves bonds using the two-step algorithm from the design doc.
///
/// Step 3a: Base bond pass-through — iterate base bonds, copy/override/delete.
/// Step 3b: New diff bonds — iterate diff bonds not already processed.
#[allow(clippy::too_many_arguments)]
fn resolve_bonds(
    base: &AtomicStructure,
    diff: &AtomicStructure,
    result: &mut AtomicStructure,
    base_to_diff: &FxHashMap<u32, u32>,
    deleted_base_ids: &rustc_hash::FxHashSet<u32>,
    provenance: &DiffProvenance,
    stats: &mut DiffStats,
) {
    // Track which diff bond pairs have been processed (to avoid double-processing in step 3b)
    let mut processed_diff_bond_pairs: rustc_hash::FxHashSet<(u32, u32)> =
        rustc_hash::FxHashSet::default();

    // Step 3a: Base bond pass-through
    for (_, base_atom) in base.iter_atoms() {
        for bond in &base_atom.bonds {
            let base_b_id = bond.other_atom_id();

            // Only process each bond once (atom_a.id < atom_b.id)
            if base_atom.id >= base_b_id {
                continue;
            }

            // If either endpoint was deleted, skip this bond
            if deleted_base_ids.contains(&base_atom.id) || deleted_base_ids.contains(&base_b_id) {
                stats.bonds_deleted += 1;
                continue;
            }

            // Map base atoms to result atoms
            let result_a = provenance.base_to_result.get(&base_atom.id);
            let result_b = provenance.base_to_result.get(&base_b_id);

            let (Some(&result_a_id), Some(&result_b_id)) = (result_a, result_b) else {
                // One or both base atoms not in result (shouldn't happen if not deleted, but be safe)
                continue;
            };

            // Check if both base atoms were matched by diff atoms
            let diff_a = base_to_diff.get(&base_atom.id);
            let diff_b = base_to_diff.get(&base_b_id);

            match (diff_a, diff_b) {
                (Some(&diff_a_id), Some(&diff_b_id)) => {
                    // Both matched by diff atoms — check if diff has a bond between them
                    let diff_bond = find_bond_between(diff, diff_a_id, diff_b_id);
                    let canonical = canonical_pair(diff_a_id, diff_b_id);
                    processed_diff_bond_pairs.insert(canonical);

                    match diff_bond {
                        Some(order) if order == BOND_DELETED => {
                            // Explicit deletion
                            stats.bonds_deleted += 1;
                        }
                        Some(order) => {
                            // Override with diff bond
                            result.add_bond(result_a_id, result_b_id, order);
                        }
                        None => {
                            // No diff bond → base bond survives by default
                            result.add_bond(result_a_id, result_b_id, bond.bond_order());
                        }
                    }
                }
                _ => {
                    // At most one matched → base bond survives unchanged
                    result.add_bond(result_a_id, result_b_id, bond.bond_order());
                }
            }
        }
    }

    // Step 3b: New diff bonds (not already processed)
    for (_, diff_atom) in diff.iter_atoms() {
        for bond in &diff_atom.bonds {
            let diff_b_id = bond.other_atom_id();

            // Only process each bond once
            if diff_atom.id >= diff_b_id {
                continue;
            }

            let canonical = canonical_pair(diff_atom.id, diff_b_id);
            if processed_diff_bond_pairs.contains(&canonical) {
                continue;
            }

            // Skip delete markers for bonds that don't exist in the base
            if bond.bond_order() == BOND_DELETED {
                continue;
            }

            // Map diff atoms to result atoms
            let result_a = provenance.diff_to_result.get(&diff_atom.id);
            let result_b = provenance.diff_to_result.get(&diff_b_id);

            if let (Some(&result_a_id), Some(&result_b_id)) = (result_a, result_b) {
                result.add_bond(result_a_id, result_b_id, bond.bond_order());
                stats.bonds_added += 1;
            } else {
                // One or both endpoints missing from result (skipped/orphaned)
                stats.orphaned_bonds += 1;
            }
        }
    }
}

/// Find the bond order between two atoms in a structure, if a bond exists.
fn find_bond_between(structure: &AtomicStructure, atom_a: u32, atom_b: u32) -> Option<u8> {
    if let Some(atom) = structure.get_atom(atom_a) {
        for bond in &atom.bonds {
            if bond.other_atom_id() == atom_b {
                return Some(bond.bond_order());
            }
        }
    }
    None
}

/// Returns a canonical (min, max) pair for deduplication.
fn canonical_pair(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

/// Enriches a diff structure with base bonds between matched atom pairs.
///
/// When viewing a diff, atoms that were moved/replaced appear without their base bonds
/// (since the bond wasn't explicitly changed and thus isn't in the diff). This function
/// copies base bonds into the diff clone where both endpoints are matched by diff atoms
/// and the diff doesn't already have a bond between them.
///
/// This modifies the diff in place (intended for use on a clone, not the stored diff).
pub fn enrich_diff_with_base_bonds(
    diff: &mut AtomicStructure,
    base: &AtomicStructure,
    tolerance: f64,
) {
    let tolerance_sq = tolerance * tolerance;
    let (matches, _) = match_diff_atoms(base, diff, tolerance_sq);

    // Build base_to_diff map
    let mut base_to_diff: FxHashMap<u32, u32> = FxHashMap::default();
    for m in &matches {
        base_to_diff.insert(m.base_id, m.diff_id);
    }

    // Collect bonds to add (to avoid borrowing issues with base iteration)
    let mut bonds_to_add: Vec<(u32, u32, u8)> = Vec::new();

    for (_, base_atom) in base.iter_atoms() {
        for bond in &base_atom.bonds {
            let base_b_id = bond.other_atom_id();
            // Only process each bond once
            if base_atom.id >= base_b_id {
                continue;
            }

            if let (Some(&diff_a_id), Some(&diff_b_id)) = (
                base_to_diff.get(&base_atom.id),
                base_to_diff.get(&base_b_id),
            ) {
                // Both endpoints matched — check if diff already has a bond between them
                let existing = find_bond_between(diff, diff_a_id, diff_b_id);
                if existing.is_none() {
                    bonds_to_add.push((diff_a_id, diff_b_id, bond.bond_order()));
                }
            }
        }
    }

    for (a, b, order) in bonds_to_add {
        diff.add_bond(a, b, order);
    }
}

// ============================================================================
// Diff Composition
// ============================================================================

/// Result of composing two diffs.
#[derive(Debug, Clone)]
pub struct DiffCompositionResult {
    /// The composed diff (is_diff = true).
    pub composed: AtomicStructure,
    /// Statistics about the composition.
    pub stats: DiffCompositionStats,
}

/// Statistics about a diff composition operation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffCompositionStats {
    /// diff1 atoms carried through (not touched by diff2).
    pub diff1_passthrough: u32,
    /// diff2 atoms carried through (not matching any diff1 atom).
    pub diff2_passthrough: u32,
    /// Matched pairs where effects were composed.
    pub composed_pairs: u32,
    /// Cancellations (diff1 add + diff2 delete).
    pub cancellations: u32,
}

/// Composes two diffs into a single diff.
///
/// The composed diff, when applied to any base, produces the same result
/// as applying diff1 then diff2:
///   apply_diff(apply_diff(base, diff1), diff2) == apply_diff(base, composed)
///
/// Both inputs must have is_diff = true.
pub fn compose_two_diffs(
    diff1: &AtomicStructure,
    diff2: &AtomicStructure,
    tolerance: f64,
) -> DiffCompositionResult {
    let tolerance_sq = tolerance * tolerance;
    let mut stats = DiffCompositionStats::default();

    // Step 1: Match diff2 atoms against diff1 non-delete-marker atoms.
    // Build a temporary structure from diff1's matchable atoms (non-delete-markers)
    // so we can use spatial grid matching.
    let mut diff1_matchable = AtomicStructure::new();
    // Maps: matchable_id → original diff1_id
    let mut matchable_to_diff1: FxHashMap<u32, u32> = FxHashMap::default();

    for (_, atom) in diff1.iter_atoms() {
        if atom.is_delete_marker() {
            continue; // Delete markers are excluded from matching
        }
        // Use atom.position — this is where the atom appears in the result of apply_diff(_, diff1).
        // For modified atoms: position is the new (moved) position.
        // For pure additions: position is where it was added.
        // For unchanged markers: position matches the base atom's position.
        let matchable_id = diff1_matchable.add_atom(atom.atomic_number, atom.position);
        matchable_to_diff1.insert(matchable_id, atom.id);
    }

    // Match diff2 atoms against the matchable diff1 atoms
    let (matches, unmatched_diff2_ids) = match_diff_atoms(&diff1_matchable, diff2, tolerance_sq);

    // Build lookup: diff1_id → diff2_id and diff2_id → diff1_id
    let mut diff1_to_diff2: FxHashMap<u32, u32> = FxHashMap::default();
    let mut diff2_to_diff1: FxHashMap<u32, u32> = FxHashMap::default();

    for m in &matches {
        let diff1_id = matchable_to_diff1[&m.base_id];
        diff1_to_diff2.insert(diff1_id, m.diff_id);
        diff2_to_diff1.insert(m.diff_id, diff1_id);
    }

    // Track cancelled diff1 atom IDs (pure addition + delete)
    let mut cancelled_diff1_ids: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();
    let mut cancelled_diff2_ids: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();

    // Step 2: Build the composed diff.
    // ID ordering invariant: diff1-origin atoms get lower IDs, diff2-origin atoms get higher IDs.
    let mut composed = AtomicStructure::new_diff();

    // Maps from original diff IDs to composed IDs (needed for bond resolution)
    let mut diff1_to_composed: FxHashMap<u32, u32> = FxHashMap::default();
    let mut diff2_to_composed: FxHashMap<u32, u32> = FxHashMap::default();

    // --- Pass 1: diff1-origin atoms (lower IDs) ---

    // Process matched pairs (diff1 side)
    for (_, diff1_atom) in diff1.iter_atoms() {
        if let Some(&diff2_id) = diff1_to_diff2.get(&diff1_atom.id) {
            let diff2_atom = diff2.get_atom(diff2_id).unwrap();
            let diff1_is_delete = diff1_atom.is_delete_marker();
            let diff1_is_unchanged = diff1_atom.is_unchanged_marker();
            let diff1_has_anchor = diff1.has_anchor_position(diff1_atom.id);
            let diff1_is_pure_addition =
                !diff1_has_anchor && !diff1_is_delete && !diff1_is_unchanged;
            let diff2_is_delete = diff2_atom.is_delete_marker();
            let diff2_is_unchanged = diff2_atom.is_unchanged_marker();
            let diff2_is_modify = !diff2_is_delete && !diff2_is_unchanged;

            // diff1 delete markers are excluded from matching, so diff1_is_delete should not
            // occur here. But handle defensively:
            if diff1_is_delete {
                // Should not happen since delete markers are excluded from matching
                let cid = composed.add_atom(diff1_atom.atomic_number, diff1_atom.position);
                if let Some(&anchor) = diff1.anchor_position(diff1_atom.id) {
                    composed.set_anchor_position(cid, anchor);
                }
                diff1_to_composed.insert(diff1_atom.id, cid);
                stats.diff1_passthrough += 1;
                continue;
            }

            if diff1_is_unchanged {
                // diff1 unchanged marker: match_pos is anchor or position
                let match_pos = diff1
                    .anchor_position(diff1_atom.id)
                    .copied()
                    .unwrap_or(diff1_atom.position);

                if diff2_is_modify {
                    // Unchanged + modify → atom at diff2 position, anchor = diff1 match_pos
                    let cid = composed.add_atom(diff2_atom.atomic_number, diff2_atom.position);
                    composed.copy_atom_metadata(cid, diff2_atom);
                    composed.set_anchor_position(cid, match_pos);
                    diff1_to_composed.insert(diff1_atom.id, cid);
                    diff2_to_composed.insert(diff2_id, cid);
                    stats.composed_pairs += 1;
                } else if diff2_is_delete {
                    // Unchanged + delete → delete marker with anchor at match_pos
                    let cid = composed.add_atom(
                        crate::crystolecule::atomic_structure::DELETED_SITE_ATOMIC_NUMBER,
                        match_pos,
                    );
                    composed.set_anchor_position(cid, match_pos);
                    diff1_to_composed.insert(diff1_atom.id, cid);
                    diff2_to_composed.insert(diff2_id, cid);
                    stats.composed_pairs += 1;
                } else {
                    // Unchanged + unchanged → unchanged marker at match_pos
                    let cid = composed.add_atom(UNCHANGED_ATOMIC_NUMBER, match_pos);
                    diff1_to_composed.insert(diff1_atom.id, cid);
                    diff2_to_composed.insert(diff2_id, cid);
                    stats.composed_pairs += 1;
                }
            } else if diff1_is_pure_addition {
                if diff2_is_modify {
                    // Pure addition + modify → pure addition at diff2 position, diff2 element/flags
                    let cid = composed.add_atom(diff2_atom.atomic_number, diff2_atom.position);
                    composed.copy_atom_metadata(cid, diff2_atom);
                    // No anchor (still a pure addition)
                    diff1_to_composed.insert(diff1_atom.id, cid);
                    diff2_to_composed.insert(diff2_id, cid);
                    stats.composed_pairs += 1;
                } else if diff2_is_delete {
                    // Pure addition + delete → CANCELLATION
                    cancelled_diff1_ids.insert(diff1_atom.id);
                    cancelled_diff2_ids.insert(diff2_id);
                    stats.cancellations += 1;
                } else {
                    // Pure addition + unchanged → diff1 atom as-is
                    let cid = composed.add_atom(diff1_atom.atomic_number, diff1_atom.position);
                    composed.copy_atom_metadata(cid, diff1_atom);
                    diff1_to_composed.insert(diff1_atom.id, cid);
                    diff2_to_composed.insert(diff2_id, cid);
                    stats.composed_pairs += 1;
                }
            } else {
                // diff1 is modified (has anchor)
                let diff1_anchor = diff1.anchor_position(diff1_atom.id).copied().unwrap();

                if diff2_is_modify {
                    // Modified + modify → diff2 position, diff1 anchor, diff2 element/flags
                    let cid = composed.add_atom(diff2_atom.atomic_number, diff2_atom.position);
                    composed.copy_atom_metadata(cid, diff2_atom);
                    composed.set_anchor_position(cid, diff1_anchor);
                    diff1_to_composed.insert(diff1_atom.id, cid);
                    diff2_to_composed.insert(diff2_id, cid);
                    stats.composed_pairs += 1;
                } else if diff2_is_delete {
                    // Modified + delete → delete marker with diff1's anchor
                    let cid = composed.add_atom(
                        crate::crystolecule::atomic_structure::DELETED_SITE_ATOMIC_NUMBER,
                        diff1_anchor,
                    );
                    composed.set_anchor_position(cid, diff1_anchor);
                    diff1_to_composed.insert(diff1_atom.id, cid);
                    diff2_to_composed.insert(diff2_id, cid);
                    stats.composed_pairs += 1;
                } else {
                    // Modified + unchanged → diff1 atom as-is
                    let cid = composed.add_atom(diff1_atom.atomic_number, diff1_atom.position);
                    composed.copy_atom_metadata(cid, diff1_atom);
                    composed.set_anchor_position(cid, diff1_anchor);
                    diff1_to_composed.insert(diff1_atom.id, cid);
                    diff2_to_composed.insert(diff2_id, cid);
                    stats.composed_pairs += 1;
                }
            }
        } else {
            // Unmatched diff1 atom → copy as-is
            let cid = composed.add_atom(diff1_atom.atomic_number, diff1_atom.position);
            composed.copy_atom_metadata(cid, diff1_atom);
            if let Some(&anchor) = diff1.anchor_position(diff1_atom.id) {
                composed.set_anchor_position(cid, anchor);
            }
            diff1_to_composed.insert(diff1_atom.id, cid);
            stats.diff1_passthrough += 1;
        }
    }

    // --- Pass 2: diff2-origin atoms (higher IDs) ---
    for &diff2_id in &unmatched_diff2_ids {
        let diff2_atom = diff2.get_atom(diff2_id).unwrap();
        let cid = composed.add_atom(diff2_atom.atomic_number, diff2_atom.position);
        composed.copy_atom_metadata(cid, diff2_atom);
        if let Some(&anchor) = diff2.anchor_position(diff2_id) {
            composed.set_anchor_position(cid, anchor);
        }
        diff2_to_composed.insert(diff2_id, cid);
        stats.diff2_passthrough += 1;
    }

    // Step 3: Compose bonds.
    compose_bonds(
        diff1,
        diff2,
        &mut composed,
        &diff1_to_composed,
        &diff2_to_composed,
        &diff1_to_diff2,
        &diff2_to_diff1,
        &cancelled_diff1_ids,
        &cancelled_diff2_ids,
    );

    DiffCompositionResult { composed, stats }
}

/// Compose bonds from diff1 and diff2 into the composed diff.
///
/// Pass A: diff1 bonds — check if diff2 overrides.
/// Pass B: diff2 bonds not yet processed — add to composed.
#[allow(clippy::too_many_arguments)]
fn compose_bonds(
    diff1: &AtomicStructure,
    diff2: &AtomicStructure,
    composed: &mut AtomicStructure,
    diff1_to_composed: &FxHashMap<u32, u32>,
    diff2_to_composed: &FxHashMap<u32, u32>,
    diff1_to_diff2: &FxHashMap<u32, u32>,
    _diff2_to_diff1: &FxHashMap<u32, u32>,
    cancelled_diff1_ids: &rustc_hash::FxHashSet<u32>,
    cancelled_diff2_ids: &rustc_hash::FxHashSet<u32>,
) {
    let mut processed_diff2_bond_pairs: rustc_hash::FxHashSet<(u32, u32)> =
        rustc_hash::FxHashSet::default();

    // Pass A: diff1 bonds
    for (_, diff1_atom) in diff1.iter_atoms() {
        for bond in &diff1_atom.bonds {
            let diff1_b_id = bond.other_atom_id();

            // Only process each bond once
            if diff1_atom.id >= diff1_b_id {
                continue;
            }

            // If either endpoint was cancelled, skip the bond
            if cancelled_diff1_ids.contains(&diff1_atom.id)
                || cancelled_diff1_ids.contains(&diff1_b_id)
            {
                continue;
            }

            // Map to composed IDs
            let composed_a = diff1_to_composed.get(&diff1_atom.id);
            let composed_b = diff1_to_composed.get(&diff1_b_id);

            let (Some(&composed_a_id), Some(&composed_b_id)) = (composed_a, composed_b) else {
                continue;
            };

            // Check if both endpoints are matched by diff2 atoms
            let diff2_a = diff1_to_diff2.get(&diff1_atom.id);
            let diff2_b = diff1_to_diff2.get(&diff1_b_id);

            match (diff2_a, diff2_b) {
                (Some(&diff2_a_id), Some(&diff2_b_id)) => {
                    // Both matched — check if diff2 has a bond between them
                    let diff2_bond = find_bond_between(diff2, diff2_a_id, diff2_b_id);
                    let canonical = canonical_pair(diff2_a_id, diff2_b_id);
                    processed_diff2_bond_pairs.insert(canonical);

                    match diff2_bond {
                        Some(order) => {
                            // diff2 overrides (including BOND_DELETED)
                            composed.add_bond(composed_a_id, composed_b_id, order);
                        }
                        None => {
                            // No diff2 bond → diff1 bond survives
                            composed.add_bond(composed_a_id, composed_b_id, bond.bond_order());
                        }
                    }
                }
                _ => {
                    // At most one matched → diff1 bond survives
                    composed.add_bond(composed_a_id, composed_b_id, bond.bond_order());
                }
            }
        }
    }

    // Pass B: diff2 bonds not yet processed
    for (_, diff2_atom) in diff2.iter_atoms() {
        for bond in &diff2_atom.bonds {
            let diff2_b_id = bond.other_atom_id();

            if diff2_atom.id >= diff2_b_id {
                continue;
            }

            let canonical = canonical_pair(diff2_atom.id, diff2_b_id);
            if processed_diff2_bond_pairs.contains(&canonical) {
                continue;
            }

            // If either endpoint was cancelled, skip
            if cancelled_diff2_ids.contains(&diff2_atom.id)
                || cancelled_diff2_ids.contains(&diff2_b_id)
            {
                continue;
            }

            // Map to composed IDs
            let composed_a = diff2_to_composed.get(&diff2_atom.id);
            let composed_b = diff2_to_composed.get(&diff2_b_id);

            if let (Some(&composed_a_id), Some(&composed_b_id)) = (composed_a, composed_b) {
                composed.add_bond(composed_a_id, composed_b_id, bond.bond_order());
            }
            // If either endpoint missing from composed, skip (orphaned)
        }
    }
}

/// Composes a sequence of diffs (left fold of compose_two_diffs).
///
/// Returns None if the slice is empty. Returns a clone of the single diff
/// if the slice has length 1.
pub fn compose_diffs(diffs: &[&AtomicStructure], tolerance: f64) -> Option<DiffCompositionResult> {
    match diffs.len() {
        0 => None,
        1 => Some(DiffCompositionResult {
            composed: diffs[0].clone(),
            stats: DiffCompositionStats {
                diff1_passthrough: diffs[0].get_num_of_atoms() as u32,
                ..Default::default()
            },
        }),
        _ => {
            let mut result = compose_two_diffs(diffs[0], diffs[1], tolerance);
            for diff in &diffs[2..] {
                result = compose_two_diffs(&result.composed, diff, tolerance);
            }
            Some(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_pair() {
        assert_eq!(canonical_pair(3, 5), (3, 5));
        assert_eq!(canonical_pair(5, 3), (3, 5));
        assert_eq!(canonical_pair(2, 2), (2, 2));
    }
}
