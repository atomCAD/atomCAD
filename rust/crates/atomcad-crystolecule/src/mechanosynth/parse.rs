//! JSON parsing and validation for the two mechanosynthesis files.
//!
//! The `Raw*` types below are the on-disk shapes; they are deliberately lenient
//! (unknown keys are ignored everywhere, per the format spec) and are converted
//! into the validated [`schema`] types by the `parse_*` functions. Every
//! rejection names the file, the operation or step, and the field.

use super::schema::{
    APEX_FRAME_TAG, BUILD_FORMAT, BuildScript, CLOSE_PAIR_WARNING_FACTOR, DEFAULT_ANCHORS,
    DEFAULT_DURATION, FRAME_COPLANAR_EPSILON, FrameAtom, LIBRARY_FORMAT, MAX_PATTERN_DEGREE,
    MechanosynthError, Method, NO_LAYER, NO_SITE, ORIGIN_PATTERN_ATOM_ID, OpLibrary, Operation,
    PATTERN_POSITION_EPSILON, Pattern, PatternAtom, PatternBond, PatternElement, Reaction, Step,
    ToolSide, ToolType, is_frame_atom,
};
use super::trajectory::Envelope;
use crate::atomic_constants::{ATOM_INFO, CHEMICAL_ELEMENTS, element_symbol};
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
    /// The library's own steric factor; the engine's `CLASH_BLOCK` when absent.
    clash: Option<serde_json::Value>,
    ops: Option<Vec<RawOp>>,
    /// Required whenever any operation is `tip`; absent is an empty list, which
    /// only a library of `bulk` and `spontaneous` operations can get away with.
    tools: Option<Vec<RawToolType>>,
}

#[derive(Deserialize)]
struct RawToolType {
    name: Option<String>,
    note: Option<String>,
    /// Absent means "no symbolic state"; present and empty is a parse error,
    /// because an empty vocabulary has no initial state to start in.
    states: Option<Vec<String>>,
    frame: Option<Vec<RawFrameAtom>>,
    /// Required since `/4`. Raw rather than typed so that a wrong shape names
    /// the tool type like every other malformed field.
    envelope: Option<RawEnvelope>,
}

#[derive(Deserialize)]
struct RawEnvelope {
    half_angle: Option<f64>,
    radius: Option<f64>,
}

#[derive(Deserialize)]
struct RawFrameAtom {
    tag: Option<String>,
    pos: Option<Vec<f64>>,
}

#[derive(Deserialize)]
struct RawToolSide {
    #[serde(rename = "type")]
    tool_type: Option<String>,
    from: Option<String>,
    to: Option<String>,
    /// Absent is an empty pattern: a tool side that asserts no change is the
    /// whole tool side of a bare probe.
    before: Option<RawPattern>,
    after: Option<RawPattern>,
}

#[derive(Deserialize)]
struct RawOp {
    name: Option<String>,
    /// The library author's one-line description of the reaction. Read only by
    /// the editor's palette and offer list; the engine never interprets it.
    note: Option<String>,
    before: Option<RawPattern>,
    after: Option<RawPattern>,
    /// Absent means false. Unknown to the pre-editor engine, which ignored it
    /// with every other unknown key — the key set is additive, so a file
    /// carrying it loads on an old build.
    chiral: Option<bool>,
    /// Required since `/2`. Raw rather than a typed `Option<String>` so that a
    /// wrong type names the operation like every other malformed field.
    method: Option<serde_json::Value>,
    agent: Option<String>,
    tool: Option<RawToolSide>,
    /// How many of the leading `before` ids a click may play; 1 when absent.
    /// Raw so that a wrong type names the operation like every other malformed
    /// field.
    anchors: Option<serde_json::Value>,
    /// Required on `tip` since `/4`, forbidden elsewhere: the two points that
    /// place the tool at the moment of reaction.
    reaction: Option<RawReaction>,
    /// Optional on any operation; `DEFAULT_DURATION` when absent. Reserved for
    /// the clock half of milestone 2 and read by nothing here.
    duration: Option<f64>,
}

#[derive(Deserialize)]
struct RawReaction {
    target: Option<Vec<f64>>,
    tool: Option<Vec<f64>>,
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
    /// The bond count the matched workpiece atom must have. Raw so that a wrong
    /// type names the operation and the atom.
    deg: Option<serde_json::Value>,
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
    /// The three metadata fields arrive as raw JSON rather than as typed
    /// `Option`s so that a wrong type is an `Invalid` error naming the step,
    /// the way every other malformed step is reported, instead of a serde
    /// message about the whole document.
    ///
    /// `method` is **not** among them any more: the kind is a fact about the
    /// reaction, so it lives on the operation, and a `method` key in a step is
    /// the unknown key the loader has always skipped.
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

