//! Supercell operation: rewrites a `Structure` (lattice vectors + motif + motif
//! offset) as an equivalent `Structure` with a larger unit cell, driven by a 3×3
//! integer matrix `M`. The physical atom pattern (the "crystal field") is
//! unchanged; only the representation changes. Design: `doc/design_supercell_node.md`.

use crate::crystolecule::motif::{Motif, MotifBond, Site, SiteSpecifier};
use crate::crystolecule::structure::Structure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use glam::f64::{DMat3, DVec3};
use glam::i32::IVec3;
use std::collections::HashMap;
use thiserror::Error;

/// Default cap on the number of sites in the resulting motif. Guards against
/// accidental inputs like `(100, 100, 100)` that would hang the UI.
pub const MAX_SUPERCELL_SITES: usize = 10_000;

/// Boundary tolerance for the half-open `[0, 1)³` containment test on
/// new-fractional coordinates, and the floor epsilon used to pick the new cell
/// in bond reduction. Both values use the same `EPS` so step 3 (containment)
/// and step 4 (bond reduction) agree on which new cell a boundary atom belongs
/// to.
const EPS: f64 = 1e-9;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SupercellError {
    #[error("supercell matrix is degenerate (rows are linearly dependent)")]
    Degenerate,
    #[error("supercell matrix has negative determinant {det} (left-handed basis is not supported)")]
    InvertedHandedness { det: i64 },
    #[error("supercell would have {site_count} sites, exceeding the limit of {limit}")]
    TooLarge { site_count: u64, limit: usize },
}

