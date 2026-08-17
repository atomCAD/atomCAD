use glam::DVec3;
use thiserror::Error;

/// Error type for CIF-related operations.
#[derive(Debug, Error)]
pub enum CifError {
    #[error("Symmetry operation parse error: {0}")]
    SymmetryParse(String),
}

/// A symmetry operation parsed from a Jones' faithful notation string
/// (e.g., "x,y,z" or "-x+1/2,-y,z+1/2").
///
/// Stored as a 3×4 matrix: each row is [c_x, c_y, c_z, translation]
/// representing an affine transformation on fractional coordinates.
#[derive(Debug, Clone)]
pub struct SymmetryOperation {
    /// 3 rows of [c_x, c_y, c_z, translation].
    pub rows: [[f64; 4]; 3],
}

impl SymmetryOperation {
    /// Apply this symmetry operation to a fractional coordinate position.
    /// The result is wrapped into [0, 1) via modulo.
    pub fn apply(&self, fract: DVec3) -> DVec3 {
        let raw = self.apply_unwrapped(fract);
        DVec3::new(wrap_fract(raw.x), wrap_fract(raw.y), wrap_fract(raw.z))
    }

    /// Apply this symmetry operation without wrapping the result.
    /// Returns the raw fractional coordinates which may be outside [0, 1).
    pub fn apply_unwrapped(&self, fract: DVec3) -> DVec3 {
        let x = self.rows[0][0] * fract.x
            + self.rows[0][1] * fract.y
            + self.rows[0][2] * fract.z
            + self.rows[0][3];
        let y = self.rows[1][0] * fract.x
            + self.rows[1][1] * fract.y
            + self.rows[1][2] * fract.z
            + self.rows[1][3];
        let z = self.rows[2][0] * fract.x
            + self.rows[2][1] * fract.y
            + self.rows[2][2] * fract.z
            + self.rows[2][3];
        DVec3::new(x, y, z)
    }
}

/// Wrap a fractional coordinate into [0, 1).
/// Handles negative values and values >= 1.
fn wrap_fract(v: f64) -> f64 {
    let r = v % 1.0;
    if r < -1e-12 {
        r + 1.0
    } else if r < 0.0 {
        // Very small negative due to floating point — snap to 0
        0.0
    } else {
        r
    }
}

/// Parse a symmetry operation string in Jones' faithful notation.
///
/// Examples: `"x,y,z"`, `"-x+1/2,-y,z+1/2"`, `"1/4+y,1/4-x,3/4+z"`.
///
/// The string must contain exactly 3 comma-separated expressions. Each
/// expression is a linear combination of x, y, z variables with an optional
/// translation constant. Variables a, b, c are treated as x, y, z.
pub fn parse_symmetry_operation(s: &str) -> Result<SymmetryOperation, CifError> {
    // Strip whitespace and underscores
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();

    let parts: Vec<&str> = cleaned.split(',').collect();
    if parts.len() != 3 {
        return Err(CifError::SymmetryParse(format!(
            "Expected 3 comma-separated components, got {}: '{}'",
            parts.len(),
            s
        )));
    }

    let mut rows = [[0.0_f64; 4]; 3];
    for (i, part) in parts.iter().enumerate() {
        rows[i] = parse_expression(part).map_err(|e| {
            CifError::SymmetryParse(format!("In component {}: {} (from '{}')", i, e, s))
        })?;
    }

    Ok(SymmetryOperation { rows })
}

