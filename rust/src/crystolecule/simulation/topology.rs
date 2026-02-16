// Molecular topology: enumerates bonded interactions from an AtomicStructure.
//
// Given the bond graph, this module builds lists of:
// - Bonds (1-2 interactions)
// - Angles (1-3 interactions)
// - Torsions (1-4 interactions)
// - Inversions (out-of-plane at sp2 centers)
//
// These interaction lists are consumed by force field implementations
// to compute energies and gradients.
//
// Topology enumeration logic ported from RDKit's Builder.cpp (BSD-3-Clause).

use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure::inline_bond::{BOND_AROMATIC, BOND_DOUBLE};
use rustc_hash::{FxHashMap, FxHashSet};

/// A bond interaction between two atoms.
#[derive(Debug, Clone)]
pub struct BondInteraction {
    /// Topology index of the first atom (idx1 < idx2).
    pub idx1: usize,
    /// Topology index of the second atom.
    pub idx2: usize,
    /// Bond order (BOND_SINGLE, BOND_DOUBLE, etc.).
    pub bond_order: u8,
}

/// An angle interaction between three atoms.
///
/// idx2 is the vertex (center) atom bonded to both idx1 and idx3.
#[derive(Debug, Clone)]
pub struct AngleInteraction {
    /// Topology index of the first end atom.
    pub idx1: usize,
    /// Topology index of the vertex (center) atom.
    pub idx2: usize,
    /// Topology index of the second end atom.
    pub idx3: usize,
}

/// A torsion (dihedral) interaction between four atoms.
///
/// The dihedral angle is measured around the central bond idx2-idx3.
#[derive(Debug, Clone)]
pub struct TorsionInteraction {
    /// Topology index of the first end atom.
    pub idx1: usize,
    /// Topology index of the first central bond atom.
    pub idx2: usize,
    /// Topology index of the second central bond atom.
    pub idx3: usize,
    /// Topology index of the second end atom.
    pub idx4: usize,
}

/// An inversion (out-of-plane) interaction at an sp2 center.
///
/// idx2 is the central atom (sp2 center) with exactly 3 neighbors.
/// idx1 and idx3 define the reference plane (together with idx2).
/// idx4 is the out-of-plane atom.
#[derive(Debug, Clone)]
pub struct InversionInteraction {
    /// Topology index of peripheral atom I.
    pub idx1: usize,
    /// Topology index of central atom J (sp2 center).
    pub idx2: usize,
    /// Topology index of peripheral atom K.
    pub idx3: usize,
    /// Topology index of peripheral atom L (out-of-plane atom).
    pub idx4: usize,
}

/// A nonbonded (van der Waals) pair interaction.
#[derive(Debug, Clone)]
pub struct NonbondedPairInteraction {
    /// Topology index of the first atom (idx1 < idx2).
    pub idx1: usize,
    /// Topology index of the second atom.
    pub idx2: usize,
}

/// Molecular topology: interaction lists extracted from a bond graph.
///
/// All atom references use topology indices (0-based, contiguous), not atom IDs.
/// Use `atom_ids` to map back to the original AtomicStructure atom IDs.
pub struct MolecularTopology {
    /// Number of atoms in the topology.
    pub num_atoms: usize,
    /// Maps topology index -> atom_id in the original AtomicStructure.
    pub atom_ids: Vec<u32>,
    /// Atomic number for each atom (indexed by topology index).
    pub atomic_numbers: Vec<i16>,
    /// Flat array of positions: [x0, y0, z0, x1, y1, z1, ...].
    pub positions: Vec<f64>,
    /// Bond (1-2) interactions.
    pub bonds: Vec<BondInteraction>,
    /// Angle (1-3) interactions.
    pub angles: Vec<AngleInteraction>,
    /// Torsion (1-4) interactions.
    pub torsions: Vec<TorsionInteraction>,
    /// Inversion (out-of-plane) interactions.
    pub inversions: Vec<InversionInteraction>,
    /// Nonbonded (1-4+) pair interactions for van der Waals.
    pub nonbonded_pairs: Vec<NonbondedPairInteraction>,
}

