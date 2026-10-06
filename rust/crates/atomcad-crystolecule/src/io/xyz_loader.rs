use crate::atomic_constants::CHEMICAL_ELEMENTS;
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure_utils::auto_create_bonds;
use crate::io::xyz_saver::FREEZE_KEYWORD;
use glam::f64::DVec3;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::num::ParseFloatError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XyzError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid XYZ format: {0}")]
    Parse(String),

    #[error("Invalid floating point number: {0}")]
    FloatParse(#[from] ParseFloatError),
}

/// Loads an XYZ file. An optional `FREEZEXYZ i j …` line after the atom block
/// (1-based atom positions, see `xyz_saver::FREEZE_KEYWORD`) marks those atoms
/// frozen; blank lines after the atom block are ignored.
pub fn load_xyz(file_path: &str, create_bonds: bool) -> Result<AtomicStructure, XyzError> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut atomic_structure = AtomicStructure::new();

    // Read the first line (number of atoms)
    let num_atoms: usize = lines
        .next()
        .ok_or_else(|| XyzError::Parse("Missing number of atoms".to_string()))??
        .trim()
        .parse()
        .map_err(|_| XyzError::Parse("Invalid number of atoms".to_string()))?;

    // Read the second line (title/comment)
    lines
        .next()
        .ok_or_else(|| XyzError::Parse("Missing title/comment".to_string()))??;

    let mut atom_ids: Vec<u32> = Vec::with_capacity(num_atoms);
    for (index, line) in lines.enumerate() {
        let line = line?;
        let line_number = index + 3;
        let parts: Vec<&str> = line.split_whitespace().collect();

        if atom_ids.len() == num_atoms {
            // Past the atom block: only the optional frozen-atom trailer and
            // blank lines may follow.
            if parts.is_empty() {
                continue;
            }
            if parts[0] == FREEZE_KEYWORD {
                for token in &parts[1..] {
                    let atom_index = token
                        .parse::<usize>()
                        .ok()
                        .filter(|i| (1..=num_atoms).contains(i))
                        .ok_or_else(|| {
                            XyzError::Parse(format!(
                                "Invalid {} index '{}' on line {} (expected 1..={})",
                                FREEZE_KEYWORD, token, line_number, num_atoms
                            ))
                        })?;
                    atomic_structure.set_atom_frozen(atom_ids[atom_index - 1], true);
                }
                continue;
            }
            if parts.len() == 4 {
                return Err(XyzError::Parse(format!(
                    "Expected {} atoms, but found more",
                    num_atoms
                )));
            }
            return Err(XyzError::Parse(format!(
                "Unexpected content after the atom block on line {}: {}",
                line_number, line
            )));
        }

        if parts.len() != 4 {
            return Err(XyzError::Parse(format!(
                "Invalid atom format on line {}: {}",
                line_number, line
            )));
        }

        let element = parts[0].to_string();
        let atomic_number = *CHEMICAL_ELEMENTS.get(&element).unwrap_or(&1) as i16; // TODO: error for unknown elements
        let x: f64 = parts[1].parse()?;
        let y: f64 = parts[2].parse()?;
        let z: f64 = parts[3].parse()?;

        atom_ids.push(atomic_structure.add_atom(atomic_number, DVec3::new(x, y, z)));
    }

    if atom_ids.len() != num_atoms {
        return Err(XyzError::Parse(format!(
            "Expected {} atoms, but found {}",
            num_atoms,
            atom_ids.len()
        )));
    }

    if create_bonds {
        auto_create_bonds(&mut atomic_structure);
    }

    Ok(atomic_structure)
}
