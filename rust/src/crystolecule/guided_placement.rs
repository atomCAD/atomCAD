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

/// The placement mode determines how guide positions are presented.
#[derive(Debug, Clone)]
pub enum GuidedPlacementMode {
    /// Fixed guide dots at computed positions (sp3 cases 2, 3; sp2/sp1 in future).
    FixedDots { guide_dots: Vec<GuideDot> },
    /// Bare atom with no bonds: wireframe sphere where the user can click anywhere.
    FreeSphere {
        center: DVec3,
        radius: f64,
        /// Cursor-tracked preview position on the sphere surface (updated by pointer_move).
        preview_position: Option<DVec3>,
    },
    /// Ring without reference (sp3 case 1 or sp2 case 1): wireframe ring where
    /// guide dots rotate together as the user moves the cursor.
    FreeRing {
        /// Center of the ring (on the cone axis, offset from anchor).
        ring_center: DVec3,
        /// Normal of the ring plane (points away from the existing bond).
        ring_normal: DVec3,
        /// Radius of the ring circle.
        ring_radius: f64,
        /// Bond distance from anchor to guide dot positions.
        bond_distance: f64,
        /// Anchor atom position (needed for placement).
        anchor_pos: DVec3,
        /// Number of preview dots (3 for sp3, 2 for sp2).
        num_ring_dots: usize,
        /// Cursor-tracked preview positions on the ring.
        preview_positions: Option<Vec<DVec3>>,
    },
}

#[derive(Debug, Clone)]
pub struct GuidedPlacementResult {
    pub anchor_atom_id: u32,
    pub hybridization: Hybridization,
    pub mode: GuidedPlacementMode,
    pub bond_distance: f64,
    pub remaining_slots: usize,
    /// True when geometric max > covalent max (atom has lone pairs / empty orbitals)
    pub has_additional_geometric_capacity: bool,
}

impl GuidedPlacementMode {
    /// Returns guide dots if in FixedDots mode, empty slice otherwise.
    pub fn guide_dots(&self) -> &[GuideDot] {
        match self {
            GuidedPlacementMode::FixedDots { guide_dots } => guide_dots,
            GuidedPlacementMode::FreeSphere { .. } | GuidedPlacementMode::FreeRing { .. } => &[],
        }
    }

    /// Returns true if this is a FreeSphere mode.
    pub fn is_free_sphere(&self) -> bool {
        matches!(self, GuidedPlacementMode::FreeSphere { .. })
    }

    /// Returns true if this is a FreeRing mode.
    pub fn is_free_ring(&self) -> bool {
        matches!(self, GuidedPlacementMode::FreeRing { .. })
    }
}

impl GuidedPlacementResult {
    /// Convenience accessor: returns guide dots from the mode (empty for FreeSphere).
    pub fn guide_dots(&self) -> &[GuideDot] {
        self.mode.guide_dots()
    }
}

// ============================================================================
// Ray-sphere intersection
// ============================================================================

/// Compute ray-sphere intersection, returning the front-hemisphere hit point.
///
/// Only returns a point on the front hemisphere (facing the ray origin).
/// Returns `None` if the ray misses the sphere entirely.
pub fn ray_sphere_nearest_point(
    ray_start: &DVec3,
    ray_dir: &DVec3,
    sphere_center: &DVec3,
    sphere_radius: f64,
) -> Option<DVec3> {
    let oc = *ray_start - *sphere_center;
    let a = ray_dir.dot(*ray_dir);
    let b = 2.0 * ray_dir.dot(oc);
    let c = oc.length_squared() - sphere_radius * sphere_radius;
    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 {
        return None;
    }

    let sqrt_disc = discriminant.sqrt();
    let t1 = (-b - sqrt_disc) / (2.0 * a);
    let t2 = (-b + sqrt_disc) / (2.0 * a);

    // Prefer the nearest positive intersection (front hit)
    let t = if t1 > 0.0 {
        t1
    } else if t2 > 0.0 {
        t2
    } else {
        return None; // Both behind the ray
    };

    Some(*ray_start + *ray_dir * t)
}

