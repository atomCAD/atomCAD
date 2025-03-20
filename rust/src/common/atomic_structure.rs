use crate::util::transform::Transform;
use glam::f64::DVec3;
use glam::f64::DQuat;
use std::collections::HashMap;
use std::collections::HashSet;

// Bigger than most realistically possible bonds, so a neighbouring atom will be in the same cell
// or in a neighbouring cell most of the time. This is important for performance reasons.
const ATOM_GRID_CELL_SIZE: f64 = 4.0;

#[derive(Clone)]
pub struct Bond {
  pub id: u64,
  pub atom_id1: u64,
  pub atom_id2: u64,
  pub multiplicity: i32,
  pub selected: bool,
}

#[derive(Clone)]
pub struct Atom {
  pub id: u64,
  pub atomic_number: i32,
  pub position: DVec3,
  pub bond_ids: Vec<u64>,
  pub selected: bool,
  pub cluster_id: u64,
}

#[derive(Clone)]
pub struct Cluster {
  pub id: u64,
  pub name: String,
  pub atom_ids: HashSet<u64>,
}

#[derive(Clone)]
pub struct AtomicStructure {
  pub frame_transform: Transform,
  pub next_id: u64,
  pub atoms: HashMap<u64, Atom>,
  // Sparse grid of atoms
  pub grid: HashMap<(i32, i32, i32), Vec<u64>>,
  pub bonds: HashMap<u64, Bond>,
  pub dirty_atom_ids: HashSet<u64>,
  pub clusters: HashMap<u64, Cluster>,
}

impl AtomicStructure {

  pub fn new() -> Self {
    let mut ret = Self {
      frame_transform: Transform::default(),
      next_id: 1,
      atoms: HashMap::new(),
      grid: HashMap::new(),
      bonds: HashMap::new(),
      dirty_atom_ids: HashSet::new(),
      clusters: HashMap::new(),
    };
    ret.add_cluster("default");
    ret
  }

  pub fn get_num_of_atoms(&self) -> usize {
    self.atoms.len()
  }