/// Applies a supercell transformation to `structure` using `matrix` (rows give
/// the new basis vectors as integer combinations of the old basis). The result
/// has `|det(matrix)|` times as many sites and bonds, and represents the same
/// infinite atom pattern.
pub fn apply_supercell(
    structure: &Structure,
    matrix: &[[i32; 3]; 3],
) -> Result<Structure, SupercellError> {
    // ---- 1. Validate ----
    let det = det_i64(matrix);
    if det == 0 {
        return Err(SupercellError::Degenerate);
    }
    if det < 0 {
        return Err(SupercellError::InvertedHandedness { det });
    }
    let n = det as u64; // det > 0 here
    let n_sites = structure.motif.sites.len() as u64;
    let projected_sites = n.saturating_mul(n_sites);
    if projected_sites > MAX_SUPERCELL_SITES as u64 {
        return Err(SupercellError::TooLarge {
            site_count: projected_sites,
            limit: MAX_SUPERCELL_SITES,
        });
    }

    // ---- 2. New lattice vectors ----
    let a = structure.lattice_vecs.a;
    let b = structure.lattice_vecs.b;
    let c = structure.lattice_vecs.c;
    let row_vec = |row: [i32; 3]| row[0] as f64 * a + row[1] as f64 * b + row[2] as f64 * c;
    let new_a = row_vec(matrix[0]);
    let new_b = row_vec(matrix[1]);
    let new_c = row_vec(matrix[2]);
    let new_lattice_vecs = UnitCellStruct::new(new_a, new_b, new_c);

    // ---- 3. Enumerate site copies, build new_sites and site_map ----
    // `minv_t` maps old fractional coords → new fractional coords. See design doc
    // section "Coordinate transforms" for the derivation.
    let minv_t: DMat3 = dmat3_from_i32(matrix).inverse().transpose();

    let (aabb_min, aabb_max) = new_cell_aabb(matrix);
    // Low side -1 catches cells whose sites (position < 1 componentwise) protrude
    // into the new cell. Upper bound is tight: a cell at aabb_max + 1 can never
    // contribute because site.position < 1.
    let scan_min = aabb_min - IVec3::ONE;
    let scan_max = aabb_max;

    let n_new_sites_expected = (n as usize).saturating_mul(n_sites as usize);
    let mut site_map: HashMap<(IVec3, usize), usize> = HashMap::with_capacity(n_new_sites_expected);
    let mut new_sites: Vec<Site> = Vec::with_capacity(n_new_sites_expected);

    for pz in scan_min.z..=scan_max.z {
        for py in scan_min.y..=scan_max.y {
            for px in scan_min.x..=scan_max.x {
                let p = IVec3::new(px, py, pz);
                let p_real = p.as_dvec3();
                for (s_idx, site) in structure.motif.sites.iter().enumerate() {
                    let old_pos = p_real + site.position;
                    let new_frac = minv_t * old_pos;
                    if in_unit_cube_half_open(new_frac, EPS) {
                        site_map.insert((p, s_idx), new_sites.len());
                        new_sites.push(Site {
                            atomic_number: site.atomic_number,
                            position: snap_unit(new_frac),
                        });
                    }
                }
            }
        }
    }

    debug_assert_eq!(new_sites.len(), n_new_sites_expected);

    // ---- 4. Rebuild bonds ----
    let n_new_bonds_expected = (n as usize).saturating_mul(structure.motif.bonds.len());
    let mut new_bonds: Vec<MotifBond> = Vec::with_capacity(n_new_bonds_expected);

    for bond in &structure.motif.bonds {
        debug_assert!(bond.site_1.relative_cell == IVec3::ZERO);
        let s1_idx = bond.site_1.site_index;
        let s2_idx = bond.site_2.site_index;
        let s2_rel = bond.site_2.relative_cell;
        let s2_frac = structure.motif.sites[s2_idx].position;

        for (&(p_1, s_idx), &new_site_1_idx) in &site_map {
            if s_idx != s1_idx {
                continue;
            }

            // End-2 in old-integer / fractional coords before reduction.
            let end_2_cell: IVec3 = p_1 + s2_rel;
            let end_2_atom_pos: DVec3 = end_2_cell.as_dvec3() + s2_frac;

            // Pick the new cell this atom belongs to by flooring its
            // new-fractional position. Adding EPS makes the floor consistent
            // with the `[−eps, 1−eps)` containment rule in step 3: an atom at
            // new_frac = -1e-12 (just below the 0 face) stays in cell 0 rather
            // than flipping to cell -1.
            let new_cell_idx: IVec3 = dvec3_floor(minv_t * end_2_atom_pos + DVec3::splat(EPS));

            // Reduce end_2_cell by Mᵀ · new_cell_idx new-cell translations.
            let end_2_reduced: IVec3 = end_2_cell - m_transpose_mul_ivec3(matrix, new_cell_idx);

            let new_site_2_idx = *site_map.get(&(end_2_reduced, s2_idx)).expect(
                "bond endpoint resolution failed: reduced site is not in site_map. \
                 This indicates a numerical-epsilon mismatch between steps 3 and 4.",
            );

            new_bonds.push(MotifBond {
                site_1: SiteSpecifier {
                    site_index: new_site_1_idx,
                    relative_cell: IVec3::ZERO,
                },
                site_2: SiteSpecifier {
                    site_index: new_site_2_idx,
                    relative_cell: new_cell_idx,
                },
                multiplicity: bond.multiplicity,
            });
        }
    }

    debug_assert_eq!(new_bonds.len(), n_new_bonds_expected);

    // Rebuild the per-site bond indices, matching the motif parser's layout.
    let num_new_sites = new_sites.len();
    let mut bonds_by_site1_index: Vec<Vec<usize>> = vec![Vec::new(); num_new_sites];
    let mut bonds_by_site2_index: Vec<Vec<usize>> = vec![Vec::new(); num_new_sites];
    for (bond_index, bond) in new_bonds.iter().enumerate() {
        bonds_by_site1_index[bond.site_1.site_index].push(bond_index);
        bonds_by_site2_index[bond.site_2.site_index].push(bond_index);
    }

    // ---- 5. Assemble ----
    let new_motif_offset = snap_unit(minv_t * structure.motif_offset);

    Ok(Structure {
        lattice_vecs: new_lattice_vecs,
        motif: Motif {
            parameters: structure.motif.parameters.clone(),
            sites: new_sites,
            bonds: new_bonds,
            bonds_by_site1_index,
            bonds_by_site2_index,
        },
        motif_offset: new_motif_offset,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Computes `det(M)` in `i64` to avoid overflow for inputs like `diag(1000, 1000, 1000)`.
fn det_i64(m: &[[i32; 3]; 3]) -> i64 {
    let m00 = m[0][0] as i64;
    let m01 = m[0][1] as i64;
    let m02 = m[0][2] as i64;
    let m10 = m[1][0] as i64;
    let m11 = m[1][1] as i64;
    let m12 = m[1][2] as i64;
    let m20 = m[2][0] as i64;
    let m21 = m[2][1] as i64;
    let m22 = m[2][2] as i64;
    m00 * (m11 * m22 - m12 * m21) - m01 * (m10 * m22 - m12 * m20) + m02 * (m10 * m21 - m11 * m20)
}

/// Axis-aligned bounding box of the new unit cell, expressed in old-integer
/// coordinates. Returns `(min, max)` over the 8 corners of the parallelepiped
/// spanned by the rows of `m` applied at the origin.
fn new_cell_aabb(m: &[[i32; 3]; 3]) -> (IVec3, IVec3) {
    let rows = [IVec3::from(m[0]), IVec3::from(m[1]), IVec3::from(m[2])];
    let corners: [IVec3; 8] = [
        IVec3::ZERO,
        rows[0],
        rows[1],
        rows[2],
        rows[0] + rows[1],
        rows[0] + rows[2],
        rows[1] + rows[2],
        rows[0] + rows[1] + rows[2],
    ];
    let mut aabb_min = corners[0];
    let mut aabb_max = corners[0];
    for c in &corners[1..] {
        aabb_min = aabb_min.min(*c);
        aabb_max = aabb_max.max(*c);
    }
    (aabb_min, aabb_max)
}

/// Half-open unit-cube containment with tolerance: accept iff each component is
/// in `[-eps, 1 - eps)`. Sites on the lower face are included, on the upper
/// face excluded.
fn in_unit_cube_half_open(v: DVec3, eps: f64) -> bool {
    let upper = 1.0 - eps;
    v.x >= -eps && v.x < upper && v.y >= -eps && v.y < upper && v.z >= -eps && v.z < upper
}

/// Snaps components that are within `EPS` of 0 or 1 to exactly 0, so recorded
/// fractional coordinates are clean on cell boundaries. Does NOT reduce
/// arbitrary values mod 1.
fn snap_unit(v: DVec3) -> DVec3 {
    let snap = |x: f64| {
        if x.abs() < EPS || (x - 1.0).abs() < EPS {
            0.0
        } else {
            x
        }
    };
    DVec3::new(snap(v.x), snap(v.y), snap(v.z))
}

/// Componentwise floor converted to `IVec3`.
fn dvec3_floor(v: DVec3) -> IVec3 {
    IVec3::new(v.x.floor() as i32, v.y.floor() as i32, v.z.floor() as i32)
}

/// Returns `Mᵀ · v` in `IVec3`, using `i64` accumulators internally to avoid
/// overflow when `|det(M)|` is large.
fn m_transpose_mul_ivec3(m: &[[i32; 3]; 3], v: IVec3) -> IVec3 {
    let vx = v.x as i64;
    let vy = v.y as i64;
    let vz = v.z as i64;
    let rx = m[0][0] as i64 * vx + m[1][0] as i64 * vy + m[2][0] as i64 * vz;
    let ry = m[0][1] as i64 * vx + m[1][1] as i64 * vy + m[2][1] as i64 * vz;
    let rz = m[0][2] as i64 * vx + m[1][2] as i64 * vy + m[2][2] as i64 * vz;
    IVec3::new(rx as i32, ry as i32, rz as i32)
}

/// Builds a `DMat3` whose columns are the actual columns of `M`. Then
/// `dmat3_from_i32(m) * v` equals `M · v`, so `dmat3_from_i32(m).inverse()` is
/// `M⁻¹` and `.inverse().transpose()` is `M⁻ᵀ` — the old→new fractional map.
fn dmat3_from_i32(m: &[[i32; 3]; 3]) -> DMat3 {
    DMat3::from_cols(
        DVec3::new(m[0][0] as f64, m[1][0] as f64, m[2][0] as f64),
        DVec3::new(m[0][1] as f64, m[1][1] as f64, m[2][1] as f64),
        DVec3::new(m[0][2] as f64, m[1][2] as f64, m[2][2] as f64),
    )
}
