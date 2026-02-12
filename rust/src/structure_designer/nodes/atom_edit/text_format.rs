//! Human-readable text format for atom_edit diffs.
//!
//! Format:
//! - `+El @ (x, y, z)` — atom addition
//! - `~El @ (x, y, z)` — atom replacement
//! - `~El @ (x, y, z) [from (ox, oy, oz)]` — atom move
//! - `- @ (x, y, z)` — atom delete marker
//! - `bond A-B order_name` — bond
//! - `unbond A-B` — bond delete marker

use crate::crystolecule::atomic_constants::{ATOM_INFO, CHEMICAL_ELEMENTS, DEFAULT_ATOM_INFO};
use crate::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DATIVE, BOND_DELETED, BOND_DOUBLE, BOND_METALLIC, BOND_QUADRUPLE,
    BOND_SINGLE, BOND_TRIPLE,
};
use crate::crystolecule::atomic_structure::{AtomicStructure, DELETED_SITE_ATOMIC_NUMBER};
use crate::structure_designer::text_format::format_float;
use glam::f64::DVec3;
use std::collections::HashMap;

/// Get element symbol from atomic number.
fn element_symbol(atomic_number: i16) -> String {
    ATOM_INFO
        .get(&(atomic_number as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO)
        .symbol
        .clone()
}

/// Normalize an element symbol to standard capitalization (first char upper, rest lower).
fn normalize_element_symbol(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let mut result = c.to_uppercase().to_string();
            result.extend(chars.flat_map(|c| c.to_lowercase()));
            result
        }
    }
}

/// Format a position as (x, y, z) text.
fn format_position(pos: &DVec3) -> String {
    format!(
        "({}, {}, {})",
        format_float(pos.x),
        format_float(pos.y),
        format_float(pos.z)
    )
}

/// Get human-readable name for a bond order.
fn bond_order_name(order: u8) -> &'static str {
    match order {
        BOND_SINGLE => "single",
        BOND_DOUBLE => "double",
        BOND_TRIPLE => "triple",
        BOND_QUADRUPLE => "quadruple",
        BOND_AROMATIC => "aromatic",
        BOND_DATIVE => "dative",
        BOND_METALLIC => "metallic",
        _ => "unknown",
    }
}

/// Parse a bond order name to its numeric value.
fn parse_bond_order_name(name: &str) -> Option<u8> {
    match name.to_lowercase().as_str() {
        "single" | "1" => Some(BOND_SINGLE),
        "double" | "2" => Some(BOND_DOUBLE),
        "triple" | "3" => Some(BOND_TRIPLE),
        "quadruple" | "4" => Some(BOND_QUADRUPLE),
        "aromatic" | "5" => Some(BOND_AROMATIC),
        "dative" | "6" => Some(BOND_DATIVE),
        "metallic" | "7" => Some(BOND_METALLIC),
        _ => None,
    }
}

/// Serialize a diff `AtomicStructure` to human-readable text format.
///
/// Format:
/// - `+El @ (x, y, z)` — atom addition (new atom, no base match expected)
/// - `~El @ (x, y, z)` — atom replacement (matches base atom at same position, changes element)
/// - `~El @ (x, y, z) [from (ox, oy, oz)]` — atom move (matches base atom at anchor, placed at new position)
/// - `- @ (x, y, z)` — atom delete marker
/// - `bond A-B order_name` — bond between atom lines A and B
/// - `unbond A-B` — bond delete marker between atom lines A and B
///
/// The `~` prefix indicates the atom is intended to match a base atom (replacement or move).
/// The `+` prefix indicates a pure addition. Both are functionally equivalent in the diff
/// algorithm (positional matching determines the actual effect), but `~` preserves user intent.
///
/// Atom line numbers (A, B) are 1-indexed, referring to the sequential order of atom entries.
pub fn serialize_diff(diff: &AtomicStructure) -> String {
    let mut lines = Vec::new();
    let mut atom_id_to_line: HashMap<u32, usize> = HashMap::new();
    let mut line_num = 1;

    // Collect and sort atom IDs for deterministic output
    let mut atom_ids: Vec<u32> = diff.iter_atoms().map(|(id, _)| *id).collect();
    atom_ids.sort();

    for &atom_id in &atom_ids {
        let atom = diff.get_atom(atom_id).unwrap();
        atom_id_to_line.insert(atom_id, line_num);
        line_num += 1;

        let pos = format_position(&atom.position);

        if atom.atomic_number == DELETED_SITE_ATOMIC_NUMBER {
            lines.push(format!("- @ {}", pos));
        } else if let Some(anchor) = diff.anchor_position(atom_id) {
            let el = element_symbol(atom.atomic_number);
            // Self-anchor (anchor == position): replacement, no [from ...] needed
            // Different anchor: move, include [from ...]
            if (anchor - atom.position).length() < 1e-10 {
                lines.push(format!("~{} @ {}", el, pos));
            } else {
                let anchor_pos = format_position(anchor);
                lines.push(format!("~{} @ {} [from {}]", el, pos, anchor_pos));
            }
        } else {
            let el = element_symbol(atom.atomic_number);
            lines.push(format!("+{} @ {}", el, pos));
        }
    }

    // Collect bonds (deduplicated: only where atom_id < other_id)
    for &atom_id in &atom_ids {
        let atom = diff.get_atom(atom_id).unwrap();
        for bond in &atom.bonds {
            let other_id = bond.other_atom_id();
            if atom_id < other_id {
                if let (Some(&a), Some(&b)) = (
                    atom_id_to_line.get(&atom_id),
                    atom_id_to_line.get(&other_id),
                ) {
                    if bond.bond_order() == BOND_DELETED {
                        lines.push(format!("unbond {}-{}", a, b));
                    } else {
                        lines.push(format!(
                            "bond {}-{} {}",
                            a,
                            b,
                            bond_order_name(bond.bond_order())
                        ));
                    }
                }
            }
        }
    }

    lines.join("\n")
}

