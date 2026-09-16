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
///
/// `/3` is the **pattern-check** format: within a pattern the bond list is
/// closed-world (a listed bond must exist at that order, an unlisted pair must
/// not exist at all), a `before` atom may state its bond count as `deg`, an
/// operation may state how many of its leading `before` ids a user may click as
/// `anchors`, and a library may state its own steric factor as `clash`. A `/2`
/// file is refused with a message saying to regenerate it: the bond semantics
/// change the *meaning* of a pattern that lists no bonds, so silently accepting
/// one would read "these atoms are not bonded" where the author meant nothing
/// at all. Backward compatibility is deliberately not a goal — every library in
/// existence is machine-written by a generator that is regenerated with the
/// format. See `doc/design_mechanosynth_pattern_checks.md`.
pub const LIBRARY_FORMAT: &str = "atomcad-msops/3";

/// The `format` string a build script must carry.
///
/// `/2` drops the step's `method` field; the operation carries the kind now.
/// The loader skips a `method` key the way it skips any other unknown one.
pub const BUILD_FORMAT: &str = "atomcad-msbuild/2";

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

/// [`Operation::anchors`] when the file states none: id 1 alone.
///
/// This is the origin convention (`ORIGIN_PATTERN_ATOM_ID`) made **strict** —
/// id 1 is the atom at the origin, the atom to click, and the only atom a click
/// may play unless the library says otherwise. See
/// `doc/design_mechanosynth_pattern_checks.md` §3.4.
pub const DEFAULT_ANCHORS: i64 = 1;

/// The largest bond count a [`PatternAtom::deg`] may state. Nothing this engine
/// models has more; a bcc tungsten apex, the widest thing anyone has proposed,
/// has eight.
pub const MAX_PATTERN_DEGREE: u32 = 8;

/// Two atoms of one pattern closer than this multiple of their covalent-radius
/// sum, with no bond listed between them, are a **load-time warning**: either
/// the bond is missing from the file, or the library really means "these two are
/// not bonded", in which case the workpiece had better agree.
pub const CLOSE_PAIR_WARNING_FACTOR: f64 = 1.1;

/// Two atoms that are **not bonded to each other** must not be closer than this
/// multiple of their covalent-radius sum. The engine's default; a library states
/// its own with `clash`.
///
/// Argued from data rather than picked (`doc/design_mechanosynth_pattern_checks.md`
/// §5.3): the legitimate floor is a **donate-then-bridge** intermediate at 1.02
/// of a bond length — an atom placed at exactly the bond distance from a
/// neighbour it will bond to one step later — and the worst bogus offer that no
/// other check catches is an upside-down chemisorbed precursor at 0.86. Any
/// value in 0.87..=0.99 separates the two on both existing libraries; 0.9 keeps
/// the larger margin on the side where a false positive is an error on a
/// legitimate build.
pub const CLASH_BLOCK: f64 = 0.9;

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

/// How an operation is performed. A closed, **engine-defined** vocabulary: each
/// kind decides how a step is sequenced and what the animation milestone will
/// do with it, and a library cannot add a way of moving atoms without code.
///
/// What a library *does* say is which instrument performs a kind — the tool
/// type for [`Method::Tip`], the [`Operation::agent`] for [`Method::Bulk`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// A positional probe visits one site. Strictly sequential; carries a tool
    /// side, hence exactly one tool type.
    Tip,
    /// One site's share of an exposure of the whole workpiece — a gas, a dose,
    /// light. Named by [`Operation::agent`]; a maximal run of consecutive bulk
    /// steps with the same agent is one **event**.
    Bulk,
    /// The workpiece rearranges by itself. Belongs to the event of the
    /// preceding non-spontaneous step.
    Spontaneous,
}

