use crate::crystolecule::atomic_constants::{ATOM_INFO, DEFAULT_ATOM_INFO};
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use crate::crystolecule::guided_placement::{
    Hybridization, TETRAHEDRAL_ANGLE, TRIGONAL_ANGLE, compute_sp3_case1_with_dihedral,
    count_active_neighbors, covalent_max_neighbors, detect_hybridization, find_dihedral_reference,
    gather_bond_directions, sp2_case2, sp3_case2, sp3_case3,
};
use glam::f64::DVec3;

// ============================================================================
// Options and result types
// ============================================================================

pub struct RemoveHydrogensOptions {
    /// Only remove H atoms that are themselves selected or bonded to a selected atom.
    pub selected_only: bool,
}

pub struct RemoveHydrogensResult {
    /// Number of hydrogen atoms removed.
    pub hydrogens_removed: usize,
}

/// Remove hydrogen atoms from the structure.
///
/// Two-phase approach: analyze immutably (collect IDs to remove), then mutate (delete atoms).
/// When `selected_only` is true, only hydrogen atoms that are themselves selected or bonded
/// to a selected atom are removed.
pub fn remove_hydrogens(
    structure: &mut AtomicStructure,
    options: &RemoveHydrogensOptions,
) -> RemoveHydrogensResult {
    // Phase 1: Analysis (immutable scan)
    let atom_ids: Vec<u32> = structure.atom_ids().copied().collect();
    let mut ids_to_remove: Vec<u32> = Vec::new();

    for &atom_id in &atom_ids {
        let atom = match structure.get_atom(atom_id) {
            Some(a) => a,
            None => continue,
        };

        if atom.atomic_number != 1 {
            continue;
        }

        if options.selected_only {
            let is_self_selected = atom.is_selected();
            let has_selected_neighbor =
                atom.bonds
                    .iter()
                    .filter(|b| !b.is_delete_marker())
                    .any(|b| {
                        structure
                            .get_atom(b.other_atom_id())
                            .is_some_and(|n| n.is_selected())
                    });
            if !is_self_selected && !has_selected_neighbor {
                continue;
            }
        }

        ids_to_remove.push(atom_id);
    }

    // Phase 2: Mutation (remove atoms)
    for &atom_id in &ids_to_remove {
        structure.delete_atom(atom_id);
    }

    RemoveHydrogensResult {
        hydrogens_removed: ids_to_remove.len(),
    }
}

pub struct AddHydrogensOptions {
    /// Only passivate atoms that are currently selected.
    pub selected_only: bool,
    /// Skip atoms already flagged as hydrogen-passivated.
    /// Default: true.
    pub skip_already_passivated: bool,
}

impl Default for AddHydrogensOptions {
    fn default() -> Self {
        Self {
            selected_only: false,
            skip_already_passivated: true,
        }
    }
}

pub struct AddHydrogensResult {
    /// Number of hydrogen atoms added.
    pub hydrogens_added: usize,
}

// ============================================================================
// X-H bond length table
// ============================================================================

/// Hardcoded table of common X-H bond lengths in Angstroms.
/// Rounded experimental values from Calculla bond length tables,
/// Wikipedia, and NIST CCCBDB.
const XH_BOND_LENGTHS: &[(i16, f64)] = &[
    (6, 1.09),  // C-H
    (7, 1.01),  // N-H
    (8, 0.96),  // O-H
    (14, 1.48), // Si-H
    (15, 1.42), // P-H
    (16, 1.34), // S-H
    (5, 1.19),  // B-H
    (32, 1.53), // Ge-H
];

