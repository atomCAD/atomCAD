use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;

use crate::structure_designer::node_type_registry::NodeTypeRegistry;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct FunctionType {
    pub parameter_types: Vec<DataType>,
    pub output_type: Box<DataType>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub enum DataType {
    None,
    Bool,
    String,
    Int,
    Float,
    Vec2,
    Vec3,
    IVec2,
    IVec3,
    IMat3,
    Mat3,
    LatticeVecs,
    DrawingPlane,
    Geometry2D,
    Blueprint,
    HasAtoms,
    Crystal,
    Molecule,
    HasStructure,
    HasFreeLinOps,
    Motif,
    Structure,
    /// The type with exactly one value — return type of effect nodes
    /// (`export_xyz`, `foreach`, …). A universal `T → Unit` widening is
    /// added at field-level so any sub-network output can be consumed by an
    /// effect-typed pin. Reverse `Unit → T` is forbidden. See
    /// `doc/design_node_execution.md`.
    Unit,
    Array(Box<DataType>),
    /// Lazy stream of `T`. Wire-time conversions allow `[T] → Iter[T]` and
    /// `T → Iter[T]` (eager wraps); `Iter[T] → Iter[T]` identity. There is
    /// **no** implicit `Iter[T] → [T]` rule — use a `collect` node. See
    /// `doc/design_iterators.md`.
    Iterator(Box<DataType>),
    Function(FunctionType),
    Record(RecordType),
}

/// A record type — either a reference to a named def in the registry, or an
/// inline anonymous schema (e.g. produced by an `expr` literal). See
/// `doc/design_record_types.md`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub enum RecordType {
    /// References a registered record type def by name. The schema is resolved
    /// via `NodeTypeRegistry::record_type_defs` at use time. A reference whose
    /// name is missing from the registry is *dangling* and is treated as a
    /// type error wherever it appears.
    Named(String),

    /// Inline anonymous record. Fields are stored in **canonical (sorted-by-name)
    /// order**; field names are distinct. The empty record `{}` is
    /// `Anonymous(vec![])`. Construct via `RecordType::anonymous` to enforce
    /// the invariant.
    Anonymous(Vec<(String, DataType)>),
}

/// Walks `t` and applies `f` to every `RecordType::Named(_)` name string
/// reachable through `Array`, `Function`, and nested `Record::Anonymous`
/// shapes. Used by the rename-record-type-def pass to rewrite `Named(old)` in
/// place. Anonymous record fields recurse so a rename inside `Box.fields[0]`
/// updates a nested `{ p: Foo }` literal too.
pub fn walk_data_type_record_names_mut<F>(t: &mut DataType, f: &mut F)
where
    F: FnMut(&mut String),
{
    match t {
        DataType::Array(inner) => walk_data_type_record_names_mut(inner, f),
        DataType::Iterator(inner) => walk_data_type_record_names_mut(inner, f),
        DataType::Function(func) => {
            for p in &mut func.parameter_types {
                walk_data_type_record_names_mut(p, f);
            }
            walk_data_type_record_names_mut(&mut func.output_type, f);
        }
        DataType::Record(RecordType::Named(name)) => f(name),
        DataType::Record(RecordType::Anonymous(fields)) => {
            for (_, ty) in fields {
                walk_data_type_record_names_mut(ty, f);
            }
        }
        _ => {}
    }
}

impl RecordType {
    pub fn named(name: String) -> Self {
        RecordType::Named(name)
    }

    /// Construct an anonymous record from an arbitrary field list. Fields are
    /// sorted ascending by name to satisfy the canonical-order invariant. The
    /// caller is responsible for ensuring field names are distinct; duplicates
    /// are kept in their relative input order (a defensive caller should
    /// validate).
    pub fn anonymous(mut fields: Vec<(String, DataType)>) -> Self {
        fields.sort_by(|(a, _), (b, _)| a.cmp(b));
        RecordType::Anonymous(fields)
    }