impl Method {
    /// The file's spelling, which is also what the `step` record reports.
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Tip => "tip",
            Method::Bulk => "bulk",
            Method::Spontaneous => "spontaneous",
        }
    }

    /// The kind this spelling names, if it is one of the three.
    pub fn parse(text: &str) -> Option<Method> {
        match text {
            "tip" => Some(Method::Tip),
            "bulk" => Some(Method::Bulk),
            "spontaneous" => Some(Method::Spontaneous),
            _ => None,
        }
    }
}

/// The frame tag reserved for the origin of a tool type's local frame.
pub const APEX_FRAME_TAG: &str = "apex";

/// Below this volume (Å³) four frame entries are treated as coplanar, so the
/// fit onto them could not tell a molecule from its mirror image. Generous:
/// a real frame spans thousands of times this.
pub const FRAME_COPLANAR_EPSILON: f64 = 1e-3;

/// One named atom of a tool type's frame, in the tool's local coordinates.
///
/// No element and no `*`: the atoms are found by **tag**, never by pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameAtom {
    /// The atom tag a design puts on the atom that plays this role. Unique
    /// within the frame; [`APEX_FRAME_TAG`] names the one at the origin.
    pub tag: String,
    /// Position in the tool's local frame, Ångström.
    pub pos: DVec3,
}

/// One kind of tool the library's process uses.
///
/// The library *envisions* the tool types; a design *supplies* one molecule per
/// type, tagged with the type's name and with the frame's tags. Binding is then
/// a tag lookup and one rigid fit — no search, no candidate list, no ambiguity,
/// so a tip of thousands of atoms binds in the time it takes to look up four
/// tags.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolType {
    /// Unique among tool types. What an operation's tool side names, and the
    /// tag a design puts on the molecule that plays it.
    pub name: String,
    /// Free text for the panel and the offer list, like an operation's.
    pub note: Option<String>,
    /// The symbolic state vocabulary, when the type has one. **The first entry
    /// is the initial state**, the one every bound tool starts in: a tool type
    /// is researched with a resting state and the library is what knows it, so
    /// no per-instance configuration exists and the symbolic check applies from
    /// the first step. `None` means the type carries no symbolic state; an
    /// empty list is a parse error.
    pub states: Option<Vec<String>>,
    /// Four or more non-coplanar named atoms, one of them [`APEX_FRAME_TAG`] at
    /// the origin. Three non-collinear correspondences already fix a proper
    /// rotation; the fourth, off their plane, is what makes a molecule that is
    /// the **mirror image** of the library's fail the residual instead of
    /// binding mirrored — a triangle is congruent to its mirror image in space,
    /// a tetrahedron is not.
    ///
    /// The frame and every tool side of the type share this one local frame, so
    /// the frame atoms must be atoms that exist and stay put in every state the
    /// tool passes through — the handle, not the apex's cargo. The legs come
    /// off the **handle** rather than off the business axis, which is usually
    /// linear and would leave the four coplanar.
    pub frame: Vec<FrameAtom>,
}

impl ToolType {
    /// The state every bound tool of this type starts in: the first entry of
    /// `states`, or `None` for a type without symbolic state.
    pub fn initial_state(&self) -> Option<&str> {
        self.states
            .as_ref()
            .and_then(|states| states.first())
            .map(String::as_str)
    }

    /// Whether `state` is in this type's vocabulary.
    pub fn has_state(&self, state: &str) -> bool {
        self.states
            .as_ref()
            .is_some_and(|states| states.iter().any(|s| s == state))
    }

    /// The frame entry carrying this tag, if the frame has one.
    pub fn frame_atom(&self, tag: &str) -> Option<&FrameAtom> {
        self.frame.iter().find(|entry| entry.tag == tag)
    }
}

