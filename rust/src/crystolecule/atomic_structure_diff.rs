//! Diff application algorithm for atomic structures.
//!
//! Applies a diff (an `AtomicStructure` with `is_diff = true`) to a base structure,
//! producing a new result structure. The diff can add, delete, replace, and move atoms,
//! as well as add, delete, and override bonds.
//!
//! This lives in the crystolecule module because diff application is a fundamental
//! operation on atomic structures, not specific to any particular node type.

use crate::crystolecule::atomic_structure::AtomicStructure;
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

        if diff_atom.is_delete_marker() {
            // Matched delete marker → base atom is removed
            deleted_base_ids.insert(m.base_id);
            stats.atoms_deleted += 1;
        } else {
            // Matched normal atom → replacement/move
            // Use the diff atom's position (which may differ from base for moves)
            let result_id = result.add_atom(diff_atom.atomic_number, diff_atom.position);
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
            continue;
        }

        let result_id = result.add_atom(diff_atom.atomic_number, diff_atom.position);
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
