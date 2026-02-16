pub mod energy;
pub mod params;
pub mod typer;

// UFF (Universal Force Field) implementation.
//
// Implements the force field described in:
// Rappé et al., "UFF, a Full Periodic Table Force Field for Molecular Mechanics
// and Molecular Dynamics Simulations", JACS 1992, 114, 10024-10035.
//
// Ported from RDKit's modular UFF implementation, cross-referenced with OpenBabel.

use energy::{
    AngleBendParams, BondStretchParams, InversionParams, TorsionAngleParams, VdwParams,
    angle_bend_energy_and_gradient, bond_stretch_energy_and_gradient,
    inversion_energy_and_gradient, torsion_energy_and_gradient, vdw_energy_and_gradient,
};
use params::{
    Hybridization, calc_angle_force_constant, calc_bond_force_constant, calc_bond_rest_length,
    calc_inversion_coefficients_and_force_constant, calc_torsion_params, calc_vdw_distance,
    calc_vdw_well_depth,
};
use typer::{assign_uff_types, bond_order_to_f64, hybridization_from_label};

use crate::crystolecule::atomic_structure::InlineBond;
use crate::crystolecule::simulation::force_field::ForceField;
use crate::crystolecule::simulation::topology::MolecularTopology;
use rustc_hash::FxHashMap;

/// UFF force field with pre-computed interaction parameters.
///
/// Constructed from a `MolecularTopology`. All parameters are pre-computed
/// so that `energy_and_gradients()` is just arithmetic over the stored params.
///
/// Ported from RDKit's Builder.cpp (BSD-3-Clause).
pub struct UffForceField {
    /// Pre-computed bond stretch parameters.
    pub bond_params: Vec<BondStretchParams>,
    /// Pre-computed angle bend parameters.
    pub angle_params: Vec<AngleBendParams>,
    /// Pre-computed torsion angle parameters.
    pub torsion_params: Vec<TorsionAngleParams>,
    /// Pre-computed inversion parameters.
    pub inversion_params: Vec<InversionParams>,
    /// Pre-computed van der Waals parameters.
    pub vdw_params: Vec<VdwParams>,
    /// Number of atoms.
    pub num_atoms: usize,
}