  pub fn get_cell_for_pos(&self, pos: &DVec3) -> (i32, i32, i32) {
    let cell = (pos / ATOM_GRID_CELL_SIZE).trunc().as_ivec3();
    (cell.x, cell.y, cell.z)
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

  pub fn add_cluster(&mut self, name: &str) -> u64 {
    let id = self.obtain_next_id();
    self.add_cluster_with_id(id, name);
    id
  }

  pub fn add_cluster_with_id(&mut self, id: u64, name: &str) {
    self.clusters.insert(id, Cluster {
      id,
      name: name.to_string(),
      atom_ids: HashSet::new(),
    });
  }

  pub fn add_atom(&mut self, atomic_number: i32, position: DVec3, cluster_id: u64) -> u64 {
    let id = self.obtain_next_id();
    self.add_atom_with_id(id, atomic_number, position, cluster_id);
    id
  }

  pub fn add_atom_with_id(&mut self, id: u64, atomic_number: i32, position: DVec3, cluster_id: u64) {

    self.atoms.insert(id, Atom {
      id,
      atomic_number,
      position,
      bond_ids: Vec::new(),
      selected: false,
      cluster_id,
    });

    self.add_atom_to_grid(id, &position);

    // Add atom ID to the cluster's atom_ids HashSet if the cluster exists
    if let Some(cluster) = self.clusters.get_mut(&cluster_id) {
      cluster.atom_ids.insert(id);
    }
    
    self.make_atom_dirty(id);
  }

  // Right now it can only delete atoms without bonds
  // Delete the bonds before calling this function
  // TODO: delete bonds in the method first.
  pub fn delete_atom(&mut self, id: u64) {
    let pos = if let Some(atom) = self.atoms.get(&id) {
      // Remove atom ID from its cluster's atom_ids HashSet if the cluster exists
      if let Some(cluster) = self.clusters.get_mut(&atom.cluster_id) {
        cluster.atom_ids.remove(&id);
      }
      Some(atom.position)
    } else {
      None
    };
    
    // Remove from the grid cell
    if let Some(pos) = pos {
      self.remove_atom_from_grid(id, &pos);
    }

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

  pub fn find_pivot_point(&self, ray_start: &DVec3, ray_dir: &DVec3) -> DVec3 {
    // Find closest atom to ray.
    // Linear search for now. We will use space partitioning later.
    
    let mut closest_distance_squared = f64::MAX;
    let mut closest_atom_position = DVec3::ZERO;

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
    if closest_distance_squared == f64::MAX {
        return DVec3::new(0.0, 0.0, 0.0);
    }

    return closest_atom_position;
  }

  fn remove_from_bond_arr(&mut self, atom_id: u64, bond_id: u64) {
    let bond_ids = &mut self.atoms.get_mut(&atom_id).unwrap().bond_ids;
    if let Some(pos) = bond_ids.iter().position(|&x| x == bond_id) {
        bond_ids.swap_remove(pos);
    }
  }
  
  pub fn transform(&mut self, rotation: &DQuat, translation: &DVec3) {
    // First, collect all atom IDs that will be transformed
    let atom_ids: Vec<u64> = self.atoms.keys().cloned().collect();
    
    // Transform all atom positions and update grid positions
    for atom_id in &atom_ids {
      let positions = if let Some(atom) = self.atoms.get_mut(atom_id) {
        let old_position = atom.position;
        atom.position = rotation.mul_vec3(atom.position) + *translation;
        Some((old_position, atom.position))
      } else {
        None
      };
      // Update grid position
      if let Some((old_position, new_position)) = positions {
        self.remove_atom_from_grid(*atom_id, &old_position);
        self.add_atom_to_grid(*atom_id, &new_position);
      }
    }

    // Then mark all atoms as dirty in a separate loop
    for atom_id in atom_ids {
      self.make_atom_dirty(atom_id);
    }
  }

  pub fn remove_lone_atoms(&mut self) {
    let lone_atom_ids: Vec<u64> = self.atoms
      .iter()
      .filter(|(_, atom)| atom.bond_ids.is_empty())
      .map(|(id, _)| *id)
      .collect();

    for atom_id in lone_atom_ids {
      self.delete_atom(atom_id);
    }
  }

  // Helper method to add an atom to the grid at a specific position
  fn add_atom_to_grid(&mut self, atom_id: u64, position: &DVec3) {
    let cell = self.get_cell_for_pos(position);
    self.grid.entry(cell).or_insert_with(Vec::new).push(atom_id);
  }

  // Helper method to remove an atom from the grid at a specific position
  fn remove_atom_from_grid(&mut self, atom_id: u64, position: &DVec3) {
    let cell = self.get_cell_for_pos(position);
    if let Some(cell_atoms) = self.grid.get_mut(&cell) {
      cell_atoms.retain(|&x| x != atom_id);
    }
  }

  /// Returns a vector of atom IDs that are within the specified radius of the given position.
  /// 
  /// # Arguments
  /// 
  /// * `position` - The center position to search around
  /// * `radius` - The maximum distance from the position to include atoms
  /// 
  /// # Returns
  /// 
  /// A vector of atom IDs that are within the radius of the position
  pub fn get_atoms_in_radius(&self, position: &DVec3, radius: f64) -> Vec<u64> {
    let mut result = Vec::new();
    
    // Calculate how many cells we need to check in each direction
    // We add 1 to ensure we cover the boundary cases
    let cell_radius = (radius / ATOM_GRID_CELL_SIZE).ceil() as i32;
    
    // Get the cell coordinates for the center position
    let center_cell = self.get_cell_for_pos(position);
    
    // Iterate through all relevant cells
    for dx in -cell_radius..=cell_radius {
      for dy in -cell_radius..=cell_radius {
        for dz in -cell_radius..=cell_radius {
          // Calculate the coordinates of the current cell
          let current_cell = (
            center_cell.0 + dx,
            center_cell.1 + dy,
            center_cell.2 + dz
          );
          
          // Check if this cell exists in our sparse grid
          if let Some(cell_atoms) = self.grid.get(&current_cell) {
            // For each atom in this cell, check if it's within the radius
            for &atom_id in cell_atoms {
              if let Some(atom) = self.atoms.get(&atom_id) {
                // Calculate the squared distance (more efficient than using sqrt)
                let squared_distance = position.distance_squared(atom.position);
                
                // If the atom is within the radius, add its ID to the result
                if squared_distance <= radius * radius {
                  result.push(atom_id);
                }
              }
            }
          }
        }
      }
    }
    
    result
  }
}