impl MolecularTopology {
    /// Builds a molecular topology from an AtomicStructure.
    ///
    /// Enumerates all bonded interactions:
    /// - Bonds: one per unique bond in the structure
    /// - Angles: all i-j-k triples where j is bonded to both i and k
    /// - Torsions: all i-j-k-l chains where j-k is a bond, i is bonded to j, l is bonded to k
    /// - Inversions: 3 permutations per sp2 center with exactly 3 bonds
    ///
    /// Delete markers (atomic_number == 0) and deleted bonds are excluded.
    pub fn from_structure(structure: &AtomicStructure) -> Self {
        // Step 1: Build atom index mapping (atom_id -> topology index)
        let mut atom_ids = Vec::new();
        let mut atomic_numbers = Vec::new();
        let mut positions = Vec::new();
        let mut id_to_idx: FxHashMap<u32, usize> = FxHashMap::default();

        for (_, atom) in structure.iter_atoms() {
            if atom.atomic_number == 0 {
                continue;
            }
            let idx = atom_ids.len();
            id_to_idx.insert(atom.id, idx);
            atom_ids.push(atom.id);
            atomic_numbers.push(atom.atomic_number);
            positions.push(atom.position.x);
            positions.push(atom.position.y);
            positions.push(atom.position.z);
        }

        let num_atoms = atom_ids.len();

        // Step 2: Build adjacency list and enumerate bonds
        // Each entry: (neighbor_topology_index, bond_order)
        let mut neighbors: Vec<Vec<(usize, u8)>> = vec![Vec::new(); num_atoms];
        let mut bonds = Vec::new();

        for (_, atom) in structure.iter_atoms() {
            if atom.atomic_number == 0 {
                continue;
            }
            let Some(&idx_i) = id_to_idx.get(&atom.id) else {
                continue;
            };

            for bond in &atom.bonds {
                if bond.is_delete_marker() {
                    continue;
                }
                let other_id = bond.other_atom_id();
                let Some(&idx_j) = id_to_idx.get(&other_id) else {
                    continue;
                };

                // Process each bond once (when idx_i < idx_j)
                if idx_i < idx_j {
                    neighbors[idx_i].push((idx_j, bond.bond_order()));
                    neighbors[idx_j].push((idx_i, bond.bond_order()));
                    bonds.push(BondInteraction {
                        idx1: idx_i,
                        idx2: idx_j,
                        bond_order: bond.bond_order(),
                    });
                }
            }
        }

        // Sort neighbor lists for deterministic ordering
        for n in &mut neighbors {
            n.sort_by_key(|&(idx, _)| idx);
        }

        // Step 3: Enumerate angles
        let angles = Self::enumerate_angles(&neighbors);

        // Step 4: Enumerate torsions
        let torsions = Self::enumerate_torsions(&bonds, &neighbors);

        // Step 5: Enumerate inversions
        let inversions = Self::enumerate_inversions(&neighbors, &atomic_numbers);

        // Step 6: Enumerate nonbonded (1-4+) pairs
        let nonbonded_pairs = Self::enumerate_nonbonded_pairs(num_atoms, &bonds, &angles);

        MolecularTopology {
            num_atoms,
            atom_ids,
            atomic_numbers,
            positions,
            bonds,
            angles,
            torsions,
            inversions,
            nonbonded_pairs,
        }
    }

    /// Enumerates all angle interactions.
    ///
    /// For each atom as the vertex, creates angle interactions for all pairs
    /// of its neighbors. The neighbor with the smaller index is always idx1.
    fn enumerate_angles(neighbors: &[Vec<(usize, u8)>]) -> Vec<AngleInteraction> {
        let mut angles = Vec::new();

        for (vertex, nbrs) in neighbors.iter().enumerate() {
            for i in 0..nbrs.len() {
                for j in (i + 1)..nbrs.len() {
                    angles.push(AngleInteraction {
                        idx1: nbrs[i].0,
                        idx2: vertex,
                        idx3: nbrs[j].0,
                    });
                }
            }
        }

        angles
    }

