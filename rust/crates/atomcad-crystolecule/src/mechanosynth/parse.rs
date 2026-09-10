//! JSON parsing and validation for the two mechanosynthesis files.
//!
//! The `Raw*` types below are the on-disk shapes; they are deliberately lenient
//! (unknown keys are ignored everywhere, per the format spec) and are converted
//! into the validated [`schema`] types by the `parse_*` functions. Every
//! rejection names the file, the operation or step, and the field.

use super::schema::{
    BUILD_FORMAT, BuildScript, LIBRARY_FORMAT, MechanosynthError, NO_LAYER, NO_SITE, OpLibrary,
    Operation, Pattern, PatternAtom, PatternBond, PatternElement, Step,
};
use crate::atomic_constants::CHEMICAL_ELEMENTS;
use glam::{DMat3, DVec3};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

// ---------------------------------------------------------------------------
// On-disk shapes
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawLibrary {
    format: Option<String>,
    tolerance: Option<f64>,
    ops: Option<Vec<RawOp>>,
}

#[derive(Deserialize)]
struct RawOp {
    name: Option<String>,
    before: Option<RawPattern>,
    after: Option<RawPattern>,
}

#[derive(Deserialize)]
struct RawPattern {
    atoms: Option<Vec<RawAtom>>,
    bonds: Option<Vec<Vec<serde_json::Value>>>,
}

#[derive(Deserialize)]
struct RawAtom {
    id: Option<i64>,
    el: Option<String>,
    pos: Option<Vec<f64>>,
}

#[derive(Deserialize)]
struct RawScript {
    format: Option<String>,
    tolerance: Option<f64>,
    steps: Option<Vec<RawStep>>,
}

