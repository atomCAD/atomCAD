use crate::api::common_api_types::APIIVec2;
use crate::api::common_api_types::APIIVec3;
use crate::api::common_api_types::APITransform;
use crate::api::common_api_types::APIVec2;
use crate::api::common_api_types::APIVec3;
use crate::structure_designer::evaluator::network_evaluator::PrintLogEntry;
use crate::structure_designer::node_network::CollapseMode;
use flutter_rust_bridge::frb;
use std::collections::HashMap;
use std::time::UNIX_EPOCH;

#[derive(Clone)]
pub enum APIEditAtomTool {
    Default,
    AddAtom,
    AddBond,
}

#[derive(PartialEq)]
pub enum APIDataTypeBase {
    None,
    Bool,
    String,
    Int,
    Float,
    Vec2,
    Vec3,
    IVec2,
    IVec3,
    IMat2,
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
    /// The type with exactly one value. Produced by effect nodes; carries no
    /// payload. See `doc/design_node_execution.md`.
    Unit,
    /// Named record type. `custom_data_type` carries the record def name
    /// (empty string when no schema chosen yet — a dangling reference). The
    /// Flutter type-selector exposes this as a separate "Record" branch
    /// with a dropdown of named defs; anonymous record types are not
    /// reachable from the UI in v1 and round-trip through `Custom` instead.
    Record,
    /// `Iter[T]`: `children = [T]`.
    /// See `doc/design_structural_function_and_iter_types.md`.
    Iter,
    /// `Optional[T]`: `children = [T]` (one entry, the inner type). A
    /// record-field modifier only — never a pin type. Represented like `Iter`
    /// (one child). See `doc/design_optional_type.md` §7.
    Optional,
    /// `Function((p0, p1, ..., pN-1) -> R)`:
    /// `children = [p0, p1, ..., pN-1, R]` (rightmost slot is the return
    /// type). See `doc/design_structural_function_and_iter_types.md`.
    Function,
    Custom,
}

pub struct APIDataType {
    pub data_type_base: APIDataTypeBase,
    /// Carries the inner string for both `Custom` (a free-form `DataType`
    /// string) and `Record` (the record def name). `Some` iff the base is
    /// one of those two; `None` otherwise.
    pub custom_data_type: Option<String>,
    pub array: bool, // combined with built_in_data_type, but only redundant with custom_data_type as the outermost array is within the string in that case.
    /// Recursive children, interpretation driven by `data_type_base`. Empty
    /// for every base except `Iter` (one child, the element type), `Optional`
    /// (one child, the inner type) and `Function` (N+1 children: params then
    /// return). See `doc/design_structural_function_and_iter_types.md` and
    /// `doc/design_optional_type.md`.
    pub children: Vec<APIDataType>,
}

pub struct InputPinView {
    pub name: String,
    pub data_type: String,
    pub multi: bool,
    /// Optional concrete type the Flutter editor should send as the drag
    /// source when a wire is dragged *off* this pin, overriding `data_type`.
    /// Populated only when the declared `data_type` is deliberately lossy:
    /// `map.f`'s `AnyFunction` declaration omits the return type, so map sets
    /// this to the concrete `(input_type) -> output_type` signature, letting a
    /// dropped `closure` reflect the map's output type exactly. `None` for
    /// every pin whose declared type already drives drag inference correctly.
    /// See `doc/design_drag_aware_add_node.md` (Tier 2).
    pub drag_hint_type: Option<String>,
}

pub struct OutputPinView {
    pub name: String,
    /// Declared pin type. For polymorphic pins (`SameAsInput`/`SameAsArrayElements`)
    /// or abstract `Fixed` types, this is the abstract declaration string
    /// (e.g. `"SameAsInput(input)"` or `"HasStructure"`).
    pub data_type: String,
    /// The concrete type the pin resolves to in the current network, if it can be
    /// resolved. `Some` only when resolution succeeds and produces a concrete type
    /// that differs from `data_type`. The Flutter UI should prefer this over
    /// `data_type` for color-coding and the primary tooltip label when present.
    pub resolved_data_type: Option<String>,
    /// `true` only when `resolved_data_type` came from the pin's
    /// `SameAsInput` `fallback_if_disconnected` because the named input had
    /// zero connections. The Flutter UI surfaces this in the tooltip as
    /// "default — no input connected" so users can distinguish a type that
    /// was inferred from an upstream wire from one that was filled in by the
    /// node's intrinsic content (e.g. `atom_edit` with no input → Molecule).
    pub resolved_via_fallback: bool,
    pub index: i32,
    /// Alignment of this pin's last-evaluated value. `None` for types without
    /// alignment (Molecule, primitives, …) or when the pin has not been
    /// evaluated in the current scene.
    pub alignment: Option<APIAlignment>,
    /// Short human-readable reason for why alignment is degraded. `None` when
    /// `alignment == Some(Aligned)`, when the pin has no alignment state, or
    /// when the pin has not been evaluated in the current scene.
    pub alignment_reason: Option<String>,
}

/// Blueprint/Crystal alignment state, mirrored from `network_result::Alignment`.
/// Surfaced in the Flutter UI as wire dash style + pin tooltip colouring.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum APIAlignment {
    Aligned,
    MotifUnaligned,
    LatticeUnaligned,
}

#[frb]
pub struct NodeView {
    pub id: u64,
    pub node_type_name: String,
    pub custom_name: Option<String>,
    #[frb(non_final)]
    pub position: APIVec2,
    pub input_pins: Vec<InputPinView>,
    pub output_type: String,
    pub output_pins: Vec<OutputPinView>,
    pub displayed_pins: Vec<i32>,
    pub function_type: String,
    /// Derived "function mode" flag: some node in the same network consumes this
    /// node's function pin (`-1` output) via an HOF `f` pin or `apply.f`. When
    /// `true` the node acts purely as a function value — the scene builder skips
    /// it and the Flutter editor disables its output-pin eye(s) (the redirect is
    /// the `apply` node). Mirrors `NodeNetwork::function_pin_consumed`; derived
    /// per refresh, never stored. See `doc/design_function_pins.md`
    /// §"Display in function mode".
    pub function_pin_consumed: bool,
    pub selected: bool,
    pub active: bool, // True if this is the active node (for properties panel/gadget)
    pub displayed: bool,
    pub return_node: bool,
    pub error: Option<String>,
    pub output_pin_strings: Vec<String>,
    pub subtitle: Option<String>,
    // Comment node specific fields (only populated for Comment nodes)
    pub comment_label: Option<String>,
    pub comment_text: Option<String>,
    pub comment_width: Option<f64>,
    pub comment_height: Option<f64>,
    /// User-supplied free-form label for `closure` nodes (populated from
    /// `ClosureData::custom_label`). `None` for all other node types and for
    /// closures without a label. Drives the title-bar `<label> · ƒ <sig>`
    /// rendering.
    pub closure_custom_label: Option<String>,
    /// Present iff this node is an HOF (the node type declares zone pins).
    /// Carries the entire body as a nested view. `None` for non-HOF nodes.
    /// Phase U3 surfaces zone-pin definitions and the body's `stored_width`/
    /// `stored_height` plus a node count; body nodes/wires arrive in U4.
    /// See `doc/design_zones_ui.md` §"Phase U3".
    pub zone: Option<ZoneView>,
    /// Surfaces "this node's layout/output type is derived from a wired input
    /// pin" for the unified `apply` / `map` UX in function-pin unification
    /// Phase D. Populated by `build_node_view` only for `apply` and `map`;
    /// `None` for every other node type. See
    /// `doc/design_function_pin_unification.md` (Phase D).
    pub derived_shape: Option<APIDerivedShapeView>,
}

