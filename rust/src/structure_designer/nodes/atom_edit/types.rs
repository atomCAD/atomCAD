use crate::api::common_api_types::SelectModifier;
use crate::crystolecule::atomic_structure::{AtomicStructure, BondReference};
use crate::crystolecule::atomic_structure_diff::{AtomSource, DiffProvenance, DiffStats};
use glam::f64::DVec2;
use glam::f64::DVec3;
use std::collections::HashSet;

use crate::util::transform::Transform;

use super::guideline::Guideline;

/// Default positional matching tolerance in Angstroms.
pub const DEFAULT_TOLERANCE: f64 = 0.1;

/// Merge tolerance for overlapping atoms during guided placement (Angstroms).
/// Set to 1.5× hydrogen covalent radius (0.31 Å) for generous matching.
pub const MERGE_TOLERANCE: f64 = 0.465;

// =============================================================================
// Parameter element constants (motif_edit mode)
// =============================================================================

/// First reserved atomic number for parameter elements.
/// PARAM_1 = -100, PARAM_2 = -101, etc.
pub const PARAM_ELEMENT_BASE: i16 = -100;

/// Maximum number of parameter elements supported.
pub const MAX_PARAM_ELEMENTS: usize = 100; // -100 to -199

/// Convert an internal parameter atomic number (-100, -101, ...)
/// to a motif parameter index (0, 1, ...).
pub fn param_atomic_number_to_index(atomic_number: i16) -> Option<usize> {
    if atomic_number <= PARAM_ELEMENT_BASE
        && atomic_number > PARAM_ELEMENT_BASE - MAX_PARAM_ELEMENTS as i16
    {
        Some((PARAM_ELEMENT_BASE - atomic_number) as usize)
    } else {
        None
    }
}

/// Convert a motif parameter index (0, 1, ...) to an internal
/// reserved atomic number (-100, -101, ...).
pub fn param_index_to_atomic_number(index: usize) -> i16 {
    PARAM_ELEMENT_BASE - index as i16
}

/// Convert an internal reserved atomic number to the motif's
/// negative atomic number convention (-1, -2, ...).
pub fn param_atomic_number_to_motif(atomic_number: i16) -> i16 {
    -(param_atomic_number_to_index(atomic_number).unwrap() as i16 + 1)
}

/// Returns true if the atomic number is a parameter element.
pub fn is_param_element(atomic_number: i16) -> bool {
    param_atomic_number_to_index(atomic_number).is_some()
}

// =============================================================================
// Cross-cell bond info
// =============================================================================

/// Metadata for a cross-cell bond: the cell offset and bond order.
/// Stored in `AtomEditData.cross_cell_bonds` keyed by `BondReference`.
/// The offset follows the normalization convention: IVec3 is the cell offset
/// of max(id1,id2) relative to min(id1,id2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossCellBondInfo {
    pub offset: glam::IVec3,
    pub bond_order: u8,
}

/// Pixel threshold (logical pixels) distinguishing click from drag.
pub(super) const DRAG_THRESHOLD: f64 = 5.0;

