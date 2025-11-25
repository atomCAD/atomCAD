use crate::crystolecule::atomic_structure::{AtomicStructure, Atom, BondReference};
use crate::crystolecule::motif::Motif;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::crystolecule_constants::{
  DEFAULT_ZINCBLENDE_MOTIF, DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
  ZINCBLENDE_SITE_CORNER, ZINCBLENDE_SITE_FACE_Z, ZINCBLENDE_SITE_FACE_Y, ZINCBLENDE_SITE_FACE_X,
  ZINCBLENDE_SITE_INTERIOR1, ZINCBLENDE_SITE_INTERIOR2, ZINCBLENDE_SITE_INTERIOR3, ZINCBLENDE_SITE_INTERIOR4
};
use crate::crystolecule::atomic_structure_utils::remove_single_bond_atoms;
use crate::crystolecule::atomic_constants::{
  DEBUG_CARBON_GRAY, DEBUG_CARBON_RED, DEBUG_CARBON_GREEN, DEBUG_CARBON_BLUE,
  DEBUG_CARBON_YELLOW, DEBUG_CARBON_MAGENTA, DEBUG_CARBON_CYAN, DEBUG_CARBON_ORANGE
};
use crate::crystolecule::lattice_fill::placed_atom_tracker::{PlacedAtomTracker, CrystallographicAddress};
use std::collections::HashMap;
use rustc_hash::FxHashMap;
use glam::IVec3;

/// Surface orientation classification for atoms.
/// 
/// This enum categorizes atoms based on their position and bonding:
/// - Bulk: Deep interior atoms (depth > threshold)
/// - Unknown: Surface atoms that don't fit standard (100) reconstruction pattern
/// - Surface variants: Atoms on {100} facets with specific surface normals
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceOrientation {
    Bulk,
    Unknown,
    Surface100,      // (100)
    SurfaceNeg100,   // (-100)
    Surface010,      // (010)
    SurfaceNeg010,   // (0-10)
    Surface001,      // (001)
    SurfaceNeg001,   // (00-1)
}

/// Depth threshold in Ångströms for classifying atoms as bulk.
/// Atoms deeper than this are not near any surface.
const BULK_DEPTH_THRESHOLD: f32 = 0.5;

/// Minimum magnitude for a bond component on an axis to be considered aligned.
/// Components below this threshold are considered insignificant.
const AXIS_ALIGNMENT_THRESHOLD: f64 = 0.5;

/// Visual debug flag: when true, enables visual debugging features:
/// - Replaces atoms with colored elements based on surface orientation
/// - Highlights reconstructed dimer bonds by selecting them
/// Set to false for normal reconstruction operation.
const SURFACE_RECONSTRUCTION_VISUAL_DEBUG: bool = false;

// ============================================================================
// Diamond (100) 2×1 Dimer Reconstruction Geometric Constants
// ============================================================================

/// Target dimer bond length for clean (unpassivated) diamond (100) surface in Ångströms.
/// Literature value: ~1.42 Å
const DIMER_BOND_LENGTH_CLEAN: f64 = 1.42;

/// Target dimer bond length for hydrogen-passivated diamond (100) surface in Ångströms.
/// Passivation weakens the π-bond, lengthening the dimer.
/// Literature value: ~1.6 Å
const DIMER_BOND_LENGTH_PASSIVATED: f64 = 1.6;

/// Vertical displacement (downward) for clean surface reconstruction in Ångströms.
/// Atoms move closer to the second layer.
/// Delta from original: -0.205 Å
const VERTICAL_DISPLACEMENT_CLEAN: f64 = -0.205;

/// Vertical displacement (downward) for passivated surface reconstruction in Ångströms.
/// Weaker reconstruction than clean surface.
/// Delta from original: -0.076 Å
const VERTICAL_DISPLACEMENT_PASSIVATED: f64 = -0.076;

/// C-H bond length for hydrogen passivation in Ångströms.
const C_H_BOND_LENGTH: f64 = 1.09;

/// Angle (in degrees) of C-H bond from surface normal.
/// Hydrogen points away from the dimer partner.
const C_H_ANGLE_FROM_NORMAL_DEGREES: f64 = 24.0;

/// Maps each surface orientation to an atomic number for visual debugging.
/// Uses custom debug carbon elements with carbon radii but distinct colors.
/// All atoms will have the same size for consistent visualization.
fn get_debug_atomic_number(orientation: SurfaceOrientation) -> i16 {
  match orientation {
    SurfaceOrientation::Bulk => DEBUG_CARBON_GRAY as i16,
    SurfaceOrientation::Unknown => DEBUG_CARBON_YELLOW as i16,
    SurfaceOrientation::Surface100 => DEBUG_CARBON_RED as i16,
    SurfaceOrientation::SurfaceNeg100 => DEBUG_CARBON_GREEN as i16,
    SurfaceOrientation::Surface010 => DEBUG_CARBON_BLUE as i16,
    SurfaceOrientation::SurfaceNeg010 => DEBUG_CARBON_ORANGE as i16,
    SurfaceOrientation::Surface001 => DEBUG_CARBON_CYAN as i16,
    SurfaceOrientation::SurfaceNeg001 => DEBUG_CARBON_MAGENTA as i16,
  }
}

