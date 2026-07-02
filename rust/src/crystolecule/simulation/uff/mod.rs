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
use typer::{assign_uff_types_with_overrides, bond_order_to_f64, hybridization_from_label};

use std::cell::{Cell, RefCell};

use crate::crystolecule::atomic_structure::InlineBond;
use crate::crystolecule::simulation::force_field::ForceField;
use crate::crystolecule::simulation::spatial_grid::SpatialGrid;
use crate::crystolecule::simulation::topology::MolecularTopology;
use rustc_hash::{FxHashMap, FxHashSet};

/// Strategy for computing van der Waals (nonbonded) interactions.
#[derive(Debug, Clone)]
pub enum VdwMode {
    /// Compute all pre-enumerated nonbonded pairs (exact, O(N^2) per step).
    AllPairs,
    /// Use spatial grid with distance cutoff (approximate, O(N*k) per step).
    /// The `f64` value is the cutoff radius in Angstroms.
    Cutoff(f64),
}

/// How often the cutoff neighbor list is rebuilt (in energy evaluations).
const CUTOFF_REBUILD_INTERVAL: u32 = 10;

/// Internal storage for the chosen vdW strategy.
enum VdwStrategy {
    /// All nonbonded pairs from the topology (exact, O(N^2) construction).
    AllPairs { params: Vec<VdwParams> },
    /// Cutoff with periodic neighbor list rebuilds via interior mutability.
    /// The pair list is rebuilt from a spatial grid every `rebuild_interval`
    /// energy evaluations to track atom movement during minimization.
    Cutoff {
        params: RefCell<Vec<VdwParams>>,
        eval_count: Cell<u32>,
        rebuild_interval: u32,
        build_radius: f64,
        atom_vdw_x: Vec<f64>,
        atom_vdw_d: Vec<f64>,
        exclusions: FxHashSet<(usize, usize)>,
        /// Indices of atoms that can move. Rebuilds scan only these.
        free_indices: Vec<usize>,
        /// Grid over frozen atoms only, built once — frozen atoms never
        /// move, so their cell assignments stay valid for the whole run.
        frozen_grid: SpatialGrid,
    },
}

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
    /// Van der Waals strategy (all-pairs or cutoff).
    vdw_strategy: VdwStrategy,
    /// Number of atoms.
    pub num_atoms: usize,
}

impl UffForceField {
    /// Constructs a UFF force field from a molecular topology using all-pairs
    /// vdW computation (the default, exact O(N^2) mode).
    ///
    /// Equivalent to `from_topology_with_vdw_mode(topology, VdwMode::AllPairs)`.
    pub fn from_topology(topology: &MolecularTopology) -> Result<Self, String> {
        Self::from_topology_with_vdw_mode(topology, VdwMode::AllPairs)
    }

    /// Constructs a UFF force field with a configurable vdW strategy.
    ///
    /// No frozen atoms — all pairs are included. See
    /// `from_topology_with_frozen` to exclude frozen-frozen pairs.
    pub fn from_topology_with_vdw_mode(
        topology: &MolecularTopology,
        vdw_mode: VdwMode,
    ) -> Result<Self, String> {
        Self::from_topology_with_frozen(topology, vdw_mode, &[])
    }