    /// Resolve to the canonical field schema. For `Named`, looks up the def in
    /// the registry (user defs first, then built-in defs) and returns its
    /// fields in canonical (sorted) order; returns `None` if the name is
    /// dangling. For `Anonymous`, returns the inline fields (already
    /// canonical).
    pub fn resolve_fields<'a>(
        &'a self,
        registry: &'a NodeTypeRegistry,
    ) -> Option<Cow<'a, [(String, DataType)]>> {
        match self {
            RecordType::Anonymous(fs) => Some(Cow::Borrowed(fs.as_slice())),
            RecordType::Named(n) => registry.lookup_record_type_def(n).map(|def| {
                let mut canonical = def.fields.clone();
                canonical.sort_by(|(a, _), (b, _)| a.cmp(b));
                Cow::Owned(canonical)
            }),
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::None => write!(f, "None"),
            DataType::Bool => write!(f, "Bool"),
            DataType::String => write!(f, "String"),
            DataType::Int => write!(f, "Int"),
            DataType::Float => write!(f, "Float"),
            DataType::Vec2 => write!(f, "Vec2"),
            DataType::Vec3 => write!(f, "Vec3"),
            DataType::IVec2 => write!(f, "IVec2"),
            DataType::IVec3 => write!(f, "IVec3"),
            DataType::IMat3 => write!(f, "IMat3"),
            DataType::Mat3 => write!(f, "Mat3"),
            DataType::LatticeVecs => write!(f, "LatticeVecs"),
            DataType::DrawingPlane => write!(f, "DrawingPlane"),
            DataType::Geometry2D => write!(f, "Geometry2D"),
            DataType::Blueprint => write!(f, "Blueprint"),
            DataType::HasAtoms => write!(f, "HasAtoms"),
            DataType::Crystal => write!(f, "Crystal"),
            DataType::Molecule => write!(f, "Molecule"),
            DataType::HasStructure => write!(f, "HasStructure"),
            DataType::HasFreeLinOps => write!(f, "HasFreeLinOps"),
            DataType::Motif => write!(f, "Motif"),
            DataType::Structure => write!(f, "Structure"),
            DataType::Unit => write!(f, "Unit"),
            DataType::Array(element_type) => {
                write!(f, "[{}]", element_type)
            }
            DataType::Iterator(element_type) => {
                write!(f, "Iter[{}]", element_type)
            }
            DataType::Function(func_type) => {
                if func_type.parameter_types.is_empty() {
                    write!(f, "() -> {}", func_type.output_type)
                } else if func_type.parameter_types.len() == 1 {
                    write!(
                        f,
                        "{} -> {}",
                        func_type.parameter_types[0], func_type.output_type
                    )
                } else {
                    let params = func_type
                        .parameter_types
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    write!(f, "({}) -> {}", params, func_type.output_type)
                }
            }
            DataType::Record(record_type) => match record_type {
                // Named records are emitted as `Record(Name)` so the string
                // round-trips through `DataType::from_string` without colliding
                // with built-in type names or with bare-identifier node
                // references in the text-format parser.
                RecordType::Named(name) => write!(f, "Record({})", name),
                RecordType::Anonymous(fields) => {
                    write!(f, "{{")?;
                    for (i, (name, ty)) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}: {}", name, ty)?;
                    }
                    write!(f, "}}")
                }
            },
        }
    }
}

impl DataType {
    pub fn is_array(&self) -> bool {
        matches!(self, DataType::Array(_))
    }

    /// Returns true for abstract phase supertypes (HasAtoms, HasStructure, HasFreeLinOps).
    /// Abstract types appear only as declared input-pin types on built-in polymorphic
    /// nodes; no `NetworkResult` value ever carries an abstract `DataType`.
    pub fn is_abstract(&self) -> bool {
        matches!(
            self,
            DataType::HasAtoms | DataType::HasStructure | DataType::HasFreeLinOps
        )
    }

    /// For drag-from-output: extract the "element type" from a value-producing pin.
    /// Used by adapters that want to set a stored `element_type` to match the source.
    /// Peels `Iter[T]` / `Array[T]` to `T`; otherwise treats the source as a
    /// single-element broadcast (`T` → `T`). Rejects abstract types and
    /// `Function(_)` (neither makes sense as an element).
    pub fn drag_element_type_from_output(&self) -> Option<DataType> {
        match self {
            DataType::Iterator(t) | DataType::Array(t) => Some((**t).clone()),
            DataType::Function(_) => None,
            t if t.is_abstract() => None,
            t => Some(t.clone()),
        }
    }