/// Classifies the surface orientation of a single atom.
/// 
/// This function determines whether an atom is bulk, on a surface, or has
/// an unknown/ambiguous orientation based on its depth and bonding pattern.
/// 
/// # Arguments
/// * `atom` - The atom to classify
/// * `structure` - The atomic structure (needed to check neighboring atoms)
/// 
/// # Returns
/// * The surface orientation classification for this atom
fn classify_atom_surface_orientation(
  atom: &Atom,
  structure: &AtomicStructure
) -> SurfaceOrientation {
  // Check if atom is deep in the bulk (> 0.5 Å from surface)
  if atom.in_crystal_depth > BULK_DEPTH_THRESHOLD {
    return SurfaceOrientation::Bulk;
  }

  // Surface atoms on {100} facets should have exactly 2 bonds (2 bulk neighbors, 2 dangling)
  if atom.bonds.len() != 2 {
    return SurfaceOrientation::Unknown;
  }

  // Get the two bonded neighbors
  let neighbor1_id = atom.bonds[0].other_atom_id();
  let neighbor2_id = atom.bonds[1].other_atom_id();
  
  let neighbor1 = structure.get_atom(neighbor1_id);
  let neighbor2 = structure.get_atom(neighbor2_id);
  
  if neighbor1.is_none() || neighbor2.is_none() {
    return SurfaceOrientation::Unknown;
  }
  
  // Calculate normalized bond directions
  let dir1 = (neighbor1.unwrap().position - atom.position).normalize();
  let dir2 = (neighbor2.unwrap().position - atom.position).normalize();
  
  // Check each axis (x, y, z) for consistent alignment
  // Both bonds should point in the same direction (same sign) on the dominant axis
  
  // X-axis check
  if dir1.x.abs() > AXIS_ALIGNMENT_THRESHOLD && dir2.x.abs() > AXIS_ALIGNMENT_THRESHOLD {
    if dir1.x > 0.0 && dir2.x > 0.0 {
      return SurfaceOrientation::SurfaceNeg100; // Both bonds point +X, surface normal is -X
    } else if dir1.x < 0.0 && dir2.x < 0.0 {
      return SurfaceOrientation::Surface100; // Both bonds point -X, surface normal is +X
    }
  }
  
  // Y-axis check
  if dir1.y.abs() > AXIS_ALIGNMENT_THRESHOLD && dir2.y.abs() > AXIS_ALIGNMENT_THRESHOLD {
    if dir1.y > 0.0 && dir2.y > 0.0 {
      return SurfaceOrientation::SurfaceNeg010; // Both bonds point +Y, surface normal is -Y
    } else if dir1.y < 0.0 && dir2.y < 0.0 {
      return SurfaceOrientation::Surface010; // Both bonds point -Y, surface normal is +Y
    }
  }
  
  // Z-axis check
  if dir1.z.abs() > AXIS_ALIGNMENT_THRESHOLD && dir2.z.abs() > AXIS_ALIGNMENT_THRESHOLD {
    if dir1.z > 0.0 && dir2.z > 0.0 {
      return SurfaceOrientation::SurfaceNeg001; // Both bonds point +Z, surface normal is -Z
    } else if dir1.z < 0.0 && dir2.z < 0.0 {
      return SurfaceOrientation::Surface001; // Both bonds point -Z, surface normal is +Z
    }
  }
  
  // No axis has consistent alignment - probably edge, corner, or non-{100} surface
  SurfaceOrientation::Unknown
}

/// A dimer pair candidate for surface reconstruction.
/// 
/// This represents a primary atom and its potential dimer partner.
/// The partner orientation has not been validated yet.
#[derive(Debug, Clone, Copy)]
struct DimerPair {
  /// Atom ID of the primary dimer atom
  primary_atom_id: u32,
  
  /// Atom ID of the dimer partner
  partner_atom_id: u32,
  
  /// Surface orientation of the primary atom
  primary_orientation: SurfaceOrientation,
}

/// Results from processing atoms for dimer reconstruction.
/// 
/// This struct contains the dimer candidates and orientation data needed
/// for validating and applying surface reconstruction.
#[derive(Debug)]
struct DimerCandidateData {
  /// Map of potential dimer partner atom IDs to their surface orientations.
  /// Only includes atoms that are not primary dimer atoms (i.e., potential partners).
  /// Excludes bulk and unknown orientation atoms.
  partner_orientations: FxHashMap<u32, SurfaceOrientation>,
  
  /// Vector of dimer pair candidates.
  /// The partner orientation has not been validated yet - validation happens later.
  dimer_pairs: Vec<DimerPair>,
}

/// Helper function to parse lattice offset notation into IVec3.
/// 
/// Parses strings like "..+", "+.-", ".+-" where:
/// - '.' = 0 (no offset)
/// - '+' = +1 unit cell
/// - '-' = -1 unit cell
const fn parse_lattice_offset(s: &str) -> IVec3 {
  let bytes = s.as_bytes();
  IVec3::new(
    match bytes[0] {
      b'.' => 0,
      b'+' => 1,
      b'-' => -1,
      _ => panic!("Invalid offset character"),
    },
    match bytes[1] {
      b'.' => 0,
      b'+' => 1,
      b'-' => -1,
      _ => panic!("Invalid offset character"),
    },
    match bytes[2] {
      b'.' => 0,
      b'+' => 1,
      b'-' => -1,
      _ => panic!("Invalid offset character"),
    },
  )
}

/// Helper to create a dimer partner offset pattern.
const fn partner_offset(offset_str: &str, partner_basis_index: usize) -> CrystallographicAddress {
  CrystallographicAddress {
    motif_space_pos: parse_lattice_offset(offset_str),
    site_index: partner_basis_index,
  }
}

