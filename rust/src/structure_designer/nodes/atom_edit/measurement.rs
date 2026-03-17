use crate::crystolecule::atomic_structure::AtomicStructure;
use glam::f64::DVec3;

/// Result of a selection-based measurement.
#[derive(Debug, Clone)]
pub enum MeasurementResult {
    /// Distance between 2 atoms in Angstroms.
    Distance { distance: f64 },
    /// Angle at the vertex atom, in degrees.
    Angle {
        angle_degrees: f64,
        /// Index into the input atom list indicating which atom is the vertex (0, 1, or 2).
        vertex_index: usize,
    },
    /// Dihedral (torsion) angle around the B-C axis, in degrees.
    Dihedral {
        angle_degrees: f64,
        /// Indices into the input atom list for the chain A-B-C-D.
        chain: [usize; 4],
    },
}

/// Holds the position and result-space atom ID for a selected atom.
#[derive(Debug, Clone, Copy)]
pub struct SelectedAtomInfo {
    pub result_atom_id: u32,
    pub position: DVec3,
}

/// Compute a measurement from 2-4 selected atoms.
///
/// Uses bonding information from `result_structure` to determine atom roles
/// (angle vertex, dihedral chain), falling back to geometric heuristics when
/// bonding is ambiguous.
///
/// Returns `None` if fewer than 2 or more than 4 atoms are provided.
pub fn compute_measurement(
    atoms: &[SelectedAtomInfo],
    result_structure: &AtomicStructure,
) -> Option<MeasurementResult> {
    match atoms.len() {
        2 => Some(compute_distance(atoms)),
        3 => Some(compute_angle(atoms, result_structure)),
        4 => Some(compute_dihedral(atoms, result_structure)),
        _ => None,
    }
}

fn compute_distance(atoms: &[SelectedAtomInfo]) -> MeasurementResult {
    let d = atoms[0].position.distance(atoms[1].position);
    MeasurementResult::Distance { distance: d }
}

fn compute_angle(atoms: &[SelectedAtomInfo], structure: &AtomicStructure) -> MeasurementResult {
    let vertex_index = find_angle_vertex(atoms, structure);
    let (arm_a, arm_b) = match vertex_index {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };

    let v = atoms[vertex_index].position;
    let va = (atoms[arm_a].position - v).normalize();
    let vb = (atoms[arm_b].position - v).normalize();

    let cos_angle = va.dot(vb).clamp(-1.0, 1.0);
    let angle_degrees = cos_angle.acos().to_degrees();

    MeasurementResult::Angle {
        angle_degrees,
        vertex_index,
    }
}

/// Determine which of the 3 atoms is the angle vertex.
///
/// 1. If exactly one atom is bonded to both others, it's the vertex.
/// 2. Otherwise, the two most distant atoms are the arms; the remaining one is the vertex.
fn find_angle_vertex(atoms: &[SelectedAtomInfo], structure: &AtomicStructure) -> usize {
    let ids = [
        atoms[0].result_atom_id,
        atoms[1].result_atom_id,
        atoms[2].result_atom_id,
    ];

    // Compute degree of each atom in the 3-atom bond subgraph
    let mut degree = [0u8; 3];
    for i in 0..3 {
        for j in (i + 1)..3 {
            if structure.has_bond_between(ids[i], ids[j]) {
                degree[i] += 1;
                degree[j] += 1;
            }
        }
    }

    // If exactly one atom has degree 2 (bonded to both others), it's the vertex
    let degree_2_count = degree.iter().filter(|&&d| d == 2).count();
    if degree_2_count == 1 {
        return degree.iter().position(|&d| d == 2).unwrap();
    }

    // Geometric fallback: most distant pair = arms, remaining atom = vertex
    find_most_distant_complement(atoms)
}