/// What a `tip` operation does to its tool, in the tool's own local frame.
///
/// The same primitive as the target side: a before/after rewrite matched by
/// nearest atom within tolerance, placed by a rigid transform — the tool's
/// pose, solved once at binding. So every property the target side has comes
/// with it, including exact placement and a failure naming the nearest atom.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSide {
    /// The [`ToolType::name`] this operation's instrument is.
    pub tool_type: String,
    /// The state the tool must be in; `None` accepts any.
    pub from: Option<String>,
    /// The state after the step; `None` leaves it unchanged.
    pub to: Option<String>,
    /// The rewrite of the tool. Both patterns may be empty, which is a tool
    /// side that changes nothing — a bare probe performing lithography names
    /// its type so the step records which instrument visited, without asserting
    /// any change. `from`/`to` still apply.
    pub before: Pattern,
    pub after: Pattern,
}

impl ToolSide {
    /// Whether tool-side pattern id `id` is a frame atom, by the same rule
    /// [`Operation::is_frame_atom`] applies to the target side.
    pub fn is_frame_atom(&self, id: i64) -> bool {
        is_frame_atom(&self.before, &self.after, id)
    }
}

/// The tool frame relative to the operation's target frame at the moment of
/// reaction. **Reserved for milestone 2**: parsed and kept, read by nothing, so
/// a generator can start writing it before an animation build reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Approach {
    pub r: DMat3,
    pub t: DVec3,
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
    /// *The matched workpiece atom has exactly this many bonds*, of any order,
    /// counting each bond once. Read on a `before` atom only.
    ///
    /// Absent is "don't care", so a library that does not trust its workpiece's
    /// bond model — an xyz import with no bond perception — may leave it out and
    /// lose only this check. A generator writes it **as drawn**: the number of
    /// bonds the atom had in the workpiece the pattern was derived from.
    ///
    /// It says what the closed-world bond rule cannot. That rule speaks only
    /// about pairs *inside* the pattern, so a bulk atom with three listed
    /// neighbours and one unlisted one passes every bond check; `deg: 3` is what
    /// rejects it. The two are complementary and both cost O(1).
    pub deg: Option<u32>,
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
    /// The library author's one-line description of the reaction, when the file
    /// states one. The editor shows it beside the operation in the palette and
    /// in the offer list — which is the whole reason it is kept: an operation
    /// name alone does not say what `si_donate_dimer` puts where. Nothing in
    /// the engine reads it.
    pub note: Option<String>,
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
    /// How the reaction is performed. Required in the file: it is a fact about
    /// the reaction, not a choice a build script makes, so it is stated once by
    /// whoever researched it and the step never types it.
    pub method: Method,
    /// The species or energy that performs a [`Method::Bulk`] step — `"Cl2"`,
    /// `"UV"`. Required on `bulk` and forbidden elsewhere; the engine reads it
    /// only for equality, to batch consecutive steps into one event.
    pub agent: Option<String>,
    /// What this operation does to its tool. `Some` exactly when
    /// [`Method::Tip`]: **one operation, one instrument** — the same reaction
    /// performed by two instruments is two operations.
    pub tool: Option<ToolSide>,
    /// The `before` atoms with ids `1..=anchors` are this operation's
    /// **anchors**: the atoms the reaction primarily acts on, and the only atoms
    /// a user may click to place it. [`DEFAULT_ANCHORS`] when the file states
    /// none.
    ///
    /// A count rather than a per-atom flag, because the library already has a
    /// numbering convention that puts the primary atom first and a count keeps
    /// *one* convention where a flag would add a second to keep aligned with it.
    ///
    /// For a **homonuclear** symmetric operation — `dimerize`, two silicons —
    /// stating `2` changes nothing: a click on either silicon already plays id 1
    /// and the search assigns the other to id 2. The field earns its keep on a
    /// **heteronuclear** pair of primary atoms, where a click on the second
    /// element is rejected by id 1 and admitted by id 2.
    pub anchors: i64,
    /// Reserved for milestone 2; see [`Approach`].
    pub approach: Option<Approach>,
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
        is_frame_atom(&self.before, &self.after, id)
    }

    /// Whether either pattern names a bond at `id` that the other does not, or
    /// names it with a different order.
    pub fn has_bond_change_at(&self, id: i64) -> bool {
        has_bond_change_at(&self.before, &self.after, id)
    }

    /// Whether `before` pattern id `id` is an **anchor**: an atom a click may
    /// play. See [`Operation::anchors`].
    pub fn is_anchor(&self, id: i64) -> bool {
        (ORIGIN_PATTERN_ATOM_ID..=self.anchors).contains(&id)
    }

    /// The `before` atoms a click may play, in file order.
    pub fn anchor_atoms(&self) -> impl Iterator<Item = &PatternAtom> {
        self.before
            .atoms
            .iter()
            .filter(|atom| self.is_anchor(atom.id))
    }
}