    /// Constructs a UFF force field with a configurable vdW strategy and
    /// frozen atom support.
    ///
    /// Interactions whose atoms are **all** frozen are skipped everywhere:
    /// bonds, angles, torsions, inversions, and vdW pairs (in both AllPairs
    /// and Cutoff modes). Such terms exert zero force on every free atom and
    /// contribute only a constant energy offset, so dropping them does not
    /// change where any free atom ends up — but the reported energy becomes
    /// "energy of all interactions involving at least one free atom". This
    /// can dramatically reduce the per-iteration cost when most atoms are
    /// frozen (e.g., FreezeBase, or a relax of a small unfrozen pocket in a
    /// large frozen structure).
    pub fn from_topology_with_frozen(
        topology: &MolecularTopology,
        vdw_mode: VdwMode,
        frozen: &[usize],
    ) -> Result<Self, String> {
        let num_atoms = topology.num_atoms;
        if num_atoms == 0 {
            return Ok(Self {
                bond_params: Vec::new(),
                angle_params: Vec::new(),
                torsion_params: Vec::new(),
                inversion_params: Vec::new(),
                vdw_strategy: VdwStrategy::AllPairs { params: Vec::new() },
                num_atoms: 0,
            });
        }

        // Per-atom frozen flags, shared by all filter points below.
        // Interactions whose atoms are *all* frozen exert zero force on every
        // free atom and contribute only a constant energy offset, so they are
        // skipped when pre-computing parameters. Note that the topology's
        // bond list itself is NOT filtered: the atom typer needs each atom's
        // complete bond environment (a frozen boundary atom's UFF type
        // depends on all its bonds), and torsion force-constant scaling
        // counts all torsions about a central bond, filtered or not.
        let mut frozen_flags = vec![false; num_atoms];
        for &idx in frozen {
            if idx < num_atoms {
                frozen_flags[idx] = true;
            }
        }

        // Step 1: Build per-atom bond lists for the atom typer.
        let mut atom_bonds: Vec<Vec<InlineBond>> = vec![Vec::new(); num_atoms];
        for bond in &topology.bonds {
            atom_bonds[bond.idx1].push(InlineBond::new(bond.idx2 as u32, bond.bond_order));
            atom_bonds[bond.idx2].push(InlineBond::new(bond.idx1 as u32, bond.bond_order));
        }

        // Step 2: Assign UFF atom types (respecting per-atom hybridization overrides).
        let bond_slices: Vec<&[InlineBond]> = atom_bonds.iter().map(|v| v.as_slice()).collect();
        let typing = assign_uff_types_with_overrides(
            &topology.atomic_numbers,
            &bond_slices,
            &topology.hybridization_overrides,
        )?;

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
            .filter(|bond| !(frozen_flags[bond.idx1] && frozen_flags[bond.idx2]))
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
                if frozen_flags[angle.idx1] && frozen_flags[angle.idx2] && frozen_flags[angle.idx3]
                {
                    return None;
                }
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
        let torsion_params =
            Self::compute_torsion_params(topology, &typing, &bond_order_map, &frozen_flags);

        // Step 7: Pre-compute inversion parameters.
        let inversion_params = Self::compute_inversion_params(topology, &typing, &frozen_flags);

        // Step 8: Build vdW strategy based on the chosen mode.
        let vdw_strategy = match vdw_mode {
            VdwMode::AllPairs => {
                let vdw_params: Vec<VdwParams> = topology
                    .nonbonded_pairs
                    .iter()
                    .filter(|pair| !(frozen_flags[pair.idx1] && frozen_flags[pair.idx2]))
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
                VdwStrategy::AllPairs { params: vdw_params }
            }
            VdwMode::Cutoff(radius) => {
                // Pre-compute the cutoff pair list using a spatial grid.
                // Uses a 3 Å skin beyond the cutoff so the list stays valid
                // between rebuilds. With max_displacement = 0.3 Å/iteration
                // and rebuild every 10 evaluations, one atom can move up to
                // 3 Å — the skin covers this. Pairs in the skin zone are
                // kept in the list but skipped at evaluation time via a
                // runtime distance check against cutoff_radius_sq.
                let skin = 3.0;
                let build_radius = radius + skin;

                // Per-atom vdW parameters for combination rules.
                let atom_vdw_x: Vec<f64> = (0..num_atoms)
                    .map(|i| params::get_uff_params(typing.labels[i]).unwrap().x1)
                    .collect();
                let atom_vdw_d: Vec<f64> = (0..num_atoms)
                    .map(|i| params::get_uff_params(typing.labels[i]).unwrap().d1)
                    .collect();

                // Build 1-2 and 1-3 exclusion set.
                let mut exclusions: FxHashSet<(usize, usize)> = FxHashSet::default();
                for bond in &topology.bonds {
                    let key = (bond.idx1.min(bond.idx2), bond.idx1.max(bond.idx2));
                    exclusions.insert(key);
                }
                for angle in &topology.angles {
                    let key = (angle.idx1.min(angle.idx3), angle.idx1.max(angle.idx3));
                    exclusions.insert(key);
                }

                // Partition atoms into free and frozen. The frozen grid is
                // built once here — frozen atoms never move — so periodic
                // pair-list rebuilds only construct a grid over the free
                // atoms, making rebuilds O(N_free) instead of O(N_total).
                let free_indices: Vec<usize> =
                    (0..num_atoms).filter(|&i| !frozen_flags[i]).collect();
                let frozen_indices: Vec<usize> =
                    (0..num_atoms).filter(|&i| frozen_flags[i]).collect();
                let frozen_grid = SpatialGrid::from_positions_subset(
                    &topology.positions,
                    &frozen_indices,
                    build_radius,
                );

                // Build initial pair list from spatial grids.
                let vdw_params = Self::build_cutoff_pairs(
                    &topology.positions,
                    &free_indices,
                    &frozen_grid,
                    build_radius,
                    &atom_vdw_x,
                    &atom_vdw_d,
                    &exclusions,
                );

                VdwStrategy::Cutoff {
                    params: RefCell::new(vdw_params),
                    eval_count: Cell::new(0),
                    rebuild_interval: CUTOFF_REBUILD_INTERVAL,
                    build_radius,
                    atom_vdw_x,
                    atom_vdw_d,
                    exclusions,
                    free_indices,
                    frozen_grid,
                }
            }
        };

        Ok(Self {
            bond_params,
            angle_params,
            torsion_params,
            inversion_params,
            vdw_strategy,
            num_atoms,
        })
    }

