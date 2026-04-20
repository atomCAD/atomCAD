//! Detects whether a rotation picked from a unit cell's symmetry axes is also
//! a symmetry of the motif (sites + bonds) that decorates the lattice.
//!
//! The axes returned by `analyze_unit_cell_symmetries` always preserve the
//! Bravais lattice; this module answers the stricter question of whether the
//! full motif is preserved. See `doc/design_blueprint_alignment.md` §5.

use crate::crystolecule::motif::Motif;
use crate::crystolecule::structure::Structure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::unit_cell_symmetries::{RotationalSymmetry, analyze_unit_cell_symmetries};
use glam::f64::{DQuat, DVec3};
use glam::i32::IVec3;

/// Tolerance for fractional-coordinate comparisons. Rotated site positions,
/// after reducing mod 1, must fall within this distance (componentwise max) of
/// a motif site to count as a match.
const FRAC_TOL: f64 = 1e-6;

/// Returns `true` iff the rotation specified by `(axis_index, step)` — applied
/// around `pivot_point` — maps every motif site and bond to itself modulo
/// lattice translations.
///
/// Preconditions:
/// * `axis_index` indexes into `analyze_unit_cell_symmetries(lattice)`. Callers
///   that already normalise the index (e.g. the `structure_rot` node) should
///   pass the raw user value; this function applies the same wrap-around rule
///   as `structure_rot` for consistency.
/// * An identity rotation (no axes, `axis_index == None`, or `step` folds to 0)
///   trivially preserves the motif and returns `true`.
pub fn rotation_preserves_motif(
    structure: &Structure,
    axis_index: Option<i32>,
    step: i32,
    pivot_point: IVec3,
) -> bool {
    let symmetry_axes = analyze_unit_cell_symmetries(&structure.lattice_vecs);
    let Some(selected) = select_axis(axis_index, &symmetry_axes) else {
        return true;
    };

    let safe_step = wrap_step(step, selected.n_fold);
    if safe_step == 0 {
        return true;
    }

    let angle = selected.smallest_angle_radians() * safe_step as f64;
    let rotation = DQuat::from_axis_angle(selected.axis, angle);

    let lattice = &structure.lattice_vecs;
    let motif = &structure.motif;
    let pivot_real = lattice.ivec3_lattice_to_real(&pivot_point);

    // Image of each canonical-cell site (site_index → (image site, cell shift)).
    let mut site_images: Vec<(usize, IVec3)> = Vec::with_capacity(motif.sites.len());
    for (i, _) in motif.sites.iter().enumerate() {
        match image_of_site(i, IVec3::ZERO, motif, lattice, structure.motif_offset, pivot_real, rotation) {
            Some(img) => site_images.push(img),
            None => return false,
        }
    }

    // Every rotated bond must exist in the motif (as an unordered pair, modulo
    // lattice translation).
    for bond in &motif.bonds {
        let img1 = match image_of_site(
            bond.site_1.site_index,
            bond.site_1.relative_cell,
            motif,
            lattice,
            structure.motif_offset,
            pivot_real,
            rotation,
        ) {
            Some(v) => v,
            None => return false,
        };
        let img2 = match image_of_site(
            bond.site_2.site_index,
            bond.site_2.relative_cell,
            motif,
            lattice,
            structure.motif_offset,
            pivot_real,
            rotation,
        ) {
            Some(v) => v,
            None => return false,
        };

        if !motif_has_bond(motif, img1, img2, bond.multiplicity) {
            return false;
        }
    }

    true
}

fn select_axis(
    axis_index: Option<i32>,
    symmetry_axes: &[RotationalSymmetry],
) -> Option<&RotationalSymmetry> {
    if symmetry_axes.is_empty() {
        return None;
    }
    let axis_index = axis_index?;
    let n = symmetry_axes.len() as i32;
    let idx = ((axis_index % n) + n) % n;
    Some(&symmetry_axes[idx as usize])
}

fn wrap_step(step: i32, n_fold: u32) -> i32 {
    let n = n_fold as i32;
    ((step % n) + n) % n
}

/// Computes the image of the motif site at `(site_index, cell_offset)` under
/// the real-space rotation around `pivot_real`. The image is expressed as
/// `(site index, lattice cell offset)` of another motif site that the rotated
/// real position coincides with (within `FRAC_TOL`). Returns `None` if no such
/// motif site exists, or if the image site has a different atomic number.
fn image_of_site(
    site_index: usize,
    cell_offset: IVec3,
    motif: &Motif,
    lattice: &UnitCellStruct,
    motif_offset: DVec3,
    pivot_real: DVec3,
    rotation: DQuat,
) -> Option<(usize, IVec3)> {
    let site = &motif.sites[site_index];
    let fractional = site.position + cell_offset.as_dvec3();
    let real_pos = lattice.dvec3_lattice_to_real(&fractional) + motif_offset;
    let rotated_real = rotation * (real_pos - pivot_real) + pivot_real;
    let rotated_frac = lattice.real_to_dvec3_lattice(&(rotated_real - motif_offset));

    let floor_cell = IVec3::new(
        rotated_frac.x.floor() as i32,
        rotated_frac.y.floor() as i32,
        rotated_frac.z.floor() as i32,
    );
    let reduced = rotated_frac - floor_cell.as_dvec3();

    for (j, candidate) in motif.sites.iter().enumerate() {
        if candidate.atomic_number != site.atomic_number {
            continue;
        }
        let diff = reduced - candidate.position;
        // Fold `diff` into [-0.5, 0.5]^3; `rounding` captures any whole-cell
        // shift needed (occurs when the image straddles a cell boundary).
        let rounding = IVec3::new(
            diff.x.round() as i32,
            diff.y.round() as i32,
            diff.z.round() as i32,
        );
        let folded = diff - rounding.as_dvec3();
        if folded.x.abs() < FRAC_TOL && folded.y.abs() < FRAC_TOL && folded.z.abs() < FRAC_TOL {
            return Some((j, floor_cell + rounding));
        }
    }
    None
}

/// Returns true iff the motif contains a bond connecting the two endpoints
/// with the given multiplicity. Endpoints are unordered and compared modulo a
/// common lattice translation.
fn motif_has_bond(
    motif: &Motif,
    endpoint1: (usize, IVec3),
    endpoint2: (usize, IVec3),
    multiplicity: i32,
) -> bool {
    let (site_a, cell_a) = endpoint1;
    let (site_b, cell_b) = endpoint2;
    let offset_ab = cell_b - cell_a;
    let offset_ba = cell_a - cell_b;

    // Narrow the scan with the precomputed site1 → bond index map when possible.
    for &bond_idx in motif.bonds_by_site1_index[site_a].iter() {
        let bond = &motif.bonds[bond_idx];
        if bond.multiplicity != multiplicity {
            continue;
        }
        if bond.site_2.site_index == site_b
            && (bond.site_2.relative_cell - bond.site_1.relative_cell) == offset_ab
        {
            return true;
        }
    }
    for &bond_idx in motif.bonds_by_site1_index[site_b].iter() {
        let bond = &motif.bonds[bond_idx];
        if bond.multiplicity != multiplicity {
            continue;
        }
        if bond.site_2.site_index == site_a
            && (bond.site_2.relative_cell - bond.site_1.relative_cell) == offset_ba
        {
            return true;
        }
    }
    false
}
