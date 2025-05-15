use crate::util::transform::Transform;
use glam::f64::DVec3;
use glam::f64::DQuat;
use glam::IVec3;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use crate::util::hit_test_utils;
use crate::renderer::tessellator::atomic_tessellator::get_displayed_atom_radius;
use crate::renderer::tessellator::atomic_tessellator::BAS_STICK_RADIUS;
use crate::api::common_api_types::SelectModifier;
use std::hash::{Hash, Hasher};
use serde::{Serialize, Deserialize};

// Bigger than most realistically possible bonds, so a neighbouring atom will be in the same cell
// or in a neighbouring cell most of the time. This is important for performance reasons.
const ATOM_GRID_CELL_SIZE: f64 = 4.0;

/// Represents the result of a hit test against an atomic structure
#[derive(Debug, Clone, PartialEq)]
pub enum HitTestResult {
    /// An atom was hit, containing the atom's ID and the distance to the hit point
    Atom(u64, f64),
    /// A bond was hit, containing the bond's ID and the distance to the hit point
    Bond(u64, f64),
    /// Nothing was hit
    None,
}

fn apply_select_modifier(in_selected: bool, select_modifier: &SelectModifier) -> bool {
  match select_modifier {
    SelectModifier::Replace => true,
    SelectModifier::Expand => true,
    SelectModifier::Toggle => !in_selected,
  }
}

// A reference to a bond, used for commands
#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct BondReference {
  pub atom_id1: u64,
  pub atom_id2: u64,
}

impl PartialEq for BondReference {
  fn eq(&self, other: &Self) -> bool {
    // Order doesn't matter for bonds: (1,2) == (2,1)
    (self.atom_id1 == other.atom_id1 && self.atom_id2 == other.atom_id2) ||
    (self.atom_id1 == other.atom_id2 && self.atom_id2 == other.atom_id1)
  }
}

impl Eq for BondReference {}

impl Hash for BondReference {
  fn hash<H: Hasher>(&self, state: &mut H) {
    // Ensure consistent hash regardless of atom order
    // Hash the smaller ID first, then the larger ID
    let (smaller, larger) = if self.atom_id1 < self.atom_id2 {
      (self.atom_id1, self.atom_id2)
    } else {
      (self.atom_id2, self.atom_id1)
    };
    smaller.hash(state);
    larger.hash(state);
  }
}

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
  pub marked: bool,
}

#[derive(Clone)]
pub struct Cluster {
  pub id: u64,
  pub name: String,
  pub atom_ids: HashSet<u64>,
  pub selected: bool,
  pub frame_transform: Transform,
  pub frame_locked_to_atoms: bool,
}

#[derive(Clone)]
pub struct AtomicStructure {
  pub frame_transform: Transform,
  pub next_atom_id: u64,
  pub next_bond_id: u64,
  pub next_cluster_id: u64,
  pub atoms: HashMap<u64, Atom>,
  // Sparse grid of atoms
  pub grid: HashMap<(i32, i32, i32), Vec<u64>>,
  pub bonds: HashMap<u64, Bond>,
  pub dirty_atom_ids: HashSet<u64>,
  pub clusters: BTreeMap<u64, Cluster>,
  pub from_selected_node: bool,
  pub selection_transform: Option<Transform>,
  pub anchor_position: Option<IVec3>,
}

impl AtomicStructure {

  // Checks if there are any selected atoms in the structure
  pub fn has_selected_atoms(&self) -> bool {
    self.atoms.values().any(|atom| atom.selected)
  }

  // Checks if there is any selection (atoms or bonds) in the structure
  pub fn has_selection(&self) -> bool {
    self.has_selected_atoms() || self.bonds.values().any(|bond| bond.selected)
  }

