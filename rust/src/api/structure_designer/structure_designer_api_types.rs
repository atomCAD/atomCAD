use crate::api::common_api_types::APIIVec2;
use crate::api::common_api_types::APIIVec3;
use crate::api::common_api_types::APITransform;
use crate::api::common_api_types::APIVec2;
use crate::api::common_api_types::APIVec3;
use flutter_rust_bridge::frb;
use std::collections::HashMap;

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
    UnitCell,
    DrawingPlane,
    Geometry2D,
    Geometry,
    Atomic,
    Motif,
    Custom,
}

pub struct APIDataType {
    pub data_type_base: APIDataTypeBase,
    pub custom_data_type: Option<String>, // Not None if and only if data_type_base == APIDataTypeBase::Custom
    pub array: bool, // combined with built_in_data_type, but only redundant with custom_data_type as the outermost array is within the string in that case.
}

pub struct InputPinView {
    pub name: String,
    pub data_type: String,
    pub multi: bool,
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
    pub function_type: String,
    pub selected: bool,
    pub active: bool, // True if this is the active node (for properties panel/gadget)
    pub displayed: bool,
    pub return_node: bool,
    pub error: Option<String>,
    pub output_string: Option<String>,
    pub subtitle: Option<String>,
    // Comment node specific fields (only populated for Comment nodes)
    pub comment_label: Option<String>,
    pub comment_text: Option<String>,
    pub comment_width: Option<f64>,
    pub comment_height: Option<f64>,
}

pub struct WireView {
    pub source_node_id: u64,
    pub source_output_pin_index: i32,
    pub dest_node_id: u64,
    pub dest_param_index: usize,
    pub selected: bool,
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

pub struct APIBoolData {
    pub value: bool,
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
}

pub struct APICuboidData {
    pub min_corner: APIIVec3,
    pub extent: APIIVec3,
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
    pub miller_index: APIIVec3,
    pub center: APIIVec3,
    pub shift: i32,
    pub subdivision: i32,
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

pub struct APILatticeMoveData {
    pub translation: APIIVec3,
    pub lattice_subdivision: i32,
}

pub struct APILatticeRotData {
    pub axis_index: Option<i32>,
    pub step: i32,
    pub pivot_point: APIIVec3,
    pub rotational_symmetries: Vec<APIRotationalSymmetry>,
    pub crystal_system: String,
}

pub struct APIAtomMoveData {
    pub translation: APIVec3,
}

pub struct APIAtomRotData {
    pub angle: f64, // In radians
    pub rot_axis: APIVec3,
    pub pivot_point: APIVec3,
}

pub struct APIAtomTransData {
    pub translation: APIVec3,
    pub rotation: APIVec3, // intrinsic euler angles in radians
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
    pub error_on_stale_entries: bool,
    pub show_gadget: bool,
    pub diff_stats: APIDiffStats,
    pub measurement: Option<APIMeasurement>,
    /// Result-space ID of the most recently selected atom (for dialog defaults).
    pub last_selected_result_atom_id: Option<u32>,
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
            Self::Geometry2D => "2D Geometry",
            Self::Geometry3D => "3D Geometry",
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

pub struct APIExportXYZData {
    pub file_name: String,
}

pub struct APICommentData {
    pub label: String,
    pub text: String,
    pub width: f64,
    pub height: f64,
}

pub struct APIAtomCutData {
    pub cut_sdf_value: f64,
    pub unit_cell_size: f64,
}

pub struct APIMapData {
    pub input_type: APIDataType,
    pub output_type: APIDataType,
}

pub struct APIUnitCellData {
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
pub struct APIAtomFillData {
    pub parameter_element_value_definition: String, // The parameter element value definition text
    pub motif_offset: APIVec3,                      // Offset in fractional lattice coordinates
    pub hydrogen_passivation: bool,                 // Whether to apply hydrogen passivation
    pub remove_single_bond_atoms_before_passivation: bool, // Whether to remove atoms with exactly one bond before passivation
    pub surface_reconstruction: bool, // Whether to apply surface reconstruction
    pub invert_phase: bool,
    pub error: Option<String>, // Optional error message from parsing
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

/// Result of evaluating a single node via CLI
#[derive(Debug, Clone)]
pub struct APINodeEvaluationResult {
    /// The node ID that was evaluated
    pub node_id: u64,
    /// The node type name (e.g., "cuboid", "atom_fill")
    pub node_type_name: String,
    /// The custom name if assigned, otherwise None
    pub custom_name: Option<String>,
    /// The output data type name (e.g., "Geometry", "Atomic", "Float")
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