/// Interaction state machine for the Default tool.
/// Tracks the current mouse interaction from down → move → up.
#[derive(Debug, Default)]
pub enum DefaultToolInteractionState {
    #[default]
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

#[derive(Debug)]
pub struct DefaultToolState {
    pub interaction_state: DefaultToolInteractionState,
    /// When true, the selection gadget (XYZ axes) is visible.
    /// Off by default to avoid occluding selected atoms.
    pub show_gadget: bool,
}

/// Info about an existing atom that overlaps a guide dot position.
/// Used for merge (same element) or replace (different element) behavior.
#[derive(Debug, Clone)]
pub struct MergeTarget {
    /// Atom ID in the result structure.
    pub result_atom_id: u32,
    /// Atomic number of the existing atom.
    pub atomic_number: i16,
    /// Position of the existing atom (needed for base atom promotion in Phase 2).
    pub position: DVec3,
    /// Provenance of the existing atom (needed to resolve to diff ID).
    pub atom_source: AtomSource,
}

#[derive(Debug)]
pub enum AddAtomToolState {
    Idle,
    GuidedPlacement {
        anchor_atom_id: u32,
        guide_dots: Vec<crate::crystolecule::guided_placement::GuideDot>,
        bond_distance: f64,
        /// If true, the bond created should be BOND_DATIVE instead of BOND_SINGLE.
        is_dative_bond: bool,
        /// Per-dot merge targets: `Some(target)` if the dot overlaps an existing atom.
        merge_targets: Vec<Option<MergeTarget>>,
        /// Toolbar hybridization to store as override on the anchor atom at placement time.
        /// 0=Auto (no override written), 1=Sp3, 2=Sp2, 3=Sp1.
        toolbar_hybridization: u8,
    },
    /// Free sphere placement: bare atom with no bonds, user clicks anywhere on sphere.
    GuidedFreeSphere {
        anchor_atom_id: u32,
        center: DVec3,
        radius: f64,
        /// Cursor-tracked preview position on the sphere surface.
        preview_position: Option<DVec3>,
        /// If true, the bond created should be BOND_DATIVE instead of BOND_SINGLE.
        is_dative_bond: bool,
        /// Toolbar hybridization to store as override on the anchor atom at placement time.
        toolbar_hybridization: u8,
    },
    /// Free ring placement: ring without reference (sp3 case 1 or sp2 case 1).
    /// Guide dots rotate together on a cone ring as the user moves the cursor.
    GuidedFreeRing {
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
        /// Toolbar hybridization to store as override on the anchor atom at placement time.
        toolbar_hybridization: u8,
    },
}

/// Interaction state machine for the AddBond tool.
/// Tracks the current pointer interaction from down → move → up.
#[derive(Debug, Default)]
pub enum AddBondInteractionState {
    #[default]
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
    /// Placement guideline tool (issue #368): constrains atom placement to a
    /// transient line. Fully self-contained — the guideline lives inside the
    /// tool variant, so switching tools (replacing the variant) drops it.
    Guideline(GuidelineTool),
}

// =============================================================================
// Guideline tool (issue #368)
// =============================================================================

/// A provenance-tagged atom reference — the same stable identity the selection
/// model uses (`SelectionProvenance` + id), but stored on the Guideline tool
/// rather than in the shared `AtomEditSelection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomRef {
    /// An atom from the base (input) structure, by immutable base atom id.
    Base(u32),
    /// An atom in the diff, by diff atom id.
    Diff(u32),
}

/// Active-phase drag sub-state for the Guideline tool (Phase 2 viewport). Tracks
/// the in-progress drag mode once the click-vs-drag threshold has been crossed;
/// the resting state is `Idle`. The pre-threshold "pointer is down" bookkeeping
/// lives in `GuidelineTool::pending` (which also covers the `Define` phase, where
/// there is no drag).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum GuidelineDragState {
    #[default]
    Idle,
    /// Place mode: dragging the ghost marker along the line (sets `t`, no atom
    /// mutation).
    GhostDragging,
    /// Move mode: line-constrained drag of the picked atom (it rides the line,
    /// tracking the cursor ray's projection).
    PickedDragging,
}

/// Transient "pointer is down, not yet dragging" bookkeeping for the Guideline
/// tool (Phase 2 viewport). Captured on `pointer_down`, consumed on `pointer_up`
/// (a click) or promoted to a `GuidelineDragState` once the drag threshold is
/// crossed in `pointer_move`. Lives on `GuidelineTool` (not a phase) so it serves
/// both `Define` (click toggles/clears the defining set) and `Active` (click
/// picks/unpicks). Not serialized; reset on clone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuidelinePending {
    /// Screen position at press time (for the click-vs-drag threshold).
    pub mouse_down: DVec2,
    /// The atom under the cursor at press time, if any (`None` = pressed empty
    /// space or the marker dot).
    pub hit: Option<AtomRef>,
}

