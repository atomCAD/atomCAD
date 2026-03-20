use crate::crystolecule::atomic_constants::CHEMICAL_ELEMENTS;
use crate::crystolecule::motif::{Motif, Site};
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use glam::f64::DVec3;
use std::fs;
use std::io;
use std::num::ParseFloatError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PoscarError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid POSCAR format: {0}")]
    Parse(String),
    #[error("Invalid floating point number: {0}")]
    FloatParse(#[from] ParseFloatError),
}

/// Loads a POSCAR file from disk and returns the unit cell and motif.
pub fn load_poscar(file_path: &str) -> Result<(UnitCellStruct, Motif), PoscarError> {
    let content = fs::read_to_string(file_path)?;
    parse_poscar(&content)
}

/// Parses POSCAR content from a string and returns the unit cell and motif.
///
/// Supports VASP 5+ format with element names on line 6.
pub fn parse_poscar(content: &str) -> Result<(UnitCellStruct, Motif), PoscarError> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.len() < 8 {
        return Err(PoscarError::Parse(
            "POSCAR file must have at least 8 lines".to_string(),
        ));
    }

    // Line 0: Comment (ignored)
    // Line 1: Scaling factor
    let scaling_factor: f64 = lines[1]
        .trim()
        .parse()
        .map_err(|_| PoscarError::Parse("Invalid scaling factor on line 2".to_string()))?;

    if scaling_factor <= 0.0 {
        return Err(PoscarError::Parse(
            "Scaling factor must be positive".to_string(),
        ));
    }

    // Lines 2-4: Lattice vectors
    let a = parse_vec3(lines[2], 3)?;
    let b = parse_vec3(lines[3], 4)?;
    let c = parse_vec3(lines[4], 5)?;

    let a = a * scaling_factor;
    let b = b * scaling_factor;
    let c = c * scaling_factor;

    let unit_cell = UnitCellStruct::new(a, b, c);

    // Line 5: Element symbols (VASP 5+)
    let species: Vec<&str> = lines[5].split_whitespace().collect();
    if species.is_empty() {
        return Err(PoscarError::Parse(
            "Missing species names on line 6".to_string(),
        ));
    }

    // Validate element symbols and get atomic numbers
    let atomic_numbers: Vec<i16> = species
        .iter()
        .map(|s| {
            CHEMICAL_ELEMENTS
                .get(*s)
                .map(|&n| n as i16)
                .ok_or_else(|| {
                    PoscarError::Parse(format!("Unknown element symbol: '{}'", s))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Line 6: Atom counts per species
    let counts: Vec<usize> = lines[6]
        .split_whitespace()
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| PoscarError::Parse(format!("Invalid atom count: '{}'", s)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if counts.len() != species.len() {
        return Err(PoscarError::Parse(format!(
            "Number of atom counts ({}) does not match number of species ({})",
            counts.len(),
            species.len()
        )));
    }

    let total_atoms: usize = counts.iter().sum();

    // Line 7: Coordinate type (Direct or Cartesian)
    let coord_line = lines[7].trim();
    let is_cartesian = match coord_line.chars().next() {
        Some('C' | 'c' | 'K' | 'k') => true,
        Some('D' | 'd') => false,
        _ => {
            return Err(PoscarError::Parse(format!(
                "Invalid coordinate type '{}': expected 'Direct' or 'Cartesian'",
                coord_line
            )))
        }
    };

    // Lines 8+: Atom positions
    if lines.len() < 8 + total_atoms {
        return Err(PoscarError::Parse(format!(
            "Expected {} atom positions but file has only {} lines after coordinate type",
            total_atoms,
            lines.len() - 8
        )));
    }

    let mut sites = Vec::with_capacity(total_atoms);
    let mut line_index = 8;

    for (species_idx, &count) in counts.iter().enumerate() {
        let atomic_number = atomic_numbers[species_idx];
        for _ in 0..count {
            let pos = parse_vec3(lines[line_index], line_index + 1)?;
            let fractional = if is_cartesian {
                unit_cell.real_to_dvec3_lattice(&pos)
            } else {
                pos
            };
            sites.push(Site {
                atomic_number,
                position: fractional,
            });
            line_index += 1;
        }
    }

    let num_sites = sites.len();
    let motif = Motif {
        parameters: Vec::new(),
        sites,
        bonds: Vec::new(),
        bonds_by_site1_index: vec![Vec::new(); num_sites],
        bonds_by_site2_index: vec![Vec::new(); num_sites],
    };

    Ok((unit_cell, motif))
}

/// Parses a line containing three whitespace-separated floats into a DVec3.
fn parse_vec3(line: &str, line_number: usize) -> Result<DVec3, PoscarError> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(PoscarError::Parse(format!(
            "Expected 3 values on line {}, found {}",
            line_number,
            parts.len()
        )));
    }
    let x: f64 = parts[0].parse()?;
    let y: f64 = parts[1].parse()?;
    let z: f64 = parts[2].parse()?;
    Ok(DVec3::new(x, y, z))
}