    /// For drag-from-input: same extraction, but rejecting scalar broadcast where the
    /// adapter's downstream connection wouldn't make sense (e.g. `collect`).
    pub fn drag_element_type_from_input_strict(&self) -> Option<DataType> {
        match self {
            DataType::Iterator(t) | DataType::Array(t) => Some((**t).clone()),
            _ => None,
        }
    }

    /// Checks if a source data type can be converted to a destination data type.
    ///
    /// `&NodeTypeRegistry` is threaded through so `RecordType::Named` references
    /// can be resolved against `record_type_defs`. Records are subtyped
    /// structurally (width + depth) using canonical-order linear merge; field
    /// positions accept only **tag-only widenings** (see
    /// `is_tag_only_widening`), never value-converting widenings such as
    /// `Int → Float`. See `doc/design_record_types.md` Phase 4.
    ///
    /// # Parameters
    /// * `source_type` - The source data type
    /// * `dest_type` - The destination data type
    /// * `registry` - The node type registry (used for record name resolution)
    ///
    /// # Returns
    /// True if the source type can be converted to the destination type
    pub fn can_be_converted_to(
        source_type: &DataType,
        dest_type: &DataType,
        registry: &NodeTypeRegistry,
    ) -> bool {
        // Same types are always compatible
        if source_type == dest_type {
            return true;
        }

        // Universal `T → Unit` widening (the "discard" rule). Any source type —
        // including iterators, functions, records, and Unit itself — coerces to
        // `Unit`. The reverse `Unit → T` is rejected by falling through to the
        // pair-table at the bottom (no Unit arm is added there).
        // See `doc/design_node_execution.md` ("The Unit type").
        if matches!(dest_type, DataType::Unit) {
            return true;
        }

        // Records: full width + structural depth subtyping. Two `Named(n)`
        // references resolve to the same def, hence the same fields, by
        // definition — short-circuit to avoid a registry lookup. Otherwise
        // resolve both sides to canonical-ordered field lists and walk the
        // destination forward, advancing the source by linear merge. Each
        // matched field is checked under `can_be_structurally_converted_to`,
        // which permits only tag-only widenings at leaf positions (no Int→Float
        // and friends) — necessary for pass-through coercion to be sound (see
        // `doc/design_record_types.md`, Subtyping section). A dangling
        // `Named(_)` reference (missing from the registry) is incompatible
        // with anything.
        if let (DataType::Record(src), DataType::Record(dst)) = (source_type, dest_type) {
            if let (RecordType::Named(s), RecordType::Named(d)) = (src, dst) {
                if s == d {
                    return true;
                }
            }
            let Some(src_fields) = src.resolve_fields(registry) else {
                return false;
            };
            let Some(dst_fields) = dst.resolve_fields(registry) else {
                return false;
            };
            let mut si = 0usize;
            for (dst_field, dst_ty) in dst_fields.iter() {
                while si < src_fields.len() && src_fields[si].0.as_str() < dst_field.as_str() {
                    si += 1;
                }
                if si == src_fields.len() || src_fields[si].0 != *dst_field {
                    return false;
                }
                if !can_be_structurally_converted_to(&src_fields[si].1, dst_ty, registry) {
                    return false;
                }
                si += 1;
            }
            return true;
        }

        // Iterator destination rules (see `doc/design_iterators.md`).
        //
        // Three rules apply, in this order:
        //   1. `[S] → Iter[T]` when `S → T` (eager wrap; element conversion at
        //      wrap time).
        //   2. `Iter[T] → Iter[T]` identity only — `Iter[S] → Iter[T]` with
        //      `S ≠ T` is **disallowed in v1** (lazy element conversion is a
        //      follow-up). The identity case is already handled by the
        //      `source == dest` short-circuit at the top of this function;
        //      reaching here with two `Iterator(_)` types means inner types
        //      differ, so we explicitly reject.
        //   3. `S → Iter[T]` (single-element broadcast) when `S → T`.
        //
        // There is **no** `Iter[T] → [T]` and **no** `Iter[T] → T` rule. Both
        // would force iterator consumption inside a wire-time conversion;
        // users wire an explicit `collect` node instead.
        if let DataType::Iterator(target_element_type) = dest_type {
            // Rule 1: array source, iterator destination → eager wrap.
            if let DataType::Array(source_element_type) = source_type {
                return DataType::can_be_converted_to(
                    source_element_type,
                    target_element_type,
                    registry,
                );
            }
            // Rule 2 (negative): different iterator element types are not
            // implicitly convertible.
            if matches!(source_type, DataType::Iterator(_)) {
                return false;
            }
            // Rule 3: scalar broadcast.
            return DataType::can_be_converted_to(source_type, target_element_type, registry);
        }

        // No `Iter[T] → [T]` and no `Iter[T] → T`. Reject any conversion whose
        // source is an iterator and destination is anything but `Iter[T]`
        // (handled above) — including arrays, scalars, records, and functions.
        if matches!(source_type, DataType::Iterator(_)) {
            return false;
        }

        // Check if we can convert T to [T] (single element to array)
        if let DataType::Array(target_element_type) = dest_type {
            if DataType::can_be_converted_to(source_type, target_element_type, registry) {
                return true;
            }
        }

        // Array-to-array element-wise conversion: [S] -> [T] when S -> T.
        // Mirrors the runtime conversion in `NetworkResult::convert_to`. Without
        // this, e.g. [Molecule] cannot flow into a [HasAtoms] input even though
        // Molecule -> HasAtoms is a permitted concrete -> abstract upcast.
        if let (DataType::Array(source_element_type), DataType::Array(target_element_type)) =
            (source_type, dest_type)
        {
            if DataType::can_be_converted_to(source_element_type, target_element_type, registry) {
                return true;
            }
        }

        // Check function type conversions for partial evaluation
        // Function F can be converted to function G if:
        // 1. F and G have the same return type
        // 2. F contains all parameters of G as its first parameters
        // 3. F may have additional parameters after G's parameters
        if let (DataType::Function(source_func), DataType::Function(dest_func)) =
            (source_type, dest_type)
        {
            // Check if return types are compatible
            if !DataType::can_be_converted_to(
                &source_func.output_type,
                &dest_func.output_type,
                registry,
            ) {
                return false;
            }

            // Check if source function has at least as many parameters as destination
            if source_func.parameter_types.len() < dest_func.parameter_types.len() {
                return false;
            }

            // Check if the first N parameters of source match destination parameters
            // where N is the number of parameters in destination function
            for (i, dest_param) in dest_func.parameter_types.iter().enumerate() {
                if !DataType::can_be_converted_to(
                    &source_func.parameter_types[i],
                    dest_param,
                    registry,
                ) {
                    return false;
                }
            }

            // If we get here, F can be converted to G by partial evaluation
            return true;
        }

        // Define conversion rules
        match (source_type, dest_type) {
            // Int <-> Float conversions
            (DataType::Int, DataType::Float) => true,
            (DataType::Float, DataType::Int) => true,

            // IVec2 <-> Vec2 conversions
            (DataType::IVec2, DataType::Vec2) => true,
            (DataType::Vec2, DataType::IVec2) => true,

            // IVec3 <-> Vec3 conversions
            (DataType::IVec3, DataType::Vec3) => true,
            (DataType::Vec3, DataType::IVec3) => true,

            // IMat3 <-> Mat3 conversions (truncating downcast — see design_matrix_types.md D3)
            (DataType::IMat3, DataType::Mat3) => true,
            (DataType::Mat3, DataType::IMat3) => true,

            // LatticeVecs -> DrawingPlane conversion (backward compatibility for old .cnnd files)
            (DataType::LatticeVecs, DataType::DrawingPlane) => true,

            // Concrete phase types upcast to the abstract supertypes that contain them
            // (no abstract -> concrete, no cross-abstract). Funneled through
            // `is_tag_only_widening` so the same predicate is reused at record
            // leaf positions in `can_be_structurally_converted_to`.
            _ => is_tag_only_widening(source_type, dest_type),
        }
    }