fn compute_dihedral(atoms: &[SelectedAtomInfo], structure: &AtomicStructure) -> MeasurementResult {
    let chain = find_dihedral_chain(atoms, structure);

    let a = atoms[chain[0]].position;
    let b = atoms[chain[1]].position;
    let c = atoms[chain[2]].position;
    let d = atoms[chain[3]].position;

    let b1 = b - a;
    let b2 = c - b;
    let b3 = d - c;

    let n1 = b1.cross(b2);
    let n2 = b2.cross(b3);

    let angle_degrees = if n1.length_squared() < 1e-20 || n2.length_squared() < 1e-20 {
        // Degenerate case: collinear atoms
        0.0
    } else {
        let n1 = n1.normalize();
        let n2 = n2.normalize();
        let m1 = n1.cross(b2.normalize());
        let x = n1.dot(n2);
        let y = m1.dot(n2);
        (-y).atan2(x).to_degrees()
    };

    MeasurementResult::Dihedral {
        angle_degrees,
        chain,
    }
}

/// Determine the A-B-C-D chain for dihedral angle measurement.
///
/// 1. If the bond subgraph has degree sequence [1,1,2,2], it's a chain:
///    degree-1 atoms are ends (A,D), degree-2 atoms are center (B,C).
/// 2. Otherwise: most distant pair = ends (A,D), other two = center (B,C).
fn find_dihedral_chain(atoms: &[SelectedAtomInfo], structure: &AtomicStructure) -> [usize; 4] {
    let ids = [
        atoms[0].result_atom_id,
        atoms[1].result_atom_id,
        atoms[2].result_atom_id,
        atoms[3].result_atom_id,
    ];

    // Compute degrees in the 4-atom bond subgraph
    let mut degree = [0u8; 4];
    let mut bonded = [[false; 4]; 4];
    for i in 0..4 {
        for j in (i + 1)..4 {
            if structure.has_bond_between(ids[i], ids[j]) {
                degree[i] += 1;
                degree[j] += 1;
                bonded[i][j] = true;
                bonded[j][i] = true;
            }
        }
    }

    // Check for degree sequence [1,1,2,2] (a bonded 4-chain)
    let mut sorted_degrees = degree;
    sorted_degrees.sort();
    if sorted_degrees == [1, 1, 2, 2] {
        // Find the two ends (degree 1) and two center atoms (degree 2)
        let mut ends = [0usize; 2];
        let mut center = [0usize; 2];
        let mut ei = 0;
        let mut ci = 0;
        for i in 0..4 {
            if degree[i] == 1 {
                ends[ei] = i;
                ei += 1;
            } else {
                center[ci] = i;
                ci += 1;
            }
        }

        // Trace the chain: A is bonded to one of the center atoms (B), D to the other (C)
        let (b, c) = if bonded[ends[0]][center[0]] {
            (center[0], center[1])
        } else {
            (center[1], center[0])
        };

        return [ends[0], b, c, ends[1]];
    }

    // Geometric fallback: most distant pair = ends (A, D), other two = center (B, C)
    let (end_a, end_d) = find_most_distant_pair(atoms);

    // The other two atoms
    let mut center = [0usize; 2];
    let mut ci = 0;
    for i in 0..4 {
        if i != end_a && i != end_d {
            center[ci] = i;
            ci += 1;
        }
    }

    // Assign B closer to A, C closer to D (for consistent orientation)
    let dist_b0_a = atoms[center[0]].position.distance(atoms[end_a].position);
    let dist_b1_a = atoms[center[1]].position.distance(atoms[end_a].position);
    let (b, c) = if dist_b0_a <= dist_b1_a {
        (center[0], center[1])
    } else {
        (center[1], center[0])
    };

    [end_a, b, c, end_d]
}

/// Find the pair of atoms with the maximum distance. Returns their indices.
fn find_most_distant_pair(atoms: &[SelectedAtomInfo]) -> (usize, usize) {
    let mut max_dist_sq = -1.0f64;
    let mut best = (0, 1);
    for i in 0..atoms.len() {
        for j in (i + 1)..atoms.len() {
            let d = atoms[i].position.distance_squared(atoms[j].position);
            if d > max_dist_sq {
                max_dist_sq = d;
                best = (i, j);
            }
        }
    }
    best
}

/// Find the atom that is NOT part of the most distant pair.
/// Used for 3-atom angle: the remaining atom is the vertex.
fn find_most_distant_complement(atoms: &[SelectedAtomInfo]) -> usize {
    let (a, b) = find_most_distant_pair(atoms);
    // Return the first index that is neither a nor b
    for i in 0..atoms.len() {
        if i != a && i != b {
            return i;
        }
    }
    // Should never reach here if atoms.len() > complement_count
    0
}
