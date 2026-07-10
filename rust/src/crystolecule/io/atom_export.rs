//! Single source of truth for atom-export formats.
//!
//! Exporting an atomic structure is one operation with a format axis, not N
//! operations. The supported-format set is consulted in several places (the
//! `export_atoms` node's eval dispatch and subtitle, the *Export visible* menu
//! action, and the Flutter Browse dialog / format indicator). To prevent those
//! from drifting apart, the format enum, its extension/label/description
//! metadata, and the extension→saver dispatch all live here, beside the savers
//! they dispatch to. Adding a new format (e.g. `.pdb`) is one enum arm plus one
//! saver.

use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::io::mol_exporter::{MolSaveError, save_mol_v3000};
use crate::crystolecule::io::xyz_saver::{XyzSaveError, save_xyz};
use thiserror::Error;

/// A supported atom-export file format. The format is derived from the file
/// extension of the destination path, never stored separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomExportFormat {
    Xyz,
    Mol,
}

/// Error from saving an atomic structure through [`AtomExportFormat::save`].
#[derive(Debug, Error)]
pub enum AtomExportError {
    #[error(transparent)]
    Xyz(#[from] XyzSaveError),
    #[error(transparent)]
    Mol(#[from] MolSaveError),
}

impl AtomExportFormat {
    /// Every supported format, in a stable order. UI listings and the
    /// human-readable extension list are derived from this.
    pub const ALL: &[AtomExportFormat] = &[AtomExportFormat::Xyz, AtomExportFormat::Mol];

    /// Case-insensitive match on the path's final extension.
    ///
    /// Returns `None` for a missing or unrecognized extension. Only the last
    /// path component's extension is considered, so a dotted directory name
    /// (`C:\my.dir\file`) does not spuriously match.
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = std::path::Path::new(path).extension()?.to_str()?;
        Self::ALL
            .iter()
            .copied()
            .find(|fmt| ext.eq_ignore_ascii_case(fmt.extension()))
    }

    /// The canonical file extension without a leading dot (`"xyz"` / `"mol"`).
    pub fn extension(&self) -> &'static str {
        match self {
            AtomExportFormat::Xyz => "xyz",
            AtomExportFormat::Mol => "mol",
        }
    }

    /// A short human-readable label for the format (`"XYZ"` / `"MOL (V3000)"`).
    pub fn label(&self) -> &'static str {
        match self {
            AtomExportFormat::Xyz => "XYZ",
            AtomExportFormat::Mol => "MOL (V3000)",
        }
    }

    /// A one-line description of what the format captures.
    pub fn description(&self) -> &'static str {
        match self {
            AtomExportFormat::Xyz => "Atomic coordinates only",
            AtomExportFormat::Mol => "Molecular structure with bond information",
        }
    }

    /// Human-readable, comma-separated list of supported extensions for error
    /// messages and UI (`".xyz, .mol"`). Rendered from [`Self::ALL`] so it can
    /// never disagree with the dispatch.
    pub fn supported_extensions_display() -> String {
        Self::ALL
            .iter()
            .map(|fmt| format!(".{}", fmt.extension()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Saves `structure` to `path` in this format.
    pub fn save(&self, structure: &AtomicStructure, path: &str) -> Result<(), AtomExportError> {
        match self {
            AtomExportFormat::Xyz => save_xyz(structure, path)?,
            AtomExportFormat::Mol => save_mol_v3000(structure, path)?,
        }
        Ok(())
    }
}
