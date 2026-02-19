use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::simulation::uff::params::{calc_bond_rest_length, get_uff_params};
use crate::crystolecule::simulation::uff::typer::{assign_uff_type, hybridization_from_label};
use glam::f64::DVec3;

// ============================================================================
// Core types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hybridization {
    Sp3,
    Sp2,
    Sp1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondMode {
    Covalent,
    Dative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondLengthMode {
    Crystal,
    Uff,
}

#[derive(Debug, Clone)]
pub struct GuideDot {
    pub position: DVec3,
    pub dot_type: GuideDotType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideDotType {
    Primary,
    Secondary,
}

#[derive(Debug, Clone)]
pub struct GuidedPlacementResult {
    pub anchor_atom_id: u32,
    pub hybridization: Hybridization,
    pub guide_dots: Vec<GuideDot>,
    pub bond_distance: f64,
    pub remaining_slots: usize,
    /// True when geometric max > covalent max (atom has lone pairs / empty orbitals)
    pub has_additional_geometric_capacity: bool,
}

// ============================================================================
// Crystal bond length table
// ============================================================================

/// Hardcoded table of sp3 semiconductor crystal bond lengths.
/// Key: (min(Z_a, Z_b), max(Z_a, Z_b)). Values in Angstroms.
/// Derived from zinc blende / diamond cubic unit cell parameter `a`
/// via `bond_length = a * sqrt(3) / 4`.
const CRYSTAL_BOND_LENGTHS: &[((i16, i16), f64)] = &[
    ((6, 6), 1.545),    // Diamond C-C
    ((14, 14), 2.352),  // Silicon Si-Si
    ((6, 14), 1.889),   // 3C-SiC
    ((32, 32), 2.450),  // Germanium Ge-Ge
    ((50, 50), 2.810),  // alpha-Sn
    ((5, 7), 1.567),    // c-BN
    ((5, 15), 1.966),   // BP
    ((7, 13), 1.897),   // AlN (zinc blende)
    ((13, 15), 2.367),  // AlP
    ((13, 33), 2.443),  // AlAs
    ((7, 31), 1.946),   // GaN (zinc blende)
    ((15, 31), 2.360),  // GaP
    ((31, 33), 2.448),  // GaAs
    ((15, 49), 2.541),  // InP
    ((33, 49), 2.623),  // InAs
    ((49, 51), 2.806),  // InSb
    ((16, 30), 2.342),  // ZnS (zinc blende)
    ((30, 34), 2.454),  // ZnSe
    ((30, 52), 2.637),  // ZnTe
    ((48, 52), 2.806),  // CdTe
];

fn crystal_bond_length(z_a: i16, z_b: i16) -> Option<f64> {
    let key = (z_a.min(z_b), z_a.max(z_b));
    CRYSTAL_BOND_LENGTHS
        .iter()
        .find(|&&(k, _)| k == key)
        .map(|&(_, v)| v)
}

// ============================================================================
// Hybridization detection
// ============================================================================

/// Detect hybridization for an atom, using an explicit override if provided,
/// otherwise auto-detecting via UFF type assignment.
pub fn detect_hybridization(
    structure: &AtomicStructure,
    atom_id: u32,
    hybridization_override: Option<Hybridization>,
) -> Hybridization {
    if let Some(h) = hybridization_override {
        return h;
    }

    let atom = match structure.get_atom(atom_id) {
        Some(a) => a,
        None => return Hybridization::Sp3,
    };

    match assign_uff_type(atom.atomic_number, &atom.bonds) {
        Ok(label) => {
            let hyb = hybridization_from_label(label);
            match hyb {
                1 => Hybridization::Sp1,
                2 => Hybridization::Sp2,
                3 => Hybridization::Sp3,
                _ => Hybridization::Sp3, // fallback
            }
        }
        Err(_) => Hybridization::Sp3, // fallback
    }
}

// ============================================================================
// Saturation check
// ============================================================================

/// Returns the maximum number of neighbors for the given element, hybridization,
/// and bond mode.
pub fn effective_max_neighbors(
    atomic_number: i16,
    hybridization: Hybridization,
    bond_mode: BondMode,
) -> usize {
    let geometric_max = match hybridization {
        Hybridization::Sp3 => 4,
        Hybridization::Sp2 => 3,
        Hybridization::Sp1 => 2,
    };

    if bond_mode == BondMode::Dative {
        return geometric_max;
    }

    // Covalent mode: element-specific limits
    match atomic_number {
        // Group 14: C, Si, Ge, Sn — full tetrahedral
        6 | 14 | 32 | 50 => geometric_max,
        // Group 15: N, P, As, Sb
        7 | 15 | 33 | 51 => match hybridization {
            Hybridization::Sp3 => 3,
            Hybridization::Sp2 => 3,
            Hybridization::Sp1 => 2,
        },
        // Group 16: O, S, Se, Te
        8 | 16 | 34 | 52 => match hybridization {
            Hybridization::Sp3 => 2,
            Hybridization::Sp2 => 2,
            Hybridization::Sp1 => 2,
        },
        // Halogens: F, Cl, Br, I
        9 | 17 | 35 | 53 => 1,
        // Boron, Aluminum
        5 | 13 => match hybridization {
            Hybridization::Sp2 => 3,
            Hybridization::Sp3 => geometric_max,
            Hybridization::Sp1 => 2,
        },
        // Noble gases
        2 | 10 | 18 | 36 | 54 | 86 => 0,
        // Hydrogen
        1 => 1,
        // Default: use geometric max
        _ => geometric_max,
    }
}

/// Count active (non-deleted) bonds on an atom.
fn count_active_neighbors(structure: &AtomicStructure, atom_id: u32) -> usize {
    match structure.get_atom(atom_id) {
        Some(atom) => atom
            .bonds
            .iter()
            .filter(|b| !b.is_delete_marker())
            .count(),
        None => 0,
    }
}

/// Returns the number of remaining bonding slots for an atom.
pub fn remaining_slots(
    structure: &AtomicStructure,
    atom_id: u32,
    hybridization: Hybridization,
    bond_mode: BondMode,
) -> usize {
    let atom = match structure.get_atom(atom_id) {
        Some(a) => a,
        None => return 0,
    };
    let max = effective_max_neighbors(atom.atomic_number, hybridization, bond_mode);
    let current = count_active_neighbors(structure, atom_id);
    max.saturating_sub(current)
}

// ============================================================================
// Bond distance computation
// ============================================================================

/// Default UFF type label for an element (bare atom with no bonds).
fn default_uff_type_for_element(atomic_number: i16) -> &'static str {
    // Use assign_uff_type with empty bonds to get the default type
    match assign_uff_type(atomic_number, &[]) {
        Ok(label) => label,
        Err(_) => "X_", // should never happen for valid elements
    }
}

/// Compute bond distance between anchor and new atom.
pub fn bond_distance(
    anchor_atomic_number: i16,
    new_atomic_number: i16,
    anchor_uff_label: &str,
    bond_length_mode: BondLengthMode,
) -> f64 {
    match bond_length_mode {
        BondLengthMode::Crystal => {
            if let Some(d) = crystal_bond_length(anchor_atomic_number, new_atomic_number) {
                return d;
            }
            // Fall back to UFF
            compute_uff_bond_distance(anchor_uff_label, new_atomic_number)
        }
        BondLengthMode::Uff => compute_uff_bond_distance(anchor_uff_label, new_atomic_number),
    }
}

fn compute_uff_bond_distance(anchor_uff_label: &str, new_atomic_number: i16) -> f64 {
    let new_uff_label = default_uff_type_for_element(new_atomic_number);
    let params_a = get_uff_params(anchor_uff_label);
    let params_b = get_uff_params(new_uff_label);
    match (params_a, params_b) {
        (Some(pa), Some(pb)) => calc_bond_rest_length(1.0, pa, pb),
        _ => 1.5, // fallback
    }
}

// ============================================================================
// sp3 candidate position computation
// ============================================================================

/// Tetrahedral angle in radians: arccos(-1/3) ≈ 109.47°
const TETRAHEDRAL_ANGLE: f64 = 1.9106332362490186;

/// Compute sp3 candidate positions for guided placement.
///
/// - Case 4 (saturated): empty vec
/// - Case 3 (1 remaining): single dot opposite centroid of existing bonds
/// - Case 2 (2 remaining): two dots symmetric about the existing bond plane
/// - Case 1/0: empty vec (stubs for Phase B/C)
pub fn compute_sp3_candidates(
    anchor_pos: DVec3,
    existing_bond_dirs: &[DVec3],
    bond_dist: f64,
) -> Vec<GuideDot> {
    match existing_bond_dirs.len() {
        4.. => vec![], // saturated
        3 => sp3_case3(anchor_pos, existing_bond_dirs, bond_dist),
        2 => sp3_case2(anchor_pos, existing_bond_dirs, bond_dist),
        _ => vec![], // case 1 and case 0: stubs for Phase B/C
    }
}

/// sp3 case 3: one remaining direction, opposite the centroid of existing bonds.
fn sp3_case3(anchor_pos: DVec3, dirs: &[DVec3], bond_dist: f64) -> Vec<GuideDot> {
    let sum = dirs[0] + dirs[1] + dirs[2];
    let d4 = if sum.length_squared() < 1e-12 {
        // Degenerate: all three bonds cancel out. Pick any perpendicular direction.
        let arb = if dirs[0].x.abs() < 0.9 {
            DVec3::X
        } else {
            DVec3::Y
        };
        arb.cross(dirs[0]).normalize()
    } else {
        (-sum).normalize()
    };
    vec![GuideDot {
        position: anchor_pos + d4 * bond_dist,
        dot_type: GuideDotType::Primary,
    }]
}

/// sp3 case 2: two remaining directions, symmetric about the plane of existing bonds.
fn sp3_case2(anchor_pos: DVec3, dirs: &[DVec3], bond_dist: f64) -> Vec<GuideDot> {
    let b1 = dirs[0];
    let b2 = dirs[1];

    let mid = (b1 + b2).normalize_or_zero();
    let n = b1.cross(b2);

    if n.length_squared() < 1e-12 || mid.length_squared() < 1e-12 {
        // Degenerate: bonds are parallel or anti-parallel
        // Pick perpendicular directions
        let arb = if b1.x.abs() < 0.9 {
            DVec3::X
        } else {
            DVec3::Y
        };
        let perp1 = b1.cross(arb).normalize();
        let perp2 = b1.cross(perp1).normalize();
        return vec![
            GuideDot {
                position: anchor_pos + perp1 * bond_dist,
                dot_type: GuideDotType::Primary,
            },
            GuideDot {
                position: anchor_pos + perp2 * bond_dist,
                dot_type: GuideDotType::Primary,
            },
        ];
    }

    let n = n.normalize();
    let neg_mid = -mid;

    // Find angle a such that dot(b1, d) = cos(109.47°)
    // d = -mid * cos(a) + n * sin(a)
    // dot(b1, d) = -dot(b1,mid)*cos(a) + dot(b1,n)*sin(a)
    // dot(b1, n) = 0 (n is perpendicular to b1 and b2)
    // So: -dot(b1,mid)*cos(a) = cos(109.47°)
    // cos(a) = -cos(109.47°) / dot(b1, mid)
    let cos_tet = TETRAHEDRAL_ANGLE.cos(); // cos(109.47°) ≈ -1/3
    let b1_dot_mid = b1.dot(mid);

    if b1_dot_mid.abs() < 1e-12 {
        // mid is perpendicular to b1 — shouldn't happen with valid sp3 bonds
        return vec![];
    }

    let cos_a = -cos_tet / b1_dot_mid;
    let cos_a = cos_a.clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).sqrt();

    let d1 = (neg_mid * cos_a + n * sin_a).normalize();
    let d2 = (neg_mid * cos_a - n * sin_a).normalize();

    vec![
        GuideDot {
            position: anchor_pos + d1 * bond_dist,
            dot_type: GuideDotType::Primary,
        },
        GuideDot {
            position: anchor_pos + d2 * bond_dist,
            dot_type: GuideDotType::Primary,
        },
    ]
}

