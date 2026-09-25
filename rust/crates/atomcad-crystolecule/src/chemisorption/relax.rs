//! UFF relaxation of one structure, with the per-term energies a chemist reads
//! to see *why* a candidate is strained.

use super::config::{ChemisorptionError, ChemisorptionSearch};
use crate::atomic_structure::AtomicStructure;
use crate::simulation::check_minimize_limits;
use crate::simulation::minimize::{MinimizationConfig, minimize_with_force_field};
use crate::simulation::topology::MolecularTopology;
use crate::simulation::uff::energy::{
    angle_bend_energy, bond_stretch_energy, inversion_energy, torsion_energy, vdw_energy,
};
use crate::simulation::uff::{UffForceField, VdwMode};
use glam::DVec3;

/// UFF energy split by term (kcal/mol). On a candidate these are differences
/// from the reference state, so they sum to its strain.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StrainTerms {
    pub stretch: f64,
    pub bend: f64,
    pub torsion: f64,
    pub inversion: f64,
    pub vdw: f64,
}

impl StrainTerms {
    pub fn total(&self) -> f64 {
        self.stretch + self.bend + self.torsion + self.inversion + self.vdw
    }

    pub fn minus(&self, other: &StrainTerms) -> StrainTerms {
        StrainTerms {
            stretch: self.stretch - other.stretch,
            bend: self.bend - other.bend,
            torsion: self.torsion - other.torsion,
            inversion: self.inversion - other.inversion,
            vdw: self.vdw - other.vdw,
        }
    }
}

/// One relaxation's outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct Relaxed {
    /// Absolute UFF energy over the interactions with at least one free atom
    /// (kcal/mol); only differences between structures of the same atoms mean
    /// anything.
    pub energy: f64,
    pub terms: StrainTerms,
    pub converged: bool,
    pub iterations: u32,
    /// Largest bond length / UFF rest length after relaxation.
    pub worst_bond_ratio: f64,
}

/// Relaxes `structure` in place, holding its frozen atoms fixed.
pub fn relax(
    structure: &mut AtomicStructure,
    config: &ChemisorptionSearch,
) -> Result<Relaxed, ChemisorptionError> {
    let num_atoms = structure.get_num_of_atoms();
    let num_free = structure.atoms_values().filter(|a| !a.is_frozen()).count();
    check_minimize_limits(num_atoms, num_free, &config.vdw_mode)
        .map_err(ChemisorptionError::Relaxation)?;

    let topology = match config.vdw_mode {
        VdwMode::AllPairs => MolecularTopology::from_structure(structure),
        VdwMode::Cutoff(_) => MolecularTopology::from_structure_bonded_only(structure),
    };
    let frozen: Vec<usize> = topology
        .atom_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| structure.get_atom(**id).is_some_and(|a| a.is_frozen()))
        .map(|(i, _)| i)
        .collect();
    let ff = UffForceField::from_topology_with_frozen(&topology, config.vdw_mode.clone(), &frozen)
        .map_err(ChemisorptionError::Relaxation)?;

    let mut p = topology.positions.clone();
    let minimization = MinimizationConfig {
        max_iterations: config.max_iterations,
        gradient_rms_tolerance: config.gradient_rms_tolerance,
        ..Default::default()
    };
    let result = minimize_with_force_field(&ff, &mut p, &minimization, &frozen);

    let terms = StrainTerms {
        stretch: ff
            .bond_params
            .iter()
            .map(|b| bond_stretch_energy(b, &p))
            .sum(),
        bend: ff
            .angle_params
            .iter()
            .map(|a| angle_bend_energy(a, &p))
            .sum(),
        torsion: ff
            .torsion_params
            .iter()
            .map(|t| torsion_energy(t, &p))
            .sum(),
        inversion: ff
            .inversion_params
            .iter()
            .map(|i| inversion_energy(i, &p))
            .sum(),
        vdw: ff.vdw_params().iter().map(|v| vdw_energy(v, &p)).sum(),
    };
    let at = |i: usize| DVec3::new(p[i * 3], p[i * 3 + 1], p[i * 3 + 2]);
    let worst_bond_ratio = ff
        .bond_params
        .iter()
        .map(|b| at(b.idx1).distance(at(b.idx2)) / b.rest_length)
        .fold(0.0, f64::max);

    for (i, id) in topology.atom_ids.iter().enumerate() {
        structure.set_atom_position(*id, at(i));
    }
    Ok(Relaxed {
        energy: result.energy,
        terms,
        converged: result.converged,
        iterations: result.iterations,
        worst_bond_ratio,
    })
}
