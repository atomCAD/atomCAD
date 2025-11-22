//! Atomic structure representation optimized for mechanical nano machines
//!
//! # Memory Optimization Strategy
//!
//! This module uses an ultra-compact inline bond representation to minimize memory usage:
//! 
//! - **InlineBond: 4 bytes** (29-bit atom_id + 3-bit bond_order)
//! - Bonds stored bidirectionally in both atoms' SmallVec
//! - For diamond structures with 2 bonds/atom average: saving 4 bytes per bond = 8 bytes total per bond pair
//! - Supports up to 536M atoms and 8 bond types
//!

use crate::util::transform::Transform;
use glam::f64::DVec3;
use glam::f64::DQuat;
use std::collections::HashMap;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use crate::util::hit_test_utils;
use crate::renderer::tessellator::atomic_tessellator::get_displayed_atom_radius;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::renderer::tessellator::atomic_tessellator::BAS_STICK_RADIUS;
use crate::api::common_api_types::SelectModifier;
use std::hash::{Hash, Hasher};
use serde::{Serialize, Deserialize};
use crate::util::memory_size_estimator::MemorySizeEstimator;

// Cell size for spatial grid - larger than typical bond length for efficient neighbor lookup
const ATOM_GRID_CELL_SIZE: f64 = 4.0;

#[derive(Debug, Clone)]
pub enum AtomDisplayState {
    Normal,
    Marked,
    SecondaryMarked,
}

#[derive(Debug, Clone)]
pub struct AtomicStructureDecorator {
    pub atom_display_states: FxHashMap<u32, AtomDisplayState>,
    pub selected_bonds: std::collections::HashSet<BondReference>,
}

impl AtomicStructureDecorator {
    pub fn new() -> Self {
        Self {
            atom_display_states: FxHashMap::default(),
            selected_bonds: std::collections::HashSet::new(),
        }
    }
    
    pub fn set_atom_display_state(&mut self, atom_id: u32, state: AtomDisplayState) {
        self.atom_display_states.insert(atom_id, state);
    }
    
    pub fn get_atom_display_state(&self, atom_id: u32) -> AtomDisplayState {
        self.atom_display_states.get(&atom_id).cloned().unwrap_or(AtomDisplayState::Normal)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HitTestResult {
    Atom(u32, f64),  // (atom_id, distance)
    Bond(BondReference, f64),  // (bond_reference, distance)
    None,
}

fn apply_select_modifier(in_selected: bool, select_modifier: &SelectModifier) -> bool {
  match select_modifier {
    SelectModifier::Replace => true,
    SelectModifier::Expand => true,
    SelectModifier::Toggle => !in_selected,
  }
}

/// Ultra-compact inline bond representation - 4 bytes total
/// Stores bond information directly in the atom's SmallVec for maximum cache efficiency
/// 
/// Memory layout: 29 bits atom_id (max 536M atoms) + 3 bits bond_order (8 types)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineBond {
  /// Packed data: lower 29 bits = other_atom_id, upper 3 bits = bond_order
  packed: u32,
}

impl InlineBond {
  const ATOM_ID_MASK: u32 = 0x1FFFFFFF;      // 29 bits
  const BOND_ORDER_SHIFT: u32 = 29;
  const BOND_ORDER_MASK: u32 = 0x7;
  
  #[inline]
  pub fn new(other_atom_id: u32, bond_order: u8) -> Self {
    debug_assert!(other_atom_id <= Self::ATOM_ID_MASK, 
      "Atom ID {} exceeds maximum of {}", other_atom_id, Self::ATOM_ID_MASK);
    debug_assert!(bond_order <= Self::BOND_ORDER_MASK as u8,
      "Bond order {} exceeds maximum of {}", bond_order, Self::BOND_ORDER_MASK);
    
    Self {
      packed: other_atom_id | ((bond_order as u32) << Self::BOND_ORDER_SHIFT)
    }
  }
  
  #[inline]
  pub fn other_atom_id(&self) -> u32 {
    self.packed & Self::ATOM_ID_MASK
  }
  