  pub fn new() -> Self {
    let mut ret = Self {
      frame_transform: Transform::default(),
      next_atom_id: 1,
      next_bond_id: 1,
      next_cluster_id: 1,
      atoms: HashMap::new(),
      grid: HashMap::new(),
      bonds: HashMap::new(),
      dirty_atom_ids: HashSet::new(),
      clusters: BTreeMap::new(),
      from_selected_node: false,
      selection_transform: None,
      anchor_position: None,
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
  
  /// Creates a BondReference from a bond ID
  ///
  /// Returns None if the bond ID doesn't exist
  pub fn get_bond_reference_by_id(&self, bond_id: u64) -> Option<BondReference> {
    self.get_bond(bond_id).map(|bond| BondReference {
      atom_id1: bond.atom_id1,
      atom_id2: bond.atom_id2,
    })
  }

  // Helper method to get a bond ID from a BondReference
  fn get_bond_id_by_reference(&self, bond_reference: &BondReference) -> Option<u64> {
    // Get the first atom
    if let Some(atom) = self.get_atom(bond_reference.atom_id1) {
      // Search through its bonds for one that connects to atom_id2
      for bond_id in &atom.bond_ids {
        if let Some(bond) = self.get_bond(*bond_id) {
          if (bond.atom_id1 == bond_reference.atom_id1 && bond.atom_id2 == bond_reference.atom_id2) ||
             (bond.atom_id1 == bond_reference.atom_id2 && bond.atom_id2 == bond_reference.atom_id1) {
            return Some(*bond_id);
          }
        }
      }
    }
    None
  }

  pub fn get_bond_by_reference(&self, bond_reference: &BondReference) -> Option<&Bond> {
    self.get_bond_id_by_reference(bond_reference)
      .and_then(|bond_id| self.bonds.get(&bond_id))
  }

  pub fn get_mut_bond_by_reference(&mut self, bond_reference: &BondReference) -> Option<&mut Bond> {
    self.get_bond_id_by_reference(bond_reference)
      .and_then(move |bond_id| self.bonds.get_mut(&bond_id))
  }

  /// Clears the 'marked' property for all atoms in the structure
  pub fn clear_marked_atoms(&mut self) {
    for atom in self.atoms.values_mut() {
      atom.marked = false;
    }
  }

  pub fn clean(&mut self) {
    self.dirty_atom_ids.clear();
  }

  fn make_atom_dirty(&mut self, atom_id: u64) {
    self.dirty_atom_ids.insert(atom_id);
  }

  pub fn obtain_next_atom_id(&mut self) -> u64 {
    let ret = self.next_atom_id;
    self.next_atom_id += 1;
    return ret;
  }

  pub fn obtain_next_bond_id(&mut self) -> u64 {
    let ret = self.next_bond_id;
    self.next_bond_id += 1;
    return ret;
  }

  pub fn obtain_next_cluster_id(&mut self) -> u64 {
    let ret = self.next_cluster_id;
    self.next_cluster_id += 1;
    return ret;
  }

  pub fn add_cluster(&mut self, name: &str) -> u64 {
    let id = self.obtain_next_cluster_id();
    self.add_cluster_with_id(id, name);
    id
  }

  pub fn add_cluster_with_id(&mut self, id: u64, name: &str) {
    self.clusters.insert(id, Cluster {
      id,
      name: name.to_string(),
      atom_ids: HashSet::new(),
      selected: false,
      frame_transform: Transform::default(),
      frame_locked_to_atoms: true,
    });
  }

  pub fn get_cluster(&self, cluster_id: u64) -> Option<&Cluster> {
    self.clusters.get(&cluster_id)
  }

  pub fn select_cluster(&mut self, cluster_id: u64, select_modifier: SelectModifier) -> HashSet<u64> {
    let mut inverted_cluster_ids = HashSet::new();
    
    match select_modifier {
      SelectModifier::Replace => {
        // Track currently selected clusters that will be deselected
        for (id, cluster) in self.clusters.iter() {
          if cluster.selected {
            inverted_cluster_ids.insert(*id);
          }
        }
        
        // Deselect all clusters
        for (_, cluster) in self.clusters.iter_mut() {
          cluster.selected = false;
        }
        
        // Select the specified cluster if it exists
        if let Some(cluster) = self.clusters.get_mut(&cluster_id) {
          // Check if it was previously unselected (will be inverted)
          if !cluster.selected {
            inverted_cluster_ids.insert(cluster_id);
          } else {
            // If it was previously selected, it's not actually inverted
            // (deselected then selected again)
            inverted_cluster_ids.remove(&cluster_id);
          }
          cluster.selected = true;
        }
      },
      SelectModifier::Toggle => {
        // Toggle selection state of the specified cluster
        if let Some(cluster) = self.clusters.get_mut(&cluster_id) {
          cluster.selected = !cluster.selected;
          // Add this cluster to the inverted set
          inverted_cluster_ids.insert(cluster_id);
        }
      },
      SelectModifier::Expand => {
        // Add the specified cluster to the selection
        if let Some(cluster) = self.clusters.get_mut(&cluster_id) {
          // Only track as inverted if we're changing its state from unselected to selected
          if !cluster.selected {
            inverted_cluster_ids.insert(cluster_id);
          }
          cluster.selected = true;
        }
      }
    }
    
    inverted_cluster_ids
  }

  pub fn invert_cluster_selections(&mut self, inverted_cluster_ids: &HashSet<u64>) {
    for cluster_id in inverted_cluster_ids {
      if let Some(cluster) = self.clusters.get_mut(&cluster_id) {
        cluster.selected = !cluster.selected;
      }
    }
  }

  pub fn rename_cluster(&mut self, cluster_id: u64, new_name: &str) {
    if let Some(cluster) = self.clusters.get_mut(&cluster_id) {
      cluster.name = new_name.to_string();
    }
  }

  /// Removes all clusters that have no atoms (empty atom_ids sets)
  /// 
  /// # Returns
  /// 
  /// A vector containing the IDs of the removed clusters
  /// 
  /// A vector containing the IDs of the removed clusters
  pub fn remove_empty_clusters(&mut self) -> Vec<u64> {
    let mut empty_cluster_ids = Vec::new();
    
    // Find all empty clusters
    for (cluster_id, cluster) in &self.clusters {
      if cluster.atom_ids.is_empty() {
        empty_cluster_ids.push(*cluster_id);
      }
    }
    
    // Remove the empty clusters
    for cluster_id in &empty_cluster_ids {
      self.clusters.remove(cluster_id);
    }
    
    empty_cluster_ids
  }

  pub fn add_atom(&mut self, atomic_number: i32, position: DVec3, cluster_id: u64) -> u64 {
    let id = self.obtain_next_atom_id();
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
      marked: false,
    });

    self.add_atom_to_grid(id, &position);

    // Add atom ID to the cluster's atom_ids HashSet if the cluster exists
    if let Some(cluster) = self.clusters.get_mut(&cluster_id) {
      cluster.atom_ids.insert(id);
    }
    
    self.make_atom_dirty(id);
  }

