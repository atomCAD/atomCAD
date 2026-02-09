use crate::crystolecule::atomic_structure::bond_reference::BondReference;
use crate::util::transform::Transform;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub enum AtomDisplayState {
    Normal,
    Marked,
    SecondaryMarked,
}

#[derive(Debug, Clone)]
pub struct AtomicStructureDecorator {
    pub atom_display_states: FxHashMap<u32, AtomDisplayState>,
    selected_bonds: std::collections::HashSet<BondReference>,
    pub from_selected_node: bool,
    pub selection_transform: Option<Transform>,
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
