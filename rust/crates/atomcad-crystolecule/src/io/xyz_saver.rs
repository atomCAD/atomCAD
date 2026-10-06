use crate::atomic_constants::ATOM_INFO;
use crate::atomic_structure::AtomicStructure;
use std::fs::File;
use std::io::{self, Write};
use thiserror::Error;

/// Keyword of the optional trailer line listing frozen atoms, e.g.
/// `FREEZEXYZ 1 3`: the 1-based positions (in file order) of the atoms a
/// simulation should keep fixed. Written by [`save_xyz`] and read back by
/// `xyz_loader::load_xyz`.
pub const FREEZE_KEYWORD: &str = "FREEZEXYZ";

#[derive(Debug, Error)]
pub enum XyzSaveError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Element not found for atomic number: {0}")]
    ElementNotFound(i16),
}

/// Saves an AtomicStructure to an XYZ file
///
/// # Arguments
///
/// * `atomic_structure` - The atomic structure to save
/// * `file_path` - The path where the XYZ file should be saved
/// * `write_frozen` - When true and at least one atom is frozen, append a
///   [`FREEZE_KEYWORD`] line after the atom block. The indices are positions in
///   the file (1-based), not atom ids — ids can have gaps.
///
/// # Returns
///
/// * `Result<(), XyzSaveError>` - Ok(()) if successful, or an error if the operation fails
pub fn save_xyz(
    atomic_structure: &AtomicStructure,
    file_path: &str,
    write_frozen: bool,
) -> Result<(), XyzSaveError> {
    let mut file = File::create(file_path)?;

    // Write number of atoms
    writeln!(file, "{}", atomic_structure.get_num_of_atoms())?;

    // Write title/comment line (using the file name as title)
    let title = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Exported from atomCAD");
    writeln!(file, "{}", title)?;

    let mut frozen_indices: Vec<usize> = Vec::new();

    // Write atom data
    for (position, (_, atom)) in atomic_structure.iter_atoms().enumerate() {
        // Get element symbol from atomic number
        let atom_info = ATOM_INFO
            .get(&(atom.atomic_number as i32))
            .ok_or(XyzSaveError::ElementNotFound(atom.atomic_number))?;

        // Write element and position
        writeln!(
            file,
            "{} {:.6} {:.6} {:.6}",
            atom_info.symbol, atom.position.x, atom.position.y, atom.position.z
        )?;

        if atom.is_frozen() {
            frozen_indices.push(position + 1);
        }
    }

    if write_frozen && !frozen_indices.is_empty() {
        let indices: Vec<String> = frozen_indices.iter().map(|i| i.to_string()).collect();
        writeln!(file, "{} {}", FREEZE_KEYWORD, indices.join(" "))?;
    }

    Ok(())
}
