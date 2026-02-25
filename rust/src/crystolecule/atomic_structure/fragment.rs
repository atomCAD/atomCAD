use std::collections::{HashSet, VecDeque};

use super::AtomicStructure;

/// Compute the fragment of atoms that are graph-theoretically closer to `moving_atom`
/// than to `reference_atom`. Uses BFS shortest path through the bond graph.
///
/// Returns a HashSet of atom IDs that should move (always includes `moving_atom`).
/// `reference_atom` is never included. Ties go to the fixed side.
/// Atoms unreachable from both (disconnected components) stay fixed.
pub fn compute_moving_fragment(
    structure: &AtomicStructure,
    moving_atom: u32,
    reference_atom: u32,
) -> HashSet<u32> {
    if moving_atom == reference_atom {
        return HashSet::new();
    }

    let dist_m = bfs_distances(structure, moving_atom);
    let dist_f = bfs_distances(structure, reference_atom);

    let mut fragment = HashSet::new();
    for (&atom_id, &dm) in &dist_m {
        let df = dist_f.get(&atom_id).copied().unwrap_or(u32::MAX);
        if dm < df {
            fragment.insert(atom_id);
        }
    }

    fragment
}

/// BFS from `start` through the bond graph, returning shortest path distances.
/// Only reachable atoms are included in the returned map.
fn bfs_distances(structure: &AtomicStructure, start: u32) -> rustc_hash::FxHashMap<u32, u32> {
    let mut distances = rustc_hash::FxHashMap::default();
    let mut queue = VecDeque::new();

    distances.insert(start, 0);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        let current_dist = distances[&current];

        if let Some(atom) = structure.get_atom(current) {
            for bond in &atom.bonds {
                let neighbor = bond.other_atom_id();
                if !distances.contains_key(&neighbor) {
                    distances.insert(neighbor, current_dist + 1);
                    queue.push_back(neighbor);
                }
            }
        }
    }

    distances
}

#[cfg(test)]
mod tests {
    // Tests are in rust/tests/crystolecule/fragment_test.rs
}
