// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashMap;

use bevy::{
    asset::{AssetLoader, LoadContext},
    camera::primitives::Aabb,
    prelude::*,
};
use molecule::{AtomInstance, BondInstance, Molecule};
use nom::IResult;
use periodic_table::{Element, PeriodicTable};
use thiserror::Error as ThisError;

#[derive(Asset, Clone, Reflect)]
pub struct PdbAsset {
    pub molecule: Molecule,
    pub aabb: Aabb,
}

// Implement the asset loader
#[derive(Default, Reflect)]
pub struct PdbAssetLoader;

/// Maps element symbols to their atomic numbers (element IDs).
fn get_element_id(symbol: &str) -> u32 {
    Element::from_symbol(symbol).map_or(0, |e| e as u32)
}

/// Parse a float from a string
fn parse_float(s: &str) -> Result<f32, std::num::ParseFloatError> {
    s.trim().parse::<f32>()
}

/// Parse an ATOM or HETATM record using whitespace separation
fn parse_atom_record(line: &str) -> IResult<&str, (&str, AtomInstance)> {
    // First, check if the line starts with ATOM or HETATM
    if !line.starts_with("ATOM") && !line.starts_with("HETATM") {
        return Err(nom::Err::Error(nom::error::Error::new(
            line,
            nom::error::ErrorKind::Tag,
        )));
    }

    // Split the line by whitespace
    let tokens: Vec<&str> = line.split_whitespace().collect();

    // A valid PDB ATOM record should have at least 7 fields (for the coordinates)
    if tokens.len() < 7 {
        return Err(nom::Err::Error(nom::error::Error::new(
            line,
            nom::error::ErrorKind::Verify,
        )));
    }

    // Serial number is the first field
    if tokens.len() < 2 {
        return Err(nom::Err::Error(nom::error::Error::new(
            line,
            nom::error::ErrorKind::Verify,
        )));
    }
    let serial = tokens[1].trim();

    // Element symbol is the second field according to user
    if tokens.len() < 3 {
        return Err(nom::Err::Error(nom::error::Error::new(
            line,
            nom::error::ErrorKind::Verify,
        )));
    }
    let element = tokens[2].trim();

    // Parse X, Y, Z coordinates (should be at positions 5, 6, 7 in tokens)
    let x = parse_float(tokens[5])
        .map_err(|_| nom::Err::Error(nom::error::Error::new(line, nom::error::ErrorKind::Float)))?;

    let y = parse_float(tokens[6])
        .map_err(|_| nom::Err::Error(nom::error::Error::new(line, nom::error::ErrorKind::Float)))?;

    let z = parse_float(tokens[7])
        .map_err(|_| nom::Err::Error(nom::error::Error::new(line, nom::error::ErrorKind::Float)))?;

    let element_id = get_element_id(element);

    Ok((
        serial,
        (
            serial,
            AtomInstance {
                position: Vec3::new(x, y, z),
                kind: element_id,
            },
        ),
    ))
}

// Parse CONECT record
fn parse_conect_record<'line>(
    line: &'line str,
    serial_to_index: &HashMap<&'line str, u32>,
) -> IResult<&'line str, Vec<BondInstance>> {
    // First, check if the line starts with CONECT
    if !line.starts_with("CONECT") {
        return Err(nom::Err::Error(nom::error::Error::new(
            line,
            nom::error::ErrorKind::Tag,
        )));
    }

    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 3 {
        return Err(nom::Err::Error(nom::error::Error::new(
            line,
            nom::error::ErrorKind::Verify,
        )));
    }

    let central_serial = tokens[1].trim();

    // Get the index for the central atom
    let Some(&central_idx) = serial_to_index.get(&central_serial) else {
        return Err(nom::Err::Error(nom::error::Error::new(
            line,
            nom::error::ErrorKind::Verify,
        )));
    };

    let mut bonds = Vec::new();

    // Process all bonded atoms (from position 2 onwards)
    for token in tokens.iter().skip(2) {
        let target_serial = token.trim();
        if let Some(&target_idx) = serial_to_index.get(&target_serial) {
            // Only add the bond if this direction hasn't been seen yet
            // (CONECT records sometimes list bonds in both directions)
            if central_idx < target_idx {
                bonds.push(BondInstance {
                    atoms: [central_idx, target_idx],
                });
            }
        }
    }

    Ok((line, bonds))
}

#[derive(Debug, ThisError)]
pub enum PdbAssetLoaderError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("No atoms found in the PDB file")]
    EmptyAtomList,
}

impl AssetLoader for PdbAssetLoader {
    type Asset = PdbAsset;
    type Settings = ();
    type Error = PdbAssetLoaderError;

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        // Read file & convert to string
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        let content = std::str::from_utf8(&buf)?;

        let mut atoms = Vec::new();
        let mut serial_to_index = HashMap::new();

        // 1st pass: Parse the PDB file and extract all atom records
        for (line_idx, line) in content.lines().enumerate() {
            if line.starts_with("ATOM") || line.starts_with("HETATM") {
                match parse_atom_record(line) {
                    Ok((_, (serial, atom))) => {
                        // Map serial number to index in our atoms array
                        serial_to_index.insert(serial, atoms.len() as u32);

                        // Add the atom to our atoms array
                        atoms.push(atom);
                    }
                    Err(e) => {
                        return Err(PdbAssetLoaderError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Error parsing PDB file on line {line_idx}, '{line}': {e:?}"),
                        )));
                    }
                }
            }
        }

        // 2nd pass: Parse the CONECT records and create bonds
        let mut bonds = Vec::new();

        for (line_idx, line) in content.lines().enumerate() {
            if line.starts_with("CONECT") {
                match parse_conect_record(line, &serial_to_index) {
                    Ok((_, new_bonds)) => {
                        bonds.extend(new_bonds);
                    }
                    Err(e) => {
                        return Err(PdbAssetLoaderError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Error parsing PDB file on line {line_idx}, '{line}': {e:?}"),
                        )));
                    }
                }
            }
        }

        if atoms.is_empty() {
            return Err(PdbAssetLoaderError::EmptyAtomList);
        }

        // Average the positions of the atoms
        let mut avg_position = Vec3::ZERO;
        for atom in atoms.iter() {
            avg_position += atom.position;
        }
        avg_position /= atoms.len() as f32;

        // Re-center the atoms
        for atom in atoms.iter_mut() {
            atom.position -= avg_position;
        }

        // Calculate AABB from atom positions including van der Waals radii
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        let periodic_table = PeriodicTable::new();
        for atom in &atoms {
            // Get the van der Waals radius for this atom
            let radius = periodic_table.element_reprs[atom.kind as usize].radius;

            // Expand the bounds by the atom's radius
            min = min.min(atom.position - Vec3::splat(radius));
            max = max.max(atom.position + Vec3::splat(radius));
        }
        let center = (min + max) * 0.5;
        let half_extents = (max - min) * 0.5;

        // Create the molecule from atoms & bonds
        let molecule = Molecule { atoms, bonds };

        // Return the asset with the molecule and its AABB
        Ok(PdbAsset {
            molecule,
            aabb: Aabb {
                center: center.into(),
                half_extents: half_extents.into(),
            },
        })
    }

    fn extensions(&self) -> &[&str] {
        &["pdb"]
    }
}

// Plugin to register the PDB asset loader
pub struct PdbLoaderPlugin;

impl Plugin for PdbLoaderPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PdbAsset>()
            .init_asset::<PdbAsset>()
            .init_asset_loader::<PdbAssetLoader>();
    }
}

// End of File