// ============================================================================
// Crystal bond length table
// ============================================================================

/// Hardcoded table of sp3 semiconductor crystal bond lengths.
/// Key: (min(Z_a, Z_b), max(Z_a, Z_b)). Values in Angstroms.
/// Derived from zinc blende / diamond cubic unit cell parameter `a`
/// via `bond_length = a * sqrt(3) / 4`.
const CRYSTAL_BOND_LENGTHS: &[((i16, i16), f64)] = &[
    ((6, 6), 1.545),   // Diamond C-C
    ((14, 14), 2.352), // Silicon Si-Si
    ((6, 14), 1.889),  // 3C-SiC
    ((32, 32), 2.450), // Germanium Ge-Ge
    ((50, 50), 2.810), // alpha-Sn
    ((5, 7), 1.567),   // c-BN
    ((5, 15), 1.966),  // BP
    ((7, 13), 1.897),  // AlN (zinc blende)
    ((13, 15), 2.367), // AlP
    ((13, 33), 2.443), // AlAs
    ((7, 31), 1.946),  // GaN (zinc blende)
    ((15, 31), 2.360), // GaP
    ((31, 33), 2.448), // GaAs
    ((15, 49), 2.541), // InP
    ((33, 49), 2.623), // InAs
    ((49, 51), 2.806), // InSb
    ((16, 30), 2.342), // ZnS (zinc blende)
    ((30, 34), 2.454), // ZnSe
    ((30, 52), 2.637), // ZnTe
    ((48, 52), 2.806), // CdTe
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
        Some(atom) => atom.bonds.iter().filter(|b| !b.is_delete_marker()).count(),
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

/// Result of sp3 candidate computation, accounting for case 1 which may need
/// either fixed dots (with dihedral reference) or a free ring (without).
pub enum Sp3CandidateResult {
    /// Fixed guide dots at computed positions.
    Dots(Vec<GuideDot>),
    /// sp3 case 1 result that may be FixedDots or FreeRing.
    Case1(Sp3Case1Result),
}

/// Compute sp3 candidate positions for guided placement.
///
/// - Case 4 (saturated): empty dots
/// - Case 3 (1 remaining): single dot opposite centroid of existing bonds
/// - Case 2 (2 remaining): two dots symmetric about the existing bond plane
/// - Case 1 (3 remaining): dihedral-aware (6 dots) or ring fallback
/// - Case 0: handled by caller (FreeSphere)
pub fn compute_sp3_candidates(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    anchor_pos: DVec3,
    existing_bond_dirs: &[DVec3],
    bond_dist: f64,
) -> Sp3CandidateResult {
    match existing_bond_dirs.len() {
        4.. => Sp3CandidateResult::Dots(vec![]), // saturated
        3 => Sp3CandidateResult::Dots(sp3_case3(anchor_pos, existing_bond_dirs, bond_dist)),
        2 => Sp3CandidateResult::Dots(sp3_case2(anchor_pos, existing_bond_dirs, bond_dist)),
        1 => {
            let bond_dir = existing_bond_dirs[0];
            // Find the neighbor atom for dihedral reference
            let neighbor_id = structure.get_atom(anchor_atom_id).and_then(|atom| {
                atom.bonds
                    .iter()
                    .find(|b| !b.is_delete_marker())
                    .map(|b| b.other_atom_id())
            });

            if let Some(neighbor_id) = neighbor_id {
                if let Some(ref_perp) =
                    find_dihedral_reference(structure, anchor_atom_id, neighbor_id)
                {
                    // Dihedral reference found: 6 fixed dots
                    Sp3CandidateResult::Case1(Sp3Case1Result::FixedDots(
                        compute_sp3_case1_with_dihedral(anchor_pos, bond_dir, ref_perp, bond_dist),
                    ))
                } else {
                    // No dihedral reference: ring fallback
                    let (ring_center, ring_normal, ring_radius) =
                        compute_sp3_case1_ring(anchor_pos, bond_dir, bond_dist);
                    Sp3CandidateResult::Case1(Sp3Case1Result::FreeRing {
                        ring_center,
                        ring_normal,
                        ring_radius,
                    })
                }
            } else {
                // No neighbor found (shouldn't happen with 1 bond, but handle gracefully)
                Sp3CandidateResult::Dots(vec![])
            }
        }
        _ => Sp3CandidateResult::Dots(vec![]), // case 0: handled by caller (FreeSphere)
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
        let arb = if b1.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
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
// sp3 case 1: dihedral-aware + ring fallback
// ============================================================================

/// Result of the sp3 case 1 computation.
#[derive(Debug, Clone)]
pub enum Sp3Case1Result {
    /// Dihedral reference found: 6 fixed dots (3 trans + 3 cis).
    FixedDots(Vec<GuideDot>),
    /// No dihedral reference: free ring mode.
    FreeRing {
        ring_center: DVec3,
        ring_normal: DVec3,
        ring_radius: f64,
    },
}

/// Find a dihedral reference direction for sp3 case 1.
///
/// Walk upstream: anchor A has one neighbor B. If B has another neighbor C,
/// project B→C perpendicular to the A→B axis and return the normalized result.
pub fn find_dihedral_reference(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    neighbor_atom_id: u32,
) -> Option<DVec3> {
    let neighbor = structure.get_atom(neighbor_atom_id)?;
    let anchor = structure.get_atom(anchor_atom_id)?;

    let bond_axis = (neighbor.position - anchor.position).normalize();

    // Look for another neighbor of B (not A)
    for bond in &neighbor.bonds {
        if bond.is_delete_marker() {
            continue;
        }
        let other_id = bond.other_atom_id();
        if other_id == anchor_atom_id {
            continue;
        }
        if let Some(other_atom) = structure.get_atom(other_id) {
            let bc_dir = other_atom.position - neighbor.position;
            // Project perpendicular to bond axis
            let perp = bc_dir - bond_axis * bc_dir.dot(bond_axis);
            if perp.length_squared() > 1e-12 {
                return Some(perp.normalize());
            }
        }
    }

    None
}

/// Compute sp3 case 1 with dihedral reference: 6 guide dots.
///
/// 3 Primary (trans/staggered) at 60°, 180°, 300° offset from reference.
/// 3 Secondary (cis/eclipsed) at 0°, 120°, 240° offset from reference.
///
/// All positions lie on a cone with axis = -bond_dir and half-angle = 180° - 109.47° = 70.53°.
pub fn compute_sp3_case1_with_dihedral(
    anchor_pos: DVec3,
    bond_dir: DVec3,
    ref_perp: DVec3,
    bond_dist: f64,
) -> Vec<GuideDot> {
    // Cone axis points away from the existing bond
    let cone_axis = -bond_dir;
    // Half-angle of the cone from the cone axis: 180° - 109.47° = 70.53°
    let cone_half_angle = std::f64::consts::PI - TETRAHEDRAL_ANGLE; // ≈ 1.2310 rad (70.53°)
    let cos_cone = cone_half_angle.cos();
    let sin_cone = cone_half_angle.sin();

    // Build orthonormal basis in the plane perpendicular to the bond axis:
    // ref_perp is already perpendicular to bond_dir (from find_dihedral_reference)
    let u = ref_perp;
    let v = bond_dir.cross(u).normalize();

    let mut dots = Vec::with_capacity(6);

    // Trans (staggered) positions: 60°, 180°, 300° from reference
    for &angle_deg in &[60.0_f64, 180.0, 300.0] {
        let angle = angle_deg.to_radians();
        let (sin_a, cos_a) = angle.sin_cos();
        let radial = u * cos_a + v * sin_a;
        let dir = cone_axis * cos_cone + radial * sin_cone;
        dots.push(GuideDot {
            position: anchor_pos + dir.normalize() * bond_dist,
            dot_type: GuideDotType::Primary,
        });
    }

    // Cis (eclipsed) positions: 0°, 120°, 240° from reference
    for &angle_deg in &[0.0_f64, 120.0, 240.0] {
        let angle = angle_deg.to_radians();
        let (sin_a, cos_a) = angle.sin_cos();
        let radial = u * cos_a + v * sin_a;
        let dir = cone_axis * cos_cone + radial * sin_cone;
        dots.push(GuideDot {
            position: anchor_pos + dir.normalize() * bond_dist,
            dot_type: GuideDotType::Secondary,
        });
    }

    dots
}

/// Compute the ring geometry for sp3 case 1 without dihedral reference.
///
/// Ring center is along -bond_dir at the projection of bond_dist onto the cone axis.
/// Ring radius is the perpendicular component.
pub fn compute_sp3_case1_ring(
    anchor_pos: DVec3,
    bond_dir: DVec3,
    bond_dist: f64,
) -> (DVec3, DVec3, f64) {
    let cone_half_angle = std::f64::consts::PI - TETRAHEDRAL_ANGLE;
    let cos_cone = cone_half_angle.cos();
    let sin_cone = cone_half_angle.sin();

    let ring_normal = -bond_dir;
    let ring_center = anchor_pos + ring_normal * (bond_dist * cos_cone);
    let ring_radius = bond_dist * sin_cone;

    (ring_center, ring_normal, ring_radius)
}

/// Compute preview positions on the ring at equal angular spacing, anchored to a reference angle.
///
/// `point_on_ring` is the closest point on the ring to the cursor.
/// `num_dots` determines the number of positions (3 for sp3, 2 for sp2).
/// `cone_half_angle` is the half-angle of the cone that defines the ring.
pub fn compute_ring_preview_positions(
    ring_center: DVec3,
    ring_normal: DVec3,
    _ring_radius: f64,
    anchor_pos: DVec3,
    bond_distance: f64,
    point_on_ring: DVec3,
    num_dots: usize,
    cone_half_angle: f64,
) -> Vec<DVec3> {
    let radial = (point_on_ring - ring_center).normalize();
    let tangent = ring_normal.cross(radial).normalize();

    // We need positions at bond_distance from the anchor, not at ring_radius from ring_center.
    // Since the ring is the intersection of the cone with a plane, all ring points
    // are equidistant from the anchor. Compute the actual positions from the anchor.
    let cos_cone = cone_half_angle.cos();
    let sin_cone = cone_half_angle.sin();
    let cone_axis = ring_normal; // -bond_dir

    let angle_step = 360.0 / num_dots as f64;
    (0..num_dots)
        .map(|i| {
            let angle = (i as f64 * angle_step).to_radians();
            let (sin_a, cos_a) = angle.sin_cos();
            let r = radial * cos_a + tangent * sin_a;
            let dir = cone_axis * cos_cone + r * sin_cone;
            anchor_pos + dir.normalize() * bond_distance
        })
        .collect()
}

/// Compute the cone half-angle based on the number of ring dots.
///
/// sp3 (3 dots): cone half-angle = π - tetrahedral_angle ≈ 70.53°
/// sp2 (2 dots): cone half-angle = π - 120° = 60°
pub fn cone_half_angle_for_ring(num_ring_dots: usize) -> f64 {
    if num_ring_dots == 2 {
        std::f64::consts::PI - TRIGONAL_ANGLE // 60°
    } else {
        std::f64::consts::PI - TETRAHEDRAL_ANGLE // 70.53°
    }
}

/// Project a ray onto the ring plane and find the closest point on the ring circle.
///
/// Returns `None` if the ray is parallel to the ring plane.
pub fn ray_ring_nearest_point(
    ray_start: &DVec3,
    ray_dir: &DVec3,
    ring_center: &DVec3,
    ring_normal: &DVec3,
    ring_radius: f64,
) -> Option<DVec3> {
    // Intersect ray with the ring plane
    let denom = ray_dir.dot(*ring_normal);
    if denom.abs() < 1e-10 {
        return None; // Ray parallel to ring plane
    }
    let t = (*ring_center - *ray_start).dot(*ring_normal) / denom;

    // Allow slightly negative t for robustness (plane behind camera is still useful for projection)
    let plane_point = *ray_start + *ray_dir * t;

    // Project onto the ring circle
    let offset = plane_point - *ring_center;
    if offset.length_squared() < 1e-12 {
        // Ray hits ring center — pick an arbitrary point on the ring
        let arb = if ring_normal.x.abs() < 0.9 {
            DVec3::X
        } else {
            DVec3::Y
        };
        let radial = ring_normal.cross(arb).normalize();
        return Some(*ring_center + radial * ring_radius);
    }

    Some(*ring_center + offset.normalize() * ring_radius)
}

// ============================================================================
// sp2 candidate position computation (trigonal planar, 120°)
// ============================================================================

/// Trigonal planar angle in radians: 120° = 2π/3
const TRIGONAL_ANGLE: f64 = 2.0 * std::f64::consts::PI / 3.0;

/// Compute sp2 candidate positions for guided placement.
///
/// - Case 3 (saturated): empty dots
/// - Case 2 (1 remaining): single dot opposite the midpoint of existing bonds, in-plane
/// - Case 1 (2 remaining): planar-reference-aware (2 dots) or ring fallback
/// - Case 0: handled by caller (FreeSphere)
pub fn compute_sp2_candidates(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    anchor_pos: DVec3,
    existing_bond_dirs: &[DVec3],
    bond_dist: f64,
) -> Sp2CandidateResult {
    match existing_bond_dirs.len() {
        3.. => Sp2CandidateResult::Dots(vec![]), // saturated
        2 => Sp2CandidateResult::Dots(sp2_case2(anchor_pos, existing_bond_dirs, bond_dist)),
        1 => {
            let bond_dir = existing_bond_dirs[0];
            let neighbor_id = structure.get_atom(anchor_atom_id).and_then(|atom| {
                atom.bonds
                    .iter()
                    .find(|b| !b.is_delete_marker())
                    .map(|b| b.other_atom_id())
            });

            if let Some(neighbor_id) = neighbor_id {
                if let Some(plane_normal) =
                    find_sp2_planar_reference(structure, anchor_atom_id, neighbor_id, bond_dir)
                {
                    // Planar reference found: 2 fixed dots at ±120° in the plane
                    Sp2CandidateResult::Dots(compute_sp2_case1_with_reference(
                        anchor_pos,
                        bond_dir,
                        plane_normal,
                        bond_dist,
                    ))
                } else {
                    // No planar reference: ring fallback (cone half-angle = 60° from -bond_dir)
                    let (ring_center, ring_normal, ring_radius) =
                        compute_sp2_case1_ring(anchor_pos, bond_dir, bond_dist);
                    Sp2CandidateResult::FreeRing {
                        ring_center,
                        ring_normal,
                        ring_radius,
                    }
                }
            } else {
                Sp2CandidateResult::Dots(vec![])
            }
        }
        _ => Sp2CandidateResult::Dots(vec![]), // case 0: handled by caller (FreeSphere)
    }
}

/// Result of sp2 candidate computation, accounting for case 1 ring fallback.
#[derive(Debug, Clone)]
pub enum Sp2CandidateResult {
    /// Fixed guide dots at computed positions.
    Dots(Vec<GuideDot>),
    /// sp2 case 1 without planar reference: free ring mode.
    FreeRing {
        ring_center: DVec3,
        ring_normal: DVec3,
        ring_radius: f64,
    },
}

/// sp2 case 2: one remaining direction, opposite the midpoint of existing bonds.
/// The result naturally lies in the b1-b2 plane.
fn sp2_case2(anchor_pos: DVec3, dirs: &[DVec3], bond_dist: f64) -> Vec<GuideDot> {
    let sum = dirs[0] + dirs[1];
    let d3 = if sum.length_squared() < 1e-12 {
        // Degenerate: bonds cancel out (180° apart). Pick perpendicular.
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
        position: anchor_pos + d3 * bond_dist,
        dot_type: GuideDotType::Primary,
    }]
}

/// Find a planar reference for sp2 case 1.
///
/// Walk upstream: anchor A has one neighbor B. If B has another neighbor C,
/// compute the plane normal from the A-B-C triangle. The sp2 directions
/// must lie in this plane.
pub fn find_sp2_planar_reference(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    neighbor_atom_id: u32,
    bond_dir: DVec3,
) -> Option<DVec3> {
    let neighbor = structure.get_atom(neighbor_atom_id)?;

    // Look for another neighbor of B (not A)
    for bond in &neighbor.bonds {
        if bond.is_delete_marker() {
            continue;
        }
        let other_id = bond.other_atom_id();
        if other_id == anchor_atom_id {
            continue;
        }
        if let Some(other_atom) = structure.get_atom(other_id) {
            let anchor = structure.get_atom(anchor_atom_id)?;
            let ba = anchor.position - neighbor.position;
            let bc = other_atom.position - neighbor.position;
            let normal = ba.cross(bc);
            if normal.length_squared() > 1e-12 {
                return Some(normal.normalize());
            }
        }
    }

    // Also check anchor's other neighbors for a plane reference
    let anchor = structure.get_atom(anchor_atom_id)?;
    for bond in &anchor.bonds {
        if bond.is_delete_marker() {
            continue;
        }
        let other_id = bond.other_atom_id();
        if other_id == neighbor_atom_id {
            continue;
        }
        if let Some(other_atom) = structure.get_atom(other_id) {
            let dir_to_other = (other_atom.position - anchor.position).normalize();
            let normal = bond_dir.cross(dir_to_other);
            if normal.length_squared() > 1e-12 {
                return Some(normal.normalize());
            }
        }
    }

    None
}

/// Compute sp2 case 1 with planar reference: 2 guide dots at ±120° from existing bond.
///
/// Both positions lie in the plane defined by the bond direction and the plane normal.
fn compute_sp2_case1_with_reference(
    anchor_pos: DVec3,
    bond_dir: DVec3,
    plane_normal: DVec3,
    bond_dist: f64,
) -> Vec<GuideDot> {
    // Build an in-plane perpendicular to the bond direction
    let in_plane_perp = plane_normal.cross(bond_dir);
    let in_plane_perp = if in_plane_perp.length_squared() < 1e-12 {
        // plane_normal parallel to bond_dir — pick arbitrary perpendicular
        let arb = if bond_dir.x.abs() < 0.9 {
            DVec3::X
        } else {
            DVec3::Y
        };
        bond_dir.cross(arb).normalize()
    } else {
        in_plane_perp.normalize()
    };

    // Two directions at ±120° from bond_dir in the plane:
    // d = bond_dir * cos(120°) + in_plane_perp * sin(120°)
    let cos_120 = (TRIGONAL_ANGLE).cos(); // cos(120°) = -0.5
    let sin_120 = (TRIGONAL_ANGLE).sin(); // sin(120°) = √3/2

    let d1 = (bond_dir * cos_120 + in_plane_perp * sin_120).normalize();
    let d2 = (bond_dir * cos_120 - in_plane_perp * sin_120).normalize();

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

/// Compute the ring geometry for sp2 case 1 without planar reference.
///
/// Ring lies on a cone with half-angle 60° (= 180° - 120°) from -bond_dir.
pub fn compute_sp2_case1_ring(
    anchor_pos: DVec3,
    bond_dir: DVec3,
    bond_dist: f64,
) -> (DVec3, DVec3, f64) {
    let cone_half_angle = std::f64::consts::PI - TRIGONAL_ANGLE; // 180° - 120° = 60°
    let cos_cone = cone_half_angle.cos();
    let sin_cone = cone_half_angle.sin();

    let ring_normal = -bond_dir;
    let ring_center = anchor_pos + ring_normal * (bond_dist * cos_cone);
    let ring_radius = bond_dist * sin_cone;

    (ring_center, ring_normal, ring_radius)
}

/// Compute 2 preview positions on the sp2 ring at 180° spacing (opposite each other).
///
/// `point_on_ring` is the closest point on the ring to the cursor.
pub fn compute_sp2_ring_preview_positions(
    ring_center: DVec3,
    ring_normal: DVec3,
    anchor_pos: DVec3,
    bond_distance: f64,
    point_on_ring: DVec3,
) -> [DVec3; 2] {
    let radial = (point_on_ring - ring_center).normalize();
    let tangent = ring_normal.cross(radial).normalize();

    let cone_half_angle = std::f64::consts::PI - TRIGONAL_ANGLE;
    let cos_cone = cone_half_angle.cos();
    let sin_cone = cone_half_angle.sin();
    let cone_axis = ring_normal;

    let mut positions = [DVec3::ZERO; 2];
    for (i, &angle_deg) in [0.0_f64, 180.0].iter().enumerate() {
        let angle = angle_deg.to_radians();
        let (sin_a, cos_a) = angle.sin_cos();
        let r = radial * cos_a + tangent * sin_a;
        let dir = cone_axis * cos_cone + r * sin_cone;
        positions[i] = anchor_pos + dir.normalize() * bond_distance;
    }
    positions
}

// ============================================================================
// sp1 candidate position computation (linear, 180°)
// ============================================================================

/// Compute sp1 candidate positions for guided placement.
///
/// - Case 2 (saturated): empty dots
/// - Case 1 (1 remaining): single dot directly opposite the existing bond
/// - Case 0: handled by caller (FreeSphere)
pub fn compute_sp1_candidates(
    anchor_pos: DVec3,
    existing_bond_dirs: &[DVec3],
    bond_dist: f64,
) -> Vec<GuideDot> {
    match existing_bond_dirs.len() {
        2.. => vec![], // saturated
        1 => {
            let d2 = -existing_bond_dirs[0];
            vec![GuideDot {
                position: anchor_pos + d2 * bond_dist,
                dot_type: GuideDotType::Primary,
            }]
        }
        _ => vec![], // case 0: handled by caller (FreeSphere)
    }
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
    // Only compute if there are remaining slots
    let mode = if slots == 0 {
        GuidedPlacementMode::FixedDots { guide_dots: vec![] }
    } else if existing_bond_dirs.is_empty() {
        // Case 0: no existing bonds → free sphere placement
        GuidedPlacementMode::FreeSphere {
            center: anchor_pos,
            radius: bond_dist,
            preview_position: None,
        }
    } else {
        match hybridization {
            Hybridization::Sp3 => {
                match compute_sp3_candidates(
                    structure,
                    anchor_atom_id,
                    anchor_pos,
                    &existing_bond_dirs,
                    bond_dist,
                ) {
                    Sp3CandidateResult::Dots(guide_dots) => {
                        GuidedPlacementMode::FixedDots { guide_dots }
                    }
                    Sp3CandidateResult::Case1(Sp3Case1Result::FixedDots(guide_dots)) => {
                        GuidedPlacementMode::FixedDots { guide_dots }
                    }
                    Sp3CandidateResult::Case1(Sp3Case1Result::FreeRing {
                        ring_center,
                        ring_normal,
                        ring_radius,
                    }) => GuidedPlacementMode::FreeRing {
                        ring_center,
                        ring_normal,
                        ring_radius,
                        bond_distance: bond_dist,
                        anchor_pos,
                        num_ring_dots: 3,
                        preview_positions: None,
                    },
                }
            }
            Hybridization::Sp2 => {
                match compute_sp2_candidates(
                    structure,
                    anchor_atom_id,
                    anchor_pos,
                    &existing_bond_dirs,
                    bond_dist,
                ) {
                    Sp2CandidateResult::Dots(guide_dots) => {
                        GuidedPlacementMode::FixedDots { guide_dots }
                    }
                    Sp2CandidateResult::FreeRing {
                        ring_center,
                        ring_normal,
                        ring_radius,
                    } => GuidedPlacementMode::FreeRing {
                        ring_center,
                        ring_normal,
                        ring_radius,
                        bond_distance: bond_dist,
                        anchor_pos,
                        num_ring_dots: 2,
                        preview_positions: None,
                    },
                }
            }
            Hybridization::Sp1 => {
                let guide_dots = compute_sp1_candidates(anchor_pos, &existing_bond_dirs, bond_dist);
                GuidedPlacementMode::FixedDots { guide_dots }
            }
        }
    };

    GuidedPlacementResult {
        anchor_atom_id,
        hybridization,
        mode,
        bond_distance: bond_dist,
        remaining_slots: slots,
        has_additional_geometric_capacity: has_additional,
    }
}