    /// Like `can_be_converted_to`, but recursively rejects the two
    /// scalar-to-collection broadcast rules (`S → Array[T]` and `S → Iter[T]`
    /// where `S` is not itself an array/iterator). Used at the drag-aware
    /// add-node popup's Stage-2 adapter-verification site (and the mirror
    /// site in `StructureDesigner::add_node_with_drag_source`).
    ///
    /// Rationale: an adapter that only matches the drag source via scalar
    /// broadcast is offering to "wrap your one value in a singleton
    /// collection," which is almost never user intent. Stage-1 static
    /// matches stay permissive (the node author declared the collection
    /// pin); only adapter-shapeshifted matches get the strict treatment.
    /// See `doc/design_drag_aware_add_node.md` §"Asymmetric verification".
    ///
    /// Keeps: identity, discard-to-`Unit`, record subtyping (field path is
    /// already strict via `can_be_structurally_converted_to`),
    /// `Array[S] → Iter[T]` eager wrap, `Array[S] → Array[T]` element-wise,
    /// function partial-application, `Int↔Float`/`IVec*↔Vec*`/`IMat3↔Mat3`,
    /// `LatticeVecs→DrawingPlane`, and tag-only phase upcasts. All recursive
    /// descents call this strict variant, not `can_be_converted_to`, so
    /// broadcast cannot leak in through a nested element type.
    pub fn can_be_converted_to_strict_no_broadcast(
        source_type: &DataType,
        dest_type: &DataType,
        registry: &NodeTypeRegistry,
    ) -> bool {
        // Identity.
        if source_type == dest_type {
            return true;
        }

        // Universal `T → Unit` discard widening.
        if matches!(dest_type, DataType::Unit) {
            return true;
        }

        // Records: identical structural subtyping to the permissive arm.
        // Field-level checks go through `can_be_structurally_converted_to`,
        // which is already strictly tag-only (no broadcast, no value
        // conversions) — safe to delegate.
        if matches!(
            (source_type, dest_type),
            (DataType::Record(_), DataType::Record(_))
        ) {
            return DataType::can_be_converted_to(source_type, dest_type, registry);
        }

        // Iterator destination: only `Array[S] → Iter[T]` eager wrap and
        // `Iter[T] → Iter[T]` identity (already handled above). The scalar
        // broadcast rule `S → Iter[T]` is dropped.
        if let DataType::Iterator(target_element_type) = dest_type {
            if let DataType::Array(source_element_type) = source_type {
                return DataType::can_be_converted_to_strict_no_broadcast(
                    source_element_type,
                    target_element_type,
                    registry,
                );
            }
            // Iter[S] → Iter[T] with S != T rejected (same as permissive).
            // Scalar broadcast rejected.
            return false;
        }

        // Iterator source against a non-iterator destination is rejected
        // (same as permissive).
        if matches!(source_type, DataType::Iterator(_)) {
            return false;
        }

        // Array destination: only element-wise `Array[S] → Array[T]`.
        // Scalar broadcast `S → Array[T]` is dropped.
        if let DataType::Array(target_element_type) = dest_type {
            if let DataType::Array(source_element_type) = source_type {
                return DataType::can_be_converted_to_strict_no_broadcast(
                    source_element_type,
                    target_element_type,
                    registry,
                );
            }
            return false;
        }

        // Function partial-application: same shape as permissive but
        // recurses strictly, so broadcast can't sneak in via parameter or
        // return types.
        if let (DataType::Function(source_func), DataType::Function(dest_func)) =
            (source_type, dest_type)
        {
            if !DataType::can_be_converted_to_strict_no_broadcast(
                &source_func.output_type,
                &dest_func.output_type,
                registry,
            ) {
                return false;
            }
            if source_func.parameter_types.len() < dest_func.parameter_types.len() {
                return false;
            }
            for (i, dest_param) in dest_func.parameter_types.iter().enumerate() {
                if !DataType::can_be_converted_to_strict_no_broadcast(
                    &source_func.parameter_types[i],
                    dest_param,
                    registry,
                ) {
                    return false;
                }
            }
            return true;
        }

        // Value-converting widenings stay (they're not broadcast).
        match (source_type, dest_type) {
            (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int) => true,
            (DataType::IVec2, DataType::Vec2) | (DataType::Vec2, DataType::IVec2) => true,
            (DataType::IVec3, DataType::Vec3) | (DataType::Vec3, DataType::IVec3) => true,
            (DataType::IMat3, DataType::Mat3) | (DataType::Mat3, DataType::IMat3) => true,
            (DataType::LatticeVecs, DataType::DrawingPlane) => true,
            _ => is_tag_only_widening(source_type, dest_type),
        }
    }
}