/// Lookup table for dimer partner offsets.
/// 
/// Indexed by [basis_index][surface_index] where surface_index corresponds to:
/// 0 = Surface100, 1 = SurfaceNeg100, 2 = Surface010, 3 = SurfaceNeg010, 4 = Surface001, 5 = SurfaceNeg001
/// 
/// Each entry contains the offset to add to the primary atom's crystallographic address.
const DIMER_PARTNER_OFFSETS: [[CrystallographicAddress; 6]; 8] = [
  // CORNER (index 0) - PRIMARY site at (0, 0, 0)
  [
    partner_offset("...", ZINCBLENDE_SITE_FACE_X),  // +X -> ...FACE_X
    partner_offset("..-", ZINCBLENDE_SITE_FACE_X),  // -X -> ..-FACE_X
    partner_offset("...", ZINCBLENDE_SITE_FACE_Y),  // +Y -> ...FACE_Y
    partner_offset("..-", ZINCBLENDE_SITE_FACE_Y),  // -Y -> ..-FACE_Y
    partner_offset("...", ZINCBLENDE_SITE_FACE_Z),  // +Z -> ...FACE_Z
    partner_offset(".-.", ZINCBLENDE_SITE_FACE_Z),  // -Z -> .-.FACE_Z
  ],
  // FACE_Z (index 1) - PRIMARY site at (0.5, 0.5, 0)
  [
    partner_offset(".+.", ZINCBLENDE_SITE_FACE_Y),  // +X -> .+.FACE_Y
    partner_offset(".+-", ZINCBLENDE_SITE_FACE_Y),  // -X -> .+-FACE_Y
    partner_offset("+..", ZINCBLENDE_SITE_FACE_X),  // +Y -> +..FACE_X
    partner_offset("+.-", ZINCBLENDE_SITE_FACE_X),  // -Y -> +.-FACE_X
    partner_offset("++.", ZINCBLENDE_SITE_CORNER),  // +Z -> ++.CORNER
    partner_offset("+..", ZINCBLENDE_SITE_CORNER),  // -Z -> +..CORNER
  ],
  // FACE_Y (index 2) - PRIMARY site at (0.5, 0, 0.5)
  [
    partner_offset("..+", ZINCBLENDE_SITE_FACE_Z),  // +X -> ..+FACE_Z
    partner_offset("...", ZINCBLENDE_SITE_FACE_Z),  // -X -> ...FACE_Z
    partner_offset("+.+", ZINCBLENDE_SITE_CORNER),  // +Y -> +.+CORNER
    partner_offset("+..", ZINCBLENDE_SITE_CORNER),  // -Y -> +..CORNER
    partner_offset("+..", ZINCBLENDE_SITE_FACE_X),  // +Z -> +..FACE_X
    partner_offset("+-.", ZINCBLENDE_SITE_FACE_X),  // -Z -> +-.FACE_X
  ],
  // FACE_X (index 3) - PRIMARY site at (0, 0.5, 0.5)
  [
    partner_offset(".++", ZINCBLENDE_SITE_CORNER),  // +X -> .++CORNER
    partner_offset(".+.", ZINCBLENDE_SITE_CORNER),  // -X -> .+.CORNER
    partner_offset("..+", ZINCBLENDE_SITE_FACE_Z),  // +Y -> ..+FACE_Z
    partner_offset("...", ZINCBLENDE_SITE_FACE_Z),  // -Y -> ...FACE_Z
    partner_offset(".+.", ZINCBLENDE_SITE_FACE_Y),  // +Z -> .+.FACE_Y
    partner_offset("...", ZINCBLENDE_SITE_FACE_Y),  // -Z -> ...FACE_Y
  ],
  // INTERIOR1 (index 4) - SECONDARY site at (0.25, 0.25, 0.25)
  [
    partner_offset("..-", ZINCBLENDE_SITE_INTERIOR2),  // +X -> ..-INTERIOR2
    partner_offset("...", ZINCBLENDE_SITE_INTERIOR2),  // -X -> ...INTERIOR2
    partner_offset("..-", ZINCBLENDE_SITE_INTERIOR3),  // +Y -> ..-INTERIOR3
    partner_offset("...", ZINCBLENDE_SITE_INTERIOR3),  // -Y -> ...INTERIOR3
    partner_offset(".-.", ZINCBLENDE_SITE_INTERIOR4),  // +Z -> .-.INTERIOR4
    partner_offset("...", ZINCBLENDE_SITE_INTERIOR4),  // -Z -> ...INTERIOR4
  ],
  // INTERIOR2 (index 5) - SECONDARY site at (0.25, 0.75, 0.75)
  [
    partner_offset(".+.", ZINCBLENDE_SITE_INTERIOR1),  // +X -> .+.INTERIOR1
    partner_offset(".++", ZINCBLENDE_SITE_INTERIOR1),  // -X -> .++INTERIOR1
    partner_offset("...", ZINCBLENDE_SITE_INTERIOR4),  // +Y -> ...INTERIOR4
    partner_offset("..+", ZINCBLENDE_SITE_INTERIOR4),  // -Y -> ..+INTERIOR4
    partner_offset("...", ZINCBLENDE_SITE_INTERIOR3),  // +Z -> ...INTERIOR3
    partner_offset(".+.", ZINCBLENDE_SITE_INTERIOR3),  // -Z -> .+.INTERIOR3
  ],
  // INTERIOR3 (index 6) - SECONDARY site at (0.75, 0.25, 0.75)
  [
    partner_offset("...", ZINCBLENDE_SITE_INTERIOR4),  // +X -> ...INTERIOR4
    partner_offset("..+", ZINCBLENDE_SITE_INTERIOR4),  // -X -> ..+INTERIOR4
    partner_offset("+..", ZINCBLENDE_SITE_INTERIOR1),  // +Y -> +..INTERIOR1
    partner_offset("+.+", ZINCBLENDE_SITE_INTERIOR1),  // -Y -> +.+INTERIOR1
    partner_offset("+-.", ZINCBLENDE_SITE_INTERIOR2),  // +Z -> +-.INTERIOR2
    partner_offset("+..", ZINCBLENDE_SITE_INTERIOR2),  // -Z -> +..INTERIOR2
  ],
  // INTERIOR4 (index 7) - SECONDARY site at (0.75, 0.75, 0.25)
  [
    partner_offset(".+-", ZINCBLENDE_SITE_INTERIOR3),  // +X -> .+-INTERIOR3
    partner_offset(".+.", ZINCBLENDE_SITE_INTERIOR3),  // -X -> .+.INTERIOR3
    partner_offset("+.-", ZINCBLENDE_SITE_INTERIOR2),  // +Y -> +.-INTERIOR2
    partner_offset("+..", ZINCBLENDE_SITE_INTERIOR2),  // -Y -> +..INTERIOR2
    partner_offset("+..", ZINCBLENDE_SITE_INTERIOR1),  // +Z -> +..INTERIOR1
    partner_offset("++.", ZINCBLENDE_SITE_INTERIOR1),  // -Z -> ++.INTERIOR1
  ],
];

