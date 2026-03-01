// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashMap;
use std::fmt;

use bevy::{
    asset::{AssetLoader, LoadContext},
    camera::primitives::Aabb,
    prelude::*,
};
use molecule::{AtomInstance, BondInstance, Molecule};
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

/// Error from parsing a single PDB record.
#[derive(Debug)]
struct RecordParseError(&'static str);

impl fmt::Display for RecordParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Extract and trim a column range from a PDB line.
///
/// Column indices are 0-based (PDB spec columns are 1-based, so subtract 1).
/// Returns an error if the line is too short to contain the requested range.
fn column(line: &str, start: usize, end: usize) -> Result<&str, RecordParseError> {
    line.get(start..end)
        .map(str::trim)
        .ok_or(RecordParseError("line too short"))
}

/// Parse an ATOM or HETATM record using the PDB fixed-column format.
///
/// PDB column layout (1-indexed → 0-indexed):
///    7-11 →  6..11  Atom serial number
///   13-16 → 12..16  Atom name (used as element fallback)
///   31-38 → 30..38  X coordinate (8.3 format)
///   39-46 → 38..46  Y coordinate (8.3 format)
///   47-54 → 46..54  Z coordinate (8.3 format)
///   77-78 → 76..78  Element symbol (if present, preferred)
fn parse_atom_record(line: &str) -> Result<(&str, AtomInstance), RecordParseError> {
    if !line.starts_with("ATOM") && !line.starts_with("HETATM") {
        return Err(RecordParseError("not an ATOM or HETATM record"));
    }

    let serial = column(line, 6, 11)?;

    // Prefer the element symbol at columns 77-78 if present; fall back to atom name at 13-16.
    let element = line
        .get(76..78)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| line.get(12..16).map(str::trim))
        .unwrap_or("");

    let x: f32 = column(line, 30, 38)?
        .parse()
        .map_err(|_| RecordParseError("invalid X coordinate"))?;
    let y: f32 = column(line, 38, 46)?
        .parse()
        .map_err(|_| RecordParseError("invalid Y coordinate"))?;
    let z: f32 = column(line, 46, 54)?
        .parse()
        .map_err(|_| RecordParseError("invalid Z coordinate"))?;

    let element_id = get_element_id(element);

    Ok((
        serial,
        AtomInstance {
            position: Vec3::new(x, y, z),
            kind: element_id,
        },
    ))
}