/// "Is this node's layout/output type derived from a wired input pin?"
///
/// Apply uses this to drive its no-pins-until-wired UX: when `f` is connected
/// the post-pass materialises arg pins from the wired source's flat function
/// type, otherwise only the `f` pin renders. Map uses it to flip its
/// `output_type` editor between editable (fallback) and read-only (derived).
///
/// `derived_from_input_pin` is `Some(pin_name)` when the wired source on
/// `pin_name` drives the derived layout/output, `None` otherwise. Per-pin
/// info continues to flow through the existing `NodeView` machinery; this
/// view holds only the derivation status. See
/// `doc/design_function_pin_unification.md` (Phase D).
pub struct APIDerivedShapeView {
    pub derived_from_input_pin: Option<String>,
}

/// Read-only view of an HOF node's body, surfaced for the Flutter editor.
///
/// Phase U4 populates body-internal `nodes` and `wires`; non-HOF body nodes
/// have `NodeView.zone == None`, terminating the recursion. Cross-scope wires
/// (captures, iteration-value references, body returns) land in U5.
pub struct ZoneView {
    /// Zone-input pins (inner-left) declared by the HOF type. From the body's
    /// perspective these are sources; reuses `OutputPinView` for shape parity
    /// with external output pins.
    pub zone_input_pins: Vec<OutputPinView>,
    /// Zone-output pins (inner-right) declared by the HOF type. From the body's
    /// perspective these are destinations; reuses `InputPinView`.
    pub zone_output_pins: Vec<InputPinView>,
    /// All nodes inside the body, keyed by id. Positions are body-local.
    /// Populated in U4; a non-HOF body node has `NodeView.zone == None`.
    pub nodes: HashMap<u64, NodeView>,
    /// All wires inside the body. U4 surfaces intra-body wires only; cross-
    /// scope wires (captures, iteration values, body returns) come in U5.
    pub wires: Vec<WireView>,
    /// Stored body width in logical pixels. The renderer uses
    /// `max(stored_width, content_bbox + padding)`.
    pub stored_width: f64,
    /// Stored body height in logical pixels.
    pub stored_height: f64,
    /// The raw stored mode, for the context menu's check-mark. `APICollapseMode`
    /// mirrors `CollapseMode`. See `doc/design_hof_node_collapse.md`.
    pub collapse_mode: APICollapseMode,
    /// Effective "body hidden, node rendered compact" — already resolved
    /// Rust-side (`Auto` reads `f`-connection). The renderer/layout reads only
    /// this, never re-deriving it.
    pub collapsed: bool,
    /// Whether this node type supports the collapse override (true for the four
    /// HOFs, false for `closure`). Gates the context-menu group.
    pub collapsable: bool,
}

/// Flutter-facing mirror of [`CollapseMode`]. The user's choice for whether a
/// collapsable HOF's inline body is shown; `Auto` (default) derives the
/// effective state from the `f` pin. See `doc/design_hof_node_collapse.md`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum APICollapseMode {
    Auto,
    Collapsed,
    Expanded,
}

impl From<CollapseMode> for APICollapseMode {
    fn from(mode: CollapseMode) -> Self {
        match mode {
            CollapseMode::Auto => APICollapseMode::Auto,
            CollapseMode::Collapsed => APICollapseMode::Collapsed,
            CollapseMode::Expanded => APICollapseMode::Expanded,
        }
    }
}

impl From<APICollapseMode> for CollapseMode {
    fn from(mode: APICollapseMode) -> Self {
        match mode {
            APICollapseMode::Auto => CollapseMode::Auto,
            APICollapseMode::Collapsed => CollapseMode::Collapsed,
            APICollapseMode::Expanded => CollapseMode::Expanded,
        }
    }
}

pub struct WireView {
    pub source_node_id: u64,
    /// Legacy source pin index. Kept for back-compat with code paths that
    /// only handle `NodeOutput` sources: `-1` = function pin, `≥ 0` = regular
    /// output pin index. For `ZoneInput` sources (zones UI phase U5) this is
    /// also set to the pin index for convenience, but consumers that need to
    /// distinguish the source-pin kind should read `source_pin` directly.
    pub source_output_pin_index: i32,
    pub dest_node_id: u64,
    pub dest_param_index: usize,
    pub selected: bool,
    /// Which argument list on the destination node this wire terminates at.
    /// Phase U4 surfaces the discriminator for body-return wires (a body
    /// node's output → its containing HOF's zone-output pin). Defaults to
    /// `External` for every wire on the regular `arguments` list. See
    /// `doc/design_zones_ui.md` §"Wire-creation API generalisation".
    pub destination_argument_kind: APIArgumentKind,
    /// Source pin kind: `NodeOutput` (today's normal wire source) or
    /// `ZoneInput` (inside-facing source pin on an HOF, used by iteration-
    /// value references). Phase U5 of `doc/design_zones_ui.md`.
    pub source_pin: APISourcePin,
    /// How many ancestor scope frames up from the wire's storage scope the
    /// source lives. `0` = source in the same network as the destination
    /// argument; `≥ 1` = capture or iteration-value reference from an outer
    /// scope. Phase U5.
    pub source_scope_depth: u32,
}

/// Discriminator for which argument list on the destination node a wire
/// terminates at. Mirrors `node_network::ArgumentKind`. See
/// `doc/design_zones_ui.md` §"Data model — Rust API extensions".
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum APIArgumentKind {
    /// Sourced from destination's `arguments` (today's regular wires +
    /// captures + iteration-value references).
    External,
    /// Sourced from destination's `zone_output_arguments` (body-return wires
    /// from a body node's output into its containing HOF's zone-output pin).
    ZoneOutput,
}

/// Discriminator for which side of an HOF node a wire's source pin sits on.
/// Mirrors `node_network::SourcePin`. See `doc/design_zones_ui.md`
/// §"Data model — Rust API extensions" → "WireView carries source kind".
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum APISourcePin {
    /// Outside-facing source pin (regular output, or function pin at
    /// `pin_index = -1`). The legacy default for every wire.
    NodeOutput { pin_index: i32 },
    /// Inside-facing source pin on a zone-owning (HOF) node. Used by
    /// iteration-value references: a wire from the HOF's zone-input pin into
    /// a body-internal node's input.
    ZoneInput { pin_index: u32 },
}

/// Wire identifier for batch selection operations
pub struct WireIdentifier {
    pub source_node_id: u64,
    pub source_output_pin_index: i32,
    pub destination_node_id: u64,
    pub destination_argument_index: usize,
}

pub struct NodeNetworkView {
    pub name: String,
    pub nodes: HashMap<u64, NodeView>,
    pub wires: Vec<WireView>,
}

pub struct APIIntData {
    pub value: i32,
}

pub struct APIStringData {
    pub value: String,
}

/// Shared payload for `record_construct` and `record_destructure` node
/// properties. Both nodes hold a single `schema: String` (the name of a
/// record type def in the project's registry; empty when no schema is
/// chosen yet).
pub struct APIRecordSchemaData {
    pub schema: String,
}

/// One field of a record type def, surfaced for the schema editor UI.
pub struct APIRecordTypeField {
    /// Editing identity of an **existing** field; `None` for a freshly added
    /// row. The schema editor echoes this back on commit so the backend can
    /// preserve input-pin wires across rename / reorder by id rather than name
    /// (`doc/design_record_field_identity.md`).
    pub id: Option<u64>,
    pub name: String,
    pub data_type: APIDataType,
    /// Cosmetic widget annotation for generic literal editors — see
    /// `APIFieldEditorHint` and `doc/design_array_node_and_field_hints.md`
    /// Part A. Round-trips through the schema editor: the getter fills it from
    /// the def, and `update_record_type_def` writes back exactly what the UI
    /// sends (a hint the row's type does not admit is rejected, not dropped).
    pub hint: Option<APIFieldEditorHint>,
}