    // Tool types come first: an operation's `tool.type` has to name one, and
    // the frame tag names an operation's type name may not collide with.
    let tools = convert_tool_types(file, raw.tools.unwrap_or_default())?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut ops = Vec::with_capacity(raw_ops.len());
    let mut warnings: Vec<String> = Vec::new();
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

        // The origin convention: `before` atom 1 sits at the origin. A
        // violation is advisory (see `OpLibrary::warnings`), never an error.
        match before.atom(ORIGIN_PATTERN_ATOM_ID) {
            None => warnings.push(format!(
                "operation '{name}': the before pattern has no atom id \
                 {ORIGIN_PATTERN_ATOM_ID}, so it does not name the atom to click"
            )),
            Some(atom) if atom.pos.length() > PATTERN_POSITION_EPSILON => warnings.push(format!(
                "operation '{name}': before atom id {ORIGIN_PATTERN_ATOM_ID} is at \
                     ({:.3}, {:.3}, {:.3}), not at the origin",
                atom.pos.x, atom.pos.y, atom.pos.z
            )),
            Some(_) => {}
        }

        let method = convert_method(file, &name, raw_op.method)?;
        let agent = convert_agent(file, &name, method, raw_op.agent)?;
        let tool = convert_tool_side(file, &name, method, &tools, raw_op.tool)?;
        let reaction = convert_reaction(file, &name, method, raw_op.reaction)?;
        let duration = convert_duration(file, &name, raw_op.duration)?;
        let anchors = convert_anchors(file, &name, &before, &after, raw_op.anchors)?;

        let operation = Operation {
            name,
            note: raw_op.note.filter(|note| !note.is_empty()),
            before,
            after,
            chiral: raw_op.chiral.unwrap_or(false),
            method,
            agent,
            tool,
            anchors,
            reaction,
            duration,
        };
        collect_pattern_warnings(&operation, &mut warnings);
        ops.push(operation);
    }

    Ok(OpLibrary::new(
        file.to_string(),
        raw.tolerance,
        convert_clash(file, raw.clash)?,
        ops,
        tools,
        warnings,
    ))
}

/// The library-wide steric factor. A positive finite number, because it
/// multiplies a covalent-radius sum.
fn convert_clash(
    file: &str,
    raw: Option<serde_json::Value>,
) -> Result<Option<f64>, MechanosynthError> {
    match raw {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => match value.as_f64() {
            Some(clash) if clash.is_finite() && clash > 0.0 => Ok(Some(clash)),
            _ => Err(invalid(
                file,
                "top level",
                "\"clash\" must be a positive number: it multiplies a covalent-radius sum",
            )),
        },
    }
}

/// How many of the leading `before` ids a click may play.
///
/// Both rules are errors rather than warnings. An out-of-range count is a
/// generator bug; an anchor that is a **frame atom** is the case the anchor rule
/// exists to remove — a click on an atom the operation does not touch, placing
/// the reaction somewhere else entirely.
///
/// An operation that **does nothing at all** is exempt from the second rule.
/// Every atom of such an operation is a frame atom by the letter of
/// [`is_frame_atom`], and "a click that places the reaction somewhere else" has
/// no meaning where there is no reaction to place. No generated library holds
/// one; the fixtures that pin "this step touches nothing" do.
fn convert_anchors(
    file: &str,
    op: &str,
    before: &Pattern,
    after: &Pattern,
    raw: Option<serde_json::Value>,
) -> Result<i64, MechanosynthError> {
    let location = format!("operation '{op}'");
    let raw_stated = !matches!(raw, None | Some(serde_json::Value::Null));
    let anchors = match raw {
        None | Some(serde_json::Value::Null) => DEFAULT_ANCHORS,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| invalid(file, &location, "\"anchors\" must be an integer"))?,
    };
    let limit = before.atoms.len() as i64;
    if limit == 0 {
        // A pure addition names no atom to click; the script's own transform is
        // what places it. Stating `anchors` on one is a generator bug.
        if raw_stated {
            return Err(invalid(
                file,
                &location,
                "\"anchors\" is stated, but the before pattern names no atom to click",
            ));
        }
        return Ok(anchors);
    }
    if anchors < 1 || anchors > limit {
        return Err(invalid(
            file,
            &location,
            format!(
                "\"anchors\" is {anchors}; it must be in 1..={limit}, the number of before atoms"
            ),
        ));
    }
    if !changes_anything(before, after) {
        return Ok(anchors);
    }
    for atom in &before.atoms {
        if (ORIGIN_PATTERN_ATOM_ID..=anchors).contains(&atom.id)
            && is_frame_atom(before, after, atom.id)
        {
            return Err(invalid(
                file,
                &location,
                format!(
                    "\"anchors\" is {anchors}, which makes atom id {} an anchor, but the \
                     operation does not touch it — clicking it would place the reaction \
                     somewhere else",
                    atom.id
                ),
            ));
        }
    }
    Ok(anchors)
}

