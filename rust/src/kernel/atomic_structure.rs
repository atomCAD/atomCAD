use glam::f32::Vec3 as Vec3;
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

pub struct AtomicStructure {
  pub next_id: u64,
  pub atoms: HashMap<u64, Atom>,
  pub bonds: HashMap<u64, Bond>,
  pub dirty_atom_ids: HashSet<u64>,
}

impl AtomicStructure {

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

  pub fn add_atom(&mut self, atomic_number: i32, position: Vec3) -> u64 {
    let id = self.obtain_next_id();
    self.add_atom_with_id(id, atomic_number, position);
    id
  }

  pub fn add_atom_with_id(&mut self, id: u64, atomic_number: i32, position: Vec3) {
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

  pub fn add_bond(&mut self, atom_id1: u64, atom_id2: u64, multiplicity: i32) -> u64 {
    let id = self.obtain_next_id();
    self.add_bond_with_id(id, atom_id1, atom_id2, multiplicity);
    id
  }

  // Right now this can only be called if no bond exist between the two atoms but both atoms exist
  // TODO: handle the case when a bond already exist
  pub fn add_bond_with_id(&mut self, id: u64, atom_id1: u64, atom_id2: u64, multiplicity: i32) {
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

  pub fn find_pivot_point(&self, ray_start: &Vec3, ray_dir: &Vec3) -> Vec3 {
    // Find closest atom to ray.
    // Linear search for now. We will use space partitioning later.
    
    let mut closest_distance_squared = f32::MAX;
    let mut closest_atom_position = Vec3::ZERO;

    for atom in self.atoms.values() {
        // Vector from ray start to atom center.
        let to_atom = atom.position - ray_start;

        // Project `to_atom` onto `ray_dir` to get the closest point on the ray.
        let projection_length = to_atom.dot(*ray_dir);

        // If the projection length is negative, the closest point on the ray is behind the ray start.
        if projection_length < 0.0 {
            continue;
        }

        let closest_point = ray_start + ray_dir * projection_length;

        // Compute squared distance from the atom center to the closest point on the ray.
        let distance_squared = (atom.position - closest_point).length_squared();

        if distance_squared < closest_distance_squared {
            closest_distance_squared = distance_squared;
            closest_atom_position = atom.position;
        }
    }

    // If no atom was found, return the ray origin.
    if closest_distance_squared == f32::MAX {
        return Vec3::new(0.0, 0.0, 0.0);
    }

    return closest_atom_position;
  }

  fn remove_from_bond_arr(&mut self, atom_id: u64, bond_id: u64) {
    let bond_ids = &mut self.atoms.get_mut(&atom_id).unwrap().bond_ids;
    if let Some(pos) = bond_ids.iter().position(|&x| x == bond_id) {
        bond_ids.swap_remove(pos);
    }
  }

}