  pub fn delete_atom(&mut self, id: u64) {
    // Get the atom and collect its bond IDs before removing it
    let (pos, bond_ids) = if let Some(atom) = self.atoms.get(&id) {
      // Remove atom ID from its cluster's atom_ids HashSet if the cluster exists
      if let Some(cluster) = self.clusters.get_mut(&atom.cluster_id) {
        cluster.atom_ids.remove(&id);
      }
      // Clone the bond IDs to avoid borrow issues when deleting bonds
      let bond_ids = atom.bond_ids.clone();
      (Some(atom.position), bond_ids)
    } else {
      (None, Vec::new())
    };

    // Delete all bonds connected to this atom
    for bond_id in bond_ids {
      self.delete_bond(bond_id);
    }
    
    // Remove from the grid cell
    if let Some(pos) = pos {
      self.remove_atom_from_grid(id, &pos);
    }

    self.atoms.remove(&id);
    self.make_atom_dirty(id);
  }

  pub fn move_atom_to_cluster(&mut self, atom_id: u64, new_cluster_id: u64) {
    // Get the atom and its current cluster_id
    if let Some(atom) = self.atoms.get_mut(&atom_id) {
      let old_cluster_id = atom.cluster_id;
      
      // Skip if the atom is already in the target cluster
      if old_cluster_id == new_cluster_id {
        return;
      }
      
      // Update the atom's cluster_id
      atom.cluster_id = new_cluster_id;
      
      // Remove atom_id from the old cluster's atom_ids
      if let Some(old_cluster) = self.clusters.get_mut(&old_cluster_id) {
        old_cluster.atom_ids.remove(&atom_id);
      }
      
      // Add atom_id to the new cluster's atom_ids
      if let Some(new_cluster) = self.clusters.get_mut(&new_cluster_id) {
        new_cluster.atom_ids.insert(atom_id);
      }
      
      // Mark the atom as dirty since it's been modified
      self.make_atom_dirty(atom_id);
    }
  }

