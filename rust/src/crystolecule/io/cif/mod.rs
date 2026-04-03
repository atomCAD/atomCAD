pub mod parser;
pub mod structure;
pub mod symmetry;

use glam::DVec3;
use std::fs;
use thiserror::Error;

use crate::crystolecule::atomic_constants::CHEMICAL_ELEMENTS;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;

use self::parser::CifParseError;
use self::structure::CifError as CifStructureError;
use self::symmetry::expand_asymmetric_unit;

/// Unified error type for CIF loading.
#[derive(Debug, Error)]
pub enum CifLoadError {
    #[error("I/O error reading CIF file: {0}")]
    Io(#[from] std::io::Error),

    #[error("CIF parse error: {0}")]
    Parse(#[from] CifParseError),

    #[error("CIF data extraction error: {0}")]
    Structure(#[from] CifStructureError),

    #[error("No data blocks found in CIF file")]
    NoDataBlocks,

    #[error("Data block '{requested}' not found. Available blocks: {available}")]
    BlockNotFound {
        requested: String,
        available: String,
    },
}

/// An atom site in the expanded conventional unit cell.
#[derive(Debug, Clone)]
pub struct ExpandedAtomSite {
    pub label: String,
    pub atomic_number: i16,
    pub fract: DVec3,
}

/// Result of loading a CIF file.
#[derive(Debug, Clone)]
pub struct CifLoadResult {
    pub unit_cell: UnitCellStruct,
    /// Full conventional cell atoms in fractional coordinates.
    pub atoms: Vec<ExpandedAtomSite>,
}

/// Load and process a CIF file. Returns unit cell and expanded atom sites.
///
/// The caller is responsible for converting to AtomicStructure/Motif and
/// for bond inference.
///
/// `block_name`: if Some, selects the data block by name; if None, uses the
/// first block. Returns an error listing available block names if the
/// requested name is not found.
pub fn load_cif(file_path: &str, block_name: Option<&str>) -> Result<CifLoadResult, CifLoadError> {
    let content = fs::read_to_string(file_path)?;
    load_cif_from_str(&content, block_name)
}

/// Load and process CIF data from a string. Same as `load_cif` but takes
/// the file content directly (useful for testing without file I/O).
pub fn load_cif_from_str(
    content: &str,
    block_name: Option<&str>,
) -> Result<CifLoadResult, CifLoadError> {
    let document = parser::parse_cif(content)?;

    if document.data_blocks.is_empty() {
        return Err(CifLoadError::NoDataBlocks);
    }

    // Select the data block
    let block = if let Some(name) = block_name {
        document
            .data_blocks
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                let available = document
                    .data_blocks
                    .iter()
                    .map(|b| b.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                CifLoadError::BlockNotFound {
                    requested: name.to_string(),
                    available,
                }
            })?
    } else {
        &document.data_blocks[0]
    };

    // Extract crystal data (unit cell, asymmetric atoms, symmetry operations)
    let crystal_data = self::structure::extract_crystal_data(block)?;

    // Expand asymmetric unit using symmetry operations
    let expanded_atoms = expand_asymmetric_unit(
        &crystal_data.asymmetric_atoms,
        &crystal_data.symmetry_operations,
        0.01, // fractional coordinate tolerance for deduplication
    );

    // Convert to ExpandedAtomSite with atomic numbers
    let atoms = expanded_atoms
        .into_iter()
        .map(|site| {
            let atomic_number = CHEMICAL_ELEMENTS
                .get(&site.element)
                .copied()
                .unwrap_or(0) as i16;
            ExpandedAtomSite {
                label: site.label,
                atomic_number,
                fract: site.fract,
            }
        })
        .collect();

    Ok(CifLoadResult {
        unit_cell: crystal_data.unit_cell,
        atoms,
    })
}
