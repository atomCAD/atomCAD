// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::atoms::{AtomCluster, AtomInstance};
use bevy::{
    asset::{AssetLoader, LoadContext},
    prelude::*,
};
use nom::IResult;
use thiserror::Error as ThisError;

#[derive(Asset, Clone, Reflect)]
pub struct PdbAsset {
    pub atom_cluster: AtomCluster,
}

// Implement the asset loader
#[derive(Default, Reflect)]
pub struct PdbAssetLoader;

/// Maps element symbols to their atomic numbers (element IDs).
fn get_element_id(symbol: &str) -> u32 {
    periodic_table::Element::from_symbol(symbol).map_or(0, |e| e as u32)
}

/// Parse a float from a string
fn parse_float(s: &str) -> Result<f32, std::num::ParseFloatError> {
    s.trim().parse::<f32>()
}

/// Parse an ATOM or HETATM record using whitespace separation
fn parse_atom_record(line: &str) -> IResult<&str, AtomInstance> {
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

    // Element symbol is the second field according to user
    let element = if tokens.len() >= 3 {
        tokens[2].trim()
    } else {
        ""
    };

    // Parse X, Y, Z coordinates (should be at positions 5, 6, 7 in tokens)
    let x = parse_float(tokens[5])
        .map_err(|_| nom::Err::Error(nom::error::Error::new(line, nom::error::ErrorKind::Float)))?;

    let y = parse_float(tokens[6])
        .map_err(|_| nom::Err::Error(nom::error::Error::new(line, nom::error::ErrorKind::Float)))?;

    let z = parse_float(tokens[7])
        .map_err(|_| nom::Err::Error(nom::error::Error::new(line, nom::error::ErrorKind::Float)))?;

    let element_id = get_element_id(element);

    Ok((
        "",
        AtomInstance {
            position: Vec3::new(x, y, z),
            kind: element_id,
        },
    ))
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

        // Parse the PDB file and extract all atom records
        for line in content.lines() {
            if line.starts_with("ATOM") || line.starts_with("HETATM") {
                match parse_atom_record(line) {
                    Ok((_, atom)) => atoms.push(atom),
                    Err(e) => {
                        // Optionally log parsing errors
                        eprintln!("Error parsing line '{}': {:?}", line, e);
                        // Continue with next line
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

        // Create the atom cluster
        let atom_cluster = AtomCluster { atoms };

        // Return the asset with the atom cluster
        Ok(PdbAsset { atom_cluster })
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