#[derive(Deserialize)]
struct RawStep {
    op: Option<String>,
    t: Option<Vec<f64>>,
    r: Option<Vec<Vec<f64>>>,
    note: Option<String>,
    /// The four metadata fields arrive as raw JSON rather than as typed
    /// `Option`s so that a wrong type is an `Invalid` error naming the step,
    /// the way every other malformed step is reported, instead of a serde
    /// message about the whole document.
    method: Option<serde_json::Value>,
    phase: Option<serde_json::Value>,
    layer: Option<serde_json::Value>,
    site: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn invalid(
    file: &str,
    location: impl Into<String>,
    message: impl Into<String>,
) -> MechanosynthError {
    MechanosynthError::Invalid {
        file: file.to_string(),
        location: location.into(),
        message: message.into(),
    }
}

/// `operation 'habst': before pattern`
fn pattern_location(op: &str, which: &str) -> String {
    format!("operation '{op}': {which} pattern")
}

// ---------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------

/// Reads and validates an operation library from disk. `path` doubles as the
/// label errors name.
pub fn load_library(path: &Path) -> Result<OpLibrary, MechanosynthError> {
    let file = path.to_string_lossy().into_owned();
    let text = std::fs::read_to_string(path).map_err(|source| MechanosynthError::Io {
        file: file.clone(),
        source,
    })?;
    parse_library(&text, &file)
}

/// Validates an operation library given its JSON text. `file` is the label
/// errors name — a path, or anything else meaningful to the caller.
pub fn parse_library(text: &str, file: &str) -> Result<OpLibrary, MechanosynthError> {
    let raw: RawLibrary = serde_json::from_str(text).map_err(|e| MechanosynthError::Json {
        file: file.to_string(),
        message: e.to_string(),
    })?;

    check_format(file, raw.format.as_deref(), LIBRARY_FORMAT)?;

    let raw_ops = raw
        .ops
        .ok_or_else(|| invalid(file, "top level", "missing required field \"ops\""))?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut ops = Vec::with_capacity(raw_ops.len());
    for (i, raw_op) in raw_ops.into_iter().enumerate() {
        let name = raw_op.name.ok_or_else(|| {
            invalid(
                file,
                format!("operation #{}", i + 1),
                "missing required field \"name\"",
            )
        })?;
        if !seen.insert(name.clone()) {
            return Err(invalid(
                file,
                format!("operation '{name}'"),
                "duplicate operation name",
            ));
        }
        let before = raw_op.before.ok_or_else(|| {
            invalid(
                file,
                format!("operation '{name}'"),
                "missing required field \"before\"",
            )
        })?;
        let after = raw_op.after.ok_or_else(|| {
            invalid(
                file,
                format!("operation '{name}'"),
                "missing required field \"after\"",
            )
        })?;

        let before = convert_pattern(file, &name, "before", before)?;
        let after = convert_pattern(file, &name, "after", after)?;

        // `"*"` on an id only `after` has would leave an added atom without an
        // element. Rejected here rather than at apply time, where it would be a
        // panic or a silent guess.
        for atom in &after.atoms {
            if atom.element == PatternElement::Any && !before.has(atom.id) {
                return Err(invalid(
                    file,
                    pattern_location(&name, "after"),
                    format!(
                        "atom id {} is added by this operation, so \"el\" must name an element, not \"*\"",
                        atom.id
                    ),
                ));
            }
        }

        ops.push(Operation {
            name,
            before,
            after,
        });
    }

    Ok(OpLibrary::new(file.to_string(), raw.tolerance, ops))
}

fn convert_pattern(
    file: &str,
    op: &str,
    which: &str,
    raw: RawPattern,
) -> Result<Pattern, MechanosynthError> {
    let location = pattern_location(op, which);
    let raw_atoms = raw.atoms.unwrap_or_default();

    let mut atoms = Vec::with_capacity(raw_atoms.len());
    let mut ids: HashSet<i64> = HashSet::new();
    for (i, raw_atom) in raw_atoms.into_iter().enumerate() {
        let id = raw_atom.id.ok_or_else(|| {
            invalid(
                file,
                &location,
                format!("atom #{} is missing required field \"id\"", i + 1),
            )
        })?;
        if !ids.insert(id) {
            return Err(invalid(file, &location, format!("duplicate atom id {id}")));
        }
        let el = raw_atom.el.ok_or_else(|| {
            invalid(
                file,
                &location,
                format!("atom id {id} is missing required field \"el\""),
            )
        })?;
        let element = parse_element(file, &location, id, &el)?;
        let pos = raw_atom.pos.ok_or_else(|| {
            invalid(
                file,
                &location,
                format!("atom id {id} is missing required field \"pos\""),
            )
        })?;
        if pos.len() != 3 {
            return Err(invalid(
                file,
                &location,
                format!(
                    "atom id {id}: \"pos\" must be an array of 3 numbers, found {}",
                    pos.len()
                ),
            ));
        }
        atoms.push(PatternAtom {
            id,
            element,
            pos: DVec3::new(pos[0], pos[1], pos[2]),
        });
    }

    let raw_bonds = raw.bonds.unwrap_or_default();
    let mut bonds = Vec::with_capacity(raw_bonds.len());
    for raw_bond in raw_bonds {
        let (a, b, order) = convert_bond(file, &location, &raw_bond)?;
        for endpoint in [a, b] {
            if !ids.contains(&endpoint) {
                return Err(invalid(
                    file,
                    &location,
                    format!("bond [{a}, {b}] references unknown atom id {endpoint}"),
                ));
            }
        }
        if a == b {
            return Err(invalid(
                file,
                &location,
                format!("bond [{a}, {b}] connects an atom to itself"),
            ));
        }
        bonds.push(PatternBond { a, b, order });
    }

    Ok(Pattern { atoms, bonds })
}

/// A bond is `[a, b]` or `[a, b, order]`; `order` defaults to 1 and must name a
/// bond type `InlineBond` can hold (1..=7; 0 is its delete marker).
fn convert_bond(
    file: &str,
    location: &str,
    raw: &[serde_json::Value],
) -> Result<(i64, i64, u8), MechanosynthError> {
    if raw.len() != 2 && raw.len() != 3 {
        return Err(invalid(
            file,
            location,
            format!(
                "a bond must be [idA, idB] or [idA, idB, order], found {} entries",
                raw.len()
            ),
        ));
    }
    let mut ids = [0i64; 2];
    for (i, slot) in ids.iter_mut().enumerate() {
        *slot = raw[i].as_i64().ok_or_else(|| {
            invalid(
                file,
                location,
                format!("bond endpoint {} is not an integer atom id", i + 1),
            )
        })?;
    }
    let order = match raw.get(2) {
        None => 1u8,
        Some(value) => {
            let order = value.as_i64().ok_or_else(|| {
                invalid(
                    file,
                    location,
                    format!("bond [{}, {}]: order is not an integer", ids[0], ids[1]),
                )
            })?;
            if !(1..=7).contains(&order) {
                return Err(invalid(
                    file,
                    location,
                    format!(
                        "bond [{}, {}]: unsupported bond order {order} (must be 1..=7)",
                        ids[0], ids[1]
                    ),
                ));
            }
            order as u8
        }
    };
    Ok((ids[0], ids[1], order))
}

fn parse_element(
    file: &str,
    location: &str,
    id: i64,
    symbol: &str,
) -> Result<PatternElement, MechanosynthError> {
    if symbol == "*" {
        return Ok(PatternElement::Any);
    }
    match CHEMICAL_ELEMENTS.get(symbol) {
        Some(&z) => Ok(PatternElement::Element(z as i16)),
        None => Err(invalid(
            file,
            location,
            format!("atom id {id}: unknown element symbol \"{symbol}\""),
        )),
    }
}

// ---------------------------------------------------------------------------
// Build script
// ---------------------------------------------------------------------------

/// Reads and validates a build script from disk. `path` doubles as the label
/// errors name.
pub fn load_build_script(path: &Path) -> Result<BuildScript, MechanosynthError> {
    let file = path.to_string_lossy().into_owned();
    let text = std::fs::read_to_string(path).map_err(|source| MechanosynthError::Io {
        file: file.clone(),
        source,
    })?;
    parse_build_script(&text, &file)
}

/// Validates a build script given its JSON text. Steps are checked for shape
/// only; whether each names a real operation is a cross-file question answered
/// by [`validate_script_ops`].
pub fn parse_build_script(text: &str, file: &str) -> Result<BuildScript, MechanosynthError> {
    let raw: RawScript = serde_json::from_str(text).map_err(|e| MechanosynthError::Json {
        file: file.to_string(),
        message: e.to_string(),
    })?;

    check_format(file, raw.format.as_deref(), BUILD_FORMAT)?;

    let raw_steps = raw
        .steps
        .ok_or_else(|| invalid(file, "top level", "missing required field \"steps\""))?;

    let mut steps = Vec::with_capacity(raw_steps.len());
    for (i, raw_step) in raw_steps.into_iter().enumerate() {
        let location = format!("step {}", i + 1);
        let op = raw_step
            .op
            .ok_or_else(|| invalid(file, &location, "missing required field \"op\""))?;
        let t = raw_step
            .t
            .ok_or_else(|| invalid(file, &location, "missing required field \"t\""))?;
        if t.len() != 3 {
            return Err(invalid(
                file,
                &location,
                format!("\"t\" must be an array of 3 numbers, found {}", t.len()),
            ));
        }
        let r = match raw_step.r {
            None => DMat3::IDENTITY,
            Some(rows) => convert_matrix(file, &location, &rows)?,
        };
        steps.push(Step {
            op,
            t: DVec3::new(t[0], t[1], t[2]),
            r,
            note: raw_step.note,
            method: optional_string(file, &location, "method", raw_step.method)?,
            phase: optional_string(file, &location, "phase", raw_step.phase)?,
            layer: optional_int(file, &location, "layer", raw_step.layer, NO_LAYER)?,
            site: optional_int(file, &location, "site", raw_step.site, NO_SITE)?,
        });
    }

    Ok(BuildScript {
        file: file.to_string(),
        tolerance: raw.tolerance,
        steps,
    })
}

/// An optional per-step string field (`method`, `phase`). Absent is the empty
/// string; a present non-string names the step and the field, like every other
/// malformed step.
fn optional_string(
    file: &str,
    location: &str,
    field: &str,
    value: Option<serde_json::Value>,
) -> Result<String, MechanosynthError> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(String::new()),
        Some(serde_json::Value::String(text)) => Ok(text),
        Some(_) => Err(invalid(
            file,
            location,
            format!("\"{field}\" must be a string"),
        )),
    }
}