/// Whether the rewrite does anything to anything: an atom it adds, or one of
/// `before`'s that it does not leave exactly as it found it.
fn changes_anything(before: &Pattern, after: &Pattern) -> bool {
    after.atoms.iter().any(|atom| !before.has(atom.id))
        || before
            .atoms
            .iter()
            .any(|atom| !is_frame_atom(before, after, atom.id))
}

/// The advisory problems of one operation, each naming it.
///
/// Three checks, all warnings: none of them makes a file unusable, and each is
/// a statement about a library the author is better placed to judge than the
/// engine is. See `doc/design_mechanosynth_pattern_checks.md` §3.5.
fn collect_pattern_warnings(op: &Operation, warnings: &mut Vec<String>) {
    warn_planar_frame(op, warnings);
    warn_valence(op, warnings);
    for (which, pattern) in [("before", &op.before), ("after", &op.after)] {
        warn_close_pairs(&op.name, which, pattern, warnings);
    }
    if let Some(tool) = &op.tool {
        for (which, pattern) in [("before", &tool.before), ("after", &tool.after)] {
            warn_close_pairs(
                &format!("{}' tool side, type '{}", op.name, tool.tool_type),
                which,
                pattern,
                warnings,
            );
        }
    }
}

/// How far off a pattern's span an atom must sit (Å) before the span is treated
/// as failing to reach it, for [`warn_planar_frame`].
///
/// Not [`RANK_EPSILON`], which is the arithmetic noise floor: a pattern file
/// rounds to 1e-6 Å, so a planar frame written at an angle can come back a
/// micro-Ångström out of plane, and a threshold at the noise floor would read
/// that as a third dimension and suppress the warning. Not the match tolerance
/// either, which is a hundred times larger. A thousandth of an Ångström is well
/// above what rounding produces and well below an orientation the fit could
/// actually use.
const SPAN_EPSILON: f64 = 1e-3;

/// A `before` pattern that spans at most a plane while `after` places an atom
/// off that span.
///
/// The fit onto such a pattern cannot tell the pattern's up from its down: a
/// rectangle has an in-plane two-fold axis, so a *proper* rotation maps its
/// atoms onto themselves with everything the operation places on the other
/// side. That is the upside-down chemisorbed precursor of
/// `doc/design_mechanosynth_pattern_checks.md` §1, and the root fix is a frame
/// atom off the plane rather than any steric threshold.
///
/// A **one-atom** `before` is exempt: its orientation comes from the
/// bond-derived fallback (`needs_derived_orientation`), which is a design and
/// not an oversight.
fn warn_planar_frame(op: &Operation, warnings: &mut Vec<String>) {
    if op.before.atoms.len() < 2 {
        return;
    }
    // The span itself decides, rather than a rank: `rank_of` answers 2 for
    // everything from a plane upwards, because the *fit* only needs to know
    // that a rotation is determined. What matters here is whether `after`
    // reaches outside what `before` spans, and a `before` that spans three
    // dimensions cannot be reached outside of.
    let span: Vec<DVec3> = op.before.atoms.iter().map(|atom| atom.pos).collect();
    let off_span = op
        .after
        .atoms
        .iter()
        .any(|atom| distance_from_span(&span, atom.pos) > SPAN_EPSILON);
    if !off_span {
        return;
    }
    warnings.push(format!(
        "operation '{}': after places an atom outside what the before pattern spans, so \
         the fit cannot tell this pattern's up from its down; name a frame atom off the \
         plane",
        op.name
    ));
}

/// How far `point` lies from the affine span of `span`, which holds at least one
/// point.
fn distance_from_span(span: &[DVec3], point: DVec3) -> f64 {
    let origin = span[0];
    let mut basis: Vec<DVec3> = Vec::with_capacity(3);
    for p in &span[1..] {
        let mut v = *p - origin;
        for b in &basis {
            v -= *b * v.dot(*b);
        }
        if v.length() > SPAN_EPSILON {
            basis.push(v.normalize());
        }
    }
    let mut residual = point - origin;
    for b in &basis {
        residual -= *b * residual.dot(*b);
    }
    residual.length()
}