/// Site layer mapping: maps (surface_orientation, site_index) to (in_surface_idx, depth_index)
/// This tells us which layer a site belongs to and its index within that layer.
/// Indexed as: SITE_LAYER_MAP[surface_orientation_idx][site_index]
/// Surface order: [+X, -X, +Y, -Y, +Z, -Z]
const SITE_LAYER_MAP: [[(u8, u8); 8]; 6] = [
  // +X
  [(0, 0), (0, 2), (1, 2), (1, 0), (0, 1), (1, 1), (0, 3), (1, 3)],
  // -X
  [(0, 0), (0, 2), (1, 2), (1, 0), (0, 1), (1, 1), (0, 3), (1, 3)],
  // +Y
  [(0, 0), (0, 2), (1, 0), (1, 2), (0, 1), (0, 3), (1, 1), (1, 3)],
  // -Y
  [(0, 0), (0, 2), (1, 0), (1, 2), (0, 1), (0, 3), (1, 1), (1, 3)],
  // +Z
  [(0, 0), (1, 0), (0, 2), (1, 2), (0, 1), (0, 3), (1, 3), (1, 1)],
  // -Z
  [(0, 0), (1, 0), (0, 2), (1, 2), (0, 1), (0, 3), (1, 3), (1, 1)],
];

/// Per-layer phase control: 24 booleans (6 surfaces × 4 depths).
/// Set to true to flip phase for that layer.
/// Indexed as: PHASE_FLIP[surface_idx * 4 + depth_idx]
/// Order: +X(0,0.25,0.5,0.75), -X(0,0.25,0.5,0.75), +Y(...), -Y(...), +Z(...), -Z(...)
/// 
/// All false = Phase A (default). Flip individual layers to customize phase pattern.
const PHASE_FLIP: [bool; 24] = [
  false, false, false, false, // +X: depths 0.00, 0.25, 0.50, 0.75
  false, false, false, false, // -X: depths 0.00, 0.25, 0.50, 0.75
  false, false, false, false, // +Y: depths 0.00, 0.25, 0.50, 0.75
  false, false, false, false, // -Y: depths 0.00, 0.25, 0.50, 0.75
  false, false, false, false, // +Z: depths 0.00, 0.25, 0.50, 0.75
  false, false, false, false, // -Z: depths 0.00, 0.25, 0.50, 0.75
];

/// Ultimate truth tables for ALL surfaces and ALL depth layers.
/// Indexed as: TRUTH_TABLES[surf_idx * 4 + depth_idx][c1%2][c2%2][in_surface_idx]
/// Guarantees that all dimer partners have opposite parities, including:
/// - Different lattice position partners (e.g., CORNER ↔ FACE_X/Y/Z)
/// - Same lattice position partners (e.g., INTERIOR1 ↔ INTERIOR4)
/// - All fractional surface cuts through any layer
const TRUTH_TABLES: [[[[bool; 2]; 2]; 2]; 24] = [
  // +X Layer 0 (index 0) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // +X Layer 1 (index 1) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // +X Layer 2 (index 2) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // +X Layer 3 (index 3) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // -X Layer 0 (index 4) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // -X Layer 1 (index 5) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // -X Layer 2 (index 6) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // -X Layer 3 (index 7) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // +Y Layer 0 (index 8) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // +Y Layer 1 (index 9) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // +Y Layer 2 (index 10) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // +Y Layer 3 (index 11) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // -Y Layer 0 (index 12) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // -Y Layer 1 (index 13) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // -Y Layer 2 (index 14) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // -Y Layer 3 (index 15) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // +Z Layer 0 (index 16) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // +Z Layer 1 (index 17) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // +Z Layer 2 (index 18) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // +Z Layer 3 (index 19) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // -Z Layer 0 (index 20) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
  // -Z Layer 1 (index 21) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // -Z Layer 2 (index 22) - CHECKERBOARD
  [[[true , false], [false, true ]], [[false, true ], [true , false]]],
  // -Z Layer 3 (index 23) - CHECKERBOARD
  [[[false, false], [true , true ]], [[true , true ], [false, false]]],
];