/// Full record type def (name plus authored field list). Used by the
/// schema editor in the user-types panel.
pub struct APIRecordTypeDef {
    pub name: String,
    pub fields: Vec<APIRecordTypeField>,
}

pub struct APIBoolData {
    pub value: bool,
}

/// Property payload for the `print` node. The single field gates the buffer
/// push to Execute passes only — see `doc/design_node_execution.md` (Phase 4).
pub struct APIPrintData {
    pub execute_only: bool,
}

pub struct APIFloatData {
    pub value: f64,
}

pub struct APIIVec2Data {
    pub value: APIIVec2,
}

pub struct APIIVec3Data {
    pub value: APIIVec3,
}

pub struct APISupercellData {
    pub a: APIIVec3,
    pub b: APIIVec3,
    pub c: APIIVec3,
}

/// `a` / `b` are rows 0 / 1 — same convention as the
/// text properties exposed by `IMat2RowsData::get_text_properties`.
pub struct APIIMat2RowsData {
    pub a: APIIVec2,
    pub b: APIIVec2,
}

/// `a` / `b` are columns 0 / 1 — same convention as the text
/// properties exposed by `IMat2ColsData::get_text_properties`.
pub struct APIIMat2ColsData {
    pub a: APIIVec2,
    pub b: APIIVec2,
}

pub struct APIIMat2DiagData {
    pub v: APIIVec2,
}

/// `a` / `b` are the two superlattice rows 0 / 1 — same convention as the
/// text properties exposed by `PlaneTilingVectorsData::get_text_properties`.
pub struct APIPlaneTilingVectorsData {
    pub a: APIIVec2,
    pub b: APIIVec2,
}

/// `a` / `b` / `c` are rows 0 / 1 / 2 — same convention as the
/// text properties exposed by `IMat3RowsData::get_text_properties`.
pub struct APIIMat3RowsData {
    pub a: APIIVec3,
    pub b: APIIVec3,
    pub c: APIIVec3,
}

/// `a` / `b` / `c` are columns 0 / 1 / 2 — same convention as the text
/// properties exposed by `IMat3ColsData::get_text_properties`.
pub struct APIIMat3ColsData {
    pub a: APIIVec3,
    pub b: APIIVec3,
    pub c: APIIVec3,
}

pub struct APIIMat3DiagData {
    pub v: APIIVec3,
}

pub struct APIMat3RowsData {
    pub a: APIVec3,
    pub b: APIVec3,
    pub c: APIVec3,
}

pub struct APIMat3ColsData {
    pub a: APIVec3,
    pub b: APIVec3,
    pub c: APIVec3,
}

pub struct APIMat3DiagData {
    pub v: APIVec3,
}

pub struct APIRangeData {
    pub start: i32,
    pub step: i32,
    pub count: i32,
}

pub struct APIVec2Data {
    pub value: APIVec2,
}

pub struct APIVec3Data {
    pub value: APIVec3,
}

pub struct APIRectData {
    pub min_corner: APIIVec2,
    pub extent: APIIVec2,
}

pub struct APICircleData {
    pub center: APIIVec2,
    pub radius: i32,
}

pub struct APIExtrudeData {
    pub height: i32,
    pub extrude_direction: APIIVec3,
    pub infinite: bool,
    pub subdivision: i32,
    pub plane_normal: bool,
}

pub struct APICuboidData {
    pub min_corner: APIIVec3,
    pub extent: APIIVec3,
    pub subdivision: i32,
}

pub struct APISphereData {
    pub center: APIIVec3,
    pub radius: i32,
}

pub struct APIHalfPlaneData {
    pub point1: APIIVec2,
    pub point2: APIIVec2,
}

pub struct APIHalfSpaceData {
    pub max_miller_index: i32,
    pub miller_index: APIIVec3,
    pub center: APIIVec3,
    pub shift: i32,
    pub subdivision: i32,
}

pub struct APIDrawingPlaneData {
    pub max_miller_index: i32,
    /// Miller plane index `(h k l)`. `None` = unset/derived (case D: derived
    /// from `u`/`v`).
    pub miller_index: Option<APIIVec3>,
    pub center: APIIVec3,
    pub shift: i32,
    pub subdivision: i32,
    /// First in-plane lattice direction `[u v w]`. `None` = unset.
    pub u_axis: Option<APIIVec3>,
    /// Second in-plane lattice direction `[u v w]`. `None` = unset.
    pub v_axis: Option<APIIVec3>,
    /// Resolved Miller index from the last evaluation (derived in case D).
    /// Read-only; `None` when the node was not the selected node at the last
    /// eval (no eval cache available). Used by the editor to display the
    /// derived index when the stored `miller_index` is unset.
    pub resolved_miller_index: Option<APIIVec3>,
}

pub struct APIFacet {
    pub miller_index: APIIVec3,
    pub shift: i32,
    pub symmetrize: bool,
    pub visible: bool,
}

pub struct APIFacetShellData {
    pub max_miller_index: i32,
    pub center: APIIVec3,
    pub facets: Vec<APIFacet>,
    pub selected_facet_index: Option<usize>,
}

pub struct APIGeoTransData {
    pub translation: APIIVec3,
    pub rotation: APIIVec3,
    pub transform_only_frame: bool,
}

pub struct APIRotationalSymmetry {
    pub axis: APIVec3,
    pub n_fold: u32,
}

pub struct APILatticeSymopData {
    pub translation: APIIVec3,
    pub rotation_axis: Option<APIVec3>,
    pub rotation_angle_degrees: f64,
    pub transform_only_frame: bool,
    pub rotational_symmetries: Vec<APIRotationalSymmetry>,
    pub crystal_system: String,
}

pub struct APIStructureMoveData {
    pub translation: APIIVec3,
    pub lattice_subdivision: i32,
}

pub struct APIStructureRotData {
    pub axis_index: Option<i32>,
    pub step: i32,
    pub pivot_point: APIIVec3,
    pub rotational_symmetries: Vec<APIRotationalSymmetry>,
    pub crystal_system: String,
}

pub struct APIFreeMoveData {
    pub translation: APIVec3,
}

pub struct APIRelaxData {
    /// Prune threshold (Å) for the `diff` output pin. `0.0` = exact.
    pub diff_min_move: f64,
}

pub struct APIXrayData {
    /// Display alpha applied to in-region atoms, in `[0.0, 1.0]`. `1.0`
    /// restores full opacity. Overridden by a wired `alpha` pin.
    pub alpha: f64,
}

pub struct APITagData {
    /// Tag name added to in-region atoms. Overridden by a wired `name` pin.
    pub name: String,
    /// Input structure's existing tag names, captured at the last eval, offered
    /// as suggestions in the editor. Empty until the node has evaluated with a
    /// wired input (§Existing-names suggestions).
    pub available_tags: Vec<String>,
}

pub struct APIUntagData {
    /// Tag name removed from in-region atoms. Overridden by a wired `name` pin.
    /// Empty removes **all** tags from in-region atoms.
    pub name: String,
    /// Input structure's existing tag names, captured at the last eval, offered
    /// as suggestions in the editor.
    pub available_tags: Vec<String>,
}

pub struct APIFreeSphereData {
    pub center: APIVec3,
    pub radius: f64,
}

pub struct APIFreeCircleData {
    pub center: APIVec2,
    pub radius: f64,
}