impl UffForceField {
    /// Constructs a UFF force field from a molecular topology.
    ///
    /// Assigns UFF atom types, then pre-computes all interaction parameters
    /// (bond stretch, angle bend, torsion, inversion). Returns an error if
    /// any atom cannot be typed or has no UFF parameters.
    ///
    /// The construction follows RDKit's Builder.cpp logic:
    /// - Bond stretch: harmonic potential using calc_bond_rest_length and calc_bond_force_constant
    /// - Angle bend: coordination order from vertex hybridization (SP→1, SP2→3, SP3D2→4, else→0)
    /// - Torsion: only for SP2/SP3 central atoms; force constant scaled by number of torsions
    ///   about the same central bond (matching RDKit's scaleForceConstant)
    /// - Inversion: C/N/O sp2 and group 15 centers with 3 bonds; detects C=O for enhanced K
    pub fn from_topology(topology: &MolecularTopology) -> Result<Self, String> {
        let num_atoms = topology.num_atoms;
        if num_atoms == 0 {
            return Ok(Self {
                bond_params: Vec::new(),
                angle_params: Vec::new(),
                torsion_params: Vec::new(),
                inversion_params: Vec::new(),
                vdw_params: Vec::new(),
                num_atoms: 0,
            });
        }

        // Step 1: Build per-atom bond lists for the atom typer.
        let mut atom_bonds: Vec<Vec<InlineBond>> = vec![Vec::new(); num_atoms];
        for bond in &topology.bonds {
            atom_bonds[bond.idx1].push(InlineBond::new(bond.idx2 as u32, bond.bond_order));
            atom_bonds[bond.idx2].push(InlineBond::new(bond.idx1 as u32, bond.bond_order));
        }

        // Step 2: Assign UFF atom types.
        let bond_slices: Vec<&[InlineBond]> = atom_bonds.iter().map(|v| v.as_slice()).collect();
        let typing = assign_uff_types(&topology.atomic_numbers, &bond_slices)?;

        // Step 3: Build bond order lookup: (min_idx, max_idx) → f64 bond order.
        let mut bond_order_map: FxHashMap<(usize, usize), f64> = FxHashMap::default();
        for bond in &topology.bonds {
            let key = (bond.idx1.min(bond.idx2), bond.idx1.max(bond.idx2));
            bond_order_map.insert(key, bond_order_to_f64(bond.bond_order));
        }

        // Step 4: Pre-compute bond stretch parameters.
        let bond_params: Vec<BondStretchParams> = topology
            .bonds
            .iter()
            .map(|bond| {
                let bo = bond_order_map
                    .get(&(bond.idx1.min(bond.idx2), bond.idx1.max(bond.idx2)))
                    .copied()
                    .unwrap_or(1.0);
                let p1 = typing.params[bond.idx1];
                let p2 = typing.params[bond.idx2];
                let rest_length = calc_bond_rest_length(bo, p1, p2);
                let force_constant = calc_bond_force_constant(rest_length, p1, p2);
                BondStretchParams {
                    idx1: bond.idx1,
                    idx2: bond.idx2,
                    rest_length,
                    force_constant,
                }
            })
            .collect();

        // Step 5: Pre-compute angle bend parameters.
        // Coordination order from vertex hybridization (RDKit Builder.cpp addAngles):
        //   SP (hyb=1) → order 1 (linear)
        //   SP2 (hyb=2, includes _R) → order 3 (trigonal planar)
        //   SP3D2 (hyb=6) → order 4 (square planar / octahedral)
        //   Everything else (SP3, etc.) → order 0 (general Fourier)
        let angle_params: Vec<AngleBendParams> = topology
            .angles
            .iter()
            .filter_map(|angle| {
                let p1 = typing.params[angle.idx1];
                let p2 = typing.params[angle.idx2]; // vertex
                let p3 = typing.params[angle.idx3];

                let hyb = hybridization_from_label(typing.labels[angle.idx2]);
                let order: u32 = match hyb {
                    1 => 1, // sp → linear
                    2 => 3, // sp2 (includes _R aromatic) → trigonal
                    6 => 4, // sp3d2 → square planar
                    _ => 0, // sp3, sp3d, others → general
                };

                let bo12 = bond_order_map
                    .get(&(angle.idx1.min(angle.idx2), angle.idx1.max(angle.idx2)))
                    .copied()
                    .unwrap_or(1.0);
                let bo23 = bond_order_map
                    .get(&(angle.idx2.min(angle.idx3), angle.idx2.max(angle.idx3)))
                    .copied()
                    .unwrap_or(1.0);

                let theta0 = p2.theta0.to_radians();
                let ka = calc_angle_force_constant(theta0, bo12, bo23, p1, p2, p3);

                if ka <= 0.0 {
                    return None;
                }

                Some(AngleBendParams::new(
                    angle.idx1, angle.idx2, angle.idx3, ka, theta0, order,
                ))
            })
            .collect();

        // Step 6: Pre-compute torsion parameters.
        // RDKit only adds torsions for central atoms that are SP2 or SP3.
        // Force constant is divided by the number of torsions about each central bond.
        let torsion_params = Self::compute_torsion_params(topology, &typing, &bond_order_map);

        // Step 7: Pre-compute inversion parameters.
        let inversion_params = Self::compute_inversion_params(topology, &typing);

        // Step 8: Pre-compute van der Waals parameters for all nonbonded pairs.
        let vdw_params: Vec<VdwParams> = topology
            .nonbonded_pairs
            .iter()
            .map(|pair| {
                let params_i = params::get_uff_params(typing.labels[pair.idx1]).unwrap();
                let params_j = params::get_uff_params(typing.labels[pair.idx2]).unwrap();
                VdwParams {
                    idx1: pair.idx1,
                    idx2: pair.idx2,
                    x_ij: calc_vdw_distance(params_i, params_j),
                    d_ij: calc_vdw_well_depth(params_i, params_j),
                }
            })
            .collect();

        Ok(Self {
            bond_params,
            angle_params,
            torsion_params,
            inversion_params,
            vdw_params,
            num_atoms,
        })
    }

