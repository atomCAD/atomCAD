use glam::DVec3;
use std::collections::HashMap;

use crate::crystolecule::atomic_constants::CHEMICAL_ELEMENTS;
use crate::crystolecule::io::cif::parser::CifDataBlock;
use crate::crystolecule::io::cif::symmetry::{
    CifAtomSite, SymmetryOperation, parse_symmetry_operation,
};
use crate::crystolecule::unit_cell_struct::UnitCellStruct;

use thiserror::Error;

/// Error type for crystal data extraction.
#[derive(Debug, Error)]
pub enum CifError {
    #[error("Missing required CIF tag: {0}")]
    MissingTag(String),

    #[error("Invalid numeric value for tag '{tag}': {value}")]
    InvalidNumber { tag: String, value: String },

    #[error("No atom sites found in CIF data block")]
    NoAtomSites,

    #[error("Atom site missing fractional coordinates")]
    MissingCoordinates,

    #[error("Could not determine element for atom site '{0}'")]
    UnknownElement(String),

    #[error("No symmetry information found (no explicit operations and no space group number)")]
    NoSymmetryInfo,

    #[error("Symmetry operation parse error: {0}")]
    SymmetryParse(String),
}

/// Extracted crystallographic data from a CIF data block.
#[derive(Debug, Clone)]
pub struct CifCrystalData {
    pub unit_cell: UnitCellStruct,
    pub asymmetric_atoms: Vec<CifAtomSite>,
    pub symmetry_operations: Vec<SymmetryOperation>,
    pub bonds: Vec<CifBond>,
}

/// A bond from `_geom_bond_*` loop data.
#[derive(Debug, Clone)]
pub struct CifBond {
    pub atom_label_1: String,
    pub atom_label_2: String,
    pub distance: f64,
    pub symmetry_code_1: Option<String>,
    pub symmetry_code_2: Option<String>,
    pub bond_order: i32,
}

/// Parsed symmetry code in `S_XYZ` format.
/// `S` is the 1-based symmetry operation index, and `X`, `Y`, `Z` encode
/// cell translations as `digit - 5` (so 5 = same cell, 4 = -1, 6 = +1).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSymmetryCode {
    pub symop_index: usize,
    pub translation: glam::IVec3,
}

/// Extract crystal data from a CIF data block.
///
/// Handles both old (`_symmetry_*`) and new (`_space_group_*`) tag names.
/// Extracts `_geom_bond_*` data if present, with bond order from
/// `_ccdc_geom_bond_type` or `_chemical_conn_bond_type` when available.
pub fn extract_crystal_data(block: &CifDataBlock) -> Result<CifCrystalData, CifError> {
    let unit_cell = extract_unit_cell(block)?;
    let asymmetric_atoms = extract_atom_sites(block)?;
    let symmetry_operations = extract_symmetry_operations(block)?;
    let bonds = extract_bonds(block);

    Ok(CifCrystalData {
        unit_cell,
        asymmetric_atoms,
        symmetry_operations,
        bonds,
    })
}

/// Extract the unit cell from the 6 crystallographic parameters.
fn extract_unit_cell(block: &CifDataBlock) -> Result<UnitCellStruct, CifError> {
    let a = get_float(block, "_cell_length_a")?;
    let b = get_float(block, "_cell_length_b")?;
    let c = get_float(block, "_cell_length_c")?;
    let alpha = get_float(block, "_cell_angle_alpha")?;
    let beta = get_float(block, "_cell_angle_beta")?;
    let gamma = get_float(block, "_cell_angle_gamma")?;

    Ok(UnitCellStruct::from_parameters(a, b, c, alpha, beta, gamma))
}

