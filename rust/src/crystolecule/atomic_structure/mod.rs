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
use glam::Vec3;
use glam::f64::DQuat;
use glam::f64::DVec3;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::collections::HashMap;

pub mod atom;
pub mod atomic_structure_decorator;
pub mod bond_reference;
pub mod fragment;
pub mod inline_bond;
pub mod tags;

// Re-export types for convenience
pub use atom::Atom;
pub use atomic_structure_decorator::{AtomDisplayState, AtomRenderStyle, AtomicStructureDecorator};
pub use bond_reference::BondReference;
pub use inline_bond::{
    BOND_AROMATIC, BOND_DATIVE, BOND_DELETED, BOND_DOUBLE, BOND_METALLIC, BOND_QUADRUPLE,
    BOND_SINGLE, BOND_TRIPLE, InlineBond,
};
pub use tags::{MAX_TAGS, TagError, TagRemap};

/// Atomic number used as a delete marker in diff structures.
/// An atom with this atomic number in a diff means "delete the matched base atom."
pub const DELETED_SITE_ATOMIC_NUMBER: i16 = 0;

/// Atomic number used as an "unchanged" marker in diff structures.
/// An atom with this atomic number means "match the base atom at this position
/// but do not modify it." Used when only bonds between existing atoms change.
pub const UNCHANGED_ATOMIC_NUMBER: i16 = -1;

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
    atoms: Vec<Option<Atom>>, // Index = atom_id - 1, next ID = atoms.len() + 1
    num_atoms: usize,         // Count of non-None atoms
    num_bonds: usize,         // Count of unique bonds (bidirectional storage counted once)
    // Spatial acceleration: sparse spatial grid of atoms
    // TODO: consider SmallVac here instead of Vec
    // also consider lazily creating this: might not be needed in a lot of usecases
    grid: FxHashMap<(i32, i32, i32), Vec<u32>>,
    decorator: AtomicStructureDecorator,
    /// Whether this structure represents a diff (contains delete markers, anchors)
    is_diff: bool,
    /// For diff structures: maps diff atom IDs to base match positions (for moved atoms)
    anchor_positions: FxHashMap<u32, DVec3>,
    /// Maps non-physical atomic numbers to real ones for force field evaluation,
    /// guided placement, hydrogen passivation, etc.
    /// Used by motif_edit to let parameter elements (e.g. -100) behave as their
    /// default real element (e.g. 6 for Carbon) in all chemistry-aware subsystems.
    effective_atomic_numbers: FxHashMap<i16, i16>,
    /// Interned tag names. Index = bit position in `Atom.tag_bits`; at most
    /// [`MAX_TAGS`] entries. A slot's name is stable for the structure's
    /// lifetime unless the slot is reclaimed (see `intern_tag`). May contain
    /// *dead* names — interned but currently carried by no atom. See
    /// `doc/design_atom_tags.md`.
    tag_names: Vec<String>,
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

    // Diff accessors
    pub fn is_diff(&self) -> bool {
        self.is_diff
    }

    pub fn set_is_diff(&mut self, is_diff: bool) {
        self.is_diff = is_diff;
    }

    pub fn anchor_position(&self, atom_id: u32) -> Option<&DVec3> {
        self.anchor_positions.get(&atom_id)
    }

    pub fn set_anchor_position(&mut self, atom_id: u32, pos: DVec3) {
        self.anchor_positions.insert(atom_id, pos);
    }

    pub fn remove_anchor_position(&mut self, atom_id: u32) {
        self.anchor_positions.remove(&atom_id);
    }

    pub fn has_anchor_position(&self, atom_id: u32) -> bool {
        self.anchor_positions.contains_key(&atom_id)
    }

    pub fn anchor_positions(&self) -> &FxHashMap<u32, DVec3> {
        &self.anchor_positions
    }

    // Effective atomic number overrides (for parameter elements in motif_edit)

    /// Returns the effective atomic number for an atom, resolving any override.
    /// For parameter elements (e.g. -100), returns the mapped real element.
    /// For normal atoms, returns the atom's own atomic number unchanged.
    pub fn effective_atomic_number(&self, atom: &Atom) -> i16 {
        self.effective_atomic_numbers
            .get(&atom.atomic_number)
            .copied()
            .unwrap_or(atom.atomic_number)
    }

    pub fn set_effective_atomic_numbers(&mut self, overrides: FxHashMap<i16, i16>) {
        self.effective_atomic_numbers = overrides;
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

    pub fn set_atom_selected(&mut self, atom_id: u32, selected: bool) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.set_selected(selected);
        }
    }

    pub fn set_atom_frozen(&mut self, atom_id: u32, frozen: bool) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.set_frozen(frozen);
        }
    }

    pub fn set_atom_ghost(&mut self, atom_id: u32, ghost: bool) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.set_ghost(ghost);
        }
    }

    pub fn set_atom_patch_ghost(&mut self, atom_id: u32, patch_ghost: bool) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.set_patch_ghost(patch_ghost);
        }
    }

    /// Overwrites an atom's entire flag word. Used by `weld_coincident_atoms`
    /// to install the unioned flags of a fused cluster onto its survivor.
    pub fn set_atom_flags(&mut self, atom_id: u32, flags: u16) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.flags = flags;
        }
    }

    pub fn set_atom_hybridization_override(&mut self, atom_id: u32, hybridization: u8) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.set_hybridization_override(hybridization);
        }
    }

    // ========================================================================
    // Tags (see `doc/design_atom_tags.md`)
    //
    // A tag is a user-chosen name attached to a set of atoms — inert, durable
    // metadata. Storage is this interned `tag_names` table plus a per-atom
    // `Atom.tag_bits` bitmask. Bit indices are **per-structure**: the same name
    // can sit at different bit positions in two structures, so every
    // cross-structure operation works at the *name* level and translates masks
    // through a [`TagRemap`]. All tag access goes through these accessors — no
    // direct `tag_bits` fiddling outside `atomic_structure/`.
    // ========================================================================

    /// The interned tag table (bit order), for serialization and UI. May
    /// include *dead* names — interned but currently carried by no atom
    /// (candidates for slot reclamation, see [`Self::intern_tag`]).
    pub fn tag_names(&self) -> &[String] {
        &self.tag_names
    }

    /// The mask of *live* tag bits — bits carried by at least one atom. One
    /// O(atoms) OR-sweep; used to detect reclaimable slots and to intern only
    /// the source's live names on merge.
    fn live_tag_mask(&self) -> u32 {
        let mut mask = 0u32;
        for atom in self.atoms_values() {
            mask |= atom.tag_bits;
            if mask == u32::MAX {
                break;
            }
        }
        mask
    }

    /// Look up or create the bit index for `name` (trimmed).
    ///
    /// Interning is idempotent: an already-present name returns its existing
    /// bit. When the table is full ([`MAX_TAGS`] entries) this first tries to
    /// reclaim a **dead slot** — the lowest bit that no atom carries — reusing
    /// it for `name` (the old name is simply forgotten; no atom referenced it).
    /// Only when all [`MAX_TAGS`] bits are live does it fail.
    ///
    /// `Err(EmptyName)` when the name is empty/whitespace after trimming;
    /// `Err(LimitReached)` when the table is full of live names.
    pub fn intern_tag(&mut self, name: &str) -> Result<u8, TagError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(TagError::EmptyName);
        }
        // Already interned?
        if let Some(idx) = self.tag_names.iter().position(|n| n == trimmed) {
            return Ok(idx as u8);
        }
        // Room to grow the dense table?
        if self.tag_names.len() < MAX_TAGS {
            let idx = self.tag_names.len() as u8;
            self.tag_names.push(trimmed.to_string());
            return Ok(idx);
        }
        // Table full — reclaim the lowest dead slot (a bit no atom carries).
        // Slot reuse never touches any `Atom.tag_bits`.
        let live = self.live_tag_mask();
        for i in 0..MAX_TAGS {
            if live & (1u32 << i) == 0 {
                self.tag_names[i] = trimmed.to_string();
                return Ok(i as u8);
            }
        }
        Err(TagError::LimitReached)
    }

    /// Bit index of `name` if interned; does not create.
    pub fn tag_index(&self, name: &str) -> Option<u8> {
        let trimmed = name.trim();
        self.tag_names
            .iter()
            .position(|n| n == trimmed)
            .map(|i| i as u8)
    }

    /// Add tag `name` to the atom (interning the name if needed). Missing atom
    /// or full table surfaces as `Err`; re-tagging an already-tagged atom is a
    /// no-op.
    pub fn add_atom_tag(&mut self, atom_id: u32, name: &str) -> Result<(), TagError> {
        let idx = self.intern_tag(name)?;
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.tag_bits |= 1u32 << idx;
        }
        Ok(())
    }

    /// Remove tag `name` from the atom. An absent name or an atom that does not
    /// carry it is a no-op by design.
    pub fn remove_atom_tag(&mut self, atom_id: u32, name: &str) {
        if let Some(idx) = self.tag_index(name) {
            if let Some(atom) = self.get_atom_mut(atom_id) {
                atom.tag_bits &= !(1u32 << idx);
            }
        }
    }

    /// Remove *all* tags from the atom.
    pub fn clear_atom_tags(&mut self, atom_id: u32) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.tag_bits = 0;
        }
    }

    /// Whether the atom carries tag `name`.
    pub fn atom_has_tag(&self, atom_id: u32, name: &str) -> bool {
        match (self.tag_index(name), self.get_atom(atom_id)) {
            (Some(idx), Some(atom)) => atom.tag_bits & (1u32 << idx) != 0,
            _ => false,
        }
    }

    /// Names of the tags this atom carries, in bit order.
    pub fn atom_tags(&self, atom_id: u32) -> Vec<&str> {
        let Some(atom) = self.get_atom(atom_id) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for i in 0..MAX_TAGS {
            if atom.tag_bits & (1u32 << i) != 0 {
                if let Some(name) = self.tag_names.get(i) {
                    names.push(name.as_str());
                }
            }
        }
        names
    }

    /// Derived query: ids of all atoms carrying `name`. O(atoms).
    pub fn atoms_with_tag(&self, name: &str) -> Vec<u32> {
        let Some(idx) = self.tag_index(name) else {
            return Vec::new();
        };
        let bit = 1u32 << idx;
        self.atoms_values()
            .filter(|a| a.tag_bits & bit != 0)
            .map(|a| a.id)
            .collect()
    }

    /// Overwrites an atom's raw tag bitmask. Low-level sibling of
    /// [`Self::set_atom_flags`], used by `weld_coincident_atoms` to install the
    /// OR of a fused cluster's masks onto its survivor. Weld runs within one
    /// structure (one shared table), so a raw OR is exact — cross-structure
    /// callers must go through [`Self::build_tag_remap`] instead.
    pub fn set_atom_tag_bits(&mut self, atom_id: u32, tag_bits: u32) {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.tag_bits = tag_bits;
        }
    }

    /// Raw tag bitmask of an atom (0 if the atom is absent). For welds and
    /// other within-structure mask arithmetic.
    pub fn atom_tag_bits(&self, atom_id: u32) -> u32 {
        self.get_atom(atom_id).map(|a| a.tag_bits).unwrap_or(0)
    }

    /// Interns each of `source`'s **live** tag names into `self` and returns the
    /// source-bit → target-bit translation. Names that no longer fit are left
    /// unmapped and returned in the dropped list — never a panic, never a
    /// silent drop. When the two tables are already identical the result is the
    /// identity remap (the cheap common case: clone-mutate diffs, repeated
    /// merges of siblings).
    ///
    /// Merging is **append-or-drop and never reclaims dead slots**, unlike the
    /// single-structure [`Self::intern_tag`]. This matters when the target table
    /// is grown across several `build_tag_remap` calls before any atom is added —
    /// exactly what `apply_diff` (base→result then diff→result) and
    /// `compose_two_diffs` (diff1→composed then diff2→composed) do. There, every
    /// name committed by an earlier call is a real union member with no atom
    /// carrying it *yet*; reclamation would see it as a "dead" slot and silently
    /// overwrite it (corrupting the earlier remap) instead of dropping the
    /// overflow name. So the 32-name budget here counts every distinct name,
    /// including any the target already holds unused.
    pub fn build_tag_remap(&mut self, source: &AtomicStructure) -> (TagRemap, Vec<String>) {
        // Fast path: identical tables → identity remap, no interning.
        if self.tag_names == source.tag_names {
            return (TagRemap::identity(), Vec::new());
        }
        let live = source.live_tag_mask();
        let mut map: [Option<u8>; MAX_TAGS] = [None; MAX_TAGS];
        let mut dropped: Vec<String> = Vec::new();
        for (i, slot) in map.iter_mut().enumerate() {
            if live & (1u32 << i) == 0 {
                continue; // dead (or nonexistent) source slot — nothing to move
            }
            let Some(name) = source.tag_names.get(i) else {
                continue;
            };
            if let Some(existing) = self.tag_names.iter().position(|n| n == name) {
                *slot = Some(existing as u8); // already committed → reuse its slot
            } else if self.tag_names.len() < MAX_TAGS {
                let idx = self.tag_names.len() as u8;
                self.tag_names.push(name.clone());
                *slot = Some(idx);
            } else {
                dropped.push(name.clone()); // no room, no reclamation → drop
            }
        }
        (TagRemap { map }, dropped)
    }

    /// Translate a mask through a [`TagRemap`]; unmapped source bits are dropped
    /// (their names are the ones reported by [`Self::build_tag_remap`]).
    pub fn remap_tag_bits(bits: u32, remap: &TagRemap) -> u32 {
        let mut out = 0u32;
        for i in 0..MAX_TAGS {
            if bits & (1u32 << i) != 0 {
                if let Some(target) = remap.map[i] {
                    out |= 1u32 << target;
                }
            }
        }
        out
    }

    /// Copies all per-atom metadata (flags except selected, and in_crystal_depth)
    /// from a source atom to a target atom in this structure.
    ///
    /// The `selected` flag is intentionally NOT copied because it is transient UI
    /// state that should not persist through diff application or structure merging.
    ///
    /// This is the canonical way to preserve metadata when constructing result atoms
    /// via `add_atom()` (which initializes all metadata to zero). Using this method
    /// instead of copying individual flags ensures that newly added metadata fields
    /// are automatically preserved without requiring updates at every call site.
    ///
    /// Tags are **per-structure** bit indices, so `tag_bits` must be translated
    /// through `tag_remap` (source structure → this structure) — the signature
    /// forces every call site to say which translation applies, so a raw
    /// cross-table bit copy cannot happen by omission (see `doc/design_atom_tags.md`).
    /// Pass [`TagRemap::identity`] for same-structure copies.
    pub(crate) fn copy_atom_metadata(
        &mut self,
        target_id: u32,
        source: &Atom,
        tag_remap: &TagRemap,
    ) {
        let remapped_tags = Self::remap_tag_bits(source.tag_bits, tag_remap);
        if let Some(target) = self.get_atom_mut(target_id) {
            // Copy all flags except selected (bit 0)
            target.flags = source.flags & !0x1;
            target.in_crystal_depth = source.in_crystal_depth;
            target.tag_bits = remapped_tags;
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
            atoms: Vec::new(),
            num_atoms: 0,
            num_bonds: 0,
            grid: FxHashMap::default(),
            decorator: AtomicStructureDecorator::new(),
            is_diff: false,
            anchor_positions: FxHashMap::default(),
            effective_atomic_numbers: FxHashMap::default(),
            tag_names: Vec::new(),
        }
    }

    /// Creates an empty structure marked as a diff
    pub fn new_diff() -> Self {
        Self {
            atoms: Vec::new(),
            num_atoms: 0,
            num_bonds: 0,
            grid: FxHashMap::default(),
            decorator: AtomicStructureDecorator::new(),
            is_diff: true,
            anchor_positions: FxHashMap::default(),
            effective_atomic_numbers: FxHashMap::default(),
            tag_names: Vec::new(),
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

    pub fn clear_all_bonds(&mut self) {
        for atom in self.atoms.iter_mut().flatten() {
            atom.bonds.clear();
        }
        self.num_bonds = 0;
    }

    pub fn get_num_of_bonds(&self) -> usize {
        self.num_bonds
    }

    /// Returns the total number of atom slots (including deleted/empty slots).
    /// Used for serialization ID restoration.
    pub fn get_num_of_atoms_including_deleted(&self) -> usize {
        self.atoms.len()
    }

    /// Adds an empty padding slot (None) to the atom vector.
    /// Used during deserialization to restore exact atom IDs when there are gaps.
    pub fn add_padding_slot(&mut self) {
        self.atoms.push(None);
    }

    /// Add an atom with a specific ID. Used by undo/redo to restore atoms
    /// at their original IDs. Panics if the slot is already occupied.
    /// Extends the atoms Vec with None padding if needed.
    pub fn add_atom_with_id(&mut self, id: u32, atomic_number: i16, position: DVec3) -> u32 {
        let index = (id - 1) as usize;
        // Extend with None padding if needed
        while self.atoms.len() <= index {
            self.atoms.push(None);
        }
        assert!(self.atoms[index].is_none(), "Slot {} already occupied", id);
        let atom = Atom {
            id,
            atomic_number,
            position,
            bonds: SmallVec::new(),
            flags: 0,
            in_crystal_depth: 0.0,
            tag_bits: 0,
        };
        self.atoms[index] = Some(atom);
        self.num_atoms += 1;
        self.add_atom_to_grid(id, &position);
        id
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
            tag_bits: 0, // No tags on a freshly created atom
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

    /// Sets the display alpha for an atom. Values below 0.0 clamp to 0.0;
    /// values at or above 1.0 remove the entry (fully opaque).
    pub fn set_atom_alpha(&mut self, atom_id: u32, alpha: f32) {
        if alpha >= 1.0 {
            self.decorator.atom_alpha.remove(&atom_id);
        } else {
            self.decorator.atom_alpha.insert(atom_id, alpha.max(0.0));
        }
    }

    /// Returns the display alpha for an atom; atoms without an entry are
    /// fully opaque (1.0).
    pub fn get_atom_alpha(&self, atom_id: u32) -> f32 {
        self.decorator
            .atom_alpha
            .get(&atom_id)
            .copied()
            .unwrap_or(1.0)
    }

    /// Sets the albedo override for an atom. Components are clamped to `[0,1]`
    /// (the `AtomInfo.color` convention). See `doc/design_style_rules.md`.
    pub fn set_atom_color(&mut self, atom_id: u32, color: Vec3) {
        self.decorator
            .atom_color
            .insert(atom_id, color.clamp(Vec3::ZERO, Vec3::ONE));
    }

    /// Removes an atom's albedo override, restoring the element-derived color.
    pub fn clear_atom_color(&mut self, atom_id: u32) {
        self.decorator.atom_color.remove(&atom_id);
    }

    /// Returns the albedo override for an atom; `None` = use the element color.
    pub fn get_atom_color(&self, atom_id: u32) -> Option<Vec3> {
        self.decorator.atom_color.get(&atom_id).copied()
    }

    /// Sets the render-style override for an atom. See `doc/design_style_rules.md`.
    pub fn set_atom_render_style(&mut self, atom_id: u32, style: AtomRenderStyle) {
        self.decorator.atom_render_style.insert(atom_id, style);
    }

    /// Removes an atom's render-style override, restoring the global preference.
    pub fn clear_atom_render_style(&mut self, atom_id: u32) {
        self.decorator.atom_render_style.remove(&atom_id);
    }

    /// Returns the render-style override for an atom; `None` = follow the global
    /// visualization preference.
    pub fn get_atom_render_style(&self, atom_id: u32) -> Option<AtomRenderStyle> {
        self.decorator.atom_render_style.get(&atom_id).copied()
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
        self.decorator.atom_alpha.remove(&id);
        self.decorator.atom_color.remove(&id);
        self.decorator.atom_render_style.remove(&id);
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
        self.decorator.atom_alpha.remove(&id);
        self.decorator.atom_color.remove(&id);
        self.decorator.atom_render_style.remove(&id);
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
            if let Some(atom) = self.get_atom_mut(atom_id1)
                && let Some(bond) = atom
                    .bonds
                    .iter_mut()
                    .find(|b| b.other_atom_id() == atom_id2)
            {
                bond.set_bond_order(bond_order);
            }
            if let Some(atom) = self.get_atom_mut(atom_id2)
                && let Some(bond) = atom
                    .bonds
                    .iter_mut()
                    .find(|b| b.other_atom_id() == atom_id1)
            {
                bond.set_bond_order(bond_order);
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
        // Only adjust the unique-bond count if the bond actually exists. Callers
        // (e.g. atom_edit's diff-view delete) may request deletion of a bond
        // whose endpoint atoms were already removed in the same operation, in
        // which case there is nothing to remove. Decrementing unconditionally
        // underflows `num_bonds` (a `usize`), which later blows up a Vec
        // allocation ("capacity overflow") when the tessellator sizes its bond
        // buffers from `get_num_of_bonds()`. See issue #385.
        let bond_existed = self.has_bond_between(bond_ref.atom_id1, bond_ref.atom_id2);

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

        if bond_existed {
            self.num_bonds -= 1; // Decrement unique bond count
        }

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

        // Fast path preserving the legacy no-override behavior exactly: a
        // space-filling scene with no per-atom render-style overrides has no
        // pickable bonds, so skip the bond loop entirely (§Picking,
        // `doc/design_style_rules.md`).
        if *visualization != AtomicStructureVisualization::BallAndStick
            && self.decorator.atom_render_style.is_empty()
        {
            return match closest_hit {
                Some((hit_result, _)) => hit_result,
                None => HitTestResult::None,
            };
        }

        // A bond is pickable iff at least one endpoint's effective mode is
        // ball-and-stick (Decision 1's first clause — *not* its overstretched
        // clause, so overstretched space-filling–space-filling bonds stay
        // rendered-but-unpickable). The effective mode is the atom's decorator
        // render-style override, else the global preference.
        let atom_is_ball_and_stick = |atom: &Atom| -> bool {
            match self.get_atom_render_style(atom.id) {
                Some(AtomRenderStyle::BallAndStick) => true,
                Some(AtomRenderStyle::SpaceFilling) => false,
                None => *visualization == AtomicStructureVisualization::BallAndStick,
            }
        };

        for atom in self.atoms_values() {
            for bond in &atom.bonds {
                let other_atom_id = bond.other_atom_id();

                if atom.id >= other_atom_id {
                    continue;
                }

                let Some(other_atom) = self.get_atom(other_atom_id) else {
                    continue;
                };

                if !atom_is_ball_and_stick(atom) && !atom_is_ball_and_stick(other_atom) {
                    continue;
                }

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

    /// Sets the atomic number of an atom.
    ///
    /// # Returns
    ///
    /// `true` if the atom was found and updated, `false` otherwise
    pub fn set_atomic_number(&mut self, atom_id: u32, atomic_number: i16) -> bool {
        if let Some(atom) = self.get_atom_mut(atom_id) {
            atom.atomic_number = atomic_number;
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

        // Also transform anchor positions (for diff structures).
        // When anchor_positions is empty (non-diff structures) this is a no-op.
        for pos in self.anchor_positions.values_mut() {
            *pos = rotation.mul_vec3(*pos) + *translation;
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

    /// Merges another structure into this one with remapped IDs.
    ///
    /// Fallible because tags merge at the **name** level: `other`'s live tag
    /// names are interned into `self` (see [`Self::build_tag_remap`]) and each
    /// incoming atom's mask is translated through the resulting [`TagRemap`].
    /// If the combined table would exceed [`MAX_TAGS`] live names the whole
    /// merge fails with `TagError::LimitReached` rather than silently dropping
    /// tags (§Maintenance in `doc/design_atom_tags.md`).
    pub fn add_atomic_structure(
        &mut self,
        other: &AtomicStructure,
    ) -> Result<FxHashMap<u32, u32>, TagError> {
        // Name-level tag union, computed once for the whole merge. A non-empty
        // dropped list means the combined table overflowed — fail the call.
        let (tag_remap, dropped) = self.build_tag_remap(other);
        if !dropped.is_empty() {
            return Err(TagError::LimitReached);
        }

        let mut atom_id_map: FxHashMap<u32, u32> = FxHashMap::default();

        for (old_atom_id, atom) in other.iter_atoms() {
            let new_atom_id = self.add_atom(atom.atomic_number, atom.position);
            atom_id_map.insert(*old_atom_id, new_atom_id);

            self.set_atom_depth(new_atom_id, atom.in_crystal_depth);

            // Copy all flags at once (selected, hydrogen_passivation, and any
            // future flags), and translate the tag mask through the name-level
            // remap so bit positions follow `self`'s table, not `other`'s.
            let remapped_tags = Self::remap_tag_bits(atom.tag_bits, &tag_remap);
            if let Some(new_atom) = self.get_atom_mut(new_atom_id) {
                new_atom.flags = atom.flags;
                new_atom.tag_bits = remapped_tags;
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

        // Merge anchor positions with remapped IDs
        for (&old_atom_id, &anchor_pos) in &other.anchor_positions {
            if let Some(&new_atom_id) = atom_id_map.get(&old_atom_id) {
                self.anchor_positions.insert(new_atom_id, anchor_pos);
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

        // Merge per-atom display alphas with remapped IDs
        for (&old_atom_id, &alpha) in &other.decorator.atom_alpha {
            if let Some(&new_atom_id) = atom_id_map.get(&old_atom_id) {
                self.decorator.atom_alpha.insert(new_atom_id, alpha);
            }
        }

        // Merge per-atom color overrides with remapped IDs
        for (&old_atom_id, &color) in &other.decorator.atom_color {
            if let Some(&new_atom_id) = atom_id_map.get(&old_atom_id) {
                self.decorator.atom_color.insert(new_atom_id, color);
            }
        }

        // Merge per-atom render-style overrides with remapped IDs
        for (&old_atom_id, &style) in &other.decorator.atom_render_style {
            if let Some(&new_atom_id) = atom_id_map.get(&old_atom_id) {
                self.decorator.atom_render_style.insert(new_atom_id, style);
            }
        }

        Ok(atom_id_map)
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
        let anchor_size = self.anchor_positions.len()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<DVec3>());
        let effective_z_size = self.effective_atomic_numbers.len()
            * (std::mem::size_of::<i16>() + std::mem::size_of::<i16>());

        base_size + atoms_size + grid_size + decorator_size + anchor_size + effective_z_size
    }
}

/// Formats a float for snapshot output, collapsing a value that rounds to zero
/// at `decimals` precision to *positive* zero.
///
/// Without this, an essentially-zero quantity (a surface atom's
/// `in_crystal_depth`, a lattice position on an axis) can land on the negative
/// side of zero — `-0.0`, or a tiny negative like `-1e-16` produced by a
/// different floating-point reduction order across machines/thread counts — and
/// render as `-0.000`. The sign carries no meaning at display precision, but it
/// makes the snapshot nondeterministic. Rounding first, then adding `0.0`,
/// normalizes IEEE-754 negative zero away.
fn fmt_snapshot_float(v: f64, decimals: usize) -> String {
    let scale = 10f64.powi(decimals as i32);
    let rounded = (v * scale).round() / scale;
    // `if rounded == 0.0 { 0.0 }` maps both `-0.0` and `0.0` to `+0.0`
    // (they compare equal); any non-zero value passes through unchanged.
    let normalized = if rounded == 0.0 { 0.0 } else { rounded };
    format!("{:.*}", decimals, normalized)
}

impl AtomicStructure {
    /// Returns a detailed string representation for snapshot testing.
    /// Includes frame transform, counts, and details of first 10 atoms and bonds.
    pub fn to_detailed_string(&self) -> String {
        let mut lines = Vec::new();

        if self.is_diff {
            lines.push("is_diff: true".to_string());
            if !self.anchor_positions.is_empty() {
                lines.push(format!("anchor_positions: {}", self.anchor_positions.len()));
            }
        }
        lines.push(format!("atoms: {}", self.num_atoms));
        lines.push(format!("bonds: {}", self.num_bonds));

        // First 10 atoms
        let atoms: Vec<&Atom> = self.atoms_values().take(10).collect();
        if !atoms.is_empty() {
            lines.push(format!("first {} atoms:", atoms.len()));
            for atom in &atoms {
                lines.push(format!(
                    "  [{}] Z={} pos=({}, {}, {}) depth={} bonds={}",
                    atom.id,
                    atom.atomic_number,
                    fmt_snapshot_float(atom.position.x, 6),
                    fmt_snapshot_float(atom.position.y, 6),
                    fmt_snapshot_float(atom.position.z, 6),
                    fmt_snapshot_float(atom.in_crystal_depth as f64, 3),
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