    /// Returns the pre-computed vdW parameter list (AllPairs mode only).
    ///
    /// # Panics
    ///
    /// Panics if the force field was built with `VdwMode::Cutoff`.
    pub fn vdw_params(&self) -> &[VdwParams] {
        match &self.vdw_strategy {
            VdwStrategy::AllPairs { params } => params,
            VdwStrategy::Cutoff { .. } => {
                panic!("vdw_params() is not available in Cutoff mode")
            }
        }
    }

    /// Returns the current vdW pair list as normalized `(min_idx, max_idx)`
    /// index pairs, in either mode. In Cutoff mode this reflects the most
    /// recent (re)build. Intended for introspection and tests.
    pub fn vdw_pair_indices(&self) -> Vec<(usize, usize)> {
        let normalize = |vp: &VdwParams| (vp.idx1.min(vp.idx2), vp.idx1.max(vp.idx2));
        match &self.vdw_strategy {
            VdwStrategy::AllPairs { params } => params.iter().map(normalize).collect(),
            VdwStrategy::Cutoff { params, .. } => params.borrow().iter().map(normalize).collect(),
        }
    }

    /// Returns the pair-list build radius (cutoff + skin) in Cutoff mode,
    /// or `None` in AllPairs mode. Intended for introspection and tests.
    pub fn cutoff_build_radius(&self) -> Option<f64> {
        match &self.vdw_strategy {
            VdwStrategy::AllPairs { .. } => None,
            VdwStrategy::Cutoff { build_radius, .. } => Some(*build_radius),
        }
    }

    /// Builds a vdW pair list from current positions using a two-grid scan.
    ///
    /// Only pairs with at least one free endpoint are wanted (frozen–frozen
    /// gradients would be zeroed by the minimizer anyway), so the scan is
    /// centered on free atoms only: a fresh grid over the free atoms is
    /// built here (O(N_free)), while the grid over the frozen atoms is the
    /// cached one built at construction (frozen atoms never move). For each
    /// free atom, the free grid is scanned with the `j > i` dedup (each
    /// free–free pair found once, from its lower index) and the frozen grid
    /// with unconditional acceptance (each free–frozen pair found once,
    /// from its free center). Frozen–frozen pairs are never visited.
    fn build_cutoff_pairs(
        positions: &[f64],
        free_indices: &[usize],
        frozen_grid: &SpatialGrid,
        build_radius: f64,
        atom_vdw_x: &[f64],
        atom_vdw_d: &[f64],
        exclusions: &FxHashSet<(usize, usize)>,
    ) -> Vec<VdwParams> {
        let free_grid = SpatialGrid::from_positions_subset(positions, free_indices, build_radius);
        let mut vdw_params: Vec<VdwParams> = Vec::new();
        let mut push_pair = |i: usize, j: usize| {
            vdw_params.push(VdwParams {
                idx1: i,
                idx2: j,
                x_ij: (atom_vdw_x[i] * atom_vdw_x[j]).sqrt(),
                d_ij: (atom_vdw_d[i] * atom_vdw_d[j]).sqrt(),
            });
        };
        for &i in free_indices {
            free_grid.for_each_neighbor(positions, i, build_radius, |j| {
                if j > i && !exclusions.contains(&(i, j)) {
                    push_pair(i, j);
                }
            });
            frozen_grid.for_each_neighbor(positions, i, build_radius, |j| {
                if !exclusions.contains(&(i.min(j), i.max(j))) {
                    push_pair(i, j);
                }
            });
        }
        vdw_params
    }