/// Precomputed in-plane axis indices for each surface orientation.
/// Each entry: (in_plane_idx_1, in_plane_idx_2)
/// Surface order: [+X, -X, +Y, -Y, +Z, -Z]
const IN_PLANE_AXES: [(usize, usize); 6] = [
  (1, 2), // +X: YZ plane
  (1, 2), // -X: YZ plane
  (0, 2), // +Y: XZ plane
  (0, 2), // -Y: XZ plane
  (0, 1), // +Z: XY plane
  (0, 1), // -Z: XY plane
];

/// Maps SurfaceOrientation to index for lookup tables.
#[inline]
fn surface_orientation_to_index(orientation: SurfaceOrientation) -> Option<usize> {
  match orientation {
    SurfaceOrientation::Surface100 => Some(0),    // +X
    SurfaceOrientation::SurfaceNeg100 => Some(1), // -X
    SurfaceOrientation::Surface010 => Some(2),    // +Y
    SurfaceOrientation::SurfaceNeg010 => Some(3), // -Y
    SurfaceOrientation::Surface001 => Some(4),    // +Z
    SurfaceOrientation::SurfaceNeg001 => Some(5), // -Z
    _ => None,
  }
}

/// Determines if an atom at the given crystallographic address is a primary dimer atom.
/// 
/// This function encodes the **phase selection** using comprehensive truth tables for all
/// 24 surface-layer combinations (6 surfaces × 4 depth layers). Each truth table implements
/// a checkerboard pattern that guarantees dimer partners have opposite parities.
/// 
/// The truth tables are indexed by:
/// - `surf_idx`: Surface orientation (0-5 for +X, -X, +Y, -Y, +Z, -Z)
/// - `depth_index`: Depth layer (0-3 for 0.00, 0.25, 0.50, 0.75 Å)
/// - `c1%2, c2%2`: In-plane lattice coordinates (modulo 2)
/// - `in_surface_idx`: Site index within layer (0 or 1)
/// 
/// Each surface-layer has exactly TWO valid solutions (Phase A and Phase B), which are
/// inverses of each other. Phase selection is controlled by the PHASE_FLIP array.
/// 
/// # Arguments
/// * `address` - Crystallographic address of the atom (motif space position + site index)
/// * `orientation` - Surface orientation of the atom
/// 
/// # Returns
/// * `true` if this is a primary dimer atom for the current phase, `false` otherwise
fn is_primary_dimer_atom(
  address: &CrystallographicAddress,
  orientation: SurfaceOrientation
) -> bool {
  // Map orientation to index for lookup tables
  let surf_idx = match surface_orientation_to_index(orientation) {
    Some(idx) => idx,
    None => return false, // Bulk or Unknown
  };
  
  // Bounds check site_index
  if address.site_index >= 8 {
    return false;
  }
  
  // Look up which layer this site belongs to
  let (in_surface_idx, depth_index) = SITE_LAYER_MAP[surf_idx][address.site_index];
  
  // Get in-plane coordinates using precomputed axes
  let (in_plane_idx_1, in_plane_idx_2) = IN_PLANE_AXES[surf_idx];
  let c1 = address.motif_space_pos[in_plane_idx_1];
  let c2 = address.motif_space_pos[in_plane_idx_2];
  
  // Calculate parity using comprehensive truth tables
  // Truth tables are indexed by (surface_idx, depth_index, c1%2, c2%2, in_surface_idx)
  // This guarantees correct dimer formation for:
  // - All surface orientations (+X, -X, +Y, -Y, +Z, -Z)
  // - All depth layers (0, 1, 2, 3) including fractional surface cuts
  // - Both different-position and same-position dimer partners
  let table_idx = surf_idx * 4 + depth_index as usize;
  let c1_mod = ((c1 % 2 + 2) % 2) as usize;  // Handle negative modulo
  let c2_mod = ((c2 % 2 + 2) % 2) as usize;
  let parity = if TRUTH_TABLES[table_idx][c1_mod][c2_mod][in_surface_idx as usize] {
    0  // primary
  } else {
    1  // secondary
  };
  
  // Apply per-layer phase flip (reuse table_idx since it's the same calculation)
  let phase_flip = PHASE_FLIP[table_idx];
  let final_parity = parity ^ (phase_flip as usize);
  
  // Primary atoms have parity 0
  final_parity == 0
}

/// Computes the crystallographic address of the dimer partner for a primary atom.
/// 
/// This function performs **geometric partner selection**, which is phase-independent and
/// purely determined by crystal structure. For each (surface_orientation, basis_index) pair,
/// the partner is always in the conventional positive direction along the lowest-indexed
/// in-plane axis. This ensures deterministic, unambiguous partner selection.
/// 
/// # Arguments
/// * `primary_address` - Crystallographic address of the primary atom
/// * `orientation` - Surface orientation of the primary atom
/// 
/// # Returns
/// * `Some(partner_address)` if a partner exists for this surface orientation
/// * `None` if this surface orientation doesn't support dimer reconstruction
fn get_dimer_partner(
  primary_address: &CrystallographicAddress,
  orientation: SurfaceOrientation
) -> Option<CrystallographicAddress> {
  // Map orientation to surface index for lookup table
  let surface_index = surface_orientation_to_index(orientation)?;
  
  // Bounds check the basis index (should be 0-7 for zincblende)
  if primary_address.site_index >= 8 {
    return None;
  }
  
  // Look up the partner offset pattern from the table
  let offset = DIMER_PARTNER_OFFSETS[primary_address.site_index][surface_index];
  
  // Add the offset to get the partner's crystallographic address
  Some(CrystallographicAddress {
    motif_space_pos: primary_address.motif_space_pos + offset.motif_space_pos,
    site_index: offset.site_index,
  })
}