/// Extract atom sites from the `_atom_site_*` loop.
fn extract_atom_sites(block: &CifDataBlock) -> Result<Vec<CifAtomSite>, CifError> {
    // Find the loop containing atom site data
    let loop_data = block
        .find_loop("_atom_site_fract_x")
        .or_else(|| block.find_loop("_atom_site_label"))
        .ok_or(CifError::NoAtomSites)?;

    let fract_x_idx = loop_data.column_index("_atom_site_fract_x");
    let fract_y_idx = loop_data.column_index("_atom_site_fract_y");
    let fract_z_idx = loop_data.column_index("_atom_site_fract_z");

    if fract_x_idx.is_none() || fract_y_idx.is_none() || fract_z_idx.is_none() {
        return Err(CifError::MissingCoordinates);
    }

    let fx = fract_x_idx.unwrap();
    let fy = fract_y_idx.unwrap();
    let fz = fract_z_idx.unwrap();

    let label_idx = loop_data.column_index("_atom_site_label");
    let type_symbol_idx = loop_data.column_index("_atom_site_type_symbol");
    let occupancy_idx = loop_data.column_index("_atom_site_occupancy");
    let calc_flag_idx = loop_data.column_index("_atom_site_calc_flag");

    let mut atoms = Vec::new();

    for row in &loop_data.rows {
        // Skip dummy atoms (calc_flag = "dum")
        if let Some(idx) = calc_flag_idx {
            if row[idx].eq_ignore_ascii_case("dum") {
                continue;
            }
        }

        let label = label_idx.map(|i| row[i].clone()).unwrap_or_default();

        // Skip dummy atoms based on type_symbol being "."
        if let Some(idx) = type_symbol_idx {
            if row[idx] == "." {
                continue;
            }
        }

        // Determine element: prefer _atom_site_type_symbol, fall back to label
        let element = if let Some(idx) = type_symbol_idx {
            parse_element_symbol(&row[idx])
        } else {
            parse_element_from_label(&label)
        };

        let element = element.ok_or_else(|| CifError::UnknownElement(label.clone()))?;

        let x: f64 = parse_cif_float(&row[fx]).ok_or_else(|| CifError::InvalidNumber {
            tag: "_atom_site_fract_x".to_string(),
            value: row[fx].clone(),
        })?;
        let y: f64 = parse_cif_float(&row[fy]).ok_or_else(|| CifError::InvalidNumber {
            tag: "_atom_site_fract_y".to_string(),
            value: row[fy].clone(),
        })?;
        let z: f64 = parse_cif_float(&row[fz]).ok_or_else(|| CifError::InvalidNumber {
            tag: "_atom_site_fract_z".to_string(),
            value: row[fz].clone(),
        })?;

        let occupancy = occupancy_idx
            .and_then(|i| parse_cif_float(&row[i]))
            .unwrap_or(1.0);

        atoms.push(CifAtomSite {
            label,
            element,
            fract: DVec3::new(x, y, z),
            occupancy,
        });
    }

    if atoms.is_empty() {
        return Err(CifError::NoAtomSites);
    }

    Ok(atoms)
}

/// Extract symmetry operations from the CIF data block.
///
/// Tries new-style `_space_group_symop_operation_xyz` first, then falls back
/// to old-style `_symmetry_equiv_pos_as_xyz`.
fn extract_symmetry_operations(block: &CifDataBlock) -> Result<Vec<SymmetryOperation>, CifError> {
    // Try to find explicit symmetry operations in a loop
    let symop_loop = block
        .find_loop("_space_group_symop_operation_xyz")
        .or_else(|| block.find_loop("_symmetry_equiv_pos_as_xyz"));

    if let Some(loop_data) = symop_loop {
        let col_idx = loop_data
            .column_index("_space_group_symop_operation_xyz")
            .or_else(|| loop_data.column_index("_symmetry_equiv_pos_as_xyz"))
            .unwrap(); // Safe: we found the loop via this tag

        let mut operations = Vec::new();
        for row in &loop_data.rows {
            let op = parse_symmetry_operation(&row[col_idx])
                .map_err(|e| CifError::SymmetryParse(e.to_string()))?;
            operations.push(op);
        }

        if !operations.is_empty() {
            return Ok(operations);
        }
    }

    // No explicit operations found — future phases will add space group lookup.
    // For now, return an error.
    Err(CifError::NoSymmetryInfo)
}

/// Extract bond data from `_geom_bond_*` loop if present.
///
/// Returns an empty Vec if no bond data exists (this is not an error —
/// many CIF files don't include explicit bonds).
fn extract_bonds(block: &CifDataBlock) -> Vec<CifBond> {
    let loop_data = match block.find_loop("_geom_bond_atom_site_label_1") {
        Some(l) => l,
        None => return Vec::new(),
    };

    let label1_idx = match loop_data.column_index("_geom_bond_atom_site_label_1") {
        Some(i) => i,
        None => return Vec::new(),
    };
    let label2_idx = match loop_data.column_index("_geom_bond_atom_site_label_2") {
        Some(i) => i,
        None => return Vec::new(),
    };

    let dist_idx = loop_data.column_index("_geom_bond_distance");
    let sym1_idx = loop_data.column_index("_geom_bond_site_symmetry_1");
    let sym2_idx = loop_data.column_index("_geom_bond_site_symmetry_2");
    let ccdc_type_idx = loop_data.column_index("_ccdc_geom_bond_type");
    let conn_type_idx = loop_data.column_index("_chemical_conn_bond_type");

    let mut bonds = Vec::new();

    for row in &loop_data.rows {
        let atom_label_1 = row[label1_idx].clone();
        let atom_label_2 = row[label2_idx].clone();

        let distance = dist_idx
            .and_then(|i| parse_cif_float(&row[i]))
            .unwrap_or(0.0);

        let symmetry_code_1 = sym1_idx
            .map(|i| row[i].clone())
            .filter(|s| s != "." && s != "?");
        let symmetry_code_2 = sym2_idx
            .map(|i| row[i].clone())
            .filter(|s| s != "." && s != "?");

        let bond_order = ccdc_type_idx
            .map(|i| parse_bond_order_ccdc(&row[i]))
            .or_else(|| conn_type_idx.map(|i| parse_bond_order_conn(&row[i])))
            .unwrap_or(1);

        bonds.push(CifBond {
            atom_label_1,
            atom_label_2,
            distance,
            symmetry_code_1,
            symmetry_code_2,
            bond_order,
        });
    }

    bonds
}