pub struct APIFreeRotData {
    pub angle_degrees: f64, // In degrees
    pub rot_axis: APIVec3,
    pub pivot_point: APIVec3,
}

pub struct APIEditAtomData {
    pub active_tool: APIEditAtomTool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub bond_tool_last_atom_id: Option<u32>,
    pub selected_atomic_number: i16,
    pub has_selected_atoms: bool,
    pub has_selection: bool,
    pub selection_transform: Option<APITransform>,
}

#[derive(Clone)]
pub enum APIAtomEditTool {
    Default,
    AddAtom,
    AddBond,
    /// Placement guideline tool (issue #368): constrains atom placement to a
    /// transient line. Self-contained — clears on tool switch / node deselect.
    Guideline,
}

/// Freeze mode for atom_edit energy minimization.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIMinimizeFreezeMode {
    /// Only diff atoms move; base atoms are frozen.
    FreezeBase,
    /// All atoms move freely.
    FreeAll,
    /// Only selected atoms move; everything else is frozen.
    FreeSelected,
}

// --- Default tool pointer event result types ---

/// Discriminant for default_tool_pointer_down result.
pub enum PointerDownResultKind {
    /// A gadget handle was hit. Flutter should start existing gadget drag.
    /// See `PointerDownResult.gadget_handle_index` for the handle.
    GadgetHit,
    /// Mouse-down on an atom. Entered PendingAtom state.
    StartedOnAtom,
    /// Mouse-down on a bond. Entered PendingBond state.
    StartedOnBond,
    /// Mouse-down on empty space. Entered PendingMarquee state.
    StartedOnEmpty,
}

/// Result of default_tool_pointer_down.
pub struct PointerDownResult {
    pub kind: PointerDownResultKind,
    /// Only valid when kind == GadgetHit.
    pub gadget_handle_index: i32,
}

/// Status of frozen atoms during a drag operation.
pub enum DragFrozenStatus {
    /// No frozen atoms in selection — all atoms moved normally.
    NoneFrozen,
    /// Some selected atoms were frozen and skipped; others moved.
    SomeFrozen,
    /// All selected atoms were frozen — nothing moved.
    AllFrozen,
}

/// Discriminant for default_tool_pointer_move result.
pub enum PointerMoveResultKind {
    /// Threshold not exceeded yet.
    StillPending,
    /// Screen-plane drag in progress (atoms moved).
    Dragging,
    /// Marquee rectangle updated.
    MarqueeUpdated,
}

/// Result of default_tool_pointer_move.
pub struct PointerMoveResult {
    pub kind: PointerMoveResultKind,
    /// Marquee rectangle in screen coords [x, y, w, h]. Only valid when kind == MarqueeUpdated.
    pub marquee_rect_x: f64,
    pub marquee_rect_y: f64,
    pub marquee_rect_w: f64,
    pub marquee_rect_h: f64,
    /// Status of frozen atoms during drag. Only meaningful when kind == Dragging.
    pub frozen_drag_status: DragFrozenStatus,
}

/// Result of default_tool_pointer_up.
pub enum PointerUpResult {
    /// Click-select happened.
    SelectionChanged,
    /// Screen-plane drag finished.
    DragCommitted,
    /// Marquee selection applied.
    MarqueeCommitted,
    /// No-op (e.g., click on empty with no prior selection).
    NothingHappened,
}

// --- AddBond tool pointer event result types ---

/// Result of add_bond_pointer_move. Contains all info Flutter needs to draw
/// the rubber-band preview line as a 2D overlay.
pub struct APIAddBondMoveResult {
    /// True if we are in the Dragging state (rubber-band should be drawn).
    pub is_dragging: bool,
    /// World position of the source atom (start of the rubber-band).
    pub source_atom_x: f64,
    pub source_atom_y: f64,
    pub source_atom_z: f64,
    /// True if source_atom position is valid.
    pub has_source_pos: bool,
    /// World position of the preview end point.
    pub preview_end_x: f64,
    pub preview_end_y: f64,
    pub preview_end_z: f64,
    /// True if preview_end position is valid.
    pub has_preview_end: bool,
    /// True if the cursor is hovering over a valid snap target atom.
    pub snapped_to_atom: bool,
    /// Current bond order setting, for visual styling of the preview line.
    pub bond_order: u8,
}

pub struct APIDiffStats {
    pub atoms_added: u32,
    pub atoms_deleted: u32,
    pub atoms_modified: u32,
    pub bonds_added: u32,
    pub bonds_deleted: u32,
    /// Anchored diff atoms whose base atom no longer exists (skipped).
    pub orphaned_tracked_atoms: u32,
    /// Delete markers that found no base atom to delete (no-op).
    pub unmatched_delete_markers: u32,
    /// Diff bonds where one or both endpoints were missing from the result (skipped).
    pub orphaned_bonds: u32,
    /// UNCHANGED markers that matched a base atom (bond endpoint references).
    pub unchanged_references: u32,
}

pub struct APIAtomEditData {
    pub active_tool: APIAtomEditTool,
    pub bond_tool_last_atom_id: Option<u32>,
    pub bond_tool_bond_order: u8,
    pub selected_atomic_number: i16,
    pub is_in_guided_placement: bool,
    pub has_selected_atoms: bool,
    pub has_selected_bonds: bool,
    pub selected_bond_count: u32,
    /// Bond order of selected bonds (1-7), or None if no bonds selected or mixed orders.
    pub selected_bond_order: Option<u8>,
    pub has_selection: bool,
    pub selection_transform: Option<APITransform>,
    pub output_diff: bool,
    pub show_anchor_arrows: bool,
    pub include_base_bonds_in_diff: bool,
    pub tolerance: f64,
    pub error_on_stale_entries: bool,
    pub show_gadget: bool,
    pub diff_stats: APIDiffStats,
    pub measurement: Option<APIMeasurement>,
    /// Result-space ID of the most recently selected atom (for dialog defaults).
    pub last_selected_result_atom_id: Option<u32>,
    /// True if any atom has the frozen flag set.
    pub has_frozen_atoms: bool,
    /// Whether continuous minimization is enabled on this atom_edit node.
    pub continuous_minimization: bool,
    /// True if the active node is motif_edit (not atom_edit).
    pub is_motif_mode: bool,
    /// Parameter element definitions (motif_edit only).
    pub parameter_elements: Vec<APIParameterElement>,
    /// Ghost atom neighbor depth (0.0–1.0, motif_edit only).
    pub neighbor_depth: f64,
}

/// A parameter element definition for motif_edit nodes.
pub struct APIParameterElement {
    /// User-defined name (e.g., "PRIMARY").
    pub name: String,
    /// Default atomic number (e.g., 6 for Carbon).
    pub default_atomic_number: i16,
    /// Reserved atomic number used internally (-100, -101, ...).
    pub reserved_atomic_number: i16,
    /// Display color as 0xRRGGBB.
    pub color: u32,
}