/// [`Operation::is_frame_atom`] over a bare pattern pair, so the tool side of
/// an operation is judged by exactly the same rule as its target side.
pub fn is_frame_atom(before: &Pattern, after: &Pattern, id: i64) -> bool {
    let (Some(before_atom), Some(after_atom)) = (before.atom(id), after.atom(id)) else {
        return false;
    };
    if before_atom.pos.distance(after_atom.pos) > PATTERN_POSITION_EPSILON {
        return false;
    }
    if after_atom.element != PatternElement::Any && after_atom.element != before_atom.element {
        return false;
    }
    !has_bond_change_at(before, after, id)
}

/// [`Operation::has_bond_change_at`] over a bare pattern pair.
pub fn has_bond_change_at(before: &Pattern, after: &Pattern, id: i64) -> bool {
    let order_in = |pattern: &Pattern, key: (i64, i64)| {
        pattern
            .bonds
            .iter()
            .find(|bond| bond.key() == key)
            .map(|bond| bond.order)
    };
    before
        .bonds
        .iter()
        .map(|bond| bond.key())
        .chain(after.bonds.iter().map(|bond| bond.key()))
        .filter(|key| key.0 == id || key.1 == id)
        .any(|key| order_in(before, key) != order_in(after, key))
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
    /// The steric factor for every check against this library;
    /// [`CLASH_BLOCK`] when the file states none. One value per library, in the
    /// same spirit as `tolerance`: stated by whoever computed the patterns.
    pub clash: Option<f64>,
    /// Every tool type the library's process uses, in file order. Empty for a
    /// library with no `tip` operation.
    pub tools: Vec<ToolType>,
    /// `name` → index into `ops`. Built at parse time; names are unique.
    index: FxHashMap<String, usize>,
    /// `name` → index into `tools`. Built at parse time; names are unique.
    tool_index: FxHashMap<String, usize>,
}

impl OpLibrary {
    /// Builds the name index. `ops` must already have unique names (the parser
    /// checks that before calling this).
    pub(super) fn new(
        file: String,
        tolerance: Option<f64>,
        clash: Option<f64>,
        ops: Vec<Operation>,
        tools: Vec<ToolType>,
        warnings: Vec<String>,
    ) -> Self {
        let index = ops
            .iter()
            .enumerate()
            .map(|(i, op)| (op.name.clone(), i))
            .collect();
        let tool_index = tools
            .iter()
            .enumerate()
            .map(|(i, tool)| (tool.name.clone(), i))
            .collect();
        Self {
            file,
            tolerance,
            clash,
            ops,
            warnings,
            tools,
            index,
            tool_index,
        }
    }

    /// The steric blocking factor in force for this library: its own `clash`,
    /// else [`CLASH_BLOCK`].
    pub fn clash_factor(&self) -> f64 {
        self.clash.unwrap_or(CLASH_BLOCK)
    }

    /// The operation with this name, if the library has one.
    pub fn get(&self, name: &str) -> Option<&Operation> {
        self.index.get(name).map(|&i| &self.ops[i])
    }