/// The two phases of the Guideline tool. See
/// `doc/atom_edit/design_atom_guidelines.md`.
#[derive(Debug)]
pub enum GuidelinePhase {
    /// No guideline yet: the user picks 1–3 atoms (the tool-local `defining`
    /// set) to define the line.
    Define { defining: Vec<AtomRef> },
    /// A frozen guideline exists. `picked` is the active atom being moved (Move
    /// mode) or `None` (Place mode, where `guideline.t` positions the ghost).
    Active {
        guideline: Guideline,
        picked: Option<AtomRef>,
        drag: GuidelineDragState,
    },
}

/// State for the Guideline tool. Not serialized, not part of undo/redo — the
/// guideline value is transient and vanishes when the tool variant is replaced.
#[derive(Debug)]
pub struct GuidelineTool {
    pub phase: GuidelinePhase,
    /// Remembered direction for the 1-atom directional line. Lives on the tool
    /// (not in `Define`) so it **persists across Clear / re-Define**: rebuilding
    /// a same-direction line from a different anchor needs no re-entry (#368).
    pub entered_direction: DVec3,
    /// Remembered along-line distance. Seeds a freshly-created line's `t` and
    /// tracks the active point, so placing at the same distance from a different
    /// anchor needs no re-entry. Also persists across Clear / re-Define (#368).
    pub remembered_t: f64,
    /// Transient pointer bookkeeping between `pointer_down` and the matching
    /// `pointer_up` / drag-threshold crossing (Phase 2 viewport). Not serialized;
    /// reset on clone.
    pub pending: Option<GuidelinePending>,
}

impl GuidelineTool {
    /// Enter the tool in `Define` with an empty defining set and no remembered
    /// settings.
    pub fn new() -> Self {
        Self {
            phase: GuidelinePhase::Define {
                defining: Vec::new(),
            },
            entered_direction: DVec3::ZERO,
            remembered_t: 0.0,
            pending: None,
        }
    }
}

impl Default for GuidelineTool {
    fn default() -> Self {
        Self::new()
    }
}

// --- Selection model ---

/// Provenance tag for selection order tracking.
/// Indicates whether a selected atom comes from the base structure or the diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionProvenance {
    Base,
    Diff,
}

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
    /// Ordered list of selected atoms, in the order they were selected.
    /// Maintained alongside the hash sets; cleared on `clear()`.
    /// Used by the modify-measurement dialog to default to the last-selected atom.
    pub selection_order: Vec<(SelectionProvenance, u32)>,
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
        self.selection_order.clear();
    }

    /// Clear bond selection (called when diff is mutated)
    pub fn clear_bonds(&mut self) {
        self.selected_bonds.clear();
    }

    /// Track a newly selected atom in the selection order.
    /// Only appends if the atom is not already tracked.
    pub fn track_selected(&mut self, provenance: SelectionProvenance, id: u32) {
        if !self
            .selection_order
            .iter()
            .any(|&(p, i)| p == provenance && i == id)
        {
            self.selection_order.push((provenance, id));
        }
    }

    /// Remove an atom from the selection order tracking.
    pub fn untrack_selected(&mut self, provenance: SelectionProvenance, id: u32) {
        self.selection_order
            .retain(|&(p, i)| !(p == provenance && i == id));
    }

    /// Replace a (provenance, id) entry in the selection order with a new one.
    /// Used when a base atom is promoted to the diff (e.g. during drag or replace).
    pub fn update_order_provenance(
        &mut self,
        old_provenance: SelectionProvenance,
        old_id: u32,
        new_provenance: SelectionProvenance,
        new_id: u32,
    ) {
        if let Some(entry) = self
            .selection_order
            .iter_mut()
            .find(|(p, i)| *p == old_provenance && *i == old_id)
        {
            *entry = (new_provenance, new_id);
        }
    }

    /// Returns the last N entries from the selection order.
    pub fn last_selected_atoms(&self, count: usize) -> Vec<(SelectionProvenance, u32)> {
        let len = self.selection_order.len();
        if count >= len {
            self.selection_order.clone()
        } else {
            self.selection_order[len - count..].to_vec()
        }
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
    /// Atom with atomic_number == -1 (bond endpoint reference, base atom unchanged)
    Unchanged,
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
        } else if atom.is_unchanged_marker() {
            DiffAtomKind::Unchanged
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