/// Processes all atoms to classify orientations and identify dimer pair candidates.
/// 
/// This function iterates through the PlacedAtomTracker (which has lattice coordinates)
/// and performs all necessary processing in a single efficient pass:
/// - Classifies surface orientation
/// - Applies debug visualization if enabled
/// - Identifies primary dimer atoms
/// - Finds dimer partner candidates
/// 
/// # Arguments
/// * `structure` - The atomic structure (mutable for debug visualization)
/// * `atom_tracker` - Tracker with lattice coordinate mappings
/// 
/// # Returns
/// * `DimerCandidateData` containing partner orientations and dimer pair candidates
fn process_atoms(
  structure: &mut AtomicStructure,
  atom_tracker: &PlacedAtomTracker
) -> DimerCandidateData {
  let mut partner_orientations: FxHashMap<u32, SurfaceOrientation> = FxHashMap::default();
  let mut dimer_pairs: Vec<DimerPair> = Vec::new();

  // Iterate through all placed atoms
  for (address, atom_id) in atom_tracker.iter_atoms() {
    // Get the atom from the structure
    let atom = match structure.get_atom(atom_id) {
      Some(a) => a,
      None => continue, // Atom doesn't exist in structure, skip
    };

    // Classify surface orientation
    let orientation = classify_atom_surface_orientation(atom, structure);
    
    // Apply debug visualization if enabled
    if SURFACE_RECONSTRUCTION_VISUAL_DEBUG {
      structure.set_atom_atomic_number(atom_id, get_debug_atomic_number(orientation));
    }

    // Skip bulk and unknown atoms - we only care about potential dimer candidates
    if orientation == SurfaceOrientation::Bulk || orientation == SurfaceOrientation::Unknown {
      continue;
    }

    // Check if this is a primary dimer atom
    if is_primary_dimer_atom(&address, orientation) {
      // Get the dimer partner crystallographic address
      if let Some(partner_address) = get_dimer_partner(&address, orientation) {
        // Look up the partner atom ID
        if let Some(partner_atom_id) = atom_tracker.get_atom_id_by_address(&partner_address) {
          // Add to dimer candidates (partner orientation not validated yet)
          dimer_pairs.push(DimerPair {
            primary_atom_id: atom_id,
            partner_atom_id,
            primary_orientation: orientation,
          });
        }
      }
    } else {
      partner_orientations.insert(atom_id, orientation);
    }

  }

  DimerCandidateData {
    partner_orientations,
    dimer_pairs,
  }
}

/// Applies surface reconstruction to a validated dimer pair.
/// 
/// This function performs the actual reconstruction on a pair of atoms that
/// have been validated to be proper dimer partners on the same surface facet.
/// 
/// Implements diamond (100) 2×1 dimer reconstruction:
/// 1. Creates dimer bond between surface atoms
/// 2. Moves atoms symmetrically toward each other (in-plane)
/// 3. Moves atoms down toward second layer (perpendicular to surface)
/// 4. Optionally adds hydrogen passivation
/// 
/// # Arguments
/// * `structure` - The atomic structure to modify
/// * `dimer_pair` - The validated dimer pair to reconstruct
/// * `hydrogen_passivation` - Whether to add hydrogen passivation to this dimer
fn apply_dimer_reconstruction(
  structure: &mut AtomicStructure,
  dimer_pair: &DimerPair,
  hydrogen_passivation: bool
) {
  // Select geometry parameters based on passivation setting
  let target_bond_length = if hydrogen_passivation {
    DIMER_BOND_LENGTH_PASSIVATED
  } else {
    DIMER_BOND_LENGTH_CLEAN
  };
  
  let vertical_displacement = if hydrogen_passivation {
    VERTICAL_DISPLACEMENT_PASSIVATED
  } else {
    VERTICAL_DISPLACEMENT_CLEAN
  };
  
  // Get the two atoms
  let atom1 = structure.get_atom(dimer_pair.primary_atom_id);
  let atom2 = structure.get_atom(dimer_pair.partner_atom_id);
  
  if atom1.is_none() || atom2.is_none() {
    return; // Safety check
  }
  
  let pos1 = atom1.unwrap().position;
  let pos2 = atom2.unwrap().position;
  
  // Calculate current distance
  let current_vec = pos2 - pos1;
  let current_distance = current_vec.length();
  
  if current_distance < 0.01 {
    return; // Safety check for coincident atoms
  }
  
  // Calculate in-plane direction (normalized vector from atom1 to atom2)
  let in_plane_direction = current_vec.normalize();
  
  // Calculate how much to move each atom toward the midpoint to achieve target bond length
  // Each atom moves half the distance change
  let distance_change = current_distance - target_bond_length;
  let move_distance = distance_change * 0.5;
  
  let surface_normal = get_surface_normal_direction(dimer_pair.primary_orientation);
  
  // Calculate new positions:
  // 1. Move toward midpoint (symmetric in-plane displacement)
  // 2. Move down perpendicular to surface (vertical displacement)
  let new_pos1 = pos1 + in_plane_direction * move_distance + surface_normal * vertical_displacement;
  let new_pos2 = pos2 - in_plane_direction * move_distance + surface_normal * vertical_displacement;
  
  // Apply position changes
  structure.set_atom_position(dimer_pair.primary_atom_id, new_pos1);
  structure.set_atom_position(dimer_pair.partner_atom_id, new_pos2);
  
  // Create the dimer bond
  structure.add_bond(
    dimer_pair.primary_atom_id,
    dimer_pair.partner_atom_id,
    1  // Single bond order
  );
  
  // Make the newly created bond selected for visualization (only in debug mode)
  if SURFACE_RECONSTRUCTION_VISUAL_DEBUG {
    let bond_ref = BondReference {
      atom_id1: dimer_pair.primary_atom_id,
      atom_id2: dimer_pair.partner_atom_id,
    };
    structure.select_bond(&bond_ref);
  }
  
  // Add hydrogen passivation if enabled
  if hydrogen_passivation {
    add_hydrogen_passivation(structure, dimer_pair, &new_pos1, &new_pos2, &in_plane_direction, &surface_normal);
  }
}

