use super::super::command::Command;
use super::super::model::Model;
use std::collections::HashMap;

/*
 * A selection command. If unselect == true it unselects otherwise it selects
 * the given atoms and bonds.
 */
pub struct SelectCommand {
  pub atom_ids: Vec<u64>,
  pub bond_ids: Vec<u64>,
  pub unselect: bool, // whether this is an unselect or select command

  // undo information
  pub original_atom_selection: HashMap<u64, bool>,
  pub original_bond_selection: HashMap<u64, bool>,
}

impl SelectCommand {
  pub fn new(atom_ids: Vec<u64>, bond_ids: Vec<u64>, unselect: bool) -> Self {
      Self { atom_ids, bond_ids, unselect, original_atom_selection: HashMap::new(), original_bond_selection: HashMap::new() }
  }
}

impl Command for SelectCommand {
  fn execute(&mut self, model: &mut Model, is_redo: bool) {
    if !is_redo {
      // Gather the original selection information for the specified atoms and bonds
      for atom_id in self.atom_ids.iter() {
        if let Some(atom) = model.get_atom(*atom_id) {
          self.original_atom_selection.insert(*atom_id, atom.selected);
        }
      }
      for bond_id in self.bond_ids.iter() {
        if let Some(bond) = model.get_bond(*bond_id) {
          self.original_bond_selection.insert(*bond_id, bond.selected);
        }
      }
    }
    model.select(&self.atom_ids, &self.bond_ids, self.unselect);
  }

  fn undo(&mut self, model: &mut Model) {
    model.select_by_maps(&self.original_atom_selection, &self.original_bond_selection);
  }
}