  #[inline]
  pub fn bond_order(&self) -> u8 {
    ((self.packed >> Self::BOND_ORDER_SHIFT) & Self::BOND_ORDER_MASK) as u8
  }
  
  #[inline]
  pub fn set_bond_order(&mut self, bond_order: u8) {
    debug_assert!(bond_order <= Self::BOND_ORDER_MASK as u8,
      "Bond order {} exceeds maximum of {}", bond_order, Self::BOND_ORDER_MASK);
    self.packed = (self.packed & Self::ATOM_ID_MASK) | ((bond_order as u32) << Self::BOND_ORDER_SHIFT);
  }
}

pub const BOND_SINGLE: u8 = 1;
pub const BOND_DOUBLE: u8 = 2;
pub const BOND_TRIPLE: u8 = 3;
pub const BOND_QUADRUPLE: u8 = 4;
pub const BOND_AROMATIC: u8 = 5;
pub const BOND_DATIVE: u8 = 6;
pub const BOND_METALLIC: u8 = 7;

// BondReference can be used to refer to a bond globally:
// without being in the context of an atom.
// The order of the atoms is irrelevant: two bond references between the same two atoms are equal. 
#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct BondReference {
  pub atom_id1: u32,
  pub atom_id2: u32,
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
    // Consistent hash regardless of atom order
    let (smaller, larger) = if self.atom_id1 < self.atom_id2 {
      (self.atom_id1, self.atom_id2)
    } else {
      (self.atom_id2, self.atom_id1)
    };
    smaller.hash(state);
    larger.hash(state);
  }
}

#[derive(Debug, Clone)]
pub struct Atom {
  pub position: DVec3,
  pub bonds: SmallVec<[InlineBond; 4]>,
  pub id: u32,
  pub in_crystal_depth: f32,
  pub atomic_number: i16,
  pub flags: u16,  // Bit 0: selected, Bit 1: hydrogen passivation
}

const ATOM_FLAG_SELECTED: u16 = 1 << 0;
const ATOM_FLAG_HYDROGEN_PASSIVATION: u16 = 1 << 1;

impl Atom {
  #[inline]
  pub fn is_selected(&self) -> bool {
    (self.flags & ATOM_FLAG_SELECTED) != 0
  }
  
  #[inline]
  pub fn set_selected(&mut self, selected: bool) {
    if selected {
      self.flags |= ATOM_FLAG_SELECTED;
    } else {
      self.flags &= !ATOM_FLAG_SELECTED;
    }
  }
  
  #[inline]
  pub fn is_hydrogen_passivation(&self) -> bool {
    (self.flags & ATOM_FLAG_HYDROGEN_PASSIVATION) != 0
  }
  
  #[inline]
  pub fn set_hydrogen_passivation(&mut self, is_passivation: bool) {
    if is_passivation {
      self.flags |= ATOM_FLAG_HYDROGEN_PASSIVATION;
    } else {
      self.flags &= !ATOM_FLAG_HYDROGEN_PASSIVATION;
    }
  }
}


#[derive(Debug, Clone)]
pub struct AtomicStructure {
  pub frame_transform: Transform,
  pub next_atom_id: u32,
  pub check_atom_id_collision: bool,
  pub atoms: FxHashMap<u32, Atom>,
  // Spatial acceleration: sparse spatial grid of atoms
  // TODO: consider SmallVac here instead of Vec
  // also consider lazily creating this: might not be needed in a lot of usecases
  pub grid: FxHashMap<(i32, i32, i32), Vec<u32>>,
  pub from_selected_node: bool, // TODO: clean this up: it does not belong here
  pub selection_transform: Option<Transform>,
  pub decorator: AtomicStructureDecorator,
}

impl AtomicStructure {

  pub fn has_selected_atoms(&self) -> bool {
    self.atoms.values().any(|atom| atom.is_selected())
  }

  // Checks if there is any selection (atoms or bonds) in the structure
  pub fn has_selection(&self) -> bool {
    self.has_selected_atoms() || !self.decorator.selected_bonds.is_empty()
  }

  pub fn new() -> Self {
    Self {
      frame_transform: Transform::default(),
      next_atom_id: 1,
      check_atom_id_collision: false,
      atoms: FxHashMap::default(),
      grid: FxHashMap::default(),
      from_selected_node: false,
      selection_transform: None,
      decorator: AtomicStructureDecorator::new(),
    }
  }