/// Measurement computed from selected atoms (2-4 atoms).
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone)]
pub enum APIMeasurement {
    /// Distance between 2 atoms in Angstroms.
    Distance {
        distance: f64,
        /// Result-space atom IDs for the two atoms.
        atom1_id: u32,
        atom2_id: u32,
        /// Element symbols for display labels.
        atom1_symbol: String,
        atom2_symbol: String,
        /// Whether the two atoms are bonded (enables Default button in dialog).
        is_bonded: bool,
    },
    /// Angle at a vertex atom, in degrees.
    Angle {
        angle_degrees: f64,
        /// Vertex atom identity.
        vertex_id: u32,
        vertex_symbol: String,
        /// Arm atoms (indices 0 and 1 for move choice).
        arm_a_id: u32,
        arm_a_symbol: String,
        arm_b_id: u32,
        arm_b_symbol: String,
    },
    /// Dihedral (torsion) angle around the central bond axis, in degrees.
    Dihedral {
        angle_degrees: f64,
        /// Chain A-B-C-D atom identities.
        chain_a_id: u32,
        chain_a_symbol: String,
        chain_b_id: u32,
        chain_b_symbol: String,
        chain_c_id: u32,
        chain_c_symbol: String,
        chain_d_id: u32,
        chain_d_symbol: String,
    },
    /// Single atom info (shown when exactly 1 atom is selected).
    AtomInfo {
        /// Element symbol (e.g., "C").
        symbol: String,
        /// Full element name (e.g., "Carbon").
        element_name: String,
        /// Number of bonds on this atom (coordination number).
        bond_count: u32,
        /// Position in Angstroms.
        x: f64,
        y: f64,
        z: f64,
        /// Hybridization override (0=Auto, 1=Sp3, 2=Sp2, 3=Sp1).
        hybridization_override: u8,
        /// Inferred hybridization from bond orders (1=Sp3, 2=Sp2, 3=Sp1, 0=unknown/terminal).
        inferred_hybridization: u8,
    },
}

/// Information about the atom under the cursor, returned by hover hit test.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone)]
pub struct APIHoveredAtomInfo {
    // Identity
    pub symbol: String,
    pub element_name: String,
    pub atomic_number: i32,
    /// For parameter elements: the resolved real element (e.g. "C (Carbon)").
    /// Empty string when the atom is a normal element with no override.
    pub effective_element: String,

    // Position (world-space Angstroms — used both for display and
    // for Flutter to project the tooltip anchor to screen space)
    pub x: f64,
    pub y: f64,
    pub z: f64,

    // Bonding
    pub bond_count: u32,

    // Frozen state
    pub is_frozen: bool,

    // Hybridization override (0=Auto, 1=Sp3, 2=Sp2, 3=Sp1)
    pub hybridization_override: u8,

    // Inferred hybridization from bond orders (1=Sp3, 2=Sp2, 3=Sp1, 0=unknown/terminal)
    pub inferred_hybridization: u8,

    // Tags — names of the tags this atom carries, in bit order. Empty for
    // untagged atoms; the popup omits the row entirely when empty.
    pub tags: Vec<String>,

    // Node origin — name of the node that produced this atom
    pub node_name: String,

    // Overlap — names of other nodes with atoms at nearly the same position
    pub overlapping_node_names: Vec<String>,
}

/// Hybridization override for guided atom placement.
/// When set to Auto, hybridization is auto-detected via UFF type assignment.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIHybridization {
    /// Auto-detect hybridization from bonding state.
    Auto,
    /// sp3 tetrahedral (109.47°).
    Sp3,
    /// sp2 trigonal planar (120°).
    Sp2,
    /// sp1 linear (180°).
    Sp1,
}

/// Bond mode for guided atom placement saturation limits.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIBondMode {
    /// Use element-specific covalent max neighbors.
    Covalent,
    /// Use geometric max (unlocks lone pair / empty orbital positions).
    Dative,
}

/// Bond length computation mode for guided atom placement.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIBondLengthMode {
    /// Use crystal lattice bond length table (with UFF fallback).
    Crystal,
    /// Always use UFF rest bond length formula.
    Uff,
}

/// Result of attempting to start guided atom placement.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone)]
pub enum GuidedPlacementApiResult {
    /// No atom was hit by the ray.
    NoAtomHit,
    /// The hit atom is saturated.
    AtomSaturated {
        /// True when the atom has lone pairs / empty orbitals
        /// (switch to Dative bond mode to access them).
        has_additional_capacity: bool,
        /// True when has_additional_capacity is true but the new element cannot
        /// form a dative bond with the anchor (no valid donor-acceptor pair).
        dative_incompatible: bool,
    },
    /// Guided placement started successfully.
    GuidedPlacementStarted {
        guide_count: i32,
        anchor_atom_id: i32,
    },
}

/// Which user-visible state the Guideline tool (issue #368) is in. Derived from
/// the tool phase plus whether an atom is picked.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIGuidelinePhase {
    /// No guideline yet — the user picks 1–3 defining atoms.
    Define,
    /// A frozen line exists, no atom picked (the ghost marker is the active point).
    Place,
    /// A frozen line exists, an atom is picked (it is the active point).
    Move,
}

/// Read-only view of the active Guideline tool for the panel (issue #368).
/// `None` (a missing `Option<APIGuidelineToolView>`) means the Guideline tool is
/// not active (some other tool, or no atom_edit node selected).
#[flutter_rust_bridge::frb]
pub struct APIGuidelineToolView {
    /// Which user-visible state the tool is in.
    pub phase: APIGuidelinePhase,
    /// Number of atoms in the tool-local defining set (Define phase; 0 otherwise).
    /// Drives the Create button label: 1 → Directional, 2 → Center, 3 → Equidistant.
    pub defining_count: u32,
    /// Whether the Create button is enabled (1–3 defining atoms).
    pub can_create: bool,
    /// Whether the 1-atom direction field should be shown (`defining_count == 1`).
    pub needs_direction: bool,
    /// The active point's along-line position `t` (signed Å from origin). The ghost
    /// marker in Place; the picked atom's live projection in Move; 0 in Define.
    pub t: f64,
}

pub struct APIRegPolyData {
    pub num_sides: i32,
    pub radius: i32,
}

pub struct APIParameterData {
    pub param_index: usize,
    pub param_name: String,
    pub data_type: APIDataType,
    pub sort_order: i32,
    pub error: Option<String>,
}

pub struct APINetworkWithValidationErrors {
    pub name: String,
    pub validation_errors: Option<String>,
}

#[frb]
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub enum NodeTypeCategory {
    Annotation,
    MathAndProgramming,
    Geometry2D,
    Geometry3D,
    AtomicStructure,
    OtherBuiltin,
    Custom,
}

impl NodeTypeCategory {
    pub fn order(&self) -> u8 {
        match self {
            Self::Annotation => 0,
            Self::MathAndProgramming => 1,
            Self::Geometry2D => 2,
            Self::Geometry3D => 3,
            Self::AtomicStructure => 4,
            Self::OtherBuiltin => 5,
            Self::Custom => 6,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Annotation => "Annotation",
            Self::MathAndProgramming => "Math and Programming",
            Self::Geometry2D => "2D Blueprint",
            Self::Geometry3D => "3D Blueprint",
            Self::AtomicStructure => "Atomic Structure",
            Self::OtherBuiltin => "Other",
            Self::Custom => "Custom",
        }
    }
}

#[derive(Clone)]
pub struct APINodeTypeView {
    pub name: String,
    pub description: String,
    pub summary: Option<String>,
    pub category: NodeTypeCategory,
}

/// Source pin context for the drag-aware add-node popup. When the user drags
/// a wire from a pin and drops on empty space, the selected node type is
/// instantiated with type properties pre-configured to match the source pin.
/// `source_pin_type` is `DataType::Display` (round-trips through
/// `DataType::from_string`); a string that fails to parse is treated as if
/// no drag source were supplied. See `doc/design_drag_aware_add_node.md`.
pub struct APIDragSource {
    pub source_pin_type: String,
    pub dragging_from_output: bool,
}

#[derive(Clone)]
pub struct APINodeCategoryView {
    pub category: NodeTypeCategory,
    pub nodes: Vec<APINodeTypeView>,
}

pub struct APIExprParameter {
    pub name: String,
    pub data_type: APIDataType,
}

