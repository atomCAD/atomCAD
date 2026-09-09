//! Value types of the two mechanosynthesis files, after parsing and validation,
//! plus the crate's error type.
//!
//! Nothing here derives `Deserialize`: the JSON shapes are private to
//! [`super::parse`], which validates them and produces these types. That keeps
//! "what a file may say" and "what the engine may assume" separate — every
//! value reaching [`super::apply`] has already had its element symbols resolved
//! to atomic numbers, its bond orders range-checked and its matrix squared off.

use glam::{DMat3, DVec3};
use rustc_hash::FxHashMap;
use thiserror::Error;

/// The `format` string an operation library must carry.
pub const LIBRARY_FORMAT: &str = "atomcad-msops/1";

/// The `format` string a build script must carry.
pub const BUILD_FORMAT: &str = "atomcad-msbuild/1";

/// Match tolerance used when neither file states one, in Ångström.
pub const DEFAULT_TOLERANCE: f64 = 0.3;

/// Below this distance (Å) two pattern positions are "the same position", so an
/// id present in both `before` and `after` is *kept* rather than *moved*.
/// Patterns are machine-written, so this is a constant rather than a knob.
pub const PATTERN_POSITION_EPSILON: f64 = 1e-6;

/// The element slot of a pattern atom.
///
/// `"*"` means "no element comparison": in `before` it matches any element, in
/// `after` on a kept id it leaves the workpiece atom's element alone. It is
/// rejected on an id that only `after` has — an added atom needs an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternElement {
    /// `"*"`.
    Any,
    /// A concrete element, as an atomic number.
    Element(i16),
}

impl PatternElement {
    /// Whether a workpiece atom of `atomic_number` may match this slot.
    pub fn matches(&self, atomic_number: i16) -> bool {
        match self {
            PatternElement::Any => true,
            PatternElement::Element(z) => *z == atomic_number,
        }
    }
}

/// One atom of a `before` or `after` pattern, in the operation's local frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternAtom {
    /// Small integer naming this atom within the operation. The same id in
    /// `before` and `after` denotes the same atom.
    pub id: i64,
    pub element: PatternElement,
    /// Position in the operation's local frame, Ångström.
    pub pos: DVec3,
}

/// One bond of a pattern. Endpoints are pattern ids, not workpiece atom ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatternBond {
    pub a: i64,
    pub b: i64,
    /// 1..=7, defaulting to 1 when the file omits it.
    pub order: u8,
}

impl PatternBond {
    /// The endpoint pair in a canonical order, so `[a, b]` and `[b, a]` name the
    /// same bond.
    pub fn key(&self) -> (i64, i64) {
        if self.a <= self.b {
            (self.a, self.b)
        } else {
            (self.b, self.a)
        }
    }
}

/// The `before` or the `after` half of an operation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pattern {
    pub atoms: Vec<PatternAtom>,
    pub bonds: Vec<PatternBond>,
}

impl Pattern {
    /// The atom with this pattern id, if the pattern has one. Patterns hold a
    /// handful of atoms, so a scan beats a map.
    pub fn atom(&self, id: i64) -> Option<&PatternAtom> {
        self.atoms.iter().find(|a| a.id == id)
    }

    /// Whether this pattern names the given id.
    pub fn has(&self, id: i64) -> bool {
        self.atom(id).is_some()
    }
}

/// A named before/after rewrite in a local frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    pub name: String,
    pub before: Pattern,
    pub after: Pattern,
}

/// A parsed operation library. `file` is the label errors name; it is the path
/// the library was read from, or whatever label the caller passed to
/// [`super::parse_library`].
#[derive(Debug, Clone, PartialEq)]
pub struct OpLibrary {
    pub file: String,
    /// The library's default match tolerance; a build script's own `tolerance`
    /// wins over it.
    pub tolerance: Option<f64>,
    pub ops: Vec<Operation>,
    /// `name` → index into `ops`. Built at parse time; names are unique.
    index: FxHashMap<String, usize>,
}

impl OpLibrary {
    /// Builds the name index. `ops` must already have unique names (the parser
    /// checks that before calling this).
    pub(super) fn new(file: String, tolerance: Option<f64>, ops: Vec<Operation>) -> Self {
        let index = ops
            .iter()
            .enumerate()
            .map(|(i, op)| (op.name.clone(), i))
            .collect();
        Self {
            file,
            tolerance,
            ops,
            index,
        }
    }

    /// The operation with this name, if the library has one.
    pub fn get(&self, name: &str) -> Option<&Operation> {
        self.index.get(name).map(|&i| &self.ops[i])
    }
}

/// One application of an operation at a rigid transform.
///
/// `p_workpiece = r · p_local + t`.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// Name of the operation in the library.
    pub op: String,
    /// Translation, Ångström.
    pub t: DVec3,
    /// Rotation applied before the translation; identity when the file omits
    /// it. Improper rotations (det −1) are allowed — they are lattice symmetry
    /// operations a generator may want.
    pub r: DMat3,
    /// Free text; the property panel shows it for the current step.
    pub note: Option<String>,
}

impl Step {
    /// A step with the identity rotation and no note.
    pub fn new(op: impl Into<String>, t: DVec3) -> Self {
        Self {
            op: op.into(),
            t,
            r: DMat3::IDENTITY,
            note: None,
        }
    }

    /// The workpiece position of a local-frame point.
    pub fn place(&self, local: DVec3) -> DVec3 {
        self.r * local + self.t
    }
}

/// A parsed build script. `file` is the label errors name.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildScript {
    pub file: String,
    /// Match tolerance for every step; overrides the library's.
    pub tolerance: Option<f64>,
    pub steps: Vec<Step>,
}

/// Everything that can go wrong reading or replaying a mechanosynthesis build.
///
/// Every variant names the file it came from; validation variants additionally
/// name the operation or the step and the offending field, because the audience
/// is whoever wrote the generator that emitted the file.
#[derive(Debug, Error)]
pub enum MechanosynthError {
    #[error("{file}: {source}")]
    Io {
        file: String,
        source: std::io::Error,
    },

    #[error("{file}: invalid JSON: {message}")]
    Json { file: String, message: String },

    /// A schema or consistency problem. `location` names the op and pattern, or
    /// the step, that the message is about.
    #[error("{file}: {location}: {message}")]
    Invalid {
        file: String,
        location: String,
        message: String,
    },

    /// A `before` atom found no workpiece atom within tolerance.
    #[error(
        "step {step} ({op} @ ({t_x:.3}, {t_y:.3}, {t_z:.3})) — before atom id {atom_id} ({element}) \
         not found within {tolerance:.2} Å; {nearest}"
    )]
    NoMatch {
        /// 1-based, as everything user-facing about steps is.
        step: usize,
        op: String,
        t_x: f64,
        t_y: f64,
        t_z: f64,
        atom_id: i64,
        /// The pattern's element symbol, or `*`.
        element: String,
        tolerance: f64,
        /// `nearest atom is H at 0.91 Å`, or a note that there is none.
        nearest: String,
    },
}

impl MechanosynthError {
    /// The file label this error is about.
    pub fn file(&self) -> Option<&str> {
        match self {
            MechanosynthError::Io { file, .. }
            | MechanosynthError::Json { file, .. }
            | MechanosynthError::Invalid { file, .. } => Some(file),
            MechanosynthError::NoMatch { .. } => None,
        }
    }
}
