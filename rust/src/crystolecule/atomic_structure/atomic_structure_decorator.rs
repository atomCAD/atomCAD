use crate::crystolecule::atomic_structure::bond_reference::BondReference;
use crate::crystolecule::guided_placement::GuideDot;
use crate::util::transform::Transform;
use glam::IVec3;
use glam::f64::DVec3;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub enum AtomDisplayState {
    Normal,
    Marked,
    SecondaryMarked,
}

/// Visual data for rendering guided placement guide dots and anchor arrows.
#[derive(Debug, Clone)]
pub struct GuidePlacementVisuals {
    pub anchor_pos: DVec3,
    pub guide_dots: Vec<GuideDot>,
    /// If set, render a wireframe sphere for free sphere placement (case 0).
    pub wireframe_sphere: Option<WireframeSphereVisuals>,
    /// If set, render a wireframe ring for free ring placement (sp3 case 1, no dihedral ref).
    pub wireframe_ring: Option<WireframeRingVisuals>,
    /// Per-dot merge flag: true if this dot overlaps an existing atom (same or different element).
    /// Parallel to `guide_dots`. Empty means no merge info available.
    pub merge_dot_flags: Vec<bool>,
    /// Per-dot merge target atom ID (in result structure). Parallel to `guide_dots`.
    /// `Some(id)` if this dot overlaps an existing atom; used to apply rim highlight.
    pub merge_target_atom_ids: Vec<Option<u32>>,
}

/// Visual data for a wireframe sphere (free sphere placement mode).
#[derive(Debug, Clone)]
pub struct WireframeSphereVisuals {
    pub center: DVec3,
    pub radius: f64,
    /// If set, a preview guide dot tracks the cursor on the sphere surface.
    pub preview_position: Option<DVec3>,
}

/// Visual data for a wireframe ring (sp3 case 1 free ring placement mode).
#[derive(Debug, Clone)]
pub struct WireframeRingVisuals {
    pub center: DVec3,
    pub normal: DVec3,
    pub radius: f64,
}

#[derive(Debug, Clone)]
pub struct AtomicStructureDecorator {
    pub atom_display_states: FxHashMap<u32, AtomDisplayState>,
    selected_bonds: std::collections::HashSet<BondReference>,
    pub from_selected_node: bool,
    pub selection_transform: Option<Transform>,
    /// Transient rendering hint: when true and the structure is a diff, render anchor arrows
    pub show_anchor_arrows: bool,
    /// Transient rendering hint: guide placement visuals for the Add Atom tool
    pub guide_placement_visuals: Option<GuidePlacementVisuals>,
    /// Display name overrides by atomic number. When present, hover tooltips
    /// and other UI consumers use these instead of the standard element names.
    /// Used by motif_edit to label parameter element atoms with user-defined names.
    pub element_name_overrides: FxHashMap<i16, String>,
    /// Ghost atom metadata: maps ghost atom ID → (primary_atom_id, cell_offset).
    /// Used by motif_edit to resolve ghost atom hits back to primary atoms
    /// for cross-cell bond creation.
    pub ghost_atom_metadata: FxHashMap<u32, (u32, IVec3)>,
}

impl Default for AtomicStructureDecorator {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicStructureDecorator {
    pub fn new() -> Self {
        Self {
            atom_display_states: FxHashMap::default(),
            selected_bonds: std::collections::HashSet::new(),
            from_selected_node: false,
            selection_transform: None,
            show_anchor_arrows: false,
            guide_placement_visuals: None,
            element_name_overrides: FxHashMap::default(),
            ghost_atom_metadata: FxHashMap::default(),
        }
    }

    // Atom display state methods
    pub fn set_atom_display_state(&mut self, atom_id: u32, state: AtomDisplayState) {
        self.atom_display_states.insert(atom_id, state);
    }

    pub fn get_atom_display_state(&self, atom_id: u32) -> AtomDisplayState {
        self.atom_display_states
            .get(&atom_id)
            .cloned()
            .unwrap_or(AtomDisplayState::Normal)
    }

    // Bond selection methods
    pub fn is_bond_selected(&self, bond_ref: &BondReference) -> bool {
        self.selected_bonds.contains(bond_ref)
    }

    pub fn select_bond(&mut self, bond_ref: &BondReference) {
        self.selected_bonds.insert(bond_ref.clone());
    }

    pub fn deselect_bond(&mut self, bond_ref: &BondReference) {
        self.selected_bonds.remove(bond_ref);
    }

    pub fn clear_bond_selection(&mut self) {
        self.selected_bonds.clear();
    }

    pub fn has_selected_bonds(&self) -> bool {
        !self.selected_bonds.is_empty()
    }

    pub fn iter_selected_bonds(&self) -> impl Iterator<Item = &BondReference> {
        self.selected_bonds.iter()
    }
}
