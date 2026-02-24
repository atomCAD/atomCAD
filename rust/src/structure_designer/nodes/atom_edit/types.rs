use crate::api::common_api_types::SelectModifier;
use crate::crystolecule::atomic_structure::{AtomicStructure, BondReference};
use crate::crystolecule::atomic_structure_diff::{DiffProvenance, DiffStats};
use glam::f64::DVec2;
use glam::f64::DVec3;
use std::collections::HashSet;

use crate::util::transform::Transform;

/// Default positional matching tolerance in Angstroms.
pub const DEFAULT_TOLERANCE: f64 = 0.1;

/// Pixel threshold (logical pixels) distinguishing click from drag.
pub(super) const DRAG_THRESHOLD: f64 = 5.0;

/// Interaction state machine for the Default tool.
/// Tracks the current mouse interaction from down → move → up.
#[derive(Debug)]
pub enum DefaultToolInteractionState {
    Idle,
    PendingAtom {
        hit_atom_id: u32,
        is_diff_view: bool,
        was_selected: bool,
        mouse_down_screen: DVec2,
        select_modifier: SelectModifier,
    },
    PendingBond {
        bond_reference: BondReference,
        mouse_down_screen: DVec2,
    },
    PendingMarquee {
        mouse_down_screen: DVec2,
    },
    ScreenPlaneDragging {
        /// Camera forward direction (plane normal).
        plane_normal: DVec3,
        /// A point on the constraint plane (selection centroid at drag start).
        plane_point: DVec3,
        /// World position on the constraint plane at drag start.
        start_world_pos: DVec3,
        /// World position on the constraint plane at the last frame.
        last_world_pos: DVec3,
    },
    MarqueeActive {
        start_screen: DVec2,
        current_screen: DVec2,
    },
}

impl Default for DefaultToolInteractionState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct DefaultToolState {
    pub replacement_atomic_number: i16,
    pub interaction_state: DefaultToolInteractionState,
    /// When true, the selection gadget (XYZ axes) is visible.
    /// Off by default to avoid occluding selected atoms.
    pub show_gadget: bool,
}

#[derive(Debug)]
pub enum AddAtomToolState {
    Idle {
        atomic_number: i16,
    },
    GuidedPlacement {
        atomic_number: i16,
        anchor_atom_id: u32,
        guide_dots: Vec<crate::crystolecule::guided_placement::GuideDot>,
        bond_distance: f64,
        /// If true, the bond created should be BOND_DATIVE instead of BOND_SINGLE.
        is_dative_bond: bool,
    },
    /// Free sphere placement: bare atom with no bonds, user clicks anywhere on sphere.
    GuidedFreeSphere {
        atomic_number: i16,
        anchor_atom_id: u32,
        center: DVec3,
        radius: f64,
        /// Cursor-tracked preview position on the sphere surface.
        preview_position: Option<DVec3>,
        /// If true, the bond created should be BOND_DATIVE instead of BOND_SINGLE.
        is_dative_bond: bool,
    },
    /// Free ring placement: ring without reference (sp3 case 1 or sp2 case 1).
    /// Guide dots rotate together on a cone ring as the user moves the cursor.
    GuidedFreeRing {
        atomic_number: i16,
        anchor_atom_id: u32,
        ring_center: DVec3,
        ring_normal: DVec3,
        ring_radius: f64,
        bond_distance: f64,
        anchor_pos: DVec3,
        /// Number of preview dots to show (3 for sp3, 2 for sp2).
        num_ring_dots: usize,
        /// Cursor-tracked preview positions on the ring.
        preview_positions: Option<Vec<DVec3>>,
        /// If true, the bond created should be BOND_DATIVE instead of BOND_SINGLE.
        is_dative_bond: bool,
    },
}

impl AddAtomToolState {
    pub fn atomic_number(&self) -> i16 {
        match self {
            AddAtomToolState::Idle { atomic_number }
            | AddAtomToolState::GuidedPlacement { atomic_number, .. }
            | AddAtomToolState::GuidedFreeSphere { atomic_number, .. }
            | AddAtomToolState::GuidedFreeRing { atomic_number, .. } => *atomic_number,
        }
    }
}

/// Interaction state machine for the AddBond tool.
/// Tracks the current pointer interaction from down → move → up.
#[derive(Debug)]
pub enum AddBondInteractionState {
    Idle,
    Pending {
        hit_atom_id: u32,
        is_diff_view: bool,
        mouse_down_screen: DVec2,
    },
    Dragging {
        source_atom_id: u32,
        preview_target: Option<u32>,
    },
}

