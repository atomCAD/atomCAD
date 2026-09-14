//! The `BuildStep` record: the network's view of one step of a build script.
//!
//! A build script travels the network as `[BuildStep]` — an ordinary array of
//! an ordinary named record — so generated blocks, hand-authored blocks and
//! loaded files concatenate with the array nodes that already exist. This
//! module is the one place the record and the engine's [`Step`] convert into
//! each other, so the two cannot drift.
//!
//! It sits beside `nodes/mechanosynth.rs`'s `step_record`, which builds the
//! *replayer's* `MechanosynthStep` output. The two records are deliberately
//! distinct types: `MechanosynthStep` carries replay provenance (`index`,
//! `count`) that an authored step does not have. See
//! `doc/design_mechanosynth_editor.md`.
//!
//! **The absent-field defaults are the build file's**, in both directions: an
//! identity `r`, empty strings, `-1` for `layer` and `site`. A record that
//! states only `op` and `t` therefore round-trips to the same `Step` a JSON
//! step stating only `op` and `t` parses to.

use crate::evaluator::network_result::{NetworkResult, dmat3_to_rows, rows_to_dmat3};
use atomcad_crystolecule::mechanosynth::{NO_LAYER, NO_SITE, Step};
use glam::DMat3;

/// The name of the built-in record type this module converts. Registered in
/// `node_type_registry.rs` beside `MechanosynthStep`.
pub const BUILD_STEP_RECORD: &str = "BuildStep";

/// One engine [`Step`] as a `BuildStep` record value.
pub fn build_step_record(step: &Step) -> NetworkResult {
    NetworkResult::record(vec![
        ("op".to_string(), NetworkResult::String(step.op.clone())),
        ("t".to_string(), NetworkResult::Vec3(step.t)),
        ("r".to_string(), NetworkResult::Mat3(step.r)),
        (
            "note".to_string(),
            NetworkResult::String(step.note.clone().unwrap_or_default()),
        ),
        (
            "method".to_string(),
            NetworkResult::String(step.method.clone()),
        ),
        (
            "phase".to_string(),
            NetworkResult::String(step.phase.clone()),
        ),
        ("layer".to_string(), NetworkResult::Int(step.layer)),
        ("site".to_string(), NetworkResult::Int(step.site)),
    ])
}

/// One `BuildStep` record value as an engine [`Step`].
///
/// `index` is 1-based and appears in the failure message only, the way step
/// numbers do everywhere else in this subsystem. A field of the wrong type is
/// an error rather than a silent default: the record type pins the shapes, so
/// a mismatch here means something built the value by hand and got it wrong.
pub fn step_from_record(value: &NetworkResult, index: usize) -> Result<Step, String> {
    let field = |name: &str| value.extract_record_field(name);
    let bad =
        |name: &str, expected: &str| format!("step {index}: field \"{name}\" must be {expected}");

    let op = match field("op") {
        None => return Err(format!("step {index}: missing field \"op\"")),
        Some(NetworkResult::String(op)) => op.clone(),
        Some(_) => return Err(bad("op", "a string")),
    };
    let t = match field("t") {
        None => return Err(format!("step {index}: missing field \"t\"")),
        Some(NetworkResult::Vec3(t)) => *t,
        Some(_) => return Err(bad("t", "a Vec3")),
    };
    let r = match field("r") {
        None => DMat3::IDENTITY,
        Some(NetworkResult::Mat3(r)) => *r,
        // An all-integer 3x3 literal lexes as `IMat3`; the wire rule coerces it
        // to `Mat3`, but a record built inside `expr` can still arrive as one.
        Some(NetworkResult::IMat3(rows)) => rows_to_dmat3(&[
            [rows[0][0] as f64, rows[0][1] as f64, rows[0][2] as f64],
            [rows[1][0] as f64, rows[1][1] as f64, rows[1][2] as f64],
            [rows[2][0] as f64, rows[2][1] as f64, rows[2][2] as f64],
        ]),
        Some(_) => return Err(bad("r", "a Mat3")),
    };

    let text = |name: &str| -> Result<String, String> {
        match field(name) {
            None => Ok(String::new()),
            Some(NetworkResult::String(text)) => Ok(text.clone()),
            Some(_) => Err(bad(name, "a string")),
        }
    };
    let number = |name: &str, absent: i32| -> Result<i32, String> {
        match field(name) {
            None => Ok(absent),
            Some(NetworkResult::Int(value)) => Ok(*value),
            Some(_) => Err(bad(name, "an integer")),
        }
    };

    let note = text("note")?;
    Ok(Step {
        op,
        t,
        r,
        // The engine's `note` is `Option<String>` because the JSON field is
        // absent or present; the record's is always a string, so the empty
        // string is "no note" in this direction.
        note: if note.is_empty() { None } else { Some(note) },
        method: text("method")?,
        phase: text("phase")?,
        layer: number("layer", NO_LAYER)?,
        site: number("site", NO_SITE)?,
    })
}

/// A whole `[BuildStep]` array as engine steps.
///
/// A non-array value is an error naming what arrived, because the only way to
/// get one is a pin type that has stopped saying `[BuildStep]`.
pub fn steps_from_array(value: &NetworkResult) -> Result<Vec<Step>, String> {
    let NetworkResult::Array(elements) = value else {
        return Err(format!(
            "expected an array of BuildStep records, got {:?}",
            value.infer_data_type()
        ));
    };
    elements
        .iter()
        .enumerate()
        .map(|(index, element)| step_from_record(element, index + 1))
        .collect()
}

/// Whether a step's rotation is the identity, to the file's own rounding.
///
/// The exporter omits an identity `r`, and the text format does the same, so
/// both ask this question and it is answered once.
pub fn is_identity_rotation(r: &DMat3) -> bool {
    const EPSILON: f64 = 1e-9;
    let rows = dmat3_to_rows(r);
    let identity = dmat3_to_rows(&DMat3::IDENTITY);
    rows.iter()
        .flatten()
        .zip(identity.iter().flatten())
        .all(|(a, b)| (a - b).abs() <= EPSILON)
}