pub struct APIExprData {
    pub parameters: Vec<APIExprParameter>,
    pub expression: String,
    pub error: Option<String>,
    pub output_type: Option<APIDataType>,
}

pub struct APIImportXYZData {
    pub file_name: Option<String>,
}

pub struct APIImportCIFData {
    pub file_name: Option<String>,
    pub block_name: Option<String>,
    pub use_cif_bonds: bool,
    pub infer_bonds: bool,
    pub bond_tolerance: f64,
}

pub struct APIInferBondsData {
    pub additive: bool,
    pub bond_tolerance: f64,
}

pub struct APIAtomReplaceData {
    /// List of (from_atomic_number, to_atomic_number) replacement rules.
    pub replacements: Vec<APIAtomReplaceRule>,
}

pub struct APIAtomReplaceRule {
    pub from_atomic_number: i32,
    pub to_atomic_number: i32,
}

pub struct APIExportAtomsData {
    pub file_name: String,
}

/// A single supported atom-export format, projected from
/// `crystolecule::io::atom_export::AtomExportFormat` for the Flutter UI. The
/// format dialog, the reactive format indicator, and any future format chooser
/// are all built from `get_atom_export_formats()` so adding a format in Rust
/// updates the UI with no Flutter edits.
pub struct APIAtomExportFormat {
    /// Canonical extension without a leading dot (`"xyz"` / `"mol"`).
    pub extension: String,
    /// Short human-readable label (`"XYZ"` / `"MOL (V3000)"`).
    pub label: String,
    /// One-line description of what the format captures.
    pub description: String,
}

pub struct APICommentData {
    pub label: String,
    pub text: String,
    pub width: f64,
    pub height: f64,
}

pub struct APIApplyDiffData {
    pub tolerance: f64,
    pub error_on_stale: bool,
}

pub struct APIAtomComposeDiffData {
    pub tolerance: f64,
    pub error_on_stale: bool,
}

pub struct APIAtomCutData {
    pub cut_sdf_value: f64,
    pub unit_cell_size: f64,
}

pub struct APIMapData {
    pub input_type: APIDataType,
    pub output_type: APIDataType,
}

pub struct APIFilterData {
    pub element_type: APIDataType,
}

pub struct APIForeachData {
    pub input_type: APIDataType,
}

/// Editable state of a `zip_with` node (n-ary element-wise map, issue #382).
/// Lane pin names (`xs1..xsN` / `element1..elementN`) are derived positionally
/// and the hidden per-lane stable ids are managed Rust-side, so they never
/// cross the API — only the ordered lane types and the stored output type are
/// exposed. `output_type` is user-editable when `f` is disconnected and
/// read-only (derived) display when `f` is wired. See `doc/design_zip_with.md`
/// (Phase 5).
pub struct APIZipWithData {
    /// The N input lane element types, in pin order (`xs1..xsN`).
    pub lane_types: Vec<APIDataType>,
    /// The stored output element type (the fallback used when `f` is
    /// disconnected; the output pin resolves to `Iter[output_type]`).
    pub output_type: APIDataType,
}

/// Editable state of a `switch` node (select a value by matching a selector
/// against literal cases; `doc/design_switch_node.md`). Case values cross the
/// API as **strings** — the editor edits plain text fields and Rust parses each
/// per `selector_type` (an Int selector rejects a non-integer field, surfaced
/// through the setter's `APIResult`). The hidden per-case stable ids are managed
/// Rust-side and never cross the API — only the ordered case values, the
/// selector type, and the value type are exposed. See `doc/design_switch_node.md`
/// (Phase 4).
pub struct APISwitchData {
    /// The selector pin type — Int or String.
    pub selector_type: APIDataType,
    /// The value type of the case pins, the `default` pin, and the output pin.
    pub value_type: APIDataType,
    /// The case literals in pin order, each rendered as a string (parsed back
    /// into the selector domain by the setter).
    pub case_values: Vec<String>,
}

pub struct APICollectData {
    pub element_type: APIDataType,
    /// Optional cap on the number of elements collected. `None` collects the
    /// full stream; overridden by the wired `limit` input pin when connected.
    pub limit: Option<i32>,
    /// Number of elements to skip before collecting. `0` (the default) means
    /// "start at the first element"; overridden by the wired `offset` input
    /// pin when connected.
    pub offset: i32,
}

/// Editable state of a `patch_build` node (surface-reconstruction patch
/// extraction). See `doc/design_surface_patches.md` §4.
pub struct APIPatchBuildData {
    /// Build threshold `ε` (Å) for the interior/ghost split.
    pub epsilon: f64,
}

/// Editable state of a `patch_latticefill` node plus the compatibility stats
/// from its most recent evaluation. See `doc/design_surface_patches.md` §5–6.
pub struct APIPatchLatticeFillData {
    /// Hydrogen-passivate the residual danglers after welding.
    pub passivate: bool,
    /// Weld tolerance in Å.
    pub tolerance: f64,
    /// Cell-selection test height: `true` (default) uses the lattice origin's
    /// height; `false` derives it from the target slab (robust to an offset
    /// target).
    pub test_height_at_origin: bool,
    /// Debug: project the placed patch atoms onto the cell-selection test plane
    /// (no weld/passivation). Non-physical; for understanding cell selection.
    pub debug_project_to_test_plane: bool,
    /// Debug: also place the one-cell-wider frontier of tiles, flagging the
    /// excluded neighbours frozen so they are visible.
    pub debug_show_frontier_tiles: bool,
    /// Compatibility stats from the last successful evaluation, or `None` if the
    /// node has not evaluated yet (or the last evaluation errored).
    pub report: Option<APICompatibilityReport>,
}

/// Welded/orphaned/over-coordination stats from a `patch_latticefill` apply,
/// surfaced as a compatibility badge (§6).
pub struct APICompatibilityReport {
    /// Number of tiles placed. Zero means nothing was tiled (the test plane
    /// missed the target) — the other counts being zero is then *not* success.
    pub placed_cells: usize,
    /// Patch-ghosts that found a real twin and fused (realized periodic / collar
    /// bonds).
    pub welded_ghosts: usize,
    /// Patch-ghosts with no real twin, dropped as true reconstruction edges. A
    /// high count at the expected depth means the patch was applied too high.
    pub orphaned_ghosts: usize,
    /// Real atoms left over-coordinated after welding (the "applied too low /
    /// sub-surface" failure mode).
    pub overcoordinated_atoms: usize,
}

pub struct APIFoldData {
    pub element_type: APIDataType,
    pub accumulator_type: APIDataType,
}

/// Shape template for a `closure` / `apply` node, mirroring the Rust
/// `ClosureKind`. Fixes the arity and which pin types are free vs. fixed; the
/// four kinds are exactly the four HOF body shapes, so a closure of a given
/// kind drops into the matching HOF's `f` pin by construction. See
/// `doc/design_closures.md`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum APIClosureKind {
    /// `(T) -> U` — map-like. `type_args`: `[T, U]`.
    Map,
    /// `(T) -> Bool` — filter-like. `type_args`: `[T]`.
    Filter,
    /// `(A, T) -> A` — fold-like. `type_args`: `[A, T]`.
    Fold,
    /// `(T) -> Unit` — foreach-like. `type_args`: `[T]`.
    Foreach,
    /// Arbitrary `(p0, p1, ..., pN-1) -> R`. Arity N is derived from the
    /// parallel `param_names` length; param types live at `type_args[0..N]`,
    /// return type at `type_args[N]`.
    Custom,
}