// ============================================================================
// Top-level entry point
// ============================================================================

/// Compute guided placement information for placing a new atom bonded to an anchor.
pub fn compute_guided_placement(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    new_element_atomic_number: i16,
    hybridization_override: Option<Hybridization>,
    bond_mode: BondMode,
    bond_length_mode: BondLengthMode,
) -> GuidedPlacementResult {
    let hybridization = detect_hybridization(structure, anchor_atom_id, hybridization_override);

    let anchor_atom = structure.get_atom(anchor_atom_id).unwrap();
    let anchor_pos = anchor_atom.position;
    let anchor_atomic_number = anchor_atom.atomic_number;

    // Compute remaining slots
    let slots = remaining_slots(structure, anchor_atom_id, hybridization, bond_mode);
    let covalent_max =
        effective_max_neighbors(anchor_atomic_number, hybridization, BondMode::Covalent);
    let geometric_max =
        effective_max_neighbors(anchor_atomic_number, hybridization, BondMode::Dative);
    let has_additional = geometric_max > covalent_max;

    // Get anchor's UFF label for bond distance computation
    let anchor_uff_label = assign_uff_type(anchor_atomic_number, &anchor_atom.bonds)
        .unwrap_or(default_uff_type_for_element(anchor_atomic_number));

    let bond_dist = bond_distance(
        anchor_atomic_number,
        new_element_atomic_number,
        anchor_uff_label,
        bond_length_mode,
    );

    // Compute existing bond directions (normalized)
    let existing_bond_dirs: Vec<DVec3> = anchor_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .filter_map(|b| {
            structure.get_atom(b.other_atom_id()).map(|neighbor| {
                let dir = neighbor.position - anchor_pos;
                if dir.length_squared() < 1e-12 {
                    DVec3::X // degenerate
                } else {
                    dir.normalize()
                }
            })
        })
        .collect();

    // Dispatch to geometry computation based on hybridization
    // Only compute guide dots if there are remaining slots
    let guide_dots = if slots == 0 {
        vec![]
    } else {
        match hybridization {
            Hybridization::Sp3 => {
                compute_sp3_candidates(anchor_pos, &existing_bond_dirs, bond_dist)
            }
            Hybridization::Sp2 | Hybridization::Sp1 => {
                // Stubs for Phase D
                vec![]
            }
        }
    };

    GuidedPlacementResult {
        anchor_atom_id,
        hybridization,
        guide_dots,
        bond_distance: bond_dist,
        remaining_slots: slots,
        has_additional_geometric_capacity: has_additional,
    }
}