/// Parse a single expression component (e.g., "-x+1/2" or "1/4+y").
///
/// Returns [c_x, c_y, c_z, translation].
fn parse_expression(expr: &str) -> Result<[f64; 4], String> {
    let mut row = [0.0_f64; 4];

    if expr.is_empty() {
        return Err("Empty expression".to_string());
    }

    let bytes = expr.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    while pos < len {
        // Determine sign
        let sign: f64;
        if bytes[pos] == b'+' {
            sign = 1.0;
            pos += 1;
        } else if bytes[pos] == b'-' {
            sign = -1.0;
            pos += 1;
        } else {
            sign = 1.0;
        }

        if pos >= len {
            return Err("Trailing sign with no term".to_string());
        }

        // Check if next char is a variable
        if let Some(var_idx) = variable_index(bytes[pos]) {
            // Simple variable: +x, -y, etc.
            pos += 1;
            // Check for /integer (e.g., x/3)
            if pos < len && bytes[pos] == b'/' {
                pos += 1;
                let (denom, new_pos) = parse_number_at(expr, pos)?;
                pos = new_pos;
                row[var_idx] += sign / denom;
            } else {
                row[var_idx] += sign;
            }
        } else {
            // Must be a number — could be:
            // 1) A constant (translation): "1/2", "0.5", "3"
            // 2) A coefficient: "2*x" or "1/2*x"
            let (num, new_pos) = parse_number_at(expr, pos)?;
            pos = new_pos;

            // Check for fraction
            let value = if pos < len && bytes[pos] == b'/' {
                pos += 1;
                // Check if next is a variable (e.g., x/3 pattern won't reach here,
                // but "1/2" is a fraction)
                let (denom, new_pos2) = parse_number_at(expr, pos)?;
                pos = new_pos2;
                num / denom
            } else {
                num
            };

            // Check if followed by '*' then variable, or directly by a variable
            if pos < len && bytes[pos] == b'*' {
                pos += 1;
                if pos < len {
                    if let Some(var_idx) = variable_index(bytes[pos]) {
                        pos += 1;
                        row[var_idx] += sign * value;
                    } else {
                        return Err(format!("Expected variable after '*' at position {}", pos));
                    }
                } else {
                    return Err("Trailing '*' with no variable".to_string());
                }
            } else if pos < len {
                if let Some(var_idx) = variable_index(bytes[pos]) {
                    // Coefficient directly before variable (e.g., "2x" — rare but handle)
                    pos += 1;
                    row[var_idx] += sign * value;
                } else {
                    // Pure constant (translation)
                    row[3] += sign * value;
                }
            } else {
                // End of string — pure constant
                row[3] += sign * value;
            }
        }
    }

    Ok(row)
}

/// Map a byte to a variable index: x/a→0, y/b→1, z/c→2.
fn variable_index(b: u8) -> Option<usize> {
    match b.to_ascii_lowercase() {
        b'x' | b'a' => Some(0),
        b'y' | b'b' => Some(1),
        b'z' | b'c' => Some(2),
        _ => None,
    }
}

/// Parse a decimal or integer number starting at position `pos`.
/// Returns (value, new_position).
fn parse_number_at(expr: &str, pos: usize) -> Result<(f64, usize), String> {
    let bytes = expr.as_bytes();
    let len = bytes.len();
    let start = pos;
    let mut p = pos;

    while p < len && (bytes[p].is_ascii_digit() || bytes[p] == b'.') {
        p += 1;
    }

    if p == start {
        return Err(format!("Expected number at position {} in '{}'", pos, expr));
    }

    let num_str = &expr[start..p];
    let value: f64 = num_str
        .parse()
        .map_err(|_| format!("Invalid number '{}' at position {}", num_str, start))?;

    Ok((value, p))
}

/// An atom site from the asymmetric unit (or after expansion).
#[derive(Debug, Clone)]
pub struct CifAtomSite {
    pub label: String,
    pub element: String,
    pub fract: DVec3,
    pub occupancy: f64,
}

/// Apply all symmetry operations to the asymmetric unit, wrap into [0,1),
/// and deduplicate positions within a tolerance.
///
/// `tolerance` is in fractional coordinate units (typically ~0.01).
pub fn expand_asymmetric_unit(
    atoms: &[CifAtomSite],
    operations: &[SymmetryOperation],
    tolerance: f64,
) -> Vec<CifAtomSite> {
    let mut expanded: Vec<CifAtomSite> = Vec::new();

    for atom in atoms {
        for op in operations {
            let new_fract = op.apply(atom.fract);

            // Check for duplicate
            let is_duplicate = expanded.iter().any(|existing| {
                existing.element == atom.element
                    && fract_distance(existing.fract, new_fract) < tolerance
            });

            if !is_duplicate {
                expanded.push(CifAtomSite {
                    label: atom.label.clone(),
                    element: atom.element.clone(),
                    fract: new_fract,
                    occupancy: atom.occupancy,
                });
            }
        }
    }

    expanded
}

/// Compute the minimum-image distance between two fractional coordinates.
/// This handles wraparound at cell boundaries (e.g., 0.01 and 0.99 are
/// distance 0.02, not 0.98).
fn fract_distance(a: DVec3, b: DVec3) -> f64 {
    let dx = min_fract_diff(a.x, b.x);
    let dy = min_fract_diff(a.y, b.y);
    let dz = min_fract_diff(a.z, b.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Minimum difference between two fractional coordinates along one axis,
/// accounting for periodic boundary at 0 and 1.
fn min_fract_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    if d > 0.5 { 1.0 - d } else { d }
}