    /// The tool type with this name, if the library declares one.
    pub fn tool_type(&self, name: &str) -> Option<&ToolType> {
        self.tool_index.get(name).map(|&i| &self.tools[i])
    }

    /// The tool type names, comma-separated — what a binding error lists when
    /// a wired molecule carries none of them.
    pub fn tool_type_names(&self) -> String {
        if self.tools.is_empty() {
            return "none".to_string();
        }
        self.tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// One application of an operation at a rigid transform.
///
/// `p_workpiece = r · p_local + t`.
///
/// **Seven fields, and a test pins that.** The step used to carry a `method`
/// string as well; the kind is a fact about the reaction, so it moved to
/// [`Operation::method`] and a `method` key in a file is now an unknown key the
/// loader skips. See `doc/design_mechanosynth_tools.md`.
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

/// A `before` atom of a step that found nothing where it looked.
///
/// Its own type so that [`MechanosynthError`] can box it; see the variant.
#[derive(Debug, Error)]
#[error(
    "step {step} ({op} @ ({t_x:.3}, {t_y:.3}, {t_z:.3})) — before atom id {atom_id} ({element}) \
     not found within {tolerance:.2} Å; {nearest}{}", in_participant(.participant)
)]
pub struct NoMatch {
    /// 1-based, as everything user-facing about steps is.
    pub step: usize,
    pub op: String,
    pub t_x: f64,
    pub t_y: f64,
    pub t_z: f64,
    pub atom_id: i64,
    /// The pattern's element symbol, or `*`.
    pub element: String,
    pub tolerance: f64,
    /// `nearest atom is H at 0.91 Å`, or a note that there is none.
    pub nearest: String,
    /// Which participant of the scene the nearest atom belongs to — `base`,
    /// `feedstock 1`, `tool 0 (habst_tool)` — so the sentence says where it
    /// looked. Empty when there is no scene to say it about (a bare
    /// [`super::apply_step`] call) or no nearest atom at all.
    pub participant: String,
}

/// A pattern bond that the workpiece does not agree with.
///
/// Its own type so that [`MechanosynthError`] can box it, for the reason
/// [`NoMatch`] is boxed: every fallible function here returns that enum.
#[derive(Debug, Error)]
#[error(
    "step {step} ({op} @ ({t_x:.3}, {t_y:.3}, {t_z:.3})) — pattern atoms {a} and {b} \
     {}{}",
    describe_bond_mismatch(*.expected, *.found),
    in_participant(.participant)
)]
pub struct BondMismatch {
    /// 1-based, as everything user-facing about steps is.
    pub step: usize,
    pub op: String,
    pub t_x: f64,
    pub t_y: f64,
    pub t_z: f64,
    /// The two pattern ids, in the pattern's own numbering.
    pub a: i64,
    pub b: i64,
    /// The order the pattern states, or `None` for a pair the pattern leaves
    /// unlisted — which, the bond list being closed-world, asserts "no bond".
    pub expected: Option<u8>,
    /// The order the workpiece has, or `None` for no bond at all.
    pub found: Option<u8>,
    /// Which participant of the scene the matched atoms belong to. Empty when
    /// there is no scene to say it about.
    pub participant: String,
}

/// A `before` atom whose matched workpiece atom has the wrong number of bonds.
#[derive(Debug, Error)]
#[error(
    "step {step} ({op} @ ({t_x:.3}, {t_y:.3}, {t_z:.3})) — before atom id {atom_id} \
     needs {expected} bond(s), the matched {element} has {found}{}",
    in_participant(.participant)
)]
pub struct DegreeMismatch {
    /// 1-based, as everything user-facing about steps is.
    pub step: usize,
    pub op: String,
    pub t_x: f64,
    pub t_y: f64,
    pub t_z: f64,
    pub atom_id: i64,
    /// The matched workpiece atom's element symbol.
    pub element: String,
    pub expected: u32,
    pub found: usize,
    /// Which participant of the scene the matched atom belongs to. Empty when
    /// there is no scene to say it about.
    pub participant: String,
}