/// Returns the outward surface normal direction for a given surface orientation.
/// 
/// # Arguments
/// * `orientation` - The surface orientation
/// 
/// # Returns
/// * A unit vector pointing outward from the surface (into vacuum)
fn get_surface_normal_direction(orientation: SurfaceOrientation) -> glam::DVec3 {
  use glam::DVec3;
  
  match orientation {
    SurfaceOrientation::Surface100 => DVec3::new(1.0, 0.0, 0.0),      // +X
    SurfaceOrientation::SurfaceNeg100 => DVec3::new(-1.0, 0.0, 0.0),  // -X
    SurfaceOrientation::Surface010 => DVec3::new(0.0, 1.0, 0.0),      // +Y
    SurfaceOrientation::SurfaceNeg010 => DVec3::new(0.0, -1.0, 0.0),  // -Y
    SurfaceOrientation::Surface001 => DVec3::new(0.0, 0.0, 1.0),      // +Z
    SurfaceOrientation::SurfaceNeg001 => DVec3::new(0.0, 0.0, -1.0),  // -Z
    _ => DVec3::new(0.0, 0.0, 1.0), // Default fallback
  }
}

/// Adds hydrogen passivation atoms to a reconstructed dimer pair.
/// 
/// For each carbon atom in the dimer, places a hydrogen atom:
/// - C-H bond length: 1.09 Å
/// - C-H direction: 24° from surface normal, pointing away from dimer partner
/// 
/// # Arguments
/// * `structure` - The atomic structure to modify
/// * `dimer_pair` - The dimer pair information
/// * `pos1` - Position of first carbon atom
/// * `pos2` - Position of second carbon atom
/// * `in_plane_direction` - Unit vector from atom1 to atom2 (in-plane)
/// * `surface_normal` - Unit vector pointing outward from surface
fn add_hydrogen_passivation(
  structure: &mut AtomicStructure,
  dimer_pair: &DimerPair,
  pos1: &glam::DVec3,
  pos2: &glam::DVec3,
  in_plane_direction: &glam::DVec3,
  surface_normal: &glam::DVec3
) {
  use glam::DVec3;
  use std::f64::consts::PI;
  
  let angle_rad = C_H_ANGLE_FROM_NORMAL_DEGREES * PI / 180.0;
  let cos_angle = angle_rad.cos();
  let sin_angle = angle_rad.sin();
  
  // For atom 1: hydrogen points away from atom 2
  // Direction: tilt from surface normal toward -in_plane_direction
  let h1_direction = (*surface_normal * cos_angle - *in_plane_direction * sin_angle).normalize();
  let h1_pos = pos1 + h1_direction * C_H_BOND_LENGTH;
  
  // For atom 2: hydrogen points away from atom 1
  // Direction: tilt from surface normal toward +in_plane_direction
  let h2_direction = (*surface_normal * cos_angle + *in_plane_direction * sin_angle).normalize();
  let h2_pos = pos2 + h2_direction * C_H_BOND_LENGTH;
  
  // Add hydrogen atoms (atomic number 1) using add_atom to update internal data structures
  let h1_id = structure.add_atom(1, h1_pos);
  let h2_id = structure.add_atom(1, h2_pos);
  
  // Create C-H bonds
  structure.add_bond(dimer_pair.primary_atom_id, h1_id, 1);
  structure.add_bond(dimer_pair.partner_atom_id, h2_id, 1);
  
  // Flag the carbon atoms as hydrogen passivated
  structure.set_atom_hydrogen_passivation(dimer_pair.primary_atom_id, true);
  structure.set_atom_hydrogen_passivation(dimer_pair.partner_atom_id, true);
}