/// True when `t` is `Iter[..]` itself, or contains an `Iter[..]` reachable
/// through `Array`, `Function` (parameter or return), or nested
/// `Record::Anonymous` shapes. `Record::Named(_)` is not followed: a named
/// record def's fields are walked through its registry entry by callers that
/// have a registry handle. See `doc/design_iterators.md` ("Iterator values
/// cannot be captured into closures").
pub fn contains_iterator(t: &DataType) -> bool {
    match t {
        DataType::Iterator(_) => true,
        DataType::Array(inner) => contains_iterator(inner),
        DataType::Function(func) => {
            func.parameter_types.iter().any(contains_iterator)
                || contains_iterator(&func.output_type)
        }
        DataType::Record(RecordType::Anonymous(fields)) => {
            fields.iter().any(|(_, ty)| contains_iterator(ty))
        }
        _ => false,
    }
}

/// True when `src` widens to `dst` without any runtime value conversion.
/// Today: identity, plus concrete phase types upcasting to their abstract
/// supertypes (`Crystal → HasAtoms`, …) — these are pure tag-level widenings
/// where the runtime variant doesn't change. Distinct from
/// `DataType::can_be_converted_to`, which also accepts value-converting
/// widenings (`Int↔Float`, `IVec3↔Vec3`, `IMat3↔Mat3`,
/// `LatticeVecs→DrawingPlane`).
///
/// Used at record-field leaf positions in
/// `can_be_structurally_converted_to`: pass-through coercion requires that a
/// destructure read the runtime payload as-is, so only widenings that need no
/// per-field conversion are admissible.
pub fn is_tag_only_widening(src: &DataType, dst: &DataType) -> bool {
    if src == dst {
        return true;
    }
    matches!(
        (src, dst),
        (DataType::Crystal, DataType::HasAtoms)
            | (DataType::Crystal, DataType::HasStructure)
            | (DataType::Molecule, DataType::HasAtoms)
            | (DataType::Molecule, DataType::HasFreeLinOps)
            | (DataType::Blueprint, DataType::HasStructure)
            | (DataType::Blueprint, DataType::HasFreeLinOps)
    )
}