/// Stored shape of a `closure` node: the kind plus the free type args that
/// fill it. The `closure` node expands this *inward* (zone-input pins for
/// the params, one zone-output pin, and a `Function` output pin);
/// `APIApplyData` carries the same data expanded *outward*. For preset
/// kinds `param_names` is empty (names come from the kind's static table);
/// for `Custom` it carries the authored parameter names.
pub struct APIClosureData {
    pub kind: APIClosureKind,
    pub type_args: Vec<APIDataType>,
    pub param_names: Vec<String>,
    /// Optional free-form display label shown in the closure node's title bar.
    /// `None` (or `Some("")`) renders the title bar as today (signature only).
    pub custom_label: Option<String>,
}

/// Stored shape of an `apply` node — identical data to `APIClosureData`,
/// expanded *outward* (a required `Function` input pin `f`, one ordinary arg
/// pin per parameter, and a value output of the return type).
pub struct APIApplyData {
    pub kind: APIClosureKind,
    pub type_args: Vec<APIDataType>,
    pub param_names: Vec<String>,
}

pub struct APIArrayAtData {
    pub element_type: APIDataType,
    /// Stored index used when the `index` input pin is not connected. `0`
    /// (the default) reads the first element; overridden by the wired
    /// `index` input pin when connected.
    pub index: i32,
}

pub struct APIIfData {
    /// Type of the `then` / `else` value pins and of the output pin.
    pub value_type: APIDataType,
}

pub struct APISequenceData {
    pub element_type: APIDataType,
    pub input_count: i32,
}

pub struct APILatticeVecsData {
    pub cell_length_a: f64,
    pub cell_length_b: f64,
    pub cell_length_c: f64,
    pub cell_angle_alpha: f64, // in degrees
    pub cell_angle_beta: f64,  // in degrees
    pub cell_angle_gamma: f64, // in degrees
    pub crystal_system: String,
}

pub struct APIMotifData {
    pub definition: String,    // The motif definition text
    pub name: Option<String>,  // Optional name for the motif
    pub error: Option<String>, // Optional error message from parsing
}

#[flutter_rust_bridge::frb]
pub struct APIMotifSubData {
    pub parameter_element_value_definition: String,
    pub error: Option<String>,
    pub available_parameters: Vec<APIMotifParameterInfo>,
}

/// Info about a motif parameter element available for override
#[flutter_rust_bridge::frb]
pub struct APIMotifParameterInfo {
    pub name: String,                   // e.g., "PRIMARY"
    pub default_atomic_number: i16,     // e.g., 6
    pub default_element_symbol: String, // e.g., "C"
}

#[flutter_rust_bridge::frb]
pub struct APIMaterializeData {
    pub parameter_element_value_definition: String, // The parameter element value definition text
    pub hydrogen_passivation: bool,                 // Whether to apply hydrogen passivation
    pub remove_unbonded_atoms: bool, // Whether to remove unbonded (zero-bond) atoms before passivation
    pub remove_single_bond_atoms_before_passivation: bool, // Whether to remove atoms with exactly one bond before passivation
    pub surface_reconstruction: bool, // Whether to apply surface reconstruction
    pub invert_phase: bool,
    pub error: Option<String>, // Optional error message from parsing
    pub available_parameters: Vec<APIMotifParameterInfo>, // Parameters from the connected motif (populated after eval)
}

/// Configuration for single CLI run
pub struct CliConfig {
    pub cnnd_file: String,
    pub network_name: String,
    pub output_file: String,
    /// Parameters as string key-value pairs (will be parsed based on parameter types)
    pub parameters: HashMap<String, String>,
}

/// Configuration for batch CLI runs
pub struct BatchCliConfig {
    pub cnnd_file: String,
    pub batch_file: String,
}

/// Result of an explicit Execute pass triggered from the UI on a single node.
///
/// Side-effect nodes (`export_atoms`, `print` with `execute_only`, future effect
/// nodes) only fire under `context.execute == true`; the Execute action sets
/// that flag for the duration of one evaluation pass on the targeted node.
/// `logs` carries the print entries produced by *this* pass only — earlier
/// display-pass entries already in `print_log` are not duplicated here. See
/// `doc/design_node_execution.md` (Phase 3 / Phase 4 — Centralized drain).
#[derive(Debug, Clone)]
pub struct APIExecuteResult {
    /// True when the Execute pass completed without surfacing a top-level
    /// `NetworkResult::Error` from the targeted node.
    pub ok: bool,
    /// Populated with the error message when `ok == false`; None otherwise.
    pub error: Option<String>,
    /// Print entries emitted by `print` nodes evaluated during this pass.
    /// Sliced from `StructureDesigner.print_log` so the Console panel does
    /// not double-display entries already pulled via `take_print_log`.
    pub logs: Vec<APIPrintLogEntry>,
}

/// One entry in the Console-panel print log, produced by the `print` node
/// (and any future node that surfaces text through `context.print_buffer`).
/// `timestamp_ms` is epoch milliseconds (FFI-friendly integer); the original
/// `SystemTime` lives Rust-side in `PrintLogEntry`.
#[derive(Debug, Clone)]
pub struct APIPrintLogEntry {
    pub timestamp_ms: i64,
    pub network_name: String,
    pub node_id: u64,
    pub node_label: String,
    pub text: String,
    /// True when the entry was produced under `context.execute == true`
    /// (an explicit Execute pass), false for normal display passes. The
    /// Console panel uses this to flag execute-pass entries with a marker.
    pub from_execute: bool,
}

impl From<&PrintLogEntry> for APIPrintLogEntry {
    fn from(entry: &PrintLogEntry) -> Self {
        let timestamp_ms = entry
            .timestamp
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            // Pre-epoch timestamps shouldn't occur (SystemTime::now() during
            // an eval pass), but if the clock is genuinely before 1970 we
            // surface a negative offset rather than panicking.
            .unwrap_or_else(|err| -(err.duration().as_millis() as i64));
        Self {
            timestamp_ms,
            network_name: entry.network_name.clone(),
            node_id: entry.node_id,
            node_label: entry.node_label.clone(),
            text: entry.text.clone(),
            from_execute: entry.from_execute,
        }
    }
}

/// Result of evaluating a single node via CLI
#[derive(Debug, Clone)]
pub struct APINodeEvaluationResult {
    /// The node ID that was evaluated
    pub node_id: u64,
    /// The node type name (e.g., "cuboid", "materialize")
    pub node_type_name: String,
    /// The custom name if assigned, otherwise None
    pub custom_name: Option<String>,
    /// The output data type name (e.g., "Blueprint", "HasAtoms", "Float")
    pub output_type: String,
    /// Brief display string (from to_display_string())
    pub display_string: String,
    /// Detailed string (from to_detailed_string()), only populated if verbose=true
    pub detailed_string: Option<String>,
    /// Whether the evaluation succeeded (no errors in this node's chain)
    pub success: bool,
    /// Error message if the node itself produced an error
    pub error_message: Option<String>,
}

/// Information for the factor-into-subnetwork dialog
pub struct FactorSelectionInfo {
    /// Whether the selection can be factored
    pub can_factor: bool,
    /// If not valid, the reason why
    pub invalid_reason: Option<String>,
    /// Suggested name for the new subnetwork
    pub suggested_name: String,
    /// Suggested names for the parameters (one per external input)
    pub suggested_param_names: Vec<String>,
}

/// Request to factor selection into subnetwork
pub struct FactorSelectionRequest {
    /// Name for the new subnetwork (custom node type)
    pub subnetwork_name: String,
    /// Names for the parameters (must match count of external inputs)
    pub param_names: Vec<String>,
}

/// Result of factoring attempt
pub struct FactorSelectionResult {
    /// Whether the factoring succeeded
    pub success: bool,
    /// Error message if factoring failed
    pub error: Option<String>,
    /// ID of the created custom node (if successful)
    pub new_node_id: Option<u64>,
}

