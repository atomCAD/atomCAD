use crate::structure_designer::nodes::edit_atom::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use serde::{Serialize, Deserialize};

/*
 * Replace command: replaces all selected atoms with a specified atomic number
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceCommand {
    pub atomic_number: i32,
}

impl ReplaceCommand {
    pub fn new(atomic_number: i32) -> Self {
        Self {
            atomic_number,
        }
    }
}

impl EditAtomCommand for ReplaceCommand {
    fn execute(&self, model: &mut AtomicStructure) {
        // Collect all selected atom IDs
        let selected_atom_ids: Vec<u64> = model.atoms
            .iter()
            .filter(|(_, atom)| atom.selected)
            .map(|(id, _)| *id)
            .collect();

        // Replace the atomic number of all selected atoms
        for atom_id in selected_atom_ids {
            model.replace_atom(atom_id, self.atomic_number);
        }
    }

    fn clone_box(&self) -> Box<dyn EditAtomCommand> {
        Box::new(self.clone())
    }
}