/// Parse a human-readable diff text into an `AtomicStructure` with `is_diff = true`.
///
/// See `serialize_diff` for the format specification.
pub fn parse_diff_text(text: &str) -> Result<AtomicStructure, String> {
    let mut diff = AtomicStructure::new_diff();
    // Maps 1-indexed line number to diff atom ID
    let mut line_to_atom_id: Vec<u32> = Vec::new();

    for (line_idx, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line_number = line_idx + 1;

        if let Some(rest) = line.strip_prefix('+') {
            // Addition: +El @ (x, y, z)
            let (element, position) = parse_element_and_position(rest.trim())
                .map_err(|e| format!("Line {}: {}", line_number, e))?;
            let atomic_number = resolve_element(&element)
                .ok_or_else(|| format!("Line {}: Unknown element '{}'", line_number, element))?;
            let atom_id = diff.add_atom(atomic_number, position);
            line_to_atom_id.push(atom_id);
        } else if let Some(rest) = line.strip_prefix("- ") {
            // Deletion: - @ (x, y, z)
            let rest = rest
                .trim()
                .strip_prefix('@')
                .ok_or_else(|| format!("Line {}: Expected '@' after '-'", line_number))?
                .trim();
            let position =
                parse_position(rest).map_err(|e| format!("Line {}: {}", line_number, e))?;
            let atom_id = diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, position);
            line_to_atom_id.push(atom_id);
        } else if let Some(rest) = line.strip_prefix('~') {
            // Modification: ~El @ (x, y, z) [from (ox, oy, oz)]
            // Without [from ...]: replacement at same position (anchor = position)
            let (element, position, anchor) = parse_modification(rest.trim())
                .map_err(|e| format!("Line {}: {}", line_number, e))?;
            let atomic_number = resolve_element(&element)
                .ok_or_else(|| format!("Line {}: Unknown element '{}'", line_number, element))?;
            let atom_id = diff.add_atom(atomic_number, position);
            // Set anchor: explicit [from ...] or self-anchor (marks as modification)
            let anchor_pos = anchor.unwrap_or(position);
            diff.set_anchor_position(atom_id, anchor_pos);
            line_to_atom_id.push(atom_id);
        } else if let Some(rest) = line.strip_prefix("bond ") {
            // Bond: bond A-B order_name
            let (a, b, order) =
                parse_bond_line(rest.trim()).map_err(|e| format!("Line {}: {}", line_number, e))?;
            let &atom_a = line_to_atom_id
                .get(a - 1)
                .ok_or_else(|| format!("Line {}: Atom index {} out of range", line_number, a))?;
            let &atom_b = line_to_atom_id
                .get(b - 1)
                .ok_or_else(|| format!("Line {}: Atom index {} out of range", line_number, b))?;
            diff.add_bond(atom_a, atom_b, order);
        } else if let Some(rest) = line.strip_prefix("unbond ") {
            // Bond deletion: unbond A-B
            let (a, b) =
                parse_atom_pair(rest.trim()).map_err(|e| format!("Line {}: {}", line_number, e))?;
            let &atom_a = line_to_atom_id
                .get(a - 1)
                .ok_or_else(|| format!("Line {}: Atom index {} out of range", line_number, a))?;
            let &atom_b = line_to_atom_id
                .get(b - 1)
                .ok_or_else(|| format!("Line {}: Atom index {} out of range", line_number, b))?;
            diff.add_bond(atom_a, atom_b, BOND_DELETED);
        } else {
            return Err(format!(
                "Line {}: Unrecognized diff entry: '{}'",
                line_number, line
            ));
        }
    }

    Ok(diff)
}