  pub fn get_num_of_atoms(&self) -> usize {
    self.atoms.len()
  }

  pub fn get_cell_for_pos(&self, pos: &DVec3) -> (i32, i32, i32) {
    let cell = (pos / ATOM_GRID_CELL_SIZE).trunc().as_ivec3();
    (cell.x, cell.y, cell.z)
  }

  pub fn get_atom(&self, atom_id: u32) -> Option<&Atom> {
    self.atoms.get(&atom_id)
  }

  pub fn get_num_of_bonds(&self) -> usize {
    self.atoms.values().map(|atom| atom.bonds.len()).sum::<usize>() / 2  // Bidirectional storage
  }

  pub fn obtain_next_atom_id(&mut self) -> u32 {
    let mut id = self.next_atom_id;
    
    // Check for collision only if we've wrapped around
    if self.check_atom_id_collision {
      while self.atoms.contains_key(&id) {
        id = if id == u32::MAX { 1 } else { id + 1 };
      }
    }
    
    // Update next_atom_id and check for wraparound
    self.next_atom_id = if id == u32::MAX { 1 } else { id + 1 };
    if self.next_atom_id == 1 {
      self.check_atom_id_collision = true;
    }
    
    id
  }

  pub fn add_atom(&mut self, atomic_number: i16, position: DVec3) -> u32 {
    let id = self.obtain_next_atom_id();
    self.add_atom_with_id(id, atomic_number, position);
    id
  }

  pub fn add_atom_with_id(&mut self, id: u32, atomic_number: i16, position: DVec3) {
    self.atoms.insert(id, Atom {
      id,
      atomic_number,
      position,
      bonds: SmallVec::new(),
      flags: 0,  // All flags cleared (including selected)
      in_crystal_depth: 0.0,
    });

    self.add_atom_to_grid(id, &position);
  }

  pub fn set_atom_depth(&mut self, atom_id: u32, depth: f32) -> bool {
    if let Some(atom) = self.atoms.get_mut(&atom_id) {
      atom.in_crystal_depth = depth;
      true
    } else {
      false
    }
  }

  pub fn delete_atom(&mut self, id: u32) {
    let (pos, connected_atoms) = if let Some(atom) = self.atoms.get(&id) {
      let connected: SmallVec<[u32; 4]> = atom.bonds.iter()
        .map(|bond| bond.other_atom_id())
        .collect();
      (Some(atom.position), connected)
    } else {
      (None, SmallVec::new())
    };

    for other_atom_id in connected_atoms {
      if let Some(other_atom) = self.atoms.get_mut(&other_atom_id) {
        other_atom.bonds.retain(|bond| bond.other_atom_id() != id);
      }
    }
    
    if let Some(pos) = pos {
      self.remove_atom_from_grid(id, &pos);
    }

    self.atoms.remove(&id);
  }

  /// Fast deletion for lone atoms (no bonds, guaranteed to exist)
  pub fn delete_lone_atom(&mut self, id: u32) {
    let pos = self.atoms.get(&id).unwrap().position;
    self.remove_atom_from_grid(id, &pos);
    self.atoms.remove(&id);
  }

  /// Safe bond creation - validates atoms exist, updates or creates bond
  /// Returns whether it was successful or not (unsuccessful if atoms do not exist)
  pub fn add_bond_checked(&mut self, atom_id1: u32, atom_id2: u32, bond_order: u8) -> bool {
    if !self.atoms.contains_key(&atom_id1) || !self.atoms.contains_key(&atom_id2) {
      return false;
    }
    
    let bond_exists = self.atoms[&atom_id1].bonds.iter()
      .any(|bond| bond.other_atom_id() == atom_id2);
    
    if bond_exists {
      if let Some(atom) = self.atoms.get_mut(&atom_id1) {
        if let Some(bond) = atom.bonds.iter_mut().find(|b| b.other_atom_id() == atom_id2) {
          bond.set_bond_order(bond_order);
        }
      }
      if let Some(atom) = self.atoms.get_mut(&atom_id2) {
        if let Some(bond) = atom.bonds.iter_mut().find(|b| b.other_atom_id() == atom_id1) {
          bond.set_bond_order(bond_order);
        }
      }
    } else {
      let bond1 = InlineBond::new(atom_id2, bond_order);
      self.atoms.get_mut(&atom_id1).unwrap().bonds.push(bond1);
      
      let bond2 = InlineBond::new(atom_id1, bond_order);
      self.atoms.get_mut(&atom_id2).unwrap().bonds.push(bond2);
    }
    
    true
  }