/// Parse a CONECT record using the PDB fixed-column format.
///
/// PDB column layout (1-indexed → 0-indexed):
///    7-11 →  6..11  Central atom serial number
///   12-16 → 11..16  Bonded atom 1
///   17-21 → 16..21  Bonded atom 2
///   22-26 → 21..26  Bonded atom 3
///   27-31 → 26..31  Bonded atom 4
fn parse_conect_record(
    line: &str,
    serial_to_index: &HashMap<&str, u32>,
) -> Result<Vec<BondInstance>, RecordParseError> {
    if !line.starts_with("CONECT") {
        return Err(RecordParseError("not a CONECT record"));
    }

    let central_serial = column(line, 6, 11)?;

    let Some(&central_idx) = serial_to_index.get(central_serial) else {
        return Err(RecordParseError("unknown central atom serial"));
    };

    let mut bonds = Vec::new();

    // Bond serials occupy 5-character columns starting at position 11 (0-indexed).
    for col_start in (11..line.len()).step_by(5) {
        let col_end = (col_start + 5).min(line.len());
        let Some(field) = line.get(col_start..col_end).map(str::trim) else {
            break;
        };
        if field.is_empty() {
            continue;
        }
        if let Some(&target_idx) = serial_to_index.get(field) {
            // Only add the bond if this direction hasn't been seen yet
            // (CONECT records sometimes list bonds in both directions)
            if central_idx < target_idx {
                bonds.push(BondInstance {
                    atoms: [central_idx, target_idx],
                });
            }
        }
    }

    Ok(bonds)
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

/// Parse PDB file content into a PdbAsset.
///
/// This is the core parsing logic, extracted from the asset loader so it can be
/// tested independently of Bevy's async asset loading infrastructure.
fn parse_pdb_content(content: &str) -> Result<PdbAsset, PdbAssetLoaderError> {
    let mut atoms = Vec::new();
    let mut serial_to_index = HashMap::new();

    // 1st pass: Parse the PDB file and extract all atom records
    for (line_idx, line) in content.lines().enumerate() {
        if line.starts_with("ATOM") || line.starts_with("HETATM") {
            match parse_atom_record(line) {
                Ok((serial, atom)) => {
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
                Ok(new_bonds) => {
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

        parse_pdb_content(content)
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

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_atom_record tests --

    #[test]
    fn parse_atom_record_standard_atom() {
        let line = "ATOM      1  H   FINA   1     -39.522  10.295  -0.431  1.00  0.00";
        let (serial, atom) = parse_atom_record(line).unwrap();
        assert_eq!(serial, "1");
        assert_eq!(atom.kind, Element::Hydrogen as u32);
        assert!((atom.position.x - (-39.522)).abs() < 1e-3);
        assert!((atom.position.y - 10.295).abs() < 1e-3);
        assert!((atom.position.z - (-0.431)).abs() < 1e-3);
    }

    #[test]
    fn parse_atom_record_carbon() {
        let line = "ATOM      2  C   FINA   1      -0.961  37.711 -18.586  1.00  0.00";
        let (serial, atom) = parse_atom_record(line).unwrap();
        assert_eq!(serial, "2");
        assert_eq!(atom.kind, Element::Carbon as u32);
        assert!((atom.position.x - (-0.961)).abs() < 1e-3);
        assert!((atom.position.y - 37.711).abs() < 1e-3);
        assert!((atom.position.z - (-18.586)).abs() < 1e-3);
    }

    #[test]
    fn parse_atom_record_two_letter_element() {
        // SI is a two-letter element symbol
        let line = "ATOM      9 SI   FINA   1       6.630  17.582 -10.256  1.00  0.00";
        let (serial, atom) = parse_atom_record(line).unwrap();
        assert_eq!(serial, "9");
        assert_eq!(atom.kind, Element::Silicon as u32);
        assert!((atom.position.x - 6.630).abs() < 1e-3);
    }

    #[test]
    fn parse_atom_record_hetatm() {
        let line = "HETATM    1  NE  NEO     1      10.000  20.000  30.000  1.00  0.00";
        let (serial, atom) = parse_atom_record(line).unwrap();
        assert_eq!(serial, "1");
        assert_eq!(atom.kind, Element::Neon as u32);
        assert!((atom.position.x - 10.0).abs() < 1e-3);
        assert!((atom.position.y - 20.0).abs() < 1e-3);
        assert!((atom.position.z - 30.0).abs() < 1e-3);
    }

    #[test]
    fn parse_atom_record_with_chain_id() {
        // Standard PDB format with chain ID "A" — previously broke the whitespace parser
        let line = "ATOM      1  N   ALA A   1       1.000   2.000   3.000  1.00  0.00";
        let (serial, atom) = parse_atom_record(line).unwrap();
        assert_eq!(serial, "1");
        assert_eq!(atom.kind, Element::Nitrogen as u32);
        assert!((atom.position.x - 1.0).abs() < 1e-3);
        assert!((atom.position.y - 2.0).abs() < 1e-3);
        assert!((atom.position.z - 3.0).abs() < 1e-3);
    }

    #[test]
    fn parse_atom_record_with_element_columns_77_78() {
        // When columns 77-78 contain the element symbol, prefer it over atom name.
        // Atom name "CA" could be C-alpha, but element column says "C".
        let line =
            "ATOM      1  CA  ALA A   1       1.000   2.000   3.000  1.00  0.00           C  ";
        let (_, atom) = parse_atom_record(line).unwrap();
        assert_eq!(atom.kind, Element::Carbon as u32);
    }

    #[test]
    fn parse_atom_record_unknown_element_returns_zero() {
        let line = "ATOM      1  XX  FINA   1       1.000   2.000   3.000  1.00  0.00";
        let (_, atom) = parse_atom_record(line).unwrap();
        assert_eq!(atom.kind, 0); // Unknown element maps to 0
    }

    #[test]
    fn parse_atom_record_rejects_non_atom_line() {
        let line = "CONECT    1  344";
        assert!(parse_atom_record(line).is_err());
    }

    #[test]
    fn parse_atom_record_rejects_too_short_line() {
        // Line too short to contain coordinates
        let line = "ATOM      1  H   FINA   1";
        assert!(parse_atom_record(line).is_err());
    }

    // -- parse_conect_record tests --

    #[test]
    fn parse_conect_single_bond() {
        let mut serial_to_index = HashMap::new();
        serial_to_index.insert("1", 0u32);
        serial_to_index.insert("344", 1u32);
        let line = "CONECT    1  344";
        let bonds = parse_conect_record(line, &serial_to_index).unwrap();
        assert_eq!(bonds.len(), 1);
        assert_eq!(bonds[0].atoms, [0, 1]);
    }

    #[test]
    fn parse_conect_multiple_bonds() {
        let mut serial_to_index = HashMap::new();
        serial_to_index.insert("2", 0u32);
        serial_to_index.insert("605", 1u32);
        serial_to_index.insert("492", 2u32);
        serial_to_index.insert("816", 3u32);
        serial_to_index.insert("4726", 4u32);
        let line = "CONECT    2  605  492  816 4726";
        let bonds = parse_conect_record(line, &serial_to_index).unwrap();
        assert_eq!(bonds.len(), 4);
        // All bonds should have central atom index 0, with targets 1..4
        for (i, bond) in bonds.iter().enumerate() {
            assert_eq!(bond.atoms[0], 0);
            assert_eq!(bond.atoms[1], (i + 1) as u32);
        }
    }

    #[test]
    fn parse_conect_deduplicates_bidirectional_bonds() {
        // When central_idx > target_idx, the bond should be skipped
        // (it was already recorded from the other direction)
        let mut serial_to_index = HashMap::new();
        serial_to_index.insert("5", 5u32);
        serial_to_index.insert("3", 3u32);
        let line = "CONECT    5    3";
        let bonds = parse_conect_record(line, &serial_to_index).unwrap();
        assert_eq!(bonds.len(), 0); // Skipped because 5 > 3
    }

    #[test]
    fn parse_conect_rejects_non_conect_line() {
        let serial_to_index = HashMap::new();
        let line = "ATOM      1  H   FINA   1     -39.522  10.295  -0.431  1.00  0.00";
        assert!(parse_conect_record(line, &serial_to_index).is_err());
    }

    #[test]
    fn parse_conect_rejects_too_short_line() {
        let serial_to_index = HashMap::new();
        let line = "CONECT";
        assert!(parse_conect_record(line, &serial_to_index).is_err());
    }

    #[test]
    fn parse_conect_skips_unknown_serial() {
        // If a bonded atom serial isn't in the map, it's silently skipped
        let mut serial_to_index = HashMap::new();
        serial_to_index.insert("1", 0u32);
        // serial "999" is not in the map
        let line = "CONECT    1  999";
        let bonds = parse_conect_record(line, &serial_to_index).unwrap();
        assert_eq!(bonds.len(), 0);
    }

    #[test]
    fn parse_does_not_panic_on_non_ascii() {
        // Multi-byte UTF-8 in a text field (residue name) shifts all subsequent column
        // boundaries. U+1F4A3 is 4 bytes, so columns after it are misaligned.
        // The parser should return an error rather than panic on the shifted coordinates.
        let atom_line =
            "ATOM      1  H   \u{1F4A3}       1       1.000   2.000   3.000  1.00  0.00";
        assert!(parse_atom_record(atom_line).is_err());

        // Multi-byte UTF-8 in a bond serial column should not panic.
        let conect_line = "CONECT    1  \u{1F4A3}  ";
        let mut serial_to_index = HashMap::new();
        serial_to_index.insert("1", 0u32);
        let bonds = parse_conect_record(conect_line, &serial_to_index).unwrap();
        assert!(bonds.is_empty());
    }

    // -- get_element_id tests --

    #[test]
    fn get_element_id_known_elements() {
        assert_eq!(get_element_id("H"), Element::Hydrogen as u32);
        assert_eq!(get_element_id("C"), Element::Carbon as u32);
        assert_eq!(get_element_id("N"), Element::Nitrogen as u32);
        assert_eq!(get_element_id("O"), Element::Oxygen as u32);
        assert_eq!(get_element_id("SI"), Element::Silicon as u32);
        assert_eq!(get_element_id("S"), Element::Sulfur as u32);
        assert_eq!(get_element_id("NE"), Element::Neon as u32);
    }

    #[test]
    fn get_element_id_unknown_returns_zero() {
        assert_eq!(get_element_id("XX"), 0);
        assert_eq!(get_element_id(""), 0);
    }

    // -- parse_pdb_content (full pipeline) tests --

    #[test]
    fn parse_pdb_content_empty_returns_error() {
        let result = parse_pdb_content("");
        assert!(matches!(result, Err(PdbAssetLoaderError::EmptyAtomList)));
    }

    #[test]
    fn parse_pdb_content_no_atoms_returns_error() {
        let content = "REMARK  This file has no atoms\nEND\n";
        let result = parse_pdb_content(content);
        assert!(matches!(result, Err(PdbAssetLoaderError::EmptyAtomList)));
    }

    #[test]
    fn parse_pdb_content_single_atom() {
        let content = "ATOM      1  C   ALA     1       1.000   2.000   3.000  1.00  0.00\n";
        let asset = parse_pdb_content(content).unwrap();

        assert_eq!(asset.molecule.atoms.len(), 1);
        assert_eq!(asset.molecule.bonds.len(), 0);
        assert_eq!(asset.molecule.atoms[0].kind, Element::Carbon as u32);
        // Single atom is re-centered to origin
        assert!(asset.molecule.atoms[0].position.length() < 1e-5);
    }

    #[test]
    fn parse_pdb_content_atoms_are_recentered() {
        let content = "\
ATOM      1  H   ALA     1      10.000  10.000  10.000  1.00  0.00
ATOM      2  H   ALA     1      20.000  10.000  10.000  1.00  0.00
";
        let asset = parse_pdb_content(content).unwrap();

        // Average position is (15, 10, 10), so atoms should be re-centered
        let a0 = asset.molecule.atoms[0].position;
        let a1 = asset.molecule.atoms[1].position;
        assert!((a0.x - (-5.0)).abs() < 1e-5);
        assert!((a0.y - 0.0).abs() < 1e-5);
        assert!((a1.x - 5.0).abs() < 1e-5);
        assert!((a1.y - 0.0).abs() < 1e-5);
    }

    #[test]
    fn parse_pdb_content_with_bonds() {
        let content = "\
ATOM      1  C   ALA     1       0.000   0.000   0.000  1.00  0.00
ATOM      2  C   ALA     1       1.540   0.000   0.000  1.00  0.00
ATOM      3  H   ALA     1      -0.500   0.900   0.000  1.00  0.00
CONECT    1    2    3
CONECT    2    1
CONECT    3    1
";
        let asset = parse_pdb_content(content).unwrap();

        assert_eq!(asset.molecule.atoms.len(), 3);
        // Bond 1-2 (indices 0-1) is recorded from CONECT 1 since 0 < 1
        // Bond 1-3 (indices 0-2) is recorded from CONECT 1 since 0 < 2
        // CONECT 2 has bond 2-1 which is skipped (1 > 0)
        // CONECT 3 has bond 3-1 which is skipped (2 > 0)
        assert_eq!(asset.molecule.bonds.len(), 2);
        assert_eq!(asset.molecule.bonds[0].atoms, [0, 1]);
        assert_eq!(asset.molecule.bonds[1].atoms, [0, 2]);
    }

    #[test]
    fn parse_pdb_content_aabb_includes_radii() {
        // Single hydrogen atom at origin after re-centering
        let content = "ATOM      1  H   ALA     1       5.000   5.000   5.000  1.00  0.00\n";
        let asset = parse_pdb_content(content).unwrap();

        // Hydrogen van der Waals radius is 1.10
        let he: Vec3 = asset.aabb.half_extents.into();
        assert!((he.x - 1.10).abs() < 1e-3);
        assert!((he.y - 1.10).abs() < 1e-3);
        assert!((he.z - 1.10).abs() < 1e-3);
    }

    #[test]
    fn parse_pdb_content_ignores_non_atom_lines() {
        let content = "\
HEADER    TEST FILE
REMARK  This is a comment
ATOM      1  C   ALA     1       0.000   0.000   0.000  1.00  0.00
REMARK  Another comment
END
";
        let asset = parse_pdb_content(content).unwrap();
        assert_eq!(asset.molecule.atoms.len(), 1);
    }

    #[test]
    fn parse_pdb_content_hetatm_and_atom_mixed() {
        let content = "\
ATOM      1  C   ALA     1       0.000   0.000   0.000  1.00  0.00
HETATM    2  NE  NEO     1       3.000   0.000   0.000  1.00  0.00
CONECT    1    2
";
        let asset = parse_pdb_content(content).unwrap();
        assert_eq!(asset.molecule.atoms.len(), 2);
        assert_eq!(asset.molecule.atoms[0].kind, Element::Carbon as u32);
        assert_eq!(asset.molecule.atoms[1].kind, Element::Neon as u32);
        assert_eq!(asset.molecule.bonds.len(), 1);
    }

    #[test]
    fn parse_pdb_content_with_chain_ids() {
        // Standard PDB format with chain ID "A" — this broke the old whitespace parser
        let content = "\
ATOM      1  N   ALA A   1       0.000   0.000   0.000  1.00  0.00
ATOM      2  C   ALA A   1       1.540   0.000   0.000  1.00  0.00
CONECT    1    2
";
        let asset = parse_pdb_content(content).unwrap();
        assert_eq!(asset.molecule.atoms.len(), 2);
        assert_eq!(asset.molecule.atoms[0].kind, Element::Nitrogen as u32);
        assert_eq!(asset.molecule.atoms[1].kind, Element::Carbon as u32);
        assert_eq!(asset.molecule.bonds.len(), 1);
    }

    #[test]
    fn parse_pdb_content_sulfur_element() {
        let content = "ATOM     19  S   FINA   1     -32.286  26.687  -3.583  1.00  0.00\n";
        let asset = parse_pdb_content(content).unwrap();
        assert_eq!(asset.molecule.atoms[0].kind, Element::Sulfur as u32);
    }

    #[test]
    fn parse_pdb_content_invalid_atom_line_returns_error() {
        // Has ATOM prefix but not enough fields for coordinates
        let content = "ATOM      1  C   ALA\n";
        let result = parse_pdb_content(content);
        assert!(result.is_err());
    }
}

// End of File
