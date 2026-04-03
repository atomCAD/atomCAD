use crate::crystolecule::atomic_constants::{ATOM_INFO, DEFAULT_ATOM_INFO};
use crate::crystolecule::motif::{MotifBond, ParameterElement, Site, SiteSpecifier};
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use glam::f64::DVec3;
use glam::i32::IVec3;
use std::collections::HashSet;

/// Infer bonds for a motif based on interatomic distances in real (Cartesian) space.
///
/// For each pair of sites (i, j) and each neighboring cell offset (dx, dy, dz) in {-1, 0, +1},
/// a bond is created when the Cartesian distance is within `(r_i + r_j) * tolerance_multiplier`,
/// where r_i and r_j are covalent radii.
///
/// Cross-cell bonds are included (e.g., a bond from site i in cell (0,0,0) to site j in
/// cell (1,0,0)). Duplicate bonds are eliminated via canonical ordering: the bond
/// (i, (0,0,0)) → (j, (dx,dy,dz)) is equivalent to (j, (0,0,0)) → (i, (-dx,-dy,-dz)).
///
/// The returned bonds always have `site_1.relative_cell == (0,0,0)`.
/// Results are sorted deterministically by (site_1.site_index, site_2.site_index, offset).
pub fn infer_motif_bonds(
    sites: &[Site],
    parameters: &[ParameterElement],
    unit_cell: &UnitCellStruct,
    tolerance_multiplier: f64,
) -> Vec<MotifBond> {
    let n = sites.len();

    // Resolve covalent radii for each site
    let radii: Vec<f64> = sites
        .iter()
        .map(|s| {
            let z = resolve_atomic_number(s, parameters);
            ATOM_INFO
                .get(&z)
                .unwrap_or(&DEFAULT_ATOM_INFO)
                .covalent_radius
        })
        .collect();

    let mut bonds = Vec::new();
    let mut seen: HashSet<(usize, usize, i32, i32, i32)> = HashSet::new();

    for i in 0..n {
        let pos_i = unit_cell.dvec3_lattice_to_real(&sites[i].position);

        for j in 0..n {
            for dx in -1..=1_i32 {
                for dy in -1..=1_i32 {
                    for dz in -1..=1_i32 {
                        // Skip self-bond in same cell
                        if i == j && dx == 0 && dy == 0 && dz == 0 {
                            continue;
                        }

                        // Canonical key to avoid duplicate bonds.
                        // Bond (i,(0,0,0))→(j,(dx,dy,dz)) == (j,(0,0,0))→(i,(-dx,-dy,-dz))
                        let key = canonical_bond_key(i, j, dx, dy, dz);

                        if seen.contains(&key) {
                            continue;
                        }

                        // Compute Cartesian position of site j in offset cell
                        let fract_j_offset = DVec3::new(
                            sites[j].position.x + dx as f64,
                            sites[j].position.y + dy as f64,
                            sites[j].position.z + dz as f64,
                        );
                        let pos_j = unit_cell.dvec3_lattice_to_real(&fract_j_offset);

                        let distance = DVec3::distance(pos_i, pos_j);
                        let max_bond_distance = (radii[i] + radii[j]) * tolerance_multiplier;

                        if distance <= max_bond_distance {
                            seen.insert(key);
                            bonds.push(MotifBond {
                                site_1: SiteSpecifier {
                                    site_index: i,
                                    relative_cell: IVec3::ZERO,
                                },
                                site_2: SiteSpecifier {
                                    site_index: j,
                                    relative_cell: IVec3::new(dx, dy, dz),
                                },
                                multiplicity: 1,
                            });
                        }
                    }
                }
            }
        }
    }

    // Sort for deterministic output
    bonds.sort_by(|a, b| {
        a.site_1
            .site_index
            .cmp(&b.site_1.site_index)
            .then(a.site_2.site_index.cmp(&b.site_2.site_index))
            .then(a.site_2.relative_cell.x.cmp(&b.site_2.relative_cell.x))
            .then(a.site_2.relative_cell.y.cmp(&b.site_2.relative_cell.y))
            .then(a.site_2.relative_cell.z.cmp(&b.site_2.relative_cell.z))
    });

    bonds
}

/// Compute canonical key for bond deduplication.
fn canonical_bond_key(i: usize, j: usize, dx: i32, dy: i32, dz: i32) -> (usize, usize, i32, i32, i32) {
    if i < j {
        (i, j, dx, dy, dz)
    } else if i > j {
        (j, i, -dx, -dy, -dz)
    } else {
        // i == j: pick the lexicographically larger offset direction
        if (dx, dy, dz) >= (-dx, -dy, -dz) {
            (i, j, dx, dy, dz)
        } else {
            (j, i, -dx, -dy, -dz)
        }
    }
}

/// Resolve the atomic number for a site, handling parameter element references.
fn resolve_atomic_number(site: &Site, parameters: &[ParameterElement]) -> i32 {
    if site.atomic_number < 0 {
        let param_index = (-site.atomic_number - 1) as usize;
        parameters
            .get(param_index)
            .map(|p| p.default_atomic_number as i32)
            .unwrap_or(6) // fallback to carbon
    } else {
        site.atomic_number as i32
    }
}
