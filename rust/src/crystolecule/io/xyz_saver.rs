use std::fs::File;
use std::io::{self, Write};
use thiserror::Error;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_constants::ATOM_INFO;

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
///
/// # Returns
///
/// * `Result<(), XyzSaveError>` - Ok(()) if successful, or an error if the operation fails
pub fn save_xyz(atomic_structure: &AtomicStructure, file_path: &str) -> Result<(), XyzSaveError> {
    let mut file = File::create(file_path)?;
    
    // Write number of atoms
    writeln!(file, "{}", atomic_structure.get_num_of_atoms())?;
    
    // Write title/comment line (using the file name as title)
    let title = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Exported from atomCAD");
    writeln!(file, "{}", title)?;
    
    // Write atom data
    for (_, atom) in atomic_structure.iter_atoms() {
        // Get element symbol from atomic number
        let atom_info = ATOM_INFO
            .get(&(atom.atomic_number as i32))
            .ok_or_else(|| XyzSaveError::ElementNotFound(atom.atomic_number))?;
        
        // Write element and position
        writeln!(
            file,
            "{} {:.6} {:.6} {:.6}",
            atom_info.symbol,
            atom.position.x,
            atom.position.y,
            atom.position.z
        )?;
    }
    
    Ok(())
}