    /// Enumerates all torsion interactions.
    ///
    /// For each bond (j, k) as the central bond, creates torsion interactions
    /// for all combinations of neighbor i of j (i != k) and neighbor l of k (l != j).
    /// Skips degenerate torsions where i == l (can happen in 3-membered rings).
    fn enumerate_torsions(
        bonds: &[BondInteraction],
        neighbors: &[Vec<(usize, u8)>],
    ) -> Vec<TorsionInteraction> {
        let mut torsions = Vec::new();

        for bond in bonds {
            let j = bond.idx1;
            let k = bond.idx2;

            for &(i, _) in &neighbors[j] {
                if i == k {
                    continue;
                }
                for &(l, _) in &neighbors[k] {
                    if l == j {
                        continue;
                    }
                    // Skip degenerate torsions in 3-membered rings
                    if i == l {
                        continue;
                    }
                    torsions.push(TorsionInteraction {
                        idx1: i,
                        idx2: j,
                        idx3: k,
                        idx4: l,
                    });
                }
            }
        }

        torsions
    }

    /// Enumerates all inversion (out-of-plane) interactions.
    ///
    /// For each sp2-like center with exactly 3 bonds, creates 3 permutations
    /// (one for each neighbor as the out-of-plane atom).
    ///
    /// Inversion centers are determined by:
    /// - C (6), N (7), O (8): only if the atom has at least one double or aromatic bond
    /// - Group 15 — P (15), As (33), Sb (51), Bi (83): always (pyramidal inversion)
    fn enumerate_inversions(
        neighbors: &[Vec<(usize, u8)>],
        atomic_numbers: &[i16],
    ) -> Vec<InversionInteraction> {
        let mut inversions = Vec::new();

        for (center, nbrs) in neighbors.iter().enumerate() {
            if nbrs.len() != 3 {
                continue;
            }

            if !Self::is_inversion_center(atomic_numbers[center], nbrs) {
                continue;
            }

            let n0 = nbrs[0].0;
            let n1 = nbrs[1].0;
            let n2 = nbrs[2].0;

            // 3 permutations: each neighbor takes a turn as the out-of-plane atom (idx4).
            // This matches RDKit's Builder.cpp enumeration order.
            inversions.push(InversionInteraction {
                idx1: n0,
                idx2: center,
                idx3: n1,
                idx4: n2,
            });
            inversions.push(InversionInteraction {
                idx1: n0,
                idx2: center,
                idx3: n2,
                idx4: n1,
            });
            inversions.push(InversionInteraction {
                idx1: n1,
                idx2: center,
                idx3: n2,
                idx4: n0,
            });
        }

        inversions
    }

    /// Enumerates all nonbonded (1-4+) pair interactions.
    ///
    /// Builds an exclusion set of 1-2 (bond) and 1-3 (angle endpoint) pairs,
    /// then includes every atom pair (i < j) not in the exclusion set.
    fn enumerate_nonbonded_pairs(
        num_atoms: usize,
        bonds: &[BondInteraction],
        angles: &[AngleInteraction],
    ) -> Vec<NonbondedPairInteraction> {
        let mut exclusions: FxHashSet<(usize, usize)> = FxHashSet::default();

        // Exclude 1-2 pairs (directly bonded)
        for bond in bonds {
            let key = (bond.idx1.min(bond.idx2), bond.idx1.max(bond.idx2));
            exclusions.insert(key);
        }

        // Exclude 1-3 pairs (angle endpoints)
        for angle in angles {
            let key = (angle.idx1.min(angle.idx3), angle.idx1.max(angle.idx3));
            exclusions.insert(key);
        }

        let mut pairs = Vec::new();
        for i in 0..num_atoms {
            for j in (i + 1)..num_atoms {
                if !exclusions.contains(&(i, j)) {
                    pairs.push(NonbondedPairInteraction { idx1: i, idx2: j });
                }
            }
        }
        pairs
    }

    /// Determines if an atom should be an inversion center based on its
    /// atomic number and bond orders.
    ///
    /// Rules (matching RDKit's Builder.cpp):
    /// - C (6), N (7), O (8): only if sp2-like (has at least one double or aromatic bond)
    /// - Group 15 — P (15), As (33), Sb (51), Bi (83): always (pyramidal inversion)
    fn is_inversion_center(atomic_number: i16, bonds: &[(usize, u8)]) -> bool {
        match atomic_number {
            // C, N, O: inversion only for sp2 centers
            6 | 7 | 8 => bonds
                .iter()
                .any(|&(_, order)| order == BOND_DOUBLE || order == BOND_AROMATIC),
            // Group 15: pyramidal inversion regardless of bond orders
            15 | 33 | 51 | 83 => true,
            _ => false,
        }
    }
}