/// Determines if the current structure is cubic diamond suitable for (100) reconstruction.
/// 
/// This function checks three conditions:
/// 1. The motif matches the built-in zincblende motif structure
/// 2. The unit cell is approximately cubic
/// 3. The unit cell size matches the diamond lattice parameter
/// 4. Both PRIMARY and SECONDARY element parameters are set to carbon (atomic number 6)
/// 
/// # Arguments
/// * `motif` - The motif to check
/// * `unit_cell` - The unit cell to check
/// * `parameter_element_values` - The parameter element values (PRIMARY, SECONDARY, etc.)
/// 
/// # Returns
/// * `true` if all conditions are met for cubic diamond
/// * `false` otherwise
pub fn is_cubic_diamond(
  motif: &Motif,
  unit_cell: &UnitCellStruct,
  parameter_element_values: &HashMap<String, i16>
) -> bool {
  // Check if motif matches the built-in zincblende motif
  if !motif.is_structurally_equal(&DEFAULT_ZINCBLENDE_MOTIF) {
    return false;
  }

  // Check if unit cell is approximately cubic
  if !unit_cell.is_approximately_cubic() {
    return false;
  }

  // Check if the unit cell size matches diamond lattice parameter
  const EPSILON: f64 = 1e-5;
  let cell_size = unit_cell.a.length();
  if (cell_size - DIAMOND_UNIT_CELL_SIZE_ANGSTROM).abs() > EPSILON {
    return false;
  }

  // Get effective parameter values (with defaults filled in)
  let effective_params = motif.get_effective_parameter_element_values(parameter_element_values);

  // Check if both PRIMARY and SECONDARY are set to carbon (atomic number 6)
  const CARBON_ATOMIC_NUMBER: i16 = 6;
  let primary_is_carbon = effective_params.get("PRIMARY") == Some(&CARBON_ATOMIC_NUMBER);
  let secondary_is_carbon = effective_params.get("SECONDARY") == Some(&CARBON_ATOMIC_NUMBER);

  primary_is_carbon && secondary_is_carbon
}

/// Performs (100) 2×1 dimer reconstruction for cubic diamond.
/// 
/// This function implements the surface reconstruction algorithm described in
/// surface_reconstructions.md for (100) surfaces of cubic diamond.
/// 
/// # Arguments
/// * `structure` - The atomic structure to apply reconstruction to
/// * `atom_tracker` - Tracker with lattice coordinate mappings for all placed atoms
/// * `_motif` - The motif defining the crystal structure
/// * `_unit_cell` - The unit cell defining the lattice
/// * `_parameter_element_values` - Map of parameter element names to atomic numbers
/// * `single_bond_atoms_already_removed` - Whether single-bond atoms were already removed
/// * `hydrogen_passivation` - Whether to add hydrogen passivation to reconstructed dimers
/// 
/// # Returns
/// * The number of dimers reconstructed
pub fn reconstruct_surface_100_diamond(
  structure: &mut AtomicStructure,
  atom_tracker: &PlacedAtomTracker,
  _motif: &Motif,
  _unit_cell: &UnitCellStruct,
  _parameter_element_values: &HashMap<String, i16>,
  single_bond_atoms_already_removed: bool,
  hydrogen_passivation: bool
) -> usize {
  // Remove single-bond atoms if they haven't been removed yet
  // This is necessary for proper surface reconstruction
  if !single_bond_atoms_already_removed {
    remove_single_bond_atoms(structure, true);
  }
  
  // Step 1: Process atoms - classify orientations and identify dimer candidates
  // Debug visualization is applied during processing if SURFACE_RECONSTRUCTION_VISUAL_DEBUG is true
  let candidate_data = process_atoms(structure, atom_tracker);

  // Step 2: Process dimer candidates - validate and apply reconstruction
  let mut dimer_count = 0;
  for dimer_pair in &candidate_data.dimer_pairs {
    // Validate: check that the partner has the same orientation as the primary
    if let Some(&partner_orientation) = candidate_data.partner_orientations.get(&dimer_pair.partner_atom_id) {
      // Only reconstruct if both atoms have the same surface orientation
      if partner_orientation == dimer_pair.primary_orientation {
        apply_dimer_reconstruction(structure, dimer_pair, hydrogen_passivation);
        dimer_count += 1;
      }
    }
  }
  
  dimer_count
}

/// Performs surface reconstruction on the atomic structure.
/// 
/// This function will be expanded to support various reconstruction types,
/// starting with (100) 2×1 dimer reconstruction for cubic diamond.
/// 
/// # Arguments
/// * `structure` - The atomic structure to apply reconstruction to
/// * `atom_tracker` - Tracker with lattice coordinate mappings for all placed atoms
/// * `motif` - The motif defining the crystal structure
/// * `unit_cell` - The unit cell defining the lattice
/// * `parameter_element_values` - Map of parameter element names to atomic numbers
/// * `single_bond_atoms_already_removed` - Whether single-bond atoms were already removed
/// * `hydrogen_passivation` - Whether to add hydrogen passivation to reconstructed dimers
/// 
/// # Returns
/// * The number of surface reconstructions performed (e.g., number of dimers for (100) reconstruction)
pub fn reconstruct_surface(
  structure: &mut AtomicStructure,
  atom_tracker: &PlacedAtomTracker,
  motif: &Motif,
  unit_cell: &UnitCellStruct,
  parameter_element_values: &HashMap<String, i16>,
  single_bond_atoms_already_removed: bool,
  hydrogen_passivation: bool
) -> usize {
  // Check if we're dealing with cubic diamond - if not, do nothing for now
  if !is_cubic_diamond(motif, unit_cell, parameter_element_values) {
    return 0;
  }

  // Perform (100) 2×1 dimer reconstruction for cubic diamond
  reconstruct_surface_100_diamond(structure, atom_tracker, motif, unit_cell, parameter_element_values, single_bond_atoms_already_removed, hydrogen_passivation)
}
















