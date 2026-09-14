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

/// Match tolerance used when a library states none, in Ångström.
///
/// Tight on purpose. A generated library's patterns are congruent to the
/// workpiece to floating-point precision (the frame atoms carry the
/// orientation, so nothing is derived from bonds), the files round to 1e-6, and
/// the smallest *environment* difference known — an ideal-site host against a
/// reconstructed dimer atom — is 0.12 Å. A gate an order of magnitude below
/// that admits the right variant and rejects the wrong one, where the old
/// 0.3 Å admitted both and placed an atom 0.11 Å off. A hand-written library
/// that needs slack states its own `tolerance`. See
/// `doc/design_mechanosynth_editor.md` §Exactness.
pub const DEFAULT_TOLERANCE: f64 = 0.05;

/// Below this distance (Å) two pattern positions are "the same position", so an
/// id present in both `before` and `after` is *kept* rather than *moved*.
/// Patterns are machine-written, so this is a constant rather than a knob.
pub const PATTERN_POSITION_EPSILON: f64 = 1e-6;

/// By convention the `before` atom with this id sits at the origin of the
/// operation's local frame, and is the atom the operation acts on — the one to
/// click. Validated as a **warning** at load time, never an error: a foreign
/// library that does not follow it still replays and still places.
pub const ORIGIN_PATTERN_ATOM_ID: i64 = 1;

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

    /// The element filter this slot imposes on a position match: `None` for
    /// `"*"`, which imposes none. The shape
    /// [`AtomicStructure::nearest_unclaimed_atom`](crate::atomic_structure::AtomicStructure::nearest_unclaimed_atom)
    /// takes.
    pub fn required_atomic_number(&self) -> Option<i16> {
        match self {
            PatternElement::Any => None,
            PatternElement::Element(z) => Some(*z),
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
    /// The reaction has a handedness: a mirrored placement is a *different*
    /// reaction, not the same one seen from the other side. The placement
    /// engine drops fits with `det r = −1` for such an operation.
    ///
    /// Defaults to false, because the existing libraries need mirrored fits —
    /// a chemisorption landing is placed with `det −1`. The replay engine
    /// ignores this entirely: a build file states the rotation it wants.
    pub chiral: bool,
}

impl Operation {
    /// Whether pattern id `id` is a **frame atom**: one the operation names
    /// only to fix its orientation, and does not react with.
    ///
    /// Recognisable from the two patterns alone — kept, at the same position,
    /// with the same element, taking part in no bond change — so no flag is
    /// needed in the file. A one-atom `before` carries no orientation, and
    /// Kabsch on four non-planar atoms gives the rotation exactly, which is why
    /// a donation lists the host's bonded neighbours as `"*"` atoms that appear
    /// unchanged in `after`.
    ///
    /// This is the *pattern* half of the story. Whether a step actually touched
    /// an atom is decided by effect at apply time
    /// ([`StepEffect::touched`](super::StepEffect::touched)), and a frame atom
    /// fails every clause of that rule by construction — which is why there is
    /// no "exclude the frame atoms" step anywhere.
    pub fn is_frame_atom(&self, id: i64) -> bool {
        let (Some(before), Some(after)) = (self.before.atom(id), self.after.atom(id)) else {
            return false;
        };
        if before.pos.distance(after.pos) > PATTERN_POSITION_EPSILON {
            return false;
        }
        if after.element != PatternElement::Any && after.element != before.element {
            return false;
        }
        !self.has_bond_change_at(id)
    }

    /// Whether either pattern names a bond at `id` that the other does not, or
    /// names it with a different order.
    pub fn has_bond_change_at(&self, id: i64) -> bool {
        let order_in = |pattern: &Pattern, key: (i64, i64)| {
            pattern
                .bonds
                .iter()
                .find(|bond| bond.key() == key)
                .map(|bond| bond.order)
        };
        self.before
            .bonds
            .iter()
            .map(|bond| bond.key())
            .chain(self.after.bonds.iter().map(|bond| bond.key()))
            .filter(|key| key.0 == id || key.1 == id)
            .any(|key| order_in(&self.before, key) != order_in(&self.after, key))
    }
}

/// A parsed operation library. `file` is the label errors name; it is the path
/// the library was read from, or whatever label the caller passed to
/// [`super::parse_library`].
#[derive(Debug, Clone, PartialEq)]
pub struct OpLibrary {
    pub file: String,
    /// The match tolerance for every replay against this library;
    /// [`DEFAULT_TOLERANCE`] when the file states none. One value per library,
    /// because a step array has no header to carry a second one.
    pub tolerance: Option<f64>,
    pub ops: Vec<Operation>,
    /// Advisory problems found at load time, each naming its operation.
    ///
    /// Never errors: a foreign library that does not follow the origin
    /// convention still replays and still places, since `t` falls out of the
    /// fit and the role rule falls back to the smallest eligible id. The
    /// convention names the natural atom to click, which is what makes "click
    /// the atom the operation acts on" true for every op in a library — worth
    /// telling the author about, not worth refusing the file over.
    pub warnings: Vec<String>,
    /// `name` → index into `ops`. Built at parse time; names are unique.
    index: FxHashMap<String, usize>,
}

impl OpLibrary {
    /// Builds the name index. `ops` must already have unique names (the parser
    /// checks that before calling this).
    pub(super) fn new(
        file: String,
        tolerance: Option<f64>,
        ops: Vec<Operation>,
        warnings: Vec<String>,
    ) -> Self {
        let index = ops
            .iter()
            .enumerate()
            .map(|(i, op)| (op.name.clone(), i))
            .collect();
        Self {
            file,
            tolerance,
            ops,
            warnings,
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
    /// Which instrument or process performs the step — a positional tool, area
    /// lithography, a gas exposure, a bulk photochemical or thermal step. The
    /// generator chooses the vocabulary; nothing here interprets it. Empty when
    /// the file says nothing.
    pub method: String,
    /// The chapter of the process the step belongs to. Many steps, possibly of
    /// mixed methods; the unit of the panel's chapter navigation. Empty when
    /// the file says nothing.
    pub phase: String,
    /// The terrace the step builds, counted by the generator (e.g. `1` for the
    /// first new layer over the seed). [`NO_LAYER`] means "no particular
    /// layer" — substrate work, bulk steps.
    pub layer: i32,
    /// Which of several structures built in one script the step serves.
    /// [`NO_SITE`] means "all" or "none" — a bulk step acts on every site at
    /// once.
    pub site: i32,
}

/// A step's `layer` when the file states none: "no particular layer".
pub const NO_LAYER: i32 = -1;

/// A step's `site` when the file states none: "all sites, or none".
pub const NO_SITE: i32 = -1;

impl Step {
    /// A step with the identity rotation, no note and no metadata.
    pub fn new(op: impl Into<String>, t: DVec3) -> Self {
        Self {
            op: op.into(),
            t,
            r: DMat3::IDENTITY,
            note: None,
            method: String::new(),
            phase: String::new(),
            layer: NO_LAYER,
            site: NO_SITE,
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
    /// Parsed and **ignored**. A build used to be able to override the
    /// library's tolerance; since build scripts became a network value
    /// (`[BuildStep]`, which has no header) the library's is the one value per
    /// replay. Still read so that a file stating it keeps loading.
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

    /// The library names no operation the caller asked for. A placement-time
    /// error: a *step* naming an unknown operation is caught by
    /// [`validate_script_ops`](super::validate_script_ops) instead, which can
    /// say which step it was.
    #[error("{library}: unknown operation '{op}'")]
    UnknownOp { library: String, op: String },

    /// The clicked atom is not in the workpiece at all — a stale id.
    #[error("atom {atom_id} is not in the workpiece")]
    NoSuchAtom { atom_id: u32 },

    /// No `before` atom of the operation admits the clicked atom's element, so
    /// the clicked atom cannot play any role in it.
    #[error("{op} does not act on {element}; its before pattern accepts {accepted}")]
    NoRole {
        op: String,
        /// The clicked atom's element symbol.
        element: String,
        /// The element symbols the `before` pattern accepts, comma-separated;
        /// `*` when a slot accepts anything.
        accepted: String,
    },

    /// The clicked atom's assigned role found no congruent assignment. The role
    /// is named because it is decided by rule and never revisited: a click on
    /// the wrong atom of an asymmetric operation has to explain itself rather
    /// than silently place the reaction somewhere else.
    #[error("cannot place {op} with the clicked {element} as before atom {role}: {nearest}")]
    NoPlacement {
        op: String,
        /// The `before` pattern id the clicked atom was given.
        role: i64,
        /// The clicked atom's element symbol.
        element: String,
        /// What *is* there, in the words [`describe_nearest`](super::describe_nearest)
        /// uses — or why no orientation could be derived.
        nearest: String,
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
            MechanosynthError::UnknownOp { library, .. } => Some(library),
            MechanosynthError::NoMatch { .. }
            | MechanosynthError::NoSuchAtom { .. }
            | MechanosynthError::NoRole { .. }
            | MechanosynthError::NoPlacement { .. } => None,
        }
    }
}
