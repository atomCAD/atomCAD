use crate::crystolecule::crystolecule_constants::{
    DEFAULT_ZINCBLENDE_MOTIF, DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
};
use crate::crystolecule::motif::Motif;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use glam::f64::DVec3;

/// Crystal structure: lattice vectors + motif + motif offset.
///
/// Aggregates the three ingredients that define an infinite repeating atomic
/// pattern. Corresponds to the `Structure` value type in the node network.
#[derive(Debug, Clone)]
pub struct Structure {
    pub lattice_vecs: UnitCellStruct,
    pub motif: Motif,
    pub motif_offset: DVec3,
}

impl Structure {
    /// Constructs a structure from lattice vectors only, using the default
    /// motif (zincblende) and zero offset. Used by primitive nodes in phase 4,
    /// which still take a `LatticeVecs` input rather than a full `Structure`.
    /// Phase 5 removes this usage when primitives gain a `Structure` input.
    pub fn from_lattice_vecs(lattice_vecs: UnitCellStruct) -> Self {
        Structure {
            lattice_vecs,
            motif: DEFAULT_ZINCBLENDE_MOTIF.clone(),
            motif_offset: DVec3::ZERO,
        }
    }

    /// The default diamond structure: cubic diamond lattice + zincblende motif
    /// + zero offset. Used as the fallback when a `structure` node has no
    /// base and no per-field overrides.
    pub fn diamond() -> Self {
        let size = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
        let lattice_vecs = UnitCellStruct {
            a: DVec3::new(size, 0.0, 0.0),
            b: DVec3::new(0.0, size, 0.0),
            c: DVec3::new(0.0, 0.0, size),
            cell_length_a: size,
            cell_length_b: size,
            cell_length_c: size,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        };
        Structure {
            lattice_vecs,
            motif: DEFAULT_ZINCBLENDE_MOTIF.clone(),
            motif_offset: DVec3::ZERO,
        }
    }
}