/// The middle of a [`BondMismatch`] message: what the pattern said against what
/// the workpiece has.
fn describe_bond_mismatch(expected: Option<u8>, found: Option<u8>) -> String {
    match (expected, found) {
        (Some(order), None) => format!("should carry a bond of order {order}, and carry none"),
        (None, Some(order)) => {
            format!("should carry no bond, and carry one of order {order}")
        }
        (Some(want), Some(got)) => {
            format!("should carry a bond of order {want}, and carry one of order {got}")
        }
        // Not reachable: a pair only mismatches when the two differ.
        (None, None) => "disagree".to_string(),
    }
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

    /// No **anchor** of the operation admits the clicked atom's element, so the
    /// clicked atom is not one the operation acts on.
    ///
    /// The anchors, not the whole `before` pattern: a frame atom is by
    /// definition one the operation does not touch, and a click is a statement
    /// about where the reaction should happen. See
    /// [`Operation::anchors`] and `doc/design_mechanosynth_pattern_checks.md`
    /// §4.3.
    #[error("{op} acts on {accepted} (its anchor); the clicked atom is {element}")]
    NoRole {
        op: String,
        /// The clicked atom's element symbol.
        element: String,
        /// The element symbols the operation's **anchors** accept,
        /// comma-separated; `*` when an anchor accepts anything.
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
    ///
    /// **Boxed.** It is by far the widest thing that can go wrong here — five
    /// strings and the whole transform — and every fallible function in this
    /// module returns this enum, so an unboxed payload would put a hundred and
    /// fifty bytes of `Err` on the stack of every match, every parse and every
    /// replay (`clippy::result_large_err`).
    #[error(transparent)]
    NoMatch(Box<NoMatch>),

    /// A pattern bond the workpiece does not agree with — missing, present
    /// where the pattern lists none, or of a different order.
    ///
    /// **Boxed**, like [`MechanosynthError::NoMatch`] and for the same reason.
    #[error(transparent)]
    BondMismatch(Box<BondMismatch>),

    /// A `before` atom's [`deg`](PatternAtom::deg) against what the matched
    /// workpiece atom actually has.
    ///
    /// **Boxed**, like [`MechanosynthError::NoMatch`] and for the same reason.
    #[error(transparent)]
    DegreeMismatch(Box<DegreeMismatch>),

    /// Merging a feedstock or a tool into the scene would overflow the
    /// structure's thirty-two tag names.
    ///
    /// A real error, unlike the cosmetic highlight that `paint` drops silently:
    /// a tool whose type tag cannot be interned cannot bind.
    #[error(
        "{molecule}: its tags do not fit — the scene already carries {existing} tag names and \
         the limit is {limit}; drop tags the replay does not need"
    )]
    SceneTags {
        /// `feedstock 1`, `tool 0`.
        molecule: String,
        existing: usize,
        limit: usize,
    },

    /// A wired tool molecule carries no tool type's name as an atom tag.
    #[error("{molecule}: no atom carries a tool type tag; the library declares {types}")]
    ToolUntagged { molecule: String, types: String },

    /// A wired tool molecule carries two tool type tags, so which type it plays
    /// is a guess — and a guess is never what a tagged design meant.
    #[error("{molecule}: carries two tool type tags, '{first}' and '{second}'")]
    ToolMultiType {
        molecule: String,
        first: String,
        second: String,
    },

    /// Two wired molecules claim the same tool type.
    #[error("{first} and {second} both carry the tool type tag '{tool_type}'")]
    ToolDuplicate {
        tool_type: String,
        first: String,
        second: String,
    },

    /// A frame tag of the bound type is on no atom of the molecule, or on two.
    #[error(
        "{molecule}: tool type '{tool_type}' expects exactly one atom tagged '{tag}', found {found}"
    )]
    ToolFrameTag {
        molecule: String,
        tool_type: String,
        tag: String,
        found: usize,
    },

    /// The tagged atoms do not have the geometry the library's frame states, so
    /// the molecule is not the tool the library was computed for — or it is its
    /// mirror image, which no proper rotation superimposes.
    #[error("{molecule}: does not fit the frame of tool type '{tool_type}': {detail}")]
    ToolPoseResidual {
        molecule: String,
        tool_type: String,
        /// Max per-atom distance of the fit, Å. `f64::INFINITY` when no fit
        /// exists at all.
        residual: f64,
        /// Why, in words: the residual against the tolerance, or the degeneracy
        /// that left the orientation undetermined.
        detail: String,
    },

    /// A `tip` step's operation names a tool type no wired molecule plays.
    #[error("step {step} ({op}): no molecule on the tools pin is tagged '{tool_type}'")]
    ToolMissing {
        step: usize,
        op: String,
        tool_type: String,
    },

    /// The tool is not in the state the operation requires.
    #[error("step {step} ({op}): {tool_type} is {found}, but the operation requires {required}")]
    ToolState {
        step: usize,
        op: String,
        tool_type: String,
        found: String,
        required: String,
    },

    /// A step's target side matched inside a tool molecule. Tools are rewritten
    /// by their operations' tool sides, never placed on.
    #[error("step {step} ({op}): the operation matched on {tool}, which is a tool")]
    StepOnTool {
        step: usize,
        op: String,
        tool: String,
    },

    /// A step's target side matched atoms of two different structures — the
    /// workpiece and a reservoir, or two reservoirs — so which one the step
    /// acts on, and which one its added atoms join, has no answer.
    #[error("step {step} ({op}): the operation matched across {first} and {second}")]
    StepAcrossParticipants {
        step: usize,
        op: String,
        first: String,
        second: String,
    },

    /// A tool side matched an atom that is not the tool's. The tool-side match
    /// ranges over the whole scene, so a tool parked in contact with the
    /// workpiece can have a base atom inside tolerance of a pattern position;
    /// the mirror of [`MechanosynthError::StepOnTool`].
    #[error("step {step} ({op}): the tool side of {tool} matched {found}, which is not the tool")]
    ToolSideOffTool {
        step: usize,
        op: String,
        tool: String,
        found: String,
    },
}