/// An optional per-step integer field (`layer`, `site`). Absent is `default`;
/// a present non-integer — a float, a string, or a value outside `i32` —
/// names the step and the field.
fn optional_int(
    file: &str,
    location: &str,
    field: &str,
    value: Option<serde_json::Value>,
    default: i32,
) -> Result<i32, MechanosynthError> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(default),
        Some(value) => value
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| invalid(file, location, format!("\"{field}\" must be an integer"))),
    }
}

/// The file gives `r` as three **rows**, because that is how a matrix reads on
/// paper and how `p = r · p_local` is written. `DMat3` is column-major, so the
/// rows are transposed into columns here — exactly once, at the boundary.
fn convert_matrix(
    file: &str,
    location: &str,
    rows: &[Vec<f64>],
) -> Result<DMat3, MechanosynthError> {
    let malformed = || invalid(file, location, "\"r\" must be a 3×3 array of numbers");
    if rows.len() != 3 || rows.iter().any(|row| row.len() != 3) {
        return Err(malformed());
    }
    Ok(DMat3::from_cols(
        DVec3::new(rows[0][0], rows[1][0], rows[2][0]),
        DVec3::new(rows[0][1], rows[1][1], rows[2][1]),
        DVec3::new(rows[0][2], rows[1][2], rows[2][2]),
    ))
}

/// Checks that every step of `script` names an operation `library` has.
///
/// This is the one validation that cannot happen while parsing a single file,
/// so it is a separate call; [`super::replay`] makes it before applying
/// anything, and the node makes it as soon as both files are loaded.
pub fn validate_script_ops(
    script: &BuildScript,
    library: &OpLibrary,
) -> Result<(), MechanosynthError> {
    for (i, step) in script.steps.iter().enumerate() {
        if library.get(&step.op).is_none() {
            return Err(invalid(
                &script.file,
                format!("step {}", i + 1),
                format!("unknown operation '{}' (not in {})", step.op, library.file),
            ));
        }
    }
    Ok(())
}

fn check_format(file: &str, found: Option<&str>, expected: &str) -> Result<(), MechanosynthError> {
    match found {
        Some(f) if f == expected => Ok(()),
        Some(f) => Err(invalid(
            file,
            "top level",
            format!("\"format\" must be \"{expected}\", found \"{f}\""),
        )),
        None => Err(invalid(
            file,
            "top level",
            format!("missing required field \"format\" (expected \"{expected}\")"),
        )),
    }
}