/// Look up the X-H bond length for a given element.
/// Falls back to covalent_radius(X) + covalent_radius(H) if not in the table.
fn lookup_xh_bond_length(atomic_number: i16) -> f64 {
    for &(z, length) in XH_BOND_LENGTHS {
        if z == atomic_number {
            return length;
        }
    }
    // Fallback: sum of covalent radii
    let r_x = ATOM_INFO
        .get(&(atomic_number as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO)
        .covalent_radius;
    let r_h = ATOM_INFO
        .get(&1)
        .unwrap_or(&DEFAULT_ATOM_INFO)
        .covalent_radius;
    r_x + r_h
}

// ============================================================================
// Geometry: compute_open_directions
// ============================================================================

/// Pick a deterministic perpendicular vector to `v`.
fn arbitrary_perpendicular(v: DVec3) -> DVec3 {
    let ref_axis = if v.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    v.cross(ref_axis).normalize()
}

/// Compute open bond directions for placing hydrogen atoms.
///
/// Given the hybridization, existing bond directions, and the number of open slots,
/// returns unit direction vectors pointing from the atom toward where hydrogens
/// should be placed.
fn compute_open_directions(
    structure: &AtomicStructure,
    atom_id: u32,
    hybridization: Hybridization,
    existing_dirs: &[DVec3],
    needed: usize,
) -> Vec<DVec3> {
    match hybridization {
        Hybridization::Sp3 => {
            compute_open_directions_sp3(structure, atom_id, existing_dirs, needed)
        }
        Hybridization::Sp2 => compute_open_directions_sp2(existing_dirs, needed),
        Hybridization::Sp1 => compute_open_directions_sp1(existing_dirs, needed),
    }
}

fn compute_open_directions_sp3(
    structure: &AtomicStructure,
    atom_id: u32,
    existing_dirs: &[DVec3],
    needed: usize,
) -> Vec<DVec3> {
    let num_existing = existing_dirs.len();
    match num_existing {
        0 => {
            // Standard tetrahedral orientation
            let dirs = [
                DVec3::new(1.0, 1.0, 1.0).normalize(),
                DVec3::new(-1.0, -1.0, 1.0).normalize(),
                DVec3::new(-1.0, 1.0, -1.0).normalize(),
                DVec3::new(1.0, -1.0, -1.0).normalize(),
            ];
            dirs[..needed.min(4)].to_vec()
        }
        1 => {
            // Try dihedral reference first
            let bond_dir = existing_dirs[0];
            let neighbor_id = structure.get_atom(atom_id).and_then(|atom| {
                atom.bonds
                    .iter()
                    .find(|b| !b.is_delete_marker())
                    .map(|b| b.other_atom_id())
            });

            if let Some(neighbor_id) = neighbor_id {
                if let Some(ref_perp) = find_dihedral_reference(structure, atom_id, neighbor_id) {
                    // Use staggered (Primary) positions only
                    let dots =
                        compute_sp3_case1_with_dihedral(DVec3::ZERO, bond_dir, ref_perp, 1.0);
                    return dots
                        .iter()
                        .filter(|d| {
                            d.dot_type
                                == crate::crystolecule::guided_placement::GuideDotType::Primary
                        })
                        .take(needed)
                        .map(|d| d.position.normalize())
                        .collect();
                }
            }

            // Fallback: cone at 70.53° from -bond_dir with arbitrary reference
            let cone_axis = -bond_dir;
            let cone_half_angle = std::f64::consts::PI - TETRAHEDRAL_ANGLE;
            let cos_cone = cone_half_angle.cos();
            let sin_cone = cone_half_angle.sin();

            let u = arbitrary_perpendicular(bond_dir);
            let v = bond_dir.cross(u).normalize();

            let angles = [0.0_f64, 120.0, 240.0];
            angles[..needed.min(3)]
                .iter()
                .map(|&angle_deg| {
                    let angle = angle_deg.to_radians();
                    let (sin_a, cos_a) = angle.sin_cos();
                    let radial = u * cos_a + v * sin_a;
                    (cone_axis * cos_cone + radial * sin_cone).normalize()
                })
                .collect()
        }
        2 => {
            // Reuse sp3_case2 logic
            let dots = sp3_case2(DVec3::ZERO, existing_dirs, 1.0);
            dots.into_iter()
                .take(needed)
                .map(|d| d.position.normalize())
                .collect()
        }
        3 => {
            // Reuse sp3_case3 logic
            let dots = sp3_case3(DVec3::ZERO, existing_dirs, 1.0);
            dots.into_iter()
                .take(needed)
                .map(|d| d.position.normalize())
                .collect()
        }
        _ => vec![],
    }
}

fn compute_open_directions_sp2(existing_dirs: &[DVec3], needed: usize) -> Vec<DVec3> {
    let num_existing = existing_dirs.len();
    match num_existing {
        0 => {
            // Equilateral triangle in XY plane
            let dirs = [
                DVec3::X,
                DVec3::new(-0.5, 3.0_f64.sqrt() / 2.0, 0.0).normalize(),
                DVec3::new(-0.5, -(3.0_f64.sqrt()) / 2.0, 0.0).normalize(),
            ];
            dirs[..needed.min(3)].to_vec()
        }
        1 => {
            // Two directions at ±120° from existing bond in an arbitrary plane
            let bond_dir = existing_dirs[0];
            let perp = arbitrary_perpendicular(bond_dir);

            let cos_120 = TRIGONAL_ANGLE.cos();
            let sin_120 = TRIGONAL_ANGLE.sin();

            let d1 = (bond_dir * cos_120 + perp * sin_120).normalize();
            let d2 = (bond_dir * cos_120 - perp * sin_120).normalize();

            [d1, d2][..needed.min(2)].to_vec()
        }
        2 => {
            // Reuse sp2_case2 logic
            let dots = sp2_case2(DVec3::ZERO, existing_dirs, 1.0);
            dots.into_iter()
                .take(needed)
                .map(|d| d.position.normalize())
                .collect()
        }
        _ => vec![],
    }
}

fn compute_open_directions_sp1(existing_dirs: &[DVec3], needed: usize) -> Vec<DVec3> {
    let num_existing = existing_dirs.len();
    match num_existing {
        0 => {
            // Arbitrary axis
            let dirs = [DVec3::X, DVec3::NEG_X];
            dirs[..needed.min(2)].to_vec()
        }
        1 => {
            // Directly opposite
            vec![-existing_dirs[0]]
        }
        _ => vec![],
    }
}

// ============================================================================
// Main algorithm
// ============================================================================

/// Add hydrogen atoms to satisfy valence requirements of all undersaturated atoms.
///
/// Two-step approach: analyze immutably (collect placements), then mutate (add atoms/bonds).
pub fn add_hydrogens(
    structure: &mut AtomicStructure,
    options: &AddHydrogensOptions,
) -> AddHydrogensResult {
    // Step 1: Analysis (immutable scan)
    let atom_ids: Vec<u32> = structure.atom_ids().copied().collect();
    let mut placements: Vec<(u32, Vec<DVec3>)> = Vec::new();

    for &atom_id in &atom_ids {
        let atom = match structure.get_atom(atom_id) {
            Some(a) => a,
            None => continue,
        };

        // Skip atoms that should not be passivated
        if atom.atomic_number <= 0 {
            continue; // delete markers, parameters
        }
        if atom.atomic_number == 1 {
            continue; // don't passivate H itself
        }
        if options.selected_only && !atom.is_selected() {
            continue;
        }
        if options.skip_already_passivated && atom.is_hydrogen_passivation() {
            continue;
        }

        let atomic_number = atom.atomic_number;
        let position = atom.position;

        let hybridization = detect_hybridization(structure, atom_id, None);
        let max_bonds = covalent_max_neighbors(atomic_number, hybridization);
        let current = count_active_neighbors(structure, atom_id);
        if current >= max_bonds {
            continue;
        }
        let needed = max_bonds - current;

        let existing_dirs = gather_bond_directions(structure, atom);
        let h_bond_len = lookup_xh_bond_length(atomic_number);
        let open_dirs =
            compute_open_directions(structure, atom_id, hybridization, &existing_dirs, needed);
        let positions: Vec<DVec3> = open_dirs
            .iter()
            .map(|d| position + *d * h_bond_len)
            .collect();

        if !positions.is_empty() {
            placements.push((atom_id, positions));
        }
    }

    // Step 2: Mutation (add atoms and bonds)
    let mut h_count = 0;
    for (parent_id, positions) in placements {
        for pos in positions {
            let h_id = structure.add_atom(1, pos);
            structure.set_atom_hydrogen_passivation(h_id, true);
            structure.add_bond(parent_id, h_id, BOND_SINGLE);
            h_count += 1;
        }
    }

    AddHydrogensResult {
        hydrogens_added: h_count,
    }
}