/// A `before` atom whose `deg`, plus what `after` bonds to it and minus what
/// `after` takes away, would exceed the element's maximum covalent valence.
///
/// A warning rather than an error, because the table is the one
/// element-specific thing in this design and a library author may know better —
/// the metals in a tool are exactly the case an open table has to allow.
fn warn_valence(op: &Operation, warnings: &mut Vec<String>) {
    for atom in &op.before.atoms {
        let (Some(deg), PatternElement::Element(z)) = (atom.deg, atom.element) else {
            continue;
        };
        let Some(limit) = max_covalent_valence(z) else {
            continue;
        };
        let delta = bond_count_delta(op, atom.id);
        let after_count = deg as i64 + delta;
        if after_count > i64::from(limit) {
            warnings.push(format!(
                "operation '{}': atom id {} is {} with deg {deg}, and the operation leaves \
                 it with {after_count} bonds, past the {limit} a {} can carry",
                op.name,
                atom.id,
                element_symbol(z),
                element_symbol(z),
            ));
        }
    }
}

/// The bonds `after` adds at `id` minus the bonds it removes, over the pairs the
/// pattern names.
fn bond_count_delta(op: &Operation, id: i64) -> i64 {
    let at = |pattern: &Pattern| -> Vec<(i64, i64)> {
        pattern
            .bonds
            .iter()
            .filter(|bond| bond.a == id || bond.b == id)
            .map(|bond| bond.key())
            .collect()
    };
    let before = at(&op.before);
    let after = at(&op.after);
    let added = after.iter().filter(|key| !before.contains(key)).count() as i64;
    let removed = before.iter().filter(|key| !after.contains(key)).count() as i64;
    added - removed
}

/// The largest number of covalent bonds an element is expected to carry, for
/// [`warn_valence`]. `None` means "this table has no opinion", which is the
/// honest answer for every element it does not list.
fn max_covalent_valence(z: i16) -> Option<u32> {
    match z {
        1 => Some(1),                // H
        6 => Some(4),                // C
        7 => Some(4),                // N
        8 => Some(2),                // O
        9 | 17 | 35 | 53 => Some(1), // F, Cl, Br, I
        14 | 32 => Some(4),          // Si, Ge
        _ => None,
    }
}

/// Two atoms of one pattern within [`CLOSE_PAIR_WARNING_FACTOR`] of their
/// covalent-radius sum with no bond listed between them.
///
/// Either the bond is missing from the file, or the library really means "these
/// two are not bonded" — in which case the workpiece had better agree, and, in
/// `after`, the pair had better clear the library's steric factor or the
/// operation is blocked on every host.
///
/// A pair with a `"*"` slot is skipped: a wildcard has no radius, and inventing
/// one would turn a real check into a guess.
fn warn_close_pairs(op: &str, which: &str, pattern: &Pattern, warnings: &mut Vec<String>) {
    for (i, a) in pattern.atoms.iter().enumerate() {
        for b in &pattern.atoms[i + 1..] {
            let (PatternElement::Element(za), PatternElement::Element(zb)) = (a.element, b.element)
            else {
                continue;
            };
            if pattern
                .bonds
                .iter()
                .any(|bond| bond.key() == (a.id.min(b.id), a.id.max(b.id)))
            {
                continue;
            }
            let (Some(ra), Some(rb)) = (covalent_radius(za), covalent_radius(zb)) else {
                continue;
            };
            let distance = a.pos.distance(b.pos);
            if distance < (ra + rb) * CLOSE_PAIR_WARNING_FACTOR {
                warnings.push(format!(
                    "operation '{op}': in the {which} pattern, atoms {} ({}) and {} ({}) \
                     are {distance:.3} Å apart with no bond between them",
                    a.id,
                    element_symbol(za),
                    b.id,
                    element_symbol(zb),
                ));
            }
        }
    }
}

fn covalent_radius(z: i16) -> Option<f64> {
    ATOM_INFO.get(&(z as i32)).map(|info| info.covalent_radius)
}

/// `tool type 'habst_tool'`
fn tool_location(name: &str) -> String {
    format!("tool type '{name}'")
}

