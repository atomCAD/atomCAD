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
//! Other optimization ideas for the future:
//! - Lazy-initialization of the grid. IT might not be needed in certain usecases and
//!   even if needed eventually it is useful to have faster processing until it is needed
//!   (e.g atom fill)
//! - Use fixed-point IVec3 for position instead of DVec3 (one unit can be 0.001 Angstrom)
//! - Structure of arrays for atoms?
//! - Long term: lazy-storing atomic representations in crystals: only compute and store atomic
//!   representation of a region if something needs to be changed. The non-changed regions might be
//!   represented as octree nodes or just use the existing grid. This can be part of
//!   a bigger stramable LOD system in atomCAD. Needs to be carefully planned before implemented though
//!   and maybe the crystal representation should be introduced only on a higher level
//!   which just uses AtomicStructure.
//!

use crate::api::common_api_types::SelectModifier;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::util::hit_test_utils;
use crate::util::memory_size_estimator::MemorySizeEstimator;
use crate::util::transform::Transform;
use glam::f64::DQuat;
use glam::f64::DVec3;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::collections::HashMap;

pub mod atom;
pub mod atomic_structure_decorator;
pub mod bond_reference;
pub mod inline_bond;

// Re-export types for convenience
pub use atom::Atom;
pub use atomic_structure_decorator::{AtomDisplayState, AtomicStructureDecorator};
pub use bond_reference::BondReference;
pub use inline_bond::{
    BOND_AROMATIC, BOND_DATIVE, BOND_DOUBLE, BOND_METALLIC, BOND_QUADRUPLE, BOND_SINGLE,
    BOND_TRIPLE, InlineBond,
};

// Cell size for spatial grid - larger than typical bond length for efficient neighbor lookup
const ATOM_GRID_CELL_SIZE: f64 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub enum HitTestResult {
    Atom(u32, f64),           // (atom_id, distance)
    Bond(BondReference, f64), // (bond_reference, distance)
    None,
}

fn apply_select_modifier(in_selected: bool, select_modifier: &SelectModifier) -> bool {
    match select_modifier {
        SelectModifier::Replace => true,
        SelectModifier::Expand => true,
        SelectModifier::Toggle => !in_selected,
    }
}

#[derive(Debug, Clone)]
pub struct AtomicStructure {
    frame_transform: Transform,
    atoms: Vec<Option<Atom>>, // Index = atom_id - 1, next ID = atoms.len() + 1
    num_atoms: usize,         // Count of non-None atoms
    num_bonds: usize,         // Count of unique bonds (bidirectional storage counted once)
    // Spatial acceleration: sparse spatial grid of atoms
    // TODO: consider SmallVac here instead of Vec
    // also consider lazily creating this: might not be needed in a lot of usecases
    grid: FxHashMap<(i32, i32, i32), Vec<u32>>,
    decorator: AtomicStructureDecorator,
}

impl Default for AtomicStructure {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicStructure {
    // Decorator access
    pub fn decorator(&self) -> &AtomicStructureDecorator {
        &self.decorator
    }

    pub fn decorator_mut(&mut self) -> &mut AtomicStructureDecorator {
        &mut self.decorator
    }

    // Frame transform access
    pub fn frame_transform(&self) -> &Transform {
        &self.frame_transform
    }

    pub fn set_frame_transform(&mut self, transform: Transform) {
        self.frame_transform = transform;
    }

    // Atom access methods
    fn get_atom_mut(&mut self, id: u32) -> Option<&mut Atom> {
        if id == 0 {
            return None;
        }
        let index = (id - 1) as usize;
        self.atoms.get_mut(index).and_then(|slot| slot.as_mut())
    }

    pub fn iter_atoms(&self) -> impl Iterator<Item = (&u32, &Atom)> {
        self.atoms
            .iter()
            .filter_map(|slot| slot.as_ref().map(|atom| (&atom.id, atom)))
    }

    pub fn atoms_values(&self) -> impl Iterator<Item = &Atom> {
        self.atoms.iter().filter_map(|slot| slot.as_ref())
    }