/// Like `DataType::can_be_converted_to`, but at leaf positions accepts only
/// tag-only widenings (identity plus concrete-to-abstract phase upcasts) —
/// never value-converting widenings such as `Int → Float` or `IVec3 → Vec3`,
/// no single-value-to-array broadcasting, no function partial application.
///
/// The no-promotion guarantee is cooperative: the record arm here delegates to
/// `can_be_converted_to`, whose record arm in turn recurses through *this*
/// function for field types. Keep the two record arms in sync — if either side
/// changes its field-level dispatch, scalar promotion can leak into records.
pub fn can_be_structurally_converted_to(
    src: &DataType,
    dst: &DataType,
    registry: &NodeTypeRegistry,
) -> bool {
    match (src, dst) {
        // Records: same width + depth structural rule as the record arm of
        // `can_be_converted_to` (which itself uses the strict variant for
        // field-level checks, so this is safe to delegate).
        (DataType::Record(_), DataType::Record(_)) => {
            DataType::can_be_converted_to(src, dst, registry)
        }
        // Arrays: element-wise, stays strict.
        (DataType::Array(s), DataType::Array(d)) => {
            can_be_structurally_converted_to(s, d, registry)
        }
        // Leaf position: identity + concrete→abstract phase upcasts only.
        _ => is_tag_only_widening(src, dst),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum DataTypeToken {
    Identifier(String),
    LeftBracket,  // [
    RightBracket, // ]
    LeftParen,    // (
    RightParen,   // )
    Arrow,        // ->
    FatArrow,     // =>
    Comma,        // ,
    Eof,
}

struct DataTypeLexer {
    input: Vec<char>,
    pos: usize,
}

impl DataType {
    /// Parses a DataType from its textual representation
    pub fn from_string(input: &str) -> Result<DataType, String> {
        let tokens = DataTypeLexer::tokenize(input)?;
        let mut parser = DataTypeParser::new(tokens);
        let data_type = parser.parse_data_type()?;
        parser.expect(DataTypeToken::Eof)?;
        Ok(data_type)
    }
}

struct DataTypeParser {
    tokens: Vec<DataTypeToken>,
    pos: usize,
}

impl DataTypeParser {
    fn new(tokens: Vec<DataTypeToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &DataTypeToken {
        self.tokens.get(self.pos).unwrap_or(&DataTypeToken::Eof)
    }

    fn bump(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: DataTypeToken) -> Result<(), String> {
        if self.peek() == &expected {
            self.bump();
            Ok(())
        } else {
            Err(format!("Expected {:?}, found {:?}", expected, self.peek()))
        }
    }

    fn parse_data_type(&mut self) -> Result<DataType, String> {
        let mut data_type = self.parse_primary_type()?;

        // Handle right-associative '->' for single-parameter functions
        if self.peek() == &DataTypeToken::Arrow {
            self.bump(); // consume '->'
            let return_type = self.parse_data_type()?;
            data_type = DataType::Function(FunctionType {
                parameter_types: vec![data_type],
                output_type: Box::new(return_type),
            });
        }

        Ok(data_type)
    }

    fn parse_primary_type(&mut self) -> Result<DataType, String> {
        match self.peek() {
            DataTypeToken::Identifier(_) => self.parse_builtin_type(),
            DataTypeToken::LeftBracket => self.parse_array_type(),
            DataTypeToken::LeftParen => self.parse_parenthesized_type(),
            other => Err(format!(
                "Unexpected token while parsing primary type: {:?}",
                other
            )),
        }
    }

    fn parse_builtin_type(&mut self) -> Result<DataType, String> {
        match self.peek().clone() {
            DataTypeToken::Identifier(name) => {
                self.bump();
                // Explicit named-record syntax: `Record(Name)`. Disambiguates
                // record references from built-ins and from bare-identifier
                // node references in the text-format parser. Anonymous record
                // syntax (`{x: Int, y: Int}`) is reserved for Phase 7 (the
                // expression language) and is not parsed here.
                if name == "Record" {
                    self.expect(DataTypeToken::LeftParen)?;
                    let inner_name = match self.peek().clone() {
                        DataTypeToken::Identifier(n) => {
                            self.bump();
                            n
                        }
                        other => {
                            return Err(format!(
                                "Expected record name after `Record(`, found {:?}",
                                other
                            ));
                        }
                    };
                    self.expect(DataTypeToken::RightParen)?;
                    return Ok(DataType::Record(RecordType::Named(inner_name)));
                }
                // Iterator type: `Iter[T]`. The bare identifier `Iter` is
                // reserved as a type-name keyword (only legal in
                // type-expression positions; the text-format parser uses
                // separate identifiers for node references). Anything other
                // than `Iter[..]` after consuming `Iter` is a parse error.
                if name == "Iter" {
                    self.expect(DataTypeToken::LeftBracket)?;
                    let element_type = self.parse_data_type()?;
                    self.expect(DataTypeToken::RightBracket)?;
                    return Ok(DataType::Iterator(Box::new(element_type)));
                }
                match name.as_str() {
                    "None" => Ok(DataType::None),
                    "Bool" => Ok(DataType::Bool),
                    "String" => Ok(DataType::String),
                    "Int" => Ok(DataType::Int),
                    "Float" => Ok(DataType::Float),
                    "Vec2" => Ok(DataType::Vec2),
                    "Vec3" => Ok(DataType::Vec3),
                    "IVec2" => Ok(DataType::IVec2),
                    "IVec3" => Ok(DataType::IVec3),
                    "IMat3" => Ok(DataType::IMat3),
                    "Mat3" => Ok(DataType::Mat3),
                    "LatticeVecs" => Ok(DataType::LatticeVecs),
                    "DrawingPlane" => Ok(DataType::DrawingPlane),
                    "Geometry2D" => Ok(DataType::Geometry2D),
                    "Blueprint" => Ok(DataType::Blueprint),
                    "HasAtoms" => Ok(DataType::HasAtoms),
                    "Crystal" => Ok(DataType::Crystal),
                    "Molecule" => Ok(DataType::Molecule),
                    "HasStructure" => Ok(DataType::HasStructure),
                    "HasFreeLinOps" => Ok(DataType::HasFreeLinOps),
                    "Motif" => Ok(DataType::Motif),
                    "Structure" => Ok(DataType::Structure),
                    "Unit" => Ok(DataType::Unit),
                    // Plain unknown identifiers are NOT silently treated as
                    // record names: the text-format parser uses this as a
                    // probe to distinguish "is this a built-in type" from
                    // "is this a node reference", and a permissive fallback
                    // would make every node-reference identifier look like a
                    // dangling record. Record types in DataType strings must
                    // use the explicit `Record(name)` syntax handled
                    // separately by the parser.
                    _ => Err(format!("Unknown data type: {}", name)),
                }
            }
            other => Err(format!("Expected identifier, found {:?}", other)),
        }
    }

    fn parse_array_type(&mut self) -> Result<DataType, String> {
        self.expect(DataTypeToken::LeftBracket)?;
        let element_type = self.parse_data_type()?;
        self.expect(DataTypeToken::RightBracket)?;
        Ok(DataType::Array(Box::new(element_type)))
    }

    fn parse_parenthesized_type(&mut self) -> Result<DataType, String> {
        self.expect(DataTypeToken::LeftParen)?;

        // Case 1: Empty parameter list for a function, e.g., '() -> Int'
        if self.peek() == &DataTypeToken::RightParen {
            self.bump(); // consume ')'
            self.expect(DataTypeToken::Arrow)?;
            let output_type = self.parse_data_type()?;
            return Ok(DataType::Function(FunctionType {
                parameter_types: vec![],
                output_type: Box::new(output_type),
            }));
        }

        // It's not an empty list, so parse the first type.
        let first_type = self.parse_data_type()?;

        // After the first type, we can have a comma (multi-param func) or a right paren (grouped type).
        if self.peek() == &DataTypeToken::Comma {
            // Case 2: Multi-parameter function, e.g., '(Int, Float) => Bool'
            let mut params = vec![first_type];
            while self.peek() == &DataTypeToken::Comma {
                self.bump(); // consume ','
                params.push(self.parse_data_type()?);
            }
            self.expect(DataTypeToken::RightParen)?;
            self.expect(DataTypeToken::FatArrow)?;
            let output_type = self.parse_data_type()?;
            Ok(DataType::Function(FunctionType {
                parameter_types: params,
                output_type: Box::new(output_type),
            }))
        } else {
            // Case 3: A single, grouped type, e.g., '(Int)' or '(Int -> Bool)'
            self.expect(DataTypeToken::RightParen)?;
            Ok(first_type)
        }
    }
}

impl DataTypeLexer {
    fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    pub fn tokenize(input: &str) -> Result<Vec<DataTypeToken>, String> {
        let mut lexer = Self::new(input);
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token()?;
            if token == DataTypeToken::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }

        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_identifier(&mut self) -> String {
        let mut result = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                result.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        result
    }

    fn next_token(&mut self) -> Result<DataTypeToken, String> {
        self.skip_whitespace();

        match self.peek() {
            None => Ok(DataTypeToken::Eof),
            Some('[') => {
                self.advance();
                Ok(DataTypeToken::LeftBracket)
            }
            Some(']') => {
                self.advance();
                Ok(DataTypeToken::RightBracket)
            }
            Some('(') => {
                self.advance();
                Ok(DataTypeToken::LeftParen)
            }
            Some(')') => {
                self.advance();
                Ok(DataTypeToken::RightParen)
            }
            Some(',') => {
                self.advance();
                Ok(DataTypeToken::Comma)
            }
            Some('-') => {
                self.advance();
                if self.peek() == Some('>') {
                    self.advance();
                    Ok(DataTypeToken::Arrow)
                } else {
                    Err("Expected '>' after '-'".to_string())
                }
            }
            Some('=') => {
                self.advance();
                if self.peek() == Some('>') {
                    self.advance();
                    Ok(DataTypeToken::FatArrow)
                } else {
                    Err("Expected '>' after '='".to_string())
                }
            }
            Some(ch) if ch.is_alphabetic() || ch == '_' => {
                let identifier = self.read_identifier();
                Ok(DataTypeToken::Identifier(identifier))
            }
            Some(other) => Err(format!("Unexpected character: {}", other)),
        }
    }
}