/// The `tools` section: one descriptor per tool type the process uses.
///
/// Every rule here is a **parse error**, unlike the origin *warning* on an
/// operation: the fit onto the frame depends on all of them, so a frame that
/// breaks one would bind wrongly rather than merely read oddly.
fn convert_tool_types(
    file: &str,
    raw_tools: Vec<RawToolType>,
) -> Result<Vec<ToolType>, MechanosynthError> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut tools = Vec::with_capacity(raw_tools.len());
    for (i, raw_tool) in raw_tools.into_iter().enumerate() {
        let name = raw_tool.name.ok_or_else(|| {
            invalid(
                file,
                format!("tool type #{}", i + 1),
                "missing required field \"name\"",
            )
        })?;
        let location = tool_location(&name);
        if !seen.insert(name.clone()) {
            return Err(invalid(file, &location, "duplicate tool type name"));
        }

        let states = match raw_tool.states {
            None => None,
            Some(states) if states.is_empty() => {
                return Err(invalid(
                    file,
                    &location,
                    "\"states\" is present but empty; omit it for a type that carries no \
                     symbolic state, because an empty vocabulary has no initial state",
                ));
            }
            Some(states) => {
                let mut seen_state: HashSet<&str> = HashSet::new();
                for state in &states {
                    if !seen_state.insert(state.as_str()) {
                        return Err(invalid(
                            file,
                            &location,
                            format!("duplicate state name \"{state}\""),
                        ));
                    }
                }
                Some(states)
            }
        };

        let raw_frame = raw_tool
            .frame
            .ok_or_else(|| invalid(file, &location, "missing required field \"frame\""))?;
        let frame = convert_frame(file, &location, &name, raw_frame)?;
        let envelope = convert_envelope(file, &location, raw_tool.envelope)?;

        tools.push(ToolType {
            name,
            note: raw_tool.note.filter(|note| !note.is_empty()),
            states,
            frame,
            envelope,
        });
    }

    // A type name colliding with a frame tag name would make a design's tags
    // ambiguous — the same string would mean "plays this type" on a molecule
    // and "is this frame atom" on one of its atoms.
    let tag_names: HashSet<&str> = tools
        .iter()
        .flat_map(|tool| tool.frame.iter().map(|entry| entry.tag.as_str()))
        .collect();
    for tool in &tools {
        if tag_names.contains(tool.name.as_str()) {
            return Err(invalid(
                file,
                tool_location(&tool.name),
                "the type name is also a frame tag name, which would make a design's tags \
                 ambiguous",
            ));
        }
    }

    Ok(tools)
}

/// Four or more named positions, one of them [`APEX_FRAME_TAG`] at the origin,
/// not coplanar.
///
/// Four is the minimum because three non-collinear points are congruent to
/// their own mirror image in space: with only three, a molecule built the wrong
/// way round would superimpose on the library's frame by a proper rotation and
/// bind mirrored. The fourth, off their plane, is what turns that into a
/// residual failure.
fn convert_frame(
    file: &str,
    location: &str,
    tool_name: &str,
    raw_frame: Vec<RawFrameAtom>,
) -> Result<Vec<FrameAtom>, MechanosynthError> {
    if raw_frame.len() < 4 {
        return Err(invalid(
            file,
            location,
            format!(
                "\"frame\" needs at least 4 entries, found {}; three points are congruent to \
                 their mirror image, so four are what make a mirrored molecule fail",
                raw_frame.len()
            ),
        ));
    }

    let mut frame: Vec<FrameAtom> = Vec::with_capacity(raw_frame.len());
    let mut seen: HashSet<String> = HashSet::new();
    for (i, raw_atom) in raw_frame.into_iter().enumerate() {
        let tag = raw_atom.tag.ok_or_else(|| {
            invalid(
                file,
                location,
                format!("frame entry #{} is missing required field \"tag\"", i + 1),
            )
        })?;
        if !seen.insert(tag.clone()) {
            return Err(invalid(
                file,
                location,
                format!("duplicate frame tag \"{tag}\""),
            ));
        }
        let pos = raw_atom.pos.ok_or_else(|| {
            invalid(
                file,
                location,
                format!("frame entry \"{tag}\" is missing required field \"pos\""),
            )
        })?;
        if pos.len() != 3 {
            return Err(invalid(
                file,
                location,
                format!(
                    "frame entry \"{tag}\": \"pos\" must be an array of 3 numbers, found {}",
                    pos.len()
                ),
            ));
        }
        frame.push(FrameAtom {
            tag,
            pos: DVec3::new(pos[0], pos[1], pos[2]),
        });
    }

    match frame.iter().find(|entry| entry.tag == APEX_FRAME_TAG) {
        None => {
            return Err(invalid(
                file,
                location,
                format!("\"frame\" has no \"{APEX_FRAME_TAG}\" entry"),
            ));
        }
        Some(apex) if apex.pos.length() > PATTERN_POSITION_EPSILON => {
            return Err(invalid(
                file,
                location,
                format!(
                    "the \"{APEX_FRAME_TAG}\" entry is at ({:.3}, {:.3}, {:.3}), not at the \
                     origin of the tool's frame",
                    apex.pos.x, apex.pos.y, apex.pos.z
                ),
            ));
        }
        Some(_) => {}
    }

    // The **axis rule**: every entry but the apex lies on one side of the apex's
    // `z = 0` plane, none on it, and that side is the direction the tool axis
    // points — from the business end toward the legs. The envelope and the sweep
    // both depend on knowing which way that is, and a frame that straddles the
    // plane, or touches it, says nothing.
    let mut sign: Option<f64> = None;
    for entry in frame.iter().filter(|entry| entry.tag != APEX_FRAME_TAG) {
        if entry.pos.z.abs() <= PATTERN_POSITION_EPSILON {
            return Err(invalid(
                file,
                location,
                format!(
                    "frame entry \"{}\" of '{tool_name}' is on the apex's z = 0 plane, \
                     so the legs do not say which way the tool axis points",
                    entry.tag
                ),
            ));
        }
        let entry_sign = if entry.pos.z < 0.0 { -1.0 } else { 1.0 };
        match sign {
            None => sign = Some(entry_sign),
            Some(seen) if seen != entry_sign => {
                return Err(invalid(
                    file,
                    location,
                    format!(
                        "the frame entries of '{tool_name}' lie on both sides of the \
                         apex's z = 0 plane, so the legs do not say which way the tool axis \
                         points; every leg belongs behind the business end"
                    ),
                ));
            }
            Some(_) => {}
        }
    }

    if frame_is_coplanar(&frame) {
        return Err(invalid(
            file,
            location,
            format!(
                "the frame entries of '{tool_name}' are coplanar, so the fit onto them cannot \
                 tell a molecule from its mirror image; take the legs off the handle rather \
                 than off the business axis"
            ),
        ));
    }

    Ok(frame)
}