/// Resolve an element symbol to an atomic number.
fn resolve_element(symbol: &str) -> Option<i16> {
    // Try as-is first, then normalized
    if let Some(&n) = CHEMICAL_ELEMENTS.get(symbol) {
        return Some(n as i16);
    }
    let normalized = normalize_element_symbol(symbol);
    CHEMICAL_ELEMENTS.get(&normalized).map(|&n| n as i16)
}

/// Parse "El @ (x, y, z)" into (element, position).
fn parse_element_and_position(text: &str) -> Result<(String, DVec3), String> {
    let at_idx = text.find('@').ok_or("Expected '@'")?;
    let element = text[..at_idx].trim().to_string();
    if element.is_empty() {
        return Err("Missing element symbol".to_string());
    }
    let pos_str = text[at_idx + 1..].trim();
    let position = parse_position(pos_str)?;
    Ok((element, position))
}

/// Parse "El @ (x, y, z) [from (ox, oy, oz)]" into (element, position, optional anchor).
fn parse_modification(text: &str) -> Result<(String, DVec3, Option<DVec3>), String> {
    let at_idx = text.find('@').ok_or("Expected '@'")?;
    let element = text[..at_idx].trim().to_string();
    if element.is_empty() {
        return Err("Missing element symbol".to_string());
    }

    let rest = text[at_idx + 1..].trim();

    if let Some(from_idx) = rest.find("[from") {
        let pos_str = rest[..from_idx].trim();
        let position = parse_position(pos_str)?;

        let from_str = rest[from_idx..].trim();
        let from_str = from_str
            .strip_prefix("[from")
            .ok_or("Expected '[from'")?
            .trim();
        let from_str = from_str.strip_suffix(']').ok_or("Expected closing ']'")?;
        let anchor = parse_position(from_str.trim())?;

        Ok((element, position, Some(anchor)))
    } else {
        let position = parse_position(rest)?;
        Ok((element, position, None))
    }
}

/// Parse a position "(x, y, z)" into `DVec3`.
fn parse_position(text: &str) -> Result<DVec3, String> {
    let text = text.trim();
    let inner = text
        .strip_prefix('(')
        .ok_or("Expected '(' for position")?
        .strip_suffix(')')
        .ok_or("Expected ')' for position")?;

    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return Err(format!(
            "Expected 3 components in position, got {}",
            parts.len()
        ));
    }

    let x: f64 = parts[0]
        .trim()
        .parse()
        .map_err(|_| format!("Invalid x coordinate: '{}'", parts[0].trim()))?;
    let y: f64 = parts[1]
        .trim()
        .parse()
        .map_err(|_| format!("Invalid y coordinate: '{}'", parts[1].trim()))?;
    let z: f64 = parts[2]
        .trim()
        .parse()
        .map_err(|_| format!("Invalid z coordinate: '{}'", parts[2].trim()))?;

    Ok(DVec3::new(x, y, z))
}

/// Parse "A-B order_name" for a bond line.
fn parse_bond_line(text: &str) -> Result<(usize, usize, u8), String> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() != 2 {
        return Err("Expected format: A-B order_name".to_string());
    }

    let (a, b) = parse_atom_pair(parts[0])?;
    let order = parse_bond_order_name(parts[1])
        .ok_or_else(|| format!("Unknown bond order: '{}'", parts[1]))?;

    Ok((a, b, order))
}

/// Parse "A-B" atom pair into 1-indexed line numbers.
fn parse_atom_pair(text: &str) -> Result<(usize, usize), String> {
    let dash_idx = text.find('-').ok_or("Expected '-' between atom indices")?;
    let a: usize = text[..dash_idx]
        .trim()
        .parse()
        .map_err(|_| format!("Invalid atom index: '{}'", text[..dash_idx].trim()))?;
    let b: usize = text[dash_idx + 1..]
        .trim()
        .parse()
        .map_err(|_| format!("Invalid atom index: '{}'", text[dash_idx + 1..].trim()))?;
    if a == 0 || b == 0 {
        return Err("Atom indices are 1-based".to_string());
    }
    Ok((a, b))
}