    /// Pre-computes torsion angle parameters with per-central-bond scaling.
    ///
    /// Two passes:
    /// 1. Build all valid torsion contributions (filtering to SP2/SP3 central atoms)
    /// 2. Count torsions per central bond and scale each force constant by 1/count
    ///
    /// This matches RDKit's `scaleForceConstant(contribsHere.size())` in Builder.cpp.
    ///
    /// Torsions whose four atoms are all frozen are dropped **after** the
    /// count-and-scale pass: a central bond can host both mixed and
    /// all-frozen torsions, and dropping the all-frozen ones before counting
    /// would inflate the surviving torsions' force constants — changing
    /// forces on free atoms.
    fn compute_torsion_params(
        topology: &MolecularTopology,
        typing: &typer::AtomTypeAssignment,
        bond_order_map: &FxHashMap<(usize, usize), f64>,
        frozen: &[bool],
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

        // Drop all-frozen torsions only now that counting is done.
        raw_torsions
            .retain(|t| !(frozen[t.idx1] && frozen[t.idx2] && frozen[t.idx3] && frozen[t.idx4]));

        raw_torsions
    }

    /// Pre-computes inversion (out-of-plane) parameters.
    ///
    /// Detects whether sp2 carbon centers are bound to sp2 oxygen (for the
    /// enhanced K=50 case in amide/carboxyl groups).
    fn compute_inversion_params(
        topology: &MolecularTopology,
        typing: &typer::AtomTypeAssignment,
        frozen: &[bool],
    ) -> Vec<InversionParams> {
        topology
            .inversions
            .iter()
            .filter(|inv| {
                !(frozen[inv.idx1] && frozen[inv.idx2] && frozen[inv.idx3] && frozen[inv.idx4])
            })
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

        // Van der Waals (nonbonded) contributions.
        match &self.vdw_strategy {
            VdwStrategy::AllPairs { params } => {
                for vp in params {
                    *energy += vdw_energy_and_gradient(vp, positions, gradients);
                }
            }
            VdwStrategy::Cutoff {
                params,
                eval_count,
                rebuild_interval,
                build_radius,
                atom_vdw_x,
                atom_vdw_d,
                exclusions,
                free_indices,
                frozen_grid,
            } => {
                // Periodically rebuild the neighbor list from current positions.
                let count = eval_count.get();
                if count > 0 && count % rebuild_interval == 0 {
                    let new_params = Self::build_cutoff_pairs(
                        positions,
                        free_indices,
                        frozen_grid,
                        *build_radius,
                        atom_vdw_x,
                        atom_vdw_d,
                        exclusions,
                    );
                    *params.borrow_mut() = new_params;
                }
                eval_count.set(count + 1);

                // Evaluate all pairs in the list. The effective cutoff is
                // build_radius (cutoff + skin). We must NOT filter by a
                // smaller radius here — a hard distance check would create
                // a discontinuity in the energy surface (pairs popping
                // in/out) that breaks L-BFGS convergence.
                for vp in params.borrow().iter() {
                    *energy += vdw_energy_and_gradient(vp, positions, gradients);
                }
            }
        }
    }
}