    /// Pre-computes torsion angle parameters with per-central-bond scaling.
    ///
    /// Two passes:
    /// 1. Build all valid torsion contributions (filtering to SP2/SP3 central atoms)
    /// 2. Count torsions per central bond and scale each force constant by 1/count
    ///
    /// This matches RDKit's `scaleForceConstant(contribsHere.size())` in Builder.cpp.
    fn compute_torsion_params(
        topology: &MolecularTopology,
        typing: &typer::AtomTypeAssignment,
        bond_order_map: &FxHashMap<(usize, usize), f64>,
    ) -> Vec<TorsionAngleParams> {
        // First pass: compute raw torsion contributions.
        let mut raw_torsions: Vec<TorsionAngleParams> = Vec::new();

        for torsion in &topology.torsions {
            let hyb2 = hybridization_from_label(typing.labels[torsion.idx2]);
            let hyb3 = hybridization_from_label(typing.labels[torsion.idx3]);

            // RDKit only adds torsions where both central atoms are SP2 or SP3.
            let hyb2_enum = match hyb2 {
                2 => Hybridization::SP2,
                3 => Hybridization::SP3,
                _ => continue,
            };
            let hyb3_enum = match hyb3 {
                2 => Hybridization::SP2,
                3 => Hybridization::SP3,
                _ => continue,
            };

            let at_num2 = topology.atomic_numbers[torsion.idx2] as i32;
            let at_num3 = topology.atomic_numbers[torsion.idx3] as i32;

            let bo23 = bond_order_map
                .get(&(
                    torsion.idx2.min(torsion.idx3),
                    torsion.idx2.max(torsion.idx3),
                ))
                .copied()
                .unwrap_or(1.0);

            // Check if either end atom is sp2 (for the sp2-sp3 propene-like special case).
            let hyb1 = hybridization_from_label(typing.labels[torsion.idx1]);
            let hyb4 = hybridization_from_label(typing.labels[torsion.idx4]);
            let end_atom_is_sp2 = hyb1 == 2 || hyb4 == 2;

            let p2 = typing.params[torsion.idx2];
            let p3 = typing.params[torsion.idx3];

            let tp = calc_torsion_params(
                bo23,
                at_num2,
                at_num3,
                hyb2_enum,
                hyb3_enum,
                p2,
                p3,
                end_atom_is_sp2,
            );

            raw_torsions.push(TorsionAngleParams {
                idx1: torsion.idx1,
                idx2: torsion.idx2,
                idx3: torsion.idx3,
                idx4: torsion.idx4,
                params: tp,
            });
        }

        // Second pass: count torsions per central bond, then scale force constants.
        let mut count_per_bond: FxHashMap<(usize, usize), usize> = FxHashMap::default();
        for t in &raw_torsions {
            let key = (t.idx2.min(t.idx3), t.idx2.max(t.idx3));
            *count_per_bond.entry(key).or_insert(0) += 1;
        }
        for t in &mut raw_torsions {
            let key = (t.idx2.min(t.idx3), t.idx2.max(t.idx3));
            let count = count_per_bond.get(&key).copied().unwrap_or(1);
            if count > 1 {
                t.params.force_constant /= count as f64;
            }
        }

        raw_torsions
    }

    /// Pre-computes inversion (out-of-plane) parameters.
    ///
    /// Detects whether sp2 carbon centers are bound to sp2 oxygen (for the
    /// enhanced K=50 case in amide/carboxyl groups).
    fn compute_inversion_params(
        topology: &MolecularTopology,
        typing: &typer::AtomTypeAssignment,
    ) -> Vec<InversionParams> {
        topology
            .inversions
            .iter()
            .map(|inv| {
                let at2_atomic_num = topology.atomic_numbers[inv.idx2] as i32;

                // Check if central atom is sp2 carbon bound to sp2 oxygen
                let is_c_bound_to_o = at2_atomic_num == 6
                    && [inv.idx1, inv.idx3, inv.idx4].iter().any(|&nbr_idx| {
                        topology.atomic_numbers[nbr_idx] == 8
                            && hybridization_from_label(typing.labels[nbr_idx]) == 2
                    });

                let (k, c0, c1, c2) =
                    calc_inversion_coefficients_and_force_constant(at2_atomic_num, is_c_bound_to_o);

                InversionParams {
                    idx1: inv.idx1,
                    idx2: inv.idx2,
                    idx3: inv.idx3,
                    idx4: inv.idx4,
                    force_constant: k,
                    c0,
                    c1,
                    c2,
                }
            })
            .collect()
    }
}

impl ForceField for UffForceField {
    fn energy_and_gradients(&self, positions: &[f64], energy: &mut f64, gradients: &mut [f64]) {
        *energy = 0.0;
        for g in gradients.iter_mut() {
            *g = 0.0;
        }

        // Bond stretch contributions
        for bp in &self.bond_params {
            *energy += bond_stretch_energy_and_gradient(bp, positions, gradients);
        }

        // Angle bend contributions
        for ap in &self.angle_params {
            *energy += angle_bend_energy_and_gradient(ap, positions, gradients);
        }

        // Torsion angle contributions
        for tp in &self.torsion_params {
            *energy += torsion_energy_and_gradient(tp, positions, gradients);
        }

        // Inversion (out-of-plane) contributions
        for ip in &self.inversion_params {
            *energy += inversion_energy_and_gradient(ip, positions, gradients);
        }

        // Van der Waals (nonbonded) contributions
        for vp in &self.vdw_params {
            *energy += vdw_energy_and_gradient(vp, positions, gradients);
        }
    }
}