/// Whether every entry lies in one plane, by the largest tetrahedron any four
/// of them span. Frames hold a handful of entries, so the cubic scan is free
/// and says something a least-squares plane fit would only approximate.
fn frame_is_coplanar(frame: &[FrameAtom]) -> bool {
    let p: Vec<DVec3> = frame.iter().map(|entry| entry.pos).collect();
    for i in 0..p.len() {
        for j in (i + 1)..p.len() {
            for k in (j + 1)..p.len() {
                for l in (k + 1)..p.len() {
                    let volume = (p[j] - p[i]).cross(p[k] - p[i]).dot(p[l] - p[i]).abs() / 6.0;
                    if volume > FRAME_COPLANAR_EPSILON {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// The tool type's collision envelope. Required since `/4`: without it there is
/// no sweep, and a default would be the engine inventing a claim the library is
/// the only one able to make.
fn convert_envelope(
    file: &str,
    location: &str,
    raw: Option<RawEnvelope>,
) -> Result<Envelope, MechanosynthError> {
    let raw = raw.ok_or_else(|| {
        invalid(
            file,
            location,
            "missing required field \"envelope\" (\"half_angle\" in degrees and \"radius\" \
             in ångström); it is what the approach sweep keeps clear",
        )
    })?;

    let half_angle = raw
        .half_angle
        .ok_or_else(|| invalid(file, location, "\"envelope\" is missing \"half_angle\""))?;
    if !half_angle.is_finite() || half_angle <= 0.0 || half_angle >= 90.0 {
        return Err(invalid(
            file,
            location,
            format!(
                "\"envelope\": \"half_angle\" is {half_angle}; it must be in degrees, \
                 strictly between 0 and 90"
            ),
        ));
    }

    let radius = raw
        .radius
        .ok_or_else(|| invalid(file, location, "\"envelope\" is missing \"radius\""))?;
    if !radius.is_finite() || radius <= 0.0 {
        return Err(invalid(
            file,
            location,
            format!(
                "\"envelope\": \"radius\" is {radius}; it must be a positive length in ångström"
            ),
        ));
    }

    Ok(Envelope {
        half_angle: half_angle.to_radians(),
        radius,
    })
}

/// The operation's kind. Required, and one of the three the engine defines:
/// the vocabulary is closed because each kind is engine *behaviour*, and a
/// library cannot add a way of moving atoms in bulk without code.
fn convert_method(
    file: &str,
    op: &str,
    raw: Option<serde_json::Value>,
) -> Result<Method, MechanosynthError> {
    let location = format!("operation '{op}'");
    match raw {
        None | Some(serde_json::Value::Null) => Err(invalid(
            file,
            location,
            "missing required field \"method\" (one of \"tip\", \"bulk\", \"spontaneous\")",
        )),
        Some(serde_json::Value::String(text)) => Method::parse(&text).ok_or_else(|| {
            invalid(
                file,
                location,
                format!(
                    "\"method\" is \"{text}\"; it must be one of \"tip\", \"bulk\", \
                     \"spontaneous\""
                ),
            )
        }),
        Some(_) => Err(invalid(file, location, "\"method\" must be a string")),
    }
}

/// `agent` is required on `bulk` and forbidden elsewhere: it names the species
/// or energy that performs an exposure, and it is what batches consecutive
/// steps into one event.
fn convert_agent(
    file: &str,
    op: &str,
    method: Method,
    agent: Option<String>,
) -> Result<Option<String>, MechanosynthError> {
    let location = format!("operation '{op}'");
    match (method, agent) {
        (Method::Bulk, None) => Err(invalid(
            file,
            location,
            "a \"bulk\" operation must name the \"agent\" that performs it",
        )),
        (Method::Bulk, Some(agent)) if agent.is_empty() => Err(invalid(
            file,
            location,
            "\"agent\" is empty; a bulk operation's agent is what batches its steps into an \
             event",
        )),
        (Method::Bulk, Some(agent)) => Ok(Some(agent)),
        (_, Some(_)) => Err(invalid(
            file,
            location,
            "\"agent\" belongs to a \"bulk\" operation only; a tip operation's instrument is \
             its tool type",
        )),
        (_, None) => Ok(None),
    }
}

/// The tool side. Present exactly on a `tip` operation — **one operation, one
/// instrument**: the same reaction performed by two instruments is two
/// operations, because the two have different provenance, different tool sides
/// and different animation.
fn convert_tool_side(
    file: &str,
    op: &str,
    method: Method,
    tools: &[ToolType],
    raw: Option<RawToolSide>,
) -> Result<Option<ToolSide>, MechanosynthError> {
    let location = format!("operation '{op}'");
    let raw = match (method, raw) {
        (Method::Tip, None) => {
            return Err(invalid(
                file,
                location,
                "a \"tip\" operation must carry a \"tool\" side naming the tool type that \
                 performs it",
            ));
        }
        (Method::Bulk | Method::Spontaneous, Some(_)) => {
            return Err(invalid(
                file,
                location,
                "only a \"tip\" operation has a \"tool\" side",
            ));
        }
        (Method::Bulk | Method::Spontaneous, None) => return Ok(None),
        (Method::Tip, Some(raw)) => raw,
    };

    let tool_type = raw.tool_type.ok_or_else(|| {
        invalid(
            file,
            &location,
            "the \"tool\" side is missing required field \"type\"",
        )
    })?;
    let Some(declared) = tools.iter().find(|tool| tool.name == tool_type) else {
        return Err(invalid(
            file,
            &location,
            format!("the \"tool\" side names type '{tool_type}', which \"tools\" does not declare"),
        ));
    };

    let check_state = |which: &str, state: &Option<String>| -> Result<(), MechanosynthError> {
        let Some(state) = state else { return Ok(()) };
        if declared.has_state(state) {
            return Ok(());
        }
        Err(invalid(
            file,
            &location,
            format!(
                "the \"tool\" side's \"{which}\" is \"{state}\", which is not a state of type \
                 '{tool_type}'"
            ),
        ))
    };
    check_state("from", &raw.from)?;
    check_state("to", &raw.to)?;

    // `convert_pattern` wraps this in `operation '<label>': <which> pattern`,
    // so the label says which half of which operation the error is about.
    let tool_op_label = format!("{op}' tool side, type '{tool_type}");
    let before = convert_pattern(
        file,
        &tool_op_label,
        "before",
        raw.before.unwrap_or(RawPattern {
            atoms: None,
            bonds: None,
        }),
    )?;
    let after = convert_pattern(
        file,
        &tool_op_label,
        "after",
        raw.after.unwrap_or(RawPattern {
            atoms: None,
            bonds: None,
        }),
    )?;
    // Same rule as the target side: an id only `after` names is an atom the
    // step creates on the tool, and an added atom needs a real element.
    for atom in &after.atoms {
        if atom.element == PatternElement::Any && !before.has(atom.id) {
            return Err(invalid(
                file,
                &location,
                format!(
                    "the \"tool\" side adds atom id {}, so \"el\" must name an element, not \
                     \"*\"",
                    atom.id
                ),
            ));
        }
    }

    Ok(Some(ToolSide {
        tool_type,
        from: raw.from,
        to: raw.to,
        before,
        after,
    }))
}

/// The two reaction points. Required on a `tip` operation and forbidden
/// elsewhere, for the same reason a tool side is: a `bulk` or `spontaneous`
/// operation has no tool to place.
fn convert_reaction(
    file: &str,
    op: &str,
    method: Method,
    raw: Option<RawReaction>,
) -> Result<Option<Reaction>, MechanosynthError> {
    let location = format!("operation '{op}'");
    let raw = match (method, raw) {
        (Method::Tip, None) => {
            return Err(invalid(
                file,
                location,
                "a \"tip\" operation must carry a \"reaction\" block with a \"target\" point \
                 in the operation's frame and a \"tool\" point in the tool's frame",
            ));
        }
        (Method::Bulk | Method::Spontaneous, Some(_)) => {
            return Err(invalid(
                file,
                location,
                "only a \"tip\" operation has a \"reaction\"; nothing places a tool for a \
                 bulk or spontaneous step",
            ));
        }
        (Method::Bulk | Method::Spontaneous, None) => return Ok(None),
        (Method::Tip, Some(raw)) => raw,
    };

    let location = format!("operation '{op}': reaction");
    let point = |which: &str, raw: Option<Vec<f64>>| -> Result<DVec3, MechanosynthError> {
        let values = raw.ok_or_else(|| {
            invalid(
                file,
                &location,
                format!("missing required field \"{which}\""),
            )
        })?;
        if values.len() != 3 {
            return Err(invalid(
                file,
                &location,
                format!(
                    "\"{which}\" must be an array of 3 numbers, found {}",
                    values.len()
                ),
            ));
        }
        if !values.iter().all(|value| value.is_finite()) {
            return Err(invalid(
                file,
                &location,
                format!("\"{which}\" must be three finite numbers"),
            ));
        }
        Ok(DVec3::new(values[0], values[1], values[2]))
    };

    Ok(Some(Reaction {
        target: point("target", raw.target)?,
        tool: point("tool", raw.tool)?,
    }))
}

/// The operation's relative duration. Positive and finite, because it is a
/// length of time; absent reads [`DEFAULT_DURATION`].
fn convert_duration(file: &str, op: &str, raw: Option<f64>) -> Result<f64, MechanosynthError> {
    match raw {
        None => Ok(DEFAULT_DURATION),
        Some(duration) if duration.is_finite() && duration > 0.0 => Ok(duration),
        Some(duration) => Err(invalid(
            file,
            format!("operation '{op}'"),
            format!("\"duration\" is {duration}; it must be a positive number of relative units"),
        )),
    }
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
        let deg = match raw_atom.deg {
            None | Some(serde_json::Value::Null) => None,
            // `deg` is a statement about the workpiece the pattern *matches*,
            // so it belongs on a `before` atom. On an `after` atom it would be
            // a statement about a structure that does not exist yet, and one
            // the rewrite has already decided.
            Some(_) if which != "before" => {
                return Err(invalid(
                    file,
                    &location,
                    format!(
                        "atom id {id}: \"deg\" belongs on a before atom — it says what the \
                         workpiece must look like for the operation to apply"
                    ),
                ));
            }
            Some(value) => {
                let deg = value
                    .as_i64()
                    .filter(|deg| (0..=i64::from(MAX_PATTERN_DEGREE)).contains(deg))
                    .ok_or_else(|| {
                        invalid(
                            file,
                            &location,
                            format!(
                                "atom id {id}: \"deg\" must be an integer in 0..={MAX_PATTERN_DEGREE}"
                            ),
                        )
                    })?;
                Some(deg as u32)
            }
        };
        atoms.push(PatternAtom {
            id,
            element,
            pos: DVec3::new(pos[0], pos[1], pos[2]),
            deg,
        });
    }

    let raw_bonds = raw.bonds.unwrap_or_default();
    let mut bonds: Vec<PatternBond> = Vec::with_capacity(raw_bonds.len());
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
        // The bond list is closed-world, so one pair means one thing: two
        // entries for it would be two contradictory statements, and picking
        // either would be a guess.
        let bond = PatternBond { a, b, order };
        if bonds.iter().any(|other| other.key() == bond.key()) {
            return Err(invalid(
                file,
                &location,
                format!("bond [{a}, {b}] is listed twice; a pattern states each pair once"),
            ));
        }
        bonds.push(bond);
    }

    // A pattern cannot list more bonds at an atom than the atom has.
    for atom in &atoms {
        let Some(deg) = atom.deg else { continue };
        let listed = bonds
            .iter()
            .filter(|bond| bond.a == atom.id || bond.b == atom.id)
            .count();
        if (deg as usize) < listed {
            return Err(invalid(
                file,
                &location,
                format!(
                    "atom id {}: \"deg\" is {deg} but the pattern lists {listed} bond(s) there",
                    atom.id
                ),
            ));
        }
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

/// An optional per-step string field (`phase`). Absent is the empty
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