impl Default for AddBondInteractionState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug)]
pub struct AddBondToolState {
    /// Bond order to use when creating bonds (1-7, default: BOND_SINGLE).
    pub bond_order: u8,
    /// Drag interaction state machine.
    pub interaction_state: AddBondInteractionState,
    /// Legacy: used by the two-click workflow. Will be removed when drag-to-bond is implemented.
    pub last_atom_id: Option<u32>,
}

#[derive(Debug)]
pub enum AtomEditTool {
    Default(DefaultToolState),
    AddAtom(AddAtomToolState),
    AddBond(AddBondToolState),
}

// --- Selection model ---

/// Provenance-based selection state for atom_edit.
///
/// Selection is stored by provenance (base/diff atom IDs) rather than result atom IDs.
/// This makes selection stable across re-evaluations, since base IDs are immutable
/// and diff IDs are under our control.
#[derive(Debug, Clone, Default)]
pub struct AtomEditSelection {
    /// Base atoms selected (by base atom ID — stable, input doesn't change during editing)
    pub selected_base_atoms: HashSet<u32>,
    /// Diff atoms selected (by diff atom ID — stable, we control the diff)
    pub selected_diff_atoms: HashSet<u32>,
    /// Bond selection in result space (cleared on any diff mutation)
    pub selected_bonds: HashSet<BondReference>,
    /// Cached selection transform (recalculated after selection changes)
    pub selection_transform: Option<Transform>,
}

impl AtomEditSelection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.selected_base_atoms.is_empty()
            && self.selected_diff_atoms.is_empty()
            && self.selected_bonds.is_empty()
    }

    pub fn has_selected_atoms(&self) -> bool {
        !self.selected_base_atoms.is_empty() || !self.selected_diff_atoms.is_empty()
    }

    pub fn clear(&mut self) {
        self.selected_base_atoms.clear();
        self.selected_diff_atoms.clear();
        self.selected_bonds.clear();
        self.selection_transform = None;
    }

    /// Clear bond selection (called when diff is mutated)
    pub fn clear_bonds(&mut self) {
        self.selected_bonds.clear();
    }
}

// --- Eval cache ---

/// Evaluation cache for the atom_edit node.
///
/// Stores the provenance and stats computed during the most recent `apply_diff()` call.
/// Retrieved by interaction functions via `structure_designer.get_selected_node_eval_cache()`.
#[derive(Debug, Clone)]
pub struct AtomEditEvalCache {
    pub provenance: DiffProvenance,
    pub stats: DiffStats,
}

// --- Bond deletion info ---

/// Info needed to delete a bond: diff atom IDs for both endpoints
/// (None if the atom needs an identity entry), plus the atom info for identity entries.
#[derive(Debug, Clone)]
pub struct BondDeletionInfo {
    pub diff_id_a: Option<u32>,
    pub diff_id_b: Option<u32>,
    pub identity_a: Option<(i16, DVec3)>,
    pub identity_b: Option<(i16, DVec3)>,
}

// --- Diff atom classification ---

/// Classification of a diff atom based on its properties (no provenance needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffAtomKind {
    /// Atom with atomic_number == 0 (marks a base atom for deletion)
    DeleteMarker,
    /// Atom with an anchor position (moved or replaced base atom)
    MatchedBase,
    /// Normal atom without anchor (pure addition to the structure)
    PureAddition,
}

/// Classify a diff atom by inspecting the diff structure directly.
pub fn classify_diff_atom(diff: &AtomicStructure, diff_id: u32) -> DiffAtomKind {
    if let Some(atom) = diff.get_atom(diff_id) {
        if atom.is_delete_marker() {
            DiffAtomKind::DeleteMarker
        } else if diff.has_anchor_position(diff_id) {
            DiffAtomKind::MatchedBase
        } else {
            DiffAtomKind::PureAddition
        }
    } else {
        // Atom not found — treat as removable
        DiffAtomKind::PureAddition
    }
}

// --- Pending click info ---

/// Owned snapshot of pending state info needed for click-select in pointer_up.
pub(super) enum PendingClickInfo {
    Atom {
        atom_id: u32,
        is_diff_view: bool,
        was_selected: bool,
    },
    Bond {
        bond_reference: BondReference,
    },
    Empty,
    Marquee {
        start_screen: DVec2,
        end_screen: DVec2,
    },
    DragCompleted,
}