  /// Fast bond creation for bulk operations - no validation, atoms must exist,
  /// no bond should exist between them
  pub fn add_bond(&mut self, atom_id1: u32, atom_id2: u32, bond_order: u8) {
    let bond1 = InlineBond::new(atom_id2, bond_order);
    self.atoms.get_mut(&atom_id1).unwrap().bonds.push(bond1);
    
    let bond2 = InlineBond::new(atom_id1, bond_order);
    self.atoms.get_mut(&atom_id2).unwrap().bonds.push(bond2);
  }
  
  pub fn has_bond_between(&self, atom_id1: u32, atom_id2: u32) -> bool {
    if let Some(atom) = self.atoms.get(&atom_id1) {
      atom.bonds.iter().any(|bond| bond.other_atom_id() == atom_id2)
    } else {
      false
    }
  }

  /// Delete a bond by BondReference
  pub fn delete_bond(&mut self, bond_ref: &BondReference) {
    // Remove bond from first atom
    if let Some(atom1) = self.atoms.get_mut(&bond_ref.atom_id1) {
      atom1.bonds.retain(|bond| bond.other_atom_id() != bond_ref.atom_id2);
    }
    
    // Remove bond from second atom
    if let Some(atom2) = self.atoms.get_mut(&bond_ref.atom_id2) {
      atom2.bonds.retain(|bond| bond.other_atom_id() != bond_ref.atom_id1);
    }
    
    // Remove from selected bonds
    self.decorator.selected_bonds.remove(bond_ref);
  }

  pub fn select(&mut self, atom_ids: &Vec<u32>, bond_references: &Vec<BondReference>, select_modifier: SelectModifier) {
    if select_modifier == SelectModifier::Replace {
      for atom in self.atoms.values_mut() {
        atom.set_selected(false);
      }
      self.decorator.selected_bonds.clear();
    }

    for atom_id in atom_ids {
      if let Some(atom) = self.atoms.get_mut(atom_id) {
        atom.set_selected(apply_select_modifier(atom.is_selected(), &select_modifier));
      }
    }
    
    for bond_reference in bond_references {
      let is_selected = self.decorator.selected_bonds.contains(bond_reference);
      let new_selection = apply_select_modifier(is_selected, &select_modifier);
      
      if new_selection {
        self.decorator.selected_bonds.insert(bond_reference.clone());
      } else {
        self.decorator.selected_bonds.remove(bond_reference);
      }
    }
  }

  /// Simple bond selection - adds bond to selected set
  pub fn select_bond(&mut self, bond_ref: &BondReference) {
    self.decorator.selected_bonds.insert(bond_ref.clone());
  }

  pub fn select_by_maps(&mut self, atom_selections: &HashMap<u32, bool>, bond_selections: &HashMap<BondReference, bool>) {
    for (key, value) in atom_selections {
      if let Some(atom) = self.atoms.get_mut(key) {
        atom.set_selected(*value);
      }
    }
    
    for (bond_ref, selected) in bond_selections {
      if *selected {
        self.decorator.selected_bonds.insert(bond_ref.clone());
      } else {
        self.decorator.selected_bonds.remove(bond_ref);
      }
    }
  }