    pub fn atom_ids(&self) -> impl Iterator<Item = &u32> {
        self.atoms
            .iter()
            .filter_map(|slot| slot.as_ref().map(|atom| &atom.id))
    }

    // Atom field setters
    pub fn set_atom_atomic_number(&mut self, atom_id: u32, atomic_number: i16) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.atomic_number = atomic_number;
        }
    }

    pub fn set_atom_in_crystal_depth(&mut self, atom_id: u32, depth: f32) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.in_crystal_depth = depth;
        }
    }

    pub fn set_atom_hydrogen_passivation(&mut self, atom_id: u32, passivated: bool) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.set_hydrogen_passivation(passivated);
        }
    }

    pub fn has_selected_atoms(&self) -> bool {
        self.atoms
            .iter()
            .filter_map(|slot| slot.as_ref())
            .any(|atom| atom.is_selected())
    }

    // Checks if there is any selection (atoms or bonds) in the structure
    pub fn has_selection(&self) -> bool {
        self.has_selected_atoms() || self.decorator.has_selected_bonds()
    }

    pub fn new() -> Self {
        Self {
            frame_transform: Transform::default(),
            atoms: Vec::new(),
            num_atoms: 0,
            num_bonds: 0,
            grid: FxHashMap::default(),
            decorator: AtomicStructureDecorator::new(),
        }
    }

    pub fn get_num_of_atoms(&self) -> usize {
        self.num_atoms
    }

    pub fn get_cell_for_pos(&self, pos: &DVec3) -> (i32, i32, i32) {
        let cell = (pos / ATOM_GRID_CELL_SIZE).trunc().as_ivec3();
        (cell.x, cell.y, cell.z)
    }

    pub fn get_atom(&self, atom_id: u32) -> Option<&Atom> {
        if atom_id == 0 {
            return None;
        }
        let index = (atom_id - 1) as usize;
        self.atoms.get(index).and_then(|slot| slot.as_ref())
    }

    pub fn get_num_of_bonds(&self) -> usize {
        self.num_bonds
    }

    /// Adds an atom to the structure and returns its ID
    pub fn add_atom(&mut self, atomic_number: i16, position: DVec3) -> u32 {
        // Next ID is always: Vec length + 1 (since IDs are 1-indexed)
        let id = (self.atoms.len() + 1) as u32;

        let atom = Atom {
            id,
            atomic_number,
            position,
            bonds: SmallVec::new(),
            flags: 0, // All flags cleared (including selected)
            in_crystal_depth: 0.0,
        };

        // Always append to end (index = id - 1 = atoms.len())
        self.atoms.push(Some(atom));
        self.num_atoms += 1;
        self.add_atom_to_grid(id, &position);

        id
    }

    pub fn set_atom_depth(&mut self, atom_id: u32, depth: f32) -> bool {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.in_crystal_depth = depth;
            true
        } else {
            false
        }
    }

    pub fn delete_atom(&mut self, id: u32) {
        if id == 0 {
            return;
        }

        let index = (id - 1) as usize;
        if index >= self.atoms.len() {
            return;
        }

        let (pos, connected_atoms) = if let Some(atom) = &self.atoms[index] {
            let connected: SmallVec<[u32; 4]> =
                atom.bonds.iter().map(|bond| bond.other_atom_id()).collect();
            (Some(atom.position), connected)
        } else {
            return; // Already deleted
        };

        // Decrement bond count for each bond being removed
        let num_bonds_to_remove = connected_atoms.len();

        for other_atom_id in connected_atoms {
            if let Some(other_atom) = self.get_atom_mut(other_atom_id) {
                other_atom.bonds.retain(|bond| bond.other_atom_id() != id);
            }
        }

        if let Some(pos) = pos {
            self.remove_atom_from_grid(id, &pos);
        }

        self.atoms[index] = None;
        self.num_atoms -= 1;
        self.num_bonds -= num_bonds_to_remove;

        // Clear from decorator
        self.decorator.atom_display_states.remove(&id);
    }

    /// Fast deletion for lone atoms (no bonds, guaranteed to exist)
    pub fn delete_lone_atom(&mut self, id: u32) {
        if id == 0 {
            return;
        }
        let index = (id - 1) as usize;
        let pos = self.atoms[index].as_ref().unwrap().position;
        self.remove_atom_from_grid(id, &pos);
        self.atoms[index] = None;
        self.num_atoms -= 1;
        self.decorator.atom_display_states.remove(&id);
    }

    /// Safe bond creation - validates atoms exist, updates or creates bond
    /// Returns whether it was successful or not (unsuccessful if atoms do not exist)
    pub fn add_bond_checked(&mut self, atom_id1: u32, atom_id2: u32, bond_order: u8) -> bool {
        if self.get_atom(atom_id1).is_none() || self.get_atom(atom_id2).is_none() {
            return false;
        }

        let bond_exists = self
            .get_atom(atom_id1)
            .unwrap()
            .bonds
            .iter()
            .any(|bond| bond.other_atom_id() == atom_id2);

        if bond_exists {
            if let Some(atom) = self.get_atom_mut(atom_id1) {
                if let Some(bond) = atom
                    .bonds
                    .iter_mut()
                    .find(|b| b.other_atom_id() == atom_id2)
                {
                    bond.set_bond_order(bond_order);
                }
            }
            if let Some(atom) = self.get_atom_mut(atom_id2) {
                if let Some(bond) = atom
                    .bonds
                    .iter_mut()
                    .find(|b| b.other_atom_id() == atom_id1)
                {
                    bond.set_bond_order(bond_order);
                }
            }
        } else {
            let bond1 = InlineBond::new(atom_id2, bond_order);
            self.get_atom_mut(atom_id1).unwrap().bonds.push(bond1);

            let bond2 = InlineBond::new(atom_id1, bond_order);
            self.get_atom_mut(atom_id2).unwrap().bonds.push(bond2);

            self.num_bonds += 1; // Count unique bond once (bidirectional storage)
        }

        true
    }

    /// Fast bond creation for bulk operations - no validation, atoms must exist,
    /// no bond should exist between them
    pub fn add_bond(&mut self, atom_id1: u32, atom_id2: u32, bond_order: u8) {
        let bond1 = InlineBond::new(atom_id2, bond_order);
        self.get_atom_mut(atom_id1).unwrap().bonds.push(bond1);

        let bond2 = InlineBond::new(atom_id1, bond_order);
        self.get_atom_mut(atom_id2).unwrap().bonds.push(bond2);

        self.num_bonds += 1; // Count unique bond once (bidirectional storage)
    }

    pub fn has_bond_between(&self, atom_id1: u32, atom_id2: u32) -> bool {
        if let Some(atom) = self.get_atom(atom_id1) {
            atom.bonds
                .iter()
                .any(|bond| bond.other_atom_id() == atom_id2)
        } else {
            false
        }
    }

    /// Delete a bond by BondReference
    pub fn delete_bond(&mut self, bond_ref: &BondReference) {
        // Remove bond from first atom
        if let Some(atom1) = self.get_atom_mut(bond_ref.atom_id1) {
            atom1
                .bonds
                .retain(|bond| bond.other_atom_id() != bond_ref.atom_id2);
        }

        // Remove bond from second atom
        if let Some(atom2) = self.get_atom_mut(bond_ref.atom_id2) {
            atom2
                .bonds
                .retain(|bond| bond.other_atom_id() != bond_ref.atom_id1);
        }

        self.num_bonds -= 1; // Decrement unique bond count

        // Remove from selected bonds
        self.decorator.deselect_bond(bond_ref);
    }

    pub fn select(
        &mut self,
        atom_ids: &Vec<u32>,
        bond_references: &Vec<BondReference>,
        select_modifier: SelectModifier,
    ) {
        if select_modifier == SelectModifier::Replace {
            for atom in self.atoms.iter_mut().flatten() {
                atom.set_selected(false);
            }
            self.decorator.clear_bond_selection();
        }

        for atom_id in atom_ids {
            if let Some(atom) = self.get_atom_mut(*atom_id) {
                atom.set_selected(apply_select_modifier(atom.is_selected(), &select_modifier));
            }
        }

        for bond_reference in bond_references {
            let is_selected = self.decorator.is_bond_selected(bond_reference);
            let new_selection = apply_select_modifier(is_selected, &select_modifier);

            if new_selection {
                self.decorator.select_bond(bond_reference);
            } else {
                self.decorator.deselect_bond(bond_reference);
            }
        }
    }

    /// Simple bond selection - adds bond to selected set
    pub fn select_bond(&mut self, bond_ref: &BondReference) {
        self.decorator.select_bond(bond_ref);
    }

    pub fn select_by_maps(
        &mut self,
        atom_selections: &HashMap<u32, bool>,
        bond_selections: &HashMap<BondReference, bool>,
    ) {
        for (key, value) in atom_selections {
            if let Some(atom) = self.get_atom_mut(*key) {
                atom.set_selected(*value);
            }
        }

        for (bond_ref, selected) in bond_selections {
            if *selected {
                self.decorator.select_bond(bond_ref);
            } else {
                self.decorator.deselect_bond(bond_ref);
            }
        }
    }

    /// Ray hit test - returns closest hit (atom/bond) or None
    ///
    /// Parameters:
    /// - atom_radius_fn: Function to get the visual radius for each atom (depends on visualization mode)
    /// - bond_radius: Visual radius for bonds (only used in BallAndStick mode)
    pub fn hit_test<F>(
        &self,
        ray_start: &DVec3,
        ray_dir: &DVec3,
        visualization: &AtomicStructureVisualization,
        atom_radius_fn: F,
        bond_radius: f64,
    ) -> HitTestResult
    where
        F: Fn(&Atom) -> f64,
    {
        let mut closest_hit: Option<(HitTestResult, f64)> = None;

        for atom in self.atoms_values() {
            let Some(distance) = hit_test_utils::sphere_hit_test(
                &atom.position,
                atom_radius_fn(atom),
                ray_start,
                ray_dir,
            ) else {
                continue;
            };

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

        for atom in self.atoms_values() {
            for bond in &atom.bonds {
                let other_atom_id = bond.other_atom_id();

                if atom.id >= other_atom_id {
                    continue;
                }

                let Some(other_atom) = self.get_atom(other_atom_id) else {
                    continue;
                };

                let Some(distance) = hit_test_utils::cylinder_hit_test(
                    &atom.position,
                    &other_atom.position,
                    bond_radius,
                    ray_start,
                    ray_dir,
                ) else {
                    continue;
                };

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

        for atom in self.atoms_values() {
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
            None
        } else {
            Some(closest_atom_position)
        }
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
        let positions = if let Some(atom) = self.get_atom_mut(atom_id) {
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
        if let Some(atom) = self.get_atom(atom_id) {
            let new_position = rotation.mul_vec3(atom.position) + *translation;
            self.set_atom_position(atom_id, new_position)
        } else {
            false
        }
    }

    pub fn transform(&mut self, rotation: &DQuat, translation: &DVec3) {
        let atom_ids: Vec<u32> = self.atom_ids().cloned().collect();

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
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.atomic_number = atomic_number;
            true
        } else {
            false
        }
    }

    // Helper method to add an atom to the grid at a specific position
    fn add_atom_to_grid(&mut self, atom_id: u32, position: &DVec3) {
        let cell = self.get_cell_for_pos(position);
        self.grid.entry(cell).or_default().push(atom_id);
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
                    let current_cell = (center_cell.0 + dx, center_cell.1 + dy, center_cell.2 + dz);

                    if let Some(cell_atoms) = self.grid.get(&current_cell) {
                        // For each atom in this cell, check if it's within the radius
                        for &atom_id in cell_atoms {
                            if let Some(atom) = self.get_atom(atom_id) {
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

        for (old_atom_id, atom) in other.iter_atoms() {
            let new_atom_id = self.add_atom(atom.atomic_number, atom.position);
            atom_id_map.insert(*old_atom_id, new_atom_id);

            self.set_atom_depth(new_atom_id, atom.in_crystal_depth);

            // Copy all flags at once (selected, hydrogen_passivation, and any future flags)
            if let Some(new_atom) = self.get_atom_mut(new_atom_id) {
                new_atom.flags = atom.flags;
            }
        }

        // Add inline bonds with remapped atom IDs
        for (old_atom_id, atom) in other.iter_atoms() {
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
        for bond_ref in other.decorator.iter_selected_bonds() {
            if let (Some(&new_id1), Some(&new_id2)) = (
                atom_id_map.get(&bond_ref.atom_id1),
                atom_id_map.get(&bond_ref.atom_id2),
            ) {
                let new_bond_ref = BondReference {
                    atom_id1: new_id1,
                    atom_id2: new_id2,
                };
                self.decorator.select_bond(&new_bond_ref);
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
        let atoms_size =
            self.atoms.len() * (std::mem::size_of::<u32>() + Atom::average_memory_bytes());
        let grid_size = self.grid.len()
            * (std::mem::size_of::<(i32, i32, i32)>()
                + std::mem::size_of::<Vec<u32>>()
                + 2 * std::mem::size_of::<u32>());
        let decorator_size = std::mem::size_of::<AtomicStructureDecorator>()
            + (self.atoms.len() / 10)
                * (std::mem::size_of::<u32>() + std::mem::size_of::<AtomDisplayState>());

        base_size + atoms_size + grid_size + decorator_size
    }
}

impl AtomicStructure {
    /// Returns a detailed string representation for snapshot testing.
    /// Includes frame transform, counts, and details of first 10 atoms and bonds.
    pub fn to_detailed_string(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!("atoms: {}", self.num_atoms));
        lines.push(format!("bonds: {}", self.num_bonds));
        lines.push("frame_transform:".to_string());
        lines.push(format!(
            "  translation: ({:.6}, {:.6}, {:.6})",
            self.frame_transform.translation.x,
            self.frame_transform.translation.y,
            self.frame_transform.translation.z
        ));
        lines.push(format!(
            "  rotation: ({:.6}, {:.6}, {:.6}, {:.6})",
            self.frame_transform.rotation.x,
            self.frame_transform.rotation.y,
            self.frame_transform.rotation.z,
            self.frame_transform.rotation.w
        ));

        // First 10 atoms
        let atoms: Vec<&Atom> = self.atoms_values().take(10).collect();
        if !atoms.is_empty() {
            lines.push(format!("first {} atoms:", atoms.len()));
            for atom in &atoms {
                lines.push(format!(
                    "  [{}] Z={} pos=({:.6}, {:.6}, {:.6}) depth={:.3} bonds={}",
                    atom.id,
                    atom.atomic_number,
                    atom.position.x,
                    atom.position.y,
                    atom.position.z,
                    atom.in_crystal_depth,
                    atom.bonds.len()
                ));
            }
            if self.num_atoms > 10 {
                lines.push(format!("  ... and {} more atoms", self.num_atoms - 10));
            }
        }

        // First 10 bonds (iterate through atoms and collect unique bonds)
        let mut bonds_shown = 0;
        lines.push(format!(
            "first {} bonds:",
            std::cmp::min(10, self.num_bonds)
        ));
        'outer: for atom in self.atoms_values() {
            for inline_bond in &atom.bonds {
                let other_id = inline_bond.other_atom_id();
                // Only show each bond once (when atom.id < other_id)
                if atom.id < other_id {
                    lines.push(format!(
                        "  {} -- {} (order={})",
                        atom.id,
                        other_id,
                        inline_bond.bond_order()
                    ));
                    bonds_shown += 1;
                    if bonds_shown >= 10 {
                        break 'outer;
                    }
                }
            }
        }
        if self.num_bonds > 10 {
            lines.push(format!("  ... and {} more bonds", self.num_bonds - 10));
        }

        lines.join("\n")
    }
}