/// Result of an Inline-a-Custom-Node attempt.
///
/// Modeled on [`FactorSelectionResult`] but without `new_node_id` — inlining
/// produces many nodes rather than one. See `doc/design_inline_custom_node.md`.
pub struct InlineResult {
    /// Whether the inline succeeded
    pub success: bool,
    /// Error message if the inline failed
    pub error: Option<String>,
}

/// Result of a closure ⇄ custom-network conversion attempt
/// (*Convert to Closure* / *Extract to Network*).
///
/// Modeled on [`InlineResult`]; see `doc/design_closure_network_conversion.md`.
pub struct ConversionResult {
    /// Whether the conversion succeeded
    pub success: bool,
    /// Error message if the conversion failed
    pub error: Option<String>,
}

/// Result of a Promote-to-Parameter attempt.
pub struct APIPromoteToParameterResult {
    pub success: bool,
    pub error: Option<String>,
    /// ID of the newly created parameter node (if successful)
    pub new_node_id: Option<u64>,
}

/// Result of applying text format edits to the active network.
pub struct APITextEditResult {
    pub success: bool,
    pub nodes_created: Vec<String>,
    pub nodes_updated: Vec<String>,
    pub nodes_deleted: Vec<String>,
    pub connections_made: Vec<String>,
    pub errors: Vec<APITextError>,
    pub warnings: Vec<String>,
}

/// A parse or edit error with location information.
pub struct APITextError {
    pub message: String,
    pub line: i32,
    pub column: i32,
}

/// One affected network in a namespace move/rename preview: its current name,
/// the name it would take, and whether that target name collides with an
/// existing, non-affected user type.
#[derive(Debug, Clone)]
pub struct APINamespaceRenameItem {
    pub old_name: String,
    pub new_name: String,
    pub conflict: bool,
}

/// Live preview of a namespace move/rename, driving the move-namespace dialog.
/// `applicable` mirrors `NamespaceRenamePlan::is_applicable` exactly — when
/// false the dialog disables its commit button and shows why (empty / invalid
/// / conflicts). An empty target prefix promotes the contents to the root.
#[derive(Debug, Clone)]
pub struct APINamespaceRenamePreview {
    pub items: Vec<APINamespaceRenameItem>,
    /// True when no network matches the source prefix (nothing to move).
    pub is_empty: bool,
    /// True when at least one resulting name is not a valid user name.
    pub has_invalid_names: bool,
    /// True when at least one resulting name collides with an existing,
    /// non-affected user type.
    pub has_conflicts: bool,
    /// True when the rename can be applied as-is (non-empty, all names valid,
    /// no conflicts). Equals `rename_namespace`'s acceptance condition.
    pub applicable: bool,
}

impl From<crate::structure_designer::structure_designer::NamespaceRenamePlan>
    for APINamespaceRenamePreview
{
    fn from(plan: crate::structure_designer::structure_designer::NamespaceRenamePlan) -> Self {
        let is_empty = plan.is_empty();
        let has_conflicts = plan.has_conflicts();
        let has_invalid_names = !plan.valid_names;
        let applicable = plan.is_applicable();
        Self {
            items: plan
                .items
                .into_iter()
                .map(|item| APINamespaceRenameItem {
                    old_name: item.old_name,
                    new_name: item.new_name,
                    conflict: item.conflict,
                })
                .collect(),
            is_empty,
            has_invalid_names,
            has_conflicts,
            applicable,
        }
    }
}

/// A candidate node in a viewport pick disambiguation.
pub struct APICandidateNode {
    pub node_id: u64,
    pub node_name: String,
}

/// Result of a viewport pick operation (click-to-activate).
pub enum APIViewportPickResult {
    /// The closest hit belongs to the already-active node — proceed with normal click handling.
    ActiveNodeHit,
    /// Unambiguous hit on a non-active node — activate it.
    ActivateNode { node_id: u64, node_name: String },
    /// Multiple non-active nodes overlap at the click point — show disambiguation popup.
    Disambiguation { candidates: Vec<APICandidateNode> },
    /// Ray missed everything — proceed with normal click handling.
    NoHit,
}

/// The editable subset of `TextValue` that the custom-node property panel can
/// render. Mirrors the "simple" data types. FRB data-carrying enum — same
/// shape as the existing `APIMeasurement` enum.
///
/// Named for the `CustomNodeData.literal_values` map it is read from / written
/// to — deliberately *not* `APITextValue`, since the "text" in the core
/// `TextValue` type refers to the node-network text format, which is unrelated
/// to this panel. See `doc/design_custom_node_property_panel.md`.
pub enum APILiteralValue {
    Bool(bool),
    Int(i32),
    Float(f64),
    Str(String),
    IVec2(APIIVec2),
    IVec3(APIIVec3),
    Vec2(APIVec2),
    Vec3(APIVec3),
    /// Row-major 3x3, matching `TextValue::IMat3`.
    IMat3(Vec<Vec<i32>>),
    /// Row-major 3x3, matching `TextValue::Mat3`.
    Mat3(Vec<Vec<f64>>),
}

/// Dedicated enum so the Flutter widget switches directly without parsing pin
/// type strings or depending on `APIDataTypeBase`'s coverage.
pub enum APISimpleParamType {
    Bool,
    Int,
    Float,
    Str,
    IVec2,
    IVec3,
    Vec2,
    Vec3,
    IMat3,
    Mat3,
}

/// FRB mirror of `FieldEditorHint` — which widget a generic literal editor
/// should render for a record-def field. **Purely cosmetic**: a hint never
/// gates a wire, converts a value, or changes what a node emits; the row's
/// `data_type` alone governs the value that crosses the FFI back. A hint whose
/// widget is not implemented is ignored and the row falls back to the plain
/// type widget. See `doc/design_array_node_and_field_hints.md` Part A.
pub enum APIFieldEditorHint {
    /// `Int` rows: atomic-number element dropdown.
    Element,
    /// `Vec3` rows: 0–1 RGB color editor.
    Color,
    /// `Str` rows: fixed-choice dropdown over these entries.
    Enum(Vec<String>),
    /// `Float` / `Int` rows: slider between the bounds. UI-only clamping.
    Range { min: f64, max: f64 },
}

/// One editable input pin of a node that supports inline literal editing,
/// surfaced for the auto-generated property panel. Used by both
/// `CustomNodeEditor` (custom-node parameters) and `RecordConstructEditor`
/// (record-construct fields).
///
/// `default_value` carries a uniform semantic across both call sites: it is
/// `Some(..)` iff a resolvable default layer exists behind the pin. For
/// custom nodes this is the value produced by the parameter node's `default`
/// input pin; for `record_construct` it is always `None` (no default layer).
pub struct APILiteralField {
    pub name: String,
    pub data_type: APISimpleParamType,
    /// The literal currently stored in the owning node's `literal_values`
    /// map, if any AND it still matches `data_type`. `None` ⇒ render the
    /// placeholder.
    pub stored_value: Option<APILiteralValue>,
    /// The value of the default layer (if any), used as the field
    /// placeholder. `None` when there is no default layer (record_construct)
    /// or the default pin is unconnected / evaluation fails / yields a
    /// non-simple type.
    pub default_value: Option<APILiteralValue>,
    /// True when the parent pin has a wire connected. When true the row renders
    /// disabled.
    pub is_wired: bool,
    /// Cosmetic widget annotation from the record def behind this row, if any.
    /// Always `None` for `CustomNodeEditor`'s parameter rows — there is no
    /// record def behind them.
    pub hint: Option<APIFieldEditorHint>,
}