/// The ` (in tool 0 (habst_tool))` tail of a match failure, or nothing when the
/// caller had no scene to name a participant from.
fn in_participant(participant: &str) -> String {
    if participant.is_empty() {
        String::new()
    } else {
        format!(" (in {participant})")
    }
}

impl MechanosynthError {
    /// The file label this error is about.
    pub fn file(&self) -> Option<&str> {
        match self {
            MechanosynthError::Io { file, .. }
            | MechanosynthError::Json { file, .. }
            | MechanosynthError::Invalid { file, .. } => Some(file),
            MechanosynthError::UnknownOp { library, .. } => Some(library),
            MechanosynthError::NoMatch(_)
            | MechanosynthError::NoSuchAtom { .. }
            | MechanosynthError::NoRole { .. }
            | MechanosynthError::NoPlacement { .. }
            | MechanosynthError::SceneTags { .. }
            | MechanosynthError::ToolUntagged { .. }
            | MechanosynthError::ToolMultiType { .. }
            | MechanosynthError::ToolDuplicate { .. }
            | MechanosynthError::ToolFrameTag { .. }
            | MechanosynthError::ToolPoseResidual { .. }
            | MechanosynthError::ToolMissing { .. }
            | MechanosynthError::ToolState { .. }
            | MechanosynthError::StepOnTool { .. }
            | MechanosynthError::StepAcrossParticipants { .. }
            | MechanosynthError::BondMismatch(_)
            | MechanosynthError::DegreeMismatch(_)
            | MechanosynthError::ToolSideOffTool { .. } => None,
        }
    }
}