  /// Ray hit test - returns closest hit (atom/bond) or None
  pub fn hit_test(&self, ray_start: &DVec3, ray_dir: &DVec3, visualization: &AtomicStructureVisualization) -> HitTestResult {
    let mut closest_hit: Option<(HitTestResult, f64)> = None;

    for atom in self.atoms.values() {
      let Some(distance) = hit_test_utils::sphere_hit_test(
          &atom.position, 
          get_displayed_atom_radius(atom, visualization), 
          ray_start, 
          ray_dir) else { continue };
      
      if closest_hit.is_none() || distance < closest_hit.as_ref().unwrap().1 {
        closest_hit = Some((HitTestResult::Atom(atom.id, distance), distance));
      }
    }
    
    if *visualization != AtomicStructureVisualization::BallAndStick {
      return match closest_hit {
        Some((hit_result, _)) => hit_result,
        None => HitTestResult::None,
      };
    }
    
    for atom in self.atoms.values() {
      for bond in &atom.bonds {
        let other_atom_id = bond.other_atom_id();
        
        if atom.id >= other_atom_id {
          continue;
        }
        
        let Some(other_atom) = self.atoms.get(&other_atom_id) else { continue };
        
        let Some(distance) = hit_test_utils::cylinder_hit_test(
            &atom.position,
            &other_atom.position,
            BAS_STICK_RADIUS,
            ray_start,
            ray_dir) else { continue };
        
        if closest_hit.is_none() || distance < closest_hit.as_ref().unwrap().1 {
          let bond_ref = BondReference {
            atom_id1: atom.id,
            atom_id2: other_atom_id,
          };
          closest_hit = Some((HitTestResult::Bond(bond_ref, distance), distance));
        }
      }
    }
    
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

  /// Sets the position of an atom directly.
  /// Updates the atom position in the grid.
  ///
  /// # Returns
  ///
  /// `true` if the atom was found and updated, `false` otherwise
  pub fn set_atom_position(&mut self, atom_id: u32, new_position: DVec3) -> bool {
    // Epsilon for determining if a position change is significant
    // 1e-5 Angstroms is far below physical significance but above numerical error
    const POSITION_EPSILON: f64 = 1e-5;
    
    // Find the atom and update its position if the change is significant
    let positions = if let Some(atom) = self.atoms.get_mut(&atom_id) {
      let old_position = atom.position;
      
      if old_position.distance_squared(new_position) < POSITION_EPSILON * POSITION_EPSILON {
        return true;
      }
      
      atom.position = new_position;
      Some((old_position, atom.position))
    } else {
      None
    };
    
    // Update grid position
    if let Some((old_position, new_position)) = positions {
      self.remove_atom_from_grid(atom_id, &old_position);
      self.add_atom_to_grid(atom_id, &new_position);
      true
    } else {
      false
    }
  }

  /// Transform a single atom by applying rotation and translation.
  /// Updates the atom position in the grid.
  ///
  /// # Returns
  ///
  /// `true` if the atom was found and transformed, `false` otherwise
  pub fn transform_atom(&mut self, atom_id: u32, rotation: &DQuat, translation: &DVec3) -> bool {
    if let Some(atom) = self.atoms.get(&atom_id) {
      let new_position = rotation.mul_vec3(atom.position) + *translation;
      self.set_atom_position(atom_id, new_position)
    } else {
      false
    }
  }
  
  pub fn transform(&mut self, rotation: &DQuat, translation: &DVec3) {
    let atom_ids: Vec<u32> = self.atoms.keys().cloned().collect();
    
    for atom_id in atom_ids {
      self.transform_atom(atom_id, rotation, translation);
    }
  }

  /// Replaces the atomic number of an atom with a new value
  ///
  /// # Returns
  ///
  /// `true` if the atom was found and updated, `false` otherwise
  pub fn replace_atom(&mut self, atom_id: u32, atomic_number: i16) -> bool {
    if let Some(atom) = self.atoms.get_mut(&atom_id) {
      atom.atomic_number = atomic_number;
      true
    } else {
      false
    }
  }

  // Helper method to add an atom to the grid at a specific position
  fn add_atom_to_grid(&mut self, atom_id: u32, position: &DVec3) {
    let cell = self.get_cell_for_pos(position);
    self.grid.entry(cell).or_insert_with(Vec::new).push(atom_id);
  }

  // Helper method to remove an atom from the grid at a specific position
  fn remove_atom_from_grid(&mut self, atom_id: u32, position: &DVec3) {
    let cell = self.get_cell_for_pos(position);
    if let Some(cell_atoms) = self.grid.get_mut(&cell) {
      cell_atoms.retain(|&x| x != atom_id);
    }
  }

  /// Returns atom IDs within radius of position using spatial grid
  pub fn get_atoms_in_radius(&self, position: &DVec3, radius: f64) -> Vec<u32> {
    let mut result = Vec::new();
    
    // Calculate how many cells we need to check in each direction
    // We add 1 to ensure we cover the boundary cases
    let cell_radius = (radius / ATOM_GRID_CELL_SIZE).ceil() as i32;
    
    // Get the cell coordinates for the center position
    let center_cell = self.get_cell_for_pos(position);
    
    for dx in -cell_radius..=cell_radius {
      for dy in -cell_radius..=cell_radius {
        for dz in -cell_radius..=cell_radius {
          let current_cell = (
            center_cell.0 + dx,
            center_cell.1 + dy,
            center_cell.2 + dz
          );
          
          if let Some(cell_atoms) = self.grid.get(&current_cell) {
            // For each atom in this cell, check if it's within the radius
            for &atom_id in cell_atoms {
              if let Some(atom) = self.atoms.get(&atom_id) {
                let squared_distance = position.distance_squared(atom.position);
                
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

  /// Merges another structure into this one with remapped IDs
  pub fn add_atomic_structure(&mut self, other: &AtomicStructure) -> FxHashMap<u32, u32> {
    let mut atom_id_map: FxHashMap<u32, u32> = FxHashMap::default();
    
    for (old_atom_id, atom) in &other.atoms {
      let new_atom_id = self.obtain_next_atom_id();
      atom_id_map.insert(*old_atom_id, new_atom_id);
      
      self.add_atom_with_id(
        new_atom_id,
        atom.atomic_number,
        atom.position
      );
      
      self.set_atom_depth(new_atom_id, atom.in_crystal_depth);
      
      if let Some(new_atom) = self.atoms.get_mut(&new_atom_id) {
        new_atom.set_selected(atom.is_selected());
      }
    }
    
    // Add inline bonds with remapped atom IDs
    for (old_atom_id, atom) in &other.atoms {
      let new_atom_id = *atom_id_map.get(old_atom_id).unwrap();
      
      for bond in &atom.bonds {
        let old_other_id = bond.other_atom_id();
        let new_other_id = *atom_id_map.get(&old_other_id).unwrap();
        
        // Only add each bond once (check atom ID ordering)
        if new_atom_id < new_other_id {
          self.add_bond_checked(new_atom_id, new_other_id, bond.bond_order());
        }
      }
    }
    
    // Merge bond selections from decorator
    for bond_ref in &other.decorator.selected_bonds {
      if let (Some(&new_id1), Some(&new_id2)) = 
          (atom_id_map.get(&bond_ref.atom_id1), atom_id_map.get(&bond_ref.atom_id2)) {
        let new_bond_ref = BondReference {
          atom_id1: new_id1,
          atom_id2: new_id2,
        };
        self.decorator.selected_bonds.insert(new_bond_ref);
      }
    }
    
    atom_id_map
  }
}

// Memory size estimation implementations

impl Atom {
  /// Average ~64 bytes: 4 InlineBonds inline in SmallVec, zero heap allocation
  const fn average_memory_bytes() -> usize {
    std::mem::size_of::<Atom>()
  }
}


impl MemorySizeEstimator for AtomicStructure {
  fn estimate_memory_bytes(&self) -> usize {
    let base_size = std::mem::size_of::<AtomicStructure>();
    let atoms_size = self.atoms.len() * (std::mem::size_of::<u32>() + Atom::average_memory_bytes());
    let grid_size = self.grid.len() * (std::mem::size_of::<(i32, i32, i32)>() + std::mem::size_of::<Vec<u32>>() + 2 * std::mem::size_of::<u32>());
    let decorator_size = std::mem::size_of::<AtomicStructureDecorator>()
      + (self.atoms.len() / 10) * (std::mem::size_of::<u32>() + std::mem::size_of::<AtomDisplayState>());

    base_size + atoms_size + grid_size + decorator_size
  }
}
