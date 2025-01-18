use glam::f32::Vec3;
use std::collections::HashMap;
use std::collections::HashSet;

pub struct Bond {
  pub id: u64,
  pub atom_id1: u64,
  pub atom_id2: u64,
  pub multiplicity: i32,
  pub selected: bool,
}

pub struct Atom {
  pub id: u64,
  pub atomic_number: i32,
  pub position: Vec3,
  pub bond_ids: Vec<u64>,
  pub selected: bool,
}

pub struct Model {
  next_id: u64,
  atoms: HashMap<u64, Atom>,
  bonds: HashMap<u64, Bond>,
  dirty_atom_ids: HashSet<u64>,
}

impl Model {

  pub fn new() -> Self {
    Self {
      next_id: 1,
      atoms: HashMap::new(),
      bonds: HashMap::new(),
      dirty_atom_ids: HashSet::new(),
    }
  }

  pub fn get_num_of_atoms(&self) -> usize {
    self.atoms.len()
  }

  pub fn get_atom(&self, atom_id: u64) -> Option<&Atom> {
    self.atoms.get(&atom_id)
  }

  pub fn get_num_of_bonds(&self) -> usize {
    self.bonds.len()
  }

  pub fn get_bond(&self, bond_id: u64) -> Option<&Bond> {
    self.bonds.get(&bond_id)
  }

  pub fn clean(&mut self) {
    self.dirty_atom_ids.clear();
  }

  fn make_atom_dirty(&mut self, atom_id: u64) {
    self.dirty_atom_ids.insert(atom_id);
  }

  pub fn obtain_next_id(&mut self) -> u64 {
    let ret = self.next_id;
    self.next_id += 1;
    return ret;
  }

  pub fn add_atom(&mut self, id: u64, atomic_number: i32, position: Vec3) {
    self.atoms.insert(id, Atom {
      id,
      atomic_number,
      position,
      bond_ids: Vec::new(),
      selected: false,
    });
    self.make_atom_dirty(id);
  }

  // Right now it can only delete atoms without bonds
  // Delete the bonds before calling this function
  // TODO: delete bonds in the method first.
  pub fn delete_atom(&mut self, id: u64) {
    self.atoms.remove(&id);
    self.make_atom_dirty(id);
  }

  // Right now this can only be called if no bond exist between the two atoms but both atoms exist
  // TODO: handle the case when a bond already exist
  pub fn add_bond(&mut self, id: u64, atom_id1: u64, atom_id2: u64, multiplicity: i32) {
    self.bonds.insert(id, Bond {
      id,
      atom_id1,
      atom_id2,
      multiplicity,
      selected: false,
    });
    self.atoms.get_mut(&atom_id1).unwrap().bond_ids.push(id);
    self.atoms.get_mut(&atom_id2).unwrap().bond_ids.push(id);
    self.make_atom_dirty(atom_id1);
    self.make_atom_dirty(atom_id2);
  }

  // Right now this can only be called if the bond exists
  pub fn delete_bond(&mut self, id: u64) {
    let (atom_id1, atom_id2) = {
      let bond = & self.bonds.get(&id).unwrap();
      (bond.atom_id1, bond.atom_id2)
    };

    self.remove_from_bond_arr(atom_id1, id);
    self.remove_from_bond_arr(atom_id2, id);

    self.make_atom_dirty(atom_id1);
    self.make_atom_dirty(atom_id2);

    self.bonds.remove(&id);
  }

  // Ignores non-existing atoms or bonds
  pub fn select(&mut self, atom_ids: &Vec<u64>, bond_ids: &Vec<u64>, unselect: bool) {
    for atom_id in atom_ids {
      if let Some(atom) = self.atoms.get_mut(atom_id) {
        atom.selected = !unselect;
      }
    }
    for bond_id in bond_ids {
      if let Some(bond) = self.bonds.get_mut(bond_id) {
        bond.selected = !unselect;
      }
    }
  }

  pub fn select_by_maps(&mut self, atom_selections: &HashMap<u64, bool>, bond_selections: &HashMap<u64, bool>) {
    for (key, value) in atom_selections {
      if let Some(atom) = self.atoms.get_mut(key) {
        atom.selected = *value;
      }
    }
    for (key, value) in bond_selections {
      if let Some(bond) = self.bonds.get_mut(key) {
        bond.selected = *value;
      }
    }
  }

  fn remove_from_bond_arr(&mut self, atom_id: u64, bond_id: u64) {
    let bond_ids = &mut self.atoms.get_mut(&atom_id).unwrap().bond_ids;
    if let Some(pos) = bond_ids.iter().position(|&x| x == bond_id) {
        bond_ids.swap_remove(pos);
    }
  }

}
