use super::super::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure::BondReference;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/*
 * A selection command. If unselect == true it unselects otherwise it selects
 * the given atoms and bonds.
 */
#[derive(Debug, Serialize, Deserialize)]
pub struct SelectCommand {
  pub atom_ids: Vec<u64>,
  pub bond_references: Vec<BondReference>,
  pub unselect: bool, // whether this is an unselect or select command

  // undo information
  pub original_atom_selection: HashMap<u64, bool>,
  pub original_bond_selection: HashMap<BondReference, bool>,
}

impl SelectCommand {
  pub fn new(atom_ids: Vec<u64>, bond_references: Vec<BondReference>, unselect: bool) -> Self {
      Self {
        atom_ids,
        bond_references,
        unselect,
        original_atom_selection: HashMap::new(),
        original_bond_selection: HashMap::new()
      }
  }
}

impl EditAtomCommand for SelectCommand {
  fn execute(&mut self, model: &mut AtomicStructure, is_redo: bool) {
    if !is_redo {
      // Gather the original selection information for the specified atoms and bonds
      for atom_id in self.atom_ids.iter() {
        if let Some(atom) = model.get_atom(*atom_id) {
          self.original_atom_selection.insert(*atom_id, atom.selected);
        }
      }
      for bond_reference in self.bond_references.iter() {
        if let Some(bond) = model.get_bond_by_reference(bond_reference) {
          self.original_bond_selection.insert(bond_reference.clone(), bond.selected);
        }
      }
    }
    model.select(&self.atom_ids, &self.bond_references, self.unselect);
  }

  fn undo(&mut self, model: &mut AtomicStructure) {
    model.select_by_maps(&self.original_atom_selection, &self.original_bond_selection);
  }
}