  pub fn add_bond(&mut self, atom_id1: u64, atom_id2: u64, multiplicity: i32) -> u64 {
    let id = self.obtain_next_bond_id();
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
  pub fn select(&mut self, atom_ids: &Vec<u64>, bond_references: &Vec<BondReference>, select_modifier: SelectModifier) {
    // If select_modifier is Replace, first unselect all the currently selected atoms and bonds
    if select_modifier == SelectModifier::Replace {
      for atom in self.atoms.values_mut() {
        atom.selected = false;
      }
      for bond in self.bonds.values_mut() {
        bond.selected = false;
      }
    }

    for atom_id in atom_ids {
      if let Some(atom) = self.atoms.get_mut(atom_id) {
        atom.selected = apply_select_modifier(atom.selected, &select_modifier);
      }
    }
    for bond_reference in bond_references {
      if let Some(bond) = self.get_mut_bond_by_reference(&bond_reference) {
        bond.selected = apply_select_modifier(bond.selected, &select_modifier);
      }
    }
  }

  pub fn select_by_maps(&mut self, atom_selections: &HashMap<u64, bool>, bond_selections: &HashMap<BondReference, bool>) {
    for (key, value) in atom_selections {
      if let Some(atom) = self.atoms.get_mut(key) {
        atom.selected = *value;
      }
    }
    for (key, value) in bond_selections {
      if let Some(bond) = self.get_mut_bond_by_reference(&key) {
        bond.selected = *value;
      }
    }
  }

  /// Tests if a ray hits any atom or bond in the structure
  /// 
  /// Returns the closest hit with its distance, or HitTestResult::None if nothing was hit
  pub fn hit_test(&self, ray_start: &DVec3, ray_dir: &DVec3) -> HitTestResult {
    let mut closest_hit: Option<(HitTestResult, f64)> = None;

    // Check atoms
    for atom in self.atoms.values() {
      if let Some(distance) = hit_test_utils::sphere_hit_test(
          &atom.position, 
          get_displayed_atom_radius(atom), 
          ray_start, 
          ray_dir) {
        // If this hit is closer than our current closest, replace it
        if closest_hit.is_none() || distance < closest_hit.as_ref().unwrap().1 {
          closest_hit = Some((HitTestResult::Atom(atom.id, distance), distance));
        }
      }
    }
    
    // Check bonds
    for bond in self.bonds.values() {
      if let Some(distance) = hit_test_utils::cylinder_hit_test(
          &self.get_atom(bond.atom_id1).unwrap().position,
          &self.get_atom(bond.atom_id2).unwrap().position,
          BAS_STICK_RADIUS,
          ray_start,
          ray_dir) {
        // If this hit is closer than our current closest, replace it
        if closest_hit.is_none() || distance < closest_hit.as_ref().unwrap().1 {
          closest_hit = Some((HitTestResult::Bond(bond.id, distance), distance));
        }
      }
    }
    
    // Return the closest hit or None if nothing was hit
    match closest_hit {
      Some((hit_result, _)) => hit_result,
      None => HitTestResult::None,
    }
  }

  pub fn find_closest_atom_to_ray(&self, ray_start: &DVec3, ray_dir: &DVec3) -> Option<DVec3> {
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

    if closest_distance_squared == f64::MAX {
        return None;
    }

    return Some(closest_atom_position);
  }

  pub fn find_pivot_point(&self, ray_start: &DVec3, ray_dir: &DVec3) -> DVec3 {
    let closest_atom_position = self.find_closest_atom_to_ray(ray_start, ray_dir);
    return if closest_atom_position.is_some() {
      closest_atom_position.unwrap()
    } else {
      DVec3::new(0.0, 0.0, 0.0)
    };
  }
  
  fn remove_from_bond_arr(&mut self, atom_id: u64, bond_id: u64) {
    let bond_ids = &mut self.atoms.get_mut(&atom_id).unwrap().bond_ids;
    if let Some(pos) = bond_ids.iter().position(|&x| x == bond_id) {
        bond_ids.swap_remove(pos);
    }
  }
  
  /// Transform a single atom by applying rotation and translation.
  /// Updates the atom position in the grid and marks it as dirty.
  ///
  /// # Arguments
  ///
  /// * `atom_id` - The ID of the atom to transform
  /// * `rotation` - The rotation to apply
  /// * `translation` - The translation to apply
  ///
  /// # Returns
  ///
  /// `true` if the atom was found and transformed, `false` otherwise
  pub fn transform_atom(&mut self, atom_id: u64, rotation: &DQuat, translation: &DVec3) -> bool {
    let positions = if let Some(atom) = self.atoms.get_mut(&atom_id) {
      let old_position = atom.position;
      atom.position = rotation.mul_vec3(atom.position) + *translation;
      Some((old_position, atom.position))
    } else {
      None
    };
    
    // Update grid position
    if let Some((old_position, new_position)) = positions {
      self.remove_atom_from_grid(atom_id, &old_position);
      self.add_atom_to_grid(atom_id, &new_position);
      self.make_atom_dirty(atom_id);
      true
    } else {
      false
    }
  }
  
  pub fn transform(&mut self, rotation: &DQuat, translation: &DVec3) {
    // First, collect all atom IDs that will be transformed
    let atom_ids: Vec<u64> = self.atoms.keys().cloned().collect();
    
    // Transform all atoms
    for atom_id in atom_ids {
      self.transform_atom(atom_id, rotation, translation);
    }
  }

  pub fn remove_lone_atoms(&mut self) {
    let lone_atoms: Vec<u64> = self.atoms.values()
      .filter(|atom| atom.bond_ids.is_empty())
      .map(|atom| atom.id)
      .collect();
    
    for atom_id in lone_atoms {
      self.delete_atom(atom_id);
    }
  }

  /// Replaces the atomic number of an atom with a new value
  ///
  /// # Arguments
  ///
  /// * `atom_id` - The ID of the atom to modify
  /// * `atomic_number` - The new atomic number to set
  ///
  /// # Returns
  ///
  /// `true` if the atom was found and updated, `false` otherwise
  pub fn replace_atom(&mut self, atom_id: u64, atomic_number: i32) -> bool {
    if let Some(atom) = self.atoms.get_mut(&atom_id) {
      atom.atomic_number = atomic_number;
      self.make_atom_dirty(atom_id);
      true
    } else {
      false
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

  /// Calculates the default frame_transform for a specific cluster.
  /// The translation is set to the average position of all atoms in the cluster,
  /// and the rotation is set to identity.
  ///
  /// # Arguments
  ///
  /// * `cluster_id` - The ID of the cluster to calculate the frame_transform for
  ///
  /// # Returns
  ///
  /// `true` if the cluster exists and has atoms, `false` otherwise
  pub fn calculate_cluster_default_frame_transform(&mut self, cluster_id: u64) -> bool {
    if let Some(cluster) = self.clusters.get(&cluster_id) {
      if cluster.atom_ids.is_empty() {
        return false;
      }
      
      // Calculate the average position of all atoms in the cluster
      let mut total_position = DVec3::ZERO;
      let mut atom_count = 0;
      
      for atom_id in &cluster.atom_ids {
        if let Some(atom) = self.atoms.get(atom_id) {
          total_position += atom.position;
          atom_count += 1;
        }
      }
      
      if atom_count > 0 {
        // Calculate average position
        let avg_position = total_position / atom_count as f64;
        
        // Update the cluster's frame_transform
        if let Some(cluster) = self.clusters.get_mut(&cluster_id) {
          cluster.frame_transform = Transform::new(avg_position, DQuat::IDENTITY);
        }
        
        return true;
      }
    }
    
    false
  }

  /// Calculates the default frame_transform for all clusters in the atomic structure.
  ///
  /// # Returns
  ///
  /// A vector of cluster IDs for which the frame_transform was successfully calculated
  pub fn calculate_all_clusters_default_frame_transforms(&mut self) -> Vec<u64> {
    let cluster_ids: Vec<u64> = self.clusters.keys().cloned().collect();
    let mut updated_clusters = Vec::new();
    
    for cluster_id in cluster_ids {
      if self.calculate_cluster_default_frame_transform(cluster_id) {
        updated_clusters.push(cluster_id);
      }
    }
    
    updated_clusters
  }

  /// Adds another atomic structure to this one, ensuring all IDs remain distinct.
  /// 
  /// This method copies atoms, bonds, and clusters from the other structure into this one,
  /// assigning new IDs to all elements to avoid conflicts. Clusters remain separate -
  /// atoms from the other structure will remain in their original clusters, but with new cluster IDs.
  /// 
  /// If a cluster's name follows the pattern 'Cluster_{id}', it will be updated to use the new cluster ID.
  ///
  /// # Arguments
  ///
  /// * `other` - The other atomic structure to add to this one
  ///
  /// # Returns
  ///
  /// A mapping of old IDs to new IDs for atoms, bonds, and clusters
  pub fn add_atomic_structure(&mut self, other: &AtomicStructure) -> (HashMap<u64, u64>, HashMap<u64, u64>, HashMap<u64, u64>) {
    let mut atom_id_map: HashMap<u64, u64> = HashMap::new();
    let mut bond_id_map: HashMap<u64, u64> = HashMap::new();
    let mut cluster_id_map: HashMap<u64, u64> = HashMap::new();
    
    // First, create new clusters with new IDs
    for (old_cluster_id, cluster) in &other.clusters {
      let new_cluster_id = self.obtain_next_cluster_id();
      cluster_id_map.insert(*old_cluster_id, new_cluster_id);
      
      // Create the new cluster
      let mut new_name = cluster.name.clone();
      
      // Update name if it follows the 'Cluster_{id}' pattern
      if let Some(captures) = regex::Regex::new(r"^Cluster_(\d+)$").unwrap().captures(&cluster.name) {
        new_name = format!("Cluster_{}", new_cluster_id);
      }
      
      self.clusters.insert(new_cluster_id, Cluster {
        id: new_cluster_id,
        name: new_name,
        atom_ids: HashSet::new(), // Will be populated when adding atoms
        selected: cluster.selected,
        frame_transform: cluster.frame_transform.clone(),
        frame_locked_to_atoms: cluster.frame_locked_to_atoms,
      });
    }
    
    // Add atoms with new IDs
    for (old_atom_id, atom) in &other.atoms {
      let new_atom_id = self.obtain_next_atom_id();
      atom_id_map.insert(*old_atom_id, new_atom_id);
      
      // Get the new cluster ID for this atom
      let new_cluster_id = *cluster_id_map.get(&atom.cluster_id).unwrap_or(&1); // Default to first cluster if mapping not found
      
      // Add the atom with the new ID and cluster ID
      self.add_atom_with_id(
        new_atom_id,
        atom.atomic_number,
        atom.position,
        new_cluster_id
      );
      
      // Copy selected state
      if let Some(new_atom) = self.atoms.get_mut(&new_atom_id) {
        new_atom.selected = atom.selected;
      }
    }
    
    // Add bonds with new IDs
    for (old_bond_id, bond) in &other.bonds {
      // Get the new atom IDs
      if let (Some(&new_atom_id1), Some(&new_atom_id2)) = 
          (atom_id_map.get(&bond.atom_id1), atom_id_map.get(&bond.atom_id2)) {
        
        let new_bond_id = self.obtain_next_bond_id();
        bond_id_map.insert(*old_bond_id, new_bond_id);
        
        // Add the bond with the new IDs
        self.add_bond_with_id(
          new_bond_id,
          new_atom_id1,
          new_atom_id2,
          bond.multiplicity
        );
        
        // Copy selected state
        if let Some(new_bond) = self.bonds.get_mut(&new_bond_id) {
          new_bond.selected = bond.selected;
        }
      }
    }
    
    // Return the ID mappings
    (atom_id_map, bond_id_map, cluster_id_map)
  }
}