/// Parse a CIF symmetry code in `S_XYZ` format.
///
/// `S` is the 1-based symmetry operation index and `X`, `Y`, `Z` encode
/// cell translations as `digit - 5` (5 = same cell, 4 = -1, 6 = +1).
///
/// Examples:
/// - `"1_555"` → symop 1, translation (0,0,0)
/// - `"2_655"` → symop 2, translation (+1,0,0)
/// - `"3_545"` → symop 3, translation (0,-1,0)
pub fn parse_symmetry_code(code: &str) -> Option<ParsedSymmetryCode> {
    let parts: Vec<&str> = code.split('_').collect();
    if parts.len() != 2 {
        return None;
    }

    let symop_index: usize = parts[0].parse().ok()?;
    let xyz = parts[1];
    if xyz.len() != 3 {
        return None;
    }

    let digits: Vec<i32> = xyz
        .chars()
        .map(|c| c.to_digit(10).map(|d| d as i32 - 5))
        .collect::<Option<Vec<_>>>()?;

    Some(ParsedSymmetryCode {
        symop_index,
        translation: glam::IVec3::new(digits[0], digits[1], digits[2]),
    })
}

// --- Helper functions ---

/// Get a float value from a CIF tag, returning an error if missing or invalid.
fn get_float(block: &CifDataBlock, tag: &str) -> Result<f64, CifError> {
    let value = block
        .get_tag(tag)
        .ok_or_else(|| CifError::MissingTag(tag.to_string()))?;

    parse_cif_float(value).ok_or_else(|| CifError::InvalidNumber {
        tag: tag.to_string(),
        value: value.to_string(),
    })
}

/// Parse a CIF numeric value, handling uncertainties like `5.4307(2)`.
/// The parser already strips uncertainties, but handle any remaining ones.
fn parse_cif_float(s: &str) -> Option<f64> {
    let s = s.trim();
    if s == "." || s == "?" {
        return None;
    }
    // Strip any remaining uncertainty notation
    let s = if let Some(paren) = s.find('(') {
        &s[..paren]
    } else {
        s
    };
    s.parse::<f64>().ok()
}

/// Parse an element symbol from `_atom_site_type_symbol`.
///
/// Handles formats like: `"C"`, `"Na"`, `"Fe3+"`, `"Na1+"`, `"Cl1-"`, `"O2-"`.
/// Strips charge indicators and trailing digits to extract the element.
fn parse_element_symbol(type_symbol: &str) -> Option<String> {
    let s = type_symbol.trim();
    if s.is_empty() || s == "." || s == "?" {
        return None;
    }

    // Extract the alphabetic prefix (the element symbol)
    let alpha: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if alpha.is_empty() {
        return None;
    }

    // Normalize: capitalize first letter, lowercase rest
    let normalized = capitalize_element(&alpha);

    // Verify it's a known element
    if CHEMICAL_ELEMENTS.contains_key(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

/// Parse an element from `_atom_site_label` (e.g., `"Na1"`, `"C"`, `"O2"`).
///
/// Labels typically start with the element symbol followed by a numeric index.
fn parse_element_from_label(label: &str) -> Option<String> {
    let s = label.trim();
    if s.is_empty() {
        return None;
    }

    // Try two-character element first (e.g., "Na1" → "Na"), then one-character
    let alpha: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if alpha.is_empty() {
        return None;
    }

    // Try the full alphabetic prefix first (e.g., "Na" from "Na1")
    let full = capitalize_element(&alpha);
    if CHEMICAL_ELEMENTS.contains_key(&full) {
        return Some(full);
    }

    // Try just the first character (e.g., "C" from "Ca1" wouldn't match above,
    // but this fallback handles single-letter elements)
    if alpha.len() > 1 {
        let first = capitalize_element(&alpha[..1]);
        if CHEMICAL_ELEMENTS.contains_key(&first) {
            return Some(first);
        }
    }

    None
}

/// Capitalize an element symbol: first letter uppercase, rest lowercase.
fn capitalize_element(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut result = first.to_uppercase().to_string();
            for c in chars {
                result.extend(c.to_lowercase());
            }
            result
        }
    }
}

/// Parse bond order from CCDC `_ccdc_geom_bond_type` value.
/// Values: S=single, D=double, T=triple, A=aromatic.
fn parse_bond_order_ccdc(value: &str) -> i32 {
    match value.trim().to_uppercase().as_str() {
        "S" => 1,
        "D" => 2,
        "T" => 3,
        "A" => 1, // Aromatic treated as single for our purposes
        _ => 1,
    }
}

/// Parse bond order from `_chemical_conn_bond_type` value.
/// Values: sing, doub, trip, arom, etc.
fn parse_bond_order_conn(value: &str) -> i32 {
    match value.trim().to_lowercase().as_str() {
        "sing" => 1,
        "doub" => 2,
        "trip" => 3,
        "arom" => 1,
        _ => 1,
    }
}

/// Build a lookup map from atom labels to their indices in the asymmetric atom list.
pub fn build_label_index(atoms: &[CifAtomSite]) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for (i, atom) in atoms.iter().enumerate() {
        map.insert(atom.label.clone(), i);
    }
    map
}
