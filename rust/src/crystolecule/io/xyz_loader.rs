use crate::crystolecule::atomic_constants::CHEMICAL_ELEMENTS;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure_utils::auto_create_bonds;
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

    for (index, line) in lines.enumerate() {
        let line = line?;

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 4 {
            return Err(XyzError::Parse(format!(
                "Invalid atom format on line {}: {}",
                index + 3,
                line
            )));
        }

        let element = parts[0].to_string();
        let atomic_number = *CHEMICAL_ELEMENTS.get(&element).unwrap_or(&1) as i16; // TODO: error for unknown elements
        let x: f64 = parts[1].parse()?;
        let y: f64 = parts[2].parse()?;
        let z: f64 = parts[3].parse()?;

        atomic_structure.add_atom(atomic_number, DVec3::new(x, y, z));
    }

    if atomic_structure.get_num_of_atoms() != num_atoms {
        return Err(XyzError::Parse(format!(
            "Expected {} atoms, but found {}",
            num_atoms,
            atomic_structure.get_num_of_atoms()
        )));
    }

    if create_bonds {
        auto_create_bonds(&mut atomic_structure);
    }

    Ok(atomic_structure)
}
